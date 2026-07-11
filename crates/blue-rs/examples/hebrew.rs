use anyhow::{Context, Result};
use blue_rs::{BlueTts, ChunkingOptions, SynthesisOptions, VoiceStyle, phonemize::Phonemizer};

fn main() -> Result<()> {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "שלום עולם. אני אוהב machine learning.".to_string());
    let renikud_model = std::env::var("RENIKUD_MODEL")
        .ok()
        .or_else(|| {
            ["renikud.onnx", "model.onnx"]
                .into_iter()
                .find(|path| std::path::Path::new(path).exists())
                .map(str::to_string)
        })
        .context("Set RENIKUD_MODEL=/path/to/renikud.onnx for Hebrew phonemization")?;

    let mut phonemizer = Phonemizer::new(Some(renikud_model))?;
    let phonemes = phonemizer.phonemize(&text)?;
    eprintln!("Phonemes: {phonemes}");

    let mut tts = BlueTts::from_dir("onnx_models")?;
    let style = VoiceStyle::from_json("voices/female1.json")?;
    let audio = tts.create(
        &phonemes,
        &style,
        SynthesisOptions {
            lang: "he".to_string(),
            total_step: 5,
            cfg_scale: 4.0,
            speed: 1.0,
            chunking: Some(ChunkingOptions {
                enabled: true,
                silence_seconds: 0.2,
                max_chars: None,
            }),
        },
    )?;

    std::fs::create_dir_all("examples/out")?;
    let out = "examples/out/hebrew-rs.wav";
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: tts.sample_rate(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(out, spec)?;
    for sample in audio {
        writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;

    println!("Saved {out}");
    Ok(())
}
