const TEMPLATE: &str = r#"# MamboRambo Local TTS API

You are using MamboRambo, a local BlueTTS HTTP API. The shipped runtime supports Hebrew and English, fixed voices, and streaming WAV output. It does not support voice cloning.

Base URL: {{base_url}}
OpenAPI schema: {{base_url}}/openapi.json
Swagger docs: {{base_url}}/docs
Model sources: {{base_url}}/v1/models/sources

Before calling the API, fetch the OpenAPI schema from /openapi.json and use it as the source of truth for request and response shapes.

Recommended flow:

1. Call GET /health.
2. If loaded=false, call GET /v1/models/sources to discover runtimes, model download URLs, and default MamboRambo Desktop model locations.
3. Check whether the model files already exist in MamboRambo Desktop's default model directory.
4. Call POST /v1/models/load with `runtime`, `model_path`, and `renikud_path`. The Blue runtime requires all three values.
5. Call POST /v1/audio/speech to synthesize speech.
6. Send JSON with input, optional voice, language, `response_format: "wav"`, and `stream: false`.
7. Save the returned WAV response to a .wav file. Streaming responses use MamboRambo binary frames.

Example:

~~~sh
curl {{base_url}}/health

curl {{base_url}}/v1/models/sources

curl -X POST {{base_url}}/v1/models/load \
  -H 'Content-Type: application/json' \
  -d '{"runtime":"blue","model_path":"/path/to/blue-onnx-v2","renikud_path":"/path/to/blue-onnx-v2/renikud.onnx"}'

curl -X POST {{base_url}}/v1/audio/speech \
  -H 'Content-Type: application/json' \
  -o speech.wav \
  -d '{"input":"שלום מממבו רמבו","language":"auto","voice":"Rotem","response_format":"wav","stream":false}'
~~~

If the API returns no_model, ask the user to install the MamboRambo model in the desktop app first.
"#;

pub fn render_skill(host: &str) -> String {
    TEMPLATE.replace("{{base_url}}", &format!("http://{host}"))
}
