#!/usr/bin/env bash
# Build the WebAssembly module for the web front-end (web/wasm).
#
# Usage: ./build-wasm.sh
#
# Requires: wasm-pack, and (optionally) wasm-opt on PATH for the size pass.
# wasm-pack's bundled wasm-opt download is skipped (see wasm/Cargo.toml); we run
# wasm-opt here instead so it works in networks that block the binaryen download.
set -euo pipefail
cd "$(dirname "$0")"

wasm-pack build wasm --target web --release --out-dir ../web/wasm
rm -f web/wasm/.gitignore   # keep the generated artifacts tracked

WASM=web/wasm/papercraft_wasm_bg.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  echo "wasm-opt -Os $(du -h "$WASM" | cut -f1) ..."
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int "$WASM" -o "$WASM.opt"
  mv "$WASM.opt" "$WASM"
  echo "  -> $(du -h "$WASM" | cut -f1)"
else
  echo "wasm-opt not found; skipping size pass (install binaryen for smaller output)."
fi
