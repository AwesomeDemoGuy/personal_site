#!/usr/bin/env bash
# Vendor the pretext library into a single self-contained ESM module at
# public/js/pretext.js, which the wasm-bindgen interop layer imports
# (see src/interop.rs).
#
# Why bundle? The npm package "@chenglou/pretext" ships its ESM entry
# (dist/layout.js) split across several files with relative imports
# (./bidi.js, ./analysis.js, ...). wasm-bindgen copies the single referenced
# module into its generated `snippets/` directory, so those relative imports
# would fail to resolve at runtime. esbuild inlines everything into one file.
#
# Usage: scripts/vendor-pretext.sh
# Requires: node + npm (npx) on PATH.

set -euo pipefail

PRETEXT_VERSION="${PRETEXT_VERSION:-0.0.8}"
ESBUILD_VERSION="${ESBUILD_VERSION:-0.25.0}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_FILE="${ROOT_DIR}/public/js/pretext.js"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

echo "Vendoring @chenglou/pretext@${PRETEXT_VERSION} -> ${OUT_FILE}"

cd "${WORK_DIR}"
npm install --no-save --no-audit --no-fund \
  "@chenglou/pretext@${PRETEXT_VERSION}"

npx --yes "esbuild@${ESBUILD_VERSION}" \
  "node_modules/@chenglou/pretext/dist/layout.js" \
  --bundle \
  --format=esm \
  --platform=browser \
  --legal-comments=none \
  --outfile="${OUT_FILE}"

echo "Wrote $(wc -c < "${OUT_FILE}") bytes to ${OUT_FILE}"
