#!/usr/bin/env bash
# KEEL build script: Linux, macOS, and Windows (Git Bash / MSYS2).
#   ./build.sh            build release binaries
#   ./build.sh test       run all tests (Rust + Python reference)
#   ./build.sh run [vehicle.xml] [--defs DIR]   build and start the KEEL app
#   KEEL_CORPUS=/path ./build.sh corpus   round-trip your own game files (never committed)
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust toolchain not found. Installing rustup (https://rustup.rs) ..."
  if [[ "${OS:-}" == "Windows_NT" ]]; then
    curl -sSfLo rustup-init.exe https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
    ./rustup-init.exe -y && rm -f rustup-init.exe
    echo "On Windows, also install the Visual Studio C++ Build Tools if the build reports a missing linker."
  else
    curl -sSf https://sh.rustup.rs | sh -s -- -y
  fi
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
fi

case "${1:-build}" in
  build)  cargo build --release
          echo "Built: target/release/keel$( [[ "${OS:-}" == Windows_NT ]] && echo .exe )" ;;
  run)    shift; cargo run --release -p keel-app -- "$@" ;;
  test)   cargo test
          if command -v python3 >/dev/null 2>&1; then python3 -m pytest -q tests/unit || true; fi ;;
  corpus) : "${KEEL_CORPUS:?set KEEL_CORPUS to a folder with your vehicle/definition XML and .mesh files}"
          cargo test --release --test roundtrip -- --ignored --nocapture ;;
  *) echo "usage: $0 [build|test|run|corpus]"; exit 2 ;;
esac
