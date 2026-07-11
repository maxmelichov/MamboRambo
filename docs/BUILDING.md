# Building

MamboRambo is built from the Rust workspace and the Tauri desktop app. The desktop bundles a Rust sidecar named `mamborambo-server`.

## Server

Build the local HTTP server:

```console
cargo build -p mamborambo-server --release --bin mamborambo-server
```

For a specific Tauri sidecar target:

```console
uv run scripts/pre_build.py --target aarch64-apple-darwin
```

That command builds `mamborambo-server` with Cargo and copies the platform-suffixed binary into:

```text
mamborambo-desktop/src-tauri/binaries/
```

## Desktop

Install frontend dependencies:

```console
cd mamborambo-desktop
pnpm install
```

Run the app in development:

```console
pnpm tauri dev
```

Build a package:

```console
pnpm tauri build
```

## Models

Packaged model releases use `mamborambo-models-v*` tags. The Qwen bundle layout is:

```text
mamborambo-models-q5_0/
  qwen3-tts-model.gguf
  qwen3-tts-codec.gguf
  metadata.json
```

Kokoro bundles contain:

```text
mamborambo-kokoro-models-kokoro-v1.0/
  kokoro-v1.0.onnx
  voices-v1.0.bin
  espeak-ng-data/
  manifest.json
```

## Releases

Server sidecar releases use `mamborambo-server-v*` tags:

```console
git tag mamborambo-server-v0.1.0
git push origin mamborambo-server-v0.1.0
```

Manual release workflow:

```console
gh workflow run release-mamborambo-server.yml \
  --ref main \
  -f version=mamborambo-server-v0.1.0
```

## Checks

```console
cargo test --workspace
cargo build -p mamborambo-server --release --bin mamborambo-server
cd mamborambo-desktop
pnpm build
pnpm tauri build --debug
```
