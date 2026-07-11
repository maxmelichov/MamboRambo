use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use blue_rs::{
    BlueTts, SynthesisOptions, VoiceStyle,
    phonemize::{Language, Phonemizer},
};

use super::{Language as RuntimeLanguage, Runtime};

const VOICES: [(&str, &str); 2] = [("female1", "female1.json"), ("male1", "male1.json")];

pub struct BlueRuntime {
    tts: BlueTts,
    phonemizer: Phonemizer,
    styles: HashMap<String, VoiceStyle>,
    languages: Vec<RuntimeLanguage>,
}

impl BlueRuntime {
    pub fn load(model_dir: PathBuf, renikud_path: PathBuf) -> Result<Self> {
        let tts = BlueTts::from_dir(&model_dir)
            .with_context(|| format!("load Blue ONNX models from {}", model_dir.display()))?;
        let phonemizer = Phonemizer::with_language(Some(&renikud_path), Language::English)
            .with_context(|| format!("load Renikud model {}", renikud_path.display()))?;

        let voices_dir = model_dir.join("voices");
        let mut styles = HashMap::new();
        for (id, file) in VOICES {
            let path = voices_dir.join(file);
            let style = VoiceStyle::from_json(&path)
                .with_context(|| format!("load Blue voice style {}", path.display()))?;
            styles.insert(id.to_owned(), style);
        }

        Ok(Self {
            tts,
            phonemizer,
            styles,
            languages: vec![
                language("en", 0),
                language("he", 1),
                language("es", 2),
                language("de", 3),
                language("it", 4),
            ],
        })
    }

    fn language_for(text: &str, requested: &str) -> Result<(Language, &'static str)> {
        let requested = requested.trim().to_ascii_lowercase();
        let code = if requested.is_empty() || requested == "auto" {
            if text.chars().any(|character| ('\u{0590}'..='\u{05ff}').contains(&character)) {
                "he"
            } else {
                "en"
            }
        } else {
            requested.as_str()
        };
        let language = match code {
            "en" | "en-us" => Language::English,
            "he" => Language::Hebrew,
            "es" => Language::Spanish,
            "de" | "ge" => Language::German,
            "it" => Language::Italian,
            _ => bail!("unsupported Blue language `{code}`; expected he, en, es, de, or it"),
        };
        Ok((language, language.code()))
    }
}

impl Runtime for BlueRuntime {
    fn languages(&self) -> &[RuntimeLanguage] {
        &self.languages
    }

    fn voices(&self) -> Option<Vec<String>> {
        Some(VOICES.into_iter().map(|(id, _)| id.to_owned()).collect())
    }

    fn synthesize_to_file(
        &mut self,
        text: &str,
        voice: Option<&str>,
        output_path: &Path,
        language: &str,
    ) -> Result<()> {
        let (_language, language_code) = Self::language_for(text, language)?;
        let voice = voice.unwrap_or("female1");
        let style = self
            .styles
            .get(voice)
            .ok_or_else(|| anyhow::anyhow!("unknown Blue voice `{voice}`; expected female1 or male1"))?;
        let audio = self.tts.synthesize_text(
            &mut self.phonemizer,
            text,
            style,
            SynthesisOptions {
                lang: language_code.to_owned(),
                total_step: 8,
                cfg_scale: 4.0,
                speed: 0.95,
                chunking: None,
            },
        )?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.tts.sample_rate(),
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(output_path, spec)?;
        for sample in audio {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
        Ok(())
    }
}

fn language(name: &str, id: i32) -> RuntimeLanguage {
    RuntimeLanguage {
        name: name.to_owned(),
        id,
    }
}

#[cfg(test)]
mod tests {
    use super::BlueRuntime;

    #[test]
    fn detects_hebrew_and_rejects_unsupported_languages() {
        assert_eq!(BlueRuntime::language_for("שלום", "auto").unwrap().1, "he");
        assert_eq!(BlueRuntime::language_for("Hello", "auto").unwrap().1, "en");
        assert!(BlueRuntime::language_for("Bonjour", "fr").is_err());
    }
}
