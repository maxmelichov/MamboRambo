mod chunking;
pub mod handling;
pub mod phonemize;
pub mod style;
mod text;

use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use ndarray::{Array, Array1, Array3};
use ort::{session::Session, value::Tensor};
use rand_distr::{Distribution, StandardNormal};
use serde_json::Value;

pub use crate::chunking::ChunkingOptions;
use crate::{chunking::append_silence, text::Tokenizer};

const SAMPLE_RATE: usize = 44_100;
const BASE_CHUNK_SIZE: usize = 512;
const CHUNK_COMPRESS_FACTOR: usize = 6;
const LATENT_DIM: usize = 24;
const COMPRESSED_CHANNELS: usize = LATENT_DIM * CHUNK_COMPRESS_FACTOR;

pub struct SynthesisOptions {
    pub lang: String,
    pub total_step: usize,
    pub cfg_scale: f32,
    pub speed: f32,
    pub chunking: Option<ChunkingOptions>,
}

impl Default for SynthesisOptions {
    fn default() -> Self {
        Self {
            lang: "he".to_string(),
            total_step: 5,
            cfg_scale: 4.0,
            speed: 1.0,
            chunking: None,
        }
    }
}

pub struct VoiceStyle {
    ttl: Array3<f32>,
    dp: Array3<f32>,
}

impl VoiceStyle {
    pub fn new(ttl: Array3<f32>, dp: Array3<f32>) -> Self {
        Self { ttl, dp }
    }

    pub fn from_json(path: impl AsRef<Path>) -> Result<Self> {
        let raw = fs::read_to_string(path.as_ref())
            .with_context(|| format!("read voice style {}", path.as_ref().display()))?;
        Self::from_json_str(&raw)
    }

    pub fn from_json_str(raw: &str) -> Result<Self> {
        let json: Value = serde_json::from_str(raw)?;
        Ok(Self {
            ttl: read_style_tensor(&json["style_ttl"])?,
            dp: read_style_tensor(&json["style_dp"])?,
        })
    }

    pub fn from_json_bytes(raw: &[u8]) -> Result<Self> {
        Self::from_json_str(std::str::from_utf8(raw)?)
    }
}

pub struct BlueTts {
    dp: Session,
    text_encoder: Session,
    vector_estimator: Session,
    vocoder: Session,
    tokenizer: Tokenizer,
}

impl BlueTts {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        Ok(Self {
            dp: load_session(dir.join("duration_predictor.onnx"))?,
            text_encoder: load_session(dir.join("text_encoder.onnx"))?,
            vector_estimator: load_session(dir.join("vector_estimator.onnx"))?,
            vocoder: load_session(dir.join("vocoder.onnx"))?,
            tokenizer: Tokenizer::from_json(dir.join("vocab.json"))?,
        })
    }

    pub fn from_model_bytes(models: BlueTtsModelBytes<'_>) -> Result<Self> {
        Ok(Self {
            dp: load_session_from_memory(models.duration_predictor)?,
            text_encoder: load_session_from_memory(models.text_encoder)?,
            vector_estimator: load_session_from_memory(models.vector_estimator)?,
            vocoder: load_session_from_memory(models.vocoder)?,
            tokenizer: Tokenizer::from_json_bytes(models.vocab)?,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        SAMPLE_RATE as u32
    }

    pub fn create(
        &mut self,
        phonemes: &str,
        style: &VoiceStyle,
        opts: SynthesisOptions,
    ) -> Result<Vec<f32>> {
        if let Some(chunking) = &opts.chunking {
            if chunking.enabled {
                let chunks = chunking::split_phonemes(phonemes, chunking.max_chars);
                let mut audio = Vec::new();
                let last_idx = chunks.len().saturating_sub(1);
                for (idx, chunk) in chunks.iter().enumerate() {
                    audio.extend(self.synthesize_chunk(chunk, style, &opts)?);
                    if idx != last_idx {
                        append_silence(&mut audio, self.sample_rate(), chunking.silence_seconds);
                    }
                }
                return Ok(audio);
            }
        }
        self.synthesize_chunk(phonemes, style, &opts)
    }

    fn synthesize_chunk(
        &mut self,
        phonemes: &str,
        style: &VoiceStyle,
        opts: &SynthesisOptions,
    ) -> Result<Vec<f32>> {
        let (text_ids, text_mask) = self.tokenizer.encode_batch(&[phonemes], &[&opts.lang])?;

        let dur = self.dp.run(ort::inputs! {
            "text_ids" => Tensor::from_array(text_ids.clone())?,
            "style_dp" => Tensor::from_array(style.dp.clone())?,
            "text_mask" => Tensor::from_array(text_mask.clone())?,
        })?;
        let duration = output_vec_f32(&dur[0])?
            .first()
            .copied()
            .context("duration output was empty")?
            / opts.speed.max(1e-6);

        let text_emb = self.text_encoder.run(ort::inputs! {
            "text_ids" => Tensor::from_array(text_ids)?,
            "style_ttl" => Tensor::from_array(style.ttl.clone())?,
            "text_mask" => Tensor::from_array(text_mask.clone())?,
        })?;
        let text_emb = output_array3(&text_emb[0])?;

        let (mut xt, latent_mask) = sample_noisy_latent(duration);
        let total_step = Array1::from_vec(vec![opts.total_step as f32]);
        let cfg_scale = Array1::from_vec(vec![opts.cfg_scale]);

        for step in 0..opts.total_step {
            let current_step = Array1::from_vec(vec![step as f32]);
            let out = self.vector_estimator.run(ort::inputs! {
                "noisy_latent" => Tensor::from_array(xt)?,
                "text_emb" => Tensor::from_array(text_emb.clone())?,
                "style_ttl" => Tensor::from_array(style.ttl.clone())?,
                "latent_mask" => Tensor::from_array(latent_mask.clone())?,
                "text_mask" => Tensor::from_array(text_mask.clone())?,
                "current_step" => Tensor::from_array(current_step)?,
                "total_step" => Tensor::from_array(total_step.clone())?,
                "cfg_scale" => Tensor::from_array(cfg_scale.clone())?,
            })?;
            xt = output_array3(&out[0])?;
        }

        let wav = self.vocoder.run(ort::inputs! {
            "latent" => Tensor::from_array(xt)?,
        })?;
        let wav = output_array3(&wav[0])?;
        let mut audio: Vec<f32> = wav.iter().copied().collect();
        let trim = BASE_CHUNK_SIZE * CHUNK_COMPRESS_FACTOR;
        if audio.len() > 2 * trim {
            audio = audio[trim..audio.len() - trim].to_vec();
        }
        Ok(audio)
    }
}

pub struct BlueTtsModelBytes<'a> {
    pub duration_predictor: &'a [u8],
    pub text_encoder: &'a [u8],
    pub vector_estimator: &'a [u8],
    pub vocoder: &'a [u8],
    pub vocab: &'a [u8],
}

fn load_session(path: impl AsRef<Path>) -> Result<Session> {
    let path = path.as_ref();
    Session::builder()
        .map_err(|e| anyhow!("{e}"))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("{e}"))?
        .with_intra_threads(8)
        .map_err(|e| anyhow!("{e}"))?
        .with_inter_threads(1)
        .map_err(|e| anyhow!("{e}"))?
        .commit_from_file(path)
        .map_err(|e| anyhow!("{e}"))
        .with_context(|| format!("load ONNX session {}", path.display()))
}

pub(crate) fn load_onnx_session(path: impl AsRef<Path>) -> Result<Session> {
    load_session(path)
}

pub(crate) fn output_array3(value: &ort::value::DynValue) -> Result<Array3<f32>> {
    let (shape, data) = value.try_extract_tensor::<f32>()?;
    let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
    match dims.as_slice() {
        [a, b, c] => Ok(Array3::from_shape_vec((*a, *b, *c), data.to_vec())?),
        [a, b] => Ok(Array3::from_shape_vec((*a, 1, *b), data.to_vec())?),
        _ => bail!("expected rank-2/rank-3 f32 tensor, got shape {shape}"),
    }
}

fn load_session_from_memory(bytes: &[u8]) -> Result<Session> {
    let builder = Session::builder()
        .map_err(|e| anyhow!("{e}"))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("{e}"))?
        .with_intra_threads(8)
        .map_err(|e| anyhow!("{e}"))?
        .with_inter_threads(1)
        .map_err(|e| anyhow!("{e}"))?;
    builder
        .commit_from_memory(bytes)
        .map_err(|e| anyhow!("{e}"))
}

fn sample_noisy_latent(duration: f32) -> (Array3<f32>, Array3<f32>) {
    let wav_len = (duration * SAMPLE_RATE as f32).max(1.0);
    let chunk = BASE_CHUNK_SIZE * CHUNK_COMPRESS_FACTOR;
    let latent_len = ((wav_len + chunk as f32 - 1.0) / chunk as f32)
        .floor()
        .max(1.0) as usize;

    let mut rng = rand::rng();
    let normal = StandardNormal;
    let xt = Array::from_shape_fn((1, COMPRESSED_CHANNELS, latent_len), |_| {
        normal.sample(&mut rng)
    });
    let mask = Array3::from_elem((1, 1, latent_len), 1.0);
    (xt, mask)
}

fn output_vec_f32(value: &ort::value::DynValue) -> Result<Vec<f32>> {
    let (_, data) = value.try_extract_tensor::<f32>()?;
    Ok(data.to_vec())
}

fn read_style_tensor(value: &Value) -> Result<Array3<f32>> {
    let dims = value["dims"].as_array().context("style dims missing")?;
    let shape = [
        dims[0].as_u64().context("bad style dim 0")? as usize,
        dims[1].as_u64().context("bad style dim 1")? as usize,
        dims[2].as_u64().context("bad style dim 2")? as usize,
    ];
    let data = flatten_f32(&value["data"]);
    Ok(Array3::from_shape_vec(shape, data)?)
}

fn flatten_f32(value: &Value) -> Vec<f32> {
    match value {
        Value::Array(items) => items.iter().flat_map(flatten_f32).collect(),
        Value::Number(n) => vec![n.as_f64().unwrap_or(0.0) as f32],
        _ => Vec::new(),
    }
}
