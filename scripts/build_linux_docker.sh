#!/usr/bin/env bash
# Build MamboRambo desktop Linux bundles inside Docker (x86_64).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="ubuntu:22.04"
CONTAINER_NAME="mamborambo-linux-build"

docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true

docker run --name "$CONTAINER_NAME" -d \
  --platform linux/amd64 \
  -v "$ROOT:/workspace" \
  -w /workspace \
  "$IMAGE" \
  sleep infinity

cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker exec "$CONTAINER_NAME" bash -lc '
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
  build-essential curl wget file patchelf ca-certificates \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libgtk-3-dev libxdo-dev clang libclang-dev libasound2-dev \
  pkg-config xdg-utils

curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt-get install -y -qq nodejs
corepack enable
corepack prepare pnpm@10.15.1 --activate

curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"

curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add x86_64-unknown-linux-gnu

cd /workspace
uv run scripts/pre_build.py --target x86_64-unknown-linux-gnu
pnpm --dir mamborambo-desktop install
export APPIMAGE_EXTRACT_AND_RUN=1
export NO_STRIP=true
export ORT_STRATEGY=system
export ORT_LIB_LOCATION=/workspace/crates/blue-rs/.ort/onnxruntime-linux-x64-1.23.2
export ORT_PREFER_DYNAMIC_LINK=1
export LD_LIBRARY_PATH="$ORT_LIB_LOCATION/lib:${LD_LIBRARY_PATH:-}"
pnpm --dir mamborambo-desktop exec tauri build --target x86_64-unknown-linux-gnu
'

echo "Linux bundles should be under target/x86_64-unknown-linux-gnu/release/bundle/"
find "$ROOT/target/x86_64-unknown-linux-gnu/release/bundle" -type f \( -name "*.AppImage" -o -name "*.deb" -o -name "*.rpm" \) -print 2>/dev/null || true
