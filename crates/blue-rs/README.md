# blue-rs

Rust ONNX inference for BlueTTS.

`BlueTts::create` is the low-level phoneme inference boundary. For normal
application input, use `BlueTts::synthesize_text` with a `Phonemizer`; it
applies BlueTTS text preparation, mixed Hebrew/Latin G2P, reference-code
pacing, and audio normalization before invoking the same ONNX pipeline.

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

## Text preparation and G2P

`Phonemizer::g2p(text, language)` is the standalone text-to-IPA function. It
prepares Hebrew structured text (dates, times, phone numbers, IDs, list
markers, punctuation and Hebrew/Latin boundaries) and emits per-language IPA
spans. `BlueTts::synthesize_text` composes that function with inference.

Plain Hebrew requires a Renikud model. Explicitly vocalized Hebrew (niqqud)
also requires the caller to attach a `NikudPhonemizer` implementation with
`Phonemizer::with_nikud_phonemizer`; the crate does not bundle Phonikud.

On macOS, `SDKROOT=...` may be needed for `espeak-rs-sys`/bindgen to find system
headers.
