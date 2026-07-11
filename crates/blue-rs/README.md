# blue-rs

Rust ONNX inference for BlueTTS.

The library expects phonemes at the inference boundary. Text phonemization is
available as a helper module and examples, but `BlueTts::create` itself does
not phonemize.

## Install

Add the crate directly from GitHub:

```bash
cargo add blue-rs --git https://github.com/thewh1teagle/blue-rs
```

## Models

Download the ONNX models from the blue-rs release:

```bash
wget https://github.com/thewh1teagle/blue-rs/releases/download/models-v1/blue-rs-onnx-models-int8.tar.gz
tar -xzf blue-rs-onnx-models-int8.tar.gz
```

This creates `./onnx_models`, which is what the examples use.

Download the default voices:

```bash
wget https://github.com/thewh1teagle/blue-rs/releases/download/models-v1/blue-rs-voices.tar.gz
tar -xzf blue-rs-voices.tar.gz
```

This creates `./voices`.

For the embedded example, `renikud.onnx` is included from this crate directory:

```bash
wget https://huggingface.co/thewh1teagle/renikud/resolve/main/model.onnx -O renikud.onnx
```

For zero-shot style extraction, download a reference clip:

```bash
wget https://github.com/thewh1teagle/phonikud-chatterbox/releases/download/asset-files-v1/male1.wav -O ref.wav
```

## Run

Basic phoneme-only inference:

```bash
SDKROOT=$(xcrun --show-sdk-path) cargo run --example basic
```

Self-contained text example with embedded ONNX model bytes:

```bash
SDKROOT=$(xcrun --show-sdk-path) cargo run --release --example embedded -- \
  --language he "שלום עולם" ../examples/out/embedded-he.wav
```

Supported `--language` values for the phonemizer helper are `he`, `en`, `es`,
`de`, and `it`.

On macOS, `SDKROOT=...` may be needed for `espeak-rs-sys`/bindgen to find system
headers.
