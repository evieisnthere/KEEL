#!/usr/bin/env bash
# Build release archives into dist/: Linux x86_64 (glibc 2.31+ via cargo-zigbuild when available) and Windows x86_64
# (mingw cross-compile). Usage: scripts/package.sh
set -euo pipefail
cd "$(dirname "$0")/.."
VER=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
rm -rf dist && mkdir -p dist
docs="README.md CHANGELOG.md"

if command -v cargo-zigbuild >/dev/null 2>&1; then
  cargo zigbuild --release -p keel-app --target x86_64-unknown-linux-gnu.2.31
  LBIN=target/x86_64-unknown-linux-gnu/release/keel
else
  cargo build --release -p keel-app
  LBIN=target/release/keel
fi
L=dist/keel-$VER-linux-x86_64
mkdir -p "$L" && cp "$LBIN" "$L/keel" && strip "$L/keel" && cp $docs "$L/"
tar -C dist -czf "$L.tar.gz" "$(basename "$L")"

if rustup target list --installed | grep -q x86_64-pc-windows-gnu && command -v x86_64-w64-mingw32-gcc >/dev/null; then
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc cargo build --release -p keel-app --target x86_64-pc-windows-gnu
  W=dist/keel-$VER-windows-x86_64
  mkdir -p "$W" && cp target/x86_64-pc-windows-gnu/release/keel.exe "$W/" && x86_64-w64-mingw32-strip "$W/keel.exe" && cp $docs "$W/"
  (cd dist && zip -qr "$(basename "$W").zip" "$(basename "$W")")
fi
(cd dist && sha256sum ./*.tar.gz ./*.zip 2>/dev/null > SHA256SUMS)
ls -la dist
