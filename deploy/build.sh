#!/bin/sh
# The production bundle: the size-optimised client (web/build.sh web: [profile.web], wasm-bindgen,
# wasm-opt -Oz) and the landing (web/app, bun), then a site folder for the static server:
#   index.html, play.html, play.js, voice.js   revalidated on every load (Cache-Control: no-cache)
#   landing/*                                  the landing's scripts, styles and fonts, named by content
#   v/<hash>/client.js, client_bg.wasm         named by their content: cached for a year
# and every text file also as .br (brotli -q 11) and .gz (gzip -9), which Caddy's `precompressed`
# serves (not the fonts: woff2 is compressed already).
# Usage: [WASM_OPT=/path/to/wasm-opt] [SITE=dir] sh deploy/build.sh   (default SITE: target-net/site)
# Needs: what web/build.sh needs, wasm-opt (binaryen), brotli, gzip, bun.
set -e
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-target-net}
wasm_opt=${WASM_OPT:-$(command -v wasm-opt || true)}
[ -x "$wasm_opt" ] || { echo "needs wasm-opt (binaryen): put it on PATH or set WASM_OPT" >&2; exit 1; }
command -v brotli > /dev/null || { echo "needs brotli" >&2; exit 1; }
command -v bun > /dev/null || { echo "needs bun" >&2; exit 1; }
WASM_OPT=$wasm_opt sh web/build.sh web
(cd web/app && bun install --frozen-lockfile && bun run build)   # -> web/index.html, web/landing/
site=${SITE:-$CARGO_TARGET_DIR/site}
hash=$(cat web/dist/client.js web/dist/client_bg.wasm | { sha256sum 2> /dev/null || shasum -a 256; } | cut -c1-12)
rm -rf "$site"
mkdir -p "$site/v/$hash"
cp web/dist/client.js web/dist/client_bg.wasm "$site/v/$hash/"
cp -R web/index.html web/play.html web/voice.js web/landing "$site/"
# client.js finds client_bg.wasm next to itself (import.meta.url), and play.js fetches it next to
# client.js's path: every "./dist/client.js" moves (the import and the wasm's fetch).
sed "s|\"./dist/client.js\"|\"./v/$hash/client.js\"|g" web/play.js > "$site/play.js"
grep -q "\"./v/$hash/client.js\"" "$site/play.js" || { echo "play.js: import not rewritten" >&2; exit 1; }
! grep -q '"./dist/client.js"' "$site/play.js" || { echo "play.js: a dist/ path left" >&2; exit 1; }
find "$site" -type f \( -name '*.html' -o -name '*.js' -o -name '*.css' -o -name '*.wasm' \) | while read -r f; do
  brotli -q 11 -f "$f"
  gzip -9 -k -f "$f"
done
ls -lR "$site"
