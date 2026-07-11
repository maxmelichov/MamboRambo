//! Model-independent handling for difficult Hebrew TTS input.
//!
//! Niqqud-bearing words are kept intact for [Phonikud](https://github.com/phonikud/phonikud)
//! grapheme-to-IPA. Separately, niqqud is stripped so Renikud can run on plain
//! Hebrew, then Phonikud's stress mark (`ˈ`) is copied onto the Renikud IPA.

use anyhow::{Result, bail};
use regex::{Captures, Regex};

/// Primary stress mark used by Phonikud (`U+02C8`).
pub const STRESS_MARK: char = '\u{02c8}';

/// Destination representation expected by the selected synthesis model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    Text,
    Phonemes,
}

/// A vocalized Hebrew source word and its IPA stages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoneticSpan {
    /// Original word, including niqqud.
    pub source: String,
    /// Phonikud IPA (stress included when present).
    pub phonikud_ipa: String,
    /// Renikud IPA from the stripped (plain) form, when available.
    pub renikud_ipa: Option<String>,
    /// Final IPA: Renikud base with Phonikud stress when both exist,
    /// otherwise Phonikud alone.
    pub ipa: String,
}

/// Safe text and phoneme-ready renderings of one original input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedText {
    pub original: String,
    /// Normalized text with niqqud preserved (for text-native models).
    pub text: String,
    /// Same as `text`, but niqqud words replaced by final IPA.
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

/// Adapter for Phonikud (or any niqqud → IPA engine).
///
/// Callers must pass the vocalized word **with niqqud kept**.
pub trait NikudPhonemizer {
    fn phonemize_nikud(&mut self, vocalized: &str) -> Result<String>;
}

impl<F> NikudPhonemizer for F
where
    F: FnMut(&str) -> Result<String>,
{
    fn phonemize_nikud(&mut self, vocalized: &str) -> Result<String> {
        self(vocalized)
    }
}

/// Adapter for Renikud (or any plain-Hebrew → IPA engine).
///
/// Callers must pass Hebrew **without niqqud**.
pub trait PlainHebrewPhonemizer {
    fn phonemize_plain(&mut self, unvocalized: &str) -> Result<String>;
}

impl<F> PlainHebrewPhonemizer for F
where
    F: FnMut(&str) -> Result<String>,
{
    fn phonemize_plain(&mut self, unvocalized: &str) -> Result<String> {
        self(unvocalized)
    }
}

/// Strip Hebrew niqqud / cantillation while keeping letters and geresh.
pub fn strip_nikud(text: &str) -> String {
    text.chars()
        .filter(|character| !is_nikud(*character))
        .collect()
}

/// True when `text` contains any Hebrew niqqud / cantillation mark.
pub fn contains_nikud(text: &str) -> bool {
    text.chars().any(is_nikud)
}

/// Index of the stressed vowel in IPA (`0` = first vowel), if any.
pub fn vowel_stress_index(ipa: &str) -> Option<usize> {
    let mut vowel_index = 0usize;
    let mut pending_stress = false;
    for character in ipa.chars() {
        if character == STRESS_MARK {
            pending_stress = true;
            continue;
        }
        if is_ipa_vowel(character) {
            if pending_stress {
                return Some(vowel_index);
            }
            vowel_index += 1;
        } else {
            // Stress must sit immediately before its vowel.
            pending_stress = false;
        }
    }
    None
}

/// Insert `ˈ` immediately before the vowel at `vowel_index`, replacing any
/// existing stress marks.
pub fn apply_vowel_stress(ipa: &str, vowel_index: usize) -> String {
    let plain: String = ipa.chars().filter(|&c| c != STRESS_MARK).collect();
    let mut output = String::with_capacity(plain.len() + STRESS_MARK.len_utf8());
    let mut seen = 0usize;
    let mut placed = false;
    for character in plain.chars() {
        if !placed && is_ipa_vowel(character) {
            if seen == vowel_index {
                output.push(STRESS_MARK);
                placed = true;
            }
            seen += 1;
        }
        output.push(character);
    }
    if !placed {
        // Clamp: stress the last vowel when indices diverge.
        return apply_vowel_stress_last(&plain);
    }
    output
}

/// Copy Phonikud stress placement onto Renikud IPA.
///
/// Renikud receives the plain (niqqud-stripped) form; Phonikud keeps stress
/// from hatama / milra rules. When Phonikud has no stress mark, Renikud IPA is
/// returned unchanged (aside from whitespace normalize).
pub fn transfer_stress(phonikud_ipa: &str, renikud_ipa: &str) -> String {
    let renikud = normalize_spaces(renikud_ipa);
    match vowel_stress_index(phonikud_ipa) {
        Some(index) => apply_vowel_stress(&renikud, index),
        None => renikud,
    }
}

/// Phonemize one vocalized Hebrew word:
/// 1. Phonikud on the word **with niqqud**
/// 2. optionally Renikud on the **stripped** form
/// 3. merge Phonikud stress onto Renikud IPA
pub fn phonemize_nikud_word(
    word: &str,
    phonikud: &mut dyn NikudPhonemizer,
    renikud: Option<&mut dyn PlainHebrewPhonemizer>,
) -> Result<PhoneticSpan> {
    if !contains_nikud(word) {
        bail!("phonemize_nikud_word expects a niqqud-bearing word, got `{word}`");
    }

    let phonikud_ipa = phonikud.phonemize_nikud(word)?.trim().to_owned();
    if phonikud_ipa.is_empty() {
        bail!("Phonikud returned empty IPA for `{word}`");
    }

    let plain = strip_nikud(word);
    if plain.chars().any(is_hebrew_letter) {
        if let Some(renikud) = renikud {
            let renikud_ipa = renikud.phonemize_plain(&plain)?.trim().to_owned();
            if renikud_ipa.is_empty() {
                bail!("Renikud returned empty IPA for stripped `{plain}`");
            }
            let ipa = transfer_stress(&phonikud_ipa, &renikud_ipa);
            return Ok(PhoneticSpan {
                source: word.to_owned(),
                phonikud_ipa,
                renikud_ipa: Some(renikud_ipa),
                ipa,
            });
        }
    }

    Ok(PhoneticSpan {
        source: word.to_owned(),
        ipa: phonikud_ipa.clone(),
        phonikud_ipa,
        renikud_ipa: None,
    })
}

/// Normalize structured text while preserving ordinary Hebrew words **and**
/// their niqqud.
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
    text = email
        .replace_all(&text, |caps: &Captures| expand_email(&caps[0]))
        .into_owned();
    text = phone
        .replace_all(&text, |caps: &Captures| expand_phone(&caps[0]))
        .into_owned();
    text = date
        .replace_all(&text, |caps: &Captures| expand_date(caps))
        .into_owned();
    text = time
        .replace_all(&text, |caps: &Captures| expand_time(caps))
        .into_owned();
    text = identifier
        .replace_all(&text, |caps: &Captures| expand_identifier(&caps[0]))
        .into_owned();
    normalize_punctuation(&text)
}

/// Prepare one sentence.
///
/// - `text` keeps niqqud (never stripped for text-native models).
/// - `phonetic_text` replaces only niqqud-bearing words with final IPA via
///   Phonikud (+ optional Renikud stress merge).
pub fn prepare_text(
    text: &str,
    mut phonikud: Option<&mut dyn NikudPhonemizer>,
    mut renikud: Option<&mut dyn PlainHebrewPhonemizer>,
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

    let phonikud = phonikud.as_deref_mut().ok_or_else(|| {
        anyhow::anyhow!("niqqud input requires a Phonikud-compatible phonemizer")
    })?;
    let (phonetic_text, phonetic_spans) =
        replace_nikud_words(&normalized, phonikud, renikud.as_deref_mut())?;
    Ok(PreparedText {
        original: text.to_owned(),
        text: normalized,
        phonetic_text,
        phonetic_spans,
    })
}

fn replace_nikud_words(
    text: &str,
    phonikud: &mut dyn NikudPhonemizer,
    mut renikud: Option<&mut dyn PlainHebrewPhonemizer>,
) -> Result<(String, Vec<PhoneticSpan>)> {
    let mut output = String::with_capacity(text.len());
    let mut spans = Vec::new();
    let mut word = String::new();

    let flush = |word: &mut String,
                 output: &mut String,
                 spans: &mut Vec<PhoneticSpan>,
                 phonikud: &mut dyn NikudPhonemizer,
                 renikud: &mut Option<&mut dyn PlainHebrewPhonemizer>|
     -> Result<()> {
        if word.is_empty() {
            return Ok(());
        }
        if contains_nikud(word) {
            let span = phonemize_nikud_word(word, phonikud, renikud.as_deref_mut())?;
            output.push_str(&span.ipa);
            spans.push(span);
            word.clear();
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
            flush(
                &mut word,
                &mut output,
                &mut spans,
                phonikud,
                &mut renikud,
            )?;
            output.push(character);
        }
    }
    flush(
        &mut word,
        &mut output,
        &mut spans,
        phonikud,
        &mut renikud,
    )?;
    Ok((output, spans))
}

fn apply_vowel_stress_last(plain: &str) -> String {
    let vowel_count = plain.chars().filter(|&c| is_ipa_vowel(c)).count();
    if vowel_count == 0 {
        return plain.to_owned();
    }
    apply_vowel_stress(plain, vowel_count - 1)
}

fn is_ipa_vowel(character: char) -> bool {
    matches!(
        character,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | 'ə'
            | 'ɑ'
            | 'ɛ'
            | 'ɔ'
            | 'ɪ'
            | 'ʊ'
            | 'ɐ'
            | 'æ'
            | 'ø'
            | 'œ'
            | 'ɨ'
            | 'ʉ'
    )
}

fn is_nikud(character: char) -> bool {
    // Hebrew points + cantillation, including Phonikud's Ole (hatama) / Meteg.
    ('\u{0591}'..='\u{05c7}').contains(&character)
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
        if character == '\''
            && matches!(previous, Some('ג' | 'צ' | 'ז'))
            && next.is_some_and(is_hebrew_letter)
        {
            output.push('׳');
            continue;
        }
        if character == '-'
            && previous.is_some_and(is_hebrew_letter)
            && next.is_some_and(is_hebrew_letter)
        {
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
                part.chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" dot ");
    format!("{local} at {domain}")
}

fn expand_phone(phone: &str) -> String {
    let prefix = if phone.starts_with('*') {
        "כוכבית "
    } else {
        ""
    };
    format!("{prefix}{}.", spell_digits(phone))
}

fn expand_time(caps: &Captures) -> String {
    format!(
        "שעה {} ו{}",
        spell_digits(&format!("{:0>2}", &caps[1])),
        spell_digits(&caps[2])
    )
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
    format!(
        "{} ב{month_name} {}",
        spell_digits(&caps[1]),
        spell_digits(&caps[3])
    )
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

fn is_hebrew_letter(character: char) -> bool {
    ('\u{05d0}'..='\u{05ea}').contains(&character)
}

fn is_hebrew_word_character(character: char) -> bool {
    is_hebrew_letter(character)
        || is_nikud(character)
        || matches!(character, '׳' | '״' | '|')
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_nikud_in_text_path() {
        let mut phonikud = |word: &str| Ok(format!("ipa:{word}"));
        let prepared =
            prepare_text("הַמְּנוֹרָה מאירה.", Some(&mut phonikud), None, true).unwrap();
        assert!(contains_nikud(&prepared.text));
        assert_eq!(prepared.for_input(InputMode::Text), "הַמְּנוֹרָה מאירה.");
        assert_eq!(prepared.phonetic_text, "ipa:הַמְּנוֹרָה מאירה.");
    }

    #[test]
    fn transfers_phonikud_stress_onto_renikud_ipa() {
        // Phonikud: stress on 2nd vowel (o). Renikud: different quality, no stress.
        let merged = transfer_stress("ʃalˈom", "ʃalom");
        assert_eq!(merged, "ʃalˈom");

        let merged = transfer_stress("haˈir", "heir");
        assert_eq!(merged, "heˈir");
    }

    #[test]
    fn hybrid_word_uses_renikud_base_and_phonikud_stress() {
        let mut phonikud = |_word: &str| Ok("menˈora".to_owned());
        let mut renikud = |plain: &str| {
            assert!(!contains_nikud(plain));
            Ok("menora".to_owned())
        };
        let span = phonemize_nikud_word(
            "מְנוֹרָה",
            &mut phonikud,
            Some(&mut renikud),
        )
        .unwrap();
        assert_eq!(span.phonikud_ipa, "menˈora");
        assert_eq!(span.renikud_ipa.as_deref(), Some("menora"));
        assert_eq!(span.ipa, "menˈora");
    }

    #[test]
    fn strip_nikud_preserves_letters() {
        assert_eq!(strip_nikud("הַמְּנוֹרָה"), "המנורה");
    }

    #[test]
    fn normalizes_reference_codes_and_phone_numbers() {
        let text = normalize_for_speech("מספר TKT-90254, טלפון 03-5551234.");
        assert!(text.contains("T K T"));
        assert!(text.contains("אפס שלוש חמש חמש חמש"));
    }
}
