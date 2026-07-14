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

const VOICES: [(&str, &str); 2] = [("Rotem", "female1.json"), ("Roi", "male1.json")];

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
        // Keep legacy ids working for existing clients.
        styles.insert(
            "female1".to_owned(),
            VoiceStyle::from_json(voices_dir.join("female1.json"))?,
        );
        styles.insert(
            "male1".to_owned(),
            VoiceStyle::from_json(voices_dir.join("male1.json"))?,
        );

        Ok(Self {
            tts,
            phonemizer,
            styles,
            languages: vec![language("en", 0), language("he", 1)],
        })
    }

    fn language_for(text: &str, requested: &str) -> Result<(Language, &'static str)> {
        let requested = requested.trim().to_ascii_lowercase();
        let code = if requested.is_empty() || requested == "auto" {
            if text
                .chars()
                .any(|character| ('\u{0590}'..='\u{05ff}').contains(&character))
            {
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
            _ => bail!("unsupported Blue language `{code}`; expected he or en"),
        };
        Ok((language, language.code()))
    }

    fn normalize_voice(voice: &str) -> &str {
        match voice.trim() {
            "female1" => "Rotem",
            "male1" => "Roi",
            other => other,
        }
    }
}

impl Runtime for BlueRuntime {
    fn languages(&self) -> &[RuntimeLanguage] {
        &self.languages
    }

    fn voices(&self) -> Option<Vec<String>> {
        Some(VOICES.into_iter().map(|(id, _)| id.to_owned()).collect())
    }

    fn sample_rate(&self) -> u32 {
        self.tts.sample_rate()
    }

    fn phonemize(&mut self, text: &str, language: &str) -> Result<String> {
        let (language, _) = Self::language_for(text, language)?;
        self.phonemizer.g2p(text, language).map(strip_language_tags)
    }

    fn supported_phonemes(&self) -> Vec<char> {
        self.tts.supported_phonemes()
    }

    fn synthesize_streaming(
        &mut self,
        text: &str,
        voice: Option<&str>,
        language: &str,
        on_chunk: &mut dyn FnMut(&[f32], u32) -> Result<()>,
    ) -> Result<Vec<f32>> {
        let (_language, language_code) = Self::language_for(text, language)?;
        let voice = Self::normalize_voice(voice.unwrap_or("Rotem"));
        let style = self.styles.get(voice).ok_or_else(|| {
            anyhow::anyhow!("unknown Blue voice `{voice}`; expected Rotem or Roi")
        })?;
        let sample_rate = self.tts.sample_rate();
        self.tts.synthesize_text_streaming(
            &mut self.phonemizer,
            text,
            style,
            SynthesisOptions {
                lang: language_code.to_owned(),
                total_step: 8,
                cfg_scale: 4.0,
                speed: 0.95,
                chunking: Some(blue_rs::ChunkingOptions {
                    enabled: true,
                    silence_seconds: 0.15,
                    max_chars: Some(200),
                }),
            },
            |chunk| on_chunk(chunk, sample_rate),
        )
    }

    fn synthesize_phonemes_streaming(
        &mut self,
        phonemes: &str,
        voice: Option<&str>,
        language: &str,
        on_chunk: &mut dyn FnMut(&[f32], u32) -> Result<()>,
    ) -> Result<Vec<f32>> {
        let (_, language_code) = Self::language_for(phonemes, language)?;
        let voice = Self::normalize_voice(voice.unwrap_or("Rotem"));
        let style = self.styles.get(voice).ok_or_else(|| {
            anyhow::anyhow!("unknown Blue voice `{voice}`; expected Rotem or Roi")
        })?;
        let audio = self.tts.create(
            phonemes,
            style,
            SynthesisOptions {
                lang: language_code.to_owned(),
                total_step: 8,
                cfg_scale: 4.0,
                speed: 0.95,
                chunking: Some(blue_rs::ChunkingOptions {
                    enabled: true,
                    silence_seconds: 0.15,
                    max_chars: Some(200),
                }),
            },
        )?;
        on_chunk(&audio, self.tts.sample_rate())?;
        Ok(audio)
    }

    fn synthesize_to_file(
        &mut self,
        text: &str,
        voice: Option<&str>,
        output_path: &Path,
        language: &str,
    ) -> Result<()> {
        let (_language, language_code) = Self::language_for(text, language)?;
        let voice = Self::normalize_voice(voice.unwrap_or("Rotem"));
        let style = self.styles.get(voice).ok_or_else(|| {
            anyhow::anyhow!("unknown Blue voice `{voice}`; expected Rotem or Roi")
        })?;
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

fn strip_language_tags(phonemes: String) -> String {
    ["en", "es", "de", "it", "he"]
        .into_iter()
        .fold(phonemes, |output, language| {
            output
                .replace(&format!("<{language}>"), "")
                .replace(&format!("</{language}>"), "")
        })
}

#[cfg(test)]
mod tests {
    use super::{BlueRuntime, strip_language_tags};

    #[test]
    fn detects_hebrew_and_rejects_unsupported_languages() {
        assert_eq!(BlueRuntime::language_for("שלום", "auto").unwrap().1, "he");
        assert_eq!(BlueRuntime::language_for("Hello", "auto").unwrap().1, "en");
        assert!(BlueRuntime::language_for("Hola", "es").is_err());
        assert!(BlueRuntime::language_for("Bonjour", "fr").is_err());
    }

    #[test]
    fn strips_internal_language_tags_from_preview_ipa() {
        assert_eq!(
            strip_language_tags("<he>ʃalˈom</he> , <en>həlˈoʊ</en>".into()),
            "ʃalˈom , həlˈoʊ"
        );
    }
}
