> [!NOTE]
> Unofficial fan project, still work in progress, not affiliated with or endorsed by Rare, Microsoft or Nintendo. See
> the [disclaimer](#disclaimer).

<h3 align="center">Conker's Bad Fur Day: Multiplayer</h3>

<p align="center">
    The N64's most foul-mouthed multiplayer, rebuilt in Rust and playable in your browser.
    <br />
    <a href="https://conker.wtf"><strong>Play now »</strong></a>
    <br />
    <br />
    <a href="#introduction"><strong>Introduction</strong></a> ·
    <a href="#features"><strong>Features</strong></a> ·
    <a href="#tech-stack"><strong>Tech Stack</strong></a> ·
    <a href="#running-locally"><strong>Running Locally</strong></a> ·
    <a href="#whats-in-this-repo"><strong>Repo</strong></a> ·
    <a href="#credits"><strong>Credits</strong></a>
</p>

<br/>

<p align="center">
  <a href="https://conker.wtf">
    <img alt="Two Tediz in a Deathmatch" src="https://conker.wtf/assets/screenshots/modes/total-war.jpg">
  </a>
</p>

<br/>

## Introduction

This is a remake of the multiplayer of **Conker's Bad Fur Day** (Rare, Nintendo 64, 2001): an authoritative Rust
server and a Rust/WebAssembly client built on Bevy, sharing one fixed-tick simulation. The original's
four-player split screen becomes online matches, with the original's bots filling the empty slots.

The rules are measured, not guessed. A statically recompiled copy of the original game runs headless as an
**oracle**: scripted inputs go in, per-frame traces come out, and the remake is checked against them. Movement, the
camera, walls, the spawn picker and the game's RNG match it bit for bit on the recorded traces, and nearly every
combat event lands on the same frame.

**No game data lives in this repository.** Every model, texture, sound and table comes from your own ROM (Conker's Bad
Fur Day, USA: `conker-extract pack` exports it once into an editable asset pack that the server reads and serves to the browsers, so players never need a ROM.

## Features

- **Eight modes** from the original:
  - *Deathmatch*: free for all, grab a gun off the floor and frag everyone
  - *War: Total War*: Squirrels vs Tediz, carry the gas canister into their base (wear a mask)
  - *War: Colors*: nick their flag and bring it home
  - *War: Beach*: the villagers leg it up the beach while the Tediz man the guns
  - *War: Heist*: four gangs, one bag of cash
  - *Tank*: tanks, free for all, and a gas bomb on a timer
  - *War: Raptor*: cavemen vs raptors; raptors feed bodies to their baby, cavemen cook the eggs
  - *Race*: hoverboards, power-ups and missiles
- **Eight maps**: Arena, Colors, Beach, Vault, Tank, Bunker, Temple and the Race track
- **The original's bots**: its AI, at its five skill levels from *Inbred* to *Einstein*
- **Cheats**: the original's codes unlock its extra characters
- **Modern controls**: mouse aim and an over-the-shoulder aim camera; rebindable, keyboard and mouse or a gamepad,
  settings saved in the browser
- **Servers and lobbies**: create a server from the website (map, mode, bots, score, cheats), join it from the list
- **Netcode**: server-authoritative, client-side prediction, lag compensation by server-side rewind, WebTransport
  with a WebSocket fallback
- **The original's sound**: weapons, voices, maps' backgrounds and outros
- **Documented**: plans, status, verification and gaps in [`docs/`](./docs)

## Tech Stack

- [Rust](https://www.rust-lang.org/) – the simulation, the server, the client and the ROM extractor
- [Bevy](https://bevy.org/) – the client and server engine (WebGL2 in the browser)
- [lightyear](https://github.com/cBournhonesque/lightyear) – netcode over WebTransport and WebSocket
- [rodio](https://github.com/RustAudio/rodio) – sound, through Web Audio in the browser
- [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) and [binaryen](https://github.com/WebAssembly/binaryen)'s `wasm-opt` – the WebAssembly build
- [React](https://react.dev/), [Vite](https://vite.dev/), [TanStack Router](https://tanstack.com/router), [Tailwind CSS](https://tailwindcss.com/) and [Zustand](https://zustand.docs.pmnd.rs/) – the website
- [Bun](https://bun.sh/) – the web server that starts a game server per match, scripts and tooling
- [Caddy](https://caddyserver.com/) on a Linux VPS – hosting

## Running Locally

You need [Rust](https://rustup.rs/) (the version is pinned in `rust-toolchain.toml`) with
`rustup target add wasm32-unknown-unknown`, `cargo install wasm-bindgen-cli --version 0.2.129`, [Bun](https://bun.sh/),
optionally `wasm-opt`, and your ROM at `roms/game.z64` (gitignored).

```sh
bun run pack                    # ROM → assets/ (gitignored); --force re-exports over your edits
ln -s ../assets web/assets      # the page fetches the pack through this link
bun run build                   # native server and client, browser client, website
bun run serve                   # website and game servers on http://localhost:8080
```

`bun run pack` also folds in the maps' background recordings from `lab/audio/`. Without them, export the maps alone:
`CONKER_ROM=roms/game.z64 cargo run --release -p conker-extract -- pack --map 2d --map 30 --map 34 --map 3f --map 2b --map 1a --map 24 --map 33`.

### How to play

Open http://localhost:8080 (or https://conker.wtf), click **Create server**, pick a map, a mode and some bots, then
**Join game**. Open a second tab to play against yourself. **Esc** opens the menu: resume, settings (controls
included) and leave.

| Action | Keyboard and mouse | Gamepad |
|---|---|---|
| Move / walk | WASD or arrows / hold Alt | Left stick / halfway |
| Jump | Space | A |
| Strafe | Q, E | Right stick ← → |
| Draw / holster | B | X |
| Fire | Left click, F or Shift | RT, LT or LB |
| Aim / scope | Right click or R | RB |
| Grenade (hold to cook) | G | Right stick ↓ |
| Push-to-talk | Hold V | |
| Mute (sound and voices) | M | |

For one game server and the native client without the website: `bun run server -- --bots 3` and
`bun run client -- --side tediz`.

### Checks

```sh
bun run check                   # rustfmt, clippy -D warnings, website typecheck, lint and format
bun run test                    # every crate; ROM tests read roms/game.z64, oracle tests skip without lab/
```

## Deployment

[conker.wtf](https://conker.wtf) runs on one Linux VPS: Caddy serves the site and the asset pack and proxies the API
to the Bun web server ([`web/app/server.ts`](./web/app/server.ts)), which starts a `conker-server` per match on its
own ports. [`deploy/build.sh`](./deploy/build.sh) builds the site, [`deploy/pack.sh`](./deploy/pack.sh) uploads the
pack, and [`deploy/README.md`](./deploy/README.md) walks through the whole setup.

## What's in this repo

| Path | What |
|---|---|
| [`crates/`](./crates) | The Rust workspace (below) |
| [`web/`](./web) | The game page (`play.html`) and the browser build; [`web/app/`](./web/app) is the website and its dev server |
| [`deploy/`](./deploy) | Production build, pack upload, Caddy and systemd files |
| [`docs/`](./docs) | Plans, status, roadmap and research |
| `roms/` | Your ROM, `game.z64` (gitignored) |
| `assets/` | The asset pack exported from it (gitignored) |
| `lab/` | The oracle, research notes, captures and screenshots (gitignored, ROM-derived) |

| Crate | What |
|---|---|
| `extract-core` | ROM validation, asset tables, rzip, `.game` code and data, textures |
| `extract-gbi`, `extract-scene`, `extract-level`, `extract-character` | Display lists, placements, levels and collision, characters and clips |
| `conker-extract` | The extractor library and its CLI (`pack` writes the asset pack) |
| `conker-assets` | Reads the asset pack back |
| `conker-sim`, `conker-camera`, `conker-anim` | The shared fixed-tick simulation, the camera and the animation |
| `conker-rules`, `conker-bots` | Weapons, damage, modes and scoring; the original's bots |
| `conker-server` | The authoritative game server |
| `conker-client` | The browser (and native) client |

Library crates take bytes and return data (no file system, threads or processes), so they build for
`wasm32-unknown-unknown`.

Start with [`docs/RUST_REMAKE_PLAN.md`](./docs/RUST_REMAKE_PLAN.md) (the plan and its status per wave), then
[`docs/ROADMAP.md`](./docs/ROADMAP.md), [`docs/STATUS.md`](./docs/STATUS.md) (details and verification, newest first),
[`docs/GAPS.md`](./docs/GAPS.md) (what is still missing), [`docs/CONTROLS_RESEARCH.md`](./docs/CONTROLS_RESEARCH.md)
(the modern controls), [`web/README.md`](./web/README.md) (the browser client), [`web/DESIGN.md`](./web/DESIGN.md)
(the website's look) and [`deploy/README.md`](./deploy/README.md).

## Credits

- Conker's Bad Fur Day, its characters, artwork, sound and music: [Rare](https://www.rare.co.uk/)
- Asset formats: [conkers-bfd-decomp](https://github.com/DevOldSchool/conkers-bfd-decomp) by DevOldSchool (MIT, commit `e2400b27dda6`)
- The oracle: [CBFD-Recompiled](https://github.com/sciaschi/CBFD-Recompiled) by sciaschi (MIT), built on
  [N64Recomp](https://github.com/N64Recomp/N64Recomp), [N64ModernRuntime](https://github.com/N64Recomp/N64ModernRuntime),
  [RT64](https://github.com/rt64/rt64) and the [Conker decompilation](https://github.com/mkst/conker)
- Website fonts: [Luckiest Guy](https://fonts.google.com/specimen/Luckiest+Guy), [Black Ops One](https://fonts.google.com/specimen/Black+Ops+One)
  and [Gloria Hallelujah](https://fonts.google.com/specimen/Gloria+Hallelujah) (SIL OFL; Luckiest Guy: Apache 2.0)

## Disclaimer

This is an unofficial, non-profit fan project, made out of nostalgia for the N64's split-screen
nights. It is not affiliated with or endorsed by Rare, Microsoft or Nintendo. Conker's Bad Fur Day and all its
names, artwork, sound and screenshots are © 2001 Rare Ltd., used here as a tribute; Nintendo 64 is a trademark of
Nintendo. This repository holds no game data, and anything will be taken down at Rare's or Microsoft's request.

A project by [Luax](https://luax.dev/) · [@luax0 on X](https://x.com/luax0)
