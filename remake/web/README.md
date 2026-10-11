# Browser client

`index.html` is the landing (Home, the server browser and the lobby: "Landing" below, built from `app/`).
`play.html` + `play.js`, the game page, load the wasm client from `dist/` (`client.js` + `client_bg.wasm`, built from
`crates/conker-client`, WebGL2) and start it at once. Nobody picks or uploads a ROM: the client fetches the asset
pack (`conker-extract pack`: scenes 0x2D and 0x30, character 117 and the held weapons as glTF + PNG, the game data as
JSON) from `assets/` next to the page, from the site that serves the game. Besides the page and the pack, it fetches only
the server's `/info` (which map it plays), its connect token (and, in dev, the certificate digest).

## Landing

The site has two pages: `/` (`index.html`), the landing, and `/play.html`, the game. The landing is a Vite + React +
TypeScript app in `app/`: Tailwind CSS, TanStack Router (hash URLs, so any static server serves every screen without a
fallback), Zustand (your name, side and connection, kept in `localStorage`), Motion for the fade-ins, lucide-react icons,
and its two fonts self-hosted with @fontsource (Luckiest Guy, Black Ops One). Build it with bun:

```sh
cd remake/web/app
bun install
bun run build    # -> web/index.html + web/landing/ (gitignored), next to play.html, dist/ and assets/
bun run check    # typecheck (tsc) + lint (oxlint with the react and jsx-a11y rules) + format check (oxfmt)
bun run dev      # Vite on :5173 for UI work; play.html, play.js, dist/ and assets/ come from the static server on :8080
```

Vite warns that its `outDir` is the app's parent folder: on purpose (`emptyOutDir: false`; `bun run build` clears
`landing/` first).

| URL | Screen |
|---|---|
| `/#/` | Home: the pitch, "Join game", and the controls (keyboard and mouse, gamepad), grouped as move, combat and camera. On a phone it says that playing needs a keyboard or a gamepad |
| `/#/servers` | The server browser: name, region, map, mode, players, ping, status (open, full, in game); search, sort (ping, players, name), refresh. "This machine" stays on top; a full server can't be opened |
| `/#/lobby/ID` | The lobby: the server's name, address, map, mode, region, players and ping; the players in two columns (Squirrels, the good guys; Tediz, the bad guys) with ready, ping and the host's badge; your name, side and connection (WebTransport or WebSocket); a ready toggle; a chat; "Join game" |

**Mock and real.** `app/src/data.ts` holds the data: `listServers()` and `getLobby(id)`, async functions returning mock
data, shaped for a server-list endpoint and a per-server lobby endpoint. The other servers, the lobby's players, ready
states, pings and chat are mock, and nothing is sent anywhere. One entry is real: "This machine", the game server on this
page's host at its default port 5888 (`localhost:5888` in dev, `DOMAIN:5888` in production), so Home → Join game → This
machine → Join game reaches a match.

**URL contract.**
- The lobby's "Join game" opens `play.html?server=HOST:PORT&transport=wt|ws&side=squirrels|tediz` (URL-encoded:
  `server=localhost%3A5888`; the client decodes it). The game page hands its URL to the client unchanged, and the client
  rejects any option it does not know (table below). `side` is new: until the client knows it, the game page says
  `Could not start: unknown option side`.
- The landing's own `?server=HOST:PORT`, before the `#`, moves "This machine", e.g.
  `http://localhost:8080/?server=localhost:5971#/servers` for a server started with `--port 5971`.
- Your name and the ready toggle stay in the landing: the client has no option for them yet.

**Imagery.** Three of the map's textures, fetched at run time from `assets/` (the pack served next to the page; nothing is
copied into this tree): the Squirrels' "S" banner (`0172`), the Tediz' lightning badge (`0605`) and the brick wall behind
the title (`0113`), drawn with big pixels. Without the pack the page shows lucide icons (a squirrel, a panda) and a plain
background instead.

**Tested in Chrome 153** (2026-10-10, the built landing on `http.server`, the debug server with `--bots 2 --port 5971`):
Home → Join game → This machine → Join game opened `play.html?server=localhost%3A5971&transport=wt&side=tediz`, which the
current `dist/` refuses (`unknown option side`); the same address without `side` connected over WebTransport (HUD up),
and `transport=ws` over WebSocket, also through Caddy with `deploy/build.sh`'s site layout (cache headers checked). Search,
sort, refresh, the side picker (arrow keys), ready, chat, a bad lobby id (back to the list), an unknown route and the
missing pack all behaved; no console errors or warnings on any screen; no horizontal scroll at 390 px. Screenshots, desktop
(1440 px) and phone (390 px): `../lab/screens/landing/` (outside this tree).

## Build

Needs `rustup target add wasm32-unknown-unknown`, `wasm-bindgen-cli` 0.2.129 (the crate's version), and optionally
binaryen's `wasm-opt`.

```sh
cd remake
CARGO_TARGET_DIR=target-net WASM_OPT=/path/to/wasm-opt sh web/build.sh web   # size-optimised: [profile.web]
CARGO_TARGET_DIR=target-net sh web/build.sh                                  # plain release, faster to link
```

`web` builds with `[profile.web]` (opt-level s, fat LTO, strip), runs wasm-bindgen, then `wasm-opt -Oz` if
`WASM_OPT` is set or `wasm-opt` is on `PATH`. Sizes, 2026-10-08:

| wasm | Raw | gzip -9 |
|---|---|---|
| rustc output (`conker-client.wasm`) | 29,421,031 B | 8,100,970 B |
| after wasm-bindgen (`client_bg.wasm`) | 26,457,609 B | 6,741,779 B |
| after `wasm-opt -Oz` (18 s, 1.9 GB RAM) | 21,436,566 B | 6,738,744 B |
| shipped `dist/` (same, after the last client fixes) | 21,437,158 B | 6,738,427 B |
| 2026-10-09, combat with a Bevy text HUD | 26,523,726 B | 8,263,738 B |
| 2026-10-09, shipped: the HUD is the page's `#hud` element (no `bevy_ui`/`bevy_text` in wasm) | 21,597,566 B | 6,795,030 B |
| 2026-10-10, wave 4 (protocol 6): rustc output | 29,763,364 B | 8,227,365 B |
| 2026-10-10, after wasm-bindgen (the netcode agent's `dist/`, no wasm-opt) | 26,777,291 B | 6,855,410 B |
| 2026-10-10, shipped: after `wasm-opt -Oz` (`lab/tools/emsdk/upstream/bin/wasm-opt`), with the fixes below | 21,723,068 B | 6,849,029 B |
| 2026-10-10, wave 5 (protocol 7, connection agent's build; the shared tree at 16:29): after `wasm-opt -Oz` | 21,795,600 B | 6,877,402 B |
| 2026-10-10, wave 5 browser test (the fixes below): rustc output | 29,854,842 B | 8,255,365 B |
| the same, after wasm-bindgen | 26,865,094 B | 6,882,173 B |
| the same, shipped: after `wasm-opt -Oz` (3 min 13 s for the whole `build.sh web` after a client change) | 21,795,958 B | 6,877,620 B |
| 2026-10-10, wave 6 (protocol 8, `../deploy/build.sh`): rustc output; brotli -q 11: 4,453,068 B | 29,854,604 B | 8,255,371 B |
| the same, shipped (`web/dist` and the site's `v/178d7d3a45cd/`, identical bytes); **brotli -q 11: 4,344,934 B** | 21,796,051 B | 6,879,989 B |
| 2026-10-10, asset pack (no extractor in the client; `../deploy/build.sh`, site `v/33b8955a8554/`); **brotli -q 11: 4,314,879 B** | 21,702,709 B | 6,837,597 B |
| 2026-10-11, sound (rodio + cpal's Web Audio; `web/build.sh web`, rustc output 30,090,587 B); **brotli -q 11: 4,375,125 B** (+57,472 on the sides build's 4,317,653) | 21,948,071 B | 6,928,500 B |

`client.js` is 123 kB (17 kB gzip, 14 kB brotli). The `web` build takes about 10 minutes after a Bevy feature change, about 3.5
minutes (wasm-opt included) after a change in the client alone; `../deploy/build.sh` adds the brotli and gzip copies
(4 min 8 s in all in wave 6). wasm-opt mostly removes LEB128 padding, which gzip already squeezes out, so the
download barely changes. Serve the wasm compressed: brotli is 37% smaller than gzip here, and the production setup
below serves it.

## Run locally

Once, export the asset pack from your ROM (into `../assets`, next to this tree: it is ROM-derived and stays out
of it; `conker-extract` refuses to overwrite a pack, so your edits are safe) and link it next to the page
(`web/assets` is gitignored):

```sh
cd remake
CONKER_ROM="/path/to/Conker's Bad Fur Day (USA).z64" cargo run --release -p conker-extract -- pack
ln -s ../../assets web/assets
(cd web/app && bun install && bun run build)   # the landing (above)
```

Then two processes, from `remake/`:

```sh
target-net/debug/conker-server --assets ../assets --digest-file web/dist/digest.txt   # ../assets: the default
python3 -m http.server 8080 --bind :: --directory web   # serve web/, not web/dist/ (:: serves IPv4 and IPv6)
```

Open <http://localhost:8080/> for the landing (Join game, This machine, Join game) or
<http://localhost:8080/play.html> for the game page, which starts by itself, with a status line while the pack loads. The client
first asks the game server which map it plays: `GET /info` next to the token endpoint below (`http://localhost:5889/info`,
`{"map":"2d","protocol":11}`; the server's `--map 2d|30`, default `2d`; a client of another protocol stops there and says
to reload). Then it fetches that map's and the pack's 13 data files from `assets/` (all at once), then the sound's two (`audio/sounds.json`,
`audio/events.json`, when `pack.json` lists `audio`), and Bevy its glTF models with their `.bin` files and PNG
textures, and after the first key press or click the WAVs, from the same folder (no `.meta` requests: `AssetMetaCheck::Never`). Once the level
and the character have loaded, the client fetches its connect token from the game server (`http://localhost:5889/token`,
the server's host and its port + 1; the server signs tokens with a key it makes at each start, or
`--token-key FILE` / `CONKER_TOKEN_KEY`, and a client can no longer make its own): a token lasts 60 s, so it is
fetched just before the client connects. The page reads the WebTransport certificate hash from `dist/digest.txt`
and passes it to the client without keeping it in the address. The server
makes a new self-signed certificate (valid 14 days) on every start, so reload the page after restarting the server:
the reload reads the new hash (before wave 5's browser test, the page wrote the hash into its address, so a reload kept
the old one and failed over WebTransport until the `digest=` was removed by hand). A `?digest=` typed in the address
is used as it is. The file is stale once the server exits, so delete it then. For a second player, open a second window, not a second tab of the same window. A
background tab may be throttled and then dropped by the server (I).

The line at the bottom left is the connection's status (also logged as `connection: ...`): `loading the level`,
`connecting to HOST:PORT over WebTransport…`, `connected to ...`, `could not connect to ...: REASON. HINT` or
`disconnected from ...: REASON. Reload to reconnect`. The HUD stays off until the player is in the match, and again
once the connection is gone. The page connects once the level and the character have
loaded and ten frames in a row took under 0.25 s, so the load's stalls can't trip the 3 s netcode timeout. Reloading
the page reconnects.

`http.server` sends no `Cache-Control`, so after a rebuild or an edit of the pack a plain reload may run the old
`client_bg.wasm`, a page or pack files (V: Chrome kept the old help text). Reload bypassing the cache
(Shift+Cmd+R), or check the size of `dist/client_bg.wasm` under `performance.getEntriesByType('resource')`. The
server reads the pack when it starts: restart it after an edit too, since the server and the pages must read the
same pack.

The game page's URL options (`play.html?a=1&b=2`):

| Option | Meaning |
|---|---|
| `transport=wt` / `ws` | WebTransport (default) or WebSocket |
| `server=host:port` | game server, default this page's host on port 5888. The server listens on IPv4 and IPv6 (`[::]`), so `localhost`, `127.0.0.1` and `[::1]` all work |
| `digest=HEX` | SHA-256 of the server certificate, for WebTransport (default: the contents of `dist/digest.txt`; none on an https page, whose server has a trusted certificate) |
| `token-url=URL` | where the client gets its connect token, once the level has loaded (default: `/token` on this page's origin over https, where the static server proxies it; over http the game server's host, its port + 1, `/token`) |
| `id=N` | netcode client id (default random) |
| `map=2d` / `30` | the map to load instead of asking the server's `/info` (it must be the server's) |
| `side=squirrels` / `tediz` | the side to play on: the squirrels (the SHC soldier) or the Tediz (the Tedi), each with its own model; default: the server puts you on the side with fewer players (the squirrels on a tie). Deathmatch scoring stays per player |
| `script` | drive the player with a test pattern instead of the keyboard |
| `lag=MS` | add MS to every packet in both directions (a round trip takes 2·MS more), to test the netcode |
| `log=DIRECTIVES` | more log filters (`EnvFilter` syntax), e.g. `log=conker_client=debug` for the `STATE` (the predicted player, every tick) and `SEEN` lines (where the screen showed each other player when it took that tick's input). The browser has no `RUST_LOG`; natively `RUST_LOG` wins |

Controls (the N64 buttons the game reads; the server runs the match).

The HUD is the original's, drawn from the pack's `hud/` over the view (the same code natively and here: textured
quads under a second camera, no UI or font renderer in the wasm build), in the original's 292 × 216 frame scaled to
the window by its height, each part keeping its distance from the window's edge or centre as the original's does from
its viewport's (`../STATUS.md`, "HUD"):
- top left, health as chocolate: one piece per point, up to 6 in two rows;
- top centre, your kills and your place, `0 (1st)`;
- top right, the held weapon's icon (drawn or put away), or while a grenade is in your hand one icon per grenade
  left; under it the radar: the other living players within about 4,300 units as red dots, turned with the view, with
  a black bar above or below a dot on a higher or lower level;
- bottom centre, the clock;
- while aiming (R) the twin guns, the rifle, 0x14 or the flamethrower: the reticle; through a scope: the green ring,
  its dots and the zoom bar;
- dead: the screen goes black but for a closing circle, until the respawn.

The original shows no ammo count and no countdown, so neither does this HUD. H shows the old text readout too (health,
weapon, drawn or not and rounds, grenades, kills, the clock, the respawn countdown) in the page's `#hud` element
(natively, as Bevy text).

| N64 | Keyboard / mouse | Gamepad | What it does (Deathmatch) |
|---|---|---|---|
| stick | WASD or arrows (hold Alt/Option to walk); the mouse while R is held and no key moves | left stick | move, camera-relative. While R aims a gun (twin guns, rifle, flamethrower, a grenade in hand) the stick's x twists the upper body, up to about 49°, then turns the body. Through a scope (sniper rifle, 0x14) it moves the look |
| A | Space | South (A) | jump; reloads a drawn gun |
| B | B | West (X) | draw or put away the weapon you picked up |
| Z | left mouse button, F, Shift | either trigger, left bumper | fire a drawn weapon (through a scope, along the look); a melee swing locks onto the nearest player within 200; crouch when unarmed |
| R | right mouse button, R | right bumper | aim: the zoom camera, the gun's twist, the scope of the sniper rifle and of 0x14. The rocket launcher and the scoped revolver (0x23) do not fire while it is held |
| C-down | G | right stick down | throw a grenade; hold to cook it (held too long it goes off in the hand) |
| C-left / C-right | Q / E | right stick left / right | strafe, standing or with the stick held, armed or not |

The twist, the scope's look and the strafe with the stick held are wave 4's: they ride the stick, R and the C
buttons the table already had. The mouse moves the stick only while R is held (2 stick units per pixel the mouse moves
in a frame), so a keyboard player can aim finer than the keys' full stick.

C toggles the free debug camera (its own keys fly it). M mutes and unmutes the sound. Ctrl is not used: browsers keep Ctrl+W. The right mouse
button opens no context menu: winit cancels `contextmenu` on the canvas (V: Chrome 153, R reaches the server).
Weapons come from the level's pickups: walk over one (it disappears for 4 s), then press B to draw it.

The client reads the gamepad only while its window has focus (in the browser: the canvas). Before wave 5 one Xbox
pad moved the players of two windows at once (V, DevTools-driven Chrome, which reports every page focused, so the
fix is untested with a real pad there).

The console logs, per page:
- `asset pack fetched and client started in N ms` (the page: the data files, the sound's two, then `start()`)
- `maps/2d/level.gltf#Scene0 loaded N s after the app started`, and the same for `characters/117/character.gltf#Scene0`
- `connection: ...` on every status change (above), and `player Netcode(N) left` when another player leaves
- once a second, each player's position, state, clip, clip time and health, and the rollback count
- `HUD` lines: the HUD's text whenever it changes (about once a second, with the clock)
- `predicted respawn on record N: the camera cuts` on the tick the client predicts its respawn (the server's
  respawn tick), or `respawned at ...: the camera cuts` when the server's count says so first (a lift ride, a
  respawn the client did not predict)
- `sound: output open, 2 channels at N Hz` after the first key press or click, then `sound: output running` once
  the AudioContext plays, and `sound muted` / `sound on` (M)

**Sound** (`crates/conker-client/src/audio.rs`; what plays and why: `../../lab/audio/README.md`). The original's match
sound from the pack's `audio/`: footsteps by floor, the characters' voices and clip sounds, the weapons (loops, shots,
cases, hits, blasts, the grenade's hiss, flight, landings and fuse), pickups, the map's two bubbling spots and the
background noise (`audio/maps/2d/background.wav`, a recording of the game's own output, looped for the whole match).
Positional sounds fade linearly from `near` to `far` around your player and pan with the follow camera; your own play
centred. Browsers start audio only after a user gesture, so the page says "Press a key or click for sound" (top
right) until the first key press or click on the game, which opens the output; the WAVs (13 MB, 7.5 MB of it the
background) then load in the background, and each sound plays once its file is in. M mutes ("Sound muted (M)").
`log=conker_client::audio=debug` adds a `SOUND t=TICK read=READ id=ID p=OWNER vol=V gain=G pan=P` line per sound
started (and `HIT` lines: the weapon a hit was put down to). A pack without `audio` in `pack.json` plays silent.

## Dev and production setups

The same pages (the landing's `index.html` and `landing/`, `play.html`, `play.js`) and client run in both. What differs:

| | Dev (above) | Production (`../deploy/README.md`) |
|---|---|---|
| Bundle | `web/build.sh` into `web/dist/` (`client.js`, `client_bg.wasm`); `bun run build` in `web/app` into `web/index.html` + `web/landing/` | `deploy/build.sh` into `target-net/site/`: `index.html` and `landing/` (the landing), `play.html`, `play.js` (its import rewritten), the client in `v/<content hash>/`, every text file also as `.br` and `.gz` |
| Asset pack (`/assets/`) | `web/assets`, a symlink to `../assets` (gitignored) | `deploy/pack.sh` uploads a copy with `.br` and `.gz` files to a host folder outside the site; Caddy serves it at `/assets/` (`handle_path`), `no-cache` with an ETag (the pack is editable); the server reads the same folder |
| Static server | `python3 -m http.server`: no compression (21.8 MB per load), no cache headers | Caddy (`deploy/Caddyfile`): brotli or gzip as the browser asks (4.3 MB), `v/*` and `landing/*` cached a year as `immutable`, the pages and `play.js` `no-cache` (revalidated) |
| Page | `http://localhost` | `https://DOMAIN` |
| Connect token (fetched once the assets loaded), and `/info` (first) | `http://<server host>:<port + 1>/token` and `/info`, the game server's own endpoint (on `[::]`) | `/token` and `/info` on the page's own origin, which Caddy proxies to the game server's endpoint on loopback |
| Token key | a new random key at each server start | `--token-key FILE`, and `--public` names the addresses the tokens carry (the server checks them) |
| Password | none | optional: the Caddyfile's `basic_auth` block (commented out) asks for one for the whole site |
| WebTransport certificate | self-signed for 14 days, pinned by its digest (`dist/digest.txt`, or `?digest=`) | the domain's Let's Encrypt certificate, checked as any site's: no digest |
| WebSocket | `ws://` | `wss://` |

**A production-like run on one machine** (wave 6's test): the server in its production shape and the Caddyfile's own
site block, over plain http, since no browser here trusts the test certificate of `../lab/deploy-test/`:

```sh
sh deploy/pack.sh /tmp/conker-pack          # the pack as the host gets it: with .br and .gz copies
target-net/debug/conker-server --assets /tmp/conker-pack --port 5955 --token-key /path/to/token.key \
  --token-addr 127.0.0.1:5956 --public 127.0.0.1 --public ::1 --digest-file /tmp/digest.txt
sed -e '/^[[:space:]]*tls /d' -e 's/auto_https disable_certs/auto_https off/' deploy/Caddyfile > /tmp/Caddyfile
DOMAIN=http://localhost:8955 SITE=$PWD/target-net/site PACK=/tmp/conker-pack TOKEN_ADDR=127.0.0.1:5956 HTTP_PORT=8955 \
  XDG_DATA_HOME=/tmp/caddy XDG_CONFIG_HOME=/tmp/caddy CADDY_ADMIN=localhost:2955 \
  caddy run --config /tmp/Caddyfile --adapter caddyfile   # its files in /tmp/caddy, its admin API off 2019
```

Open `http://localhost:8955/play.html?server=localhost:5955&token-url=/token&digest=$(cat /tmp/digest.txt)` (and
`...&transport=ws` without the digest). Over http the page would fetch its token from port + 1 itself and read
`dist/digest.txt`, which the site does not have: `token-url=/token` sends it through Caddy as an https page does, and
`digest=` stands in for the trusted certificate. `../lab/screens/wave6/browser/run.sh` (outside this tree) starts both
setups with the test's bots and places.

## Bots

`--bots N` puts the original's multiplayer AI (conker-bots) on slots 0..N-1, on the game's start records for those
slots, from the server's start. Humans take the slots after them, so with `--bots 2` the first page is slot 2 and the
second slot 3: `--spawn` and `--give` count slots, so pass four `--spawn`s (the bots' first) to place two humans. A
bot shows on every page as any other player, interpolated, named `Local(N)` in the console lines (`Local(0)` is
slot 0). The bots pick up weapons, ride the lifts, hunt the humans and each other, die and respawn.

## Match settings and the end

The server's operator picks the original's SET-UP options (defaults: the original's):

| Flag | Values (default) | What |
|---|---|---|
| `--time` | `3`-`10` minutes, `unlimited` (`10`) | the clock; untimed, no clock is drawn and the lives count |
| `--lives` | `1`-`5` (`3`) | untimed only (timed gives 99): a player out of lives stays down; the last one standing wins |
| `--kills` | `0`-`10`, `0` = `unlimited` (`0`) | the first player to reach it ends the match |
| `--skill` | `inbred`, `crap`, `normal`, `bastard`, `einstein` or `0`-`4` (`normal`) | the bots' skill |
| `--radar` | `on`, `off` (`on`) | the HUD's radar |

E.g. `conker-server --bots 3 --time unlimited --lives 2 --kills 5 --skill bastard --radar off`. The match ends as the
original's does: the clock reaching 0, a player at the kill limit, one player left in play (or no human) once someone
is out of lives. Untimed, the HUD shows the lives left left of the health instead of the clock, and a player out of
lives a skull. After the end fade every page shows the results (STATS: a row per human with place, accuracy, kills,
headshots and deaths) over the frozen level, with RESTART and QUIT (a push of the stick or A/D left or right toggles,
the game's A = Space or the pad's South button chooses). The next match, with the same settings, starts
once every human in it chose RESTART, or 40 s after the end. QUIT leaves: the browser goes back to the landing page
(`index.html`), the native client exits. With the last human gone, a finished match waits for the next one to join.

## Tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-08)

- WebGL2 through ANGLE/Metal: the level and characters render.
- Bevy logs WARN lines for plugins that need compute or storage buffers (OIT, SSAO, atmosphere, motion vectors). They
  are expected on WebGL2 and harmless.
- No audio is built in, so there is no AudioContext and no autoplay prompt.
- ROM to running client: 1.8 s, including with a second client running. It took 3.3-4.7 s while a cargo build was
  using the CPU. The GLBs finish loading 2.1-2.5 s after the app starts.
- WebTransport (certificate hash, default server from the page host) and WebSocket both connect.
- Two pages, one per transport: each sees the other player move and animate (idle, run, skid, wall push, creep,
  crouch, jump) with 0 rollbacks. Screenshots: `lab/screens/wave2/browser-*.png` (outside this tree).
- The automated Chrome reported both tabs visible and focused, so background-tab throttling was not exercised.

## Test combat in two windows

The server can place both players face to face, 143 units apart, and give player 1 the twin guns. These are the
positions of the oracle's e_w09 encounter at read 745:

```sh
target-net/debug/conker-server --digest-file web/dist/digest.txt \
  --spawn -123.602036,-293.800781,112.003021,25345 --spawn -219.113525,-290.902344,5.3149147,57088 \
  --wait 2 --margin 150 --give 0:30:09
```

1. Open <http://localhost:8080/play.html> in one window. It joins first, so it is player 1 (slot 0).
2. Open <http://localhost:8080/play.html?transport=ws> in a second window: player 2.
3. The match starts 5 s after player 2 joins, with both players back on their places. Player 1's HUD then shows
   the twin guns' icon (top right).
4. After the 5 s spawn immunity, in player 1's window press B (the text readout, H: `twin guns (drawn) 60 rounds`).
   Then hold the left mouse button or F.
5. Expected:
   - Yellow bullets (placeholder spheres) fly.
   - Player 2 loses a chocolate piece per hit, every 20 ticks. With none left the body lies on the ground and, 63
     ticks later, the screen closes to a black circle (H: `DEAD - respawn in N s`).
   - Player 1's score reads `1 (1st)`.
   - 151 ticks (5 s) after the kill, player 2 is back with 6 pieces on another start record.
   - Both clocks count down together.
6. G throws a grenade (about 1 s wind-up, then about 4.4 s to the blast). Its outer blast (radius 500) costs 2 HP and
   knocks the victim about 400 units away, out of the line of fire.

Each restart of the server needs both pages reloaded (a new certificate, and `--wait` starts the match only once).

Driving it from Chrome DevTools: synthetic events on the canvas work, because winit reads `code`. For example,
`canvas.dispatchEvent(new KeyboardEvent('keydown', {code: 'KeyF', bubbles: true}))`, then the matching `keyup` later.
A key must stay down across a frame: a keydown and keyup in the same frame never reads as held (I: Bevy's
`ButtonInput`). The text readout (H) is DOM text, so DevTools `wait_for "DEAD"` can time screenshots. The server log
(`RUST_LOG=info,conker_server=debug`) has the `EVENT` lines (hits, explosions, respawns).

## Combat tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-09)

Setup: the server above, page 1 over WebTransport, page 2 over WebSocket. Screenshots are in
`lab/screens/wave3/browser-*.png`, outside this tree.

- Player 1 drew the twin guns (B) and held F. The server logged 6 hits 20 ticks apart (t=1145 to 1245, the last one
  lethal), then the respawn on record 6 at t=1396.
- Page 1 showed `kills 1`, 26 rounds left, and bullets in flight. Page 2 showed `HP 0/6 ... DEAD - respawn in 0.3 s`
  with the body on the ground, then `HP 6/6` on record 6. Both clocks ran together.
- The grenade (G) wound up from t=1326 and was thrown at t=1355, taking the count from 3 to 2. It exploded at t=1488,
  433 units from player 2: `HP 4/6`, and player 2 was knocked away. Page 2's own grenade showed in flight on page 2.
- The right mouse button sends R (buttons 0x10 on the server) and opens no context menu.
- Console: only the four WebGL2 WARN lines listed above, no errors. `HUD` log lines fell to about 1 per second; before
  this fix the client wrote one every tick.
- Server: no errors in the final runs. One earlier run logged `server_late_input_mismatch` for both browser clients at
  once, while two more pages were starting in the same Chrome (transient).
- Screenshot capture through DevTools lags up to about 3 s behind the call.
- The white quad at the top left of player 1's view at record 1 is also in the native client (`duel-p1.png`), so it
  is not a web problem.

## Test wave 4 in two windows, with bots and lag

The server places both humans face to face (e_w09's places, as above) after two bots and hands each a weapon at
read 30, a few ticks after the match starts (`--wait 2` starts it 10 s after the second page joins):

```sh
target-net/debug/conker-server --digest-file web/dist/digest.txt \
  --bots 2 --wait 2 --margin 300 --spawn -2283,-521,6017 --spawn 2005,-521,-6199 \
  --spawn -123.602036,-293.800781,112.003021,25345 --spawn -219.113525,-290.902344,5.3149147,57088 \
  --give 2:30:09 --give 3:30:09     # 15 flamethrower, 16 rocket launcher, 14 throwing knives, 24 sniper rifle, 22 chainsaw
```

Open `http://localhost:8080/play.html?lag=150&log=conker_client=debug` (slot 2, the first to join) and
`http://localhost:8080/play.html?transport=ws&lag=150&log=conker_client=debug` (slot 3) in two windows. `RUST_LOG=info,conker_server=debug` on the server logs its `STATE`, `LAGCOMP` (every bullet tested: where its
shooter saw the targets) and `EVENT` lines.

Driving the pages from DevTools (what the test below did):
- Keys: `canvas.dispatchEvent(new KeyboardEvent('keydown', {code: 'KeyF', bubbles: true}))`, the `keyup` later. Two
  pages fire at once when both schedule their events (`setTimeout`) for the same `Date.now()`.
- Mouse buttons: `PointerEvent('pointerdown' / 'pointerup', {button: 2, buttons: 2, pointerType: 'mouse'})` on the
  canvas (2 = right = R).
- Mouse motion (it aims while R is held): winit reads `pointermove` on `window`, and in Chrome only its coalesced
  events, so a synthetic one needs them: `new PointerEvent('pointermove', {...init, coalescedEvents: [new
  PointerEvent('pointermove', init)]})` with `init = {movementX: 10, movementY: -5, buttons: 2, pointerType: 'mouse'}`
  (V: stick (20, 10) on the server).
- Logs: wrap `console.log` before the page starts (DevTools' `initScript`) and keep the `STATE` / `SEEN` lines (and,
  since wave 5, `DRAWN` every frame and `CORRECTION`) in an array instead of the console (about 180 lines a second);
  Bevy's line is `%cLEVEL%c file:line%c message`, all through `console.log` (a Rust `warn!` is not a console warning).
  Wave 5's hook posts them to a small static server instead (`lab/screens/wave5/scripts/hook.js` and `serve.py`,
  outside this tree; the `initScript` only applies to the navigation it is passed with, so pass it on each reload).
  `lab/screens/wave4/browser_compare.py SERVER_LOG PAGE_LOG...` (outside this tree) compares them with the server's:
  prediction errors, and each shooter's rewound targets against what its page showed.
- Foreign input: count trusted (`event.isTrusted`) `keydown` / `pointerdown` / `pointermove` events and
  `gamepadconnected`: none in these runs.

## Wave 4 tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-10)

Seven server runs, both pages at `lag=150` (300 ms added to each round trip), two bots in every run, then a smoke run
of the final bundle without lag. Screenshots are in `lab/screens/wave4/browser-*.png`, and the page and server logs
in `lab/screens/wave4/browser-logs/`, outside this tree.
- **Mutual fire with lag:** both players fired the twin guns at once, one strafing across the other's line. Both were
  hit on the same tick (2082), then at 2105 and 2128. Every bullet test used the page's own report (`Saw`, 306 of
  306). The rewound targets equal where the shooter's page drew them: 918 comparisons (the other human and both
  bots), max 0.000000. The first run (no logs) also hit both ways, with 552 of 552 tests on the page's `Saw`.
- **Victim's page:** a hit is the server's decision, so the victim's page mispredicts until the correction arrives,
  the client's lead later: 12-13 ticks at 150 ms each way, then the position snaps. Seen: 16-26 units standing, 110
  strafing, 153 in the flamethrower's burning run, 318 in a bot's knock-back. Outside those windows prediction is
  exact. Respawns are predicted exactly: on the respawn tick and record the server picked (records 8, 16 and 10).
- **Fixed: the camera cut after a respawn came 13 ticks late.** The predicted body was at its new record on the
  server's tick, but the camera cut only when the server's `Combat.lives` arrived, so for about 0.4 s the view stayed
  at the death spot, looking across the map (`browser-rocket-dead-p1-wt.png`). The client now cuts on its own
  predicted respawn, to the record's camera, as the server does. After the fix: cuts at t=2388 on record 16 and
  t=2248 on record 10, the server's respawn ticks and records. Native e_w09 at 150 ms: cut at 929 on record 6, the
  server's; its other results are unchanged.
- **Weapons:**
  - Flamethrower: two hits, then the victim's burning run.
  - Rocket launcher (0x16): one rocket from 143 units killed the standing target (7 damage) and cost its shooter 2.
    A rocket fired as the flame hit its shooter flew about 35° off the target and hit a wall.
  - Sniper rifle (0x24): R (right mouse) opened the scope, the mouse moved the look, and F killed the target (7).
  - Scoped gun (0x14): with R held, F fired its aimed shot (animation 0x12b) and killed the standing target (7).
  - Grenades: a quick throw went off 132 ticks after leaving the hand; held 2 s (cooked), 76.
- **Controls:**
  - The mouse moved the stick while R was held: (20, 10) for 10 and −5 pixels a frame, and it turned the aim camera.
  - W with E or Q sent C-right or C-left with the stick held.
  - The blade was drawn with B.
- **Bots:** they moved, picked up weapons (7-15 pickups a run), rode the lifts (3 rides in the last run) and fought:
  - They killed the humans 8 times: one-hit blade kills (7 damage), and once three hits of 2.
  - They killed each other 3 times in one run: bot 0 killed bot 1 once, and bot 1 killed bot 0 twice with
    explosions. Each respawned. In the same run bot 1 set off 10 explosions one every ~124 ticks around slot 3's room
    (x ≈ −900, z 5,870-6,750), one of which hit it.
  - A bot killed itself with its own explosion (respawned on record 4).
  - No human killed a bot: in the run where both humans held the blade and swung whenever a bot came within 190 units,
    the bots shot from farther away.
- **Console:** 0 errors and 0 warnings besides the WebGL2 WARN lines above, on both pages in every run. The server
  logged no ERROR or WARN. No foreign input (keys, pointer, gamepad) reached the pages in runs 2-6, where the hook
  counted it.
- **Screenshots:** capture through DevTools still lags 1-3 s behind the call.
- **Not drawn:** flames and weapons in hand. Bullets, grenades and rockets are placeholder spheres. The white quad
  near record 1 is still there.

## Connection tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-10, wave 5)

Plain release bundle (`build.sh`), the debug server on port 5890 with its own digest file, `http.server --bind ::`.
Screenshots in `lab/screens/wave5/browser-*.png`, outside this tree.
- **Dual-stack:** `http://localhost:8090/?server=localhost:5890` over WebTransport (Chrome resolves `localhost` to
  `::1`; it hung before), `http://[::1]:8090/?transport=ws&server=[::1]:5890` and `http://127.0.0.1:8090/` over
  WebSocket all connected. The level and the character loaded 0.63 and 1.04 s after the start, then the page connected.
- **Closing a page:** the slot is freed 4-13 ms after `pagehide` (three closes, both transports): winit's exit drops
  the session, which closes the transport (server log `left: Link failed: Disconnected by peer: (dropped)`). The
  other page logged `player Netcode(N) left`. A page cannot send netcode's own disconnect: no frame runs after
  `pagehide`.
- **Status line:** `connected to ...`; with the server killed, `disconnected from [::1]:5890: the connection
  dropped (WebSocket code 1006). Reload to reconnect` (over WebTransport: `connection lost: Connection lost`); reloaded with the server down, `could not connect ...
  over WebSocket: connection lost. Is the server running?`; over WebTransport with the server down or with the
  previous server's digest, `could not connect ... over WebTransport: failed to await connection: Opening handshake
  failed. Is the server running? A restarted server has a new certificate: ...`. After restarting the server, a
  reload connected again.

## Wave 5 tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-10, browser test)

The `build.sh web` bundle (sizes above), the debug server on port 5895 with `--bots 2 --wait 2 --margin 300` and the
four `--spawn`s of "Test wave 4 in two windows", page 1 over WebTransport (`id=1`), page 2 over WebSocket (`id=2`),
both `lag=150&log=conker_client=debug`. Screenshots `lab/screens/wave5/browser-{before,bug,p1,p2,p3,fixed}-*.png`,
logs in `lab/screens/wave5/browser/` (outside this tree; gunzip `run1/` and `run2/` for
`lab/screens/wave5/smooth_report.py`).

What changed in this test (rebuilt and re-tested):
- **Reload after a server restart (`index.js`):** the page wrote the certificate digest into its address, so a reload
  kept the old one and failed over WebTransport (`Opening handshake failed`), although the status line says to reload.
  It now reads `dist/digest.txt` when the ROM is picked. After the fix, a reload after the server restart connected,
  and a page opened before the server started connected once the ROM was picked after it (no reload).
- **`#hud` (`render.rs`):** it said `connecting...` beside a failed or lost connection; in the browser it now stays
  empty without a player (the `#net` line has the status). The native HUD still says `connecting...`.
- **WebTransport's reason (`connection.rs`, `tidy`):** `connection lost: Connection lost` now reads `connection lost`
  (a cause that repeats the one before is dropped). Re-tested: the server killed under a WebTransport and a WebSocket
  page of the new bundle, `disconnected from localhost:5895: connection lost. Reload to reconnect` and the WebSocket
  line below, with no HUD text on either page.

Results:
- **Status line:** opened before the server, `could not connect to localhost:5895 over WebTransport: failed to await
  connection: Opening handshake failed. Is the server running? ...` and, over WebSocket, `could not connect ...:
  connection lost. Is the server running?` (Chrome adds its own console error, `WebSocket connection to ... failed`).
  Server killed mid-match (SIGKILL): the WebSocket page showed `disconnected from localhost:5895: the connection
  dropped (WebSocket code 1006). Reload to reconnect` 20 ms later, the WebTransport page `disconnected ...: connection
  lost ...` 52 ms later (I: Chrome learns it from localhost's closed port; from a remote server it may take the
  3 s timeout).
- **Leaving:** closing a page freed its slot 29 ms (WebSocket) and 38 ms (WebTransport) after its `pagehide`, then 24
  and 12 ms in a second run. The other page dropped the player 0.34 and 0.38 s after the close (150 ms of lag
  included). Rejoining with the same id took a slot at once: page 2 its own freed slot 3; page 1 slot 3 too, since
  its leaving had moved page 2 down to slot 2. That move cost page 2 one 14-tick rollback (the slot shift of
  `Match::remove_player`), which moved nothing on screen.
- **Hits on the victim's page** (`smooth_report.py`; before: the predicted body, drawn until wave 5; after: now):
  - Twin guns, 5 hits on page 2: each corrected by 26.48 units 12 ticks after the hit. Beyond its own motion the
    player moved 26.48 → 3.31 a tick (6.62 in the one 54 ms frame that ran two ticks; 1 of 2,912 frames took over
    25 ms), look-at 26.47 → 6.62, eye 6.42 → 4.02, facing 1,025 → 257 game units. Converged 8 ticks after each
    correction (27 once, when the next hit's misprediction began first).
  - Grenade knock-back (page 1 unarmed, e_gren's throw, 2 damage): corrected by 334.6 units 13 ticks after the blast,
    then blended away over 17 ticks at 19.7 a tick. The drawn body moved 16-35 a tick with its blast flight, against
    334.65 in one frame before; look-at 135 → 30, eye 46 → 27, facing 2,170 → 128, view yaw at most 3.4° off.
  - After each of a bot's melee kills (7 damage), the victim's page took a rollback nearly every tick for 23-45 ticks
    (0.1-58 units) while the bot stood 41-46 units from the body (the known pushing misprediction against a moving
    player), until it walked away. Drawn per frame: 58.5 → 8.8.
  - The local player moves once a tick (30 Hz), so at 60 fps every other frame shows no move (as before wave 5).
  - Prediction otherwise as in wave 4: page 1 3,572 ticks, 1 over 0.001 (0.039); page 2 61 over 0.001 (5 hits × 12),
    0 state misses. Lag compensation: 108 bullet tests, all on the page's `Saw`, rewound target vs the page 324
    comparisons, max 0.000000. A lethal grenade moved nothing; its respawn was predicted on the server's tick and
    record (6488, record 6).
- **Held weapons, feet, HUD names:** both pages show the twin guns in both hands of their own and the other player's
  model, feet on the floor (`browser-p1-wt-twin-guns-drawn.png`, `browser-p2-ws-twin-guns-drawn.png`). Page 2, given
  0x39, 0x14, 0x23, 0x21 and 0x22 in turn, showed `rifle`, `throwing knives`, `scoped revolver` (`6 rounds`),
  `katana` and `chainsaw`. The knives and the revolver were not visible from the follow camera (behind the body, or
  not drawn: not checked from the front). Page 1 only held the twin guns (an unchanged name). The wave-4
  notes above use the old names: pistol (0x39), scoped gun (0x14), one-shot gun (0x23), melee (0x21), blade (0x22).
- **Console:** Chrome's console had no errors or warnings besides its own WebSocket error while the server was down.
  The Rust log had only the four WebGL2 WARN lines per load and the disconnect lines; the server logged no ERROR or
  WARN in 3 runs. No foreign input (trusted key or pointer events, gamepads) reached the pages. Both pages ran at
  60 fps (median frame 17 ms), each in its own tab.

## Wave 6 tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-10, browser test)

The DevTools-driven browser reports itself as Google Chrome 153; it is Comet 153.0.8010.222 (Chromium). Bundle:
`deploy/build.sh` (sizes above). Each run: the debug server with `--bots 2 --wait 2 --margin 300` and the four
`--spawn`s of "Test wave 4 in two windows", two pages at `lag=150&log=conker_client=debug`, each in its own window
(isolated contexts), the first to join (slot 2) throwing a grenade (a G tap 2-3 s into the match) at the second (slot
3, 143 units ahead), then closing, rejoining and closing again. Logs, screenshots and the scripts are in
`lab/screens/wave6/` (outside this tree; `browser/run.sh` starts the processes, `browser/hook.js` is the DevTools
init script).

| Run | Setup | Thrower | Victim |
|---|---|---|---|
| dev1 | dev: `serve.py` (static, no-store) on 8950, server on 5950 | WebTransport | WebSocket |
| prod1 | production-like (above): Caddy on 8955, server on 5955 | WebTransport | WebSocket |
| prod2 | the same | WebSocket | WebTransport |

Results:
- **Connect token:** each page fetched one when its ROM was picked, and the server logged `token issued to client N`
  for the same id. Dev: `GET http://localhost:5951/token?id=…` 200 (cross-origin, `Access-Control-Allow-Origin: *`,
  `Cache-Control: no-store`, 4,096 hex digits). Production-like: `GET http://localhost:8955/token?id=…` 200 with
  `via: 1.1 Caddy`. All pages connected over their transport; the address kept no token.
- **Compression and caching** (production-like, from the network panel): Chrome sent `Accept-Encoding: gzip, deflate,
  br, zstd` (also over http to `localhost`), and Caddy answered with brotli: `client_bg.wasm` 4,344,934 B,
  `application/wasm`, `Cache-Control: public, max-age=31536000, immutable`, `Vary: Accept-Encoding`; `client.js`
  14,287 B, immutable; `/` 807 B and `index.js` 1,567 B, `no-cache`. On a reload `/` and `index.js` were revalidated
  (300 B each, 304) and `client.js` and the wasm came from the cache without a request.
- **The other player's grenade is predicted on the victim's page.** Blast and knock-back (2 damage) at t=2244, 2418
  and 1686: on the victim's page no rollback and no correction at the blast, and 0 ticks over 0.001 against the server
  through the blast and the flight (wave 5 in Chrome: corrected by 334.6 units 13 ticks after the blast). The throw
  itself costs the victim one 13-tick rollback when it arrives (t≈2160, 2340, 1590), which moves nothing: no
  `CORRECTION` line.
- **Leaving costs the others no rollback.** Five closes (dev 2, production-like 3) freed the slot 11-44 ms after
  the `pagehide` beacon reached the static server (once 95 ms before it), and the remaining page's rollback count did
  not change (watched 6-31 s, until a bot reached it or the page closed). A page reopened after a close took the
  freed slot 2 (on the server's `--spawn` for slot 2), again with no rollback on the other page (wave 5: a 14-tick
  rollback for the slot shift).
- **Prediction otherwise:** the throwers: 925, 1,729 and 1,929 ticks, 1 over 0.001 each (0.039, the match-start tick).
  Every other tick over 0.001 is a bot's server-decided hit: melee kills (43 and 37 ticks; the known pushing case) and
  other hits (10-16 ticks each, the client's lead at 150 ms). Every page took one 13-14-tick rollback at the match
  start (`--wait` puts everyone back on their places), which moved nothing unless a bot had hit it before the start:
  the bots roam and shoot from the server's start.
- **Token endpoint down** (game server stopped, Caddy up): the form says `could not get a connect token from
  http://localhost:8955/token: HTTP 502. Is the server running?`, and Chrome logs its own `Failed to load resource:
  502`. A page still running says `disconnected from localhost:5955: connection lost. Reload to reconnect`.
- **Console:** 0 errors and 0 warnings in Chrome's console on the 8 game pages, besides that 502. The Rust log had only
  the four WebGL2 WARN lines per load; the servers logged no ERROR or WARN. No foreign input (trusted key or pointer
  events, gamepads) reached the pages.
- **Fixed: the ROM picker sat at the left edge** of a wide window (the form's grid stretched its track to the help
  text, and the file input kept to its start): `#rom` now centres its items (`justify-items: center`).
- **Seen once, not reproduced:** a DevTools reload of a disconnected page started its navigation 10 s late, and the
  DevTools calls to that page then timed out for minutes; the new document itself loaded in 0.1 s. Three more
  reloads (from the page and from DevTools, connected and disconnected) started 52-110 ms after the call (I: the
  browser, which someone was using at the time, or the DevTools link).
- **After the fix** (rebuilt with `deploy/build.sh`: the client's folder keeps its hash, only `index.html` changed),
  the dev flow exactly as "Run locally" gives it (`python3 -m http.server`, `--digest-file web/dist/digest.txt`)
  connected a WebTransport and a WebSocket page, with no console errors.
- **Not tested:** an https page (WebTransport without a digest, `wss://`, the same-origin `/token` over https): no
  browser here trusts the test certificate. The wasm client then passes an empty `serverCertificateHashes` list
  (lightyear 0.30.1 always sets it): Chromium treats an empty list as no pinning (I); check it on the host.

Screenshots (`lab/screens/wave6/`): `browser-prod-p2-ws-before-blast-{1,2}.png`, `browser-prod-p2-ws-blast-flight.png`
(the victim in the air, `HP 4/6`) and `browser-prod2-p2-wt-{before-blast,after-blast,after-blast-2}.png` (the
WebTransport victim before and after its knock-back); `browser-prod-p1-wt-thrower.png` (`grenades 2`; the white quad
top left); `browser-{dev,prod}-p1-wt-rejoined-slot2.png`, `browser-prod-p2-ws-after-rejoin.png`,
`browser-dev-p2-ws-after-blast-and-leave.png`; `browser-prod-token-502-server-down.png`,
`browser-prod2-p2-wt-server-stopped.png`; `browser-form-centered-after-fix.png`, `browser-dev-readme-flow-ws.png`.

## Asset pack tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-10)

The `deploy/build.sh` bundle (sizes above), two pages per setup, each in its own isolated window, both in the same
match with two bots. Screenshots in `lab/screens/pack/`, logs and the scripts (`prod.sh`) in `lab/screens/pack/browser/`
(outside this tree).
- **Dev** (`python3 -m http.server`, `web/assets` linked, the server with `--bots 2`, e_w09's places and the twin guns
  for both humans): neither page picked anything. Each fetched 204 pack files (12 data files, 3 glTF, 3 `.bin`, 186
  PNGs; 14,148,973 B), **no `.meta` request**, then its token once the level and the character were in (WebTransport
  page: token at 1,362 ms after the page load, level 0.70 s and character 0.96 s after the app started).
  Page load to playing (the HUD up): **2.1 s over WebTransport, 1.8 s over WebSocket** (pack data in and `start()`
  done 0.47-0.48 s; connected 1.73 / 1.47 s). B drew the twin guns on both pages: each shows its own player and the
  other one holding them in both hands (`browser-dev-p{1-wt,2-ws}-twin-guns.png`), and the other player running
  with them (`browser-dev-p2-ws-sees-p1-run-2.png`). Console: no errors or warnings; the Rust log has only the four
  WebGL2 WARN lines (no missing animation).
- **Production-like** (`deploy/pack.sh` into a folder outside this tree, Caddy over plain http with the Caddyfile's
  `/assets/` block, the server with `--token-key` and `--public`, `token-url=/token`): the same 204 files as brotli,
  **3,359,854 B on the wire** (`character.bin` 7,154,832 → 2,657,557, `character.gltf` 5,552,863 → 137,321),
  `Cache-Control: no-cache` with an ETag; the wasm 4,314,879 B as brotli. Page load to playing: 2.1 s (WebTransport),
  2.0 s (WebSocket). A reload revalidated all 204 files (304, 61,200 B in all) and took the wasm from the cache:
  playing 2.2 s after the reload.
- **Password** (the Caddyfile's `basic_auth` on): with the browser's cached credentials (as after its password
  prompt) the page loaded all 204 files (200), got its token through `/token` (200) and played 2.1 s after the load;
  the same fetch without credentials got 401. A `http://user:password@host/` address does **not** work: Chrome then
  refuses every fetch relative to the page (`Request cannot be constructed from a URL that includes credentials`);
  type the password in the browser's prompt instead.
- **Load order:** the level, then the character, then the held weapons, as before. With the pack's separate files a
  debug native client loading the level and the character at once did not overflow (3 runs), and was no faster
  (character loaded 2.15-2.36 s after the start, against 2.22-2.61 s in turn), so the order stays.

## Sound tested in Chrome 153 (macOS, Apple M2 Max, 2026-10-11)

`play.html?server=localhost:6830&transport=ws` on `http.server` (port 6840), the release server with 2 bots, the
bundle with sound. Test-only init scripts (DevTools) recorded every `AudioContext` the page made and tapped an analyser
on its destination. Before any key: no AudioContext, the hint "Press a key or click for sound" up, the client's
`SOUND` log already running (the rules run without a device). Key press at 17,071 ms: `sound: output open, 2 channels
at 44100 Hz` at 17,075 ms, `sound: output running` at 17,195 ms, the context `running` (base latency 5.8 ms), the hint
gone. M: "Sound muted (M)", the destination's RMS 0.05-0.12 (peak 0.62-0.79) with sound, 1.9e-30 muted, back on the
second M. The console had no error or warning messages. Screenshots: `../../lab/screens/audio/browser-hint.png`,
`browser-muted.png`.

## Manual test in Firefox and Safari

For each browser, using the same server and static server as above:
1. Open `http://localhost:8080/play.html`. It starts once the wasm is compiled and the pack's data files are in.
2. Console:
   - Note the start time.
   - Check for errors: only the WebGL2 WARN lines above are expected.
   - `AdapterInfo` names the GPU backend.
3. Check that the level is drawn, with your character from behind at spawn record 1, and that it runs, walks (Alt) and
   jumps when you press the keys.
4. WebTransport: wait for `connected over WebTransport`.
5. WebSocket: open `http://localhost:8080/play.html?transport=ws` in a second window and wait for `connected over WebSocket`.
6. Move in each window and check that the other window shows that player moving and animating. In the console, the
   `remote/interpolated` lines should show its position and clip changing.
7. Reload one window. The old player disappears at once (the closed page's transport closes; the netcode timeout,
   3 s, is the fallback), and the reloaded page joins as a new player.

Known limits per browser (from `lab/research/rust-stack.md` and `web-platform.md`):

| | Firefox | Safari |
|---|---|---|
| WebTransport | 114+ | 26.4+; none in 26.0-26.3 and in Lockdown Mode, and it bypasses proxies |
| `serverCertificateHashes` | 125+ | 26.4+ |
| Notes | lightyear had Firefox WebTransport certificate rejections before (lightyear #251). If WT fails, use `?transport=ws` and report it. | Alt is Option. JavaScriptCore never fully optimises wasm functions over 50,000 bytes, and its JIT pool is shared (512 MB on Apple silicon), so the 21 MB module may run slower than in Chrome. |

Limits in every browser:
- **Failed WebTransport connections look alike:** Chrome reports a stopped server and a stale certificate digest with
  the same `Opening handshake failed`, so the status line names both causes. An attempt that never ends (blocked UDP)
  fails after 10 s. A missing digest is named as such.
- **Dual-stack:** the server binds `[::]`, which accepts IPv4 too on macOS and Linux (their default); Windows would
  need `IPV6_V6ONLY` off. The native client's WebTransport binds IPv4 only (lightyear), so native clients reach the
  server over IPv4 with WebTransport, or over either with WebSocket.
- **Secure context (spec, not tested here):** WebTransport needs a secure context. `http://localhost` and `http://127.0.0.1` are secure
  contexts; a LAN address such as `http://192.168.1.20:8080` is not, so use `?transport=ws` there.
- **Mixed content (spec, not tested here):** plain `ws://` is blocked from an `https://` page. An https page
  uses `wss://` and checks the WebTransport certificate as any site's (no digest): deploying with a real
  certificate is `../deploy/README.md` (wave 6).
- **Background tabs (I, untested):** browsers throttle hidden tabs, and a client that stops sending for 3 s is
  dropped. Keep each player in its own visible window.
