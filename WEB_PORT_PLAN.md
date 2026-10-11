# Conker's Bad Fur Day: multiplayer in Rust/WASM, then online

> **Superseded by `RUST_REMAKE_PLAN.md` (v4, 2026-10-08).** Still valid here: the oracle (H0, M0a), §4's determinism
> measurements (now only characterising the oracle's noise, for v4's comparison tolerances) and the research references. The Rust N64 runtime, renderer fork, spikes and lockstep design are dropped.

Revised 2026-10-08 (v3). Status: H0 done, determinism baseline measured, nothing of the port built yet.

**Goal.** (1) Play Conker's multiplayer mode in the browser, from a Rust codebase compiled to WebAssembly.
(2) Then online: people join a game and each controls one of the players on the map.

**What "multiplayer in Rust" means in this plan.** Rare's own multiplayer code runs (machine-translated from your ROM), under
a **Rust runtime, Rust renderer and Rust netcode**. Multiplayer functions are then rewritten in readable Rust one at a time
(the H-track), starting with whatever you want to change. A from-scratch Rust rewrite of the mode (7 game types, bots, maps,
weapons, physics, Rare's asset formats) would take months to years before anyone plays, and would need this translated
version as its reference anyway, so it is not the plan.

Evidence: `lab/research/*.md` (5 reports, `file:line`/URL per claim). Tags: **V** verified, **I** inferred, **?** unknown.

## 1. Facts this plan rests on
- Multiplayer: up to 4 players split-screen, 7 game types (Beach, Raptor, Heist, War/Total War, Tank, Race, Deathmatch),
  with or without computer-controlled players (V, [Wikipedia](https://en.wikipedia.org/wiki/Conker%27s_Bad_Fur_Day),
  [Kanpai review](https://www.kanpai.fr/culture-japonaise/conkers-bad-fur-day)). Bots = solo testing of multiplayer paths.
- Game code: 6,163 functions, ~575k MIPS instructions; generated C = 1.61M lines, 62 MB, 6,009 `RECOMP_FUNC`s; regenerates
  from the ROM in 1.5 s (V, measured locally).
- Runtime to replace: N64ModernRuntime, ~8.5k needed lines (V, `n64modernruntime.md`).
- CBFD-Recompiled's split-screen draws no level in player 1's view (issue #78): RT64's GPU-side depth handling, game DLs
  are correct (V, maintainer's notes). Our renderer won't share RT64's bug, but split-screen depth is tested first in M4.

## 2. Architecture
```
 every client (browser tab, same .wasm):
   translated game code (all 4 players simulated) ── Rust runtime (deterministic, virtual time)
        │                                               ├─ inputs for ports 1-4 by poll index  <── netcode (Rust)
        │                                               ├─ audio ucode ─> virtual AI ─> AudioWorklet
        └─ display lists ─> Rust renderer (fast3d-rs fork, WebGPU): draws only the LOCAL player's viewport, upscaled
 relay server (Rust, tiny): lobby + input relay. No game simulation on the server.
```
- **Lockstep**: every client runs the whole game; only controller inputs travel. Same inputs + same `.wasm` ⇒ same game.
  WASM helps: IEEE float results are identical on every machine (NaN payload bits excepted) (I, from the wasm spec's
  determinism rules, not re-read here), which native builds don't guarantee.
- **Per-player view is renderer-only**: all clients simulate 4-way split-screen identically; each renderer skips the other
  viewports and draws its own at full resolution. No risk to game state (I; validated by desync checks in N3).
- **H-track (Rust-ification)** uses the recompiled ABI (`void f(uint8_t* rdram, recomp_context* ctx)`, weak symbols), so a
  Rust function should replace a C one at link time, under either runtime (V: `RECOMP_FUNC` is `weak,noinline` on clang,
  `recomp.h:22`; I: that the strong Rust symbol wins for same-file calls and function-table entries is untested; that link
  test is H1's first step).

## 3. Constraints to accept now
- **No rollback, no mid-match join (v1).** A blocked N64 thread is a host stack paused mid-call, so there are no savestates
  (V, `n64modernruntime.md` §2.1). Hence: **delay-based lockstep** (2-3 frames of input delay at the game's 30 Hz ≈ 66-100 ms,
  I), players **join in the lobby and boot together**, spectators are free (they just receive inputs). Later routes to
  savestates: a dispatcher-style translator where all guest state = registers + RDRAM (Path B, §7), or a quiescent-point
  snapshot spike (fragile, research only).
- **4 players max** is most likely baked into the game's data (I; confirmed when the H-track first reads the multiplayer
  player array). More players = rewriting the multiplayer state and everything that reads
  it in Rust (deep H-track work). 4 humans/bots is the v1 target.
- **Same build everywhere.** Path A builds from each player's own ROM with a pinned toolchain (emsdk 6.0.11, fixed flags);
  the lobby compares `.wasm` hashes and refuses mismatches. Giving friends your build means handing them translated Rare
  code: your call.
- Browser matrix (WebGPU, V): Chrome Win/mac/ChromeOS/Android 12+, Safari 26+, Firefox Windows + Apple-Silicon Mac.

## 4. Determinism: requirement #1
Lockstep fails on the first difference, so the Rust runtime must be deterministic by construction.

**Measured baseline (V, 2026-10-08):** two identical no-input headless runs of the current C++ runtime differ by
**98 words at frame 60, 103 at frame 300, 2,810 at frame 580** (of 2,097,152), growing. (Partly measurement noise: the dump is
taken while game threads run. The growth is real divergence.) Target for the Rust runtime: **0, at every frame, every run.**

Sources of nondeterminism to design out (V unless marked; `n64modernruntime.md` §2, `cbfd-recompiled.md` §1, §5):
| Source | Today | Rust runtime |
|---|---|---|
| VI retrace | wall-clock 60 Hz thread | virtual clock; VI delivered at fixed points; real time only paces presentation |
| `osGetCount`/`osGetTime` | `high_resolution_clock` | derived from virtual time |
| Timers (`osSetTimer`) | timer thread, `wait_dequeue_timed` | min-heap on virtual time |
| Event delivery | service threads enqueue anytime; drained at the next OS call | fixed order at fixed points |
| Service threads (VI, timer, gfx, SP, saving…) | run concurrently, touch RDRAM | **none concurrent**: callbacks on the one runner |
| `AI_LEN` (game sizes its next audio buffer 736/552 from it) | real device's queue | virtual consumption at 22,020 Hz; real playback behind its own buffer |
| AI event | every VI regardless | same, on virtual VI |
| Spin-wait hook (1 ms = 3000 passes), song "playing" 500 ms window | wall clock | virtual time |
| Controller input | polled when the game asks | **input for poll #n** from the lockstep buffer |
| Shutdown | threads race the RDRAM free (crashed run A, V) | single runner, orderly stop |
| Rust pitfalls (I) | — | no `HashMap` iteration in game-facing paths, no wall-clock reads outside pacing |

**D1 revised (hard requirement):** exactly one thing runs at a time, scheduled deterministically. The backend is either
Workers with strict baton passing or JSPI stack switching (S1 decides); the "N+8 real threads" shape is out.

## 5. Milestones (Build / Verify / Blocked by)
**Done (V):**
- **H0** vanilla headless host on this Mac (`lab/work/CBFD-Recompiled`, `CONKER_RT64=OFF`, conker.toml cut to stubs + 25
  patches + 17 correctness hooks, enhancement files excluded, signed with the release's entitlements): 20 s, 1199 VIs,
  598 display lists, same counts as the shipped build.
- **M0a** RAM dumps: `CONKER_DUMP_DLS=60,300 CONKER_DUMP_DIR=…` (null renderer), `lab/work/scripts/ramdiff.py`; baseline above.

**M0b. Input script + replay** in the native host (inputs per poll index from a file). Verify: the harness reproduces a
scripted path into multiplayer with bots. Blocked by: none.

**S1. Runner spike (Rust).** Synthetic N64-OS workload (8-10 threads blocking deep in calls) on a deterministic scheduler,
two backends (Workers+baton, JSPI). Verify: Chrome/Firefox/Safari 27, switch cost, identical event trace every run.

**S2. Compile + memory spike.** Vanilla C → wasm32 (Emscripten; bare clang for comparison) with a new `recomp.h` (address
mask, FCSR rounding emulation, 64-bit `CVT_W`). Verify: `.wasm` size, compile time/memory per engine, mask cost.

**M3. Rust runtime, native first.** Replace N64ModernRuntime in the native headless build (link the vanilla recompiled C on
this Mac): scheduler, queues, virtual VI/timers, PI DMA + EEPROM 16k, 137 libultra entry points, RSP glue + scalar vector
unit, the 17 hooks, null renderer. Verify: **two runs of the same input script ⇒ identical RAM at every dumped frame**
(M0 baseline → 0), and the script still reaches multiplayer with bots. Then **M3w**: same runtime on wasm32. Blocked by: S1, S2, M0b.

**M4. Renderer.** Fork fast3d-rs; F3DEX2CBFD (G_TRI4, G_VTX decode, separate normals, 12 lights + coord modifier, CPU texgen);
**split-screen + local-viewport mode first** (#78 territory). Re-add the two render-fix hooks (frame clear, depth-glow skip)
from `conker.full.toml` for the renderer build; headless stays at 17 hooks. Verify: 4-player screens vs the native windowed
build's screenshots.

**M5. Audio.** Virtual AI (§4), AudioWorklet ring, 22,020 Hz, MP3 voices.

**M6. Shell.** ROM picker + hash check, OPFS (ROM, saves), gamepad remap (N64 pads report non-standard mappings), keyboard,
4 local ports (couch multiplayer in one tab works here).

**N1. Lobby + relay.** Tiny Rust server (WebSocket relay; WebRTC DataChannel mesh later). Rooms, seats 1-4, bot slots,
`.wasm` hash check, synchronized boot.
**N2. Lockstep.** Each client sends its port's input per poll index; input delay configurable; stall when an input is late.
**N3. Desync detection + replays.** XXH3 of game RAM every N frames; on mismatch, dump both sides; record input logs (a replay
= ROM + input log).
**N4. Spectators.** Receive inputs, choose any viewport.

**H-track (parallel, aimed at multiplayer code).** H1 shadow mode (Rust runs alongside the C copy on every real call,
results compared): first candidates already listed (pure float leaf functions, e.g. `func_15144B68`, 34 callers;
`lab/work/scripts/find_candidates.py`), paused. H2 captured-snapshot harness. H3 RDRAM accessor layer
(`struct Player<'a>(&'a mut Rdram, u32)`: game data stays in RDRAM's layout while any C touches it). H4 = the first
multiplayer change you name.

**P (optional, later). Path B dispatcher translator.** Rust MIPS→WASM in the browser on each player's ROM, guest state =
registers + RDRAM ⇒ savestates ⇒ rollback, mid-match join, desync recovery, and a hostable bring-your-own-ROM site.

## 6. Kept from v2 (still valid)
- **Rust runtime surface:** scheduler (515 lines native), queues + event inbox (222), VI/gfx/SP/DP (~850), timers (234),
  audio + input (501), RSP glue + VU (1,993), libultra wrappers (~1,070), PI + saves (760), ROM/overlays/heap (~1,000);
  keep libultra struct layouts (`OSThread` first 0x18 bytes, `OSMesgQueue`) (V, `n64modernruntime.md` §5, §8).
- **Conker checklist:** PI DMA posts `OSIoMesg*` and fills it; `__osRunningThread` (0x8002BE00) kept current; `osCicId` = 6105;
  `__osEepromTimerQ` after `osContInit`; AI_LEN = queue − one 736-frame buffer; FR=1 per thread; TLB `.game`/`.debugger` bytes
  in addressable memory; real ROM words for PI raw reads (anti-piracy); script-interpreter `setjmp` abort; VR4300
  divide-by-zero (V, `cbfd-recompiled.md` §1).
- **Runtime bugs not to copy:** `osYieldThread` asserts; cross-thread `osStopThread` asserts; queue removal head-only;
  non-atomic `wait_one_frame`; shutdown race (new, V).
- **wasm32 hazards:** no dynamic rounding (emulate FCSR.RM), 32-bit `long` in `CVT_W`, trapping float→int and division,
  `-fno-strict-aliasing`, RDRAM byte order (`^3`/`^2`) un-swizzled by every host consumer (V, `n64recomp-codegen.md` §5).
- **Memory:** mask `addr & 0x1FFFFFFF` (TLB windows at ~336-352 MB) or two-range remap (~100 MB); S2 decides (I).
- **License/distribution:** personal project; GPL-3 parts fine.

## 7. Toolchain (2026-10-08, V)
Installed: Rust stable 1.98.1 + nightly 1.101 (`rust-src`, `wasm32-unknown-unknown`, `wasm32-unknown-emscripten` on both),
emsdk 6.0.11 in `lab/tools/emsdk` (clang, `wasm-opt`; run with `EMSDK_PYTHON=/opt/homebrew/bin/python3.14`, system Python is
3.9), Homebrew llvm/lld 23 (wasm32 + mips targets), cmake, ninja, Node 26, Docker. N64Recomp + RSPRecomp built in
`lab/work/CBFD-Recompiled/tools/N64Recomp/build`.

## 8. Open questions
| Question | Resolved by |
|---|---|
| Is "join in the lobby, boot together" acceptable for v1? | you |
| 4 players (humans + bots) enough, or more? | you |
| Workers-baton vs JSPI under the determinism constraint | S1 |
| `.wasm` size/compile cost; memory layout | S2 |
| Does rendering code mutate game state (local-viewport mode)? | N3 desync checks |
| Input delay that feels OK for Conker's shooters | N2 playtest |
