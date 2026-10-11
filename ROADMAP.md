# Roadmap: Conker multiplayer remake (after wave 6)

Where things stand: `RUST_REMAKE_PLAN.md` (status) and `remake/STATUS.md` (details, verification, gaps).
Written 2026-10-10, updated after waves 5 and 6 the same day. Within each list, order is priority.

## Before wave 5 (done 2026-10-10)
- Feet under the floor: fixed. The game draws the model 0.5 × 43 above the player (bank 0x11 record +6, obj+0xC8,
  `func_1502DF38`); `MODEL_LIFT` in the client. Matches the game's drawn joints on idle, run, crouch, crouch-walk.
- Held weapons: drawn in the hand(s) from the game's own bank-09 models on the hand joints (9 right, 4 left), shown
  and hidden on the draw/holster clip events. Left over:
  - 3 of the twin guns' materials draw untextured (partial texture loads, an extract-gbi limit) → wave 6, then
    wave 7;
  - the weapon disappears 3-4 frames early at death → wave 7, then wave 8;
  - C-down with the twin guns or flamethrower drawn throws a grenade here but not in the game → wave 7, then
    wave 8;
  - ~~the HUD names don't match the models for some weapons~~ **done in wave 5**: 0x39 rifle, 0x14 throwing knives,
    0x23 scoped revolver, 0x21 katana, 0x22 chainsaw.

## Wave 5 (done 2026-10-10, workflow `wf_79fd19eb-e93`)
Chosen: A, split in two smaller waves. **Wave 5** = items 1-2 below (smooth corrections, connection robustness) and
the HUD weapon names. Items 3-5 moved to wave 6. Results: `remake/STATUS.md`, "Wave 5".

**A. Ready to play online (recommended):**
1. **Done, except other players' grenades.** Smooth the hit correction on the victim's screen. It was a 12-13-tick
   snap at 150 ms of lag; now it blends on screen. In Chrome at 150 ms:
   - a twin-gun hit moves the player 3.3 units a tick instead of 26.5 in one frame;
   - a grenade's knock-back moves it 19.7 units a tick instead of 335.

   The pushing client's prediction improved (e_w15: 13-14 → 11 ticks over 0.001). Predicting other players' grenades
   near the local player is blocked: protocol 7 no longer sends their state to other clients → wave 6 (**done in
   wave 6**, protocol 8).
2. **Done.** Connection robustness:
   - dual-stack `[::]` bind;
   - a disconnect on tab close (`pagehide`; the page's transport closes, so the slot is free 4-38 ms later), then a
     3 s timeout;
   - gate input until assets are loaded;
   - a message on the page when the connection fails (the `#net` line);
   - check window focus before reading the gamepad;
   - send `Fighting` only to its owner (server → client traffic 17.7 → 9.4-9.9 KB/s per client);
   - the smoke test's start-up rollback (0 now).
3. Visuals: flame particles, bullet, grenade and rocket models, the gun twist on the model, the white quad → wave 6,
   then wave 7.
4. Browsers: Firefox and Safari 26.4+ (`remake/web/README.md` two-window test) → wave 6, then wave 7. Background
   tabs and gamepads → wave 7, then wave 8.
5. Deployment prepared: a VPS with real certificates, a non-zero netcode key, `wss://`, brotli for the wasm, a
   hostname-safe connect token, cache headers → wave 6 (**prepared and tested locally in wave 6**; the real
   deployment → wave 7). Needs from you: a host and a domain.

**B. Closer to the original:**
1. Bots to the original's numbers (kills 3-7 vs 10 per 5 minutes): fire along the bot's aim yaw and pitch, the bot
   turn rate in tight corners (the pit trap), re-pickups, bots drawing weapons 0x16 and 0x23; `bots.sh` passing every run.
2. Fidelity gaps: the slide on steep faces (class-6 surfaces), the burning player's steering, 0x23's scope-hold lapse,
   the slope during a strafe, the bots' twist, the first strafe frame's speed, the sniper's head/torso spheres against
   the skeleton, the k6/k10 camera residue.

## Wave 6 (done 2026-10-10, workflow `wf_0cb1c326-ba0`, 4 agents)
Items 1 and 4 are done. Item 1 is prepared and tested on this machine only, since there is no host or domain yet.
Items 2-3 moved to wave 7, together with the real deployment. Results: `remake/STATUS.md`, "Wave 6".
1. **Done, tested locally.** Deployment prepared, with secure connect (`remake/deploy/README.md`):
   - a connect-token endpoint: **done**. The server signs each page's token with `--token-key` (a random key per
     start in dev) and serves it at `/token`; Caddy proxies it in production. A client can no longer make its own
     token. Tokens last 60 s;
   - real certificates: **prepared**. `--cert`/`--cert-key` serve WebTransport and `wss://`, and certbot
     (`--standalone`) has hooks for Caddy and the server. Tested with a test CA; no real certificate was requested;
   - a hostname-safe connect token: **done**. Tokens name the `--public` addresses, and the server checks them;
   - brotli and cache headers: **done** (Caddy). The wasm is 4.3 MB with brotli (6.9 MB gzip, 21.8 MB raw). The
     client folder is named by its content and cached for a year; the page is revalidated;
   - `[::]` accepting IPv4: verified on macOS (`tcp46`/`udp46`). On Linux it is a step-9 check in the deploy README.
2. Visuals → wave 7.
3. Browsers → wave 7.
4. **Done.** Wave 5's leftovers:
   - other players' grenades are predicted (protocol 8). e_gren's victim: 0 ticks over 0.001 at 0 and 150 ms (was
     3-4 and 12-14); in Chrome, 0 at three blasts;
   - stable slots: a leave costs the other players no rollback (was 3-14 ticks each), and the next joiner takes the
     free slot;
   - the native HUD shows the connection status and its reason.

## Asset pack (done 2026-10-10)
`assets/` holds the editable pack (see its README.md); the page no longer needs a ROM. Leftovers: the server and the
clients don't check they read the same pack (natively only); `pickups.json` accepts any weapon id; a
`user:password@` URL breaks the page under the optional password (type it in the browser's prompt instead); the
bots camped with the sniper rifle in the last `bots.sh` runs (known bot weakness, not caused by the pack).

## Done since the asset pack (2026-10-11)
- Landing app with a mock server browser and lobby (UI only: real server lists and lobbies are still to wire).
- Squirrels vs Tediz sides with their own bases; audio (effects, voices, ambience, background); the original's HUD.
- Leftovers: bullet fly-by 802 and the cooked grenade's fuse sound; the three grenade landing sounds in random order;
  the hit's weapon is inferred on the client (the server sends no weapon per hit); a side is fixed until reconnect;
  a squirrel's sniper spheres and crouch height are the Tedi's (unmeasured); the mock lobby shows 9/8 on a full server.
- HUD leftovers: in the sniper scope the local player's own body and weapon show at the edge of the view (hide
  the own model while scoped); the rank on ties (the original's rule is unknown); the death iris timing for
  grenade deaths and its centre; weapon 0x16's R tint; the scope dots' turn; the page's `#net` line and sound
  hint overlap the clock and weapon icon in narrow windows. Deploy `deploy/pack.sh` before a new `web/dist`
  (the client needs `hud/hud.json`). `lab/hud/` holds 1.3 GB of RAM dumps that can be deleted.

## Wave 7: visuals, browsers, the real deployment
1. **Visuals:**
   - flame particles;
   - bullet, grenade and rocket models (placeholder spheres today);
   - the gun twist on the model;
   - the white quad near record 1;
   - the twin guns' 3 untextured materials (partial texture loads, an extract-gbi limit).
2. **Browsers:** Firefox and Safari 26.4+ (`remake/web/README.md`, "Manual test in Firefox and Safari"), including
   how each treats the empty `serverCertificateHashes` list that the wasm client always passes.
3. **The real deployment.** Needs from you: a host (a Debian 12 or Ubuntu 24.04 VPS) and a domain, with its A and
   AAAA records. Then:
   - follow `remake/deploy/README.md`, steps 1-9;
   - first check, in Chrome on the https page: WebTransport without a digest, `?transport=ws` over `wss://`, and
     `/token` from the same origin. These could not be tested locally, because no browser here trusts the test
     certificate;
   - then the rest of the README's checks: brotli and cache headers, IPv4 into `[::]` (`bindv6only` 0, a page from
     an IPv4-only network), `certbot renew --dry-run`, the server's Linux build;
   - play a match over the internet with real latency.

   If there is no host yet, the rest of the wave goes ahead without this item.
4. **Wave 6's small leftovers:**
   - the bots treat a free slot between occupied ones as a human who is always dead. `Bots::new` takes a count, so
     the fix is a conker-bots API change that lets the roster skip free slots;
   - a per-slot lift reset in conker-bots' `Lifts`. Today the server rebuilds every lift when a joiner takes a free
     slot, so nobody can take a lift for one tick; `Lifts::remove` is unused;
   - remove the unused `DEV_PRIVATE_KEY` from conker-sim `net.rs`.

## Wave 8
- Background tabs (throttled tabs and the 3 s timeout) and a real gamepad with two windows.
- Smoothing details:
  - a correction during the camera hand-off restarts its weight (a view jump of up to about 60 units, not seen yet);
  - the drawn camera's hint cut on a different tick;
  - draw the local player between 30 Hz ticks on 60 Hz displays.
- Leaving and joining details:
  - Ctrl-C in a terminal skips the disconnect;
  - native WebTransport with a digest is IPv4 only (lightyear binds `InAddrAnyV4`; wave 6's `ca=` path is
    dual-stack);
  - a waiting player's lethal hit still counts for its attacker;
  - the 10 s connect watchdog is untested in a browser.
- Held weapons: hide the weapon on the game's frame at death; C-down with the twin guns or flamethrower drawn; check
  the held knives and revolver from the front.
- Deployment details, from wave 6:
  - reload a renewed certificate without restarting the game server (today each renewal ends the matches in
    progress);
  - tokens that survive a slow load (they last 60 s from the ROM pick);
  - natively: a connect timeout for the token fetch (plain http); WebTransport without a digest or `ca=` (lightyear
    pins an empty list);
  - rate-limit `/token` if abuse appears. The endpoint answers one request at a time.
- The native HUD's wording (`Reload to reconnect` is the browser's), and its `connecting…` state.
- Test setup: with `--wait`, every page takes one rollback at the match start, and the bots hit humans before the
  start.

## After wave 5
1. Whichever of A or B wave 5 did not take.
2. HUD and lobby: the game's own HUD, a scoreboard, the join flow, spectators, the match-end screen (scene 0x1D).
3. Audio: MP3 voices, then the sound banks, triggered by combat events.
4. Other modes: War: Colors first (scenario 9, same map; start records and masks known), then Beach, Heist, Raptor,
   Tank, Total War, Race, each with its own maps and rules, traced from the original the same way.
5. More characters and maps (each mode's teams, classes and scenes).
6. Beyond the original: more than 4 players, matchmaking or room codes, rollback-safe drop-in for spectators.
7. Fidelity details: 3-VI frames, the render-only camera offsets, segment-8 render state, fog and sky from the game,
   the level's duplicate geometry; consolidate the duplicated tables and world queries into conker-sim; refresh the
   stale lab notes.
