use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ReadySignal {
    pub status: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct RunnerInfo {
    pub base_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LanguagesResponse {
    pub languages: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoicesResponse {
    pub voices: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoadModelRequest {
    #[serde(default = "default_runtime")]
    pub runtime: String,
    pub model_path: String,
    pub renikud_path: String,
}

fn default_runtime() -> String {
    mamborambo_registry::DEFAULT_RUNTIME_ID.into()
}

#[derive(Debug, Deserialize)]
pub struct SpeechRequest {
    pub input: String,
    pub voice_reference: Option<String>,
    pub voice: Option<String>,
    pub output_path: Option<String>,
    pub language: Option<String>,
}
