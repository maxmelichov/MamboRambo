use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::runtime::Language;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub loaded: bool,
    pub model: String,
    pub runtime: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelsResponse {
    pub loaded: bool,
    pub runtime: String,
    pub model: String,
    pub path: String,
    pub codec: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LanguagesResponse {
    pub languages: Vec<String>,
    pub items: Vec<Language>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoicesResponse {
    pub runtime: String,
    pub voices: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoadResponse {
    pub status: String,
    pub model: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoadBody {
    #[serde(default)]
    pub model_path: String,
    #[serde(default)]
    pub renikud_path: String,
}

impl Default for LoadBody {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            renikud_path: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SpeechBody {
    pub input: String,
    #[serde(default)]
    pub voice_reference: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub response_format: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub stream: bool,
}
