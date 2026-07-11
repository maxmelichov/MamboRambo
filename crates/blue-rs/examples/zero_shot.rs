use anyhow::Result;
use blue_rs::{
    BlueTts, ChunkingOptions, SynthesisOptions,
    phonemize::{Language, Phonemizer},
    style::VoiceStyleExtractor,
};

fn main() -> Result<()> {
    let mut style_extractor = VoiceStyleExtractor::from_dir("onnx_models")?;
    let style = style_extractor.from_wav("ref.wav")?;

    let mut phonemizer = Phonemizer::with_language(Some("renikud.onnx"), Language::Hebrew)?;
    let phonemes = phonemizer
        .phonemize("שימו לב נוסעים יקרים, הרכבת תיכנס לתחנת תל אביב מרכז בעוד מספר דקות.")?;

    let mut tts = BlueTts::from_dir("onnx_models")?;
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
    let out = "examples/out/zero-shot-rs.wav";
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
