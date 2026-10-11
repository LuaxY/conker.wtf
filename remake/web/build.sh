#!/bin/sh
# Builds the browser client (WebGL2) into web/dist/: client.js + client_bg.wasm.
# Usage: web/build.sh [release|web]   ("web" = the workspace's size-optimised profile)
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli 0.2.129 (= the crate version).
# Optional: wasm-opt (binaryen; on PATH or WASM_OPT=path) shrinks the web build with -Oz.
set -e
cd "$(dirname "$0")/.."
profile=${1:-release}
cargo build -p conker-client --target wasm32-unknown-unknown --profile "$profile"
wasm-bindgen --target web --no-typescript --out-name client --out-dir web/dist \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/$profile/conker-client.wasm"
wasm_opt=${WASM_OPT:-$(command -v wasm-opt || true)}
if [ "$profile" = web ] && [ -n "$wasm_opt" ]; then
  "$wasm_opt" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-reference-types --enable-multivalue \
    web/dist/client_bg.wasm -o web/dist/client_bg.wasm
fi
ls -l web/dist
