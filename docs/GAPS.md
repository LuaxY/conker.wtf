# Multiplayer gaps (G6 audit, 2026-10-15)

What is still missing, after G1-G5 and the controls rounds R1-R3 (protocol 42). Five read-only auditors re-read every
"Left", "unverified", "not ported" and "handoff" in `STATUS.md` and `ROADMAP.md`, plus the 54 `ponytail:`
comments in the code. Every item was checked against today's code; a later doc saying "done" didn't count without a
`file:line` or a test. Nothing was built, run or fixed. The per-item evidence (closures, justifications, source lines)
is in `lab/research/mp-original/audit-g6/part-a.md` … `part-e.md` (gitignored).

**Totals.** Raw rows across the five parts (an item listed in several sections counts more than once): 362 closed
(55 of them on doc evidence only), 191 justified (approximations within the oracle's noise, by design, or
deliberately not ported), 262 open, 50 manual. Merged, the open items are the 26 entries of section 2 and the 60 of
section 3 (many group several items), plus the inferred values of section 4. Most "Left" lines in STATUS's older
sections are historical: this file is the current list.

Key: lane (rules, bots, client, server, net, pack, research), size S/M/L, **O** = needs a new oracle run.
Files: `game.rs`, `parts.rs`, `spread.rs`, `beach.rs` (rules) = conker-rules; `brain.rs`, `npc.rs`, bots' `lib.rs` =
conker-bots; `main.rs` = conker-server; `orbit.rs`, `crosshair.rs`, `held.rs`, `render.rs`, client `lib.rs` =
conker-client; `volume.rs` = conker-camera.

## 1. Your decisions
- `controls-hip-bolt-lock-on`: under MODERN the crossbow and knives' hip bolt flies to the crosshair within its cone;
  the N64 ±28° lock-on was dropped (`game.rs:3015-3028`). Bots keep it. Keep, or bring the lock-on back?
- `controls-tommy-climb`: the tommy gun's climb (up to about 33°) lifts modern bullets off the crosshair
  (`game.rs:2957-2967`). Keep it, scale it down, or turn it into a client camera kick?
- `controls-spread-extras`: build or skip: a movement/jump cone outside the scope, bloom over sustained fire, a crouch
  bonus, the cone drawn on the crosshair (`spread::cone` is unused by the client).
- `deploy-license`: no `license` in `Cargo.toml`'s `[workspace.package]`.
- `effects-bullet-visuals` (F1 lane D's options): one's own tracer drawn from the predicted shot (today a round trip
  late, snapshots only); impact sparks for bullets and the sniper's ray; the sniper's red laser (the N64's 3 frames
  before its shot; MODERN now shoots on the press, so it would have to be instant); 0x23's and the shotgun's flashes.

## 2. Wrong today (found in the code; fix first)
Most are small. The first two were found by reading the code, not by running it.
- **Fixed in F1 (protocol 43).** `controls-mounted-gun-modern-aim` (client S): under MODERN the rules turn a seated gun from the input's aim
  (`game.rs:2066-2067`), but the client freezes the orbit while seated (`orbit.rs:424-430`) and sends the mouse as a
  stick (`lib.rs:1247-1255`), so `camera_yaw`/`aim_pitch` stay at their mount-time values: the mouse likely can't turn
  a mounted gun.
- `cameras-vault-fixed-hints` (client S): Heist's four fixed cameras on 0x1A (commands 5 and 0x11) are dropped:
  `Fixed::parse` keeps command 0 only (`conker-camera/src/volume.rs:174-181`; a test asserts command 5 gives nothing).
- **Fixed in F1 (protocol 43).** `controls-sniper-shots-uncounted` (rules S): the sniper's aimed ray (`game.rs:2615-2626`) never adds to
  `Fighter::shots`, which breaks score ties (`game.rs:853`, `:893`).
- `cameras-burning-busy-flag` (rules S): `camera_view` never sets `PlayerView.busy` (the burning state,
  `brain.rs:149`), so a burning player isn't refused R's aim.
- `cameras-ceiling-0x2b` (rules S, O): camera ceilings (action-2 volumes) are skipped (`volume.rs:229`); Beach has one.
- `dm-start-draw-per-match` (server S): the level scripts' start draws (0x34/0x3F Deathmatch, Beach) run once per
  server run (`main.rs:713`); the original draws each match, so a RESTART keeps everyone's start.
- `heist-outro-pick-latch` (server S): `human_won` (`main.rs:1387`) is recomputed each call, not latched like
  `winner`; a leave during the end fade can change the outro.
- `server-bases-k` (server S): Bases' k counts a side's current players, so after a leave two players can share a
  start record.
- `server-join-team-wrap` (server S): Deathmatch's per-join team (`joins as u8`) wraps after 256 joins.
- `tank-shell-hit-weapon` (server S): `hit_weapon` (`main.rs:2026`) has no tank-shell case.
- `war-colors-flag-take-mid-swing` (rules S, O): Colors' flag take still accepts a landed melee swing (`game.rs:1687`);
  the original refuses obj+0x89, as fixed for Heist's bag (`game.rs:1721`).
- `beach-escape-jingle-freeze` (bots S, O): script query (6, 3) "escape jingle playing" always answers 0
  (`brain.rs:712`), so a human's freeze is always 150 frames.
- `held-crossbow-frame-lag` (client S): the crossbow is drawn a frame ahead of the oracle; `held.rs:615` should use
  `clock − 1`.
- `client-hit-clip-frame-early` (client S): a hit's clip starts a frame early and its blend returns to the old clip
  (the rules' rule at `parts.rs:153` isn't mirrored).
- `client-escape-fade-alpha` (client S): `fade_escapees` (`beach.rs:361`) skips `display_alpha` (`hud.rs:299`).
- `controls-pad-start-browser-lock` (client S): a pad's Start opens the menu but the browser keeps the pointer lock;
  no `exitPointerLock` anywhere.
- `controls-auto-trigger-tail-before-grenade` (rules S): an automatic's trigger let go for a grenade still runs its
  7-frame tail before the holster.
- `controls-free-camera-m-grab` (client S): with the debug camera on, Bevy's free camera toggles its own cursor grab
  on M (Mute) (`render.rs:548`).
- `race-restart-during-results` (client S): the original's RESTART works during the 40 s results; ours only after.
- `race-missile-sound-snap` (client S): a missile snapped far onto the track plays a second launch sound.
- `audio-pickup-sound-roaming` (client S): a roaming pickup's sound plays at its next spot.
- `client-lobby-picks-from-info` (client S): the lobby's allowed characters come from the dev web server's settings
  (`Lobby.tsx:19`), not the game server's `/info`.
- `hud-timer-latency` (net S): the clock has no server tick on `MatchView`, so the timer lags by the latency.
- `net-voice-one-per-tick` (net S): `MatchView::voice` carries one line a tick; overlapping lines are dropped.
- `client-sweepers-zone-voices` (client S): zone voices aren't reset on a restart; sweepers and lamps use per-client
  random numbers and move once a tick.
- `bots-sh-flake` (bots S): `bots.sh` still FAILs a line most runs (a bot that never dies or never hits).

**From play (F1-F3, 2026-10-15), still open**
- `play-bazooka-silent` (client S): the user heard no launch sound in a browser; 699 plays on every native path
  (headless, so never heard). Fixed only for the Sergeant, Tediz Leader and Beach's clip (re-pack). Needs: which
  character, every shot or the first, `audio/common/0699.wav` loaded.
- `play-rocket-point-blank` (rules S): a MODERN rocket aimed at a wall nearer than its muzzle flies toward that
  point, maybe back past the shooter (untested; fall back to the facing).
- `play-sniper-first-tick` (rules S): a MODERN Fire on the tick the aim goes down is lost, not delayed.
- `play-reconnect-new-player` (server M): RECONNECT joins as a new player (score lost); the old slot stands up to
  5 min. Reclaiming it by identity is the upgrade. A crashed client holds its slot 5 min.
- `play-small` (client S): the lost page cuts its reason at 46 characters; `team=` not asked again after a
  reconnect; a loss with the settings page open keeps unapplied changes; the web re-lock click also fires; the
  capture hint shows with F8's camera on; a trackpad queues long wheel-zoom holds; the gun's sight has no N64
  smoothing and the pads' aim-assist tests from the body's arm, not the gun's eye.
- `results-restart` (server/client M): RESTART on the results now takes a click and Enter and votes; going on to
  the next game is out of scope for now (the user's call).

## 3. Not ported or not drawn
**Deathmatch bots**
- `dm-0x34-fights` (bots M, O): on 0x34 the bots fire the twin guns 177 shots a run against 119 and hit 0.071 a shot
  against 0.108 (shots per drawn frame 0.165 vs 0.117, untraced); they stand in the electric box 486 frames a run
  (153 still) against 232 (2.5), and see enemies wall-free 143.5 frames vs 15.8.
- `bots-dm-kill-rate` (bots M, O): deaths a match 2D 10.6 (`bot_match`) / 15 (server) vs 13; 0x30 5.25 vs 6.0;
  0x34 12.75 vs about 16.
- `bots-pitch-byte-1d1` (bots S, O): obj+0x1D1's 1.1° aim steps and ramp not ported (0x23 hits 0.090 vs 0.096,
  crossbow 0.221 vs 0.226; may not move the rates).
- `dm-mounted-gun-take-rate` (bots S): 0x30's gun taken 2 times in 8 runs vs 4 in 5; never re-measured.
- `dm-23-holders-die-more` (bots S, O): 0x23 holders die in 6 of 19 spells vs 1 of 33.
- `bots-twist-target` (bots S, O): the bots' twist (`func_150639BC`) not ported.
- `bots-r-release-0x16-0x23`, `bots-camping-roles` (few bots camp far longer), `bots-club-spells` (104-144 vs 193),
  `bots-flame-noise` (bots don't hear the flame stream, `lib.rs:969`): bots S, O.
- `bots-joiner-slotting` (bots M, O): a joiner is appended, not slotted as the original; a mid-match join, leave or
  switch can leave bots out of team order until the next `reseat`.

**Tank**
- `tank-hit-rate` (bots M, O): hits 29.4% vs 27.6%; the bots' hull turning while accelerating (`func_15056B08`, up
  to 0x400 a frame) isn't ported (the oracle fires 30-46% of shots while turning, the remake 16-25%).
- `tank-class-script-ops` (bots M, O): ops 0x9B, 0x70, 0x3B, 0x8A are no-ops, 0x31 only draws the RNG, op 0x67's
  +0x246 turn unread (`npc.rs:303-318`).
- `tank-ranking` (research S, O): Tank's ranking not decoded; shield vs bomb, the +5 kick, untimed lives unchecked.
- `tank-lights` (pack/client S): no head and brake lights (quads only, not in `lights.json`).
- `tank-e-plant-margin` (bots S): carriers coast 2 past the trigger (stick 10.24/33.28 vs 10/33).

**Race**
- `race-finish-cameras` (client M, O): the finish's cut, hold, drag and second cut; the law isn't decoded.
- `race-camera-mode8` (client M): the FOV add (10 + 24.6·s, +30 on the grid), the boost distance, the yaw's
  hand-over (about 100 frames), the death's look-down (measured, not ported); the listener 80-117 behind the racer.
- `race-lamp-drift` (client S, O): the countdown lamps follow about half the camera's move; drawn fixed.
- `race-powerup-swing` (client S, O): the power-ups' ±8 swing isn't drawn.
- `race-bot-kill-ops` (bots S, O): 0xBC's kill (command 0x21, `obj(0xE020)`) returns 0 (`brain.rs:612`).

**Heist**
- `heist-roster` (bots M, O): with the original's roster (humans seated first) the bots take the bag 23-24 times vs
  30 and hit 28-31 vs 39; cause not found. With 3 bots, pickups are about half the oracle's.
- `heist-carrier-hit-clip` (rules S, O): the carrier's hit clip 0x11B isn't played.

**Beach**
- `beach-swim-effects` (pack/client M, O): the splash drops (the table's #0xBE is an eyeball; `func_15147C4C`
  untraced; `effects::drops` is ready), op 102 bubbles, op 12, the wade's op 90 and op 97 step splashes.
- `bots-beach-scores` (bots S, O): 2 of 4 runs end 20-4, 2 stand at 11-16.
- `beach-wade-end-drive` (rules S): one frame at 75% after a wade's end (`beach::drive` runs before the step).

**Raptor**
- `raptor-bots-balance` (bots M, O): server cavemen score 16-19 a match vs 14-16 (about 15% over); the bot bite runs
  to its end where the original's are cut at 12-frame steps; War's Raptor with a raptor human never run.
- `raptor-camera-rise` (client S): the camera's rise to about 225 when turning and the lower look in a jump.
- `raptor-team-header` (pack S, O): the team header textures for 2+ humans aren't packed (YOU WIN/LOSE stands in).

**Total War and Colors**
- `war-0x2d-lighting` (client M, O): 0x2D gets a room tint only (`lights.rs:4-25`); the level's light table and the
  shelter's lighting during the countdown are unread.
- `war-flag-169-xlu` (client S, O): Colors' flag 169 switches to its translucent table in some frames; not drawn.

**All team modes**
- `client-team-intros` (pack+client M, O): the intro cutscenes (Total War cs1/cs3, Colors, Beach cs1/cs2, Raptor
  cs1/cs3) have no code and no export, and were never cut on purpose. Port them, or record the cut.

**Effects**
- `effects-not-drawn` (client M, O): the tanks' exhaust, the 0x34 cone's smoke, cs5's smoke and clods, Beach's far
  fire (spawner not found), the blasts' light, bullet impact sparks, the bubbles' sway, the strike's streak geometry
  (`func_15140410`).

**Cameras**
- `cameras-occlusion-swing` (client M, O): no occlusion or floor-clearance swing in conker-camera (the N64 camera;
  the modern orbit slides along its look line).
- `cameras-gun-eye-fades` (client S, O): the mounted gun's eye fades; `smooth::Drawn`'s corrected view has no kick.
- `cameras-render-offsets` (client S, O): the render-only yaw/pitch offsets (up to about 3.7° when turning).
- `cameras-beach-tunnel-hints` (research S, O): Beach's tunnel hints are active but never compared with an oracle frame.
- `cameras-k6-k10-residue`, `cameras-scope-grounded-trunc` (S, O): a 0.08 yaw residue after 0x23/0x24's side step;
  the scope's grounded test compares floats where the game truncates.

**Movement and rules**
- `sim-class6-slide` (rules L, O): the slide on the 12 steep class-6 faces (`func_15051558`, about 960 lines of MIPS).
- `rules-burning-run-steering` (rules M, O): the burning run's steering, bounce and pat-out (`game.rs:4091-4095`);
  e_w15 still has 2 known misses, its path off up to 1,117.5 (wave 4's figure).
- `sim-replay-residue` (rules S, O): the strafe on a slope (combat4_lock stops at row 995), a strafe's first-frame
  speed, short C taps during a throw (4.96), 0x23's scope hold lapsing 1 frame in 30 (camera4_w23 row 530), the
  spawn's frame of state 0, A from state 54, high jump from 27 and other entries.
- `sim-floor-reach-85` (rules M): the floor query's reach on non-Race maps (`STEP_UP` 50; only the board uses 150).
- `sim-anim-pose-mixer` (research M, O): the cross-fade is assumed linear; it costs e_w09's last bullet (3 rows).
- `rules-parts-residue` (rules S): ohit's 12 head hits not replayed with parts, earlier-slot strikes treated as after
  the victim's update, combat4_w14's aimed head graze (132.0 vs 134.2).
- `rules-flame-shots` (rules S, O): the flame counts no shots.
- `rules-sergeant-shot-root-motion`, `rules-stale-health-other-modes`, `rules-repickup-reholster`,
  `server-restart-deadline-quit` (the deadline restarts where the original quits): rules/server S, O.

**Prediction and net**
- `net-lagcomp-pose-rewind` (net M): lag compensation moves the spheres but doesn't rewind the pose; explosions and
  fires are tested in server time, not the shooter's view.
- `prediction-push-remote` (client M): pushing a moving remote player mispredicts (e_w15 11-21 ticks, bot melee 23-45
  rollback ticks in Chrome; old figures).
- `controls-remote-upper-body-aim` (net M): other players' upper body isn't aimed (their pitch isn't on the wire).
- `prediction-observer-rollbacks` (client S): others' 0x34 plate steps (rollbacks 2 → 18), others' sitting bolts'
  push, Tank's doors (on `MatchView`).
- `client-others-30hz` (client S): other players (and the radar and listener in Tank, Race and on a mounted gun) are
  drawn at whole ticks.
- `prediction-small` (client S): `seat_view` right after a leave, the thrown 0x3A drawn from `MatchView` a few frames
  late, the scare's first frame, the plunger a round trip behind, the pistol's head unposed in prediction.

**Controls**
- `controls-lead-empty-space` (rules M): the aim ray converges on the far wall, so leading a target misses by the
  muzzle offset (±20 for the twin guns; strafing column about 40 even without spread).
- `controls-upper-body` (client S): the layered pose switches in one frame (no cross-fade); non-28-joint bodies bend
  the spine only; the drawn bend vs the server's spheres (head drift 17 at ±80°) unmeasured.
- `controls-muzzle-check-bodies` (client S): the crosshair's blocked-muzzle check tests walls only.
- `controls-layout-map-fallback` (client S): no `getLayoutMap`; labels learn a key only once pressed.

**Pack and rendering**
- `client-fog-sky` (pack+client M, O): sky and fog colour and range, near/far, sun and ambient are all guessed.
- `pack-segment8-levels` (pack M, O): the maps' segment-8 render state; 0x2D's duplicate segments 12/13.
- `pack-char-materials` (pack S): 31 textureless character materials draw vertex colour only (PRIM, ENV, K5
  dropped); 15 models' alpha never seen in a frame; cs5's other tank untextured; 0x34's object 71 untextured.
- `pack-small` (pack S, O): Beach's placements 0, 4, 5 sink 44 in the game but are static here; Beach's emitters
  start at 1099 vs 1100-1108; picks 177, 178, 180 never run on the oracle.
- `held-checks` (client S, O): the 0.6 picks' lift and held size, the held knives and revolver from the front, the
  laser's colour for other holders, the forced blink-shut, no screen of a sinking floor or the treads.
- `audio-loop-seams` (client S): 0x34's and 0x3F's loops have no crossfade at the seam.
- `audio-small` (client S, O): Race's hum height halving, the mounted gun's 0x622 loop and 0x617.

## 4. Unverified approximations
These work, but a value or law is inferred (I) and was never checked against the oracle. Low priority; an oracle run
each would settle them. Race: the trail's widths and lamp flicker, the exhaust glow, the autopilot ramp,
`BOOST_AUTO`, the respawn over the void, the missile's hit radius and homing height. Tank: the camera's heading
follow, the wall pull-in, the shells' blast, tread side per wheel, the muzzle point, song 50's volume. Heist: the
bag's fall, shadow and carried place, the tommy gun's hold, the voice pick, the lob's settle, the steal sounds, the
outro's axes. Raptor: the bite edge, lock-on, egg touch, jump zones, the body in the jaws, the outro ops 5/0x11/6.
Beach: the rules' 0x207/0x26, teleport, strike blast, the swim clips' rates, the grenade on 0x2B. Total War and
Colors: the cloth rate, flag 0x45's other values and touch radius, the delivery sound, the mirror mask, the doorway
hit, both HUDs' fills. Deathmatch: 0x1A's bat joint and masks, the traps' contact and riders, the mounted gun's
homing joint, the shotgun's point-blank sphere, the caveman's first swing, the high jump's entries. Effects and
cameras: the burning flames' joint table, blast columns, grenade numbers, other guns' flashes, the electric arcs'
length, the fixed hints' floor refresh, the R sweep's sizes, the takeover's camera snap, the light share and shake
signs. Bots: the trigger query from the eyes, the zone test's frame. Other: the iris' death durations, the stick
glow's phase, the muffle curve, the radio clicks, the spawn record's bytes +6 and +0x15, the knock's ground speed cap.
Full list with sources: the audit parts.

## 5. Needed for a real deployment
- `deploy-real-hosting` (server M): `deploy/` is a dev tool; there's no hosting yet.
- `hud-join-flow-production` (server M): the lobby and server list come only from the local dev web server
  (`web/app/server.ts`); a deployed static site has none.
- `web-landing-counts-logs` (server S): the landing page has no player counts, pings or logs (no endpoint).

## 6. Beyond the original (optional)
Spectators and rollback-safe drop-in; matchmaking or room codes (the server browser lists `api/servers`).

## 7. Code and doc health
- `sim-consolidate-tables`: `SURFACES` vs `SINK`, `rise_along` copied, `raycast`/`hits_body`/`push_out` still in
  conker-rules.
- Unused `avian3d` and `bevy_enhanced_input` in `[workspace.dependencies]`; `README.md:4` links outside the tree.
- `pack-format-bump`: `FORMAT` (`conker-assets/src/lib.rs:39`) never bumped for `maps/24/class.bin`.
- Tests: `bots/tests/rom.rs` `play` never sets `Fighter::bot`; the server's `LAGCOMP` log tests the cylinder; no test
  for the radar's carrier mask or `characters::body` against `character.json`.
- Stale comments: `conker-extract/src/outro.rs:12` (op 9 is read), `conker-camera/src/lib.rs:147` (`busy` is known).
- Stale notes: `RUST_REMAKE_PLAN.md` (last 2026-10-12), `lab/oracle/README.md`, the camera4/combat READMEs, the Beach
  research doc's `/tmp/beachp` paths.
- `client-warp-spawn-cut`: `--warp` moves a player without the spawn's camera cut (dev only).
- `net-lightyear-relink-report`: `relink_evicted_session` still pins `lightyear_aeronet =0.30.1`; the upstream report
  is unfiled (your account).
- `sim-3vi-frames`: the tick is fixed at 2 VIs; the original's 3-VI frames are untraced and not justified in writing.

## 8. Your manual checks
**Chrome (G6's browser half):** every mode joined, with screens; the bone crossbow 0x41, bone knives 0x3F and a
raptor death; the 10 s connect watchdog and the page's status words; `#net`/`#sound`; a mounted gunner's hide;
RESTART's fade; the stuck-in-gas case; the landing page; a cheat pick's glTF loading after the connection; the R
twist.
**Modern controls in a browser:** pointer lock (`unadjustedMovement` and its fallback); Esc or lock loss opening the
menu with neutral input; RESUME re-locking ("CLICK RESUME AGAIN" on refusal); mouse 4/5 not navigating; settings in
localStorage (`conker-settings`, APPLY); LEAVE MATCH returning to the page (fixed; checked headless with the keys, a real click not); the PRESS F prompt and crosshair; whether
Chrome scales mouse movement on high-DPI screens; middle-click autoscroll; a pointer-lock permission prompt.
**Other browsers:** Firefox and Safari 26.4+ (two windows; the empty `serverCertificateHashes` list), background tabs
and the 5 min timeout (F3: a hidden page ticks on a timer; RECONNECT after a drop).
**Pads and input:** a real Xbox/DualSense with two windows (X tap/hold, the edge ramp, aim-assist, triggers past 0.5,
Start, glyphs); rumble on native Windows/Linux; the team screen's BALANCE on a pad; AZERTY labels; a native window's
grab and close; tuning the spread cones in real play.
**Listening:** Beach's mix (zones, stem, radio, outro voices), the villagers' cries, the club and caveman sounds.
**Deployment:** a host and domain (`deploy/README.md` steps 1-9); certbot under systemd reloading into
`/etc/conker/tls` (`renew --dry-run`); the unit's hardening; Caddy with HTTP/3; IPv4 into `[::]` on Linux; the Linux
build; WebTransport without a digest, `wss://` and same-origin `/token` on the https page; native WebTransport against
system roots; the pack and web build synced to the host; a match over the real internet.
