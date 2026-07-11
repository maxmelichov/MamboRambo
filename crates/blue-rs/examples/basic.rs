use anyhow::Result;
use blue_rs::{BlueTts, ChunkingOptions, SynthesisOptions, VoiceStyle};

fn main() -> Result<()> {
    let mut tts = BlueTts::from_dir("onnx_models")?;
    let style = VoiceStyle::from_json("voices/female1.json")?;

    let phonemes = "sˈimu lˈev nosʔˈim jekaʁˈim, haʁakˈevet tiχanˈes letaχanˈat tˈel ʔavˈiv meʁkˈaz beʔˈod mispˈaʁ dakˈot. ɐtˈɛnʃən dˈɪɹ pˈæsɪndʒɚz, ðə tɹˈeɪn wˈɪl ˈɛntɚ tˈɛl ˈævaɪv sˈɛntɹəl stˈeɪʃən ˈɪn ˈeɪ fjˈuː mˈɪnɪts.";

    let audio = tts.create(
        phonemes,
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
    let out = "examples/out/basic-rs.wav";
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: tts.sample_rate(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(out, spec)?;
    for sample in audio {
        let sample = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(sample)?;
    }
    writer.finalize()?;

    println!("Saved {out}");
    Ok(())
}
