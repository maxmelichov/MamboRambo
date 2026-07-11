//! Model-independent handling for difficult Hebrew TTS input.
//!
//! This layer is intentionally above Blue's tokenizer: it normalizes written
//! forms, and can replace only niqqud-bearing words with IPA.  It does not
//! choose a voice or run model inference.

use anyhow::{Result, bail};
use regex::{Captures, Regex};

/// Destination representation expected by the selected synthesis model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    Text,
    Phonemes,
}

/// A vocalized Hebrew source word and its IPA equivalent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoneticSpan {
    pub source: String,
    pub ipa: String,
}

/// Safe text and phoneme-ready renderings of one original input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedText {
    pub original: String,
    pub text: String,
    pub phonetic_text: String,
    pub phonetic_spans: Vec<PhoneticSpan>,
}

impl PreparedText {
    pub fn for_input(&self, mode: InputMode) -> &str {
        match mode {
            InputMode::Text => &self.text,
            InputMode::Phonemes => &self.phonetic_text,
        }
    }
}

/// Adapter for a niqqud-to-IPA engine such as Phonikud.
///
/// Keeping this as a trait avoids requiring Python or an unavailable Rust
/// Phonikud crate in every BlueTTS consumer.  A host can use a native port,
/// FFI, or a service implementation.
pub trait NikudPhonemizer {
    fn phonemize_nikud(&mut self, text: &str) -> Result<String>;
}

impl<F> NikudPhonemizer for F
where
    F: FnMut(&str) -> Result<String>,
{
    fn phonemize_nikud(&mut self, text: &str) -> Result<String> {
        self(text)
    }
}

/// Normalize structured text while preserving ordinary Hebrew words.
pub fn normalize_for_speech(text: &str) -> String {
    let heading = Regex::new(r"(?m)^\s{0,3}#{1,6}\s+").expect("valid heading regex");
    let list = Regex::new(r"(?m)(^|\n)\s*(\d{1,2})[.)]\s+").expect("valid list regex");
    let email =
        Regex::new(r"(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}").expect("valid email regex");
    let phone = Regex::new(r"(?:\*\d{2,}|0\d{1,2}-\d{6,8})").expect("valid phone regex");
    let date = Regex::new(r"([0-3]?\d)[/.]([01]?\d)[/.](\d{2}|\d{4})").expect("valid date regex");
    let time = Regex::new(r"([01]?\d|2[0-3]):([0-5]\d)").expect("valid time regex");
    let identifier =
        Regex::new(r"[A-Za-z]+(?:[-_/]?[A-Za-z0-9]+)+").expect("valid identifier regex");

    let mut text = heading.replace_all(text, "").into_owned();
    text = text
        .replace('–', "-")
        .replace('—', "-")
        .replace('‑', "-")
        .replace('[', " ")
        .replace(']', " ")
        .replace('{', " ")
        .replace('}', " ")
        .replace('(', ",")
        .replace(')', ",");
    text = normalize_hebrew_punctuation(&text);
    text = list
        .replace_all(&text, |caps: &Captures| {
            format!("{}{}. ", &caps[1], list_number(caps[2].parse().unwrap_or(0)))
        })
        .into_owned();
    text = email.replace_all(&text, |caps: &Captures| expand_email(&caps[0])).into_owned();
    text = phone.replace_all(&text, |caps: &Captures| expand_phone(&caps[0])).into_owned();
    text = date.replace_all(&text, |caps: &Captures| expand_date(caps)).into_owned();
    text = time.replace_all(&text, |caps: &Captures| expand_time(caps)).into_owned();
    text = identifier
        .replace_all(&text, |caps: &Captures| expand_identifier(&caps[0]))
        .into_owned();
    normalize_punctuation(&text)
}

/// Prepare one sentence. When `phonetic_mode` is false no phonemizer is needed.
///
/// Niqqud is retained in `text` for text-native models. `phonetic_text`
/// replaces only the niqqud-bearing Hebrew words with IPA for Blue-like models.
pub fn prepare_text(
    text: &str,
    mut phonemizer: Option<&mut dyn NikudPhonemizer>,
    phonetic_mode: bool,
) -> Result<PreparedText> {
    let normalized = normalize_for_speech(text);
    if !phonetic_mode || !contains_nikud(&normalized) {
        return Ok(PreparedText {
            original: text.to_owned(),
            text: normalized.clone(),
            phonetic_text: normalized,
            phonetic_spans: Vec::new(),
        });
    }

    let phonemizer = phonemizer
        .as_deref_mut()
        .ok_or_else(|| anyhow::anyhow!("niqqud input requires a Phonikud-compatible phonemizer"))?;
    let (phonetic_text, phonetic_spans) = replace_nikud_with_ipa(&normalized, phonemizer)?;
    Ok(PreparedText {
        original: text.to_owned(),
        text: normalized,
        phonetic_text,
        phonetic_spans,
    })
}

fn replace_nikud_with_ipa(
    text: &str,
    phonemizer: &mut dyn NikudPhonemizer,
) -> Result<(String, Vec<PhoneticSpan>)> {
    let mut output = String::with_capacity(text.len());
    let mut spans = Vec::new();
    let mut word = String::new();

    let flush_word = |word: &mut String,
                      output: &mut String,
                      spans: &mut Vec<PhoneticSpan>,
                      phonemizer: &mut dyn NikudPhonemizer|
     -> Result<()> {
        if word.is_empty() {
            return Ok(());
        }
        if contains_nikud(word) {
            let ipa = phonemizer.phonemize_nikud(word)?.trim().to_owned();
            if ipa.is_empty() {
                bail!("Phonikud returned an empty IPA string for `{word}`");
            }
            output.push_str(&ipa);
            spans.push(PhoneticSpan {
                source: std::mem::take(word),
                ipa,
            });
        } else {
            output.push_str(word);
            word.clear();
        }
        Ok(())
    };

    for character in text.chars() {
        if is_hebrew_word_character(character) {
            word.push(character);
        } else {
            flush_word(&mut word, &mut output, &mut spans, phonemizer)?;
            output.push(character);
        }
    }
    flush_word(&mut word, &mut output, &mut spans, phonemizer)?;
    Ok((output, spans))
}

fn normalize_hebrew_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    for (index, character) in chars.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let next = chars.get(index + 1).copied();
        if matches!(character, '"' | '״')
            && previous.is_some_and(is_hebrew_letter)
            && next.is_some_and(is_hebrew_letter)
        {
            continue;
        }
        if character == '\'' && matches!(previous, Some('ג' | 'צ' | 'ז')) && next.is_some_and(is_hebrew_letter) {
            output.push('׳');
            continue;
        }
        if character == '-' && previous.is_some_and(is_hebrew_letter) && next.is_some_and(is_hebrew_letter) {
            continue;
        }
        output.push(character);
    }
    output
}

fn normalize_punctuation(text: &str) -> String {
    let text = text.replace('…', ",");
    let dots = Regex::new(r"\.{2,}").expect("valid dot regex");
    let bangs = Regex::new(r"!{2,}").expect("valid exclamation regex");
    let questions = Regex::new(r"\?{2,}").expect("valid question regex");
    let commas = Regex::new(r",{2,}").expect("valid comma regex");
    let whitespace = Regex::new(r"\s+").expect("valid whitespace regex");
    let text = dots.replace_all(&text, ",");
    let text = bangs.replace_all(&text, "!");
    let text = questions.replace_all(&text, "?");
    let text = commas.replace_all(&text, ",");
    whitespace.replace_all(&text, " ").trim().to_owned()
}

fn expand_email(email: &str) -> String {
    let (local, domain) = email.split_once('@').expect("email regex contains @");
    let local = local.replace(['.', '_'], " dot ").replace('-', " dash ");
    let domain = domain
        .split('.')
        .map(|part| {
            if part.len() <= 2 && part.chars().all(|c| c.is_ascii_alphabetic()) {
                part.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" dot ");
    format!("{local} at {domain}")
}

fn expand_phone(phone: &str) -> String {
    let prefix = if phone.starts_with('*') { "כוכבית " } else { "" };
    format!("{prefix}{}.", spell_digits(phone))
}

fn expand_time(caps: &Captures) -> String {
    format!("שעה {} ו{}", spell_digits(&format!("{:02}", &caps[1])), spell_digits(&caps[2]))
}

fn expand_date(caps: &Captures) -> String {
    let day = caps[1].parse::<u8>().unwrap_or(0);
    let month = caps[2].parse::<u8>().unwrap_or(0);
    let Some(month_name) = month_name(month) else {
        return caps[0].to_owned();
    };
    if !(1..=31).contains(&day) {
        return caps[0].to_owned();
    }
    format!("{} ב{month_name} {}", spell_digits(&caps[1]), spell_digits(&caps[3]))
}

fn expand_identifier(token: &str) -> String {
    let letters: Vec<_> = token
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .map(|character| character.to_ascii_uppercase())
        .collect();
    if letters.is_empty() || !token.chars().any(|character| character.is_ascii_digit()) {
        return token.to_owned();
    }
    let digit_groups = token
        .split(['-', '_', '/'])
        .filter(|part| part.chars().any(|character| character.is_ascii_digit()))
        .map(spell_digits)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let letters = letters
        .into_iter()
        .map(|letter| letter.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{letters} {}.", digit_groups.join(", "))
}

fn spell_digits(text: &str) -> String {
    text.chars()
        .filter_map(|digit| match digit {
            '0' => Some("אפס"),
            '1' => Some("אחת"),
            '2' => Some("שתיים"),
            '3' => Some("שלוש"),
            '4' => Some("ארבע"),
            '5' => Some("חמש"),
            '6' => Some("שש"),
            '7' => Some("שבע"),
            '8' => Some("שמונה"),
            '9' => Some("תשע"),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn list_number(number: u8) -> String {
    match number {
        1 => "אחד".to_owned(),
        2 => "שתיים".to_owned(),
        3 => "שלוש".to_owned(),
        4 => "ארבע".to_owned(),
        5 => "חמש".to_owned(),
        6 => "שש".to_owned(),
        7 => "שבע".to_owned(),
        8 => "שמונה".to_owned(),
        9 => "תשע".to_owned(),
        10 => "עשר".to_owned(),
        _ => spell_digits(&number.to_string()),
    }
}

fn month_name(month: u8) -> Option<&'static str> {
    Some(match month {
        1 => "ינואר",
        2 => "פברואר",
        3 => "מרץ",
        4 => "אפריל",
        5 => "מאי",
        6 => "יוני",
        7 => "יולי",
        8 => "אוגוסט",
        9 => "ספטמבר",
        10 => "אוקטובר",
        11 => "נובמבר",
        12 => "דצמבר",
        _ => return None,
    })
}

fn contains_nikud(text: &str) -> bool {
    text.chars().any(|character| ('\u{0591}'..='\u{05c7}').contains(&character))
}

fn is_hebrew_letter(character: char) -> bool {
    ('\u{05d0}'..='\u{05ea}').contains(&character)
}

fn is_hebrew_word_character(character: char) -> bool {
    is_hebrew_letter(character)
        || ('\u{0591}'..='\u{05c7}').contains(&character)
        || matches!(character, '׳' | '״')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_only_vocalized_words_to_ipa() {
        let mut phonikud = |word: &str| Ok(format!("ipa:{word}"));
        let prepared = prepare_text("הַמְּנוֹרָה מאירה.", Some(&mut phonikud), true).unwrap();
        assert_eq!(prepared.phonetic_text, "ipa:הַמְּנוֹרָה מאירה.");
        assert_eq!(prepared.phonetic_spans.len(), 1);
        assert_eq!(prepared.for_input(InputMode::Text), "הַמְּנוֹרָה מאירה.");
    }

    #[test]
    fn normalizes_reference_codes_and_phone_numbers() {
        let text = normalize_for_speech("מספר TKT-90254, טלפון 03-5551234.");
        assert!(text.contains("T K T"));
        assert!(text.contains("אפס שלוש חמש חמש חמש"));
    }
}
