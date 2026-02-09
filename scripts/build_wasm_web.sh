#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM_CRATE_DIR="${ROOT_DIR}/wasm/clinote_wasm"
WASM_TARGET="${WASM_CRATE_DIR}/target/wasm32-unknown-unknown/release/clinote_wasm.wasm"
WASM_PKG_DIR="${WASM_CRATE_DIR}/pkg"
DOCS_PKG_DIR="${ROOT_DIR}/docs/pkg"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found."
  echo "install with: cargo install wasm-bindgen-cli"
  exit 1
fi

echo "== build wasm crate =="
cargo build \
  --manifest-path "${WASM_CRATE_DIR}/Cargo.toml" \
  --target wasm32-unknown-unknown \
  --release

echo "== run wasm-bindgen =="
wasm-bindgen \
  --target web \
  --out-dir "${WASM_PKG_DIR}" \
  "${WASM_TARGET}"

echo "== copy wasm assets into docs/ =="
mkdir -p "${DOCS_PKG_DIR}"
cp -f "${WASM_PKG_DIR}/"* "${DOCS_PKG_DIR}/"
cp -f "${WASM_CRATE_DIR}/web/wasm-test.html" "${ROOT_DIR}/docs/wasm-test.html"

echo "done: docs/pkg and docs/wasm-test.html are ready for GitHub Pages"
