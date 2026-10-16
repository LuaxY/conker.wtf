# Conker's Bad Fur Day multiplayer: Rust remake

An authoritative Rust server and a Rust/WebAssembly browser client for a remake of Conker's Bad Fur Day
multiplayer, sharing one simulation crate. See `docs/RUST_REMAKE_PLAN.md` for the plan.

**No ROM data is committed here.** The tracked tree is only code. Every asset comes from the operator's own ROM (Conker's
Bad Fur Day, USA, SHA-1 `4cbadd3c4e0729dec46af64ad018050eada4f47a`): `conker-extract pack` exports the asset pack
to `assets` (gitignored, like `roms/` and `lab/`), which the server reads and the page fetches (`web/README.md`). Never commit a
ROM or anything derived from one.

## Crates
- `extract-core`: ROM validation, asset tables, rzip, `.game` code/data, texture blob
- `extract-gbi`, `extract-scene`, `extract-level`, `extract-character`: payload decoders
- `conker-extract`: the extractor library and its CLI (`pack` writes the asset pack)
- `conker-assets`: reads the asset pack's data files back
- `conker-sim`: the shared fixed-tick simulation
- `conker-server`: the authoritative server
- `conker-client`: the browser (and native) client

Library crates take byte slices and return data (no file system, threads or processes) so they build
for `wasm32-unknown-unknown`.

## Build and test
```sh
cargo build --workspace
CONKER_ROM="$PWD/roms/game.z64" cargo test --workspace   # ROM tests skip without CONKER_ROM
cargo check -p extract-core --target wasm32-unknown-unknown
cargo run -p conker-extract -- --rom roms/game.z64 list
```

Shortcuts in `package.json` (run from the repo root with bun; the ROM defaults to
`roms/game.z64`, or set `CONKER_ROM`): `bun run build` (native server and client, the
browser client, the landing), `bun run serve` (the dev site on :8080 that runs and kills game servers), `bun run
start` (both), `bun run server -- --bots 3` and `bun run client -- --side tediz` (one game server and the native
client, extra flags passed on), `bun run pack -- --force` (re-export `assets`), `bun run test`, `bun run check`.

Ported formats credit DevOldSchool/conkers-bfd-decomp (MIT, commit `e2400b27dda6`) and CBFD-Recompiled (MIT)
in the modules that use them.
