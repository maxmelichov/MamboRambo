#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent

HOST_TRIPLE_MAP = {
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
    ("Linux", "aarch64"): "aarch64-unknown-linux-gnu",
    ("Windows", "AMD64"): "x86_64-pc-windows-msvc",
}

def detect_host_target() -> str | None:
    return HOST_TRIPLE_MAP.get((platform.system(), platform.machine()))


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the MamboRambo server sidecar for Tauri builds")
    parser.add_argument("--target", help="Rust target triple, for example x86_64-unknown-linux-gnu")
    parser.add_argument("--profile", default="release", choices=["debug", "release"])
    args = parser.parse_args()

    target = args.target or detect_host_target()
    if not target:
        print("Warning: could not detect host target; pass --target or place the server sidecar manually.")
        return 0

    is_windows = target.endswith("windows-msvc")
    sidecar_name = f"mamborambo-server-{target}" + (".exe" if is_windows else "")
    dest_dir = ROOT / "mamborambo-desktop" / "src-tauri" / "binaries"
    dest = dest_dir / sidecar_name
    profile_args = [] if args.profile == "debug" else ["--release"]
    cmd = [
        "cargo",
        "build",
        "-p",
        "mamborambo-server",
        "--bin",
        "mamborambo-server",
        "--target",
        target,
        *profile_args,
    ]
    print("+", " ".join(cmd))
    build_env = os.environ.copy()
    if platform.system() == "Darwin":
        build_env["RUSTFLAGS"] = f"{build_env.get('RUSTFLAGS', '')} -C link-arg=-Wl,-headerpad_max_install_names".strip()
    subprocess.run(cmd, cwd=ROOT, env=build_env, check=True)

    source = ROOT / "target" / target / args.profile / ("mamborambo-server.exe" if is_windows else "mamborambo-server")
    if not source.exists():
        raise FileNotFoundError(source)
    dest_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest)
    if not is_windows:
        dest.chmod(dest.stat().st_mode | 0o111)
    if platform.system() == "Darwin":
        ort_root = Path(os.environ.get("ORT_LIB_LOCATION", ROOT / "crates" / "blue-rs" / ".ort" / "onnxruntime-osx-arm64-1.23.2"))
        onnx_runtime = ort_root / "lib" / "libonnxruntime.1.23.2.dylib"
        if not onnx_runtime.exists():
            onnx_runtime = next(
                (path for path in (ROOT / "target").glob("**/libonnxruntime.1.23.2.dylib")),
                onnx_runtime,
            )
        if not onnx_runtime.exists():
            raise FileNotFoundError(
                f"{onnx_runtime} (set ORT_LIB_LOCATION to the ONNX Runtime distribution)"
            )
        shutil.copy2(onnx_runtime, dest_dir / onnx_runtime.name)
        for rpath in ("@loader_path", "@loader_path/../Resources"):
            subprocess.run(
                ["install_name_tool", "-add_rpath", rpath, str(dest)],
                check=True,
            )
    print(f"Installed MamboRambo server sidecar at {dest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
