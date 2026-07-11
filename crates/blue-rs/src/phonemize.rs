use std::path::Path;

use anyhow::{Result, anyhow, bail};
use espeak_rs::text_to_phonemes;
use ort::session::Session;
use regex::Regex;
use renikud_rs::G2P;

/// Languages supported by the BlueTTS model.
///
/// Codes:
/// - `he` Hebrew, via Renikud when Hebrew characters are present.
/// - `en` English, via eSpeak voice `en-us`.
/// - `es` Spanish, via eSpeak voice `es`.
/// - `de` German, via eSpeak voice `de`.
/// - `it` Italian, via eSpeak voice `it`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Hebrew,
    English,
    Spanish,
    German,
    Italian,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Hebrew => "he",
            Self::English => "en",
            Self::Spanish => "es",
            Self::German => "de",
            Self::Italian => "it",
        }
    }

    pub fn espeak_voice(self) -> Option<&'static str> {
        match self {
            Self::Hebrew => None,
            Self::English => Some("en-us"),
            Self::Spanish => Some("es"),
            Self::German => Some("de"),
            Self::Italian => Some("it"),
        }
    }
}

impl TryFrom<&str> for Language {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "he" => Ok(Self::Hebrew),
            "en" | "en-us" => Ok(Self::English),
            "es" => Ok(Self::Spanish),
            "de" | "ge" => Ok(Self::German),
            "it" => Ok(Self::Italian),
            _ => bail!("unsupported language code `{value}`; expected he, en, es, de, or it"),
        }
    }
}

pub struct Phonemizer {
    hebrew: Option<G2P>,
    language: Language,
    latin_re: Regex,
}

impl Phonemizer {
    /// Create a phonemizer with Hebrew as the default language.
    ///
    /// Supported language codes are `he`, `en`, `es`, `de`, and `it`. Use
    /// [`Self::with_language`] or [`Self::phonemize_lang`] to select one.
    ///
    /// `renikud_model` is only required when phonemizing Hebrew text.
    pub fn new(renikud_model: Option<impl AsRef<Path>>) -> Result<Self> {
        Self::with_language(renikud_model, Language::Hebrew)
    }

    /// Create a phonemizer with an explicit default language.
    ///
    /// Supported model language codes are `he`, `en`, `es`, `de`, and `it`.
    /// Non-Hebrew languages use eSpeak. Hebrew uses Renikud when Hebrew
    /// characters are present.
    pub fn with_language(
        renikud_model: Option<impl AsRef<Path>>,
        language: Language,
    ) -> Result<Self> {
        let hebrew = match renikud_model {
            Some(path) => Some(G2P::new(path.as_ref().to_string_lossy().as_ref())?),
            None => None,
        };
        Ok(Self {
            hebrew,
            language,
            latin_re: Regex::new(r"[A-Za-z]+(?:['-][A-Za-z]+)*")?,
        })
    }

    /// Create a phonemizer from embedded Renikud ONNX bytes.
    ///
    /// Supported model language codes are `he`, `en`, `es`, `de`, and `it`.
    /// This is useful for self-contained binaries built with `include_bytes!`.
    pub fn from_renikud_bytes(bytes: &[u8], language: Language) -> Result<Self> {
        let builder = Session::builder()?;
        let session = builder.commit_from_memory(bytes)?;
        Ok(Self {
            hebrew: Some(G2P::from_session(session)?),
            language,
            latin_re: Regex::new(r"[A-Za-z]+(?:['-][A-Za-z]+)*")?,
        })
    }

    /// Phonemize text using the default language.
    ///
    /// Supported model language codes are `he`, `en`, `es`, `de`, and `it`.
    /// For mixed Hebrew/Latin input, Hebrew spans use Renikud and Latin spans
    /// use the default language's eSpeak voice, falling back to English for
    /// Hebrew default.
    pub fn phonemize(&mut self, text: &str) -> Result<String> {
        self.phonemize_lang(text, self.language)
    }

    /// Phonemize text using an explicit supported model language.
    ///
    /// Supported model language codes are `he`, `en`, `es`, `de`, and `it`.
    pub fn phonemize_lang(&mut self, text: &str, language: Language) -> Result<String> {
        if language != Language::Hebrew && !contains_hebrew(text) {
            return self.phonemize_espeak(text, language);
        }

        let mut result = String::new();
        let mut last = 0;

        let latin_spans: Vec<(usize, usize)> = self
            .latin_re
            .find_iter(text)
            .map(|m| (m.start(), m.end()))
            .collect();

        for (start, end) in latin_spans {
            let non_latin = &text[last..start];
            if !non_latin.is_empty() {
                result.push_str(&self.phonemize_non_latin(non_latin)?);
            }

            let latin_language = if language == Language::Hebrew {
                Language::English
            } else {
                language
            };
            let ipa = self.phonemize_espeak(&text[start..end], latin_language)?;
            result.push_str(&ipa);
            last = end;
        }

        let rest = &text[last..];
        if !rest.is_empty() {
            result.push_str(&self.phonemize_non_latin(rest)?);
        }

        Ok(normalize_spaces(&result))
    }

    fn phonemize_espeak(&self, text: &str, language: Language) -> Result<String> {
        let voice = language
            .espeak_voice()
            .ok_or_else(|| anyhow!("language `{}` does not use eSpeak", language.code()))?;
        Ok(text_to_phonemes(text, voice, None)
            .map_err(|e| anyhow!("{e}"))?
            .join(" "))
    }

    fn phonemize_non_latin(&mut self, text: &str) -> Result<String> {
        if !contains_hebrew(text) {
            return Ok(text.to_string());
        }

        let Some(g2p) = self.hebrew.as_mut() else {
            bail!("Hebrew phonemization needs a Renikud model path");
        };
        g2p.phonemize(text)
    }
}

pub fn phonemize(text: &str, renikud_model: Option<impl AsRef<Path>>) -> Result<String> {
    let mut phonemizer = Phonemizer::new(renikud_model)?;
    phonemizer.phonemize(text)
}

fn contains_hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{0590}'..='\u{05ff}').contains(&c))
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
