use serde::Serialize;
use utoipa::ToSchema;

const BLUE_MODELS_TAG: &str = "blue-onnx-v2";
const BLUE_MODEL_BASE_URL: &str = "https://huggingface.co/notmax123/blue-onnx-v2/resolve/main";
const RENIKUD_URL: &str = "https://huggingface.co/thewh1teagle/renikud/resolve/main/model.onnx";

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelSourceFile {
    name: &'static str,
    url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelSource {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    size: &'static str,
    description: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    files: Vec<ModelSourceFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_url: Option<&'static str>,
    directory: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelSourcesResponse {
    runtimes: Vec<ModelSource>,
    voices_url: &'static str,
    default_paths: Vec<&'static str>,
}

pub fn model_sources() -> ModelSourcesResponse {
    ModelSourcesResponse {
        runtimes: vec![
            ModelSource {
                id: "blue",
                name: "BlueTTS",
                version: BLUE_MODELS_TAG,
                size: "~275 MB",
                description: "Fast local multilingual speech with Hebrew, English, Spanish, German, and Italian.",
                files: vec![
                    ModelSourceFile {
                        name: "duration_predictor.onnx",
                        url: format!("{BLUE_MODEL_BASE_URL}/duration_predictor.onnx"),
                    },
                    ModelSourceFile {
                        name: "text_encoder.onnx",
                        url: format!("{BLUE_MODEL_BASE_URL}/text_encoder.onnx"),
                    },
                    ModelSourceFile {
                        name: "vector_estimator.onnx",
                        url: format!("{BLUE_MODEL_BASE_URL}/vector_estimator.onnx"),
                    },
                    ModelSourceFile {
                        name: "vocoder.onnx",
                        url: format!("{BLUE_MODEL_BASE_URL}/vocoder.onnx"),
                    },
                    ModelSourceFile {
                        name: "vocab.json",
                        url: format!("{BLUE_MODEL_BASE_URL}/vocab.json"),
                    },
                    ModelSourceFile {
                        name: "tts.json",
                        url: format!("{BLUE_MODEL_BASE_URL}/tts.json"),
                    },
                    ModelSourceFile {
                        name: "voices/female1.json",
                        url: format!("{BLUE_MODEL_BASE_URL}/voices/female1.json"),
                    },
                    ModelSourceFile {
                        name: "voices/male1.json",
                        url: format!("{BLUE_MODEL_BASE_URL}/voices/male1.json"),
                    },
                    ModelSourceFile {
                        name: "renikud.onnx",
                        url: RENIKUD_URL.into(),
                    },
                ],
                archive_url: None,
                directory: "blue-onnx-v2",
            },
        ],
        voices_url: "",
        default_paths: vec![
            "macOS: ~/Library/Application Support/com.maxmelichov.mamborambo/models",
            "Windows: %LOCALAPPDATA%\\com.maxmelichov.mamborambo\\models",
            "Linux: ~/.local/share/com.maxmelichov.mamborambo/models",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::model_sources;

    #[test]
    fn advertises_one_complete_blue_bundle() {
        let sources = model_sources();
        assert_eq!(sources.runtimes.len(), 1);
        let blue = &sources.runtimes[0];
        assert_eq!(blue.id, "blue");
        assert!(blue.files.iter().any(|file| file.name == "voices/female1.json"));
        assert!(blue.files.iter().any(|file| file.name == "voices/male1.json"));
        assert!(blue.files.iter().any(|file| file.name == "renikud.onnx"));
    }
}
