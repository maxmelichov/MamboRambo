use std::{f32::consts::PI, fs, path::Path};

use anyhow::{Context, Result, bail};
use ndarray::{Array2, Array3};
use ort::{session::Session, value::Tensor};
use rustfft::{FftPlanner, num_complex::Complex32};
use serde_json::Value;

use crate::{VoiceStyle, load_onnx_session, output_array3};

#[derive(Clone, Copy)]
struct AudioConfig {
    sample_rate: u32,
    n_fft: usize,
    win_length: usize,
    hop_length: usize,
    n_mels: usize,
    chunk_compress_factor: usize,
}

pub struct VoiceStyleExtractor {
    codec_encoder: Session,
    style_encoder: Session,
    duration_style_encoder: Session,
    cfg: AudioConfig,
}

impl VoiceStyleExtractor {
    pub fn from_dir(onnx_dir: impl AsRef<Path>) -> Result<Self> {
        let onnx_dir = onnx_dir.as_ref();
        Self::new(
            onnx_dir.join("codec_encoder.onnx"),
            onnx_dir.join("style_encoder.onnx"),
            onnx_dir.join("duration_style_encoder.onnx"),
            onnx_dir.join("tts.json"),
        )
    }

    pub fn new(
        codec_encoder: impl AsRef<Path>,
        style_encoder: impl AsRef<Path>,
        duration_style_encoder: impl AsRef<Path>,
        config: impl AsRef<Path>,
    ) -> Result<Self> {
        Ok(Self {
            codec_encoder: load_onnx_session(codec_encoder)?,
            style_encoder: load_onnx_session(style_encoder)?,
            duration_style_encoder: load_onnx_session(duration_style_encoder)?,
            cfg: load_audio_config(config)?,
        })
    }

    pub fn from_wav(&mut self, wav_path: impl AsRef<Path>) -> Result<VoiceStyle> {
        let wav = read_wav_mono(wav_path.as_ref(), self.cfg.sample_rate)?;
        let mut mel = linear_mel_features(&wav, self.cfg);
        let frames =
            (mel.shape()[2] / self.cfg.chunk_compress_factor) * self.cfg.chunk_compress_factor;
        if frames < mel.shape()[2] {
            mel = mel.slice_move(ndarray::s![.., .., ..frames]);
        }

        let z_ref = self.codec_encoder.run(ort::inputs! {
            "mel" => Tensor::from_array(mel)?,
        })?;
        let z_ref = trim_reference_latents(output_array3(&z_ref[0])?);
        let ref_mask = Array3::from_elem((z_ref.shape()[0], 1, z_ref.shape()[2]), 1.0f32);

        let style_ttl = self.style_encoder.run(ort::inputs! {
            "z_ref" => Tensor::from_array(z_ref.clone())?,
            "ref_mask" => Tensor::from_array(ref_mask.clone())?,
        })?;
        let style_dp = self.duration_style_encoder.run(ort::inputs! {
            "z_ref" => Tensor::from_array(z_ref)?,
            "ref_mask" => Tensor::from_array(ref_mask)?,
        })?;

        Ok(VoiceStyle::new(
            output_array3(&style_ttl[0])?,
            output_array3(&style_dp[0])?,
        ))
    }
}

fn load_audio_config(path: impl AsRef<Path>) -> Result<AudioConfig> {
    let raw = fs::read_to_string(path.as_ref())
        .with_context(|| format!("read config {}", path.as_ref().display()))?;
    let json: Value = serde_json::from_str(&raw)?;
    let ae = &json["ae"];
    let ttl = &json["ttl"];
    let spec = &ae["encoder"]["spec_processor"];
    Ok(AudioConfig {
        sample_rate: ae["sample_rate"].as_u64().unwrap_or(44_100) as u32,
        n_fft: spec["n_fft"].as_u64().unwrap_or(2048) as usize,
        win_length: spec["win_length"]
            .as_u64()
            .or_else(|| spec["n_fft"].as_u64())
            .unwrap_or(2048) as usize,
        hop_length: spec["hop_length"].as_u64().unwrap_or(512) as usize,
        n_mels: spec["n_mels"].as_u64().unwrap_or(1253) as usize,
        chunk_compress_factor: ttl["chunk_compress_factor"].as_u64().unwrap_or(6) as usize,
    })
}

fn read_wav_mono(path: &Path, target_sr: u32) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("read reference wav {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let mut interleaved = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                interleaved.push(sample?);
            }
        }
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                for sample in reader.samples::<i16>() {
                    interleaved.push(sample? as f32 / i16::MAX as f32);
                }
            } else {
                let scale = ((1_i64 << (spec.bits_per_sample - 1)) - 1) as f32;
                for sample in reader.samples::<i32>() {
                    interleaved.push(sample? as f32 / scale);
                }
            }
        }
    }

    let mut mono = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        mono.push(frame.iter().sum::<f32>() / frame.len() as f32);
    }
    if spec.sample_rate == target_sr {
        return Ok(mono);
    }
    Ok(resample_linear(&mono, spec.sample_rate, target_sr))
}

fn resample_linear(input: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
    if input.is_empty() || from_sr == to_sr {
        return input.to_vec();
    }
    let out_len = ((input.len() as f64) * (to_sr as f64) / (from_sr as f64))
        .round()
        .max(1.0) as usize;
    let ratio = from_sr as f64 / to_sr as f64;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let left = pos.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let frac = (pos - left as f64) as f32;
            input[left] * (1.0 - frac) + input[right] * frac
        })
        .collect()
}

fn linear_mel_features(wav: &[f32], cfg: AudioConfig) -> Array3<f32> {
    let spec = stft_magnitude(wav, cfg.n_fft, cfg.win_length, cfg.hop_length);
    let mel_basis = mel_filterbank(cfg.sample_rate, cfg.n_fft, cfg.n_mels);
    let mel = mel_basis.dot(&spec);
    let n_freq = cfg.n_fft / 2 + 1;
    let frames = spec.shape()[1];
    let mut out = Array3::<f32>::zeros((1, n_freq + cfg.n_mels, frames));
    for f in 0..n_freq {
        for t in 0..frames {
            out[[0, f, t]] = spec[[f, t]].max(1e-5).ln();
        }
    }
    for m in 0..cfg.n_mels {
        for t in 0..frames {
            out[[0, n_freq + m, t]] = mel[[m, t]].max(1e-5).ln();
        }
    }
    out
}

fn stft_magnitude(wav: &[f32], n_fft: usize, win_length: usize, hop_length: usize) -> Array2<f32> {
    let pad = n_fft / 2;
    let padded = reflect_pad(wav, pad);
    let frames = if padded.len() < n_fft {
        1
    } else {
        (padded.len() - n_fft) / hop_length + 1
    };
    let n_freq = n_fft / 2 + 1;
    let window = hann_window(win_length);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);
    let mut out = Array2::<f32>::zeros((n_freq, frames));
    let mut buffer = vec![Complex32::new(0.0, 0.0); n_fft];

    for frame in 0..frames {
        buffer.fill(Complex32::new(0.0, 0.0));
        let start = frame * hop_length;
        let win_offset = (n_fft - win_length) / 2;
        for i in 0..win_length {
            buffer[win_offset + i].re = padded[start + win_offset + i] * window[i];
        }
        fft.process(&mut buffer);
        for f in 0..n_freq {
            out[[f, frame]] = buffer[f].norm();
        }
    }
    out
}

fn reflect_pad(wav: &[f32], pad: usize) -> Vec<f32> {
    if wav.is_empty() {
        return vec![0.0; pad * 2];
    }
    let mut out = Vec::with_capacity(wav.len() + pad * 2);
    for i in (1..=pad).rev() {
        out.push(wav[i.min(wav.len() - 1)]);
    }
    out.extend_from_slice(wav);
    for i in 0..pad {
        let idx = wav.len().saturating_sub(2 + i);
        out.push(wav[idx]);
    }
    out
}

fn hann_window(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| 0.5 - 0.5 * ((2.0 * PI * i as f32) / len as f32).cos())
        .collect()
}

fn mel_filterbank(sr: u32, n_fft: usize, n_mels: usize) -> Array2<f32> {
    let n_freq = n_fft / 2 + 1;
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sr as f32 / 2.0);
    let mel_points: Vec<f32> = (0..n_mels + 2)
        .map(|i| min_mel + (max_mel - min_mel) * i as f32 / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.into_iter().map(mel_to_hz).collect();
    let bins: Vec<f32> = hz_points
        .into_iter()
        .map(|hz| (n_fft as f32 + 1.0) * hz / sr as f32)
        .collect();

    let mut fb = Array2::<f32>::zeros((n_mels, n_freq));
    for m in 0..n_mels {
        let left = bins[m];
        let center = bins[m + 1];
        let right = bins[m + 2];
        if center <= left || right <= center {
            continue;
        }
        for k in 0..n_freq {
            let x = k as f32;
            let weight = if x >= left && x <= center {
                (x - left) / (center - left)
            } else if x > center && x <= right {
                (right - x) / (right - center)
            } else {
                0.0
            };
            fb[[m, k]] = weight.max(0.0);
        }
    }
    fb
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10_f32.powf(mel / 2595.0) - 1.0)
}

fn trim_reference_latents(z: Array3<f32>) -> Array3<f32> {
    let t = z.shape()[2];
    let tail = 2.max((t as f32 * 0.05) as usize);
    let t_trim = 1.max(t.saturating_sub(tail));
    let capped = t_trim.min(150);
    z.slice_move(ndarray::s![.., .., ..capped])
}

#[allow(dead_code)]
fn ensure_supported_config(cfg: AudioConfig) -> Result<()> {
    if cfg.n_fft == 0 || cfg.win_length == 0 || cfg.hop_length == 0 {
        bail!("invalid audio feature config")
    }
    Ok(())
}
