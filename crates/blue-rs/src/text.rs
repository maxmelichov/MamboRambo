use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use ndarray::{Array2, Array3};
use regex::Regex;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

pub(crate) struct Tokenizer {
    pad_id: i64,
    char_to_id: HashMap<char, i64>,
    tag_re: Regex,
}

impl Tokenizer {
    pub(crate) fn from_json(path: impl AsRef<Path>) -> Result<Self> {
        let raw = fs::read_to_string(path.as_ref())
            .with_context(|| format!("read vocab {}", path.as_ref().display()))?;
        Self::from_json_bytes(raw.as_bytes())
    }

    pub(crate) fn from_json_bytes(raw: &[u8]) -> Result<Self> {
        let json: Value = serde_json::from_slice(raw)?;
        let pad_id = json["pad_id"].as_i64().unwrap_or(0);
        let mut char_to_id = HashMap::new();
        for (k, v) in json["char_to_id"]
            .as_object()
            .context("char_to_id missing")?
        {
            if let Some(ch) = k.chars().next() {
                char_to_id.insert(ch, v.as_i64().unwrap_or(pad_id));
            }
        }
        Ok(Self {
            pad_id,
            char_to_id,
            tag_re: Regex::new(r"</?[^>]+>")?,
        })
    }

    pub(crate) fn encode_batch(
        &self,
        texts: &[&str],
        langs: &[&str],
    ) -> Result<(Array2<i64>, Array3<f32>)> {
        if texts.len() != langs.len() {
            bail!("texts/langs length mismatch");
        }
        let encoded: Vec<Vec<i64>> = texts
            .iter()
            .zip(langs)
            .map(|(text, lang)| self.encode_one(text, lang))
            .collect::<Result<_>>()?;
        let max_len = encoded.iter().map(Vec::len).max().unwrap_or(0);
        let mut ids = Array2::from_elem((texts.len(), max_len), self.pad_id);
        let mut mask = Array3::zeros((texts.len(), 1, max_len));
        for (row, seq) in encoded.iter().enumerate() {
            for (col, id) in seq.iter().copied().enumerate() {
                ids[[row, col]] = id;
                mask[[row, 0, col]] = 1.0;
            }
        }
        Ok((ids, mask))
    }

    fn encode_one(&self, text: &str, lang: &str) -> Result<Vec<i64>> {
        if !matches!(lang, "en" | "es" | "de" | "it" | "he") {
            bail!("invalid language: {lang}");
        }
        let mut text = preprocess_phonemes(text);
        if !text.ends_with(|c| {
            matches!(
                c,
                '.' | '!' | '?' | ';' | ':' | ',' | '\'' | '"' | ')' | ']' | '}'
            )
        }) {
            text.push('.');
        }
        let wrapped = format!("<{lang}>{text}</{lang}>");
        let stripped = self.tag_re.replace_all(&wrapped, "");
        Ok(stripped
            .chars()
            .map(|ch| *self.char_to_id.get(&ch).unwrap_or(&self.pad_id))
            .collect())
    }
}

fn preprocess_phonemes(text: &str) -> String {
    let mut text: String = text
        .nfkd()
        .filter(|character| !is_hebrew_nikud(*character) && !is_emoji(*character))
        .collect();
    for (from, to) in [
        ("–", "-"),
        ("‑", "-"),
        ("—", "-"),
        ("_", " "),
        ("“", "\""),
        ("”", "\""),
        ("‘", "'"),
        ("’", "'"),
        ("´", "'"),
        ("`", "'"),
        ("[", " "),
        ("]", " "),
        ("|", " "),
        ("/", " "),
        ("#", " "),
        ("→", " "),
        ("←", " "),
    ] {
        text = text.replace(from, to);
    }
    text = text.replace('@', " at ");
    text = text.replace("e.g.,", "for example, ");
    text = text.replace("i.e.,", "that is, ");
    text = Regex::new(r"(?u)([\u{0590}-\u{05ff}])[-–—‑]+([A-Za-z0-9])")
        .expect("valid regex")
        .replace_all(&text, "$1 $2")
        .into_owned();
    text = Regex::new(r"(?u)([A-Za-z0-9])[-–—‑]+([\u{0590}-\u{05ff}])")
        .expect("valid regex")
        .replace_all(&text, "$1 $2")
        .into_owned();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_hebrew_nikud(character: char) -> bool {
    matches!(character, '\u{0591}'..='\u{05bd}' | '\u{05bf}' | '\u{05c1}'..='\u{05c2}' | '\u{05c4}'..='\u{05c5}' | '\u{05c7}')
}

fn is_emoji(character: char) -> bool {
    matches!(
        character as u32,
        0x1f600..=0x1f64f
            | 0x1f300..=0x1f5ff
            | 0x1f680..=0x1faff
            | 0x2600..=0x26ff
            | 0x2700..=0x27bf
            | 0x1f1e6..=0x1f1ff
    )
}
