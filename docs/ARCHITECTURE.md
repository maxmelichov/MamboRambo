# Architecture

MamboRambo is split into three layers: runtime crates, a Rust HTTP sidecar, and the Tauri desktop app.

## Runtime Crates

Runtime code lives under `crates/`.

- `crates/qwentts-rs` loads Qwen GGUF model files, runs tokenization, generation, codec decoding, and WAV writing.
- `crates/kokoro-rs` loads Kokoro ONNX model files, voices, phonemization data, and WAV writing.
- `crates/ggml-rs-sys` builds or links the ggml/gguf native layer used by Qwen.

The old top-level `runtimes/` tree is no longer part of the build.

## Server

`mamborambo-server/` is the local CLI and HTTP service. Its binary is named `mamborambo-server`.

Responsibilities:

- Provide `mamborambo-server speak` for command-line synthesis.
- Provide `mamborambo-server serve` for local HTTP usage.
- Own model loading, runtime selection, request validation, and response formatting.
- Expose the local API used by the desktop app.

The desktop starts `mamborambo-server` as a sidecar subprocess and reads its ready signal from stdout.

## Desktop

`mamborambo-desktop/` is the Tauri + React app.

Responsibilities:

- Own the user interface.
- Download and locate model bundles.
- Start and stop the `mamborambo-server` sidecar.
- Call the local HTTP API for model loading, language/voice metadata, and synthesis.

The desktop does not link directly to model runtimes.

## Models

Model files are stored outside git under the app data directory or under local ignored model directories during development.

Qwen bundles contain:

- `qwen3-tts-model.gguf`
- `qwen3-tts-codec.gguf`

Kokoro bundles contain:

- `kokoro-v1.0.onnx`
- `voices-v1.0.bin`
- `espeak-ng-data/`

## Release Flow

Typical release order:

1. Publish model bundles when model files change.
2. Build and release `mamborambo-server` sidecars with `mamborambo-server-v*` tags.
3. Build desktop packages that bundle the matching `mamborambo-server` sidecar.

Server release artifacts:

- `mamborambo-server-darwin-arm64.tar.gz`
- `mamborambo-server-linux-x64.tar.gz`
- `mamborambo-server-linux-arm64.tar.gz`
- `mamborambo-server-windows-x64.zip`
