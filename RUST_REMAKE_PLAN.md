# Conker's Bad Fur Day multiplayer: Rust remake (authoritative server + browser client)

Plan v4, 2026-10-08. Supersedes `WEB_PORT_PLAN.md` (v3) except its oracle sections.
Status: **wave 1 done** (2026-10-08, workflow `wf_24c6f5ed-57a`, 9 agents). Details and the ranked wave-2 list:
`remake/STATUS.md`. In short (V, re-run here: 81/81 extractor tests pass):
- `remake/` workspace (9 crates, ~13.6k lines Rust, no ROM data). Extractor reproduces DevOldSchool's numbers on this ROM
  (models, textures, 98,479 collision faces, 3,518 joints, 2,621 clips); `conker-extract slice` writes scene 0x2D
  placements, 20 MP spawn records, collision (4,299 faces), level glTF, and character 117 (player 1's model in the
  oracle's Deathmatch: 28 joints, 764 clips) to `lab/extract/slice/`.
- Oracle: scripted input (`CONKER_INPUT_SCRIPT`), per-frame traces (`CONKER_TRACE_PLAYERS`), and `CONKER_FIXED_DT=1`
  makes repeated runs identical (0 game-state differences). Movement rules measured: `lab/oracle/README.md`.
- Networking skeleton: server + 2 native clients with prediction/interpolation work; one server entity with
  WebTransport + WebSocket fails on stock lightyear 0.30.1, passes with a 20-line workaround.

**Wave 2 done** (2026-10-08, workflow `wf_6b1226e6-de3`, 8 agents; two runs interrupted by the account's usage limit and
resumed). The slice is playable: two players run around scene 0x2D as character 117 (a Tediz) in **Chrome 153 over
WebTransport and WebSocket**, server-authoritative, 0 prediction error, 0 rollbacks (V, `lab/screens/wave2/`).
- Movement (`conker-sim`): idle/walk/run/turn match the original **exactly** through the server, jump within 0.0008;
  walls are approximated (ledge diverges at a corner, max ~205).
- Camera (`conker-camera`): bit-exact on 8 traces / 2,575 rows; wall avoidance and R's aim camera not ported.
- Animation (`conker-anim`): clip choice 4,951/4,952 frames, rate and time bit-exact.
- Wasm client 6.7 MB gzip. Firefox and Safari not yet tested. Ranked wave-3 list: `remake/STATUS.md` ("Wave 3").

**Wave 3 done** (2026-10-09, workflow `wf_54c32185-a13`, 9 agents; stopped twice by the usage limit and resumed).
Deathmatch combat is playable in Chrome over both transports, server-authoritative (V, `lab/screens/wave3/`). Before it,
the oracle gained a second scripted player (`CONKER_MP_HUMANS=2`, `CONKER_INPUT_SCRIPT_P2`; `lab/oracle/README.md`).
- Walls: the game's own routine, bit-exact on 4,707 captured calls; `ledge` 204.5 → 0.
- Movement + camera loop closed: all 28 movement traces within 0.001 with 0 state misses, with the camera in the loop;
  camera bit-exact on 12 traces fed the original's state; R's aim camera, wall ray and sweep, strafing.
- Combat (new `conker-rules`): 7 of 9 weapons, grenades, pickups, damage, knock-back, death, respawn, score (kills at
  0x800E0B36), timer; 487 of 508 oracle events on the exact frame. Flamethrower damage and weapons 0x14/0x16 missing.
- Spawn picker (`func_15084D70`) and the game's RNG bit-exact on 131 picks; surface words decoded.
- Lag compensation by server-side rewind (exact with one shooter per tick); text HUD; wasm 6.8 MB gzip.
- Ranked wave-4 list: `remake/STATUS.md` ("Wave 4, ranked").

**Wave 4 done** (2026-10-10, workflow `wf_b5b7bf1a-42d`, 7 agents; stopped twice by the usage limit and resumed).
Two browser players + 2 server bots fight in Chrome with 150 ms lag each way (V, `lab/screens/wave4/`).
- Combat finished: flamethrower, 0x14 (scoped shot), 0x16 (rocket launcher), grenade cook/fuse, melee lock-on, sniper
  head/body (fitted, I), random picks; 506 of 508 oracle events on the exact frame (487 in wave 3), new traces 172/172.
- Netcode: lag compensation exact per bullet with several shooters (rewound target = shooter's screen, max 0); the
  client predicts its own knock-back, death and respawn (mispredicts only during its lead after a server-decided hit:
  visible 12-13-tick snap at 150 ms); `Match::remove_player`; protocol 6.
- Movement/camera: C-strafe, root motion, player pushing, gun twist and scope, zone gate, the game's thresholds; the
  ledge camera cut; 28 movement traces still within 0.001, exclusions ~810 rows → 2. Class-6 slide not ported.
- Bots (new `conker-bots`): the game's AI script machine and Dijkstra paths (391/391 node changes); partial: kills
  below the original's (mean 3.4-6.8 vs 10), a pit trap, `bots.sh` passes only on some runs.
- Ranked wave-5 list: `remake/STATUS.md` ("Wave 5, ranked"); directions and later goals: `ROADMAP.md`.

**After the asset pack** (2026-10-11, sub-agents): a branded landing app (`remake/web/app`, Vite + React + TS +
Tailwind, bun) with a mock server browser and lobby, the game at `play.html`; sides: Squirrels (model 128, green south
base) vs Tediz (117, red north base), chosen with `side=`, Deathmatch still free-for-all (protocol 10); audio from the
pack (183 sounds, events mapped from the original, background recorded from the oracle) played with rodio natively and
in the browser after the first key press, M mutes. HUD from the original's own textures and layout (`hud/` in the pack: chocolate health, score and rank, clock,
radar, held-weapon and grenade icons, aim reticle, sniper scope, death iris), checked against its RAM dumps; H shows
the old text readout.

**Deathmatch completed against the original** (2026-10-11, two sub-agents; protocol 11). The original's SET-UP options
as server flags (`--time`, `--lives`, `--kills`, `--skill` with all 5 levels, `--radar`), its end conditions, and its
STATS results screen with RESTART/QUIT (the next match once every human chose RESTART, or after 40 s). A second map:
`--map 30` (the original's Colors map, scenario 5), with its starts, pickups and sides taken from the oracle; the client
asks the server's `/info` which map to load. Scene 0x2D is the **Total War** map (scenario 9), not Colors: names fixed.
Then the Bunker (`--map 34`, scenario 4) and its two new weapons, the crossbow (0x18, an auto-aimed bolt) and the
shotgun (0x37, a four-sphere blast), traced from the oracle (10 new combat replays, 57 in all).
Then the Temple (`--map 3f`, scenario 7): its starts, pickups and the props its Deathmatch hides from the oracle, its
sides from Raptor's two bases.
With it the caveman (class 0xB, model 136; its bots there) and the club (item 0x40, everyone's start item), traced
from the oracle (protocol 12); bots replan after a placement (lift, respawn) as in the game.

**Asset pack** (2026-10-10, two sub-agents, no workflow). `conker-extract pack` exports once what multiplayer needs
into `assets/` (205 files, 14.2 MB, 3.4 MB brotli; glTF + PNG + JSON, two raw script tables), read back bit-exact by
the new crate `conker-assets`. The server (`--assets`), the native client and the browser load the pack: the page no
longer asks for a ROM and fetches `/assets/` from its own site. Edits (textures, spawns, model lift, pickups, sink
table) take effect on reload; a malformed file is refused naming the file and field. This changes §2's "only code is
hosted": the site now serves Rare's data, so keep a public deployment private (the Caddyfile's optional password).

**Wave 6 done** (2026-10-10, workflow `wf_0cb1c326-ba0`, 4 agents, no interruption; option A part 2). Deployment
prepared and tested on this machine only (nothing deployed): server-issued connect tokens signed with a key that
never reaches the client (forged, zero-key, tampered, expired and wrong-address tokens refused), real certificate files
for WebTransport and `wss://`, `--public` addresses with the address check on, and `remake/deploy/` (VPS guide,
Caddy with brotli and cache headers, systemd unit, certbot hooks, production build). Other players' grenades
predicted (e_gren victim 13 ticks off → 0), stable slots on leave; protocol 8. Wave 7 in `ROADMAP.md`.

**Wave 5 done** (2026-10-10, workflow `wf_79fd19eb-e93`, 4 agents, no interruption; option A part 1). Dual-stack
server, 3 s timeout and instant leave on page close, input gated until loaded (0 start-up rollbacks), a connection
status line, gamepad focus, `Fighting` to its owner only (server → client 17.7 → 9.7 KB/s), smoothed corrections on
screen (e.g. a 334-unit grenade knock-back blended at 20 units/tick), HUD weapon names; protocol 7. Wave 6 in
`ROADMAP.md`.

**Fixes before wave 5** (2026-10-10, two agents): the character is drawn 0.5 × 43 above the player as the game does
(feet were 21.5 under the floor), and held weapons are drawn in the hands from the game's own models and joints
(details and leftovers in `ROADMAP.md`; evidence in `lab/screens/fixes/`, `lab/work/feet/`, `lab/oracle/*weapon_hold*`).

**Goal.** Re-create Conker's multiplayer in Rust: a **server that is the authority** (it runs the game rules) and a **browser
client** (Rust → WebAssembly), sharing one simulation crate. Players join over the internet and each controls a character.
**Method.** Game logic is re-derived from the original (decomp names + our running copy of the original as the reference,
the "oracle"); assets are extracted from each player's own ROM by a Rust extractor. Nothing of Rare's is hosted.
Why a remake and not the translated game: server authority, client prediction, mid-match join, more than 4 players and
rollback all need a simulation we own; the translated game has no savestates and a 4-player layout.

Evidence: `lab/research/{asset-formats,mp-footprint,rust-stack,prior-art}.md` (+ the five v3 reports). Tags: **V** verified
here, **3P** documented by a third party, **I** inferred/estimated.

## 1. What we know
**Oracle (V, done today).** The native headless build of the original (`lab/work/CBFD-Recompiled`) boots straight into a
multiplayer match: `CONKER_MP_SCENARIO=3 CONKER_MP_BOTS=3 ConkerRecomp --headless --seconds 45` runs Deathmatch on scene
0x2D with player 1 idle and 3 bots. Dumps show the timer counting down at 60/s, bots moving, players dying and respawning.
Decoder: `lab/research/mp-probe/ram.py <dump> match`.
**Footprint (V, measured).** In 20 s of that match, **1,099 functions ran, 585 KB of code (25.5% of the game)**: 943 game
functions (513 KB) + 147 system/audio-library ones (not remade). Only 19 of the 59 statically multiplayer-flagged functions
ran in that window (the rest may be menus, other modes, match end): the match is mostly shared engine (movement, collision, animation, weapons, AI, rendering). Hottest = the hand-written
math. Other modes will add more. Tool: `lab/work/scripts/trace_footprint.py` (trace_mode build).
**Assets (V/3P).** Containers (asset tree, rzip, texture blob) solved and verified on your ROM. Payload formats documented by
**DevOldSchool/conkers-bfd-decomp** (MIT, 2026): models, skeletons + clips (30 Hz), 0x44-byte placements, collision meshes
(98,479 triangles), textures, sound/music banks, MP3 voices; 7 spot checks agree. **sylas94/conker-asset-viewer** (no
license: behaviour reference only) proves ROM → glTF in the browser in seconds. **Still to reverse-engineer:** collision
surface meanings (floor/wall/water/kill), which placements are MP spawns/pickups/objectives, room/portal visibility.
**Stack (V, versions of 2026-10-08).** Bevy 0.19.1 (client + headless server), lightyear 0.30.1 (server authority,
prediction, interpolation, lag compensation; bevy_replicon 0.44 inside), avian3d 0.7 for collision queries only,
bevy_enhanced_input 0.26, wasm-bindgen 0.2.129, Rust ≥ 1.95 (1.98.1 installed). **WebGL2 build first** (Bevy's WebGPU is
experimental and needs a separate binary; WebGL2 reaches every browser). **WebTransport + WebSocket fallback** (Safari 26.4+
has WebTransport; lightyear bug #1693 with two server entities: test one entity with both transports first). Small VPS,
Let's Encrypt cert. Near-identical prior art: `glashoff/noob_tube` (Bevy 0.19 + lightyear + avian, WebGL2, WebTransport, VPS).
**Prior art (3P/I).** Nobody has shipped a faithful N64 multiplayer remake with an authoritative server. Closest: Perfect Dark
port netplay, still "highly experimental" after ~2.8 years. Estimates for 1-3 people + AI agents: **slice 3-6 months (2 at
best), 2-3 faithful modes 12-18 months, all 7 modes 18-36 months.** Alternative that exists today: RMG-K emulator rollback
netplay (2 players, Conker untested); this remake is justified by 3+ players, browser, server authority, drop-in.

## 2. Architecture
```
conker-extract  ROM → textures, placements, collision, level/prop meshes, characters+rigs+clips, audio   (native + wasm)
conker-sim      fixed-tick rules: movement, collision queries, weapons, damage, modes, bots              (shared)
conker-server   Bevy headless + lightyear server: authority, bots, lag compensation                       (native, VPS)
conker-client   Bevy + lightyear client: prediction, interpolation, rendering, audio, input, lobby UI      (wasm, WebGL2)
oracle tools    input scripts → original game → RAM traces; compare with conker-sim trajectories           (local only)
```
- ~~Assets extracted client-side from each player's ROM; only code is hosted.~~ Since 2026-10-10: the operator exports an
  editable asset pack once from their own ROM (`conker-extract pack`); the server loads it and the site serves it.
- Tick: 30 Hz to start (the original's game logic and animation clips are 30 Hz, V/3P); revisit after the slice.
- Movement: custom kinematic controller ported from Conker's own rules over the extracted triangle mesh (not a physics solver).
- Protocol version = simulation version (prior art: every fidelity fix breaks compatibility).

## 3. Verification against the oracle
1. Assets: our extractor's output vs DevOldSchool's tools run locally on the same ROM (same meshes, clips, placements).
2. Movement: the same scripted player-1 inputs into the oracle and into `conker-sim`; compare position/velocity per frame.
   A match needs ≥ 2 participants (V: 0 bots stays in scene 0x1D, MP flag 0; 1 bot runs Deathmatch, bot spawns ~8,000
   units away), so use 1 bot and compare before they meet; tolerance, since the oracle's runtime isn't bit-deterministic
   (v3 §4). **Blocked by P0's input injection** (the headless host has no input path today).
3. Rules: scripted encounters (damage, death, respawn, scoring, timer) compared on the RAM map.
4. Bots: behaviour compared qualitatively, then by statistics (kills/min, paths).
Keep a written log of every root-caused mismatch (prior-art lesson). Keep the RNG bit-exact where the game's is used.

## 4. Phases
**P0. Measure (in progress).**
- Done (V): boot to bot match; RAM map v1 (`mp-footprint.md` §b: player objects `0x800CC2D0 + 0x32C·i`, stats
  `0x800D213C + 0x1C·i`, timer, scenario tables); Deathmatch trace.
- Next: scripted player-1 input in the headless host (`get_input()` fed from a file, keyed by controller-poll index);
  traces for every scenario (0-12); finish the RAM map (score is NOT the guessed field, V: kills observed, it stayed 0
  across four dumps; projectiles, pickups, objectives); start collision-surface RE.
**P1. Extractor (`conker-extract`).** Order: container + textures + placements (S) → collision for MP scenes (M) → level and
prop geometry via a CBFD display-list walker (M) → characters, rigs, clips (M) → audio (MP3 passthrough, then sound banks)
(M). CLI writes glTF/PNG/WAV for inspection; then the same crate in the browser. Attribution to DevOldSchool (MIT).
**P2. Vertical slice.** One map (scene 0x2D, the oracle's Deathmatch), one character with idle/run/jump/fall, the movement
controller on extracted collision, third-person camera, 2+ browser clients through the server with prediction and
interpolation. Verify: movement trajectories vs the oracle; two remote clients see each other smoothly.
**P3. Combat.** Deathmatch weapons, health/damage/death/respawn, score, timer, lag-compensated hits. Then server-side bots.
**P4. Modes.** One team mode (Colors, scenario 9, same scene), then the rest one by one (each is its own subsystem).
**P5. Product.** Audio, HUD, lobby/matchmaking UI, spectators, >4 players, mid-match join.

## 5. Risks
- Bevy/lightyear API churn: pin exact versions (above); upgrade deliberately.
- WebTransport + WebSocket on one lightyear server (bug #1693): test in P2's first week.
- WASM size: size-optimised profile from day one (a dev build of a real game exceeded 200 MB, 3P).
- Movement feel: the slow part per prior art; budget for side-by-side tuning against the oracle.
- DevOldSchool's tooling is 5 weeks old and AI-driven: pin commit `e2400b27dda6`, re-verify every field we use.
- Multiplayer semantics (spawns, pickups, objectives, surfaces) are unpublished: our RE, from oracle RAM + code.

## 6. Superseded from v3 (kept only as reference)
Rust N64 runtime, fast3d-rs fork, thread/compile spikes, lockstep netcode, H-track as the method. The translated code and
the oracle stay as the reference. Emscripten (`lab/tools/emsdk`) is installed but not needed (Rust → wasm via wasm-bindgen).
