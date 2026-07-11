use std::path::Path;

use anyhow::{Result, anyhow, bail};
use espeak_rs::text_to_phonemes;
use ort::session::Session;
use regex::Regex;
use renikud_rs::G2P;

use crate::handling::{
    NikudPhonemizer, PlainHebrewPhonemizer, contains_nikud, phonemize_nikud_word, strip_nikud,
};

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
    /// Optional Phonikud (or compatible) engine for niqqud → IPA.
    ///
    /// When set, vocalized words keep niqqud for Phonikud, are stripped for
    /// Renikud, then Phonikud stress is transferred onto Renikud IPA.
    nikud: Option<Box<dyn NikudPhonemizer + Send>>,
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
            nikud: None,
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
            nikud: None,
        })
    }

    /// Attach a Phonikud-compatible engine for niqqud-bearing Hebrew words.
    ///
    /// Niqqud is **not** stripped before this engine. Renikud still receives
    /// the stripped form, and Phonikud stress is placed onto Renikud IPA.
    pub fn with_nikud_phonemizer(
        mut self,
        nikud: impl NikudPhonemizer + Send + 'static,
    ) -> Self {
        self.nikud = Some(Box::new(nikud));
        self
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

        if contains_nikud(text) && self.nikud.is_some() {
            return self.phonemize_hebrew_with_nikud(text);
        }

        // Renikud expects plain Hebrew — strip niqqud when Phonikud is absent.
        let plain = if contains_nikud(text) {
            strip_nikud(text)
        } else {
            text.to_owned()
        };
        self.phonemize_renikud(&plain)
    }

    /// Word-level hybrid path: Phonikud(keep niqqud) + Renikud(strip) + stress merge.
    fn phonemize_hebrew_with_nikud(&mut self, text: &str) -> Result<String> {
        let mut output = String::with_capacity(text.len());
        let mut word = String::new();

        for character in text.chars() {
            if is_hebrew_word_character(character) {
                word.push(character);
                continue;
            }
            self.flush_hebrew_word(&mut word, &mut output)?;
            output.push(character);
        }
        self.flush_hebrew_word(&mut word, &mut output)?;
        Ok(output)
    }

    fn flush_hebrew_word(&mut self, word: &mut String, output: &mut String) -> Result<()> {
        if word.is_empty() {
            return Ok(());
        }

        if contains_nikud(word) {
            // Temporarily take the nikud engine so Renikud can be borrowed too.
            let mut nikud = self
                .nikud
                .take()
                .ok_or_else(|| anyhow!("niqqud phonemizer missing"))?;
            let has_renikud = self.hebrew.is_some();
            let span = if has_renikud {
                let mut renikud_adapter = RenikudAdapter {
                    g2p: self.hebrew.as_mut(),
                };
                phonemize_nikud_word(
                    word,
                    nikud.as_mut(),
                    Some(&mut renikud_adapter as &mut dyn PlainHebrewPhonemizer),
                )
            } else {
                phonemize_nikud_word(word, nikud.as_mut(), None)
            };
            self.nikud = Some(nikud);
            output.push_str(&span?.ipa);
        } else if word.chars().any(|c| ('\u{0590}'..='\u{05ff}').contains(&c)) {
            output.push_str(&self.phonemize_renikud(word)?);
        } else {
            output.push_str(word);
        }
        word.clear();
        Ok(())
    }

    fn phonemize_renikud(&mut self, text: &str) -> Result<String> {
        let Some(g2p) = self.hebrew.as_mut() else {
            bail!("Hebrew phonemization needs a Renikud model path");
        };
        g2p.phonemize(text)
    }
}

/// Thin adapter so Renikud `G2P` satisfies [`PlainHebrewPhonemizer`].
struct RenikudAdapter<'a> {
    g2p: Option<&'a mut G2P>,
}

impl PlainHebrewPhonemizer for RenikudAdapter<'_> {
    fn phonemize_plain(&mut self, unvocalized: &str) -> Result<String> {
        let Some(g2p) = self.g2p.as_mut() else {
            bail!("Hebrew phonemization needs a Renikud model path");
        };
        g2p.phonemize(unvocalized)
    }
}

pub fn phonemize(text: &str, renikud_model: Option<impl AsRef<Path>>) -> Result<String> {
    let mut phonemizer = Phonemizer::new(renikud_model)?;
    phonemizer.phonemize(text)
}

fn contains_hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{0590}'..='\u{05ff}').contains(&c))
}

fn is_hebrew_word_character(character: char) -> bool {
    ('\u{05d0}'..='\u{05ea}').contains(&character)
        || ('\u{0591}'..='\u{05c7}').contains(&character)
        || matches!(character, '׳' | '״' | '|')
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
