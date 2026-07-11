#[derive(Clone, Debug)]
pub struct ChunkingOptions {
    pub enabled: bool,
    pub silence_seconds: f32,
    pub max_chars: Option<usize>,
}

impl Default for ChunkingOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            silence_seconds: 0.2,
            max_chars: None,
        }
    }
}

pub(crate) fn split_phonemes(input: &str, max_chars: Option<usize>) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        current.push(ch);
        if is_sentence_boundary(ch) || max_chars.is_some_and(|max| current.chars().count() >= max) {
            push_chunk(&mut chunks, &mut current);
        }
    }
    push_chunk(&mut chunks, &mut current);

    if chunks.is_empty() {
        chunks.push(input.trim().to_string());
    }
    chunks
}

pub(crate) fn append_silence(audio: &mut Vec<f32>, sample_rate: u32, seconds: f32) {
    let n = (seconds.max(0.0) * sample_rate as f32).round() as usize;
    audio.extend(std::iter::repeat_n(0.0, n));
}

fn push_chunk(chunks: &mut Vec<String>, current: &mut String) {
    let chunk = current.trim();
    if !chunk.is_empty() {
        chunks.push(chunk.to_string());
    }
    current.clear();
}

fn is_sentence_boundary(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | ';' | '…' | '।' | '؟' | '。')
}
