#!/bin/sh
# Uploads the asset pack (conker-extract pack; default assets, gitignored) for Caddy to
# serve at /assets/ and the game server to read (deploy/README.md): a copy in a temporary folder
# outside this tree, with a .br (brotli -q 11) and a .gz (gzip -9) copy of every JSON, glTF, .bin
# and WAV file for Caddy's `precompressed` (the background WAV goes as is, 4.2 MB brotli: an OGG
# would need a decoder in the wasm and an encoder here), rsynced to DEST with --delete. Caddy serves
# a .br as long as it exists, even when the file beside it is newer: edit the pack on your machine
# and run this again, never edit it on the host.
# Usage: sh deploy/pack.sh DEST [PACK]   (DEST: an rsync destination, e.g. vps:/srv/conker/assets;
#        local paths are relative to the repo root)
# Needs: brotli, gzip, rsync.
set -e
cd "$(dirname "$0")/.."
dest=${1:?usage: sh deploy/pack.sh DEST [PACK]}
pack=${2:-assets}
[ -f "$pack/pack.json" ] || { echo "no asset pack in $pack: cargo run --release -p conker-extract -- pack" >&2; exit 1; }
command -v brotli > /dev/null || { echo "needs brotli" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
chmod 755 "$tmp" # rsync -a gives DEST its mode: Caddy and the game server read it
cp -R "$pack/." "$tmp/"
find "$tmp" -type f \( -name '*.json' -o -name '*.gltf' -o -name '*.bin' -o -name '*.wav' \) | while read -r f; do
  brotli -q 11 -f "$f"
  gzip -9 -k -f "$f"
done
rsync -a --delete "$tmp/" "$dest/"
echo "pack: $(find "$tmp" -type f | wc -l | tr -d ' ') files ($(du -sh "$tmp" | cut -f1)) -> $dest"
