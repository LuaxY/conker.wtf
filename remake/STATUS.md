# Status after wave 6 (2026-10-10)

## Controls C4: the client's feel (2026-10-15)
**Lane C4-client** of `../CONTROLS_RESEARCH.md` (§4, §8, §9, §10). Client only plus one helper pulled out in
conker-rules (`game::can_mount`, `mounted_guns`' own gate, same behaviour). **No wire change**, no `PROTOCOL_ID` bump
from this half; the shoulder bit was already sent (C2).

**R2's leftovers:**
- The N64 reticle hides while aiming under MODERN (`hud::shown`: only without an orbit view).
- The radar's up and the audio listener follow the orbit camera's view (`Orbit::view`: eye and look of the frame it
  drew; none when Tank, Race, a mounted gun or the debug camera draws).
- The pad's X: a tap (< `HOLD` 0.25 s) is Reload, a hold Interact (`orbit::hold`, `shared_x`: only while Reload and
  Interact share a pad button). Presses shorter than a 30 Hz tick (Jump, Fire, Reload, Interact, Grenade) are latched
  in `Orbit::taps` and sent once, so a quick tap (the grenade's included: the rules' press latch throws it) is never
  lost between ticks; a held Grenade stays held, as on the keys.
- The hit marker gets a black outline under it (`crosshair::outline`): readable over held fire's muzzle flash.
- "PRESS F TO MOUNT" next to a free mounted gun (`hud::mount_prompt` on `game::can_mount`, the server's own gate).
- One key-label helper, `hud::key_label` (`interact_label` gone): the bound key, or the pad button in its glyphs.
- Settings: a DEFAULTS button next to APPLY / BACK resets the shown tab (its bindings and values; live, not saved
  until APPLY; `settings_ui::reset`).

**Masked upper-body aim** (`upper.rs`, new; `render.rs`'s `load_characters` and `pose_players`):
- Each character's graph gets two mask groups (`joint_01`'s tree; the rest) and masked twins of the gaits (plain and
  armed) and of the guns' idle / fire / aim / reload / draw / holster clips (`add_clip_with_mask`).
- The local modern player with a gun on foot, aiming (R, drawn) or in a gun action, plays the legs' clip (its gait,
  else its crouch or jump, else the gun's stand) under the upper body's: the weapon's aim clip (sniper rifle,
  crossbow), else its fire clip's first frame (arms out), or the action itself. Crouched unaimed it shows the gun's
  stand over the plain crouch (§9's missing armed crouch).
- `upper::bend_upper` (PostUpdate, after the clips) pitches the spine by 0.35 of the aim pitch and the shoulders
  (`joint_02`, `joint_07`) by the rest, so the guns follow the aim ray below the crosshair instead of crossing it.
- Other players are unchanged (their aim pitch is not on the wire). The server's hit spheres stay posed from the
  rules' single clip: the bend moves the drawn head about 2.5 units at ±12° and 17 at the ±80° limit (`head_drift`),
  inside the 20-unit head sphere; arms are no hit part. On top of that, whatever `joint_01` differs between the
  upper body's clip (aim pose, fire's first frame) and the rules' whole-body clip: not measured.

**Shoulder swap:** the camera eases across (`ease_side`: exponential, 0.05 s time constant, settled in ~0.2 s)
while the aim ray, the crosshair's point and the server's ray switch at once (the SHOULDER bit; scripts set it from
the actions column).

**Pad feel** (`orbit.rs`):
- Move stick radial: nothing inside the dead zone (0.10 default, C2b's setting), full past 0.95, linear between;
  Southpaw swaps the sticks and their dead zones. The look stick keeps C2b's squared curve.
- Acceleration: 0.3 s at the look stick's edge ramps the turn to 1.75× over 0.25 s (`ramp`).
- Aim assist, pads only: the look turns at 0.4× while the server's look ray crosses a living enemy's body
  (`over_body`, short of the first wall; teammates excluded in team modes). The mouse never.
- Vibration (Settings: Vibration): a hit taken (strong 0.6, 0.18 s) and a shot fired (weak 0.3, 0.06 s) through
  Bevy's `GamepadRumbleRequest`. gilrs has no force feedback in the browser or on macOS: native Windows / Linux only.

**Glyphs:** the pickup and mount prompts name the pad's button (A / X / SQUARE…, C3's vendor detection and Button
prompts setting) while the pad is the device last used (`bindings::UsingPad`), else the key; the settings sheet's
controls line and the rebind box's cancel line use the same glyphs.

**Tests** (10 new): the X tap / hold and short taps reaching a tick; the shoulder ease and the instant ray; the
edge ramp, the assist's slowdown and the rumble levels; the mount prompt's gate and the glyph labels; DEFAULTS per
tab; the sheet's hint in both glyph sets; the marker's outline; the masked twins, the layer choice, the bend's angles
and the head drift. `bun run test`, `bun run check` and the wasm clippy pass.

**Screens** (`../lab/screens/controls-c4/`, `shots.sh`, native offscreen on port 5974): `aim.png` (aimed at
player 2, the guns below the crosshair), `aim-side.png` / `aim-up-side.png` / `aim-strafe-side.png` (the debug
camera from the side: aimed at −12°, at +24°, and strafing with the run's legs under the aim), `crouch-side.png`
(the armed crouch), `swap.png` (over the left shoulder, the crosshair centred), `mount.png` (0x30's gun A:
PRESS F TO MOUNT).

**Left / unverified:** real pads (the X tap / hold, the ramp and assist tuning, rumble on Windows / Linux) and the
browser; offscreen runs keep default settings, so the PlayStation glyphs are only unit-tested. No cross-fade into or
out of the layered pose (it switches in a frame). Other skeletons than the 28-joint bodies bend their spine only.

## Controls C4: spread and the rules' leftovers (2026-10-15)
**Lane C4-rules** of `../CONTROLS_RESEARCH.md` (§3 "Accuracy", §9, §10). Shared simulation (conker-rules,
conker-sim) and one mirror in the client's `orbit.rs`. Everything is MODERN-gated (bots never set it): the full
`bun run test` passes with no N64-path expected value edited. Two modern tests changed, each by the item that asked
for the change (below).

**Spread** (`conker-rules/src/spread.rs`, new): under MODERN a shot leaves along its aim, turned within a cone.
- `cone(f)` gives the cone: the hip's, a tighter one aimed (R held), none through a still scope, and up to
  `SCOPE_MOVING` in proportion to the scope's speed (full at 0.5 × 35). It is 0 without MODERN.
- `scatter` turns the shot by an offset hashed from (tick, slot, `Fighter::shots`) with lowbias32: a uniform radius
  and angle (CS's, denser at the centre), `cos_sin` and `sqrt` only.
  - It never draws the match's RNG, so the server and the prediction agree bit for bit.
  - With no cone it returns the direction itself, so bot shots don't change.
  - A player fires at most one shot a frame (the twin guns' muzzles fire 2 frames apart), so the shot count tells
    the shots apart.
- Damage, rate of fire and ammo are untouched.
- Where the cone applies:
  - the bullets (twin guns, pistol, 0x23, tommy gun): the aim ray is turned before its hit point is found;
  - the sniper's ray;
  - aimed shots (the crossbow through the scope, 0x14);
  - the hip bolt;
  - a mounted gun's sight.

  `Match::aims` keeps the true aim, so the crosshair, the muzzle check and the laser are unaffected.
- **The hip bolt no longer locks on under MODERN.** The lock-on (`lock_on`, a ±28° cone) would leave a spread
  cone meaningless, so the bolt goes from the muzzle toward what the turned aim ray meets, as the bullets do. Bots
  keep the lock-on. This changes behaviour beyond spread.

| Weapon | Hip | Aimed (R) | Scope |
|---|---|---|---|
| twin guns 0x09 | 4.5° | 1.6° | – |
| pistol 0x39 | 3.5° | 1.0° | – |
| tommy gun 0x38 | 5.0° | 2.0° | – |
| 0x23 | 3.5° | – (R held starts nothing, as the original) | – |
| crossbows 0x18, 0x41; bone knives 0x3F | 7.5° (the hip bolt) | – | 0° still, up to 10° moving |
| sniper 0x24 | – | – | 0° still, up to 10° moving |
| mounted gun (on the aim) | 1.0° | | |

The other weapons have no cone (the shotgun's spheres, the flame, rockets, blades). The constants are `ponytail:`
guesses, tuned on the harness below.

**Headshot rate** (`spread_hits_and_headshots_per_100_shots`, `#[ignore]`d; run it with `cargo test --release -p
conker-rules --lib spread -- --ignored --nocapture`, 0.4 s; it is slow in debug):
- A scripted MODERN shooter with a perfect mouse aims at the centre of a posed Tedi's head (the pack's poses, as the
  server's), 100 shots a cell.
- The target is put back every frame: 2 health, so a head hit is a type-0x2E kill (a headshot); never immune or
  stunned; one idle pose.
- Strafing: the target goes ±200 at 35 a frame, and the shooter leads it by its script and the flight time.
- "Scoped, A-D": the shooter strafes left and right every 20 frames while scoped.
- The tommy gun fires 8-frame bursts. 0x23's and the tommy gun's bullets have no head by the rules (the cylinder).

Each cell is hits / headshots per 100 shots, before → after (before: this tree without spread; the crossbow's hip
bolt locked on):

| Weapon, mode | 300 | 800 | 1,500 | 300 strafing | 800 strafing | 1,500 strafing |
|---|---|---|---|---|---|---|
| twin guns, hip | 100/100 → 91/89 | 100/100 → 48/28 | 100/100 → 24/11 | 41/17 → 35/21 | 52/5 → 30/7 | 42/3 → 21/5 |
| twin guns, aimed (R) | 100/100 → 100/100 | 100/100 → 94/92 | 100/100 → 71/58 | 41/17 → 39/18 | 52/5 → 41/13 | 42/3 → 47/14 |
| pistol, hip | 100/100 → 100/100 | 100/100 → 76/63 | 100/100 → 42/30 | 76/76 → 69/49 | 100/100 → 68/41 | 86/86 → 38/23 |
| pistol, aimed (R) | 100/100 → 100/100 | 100/100 → 100/100 | 100/100 → 95/88 | 76/76 → 75/69 | 100/100 → 97/67 | 86/86 → 70/54 |
| 0x23, hip | 100/0 → 92/0 | 100/0 → 61/0 | 100/0 → 35/0 | 94/0 → 88/0 | 100/0 → 68/0 | 100/0 → 46/0 |
| tommy gun, hip | 100/0 → 78/0 | 34/0 → 28/0 | 34/0 → 8/0 | 70/0 → 65/0 | 100/0 → 46/0 | 48/0 → 10/0 |
| tommy gun, aimed (R) | 100/0 → 87/0 | 34/0 → 50/0 | 34/0 → 26/0 | 70/0 → 68/0 | 100/0 → 83/0 | 48/0 → 32/0 |
| sniper, scoped, still | 100/100 → 100/100 | 100/100 → 100/100 | 100/100 → 100/100 | 100/100 → 100/100 | 100/100 → 100/100 | 100/100 → 100/100 |
| sniper, scoped, A-D | 100/100 → 57/48 | 100/100 → 50/41 | 100/100 → 36/32 | 100/100 → 80/73 | 100/100 → 56/45 | 100/100 → 39/35 |
| crossbow, hip | 100/0 → 66/46 | 100/0 → 33/20 | 100/0 → 15/12 | 5/0 → 33/13 | 1/0 → 18/7 | 6/0 → 9/3 |
| crossbow, scoped, still | 100/99 → 100/99 | 100/99 → 100/99 | 100/99 → 100/99 | 99/99 → 99/99 | 99/99 → 99/99 | 99/99 → 99/99 |
| crossbow, scoped, A-D | 100/99 → 75/67 | 100/99 → 54/46 | 100/99 → 33/25 | 88/88 → 86/79 | 91/91 → 51/35 | 99/99 → 33/26 |

What the measure shows:
- Without spread, every modern bullet weapon put 100 of 100 on a standing head at any range. The N64's level twin-gun
  bullets at 41 up hit the body only.
- With spread, hip fire is reliable at 300 and falls off past 800, and aiming keeps the twin guns and the pistol
  deadly to mid range.
- A still scope stays exact (CS's AWP); A-D in the scope costs about half the hits.
- The strafing columns are low even before: see "Left".

**The shoulder ray against a wall:** `movement::aim_ray(pos, input, world)` starts the ray `aim_reach(…)` out from
the pivot.
- That is `AIM_SHOULDER`, or `AIM_WALL_GAP` (8) short of a wall nearer, found with `CollisionWorld::ray`, the line
  query the client's arm already uses.
- With no wall the start's bits are the same as before.
- The client's `orbit::arm_at` calls the same `aim_reach` (for the eased shoulder too), so the eye's line stays the
  ray's, and `crosshair.rs` gets it through `arm`.

**A grenade tap during a draw** is latched: the press now counts with a weapon drawn or being drawn
(`Action::Draw`), and `redraw` arms once the latched throw has a weapon drawn (`redraw |= throw && drawn`). The
draw finishes, then the weapon holsters, the grenade is thrown, and the weapon is drawn again.

**Fire in the draw's last frame:** `pad(f, settings, input, t)` counts a draw's last frame (`t − since ≥ draw`) as
drawn.
- The rules end a timed action before a new one starts (V ctlw_k5: B in a reload's last frame holsters), and the N64's
  Z held through a draw fires in that frame. So FIRE held now fires there too, one frame earlier than before.
- The same row also fixes JUMP there: it no longer gives A for a weapon that doesn't jump, which used to start a
  reload. GRENADE still waits for the frame after.

**Tests** (headless; 7 new, 1 ignored):
- `a_wall_at_the_shoulder_pulls_the_aim_rays_start_in` (sim): a wall 10 out pulls the start to 2; the other shoulder
  keeps its 20.
- `a_wall_at_the_shoulder_pulls_the_arm_in_as_the_aim_ray` (client): the arm's start equals `aim_ray`'s, bit for bit.
- `modern_grenade_tapped_during_a_draw_throws_after_it`: Draw, Free, Holster, Free, Pull, Hold, Throw, Free, Draw,
  Free; 2 grenades left and one in flight. It fails on the old latch.
- `modern_the_draws_last_frame_counts_as_drawn`: the twin guns' fire starts in the draw's last frame; JUMP there jumps
  and doesn't reload. It fails without the change.
- `the_cone_is_modern_only_and_tightens_aimed_and_scoped`: no cone without MODERN; hip, then aimed, then still scope,
  half speed, mounted gun.
- `the_scatter_fills_its_cone_from_tick_slot_and_shot`: within the cone, all 4 quarters reached, deterministic.
- `spread_hits_and_headshots_per_100_shots` (ignored): the table above.

Existing tests:
- Expected values changed:
  - `modern_fire_draws_and_fires_once_ready`: the shot comes on frame `draw` instead of `draw + 1` (item 4).
  - `modern_bullets_go_to_the_aims_hit_point`: the line meets the wall where the turned ray does (`spread::scatter`
    of that frame and shot), and the test checks that the turn moved it (item 1).
- Call sites only: `pad`'s `t` (3 places, `0`) and `aim_ray`'s world (C2b's arm test). Every replay, rules and bot
  test is otherwise unchanged.

**Wire and shared simulation:**
- No wire change: `Input` and `Fighter` keep their fields, and `PROTOCOL_ID` is untouched.
- The shared simulation changes for MODERN players only:
  - shot directions (spread);
  - the hip bolt's aim;
  - the aim ray's start by a wall;
  - `pad`'s draw-end frame;
  - the grenade latch.
- A client and server of different trees would mispredict modern shots, so it is worth a protocol bump with the next
  round.

**Left:**
- **A lead into empty space misses by the muzzle's offset.** The aim ray converges on what it meets, so a lead (or
  any shot past a body's edge) converges on the far wall, and the bullets run parallel to the crosshair, offset by the
  muzzle (±20 for the twin guns). That is why the twin guns' strafing column is ~40 even without spread. A fix would
  converge at a set distance or the nearest body near the ray.
- **The tommy gun's climb** (up to 30 steps × 200 units ≈ 33°) lifts its bullets off the crosshair, where the N64's
  reticle climbed with them. Under MODERN it may want to become a camera kick (client) or be scaled.
- The sniper's aimed ray doesn't count in `Fighter::shots` (the Aim branch starts the fire itself; a pre-existing
  accuracy stat). Its spread hash varies by the tick.
- Not built:
  - a movement or jump cone outside the scope;
  - bloom over sustained fire;
  - a crouch bonus;
  - the cone drawn on the crosshair (`spread::cone(f)` gives it, in degrees, for the client).
- The pistol's head needs the server's posed parts: without the pack (the replays) and in the client's prediction it
  has none, as before.

## Controls C3: the Esc menu, settings and rebinding (2026-10-15)
**Lane C3** of `../CONTROLS_RESEARCH.md` (§5, §6, §7, §10). Client only, **no wire change** and no simulation change:
the menu only holds the input `buffer_input` already sends while paused.

**`menu.rs`** (new): RESUME / SETTINGS / LEAVE MATCH over the running match, drawn as the HUD's own quads and font
(`hud.rs` draws `menu::Quads` after its own, z 10-13).
- Opens on Esc, the pad's Menu (Start), natively the window losing focus, in the browser the pointer lock's loss
  (C1's `Paused`). Only while playing, never over the results, which close it.
- While open the menu holds `pointer::Paused` every frame. That one flag gates everything: `buffer_input` sends a
  neutral stick and no buttons with the yaw kept, `Controls` sees no devices (team screen, H, M, F8, the pad's
  look), `orbit::look` stands still, and the native cursor is freed (grabbed again on resume). The menu closes the
  team screen when it opens.
- It stays open until the closing press is let go (`Page::Closing`), so a RESUME click fires no shot. Natively the
  click that refocuses the window is not the menu's.
- RESUME: natively at once. In the browser a click or key asks for the pointer lock and the menu closes once it holds.
  `pointerlockerror` or a rejected promise keeps it open ("CLICK RESUME AGAIN"). The pad resumes without the lock.
  `pointer.rs`'s pointerdown no longer locks while the menu is open, so clicks on the menu stay clicks.
- LEAVE MATCH: natively `AppExit` (`connection::leave` disconnects first). In the browser netcode's `Disconnect`,
  then `index.html` after `LEAVE_DELAY`.
- Navigation: mouse hover and click on the last frame's rectangles (`Quads::hit`, `hud::frame_point`); arrows, Enter,
  Esc, the wheel; pad D-pad or left stick, A, B, LB / RB for the tabs; held directions repeat (0.4 s, then 0.08 s).

**`settings_ui.rs`** (new): the settings screen's data.
- Tabs Keyboard & Mouse, Controller, Mouse/Look, Gameplay. Rows are grouped Movement / Combat / Camera / UI. Every
  §5 v1 setting is there with its range and step: sensitivities, aim multipliers, invert Y, dead zones, stick layout,
  hold/toggle per device, crosshair colour and size, button prompts, vibration.
- Changes apply live. APPLY saves (`settings::save`: `CONKER_SETTINGS` natively, localStorage in the browser; never
  offscreen). BACK, B or Esc drops what wasn't applied.
- Rebinding: "PRESS A KEY OR BUTTON - ESC CANCELS, BACKSPACE CLEARS" with a 5 s countdown.
  - The opening press is ignored until everything is released, and a trigger counts once past 0.5.
  - Mouse 1-5 and the wheel can be bound; Esc and the pad's Menu never can.
  - Keyboard slots take keys and mouse, the controller slot takes pad buttons.
  - A conflict asks SWAP / REPLACE / CANCEL. The pad's X keeps Reload and Interact together, as the defaults do.
- Labels show the current bindings: keys through C1's `KeyLabels` (AZERTY's logical letters), pad buttons as Xbox or
  PlayStation glyphs (vendor 054c / 045e, else the name, else Xbox; the Button prompts setting overrides).

**Hooks in shared files:**
- `bindings.rs`: `Controls::devices` is empty while paused.
- `pointer.rs`: `lock`, `locked`, `refused`; the native grab follows `Paused`; the web lock loss sets `Paused` and
  the menu clears it.
- `hud.rs`: `text` is `pub(crate)` and the menu's quads are drawn.
- `lib.rs`: the modules, `menu::build`, and the `keys=READ:KEY,…` option, which presses keys at script reads for
  screenshots.
- `web/play.html` and `web/README.md` describe the menu.

**Tests** (16 new, `menu::tests` and `settings_ui::tests`):
- the menu opens on each source and never over the results;
- the closing press and the browser's lock;
- `Controls` sees nothing while paused;
- focus through tabs, rows, scrolling and buttons;
- held repeats;
- rebinding: arming, timeout, Esc / Backspace / Start, mouse 4, wheel, pad, trigger threshold;
- conflicts: cancel, swap, replace, the X pair;
- APPLY saves and BACK drops (a file under `$CONKER_SETTINGS`'s directory);
- every field steps within its range;
- labels and glyphs;
- the pointer hits what is drawn (the pack's HUD at 1280×720).

**Screens:** `lab/screens/controls-c3/` (`shots.sh`: native, offscreen, 0x2D with no bots, keys pressed by
`--keys`).
- `menu.png`: the main page.
- `keys.png`, `pad.png`, `look.png`, `gameplay.png`: the four tabs.
- `keys-end.png`: Keyboard & Mouse scrolled to its end.
- `rebind.png`: Jump's primary waiting.
- `conflict.png`: W pressed for it, showing Move Forward's.

**Unverified / left:**
- Browser-only paths, with no browser here: the lock loss opening the menu, RESUME's relock and its refusal, the
  pointerdown gate, localStorage, LEAVE MATCH's navigation.
- Real pads: vendor glyphs, triggers, Start.
- AZERTY labels on a real layout.
- A pad's Start opens the menu while the browser still holds the lock; the mouse stays captured until Esc.
- Skipped: reset to defaults, per-context bindings (Tank and Race have no bindable actions of their own, so one
  context and conflicts are global), the `getLayoutMap` fallback.
- C2c's `hud::interact_label` duplicates `settings_ui::key_label`'s naming.

## Controls C2b: the orbit camera, the crosshair and hit markers (2026-10-15)
**Lane C2b** of `../CONTROLS_RESEARCH.md` (§3, §5, §10). Client only, **no wire change**: windowed clients now send
MODERN (C2a's fields, C2c's GRENADE and INTERACT). Headless clients and bots without a modern script line send the
N64 input as before, so every replay and oracle test is untouched.

**`orbit.rs`** (new): the `Orbit` resource holds the look (yaw, pitch in game units), the shoulder and the hold/toggle
latches.
- The arm is C2a's exactly: pivot 67.5 above the feet, `±20·right` (SHOULDER: left), 160 back along the look, pitch
  within ±`AIM_PITCH_MAX`. `camera_yaw` = aim facing − 0x4000, `aim_pitch` up positive. A test checks the start
  against `movement::aim_ray` bit for bit (`cos_sin` only).
- Collision slides the eye only along its look line (`CollisionWorld::ray` back from the start, 8 kept off the wall),
  so the centre of the view stays on the aim ray.
- Aiming (R held) zooms 1.5× (tangent ratio) over 0.06 s; the mouse's aim multiplier is FOV-normalised. A scoped
  weapon shows the scope's own view and sight from the aim (the rules point the scope there); the wheel / D-pad zoom
  it (C-up / C-down while aiming). The N64 follow camera keeps running underneath; Tank, Race, the raptor and the
  mounted guns keep their cameras and inputs (the mounted gun: the mouse moves its stick, as C1).
- Look: 0.022°/count × sensitivity, invert Y, pad 450°/s across and 270°/s up × sensitivity, radial dead zone
  0.10, outer 0.95, curve exponent 2, no acceleration or smoothing. Integrated per frame in `PreUpdate`, the mouse
  only while the cursor is grabbed (native; the browser's pointer lock gates it there), not on the team screen,
  paused or in the free camera.
- `orbit::input`: camera-relative WASD (the stick under `camera_yaw`), walk halves it, hold/toggle for aim, crouch and
  walk from Settings, the actions JUMP, CROUCH, FIRE, RELOAD, SHOULDER, INTERACT, GRENADE from the bindings, Holster
  on B. The look resets to the facing on a respawn.
- `lib.rs`'s `buffer_input` sends it (`actions | MODERN`, `aim_pitch`) and `render.rs` chains `orbit::camera` after
  `follow_camera`. Script lines take 3 optional columns (actions hex, look x and y in mouse counts), which turn a
  scripted client modern; `results.rs` reads JUMP for the results screen's A.

**`crosshair.rs`** (new): quads for the HUD's renderer (`hud.rs` draws them with its own, z 6.8).
- The centre crosshair in the Settings' colour and size, while alive, on foot and not scoped.
- The muzzle check: the weapon's muzzle (from its `Attack`) toward the point the aim ray meets; a wall first greys the
  crosshair and marks that wall's point with a dot in the crosshair's colour.
- Hit markers from the server's own counts (`Combat::hits`, `headshots`, already replicated; enemies only): a hit
  shows 12 squares on the diagonals for 0.3 s, fading, red for a headshot; a restart's reset shows none.

**Tests** (client, 9 new): the mouse's signs, invert and pitch clamp; the stick's zones and 450°/s; the aim
FOV and scale; the arm against `aim_ray` and the wall slide along the line; the shoulder's 40 across; the default
bindings' actions; toggles and the respawn reset; markers from the counts; a ledge blocking the muzzle; the script's
modern columns. The full `bun run test` passes with no existing expected value edited.

**Prediction** (`lab/screens/controls-c2b/predict.sh`: 2d, `--wait 2`, both given the twin guns; client 1 plays
`modern.txt` (strafes, aims, fires, walks aimed, jumps, crouches, turns with the mouse), client 2 `n64.txt`, the same
timeline on the N64 bits; smoke.sh's predicted-against-server test):

| Run | Client | Ticks | Max / mean error | State misses | Rollbacks (ticks) | Shots |
|---|---|---|---|---|---|---|
| no lag | modern | 1315 | 0 / 0 | 0 | 2 (8) | not counted |
| no lag | N64 | 1320 | 0 / 0 | 0 | 2 (6) | not counted |
| `LAG=75` | modern | 1298 | 0 / 0 | 0 | 2 (16) | 100 |
| `LAG=75` | N64 | 1300 | 0 / 0 | 0 | 2 (18) | 100 |

The server's states show the modern client strafing at ~10.45 a tick while aiming, where the N64 client holding R
stands still.

**Screens** (`lab/screens/controls-c2b/shots.sh`, native offscreen, 2d at e_w09's places, the other player idle 146.5
ahead):
- `orbit.png`: the arm over the right shoulder, the green crosshair on the target.
- `aim.png`: R held, zoomed, the target centred.
- `hit.png` (`hit.txt`, one FIRE tap): the marker's white ticks round the crosshair, 10 reads after the server's hit.
- `scope.png`: the sniper's scope from the aim, zoomed with C-up.
- `blocked.png`: on 2d's tower aiming over the parapet, the crosshair grey and the muzzle's wall marked below it.

**Left**:
- Browser-only parts unverified (pointer lock and the look's gating there; no `web/dist` build here).
- The N64 reticle still shows while aiming, under the crosshair; the radar's "ahead" and the audio listener still
  follow the N64 camera.
- The pad's X sends both RELOAD and INTERACT (no tap/hold split); Southpaw, the look acceleration ramp and aim assist
  are C4's; the mounted gun's mouse aim stays C1's stopgap.
- The shoulder's ray start can sit inside a wall; the muzzle check tests walls only (not bodies).
- Seen in the screens: under held fire the white marker is lost in the muzzle flash, and while aiming the held gun
  crosses the view's centre (the upper body isn't posed to the aim: C4's masked animation).

## Controls C2c: weapon handling (2026-10-15)
**Lane C2c** of `../CONTROLS_RESEARCH.md` (§1 decisions 8-24, §4b, §10). Shared simulation (conker-rules, conker-sim)
and the client's "PRESS F" prompt. Everything new is gated on MODERN (or runs on the new fields, which stay 0 without
it): the full `bun run test` passes with **no existing test or expected value edited**, C2a's `pad()` test included.

**`Fighter`** (`conker-rules/src/game.rs`) gains `actions` and `late_actions` (this frame's and last frame's
`movement::action` bits, as `buttons` / `late`, set right after `pad()` in `Match::player`; `in_body` keeps the slot's),
and two latches, `throw` and `redraw`. The latches are set and cleared just after `pad()` reads them:
- `throw`: GRENADE pressed with a weapon drawn and a grenade left; ends with the pull, a hit or no grenade left.
- `redraw`: that same press, or a MODERN player's weapon pickup arriving; ends with the draw, a hit or no weapon.

**`pad()`**: C2a's rows are unchanged. New rows, all "free" = no action under way, no flag or canister carried (B puts
a flag down). Its B comes only after a frame without B, so a press refused (in the water, B is masked) comes again:

| Action | Drawn | Not drawn | Mounted gun |
|---|---|---|---|
| FIRE | Z (none while a grenade is asked for) | B if a weapon is held, free (draws it) | Z |
| GRENADE (held, or `throw`) | B, free (holsters) | C-down while held; a tap's `throw`: one frame, free | – |
| `redraw` | – | B, free, not while `throw` | – |
| B (Q) | B | B | A (off the gun) |

Per decision:
- **8, 9: jump and crouch armed.** Under MODERN the rules give the movement A and Z from JUMP and CROUCH directly,
  armed or not, instead of stripping them while drawn. The trigger (Z) and the reload (A) stay `pad()`'s:
  - the reload's `pressed(A)` now only comes from RELOAD, so "the reload reads RELOAD";
  - this keeps C2a's table (and its test) as it was.
  - conker-sim (`movement.rs`) lifts its armed gates under MODERN:
    - the high jump;
    - `ground`'s crouch Z;
    - `crouch_state`'s armed return (reachable only through `ground`'s Z, so it was dropped).
  - The armed animation match gains `CROUCH | CROUCH_WALK | HIGH_JUMP` under MODERN: the unarmed clip, the gun in
    hand.
  - The hit volumes follow from the state: the crouched cylinder, and the spheres posed from that clip.
- **10, 22, 24: Fire draws.** FIRE with a holstered weapon is a B press (the draw refills the magazine, as the
  original's B). FIRE still held fires the frame after the draw ends: `pad()` reads `drawn` at the frame's start.
- **11: draw on pickup.** The weapon arrives as today (2 frames after the touch), `give()` unchanged, and `redraw` makes
  `pad()` press B in that same frame. A pickup arriving during a hit is not drawn afterwards: the hit clears `redraw`.
- **12, 18: manual pickup.** `game::can_take(settings, gives, pos, fighter, reach, countdown, interact)` is the whole
  touch test, moved out of `pickups()`.
  - `interact` is `None` without MODERN (as before), else whether Interact was pressed this frame:
    `inputs[i].actions` against `Fighter::actions`, still last frame's since `pickups()` runs before `pad()`.
  - Only `Gives::Weapon` waits for it, and not Tank's, Race's or Beach's item 0x3E. Grenades and masks stay automatic,
    and so do the canisters, flags, bag, eggs and bomb, which have their own touches.
  - Raptor's bone weapons wait (cavemen only, as before).
- **13, 23: grenade armed.** A tap holsters (22 frames for the twin guns), presses C-down for one frame (an N64 tap),
  pulls, throws and draws again. A held G cooks as long as it is held.
- **19: mounted guns.** Mounting by touch needs an Interact press under MODERN (`actions` against `late_actions`, after
  `player()`). B gets off, as A. Seated, `modern_aim` sets the gun's yaw and pitch from the aim (pitch within the N64
  gun scope's −60..50) instead of the scope camera.
- **21: move while drawing or holstering.** The draw's and holster's hold (`lock = 0xFF`, idle on the end frame) is
  N64 only; the shotgun's shot still holds.
- **R1's leftovers.**
  - Under MODERN the pistol's, 0x23's and the tommy gun's bullets go from the muzzle to the aim ray's hit point, as the
    twin guns' do (`strike`). Bots keep the pitch-only path, as traced.
  - The C buttons no longer strafe under MODERN (`ground`'s mask is 0), and the armed C-button run (`Action::CButton`)
    no longer starts.

**The prompt** (`conker-client/src/hud.rs`):
- `pickup_prompt` shows "PRESS F TO PICK UP" under the centre while `can_take` is false without the press and true
  with it, for a weapon's pickup that is shown (`MatchView::taken`, placement < 64) and roaming pickups where
  `MatchView::roaming` has them.
- Its inputs: the pack's pickups (`hud::build` takes them; Raptor's from `Nest`), the predicted `Fighting` and a human's
  reach.
- The key is Interact's first key or mouse slot, uppercase: as the layout types it once pressed (`KeyLabels`), else the
  `KeyCode` without `Key` / `Digit`, else the pad button's name.
- `interact_label` is private to `hud.rs`; C3's rebinding screen may want a shared one.
- The prompt shows whatever the client sends: decision 3 makes every human MODERN.

**Tests** (headless; 8 new):
- Rules (`game.rs`):
  - `modern_weapon_pickups_wait_for_interact_and_draw`: no take without the press while a MODERN player on the grenades
    takes them; `can_take` agrees with and without the press. One press of a held F takes the twin guns, whose draw
    starts on arrival and refills them.
  - `modern_fire_draws_and_fires_once_ready`: the pistol with 0 rounds, FIRE held: one shot on frame `draw + 1`, the
    magazine full less one.
  - `modern_jumps_and_crouches_armed`: twin guns drawn. JUMP jumps (no reload) and RELOAD reloads. CROUCH crouches with
    the crouch clip, the guns drawn, and a head-height shot passes over. FIRE fires crouched, and CROUCH + JUMP is the
    high jump.
  - `modern_grenade_holsters_throws_and_draws_again`: one frame of GRENADE gives Holster, Free, Pull, Hold, Throw,
    Free, Draw, Free, then 2 grenades left and one in flight.
  - `modern_moves_while_drawing`: a forward stick moves through the twin guns' draw only under MODERN.
  - `modern_mounted_gun_on_interact_off_q_turned_by_the_aim`: no mount by touch, a mount on F. Seated, the aim turns
    it to 45° and −22.5°, and 67.5° down is clamped to 50. Q gets off.
  - `modern_bullets_go_to_the_aims_hit_point`: pistol, 0x23, tommy gun. The first bullet's line meets a wall 3,000
    ahead at the ray's hit point, 20 to the shoulder's side. It fails with the old pitch-only path.
- Sim: `modern_crouches_and_high_jumps_armed_and_leaves_the_c_strafe`.
- Client: `the_pickup_prompt_follows_the_gate_and_the_binding`.

**Wire and shared simulation:**
- `Fighter` (replicated in `Fighting`, serde) gains `actions`, `late_actions`, `throw` and `redraw`, so the protocol
  needs its bump. `PROTOCOL_ID` is untouched here.
- `Input` is unchanged.
- `Match::pickups` takes the frame's inputs.
- conker-sim's `ground`, `step_among` and `crouch_state` change only under MODERN.

**For the client (C2b):** under MODERN, send Grenade as `action::GRENADE` and Interact as `action::INTERACT`. Stop
sending C-down for Grenade, and C-left / C-right at all (the scope's zoom may keep C-up / C-down while aiming).
C2b's `orbit::input` already does all of this.

**Left:**
- No prompt for the mounted gun's F.
- A grenade tapped during a draw is lost (the latch keys on a drawn weapon).
- FIRE held through a draw fires a frame after the original's draw ends.
- An automatic weapon's trigger let go for a grenade takes its 7-frame tail before the holster.
- The pad's X tap/hold split (Reload vs Interact) is the client's.
- No browser check (the prompt's look and the key label on AZERTY are unverified).

## Controls C2a: modern rules and the aim on the wire (2026-10-14)
**Lane C2a** of `../CONTROLS_RESEARCH.md` (§3 "Aim on the wire", §4, §10). Shared simulation (conker-sim, conker-rules)
and the wire; the client is not wired to MODERN yet (round 2). Everything new is gated on the MODERN bit: **with
`actions == 0` nothing changes**, and every replay, rules and bot test passes with no expected value edited.

**`Input`** (`conker-sim/src/movement.rs`): two new fields, `actions: u8` and `aim_pitch: i16` (`Default` as before).
- `buttons` bits read anywhere, re-verified: A, B, Z, R and the C buttons only. The new actions stay out of `buttons`
  (the oracle traces set L and the D-pad).
- `movement::action` bits:

  | Bit | Action | Bit | Action |
  |---|---|---|---|
  | 0x01 | JUMP | 0x10 | SHOULDER (left shoulder; clear = right) |
  | 0x02 | CROUCH | 0x20 | INTERACT (reserved, C2c) |
  | 0x04 | FIRE | 0x40 | GRENADE (reserved, C2c) |
  | 0x08 | RELOAD | 0x80 | MODERN (gates everything new) |

  - Ten candidates don't fit a byte. **HOLSTER rides the N64 B bit** (its meaning in every context: draw/holster, the
    flag's put-down, the bag's throw; C1 already sends B for it); the seat's dismount on Q can be a `pad()` rule in C2c.
    **WALK needs no bit**: the client halves the stick, as today.
- `aim_pitch`: the game's angle units (0x4000 = 90°), **up positive**, read within ±`AIM_PITCH_MAX` (14563 = 80°, the
  bots' scope's limit). **The aim's yaw is `camera_yaw`'s: the aim looks along the facing `camera_yaw + 0x4000`**
  (`aim_yaw`), where a forward stick moves. Round 2's client sends `camera_yaw = aim facing − 0x4000`.

**`pad(&Fighter, &Settings, &Input) -> Input`** (`conker-rules/src/game.rs`), at the top of `Match::player` (after the
death's weapon swap, before the Tank / Race / Raptor dispatch), so the server and the prediction run it alike. Its first
line returns the input unchanged when `actions == 0`. It only adds bits: R, the C buttons and B pass through.

| Action | On foot, a weapon drawn | On foot, none drawn | Mounted gun, raptor, Tank, Race |
|---|---|---|---|
| JUMP | A if the weapon jumps (else A reloads) | A | A (off the gun, the jump, Tank's boost) |
| CROUCH | – (Z fires) | Z, not during a draw | – |
| FIRE | Z | – (C2c: B draws) | Z |
| RELOAD | A if the weapon reloads | – | – |

No weapon both jumps and reloads, so JUMP and RELOAD never share A. CROUCH stays off a draw: a Z held as it ends fires.

**The aim ray** (`movement::aim_ray`, filled by `Match::modern_aim` into `m.aims[i]` each MODERN frame, after the
server's `bots.scopes()`; never cleared without MODERN, since that slot may be a bot's). The existing bot-ray code then
sends the twin guns' bullets from the muzzle to the first body part or wall on it, and the pistol's, 0x23's and the
tommy gun's along its pitch.
- The camera arm, which round 2's orbit camera mirrors:
  - `AIM_PIVOT_HEIGHT` = 67.5 above the feet (the eye the bots' rays start from).
  - `AIM_ARM` = 160 behind the pivot along the look.
  - `AIM_SHOULDER` = 20 to the right (`right` = forward × up = `(sin yaw, 0, cos yaw)`), to the left with SHOULDER.
  - Collision only slides the camera along its look line.
- The ray starts at the point of the camera's look line nearest the weapon (Lyra's `CameraTowardsFocus`). The pivot
  stands in for the weapon (Lyra takes the pawn's location), so the start is `pivot ± 20·right`, whatever the arm's
  length. That start lies inside the shooter's own body: every hit test along the ray already leaves the owner out.
- `cos_sin` only: native and wasm agree bit for bit.

**The MODERN gate in the movement** (`step_among`, `ground`):
- R's aim no longer centres the stick. While R is held the stick's target is ×0.75 (`AIM_SHARE`), through the scope
  ×0.5 (`SCOPE_SHARE`) instead of the 0.1 scope drive. It keys on R held, not the aim camera, which shuts in the air
  and only restarts on a new press.
- `aim_facing`: while R is held, through the scope, or while FIRE is held with a weapon drawn (not swimming), the
  facing is the aim's. The facing no longer chases the move angle, the stick moves the player under it (a strafe),
  the twist is off, a root-motion step and a crouch-walk go along the move angle, and the air's turn is undone.
- Hip-fire runs at full speed (the shotgun's shot still holds the player, as the original's).
- The rules snap the facing too at the frame's start (not hurt, not on a mounted gun), so the press frame's shot, the
  lock-on and the muzzles already use it.
- The scope's yaw and pitch come from the aim (`modern_aim`, on foot; a mounted gun keeps its own turn, C2c).

**Tests** (headless; 7 new):
- `pad_maps_the_actions_and_is_the_identity_without`: 2,000 random inputs with `actions == 0` × 42 fighter states × 3
  modes give the identity; then the table above.
- `the_modern_aim_strafes_at_three_quarters_and_the_scope_at_half` (sim): a right stick with R aims strafes at 26.25
  (0.75 × 35), through 0x14's scope at 17.5, facing the aim, moving sideways. Without MODERN, R still stops the player.
- `modern_hip_fire_faces_the_aim_at_full_speed` (sim): FIRE held runs sideways at 35 facing the aim; let go, the facing
  turns back to the run.
- `modern_bullets_follow_the_aim_pitch`: the twin guns hit a Tedi on a ledge 415 above, and one 415 below the shooter's
  ledge, through `aim_pitch`. Level shots miss both.
- `the_shoulder_moves_the_modern_aim_ray`: the start at +20 z, or −20 with SHOULDER, for an aim along +x. Nothing is
  aimed without MODERN.
- `the_modern_aim_ray_leaves_out_the_shooters_own_body`: aimed 80° down the ray crosses the shooter's own torso
  sphere, `first_part` leaves it out, and the twin guns fired along it hurt nobody.
- `the_modern_scope_looks_along_the_aim`: the scope's 45° and −22.5° come from the aim, not the camera's.
- The full `bun run test` passes with zero edits to existing expected values. The only edits to existing tests add
  `..Default::default()` to `Input` literals that listed every field.
- `bots.sh` (Deathmatch 0x2D, 2 bots, 1 idle human, 150 s), this tree on ports 5974 and 5976 and an export of HEAD
  (`git archive`, its own target) on 5978, give the same result:
  - Bot 1: 17 hits and no death, the known FAIL line (a bot that never dies). Bot 2: 10 hits, 1 death and respawn.
  - Every other check is ok.
  - The servers' high jumps and lifts are the same events at the same positions, 23 ticks later at HEAD (the
    client joined later).

**Wire and shared simulation:** `Input` gains `actions` and `aim_pitch` (lightyear's native input), so the protocol
needs its bump (`PROTOCOL_ID` untouched here). The new movement and rules paths run only under MODERN.

**Left for round 2:**
- The pistol's, 0x23's and the tommy gun's bullets keep the facing's yaw and take only the ray's pitch, so they run
  parallel to the crosshair, offset by muzzle − start (about 20 sideways). The twin guns converge on the hit point.
- The C buttons still strafe under MODERN. They are kept for the cameras; the modern client should not send
  C-left / C-right.
- A mounted gun under MODERN still turns with the stick through the N64 scope camera.

## Controls C1: bindings, settings and pointer lock (2026-10-14)
**Lane C1** of `../CONTROLS_RESEARCH.md` (§5, §8, §10). Client only (and the play page's help). No wire change,
`PROTOCOL_ID` untouched, no shared simulation touched. The client still sends the N64 `Input`, built with
`..Default::default()` so C2a's new fields stay zero.

**Bindings as data** (`bindings.rs`, new)
- `device_input`'s hard-coded tables are now a `Bindings` resource: 20 named actions (MoveForward/Back/Left/Right, Fire,
  Aim, ScopeZoomIn/Out, Jump, Crouch, Reload, Holster, Interact, Grenade, ShoulderSwap, Walk, Scoreboard, Mute, TextHud,
  DebugCamera). Each has two keyboard/mouse slots (keys, mouse buttons 1-5, wheel up/down) and one pad slot.
  - Move is four actions because a slot holds one key. The sticks and the mouse look are not slots.
  - Esc, mouse buttons past 5 and the pad's Start (Menu) never bind.
  - Inputs are stored by name (`KeyW`, `MouseBack`, `WheelUp`, `RightTrigger2`), through Bevy's reflection: bevy's
    `serialize` feature stays off.
- The defaults are §7's: Crouch on C, Walk on Left Shift, the debug camera on **F8**.
- Until round 2's MODERN, each action sends today's N64 bit:
  - Fire and Crouch → Z, Jump and Reload → A, Holster → B, Aim → R, Grenade → C-down.
  - The zoom actions → C-up / C-down, **only while Aim is held**: unaimed, wheel-down would throw a grenade.
  - Interact, ShoulderSwap and the screens' actions send nothing; Walk halves the stick.
  - The pad's right stick still pushes the C buttons, as before.
- So the keys changed even though the bits didn't:
  - Shift walks instead of crouching. Alt, F, B and T do nothing now.
  - R reloads (A) instead of aiming: the right mouse button aims, and the R-held mouse aim works through it.
  - Q holsters.
  - **Keyboard C-left / C-right strafe is gone** until C2b's camera (Q / E are Holster and Interact). The pad keeps it.
  - On the pad, RT fires, LT aims, Y draws, B crouches, X reloads, LB throws a grenade and L3 walks.
- `device_input(&Bindings, &Devices)` returns the N64 `Input` plus the frame's look for round 2: the mouse counts and
  the right stick.
- The wheel is latched between ticks (`Wheel`): the input is read at 30 Hz, so a frame's turn would be lost.
- `Controls` (a system param) gives the screens their actions. Mute (M), TextHud (H), DebugCamera (F8) and Scoreboard
  (Tab / Select) go through the table; `MUTE_KEY`, `TEXT_HUD_KEY` and `DEBUG_CAMERA_KEY` are gone.
- **The team screen's BALANCE** moves with the stick up / down (W / S) or C-left / C-right (the pad's right stick).
  Q now closes it (B). Its text says `(W/S)`.
- AZERTY: `KeyLabels` caches `KeyCode → logical key` from every key press, for later prompts. No UI yet.

**Settings** (`settings.rs`, new)
- One serde `Settings`: a `version`, the binding overrides by action name (`[primary, alternate, pad]`), and the §5 v1
  values with their defaults (mouse, pad, crosshair; hold/toggle per device).
  - Only the bindings that differ from the defaults are stored, so a new action gets its default.
  - A missing value takes its default (`serde(default)`). An unknown action or input name keeps that action's
    default, with a warning.
  - The ranges are clamped on load.
  - A file that doesn't parse gives the defaults and is kept as `.bak`.
- Saved in localStorage `conker-settings` (web; web-sys `Storage`). Natively in `$CONKER_SETTINGS` if set, else
  `<config dir>/conker/settings.json`; the config dir is computed by hand: no `dirs` crate.
- Only a window loads them; headless and offscreen runs keep the defaults and never touch the disk.
- Nothing applies the values but the bindings yet, and nothing saves (`save` is for C3's Apply).

**Pointer lock** (`pointer.rs`, new)
- **Web:**
  - A press on `#game` calls `requestPointerLock({ unadjustedMovement: true })` through `Reflect`, because web-sys's
    takes no options. If that rejects with `NotSupportedError`, it calls the plain `requestPointerLock()`.
  - It uses `pointerdown`, not `click`, so the right button also locks. winit cancels `pointerdown`, which suppresses
    the compat `mousedown`.
  - A `pointerlockchange` listener sets `Paused` when the lock is lost (Esc). `buffer_input` then sends a neutral
    stick and no buttons, keeping the camera's yaw, until a press locks again.
  - On that loss, Bevy's `CursorOptions` is reset to none and visible (the debug camera's own lock would still read as
    locked).
  - `mouseup` and `pointerup` listeners `preventDefault` buttons 3 and 4 (back / forward).
  - Bevy's grab mode is never set to Locked on the web: winit would call the plain lock and lose the raw counts.
- **Native:**
  - A click while playing locks and hides the cursor. Losing the focus, or Esc, frees it. Esc was added so the cursor
    isn't trapped until an app switch.
  - No pause natively: losing the focus already drops the keys, and a pad player would need a click.
  - It stays out of the way while the debug camera is on, which grabs on its own.
- **Choice:** the lock is requested from the first press. The screens are keyboard or pad driven, the page's overlays
  have `pointer-events: none`, and the mouse counts come the same locked or not, so the R-held mouse aim is unchanged.
- The mouse counts are the device's, as before: no acceleration or smoothing added.
- New web-sys features: `Storage`, `EventTarget`, `Event`, `MouseEvent`. The client also gets `serde` (already in the
  tree). No new crates.

**Tests** (`cargo test -p conker-client`, 189, with `CONKER_SETTINGS` under `/tmp`)
- Inputs round-trip by name; Esc and Start never bind.
- Only overrides are stored, and a bad or unknown override keeps the default.
- The defaults send today's bits, including the wheel's zoom only while aiming.
- A rebound action sends its bit, and the mouse counts pass through.
- Settings round-trip with only their overrides; missing actions and values take their defaults.
- Values are clamped.
- A corrupt file gives the defaults and a byte-exact `.bak`.
- The test with disk access writes next to `$CONKER_SETTINGS` (else the temp dir), never the config dir.
- The old `keyboard_maps_to_the_n64_pad` and `combat_keys_and_mouse_map_to_the_n64_buttons` are replaced by these.

**Unverified** (no browser in this lane; checked only by the wasm clippy and by review)
- The pointer lock, `unadjustedMovement` and its fallback.
- The lock-loss listener and the pause.
- Mouse 4/5's `preventDefault`: whether `mouseup` still comes after winit's cancelled `pointerdown`, and whether
  `pointerup` stops the navigation.
- localStorage and Esc in the browser.
- The native grab needs a real click, and no click was sent. A windowed native run (server on 5970, `--screenshot`,
  `CONKER_SETTINGS` under `/tmp`) only shows:
  - the settings were read from that path, and the `Escape` override was refused with a warning;
  - the client connected, synced and played 20 s, then left cleanly;
  - with no click, nothing grabbed the cursor.
  - Its screenshot came out black, though the HUD log shows a live player. No rendering code changed; the display was
    probably asleep.
- Pre-existing: Bevy's free camera toggles its own cursor grab on M, the Mute key, while the debug camera is on.

## Tank, Beach, the held models and the laser drawn (2026-10-14)
**Lane G5b-world.** The client draws G5a's non-effect exports. The oracle runs are on the private copy
(`ConkerRecomp.g5w`; `../lab/research/mp-original/world-g5-scratch/`: `las`, t5). Tags: V (oracle), C (code), I (inferred).
No wire change and `PROTOCOL_ID` untouched. Shared simulation: only where conker-camera's fixed cameras get their
points (below). Pack re-exported: `verify-pack.sh` says "pack equal", 2,160 files.

**The crossbow's aim laser** (`laser.rs`, new)
- The dot (`strike_glow`) lies on the struck surface (the collision triangle's normal; a body part faces the beam), in
  every view. The beam (`strike_streak`, S wrapping every 164 units and swinging about one wrap) runs from
  `joint_09` × (−6, −25, 119) to the dot, in the other players' views only. One's own laser comes from the
  prediction's `Match::laser` (lib's `predict` → `laser::Own`), the others' from `Combat::laser`.
- They step every other frame and ease 0.2 of the gap a step: the alpha 0xFA → 0xC4, the beam's width 4 → 40, the dot's
  1 → 20 (V las DLs 259-300).
- The colour comes from the holder's model (C `func_151026BC`): 128 green (0x32, 0xFF, 0), else red. The switch's default
  is green unless record +0x78 is 0x3B (I); V las drew red.
- Against las: the dot at (−2308, −454, 4771) vs (−2306, −454, 4789), and the beam's start at (−2241, −473, 5961) vs
  (−2232, −472, 5961).
- The hum 1232 (`laser` cue) loops for the aimer alone (`Seen::laser`, `On::Laser`).
- ponytail: the original's alpha wander (0xA5-0xE3) and the dot's ±3 jitter settle at their means here.

**Tank**
- **Treads.** Each side scrolls on its own: `clamp(speed ± Δfacing/70, ±45) × −7` a frame (C `func_150ED638`).
- **The carried bomb.** It is now `maps/24/props.gltf` scene 1 on the turret joint (V k1 DL 45: centre 109 up and 51
  behind the pivot). It replaces the upright guess.
- **The rumble** (1193, `Sounds::rumble`, C `func_151BB0E0`, V t5):
  - the first starts 150 frames into the drive loop, then another every 450 or 706 frames;
  - each rises 8/256 a frame from 8/256, holds 150 or 406 frames, then falls from 255/256 to 7/256;
  - idling stops it at once. cs5's outro tanks rumble too.
  - **Correction:** the drive loop never fades (t5: 1,400 frames). The old docs said it did (conker-assets,
    events.json's note, the README, this file's G4 row on sound 0x4A9).
  - Remake log (`world-g5/tank.sh r`): the drive loop at read 30, rumbles at 180, 886 and 1592.

**Beach's cameras.**
- conker-camera takes the fixed cameras' points from the pack (`Slice::cameras`, `CameraLevel::new`'s new `points`);
  `SEA_POINTS` is gone. The sea's two are unchanged.
- 0x2B's tunnel hints 0x45, 0x47 and 0x48 now have their cameras too: points 3-5, reaches 838, 685 and 685 (C: the
  table names them; the README's "four no hint names" was wrong).

**Characters**
- **Head stumps.** A shotgun or chainsaw kill (reaction 0x2F, weapon 0x37 or 0x22) cuts the body until the respawn:
  joints 1-15 go to scale 1e-4 (I: 0 in the original), and the stump shows (#0x59 on 117, #0x58 on 128; V sg_e1
  791 → 969).
- **The racer helmets** (#0x5A, #0x5C, #0x5B) show on classes 0xD-0xF on any map. Race keeps hanging its own.
- **Sinking floors.** Bodies are drawn at their floor's sink (`render::stands`).

**Unlit vertices.**
- The level glTFs carry `_UNLIT` (the vertex flag's bit 15: the oracle's `lightVerticesCBFD` skips those vertices).
- The client registers it (`lights::ATTRIBUTE_UNLIT`), and `relight` (0x2B, 0x1A) and `tint_room` leave those vertices
  their colour.
- Counts: 97 on 0x2B (V: exactly the 97 that keep their colour, the bulbs' 12 among them), 58 on 0x1A, also 2d, 30, 24,
  33 and 24's props.
- conker-extract's dev `gltf-json` takes `extras` so the attribute validates.

**Restart.** The blast sounds now take the effects lane's guard (`effects::live`, `Sounds::prime`): a match's end or
restart re-primes the lists, so no frozen rocket, grenade, shot or shell goes off.

**Heist's outro check.** `heist/shots.sh` d/e was wrong, not the outro pick. A joining human now takes a waiting bot's
slot, so slot 0 was a human, the delivery a human win and the outro cs2. The script now warps bot 2 into GREENS' zone
(2481, 0, 4211): cs3 again (`world-g5/heist.sh d`).

| Check | Result |
|---|---|
| New tests | conker-client: `the_laser_eases_in_as_las`, `the_laser_is_green_on_model_128`, `the_laser_hums_for_its_aimer`, `the_rumble_swells_as_t5`, `a_restart_blasts_nothing`, `flagged_vertices_are_passed_by`, `a_shotgun_or_chainsaw_kill_cuts_to_the_respawn`, `stumps_and_helmets_hang_by_model_and_class`, `a_body_stands_in_its_floors_sink`, `the_carried_bomb_rides_the_turret_as_k1`; updated `treads_brakes_flags_and_props` and `the_outro_tanks_run_their_engines` (rumbles at 151). conker-sim (`net`): `the_beach_hints_take_the_packs_camera_points`. extract-level: the flag in `primitives_dedupe_and_drop_zero_area`. conker-extract `pack.rs`: `_UNLIT` 97 on 2b, 58 on 1a |
| Screens (`../lab/screens/world-g5/`, ports 5976-5982) | `laser.sh`: `laser-side.png`, `laser-early.png` (the beam at 268, thinner), `laser-dot.png` (the dot lying on the ramp), `laser-own.png` (the dot nearly edge-on from the eye's height). `tank.sh a r`: `bomb-carried.png`, `circling-own.png`. `tunnel.sh`: `tunnel-own.png`, `tunnel-free.png` (hint 0x47's point). `characters.sh lineup-b`: the helmets on 0xD-0xF. `weapons.sh` (sg_e1): `stump-p2.png` (the cut body, the stuffing on top). `heist.sh d`: cs3 |
| Pack | `bun run pack --force`, then `verify-pack.sh`: pack equal, 2,160 files (level glTFs with `_UNLIT`, README, events.json's tank notes) |

**Left.**
- The laser's default-model colour test (+0x78 == 0x3B, I), its alpha wander and the dot's jitter.
- No screen of a sinking floor, and none of the treads' scroll (unit tests only).
- The tunnel cameras weren't compared with an oracle frame.
- **The splash drops' model (I).** The table does say id 0x35 → #0xBE (V RAM `0x800A3880 + 4·0x35`, with 0x1B → 0xE5,
  0x39 → 0x132 and 0x54 → 0x157), but #0xBE is an eyeball (texture 0x2782). Two candidates: type 0x22 reads +0xE as a
  sprite kind (kind 0x35: blob 0x183, 64 × 16), or the draw re-textures it. `func_15147C4C`'s path wasn't traced, so
  the export is unchanged.
- Not done: flag 169's translucent frames; the crossfade at 0x34's and 0x3F's loop seams.

## Effects: the exports drawn (2026-10-14)
**Lane G5b-effects.** The client draws G5a's exports: `hud/*.png`'s effect sprites and `weapons/deathmatch/effects.gltf`
on every map, from the recipes in `../lab/research/mp-original/effects-g4.md`. New oracle checks are in
`effects-g5-scratch/NOTES.md` (twin2 and rifle DLs on the private copy). No wire change, no shared-simulation change,
`PROTOCOL_ID` untouched, no pack re-export.

`conker-client/src/effects.rs` now has three kinds of drawable:
- `Particle`: a 2D sprite, now also flat, tall, animated or riding a joint;
- `Streak`: a 3D sprite, a camera-facing quad between two points with an alpha at each end;
- `Chunk`: an `effects.gltf` scene thrown, spun and faded on its own material copies.

Their triggers are `watch` (shots and the match's lists), `bodies` (clips and deaths), `sea` (Beach) and `boards`
(Race). Tags are V (oracle), C (code) and I (inferred).

**Shots**
- **The twin guns' 3D flash** (`gun_flash_3d`). It runs from the 2D flash's point along the hand joint's −y
  (joint 9 or 4, with the facing as fallback). It is 41.5 × (0.604-1.605) long, 14.1-20 half-wide, and lives 6-9 VIs
  (C; V 29.6-65.3 long, 14.7-17.2 half-wide).
- **The other bullet guns' star**: six rays 73° off the barrel, 39-51 long (V rifle; I for the other guns).
- **Tracers** (`tracer`). Each bullet in `MatchView::bullets` draws one, from a pool. Alpha is at the head and 0 at
  the tail. The tail sits 0.62 of the bullet's motion back on its first list, then 1.22 (V 183-190, then 357-371). It
  moves with the bullet between ticks.
- **The local player's shots are predicted.** The flash watermark is the max of `Combat::shots` and the predicted
  `Fighting.state.shots`.
- **Tank**:
  - each shot draws the 3D flash on `joint_04`, from (0, 12, 120) to (0, 27, 215) × 3.59-6.1, plus the 2D flash at
    its start;
  - the shell's glow rides each shell;
  - a scope shot (power 0x02 and no new shell) draws a tracer from `SCOPE_EYE`, level along the gun, to the wall the
    ray meets. `Tank::bullets` is not on the wire, so it is derived from the shot. I: a hull it hits does not stop it.

**Blasts.** A rocket, grenade or shell leaving the lists sets off:
- the columns;
- the ring (`effects.gltf` scene 0, every map: L 14-18, 1.182·√t, alpha a − a·t²/L²; I the law Race's smoke shell's).

A rocket or shell (C: not the grenade) that struck a face within two frames of its motion (I) also leaves:
- the scorch, a black `scorch.png` decal on that face, 150-300 wide, 319-523 VIs, with random flips;
- the smoke left (now only then, as C).

Tank's shell blast moved here from `tank.rs`'s burst (I: the rocket's).

**Restarts.** Nothing goes off across the end or a restart (`live`: both phases < 2). The restart used to empty the
frozen lists and set every rocket and grenade off at once (G4's note). The same guard is on Beach's rocket shake.

**Bodies**
- **Flame tongues** (`flame_tongue`): 0.2-0.6 a frame from the nozzle. Life 50-71 VIs. They rise faster each frame.
  I: one quad from 3 frames back.
- **The burning player**, now on every map. Its flames are `flame_a`/`flame_b`:
  - about one a frame, at most 25, each riding a random joint 0-15;
  - alpha 80-160, life 12-60 VIs;
  - I: 6-34 wide, twice as tall, 0.23 frames a VI.

  `traps.rs`' stand-in is gone.
- **The explosive death's sparks** (scene 1). When a player's clip turns 0xF3 at 0 health: 8-12 sparks, 33 up,
  32°-77°. Beach's strike drops its old debris for them.
- **The sea**: at the stroke clip's 10.8 (right hand) and 26.4 (left), where the water is within 300:
  - a ripple: flat, 80 wide, `ripple_00..09` at 2 frames each, alpha 150, 40 VIs;
  - the 3-6 drops are not drawn (`effects::drops` is ready). The pack's scene 3 (#0xBE) is an eyeball: texture
    0x2782, seen in `swimmer-own.png`'s first take. §9's model id 0x35 → #0xBE needs a recheck (pack/G5a).
- **Item 0x3A's fuse sparks.** Bursts of 8-13 `fuse_spark` streaks plus a `shell.png` flash. They come from the left
  hand (`joint_04`) during the pull and hold, and from the lob (+28) in flight. Lob maps only.
- **Race's death**, `race_blast`, per the pack-readme:
  - the board's three pieces and the racer's six;
  - the fire shell and the smoke shell;
  - N from the last move's crossed face.

  It replaces `race.rs`' burst.

**The rest**
- **Race's board trail**: a point a frame at `race::EXHAUST_AT`, at most 25. Its length is 10 × (speed − 10), 0-600.
  Its width eases toward 50 (I: × 0.35 a frame) and tapers to 0. It uses `race_trail.png`, U across.
- **War: Beach**:
  - flak: the `smoke_grey` flash × 1.028 a VI in prim 0xFF8E13 (I), plus 5-12 blended `flak` puffs over the sea;
  - the three bunkers' tracer bursts, 191 a frame (I: the first burst at once).
- **Heist cs3's bubbles** go through `effects::bubble`: `bubble_particle` blended (not added), emitters A and B in turn
  (I), rising to their caps 3.5 and 4.0.

| Check | Result |
|---|---|
| `effects::tests` (13) | `the_3d_flashes_as_the_original` (lengths bracket V's 29.6/65.3, Tank's 228; widths; the star's 0.26-0.32 forward), `the_tracers_as_the_original` (186 then 366, the scope's 3 frames to 900, the bunkers' rate and aim), `a_restart_sets_nothing_off`, `the_ring_and_scorch_as_the_original`, `the_tongues_and_flames_as_the_original` (rates 0.41 and 1.0), `the_sparks_and_flak_as_the_original` (V 3153 → 3922 in 4 frames; y += vy + g/2), `the_ripples_and_bubbles_as_the_original` (the 20-frame sequence), `the_race_blast_and_trail_as_the_original`, `the_fuse_as_the_original`; G4's four kept. `beach::a_rocket_shakes_every_map` adds the restart |
| Screens `../lab/screens/effects-g5/shots.sh` (5970/5972/5974) | `twin-side.png` (the 3D flashes along the barrels), `twin-broad.png` (a tracer at full length, broadside), `blast-side.png` (the ring, columns and the scorch on the trapdoor), `blast-scorch.png`, `flame-side.png` (tongues at the nozzle), `ripples-close.png` (a ripple under the swimmer), `race-death.png` (both shells and pieces), `race-trail.png` (a board's trail). The oracle side is numeric: NOTES' DL quads and the recipes' V values (the headless oracle draws no frames) |

**Left:**
- not ported (their recipes were not read): the tanks' exhaust, the cone's smoke, cs5's smoke and clods;
- not ported: Beach's real fire (no spawner), the blasts' light, the bullets' impact sparks, the bubbles' sway;
- not drawn: the swim's drops (the pack's model is wrong, above);
- inferred, not verified: the race trail's 20/10 widths, a flame's joint table and size law.

**Handoff (G5b-world):** `audio.rs`' `gone` (the blast sounds) has the same restart hazard; `effects::live` is the
guard.

## Rules and bots: G4's leftovers (2026-10-14)
**Lane G5a-rules.** Notes: `lab/research/mp-original/rules-bots-g5.md` (+ `-scratch/`). Wire changed (`Combat::popped`;
`Fighter::popped` and `gun::Mount::recoil` in `Fighting`); the shared simulation changed (the drown, the knock's drift,
the dead pose's rows, the head pop, the gun's recoil, the parts' sink, the bots' pickup reach, Tank's barrel, Heist's
take). `PROTOCOL_ID` untouched.
- **A swimmer's death is the original's.** A lethal hit on a swimmer takes cause 4 whatever the damage type
  (`func_1505E874`): clip 0x24 to the respawn 88 frames after the hit, not the land death's 163, the body afloat on the
  current with no knock (`damage::DROWN`, `Match::hurt`/`hurt_frame`). `knock()` adds the current's drift (fading 0.15 a
  frame, along the floor's plane on the ground), so a body drifting ashore slides as die1's; with no drift the old path is
  kept bit for bit. Test `a_swimmer_drowns_as_die1` (V die1: hit 460, 0x24 on 461-547, the respawn at 548): afloat exact,
  ashore within 1.26 (the exit frame's brake, I). New oracle rows `lab/oracle/map2b/sea/die1.csv`, `die2.csv`.
- **The dead pose's twitch is the clients'.** The rules' fixed 0x18A rows are gone (`(DEAD, 81)` for the area stagger's
  and 0x33's deaths). `tests/combat.rs` maps the oracle's 0x18A to 0x59: e_w38 31 of 31 events.
- **A bullet on a corpse's head pops it.** `Fighter::popped` (`Match::pop`: a head hit taken, or any head hit on a
  corpse; a bolt's too, not weapon 0x14's, V combat4_w14 833), cleared at the respawn; `parts.rs` hides joints 12-15
  (0xF000). The wire's `Combat::popped` (server `combat()`), and conker-client `corpses.rs` hides the head on it (one
  condition). `tests/combat.rs` compares it with obj+0x9C bit 12 on every row: posed (`COMBAT_POSES=last`) all 66 traces
  match, e_w39's corpse pop at 971 and e_w09's at 1005 included; the fitted volumes miss those two (no head on a falling
  corpse), which the test lets by there only.
- **The mounted gun's muzzle recoils.** `gun::Recoil` (the fire clip's clock: clip 1 while firing, rate 1, blend 2; moved
  from conker-client, which re-exports it) rides in `Mount::recoil`; `gun::fire_clip` reads `characters/138` into
  `Arena::fire_clip` (server `main.rs`, client `run`, `bot_match`). `fire_gun` aims from the eye kicked by the pose drawn
  the frame before and the muzzle and sight plane by this frame's (`gun::muzzle`/`sight` take the kick). Test
  `the_gun_recoils_as_k2`: V k2's shots at reads 107, 115, 120 and 137 within 9.9e-5 (at rest: 0.005-0.046 off). A bot's
  shot leaves the kicked muzzle along the kicked barrel (I). conker-anim's `AnimPlayer` has serde under `serde`.
- **0x34's twin guns: the body parts sink with the floor, the bots aim through the acos table, and reach farther for
  pickups.** The 302 against 107 shots compared patterned seeds on both sides (the oracle's low RNG word 0-7, the
  remake's raw states 1-8); with independent draws (oracle p34r, both words at read 23; remake seeds 9-40) it was 217
  against 119. Bursts, the fire cycle, sight and range already matched; the bots killed the idle human slower at range.
  - `parts.rs` adds `body.sink` (obj+0xCC) to the posed spheres: lldb on n34s2 shows the 402 hit on the head (joint
    12, part 0xA) at +45.47, the human on kind 0xA (sink −6) with its joints 6.0 lower than on kind 5. The broad sphere
    does not sink (C). The posed combat replays are unchanged.
  - conker-bots `attack()`: the yaw and pitch through `func_1505A630`'s table (C; V: 3,633 of 3,655 still frames carry
    the table's pitch), the aim point rebuilt from them; the mounted gun's pitch target too.
  - `Match::pickups`: a bot's reach is 1.4× in every mode but Race (`func_1511DF6C`, `0x800A3304`; V: 0x34's bots take
    pickups 120-135 away), `tank::touches`. `bot_match` marks its bots as the server does.

    | 0x34, 1 + 3, 4,403 reads | Oracle p34r (8) | Remake before (32) | Remake now (32) |
    |---|---|---|---|
    | Twin-gun shots / hits a run | 119.1 / 12.9 | 217.2 / 14.0 | 176.8 / 12.6 |
    | Deaths / the human's / hp drops | 5.1 / 1.6 / 21.9 | 5.8 / 2.1 / 23.9 | 5.8 / 2.4 / 23.5 |
    | hp a far burst on the human | 1.09 | 0.52 | 1.11 |

    ("Before" has the reach fix.) Left: 177 shots against 119, hits per shot 0.071 against 0.108 (shots per drawn frame
    0.165 against 0.117, untraced).
  - The pit (left): no box-specific gate in the original (0xBB's attack pick is ai.bin data the remake runs; state 6's
    gate at 0x2D24 is never reached on either side). Inside the box the remake's bots see an enemy wall-free 143.5
    frames a run against 15.8 and switch to states 10 and 6 there (the oracle's stay in 4 and 5). 15 bots, seeds 1-6:
    box frames 486 a run, standing 153, against G4's 348 / 28.7 and the oracle's 232 / 2.5. Seed 6's p4 stands 732
    frames at (−342, −399, 3011) in state 4, goal 82, script 5 (the G3d r34b15s6 pattern); it comes with the 1.4 reach
    (the same seed without it: 536 / 83). Without seed 6: 325 / 16.2.
- **Tank's gun is solved from its pivot and fires from its barrel.** `func_150ED298` measures from joint 3 (C), and the
  shell starts 101 along the pitched barrel from it (V: 969 shells, pivot 26.1 ahead and 113.9 up, residual 2.6). The
  remake solved from the hull's centre and fired from 128 ahead, so its shells came down ~128 past the aim point.
  `tank::PIVOT`, `BARREL` replace `MUZZLE`. The lead and the targets' motion were ruled out (V, C). 12 seeds: hits
  30.3% → 29.4% (oracle 27.6%); `shells_hit_as_f1` checks c1's start within 1.5/0.5; `tank_bots_bomb_and_fight` 28.3%.
  Left: the bots' hull turning (`func_15056B08`: the facing turns up to 0x400 a frame while the bot accelerates; the
  oracle fires 30-46% of its 250-1,500 shots while turning, the remake 16-25%, and those shots rarely hit).
- **Heist: no bag take mid-swing.** `func_150597FC` refuses a taker with obj+0x89 set, which a landed bat swing holds
  11-39 frames (V hb, hb2: 21 of 21). The remake's take accepted `Action::Fire`: 1.9 steals a match within 5 frames of
  the drop (oracle 0), each sending the robbed carrier into a 45-53-frame retreat near the new carrier. Seeds 1-12:
  state 8 within 500 of the enemy carrier 69.4% → 76.1% (oracle 79.5%, 75.5-83.5), takes 20.2 a match (21.2). Left: the
  stale health on rocket and tommy hits, the bag's fall, the carried bag's place, the tommy gun's hold. The Colors flags
  keep `Fire` (same C function, unverified there). conker-bots' ROM `play` harness does not set `Fighter::bot`, so the
  Heist share above and its Tank and Heist tests run without the bots' 1.4 reach the server applies.
- Checks: `bun run test`, `bun run check`, the client's wasm clippy.

## Client polish (2026-10-14)
**Batch G5a-polish. Sounds can change pitch: Race's hums follow `func_150D5124`'s cents with the original's Doppler,
and Tank's drive loop follows the hull's speed and pitch. Song 0x8A plays at the capture's level. Race's camera
springs its heights. 0x1A's lamps go dark with the light they hang by. Heist's cs3 clears black at the sides, and the
HUD names every weapon. No wire change, no shared-simulation change.** Research: scratch
`../lab/research/mp-original/polish-g5-scratch/` (`run.sh`, `polish.py`, `song.py`; oracle `ConkerRecomp.g5c`, runs in
`/tmp/g5c`: r1 Race, t2 Tank driven, c1 Colors, w1 Total War, v1 0x1A with 40 RAM dumps).

| Item | Original (tag, run) | Change |
|---|---|---|
| Pitch in the mixer (`audio.rs`) | The records' cents (+0x20) → `alCents2Ratio`, × the Doppler, eased (+0x2C) each frame (C `func_10011624`) | `Control.pitch` (atomic, the ratio) read by `Playing` per output frame: a fractional step with linear interpolation, loops keeping the phase; bit-identical at 1.0. Native and WASM (the same rodio source). `Voice.bend` (`Bend::step`) per tick |
| Doppler | v = trunc((d_prev − d) · 2/3) over the u16 distances to the listener; f = 367/(367 − v) in [0.5, 2], 2 from v ≥ 367; ratio += (2^(c/1200) · f − ratio) · 0.3; a new record starts at 2^(c/1200) (C; V r1 reads 140-143, within 2e-5) | `doppler`, `DOPPLER_EASE`; Race's hums only (Tank's loop has no Doppler, C) |
| Race: the hums' pitch | Hum 0 (0x5B3): min(trunc(5 · height), 1000) in state 2, else 400; hum 1 (0x5B4): 400 in state 0, and in state 2 under 5 racers, else 0; hum 2: 0. Up +512, down −16 a frame toward it (C `func_150D5124`; V r1) | `hum_cents`, `cents_step`; `hum_target`'s hum 1 order fixed (state 0 first) |
| Tank: the engine's pitch | 0x4A8 at trunc(\|speed\| · hull pitch° · 0.5) cents, read the frame before; 0x4A7 (idle) flat (C `func_151BB0E0`; V t2: 4 rows −250 to 469 cents) | `engine_cents`; `Seen.pitch` from conker-rules' `hull_pitch` |
| Song 0x8A's level | The least-squares gain of the pack's WAV in Beach cs4's capture: 0.79 over 3 s, 0.75 over 9 s (V b4) | `SONG_LEVEL` 0.75 (was 1.0) |
| The radio heard by all | A bot's line 0x64B at 32700 and its clicks, near/far 0x7FFF/0x7FFE, playing 4372-4602 from the listener (V c1 reads 2294-2374; w1 had no line in 120 s) | None (already `Src::radio`); the run added to the test's doc |
| Race: the camera's springs | Mode 8's look-at height (`conker_camera`'s: floor + air · min(air/300, 1) + 0.75 · head, springs (4, 8), doubled going down) + the view's 123 fits with a 130 head (C; V r1 480-850 rms 4.1, was 21.3 at the racer + 195). The eye's height lags too: a (4, 8) spring toward `CAM`'s height fits (I; rms 12.9, was 42.4). M13's runs agree (`/tmp/m13/cam`, 7 runs: eye rms 7-38, was 19-78; look-at 3-11, was 17-32, but `c_a`'s fall from 490 up, 126 against 135) | `race::heights` after the go, its state in `tank::Cam`'s new second field; the grid's frames hand over from `GRID` |
| 0x1A's lamp pairing | Segment 4 + k's byte (0x800BE500 + k) toggles with the light whose driver kind is 0x900 + 0x100 · k: lights 11, 10, 20, 12, 29 (V v1: 40/40 dumps each, no other light over 33/40). Each lamp follows the light it hangs by: the original isn't cross-wired, the kinds just aren't in the triggers' order (the trigger records hold no kind, C) | `lights::BY_KIND` orders `Flickers::switches`, so `segments.rs`' lamp k follows the right light |
| Heist: the HUD names | — | `weapon_name` gains 0x18, 0x19, 0x37, 0x38, 0x3F, 0x40, 0x41; every `conker_rules::weapon` id has a name (the radar's team colours were already M4's `hud.heist.radar`) |
| Heist: cs3's backdrop | The 740-unit backdrop fills the original's 4:3, not 16:9 (V `../lab/screens/heist/cs3-bubbles.png`: the sky at the sides) | `outro::backdrop`: the view clears black while 0x1A's cutscene 3 shows |

| Test (conker-client) | Against |
|---|---|
| `audio` `records_bend_their_pitch_as_the_oracle` | r1's hum cents (174; 207 → 191; 159 → 174) and Doppler ratios (400 and 0 cents over distances 99 → 86, 82, 89); t2's four engine rows; the mixer reading 100 · k · ratio at 400 cents and back to step 1 |
| `audio` `the_outro_song_plays_at_the_captures_level` | b4's 9 s excerpt (lab only, skipped without it): the fitted gain within 0.01 of 0.75 |
| `race` `the_board_camera_springs_its_heights_as_the_oracle` | r1 reads 545-575 (a jump): within 45 (eye) and 12 (look-at) of the oracle, where unsprung is 129 and 40 off |
| `lights` `the_vault_lamps_pair_with_their_lights` | v1 dumps 212, 230, 326, 338, 344: the lights off → the bytes set |
| `outro` `heists_backdrop_outro_clears_black` | The pack's 0x1A outros: only the one with the 37 × 37.01 backdrop clears black |
| `lib` `hud_reads_health_weapon_score_clock_and_respawn` | Every rules weapon has a name |

Screens (`../lab/screens/polish-g5/`, copies of `heist/shots.sh` and `race/shots.sh` on ports 5986/5988):
`cs3-early.png`, `cs3-bubbles.png` (`CONKER_OUTRO=3 PORT=5980 zsh heist.sh d`: black at the sides), `racing-11.png`
(300), `kick.png` (214, the hand-over from the grid). No lamp screen: each client switches the lights at random,
so a dark lamp can't be timed (the unit test carries the dumps). Unforced, `heist.sh d` now shows cs2 (`human_won`), where
`heist/shots.sh` expects cs3. Not looked into: the rules lane's changes are in progress.

**Left.**
- **Race's finish cameras (V F1, `/tmp/m13/cam/F1`; I):** at the finish (232) the eye cuts to (−133, 1034, −6925),
  about 95 ahead of and 390 over the racer, and the look-at is the racer itself. The eye holds until the racer is
  about 950 away, then is dragged along at 950-993. At 321 (back 993) it cuts to about 954 ahead (3449, 977, −8462)
  and trails again. It isn't a camera hint: the scene has no points (`[0x0C, 0x25 | 0x33, 8]` empty). The law
  (the hold, the drag's height, the cut's trigger) isn't decoded. Not ported.
- **Race:** the listener sits 80-117 behind the own racer (cam+0x2A4), so the original's own hums wobble about ±2 %
  by Doppler. The remake's listener is on the racer (none; I). Also open: the FOV add, the boost's distance, the
  yaw's hand-over and the death's look-down (M13's list).
- **Race's lamp quads' drift (I):** with the camera still (cd 142-202), the quads hold their centres. From the go
  they follow about half the camera's xz move (cd 200 → 235: the camera (+83, −55), the quads (+42, −28)) but not its
  y. Not decoded; the remake draws them fixed.
- **Unlit vertices dimming with the lamps:** pack-side. The extractor drops the vertices' lighting flag (bit 15), so
  the client can't tell an unlit vertex. G5a-pack would need to export it.
- **The blink and the bag's clock (justified):** both run on the client's tick, which lightyear keeps at the
  server's plus the client's lead. Two clients differ only by their leads; the original's VI counter (0x800BE9AC) is
  one machine's. The bag counts from when its state arrives, in the same snapshot as its place: counting from
  `CashView::since` would run the clip ahead of where it's drawn. `heist.rs`' stale comment fixed.
- **The 0.6 picks' lift and held size:** not checked against a frame. That needs an oracle run on a 0.6 pick (176,
  177) with DL dumps of its held weapon's matrix.
- **Helmets 0xD-0xF off 0x33:** the pack now has them (`held.json` 26-28, G5a-pack). Drawing them is G5b's.
- **Flag 169's translucent frames:** client-side, not ported. c1a DL 430-466 and c1b DL 428-434 fall between the
  capture's celebration and the flags' destruction (c1a: capture 382, respawn 472; c1b: outro load 438). That
  suggests the destroyed flags fading out (I); the switch isn't traced.

| Check (2026-10-14) | Result |
|---|---|
| `bun run test` | 647 pass, 1 ignored |
| `bun run check` | Fails only on `cargo fmt --check` in other lanes' in-progress files (conker-bots `brain.rs`, conker-rules `tank.rs`, `tests/zz_g5_head.rs`). Its clippy step (`-D warnings`, every crate) and `web/app`'s check pass when run alone |
| `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | Pass |

Files: conker-client `audio.rs`, `race.rs`, `tank.rs` (`Cam`'s second field), `lights.rs`, `outro.rs`, `heist.rs`
(a comment), `lib.rs` (`weapon_name` and its test).

## The G5 exports in the pack (2026-10-14)
**Batch G5a-pack. Every pack export G5 asked for is in `../assets` (2,160 files, was 2,103; `verify-pack.sh`: pack
equal): the world effects' sprites, the scorch and the fuse's sparks in `hud/`; one shared `effects.gltf` (the ring
moved off 0x2B) with the strike's chunks, the flak, the drops and Race's death blast; the maps' camera points; sounds
1193 (the tank's rumble) and 1232 (the crossbow's laser) with their cues; the head stumps and the racers' helmets as
held models; the tank's treads one material a side and its carried gas bomb. Nothing is drawn new in the client
(G5b's); the moved files' consumers (`traps.rs`, `effects.rs`, `beach.rs`, `tank.rs`) follow them. No wire change,
no shared-simulation change.** Research: `../lab/research/mp-original/g5-pack-scratch/` (`race/`, `laser/`,
`tank/` NOTES.md, `probe/`; oracle `ConkerRecomp.g5p`). Every entry is documented in `pack-readme.md`.

| Entry | Export | Pack test | G5b client follow-up |
|---|---|---|---|
| World sprites (`effects-g4.md`'s list) | `hud/` `gun_flash_3d` 0x3CF, `tracer` 0x11FE, `puff` 0xBA6 (moved from `maps/34/traps/`), `flame_tongue` 0xBA7, `smoke_grey` 0x721, `ripple_00`-`_09`, `flame_a0`-`_b4` 0x8FF-0x908, `dirt` 0x4BD; `bubble_particle` is now 0x4B3 RGBA32 32×32 (0xC01 was a selector word; comments fixed in `hud.rs`, conker-assets `outro.rs`). Kinds read from the table at 0x80090B60 + 12 · kind (C, V) | `pack_hud_reads_back`: each size, none blank, 10 distinct ripples | The recipes of `effects-g4.md` §1 (3D flash, tracers), §3 (tongues), §5 (flak flash), §6 (bunker tracers), §9 (ripples), §11 (burning flames, in place of `traps.rs`' puff stand-in), §14 (dirt, exhaust); `outro.rs`' bubbles now get the right 32×32 texture (their sizes are the recipe's, unchanged) |
| Scorch (kind 0x4F) | `hud/scorch.png`: 0x4E5 I4 32×32, the table's id is the drawn one (V rkt DLs 126-160; also Tank's shell, V k1) | the same | A decal on the struck face at each rocket, grenade and tank-shell blast: black, alpha × 0xE8, random flips; size and life in the readme (half-width reading I) |
| `effects.gltf` | `weapons/deathmatch/effects.gltf` (12 scenes, conker-assets `EFFECTS`, `effects_gltf`): `ring` #0x157 (was `maps/2b/effects.gltf`), `spark` #0x132, `flak` #0xE5, `drop` #0xBE, then Race's blast (V d1: every vertex of DL 380 found in these) `fire` #0x158, `board_6c`-`_6e` #0x16F-#0x171, `gib_7e`-`_81` #0x17D-#0x17F, #0x179 | `pack_audio_reads_back`: scene names, triangles, alpha modes, textures; no `maps/*/effects.gltf` | The ring at every map's blasts (§7); the chunks (§4), flak puffs (§5), splash drops (§9); Race's death: replace `race.rs`' `beach::burst` stand-in with the readme's recipe (3 board pieces, 6 racer pieces, the fire and smoke shells) |
| Camera points | `maps/<map>/cameras.json` (`Slice::cameras`, `CameraPoint`): the ROM's `[0x0C, scene, 8]`, 0x18-byte records, `at` s16 and the rest as hex. 0x2B 6 points (the first two are `SEA_POINTS`), 0x1A 4, the others none | `pack_reads_back_as_the_rom_path`: Beach's 6, the vault's 4; the round trip (conker-assets) | conker-camera `volume.rs` reads its points from `Slice::cameras` instead of `SEA_POINTS`; 0x1A's hints 0x40-0x43 (kind 0, `action` 5, points 0-3, reach 181) then work too |
| Sound 0x4A9 (1193) | `audio/characters/119/1193.wav`; cue `tank_rumble` (`On::TankRumble`, at 0x2EE0, near 200, far 800) | `tank_audio_reads_back`: the cue and the file | `audio.rs`: 150 frames after a tank's drive loop starts, the rumble ramps 8/256 a frame, holds 150 or 406 frames, falls; stop at idle; the drive loop fades from 450/706 frames (C `func_151BB0E0`; cs5's outro tanks too) |
| Head stumps | `held.json` 24 (#0x59, `joint_00`, model 117) and 25 (#0x58, `joint_01`, 128), no events | `pack_reads_back_as_the_rom_path` (the held tail); `rom` `slice_in_memory` (29 held; #0x59's one shade-only draw) | Show the dead player's model's stump from a 0x2F kill (weapon 0x37 or 0x22) to the respawn, joints 1-15 at scale 0 (`corpses.rs` keys the pop already) |
| Tank treads | `character.json` `tank.treads` `[13, 15]`: primitive 13 split by joint, material 15 the `joint_12` side (same texture) | `pack_reads_back_as_the_rom_path`: 48 indices a side, each single-joint | `tank.rs` scrolls both alike today; scroll side k by its own speed: clamp(v ± Δfacing / 70, ±45) × −7 a frame, the first side +, wrapping at 0x800 |
| Tank's carried bomb | `maps/24/props.gltf` scene 1: action 0x18's one record, bank 09 #0x3F on matrix slot 2 = `tank.turret` (C; V k1 DL 45: drawn with joint 2's matrix), 48 triangles | `pack_reads_back_as_the_rom_path`, `rom` `tank_model_and_map` | Draw scene 1 in the carrier's `joint_02` frame at the tank's scale while it carries, in place of the guess (`tank.rs`: 190 up, 0.3 of the post's) |
| Tank muzzle "smoke" | None exists (V k1 DLs 56-90): the shot makes the 3D flash (`gun_flash_3d` on `tank.muzzle`), a 2D flash (`race_exhaust_*`), a light and the shell's glow (`race_exhaust_*`, size rand × 800 + 1700); STATUS's 5-9 puffs were the flash's life | — | The tank shot's flashes and the shell's glow (recipes in `g5-pack-scratch/tank/NOTES.md`); the exhaust is `smoke_grey` (its recipe not read) |
| Crossbow laser | Its textures were already packed: the dot 0xE20 = `strike_glow`, the beam 0xB97 = `strike_streak` (V las); sound 1232: `audio/common/1232.wav`, cue `laser` (`On::Laser`, 2000, the aimer only, free channel) | `pack_audio_reads_back`: the cue and the file | Draw the dot (both views) and the beam (other views only), red with the laser's alpha, while `Combat::laser`; play the loop for the aimer (alpha, growth and scroll laws open: values only) |
| Fuse sparks | `hud/fuse_spark.png`: 0x1041 I8 32×16 (kind 0x42, V lob) | `pack_hud_reads_back` | Item 0x3A's spark bursts from the pull through the flight, each with a `shell.png` flash (readme) |
| Racer helmets off 0x33 | `held.json` 26-28: #0x5A, #0x5C, #0x5B on `joint_12` (classes 0xD, 0xE, 0xF) | `pack_reads_back_as_the_rom_path` (the held tail) | Show a class 0xD-0xF player's helmet on any map (C attaches it in any mode) |
| Flag 169 translucent | Not pack-side: the original switches the whole flag to its XLU table in some frames (V c1a, c1b); when is not traced | — | Client, if wanted: a per-entity `AlphaMode::Blend` copy |
| 0x34's, 0x3F's loops | Not fixable in the pack: songs 36 and 84 never repeat sample for sample (V, 900 s runs); the cuts are near-silent periods | — | Client, if wanted: a short crossfade at the seam |
| Song 0x8A's level | Not settled (I: gain 0.62-0.76 against cs4's `b4` capture, music and voice over it); the WAV is unchanged | — | Client-side: G5a-polish settles its level against cs4's capture (`the_outro_song_plays_at_the_captures_level`) |

| Check (2026-10-14) | Result |
|---|---|
| `bun run test` (ROM) | all pass (conker-extract `pack` 6, `rom` 23, `traps` 1; conker-bots' `rom` 26 in 2,113 s, run beside other lanes') |
| `lab/tools/verify-pack.sh` | pack equal, 2,160 files (`maps/2b/effects.*`, its texture 3301 and `maps/34/traps/puff.png` gone) |
| clippy `-D warnings` | conker-extract, conker-assets, conker-client native, conker-client wasm32 clean; `cargo fmt --check` clean on these crates (its diffs are in other lanes' files); `web/app` check clean |

Files: conker-extract `lib.rs` (`effects_level`, `camera_points`, `split_treads`, `STUMP_ACTIONS`, `TANK_BOMB`,
`attached`, the helmets in `held_level`), `hud.rs`, `traps.rs`, `audio.rs`, `pack-readme.md`, tests `pack.rs`, `rom.rs`,
`traps.rs`, `Cargo.toml` (png dev-dependency); conker-assets `lib.rs` (`EFFECTS`, `effects_gltf`, `CameraPoint`,
`Slice::cameras`, `Tank::treads` a pair), `audio.rs` (`On::Laser`, `On::TankRumble`), `outro.rs` (a comment);
conker-client `traps.rs`, `effects.rs` (`hud/puff.png`), `beach.rs` (the shared ring), `tank.rs` (both tread materials).

## Cameras, clips and outros (2026-10-14)
**Batch G4b. Beach's sea sides hold the original's fixed cameras (hints 0x26/0x27, mode 0x40), and a swimmer dies in
clip 0x24. Map 0x34's corridor camera now matches the original's eye bit for bit through read 777. The scope opens at
one view's zoom, so its dots start at 177°. The original's egg carrier keeps the on-foot camera, as the remake's
already did. A dead body draws the 0x18A twitch as the original does (1 in 8, again every 64 frames), and a head kill
hides the head. A mounted gun's scope eye kicks with its fire clip. e1's knock slides 140.9 (V 140.7, was 167.7).
In the outros the villagers' heads turn in joint 0's frame, and Colors cs5's tanks run their own engines. No wire
change; the knock is shared simulation (below).** Research: `../lab/research/mp-original/cameras-clips-g4.md` (§1-11
and the G5 pack list), scratch `cameras-clips-g4-scratch/` (one folder per item group; oracle `ConkerRecomp.g4b`).

| Part | What |
|---|---|
| conker-camera `volume.rs`, `lib.rs` | **Kind-5 hints** (`Fixed`, `HINT_FIXED`, `CameraLevel::fixed`; `func_1512E8E0` → mode 0x40, `func_15129934`): the eye springs (1/3 across, 1/2 up) toward the line from the hint's point to the look-at, `reach` short of it, the rise × 1.2; a cut puts it there; a death (the hint's veto, cam+0x6D2) or R's aim drops it, C-strafe is skipped under it. 0x26: point 0 (2024, 277, 9703), reach 751; 0x27: point 1 (−2290, 728, 10913), reach 707 (`SEA_POINTS`, V cam+0x288; the pack lacks [0x0C, scene, 8]). **`eye_spring`** (cam+0x964/0x968 and velocities): mode 0x40 drops the distance and height springs to 1/2, `func_15123A54` brings them back to 6/8 at (1, 2). **`CameraWorld::floor_at`**: after a sweep that hit, the full floor query at the eye's own height, no fast path, the cached triangle untouched (`func_1512BB10` → `func_1510E8BC(0, …)`). **`VIEW_ANGLES`** = one view's 60.6 × 50 (0x42726666; the split screen's 82.416 is gone). **`PlayerView::gun_kick`**, `gun_eye_kicked`: the scope eye on the gun's fire clip as last drawn |
| conker-sim `follow.rs` | Reads the spawn's hints on a cut; sets `fixed` after each step; `Level::floor_at` = `world.floor(x, z, eye y, start y, None)` |
| conker-rules `game.rs` (shared simulation) | **`knock()`'s airborne branch**: the brake is 0.1·k a frame (obj+0x109, `func_150585F0`), a wall met within ±0x2200 of head-on caps the speed at 14 (`func_1504CB98`), no move under speed 1 (`func_15059C84`). conker-sim `movement.rs`: `push_angle`, `against` public |
| conker-client `lib.rs`, `smooth.rs` | **`DROWN`** (0x24, rate 1, blend 3) for a dead player whose body still swims, before the twitch. The drawn camera takes `fixed` with `hint_cut`. `predict` sets `view.gun_kick` from `FollowCamera.gun` |
| `corpses.rs` (new) | **`Corpse::shown`**: frames in 0x59 (or the server's fixed 0x18A rows); draw k at frame 63 + 64(k − 1), `(rand & 7) < k` (`func_15072A7C`), seeded by slot, `Combat::deaths` and k (every client agrees); a hit shows 0x18A (rate 1, blend 4) to the end of the dead pose. **The head pop**: from the tick `Combat::anim` is 0x17E with `Combat::hit` not 0x37/0x22 while health is 0, to the respawn, `hide_heads` scales `joint_12` (with joints 13-15 below it) to 0 (`func_1507DE4C` → obj+0x9C 0xF000 on every model, `func_15034728`) |
| `guns.rs`, `main.rs` | `FireClip` (138's 12 fire frames from its skeleton: joint 0's and 4's x turns, joint 4's z slide), `Recoil` (an `AnimPlayer` clock: the fire clip at rate 1, blend 2 in gun state 4, the pose drawn the tick before), loaded on maps with guns (fs native, fetch on wasm) |
| `outro.rs` | Villagers (156/157): `head_turn(h, t, under)` = under⁻¹ · turn(−pitch, yaw), `under` joint_01's sampled rotation (`func_15036CE8` poses the head in joint 0's frame, a1 = 0) |
| `audio.rs` | **`Sounds::outro_tanks`**: Colors cs5's tanks' engines (`func_151BB0E0`: 1191 idle / 1192 moving by the path's speed, gears 1187-1190, 1189 into coasting), following each tank, stopped with the outro's sounds (`Owner::OutroTank`) |
| `tank.rs`, `hud.rs`, `render.rs` | `SCOPE_ZOOM` (−27.8, −22.94) (V tank_pw2); the dots and Tank bar tests on one view's zoom; `render.rs`' gcA fov test pinned to gcA's own 82.416 (a 2-view run) |
| `package.json` | `bun run test` and `bun run check` include conker-camera (its 35 unit and 7 oracle tests were not run before) |
| `../lab/oracle/map2b/sea/` | `east.csv`, `east_fields.csv`, `east.txt`: a villager on the east beach inside 0x26 (mode 0x40 over reads 24-660) |
| `../lab/screens/cameras-g4/shots.sh` | `sea`, `scope`, `outro` (below) |

| Check (2026-10-14) | Result |
|---|---|
| sim (ROM) `tests/sea.rs` | swim1's camera now over 300-749 (the 450-599 exclusion lifted): eye max 0.00098. New `east`: 25-85 eye 0.0, 225-660 0.00297 (86-224 skipped: the movement replay is 0.25 off from a wall push on) |
| sim (ROM) `corridor_camera_matches_the_game` | reads 113-777 (was 113-335): 113-335 0.0042 (unchanged), 336-642 0.0 (was 39.38: the test fed the facing for obj+0x40, which faces the wall in state 122; `c34/obj.csv` now feeds the game's), 643-777 0.0 (was 40.0, 508.5 at 677) |
| camera unit | `a_fixed_hint_holds_the_eye_short_of_the_look_at` (east read 24's eye bits; dead and aiming; the springs 1.0111/2.0133 after, V swim1k 483), `the_last_hint_the_player_is_in_decides_the_fixed_camera`, `a_swept_eye_keeps_clear_of_a_ledge_the_fast_path_misses` (c34 643: eye 119, floor −48), `the_scope_zooms_as_one_view` (h1 reads 201, 245, 270 to the bit). sim `a_spawn_in_a_fixed_hint_cuts_onto_its_camera` |
| rules (ROM) `the_gun_bullets_knock_slides_as_e1` | e1's reads 102-129 to 0.01 in position, 1e-4 in speed (V's angle 0x9595), at rest (2086.506, 645, 804.079); bots' `mounted_guns_m2_and_e1`: "P2 knocked 140.9" |
| client | `a_dead_swimmer_drowns_until_the_respawn`; `a_first_death_twitches_one_time_in_eight` (call k hits k/8 ± 0.015 over 8000 deaths), `the_twitch_comes_63_frames_into_the_dead_pose`, `a_body_that_lies_draws_again` (V out1: a miss at 63, a twitch at 127), `every_client_draws_the_same`, `a_head_kill_pops_to_the_respawn`, `the_pop_hides_the_head_joints` (16 models); `the_fire_clip_kicks_the_scopes_eye_as_k2` (reads 105-180 within 0.074); `villager_heads_match_the_oracle` (b4 t 418/618, b3 t 6/310, within 0.5°); `the_outro_tanks_run_their_engines`; `the_egg_carrier_keeps_the_on_foot_camera` ([247, 100, 61.5], aimed [90, 86.5, 86.5]); the dots test: 177.35° at the open (was 136°) |
| `bun run test` (all lanes' edits in the tree) | pass: 639 tests, 1 ignored (with `CONKER_ROM`; conker-camera's now included) |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |
| Pack (`../lab/tools/verify-pack.sh`) | pack equal (2103 files); no re-export |
| `smoke.sh` (`MAP=30`, port 5976) | pass: three clients, prediction error 0, 0 rollbacks (the knock's change predicted as the server steps it) |
| `bots.sh` (`MAP=2b MODE=beach BOTS=7 HUMANS=2`, port 5974, 120 s) | every bot moves and is seen moving; no fault, unknown native or panic |
| Screens (`../lab/screens/cameras-g4/shots.sh`, ports 5974/5976, debug binaries, 1280 × 720) | `fixed-east.png` (a villager on the east beach in hint 0x26: the eye held off on the line from point 0, the villager small on the boardwalk), `fixed-west.png` (a swimmer carried along the west shore in hint 0x27), `follow.png` (a swimmer off the pier out of both hints: the follow camera behind it), `scope-open.png` (the sniper scope at the open: the dots turned 177°), `outro-cs4.png` (Beach cs4: the villagers' heads turned up in joint 0's frame) |

**Findings.**
- **The egg carrier's camera** (V carry2, carry3): mode 8, s126+0x78 = 0, targets 247/100, head 90; R opens the
  unarmed aim. Nothing to port. `lab/screens/raptor/egg-carried.png`'s eye sat in the rock because `--warp` moves a
  player 12,000 units without a camera cut (the original does the same under a poked teleport).
- **Corrections to earlier notes:** the head pop hides joints 12-15 on every model, not only the zombies' (every player
  object has obj+0 = 1; conker-rules `parts.rs`' zombie-only `hidden` is wrong too). The villagers' head turn is case 0
  as the others' (the m2c `arg12` = 1 was an unused argument); its base is joint 0.
- **A swimmer's death** (V die1, die2): 0x24 from the hit's next frame, the respawn 88 frames after the hit, the body
  afloat and drifting with the current; the camera stays the follow camera.
- **The gun's muzzle** (V k2): the shot's eye is the pose drawn the tick before, its muzzle and sight the pose after
  this tick's advance; with that the directions match to 2e-4 (at rest 0.005-0.046 off).

**Handoffs (G4c, rules; details in the research doc).**
- A lethal hit on a swimmer: one 0x24 state of 88 frames, the body floating on the current (today `damage::sequence`'s
  163 frames for 0x20 and `knock()`'s ground movement). If the drift ashore lands, the client must latch "died
  swimming".
- `damage::sequence`'s (0x33, true) and (AREA_STAGGER, true) become plain `(DEAD, 81)` (the client draws the twitch);
  the replays expecting 0x18A on e_w38, e_w21, sg_e2 then expect 0x59.
- The corpse's head pop: a per-fighter pop mask (0xF000 on a pop, 0xFFFE on a 0x2F kill, 0 at the respawn) used as
  `parts.rs`' `hidden` for every model, and a bullet on a dead head producing it (`hurt()` returns false at health 0).
- The gun's muzzle half: `fire_gun` at tick n with the eye on kick(n − 1), the muzzle and sight on kick(n), the clock on
  the `Mount`, the table from 138's skeleton.
- `--warp` should raise the spawn cut (cam+0x23C).

**G5 pack list.**
1. The camera points [0x0C, scene, 8] (0x18-byte records, s16 x, y, z at +0; 0x2B's 6, 0x1A's 4), for
   `CameraLevel::new` in place of `SEA_POINTS` (and Heist's trip-wire hints 0x40-0x43, commands 5/0x11, not ported).
2. Sound 0x4A9 (1193), the tank's rumble: a `tank_cues()` cue `record([1193], 0x2EE0, 200, 800)` for character 119
   (`audio/characters/119/1193.wav`); the client's timers are in the research doc.
3. The shotgun's and chainsaw's head stumps: bank-09 #0x59 (action 0x73, joint 0, model 117) and #0x58 (action 0x72,
   joint 1, model 128) as held models in `weapons/deathmatch/held.json`.

**Left.**
- I: the gun eye's fades (reads 103-104, 181-183 off by 0.1-0.85: the pose mixer is unread); `smooth::Drawn`'s
  corrected view has no kick. The knock's speed cap on the ground (no V). The fixed camera's floor refresh (cam+0x35C)
  in mode 0x40; whether cam+0x6D2 stays set after a respawn into the same hint.
- C, not exercised: R's aim's sweep without a floor clamp, the other sweep sizes, `func_1510E950` clearing the
  triangle on a miss (`cameras-clips-g4.md` §3).
- Colors cs5's other tank binds segment 7 to noise (I: a stale binding); the pack draws it untextured.

## Effects: research and the first ports (2026-10-14)
**Lane G4a, wave G4.** A recipe for each of the 14 effects is in `../lab/research/mp-original/effects-g4.md` (scratch:
`effects-g4-scratch/`, including `beach.md` and `maps.md`). Each covers the trigger, the timing and motion, the
geometry, the texture, the blend, the sound and the pack's export, with every claim tagged V, C or I.

The client now draws, on every map, the effects whose textures are already in the pack. They live in the new
`conker-client/src/effects.rs`. Each is a port of the original's 2D sprite (`func_15130280` / `func_151303EC`): a
camera-facing square of 0.1464 × its size, with its life counted in VIs, its fades, its y change and its drag.
- **The guns' flashes.** One spawns each time a player's `Combat::shots` goes up with a weapon drawn, using a random
  one of `hud/race_exhaust_0..3`. Life 7-10 VIs, alpha 255, then life × 85 below 3 VIs. Placement:
  - the twin guns alternate between their two V muzzle points;
  - the other guns use the rules' bullet muzzle (I);
  - the rocket launcher fires two flashes, one ahead of the tube and one behind it (V).
- **The rocket.** Its glow follows it, and a streak runs from it back to a tail that leaves the launch point at
  50 a frame, untextured. Its trail puffs are `explosion.png`, one every 90 units (I), riding at 0.6-0.76 of its
  motion and darkening over their last 25 VIs.
- **Blasts.** Where a rocket or a grenade leaves the match's lists:
  - 3-7 columns of 7-10 `explosion.png` puffs, each puff slower and bigger than the last;
  - their motion keeps 0.7744 a frame, and their colour class fades in below 20 VIs;
  - the rocket's blast also leaves 60-90 frames of rising, growing `fire.png` smoke.

  I: the columns rise straight up, and the grenade uses the rocket's numbers.
- **Flame puffs.** While a player's clip is the flamethrower's fire clip, `puff.png` puffs spawn at 0.23 a frame from
  the nozzle (life 60-80 VIs, the push fitted to V).
- **0x34's electric sparks.** A `shell.png` glow sits at both ends of each arc and bolt, in the arc's colour. `traps.rs`
  pushes the glows through `effects::Queue`.

**Not ported:** what the pack lacks (recipes in the doc), the race board's trail (its exhaust point is not decoded) and
cs3's bubbles (`outro.rs` is lane G4b's).

| Check | Result |
|---|---|
| `effects::tests` | `flashes_as_the_original`: the V flash points within 0.1, 4-5 frames, the last at alpha 85 or 170. `the_rocket_trail_as_the_original`: the trail's motion share and fades, and the streak's tail at V DLs 120/122. `the_blast_as_the_original`: the column law, V's drag step (−387.72 → −334.66 → −262.5), the smoke's growth, the arc glow. `the_flame_puffs_as_the_original`: the rate 0.2255 and the mean push (2.84, 3.84) |
| Screens `../lab/screens/effects-g4/` | `shots.sh twin` (5970): `twin.png`, `twin-side.png`, the flash at the guns. The client logged the flashes at exactly V's (−2228.99, −458.66, 5974.02) / (−2269.32, −476.56, 5966.6). `rocket`: `rocket-side.png` shows the glow, streak and trail. `blast`: `blast.png` shows the launch pair, `blast-side.png` the columns and `blast-smoke.png` the columns rising. `flame`: `flame.png` shows the puffs at the nozzle. `electric.sh` (5972): `electric*.png` shows the glows on the arcs and the bolt's foot. The oracle's matching texrects are in the e8 runs' hudwalks (split-screen, so position and size are compared in world units, not pixels) |

**G5's pack list** (the full table is at the doc's end):
- textures: 0x3CF, 0x11FE, 0xBA7, 0x721, the ten ripple frames, 0x4B3 (it replaces `bubble_particle`'s 0xC01),
  0x8FF-0x908, 0x4BD;
- bank-09 models: #0x132, #0xE5, #0xBE;
- moves: the ring 0x157 and puff 0xBA6 out of the 0x2B and 0x34 folders, to shared names.

**The wire is unchanged** (no `net.rs` type; `PROTOCOL_ID` untouched). **The shared simulation is unchanged** (client
drawing only). The pack is unchanged (no `conker-extract` edit).

## Rules and bots: G3's leftovers (2026-10-14)
**Lane G4c, wave G4.** Notes: `lab/research/mp-original/rules-bots-g4.md` (+ `-scratch/`).
- **0x34's, 0x3F's and Beach's start records are drawn as the level script does.** On 0x34 and 0x3F (Deathmatch),
  the first record is `random(8)` and slot s starts on `(first + s) % 12`. On Beach, the Tediz draw `random(2)` on
  `[3, 10]`, then the villagers draw `random(3)` on `[0, 1, 2, 11]`. The other maps keep their tables. This was read in
  scripts 0xBD, 0xBE and 0xB9 and matches 10 oracle captures; the draw depends on the roster (1 + 3: 0, 1 + 5: 3,
  2 + 3 and 1 + 15: 7).
  - `spawn::Starts::draw`: the server draws once a run from the match's RNG. A rematch keeps everyone's place.
  - The harnesses (`bot_match`, the ROM `play`) start 0x34 and 0x3F on the oracle's draw.
  - Test `the_scripts_draw_the_first_records`: each capture's records and the RNG after.
  - Re-check of 0x34 1 + 3 with matched starts (8 seeds each side): deaths 6.2 against 7.8, hp drops 28.8 against 22.0.
    The twin guns fire 302 shots a run against 107. The older comparison, 7.8 deaths and 18.3 drops, had mismatched
    starts.
- **0x34's electric floor is heard.** While the floor is live, the AI's noise list holds three noises of radius 500 at
  trigger records 60, 59 and 58. These are `func_15105BC8`'s objects. They are heard on the first 150 of the floor's 151
  frames, ahead of the jets (V `elec1`, and the oracle's 15-bot runs). Bots in that radius walk out, as they do from the
  jets. The remake had no noise there; its comment said "the electric floor has no emitter", from a run where no bot
  heard it.
  - Test: `the_jets_are_heard_while_they_burn` now covers the floor too.
  - 0x34 with 15 bots, 1,737 frames (oracle o15s0/3/6/9; remake seeds 1-6):

    | | Oracle | Remake before | Remake now |
    |---|---|---|---|
    | Box frames a run | 232 | 527 | 348 |
    | Visits | 3.8 | 7.0 | 5.2 |
    | Standing | 2.5 | 62.5 | 28.7 |
    | Deaths on the floor | 0.25 | 1.5 | 0.5 |
    | Box entries in state 11 (a heard noise) | 0.25 | 7.2 | 3.5 |
    | Bots seeing an enemy from the box | 39% | 73% | 59% |

  - Ruled out:
    - The rules refusing hits on immune or stunned players (C, 0x60000 on the trap damage types).
    - The draw and holster lock (V: 183 of 189 draws and 148 of 149 holsters stop).
    - The sight ray: 1,402 of 1,418 oracle flips agree, 12 of 12 in the box.
- **Race bots kick as the original's.** Script 0xBC pairs every B press with class script 0x1F. That kick is 12 frames
  of state 84, with no hop. The victim is tested on the 12th frame. The target speed is 60 on the first frame and on the
  frame after the last. The oracle runs show this in R11r1-r3 and R12r1-r2, and R11r1 214 knocks the idle human.
  - Before, the bots used the human's 29-frame kick.
  - `board::Board::bot`: server-only, `serde(skip)`, so the wire is unchanged. It is set by `Board::class(0x1F)`.
  - Test `a_bot_kicks_as_r11`.

  | Route, seeds 1-6, 3 bots | Oracle | Remake before | Remake now |
  |---|---|---|---|
  | 11: laps 3 to 5 | 836-837 | 836-838 | 836-838 |
  | 11: finishes after the go | 4131-4268 | 4130-4213 | 4150-4248 |
  | 12: steady laps | 1063-1081 | 1064-1081 | 1061-1081 |
  | 12: finishes after the go | 5122-5213 | 5015-5179 | 5006-5192 |

  The 1061 is seed 6's lap after a death. The idle human's deaths on route 11, seed 2, are 376, 596 and 844, against
  the oracle's 378, 599 and 845.
- **Left.**
  - *0x34:* the bots still fight in the pit. They stand 28.7 frames a run against 2.5, mostly grenade pulls (11.8) and
    hits (7.3). The oracle's bots make no grenade noise within 1,600 of the box, but the remake's make 168 noise-frames
    a run there (before this fix). Not traced.
  - *Tank's hit rate (findings only):* the same shell tool on both sides (oracle h1-h5 and h7; remake seeds 1-6):

    | | Oracle | Remake |
    |---|---|---|
    | Hits per shot | 27.6% | 30.2% |
    | Shots under 2,000, hit rate by aim error 0 / 1 | 32.5% / 29.2% | 39.3% / 39.0% |
    | Hit rate at 250-750 | 40% / 18% | 49% / 34% |
    | Hit rate at 1,250-2,000 | 9-19% | 19-29% |
    | Shots beyond 2,000 a run | 60 | 84 |

    Point-blank shots match. The hit volume matches: hits to 100, mixed at 100-125, then misses. Neither side's shells
    take the hull's speed. So the remake's aim at mid range is tighter: the lead, the gun pitch's convergence, or the
    targets' motion. Not resolved.
  - *Heist's attack share near the carrier (71% against 75-84%):* not worked. G3e's findings stand.

## Bots in Heist, Tank and Race (2026-10-14)
**Lane G3e, wave G3.**
- **Tank.** The scope's Z fires a bullet now, as the original does: instant, 1 damage, 2 frames after the press. The
  bots' shell deaths went from 7.5 a match to 12.4 (the oracle's 11.5). Four changes did it:
  - the gun aims at the script's enemy;
  - the bots reach pickups at the original's ×1.4;
  - the class script is held through the hit clip;
  - the health reads stale on the hit's frame.
- **Race.** On route 12 the centre-line bot no longer dies at the chasm (it did in 5 seeds of 6; the original never
  does). The cause was the floor query's reach: 150 on the Race map, against the remake's 50. Above that reach, ceilings
  no longer count as floors. The bots turn up to 1792 a frame, as the original's do. The missile's reach checks out. The
  bots' kicks are findings only.
- **Heist.** The attack share near the carrier is 71% now, against the original's 75-84% (61% was before L1's fixes).
  The Tank mechanisms don't move it. The takes match: the ROM path averages 20.7 and the oracle 21.25. Server: see below.

Research: `../lab/research/mp-original/mode-bots-g3.md` and `mode-bots-g3-scratch/`. The oracle runs used the private copy
`ConkerRecomp.g3e`, an idle P1 and the RNG poked. Traces are not kept:
- Tank: h1-h5 and h7 (7 bots), sc3-sc5 (two humans, the scope).
- Race: R12r1-r3 and R11r1-r3 (3 bots, P1 with 99 lives, 5 laps).
- 12 s RAM reads of `D_800DBE64`.

**The wire is unchanged** (no `net.rs` type; `PROTOCOL_ID` untouched). **The shared simulation changed:**
- `Match::tank_player`, which the client predicts with: the scope shot, and a bot's gun pitch from `Tank::aims`.
- conker-sim's board: the floor query `floor_up` (step 150, faces up only), and `AUTO_TURN` 1792.

`Tank::bullets` and `Tank::aims` are not on the wire.

| Part | What |
|---|---|
| conker-rules `tank.rs` | **The scope shot**: Z with the scope on pushes a `Bullet::aimed` (`MG_BULLET`, `BULLET_SPEED`) from `SCOPE_EYE` (27.3 ahead, 111.6 up), level along the turret, into `Tank::bullets`. `fly_shells` flies it from the next frame against the hulls (`hits_hull`) and takes 1 hp with `tank_hit` (V sc3-sc5). **`Tank::aims`**: each bot's gun target this frame. `tank_player` pitches a bot's gun by `range_error` toward it, and a human's as before. **`touches`**: the pickup radius 100 · reach/64, × `race::AUTO_REACH` for a bot (C `func_1511DF6C`), for the bomb and the power-ups |
| conker-rules `weapon.rs` | `within(pickup, player, r)`: `touches`' test for any radius |
| conker-rules `gas.rs`, `game.rs` (shared, two lines each) | `Effect::TankAim(slot, at)`, applied to `Tank::aims`. In Tank, pickups touch by `tank::touches` |
| conker-bots `lib.rs` (shared) | Tank: the gun aims at G+0x11C (ENEMY) at any distance, without the sight test, led by the target's velocity × distance/100. It pushes `TankAim` (V h1: s126+0x129 = G+0x11C in every state). A tank bot in its hit clip (`Action::Hurt`, non-lethal) doesn't run its class script, and its wait is 0 (V h1 1596-1610) |
| conker-bots `brain.rs` (shared) | Tank: on the HIT_BY-0x80 frame, a victim whose slot is after its shooter's reads its health before the hit (V h1, h4: 35 of 38; 6 of 49 the other way) |
| conker-sim `collision.rs` (shared) | **`floor_up(x, z, y, ref, step)`**: the floor query with the step a parameter (`func_1510E950`'s `D_800DBE64` for an object, which `func_15002F40` sets: 150 on 0x32/0x33, else 85 in multiplayer; V RAM). It has no fast path and skips triangles facing down (V R12r1 1880-1885). `floor` is unchanged: it shares the loop (`query`) and keeps 50 |
| conker-sim `board.rs` (shared) | `STEP_UP` 150 for the board's floor. **`AUTO_TURN` 1792** (0x700; V R11r1-r3 and R12r1-r3 at the start and on sharp bends; it was 833) |
| conker-bots `tests/rom.rs` | `tank_bots_bomb_and_fight`: shell deaths asserted 9-14 (was 5-14). `race_run` also returns each bot death's track count. `race_bots_finish` asserts that no route-12 bot dies at the chasm (count % 33 = 22) and prints the deaths |

| Check | Result |
|---|---|
| rules `the_scope_shot_as_sc5` (new) | Straight ahead, the target loses 1 hp with `CLIP_HIT` on the second frame after the press, not the first. 150 to the side, it misses. No shell flies |
| sim `the_boards_query_reaches_its_step_and_skips_ceilings` (new) | A floor 120 and a ceiling 100 over the point: `floor` finds neither, `floor_up` 150 finds the floor, `floor_up` 110 nothing |
| bots ROM `tank_bots_bomb_and_fight` (seeds 1-4) | Shots 329, hits 28.9%, shell deaths 11.5, blast deaths 13.2, plants 3.8. Longest carry 1199 (oracle 1197; 1634 before the stale health) |
| bots ROM `race_bots_finish` | Route 11 (seed 2): laps 836-837, the human's deaths at 377, 597, 845 (L1 378, 599, 845). Route 12 (seeds 1, 2): steady laps 1065-1081, the leader out after 3 missile kills, no chasm death. 7 bots: the mean finishes within 5% |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`) | `bun run test` 577 passed, 0 failed, 1 ignored (with the ROM). `bun run check` and the wasm clippy pass |

| Run (ROM path; oracle) | Before | After | Oracle |
|---|---|---|---|
| Tank, 7 bots, seeds 1-12: shell deaths, hits of 1, hit rate | 7.5, 88.5, — | 12.4 (10-15), 98 (86-115), 29.3% | h1-h5, h7: 11.5 (10-13), 91.5 (81-105); G1's ten: 10.5, 89.5, 25.6% |
| Tank: hp ≤ 2 retreats a match, COOL 60 / 600 that run long | 0 / 8.8 | 6.1 / 6.0 | 7.5 / 5.2 |
| Race route 12, 3 bots, seeds 1-6: chasm deaths, the leader out, steady laps | 5 of 6 (+1 later), 5 of 6, 941-1081 | 0, 6 of 6, 1064-1081 | R12r1-r3 + L2: 0, 4 of 4, 1063-1081 |
| Race route 11, 3 bots, seeds 1-6: laps, finishes after the go | 836-837, 4164-4214 (G1) | 836-837, 4130-4213 | R11r1-r3: 836-837, 4131-4268 |
| Heist, 3 bots, seeds 1-12: state 8 within 500 of the enemy carrier, takes | 71.3% (45.6-96.0), 20.7 (14-31) | unchanged | 75-84% (4 samples), 21.25 (17-30) |
| Server smoke (`bots.sh`, ports 5986/5988) | | Heist, 3 bots, 5 min: seed 1 takes 9 to the end at 8160 (3 deliveries); seed 2 takes 17 to 6587 (4; its bot 2 hit no one and bot 3 never died, two FAIL lines). Tank, 3 bots, 5 min: all ok, every bot hits, dies and respawns; bombs 5 taken, 4 planted, deaths 2 by blast and 1 by shell, 2 power-ups. Race route 12, 3 bots: all ok; the idle human goes out at 1150, 3319 and 5502, which decides each race before a lap (5 missiles, 8 knocks). The bots' one death is bot 1 in race 2, 11 frames after slot 2's missile knocks it up, not at the chasm | |

**Findings** (V the runs; C the code; details in `mode-bots-g3.md`).
- **The floor query's reach** (C `func_15002F40`; V RAM). It is 150 on scenes 0x32 and 0x33, and 85 on every other
  multiplayer map (Tank 0x24 seen). Only the board takes it now. Everything else keeps 50: on foot, the tanks, the
  raptors, the swim, the follow camera.
- **The missile** (V, 12 knocks). It hits on the frame after a frame-end distance of 108-198 and never after 225-296. The
  remake's rule (within a step of 200) matches 11 of the 12. `MISSILE_HIT` 120 matters only under a step. C: the radius is
  the victim's obj+0xD2 (46) plus 4 · scale · the attack record's +0xE.
- **The bots' kicks** (V L1, R12r1, R12r2). A bot kicks for 12 frames of state 84 with no hop. The human's B kick is 29
  frames with a hop. On the start kicks the target speed is 60 on the first and last frames, so the speed drops by up to
  25 toward it (L1 bot 3: 73 → 60 at 276, 71 → 60 at 288). Other kicks lose 25 at their end (100 → 75).
- **Heist.** Tank's two mechanisms, widened to Heist, left the carrier share at 71.4%, so they were not kept there. In the
  remake, COOL 600 at hp ≤ 2 starts 5-9 times a match.

**Left.**
- **Tank** runs a little hot: hits +7%, a 29.3% hit rate against 25.6%. A victim before its shooter in slot order is
  fresh, as the original is; it is not tuned further. The stale health in other modes isn't checked.
- **Race's kicks.** The bots still kick with the human's 29-frame B kick. The port: class script 0x1F's 12 frames and
  its speed steps (the script is unread).
- **Heist.** The near-carrier share is 4-13 points under the oracle's samples, within the seeds' spread. The server's bots take the bag 16.6 times per 9000 frames (four runs: 9, 17, 17, 14 to frames 8160-9317), against the ROM path's 20.7 (14-31), so they are within its spread. The server's matches end at the score, and a delivered bag is off the map for about 230 frames. This is not traced further.
- **85 on the other maps.** The floor query's reach for the simulation lane: `floor` stays 50 everywhere but the board.
- **Handoff, client `tank.rs`.** The scope shot is a bullet (`Tank::bullets`, not on the wire). The client draws no shell
  for it, which is right, and nothing else: the original's flash or tracer for it is not traced.

## Deathmatch bots: the weapons and aim (2026-10-14)
**Lane G3d, wave G3.** **No AI change.** The gaps in the task came from 5 oracle seeds. With 8 more (13), the
original shows the same long holds and stands the remake does, and the twin guns, sniper and 0x23 fall within noise.
The only code change is the harness: `bot_match` now counts the sniper's shots. Found out of lane: **0x34's start
records depend on the player count**, so the remake's (s + 7) % 12 is wrong for 1 + 3 and 1 + 5 (below). Research:
`../lab/research/mp-original/dm-bots-g3.md`, with `dm-bots-g3-scratch/` (`tools/run.sh` on a private
`ConkerRecomp.g3d`). The remake runs used this tree with the other G3 lanes' edits in progress.

**The wire did not change, and neither did the shared simulation.**

| Part | What |
|---|---|
| conker-bots `examples/bot_match.rs` | The sniper's shots are counted at each start of its fire clip (`weapon::SNIPER.fire.0`), since `Match::ray` leaves the rules' `shots` alone (I: one clip start is one step of s126+0x1AB). They go into the `shots` column and the summary |

| Measure (0x30, 1 idle human + 3 bots, 4,519 reads) | Oracle, 13 seeds | Remake, 8 seeds |
|---|---|---|
| sniper 0x24 spells: count, mean / median | 25, 1,236 / 1,071 | 12, 1,895 / 1,235 (Mann-Whitney p 0.26) |
| 0x23 spells: count, mean / median | 33, 556 / 285 | 19, 883 / 396 (p 0.08) |
| longest: sniper camps never killed; 0x23 | 3,913, 3,466; 4,371 (role 1), 2,226 (role 3) | 3,998, 3,974; 4,347, 2,708 (both role 3) |
| twin guns: shots / hp drops a run, rate | 306 / 17.3, 0.056 | 406 / 13.0, 0.032 |
| twin guns under 3,000 xz | 0.074 | 0.063 |
| twin guns 3,000-5,000 xz: shots / hits a run | 100 / 2.4 | 207 / 0.6 |
| sniper: shots / hits a run, rate | 6.5 / 1.9, 0.294 | 11.8 / 3.0, 0.255 |
| 0x23: rate | 0.096 | 0.090 |

**Findings** (V the runs; C the code; I inferred).
- **No hold or switch rule** (V). A hold ends by death, a take, a kill-gate fall or the match's end. G+0x2378 is
  always 0. Forced into role 3 at the start (3 runs), the original also keeps 0x23 drawn in state 9 for 1,716 and
  2,312 frames, as the remake's role-3 starters do (r30s3, s5).
- **The twin-gun gap is one stand, and the original makes it too** (V). In r30s1 and s2, p3 stands at
  (−1671, −1402, −4962) and (−1727, −1402, −4840) and fires 747 and 738 shots, 0 hits, at the human on its respawn spot (150, −916, −7639).
  Oracle n30s4: p2 at (−1667, −1402, −4942) does the same, 438 shots, 0 hits, 2,411 to the end. Forced there (aim3b),
  it lands 1 hit in 109. The pitch byte obj+0x1D1 (`func_1504C9E4`'s ramp) feeds the pistol's and 0x23's bullets, not
  the twin guns' (C). It is not ported, and porting it would not move this rate.
- **0x34's start records** (V). With 1 + 3 every oracle seed starts slots 0-3 on records 0-3; with 1 + 5, records 3-8.
  With 2 + 3 and 1 + 15 they start on 7 onwards, the remake's `DEATHMATCH_START_34`. The offset is probably drawn at
  setup, per match (I). Every 1 + 3 comparison on 0x34 (the tables further down included) starts the two sides
  differently. 0x30's table holds for 1 + 3.

**Left.**
- **The electric floor** (0x34, 15 bots, 1,737 reads; the start table is V for 16 players). The remake's bots spend 527
  frames a run in its box against 151 (t34, 4 runs). They visit 7 times against 3.25, and stand still 62 frames
  against 0. The long visits are state 4 (goal 82/72/74) or 11, the bot stopped to attack (script 5). Over all of 0x34
  the share of drawn frames standing in state 4 matches (0.79 against 0.71), so the difference is local to the box.
  Not traced. The harness's 140 frames a run alive on the live floor (oracle 15) may be `trap_hits` refusing immune
  or stunned victims, not the bots (not checked).
- **Remake 0x23 holders die more**: 6 of 19 spells end in death, against 1 of 33 (untraced; it shortens the holds).
- **The crossbow's pitch** was not looked at. Its 0x34 hit rate is 0.221 against 0.226, but on mismatched starts.

| Check | Result |
|---|---|
| Oracle runs (`run.sh`) | n30s0-s7 (0x30), n34s0-s7 (0x34), role3a-c (role 3 at read 25), aim3/aim3b (the stand), w112* (the 112 camp) |
| `bot_match` (release) | r30s1-8 (0x30, 4,496 frames), r34s1-6 (0x34, 4,403), r34b15s1-6 (0x34, 15 bots, 1,760) |
| `bun run test` | 576 pass, 7 ignored (with the other G3 lanes' work in the tree) |
| `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | Both pass. Extra check: clippy `-p conker-bots --all-targets` fails only on `tests/rom.rs`:2236 `type_complexity` (G3e's `race_run`, in progress) |

## Prediction: the leftovers (2026-10-14)
**Lane G3c, wave G3.** The local player's frame now predicts what it touches:
- 0x34's pressure plates, fire jets and cones;
- the Colors flags' and the Heist bag's push;
- the mounted guns' touch and mount;
- 0x2D's sliding doors and covers, with the collision now on the right tick;
- the crossbow bolt's push.

The smoothing details are fixed too:
- a correction during the camera hand-off;
- the drawn camera's hint cut;
- the local player and the view drawn between 30 Hz ticks.

Every live number below compares the server's `STATE` lines with each client's. A "tick over" is a tick where the predicted position is more than 0.001 off. "Rollbacks" are lightyear's. The setup is 2 scripted headless clients over WebSocket on port 5978, `--bots 0`, with debug builds. "Before" is the tree before this lane.

**The wire changed:** `Fighting` (conker-rules `net.rs`, sent to its owner only) gains three fields:
- `traps: Option<traps::State>`: 0x34's plates, and each plate's trap countdown and owner;
- `sliding: Option<[gas::Prop; 8]>`: Total War's doors and covers;
- `bolts: Vec<(u32, [f32; 3])>`: the player's sitting bolts, with their birth ticks.

`PROTOCOL_ID` is left to the wave's bump.

**The shared simulation changed in two places:**
- `Match::step_player`, the client's frame, now runs these for its one player, in `step_seen`'s order: the trap hits, the cones, every mounted gun (`mounted_guns`, which was only its own gun's `fire_gun`) and the plates. `trap_hits` and `cones` take a player range.
- The server seats 0x2D's collision from the props after this tick's slide (`Gas::slid`). It was one tick late: the moves collided with the doors where they stood before the in-step `gas.step()`.

`step_seen` is otherwise unchanged, and the rules tests and replays pass.

| Part | What |
|---|---|
| conker-rules `traps.rs` | `traps::State` (the plates; per plate, its first trap's countdown and owner), with `offsets()`. `Traps::state` and `set_state`. `Plate` gets serde |
| conker-rules `gas.rs` | `Gas::step` is public. New: `slid()` (a copy after the slide), `sliding()` and `set_sliding()` (props 0-7). `Prop` gets serde |
| conker-rules `game.rs` | `step_player` as above. The bolt push's comment notes where a client's bolts come from |
| conker-rules `net.rs` | `Fighting::traps`, `sliding` and `bolts`. `Fighting::shots()` rebuilds the bolts as `Shot`s |
| conker-server `main.rs` | 0x2D's `gas::seat` from `game.m.gas.slid()` |
| conker-client `lib.rs` | **`MatchArena`** keeps 0x34's `Traps`. **`own_tick`** seats into its `Match`: the `MatchView`'s flags, bag and guns (`seat_view`), the predicted traps, props and bolts, and `live`, from the view's start (the bolts' first move). **`seat_props`** seats 0x2D's and 0x34's collision from the predicted fighter: 0x2D's after the slide, as the server's. **`predict`** keeps what was drawn as the tick started |
| conker-client `traps.rs`, `render.rs` (`show_pickups`) | Draw the plates' sink and the doors' and covers' lift from the prediction. Before, they came from the `MatchView`, a lead behind the collision |
| conker-client `smooth.rs` | **Hand-off.** The view's weight is `handoff / HANDOFF` throughout. A correction keeps it, and it climbs back by 1/8 a tick over the blend. Before, the correction reset it to 1 at once. **Hint cut.** The drawn body's camera takes the follow camera's `hint_cut`. **`Frame` and `between`**: the frame `f` of the way from the frame drawn as the last tick started |
| conker-client `render.rs` | `place_players` and `follow_camera` draw the local player and the view `between` the ticks, by `Time<Fixed>`'s overstep fraction. Nothing is interpolated across a follow camera cut, a respawn or a snap |

| Run | Before | After |
|---|---|---|
| 0x34: client 1 steps on plate 6 and walks into its own fire jet (a suicide); client 2 runs into a cone | Client 1: 8 ticks over, max 100.7, 6 rollbacks. Client 2: 3 over, max 103.4, 2 rollbacks | Client 1: **0 over**, 3 rollbacks. Client 2: **0 over**, 4 rollbacks |
| 0x34: slot 0 circles over plate 6 (on and off it), slot 1 idles | Circler: 30 over, max 10, 52 rollbacks. Observer: 2 rollbacks | Circler: **0 over**, 3 rollbacks. Observer: 18 rollbacks, none moving it (left, below) |
| The same with the circler in slot 1 | Circler: 30 over, max 10, 18 rollbacks. Observer: 1 rollback | Circler: **0 over**, 3 rollbacks. Observer: 15 rollbacks |
| Map 30: walk into mounted gun A and mount it | 5 over, max 61.5 at the mount, 8 state misses, 2 rollbacks | The mount is predicted. Only the start-up errors are left (max 12.4 at the match start), 1 rollback |
| Colors: a Tedi runs into the red flag and pushes it | 29 over, max 31.2, 34 rollbacks | **0 over**, 1 rollback |
| 0x34: crossbow given (`--give 0:60:0x18`), drawn, then 4 hip shots 60 reads apart, standing | Shooter: 15 ticks over, 5.02 at each shot for 3-4 ticks (the bolt's push), 4 corrections, 6 rollbacks. Idle client: 1 rollback | Shooter: **0 over**, 0 corrections, 2 rollbacks. Idle client: 0 over, 5 rollbacks that move nothing (both runs alongside the test suite) |
| 0x2D: a canister delivered, then walk through cover 3 as it opens | The server let the player through at delivery + 3. The client was 4 ticks over (7.5, 15.5, 24, 25.5), 3 rollbacks | The server lets the player through a tick earlier, as the slide stands. The client is 3 ticks over (7.5, 15.5, 24): the delivery itself is the server's. 3 rollbacks |

| Test | What |
|---|---|
| rules `a_players_own_frame_rides_plates_and_takes_traps` (new) | A world with a plate, a fire jet over it and a cone, set up as 0x34's records. Each tick, `step_player` must equal `Match::step` in the player, `traps.state()` and the hit events. The plate sinks, the jet kills (health 0, 1 hit), and the cone burns |
| client `the_prediction_takes_its_bolts_push` (new) | A crossbow hip shot. Each tick `own_tick` runs on a round-tripped `Fighting` and must equal the server's body (pushed 2 to 10 units by the bolt). Without the bolts seeded it fails at tick 81 |
| client `a_correction_during_the_hand_off_keeps_the_views_weight` (new) | The follow camera 60 units off, at weight 2/8: the view is equal before and after a correction (the old code jumped 45 units). The weight climbs back to 1 over the blend |
| client `the_drawn_bodys_camera_cuts_on_the_follow_cameras_tick` (new) | A trip wire across z = 0 and a drawn body 40 behind the predicted one: both cameras cut on the same tick (the old code's drawn camera cut on another) |
| client `frames_between_ticks_are_drawn_the_overstep_of_the_way` (new) | Halfway: the body and the eye at 17.5 of 35, the facing turning the short way. With no `prev`, or after a reset, it draws the tick |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | `test`: 576 pass, 7 ignored (with the other G3 lanes' work in the tree). Wasm clippy is clean. `check` passes in full (fmt, clippy on every package, the web app's check) |

**Left:**
- **Others' plates.** A remote player's step onto or off a 0x34 plate costs the other clients rollbacks that move nothing (an observer: 2 → 18 and 1 → 15 rollbacks): the plates are compared in `Fighting`. The upgrade is to predict the plates outside `Fighting`, from the `MatchView`'s confirmed state.
- **The mount's RNG.** `Mount.rng` is drawn from the match's RNG. When the server's RNG moves in between, the mount costs one rollback, which doesn't show. It didn't happen in the run.
- **What `seat_view` copies:** the flags, the bag and the guns.
  - They come from the confirmed `MatchView`. Their moves since it (another player taking a flag, a free gun's pitch decay, its leave count) reach this client as corrections.
  - A touch right after another player's leave can mispredict.
- **Server decisions.** A Total War delivery's effects are the server's, so they stay as corrections (the 3 ticks above).
- **Other players' bolts.** Another player's sitting bolt is not in this client's frame, so its push on the local player is not predicted. It only reaches a player standing on its shooter.
- **Tank's doors.** They stay on the `MatchView` (`tank::seat`).
- **What is still drawn per tick:**
  - the other players (their interpolation's whole ticks);
  - the radar's orientation and the audio's listener (`Drawn::view`; the HUD's screen points project through the drawn camera).

## Match flow and the crossbow's release (2026-10-14)
**Lane G3a, wave G3.** The crossbow's aimed shot was already fired on Z's release, as in the original (V: the new
replay `combat8_xbl`). G2's "fires on the press" was wrong. What the remake lacked was **the aim laser's state**. It
is now in the rules (`Match::laser`) and on the wire (`Combat::laser`), ready for G5 to draw. Hits and headshots are
counted where the original's scorer counts them, with tests. A human out of lives now takes over a **dying**
team-mate bot as the original does. A client refused by a full match is **disconnected with the reason** and sees
it. **STATS rows past P4** get "P5", "P6"... in the HUD's font. The team screen's up/down (the extra team size) is
already covered by `--bots`. A human joining mid-match with no vacant participant stays one of the script's bots
(left; why below). Research: `../lab/research/mp-original/match-flow-g3.md`, with `match-flow-g3-scratch/`
(`xb_run.sh`: the lldb run on a private `ConkerRecomp.g3a`; `m2c.sh`; `code/`).

**The wire changed:** `Combat` gains `laser: Option<[i16; 3]>` (the dot; `None` while off). `PROTOCOL_ID` was left
to the wave's bump. **The shared simulation's step did not change.** `Match::laser` is a query. The headshot count
moved into `hurt` (same counts). `takes_over` lost its health test (the TakeOver effect, which the server applies).

| Part | What |
|---|---|
| conker-rules `game.rs` | **`Match::laser(i, world)`**: on while the holder of 0x18/0x41 aims (`Action::Aim`, clip 0x10A) and all of these hold: the tick is past `since + LASER_BLEND` (5), Z is held, R is held (bots excepted), the scope is up. It points where the scope's line first meets a wall or a body part. **`Fighter::lives(start)`**: the original's lives byte, with a dying fighter's life still in (moved from the server's `combat()`). **`hurt`** counts the headshot of every reaction-0x2E hit with the hit (enemy only); `ray`, the bullet head pop and the bolts no longer count it themselves. **`takes_over`**: no `health > 0` |
| conker-sim `net.rs` | `Combat::laser` |
| conker-server `main.rs` | `publish` fills `Combat::laser` from `Match::laser` (it takes `Res<Collision>`). `combat()`'s `lives_left` is `Fighter::lives`. **`join_player`**'s two refusals (every team full; Race's 2 humans) trigger aeronet's `Disconnect` on the client's session with the reason |
| conker-client `connection.rs` | `why()`: a non-empty `LinkFailed(UnlinkReason::ByPeer(r))` reads as `r` (one arm; its test updated) |
| conker-client `hud.rs` | `results()`: a row past the pack's labels gets `P{n+1}` in the HUD's font, in the row's colour, on the row's text top, scaled with the row |
| conker-bots `brain.rs`, `lib.rs` | `takes_over`: no `health > 0`. `Body::lives` from `Fighter::lives` (the original's byte, which the script's pick reads) |

| Check (2026-10-14) | Result |
|---|---|
| rules replay `combat8_xbl` (new case, crossbow at read 40) | 731 rows, pos 0.0000, **events 9/9, missing 0, extra 0**: 0x10A at 137 and 331, the shots (0x10C) at **302 and 472**, Z let go at 301 and 471 after 161-frame holds |
| rules `the_crossbow_laser_shines_while_z_is_held` (new) | 0x18 and 0x41: shots at 236 and 406 (the releases), 0x10A from 76 and 265, the laser on over 81..236 and 270..406, its dot on player 1's body and on the released bolt's line. Off without the scope; off after a hit |
| rules `hits_count_per_enemy_and_tick` (new) | A cook-off in the hand on 2 enemies: shots/hits/kills/suicides (1, 2, 2, 1). A burn: 2 ticks = 2 hits. An immune victim: 0. A sniper hit on an enemy: 1 hit, 1 headshot; on a team-mate: 0, 0 |
| rules (ROM) `a_plate_sets_off_its_traps` (extended) | The presser has the kill and the hit: (1, 1), (0, 0), (1, 1), (0, 0) |
| rules `a_dying_bot_is_taken_over` (new) | A dying bot with 2 lives (byte 2) is taken over, and the human respawns at the death's end. With its last life (byte 1), the human is out again |
| bots (ROM) `total_war_takeover` | Passes (V t1: out 424, swap 437) |
| client `rows_past_four_are_labelled` (new) | 6 rows: 4 pack labels, then "P5" and "P6" (2 white glyphs each) in rows 5's and 6's bands |
| Live: `bots.sh` `MODE=total-war BOTS=0 HUMANS=5 PORT=5970` (WebSocket) and 5 clients over WebTransport on 5972 | The 5th joiner: server `the match is full: every team has its humans, disconnected`. The client shows `disconnected from 127.0.0.1:597x: the match is full: every team has its humans. Run the client again to reconnect`. The 4 others play on |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | 573 pass, 4 ignored (with the other G3 lanes' work in the tree). Wasm clippy is clean. `check`: my files pass `cargo fmt` and clippy. What fails is in other lanes' files still being edited: fmt in conker-bots `brain.rs`:1367, `tests/rom.rs` and `tests/g3d_probe.rs`, client `lib.rs` and `traps.rs`, rules `tank.rs` and `traps.rs`; clippy's `type_complexity` in server `tls.rs`:148 |

**Findings** (V the runs above; C the code; I inferred; details in `match-flow-g3.md`).
- **The release** (C `func_15065A5C` case 0x18/0x41). While scoped, Z up this frame with Z down last frame
  (s126+0x88) and the cooldown at 0 fire 0x10C, with a 0x3C cooldown. Otherwise the clip stays 0x10A. There is no
  minimum hold (C; V G2's `las`: a one-frame hold fired), and the bone crossbow behaves the same. s126+0x88 is written at the end of `func_1504CB98` on every
  path, so a hit never makes a stale release.
- **The laser** (C `func_15054A94` → `func_151026BC`; ended by `func_151027E8`). It is on while all of these hold:
  the parent is in 0x10A past its blend, the scope is up (for a human, R held), Z is held. It ends on the release, a
  hit, the holster, or the child's removal at a death or a weapon change (`func_15054A5C`). V combat8_xbl: made at
  141 and 335, 5 frames into 0x10A; ended at 301 and 471.
- **The scorer** `func_151E5FF4` (C): a hit counts for a human attacker on another team, and reaction 0x2E adds a
  headshot. Kills count for anyone.
- **The takeover** (C `func_1508E89C`): no health test. The script's pick is the most health + 6·lives > 0, and the
  lives byte drops at the respawn, so a dying bot qualifies.
- **The team screen** (C `func_151E1744`): up/down set the extra team size 0x8008FDA0 (≥ 0). The size is the row's
  most humans per team plus that, clamped by the row's [min, max] over the teams, and the bots fill each team to it.
  Every row's minimum is 0, so `--bots` (the total) reaches each such size: `TeamRow::bots` fills the teams evenly.
  No option was added.

**Left.**
- **A human joining with no vacant participant** (or into a bot's) is one of the script's bots until the next
  match: no Beach 90 s deadline, no Total War traitor check or takeover. The scripts split humans from bots by
  index against a count read once at the start, and that count lives in a per-script global (G+0x1EC8 in War's
  scripts; in Heist's that global is the carrier). Fixing it needs a remap of every per-participant array and
  participant-valued global, or that count mapped per script. Neither fits E1's rule (only the changing slot's
  memory starts over).
- **The laser is not drawn (G5).** The handoff:
  - Use `Combat::laser` for the others, and `Match::laser` on the predicted fighter for one's own.
  - The beam runs from the muzzle to the dot and only the others' views draw it; the dot is drawn by all.
  - Colour bytes: (0xFF, 0) for models 0x5A, 0x5F, 0x74, 0x75, 0x7A, else (0x32, 0xFF).
  - Sound 1232 at 2000, 2D, for the aimer.
  - **Pack:** the dot (I8 64 × 64 at 0x802288F0) and the beam (I8 128 × 32 at 0x80229900) textures (G2's
    findings above).
- **STATS P5+** use the HUD font, not a pack cell. The original has no P5, so there is nothing to extract.

## Net and deploy details (2026-10-14)
**Lane G3b, wave 8's leaving/joining, deployment and native HUD items.** No wire change (`net.rs`, `PROTOCOL_ID`
untouched), no change to the shared simulation. One server rule changed for `--wait` only (the bots wait for the
start).

| Item | Result |
|---|---|
| 1. Ctrl-C skips the disconnect | **Fixed.** SIGINT, SIGTERM and SIGHUP now go through Bevy's Ctrl-C exit on the client (headless and windowed) and the server. ctrlc's `termination` feature adds SIGTERM and SIGHUP; the handler is set with `ctrlc::set_handler` (overwrite), because a script's background job starts with SIGINT ignored and Bevy's own plugin then stands down (it did in the first run: nothing was caught). The client's `leave` already turned an exit into the disconnect. The server's new `shutdown.rs` holds the first exit 0.2 s: it triggers lightyear's `Stop` in `PostUpdate` before the sends, so every client's disconnect goes out in the same frame, before the next frame's receive unlinks the transports. Exit code 130 (Bevy's); the systemd unit has `SuccessExitStatus=130`. A second Ctrl-C exits at once (Bevy) |
| 2. Native WebTransport with a digest is IPv4-only | **Fixed.** `connection::wt` (was `ca`) opens every native WebTransport link itself: it binds the server address's family (`InAddrAnyV4`, `InAddrAnyV6`, dual-stack for a host name) and checks the certificate against `ca=`, else the digest, else (new) the system's roots. Lightyear's `WebTransportClientIo` is now the browser's only |
| 3. Token fetch without a connect timeout | **Fixed.** `FETCH_SECS` = 4 to connect (each resolved address), then 4 for the answer; a timeout reads `could not get a connect token from URL: no answer in 4 s`. The native fetch now runs on a thread (one `Fetching` for both targets), so the window no longer freezes; the fetch counts as connecting (the HUD says `connecting to ... over ...`, and the 10 s watchdog bounds fetch and handshake together). `server_info`'s retries stop after 5 s in all |
| 4. Tokens that survive a slow load | **Already so** since the asset-pack wave: both targets fetch the token in `connection::connect`, which runs only once the level and the map's characters have loaded and 10 calm frames passed (`ready`), right before the connect; `web/play.js` and `web/app` fetch none. No change; no refetch on expiry (a connect attempt lasts 10 s at most, the token 60 s) |
| 5. Reload a renewed certificate | **Fixed.** `tls.rs`: every handshake (WebTransport and `wss://`) takes the certificate from one rustls resolver (`tls::Certificate`); with `--cert`, `tls::watch` looks at the two files every 10 s and swaps in a pair that loads and matches (`CertifiedKey::from_der` checks the key), else keeps the one in use and tries again (a renewal writes one file, then the other). Open sessions keep theirs. The WebTransport server is opened by `tls::open_webtransport` (lightyear's `WebTransportServerIo` takes a fixed identity), as lightyear does; its own observers still accept the sessions, and `relink_evicted_session` still covers the second transport. Both TLS configs use ring explicitly (rustls has aws-lc-rs compiled in too). `deploy/renew-hook.sh` no longer restarts the game server (it starts it, a no-op once it runs, for the first issue) |
| 6. Rate-limit `/token` | **Fixed** (`ponytail:`). 10 tokens per 10 s per address (then `429`), counted in a map pruned on each request: the peer's address, or for a loopback peer the last `X-Forwarded-For` entry (Caddy's); a loopback peer without one (dev, this machine's clients) and `/info` are not limited. Still one request at a time with the 2 s read limit. An IPv6 client is counted per /128 |
| 7. Native HUD wording | **Fixed.** `NetStatus::text` keeps the page's words in the browser and says natively: `Run the client again to reconnect`, `--transport ws tries WebSocket`, `pass --digest HEX, the server's --digest-file`, `A restarted server has a new certificate: pass its --digest`. The native ellipsis is `...`: Bevy's default font (FiraMono subset) has the 95 ASCII glyphs only, no U+2026. anyhow's numbered causes (`0: A 1: B`, the WebSocket reset) read as `A: B` |
| 8. `--wait`'s rollback and the bots before the start | **Bots fixed; the rollback shown harmless.** The bots: the match steps from the server's own start, and the bots' AI with it, so they roamed, took weapons and fought the humans waiting for the start (tick 120 of the run below: bot 3 fighting slot 0, bot 1 holding 0x39). Now their AI waits for a `--wait` start (`idle`, next to Race's `warm_up`; the match still steps, the humans move). The rollback: at the start tick the server puts every fighter back on its start place (`Fighter::new`, a new `Match`); the client's prediction has no such step, so the first confirmed state of that tick differs and it rolls back its lead (3 ticks locally, 13-14 at `lag=150`). It is the reset itself reaching the client, once per match start, only in `--wait` replays and tests; a player that stood still (script files) moves nothing, one that walked (the circle pattern, a page) goes back to its start place as the server meant. Predicting it would need the slot's start place (not replicated) and a copy of the start block in the client's prediction (`lib.rs`, the prediction lane) |

### Tests (unit)
- conker-client 140 pass: `statuses_read_with_their_reasons_and_hints` (native words, ASCII, `connecting to ...`),
  `a_silent_endpoint_times_out_and_sockets_follow_the_family` (a listener that never answers: `no answer in 4 s`
  within 6 s; V4, V6 and dual binds; the hex reader), `netcode_states_read_as_words` (+ the numbered causes).
- conker-server 30 pass: `tokens_are_limited_per_address` (over a socket: 10 then 429 for one forwarded address,
  `/info` still 200, another address 200, the loopback without `X-Forwarded-For` unlimited; the window's end),
  `pairs_load_checked_and_swap` (a mismatched pair refused with `KeyMismatch`; the resolver swaps).

### Runs (ports 5974 and 5976)
- `crates/conker-server/sigint.sh` (new; four headless clients):

  | Signal | Seen | Exit |
  |---|---|---|
  | SIGINT to client 1 (WebTransport) | the server logs `client Netcode(1) left: Disconnected by peer` 0.07 s later | 0.18 s |
  | SIGTERM to client 2 (WebTransport) | `left` 0.06 s later | 0.20 s |
  | SIGTERM to the server | client 3 (WebTransport): `the server closed the connection` 0.04 s later; client 4 (WebSocket) 0.01 s | 0.21 s |

  Before the fix (first run, same script): nothing caught; the remaining WebTransport client saw `nothing from the
  server for 3 s`.
- `[::1]`, `127.0.0.1` and `localhost` over WebTransport with a digest: all three connected.
- A native WebTransport client with neither digest nor `ca=` against the dev server: refused at once, `invalid
  peer certificate: UnknownIssuer`, with the native hint (it checked against lightyear's empty hash list before).
- `smoke.sh wt ws ws` and `wt ws wt` (5974): 16/16 both; prediction max error 0, 0 state misses; rollbacks 0, and
  once one 2-tick rollback (a pickup).
- `../lab/deploy-test/test-reload.sh` (new, 5976; the test CA, plus a throwaway localhost certificate from the
  lab's other CA): 8/8. Client A connected; the new certificate alone (old key) was refused by the server's check
  (`KeyMismatch`) and the one in use stayed (a client trusting the first CA still connected); with the key the
  server logged `certificate reloaded ... digest 22D6...`; clients trusting the other CA connected over
  WebTransport and `wss://`, one trusting the first was refused (`UnknownIssuer`); client A played on until its
  own exit (1 leave, 1 rollback of 3 ticks).
- `--wait 1 --bots 3`, one headless client on the circle pattern: before, the bots moved from tick 30 and bot 3
  fought slot 0 at tick 120; after, the bots stood at their places until the start (tick 193), then moved. Both
  runs: one rollback at the start tick (`confirmed_tick` = the server's `match started at tick`), 3 ticks.
- `combat.sh` e_w09 (5974; `--wait 2`, no bots): 39 of 43 oracle events, clock 742/742, lag compensation rewound vs
  shown max 0: as recorded before.
- Native windowed client, a minted token and a digest, nothing on the port: the HUD reads `connecting to
  127.0.0.1:5974 over WebTransport...` (`../lab/screens/net-deploy/native-hud-connecting.png`).

### Changed
- conker-server: new `shutdown.rs`, `sigint.sh`; `tls.rs` rewritten (`Certificate`, `load`, `self_signed`,
  `digest`, `configs`, `watch`, `WebTransportTls`, `open_webtransport`); `token.rs` (`PER_WINDOW`, `WINDOW`,
  `Limit`); `main.rs`: the transport setup, `mod shutdown`, the bots' `idle` gate. Dependencies: `aeronet_webtransport
  =0.21.0` (lightyear's, already in the tree) and `ctrlc` with `termination` (bevy_app's, already in the tree)
  direct; `tokio` no longer direct (PEM files load with rustls' reader).
- conker-client: `connection.rs` (`FETCH_SECS`, `page`, `wt` for `ca`, one `Fetching`, `hex`, the connecting
  status from the token fetch on); `ctrlc` with `termination` (native).
- deploy: `renew-hook.sh` (no restart of the game server), `conker-server.service` (`SuccessExitStatus=130`),
  `README.md` (step 8's renewal, the token limit, graceful stops, the local reload test).
- The landing's Kill (SIGTERM, then SIGKILL after 3 s) now gets the server's clean 0.2 s stop.

### Left
- A real host: the reload under certbot and systemd (`install` into `/etc/conker/tls`), and the native
  system-roots WebTransport path against a real certificate.
- Predicting the `--wait` start's reset on the client (prediction lane).
- With `--cert`, `--digest-file` is written once at the start, not again on a reload (only a dev page pinning a
  `--cert` server's digest would notice; https pages pin nothing).
- `/token`: one request at a time (a slow client holds it up to 2 s); per-/64 counting for IPv6; tokens are free
  to anyone who reaches the page anyway.
- The 10 s connect watchdog is still untested in a browser; the browser's own words are unchanged and were not
  re-run (no browser here).

## Held weapons: the crossbow and item 0x3A in the client (2026-10-14)
**Batch L2b, wave G2, the client lane. The crossbow and the bone crossbow are drawn from their own rig and posed by
the holder's clip: the bind pose in the draw, the fire clip at the holder's clock, the holster clip, else the one-tick
pose. The loaded bolt shows on fire ticks 6-10, and the holster's op 41 hides the crossbow at t14. Item 0x3A's grenade
(0x1A, 0x3F) is in the hand from the pull's first frame to the throw's first, posed by the frames since the pull at
its own scale 1.0. In flight it shows its rig's clip 0 (the bent fuse). Its fuse loops from the pull, and it bounces
and blasts with its own cues. The HUD shows its bomb icon. A bolt that hits a wall is silent, as in the original. Class
clips now sound as the class plays them (0x175/0x16C in the pack). The aim laser was traced on the oracle but is not
drawn (Left).** Research: `../lab/research/mp-original/held-client-scratch/`: `run.sh` (plain runs, DL dumps; `CHARS`
added), `snd.sh` (lldb sound calls, `EXTRA` breaks), on the private `ConkerRecomp.g2a`; `tools/dlshot.py` (a
flat-shaded DL render, the child objects in orange), `xbow.py`, `lobjoints.py`, `lobmtx.py`, `clipsample.py`;
`code/func_1506BF5C.c`; `scripts/` (`cb.txt`, `c3.txt`, `laser.txt`); `out/` (the sound runs' calls). Runs: cbs (the
crossbow, DLs 100-233), l1a, l3f (item 0x3A, DLs 126-220), xsnd, xsnd2 (a bolt on a wall), remapA, remapB (the model
remap), las2, lasd (the laser, sound calls and DLs 258, 270, 300). Screens: `../lab/screens/held-client/` (`shots.sh`,
ports 5970-5974).

**The wire did not change** (`PROTOCOL_ID` stays 35), **nor the shared simulation.** The client draws from state
that is already replicated: the clip and clock, the weapon, the cook timer, and the shots. **The pack changed**
(re-exported, "pack equal"). The rigs gain clip 0, `hud.json` gains `grenades.lob_at`, and the clip sounds gain
0x16C/0x175.

| Part | What |
|---|---|
| conker-client `held.rs` | **`Rigged`** (a child object from its own rig: the list it shows, its animator with clips 0-3, each joint's rest, the loaded bolt, and a strong `Handle<Gltf>`: without one the clips unloaded and no rig animated), `rig_ready`, `pose_rig` (every joint at rest, then the clip's keys). **`crossbow_pose`** (`func_15054A94`; held-leftovers §1): draw (0xFB, 0x2C8) → clip 0; fire (0xFC, 0x2C9) → clip 2 at the clock, to 30; holster 0xC6 → clip 3, to 35; else clip 1 at 0. **`loaded_bolt`**: the fire clock in [6, 11), `projectiles.gltf`'s bolt under the rig's `joint_02`. The holster's removal at t14 is `held.json`'s (0xC6, 14) through `shows`. **Item 0x3A**: `Lob` (the map's list: 0 on 0x1A, 1 on 0x3F, `None` elsewhere); `hang` hangs this map's model only; `show` keys it on 0x1F9/0x1FA (the pull and hold, no events). **`lob_pose`** (clip 2 at 0-22, then clip 1 looping on 30) by **`since_pull`** (from the cook timer: the local player's predicted, the others' replicated). **`unscale_lobs`**: its root's scale undoes the player's and the clip's, so it is drawn at 1.0 |
| conker-client `projectiles.rs` | **`Shape`** (`Scene(i)` or `Lob(k)`), `Look::lift`, **`lob(k)`**: the rig's clip 0, scale 1.0, 28 above the object, upright, +z along the throw, never turning (`pose_lobs`). `look(k, weapon, lob)` |
| conker-client `hud.rs` | `Shown::lob`: the grenades row shows `lob_icon` from `lob_at` (247, 6) by `step` while 0x1F9/0x1FA plays |
| conker-client `audio.rs` | **Item 0x3A** (`Pack::lob`, from `MatchArena`): `On::Lit { weapon: 0x3A }` (0x96) starts at the pull's first frame, at the holder. The grenade takes it over at its birth, so its first bounce stops it, as the original's one sound at a time. It stops if the pull ends without a throw. Bounces and blast use `LOB`'s cues; the in-hand blast covers 0x1F9/0x1FA. **The bolt on a wall** plays nothing; the knife's impact stays. **Class clips**: a player's clip events are looked up under `characters::clip(class, id)` (`Seen::class`) |
| conker-extract `lib.rs` (pack) | `rig()`: an empty pair (clip 0) gets one frame at its descriptor's channel bases. The crossbow's are its rest; item 0x3A's turn joints 1-3 by −12.5°, −24.1°, −26.4° about z (the bent fuse) |
| conker-extract `hud.rs`, conker-assets `hud.rs` (pack) | `Grenades::lob_at` [247, 6] |
| conker-extract `audio.rs` (pack) | `MATCH_CLIPS` gains 0x16C and 0x175 (everyone but the Tediz pulls and throws with them) |
| conker-anim `lib.rs` | `combat::BONE_CROSSBOW_DRAW` 0x2C8, `BONE_CROSSBOW_FIRE` 0x2C9 |
| conker-client `render.rs` | Passes the map's `Lob` list to `held::build` |
| conker-assets `lib.rs`, `pack-readme.md` | Docs: the rigs' clip 0, the thrown grenade's pose, `lob_at` |

| Check (2026-10-14) | Result |
|---|---|
| client `the_crossbow_follows_its_holder` (new) | Both crossbows: draw → (0, 0); fire t9 → (2, 9), t30.5 → (2, 30); holster t7 → (3, 7), t35.9 → (3, 35); the aim, the aimed shot, the idle and the run → (1, 0). The loaded bolt on clocks 6-10, not 11, not in 0x10C |
| client `the_lobbed_grenade_follows_its_pull` (new) | p1a's reads 102 → (2, 0), 124 → (2, 22), 125 → (1, 0), 131 → (1, 6), 155 → (1, 0); `since_pull` 0 and 23 frames |
| client `projectiles_turn_as_the_original`, `layout_matches_the_originals_draws` (extended) | The lob's shape, scale and lift; upright with +z along l1a's heading in flight and at rest. The bombs at x 247, 236, 225, y 6, 11 × 14 |
| client `the_lobs_fuse_follows_the_grenade` (new, map 1a) | 0x96 at the pull; the grenade born at tick 110 takes it; 0x186-0x188 at 111, the blast (1549) at 112; the fuse gone after the bounce; a pull left without a throw stops it |
| client `grenade_clips_sound_as_each_class_plays_them` (new) | A squirrel's pull plays 1548 at t25 and its throw one of 889/891/1420. A Tedi's pull plays nothing and its throw one of 1235/1237 |
| client `a_bolt_passes_once` (extended) | Gone at tick 115: the bolt plays only its pass (109); the knife's last sound is its impact (176, 1113 or 1114) |
| extract (ROM) `child_object_rigs` (extended) | Both rigs name clips 0-3. Clip 0 is one frame: the crossbow's at rest; the lob's joints 1-3 at (0, 0, 63264), (0, 0, 61152), (0, 0, 60736) |
| Screens: the crossbow | `crossbow-fire-t8.png`, `-t9.png` (fire clock 8 and 9 from cbs's camera); `crossbow-fire-vs-dl147.png` (cbs DL 147 \| t8 \| t9: the string and the nock as the oracle's); `crossbow-bolt-window.png` (reads 854, 855, 859, 860: the bolt on 855 and 859 only) |
| Screens: item 0x3A | `lob-1a-pull.png` (read 775), `lob-1a-hold.png` (787), `lob-1a-flight-side.png` (812), `lob-3f-787.png` (l3f's camera), `lob-hold-vs-dl126.png` (l3f DL 126 \| ours: the hand, the size, the fuse up) |
| Pack | `bun run pack --force`, then `lab/tools/verify-pack.sh`: pack equal (2103 files) |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | 563 pass, 1 ignored (with the body-part and bots lanes' work in the tree); `check` passes (fmt, clippy on every package, `web/app`); wasm clippy passes |

**Findings** (V the runs above; C the code; I inferred).
- **The lob's frame** (V l3f DL 126, l1a). The grenade's root is the clip's root (pull and hold: translation (26.7,
  −18.4, 4.6) and its turn) in the hand's frame (`joint_09`) normalised. The glTF's `joint_09` relative to `joint_00`
  matches the oracle's. The pull's clip 2 grows the root from 0.06, so the grenade appears small. l1a's DL 133 was
  already past the throw (read 130).
- **The thrown lob** (V l1a DLs 133-220, l3f DLs 140-200): clip 0 (the bent fuse), joint 0 at 1.0, 28 above the
  object, upright with +z on the throw's heading. It never turns, even at rest.
- **The oracle draws the crossbow a frame behind its clock** (V cbs). DL 147 holds the fire clock 9 but draws the
  t8 pose (I: the drawn matrices are the last update's). We draw each clock's own pose, as the recipe says. In our run
  (Z at read 850, clock = read − 849) the loaded bolt shows on reads 855-859, the clocks 6-10.
- **A bolt on a wall is silent** (V xsnd2: calls 160, 272; xsnd). `func_151D3480` (the wall test) leads to
  `func_15060F28` (the removal), then `func_10010AA8(bolt, 4)` stops the bolt's sounds. No sound starts.
- **The model remap** (C `func_1506BF5C`; V remapA, remapB). The compared model is rec = *(0x800D1C90 + 4·model). If
  rec[4] ≠ 0, it is *(u8*)*(0x80086CAC + 4·rec[5]). Only models 0-4 remap (to 0), and 150 stays 150. The pack's
  per-model filter therefore already gives the original's sounds on every pick (Conker is folder 0). What differed
  was the clip id: every class but the Tediz plays 0x175/0x16C (`func_15065A5C`), so those clips' events were missing.
  0x176 has no sound events.
- **The aim laser** (C `func_151026BC` → `func_15100600`, effect type 3; V las2, lasd on 0x18, `laser.txt`):
  - **Its life.** Pressing Z while aiming (0x10A) makes it. Releasing Z fires the aimed shot (0x10C, sound 1091) and
    ends it. While it is on, the aimer hears a 2D sound 1232 at 2000; `func_15101090` stops that handle.
  - **The dot.** A 4-vertex quad of about 15 units on the hit surface: I8 64 × 64 at 0x802288F0, clamped. Both
    players' views draw it.
  - **The beam.** A 4-vertex quad about 27 wide from the muzzle to the dot (about 1190 long): I8 128 × 32 at
    0x80229900, wrapped in s (s −2979 to −31738). Only the other players' views draw it.
  - **The look.** Combiner 0xFF9BFF/0xFF36FF7F, env (FF, 00, 00, C2): the Tedi (0x75) is red, as the brief's colour
    rule says.
- **The thrown lob appears a few frames late** (I). It is drawn from the server's `MatchView`, behind the predicted
  throw. A server hiccup (one 213-unit jump seen) can exceed `fly`'s 60-a-frame reach, and the flight respawns.

**Left.**
- **The aim laser is not drawn.** In the original it lives between the Z press and the Z release. The rules fire the
  aimed crossbow on the Z press (`game.rs`, not this lane), so it would never show. Three things are needed: (1) the
  rules' release-fire; (2) one bit on the wire for "Z held while aiming" (it isn't there; not added, being moot until
  (1)); (3) the dot and beam textures in the pack (not extracted). Sound 1232 isn't played either.
- **The lob's fuse sparks** (white quads on the oracle) are not drawn.
- **The crossbow's frame lag**: we draw one frame ahead of the oracle (above). Matching it is `crossbow_pose(anim,
  clock − 1)`.

## Body parts: the hit test (2026-10-14)
**Batch L2c, wave G2. The server now tests the twin guns' and the pistol's bullets, the sniper's ray, aimed shots,
bolts and knives against each player's body-part spheres, as the original does (`func_1508114C`). The spheres sit on
the joints of the pose the last frame drew, posed from the pack's glTF. The pistol now has a head: on pc3's ray, a
crouched Tedi's head (part 0xA) is 177.72 along (the oracle's 177.7), and the hit takes 6 → 3. A standing Tedi's neck is
now missed, where the cylinder hit it. e_w39's 4 pass-through gaps close (bullet rows 19/19, 15 before). e_w09 matches
119 of 122 rows (122 with the cylinder): the last bullet meets the hit reaction's flung hand. The 0x23, tommy gun and
mounted gun bullets keep the cylinder. Without a table (client prediction, replays without a pack) the rules run as
before. Deaths a match stay near the oracle's. In the server's bots matches they rise to 15 on 2D (oracle 13) and 13 on
0x34 (oracle about 16). In `bot_match` they fall to 10.6 on 2D and hold at 12.8 on 0x34.**
Research: no new oracle runs. Evidence comes from the traced replays (`lab/oracle/trajectories`): obj+0x84 and +0xB4
per row, and the `_lldb` bullet calls (`func_151C2734`'s position each frame).

**The wire did not change** (`PROTOCOL_ID` stays 35): `Match::parts` and `Bullet::parts` are not sent. **The shared
simulation changed only behind the table.** The client never fills `Match::parts`. With it empty, `first_part` and
`fly_bullets` take the old paths byte for byte. The default replays' numbers are unchanged.

| Part | What |
|---|---|
| conker-assets `pose.rs` (new) | **`Skeleton`**: `parse` reads the joints (`joint_NN`), rest TRS and every clip's channels from a character's glTF and bin. `posed_by` poses a mesh-only pick on another folder's clips, its translations shifted by the rest difference. `sample` is linear with shortest-arc slerp, held past the ends. `model` composes joint → model transforms. `blend` is also new. `parse_json` is public |
| conker-rules `parts.rs` (new) | **`Poses`**: `models(scene, cheats)` (the sides', the scene's own, the allowed picks); `load` (models without `parts` get none). **`tick`** drives each slot's clock as the client's `tick_anims` does for a remote player. Two rules differ: a clip a hit started stays at time 0 for that frame, and it fades in fresh from the pose showing. **`place`** puts the spheres at pos + Ry(facing + 90°) · scale · (joint · offset + (0, lift, 0)), radius × scale. It also applies the twist on `joint_01` (keeping the head), pins the creep root, cross-fades by the clock, and hides 0xF000 on a popped zombie. None in Race and Tank |
| conker-rules `projectile.rs` | **`ray_sphere`** (`func_151452C4`). **`BodyParts::hit`** (`func_1508114C`): the broad sphere first (a miss is no hit). Then the table's order, skipping hidden joints: a sphere replaces the best if it is the first, or nearer with part ≥ the best's. **`Bullet::parts`** (flag 8). `HEAD` = 0xA |
| conker-rules `game.rs` | **`Match::parts`** (per slot; `None`: a model without parts, the cylinder, no head). **`first_posed`**: the parts test; corpses lying still (`DEAD`) let everything through. Under lag compensation the spheres move by where the shooter saw the victim, less where they were placed (`BodyParts::origin`). `first_part` uses it when there is a table. `fly_bullets`: 0x09's and 0x39's bullets (`parts`) take it with the head on part 0xA. The pistol's ponytail exclusion is gone |
| conker-server `main.rs` | `Game::poses`, loaded at start from the pack. `place` runs before each step, `tick` after it on the step's events. A slot without a clock yet (a joiner's first tick) gets `None` for that tick |
| conker-bots `examples/bot_match.rs` | Fills the table as the server does (from the pack; `BOT_PARTS=0`: the cylinder). The ROM tests keep the cylinder |
| conker-rules `tests/combat.rs` | `COMBAT_POSES=last\|this`: the replays with the poses (the clocks seeded from obj+0x84 and +0xB4); `COMBAT_ALL` prints every case and asserts none |

| Check (2026-10-14) | Result |
|---|---|
| assets `the_tedis_head_crouched_and_standing` (new) | 117's clip 0x16 at tick 16: the head sphere 43.7 above the feet; 0xF at 22: 70.0; Conker on 117's clips |
| assets `slerp_takes_the_short_way_and_blend_mixes`; rules `ray_sphere_as_the_game`, `parts_keep_the_table_order` (new) | Pass |
| rules `posed_bullets_hit_the_parts` (new) | A hand-made table. Pistol on the head: 6 → 3; on a body sphere: 4; between the spheres (broad sphere only): no hit. Tommy gun (no flag): the cylinder. A `None` slot: the cylinder |
| rules `the_pistol_meets_the_crouched_head_and_passes_the_standing_neck` (new, the pack) | pc3's ray. Crouched: head 177.72 along, 6 → 3. Standing: no hit, 6. Without a table: the cylinder, 4 |
| rules `combat_traces` (the cylinder; asserts unchanged) | Pass |
| `bun run test` | 562 pass, 1 ignored |
| `bun run check`; wasm clippy (`conker-client`) | Clippy passes on every package but conker-client. Fails only in other lanes' in-progress files. `cargo fmt --check`: conker-client `audio.rs`, `held.rs`, `hud.rs`, `projectiles.rs`; conker-extract `audio.rs`, `tests/rom.rs`. Clippy (native and wasm): `projectiles.rs:184` `fly`, too many arguments. `web/app` check passes |

| Replay (rows as the oracle's) | Cylinder | Poses, last frame's | Poses, this frame's |
|---|---|---|---|
| e_w39 bullets | 15/19 | 19/19 | 19/19 |
| e_w09 bullets | 122/122 | 119/122 | 86/122 |
| combat4_pitch24 events | 25/25 | 25/25 | 25/25 (19/25 before the hit-clip rule) |
| combat4_w14 P2's position (max error) | 0.010 | 0.128 from 850 | 0.140 from 849 |
| The other 52 cases | | as the cylinder | as the cylinder |

| Server run (`bots.sh BOTS=3 TIME=5`, 1 idle human; ports 5976, 5978) | Before (cylinder) | After (parts) | Oracle |
|---|---|---|---|
| 2D, seeds 1 and 2: deaths | 12, 9 (hits 32, 51) | 16, 14 (hits 62, 61) | 13 (`bot_long3`, 5 minutes) |
| 0x34, seeds 1 and 2: deaths | 3, 10 (hits 7, 33) | 13, 13 (hits 31, 56) | 8 in 4,403 reads (q3s4b3); 10 and 9 in about 5,200 (o34b3s0, s1) |
| Faults, panics | none | none; the known FAIL of a bot that never died (0x34 seed 2; before: seeds 1 and 2) | |

| `bot_match` (deterministic; `BOT_PARTS=0` / the pack) | Cylinder | Parts | Oracle |
|---|---|---|---|
| 2D, 3 bots, 9,000 frames, seeds 1-8: deaths | 13.75 (11-18) | 10.6 (4-14) | 13 |
| 0x34, 3 bots, 9,000 frames, seeds 1-8: deaths | 12.0 (7-17) | 12.75 (6-18) | about 16 (above) |
| 0x30, 3 bots, 4,496 frames, seeds 1-8: deaths / twin-gun shots, hp drops, ratio (`hits.py`) | 4.9 / 396, 11.0, 0.028 | 5.25 / 405, 13.0, 0.032 | 6.0 / 281, 20.4, 0.073 |

**Findings** (V the replays; C the code; I inferred).
- **The last frame's pose** (V). Testing this frame's pose (the step run first on a copy) loses 33 more e_w09 rows. It
  also lost 6 of pitch24's events before the hit-clip rule.
- **A hit's clip starts a frame late** (V). The hit lands after the victim's own update. In 43 of 46 traced 0x11D
  starts, the first row shows time 0, while a clip the player's own update starts shows its rate (0x7C: 1.0). Advanced
  at once, the reaction's flung hand (joint 9, r 5.5, about 38 lateral at +37) stopped 6 of e_w09's right-gun bullets.
  Lethal hits too: e_w09's and e_w39's kills (0x11D) and pitch24's (0x17E) start at 0. Without them, e_w09 loses a row.
  Only combat4_w14's aimed kill shows 0x17E at 1.0.
- **The hit's fade starts fresh** (I, V e_w09). Three hits came while the last reaction was still fading to idle.
  With conker-anim's "back to the clip we were fading from", that fade took 1023 − weight and stopped 3 more bullets.
- **Corpses lying still** (V e_w09, 2 bullets): `DEAD` (0x59) lets bullets through, as before. A falling corpse is
  still tested (V e_w39: its head pops).
- **Seeded clocks**: a replay clock seeded mid-clip had no length (`play` sets it). `tick` now sets it every frame,
  which made combat4_w14's idle loop at 52 as the game's.
- **Item 5**: `game.rs` already names `func_150FED30` the pistol's and `func_150FE860` the tommy gun's (I). The
  research note `bots-fidelity.md` (E2c, lines 320-326) is stale: it gives `func_150FE860` to the pistol and says the
  pistol's flag is 0. pc3 (6 → 3 on the head) shows the pistol's bullets test parts.

**Left.**
- **e_w09's last bullet** (3 rows). Our hit reaction is at t2 with the idle at 0.51 when the right-gun bullet's second
  segment starts 3.5 from the hand. The game's pose mixer is unread (conker-anim: weight / 1023 linear, blend 4, I).
- **Aimed shots meet the idle head nearer**: combat4_w14 lands 132.0 ahead, the game 134.2 (the cylinder's fitted
  sphere also gives 134.2). The ray grazes the r 11 head sphere, so the push angle is 85 off and P2 drifts 0.13 over
  its fall.
- **Lag compensation moves the spheres** to where the shooter saw the victim; it does not rewind the pose.
- **ohit** backs `a_bullet_on_the_head_hurts_through_the_stun`, which runs the cylinder path. Its 12 head hits were
  not replayed with the parts.
- **The sniper's hits on 0x30 fell** (bot_match, 5.6 → 3.0 a run) with the parts. The oracle's count was not split.
- **The client draws a hit's clip a frame early** and its blend comes back (the client's lane).
- **Hits before the victim's update**: an earlier slot's strike (V w14's aimed kill, 0x17E at 1.0; melee: 0x10F 28 at
  0, 10 not) may land before the victim's own update. `tick` treats every hit as landing after it.
- **The server's `LAGCOMP` log** (`tested_bullets`, `first_hit`) still reports the cylinder.

## Deathmatch bots: the leftovers (2026-10-14)
**Batch G2, the bots' lane. 0x34's bots flee a burning row of fire jets as the original's: the game makes a noise of
radius 500 at each row's middle jet while it burns (`func_151A6600`, V), which the remake never heard, so its bots
walked on into the flames. Deaths in a jet over the oracle's minute with 15 bots went from 5.8 a run to 1.0 (oracle
0-4, mean 1.5). The ROM harness and `bot_match` now seat the mounted guns as the server does. With them and 5 oracle
seeds, 0x30's "4.1 deaths against 9" is 4.9 against 6.0: the 9 was one seed (o30s0, also the 50 hp drops). The bots
still hold the sniper rifle and 0x23 two or three times as long as the original's (Left). A 0x34 bot on the mounted
gun stays there until it dies or the match ends, in both games. The Vault's pickups take a new spot at their return,
not at the take (V); the remake already did. The crossbow's pitch and the sniper's hit rate are not done (Left).**
Research: `../lab/research/mp-original/dm-bots-leftovers-scratch/`: `tools/` (`run.sh`, `lldb.sh`, `t34.py` the traps,
`ez.py` the electric floor's box, `hits.py` shots and hits per weapon, `held.py` held and drawn, `gun.py` gun spells,
`roam.py` the Vault's pickups, `o30.py`, `roles.py`, `goal.py`, `bystate.py`, `spells.py`, `takes.py`), `out/`. Runs on
the private `ConkerRecomp.g2c` (`CONKER_FIXED_DT=1`, an idle human; other seeds by poking the RNG's low word
0x800885B4 at read 22):
- o30s0-s4: 0x30 (scenario 5), 3 bots, 4,519 reads, s126 p1-p3 and G each read. s0 is unpoked (the old o30).
- t34s0-s4: 0x34 (scenario 4), 15 bots, 60 s, with props 6-9's +0x70 words (the plates). s0 is unpoked (`s4_h1b15`);
  s1 stopped at 470 reads (unused).
- o34b3s0, o34b3s1, o34b5s0: 0x34 with 3 and 5 bots, 4,400 reads, s126 and G.
- noise34 (lldb, `combat_lldb.py`): the human poked onto plate 0 at read 80; every `func_1508B20C` call (all from
  `func_151A6600`): radius 500 at the middle jets of plate 0's three rows (x −2587, −1528, 1586), each read 83-203.
- v1a3, v1a7: 0x1A Deathmatch (scenario 6), 3 and 7 bots, with G+0xF70..+0x1170 (holders, timers, spots).

**The wire did not change** (`PROTOCOL_ID` untouched), **nor the shared simulation**: the jets' noise is the bots'
observation only (server-side AI input).

| Part | What |
|---|---|
| conker-bots `lib.rs` | **`jets`**: per trap of 0x34, the frames its emitter is heard on (`Bots::emit`). A row's middle jet (trigger command 1, not the electric floor) is a noise of radius 500 at its zone's centre from the frame its trap is set (`left` = `FIRE_FRAMES` − 1, the bottoming's next) for 121 frames, as `func_151A6600` adds it (C; V noise34). `observe` appends them to the frame's noises, so `func_1508B3F8` sends the bots within 500 to the walkway node outside. The `noises` doc sat on `const ARRIVE`; it is on `fn noises` again |
| conker-bots `tests/rom.rs` | `play()` seats `gun::spawn(scene)` (the ROM tests had no mounted guns). `bots_set_off_the_0x34_traps` counts deaths in a trap that hits (`Traps::hits`, not `left > 0`) per kind, and asserts at most 4 in a jet (the oracle's most) |
| conker-bots `examples/bot_match.rs` | The mounted guns and 0x34's traps (seated each frame) as the server has them. The csv adds `role, drawn, gdc, enemy, attacking, shots`; the summary adds the plates' presses, the deaths and the living's frames in each kind of trap, on and at all |
| conker-rules `weapon.rs`, `game.rs` | Docs only: `Roaming` and `vault_pickups_roam` cite v1a3 and v1a7 |

| Check (2026-10-14) | Result |
|---|---|
| bots `the_jets_are_heard_while_they_burn` (new) | Plates 0 and 3 stood from frame 80: jet 1 burns 83-202 and is heard 83-203 at (−2587, −379, 3247), radius 500; the side jets and the electric floor never |
| bots ROM `bots_set_off_the_0x34_traps` (seed 1) | 23 presses, deaths [0 in a jet, 1 on the electric floor]; without the noise [9, 3], which fails |
| bots ROM (all, with the guns in `play()`) | 26 pass |
| `bots.sh` (debug, ports 5980-5988, 150 s; the other lanes' work in the tree) | **0x34** `BOTS=7` seeds 1, 2: 23/24 ok each, 15 and 13 deaths, 18 and 15 plate presses; bot 1 (a plate-0 presser) never dies, the one FAIL line. The same tree with the noise off: 24 and 16 deaths, 26 and 18 presses; seed 1 has the same FAIL, seed 2 none (the histories part at the first press). **0x30** `BOTS=3` seeds 1, 2: 11/12 ok each; one bot never dies (seed 1) or never hits (seed 2), as in earlier runs (the noise is empty off 0x34). **0x1A** `BOTS=3` seed 1: 12/12. No fault, unknown native or panic |
| `bot_match` 0x34, 3 bots, 4,403 frames, 8 seeds | deaths 7.1 (5-9), hp drops 21.0; oracle o34b3s0-s1: 8 and 9 deaths |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`) | `test` passes (562, 1 ignored). `check` fails on the other lanes' work in progress: `cargo fmt` in their files (client, server, extract, hit test, as they change) and clippy's `too_many_arguments` on conker-client `projectiles.rs` `fly`, wasm clippy too (`web/app`'s check not reached). This lane's files are clean (clippy `-p conker-bots --all-targets` passes) |

The numbers below are `bot_match` runs of this tree. Its `Match::parts` is empty, so the hit test there is the
fitted cylinder and axis spheres, with the body-part work (G2b) in progress in `game.rs` and `projectile.rs`.

| 0x34, 15 bots, 1,737 frames (means; ranges) | Before (no noise) | After | Oracle (t34 s0, s2-s4) |
|---|---|---|---|
| plate presses | 21.7 (14-26) | 24.7 (18-34) | 20 (14-33) |
| deaths in a jet | 5.8 (2-10) | 1.0 (0-2) | 1.5 (0-4) |
| deaths on the electric floor | 1.2 (0-2) | 1.3 (0-3) | 0.25 (0-1) |
| the living's frames in a burning jet (the oracle's 31 at 0 deaths: `t34.py`'s window or the kill rule, untraced) | (not split) | 1 (0-2) | 16 (0-31) |
| the living's frames on the live electric floor | (not split) | 139 (48-270) | 15 (0-58) |
| the living's frames in the zones, on or off: jets / electric | (not split) | 129 / 944 | 209 / 259 |

| 0x30, 3 bots, 4,496 frames | Remake, 8 seeds | Oracle o30s0-s4 |
|---|---|---|
| deaths / hp drops a run | 4.9 (3-8) / 21.8 (12-28) | 6.0 (4-9) / 28 (15-50) |
| held / drawn frames a run: twin guns 0x09 | 3,790 / 1,922 | 4,981 / 1,824 |
| sniper 0x24 | 3,612 / 2,650 | 1,723 / 837 |
| 0x23 | 1,822 / 692 | 800 / 63 |
| mounted gun (drawn) | 395 | 1,388 |
| twin-gun shots / hp drops given to the shooter / ratio | 397 / 11.0 / 0.028 | 281 / 20.4 / 0.073 |
| mounted-gun spells | 2 in 8 runs, both to the end | 4 in 5 runs: 2 to the end (1,628, 2,953 frames), 2 ended by the gunner's death (155, 2,205) |

**Findings** (V the runs; C the code).
- **The jets' noise** (noise34, C). While a row burns, `func_151A6600` calls `func_1508B20C(x, y, z, 500)` each frame
  for the row's middle jet only (record command 1), from the frame after the plate bottoms for 121 frames. The
  electric floor has no emitter. The bots then flee as from a shot (`func_1508B3F8`).
- **The electric floor.** The remake's bots stay on its box 3.6 times as long (944 frames a run against 259) and die
  there 1.3 times a run against 0.25. They are not passing: in `bot_match` seed 5, p3, role 2 with the twin guns
  drawn, stands in AI state 4 at (276, −399, 2817) for 560 frames, firing 120 shots at p4 and p14 200-1,900 away
  and landing a few. A long stand like this ends when someone dies, so it ties to the twin guns' hit rate below.
- **0x30's deaths** (o30s0-s4). The oracle ranges 4-9 deaths and 15-50 drops over 5 seeds; the old "9" and "50" are
  s0. With the mounted guns seated, the remake is 3-8 and 12-28. The twin guns are now drawn as long as the
  original's. The sniper and 0x23 are held 2.1 and 2.3 times as long, and drawn 3.2 and 11 times: in state 6 with a
  bot enemy, 0x23 is drawn 43% of the time against 14% (`goal.py`). The takes a run match (`takes.py`), so the bots
  holding them live longer: the twin guns land 0.028 drops a shot against 0.073.
- **The mounted gun** (o30s0-s4, o34b3s0-s1, o34b5s0). Every spell is a role-4 bot's and none ends while the gunner
  lives: it camps until it dies or the match ends. The remake's script and gun rules do the same; no change.
- **The Vault's roam** (v1a3, v1a7). 8 and 18 returns. At a take the spot (G+0x10F0) stays and the timer goes to
  300 VIs; 150 frames on the pickup is back on a new spot no pickup's spot is, its own included (v1a3 k2: taken at 486
  on spot 6, back at 636 on 4 while k0, k1 and k3 held 3, 5 and 0). The remake's `Roaming::pick` at each return is
  this rule.

**Left.**
- **V remake: the twin guns' hit rate** (0.028 drops a shot against 0.073 on 0x30). It keeps the sniper and 0x23 bots
  alive longer and the electric-floor stands going. Re-measure with the body-part hit test in `bot_match`
  (`Match::parts` filled), then the pitch byte obj+0x1D1's 1.1° steps and its 3-a-frame ramp (`func_1504C9E4`), still
  not ported.
- **0x23 drawn in state 6** (43% against 14% with a bot enemy): the draw rules of 0xBB (0x3530-0x3722, G+0x1EDC,
  G+0x24F8, query (5, 0x22)) were not traced against it.
- **The mounted gun is taken less** on 0x30 (2 spells in 8 runs against 4 in 5).
- **Not done: the crossbow's pitch** (o34p: 0x18's obj+0x1D1 is far from the aim's, units untraced) **and the sniper's
  hit rate**. The remake's sniper ray does not step the shot counter (`shots` 0 with 0x24 drawn, the oracle's
  s126+0x1AB 4.0 a run), so its rate cannot be read from `bot_match` yet. The hp drops given to a bot with 0x24 drawn
  are 5.8 a run against 1.6, as it is drawn 3.2 times as long.

## Race: the leftovers (2026-10-14)
**Batch G1, the Race lane. Route 12's 3 bots now lap as L2's: 1067-1081 frames against 1063-1081 (934 before). The
cause was the leader's deaths. In L2 the second bot takes the missile at placement 6 from 324 above it and fires at
once. The missile catches the leader (boosting at 229) 26 frames on. The hit runs the bot's class script 0xF: up at 55,
held in state 84. The wall then moves it, and script 0xBC blows it up (command 0x20: health 0). That happens three
times, then the leader is out (4128), and the other two ride at the rubber band's 100. The remake missed five things:
the AI's pickup reach (1.4 × 262.5), the missile's 200 a frame (155), the class script's hold, the wall's flag and the
kill. The track is now the original's Catmull-Rom spline. Route 11's laps went from 832-835 to 836-837 (L1 837), and
every finish moved into L1's, L2's, L7e's and L7f's spreads. The race total is on the wire, and the HUD reads it. The
camera's springs and finish cameras, the death blast and the hums' pitch are findings and a pack handoff only (Left).**
Research: `../lab/research/mp-original/race-leftovers-scratch/`: `code/` (`func_1505C1E4`, `func_1505C7D8`,
`func_15075C24`, `func_150497E0`, `func_150498A4`, `func_1511DF6C`, `func_15058EA4`, `func_150D5124` and others),
`path.py` (a racer's rows), `prog.py` (the table every N reads), `seg.py` (per-node times), `lldb.sh`. Runs on the
private `ConkerRecomp.g1c` (traces not kept):
- L1, L2 (M11's, read again): 3 bots and an idle human with 99 lives, routes 11 and 12.
- k1-k3 (lldb on L2's set-up): k3 watched bot 3's obj+0x232 and obj+0x1CA. The class went 13 → 15 at 1909
  (`func_1505C1E4` < `func_1505C7D8` < `func_150538CC`, the missile's hit). Health went 0 at 1915 (`func_15097A8C` <
  the script machine), the respawn put 1536 back at 2006, and 0x14 0xD came at 1977.
- d1: L1's set-up with DL and RAM dumps at reads 380, 384, 392 and 404 (the idle human dies at 378).

**The wire changed:** `RacerView.total` (`PROTOCOL_ID` stays 34, as the batch asked). **The shared simulation
changed:** the board's `held` and wall flag, Race's pickup reach, the track's spline (the client predicts with it), the
missile's step, and the script's `Kill` and `Class` ops.

| Part | What |
|---|---|
| conker-rules `race.rs` | **`Track::spline`**: `at` is the Catmull-Rom curve through the nodes before, at, after and after that, at the lateral (x, y, z; `func_15088F30`, `func_150497E0`). `follow` projects on the curve's tangent there (`func_150498A4`), with the chord as the scale. `angle` and `respawn` are unchanged. **`touches`**: a power-up touches within `REACH` 262.5 (100 · s126+0x1AF 168 / 64; 0xBC's command 0x7B sets 168 for every racer), × `AUTO_REACH` 1.4 on the autopilot, height as `func_1511DF6C`. **`Op::Kill`** (player command 0x20 with 0): health 0, so the racer's next frame is its death. **`Op::Class`** (command 0x14): a bot's class script (`Board::class`). A bot's hit (`knock`) is `bot_knock` and ends its canister's boost. The missile moves at most `MISSILE_STEP` 200 a frame, and at most `MISSILE_AHEAD` 0.2 of a node along the track (`func_150538CC`). The racer's wall flag is cleared at its death |
| conker-sim `board.rs` | **`walled`**: the wall routine moved the board this frame (obj+0xF4 bit 0x10000, query 0xC3). **`Board::held`** (a bot's class script 0xF): state 84 until `class` 0xD (speed ≤ `RIDE_AGAIN` 75) or 0x1F (its kick's). **`bot_knock`**: the knock's 30 frames (no hover, gravity 2) at `BOT_KNOCK_Y` 55, the speed kept. While held, the autopilot turns at most `KNOCKED_TURN` 256 a frame |
| conker-bots `brain.rs` | Race: player command 0x20 → `Kill`, 0x14 → `Class`, query 0x91 → 0xF while held, else 0xD |
| conker-rules `game.rs` | Race's pickups touch by `race::touches` |
| conker-sim `net.rs`, conker-server `main.rs`, conker-client `hud.rs` | **`RacerView.total`**: the race clock at the racer's last lap crossing (`0x800E0C18`), −1 for a racer the end rule finished short. The STATS rows show it. The client's per-human "finished its laps" map is gone, so a client that joins after the finish shows the total too |
| conker-client `audio.rs` | The missiles' sounds match within 8 frames at `MISSILE_STEP` |
| conker-bots `tests/rom.rs` | `race_bots_finish`: route 11 runs seed 2 (below). Route 12 runs seeds 1 and 2. The 3 bots' steady lap (the median of the last 3) is within 2% of L1's and L2's, and on route 12 a bot is out |

| Check (2026-10-14) | Result |
|---|---|
| rules `the_scripts_kill_and_the_reach` (new) | Command 0x20 kills on the next frame. A power-up 198 off and 324 below a bot touches it, but not a human |
| sim `bot_knocked_up_as_l1` (new) | y velocity 53 after the hit's frame, turning 256 a frame. After 41 frames the knock is over (hover back) but the hold remains (state 84). 0xD → state 5 at 75 + 1 |
| bots ROM `race_bots_finish` | Passes. Route 11 (seed 2): laps 836-837, finishes 4164-4214, 2 missiles, the human dies at 376, 596, 844. Route 12 (seeds 1 and 2): steady laps 1067-1081, finishes 5156-5229, 10 and 11 missiles; the leader is hit at 1886, 2927, 3997 (seed 2: 1903, 2938, 4010), blown up 7-40 frames later each time, out after the third. 7 bots: mean finish 4245 (route 11) and 4766 (route 12) |
| `bots.sh MODE=race MAP=33 BOTS=3`, 60 s: route 12 seeds 1 and 2, route 11 seed 1 (ports 5980, 5982, 5984) | All pass: every bot moves, every client sees it; no fault, unknown native or panic. Each race is decided at the idle human's elimination (t 1169, 1174, 1149; 3 lives), before the first lap ends. Boosts 3, 3, 2; missiles 1, 3, 2; knocks 2, 4, 2; bot 1 dies once in each (the chasm, Left) |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`) | 552 pass, 1 ignored (with the Tank and pack lanes' work in the tree); `check` and wasm clippy pass |

| Run (ROM path, an idle human with 99 lives) | Before | After | Oracle |
|---|---|---|---|
| route 12, 3 bots: steady laps | 934, nobody dies | 1067-1081; the leader out after 3 deaths | 1063-1081 (L2: the leader blown up at 1916, 2971, 4036, out at 4128) |
| route 12, 3 bots: finishes after the go | (not kept) | 5156-5229 | 5170-5190 |
| route 12, 3 bots: missiles | 1 | 10, 11 | 10 (L2: 846 … 5077) |
| route 11, 3 bots: steady laps / finishes | 832-835 / (not kept) | 836-837 / 4164-4214 | 837 / 4178-4230 (L1) |
| 7 bots, mean finish: route 11 / route 12 | 4224 / 4720 | 4245 / 4766 | 4249 / 4814 (L7e, L7f) |

**Findings** (V the runs; C the code).
- **The hit** (k3, L1, L2). The missile's hit (`func_1505C7D8` → `func_1505C1E4`) sets a bot's class script to 0xF. Its
  op 6 (`func_15075C24`) gives y velocity 55 under gravity 2, and it holds state 84. The board's knock still turns the
  hover off for 30 frames (L1 bot 1, hit at 234: gravity 2 to 265, then the hover). 0xBC counts 61 frames of 0xF
  (`G[0x114]`, `timer9`), then sends 0x14 0xD: state 5, speed 100 → 75 (L1 bot 2, 288 → 348). While held the bot turns
  at most 256 a frame (L1 288-337). A B press (0x14 0x1F) ends the hold too (L1 bot 1, held from 234, kicks at 305).
- **The kill** (0xBC). A bot the wall moved gets command 0x20(0), then 0x21 and `obj(0xE020)`: outside trigger 0xF
  (at 0xAE4), and in class 0xF on any walled frame (0xBF2-0xC5A). In L2 the hit bot's speed falls 25 a frame to 100;
  the wall moves it at 1915, and it is in state 112 at 1916. At 2971 the bot, lifted again at 2959, hits the roof at
  y 566.
- **The reach** (L1, L2, RAM). s126+0x1AF is 168 for every racer from the match's start. `func_1511DF6C` multiplies by
  `0x800A3304` = 1.4 when s126+0x84 ≠ 0 (an AI-driven racer). It does so in every mode for bots; only Race's is ported.
  L2 1881: bot 2, 198 off and 324 above, takes placement 6's missile.
- **The missile** (`func_150538CC` kind 0xD). Each frame it moves at most 200 toward the track point 0.2 of a node
  past its own place, 100 up. Homing, it moves toward the target's feet. L2: fired at 1883, 1.3 nodes behind the
  leader boosting at 229, it hits 26 frames on.
- **The spline.** `func_15087350` projects with the curve's point and normalized tangent, scaled by the chord: at
  most ±0.5 a step, within 50, 11 steps. Its +0x14 is the along-track speed (about the xz speed).
- **The hums' second value** (`func_150D5124`, C). Hum 0 (0x5B3): 5 × the height (at most 1000) in state 2, else 400.
  Hum 1 (0x5B4): 400 in state 0, 0 in state 1 or with 5 or more racers, else 400. Hum 2 (0x5BC): 0. It rises dt · 256
  a frame and falls dt · 8 (512 and 16 at 2 VIs a frame). It goes to the voice's +0x20, and to
  `func_100111C8(id, value, volume)` at the stop. I: the pitch, in a unit not measured.
- **The death blast** (d1). Effect type 0x19 (`func_151321D0` update, `func_15132B80` draw; lists at 0x800DCE50 +
  4 × type). At read 380 there are 11 objects, 51 above the death point:
  - a smoke shell: scale 2.75, alpha 234, model DL 0x801C5310;
  - a fire shell: scale 1.35, alpha 110, DL 0x801C4360;
  - 9 debris: scale 0.5, alpha 255, flying 12-20 a frame outward. DLs 0x801C3520 ×2, 0x801C2EA0 ×2, 0x801C1E00,
    0x801C2210, 0x801C0B00, 0x801C0480 and 0x801BAB40, with textures 0x801C2890 and 0x801BADD0.

  5 debris are left at 404.

**Left.**
- **The lateral-0 bot at the chasm** (after node 22, lap 1). In about half the seeds it is lifted 9 frames where L2's
  is lifted 10 (846-855), lands about 40 low on the far ramp at (−8517, −350, 263) (L2: (−8523, −316, 225)) and dies.
- **The bots' turn.** In state 5 the original's bots turn up to 1792 a frame (V). The remake's `AUTO_TURN` is 833 (I).
  Not changed.
- **The start's kicks** follow the script's `random(0x78)` timers. L1's bot kicks the idle human at 305-316 (its
  deaths 378, 599, 845). Seed 2 does too (376, 596, 844), but seed 1 doesn't (357, 577, 825), so the ROM test runs
  seed 2. Not ported: a kick's speed 60 at its start and end (L1 bot 3, 276 and 288), the class script's frame
  of state 15, and 0x21 and `obj(0xE020)` at the kill.
- The missile's hit radius (`MISSILE_HIT` 120, I), and its homing aims 40 up (C: the feet).
- **Camera** (unchanged). M13's list in "Race: the client" stands: mode 8 (`func_150D1530`) with its FOV, the boost's
  distance, the vertical springs, the hand-over after the go, the death's look-down and the finish cameras.
- **Hums' pitch.** The law is above. `Playing` takes its rate at the start, so a per-tick rate control (like the
  volume's) is needed, and an oracle audio dump (`CONKER_AUDIO_DUMP`) to fix the unit.
- **Handoff, pack lane: the death blast.** The pack needs the type-0x19 models above (the two shells and the 7
  debris models) with their textures, and the shells' scale and alpha over time (decompile `func_151321D0`). Their
  asset is not identified. Then the client draws them in place of `beach::burst`. To regenerate the dumps:
  `OUT=/tmp/g1c/runs BIN=./host/build-headless/ConkerRecomp.g1c SCEN=11 BOTS=3 RACE=5 POKE=30:800D2144=63060000
  DLS=380,384,392,404 race-scratch/run.sh d1 20 race-scratch/idle.txt`. Then run `race-client-scratch/cam/deb.py
  dl_380.bin 0x19`, and `../lab/hud/texfind` on the dumps for the texture ids.

## Tank: the leftovers (2026-10-14)
**Batch G1 L2b. The bots' shells now hit at the original's rate: 26.7% of their shots over seeds 1-12, against the
oracle's 25.6% over ten runs (33.8% before). Two fixes did it: the gun's pitch is now the original's slow controller
instead of a solve, and the hull's volume is the original's ellipsoid. C-right turned the turret the wrong way. The ROM
test's tiny seeds drew too many Collectors. Seeded as the server is, the bomb's numbers fall in the oracle's spread
(blast deaths 16.5 against 14.8, plants 3.9 against 3.8). Op 0x67 of the class scripts sets a bot tank's accel and
brake for one frame. Without it, the carriers into bunker E parked 2 units outside its trigger and never planted.
Wrecks are solid while they show, and lie where the death's knock left the body. The scope shot does 1 damage, as the
remake's already did. The client tilts the hull on slopes. Left: shell deaths run low (7.5 against 10.5), the per-side
treads (a pack handoff), the scope's bullet path, the engine's pitch.** Research: `../lab/research/mp-original/tank.md`
§10 and `tank-leftovers-scratch/`: `run.sh` on the private `ConkerRecomp.g1b`, and `counts.py`, `blast.py`,
`carry.py`, `cycle.py`, `plantpos.py`, `roles.py`, `shells.py`, `aim.py`, `ellipsoid.py`, `pitch.py`, `states.py`.
The runs (traces not kept):
- h1-h9: 7 bots and an idle human, 310 s, the RNG word 0x800885B4 poked at read 20: h1 1234567, h2 7654321,
  h3 24681357, h4 99991111, h5 31415926, h7 16180339, h8 14142135, h9 27182818. h6 is unpoked.
- sc1, sc2: two humans, P2 600 ahead of P1 (`sc1.poke`, `sc1_p1.txt`, `sc2_p1.txt`). P1 takes the launcher,
  scope-fires 4 times, kills P2 with shells, then drives at P2's wreck (sc1 from 560, sc2 from 380).

The wire is unchanged: `PROTOCOL_ID` stays 34. **The shared simulation changed.** conker-sim's tank `step` turns the
turret the other way on C-right/C-left (a client predicts its tank's turret as the server now turns it), and
`Drive::Stick`'s `clip` became `rates`. The rules' shells, the bots' pitch and rates and the solid wrecks are
server-side.

| Part | What |
|---|---|
| conker-rules `tank.rs` | **`BODY`**: model 119 has no body parts (`func_1508114C` → −1), so the shell's test is `func_15081C20`'s ellipsoid: radius obj+0xD2 113 across, obj+0xD4 90 up, centred obj+0xD6 52 over the feet. `hits_hull` scales y by 113/90 and casts against a sphere. A dead tank stops a shell unhurt (its obj+0xF8 keeps 0x40). **The pitch**: `solve_pitch` gives way to `range_error` (`func_150ED298`: how far short a shell at this pitch lands, 148.25 a frame each way, 1.25·t² of fall; out of reach, the pitch it lacks × 1000). Each frame adds 0.001 × that error. A bot's aims at the target's feet with no clamp, and its aim error aims 400 nearer. A human's aims 40 over and is clamped to ±4° a frame. **`Tank::rates`**: the bots' mover rates for this frame (cleared after the players). A bot steps with them, else with 0x32 each; a human gets 0x32 in its fire clip, else the stick's. **Wrecks**: the pos is set after the death's knock frame, and a wreck pushes as a tank body while it shows once its slot is back. `hull_pitch` is public |
| conker-sim `tank.rs`, `movement.rs` | C-right −3, C-left +3: `turret` is −s126+0x1BC (the shells fly at facing − 0x1BC, C-right adds 3, V b1). `Drive::Stick { rates: Option<[u8; 2]> }` (accel and brake ×10, obj+0x247/+0x248) replaces `clip`, and `CLIP_RATE` 0x32 replaces `CLIP_ACCEL` |
| conker-rules `gas.rs`, `game.rs` | `Effect::TankRates(slot, [accel, brake])` goes into `Tank::rates` |
| conker-bots `npc.rs`, `lib.rs` | Op 0x67 (`func_15077364`) keeps a1 and a2 in `Npc::rates`. The tank branch sends them as `TankRates` once, then puts back 0x32 |
| conker-client `tank.rs` | The hull is pitched by `hull_pitch` (obj+0xB8, V t1), so the gun's world pitch is now the rules' |
| conker-bots `tests/rom.rs` | `tank_match` is seeded as the server (`GAME_SEED ^ seed`). `tank_bots_bomb_and_fight` checks against the ten runs, including the longest carry. `tank_bomb_as_c1` waits for both the doors and the bomb, since the new seed's bomb is back before the doors open |

| Check (2026-10-14) | Result |
|---|---|
| rules `shells_hit_as_f2`, `wrecks_stand_as_tanks`, `shield_and_turret` | f2's lines 98 off the feet (hit), 148 and 197 (miss). sc2: the wreck lies where the knocked body lay, and a tank driving at it stops 110-160 short. C-right → 357 |
| sim `a_bots_mover_takes_its_scripts_rates`, `tanks_push_and_turrets_turn` | h4 3424-3427: 10 → 9.2 → 4.2 → 0, and 0.4 from rest. C-right then C-left |
| bots ROM `tank_bots_bomb_and_fight` (seeds 1-4) | A run: 340 shots, hits 26.3%, shell deaths 7.2, blast deaths 17.2, plants 4.2, the longest carry 1194. Every run plants, kills by blast and ends each carry within 1500 |
| bots ROM `tank_bomb_as_c1` | shown 401, plant 429, doors +231, haze +271, blast +300, open +389, back +347 |
| `bots.sh` `MODE=tank MAP=24 BOTS=7`, 150 s, seeds 1 and 2 (ports 5970, 5974) | Both pass: every bot moves, fights, dies and respawns, every client sees it; no fault, unknown native or panic. Seed 1: 3 takes, plants at 1617 and 3237, blast deaths 9, shell deaths 2, 3 power-ups. Seed 2: 3 takes, plants at 1675 and 3514, 9 and 3, 5 power-ups |
| `../lab/screens/tank-leftovers/shots.sh` a, t (ports 5972, 5974; release build) | climb70.png, climb90.png: slot 0 stopped on the bridge's south ramp (z −1264, 9°), seen level from the side. The hull lies along the ramp, nose up (its top edge ~7° against the ramp's ~10° in the frame). climb62.png from above. Part a's four shots are unchanged |
| `bun run test`, `bun run check`, wasm clippy | Pass: 552 tests, 1 ignored (with `CONKER_ROM`); `check` and wasm clippy clean |

**The numbers** to read 9023: the oracle's ten runs (b1, h1-h9) against the remake's seeds 1-12.

| | Oracle (range) | Remake, old seeding | Remake now |
|---|---|---|---|
| Shots a run | 350 (312-419) | 327 | 331 |
| Hits of 1 / shots | 25.6% (19.6-29.5) | 33.8% before this batch, 27.5% with the pitch and volume | 26.7% |
| Shell deaths | 10.5 (5-14) | 7.0 | 7.5 |
| Blast deaths | 14.8 (11-23) | 17.9 | 16.5 |
| Plants | 3.8 (2-5) | 4.2 | 3.9 |
| Takes; failed carries | 5.2; 0.9 | 5.0; 0.3 | 5.1; 0.8 |
| Longest carry | 1197 | ≥ 3300 in 3 of 12 seeds (once seeded as the server) | 1415 |

**Findings** (V the runs above; C).
- **The hits.** The gun's pitch is never solved. The original adds 0.001 × `func_150ED298`'s miss distance a frame
  (`func_150ED748`), so the pitch trails a moving target and the original's bots miss more. The volume: f2 hits a line
  98 off the feet and misses 148 (M9 had 150 and 200). On b1 the ellipsoid calls 58 of the 60 hits, and the old
  150-wide cylinder 81. Residual (I): clip 1's recoil moves joint 3, whose matrix the shell takes, and the pitch
  ignores the lead.
- **The bomb.** The "21 against 12" compared seed 1 with b1 alone, and b1's first bomb waited 5485 frames. Per blast
  the deaths agree (4.1 in the oracle). Every tank outside a bunker dies, immune or not (14 immune ones died over b1
  and h1-h5), and the dead are spared, as in the remake. Seeding the test with `Rng { state: 1..4 }` made the first
  draws near 0 (the 2nd and 7th are 0 for any small seed). The role roll (30/25/15/15) then gave Collectors: slot 4
  collected the whole match in 3 of 6 seeds.
- **The E carriers.** Scripts 4 (path) and 5 (stop) start with `67 8a 04 08 00`: accel 0.4 and brake 0.8 for that
  frame. The tank's update sets 0x32 (5) every frame. V h4 to read 4000: +5 ×396, −5 ×168, first steps +0.4 ×82 and
  −0.8 ×37, never the stick's 1.4 or 2.0. A carrier into E stops 120 from home node 0 (x 7127) and coasts 6.7 to
  x 7021-7023 (b1, h4, h5: 4 plants), inside trigger 0x14 (x > 7017). Braking 5 from the first frame, it stopped at
  7015.2 and never planted (seeds 1, 6 and 9: 3300-5100 frames).
- **The scope shot** (sc1). With 0x02 and R held, each Z plays the fire clip and the target loses 1 hp two reads after
  the press (the read after the fire clip's), with clip 7 and the knock. No shell object flies: it is the bullet
  pool's shot. The remake's scoped Z fires a shell, 1 damage too, which arrives a frame later (+3 at 600) on an arc.
- **Wrecks** (sc1, sc2). Prop 18 + slot shows at the knocked body's spot (5904, −716, 708 at 279). While it shows it
  holds a tank driving at it 127-150 off, after P2's respawn (366; sc2 444-471). Once it is gone (609), a tank drives
  through (sc1 630).
- **The class scripts' other ops** (0x31, 0x9B, 0x70, 0x3B, 0x8A) are not ported. Op 0x67's +0x246 (a0 + rand % a3,
  the turn, I) is not read. The AI states' shares and their distances already match b1, h1 and h2.

**Left.**
- **Shell deaths run low**: 7.5 against 10.5, from the same hits (88 a run against 89.5). The oracle's hits finish
  more tanks. I: the attackers' "weakest enemy" (0x66) or the retreat at hp ≤ 2, not checked.
- **Per-side treads, a pack handoff.** `characters/119/character.gltf` draws both treads in one primitive (mesh
  `bank01_0119`, primitive 13, `material_13`), skinned on joints 12 and 13. The game scrolls two tiles
  (`func_150ED638`). The pack should split that primitive by joint into two, each with its own material, and list them
  in `character.json`'s `tank` as `treads: [m, n]` in `tread_joints`' order. The client then scrolls each by
  clamp(v ± Δfacing/70, ±45) × −7 (model 119: slot 0 takes v + Δ/70). Above |target| 4, v is
  v·[0x800A15B8] + target·[0x800A15B4] (constants not read).
- **The E plants' margin is thin.** The remake's carrier arrives about x 7012.5 and coasts to about 7019, 2 past the
  trigger's edge, because a bot's stick asks 10.24 and 33.28 where the original sets obj+0x44 to 10 and 33. Changing
  the bots' stick mapping can park E's carriers again (`tank_bots_bomb_and_fight`'s longest carry guards it).
- **The scope's bullet**: the pool's (`func_151C0698`: speed 200, life 0x12C). Its line is not read (I: from the
  scope's eye, level along the turret).
- **The engine's pitch.** V snd saw f14 at 1.445-1.482 from idle to full speed (≤ 2.5%). `func_151BB0E0`'s formula
  is not read, and the engine is `audio.rs`'s (another lane).
- **Muzzle smoke and the carried bomb's look** need what the pack lacks: `func_15102B38`'s smoke particles, and the
  model and joint item 0x18 draws on the tank (`command(p, 0x4B, 0x18)`). Next: a DL dump of c1 around 60.

## Held weapons: the crossbow's rig, body parts and item 0x3A in the pack (2026-10-14)
**Batch G1 L2a, the pack lane. The pack now holds what the crossbow's string, the head test and item 0x3A need; nothing
draws or tests them yet (the handoffs below). The crossbow and the bone crossbow have their own rig with the string's 3
clips (`crossbow.gltf`), and `held.json` hides them at the holster's op 41 (0xC6, t14). Every player model in the game's
body-part table has its spheres and broad sphere in `character.json` (`parts`). Item 0x3A's grenade (#137 on 0x1A,
#186 on 0x3F) is in the hand glTF, in flight in `projectiles.gltf` and rigged with its clips (`lob.gltf`). Its sounds
and HUD icon were traced on the oracle and packed: a looping fuse from the pull, its own bounces, 0x26's blast, and
another cell of the grenade texture. Left: the client and rules sides (handoffs).** Research:
`../lab/research/mp-original/held-leftovers.md` §1, §2 and `held-pack-scratch/`: `run.sh` (plain runs: the grenade
object's anim struct `*800CCBF8`, P1's s126, DL dumps) and `snd.sh` (lldb sound calls), on the private
`ConkerRecomp.g1a`, with L1's C-down script (`scripts/c3.txt`); `tools/*.py`, `out/` (the runs' summaries). Runs: p1a,
p3f, p34 (C-down on 0x1A, 0x3F, 0x34; DLs 110, 126, 140), s1a, s3f, s34 (the same with the sound calls).

No wire change: `PROTOCOL_ID` stays 34. The pack format stays 1 (new fields are optional, new files additive).

| Part | What |
|---|---|
| extract-character `held.rs` | **Op 41** (`func_1506DBD4`: delete the actor's first child object) is read as a removal of every kind-0 child at its time: #110's and #47's `remove` gain (0xC6, 14.0) (the only op-41 route in the Tedi's table) |
| conker-extract `lib.rs`, conker-assets `lib.rs` | **The rigs** (`assets::RIGS`, `rig_gltf`, `conker_extract::rig`): `weapons/deathmatch/crossbow.gltf` (+ `.bin`, `textures/crossbow-00..07.png`): one skinned mesh `bank01_0110` on `joint_00..02`, list 0 = #110 (the crossbow), list 1 = #47 (the bone crossbow), animations `pair_0001_anim_0001` (1 tick), `pair_0002_anim_0002` (31, the fire), `pair_0003_anim_0003` (36, the holster). `lob.gltf` (+ `.bin`, `textures/lob-00..02.png`): mesh `bank01_0137` on `joint_00..03`, list 0 = #137 (0x1A), list 1 = #186 (0x3F), the same three names (30, 24, 30 ticks). Clip 0 of both is the bind pose (the glTF's rest). **Item 0x3A's models** (`LOB_ACTIONS` 0x86, 0x9F): `held.json` gains #137 and #186 (the last two, joint 9, no events: code creates them), so 24 held models; `projectiles.gltf` scenes 6 (#137) and 7 (#186). `held.gltf` keeps the crossbows' bind-pose scenes |
| conker-extract `lib.rs`, conker-assets `lib.rs` | **`parts`** (`body_parts`, `part_models`, `assets::Parts`): `{"broad": [r, h], "spheres": [[joint, part, radius, x, y, z], …]}` in table order, from the 23-entry table at 0x8009BD30 (set pointers 0x80086C60, counts 0x8009CBCC) and the bank-0x11 record's +0xE/+0x10; on 0, 59, 83, 117, 128, 136, 144, 150, 156, 157, 159, 160, 176-178, 180. None on 119, 152 and the rest (the cylinder) |
| conker-extract `audio.rs`, conker-assets `audio.rs` | **Item 0x3A's cues**: `On::Lit { weapon: 0x3A }` (new): sound 0x96 (it loops: `loop` 1-5143), volume 14000, 400-17000, channel `a`, at the grenade from the pull's first frame; `On::Bounce { weapon: 0x3A }`: 0x186-0x188, 28000, 300-1800, channel `a`; `On::Explosion { weapon: 0x3A }` = 0x26's (0x60D, 32700, 100-3000, pitch 0-499) |
| conker-extract `hud.rs`, conker-assets `hud.rs` | **`grenades.lob_icon`**: `grenade.png` texels [16, 15, 11, 14] (the bomb, 11 × 14) |
| conker-extract `pack-readme.md` | The new files, `parts`, `lit`, `lob_icon`, the held entries and glTF names |

| Check (2026-10-14) | Result |
|---|---|
| extract (ROM) `child_object_rigs` | #110: 3 joints, clips (id, pair, ticks) (0, 0, 66), (1, 1, 1), (2, 9, 31), (3, 10, 36); clip 1's joint-1 translation (13.015, −2.472, 67.389); clip 2's joint-2 scales start (0.002, 0.703, 1.0); #47 and #186 equal their rig's first entry in joints and clips; both rig glTFs name the three animations and draw lists {0, 1}. #137: 4 joints, ticks 10, 30, 24, 30 |
| extract (ROM) `body_part_spheres` | All 23 entries (model, set, spheres, head spheres) as `parts_rom.txt`. 117: set 4, 35 spheres, the first [12, 0xA, 38, 0, 33, 5], the ears last, broad [117, 95]. 83: 7 spheres, none of part 0xA, broad [339, 227]. 159: parts {0, 4-9, 0xA}. 152, 154, 155, 119, 138: none |
| extract (ROM) `slice_in_memory`, `projectiles_and_undrawn_quads` | 24 held models (mesh `01-89-00`, `01-ba-00`); 8 projectile scenes |
| extract (ROM) `pack_reads_back_as_the_rom_path` | #110 removes at (0xC6, 14); the last two entries (137, 9) and (186, 9) without events; the Tedi's `parts`; none in 152's and 119's `character.json`; both rig glTFs read back equal to the ROM path's |
| extract (ROM) `pack_audio_reads_back`, `pack_hud_reads_back` | The three 0x3A cues as above, no `holding` or `fuse` for 0x3A, its sounds in `sounds.json`; `lob_icon` |
| Pack | `bun run pack --force`, then `lab/tools/verify-pack.sh`: pack equal (2099 files) |
| `bun run test`, `bun run check`, wasm clippy | Pass: 551 tests, 1 ignored (with `CONKER_ROM`); `check` (fmt, clippy on the 10 packages, `web/app`) and wasm clippy pass. Run with a private `CARGO_TARGET_DIR`: in `target-net` the parallel lanes' builds removed dependencies mid-build three times. The first run stopped on conker-client's doctest build (`hull_pitch` private, the Tank lane's in-progress `tank.rs`), which passes since. A last `bun run check` in `target-net` passes |

**Findings** (V p1a/p3f, s1a/s3f; C).
- **The grenade in the hand is a child object** (actions 0x86/0x9F, kind 0, joint 9), created by code at the pull's
  first frame (P1's item s126+0x75 = 0xBA from then to the throw's first frame), not by a clip event. Its own clips (obj
  2's anim struct): clip 2 from the pull's first frame (own clock 0 → 22, a tick a frame), clip 1 from the next frame
  (own clock from 0, a 30-tick loop) while 0x1FA holds, and clip 0 (the bind pose, no longer a child) from the throw's
  first frame (0x1FC). The same on 0x3F with #186. Its object scale (obj+0x14C) is 1.0, a player's 0.5 (I: whether the
  hand draw uses it was not checked on a DL).
- **Its sounds** (V s1a, s3f; C `func_150C44A4`): 0x96 at the pull's first frame (14000, 400-17000), the grenade's
  one sound at a time; each bounce plays one of 0x186-0x188 (28000, 300-1800) by the shuffle of 0x26's bounces
  (`func_1000F568`, a bounce leaving it faster than 5 up); its blast is 0x26's (0x60D through `func_10010F88`, 32700,
  100-3000, 120 frames after the pull, as s34's 0x26). No in-hand hiss (0x513), no last-60
  fuse (0x3A1), no throw sound (the throw clip's sound ops are filtered by class, and the Wise Guy's check against
  0x3B fails).
- **Its HUD** (V p1a, p3f DL 110, 126; `rects.py`): one bomb per grenade left, at 0x26's x 247, 236, 225 and y 6, from
  the pull to the throw (gone by DL 140, where 0x26's icons stay).
- Noted, not changed: `clip_sounds` passes the model entry to `sound_op`'s character filter, while the game compares
  the anim owner's remapped model (`func_1506BF5C`: 0x800D1C90[model]+4 → the 0x80086CAC table) (C). Class-filtered
  sounds may differ on the picks. `MATCH_CLIPS` lacks 0x175/0x16C (the non-Tedi grenade clips).

**Left: the handoffs.**
- **Client, the crossbow** (`held.rs`, held-leftovers §1's recipe). Load `weapons/deathmatch/crossbow.gltf#Scene0`
  (`conker_assets::rig_gltf(PACK_WEAPONS, "crossbow")`), draw only the primitives of `extras.list` 0 for weapon 0x18
  (#110) or 1 for 0x41 (#47), hung on the holder's `joint_09` in place of `held.gltf`'s bind-pose scene, scaled as the
  held models. Pose: draw (0xFB, 0x2C8) → the bind pose; fire (0xFC, 0x2C9) → `pair_0002_anim_0002` at the holder's
  clock, clamped to length − 1 (30); holster 0xC6 → `pair_0003_anim_0003` likewise (35); anything else →
  `pair_0001_anim_0001` (one tick). The loaded bolt: `projectiles.gltf` scene 3 under the rig's `joint_02` while the
  fire clock is in [6, 11). `held.json`'s #110 and #47 remove at (0xC6, 14): `shows()` already hides it from t14 (#47's is inert: the bone
  crossbow has no holster in multiplayer).
- **Client, item 0x3A** (`held.rs`, `projectiles.rs`, `hud.rs`, `audio.rs`; L1's handoff). In the hand: `lob.gltf`,
  list 0 on 0x1A, list 1 on 0x3F, on `joint_09`, from the pull's first frame to the throw's first (`Fighter::lobs`):
  `pair_0002_anim_0002` at frames since the pull (0-22), then `pair_0001_anim_0001` looping from 0. (`held.json`'s
  last two entries, `held.gltf` scenes 22 and 23, are the same models in the bind pose, for a client that doesn't
  pose them; they have no events, so `shows()` must key them on the item.) Thrown: `projectiles.gltf` scene 6 (0x1A)
  or 7 (0x3F) in place of the placeholder sphere (`projectiles.rs` loads scenes 0-3 by index, so 6 and 7 are inert
  until then). HUD: `grenades.lob_icon` instead of `icon`, at `at` + k·`step`, one
  per grenade left, while the item is 0xBA. Audio: `On::Lit { weapon: 0x3A }` at the grenade from the pull, stopped
  by its next sound (a bounce) or the blast (the grenade's channel `a`); `Bounce { weapon: 0x3A }` and
  `Explosion { weapon: 0x3A }` play as 0x26's do; there is no `Holding` or `Fuse` for it.
- **Rules/server, the head test** (held-leftovers §2's server recipe): read `conker_assets::Character::parts`
  (`Parts { broad: [i16; 2], spheres: Vec<[i16; 6]> }`; `None` → the cylinder). The broad sphere first (radius and
  height × the scale, a miss is no hit), then each sphere in table order at joint matrix · (x, y, z), radius × the
  scale, skipping joints hidden by the pop (0xF000 on 159/160, 0xFF8 on 90/116/122); a hit replaces the best if it is
  the first or **nearer and of a part ≥ the best's**. Who tests parts: the twin guns' and the pistol 0x39's bullets,
  the sniper ray and the projectiles. Check: `pc2`/`pc3` (the standing shot misses, the crouched one hits part 0xA on
  joint 12 at 177.7).

## Beach: the sea (2026-10-14)
**Batch E7b. War: Beach's sea is water now, not a floor. A player who falls in swims 41.67 under the surface, drifts
shoreward on the current, strokes with the stick at 40 %, wades out at the shore, and can jump out with A. The replays of
two oracle runs match to 0.00033 over 557 and 648 frames, the camera too where it follows. In the water a drawn weapon
is put away and B and Z do nothing, as in the original. The client predicts the swim, plays its clips at the original's
rates and its sounds (the stroke's splash, water footsteps, the swim's 0x6C-0x6E). Bots stay out of the sea, as the
original's do. Left: the swim's ripples and bubbles, the sea's camera hints, a swimmer's own death clip.**
Research: `../lab/research/mp-original/beach-environment.md` §8 and `beach-sea-scratch/` (`run.sh`, `dbg.sh`, `m2c.sh`
on the private `ConkerRecomp.e7b`). The runs, kept in `../lab/oracle/map2b/sea/`:
- sea: P1 dropped into the sea at read 100 and again at 200 (`sea.poke`), no input: the float, the current, the carry to
  the shore, standing up.
- swim1: P1 dropped in at 100 (`sw.poke`), `swim1.txt`: strokes south to the shore, the wade, back in, the jump out, a
  landing on the sea floor that goes back into the water, B and Z, a second swim out and the walk up the beach.
- `dbg_*` (lldb on swim1): the wade's end, the wall box, the floor query, the camera's water, the sounds.

`PROTOCOL_ID` 33 → 34: `PlayerState` gains the swim's fields, and the pack has a water layer with the current. The bump
also covers L1's client-side item 0x3A, which changed the prediction without one.

| Part | What |
|---|---|
| extract-scene, conker-extract, conker-assets | **Placement 1 is water** (`deathmatch_water(0x2B)`): out of the floors and walls, a `{"water": 1}` run in `maps/2b/collision.json` (10 triangles at y −539) with the current's path (4 points: x, y, z, speed). `Layer::Water`, `Run::current`. Audio: the swim's clips (0x22, 0x8B, 0xCC) join `MATCH_CLIPS`, op 40 (the stroke's splash) is decoded, and the footstep table gains surface kind 8 (`WATER_STEPS`) |
| conker-sim `collision.rs`, `ray.rs` | `CollisionWorld::water(x, z)` (the water triangle over the point by the floor's integer routine), `current`, `holds`. The line query stops at the water (C: no 0x40 split in `func_150ACBD8`) |
| conker-sim `swim.rs` (new), `movement.rs` | **The swim** (obj+0xAD): in at a frame's end more than 60 under the surface while falling slower than 5; the float to water − 41.67; the current's drift (obj+0x16C/0x170), faded 0.15 a frame on land; strokes (40 %, the turn 4 steps of 256, states 34/204/206/209); out under 10 over the floor (speed 15, gravity 4, the wade 139 or a hard landing); A within 90 of the surface jumps out (y velocity 46, the slow brake). **What the replays needed besides:** the wade's clip end (`swim::wade`: idle at clock 23, the move angle onto the facing); obj+0x7F (`aloft`: a landing under speed 9 snaps the move angle to the facing; a soft landing clears the lock); a swimmer's wall box 90 across, not 60; the floor query at the height after the fall; the swimmer held on the sea floor |
| conker-camera, conker-sim `follow.rs` | `PlayerView::swim` (the water's height) and `CameraWorld::water_below`. A swimmer's look-at is y + lift (×0.25 when deep, \|y − water\| ≥ 100); the water under the eye counts as the floor when it lies between the floor and the eye; a swimmer's base is water + 50; no 600 freeze in the water; a deep swimmer can't aim |
| conker-rules `game.rs` | In the water: a drawn weapon is put away at once, B and Z are masked (the swim's weapon handler, C), any action but Free/Mask/Pose ends, and item 0x3E's drive is skipped (obj+0xAA stays the swim's 40) |
| conker-anim | `id::WADE`, `SWIM`, `SWIM_DEEP`, `STROKE`, `STROKE_DEEP`, `SWIM_HOLD` and their rates (0.9, 1.2, e/18 + 0.2; blends 10, 9, 7, 4) |
| conker-client `audio.rs`, `lib.rs` | A footstep with the feet under the water plays surface kind 8's sounds. `ANIM_IDS` gains the six swim states, so the local swimmer shows its predicted clip |
| conker-server `main.rs`, `bots.sh` | Logs `slot N swims` / `leaves the water`; `MODE=beach` reports the swims |
| `../lab/screens/sea/shots.sh` | swimmer.png (a stroke at the surface, own view), exit.png (wading up the shore), sea.png (free camera) |

| Check (2026-10-14) | Result |
|---|---|
| sim (ROM) `tests/sea.rs` | `sea` rows 101-659: 557 frames, max error 0.00018, the eye's 0.00073 over 400-659. `swim1` rows 101-749: 648 frames, max 0.00033, the eye's 0.00286 over 300-449 and 600-749 (450-599: hint 0x27). The tolerance is 0.01 |
| rules `swimmers_neither_draw_nor_fire` | Twin guns drawn, then dropped into a synthetic sea: holstered at once, 60 frames of B and Z fire nothing. A villager with item 0x3E strokes at speed 14 and keeps its boost |
| extract (ROM) `pack_audio_reads_back` | The stroke's splash (0x8F, 0x90, 0x614, 0x615), the wade's steps (kinds 4, 1), the swim's 0x6C-0x6E, surface kind 8's 0x24C, 0x252, 0x2AB |
| Pack | `bun run pack --force`, then `lab/tools/verify-pack.sh`: pack equal (2077 files) |
| Smoke (`bots.sh`, `MODE=beach MAP=2b BOTS=7`, port 5970, 180 s) | No bot swims. Every bot moves and is seen moving, the villagers die and respawn, the Tediz fight; escapes, a detonation, a strike, 15 villager points. No fault, unknown native or panic |
| Screens (`../lab/screens/sea/shots.sh`, port 5980, debug binaries) | The current carries both villagers to the shore (in at read 30; out at 85 and 173), as in the oracle's `sea`. swimmer.png, exit.png, sea.png. The swimmers' clients predicted the swims and exits without a rollback (each client's one rollback at tick 930 is the match start's, the same on the Tedi's client, which never swims; the later ones came on land: a wall push, a fall into an escape tunnel) |
| `bun run test`, `bun run check`, wasm clippy | Pass: 546 tests, 1 ignored (with `CONKER_ROM`). `swim.rs` allows `neg_cmp_op_on_partial_ord` as `walls.rs` and `ray.rs` do (the MIPS compares' NaN) |

**Findings.**
- **The wade ends on its clip** (C, V swim1 236, 418, 514, 528). State 139's clock gains (0.4·speed + 0.6·drive)/18 +
  0.2 a frame; at 23 the player idles, and Beach's item handler, run after the drive, leaves the next frame at 100 %.
  The rules call `beach::drive` before the step, so the port's frame after a wade's end drives at 75 % (+1 speed for a
  frame). The replay feeds obj+0xAA from the run.
- **The sounds** (V `dbg_snd`, C the op handlers): the stroke's op 40 at tick 15 of 0x22; op 9 footsteps use surface
  kind 8 when the feet are under the water (the wade's 0x2AB-0x2AE and 0x24C-0x24F, a landing's 0x252); the swim's op 82
  at 25 and 26 of 0xCC. No splash on entry in multiplayer (`func_15083568`, C).
- **The camera hints** of kind 5 at records 0x26/0x27 (the sea's sides) take the original's camera out of follow (swim1
  450-599). Not ported: the port keeps following there.

**Left.**
- The swim's effects, all untraced on screen: op 12 (`func_1507E7E4`, kinds 6 and 7 on clip 0xCC; the landing's 0x21),
  op 102's bubbles (0x29/0x2C on 0xCB, 0xCE, 0xD1), and the likeliest ripple and foot-splash spawners: op 90 on the
  stroke (0x23/0x24 → `func_151AABC4(obj, 0|1)`) and on the wade (0x29/0x2A → `func_151AB920(obj, 0|1)`), op 97 on the
  wade's steps (`func_15141A7C`, by the floor's kind). The client has no clip particle effects yet.
- Camera hints of kind 5 (records 0x26/0x27).
- A swimmer's death (C, untraced in time): `func_1505E874` picks cause 4 when obj+0xAD is set, and `func_1507CD64(obj,
  4)` plays clip 0x24, not the land deaths' 0x58/0x13. The port hits a swimmer as on land (C: the hit runs) and plays
  the land death.
- SWIM_HOLD 203's rate (I: 1); SWIM_DEEP's first frames at 2 (V, not modelled). The eye's water is queried every tick,
  not only with the original's full query.

## Heist and rules leftovers (2026-10-14)
**Batch L1. Heist's bots were never short of takes. Over four oracle matches the original gets 17-30 takes in 9000
frames, and hb3's 30 is the top of that range. The port gets 13-24 on the ROM path and 17-21 on the server. A thrown bag
now stops at walls and slides along them, as in the original. On 0x1A and 0x3F, C-down pulls item 0x3A: a lobbed
grenade with its own clips, flight and bounces. The dead pose's 0x18A is a random twitch, 1 in 8 on the first death.
Race takes at most 2 humans. Left: item 0x3A's client side (handoff below), porting the twitch, and the bots' retreats
near the carrier.** Research: `../lab/research/mp-original/heist-leftovers-scratch/`. The tools are `run.sh` and
`runobj.sh`, run on the private `ConkerRecomp.l1`. The runs, each with its `.out` summary:
- hb, hb2, hb_0BADF00D, hb_00C0FFEE: Heist, 1 idle human and 3 bots, 9000 frames, `heist-scratch/idle.txt`. hb has no
  poke and reproduces hb3. The others poke the RNG at read 22 (`22:800885B4=`12345678, 0BADF00D, 00C0FFEE).
- bt1100, bt1200: a bag thrown at the vault's ring wall.
- c3a, c3f, g26: C-down on 0x1A, 0x3F and 0x2D, with `scripts/c3a.txt`.
- `twitch.out`: s126+0x1A9 in five combat traces and in hb.

No wire change: `PROTOCOL_ID` stays 33.

| Part | What |
|---|---|
| conker-rules `heist.rs` | **The thrown bag's walls.** Its flight and slide now go through the wall routine (`CollisionWorld::sweep`) with a 24-unit box (`heist::SWEPT`). V bt1200: the bag hits the vault's ring wall in the air. It is held at x −1324.9 and goes on along the wall in z, then lands and slides along it. V bt1100: the bag lands short, slides into the wall and stops at −1324.6. The 24 is fitted on the ROM's walls: 24 gives −1324.92 and −1324.56, while 25 and 29 stop 1 and 5 units short |
| conker-rules `projectile.rs`, `game.rs` | **Item 0x3A** (`projectile::LOB`), on with `Arena::lob` on 0x1A and 0x3F. The pull is 0x1F9 (22 frames, then 0x1FA held). Letting C-down go plays the throw, 0x1FC (19 frames); the pull and the throw start on the same frames as 0x26's (V g26). The grenade leaves the hand on the throw's first frame, already a step along (`Grenade::lob`): speed s = ((240 − cook)·0.875 + 40)/2 at 30° (xz s·cos 30°, y s/2). The hand is at (9.0, 27.4, 62.4) (forward, side, up), a Tedi's at (4.6, 27.4, 41.9). The timer runs every frame from the pull, so the blast comes 120 frames after the pull, in the hand or not (V c3a 102→222, 502→622, 762→882; c3f 102→222). **The flight** (`Grenade::step_lob`) has no drag. Each landing halves the speed and bounces at 0.5·\|y_vel\| + 2. A bounce under 8 settles it (I: V 9.67 bounced, 6.16 settled). Once settled, the speed halves every frame. A wall takes 70% of the speed off and reflects the move (V c3a 590: 77.19 → 23.16; I: the sweep's push is used as the normal). `Fighter::lobs` tells item 0x3A by its clips. `item` shows 0xBA up to the throw's first frame, `camera_weapon` 0x3A, and `in_hand` is never true (s126+0x11A stays 1). The blast is 0x26's (V c3a: 2 hp lost at 460, a kill at 230) |
| conker-bots `lib.rs` | The wind-up noise ends at item 0x3A's throw. The bots throw it through their C-down (command 0x61 #4) |
| conker-server `main.rs` | Race refuses a third human (`RACE_HUMANS`: "Race allows at most 2 humans", `menus-and-flow.md`, C). `arena.lob` is set on 0x1A and 0x3F |
| conker-client `lib.rs` (one line, outside this lane) | `MatchArena` sets `lob` on 0x1A and 0x3F, so the client predicts item 0x3A the way the server flies it. A protocol-33 client built before this flies item 0x3A with 0x26's physics |

| Check (2026-10-14) | Result |
|---|---|
| Heist takes (1 idle human, 3 bots, 9000 frames) | Oracle: hb 30 (hb3's 30 carrier hits), hb2 20 (18), hb_0BADF00D 17 (13), hb_00C0FFEE 18 (15); mean 21.25. Port, ROM path, seeds 1-12 before item 0x3A: 24 23 18 23 22 24 15 19 13 16 18 21 (mean 19.7). Seeds 1-4 with it: 24 17 18 23. Server seeds 3-5, before L1's fixes: 19 18 21 |
| Unit and ROM tests | bots (ROM) `heist_takes_as_the_oracles`: 4 seeds, mean 16-30, each seed at least 10. bots (ROM) `heist_bag_stops_at_the_vault_wall` (bt1200, bt1100). rules `a_throw_stops_at_a_wall`, `a_lob_flies_bounces_and_rests` (c3a's first throw: the release step, landings 15, 26 and 32 frames on, at rest 436.56 on), `item_3a_is_lobbed` (clips, item bytes, blast 120 frames after the pull). server `race_flags_grid_and_classes` (a third human is refused) |
| Smoke (`bots.sh`, ports 5980-5988, 120-150 s) | Heist `BOTS=3`, seeds 1-3: takes 8, 9 and 11, one delivery. DM 1a, DM 2d and Raptor `BOTS=3`, Colors `BOTS=7`, Race. Every client connects and sees every bot move, and every bot moves. No fault, unknown native or panic. The FAIL lines are the known ones (a bot that never died). Race is decided |
| `bun run test`, `bun run check`, wasm clippy | All three stop in the sea lane's in-progress files. `conker-sim/tests/sea.rs` doesn't compile (E0284), and clippy fails on `swim.rs`'s `neg_cmp_op_on_partial_ord`. Without them: the same packages' tests pass, 541 with 1 ignored (all but `sea.rs`; rom.rs's 26 ROM tests take 70 s in debug). Clippy on all 10 packages and wasm clippy pass with that one lint allowed. `cargo fmt --check`'s only diff is `swim.rs` |

**Findings.**
- **Heist's takes** (V). The server and ROM paths don't differ beyond noise. The oracle's matches spread from 17 to 30,
  so hb3's 30 is the original's high end. It is not the port's rate. Comparing frame by frame on the first missed take
  can't be done: the bots' first choices draw the RNG from read 24 on, and the matches part there.
- **The bots near the carrier** (V, not fixed). Within 500 of the enemy carrier, the original's bots attack (state 8) in
  75-84% of frames, in all four samples. Before L1's fixes, the port's attacked in 61% and retreated in 36% (state 3,
  COOL > 0): more of its bots sit at hp ≤ 2 with COOL 600. The original's state 6 (fetch a pickup) lasts 1-2 frames; the port's lasts longer.
- **The dead pose's 0x18A** (V, C; not ported). 63 frames into clip 0x59, `func_15072A7C` runs (C). It adds 1 to
  s126+0x1A9 and plays 0x18A (rate 1, blend 4) if `(random_u32() & 7) < s126+0x1A9`. The count is 0 again at the
  respawn, so a first death twitches 1 time in 8. I: the call is a clip event at 0x59's end (64 ticks long).
  V `twitch.out`: the count goes 0 → 1 63 frames into 0x59 in e_w38, e_w39, e_w21, sg_e2, e40 and e40c. 0x18A plays in
  e_w38, e_w21 and sg_e2 only. sgpb's kill lies in 0x59 only 51 frames, so no call. In hb, 0x18A plays in 1 of 6 bot
  deaths, and the count is 0 at every spawn.
  - Why it isn't ported: the replays feed the game's RNG only on frames with a logged pick, and none lands on the
    twitch's frame (e_w38: reads 22 and 1020). A port would turn sg_e2's and e_w21's matches into misses.
  - The port spec: one draw 63 frames into an 81-frame 0x59; 0x18A for the last 18 frames when `& 7 == 0`. Then drop
    the fixed 63 + 18 rows of `damage::sequence` for 0x33 and `AREA_STAGGER`.

**Left.**
- **Item 0x3A on the client** (handoff, C/V as above). These need work:
  - The models: objects 137 (0x89, scene 0x1A) and 186 (0xBA, scene 0x3F), created by actions 0x86/0x9F. The pack
    lacks them, and thrown grenades are still placeholder spheres. Draw the model in the hand from the pull's first
    frame to the throw's first (`Fighter::lobs`, anim 0x1F9/0x1FA), at the hand point above. `held.rs`'s grenade
    entries are keyed on 0x176/0x175 and 0x16B/0x16C, so nothing shows for item 0x3A today.
  - The sounds: `audio.rs`'s grenade cues are keyed on item 0x26 (`GRENADE`). Item 0x3A's cues were not traced.
  - The HUD's grenade icon on 0x1A and 0x3F: not checked.
  - The clips 0x1F9, 0x1FA and 0x1FC are already in every character of the pack (7, 9 and 6 keys), and the client plays
    them from `anim`. `characters::clip` keeps them (V: classes 1 and 3 both use them).
- Item 0x3A's blast in the hand goes off at its hold hand, and the item byte after it shows 0xA6 (I: not traced).
- The settle threshold 8 and the wall's reflection are inferred (I). At c3a 590 the speed cut matches (77.19 →
  23.16), but the direction doesn't. The original turned 22 units before the port's wall and rose 16, probably off an
  object, and went on (−8.83, 7.49) a frame; the port goes on (−11.56, −0.67).
- The bots' retreats near the carrier and the long state 6 (above).

## Held weapons: the leftovers (2026-10-14)
**Batch E4, the held-weapons lane. Every gap was checked on an oracle run, and all but one are fixed. With a weapon
drawn, C-down strafes back and pulls no grenade. A hip bolt pushes its shooter while it sits at the muzzle, and bolts
and aimed shots first move on the original's parity. A bolt that nears a player plays its pass sound once. The
shotgun's point-blank sphere 0x3A lands with the kill. An unarmed player near an armed enemy runs scared (0x2B, which
were sg_e2's back-steps). A killed player keeps its weapon 3 frames. Everyone but the Tediz pulls with 0x175/0x16C and
holds bank-09 #76. The club's icon sits a pixel right. The tommy gun draws at 1.4 and is in the hand 7 frames in. Left:
the crossbow's string clips.** Research: `../lab/research/mp-original/held-weapons-scratch/` (`lldb.sh` on the private `ConkerRecomp.e4`; the runs
dth, cbp2, cbgate, cd09, cd15, sgpb, sge2a, sqgren, club1, k530, with their `.out` files). Installed as replays in
`../lab/oracle/`: combat7_cd09, combat7_cd15, combat7_sgpb, combat7_e_w38 (trajectories, events, scripts).
`PROTOCOL_ID` 33: `PlayerState::scared` is on the wire (E11 had taken 32). One server line in E11's lane sets
`Match::live` at the match start.

| Part | What |
|---|---|
| conker-rules `game.rs` | **C-down** pulls a grenade only with nothing drawn (s126+0x78 = 0, `func_1504CB98` at 0x1504CF9C, C). With a weapon drawn it strafes back (V e4 cd09: the twin guns' 0x22C while held; cd15: the flamethrower's 0xED). **`Match::live`, `first_move`**: a bolt or an aimed shot first moves on the first frame after its birth that is an even number of frames from the live frame (read 23). `func_150619A8` activates a new object on even reads (V combat5_cb2-4, cb_e1: 15 bolts, all first moving on odd reads; e4 cbgate). **The bolt's push**: until it moves, a bolt is an object of radius 5, centre 0 and scale 1 with no owner, so it pushes its shooter too (`func_1505D6F0`, C; V e4 cbp2: (1.843, 2.152), then (1.577, 1.841)). **The death**: 3 frames into a lethal hurt, a pickup's weapon gives way to the start item, holstered (`func_1508DC24`; V e4 dth: the twin guns drawn through 790-792, gone at 793). A start item stays (V combat6_e40's club). **The scare** (s126+0x4A): 20 while an enemy with a weapon drawn is within 30000 (squared, on the push's metric) of an unarmed player, else one less a frame (`func_1505D6F0`, `func_1504BC38`, C). **The spread**: no sphere tests the stun (no type flag 1 in `func_1505B9C4`, C), so 0x32 lands on 0x3A's stagger in the same frame (V e4 sgpb 790: 0x3A takes 1, then 0x32 kills). **`in_hand`**: the tommy gun attaches at 7, not 10 (V combat7_e_w38: 761 → 768) |
| conker-rules `characters.rs` | `clip()`: any model but the Tediz' (117, 177) pulls and throws as 0x175 and 0x16C (`func_15065A5C`'s case 0x26, C; V e4 sqgren: the squirrel; combat6_ctlw40c: the caveman) |
| conker-rules `damage.rs`, `weapon.rs` | Docs only. The club's lethal first swing was already right (V e4 club1, 850-1002: 0x11B for 19 frames, 0x58 for 52, 0x59 for 81). Also the C buttons and the spread's guards |
| conker-sim `movement.rs`, `net.rs`; conker-anim | **`state::SCARED_RUN`** 43, clip 0x2B at rate e/12: chosen when scared, unarmed and e ≥ 5 (`func_1504C0E8`, C; V sg_e2 766: 1.7833 at e 21.4). It may jump. `PlayerState::scared`, `Other::within`. Protocol 33. **`combat::TOMMY_DRAW`** 0x1DD plays at 1.4, as the pistol's draw (V combat7_e_w38 761-775; it played at 1, so the held model's draw event came at 11) |
| conker-client `held.rs` | The grenade in the hand goes by class (`shows(.., class)`): #76 in a non-Tedi's hand |
| conker-client `audio.rs` | **The bolt's pass**: a new shot beside a crossbow holder is a bolt. It plays `On::Pass { weapon: 0x18 }` once, when a player is within \|step\|·7.5 and within 0x100 of its heading (`func_150538CC`'s type 6 → `func_1505DDA8(obj, owner, 0x200, 15.0)`, C). Bolts drop the knife's flight cue |
| conker-client `hud.rs` | The club's icon a pixel right, the bat's a pixel lower (`func_150918EC`, C) |
| conker-extract, conker-assets | `HELD_CLIPS` takes 0x175: held #76, so 22 held models. The bolt's cue `On::Pass { weapon: 0x18 }`: sounds 1086-1089 (0x43E + rand & 3), volume 30000, 2000-2100 |
| conker-server `main.rs` (E11's lane, one line) | The match's `live: (t + LIVE_READ).wrapping_sub(game.setup.first_read)` |

| Check (2026-10-14) | Result |
|---|---|
| Replays (`combat_traces`, 65 cases) | combat7_cd09 10/10 events, combat7_cd15 8/8, position 0.0000. combat7_sgpb 13/13 (kill 1/1). combat5_sg_e2 39/39 (was 2 known). combat6_ctlw40c 35/35 (was 8 known). combat5_cb3 55/55, position 0.0000. combat7_e_w38 30/32, 3 known (below) |
| Unit tests | rules `c_down_with_a_weapon_drawn_steps_back`, `a_hip_bolt_pushes_its_shooter`, `a_killed_player_keeps_its_weapon_three_frames`; sim `state_choice_follows_func_1504c0e8` (the scare: e 5.88 runs scared, a creep unscared); client `draw_events_fire_on_the_attach_frame` (0x1DD's event at 8 frames); client `each_class_holds_its_own_grenade`, `a_bolt_passes_once` (the bolt plays one of 1086-1089 once, the knife only 559/560) |
| Pack | Re-exported with `--force` twice; `verify-pack.sh`: "pack equal" |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | 538 pass, 1 ignored (with E11's work in the tree); `check` passes (fmt, clippy on all 10 packages, `web/app`); wasm clippy passes |

**Left.**
- **The crossbow's string clips** (not done). The crossbow is drawn as its child object (bank-01 0x6E) in the bind
  pose. The string's clips are model 110's own animations, which the pack doesn't have.
- **Item 0x3A on 0x1A and 0x3F.** There (and for model 0x3B) C-down sets s126+0x19B = 5, and the manager gives item
  0x3A instead of 0x26: hold 0x1F9, throw 0x1FC, actions 0x86/0x9F, and the thrown object is 0xBA on 0x3F, 0x89
  elsewhere (C). Not done (L1: ported in the rules, above).
- **Beach.** The C-down grenade check runs only on scenes 0x2D, 0x30, 0x34, 0x1A and 0x3F (C), so 0x2B has no grenade
  at all. The remake keeps its villager rule (item 0x3E, V beach-gunner gren).
- **The bolt's push isn't predicted.** Bolts aren't in `Fighting`, so the server's correction moves the shooter. The
  scare is set in the order of the match's updates: a lone player's frame can differ from the whole match's on its
  first frame (the test masks `scared`), so a client may roll back.
- **The bolt's wall impact** still plays the knife's sound (I: not traced).
- **The tommy gun's kill** ends the dead pose with 0x18A, 63 frames in. V: e_w38 1003-1021; also sg_e2 1194-1212. The
  pistol's (e_w39) lies all 81 frames. It isn't a hit on the body: `func_1505B9C4` ran only at 820, 850 and 869. The
  trigger was not found; e_w38 keeps 3 known events (L1: found, above).
- **The pistol head test** stays deferred: it needs the model's head part 0xA.
- `conker-server/src/token.rs`'s doc still says `"protocol":32` (E11's file).

## Around the match (2026-10-14)
**Batch E11, the server and web lane. The server seats its players as the original does: the humans in the first
slots, the bots after them team by team, as many as the original's bots screen (Deathmatch, Tank, Race) or team screen
(the War modes) allows with the humans in. A human in a team mode switches team in the match (Tab or a gamepad's
Select: the team screen, Heist's four teams included, with the first human's BALANCE); `team=T` asks for one from the
start (the lobby's Heist pick). The server's `/info` names its pack's fingerprint and who is playing: a client refuses a
server whose pack differs from its own, and the server browser and the lobby show the humans and bots, reloaded every
5 s. A pickup the mode can't give fails the server's start.** Research: the decomp (`game_20AE20.c`: the bots screen
`func_151E1214`, `D_800AB74C`; the team screen `func_151E1744`, its rows at `0x800AB710`), no oracle run. `PROTOCOL_ID`
31 → 32. No pack change (the fingerprint is computed from the files at runtime).

The original's rules (C):
- The bots screen's most players with 1-4 humans: Deathmatch 10, 8, 4, 4; Tank and Race 8, 4, 4, 4.
- The team rows (teams; under 3 humans the most players and humans a team; from 3 the same; byte +7, team 1's
  humans and size): Heist 4; 8, 2; 4, 1. Beach 2; 12, 8; 12, 8; 2. Total War and Colors 2; 12, 8; 4, 2. Raptor 2;
  12, 8; 8, 2; 2.
- Humans take slots 0..H−1 (default team: slot & (teams − 1)); the bots follow grouped by team, team 0's first.
- BALANCE: controller 1's C-left/C-right, 2 teams, under 3 humans: team 0's size + b, team 1's − b, each clamped to
  1-8 (team 1 to byte +7; the total is then not kept).
- Beach's most is 6 villagers against 2 Tediz (7v1 and 8v1 with BALANCE; 8v2 only with two human Tediz): `beach.md`
  line 33's "8v2" is wrong.

| Part | What |
|---|---|
| conker-server `main.rs` | **The roster.** `TeamRow` (the four rows), `FFA_MOST`, `bots_for`: the bots with 1-4 humans are Deathmatch 9, 6, 1, 0; Tank and Race 7, 2, 1, 0; Heist 7, 6, 1, 0; Total War and Colors 11, 10, 1, 0; Beach 7, 6, 5, 4; Raptor 7, 6, 3, 2. `--bots N` is the count asked, capped at the one-human figure (the old Beach 10, Heist/Tank/Race 7 checks are gone), spread one at a time to the team with the fewest players within the row (`TeamRow::bots`, BALANCE after). `Game::wanted`, **`trim`** (the surplus bots go, the highest slot of a team first; a joining human takes the lowest bot's slot), **`top_up`** (missing bots join the lowest free slots, team 0's first), `human_team` (the team asked for if it has room for a human, else the one with the fewest humans; none: the join is refused), `join_human`, **`reseat`** (each match start: the humans in 0..H−1, the bots by team; free for all the team is the slot; Heist's start records by the team's k-th member) in the new **`seat`** system, which also took the next-match decision from `simulate`. A join asks the side's team (the Tediz 1, Raptor's raptors; Heist none); a leaver's slot refills. `ROSTER` lines log each change. **`team_requests`**: `Teams::Switch` respawns the player on the team (the bots refill around it; a team with no human room refuses), `Teams::Balance` (the first human, 2 teams, under 3 humans, ±7). **`unknown_pickup`**: Tank's pickups must be power-up bits, Race's the canister or the missile, any other a weapon the rules know or 0x3E; else the server fails at its start, naming the placement. The pack's fingerprint in the start log |
| conker-server `token.rs` | `/info` adds `pack`, `humans` and `bots` (`Live`, set by `publish`) |
| conker-sim `net.rs` | **`Teams`** (`Switch(t)`, `Balance(b)`; client to server on `JoinChannel`), `MatchView::teams` (0 free for all, 2, Heist 4) and `balance`, **`pack_files`** and **`pack_hash`** (FNV-1a 64 over `pack.json`, the map's data files and the three sides' characters, in order) |
| conker-client `teams.rs` (new), `lib.rs` | The team screen: Tab or Select opens and closes it; the stick left/right switches team (wrapping), Q/E (C-left/C-right) move the BALANCE, A or B closes it; the player stands while it is open (`buffer_input`). A text node natively, the page's `#teams` in the browser. `team=T` (Heist 0-3, else 0-1) |
| conker-client `connection.rs`, `main.rs` | `info_pack`, `check_pack`: native and browser clients hash their own pack's files and stop on a mismatch ("this asset pack (…) is not the server's (…)"). With `map=` (no `/info`) there is no check |
| web `play.html`, `README.md` | `#teams`, the keys help; `team=`; the lobby's facts |
| web `app` | `data.ts`: `MODES[].maxBots` per human count (1-4), `players()`. `Servers.tsx`: each row's "1 human, 3 bots"; the form's bots up to the one-human cap, a line with the caps for 1-4 humans. `Lobby.tsx`: "Playing", Heist's team select (`team=`). `ui.tsx` `useRefresh` (the route's data every 5 s). `server.ts` checks `bots` against the one-human cap. No new dependency |
| `bots.sh` | The bots are the last `ROSTER` line's (slots and teams; humans now sit first); `TEAM=` for client 1 |

| Check (2026-10-14) | Result |
|---|---|
| server `bots_by_human_count` (new) | The table above for every mode. BALANCE: Total War, 1 human, 11 bots, b = 3: 8v3 (team 0's 9 clamped to 8); b = −7 with 3 bots: 1v3; Beach with two human Tediz, b = 2: 8v2 |
| server `humans_sit_before_the_bots` (new) | Deathmatch, 9 bots: the first human takes slot 0, the bots 1-9; the second slot 1, with 6 bots; a leaver's slot refills; the next match start seats the human first (teams = slots); 1 bot with 3 humans, none with 4 |
| server `beach_caps_the_tediz`, `heist_teams_records_and_outros`, `raptor_teams_and_outros`, `total_war_teams_are_the_sides` (rewritten) | Beach, 7 bots: 5 villagers then 2 Tediz; two human Tediz (6v2), a third asking plays a villager. Heist, 6 bots: teams 2, 2, 1, 1; the human takes REDS in slot 0; reseated: teams 0, 0, 1, 1, 2, 2, 3 on records 0, 8, 1, 11, 2, 14, 3; the 2nd human BLUES; from the 3rd 1 bot, then none, a 5th refused. Raptor, 7 bots: 5 cavemen and 2 raptors; a human raptor: 6 v 2 |
| server `pickups_give_what_the_mode_has` (new); sim `the_pack_hash_changes_with_any_shared_file` (new); client `the_screen_toggles_switches_and_balances`, `the_screen_lists_each_team` (new), `connection` (the pack check) | Pass |
| `bun run test` (`--no-fail-fast`) | 536 pass, 1 ignored; 2 fail in files the other lanes were editing at the time (conker-rules `combat_traces`, conker-sim `state_choice_follows_func_1504c0e8`), neither touched here |
| `bun run check`; wasm clippy (`conker-client`, `-D warnings`); `web/app` `bun run check` (typecheck included) | Pass |

| Server run (`bots.sh`) | Before | After | Oracle |
|---|---|---|---|
| Heist, `BOTS=3 TIME=5`, seeds 1, 2 | human in slot 3, the bots on teams 0-2: takes 14, 16 | the original's roster (`ROSTER join: humans [0] bots [1, 2, 3] teams [0, 1, 2, 3]`): takes 17 (to 9317), 14 (won at 7425) | hb3: 30 takes to read 9023; the ROM path with this roster 23-24 |
| Total War `TEAM=1`; Heist `TEAM=2` (120 s) | | `ROSTER switch`: the human on team 1, 2v2 (bots 1 and 3 against it and bot 2); on GREENS (15 takes) | |
| Smoke (120 s): DM 2d `BOTS=3`, Total War `BOTS=3 HUMANS=2`, Colors `BOTS=7`, Beach `BOTS=7` (twice), Tank `BOTS=3`, Raptor `BOTS=3`, Race `BOTS=3` | | Every client connected (the pack check passing) and sees every bot move; every bot moves. No fault, unknown native or panic. Rosters: Colors 4v4, Beach 6v2 (reseated at the match start), Raptor 2 cavemen v 2 raptors, Total War with 2 humans 3v2. The FAIL lines are the known ones (a bot that never hit or died in 120 s) | |

**Left.**
- **Heist's takes.** With the original's roster the server's 3 bots still take the bag about half as often as hb3
  (14-17 against 30) and below the ROM path's 23-24 with the same roster: the roster was not the server's gap. What
  else differs between the server and the ROM path is not traced.
- The team screen's up/down (the original's extra team size) is not there: `--bots` sets the count. Teams change in
  the match (the remake has no screen before it): a switch respawns the player, at no cost.
- Race's 2 humans at most is not enforced. BALANCE is tested by units only (headless clients have no pad).
- Mid-match, a join, leave or switch can leave the bots out of team order or a bot before a human until the next
  match start (`reseat`).
- `beach.md` line 33 (lab) still says 8v2.

## Bots: the follow-ups (2026-10-14)
**Batch M6b, the bots lane. Four gaps against the original, one mode each. War: Raptor: a bolt or a knife no longer
kills a raptor through its head (the original's raptor has no head part), and a caveman bot clubs a raptor from 160, as
script 0xBE has it (query 0xEF answers 0x1F for a raptor). Race: a bot's power-up press is held, as the original's,
so the bots fire their missiles and take canisters again. Tank: the bots fire on the original's rule (`func_1508D850`).
Heist: the pickup gap is the server's roster: its bots fill slots 0-2, while the original seats its humans first. Not
fixed.** Research: `../lab/research/mp-original/bots-followups-scratch/` (`tools/lldb_m6b.sh` on the private
`ConkerRecomp.m6b`; `tankfire.py`, `tankfire2.py`, the race scripts; `code/`: the decompiled natives; `b1r_players.txt`,
`hb3_players.txt`, `tank-tbl/`). The traces (b1r, tf1, tf2, L1, L2, fire1) were not kept. `PROTOCOL_ID` stays 31:
`Fighter::aim_error` is not on the wire.

| Part | What |
|---|---|
| conker-rules `game.rs` | **`first_part`**: a raptor (class 9) has no head part, so every bolt and knife on one hits for 1. V b1r: 103 of 103 hits for 1. Before, a head hit counted as `SNIPER_RAY`, and 9 of the raptor bots' 21 deaths in a probe run were these one-shot kills. **`Fighter::aim_error`** (Tank, s126+0x12A; `serde(skip)`, the server's alone), set by **`Effect::AimError`** |
| conker-rules `race.rs` | A bot's held Z counts as a press every frame (a human's must be new). C `func_150528C8`: while riding (+0x226 = 0xFF), every frame +0x8C holds 0x2000 uses the power-up held (`func_150D7790`). +0x8C is the bot's last command-0x61 press, and `func_1504C854` never decays a racer's. Before, a bot that pressed before it took a missile kept it, and with a missile in hand it took no canister |
| conker-rules `tank.rs` | **`BOT_REFIRE`** 51: a bot's fire clip lasts 51 frames, and a press inside it fires nothing. V tf2: 40 whole bot clips, all 51 frames; the 34 presses 41-42 frames in were ignored. I: why the human's (f3) lasts 34. **The pitch** of a bot with `aim_error` solves for the target 400 nearer (min 100; C `func_150ED298`'s last argument) |
| conker-bots `brain.rs` | **`tank_fire`** = `func_1508D850`, command (0xF, 0xE), which ends each of 0xB8's bot frames and was a no-op before. Band by the enemy's xz distance: >5000, >2000, else near. When the fire counter G+0x29C passes `skill::TANK_FIRE[skill][band]` (V the oracle's RAM; normal skill 420/300/80 VIs), it restarts at 0, and with the enemy in sight (G+0x16C0 bit 16 + e) the bot presses Z (command 0x61 0x2000). Each restart draws the aim error: 1 when `STEADY[skill]` < rand·100. **`World::tank`**: in Tank the idle reset of the counter is skipped (C `func_1508A6FC` tests G+0x16BC = 0xB8). **Query 0xEF** answers 0x1F for a raptor and 1 for everyone else. V b1r: model 83's objects; the cavemen answer 1 or 5. 0xBE's free-for-all melee reach (0x2BDE) is 160 against a 0x1F, else 80. The raptor's push holds a club 147 off, so before this a caveman bot never swung at a raptor in 3f's Deathmatch. Command 0x61 keeps the press in `Npc::last` |
| conker-bots `lib.rs`, `npc.rs` | Tank: Z is the script's press. Gone: the old guess (fire when ready with the lead within 5°, a −2..2° error drawn every frame). `Npc::error` (0/1) turns the turret 10° and goes to the rules as `AimError`. Race: the bot's input holds `last & Z` |

| Check (2026-10-14) | Result |
|---|---|
| rules `a_bot_holding_z_uses_each_power_up` (new), `refire_after_34` | A bot holding Z fires each missile as it takes it (3 of 3), a human only on a new press (1). A bot's press 42 frames after a shot is ignored, at 51 it fires |
| bots `trigger_natives` | 0xEF answers 1 for a squirrel, 0x1F for a raptor |
| rom `raptor_bots_play` (rewritten; seeds 1-3, 9000 frames; b1's roster: 6 cavemen with the idle human, 2 raptors) | Scores (cavemen-raptors) 17-12 at 9023, 14-20 at 8473, 14-20 at 8287. A 1000 frames: the cavemen's kills 1.75 (asserted 1.4-2.0; b1 and b1r 1.63-1.65), the raptors' 2.02 (1.4-2.4; b1r 1.65, b1 2.35), bites 8.3 (6-9; b1r 7.3). No hit on a raptor takes more than 1 |
| rom `race_bots_finish` (L1, L2, L7e, L7f) | The bots fire missiles (asserted; 1, 1, 1 and 9 in the four races). 3 bots' steady laps: route 11 832-835 (L1 837; before 946-948), route 12 934 (L2 1063-1081; before 1068-1070). 7 bots: route 11 830-842, route 12 931-934 (before 833, 935); mean finishes within 5% of L7e's and L7f's |
| rom `tank_bots_bomb_and_fight` (b1's roster, seed 1, 9000 frames) | Shots 335 (b1 321 to read 9023; asserted within 10%). Shell deaths 8 (b1 about 10; before 20; asserted 5-15). Hits of 1: 115 (b1 63). 4 plants, 21 blast deaths (b1 2 bombs, 12) |
| rom `heist_bots_fetch_and_deliver` | Passes, 33 pickups (7 bots) |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`, `-D warnings`) | 526 pass, 1 ignored (with the other lanes' work in the tree); `check` passes (fmt, clippy on all 10 packages, `web/app`); wasm clippy passes |

| Server run (`bots.sh`) | Before | After | Oracle |
|---|---|---|---|
| War: Raptor, `SIDE=cavemen BOTS=7`, 300 s (b1's roster), cavemen-raptors | 20-7 (at 8472), 20-8 (7577) | seeds 1-6: 17-11, 16-11, 18-16, 16-20 (at 9030), 19-16, 16-18 | b1 14-20 at 8510; b1r 16-16 at 9688 |
| 3f Deathmatch, the raptor pick (`CHARACTER=10 BOTS=3`, 150 s), hits on the idle raptor | 1, 2 (only 0xEF reverted) | 5 (1 kill), 20 (5 kills) | none run |
| Tank, `BOTS=7 TIME=5`, about 9000 frames, seeds 1, 2 | shell deaths 21, 17 (seeds 3, 4); 40, 45 in 600 s | shell deaths 7, 9; blast deaths 16, 16 (3, 4 plants); shell hits 94, 110 | b1: 23 shell and 34 blast deaths in 620 s (about 11 and 16 a 300 s), 63 shell hits to read 9023 |
| Heist, `BOTS=3 TIME=5`, seeds 1, 2 | The same code (the batch changes no Heist path); the task's "half the oracle's" | takes 14 (9000 frames), 16 (the match won at 7158) | hb3: 30 takes, 39 hits to read 9023 |
| Smoke: DM 2d `BOTS=3`, TW 2d `BOTS=3 HUMANS=2` (seeds 1, 2), Colors 30 `BOTS=7`, Beach 2b `BOTS=7`, 120-150 s | | Every bot moves. No fault, unknown native or panic. TW's FAIL lines are the known ones: a bot that never died in 2 minutes. DM, Colors, Beach and TW seed 1 ran before the last Tank and Raptor edits, TW seed 2 on the final build. These modes run the same code: every change is gated on Tank, Race or class 9, and with (0xF, 0xE) made an unknown native outside Tank every other ROM test still saw none | |

**Heist: why 3 bots take the bag half as often.** The roster. The server seats its bots in slots 0-2 before any human
joins, and the human takes slot 3. Heist's teams and start records follow the slot (team = slot & 3,
`heist::RECORDS`), so the bots hold teams 0-2. Bases 0 and 1 are at the map's far ends (x 7052 and −8052), base 2 is
near the middle, and so is base 3 (the human's). In the original the humans come first (hb3's idle human is slot 0),
so its 3 bots hold teams 1-3, with one far base. The ROM path (`play`: the same script and rules, 9000 frames, seeds
1 and 2) shows it:
- the human in slot 0: 24 and 23 takes, 31 and 28 hits;
- the human in slot 3: 13 and 16 takes, 15 and 16 hits. The server got 14 and 16.

**Left.**
- **Heist's roster** (not fixed). Seating humans first means renumbering the slots as humans join: the bots start
  alone, and `PlayerId::Local(slot)` is on the wire. With the original's roster the bots still take the bag about 80%
  as often (23-24 against 30; hits 28-31 against 39). Cause not found.
- **Tank's hit rate.** At b1's shot count the shells hit about 1.8× as often (115 against 63; on the server 94-110).
  95 of the 115 hits are near-band shots (under 2000) on the bot's own enemy. Yet the shell deaths are b1's or fewer,
  because the hits spread over more tanks: b1's 63 hits made about 10 kills. Next: the near solve (`func_150ED298`'s
  clamps) and the turret's lead (the remake's `d / 100`). Blast deaths in the ROM test are 21, against b1's 12
  (4 plants against 2).
- **Race route 12 with 3 bots** laps in 934, 12% faster than L2's 1063-1081 (asserted within 15%). The 7 bots' 931-934
  is L7f's pace. In L2, a bot passes at the boosts; here the convoy keeps its order and leads there. Not traced.
- **Raptor.** On the server the cavemen still score about 15% over b1 and b1r (16-19 against 14-16 a match), and the
  raptors spread wider (11-20). Not checked: a War: Raptor run with a raptor human under the original's roster. In
  these runs an idle raptor human took 12-18 hits a match before and 6-26 after, so bots hitting it "rarely" in War:
  Raptor was not reproduced. There the reach was always 160, and the club hits on raptors in the ROM path match b1r's
  (43 against 47).

## Maps: runtime segments (2026-10-14)
**Batch F4, the pack lane. Map 0x1A's nine white level materials are two computer screens (a 3-frame loop) and the
lamp fixtures of its five switching lights (lit or dark with their light). The game binds these segments every frame.
`level.gltf` now draws each one's first frame, with the lamps lit. The new `maps/<map>/segments.json` (in `Slice`)
gives the client the screens' frames and each lamp's materials. No other packed map binds a level segment at run time.
0x33's crowd now uses the same mechanism. Also from M14c: 31 textured character materials whose combiner reads no
texel now carry no texture and draw their vertex colours (Conker's head and tail, material 11, no longer navy).**
Scripts: `../lab/research/mp-original/race-effects-scratch/seg1a.py` (the rule against the dumps), `cc.py` (combiners).

| Part | What |
|---|---|
| extract-level | `Level::segments` (segment → texture id): `to_gltf` draws the materials on that segment with that texture, but keeps the unbound material as the dedup key and the name `m<n>-seg<address>`. Bound materials no longer merge with others drawing the same texture. `bind_material` moved here from conker-extract |
| extract-gbi | `Material::samples_texel`: the combiner reads TEXEL0/1 (as colour, alpha or a colour factor) in either cycle, or the list never set it. `texture_png` gives no texture otherwise, for every exporter (characters, levels, held, outros, props) |
| conker-extract `lib.rs` | `vault_ids` (the 5 ids at 0x80090324). `map_level` fills `segments`: on 0x1A, 2 → 0x60B, 3 → 0x60C, 4-8 → 0x16C; on 0x33, each crowd segment → `ids[0]` (`CROWD_PHASE` and the material rewrite are gone). `map_segments` builds `segments.json` and its frame PNGs. A lamp's segment is 4 + (light command − 8), read from the triggers in `switching` order. `race_crowd` and `map_segments` share `segment_frames`. `bind_faces` leaves textureless materials out of `faces` (176's 12) |
| conker-assets | `Segments { screens: Option<race::Crowd>, lamps: Vec<Lamp> }` and `Lamp { materials, on, off }`. `Slice::segments` comes from `maps/<map>/segments.json`, the 14th data file, so the web fetches it through `Slice::files`. `load` refuses screens with step 0 or no frames, and a lamp count other than 0 or `switching.len()`. `Crowd::invalid` is shared with `race.json`. `Panel` doc: the glTF draws `frames[0]` |
| `pack-readme.md` | `segments.json`; the `-seg` names; the crowd draws `frames[0]` |
| Pack | Re-exported. 2,043 → 2,054 files, 224,559,936 bytes. New: `maps/*/segments.json` (8; `{}` except 1a), `maps/1a/textures/0364-, 1547-, 1548-, 3197-32x64-rgba16-i.png` (1549 was there). Changed: `maps/1a/level.gltf` (9 materials textured), `maps/33/level.gltf` and `race.json` (crowd names `m53-seg06000000`, `m55-seg07000000`, `m56-seg04000000`, `m57-seg05000000`, drawing frame 0), 15 `character.gltf`s, `characters/176/character.json`, `characters/0/textures/character-06.png`. Removed by hand: the stale `characters/0/textures/character-07.png` (images shifted down by one). `README.md`. An older pack has no `segments.json`, so `Slice::load` refuses it ("missing"): re-export |

**0x1A's rule (C, V).** Scene 0x1A's case in `func_1510B9D0`'s switch calls `func_15100464`. Of the five ids at
0x80090324 (0x60B, 0x60C, 0x60D, 0x16C, 0xC7D), it binds segment 2 to `ids[(t/4) % 3]` and segment 3 to
`ids[(t/4 + 1) % 3]`. t is the u8 at 0x800DD405, +1 a frame (`func_15100340`). Segment 4 + k gets `ids[3]` (lit) while
switching light k is on and `ids[4]` (dark) while it's off, from the byte at 0x800BE500 + k. `func_15100340` toggles
that byte together with light kinds 9-13. `seg1a.py` checks this against hud1-3, bag_dl, bag_ap, bag_dl3, dm3, dm15,
cs2 and cs3: every dump's segments 2-8 match.

| Segment | Materials | Frames |
|---|---|---|
| 2, 3 (screens, MASK) | `m69`, `m71` | 1547 → 1548 → 1549, segment 3 one frame ahead; every 8 VIs |
| 4-8 (lamps of `switching` 0-4: lights 10, 11, 12, 20, 29) | 4: `m30`, `m31`; 5: `m75`; 6: `m68`; 7: `m70`; 8: `m73`, `m74` | on 0364, off 3197 |

**Sweep.** 2d, 30, 34, 3f, 2b and 24 have no case in the switch (C), and their levels draw no segment image
(`runtime_segments`). The tank's segment-8 calls are render-state lists, not textures.

**Characters ignoring their texture.** All 31 materials have the combine `fffe8f f517f8ff`. Colour is cycle 1 = 0, then
cycle 2 = (SHADE − ENV) × K5 + PRIM; alpha is SHADE × ENV. They now have no `baseColorTexture`, so they draw white ×
COLOR_0. By character, material (texture): 0: 3 (3097), 6 (3099), 8 (3098), 11 (3649). 59: 4 (7244). 83: 6 (3701),
9 (3709), 10 (3709). 88: 5 (1540), 7 (4211). 90: 2 (4397), 6 (4399). 128: 2 (4214), 4 (4211). 134: 11 (1520),
13 (3521, BLEND). 135: 11 (2648), 14 (4200). 144: 2 (4751). 150: 5 (1562), 7 (2260). 156: 2 (1363), 4 (4211). 157:
2 (4211), 7 (1356, BLEND). 165: 6 (3702), 9 (3709), 10 (3709). 176: 9 (2648), 12 (4200). 177: 5 (4545). No level,
held, outro or props material has this combine. Material indices are unchanged.

| Check (2026-10-14) | Result |
|---|---|
| `tests/rom.rs` `runtime_segments` (new) | `vault_ids`. 0x1A's `segments` cover every segment its draws read. 104 materials, 9 `-seg`, each drawing its first frame. The screens' and lamps' materials and frames, in command order. 5 PNGs. The six other maps: no segment drawn, `segments.json` empty |
| `tests/rom.rs` `race_crowd_and_effects_follow_the_oracle` | Now: `segments` = each crowd segment's frame 0 and cover every drawn segment. Still 73 materials, 54 kept apart (`m54-tex4704`). Panels `m56`/`m57`/`m53`/`m55-seg…` draw `frames[0]` |
| `tests/rom.rs` `parts_ignoring_their_texture_draw_vertex_colours` (new) | Conker's material 11: image flat 3649, combine `fffe8ff517f8ff`, no `baseColorTexture` |
| `tests/pack.rs` | 0x1A's `segments` = `map_segments`, 5 lamps, each material drawing its first frame, every frame a PNG. Every map: no `-seg` material without a texture, `segments.json` `{}` off 1a. The crowd draws `frames[0]`. Faces: every face material is textured |
| extract-gbi, conker-assets | `samples_texel` (Conker's combine vs a TEXEL0 one). The slice round trip with `segments`, the lamp count and empty-frames refusals, `{}` when empty |
| Pack (`../lab/tools/verify-pack.sh`) | "pack equal", 2,054 files |
| `bun run test`, `bun run check` | Pass (clippy `-D warnings` on extract-gbi and extract-level too) |

**Gaps (I).** 8 VIs a screen frame assumes 2 VIs a game frame (t counts frames). The characters' colour drops PRIM,
ENV and K5. Vertex colour alone is what M14c saw on Conker in a1 (V). 88, 90, 134 and 135 were not checked against the
oracle.

**Handoff (client; no wire or rules change, and no new fetch: `segments.json` is in `Slice::files`).**
- **0x1A screens:** `slice.segments.screens` (a `race::Crowd`). For each panel, set the `level.gltf` material named
  `panel.material` to `maps/1a/<frames[i]>`, with i = (VIs / `step`) % 3 on one clock (`step` 8). Keep MASK. Race's
  crowd code works unchanged if it takes the map folder as a parameter.
- **0x1A lamps:** `slice.segments.lamps[k]` follows `lights.switching[k]` (`lights.rs`'s switch k). While it's on, the
  materials in `lamps[k].materials` draw `on`; while it's off they draw `off`. No material is shared between lamps.
- **0x33:** the crowd's material names changed (`m56-seg04000000` …) and the glTF draws `frames[0]`. Nothing to change
  if the names are read from `race.json`.
- **Characters:** the 31 materials above have no base-colour texture, so don't assume one. 176's face segment 6 now
  lists only material 10.

**Client (batch F3c, conker-client `segments.rs`).** One system binds every runtime-segment material by name as the
level spawns and swaps its texture in place (alpha mode kept, the glTF texture's sampler): the panels of
`segments.screens` (or Race's `race.json` `crowd` on 0x33) on one clock, `frames[(tick × 2 / step) % len]`, and each
`segments.lamps[k]` material `on` while `lights::Flickers::switches[k]` is on, else `off` (lit until known). Tests
`the_vault_screens_loop_with_one_ahead` (1547/1548 → 1548/1549 → 1549/1547 every 4 frames) and
`the_vault_lamps_follow_their_lights`. Screens: `../lab/screens/vault/screens.png` and `screens-4.png` (reads 120
and 124, a free camera at (−350, 250, −250): the right-hand monitors step one frame). The textureless character
materials need no client change: only faces' sampler and the tank's tread width read `base_color_texture`, both
`None`-safe, and the code that sets one (team looks 119/14 and 59/2, 119's brake, the faces) only touches textured
materials. **For the pack lane:** by position the lamps do not sit by the light they follow: segment 4's fixtures
(`m30`/`m31`, about (1896, 150, 671)) are by light 11 (1887, 300, 739), 5's (`m75`) by light 10, 6's (`m68`) by 20,
7's (`m70`) by 12, and 8's by 29. `segments.lamps[k]` pairs segment 4 + (command − 8) with `switching[k]` = lights
10, 11, 12, 20, 29, so lamps 0-3 follow a light across the map. Either the original cross-wires them (C says byte k
toggles with light kind 9 + k) or the command-to-light pairing is off; worth one dump of a lamp going dark.

## Characters: the client and the lobby (2026-10-14)
**Batch M14c. Every pick is drawn, posed and heard by its class. The client loads the models of the match's allowed
picks (`MatchView.characters`) and of every player's class, one glTF at a time after the map's own. A class whose
model isn't in yet is drawn as its side's body, so no pick indexes past a list (M14b's crash is gone). The nine
mesh-only models (0, 150, 152, 159, 160, 176, 177, 178, 180) play 117's clips through the retarget. Bodies are drawn
at `characters::scale`: 0.6 for the Sergeant and the Tediz Leader, with the camera's aim raised to match, and 0.7 for
the raptor. Every model's `hidden` lists apply, and the leaders hold the bazooka their own way. The blinkers blink. In
Race any pick rides the board. `--character` takes an index or a name. The web lobby has a picker over the server's
allowed set, and the create form has the cheats.** Research: `../lab/research/mp-original/characters-scratch/m14c/`
(`run.sh` on the private `ConkerRecomp.m14c`, `scale.py`, `combine.py`, `kick.txt`). Screens:
`../lab/screens/characters/`.

| Part | What |
|---|---|
| conker-client `lib.rs` | **`PICKS`**: (folder, name) for the 22 picks. **`parse_pick`**: an index or a name, any case, for `character=`. **`map_characters(map)`** is the old list. **`characters(map)`** adds every model a pick allowed with every cheat would need, and **`character_files`** adds 0x3F's baby and body. **`body(c, characters)`**: the class's model, else `c.side`. **`Bodies`** (names, `clips_from`). **`BodyClips`** gives a mesh-only body its `clips_from`'s clips. **`camera_view`**: `head` × `scale(class)` / 0.5, except for the raptor (its camera is its own). `tick_anims`: durations by `clip(class, id)`, racers by `MatchView.race`, the kick's rate by human or bot (below). `attach`: the racer's camera cut by `MatchView.race` |
| conker-client `render.rs` | **`load_characters`**: one glTF at a time, keeping each handle (the connection waits on the own ones). Order: the map's own, then the players' bodies, then the allowed picks; a mesh-only pick comes after its `clips_from`. `Look` keeps the joints' bind translations. **`Characters::drawn`**: the class's body once loaded, else the side's; `rebuild_models` redraws a player when that changes. `add_player_models` scales by `scale(class)` (the lift too). **The retarget** (`Retarget`, `rig_ready`, `rest_retargeted`, `retarget`): a mesh-only rig gets an `AnimationTargetId` per joint (its path from `joint_00`), `AnimatedBy` and an `AnimationPlayer` on `joint_00`, and 117's graph. Before the animation systems, each joint's translation is reset to 117's bind; after them, `own bind − 117's bind` is added. So joint k sits at `key − 117's node k + own node k`, and a joint the clip doesn't key rests at its own bind. Rotations and scales are 117's keys. `pose_players` asks `clip(class, id)`. **`hide_lists`**: for every glTF with `hidden`, the primitives whose `extras.list` bit is set are hidden. This replaces `raptor::first_list_only`, which is removed |
| conker-client `faces.rs` (new) | **`Blink`** (C `func_1507E2B0`, `func_1502EEF4`): open ⌊T/2⌋ frames, T = `rand() % 140 + 10` VIs, then half, shut, half, a frame each. Each blinker (a player whose model has `faces`) gets its own copies of its eye materials (as `wear` does), and its three looks load with the glTF image's sampler. Both eyes and both segments switch together, bots too. Each slot has its own LCG, set from the match start once known, so `--wait` replays show the same blinks every run. Held at health 0 until the respawn (the local player's by its prediction). The outros' actors and 0x3F's baby are not players, so they keep the glTF's open eyes |
| conker-client `race.rs`, `held.rs`, `beach.rs`, `audio.rs`, `raptor.rs`, `connection.rs`, `smooth.rs`, `main.rs` | Race: `racer()` is gone. The board, gear, clips, held items and camera go by `MatchView.race`, so any pick rides. **`rate(clip, human)`**: the kick plays at 1.35 for a human whatever the pick, 1 for a bot (V kick7, below). Gear: helmets only on classes 0xD-0xF (`race.json` `heads`, C `func_15083384`); the other picks ride bare-headed. Beach's dress ORs the model's own `hidden`. Audio: voices and clip sounds by the body's model. The connection gate waits on `map_characters` only |
| web `app/src/data.ts`, `store.ts`, `Lobby.tsx`, `Servers.tsx`, `server.ts`, `README.md` | `PICKS` (names), `CHEATS` (code → picks), **`allowed(map, mode, cheats)`** as the server's `characters()` (0x3F with all: 0x3F_CFFA, as its test), `Settings.cheats`. Create server: the 7 codes as checkboxes, disabled where the map and mode have no select. `server.ts` accepts only distinct known codes and passes `--cheats a,b` when any are on; a "Cheats N" settings row. Lobby: a "Character" select ("The server's choice" plus the allowed picks by name), shown only where there is a select. The choice is kept in `usePlayer.character` and goes into the play URL as `character=N` only when this server allows it. README: the `character=` option |
| `../lab/screens/characters/shots.sh` (new) | Ports 5980-5988, `--cheats all`. Each shot: one offscreen client plus a headless one per further pick, joined in order onto `--spawn`s in a row facing the camera |

| Check (2026-10-14) | Result |
|---|---|
| client `bodies_by_map_and_class` | Every pick allowed on 0x2D, 0x34, 0x30, 0x1A, 0x3F and 0x33 with every cheat draws its own model. 0x2D's extra models in pick order (176, 177, 59, 152, 144, 156, 0, 150, 157, 159, 160, 178, 180); none on 0x24 and 0x2B; 0x3F's files end with 165 and 22 |
| client `a_pick_is_drawn_as_its_side_till_its_model_is_in` | The Sergeant on the Tediz side is drawn as the Tedi until 176 is in, then as 176. Conker and Gregg with no model on the list are drawn as their sides' |
| client `picks_are_the_rules_and_have_names`, `mesh_only_picks_take_their_clips_from` | The folders are conker-rules' models; `parse_pick` and `character=` (index, name, bad values). A `clips_from` body has the Tedi's clips and durations |
| client `a_blink_follows_the_oracle` | V c1's blinks start at reads 132, 141 and 199 (T 127, 13, 111): half, shut, half, open. T spans 5-74 frames |
| client `racers_show_their_clips` | `rate(KICKING, human)` 1.35, a bot's 1 |
| `bun run test` | 520 pass, 1 ignored (with the bots and pack lanes' work in the tree). Then `cargo test -p conker-client` after the last edits (the drawn-body test): 126 pass |
| `bun run check` | Stops at `cargo fmt --check` on the other lanes' in-progress files (`conker-bots/tests/rom.rs`, `conker-rules/src/game.rs`, `conker-assets/src/lib.rs`, `conker-extract`, `extract-level`); conker-client passes fmt. Its clippy (`-D warnings`, all 10 packages) passes when run alone, and so does the `web/app` check (typecheck, oxlint, oxfmt) |
| wasm clippy (`conker-client`, `-D warnings`) | Passes |

| Oracle (`ConkerRecomp.m14c`; scenario 3 = DM 0x2D, 1 bot, idle; DL 100) | Result |
|---|---|
| Scale (`scale.py` on s0, s1, s3: `CONKER_MP_CHARS` 0, 1, 3) | 128's vertices are drawn under a modelview of scale 0.5000; 176's and 177's under 0.6000 = `scale(class)` |
| Camera (the same runs, read 90) | Aim +67.5 over the squirrel and +81 over the Sergeant and the Tediz Leader. Ours is `head` × 0.75 with `head` × 1.2. The eye is +100 in all three. Ours should be too, by the code (`head` feeds only the aim and the scope's eye); not measured |
| Conker's head (`combine.py`, F2's a1) | The 54 triangles of material 11 (the head and the tail) are drawn under combine `fffe8ff517f8ff`, (shade − env) × K5 + prim, which samples no texel (F3's `cc.py` agrees). Their vertex colours average (0.93, 0.53, 0.27), orange. The pack gives material 11 texture 3649 (at +1024: a blue strip), so the client draws Conker's head navy (gap) |
| Kick rate (`race-scratch/run.sh` kick7: scenario 11, 1 bot, `CONKER_MP_CHARS=7`, `kick.txt`) | The human (pick 7) kicks (state 84) at 1.35 at reads 242-270 and 301+. 296-300 is the knock, at 1. So the rate goes by human, not by class |

| Screen (`zsh shots.sh [name]`; 0x2D unless noted) | What it shows |
|---|---|
| `lineup-a.png`, `lineup-b.png`, `lineup-c.png` (read 90) | Every pick allowed on 0x2D with every cheat (19), idle: the squirrel, Sergeant, Tedi, Tediz Leader, Wise Guy, weasel and caveman; the four racers, the villager, Conker and Neo; the villager woman, both zombies, both Greggs. Every mesh-only model stands posed (the retarget), none collapsed or stretched |
| `scale.png` | The Tedi, Sergeant, Tediz Leader and squirrel side by side: 0.6 against 0.5 |
| `bazooka.png` (B at 40, read 100) | The Sergeant, Tediz Leader and Tedi given 0x16: the leaders on 0x2C5, the Tedi on 0xBD |
| `blink.png` (read 44) | Conker's eyes shut (slot 0's first blink after the start), the squirrel's open |
| `raptor.png` (0x3F) | The raptor pick at 0.7 next to the caveman |
| `race.png`, `race-grid.png` (0x33, read 300 and 150) | Conker (WELLYTOP) on the board, bare-headed, behind it in the race camera and from the front on the grid beside a bot |
| `../lab/screens/raptor/shots.sh a`, rerun into `/tmp` (not kept) | `hide_lists` on 165 and 22 (`first_list_only` gone): the baby draws one head (nest, read 345) and the raptor carries the body (205). Both look as the committed `nest.png` and `body-side.png`, except that the raptor's and the baby's eyes are now textured |

Natively, 0x2D's 13 extra glTFs (every cheat) load within 0.45 s of connecting, so a pick shows its side's body for
under half a second.

**Gaps.**
- **Pack fix needed: Conker's (0) head and tail draw navy.** Material 11 should carry no texture (oracle above).
  Fifteen models carry such textured materials under a combine that samples no texel (0: 3, 6, 8, 11; 128: 2, 4;
  134, 135, 144, 150, 156, 157, 165, 176, 177, 59, 83, 88, 90). Of those in the shots, only 0's is visibly wrong:
  the others' textures are close to their vertex colours. 134, 135, 88 and 90 were not looked at.
- **C, not drawn:** the helmets of the racer classes 0xD-0xF outside Race. 0x3F's Deathmatch allows 0xD (pick 8),
  and EATBOX adds all the racers to every Deathmatch; C attaches the helmet in any mode. The helmet models are in
  0x33's `props.gltf`, so these picks are drawn bare-headed there.
- **I:** 176's and 177's lift is drawn × 0.6 (the lift scales with the model). Not checked against a frame.
- **I:** the held weapons stay at their size on the 0.6 picks. The leaders' bazooka looked right in `bazooka.png`.
- On the web, a cheat pick's model loads after the connection (each glTF a few MB), so it shows its side's body
  meanwhile. Not measured in a browser.
- No portraits in the lobby: the picker is names only.
- The lobby's allowed set comes from the dev server's settings, not the game server's `/info`. A server started
  outside the dev server (other cheats) is not reflected. The game server still refuses a pick it doesn't allow.

## Race: the effects (2026-10-14)
**Batch F3, the pack lane. Race's effect textures are in the pack: the countdown lamps' flare, the canister's boost
flare, the exhaust glow's four frames and the missile's trail (`hud/race_*.png`). The white panels near the grid were
0x33's crowd: five-frame cheering spectators that the game binds to segments 4-7 every frame. `level.gltf` now draws
each panel with one of its frames, the other frames are in `maps/33/textures/`, and `race.json`'s `crowd` gives each
panel's material and frame order. The client changes are a handoff (below).** Research and scripts:
`../lab/research/mp-original/race-effects-scratch/`. `segtex.py` lists the textures set through segments 2-7.
`fxdl.py` dumps one texture's draws, with state, vertices and matrix. `cc.py` decodes combiners. `glow.py` and
`lamps.py` list the quads. `sheet.py` makes contact sheets. These read M13's dumps in `/tmp/m13` (cd, it, w1, w3, w3b,
w3c), so no new oracle run was needed. `../lab/hud/texfind` gained `raw DIR ids…`.

| Part | What |
|---|---|
| conker-extract `hud.rs` | `TEXTURES` + 7 (43). `race_lamp` 0x235, I8 64×64. `race_boost` 0xE21, I8 64×64. `race_exhaust_0..3` 0x923-0x926, RGBA32 32×32. `race_trail` 0xB39, I8 128×1. All load with dxt 0, so odd rows are swapped as for the others. M13 read the trail as "I8 16×1". Its tile is I8 256×1, but `G_TEXTURE` scales by 0.5 and s reaches 8100, so only the blob's 128 bytes are sampled (a ramp 0 → 0xFF → 0). The second half of the 256-byte load is RAM garbage (V w1 DL 222) |
| conker-extract `lib.rs` | **`crowd_frames(rom)`**: for segments 4-7, the 5 ids in the order shown, read from the two tables addressed at 0x80088900 (`.game` data). **`map_level`**: on 0x33, binds each segment to its frame at `CROWD_PHASE` 1 (`bind_material`, as the characters' segments; F4: replaced by `Level::segments` at frame 0, see "Maps: runtime segments"). **`race_crowd(rom, textures)`**: `race.json`'s `Crowd` (each panel's material name, read from the produced glTF, and its 5 frame paths) and the 10 frame PNGs. The PNGs are named by extract-level's `texture_png`, as the level's. **`race_mode`** fills `crowd`. **`pack`** adds the frames to 0x33's level images |
| extract-level | `texture_png` is public (docs only otherwise) |
| conker-assets `race.rs` | `Race::crowd: Option<Crowd>` (serde default, skipped when absent, so older packs load). `Crowd { step, panels }`, `Panel { material, frames }`. `load` refuses `step` 0 or a panel with no frames. The round trip covers both cases |
| `pack-readme.md` | The 7 HUD PNGs, 0x33's extra frames, `race.json`'s `crowd` |
| Pack | Re-exported (`bun run pack -- --force`): 2,027 → 2,043 files, 224,505,862 → 224,544,672 bytes. New: `hud/race_lamp.png`, `race_boost.png`, `race_exhaust_0..3.png`, `race_trail.png`; `maps/33/textures/4697…4702, 4706, 4708, 4710-64x32-rgba16-i.png` (9; 4704 was there). Changed: `maps/33/level.gltf` (materials 53, 55, 56, 57 textured), `maps/33/race.json` (`crowd`), `hud/hud.json` (7 textures), `README.md` |

**The crowd (C, V).** `func_1510B9D0` switches on the scene (word 0x800BE9F0 − 0x13, table 0x800A2C2C). Scene 0x33 calls
`func_150D765C`. That function binds segment s = 2…7 to `func_1510D0EC(table[(k + 13 (s − 2)) % 5])`. Segments 2-5 use
table A (0x80090230) and 6-7 use table B (0x80090244). k is the u16 at 0x800D9910 >> 8. `func_150D7630` adds VIs × 64
to it every frame (2 VIs in Race), so the frame changes every 4 VIs, 15 times a second. The u16 wraps every 1,024 VIs,
and 256 ≢ 0 (mod 5), so the cycle skips one frame then. The counter's phase is free (I).

| | Ids | Level materials |
|---|---|---|
| A (segments 2-5) | 0x125E, 0x1260, 0x1262, 0x1264, 0x1266 (4702-4710 even) | seg 4: 56, seg 5: 57 |
| B (segments 6-7) | 0x1259-0x125D (4697-4701) | seg 6: 53 (the most quads), seg 7: 55 |

Material 54 is the static 0x1260 panel, A's second frame. Binding at k = 0 would put segment 4 on 0x1260 too, and the
writer would merge 56 into 54. Phase 1 keeps all 73 materials and gives the names `m53-tex4700`, `m55-tex4698`,
`m56-tex4706`, `m57-tex4702` (F4: superseded; bound segments no longer merge, so the names are `m<n>-seg<address>` and
the glTF draws frame 0). Segments 2 and 3 are bound too, but no 0x33 level material draws them (V). The panels
are RGBA16 64×32, BLEND, shown in every Race frame seen (never hidden). They show two cheering spectators.

**What the original's DLs show** (V; all world-space quads, the matrix before them the identity; render mode
0x00504350: blended, z compare, no z write):

| Effect | Texture | Colour × alpha | Quad (world units) |
|---|---|---|---|
| Countdown lamps (cd 142-285, it 206-270) | `race_lamp` | env RGB: red (255, 0, 0) in stages 1-2, green from stage 3 (197); alpha = tex α × env α | Squares 845-857 wide, centred about (274, 1616, −7537), (282, 1489, −7538), (290, 1365, −7545), top to bottom. They appear one per stage (142, 172, 197) and turn with the camera. Their centres drift toward it (I). **Env alpha is 0 in every dump up to the go** (142-202, 16 dumps). After the go the lower two flicker 0x11-0x84 (law I) and the top one stays 0. None after about 270 (it 285, cd 600). So the red lamps never show in the original |
| Boost flare (w3b/w3c 480-510) | `race_boost` | white; alpha = tex α × 0x6B-0x71 | 348-399 × 74-76, centred on the racer: −6…19 ahead, 24-27 to the side, 33-90 up (M13) |
| Exhaust glow | `race_exhaust_0..3` | tex RGB; alpha = tex α × env α 0xE1-0xFD | Per racer, one frame per game frame from the four. The frame looks random (V runs such as 924, 923, 923, 925 …, both views of w1 equal; I). 84-91 × 78-85 while boosting: 7-35 behind, 22-25 to the side, 42-99 up (w3c). 17-178 wide in cd/w3 without a boost (size law not decoded). `.game` lists 0x923-0x926 next to 0xBA6 (the traps' puff) at 0x8009016C |
| Missile trail (w1 218-240) | `race_trail` | white; alpha = tex α × vertex α | A ribbon of 7 points about 200 apart (a missile's frames), 58 wide. u runs 0 → 1 across the width. Vertex alpha falls 255, 212, 169, 127, 84, 42, 0 from the missile back. A second, narrower 3-column strip follows it with env α 0xFF (not decoded) |

| Check (2026-10-14) | Result |
|---|---|
| `texfind` on M13's dumps | 0x801C02B0 = blob 0x235, 0x801B7770 = 0xE21, 10 RAM copies of the glow = 0x923-0x926, 0x801AB800 = 0xB39, segments 4-7 in cd 142/200 and w1 218 = 0x1259-0x1266 (12 matches), each byte for byte |
| Crowd rule | k 60 (cd 142), 89 (cd 200), 98 (w1 218): the rule gives the four ids drawn |
| `tests/rom.rs` `race_crowd_and_effects_follow_the_oracle` | (F4 changed the names and frame, see "Maps: runtime segments".) Tables, order and the three frames above. `map_level(0x33)` has no segment image left. 73 materials, 53-57 = `m53-tex4700`, `m54-tex4704`, `m55-tex4698`, `m56-tex4706`, `m57-tex4702`. `race_mode`'s crowd: step 4, panel order, frames, `frames[1]` = the bound texture. 10 PNGs. Blob sizes, and the trail rising to 0xFF at byte 64 and back to 0 |
| `tests/pack.rs` | `race.json` = `race_mode` (now with the crowd). Each panel's material in `level.gltf` draws `frames[1]` (F4: `frames[0]`). Every frame is a PNG. `hud.json` sizes of the new textures (all HUD PNG sizes are checked already) |
| conker-assets | 14 pass (round trip with and without `crowd`; the refusal) |
| Pack (`../lab/tools/verify-pack.sh`) | "pack equal", 2,043 files |
| `bun run test` | 520 pass, 1 ignored |
| `bun run check` | Fails only on the other lanes' in-progress files: `cargo fmt --check` (conker-client `beach.rs`, `faces.rs`, `lib.rs`, `render.rs`) and clippy (conker-client `lib.rs` `attach`: too many arguments, a complex type). Clippy `-D warnings` on conker-extract, conker-assets and extract-level, and the `web/app` check, pass |

**Gaps.**
- **I:** the lamps' flicker law after the go, why the top lamp and every red stage stay at alpha 0, and the drift of
  the quads' centres. The exhaust glow's size law without a boost, and its frame choice. The trail's second strip.
  Where in the code each effect's alpha comes from was not read (`func_15065A5C`'s 0x235 is a sound id, not this
  texture).
- **Map 0x1A has the same gap** (done in F4, "Maps: runtime segments"): 9 level materials on segments 2-8 (`m30`/`m31` seg 4, `m68` seg 6, `m69` seg 2,
  `m70` seg 7, `m71` seg 3, `m73`/`m74` seg 8, `m75` seg 5). There the switch calls `func_15100464`. Segments 2 and 3
  cycle 3 ids, `table[(t + i) % 3]` with t = byte 0x800DD405 >> 2 and the table at 0x80090324 (0x60B-0x60D).
  Segments 4-8 take `table[3 + byte 0x800BE500 + k]` (C, not seen in a frame). The tank's segment-8 render-state
  lists are another matter.
- `../lab/research/mp-original/race.md` §M13 still says "find the missile trail in w1b/w5"; this section has it.

**Handoff (client: conker-client `race.rs`; nothing changes in the wire or the rules).** The PNGs load through Bevy's
asset server like the others, and the web build fetches no new JSON.
- **Crowd:** `Race::crowd` (`Option`). For each `Panel`, the `level.gltf` primitives whose `GltfMaterialName` is
  `panel.material` (one material per panel, used by nothing else) get that material's `base_color_texture` set to
  `maps/33/<frames[i]>` with i = (VIs / `step`) % `frames.len()`, on one clock shared by all four panels (VIs =
  elapsed seconds × 60: 15 frames a second; the phase is free). Mutate the material or use one copy per panel, as
  `beach.rs` `scroll_water` does. Keep its BLEND.
- **Boost flare:** replace `sprites.streak` (`hud/strike_streak.png`) in `flares` with `hud/race_boost.png`. Keep the
  size, place and alpha 0x6E: white × texture alpha, blended, no depth write.
- **Exhaust glow (new):** a camera-facing quad per racer, blended, depth-tested, no depth write. Colour is
  `hud/race_exhaust_{f}.png` with f random 0-3 each game frame per racer, alpha × about 0.96 (0xF5). Use about 88 × 82
  at 25 behind, 24 to the side, 60 up while boosting. Without a boost the size varies 17-178 (law not decoded: a
  steady 88 is the I choice).
- **Missile trail (new):** a ribbon through each missile's last 7 positions, 58 wide, facing the camera (I), with u 0 → 1
  across its width on `hud/race_trail.png` (clamp). White; alpha = texture × (1 − n/6) at point n from the missile.
  Blended, no depth write.
- **Countdown lamps:** to match the original, draw nothing before the go. After the go, draw the lower two lamps
  (centres above, about 850 square, camera-facing) green × `hud/race_lamp.png` alpha × a flicker of 0.07-0.5 for about
  70 frames (I). Skipping them entirely is also close to the original.

**Client (batch F3c, conker-client `race.rs`, `segments.rs`).** **Crowd** (`segments.rs`, shared with 0x1A's
screens, see Maps: runtime segments): each panel's level material, found by name, gets `frames[(VIs / step) % 5]` as
its texture in place (BLEND kept, the glTF texture's wrapping sampler), VIs = match tick × 2, one clock for all (the
u16's skipped frame every 1,024 VIs is not drawn). **Flare:** `race_boost.png` in place of
the streak. **Exhaust:** an 88 × 82 camera-facing quad per live racer, frame = a hash of (tick, slot) (I), alpha 0xF5.
Both sit 24 to the racer's right, −x in its frame (V: M13's w3c 480/490/500 put the flare 24-26 and the glow 22-25 to
player 0's right, the glow 16-31 behind). **Lamps:** the lower two, green, 850 square, from `RaceView::go` for 70
frames at alpha 0x11-0x84 (a hash, I). **Trail:** `ribbon` through each missile's last 7 places (reset on a jump over
600), 58 wide facing the camera, alpha 1 − n/6. All use `traps::material` (Blend: an I8 PNG's RGB is its intensity,
so the edges are greyer than white × alpha; hud.rs loads these PNGs as sRGB first and Bevy 0.19 keeps one handle per
path). Checks: client `race` and `segments` tests (the crowd's clock, exhaust frames, the lamps' window and range, the
trail's alphas, width and u); `bun run test` 527 pass, 1 ignored; wasm clippy and `bun run check` pass. Screens
(`../lab/screens/race/shots.sh`, new `lamps`, `lamps-side`, `trail`): `grid.png` (150: the crowd, the glows),
`lamps.png` (220: green on the post over the arch), `lamps-side.png` (free camera), `boost.png` (225), `missile.png`
(238), `trail.png` (240). No oracle frame renders exist (M13's `hw_*/hud.png` are HUD only): compared by the dumps'
numbers. **Gaps (I):** seen from the side, the lamp quads float about 380 in front of the post (the original's drift
toward the camera, not decoded); the glow's size without a boost; the trail's second strip; the other screens
predate the glow.

## Character materials' alpha (2026-10-14)
**Batch F2, the pack lane. Every character glTF material now says how the original draws it: `alphaMode` `BLEND`
(translucent), `MASK` with `alphaCutoff` 0.5 (alpha-tested) or `OPAQUE`, and `doubleSided` from the culling at the
draw. Conker's and Neo's eyes blend. The rule is the map exporter's, unchanged. The root cause was elsewhere: the
characters set most render modes through segment 8, which the character walker never resolved. Colors cs5's lit tank
is packed as its own folder, `characters/40-7-9`, so the client needs no change.** Research and scripts:
`../lab/research/mp-original/alpha-scratch/` (`rm.py`: the render state at each texture's draws in a DL dump;
`check.py`: a packed glTF against a frame; runs a1, r1, k1 on a private `ConkerRecomp.f2`, since deleted).

| Part | What |
|---|---|
| extract-gbi | **`Material::alpha_mode()`**: extract-level's rule moved here as is (DOS `decode_other_mode`, othermode low word). `BLEND` if FORCE_BL (0x4000) or z-mode XLU (bits 10-11 = 2); `MASK` if alpha compare (bits 0-1) or CVG_X_ALPHA (0x1000); else `OPAQUE`. **`walk_from(start, geometry_mode, resolve)`**: `walk` from an inherited geometry mode (`walk` = from 0) |
| extract-level | Calls `m.alpha_mode()`; its own copy is gone (the unit test stays) |
| extract-character `model.rs` | **`parse_model(model, game_data)`**: segment 8 resolves to the renderer's render-state lists in `.game` data (`RENDER_STATES` 0x80083140, as `held.rs` already did for the attachments). The lists start from G_CULL_BACK (`INHERITED`). Callers pass `rom.game_data()` |
| extract-character `gltf.rs` | `alphaMode` on every material (`alphaCutoff` 0.5 when `MASK`), as the level writer does. `doubleSided` unchanged: G_CULL_BACK off at the draw |
| conker-extract `outro.rs` | An actor whose script has op 9 (obj+0x6C+c = b + 10: face segment 6 + c from texture-table entry b) gets the folder `<model>-<segment>-<entry>`. **`characters`** returns `(folder, Character)` and binds that segment to that entry's texture. Its op is at t 1: the folder draws it from t 0, a tick early |
| conker-extract `lib.rs` | The pack's outro characters carry their folder names (manifest, glTF path). `textures` passes `game_data` too |
| `pack-readme.md` | `characters/40-7-9/`; what `alphaMode`/`doubleSided` mean in `character.gltf` |
| Pack | Re-exported (`bun run pack -- --force`): 2,009 → 2,027 files, 224,505,862 bytes. New: `characters/40-7-9/` (glTF, bin, 16 PNGs). Changed: all 30 character glTFs (`alphaMode` everywhere, `extras.otherMode` now the real one), 152's and 157's `.bin`, `held.gltf`, `projectiles.gltf`, `maps/30/outros.json` (cs5's actor 4 → `40-7-9`), `pack.json`, `README.md`. No PNG changed |

**Why the extras were wrong.** The characters' lists set their render mode with `DE 080000xx` calls into a table of
24 two-command lists, each an `EF` (both othermode words) and a `DF`. Segment 8 is 0x80083140 at every character
draw in a1, r1, cs5b, c1a, c1b and x1. The walker left those calls unresolved. Those draws read othermode 0, and a
list's earlier `EF` (an XLU eye or cloth) carried over onto the later draws. So `extras.otherMode` called many opaque
materials XLU. For example, 157's 4210, 1358 and 1355 read 0x04D049D8, and x1 draws them at 0x04D12078. The 0x80083140
table's slots by offset & 0x30: +0x00 0x04D12078, +0x10 0x04D13078 (CVG_X_ALPHA: `MASK`), +0x20 0x04D12D58 (decal),
+0x30 0x04D12278, under six high words. The fog variants (0x800838C0, 0x80083EC0, seen in t1d) classify the same.
Resolving segment 8 also merges materials that differed only by the unresolved call: 152 has 8 → 6 materials, 157
has 16 → 15 (list 0: 15 → 14 primitives), and `held.gltf` has 71 → 68 (its bank-01 objects 0x2F and 0x6E, still
opaque and culled).

**Culling.** The renderer leaves G_CULL_BACK on: the geometry mode is 0x620405 at every draw of 0, 150, 117 and 90,
whose lists never set it (V a1, cs5b). An explicit clear still draws both sides: 0x620005 at 169's cloth, 40's 4364,
4369 and 4689, and 128's 4212 and 807 (V). The glTFs of 0, 90, 134, 150 and 152 said `doubleSided` true. Now 61
materials are false: all 13 of 0, 9 of 90, 14 of 134, 19 of 150 and 6 of 152 (134 and 152 not seen in a frame: I).

| Model | → `BLEND` (material: texture) | → `MASK` |
|---|---|---|
| 0 Conker | 12: 1937 (the eyes) | |
| 22 Raptor's body | | 5: 4725, 12: 4738 |
| 40 tank (cs5) | 5: 4216 (the lights, segments 6/7) | 15: 4364 |
| 83 raptor | | 7: 3689, 8: 3709, 15: 3696 |
| 88 | 14: 807 | |
| 90 | | 8: 4545 |
| 119 tank (Tank) | 11: 4355, 12: 4368 | 14: 4364 |
| 128 squirrel | 12: 807 | |
| 134 Don Weaso | 13: 3521 | |
| 135 | 15: 807, 17: 808 | |
| 136 caveman | | 3: 4730 (an eye), 4: 4725, 6: 4725, 8: 4738 |
| 138 mounted gun | 14: 992 | 15: 988 (the belt) |
| 144 racer | 11: untextured | 5: 4725, 9: 4725 |
| 150 Neo | 15: 1937 (the eyes), 16: 3763 | |
| 152 | 5: 7256 | |
| 156 villager | 10: 1359 | |
| 157 villager | 7: 1356, 10: 4211 | 13: 1360 |
| 159 | | 8: 2769, 12: 7761, 13: 7754, 14: 7753 |
| 160 | | 5: 7759, 12: 2769, 14: 7748, 15: 960, 16: 960, 20: 7749, 21: 7750, 23: 7761, 24: 7754, 25: 7753 |
| 165 baby | | 7: 3689, 8: 3709 |
| 168 green flag | 2-6: 2291, 2292, 2287, 2288, 2289 (the cloth) | 1: 2290 |
| 169 red flag | 0-5: 2293, 2294, 2297, 2296, 2295, 2298 (the cloth) | 6: 1637 |
| 176 Sergeant | 13: 807, 16: 808 | |
| 177 | | 4: 4545 |
| 178 | | 10: 1698 |
| 180 | | 0: 4092, 1: 1700, 3: 4093, 4: 4095, 5: 1699, 13: 1699 |

That is 72 materials: 30 `BLEND` and 42 `MASK`. Every other material stays `OPAQUE`. 59, 87, 117 and 171 have none.
The indices are the new glTFs' (152's and 157's shifted with the merges). `40-7-9` is 40 with one more
material: 5 is 4368 `BLEND` (segment 7), 6 is 4216 `BLEND` (segment 6), and the rest move up one.

| Check (2026-10-14) | Result |
|---|---|
| Oracle, `check.py` (per texture drawn under 0x80083140: the set of (alpha by the rule, both sides) at its draws = the packed materials' of that texture) | **a1** (new: scenario 3, Conker and Neo humans, 2 bots, DL 100/101): 150 12/13 equal (1937 and 3763 `BLEND`; the odd one is 1946, its shut eye, which the glTF draws open); 117 4/4; Conker's own 7 equal (the rest are Neo's shared textures). **r1** (new: scenario 2, War: Raptor, 3 bots, DL 101): 136 3/4 (4725 and 4738 both `MASK` and `OPAQUE` as packed; 4732, the half eye, draws `MASK` and `OPAQUE` like its two 4730 eye materials). **cs5b** (DL 600): 90 7/7 (4545 `MASK`); 40-7-9 16/16, one tank's segment 7 on 4368 (`BLEND`, culled), the other's on 4216. **c1b** DL 420: 169 8/8 (cloth `BLEND` both sides, 1637 `MASK` both sides, pole opaque). **c1a** DL 476: 138 16/16, 128 11/11. **x1** DL 1200: 157 6/7 (4201: a shut eye). **k1** (new: scenario 1, Tank, 3 bots, DL 100/101): 119 14/14 (4355 and 4368, the lights, `BLEND`; 4364 `MASK` both sides) |
| `tests/rom.rs` `character_materials_follow_the_oracle` | 14 (model, texture) rows of those frames (0's and 150's eyes `BLEND`, 90's 4545, 40's 4364, 136's eyes and 4725, 169's cloth and 1637, 138's belt, 157's 1356...), `alphaCutoff` 0.5 exactly on `MASK`. The outro tanks: `40` and `40-7-9`, 4368 only on the latter, `BLEND` |
| `tests/rom.rs` (updated) | The outros' tests take `(folder, Character)`. Colors' characters are 135, 40, 40-7-9, 88, 90. `villager_looks_hide_their_lists`: 157's list 0 has 14 primitives (was 15), and its looks draw 16 |
| `tests/pack.rs` | 0's 1937 is `BLEND` and culled. `40-7-9` draws 4368 `BLEND` and is in `pack.json`. 40 has no 4368 |
| Client (read, not edited) | Bevy's glTF loader maps `alphaMode`/`alphaCutoff` to `AlphaMode::Blend`/`Mask(0.5)`, and `doubleSided` to `cull_mode: None`. Nothing overrides them. `render.rs` `wear()` (team looks, outro teams), `tank.rs` (brake lights, treads) and `guns.rs` (the belt, now `MASK`) each clone the loaded material and change only the texture or UV transform. The outro loads `character_gltf(&a.character)` for any folder, and no client code keys on "40" |
| `../lab/tools/verify-pack.sh` | "pack equal", 2,027 files |
| `bun run test`, `bun run check` | 515 pass, 1 ignored; check passes (fmt, clippy `-D warnings`, `web/app`), with the other lanes' work in the tree. Also clippy `-D warnings` and the tests of extract-gbi and extract-level (not in either script): pass |

**Gaps.**
- **V, not packed:** some frames draw an object through a translucent table, where every slot is XLU: 0x80082FC0,
  or its fog variants 0x80083D40 and 0x80083740. Colors' flag 169 uses one in c1a DL 430-466 and c1b DL 428-434,
  where the whole flag, pole included, is translucent. In c1b DL 420 the flag uses the opaque table, which is what's
  packed. When the renderer switches (a fade?) is not traced. It's client work if wanted: a per-entity copy with
  `AlphaMode::Blend`.
- **V:** 0, 40, 90, 117, 119, 128, 136, 138, 150, 157 and 169 were seen in a frame. **I:** the rest (22, 83, 88,
  134, 135, 144, 152, 156, 159, 160, 165, 176, 177, 178 and 180) follow the same tables but weren't seen.
- **I:** the lit tank shows its lights from t 0, not t 1. Op 9 with a code under 10 (an eye command) isn't used by
  these outros and isn't handled. An outro using one fails the export.
- `../lab/research/mp-original/faces.md` §1 says "the other eye materials are opaque". That was read from the old
  extras. The caveman's (136) two eye materials are 1 `OPAQUE` and 3 `MASK` (V r1: 4732 drawn both ways). 144's and
  22's are opaque. The Faces section's alpha gap and the M14c handoff's "Alpha" line are done by the glTF: the blink
  swap keeps the material's alpha mode, as `wear()` does.
- The level writer's own segment-8 calls (the maps' `stateCall`s) stay unresolved. Their tables differ by object
  kind (not checked here, not in this lane's scope).

## Race: the client (2026-10-14)
**Batch M13. Race is drawn and heard. `--mode race` on 0x33 (either route, any laps) shows the racers on their
boards with their helmets, the shown barriers, the power-ups and the missiles, from the race camera. The human's
board is predicted. The HUD is the original's place, lap, clocks, items and overview map, and the results are the
STATS in the level. The hums, the countdown, the missiles, the walls, the kicks and the deaths play; there is no
music.** Research: `../lab/research/mp-original/race-client-scratch/` (`hud/`: the HUD, the results, the map;
`cam/`: the camera, the clip rates, the effects; runs cd, res, it, idle, w1, w2b, w3b/c, c_fwd, c_a, c_steer, F1 on
a private `ConkerRecomp.m13`).

| Part | What |
|---|---|
| conker-client `race.rs` (new) | **`Course`:** `race.json` and the nav graph. The route the match names gives the prediction's track; its hidden barriers move 20000 up in the client's collision, as on the server, and are not drawn. **Racers:** the human (0xB) is 136 and the bots (0xC-0xF) are 144. Each gets the board (`props.gltf`, joint 23) and its class's helmet (joint 12). **Clips:** state 5 rides; state 84 shows the kick (0x354) while the kick runs, else the knock (0x54); state 112 shows the death (0x170). The human's kick plays at 1.35 and every other clip at 1 (C, V anim1/2). **Death:** the racer, board and helmet are hidden from the death to the respawn (V idle 377-463). The blast, 55 up, is `beach::burst`: 10 puffs within 300 and 9 chunks (I). **Boost:** a flare while the canister's boost runs (I below). **Power-ups** (placements 4-10) spin 3.03° a frame. Taken, they shrink over 4 frames; back, they grow with the overshoot (V L1 541-557). **Missiles:** `projectiles.gltf` scene 4 at 1.0, the nose along the way. **Camera:** `tank::behind` with the fit below; on the grid (before the racer's go) 500 back, 180 up, looking at 120 (V c_fwd). It goes through walls (C `func_15123A54`: a board's camera casts no wall ray) and cuts at a respawn. **`warm_up`:** nothing is predicted before the match starts, as on the server (the board drifted off its grid record) |
| conker-client `lib.rs` | The human's board is predicted (conker-rules' `race_player`), corrected as on foot, and held through the warm-up (the countdown freezes it). The race camera, the clips and the rates |
| conker-client `hud.rs` | **`RaceHud`:** the place and its suffix, centred on (146, 5); "LAP l/n", centred on (31, 19); the race clock, `func_150916B4` at (146, 204). Those three show while laps ≥ lap + 1. The last lap shows at (25, 30) once it is > 0 (C; the first crossing counts, as the original's 0:00:93 at it 229). **Items:** one icon per round, 18 px leftward from (260, 5): the missile's `atlas` cell and the canister's `atlas2` (V it). **Map:** `race_map_*` 64×64 at (226, 13), × (255, 255, 255, 0x80). Dots are 2×2 at `race_dot` (the quarter-pixel law with its truncations): the own green, the others in play red. The map is skipped when the viewer is out. No kills, rank or radar. **Results:** from the results' frame, the phase-6 STATS in the level, with everyone on the autopilot. That is the header at (116, 4); a row per human (label at (72, 70 + 18k), `"       : %s"` centred on 88, the total at 138, "(" 168, the best at 188, ")" 214); white with a black shadow; no column icons; alpha 0x10 then +8 a frame. A total exists only for a human that finished its laps; a forced finish shows "--: --: --" (C). A human out (result 2) gets Deathmatch's STATS |
| conker-client `audio.rs` | **Hums** (`race_hum` 0-2) per racer: hum 0 under 5 racers, hum 2 under 7 or for the listener, hum 1 always. Each volume moves ±512 a frame toward its law of the speed state (> 6, > 3) and the height over the floor. The **countdown** (`race_countdown` 1-3) plays flat at each stage. **Missiles:** each is tracked in order. Its launch (`Shot` 51) plays at the nearest racer, and its flight (`race_missile`) follows it from its 15th frame. A missile gone near a racer knocked up that frame plays its hit (`Hit` 51); otherwise a knock plays the nearest kicker's `race_kick`. A **canister** used plays `Shot` 49. A health drop plays `race_wall`, or `race_death` (kind 0xE or not) at 0 hp |
| conker-client `tank.rs`, `beach.rs` | `behind` takes `Option<&CollisionWorld>` (Race passes none). `Sprites::quad` and `streak` are visible to the crate |

| Check (2026-10-14) | Result |
|---|---|
| client `race` (3) | The clips by state and kick, the rates. The power-ups' shrink, overshoot and spin (V L1). The missile's nose |
| client `hud` `race_matches_the_originals_draws` | V hud-b2: the place, "LAP 1/1", "0:00:00" and the map at (226, 13). V it: missile icons at 260, 242, 224. The clock past 10 minutes from x 125. The last lap from (9, 30). V res 320: the STATS header, the label, every glyph's x (88 … 214) and the suffix at (100, 68) |
| client `audio` `race_plays_its_events` | The hums from frame 1. Beeps at 10, 20 and 30. The launch at 31, the flight from 46, the hit at 50, the kick at 55, the scrape at 56, the death at 58. The hum law |
| Prediction (native, idle human, route 11, 1,321 frames against the server's) | After the start, within 0.5 through walls, jumps, the kick and three deaths with their respawns. There are rollbacks (the race fields differ) but no snap. Before `warm_up`, frames 44-179 drifted |
| Sounds (`RUST_LOG=conker_client::audio=debug`) | The hums, the beeps at reads 145/176/201 (oracle 142/172/197), the kick, the deaths (1224, 694/695, 690), the canister and the pickups. A missile's launch, flight and hit. No music |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`) | 514 pass, 1 ignored; one fails outside this lane (conker-extract `villager_looks_hide_their_lists`: 157's first list has 14 primitives where the test wants 15). Clippy `-D warnings` on all crates, wasm clippy and the `web/app` check pass. `cargo fmt --check` fails only on conker-extract's `tests/pack.rs` and `tests/rom.rs` (another lane's edits) |

**Screens** (`../lab/screens/race/shots.sh`, ports 5980-5989; seed 1, an idle human). Route 11, 1 lap, 1 bot:
`grid.png` (150: the countdown, 2nd, the map), `racing-11.png` (300), `death.png` (359: the blast, 4 frames after
the death), `respawn.png` (455), `results.png` and `results-late.png` (1300, 1600: "P1: 2nd --: --: -- (0:00:96)", a
forced finish). Route 12, 2 laps, 3 bots: `kick.png` (214: B at the go), `racing-12.png` (300), `grid-side.png` (230,
a free camera: a bot in its helmet). Route 11, 3 bots: `hud-items.png` (100: two missiles held), `missile.png` (238:
one in flight), `missile-hit.png` (Z at 345; at 361, a free camera: slot 1 knocked up by the missile at about 358), `boost.png`
(225: the flare, two canisters).

**Gaps.**
- **Camera (V/C, `cam/`):** `Behind` is a fit: at rest 280 back, 198 up, looking at 195; at 170 and over 426, 232,
  195; share 0.13; follow 0.045 + 0.25 · share, at least 0.1°. Residuals: back ±3; up +30 mean, ±160 in jumps; look
  up to −216 in jumps. It misses the board's follow camera (mode 8, `func_150D1530`):
  - the FOV add (10 + 24.6 · s horizontally, +30 on the grid);
  - the boost's distance (426 + 2 · (v − 170): 594 at 254);
  - the vertical springs;
  - the hand-over after the go (the yaw frozen about 100 frames);
  - the death's look-down (123 → 2.6 over 87 frames);
  - the finish cameras (a fixed eye at the finish, the sweep from 321; V F1).
  `conker_camera::CameraState` in mode 8 with the board flag would be closer.
- **Pack (none packed):** these textures are not in the pack, so they have stand-ins or nothing (packed now: Race: the
  effects, above; the trail is I8 128×1):
  - the countdown's lamp flare (I8 64×64 at DRAM 0x801C02B0: three world quads, red, then green at go; nothing drawn);
  - the boost flare (I8 64×64 `801b7770`, ≈370 × 75 at alpha 0x6E: the strike's streak stands in, facing the camera, I);
  - the riding exhaust glow (RGBA32 32×32 per racer; nothing);
  - the missile's trail (I8 16×1 `801ab800`, a 7-point ribbon 58 wide fading linearly; nothing).
- **I:** the death's blast is Beach's puffs and chunks. The original draws a smoke shell (1.94 → 5.83), a fire shell
  and 9 textured debris for about 48 frames. The hums' pitch is fixed at their start.
- **Wire:** `MatchView` has no total. The client keeps whether each human finished its laps and shows `time − 2`, so
  a client that joins after the finish shows "--".
- RESTART/QUIT act once the match ends. In the original, A on RESTART restarts during the 40 s. The original then
  goes to the MULTI menu, not STATS (V res 1488+).
- **I:** the board leans only as the riding clip and joint 23 pose it; no lean or roll law was found (race.md,
  `cam/`). The original shows no wrong-way or out message (HUD flags 0x1096).
- **Picks (M14c):** the board, helmet, clips and camera key on the class being 0xB-0xF, and the 1.35 kick on 0xB.
  Race's picks (0xC-0xF) are inside that range, but a human who picks one kicks at 1, and a cheat's pick outside it
  would ride with none of these.
- The white panels near the grid are 0x33's level materials 53-57 on the runtime segments 4-7 (BLEND), drawn white
  (pack lane). (Done: the crowd, Race: the effects, above.)
- The missiles' sounds match them in order within 8 frames' flight. A missile snapped far onto the track reads as a
  new one: a second launch, seen once, from a dying human.

## Faces: the pack (2026-10-14)
**The grey eyes and mouths are textured. The face segments are 6/7 (the eyes, every model with eyes) and 0x0A/0x0B
(Conker's and Neo's mouth), not only "0x0A/0x0B". Every character glTF now binds each face segment to the
texture a match draws outside a blink: the eyes open and the mouth as it always is. The ten characters whose eyes
blink in a match get `faces` in `character.json`: the eye materials and their open, half and shut PNGs. The client
swaps them (M14c, below).** Research: `../lab/research/mp-original/faces.md` and `faces-scratch/results.txt` (runs c1,
c4, n1, v1, k1, b1, b2, h3, t1-t3 on a private `ConkerRecomp.faces`, since deleted).

| Part | What |
|---|---|
| conker-extract `lib.rs` | **`face_entries(rom, model, segment)`**: the bank-0x11 record's entries. Segments 6/7 take +8..+0xA and +0xB..+0xD (codes 0/1/2 = open/half/shut); 0x0A/0x0B take +0x3B/+0x3C. **`bind_faces`**: each material still on a face segment is bound (`bind_segment`) to its code-0 entry's texture, reading the drawn model's table and record (`CONKER_DRAWN` for 0). For a character that `blinks` (`PLAYERS`, `PICKS`, `BODY`), its eye segments' three looks go to `textures/face-<entry>.png`. `faces` lists each segment's glTF materials, checked to draw nothing else. It runs on every match character and every outro character, after the Wise Guy's and the tank's binds (already flat, so untouched). `pack_data`/`character_file` carry `faces` |
| conker-assets | `Character::faces: Vec<FaceSegment>` (serde default, skipped when empty). `FaceSegment { segment, materials, textures: [open, half, shut] }`. The round trip covers both cases |
| `pack-readme.md` | `faces`, and what the glTF draws: every face open, the mouth; 165, 134, 135, 88 and 40 never blink |
| Pack | Re-exported (`bun run pack -- --force`): 1,961 → 2,009 files, 224,340,309 → 224,404,651 bytes. New: 30 `face-NN.png` (0, 22, 83, 128, 136, 144, 150, 156, 157, 176) and 18 bound `character-NN.png`. Changed: the glTF/bin of 0, 22, 40, 83, 88, 128, 134, 135, 136, 144, 150, 156, 157, 165 and 176, those blinkers' `character.json`, and `README.md` |

| Character | Eyes: open / half / shut (texture ids) | `faces` materials |
|---|---|---|
| 0 Conker, 150 Neo | 1937 / 1939 / 1940 (Neo: 1946), 32×64; mouth 3649, static | 0: 12; 150: 15 (segments 6 and 7 share it) |
| 128 squirrel, 176 Sergeant, 156/157 villagers | 4200 / 4203 / 4201, 40×40 (128: 4200 / 4201 / 4201; 176: 4200 / 4203 / 4203) | 128: 5; 176: 10, 12; 156: 5; 157: 3 |
| 136 caveman, 144 racer | 4730 / 4732 / 4731, 32×32 | 136: 1, 3; 144: 3 |
| 83 raptor | 3694 / 3692 / 3690, 32×64 | 12 |
| 22 Raptor's body | segment 6: 4734 / 4735 / 4735; segment 7: 4730 / 4732 / 4731 | 6: 6, 8; 7: 1 |
| 165 baby, 134 Don Weaso, 135/88 (outros), 40 (Colors cs5 tank) | bound open (3695, 3271, 4200, 4216); never blink | none |

| Check (2026-10-14) | Result |
|---|---|
| Oracle | The mechanism (`func_1502F01C`, `func_1507E2B0`, `func_1502EEF4`) per faces.md §1. Every entry's RAM texture is the ROM texture of the packed id (`texcheck.py`, 33 entries over 14 models). The per-frame half/shut/half/open seen in both DL buffers (c4) |
| `tests/rom.rs` `faces_follow_the_oracle` | The record entries and texture ids of 14 models = the table. `bind_faces` on 0: flat 1937, no segment left, `faces` (6, [12]) and (7, [12]), 3 PNGs; on 22: two sets, 5 PNGs; 165, 134 and 40: no `faces`, bound to 3695, 3271 and 4216 |
| `tests/pack.rs` | No material image is a segment on any character. The blinkers (and only they) have `faces` [6, 7]. Each listed material's base-colour image = `textures[0]` byte for byte, and the PNGs are PNGs |
| conker-assets | 14 pass |
| Pack (`../lab/tools/verify-pack.sh`) | "pack equal", 2,009 files |
| `bun run test`, `bun run check` | 514 pass, 1 ignored; check passes (fmt, clippy `-D warnings`, `web/app`), with the other lanes' work in the tree |

**Gaps.**
- **Alpha:** Conker's and Neo's eye material is translucent in the original (render mode 0x04D04DD8, XLU). Its open
  and half PNGs are transparent where the eyeball (material 0) shows through. The glTF writer marks every character
  material opaque, so today's client draws those pixels cream (255, 247, 206) over the pupils. Other character
  materials share this (many XLU materials are opaque in the glTFs).
- **I:** why the baby never blinks (codes still in b1, flag 0). The meaning of 22's two different eye sets.
- **C, not seen:** a forced shut while alive (obj+0x127 ≠ 0xFF and (obj+0x31C)+0x120 ≠ 0); a death during a blink.
- **Outro lane:** Colors cs5's op 9 puts the tank's (40) segment 7 on entry 9 (texture 4368, lit lights). It is not
  packed; the glTF shows entry 0.

**Handoff to M14c (client): the blink.**
- **Load:** for each `faces` entry, load its three `textures` (relative to the character's folder). Each listed
  `material_NN` draws that eye; its glTF texture is already the open one. Swap per entity, as `render.rs` `wear()`
  does for the team looks: a per-entity copy of each listed material, with its `base_color_texture` set to the
  current look. Both segments switch together. When two segments list the same material their textures are equal
  (Conker's 12); 22 has two different sets, so give each segment its own.
- **Timing (V c1/c4, C `func_1507E2B0`):** open for T = rand(0..140) + 10 VIs (T/60 s, 0.17-2.5 s). Then half, shut
  and half, one game frame each (1/30 s each). Then open with a new T. Every blinker blinks this way, bots and hurt
  players too; each entity has its own timer.
- **Hold:** at health 0, keep the current look until the respawn (the original freezes it). Outros never blink
  (keep the glTF's look), and neither does the baby (165).
- **Alpha:** draw Conker's (0: 12) and Neo's (150: 15) eye material with `AlphaMode::Blend`; their PNGs carry the alpha.
- Nothing to do for the mouth (0x0A/0x0B) or the team segments: the glTF draws them.

## Characters: rules, server and wire (2026-10-14)
**Batch M14b. A human may play any of the original's 22 menu picks that the map allows. Deathmatch allows them on
0x2D, 0x34, 0x30, 0x1A and 0x3F, and Race on 0x33; the 7 cheats (`--cheats`) widen the sets. A class's bodies are its
model's record × its scale (`characters::body`). `Side` is now the team and spawn key only. A raptor picked on 0x3F
plays the raptor in Deathmatch. Protocol 31. Race's first-match crash is fixed at the root: nothing runs before the
first match.** Research: `../lab/research/mp-original/characters.md` (§3, §5) and `characters-scratch/results.txt`.
No new oracle run.

| Part | What |
|---|---|
| conker-rules `characters.rs` (new) | **`PICKS`**: the 22 menu rows (class, model; V `D_800AB57C`). **`CHEATS`**: code, bit, and the picks it adds (C `func_151E0B70`, `game_20AE20.c`). **`allowed(scene, cheats)`**: the set by the scenario row's +5, with each cheat's picks OR'ed in. The sets are 0x815 on 0x2D/0x34/0x30, 0x831 on 0x1A, 0x5C0 on 0x3F and 0x4380 on 0x33; 0 elsewhere (Tank, the team modes). **`pick(scene, cheats, slot, asked)`**: the asked pick if allowed, else the set's default for `slot & 3`. **`model(class)`** (`func_150169A0`). **`scale(class)`**: 0.6 for 0x25/0x26, 0.7 for 9, else 0.5. **`clip(class, id)`**: the Sergeant's and the Tediz Leader's 0x2C5/0x2C6/0x2C7 for 0xBD/0xE8/0x77 and 0x207, 0x34F/0x350 for 0x1A1/0x1A4. **`Body`, `body(class)`**: the record × scale, truncated in f32 (the hitbox, and the push body as `Other`) |
| conker-rules `game.rs`, `damage.rs`, `colors.rs`, `heist.rs`, `raptor.rs`, `projectile.rs` | `Fighter::hitbox()` and `other()` go by class; `Hitbox::of(side)` and `Hitbox::RAPTOR` are gone. The flag's and the bag's touch use the toucher's own body. A class-9 fighter runs `raptor_player` in any mode (the gate was `settings.raptor`), so a picked raptor moves, jumps and bites as War: Raptor's, with Deathmatch's scoring. `animate` drives `conker_anim::request` with `characters::clip(class, anim)`; the rules keep the canonical ids |
| conker-sim `movement.rs`, `tank.rs`, `raptor.rs` | `Other::new(pos, [radius, height, centre])` (y_scale = r/h). `Other::player(side)` is gone and `push_body` is merged into `push_apart`. `leap`, `step_among`, `ground` and `air` take the mover's `Other` |
| conker-sim `net.rs` | **Protocol 31:** `Join::character: Option<u8>` and `MatchView::characters: u32` (below) |
| conker-server | `--cheats code,...` or `all` (any case; an unknown code is an error). A human's class is its pick (`Join.character`) where the map has a select. A pick not allowed takes the slot's default, logged ("not allowed here"); on a map with no select the pick is ignored, logged. No pick keeps the scenario's class, as before. Bots keep the scenario's classes. `MatchView.characters` = `allowed`. The join line logs the class and the ask. Race: no bot AI or step before the first match (below). `bots.sh`: `CHEATS`, and `CHARACTER` (client 1's `--character`) |
| conker-client (minimal) | `character=N` / `--character N` → `Join.character`; `rival()` copies the class. Nothing drawn (M14c) |
| conker-bots `tests/rom.rs` | The fighters' classes set as the server sets them, since the bodies now go by class |

| Check (2026-10-14) | Result |
|---|---|
| rules `characters` (6) | The §3 table, all 22 rows (class 9's push body = conker-sim's `raptor::body`). A Sergeant pushes wider (two bodies 10 apart part by 24.8, Tedi 14.4). The Sergeant's clips. `model` = `beach::model` on the villagers. The sets and cheats. A refused pick takes the slot's default |
| rules `raptor` `a_picked_raptor_plays_in_a_deathmatch` | In Deathmatch a class-9 fighter's A jumps 70 (state 51) and a Tedi's jumps lower. A bite kills (kills 1, no team score). Clubbed down, it respawns a raptor on r7's frames (clip 19, hp 6, class 9), the kill the caveman's |
| sim `bodies_scale_as_the_oracles` | `Other::new`'s y_scale bits for the Tedi, the squirrel and the raptor (0x3F02_D82E, 0x3EE8_BA2F, 0x3F1C_5FD8) |
| server `picks_by_map_mode_and_cheats` | The flag (`all` = 0x3_01F0, a list, a bad code). 0x2D: 0x815, picks and refusals by slot. 0x3F with every cheat: 0x3F_CFFA, Conker and the raptor allowed. War: Raptor and Tank: 0, the scenario's classes. Race: 0x4380, pick 9 → 0xE, no pick → `race::HUMAN` |
| `bun run test` | 514 pass, 1 ignored (with the Race client and Faces lanes' work in the tree) |
| `bun run check` | Stops at `cargo fmt --check` on the Race client lane's in-progress conker-client files (`audio.rs`, `hud.rs`, `lib.rs` 1348/1451, `race.rs`); mine pass fmt. Its clippy (`-D warnings`, all 10 packages) passes when run alone, and so does the `web/app` check |
| wasm clippy (`conker-client`, `-D warnings`) | Passes. A first run failed on the Race lane's `hud.rs:737` and `race.rs:179`; they were fixed in that lane meanwhile |

| Run (`bots.sh`, `CARGO_TARGET_DIR=target-net`, ports 5970-5978) | Result |
|---|---|
| Deathmatch picks: 3 bots and 1 idle human picking, 90 s | 0x2D (`SIDE=tediz CHARACTER=0`): the Tediz side, class 0x4. 0x34 (`SIDE=squirrels CHARACTER=2`): class 0x3. 0x30 (`CHEATS=all CHARACTER=15`): class 0x0, Conker. 0x1A (`CHARACTER=5`): class 0x2, the weasel guard. In each, every bot moves and is seen moving, with no fault, unknown native or panic. Each run had 1-2 "fights, dies and respawns" FAILs (bots that never died in 90 s), the known flake |
| Raptor pick on 0x3F (`CHARACTER=10`), 90 s | Class 0x9. Only the body shows: bot 1 chased the idle raptor for 30 s, held 133-147 off (124 + 23), pushed it along and never hit it. One hit in all (bot 0), as in War: Raptor's run below. The movement isn't exercised (idle, state 0): the unit test covers it |
| Race (`MODE=race MAP=33 BOTS=7`, 1 idle human), 120 s | 2 matches, and the first now equals the second. Each is decided at the human's out 907 ticks after its start (t=1166 from the join at 259; t=3313). The human dies 351, 571 and 819 ticks in, in both. **0 bot deaths** (before: 7 crashes in the warm-up plus the human's 3 a match). Places [3, 2, 1, 6, 5, 4, 7, 8]; bots 1 and 2 end lap 1 27-47 ticks after the decision |
| Race pick (`CHARACTER=9`), 60 s | Class 0xE (the 0x77 helmet). The same first match (decided at t=1152), 0 bot deaths |
| The existing modes (no pick), 120 s: Total War 2d (3 bots, 2 humans), Colors 30 (7), Beach 2b (7), Heist 1a (3), Tank 24 (3), War: Raptor 3f (3), Deathmatch 2d (3) | The humans' classes are the scenario's: 3/4, 3, 0x14, 1, 8, 9, 3. Colors: 4 takes, 3 drops. Beach: 5 escapes, a strike, scores 5-5. Heist: 2 bags, 12 takes. Tank: 3 bombs taken, 1 planted, a blast and a shell death. War: Raptor: 6 eggs, scores to 4-2, 1 hit on the raptor human. No fault, unknown native or panic in any. The FAIL lines are the known ones (bots that never die or never hit in 120 s) |

**Race's first match: the root cause.** The earlier reading (§ Race below: "the script counts 0 humans in the first
match") had the effect right but not the place. The server starts the bots at its own start, so they raced alone
before any human joined. Script 0xBC counted no human to hold them back, and all 7 crashed at node 21's corner at
ticks 240-249. The match itself starts at the human's join (tick 260), where `restart_bots` puts the humans first.
The original never runs a race without its humans. **Fix:** in Race, nothing runs before the first match
(`warm_up = race && !started`: no bot AI, no `Match::step`), so the first match is like every later one. Run above:
the first match's ticks equal the second's and no bot dies (STATUS's Race runs had 16 deaths in 200 s, 7 of them these
crashes). A second human who joins mid-race still comes after the bots (gap below).

**Bodies that changed (§3's values used, as asked).**
- Beach's village women (classes 0x1D-0x20, model 157): push radius 28 (57 × 0.5), was the squirrel's 30.
- Race's bots (0xC-0xF, 144): hitbox 46/80/44, was 36/64/44. Nothing in Race reads a hitbox: missiles hit within 120
  and the kick reaches 250.
- The bots ROM test's Heist/vault fighters now have the Wise Guy's body (class 1), as the server already gave them.
- Unchanged: the Tedi (hitbox 26/43/35, push 23/45/33), and the squirrel, Wise Guy, caveman, racer human and village
  man (36/64/44, 30/66/33). The raptor equals the old `Hitbox::RAPTOR` and `raptor::body`. The tank's own `tank::BODY`
  is unchanged.

**Gaps.**
- **Until M14c, some picks crash every client in the match**, among them half of 0x2D's default set. The list is
  the handoff's first item.
- **I:** 0x2C7 (the Sergeant's bazooka shot) has no root-motion track in the pack, only 0x77 has one. A Sergeant's
  shot has no push back.
- **C/I, defaults by `slot & 3`:** the original's go by controller, and it has no check that two players picked the
  same.
- **I:** a second human joining mid-race is still appended after the bots (Bots' roster), so the script counts it
  from the next match.
- **I:** the tank (8) and the unused 5-7 get the squirrel's record in `body`; the tank's play reads `tank::BODY`.
- **V/I:** BILLYMILLROUNDABOUT's string in the ROM dump has 18 letters; the code table's entry is used as is.
- **I:** the bots rarely hit a raptor, picked or War: Raptor's: 1 hit in 90-120 s on the idle raptor human in both
  runs. Its push radius (124) keeps a melee bot 147 off. The values are the oracle's, but no oracle run measures bots
  against a human raptor.
- No test cross-checks `characters::body` against the pack's `character.json` (the pack is not read by conker-rules).
- The client's own picks, scale and clips: M14c.

**M14c handoff (the client).**
- **Wire (protocol 31):**
  - `Join.character` is the menu pick, an index into `characters::PICKS` (0-21), not a class.
  - `MatchView.characters`: bit k means pick k is allowed here; 0 means no select (hide the picker).
  - `Combat.class` is the class each player plays; the model, scale, bodies and clips all follow it. The own
    fighter's class comes from the server's `Fighting`, so the prediction already uses the right body.
  - In Race the cheats add non-racer classes (Conker 0, 0x18, 0x20-0x22, ...); the server rides them all on boards
    (`settings.race`). The client's `race::racer(class)` (0xB-0xF) must become `MatchView.race`.
- **The client's work:**
  - **First, a crash.** Draw by `characters::model(class)` for every class. The client's `body()` maps the classes
    1, 0xC-0xF and 0x11-0x20 to the map's extra models, past `PACK_CHARACTERS`. A pick of one of them where the map
    lacks that model indexes past the list in every client of the match (`render.rs:148` `Characters::of`,
    `lib.rs:636` `BodyClips::of`, `audio.rs:857` `cx.pack.clips[is.body]`).
    - On 0x2D/0x34/0x30 with no cheat, picks 4 (the Wise Guy, class 1) and 0xB (the village man, 0x18) crash: half
      the set.
    - A refused pick in a slot with `slot & 3` = 2 or 3 falls back to those two, so it crashes too.
    - On 0x1A, pick 0xB draws the Wise Guy, the first of 1a's extra models.
    - The cheats' racers and villagers crash off their maps.
    - 0x3F's picks 7/8 (0xC/0xD) index the raptor's model and clips: untested.

    Not guarded here (the client lane). The guard is the three sites' `.get(..)` falling back to the side's body.
    Nothing sends `character` yet but `--character` / `character=`; the runs above kept to safe picks.
  - Draw clips by `characters::clip(class, anim)` and at `characters::scale(class)`.
  - Retarget the picks onto 117's clips (M14a's handoff).
  - A lobby picker over `MatchView.characters`, `character=` in the URL, and the dev server form's `--cheats`.

## Race: rules, server and bots (2026-10-13)
**Batch M12, the rules half. `--mode race` on `--map 33` plays the original's Race (scenarios 11 and 12). Up to 8
racers ride hoverboards: the human is class 0xB, the bots 0xC-0xF by slot. They race `--laps` 1-20 (default 5) of
`--route 11` (the low road) or `12` (the elevated one), with 6 hp and the lives (3), untimed.**
- **The board.** Its target is 135 + 0.6 · stick (100-180), gained at 1 a frame. The autopilot (the bots, a finished
  human) gains 1 a frame under 100 and 2 from there, and sheds up to 25. The steering stays within 0x2400 of the
  track's direction. A jumps. A wall scrape costs 1 hp; a head-on hit or the last hp kills, and so does no floor.
- **The start.** The racers line up on the grid; a 90-frame countdown runs from frame 90, and the boards go at +181.
- **The race table** follows each racer along the route's 33 nodes a lap: laps, lap times, places, the finish.
- **The end.** The race ends when every human has finished or is out, or when at most one racer is still racing.
  Then come the results (39 frames on) and 40 s with everyone on the autopilot. A human out ends the match as in
  Deathmatch.
- **Power-ups.** A canister boosts to 254 (229 on the autopilot) for 60 frames. Missiles (up to 3) home on the racer
  ahead from their 10th frame and knock it up, with no damage. B kicks a racer within 250 into the air. A taken
  power-up is back 10 frames later.
- **Bots and wire.** Script 0xBC, run by conker-bots, drives the bots. Protocol 30.

Research: `../lab/research/mp-original/race.md` (§1-§4; runs c_*, w1-w5, F1, L1, L2, b1) and `race-scratch/`
(`lscript_bc.txt`, `code/`). New oracle runs L7e and L7f are below.

| Part | What |
|---|---|
| conker-sim `board.rs` (new) | **`step(p, board, input, world, drive)`:** the board branch of `func_1504CB98` and the hover spring of `func_1504CA60` (lift 20, gravity 3.8 above and −3.5 below, the −18/−80 fall caps). The speed is as above; a boost gives `BOOST` 254, or `BOOST_AUTO` 229 on the autopilot. The steering turns `stick · 100 / 9`, halved above 100 and cut to a third when knocked. The autopilot turns at most 833 toward its point. The track clamp is `func_15087E54`'s 0x2400. A jumps at 76 (bounce 45). The kick runs 29 frames: the hit on frame 10 and the hop (25) on frame 12. A knock gives y velocity 50 under gravity 2 for 30 frames and costs 3.5 speed. Touching the floor, the step follows the floor's plane. Walls use the long-step sweep (`D_800CBDD4`): a deflection, a 12-frame stun above speed 30 (speed × 0.7), and a hit every 20 frames at most (`Moved::wall`, head-on 0x5E01..0xA200). No floor sets `Moved::void` |
| conker-rules `race.rs` (new) | **`Track`:** the route's nodes and their edge partners. A point is node + (partner − node) · lateral. The lift of nodes with +0xF 5/6 is 75/55. `respawn(count, lateral)` puts the racer in the middle of the node, facing the track turned ±0xA00 (`func_15087EF0`). **`Ride`:** the race-table entry (count, frac, lateral, aim, lap, place, order, the times, the autopilot's speed, the board). **`Match::race_player`** replaces the on-foot frame. It handles the item on Z, the lateral and the autopilot's point 0.2 nodes ahead, then the board. A wall hit or the void kills; a scrape costs 1 hp. It runs the kick and then `race_table`, `func_15087350`'s projection: 50 near, 0.75 a step, at most 11 steps. That gives the laps, lap times (capped) and the lifts. **`race_take`** (the same item stacks to 3), **`fly_missiles`** (310 along the track, homing within 1000 from frame 10, 120 frames, a hit within 120 knocks up), **`knock`**, **`race_death`** (state 112, the item gone), **`race_respawn`** (88 frames on: hp 6, immune 150, the life counted then; a respawn node with no floor within 300 below skips to the next), **`finish`**, **`race_end`** (places each frame: the finished by order, then the racing by progress, then the out; the end rule; the humans not yet finished are finished; the results 39 frames on). `Op::Drive`/`Lateral` carry 0xBC's c0x12/c0x14 |
| conker-rules `game.rs`, `spawn.rs` | `Settings::race`, `laps`, `high`; `Arena::track`; `Fighter::ride`; `Match::race`; `Effect::Race(Op)`. In Race: no cone hits (0x33's ramps are floor kind 0xE), missiles and the end each frame, the end fade after the results' 40 s, the places by race place, the pickup counted the frame after its take. Spawn: 0x33's grid, slot k on record k |
| conker-bots | `script(0x33) => 0xBC`. Queries: `0xF000` 8-0x10 (the lateral, progress, track speed, lap, the racer ±1 place, table ready, the route, opt[2]/opt[7]); a prop's toucher (0x3001-0x3007 = the arena's pickups, on the frame after the take); a racer's item count (0xDF) and 0xB3 (0). Commands: c0x12 (speed) and c0x14 (lateral) become `Effect::Race`; 0x11, 0x13 and 0x16 are the match's own; `0xE01F` 5; props 0/1/4; player 0x18/0x20/0x21/0x27/0x76/0x78. A race bot's input is only the script's pad (Z, B); the autopilot steers. Race's globals stay as they are on a join or leave |
| conker-server | `--mode race`, `--route 11/12`, `--laps 1-20`: map 33 only (and 33 plays only Race), no `--cavemen`, `--time`, `--kills` or `--score`, at most 7 bots. `race.json`'s route gives the track, and its hidden barriers are shifted 20000 up. The classes are `race::HUMAN` and `bot_class(slot)`. Each joiner gets its `Ride` with its own go. `respawn_in` counts the 88 frames. **RACE lines:** laps, finishes, outs, boosts, takes, missiles, knocks, deaths, the decision and the results. `bots.sh` takes MODE=race, ROUTE and LAPS. It prints each bot's laps and a race summary, and checks for a lap or a power-up taken and the decision |
| conker-sim `net.rs` | Protocol 30: `MatchView::race`, `racing: RaceView` (below) |
| conker-client | `match_settings` copies `race`, `laps` and `high`, so the local board is predicted. Nothing is drawn (M13) |
| web | `data.ts`: mode `race` ("Race": lives, no kills or score, at most 7 bots, `laps`); map `33` plays Race only. `Settings` gains `laps` (1-20) and `route` (11, 12; `ROUTES`). `settingRows` shows Laps and Route in place of Time. `Servers.tsx`: Laps and Route selects (Race only; Time off in Race). `server.ts` validates both and passes `--laps`/`--route` in place of `--time`/`--kills`/`--score` |

| Check (2026-10-14) | Result |
|---|---|
| sim `board` (7) | c_fwd/c_back speeds, c_fwd hover, c_steer and the clamp, c_a jump, c_b kick, w5 knock, walls and the void |
| rules `race` (7) | r0 countdown; the table follows node by node (−1 behind the line); F1's lap, finish, autopilot and end (results 39 frames on, 1201 more to the fade); bots race and finish in order, and bots alone stay undecided; the void kills, back on the track 88 frames on (hp 6, immune 150); canister, missile and kick; b1, a human out ends it (and places behind the bot racing) |
| bots ROM `race_bots_finish` | 3 bots (as L1/L2) and 7 (as L7e/L7f), both routes, 9000 frames: no unknown natives or faults, the results come, ≥ bots − 2 finish; the 3 bots' last laps within 15% of 837/1066; the idle human's first 3 deaths within 8 reads of L1's; the 7 bots' mean finish within 5% of the oracle's |
| server `race_flags_grid_and_classes` | the flags and bad combinations, the grid and the classes |
| `bun run test`, `bun run check`, wasm clippy (`conker-client`), `web/app` check | 499 pass, 1 ignored; check, clippy and the web check pass (with the Raptor client lane's work in the tree) |

| Run (ROM test vs oracle, an idle human with 99 lives) | Remake | Oracle |
|---|---|---|
| route 11, 3 bots: steady laps | 946-948 | 837 (L1) |
| route 12, 3 bots: steady laps | 1067-1070 (a bot out after 4 deaths) | 1063-1081 (L2: a bot out at 4128) |
| the idle human's deaths (reads) | 374, 594, 842, then 1181 | 378, 599, 845, 1085 (L1, L7c, L7e) |
| route 11, 7 bots (seed 2): first lap / steady | 718-827 / 832-834 | 786-876 / 798-959 (L7e) |
| route 11, 7 bots: finishes after the go | 4150-4295, mean 4224 | 4165-4323, mean 4249 |
| route 12, 7 bots: first lap / steady | 826-935 / 935-940 | 841-1002 / 881-1088 (L7f) |
| route 12, 7 bots: finishes after the go | 4645-4793, mean 4720 | 4762-4880, mean 4814 |

L7e/L7f: `race-scratch/run.sh` with `SCEN=11/12 BOTS=7 RACE=5 POKE=30:800D2144=63060000`, idle P1, 240 s, on the
private `ConkerRecomp.m11` (outputs in `/tmp/m12r/runs`, not kept). `RACE=` sets the laps: earlier tries with `RACE=1`
(L7c, L7d) were 1-lap races. L7d's results came 39 frames after the 7th finish (1249 → 1288), as F1's. L7c went to
phase 7 at 1170 with no phase 6 (I). The lap counts come from crossings of the line through nodes 19 and 48.

| Server run (`bots.sh 200`, MODE=race MAP=33, 7 bots + 1 idle human with 3 lives, ports 5970-5976) | Result |
|---|---|
| route 11, seeds 1 and 2 | 3 matches each. Each is decided at the human's elimination, 907 ticks after its start (read 930; b1: 933), as Deathmatch's end. Bots 1 and 2 finish lap 1 before it; places [3, 2, 1, 6, 5, 4, 7, 8], the human last. 12 boosts, 0 missiles, 12-13 knocks, 16 deaths (each bot's first-match crash, below, and the human's 3 a match) |
| route 12, seeds 1 and 2 | The same, decided 927 ticks in (read 950; L7d's human: respawn 948). No bot finishes lap 1 first (L7f's first laps end at reads 1088-1249). 15 boosts, 12-13 knocks, 16 deaths |
| all four | every bots.sh check ok: the client connected, every bot moves and is seen moving, a lap or power-up, the decision, no script faults or unknown natives, no panics. The seeds change only the knocks |

**M13 handoff (the wire, protocol 30).**
- `MatchView::race`; `MatchView::racing` (`RaceView`):
  - `route` (11/12) picks `race.json`'s route. The client must hide the barriers not in its `shown`, as the server
    shifts them; the client world is not shifted yet.
  - `laps`.
  - `go`, the tick the boards go. The beeps (0x7687 at stages 1 and 2, 0x7688 at 3) come 62, 32 and 7 ticks before
    it; `stage` is the countdown's stage now.
  - `results`, the tick the results in the level start (HUD off, everyone on the autopilot, the fade
    `race::RESULTS_FOR` = 1201 later), and `result` (1, or 2 with a human out).
  - `racers` (`RacerView`): `slot`, `lap` (−1 behind the line), `place`, `order` (the finish, 0 racing), `progress`
    (nodes · 16), the human's `time`/`last`/`best` (VIs, −1 none), `boost` (VIs left), `kick` (frame), `knock`
    (frames left), `jumped`, `auto`.
  - `missiles` (pos, the homed slot).
- Per player:
  - `Combat::class`: 0xB is the human (model 136), 0xC-0xF the bots (144, helmets by class).
  - `anim` / `PlayerState::state`: 5 riding, 84 kicking or knocked up, 112 dead.
  - `weapon`: 0x31 a canister, 0x33 missiles, with `rounds`.
  - `respawn_in` counts the 88 frames.
- The power-ups are the arena's 7 pickups (placements 4-10), hidden while `taken`; they are back 10 frames on.
- `Fighter::ride` (the board, the table entry) is predicted with the match settings.
- The client's `Arena::track` is empty, so its prediction steers with no track clamp and the autopilot aims 300
  ahead. Building it from `nav.json` and the route's `lap`, as the server does, fixes both.

**Gaps.**
- **I, the 3-bot laps on route 11** are 13% slow (947 vs 837). L1's bot 3 fired its 2 missiles at reads 930/939 and
  then rode its canister line every lap: script mode 2 (`G[0xa4]`) applies while it holds no item. Ours holds its
  missile for good, because mode 3's fire needs a human one place ahead. How the oracle's human got there is
  untraced. The 7-bot runs and route 12 match.
- **I, the autopilot's ramp** (+1 under 100, +2 from there, −25) is read off L7e's rows, not the code. A finished
  human's autopilot uses it too, unchecked.
  - `BOOST_AUTO` 229 is V (L1, L2), but why it is not 254 is unknown.
- **C/I, the track:**
  - The track is straight between nodes; the original uses a Catmull-Rom window of 4 nodes (`func_15088F30`).
  - Respawn facings are off by 460-2210 (L1 466/687/933).
  - The idle human's 4th death comes at 1181, not 1085. Later, on route 11, it loops: it respawns at node 13 and
    hits the same wall.
- **I, the respawn over the void:** the next node with a floor within 300 below. V L1 1798 (a chasm death back on
  node 9), but the rule is untraced.
- **I:**
  - No cone hits in Race.
  - The lateral eases 0.02 a frame.
  - The kick reaches 250; a missile hits within 120.
  - A lone racer races on; bots alone are never decided.
- **Server, I:** the human joins after the bots, so in the first match the script counts 0 humans (Bots' roster
  ponytail). With no rubber band, the bots race at up to 200, and all 7 crash at node 21's corner about 60 frames
  after the go. From the second match on, the human is participant 0. **Fixed (2026-10-14):** the crashes came in
  the warm-up before the human's join, not in the match; Race no longer runs before its first match (the
  Characters section above).
- **Server, V b1:** with the default 3 lives the idle human is out about 930 frames in, which ends each match before
  most bots' first lap. bots.sh now reports the bots' laps without failing on them.
- **Not done:** opt[0x13], the reverse direction (lift codes 7/8/9), which the menus never set.
  - No checks of a human finishing on the server.
  - The client draws nothing (M13).

## Raptor: the client (2026-10-13)
**Batch M7. The client draws and plays War: Raptor (`--mode raptor` on map 3f):
- raptors (class 9) are model 83 at 0.7 with their clips at the oracle's rates. A bite kill puts the body (22, 0.51)
  in the jaws (joint 6) and hides the victim; dropped bodies lie where they fell. The raptor's camera sits behind it. Cavemen are 136 as
  before;
- the nest: the baby (165, 0.45) idles and eats a fed body (its four clips, the body falling from the raptor's jaws
  into its own). The three eggs (171, 0.2) sit at their places or in a caveman's hands. Cooked eggs sit at the fire
  where they landed, and an egg splats as it lands;
- the HUD: the team strip with Raptor's heads, the egg's and body's icons (flag 0x4000, blinking), no weapon icon for a
  raptor or an egg carrier, and the radar. STATS is the other team modes' (YOU WIN/LOSE);
- the sounds: the raptor's clips (bite, steps, idle), the baby's and the fed body's clip events, the splat;
- the outros: cs2 when the cavemen win, cs4 when the raptors win.

No wire or rules change.** One extract change, made before the lane moved: conker-extract `hud.rs` packs
`raptor_teams` (the RAM's 0x80206BC0 that h1's DL 40 draws), re-exported and checked "pack equal". Research:
`../lab/research/mp-original/raptor-client-scratch/` (oracle `ConkerRecomp.m7`, `run.sh`): HUD walks h1, h2, s1, s2
(`draws_*.txt`), and runs hg (the human as a raptor, DLs 200/400), b22 and b22b (h1's bite, DLs 70-130), read with
`characters-scratch/drawn.py` (dumps deleted).

| Part | What |
|---|---|
| conker-client `raptor.rs` (new) | **`rate`** (idle 1, then 0.5; slow 0.1 + speed/14; run 0.5 + speed/43.4; bites, jump, drop, respawn 0.8) and **`carry_rate`** (a caveman with an egg: walk speed/6 + 0.4, run speed/16 + 0.5). **`camera`**: `tank::behind` with `CAM` (at rest 340 back, 150 up, looking 129 over it; at full speed 410, 146, 124), the stick read along its heading. **`Nest`** (resource from the pack's `raptor.json`): the feed's and drops' ticks, cooked eggs, splats. **`add`**: the baby (record x/z, on the floor) and 3 eggs. **`draw_baby`** (idle 55, the feed's clips 10, 7, 8, 56), **`draw_eggs`** (in the carrier's hands at `HELD` 40 ahead, 38 up, world units), **`draw_bodies`** (prey: clip 12 at the jaw; fed: the feeder's jaw, clip 7 falling 24 frames to the baby's, then the baby's; drops: clip 13, gone after `DROPPED`; none in outros), **`props`** (cooked egg placements moved to `cooked[k]`, the others hidden), **`splat`** (`beach::burst`, 8 puffs). **`first_list_only`**: 83's, 165's and 22's parts other than list 0 hidden. **`hide_bitten`**: a caveman dead from a bite (clip 0x170) is not drawn |
| `tank.rs` | The hull camera's body as `behind(follow, cam, b, &Behind, world)` (shared with the raptor; the tank's numbers unchanged) |
| `lib.rs`, `main.rs` | `characters("3f")` adds 83; `body()`: class 9 → it; Raptor's clip rates in `tick_anims`; the raptor camera in `predict`; `run` loads `Raptor` (native and wasm) |
| `render.rs`, `held.rs` | Raptors at `raptor::SCALE`; their creep root pinned for clips 1 and 53; `add_props` on `m.raptor`; `show_pickups` leaves `Nest::owns` to `raptor.rs`; a raptor holds no models |
| `hud.rs` | `RAPTOR_ICONS` (`atlas2` egg [48, 42, 12, 15] at [136, 3]; `raptor_teams` body [0, 16, 12, 15] at [148, 3]), `raptor_strip` (Beach's layout on `raptor_teams`), `Shown::raptor`, egg carriers from `MatchView::raptors`, no weapon icon for them and raptors; STATS rows by team, the header YOU WIN/LOSE |
| `outro.rs`, `audio.rs` | Raptor is a team mode (winner 0 → cs2, 1 → cs4). `Sounds::raptor`: the baby's (165) and fed body's (22) clip events at the nest, the splat's 0x1A6 and 0x434 (0x7D00, 500/1000) |
| `lab/screens/raptor/` | `shots.sh` (groups a-d, ports 5980-5987), `bite.txt` (slot 0: Z at 180, runs 210-269) |

| Check (2026-10-13) | Result |
|---|---|
| raptor `the_pack_agrees` | 83's scale, the three characters, the carry joint, 22's clips (names, logical ids, durations), the baby's floor (−10) |
| raptor `rates_follow_the_oracle`, `the_feed_follows_the_oracle` | the rates above (V r1, r8; the carrier's r4); the baby's and the body's clip at each frame of r3's feed |
| raptor `the_camera_sits_behind_and_the_stick_reads_its_heading` | the eye behind the raptor; the stick yaw = its heading − 90° (±1) |
| hud `raptor_matches_the_originals_draws` | h1 DL 40 (no icons) and 101 (the body's icon at [148, 3]); h2 DL 235 (the egg's at [136, 3], no weapon icon) and 240 (blink off); s1 560 (STATS rows and the strip) |
| Oracle hg (`drawn.py`) | 83 draws 456 vertices, all of list 0; obj+0x94 = 2 (list 1 hidden); lists 2-3 (the second table) not drawn |
| Oracle b22, b22b (h1's bite, `drawn.py`) | the victim (136) drawn at DLs 70 and 85 (160 vertices), not from the kill's DL 95 on (101, 130 too); the body (22) from 95, 215 vertices, list 0 only |
| Live, `shots.sh` (audio debug) | the bite's 946 (read 184), 1068 and 1085 at the kill (198), steps 968/969/978/979 running; the feed: the baby's 1055 and 946, the body's 190, 957 eating; the baby's idle 269-274; the splat's 1076 (0x434); FEED and SPLAT logged on every client |
| `bun run test` | nine crates pass (conker-client 117); conker-rules' tests do not build (`race.rs:882`, the Race lane's work in progress) |
| `bun run check` | clippy `-D warnings` (all ten crates) and `web/app` pass; `cargo fmt --check` fails only on conker-bots' `tests/rom.rs` (the Race lane's work in progress; an earlier run passed whole) |
| `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |

**HUD findings.** Raptor's HUD is Beach's strip (raptor heads at [66, 196], the cavemen's head at [198, 196], both
from `raptor_teams`) and the clock. Its icons blink (vi & 31 ≥ 11): the egg's while a caveman carries one, the body's
while a raptor has prey, for everyone. No weapon icon for an egg carrier or a raptor. The radar shows team colours. No
lives count (a team mode, as Colors and Beach).

Screens (`../lab/screens/raptor/`, 1280 × 720):
- **The raptor:** `bite.png` (slot 0's view and HUD, the body's icon), `body-side.png` (the body in its jaws, the
  victim not drawn), `running.png` (with the body).
- **The nest:** `nest-idle.png` (the baby idle), `nest.png` (the baby with the body, the eggs behind),
  `nest-late.png` (cs4's start).
- **The eggs:** `egg-side.png` (one carried, two in place), `egg-carried.png` and `hud-egg.png` (the carrier's HUD,
  the icon on and off; its camera is in the alcove's rock: the on-foot camera's, not Raptor's), `cooked.png` (the
  cooked egg at the fire).
- **The outros:** `cs4-early.png`, `cs4-late.png`, `cs4-stats.png` (YOU LOSE), `cs2-early.png`, `cs2-late.png`,
  `cs2-stats.png` (YOU WIN).

**Gaps.**
- **Pack lane (route to M14a):** sound 0x1A6 (the splat's first) is not in the pack (C script 0xBE plays it). The
  outros' lengths: `outros.json` says cs2 765 and cs4 415 frames, the oracle played 750 and 375 (V). 83, 165 and 22 could
  carry `hidden` in `character.json` (83: 0xE, V hg; 22: 0x2, V b22; 165: 0x2, I) instead of `first_list_only`.
- **As before (pack-wide):** the runtime face textures: 83's and 165's eye quads (segments 6/7) are drawn grey.
- **Not in the original:** no speech bubbles in either outro (V: cs2 two runs, cs4 one). The egg's pickup and cooking
  have no sound of their own (C: script 0xBE only plays the splat's two).
- **Uncompared:** the baby's look (its head about twice its body, idle too; 165 was never in the oracle's view).
- **I:** the body's place and turn in the jaws (joint 6, no turn of its own); its fall to the baby (a straight line
  over 24 frames); the baby's feed rates (1); the dropped body's clip 13 looped; the splat's puffs; 165's list 1
  hidden (its second table, as 83's).
- **Not modelled:** the raptor camera's rise when turning and its look on a jump; the header for two or more humans
  on one team (the team header textures are not in the pack: YOU WIN/LOSE stands in).
- **Not exercised live:** the bone weapons' sounds (the cavemen's clip events, as other weapons); the drop (Z with a
  body); a raptor's death and respawn.

## Characters: the pack (2026-10-14)
**Batch M14a. The pack has the nine picks no mode's map brought: Conker (0), Neo Conker (150), the weasel guard (152),
the zombies (159, 160), the Sergeant and the Tediz Leader (176, 177) and Gregg (178, 180). They are mesh-only glTFs on
the Tedi's skeleton whose `character.json` says `clips_from: "117"`, with their voices and clip sounds. Every player
model's `character.json` (the old ones too) gains its `hitbox` and push `body` from its bank-0x11 record. Conker draws
model 1 (V). No rules, server or client change: M14b and M14c use them.** Research:
`../lab/research/mp-original/characters.md` (§3, §5; §4's Conker question settled), `characters-scratch/`
(`results.txt`'s M14a block, `drawn.py`; runs k15, c16, c18, c19, c1, c5 on a private `ConkerRecomp.m14a`, idle on
0x2D, DL 200, dumps deleted).

| Part | What |
|---|---|
| conker-extract `lib.rs` | `PICKS`; `mesh_only`: the model on the Tedi's rig with no clips, so its glTF has no animations; 0 takes model 1's mesh (`CONKER_DRAWN`) under 0's number (its record, voices, class's model). Packed with any map (a cheat adds them to every Deathmatch map and Race; 152 is 1a's), after the racer. `character_file`: `hitbox`/`body` (`hitbox_body`: record +0x20/+0x22/+0x24, +0x1A/+0x1C/+0x1E) for `PLAYERS` and the picks; `clips_from` "117" for the picks; `hidden` (`HIDDEN`). The tank (119) gets none: its record (227/180/105, 157/119/105) is not its hit test (conker-rules' `tank::BODY`) |
| conker-extract `audio.rs` | Unchanged: as characters, the picks get their clip sounds (bank-02 entry 0's routes with their own `[0x12, model]` table) and hit voices; a sound two characters play moves to `common` |
| conker-assets | `Character` + `hitbox`, `body` (`Option<[i16; 3]>`), `clips_from` (`Option<String>`), `hidden` (`Option<u16>`): serde defaults, skipped when absent, so older files read and the others' files write as before. The round trip covers them |
| `pack-readme.md` | The nine folders and the four fields |
| Tests | `tests/rom.rs` `pick_characters`: each pick's 28 joints, parents, matrix and animation slots and flags = the Tedi's, bank-02 entry 0; `mesh_only(0)` = #1 (701 vertices; #0 has 913); 150's lists 5-9 load 0-4's vertex addresses; the zombies' 11 lists; all 22 menu rows' `hitbox_body` × class scale, truncated in f32 = §3's table (the bats included); the voice tables 176 = 128, 177 = 117, 152 = 59, 150 = 0, 178 = 180 (all zero), 144 = 136, the others' distinct, none for #1. `tests/pack.rs`: the picks after 144 in `pack.json`; their `character.json` and glTF read back (no animations, the 28 joints' names, children and `animSlot` = 117's); `hitbox`/`body` = the record on the 17 player models, absent on 119, 87, 134; `hidden`; the audio: folders, 177 = 117, 152 = 59, 178 = 180 (events and hit voices), 150 = 0 less two, 176's voices among 128's, 1254 in `common`, 159's in its folder |
| Pack | Re-exported (`bun run pack -- --force`). New: `characters/0/`, `150/`, `152/`, `159/`, `160/`, `176/`, `177/`, `178/`, `180/`; `audio/characters/159/`, `160/` (4 WAVs each); 28 WAVs in `audio/common` (0's and 150's 361-373, 378, 382-384, 873-875, 1132, 1133, 1513-1518). Changed: the player `character.json`s, `audio/events.json`, `sounds.json`, `pack.json`, `README.md`. Moved to `common` (two players now), the stale copies removed by hand: `audio/characters/117/` (1243, 1244, 1254: 177's too), 128's 0882-0887 (176's), 59's 1352-1360 and 1366-1369 (152's) |

**Size:** 222,073,064 → 224,315,158 bytes (+2.24 MB: the nine characters 0.79 MB, 36 new WAVs 1.26 MB, the JSON),
1800 → 1958 files. With the 764 clips embedded each would have been about 12 MB (about 108 MB).

| Pick | Model, vertices | Drawn (V) | Lift | Hitbox, full size | Body, full size | Voices |
|---|---|---|---|---|---|---|
| Conker | 0 → mesh #1, 701 | all 701 of #1 | 43 | 49/100/87 | 51/97/91 | own |
| Neo Conker | 150, 1800 | lists 0-4 (900); `hidden` 0x3E0 | 43 | 49/100/87 | 51/97/91 | = 0 |
| weasel guard | 152, 179 | all | 49 | 73/128/88 | 34/133/66 | = 59 |
| zombie | 159, 369 | lists 0-3, 5-7 (272); `hidden` 0x710 | 40 | 51/112/75 | 40/117/64 | own |
| zombie | 160, 403 | lists 0-3, 5-7 (306); `hidden` 0x710 | 40 | 47/112/88 | 30/114/66 | own |
| Sergeant | 176, 387 | all | 41 | 73/128/88 | 60/133/66 | = 128 |
| Tediz Leader | 177, 393 | (one list, I) | 41 | 73/128/88 | 60/133/66 | = 117 |
| Gregg | 178, 356 / 180, 390 | (one list, I) | 43 | 53/87/71 | 47/91/66 | none (zero table) |

| Check (2026-10-14) | Result |
|---|---|
| Oracle k15, c16, c18, c19, c1, c5 (`drawn.py`) | the table's "Drawn": every G_VTX's bytes and command matched to the bank-01 asset and its lists; obj+4 01 00 for Conker; obj+0x94 0x710 for the zombies, 0 for the others |
| conker-assets, conker-extract tests | pass (conker-assets 14, `tests/pack.rs` 6, `tests/rom.rs` 16, `tests/traps.rs` 1) |
| `bun run test` | 497 pass, 1 ignored (with the Raptor client and Race lanes' work in the tree; a later run stopped at conker-client's in-progress `render.rs`, the Raptor client lane's) |
| `bun run check` | fails at `cargo fmt --check` only on the other lanes' in-progress files (conker-bots, conker-rules, conker-server, conker-sim). conker-assets and conker-extract pass fmt and clippy `-D warnings`; `web/app` check passes |
| Pack (`bun run pack -- --force`, the stale WAVs removed, then `../lab/tools/verify-pack.sh`) | "pack equal", 1958 files |

**Gaps.**
- **I:** 177, 178 and 180 were not run on the oracle; each has one list, read as drawn whole.
- **Not found:** where the zombies' obj+0x94 = 0x710 comes from (not the bank-0x11 record); whether a hit or a death
  changes it (I: the hidden lists look like damage states). What 150's second list table (lists 5-9: the same vertices,
  untextured, combine 0) is for: never run in a match (V).
- **As before:** the runtime face textures (segments 6/7, and 0's and 150's 0x0A/0x0B) are untextured in the glTFs. (Done: Faces, above.)
- **I:** two of 0's clip events (369 on clip 0x13, 373 on 0x58) are for model 0 only (the op-9 event's character
  byte); whether the game compares 0 or the drawn rep 1 is open. 150 lacks them.
- **C/I:** the code's voices by model stay their models' (`code_cues`: the squirrel's pickup line, class 0x80, and the
  Wise Guy's, model 0x3B, C; the Tedi's bazooka line, I), so 176, 152 and 177 do not say them.
- **Open (§4):** 176/177's lift at 0.6 and their bazooka and gun clips. No `scale` in the pack: the class's scale is
  code (0.6, 0.7 are not in the records), the rules' `scale(class)`.
- **Not packed:** the bats 154/155 (unreachable; their records are `pick_characters` data).
- The client loads every `sounds.json` sound: 36 more WAVs (1.3 MB).

**Handoff to M14b (rules, sim, server).** A class's `Body`: `characters/<model(class)>/character.json`'s `hitbox`
(radius, half, centre) and `body` (radius, height, centre) × `scale(class)`, each `(f32::from(v) * scale) as i16` in
f32 (0.7 × 290 is 203, not 202); `pick_characters`' table is §3's. Class 0's folder is `0`. The tank keeps
`tank::BODY`.

**Handoff to M14c (client).**
- A pick: its `character.gltf` (mesh and skin, the 28 nodes `joint_00`-`joint_27` as 117's) driven by **117's glTF
  animations**, found by 117's `character.json` `clips` (the pick's are empty); `model_lift`, voices
  (`events.json` `characters["<model>"]`, the hit cues by `character`) are its own.
- **Retarget the translations.** The Tedi's clips key the translation of joints 0-15, 24 and 26 as the Tedi's bind
  offset plus the clip's delta, and the picks' bind offsets differ from the Tedi's (on 22 to 28 joints): played raw,
  a pick takes the Tedi's proportions. Draw joint k at `key − 117's node k translation + its own node k translation` (root motion
  included). In Bevy: before the animation each frame set those joints' translation to 117's node value, after it add
  `own − 117's` (a joint the playing clip does not key then rests at its own).
- `hidden`: hide the primitives whose `extras.list` bit is set, OR'ed with the class's `looks`.

**Pack follow-ups (M7's three).** Pack 1,961 files, 224,340,309 bytes, "pack equal"; `bun run test` and `check` pass.
- **The splat:** event `raptor_splat` (two cues: 0x1A6 and 0x434, 0x7D00, 500/1000, C script 0xBE). 0x1A6 is now in
  `common/0422.wav`; the client's `SPLAT_SOUNDS` find both ids in `sounds.json`.
- **Outro lengths, the table read was wrong.** `func_1501CE54` ends a cutscene at its first object's 0xFE record (its
  record 2) unless that is −9999; only then at the longest 0xFF record. Raptor's 0xFE records are 1500 and 750, the
  other maps' −9999 (C; V M7's 750 and 375 frames). `outros.json` now gives 1500 and 750.
- **`hidden`:** 83 = 0xE, 165 = 0x2, 22 = 0x2. 165 is checked too: on M7's h2 DLs 200-246 it draws its list 0, 443
  vertices, of 2 lists. 165 and 22 now have a `character.json` (with no hitbox). `first_list_only` can go.
- **165's head is about twice its body's size in the original too.** Its head joints' bind mesh is 172×211×263 against
  the body's 88×119×101. On the oracle (h2 DL 200) every load is drawn at one scale (0.45), so the head is drawn
  94×83×117 and the body 45×58×47 (V). The raptor's clips scale joint 5 by 0.6-1.37 on both models.

## Tank: the client (2026-10-13)
**Batch M10, the client half. The client draws and plays Tank (`--mode tank` on map 24):
- every player is model 119 at 0.5. Its turret (joint 2) turns by `turret` and its gun (joint 3) pitches by `pitch`,
  over the clip, so the gun points where the rules fire. The treads' texture scrolls with the speed and the brake
  lights light while braking. The flag is the slot's (red, blue, green, yellow from slot 3). The model hides from the
  death to the respawn while its wreck stands in;
- the local tank's camera follows the hull, and the scope (power-up 0x02, R) looks from the turret;
- the shells (`projectiles.gltf#Scene4` at 1.5) and their blasts (War: Beach's puffs);
- the props: the bunkers' doors slide apart along their own z, the bomb shows on its post or over its carrier, and the
  wrecks stand where tanks died (330 frames);
- the HUD: the power-up's icon at the weapon icon's place and the tank's scope. The countdown digit and the gas haze
  are Total War's, unchanged;
- the sounds: the engine (idle, driving) and its gear changes, the shot (clip 1's event), the shells' blasts, the
  deaths, the bomb and power-up pickups, the countdown's song at the plant, and the gas.

No wire, server or pack change.** Research: this batch's `../lab/research/mp-original/tank-client-scratch/` (oracle
`ConkerRecomp.m10`): `run.sh`, `hw.sh` (HUD walks), the runs pw1 (each power-up), pw2 (the scope, R 170-200) and bm1
(the bomb, the countdown, the gas), their `draws_*.txt`, and `probe.sh` (a remake match with free cameras).

| Part | What |
|---|---|
| conker-client `tank.rs` (new) | **`Parts`** per tank: joints `joint_02`/`joint_03`, its own copies of the brake lights' material (off, and lit with the pack's `Tank::brake_lit`) and the treads'. **`pose`** (PostUpdate, after the clips): the turret × `Quat::from_rotation_y(turret)`, the gun × `from_rotation_x(−pitch)` (`turns`); the model hidden while `health == 0 && respawn_in > 0` (V c1: obj+0x6A/6B from the death + 1 to the respawn). **`lights`:** the brake swap (`braking`: past 5 with the target behind, C) and the treads' `uv_transform` (`scroll`: −7 S a frame per unit of speed, wrap 0x800, 32 S a texel, C `func_150ED638`). **`camera`** (below). **`shells`:** a model per `TankView::shells` entry, nose along (yaw, vy/speed); a shell gone is a blast of 16 puffs over 120 (`beach::burst`). **`props`:** doors 0-7 at home + their z × `lifts`; the bomb (12) on `POST` while `bomb`, over its carrier (190 up, 0.3 of the post's scale) while carried; wreck 18 + slot at (pos, facing); 9, 13-15, 17, 26-33 hidden (`owns`) |
| `tank.rs` `camera` | **Behind the hull:** at rest 310 back, 159 up, looking at 88.5 over the tank; at full speed 400, 135, 52; straight in the speed's share (−1..1, eased a tenth a frame; reversing nearer). The heading closes on the hull's at 0.067 of the gap a frame, + 0.03 at full speed, at least 0.2°; a wall between pulls the eye in. **Scope** (power 0x02 and R): the eye 27.5 ahead along the turret's yaw and 111.6 up, looking level, `Scope { on, weapon 0x1F, zoom (−38.7, −23.5) }` (field of view 26.5°); R let go cuts back behind the hull. The on-foot camera still runs for the rules' aim gate, as the server's (`FollowCamera::tank`) |
| `lib.rs`, `smooth.rs` | `characters("24")` adds 119 (`tank::MODEL`); `body()`: class 8 → it. Tank clips play at rate 1 (I). `seat_props`: `tank::seat` (the doors' collision). `FollowCamera::tank`: `follow` is the tank's view, `tank.0` the on-foot camera; a correction blends the body only (`drawn.camera` cleared) |
| `render.rs`, `beach.rs` | `dress_teams`: a tank's team look is `tank::flag(slot)`; `add_props` also on `m.tank`; `show_pickups` leaves `tank::owns` to `tank.rs`. `beach::burst` and `Sprites` are `pub(crate)` (the strike's puffs as a function) |
| `hud.rs` | **`Shown::tank`:** `weapon` is `Combat::power`, its icon from `TANK_ICONS` (`atlas2`); the scope's zoom by `zoom_limit(0x1F)`; no dots |
| `audio.rs` | **`Sounds::tank`:** per tank the engine loop (`On::TankEngine`: 1191 idling, 1192 driving) from the spawn to the death; `engine()` states 0 idle, 1 speeding up (the target past the speed, faster than 1), 2 coasting (faster than 5); a change plays `On::TankGear` (1187-1190, into coasting 1189). A shell gone: `On::TankBlast` (the rocket's set; on the ground also 750), on a tank if one within 300 lost health. A death: `On::TankDeath` (742 after 22 frames, 735 by the bomb). The bomb taken: `On::BombPickup` (11, 12). The plant: the map's `countdown` song once, unducked (id `COUNTDOWN`). `Seen::target`; Tank's bomb carrier sends no canister radio |
| `lab/screens/tank/` | `shots.sh` (groups a-d, ports 5980-5987), `drive.txt` (slot 0), `scope.txt` (R held) |

| Check (2026-10-13) | Result |
|---|---|
| tank `the_turret_and_gun_turn_where_the_rules_fire` | the muzzle (joint 4 over joint 3) rises with the pitch; turret +90° on a hull facing 0 points along game angle 0x4000, as the rules' shell (facing + turret) |
| tank `treads_brakes_flags_and_props`, `the_shell_points_along_its_flight` | the scroll's wrap; braking both ways; flags by slot; the placements; the nose along yaw and vy/speed |
| tank `the_camera_follows_the_hull_and_the_scope_sits_on_the_turret` | V t1: 310/159/88.5 at rest, 400/135/52 at full speed; V pw2: the eye 27.5 along the turret, 111.6 up, level; the cut back at R's release |
| hud `tank_matches_the_originals_draws` | **pw1** DL 106 boost `atlas2` [0, 25, 22, 14], 190 scope [0, 16, 22, 9], 250 shells [0, 39, 22, 12], 346 shield [0, 4, 22, 12], all at [260, 5]; none without (s0 200). **pw2** DLs 172-195: the back [52, 46, 8, 124] alpha 0x41, the ring's quarters, the bar [54, 67.5, 4, 80.75] (ours 67.34 and 81.3: unrounded), no dots |
| audio `tank_sounds_follow_the_engine_shells_and_bomb` | 1191 at the spawn, 1192 + a gear speeding up, 1189 coasting, the rocket's blast on a hurt tank, 742 22 frames after a death, the engine stopped, 11/12 and the countdown song together |
| lib `bodies_by_map_and_class` | class 8 on 24 draws 119 |
| Live, `shots.sh` (audio debug) | 1191 per tank at the start, 1192 + 1188 driving (read 30), 699 at the shot (101), 693/702 + 750 at its ground blast (121), 12 at the bomb's take (494), the countdown song at the plant (524), 1231 at 30 left (795), 735 twice at the blast (824); power-ups 1296, 1187, 385 (c) |
| `bun run test` | pass (34 test binaries, 0 failures) |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |

**HUD findings** (the oracle's DL walks: 1 human and 1 bot, one screen). Tank's HUD is Deathmatch's (health in two
rows of 3 pieces, the score and rank, the clock, the radar), plus:
- the power-up's icon at the weapon icon's place ([260, 5], right/top) from `atlas2`'s first column, as above. No
  count or timer: the shells' 3 and the shield's time are not drawn;
- the scope: the back, the ring and a fixed bar (no zoom on the tank's scope), no dots;
- the countdown digit (centred at 150, 86) and the gas haze are Total War's draws (same command, same two IA8 layers).

Screens (`../lab/screens/tank/`, 1280 × 720):
- **The tank:** `driving.png` (slot 0's view and HUD, braking: the brake lights lit), `turret.png` (turned 90°),
  `firing.png` (the shell leaving), `blast.png` (its puffs on slot 1).
- **The bomb:** `bomb-carried.png` (over the carrier), `countdown.png` (the carrier in bunker E, the digit 4).
- **The doors and gas:** `doors.png` (bunker E's doors shutting, from outside), `gas.png` (outside at 7 frames
  before the blast: the haze, the digit 1), `wreck.png` (an outsider's wreck after the blast).
- **The HUD:** `hud-power.png` (the shield's icon), `scope.png` (the scope's view: back, ring and bar).

**Gaps.**
- **Item 6, cs5's op 9:** a no-op. The pack's outro leaves op 9's texture swap out (`conker-extract/src/outro.rs`);
  the headlights' texture it swaps in is already the glTF's own.
- **Pack lane:** the tank's icons and scope bar could move into `hud.json` (e.g. a `tank` block: the four icons and
  the bar); they are constants in `hud.rs` now.
- **I, the camera:** the heading's follow is fitted to t1 (86° → 21.6° in 20 frames standing, about 35° behind
  turning at speed 39); turning in place the original's spring lags more. The wall pull-in is the remake's. The scope's look is level: the gun's pitch aims high at far targets and is
  not followed (V pw2: level, no target).
- **I:** the carried bomb's place and size (item 0x18's look is not found); the shells' blast (the strike's puffs:
  `func_15137610`, the impact, is unread; V bm1 363's puffs share their colours); the tank's clip rate 1.
- **Not done:** the treads scroll as one (the original's sides differ by the turn: ±Δfacing/70); the hull does not tilt
  with the floor; the engine's pitch by speed (`|speed| · obj+0xB8 / 2`, a running voice's rate is fixed here); the
  fire's smoke (`func_15102B38`, 5-9 puffs: C); the rumble (`func_151C1FB8`: C); a shield effect (none seen in pw2's
  draws); the tank's lights (not in `lights.json`).
- **Live notes:** the power-ups are dealt to the six spots at random per run (`shots.sh c` finds the scope's taker in
  the server log). A pickup's sound plays at the player nearest the pickup's current spot, which for a roaming pickup
  may already be its next one.

## Raptor: rules, server and bots (2026-10-13)
**Batch M6, the rules half. `--mode raptor` on `--map 3f` plays the original's War: Raptor (scenario 2): the cavemen
(team 0, class 0xB, the club, the Ugas' camp) against the raptors (team 1, class 9, model 83 at 0.7, 6 hp, west by
the nest). A human asking for the Tediz plays a raptor; bots in slots 2 and 6 of each 8 are raptors. Every kill is +1
for the killer's team, a body dropped in the nest +1, an egg thrown into the cavemen's fire +2; the match ends at
`--score` (1-40, default 20) or on the clock (99 lives); untimed, lives count and it ends as Heist's. The raptor runs
its own frame: it speeds up 1 a frame to 35, brakes, turns 1024-768 a frame, jumps (y velocity 70). Its standing bite
(Z or B, clip 49) kills every caveman in its area on frame 14 and leaves the body in its jaws. At full speed the bite
is the running one (clip 53, 3 damage, knocked up). B or Z drops the body (clip 8). It takes nothing. Cavemen take the
3 eggs from the nest, carry them at the bag's pace and throw them into their fire (clip 0xD6); a hit or a death
resets the egg. The cooked eggs show in turn. The bone crossbow (0x41) and knives (0x3F) hit for 1; the 10 pickups
are back 120 frames after a take. Script 0xBE, run by conker-bots, does the eggs, the nest, the hazard and the bots.
Protocol 29.** Research: `../lab/research/mp-original/raptor.md` (runs r0-r12, b1, b2), `raptor-scratch/`.

| Part | What |
|---|---|
| conker-sim `raptor.rs` (new) | **`step(p, input, world, others, drive)`:** the raptor's gait. Target = stick · 35/75, +1 a frame (`TOP` 35), braking 1.8 while that leaves 15, then 1.1, 0 under 1. It turns toward the stick at 1024 a frame up to speed 21, then down to 768 at 35. States 0/1/10 (idle, slow, run). A jumps (state 51, y velocity 70, `fall` as r1's steps, 56 frames, 3 landing frames, then 0). `Drive::Hold` (a bite, a hurt) and `Drive::Lunge(n)`, the running bite's root motion (`LUNGE`, −2.5 a frame). **`body`:** radius 124, centre 126 up, scale 124/203 (V b1's obj+0xE4/E8/EC; the cavemen gather 145-150 from it, as b1's) |
| conker-rules `raptor.rs` (new) | **`Match::raptor_player`** in place of the on-foot frame. Z/B when free on the ground: the drop with a body, the running bite at 35, else the bite. The bite turns toward an enemy within 60° and 500 at 1100 a frame until frame 14. **`bite()`:** the area (`in_reach`, (224, 84) ahead standing, (140, 154) after the lunge) hits every enemy in it, 4 frames, once. A hit plays 54 (club, 13 frames), 12 (17) or 23 (the hazard's hop, to the landing), with no knock-back. A death plays 47 then 48 from 59, and `down()` at 119 with clip 19. **Eggs:** `touch_eggs` (a living caveman's body over one, as the bag's), `hold_eggs` (40 ahead, 38 up), `let_go_egg`. **`raptor_op`** applies script 0xBE's `Op`: Egg made/gone, LetGo, Face, Throw, Nest (clip 50, the baby eats 156 frames), Cooked. Consts `CLASS` 9, `CAVEMAN` 0xB, `SCORE` 20 |
| conker-rules `game.rs`, `damage.rs`, `colors.rs`, `weapon.rs`, `gas.rs` | `Settings::raptor`; `Fighter::prey`, `hitbox()` (`Hitbox::RAPTOR` 125/202/125 for class 9: every area test, bullet and bolt), `other()`; `Match::raptor`. A lethal enemy hit is +1 to the attacker's team (`colors.scores`); the end and the standings are Heist's with those scores; 2 teams. Pickups refuse class 9. The egg is `flag` 3 (B does not drop it; the carrier is heavy and plays 0xD8/0xD7/0x2E4). Damage types `BITE` (0x46, kill → clip 0x170 87 frames), `RUN_BITE` (0x1C, 3), `HAZARD` (script type 0x3E). Poses `Bite`, `RunBite`, `Drop`, `Nest`, `Hatch`, `EggThrow`. `BONE_CROSSBOW` (draw 0x2C8, fire 0x2C9), `BONE_KNIVES` (0x2CA/0x2CB/0x2CC, a bolt of `SHOT_BODY` at frame 8, 14-frame cycle), `PICKUPS_3F_RAPTOR` (placements 15-24). `Effect::Raptor(Op)` |
| conker-bots | 0xBE's natives: the baby's and eggs' handles (`obj`/`obj2`), an egg's 0xBF and 0x97, player queries 0x95 (carries egg k), 0xB2 (a body in the jaws), 0xD1 (hit since the last read, read and cleared), 0x97 (x, z packed); commands 7 (an egg let go), 1 (+0x25C 0x10: the throw), 0x10 (face), 0x14 0x28 (the nest), the cooked eggs' 0 and 2. **Raptor bots:** class 83's behaviours run as Conker's (`RAPTOR_BEHAVIOURS`: 0x1F→7, 0x1A→1, …); 0x22, the bite, presses Z once free and facing the target within 30°, else turns to it. The count-2 role pick keeps raptor bots Attackers: they never feed (kept, as the original) |
| conker-server | `--mode raptor` (map 3f, no `--cavemen`, `--score` 1-40, no `unlimited`). Teams by side; class 9 for the Tediz' side, 0xB for the cavemen; all hold the club (a raptor never draws it, V: the slot reads 0xC0); the raptors' base is `starts.sides[1]`. `arena()` takes `raptor.json`'s 10 pickups. The script flag; `respawn_in` = 119 − step for a raptor; the team score in `Combat::score`; `Fighter::hitbox` for the lag-compensated bullets. **RAPTOR lines:** the script's ops (not Face) and each score change. `bots.sh` MODE=raptor (+ `SIDE=` for client 1) |
| conker-sim `net.rs` | Protocol 29: `MatchView::raptor`, `raptors: RaptorView` (below) |
| conker-client | `match_settings` copies `raptor`, so a local raptor is predicted on its own frame. Nothing drawn (M7) |
| web | `data.ts`: mode `raptor` ("War: Raptor", lives, score 1-40, default 20, no unlimited); map `3f` plays Deathmatch and Raptor |

| Check (2026-10-13) | Result |
|---|---|
| sim `raptor` (4) | r1: the run to 35 and the braking, the turns (1024, 768 at 35), the jump; r8: the lunge |
| rules `raptor` (5) | r2/r5/r6: the standing bite kills on frame 14 within its area, misses ~30° off; r8: the running bite's 3; r7: death, `down()` at 119; r4: an egg taken, carried, thrown (+2); a kill to the target ends the match |
| bots ROM `raptor_bots_play` | 7 bots, 9000 frames, seed 1: [21, 11], bites [25, 16], 5 body drops, no unknown natives, no faults |
| server `raptor_teams_and_outros` | the flags, sides, teams, classes, the club, the records, the outros (team 0: cs2 765, team 1: cs4 415) |
| `bun run test`, `bun run check`, wasm clippy | all pass |

| Run (cavemen-raptors) | Result |
|---|---|
| oracle b1, b2 (V) | 14-20 at 8510 (raptors); 20-17 at 12037 (cavemen) |
| ROM test, seeds 1-4 | 21-11, 20-18, 21-16, 20-13 (cavemen) |
| server `bots.sh 450` (MODE=raptor MAP=3f, 7 bots + 1 idle human), seeds 1-3 | 20-7 at 8478, 20-6 at 8179, 20-9 at 8159 (seed 3, `SIDE=tediz`); every bots.sh check ok, no panics. Per run: 9 eggs made, none thrown or cooked, no nest drops (as b1: no Collector took an egg; raptor bots never feed) |

**M7 handoff (the wire, protocol 29).**
- `MatchView::raptor`; `score_limit`, `scores` ([0] cavemen, [1] raptors), `winner` 0 or 1; the outros by winner
  (the pack's cs2 and cs4).
- `MatchView::raptors` (`RaptorView`): `eggs[k]` = (shown, carrier slot, pos; a carried egg's pos is already 40
  ahead and 38 up of its carrier); `cooked[k]` (shown at, else `None`; props 0xFF − k); `fed` (the nest drop's
  tick: the baby eats `raptor::EATS` = 156 frames from it); `prey` (bit n: slot n has a body in its jaws, model 22
  on the rig's `carry` joint); `drops` (pos, facing, tick: a dropped body, gone `raptor::DROPPED` = 39 frames on).
- Per player: `Combat::class` 9 is a raptor (model 83; `Combat::weapon` still names the club: do not draw it);
  `Combat::anim` carries its clips (0 idle, 1 slow, 10 run, 51 jump, 49 bite, 53 running bite, 8 drop, 50 nest, 54/12/23
  hurt, 47/48 death, 19 respawn) and the egg carrier's (0xD5 grab, 0xD8/0xD7/0x2E4 carry, 0xD6 throw); the victim of a
  bite plays 0x170. `Combat::flag` stays `None` for an egg (the egg's carrier is in `RaptorView`). `respawn_in` counts
  a raptor's 119 frames. The raptor's camera (raptor.md §2.2: ~342 behind, ~150 up at rest) is the client's.

**Gaps.**
- **I, the bots' bite:** class 83's behaviour 34 (the bite) is not in the pack. The bot turns to its target and presses
  Z once within 30°. The oracle's bot bites are often cut short by its next move (b1: clip 49 restarted at 12-frame
  steps), while ours run to the end. Our cavemen win every run; the oracle's raptors won b1.
- **I, the server's raptors score half the ROM test's** (6-9 vs 11-18, fewer bites a match: ~30 vs ~41 per 9000
  frames). Ruled out: slot layout, the human's order, debug vs release, the camera volumes (the ROM test with them
  still gives 10-17). Not ruled out: the server world's `roots`/`sink`. Also, the cavemen's friendly fire leaves
  victims immune (10 frames), and a bite on them is refused.
- **I:** the standing bite's near edge (the sphere puts it at 104; no V under 144). The lock-on's cone and range (60°,
  500). No knock-back on a raptor, and a hit cuts its bite short. The egg's touch is the bag's radius 29. A thrown egg
  splats the frame after the throw. Untimed, the end is Heist's.
- **Pack lane:** `outros.json`'s cs2 and cs4 last 765 and 415 frames (`length / 2`), the oracle's 750 and 375.
- **Item 8, the pistol's head:** the code confirms it. `func_150FED30` builds its bullets with `func_151C229C`'s arg15
  = 1 (flag 8: parts tested); `func_150FE860` with 0 (C). V e_w39: its bullet pops the corpse's head (1278, read 970).
  `func_150FE860` is most likely the tommy gun's (I). The comments are fixed, but the head test stays off for the
  pistol: the remake's head is a height (`BULLET_HEAD` 47.25, fitted to the twin guns), which would turn e_w39's level
  body hits at +48.6 (2 each, V) into head hits. It goes on with the model's head part (part 0xA).
- **Not done:** the jump zones (triggers 4-7) are the script's A presses, not checked against the oracle; a feed by a
  human raptor and an egg delivery are not run live (the bots never feed, and no Collector took an egg in the runs).

## Map 0x33 and the racers: the pack (2026-10-13)
**Batch M12, the pack half. The pack has map 0x33 (Race's map, scenarios 11 and 12) like the other maps, with its four
route barriers solid, the 7 power-ups, 17 spawns (the grid), nav and `ai.bin` (script 0xBC). A new `maps/33/race.json`
holds the two routes (the barriers each shows, one lap of nav nodes, the overview map), the racers' characters by
class, the board and the helmets. Character 144 (the bots) is packed. The board is a held model on the rider's foot
joint, in `maps/33/props.gltf` with the helmets. Race's sounds are cues; Race has no music. No rules, server or client
change: M12's rules half and M13 play it.** Research: `../lab/research/mp-original/race.md` (§M12), `race-scratch/`;
this batch's `race-pack-scratch/` (oracle `ConkerRecomp.m11`): `lap.py` (the lap walk), `audio.sh` →
`../lab/audio/captures/race_{a,b,c}_calls.txt`, `music.sh` → `../lab/audio/map33/`, and `runs/`. The runs: k1 (the
kick and the board, DLs 250/252/390), cls (the helmets by class, DL 60), m1 (the missile, DLs 222-232), h1 (2 players,
DL 230), and the HUD walks `hud-k250`, `hud-h230`.

| Part | What |
|---|---|
| extract-scene | `deathmatch_solid(0x33)` = the barriers 0-3 (V L1, L2: `+0x4F` 0x01; the power-ups have 0x21). 56 of the 1849 collision faces, 14 each, as the layers `{placement: 0..3}`; each scenario hides two by `+0x6E` (`race.json`'s `shown`) |
| extract-character `held.rs` | `action_records(game_data, action)`: an action's 16-byte records, used by `held_models` and by Race's models, which a script makes |
| conker-extract `lib.rs` | `RACE_MAP` (0x33), `RACER` (144), `RACE_AI` (0xBC). With 0x33 in `--map`: 144 on the Tedi's rig (the Tedi's clips, lift 49) after the raptor; its `looks` hide list 1 on class 0xF (obj+0x94 = 2, V cls). `race_attached`: action 0x79 (the board, bank-09 #0x5D on slot 23) and the helmets 0x76, 0x78, 0x77 (#0x5A, #0x5C, #0x5B on slot 12) → `maps/33/props.gltf`, 4 scenes at the origin. `race_mode()` → `race.json`: the barriers; the routes (scenario 11 shows 0 and 3, uids 0xFF/0xFC, `race_map_low`; 12 shows 1 and 2, `race_map_high`); each lap is `func_150888A8`'s walk from node 0x13 (previous 0x12), 33 nodes; the racers (0xB → 136, 0xC-0xF → 144); the board; the heads (0xD, 0xE, 0xF). `pickup_table(0x33)`: canisters `[3, 100]` at placements 4, 5, 9 give 0x31, missiles `[3, 101]` at 6, 7, 8, 10 give 0x33, respawn 18 VIs. `ai_script(0x33)` = 0xBC. `HELD_CLIPS` + the kick 0x354: `held.json` gains #0x34 (the fist on slot 9, t5-t30) at index 11. The missile is `projectiles.gltf#Scene4` (#0x33) drawn at 1.0 (V m1 DL 224) |
| conker-extract `audio.rs` | `race_cues()` (below); `clip_sounds` keeps 0x354's swing (90 at t11) on 136 and 144. `map_sounds("33")`: no emitter, zone or song |
| conker-extract `hud.rs` | `race_map_low`/`race_map_high` (blobs 0xCF1, 0xCF2, IA8 64 × 64). Weapon icons: 0x33 is `atlas` [50, 39, 14, 12] (the DL clamps at the atlas's edge); 0x31 is `atlas2` cell 6, [0, 28, 22, 14] |
| conker-assets | `race.rs`: `Race` (`barriers`, `routes` of `Route { scenario, shown, lap, map }`, `racers`, `board` and `heads` as `Attached { scene, joint }`) and `race_json`; `load` refuses a shown non-barrier and a lap that passes a node twice. New `On` variants: `RaceHum { hum }`, `RaceCountdown { stage }`, `RaceMissile`, `RaceWall`, `RaceKick`, `RaceDeath { kind_e }`; the sample covers them |
| Tests | `tests/rom.rs` `race_map_and_racers`: 120 nav nodes; both laps verbatim; shown, uids; the pickups; 17 spawns; AI 0xBC; the attached models; the board's 64 vertices; 144's 28 joints and 2 lists. `tests/pack.rs`: 0x33 in the read-back (ROM path, solid rule, 17 spawns). A Race block: `race.json` = `race_mode` byte for byte; no lights, lifts or kill gates; respawn 18; barrier faces; 144's clips = the Tedi's; looks; props.gltf's nodes. HUD icons and maps; `race_audio_reads_back`. conker-assets: the round trip and the refusals |
| `package.json` | `pack` + `--map 33` (no `--background`: no music) |
| Pack | Re-exported (`--force`, the new `pack` arguments). New: `maps/33/` (63 textures, 1849 collision faces, 17 spawns, 7 pickups, 16 triggers, nav, `ai.bin`, `lights.json` empty, `props.gltf`, `race.json`), `characters/144/` (6 textures), Race's WAVs in `audio/common`, `hud/race_map_low.png`, `race_map_high.png`. Changed: `weapons/deathmatch/held.*` (21; indices from 11 up shift by 1), `audio/events.json`, `sounds.json`, `hud/hud.json`, `pack.json`, `README.md`. Removed by hand, stale since they moved to `common` (now played by more than one owner): `audio/characters/119/0750.wav`, `0752.wav`, `audio/characters/136/` (1051-1054, 1061, 1063, 1067-1070) |

**The board.** It is bank-09 #0x5D (64 vertices), the one record of action 0x79: an attachment on matrix slot 0x17,
joint 23, a foot. Script 0xBC's player command 0x78 → `func_15097A8C` creates it at each spawn (C). It goes at the
death and comes back at the respawn (V k1). It is drawn in joint 23's frame with no offset of its own, as a held weapon
(V k1, b2, cls). It is part of neither 136's nor 144's model, and it is not an object of its own. The helmets work the
same way, on joint 12 by class: 0xD #0x5A, 0xE #0x5C, 0xF #0x5B, 0xC none (V cls).

**Race's sounds** (C as noted in `race_cues`, V race_a-c):

| Cue | Sounds | Volume | Near/far |
|---|---|---|---|
| hums 0, 1, 2 (records; the law in `On::RaceHum`) | 1459, 1460, 1468 | 1000 | 200/1000 |
| countdown beeps 1 and 2; go | 1671; 1672 (unpositioned) | 24000 | 0/0 |
| power-up taken | 1577 | 32767 | 100/1000 |
| canister used | 1576 | 32767 | 500/1000 |
| missile fired | 567 | 30000 | 500/2000 |
| missile in flight | 750-752 | 30000 | 500/1000 |
| missile hit | 263 | 28000 | 300/1800 |
| kick hit (the swing is clip event 90) | 1015 | 28000 | 300/1800 |
| wall scrape | 778-785 | 28000 | 300/1800 |
| blow-up; over kind 0xE | 1224; 1076 | 32767; 32000 | 700/2000 |
| blow-up's blast; debris | 694-695; 689-692 | 32000 | 1000/3000; 700/2000 |

There is no music: song11b saw no song or MP3 start, and `../lab/audio/map33/quiet.wav`, with the hums muted, has
silent stretches. A jump, the finish and the results play nothing (V race_a, race_c).

| Check (2026-10-13) | Result |
|---|---|
| Oracle k1, b2, cls | the board on joint 23 from each spawn to the death; the helmets on joint 12 by class; list 1 hidden on 0xF |
| Oracle m1 | the missile is #0x33 at scale 1.0 (DL 224) |
| Oracle race_a-c | every cue's id, volume and distances above |
| `lap.py` | the walk's laps = `race_lap`'s (33 nodes each) |
| conker-assets, conker-extract tests | pass (conker-assets 14, `tests/pack.rs` 6, `tests/rom.rs` 15, `tests/traps.rs` 1) |
| `bun run test` | 476 pass, 1 ignored (with the Raptor and Tank-client lanes' work in the tree) |
| `bun run check` | fails at `cargo fmt --check`, only on the other lanes' in-progress files (conker-bots, conker-client, conker-rules, conker-sim, conker-server). This lane's crates (conker-extract, conker-assets, extract-scene, extract-character) pass `fmt --check` and clippy `-D warnings`; `web/app` check passes |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1799 files |

**Gaps.**
- **I:** the hum law's height is obj+0x28, read as the height above the floor (C: the airborne test). The hums' halving
  when byte 0x800C35EA is 1 is not packed. 1224's volume, passed as −0x7D00, is read as full.
- **I:** the power-ups' swing of ±8 on +0 and +0x18 is read as a bob.
- **Not found:** a start/finish prop (0x33's 11 placements are the barriers and the power-ups). The map has no kill
  gates (death is no floor) and no lights.
- **Not decoded:** the results screen's layout (phase 6). The race HUD is not in `hud.json` (numbers below, for M13).
- **Bots lane:** `conker_bots::script(0x33)` still gives 0xBA; the pack's `ai.bin` is 0xBC.

**Handoff to M12's rules half (rules, sim, server, bots).**
- Read `race.json`: the route by scenario; the barriers not in `shown` are not solid (skip their
  `{placement: k}` collision layers); the lap is the order of nav nodes to pass.
- Pickups: `Gives::Weapon(0x31)` (canister, the boost) and `(0x33)` (missile), respawn 18 VIs; up to 3 of one type,
  and a new type replaces the count (race.md).
- `conker_bots::script(0x33)` → 0xBC. 144's clips are the Tedi's: ride 5 (30 frames, also jumping and steering),
  knocked up 0x54 (29), kick 0x354 (29: attack 0x15 at t11), dead 0x170 (88); bots play 0xF for one frame between
  kicks.
- Emit the cues: `race_hum` from the spawn to the death; `race_countdown` 1/2/3 at 60/30/5 ticks before the go;
  `pickup`, `shot` 0x31/0x33, `race_missile`, `hit` 0x33, `race_kick`, `race_wall`, `race_death`.

**Handoff to M13 (client).**
- Racers: class 0xB is 136 and 0xC-0xF are 144 (with `looks`). Draw `props.gltf` scene `board.scene` on
  `joint_<board.joint>` from the spawn to the death, and the helmets by class on joint 12. `held.json` index 11 is now
  the kick's fist.
- Barriers: draw only the route's `shown` placements in `level.gltf`.
- Power-ups: they spin +3.03° a frame. On a take they shrink 7 → 0 over 4 frames; on the return they grow 1.82, 3.64,
  5.46, 7.0, 7.16, 7.01, 7.0 (V L1 541-557). The missile is `projectiles.gltf#Scene4` at 1.0.
- HUD (V k1 DL 250, 1 view; h1 DL 230, 2 views):
  - health at the top left (`atlas` [49, 5, 15, 4] pieces), the lives pips at (7, 6);
  - the place ("1st", font + `rank_*`) centred at the top; "LAP 1/5" in the font at (10, 20);
  - the overview map at [226, 13]-[290, 77], TEXEL0 × ENV with env alpha 0x80, with 2 × 2 dots, one's own green and
    the others red (2 players: at [198, 14] and [198, 122]);
  - the race clock "0:01:63" centred near x 146, top 204;
  - the items at x 232, top 6, one icon per item, 18 px apart leftward.

## HUD and audio leftovers (2026-10-13)
**Batch E9. Through a scope the local player's body and weapon hide in its own view. The rank's ties follow the
original's rule. The death iris is centred on the dying player's head. Weapon 0x16's R view tints the framebuffer, and
the scope dots turn with the zoom. In a narrow window the page's lines keep clear of the HUD. Sounds: bullets whizz
past (800-802, 807-811), a cooked grenade hisses 929 in the hand, the grenade's landings draw from a shuffle bag, and
a lethal head shot pops (1278). Each hit carries its weapon (protocol 27), and the music comes back after RESTART. A
speaker that is voicing makes no radio call. 87 and 134 have their `character.json`.** Research:
`../lab/research/mp-original/hud-audio-leftovers-scratch/` (`hud/notes.txt`: runs scopeB, w16B, deathC; `audio/`;
`pack/`).

| Part | What |
|---|---|
| conker-client `outro.rs` (`show`) | The local player's model hides while its scope is up (C `func_1502E4C4`: the view's bit in obj+0x74 while s126+0x197 is set; V scopeB: from the frame the ring shows to the release), not with the free camera. A mounted gun and its gunner were already hidden (`guns.rs`) |
| `hud.rs` | **Iris:** `Shown::iris_at` is the head joint (`Twisted`'s joint 12) + 30 · scale up, projected with the 3D camera into the frame (`frame_point`); a blast victim (`BLOWN_UP`) uses its own place (C `func_1507E1D0`). The radius stays keyed on the frames before the respawn: the original's respawn is the death's start + 88 for every death (V deathC: a grenade at 181 → 269, a poked death's script command at 376 → 464). **Dots:** `dot_turn(zoom)` = (F + 50) · 0.02 · 2π, F = clamp(the mean of the two zoom angles, −50, 0); dot k at k · 30° + turn, (cx + 55 sin, cy + 55 cos), in quarter pixels (C `func_151CBBE0`, V scopeB DL 230). **0x16's R:** `Tex::Tint` (a multiply quad over the window at z 4, colour (R, 255, 0)), R = `tint(frames since open)`, a triangle wave 0x90 ↔ 0xFF by 4; the ring only, no dots or bar; under the clock, score and ring, over health, icon and radar (C `func_151A5170`/`func_151A5070`, V w16B) |
| `audio.rs` | **Fly-by:** `MatchView::bullets` (`BulletView`: pos, vel, life, born, owner). A new bullet is armed at its owner's weapon chance (0x09 8, 0x39 1, 0x23 255, 0x1B 50, of 256; none for 0x38); while armed, each step picks one of 8 sounds and its lead, and plays it once the bullet heads at the camera within its lead and its reach; the sound then follows the bullet (C table 0x8008FBE8). **Hiss:** 929 in place of 1299 once the holder's cook (`Combat::cook`) is under 120; it follows the thrower. The grenade's own fuse only from 60 past its birth. **Bounces:** 1447-1449 from `func_1000F568`'s bag (a permutation, never the last again at once) on each bounce up faster than 5. **Head pop:** 1278 on a lethal hit whose reaction is `DEATH_SHOT` (`func_151D4DAC`: a parts-tested bullet on the head under 3 health). **Hits** use `Combat::hit`; the client's guess (`hit_weapon`) is gone. **Music:** RESTART's fade stops it and clears `background`, so the next match starts it again. **Radio:** `voicing(slot)`: a radio line pending or one of the speaker's lose, flag, carry or plunger lines playing stops a new call (I). Everyone hears the radio in every War mode (C: no mode, map or team test), as before |
| conker-sim `net.rs`, conker-server | `Combat::hit` (the weapon of the tick's hit; 0 for a quiet head −1 above 2 health), `Combat::cook`, `MatchView::bullets: Vec<BulletView>`. **Protocol 27** (Tank's is 28) |
| conker-assets, conker-extract | `On::Head { weapon }` (the 0x09/0x39 lethal head cues), `On::FlyBy { chances, leads }` (800-802, 807-811 in `audio/common`), `bounce` 1447-1449 with the bag; the grenade's flight cue is gone. `characters/87/character.json` (12 clips) and `characters/134/character.json` (5): `characters("1a")` lists 87 and 134, and `heist.rs` reads the bag's clips from 87's. `hud/bubble_particle.png`: the decode is right (CI4 64 × 64, its TLUT a grey ramp with every alpha bit set; as IA16 the alphas are noise), so cs3's bubbles now draw added (I) |
| web `play.html` | `--hud` = min(100vh/216, 100vw/292): `#net` sits 35 frame lines up (over the clock, the strips and pips), `#sound` 69 down (under the radar) |

| Check (2026-10-13) | Result |
|---|---|
| client `the_scope_hides_the_own_body` | the own model hidden with the scope up, the other shown; both shown with the free camera and with the scope down |
| client `the_dots_turn_0x16_tints_and_the_iris_follows` | turn 177.35° at scopeB read 200's zoom (−27.8, −22.94), 0 at a mean of −62; tint 0x90, 0x94, 0xFC, 0xFF, 0xFB, 0x90, 0x94; dot 0 at [145.5, 50, 6, 6]; 0x16: one tint quad at z 4, the ring's 4 quarters, no dots or bar; the iris at (100, 80) |
| client audio | `bounces_draw_from_the_shuffle_bag`; `a_bullet_passing_the_camera_whizzes_once` (0x23: one of 800-802; 0x38: none); `a_cooked_hiss_and_a_head_pop` (cook 240 → 1299, 118 → 929; `DEATH_SHOT` → 1278, `HIT` none); the music after RESTART (Deathmatch and Colors); `a_flag_taken_calls_on_the_radio` with `voicing` |
| client `rows_colour_by_port_and_share_ties` | the tie rule (kills − deaths, fewer shots, shared places) |
| server `a_hit_names_its_attackers_weapon` | the attacker's weapon on each hit |
| extract `pack_reads_back_as_the_rom_path` | 87's table (12 clips, pair 0 the logical 0, 9, 10) and 134's 5 clips |
| Live e_w09 (`combat.sh`, port 5982, `RUST_LOG=conker_client::audio=debug`) | one 802 on the victim's client (read 980, the bullet's own voice); the body kill has no 1278; HIT lines carry 0x09 |
| Live grenades (port 5986) | an 80-frame cook hisses 929 from throw + 4, and the grenade's own 929 does not play; a 10-frame cook hisses 1299, bounces 1447, 1449, 1447, the grenade's 929 at 676, the blast at 736 |
| Pack | `bun run pack --force`: 1689 files; `lab/tools/verify-pack.sh`: pack equal |
| `bun run test` | 456 pass, 1 ignored (with the Tank lane's work in the tree) |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |

Screens (1280 × 720):
- `../lab/screens/hud/e9-scope-p1.png`: the sniper scope (0x24) at read 900, with no own body. The ring, the turned
  dots and the bar show, and the bear in front is player 2.
- `e9-rview-p1.png`: 0x16's R view with the yellow-green tint, the ring only.
- `e9-iris-p2.png`: player 2's iris at read 1060, 112 reads after its twin-gun death, centred on its corpse's head.
- `../lab/screens/heist/cs3-bubbles.png`: the added bubbles.

**Gaps.**
- **I, the bubbles:** the original's bubble draw is not in any dump (0x1207 is a caustic texture). 0xC01 is a white
  square with a dark rim, so cs3's bubbles still look like small white squares.
- **The dots' turn at the open:** the remake's camera zooms use the split screen's view angles (82.416), not the full
  screen's 60.6/50 (camera lane). So the scope opens at a turn of 136°, where the original's is 177°.
- **The iris:** the death durations are the rules' (clip death 133 frames against 134; bullet deaths are I).
- **Rules lane (not touched):**
  - a corpse's head pop (the original pops a dead head too);
  - the pistol's bullets test the head: its spawner is `func_150FED30` (arg15 = 1, V e_w39's 1278 at read 970),
    where `game.rs` (3400-3404) and `bots-fidelity.md` take `func_150FE860` (most likely the tommy gun's, I).
- **Not run live:** a mounted gun's gunner hide (by `guns.rs`, unchanged), and `#net`/`#sound` in a browser (no
  browser in this batch).

## Tank: rules, server and bots (2026-10-13)
**Batch M9. `--mode tank` on `--map 24` plays the original's scenario 1: everyone a tank (class 8, model 119, 6 hp),
free for all, ranked by kills as Deathmatch (time, lives and the kill limit as Deathmatch's). The hull drives as the
original (the stick's target ±48, +1.4 a frame toward it, half the speed moved, turning slower with speed); the turret
turns 3° a frame on C-left/C-right; the gun pitches itself (−20..80) onto the nearest tank in its cone. Z fires a shell
(296.5, 5/frame² gravity) that hits a tank within 150 for 1 damage and a 50 knock; the next press fires 34 frames on.
A dead tank leaves a wreck for 330 frames and is back on a record on its 88th frame (6 hp, 150 immune). The four
power-ups roam six spots (back 150 frames after a take): the shield (immune 900 frames), the boost (A doubles the
top speed for 300 frames of holding), 3 one-shot shells, and R's scope. Script 0xB8, run by conker-bots, does the
bomb as Total War's gas: shown on the centre post 300-450 frames in, touched and carried into a bunker, it plants a
300-frame countdown; the doors shut at 70 left, the gas at 30; at 0 every tank outside a bunker dies, the bomber
credited. The bots fetch it, plant it, take the power-ups and fight. Protocol 28.** Research:
`../lab/research/mp-original/tank.md` (runs t1, f1-f3, p1, c1, b1), `tank-scratch/` (the script's listing, the code).

| Part | What |
|---|---|
| conker-sim `tank.rs` (new) | **`step(p, input, world, others, drive)`:** the hull (`func_1505A3A8`). Target = stick y · 0.64 (boosting with A: 80 · 1.28); `speed()` +1.4 toward it (0.014 within 1), brake 2.0 (the fire clip's 5); the turn `facing − x · rate`, rate `max(10, (18 − v·0.5)·0.5 + 9)`, at most 1024 a frame, before the move; half the speed moved along the floor; tanks push (`body`, radius 78) with no damage; falls 6 a frame. `Drive::Knock`: a hit's knock (speed −0.1, ×0.9). C-right/C-left turn `turret` ±3°. `movement.rs`: `PlayerState::turret`, `pitch` (degrees; replicated and predicted); `push_body` |
| conker-rules `tank.rs` (new) | **`Match::tank_player`** in place of the on-foot frame: the hit clip (7: 13 knock frames, 15 in all) or death (immune 255, `down()` at step 88), the power-up's timer, boost and scope, the hull, a bot's turret closing 1/8 of the gap to its camera yaw, the gun's pitch (the best tank within 35° and 7000; `solve_pitch` less the hull's), the shell on a Z press (`ready` + 34). **`fly_shells`:** half the speed a frame, `y += vy/2 − 1.375`, `vy −= 5`; `hits_body` with `BODY` (radius 150); gone at a wall. **`tank_hit`:** 1 damage (all with 0x10), knock 50, immunity kept (not by the bomb, clip 6); the wreck. **`touch_bomb`**, **`doors`** (8 a frame over 350 while +0x80 is 0), **`seat`** (a shut door collides), **`bunker`**, **`wrecks()`**, `CLASS` 8 |
| conker-rules `game.rs`, `weapon.rs`, `spawn.rs` | `Settings::tank`, `Fighter::power`, `Match::tank`. `Gives::Power(bit)`: a touch sets the bit and its timer (1800/600 VIs, 3 shells). The script's effects: Show/Set reach the bomb (placement 12) through `prop_mut`; `Countdown` from 0 plants in the last carrier's bunker; `Hit` is the blast (lethal, ignores immunity, the bomber's kill; its own death no suicide); `Carry` is 0x4B/0x4C. `TANK_START_24`: slots 0-3 on records 0-3, 4-7 on 0x10-0x13, respawning anywhere |
| conker-bots | **`script(0x24)` = 0xB8; `class(0x24)` = 119:** the tank's own class data (`[0x0F, 119]`): its script 4 walks the path at 33, 5 waits, 6 chases. On Conker's class (0) the 0x60 command's script 4 is a chase at 10, and the bots stood still. **Natives:** query (3, 0x21) on the bomb (0x3005: `touched[2]`), player query 0xA5 4 (alive), commands 0x4B/0x4C (carry on/off), 0x70 and (0xF, 0xE) no-ops; the countdown is G+0x2008. **`tank_drive`:** the walk target as the hull's stick (x = −gap/18, forward within 45°, y = speed / 0.64: 52 for 33); the turret's target is the script's enemy (G+0x11C) alive, within 4000 and in sight, led by its last step · distance / 100; Z pressed once ready and within 5°; each shot draws the aim error s126+0x12A in −2..2, 10° a unit |
| conker-assets, conker-extract | `maps/24/class.bin` (the tank's `[0x0F, 119]`, 1280 bytes) in place of `game/class.bin` on map 24; `pack_data` packs it; `Gives::Power` packs as the weapon bit. Copied into `../assets/maps/24/` from a fresh `pack --map 24` (the other files identical) |
| conker-server | `--mode tank`: map 24 only (it plays nothing else), no `--cavemen`, ≤ 7 bots; one side, class 8, each its own team, on `TANK_START_24`. `arena()` maps the pack's weapon bits to `Power`. The script flag (the AI runs with no bot). `tank::seat` once the effects are in. `combat()`: `respawn_in` = 88 − step; `power`, `power_left`. **TANK lines:** bomb taken, planted (by, bunker), blast kills (bomber), power-up taken. `bots.sh` MODE=tank: bombs taken and planted, blast and shell deaths, power-ups |
| conker-client | `match_settings` copies `tank`, so the own tank is predicted on the tank's path (without it the on-foot hurt indexed clip 7's sequence and panicked). Nothing drawn (M10) |
| web | `data.ts`: mode `tank` (kills, lives, no score, ≤ 7 bots) and map `24` ("Tank 0x24", Tank only) |

| Check (2026-10-13) | Result |
|---|---|
| sim `tank` (7) | t1: drive 48 at +1.4, brake 2.0, reverse, the turns (1024 at rest, 991 at speed 4, 619 at 39); p1: boost 102.4; c1: the fire clip's ±5; f1: the knock 24.95, 44.91, 40.33; push without damage; the turret ±3 |
| rules `tank` (6) | f1: a shell hits a tank 600 ahead on its third frame, 1 damage; f2: 150 hits, 200 misses; f3: refire at 34, no fire held; c1/f1: death → back at 88 with 6 hp and 150 immune; p1: the shield's 900 frames; the doors' 44 frames |
| bots rom `tank_bomb_as_c1` | the real 0xB8 with two humans: shown at 300, P1 touches, carries into bunker E: plant 328, doors shut +231, gas +271, blast +300 (P2 dies, P1 +1), doors open +389, bomb back +424; no unknown native |
| bots rom `tank_bots_bomb_and_fight` | 7 bots + 1 idle human, 9000 frames (seed 1): 6 takes, 3 plants, 16 blast deaths, 20 shell deaths; no unknown native, no fault |
| server `tank_mode_records_and_class` | the flags' refusals; records 0-3 then 16, 17; class 8; own teams; masks all ones |
| `bun run test` | 453 pass, 1 ignored, 1 fails: conker-extract `pack_reads_back_as_the_rom_path` line 468 (`characters/87/character.json`, the HUD and audio lane's change in progress) |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |

**Runs (`bots.sh`, MODE=tank MAP=24, 7 bots + 1 idle human, ports 5970-5976) against oracle b1 (620 s, 7 bots + 1
idle human).** Every bot moves, fights, dies and respawns; the client sees each move; no fault, no unknown native, no
panic.

| Run | Bombs planted | Blast deaths | Shell deaths | Power-ups | Kills (slots 0-6; the human 0) |
|---|---|---|---|---|---|
| **b1**, 620 s | **7** (6 bomb carriers) | **34** | **23** | **27** | 0, 11, 8, 12, 6, 6, 4, 10 (57) |
| 600 s, seed 1 | 6 | 30 | 40 | 21 | 12, 5, 14, 8, 5, 10, 14 (68) |
| 600 s, seed 2 | 4 | 18 | 45 | 26 | 18, 7, 5, 7, 11, 9, 5 (62) |
| 300 s, seed 3 | 2 | 11 | 21 | 10 | 1, 4, 1, 6, 6, 3, 11 (32) |
| 300 s, seed 4 | 2 (7 takes) | 9 | 17 | 7 | 0, 1, 2, 9, 8, 2, 4 (26) |

The 600 s runs logged no takes (the line was added after them). The bombs, blasts and power-ups are b1's (blasts kill
about 5 a bomb as there); the shells kill about twice b1's. A 3-minute match (`TIME=3`, seed 5, 260 s) ends on the
clock (t 5636, places by Deathmatch's rule), shows the results, starts the next match at 6893, and its bots play on
(a power-up at 6943, the bomb taken at 7617); no panic.

**Gaps.**
- **I, the bots' gun:** the original's fire path for tanks is not found. s126+0x129 is taken as the script's enemy
  (G+0x11C); the 4000 range, the sight test, the 5° gate and the error's −2..2 draw (10° a unit is C) are the
  remake's, tuned to b1. The shells kill about twice as often.
- **I:** the hull's radius 78 (fits p1); stick 52 for the class scripts' 33 (33.3); the bot turret's lead (its last
  step · distance / 100 for the C speed · distance / 200).
- **C, not done:** the class scripts' ops 0x8A, 0x03, 0x61, 0x70, 0x9B, 0x3B and 0x1A are skipped (`Npc::command`'s
  default); the pitch error (×400); A and R are never pressed by bots (C: as the original).
- **I, the ranking:** Deathmatch's (kills − deaths, then fewer shots); tank.md has the ranks undecoded, and b1's
  winner (12 kills, 6 deaths) wins either way.
- **Open (tank.md §8):** the scope shot's damage and range (scoped shells are plain); whether the shield stops the
  bomb (here not); the +5 speed kick after firing while moving; untimed lives and the kill limit not compared.
- **Not done:** the wrecks do not collide; `hit_weapon` names no weapon for a shell (0).
- **The pack:** its `format` is not bumped for `maps/24/class.bin` (a pack without it fails to load map 24 only);
  `../assets/README.md` is not regenerated (the row is in `pack-readme.md`).

**Handoff to M10 (client).**
- **Wire (protocol 28):**
  - `MatchView::tank`; `MatchView::tanks: TankView`:
    - `shells` (pos, yaw, vy/speed);
    - `bomb` (shown on the post at `tank::POST`), `carrier` (slot), `planted` (bunker 0-3 while the countdown runs);
    - `wrecks` (slot, pos, facing; placement 18 + slot).
  - The doors are `lifts[0..8]` (0 shut … 350 open, sliding apart along z); the countdown and the gas are
    `countdown`, `gas`.
  - `Combat::power` (0x80, 0x01, 0x10, 0x02) and `power_left` (VIs; shells for 0x10).
  - `Combat::anim` is the tank's clip: 0 idle, 1 fire (34 frames), 6 bomb death, 7 hit or shell death.
  - `Combat::class` is 8.
  - `Player`: `facing` (the hull), `turret` (degrees from the hull), `pitch` (degrees over the hull's floor pitch),
    `xz_speed`, `target_speed` (braking while below the speed).
- `body()` maps class 8 to the side's body for now. Draw character 119 with the pack's `tank` parts.
- The HUD's power-up, the countdown and the haze as Total War's gas.
- Engine and fire cues per the pack section's handoff.

## Heist: the client (2026-10-13)
**Batch M4, the client half. The client draws and plays War: Heist:
- the HUD: each team's "$" pips, the carrying team's "+1" blinking with the bag icon; skulls for empty or out teams;
  the radar in Heist's team colours, the free bag a steady white blip, the carrier's blip blinking opposite the icon;
  no weapon icon while carrying; the clock white;
- the bag (character 87 at 0.4): its clip per state, in its carrier's `joint_09`, over its shadow, hidden in the
  outro;
- its clip events, the throw's sound 91, the bases' hum stopping on leave, and the script's voice lines, now on the
  wire (protocol 26);
- the outro: cs2 if a human's team won, else cs3, with cs3's REDS shirt, roll and pitch, cement shoe, light and
  bubbles; the props' scale;
- STATS: the header, six columns with the bags second, rows in team colours, no strip.

Web: nothing (it already offers Heist).** Research: `../lab/research/mp-original/heist-presentation.md`,
`heist-presentation-scratch/` (`hud/`, `bag/`, `outro/`).

| Part | What |
|---|---|
| conker-client `hud.rs` | **`Shown::heist` (`Bags`: scores, limit, alive, carrying):** each team's pips (`pips.png` [14, 16, 7, 7], 7 × 7 at y 200, the team's colour or grey 404040); the carrying team's "+1" and the bag icon ([142, 3, 12, 11]) while `flag_icons(vi)`; a team with nobody in play draws its skull (atlas2 [48, 20, 16, 16], tinted). **`shown()`:** the radar's blips in `hud.heist.radar`'s colours; the free bag white (`hud.heist.cash`); the carrier's blip masked while the icon shows; weapon 0 while carrying; no kills or rank. **STATS:** `rows`/`results` with `heist` rank by `teams_places(…, 4)`, columns at 89 with the bags second, Heist's icons and label, rows in team colours |
| `heist.rs` (new) | **`clip(state, k, carrier, slid)`:** the bag's logical clip and time per `CashView::state`. Grab: clip 6, then 5 at the hand from 10 in, held at 8; then the carrier's idle/walk/run times. Thrown: 9, held at 8; landed: 9 then 10. Let go of: 7, then 8 at 0.8. Walking: 3 at 1.3. Standing: 0 for 100 frames, then 4 once. **Logical → glTF pairs** `[0, 1, 3, 2, 4, 6, 6, 7, 8, 0, 0]` (C: the glTF's `logicalIds`). **`Bag`** (resource): state, since (this client's tick), the clip this tick and last, where it lies or whose hand. **Systems:** `advance`, `add` (model plus a shadow disc), `ready`, `draw` (free: at `CashView` over the shadow; carried: `joint_09` at the bag's own scale, its clip's root placing it, V). `BAG` debug lines on state changes |
| `audio.rs` | **`bag_passes`:** a clip event fires when the clock passes it, from `was` up to but not `is`, round a loop. **`Sounds::heist`:** 87's events at the bag, the throw's 91 as it leaves the hand, its sounds stopped when it goes. **Voice lines:** `MatchView::voice`: the line alone, centred at its own volume for the speaker, 3D for the others. **Zones:** a `stops_on_leave` zone stops the tick after the last player leaves |
| `outro.rs` | **`key()`:** Heist's 0 (cs2) when `human_won`, else 1 (cs3). **`team_mode`** includes Heist. **`place()`:** roll about the forward axis (channel 6) and pitch about the side axis (channel 8). **Also:** `Prop.scale`; `OutroLight` (the vault's white light from its `t`). **`wear`:** an actor's team shirt (`Actor::team`) via render's `TeamLooks`/`wear`. **`bubbles`:** one a frame while an emitter runs, as traps' `Puff` |
| `render.rs`, `traps.rs`, `lib.rs` | `TeamLooks` and `wear` shared with the outro (`dress_teams` uses them); `Puff`, `material`, `puffs` `pub(crate)`; the carrier's walk (e/6 + 0.4) and run (e/26 + 0.5) clip rates |
| conker-sim `net.rs`, conker-server | **`MatchView::voice: Option<(tick, slot, id)>`:** the tick's last `Event::Voice`, kept until the next. **Protocol 26** |
| `lab/screens/heist/` | `shots.sh` (groups a-f, ports 5970-5979), `carry.txt` (slot 0: B at 290) |

| Check (2026-10-13) | Result |
|---|---|
| hud `heist_matches_the_originals_draws` | **hud1:** DL 341 (REDS "+1" ff0000 at 24, the others 404040, the bag icon); DL 340 (neither); DL 401 (1 bag, steady); DL 296 (the free bag's blip [258, 44, 2, 2] ffffff). **hud2:** DL 300 (GREENS and YELLOWS skulls); DL 1000 (BLUES "+1" 0000ff); DL 1060 (BLUES' blip colour, 4040ff). **hud3:** DL 200 (BLUES' skull). **STATS** (`stats_cs2_dl_790`, `stats_cs3b_dl_970`): YOU WIN, GREENS WIN, the shared 2nd, the columns, the row colours |
| heist `bag_clips_follow_the_oracle` | `bag/bag_timeline.txt` reads 328-640: arrive, stand cycle, grab, carry, throw, landing, slide, walk, let go, carried walk/run |
| audio `bag_events_fire_as_the_oracle` | `snd_bag_calls.txt`: clip 5's t-0 at the hand (343), never its t-8; clip 9's t-0 at the release (395), its t-8 at the landing (411); clips 7 and 8 at their starts and each loop |
| outro `heist_outros_follow_the_oracle` | `outro_cs2_*.csv`, `outro_cs3_*.csv`, `fade_cs2.csv`, `fade_cs3.csv`: the key, the clock, each actor's place, facing, scale and roll, the fade's alpha |
| Live, `shots.sh a` (audio debug, slot 0) | voices 1130 at the take (read 237), 1131 at the retake (406), 1129 at the delivery (455), centred at 1.0; the bag's 473 at the hand (245), 473 + 91 at the release (309), 190 + 473 at the landing (325); REDS' base hum 1116 (454) |
| Live, `shots.sh d, e` (cs3) | a bot's voices 3D at its distance; the outro's sounds 10 and 12 (reads 716, 782); results after cs3's 775 frames |
| `bun run test` | 435 pass, 1 ignored (with the Raptor pack lane's work in the tree) |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |

Screens (`../lab/screens/heist/`, 1280 × 720):
- **The HUD and radar:**
  - `hud-carrier.png`: the carrier's HUD, no bat;
  - `hud-enemy-a.png` (BLUES, the blink's hidden phase: no icon, REDS' pip grey, the carrier's red blip) and
    `hud-enemy-b.png` (GREENS, its shown phase: the icon and REDS' "+1", no red blip) while REDS carry;
  - `radar-free.png`: the free bag's white blip beside REDS' red;
  - `skulls.png`: three empty teams.
- **The bag:** `carried.png`, `thrown.png` (in flight over its shadow), `walking.png` (walking home), `delivery.png`
  (REDS' 1 bag).
- **cs2:** `cs2-early.png`, `cs2-mid.png`, `cs2-late.png`, `cs2-stats.png` (REDS WIN, four rows).
- **cs3:** `cs3-early.png`, `cs3-bubbles.png` (the sinking Wise Guy in REDS' shirt, the cement shoe), `cs3-late.png`,
  `cs3-stats.png` (REDS WIN, the two humans' rows).

**Gaps.**
- **Pack fixes (2026-10-13, "HUD and audio leftovers"):**
  - `hud/bubble_particle.png` (0xC01), not changed: the decode was right (its TLUT read as RGBA16 is a grey ramp,
    every alpha bit set; read as IA16 its alphas are noise; no row interleave). It is a soft-edged white square.
    cs3's bubbles now draw added to the view and still look like squares (the original's draw is not found).
  - 87 and 134 have their `characters/<id>/character.json`; `characters("1a")` lists them, and the bag's clips come
    from 87's.
- **I, the bag:**
  - The shadow's radius (24) and constant alpha 0xA0; the oracle's changes with height (0x73, 0x6B, 0x8B).
  - The clip clock is the client's: a state counts from the tick this client saw it, so a client joining mid-stand
    starts the stand cycle there.
  - One tick off at read 360: idle at (6, 0) against the oracle's (5, 8).
  - The carried bag's events are heard as the carrier's own.
- **I, the outro:**
  - The roll and pitch axes and signs.
  - The bubbles' rate (one a frame), spread and rise (1-2 a frame).
  - cs3's 740-unit black backdrop does not fill 16:9, so the clear colour shows at the sides (V `cs3-bubbles.png`).
- **C, the wire:** `voice` carries one line a tick (the last); two in one tick drop the first (none seen).
- **I, the blink:** the blink's VI is 2 × the client's own tick, not the match's, so each client's phase is its own
  (`hud-enemy-a/b`, taken 5 reads apart, are in opposite phases).
- **Not tested at the unit level:** the radar's carrier mask and the free bag's blip are chosen in `shown()`, which
  reads the ECS. The test checks their draws and colours; the screens show the mask.

## Raptor: the pack (2026-10-13)
**Batch M6, the pack half. War: Raptor (scenario 2) on map 0x3F is in the pack: the raptor (model 83, its own rig, 37
clips), the baby (165, on the raptor's rig and clips), the egg (171), the carried body (22), the bone crossbow (0x41)
and bone knives (0x3F) in the hand, as HUD icons and in flight, the cooked eggs and the bone weapons as props, the
mode's map data (`maps/3f/raptor.json`: hidden placements, pickups, team spawns, script zones, records, the raptor's
rig and bites), their sounds and both outros. The raptor's skull is joint 5 (I), its jaw joint 6 (C), and a carried
body sits at joint 6's origin (V). No rules, server or client change: M6's rules and M7's client play it.** Research:
`../lab/research/mp-original/raptor.md`, `raptor-scratch/`; this batch's `raptor-pack-scratch/` (oracle
`ConkerRecomp.m5`: runs k1 the knives, c1 the crossbow, s1 the scales; `results.txt`).

| Part | What |
|---|---|
| conker-extract `lib.rs` | `TEMPLE` (0x3F), `RAPTOR` (83), `BABY` (165), `EGG` (171), `BODY` (22); `load_character` (165 on 83's rig: obj+6 = 83, V r3). With 0x3F in `--map`: 83 as a player character (`character.json`, lift 0), 165, 171 and 22 as prop characters. `raptor_mode()` → `maps/3f/raptor.json`: `hidden` 0-14 (V r4), `cooked` the placements of prop ids 0xFF − k (C), `pickups` k0-k9 on placements 15-24, respawn 240 VIs (C script 0xBE, V r9-r11), `spawns` conker-rules' `RAPTOR_START_3F`, zones `delivery` 0, `feed` 1/0x13, `hazard` 3/0xD, `jumps` 4-7 (C), the records of uids 0x11, 0x13, 0x14-0x16 (`[0x0E, 0x3F]`), `raptor`: scale 0.7 (V s1), head 5 (I), jaw = the model's special joint 0 = 6 (C), carry 6 (V r2), the attack table (class word 1, 0x28-byte entries 0-7) and the 9 op-7 hit events of its routes (C `func_1506BEDC`). `props_level(0x3F)`: the cooked eggs, 4-6 and the bone weapons in place. `HELD_CLIPS` + 0x2C8 (bone crossbow: bank-01 #0x2F, a child object, V c1) and 0x2CA (bone knives: bank-09 #0x7B, V k1), both at joint 9. `PROJECTILES` + bank-09 #0x7C (the flying bone knife, 45 vertices, V k1; the bone bolt is the crossbow's #0x35, V c1) |
| conker-extract `outro.rs` | `OUTROS` + 0x3F: cs2 if the cavemen rank first, else cs4 (C, V b2, b1). `HEADS` + 136 (jaw 15 C, head 12 I). Characters through `load_character`. `bubbles(0x3F)` empty: Raptor's outros have no subtitles (below) |
| conker-extract `hud.rs`, `audio.rs` | Icons 0x3F = 0x14's cell, 0x41 = 0x18's (C `func_150918EC`). `MATCH_CLIPS` + the egg carrier's 0xD5-0xD8, 0x2E4 (V r4), the bite victim's 0x170 (V r2), the bone weapons' 0x2C8, 0x2C9, 0x2CA, 0x2CC (V r9, r10). `VOICED` + op 141 (the raptor's and baby's calls and steps, B, 300/3000) and 145 (the body's, A, 190/1300) (C). Every clip event of 83, 165 (83's clips) and 22 is kept. `REACTIONS` + 0x3F, 0x41 → 0x20 (I) |
| conker-assets | `raptor.rs` (new): `Raptor`, `Record`, `Rig`, `Attack`, `Hit`, `raptor_json`; `load` refuses a hit naming an attack past the table. Two tests (round trip, the refusal) |
| Tests | `tests/pack.rs`: a Raptor block (`raptor.json` read back and its values, 83's rig and clips, 165/171/22's joints and clips, no `character.json` but 83's, `props.gltf` nodes, the bone weapons' held entries); `pack_audio_reads_back` (83/165/22 events on their clips, the raptor's call, `voice-1c5.wav`, the manifest); `pack_hud_reads_back` (the bone weapons' icons). `tests/rom.rs`: `raptor_models_and_outros` (83's joints 4-7, 165 = 83's clips, the egg, both outros' cast, voice, heads, masks, lengths, no subtitles), `projectiles_and_undrawn_quads` (6 scenes), held 20 |
| Pack | Re-exported (`bun run pack --force`). New: `maps/3f/raptor.json`, `props.gltf`/`.bin`, `outros.json`, `characters/83/`, `165/`, `171/`, `22/`, `audio/characters/83/` (6 WAVs), `22/` (27), `audio/maps/3f/voice-1c5.wav`. Changed: `held.*` (20 models), `projectiles.*` (scene 5), `audio/events.json`, `sounds.json`, `pack.json`, `README.md`; 1042-1044, 1055, 1056, 1060 moved from `characters/136` to `common` (the body plays 1042-1044 and 1060, the raptor 1055 and 1056; the 6 stale WAVs removed from `assets/`) |

| Check (2026-10-13) | Result |
|---|---|
| Oracle k1, c1, s1 (`raptor-pack-scratch/results.txt`) | Hand knife #0x7B (45 vertices) and crossbow #0x2F (160) on joint 9; flying knife #0x7C, bolt #0x35; scales raptor 0.70, baby 0.45, body 0.51, egg 0.20, caveman 0.5 |
| conker-assets, conker-extract tests | pass (`tests/pack.rs` 5, `tests/rom.rs` 14, `tests/traps.rs`) |
| `bun run test` | 435 pass, 1 ignored, with the Heist client lane's work in the tree |
| `bun run check` | fmt, clippy and the web check pass on every crate but conker-client, where the Heist client lane's work in progress has rustfmt diffs and one clippy `type_complexity` (`outro.rs` 466) |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1678 files |

**Gaps.**
- **No oracle CSVs for the outros:** they are exported from the tables (C) without an oracle check. The 0xFF lengths
  are 1,530 and 830 VIs; the oracle's outros lasted 1,500 (b2) and 749 VIs (b1), ending at the table's last command.
  `length` is left as the table's (I which rule the game uses).
- **No subtitles (V missing):** no DLs of Raptor's bubbles. Op 7's b picks the text, entry 1 + 3 × objects + scripts
  + b (C). cs2 has two bubbles: b 0 ("i'm stuffed. / that was very tasty.", open 120, turn 225, close 310) and b 1
  ("ha, ha ! / look, Fred's crapped himself.", 1150/1200/1255); `Subtitles` holds one open-close run, so cs2 needs a
  second run. cs4 has one ("i think he enjoyed that !", 211/302) and fits today's model. Both need the bubbles' places
  (the `heist-presentation-scratch/outro/bub.py` flow).
- **I, outros:** cs2's op 5 b 0x24 (kind-0 object 5 at 928), op 0x11 c 0 at 1500 (a head-turn off) and cs4's baby's op 6
  0x3C7 with e 1000 are ignored; the caveman's head joint 12 is the villagers'.
- **I:** the raptor's head joint (5, by its skull's 316 vertices); the bone weapons' hit reactions (0x20, as the other
  blades'); the baby's sounds are the raptor's events as is (no pitch of its own checked).
- **Not in the pack's model:** the cooked eggs are not in the collision (solid while shown not checked); the body's
  hang is joint 6's origin turned with the raptor (V r2 fit, RMS 1.8 at 0.7), with no offset; the raptor's 0.7 is
  the oracle's obj+0x14C (not found in the ROM). The baby stands at its record's y − 10 (V r3), not snapped.

**Handoff to M6 (rules, server, bots).**
- `maps/3f/raptor.json` (`conker_assets::raptor::Raptor::load`): pickups (only cavemen take them; respawn 240 VIs),
  `hidden` from the start, `cooked` shown in turn at a delivery, the zones, the records, the bites (`hits` run
  `attacks[attack]`'s area test every frame for `window` frames from `t`; reaction 0x46 kills, 0x1C knocks back).
  The pack's `ai.bin` on 3f already is script 0xBE (Deathmatch's and the mode's), so no new AI script.
- Raptor bots never feed (raptor.md); the idle clip plays at 0.5 tick a frame.

**Handoff to M7 (client).**
- `characters("3f")` gains 83 (a player, `character.json`) and the prop characters 165, 171, 22; draw 83 at 0.7 with
  no lift, the baby 0.45, the body 0.51 at joint 6's origin, the eggs 0.2.
- `held.json` has 20 entries: the bone crossbow (`01-2f`) and knives (`09-7b`) are scenes 8 and 9, so later indices
  moved by 2. The flying knife is `projectiles.gltf#Scene5`, the bone bolt `#Scene3`.
- `events.json` gains 83, 165 and 22 and the cavemen's new clips (egg carry, bite victim, bone weapons).
- Outros on 3f: `winner` 0 when the cavemen rank first; no `subtitles`.

## Heist: rules, server and bots (2026-10-13)
**Batch M3. `--mode heist` on `--map 1a` plays the original's scenario 0. Four teams (REDS, BLUES, GREENS, YELLOWS)
of Wise Guys, each with the bat and 6 health, fetch the cash bag (character 87) from the vault's centre to their own
base. Script 0xB7's team branch, run by conker-bots, does the bag's work:
- it creates and destroys the bag;
- it notices a pickup;
- it makes a carrier that is hit, dies or throws let go;
- it sends the bag walking home on the nav graph;
- it scores a carrier inside its own base.

The carrier runs at 25.2 (0.72 × 35) with the bat put away, and B throws the bag. Winning:
- the first team to the bag target (1-5, default 3) wins;
- a time-out ranks the teams by bags, then kills, then fewer shots;
- untimed, the last team with lives wins.

The outro is cs2 if a human's team won, else cs3; no mode can index past the outro list any more. Protocol 25.**
Research: `../lab/research/mp-original/heist.md` (M3), `heist-presentation.md` §2,
`heist-presentation-scratch/bag/bag_timeline.txt`. Oracle runs bots1 and hb3 are in `/tmp/m3`; a frame is "+n" after
the start (oracle read 23).

| Part | What |
|---|---|
| conker-rules `heist.rs` (new) | **Constants:** `RECORDS` (team t starts on t and respawns on t's three records), `HOME`, `SCORE` 3, the carrier's clips `GRAB` 0x114 (26 frames) … `THROW` 0x118 (20 frames, release 15 in), `THROWN` 58, `DROPPED` 3. **`Cash`:** shown, carrier, pos, yaw, `State` (stand, carried, flight, fall, slide, walk), since, touchable_at, thrown, last. **The throw (V h2):** the bag leaves 49.5 ahead and 42.7 up and flies 9 a frame, rising 18.4 less 3 a frame. It lands at +16, 193.5 ahead, then slides 5, 5, then × 0.9 down to 0.5, resting about 244 ahead. **Other:** `body` (radius 29, pushes players), `touches` (as the flag's), `carry_clip` |
| conker-rules `game.rs`, `gas.rs`, `colors.rs` | **Settings and state:** `Settings::heist`, `Match::heist` (bag plus four scores), `Fighter::bags`. **Effects:** `Cash(shown)` (the script's create/destroy), `CashAt(pos, yaw, walking)` (the walker's step, applied only while the bag `walks`), `Voice(slot, id)` → `Event::Voice`; `LetGo`/`Settle(heist::CASH)`; `Score` adds a bag and credits the last carrier. **The carry:** a touch in `pickups` (alive, free hands, no stun) makes `flag = Some(CASH)`, the grab pose, and the bat undrawn. B throws (`Pose::Throw`). `body.heavy` sets the speed. A takeover hands the bag to the human. **Ending:** `ends()` checks four teams; `teams_places`/`teams_result`/`teams()` generalise the team ranking (out teams below, the later out better); a target reached latches on the delivery frame. **Flag-1 pickups** (Heist with ≤ 4 players, `Match::unique`): a roaming pickup stays gone while its holder keeps the weapon. The holder's death drops it at the death spot for 150 frames; then it moves to a random spot |
| conker-sim | `PlayerState::heavy`: the drive × 0.8 × 0.9 (cap 25.2). It is predicted, as `holding` is. `net.rs`: the wire (below) |
| conker-bots | **`brain.rs`, the bag's queries** (handle 0x2010, checked before Beach's guns): 0x98, 0x99, 0xA3, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCD (`off_heading` shared with the flags). **The bag's commands:** 1 settle, 2, 0x14, 0x5B-0x5D, 0x7D. **Players:** 0xA5 6 (the team's score S), 0xC9, 0xD0 (xz distance to a node), cmd 7 (let go). **Objects:** obj(0x2011) create and (4,2) destroy; (3,7) voice lines → `Effect::Voice`. **`lib.rs`:** the walker (class 0x57's scripts 4/5: speed 5, +2.5 a frame to 17.5, turning ≤ 3840, floor from the world). `players[0x10]` is the bag, for cmd 0x19 chases. `mirror`: PICKUP_FLAGS 1 and the holder when unique |
| conker-server | `--mode heist`: map 1a only, no `--cavemen`, `--score` 1-5 (default 3), ≤ 7 bots. Team = slot & 3. Under 3 humans: 8 players, ≤ 2 humans a team; from 3: 4 players, one a team. Placement on `RECORDS` with the team's mask. `winner`/`human_won` via `teams_result`. **Outros:** `outros` is (winner, cutscene, frames), found by cutscene in Heist (2 or 3), by winner elsewhere, 0 frames if none. `--seed N` (xors the game RNG's seed). HEIST log lines. `bots.sh`: `TIME`, `SEED`, and a Heist report (bags made, takes, let-gos, voices, deliveries) |
| conker-client | `match_settings` copies `heist`; nothing drawn for Heist yet (M4) |
| web `data.ts`, `server.ts`, `Servers.tsx` | **`data.ts`:** "heist" (War: Heist) in 0x1A's modes. `ModeInfo` gains `maxScore`, `defaultScore`, `maxBots`. `fit()` clamps bots and the score when the mode or map changes. **`server.ts`:** `validate` rejects scores and bots over a mode's limits. **`Servers.tsx`:** its option ranges follow the limits |

| Check (2026-10-13) | Result |
|---|---|
| heist `a_throw_flies_lands_and_slides` (V h2) | lands at +16, 193.5 ahead, rests at 243.5 ± 1, touchable at release + 58 |
| movement `the_bag_drives_at_25_2` (V h1, bots1) | top 25.2; 33 frames of 0.75 reach 24.75 |
| game `heist_grabs_carries_throws_and_scores` | 26-frame grab, then 0, 0.75, 1.5; top in (24.75, 25.2]; the throw leaves at +15 (touchable 58 later); a drop is touchable at +3; with `--score 2` the 2nd delivery latches on its frame |
| game `heist_ranks_four_teams` | bots1's S [0, 2, 1, 1] with kills → ranks [4, 1, 2, 3], winner BLUES; unt1's out order → [2, 3, 1, 4]; a 1st-place tie → [1, 1, 3, 3], no winner |
| game `heist_eliminates_and_hands_the_bag_over`, `heist_pickups_are_unique` | the last team in play wins untimed; a takeover moves the bag; a held weapon stays gone, a dead holder's drops at its spot for 150 frames, then roams |
| server `heist_teams_records_and_outros`, `the_results_count_down` | teams [0, 1, 2, 3, 0, 1, 2], records 0, 1, 2, 3, 8, 11, 14; a GREENS win with a human → cs2 (325 frames); a YELLOWS win without one → cs3 (775); a 2nd human joins, a 3rd is refused with 7 bots |
| ROM `heist_bots_fetch_and_deliver` (7 bots, 9,000 frames) | bag at +247 (bots1 +234, in 210-299); 23 pickups; carry top 25.2 outside hurt frames; 1,454 frames walking home; GREENS delivers; no unknown natives, 0 faults |
| `bun run test` (428), `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass, with the other lanes' work in the tree |

Runs (`bots.sh`, `MODE=heist MAP=1a TIME=5 HUMANS=1`, 350 s so the 5-minute time-out lands; ports
5970-5976; no faults, unknown natives or panics). The oracle columns are the first 300 s of bots1 (10 min) and hb3
(300 s).

| Run | Bag made | Pickups | Deliveries | End |
|---|---|---|---|---|
| 7 bots, seed 1 | +264 | 16 | REDS +1511, GREENS +7284 | time-out, bags [1, 0, 1, 0]: GREENS 1st on kills (11 vs 4), the human's YELLOWS 4th (cs3) |
| 7 bots, seed 2 | +211 | 30 | BLUES +2257, REDS +8707 | time-out, bags [1, 1, 0, 0]: BLUES 1st on kills (13 vs 4) |
| oracle bots1 | +234 | 25 | BLUES +5339, GREENS +6484 | (at 10 min: S [0, 2, 1, 1], ranks [4, 1, 2, 3]) |
| 3 bots, seed 1 | +210 | 14 | none | time-out, bags all 0: GREENS 1st on kills (12) |
| 3 bots, seed 2 | +287 | 16 | BLUES +1864, +4269, +6859 | BLUES reaches 3: latched on the delivery frame |
| oracle hb3 | +270 | 30 | none | S all 0, kills [0, 4, 2, 4] |

With 7 bots, the deliveries (2 in 300 s) and pickups (16-30) match bots1's. With 3 bots, deliveries are within the
oracle's variance, but pickups are about half of hb3's 30.
Delivering carries last as the oracle's: ~780-800 frames from the take (smoke 1130 → 1930; 3 bots seed 2: 787, 780,
774) against bots1's 780 and 849.

**Gaps.**
- **I, the bag:** there is no wall test in a throw's flight or slide, so a throw at a wall goes through it. The
  walker's arrive radius (60) and its gain's shape are guesses. The held offset (10 ahead, 40 up) stands in for the
  joint. The bag's body is a squirrel's with radius 29.
- **I, the carrier:** its hit clip (0x11B) isn't played; the hit's drop comes from the script's cmd 7.
- **I, teams and voices:** teams come from the slot (slot & 3, as the oracle runs had them); there is no team choice.
  The voice lines' random pick uses the script's own draw, not checked against the oracle's sequence.
- **V:** with 3 bots, pickups are about half the oracle's (fewer hits on the carrier). The takeover keeps Total War's
  gate (an out human, 13 frames on), so timed Heist (99 lives) never takes over, as in bots1; untimed it does.
- **C, not on the wire:** the voice lines (`Event::Voice` is server-side only, logged as HEIST lines).
- **Server, I:** `human_won`, `winner` and the outro's length (`results_at`) are worked out from the live places at
  each call, so a kill in the end phase can still flip a kills tie-break, and the cutscene with it. When the original
  makes its pick is not traced. `--seed` is new
  (for the runs).

**For M4 (the client), protocol 25.**
- **`MatchView`:**
  - `heist`;
  - `heist_scores: [u16; 4]` (each team's bags, also in each player's `Combat::score`);
  - `score_limit` (the bag target);
  - `winner` 0-3;
  - `human_won` (cs2 if set, else cs3);
  - `cash: CashView`.
- **`CashView`:**
  - `shown`;
  - `carrier: Option<u16>` (a slot);
  - `pos`, `yaw`;
  - `state`: 0 stand (clip 0), 1 carried (6, 5), 2 flight (9), 3 fall (7, 8), 4 slide (10), 5 walk (3);
  - `since` (the state's first tick);
  - `touchable`.
- **`Combat::bags`:** the player's deliveries, for the STATS bags column. `Combat::flag` stays Colors' only.
- **The carrier's `anim`:** 0x114 grab, 0x115 idle, 0x116 walk, 0x117 run, 0x118 throw. Its bat is undrawn (`drawn`
  false). `PlayerState::heavy` is predicted.
- **Not on the wire yet:** the voice lines' ids (0x465-0x46B, 0x547) are `Event::Voice{player, id}` on the
  server only.

## Map 0x24 and the tank: the pack (2026-10-13)
**Batch M8, the pack half. The pack has map 0x24 (Tank's map, scenario 1) like the other maps, the tank (model 119)
with its flags, brake lights and tread lists, Tank's props, the shell's model, Tank's sounds, music and bomb countdown.
The turret is joint 2 and the gun joint 3 (C, V). No rules, server or client change: M9/M10 play it.** Research:
`../lab/research/mp-original/tank.md` (§7, §9 M8b), `tank-scratch/`; this batch's `tank-pack-scratch/` (oracle
`ConkerRecomp.m8`: runs j1 the joints, s1 the sounds, sh1 the shell (`sh1_draws_42.txt`); b1, p1, c1 are tank.md's;
decompiled callbacks in `code/`) and `../lab/audio/map24/` (music).

| Part | What |
|---|---|
| conker-extract `lib.rs` | `TANK_MAP` (0x24), `TANK` (119), `TANK_AI` (0xB8). With 0x24 in `--map`: 119 as a prop character (own rig, 14 joints, 8 clips, lift 0), after the Wise Guy. `tank()` binds segment 6 → texture entry 15 (brake lights off), 7 → entry 9 (headlights on), 0x0A → entry 16 (red flag) in the glTF, and writes `textures/team-0..3.png` (entries 16-19) and `textures/brake-lit.png` (entry 12) (C `func_1502F01C`: a light code c ≥ 10 binds entry c − 10; V b1). `character.json` `teams` (as 59's) and `tank`. `hidden_placements(0x24)` = `TANK_PROPS` (9, 12-15, 17-33: `[3, 72]`, the bomb `[3, 80]`, the spare canisters, the wrecks `[3, 95]`) → `maps/24/props.gltf` (one scene, in place). `Roaming` (0x1A's and 0x24's): Tank's 4 power-ups (shield `[3, 88]` 0x80, canisters `[3, 71]` 0x01, container `[3, 81]` 0x10, launcher `[3, 70]` 0x02; placements 16, 8, 10, 11) on 6 spots (nodes 0x7B, 0x17, 0x40, 0x7C-0x7E + 60), respawn 300 VIs. `map_lights(0x24)`: the 16 type-9 triggers, steady, ambient (19, 68, 219). `ai_script(0x24)` = 0xB8. `PROJECTILES` + bank-09 #0x33: `projectiles.gltf` scene 4, the shell (drawn at 1.5, V sh1 DL 42) |
| extract-scene | `deathmatch_solid(0x24)` = the bunker doors 0-7 (V b1: `+0x4F` 0x01, `+0x6E` 0): 48 of the 1694 collision faces |
| conker-extract `audio.rs` | `clip_sounds` keeps every clip event of 119 (fire: clip 1's 699). `map_sounds("24")`: song 32, channels 0-15, volume 18000. `tank_cues()` (character 119; C `func_151BB0E0`, `func_15055A2C`, `func_1511DD98`, V s1): `tank_engine {moving}` records 1191 at 0x2000 idle / 1192 at 0x2EE0 moving (200/800); `tank_gear {coast}` 1187-1190 / 1189 into coasting (28000, 300/1800, channel a, if idle); `tank_blast` the rocket's set (689, 693-695, 701, 702; 32700, 1000/3000, pitch + 0-499), `{ground: true}` + 750 (30000, 500/1000); `tank_death {bomb}` 742 22 frames after the killing blast / 735 (12000, 500/1000); `bomb_pickup` 11/12 and the power-ups' `pickup` 1296, 1187, 385, 752 (26000, 100/1000). The gas is `gas` (1231) as Total War's. `--song 24=` → `audio/maps/24/countdown.wav`, `sounds.json` `maps["24"].lines["countdown"]` |
| conker-assets | `Character.tank` (`Tank`: `turret`, `gun`, `muzzle`, `brake`, `brake_lit`, `treads`, `tread_joints`); `On::TankEngine`, `TankGear`, `TankBlast`, `TankDeath`, `BombPickup`. The sample covers each; round trips bit-exact |
| Tests | `tests/pack.rs`: 0x24 in the read-back (ROM path, solid rule, 20 spawns), a Tank block (AI 0xB8, no lift or kill gate, bunker triggers 0x11-0x14's boxes, 48 door faces, power-ups, respawn and roams, lights, 119's clips, rig, `tank` parts, materials, team PNGs, props.gltf); `tank_audio_reads_back` (cues, countdown line, note, files). `tests/rom.rs`: `tank_model_and_map` (119's parents, joints 2-4 and 12/13, tread vertices 190-217 on 13 and 218-245 on 12, clips; 34 placements, the doors, the bomb at (−20, −532, −108), floor −617 under it, spawns, spots, props), `projectiles_and_undrawn_quads` (5 scenes) |
| `../lab/audio/map24/` | `background_loop.wav`: song 32 (engines muted by `tank-pack-scratch/mute.py`), recorded 617.8 s, period 255.803 s, cut at 88.80 s with a crossfade. `countdown.wav`: song 50 (on player 1 at the plant), 26.4 s, trimmed |
| Pack | Re-exported (`--force`; `bun run pack` + `--map 24 --background 24=../lab/audio/map24/background_loop.wav --song 24=../lab/audio/map24/countdown.wav`). New: `maps/24/` (level 69 textures, collision 1694 faces, 20 spawns, 4 pickups, 34 triggers, nav, ai.bin, 16 lights, props.gltf), `characters/119/` (19 textures), `audio/characters/119/` (9 WAVs), `audio/maps/24/` (background, countdown). Changed: `weapons/deathmatch/projectiles.*` (scene 4), `audio/events.json`, `sounds.json`, `pack.json`, `README.md`; 0012, 1191, 1192 moved from `maps/1a`, `2b`, `30` to `common` (now played by two scopes) |

**The turret and the gun.** `func_15033FE0` → `func_150344A0` (models 0x28 and 0x77, C) overrides two joint angles:
joint 2 about y by −obj+0x2E4·65536/18000 (the turret's yaw) and joint 3 about x by obj+0x2E6·65536/18000 (the gun's
pitch). V j1 (turret poked to 90° yaw, 36° pitch): DL 60 moves only joints 2 and 3, by −90.00° and −36.21°. Joint 4
sits 110.4 along joint 3's z: the muzzle (I). Joints 12 and 13 hold the treads (lists 0 and 1 of `func_150ED638`).

| Check (2026-10-13) | Result |
|---|---|
| Oracle j1, b1 | The joints as above. Brake lights obj+0x6C 25 off / 22 on, headlights +0x6D 19; flags 16-19 by slot, slots from 3 up yellow |
| Oracle s1 (`../lab/audio/captures/tank_s1_calls.txt`) | Every cue's id, volume and distances above, from the sound calls (lldb on a re-signed copy of m8) |
| Oracle sh1 | A new 16-vertex batch in DL 42 at muzzle height 410 ahead = bank-09 #0x33's vertices at scale 1.5, nose at +48 |
| Oracle lights (`lights24.py`, `lit24.py`) | 16 lights = the 16 type-9 triggers, ambient (19, 68, 219) |
| conker-assets, conker-extract tests | pass (`tests/pack.rs`, `tests/rom.rs`, `tests/traps.rs`) |
| `bun run test` | 428 pass, 1 ignored |
| `bun run check` | pass |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1546 files |

**Gaps.**
- **I:** the power-ups' start spots [1, 3, 2, 4] (the game picks random free ones; one V run with 2 humans).
- **I:** which tread list is the left side; the muzzle joint; the death sound's 22-frame delay (seen once); song 50's
  volume on player 1 (the line is at 32767).
- **C, not packed:** the engine's 0x4A9 (1193) on its 900/1412-VI timer (not heard in s1); the engine's pitch
  (|speed|·obj+0xB8·0.5, I).
- **Not in the pack's model:** the tanks' own lights (white, size 12 ahead; red, size 15 behind while braking) are not
  in `lights.json`. The wrecks are not in the collision (solid while shown: not checked). The shell's two 4-vertex
  quads along its path (I: smoke) are not packed. The bomb's look on the carrier (item 0x18) is not found.
- Power-ups are `Gives::Weapon(bit)`: conker-server's `arena()` matches `Gives` exhaustively, so a new variant is M9's.
- The background's run log was lost (the WAV is whole); the loop is not sample-exact (period from correlation).

**Handoff to M9 (rules, server, bots).**
- `conker_bots::script(0x24)` → 0xB8 (the pack's `ai.bin` already is). `weapon::pickups(0x24)`: move
  `TANK_PICKUPS`/`TANK_SPOTS` into conker-rules with a `Gives::Power(bit)`; roaming as 0x1A's, respawn 300 VIs.
- Doors 0-7 open (+0x80, 8 per frame) around the plant; wrecks: placement 18 + slot at the death spot for 330 frames;
  the bomb is placement 12. The server refuses `--map 24` until then.

**Handoff to M10 (client).**
- `character.json` `tank`: yaw joint `turret` about y, pitch joint `gun` about x; scroll the `treads` material's
  vertices on `tread_joints[0]`/`[1]` by −7·speed per frame in S (wrap 0x800); swap `brake` to `brake_lit` while
  braking; `team-<slot>.png` on the flag; the tank's two lights; the shell is `projectiles.gltf#Scene4` at 1.5.
- Cues: `tank_engine` by moving; `tank_gear` at a state change (speeding up: |v| > 1 with the accel; coasting: |v| > 5
  without); `tank_blast`, `tank_death`, `bomb_pickup`; the `countdown` line at the plant; `gas` over Tank's gas span.
- Outro export (not changed): Colors cs5's op 9 on its tanks is the headlights' entry 9, already bound in the glTF.

## Heist's presentation in the pack (2026-10-13)
**Batch M4, the pack half. Heist's two outros, its voices and the bag's sounds, its HUD strip and STATS pictures, and
a stop-on-leave flag for sound zones are in the pack. No protocol change, no client change.** Research:
`../lab/research/mp-original/heist-presentation.md` and its scratch (`outro/` CSVs and `outro.diff`, `bag/snd_bag_calls.txt`,
`hud/`), from the headless oracle (`ConkerRecomp.m1`/`.m4`).

| Part | What |
|---|---|
| conker-extract `outro.rs` | `outro.diff` applied: `OUTROS` (0x1A, [(0, 2), (1, 3)]): winner 0 is "a team with a human won", 1 "none did". Actor scale from the scene record's +0x20; `HEADS` (134, none, jaw 6); a scriptless trailing kind-3 object; a t-0 op 2 with c 2 holds frame 0. Decoded beyond the diff: `Actor.team` (op 5 b 0x6F: obj+0x68 = d, team d − 1; V cs3's Wise Guy in REDS' shirt, obj+0x68 = 1); `Actor.roll`/`pitch` (channels 6/8, only when they change: V 0 → −16 → 13 → 0 on the sinking Wise Guy); `Prop.scale` (channel 5 / 100, channel 4 / 100, the placement's +0x28: (37, 37.01, 2.195)); `Outro.light` (op 5 b 0x65 d 6 on a kind-0 object: a white light of size 8 at (−644, 180, 4728) from t 0); `Outro.particles` (op 5 b 0x50/0x51 c 0 → c 1: the bubbles, 5 bursts on actor 1 from t 530 to 1321); the attachment (op 0x0D b 0x68: bank-09 0x51 on joint 19 from t 400) is the existing `attach` |
| conker-extract `lib.rs` (via `outros`) | `characters/134/` (Don Weaso, 5 clips), `maps/1a/outros.json`, `outro.gltf` (scenes `09-51`, `prop-13`; texture 2683), `audio/maps/1a/voice-b5.wav`, `voice-ce.wav`, `0010.wav`, `0012.wav` |
| conker-extract `audio.rs` | `MATCH_CLIPS` + 0x114-0x118 (the carrier's grab, idle, walk, run, throw: footsteps 0x2000 at 10/30 on 0x116, 0x2001 at 7/17 on 0x117; every character shares them). With map 1a: `events.characters["87"]`, the bag's own clip events, every clip (`clip_sounds` keeps all of `CASH`'s): 473 at clips 5 and 9 (t 0 and 8), at 7 and 8 with chance 64; 190 at 5 and 9 t 8. `heist_cues`: `cash_pickup` [1130, 1131], `cash_steal { rare: true }` [1351], `cash_delivery` [1128, 1129] (32700, own 32700, 500/1000, channel b), `cash_steal { rare: false }` 1125-1127 (a record, 32700, 500/1000), `cash_throw` [91] (28000, 500/2500, channel b, at the bag). `zones()` sets `stops_on_leave` on every kind-2 zone |
| conker-assets | `outro.rs`: `Actor.team/roll/pitch`, `Prop.scale`, `Light`, `Particles`, `Outro.light/particles` (all serde-default, absent before). `audio.rs`: `Zone.stops_on_leave` (default false), `On::CashPickup`, `CashSteal { rare }`, `CashDelivery`, `CashThrow`. `hud.rs`: `Hud.heist: Heist` and `End.heist: HeistResults` (both required: an older pack does not load) |
| conker-extract `hud.rs` | `hud.json` `heist`: anchor centre/bottom, y 200, centre 36, step 73, `pip` `pips` [14, 16, 7, 7] (blob 0xD14, again under its own name), pip_step 8, empty 404040, `skull` atlas2 [48, 20, 16, 16], `colours` ff0000/0000ff/00ff00/ffff00, `bag` [142, 3, 12, 11] from atlas [50, 26, 12, 11], `radar` ff3737/4040ff/78ff16/ffb11e, `cash` ffffff. `hud/bubble_particle.png` (0xC01). `end.json` `heist`: `won[team]` = the name sprite (REDS 0x863, BLUES 0x851: 2 tiles at 81; GREENS 0x858, YELLOWS 0x86F: 3 tiles at 65) + WIN at 151, y 34; `icons` (the bags' icon [103, 49], then accuracy, kills, headshots, deaths), `label` [34, 64], `columns` [86, 110, 144, 170, 200, 230]: place, bags, accuracy, kills, headshots, deaths |
| `pack-readme.md` | the outros' new fields, Heist's files, `hud.json`/`end.json` `heist`, the `cash_*` cues, character 87's events, `stops_on_leave` |

| Check (2026-10-13) | Result |
|---|---|
| conker-extract `heist_outros_follow_the_oracle` (rom.rs) | the decode: winners, cutscenes, lengths 650/1550, characters [134, 59], the camera cuts, voices, Don's scale 0.74, the light, the Wise Guy's team, attachment and roll, the sounds (0x0A at 594, 0x0C at 726), the five bubble bursts, the backdrop's placement, rotation and scale. Against the oracle's `outro/` CSVs (skipped when absent): every frame's camera (place within 0.05, yaw within 1e-3), every actor row (place 0.05, facing 1, scale, obj+0x68 = team + 1, roll 1e-3, clip and time; anim-0 rows held): cs2 325 rows, cs3 1286 + 264 held, and the fade on every frame. Exact |
| conker-extract `pack_audio_reads_back` | `characters/87` in the manifest; the 1a outros' voices listed; 0x1A's and 0x2B's six zones stop on leave; 87's 8 events; 59's four carrier footsteps; the five Heist cues' ids, volumes, distances and channels |
| conker-extract `pack_hud_reads_back` | `heist`'s pip, skull and bag rects, `pips.png` 32 × 32, `bubble_particle.png` 64 × 64, the names 64/64/96/96 × 32, GREENS WIN's tiles at 65, 97, 129, 151, 183 (V cs3b DL 970) |
| conker-assets `write_then_load_gives_the_same_audio` | `stops_on_leave` round-trips |
| `bun run test`, `bun run check` | pass (428 tests, with M3's and the Tank agent's work in progress in the tree) |
| Pack | `bun run pack --force`, then `../lab/tools/verify-pack.sh`: "pack equal" (1,432 files; 1,546 after the Tank agent's re-export with map 24, still equal). Against the pack before M4: new `characters/134/`, `maps/1a/outro.{gltf,bin}` and texture 2683, `audio/characters/87/0473.wav`, `audio/common/0091.wav`, `1125-1131.wav`, `1351.wav`, `audio/maps/1a/voice-b5.wav`, `voice-ce.wav`, `0010.wav`, `0012.wav`, `hud/pips.png`, `bubble_particle.png`, `reds/blues/greens/yellows.png`; changed `maps/1a/outros.json`, `events.json` (+ 5 cues, 87, the carrier clips' footsteps on every character), `sounds.json`, `hud.json`, `end.json`, `pack.json`. 2d/30/2b's `outros.json` are unchanged (no team, roll, pitch, scale, light or particles there) |

**Gaps.**
- **C, a departure from the research's recipe:** `stops_on_leave` is set on 0x2B's two escape-tunnel zones too, not
  only 0x1A's. `func_150A09D0`'s pass stops any kind-2 record that no player entered since its last pass. That is the
  same code for every map, but it was seen (V) only on 0x1A. Beach's earlier "never stops" was C, with nothing found
  that clears it, and its V is consistent with either.
- **I:** the prop's z scale (the placement's +0x28) and the roll/pitch axes (not traced). Texture 0xC01's format: CI4
  with the 16-entry ramp after its texels (2,080 bytes). It decodes to a soft white square, and no DL of it was seen.
  The particles' motion is not decoded either (sizes 31-118 and lives 50-156 frames as seen). The name sprites' tile
  layout is a guess for all but GREENS: REDS and BLUES 2 tiles at 81, YELLOWS 3 at 65.
- **C, not heard:** the steal's 1125-1127 path. 87's events at clip 5 t 8 (190 + 473): at the grab, V hears only
  473, because the clip is held at 8.
- **Not packed:** the tommy gun's release sounds (727, 747-749: not logged). There is no per-team STATS row colour
  table: rows use `results.colours[team]`, the pure team colours.

**Handoff (client).**
- `outro.rs`:
  - The Heist pick: key 0 when the winning team has a human, else 1. `team_mode` includes Heist.
  - `dress`: apply `Actor.team` (team-N.png on 59's material, as `render.rs`' `dress_teams`), `Actor.roll`/`pitch`,
    `Prop.scale`, `Outro.light` (a point light), and `Outro.particles` (billboards of `hud/bubble_particle.png` from
    the actor, `t`..`until`).
  - Hide the bag during an outro.
- `hud.rs`: `hud.heist` (the strip, the bag icon, the radar colours) and `end.heist` (the header: `won[team]` with 2+
  humans, else `teams.won`/`lost`; `icons`, `label`, `columns`; rows `teams.down` lower in `results.colours[team]`;
  no strip).
- `audio.rs`:
  - The `cash_*` cues: pickup, steal and delivery at the player; `rare` in one steal of three; the throw at the bag.
  - `events.characters["87"]` on the bag's clips; skip clip 5's t-8 events at the grab.
  - `Zone.stops_on_leave`: stop the frame after the last player leaves, restart on entry.
- Files: `characters("1a")` adds "87" and "134". `audio/characters/87/`. `maps/1a/outros.json`, `outro.gltf`.
  `audio/maps/1a/voice-b5.wav`, `voice-ce.wav`, `0010.wav`, `0012.wav`. `hud/pips.png`, `bubble_particle.png`,
  `reds.png`, `blues.png`, `greens.png`, `yellows.png`.

## Visuals: projectiles, flames, the twist, the twin guns' textures (2026-10-13)
**Batch E8 (Wave 7 item 1). Rockets, grenades, weapon 0x14's knives and the crossbow's bolts fly as the original's
models, turned as its display lists turn them. Bullets draw nothing, as in the original: the placeholder spheres are
gone. The flamethrower's stream is a ribbon through its flame points, drawn with the jets' flame texture. The twin
guns' twist turns the upper body toward the aim. The twin guns' three untextured materials are textured. 0x2D's white
quads (and 0x34's two) are out of the levels, because the original never draws them. Every map's rockets shake the
views. No protocol change.** Research: oracle runs on 0x2D (`ConkerRecomp.e8`, slot 0 given the weapon by a poke;
scratch `../lab/research/mp-original/visuals-scratch`: `rsp.py` now keeps the RDP state, `gren2.py` the grenade's
angles, plus the input scripts; DL dumps in `/tmp/e8`: twin, rocket, gren4, knife, bolt2, flame, quad3). The game
rebuilds its display list every other frame while its objects move every frame, so a change between two lists covers
two frames (the rocket moves 400 a list, 200 a frame). Every rate below is per frame.

| Part | What |
|---|---|
| conker-extract, conker-assets | `PROJECTILES` (the grenade bank-01 #0x7E, the rocket bank-09 #0xE1, the knife #0x23, the bolt #0x35) → `weapons/deathmatch/projectiles.gltf` (`projectiles_gltf`): scene i each, +z along the flight (V the oracle's batches: the grenade 12 + 8 + 8 vertices, the rocket 16 with its nose at +484, the bolt 23). `weapons/deathmatch/flame.png` (`flame_png`): the jets' flame (`traps::TEXTURES[0]`). `UNDRAWN` = bank-03 #0x33 (class 51): `map_level` drops its instances (0x2D's placements 40-45, 0x34's 29-30). They are solid in the match but never drawn (V quad3 DL 130: slot 0's camera faces placement 40 from 700 units and draws no batch within 300 of it; I on 0x34). `pack-readme.md` rows |
| extract-gbi `dl.rs` | `State::texture`: when a draw's G_TEXTURE tile was described for an earlier image at the same TMEM address, it takes the tile described since, for the newer load. Bank-09 #0x1A is the case: G_TEXTURE picks tile 1, its 64 × 32 image describes tiles 1 and 0, and the next three images (3790, 3796, 3112) re-describe tile 0 only. The rule does not apply when G_TEXTURE chose its tile after that description (bank-01 #0x00 and #0x45 describe tile 5, then choose tile 0). C: F3DEX2's triangles carry G_TEXTURE's tile, so the console samples a 64 × 32 window over each new 16 × 16 image and the old image's leftovers (I, not compared on screen). The export takes each image's own tile instead, a deliberate departure |
| conker-client `projectiles.rs` (new) | `fly` matches the `MatchView`'s shots, grenades and rockets in order per list (reach a frame: 2 bolt steps, 60, 400). Each shows its model from its second view (I), +z along its step. The rocket is at 0.047, nose ahead (V rocket DLs 120-124). The grenade is at 0.5, pointing along the ground and rolling forward 32.7° a frame from 32.7° up (V gren4 DLs 119-129, its frames 16-26, within 0.6°); it then settles from its last frame in the air (V −98° at DL 129, then −59°, −17°, … to 3°; here × 0.775 a frame, I). The knife is at 0.5, 40° up, then rolls 30° a frame (V knife DLs 121-132). The bolt is at 0.5, turned 60° about its length, then 30° back a frame (V bolt2 DLs 151-158). Bullets have no model (V twin DLs 104-110). `flames` gives each flamethrower (0x15) holder a stream. From `DELAY` frames into the fire clip, a point a frame leaves the nozzle (74.1 ahead, 9.9 left, 52 up, 4.8° left of the facing) at 41.7 a frame along the stream. It rises 9.1, easing toward 16.5 by 0.16 a frame. Its width is 16, +6.6 a frame; its alpha is 200 for 9 frames, then fades out over 7. The first wall ahead of the nozzle stops it. The ribbon faces the eye, with `flame.png` repeating every 218 units and scrolling (V flame DLs 106-170, the stream's 15 records at 0x8021DFF0) |
| conker-client `held.rs` | `twist` runs after the clips. `joint_01` (the upper body, with the arms and guns) turns by −twist about up, to `facing − twist` as the camera aims; `joint_12` (the head) turns back to the legs' facing (V twin DLs 165/186/230: `joint_01` is at 5.3° on the root at twist 0, 54.9° at 9000, −9.3° at −2660; the arms and guns within 1.6°) |
| conker-client `beach.rs`, `render.rs` | `rocket_shakes` runs on every map, before the Beach gate (800/2000 at the rocket's last spot: the rocket pool's `func_151C05A4`, C). `render.rs`: the spheres are gone, and `fly` and `flames` run in their place. The HUD's setup is native-only (on wasm it spawned nothing) |
| Fades (item 7) | Already covered, nothing to change: every fade (capture, escape, end, RESTART, outro) is a HUD quad, and every quad's alpha goes through E3's `display_alpha` (`hud.rs`, `hud_alphas_blend_as_the_console`) |

| Check (2026-10-13) | Result |
|---|---|
| extract-gbi `a_stale_texture_tile_gives_way_to_the_newer_one` | #0x1A's sequence: the first image at 64 × 32 on tile 1, the next two at 16 × 16 (line 2) with their own palettes |
| conker-extract `slice_in_memory`, `textures_counts` | every held material is textured; 09-1a-00 draws 3790, 3796 and 3112 at 16 × 16. `textures()` over banks 01, 03 and 04: 2,894 PNGs, and every mesh's textures equal HEAD's (the first form of the rule changed bank-01 #0x00's draw 27 and #0x45's draws 3-4; the G_TEXTURE condition restores them) |
| conker-extract `projectiles_and_undrawn_quads` | the four models' vertices 28, 16, 34, 23, the rocket's z −316…484, 4 scenes; 0x2D's scene level has 6 class-51 instances and 0x34's has 2, and their map levels have none |
| conker-client `projectiles_turn_as_the_original` | the knife's z at frames 0 and 2 against knife DLs 121/123, its side axis up × flight; the bolt's x at 60° then level (bolt2 151/155); the grenade's z at frames 16 and 18 against gren4 DLs 119/121 (within 0.01), its frame-26 pitch −98.1° ± 1, lying along the throw at rest; knife or bolt by the weapon |
| conker-client `the_flame_stream_follows_the_records` | flame DL 130's records: 41.7 a frame along the stream, rising 9.1, then 12.2 ± 0.3 on its fourth frame (V 12.25); 35.8 wide then; 15 points; alphas 29, 57, … 171, 200, 200; 30 ribbon vertices; a wall 200 ahead keeps 4 points short of it |
| conker-client `the_twist_turns_the_upper_body_to_the_aim`, `a_rocket_shakes_every_map` | the upper body faces `facing − twist` (49.44° at 9000) with the head kept on the legs; a rocket gone on a Deathmatch map leaves one shake at its last spot, dropped once over |
| `bun run test`, `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass (417 tests, with M2's work in the tree): the `textures_counts` and rustfmt/clippy failures listed under M2 below are fixed |
| Pack | `bun run pack --force`, then `../lab/tools/verify-pack.sh`: "pack equal" (1,396 files). Against the pack before E8: new `weapons/deathmatch/projectiles.{gltf,bin}` and `flame.png`; `held.{gltf,bin}` (materials 15-17 textured; meshes and nodes equal); levels 2d and 34 without the class-51 quads |

Screenshots (`../lab/screens/visuals/shots.sh`, ports 5980-5988, offscreen 1280 × 720, release build in `target-net`;
Deathmatch on 0x2D, slot 0 at (−2247, −521, 6015) facing 16896, given the weapon at read 40, the same scripts as the
oracle's runs). The clients' reads trail the server's by 2-3 (the view's delay), so the side shots come a few reads
after the oracle's lists. `rocket-side2.png`: the rocket over the trapdoor, nose ahead (read 128). `grenade-side.png`
(in flight) and `grenade-rest.png` (lying along its throw). `knife-side.png` (tumbling, blade first),
`knife.png` (the scope's view as it leaves). `bolt-side.png` (tip ahead). `flame.png` and `flame-side.png` (the stream
rising from the nozzle). `twist.png` (slot 0's aim view, R held, the stick right) and `twist-free.png` (the same from
the side: the upper body turned on the legs). `quad2.png`: wave 4's duel spot without the white quad (it was at the top left). `rocket.png`, `grenade.png`
and `bolt.png` are slot 0's views at the throw or shot; their projectiles are small, hidden or still in hand.

**Gaps.**
- **I, not drawn:** the muzzle flashes (the twin guns' bank-09 #0xDF model and texrects of blobs 0x923, 0x924 and
  0x926; the rocket's launch flash uses the same blobs). The rocket's trail (texrects of blob 0x78A, 7-13 units, 4-5
  every other frame). The stream's puffs (texrects of 0xBA6) and tongues (ribbons of 0xBA7 at the nozzle).
- **I, approximations:** a projectile shows from its second view, one frame later than the original's first list, plus
  the frame its scene takes to spawn. Knife or bolt comes from the weapon of the player nearest where it appears. A
  bounce is a fall that stops (the views' steps are uneven). The grenade's settling is a decay, not V's wobble (up to
  20° off at its frames 30-36). The stream's nozzle doesn't follow the twist, and the head's own drift under the
  twist (V 7.5°, 17.2°, 28.2° on the root) is not modelled.
- **C/I:** the twin guns' three materials show their own 16 × 16 images, where the console's G_TEXTURE tile would
  sample a 64 × 32 window (above).
- **Not traced:** the burning player off 0x34 (traps.rs draws it there only). `fade_escapees` fades a textured 3D model
  in plain alpha, without `display_alpha` (it has no single luminance). On 0x34, bank-03 #0x71 at placements 31 and 32 (among
  others) is still untextured.

## Deathmatch on 0x1A, the Vault (2026-10-13)
**Batch M2, the rules and client half (the pack half: c19ad4a). `--map 1a` plays the original's Deathmatch scenario
6: everyone is the Wise Guy (class 1, model 59, the squirrels' body) in their team's shirt, holding the bat. The first
four pickups (tommy gun, bazooka, knives, crossbow) roam over spots 0-7, and every pickup is back 150 frames after a
take. The tommy gun fires a bullet on the press, then one every 4 frames, with no clip, and its aim climbs. Bots run
Heist's script 0xB7 (its FFA branch). The level is lit by its 34 lights, five of which flicker with a buzz. Song 30
plays (the pack's `background.wav`, sequence 30 recorded on the oracle). Protocol 24.** Research: `../lab/research/mp-original/heist.md` §1, §3, §3.1 (oracle runs in `/tmp/m1`: dm6, h3, h4, bots1).

| Part | What |
|---|---|
| conker-rules `weapon.rs` | `BAT` (0x19: the club's handler, draw 0x10E / put-away 0x110, 15 frames each, C; V h3), `TOMMY` (0x38: draw 0x1DD, idle 0x1DF, put-away 0x1DE, `Attack::Bullets` of type 0x35 = the pistol bullet's numbers, cycle 4 at +3, `rounds` 0), `tommy_clip` (0x1E0 / 0x253 / 0x254 by speed, C), `Climb` (s126+0x1A2/0x1A4: the rate +10 a VI, min(climb, 175) + rate a VI, capped 0x1E00, 0 when Z is let go, V h4), `Weapon::club()`. `PICKUPS_1A`, `VAULT_SPOTS` (12), `SPOTS_1A`, `VAULT_RESPAWN` 150, `Roaming` (`pick`: `(int)(r · 0.99999 · n)`, drawn again while any pickup holds the spot, `func_1508DC24`, C), `pickups(0x1A)` |
| conker-rules `game.rs`, `spawn.rs` | `Arena.roaming`, `Arena.respawn` (derives `Default`), `PickupState.spot`, `Match::pickup_at`. A roaming pickup picks a spot at each appearance: the first frame, then each return. The touch test uses its spot. The bat runs the club's rules: R blocks Z, the combo, and the club's walk and run. The tommy gun fires on the press (n = 0), with no clip check for `rounds` 0. `Fighter.climb` steps while it fires with Z held, and `strike` pitches its bullets up by `climb >> 8` steps of 200 angle units. Its fire clip follows the speed every frame. `in_hand`: the bat attaches 14 frames into its draw (its held model's event). `starts(0x1A)`: Deathmatch records [0-3, 20-31] (C, V dm6), masks all ones |
| conker-sim `net.rs`, conker-server | `PROTOCOL_ID` 24: `MatchView.roaming` (placement, position) and the fighter's `climb`. `--map 1a` (Deathmatch only): `Game::vault`, so every player is a squirrel (its asked side ignored) with class 1 and start item 0x19, on slot s's Deathmatch record. `arena()` reads the pack's `roams` (the leading pickups) and `respawn` (VIs / 2) |
| conker-bots | `script(0x1A)` = 0xB7. `mirror`: RESPAWN from the arena, PICKUP_FLAGS 3 for the roaming pickups and 2 for the fixed ones (V dm6), NODE_OF from `pickup_at`, G[0x10F0][k] = k (`g::SPOT`), G[0xF70][k] = −1 (holder). Query 0xD3 gives the bat 0 (C `func_15099C14`). ROM test `vault_deathmatch_bots_fight` |
| conker-client | `WISE_GUY` = "59" in `characters("1a")`; `body()` maps class 1 to it. `render.rs`: `dress_teams` (each Wise Guy gets its own copy of `material_02` with `team-<team & 3>.png`, the shirt; V tm4, tm8). `show_pickups` moves the roaming nodes to `MatchView.roaming`. `audio.rs`: the bat's hits are melee hits and the tommy gun's are bullet hits; a roaming pickup sounds from its spot; the switching lights buzz. `lights.rs`: lights 10, 11, 12, 20 and 29 switch (on → off 35/1000 a frame, off → on 70/1000), and the level is relit from its 10 nearest lights |
| web `data.ts` | `MapId` "1a", `MAPS["1a"]`: "Vault 0x1A", Deathmatch only |

| Check (2026-10-13) | Result |
|---|---|
| `tommy_gun_fires_and_climbs` (V h4: Z read 160-400) | 62 bullets (s126+0x1AA: 1 at read 161, then +1 at 164, 168, … 404; heist.md's "61" counted from 164); climb 40, 120, 240, 335, 375, … 0x1E00, 0 the frame after release; back to `Free` 7 frames later |
| `tommy_climb_is_h4s`, `roaming_picks_a_free_spot`, `table_is_consistent` | h4's sequence; 20 picks with 7 spots held all land on the free one, none when all are held; `SPOTS_1A` = spots [0, 2, 3, 1, 8-11] as the pack |
| `vault_pickups_roam` | first frame: k0-k3 on four distinct spots in 0-7; k2 taken at t is still away at t + 149 and back at t + 150 on a spot none held; the others don't move |
| `bat_swings_as_the_club` (V h3) | draw clip 0x10E; Z at 20 hits at 30 (10 frames into 0x113) for 1 |
| `vault_players_hold_the_bat` (server) | 6 bots: all squirrels, start 0x19, class 1, mask all ones, on records 0, 1, 2, 3, 20, 21 (jitter ≤ 40) |
| `draw_events_fire_on_the_attach_frame` (client, + 0x19, 0x38) | the pack's held-model events fire on `in_hand`'s frame: the bat 15, the tommy gun 11 (the default 10 holds) |
| `vault_deathmatch_bots_fight` (ROM, 3 bots, 3,572 frames) and `bots.sh` `MAP=1a` (ports 5970-5974) | 3 bots: 7 pickups, 16 hits, 4 kills, 4 deaths, against the oracle's dm6 (1 idle human + 3 bots, 119 s): 7, 17, 4, 4. 5 and 7 bots are within one run's variance of the oracle's. No unknown native |
| Client tests (96), `bodies_by_map_and_class`, `lights_switch_off_and_on`, `a_vertex_takes_its_nearest_lights` | pass |
| Screens (`../lab/screens/vault/shots.sh`, ports 5976/5978, 1280 × 720) | `bat.png` (slot 0 draws the bat), `tommy.png` (slot 1, blue shirt, firing the tommy gun; slot 0, red, with the bat), `roam.png` (a roaming pickup on its spot), `relit.png` (the relit level) |
| `bun run test` (all lanes' edits in the tree) | every suite passes except conker-extract's `textures_counts` (2,895 against 2,894), from E8's uncommitted extract-gbi edits |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | fail on E8's work in progress only: rustfmt in `beach.rs`, `projectiles.rs`, conker-extract's `tests/rom.rs`, `zz_e8_dump.rs`; clippy on `render.rs` `setup_combat` (`commands` unused on wasm) and `projectiles.rs` (9 arguments). This batch's files are clean, and web `bun run check` passes |

**Gaps.**
- **I:** the roaming spot is picked at the return, not at the take (both fit dm6's 5 returns). The climb's pitch is added
  to the bullet's own pitch in `strike`; the original subtracts `0x1A2 >> 8` from the stick-y the aim reads, and the
  pitch byte's 3-step chase is left out. The tommy gun stops 7 frames after Z, the twin guns' rule, and walks and runs
  with the plain gait. The bat's attach frame (14) comes from its held model's draw event: s126+0x11A was not traced.
  The respawn masks (all ones) include the lift-end records 4-7; dm6 only showed 13, 14 and 17. The bat's reach is the
  club's (the same handler, C), not measured on 0x1A. The tommy bullet's damage-type id 0x35 is inferred from its numbers.
- **V, other:** flag 1 (roam, gone while held: team mode with ≤ 4 participants) and its death drop are not ported:
  Deathmatch is always flag 3. Bots never took k4-k7 in the oracle (bots1) and rarely do here. `weapon_class(0x40)`
  stays 3 (0x3F's script reads it). The Total War takeover's holder-table write is overwritten the next frame (no
  script reads it today).
- **Client:** the bases' hum zones don't stop, the radar's blips don't take the team colours, and the text HUD names
  "weapon 0x19/0x38".
- **For M3 (Heist):** the four teams and their sizes, the money bag (character 87 on the nav graph, class 0x57's
  scripts), carrying it (states 20-24), the deliveries and scores, flag-1 roaming, the base zones, and script 0xB7's
  team branch with its outros.

## Deathmatch maps: traps, zones, the restart fade (2026-10-13)
**Batch E10, the rules half. 0x34's pressure plates (placements 6-9) sink and rise as the original's and switch on its
traps: 27 fire jets for 120 frames, the electric floor for 151; one touch kills, credited to the presser. Its 7 burning
cones hit, throw and set alight. The action-1 records of 0x30, 0x34 and 0x3F are the floor search's zones. The camera's
box sits half its height above the eye, which fixes 0x34's corridor sweep. RESTART fades for 17 ticks (`MatchView`
phase 3). Protocol 23. The client draws the plates at their y, the jets' flames and the electric floor's bolts and
arcs, with their sounds.** Research: `../lab/research/mp-original/dm-maps-results.md` §1-2, §5.5 (scratch
`dm-maps-results-scratch/out`: box34, trap34, elec34, cone34, cone34k, c34cam).

| Part | What |
|---|---|
| conker-camera `volume.rs` | `CameraLevel.floor_zones`: the type-1 records with action 1 or 3, on at load. `full_floor(pos, floor_y)`: the last zone the player is in has action 1 (obj+0xF8 bit 2, C) |
| conker-sim `collision.rs`, `movement.rs` | `last_floor(start, floor_y, floor)`: last frame's triangle for the floor query's fast path, none inside a full-search zone. Used by the ground step, the air step, the burning run and the knock-back. `placement(tri)`. Agent A: `Body::camera`'s centre is `height >> 1` above the eye (`func_15044660`'s type-0x2D case falls through to the common tail, C; V the game's results sit 30 + 30·n.y off the wall plane) |
| conker-rules `traps.rs` (new) | `parse` (kind 12, action 9: the fire jets by word 0x1C in the plates' lists at 0x800A2380/238C/2398, the electric boxes 43 and 82 by arg 8), `Plate::step` (`func_151177C0` then the contact routine: 0 → 2 on contact; 2 sinks 6, to −10 → 3 and the callback; 3 holds while stood on, else 1; 1 rises 3), `Traps::step` (the clocks, the plates, the switch-on with the owner = the lowest slot on the plate), `hits`, `on()`, `offsets()`, `seat` (`CollisionWorld::shift` on placements 6-9) |
| conker-rules `game.rs`, `damage.rs`, `flame.rs`, `surface.rs` | `Match.traps`. At the frame's start `trap_hits`: each player inside a trap that is on takes `TRAP_FIRE` (0x34: reaction 0x2B, 7) or `TRAP_SPARK` (0x2E: 0x21, 7), guarded, at its own facing, credited to the owner. After the players `cones`: grounded, alive, on a kind-0xE floor → `CONE` (type 1: 0x10, −1, xz 7, yv 55, g 2), unguarded, no attacker, and `Burn::new(None)` (`Burn.attacker` is an `Option`). At the frame's end `plates`: the contacts (floor on the plate, ≤ `CONTACT` above), `Traps::step`, the riders moved by the plate's dy. The flame hit (0x10, lethal or not) keeps its 0x69 until a landing that bounces under 28 and 5 frames in; a lethal one then falls (0x58 52, 0x59; V cone34k). A death that starts grounded on kind 0xE/0x13 plays `CONE_DEATH` (clip 0x2D8, 20 frames I, then the dead pose). `places`' doc: the slot clause is dead code |
| conker-sim `net.rs`, conker-server | `PROTOCOL_ID` 23. `MatchView.plates` ([i8; 4], below rest) and `traps` (u32, bit k = trap k on). `phase` 3 = RESTART's fade, `restart_in` 17 → 1 there: the votes set the next match 17 ticks on (`RESTART_FADE`), the deadline at once; votes clear at each match start. `Ai::traps()` at setup and at each restart; `traps::seat` before the frame. Log: `TRAPS t=… plate k down: n traps on for slot …` |
| conker-client (hooks) | `seat_props` seats the plates on 0x34; phase 3 is over for the prediction, the HUD text (`lib.rs`) and the muffle and music (`audio.rs`); `lights.rs`' `< 2` already was. `outro.rs` keeps `== 2`: `results.rs` needs `outro::results` false in phase 3 to ignore the pad |
| conker-bots | No native to add: 0xBD's prop command 7, command 4, trigger query 0x10 and `query(0x2000, 0xAC)` are answered already. The ROM tests' Deathmatch helper has the traps (as the server) |
| conker-client `traps.rs` (new, map 34 only; agent B) | The plates at rest + `MatchView.plates` (PostUpdate, after `show_pickups` puts the props back at rest). A jet that is on: its flame quad turned to the camera, scrolling, flickering and fading to the top (C, V), particles from its base (rate C, motion and sizes I). The electric floor: bolts from (303, 39, 2591) down and arcs on each box's base, cyan 9-point ribbons (C, V; reach and the arcs' ends I, lifted 4). A burning player's particles (I). `audio.rs`: 0x1AA at the middle jet of each row (record flag 8: traps 1, 4, 17, … = records 28, 31, 64, V), 0x236 once for the floor at (303, −417, 2591), looped while on, stopped as their bit goes |
| conker-extract `traps.rs` (new; agent B) | `maps/34/traps/flame.png` (blob 0x916), `puff.png` (0xBA6), `arc.png` (0x753), `audio/maps/34/0566.wav` (0x1AA stays `common/0426.wav`); `pack-readme.md` |

| Check (2026-10-13) | Result |
|---|---|
| `floor_zones_switch_the_fast_path_off` (conker-sim oracle, the sequential replay) | zones 8 / 33 / 6. Rows off: 0x30 374 → 160 (214 fixed, 0 broken), 0x34 2,471 → 2,311 (161 fixed, 1 broken), 0x3F 8 → 8 |
| `the_plates_replay_box34` (conker-rules) | the 4 plates' y and state equal box34's RAM on all 310 reads; 5 bottomings (6 at 59 and 101, 7 at 88 and 129, 9 at 28) |
| `the_plates_hold_the_0x34_floors` (ROM, `s4_h1b15`) | contacts and plates equal box34's RAM on 312 reads; grounded rows off: 937 with the plates at rest → 63 with them moving (26,444 rows); 14 bottomings, no one in a trap that is on |
| `the_traps_of_0x34` (ROM) | the plates' lists equal the ROM's 9-byte arrays; 27 fire ids 0-26, the electric boxes 43 and 82, all off at load; none on 0x2D |
| `a_plate_sets_off_its_traps` (ROM; V trap34, elec34: put on at read 80) | P2 in jet 1 hit in frame 83, P4 on the electric floor in 84, both to 0 health; kills [1, 0, 1, 0]; the jet on frames 83-202 (120), the floor 84-234 (151, as elec34's `func_1515FBC4` calls) |
| `a_cone_throws_and_burns` (ROM; V cone34k) | hit in frame 40: health 0, y velocity 55, alight with no attacker; y within 0.005 of the oracle's over reads 41-139 (the hop, bounces 28 and 16); 0x69 to read 112, then 0x58, not 0x2D8 |
| `a_death_on_a_cone_plays_0x2d8`, `restart_fades_for_17_ticks`, `places` | a death starting grounded on kind 0xE plays 0x2D8, on kind 5 the fall; `restarting` 17 at D, 1 at D + 16, none at D + 17; the research's [1, 4] |
| Agent A: `corridor_camera_calls_match_the_game`, `corridor_camera_matches_the_game` (c34cam) | sweeps that differed 143 of 415 → 0, rays 475/475; the eye at reads 219-290 (up to 344 off) → 0, reads 113-335 within 0.004; 0x2D's camera outputs unchanged |
| `bots_set_off_the_0x34_traps` (conker-bots ROM, 15 bots, 1,737 frames) | seed 1: 5 presses, 5 trap kills, no unknown native. The oracle: 14 presses, no one inside a trap that is on |
| Run `bots.sh` (port 5970, `MAP=34 BOTS=15`, 90 s) | 48/48 ok, no faults or unknown natives; 15 `TRAPS` lines; e.g. plate 1 by slot 13 at t 707, slot 8 killed in its jets at 791 credited to 13; walking into one's own jets is a suicide (t 172, 436, 610) |
| Run `../lab/screens/map34/rules.sh trap` (port 5972; 2 idle humans, 1 life, warps at read 300) | `TRAPS t=306 plate 0 down: 9 traps on for slot Some(0)`, `Hit { victim: 1, attacker: 0, damage: 7, lethal: true }` at read 303 (F + 3); slot 0's HUD: kills 1. Both press A on the results: "next match at tick 948", started at 965; again 1399 → 1416 (17 ticks of phase 3) |
| Run `rules.sh cone` (port 5974) | read 300: `Hit { victim: 1, attacker: 1, damage: 1, lethal: false }` (unowned); the burn's ticks are refused (immune), as cone34's health stays 5 |
| Screens (`../lab/screens/map34/traps.sh`, port 5978, 1280 × 720) | `jets.png` (plate 6's row lit), `electric.png` (a bolt and a floor arc, slot 1 on plate 9), `burning.png` (slot 2 alight from cone 1), `plate-up.png` / `plate.png` (plate 6 at rest, then down: a pixel diff shows it moved) |
| `bun run test` (all lanes' edits in the tree) | 402 pass, 1 ignored, 0 fail |
| `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1389 files (map 1a and the Heist characters from the other exporter lane included) |
| Run `rules.sh camera` (port 5976, c34's script from start record 9, `DRAWN` lines) | the walk completes. The eye starts at the record's eye (y 995), the original's re-cut after its poke (650), so the two differ by 345 in y to read 200; from read 260 within about 50, the positions within 40 (a live run's timing) |

**Gaps.**
- **I:** `CONTACT` = 5 (box34: contacts set 3.9 and 1.9 above the plate, not 23.5 and 25.5). The riders go with the plate
  every frame; the original's lag it by a frame at times (box34 58-60, 99-101). 0x2D8's 20 frames are its clip's length.
  The flame hit's 0x69 ends one frame early (read 112 against 113): the rule "a landing bouncing under 28" fits e_w15
  and cone34(k) only. The lethal flame hit's sequence (0x69, 0x58, 0x59) is V for the cone, I for the flamethrower.
- **V, other:** the bots spend 2-3 times the oracle's frames in the traps' zones (seeds 1-4: 3-10 presses, 3-7 kills a
  minute against 14 and none): their paths, not the traps. The deadline starts the next match without the fade (the
  original's deadline quits).
- **Prediction:** a client's `Match` has no traps, so a local player on a moving plate, in a jet or on a cone is
  corrected by the server (a few units on a plate; the hop and the kill arrive as corrections).
- **Camera, agent A:** from read 336 the remake's target eye drifts (camera logic, not the sweep); at reads 643-774 the
  original's eye is 40 above a y 79 platform and the remake's is not (I: the camera's floor clearance). `walls.rs`'
  unused `dd3` branch sets `at[2] = 0.8`, which the original's delay slot undoes.
- **Client, agent B:** the electric floor's sparks (blobs 0x904, 0x908, 0x1042) are not drawn; the arcs run edge to
  edge where the oracle's are about 150 long with their ends inside the box; a burning player's fire is drawn on 0x34
  only. In `plate.png` the player's feet look a little sunk into the plate (unchecked).
- Not ported: the camera script's zones (0x30's 0x3C/0x3D, 0x34's 0x6D: only the depth-buffer occlusion swing, which
  the remake lacks); kind 17 (read by nothing). Not this batch: §5.7's hits tests and the headshot on every 0x2E hit.

## Map 0x1A, the Vault, and the Heist characters: the pack (2026-10-13)
**Batch M2, the pack half. The pack has map 0x1A (the Vault, Heist's map and Deathmatch scenario 6) like the other
maps, the Wise Guy (59) with Heist's four team looks, the money bag (87), the bat (0x19) and the tommy gun (0x38) in
hand, on the HUD and in sound, and 0x1A's music. Research unknown 1 is settled: a team changes one texture of model
59, nothing else (V). No rules, server or client change: M2b plays it.** Research: `../lab/research/mp-original/heist.md`;
the oracle's answers: `../lab/oracle/map1a/props_pickups_lights.md` (props, pickups, lights), `../lab/oracle/map1a/teams/`
(team looks), `../lab/audio/map1a/` (music).

| Part | What |
|---|---|
| conker-extract `lib.rs` | `VAULT`, `WISE_GUY`, `CASH`. With 0x1A in `--map`: 59 on the Tedi's rig (as the villagers), 87 as a prop character (own rig, as the flags). `pickup_table` (0x1A's 8 pickups in manager order: placements 10, 1, 2, 3 the tommy gun, bazooka, knives, crossbow, which roam; 8, 9 sniper rifles, 11, 12 grenades, on spots 8-11) and `ai_script` (0xB7). Spots are nodes by id (byte 7, not the index) + (0, 60, 0). `hidden_placements(0x1A)` = 4-7. `map_lights` reads 0x1A's 34 type-9 triggers. Team looks: `team_ids` (the model's texture table, header word 6, entries 1-4), `bind_segment` (segment 0x0A → team 0's texture in the glTF), `characters/59/textures/team-0..3.png`, `character.json` `teams`. `HELD_CLIPS` + 0x10E (bat), 0x1DD (tommy) |
| extract-scene | `deathmatch_solid(0x1A)` = [0] |
| conker-extract `hud.rs` | Icons: 0x19 `atlas` (25, 20) 22 × 7, 0x38 `atlas2` (25, 19) 22 × 9 (V) |
| conker-extract `audio.rs` | `MATCH_CLIPS` + the bat's 0x10E, 0x110 and the tommy gun's 0x1DD-0x1E0, 0x253, 0x254. The tommy gun's loop: 719 while firing, then 727 and one of 747-749 at 16000 (C `func_1000EFB4`, as the twin guns'). Battle cries with it: the squirrel's, the Wise Guy's (1347-1350, one start in four). Pickup sounds: 688 for 0x38; the Wise Guy's voices (1346; 1342/1343 on the bazooka). Hit reactions: bat as the club, tommy as the pistol (I). Cues of a character not packed are dropped. `map_sounds("1a")`: song 30, all 16 channels, volume 8000 (C `func_150000B0`, V); emitters: the two lamps (as 0x2D's) and the five switching lights (1099-1108 at 16000, 500/1000, as each comes back on; C). The bases' four zones (1116) come from `zones()` as is |
| conker-assets | `Pickup.respawn`, `Pickup.roams`; `Character.teams` (`Teams`: material, textures); `Lights.switching`; `nav.json` keeps the header's word 2 and each node's byte 15 (`{"word_2", "nodes"}` on 0x1A only; other maps' files unchanged). The sample pack covers each; round trips stay bit-exact |
| Tests | `tests/pack.rs`: 0x1A in the read-back (ROM path, solid rule, spawns, pickups on spots, nav, triggers, AI 0xB7, level glTF), a Vault block (lifts 4, no kill gate, respawn and roams, lights, 59's clips, lift 49, team material and PNGs, 87's rig). `tests/rom.rs`: 18 held models, the bat's and the tommy gun's events |
| `../lab/audio/map1a/background_loop.wav` | Song 30 recorded on the oracle (DM scenario 6, players moved out of their bases' zones; Heist's scenario 0 plays the same song, correlation 1.000): period 4,873,056 samples (221.301 s at 22,020 Hz, 26,484 audio blocks), cut from 60.2 s with a 0.5 s crossfade, seam difference 0. Tools: `../lab/oracle/map1a/tools/` |
| Pack | Re-exported (`--force`, `--map 1a --background 1a=../lab/audio/map1a/background_loop.wav` in `bun run pack`). New: `maps/1a/` (level 75 textures, collision 1892 faces, 32 spawns, 8 pickups, 68 triggers, nav, ai.bin, 34 lights), `characters/59/`, `characters/87/`, `audio/characters/59/` (20 WAVs), `audio/maps/1a/`. Changed: `weapons/deathmatch/held.*` (18 models, 2 new textures), `hud/hud.json`, `audio/events.json`, `audio/sounds.json`, `pack.json`, `README.md` |

| Check (2026-10-13) | Result |
|---|---|
| Oracle: team looks (`ConkerRecomp.m1`, runs tm4, tm4b DLs 96/100, tm8, h4i) | Model 59's material 2 draws segment 0x0A: REDS 7246, BLUES 7230, GREENS 7231, YELLOWS 7232 (CI8 32 × 32, RGBA16 palette). Prim, env and fog colours and every other command are the same for all teams. The pack's `team-0..3.png` equal the oracle's RAM textures within 1 (5-bit rounding) once flipped upright and de-interleaved (`../lab/oracle/map1a/team_pngcmp.py`) |
| Oracle: props, pickups, lights (dm3, dm15, hs7, lt3, walls6c/6d/13; `props_pickups_lights.md`) | Solid {0} only (13 is drawn but the match sets `+0x4F` 0x20 on it; lldb: only 0 is ever a candidate). 4-7 hidden all match. Every pickup is at its node + (0, 60, 0) on 63,121 pickup-frames, 0 mismatches. k4-k7 on spots 8-11. Respawn 300. 34 lights = the 34 type-9 triggers, ambient (94, 94, 96), every level vertex lit |
| Oracle: HUD icons | 0x19 and 0x38's rects as above (V) |
| conker-assets tests | 10 pass: the sample covers `respawn`, `roams`, `teams`, `switching`, `word_2`, `byte_15`; round trips bit-exact |
| conker-extract tests | `pack_reads_back_as_the_rom_path`, `pack_audio_reads_back` (0x1A's background, zones, emitters, song note; the tommy gun's and the Wise Guy's cues; 59's clip events), `tests/rom.rs` pass |
| `bun run test` (`--no-fail-fast`) | 400 pass, 1 ignored |
| `bun run check` | `cargo fmt --check` fails only on the other lanes' files (`conker-extract/tests/traps.rs`, `conker-server/src/main.rs`); clippy (`-D warnings`, all crates) and the web check pass |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1389 files |
| Native screenshot of 0x1A | Not taken: `conker-server --map 1a` refuses the map ("not 2d, 30, 34, 3f or 2b"), so the client cannot load it before M2b |

**Gaps.**
- **I:** k0-k3's start spots: the game picks random free ones of spots 0-7 (V: dm3 [3, 5, 6, 0], dm15 [6, 7, 4, 2], hs7 [0, 2, 7, 3]); the pack puts each on the spot nearest its placement (`position`, and its level node).
- **I:** the tommy gun's hit reactions use the pistol bullet's numbers (heist.md: type 0x35 not traced); the bat's are the club's (C, shared handler).
- **I:** the switching lights' chances are per frame (C gives the chances; the oracle's on/off spans agree with per frame).
- **V, not in the pack's model:** 0x1A's base zones stop when the player leaves (V, read 25), while `zones` says "starts on the first entry and never stops" (0x2B's, V). Only zone 0x37's sound is restarted by the script at read 24.
- **V, not in the pack's model:** the pulse lights' size is truncated (trunc(50 + 40 sin φ)); the original lights each level vertex by its 10 nearest lights.
- **C:** the header's word 2 (61) and node byte 15 of 0x1A's nav graph are kept as is; their meaning is not known (the bots ignore both).
- **C:** the money bag's logical-to-pair mapping (README) is read from its bank-02 table; its 0.4 scale is the scene record's (heist.md).
- The outro actors 71 and 134 are not packed (M4).

**Handoff to M2b (rules, server, client).**
- Rules: `conker_bots::script(0x1A)` → 0xB7 (the pack's `ai.bin` already is); `weapon::pickups(0x1A)`: move `VAULT_PICKUPS`/`VAULT_SPOTS` from conker-extract into conker-rules (then `pickup_table` can call it). Respawn from `pickups.json` (300 VIs). Roaming: at each (re)appearance a random spot of `roams` no other pickup stands on (manager flags 3 for k0-k3, 2 for k4-k7). The server refuses `--map 1a`: add it.
- Weapons: the bat 0x19 (start item; draw 0x10E, shown at 15; put away 0x110, hidden at 14 (held events, C); idle 0x10F; swing 0x113, hit 10 frames in, 1 damage). The tommy gun 0x38 (draw 0x1DD, shown at 11; put away 0x1DE, hidden at 12, so heist.md's "idle 0x1DE/0x1DF" is 0x1DE put away, 0x1DF idle; fire 0x1E0 standing, 0x253 moving, 0x254 above speed 20; 1 bullet per 4 frames, 2 damage, 26-unit knock-back over 3 frames, 18-frame stun; no reload; the 0x1A2 recoil climb).
- Characters: every Heist player is class 1, model 59; `characters("1a")` adds "59". Its team look: in `character.json` `teams`, material 2's texture ← `textures/team-<team>.png`. Radar blips by team: REDS ff3737, BLUES 4040ff, GREENS 78ff16, YELLOWS ffb11e (V).
- Client: roaming pickups move their level node to the server's position at each (re)appearance; `lights.json` `switching` and the switching lights' emitters; the level lit everywhere (as 2b's relight, ambient (94, 94, 96)); the background (song 30); the zones' stop on leaving.
- M3/M4: the money bag (87, `characters/87/`, packed with 0x1A) and the outro actors.

## The results screen's details (2026-10-13)
**Batch E10, the client half. An eliminated player's view is the original's one view: black over the whole HUD, with
only the skull on it (and the end fade later). The chosen STATS button pulses: its lit tiles go over its dim ones at the
STATS' alpha × (3v + 63) / 256, a 32-frame triangle. The stick between the buttons steps through its six frames and
back. In the end phase's fade the radar, kills and rank go at once, the clock a frame later. RESTART's 17-frame fade is
drawn from `MatchView` phase 3 (protocol 23, the rules lane's). Docs: the P3/P4 colours, the accuracy rule and ties;
the STATUS notes the research found wrong are corrected in place.** Research:
`../lab/research/mp-original/dm-maps-results.md` §5 (5.1-5.8).

| Part | What |
|---|---|
| conker-client `hud.rs` | Out: `black(255, OUT_Z)` at z 7.0, over every HUD part (the reticle, scope and gun sight at 6.0-6.5 too); the skull at 7.5; a War strip at 8.0 over it (I). `Shown.ending` is the end phase's frame k from 1: no radar, kills or rank, and no clock from k = 2. `Shown.results` also carries the STATS' alpha and the VI counter (2·tick). The alpha is applied in `sprites` (it was a multiply over every quad in `draw`). The chosen button draws its `image` at 255 and its `lit` over it at `glow(a, vi)` = min(255, (a·(3v + 63)) >> 8), t = (2·vi) & 0x7F, v = t < 64 ? t : 127 − t. `stick(vi)`: n = (vi >> 1) % 10, 10 − n from 6, a 32-texel uv step. `restart_fade(m)` = `end_fade(17 − restart_in)` in phase 3; `draw` then shows only that fade (no HUD, no STATS). `black()` is shared by the fades |
| `results.rs` | Doc only. In phase 3 `outro::results` is false, so the menu resets and ignores the pad |
| conker-extract `hud.rs` | `stick.png` is the six tiles 0x7E7..0x7EC side by side (192 × 32; it was 0x7E8 alone). The P3/P4 colours' comment is C (the table 0x800ABA90). `pack-readme.md`: the pulse, the stick's frames |
| conker-assets `hud.rs` | `Results`' doc: the lit pulse, the stick's strip. No new field: the ping-pong is the original's code, and the strip's width / 32 gives the frames |
| Pack | Re-exported (`--force`): `hud/stick.png` (192 × 32), `hud/end.json` (the stick texture's size) and `README.md` changed |

| Check (2026-10-13) | Result |
|---|---|
| New tests (conker-client) | `the_out_view_is_black_over_the_hud`: every quad but the black, skull and fade is under the black; the skull is over it, the fade over both; a TW strip at black + 1. `the_end_phase_hides_the_radar_kills_and_rank`: at k = 1 only the clock's 4 glyphs, no radar; at k = 2 no glyphs; health and weapon stay (7 atlas quads), and the TW strip. `the_stats_button_pulses_and_the_stick_steps`: glow(255) = 62, 107, 110, 155, 203, 206, 251 at v = 0, 15, 16, 31, 47, 48, 63; glow(0x80, v = 63) = 0x7E; period 32 frames; stick [0, 1, 2, 3, 4, 5, 4, 3, 2, 1]. With the pack at a = 0x80: RESTART at 255 under its lit at 0x7E, QUIT and STATS at 0x80; frame 3's uv is 96..128. `restart_fades_from_restart_in` (a constructed `MatchView`): `restart_in` 17..1 gives 0, 16 … 240, 255; phases 0-2 give none. `rows_colour_by_port_and_share_ties`: 4 humans are coloured 0-3, and the pack's colours are red, blue, green, yellow. Humans in slots 0-1, bots in 2-3, shots 0, 5, 0, 0: places [1, 4] ([1, 4, 1, 1] with 4 humans). Extended: `the_out_skull_grows_then_shrinks` (sides 4, 6 … 30, 32, 32). conker-extract `pack_hud_reads_back`: the stick is 192 × 32 |
| Screens (`../lab/screens/end/details.sh`, ports 5980/5982; 2d, 2 offscreen humans, 1 life, untimed, slot 0 warped over the pit at read 300) | Kill gate at read 312, the end fade from 363-364. `out-p1.png` (read 345, the out player): black and the skull only; before, the lives digit, score and rank showed on the black. `out-p2.png` (368, the other player in the fade): health and the lives digit, no radar, score or rank; its floor is 0.67× its brightness under the STATS (the fade at k ≈ 5). `pulse-p1.png` and `pulse-p2.png` (reads 400 and 412, one per client). `pulse-crops.png` crops RESTART and the stick from both: the letters' green mean is 76 vs 171, and the stick is in two frames |
| `bun run test` (`--no-fail-fast`, the other lanes' edits in the tree) | 390 pass, 1 ignored, 1 fails: conker-extract `pack_audio_reads_back` expects no `characters/59` audio, which the exporter lane's Heist work adds |
| `bun run check` | `cargo fmt --check` fails only on the other lanes' files (conker-rules `game.rs`, `traps.rs`, `tests/rom.rs`; conker-extract `lib.rs`, `audio.rs`, `tests/rom.rs`; conker-assets `lib.rs`). Its clippy step (all crates, `-D warnings`) and the web check pass |
| Pack (`bun run pack --force`, then `../lab/tools/verify-pack.sh`) | "pack equal", 1271 files (with the exporter lane's work in the tree) |
| `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass. Clippy (`--tests`) and `cargo fmt --check` are clean on this batch's files |

**Gaps.**
- **Outside this lane, fixed since (the top section):** phase 3 counts as over in `lib.rs` (the prediction, the HUD text) and `audio.rs` (the muffle, the music); `lights.rs`' `< 2` already did; `places`' doc no longer claims a slot rule. §5.7's rules tests (a blast on 2 enemies: +2 hits; a burn: +1 a tick; immune: 0; own: 0) and the headshot on every reaction-0x2E hit are the rules lane's.
- **V, a choice:** the original shows the STATS once more on RESTART's first frame (fade 0). Here phase 3 draws no STATS, so that frame shows the frozen level alone (the research's recipe).
- **I:** the stick's counter `0x80085CD0` and the glow's `0x800BE9AC` are both taken as 2·tick; their phase against the original's is not read. The strip over the black follows the split screen's text list (V t1d); in one view the original covers it too.
- **Checked, unchanged:** `end_fade` draws 16 on the first phase-1 tick this client sees, while the original is 0 on F and 16 on F + 1 (V b_kl2). The client sees phase 1 3-4 ticks late (E5's gap), so the +1 stays.
- No screen of RESTART's fade: the server's phase 3 was not in the tree when the screens were taken, so the constructed-`MatchView` test is the check. No screen of the clock going either: the scenario is untimed, so it has no clock.

## War: Beach's environment (2026-10-13)
**Batch E7. War: Beach has the original's night. The level is lit per vertex, as CBFD lights it: the ambient, the
lamps with their flicker, the red pulses, a strike's red light, and the sweeping ground lights (a white light crossing
the map about every 32 frames). The detonation draws three rings at the bunkers' charges and shakes every view with
the original's six-channel sine; a strike's blast and a rocket's shake the views near them. A strike lays a red glow
on the floor and spins a red streak. The escape tunnels' loops start on the first entry and never stop. The bots'
radio lines (their own, the id + 1, at 32700, with the clicks) are heard by everyone, centred. Song 0x4D's channels
13/14 play as a recorded stem by the distance from the north beach. 0x2D's gate lifts are predicted. No protocol
change.** Research: `../lab/research/mp-original/beach-environment.md` (items 1-7; 8, the sea, is the rules' batch).
The relight was checked on one oracle frame first (`beach-environment-scratch/relight/`).

| Part | What |
|---|---|
| conker-assets | `audio.rs`: `Background.follow` (`Follow`: a stem's volume by distance, `at`, `near`, `far`), `MapSounds.zones` (`Zone`: trigger, centre, horizontal, play), `On::PlungerReady`/`On::Plunger` carry `bot`. `lib.rs`: `effects_gltf(map)` |
| conker-extract | `audio.rs`: `zones()` reads the kind-2 sound triggers: id = word 0x1C >> 16, volume = (action & 0x7F)·0x147, near = word 0x1C & 0xFFFF, far = word 0x18. On 2b, 0x23 and 0x24 play 0x4A7 at 17331 and 16350 (451-1385, 601-1487, horizontal). The radio cues for bots are added (the line id + 1, 32700, delay 10). `loop_cut` is the backgrounds' loop cut (frames 1,001,910 + 1,878,088, an 11,010-frame crossfade, round half to even); `STEM_LAG` is 644. `lib.rs`: `PackMap.stem`, and 2b's `effects.gltf` (the ring, bank 09 #0x157). `hud.rs`: `strike_glow.png` (0xE20) and `strike_streak.png` (0xB97). `main.rs`: `--stem [<map>=]<wav>`. `package.json`'s `pack` adds `--stem 2b=../lab/audio/map2b/background_stem.wav`. README updated |
| extract-character | `parse_effect`: a bank-09 model whose header word 0 is its one display list (the ring 0x157 and the spark 0x132; `parse_attachment` reads word 0 as a part table and fails on them). Both share one resolver |
| conker-client `audio.rs` (Beach parts) | Zones start when a player is first inside one: a flat voice, its volume by the listener's (horizontal) distance, looped by the bank (0x4A7 loops), restarted if it ends. The stem is a second background voice, started and stopped with the first. Its gain is `follow`: clamp(32767·(far − d)/(far − near)) >> 8, capped at 100/100, over the 3D distance (full within ~3213 of (0, −377, 8227), silent at 4000). The radio: every line and its clicks are flat and heard by all, bots included (`radio`, `clicks_off`, `Src::radio`). `gone` is `pub(crate)` (the rockets' shake). Logs `STEM … gain` once a second |
| `beach.rs` | `RECORDS` are the explosion records. Rings: `ring(t, a, L)` gives an xz scale of 6.6944·√(t − 1) and alpha a·(1 − ((t − 1)/L)²), with y 2.609, gone after t = 12. Each ring has a random yaw, a = 200 + rand % 56, L = 12 + 4r, and its own material. The detonation keeps its debris, without puffs. `Wave` (`DETONATION`, case 4; `BLAST`, case 5) with `envelope` and `at` (phases from 0, wrapped; each channel stops at its next sign change after the decay), and `shake_share`. `Blasts.shakes` holds the detonation (record 0, 2007/9005), a strike's blast (the strike, 506/1013) and a rocket gone (800/2000). `camera()` adds x, y, z to the eye and pitch, yaw, roll (degrees) to its angles. `glow(k, b)` and `streak(k, b)`: the floor glow (a quad of ±50·s on the floor below y + 100, plus 5) and the streak (1500 × w, spun 0.285, 0.106 and −0.086 rad a frame) |
| `lights.rs` | `share` has no distance cut; `reach` keeps it for 0x2D's room. `Sweepers` (constants, `tick`, `ground`) are stepped in FixedUpdate, headless too, and log each spawn. `relight` (PostUpdate, 2b only): each unlit draw's vertex colour × min(1, ambient + Σ rgb·share) over up to 10 lights, the nearest to the point 1000 in front of the view. A mesh's colours are read once decoded (its own `AssetEvent::Added` reader) |
| `lib.rs` (prediction) | `Before` (a local rollback component) holds the body's position before its last step. `own_tick` tests every lift with `takes(old, pos, floor)` (the shape-3 filter is gone), sets `Before` after the ride and clears it on a step's respawn, as `Lifts::ride` and `reset` do. **`lift.rs` is unchanged** |
| Pack | Re-exported (`--force`). New: `audio/maps/2b/1191.wav` (0x4A7), `audio/maps/2b/background_stem.wav`, the bots' lines `audio/characters/156/1706.wav`, `1708.wav` and `157/1702.wav`, `1704.wav`, `maps/2b/effects.gltf`, `effects.bin` and `textures/3301-32x32-ia16-i.png`, `hud/strike_glow.png`, `hud/strike_streak.png`. Changed: `audio/events.json`, `audio/sounds.json`, `hud/hud.json`, `README.md`. `diff -r assets` against a fresh export into a temp folder is empty |

| Check (2026-10-13) | Result |
|---|---|
| Oracle relight frame (`ConkerRecomp.e7`, DLs 150-300; `beach-environment-scratch/relight/`) | Every terrain draw has G_LIGHTING (geometry mode 0x00220405, no normals). The pack's vertex colours are the drawn ones (1752 of 1754). FULL = 2048 is exact in world units (clip space × the coordinate modifier is a rotation): a sweeper's share 0.5964 against 0.5977 at 185, a lamp's 0.4916 at 456. A far vertex of colour 111 shades to (49, 62, 93), the remake's ambient-only (48, 61, 92). Up to 10 lights a view (not 7), with no distance cut. The flashes are kind 0x4 |
| New tests | conker-extract: `loop_cut_is_the_backgrounds` (lab-gated, byte for byte), and `tests/pack.rs` (the zones, the bots' radio ids and volumes, the ring: one draw of 336 indices, BLEND; the strike textures). conker-client: `the_stem_follows_and_zones_fade_by_distance`, `the_prediction_rides_a_gate` (a crossing rides with `Before`, not from None, as `Lifts::ride`), `the_rings_follow_the_original` (`explosion.csv`: the three rings' alpha within 1, scale within 0.001), `the_shakes_follow_the_original` (`shake.csv`: the amplitudes, and all 12 channels' stop frames exactly), `the_strike_marks_follow_the_original` (`strike.csv`), `the_beach_is_lit_as_the_original` (the frame's numbers), `sweepers_cross_the_beach` (the generator's ranges, about 1000 in 32,000 frames; light 0x801BF040 from read 212 to 280), `sweepers_light_the_ground` (the pack's collision against `sweep.csv`'s rays). Updated: `a_flag_taken_calls_on_the_radio` (a bot's lines) |
| Screens (`beach/shots.sh`, ports 5980/5984; `relit.png` added to `live`) | `blasts.png` (det + 8: a ring at the bunker, the night, the red pulses), `shake.png` (det + 4), `strike-light.png` (call + 20: the glows under three villagers, the streaks), `strike.png` (the blast), `relit.png` (the whole map at night). `relit-sweeper.png` and `relit-dark.png` (port 5986, three clients over the map; a sweeper at (2776, 180, 535), 159 from a vertex, share 0.81: `relit-sweeper-crop.png`, against the same corner without one). `gate-lift.png` (port 5982, 0x2D, one human walked through gate 1): the server's `t=335 slot 0 takes the lift to record 4` and the client's `t=335 predicted respawn on record 4: the camera cuts` on the same tick |
| Sounds (port 5988, `RUST_LOG=conker_client::audio=debug`, 2 bots, the human on the north beach) | Bot 0 (a villager) on the plunger: 0x5B0 at 25000, its line 0x6A8 (157's 0x6A7 + 1) at 32700 ten ticks later, 0x5B1, all centred and full for the far human. The human into tunnel 0xD: `id=1191 p=Zone(0) vol=17331` on entry. `STEM gain=1.00` on the north beach, 0.00 once away |
| `bun run test` | pass: 382 tests, 1 ignored |
| `bun run check` | pass: fmt, clippy on every crate, the web check. `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` passes too |

**Gaps.**
- **V:** the pack drops the vertex flag's bit 15 (unlit). 97 of 2b's terrain vertices keep their colour in the original (83 black, 2 grey, and the two bulbs' 12 white); here the bulbs dim with their lamp's flicker while it is dark.
- **V:** the bots' click-off is 25000 in the original; the pack's cue says 30000 (left as is). The stem's lag alternates 552/736 samples; 644 is the midpoint (±92, ±4 ms).
- **C:** the radio is `func_15103C14` on every War map, so Total War's and Colors' bots are now heard by everyone too.
- **I:** the streak's geometry (`func_15140410`) is not decoded: a quad centred on the target, turned by the three angles. The glow and streak blend as Bevy's alpha blend (render modes 0x03 and 0x1A not read). The remake's blast is call + 40 (the oracle's + 41), so the glow shrinks a frame earlier.
- **I:** not ported: the flak (the map's emitter `func_150E9E34`: white lights of size 83-94, V one in DL 250), the far fire and smoke, the strike's 11 sparks (model 0x132; `parse_effect` reads it), the generic blast's light.
- **I:** the rockets' shake is War: Beach's only (the original's rocket code serves every map). A light's share uses this frame's size (the original's is a frame late). The shake's angles go to Bevy's YXZ (the signs unchecked on a frame).
- **I:** sweepers and lamps use each client's own random numbers; sweepers move once a tick (not interpolated). On 2b's sparse terrain (1779 vertices) a sweeper lights a vertex only within ~450, as in the original.
- **I:** a mesh two nodes share (03-5b, placements 6 and 7) is relit at its first node (`ponytail` note). Zone voices are not reset on a match restart (as the emitters).
- **Lane:** `extract-character` (`held.rs`, `lib.rs`) is the pack's parser, outside both lanes' lists.

## Mounted guns: the leftovers (2026-10-13)
**Batch E3. The jump onto a gun keeps the speed the player came with and homes on the seat. The leap off follows the
multiplayer rule: facing 0xB000 at z > 0, else 0x3000, the jump 0x1200 left of it. The press's own frame moves (15
along, the gun's push), and the air brakes 0.875 a frame from the next. A human gunner's bullets head from the muzzle
for what the scope's ray meets: e1's first shot knocks player 2 at 210.2° (V 210.4°, was ~220°). A hurt gunner's
trigger takes Z and B. A capture's respawn wins over `Effect::Gun`, as in the original. The fire process now lives on
the gunner's mount, so its client predicts the shots: the flash, the kick, the fire's sounds and `Fighter.shots`. The
release's sound plays again 15 ticks on. The HUD blends its alphas as the N64 does, which removes the flash's pale
box. Protocol 22.** Oracle runs: `../lab/research/mp-original/mounted-gun-scratch/e3/` (l1, l3, h1-h3, k1-k3).

| Part | What |
|---|---|
| conker-sim `movement.rs` | `hop(p, to, world)`: the speed brakes 2.5 a frame along the move angle, then x and z go 0.2 of the way to `to`. `leap(p, angle, world, side, others)`: y-vel 30, speed 30, gravity 4, lock 0xFF, facing = angle − 0x1200. The press's frame moves 15 along `angle`, integrates y once (26 left), pushes out of `others` and sweeps the walls. `air()`: a lock also brakes a jump with the stick released (`func_1504CB98`: obj+0x89 zeroes the drive, C) |
| conker-rules `gun.rs` | `Mount.to`: the seat's x and z at the touch. The fire process (`firing`, `next`, `barrel`, `rng`) and `Mount::fire()` moved from `Gun`, which keeps yaw, pitch, the wait, `shots` and `last` for the clients. `sight()`: the scope's ray crossed with the muzzle's plane, through the barrel joint's (±50, 0, 200) and (±13, 50, 220) (tables 0x800A1E04 and 0x800A1E1C, read in k3). `SIGHT` (333) is gone |
| `game.rs` | The touch keeps `xz_speed`, sets `Mount.to` to the seat, continues the gun's barrel and seeds the mount's draws from the match's. A: the MP facing rule (`func_1505F298`: obj+0x1C > 0, C). `fire_gun(i)`: `Mount::fire`, then a human's bullet goes from the muzzle toward the first body part or wall the ray from `sight` meets (`first_part`, `raycast`: the twin guns' aim cast, `func_151C4B0C`), or from that point along the ray if none. A bot's goes along the barrel; `Fighter.shots` counts. `mounted_guns` runs it for each seated gunner after the players, and `step_player` after its player. `hurt_frame` steps the trigger with Z and B. `Effect::Gun`: comment only (below) |
| conker-client | `hud.rs`: the local gunner's flash and kick come from the predicted `Fighting` (shot count, the mount's barrel), not `GunView`. `display_alpha(l, a)` = l·lin(a) + (1 − l)·(1 − lin(1 − a)), l the colour's luminance. It applies to every HUD quad's alpha and, once loaded, to every HUD texel (`display_space`). `audio.rs`: `Sounds.replays` replays the release 15 ticks on, same id, at the release's spot; the local gunner's gun state and trigger come from the predicted mount |
| conker-sim `net.rs` | `PROTOCOL_ID` 21 → **22** (`Mount` gains `to`, `firing`, `next`, `barrel`, `rng`) |

| Check (2026-10-13) | Result |
|---|---|
| Oracle | l1 (gun A turned 90°, z < 0) and l3 (0x34, z > 0): the leap's facing 0x3000 / 0xB000, the move 0x1200 left, whatever the yaw. h1-h3 (0x34, P2's twin guns hit the seated P1 at 116): Z held fires on through state 29, budget −2 a frame. Let go at 118, the fire runs its 20 frames. First pressed at 118, it fires at 119. k1/k2 (lldb, e1): the first shot's direction (−0.845, −0.201, 0.495) meets P2's body 146 along the ray; later ones go to (2102.5, 675.39, 862.5) on the floor, 334 along it from the eye, which the fire clip lifts 5 units. bots1 (Colors) 3072/3073: the seated gunner is at its base on the recreate frame |
| New tests | conker-sim `hop_keeps_the_speed_and_homes` (b30 4896-4910: speed and x, z to 1e-3 toward the undrawn gun's origin; t1 132-155 toward the seat, the facing turning 3072 a frame). conker-bots `mounted_guns_m2_and_e1` extended. m2's leap 1601-1615: facing 0xB000, speed 30 → 18.625, positions and the landing (175.39, 543.95, 6186.73) to 0.05. l1's on gun A turned: facing 0x3000, the landing (−401.42, −388, −3808.90). e1: 13 shots (V 13), the knock 0x9577 (V 0x9595), later shots within 0.011 of V's two directions. From V's eye the floor point is V's to 0.1. conker-rules `a_hurt_gunner_fires_and_a_capture_respawns_it` and `the_gunners_client_predicts_its_shots` (a client match per tick, guns at home: the same Shot events and `Fighter` as the server's, 60 ticks). conker-client `a_gun_release_plays_twice` and `hud_alphas_blend_as_the_console` (a black fade within 8/255 of the console's over grey 0.1-0.9, 28/255 unweighted; white over black exact) |
| Screens (`guns.sh flash N`, N = 160, 163, 167, 170; ports 5970-5974) | `lab/screens/guns/flash-display-b-164.png` (= `flash-b-163`: slot 1's sight at read 164, barrel 1's flash on the left): the glow fades into the ground with no hard edge. Before: `flash-a-167.png` (kept from the old build) and `own-flash.png`, the pale box with straight sides. The shots now fall on other reads (the mount's own draws), so 167 shows no flash; 163 (slot 0) and 170 show the bullet only. The gunner clients roll back only around their mounts (not predicted), then none from tick 480 to the end: the fire at 554-882, the overheat, the stop and the leap at 954 |
| `smoke.sh`, map 30 (port 5972) | pass: three clients, 0 rollbacks, prediction error 0 |
| `bots.sh`, 5 bots at einstein, 150 s | Map 34 (port 5970): every bot moves; the bot that took the gun at t=2494 fired from it to the end (overheat cycles), 2 hits, and so never died (FAIL "dies 0", the gun campers of earlier runs). The others fight, die and respawn. Map 30 (port 5976): pass (4-37 hits, 4-7 deaths each, 27 lift rides); gun A taken twice, its first gunner killed on it at t=1715. Both: no fault, no panic |
| `bun run test` | pass (382 tests, 1 ignored) |
| `bun run check` | pass. `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` passes |

**Gaps.**
- **I:** the fire clip's recoil is not modelled: the eye and muzzle stay at rest, so later shots are within 0.011 of V's
  directions, not exact.
- **I:** the jump homes on the seat; the original homes on joint 3 as last drawn (b30 and m2 homed on the origin: the
  oracle had not drawn those guns yet).
- **I, unobserved:** A while hurt on the seat (only Z and B were run). The barrels' turn across two mounts. A gunner
  left on a recreated gun (it falls from the seat, as before).
- **I:** `display_alpha` is exact for black overlays and for white over black only. A bright overlay over a bright
  scene comes out up to 25/255 fainter than the console's; the toe of sRGB adds up to 4/255 near black.
- **I:** the release's replay sits at the release's spot; the original copies the voice's level and pan (no position).
- **Latency:** the touch is still not predicted (the client's guns are at home), so the mount costs a rollback.
  e1's knock-back slides 167.7 (V 140.7); that is the knock's physics, outside this batch.
- **Lane:** `audio.rs` also has the Beach lane's uncommitted edits; this batch touched `guns()`, `Sounds.replays` and
  the gun state handed to it in `hear()`.

## Total War and Colors leftovers: the rules (2026-10-13)
**Batch E5, the rules half. In a lives-mode Total War match, a human out of lives takes over its team-mate bot 13
frames after its out frame, with no input. It gets the bot's body where it stands (momentum, health, weapon, the bot's
lives) and 90 immune frames; the bot's slot gets the corpse and is out. The camera snaps behind the new body. The out
view is black with the skull growing to 32×32, and after the swap the skull shrinks 1 px a frame, then the view cuts
in with no fade. The canister carrier cannot jump, Z does nothing, and its camera reads weapon 0x25. 0x2D's doors and
covers slide (doors 10 up / 2 down a frame, covers 40 / 8) and hide past 500 above home. They are solid exactly while
shown, at their height, on the server and in the client's prediction; the pack now has the doors' collision. The mask
put-on replays e2. Colors' flag is a body: it pushes players out to the radii's sum and is taken on the overlap. The
final capture's end fade comes at +39 and the outro at +55. Protocol 21.** Research:
`../lab/research/mp-original/war-leftovers.md` (items 1, 3, 4, 5, 8, 9).

| Part | What |
|---|---|
| conker-rules `game.rs`, the takeover | `Fighter.out_at` (the out frame) and `extra` (lives brought by a takeover); `lives_left(i)` = lives + extra − deaths, 0 once out (the taken-over bot keeps its deaths; V t1 438: lives 0), used by `down`; the server's HUD lives add `extra` too. `Effect::TakeOver(i, j)` applies from `out_at + TAKEOVER_DELAY` (13) if i is out and j is a living, in-play bot of i's team; otherwise it is ignored (the script resends it each frame). `in_body` swaps the body and play state; each slot keeps its identity and stats (team, side, kills, deaths, suicides, shots, hits, headshots, bot, start, class…). i: out 0, `TAKEOVER_IMMUNE` 90, `extra` so that its lives are the bot's. j: the corpse, out on i's place from that frame. `Event::TakeOver { player, from }` |
| `game.rs`, the rest | Carrier: A and Z are masked out while `carrying` (as for the flag carrier), and `camera_weapon()` is `gas::CANISTER` (0x25: the armed reach). Mask: after the put-on's first frame (a full move) the stick and buttons are dropped and the state is held (`timer = 2`), so the idle no longer cuts the turn: the speed brakes 2.5 and the facing turns ≤ 3072 a frame to the frozen move angle. e2 matches `game.rs`'s 3 → 4 → 1.5 → 0, not STATUS's old 3 → 0.5 → 0. Flags: every shown, uncarried flag is an `Other` in the players' `push_apart`. End latch: a script's score (a capture, an escape) latches `ended_at` on its own frame, anything the step brings on the next, and `END_DELAY` is 39 (it was 40), so a final capture fades at +39 and is Over at +55 (it was +41 / +57); Deathmatch's clock is unchanged |
| `gas.rs` | `Prop.lift` replaces `since` and `hide_after`. `Gas::step` (`func_1511EC50`) runs only while shown: v = 2 for the doors and 8 for the covers. With the flag set, lift += 5v, and past `RISE` (500) it clamps and hides; with the flag clear, lift −= v, not below 0. `Gas::lifts()`. `gas::seat(world, total_war, shown, lifts)` shifts placements 0-7's triangles to their lift while shown and 20 000 up while hidden (ponytail note), with the Deathmatch set (0xFC, no lift) outside Total War |
| `colors.rs` | `TOUCH` is gone. `body(pos)` is the flag as a body (r 30, y-scale 0x3EE8BA2F: the squirrel's). `touches(flag, player, side)` is that body overlapping the player's, both on the floor (flag not falling, player not airborne); it is measured at the push's height (y − 4.5 after the ground step) so the push's rest at 59.97 is no grab |
| conker-bots | `Body.out` (frames since the out frame, from `Fighter::out_at`). (5, 0x1D) leaves out the out players. `Body.lives`: 0xA5/3 is the stats record's +8, the lives (`func_150859AC(j, 3)` also hands the human the bot's lives in the swap, C; V t1 438): `lives_left` in an untimed Total War or Deathmatch, else the 6 that a timed match, Colors and Beach set and never take (`total-war-teams.md`; V s4, s5, s7: 6 everywhere). (0xF, 0xF) checks the match's own gate (`takes_over`), moves G+0x18's bit i → j and copies j's BAND, HEALTH, HIT_BY, HELD and SEES rows to i (with its pickup's holder), then pushes `Effect::TakeOver`. `observe` moves the bot memory's `held` and `hit_by` and marks both bodies placed |
| conker-server | Seats the props on 0x2D after the script's effects and publishes `MatchView::lifts`. On a TakeOver it resets both slots' lift memory, logs "TW t=… slot i takes over bot j's body", snaps the slot's camera (`Follow::spawn` behind the body) and counts it as a respawn (the client cuts on `Combat.lives`) |
| conker-client (`lib.rs`, `hud.rs`; plus `render.rs`, see the gaps) | `seat_props` seats the beach plunger and, on scene 0x2D, the doors and covers from `MatchView` (`props`, `lifts`) for the prediction. `hud.rs`: `Shown.skull` (half-size) replaces `out`: the view is filled black under the HUD with the skull centred on end.json's rect. `skull_byte` grows the byte 4 a frame to 60 while out and shrinks it 4 a frame after the takeover; half-size = (byte + 4)/4. The radar hides while the skull shows. `render.rs` draws the doors and covers at home + lift |
| conker-sim `net.rs` | `MatchView.lifts: [i16; 8]`; `PROTOCOL_ID` 20 → **21** (the takeover's `out_at`/`extra`, the lifts) |
| conker-extract | `solid_placements(scene)`: on 0x2D the doors 0 and 1 plus the Deathmatch's solid set, used by `pack_data` and `tests/pack.rs`; the pack README's collision line names the doors |
| Pack | Re-exported (`--force`): `maps/2d/collision.json` gains placements 0 and 1. `diff -r assets` against a fresh `conker-extract pack` into a temp folder is empty |

| Check (2026-10-13) | Result |
|---|---|
| New tests | conker-rules: `an_out_human_takes_over_a_bot` (refused before out + 13 and into an enemy; then the bot's body, health 4, lives 2, immune 90, speed 12 → 9.5 the next frame; the bot is out on the human's place with the corpse; stats kept per slot). `the_mask_put_on_replays_e2` (speeds 1, 2, 3, 4, 1.5, 0; move angles 23296 … 42496, then frozen; facings 19968 … 41472, 42496; still from the 6th frame). `the_flag_touch_is_the_bodies_overlap` (e0 404: 7.1 pushed off the falling flag; rest at 60 for a squirrel and 53 for a Tedi, no grab; at 60.0 no, at 59 yes, not while the flag falls or the player is in the air). `gas.rs` `doors_and_covers_slide_and_hide` (d2: door −1293 at the 1st move, −803 at the 50th, hidden at the 51st; a cover hidden at the 13th; back home 250 and 63 frames after a show). `tests/rom.rs` `a_cover_is_solid_while_shown` (a run into cover 2 stops while it is shown at home, passes while hidden or lifted 500, and outside Total War the cover stops it). conker-bots `rom.rs`: `total_war_takeover` (t1's roster with the real script: one TakeOver at out + 13, human 0 into bot 2, asked = applied, the bot out 4th) and `colors_flags_and_captures` (the final capture: no capture fade nor respawn, cheer +30, Fading +39, Over +55; V c1b 412/422/438). conker-client: `the_out_skull_grows_then_shrinks`; the skull's rect at half-size 16 is [130, 92, 32, 32] and at 2 it is [144, 106, 4, 4] |
| Updated tests | The end-latch values (Deathmatch's clock and kills latch a frame later with the same fade frame), the gas test's carrier (A keeps y, `camera_weapon` 0x25 and back) and mask parts |
| Takeover run (`total-war/e5-run.sh`, port 5970, `--map 2d --mode total-war`, 3 bots, 1 life, untimed, one offscreen human warped over the pit at read 300) | `t=483 slot 3 falls through a kill gate`, `TW t=504 slot 3 takes over bot 0's body` (out + 13). Screens: `total-war/takeover-black.png` (read 328: black, the skull growing), `takeover-shrink.png` (344: the skull shrunk to a few px), `takeover-cut.png` (362: behind the new body, the lives counter at 1, no fade) |
| Gas run (port 5972, `--mode total-war`, 4 offscreen clients) | `GAS t=1110 Carry(0, true)`, delivered at 1308, the gas at 1579-1697. `total-war/e5-carrying.png` (the carrier with the canister), `e5-cover-rising.png` (read 1005), `e5-door-sliding.png` (1100), `e5-door-shut.png` (1290) |
| Flag run (port 5974, `--map 30 --mode colors --score 2`, the `flags.sh` spawns) | `COLORS t=533 Flag(0, true)` and `flag 0 taken by slot 0` on the same frame, the capture `Score(0, 1)` at 611. `colors/flag-taken.png` (read 380, the carrier's own view), `flag-taken-side.png` (384, a free view) |
| `bots.sh`, 100 s, `BOTS=3 HUMANS=1` (ports 5976/5978; a copy with offscreen clients while the headless client still panicked in the outros lane, since fixed there) | Total War: every bot moves and is seen moving; bot 0 fights, dies and respawns, bots 1 and 2 never die (bot 2 never hits: the snipers and campers of earlier War runs); 4 lift rides. Colors: every bot moves; bot 2 dies and respawns, bots 0 and 1 fight (11 and 4 hits) and never die; 1 flag take, no capture. Both: no fault, unknown native or panic |
| `bun run test` | pass (368 tests, 1 ignored); conker-bots re-run after the last edit: pass |
| `bun run check` | pass (fmt, clippy on every crate, the web check). `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` passes |

**Gaps.**
- **I:** the remake's lives drop at the death, the original's +8 at the respawn moment, so between a team-mate bot's death and its respawn the script's health + 6·lives ranks it one life lower. Such a dead bot still ranks above 0 in the original and might be taken over (not observed); the remake's gate, after the research's recipe, takes only a living one.
- **I, latency:** the props' slide is stepped in `Match::step` after the moves, so their collision is a frame behind the drawn height (the ponytail note in the server). A hidden prop's triangles sit 20 000 up rather than out of the world.
- **I:** the client's prediction does not push off the flags (they are `Other`s only on the server); the server's correction does.
- **I, latency:** the skull counts from the tick this client sees the out (3-4 ticks late, as the other HUD timers). **I:** the camera snap is the follow camera's spawn rest, not `func_15128774`'s exact eye (V t1 438: 247 back, 100 up).
- **Lane:** `render.rs` (`show_pickups`: the doors and covers drawn at home + lift) is outside this batch's client lane; it was the only place that draws them.

## The outros' leftovers (2026-10-13)
**Batch E6. The six War outros now play most of what the original scripts do. Heads turn on the channels, and
jaws talk. Clips cross-fade. Attachments sit in hands (the Tediz' guns, Colors cs1's pencil and drawing), masks hide
the villagers' parts, and Beach's and Colors' props appear. Colors cs5's tanks and march sound where they are, with
their fades, and Beach cs4 plays its song 0x8A from a new oracle recording. The music plays on through the outro and
the STATS, ducked by the voice with the original's factor law. The bubble grows from its tail, its text fades in and
out, page by page, and it shrinks away. The match's end fade lingers over the first frames, each line has its own
height, and the STATS fade in.** Research: `../lab/research/mp-original/outros-leftovers.md` (items 1-7 in its
order; 8 left for the Tank mode).

| Part | What |
|---|---|
| conker-assets `outro.rs` | `Actor.head` (op 0x11: `joint`, ch1 `pitch`, ch2 `yaw` keys from `t`), `talk` (op 5 b=0x17: `jaw`, `skew`, `says` {t, SHUT/CHATTER/OPEN, degrees}), `mask` (op 5 b=0x3C), `attach` (op 0x0D: bank-09 `entry` on `joint`, on/off), `Play.blend` (e − 1, default 5), `Outro.props` (kind-3 objects: placement, path point, ch8/ch7), `sounds` (ops 6 and 0x0A), `song` (op 8), `near` (op 5 b=0x25). `Outro::bubble(k)` replaces `page`: open scale min(n/3, 1) about the tail's tip and text alpha ⌊25.5·min(n − 2, 10)⌋; a page turn fades the old page out over 10 frames, then the new one in; the close fades out over 10 and shrinks 0.25 a frame. `Actor::blend(k)` gives the old clip frozen at the switch and its weight 1 − 2·⌊500/blend⌋·(k − at + 1)/1023. `frame` is public, `tip(quads)` new |
| conker-extract `outro.rs` | The decoder drops records with t < 0 (Colors cs1's officer's op 0x11 at −1, cs5's 0x4A9 at −3/−1), no longer clamping them to 0. It reads the new fields. Heads go by model {90: 3, 88/135: 4, 156/157: 12}. Talk's skew is off for 135. Attachments come from the action table at 0x80086CC4. Props are found by the placement uid byte 0x33. `models()` writes `maps/<map>/outro.gltf` (scenes `09-<entry>` and `prop-<placement>`), and `songs()` packs a song with its map's recording |
| conker-extract, shared files (minimal) | `lib.rs`: `PackMap.song`; the `outro.gltf` per map; the outro sounds' ids to `pack_audio`; the songs' files. `main.rs`: `--song [<map>=]<wav>` (as `--background`). `audio.rs`: `pack_audio` takes the outro sound ids (map folder). `tests/pack.rs`: `song: None` on two `PackMap`s, plus the asserts below. `package.json`'s `pack`: `--song 2b=../lab/audio/map2b/song_8a.wav` |
| conker-client `outro.rs` | `show` plays the current and the blended clip as two weighted nodes, shows the props and sets the near plane (30 in Beach cs3). The new `dress` (only with an asset server: a headless client has none) loads `outro.gltf` once the actors are in, spawns the props and hangs each attachment hidden under its joint. It hides masked display lists (`extras.list`) and switches attachments by their last record ≤ t. The new `pose` (PostUpdate, after the animation, before propagation) sets the head joint to `turn(−pitch, yaw)` (= Ry·Rx) from the turn's t. It also sets the jaw to openness/255·degrees about x plus the skew's 1.41° steps about y, with SHUT −60, OPEN +60 and CHATTER ±(rand % 20 + 40) a frame (`jaw`, a seeded `Rng`). Villagers' heads are left to their clip |
| `audio.rs` | `DUCK` is the factor law: f in [14000, 32768], −1024 / +4096 a frame, music = recording · f/32768 (Beach: 18000 → 7690, back +2250 a frame). Any MP3 ducks (the outro voices too). At the end phase the music stops only outside War (`team_mode`). `outro_sounds`: script loops at their actor with linear fade-in/out, stopped at `stop`; one-shots on their frame; pitch d·50 cents; volume 0x7F7F, near 31000, far 32000. The outro camera is the ear while an outro runs. `SONG − n`: the outro's song at its frame, flat, as recorded, outside the duck |
| `hud.rs` (the parallel lane's file; minimal) | `outro(hud, o, k)` takes the frame. The bubble is scaled about `tip`, the text gets the bubble's alpha, and each line is placed by its own height (`line_height`: the tallest glyph's offset + size, a space 12). The match's end fade is held as 255 − 16·(k + 2) over the outro's first frames (`FADE_Z`). The STATS alpha is `stats_alpha` = min(255, 8k) from the results' first tick. Tests `outro_bubble_lines_and_fades` (new) and `outro_subtitles_match_the_originals_draws` (no more +1 frame) |
| lab | `audio/tools/song_record.sh` (Beach idle under lldb, the match's 0x4D swapped for the song) → `audio/map2b/song_8a.wav` (15.02 s, trimmed at the first and last non-zero samples; a re-run is sample-exact). The outro shot scripts take `PORT` and `RUST_LOG` |
| Pack | Re-exported. `diff -r assets` against a fresh `conker-extract pack` (same flags) into a temp folder is empty. New: `maps/{2d,30,2b}/outro.gltf` + `outro.bin`, `audio/maps/30/{1192,1585,1587,1758}.wav` (0x4A8, 0x631, 0x633, 0x6DE), `audio/maps/2b/song-8a.wav`. Changed: the three `outros.json`, `audio/sounds.json`, `README.md` |

| Check (2026-10-13) | Result |
|---|---|
| New tests | conker-assets: `bubbles_grow_fade_and_shrink_as_the_originals` (every even DL read of `bubble_tw4.txt` and `bubble_cs1.txt`: bubble width and glyph alpha; TW read 840 skipped, an lldb outlier). `clips_switch_at_their_frames` covers the blend. The sample outro round-trips the new fields. conker-extract `rom.rs` `outro_commands_follow_the_research`: heads (cs5 at 198/196/198/198, 300/303, 697), talk (TW cs4 jaw 6 with skew, 14 says, first (36, CHATTER, 45)), masks, attachments, blends [9, 5], the three props, cs5's six sounds, near 30, the `outro.gltf` scene names. `pack.rs` checks the four sound WAVs in 30's folder, every `outro.gltf`, and `song-8a.wav` with 2b's recording (only Beach cs4 has a song). conker-client: `outro_heads_match_the_oracle`: TW cs4 at t 130/348/588/748 within 0.5° of `heads_tw4.csv` (36.0/0.8 is the sign check), and cs5's three soldiers at t 328 against the research's 67.6/83.0/22.4. `talk_moves_the_jaw`: shut before frame 18, ≤ 45°, ≤ 10.6° a frame, shut 5 frames after t 153. `the_music_ducks_by_the_factor_law`: 18000 → 7690 in 19 frames, back in 5. `the_music_plays_through_the_outro_and_script_sounds_fade`: War keeps the music at phase 2, Deathmatch stops it; 0x4A8 is silent at t 0, full at its fade-in's end and half way down half way through its fade-out; the one-shot plays on frame 25 only |
| Oracle | **0x8A**: `song_record.sh 8a 60`. It plays 15.02 s from its first sample and then stays silent: no loop. In Beach cs4's capture (`b4` audio) it is found at 10.68 s with gain 0.62/0.76/0.74 and corr 0.57-0.65 over its first 3 s (the music and the voice play over it). After that the match drifts (lldb frames). **0x4A8's loop**: the bank's loop shows in `cs5b`'s capture, repeating every 1.40 s from 17.6 s to 27.4 s (corr 0.77-0.82) |
| Sounds (`RUST_LOG=info,conker_client::audio=debug`) | Colors cs5 (`colors/outro.sh`): 0x4A8 twice (the two tanks) at the outro's start (volume 0, fading in), 0x6DE at frame 1, again at read 5934 (t 908, +150 cents), 0x633 at 5994, 0x631 at 6023. Beach cs4 (`beach/shots.sh end`): the song (id 0xFDFF) at read 510 = outro frame 1, the voice 0x163 at 609 (frame 100: t 200) |
| Screens (read) | `total-war/outro-tediz-{early,bubble,stats}.png`: the Tedi holds 0x57 on joint 14 under the bubble. `total-war/outro-squirrels-bubble.png`: the officer's head tilted by its turn, his jaw open. `colors/outro-squirrels-bubble.png`: the officer talks with his head unturned (cs1's turn is at t −1). `colors/outro-squirrels-stats.png`: one 88 holds the drawing 0x64 (joint 23), the other still the pencil 0x92. `colors/outro-tediz-{early,mid}.png`: the Tediz run with their guns. `beach/outro-cs4.png`: the villagers' masked looks. `beach/outro-cs3.png`: the Tediz march with their guns. The props, by one-off free-camera clients during the outros: `beach/outro-cs4-prop.png` (0xFC, the "PARIS 200 km" painting; read 750, `--view 1120,30,-350,1076,-60,257`), `beach/outro-cs3-prop.png` (0xFD, the barbed-wire fence; read 950, `CONKER_OUTRO=3`, `--view 1229,-480,-1440,1929,-500,-1468`), `colors/outro-tediz-prop.png` (0xFF, the Tediz' banner frame; over + 200, `CONKER_OUTRO=5`, `--view -622,-200,3506,-52,-250,3692`). No "no prop/attachment" warning in any client log. TW cs4's turn shows only in the test: its bubble shot (t 280) falls between the yaw keys |
| `bun run test` | pass (368 tests, 1 ignored) |
| Headless | `PORT=5980 MAP=2d MODE=total-war BOTS=3 HUMANS=1 crates/conker-server/bots.sh 100`: "no panics" ok (`dress` had panicked headless, found by the rules lane). Its two "fights" FAILs are the bots' |
| `bun run check` | pass. `cargo clippy --tests -p conker-client -p conker-assets -p conker-extract -- -D warnings` and `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` pass |

**Gaps.**
- **Not done, for the Tank mode:** cs5's smoke (op 5 b=0x0A), the tanks' own engine (model 40's class sounds 0x4A3-0x4A9) and op 9's texture swap on one tank.
- **I, left:** the villagers' head turns (156/157, joint 12). The oracle (`heads_b3/b4.csv`) shows the channels composing with the clip's pose or on other axes (`arg12` = 1), and the research does not settle which. Their heads keep the clip's motion.
- **I:** the chatter's speeds and skews come from the remake's `Rng`, seeded per actor; the original's `rand` sequence is not reproduced. The rates and bounds are the original's.
- **I:** the bubble grows about the tail quad's bottom corner (`tip`). The DLs fit the widths and alphas, not the pivot.
- **I:** the STATS fade-in (min(255, 8k)) applies to every results screen, Deathmatch's too; it is seen (V) only after the War outros.
- **I:** 0x8A was recorded on the match song's slot at its 0x4650, where Beach cs4 plays it on slot 1 at 0x4650. In `b4` its first 3 s match at gain 0.62-0.76, not 1. That run's lldb frames (2-4 VIs) blur the comparison; the level is not settled.
- **V, a choice:** the song plays on into the STATS (15 s against Beach cs4's 325 frames): nothing in the original stops it.
- **Found, fixed 2026-10-13:** `Sounds.background` was never reset, so after a restart a Deathmatch played no music; War's kept playing from the last match. RESTART's fade now stops it in every mode.

## Total War and Colors leftovers: the client (2026-10-13)
**Batch E5, the client half. The lamps of 0x2D's shelter and 0x2B flicker in random bursts, each flash with its
buzz. A taken pickup shrinks away over 3 frames, the canister too. In War: Colors, 30 frames after a capture the
scorers' jingle plays for everyone and a voice plays at each loser. The flag and canister lines come over the radio
with its clicks. Fades (and the end phase's new fade) cover the whole view, the HUD and the iris included. Any
player's gas-mask put-on muffles the effects through a 520 Hz low-pass. The filter is the mixer's own DSP, so it
builds for wasm too; it has not been run in a browser.** Research:
`../lab/research/mp-original/war-leftovers.md` (items 3, 5, 6, 7, 9).

| Part | What |
|---|---|
| conker-client `lights.rs` | `Flicker` ports `func_151607A4`/`func_15160954`: steady, a burst of 1-5 flashes starts with chance 0.005 a frame; a flash sets the size to 0, step full/(r·12+8), wait r·28+1 counted down while ≥ 0, then grows back to full. `Flickers` (seeded by the client id) steps in FixedUpdate, not in rollback, while the match is not over. It drives `shine` (the lamp's size) and `tint_room`: on 0x2D (the map with a countdown light) the shelter (trigger 0) is shaded by lit(now)/lit(full) while a lamp flickers |
| `audio.rs` | A map emitter plays when the lamp at its `at` starts a flash (one of 0x44B-0x454 a flash). `radio()` is shared by the canister, the flag and Beach's plunger lines: busy for 300 frames per slot (canister or flag), a human's click 0x5B0 at once, the line 10 frames later, 0x5B1 once it has played; a bot gets its odd-id line only. `colors()`: a score rise queues the cheer 30 ticks on; then `line()` plays the scorers' jingle and `On::Lose` plays at each loser that is not voicing. `LINES` (escape, the two jingles) duck the music, one at a time; the outro's voice stops a jingle. `mask()`: a put-on muffles, a take-off by a player whose last mask event was a put-on unmuffles, Over resets. `Playing` runs each effect through a `Biquad` (RBJ low-pass, Q 1/√2, 520 Hz), heard while `MUFFLED`. Fix: `Playing` looped every sound that was not in the bank (lines, outro voices, so they never ended); now only the background loops |
| `render.rs` | `show_pickups`: `shrink(k)` scales a taken pickup 0.74, 0.48, 0.22, then hides it, from the tick the client sees the take; canisters 27/28 too, while the doors and covers just hide; a node hidden when first seen does not shrink |
| `hud.rs` | Fades at z 9.5 (`FADE_Z`), over everything, the iris included (it was 8.5, under the iris). `end_fade(k)` = min(255, 16k) in phase 1, from the tick it is seen, in every mode; the log line "end fade from tick". |
| conker-assets | `Light.flicker`; `Emitter { at, play }` (`start_chance` gone); `On::Lose { team }` |
| conker-extract | `lights.json`: the four lamps (2 on 0x2D, 2 on 0x2B) get `flicker`. `events.json`: `carry` and `flag` become radio cues (variants id + 2k, a bot's id + 1 at 32700, a human's 30000, delay 10); the radio clicks move to the common cues; new `lose` cues (0x657-0x659, 0x662-0x664, 24000, 800-1000); map 30's lines `jingle-squirrels` and `jingle-tediz` (MP3 0x1A4/0x1A7 as WAV). The pack README follows |
| Pack | Re-exported: `diff -r assets` against a fresh `conker-extract pack` into a temp folder is empty. New files: `audio/common/` 1445-1446, 1604-1615, 1623-1625, 1634-1636 (0x5A5-0x5A6, 0x644-0x64F, 0x657-0x659, 0x662-0x664) and `audio/maps/30/jingle-1a4.wav`, `jingle-1a7.wav`. Changed: `events.json`, `sounds.json`, `maps/2b` and `maps/2d/lights.json`, `README.md` |

| Check (2026-10-13) | Result |
|---|---|
| New tests | `lamps_flicker_in_bursts` (200,000 frames, seed 7): 1-5 flashes a burst, all of 1-5 seen, start rate 0.0045-0.0055, dark runs 3-30 frames, ramps 7-20, bursts end at full, one buzz a flash. `lines_end_and_the_muffle_cuts_at_520_hz`: a line ends and the background loops; the filter gives 100 Hz > 0.95, 520 Hz 0.707 ± 0.02, 5 kHz < 0.02. `the_mask_muffles_from_any_put_on`. `a_flag_taken_calls_on_the_radio` (pack): 0x5B0 at once, 0x5A3/0x5A5 at +10, 0x5B1 at +11 headless, nothing again from that slot within 300, a bot's 0x5A4/0x5A6 unclicked. `a_capture_cheers_30_frames_on` (pack): the jingle and a Tediz voice (0x662-0x664) at each of the two losers at +30. `a_taken_pickup_shrinks_over_3_frames`. `colors_matches_the_originals_draws`: the fade is the top quad over the iris; `end_fade` 1, 7, 13, 15, 16 → 16, 0x70, 0xD0, 0xF0, 255 (V c1b DL 428/434/436). `pack_audio_reads_back`: the jingles (22050 Hz, > 1 s), the flag, canister and lose cues |
| Sounds, `colors/flags.sh capture` with `RUST_LOG=info,conker_client::audio=debug` | Flag take: 0x5B0 read 326, 0x5A3 336, 0x5B1 362 (V c1a 328 / 365); again at 803/813/838. Captures at 400 and 840 (the final one): the squirrels' jingle and Tediz voices 0x662-0x664 at 434 and 873 (+34/+33; V +30); the outro's voice at 910 |
| Sounds, `total-war/gas.sh gas` (same log level) | Canister taken (server t 1100): 0x5B0 read 700, 0x599 710, 0x5B1 804. P2's mask: `MUFFLE … p=1 masked=true muffled=true` at 1006 in P1's client (another player's put-on), `muffled=false` at its take-off, 1392 |
| Screens (read) | `total-war/flicker-lit.png` / `flicker-dim.png` (`gas.sh flicker`: ids 2 and 4 at reads 310/330; id 4 buzzed at 301, its shelter and lamps dimmed while the tunnel is not). `colors/end-a.png` / `end-b.png` (new in `flags.sh capture`: the end fade seen from read 883/884, the clock dimmed with the view at 894); the capture set regenerated (`fade-black`: the HUD under the fade) |
| `bun run test` | All pass except conker-bots `colors_bots_take_a_flag`. That test is not new; conker-bots depends on none of this batch's crates, and the failure comes with the bots lane's uncommitted conker-bots/conker-rules edits. conker-client, conker-extract and conker-assets were re-run after the last edits: pass |
| `bun run check` | Clippy (all crates, and this batch's with `--tests`) and the web check pass. `cargo fmt --check` flags only `conker-rules/src/game.rs:2076`, a line in the rules lane's uncommitted change. `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` passes |

**Gaps.**
- **I:** 0x2D's calm lamps are not drawn as lights (the shelter keeps its vertex colours), so a flicker dims the whole shelter by lit(now)/lit(full), not the original's per-vertex light. 0x2B's lamps shine with the flickered size. The flicker steps per client (seeded by its id), so each client sees its own bursts, as each console did. Deathmatch on 2D flickers too (the lamps belong to the map).
- **C, kept:** any player's put-on muffles the local sound, bots' too (`func_15074664` has no camera test). A death drops the mask without a take-off, so the muffle stays until a wearer's take-off or Over (I: the reset at Over). The muffle filters the effects only, not the background, the lines or the jingles (I: the parameter is set on effects player 1).
- **I:** a bot's radio clicks (volume −25000) are taken as unheard and not played.
- **I, latency:** the jingle, the losers' voices and the end fade count from the tick this client sees the change, 3-4 ticks late (the server's tick is not sent).
- **Rules lane:** the remake's end phase starts at the capture + 41 and Over at + 57; the original is +39 and +55/56.
- **V, a choice:** the research's recipe says no change for the end phase. But the client drew nothing in Fading, and the DLs show phase 7's fade (V c1b DL 428/434/436), so `end_fade` now draws it, in every mode: phase 7 is shared (C `func_151E53E8`).
- **Done in E10** ("The results screen's details"): the end phase's HUD mask (`func_151E53E8`: HUD flags `&= 0xE35F`, the radar and others off). No screen of the shrink: the canister is taken 2 frames after it shows, under its taker, and idle offscreen humans take nothing else.

## Bots and movement fidelity, part 2 (2026-10-13)
**Batch E2, the bots. Bots now hear only what the original's bots hear: a grenade's wind-up at its thrower, then
its fuse at the grenade. Gunfire and blasts make no noise. Query 0xD4 (s126+0x78) gives the drawn weapon from the
draw's first frame and 0 from the holster's first frame, which ends 0x34's draw/holster stall. A burning bot's
cooldown stays at 0x14, and the props command (3, 1) is known. New regression tests cover 0x30's lift pair and
0x34's corridor. Without the invented gunfire noise, Deathmatch bots fight less on 2D and 0x34 (less clearly elsewhere): some
stand and fire at what they can't hit (see Gaps).** Research: `../lab/research/mp-original/bots-fidelity.md` (Q1, Q3-Q5) and
`bots-beach-club.md` (Q2).

| Part | What |
|---|---|
| conker-bots `lib.rs` | `noises(m)` reads the frame just run and replaces the `Event::Shot` and `Event::Explosion` noises. The thrower is heard 900 around from 18 frames into the pull until the release (throw + 14), then each live grenade 900 around until it goes off (`func_15033AD8`, `func_15054F74`). `Body::drawn` is `Fighter::camera_weapon()` (the canister while carrying), not `f.drawn`, which lags the draw by its clip and the holster by its clip. `Body::burning` is `f.burn.is_some()` |
| conker-bots `brain.rs` | `Body::burning` (s126+0x1AC) holds COOL G+0x25C at 0x14 every frame it burns (`func_1508A6FC`). Command (3, 1) hides a prop and returns 1, as (3, 7) does. It covers 0xBA's masks 0x14-0x19 (0x48DE) and 0xBD's camera cutaway on 0x34, 0xF3-0xF5 (0x335C). It no longer counts as an unknown native (6 calls a match on 0x2D) |

| Check (2026-10-13) | Result |
|---|---|
| New tests | `a_grenade_is_heard_and_nothing_else` (lib): C-down held 41-43 gives the thrower at 60-84 and the grenade at 85-203; held 41-101 gives 60-116 and 117-177, as oracle q5gren2 and q5cook. The twin guns' shots give no noise. `a_placement_replans_once` now also checks that a burning bot's COOL stays 0x14. `rom.rs`: `lift_pair_riders_leave` (0x30, 15 bots, seed 3, 3,000 frames): no bot rides more than 6 times in a row less than 60 frames apart; the same run on 0xBA rides 1,087 times in a row. `no_jump_loop_in_the_0x34_corridor` (0x34, 3 bots, 4,500 frames): no stay over 300 frames in the corridor box with a running jump. It passes on 0xBA too, since HEAD's 0xBD already closed that loop, so it guards the corridor time |
| Oracle (`ConkerRecomp.e2bot`, scratch `bots-fidelity-scratch/e2/`) | **o34draw** (scenario 4, 1 human + 3 idle; s126+0x78 watched, BRK `func_1508A6FC`): +0x78 is set on the draw's first frame and cleared on the holster's first frame, and the holster counter G+0x2208 restarts 2 reads after it reaches 0. The remake's lagging `drawn` kept counting through the holster, and the last B drew again: draw 40 frames, drawn 45, holster 29, over and over. **o34emit** (BRK `func_151A6600`, `func_1508B20C`): 1,452 noise calls on 0x34, all from the particle emitters (500 around, 121 frames each). The prop flags G+0x23F0.. stay 1 while the emitters come and go, so (6, 3, 1) does not drive them and they are not ported |
| `bot_match`, 8 seeds a map (means): deaths / the human's / hp drops (hits + deaths) / pickups a bot. "vA" is E2 with HEAD's noise sources put back (shots and blasts, no grenade) | **2D** (3 bots, 9,000 frames; oracle `bot_long3`): HEAD 12.2 / 5.0 / 55.5 / 5.8; E2 8.8 / 3.5 / 39.8 / 4.8; vA 12.4 / 4.6 / 56.8 / 6.2; oracle 13 / 5 / 48 / 6. **0x30** (3 bots, 4,496; q3s5b3): HEAD 2.0 / 0.4 / 10.5 / 2.4; E2 1.6 / 0.4 / 8.5 / 2.0; vA 2.1 / 0.4 / 11.8; oracle 9 / 4 / 50; the tree with part 1's high jump 4.2 / 1.6 / 19.6. **0x34** (3 bots, 4,403; q3s4b3): HEAD 4.8 / 1.2 / 16.1 / 3.5; E2 4.0 / 1.2 / 11.8 / 3.9; vA 6.0 / 2.0 / 18.5; oracle 8 / 3 / 37. **0x3F** (15 bots, 1,738; oracle s7_h1b15, its human kept alive): HEAD 20.9 / 2.2 / 112; E2 18.9 / 1.6 / 116; vA 20.8 / 2.0 / 113; oracle 22 / 0 / 158. With HEAD's drawn and E2's noise, 2D gives 9.0 deaths: the drop comes from the noise |
| 2D: longest stay within 20 units, a bot's best per run (5 seeds) | HEAD 953-5,359; E2 3,593-7,973; oracle `bot_long3` 1,334-2,975 |
| 0x34 state 5 (3 bots, 4,500 frames, seeds 1-5): the longest episode a run | HEAD 528-3,269; E2 507-1,011; oracle q3s4b3 182 |
| Club, 0x3F (15 bots, 4,474 frames, seeds 1-3; s126+0x78 per bot-frame) | Spells / median / mean / drawn share: HEAD 136-148 / 62-93 / 130-156 / 29-37 %; E2 133-143 / 104-144 / 158-160 / 35 %; oracle c1 104 / 193 / 226 / 35 % (living bot-frames; the remake's share counts the dead too) |
| `bots.sh` (debug, HEAD and E2 side by side on ports 5980-5986, part of it under a parallel build's load) | **Colors** `BOTS=7`, 150 s ×3: kills HEAD 4 / 6 / 4, E2 8 / 8 / 8; captures HEAD 1 / 0 / 1 (t 4022, 4052), E2 0 / 0 / 0 (takes 0 / 1 / 1). 300 s ×2: kills HEAD 14 / 9, E2 14 / 16; takes 2 / 3 against 6 / 5; captures 0 / 2 against 0 / 1 (t 7607). Oracle bots1: captures at 2983 and 6449. **Beach** `BOTS=7`, 150 s ×3, scores [villagers, Tediz] at the last point (t 4,600-4,950): HEAD [5, 10], [5, 10], [0, 9]; E2 [1, 9], [1, 9], [3, 5]; escapes 3 / 3 / 0 against 1 / 2 / 3. Oracle bots1: 20-7 to the Tediz at 8,125 (about 12-4 at 4,800). No fault, unknown native or panic in any run |
| `bun run test`, `bun run check` | pass (346 tests, with part 1's work in the tree) |

**Gaps.**
- **Fixed below (E2c): bots stand and fire at what they can't hit.** The cause was the bullets, not state 8: the twin guns (weapon 0x09, not the bolt) flew level along the facing and never reached a target above or below.
- **V, not ported:** the flame stream's noise (`func_1519A9A4`, V q3s5b3: at the stream's middle, half its length around, up to 352) and 0x34's particle emitters (`func_151A6600`, V o34emit: 500 around, 121 frames; their trigger is untraced).
- **V remake:** 0x34's longest state-5 episodes (507-1,011 against 182) are now chases across the map, the goal following the enemy's node, not the draw/holster stall. Standoffs remain on 0x34: r34b5s1's 0x23 bot stands 8,608 frames in both builds, in states 15 and 6, firing across heights.
- **I:** `(4, 0x10)` asks whether any participant is in the trigger; the original tests the humans' view eyes (0xBD's cutaway).
- **I:** The AI hears the noises of the frame just run; the original pushes them during its frame. The windows' first and last reads match the oracle.
- **Out of E2's scope:** 0x30 has 2.0 deaths per 4,496 reads at HEAD against the oracle's 9 (part 1's high jump brings it to 4.2). The club's spells are still shorter than the oracle's (median 104-144 against 193).

**Fixed (2026-10-13, E2c): bots stand and fire at what they can't hit.** Only the target's death or a change in
sight ends AI state 8, in the original as in the remake. The original's stands end because its bots' bullets reach
targets above and below them. A bot's twin-gun bullet leaves the muzzle toward the first body part or wall on its aim
ray, from the eye 67.5 up through the aim point (`func_151C62D0` → `func_151D5174` → `func_151C4B0C`, C). Its
pistol and 0x23 bullets keep the facing and take the pitch byte obj+0x1D1 (`func_15145740`, C), which follows the
aim's pitch (V o34p). The remake fired all three level along the facing. The oracle stands too: its p3 under the
idle human's ledge on 2D (3,813 away, 420 up, reads 2735-4564) fired 394 bullets for 0 hits. The aim point restarts
every 3 frames at 0.25-0.58 of the way, so the bullets hit the ledge's lip; the remake now does the same. That stand
ended when another bot killed the human. Also on 0x30 (method step 2): a twin-gun bullet on the head (part 0xA) takes
1 health at once, stunned or not, and below 3 it kills with type 0x20 (`func_151D4DAC`; C, V ohit). In ohit that
path made 5 of the 6 bullet deaths, all 4 of the human's. Only bullets that test body parts have a head
(`func_151C229C`'s flag 8): the twin guns' do; the pistol's, 0x23's and, in multiplayer, the mounted gun's do not
(C; V e_w39, e1). No wire change (protocol 20). Research: `../lab/research/mp-original/bots-fidelity.md` (E2c).

| Part | What |
|---|---|
| conker-rules `game.rs` | `Match::aims`: per slot, a bot's eye and unit aim direction (not on the wire). `strike`: a twin-gun bullet is re-aimed from its muzzle at the first part or wall on the ray (`first_part`, `raycast`); a pistol or 0x23 bullet takes the ray's pitch on its facing. `fly_bullets`: a twin-gun body hit at `BULLET_HEAD` (47.25, I: fitted to ohit) or more above the feet (26 less crouched) takes 1 health directly. Below 3 health it is a `SNIPER_RAY` hit instead, with no stun or immunity check (a kill, reaction 0x2E, and a headshot on an enemy) |
| conker-bots `lib.rs` | `Bots::scopes` fills `m.aims` for each aiming bot: from (x, y + 67.5, z) through `Bots::aim` |

| Check (2026-10-13) | Result |
|---|---|
| New tests | conker-rules `a_bots_bullets_follow_its_aim_ray`: a target on a ledge 415 up and 3,600 away. The twin guns hit it with the ray and pass under the ledge without it; the pistol and 0x23 take the ray's pitch. `a_bullet_on_the_head_hurts_through_the_stun` (ohit 1328-1333): a stunned player at 4 health is hit on the head at +55, +60 and +61 and goes to 3, 2, then dead, with reaction 0x2E and a headshot. A body hit at +40 is refused. An unstunned head hit takes 3 → 1 (ohit 2379). The pistol at +60.5 has no head. conker-bots `rom.rs` `no_endless_stand_below_a_ledge_on_0x2d` (2D, 3 bots, seed 5, 4,500 frames): no bot stays within 20 for over 3,000 frames. HEAD fails it: slot 1 stands from frame 1,164. `colors_bots_take_a_flag` now takes at 2,292 |
| Oracle (`ConkerRecomp.e2c`, scratch `bots-fidelity-scratch/e2c/`) | **obul, obul2, omem** (2D): the eye sits at +67.5 exactly (7,771 of 9,956 rows). The bullets' pitch lies between the muzzle's and the eye's to the aim. The 1,800-frame stand is above. **ohit** (0x30, BRK `func_151D4DAC` with the hit record's +0x58 part and +0x59 kind): 209 body contacts in 4,418 reads, all type 0x1A and kind 3. Head hits (part 0xA) land at +47.3 to +60.6; a living body is hit at +47.2 or less. **o34p** (0x34, obj+0x1D1 and s126 per bot): obj+0x1D1·1.1° follows the aim's pitch for 0x23 (mean error 3.4° where the pitch is over 3°) and 0x09 (1.8°) |
| `bot_match`, 8 seeds a map (means): deaths / the human's / hp drops (hits + deaths; the head's direct −1 is no hit) / pickups a bot | **2D** (3 bots, 9,000 frames): HEAD 9.8 / 4.0 / 39.0 / 4.9; E2c 12.0 / 4.4 / 52.1 / 6.2; oracle 13 / 5 / 48 / 6. **0x30** (3, 4,496): HEAD 4.2 / 1.6 / 19.6 / 2.8; E2c 4.1 / 1.0 / 17.4 / 3.1; oracle 9 / 4 / 50. **0x34** (3, 4,403): HEAD 4.1 / 1.2 / 14.5 / 3.6; E2c 7.0 / 3.5 / 28.9 / 3.8; oracle 8 / 3 / 37. **0x3F** (15, 1,738, club only): HEAD 17.0 / 2.2 / 106.6; E2c 17.1 / 2.2 / 103.8; oracle 22 / 0 / 158 |
| 2D: longest stay within 20 units, a bot's best per run (8 seeds) | HEAD 1,971-7,973 (mean 4,319); E2c 1,702-5,934 (mean 3,132); oracle `bot_long3` 1,334-2,975. Seed 8's 5,934 is p1 at the oracle's own spot under the ledge, firing at the human. It lasts until another bot kills the human |
| `bots.sh` (debug, ports 5970-5978). "Off" is this tree with both changes switched off by a temporary switch, so HEAD in these crates. Each run repeats exactly | **Colors** 150 s: kills off 8, E2c 10; takes 2 / 3; drops 2 / 2; captures 0 / 0. 300 s: kills 17 / 16; takes 3 / 4; captures 1 at 8,154 / 1 at 5,636 (bots1: 2,983 and 6,449, about 20 deaths by 9,000). **Beach** 150 s: the same events in both (an escape, a strike, 11 villager points, [1, 11] at the last), 8 frames apart. Colors ROM runs (11 seeds, 5,400 frames each): deaths off 8.5, E2c 11.5 (bots1: 9, and 21 in 9,898). First take off 1,756-12,375 (2 seeds past 5,400), E2c 1,758-4,787 (none past). No fault, unknown native or panic |
| `bun run test`, `bun run check` | pass (355 tests) |

**Left.**
- **V remake: 0x30 deaths are still 4.1 against 9.** The head rule is real (V) but was not 0x30's first divergence (HEAD 4.2 → 4.1). The gap has a different, untraced cause: the weapons the bots hold. A
  match (v4, 8 seeds) has the twin guns drawn 2,056 frames, the sniper 2,706 and 0x23 893. In o30 the oracle has
  3,384, 805 and 103. The bots fire 443 shots a match against the oracle's 553 (s126+0x1AB). The oracle lands 209
  twin-gun contacts in 4,418 reads, mostly on stunned players. Next: what sets the wanted weapon (G+0x2338) and the
  pickups on 0x30.
- **I:** the head is a height on the bullets' column, fitted to ohit, not the game's parts (`func_1508114C`).
  obj+0x1D1's 1.1° steps and its ramp (3 steps a frame, `func_1504C9E4`) are not ported. Neither is the crossbow's
  pitch without lock-on (o34p: 0x18's obj+0x1D1 is far from the aim's, units untraced).

## Bots and movement fidelity, part 1 (2026-10-13)
**Batch E2, movement and rules. Humans and bots have the high jump (state 235, A with Z held: 9 still frames, then
vy 76, about 356 up, and a hard landing). The bots' scripts press Z+A when a wall moved them (query 0xC3 bit
0x10000), so they climb ledges. The kill gates (trigger type 1, action 4) of 0x2D, 0x30 and 0x2B kill whoever
falls through: a bot is back on its record the same frame, a human lies dead 8 frames, and on Beach a villager's
crossing is the Tediz' point. A knocked body that rises into a slope rides onto it instead of sinking under the map,
and the trigger tests no longer wrap at 16 bits. War: Beach's plunger top sinks with its prop, taking its presser
from 508 to 448. Protocol 20.** Research: `../lab/research/mp-original/bots-fidelity.md` (Q2) and
`bots-beach-club.md` (Q1, Q3).

| Part | What |
|---|---|
| conker-sim `movement.rs` | `state::HIGH_JUMP` 235. Entry: on the ground, A pressed with Z held, no weapon drawn, from a `can_jump` state, 22 or 27. The entry frame is a plain ground frame; then timer 0x64 and lock 0x17 (the lock brakes 2.5 a frame, the walls still slide). The 10th frame launches: vy 76, gravity 2, but 9 on air frames 4-7. While `vy − g > 6` there is no drive and xz is 0. It lands hard (54 for 12 frames). `PlayerState::walled`: the wall routine moved x or z this frame (obj+0xF4 bit 0x10000, `func_15058898`), set in the ground and air steps |
| conker-sim `collision.rs`, `net.rs` | `CollisionWorld::shift(placement, dy)` moves a placement's triangles (absolute, so repeating it is harmless). Protocol 20: state 235 and `walled` in `PlayerState` |
| conker-anim, client | `id::HIGH_JUMP` 235 plays the pack's clip pair 21 (logical 235/404/741, 54 frames) at rate 1.08 (`*0x80099660`), blend 2; `ANIM_IDS` has it. `seat_plunger` moves the plunger's collision to `MatchView.plunger` before prediction, and `audio.rs` looks for the presser at that y |
| conker-bots (one query) | Query 0xC3 ORs in 0x10000 when `Body::walled` (a new field, filled in `bodies()`). Scripts 0xBB, 0xBD, 0xBA, 0xB9 (villagers) and 0xBE press Z then A on it |
| conker-rules `lift.rs` | `kill_gates`: the records with type 1, shape 3 and action 4: 0x2D's 0x4E/0x4F at y −185, 0x30's 0x0C at −2071, 0x2B's 0x0F at −1641 (0x34 and 0x3F have none). `Lifts::ride` tests them first, for every participant, the dead too, and a bot taken is a placement (the bots' `lifted`). `Lift::gate` refuses offsets past s16 |
| conker-rules `game.rs`, `damage.rs` | `Match::fall_kill`: deaths +1, the weapon dropped, no kill and no +0x70 for anyone. On Beach the villager's point comes at once (a corpse's pending blast point scores here, and not again later). A bot respawns the same frame; a human gets `damage::GATE` (dead and still, its clip kept, 8 frames). `down` (the out-of-lives test, else the respawn) is shared with `hurt_frame`. `knock()`: a body rising (y_vel ≥ 0) that ends below its floor is put on it and keeps flying. Who stands on the plunger's top rides it after `plunger.step`. `Match::player` clears `walled` at each frame's start, a hit frame's too (`func_1504A730`) |
| conker-rules `beach.rs` | `Plunger::top(y)`; `under(pos, y)` covers that top up to 508, because a world that is not seated keeps its presser at 508. `PROPS` [0, 5] (prop 4 has no collision); `seat(world, y)` |
| conker-camera `volume.rs` | `CameraVolume::fits`: `contains` and `crossed` are false when an offset is past s16 (history B's escape fired at y −525,400) |
| Server | Calls `beach::seat` before the bots and the step (`ResMut<Collision>`), and `lifts.reset` on every `Respawn` event. New log lines: "falls through a kill gate", "takes / ends a high jump" |

| Check (2026-10-13) | Result |
|---|---|
| New tests | conker-sim: `high_jump_matches_the_game` replays oracle q2za (a crouched human, reads 49-79) and q2s5w (bot 12's wall slide on 0x30, reads 110-170): states equal, y and vy within 0.0008, xz within 0.006. Also `z_and_a_high_jump`, `a_wall_slide_is_walled`, `a_placement_shifts`. conker-rules: `a_kill_gate_kills_who_falls_through`, `a_kill_gate_takes_the_dead_and_scores_once`, `kill_gates_of_the_maps` (ROM: the gates per scene; fall6's drop crosses 0x2B's), `a_knock_up_a_slope_stays_on_it` (fails without the fix: 3 under the slope on its second frame), and `beach_plunger` (the presser 508 → 448 in 15 frames, the prop's y + 116 each frame). conker-camera: the wrap cases |
| Oracle (`ConkerRecomp.e2sim`, a copy of e2bc) | A Deathmatch drop through 0x2D's 0x4E (scenario 3, the human teleported to (−3397, 300, 1600) at read 60): it crosses at read 79, has hp 0 at 80 and is frozen at −192.75, and is back at 88. `0x800E0AD0` deaths +1; gGameState (kills, +0x70) unchanged; the player's rank drops (0x800E0AC0), as the remake's kills − deaths does |
| The climb on 0x30 (debug, `CARGO_TARGET_DIR=target-net`; a `/tmp` script: `--bots 15` and one idle human, every slot `--spawn`ed where q2s5w's participants stood at read 23) | Slot 12, from the oracle bot 12's spawn (1317, −680, −1677), walks to node 47 at the wall's foot, takes 235 at t 115 at (1115, −624, −494), and lands as 54 on the ledge at y −308 at t 157 (the oracle: 235 at read 112 at (1048, −624, −494), 54 on −309 at 153). It goes on over nodes 46, 45, 44, and climbs there again at t 1116. With the slots shifted by one, slot 11 makes the same climb (t 115 → 158, lands at −309). The plain `bots.sh` runs on 0x30 (5 bots 240 s; 15 bots 150 s) have 0 and 8 high jumps, none there: the bots start elsewhere |
| Beach (`bots.sh`, `BOTS=7`, 300 s, port 5972; `SCORE=unlimited` in the second) | Both: villager 0 is blasted into the west pit and crosses 0x0F at t ≈ 7550 (y −1707). It respawns on record 11 that frame and the Tediz score [4, 18] → [4, 19]. No body is ever below −1404 (the tunnel), no y is past ±10,000, and the escapes are the script's two. Run 1 ends 4-20 at t 7550 and goes on into a second match; run 2 runs on to 4-26. The two histories are the same until the first match ends: the start differs by 14 ticks, and the bots' RNG is the same. Villager bots also high-jump (0xB9 presses Z+A for team 0, C). No fault, unknown native or panic |
| 2d Deathmatch (`BOTS=3`, 150 s), Total War (`BOTS=3 HUMANS=2`, 150 s), `smoke.sh` (2d, port 5976) | DM: every bot moves and fights; bot 2 never died (25 hits, 4 kills); no high jump or gate. TW: 3 high jumps (0xBA on a wall), no gate; the usual War FAIL lines (bots that neither fought nor died). Both diverge from HEAD where a bot pushes a wall, as the original does. No fault, unknown native or panic. `smoke.sh` passes: the predictions equal the server's (0.000000 over 685, 683, 272 ticks) |
| `bun run test`, `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass (346 tests); conker-camera's 31 (not in `bun run test`) pass |

**Gaps.**
- **I:** The high jump's entry from states other than 22 (q2za) and the wall slide (q2s5w) is untraced, and so is the drawn-weapon exclusion. The rise's hold (`vy − g > 6`) is fitted to the two traces. A body that slides off an edge during the wait just falls.
- **I:** The knock's rise onto a slope follows `bots-beach-club.md` §1.5's recipe: the game's hurt movement on the rise is unread, and the recipe meets the oracle's bound (no body more than 23 under its floor). The other choice, landing it as a fall does, would bounce it off the slope. The s16 guard covers bodies the game never has.
- **I:** A human's 8 dead frames after a gate are its fade, as the lifts' human path is (V for the count, I for the reason).
- **Remake:** A predicted presser stands on the server's last plunger y, a round trip behind, and the server corrects it. The client plays no sound for the high jump.

## Match flow fixes (2026-10-13)
**Batch E1. A join or leave mid-match no longer restarts the level's AI script: the gas, the canisters, the masks,
Colors' flags and scores and Beach's escapes and timers carry on, and only the changing slot's own AI memory starts
over. The bots' roster skips free slots, the lifts reset per slot, and a waiting player can't be hurt. Both teams out
on one frame, Beach's 8 villagers and the end phase follow the original. The outro and the STATS go by the server's
winner. Protocol 19.**

| Part | What |
|---|---|
| conker-bots (`lib.rs`, `brain.rs`) | `Bots::new(assets, humans, bots, ..)` takes the slots, so a free slot is no participant. `Bots::join(slot, bot)` takes the lowest vacant participant, else the next (at most 16). `Bots::leave(slot)` leaves its participant vacant. Either way only that participant starts over: its NPC, aims and scopes, its team G+0xE64, its seen bit G+0x18 and its dead flag G+0x35C. A vacant participant is left out of query 0x1D and has team −1, so no human takes a bot over by it and its going is no loss to the tally. It stands far away with place 255, and player commands on it do nothing. A Beach joiner appears on its first frame, so its deadline starts 48 frames in. The human and bot loops (0x23, visibility, noises, ranks, +0x70) go by `Body::is_bot` |
| conker-rules | `Lifts::reset(slot)` replaces the unused `remove`. `Match::waiting`: `hurt` skips a waiting player. `apply` does nothing once over. Total War: players out on one frame take that frame's places, team 0's first (the worse). `beach::MAX_VILLAGERS` 8. `restart_gas` is gone |
| Server | `restart_bots` runs only at the server's start and the match start, over the taken slots. A join calls `Bots::join` and `Lifts::reset`, a leave `Bots::leave`. `capped` returns `Option<Side>`: a full Beach (8 + 2) refuses a joiner, and `--bots` over 10 on Beach is an error. `Game::won` latches the winner on the first tick over; `results_at` and `MatchView::winner` use it. `restart_in` counts through the outro (no `.min(RESULTS)`). `Match::waiting` is set before the script's effects and the step |
| conker-sim `net.rs` | Protocol 19: `MatchView::winner`, `RESULTS` shared with the client, `DEV_PRIVATE_KEY` removed |
| Client | `outro.rs`: the outro is the one for `MatchView::winner`, from frame `frames − (restart_in − RESULTS)`, so a client that joins mid-outro starts there (a forced `CONKER_OUTRO` starts at 0). `hud.rs`: `rows` and `header` take the winner instead of ranking the roster. More than 4 rows share the 4 rows' height, each scaled by 4/n around its label's and columns' tops |

| Check (2026-10-13) | Result |
|---|---|
| New tests | conker-bots `rom.rs`: `colors_keeps_its_flags_through_a_join_and_a_leave` (a human leaves 40 frames after the red flag's grab and another joins its slot 40 later: the vacant participant's team G+0xE64 is −1 and the joiner's 1; both flags stay with their carriers for 1,200 frames, no fault or unknown native), `beach_keeps_its_deadlines_through_a_join_and_a_leave` (villager 1 leaves at 1000 and a joiner takes its slot at 1100: villager 0's strike is still at 2748, the joiner's G+0x1574 = 5400 at 1148; a restarted script would strike at 3848). conker-rules: `total_war_both_out_on_one_frame` (fails without the fix: the squirrel in the later slot won), `a_waiting_player_takes_no_hit`, `nothing_applies_once_over`, `lift::a_joiner_rides_from_its_own_place`. Server: `beach_caps_the_tediz` (10 bots: 8 + 2, then `None`), `match_settings_parse` (`--bots 11` on Beach), `the_results_count_down` (through the outro; the latch). Client: `eight_rows_fit_in_four`, `a_late_client_joins_the_outro_where_the_server_is`, and the rows and header from the winner |
| Mid-match join and leave (debug, `CARGO_TARGET_DIR=target-net`, ports 5970-5974; a `/tmp` script: one idle human all match, a second joining on a trigger and leaving 15 s later) | **Total War** (`--bots 4`, the idle squirrel warped onto canister 27, then into the shelter): delivered at t 1279; the joiner came in at ~1310 and left at ~1770. The haze came at 1550 and the gas kill at 1579 (+300; the joiner died too, credited to the deliverer). The gas went off at 1668 and canister 27 was back at 1959 (+680, within 600-900). **Colors** (`--bots 7`, the idle Tedi warped onto the green flag): taken at 620; the joiner came in at ~650 and left at ~1080. The flag stayed carried until its carrier died at 1513, was retaken 51 frames later, and never went home. **Beach** (`--bots 7`): the joiner came in at ~1290 and left at ~1756, with no burst of start records at either (the match start's is the only one). The idle villager's strike came at t 2991 = start + 2748 (read 2771; V 2772), and the scores ran on from [0, 1] to [0, 14]. No fault, unknown native or panic. Two Colors bots camp (states 9 and 1, from t ~510 and ~750); the `bots.sh` run without a roster change has the same two |
| `bots.sh`, `smoke.sh` (no roster change) | Beach `BOTS=7` 150 s: pass (6 escapes, a strike at start + 2748, 7 points). DM `BOTS=3` 150 s (port 5970): every bot moves, fights and dies; bot 2 died near the end with no respawn yet, and the `(6, 3, 1)` native is the known one (6 calls a match on 0x2D, as before). Total War `BOTS=3 HUMANS=2` and Colors `BOTS=7`, 120 s: no faults, unknown natives or panics; some bots neither died nor respawned in 120 s (the script's FAIL lines), as in earlier War runs (snipers, campers); Colors had no take in 120 s (earlier runs: 2258 frames, or none in 330 s). `smoke.sh` (2d, port 5976; client 3 joins mid-match, protocol 19): pass, the predictions equal the server's (0.000000 over 685, 683, 272 ticks) |
| `bun run test`, `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass (335 tests) |

**Gaps.**
- **C:** The both-teams-out rule is read from the code (`func_151E565C`'s team loop at 0x151E5DD0), not traced, and an ordinary end can't reach it (I, `total-war-teams.md` §4). Colors untimed with an unlimited score still never ends: the original's SET-UP allows it (`menus-and-flow.md`: time and score each have UNLIMITED) and nothing ends it (C; I, not run to the end, `colors.md`).
- **I:** The script splits humans from bots by index, counted once at its start (G+0x1EC8). A human that joins with no vacant participant is appended after the bots and stays one of the script's bots until the next match: on Beach it has no 90 s deadline, and in Total War no traitor check and no takeover. The original has no mid-match joins to compare with. A Beach joiner gets no start record from the script (the server's placement mask stands until its first escape).
- **Remake:** A client refused by a full Beach (8 + 2) stays connected with no player: lightyear's server offers no per-client kick here. Rows past the fourth have no label (the pack has P1-P4). The winner latches on the first tick over, so a leave during the end fade can still change it.

## War: Beach: the client (2026-10-13)
**The client draws War: Beach (`--map 2b --mode beach`; `../lab/research/mp-original/beach-presentation.md` §8). Each
villager is drawn as 156 or 157 in its class's look. The client has Beach's HUD and STATS, the escape (the model's
fade, the view's dim and fade in, the radio line), the detonation (explosions, debris, a villager's 60-frame cut, the
shake), the strike (red light, blast, red flash), the plunger, the water, the lamps and exit lights, and the outros
cs4/cs3. The local prediction now rides the box lifts (0x2B's, 0x30's). No pack, protocol or server change.**

| Part | What |
|---|---|
| `lib.rs` | `VILLAGERS`, `characters(map)` (2b adds 156, 157 after `PACK_CHARACTERS`), `body(&Combat)` (class 0x11-0x1C → 156, 0x1D-0x20 → 157, else the side's), `BodyClips`. `MatchArena` holds the scene's lifts (`conker_rules::lift::parse`: the lift rules moved from conker-bots into conker-rules, which the server, the bots and the client share). `own_tick` rides a box lift (shape 2) before the step, as the server does, with a respawn's event: the camera cuts on the ride's tick, not a lead later. 0x2D's gates (shape 3) need the last move and stay the server's |
| `render.rs` | Model, rig, lift and clips by body. `rebuild_models`: a class re-rolled onto the other model gets a new model, rig and held models |
| `beach.rs` (new) | `dress`: each glTF primitive's `extras.list` is hidden by `Character.looks[class]`. `fade_escapees`: the model's own material copies, alpha −16 a frame, hidden while frozen, back at the respawn. The plunger (placements 0, 4, 5 at `plunger − 392`). The water (`04-2b-05@1`'s `m50-tex0530`, v + 0.25/64 a frame). `Blasts`: the detonation is `cooldown` 0 → > 0; a strike is a villager's `deadline` reaching 0, its blast at BLOWN_UP or the call + 40. Camera-facing puffs of `hud/explosion.png` in det1's colours, and falling debris, at the three records and at the struck villager. The strike's red light (86 → 9 → 38 → 25 over 40 frames). `camera`: a villager's view on camera 0x9002 for 60 frames (`det1_camera.csv`), every view shaken |
| `hud.rs` | The Beach strip (`beach_teams`), a villager's countdown `deadline / 60`, no radar, no lives. `escape_fade`: alpha 0x11 for the freeze's last 30 frames (the re-sent fade, kept), then in over 100 VIs. `strike_flash`: the iris quarters red 0xF8 → 0xC2 over 7 frames, then black. `rows` drops villager humans; the header is `beach_won` (FRENCHYS / TEDIZ WIN) |
| `audio.rs` | Sounds per body. Beach's cues: the detonation ×3 at its records with debris, the presser's plunger voice, PlungerReady in trigger 0x25, the strike's call and blast with debris, the radio click and voice (humans, 300 ticks apart), and the escape line 0x1A8 with the music ducked (18000 → 7690 at −563 a frame, back +4096). The rocket's shot sound also on Beach's fire clip 0x207 |
| `lights.rs` | `calm` lights shine in Beach (the two lamps, the two red exit lights at 87 + 43·sin φ). The pulse now counts ticks: 2.05 s, as the notes say. Total War's red light (and its tint) pulsed twice that fast. The room tint stays Total War's |
| `outro.rs`, `held.rs`, `main.rs`, `connection.rs` | Beach is a team mode (cs4 or cs3, none on a draw). `Hands` goes with a rebuilt model. The slice, the loads and the readiness go by `characters(map)` |

| Check (2026-10-13) | Result |
|---|---|
| Client tests | New: `beach_matches_the_originals_draws` (det1 DL 318: heads [66/198, 196, 16, 16] on rows 0/16, scores [83,197,8,11] / [215,197,6,10], "81" at [140,6,8,11] / [147,6,6,10]; no radar, no icon for 0x3E, no countdown for a Tedi; the escape's 0x11 and 0xF5, 0xEB, 0xE1, 0x99 (e1 DL 252-270); the strike's flash; FRENCHYS [65,34,96,32] + WIN [151,34]; only the Tedi human's row). Also `the_looks_hide_their_lists` (pack: 156 0x1B → lists 0, 4, 5; 157 0x1E → 0, 2, 4; 0x14 → 0, 1, 2, 5; 0x11 → 0, 2; 0x1F → 0, 1, 3), `escapes_strikes_and_the_cut_follow_the_original` and `the_prediction_rides_a_lift`. The other modes' tests are unchanged |
| Screens (`../lab/screens/beach/shots.sh live|blast|end`, port 5980; all read) | `villager` (79 at the top, strip 0-0, clock, no radar/icon), `tedi` (strip, no countdown, the gun in the slit), `looks` (157 0x1E/0x1D: hat, red coat, kilt; 0x1F: nightcap, scarf; 156 0x14: suit, sunglasses), `escape-fade` (the escapee translucent at the tunnel), `escape-dim` (the tunnel's dark; 0x11 is all but invisible, as on the N64), `escape-in` (the new look on its record, the view still dark, 89, villagers 2), `lift` (record 4's view after the cut), `mg` (the gun and belt in the bunker), `shake`/`blasts`/`detonation` (puffs at the gun's record; a villager's view on 0x9002's bunkers, villagers 1), `strike-light` (three struck villagers in HIT, red-lit, countdown 0), `strike` (the red ring, the blast and debris, Tediz 4), `zone` (a Tedi holding the bazooka), `outro-cs4`, `outro-cs3` (forced), `stats` (FRENCHYS WIN, the two Tediz rows, strip 0-1) |
| Lifts | Groundwork's 2,350/2,544 errors were the server's placement, unpredicted. With the warp into lift 0 (`lift`), the ride comes as one correction drawn at once and the camera cuts on `Combat.lives` (`respawned at …: the camera cuts`). A walked-in ride is now predicted on its tick (`the_prediction_rides_a_lift`); only the record's jitter (≤ 40) is corrected. 0x30's lifts are boxes too (shape 2), so the same path covers them: checked by reading, not run |
| Sounds (one villager on the plunger at 300, into tunnel 0xD at 400; `conker_client::audio=debug`) | radio click 0x5B0 (read 319), detonation 0x2B7/0x2B8/0x2B6 (320, 342, 364: delays 0/23/45), the plunger voice 0x6A7 (329), debris 0x1B9-0x1C0 (357-392), click off 0x5B1 (365), the escape line (404), the lamps' buzz 0x44B-0x453. Not run: PlungerReady, the strike's sounds |
| `bun run test`, `bun run check`, `cargo clippy -p conker-client --target wasm32-unknown-unknown -- -D warnings` | pass (327 tests; the client's 60) |

**Gaps.**
- **I:** The explosions' sizes and emitter are not decoded; the puffs and the shake's motion are a guess. The strike's ground mark 0x3103 is not packed. The lamps' flicker (behaviour 4) and the sweeping ground lights (19) are not in the pack: the lamps are steady. The outros' part masks (op 5 0x3C) are not decoded, so outro actors draw every list (`outro-cs4`). cs4's song 0x8A is missing.
- **C (a choice):** The detonation's cut goes to every villager client. The original cuts only for a single local player 0 who is a villager.
- **Remake:** Fades blend in linear light, as Colors' do, so the escape's fade-in reads lighter than the N64's at the same alpha. An observer sees an escapee's fade start before its interpolated position reaches the tunnel: `Combat` is not interpolated (a free camera at the tunnel sees it from read ~309). 0x2D's gate lifts are not predicted.

## War: Beach: rules, server and bots (2026-10-13)
**`conker-server --map 2b --mode beach [--score 1-40|unlimited]` (default 20) plays War: Beach (scenario 8). The villagers
(team 0, the squirrels' side, item 0x3E: no attack) run for the tunnels under the Tediz' bunkers. The Tediz (team 1, at
most 2) hold the bunker tops and the mounted gun. Script 0xB9 is the referee, as Colors' 0xBB is: the escapes, the
plunger's detonation, the weapon zones and the 90 s deadline come through `gas::Effect`s. The match keeps the engine's
part. Protocol 18. No client drawing (handoff below).** Research: `../lab/research/mp-original/beach.md`.

| Part | What |
|---|---|
| `beach.rs` (new) | Teams, `MAX_TEDIZ` 2, `CLASSES` [0x1E, 0x14, 0x1D, 0x1F] (start, by villager order), `TEDI_CLASS` 3, `model` (156 below 0x1D, 157, Tedi 117), `reroll` (0x11 + rand & 0xF), `drive` (item 0x3E: 75 %, 150 % while the boost lasts, −2 VIs a frame), `BOOST` 600, `BOTTLE_RESPAWN` 30, `Plunger` (392 → 332 at 4 a frame when stood on or held, springs back a truncated tenth, settles at 383; its top at y 508), `EJECT` |
| `game.rs` | `Settings.beach`; 6 lives never lost; `Fighter.class` (re-rolled each respawn) and `boost`; 0x3E's speed before the movement (not while crouched); a bottle gives 600 VIs of boost and returns 30 frames after it is touched; a villager's death scores for the Tediz when it registers (MG +35 frames, blast at its sequence's first state, reaction 0x2C at once); death points latch on the next frame, the script's points on the same frame; score end, places and standings as Colors'; `Match.plunger` stepped after the guns. Effects `Start` (respawn mask), `Give` (0 takes it), `Strike`, `Plunger`, `Place`. `damage.rs`: `DETONATION` (script 0x29: reaction 0x2C, 7, no attacker), `STRIKE` (0x47: HIT 40 frames, then `DETONATION`). `gun.rs`: `places(0x2B)` |
| Bots (`brain.rs`, `lib.rs`) | 0xB9's natives. Queries (2, 0xBC) on 0x2010 (the gun's gunner), (3, 0x1E)/(3, 0x17) on 0x30FF (plunger y, presser), (5, 0x1D) 0 on the first frame, (6, 3) 0 (jingle), (0xF, 6) `pick_target` (`func_15089F9C`), player 0xA5 1 (class), 0xAA (drawn). Commands (3, 0xD) on 0x30FF (bit 4: held), (9, 0xC), player 0x1F/0x27 (effects), 0x54/0x8B (the client's); object 0xE02A (strike). Triggers 0x26/0x27 put a Tedi by the gun (`Place`) |
| Server | `--mode beach` (2b only, no `--cavemen`); joins past 2 Tediz become villagers; start item and class by side; respawn mask from the script's record; `BEACH t=…` log lines (Start, Score, Respawn, Strike, Place, Plunger, Give, "villager down"), "match ends"; `bots.sh` `MODE=beach` (villager bots need only move; escapes, detonations, strikes, points, zone gives, ejections, gun mounts) |
| Web | `data.ts`: mode `beach` (War: Beach, score), map `2b` (Deathmatch, Beach); `server.ts`'s whitelist follows |

**Handoff (client agent).** Everything is in `MatchView` / `Combat` (net.rs), and the whole `Fighter` (`class`,
`boost`) is replicated, so prediction already runs 0x3E's speed. Presentation details: `beach-presentation.md` and
the pack section below.
- `MatchView.beach`; `scores` = [villagers, Tediz], `score_limit`; the strip is `Hud.beach.strip`; the end screen is `beach_won`; outros cs4/cs3 through `places`.
- `Combat.class` → `beach::model(class)` and `Character.looks[class]`. It changes on `Event::Respawn` (a new look after each escape or death).
- `Combat.deadline` (VIs, human villagers only; 5400 → 0, −2 a frame): `Hud.beach.escape` shows `deadline / 60`.
- **Strike** (deadline 0): `Event::Hit { damage: 0, attacker == victim }` on a villager, whose anim is HIT 0x11D for 40 frames → `StrikeCall` at it. Then `Hit { damage: 7, lethal }` and BLOWN_UP 0xF3 → `StrikeBlast`, `Debris`.
- **Escape**: `Combat.freeze` > 0 (150 a human, 30 a bot, counting down). Fade the escapee's model; for a human, the radio line `lines.escape` and the music duck, then fade its view out over the last 30. `Event::Respawn` on the next villager record when it ends, then a fade in over 100 VIs.
- **Detonation**: `MatchView.cooldown` 0 → 900 (it counts down; the plunger stays held until 20) → `Detonation` ×3, `Debris`, and the `Plunger` voice by the presser's class (the player `beach::Plunger::under` that frame). Add the camera cut 0x9002 for 60 frames if the only human is a villager. Each Tedi takes `Hit { damage: 7, attacker == victim }`.
- `MatchView.plunger`: the plunger's y (392 home). Move placements 0, 4, 5 by `plunger − 392` (I: the groundwork saw all three at the same y; only prop 0 is modelled, so check they are one body). `PlungerReady` plays for a villager in trigger 0x25 while `cooldown == 0`.
- `MatchView.holders`: [sniper zone (west top), bazooka zone (east top)] holders' slots. The original shows nothing for them.

| Check (2026-10-13) | Result |
|---|---|
| `game.rs` tests | `beach_scores_and_ends` (MG death point 35 frames after hp 0, blast 88, the strike's blast at once, latched a frame later; a Tedi's none; escape point same frame, respawn on the named record; no lives lost), `beach_items` (V spd/spr: 26.25, 52.5 for 300 frames, crouch 14; bottle back in 30; a Tedi's bottle does nothing; zone item in and out), `beach_plunger` (392 → 332 in 15 frames, V wz 901-917; the hold) |
| `conker-bots` `rom.rs` `beach_script_runs_the_match` (real 0xB9 on 0x2B) | 2 human villagers + a human Tedi. The detonation scores 1 villager point and kills the Tedi with no credit. The escape: a 150-frame freeze, then respawn on `[0,1,2,11]`'s next record. The strike at 2748 = V 2772 − 24, the death point at 2788 (V 2812). Scores [2, 1]; no unknown natives, no faults |
| Server tests | `match_settings_parse` (beach: map 2b only, score 20), `beach_caps_the_tediz` (7 bots: sides S T S T S S S, classes 0x1E 3 0x14 3 0x1D 0x1F 0x1E, masks by record) |
| `MODE=beach MAP=2b BOTS=7 bots.sh 300` (1 idle human villager, port 5970) vs V bots1 (Tediz 20-7 at read 8125) | 0 faults, 0 unknown natives, 0 panics. As in bots1, one Tedi (slot 3) was on the gun all match and the other took the bazooka on the east top, then the sniper on the west. The detonation was at read 4732 (bots1 4526): +2, both Tediz killed. The idle human's strikes were at reads 2771, 5560, 8349 (V 2772, 5562). **The score was the other way round: villagers 18, Tediz 10 by read 8808, no end.** That is 16 escapes (bots1 ~5), and 7 kills by the gunner in 30 hits (bots1 ~18). The villagers' runs (each ends in a death or an escape) come at the same rate, 26 by read 8808 vs bots1's 25 by 8125; only the lethal share differs (10/26 vs 20/25). So the cause is the bot gunner's hit rate (the mounted-gun section's known gap), not the escapes or the villagers' movement. Villager bots run at 393/s, 75 % of the 524/s of Total War's bots |
| DM, TW, Colors vs HEAD (`d3c51bf` built in `target-net/head`; same pack, 7 bots, 150 s, ports 5970-5976) | Colors: event lists identical. TW: run-to-run timing splits it into the same two event lists in both builds (a HEAD run equals a new run through read 1500). DM: not checkable by read (the two new-build runs differ from each other); the unknown native `(6, 3, 1)` ×6 on 2d Deathmatch with 7 bots is HEAD's too |
| `bun run test`, `bun run check` | pass |

**Fixed (2026-10-13, later): the Tediz' kills.** The gunner was not the cause: it hit as often as bots1's (~30 MG hits).
The bazooka Tedi was. Its rockets flew level along its facing, so from the east top (y ≈ 724) they burst at y ≈ 788
and never reached villagers about 1,000 below. HEAD: 9-15 blasts, 0 kills. bots1: 16 blast hits, 8 kills. Traces are
in `../lab/research/mp-original/beach-gunner-scratch/`; notes in `beach.md` §7.4.
- **The aim (V r1):** a rocket leaves along the shooter's aim. 31 rockets matched s126+0x16C/0x170 to within 0.3°, up to 37° down. `Rocket::fire` takes `aim(f)`'s direction, and bots write the aim for 0x16 too (`Bots::scopes`).
- **The rate:** one rocket every 60 frames, not 120 (s126+0x19E = 120 VIs, C; V r1 62 frames apart).
- **The clip:** Beach's fire clip is 0x207 (`func_15065A5C`, C), which has no root motion. 0x77's root motion pushed the bot back 370 over 9 shots.
- **The blast:** Beach's outer blast is type 0x26, 3 damage (`func_151C1654`, C; V r3: a villager 6 → 3).
- **Grenades:** villagers' item 0x3E throws none (V gren: C-down held, the pull never starts, 3 kept).
- **Tests:** `a_bots_rocket_follows_its_aim` (the aim, 50/110/170, 0x207, 6 → 3); `beach_items` adds the grenade check.
- **Runs:** `MODE=beach MAP=2b BOTS=7`, 300 s, 1 idle human; deaths/escapes counted to read 8125. bots1: Tediz 20-7, 20/5.

  | Build | Start ticks | Result | Deaths / escapes |
  |---|---|---|---|
  | New | 59, 118 (the same events) | Tediz won 20-4 at read 7355 | 20 / 2 |
  | New | 178, 233 (the same events) | [11, 16] when the run stopped (read ~8970), no end | 14 / 9 |
  | HEAD | 59, 179, 237, 315 | [13, 14], [16, 15], [16, 15], [18, 10] | 13/13, 13/10, 13/10, 8/15 |

  There are 0 faults, 0 unknown natives and 0 panics.
- **vs HEAD (7 bots, 300 s; Colors 6 bots, 330 s; same start tick):**
  - DM 0x34: identical event lists. No bot drew 0x16, and HEAD also has the `(6, 3, 1)` native and bot 0's stall.
  - DM 0x30: identical to read 1383, then bot 5's rocket diverges (aimed, it hits bot 1).
  - Colors: identical to read 1257, then bot 4's rocket diverges (aimed, 60 frames apart).
  - Bots stuck in state 122 at y −2484 occur in both builds.
- **Checks:** `bun run test` passes. In `bun run check`, fmt, clippy (`-D warnings`) and web pass everywhere except `conker-client`, the client agent's work in progress (`cargo fmt` diffs in 5 files; clippy `type_complexity` at `beach.rs:340`).
- **The wire:** no protocol change (rockets are positions on the wire).
- **Client handoff:** `audio.rs`'s `fire` closure checks `w.fire.0` (0x77), so it misses Beach's 0x207. Rockets now fly pitched.

Gaps (V seen, C code, I inferred):
- **Bots:** two of the four new runs end like bots1 (Tediz 20-4); the other two stand at [11, 16] (9 escapes vs bots1's 5) (V).
- **I:** 0x207 and type 0x26 apply in Beach mode only, not in Deathmatch on 0x2B.
- **I:** the escape's teleport and the Tedi ejection are where the C points but not read. The ejection spot is by the gun's seat.
- **I:** the plunger's collision is static, so the presser stays at y 508 while it sinks (V: 448).
- **I:** `pick_target`'s start score is −1 (the real one is in compressed data).
- **I:** the strike's blast hurts no one else (beach-gunner r3: no one was within 2,500 at the strikes).
- **I:** the jingle query always answers 0, so a human's freeze is always 150.
- **I:** 0x3E's handler is skipped while crouched.
- **I:** classes at the start go by villager order (V x1 shows 1e 14 1d 1f).
- **C:** the villager cap (8, BALANCE) is not enforced.
- **I:** where the game refuses a villager's grenade (V: it does); the remake checks item 0x3E.

## War: Beach's presentation in the pack (2026-10-13)
**`conker-extract pack --map 2b` now also packs what War: Beach shows and plays: the villagers' 14 looks, Beach's two
outros, the escape countdown, the Beach strip and its STATS header, the radio, detonation and strike sounds, Beach's
lights, the explosion sprites, and a corrected music loop. Only `conker-extract`, `conker-assets`, `extract-character`'s
glTF writer, `../assets` and `../lab/audio/map2b/` changed; no rules, server or client code.** Research:
`../lab/research/mp-original/beach-presentation.md` (§ below), scratch `beach-presentation-scratch/`.

| Part | What |
|---|---|
| Looks (§5) | `villager_look(class)` (`conker-extract`, the game's `func_15083384`): classes 0x11-0x1C → model 156, mask `0x3E & !(4 << (v >> 2)) & ![0, 2, 0x20, 0x22][v & 3]` (v = class − 0x11); 0x1D-0x20 → 157, mask 0x0A (< 0x1F) or 0x14. A set bit *i* hides display list *i* (bit 0 is never set: the body always shows). Packed as `Character.looks` (class → mask) in `characters/156/character.json` (12 classes) and `157/` (4). The pack's other characters have none |
| glTF (all characters) | `extract-character` now writes one primitive per (display list, material), not per material, with `extras: {"list": n}` on each. Counts: 156 14 → 16 primitives, 157 17 → 19, 40 19 → 22, 88 16 → 18, 90 9 → 14; the others are unchanged apart from the extras. Material indices did not change (the gun belt is still material 15) |
| Outros (§4) | `OUTROS` gains `(0x2B, [(0, 4), (1, 3)])`: `maps/2b/outros.json`, team 0 (villagers) cs4 650 VIs, voice 0x163 at 200; team 1 (Tediz) cs3 1150 VIs, voice 0x161 at 40; characters 156, 157, 90; WAVs `audio/maps/2b/voice-161.wav`, `voice-163.wav` |
| HUD (§1, §3) | `hud.json` `beach`: `strip` = Total War's with texture `beach_teams` (0xD12, 16×32; team 0 the villager head `[0,16,16,16]`, team 1 the Tedi `[0,0,16,16]`), `escape` = the countdown's text (white, no shadow, centred at x 146, top 6; the text is the seconds left, 90 → 0, villagers only). `end.json` `teams.beach_won` = [FRENCHYS WIN (new `frenchys.png`, 0x855), TEDIZ WIN] |
| Sounds (§6) | `events.json` cues `plunger_ready`, `plunger` (per villager model: 156 1705/1707, 157 1701/1703, delay 10), `radio_click {end}` (0x5B0 / 0x5B1 on the voice slot), `detonation` ×3 (0x2B7, 0x2B8, 0x2B6 at their own `at`, delays 0/23/45 ticks), `debris` (0x1B9-0x1C1), `strike_call` (0x69E; and per villager its pain voice 1715-1726 on its voice slot), `strike_blast` (0x69D). `Cue` gained `at` and `delay`. `maps.2b.lines.escape` = `audio/maps/2b/voice-1a8.wav` (the escape's radio MP3, with the music-duck numbers in its note). The lamps' buzz is 2b's two packed emitters (1099-1108) |
| Music | The groundwork's 93.29 s period was wrong: song 0x4D repeats every **85.290 s** (1,878,088 samples at 22,020 Hz, by cross-correlating the 331 s south recording). `background_loop.wav` is now one period from 45.5 s of `../lab/audio/map2b/background_south.wav` (kept, with its `.log`), with a 0.5 s crossfade (seam difference 0), 7.5 MB |
| Lights, effects (§7) | `maps/2b/lights.json`: ambient (0x70, 0x8D, 0xD5), the two white lamps (size 50) and the two red exit lights (87 + 43·sin φ, φ += 0.1021 a frame), all `calm`. HUD textures for the effects: `explosion.png` (0x78A, RGBA32 32×32), `smoke.png` (0xE4A), `fire.png` (0x71F, I8), `shell.png` (0x1042, IA16 16×16); the strike's flash is `iris.png` |

**Handoff (client agent).** Types are in `conker-assets`. `looks`, `lines`, `at` and `delay` default when absent;
`Hud.beach` and `Teams.beach_won` are required, so an older pack no longer loads (re-export with `bun run pack`). The
split primitives share their material: the client's by-name lookups (`guns.rs`'s belt) already walk every primitive.
- Looks: `Character.looks: BTreeMap<u8, u8>`; on a villager's mesh, hide every primitive whose `GltfExtras` `list` bit is set in `looks[class]` (Bevy puts the primitive's extras on the primitive entity). Masks: 0x11 0x3A, 0x12 0x38, 0x13 0x1A, 0x14 0x18, 0x15 0x36, 0x16 0x34, 0x17 0x16, 0x18 0x14, 0x19 0x2E, 0x1A 0x2C, 0x1B 0x0E, 0x1C 0x0C, 0x1D/0x1E 0x0A, 0x1F/0x20 0x14.
- HUD: `Hud.beach.strip` (draw as `Hud.strip`), `Hud.beach.escape` (a `Text`: format the seconds, team 0's views only). `End.teams.beach_won[team]` in place of `team_won` on 2b.
- Sounds: `On::{PlungerReady, Plunger, RadioClick { end }, Detonation, Debris, StrikeCall, StrikeBlast}`; play at `Cue::at` when set and `Cue::delay` ticks late. `MapSounds.lines["escape"]` (`Line { file, volume, note }`) for a human villager's escape, unpositioned; duck the music as its note says (0x4650 → 0x1E0A at −0x233 a frame, back at +0x1000).
- Escape (§2): the escapee's alpha −16 a frame, then hidden; a fade to black of alpha 0x11 on its view for the last 30 ticks; on respawn, the new look and a fade in from black over 100 VIs (255 − e·255/100, e +2 a frame).
- Detonation (§7): camera cut to level camera 0x9002 for 60 frames with one viewport and player 0 a villager (`det1_camera.csv`); a shake of 8 for 100 VIs on every view (`func_151D8868 {1, 100, 8, 0xF, −1}`); explosions of size 4r+12 at the three records.
- Strike (§7): red light kind 0x600 at the target, size 86 → 9 → 38 → 25 over the 40 frames to the blast; flash = `iris.png` tinted 0xFF0000, alpha 0xF8 → 0xC2, centred at (146, 83), then black (ev3 DL 1046-1060); ground decal 0x3103 not in the pack.
- Water: node `04-2b-05@1` of `maps/2b/level.gltf`, material 50 (`m50-tex0530`): scroll its texture's T by +0.25 texel a frame.

| Check (2026-10-13) | Result |
|---|---|
| `conker-extract` `rom.rs` | `beach_outros_follow_the_oracle` (the old scratch check, now a test: lengths, cameras, actors, voices, every frame vs the oracle CSVs in `beach-presentation-scratch`); `villager_looks_hide_their_lists` (masks, primitives and triangles per list, lists drawn per class) |
| `conker-extract` `pack.rs` | looks read back (12 + 4 classes, none elsewhere); Beach cues, lines and outro voices; HUD sizes, the strip's heads vs det1 DL 318, the countdown's digits "81" (det1) and "89" (e1 DL 99) laid out as the DLs, the STATS header tiles vs cs4/cs3 |
| Textures vs the oracle's RAM | `beach_teams`, `explosion`, `smoke`: every pixel equal to the RAM bytes after undoing the odd-row interleave; `fire`, `shell`: equal to the RAM PNGs |
| Fresh export (`--out /tmp/pack-beach`, `package.json`'s flags) vs `../assets` | new: 6 HUD PNGs, `audio/characters/156/{1705,1707}`, `157/{1701,1703}`, `audio/common/{0696,1456,1457,1693,1694}`, `audio/maps/2b/voice-{161,163,1a8}.wav`; changed: `README.md`, `events.json`, `sounds.json`, `hud.json`, `end.json` (additions only), `maps/2b/{background.wav,lights.json,outros.json}`, `characters/{156,157}/character.json` (looks), all 12 `character.gltf` (extras) and `character.bin` of 156, 157, 40, 88, 90 (split primitives). Copied in: `diff -r` empty |
| `bun run test`, `bun run check` | pass (the whole workspace, with the Beach gameplay agent's work in progress) |

Gaps:
- Outros: song 0x8A (cs4 t 1), cs3's near plane 30, the actors' part masks (op 5 0x3C), props 0xFC/0xFD, the Tediz'
  attachment set 0x70, head turns (op 0x11) are not decoded.
- Not packed: the escape's positional loop 0x4A7 at (−1536, −1434, −3769); the bots' radio record (`func_15103AA0`);
  song 0x4D's channels 13/14, which follow a source at (0, −377, 8227) (4000/3000) and are not in the south recording;
  the strike's decal 0x3103; the explosions' sizes over time and the particle emitter; the sweeping ground lights.
- The lamps' flicker (behaviour 4) is not modelled (steady). The groundwork's emitters start at 1099, the research
  says 1100-1108: left as packed.
- The new background loop, radio and outro voices were not listened to.
- The research doc still points at `/tmp/beachp` (deleted) and the scratch `beach_outro_check.rs`, which
  `beach_outros_follow_the_oracle` replaces.

## Map 0x2B, the Beach, and the villagers 156/157: the pack and Deathmatch on it (2026-10-13)
**The groundwork for War: Beach (scenario 8): `conker-extract pack --map 2b` packs scene 0x2B and the villagers
(models 156 and 157), and `conker-server --map 2b` plays Deathmatch on it with the original's start records and its two
vitamin pickups; the clients load and draw it through `/info` as the other maps. The mode itself (teams of villagers
and Tediz, the escape, the gun) is not here.** Oracle runs and tools: `../lab/oracle/map2b/` (private copy
`ConkerRecomp.pack2b`, re-signed with `get-task-allow` for lldb; scenario 8 needs `CONKER_MP_OPTS=-48=2,-24=9,-23=5`:
team row 2 = 2 teams, names 9 and 5).

| Part | What |
|---|---|
| Oracle (V) | `s8_h1b15` (1 human + 15 bots, 60 s, RAM at DL 100-1500), `s8_h2b3` (2 + 3, 8 s), `spawn_lldb`. Start by slot `[1,10,2,3,11,10,0,3,1,10,2,3,11,10,0,3]`: team 0 (villagers) cycles 1, 2, 11, 0 (the north beach), team 1 (Tediz) 10, 3 (the bunker, z ≈ −4,260); with 2 humans team 0 started one place on (2, 11, 0: untraced). Masks all ones. Respawns go straight to a record the level script names (a1 = 10/3/0-2, `func_1507D1D8`), and 28 picks of record 5 by `func_15172E7C` are the lifts. Classes: Tediz 3 (117); villagers 0x12, 0x14, 0x1B, 0x1D, 0x1E, 0x1F, each drawn 156 or 157, re-drawn on respawn (p2: 0x14 → 157 at DL 1500); villagers hold 0xBE (item 0x3E). Props: 2, 3 hidden all match (+0x6E), 0, 1, 5 layer 2 (1's +0x4F is 0x51), 6, 7 the pickups (0x21); 0, 4, 5 move (y 392 → 348 by DL 1500). Pickups: 2, item 0x3E, respawn 60 VIs (MP+0xEAC), at the props' RAM positions. The mounted gun: object 16, model 138, at (−337, −387.1, −3561), +0x7A 0xBF00. AI script 0xB9 (G+0x16BC). Sound (`../lab/audio/map2b`, 2 idle humans 25 s): song 77, mute 0xFC00 (channels 0-9), volume 18000; two bubbling spots (`func_151618BC`, 0x2D's) at (2616, −236, 8004) and (−3032, −327, 8306) |
| Rules (outside this task's lanes, one match arm each, as 0x30/0x34/0x3F were added) | `spawn::DEATHMATCH_START_2B`, `BEACH_START_2B`, `starts(0x2B)` (masks all ones; the squirrels take the villagers' records); `weapon::PICKUPS_2B` (placements 6, 7 → item 0x3E), `SPOTS_2B`, `pickups(0x2B)`; `extract_scene::deathmatch_solid(0x2B)` = 0, 1, 5; `conker_bots::script(0x2B)` = 0xB9 |
| Pack | `conker-extract pack … --map 2b --background 2b=../lab/audio/map2b/background_loop.wav` (`package.json`'s `pack`). `hidden_placements(0x2B)` = 2, 3. With 2b: `VILLAGERS` 156 and 157 after `PACK_CHARACTERS` (the Tedi's rig and 764 clips, `character.json` with lifts 41 and 40; `PACK_CHARACTERS`, the `Side` order, is unchanged), the mounted gun 138 (as with 30/34). `pack_audio` takes the character entries: the villagers' clip sounds and hit voices. `map_sounds("2b")`; a sound several maps' emitters play stays in the first map's folder (0x2D's bubbles stay in `maps/2d`). README updated |
| Server, client | `--map 2b` (messages, docs); nothing else: lifts (6 kind-8 triggers to records 4-9) work as on 0x30, `gun::spawn(0x2B)` is empty (the gun is the mode's) |
| Lab | `../lab/oracle/map2b/tools/` (`map2b-run.sh`, `spawns.sh`, `map2b-audio.sh`, `audio.sh`); `../lab/screens/map2b/shots.sh` (port 5980) |

**Villager voices (ROM, `[0x12, 156/157]`):** both share the pain voices 1715-1726 (bullets, blades) and 1726-1731
(0x23, 0x21, the gun's bullet), the flame's 468-471 as everyone; each has a cry on clips 2 and 0x28E at 26 (156:
1709-1711, 157: 1712-1714). **No death voice**: the Tedi's 0x13/0x58 entries are empty in their tables.

**Background loop** (`../lab/audio/map2b/background_long.wav`, 900 s, 2 idle humans, P1 moved at read 24 to record 6
so neither hears the bubbles; the match runs to 601.8 s, then the end's music): song 77 never repeats sample for sample
(no 64-frame window recurs within ±2 s of the period), has no silent spot (quietest 0.05 s blocks: mean |x| ≈ 284),
and its loud bursts come every 93.29 s (93, 186, 279, 373, 466, 559 s; envelope correlation 0.26 at 93.29 s, 0.39 at
3 periods). `background_loop.wav` is one period from 67.5 s with a 0.5 s crossfade at its start from what follows the
cut (`tools/xfade.py`): the loop's end runs into its start as the recording does (seam difference 0), 8.2 MB.

| Check (2026-10-13) | Result |
|---|---|
| `conker-extract` `pack.rs` (ROM) | 0x2B reads back (collision less hidden 2, 3 with only 0, 1, 5 solid = "collision flags, not hidden, not a pickup", 12 spawns, the 2 pickups on `SPOTS_2B`, nav, triggers, AI 0xB9, glTF); the villagers: lifts 41/40, the Tedi's clips, glTF animations by name, `pack.json` characters; audio: 2b's two emitters = 0x2D's, 1099 stays `maps/2d/1099.wav`, sequence 77 note, the villagers' pain voices and cry, no death voice |
| Fresh export (`bun run pack --out /tmp/pack-2b`) vs `../assets` | additions only: `maps/2b/` (11 files + textures), `characters/156`, `157`, `audio/characters/156`, `157` (3 WAVs each), `audio/common/1715-1731.wav`, `audio/maps/2b/background.wav`; `events.json`, `sounds.json` gained entries only, `pack.json` lists grew, `README.md` the new template. Copied in: `diff -r` empty |
| `MAP=2b smoke.sh wt ws ws` (ports 5982, 5984; one on `/tmp/pack-try`, two on `../assets`) | 16/16 each. One client per run took a lift (t=373 to record 4, t=488 to record 5): its prediction error at that tick is the server's placement (2,350 / 2,544, 1 rollback), 0 for the others |
| Bots (screens run, 3 bots 90 s) | 0 faults; script 0xB9 calls natives the remake lacks: `(5, 2, 170)`, `(5, 3, 30)`, `(5, 15, 6)`, `(6, 2, 39)` (Beach's) |
| Screens (`../lab/screens/map2b/shots.sh`, read) | `own` (a squirrel at record 11 on the west boardwalk, cliff and sea), `beach` (the north beach from the sea: the two boardwalks, the sand, the hills, villager-side bots), `overview` (the island from the south: the Tediz' bunkers in front, the beach behind), `mg` (the bunker with the gun's slit and Tediz inside; no gun drawn), `rec3` (the respawn camera of record 3: three Tediz in the bunker, the red hatch, the path north), `rec5`, `rec4` (the empty pillboxes on the cliffs, the lifts' targets). The oracle draws nothing (null renderer): no frame to compare |
| `bun run test`, `bun run check` | pass |

Gaps:
- **The mode** (War: Beach: teams, the escape, the respawns onto named records, the MG on 0x2B, the villagers as a side,
  item 0x3E's rules, its 60-VI pickup respawn, the natives above) is the next agents'. Until then a pickup of 0x3E
  holds an id the rules don't know (no fire, no panic, as 0x37 was), and Deathmatch on 2b has no weapon pickups.
- The client draws no villager (no `Side` uses 156/157); they are only in the pack.
- Placements 0, 4, 5 (at (−1468, 392, 2348)) sink 44 in the game during the match; static here. Placement 1's
  `+0x4F` 0x51 is the first 0x40 prop: whether the line query treats it differently is unread.
- The villager voice cries (clip 2, 0x28E) and the background loop were not listened to.
**The client now draws the mounted machine guns on maps 30 and 34: the gun turned and pitched as the match has it,
its fire loop with the belt of rounds scrolling (no flash on the model, as in the original). The gunner sits on the seat
(the jump on plays at 1.3). The gunner's own first-person view has the gun's sight, its heading and elevation tapes,
the zoom bar, the two handles that kick and light up on the side that fired, and the muzzle flash. The guns' sounds
play. No rules, server or bot change (the pack: Fixed, below).** Research: `../lab/research/mp-original/mounted-gun.md` §3, §4.4, §5.3,
and the oracle's one-view DLs, now kept in `mounted-gun-scratch/one-view/` (DLs 125, 221, 223, 225 and 240).

| Part | What |
|---|---|
| `guns.rs` (new) | One model 138 per `MatchView.guns`, at scale 0.5, turned `model_yaw(yaw)`. After the clips, `joint_04` gets `R_x(pitch)` (positive down). In state 4, `pair_0000_anim_0001` loops (12 ticks: the glTF's last key + 1 tick). In any other state the joints go back to their rest poses. While firing, the belt's own copy of material 15 scrolls V by +2.5/16 a frame on map 30 and +1/4 on 34 (Fixed, below). `func_15062FC0` moves T, not S as the pack's handoff said: it moves the belt's 12 vertices' T by −40/32 texel a VI, and the belt's sampler repeats along T (C). No flash on the model (the original draws none: C, only the gunner's overlay; V, no flash triangles in DLs 221-225); a placeholder sphere was tried and removed. The local gunner's gun turns with its predicted facing and `scope_pitch` and uses its own `Mount.state`. It is hidden while its sight is up (free camera off) |
| `lib.rs` | `tick_anims`: `JUMP` plays at 1.3 while the player's mount is in `gun::LAUNCH` (the local player from `Fighting`; the others from the `GunView` they man) |
| `hud.rs` | `Shown.gun: Option<Sight>` replaces the scope and the reticle while the scope is the gun's (`scope.weapon == GUN`). The weapon icon goes, because 0x1B has none (V: no icon in the one-view DLs). The handles draw over their lit pictures (`lit` at z 0.2, the handle at 255 − `lit` at z 0.3), dropped by the kick. Then the flash (z 0.4), and the zoom bar centred on y 108. The sight and its tapes go over the rest. The tapes start `per_degree` × the heading (`scope.yaw` + 90, obj+0x40) and the elevation (−`scope.pitch`) in, in 1/32 texels and wrapped within one picture. Their textures now repeat. `gun_fire`: each barrel's last shot is the tick this client saw the count rise. It flashes from that frame, with size, alpha and life diced from the count, and kicks its side's handle (`flash_alpha`, `kick`: Fixed, below) |
| `audio.rs` | New `Owner::Gun(k)`. A `GunView.state` change stops that gun's loops and plays the `On::Gun { from, to }` cues at the gun. `trigger` falling plays `GunStop` at the gunner. A seated player's `Seen.weapon` is `GUN`, so `On::Firing { 0x1B }` (the squirrel's battle cry) holds on 0x1A4. This also stops the held weapon's scope sound from playing when the gun's scope opens. For the local gunner: the zoom loops while it moves, plays step 1 when it stops, then step 2 16 ticks later; step 2's loop is cut after 3 ticks, as its 0.1 s envelope would. The aim (yaw target × pitch target, as the pack's note) loops from its 5th moving tick and plays step 1 when it stops. Leaving the gun stops these loops |
| Lab | `../lab/screens/guns/guns.sh mount\|heat\|map34\|flash N\|belt34 N` (port 5986, or `PORT`), with `gunner.txt` |

| Check (2026-10-12) | Result |
|---|---|
| `hud.rs` `gun_matches_the_originals_draws` | Every gun draw of one-view DLs 125, 221, 223, 225 and 240 is one of ours: rect within 0.25 px, colour (handles and flash: alpha), mirroring, and tape texels modulo a picture (heading 108° → −48; 177.19° → −78.72; 40° up → −17.75). The draws are the 4 handle pictures, 2 bars, 9 sight pictures, the tapes, barrel 0's flash (right) and barrel 1's (mirrored, left). The handles: rest, `kick(k)` on the right then the left, and 240's right at rest lit 1; 225's flash at alpha 0 |
| `hud.rs` `gun_shots_flash_and_kick_their_side` | A shot flashes from its own frame (size and alpha in range) and is gone 3 frames on. Its side's handle follows `kick` (barrel 0 the right, barrel 1 the left) |
| `guns.rs` `the_barrels_pose_as_the_rules_shoot` | For the glTF's `joint_04`/`05`/`07` turned, scaled, and pitched at −60..50 at 4 yaws, each barrel's (±13, 0, 200) equals `gun::muzzle` to 0.05. Barrel 0 is `joint_05`: the model's +x, the gun's −side (the gunner's left). A positive pitch looks down |
| Screens (read) | `own-idle` (gun A, read 90: the sight, tapes, handles at rest, zoom bar at 0, no weapon icon). `own-firing`/`-b` (the zoom bar longer after C-up; bullets out; a handle down and lit on the side that fired: the right for barrel 0, the left for barrel 1). `own-overheated` (460, state 0xB: handles at rest). `mounted` (free camera: the squirrel on the seat in 0x1A4, the barrels pitched up). `dismount` (527: the leap off). `idle-a`, `idle-b` (640, after the dismount: rest pose, belt, level barrels, each on its pad). `idle-34`. `own-flash` (`flash` mode, read 168: barrel 1's flash mirrored on the left over the handles). |
| `bun run test`, `bun run check`, wasm32 clippy of conker-client | All pass. The Deathmatch, Total War and Colors client tests are unchanged |

**Not done or I:**
- **Barrel sides (C/V):** barrel 0 = `joint_05` shoots from the gunner's left (V e1: barrel 1's bullets come from +side). The original's HUD draws barrel 0's flash on the right and kicks the right handle (C `func_150FB8C0`: +59 for barrel 0; V: the flipped flash in DL 223). Both are kept as the original has them.
- **HUD:** the flash texture's faint glow (alpha 1-25) is cut at the quad's edge, and Bevy's alpha blend shows it as a pale box (`own-flash`, `flash-a-167`; not done: the N64 blends in display space). Shots reach the local gunner a round trip late (server-only fire). A handle's lit on its shot's frame is 0, not the 1 a kick 6-16 frames back leaves (I). The original's 2-player HUD updates per view (twice a frame): only the one-view timing is modelled.
- **Model:** the rest pose at rest is I. The pack's handoff says the belt scrolls S; the code scrolls T (C, above). The local gunner's gun is hidden in its own view (I: the original's 3-D gun in that view is not read).
- **Sound:** the aim sound keys on yaw target × pitch target, so a level turn makes no sound (C, per the pack's note; not V). `gun_stop`'s replay is not played (V gcQ: the same sound again, unpositioned, 15 frames on; it needs the first pick carried by `Later`). A Tedi hit by the gun makes no sound, as in V gcS/gcK: character 117's voice table has no reaction 0x31 word (C), so it has no cue, as for weapon 0x23. The caveman (136) has one, but map 0x3F has no gun.

**Fixed (2026-10-12):**
- **Belt on 0x34 (C/V):** `func_15062D10` scrolls the belt's 3 mip tiles 4 texels of 16 a frame (V gcA DLs 130-132, 190), so `guns.rs` scrolls V by +1/4 a frame on 34. The 0x30 sign was wrong and is now +2.5/16: the rounds feed toward the receiver. Tested by `the_belt_feeds_the_gun`. Screens: `belt-34-150`/`-152` (`belt34 150`): the rounds half a round on.
- **FOV (C/V):** every scope's view is the view's angles plus its zoom (`func_15126378` → `func_1510B128`; V gcA projections: 50° unzoomed, 34° at zoom −16). `render::fov` sets the follow camera's FOV for all scopes. Tested by `a_scope_narrows_the_view_by_its_zoom`. Screens: `flash-a-89` against `flash-a-167`: the towers are 1.3× larger.
- **Flash and kicks (C/V):** `flash_alpha` (`func_15154C90`): the life is 5-8 VIs, it loses 2 a frame from the shot's own, and the alpha is at most 0x55 a VI left. `kick` (`func_151CB970`): the drop is 21·sin of a phase stepping 0.505 until π, and lit is 255 − v, where v closes 0.676 of its gap a frame. A new shot restarts its side, which covers V 240's overlap. The handles also slide down 3 a degree of mean zoom, up to 75 (`func_151CC1D4`, C). Tested against DLs 221/223/225/240 by `gun_kicks_and_flashes_fade_as_the_original` and `gun_matches_the_originals_draws`. Screens: `flash-a-167`/`168`/`169` (`flash 167..169`): full, fainter, gone.
- **Zoom bar:** 16 frames after the zoom stops, the bar turns the pack's new `bar_flash` (red, 0x800AAC80) and fades back to `bar_colour` over 35 frames (`func_151CBC60`, `func_151CC2BC`). Tested against V gcA ff2b00e2 and ff0e00f5 by `the_zoom_bar_flashes_red_and_fades`.
- **MG hit sound (V):** it is reaction 0x31's voice, the same as 0x23's, but only when the hit does not kill (V gcQ: 887 at 24000; gcQK: nothing on the kill). The pack adds `On::Hit { 0x1B, lethal: Some(false) }` per character. `hit_weapon` names 0x19C hits the nearest of a 0x23 holder or a seated gunner. Tested by `hits_name_their_weapon`.

## Mounted guns: the gameplay (2026-10-12)
**The mounted machine guns now work on the server: two on map 30 (Deathmatch and War: Colors) and one on 34. A player
who touches a free gun jumps onto its seat and turns the gun with the gun's own scope. Z fires the two barrels and
overheats the gun after 300 frames; B stops it; A jumps off. Dying throws the gunner off. Bots mount, aim, hold fire
and overheat as in the oracle's b30. In Colors the guns come back after each capture. The client draws none of it yet.**
Research: `../lab/research/mp-original/mounted-gun.md` (§2-§5; runs m2, e1, e2, b30, b34).

| Part | What |
|---|---|
| Rules | New `gun.rs`: `places`/`spawn` per scene; `Gun` (server-only, `Match.guns`: yaw, pitch, fire timer, barrel, shots, the dismount's wait); `Mount` (`Fighter.gun`, predicted: state, timer, budget 600, lock 150, trigger; `step(z, b)`); `seat`, `muzzle`, `barrel_dir`, `touches`. `game.rs` covers the whole cycle. **Touch:** state 1, no gunner, alive, no stun, scope or flag, and the player's body overlapping the gun's (`gun::body`; fixed below). **Launch:** y-vel 44, gravity 3.2, immune 255, homing on the seat's xz. **Seat:** taken once falling below the origin + 50. **Seated:** the gun's yaw follows the scope; Z/B/A. **Fire:** the timer drops 1 a frame and fires a shot each time it goes below 0, then gains 4 + 4·rand; the barrels alternate. A human's shots go to the point 333 along the scope from `gun_eye`; a bot's go along the barrel, toed in. **A:** a leap to yaw + 0x8000 + 1024 with immune 20, and the gun stays in state 5 for 30 frames. **Hits:** the gunner stays seated unless the hit kills (reaction 0x2E, 0x17E) or knocks up faster than 30. `Effect::Gun(k)`: the capture's recreate. `damage::MG_BULLET` (entry 0x2C, reaction 0x31: 2 HP, 0x19C for 37 frames). `Bullet::aimed` (3-D, speed 252) |
| Sim | `movement::{hop, sit, leap}` (the launch's homing, the seat, the jump back; `leap` also sets the facing to angle − 0x1200) |
| Camera | Weapon `GUN` (0x1B). The scope opens by itself on the seat, with no in-hand test (`attached`). Rates 19 (yaw) and 31 (pitch), shares 0.25/0.5 clamped ±3; pitch clamped to [−60, 50] with no yaw limit; `spring_angle(6, 8)`. Also `gun_eye(origin, yaw, pitch)` and `Scope::steer(heading, stick)` |
| Server | `PROTOCOL_ID` 17. The guns spawn at the match start, `MatchView.guns` is published every tick, and a `GUN t=… gun k state a -> b slot s` line is logged on each change |
| Bots | `obj` creating uid 0x13/0x14 → `Effect::Gun`. Query 0xBC on 0x2012/0x2013 returns the gunner's participant + 100. The 0x1B attack (`func_1508CAD8`): the stick is 8 a degree off the aim (±90 yaw, ±30 pitch); Z once within about 4° and seen; R always; hold 1, so Z stays held while fighting. A seated shooter's shots make no noise (see I) |

**Handoff for the client:**
- **`MatchView.guns: Vec<GunView>`**, in `gun::places(scene)` order (0x30: A south, B north; 0x34: one); empty on other maps.
  - `pos: [f32; 3]`: the origin (model 138 at 0.5).
  - `yaw: u16`: the turn (forward = (cos a, −sin a) in x, z).
  - `pitch: f32`: the degrees `joint_04` pitches down (positive down, ≈ −60..50 while manned, then decaying to 0 by 1/8 a frame).
  - `state: u8`: 1 free, 2 a player jumping on, 3 manned, 4 firing, 5 the 30 frames after A, 0xB stopped (B or overheat, 25 frames). The pack's `On::Gun { from, to }` cues key on these changes. Play the fire clip, the belt scroll and the fire loop while it is 4.
  - `gunner: Option<u16>`: the slot, from the jump (state 2) until A or death.
  - `shots: u16`: counts every bullet. On each new one, flash `barrel` (0 = `joint_05` = `gun::muzzle(.., 0)`, on the −(sin a, cos a) side; 1 = `joint_07`). The pack draws barrel 0's HUD flash at x 205 (right). Check which side the world shows it on.
  - `trigger: bool`: on from Z until Z is let go with the lock off. Its fall is the release (`On::GunStop`, 0x618-0x61A, at the gunner).
- **The gunner** (predicted): `Fighter.gun: Option<gun::Mount>` (`gun` index, `at` origin, `state`, `budget`, `lock`, `trigger`) and `Fighter::seated()`. `Fighter::camera_view` gives weapon `conker_camera::GUN`, `attached: true` and `gun: Some(origin)`, so the scope opens by itself. The first-person eye is `conker_camera::gun_eye(origin, yaw, pitch)`, from the scope's yaw/pitch. The seated frame turns with `body.scope_yaw`/`scope_pitch` from `take_camera`. Draw the local gunner's gun from the scope, not from the lagging `GunView` (I). Use the gun HUD (`Hud.gun`) instead of the scope's.
- **Clips:**
  - Launch: `JUMP` 0x32 for 19 frames (the game plays it at 1.3; `request` gives 1.0), then `FALL` 0x38 until the seat.
  - Seated: 0x1A1 (`anim::GUN_SEATED`), and 0x1A4 (`GUN_FIRING`) while the gunner's state is 4.
  - Hits: an MG bullet plays 0x19C for 37 frames. Other non-lethal hits play their own hit clip on the seat (V e2: 0x11D). A death on the seat is 0x17E.
  - The jump off is `JUMP`. Seat position: `gun::seat(origin, yaw, gun::pitch10(pitch))`.
  - Sounds per the pack's handoff below (no sound per shot).

| Check (2026-10-12) | Result |
|---|---|
| `gun.rs` `m2_trigger` | m2's Z/B timeline row for row. Carry-over: Z 101-110 → state 4 to 130. Burst 171-230 → 3 at 250, budget 460. B at 291 → 0xB for 25 frames, lock 150. Overheat at 795 (budget 2 → 0xB, 25 frames, lock 0 at 870, the trigger falls then) |
| `rom.rs` `mounted_guns_m2_and_e1` | m2: launch at 61 (y-vel 44, 0x32); top at 72; 0x38 at 80; seated at 83 at (166.91, 616.14, 6000.88); 0x1A1 at 84. e1: seated at 66; Z 101-160 gives 13 shots (V 13); the hit at 101 is 0x19C on P2, 6 → 4 HP; knocked 143.4 (V 140.7) |
| `game.rs` `a_mounted_gun_takes_one_gunner` | a manned gun refuses a second player; 30 frames of Z give 4-8 shots; A gives state 5 for 30 frames, and then the other player gets on; a lethal hit detaches (gun FREE, 0x17E) |
| camera `the_gun_scope_opens_on_the_seat_and_clamps_its_pitch` | opens without R; pitch clamped at −60/50; half stick at the half rates |
| `bots.sh` (BOTS=5, 150 s) | `MAP=34`: 50 hits / 28 pickups (HEAD 50/28); `MAP=30`: 34/18 (HEAD 34/18); both event lists identical to HEAD's by read. No bot came near a gun in 150 s (b34's first mount was at read 1647) |
| `bots.sh` `MAP=30 BOTS=7` 300 s | a bot jumps on gun A at 943, seated at 965 (22 frames; b30: 4895 → 4917), fires from 2910 (two 2-HP hits), is killed on it at 3217 and the gun goes 4 → 1 with no state 5 (V b30/b34) |
| `bots.sh` `MODE=colors MAP=30 BOTS=6` 330 s | the gunner (slot 3) is on gun A at 664, and from 938 runs b30's cycle exactly: 4 for 299 frames, 0xB for 25, 3 for 50, 4 again (22 overheats; 7 hits). The capture at 3529 recreates both guns, and it is back on at 3980. Takes 2258/3162, capture 3439 (last runs 2171/3007, 3351/6630) |
| Total War (`MAP=2d`) | unchanged: with the same start-tick parity as HEAD's run, the events are identical (22 hits, 18 pickups). Runs whose match starts on the other parity give 26/22 with or without this work |
| `bun run test`, `bun run check`, `cargo test -p conker-camera`, wasm32 clippy of conker-client | pass |
| `bots.sh` at einstein, 300 s (later): maps 30 and 34 with 7 bots, map 30 with 6 | Hits from the seat per 100 frames of fire, pooled: 0.29 (24 in 8372) vs HEAD's 0.14 (17 in 11958). The counts are small. At the default skill, bots still mount and fire (map 30: 2 mounts, 10 hits; map 34: 2 mounts, 4 hits) |
| Total War, Deathmatch (5 bots, 150 s), Colors (later) | **Total War:** 8 hits, 10 pickups, identical to HEAD's event for event. **Deathmatch, map 34:** 50/28 (HEAD 49/28, 32/33). **Deathmatch, map 30:** 38/18 and 33/19 (HEAD 46/23, 41/16, 48/20). Map 30's events change with the match's start tick even between two runs of one build, and the 33/19 run had no bot within 120 of a gun. **Colors:** takes/captures 1/1 and 8/2 (HEAD 4/1) |

**Fixed (2026-10-12, later):**
- **Body:** the gun is a solid body for every mover (`gun::body` joins `player()`'s push list). It has radius 70, lift 70 and y-scale 0.708 (C `func_1505D6F0` → `func_1505D408`). The player moves 0.4 of the overlap and the gun never moves; the gunner and a launching player are exempt.
- **Touch:** the touch is that same overlap test (C `func_15059140`). For a Tedi it reaches about 89.5 in xz at the gun's floor, not 44 (V t1: 91 no, 88 yes). Test: `rom.rs` `mounted_gun_bodies` against p1, w1 and t1, within 0.01.
- **Lead:** at skill ≥ 3 a bot adds the target's last-frame move × distance × factor / DT to its aim (C `func_1508CAD8`). The factor is 0.016 for 0x1B (the gun), 0x18 and 0x41; 0.0248 for 0x14 and 0x3F; 0.032 for 0x16. V lead/l1 covers 0x14 and 0x16 (skill 4, 145 of 146 aim points to 1e-4). Test: `skill_3_and_4_lead_a_moving_target`.
- **Seat:** the whole gun tilts about its origin by a sixteenth of the pitch, floored to 32 angle units, and `seat` follows it (V m2 within 0.02; e1).

**Not done or I:**
- **Seat and launch:** the launch moves 0.2 of the way to the origin each frame and keeps no speed of its own. The original homes on the seat once the gun has been drawn, and keeps its xz speed, which drops 2.5 a frame (C `func_1505327C`; V t1, b30). The leap lacks its first frame (17.28 = 15 + the body's push of 2.29) and the brake of 0.875 a frame (V m2 1600-1615); its y-velocity row is not explained. `muzzle`, `gun_eye` and the client's model ignore the tilt (joint 4 should take the pitch less the tilt; I).
- **Prediction:** the client's `known_match` has no guns, so a local player walking into a gun is pushed only by the server's corrections (fix: `guns: gun::spawn(scene)`).
- **Bots:** on a respawn or warp the lead sees one huge move (I: the original's obj+0x2C there is not read). A seated shooter's shots are not heard: in 0xBB/0xBD the noise state presses A off the seat, but b34's bot fired 144 frames on (V; no noise call found in `func_150FC614`'s chain, so I).
- **Server and client:** the fire is server-only, so the gunner's shot counts reach the client late, and `Fighter.shots` may cause rollbacks.
- **Hits and resets:** e1's knock angle (V 210.4°) vs the bullet direction's (~220°). A hurt gunner's trigger runs down with no buttons. `Effect::Gun` drops the gunner where the seat was (I).
- **Logs:** the capture's recreate isn't in the `GUN` log (see `COLORS … Gun(k)`).
- **Runs:** `bots.sh` binds port + 1 for tokens, so parallel runs need ports two apart (5970, 5972, …).

## Mounted guns: the pack (2026-10-12)
**The pack now has what the client needs for the mounted machine guns on maps 30 and 34: the gun model with its fire
loop, the gunner's sight, tapes, handles, zoom bar and muzzle flash, and the guns' sounds. Nothing uses them yet.**
Research: `../lab/research/mp-original/mounted-gun.md` (§3, §4.4, §5.2) and the oracle's one-view DLs and sound calls.

| Part | What |
|---|---|
| Extract | `MOUNTED_GUN` = 138, packed with map 30 or 34 as `characters/138` (12 joints; one glTF animation, `pair_0000_anim_0001` = logical clip 1, the 12-tick fire loop; no `character.json`). `hud.rs`: 8 textures (`gun_handle` 0x3D1 and `gun_handle_lit` 0x3D2 IA16 64×32, `gun_flash` 0x3D0 RGBA32 32×32, `gun_corner` 0x1127 I8 16×16, `gun_tick_side` 0x1198 and `gun_tick_end` 0x1197 I4 16×16, `gun_tape_heading` 0xFF3 I8 16×8, `gun_tape_elevation` 0xFF4 I8 8×16) and `gun()`, the one-view layout (`func_151CC840`, `func_151CC524`, `func_151CB5FC`, `func_151C9F38`, `func_150FB8C0`), rect for rect the oracle's DLs. `audio.rs` `gun_cues()`: 12 cues, 12 WAVs. README template updated |
| Assets | `hud::{Gun, Sprite, Tape, Flash}`, `Hud.gun`; `On::{Gun { from, to }, GunStop, GunZoom { step }, GunAim { step }}` |

**Handoff for the client:**
- **Model:** `characters/138/character.gltf`, drawn at scale 0.5 and turned to the gun's yaw.
  - `joint_03`: the seat.
  - `joint_04`: the pivot; put the pitch here. The eye is `joint_04` × (0, 116, 130).
  - `joint_05` and `joint_07`: the barrels. Their muzzles are at (13, 0, 200) and (−13, 0, 200), local to each barrel joint.
  - `joint_06` and `joint_08`: the barrel tips.
  - Play `pair_0000_anim_0001` while the gun fires (state 4).
  - The belt is `textures/character-15.png` (material 15). On 0x30, while the clip plays, scroll its S by −40 × the frame's VIs (I: 1/32 texel).
- **HUD:** `Hud.gun` (`hud.json` `gun`), anchored centre-middle, drawn with normal alpha blending; a grey picture is its grey × the colour.
  - `sight: Vec<Sprite>`, drawn in `colour`.
  - `tapes: [Tape; 2]`, drawn in `tape_colour`. [0] is the heading, [1] the elevation; each starts `per_degree` × the angle in degrees texels in.
  - `handles: [Sprite; 2]`, plus `lit: Image`, which crossfades with them while the gun fires. At rest the handle's alpha is 255 and `lit`'s is 0. On a shot, on the side that fired, `lit` goes to 0x53 and the handle to 0xAC, then `lit` fades (V: 0x09, then 0x01).
  - The zoom bar works like `Scope`'s: `back`, `back_colour`, `bar_x`, `bar_width`, `bar_half`, `bar_colour`.
  - `flash: Flash`: barrel 0 at `centre`, barrel 1 mirrored; its size, alpha and life are random within their ranges. Barrel 0 is `joint_05`, at +x: its flash is at x 205, on the right, and the right handle drops. Check that +x shows on the right in the remake.
  - A `Sprite` is `{ rect, image, flip: [x, y] }`.
- **Sounds:** `events.json` cues named `gun`, `gun_stop`, `gun_zoom` and `gun_aim`.
  - `On::Gun { from, to }` plays at the gun, on the gun's own channels. A looping sound plays until the gun's state changes again; that change is what stops 0x60E on B:
    - 3→4: the 0x616 loop on A and 0x60E on B;
    - 4→3 and 4→11: 0x617 on A;
    - 11→3: 0x61B on B.
  - `On::GunStop` plays at the gunner.
  - `On::GunZoom { step: 0 | 1 | 2 }` and `On::GunAim { step: 0 | 1 }` are heard only by the gunner (`own`).
  - Character 128 also has `On::Firing { weapon: 0x1B, clips: [0x1A4, 0x350] }`, the squirrel gunner's battle cry.
  - There is no sound per shot.
- **Player clips:** 0x32 (launch), 0x1A1 (seated) and 0x1A4 (seated firing) are in 128, 117 and 136. They are not in 135, 40, 88 or 90, which are outro actors only and never gunners.

| Check (2026-10-12) | Result |
|---|---|
| `tests/rom.rs` `mounted_gun_model` | 12 joints; the j3/j4/j5/j7 translations and the j3-j8 parents; one playable clip (pair 0, logical [1], 12 ticks) |
| `tests/pack.rs` `pack_audio_reads_back` | `Audio::load` reads back the gun cues (3→4: 0x616; `gun_stop`: 0x618-0x61A; `gun_zoom` 2: 0x622), and each new WAV is the bank's sound, sample for sample |
| `tests/pack.rs` `pack_hud_reads_back` | the gun textures' sizes (every PNG's size is checked against `hud.json`), 9 sight sprites, the handles at x 9 and 185, the flash centre; `Hud` round trip |
| Pack | the task's command to `/tmp/pack-mg`, copied into `../assets`, `diff -r` empty. Additions only: characters/138, 12 WAVs (1550, 1558-1565, 1568-1570), 8 `hud/gun_*.png`, and gun entries in events/sounds/hud.json; in pack.json, 138 is appended to `characters`; in the README, two table rows are extended and new text added |
| PNGs (seen) | the 8 HUD PNGs equal the oracle's RAM textures pixel for pixel; `character-15.png` is the belt of rounds |
| `cargo test -p conker-extract -p conker-assets` | pass |
| `bun run check` | clippy (every crate) and the web check pass. `cargo fmt --check` fails only in `crates/conker-camera/src/lib.rs`, the rules agent's work in progress |

**Not done or I:**
- **HUD:** the handle recoil (the side that fired drops 10-21 pixels) and the curve of `lit`'s fade; the flash's fade curve (V: alpha 0xF9, then 0 a frame later); the 2-player sight (I8 64×64), since the remake has no split screen.
- **Model:** the belt scroll on 0x34 (tile scrolls 0x253/0x255/0x257); model 138 was not rendered (no renderer here), only checked by its glTF numbers and textures.
- **Sounds:** `gun_stop`'s unpositioned replay 15 frames later is not exported; 0x622's sample loops but its envelope fades in 0.1 s and nothing stops it; when B stops the gun, no 0x617 was seen (once).

## War: Colors, part 3: the client (2026-10-12)
**The client now draws War: Colors. The two flags stand at home, lie where they were let go of, and hang in the
carrier's hand with the cloth waving. The HUD shows the captures in the strip, the blinking carried-flag icons, the
flags on the radar and the fade around each capture. A flag being taken plays its sound. The STATS use the strip
with the captures.** Research: `../lab/research/mp-original/colors.md`.

| Part | What |
|---|---|
| Extract | With map 30, the pack has 01:169 (red) and 01:168 (green) as characters (9 joints, 4 cloth clips: glTF animation n is pair n). `hud.json`: `flags` (`teams.png` row 16: the red at (236,196), the green at (40,196), centre-bottom) and the radar's white `flag_colour` (obj+0x328 = 5). `events.json`: `flag` cues: squirrel 0x5A3 / bot 0x5A4, Tedi 0x642 / bot 0x643, volumes 30000/32700 (script 0xBB's obj mode 4). README template updated |
| Assets | `Hud.flags`, `Radar.flag_colour`, `On::Flag { flag, bot }` |
| Client | New `flags.rs`. Each standing flag is drawn at half scale. At home it faces 0x3E00 (177.19°); a dropped flag keeps its last carrier's turn. A carrier's flag hangs under its `joint_09` (the right hand = rules' `HELD`). The pole stays upright because the carry clips leave the root unturned. Cloth clip pair: 3 standing, 0 carried, 1 walking (0x1B0), 2 running (0x1B1). Clip lengths are +1 tick, and the flags hide during outros. `hud.rs`: the strip shows the team scores S with the heads (as at `func_151E9D18`). The carried icons are shown when `vi & 31 >= 11`, and the carrier's radar blip only in the other phase. Flags not carried are white blips. War team colours are used in both War modes. The capture fade is `2·frames·255/VIs` from the tick the client sees `MatchView.fade` change (not the value already there when it joins: no fade from black on a late join), black over the whole view including the HUD, in phase 0 only. `audio.rs`: a rising `Combat.flag` plays the `flag` cue |
| Lab | `../lab/screens/colors/flags.sh live\|capture` (port 5984; uses `--spawn`, `--warp` and `--view`) |

| Check (2026-10-12) | Result |
|---|---|
| `hud.rs` `colors_matches_the_originals_draws` | e0 DL 360 / e1 DL 325: head rects, the S digits at 215/83, the red icon (236,196) and green icon (40,196). The blink is on for 21 of 32 VIs; the fade values are sampled |
| `flags.rs` `flags_stand_and_wave_as_the_game` | the home yaw is 177.19° ± 0.01; a dropped flag keeps its last turn; the cloth pairs match by clip |
| Pack | the task's command to `/tmp/pack-cc`, copied into `../assets`, `diff -r` empty. Only additions: characters/168 and /169, 4 WAVs, and `flag` entries in events/sounds/hud/pack.json plus the README |
| `bun run test`, `bun run check`, wasm32 clippy of conker-client | pass, run on the tree with the bots agent's in-progress work |
| Fade timing | the server fades at reads 461 (out, 60 VIs) and 490 (in, 100 VIs); clients see them at 464-465 and 493. Mean frame brightness: 0.30 before, 0.17 at 485, 0.03 at 497, 0.21 at 525. The clock darkens with it |
| Screens (read) | `flag-home` (the green flag upright at home), `carrier` (the pole upright in the hand), `tedi` (the red banner held aloft, the icon off and the carrier's blip on), `carrier-hud-a/b` (the icon on/off), `radar` (white flag blip), `scored-a/b` (the strip at Tediz 0, squirrels 1, the red icon during the plant), `cheer`, `lose`, `fade-out/-black/-in`, `stats` (SQUIRRELS WIN, the strip at 0 and 2) |

Not done or I: the cloth's clip rate (a tick a frame); whether the fade covers the HUD; Bevy blends in linear light, so
partial fades look lighter; the fade starts at the tick the client sees it (≈3 ticks late), and a client that joins
mid-fade misses it; the capture jingles (0x7662/0x7664, 0x7657/0x7659) are not played; the flag sound's 3rd argument
is not read; the home flag's "S" reads mirrored from the approach side (not checked against the original); after the
final capture the rules still fade and respawn during the end phase (rules lane; the client draws the fade in phase 0
only); split screen.

## War: Colors, part 2: the victory outros (2026-10-12)
**When War: Colors ends, the winning team's cutscene now plays before the STATS: cs1 (0x30/1, 555 frames) if the
squirrels win, cs5 (0x30/5, 550 frames) if the Tediz win, none on a DRAW.**
- cs1: an officer (135) walks and turns through 8 clips, with 2 squirrel soldiers (88) and 2 prisoner Tediz (90). It
  has 4 camera cuts, 5 bubble pages in a bubble that moves 3 times, and voice 0xD0.
- cs5: 6 Tediz (90) and 2 tanks (40) march along curved paths past 3 cameras, and 1 Tedi stands.

Total War's outros run on the same general code. Their `maps/2d/outros.json` is byte-identical. Research:
`../lab/research/mp-original/colors-outro.md`.

| Part | What |
|---|---|
| Extract | `outro.rs` decodes `[6, 0x2D, 2/4]` and `[6, 0x30, 1/5]` with one decoder. It reads every camera (op 3 cuts) and every kind-2 actor. Actors get multi-point paths (`path_at`: Catmull-Rom by f32 arc length, as the game) and ch7 yaw. Op 2 clip sequences (`plays`) can play `once`, which holds the last tick. It also reads the s16 script start t, an optional voice and subtitles, and the bubble's `moves` (cs1: right, left, turned, right; quads and text from the DLs). Characters 135/40/88/90 carry every clip their outros play. `audio/maps/30/voice-d0.wav`. README template updated |
| Assets | `Outro`: `cuts`, `cast`, `Actor.{once,path,along,yaw,plays}`, `Play`, `Place`, `Subtitles.moves`; `voice`/`subtitles` are `Option`. Empty fields are skipped, so TW's JSON is unchanged. `Outro::view/camera/actors/fade`, `Actor::place/facing/clip`, `Subtitles::place`. The fade is now the game's integer formula (`func_1517EF00`): 255 − e·255/d |
| Client | `outro.rs`: `team_mode(m)` = `total_war \|\| colors` gates the outro; the winner comes from `team_result` (the score first in Colors, from the rules agent's `Combat.score`). Every actor of every outro spawns, with an `AnimationGraph` of all its clips. Each tick `show` places and turns it (`Actor::place/facing`), stops the old clip and seeks the current one. The camera follows the cuts. `hud::outro` draws the bubble wherever `Subtitles::place(t)` puts it. The voice is optional. Debug: `CONKER_OUTRO=<cutscene>` plays that outro of the map at any match's end |
| Server | `results_at`: `team_result(&m.standings())` under `total_war \|\| colors`. The outro lengths come from the pack per map and winner, as before |
| Screens | `../lab/screens/colors/outro.sh` (Deathmatch on 30, a 3-minute clock, `CONKER_OUTRO`). `outro-squirrels-{early,bubble,stats}.png`, `outro-tediz-{early,mid,stats}.png` |

| Check (2026-10-12) | Result |
|---|---|
| `rom.rs` `colors_outros_follow_the_oracle` | every frame k ≥ 1 against the oracle CSVs: eye/look-at < 0.05, yaw < 1e-3°, actor place < 0.05, facing ±1, clip exact, clip time < 0.01 tick, fade exact; plus the lengths, cuts, pages, moves, voice and characters |
| `rom.rs` `outros_follow_the_oracle` (TW) | pass. The TW fade CSV alpha column (derived, 5 rows) was recomputed with the integer formula: ±1/255 on 5 frames |
| `hud.rs` `outro_subtitles_match_the_originals_draws` | TW pages, and cs1 DLs 461/536/636/666/756 (each bubble place), glyph rects exact |
| Pack | the task's command to `/tmp/pack-cout`, copied into `../assets`; `diff -r` empty; `maps/2d/outros.json` byte-identical to before |
| `bun run test`, `bun run check` | pass (exit 0), on the tree with the rules agent's Colors work in it. Deathmatch replays: this part touches no rules/sim code |
| Screens | read: cs1's officer with page 1 on the right, page 3 in the turned bubble on camera 8, the STATS over the officer and a soldier; cs5's low shot of the marching Tediz' feet with a tank behind, the column on camera 3, the STATS over the standing Tedi |

Not done: attachments (op 0x0D: helmets/guns on 88/90), the positional sounds (cs5 engines, march loop, one-shots), cs5's
smoke, op 9, heads/talk, the page and bubble fades, clip blends, the per-camera FOV.

## War: Colors, part 1: rules, server and bots (2026-10-12)
**`--mode colors` (map 30 only) plays the original's capture the flag (scenario 10, scene 0x30). As in Total War, the
level's AI script (0xBB, run by conker-bots) is the authority: its natives become `gas::Effect`s that the match
applies. The engine side (touch, grab, carry, put-down, the dropped flag's fall, the held flag) is in the new
`conker_rules::colors` and `game.rs`. `PROTOCOL_ID` 16.** The client only receives the data (the Colors client is
next). Research: `../lab/research/mp-original/colors.md`.

| Part | What |
|---|---|
| Rules | `colors.rs`: `FLAGS` (f 0 red: handle 0x201B, uid 0x1C, model 01:169, home (-330, 410, -3803), taken by team 0; f 1 green: 0x201A, 0x1B, 01:168, (131, 1342, 6144), taken by team 1), `Flag`, `Colors` (flags, team scores, fade, respawn bits), `Pose` (Grab 0xD5 17 frames, PutDown 0x28D 39 with release at 20, Plant 0x22D 30 with release at 19, Cheer 0x19A 49, Lose 0x2E7 60), `carry_clip` (0x1AF/0x1B0/0x1B1 = states 175/176/177). `Settings.colors`, `score` (default 20, 0 none). 6 lives, never lost. A touch (horizontal 45, I) grabs: no weapon, Z and A masked, B puts it down; untouchable until release + 51. `ends`: the clock, or a team at the score target on the capture's frame. `Standing.score`: `team_places` ranks S, then kills, then fewer shots (DRAW when all are equal) |
| Carry speed (C + V) | `func_1504CB98`: holding an object scales the drive by 0.8 (`0x3F4CCCCD`) and the ground acceleration is 0.75 (the fine step 0.0075): top 28 = 0.8 × 35. `PlayerState.holding`; non-holding movement unchanged |
| Effects | `Flag(f, shown)` (obj/obj2 on a flag), `LetGo` (command 7), `Settle` (0x23 on a flag), `Score` (0x62), `Pose` (0x45: 9 plant, 0xA cheer, 0xC lose), `Immune` (0x13), `Hold` (0x53), `Respawn` (0x82), `Fade` (`0x50BB` 0x12) |
| Bots | `brain.rs`: flag handles in obj/obj2/query (0x98, 0xB2, 0xC9, 0xCA, 0xCB) and command (2, 0x7D, 0x23); the guns (uids 0x13/0x14) are stubs; player queries 0x95, 0xA8, 0xED. The script's init-time `obj` is only a lookup: the flags appear at the timed create (live + 299) |
| Server | `--mode colors`, `--score 1-40|unlimited`. Teams by side. `MatchView.colors/score_limit/scores/flags/fade`, `Combat.flag/score`. `COLORS t=...` log lines (takes, LetGo, Score, Respawn, Fade). `bots.sh` takes `MODE=colors` and `SCORE` and counts the flags |
| Website | `data.ts`: Colors on map 30; score 1-40 or unlimited (default 20); no kill limit, no lives. `server.ts` validates and passes `--score` |

| Check (2026-10-12) | Result |
|---|---|
| ROM test `colors_flags_and_captures` (conker-bots) | flags at 299; grab at once; put-down release +50; retaken at release + 51; a dropped flag is gone after 900-904 frames and back 300 later; plant release +19, cheer +30, freeze (lock ≥ 199) through +89; respawn at +90 and flags home at +390 (+150/+450 when the scorer's own flag was carried, from the script's alternating early return); the 3rd capture ends the match on its frame; no unknown natives, no faults |
| Tests (target-net, `CONKER_ROM`, the full line) | pass (296) |
| Deathmatch combat replays | identical to HEAD's (`--nocapture`, HEAD from `git archive`) |
| clippy `-D warnings` (all targets; conker-client wasm32 too), `cargo fmt` of these files, web `bun run check` | clean (the outro agent's in-progress conker-extract was left out) |
| Bots (`MODE=colors MAP=30`, 330 s each: `BOTS=7`, the idle human a Tedi, port 5970; `BOTS=6`, the human a squirrel as in the research's bots1, port 5972) | no faults, no unknown natives, no panics, and the flags show at t 555/556. **But 0 takes, 0 captures in either run** (bots1: the first take at 1719, captures at 2983 and 6449 in 9898 frames). The collectors do run at the flags (state 4, goal = query 0xCA = node 111 under the red flag, 103 under the green). The squirrels reach node 109 (~1,150 from the red flag) by the lift and 25 → 108 → 109, and by the next 30-tick sample they are in state 6 (fetching a weapon at G[0x14F4]'s node 123), with an enemy newly set: the Tediz' gunner, which camps at node 3 by the stubbed south mounted gun, or the idle human. The straight walk (script 6) never shows. The Tediz get no closer than 800 to the green flag's high ledge, as in bots1 (700-900). Query 0xCB was checked against the C (an unlimited ray, then the barrier lines only past a hit beyond the target) on every same-band node within 2,000 of each flag: the same answers. From 109 it is 0 in both, so the straight walk can only start from 110. Open: the guns, the roles drawn, what the bots see. `bots.sh`'s Deathmatch check fails for bots that camp and never die (snipers, gunners) |

**Fixed: the bots take the flags (2026-10-12).** The original's pickup radius is `100 · s126+0x1AF / 64` (C
`func_1511DF6C`). A bot's +0x1AF is set by its AI script with command 0x7B: 0xBB sets 0 at 0x18BC every frame, 0x40 at
0x1F8E (within 100 of its goal) and 0x2902 (state 6) (V, the oracle's bots: 64 only in state 6). The remake used 64 for
every bot, so a squirrel collector at node 109 took melee pickup 7 (on its way). Then it held something other than its
preferred weapon, went to state 6 and walked back across the map. Fix: `Match.reach` per slot, filled by `Bots::scopes`
from `Npc::pickup`; humans keep 64. `weapon::touches(.., reach)` covers pickups and canisters, so Deathmatch and Total
War bots are gated too (0xBA's 0x7B: 0 at 0x16D6, 0x40/0x20 in its fetch states). Test: `rom.rs`
`colors_bots_take_a_flag`. It uses the server's 7 bots + 1 Tedi: a Tedi takes the green flag at 2,398, with a bound of
5,400. Without the fix: no take in 9,900. Same set-up, seeds 1-6, 9,900 frames: first take at 1,766-3,199, 1-2
captures each. `bots.sh` 330 s: `BOTS=6` (bots1's set-up), takes at 2171 and 3007, captures at 3351 and 6630 (bots1:
1719; 2983, 6449). `BOTS=7`: 3 takes (the first at 6513), 0 captures. DM/TW vs HEAD's server, same runs: 2d 9/9 hits,
10/9 pickups; 30 35/44 hits, 21/18 pickups; TW 42/38 hits, 30/37 pickups, 16/16 vs 15/16 checks. Open: whether the
flag touch also reads +0x1AF (I); the mounted guns (the oracle's Tedi gunner mounts gun A at ~480 and stays there).

Open or I: the touch radius (45, between 40 picked up and 48 not); the meaning of 0x45 values besides 9/0xA/0xC; the
+0x8A command (0x3F, a no-op); the guns (stubbed); a join or a leave restarts the script, which brings the flags home
(the scores stay); untimed with an unlimited score never ends; the end is on the capture's frame, not the original's
later phases. The pack has no flag models (01:168, 01:169) for map 30 yet.

## Landing: real game servers from a dev web server (2026-10-12)
**The landing's mock data is gone. A temporary, local-only dev web server (`web/app/server.ts`, `bun run serve`) now serves
`web/` in place of `python3 -m http.server`, and starts, lists and kills game servers: the Servers screen lists them,
"Create server" (map, the map's modes, bots, skill, time, lives, kill limit, radar) runs one, Kill stops one, and the
lobby joins with that server's own digest.** Not for the internet (loopback unless `--host`). The real hosting is still
to come. Details: `web/README.md`, "Dev web server".

| Part | What |
|---|---|
| Dev web server | `Bun.serve` on 127.0.0.1 and ::1 (`--port 8080`, `--host`, `--game-ports 5900-5949`, `--server`, `--assets`). `GET/POST /api/servers`, `DELETE /api/servers/:id`. Every value is whitelisted against `data.ts`'s tables, the argv is an array, JSON only (415 otherwise). It takes an even port N with TCP N, N + 1 free, writes the log and digest to a temp folder, answers once `/info` does. Kill is SIGTERM, then SIGKILL after 3 s. Ctrl-C/SIGTERM kills every child |
| Landing | `data.ts`: `MAPS` (each map's modes: data, for Colors next), `MODES`, `SKILLS`, the API calls; no mocks, no "This machine". Servers: list, status, Lobby, Kill/Clear, refresh, the create form. Lobby: settings, side, connection, Join (`digest=`). Gone: regions, players, pings, ready, chat, name. Without the API (a static site, `deploy/`'s Caddy), the Servers screen says so and links to `play.html` |
| Toolchain | `@types/bun` (dev), `tsconfig.server.json`, `check` covers `server.ts`. Vite's dev proxy adds `/api/` |

| Check (2026-10-12) | Result |
|---|---|
| `bun run check`, `bun run build` | pass |
| `bun run serve --port 8094 --game-ports 5990-5999`, release server | static: `/`, `play.html`, `play.js`, `.wasm`, `.gltf`, `.bin`, `.wav`, `pack.json` with their types, `..` refused. POST Deathmatch on 3f with 4 bots → 201 on 5990 in 0.9 s; Total War on 2d → 5992. GET lists both `running` with digests and `info` (`3f`/`2d`, protocol 15), `address` follows the page's host (`[::1]:5990`, `127.0.0.1:5990`); `/info` answers on 5991 and 5993. DELETE 1 → 204, pid gone, 5990/5991 free, temp folder gone. A server killed from outside shows `exited` (137), DELETE clears it. SIGTERM to `bun run serve` → the last game server killed, nothing left. 400 for total-war on 30, `;`/`|`/`$(...)` in map, skill, bots and name, bots 16, time 2.5, `__proto__`, non-JSON; 415 for text/plain; 404 for an unknown id. A bad `--assets` → 500 with the server's own error. Rerun on the final code: the same, then SIGINT (Ctrl-C) with a server up → killed, nothing left |
| Browser | not run here (left to the user in Chrome) |

Not done: no auth, no player counts or pings (the game server has no such endpoint), no logs in the page (the temp
folder's `server.log`), no `--cavemen` switch (0x3F always has cavemen), and game servers left running if the dev server
is SIGKILLed. `deploy/` (Caddy) has no `/api`, so its landing shows no list (`deploy/README.md` updated).

## War: Total War, part 6: the victory outros (2026-10-12)
**When Total War ends, the winning team's cutscene now plays before the STATS: cs2 (0x2D/2) if the squirrels
win, cs4 (0x2D/4) if the Tediz win, none on a DRAW. Each one has the original's camera path, its one actor
looping its clip, the fade from black, the speech bubble's pages and the voice line. Players and the HUD are
hidden. The STATS then come up over the last picture, with the actor still looping. The server's results
countdown and RESTART votes start after the outro.** `PROTOCOL_ID` unchanged (15): no replicated field was
added. Both sides compute the outro from `team_result`. Research: `../lab/research/mp-original/total-war-outro.md`.

| Part | What |
|---|---|
| Extract | New `outro.rs`. It decodes only what cs2 and cs4 use from `[6, 0x2D, n]`: the 0xFF length; the camera (kind 1) path and channels 10/7/8 (keys `f32, interp, channel, t`, Catmull-Rom for interp 0); the actor (kind 2): its uid's model from bank 0x0E's 0x30-byte records (0x13 → 135, 0x18 → 90), its place and facing (ch7 + 90°), and its op 2 clip with rate 2·ticks / VIs to the next op 2; op 5 b=1 fade (colours 0x80084110); op 7 subtitle pages; op 0x0B voice. It writes `maps/<map>/outros.json`, characters 135 and 90 with just that clip, and `audio/maps/2d/voice-b1.wav` / `voice-b3.wav` (bank 0x16 MP3s decoded by `audio::mp3_wav`, symphonia 0.5 mp3-only, conker-extract only). HUD textures `bubble` 0x1047 and `tail` 0x1049 (IA8, the RAM's 0x8025F250 / 0x8022E320). The bubble quads are the DLs' rects and s/t (steps in 5.10). The manifest lists 135 and 90 |
| Assets | `outro::{Outro, Camera, Key, Actor, Fade, Voice, Subtitles, Page, Quad}` and `value` (the key curve). `Slice.outros` is the 13th data file. Frame k shows t = min(2k, length), the STATS show at `frames()` = length / 2 |
| Client | New `outro.rs`. `Playing` (outro, frame) starts the tick the client sees Over in Total War, using `team_result` over the roster's `Combat`. The actors are spawned hidden once (their glTF handle is held for the clip). `show` poses the actor at `clip_time(k)`, hides `Model`s and drives the camera (eye + 400 along yaw/pitch). `hud::outro` draws the fade over the whole view, then the bubble quads and the lines, upper case and black. The text gets spaces (4 px), and the pen now moves the glyph's x offset too (the `'`), while widths are measured without it. `results::menu` waits for the outro. The audio plays the voice flat at `MIX` on its frame (`VOICE − n`), and Over stops only the background |
| Server | `Game.outros` (frames per winner from the pack). `results_at` = end fade + 1 + the winner's outro (0 on a DRAW or outside Total War). `restart_in` counts from it, and `restart_votes` only takes votes from it |
| Pack | `../assets`: `README.md` (outros.json, 135/90, voices, bubble/tail, the pen rule), `pack.json`, `hud/hud.json`, `hud/bubble.png`, `hud/tail.png`, `maps/*/outros.json`, `characters/135`, `characters/90`, `audio/maps/2d/voice-b1.wav`, `voice-b3.wav` |
| Lab | `../lab/screens/total-war/outro.sh squirrels|tediz` (port 5980; the gas ends the match: P1 on its side's canister, warped in at read 1000, the others die at 1300, Over seen at ~1554) |

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line + extract-character) | pass (exit 0, 288 tests). New: conker-extract `outros_follow_the_oracle` checks winners, lengths, models (135 has 40 joints), clips, page times, voices (22050 Hz, 8.81 / 12.24 s ± 0.08), and against `outro_cs{2,4}.csv` at reads 458, 476, 556, 756 and the last: t exact, eye/look-at within 0.05, yaw within 0.001°, actor place/facing/clip/rate exact, clip time within 0.001 tick, and every `fade_cs*.csv` alpha exact. conker-client `outro_subtitles_match_the_originals_draws`: every glyph rect equals the DL's (`hw/cs*b_dl_*`), 7 of 8 pages exact, the 8th as listed below. conker-assets `keys_hold_ease_and_interpolate` and the round trip with an outro. conker-server `the_results_count_down` (Total War: DRAW at once, Tediz win + 400). conker-extract `pack_audio_reads_back` lists the voices |
| Deathmatch combat replays | output identical to HEAD's (110 lines, `--nocapture`; HEAD from `git archive`) |
| `cargo fmt --check`; clippy `-D warnings` (10 crates, all targets; conker-client wasm32) | clean |
| Fresh export (the task's command) vs `../assets` | `diff -r` empty after the copy (983 files) |
| Browser delivery (read, not run) | the pack is served whole (`web/assets` symlink; Caddy `handle_path /assets/*`), and the page fetches `Slice::files`, which now includes `outros.json`. Bevy loads the glTFs, PNGs and WAVs from the same folder |
| `outro.sh squirrels` | `outro-squirrels-early.png` (frame ~4): the officer (135) at the squirrels' base, half faded from black. `-bubble.png` (frame 120): page 2 in the bubble at the top right, as the DL sheet. `-stats.png`: SQUIRRELS WIN over the last picture, the clip in another pose |
| `outro.sh tediz` | `outro-tediz-early.png`: the stitched Tedi (90) before the Tediz' base, fist up, fading in. `-bubble.png` (frame 140): ARE VICTORIOUS ! at the top left. `-stats.png`: TEDIZ WIN over the frozen frame, the Tedi still looping |

Not done or I. On cs4's first page the original draws lines 2-3 one pixel higher than lines are spaced elsewhere (open; the test allows it). The `.` advance with its x offset is unverified, since it always ends a line. Not modelled: the head channels 0-2 (nod and turn), the talk flag (op5 b=0x17), the bubble growing at the open and shrinking at the close, each page's text fading in and out, cs4's attachment set 0x71 (op 0x0D), the STATS fading in (+4·dt), the DLs' two other fill alphas (env 0xD7/0xC7, then 0x17 at t 32; open in the research), and the voice fading out when the STATS are left. The voice's volume (`MIX`) and whether the match music stops are I. The fade blends in linear light, so it reads lighter than the N64's at the same alpha. A client that has never drawn a solid quad (here a player who never died) shows the fade only once Bevy has compiled its pipeline, a few frames in. Lift 0 for 135/90 and their feet on the floor are not visible in any shot (the framing cuts at the knees). The outro restarts on a client that joins during it, and roster changes during Over can change the winner the client computes.

Reported "stuck gas death" (2026-10-12; `../lab/screens/total-war/stuck-gas-death-*.log`: 1 browser squirrel, 1
headless Tedi, untimed, 1 life, warp at read 1500): not a bug in the rules or the server. That run's own logs show
the end: the Tedi's HUD lines (the server's replicated `Combat` and `MatchView`) read OUT OF LIVES at t 2339 and MATCH
OVER at 2395. That is the gas hit at 2142 + 86 + 52 + 55 = 2335, then `END_DELAY` + `FADE`. Clip 0x59 loops, as
an out player's dead pose does, and the server logs no line at the Over. Native rerun (port 5970, same flags, the
squirrel placed on canister 0): out at 2199, Over at 2255, both clients log `outro: cutscene 2, 325 frames`. With
`--wait` the server never starts a next match (by design). The browser was not checked. No code change. New guard
test conker-rules `a_gas_death_puts_out_or_respawns`: Total War, untimed, a gas `Hit` through `apply`; with 1 life
the victim is out and the match ends at 193; with 2 lives it respawns at 193. It passes on the current tree. There
was no fix, so it is a guard, not a regression test.

## War: Total War, part 5: the gas's look and the mask clips (2026-10-12)
**A gassed player's view gets the original's green/red scrolling haze (0xE013) from the script's call. During the
countdown the shelter goes red (0xE016: lamps off, the red light pulsing), and a canister at home and a carrier shine
green (0xE017, kind 0x10). The mask goes on and comes off with its clips (0x151 / 0x14E): predicted, no move, jump or
fire, its sounds, the mask in the hand then on the head.** `PROTOCOL_ID` 15. Research:
`../lab/research/mp-original/total-war-effects.md`.

| Part | What |
|---|---|
| Rules | `Action::Mask { on }` for `gas::MASK_FRAMES` (19): the mask's delivery starts the put-on (anim 0x151), `Effect::Release` the take-off (0x14E), neither while hurt; from the second frame the stick and buttons are zeroed. `Effect::Haze(slot)`, `Fighter::gassed` (the call's frame, kept through a respawn), `Fighter::haze(tick)` (1-`gas::HAZE_FRAMES` = 150) |
| Bots | object 3, kind 0xE, handle 0xE013 → `Haze(args[1]'s slot)` (0xBA at countdown 29, players outside trigger 0) |
| Net, server | `PROTOCOL_ID` 15: `Combat.haze` (k, 0 none; `combat()` from `Fighter::haze`), the fighter's `Mask` and `gassed`; the `GAS` log lists `Haze`; the client's `/info` doc 15 |
| Anim | `combat::MASK_ON` 0x151, `MASK_OFF` 0x14E, rate 3 (clock 3 … 57) |
| Extract | `extract_character::due`: an event fires at the running max of the times before it (the dispatcher stops at the first record not yet due), for held models and clip sounds (existing events unchanged). `HELD_CLIPS` and `MATCH_CLIPS` add 0x151 / 0x14E: held #66 (joint 12) and #67 (joint 9) + 5 textures; events 0x63E t10, 0x69B t50 (put-on), 0x69C t18, 0x63F t59 (take-off) + 4 WAVs. HUD texture `haze` (0xA48 IA8 64×64) and `hud.json` `haze` (2 layers: colour, share, texels per pixel, t). `maps/<map>/lights.json` (`map_lights`: 2d's ambient (50,50,52), 2 lamps `calm`, red `countdown` 85 ± 55 step 0.1021, a green `shown` 60 ± 30 step 0.2021 per canister, a carrier's green 30; other maps none). `props.gltf` drops scene 2 (the 3/84 mask placeholder) |
| Assets | `Lights`, `Light`, `When` (`Slice.lights`, the 12th data file), `hud::Haze`, `HazeLayer` |
| Client HUD | `hud::haze`: two quads over the whole window (the iris's far margin), texels from the frame's top-left scrolling k/4 a frame, z 0.5/0.6 under the HUD; alpha min(4k, 128) then −8 a frame from k 132; its texture wraps (the other HUD textures still clamp) |
| Client held | `mask_shows`: in 0x151 the last put-on event that fired decides (hand from frame 9); in 0x14E nothing; otherwise a masked player has the models whose last put-on event is a create (the head, from frame 20). `masked` from the prediction for the local player. The canister only (props scene 1), plus the carrier's point light |
| Client lights | new `lights.rs`. During the countdown (`countdown > 0`), the level's and props' vertices inside trigger 0 (386, y −1550 … −979) get colour × min(1, ambient + Σ lit rgb × min(1, 2048·size/d²), none from 362√size), in display values, and are restored after. The red and green lights (not the calm lamps) are also Bevy `PointLight`s, 2500 lux where full |
| Pack | `../assets`: `README.md`, `audio/events.json`, `sounds.json` + 1598, 1599, 1691, 1692.wav, `hud/haze.png`, `hud.json`, `maps/*/lights.json`, `maps/2d/props.gltf`, `weapons/deathmatch/held.*` + 5 textures |
| Lab | `gas.sh fx` (port 5980; slot 3 placed by canister 1): `room.png`, `haze.png`, `hand.png`, `green.png`, `mask-on.png`, `carrier-light.png` |

Lights: not faithful. The CBFD vertex lighting needs the level's light table, which is still open: about 30 lights,
plus kind-5 lights whose role is unknown. So the remake keeps its unlit look outside the countdown and relights only
the shelter (trigger 0) from the lights the research found. The camera's look-at going from +67.5 to +90 after the
put-on: the mechanism is `func_15124C38`'s wall-hit share (C: 1.0 instead of 0.75 of the head's 90; no other branch
gives +90), which the remake's `look_at_height` already has, so nothing to implement. The trigger is I (the turn
the put-on finishes bringing the wall ray into a hit). Not reproduced: in `fx`, P2 on mask 21's spot had the wall
hit from read 990 to 1437 (+90 throughout, no rise to see), and e2's spot (k23) was not tried.

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line + extract-character) | pass (exit 0): conker-rules 67 (`gas_canister_masks_and_hits`: put-on 19 frames, A/B/Z held do nothing, take-off, haze through a respawn), conker-client 46 (new `mask_in_hand_then_on_head`, `gas_haze_matches_the_originals_draws`: st (2, 32)/(2, 0), steps 1/0.5, env 32/16 at k 8, fade 0x70 … 0x10; `lights::reach_and_switches`), conker-bots ROM (`total_war_gas`: hazes for slots 1, 2, 4 at delivery + 271), conker-extract (`rom.rs` 16 held; `pack.rs` the props nodes end `03-55@28`, `03-55`, the mask's 4 sounds for 117 and 128), extract-character (new `gas_mask_models`, 117 and 128), -server, -sim, -anim, -assets, extract-scene |
| Deathmatch combat replays | output identical to HEAD's (110 lines, `--nocapture`; HEAD from `git archive`) |
| `cargo fmt --check`; clippy `-D warnings` (10 crates, all targets; conker-client wasm32) | clean |
| Fresh export (the task's command) vs `../assets` | `diff -r` empty after the copy (950 files) |
| `gas.sh dm` vs part 4's | `dm-cover.png` byte-identical; `dm-door.png` 2 ceiling pixels off by 1 level, from the 2 more (hidden) held models per player: without them, byte-identical (entity order; waiting for `MatchView` to skip them in Deathmatch moved them too, so kept) |
| `gas.sh fx` (delivery at read 1000, haze from 1271, kill ~1300, mask off 1389) | `room.png` P1 in the shelter at 1100: dark red walls and door, the floor bright red under the light, digit 4; `haze.png` P2 masked at 1350: green wisps with darker red streaks over the whole view, HUD on top; `hand.png` P2 at 1016 holding the mask out in its right hand; `mask-on.png` P2 in the mask at 1150; `green.png` canister 1 at home and slot 3 beside it lit green on its canister side; `carrier-light.png` P1 carrying canister 0, its body and tail lit green |

I or skipped: Bevy blends the HUD in linear light, the N64 in display values, so the haze reads stronger over dark
areas (`haze.png`: a dark wall's G 74 → 134, where display-space blending gives at most 116), as every HUD quad.
k = 1 in the state after the frame after the call (±1 frame against the DL counts). Characters keep the remake's
sun and ambient, so the red light adds to them (the original: ambient + lights). The terrain by the canisters stays
unchanged (only characters and lit models take their green). The room is trigger 0's box, not the light table.
2048 (`lights::FULL`) takes clip space as world units (unmeasured; the knob). The lamps' flicker (behaviour 4) is not
modelled. The swing phases come from the client's clock. On the mask: the original finishes a turn under way during
the put-on (+3072 a frame to the move angle), which the remake's zeroed stick and idle cut short. The put-on brakes
3 → 0.5 → 0 (V e2: 3 → 4 → 1.5 → 0). Hurt players skip the clips. The head mask shows from the idle after the
put-on by rule (its create at t59 never fires). `0x80042778` = 5 (I: the muffled mask sound) is not done.
Offscreen clients log bevy_light's `ShadowLodOrigin` warning (point lights with no window camera; no shadows used).

## War: Total War, part 4: the gas on the client (2026-10-12)
**The client draws part 3's state: in Total War the doors, covers, masks and canisters show by `MatchView.props` /
`taken`, a carrier holds the canister, a masked player wears the mask, the countdown digit prints mid-screen, the gas
loops while it is out and a canister taken plays its line. Deathmatch draws as before (screens byte-identical).**
`PROTOCOL_ID` unchanged (14); no rules or bots change.

| Part | What |
|---|---|
| Pack | `maps/2d/props.gltf` + `props.bin` (`conker_extract::props_level`): scene 0 the 10 placements the level cuts (0, 1, 21-28) named `model@placement` as the level's nodes, scene 1 the canister 3/85, scene 2 the mask 3/84, at the origin; textures are the level's PNGs (the image loop skips a path already added). `hud.json` `countdown` (150, 86, centred, white, no shadow). `audio/events.json` `gas` (0x74CF → 1231, looped) and 4 `carry` (0x7599/0x759A → 1433/1434, 0x7640/0x7641 → 1600/1601) + `sounds.json` + those 5 WAVs; `README.md`. `conker_assets::props_gltf`, `Hud::countdown`, `On::Gas`, `On::Carry { canister, bot }`. `../assets`: the 2 props files and 5 WAVs added; `events.json`, `sounds.json`, `hud.json`, `README.md` rewritten (additive) |
| Client render | `add_props`: in Total War, `props.gltf#Scene0` spawned once (Deathmatch never loads it); `show_pickups` also hides a `gas::PROPS` placement whose `props` bit is clear (Total War only); the masks follow `taken` like the pickups |
| Client held | `hang_gas` / `show_gas`: scene 1 under `joint_00` at (0, −30, 24), scene 2 under `joint_12` at its origin, both at the world's size, shown by `Combat.carrying` / `.masked`. The weapon is already hidden while carrying (the pickup holsters: `drawn` false, no event in 0xD7/0xD8; the prediction too: `Fighting.state` is the whole `Fighter`, a draw needs `!carrying`) |
| Client HUD | `Shown::countdown` = `hud::countdown(m.countdown)`: `(countdown + 1 + 59) / 60` while 1-299, as text |
| Client audio | `Sounds::gas`: the `gas` cue at the listener, unpositioned, from `MatchView.gas`'s rise to its fall (stopped by tag); `Seen::carry`: a carry starting plays `carry { canister: team, bot }` at the carrier (bot: `PeerId::Local`) |
| Server (test only) | `--warp SLOT:READ:X,Y,Z` moves a player at a read (as the research's position pokes) to deliver without walking; `replay_flags_parse` covers it |
| Lab | `../lab/screens/total-war/gas.sh`: `gas` (port 5980, 3 squirrels, no bot) and `dm [DIR]` (port 5982) |

Findings: the countdown's two forms agree: script 0xBA prints at 0x3DFE (`command(0xF000, 1, G+0x1FE4)`) before
`timer10` decrements at 0x3E06, and `MatchView.countdown` is after the frame, so the printed `(t + 59) / 60` with
`t = countdown + 1` is `countdown / 60 + 1`. F 1 (`func_1509DFC4`) draws `func_15042D94(150, 86, 1, "%d", ..)` (format
at 0x8009F300) in white; `func_15043384`'s flags: 1 centred, 0x80 no shadow, 0x40 clear (off the viewport's scale).
The script's objects of kind 7 (`(h >> 12) & 0xF`) are sounds, `h & 0xFFF` the sound: 0x74CF mode 1 is a loop with
volume 0x6590 at the listener; 0x7599 / 0x7640 mode 4 (`func_15103C14`) a record at the carrier heard everywhere,
volume 30000, and a carrier without a camera (a bot) plays the next id at 32700. 1231 loops (0-22756); 1433/1600 are
the voice lines at 15956 Hz, 1434/1601 the same at 7978 Hz.

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line) | pass (exit 0): conker-client 43 (new `gas_countdown_prints_as_the_game`: 300→none, 299/240→5, 239→4, 60→2, 59/1→1, 0→none; the 5 centred at x 150, top 86, white), -server 16 (`replay_flags_parse` + `--warp`), -extract (`pack.rs`: the props nodes `04-2d-04@0` … `03-55@28`, `03-55`, `03-54`, and `props.gltf` read back), -rules 67, -bots, -sim, -anim, -assets, extract-scene |
| `cargo fmt --check`; clippy (assets, extract, client, server; all targets; conker-client wasm32) | clean (`-D warnings`; clippy over rules, sim, bots, anim too) |
| Fresh export (the task's command, `/tmp/pack-gas2`) vs `../assets` | `diff -r` empty after the copy (no extractor change since); `maps/2d/level.gltf`/`.bin` unchanged |
| `gas.sh gas` (P1 on canister 0 at read 960, warped into the shelter at 1000) | `../lab/screens/total-war/canister.png` canister 1 on its spot; `carrying.png` P1's own view from behind, the canister in front; `carrier.png` P1 side-on hugging the upright canister; `countdown.png` the digit 4 mid-screen (P1 in the shelter); `masked.png` P2 in the mask on its spot (21); `doors.png` door 1 shown across the corridor during the gas, digit 1 |
| `gas.sh dm` vs the same run before the change (`--wait 2`) | `dm-cover.png` (the shelter, no mask), `dm-door.png` (no door): byte-identical |

I or skipped: the canister's and the mask's joints, offsets and size (the world's) are fitted (the game draws item
0x25 / 0xA0 by a weapon-manager path not read; the canister sits between 0xD8's hands: the squirrel's ±31 apart, the
Tedi's (117) ±40 at 39 up, so 12 short of each, not screenshotted); the mask's put-on/take-off clips (route 0x51 / 0x4E) are not played (the rules own the clip); 0xE013 (the gassed screen), 0xE016 (the gas) and
0xE017 (a canister back) are skipped, no cheap placeholder; 0x75A3 / 0x7642 (the carrier's) are not reached (part 3);
`func_15103C14`'s per-team 600-tick busy table is not kept (every carry plays); the kill's frame shows no digit (the
game prints 1 then); the delivery's command (0xA032, 0) sound is not played.

## War: Total War, part 3: the gas canisters, the gas and the masks (2026-10-12)
**Server, rules and protocol (no drawing yet): 0xBA's script holds the gas (canister timers, carriers, the
countdown, the deliverer, the masks' removal) and runs in Total War with or without bots; its natives turn into
`gas::Effect`s that the server applies to the match before each step. The match keeps the engine's side: the props
the script drives (shown, +0x80, +0x84), the canisters' touch, the masks as pickups, the carrier and the hits.**
`PROTOCOL_ID` 14. Research: `../lab/research/mp-original/total-war-gas.md` (runs d1-d3).

| Part | What |
|---|---|
| Rules | new `gas` module: `PROPS` (placements 0-7, 27, 28 with their ids 0xFF-0xF8, 0x1A, 0x1B), `CANISTERS`, `Gas` (`props`, `countdown`, `gas`; doors hide 50 frames, covers 12, after +0x80 goes to 1), `admits` (+0x84), `Effect`; `Match::gas`, `Match::apply(&[Effect]) -> Vec<Event>`, `Match::restart_gas`; a canister shown is touched as a pickup (gate +0x84, slot + 1 into +0x80, hidden at once); `Gives::Mask` (k19-k24 = placements 21-26 in `PICKUPS_2D`, now 25; `Arena::deathmatch` leaves them out), touched only while `countdown > 0` and not by a masked player, delivered at touch + 2 in the weapon's slot; `Fighter::carrying` (armed 0x25: 80 % speed, all C strafe, no crouch, no skid; no draw/fire/grenade/reload; holstered; item 0xA5; anims 0xD7 moving / 0xD8 standing) and `Fighter::masked` (item 0xA0); both go at a lethal hit; `hurt`'s attacker is an `Option` (none: no score; reaction 0x2D: no +0x70); `damage::GAS` (type 0x1F), `DOORWAY` (0x3B), `script_type`, their sequences |
| Bots | `Bots::tick` returns the effects (only in Total War); natives: object 3/4 kind 7 (0x74CF → `Gas(true)`, its handle stopped → `Gas(false)`, other sounds 0), kind 0xE effects ignored; prop query 0x21 (a canister's +0x80, as participant + 1); prop commands 0 → `Show`, 4 → `Set` (0x7FFFFFFF keeps); F 0xC → `Release`; player commands 0x2A / 0x2B → `Hit`; after the script: `Countdown(G+0x1FE4)` and `Carry` for G+0xFF0 = 0x25 changes; the mirror lists masks (gift 0x20) and keeps G+0xEB0 on a mask pickup while one is worn; `Body::drawn` 0x25 while carrying; `g::COUNTDOWN` |
| Net | `PROTOCOL_ID` 14: `MatchView.countdown`, `.gas`, `.props` (bit n: placement n shown, n in 0-7, 27, 28), masks in `.taken` (21-26); `Combat.carrying`, `.masked`; the fighter's two flags |
| Server | `arena(pack, total_war)` drops the masks in Deathmatch; `Ai::restart` makes the AI with no bot in Total War; `restart_bots` also restarts the gas; `simulate` applies the effects before the step (their hit events first); `GAS` info lines (delivered, canister back, carry, gas sound, mask off) |
| Pack | `maps/2d/pickups.json` + 6 `"mask"` entries; `README.md` (pickups count, `"mask"`). `../assets`: those two files rewritten |
| Client | compiles (pass-through); `/info` doc protocol 14 |

Effects (`conker_rules::gas::Effect`): `Show(placement)`, `Set { placement, flag, gate }`, `Countdown(i32)`, `Gas(bool)`,
`Hit { slot, kind, attacker: Option<slot> }`, `Release(slot)`, `Carry(slot, bool)`. Sound/visual-only calls are left to
the client from the state: 0xE016 (the gas, at the delivery and per victim), 0xE013 (a player's gassed screen, from
countdown 29 outside the shelter), 0xE017 (a canister back), 0x7599 / 0x7640 (a canister taken), command (0xA032, 0) (a
delivery sound); 0x75A3 / 0x7642 (the carrier's) are not reached (level query 0x25 answers 0, as before).

Findings: the carrier's 80 % is `func_15064B94`'s 0x25 (C; conker-sim's `armed_walk` had it), V d1: the drive fits
79.4-81.1 % on both stick settings (+1 a frame to within 1 of the target, then +0.01). The script ticks one canister's
timer a frame by turns (G+0x1FE0) and returns before the touches and the delivery test while that timer runs (0x4338),
so a touch becomes a carry 1 or 2 frames later and a delivery is seen within 2. The gas (0x2D) and doorway (0x0E) hits
are `func_1505D024` without guard flags (immune players are hit too); the deliverer gets hits and kills for enemy
victims only, nobody's +0x70 moves (d1, d3). Death clips: gas 0x16E 86, 0x16F 52, 0x59 55; doorway 0xF1 59, 0xF6 87
(V d2). The shelter's floor is at y −1448 by trigger 5 (d1).

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line) | pass (exit 0): conker-rules 67 (new `gas_canister_masks_and_hits`; gas.rs `doors_and_covers_hide_after_their_flag`) + combat 3 + ROM; conker-bots 15 + oracle 3 + ROM 6 (new `gas_props_of_0x2d`: ids, canister places, masks k + 1; `total_war_gas`, no bot, 5 humans placed as d1/d2: canister 0 at 757 (window 600-902), carried 1-2 frames after the touch, the doorway 1 a frame at delivery + 211..216, the gas on + 300 for the two outside and unmasked (enemy and team-mate), the deliverer 1 kill 1 hit 0 suicides, mask off at + 389, sound 271-389, doors/covers as d2, canister back 600-902 after, 0 unknown natives); -server 16, -client 42, -sim, -anim, -assets, -extract (`pack.rs` reads the masks), extract-scene |
| Deathmatch combat replays | output identical to HEAD's (110 lines, `--nocapture`) |
| Fresh export (the task's command) vs `../assets` | `diff -r` empty after copying `README.md` and `maps/2d/pickups.json` (929 files) |
| `cargo fmt`, clippy (rules, bots, server, sim, assets, extract, anim, client; all targets; conker-client wasm32) | clean |
| `MODE=total-war HUMANS=1 BOTS=5 bots.sh 300` (port 5970) | canister 1 back 714 frames into the match, canister 0 879 (`GAS` lines); no bot touched or carried one, so no countdown and no mask taken (the original's bots carried none in 620 s either, run1); 51 hits, 12 kills, 5 lift rides; 0 panics, warnings, unknown natives or faults. 15/16: bot 4 never died (a sniper on its tower spot, as in part 1) |

Stubs left for the client batch: draw the props by `MatchView.props` (Total War only; Deathmatch leaves it at 0xFF and
the level has no 0, 1) and the masks by `taken`; placements 0, 1 and 21-28 are cut from the level glTF
(`conker_extract::hidden_placements(0x2D)`), so their models must be exported (and hidden in Deathmatch); the countdown
digit (`countdown / 60 + 1` while > 0, F 1's text at (0x96, 0x56)); the gas's loop and the visual effects above; the
carrier's canister model in hand (item 0x25) and the mask on the head (item 0x20); `Combat.carrying`/`masked` for the
other players.

Gaps (I unless tagged): the props' collision stays Deathmatch's (covers 2-7 solid, doors 0-1 not; the game's follows
+0x6E); the mask's put-on (0x151, 19 frames, V d2 1104) and take-off (0x14E, 19, V d2 1452) clips are not played; a
canister hides at once (the game shrinks it for 3 frames); the carrier's jumps are allowed and its camera weapon stays
0 (V: s126+0x78 = 0x25); a non-lethal doorway hit's length (59, as the lethal one's first state); the doors hide 50
frames after the match start's flag (the game 51); a join or leave mid-match restarts the script and so the gas; the
carrier keeps its weapon, holstered, through the carry and after it (d1's carrier had none); the mirror gives the masks
the weapons' pickup flags 6 (the game's for them unread).

## War: Total War, part 2: the HUD and the team STATS (2026-10-12)
**In Total War the client draws the team strip in place of its own kills and rank, the radar's blips by team (squirrels
green, Tediz red; Deathmatch stays red), and after the end fade the team STATS: DRAW, YOU WIN / YOU LOSE (one human) or
the winning team's name and WIN (more), the columns' icons, labels and rows 40 lower, each row with its team's place in
its team's colour, the strip under them.** Not in it: the outro cutscene (the game plays the winner's, squirrels 2 /
Tediz 4, then the STATS over it; here the STATS come right after the fade, as after a DRAW in the game); the gas batch's
countdown. `PROTOCOL_ID` unchanged (13). Research: `../lab/research/mp-original/total-war-teams.md` §6.

| Part | What |
|---|---|
| Pack | `hud.json`: `strip` (texture 0xD10 → `teams.png`, 16 × 16 cells: flags on the clock, heads untimed; the squirrels' at x 198, the Tediz' at 66, y 196; numbers grey C0C0C0 over a black shadow at 215/83, 197) and `radar.team_colours`; `end.json`: `teams` (`down` 40; sprites 0x58 DRAW, 0x52 YOU, 0x56 WIN, 0x57 LOSE, 0x50 SQUIRRELS, 0x51 TEDIZ → `draw/you/win/lose/squirrels/tediz.png`, centred on y 50 as STATS is on 20); `README.md`. Added to `../assets`: the 7 PNGs; rewritten (additive): `hud/hud.json`, `hud/end.json`, `README.md`. The new fields are required: a pack exported before this batch no longer loads |
| Client | `Shown::strip` (each team's kills, untimed its lives left, summed over everyone's `Combat`), `Shown::header` (`header()`: `team_result`, then the human count), blips carry their colour, a results `Row` carries its colour (`rows(.., total_war)`: the team's place from `team_places` and colour, else as before) |

Findings: the strip is V (the gas run's DL 3000 RAM dump, `../lab/research/mp-original/total-war-gas-scratch/run1`,
walked with `../lab/hud/tools/hudwalk.py`: timed, Tediz 4 / squirrels 2; texels (16, 16) at x 66 and (0, 16) at 198;
the numbers' format `" %d"` at 79/211, the space moving the pen 4); the heads row untimed is C (HUD flag 0x100). The
STATS draw the strip live, as in the match (`func_151EA15C` passes 0x800E0BD3 == 2, which is 2 in every RAM dump). Row
colours: table 0x800ABA90 = red, blue, green, yellow (V RAM) by team (C), so squirrels red, Tediz blue. hudwalk's
RGBA32 view of 0xD10 is garbled (its TMEM model); `teams.png` equals the research's own decode (`spr.py`'s rule, as
the 11 end PNGs). +0x70: no STATS column shows it (C `func_151EA15C`: place, accuracy, kills, headshots, deaths), so the
forgiven-traitor difference noted in part 1 stays invisible; only the voice line would read it (not ported).

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line) | pass (exit 0): conker-rules 65 + combat + oracle + ROM, conker-bots, -server 16, -client 42, -sim, -anim, -assets, -extract (`pack.rs` reads the new `hud.json`/`end.json` back), extract-scene |
| Client tests (with the new pack, no "skipping") | 42 (new `total_war_rows_and_header_go_by_team`, `total_war_matches_the_originals_draws`: strip rects/texels and digits at 83/215, no kills or rank, heads untimed, every header's rects, icons y 89, labels 104, rows 106/121 blue/red, the strip in the results' anchor); Deathmatch's layout and end tests unchanged but for the row's colour index |
| Fresh export (the task's command) vs `../assets` | equal after the copy, but `game/ai.bin` (kept, as before); the 7 new PNGs equal `/tmp` decodes by `spr.py` pixel for pixel |
| `cargo fmt`, clippy (conker-client, -assets, -extract, all targets; conker-client wasm32) | clean |
| Native, `../lab/screens/total-war/shots.sh hud` (ports 5980/5982) | `hud-timed.png` (2d, 1 human + 3 bots, 45 s): flags at x 66/198 with 0 and 0, clock 2:16, no kills or rank; `hud-untimed.png` (2 bots, the Tediz one placed by the squirrels' base, 8 s): heads, Tediz 3 / squirrels 6, the lives digit 3, no clock; radar: a red Tediz and a green squirrel blip |
| Native, `shots.sh stats` / `lose` (3-minute matches, ports 5980-5987, the shot 195 s in) | `you-win.png` (1 human alone: the Tediz empty, out), `you-lose.png` (a Tedi + an einstein squirrel bot placed in the Tediz' base: P1 2nd, 1 death, strip 0 / 1), `draw.png` (a human per side, idle: both 1st, P1 red, P2 blue), `team-win.png` and `team-win-tedi.png` (2 humans + 2 bots: TEDIZ WIN, the Tedi 1st in blue, the squirrel 2nd in red, strip 1 / 0), `draw-hud.png` (the Tedi's HUD at read 1500). At 1280 × 720 (× 3.33, centred) the header, icons (y 89), rows (106, 121) and strip (196) sit where the frame's numbers put them; RESTART/QUIT as before |

Gaps: the outro cutscene (above); the icons' and labels' 40 (C: drawn from the rows' y, V for the rows); the heads row
(C); the space's width from one dump; one human: the header goes by the viewer's team (the game's: slot 0's, its only
human); a human gone from the match is no longer counted (the game counts the controllers that joined); the labels
P1, P2 go by row as in Deathmatch (the game's by controller). `shots.sh`: bots take the squirrels in even slots, so a
lone bot is the human squirrel's team-mate (its first `lose` run was a YOU WIN: the Tediz empty).

## War: Total War, part 1: teams, the server mode, the bots (2026-10-12)
**`conker-server --mode total-war` (map 2d only, not with `--cavemen`) plays scenario 9's rules: every player's team is
its side (0 squirrels, 1 Tediz); friendly fire hurts in full but scores no hit, kill or headshot (the attacker's +0x70,
`suicides`, counts it); the match ends on the clock (timed; the kill limit is not read) or once a team has no member in
play (untimed); the teams rank by kills, then fewer shots, both equal a DRAW. Bots run 0xBA's team branches.** Not in
it: the gas canister, gas and masks (next batch), the client's Total War HUD/STATS/outro. Findings:
`../lab/research/mp-original/total-war-teams.md`.

| Part | What |
|---|---|
| Rules | `Settings::total_war`; `hurt` scores by team (`func_151E5FF4`): hits, headshots and kills on other teams only, a death on the attacker's own team (itself included) in `suicides`; `Standing.team`; pure `team_places(&[Option<Standing>]) -> [u8; 2]` and `team_result(..) -> Option<team>` (`None` = DRAW) for the server and the client; `Match::ends` (team branch) and `Match::places` (each player its team's place) |
| Net | `PROTOCOL_ID` 13: `MatchView.total_war`; `Combat.team` is the side in Total War (it already carried `team`, `side`, `kills`, `shots`, `out`, `lives_left`) |
| Server | `--mode deathmatch\|total-war` (rejected off 2d or with `--cavemen`); `Game::team` (the side in Total War, else one per join); a client asking for the cavemen in Total War gets the smaller side; the per-second bot lines show `fight (enemy, target)` as slots; a warning every 10 s if the AI met an unknown native or faulted. `bots.sh` takes `MODE=` |
| Client | `match_settings` carries `total_war`; `hud::standing` fills `team` (compile only); the `/info` doc says protocol 13 |
| Bots | query 0xD5 = the FFA flag `0x8008FDC8` (0 in Total War); commands F 5 (`func_1508A1BC`, the ranking) and F 7 (`func_1508BF14`, a team shares out its targets: roles 2/3 first, each the enemy ranking it lowest, the score doubled per team-mate already on it; ported from the MIPS); F 0x11 / 0x17 (`func_1508B194` / `func_1508B1D4`: a human's +0x70, zeroed: the script's "Killed too many team mates!!!" makes a human with 4 every bot's enemy until it dies); `func_1508A6FC`'s team tally G+0xDDC on each death (the team roles at 0x37F2 reassign on net losses) |

Decided: lives already go at the respawn moment (`out`, `lives_left` change at the end of the death sequence, V
`elim`), so nothing changed there. The team branch's "no team that started with a human is in" falls under "< 2 teams"
with two teams; that check waits for someone to be out, as Deathmatch's does (players join as they go). Not ported
(I): untimed, the game ranks the teams in play by Σ(lives·6 + health) first, which nothing shows (the match ends only
once a team is out); two teams out, the one whose last member went out first is 2nd (the game: team 0 first); the
+0x70 exemption for reaction 0x2D (no such reaction in the remake).

Script calls stubbed for the gas batch (0xD5 = 0 reaches them): prop query (3, 0x21), a canister's carrier + 1 → 0 (no
canister); the canisters' timers (G+0x1FD8, 300 + random(150)) run out after 600-900 frames and call prop command (3,
0) and kind-0xE objects, no-ops as before; the script lists 25 pickups in Total War (+6 masks, gift 0x20), the mirror
keeps the arena's 19. Not reached while nobody carries a canister: the countdown (G+0x1FE4, command F 1's HUD text),
the gas kill (player command 0x2B), 0x2A, the kind-7 objects.

| Check (2026-10-12) | Result |
|---|---|
| Tests (target-net, `CONKER_ROM`, the full line) | pass: conker-rules 65 (new: `a_team_kill_scores_no_hit_kill_or_headshot`, `teams_rank_by_kills_then_fewer_shots`, `total_war_ends_by_team`) + combat 3 + oracle 4 + ROM 2; conker-bots 15 + oracle 3 + ROM 4 (new `total_war_runs_the_team_branch`: G+0xE64 = [0, 2, 0, 2], every threat on the other team, 0 unknown natives over 1,000 frames with the canister stub hit 346 times, the traitor flip and its reset at the death); conker-server 16 (new `total_war_teams_are_the_sides`, `--mode` parse cases); -sim, -client 40, -anim, -assets, -extract, extract-scene |
| Combat replays (Deathmatch) | output identical before and after (61 trace lines) |
| `MODE=total-war HUMANS=1 BOTS=5 bots.sh 240` (port 5970; 1 idle human, put on the Tediz, + 5 bots: 3 v 3 as the oracle's `bots3v3`) | enemy G+0x11C on the other team in 631 of 631 bot-seconds with one (oracle 867/867), target G+0x31C 162/162; 38 hits, all on enemies, 9 kills (oracle: 59 hits, 56 on enemies, 1 team kill, 2 own grenades); 42 pickups, 3 lift rides; 0 panics, unknown natives or faults. 15/16: bot 4, a sniper (0x24) at its role spot on the south tower 219 of 247 s with 5 kills, never died; bot 2 camped (state 10, 0x39) |
| `MAP=2d bots.sh 150` (Deathmatch, port 5974) | 6/7, 9 hits, 2 kills: as before (bot 1 without a hit, the known flake) |
| `cargo fmt --check`, clippy (rules, sim, bots, server, client, all targets) | clean |

Open: the takeover (script 0x4B16, command F 0xF = `func_1508E89C`: a human out, class 0xA, takes the healthiest
team-mate bot) never runs, since query 0x1D (participants of class ≠ 0xA) answers everyone, as in Deathmatch; the
client now ranks by `team_places`/`team_result` (the HUD batch above); command F 0x17 zeroes a forgiven traitor's +0x70 in
the game (`func_1508B1D4`, called only at 0xC58/0x4B72 for G+0x2034 set), the remake keeps `suicides` whole and the
bots count from a baseline (`World::forgiven`), so a results screen showing +0x70 would differ after a traitor's
death (no STATS column shows +0x70, so nothing differs on screen); no oracle run
(compared with the research's `bots3v3`); Deathmatch's per-join team (`joins as u8`) wraps after 256 joins (as before).

## Per-map solid placements and hidden props (2026-10-11)
**`WALL_PLACEMENTS` (0x2D's 10 and 11 on every map) is gone: the pack's collision holds each map's solid placements
only, and `CollisionWorld::layered` makes every placement it is given wall layer 2. 0x2D and 0x30 also leave out their
hidden props (0x30's Colors flags).** Evidence and tools: `../lab/oracle/walls_README.md`.

| Map | Solid (floors, walls, rays) | Hidden (out of the level and collision) |
|---|---|---|
| 0x2D | 2-7, 10, 11, 40-45 | 0, 1, 8, 9, 21-28, 37 |
| 0x30 | none | 0, 18, 19 |
| 0x34 | 0-12, 29-32 | none |
| 0x3F | none | 0-6, 15-22 (as before) |

The game's rule (C, V under lldb in s3/s4/s5/s7): layer 2 = props with `+0x6E` clear and `(+0x4F & 0x60) != 0x20`;
the match sets 0x20 on its pickups. `extract_scene::deathmatch_solid` holds the table; `tests/pack.rs` checks it equals
"collision flags, not hidden, not a pickup" from the ROM. `../assets` rewritten: `maps/{2d,30,34,3f}/collision.json`,
`maps/{2d,30}/level.gltf`, `README.md`.

| Check | Result |
|---|---|
| 0x2D (sim oracle tests) | unchanged: 28 traces, camera rays 0 of 2527 differ; wall zones: humans 0 of 161,317 rows differ, bots 6 of 80,625 (as before); wall trees match RAM |
| Grounded floors vs the game (new `floors_match_the_game_on_other_maps`) | 0x30 127 of 25,908 rows off (134 with every placement), 0x34 891 of 26,444 (=), 0x3F 17 of 26,477 (24) |
| Bots' sight (0x2D) | 139/142 (137) |
| `smoke.sh` | 2d, 30, 34, 3f: 16/16 each, prediction error 0 |
| Tests (target-net, `CONKER_ROM`) | conker-sim, -rules (combat 1157/1165, as before), -extract, -assets, -bots, -server, -client, extract-scene |
| Screens | `../lab/screens/maps-fix/`: 0x30 with and without the flags, 0x3F's former walls (placements 10, 11) |

Gaps: 0x34's boxes 6-9 stand 10 lower in the game while stood on (most of its 891 rows); 0x2D's 0 and 1 are shown for
the first 51 frames in the game; scenario 3 (0x2D) crashes the oracle at read 485 with 15 bots.

## Each map's own AI script, and the natives it calls (2026-10-11)
**Bots run their scene's AI script, from the pack's `maps/<map>/ai.bin` (0x2D 0xBA, 0x30 0xBB, 0x34 0xBD, 0x3F 0xBE),
with the level natives those scripts call. 0x30's bots fetch weapons and fight; 0x34's unarmed node loop is gone; on
0x3F bots draw and swing the club and pick up guns.** `PROTOCOL_ID` unchanged.

| Part | What |
|---|---|
| Pack | `maps/<map>/ai.bin` = `[0x14, conker_bots::script(scene)]` (conker-extract; conker-bots now a normal dependency); `Slice` reads it in place of `game/ai.bin`; the pack README. Added to `../assets`: `maps/{2d,30,34,3f}/ai.bin`; rewritten: `README.md`. `game/ai.bin` (0xBA, = `maps/2d/ai.bin`) is read by no code now but left in place (an old web build may still fetch it) |
| Root cause on 0x30 (V) | the AI's globals were 0x2600 bytes; 0xBB keeps its weapon choice at G+0x2658-0x26A8 (s5's RAM: the next block starts at G+0x26B0). Its writes were dropped, so each bot's wanted weapon (G+0x2438) read 0 = unarmed: no bot fetched one, all went straight to attack (state 7) or a sniper spot. `g::SIZE` = 0x26B0; G+0x1EC0-0x26B0 then matches the oracle's but for the random draws (roles, wanted weapons) |
| Natives (C; V as said) | type-0 handles → 0 (`func_150AE140`); player 0xA5 case 3 → 6 (stats +8, V: every player in s4/s5/s7 RAM); 0xAC: the first listed trigger the player is in, \| 0x4000 (`func_150A2AEC`); 0xBC → 0 (obj+0x13C, V: 0 in every dump); 0xEF → 1 (obj+0, V); 0xF0 → the player's place (`0x800E0AC0`, written by `func_151E565C`; V: the idle human's 1 → 14 in s4, bots 0). It was 0, which fired every script's rubber band ("a human leads and no bot has role 0: give one bot role 0") on every frame, on 0x2D too; type-4 queries 6 (the player is inside) and 16 (someone is) through `func_150A1DA0` mode 0 (`CameraHint::contains`; V: 79 of 80 set bits and every clear one of the type-3 triggers against the player bitsets `0x800D30F0` in s4/s5/s7 RAM); type-4 commands 0/1 (switch a trigger, `func_150A2864`); player command 0x7C (obj+0x23E, the arrival radius); prop 7 and sound 3 (a display flag, a music fade: no-ops) |
| `bot_match` | on 0x3F every player starts with the club (as the server's cavemen) |

| Check (2026-10-11) | Result |
|---|---|
| `bot_match`, 15 bots, 1,760 reads vs the oracle (s5, s4, s7 traces and RAM) | 0x30: 14/26/13 lift rides (seeds 1-3; oracle 26); longest stand-still per bot median 247 reads (oracle 155), max 1,675 (958). 0x34: median 187 (214), max 473 (499); unarmed 18% of bot-frames (oracle 32% of RAM samples). 0x3F: every bot holds the club, drawn 21% of bot-frames (oracle 52%), hits with 0x40 by 12 of 15 bots; 3 gun pickups |
| 0x30 lift loop | none: ≤ 4 rides per bot in 60 s, but for one 3-round bounce on lifts 2/3 (seed 2: 6 rides in 200 ticks, then none; the oracle's p15: 4 in 380) |
| 0x34 unarmed | no node loop; 5 bots 5 min: longest unarmed stretches 53-141 ticks, but one bot camped unarmed 3,395 ticks at a state-9 spot (node 39; gaps) |
| 0x2D | 391/391 node changes; sight 139/142 (137 before, from the collision agent's solid-placement filter in `tests/oracle.rs`, not this change); bot tests 15 (new `trigger_natives`). The 0xF0 fix changes 0x2D matches (the rubber band now waits for a leading human) |
| `bots.sh` (ports 5972-5988) | default 2 bots 150 s: 2d 6/7 (bot 1 no hit, the old flake), 30 5/7, 34 6/7, 3f 6/7 (each FAIL: a bot that hit but never died). 0x30: 3 bots 120 s 8/10, 27 hits, 4 kills, 12 pickups, 8 rides; 5 bots 150 s 15/16, 34 hits, 5 kills, 22 pickups, 10 rides (0xBA's, last section: 27/39 hits, 4/7 kills, 12/7 pickups, 19/37 rides). 0x34: 5 bots 150 s 16/16, 50 hits, 8 kills, 28 pickups (54, 13, 32); 3 bots 120 s, two runs: 7/10 and 8/10, 9 and 14 hits, 0 and 2 kills, 12 and 18 pickups (26, 4, 20: campers, gaps). 0x3F: 3 bots 150 s 10/10, 39 hits, 6 kills, 2 gun pickups |
| `smoke.sh` | 2d (port 5965) and 3f (5970): 16/16 each, prediction error 0 |
| Fresh export (`--map 2d --map 30 --map 34 --map 3f`, four `--background`s) | equals `../assets` for `ai.bin` ×4 and `README.md`; differs in `game/ai.bin` (kept, above) and in `maps/*/collision.json`, `maps/{2d,30}/level.gltf` (the collision agent's exporter work in progress, not merged into the pack) |
| Tests (target-net, `CONKER_ROM`) | conker-bots (lib 15, oracle 3, rom 3), conker-assets, conker-extract `pack`, conker-server; clippy clean |

Gaps:
- 0x30: bots sent to sniper node id 48 stall against the wall the oracle's bots climb (state 235, a pull-up at
  (1096, −624, −494)); the remake's bots don't climb ledges.
- Camping: with few bots, 0xBD/0xBB bots stay in a camping state (9 or 1) far longer than the oracle's (a role changes
  only on a death or the rubber band here; in s4 role-4 bots changed role by DL 700 with no death): `func_1508A6FC`'s
  returned mask is ported as deaths only (I).
- Query 16's camera test and camera handles on query 6 read "outside" (the views are the clients').
- The zone test takes this frame's position (the game's bitset is from the start of the player's last update);
  shapes 1 and 3 never fire (`Body::old` = pos; no type-3 trigger of the four maps uses them). 0xA5: case 3 only.
- `(6, 3, 1)` (prop command 1) on 0x2D, 6 calls a match, as before.

## The caveman (class 0xB, model 136) and item 0x40, the club (2026-10-11)
**Scene 0x3F's Deathmatch (scenario 7) as the original sets it up: caveman bots, Tedi humans, and every player
holding the club from the start and after each respawn; `conker-server --cavemen` does the same on any map.**
Oracle findings and tools: `../lab/oracle/caveman_README.md` (`CONKER_MP_CHARS`, the `combat6_*` traces).

| Part | What |
|---|---|
| Rules | `weapon::CLUB` (draw 0x2C2, swing 0x113 with its hit at 10, the second swing `CLUB_COMBO` within `COMBO_WINDOW` of a hit, walk/run 0x112/0x111, C 0x112); `damage::CLUB_SWING`/`CLUB_COMBO` (0x25/0x26) and their reactions; `Fighter::start` (MP+0x1170: given back whenever the player holds no weapon), `Fighter::combo`; an A pressed in a melee swing's hold is spent, a C press in a jump or landing ignored |
| Sim | `Side::Cavemen` (the squirrel's body for hits and pushes); `PROTOCOL_ID` 12 |
| Server | On 0x3F (or `--cavemen`): bots `Side::Cavemen` in the squirrels' base (the Ugas' on 0x3F), humans Tediz unless they ask, `start` 0x40 for all |
| Pack | `characters/136/` (lift 49, the Tedi's clips), `audio/characters/136/` (16 WAVs); `held.{gltf,bin,json}` rewritten with the club's model (entry 122); `weapons/deathmatch/textures/3715-…png`; entries added to `pack.json`, `audio/events.json` (the club's clip sounds, 136's events, hit voices for 0x40), `audio/sounds.json`, `hud/hud.json` (icon 0x40); `README.md` |
| Client | the third character (render, sounds, `side=cavemen`), the club in the hand, its HUD icon, hit voices |
| Bots | `weapon_class(0x40)` = 3 (melee) |

| Check | Result |
|---|---|
| Combat replays (`combat6_*`) | ctlw40 35/35 events, pos 0.009; e40 46/46, pos 0.04, kill 1/1; e40c (cavemen) 46/46, pos 0.04 (5.1/9.0 as Tediz); ctlw40c 31/35 (the caveman's grenade clips, known); 0 armed-rate error over 44/20 rows |
| Caveman movement vs the Tedi (oracle) | 10 scripts, 4,728 reads: 0 differing game values (`tools/compare.py`) |
| Tests (target-net) | conker-rules (61 + combat + oracle), -sim, -anim, -bots, -server (15, new: `cavemen_bots_hold_the_club`), -client (40), -assets, -extract (pack.rs with the ROM); wasm check; clippy |
| `smoke.sh` | 2d (port 5934) and 3f (5936): 16/16, prediction error 0; on 3f every human holds 0x40 |
| Native runs | `../lab/screens/caveman/shots.sh`: 2d `--cavemen` and 3f with 3 bots: `2d-tedi.png`, `3f-tedi.png` (the swing, the HUD icon), `2d-view26.png`, `3f-view26.png` (a caveman) |

Gaps: the bots hold the club but never draw it (their AI script; 0x3F's own script is not run); the caveman's grenade
clips 0x175/0x16C (the Tedi's play); the club icon's 1-pixel offset; a lethal first swing is untraced; the club's
and the caveman's sounds are in the pack but were not listened to (native runs only, offscreen).

## Deathmatch on scene 0x3F, the Temple (2026-10-11)
**`conker-server --map 3f` plays Deathmatch on scene 0x3F (the original's scenario 7); the clients load it through
`/info` as for 0x30 and 0x34, which are unchanged.** It plays with today's characters; scenario 7's cavemen and club
(item 0x40) are another agent's work.

| Step | What |
|---|---|
| Oracle (`../lab/oracle/map3f`, a private copy of the oracle; runners in `tools/`) | Scenario 7: 1 human + 15 bots (60 s, RAM dumps at DL 100-1700) and 2 + 3 (8 s). Scenario 2 (Raptor): 2 + 14 (70 s, RAM at DL 100-500). Deathmatch start by slot = `(slot + 7) % 12` on 16 records, 0x34's table, in both layouts (V); masks all ones (V, 17 respawns). 10 pickups, respawn 240 VIs (V): the weapons are the props with ids 14-21 (placements 7-14: 0x22, 0x23, 0x24, 0x09, 0x21, 0x37, 0x39, 0x15), the grenades ids 4 and 5 (placements 23, 24); all stand on their placements at 3-57 s. Placements 0-6 and 15-22 stay hidden (+0x6E) all match: Raptor's 8 bone weapons on the 8 weapon spots, 3 props at the west nest, 4 scene objects at (6026, 156, 2188). Raptor: team 0 (Ugas, class 0xB) starts on record 8 + k, team 1 (raptors, class 9, model 83) on record k; masks 0xFFFFFF00 and 0xFFFF00FF (V, 13 respawns). Ambience: **song 84**, no channel muted, volume 24000 (`func_1000E40C`, called every frame from `func_1000D758`), no emitter in 25 s. Surface kinds 4, 5, 7, 9, 0xA; **7 is new** (70 faces, the stream bed at x −6517..−5475, z −2555..−1564, floor −10): bot 8 crossed it at 17.5, half of 35 (reads 1339-1458), as `SINK[7]` = (45, −5, 50) says, which the sim already applies. Trigger kinds 5 (2, by the stream and the nest) and 18 (1, east) are new; no lift |
| Per slot (for the club's port) | Humans: class 3, model 117. Bots: class 0xB, model 136. Everyone's s126+0x75 = 0xC0 from DL 100 (0x34's unarmed Tedi: 0) |
| Oracle crash | Scenario 7 crashes the oracle 31 frames after the human dies (read 245 after the death at 214, native and under lldb): `func_15042ECC` (HUD layout) → `func_151EFF94` (sprintf) → strlen of an address outside RDRAM (I: the caveman killer's name). The 60 s capture keeps player 0's health at 6 (`combat_lldb.py` pokes every 2 reads); the bots die and respawn normally |
| Sides | Raptor's bases, as Colors' on 0x30: the squirrels on team 0's records 8-15 (east), the Tediz on team 1's 0-7 (west, by the nest), each respawning in its own half (I, the remake's choice) |
| Rules | `spawn::DEATHMATCH_START_3F` (= 0x34's), `RAPTOR_START_3F`, `RAPTOR_MASKS_3F`, `starts(0x3F)`; `weapon::PICKUPS_3F`, `pickups(0x3F)` (no spots) |
| Pack | `conker-extract pack --map 2d --map 30 --map 34 --map 3f --background 3f=../lab/audio/map3f/background_loop.wav …`; `hidden_placements(0x3F)` leaves the 15 hidden placements out of the level and the collision (4,251 → 2,683 faces; the collision part I, from 0x2D's RAM); `map_sounds`: sequence 84, channels 0-15, volume 24000. Added to `../assets`: `maps/3f/` (7 files, 73 textures), `audio/maps/3f/background.wav`, `audio/common/0672-0682.wav` (kind 7's footsteps); `pack.json`, `audio/events.json`, `audio/sounds.json` gained the new entries only (each written as the fresh export once the merge equalled it), `README.md` is the new template. A fresh four-map export equalled `../assets` file for file (896 files) at the merge, before the caveman agent's exporter changes (character 136, the club's held model), which need their own merge |
| Background loop | Song 84 never repeats sample for sample in 900 s (an untimed run); its loudness repeats every 191.82 s (envelope correlation 0.51, also at 2 and 3 periods). `background.wav` is one period cut at 386.15 s, digital silence at both ends (zero runs of 151 and 2,810 frames), 16.9 MB |

| Check (2026-10-11) | Result |
|---|---|
| `conker-rules` oracle test | 20 captures (3 on 0x3F): 253 placements + 100 respawns (1,842 RNG draws), 0 mismatches; 151 start records, 100 masks (s2's against the sides' tables); 20 traces, 179 rows; unit test: 0x3F's sides are two disjoint bases |
| `conker-extract` `pack.rs` (ROM) | 0x3F reads back (collision less the hidden placements, 16 spawns, pickups on their placements, nav, triggers, glTF); audio: kind 7's footsteps, no emitter, sequence 84 at 24000 |
| `MAP=3f smoke.sh` (ports 5901, 5902) | 16/16 on a fresh export and again on the merged `../assets`, prediction max error 0 (685/685/271 and 684/682/269 ticks) |
| `MAP=3f bots.sh` (5 bots 150 s; 3 bots 120 s) | every bot moves (65-157 positions/s); 5 and 4 pickups; 10 and 9 hits; 1 and 2 kills; the script's fights-dies-respawns check fails as on 0x30 and 0x34 (4 of 5 and 3 of 3 bots without a death): the bots run 0x2D's AI script, 0x3F's is 0xBE (the bots section above) |
| Browser (`/tmp/web-map3f`, headless Chrome) | WebTransport and WebSocket, `/info` (`{"map":"3f","protocol":12}`) then `maps/3f/`: `../lab/screens/map3f/browser-wt.png`, `browser-ws.png` (walking) |
| Tests (target-net) | conker-assets 6, -bots 14 + oracle 3 + rom 3, -client 40, -extract (ROM) 5 + `pack` 4, -rules 62 + combat 3 + oracle 4 + rom 2, -sim 45 + oracle 11, -server 15 pass; -extract `rom.rs` `slice_in_memory` fails on its 13 held models, the club's in-flight 14th (the caveman agent's) |

Gaps:
- The cavemen and the club (above, per slot) are the caveman agent's; the oracle cannot show a human's respawn on 0x3F
  (the crash).
- Trigger kinds 5 and 18: nothing reads them (C, V; corrected 2026-10-13, `dm-maps-results.md` §3a). The per-frame
  music volume call is song 0x54's hook `func_1000CA18`, a constant 24000 off scene 0x37 sent every frame: the remake's
  24000 already (§3b). The 6 action-1 records are the floor search's zones (the top section), not camera volumes.
- `conker-sim`'s `WALL_PLACEMENTS` (10, 11) are 0x2D's on every map: on 0x3F they are the twin guns' and the katana's
  models (on 0x30 the same placements are pickups too). 0x30's Colors flags (placements 18, 19) and its placement 0
  are hidden in its Deathmatch but still drawn there (left as they are: 0x30 unchanged). Both fixed since (the top section).

## Bots on 0x30 and 0x34: the placement replan, and each scene's own AI script (2026-10-11)
**A placement (match start, respawn, lift) now makes the AI replan on the next frame, as in the game: 0x30's
every-36-ticks lift ping-pong is gone. The deeper cause of both loops is found, not fixed: the remake runs 0x2D's AI
script on every map.**

| Finding | What |
|---|---|
| Placement replan (V, fixed) | `func_15085430` (every placement) sets obj+0x22C bit 0x200; the next frame `func_1508A6FC` clears it and sets state 0 and a replan (G+0x1C), as a hit does. Oracle scenario 5 (`../lab/oracle/trajectories/bot_s5*_lldb.txt`): the flag set by the lift's placement and cleared the next frame on all six rides watched (p6 at reads 323, 416, 539, p8 at 48, 499, 540), state 6 → 0 with it (watched at p6's 324); then the script resets G+0xDC to −1 and picks again (p6, p8, p15 after their rides; p6 goal 90 → 101 → 90). Ported as `Npc::placed`: set by command 0x5A (the start), `lifted`, `Event::Respawn`; taken in `frame` |
| Scene scripts (V, not fixed) | The game runs `[0x14, id]` per scene: 0x2D 0xBA, 0x30 0xBB, 0x34 0xBD, 0x3F 0xBE (G+0x16BC in the RAM of s5, s10, s4 and map3f s2); the pack has one `game/ai.bin` (0xBA). 0xBA's G+0x24D0 lists 16 pickup nodes of 0x2D for an unarmed bot to head for (C: an unarmed bot never searches the wanted weapon's pickups, GIVES[−1] = G+0x1170 = 0). On 0x30 its id 29 lies inside lift 4's box (the reported loop); on 0x34 ids 31 and 61 are bare corridor nodes: the bot walks there, finds nothing, cools down 300 VIs, wanders, repeats (the nodes 60-62 loop). Not the original's: its 0x34 bots stayed unarmed up to ~900 frames of 60 s, moving about (RAM at DL 300-1700, trace) |

| Check (2026-10-11) | Result |
|---|---|
| 0x2D start | all three bots in state 6 at read 24, as the oracle's `bot_states_lldb.txt` (was 24/25/26) |
| `MAP=30 bots.sh` (3 bots 120 s; 5 bots 150 s) | 27 and 39 hits, 4 and 7 kills, 12 and 7 pickups, 19 and 37 lift rides (before: 7/8 hits, 2/5 kills, 4/16 pickups, 103/190 rides; a 5-bot rerun before the fix today: 10, 1, 5, 114). Rides come in bursts of 2-5, 40-300 ticks apart (the oracle's 41-150). 15 bots 60 s: 47 rides (oracle scenario 5: 25), but slot 1, unarmed with goal node 29, still rides lifts 4/2 every ~290 ticks |
| `MAP=34 bots.sh` (3 bots 120 s; 5 bots 150 s) | 26 and 54 hits, 4 and 13 kills, 20 and 32 pickups; 9/10 and 16/16 checks (before: 16/39 hits, 3/6 kills, 17/33 pickups). In the 5-bot run bots 0 and 3 were still unarmed for 2,070 and 2,790 ticks, cycling states 3/6 on goals 61 and 31 (0xBA's ids) |
| `MAP=2d bots.sh 150` | 9 hits, 2 kills, 12 pickups, 6/7 (bot 1 no hit, the old flake). `bot_match` 3 bots, seeds 1-5: kills 1/10/6/12/10, rides 1/6/2/15/12; with the flag off 5/11/10/9/7, 6/1/8/13/10: the same spread |
| Tests (target-net) | conker-bots (new `a_placement_replans_once`; oracle 391/391 node changes, sight 137/142; ROM), conker-extract `pack` (ROM) pass; clippy clean |

Also: `conker_bots::script(scene)` and `Assets::paths` name each scene's script; `bot_match` takes `BOT_SCENE=30|34`
(it then runs that script with its level natives answering 0, so neither the game's numbers nor the server's);
`../lab/oracle/tools/bot_lldb.sh` takes `SCENARIO`; the three scripts are in `../lab/extract/bot/lscript_14_{bb,bd,be}.bin`.

Gaps:
- Per-map AI needs a pack change (`maps/<map>/ai.bin` = `[0x14, script(scene)]`, loaded per map) and the natives those
  scripts call: player queries 0xAC (`func_150A2AEC`, a trigger the player stands in, as a type-4 handle), 0xBC
  (obj+0x13C), 0xA5; type-4 handle queries 6 and 16, commands 0x7C, prop 7, type-4 0. Until then 0x30's and 0x34's
  unarmed bots still loop on 0x2D's node ids (above). Done since: "Each map's own AI script" (top).

## The crossbow 0x18 and the shotgun 0x37 (2026-10-11)
**Both weapons play: pickups (0x34's, or `--give SLOT:READ:0x18`), draw, fire, hits and kills in the rules, the bots,
the server and the client (held model, clips, sounds, HUD icon).** Oracle tools and findings:
`../lab/oracle/combat5_README.md`.

| Step | What |
|---|---|
| Rules | `Attack::Bolt` (hip 0xFC: a bolt at frame 10 auto-aimed by the lock-on from the muzzle; R aims 0x10A, a Z release shoots 0x10C from the scope's eye; 125 a frame, cooldown 30) and `Attack::Spread` (0x1F4 or, on an RNG draw, 0x235; four spheres resolved after every player's update; cooldown 60; draw/holster/fire lock the movement); damage types `SHOTGUN_*`, the area stagger `AREA_STAGGER`, a lethal body hit's sequence; `lock_on` takes origin, range and cone |
| Anim | draw/idle/holster/fire/aim ids, armed walk and run (0xEC/0xF8 at 80 %, 0x1F5/0x1F6), shotgun draw rate 1.4 |
| Bots | they take, draw and fire both (the crossbow from the hip; its scope as weapon 0x14's, I); `bot_match` `BOT_GIVE=new:old` swaps one pickup's weapon |
| Pack | held models 11 → 13 (the shotgun's bank-09 entry 102; the crossbow is a child object, bank-01 model 110, drawn in its bind pose); HUD icons 0x18, 0x37; sounds 1090, 1091; clip sounds of the new clips (incl. op 144, the shotgun's blast); pickup, hit and voice cues |
| Client | the held models, `hit_weapon` knows both, bolts drawn and heard as aimed shots (placeholder) |

| Check (2026-10-11) | Result |
|---|---|
| `tests/combat.rs` | 57 cases pass (47 old, 10 new): ctlw18 18/18 events, cb3 55/55, cb4 23/23, cb_e1 22/22 + kill, cb_e2 39/39 + kill (6 bolt hits), ctlw37 20/20, sg3 33/33 (17 RNG draws fed), sg_e1 13/13 + kill, sg_e2 37/39 + kill (2 known), sg_e3 16/16 + kill; position errors ≤ 0.44 but the crossbow shooter's (below) |
| Tests (target-net) | conker-rules 60 + combat + oracle, -sim, -bots, -anim, -server, -client 40, -assets, -extract (ROM) pass; clippy clean; wasm32 check |
| `bot_match` (seed 7, 9000 frames) | `BOT_GIVE=18:09`: crossbow drawn 4,750 frames, 7 hits; `37:09`: shotgun 867 frames, 6 hits |
| `smoke.sh` (port 5901) | 16/16, prediction max error 0; `MAP=34 BOTS=5 bots.sh` (port 5910): every bot fights, dies, respawns; the shotgun taken 9 times |
| Native (`../lab/screens/weapons2/shots.sh`, cb_e2 and sg_e1 replayed) | crossbow: 6 hits, the last lethal, each a frame after the oracle's (the server's reads 852-1053 for the trace's frames 851-1052, below); shotgun: the kill in frame 790 as the oracle's; `crossbow-p1.png`, `crossbow-side-p1.png`, `shotgun-p1.png` |

Gaps:
- Firing the crossbow from the hip moves the shooter in the game (~5.3 a shot; cb3 drifts 69 over 15 shots, cb_e2 19.8),
  not modelled; the victim's facing then follows the drifted angle (up to 332).
- The crossbow in the hand doesn't play its own clips (string); bolts sound like weapon 0x14's aimed shots.
- A bolt first moves on an odd match tick (V on the replays, where tick = read); a server started with `--first-read 745`
  has tick 388 there, the other parity, so its bolts land a frame later than the oracle's (play is unaffected).
- The shotgun's point-blank sphere (0x3A) never traced a hit; player 2's back-steps in sg_e2 (conker-sim).

## Deathmatch on scene 0x34, the Bunker (2026-10-11)
**`conker-server --map 34` plays Deathmatch on scene 0x34 (the original's scenario 4); the clients load it through
`/info` as for 0x30; `--map 2d` and `30` are unchanged.** Its crossbow (0x18) and shotgun (0x37) pickups are in: until
those weapons are ported, a player who takes one holds an id the rules don't know, which never draws (`Fighter::weapon`
is `None`: no fire, no panic; the HUD prints `weapon 0x37`).

| Step | What |
|---|---|
| Oracle (`../lab/oracle/map34`, a private copy of the oracle) | Scenario 4, 1 human + 15 bots (60 s, RAM dumps at DL 100-1700) and 2 + 3 (8 s). Deathmatch start by slot = `(slot + 7) % 12`, `[7,8,9,10,11,0,1,2,3,4,5,6,7,8,9,10]`, in both layouts; masks all ones (19 respawns). 16 slots on 12 records: slots 0 and 12 share record 7 and push apart. 16 pickups, respawn 240 VIs (V, RAM at 3, 10, 57 s): pickup k is the prop with id k + 1 and stands exactly on its placement (no spots, as 0x2D): placements 13-28. Ambience: **song 36** (not 19), channels 0-5 (mute 0xFFC0), no emitter in 25 s (`../lab/audio/map34`). Surface kinds new here: 6 (77 faces, the low floors, footsteps 606-616 and 683-686) and 0xE (35 faces, a 24° slope in x −3719..−277, z −4438..1227: no footsteps, sink −100). No lift (type-8) trigger. The mounted gun: one model-138 object at (2428, 645, 537) (RAM, as 0x30's), left out |
| Sides | No War mode on 0x34, so no team records: `starts(0x34).sides` is the Deathmatch table split by slot parity (squirrels the even slots' records, Tediz the odd ones'), masks all ones (I, the remake's choice). With sides alternating by slot (the bots) that is exactly the Deathmatch table; a side only changes the model |
| Rules | `spawn::DEATHMATCH_START_34`, `starts(0x34)`; `weapon::PICKUPS_34`, `pickups(0x34)` (no spots) |
| Pack | `conker-extract pack --map 2d --map 30 --map 34 --background 34=../lab/audio/map34/background_loop.wav …`; `map_sounds` names each map's sequence (36 on 0x34). Added to `../assets`: `maps/34/` (7 files, 120 textures), `audio/maps/34/background.wav`, `audio/common/0606-0616.wav`; `pack.json`, `audio/events.json`, `audio/sounds.json` gained the new entries only (each written as the fresh export once the merge equalled it) and `README.md` is the new template. A fresh three-map export equals `../assets` file for file |
| Background loop | Song 36 never repeats sample for sample (two runs differ from 0.8 s; no 64-frame window recurs in 340 s or in a 900 s untimed run), but its loudness repeats every 85.35 s (envelope correlation 0.97, also at 2-5 periods). `background.wav` is one 85.35 s period cut at 171.0 s where both ends are near silent (mean \|x\| 3-4) with a zero jump at the seam: not sample-exact like 0x2D's and 0x30's |

| Check (2026-10-11) | Result |
|---|---|
| `conker-rules` oracle test | 17 captures (2 on 0x34): 211 placements + 70 respawns (1,431 RNG draws), 0 mismatches; 114 start records, 70 masks; 17 traces, 138 rows (the read-30 idle check now skips humans sharing their record); unit test: 0x34's sides are its Deathmatch table by parity |
| `conker-extract` `pack.rs` (ROM) | 0x30 and 0x34 read back exactly (collision, 12 spawns, pickups on their placements, nav, triggers, glTF); audio: kinds 6 and 0xE, no emitter, sequence 36 |
| `MAP=34 smoke.sh wt ws ws` (ports 5956, 5960) | 16/16 twice, prediction max error 0; `MAP=2d`, `MAP=30` 16/16, max error 0 (one 0x30 run under load from a parallel bots run had max error 2,103 with 2 state misses; two reruns alone: 0) |
| `MAP=34 bots.sh` (3 bots 120 s; 5 bots 150 s) | every bot moves (106-153 positions/s); 17 and 33 pickups (crossbow and shotgun taken in both); 16 and 39 hits, 3 and 6 kills; the script's fights-dies-respawns check fails as on 0x30 (run 1: 1 bot without a hit, 2 without a death; run 2: 2 without a hit, 2 without a death). Records 3 (21 under a floor at −761; the step-up, 50, covers it as the oracle does, not watched tick by tick) and 6 (106 above its floor) start fine |
| Unknown weapons | `--give 0:100:0x18 --give 1:100:0x37`, 2 bots and 2 headless clients for 24 s: no panic in the server or the clients; the bots carry them holstered, and a human client that walked into the shotgun held it holstered through 23 HUD samples |
| Browser (`/tmp/web-map34`, headless Chrome) | WebTransport and WebSocket, `/info` (`{"map":"34","protocol":11}`) then `maps/34/`: `../lab/screens/map34/browser-wt.png`, `browser-ws.png` |
| Tests (target-net) | conker-assets, -extract (ROM), -rules 59 + oracle 4, -sim, -bots, -server 14, -client 40 pass |

Gaps:
- The crossbow and the shotgun need their port (another agent's work); the mounted gun is left out.
- A bot can loop for a long while near its start: in the 5-bot run bot 0 stayed unarmed all match, running and
  jumping (state 55) between nav nodes 60-62 by record 7, later 28/29 on the low floor; untraced.
- Corrected 2026-10-13 (the top section): kind 0xE is the burning cones (a hit, a hop, set alight; `func_1507CD64(obj,
  0xD)` is the death clip 0x2D8 when a death starts grounded on one); kind 12 is the traps that placements 6-9, the
  pressure plates (all four sink 10), switch on; kind 17 (record 56) is read by nothing in Deathmatch. The 33 action-1
  records are the floor search's zones; the camera script's zone (0x6D) only switches off the occlusion swing, which
  the remake lacks.

## Match settings, the end and the results (2026-10-11)
**The server takes the original's SET-UP options (`--time 3-10|unlimited`, `--lives 1-5`, `--kills 0-10`, `--skill
inbred..einstein`, `--radar on|off`; defaults 10 min, 3, unlimited, NORMAL, on) and ends the match as
`func_151E565C` does: the clock at 0, a player at the kill limit, fewer than 2 in play or no human in play once someone
is out of lives. After the fade every client draws the original's STATS over the frozen level; the next match (same
settings) starts once every human chose RESTART, or 40 s after the end; QUIT leaves (browser: `index.html`).** Untimed,
the HUD has no clock and shows the lives left (the original's digit); a player out of lives stays down under a skull.

| Part | What |
|---|---|
| Rules | `Settings`, `Match::with`, `ends`, `places` (kills − deaths, the kill limit's holders +1000 once reached, then fewer shots s126+0x1AA; tied humans share a place); `Fighter` `deaths`, `shots` (+1 per round, swing, grenade pull), `hits`, `headshots`, `out`; 5 bot skill levels (tables at 0x800872B0) |
| Net | `PROTOCOL_ID` 11: `MatchView` time, lives, kill limit, radar, `restart_in`; `Combat` deaths, shots, hits, headshots, out, `lives_left` (drops at the respawn, as `func_15085710`); `Restart` message (client → server) |
| Pack | `conker-extract pack` adds `hud/end.json` (lives digits, skull, STATS layout) and 11 PNGs (RGBA32 tiles 0xD14, 0xD0F, 0x816, 0x80E/0x810, 0x7FC/0x7FE, 0x7E8, 0x888, 0x887, 0x843); `hud.json` unchanged. Copied into `../assets/hud/` (new files only), `README.md` gained the section |
| Client | `hud.rs`: lives digit, skull, results (a row per human: place, accuracy, kills, headshots, deaths; RESTART/QUIT lit by the menu); `results.rs`: the menu (stick left/right toggles, A chooses) |

| Check (2026-10-11) | Result |
|---|---|
| Oracle end timing (`../lab/research/mp-original`, `CONKER_MP_OPTS`) | kill limit 1: lethal hit 118, phase 7 at row 159 = the remake's `ended_at` + 41 (the first `Fading` row); time-out the same; lives 1 (1 human + bot): the human out at 384, phase 7 at 424; results auto-quit after 0x961 VIs = 40 s |
| Pack | the 11 PNGs equal the research's own decode (`../lab/hud/out/end`) pixel for pixel; a fresh `hud.json` equals `../assets`'; `pack.rs` (ROM) reads `end.json` back |
| e2e, native | `--bots 1 --time unlimited --lives 1 --skill einstein`: the human out at t 2097, next match at 3506 by the 40 s deadline; with the script's A taps: "slot 1 votes to restart", next match at 2427; `--bots 2 --kills 1 --lives 2`, 2 offscreen clients: the kill at t 1996 ends it, `../lab/screens/end/results.png` (P1 2nd 0% 0 0 1), `untimed-lives.png` |
| `smoke.sh` (port 5911) | 16/16, prediction max error 0 |
| Tests (target-net) | conker-rules 58 + combat replays 3/3 (ROM), -bots, -sim, -server 14, -client 40, -assets, -extract (ROM) pass; clippy clean; wasm `cargo check` passes; no browser run |
| Chrome (after both agents, `web/build.sh web`, 4,423,488 B brotli) | `--kills 1 --bots 3` on 0x2D: the STATS screen over the level (`../lab/screens/end/chrome-results.png`), a held Space = "slot 3 votes to restart" and the next match at once; on 0x30 the match plays (`chrome-map30-1.png`). A DevTools key tap (down and up in the same tick) is no press: the menu reads the pad per tick, like the game |
| Next match's RNG | with `--kills 1` no respawn ran the rules' RNG, so every next match replayed the last (the same grenade at read 213); the next match now goes on from the bots' RNG (`Bots::rng`) |

Gaps (corrected 2026-10-13 from `../lab/research/mp-original/dm-maps-results.md` §5; see "The results screen's
details"):
- The skull grows as the original's (`hud::skull_byte`, E5: 4 px on the out frame, 32 from the 14th). Since E10 the out
  view's black covers the HUD, as in one view; the level still draws under it (the original skips its 3D pass: the
  same picture). Since E10 the lit button pulses and the stick moves.
- Since E10 the end phase hides the radar, score and rank (HUD flags `&= 0xE35F`), and the clock a frame later.
- `hits` counts as the original's `gGameState+0x66` (C, its one writer `func_151E5FF4`): +1 per damage run by a human of
  another team, so per bullet, per victim per swing or blast, per burn tick, never a self-hit. Left: the flame counts no
  shots; headshots only from rays and aimed shots (the original counts every reaction-0x2E hit). P3/P4's row colours are
  the original's (C: the table 0x800ABA90, green 00ff00, yellow ffff00).
- RESTART's 17-frame fade: the client draws it from `MatchView` phase 3 (E10), which the server publishes since (the
  top section); the deadline still starts the next match at once.
  A finished match with no human left waits for one to join. Ties: humans with equal score and shots share a place and
  bots get none, as in the original (the slot clause in `func_151E565C` is dead code; this note's "tied bots by slot"
  was wrong).

## Deathmatch on scene 0x30 (2026-10-11)
**`conker-server --map 30` plays Deathmatch on scene 0x30 (the original's scenario 5); `--map 2d` (default) is
unchanged.** The server's token endpoint answers `GET /info` with `{"map":"30","protocol":11}`; native clients and the
page fetch it first (`connection::info_url`/`info_map`; `map=` skips it), then load `maps/<map>/`. Caddy proxies `/info`
with `/token`. The client's level, sound (background, emitters) and collision follow the server's map; `PACK_MAP`
remains only as `--map`'s default name (and in a held-weapons test).

| Step | What |
|---|---|
| Oracle (`../lab/oracle/map30`, private copy of the oracle) | Scenario 5 (1 human + 15 bots) and 10 (War: Colors, 2 + 14, 70 s): `spawn_lldb` + traces, RAM dumps. DM start by slot `[12,14,10,15,8,4,9,5,1,21,18,22,19,23,20,24]`, masks all ones (V). Colors: team 0 = squirrels (class 4, model 128), green flag (131, 1342, 6144), starts `[12,10,11,13,6,7,8,9]`, mask 0x3FC0; team 1 = Tediz, red flag (−330, 410, −3803), `[14,15,16,17,2,3,4,5]`, mask 0x3C03C (V; per-rank order I). 17 pickups, respawn 240 (V, RAM); the game moves them off their placed models by 2-344 units (`SPOTS_30`, RAM; ROM source untraced). New surface kind 0x11 on 59 faces: sink 0, no slide class, only its footsteps. Ambience: song 19, channels 0-5 (mute 0xFFC0); no emitters heard in 25 s |
| Rules | `spawn::starts(scene)` (`Starts`: DM table, the sides' records and masks: 0x2D Total War's, 0x30 Colors'); `weapon::pickups(scene)` (`PICKUPS_30`, `SPOTS_30`) |
| Pack | `conker-extract pack --map 2d --map 30 --background 2d=… --background 30=…` (repeatable; default 2d only). Added to `../assets`: `maps/30/` (113 textures), `audio/maps/30/background.wav` (one loop period, seam sample-exact, `../lab/audio/map30`), `audio/common/0650-0660.wav`; `pack.json`, `audio/events.json`, `audio/sounds.json` and `README.md` gained the new entries only. A fresh two-map export equals `../assets` file for file |
| Bots | Lifts of shape 2 (oriented boxes, 10 on 0x30) via `Lifts::new(triggers, scene)`; the pickup-gives check no longer reads 0x2D's table |
| Mounted guns | Two model-138 objects (hp 15) at (−322, −387, −3621) and (170, 544, 5938), present in DM: touching one mounts it (player state 161, the camera moves to the gun), Z fires (state 164), B does not dismount; damage untested. Not implemented |

| Check (2026-10-11) | Result |
|---|---|
| `conker-rules` oracle test | 15 captures (2 on 0x30): 187 placements + 51 respawns (1,187 RNG draws), 0 mismatches; 93 start records, 51 masks; 15 traces, 115 rows |
| `conker-extract` `pack.rs` (ROM) | both maps read back exactly (collision, 25 spawns, pickups = `PICKUPS_30` + `SPOTS_30`, nav, triggers, glTF); audio: kind 0x11 footsteps, 0x30 without emitters |
| `MAP=30 smoke.sh wt ws ws` (port 5940) | 16/16, prediction max error 0 (685/686/268 ticks); `MAP=2d` (5946): 16/16, max error 0 |
| `MAP=30 bots.sh` (3 bots 120 s; 5 bots 150 s) | every bot moves (61-156 positions/s), 4 and 16 pickups, 7 and 8 hits, 2 and 5 kills, 103 and 190 lift rides; the script's every-bot-fights check fails on this larger map (2 and 3 bots without a hit) |
| Browser (`/tmp/web-map30`, headless Chrome) | WebTransport and WebSocket, `/info` then `maps/30/`: `../lab/screens/map30/browser-wt.png`, `browser-ws-overview.png` |
| Tests (target-net) | conker-assets, -extract, -rules, -sim, -bots, -server, -client pass; clippy clean in this work |

Gaps:
- **Bots loop on lift pairs:** 1-2 bots per match ride a pair back and forth all match (e.g. lifts 4 ↔ 7, every 36
  ticks): their goal, node 29, lies inside lift 4's box, so the lift fires before they arrive and the AI keeps the goal.
  The original's bots ride the same pair too (s10 player 9: 4 rides in 380 reads) but wander 100-150 reads between
  rides; why the original's AI changes its goal is untraced.
- 0x30's camera script zones (0x3C, 0x3D) only switch off the depth-buffer occlusion swing, which the remake lacks: no
  port (2026-10-13). Its 8 action-1 records are not camera volumes but the floor search's zones, ported since (the top
  section). The mounted guns are left out. No team modes (Colors' tables are only used for the sides).

## HUD (2026-10-11)
**The match shows the original's Deathmatch HUD, natively and in the browser, from the pack's `hud/`: chocolate health
(top left), kills and place `0 (1st)` (top centre), the held weapon's icon or the grenades in the hand and the radar
(top right), the clock (bottom centre), the aim reticle, the scope, and the death iris.** One implementation
(`crates/conker-client/src/hud.rs`): textured quads (unlit, alpha-blended `StandardMaterial`s) on render layer 1 under
a second, orthographic `Camera3d`; nothing added to the wasm build's features. The old text readout stays as a debug
toggle (H; in the browser the page's `#hud`), and as the `HUD` log lines.

**What the original draws** (`../lab/hud/README.md`: oracle RAM dumps of 1 human + 1 bot, display lists walked with
TMEM simulated, the HUD code `func_150918EC`, `func_15093B58`, `func_150417AC`, `func_1517FB9C`):
- **Its frame is 292 × 216, not 320 × 240** (V: `G_SETCIMG` width 292, scissor (2, 0)-(290, 216); C: its VI mode shows it
  at (16, 12) of a 320 × 240 TV picture). `hud.json` keeps the original's 292 × 216 coordinates (`tv` gives the offset);
  the code places every part off its viewport's left, right, centre and top (C), so the client scales the frame by the
  window's height (its width when narrower) and keeps each part's distance from its anchor's edge or centre: on a wide
  window the health stays at the left edge and the radar at the right one, as on the original's split screens.
- No ammo count, no drawn/away state, no countdown, no messages (V: every weapon, a kill, a death, a pickup), so the new
  HUD has none either; the text readout keeps them.
- Elements, positions and rules: the table in `../lab/hud/README.md` (V unless noted there; the tie rule, the dots'
  turn, the iris's centre and 0x16's R view were read on 2026-10-13: "HUD and audio leftovers").

| Step | What |
|---|---|
| Pack (`conker-extract`, `conker_assets::hud`) | `hud/`: 12 textures from the texture blob by id (atlases 0xD16/0xD17 CI4, radar 0xCF3 IA8, grenade 0xD13, reticle 0x1013, scope 0xFEE + dot 0xFF2, iris 0x7B0, rank suffixes 0x1081/0xD21/0xEA2/0x11B7), 95 font glyphs decoded from the font's run-length records (ROM 0x40F10, the game's font table), `hud.json` (frame, anchors, rectangles, the picture per state, colours, rules: health rows, weapon icons by id, grenade step, text layout, radar scale and range, reticle weapons, the scope's zoom bar, the iris's radius per frame). `pack.json` gains `hud: true`; format still 1 (a pack without `hud/` fails with `hud/hud.json: … export a new one`). The glyphs and suffixes are white with the shape in the alpha (their colour is the text's); the rest are the game's texels. `conker_camera::zoom_limit` (the scope's range, for the zoom bar) is the camera's own match, moved to a function |
| `../assets` | A fresh export (temporary folder, the same `--background`) equalled the pack but for `hud/`, `pack.json` and `README.md`; those three were copied, nothing else touched: the pack equals a fresh export file for file |
| Client | `hud::Shown` from what the text HUD read (`Combat`, `MatchView`) plus the others' `Player.pos`/`Combat` (radar, place), the local `Player.aiming`, the follow camera's scope and drawn view (radar's turn), the clock of the local animation (a grenade in the hand); `hud::sprites` (pure) → `place` → quads, re-spawned each frame, materials cached per (texture, texels, colour). Sharp texels for the atlases, grenade, glyphs and suffixes; smoothed for the radar, reticle, scope and iris (the original scales those). `NoAutomaticBatching` on the quads: batched, WebGL2 drew them all with the batch's first material (V: glyphs showed chocolate). Browser: `start()` fetches `hud/hud.json` with the data files; the PNGs load through Bevy |

| Check (2026-10-11) | Result |
|---|---|
| Layout against the original (`hud::tests::layout_matches_the_originals_draws`, the pack's `hud.json` at the frame's size) | Equal to the dumps' rectangles: health rows for 0-6, the 9 weapon icons (rect and texels), 3 grenades at 275/264/253, the score `0`/`1` at 135/136, the rank `(1st)` at 149/152/157/169 and `(2nd)` at 148/151/157/169, the clock `9:48` at 135/142/145/152 (y 198, 200) and its shadow, the radar disc, 3 blips from the trace's positions and camera to the quarter pixel (258, 42.25; 259.75, 37.5; 260.5, 37.25), the reticle (134, 96, 24, 24), the scope ring's quarters, the iris (radius 120 at 54 frames). Differences: the zoom bar's bottom 139 vs 138.75 (the original's quarter pixels); the iris's top quarter starts above the frame (−29) where the original clips it at 0 (the same pixels); the dots' turn (the original's 0-3.9°, here 0) |
| Textures and font (`../lab/hud/tools/verify_pack.py`) | The 10 dumped textures equal the pack's PNGs within 1 (RGBA16 expansion); 95/95 glyphs and metrics equal the game's decoded glyphs in RAM |
| Extractor tests + conker-assets (ROM) | 102 pass (2 new: the glyph decoder, `pack_hud_reads_back`) |
| target-net: conker-client, conker-server | 37 (2 new: the layout above, the anchors), 12 |
| conker-sim `--release --test oracle` (ROM, `--include-ignored`) | 12/12; camera4_w23 0.0866, facing misses 224; zone gate 161,317 / 0 and 80,625 / 6: unchanged. conker-camera 30 + 7 |
| wasm32: conker-client (clippy), conker-assets, conker-extract, conker-sim, conker-rules, conker-bots, conker-camera | clean |
| clippy (changed crates, both targets, wasm32); `cargo fmt --all --check` | 0 warnings; clean |
| `smoke.sh wt ws ws` (port 6970) | 16/16, prediction max error 0 (684/684/268 ticks), 0 rollbacks (run twice by mistake: the same both times) |
| Native screenshots (`../lab/hud/shots.sh`: e_w09 replayed, two offscreen clients, 1280 × 720) | `../lab/screens/hud/`, below |
| Chrome 153 (`web/play.html`, ports 6950 game, 6960 http; page 1 WebTransport, page 2 WebSocket) | every state, a wide and a narrow window, below; console: no errors or warnings |
| wasm (`web/build.sh web`, wasm-opt; `web/dist` rebuilt) | 21,948,071 → **22,073,687 B** (+125,616; rustc output 30,090,587 → 30,235,242); gzip -9 6,928,500 → 6,972,538 (+44,038); brotli -q 11 4,375,125 → 4,405,221 (+30,096); `client.js` unchanged (131,914 B) |

Screenshots (`../lab/screens/hud/`): native, `scope-p2.png` full health unarmed (the radar's blip: the other player),
`hurt-p2.png` the victim hit three times (3 pieces) holding the twin guns, `aim-p2.png` 3 grenades while one is in the
hand, `aim-p1.png` the reticle (R with the twin guns), `scope-p1.png` the sniper rifle's scope with its zoom bar,
`kill-p1.png` `1 (1st)` after the kill, `kill-p2.png` the victim's iris 35 frames before the respawn (its place `(2nd)`
showing through), `hurt-p1.png` the shooter at read 760: full health, the twin guns' icon (put away). Chrome (1280 × 720 unless named): `chrome-p2-unarmed.png`,
`chrome-p2-grenades.png`, `chrome-p1-aim.png` (1718 × 912), `chrome-p2-hurt.png`, `chrome-p1-kill.png`,
`chrome-p2-dead.png` (the iris, after a grenade cooked in the hand), `chrome-p2-scope.png`, `chrome-p2-wide.png`
(1800 × 560: health at the left edge, weapon and radar at the right one), `chrome-p2-narrow.png` (560 × 878: scaled by
width, the clock at the bottom).

Gaps:
- The rank on a tie (corrected 2026-10-13): the rule is read (`func_151E565C`, C) and `places` follows it: kills −
  deaths, then fewer shots; equal score and shots share the place. The 1st or 2nd at 0-0 was the shots tie-break.
- (2026-10-13: the `#net` line and the sound hint moved; see "HUD and audio leftovers".)
- (Corrected 2026-10-13, "HUD and audio leftovers".) The iris is keyed on the frames left before the respawn, as the
  original (its respawn is the death's start + 88 for every death); its centre follows the dying player's projected
  head (a blast victim's own place). 0x16's R view tints the framebuffer; the dots turn with the zoom.
- (Fixed 2026-10-13.) In a narrow window the page's `#net` line covered part of the clock, and before the first key
  press the page's sound hint covered the weapon icon.
- The white flat polygons near the camera in some screenshots are the level's own (V: a free-camera view from above
  shows white triangles in the level; the HUD's quads are on their own layer and camera).
- The bundle is the final code but two doc-comment edits (`Shown`, the crate doc). `kill-*`/`aim-*` and the Chrome
  shots are on the final code; `hurt-*`/`scope-*` predate `NoAutomaticBatching` (render-only: native drew right before
  it, which is why only the browser showed the bug).
- Deploying: the new client refuses a pack without `hud/hud.json` (natively and in the browser), so the site's pack
  must be re-synced (`deploy/pack.sh`: it copies the whole folder, `hud.json` gets its `.br`/`.gz`) before this
  `web/dist` goes live, or every page stops at `hud/hud.json: HTTP 404`.
- The rank suffixes `rd` and `th` (0xEA2, 0x11B7) are C (the text sprites' records at 0x80091730/40): no two-player
  frame shows them. Native screenshots are 1280 × 720 only (offscreen size fixed); wide and narrow windows are the
  Chrome shots and `hud::tests::anchors_keep_their_edges_distance`.
- The landing (`web/app`) does not mention the HUD: untouched, bun not run.
- `../RUST_REMAKE_PLAN.md` and `../ROADMAP.md` were not updated.

## Audio (2026-10-11)
**The match has the original's sound, natively and in the browser: footsteps by floor, both sides' voices and clip
sounds, the weapons (loops and tails, shots, cases, hits, blasts, the grenade's hiss, landings and fuse), pickups,
the map's two bubbling spots and the background noise.**

| Step | What |
|---|---|
| 1, extraction (`../lab/audio/README.md`) | The pack's `audio/`: 183 sounds (`sounds.json`, 184 WAVs: `common/`, `characters/117`, `characters/128`, `maps/2d/` with `background.wav`, a recording of the game's own output); `events.json`: the background, 2 emitters, the clip events of 117 (72) and 128 (70), footsteps per surface kind, 58 cues. Extractor tests 100 pass; the oracle rebuilt with `CONKER_AUDIO_DUMP`, its pinned traces unchanged |
| 2, playback (`crates/conker-client/src/audio.rs`) | below |

How it plays:
- **Events, read from the replicated state** (the server sends none; nothing in conker-sim/conker-rules/the protocol
  changed): each player's clock passing a clip event's time (the local player's predicted clock, the others' from
  `Combat::anim`; `held.json`'s rule); a cue's condition starting and ending (the fire clips, the chainsaw in hand,
  burning); the fire clip starting (a shot, its case later); the health dropping (a hit: its weapon is not sent, so
  it is read from the victim's first reaction, e.g. 0x11B melee 0x21, 0xF3 a grenade's or, with a rocket just gone, a
  rocket's, and the nearest armed player for a bullet's stagger); `MatchView` (a pickup taken, by the nearest player;
  a rocket gone; weapon 0x14's shots); the local fighter's grenades (every grenade in flight); the background from the
  first tick of play to the match's end; the emitters' bursts (0.5 % a tick).
- **Rules (`events.json`):** linear `near`/`far` from the local player, silent below 401, `own` centred, channels a/b
  per owner, `chance`, `if_idle`, `pitch` as the playback rate, `ticks`; equal-power pan along the follow camera's
  right; effects × 0.6 against the background recording at 1.0.
- **Playback:** rodio 0.22's mixer directly, not bevy_audio: its sinks repeat whole files (426 and 719 loop after an
  intro), its spatial pan keeps half the volume in the far ear (the measured mix needs equal power), and it opens the
  device at startup, before a page may. Each sound is a source with its loop points, linear ADSR (only 13 and 176
  decay) and two ear volumes set every tick through atomics (smoothed over ~20 ms). WAVs load through a Bevy asset
  loader once the device is open. Browser: the AudioContext is created on the first key press or click on the game
  (the page shows "Press a key or click for sound" until then); M mutes.
- **Two timings from the oracle where `events.json` says otherwise (V):** a held cue ends a tick after its condition
  (the twin guns' tail 727 + case: 7/7 a read after the fire clip in e_w09, all 7 now on the read); the grenade's hiss
  1299 runs from 4 ticks into the throw until the blast (e_gren, sq_e_gren, ctlw_k*; `events.json`: while in the
  hand), and its landing sounds play on its first three landings, the Flight cue's then the Bounce cue's (27, 42 and
  50 ticks after the launch, all captures; `events.json`: clip ticks 4, 15, 23 of the grenade object).

| Check (2026-10-11) | Result |
|---|---|
| `combat.sh` e_w09 (port 6814; `log` SOUND lines vs step 1's captures, `../lab/audio/tools/remake_compare.py`, ids or the same candidate list, ±6 reads) | shooter's client: 24/26 oracle calls matched, its own sounds on the read (719 ×7, 727 ×7, cases ×7, 1553), the victim's 4 reads late (1254, 190); victim's client: 24/26, its own on the read, the shooter's 4-5 late. Missed: 802 (bullet fly-by, step 1's open item), 1278 (below). Bubbles 18/27 and 29/28 (random) |
| `combat.sh` e_gren (port 6815) | 9/9 on both clients: 1237, 1447/1448/1449, 929, 1549 on the read; 1299 2 reads early (own); 1243, 190 on the read for their owner, 4-5 late for the other. Combat events unchanged (39/43, 9/11, clocks 742/742, 452/452) |
| `smoke.sh wt ws ws` (port 6816) | 16/16, prediction max error 0; headless clients log 176/181/91 SOUND lines |
| Native windowed, release, 2 bots (`--script`, 45 s) | output open 44.1 kHz stereo, `sound: output running`, 360 sounds, no stream error; the audio thread (CoreAudio IO) 96 of 4,947 samples (1.9 % of a core, `sample` 10 s), the sound logic 4; the whole client 336-363 % CPU (continuous rendering), against 346-361 % (171-217 % in its last 8 s) with a pack without sound |
| Chrome 153, `web/play.html` (ports 6830 game, 6840 http, `?transport=ws`) | no AudioContext before the gesture, the hint shown; key press at 17,071 ms → output open 17,075 → `running` 17,195, context `running` at 44.1 kHz; M: "Sound muted (M)", destination RMS 0.05-0.12 → 1.9e-30 → back; console: no error or warning messages |
| conker-sim oracle (`--release`, ROM), conker-rules release | 12/12, camera4_w23 0.0866, facing misses 224, zone gate 161,317 / 0 and 80,625 / 6; 53 + 3 + 4 + 2: unchanged |
| target-net: conker-client, conker-server; clippy (native, wasm32); `cargo fmt --all --check` | 35 (5 new: clip windows, WAV, loop intro and release, hit weapons, gone projectiles), 12; 0 warnings; clean |
| Landing (`web/app`): `bun run check`, `bun run build` | clean ("Mute: M" in the controls) |
| wasm (`web/build.sh web`, wasm-opt; `web/dist` rebuilt) | 21,727,816 → 21,948,071 B (+220,255; rustc output 30,090,587); gzip -9 6,839,155 → 6,928,500 (+89,345); brotli -q 11 4,317,653 → 4,375,125 (+57,472); `client.js` 123,829 → 131,914 B. The base: the previous `web/dist` (its sources differed only by this step) |
| Sound files | 184 WAVs, 13,315,904 B, brotli 8,896,890; `background.wav` 7,512,396 B, brotli 4,243,369 (gzip 5,391,137): shipped as is (`deploy/pack.sh` now precompresses WAVs too), no OGG: no decoder in the wasm and no encoder at deploy for 3 MB once (ETag-cached) |

Gaps:
- (2026-10-13, "HUD and audio leftovers": 1278 is a bullet's head pop; 802 and the in-hand 929 play.) **1278 (the
  killing bullet):** played at the lethal hit (e_w09 read 948); the original plays it at 1004, from a bullet
  routine (`func_151D4DAC`), 970 in e_w39 and never in sq_e_w09: its trigger is I.
- 802 (a bullet passing by) is not played; the in-hand fuse (929 while cooking past 60 ticks left) is not either.
- Remote players' sounds come 4-5 reads after the original's (their state's trip and the interpolation delay); the
  local player's own are on the read.
- (Fixed 2026-10-13: the server sends it, `Combat::hit`.) A hit's weapon is inferred (wrong when another player
  holding a matching weapon is nearer than the shooter).
- (Fixed 2026-10-13: a shuffle bag at each bounce.) The original picks the grenade's three landing sounds in a random
  order; here 1447, 1448, 1449 in turn.
- Headless clients have no samples: a sound that does not loop ends at once there, so `if_idle` always passes.
- Bevy's free debug camera also takes M (cursor grab) while it is on.
- In the browser the WAVs (8.9 MB brotli) load after the first gesture; a sound plays once its file is in (the
  background about 0.1 s later on localhost). Loop `count` is ignored (no sound of the pack has one).
- The native and Chrome runs used the build before two lint-only edits (`as_chunks` in the WAV reader, a distance
  helper); the tests, the replays, `smoke.sh` and the shipped bundle are on the final code.
- `../RUST_REMAKE_PLAN.md` and `../ROADMAP.md` were not updated.

## Sides: squirrels and Tediz (2026-10-11, after the asset pack)
**A player picks a side, `side=squirrels|tediz` (URL query, or `--side` natively); without it the server puts the
player on the side with fewer players (the squirrels on a tie), and bots alternate (even slots squirrels).** The side
picks the model and its body; the match stays a Deathmatch (every player its own team, scoring per player). **Each side
starts and respawns in its own base, as War: Total War does (after the bases fix below): the squirrels in the south, green
base (records 0, 6, 7, ...), the Tediz in the north, red one (1, 13, 14, ...).**

What the original does (oracle runs in `../lab/oracle`: `sides_s9` = War: Total War, scenario 9, 2 humans + 2 bots,
both humans on `jump.txt`, raw trace and RAM dumps; `sides_dm_custom` = Deathmatch with the menu's characters poked
by lldb through `tools/combat_lldb.sh`, which re-signs a temporary copy, the oracle binary untouched; the ROM probe
`../lab/tools/sides_probe`, modes `models`, `routes`, `scripts`). V oracle, C code/ROM, I inferred:
- **Model:** the squirrel soldier (SHC) is bank-01 **model 128** (V: obj+4 = 128 for team 0, 117 for team 1). The
  class decides it: War modes give team 0 class 4 and team 1 class 3 (C, decomp `func_151E2834`); class 3 is the Tedi.
- **Deathmatch** with the table's classes (`0x800E0B8C` = 0, the oracle's setup) is all Tediz (V). With the menu's
  characters, menu entry 0 is model 128 / class 4 (C, table `0x800AB57C`): a human who picks it plays a squirrel in
  Deathmatch (V, `sides_dm_custom`: obj+4 = 128, teams stay unique); bots keep class 3 (V).
- **Animations:** obj+6 = 0 for both (V): the squirrel plays Conker's clips (bank-02 entry 0), the same 764 as the
  Tedi. Its class data `[0x0F, 128]` routes all 857 logical ids to the same pairs (C); its events differ on 24 ids:
  op 12's argument (0x01… instead of 0x1E…) and three op-82 events (C; I: voice and sound effects), while the held
  models' create/remove events are equal (C). What sets obj+6 for 128, which is in no route group, is unread (I).
- **Skeleton and hands:** the same 28-joint tree and channel slots, other proportions (C). The held models hang on
  matrix slots 9 and 4 for every character (the action records are global, C; seen right in the screenshots).
- **Model lift:** bank-0x11 record +6 = 42 (C) = obj+0xC8 42.0 (V); the Tedi's 43.
- **Camera:** the look-at is 67.5 above both (V, every row of `sides_s9`).
- **Bodies (differ):** area tests obj+0xD2/D4/D6 radius 36, half height 64, centre 44 (Tedi 26, 43, 35); push body
  obj+0xE4/E8/EC radius 30, centre 33, y scale 30/66 (Tedi 23, 33, 23/45) (V, RAM; they are half the bank-0x11
  record's values, C).
- **The same:** movement (V: 277 reads of run, two running jumps, skid: xz speed, target speed and animation rate
  identical every read; the read-24 difference is slot 0's first frame, as in Deathmatch), health 6 (V), the class
  data block and attack tables (V, RAM), the bots' 44 behaviour scripts (C).

Changes:
- **Rules:** conker-sim `movement::Side`; `Other::player(side, pos)`; `push_apart` and `step_among` take the mover's
  side. conker-rules `Fighter.side` (kept through respawns), `damage::Hitbox::of(side)`; `in_reach`,
  `hits_ellipsoid` and `hits_body(.., hitbox, crouched)` take the victim's (`hits_cylinder`, `BODY_TOP`, `CROUCH_TOP`
  are gone). The bullet cylinder's ends (centre ± half, −20 to 108) and the crouch top (−26) are I for the squirrel; the
  sniper's head and torso spheres keep the Tedi's fit for both (I, open).
- **Protocol 9:** `Combat.side`; `Join { side: Option<Side> }`, a client's first message (reliable `JoinChannel`).
  The server puts a player in the match when its `Join` arrives (`join`), not at the netcode connect.
- **Pack:** `characters/128/` (glTF, 10 PNGs, `character.json`: lift 42, the Tedi's clips); `PACK_CHARACTERS =
  ["128", "117"]` (`Side` order); `conker_assets::Character`, `Slice.characters`; `Slice::load/files/write` take the
  character list; `conker_extract::squirrel`; `extract_character::SQUIRREL_SOLDIER`. The world's root motion is the
  Tedi's (the same clips). Format still 1: an old pack fails with `pack.json: characters: no "128"`.
  `../assets`: a fresh export differed only by `characters/128/`, `pack.json` (the list) and `README.md` (template):
  those were copied, nothing else touched; the pack now equals a fresh export file for file.
- **Client:** `Config.side`; sends `Join` on connect; `SideClips`; render loads both characters one after the other,
  draws each player with its side's model, lift and clips (`Characters`, `Rig.side`); held weapons hang on each
  model's own `joint_09`/`joint_04`. `combat.sh` runs both clients with `--side tediz` (the oracle's encounter).
- **Bases fix (2026-10-11; the user reported the two sides' bases swapped):** the server started players by slot on
  `DEATHMATCH_START` (1, 0, 13, 6, ...) with even slots squirrels, so the first squirrel stood on record 1, in the
  Tediz' base, and the first Tedi on record 0, in the squirrels'. Which base is which (C, the level in the pack:
  `../lab/screens/sides/textures_by_base.py`, `base-*.png`): the squirrels' "S" banner (texture `0172`) hangs only on the south wall (z −8116,
  behind records 0, 6, 7) and the Tediz' lightning badge (`0605`) only in the north (z 7373-7655 and the west wall at
  x −4300, behind 1, 13, 14); the textures only the south uses average green or olive (`0729` 41,66,16; `2728`
  32,60,18; `0730`/`0731` 66,83,36; `0289`; `0189`), the north's only ones red (`3755` 80,22,21 on 42 triangles;
  `1858` 107,37,36; `0606`/`0607` ~90,20,18; `0831` 61,1,0; `1582`). So south = green = team 0 = model 128, matching
  the oracle's War: Total War (`sides_s9`: team 0 starts on `TOTAL_WAR_START[0]`). Now:
  - **Server:** `Game::placement(slot, side)`: a side's k-th player (k = its players now, `Game::members`) starts on
    `TOTAL_WAR_START[side][k % 8]` (jittered) and respawns in its half, `TOTAL_WAR_MASKS[side]` (V for k = 0, 1 in the
    original; Total War indexes by team, here by side, since the remake's Deathmatch has no teams), for bots and humans,
    at the start and mid-match. The respawn's scoring is unchanged: every other player is an enemy. A `--spawn` slot
    (oracle replays) keeps the original's Deathmatch: the slot's `DEATHMATCH_START` record (facing, camera) and every
    record for respawns.
  - **Rules:** `Fighter.spawn_mask` (default all records, carried through respawns) is the respawn's `eligible`;
    `Arena.masks` is gone (`Match::deathmatch` and the oracle paths are untouched).
  - **Protocol 10:** the mask rides in the fighter (`Fighting`), so the owner predicts the server's pick (a Tedi
    replayed from e_w09 still respawns on record 6, in the south, with the client predicting the same).
  - **Landing (`web/app`):** the Squirrels are green (`--color-squirrels: #8cc63f`, 9.2:1 on `bunker`, 8.4:1 on
    `trench`; was blue `#5ca4ff`), the Tediz stay red (`#ff5b4f`, 6.2:1 and 5.6:1): titles, team columns, side
    picker, chat names and the emblems' icons follow the theme colour. The pad's X key keeps the Xbox blue as a
    literal.

| Check (2026-10-11) | Result |
|---|---|
| conker-sim `--release --test oracle -- --include-ignored` | 12/12; camera4_w23 0.0866, facing misses 224; zone gate humans 161,317 rows, 0 differ; bots 80,625, 6 differ: unchanged |
| conker-rules release | 53 unit + 3 combat + 4 oracle + 2 ROM; combat replay 506/508, combat4 172/172: unchanged |
| Extractor tests + conker-assets (ROM) | 94 pass; the pack test checks both characters (lifts 42/43, the same clips and animation names, 128's glTF against the in-memory one) |
| target-net: conker-client, conker-server, conker-sim `net`, conker-rules `net` | 30, 12, 45 + 11 (1 ignored), 54 + 3 + 4 + 2: all pass |
| wasm32: conker-client (target-net), and 6 library crates | clean |
| `cargo fmt --all --check`; clippy (changed crates, both target dirs, conker-client for wasm32) | clean; 0 warnings |
| `smoke.sh wt ws ws`, twice | 16/16 both; sides squirrels, Tediz, squirrels (the server's pick); prediction max error 0 (682/683/270 and 684/683/269 ticks), 0 state misses |
| `combat.sh` e_w09, three times | 39/43 events, clock 742/742, lag compensation max 0; victim 24, 20, 17 ticks over 0.001 (wave 6: 15-20) |
| conker-bots release (ROM) | 13 unit + 3 oracle + 3 ROM: unchanged |
| Native screenshots (`../lab/screens/sides/shots.sh`, offscreen) | `idle.png`, `idle-low.png`, `run.png`, below |
| Chrome (`web/play.html`, bundle rebuilt: 21,727,542 B) | two pages, `?side=squirrels` (WebTransport) and `?side=tediz` (WebSocket), 2 bots: each page shows the other's model and both bots' (`browser-*-page-bots.png`); console: no error or warn messages; `?side=bears`: "Could not start: bad value for side" |

| Bases fix (2026-10-11) | Result |
|---|---|
| conker-rules release (ROM) | 53 unit + 3 combat + 4 oracle + 2 ROM; combat replay 506/508, combat4 172/172; spawn oracle 99 placements + 32 respawns, 0 mismatches: unchanged |
| conker-sim `--release --test oracle -- --include-ignored` (ROM) | 12/12; camera4_w23 0.0866, facing misses 224; zone gate 161,317 / 0 and 80,625 / 6: unchanged |
| target-net: conker-client, conker-server, conker-rules `net`, conker-sim `net` | 30, 12, 54 + 3 + 4 + 2, 45 + 11 (1 ignored): all pass |
| conker-bots release (ROM) | 13 unit + 3 oracle + 3 ROM: unchanged |
| clippy (conker-rules, conker-bots; conker-server, conker-client; conker-client wasm32); `cargo fmt --all --check` | 0 warnings; clean |
| `combat.sh` e_w09 (port 6750) | 39/43 events, clock 742/742, respawn record 6 as the oracle's, lag compensation max 0; victim 23 ticks over 0.001: unchanged |
| `replay.sh` walk (port 6760) | 247 rows, max error 0.0000, 1 state miss at read 24: unchanged |
| `smoke.sh wt ws ws` (port 6720) | 16/16; squirrels record 0, Tedi record 1, the third (squirrels, mid-match) record 6; prediction 0 for client 1, max 0.185 (client 2) and 0.136 (client 3) at ticks 641-645, when the two squirrels touched (59.8 apart; contact under 60): the known push misprediction, now met because teammates start 684 apart |
| Native, `../lab/screens/sides/bases.sh` (port 6730, 2 bots + a squirrel and a Tedi human) | bot 0 squirrel record 0, bot 1 Tedi record 1, the squirrel human record 6, the Tedi human record 13; `base-squirrels.png`, `base-tediz.png` |
| Landing: `bun run check`, `bun run build`; Chrome on `http.server` (port 6740) | clean; `../lab/screens/landing/home-green.png`, `lobby-green.png`; no console errors or warnings |

Screenshots (`../lab/screens/sides/`): `idle.png` the squirrel (green helmet, ammo belt, tail) and the Tedi facing the
camera, twin guns drawn in both hands, feet on the floor; `idle-low.png` the same from low and to the side (boots and
feet on the floor); `run.png` both running at the camera with the guns down; `browser-{squirrels,tediz}-page*.png`.
`base-squirrels.png`: the south base from above at read 30, olive-green walls stencilled "SF", the green winged-"S"
banner, two squirrels (bot 0 and the human) on records 0 and 6; `base-tediz.png`: the north base, grey marble with red
trim, red door frames and a red banner, two Tediz on records 1 and 13.

Gaps:
- Sounds: done since (Audio, above).
- The squirrel's bullet cylinder ends, crouch top and sniper parts are not measured on the oracle (I).
- A side is fixed for a player's life; there is no way to switch without reconnecting.
- Bases: k counts a side's current players, so after a leave two players of a side can share a start record
  (jittered, pushed apart). Records past k = 1 are I (Deathmatch's lists by analogy). Teammates now start near each
  other, so `smoke.sh`'s scripted circles touch and show the push misprediction (above).
- Protocol 10: `web/dist` (and the site) predate it and can't join a new server until `web/build.sh` is re-run (not
  run here; the browser check of the bases was native instead).
- Scripts without `side` get the server's pick: `replay.sh`'s lone player is now a squirrel (harmless: the movement is
  the same, V, and no other body is near). The page's `start()` fetches 13 data files now (the characters last).
- The server logs `passed the handshake` at connect and `connected via T as SIDE …; slot N on record …` at the join;
  the lab scripts that grep `connected via` or `slot N on` still match.
- `../RUST_REMAKE_PLAN.md` and `../ROADMAP.md` were not updated.

## Asset pack (2026-10-10, after wave 6)
**Nobody picks or uploads a ROM any more.** The server, the native client and the page read the asset pack
(`../assets`, exported once with `cargo run --release -p conker-extract -- pack`; ROM-derived, outside this tree).
- **Server:** `--assets DIR` (else `CONKER_ASSETS`, else `../assets`) replaces `--rom`. The world, spawns, pickups
  (what each gives) and the bots' data come from the pack. extract-core, extract-scene, extract-character and anyhow
  are gone from it. The world is built by one function, `conker_sim::net::scene_world(&pack)`, on both sides; it also
  takes the pack's sink table (new `CollisionWorld::sink`; None = the old `SURFACES`, so the oracle tests are untouched).
- **Native client:** `assets=DIR` (same fallbacks) replaces `rom=`. Bevy's asset folder is the pack
  (`AssetMetaCheck::Never`), the `rom://` memory source is gone, `model_lift` comes from `character.json`, and the
  character's clips are found by animation name (`pair_NNNN_anim_IIII`), not index. conker-extract and extract-core
  are gone from the client.
- **Page:** no file picker; it starts by itself with a status line. `start()` (async) fetches the 12 data files from
  `assets/` at once; Bevy loads the glTF files from there (the default wasm reader). The connect token is fetched by the
  client (Rust, `web::fetch`), once the level and the character have loaded, just before it connects (a token lasts
  60 s); the page's token code is gone. Choice: Rust fetches the data files, because the same helper had to fetch the
  token anyway, so `index.js` needs no file list.
- **Serving:** dev, a gitignored `web/assets` symlink to `../../assets`. Production: `deploy/pack.sh` (new) copies the
  pack to a temporary folder outside this tree, adds `.br`/`.gz` copies and rsyncs it (`--delete`); the Caddyfile
  serves it at `/assets/` (`handle_path`, `no-cache` + ETag), with an optional `basic_auth` block for the whole site
  (commented out). The systemd unit passes `--assets /srv/conker/assets`.
- **Scripts:** smoke, combat, bots and replay (which also takes `PORT` now) and the lab's deploy-test and screens
  scripts no longer pass `--rom`; they check for `../assets/pack.json`.
- **Pack README template:** pickups are found by the node name's `@N`, clips by name, and edits take effect when the
  server or the page starts.

| Check (2026-10-10) | Result |
|---|---|
| `replay.sh` walk, run | 247 rows each, max error 0.0000, 1 state miss at read 24 (the known one): as before |
| `combat.sh` e_w09, e_gren | 39/43 events, clock 742/742; 9/11, clock 452/452: unchanged |
| `smoke.sh wt ws ws`, twice | 16/16 both, prediction max error 0 (684/684/270 and 683/684/269 ticks), 0 state misses |
| `bots.sh` (150 s, 300 s, and 150 s on prebuilt binaries) | 9/10 each: "bot 1 fights, dies" fails (wave 4's flake). In all three runs both bots camped with the sniper rifle and bot 0 only killed itself (wave 6's runs: 7-13 hits). A temporary A/B test built the server's old ROM world and compared it with the pack's: equal (debug dumps of 691,840 characters each), and so were the bots' four assets and the spawn records (`../lab/screens/pack/ab_world.md`) |
| Extractor tests + conker-assets (ROM, no DOS variables) | 94 pass, the pack test included (it now also checks the 764 animation names) |
| conker-sim `--release --test oracle -- --include-ignored` | 12/12; camera4_w23 0.0866, facing misses 224; zone gate: humans 161,317 rows, 0 differ; bots 80,625, 6 differ: unchanged |
| conker-rules release | 53 unit + 3 combat + 4 oracle + 2 ROM; combat replay 506/508, combat4 172/172: unchanged |
| target-net: conker-client, conker-server, conker-sim `net` | 30, 12, 45 + 11 (1 ignored): all pass |
| `cargo check --target wasm32-unknown-unknown`: conker-client (target-net); conker-assets, conker-extract, conker-sim, conker-rules, conker-bots | clean |
| `cargo fmt --all --check`; clippy `--all-targets` (changed crates, both target dirs; conker-client for wasm32) | clean; 0 warnings |
| Editing a copy of the pack (`../lab/screens/pack/edit.sh`, native, offscreen) | the floor PNG tinted magenta shows (`native-texture.png`); spawn record 1 moved to record 13's place puts slot 0 there (`native-spawn.png`); `model_lift` 143 floats the model 50 units (`native-lift.png`); a renamed field is refused by both: `` …/maps/2d/spawns.json: missing field `yaw` at line 9 column 3 `` |
| Chrome 153 (`web/README.md`, "Asset pack tested in Chrome") | dev and production-like, two pages each (WebTransport, WebSocket): 204 pack files, 0 `.meta` requests, page load to playing 1.8-2.2 s; brotli pack 3,359,854 B of 14,148,973; reload: 204 × 304; password: 401 without, plays with |
| `../lab/deploy-test/test-caddy.sh` (https) | 14/14: wave 6's 10, and 3 new for `/assets/` (br, no-cache, 304; a PNG unencoded) and 1 for the password (401 / 200 on `/`, `/assets/`, `/token`; the pack's 205 files in 1.62 s with it) |
| `../lab/deploy-test/test-native.sh` | 15/15 |
| wasm | 21,796,051 → **21,702,709 B**; gzip -9 6,879,989 → 6,837,597; brotli -q 11 4,344,934 → **4,314,879** |

Gaps:
- The shipped bundle (`web/dist`, the site's `v/33b8955a8554/`) predates one last edit, clippy's borrow fix in
  `render.rs` (`get_handle_untyped(level_scene())`, the same asset path). Rebuild to have identical source.
- One `smoke.sh` run early on was started without `CARGO_TARGET_DIR` and built the Bevy binaries in `target/` (its
  dependencies were already there; nothing deleted). Every number above comes from target-net runs.
- The pack's `README.md` in `../assets` predates the template fix (its data files equal a fresh export); copy the
  new one from a `--force` export to a temporary folder if wanted.
- The server and the pages must read the same pack; nothing checks it (a client whose collision has fewer triangles
  than the server's would index past it). The browser always fetches the site's own pack.
- `pickups.json` accepts any weapon id; an unknown one gives a weapon the rules don't know.
- A `user:password@` address breaks the page under `basic_auth` (Chrome refuses fetches relative to it): use the
  browser's prompt.
- bots.sh: see the table. Not re-run: e_w15, `leave.sh`, the lag runs.
- The scripts' `cargo build` re-links each binary (a new inode), and the next start then took 3-8 s here (macOS
  checking it, I): in the script runs the clients joined at tick ~220-250 instead of ~100. It changes when the bots'
  AI restarts, not the results above; `../lab/screens/wave6/pinned.sh` runs a script on prebuilt binaries.

**Wave 6: deployment is prepared, with a secure connect, and tested on this machine; wave 5's leftovers are done.**
Wave 6 is the second half of option A, "ready to play online", in `../ROADMAP.md`. Nothing is deployed: there is no
host or domain yet, no certificate was requested, and no https page was tested, because no browser here trusts the test
certificate.
- **Secure connect:**
  - The server signs each client's netcode connect token with a private key that never leaves it, and hands the
    token out at `/token`. The page fetches it when the ROM is picked (Asset pack: the client fetches it once the
    level has loaded); a client can no longer make its own.
  - A token lasts 60 s and names the server's public addresses, which the server then checks.
  - WebTransport and `wss://` take a real certificate.
- **Tested locally** (`../lab/deploy-test/`, both scripts re-run here):
  - The server runs in its production shape with a certificate from a test CA. Native clients that check it connect
    over WebTransport and `wss://` (15/15 checks).
  - Refused: another CA, plain `ws://`, and zero-key, foreign-key, tampered, misaddressed and expired tokens.
  - `deploy/Caddyfile` serves the site over https with the test certificate (10/10 checks). The wasm goes out as
    brotli, 4,344,934 B instead of 21,796,051 B (6,879,989 B with gzip), cached for a year in a folder named by its
    content. The page is revalidated on every load, and `/token` is proxied to the server.
- **Deploy kit** (`deploy/`): step-by-step instructions for a Debian or Ubuntu VPS (`deploy/README.md`), plus
  `build.sh`, the `Caddyfile`, a systemd unit and a certbot hook.
- **Other players' grenades are predicted** (protocol 8). e_gren's victim has 0 ticks over 0.001 at 0 and at 150 ms of
  lag (before, on protocol 7: 3 ticks, max 101.5; and 13, max 334.6). In Chrome: 0 at three blasts.
- **Stable slots:** a player who leaves frees its slot and nobody else moves; the next joiner takes the free slot.
  The others take no rollback (wave 5: 3-14 ticks each).
- **Native HUD:** whenever the client is not connected, it shows the connection status and its reason.
- **Dev flow:** unchanged (self-signed certificate and digest, `ws://`, `http://localhost`). One addition: every server
  now also listens on TCP PORT+1 for tokens.

The wave-6 section comes first, then the records of waves 5, 4, 3, 2 and 1. Facts that a later wave changed are updated
in place and marked with that wave ("Wave 6", "Wave 5", "Wave 4", "Wave 3" or "Wave 2").

Paths starting with `../` are outside this tree, in the lab next to it: ROM-derived outputs, the oracle and references.
None of them is part of the publishable source.

## Reproduce
```sh
ROOT=/path/to/checkout   # this tree is $ROOT/remake, the lab is $ROOT/lab
export CONKER_ROM="$ROOT/Conker's Bad Fur Day (USA).z64"
# Extractor crates (ROM tests skip without CONKER_ROM). Optional DOS cross-checks, as ABSOLUTE paths
# (cargo runs each test binary from its crate directory, so ../lab/... breaks them):
#   CONKER_DOS_TEXTURES=$ROOT/lab/reference/dos/build/assets/textures
#   CONKER_DOS_LEVEL=$ROOT/lab/reference/dos-out/scene-2d/level CONKER_DOS_OUT=$ROOT/lab/reference/dos-out
cargo test -p extract-core -p extract-gbi -p extract-scene -p extract-level -p extract-character -p conker-extract
# Simulation, camera, animation, match rules, bots. Their oracle tests read ../lab/oracle and skip without it.
cargo test -p conker-sim -p conker-camera -p conker-anim -p conker-rules -- --nocapture
cargo test --release -p conker-rules -p conker-bots -- --nocapture   # the combat replay's per-trace lines; the bots
cargo test --release -p conker-sim --test oracle -- --nocapture --include-ignored   # every trace + report_all's lines
#   (Wave 4: 12/12 pass.) Debugging: ORACLE_DUMP=name:first:last, ORACLE_RESYNC=1
cargo check --target wasm32-unknown-unknown --lib -p extract-core -p extract-gbi -p extract-scene -p extract-level \
  -p extract-character -p conker-extract -p conker-camera -p conker-anim -p conker-sim -p conker-rules -p conker-bots
cargo run --release -p conker-extract -- pack                  # once: the asset pack in ../assets (see web/README.md)
cargo run --release -p conker-extract -- slice                 # files in ../lab/extract/slice
cargo run --release -p conker-extract --example slice_glb      # the in-memory Slice's two GLBs, ../lab/extract/slice-glb
cargo run --release -p conker-bots --example bot_match -- 9000 3 - 1   # 1 idle human + 3 bots, 5 minutes, seed 1
# Networking crates: keep Bevy/lightyear (14 GB of build output) in their own target dir.
export CARGO_TARGET_DIR=target-net
cargo test -p conker-sim --features net
cargo test -p conker-rules --features net
cargo test -p conker-client -p conker-server
# The scripts below read the pack (CONKER_ASSETS, default ../assets), not the ROM.
zsh crates/conker-server/smoke.sh wt ws ws                     # 3 headless clients, the third joins mid-match ([PORT=5888])
#   Wave 6: every server also opens TCP PORT+1 (connect tokens), so two servers on adjacent ports collide.
zsh crates/conker-server/replay.sh ../lab/oracle/scripts/walk.txt ../lab/oracle/trajectories/walk_a.csv
zsh crates/conker-server/combat.sh combat_e_w09 09             # an oracle encounter through server + 2 clients
zsh crates/conker-server/combat.sh combat_e_w09 09 150 150     #   with 150 ms added both ways on both clients
zsh crates/conker-server/combat.sh combat_e_gren -             #   (weapon '-' = none; also combat_e_w15 15); options:
                                                               #   P1/P2 scripts, SERVER_FLAGS=--no-lagcomp, THIRD=1, PORT
zsh ../lab/screens/wave5/measure.sh combat_e_gren - 150 5931   # combat.sh + the smoothing report (Wave 5)
zsh ../lab/screens/wave5/scripts/kill.sh ws                    # kill a client, time the freed slot (also rejoin.sh)
HUMANS=2 BOTS=2 zsh crates/conker-server/bots.sh 300           # bots + headless clients ([PORT=5888])
zsh ../lab/screens/wave6/leave.sh                              # a leave, then a join: slots, rollbacks ([PORT=5964])
BIN=dir PORT=5961 zsh ../lab/screens/wave6/pinned.sh combat.sh combat_e_gren -   # a script on prebuilt binaries
sh ../lab/deploy-test/make-certs.sh                            # test CA, certificate, token key (lab only, untrusted)
PORT=5975 zsh ../lab/deploy-test/test-native.sh                # the production shape with native clients (about 3 min)
PORT=5985 zsh ../lab/deploy-test/test-caddy.sh                 # deploy/Caddyfile over https (also 8980, 8981, 2999)
cargo run -p conker-server --example mint_token -- KEY_HEX ADDR ID EXPIRE_SECS   # a token for the refusal tests
TAG=scope TRACE=combat_e_w09 FIRST=745 WEAPON=24 S1=scope_p1.txt AT1=840 OFFSCREEN=1 \
  zsh ../lab/screens/wave4/shots.sh                            # screenshots of an encounter (into that folder)
WASM_OPT=/path/to/wasm-opt sh web/build.sh web                 # browser bundle in web/dist (web/README.md)
WASM_OPT=/path/to/wasm-opt sh deploy/build.sh                  # the production site in target-net/site (deploy/)
```
Don't run `cargo test --workspace` in `target/`: it builds Bevy a second time.

## Wave 6 (2026-10-10)
Four agents (workflow `wf_0cb1c326-ba0`). The deploy and leftovers agents built in parallel on one tree. Both edited
`conker-server/src/main.rs` and `conker-client/src/lib.rs`, each only its own functions. Then came a browser test,
then this file.

| Agent | Wrote | Status |
|---|---|---|
| deploy | conker-server's TLS and tokens (new `token.rs`, `tls.rs`, `examples/mint_token.rs`), conker-client's `connection.rs`, `web/index.js`, new `deploy/`, `../lab/deploy-test/` | done |
| leftovers | conker-rules (`game.rs`, `net.rs`), conker-sim `net.rs` (protocol 8), the server's slot handling, the client's `known_match` and `render.rs`'s HUD | done |
| browser | `web/index.html` (the ROM picker centred), `web/README.md`, the `web/dist` bundle and the site | done |
| status | this file, `../ROADMAP.md` | done |

Lab outputs, in `../lab/screens/wave6/`:
- `deploy/`: the deploy agent's test outputs, Chrome screenshots and logs;
- `runs/` and `logs/`: the leftovers agent's before/after runs;
- `browser/`: the browser test's scripts (`run.sh`, `hook.js`, `overticks.py`, `pagecheck.py`) and logs;
- `status/`: this file's runs;
- `leave.sh`, `pinned.sh`, and the screenshots below.

The test CA, the certificates and the token key are in `../lab/deploy-test/certs/`; nothing on this machine trusts
them.

### Verification run (status agent, rustc 1.98.1, all re-run here on 2026-10-10)
| Check | Result |
|---|---|
| Extractor tests, 6 crates, ROM + the three DOS variables (absolute) | 88/88 pass, nothing skipped |
| conker-camera, debug and release | 30 unit + 7 oracle/ROM tests pass |
| conker-anim, debug and release | 9 unit + 1 oracle + 4 ROM tests pass |
| conker-rules, debug and release | **53 unit** (wave 5: 51) + 3 combat + 4 oracle + 2 ROM tests pass. Combat replay unchanged: 506/508 on the 29 traces with events, 172/172 on the 9 combat4 traces |
| conker-bots, release | 13 unit + 3 oracle + 3 ROM tests pass |
| conker-sim, debug | 44 unit + 11 oracle tests pass, `report_all` ignored |
| conker-sim, `--release --test oracle -- --include-ignored` | **12/12 pass**. Numbers unchanged: camera4_w23 0.087; scope facing misses 69 / 894 / 224; zone gate: humans 161,317 rows, 0 differ; bots 80,625 rows, 6 differ |
| target-net: conker-sim `--features net`, conker-rules `--features net`, conker-server, conker-client | 45 + 11 (1 ignored); 54 + 3 + 4 + 2; **12** (wave 5: 8); **30** (29). All pass |
| `cargo check --target wasm32-unknown-unknown --lib` for the 11 library crates, and for conker-client (target-net) | clean |
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets`: the 11 default-target crates; conker-client, conker-server and conker-sim `net` in target-net | 0 warnings |
| `smoke.sh wt ws ws`, twice (`PORT=6030`) | **16/16 ok in both runs**. Prediction vs server: 685 / 685 / 271 and 684 / 684 / 270 ticks, max error 0, 0 state misses. Run 1: 0 rollbacks. Run 2: four 3-tick rollbacks, one after each of the 4 server-decided pickups (client 1 at t=713, 743 and 892, client 3 at t=643), and 0 `CORRECTION` lines. Each run's server logged `token issued` for all 3 clients |
| `../lab/deploy-test/test-native.sh` (`PORT=6050`) | **15/15 ok** in 172 s. The deploy agent's report says 18/18; its own file has these same 15 lines |
| `../lab/deploy-test/test-caddy.sh` (`PORT=6060`, Caddy 2.11.7 on 8980/8981) | **10/10 ok**, on the shipped site `v/178d7d3a45cd/` (the numbers below) |
| Rust source | 33,454 lines in 13 crates, tests included (wave 5: 32,757) |
| Bundle | Not rebuilt here. `web/dist` and the site's `v/178d7d3a45cd/` hold identical bytes, and no client source is newer than them. `client_bg.wasm`: 21,796,051 B; 6,879,989 B gzip -9; **4,344,934 B brotli -q 11** (37% smaller than gzip). `client.js`: 122,808 / 17,249 / 14,287 B |

Not re-run here: the browser test, `combat.sh` (e_gren, e_w09, e_w15), `leave.sh`, `bots.sh` and the traffic runs.
Their numbers below are the agents'. Every process started for these checks was stopped.

### Secure connect (deploy agent)
**Server flags** (conker-server; dev needs none of them):

| Flag | Dev (no flag) | Production (`deploy/conker-server.service`) |
|---|---|---|
| `--token-key FILE`, or `CONKER_TOKEN_KEY` | a random key at each start | 64 hex digits (`openssl rand -hex 32`). An all-zero key is refused |
| `--token-addr ADDR:PORT` | `[::]:PORT+1`. The server exits with a message if the port is taken | `127.0.0.1:5889`: Caddy proxies the page's `/token` to it |
| `--public IP[:PORT]`, repeatable | tokens name 127.0.0.1 and ::1, and the address check is off | the A and AAAA addresses: tokens name them, and the server checks them |
| `--cert PEM --cert-key PEM` | self-signed for 14 days, pinned by its digest; `ws://` | the domain's certificate for WebTransport and `wss://` (PKCS#8, SEC1 and PKCS#1 keys load) |

- **Endpoint:** `GET /token[?id=N]` answers the 2,048-byte token as 4,096 hex digits, with
  `Access-Control-Allow-Origin: *` and `Cache-Control: no-store`. A token lasts 60 s (`TOKEN_SECS`) and carries the
  3 s timeout. Without `id=` the server picks a random id. It logs `token issued to client N`.
- **Native client:** new options `token=HEX`, `token-url=URL` (plain http), and `ca=PATH`. `ca=` is dev/test only:
  `wss://` and WebTransport are checked against that CA, and the client binds dual-stack. Without `token=`, a native
  client fetches its token from `http://HOST:PORT+1/token?id=<id>`. So `smoke.sh`, `combat.sh`, `bots.sh`,
  `replay.sh` and the wave-5 lab scripts run unchanged.
- **Page:** fetches its token when the ROM is picked (Asset pack: the client fetches it, from the same places, once
  the level and the character have loaded):
  - over https, `/token` on its own origin; over http, `http://<server host>:<port+1>/token`; `?token-url=`
    overrides both;
  - it makes a random id when `?id=` is absent, so the server's log matches the page;
  - an https page skips `digest.txt` and uses `wss://`;
  - the token and digest go to `start()` only, never into the address;
  - if the fetch fails, the form names the reason, and the ROM can be picked again without a reload.
- **Locally:** what the two scripts check (re-run here, above):
  - `test-native.sh`:
    - the endpoint answers on loopback and is refused on `::1`;
    - WebTransport and `wss://` connect with the certificate checked against the test CA;
    - refused, as each should be: another CA (`UnknownIssuer`, both transports), plain `ws://`, a zero-key, a
      foreign-key and a tampered token (`Crypto(Failed)`), a token naming 10.9.9.9 (`server address not in connect
      token whitelist`), a token that expired after 1 s and an endpoint token used 113 s after issue
      (`TokenExpired`);
    - a PKCS#8 key works as well as a SEC1 key.
  - `test-caddy.sh`, with `curl --cacert`:
    - the wasm: HTTP/2 200, `application/wasm`, brotli 4,344,934 B or gzip 6,879,989 B as asked, 21,796,051 B without
      encoding, `public, max-age=31536000, immutable`, `Vary: Accept-Encoding`;
    - `/` and `/index.js`: `no-cache`, then 304 on revalidation;
    - `/token` through Caddy: 200, `no-store`, 4,096 hex digits;
    - http answers with a 308 redirect to https;
    - a native client connects over WebTransport with a token fetched through Caddy, certificate checked.
- **Dual-stack on macOS:** during smoke, `netstat` showed `tcp46`/`udp46 *.PORT` and `tcp46 *.PORT+1` (deploy
  agent, `deploy/netstat-smoke.txt`).
- **Regression** (the deploy agent's runs): `combat.sh` e_w09, twice: 39/43 events, clock 742/742, lag compensation
  max 0. The victim had 18 and 17 ticks over 0.001, within the spread of the other agent's runs before wave 6
  (15-18). e_gren: 9/11 events.

### What an operator does (`deploy/README.md`)
1. A Debian 12 or Ubuntu 24.04 VPS. DNS A and AAAA records for the game's name.
2. Packages: Caddy, certbot, ufw, build tools, rustup. Firewall: 22, 80 and 443 (tcp; 443 udp optional), and 5888
   tcp and udp. 5889 (tokens) stays on loopback.
3. A `conker` system user, `/opt/conker` and `/etc/conker`, and the token key (`openssl rand -hex 32`, mode 0640).
4. On the host: build the server (`cargo build --release -p conker-server`; add swap for the link). Put the
   operator's **own** ROM in `/opt/conker`; each player's ROM stays in their browser.
5. On your machine: build the site (`deploy/build.sh`), then `rsync` it to `/srv/conker/site`.
6. Install the systemd unit with your `--public` addresses, and the Caddyfile with `DOMAIN`, `SITE` and `TLS_DIR`.
   Then run `certbot certonly --standalone` with the README's hooks: Caddy stops while certbot answers the
   challenge, and the deploy hook copies the certificate to `/etc/conker/tls` and restarts Caddy and the server.
7. Run the README's step 9 checks: the services; `net.ipv6.bindv6only` 0; `ss` shows `*:5888`; the curl header
   checks; `/token` returning 4,096 bytes; Chrome over WebTransport and `?transport=ws`; an IPv4-only network;
   `certbot renew --dry-run`.

Deploy the server and the page together: the token carries `PROTOCOL_ID`, so a page from another version is refused.

### Wave 5's leftovers (leftovers agent)
- **Other players' grenades** (protocol 8).
  - How: each client's own `Fighting` now carries every grenade in flight as `(birth, Grenade)` (`Fighting::of`
    clones `m.grenades`). No new component.
  - The client flies them in its own tick (`Match::step_player`, `fly_grenades`).
  - Another player's throw causes one rollback that moves nothing; the blast and the knock-back are then predicted.

  | `combat.sh combat_e_gren -`, victim | Before (protocol 7, 1 run) | Wave 6 (2 runs) |
  |---|---|---|
  | 0 ms | 3 ticks over 0.001, max 101.5 | 0 and 0, max 0 |
  | 150 ms each way | 13 ticks, max 334.6 | 0 and 0, max 0; 0 smoothing corrections |

  - Unchanged: the thrower (1 tick, 0.039), events 9/11, clock 452/452.
  - Cost:
    - every client takes one invisible rollback per throw by someone else (13 ticks in Chrome at 150 ms);
    - about 8-10 KB per grenade flight to each client (+0.84 KB/s at 0 ms and +1.03-1.05 KB/s at 150 ms, averaged
      over 10 s);
    - `bots.sh` traffic with 2 humans and 2 bots is unchanged: 8.7-8.9 KB/s per client from the server, before and
      after.
- **Stable slots** (conker-rules):
  - `Match.free`;
  - `remove_player(i)` leaves a stand-in in slot i and marks the slot free; it still drops the leaver's projectiles,
    undelivered pickups and fires;
  - `next_slot()` and `join(f) -> slot`;
  - `step` skips free slots;
  - `Fighter::vacant(tick)`: dead, far away, team `u8::MAX`.

  The server joins on `next_slot()`, so nobody's slot moves on a leave, and `History::remove` is gone. `leave.sh`
  (3 clients, slot 0 leaves, a fourth joins):

  | | Before (1 run) | Wave 6 |
  |---|---|---|
  | slots 1 and 2: rollbacks on the leave, then on the join | 1 (4 ticks) then 1 more; 2 (6 ticks) then 1 more | 0 and 0 on both |
  | the joiner | slot 2 | slot 0, on record 1 |
  | prediction max error, 4 clients | – | 0 |

  A new test shows that a free slot changes no respawn pick or RNG draw. The oracle replays are unchanged (above).
- **Native HUD** (`render.rs`'s `update_hud`): the match numbers only while connected; otherwise the status line's text,
  e.g. `could not connect to 127.0.0.1:5969 over WebSocket: … Is the server running?` or `disconnected from …: the
  server closed the connection. Reload to reconnect`. The browser is unchanged (empty HUD, the page's `#net` line).
- `known_match` takes the grenades. It adds stand-ins up to each grenade's owner and the fire's attacker; without them,
  a lethal blast from an unknown player crashed the client (slot index out of range).

### Browser (browser agent; not re-run here; `web/README.md`, "Wave 6 tested in Chrome 153")
The DevTools-driven browser reports Google Chrome 153; it is Comet 153.0.8010.222 (Chromium), on macOS (M2 Max). Each
run had two pages at `lag=150` (one WebTransport, one WebSocket) and 2 bots.
- **Runs:**
  - dev1: the dev setup;
  - prod1 and prod2: the production shape over plain http. The server had a token key file, `--public` with the
    address check on, and tokens on loopback. Caddy served the site with `deploy/Caddyfile`'s site block, minus
    `tls`.
- **Tokens:**
  - every page fetched one at the ROM pick, and the server logged it for the same id;
  - dev: `http://localhost:5951/token`, cross-origin, 200;
  - production shape: the page's own `/token`, 200 `via: 1.1 Caddy`.
- **Compression and caching:**
  - Chrome sent `Accept-Encoding: gzip, deflate, br, zstd` and got brotli: the wasm 4,344,934 B, immutable;
    `client.js` 14,287 B; `/` 807 B and `index.js` 1,567 B, `no-cache`;
  - on a reload, `/` and `index.js` answered 304, and the wasm and `client.js` came from the cache without a request.
- **Grenades:**
  - 3 blasts on the victim's page, on both transports: no rollback and no correction at the blast, 0 ticks over 0.001
    through the knock-back flight (wave 5: corrected by 334.6 units);
  - the throw costs the victim one 13-tick rollback that moves nothing.
- **Leaving:**
  - 5 closes freed the slot 11-44 ms after the `pagehide` beacon;
  - the other page took no rollback, and none on the 2 rejoins into the freed slot 2.
- **Prediction:** throwers 925, 1,729 and 1,929 ticks, 1 over 0.001 each (the match-start tick). Every other tick
  over 0.001 on the victims (43, 52 and 84) was a bot's server-decided hit, or prod1's match-start reset.
- **Endpoint down:** the form says `could not get a connect token from http://localhost:8955/token: HTTP 502. Is the
  server running?`.
- **Console:** 0 errors or warnings on 8 game pages, apart from that 502; 0 ERROR/WARN in the server logs; no foreign
  input.
- **Fix:** the ROM picker sat at the far left of a wide window (also before wave 6). `#rom` now has
  `justify-items: center`.
- **Re-checked after the fix:** the dev flow as `web/README.md` gives it connected over both transports.
- **Deploy agent's Chrome pass:** dev WebTransport and WebSocket with server-signed tokens; the production shape; the
  hashed bundle from `v/<hash>/`; and with the endpoint down, `Failed to fetch. Is the server running?`, after which
  a second ROM pick connected without a reload.

### Screenshots (`../lab/screens/wave6/`)
| File | Shows |
|---|---|
| `browser-prod-p2-ws-before-blast-{1,2}.png`, `browser-prod-p2-ws-blast-flight.png` | the WebSocket victim before the blast and in the air (`HP 4/6`), predicted |
| `browser-prod2-p2-wt-{before-blast,after-blast,after-blast-2}.png` | the WebTransport victim before and after its knock-back |
| `browser-prod-p1-wt-thrower.png` | the thrower (`grenades 2`); the white quad top left |
| `browser-{dev,prod}-p1-wt-rejoined-slot2.png`, `browser-prod-p2-ws-after-rejoin.png`, `browser-dev-p2-ws-after-blast-and-leave.png` | a rejoin into the freed slot 2; the page that stayed |
| `browser-prod-token-502-server-down.png`, `browser-prod2-p2-wt-server-stopped.png` | the token error in the form; a running page after the server stopped |
| `browser-form-centered-after-fix.png`, `browser-dev-readme-flow-ws.png` | the centred ROM picker; the README's dev flow over WebSocket |
| `deploy/chrome-dev-{wt,ws}-token.png`, `deploy/chrome-prodshape-{wt,ws}-keyfile-public.png`, `deploy/chrome-prod-bundle-hashed-wt.png` | connected with a server-signed token: dev, production shape, the hashed bundle |
| `deploy/chrome-no-token-server-down.png` | the form's message when the token endpoint is down |
| `hud-failed-ws.png`, `hud-lost-ws.png` | the native HUD: a failed connect, a lost connection |

### Per crate (wave 6)
| Crate | Rust lines (wave 5) | Tests (re-run here) |
|---|---|---|
| conker-server | 1,739 (1,370). New: `token.rs` 190, `tls.rs` 31, `examples/mint_token.rs` 34 | 12 unit (8), plus the scripts |
| conker-client | 3,596 (3,364); `connection.rs` 726 (513) | 30 unit (29) |
| conker-rules | 6,414 (6,321) | 53 unit (51), 54 with `net` (52) |
| conker-sim | 5,786 (5,783) | unchanged |
| conker-camera, conker-anim, conker-bots, 6 extractor crates | unchanged | unchanged |

`deploy/` (new, outside the crates): `README.md`, `build.sh`, `Caddyfile`, `conker-server.service`, `renew-hook.sh`.

**API changes in wave 6:**
- **conker-sim `net`:** `PROTOCOL_ID = 8`. `DEV_PRIVATE_KEY` is now unused.
- **conker-rules:**
  - `Match.free`, `Match::next_slot`, `Match::join`;
  - `remove_player` no longer renumbers;
  - `Fighter::vacant`;
  - the `net` feature's `Fighting` carries every grenade in flight.
- **conker-server:**
  - flags `--token-key` (or `CONKER_TOKEN_KEY`), `--token-addr`, `--public`, `--cert` and `--cert-key`;
  - modules `token` (`TOKEN_SECS` 60, `TIMEOUT_SECS` 3) and `tls`;
  - the `mint_token` example;
  - joins on `next_slot()`; `History::remove` is removed;
  - new dependency `tokio` (rt), already in the tree.
- **conker-client:**
  - options `token`, `token-url`, `ca`;
  - `connection::token_url`, `connection::secure`;
  - `known_match(…, grenades)`;
  - native dependencies `aeronet_webtransport =0.21.0` and `lightyear_aeronet =0.30.1`.
- **web:** the token fetch, `?token-url=`, a random `id`, `wss://` and no digest on https pages, the centred ROM
  picker.

### Honest gaps
- **Not shown in a browser:**
  - an https page: WebTransport without a digest, `wss://` from https, the same-origin `/token` over https;
  - the wasm client always passes an empty `serverCertificateHashes` list (lightyear 0.30.1). That Chromium treats an
    empty list as "no pinning" is inferred.
- **Tokens:**
  - a token lasts 60 s from the ROM pick, so a load slower than that fails to connect and the page needs a reload;
  - the endpoint answers one request at a time, with a 2 s read limit (`ponytail:`), and has no rate limit (tokens
    are free to anyone anyway).
- **Native client:**
  - the token fetch is plain http with no connect timeout (`ponytail:`);
  - WebTransport without a digest or `ca=` still fails: lightyear pins an empty hash list;
  - the digest path is still IPv4-only; only `ca=` binds dual-stack.
- **Operations:**
  - each certificate renewal restarts the game server, so matches in progress end (the README shows how to move
    certbot's timer);
  - the Linux build of the server was not tried (Docker was not running), so the README's package list and swap note
    are inferred;
  - locally, Caddy's http-to-https redirect names port 443, not the test port (right in production).
- **Ports:** every server now also takes TCP PORT+1, so two servers on adjacent ports collide; the second one exits
  with a message naming `--token-addr`. The default 5888 also takes 5889.
- **Prediction:**
  - every throw by another player costs each client one invisible rollback, and so does a leave with a grenade in
    flight;
  - server-decided events still mispredict for the client's lead: pickups (smoke run 2 above), a bot's hits, random
    picks;
  - pushing against a moving remote player: e_w09's victim drifted between 15 and 20 ticks over 0.001 on this loaded
    machine, with the old and new binaries alike.
- **Slots:**
  - the bots treat a free slot between occupied ones as a human who is always dead (`Bots::new` takes a count, so a fix
    is a conker-bots API change);
  - a joiner who takes a free slot makes the server rebuild the lifts, so nobody can take a lift for one tick
    (`ponytail:`). `Lifts::remove` in conker-bots is now unused.
- **Native HUD:**
  - shows the browser's wording, `Reload to reconnect` included;
  - the `connecting…` state was not captured, and Bevy's default font may lack the ellipsis.
- **Test setup quirks** (not regressions):
  - with `--wait`, every page takes one 13-14-tick rollback at the match start;
  - bots roam and hit humans before the start.
- **`bots.sh`:** "bot 1 fights, dies and respawns" failed in all 4 traffic runs, before and after (the wave-4 flake).
- **Docs:** `../RUST_REMAKE_PLAN.md` has no wave-6 line yet.

### Only a real host can show
- Chrome trusting the certificate: WebTransport without a digest, `wss://` from the https page, and `/token` on the
  same origin over https. This is the first check to run on the host.
- Let's Encrypt issuing and renewing: `certbot --standalone` with the pre, post and deploy hooks, and
  `renew --dry-run`.
- The systemd unit's hardening, and the hook's file permissions (Caddy and the `conker` group reading
  `/etc/conker/tls`).
- Caddy on ports 80 and 443, with HTTP/3.
- IPv4 into the `[::]` sockets on Linux: `sysctl net.ipv6.bindv6only` must print 0, and `ss -tuln` must show `*:5888`
  for tcp and udp. A page loaded from an IPv4-only network shows it end to end.
- The server built and run on Linux.
- Latency and loss over a real internet path; until now lag was only added on loopback.

### Next
Wave 7: the visuals, Firefox and Safari 26.4+, the real deployment once there is a host and a domain, and the small
leftovers above. The list is in `../ROADMAP.md` ("Wave 7"); the rest moved to "Wave 8".

## Wave 5 (2026-10-10)
Wave 5's summary, as this file's top read after it:

**Wave 5: the connection is robust and corrections are smooth on screen. Other players' grenades are still not
predicted (blocked).** Wave 5 is the first half of option A, "ready to play online", in `../ROADMAP.md`:
- the server listens on IPv4 and IPv6;
- a player who leaves is gone at once;
- the page says what the connection is doing;
- a correction from the server no longer snaps the local player's model and view.

It was verified natively through the server, and in Chrome 153 over WebTransport and WebSocket with 2 bots and 150 ms
of lag each way.
- **Leaving:**
  - A closed page frees its slot 4-38 ms after `pagehide` (7 closes, both transports).
  - A native client sends netcode's disconnect.
  - A killed client is dropped after 0.1 s over WebSocket and 2.97 s over WebTransport, the netcode timeout
    (10 s → 3 s).
  - Rejoining with the same id works at once.
- **Start-up:** 0 start-up rollbacks (wave 4: 1 per client). The server counted the new player's spawn immunity down
  from its join, while the client started from its first input. A new human now waits until its first input.
- **Traffic:** each player's `Fighting` now goes to its own client only (protocol 7). Server → client traffic fell from
  17.7 to 9.4-9.9 KB/s per client with 2 humans and 2 bots.
- **Smoothing:** the simulation still corrects at once; only the drawing blends, over 8 ticks or at most 20 units a
  tick. On the victim's screen in Chrome at 150 ms (and the same headless):
  - a twin-gun hit moves the player 3.31 units a tick instead of 26.48 in one frame;
  - a grenade's knock-back moves it 19.7 units a tick instead of 335.

  Prediction and lag compensation are unchanged (rewound target vs the shooter's screen, max 0).
- **HUD names** match the models in the hand: rifle, throwing knives, scoped revolver, katana, chainsaw.
- **Partial:** other players' grenades are not predicted. Protocol 7 no longer sends their state to other clients.
  **Wave 6: predicted (protocol 8).**

**Fixes after wave 4 (2026-10-10).**
- **Model height:** the game draws a player's model 0.5 × 43 above its position. The 43 is bank 0x11's per-model
  record +6, copied to obj+0xC8 and used by `func_1502DF38`. conker-client's `MODEL_LIFT` lifts the model child, not
  the player entity.
  - Before: the feet were 21.5 units under the floor.
  - Now (V against the game's joint matrices in RAM): idle, run, crouch and crouch-walk within 0.06.
- **Held weapons:** drawn from the game's own bank-09 models (twin guns 26/27, rifle 101, 0x23 65, sniper 77,
  flamethrower 48, 0x21 58, 0x22 55, knives 33, rocket launcher 32, grenade 75) on the hand joints (right 9, left 4).
  - Placement: no offset of their own (C, V in RAM).
  - Timing: shown and hidden on the draw/holster clips' events (C, V on the twin guns, flamethrower and grenade).
  - API: `extract_character::held`, `Slice.held`/`held_glb`, `conker_client::held`.
  - Gaps: 3 twin-gun materials untextured (partial texture loads); the weapon hides 3-4 frames early at death; C-down
    with the twin guns or flamethrower drawn throws a grenade here but not in the game.
- Screenshots: `../lab/screens/fixes/`.

Four agents (workflow `wf_79fd19eb-e93`). The connection and corrections agents built in parallel on one tree (both
edited `conker-client/src/lib.rs`, each only its own functions); then came a browser test, then this file.

| Agent | Wrote | Status |
|---|---|---|
| connection | conker-server, conker-sim `net.rs`, conker-client's connection parts (new `connection.rs`), `web/` | done |
| corrections | conker-client's prediction, drawing and HUD (new `smooth.rs`, `render.rs`, `predict`, `weapon_name`) | **partial**: other players' grenades are blocked by protocol 7 |
| browser | `web/index.js`, `render.rs`'s `update_hud`, `connection.rs`'s `tidy`, the `web/dist` bundle | done |
| status | this file, `../ROADMAP.md` | done |

Lab outputs, in `../lab/screens/wave5/`:
- **Smoothing:**
  - `corrections.md` holds the design, the limits and the full before/after tables; this section summarises it.
  - `smooth_report.py` computes the per-frame numbers from a run's logs; `measure.sh` runs combat.sh and then the
    report.
  - `runs/` holds measure.sh's outputs: protocol 6, then `p7/`, `push/` and `final/`.
  - `frames.sh`, `frames.log` and `gren-*.png` are the screenshot frames.
- **Connection:**
  - `scripts/{kill,rejoin,run3,rbsum,offscreen2}.sh`;
  - `logs/*.tgz` (the smoke, combat and traffic runs), `bytes-{before,after}-120.txt`, `combat-*.txt`,
    `oracle-tests.txt`, `chrome-server*.log`.
- **Browser test:**
  - `scripts/serve.py`: a static server that sends `Cache-Control: no-store` and saves page logs and `pagehide`
    beacons;
  - `scripts/hook.js`: the DevTools init script, which keeps STATE/SEEN/DRAWN/CORRECTION lines out of the console and
    counts foreign input;
  - `browser/`: page and server logs;
  - `browser-*.png`.

### Verification run (status agent, rustc 1.98.1, all re-run here on 2026-10-10)
| Check | Result |
|---|---|
| Extractor tests, 6 crates, ROM + the three DOS variables (absolute) | **88/88 pass** (wave 4: 86), nothing skipped; the DOS checks pass |
| conker-camera | 30 unit + 7 oracle/ROM tests pass |
| conker-anim | 9 unit + 1 oracle + 4 ROM tests pass |
| conker-rules, debug and release | 51 unit + 3 combat + 4 oracle + 2 ROM tests pass. Combat replay unchanged: 506/508 (29 traces), 172/172 (9 combat4 traces) |
| conker-bots, release | 13 unit + 3 oracle + 3 ROM tests pass |
| conker-sim, debug | 44 unit + 11 oracle tests pass, `report_all` ignored |
| conker-sim, `--release --test oracle -- --include-ignored` | **12/12 pass**. Numbers unchanged: camera4_w23 0.087; scope facing misses 69 / 894 / 224; zone gate: humans 161,317 rows, 0 differ; bots 80,625 rows, 6 differ |
| target-net: conker-sim `--features net`, conker-rules `--features net`, conker-server, conker-client | 45 + 11 (1 ignored); 52 + 3 + 4 + 2; 8; **29** (wave 4: 16). All pass |
| `cargo check --target wasm32-unknown-unknown --lib` for the 11 library crates, and for conker-client (target-net) | clean |
| `cargo fmt --all --check` | clean. The connection agent saw diffs mid-wave, in lines then being edited; none are left |
| `cargo clippy --all-targets`: the 11 default-target crates; conker-client, conker-server and conker-sim `net` in target-net | 0 warnings |
| `smoke.sh wt ws ws`, twice (`PORT=5940`) | **16/16 ok in both runs**. Prediction vs server: 684 / 684 / 269 and 682 / 684 / 269 ticks, max error 0, 0 state misses. **0 start-up rollbacks** (wave 4: 1 per client). Run 2's client 2 took one 3-tick rollback after a grenade pickup at t=626: the pickup is server-decided, and nothing moved on screen (no `CORRECTION` line) |
| `combat.sh combat_e_w09 09` (`PORT=5941`) | 39/43 events, clock 742/742. Shooter: 1 tick over 0.001 (0.039). Victim: 15 ticks over 0.001, max 26.48, 7 rollbacks. Lag compensation: 122 bullet tests, all on `Saw`; rewound vs shown, 102 tests, max 0 |
| Rust source | 32,757 lines in 13 crates, tests included (wave 4: 31,362) |
| `web/dist` | not rebuilt here. Re-measured: `client_bg.wasm` 21,795,958 B, 6,877,620 B with gzip -9 (the browser agent's build); `client.js` 124,691 B / 17,536 B |

Not re-run here:
- the browser test;
- `bots.sh`;
- the 150 ms combat runs, e_gren and e_w15;
- the kill, rejoin and traffic scripts.

Their numbers below are the agents'. Every process started for these checks was stopped.

### What players notice
- **Status line:** the bottom left of the page says what the connection is doing:
  - `loading the level`, then `connecting to HOST:PORT over WebTransport…`, then `connected to ...`;
  - `could not connect to ...: REASON. HINT` if it fails;
  - `disconnected from ...: REASON. Reload to reconnect` if it is lost.

  In the browser the HUD is empty while there is no player.
- **Addresses:** `http://localhost` works over WebTransport (it hung in wave 4). `[::1]` and `127.0.0.1` work too.
- **Leaving:** a closed page or a quit client disappears for the other players at once (0.34-0.38 s in Chrome with
  150 ms of lag; wave 4: about 10 s). A killed client disappears after 0.13 s over WebSocket and 3.1 s over
  WebTransport.
- **Reload:** reloading after a server restart reconnects. An address saved before wave 5 that still holds `digest=`
  must have it removed once.
- **Hits:** a hit no longer snaps your own model and view. A twin-gun hit moves you 3.3 units a tick for 8 ticks
  (26.5 in one frame before); a grenade's knock-back moves you about 20 units a tick for 17 ticks (335 before).
- **HUD names:**
  - 0x39 is `rifle` (was `pistol`);
  - 0x14 is `throwing knives` (was `scoped gun`);
  - 0x23 is `scoped revolver` (was `one-shot gun`);
  - 0x21 is `katana` (was `melee`);
  - 0x22 is `chainsaw` (was `blade`).
- **Gamepad:** a gamepad drives only the window that has focus.
- **Slots:** when a player with a lower slot leaves, the others move down a slot. Each of them takes one rollback
  (3-14 ticks). In the one case checked (Chrome, 14 ticks) it moved nothing on screen. **Wave 6: slots are stable;
  nobody moves and nobody takes a rollback.**

### Connection (connection agent; the browser agent's fixes)
- **Dual-stack:**
  - The server listens on `[::]:PORT` for WebTransport (UDP) and WebSocket (TCP).
  - `page_server` keeps the page's host (it turned `localhost` into `127.0.0.1`).
  - Connected in Chrome: `localhost` over WebTransport (Chrome resolves it to ::1), `[::1]` over WebSocket, and
    `127.0.0.1` over both.
- **Leaving.** The netcode timeout is 3 s (`connection::TIMEOUT_SECS`, carried in the connect token; wave 4: 10 s).
  - A native client sends netcode's disconnect and exits `LEAVE_DELAY` (0.2 s) later. This covers `--seconds`, a closed
    window and a screenshot run's exit.
  - A page can't send it, because no frame runs after `pagehide`. Instead, winit's exit drops the session, which closes
    the transport.
  - The server logs `client X left: REASON`; other clients log `player X left` and stop drawing it.

  | Case | Slot freed | Other clients drop the player |
  |---|---|---|
  | page closed (Chrome) | 4, 4 and 13 ms after `pagehide` (connection agent); 29 / 38 and 24 / 12 ms, WS / WT (browser agent) | 0.34 / 0.38 s, with 150 ms of lag |
  | native exit | at once (`left: Disconnected by peer`) | – |
  | client killed, WebSocket | 0.00-0.09 s (the TCP connection drops) | 0.13 s |
  | client killed, WebTransport | 2.97 s (the timeout) | 3.11 s |
  | server killed, as seen in a Chrome page | – | status line after 20 ms (WS), 52 ms (WT) |

  Rejoining with the same id worked at once: natively after a clean leave, after a WebSocket kill and 3.5 s later
  (`rejoin.sh`), and from a page in Chrome.
- **Start-up rollbacks.** Before: 1 per client in smoke.sh.
  - Cause: the server counted the new player's spawn immunity (`Fighting.immune`) down from its join, but the client
    predicts from its first input, 16-20 ticks later.
  - Fix: a human waits (`Waiting`) until the first tick its client sent an input for. Meanwhile its fighter is restored
    after each step, and its camera stays still.
  - Fix: the client adds `InputMarker` together with `FollowCamera` (`on_controlled` is removed), so prediction and
    input start on the server's tick.
  - Result: 0 start-up rollbacks for 9 clients in 3 runs, and for 6 in this file's 2 runs.
- **Connect gate:** the client connects only after the level and the character have loaded and 10 frames in a row
  took under 0.25 s (`CALM`, `CALM_SECS`). That took 0.63 s and 1.04 s in Chrome. A 10 s watchdog (`CONNECT_SECS`)
  fails an attempt that never answers.
- **Status:** `connection::NetStatus` holds the state: loading, connecting, connected, failed (with a hint) or
  disconnected (with the reason).
  - Every change is logged as `connection: ...` and written to the page's new `#net` line.
  - `why()` and `tidy()` clean the error texts. The browser agent made `tidy` drop a cause that repeats the one before
    (`connection lost: Connection lost`).
- **Gamepad:** `buffer_input` reads the pad only while a window has focus (in the browser: the canvas).
- **`Fighting` to its owner only:**
  - A replicon visibility filter (`FightingOwner`) sends each player's `Fighting` to its own client; bots' go to no one.
  - `Combat` gains `slot`, `team` and `stun`, and other clients build a stand-in fighter from it (`rival()`).
  - Protocol 7.
  - Traffic per client, 2 humans + 2 bots, 120 s: server → client 17.7 → 9.4 / 9.9 KB/s; client → server unchanged
    (4.5 / 5.6 → 4.6 / 5.8).
- **Through the server**, the connection agent's runs:
  - smoke.sh ×3: all pass, max error 0. One run had a 4-tick rollback on client 3, the slot shift after clients 1 and
    2 left.
  - combat.sh, as wave 4 or within its spread:
    - e_w09: victim 13 ticks over 0.001, 7 rollbacks;
    - e_w09 at 150 ms: victim 80 ticks;
    - e_gren: victim 4 ticks, 2 rollbacks;
    - e_w15: shooter 10-14 ticks, max 9.7-17.3;
    - lag compensation max 0 throughout.
  - `bots.sh` failed its "bot 1 dies" check before and after the change, the known wave-4 flake.
  - `offscreen2.sh`: two debug clients stayed connected with 11 busy loops running.

### Smooth corrections (corrections agent)
- **How:** a rollback still moves the predicted body at once, so the simulation stays exact; only the drawing changed
  (new `smooth.rs`).
  - The local model is drawn where and as it was drawn before the correction. It then blends linearly to the predicted
    body over 8 live ticks (`BLEND`), or longer so that it never adds more than 20 units a tick (`MAX_STEP`).
  - A second follow camera, stepped with the drawn body, gives the view during the blend. It then hands the view back
    to the follow camera over 8 ticks (`HANDOFF`).
  - Jumps over 600 units (`SNAP`) are drawn at once. 600 is less than the distance between the two closest spawn
    records (673) and more than a grenade's knock-back. A predicted respawn or a camera cut ends a blend.
  - What reads the drawn values: only `render.rs`'s `place_players` and `follow_camera`. `Player`, `Fighting`, the
    follow camera, the `Aim` history and the input's camera yaw are unchanged.
- **Numbers.** Headless, the victim's client, per frame, before → after, 2 runs each (`corrections.md`, `runs/`):

  | Encounter | Correction | Player moves | Look-at moves | Facing turn per correction | Settles after |
  |---|---|---|---|---|---|
  | e_w09, 0 and 150 ms | 26.5 | 26.48 → 3.31 (5.82 when two overlap) | 26.5 → 3.3-6.1 | 1,025 → 129 | 8 ticks |
  | e_gren, 0 ms | 132 | 132.1 → 16.5 | 49.3 → 33.0 | 2,230 → 140-279 | 8 ticks |
  | e_gren, 150 ms | 318-349 | 318-349 → 19.4-19.9 | 126-144 → 28-31 | | 16-18 ticks |
  | e_w15, 0 ms | 55 | 55.3 → 6.9 | 62-63 → 20-21 | 16,257 → 2,201 | 8-15 ticks |
  | e_w15, 150 ms (the burning run) | 155-158 | 155-158 → 19.4-19.9 | 169-172 → 33-34 | 5,940-10,190 → 823-1,274 | 34-35 ticks |
  | e_w15, the pushing client | 0.6-11.9 | 7.8-11.9 → 1.5-2.4 | 9.4-12.2 → 3.5-3.9 | | |

  - Body kinks (changes of per-tick velocity) at e_gren 150 ms: 335-362 → 33-36.
  - The drawn view's yaw is at most 3.5° off the follow camera's.
  - The eye moves less in e_gren at 150 ms (45-47 → 27-30). It moves more where the drawn camera chases a fast blend
    on top of the blast or burn flight: e_gren at 0 ms 41.8 → 47.5, e_w15 28.3 → 30-32. That motion is smooth: eye
    kinks 16.9 → 17.7 and 9.6 → 4.6-5.2.
  - Cost: the drawn player is off the server's position during the blend too (e_w09 at 150 ms: frames over 0.001,
    118-139 → 181-210).
- **Pushing** (`pushing_body`):
  - The other body that the server's step pushes against comes from that player's server history at the right tick:
    after the tick if it moves first in slot order, else the tick before.
  - Past the end of that history, it carries on at its last step's pace.
  - e_w15's pushing client: 13-14 → 11 ticks over 0.001 at 0 ms (max 13.2-15.1 → 11.06), and 22-24 → 21 at 150 ms
    (max 23.5 → 15.08). The victim is unchanged (its corrections are the server-decided flame hits).
- **Other players' grenades: blocked.** **Wave 6: done, as proposed here.**
  - Protocol 7 sends `Fighting`, which carried each grenade's full state, to its owner only, and `MatchView.grenades`
    has positions only.
  - Needed: each grenade's `(birth, Grenade)` sent to every client (a conker-rules `net` component and a protocol
    bump), then flown in the client's own tick by `Match::fly_grenades`' rule.
  - It would remove e_gren's victim mispredictions: 3-4 ticks at 0 ms (max 101-132) and 12-14 at 150 ms (max
    318-349).
- **HUD names** (`weapon_name`): checked against `../lab/screens/fixes/weapon-*.png`, as listed above. The twin guns,
  sniper rifle, flamethrower and rocket launcher keep their names.

### Browser (browser agent; Chrome 153, macOS, M2 Max; not re-run here; `web/README.md`, "Wave 5 tested in Chrome 153")
**Setup:**
- Game server on port 5895 with `--bots 2`.
- Page 1 over WebTransport (`id=1`, slot 2) and page 2 over WebSocket (`id=2`, slot 3), both with `lag=150`.
- The connection agent ran its own pass first, on ports 5890 and 8090.

**Three fixes, rebuilt and re-tested:**
- `index.js` reads `dist/digest.txt` when the ROM is picked. It used to write the digest into the page's address, so a
  reload after a server restart failed over WebTransport.
- In the browser, the `#hud` is empty without a player (it said `connecting...` next to a failed connection).
- `tidy` drops a repeated cause.

**Results:**
- **Status line:**
  - Every state was shown, opened before the server and with the server killed (above).
  - A page opened before the server connected once the ROM was picked after the server came up.
- **Hits on the victim's page** (`smooth_report.py` on the page logs):
  - Five twin-gun hits, each corrected by 26.48 units: 3.31 a tick on screen (6.62 in the one 54 ms frame); look-at
    26.47 → 6.62; facing 1,025 → 257. Settled after 8 ticks (27 once).
  - A grenade's knock-back of 334.6 units, blended over 17 ticks at 19.7 a tick: look-at 135 → 30, eye 46 → 27, facing
    2,170 → 128.
- **Prediction:**
  - Page 1: 3,572 ticks, 1 over 0.001.
  - Page 2: 61 over 0.001 (5 hits × 12), 0 state misses.
  - The respawn was predicted on the server's tick and record.
- **Lag compensation:** 108 bullet tests, all on the page's `Saw`; 324 comparisons of rewound target against page, max
  0.000000.
- **Bot melee kills:** after each one, the victim's page rolled back nearly every tick for 23-45 ticks. The bot stood
  41-46 units from the body (pushing against a moving remote player). On screen: 58.5 → 8.8 per frame.
- **Held weapons and HUD:** the twin guns are in both hands of both models on both pages, with feet on the floor. Page
  2 named each given weapon as above.
- **Frames and errors:** 60 fps (median frame 17 ms). 0 console errors other than Chrome's own WebSocket error while
  the server was down. 0 ERROR/WARN in the server logs. No foreign input.
- **Bundle** (`build.sh web`, wasm-opt): 29,854,842 B rustc output; 26,865,094 B after wasm-bindgen; **21,795,958 B /
  6,877,620 B gzip** shipped (wave 4: 21,723,068 / 6,849,029). The build took 3 min 13 s after a client change.

### Screenshots (`../lab/screens/wave5/`)
All are Chrome 153. `p1` is the WebTransport page and `p2` the WebSocket page.

| File | Shows |
|---|---|
| `browser-connected-wt-localhost.png`, `browser-reconnected-ws.png`, `browser-reconnected-ws-ipv6.png` | connected over `localhost` (WT) and `[::1]` (WS); reconnected after a server restart |
| `browser-failed-wt-server-down.png`, `browser-failed-ws-server-down.png`, `browser-failed-wt-stale-digest.png`, `browser-before-p{1,2}-*-no-server.png` | the red `could not connect` lines. The older HUD still said `connecting...` |
| `browser-disconnected-{ws,wt}-server-killed.png`, `browser-p{1,2}-*-server-killed.png` | `disconnected ...` after the server was killed (before the `tidy` and HUD fixes) |
| `browser-fixed-wt-server-killed.png` | the new bundle: only `disconnected from localhost:5895: connection lost. Reload to reconnect`, no HUD |
| `browser-bug-p1-wt-reload-keeps-old-digest.png` | the `index.js` bug: the server running, the reloaded page still failing |
| `browser-p3-wt-opened-before-server-connected.png` | a page opened before the server, connected after a late ROM pick |
| `browser-p1-wt-twin-guns-drawn.png`, `browser-p2-ws-twin-guns-drawn.png` | twin guns in both hands of both players, feet on the floor. The white quad is still near record 1 |
| `browser-p2-ws-knives-0x14.png`, `browser-p2-ws-scoped-revolver-0x23.png` | HUD `throwing knives (drawn)` and `scoped revolver (drawn) 6 rounds`. The weapon itself is not visible from behind |
| `browser-p1-wt-sees-p2.png`, `browser-p2-ws-dead-by-bot.png` | after the knock-back; page 2 dead from a bot (`DEAD - respawn in 0.5 s`) |
| `gren-936.png` … `gren-960.png` | native offscreen, e_gren at 150 ms, the victim's view from 2 ticks before the correction to 23 after (one run per frame) |

### Per crate (wave 5)
| Crate | Rust lines (wave 4) | Tests (re-run here) |
|---|---|---|
| conker-client | 3,364 (2,339; `held.rs` came with the fixes after wave 4). New: `connection.rs` 513, `smooth.rs` 307 | 29 unit (16) |
| conker-server | 1,370 (1,249) | 8 unit, plus the scripts |
| conker-sim | 5,783 (5,774) | 44 unit (45 with `net`) + 12 oracle |
| conker-rules, conker-camera, conker-anim, conker-bots | unchanged: 6,321, 3,064, 1,071, 3,889 | unchanged |
| 6 extractor crates | 7,895 (7,655; the fixes after wave 4) | 88 |

**API changes in wave 5:**
- **conker-sim `net`:** `PROTOCOL_ID = 7` (**Wave 6: 8**). `Combat` gains `slot`, `team` and `stun`.
- **conker-server:**
  - binds `[::]`;
  - netcode timeout 3 s;
  - the `FightingOwner` visibility filter;
  - a joining human waits (`Waiting`) until its first input;
  - `combat(f, slot, tick, lives)`;
  - logs a `NET` traffic line per client every 10 s, and `client X left: REASON`.
- **conker-client:**
  - New module `connection`:
    - `build(app)` replaces `connect`, `on_connected`, `on_disconnected`, `delay_sends` and `Outbox`;
    - `NetStatus` with `text()`, and `why`;
    - `TIMEOUT_SECS` (3), `CONNECT_SECS` (10), `LEAVE_DELAY` (0.2), `CALM` (10), `CALM_SECS` (0.25).
  - New module `smooth`:
    - `Drawn::{correct, step, tick, body, facing, view, reset}`;
    - `BLEND` (8), `MAX_STEP` (20), `HANDOFF` (8), `SNAP` (600);
    - systems `note_corrections` (PreUpdate, after the rollback) and `log_drawn`.
  - `FollowCamera.drawn`.
  - `rival(&Combat, body) -> Fighter`, `pushing_body(h, slot, own, tick)`; the private `known(others, slot, tick)`.
  - `InputMarker` is added with `FollowCamera` (`on_controlled` is removed).
  - `page_server` keeps the host.
  - New debug lines `DRAWN` (every frame) and `CORRECTION`.
- **web:** the `#net` line, and `index.js` reads `dist/digest.txt` at the ROM pick.
- **Scripts:** `PORT` for `smoke.sh` and `combat.sh`.

### Honest gaps
- **Prediction:**
  - Other players' grenades are not predicted (blocked, above). **Wave 6: predicted.**
  - Each event the server decides (a hit by someone else, a pickup, a random pick) still mispredicts for the client's
    lead. Wave 5 only hides it on screen.
  - Pushing against a moving remote player still mispredicts (e_w15: 11 / 21 ticks; after a bot's melee kill in
    Chrome, 23-45 ticks of rollbacks).
- **Slots shift on leave:** `Match::remove_player` moves every later slot down. Each remaining client with a higher
  slot takes one rollback (3-14 ticks, on `Fighting.slot`). This was hidden while leaves took 10 s. Stable slots need
  a conker-rules change. **Wave 6: done (`Match.free`).**
- **Smoothing:**
  - A correction that arrives during the camera hand-off restarts the drawn camera's weight at 1. The view then jumps
    by the remaining gap: up to about 60 units for a second blast 10-25 ticks after a first. No measured encounter
    triggered it.
  - The drawn camera can take the level hint's cut on a different tick than the follow camera.
  - During a blend, the stick is relative to the follow camera's yaw (up to 3.5° off the view shown).
  - The eye moves faster during fast blends.
  - The local player moves once per 30 Hz tick, so on a 60 Hz display every other frame shows no move (as before).
- **Leaving and joining:**
  - A page can't send netcode's disconnect; its transport's close frees the slot instead.
  - A WebTransport client killed without a disconnect stays 3 s, and rejoining with the same native id is refused
    during those 3 s (browsers pick a random id).
  - Ctrl-C in a terminal skips the disconnect path, because another crate installs its handler before Bevy's.
  - A player still waiting for its first input can't be hurt or moved, but a lethal hit on it would still count for
    the attacker. That needs 6+ damage within about 0.5 s of joining; it is marked `ponytail:` in the server.
  - A debug client that stalls for over 3 s after connecting can still time out.
- **Transports:**
  - The `[::]` bind relies on the OS's dual-stack default: on for macOS and Linux, off for Windows (`IPV6_V6ONLY`).
  - Native WebTransport clients reach IPv4 only: lightyear 0.30.1 binds them to `InAddrAnyV4`. **Wave 6:** still so
    with a digest; with `ca=` they bind dual-stack.
  - Chrome reports a stopped server and a stale digest with the same `Opening handshake failed`, so the line names
    both.
  - From a remote server, a WebTransport page may notice a dead server only after the 3 s timeout (I).
- **Untested:**
  - closing a real native window (a headless unit test only);
  - a real gamepad with two windows;
  - the 10 s connect watchdog in a browser;
  - the held knives and revolver from the front;
  - background tabs;
  - Firefox and Safari.
- **Native:** the HUD still says `connecting...` after a failed or lost connection; native clients have no status
  line. **Wave 6: the native HUD shows the status and its reason.**
- **Logs:** `log=conker_client=debug` now also logs `DRAWN` every frame, about twice the lines (the lab's `hook.js`
  keeps them out of the console).
- **Bots:** `bots.sh`'s "bot 1 dies" check still fails in most runs (wave 4's gaps).
- **Docs:**
  - `../RUST_REMAKE_PLAN.md` has no wave-5 line yet. **Wave 6: it has one.**
  - `web/README.md`'s older sections keep the old weapon names and the old WebTransport text, as a record.
  - Rendering, deployment and the browser list are unchanged from wave 4's gaps (flame particles, projectile models,
    the twist on the model, the white quad, the twin guns' 3 untextured materials, `wss://`, real certificates).
    **Wave 6:** `wss://` and real certificates are prepared (`deploy/`), not yet on a host.

### Next
Wave 6 is the rest of option A: secure deployment, the visuals and Firefox/Safari, plus wave 5's leftovers that fit.
The list is in `../ROADMAP.md` ("Wave 6"), with the rest moved to "Wave 7". **Wave 6** took the deployment and the
leftovers; the visuals and the browsers moved to wave 7.

## Wave 4 (2026-10-09 to 2026-10-10)
Wave 4's summary, as this file's top read after it:

**Wave 4: combat, its netcode and the controls are done; the class-6 slide and the bots are partial.** Deathmatch
now has all nine weapons and the grenade's cook timer. Every aimed hit is tested where the shooter's own client showed
the target, and clients predict their own hits, deaths and respawns. The remaining controls (C-strafes with the stick
held, R's upper-body twist, the scopes, the camera's armed aim) and movement rules (players pushing each other, root
motion, the wall routine's zone gate) are ported. A new crate, `conker-bots`, runs the original's own AI script for
server-side bots. All of it runs natively through the server and in Chrome 153 over WebTransport and WebSocket, with
2 bots and 150 ms of lag each way.
- **Combat:** replayed through conker-rules, 506 of 508 wave-3 oracle events (wave 3: 487) and 172 of 172 events of
  9 new traces land on the exact frame. The 2 misses are e_w15's burning player hitting a wall: the burning run's
  steering is not ported.
- **Netcode:** the rewound target equals what the shooter's screen showed, max 0.000000 (e_w09 re-run here with 0 and
  150 ms each way; the netcode agent's mutual-fire duel, 398 bullet tests; 918 comparisons in Chrome). e_w09's victim
  now mispredicts only between a hit and the server's correction: 16-23 ticks over 0.001 (wave 3: 44).
- **Movement and camera:** all 28 movement traces stay within 0.001 with 0 state misses in both camera modes, with 2
  excluded ranges left (wave 3: 6, about 810 rows); 28 combat and armed traces stay within 0.087. The camera is
  bit-exact on the 12 camera traces and on 16 of 18 armed traces.
- **Partial:**
  - the class-6 slide is not ported (about 960 lines of MIPS, and one trace);
  - bots: the original's navigation matches (391 of 391 node changes), but 3-bot matches score fewer kills than the
    original's (mean 6.8 against 10), and a bot can stay alive for a whole match (sniper camps, a pit it can't leave):
    `bots.sh`'s "every bot dies" check failed in the bots and netcode agents' runs (it passed in this file's one run).

Seven agents (workflow `wf_b5b7bf1a-42d`). Several were interrupted and resumed; the bots agent's result is its third
attempt, and the camera and movement agents finished work an earlier run of theirs had started.

| Agent | Wrote | Status |
|---|---|---|
| movement | conker-sim (but `follow.rs`), extract-character, conker-extract | **partial**: the class-6 slide is not ported |
| camera | conker-camera, conker-sim `follow.rs` | done |
| weapons | conker-rules, conker-anim | done |
| bots | new conker-bots, conker-server | **partial**: kills below the original's; `bots.sh` fails "every bot dies" |
| netcode (integration) | all crates, `web/` | done |
| browser | conker-client, `web/` | done |
| status | this file | done |

Lab outputs (`../lab/oracle/` unless named):
- Notes, the detailed record this section summarises: `camera4_README.md`, `combat4_README.md` (extends
  `combat_README.md`) and `bot_README.md`. The movement agent wrote no note: its rules are below and in
  `movement.rs`'s doc comments.
- Traces (pinned):
  - `camera4_w09` … `camera4_w39`: each of the 9 weapons drawn, run, turned and aimed with R (the stick in every
    direction), plus `raw/camera4_*.bin` (about 7 MB each, kept: the tests read them) and `camera4_scope_watch`.
  - 20 `combat4_*` traces (scripts for both players, `.csv`/`.raw`, event tables, lldb captures): the lock-on range,
    the sniper's pitch, weapon 0x14's aimed shot and probe, the rocket, the grenade's cook timer, two players dead at
    once, and the burn and the RNG word logged at the random picks (`combat4_e_w09`, `e_w15`, `e_w21`, …).
  - `bot_long3` (1 idle human + 3 bots, 5 minutes; its first 6,146 reads equal `combat_bots3`) and `bot_long2`
    (2 bots), plus lldb captures of the AI (`bot_states`, `bot_bullets`, `bot_natives`, `bot_probe1`, `bot_watch1`)
    and `raw/bot_dumps/` (8 MB).
- Tools: `camera4_run.sh`, `combat4_lldb.sh` (logs the RNG word at the drawing functions), `move4_raw.py`,
  `move4_scope_model.py`, `bot_run.sh`, `bot_lldb.sh`, `bot_lscript.py` (the AI script's disassembler), `bot_stats.py`.
- `../lab/screens/wave4/`: screenshots, `shots.sh` and its scripts, the browser run's logs (`browser-logs/`) and
  `browser_compare.py`.

### Verification run (status agent, rustc 1.98.1, all re-run here on 2026-10-10)
| Check | Result |
|---|---|
| Extractor tests, 6 crates, ROM + the three DOS variables (absolute) | **86/86 pass** (wave 3: 85; extract-character gained the root tracks), nothing skipped; 1,272 DOS textures compared, all 1,770,276 clip values equal DOS's |
| conker-camera | 30 unit + 7 oracle/ROM tests pass |
| conker-anim | 9 unit + 1 oracle + 4 ROM tests pass |
| conker-rules, debug and release | 51 unit + 3 combat + 4 spawn/surface oracle + 2 ROM tests pass; the combat replay's numbers are under goal 1 |
| conker-bots (new), release | 13 unit + 3 oracle + 3 ROM tests pass |
| conker-sim, debug | 44 unit + 11 oracle tests pass, `report_all` ignored |
| conker-sim, `--release --test oracle -- --include-ignored` | **12/12 pass**; `report_all`'s numbers are in goal 3's tables |
| target-net: conker-sim `--features net`, conker-rules `--features net`, conker-server, conker-client | 45 + 11 (1 ignored); 52 + 3 + 4 + 2; 8; 16: all pass (Wave 5: conker-client 29) |
| `cargo check --target wasm32-unknown-unknown --lib`, 11 library crates (6 extractor + camera, anim, sim, rules, bots) | clean |
| The same for conker-client (target-net; includes conker-sim and conker-rules `net`) | clean |
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets`: the 11 default-target crates; conker-client, conker-server and conker-sim `net` in target-net | 0 warnings |
| `smoke.sh wt ws ws` (3 clients, the third mid-match) | **16/16 ok**; prediction vs server 696 / 695 / 281 ticks, max error 0, 0 state misses; 1 rollback per client (2-3 ticks; wave 3: 0). **Wave 5:** 0 start-up rollbacks (the spawn immunity was counted from the join) |
| `replay.sh` walk and run | each 247 rows, max error 0, 1 state miss (read 24, the known spawn frame) |
| `combat.sh combat_e_w09 09` | 39 of 43 oracle events, clock 742/742, lag compensation 122 tests all on the client's `Saw`, rewound vs shown max 0 |
| `combat.sh combat_e_gren -` | 9 of 11 events, clock 452/452 |
| `combat.sh combat_e_w15 15` (the flamethrower, new) | 17 of 19 events (the burning run's wall bump), clock 255/255 |
| `combat.sh combat_e_w09 09 150 150` | 39 of 43 events, clock 742/742; the victim 78 ticks over 0.001 (6 hits × 13); 122 bullet tests on `Saw`, rewound vs shown max 0 |
| `THIRD=1 combat.sh combat_e_w09 09` | the third client joined on slot 2, record 13, and left cleanly ("slot 2 left; 2 players"); the duel unchanged (39/43, clock 742/742, lag compensation max 0) |
| `HUMANS=2 BOTS=2 bots.sh 300` | **10/10 ok, exit 0** in this run: each bot moved, hit (32, 22), died twice and respawned twice; 5 lift rides. It failed in the agents' runs (goal 4) |
| `bot_match`, 3 bots, 9,000 frames, seeds 1-5 | kills 3 / 10 / 7 / 6 / 8, mean 6.8 (the original's bot_long3: 10); details under goal 4 |
| Rust source | 31,362 lines in 13 crates, tests included (wave 3: 22,507 in 12) |
| `web/dist` | not rebuilt here; re-measured: `client_bg.wasm` 21,723,068 B, 6,849,029 B with gzip -9 (wasm-opt run by the browser agent); `client.js` 124,533 B / 17,509 B (Wave 5: 21,795,958 B / 6,877,620 B; 124,691 B / 17,536 B) |

Browser runs were not repeated here (the browser agent's, below). Every process started for these checks was stopped.

### Goal 1: finish combat (weapons agent; conker-rules, conker-anim)
Details and traces: `../lab/oracle/combat4_README.md`. V unless tagged.
- **Weapons now ported** (wave 3's table, under goal 3 there, is updated in place):
  - **0x15 flamethrower** (`flame`): from 8 frames after Z, area hits of type 0x1C (1 damage, knocked at 30, 10
    immune frames, the 0x69 reaction) at the nozzle and along the stream; its length and hit count per frame are the
    traced ones (the particles are not simulated, I). V: first hits 828, 858, 948, 978, last 916. A hit sets the
    victim on fire for 125 frames, with a type-0x21 burn hit every 125 VIs (refused while immune or stunned), and the
    victim runs: speed to 28, the facing swaying around the move angle, a wall stopping it for 11 frames.
  - **0x14, an aimed shot** (pickup class 60): Z without R plays 0xF7 and hits nothing. With R held (the scope) Z
    fires an object at 75 a frame from the scope's eye moved 27 to the side (C, exact on probe14), toward whatever the
    scope's line meets first. Head: type 0x20, kills; body: type 0x10, 1 damage and the 0xD3 stagger. 40 frames
    between shots.
  - **0x16, a rocket launcher** (pickup class 58): Z fires a rocket at 200 a frame along the facing and moves the
    shooter back 91.6 by root motion (0x77). The rocket goes off where it first touches a body's area ellipsoid
    (radius 26, half height 43, centre +35: 119.07 along its path, the trace 119.05) or at the wall: two blasts, type
    0x16 (radius 40, kills) and 0x15 (radius 300, 2 damage), owner included. R held blocks the fire (as it does
    0x23's). 120 frames between rockets.
  - **The grenade's cook timer is its fuse** (C, V on every write): 240 at the pull, −2 a frame while C-down is held;
    the thrown grenade keeps counting down from it, and at 0 in the hand it goes off there. Throw speed
    `40 + (240 − cook)/4`, so 41 after a tap and 90 at most. This replaces wave 3's fuse table and covers holds of
    2-59 frames.
  - **Sniper 0x24 and 0x14's target**: the ray leaves the scope's eye (feet + 67.5) along the scope's yaw and pitch
    and tests body parts, not a column. Fitted (I, six shots): a head sphere at +66 (radius 20) and a torso sphere at
    +35 (radius 22), both 26 lower for a crouching player. `combat4_pitch24`: level → head (kill), 11.9° down → body
    (1 damage), 5.1° and 23.8° down → miss.
- **Melee lock-on**: range bracketed between 165.9 (locks) and 201.1 (doesn't) by `combat4_lock`, consistent with the
  code's 200 (C). The lock-on happens on the Z frame itself (wave 3: a frame late); the swing then ignores the stick
  and brakes 2.5 a frame.
- **Random picks**: the armed idle's fidget (`func_1507F640` → `func_1507EFD0`) and melee 0x21's swing (one draw: odd
  0x12A, even 0x129) are ported with their draws. The game's RNG can't be kept in step, so the replay feeds the game's
  word logged at the drawing functions; the port then picks what the game picked (e_w09 44/44, e_w21 43/43).
- **Wired from conker-sim** (`step_among`, `animate`, `root_motion`): players push each other, and root motion moves
  0x16's Z and the 0x17E deaths. Largest position error per trace, before → after: e_w14b 25.8 → 0.04, ctlw_k7 93 →
  0.009, e_w24 33.8 → 0.04, e_w22 27.4 → 0.04, e_w21 3.1 → 0.002, combat4_w24 33.8 → 0.007.
- **Timing and facing rules** that made those exact: root motion is measured at the end of the frame; the sniper's ray
  is resolved after that (its victim's 0x17E lasts 76 frames, a 0x14 shot's 75); the blade death spins the victim's
  facing toward the move angle by 3072 a frame; an outer-blast victim turns the same way, an inner-blast one keeps its
  facing; hit, dead and burning players are pushed too.
- **Two players dead at once** (`combat4_dead2`, C + V): the respawn picker counts a dead player where its body lies,
  and one that respawned earlier in the frame on its new record. This settles wave 3's open question; the port
  already did so.
- **conker-anim**: armed walk and run ids exact on the 9 armed camera traces (219 rows each, ids and positions); armed
  rates (`request_at`) exact on 845 rows once they count the speed percentage from before the frame (65 on a strafing
  frame), which makes the flamethrower's 0xED a run; melee 0x21's landing 0x19D lasts 14 frames (ctlw_k1's last
  miss).
- **The camera's inputs**: s126+0x78 (the drawn weapon, 0x26 with a grenade in hand) and s126+0x11A (weapon in the
  hand) equal the game's on every row of all 47 replayed traces (`Fighter::camera_weapon`, `in_hand`,
  `camera_view`).

**Combat replay** (`cargo test --release -p conker-rules --test combat -- --nocapture`, re-run here; conker-sim's
movement plus `Match::step`, both players):

| Set | Traces | Rows | Oracle events on the exact frame |
|---|---|---|---|
| wave 3's combat traces | 29 | 17,267 | **506 / 508** (wave 3: 487) |
| combat4 (w14, probe14, w16, dead2, w24, lock to row 995, pitch24, pitch14, cook) | 9 | 4,520 | **172 / 172** |
| the camera agent's camera4 probes, to row 249 | 9 | 1,971 | animation ids and positions exact on every row |

- Asserted on every row: health, match phase, kill-counter writes, s126+0x78 and +0x11A, armed rates (845 rows,
  error 0). Unchanged: 142 bullet births within 1.4e-6, 381 bullet steps exact, 519 of 523 bullet rows within 2.
  Respawns: 25 picks consistent with the picker, 21 of them independent of the RNG. Spawn picker: 131 picks and 714
  draws, 0 mismatches.
- Largest position errors left: e_w15 23.5 (player 1, pushed by the burning player) and 1,117.5 (player 2, the
  burning run); combat_buttons 4.96 and ctl_cd_cr 4.97 (from rows 204 and 84, short C taps during the throw; conker-sim
  alone, fed the trace's state, is exact there); combat4_cook's grenade 4.0 (bouncing up a ramp); combat4_pitch14
  1.43 (the body hit's knock angle, I); combat4_w16 0.89 (the rocket's hand position, I); combat4_lock 0.70 (row
  841). Every other trace is within 0.12.

### Goal 2: combat netcode (netcode agent; browser agent's fix)
**Lag compensation is in conker-rules.** `Match::step_seen(arena, inputs, world, seen)` takes
`seen(shooter, victim) -> Option<[f32; 3]>`, and every aimed hit test reads where its own shooter saw the target, per
bullet: bullets, rockets, 0x14's shots, the sniper's ray, the flame stream, the melee strike and the lock-on.
Explosions and fires are not rewound. The server's `compensate` is gone. Two fixes made it exact:
- lightyear's `InterpolationDelay` includes the client's fixed-step overstep (wave 3's mean 1.79 / max 9.48): the
  client now sends the exact interpolation time with each input (the `Saw` message);
- lightyear 0.30's interpolation delay does not cover RTT/2, so under latency remote players ran past their data and
  froze on whole ticks: the client sets the delay to RTT/2 rounded up plus 1 tick. Remote players are therefore drawn
  at least 1 tick later than before, also on a LAN.

**Clients predict their own hits.** The server replicates each fighter's rules state (`Fighting`; Wave 5: to its owner
only), and the client runs `Match::step_player` for its own slot, so knock-back, deaths, respawns (record included)
and its own grenades are predicted. What the server decides (a hit by someone else, a pickup, a random pick)
mispredicts for the client's lead: 4 ticks on localhost, about 13 at 150 ms each way. `Rules` is removed (the fighter
carries what it held).

**Wiring:** root tracks loaded on both sides; cameras call `Follow::step_view` with the rules' view and hand the aim
gate and scope back to the fighter (`Fighter::take_camera`), so armed aim, the scopes, R's block conditions and the
hint cut run in play; animation rates use `request_at`; rockets and shots are drawn; `Match::remove_player` replaces
the server's `remove_slot`; `lag=` delays packets both ways; `BULLET_RANGE` is 20,000 (bot bullets live up to 3,287
units, V); the mouse moves the stick while R is held; `--bots N`. Protocol 6 (Wave 5: 7). Bugs fixed: the respawn
camera used the start record's camera (netcode agent); the camera cut 13 ticks after a predicted respawn, when the
server's count arrived (browser agent: the client now cuts on its own predicted respawn).

**Through the server** (`combat.sh`, re-run here):

| Encounter | Events | Clock | Shooter's prediction (ticks over 0.001) | Victim's prediction | Lag compensation |
|---|---|---|---|---|---|
| e_w09 | 39/43 (the 4 misses: the fidget's picks, whose RNG can't match the game's) | 742/742 | 1 (0.039, the replay's start) | 16, max 26.5, 8 rollbacks (wave 3: 44, 41) | 122 tests, all on `Saw`, rewound vs shown max 0 |
| e_gren | 9/11 (the held grenade's weapon events, a logging gap, as in wave 3) | 452/452 | 1 (0.039); its own grenade's push is predicted (wave 3: 1.10) | 4, max 132 (the blast flight), 3 rollbacks (wave 3: max 161, 70 rollbacks) | – |
| e_w15 (new) | 17/19 (the burning run's wall bump at 880 and 891, as in the rules replay) | 255/255 | 13, max 10.5, 7 rollbacks | 14, max 55.3, 10 rollbacks (the two shoving each other) | – |
| e_w09, 150 ms both ways on both clients | 39/43 | 742/742 | 1 (0.039) | 78 (6 hits × 13 ticks of lead), max 26.5, 8 rollbacks | 122 tests, all on `Saw`, max 0 |

The netcode agent's own runs: e_w09's victim 23 ticks and 8 rollbacks; e_gren's victim 2 ticks; e_w15 17/19 events,
shooter 13 and victim 16 ticks (the burning victim and the shooter shoving each other). A **mutual-fire duel** at 150
ms each way (`../lab/screens/wave4/duel4_p*.txt`): 207 + 191 bullet tests, all on `Saw`, rewound target vs the
shooter's screen max 0.000000 (wave 3: mean 1.79, max 9.48 with one shooter); with `--no-lagcomp` the same scripts hit
once instead of 6 times. THIRD=1: a third client joins on slot 2, record 13, and leaves cleanly.

**Controls** (also in `web/README.md`; wave 3's table is unchanged except the mouse):

| N64 | Keyboard / mouse | Gamepad |
|---|---|---|
| stick | WASD or arrows (Alt walks); **the mouse while R is held and no key moves** (2 stick units per pixel a frame) | left stick |
| A | Space | South |
| B | B | West |
| Z | left mouse, F, Shift | triggers, left bumper |
| R | right mouse, R | right bumper |
| C-down | G | right stick down |
| C-left / C-right | Q / E | right stick left / right |

With R and a gun the stick twists the upper body (about 49°), then turns the body; with R and 0x24 or 0x14 it moves
the scope's look and Z fires along it; C-left/C-right strafe with the stick held, armed or not; holding C-down cooks
the grenade; R blocks 0x16's and 0x23's fire.

### Goal 3: controls and movement (movement and camera agents; conker-sim, conker-camera)
**Movement rules ported** (V unless tagged; the first four come from the movement agent's first attempt, checked
and kept):
- C-strafe with the stick held, short C taps, armed strafing, the stop rule (25089/25090) and the state-122 reversal
  brake.
- **Players push each other** (the wall routine's layer 3, `func_1505D6F0` / `func_1505D408`): `step_among` with the
  other bodies as `Other`. conker-rules passes the players; a new grenade's push on its thrower stays conker-rules'
  `push_out` (1.10 against the traced 1.09; a grenade as an `Other` body gives about 1.85).
- **Root motion at runtime**: creep, 0x16's Z and the 0x17E death drift (0.00001 on 2 traces), from root tracks the
  extractor now reads (`Clip::root_motion`, `Slice::root_clips`).
- **Thresholds read in the code** (C, replacing most of wave 2's bracketed values): 14/6/18 in `func_1504C0E8`, −10
  at 0x1504E96C, 0x51/7/3 at 0x1504E9C8, 15/30 for the snap-down.
- **R's twist with a body-aimed weapon** (`func_15063A38`; twin guns, pistol, flamethrower, grenade): the stick turns
  the upper body by x·10 a frame up to ±9000; past that the facing and the move angle turn. On release the twist
  relaxes by a quarter a frame and snaps to 0 below 30. V: guns within 0.0005, 0 twist misses (were up to 3.7 off).
- **The scope** (0x1504D4B4): the facing takes `trunc(look_yaw · 182.04445)`, the move angle turns toward it in
  256-steps at a 0.1 drive and the facing back toward the move angle; 0x23 and 0x24 hold the state; through the
  scope the strafe uses C-left/C-right only. The scope model is exact on 771 of 784 scoped rows, 13 one unit off
  (movement agent, `move4_scope_model.py`; not re-run here).
- **The wall routine's zone gate** (obj+0xF8 bit 4, set by `func_150829D8`, cleared inside type-1 trigger zones of
  kind 0): it equals "not inside a camera volume" at the frame's start, so `walls_on` reads `world.camera`. The
  routine is off on 94% of human rows. V below.

**Camera rules ported** (`../lab/oracle/camera4_README.md`; C, then V):
- **Armed aim** per weapon (cam+0x23E latched from s126+0x78): eye distance and lift 90/25 unarmed, 110/25 for the
  flamethrower, 75/5 for the others; per-weapon stick rates and springs; body-aimed weapons aim at
  `facing − twist + 0xC000` and the stick only pitches; the guns' stiffer follow camera (0x09, 0x39).
- **The scope** (`func_15126378`, release `func_15127EB8`), for 11 weapon ids (0x14, 0x16, 0x23, 0x24 in this
  Deathmatch): the eye in the head looking along yaw/pitch, zoom, release rules. While scoped the camera's yaw is the
  eye-to-look-at direction (the stick read a stale yaw on 199 rows per scoped trace before), and the player brakes
  2.5 a frame to a stop (`Follow::aiming()` includes the scope).
- **R's block conditions** (`func_1512317C`): `PlayerView::refuses_aim` (dead, busy, attached, airborne unless weapon
  3, stunned unless body-aimed, kind 1 above speed 15).
- **The level's hint cut**: hints are type-0 records of the camera-volume table; scene 0x2D has one, record 82, a trip
  wire at the plateau's east edge (command 30 → `func_15128774`). Wave 3's reading (hint type 5) was the kind-5 path,
  unused on 0x2D. Parsed from the ROM it fires once in 4,769 frames, at camera_ledge 410, the frame the game cut.
- Not ported: ceilings (no action-2 volume on 0x2D).

**Movement against the original** (`cargo test --release -p conker-sim --test oracle -- --include-ignored`, player 1,
re-run here). "Trace camera" takes the yaw from the trace; "closed loop" runs `Follow`, answered by the sim's own
collision, as server and client do. 0 state misses and 0 facing misses on every row in both modes.

| Trace | Rows counted (wave 3) | Trace camera max | Closed loop max | Closed-loop eye max (wave 3) |
|---|---|---|---|---|
| idle, walk, run, turn | 210, 240, 240, 330 | 0 | 0 | 0 |
| jump | 270 | 0.0008 | 0.0008 | 0.0006 |
| **ledge** | 480 | 0 | 0 | **0** (184.4: the hint cut) |
| camera_circle, camera_swing, anim_idle_long | 479, 540, 2,125 | 0 | 0 | 0 |
| move_jump | 476 | 0.0010 | 0.0010 | 0.0008 |
| move_wall2, move_release, camera_wall, move_wall3, move_slope, move_speeds, move_wall1, move_wall5 | 647, 476, 449, 677, 806, 476, 648, 707 | 0 | 0 | 0 |
| camera_jump | 450 | 0.0010 | 0.0010 | 0.0007 |
| anim_jump_stand | 386 | 0.0009 | 0.0009 | 0.0007 |
| move_crouch | 409 | 0 | 0 | 0 |
| **anim_crouch** | 446 (442, 4 excluded) | 0 | 0 | **0** (0.25) |
| **move_turnrun** | 476 (292, 184 excluded) | 0.0001 | 0.0001 | 0.0017 |
| move_wall6 | 699 (38 excluded: the bot's hit at 730) | 0 | 0 | 0 |
| **anim_creep** | 420 (25, 395 excluded) | 0.0002 | 0.0002 | 0.0021 (0) |
| **camera_buttons** | 719 (532; 1 excluded: read 160, the grenade's push) | 0 | 0 | **0.031** (0.54) |
| camera_aim | 627 | 0 | 0 | 0 |
| wall_jump | 660 | 0.0008 | 0.0008 | 0.0010 |

- Excluded rows left: camera_buttons 160 (the grenade's push on its thrower, a conker-rules rule) and move_wall6 730
  to the end (the bot's sniper hit).
- "Yaw steps off" in the closed loop: camera_aim 51 (the stick override, as in wave 3), 0 elsewhere.

**Combat and armed traces** (`combat_movement`, 28 asserted within 0.1: 19 combat traces and the 9 camera4 probes,
player 1's movement with the trace's camera; the grenade-push rows of combat_buttons 219, ctl_cd_cr 99, ctlw_k4 and
k7 406 excluded):
- Player 1 within 0.0005 on all 19 combat traces (0.0001 or less on 17). Player 2 standing and pushed by player 1
  (e_w14b, e_w14, e_w16, combat_approach) within 0.0008.
- camera4: w21, w22 0; w09, w39 0.0005; w15, w24 0.0002 (0 twist misses, 0 facing misses); w14 0.029 (69 facing
  misses), w16 0.050 (894), w23 0.087 (224; row 530 excluded: 0x23's scope hold lapses, I). The facing misses are the
  scope's one-unit rows; they are reported, not asserted.
- e_w21 and e_w22: 653 and 655 facing misses from read 842: the game's facing turns to the melee lock-on's move
  angle (22688), a conker-rules rule this sim-only replay doesn't run (it keeps 25090); positions within 0.0001.

**Other movement checks** (re-run here): wall routine 4,707 calls, 0 differ; quadtrees 545 + 1 nodes and 1,759
bounds equal; camera rays 2,527 (1,218 hits), sweeps 1,745, mode dispatches 1,685, all 0 differ; state-122 heading
camera_wall 34/34, move_turnrun 20/20, wall_jump 167/169 (342, 352, max 0.0055°); death drift e_w22 rows 852-865
0.00000, e_w24 905-960 0.00001. **Zone gate:** human rows 161,317 (the routine off on 151,690), 0 differ; bot rows
80,625, 6 differ (4 distinct reads: bot_long2 p1 413 and 7782, bot_long3 p3 524 and p2 5998, the last two repeated
in combat_bots3). The movement agent measured 152,344 / 62,693 / 2 before the bot traces it now also reads.

**Camera against the original** (`cargo test -p conker-camera`, re-run here):
- 12 camera traces, oracle-fed, 4,757 rows: eye and look-at bit-exact; stick override, blend and side equal
  cam+0x384/0x5CC/0x390 on 3,581 of 3,581 rows; the volume flag on 3,581 of 3,581.
- 18 armed traces, 16,823 rows: eye and look-at bit-exact on 16; there the yaw cam+0x37C and the override fields are
  bit-exact on all raw rows (asserted). combat_ctlw_k6 and k10 (weapons 0x23, 0x24): exact to read 334, then up to
  0.0815 off (rows 368-402) after a C-right side step; cause open (the yaw spring's velocity), the test allows 0.1.
- ROM-fed without the sim's collision: unchanged from wave 3 (camera_jump 0.108, camera_buttons 1.37, camera_ledge
  175 and camera_wall 144; the closed loop above supplies what this replay lacks).

### Goal 4: bots (bots agent; new conker-bots, conker-server)
Details: `../lab/oracle/bot_README.md`. **How the game does it** (C unless tagged): bots are participants after the
humans. Three layers drive them:
1. the multiplayer AI script `[0x14, 0xBA]` (19 KB of bytecode run by the level-script machine `func_150ADAF0`): roles
   (a weighted pick, 25/15/5), threat ranking, a state per bot (0 replan, 1 role spot, 6 fetch a weapon, 7/8 attack
   and chase, 10 camp, …), one bot replanned per frame;
2. the character class's behaviour scripts `[0x0F, 0]` (walk, chase, wait, node search);
3. the body, steered directly, with a virtual pad for the weapon buttons; R held while aiming, the shot along the aim's
   yaw and pitch (s126+0x16C/+0x170).

**Ported** (`conker-bots`, wasm-clean): the script machine runs the original script as is, from the player's ROM; the
game's C helpers it calls are in `brain` (frame, threats, ranking, sight, noises, attack and fire timing, aim); the
behaviour scripts in `npc`; Dijkstra over the 143-node navigation graph `[0x19, 0x2D]` with its barrier lines in
`nav`; the level's lifts (type-8 trigger gates leading to spawn records 2-5) in `lift`. V:
- the path search: **391 of 391** node changes of combat_bots3 and bot_long3;
- sight: 137 of 142 tested flips as the game's (the 5 misses are lines thousands of units long);
- lifts: the rides of bots3 p1 at 604 → 605 and p3 at 1591 → 1592, on the frame and record; the frame after a ride
  the path is searched again (p1 605 → 606, node 105 → 104);
- drawing through player command 0x61 (B for one frame), as the game's bots do;
- the bot's scope: with 0x14 or 0x24 and R held the game sets its scope flag (3,793/3,793 and 3,161/3,161 rows in
  bot_long3); `Bots::scopes` writes the aim's yaw and the game's pitch into the fighter, so snipers hit across heights.

**Approximations** (I): a bot drives the same `Input` a human sends (its walk target becomes a stick and a camera yaw,
so a human's acceleration and turn caps apply, not the bot class's); bullets and rockets fly along the facing, flat
(conker-rules), where the game's bot fires along its aim with pitch; a bot lets go of R with 0x16 or 0x23 drawn (the
oracle's bots never drew either); the scope's eye 67.5 above the feet; its own RNG; lifts move humans at once (the game
fades first).

**Statistics**, 1 idle human + 3 bots, 5 minutes, against the oracle's bot_long3:

| | Oracle (bot_long3) | Remake now (`BULLET_RANGE` 20,000), seeds 1 / 2 / 3 / 4 / 5, re-run here | Bots agent's run (range 1,500), seeds 1-5 |
|---|---|---|---|
| Kills | 10 | 3 / 10 / 7 / 6 / 8 (mean 6.8) | 2 / 4 / 1 / 6 / 4 (mean 3.4) |
| Deaths (all) | 13 | 4 / 12 / 7 / 6 / 9 | 3 / 5 / 1 / 6 / 5 |
| The idle human killed | 5 | 2 / 3 / 1 / 1 / 3 | 0 / 0 / 0 / 1 / 0 |
| Distance per bot-minute | 11,144 | 10,622 / 13,040 / 8,051 / 8,699 / 13,863 (mean 10,855) | mean 7,349 |
| Weapons picked per bot | 6.0 | 6.3 / 7.7 / 5.0 / 5.7 / 7.0 (mean 6.3) | mean 5.0 |
| Lift rides | 6 | 1 / 2 / 6 / 11 / 6 | 1 / 14 / 0 / 11 / 8 |

`bot_match` is deterministic (the netcode agent's seeds gave the same kills); measured with `bot_stats.py remake`. Camps
remain: in seed 1 one bot spends 88% of the match in AI state 10 (camp; the original: 0%) and another 94% in state 1
(the original: 8.4%). AI-state shares, the 2-bot runs and the oracle's own camps are in `bot_README.md` §5.

**In the server:** `--bots N` plays slots 0..N; humans take the slots after them and a leaving human frees only its
own. Per tick: `Lifts::ride`, `Bots::lifted` per ride, `Bots::tick`, `Bots::scopes`, `Match::step_seen`,
`Bots::observe`. Clients see bots as ordinary interpolated players. `bots.sh` (re-run here, 2 humans + 2 bots, 300 s):
**passed** (exit 0): bot 0 moved (232 distinct logged positions), made 32 hits and 5 kills, died twice (once by its
own explosion) and respawned twice; bot 1 moved (184), made 22 hits and 3 kills, died and respawned twice; both clients
saw both bots move; 5 lift rides; 0 ERROR or WARN lines. That is one run, and the outcome depends on the match: the
bots agent's two final 300 s runs failed (one bot never died), and in 3 of 3 of its runs with one idle human bot 0
overshot node 35 at (−3405, 1, 2409), fell to y = −689 and pushed the wall under node 54 for the rest of
the match (the oracle's bots never went below −600). The netcode agent's three 240 s runs: bot 0 died 3 times in one;
in the other two it rode the lift, picked up the sniper rifle and camped (AI state 6) with 0 kills.

### Browser (browser agent; Chrome 153, macOS, M2 Max; not re-run here; steps in `web/README.md`, "Test wave 4 in two windows")
Seven runs with 2 server bots and `lag=150` on both pages, one page over WebTransport and one over WebSocket, then a
smoke run of the shipped bundle:
- **Bundle** (`web/build.sh web` with wasm-opt): 29,763,364 B before wasm-bindgen, 26,777,291 B after, **21,723,068 B
  / 6,849,029 B gzip** shipped after `wasm-opt -Oz` (wave 3: 21,597,566 / 6,795,030). `client.js` unchanged.
- **Both players firing at once** (twin guns): both hit on the same tick (2082), then 2105 and 2128. All 306 bullet
  tests used the page's own report; the rewound target against what the shooter's page drew: 918 comparisons (the
  other human and both bots), max 0.000000.
- **The victim's page**: exact except for 12-13 ticks after each hit the server decides, then a snap to the server's
  position: 16-26 units standing, 110 strafing, 153 in the burning run, 318 in a bot's knock-back (Wave 5: blended on
  screen, at most 20 units a tick). Respawns predicted exactly (records 8, 16, 10).
- **Weapons**: the flamethrower (2 hits, then the burning run); the rocket launcher (one rocket from 143 units killed
  the target and cost its shooter 2); the sniper rifle (right mouse opened the scope, the mouse moved the look, F
  killed); the scoped gun 0x14 (a kill with right mouse held); grenades (quick throw off 132 ticks after leaving the
  hand, a 2 s cook 76). W+E and W+Q strafe with the stick held; B draws. The R twist with a gun was not exercised.
- **Bots** moved, picked up weapons, rode lifts, killed humans 8 times and each other 3 times, one killed itself; no
  human killed a bot (in the blade run the bots shot from beyond 190 units).
- 0 console errors or warnings on every page (only Bevy's WebGL2 WARN lines), 0 ERROR/WARN in the 8 server logs, no
  foreign input in runs 2-7.
- **Fixed**: the camera after a respawn (above). New option `?log=DIRECTIVES` (`Config.log`, `log_plugin`), since the
  browser has no `RUST_LOG`. `browser_compare.py SERVER_LOG PAGE_LOG...` reproduces these numbers from
  `browser-logs/`.

### Screenshots (`../lab/screens/wave4/`)
Native ones are offscreen renders (`offscreen`: the session's screen was locked, so windows captured black). Browser
names: `-p1-` is slot 2 (the first page to join), `-p2-` slot 3; `wt`/`ws` the transport.

| File | Shows |
|---|---|
| `scope-p1.png`, `scope-p2.png` | native: through the sniper's scope, player 2 centred (the head shot lands at read 863); player 2 dead, `respawn in 5.4 s` |
| `duel4-p1.png`, `duel4-p2.png` | native, both firing the twin guns at 150 ms each way, bullets in flight; player 2's HUD still 6/6 (the hit at 854 hasn't arrived) |
| `flame-p1.png`, `flame-p2.png` | native: the flamethrower drawn, player 2 in its flame reaction; the burning run at HP 5/6 (no flame particles drawn) |
| `bots-p1.png`, `bots-dead-p1.png` | native: an idle human at HP 2/6 after bot 0's second pistol hit; the same human dead later |
| `browser-duel-p1-wt.png`, `browser-duel-p2-ws.png`, `browser-duel-fire-p1-wt.png`, `browser-duel-hit-p2-ws.png` | Chrome, the twin-gun duel with lag: HP 5/6 and 4/6, bullets in flight, the other player ahead with guns up |
| `browser-bot-coming-p2-ws.png`, `browser-bot-kill-p2-ws.png`, `browser-bot-attack-slot3-ws.png` | Chrome: a bot through the far door; dead after a bot's blade (`respawn in 4.1 s`); HP 2/6 after bot hits |
| `browser-flame-p1-ws.png`, `browser-rocket-p2-wt.png` | Chrome: the flamethrower drawn (the victim ran off); the burning run at HP 5/6, rocket launcher drawn |
| `browser-scope-slot2-wt.png`, `browser-scope-slot3-ws.png`, `browser-scoped-gun-slot2-wt.png` | Chrome: through the sniper's scope, target centred; its victim falling (`respawn in 5.5 s`); through 0x14's scope, kills 1 |
| `browser-rocket-dead-p1-wt.png` | Chrome: the respawn camera bug before the fix (the view left at the death spot) |
| `shots.sh`, `duel4_p1.txt`, `duel4_p2.txt`, `scope_p1.txt`, `idle_p2.txt`, `browser_compare.py`, `browser-logs/` | the screenshot script and its scripts; the browser runs' logs and their comparison tool |

The white quad near record 1 is still there (`duel4-p1.png`, top left).

### Per crate (wave 4)
| Crate | Rust lines (wave 3) | Tests (re-run here) |
|---|---|---|
| conker-sim | 5,774 (4,530) | 44 unit (45 with `net`) + 12 oracle |
| conker-camera | 3,064 (2,212) | 30 unit + 7 oracle/ROM |
| conker-rules | 6,321 (4,282) | 51 unit (52 with `net`) + 3 combat + 4 oracle + 2 ROM |
| conker-anim | 1,071 (898) | 9 unit + 1 oracle + 4 ROM |
| conker-bots (new) | 3,889 | 13 unit + 3 oracle + 3 ROM, plus the `bot_match` example |
| conker-server | 1,249 (1,134) | 8 unit, plus `smoke.sh`, `replay.sh`, `combat.sh`, `bots.sh` |
| conker-client | 2,339 (1,881) | 16 unit |
| 6 extractor crates | 7,655 | 86 |

**Public API added in wave 4** (additive, except where marked):
- **conker-sim**:
  - `movement::step_among(&mut p, &Input, &world, &[Other])`, `Other { pos, radius, lift, y_scale }` with
    `Other::player(pos)`, `push_apart(&mut pos, &[Other]) -> bool`: bodies (layer 3).
  - `movement::animate(&mut p, id, rate, start, &world)`, `root_motion(&mut p, &world) -> bool`,
    `walls_on(&p, &world) -> bool` (the zone gate).
  - `PlayerState` gains `root: root::Root`, `twist: i16`, `scope_yaw: Option<f32>`; `TWIST_MAX` (9000),
    `SCOPE_DRIVE` (0.1).
  - `root::{RootTrack { ids, duration, step, z, end } with z_at() and travel(), Root}`; `CollisionWorld.roots`,
    `CollisionWorld::root_track(id)`.
  - `follow`: `Follow::step_view(PlayerView, &Input, &world)` (the armed entry point); `view(&p)` fills `speed`;
    `aiming()` includes the scope; each step applies the level's hint cut.
  - `net`: `PROTOCOL_ID = 6` (Wave 5: 7); `Saw { tick, at, overstep }` on `SawChannel` (unordered, unreliable);
    `MatchView.rockets`, `.shots`. **Removed:** `Rules`.
- **conker-camera**: `aim_reach(w)`, `aims_with_body(w)`, `scoped(w)`, `VIEW_ANGLES`, `FLAMETHROWER`; `Scope` and
  `CameraState.{scope, weapon}`; `PlayerView` fields `facing, weapon, twist, stunned, in_hand, dead, speed, kind,
  busy, attached` (defaults in `PlayerView::new`) and `refuses_aim(weapon)`; `volume::{CameraHint { parse_table,
  contains }, HINT_CUT}`, `CameraVolume::crossed`, `CameraLevel::{hints, hint_cut(old, pos, floor_y)}`.
- **conker-rules**:
  - `game`: `Seen<'a>`, `Match::step_seen` (`step` = no rewind), `Match::step_player(i, arena, &Input, world, seen)`,
    `Match::remove_player(i)`, `Match.{rockets, shots}`.
  - `Fighter`: fields `scope_pitch`, `fidget`, `cook`, `dropped`, `burn`, `heat`, `bump`, `ready`; methods
    `camera_weapon()`, `in_hand(tick)`, `camera_view(tick)`, `take_camera(&Follow)`.
  - `weapon::Attack::{Flame, Rocket, Shot}`; modules `flame`, `fidget`;
    `projectile::{Rocket, Shot (Shot::toward), COOK, cooked_speed, hits_ellipsoid, hits_sphere, hits_parts,
    HEAD_PART, TORSO_PART, CROUCH_DROP}`; `damage::{SHOT_BODY, ROCKET_INNER, ROCKET_OUTER, Facing::Away}`
    (`Facing::Flight` kept, no longer returned).
  - Changed values: `weapon::HEAD` 55 → 66 (the head sphere's centre), `weapon::BULLET_RANGE` 1,500 → 20,000.
  - Feature `net`: serde on `Fighter`, `Action`, `Fidget`, `Burn`, `Grenade`; `net::Fighting { slot, state,
    grenades }` with `new`, `of(&Match, slot)`, `fighter(body)`; `net::register(&mut App)` (after `ProtocolPlugin`).
- **conker-anim**: `request_at(id, speed, target, percent)`; `armed_move`; the armed, fidget and burn ids.
- **conker-bots** (new): `Assets { nav, triggers, ai, class }` with `Assets::paths(scene)`; `Bots::new(assets,
  players, bot_slots, skill, rng)`; per tick `lift::Lifts::ride` → `Bots::lifted(slot)` → `Bots::tick(&Match, &Arena,
  &world, &mut [Input])` → `Bots::scopes(&mut Match)` → `Match::step` → `Bots::observe(&events, &Match)`; queries
  `state(k)`, `aim(slot)`, `npc(k)`, `bot_slots()`, `by_slot`, `slot`; modules `brain`, `script`, `npc`, `nav`
  (`Graph::parse`), `lift` (`Lifts::new(triggers, scene)`, `remove(i)`); `DEFAULT_SKILL` (2).
- **conker-server**: `--bots N`; lag compensation through `Match::step_seen` and each input's `Saw`; bot log lines
  every 30 ticks (AI state, goal, script, node, waypoint).
- **conker-client**: `MatchArena`, `known_match`, `Aim`, `Shows`, `Positions`, `Shown`; `mouse_aim`, `MOUSE_AIM`;
  `Config.{offscreen, log}` (`offscreen`, `log=`), `log_plugin(&Config)`, `render::Offscreen`;
  `FollowCamera.aims: [Aim; 128]`, `FollowCamera.lives` (counts the client's own cuts); `Anim(AnimPlayer, u8)`.
- **extract-character**: `Clip.root_motion: Option<RootMotion { z, end }>`. **conker-extract**:
  `Slice.root_clips: Vec<RootClip { logical, duration, step, z, end }>`.

### Honest gaps
- **Movement not ported:**
  - The class-6 slide: `func_15051558` (749 lines) every frame after the floor, its velocity from `func_1510F820` →
    `func_1510E388`, carried into the air. The only trace is one idle teleport run (surface_slide); the stick's part
    is untraced.
  - 0x23's scope hold lapses for one frame about every 30 (its aim clip restarting, I; camera4_w23 row 530).
  - The twist's untraced conditions (I): the grenade in hand, Z's part of the 0x2010 mask, R while airborne. Bots'
    twist target (`func_150639BC`).
  - The zone gate's other skip conditions (rules states); 6 bot rows differ.
  - The game slows a strafe climbing a slope, the sim doesn't (combat4_lock from row 996).
  - conker-sim never sets speed percent 65 on a strafe's first frame; callers of `request_at` pass it.
- **Movement differences reported, not asserted:** the scope's one-unit facing rows (w14 69, w16 894, w23 224); the
  facing after the melee lock-on in the sim-only replay (e_w21/e_w22 653/655 rows; conker-rules runs it); conker-rules'
  replay of short C taps during a throw (4.96-4.97).
- **Camera:** combat_ctlw_k6/k10's 0.08 residue (cause open). Not ported, all clear in multiplayer: ceilings, the
  flags cam+0x2C & 0x80 and cam+0x84 & 0x1000000, the airborne exceptions obj+0xAD and struct126+0x9C,
  `func_15125490`. Meanings unknown (C only): struct126+0x4E, +0x1AC, obj+0x65. The scope's grounded test compares
  `pos.y` with `floor_y` where the game truncates obj+0x28.
- **Combat:**
  - The burning run's steering: e_w15's 2 misses (the wall bump at 880 and 891), the burning player's path off by up
    to 1,117 and the shooter it pushes by 23 (observations in `combat4_README.md` §1).
  - Inferred fits (I): the head and torso spheres, the knock-back angle of sniper and 0x14 hits (the game's varies by
    up to 21 units for one aim), the rocket's hand position, the two RNG draws after a respawn pick, and the camera
    fields for a grenade pulled with a weapon drawn (never traced).
- **Netcode:**
  - Each event the server decides (a hit by someone else, a pickup, a random pick) mispredicts for the client's lead,
    4 ticks on localhost and about 13 at 150 ms each way, then snaps (16-318 units in the browser runs; Wave 5:
    blended on screen); each random pick costs a rollback.
  - Pushing mispredicts while both players move (the remote player's newest state is a lead behind; e_w15 13 and 16
    ticks; Wave 5: the pushing client 11). Other players' grenades are not predicted (Wave 5: blocked by protocol 7).
  - About 27 `Saw` reports per run fall back to lightyear's `InterpolationDelay` (join, sync, match end; never on a
    bullet tick). Remote players are drawn at least 1 tick later than in wave 3.
  - ~~Every client receives every player's `Fighting`.~~ **Wave 5:** to its owner only.
  - Explosions and fires are not rewound.
  - ~~smoke.sh now shows 1 rollback per client (2-3 ticks) where wave 3 had none; not investigated.~~ **Wave 5:** fixed
    (the server counted the spawn immunity from the join, the client from its first input).
- **Bots:**
  - 3-bot kills below the original's (6.8 against 10); `bots.sh`'s "every bot dies" fails in most runs (one bot
    camps or is stuck for the whole match).
  - Bullets and rockets fly flat along the facing; the game's bots fire along s126+0x16C/+0x170 with pitch. This needs
    an aim field apart from `scope_yaw` (which drives the scope's facing rule) across conker-bots and conker-rules.
  - The pit trap: bot 0 overshoots node 35 (a human's turn caps), falls to y = −689 and can't leave.
  - Untraced: the R release with 0x16/0x23; picking up a weapon already held re-holsters it (`Match::give`), so a bot
    parked on a pickup loses its draw every 240 frames. Not ported: the bots' twist target and their earlier zone
    test.
  - Sniper camping in server runs (two of the netcode agent's three; the browser run's repeated explosions around one
    room, `browser-logs/server-scope2.log.gz`).
- **Rendering:** no flame particles, weapons in hand (drawn since the fixes after wave 4) or rocket models; the twist
  isn't shown on the model; bullets, grenades and rockets are placeholder spheres; the white quad near record 1.
- **Browsers:** Firefox, Safari and background tabs untested; the client reads the gamepad without checking window
  focus (not reproducible with DevTools-driven Chrome; Wave 5: it checks); Python's `http.server` sends no cache
  headers, so a rebuilt bundle needs a reload that bypasses the cache.
- **Duplication left:** the two surface tables (conker-sim `SURFACES`, conker-rules `surface::SINK`); conker-rules'
  `raycast`, `hits_body` and its copy of `rise_along` belong in conker-sim. (`push_bodies` and the server's
  `remove_slot` are gone.)
- **Docs outside this tree are stale:** `../RUST_REMAKE_PLAN.md` says wave 4 is running; `camera4_README.md` says the
  scope's move-angle turn is not ported (the movement agent ported it) and that server and client call `step_blocked`
  (they call `step_view`); `../lab/oracle/README.md` lacks waves 2-4; `combat_README.md` lacks wave 3's corrections
  (wave 4's are in `combat4_README.md`).

### Wave 5, ranked
1. **Bots to the original's numbers:** fire bullets and rockets along the bot's aim yaw and pitch (an aim field next to
   `scope_yaw`); keep bots on their path through tight corners (the bot class's turn rate) to end the pit trap; trace a
   re-pickup and a bot drawing 0x16/0x23 (lldb gives); then re-run `bot_match` seeds 1-5 against bot_long3's 10 kills
   and `bots.sh`.
2. **Fidelity gaps found in wave 4:** the class-6 slide (trace the 4 standable class-6 faces with the stick held
   first); the burning run's steering (e_w15); 0x23's scope-hold lapse; the slope during a strafe; the bots' twist
   (`func_150639BC`); speed percent 65 on a strafe's first frame; the head/torso spheres against the skeleton
   (`func_15081E78` part 0xA); the k6/k10 camera residue (cam+0x7C8 and the strafe springs under lldb).
3. **Netcode robustness** (carried from wave 2): dual-stack `[::]` bind; a disconnect on exit and `pagehide`, then
   back to a 3 s timeout; gate input on asset loading; a status line on connection failure; check window focus before
   reading the gamepad. New: smooth the 12-13-tick hit snap on screen (or extrapolate/predict remote players by the
   lead); predict other players' grenades near the local player; send `Fighting` only to its owner; the smoke test's
   new start-up rollback. **Wave 5: done** (wave 5's section), except other players' grenades (blocked).
4. **HUD and lobby:** the game's own HUD, a scoreboard, the join flow, spectators, the match-end screen (scene 0x1D);
   weapons in hand, flame particles, bullet, grenade and rocket models, the twist on the model.
5. **Audio:** MP3 voices, then the sound banks; combat events give them triggers.
6. **Deployment:** a VPS with real certificates, a non-zero netcode key, `wss://`, brotli for the wasm, a
   hostname-safe connect token; cache headers for the bundle. (Wave 6: prepared and tested locally, `deploy/`.)
7. **Browsers:** Firefox and Safari 26.4+ (`web/README.md`'s wave-4 two-window test), background tabs, gamepads.
8. **Fidelity details:** 3-VI frames, the render-only camera offsets, segment-8 render state, fog and sky from the
   game, the level duplicates and the white quad; consolidate the duplicated tables and world queries into conker-sim;
   update the stale lab notes and `../RUST_REMAKE_PLAN.md`.
9. **Modes:** War: Total War (scenario 9, same scene; its start records and masks are known), then the others.

## Wave 3 (2026-10-08 to 2026-10-09)
Wave 3's summary, as this file's top read after it:

**Wave 3's four goals are met; three pieces are partial.** The game's own wall routine is ported, the movement/camera
loop is closed, Deathmatch combat is traced from the original and ported (new crate `conker-rules`), and the spawn
picker and the collision surface words are decoded. Two players fight a Deathmatch through the Rust server, natively
and in Chrome 153 over WebTransport and WebSocket: weapons, grenades, hits, deaths, respawns, kills and the match clock.
- **Movement:** all 28 movement traces replay the original within 0.001 units with 0 state misses, both with the
  trace's camera and with conker-camera in the loop as server and client run it. Rows that need something not ported
  (C-down, C-strafe with the stick held, creep root motion, the hit at move_wall6 730) are excluded and listed.
- **Camera:** bit-exact against the original's camera on 12 traces / 4,757 rows, now with walls and R's aim camera.
- **Combat:** replayed through conker-rules, 487 of 508 oracle events land on the exact frame over 29 traces. Through
  the server, the gun encounter e_w09 matches 39 of 43 events (the 4 misses are an unported idle fidget).
- **Partial** (Wave 4: all three done, see above):
  - the flamethrower's flames do no damage (its states play);
  - weapons 0x14 and 0x16 never hit anything in the original's traces, so their hits are unknown;
  - lag compensation is exact only when one player's bullets are in flight in a tick.

Nine agents (workflow `wf_54c32185-a13`; stopped once by the account's usage limit and resumed):

| Agent | Wrote | Status |
|---|---|---|
| walls | conker-sim | done |
| camera | conker-camera | done |
| combat trace | `../lab/oracle/combat_*` | **partial**: weapons 0x14 and 0x16 uncharacterised |
| spawns and surfaces | new conker-rules | done |
| closed loop | conker-sim, conker-camera | done |
| combat port | conker-rules, conker-anim | **partial**: the flamethrower does no damage |
| integration | conker-sim `net`, conker-server, conker-client, `web/` | **partial**: lag compensation with two shooters in one tick |
| browser | conker-client, `web/` | done |
| status | this file | done |

Lab outputs (`../lab/oracle/` unless named):
- Notes, the detailed record that this section summarises: `wall_README.md`, `loop_README.md`, `combat_README.md`
  (the porting spec), `spawn_README.md`, `surface_README.md`. The main `README.md` there is not updated.
- New pinned traces: `camera_aim`, `wall_jump`, 44 combat traces with two scripted humans (`combat_*.csv` + `.raw`,
  and 44 event tables `combat_*_events.csv`), 13 spawn captures, 4 surface runs.
- lldb captures: `wall_*_lldb.csv` (9 traces, 4,707 wall-routine calls), `camera_*_camcalls.csv` (3), `loop_*_lldb.csv`
  (7), `spawn_*_lldb.txt`, `surface_*_lldb.txt`, `combat_*_lldb*.txt`. RAM: `raw/combat_ref_dump.bin` (one 8 MB dump of a
  2-human match), `raw/combat_ctl_cdhold_dumps/` (the grenade fuse); the other combat dumps (~1.2 GB) were deleted.
- Tools: `wall_lldb.sh/.py`, `wall_ram_tree.py`, `wall_asm.py`, `wall_asm_range.py`, `camera_run.sh`,
  `camera_lldb.sh/.py`, `loop_lldb.sh/.py`, `spawn_lldb.sh/.py`, `surface_run.sh`, `surface_lldb.py`, and the
  `combat_*` set (run, lldb, enc, ctlw, pilot, table, check, m2c, types, anims, bullets, map, ram, events, fdiff).
- `../lab/research/mp-probe/ram.py`: wave 1's wrong `score?` field is replaced by the kill counter.
- `../lab/screens/wave3/` (below).

### Verification run (status agent, rustc 1.98.1, all re-run here on 2026-10-09)
| Check | Result |
|---|---|
| Extractor tests, 6 crates, ROM + the three DOS variables (absolute) | **85/85 pass**, nothing skipped; 1,272 DOS textures compared, all 1,770,276 clip values equal DOS's |
| The 10 default-target crates without env | 207/207 pass; the ROM and oracle tests return early |
| conker-camera | 21 unit + 5 oracle/ROM tests pass |
| conker-anim | 7 unit + 1 oracle + 3 ROM tests pass |
| conker-rules (new) | 32 unit + 3 combat + 4 spawn/surface oracle + 2 ROM tests pass |
| conker-sim, debug | 36 unit + 8 oracle tests pass, `report_all` ignored |
| conker-sim, `--release --test oracle -- --include-ignored` | **9/9 pass, `ledge` included**; `report_all`'s numbers are in the movement table below |
| target-net: conker-sim `--features net`, conker-client, conker-server | 38 + 8 (1 ignored), 14, 7: all pass |
| `cargo check --target wasm32-unknown-unknown --lib`, 10 library crates (6 extractor + camera, anim, sim, rules) | clean |
| The same for conker-client (target-net; includes conker-sim `net`) | clean |
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets`: the 10 default-target crates; conker-client, conker-server and conker-sim `net` in target-net | 0 warnings. Only the macOS linker note `__eh_frame section too large` on the debug binaries, as in wave 2 |
| `smoke.sh wt ws ws` (3 clients, the third mid-match) | **16/16 ok**; prediction vs server 696 / 697 / 280 ticks, max error 0, 0 state misses, 0 rollbacks |
| `combat.sh combat_e_w09 09` | 39 of 43 oracle events on the exact row, clock 742/742 (details under "Integration") |
| `combat.sh combat_e_gren -` | 9 of 11 events, clock 452/452 |
| `combat.sh` lag-compensation duel, 150 ms on both clients, with and without `--no-lagcomp` | reproduced (under "Integration") |
| One native windowed client after its HUD became native-only text | renders HP, weapon and rounds, grenades, kills and the clock (screenshot kept outside the tree) |
| Rust source | 22,507 lines in 12 crates, tests included (wave 2: 13,081 in 11) |
| `web/dist` | not rebuilt here; re-measured: `client_bg.wasm` 21,597,566 B, 6,795,030 B with gzip -9 |

Every process started for these checks was stopped.

### Goal 1: the game's wall routine (walls agent; conker-sim)
**Ported** (C, then V bit for bit): `func_15044380` → the hand-written MIPS from 0x150AB1F0 with helpers 0x150492CC,
0x150AC1C4, 0x150AAA20, 0x150ABD20 and 0x150AC344, as `CollisionWorld::sweep` (`collision/walls.rs`).
- Layers on scene 0x2D:
  - 0: the terrain (1,751 triangles).
  - 1: the secondary segment (8).
  - 2: placements 10 and 11 only, the plateau sides (I: the other placements have no collision vertices in RAM).
  - 3: actor meshes, none in multiplayer.
  - Pass 2 (ledge grabs: surfaces with all of 0x0E000000) is empty on 0x2D (V).
- The static layers use the game's quadtree: root half 0x8000, a node with 10+ triangles splits, down to half 256.
- The player's body:
  - A box 2·30 wide and `H = trunc(90 + |Δy|)` tall, centred `H >> 1` above the feet, radius 30, step capped at 60.
  - It is pushed only from the previous position's side. A plane push applies when the point projects inside the
    triangle, else an edge push.
  - `n.y/|n| < 0.5` is a wall unless surface bit 24 is set (never on 0x2D). A ceiling hit while rising zeroes y_vel.
- Around it (`func_1504CB98`):
  - Grounded frames fall a step first (`y_vel = −9`, `y −= 4.5`).
  - After a push the floor is re-queried in full at the pushed position.
- **State 122 and the camera's heading:**
  - The push's angle comes from the game's table arctangent `func_1505A630`, whose table is read from the ROM's
    `.game` data at 0x33A0. An exact arctangent is 2-3 units off.
  - Within [0x5E01, 0xA200) of the move angle: speed > 10 caps at 14; speed > 2 gives state 122 and
    `obj+0x40 = (angle − 0x4000)·360/65536`, the heading the camera follows.
  - Otherwise, and when airborne at the frame's start (obj+0x81) or obj+0x83 is set, the heading is
    `(facing + 0x4000)·360/65536`.
- Against a wall (V, new trace `wall_jump`):
  - A gives a running jump, and landing against the wall goes straight back to 122.
  - Z doesn't crouch: the move angle snaps to the facing and the push goes on.

**Verified:**
- `wall_routine_matches_the_game_calls`: 4,707 captured calls (9 traces, 2,500 with a push), 0 differ bit for bit in
  position, layers that pushed and y_vel.
- `wall_trees_match_the_game`: 545 + 1 quadtree nodes and 1,759 static triangle bounds equal the game's RAM.
- `heading_matches_the_game`, state-122 rows: camera_wall 34/34 and move_turnrun 20/20 exact; wall_jump 167/169 (rows
  342 and 352, max 0.0055°).
- Every wall trace replays exactly (movement table below): ledge 204.5 → 0, move_wall5 1396 → 0, move_wall1 2.73 → 0.
  Wave 2's floor error while pushing (0.18) is gone.

**Not ported** (Wave 4: both are ported, goal 3 of wave 4):
- The level-script zone gate (I as an effect on play). The game skips the routine while obj+0xF8 bit 4 is clear,
  which `func_150A11C4` does in some zones, so the first call comes at reads 133-254 depending on the path. The port
  always runs it; no trace touches a wall inside such a zone.
- Layer 3, so players don't push each other. In the game two overlapping players are pushed apart (combat traces
  e_w14b from 820, e_w21 from 1032, up to 25 units).

### Goal 2: the closed movement/camera loop (camera agent, closed-loop agent; conker-camera, conker-sim)
**The stick reads the camera at full precision** (C, then V on 2,154 of 2,154 steering rows). This corrects wave 2's
rule below.
- The player update (`func_1504A730`) stores `D_800CC280 = trunc(cam+0x37C · 182.04445)`, or
  `trunc(cam+0x384 · 182.04445)` while the stick override cam+0x384 is not 999.
- The target is that plus the stick's own angle (`func_1505A630`: 0 right, 0x4000 up), all 16 bits.
- The move angle turns by whole 256-steps of the difference, `((target − move) as i16 >> 8) << 8`, and keeps its own
  low byte.
- Wave 2 rounded the camera down to a 256-step instead. That rounding, not a later camera read, caused wave 2's
  near-boundary misses and the walls agent's "flips". None remain.

**Other camera rules ported** (V on the camera traces unless tagged):
- **Stick override** (`func_1512C068`/`func_1512C150`): when the eye comes within 42 of the look-at, cam+0x384 =
  yaw ∓ 90 (the side from cam+0x390 = heading − yaw − 180). It eases back at +0.006 per call (cam+0x5CC, 2 calls per
  tick) and switches off at 1. Equal to the dumped fields on 3,581 of 3,581 rows.
- **Camera volumes**, the cam+0x84 bit 0x40000 that gates the wall ray:
  - The bit is set when last tick's eye is inside the level's camera volumes: 0x34-byte trigger records in asset
    `[0x0C, scene, 5]`.
  - Shapes: 2 is an oriented box (38 of 0x2D's 49), 0 a cylinder with an octagonal distance (11; C only, no traced
    eye entered one).
  - The last containing volume decides: action 0 sets the bit, 6 clears it, 2 sets a ceiling (none on 0x2D).
  - Their sines come from the level's own `func_150AD78C`.
  - Answered from the ROM's 49 volumes, the bit equals the dump on 3,581 of 3,581 rows.
- **Wall ray** (`func_150AC9C0` with the triangle test at 0x150AD364, hand-written MIPS): ported as
  `CollisionWorld::ray`; 2,527 captured rays (1,218 hits), 0 differ. **Sphere sweep**: the wall routine with the
  camera's body; 1,745 captured sweeps, 0 differ. Mode dispatch flags: 1,685, 0 differ.
- **R's aim camera** (mode 0x100, unarmed):
  - Phase 1 zooms in for 60 VIs whatever R does: distance 110 then 90, eye at look-at height, the yaw springing
    toward heading − 180 once closer than 132.
  - Phase 2, while R is held: the stick turns the yaw 0.025° per unit per VI and the pitch within [−45, 80];
    distance cos(pitch)·90, height sin(pitch)·90.
  - Release: phase 3 (targets 200/100), then 247/100 and the cam+0x84 flags saved at the press are restored.
  - The view offset cam+0x18C springs to 25 while aiming.
- **Aim gate** (V camera_aim, camera_buttons): while the camera is in aim phase 1, 2 or 3 the player's stick reads
  centred, so a run brakes into the usual skid and the facing holds. The player moves again on the tick the camera
  spends in phase 0 before it pops. R is refused while airborne and at health 0 (C).

**Movement rules found** (they extend "Rules found in wave 2" below):
- **Facing** (`func_150599C8`, C, then V by watchpoint):
  - It turns at most 3072 per frame, and a clockwise turn stops one short, also for exactly −3072.
  - It is written only while obj+0x80 is set. The flag clears once the facing reaches the move angle and is set
    again at the end of ground frames and on landing, so in a jump the facing stops for good once it has caught up.
  - In the air it aims at the stick's own target while the stick is on and the player is at least 50 above the floor
    (obj+0x28); otherwise at the move angle (V on 126 calls).
  - Skids keep turning it.
- **Crouch** (V): Z from a run snaps the move angle to the facing after that frame's step; while crouch-walking the
  move angle is the facing.
- **C-left/C-right strafe with the stick centred** (V camera_buttons 173-262):
  - The target speed reads 34 and the speed aims at 34 × 0.65 = 22.0999985.
  - The move angle turns toward facing ± 0x4000 with the usual turn cap, and the facing holds.
  - Letting go snaps the move angle back to the facing.
- **Surface speed** (C, V wall_jump 470-476):
  - Within 10 of last frame's floor, obj+0xAA takes the kind's percentage (90 on kind 0xA).
  - The speed then aims at target × 0.9 while obj+0x44 keeps the target.
  - It is cleared at the end of a frame with no countdown running.
  - The sink (goal 4) lowers the camera's look-at.

**Movement against the original** (`cargo test --release -p conker-sim --test oracle -- --include-ignored`, player 1,
tolerance 0.1, asserted by `trace_camera` and `camera_loop`, re-run here):
- "Trace camera": the yaw comes from the trace.
- "Closed loop": `Follow::tick`, the camera answered by the sim's own collision (floor, wall ray, sweep, camera
  volumes), as server and client run it.
- Wave 2 counted every row; wave 3 excludes the rows listed under the table. Compare those traces with care.
- Wave 4: re-measured in wave 4's goal 3; only camera_buttons 160 and move_wall6 730 on are still excluded.

| Trace | Rows counted (excluded) | Trace camera max | Closed loop max | Closed-loop eye max | Wave 2: trace camera / closed loop |
|---|---|---|---|---|---|
| **idle, walk, run, turn** | 210, 240, 240, 330 | 0 | 0 | 0 | 0 / 0 |
| **jump** | 270 | 0.0008 | 0.0008 | 0.0006 | 0.0008 / 0.0008 |
| **ledge** | 480 | **0** | **0** | 184.4 (the level's camera cut at 410) | 204.5 / 205.6 |
| camera_circle, camera_swing, anim_idle_long | 479, 540, 2,125 | 0 | 0 | 0 | 0 / 0 |
| move_jump | 476 | 0.0010 | 0.0010 | 0.0008 | 0.016 / 0.016 |
| move_wall2 | 647 | 0 | 0 | 0 | 0 / 3225 |
| camera_jump | 450 | 0.0010 | 0.0010 | 0.0007 | 0.001 / 58.8 |
| anim_jump_stand | 386 | 0.0009 | 0.0009 | 0.0007 | 0.155 / 20.3 |
| move_release | 476 | 0 | 0 | 0 | 0.160 / 31.7 |
| camera_wall | 449 | 0 | 0 | 0 | 0.177 / 26.2 |
| move_wall3 | 677 | 0 | 0 | 0 | 0.248 / 13.7 |
| move_slope | 806 | 0 | 0 | 0 | 0.255 / 41.2 |
| move_speeds | 476 | 0 | 0 | 0 | 0.368 / 42.3 |
| move_crouch | 409 | 0.0007 | 0.0007 | 0.0007 | 0.581 / 57.7 |
| anim_crouch | 442 (4) | 0.0002 | 0.0002 | 0.25 (after the creep resync) | 0.612 / 16.6 |
| move_turnrun | 292 (184) | 0.0001 | 0.0001 | 0.0017 | 0.709 / 42.6 |
| move_wall1 | 648 | 0 | 0 | 0 | 2.73 / 3.32 |
| move_wall6 | 699 (38) | 0 | 0 | 0 | 20.7 / 323 |
| anim_creep | 25 (395) | 0 | 0 | 0 | 116.5 / 116.5 |
| move_wall5 | 707 | 0 | 0 | 0 | 1396 / 1396 |
| camera_buttons | 532 (188) | 0 | 0 | 0.54 (after the C-down resync) | 1397 / 447 |
| camera_aim (new) | 627 | 0 | 0 | 0 | – |
| wall_jump (new) | 660 | 0.0008 | 0.0008 | 0.0010 | – |

- 0 state misses and 0 facing misses on every counted row in both modes. The 0.0006-0.0010 values are the jump arc.
- Excluded rows (the traced state is copied back after each range):
  - camera_buttons 113-172: C-down, the grenade throw, which also moves the player 1.09 at read 160.
  - camera_buttons 623 to the end: C-left with the stick held, then Z out of that strafe.
  - move_wall6 730 to the end: the bot's sniper hit (goal 3).
  - Creep root motion: anim_creep 56 on, anim_crouch 296-299, move_turnrun 323 on.
- "Yaw steps off" in the closed loop: camera_aim 51, all of them the stick override, which the trace's eye → look-at
  angle can't show (eye and look-at are exact there); camera_buttons 4, after the C-down resync; 0 elsewhere.
- ledge's closed-loop eye: the level's camera hint cut at read 410 is not extracted (`hint_cut` stays false), so the
  eye differs by up to 184 afterwards. The player's positions stay exact. (Wave 4: the hint is read from the ROM and
  the eye is exact.)

**Camera against the original** (`cargo test -p conker-camera`, re-run here):
- Oracle-fed (heading, head and floor from the raw dumps, wall queries answered from the captures): eye and look-at
  **bit-exact on all 12 traces, 4,757 rows**. Wave 2 had 8 traces and 2,575 rows, and camera_buttons was 159 off
  during R. Captured query inputs match to 1.5e-8. Two rays of the ledge cut's tick stay unanswered; both hit beyond
  the target.
- ROM-fed, conker-camera on its own (extract-scene's exact-plane floor, the heading computed from the facing, no wall
  ray or mover):
  - 7 traces and camera_aim are exact.
  - camera_jump 0.108 and camera_buttons 1.37: the exact-plane floor under the eye.
  - camera_ledge 175 and camera_wall 144: this replay has no wall heading, ray or sweep. conker-sim's closed loop
    supplies all three, and there both traces are exact (table above).

### Goal 3: Deathmatch combat (combat-trace, combat-port and integration agents)
**Traced** (`../lab/oracle/combat_README.md`, V unless tagged):
- Setup: two scripted humans, 0 bots.
  - Both run to the middle. Player 1 stops 146.5 units from player 2, facing it, at read 744.
  - An encounter follows from read 760. A weapon is given at read 745 by writing MP+0xEB0[p] = k
    (MP = `*(0x800D23B0)`), which is what touching pickup k does.
- Loadout at spawn and every respawn: health 6, no weapon, 3 grenades, 150 frames of invulnerability.
- Controls:
  - Unarmed: B, R and L do nothing, Z crouches, A jumps, C-left/C-right strafe.
  - C-down throws a grenade: pull 28 frames, hold while held, throw 28.
  - Armed: B draws or holsters, Z fires or swings, R aims (the sniper only). A reloads the guns and jumps with 0x14,
    0x21 and 0x22.
- Pickups (`func_1508DC24`, `func_1511DF6C`):
  - 19 on 0x2D, all the level's own placements.
  - Touched within 100 units horizontally plus a height test, before the player moves that frame.
  - One weapon at a time plus grenades (+3, at most 6). A taken pickup is back after 120 frames.
- Weapons:

  | Id | Weapon (I names) | Delivery | Damage per hit |
  |---|---|---|---|
  | 0x09 | twin guns | bullets at 300 per frame, clip 60 | 1 |
  | 0x39 | pistol (Wave 5: a rifle, by its model) | one bullet per press, clip 30 | 2 |
  | 0x23 | one-shot gun (Wave 5: scoped revolver) | bullet | kills |
  | 0x24 | sniper rifle | instant ray | kills (type 0x20), or 1 (type 0x12: the bot's shot at move_wall6 730) |
  | 0x15 | flamethrower | flame particles, each an area hit | 1 (Wave 4: ported, type 0x1C, plus the burn) |
  | 0x21 | melee (Wave 5: katana) | sphere | 1 |
  | 0x22 | blade (Wave 5: chainsaw) | sphere | kills |
  | 0x14, 0x16 | ? (Wave 4: an aimed shot through the scope; a rocket launcher. Wave 5: 0x14's model is throwing knives) | no hit in any trace (Wave 4: traced, wave 4's goal 1) | – |
  | grenade (C-down) | physics object; hits the thrower too | kills within 360, 2 within 500 |

  Ammunition is unlimited: the guns reload when the clip empties, or on A.
- Damage (`func_1505D024` → `func_1505B9C4`; area hits `func_1505C7D8`):
  - A 0x28-byte damage-type table at 0x8009A9F8 (plus a per-character table for melee) gives the damage, the
    knock-back speed, y velocity and gravity, k, the immunity and the reaction.
  - Knock-back each frame: `s' = s − 0.1·k`; move 0.5·s' when s' ≥ 1; `s ← 0.6·s'` (V on 77 frames, max 5e-7). This
    generalises move_wall6's hit at read 730 (k = 1); the gun hits have k = 30.
- Death and respawn: the lethal reaction plays without control. The respawn comes 88-180 frames after the hit,
  depending on the weapon, at the record the spawn picker chooses (goal 4), without jitter.
- Score: kills, an s16 at 0x800E0B36 + 2·i (`func_151E5FF4`). Suicides give no kill. Wave 1's guessed field counts
  only in team modes.
- Timer 0x8008FD78: 36,000 at the start, −2 per frame. At 0 the match ends 39 frames later, fades for 16 frames and
  loads scene 0x1D. There is no kill limit.
- Corrections by the combat-port agent (V unless tagged; not yet in `combat_README.md`):
  - Weapon 0x16 is a normal draw/idle/holster weapon (0x76, 0xBD, 0x78). Its Z (0x77, 28 frames) moves the player
    back 46.5 by root motion (Wave 4: 91.6 over the clip, V k7 within 0.009).
  - Explosion victims move from the row after the hit. The clear-line test runs from the blast + 90 to the victim
    + 45 (C).
  - Blown-up players bounce: 28 on the first landing, then 0.5·|landing y_vel|, speed ×0.6.
  - The grenade's throw speed depends on the hold: 41 for one frame, 83.5 after 60, 90 after 73. A new grenade pushes
    its thrower out to 32.74. (Wave 4: speed `40 + (240 − cook)/4` from the cook timer, which is also the fuse.)
  - The sniper's aim ends 8 frames after R is let go.
  - One frame into a 0x21/0x22 swing the move angle snaps onto the victim (lock-on; the 200-unit range is I).
    (Wave 4: on the Z frame itself; the range is bracketed between 165.9 and 201.1, the code's 200.)
  - Bullets live (distance to the wall)/300 frames (142 births, max error 1.4e-6).

**Ported** (`conker-rules::{game, weapon, damage, projectile}`). `Match` is plain data (Clone), so a rollback is a
clone. Replay test `tests/combat.rs`, re-run here: 29 traces, 17,267 frames × 2 players, conker-sim's movement plus
`Match::step`:
- **Events: 487 of 508 on the exact frame and value; 25 of the 29 traces match all of theirs.** (Wave 4: 506 of 508;
  only e_w15's 2 remain.) The 21 misses:
  - 12 in e_w15: the flamethrower is not ported.
  - 4 in e_w09: the twin guns' idle fidget 0x144 (it looks random; not ported).
  - 4 where the game picked swing 0x129 over 0x12A (e_w21, ctlw_k1).
  - 1: weapon 0x21's landing one frame early (ctlw_k1).
- Health, phase and timer are equal on every row. The timer differs on one row in `end`, the oracle's lldb poke.
  The kill counter: all 6 lldb writes match on frame and value.
- Bullets: 519 of 523 logged rows within 2 units. Max 0.32 for the twin guns and 1.87 for the pistol: the game's
  muzzle follows the animated hand, the port's is a fixed offset. 381 bullet steps exact
  (`bullets_move_as_the_game_moves_them`).
- Grenades: 140 frames, max 0.0015 on e_gren and 0.32 on ammo_cook's cooked throw.
- Positions are within 0.04 on every row where nothing unported moves a player: e_gren (the blast flight and 5
  bounces), e_w09/w39/w23 (knock-back), e_w14, e_w16, ctlw_k0/k1/k3/k4/k5/k8, ammo_cook and the C-down runs except
  ctl_cd_cr. The larger errors (Wave 4 values in brackets):
  - Root motion: ctlw_k7 93 (0x16's Z), e_w22 27 and e_w24 34 (the 0x17E death spin). [0.009, 0.04, 0.04]
  - Players pushing each other: e_w14b 26, e_w21 3. [0.04, 0.002]
  - The flamethrower: e_w15 613. [1,117.5: the burning run's steering]
  - Reversing the stick in state 122 brakes 4.5 per frame in the game: pick87 24.5. [0]
  - Short C-left/C-right taps: 4.3-5.0 at combat_buttons 204, ctl_cd_cr 84, ctlw_k6 and k10 271. The cause is not
    established (I: the strafe rule was fitted to a hold with the stick centred, on camera_buttons). [k6, k10: 0;
    combat_buttons 4.96, ctl_cd_cr 4.97]
  - pick57: 0.21 from row 70 (a scripted run to the pickup), unexplained. [0.0005]
- Respawns: all 16 in the combat traces (6 encounters, 10 in a bot match) are consistent with `spawn::respawn`; 13 do
  not depend on the RNG state.

**conker-anim** (combat-port agent):
- Ids: the `combat` module (draw, idle, holster, fire and reload per weapon, the hit and death sequences),
  `MELEE_LAND` 0x19D, and weapon 0x16's `W16_DRAW` 0x76, `W16_IDLE` 0xBD, `W16_HOLSTER` 0x78, `W16_FIRE` 0x77.
- `request()` rates for the combat ids:
  - `MELEE_WALK` 0x124: (0.4·speed + 0.66·target)/27 (V at targets 19.2 and 35, 69 rows).
  - `BLADE_WALK` 0x15B: (0.4·speed + 0.98·target)/16 (V at target 19.2; the intercept is I).
  - The twin guns' move 0x12B plays at rate 1 (V, 1,708 rows).

### Goal 4: spawns and surfaces (spawns agent; conker-rules)
**RNG and spawn picker** (`../lab/oracle/spawn_README.md`; C, then V on 13 captures):
- `func_150ADA68`, named as the picker in earlier notes, is the RNG's `random_float`. The picker is `func_15084D70`.
- The RNG is a 64-bit word at 0x800885B0 with 33 live bits: `random_u32` 0x150ADA20, and `random_float` = low 16
  bits × f32 0x37800080. Hundreds of call sites draw every frame and the state at match start depends on boot, so
  only the algorithm and each rule's draw count can be ported.
- **Placement:** one draw, `r = 80f − 40`, `x += sin(yaw·π/128)·r`, `z += cos(…)·r`, truncated. At match start: one
  draw per human from the scene load (record 0), then one per participant in slot order from the level script.
- **Start records:**
  - Deathmatch by slot: [1, 0, 13, 6, 14, 7, 15, 8, 16, 9, 17, 10, 18, 11, 19, 12] (V for slots 0-15).
  - Total War (scenario 9) by team: team 0 on 0, 6, 7, … (south), team 1 on 1, 13, 14, … (north). V for the first two
    of each team.
- **Respawn:**
  - Candidates are the records in the team's mask, minus the player's previous respawn record (which takes no draw).
  - Score = the squared XZ distance to the nearest enemy (capped at 1e8) + (500·f)². The first highest wins, with no
    jitter.
  - Deathmatch's masks stay all ones. Total War's level script sets 0xFFF01FFD / 0xFFFFE03E before the first respawn
    (when exactly is I).
- **Record bytes** (C): +6 → obj+0xB8 in degrees (consumer untraced); +0x14 is never read; +0x15 → struct126+0x23
  (meaning untraced); +0x16 is a redirect the respawn path never follows.
- `func_15172E7C` moves players onto records 2-5 mid-match with the placement path (trigger untraced; I: lifts or
  teleports).
- **Verified:**
  - 131 picks (99 placements + 32 respawns) and 714 RNG draws: record, position, facing, camera, record bytes and the
    RNG state afterwards, 0 mismatches.
  - 61 start records and 32 respawn masks as tabled.
  - 80 trace rows on their placements (read 23 for every participant, read 30 for idle humans).
  - The 16 combat respawns above.

**Surface words** (`surface_README.md`; C, then V on 4 teleport runs):
- **Kind** = bits 0-4.
  - It indexes the per-kind table at 0x80099140 (sink depth, depth per speed, speed percent) and picks projectile
    impact effects. Kinds 0xE and 0x13 call `func_1507CD64(obj, 0xD)`.
  - Scene 0x2D has kinds 0, 1, 3, 4, 5, 9 and 0xA. Only 0xA has a table entry (−6, 0, 90).
  - On 0xA the sink (obj+0xCC) steps to −6 by (target − sink)/12 ± 1 per frame. The camera's look-at settles 61.5
    above the feet instead of 67.5; the player's y doesn't change. The speed aims at 0.9 × the target (goal 2).
  - 0x9 behaves like 0x5 when standing and running (V for those states only).
- **Slide class** = bits 28-30 (7 reads as 0).
  - It is the 0x60000005 word on 12 faces, the steep sides of the two central structures (normal.y 0.35-0.79, 8 of
    them under 0.5).
  - An idle player there slides downhill: state 1, speed 0, the facing turning 4096 per frame toward downhill,
    gravity 5, the horizontal step growing about 3.0 per frame. It then falls (states 56, 207) and lands hard (9).
  - **Not ported:** the slide. The sim's wall routine applies the game's own test (`n.y/|n| < 0.5`, so the 8
    steepest faces are walls to it, as to the game's), but what the sim does when a player stands on these faces is
    untested: no movement trace does.
- Bits 12-15 are tested against 3 and 4; they are 0 on every 0x2D word. Across the ROM, kinds stop at 0x13 and
  class 7 never occurs.
- **Verified:** the sink target and value on 1,264 frames, 0 mismatches. The table, its 0.1, the RNG scale, the 1e8
  cap and π equal the ROM's `.game` data.
- **Wired:**
  - conker-sim's movement carries the sink and the speed percent, and the camera adds the sink to the look-at.
  - The server places players with `Match::deathmatch` (start records and jitter), and `Match::step` respawns them
    with the picker.

### Integration and browser (integration and browser agents)
**Server** (conker-server):
- Runs conker-rules' `Match` at 30 Hz on the layered world (`scene_world`: the game's wall layers, the ROM's
  arctangent table and camera volumes). Each player has a `Follow` camera for R's aim gate; health 0 blocks R.
  (Wave 4: `Follow::step_view` with the fighter's view, so armed aim and the scopes run.)
- Players join on the Deathmatch start records by slot (record 1, 0, 13, …), mid-match too. A leaving player's slot
  is removed (`remove_slot`) with the bullet, grenade and pickup indices kept valid. (Wave 4: `Match::remove_player`.)
- **Lag compensation** (Wave 4: replaced by conker-rules' `Match::step_seen`, per bullet, from each input's `Saw`):
  - It keeps every slot's position for the last 60 ticks, and each shooter's view tick from lightyear's
    `InterpolationDelay` (switched on).
  - Bullets are tested against the rewound victim (`compensate`) when one player's bullets are in flight in a tick.
    With two shooters in one tick it falls back to unrewound tests.
- Replicated: `Player` and `Rules` (predicted), `Combat` (to everyone), `MatchView` (one entity). Protocol 4.
  (Wave 4: `Rules` removed, `Fighting` and the client's `Saw` added; protocol 6. Wave 5: `Fighting` to its owner
  only; protocol 7.)
- CLI additions:
  - `--spawn X,Y,Z[,FACING]` (repeatable, one per slot), `--give SLOT:READ:ID`.
  - `--wait N` (start once N players joined), `--margin TICKS`, `--first-read R` (the oracle read the start stands
    for).
  - `--no-lagcomp`.
  - Debug lines `COMBAT`, `EVENT`, `LAGCOMP` per tick.

**Client** (conker-client and `web/`):
- Predicts its own movement and camera (`movement::step` with `Rules`, then `Follow::step_blocked`; `aiming` is kept
  per tick for rollbacks). Health, weapon, hits, deaths and the match come from the server. (Wave 4: it runs
  `Match::step_player` for its own slot and `Follow::step_view`, so its own hits, deaths and respawns are predicted.) Remote players are
  interpolated, and every player is animated from the server's `Combat.anim` (`Clips` covers all 855 animation ids).
- Pickups are the level GLB's own placement nodes, hidden while taken; no extractor change was needed. Bullets and
  grenades are placeholder spheres. No weapon is drawn in hand.
- HUD, e.g. `HP 6/6   twin guns (drawn) 42 rounds   grenades 3   kills 1   9:39`, or `DEAD - respawn in 3.3 s`.
  - Natively it is Bevy text.
  - In the browser it is the page's `<pre id="hud">`: a page without that element shows no HUD, and no error.
  - It is logged only when its text changes (about once a second, not 30 times).
- Options: `lag=` (ms added to packets from the server; one direction only; Wave 4: both directions) and `at=` (a
  screenshot at a script read). Wave 4 adds `offscreen` and `log=`.
- **Loading fix:** windowed debug clients crashed while loading. With clips requested per animation, the GLB load
  nested inside itself on one IO thread and overflowed its stack. The character now loads after the level, with its
  clips taken from the loaded glTF: level 0.76 s, character 2.63 s (wave 2 stalled about 8 s).
- Controls (also in `web/README.md`):

  | N64 | Keyboard / mouse | Gamepad | Deathmatch |
  |---|---|---|---|
  | stick | WASD or arrows (Alt walks) | left stick | move |
  | A | Space | South | jump; reload |
  | B | B | West | draw / put away |
  | Z | left mouse, F, Shift | either trigger, left bumper | fire; crouch when unarmed |
  | R | right mouse, R | right bumper | aim |
  | C-down | G | right stick down | grenade |
  | C-left / C-right | Q / E | right stick left / right | strafe |

  Ctrl is not used (browsers keep Ctrl+W). C still toggles the debug camera. (Wave 4: the mouse moves the stick while
  R is held.)

**Through the server** (`combat.sh`, re-run here): the server waits for 2 players, places both where the original's
stood at read 745 and gives the weapon at 745; two headless ws clients run the encounter's two scripts.
- **e_w09** (twin guns): 39 of 43 oracle events on the exact row.
  - All 6 hits: rows 825, 855, 875, 895, 915, and the kill at 945 with the kill counter.
  - Every animation change, the weapon at 746, and the respawn on record 6 at 1096.
  - The 4 misses are the idle fidget 0x144. One event is only on the server (1097, animation 0 → 0xF); it is in the
    raw trace, and the oracle's event table omits it.
  - Clock: 742 of 742 rows equal.
- **e_gren:** 9 of 11. The 2-damage blast lands at 924 as in the original. The 2 misses are the held grenade (weapon
  0x26 events), which the server's log doesn't report as a weapon. Clock 452/452.
- **Prediction against the server:**
  - The shooter is exact except 1 tick of 0.039 (the replay's start reset) in e_w09, and up to 1.10 for a few ticks
    in e_gren (its own grenade's push).
  - The victim mispredicts every event the server decides, because a player who is hit or dead predicts no movement.
    In e_w09: 44 of 1,112 ticks over 0.001, max 26.5 during knock-back, 5 ticks at the respawn teleport (6,794
    units); 41 rollbacks. In e_gren: max 161 (the blast flight), 70 rollbacks. (Wave 4: 16 ticks / 8 rollbacks and
    4 ticks / 3 rollbacks, wave 4's goal 2.)
- **Lag compensation** (the duel scripts in `../lab/screens/wave3/`, 150 ms added on both clients, player 2 strafing
  across player 1's line of fire):
  - With the rewind, player 2 is hit at rows 832, 875 and 923.
  - Without it (`--no-lagcomp`): hits at 822, 842, 862, 883 and 903, and the kill at 923.
  - 12 of 278 bullets got a different outcome than without the rewind.
  - The rewound victim against where player 1's client drew it: mean 1.79, max 9.48 units in this run. The
    integration agent's runs gave means of 0.2-1.7 and a max under 7.4. (Wave 4: max 0.000000.)
- A third client joining 4 s into a match got slot 2 on record 13 and saw the clock; its slot was removed cleanly
  when it left (integration agent; not re-run). The smoke test's mid-match join was re-run (16/16).
- `replay.sh` walk and run through the server: still exact over 247 rows each (integration agent; not re-run).

**Browser** (browser agent; Chrome 153, macOS, M2 Max; not re-run here; steps in `web/README.md`, "Test combat in two
windows"):
- Page 1 over WebTransport, page 2 over WebSocket, against the native server with e_w09's spawns and the twin guns
  given to player 1.
  - Player 1 drew (B) and held F. The server logged 6 hits 20 ticks apart (t = 1145-1245, the last lethal), then
    player 2's respawn on record 6 at t = 1396.
  - Page 1's HUD: `kills 1`, 26 rounds, bullets in flight. Page 2's: `HP 0/6 … DEAD - respawn in 0.3 s` with the
    body on the ground, then `HP 6/6`. Both clocks in step.
- Grenade (G): about 1 s of wind-up and 4.4 s of flight. The blast landed 433 units from player 2, inside the
  500-unit outer radius: HP 4/6, knocked about 400 units.
- The right mouse button sends R (the server saw 0x10) and opens no context menu: winit cancels `contextmenu` on the
  canvas.
- Console: the 4 expected WebGL2 WARN lines, 0 errors. Server: 0 ERROR or WARN in the final runs. Earlier, 11
  transient `server_late_input_mismatch` errors while other pages were starting in the same Chrome.
- **Bundle:** with the HUD as page text, `bevy_ui` and `bevy_text` became native-only. 26,523,726 B / 8,263,738 B
  gzip → **21,597,566 B / 6,795,030 B** (wave 2: 21,437,158 / 6,738,427). `client.js` 124,533 B / 17,509 B.
- Someone used the machine during the first runs (keyboard on one page, an Xbox controller on others). The final runs
  logged no foreign input.

### Screenshots (`../lab/screens/wave3/`)
| File | Shows |
|---|---|
| `kill-p1.png`, `kill-p2.png` | native: player 1 firing the twin guns at player 2 146 units ahead (`twin guns (drawn) 42 rounds`); player 2 dead on the ground, player 1 armed behind it (`HP 0/6 … DEAD - respawn in 3.3 s`) |
| `duel-p1.png`, `duel-p2.png` | native, the lag-compensation duel: player 1 firing while player 2 strafes off the line, a bullet in flight, 50 rounds; player 2 at HP 3/6 |
| `grenade-p1.png`, `grenade-p2.png` | native: the grenade in flight down the corridor, 2 left; player 2 at HP 4/6 after the blast |
| `pickup-140-p1.png`, `pickup-170-p1.png` | native, fixed camera: the twin-gun model on placement 12, then the same spot with the model hidden after the pickup (`twin guns (away)`) |
| `browser-kill-p1-wt.png`, `browser-after-kill-p1-wt.png` | Chrome, WebTransport: `kills 1`, 26 rounds, two bullets in flight; a few seconds later, clock 9:34, player 2 gone |
| `browser-dying-p2-ws.png`, `browser-dead-p2-ws.png`, `browser-respawn-p2-ws.png` | Chrome, WebSocket: player 2 falling (`respawn in 3.7 s`), on the ground (0.3 s) with player 1 armed behind, then at HP 6/6 in the record-6 room |
| `browser-hit-p2-ws.png`, `browser-grenade-p2-ws.png` | Chrome: player 2 at HP 4/6 after the blast; its own grenade in the air, `grenades 2` |
| `browser-start-p1-wt.png`, `browser-start-p2-ws.png`, `browser-fire-p1-wt.png`, `browser-bullet-p1-wt.png` | Chrome, the bundle before the page-text HUD: both views at match start, firing (55 rounds), a bullet near the far wall |
| `combat_map.png` | top-down map of scene 0x2D (floors by height, walls, spawns, pickups) from the combat-trace agent's `combat_map.py` |
| `shots.sh`, `duel_p1.txt`, `duel_p2.txt` | the windowed screenshot script and the duel's two scripts |

A white quad sits at the top left of player 1's view at spawn record 1, in native and browser shots alike (seen again
in this run's native check): a render-side gap, not a web one.

### Per crate (wave 3)
| Crate | Rust lines (wave 2) | Tests (re-run here) |
|---|---|---|
| conker-sim | 4,530 (2,086) | 36 unit (38 with `net`) + 9 oracle |
| conker-camera | 2,212 (1,004) | 21 unit + 5 oracle/ROM |
| conker-rules (new) | 4,282 | 32 unit + 3 combat + 4 oracle + 2 ROM |
| conker-anim | 898 (747) | 7 unit + 1 oracle + 3 ROM |
| conker-server | 1,134 (312) | 7 unit, plus `smoke.sh` and `combat.sh` |
| conker-client | 1,881 (1,362) | 14 unit |
| 6 extractor crates | unchanged | 85 |

**conker-sim** (walls, closed-loop and integration agents). Works: goals 1-2 and the network types for combat. API:
- `collision::Layer {Terrain, Secondary, Placement(u16)}` and `WALL_PLACEMENTS = [10, 11]`.
- `CollisionWorld::layered(iter of (points, surface, Layer))`: the game's layers. `CollisionWorld::new` still exists
  but makes every triangle a terrain wall; server and client use `layered`.
- `CollisionWorld::sweep(old, new, &Body, &Sweep, y_vel) -> Swept {pos, hits, y_vel}`, `wall_tree(k)`.
- `CollisionWorld::ray(origin, dir, len) -> Option<RayHit {dist, point, tri}>` (len 0 = unlimited).
- `CollisionWorld` fields `acos: Option<trig::AcosTable>` and `camera: Option<conker_camera::volume::CameraLevel>`.
- `Body {half_width, radius, height, center_y}` with `Body::player(old_y, new_y)` and `Body::camera(size, height)`.
- `Sweep {any_slope, dd3, long_step, no_secondary, no_objects}`; `Default` is the player.
- `trig::AcosTable { from_game_data, acos, atan, angle }`.
- `movement::PlayerState` gains `heading` (obj+0x40), `turning` (obj+0x80), `aiming`, `prev_strafe`, `sink`
  (obj+0xCC), `speed_percent` (obj+0xAA).
- `movement::{BUTTON_C_LEFT, BUTTON_C_RIGHT, STRAFE_TARGET (34), STRAFE_SHARE (0.65), SURFACES: [[i8; 3]; 20]}`.
- `follow::Follow::{tick(&mut p, &input, &world), step_input, step_blocked(p, input, world, aim_blocked), aiming(),
  yaw()}`. `tick` moves, steps the camera, then sets `p.aiming`; `step(p, buttons, world)` is kept (stick centred).
- `Input.camera_yaw` carries `Follow::yaw()` = `CameraState::stick_yaw()`, all 16 bits.
- `net`:
  - `PROTOCOL_ID = 4` (Wave 4: 6; Wave 5: 7), `SCENE`.
  - `Rules { ignore, driven }` with `Rules::ignoring(drawn, jumps, z_armed)` and `apply(&Input)` (Wave 4: removed).
  - `Combat { health, weapon, drawn, rounds, grenades, kills, suicides, immune, anim, respawn_in, lives }` (Wave 5: +
    `slot`, `team`, `stun`).
  - `MatchView { timer, phase, start, first_read, bullets, grenades, taken }`.
  - `scene_world(triangles with layers, game_data, camera_table)`.
  - The input plugin has `lag_compensation: true`.
- Removed: `push_out_of_walls`, `WALL_RADIUS`, `WALL_SPHERE_Y`, `WALL_MAX_NY`.

**conker-camera** (camera and closed-loop agents). Works: goal 2's camera. API (additive):
- `CameraState::step_pad(&PlayerView, buttons, stick: [i8; 2] after the deadzone, &mut impl CameraWorld)`;
  `step_with` = `step_pad` with stick [0, 0]; `step` unchanged.
- `view_eye()` / `view_at()` (render from these), `stick_yaw()` (honours the override). `yaw_u16()` is kept but is
  not what the stick reads.
- New pub fields: `dist_target`, `height_target`, `look_offset`, `look_offset_target`, `look_offset_vel`, `held`,
  `aim: Aim`, `stick_override`, `override_blend`, `heading_rel`. `Default` is manual (247/100, override 999, blend 1).
- `PlayerView` gains `aim_blocked` and `sink: i16` (`PlayerView::new` sets false and 0).
- `trait CameraWorld { floor_below, ray, sweep, sweep_eye (defaults to sweep), in_volume(eye) }`, `RayHit`.
- `volume::{CameraVolume { parse_table, contains }, in_volume(&[CameraVolume], eye), CameraLevel::new(scene, table)
  with in_volume and skips_objects}`.
- `math::{sin_level, cos_level}`; constants `R`, `AIM_DISTANCE` (90), `AIM_LIFT` (25), `NO_OVERRIDE` (999).

**conker-rules** (new: spawns and combat-port agents). No engine, no I/O, wasm-clean. It depends on conker-sim,
conker-camera (the game's sin/cos) and conker-anim (the weapon, hit and death animation ids). API (the crate root
re-exports `Arena`, `Event`, `Fighter`, `Match`, `Phase`, `Rng`, the spawn items and `Surface`):
- `rng::Rng { state: u64 }`: `new`, `next_u32`, `next_f32`, `GAME_SEED`, `Default`.
- `spawn`:
  - `Spawn` (field for field `extract_scene::SpawnPoint`), `Placement { index, pos, facing, camera }`.
  - `place(&[Spawn], index, &mut Rng)`.
  - `respawn(&[Spawn], eligible: u32, last: Option<usize>, enemies: &[[f32; 2]], &mut Rng)`.
  - `DEATHMATCH_START`, `TOTAL_WAR_START`, `DEATHMATCH_MASKS`, `TOTAL_WAR_MASKS`, `JITTER`, `b6_degrees`.
- `surface`: `Surface(u32)` with `kind()`, `slide()`, `sink()`; `SINK: [[i8; 3]; 32]`;
  `sink_target(Surface, speed, height) -> i16`; `step_sink(sink, target) -> i16`.
- `game`:
  - `Arena { spawns, pickups, masks }`, built with `Arena::deathmatch(spawns, |n| placement position)`.
  - `Match { tick, players, bullets, grenades, pickups, timer, zero_at, rng }` with
    `deathmatch(&arena, humans, players, rng, tick)`, `new(&arena, players, rng, tick)`,
    `step(&arena, &[Input], &CollisionWorld) -> Vec<Event>` (one 30 Hz frame), `give(p, weapon_id)` and
    `phase() -> Phase {Playing, Fading, Over}`.
  - `Fighter { body: PlayerState, team, health, immune, stun, anim, action, since, weapon, drawn, rounds, grenades,
    knock_k, kills, suicides, last_spawn, … }` with `new(&Placement, team, tick)`, `item(tick)` and `weapon()`.
  - `Event::{Hit {victim, attacker, damage, lethal}, Shot, Respawn, Pickup, Explosion}`.
  - Constants: `HEALTH`, `SPAWN_IMMUNE`, `MATCH_TIME` (36000), `END_DELAY` (40), `FADE` (16), `STUN_HIT`,
    `STUN_DEAD`, `BUTTON_B`, `BUTTON_R`, `C_DOWN`, `C_BUTTONS`.
- `weapon`:
  - `Weapon` with `jumps()` and `automatic()`.
  - `TWIN_GUNS`, `PISTOL`, `GUN23`, `SNIPER`, `FLAMETHROWER`, `MELEE`, `BLADE`, `W14`, `W16`, and `weapon(id)`.
  - `Attack`, `PICKUPS_2D: [Pickup; 19]`, `Gives`, `touches()`.
  - `PICKUP_RADIUS` (100), `PICKUP_RESPAWN` (120), `MAX_GRENADES` (6), `START_GRENADES` (3), `BULLET_SPEED` (300),
    `BULLET_RANGE` (Wave 4: 20,000).
- `damage`: `DamageType` and the traced types, `knockback(speed, k)`, `sequence(reaction, lethal)`,
  `facing(reaction)`, `in_reach(centre, r, pos)`.
- `projectile`:
  - `Bullet { fire, reach, advance }`; `Grenade { throw(owner, pos, facing, hold, fuse), step(world) }`;
    `throw_speed(hold)`, `COOK_HOLD`.
  - World queries written over conker-sim's API: `raycast`, `hits_body`, `rise_along` (a copy of conker-sim's private
    function), `local_to_world`.

**conker-anim**: the combat ids and rates under goal 3; additive.

**conker-server**: the server and CLI under "Integration".

**conker-client** API: `run(cfg, slice, world, script)`; `world(&Slice, rom) -> Result<CollisionWorld, String>`;
`device_input(keys, mouse, pad)`; `BUTTON_B`, `BUTTON_R`; `Config.lag`, `Config.at`; `script_read`, `shown_anim`,
`hud_text`, `weapon_name`; `Clips` (all 855 ids); `FollowCamera::new(follow, since, lives)` with an aim history;
`render::screenshot`.

**Extractor crates and conker-extract:** unchanged in wave 3. The pickups are the level GLB's placement nodes, so no
extractor change was needed.

### Honest gaps
Wave 4 closed most of these: the flamethrower, weapons 0x14 and 0x16, root motion, pushing, the random picks, the cook
timer, armed aim, the dead-player question, client prediction of hits, per-bullet lag compensation, symmetric `lag=`,
`Match::remove_player`, the hint cut, the zone gate, C-strafes, the stop rule and the 122 brake. What remains is in
wave 4's "Honest gaps".
- **Combat not ported:**
  - The flamethrower's flames: no damage of type 0x1C, no burning, no type-0x21 burn hit. The data to fit them is the
    288 `func_1505D1C4` calls in `combat_e_w15_lldb.txt` (README §3.5).
  - Weapons 0x14 and 0x16: their Z states play, but no hit was ever traced.
  - Root motion: 0x16's Z (back 46.5), the 0x17E death spin of sniper and 0x22 deaths (+3072 per frame, drifting),
    creep.
  - Players pushing each other (the wall routine's layer 3).
  - The twin guns' idle fidget 0x144 and the 0x129-or-0x12A swing pick, which look random.
  - Grenade holds of 2-59 frames: the throw speed (41 assumed) and the fuse (the quick throw's) are unmeasured, and so
    is the cap of 90.
  - Armed aim variants (cam+0x23E: per-weapon distance, lift and stick sensitivity); only unarmed aim is ported.
  - The bots' armed run ids at full stick (0x22C twin guns, 0x15C blade).
- **Combat inferred (I):** the melee lock-on range 200; sniper type 0x20 versus 0x12 as head versus body; the bounce
  minimum of 5; how dead bodies stop bullets before they lie in 0x59; kill credit for one's own grenade (C only).
- **Respawn scores and dead players:** the spawns agent saw the respawning player's own object at (0, 0, 0) at pick
  time. Whether other dead, not yet respawned players also sit there is open; the port counts them where their
  bodies lie. One pick (bots3 1695) is consistent with that but doesn't decide it. (Wave 4: settled by
  `combat4_dead2`: they count where their bodies lie.)
- The game's RNG can't be kept in step (every explosion draws it more for effects, and its start depends on boot);
  the port's RNG only serves the spawn picker.
- **Prediction:** a player who is hit or dead predicts no movement, so every knock-back tick and respawn is a server
  correction (41-70 rollbacks for the victim per replay). `Match` and `Fighter` have no serde, so clients can't run
  the rules. `Rules` mispredicts only on the tick a draw or holster completes, and only if Z or A is pressed then.
- **Lag compensation:**
  - Two shooters in one tick fall back to no rewind (9 of 72 bullet ticks in a mutual-fire run).
  - Melee strikes and the sniper ray use the shooter's shifted position, and a new bullet's first-frame reach is
    measured from the shifted muzzle.
  - One victim is rewound per tick, not per bullet.
  - The history covers 60 ticks (2 s), and `lag=` delays only server-to-client packets.
  - The fix belongs in conker-rules: a `Match::step` variant taking the positions each shooter saw (e.g.
    `seen(shooter, victim)`), used in `first_body`, the melee strike and the sniper ray. It would replace the
    server's `compensate`.
- **Movement gaps the combat replays show:** after a walk stops the game keeps move 25089 / facing 25090 where the
  sim snaps (e_w21/e_w22 from 816); reversing the stick in state 122 brakes 4.5 per frame (pick87 133-136); short
  C taps are 4.3-5.0 off (cause open).
- **Movement not ported:**
  - C-down's own move (1.09 at camera_buttons 160; the throw is in conker-rules).
  - C-strafe with the stick held (the facing turns 468 per frame).
  - Creep root motion.
  - The class-6 slide.
  - Speed percentages for crouch-walk and strafe, and obj+0x13C ≥ 100 (× 0.8).
  - The wall routine's zone gate.
  - Wave 2's bracketed thresholds (I): gait 6/14, skid 21, stop 4, snap-down 15, hard landing −80, crouch-walk 5,
    the reversal constants, air control −11, the turn-cap fit.
- **Camera not ported:**
  - The level's camera hint cut (cam+0x6FC type 5, `func_1512E8E0` → `func_15128774`): `hint_cut` stays an input,
    so the closed loop misses such cuts (ledge 410).
  - Ceilings (`func_1512E140`; it never binds in a trace).
  - Volume shapes 1 and 3 (none on 0x2D); cylinders are C only.
  - The cut tick's two extra mode dispatches (no effect seen).
- **Duplication to clean up:** two copies of the per-kind surface table (conker-sim `SURFACES`, conker-rules
  `surface::SINK`). conker-rules' world queries (`raycast`, `hits_body`, the copied `rise_along`, a private
  `push_out`) belong in conker-sim. The server's `remove_slot` belongs in conker-rules as `Match::remove_player`.
- **Rendering:** bullets and grenades are spheres, no weapon is drawn in hand, and the white quad sits near record 1.
- **Browsers:** Firefox and Safari are untested. One gamepad drove the players of two DevTools-driven windows at
  once (that Chrome reports every page as focused); the client doesn't check window focus before reading the pad
  (Wave 5: it does).
- 3-VI frames are untested everywhere; all traces are pinned at 2 VIs.
- **Docs outside this tree are stale:** `../RUST_REMAKE_PLAN.md` says wave 3 is running; `../lab/oracle/README.md`
  lacks wave 2's and wave 3's corrections; `combat_README.md` lacks the combat-port agent's corrections;
  `wall_README.md` says the tests replay flipped rows. Those tests are now `trace_camera` and `camera_loop`, and no
  flips occur.

### Wave 4, ranked
1. **Finish combat:**
   - The flamethrower (fit e_w15's particle log).
   - Root motion at runtime: 0x16's Z, the 0x17E death spin, creep.
   - Player pushing (the wall routine's layer 3).
   - The fidget and swing picks: find their RNG draws.
   - Armed aim variants (trace R with each weapon) and the armed run ids.
   - Targeted oracle runs: weapons 0x14 and 0x16 at contact range or against another target class; a cooked grenade
     held 20-40 frames (speed, fuse); a melee swing at 150-250 units (the lock-on range); two players dead at once
     (the picker); the sniper's head versus body.
2. **Combat netcode:**
   - The lag-compensation hook in conker-rules (`seen(shooter, victim)`) replacing the server's `compensate`.
   - `Match::remove_player`.
   - serde on `Match`/`Fighter` (or a one-player step), so clients predict knock-back and respawns.
   - A symmetric `lag=` for testing.
3. **Remaining controls and movement:**
   - C-strafe with the stick held, C-down's move and the short C taps.
   - The stop rule (25089/25090) and the state-122 reversal brake.
   - The class-6 slide and the wall routine's zone gate.
   - The level's camera hint cut and ceilings.
   - Measure the bracketed thresholds with targeted traces.
4. **Bots** (server-side): compared qualitatively, then by statistics (kills/min, paths). The oracle's bot matches
   (`combat_bots3`, the spawn captures) are a start.
5. **Netcode robustness** (carried from wave 2):
   - Dual-stack `[::]` bind; a disconnect on exit and `pagehide`, then back to a 3 s timeout.
   - Gate input on asset loading; a status line on connection failure.
   - Check window focus before reading the gamepad.
6. **HUD and lobby:** the game's own HUD, a scoreboard, the join flow, spectators, the match-end screen (scene 0x1D);
   weapons in hand, bullet and grenade models.
7. **Audio:** MP3 voices, then the sound banks; combat events now give them triggers.
8. **Deployment:** a VPS with real certificates, a non-zero netcode key, `wss://`, brotli for the wasm, and a
   hostname-safe connect token.
9. **Browsers:** Firefox and Safari 26.4+ (`web/README.md`'s two-window combat test), background tabs, gamepads.
10. **Fidelity details:** 3-VI frames, the render-only camera offsets, segment-8 render state, fog and sky from the
    game, the level duplicates and the white quad. Consolidate the duplicated tables and world queries into
    conker-sim.
11. **Modes:** War: Total War (scenario 9, same scene; its start records and masks are known), then the others.

## Wave 2 (2026-10-08)
Seven agents worked in this wave:
- movement (conker-sim): **partial**, walls
- camera (new conker-camera): **partial**, wall avoidance, aim camera and the blocked-view cut
- animation (new conker-anim): done
- extractor (in-memory `Slice`): done
- client assets: done
- integration (server, prediction, camera and animation in the client): done
- browser: done

Lab outputs:
- `../lab/oracle/`: 26 scripted scenarios (the wave-1 six plus 20 new scripts), `anim_README.md`, and tools `move_run.sh`,
  `move_raw.sh`, `camera_run.sh`, `camera_probe.py`, `camera_recomp.py`, `anim_fields.py`, `anim_lldb.sh`/`.py`.
- `../lab/extract/slice-glb/` and `../lab/extract/wave2-check/`.
- `../lab/screens/wave2/`.

### Verification run (status agent, rustc 1.98.1, all re-run here)
| Check | Result |
|---|---|
| Extractor tests, 6 crates, ROM + the three DOS variables (absolute) | **85/85 pass** (wave 1: 81), 36 s wall in debug. 1,272 DOS textures compared (704 + 471 + 91 + 6); all 1,770,276 clip values equal DOS's |
| The same without env | 85/85; the ROM and DOS tests return early (28 skip lines) |
| conker-camera | 12 unit + 4 oracle/ROM tests pass |
| conker-anim | 5 unit + 1 oracle replay + 3 ROM tests pass |
| conker-sim (default target, ROM) | 25 unit + 6 oracle tests pass, 2 ignored (`ledge`, `report_all`). With `--include-ignored`, `ledge` fails as designed (max 204.5, 29 state misses) and `report_all` prints the tables below |
| target-net: conker-sim `--features net`, conker-client, conker-server | 26 + 6 (2 ignored), 10, 1: all pass |
| `cargo check --target wasm32-unknown-unknown --lib`, 9 library crates (6 extractor + camera, anim, sim) | clean, 0 warnings |
| `cargo fmt --all --check` | clean |
| Rust source | 13,081 lines in 11 crates (`*.rs`, tests included) |
| `smoke.sh wt ws ws` | **16/16 ok** (below). No process left running |
| `replay.sh`, the six wave-1 scripts through server + client | see "Through the server" below |
| `cargo check --target wasm32-unknown-unknown -p conker-client --lib` (target-net; includes conker-sim `net`) | clean |
| clippy, the web-profile wasm build | not re-run here. The agents report clippy clean (native; wasm32 for conker-client and conker-sim+net). `web/dist` was built by the browser agent |

### Fidelity against the original (oracle traces in `../lab/oracle/trajectories`)
**Wave 3:** the movement and camera tables below are wave 2's record. Wave 3's tables (goal 2 of the wave-3 section)
supersede them: every trace is now within 0.001 in both modes, ledge included.

**Movement** (`cargo test -p conker-sim --test oracle -- --include-ignored`, player 1, reads 30 to the end, row N+1 = the
result of frame N, tolerance 0.1 units):
- "Trace camera": the yaw comes from the trace. "conker-camera": the loop is closed as server and client run it.
- "First bad" is the first row over 0.1 units. With conker-camera, most first bad rows are a move angle one 256-step
  off. The harness names three causes (I):
  - the game reads a slightly later camera near a step boundary
  - the facing in the air differs
  - the camera needs the wall-aligned heading in state 122
- In six traces the step happens at speed 4-5, a few frames after setting off (I): `move_release` 149 (16641 instead of
  16385), `move_speeds` 142, `anim_jump_stand` 297, `move_wall3` 138, `move_wall5` 177 and `move_wall6` 198.
- "Yaw steps off" counts the rows where conker-camera's 256-step differs from the oracle camera's.

| Trace | Rows | Trace camera: max / state misses | conker-camera: max / state misses / yaw steps off | First bad (trace cam / conker-camera) |
|---|---|---|---|---|
| **idle** | 210 | 0 / 0 | 0 / 0 / 0 | – |
| **walk** | 240 | 0 / 0 | 0 / 0 / 0 | – |
| **run** | 240 | 0 / 0 | 0 / 0 / 0 | – |
| **turn** | 330 | 0 / 0 | 0 / 0 / 0 | – |
| **jump** | 270 | 0.0008 / 0 | 0.0008 / 0 / 0 | – |
| **ledge** | 480 | **204.5** / 29 | 205.6 / 29 / 130 | 275 (two-wall corner) / 275 |
| camera_circle | 479 | 0 / 0 | 0 / 0 / 0 | – |
| camera_swing | 540 | 0 / 0 | 0 / 0 / 0 | – |
| anim_idle_long | 2125 | 0 / 0 | 0 / 0 / 0 | – |
| move_jump | 476 | 0.016 / 0 | 0.016 / 0 / 11 | – |
| move_wall2 | 647 | 0 / 0 | 3225 / 244 / 482 | – / 196 (wall push) |
| camera_jump | 450 | 0.001 / 0 | 58.8 / 0 / 320 | – / 165 |
| anim_jump_stand | 386 | 0.155 / 0 | 20.3 / 0 / 121 | 377 / 297 |
| move_release | 476 | 0.160 / 0 | 31.7 / 0 / 359 | 321 / 149 |
| camera_wall | 449 | 0.177 / 0 | 26.2 / 1 / 243 | 399 / 231 |
| move_wall3 | 677 | 0.248 / 0 | 13.7 / 0 / 531 | 214 / 138 |
| move_slope | 806 | 0.255 / 0 | 41.2 / 0 / 422 | 632 / 399 |
| move_speeds | 476 | 0.368 / 0 | 42.3 / 0 / 366 | 328 / 142 |
| move_crouch | 409 | 0.581 / 0 | 57.7 / 0 / 340 | 99 / 99 |
| anim_crouch | 446 | 0.612 / 0 | 16.6 / 0 / 181 | 296 (creep) / 296 |
| move_turnrun | 476 | 0.709 / 0 | 42.6 / 2 / 297 | 261 / 209 |
| move_wall1 | 648 | 2.73 / 0 | 3.32 / 0 / 408 | 208 (run along a wall) / 208 |
| move_wall6 | 737 | 20.7 / 18 (the hit at read 730, see below; walls alone stay under 1) | 323 / 25 / 570 | 316 / 198 |
| anim_creep | 420 | 116.5 / 0 | 116.5 / 0 / 241 | 56 (creep root motion) / 56 |
| move_wall5 | 707 | 1396 / 23 | 1396 / 85 / 561 | 280 / 177 |
| camera_buttons | 720 | 1397 / 187 | 447 / 187 / 384 | 115 (C-down, state 118) / 115 |

In bold, the six required scenarios. With the trace camera:
- The floor query alone is bit-exact on 9,160 of 9,262 traced frames. The other 102 are wall-push frames, about 0.01 off.
- The movement agent attributes the sub-unit errors to walls and to rows near a 256 boundary where the game read a
  later camera. The harness names five such rows (move_crouch 99/151, move_release 321, ledge 320, move_speeds 328).
  Others look the same, e.g. anim_jump_stand 377.
- `ledge`: with the traced state copied back after each bad row (`ORACLE_RESYNC=1`), only wall frames remain
  (275-282, 306-316, 320, 450-452). The edge fall, fall clip, hard landing and the first plateau wall all match.

**Through the server** (`replay.sh`: conker-server with `--spawn` at the original's spawn, one headless ws client running
the script, conker-camera in the loop; the server's state per tick against the trace):

| Script | Rows (reads) | Max error | Mean | State misses |
|---|---|---|---|---|
| idle | 217 (24-240) | 0.0000 | 0.0000 | 1 (read 24) |
| walk | 247 (24-270) | 0.0000 | 0.0000 | 1 (read 24) |
| run | 247 (24-270) | 0.0000 | 0.0000 | 1 (read 24) |
| turn | 337 (24-360) | 0.0000 | 0.0000 | 1 (read 24) |
| jump | 277 (24-300) | 0.0008 (read 164) | 0.0000 | 1 (read 24) |
| ledge | 487 (24-510) | **205.6** (read 419) | 27.5 | 30 (walls; the movement harness gives 204.5 / 29) |

- Spawn offset 0 in every run.
- The script prints 4 decimals. The integration agent compared at f32 precision: idle, walk, run and turn are
  f32-exact on every row, and jump on 240 of 277.

Each run has exactly 1 state miss, at read 24. That frame is the original's one-frame spawn state (state 0, y_vel 0,
gravity 6), and `PlayerState::spawn` starts idle instead; the next frame matches.

**Prediction** (this run's `smoke.sh wt ws ws`): server, then clients 1 (wt) and 2 (ws), then client 3 (ws) joining
8.6 s later, mid-match. Results:
- Every client connects, and the server applies every player's inputs (23 / 23 / 9 distinct positions).
- Each client sees every other player move and animate (9-23 positions, 5-16 clip times).
- The server wrote 1,676 state rows: none below the floor, none without one.
- **Predicted vs server state at the same tick: 694 / 694 / 279 ticks, max error 0.000000, 0 state misses,
  0 rollbacks.**
- The integration agent's windowed pair (`pair.sh`): 665 and 666 ticks, max error 0, 0 rollbacks.

**Camera** (`cargo test -p conker-camera`). Eye and look-at are compared with the trace's camera columns.
- "Oracle-fed" takes heading, head height and the floor under the eye from the raw dumps.
- "ROM-fed" uses extract-scene's `floor_below` on the 0x2D collision, and the heading computed from the facing.

| Traces | Rows | Oracle-fed | ROM-fed |
|---|---|---|---|
| idle_a, walk_a, run_a, turn_a, jump_a, camera_circle, camera_swing | 2,148 | **0.0 (bit-equal, asserted)** | 0.0 |
| camera_jump | 427 | 0.0 (bit-equal) | eye 0.108 at rows 264-269: extract-scene's exact-plane floor, not the game's integer one |
| camera_ledge | 455 | eye 1.21 (339-355, sphere sweep), 166 (411-438, blocked-view cut) | diverges from row 206: state 122 needs the wall-aligned heading |
| camera_wall | 426 | eye 108 (263-319, wall ray + sweep), 0.074 (388-394) | diverges from row 206 (same cause) |
| camera_buttons | 697 | within 0.05 except R's aim camera (354-411, 534-615, max 159) and its aftermath (618-632, 0.21) | same, plus 1.37 at 666-709 |

Other camera checks:
- Stick rule: 1,226 of 1,231 running frames land within +1 of the target computed from the oracle's camera.
- The spawn frames (reads 23→24): look-at exact, eye within 1e-3.

**Animation** (`cargo test -p conker-anim`, player 1, frames 24 to the end of 10 traces):

| Measure | Result |
|---|---|
| Rate (obj+0x4C) and clip time (obj+0xB4) | **5,170/5,170 frames bit-exact** each |
| Ground choice (`Ground::choose`) vs the traced id, on ground frames that are not held | 4,951/4,952. The miss: anim_jump_stand read 354 (stick reversed at full speed; the game's drive target is 0 there) |
| Clock and blend state (id, previous id, time, previous time, duration, step, weight) vs the lldb clock captures | 5,160/5,160 rows exact |
| State byte obj+0x85 = low byte of the animation id obj+0x84 | 10,620/10,620 rows |
| Creep root motion | 169 frames, max 0.012 units |
| Rate constants | all 7 equal the ROM's `.game` data words |

Per-scenario numbers and V/C/I tags: `../lab/oracle/anim_README.md`.

### Rules found in wave 2 (they correct or extend `../lab/oracle/README.md`, which is not updated yet)
V = matched on traces, C = read in the recompiled code, I = inferred or only bracketed by the traces.

**Corrections to the README:**
- ~~The **"−256" is explained** (V): the stick target is `(camera_yaw & 0xFF00) + stick angle − 16384`, using last tick's
  camera, quantised down to 256 steps. The spawn camera sits at 16895.996, so forward aims at 16640. It is not a
  constant offset; a camera reporting exactly 16896 makes every scenario start one step off. Measured on 1,226 of
  1,231 running frames.~~ **Wave 3 (C, then V on 2,154 of 2,154 steering rows):** the camera is not rounded. The
  target is `trunc(cam+0x37C · 182.04445)` (or the stick override cam+0x384 when it is not 999) plus the stick angle,
  all 16 bits; the move angle turns by `((target − move) as i16 >> 8) << 8` and keeps its own low byte. That explains
  the −256 (a difference of −1 floors to a whole −256 step) and the near-boundary misses.
- **On stop, the move angle snaps to the facing** (V), not the other way round. Below speed 1 the player does not move.
- **Slopes** (V, to 1e-6 on 12 triangles): every step covers the same distance along the floor's plane, so the
  horizontal part is scaled by 1/sqrt(1 + rise²) along the move direction, not by normal.y.
- **Jump take-off** (V): the first airborne step acts as y_vel 58.999985. The README's 55 is the value on the row.
- **The floor height** is the game's own fixed-point routine, ported bit for bit (`collision::height`).
  - When last frame's floor triangle still covers the point, the position is rounded; otherwise every triangle is
    searched with the position truncated.
  - A floor counts up to 50 above the reference height.

**Movement:**
- Turning (I, linear fit): at most 6400 per frame below speed 5, then floor(15.8 − 0.24·speed)·256. The low byte of the
  move angle is kept, and an exact reversal turns clockwise. The facing turns at most 3072, otherwise stops one unit
  short (hence 16641). **Wave 3 (C, `func_150599C8`):** a clockwise facing turn stops one short, also for exactly
  −3072; the airborne facing and obj+0x80 are in the wave-3 section.
- Speed (V): +1 per frame while that stays at or under the target, then +0.01 up to it.
- Gait (I thresholds): on w = 0.6·target + 0.4·speed (conker-anim's e), creep up to 6, walk up to 14, run above.
- Releasing the stick: above speed 21 a skid of 17 frames (I threshold), below 4 idle, otherwise −2.5 per frame.
- Stick reversed against the motion: brakes 4.5 per frame, then restarts at speed 1 with a 3072 turn (I thresholds).
- Edges: the floor is followed down at most 15 units (I). Walking off adds the plane's drop plus half the updated
  y_vel (V). The fall clip lasts 15 frames, then the run clip plays in the air.
- Jumps: gravity 4 while A stays held, at any length, then 6.2. In the air the turn is 256 per frame while
  y_vel > −11 (I).
- Landings: a soft landing freezes for 15 frames. A hard one (y_vel < −80, I) lasts 12 frames: frozen for 6, then
  accelerating for 6. A running jump lands straight into a run.
- Crouch:
  - Z gives state 22. A slide from a run starts at the run's speed and brakes 3.2 per frame.
  - Below speed 5 (I) the slide becomes a crouch-walk (27) aiming at 0.24 × stick, capped at f32 0x415FFFFF.
- ~~Walls (I as a model): a sphere of radius 30, centred 42.5 above the feet. Pushing into a wall within ±47.8° gives
  state 122 and caps the speed at 14. The slide uses the already-accelerated speed (15), then caps. The floor is
  sampled before the push.~~ **Wave 3:** replaced by the game's own wall routine (goal 1 of the wave-3 section): a
  box 60 wide and trunc(90 + |Δy|) tall, radius 30, state 122 from the push angle within [0x5E01, 0xA200), and the
  floor re-queried after a push.
- Surfaces met:
  - Floors: 0x5 (12,911 frames), 0x3 (the ramp, 1,705) and 0x4 (the plateau, 2,183).
  - Walls: 0x5, 0x4, or no word (placements).
  - The words don't separate walls from floors; slope does: |normal.y| < 0.5 is a wall. **Wave 3:** the game's test is
    `n.y/|n| < 0.5` unless surface bit 24 is set. The words' meanings are decoded (kind, slide class, sink: goal 4);
    the 12 class-6 faces are slide floors (8 of them steeper than 0.5). The slide is not ported, and the sim on
    those faces is untested.

**Camera** (V; constants from the game's rodata; mode 8, `func_15122C5C`):
- Tick order: the stick is resolved with last tick's camera, the player moves, then the camera steps.
- Look-at:
  - x/z are the player's.
  - Its height springs (4, 8; doubled going down) toward floor + air·min(air/300, 1) + 0.75·head, which is +67.5
    standing and +30 crouch-walking.
- Yaw:
  - Each tick it is recomputed from the old eye to the new look-at. That drag makes the camera trail: distance up to
    ~320 at a full run.
  - It then springs (k 1, damping 2.8, snapping within 10°) toward heading − 180. This swing-back is why a held
    sideways stick runs a circle.
- Distance springs (6, 8) back to 247.
- Eye height springs (6, 8) to 100 above max(floor under the eye, player y), at least 40 above that floor. It
  freezes while the player is more than 600 above its floor.
- Buttons:
  - Any C button holds the pre-move yaw, blending in only part of the drag. While strafing, the swing-back
    constants spring from 0.
  - C-up, L, Z and C-down change nothing while standing.
  - Multiplayer has no zoom cycling.
- Measured, not ported (**Wave 3:** the wall ray, the sphere sweep, the cam+0x84 bit 0x40000 rule (camera volumes) and
  R's aim camera are ported, see goal 2; the blocked-view cut, a level camera hint, and ceilings are still not):
  - The wall ray (`func_15123A54`: from the player +140 up, 277 long, distance = hit − 12, at least 50). It is gated
    by cam+0x84 bit 0x40000, whose rule is unknown.
  - The sphere sweep (`func_1512BB10` → `func_15044380`).
  - The blocked-view cut (ledge read 411).
  - R's aim camera (mode 0x100: distance 90, eye +67.5, look-at +92.5).
  - Ceilings.
- Fields: cam+0x37C is the yaw, +0x380 the view yaw, +0x39C the look-ahead offset, +0x5E8 a view-yaw offset.

**Animation** (V/C, `../lab/oracle/anim_README.md`):
- The movement state is the animation id: obj+0x85 is the low byte of obj+0x84 on every traced row, so state → clip is
  the identity.
- Rates per id (e = 0.4·speed + 0.6·target):

  | State | Rate |
  |---|---|
  | creep | e/8 + 0.1 |
  | walk | e/8 + 0.3 |
  | run | e/23 + 0.8 |
  | wall push | e/15 + 0.6 |
  | crouch-walk | target/15 + 0.5 |
  | crouch | 1.5 |
  | skid, fall | 0.5 |
  | idle, jump, land | 1 |
  | running jump | 0.8, starting at tick 15 |

- The clock: time += rate, wrapping at the clip's duration. Every player clip loops.
- The cross-fade: step = 500 / trunc(blend) per VI, weight from 1023, minus 2 steps per frame. Switching back to the
  clip being faded from fades back.
- Root motion: the game moves the player by speed, never by the clip, except creep (state 0). There the player moves
  by 0.5 × clip 0's root-z advance of two frames earlier. Renderers drop root x/z and keep root y.

### Per crate
#### conker-sim: movement in game units (movement + integration agents), 2,086 lines
- **Works:**
  - The rules above, on scene 0x2D's collision (`CollisionWorld`: 512-unit XZ grid, the game's integer floor routine).
  - Only + − × ÷ and sqrt are used, so native and wasm give the same bits.
  - `follow` wires conker-camera to the floor query, with a last-triangle cache.
  - Under `net`: lightyear protocol 2, the player state replicated, predicted and linearly interpolated.
- **API:**
  - `collision::CollisionWorld::new(impl IntoIterator<Item = ([[f32;3];3], u32)>)`: triangles plus surface word
    (vertices rounded to integers, as the game stores them).
  - `CollisionWorld::floor(x, z, y, ref_y, prev_tri: Option<u32>) -> Option<Floor { y, tri, surface }>`: the game's
    floor query.
  - ~~`CollisionWorld::push_out_of_walls(pos) -> pos`: the approximate walls.~~ **Wave 3:** removed, with
    `WALL_RADIUS`, `WALL_SPHERE_Y` and `WALL_MAX_NY`; the game's routine is `CollisionWorld::sweep` (wave-3 API).
  - `collision::height(&[[i32;3];3], x, z)`, plus the constants `NO_FLOOR`, `STEP_UP`, `WALL_RADIUS`,
    `WALL_SPHERE_Y`, `WALL_MAX_NY`.
  - `movement::Input { stick_x: i8, stick_y: i8, buttons: u16, camera_yaw: u16 }`: the raw stick (before the
    deadzone), N64 buttons, and last tick's camera yaw (only `>> 8` is used). **Wave 3:** all 16 bits are used; it
    carries `Follow::yaw()` = `CameraState::stick_yaw()`.
  - `movement::PlayerState::spawn(pos, facing)` and `movement::step(&mut PlayerState, &Input, &CollisionWorld)`: one
    30 Hz tick.
  - `movement::state::{CREEP 0, WALK 1, RUN 2, IDLE 15, CROUCH 22, CROUCH_WALK 27, SKID 33, JUMP 50, LAND 54,
    RUN_JUMP 55, FALL 56, PUSH_WALL 122}`, `BUTTON_A`, `BUTTON_Z`.
  - Helpers `deadzone`, `angle_diff`, `cos_sin`, `atan2_u16`, `stick_angle8`, `target_angle`, and the rule constants.
  - `follow::view(&PlayerState) -> PlayerView` (head 40 in state 27).
  - `follow::Follow { cam, floor }`: `Follow::spawn(eye, &p)`, `.step(&p, buttons, &world)`, `.yaw() -> u16`.
  - `TICK_HZ = 30.0`.
  - With `net`:
    - `PROTOCOL_ID = 2`, `DEV_PRIVATE_KEY`, `DEFAULT_PORT = 5888`.
    - `PlayerId(PeerId)`, `Player(PlayerState)`, `Collision(CollisionWorld)`.
    - `lerp(a, b, t)`: position and facing take the short way; everything else is b's.
    - `ProtocolPlugin`, which registers `InputPlugin::<movement::Input>`. `Input`, `PlayerState` and `Floor` also
      derive serde and Reflect.
  - The wave-1 placeholder (`Body`, `PlayerInput`, the old `step`) is gone.
- **Depends on** conker-camera, not on conker-anim. The state byte is the animation id, and the client drives
  `AnimPlayer` from (state, speed, target).
- **Gaps** (**Wave 3:** the first three are fixed; the walls, the heading and the stick read are ported, and every
  trace replays within 0.001. Strafing with the stick centred, R's brake and aim gate, and A from state 122 are
  simulated now; the rest of this list stands, see the wave-3 gaps):
  - Walls are a model, not the game's routine (`func_15044380` → 0x150AB1F0). This breaks ledge, two-wall corners
    (push about 2.4° off the normal), run-state slides along a wall (0.3-0.6 short per frame), move_wall5, and the
    floor height while pushing (y off by up to 0.18). In move_wall6 the walls drift only 0.6-1.0; its 20.7 and 18
    state misses are the hit at read 730, which the sim does not model.
  - No wall-aligned heading is produced for the camera in state 122. The closed loop diverges there (move_wall2).
  - The later camera read near a 256 boundary is not modelled. It accounts for most of the closed-loop divergence in
    the table above.
  - Thresholds are bracketed, not measured (I): gait 6/14, skid 21, stop 4, snap-down 15, hard landing −80,
    crouch-walk 5, reversal 0x6000/8/4/10.5, air control −11, the turn-cap fit, wall |normal.y| 0.5.
  - Creep moves by speed here, not by clip 0's root motion: 0.46 units off per frame at speed 3, 116 units over
    anim_creep.
  - Not simulated:
    - multiplayer C buttons: C-down states 118/119/107 and C-left/right strafing (camera_buttons: 187 state misses)
    - R: the brake into a skid and the aim camera
    - A from states 122, 54, 22 and 27 (untraced)
  - Clip lengths are hard-coded (jump 24, fall 15, skid 17, landings 15/12) rather than read from the clips.
  - The spawn frame is idle here; the original has one frame of state 0.
  - Untraced: 3-VI frames, floors on placement collision, slopes with |normal.y| between 0.5 and 0.78.

#### conker-camera (new, camera agent), 1,004 lines, no dependencies
- **Works:** the multiplayer follow camera (mode 8), bit-equal to the oracle on the 8 traces above. The libultra
  sinf/cosf, the game's asinf (`func_15048C30`) and its springs are ported op for op, so server and wasm client agree.
- **API:**
  - `CameraState::spawn(spawn_record_eye, &PlayerView)`, then
    `.step(&PlayerView, buttons, floor_below: FnMut(eye, eye_y_at_tick_start) -> Option<f32>)` and `.yaw_u16()`.
    The state is Copy with all fields pub (eye, at, yaw, the spring velocities, floor, base, strafe blend,
    prev_pos, cut), so it can be rolled back and replicated.
  - `PlayerView { pos, floor_y, heading, head, strafing }` and `PlayerView::new(pos, floor_y, facing)`.
  - `heading_from_facing(facing)` = facing·360/65536 + 90: exact except in state 122.
  - Constants `DT`, `DISTANCE` 247, `HEIGHT` 100, `HEAD_STANDING` 90, `C_BUTTONS`, `C_DOWN`.
  - `math::{sin, cos, asin, yaw_of, angle_diff, wrap360, spring, spring_angle, DEG_TO_RAD}`.
- **Gaps:**
  - Not ported: the wall ray, the sphere sweep, the blocked-view cut, R's aim camera, ceilings, water, and the
    wall-hit look-at variants. **The eye can go through a wall behind the player** (camera_wall: up to 108).
    **Wave 3:** the wall ray and sweep (answered by conker-sim through `CameraWorld`), the volumes that gate them and
    unarmed aim are ported; camera_wall is exact. The blocked-view cut (a level camera hint), ceilings, water and the
    armed aim variants are not.
  - `strafing` stands in for s126+0x1B3, which lies 3 bytes past the raw dump. What was verified is "C-left or
    C-right held".
  - Render-only view yaw/pitch offsets (cam+0x5E8/0x3A8, up to ~3.7° while turning) are not ported.
  - DT is fixed at 2 VIs; 3-VI frames are untested.

#### conker-anim (new, animation agent), 747 lines, no dependencies
- **Works:** picks, paces and blends the player animation exactly as the game does (numbers above).
- **API:**
  - `request(id, speed, target) -> Request { id, rate, blend, start }`, `effective_speed(speed, target)`.
  - `SPAWN`, `NONE`, `id::{CREEP, WALK, RUN, IDLE, CROUCH, CROUCH_WALK, SKID, JUMP, LAND, RUN_JUMP, FALL, WALL_PUSH}`.
  - `Ground { crouch_walk_ok }.choose(speed, target, z_held, height) -> u16`: the game's ground choice.
  - `AnimPlayer { id, time, rate, duration, prev, prev_time, weight, step }` with `play`, `advance`,
    `tick(req, duration_ticks)` and `prev_weight()`.
  - Per tick: `tick(request(state, speed, target), duration)`. Render clip `id` at `time/30` s, blended with `prev` at
    `prev_time/30` s by `prev_weight()`.
- **Gaps:**
  - The place in `func_1504CB98` where the drive target is zeroed (stick against the motion) is not located.
    Callers pass that zero themselves.
  - The renderer's blend mixing (assumed linear weight/1023) was not read (I). The game snapshots the blended pose;
    `AnimPlayer` keeps only the source clip and time.
  - Not ported (untraced):
    - weapon and held-object locomotion (0x2B, 0xDF), needed for combat. **Wave 3:** the traced weapon ids are in
      the `combat` module, with fitted rates for 0x124, 0x15B and 0x12B; 0x2B and 0xDF are not among them.
    - wading (0x8B), special idles, the obj+0x103 restart, and the 3-VI clock
    - the two displacements at creep's loop point
  - No idle variation in 70 s of idle.

#### conker-extract and extractor crates (extractor agent)
- **Works:** `Slice::from_rom(Vec<u8>)` builds everything the client needs in memory, wasm-compatible:
  - Scene 0x2D as one GLB with 144 PNGs embedded. Node extras carry `model` and `placement`.
  - Model 117 as one GLB: 28 joints, 764 animations, 4 PNGs. Animation extras carry pair, frames, durationTicks and
    logicalIds.
  - 4,299 collision triangles, 20 spawn records, and one `ClipInfo` per animation.
  - The Khronos validator reports 0 errors and 0 warnings on both GLBs.
  - Texture decoding exists once, in extract-gbi; extract-level and extract-character both go through it.
  - **PNGs are now upright** (rows reversed, v = 1 − v). Everything else is unchanged from wave 1 (`compare_wave2.py`):
    - 6 slice files are byte-identical.
    - 2,894 texture PNGs, 144 level PNGs and 4 character PNGs are exactly flipped.
    - 4,586 TEXCOORD_0 values are 1 − v (max error 2.4e-7).
- **API:**
  - `Slice { level_glb, character_glb, collision: Vec<Triangle>, spawns: Vec<SpawnPoint>, clips: Vec<ClipInfo> }`,
    from `Slice::from_rom(rom: Vec<u8>) -> anyhow::Result<Slice>`. It accepts z64, v64 and n64 byte orders and checks
    the US SHA-1.
  - `ClipInfo { pair, logical: Vec<u16>, frames, duration_ticks }`. Loop on `duration_ticks`.
  - Re-exported: `Triangle { points, layer, surface: Option<u32> }`, `Layer::{Terrain, Secondary, Placement(i)}`, and
    `SpawnPoint { position: [i16;3], yaw: u8, camera: [f32;3], byte_06, byte_14, byte_15 }`. Facing = yaw << 8.
  - `SLICE_SCENE = 0x2D`, `SLICE_CHARACTER = 117`. The CLI exports are unchanged (`Export { files, summary }`).
  - extract-gbi:
    - `decode(format, w, h, data, tlut, interleaved)` returns RGBA8.
    - `png(w, h, rgba)`.
    - `Material::texture_png(|id| payload)` returns an upright PNG.
    - `glb(json, bin)` packs a binary glTF.
    - `Texture::uv` returns v = 1 − t/height.
  - extract-level: `Gltf::glb()`.
- **Timing:**
  - `Slice::from_rom`: 198-211 ms native in release (4 runs); 2.2-2.65 s in debug builds (test, client, server).
  - In Chrome only the total was measured: ROM picked → client running, 1.5 s (client agent) and 1.8 s
    (1,793-1,839 ms, browser agent). That includes the SHA-1 check, extraction and app start.
- **Sizes:** level GLB 808,364 bytes; character GLB 12,481,696 bytes. Only 7.2 MB of the character GLB is binary;
  about 5.3 MB is JSON for 764 animations × 28 joints. It did not slow loading in Chrome (below).
- **Gaps:** see the wave-1 extractor sections (segment-8 render state, mip levels, surface meanings).
  `conker-extract textures` still names and deduplicates PNGs through a pseudo-`Level`.

#### conker-server (integration agent), 312 lines
- **Works:**
  - Authoritative at 30 Hz on scene 0x2D's collision, built from the operator's ROM.
  - Joining players get spawn record 1 first, then the others nearest-first, mid-match too. **Wave 3:** slots take the
    game's Deathmatch start records (1, 0, 13, 6, …) with its jitter, and respawns use the game's picker
    (conker-rules).
  - One server entity carries both WebTransport and WebSocket (wave 1's relink workaround).
- **CLI:** `conker-server [--rom PATH | CONKER_ROM] [--port N] [--digest-file PATH] [--seconds N] [--spawn X,Y,Z]`.
  - `--spawn` moves the first player's spawn, for oracle replays. **Wave 3:** `--spawn X,Y,Z[,FACING]` is
    repeatable (one per slot); `--give`, `--wait`, `--margin`, `--first-read` and `--no-lagcomp` are new.
  - `RUST_LOG=conker_server=debug` prints a `STATE t= p= pos= st= fy=` line per player and tick.
- **Scripts:** `smoke.sh [wt ws ws]` and `replay.sh <script> <trace> [seconds]`.
- **Gaps:**
  - ~~Spawn order is assumed (I): record 1 first is measured, the rest is not, and records are reused round-robin
    after 20 joins. The game's picker (`func_150ADA68`) and its jitter are unread.~~ **Wave 3:** the picker is
    `func_15084D70` (`func_150ADA68` is the RNG's `random_float`), ported bit-exact in conker-rules and used by the
    server.
  - The netcode timeout is 10 s (carried in the connect token), so a player who quits stays about 10 s.
    (Wave 5: 3 s, and a quitting client disconnects at once.)
  - It binds IPv4 0.0.0.0 only, so WebTransport to `localhost`/`::1` hangs. (Wave 5: binds `[::]`.)
  - Dev-only security, unchanged: zero key, self-signed 14-day certificate, plain `ws://`. (Wave 6: the server signs
    every token with its own key; `--cert` gives a real certificate and `wss://`.)

#### conker-client and `web/` (client-assets, integration and browser agents), 1,362 lines
- **Works:**
  - Renders scene 0x2D and character 117 from the user's ROM, natively and in the browser.
    - The two GLBs go through an in-memory `rom://` asset source and Bevy's glTF loader. Samplers, alpha, unlit and
      double-sided come straight from the extractor.
    - The only added material code converts vertex colours to linear.
  - Prediction runs the same `movement::step` on the client's own `CollisionWorld`. Remote players are interpolated.
  - Every player is animated by conker-anim (`AnimPlayer`, clip per logical id = state).
  - The view and the `camera_yaw` sent with each input both come from conker-camera.
  - Measured (V):
    - Player 1 stands exactly on record 1, facing the record's direction.
    - Textures are upright ("DANGER" poster).
- **Config** (CLI `--key value` or URL `?key=`): transport, server, digest, id, script, headless, seconds, rom,
  screenshot, view, position.
  - `script` with an empty value runs a circle test pattern; with a path it runs an oracle script (native only).
  - `view` starts the free camera at an eye and look-at.
- **API:**
  - `run(cfg: Config, slice: Slice, script: Vec<ScriptLine>)`. The slice is no longer optional.
  - The wasm build exports `start(rom: Uint8Array)`, which throws the error string.
  - `page_server(host) -> "host:5888"` (`localhost` → `127.0.0.1`; Wave 5: the host unchanged). `query_pairs`
    percent-decodes.
  - Input and scripts: `parse_script`, `script_input`, `circle_input`, `device_input`.
  - `FollowCamera { follow, since }`, `Anim(AnimPlayer)`, `Clips` (id → glTF animation and loop length),
    `ANIM_IDS`, `clip_for(&[ClipInfo], logical)`.
  - Constants `STICK_MAX` 80, `STICK_WALK` 41, `C_UP`/`C_DOWN`/`C_LEFT`/`C_RIGHT`, `FIRST_READ` 23.
  - `render::{build, model_yaw, shade_to_linear, screenshot_then_exit, FIRST_SPAWN, MODEL_SCALE = 0.5,
    DEBUG_CAMERA_KEY}`.
- **Input:**
  - Keyboard: WASD or arrows give a full stick (80), Alt walks at 41, Space = A, Shift = Z, C toggles the free camera.
  - Gamepad: left stick × 80, South = A, left trigger = Z, right stick = C buttons. The C buttons only hold the
    camera's yaw, since strafing is not simulated.
  - **Wave 3:** replaced by the combat mapping (the controls table under "Integration" in the wave-3 section): B, Z on
    the left mouse/F/Shift, R on the right mouse, G for C-down, Q/E strafe.
- **wasm** (`--profile web`, shipped `web/dist/client_bg.wasm`, re-measured here):

  | Stage | Raw | gzip -9 |
  |---|---|---|
  | rustc output | 29,421,031 B | 8,100,970 B |
  | after wasm-bindgen | 26,457,609 B | 6,741,779 B |
  | **after `wasm-opt -Oz` (shipped)** | **21,437,158 B** | **6,738,427 B** |
  | wave 1, after wasm-bindgen | 23,636,661 B | 5,962,255 B |

  - `client.js` is 124,110 B (17,455 B gzip).
  - wasm-opt cuts 19% raw but only 0.05% gzipped. `build.sh web` runs it when `WASM_OPT` is set or `wasm-opt` is
    on PATH.
  - A cold web build takes about 5 min; relinking about 3.5 min.
- **Gaps:**
  - No message on the page when a connection fails or never opens. (Wave 5: the `#net` status line.)
  - No disconnect is sent on exit or `pagehide`. (Wave 5: sent on a native exit; a closed page closes its transport.)
  - Inputs sent while a windowed client stalls at startup (~8 s in debug) are dropped. That gave 2 rollbacks in the
    first pair attempt, until the pair scripts waited. (Wave 5: the client connects only after loading and 10 calm
    frames, and the server holds a new player until its first input.)
  - Assumed render values (I): FOV 50°, near 4, far 40,000, fog 8,000-25,000, the sky colour, a 2,500 lux
    directional light, 400 ambient, and the sRGB→linear vertex-colour conversion.
  - The level's duplicates (segments 12/13, six model-51 planes) are not filtered.
  - The `rom://` source keeps ~13 MB of GLB bytes alive.
  - The macOS debug binary triggers the linker note `__eh_frame section too large`.

### Browser (browser agent; Chrome 153, macOS, M2 Max, WebGL2 through ANGLE/Metal)
| | Result |
|---|---|
| WebTransport (certificate-hash flow, server `--digest-file web/dist/digest.txt`) | **connects**; plays; 0 rollbacks |
| WebSocket (`?transport=ws`) | **connects**; plays; 0 rollbacks |
| Two pages, one per transport | each saw the other player move and animate, with 0 rollbacks on every page in 3 sessions. Session 1: the ws page saw the wt player go idle 15 → run 2 → skid 33 → idle, and the wt page saw run 2 → wall push 122 → idle. Session 2: walk 1, run 2, jump 50, land 54, skid 33. The browser report calls 1 "creep" and 54 "crouch"; by conker-anim's ids they are walk and land |
| Same input as the native pair | player 1 ended at (-2199.42, -521, 6043.29), exactly where the native pair run ended |
| Startup | ROM picked → client running: 1.8 s (3.3-4.7 s while cargo used the CPU). Both GLBs loaded 2.1-2.5 s after app start. The wasm downloads in 167 ms from localhost |
| Wrong ROM | the page shows the SHA-1 mismatch and the file input is re-enabled |
| Console | only Bevy WARN lines for plugins WebGL2 cannot run (OIT, SSAO, atmosphere, motion vectors) |
| Firefox, Safari 26.4+ | **not run**: a manual checklist and the known limits are in `web/README.md` |

Fixed during testing:
- The server address now defaults to the page's host. `localhost` is mapped to `127.0.0.1`, because WebTransport to
  `localhost` hung (I: Chrome tries ::1 for QUIC).
- URL options are percent-decoded; `URLSearchParams` had turned `:` into `%3A`.
- `digest.txt` is not fetched for ws.
- A favicon was added.

Not tested: background-tab throttling. When `server` is a hostname, the netcode token's address is `0.0.0.0:0`. That
only works because the dev server has `server_addr_check` off. Production needs an IP or `additional_expected_addresses`.
(Wave 6: the server issues the tokens, naming its `--public` addresses, and checks them.)

### Screenshots (`../lab/screens/wave2/`)
| File | Shows |
|---|---|
| `browser-p1-wt-spawn.png` | Chrome, player 1 over WebTransport at spawn record 1, from behind |
| `browser-p1-wt.png`, `browser-p2-ws.png` | Chrome, each player's view (WT and WS), the other player in view facing it |
| `browser-p1-wt-sees-p2-run.png` | Chrome, player 2 in the run pose, seen from player 1 |
| `client-browser-chrome.png` | the client agent's first Chrome run (ws) |
| `pair-p1.png`, `pair-p2.png` (+ `pair.sh`, `pair_p*.txt`) | native windowed pair through the server, each from behind its player with the other in view (26 ms apart) |
| `client-follow-camera.png`, `client-two-players.png`, `client-overview.png` | native: wave-2 client before integration (spawn courtyard, two players, the whole map) |
| `client-character-front.png`, `client-character-side.png` | model 117 close-ups (feet at y = -521, facing record 1's direction) |
| `client-idle-anim-3-phases.png` | three moments of clip 122. The client agent took it for idle; it is the wall push (see the extract-character note below) |
| `client-texture-danger-poster.png` | texture 1667 upright and unmirrored |
| `client-vertex-colour-ab.png` | vertex colours with and without the linear conversion (camera not identical) |

### Honest gaps (cross-cutting)
**Wave 3:** the first five items and the spawn/surface item are closed (all 28 traces within 0.001 in both modes;
walls, combat, a HUD, spawns and surfaces exist). The move_wall6 hit is the bot's sniper (type 0x12, 1 damage), and its
knock-back is the general rule `s' = 0.6·(s − 0.1·k)` with k = 1. Modes, bots and audio are still missing. The rest
stands; the wave-3 gaps list is current.
- **With conker-camera in the loop, 9 of the 26 traces stay within 0.1 units:** idle, walk, run, turn, jump,
  camera_circle, camera_swing, anim_idle_long and move_jump. With the trace's camera, 11 do.
  - `camera_loop` asserts 7 of these 9. The integration report's "closed loop exact" covers those 7, not every trace.
  - Elsewhere a one-step flip of the camera's 256-step turns sub-unit errors into 14 to 59 units (move_wall3 13.7,
    move_crouch 57.7, camera_jump 58.8), and 3,225 once a wall push is involved (move_wall2).
  - Server and client run this closed loop, so players agree with each other exactly, but not always with the
    original's path.
- **Walls** (sim and camera) are the main fidelity gap. They also block jumping off a wall push, and every combat trace
  will touch them.
- **Nothing of combat, modes, bots, HUD or audio exists yet.**
- Player 1's health stays 6 in every trace but one. In `move_wall6`, at read 730, player 1 is standing idle against
  the x = −3282 wall at (-3282.7, -521, 6057.8) when its health drops to 5. Meanwhile the bot is about 10,200 units
  away in states 87 → 86. This is the first damage in any trace, and a lead for combat (I: a projectile from the bot).
  The knock-back, read from the trace (V on this one event):
  - state 29 for 18 frames (reads 730-747), then idle
  - speed 20 along move angle 44933, then speed' = 0.6·speed − 0.06 each frame (20 → 11.94 → 7.104 → 4.2024 → …)
  - the facing turns 3072 per frame
- Spawn semantics and surface meanings are still unknown (I).
- 3-VI frames are untested everywhere (movement, camera, animation): all traces are pinned at 2 VIs.
- Firefox and Safari are untested. So are deployment, real certificates and the production netcode settings.
- `../RUST_REMAKE_PLAN.md` still says "wave 1 done", and `../lab/oracle/README.md` still has the −256, stop-snap,
  slope and jump-velocity statements corrected above. Both are outside this tree's wave-2 write scope. **Wave 3:** the
  plan now says "wave 3 running"; the oracle README is still not updated (outside wave 3's scope too).

### Wave 3, ranked
**Wave 3 outcomes:**
1. Walls: **done** (bit-exact routine, heading, sweep and ray).
2. Camera loop: **done**, beyond the target: every trace within 0.001, wall traces included.
3. Combat: **done except** the flamethrower, weapons 0x14/0x16 and lag compensation with two shooters in one tick.
   The encounters used two scripted humans instead of the bot. The locomotion ids 0x2B and 0xDF named below do not
   appear in wave 3's reports or in conker-anim; the weapon ids actually traced are in its `combat` module.
4. Surfaces and spawns: **done**; the class-6 slide is described, not ported. The picker is `func_15084D70`, not
   `func_150ADA68`.
5. Controls: **partly**. Done: C-left/C-right with the stick centred, R's brake and aim camera, A from a wall push.
   C-down is in conker-rules, except its 1.09-unit move. Not done: the blocked-view cut, A from landing and crouch
   (untraced), creep root motion, the bracketed thresholds.
6. Bots: not started.
7. Netcode robustness: only "spawn per joined slot" (the game's start records by slot).
8. HUD: a text HUD (health, weapon, rounds, grenades, kills, clock, respawn countdown). No lobby or spectators.
9-12. Not started; Chrome was re-tested with combat.

The list as written in wave 2:
1. **Port the game's wall routine.** `func_15044380` → 0x150AB1F0, helpers 0x150492CC, 0x150AC1C4, 0x150AAA20,
   0x150ABD20, 0x150AC344.
   - Re-enable `ledge`, then check move_wall1/2/3/5/6, move_turnrun and camera_wall.
   - Produce the wall-aligned heading for the camera in state 122.
   - Then port the camera's sphere sweep (the same mover), and its wall ray once the cam+0x84 bit 0x40000 rule is
     traced.
2. **Close the camera loop.** Model when in the frame the game reads the camera near a 256 boundary, and the airborne
   facing. Target: `report_all`'s "(camera)" rows within 0.1 on the non-wall traces.
3. **Combat, traced from the oracle**: weapons and pickups, health and damage (starts at 6), death and respawn, score
   (not wave 1's guessed field), the match timer, lag-compensated hits.
   - New scripted encounters with the bot. Start from `move_wall6` read 730: the only hit traced so far.
   - The weapon locomotion ids 0x2B and 0xDF in conker-anim.
4. **Collision surfaces and spawn semantics.**
   - Surface words, read from their consumers (`func_150E1D14`, `func_150450CC`, `func_15175958`), with oracle runs
     onto 0x9/0xA faces.
   - The spawn picker `func_150ADA68`: its jitter and record choice, per player and on respawn.
5. **Remaining player controls:**
   - C-down (states 118/119/107), C-left/right strafing, R's brake and aim camera, the blocked-view cut.
   - A from wall push, landing and crouch, and creep root motion (clip 0's root z at runtime).
   - The bracketed thresholds, measured with targeted traces.
6. **Bots** (server-side), compared qualitatively, then by statistics (kills/min, paths).
7. **Netcode robustness:**
   - Dual-stack `[::]` bind; a disconnect on exit and `pagehide`, then back to a 3 s timeout.
   - Gate input on asset loading; a status line on connection failure.
   - Spawn per joined slot.
8. **HUD and lobby**: score, timer, health, the join flow, spectators.
9. **Audio**: MP3 voices, then the sound banks.
10. **Deployment**: a VPS with real certificates, a non-zero netcode key, `wss://`, brotli for the wasm, and a
    hostname-safe connect token.
11. **Browsers:** Firefox and Safari 26.4+ (`web/README.md` checklist), and background tabs.
12. **Fidelity details:** 3-VI frames, the render-only camera offsets, segment-8 render state, fog and sky from the
    game, the level duplicates.


## Wave 1 integration run (2026-10-08, rustc 1.98.1)
Wave 1's numbers, kept for comparison. Wave 2's re-run is at the top.

| Check | Result |
|---|---|
| Extractor tests: 6 crates, with ROM and all three DOS env vars | **81/81 pass** (56 unit + 25 ROM), 30.3 s wall in debug, 18.8 s of it the `textures` test. `--nocapture` shows no test skipped; the DOS texture check compared 1,272 textures |
| Same, without env | 81/81 pass: the 25 ROM tests return early (24 print the skip line) |
| `cargo check --target wasm32-unknown-unknown --lib`, 6 extractor crates | clean |
| `CARGO_TARGET_DIR=target-net cargo check --workspace --all-targets` (all 9 crates) | clean, 0 warnings |
| Net tests | conker-sim 7/7 (8/8 with `net`), conker-client 4/4, conker-server 0 (covered by smoke.sh) |
| `smoke.sh wt ws` | 6/6 ok: both clients connect; the server applies both players' inputs (14 distinct positions each); each client sees the other move (14 each). No processes left running |
| `conker-extract slice` (debug) | 153 files, 14,005,218 bytes, 2.3 s |
| Slice vs the outputs the decoder agents verified (`../lab/extract/{scene,level,character}`) | **byte-identical**: `level-2d.gltf`/`.bin`, all 144 PNGs, `bank01-0117.gltf`/`.bin`, `collision.obj`. Equal JSON for collision (every key), spawns (20/20), placements (46/46) and clips (766 pairs) |
| `conker-extract list` before and after the CLI rewrite | byte-identical (32 lines) |
| clippy and fmt on conker-extract | clean |

## Crates (wave 1; wave-2 changes marked)
### extract-core: containers (scaffold agent)
- **Works:**
  - ROM byte-order normalisation (z64/v64/n64) and the US SHA-1 check.
  - The asset table, bank tables and nested tables (`Rom::asset(&[bank, index, sub..])` decompresses each step).
  - rzip.
  - `.game` code and data.
  - The texture blob, where texture id = blob index.
- **Evidence:**
  - 30 top entries; 29 banks with entry counts equal to the probes' `out_bank_stats.txt`.
  - 1,670 compressed entries all inflate to their stated size; first-level payloads total 60,507,058 bytes.
  - 7,762 textures, 2 of them empty.
  - `.game` code is 2,072,880 bytes, and the flat image rebuilt from it has the expected SHA-1 (`842e3d34…`).
  - 7 unit tests + 6 ROM tests.
- **Gaps:**
  - `Table::parse` does not require bit 31 on the last entry. All 30 tables have it.

### extract-gbi: F3DEX2CBFD display-list walker + N64 texture decoder (gfx agent)
- **Works:**
  - Conker's G_VTX, G_TRI1/2, G_QUAD, G_TRI4, CBFD normals, matrices, DL call/branch, and the texture, tile and othermode state.
  - Flat texture references (id + mode).
  - Decoding RGBA16/32, CI4/8 (RGBA16 or IA16 palette), IA4/8/16 and I4/8, including TMEM-interleaved rows.
  - PNG output.
- **Evidence:**
  - Triangle counts equal DOS's: bank 03 = 3,893 + 1 degenerate G_TRI4 slot = 3,894; bank 04 = 147,621 + 102 = 147,723 (765 models, 209,274 vertices); bank 01 = 62,073 (183 models, 379 lists).
  - No opcode left uninterpreted in any of the three banks.
  - 1,272 textures are within 1 per channel of DOS's PNGs.
  - Format and size agree with DOS for 966 textures. The 10 that differ are explained (see cross-agent notes).
  - 18 unit tests + 5 ROM tests.
- **Gaps:**
  - Segment-8 render-state calls are unresolved: 61 in bank 03, 60 in bank 04, 1,134 in bank 01.
  - A texture is only found when its TMEM address is exactly the start of a load, so mip levels and second tiles get no image.
  - Lights, G_MOVEWORD and G_BRANCH_Z are not interpreted. None occur in banks 01/03/04.
  - Bank 09 is not walked.
  - **New in this run:** `conker-extract textures` gets 3 decode failures, where the computed size exceeds the payload (all outside the slice):
    - a draw in model `[03, 0x26]`: needs 4,096 bytes, the payload has 2,560
    - a draw in model `[03, 0x5A]`: needs 256, has 160
    - a draw in segment `[04, 0x05, 0]`: needs 4,096, has 2,560
    - ~~Probably (inferred, not checked): 2,560 bytes = 2,048 texels + a 512-byte palette, a 64×32 CI8 payload that the walker sizes as 64×64.~~
    - **Wave 2, measured from the raw commands: not a decoder bug.** The display lists themselves read past their payloads, so these textures stay off their materials:
      - `[03,0x26]` and `[04,0x05,0]` load 2,048 RGBA16 texels for a 32×64 RGBA16 tile (4,096 bytes) from a 2,560-byte CI8 32×64 + 256-colour palette payload.
      - `[03,0x5A]` loads 128 texels for a CI8 16×16 tile, plus a 256-entry TLUT, from a 160-byte CI4 16×16 + 16-colour payload.
- **Wave 2:**
  - extract-gbi is now the only texture decoder: `decode`, `png`, `Material::texture_png(|id| payload)` and `glb`.
  - PNGs are upright (rows reversed from memory order, v = 1 − t/height).
  - The tests compare DOS's PNGs after flipping DOS's rows back, so DOS also writes this art top row first.

### extract-scene: placements, spawns, collision (scene agent)
- **Works:**
  - 0x44-byte placements with their transform and collision flag.
  - The MP spawn table `[0x0C, scene, 6]`, 0x18-byte records. No 0x44 placement is a spawn.
  - Terrain and secondary collision meshes with their surface words; placement collision; `floor_below`.
- **Evidence:**
  - 727 + 511 placement records.
  - 96 collision meshes and 98,479 triangles. Every mesh's triangle records and surface words have the same SHA-1 as DOS's.
  - Scene 0x2D:
    - 46 placements (12 from `[0B,2D]`, 34 from `[0C,2D,2]`), all adding collision.
    - 20 spawns, all within -1.24..+4.0 units of a floor.
    - Collision faces: 1,751 terrain + 8 secondary + 2,540 from placements = 4,299.
  - The spawn table in oracle RAM equals the ROM bytes.
  - 8 unit tests + 3 ROM tests.
- **Gaps:**
  - Surface-word meanings are unknown. Scene 0x2D has 8 words: 0x5 ×865, 0x4 ×378, 0x9 ×240, 0x0 ×176, 0xA ×38, 0x1 ×22, 0x3 ×20, 0x60000005 ×12.
    **Wave 3:** decoded in conker-rules (`surface`): kind = bits 0-4 (only 0xA has a sink/speed entry on 0x2D), slide
    class = bits 28-30 (the 12 faces of 0x60000005).
  - Spawn bytes +6, +0x14 and +0x15 are unknown, and so is the picker's RNG (`func_150ADA68`). **Wave 3:** +6 →
    obj+0xB8 in degrees, +0x14 never read, +0x15 → struct126+0x23 (consumers untraced). The RNG and the picker
    (`func_15084D70`; `func_150ADA68` is `random_float`) are ported in conker-rules.
  - `floor_below` is a linear scan, too slow for per-tick use. It also uses exact planes, where the game uses integer
    math: 0.108 off on a slope in camera_jump. **Wave 2:** per-tick floors use conker-sim's `CollisionWorld`
    instead, a 512-unit grid with the game's fixed-point routine, bit-exact on 9,160 of 9,262 traced frames.
  - Wave 2, surface words met by the player:
    - Floors 0x5, 0x3 (the ramp) and 0x4 (the plateau).
    - Walls 0x5, 0x4, or none (placements).
    - Meanings are still unknown.
  - Scene 0x1D's segment 0 (234 faces, no surface table) is kept here; DOS leaves it out.

### extract-level: level glTF (level agent)
- **Works:**
  - Bundle `[04, scene]`: slots 0-2 drawn in place, slot 3 collision-only.
  - Every placement's model drawn at its transform.
  - glTF 2.0 with deduplicated materials, PNG textures, `KHR_materials_unlit` for draws without normals, and the raw RDP state in material extras.
- **Evidence (scene 0x2D):**
  - 48 instances of 27 models; 4,298 faces after zero-area faces are dropped.
  - 144 PNGs; buffer of 150,684 bytes.
  - Per model: face counts and bounding boxes equal DOS's for all 27, and texture ids equal per face. Vertex counts differ for 1 model, which is dedup policy.
  - The Khronos validator reports 0 errors and 0 warnings.
  - x/z extents equal the collision's.
  - 8 unit tests + 4 ROM tests.
- **Gaps:**
  - Lighting is a heuristic (lit only where vertices have normals).
  - 47 materials call segment 8, so their othermode may be stale.
  - Not handled: combiner formulas, fog, rooms/portals, vertex-colour animation.
  - Segments 12 and 13 sit at the same transform, and six model-51 planes are exported. Which of these show in multiplayer is unknown; node extras carry the model path and placement index so a client can filter.
- **Wave 2:**
  - `Gltf::glb()` builds the level as one GLB in memory, with the PNGs moved into buffer views. Its texture decoding
    now goes through extract-gbi.
  - The Khronos validator on `level-2d.glb` reports 0 errors and 0 warnings (3 non-power-of-two infos).

### extract-character: rig, skin, clips (character agent)
- **Works:**
  - The bank-01 model: header, 28 joint records, both display-list tables.
  - Skinned glTF with inverse bind matrices.
  - The bank-02 clip decoder, ported from DOS: bitstream, Euler angles, 30 Hz keys.
  - Bank-0x0F routes to logical ids, and the 0xFF duration overrides.
- **Evidence:**
  - Model 117 (the one every Deathmatch participant uses, read from oracle RAM): 28 joints, 160 vertices, 172 triangles, 4 textures, 764 animations.
  - Positions are bit-exact against DOS.
  - All 1,770,276 animation values equal DOS's (maximum difference 0).
  - Whole bank 02: 2,621 clips and 57,732 frames, DOS's totals.
  - The Khronos validator reports 0 errors.
  - 13 unit tests + 5 ROM tests.
- **Gaps:**
  - Clips carry pair and logical ids, not names like idle/run/jump. ~~In the oracle, 122 is player 1 idle and 2 is the bots' (probably run).~~
    - **Wave 2 correction (V, conker-anim):** the logical id is obj+0x84, and the movement state is its low byte on
      all 10,620 traced rows. Idle is **15** (52 ticks), run 2 (28 ticks), and **122 is the wall push** (43 ticks).
    - The client agent's interim "idle" loop and its screenshot `client-idle-anim-3-phases.png` played clip 122. The
      integrated client maps every state to its clip.
  - glTF clips end at (duration-1)/30 s; loop on `durationTicks` instead. The clock wraps at the duration, so the
    last tick interpolates last key → first key (I in the renderer).
  - ~~Root motion is baked into the root channel, and whether the game applies it is unknown.~~ **Wave 2 (V):** the
    game moves the player by speed, except in creep (state 0), which moves by 0.5 × clip 0's root-z advance two frames
    late. Renderers drop root x/z and keep root y.
- **Wave 2:** textures decode through extract-gbi, and the PNGs are upright. On `character-117.glb` the Khronos
  validator reports 0 errors and 0 warnings (28 hints).
  - Runtime face and eye textures (segments 6/7/10/11) are untextured. Model 117 has none.

### conker-extract: library + CLI (this integration)
- **Works:**
  - `conker-extract [--rom <path>] [--out <dir>] list | textures | scene <id> | level <id> | character <name|id> | slice`.
  - Ids are decimal or `0x` hex; character names are `deathmatch` (117) and `conker` (0).
  - `--rom` falls back to `CONKER_ROM`. `--out` defaults to `../lab/extract/slice`, resolved at compile time from the crate directory, so output never lands in this tree.
  - The library returns `(relative path, bytes)` lists and only the binary touches the file system; the library checks for wasm32.
- **Evidence:**
  - The slice is byte-identical to the decoder agents' outputs (see above).
  - Other subcommands run on the US ROM:
    - `level 0x1A`: 16 instances, 75 PNGs.
    - `scene 0x1A`: 14 placements, 32 spawns, 2,844 faces.
    - `character conker`: 764 animations.
    - `character 90`: 70 animations, DOS's count.
    - `textures`: 2,894 PNGs (2,894 of 7,762 ids drawn by banks 01/03/04, 1,221 lists, 3 warnings) in 2.5 s release, 18.8 s debug.
  - Bad arguments exit 1 with the usage text.
  - 2 unit tests + 2 ROM tests: one pins the slice counts, the other the `textures` counts (2,894 files, 0 rejected, 3 warnings).
- **Gaps:**
  - `textures` reuses extract-level's private decoder by putting every model in a pseudo-`Level` and keeping only the images. **Wave 2:** decoding is in extract-gbi now, but the PNG naming and deduplication still go through the pseudo-`Level`.
  - Textures drawn only by zero-area faces are skipped.
  - No audio.
  - ~~No GLB output.~~ **Wave 2:** the in-memory `Slice` API (both GLBs, collision, spawns, clips) and `examples/slice_glb.rs`; see the wave-2 section. 4 new tests (85 in all).
  - If the checkout moves, pass `--out`, since the default path is compiled in.

### conker-sim, conker-server, conker-client: networking skeleton (net agent)
Wave 2 replaced the placeholder sim and most of the client; their current state is in the wave-2 section. Kept here:
the transport findings.
- **Works:**
  - Headless Bevy 0.19.1 + lightyear 0.30.1 server, authoritative at 30 Hz.
  - Native and WebGL2 wasm clients, with the local player predicted and remote players interpolated.
  - Keyboard and gamepad input.
  - `--headless --script` client mode, for tests.
  - Transport chosen with `--transport wt|ws` or `?transport=`.
- **Sim:**
  - `conker-sim` ~~is a **placeholder**~~ was wave 1's placeholder: a capsule on the plane y = 0, in metres (6 m/s walking, 1.8 m capsule).
  - The extracted world and the oracle are in game units. For example, player 1 spawns at (-2267, -521, 6016) and a full run moves 17.5 units per frame.
  - Wave 2 replaces `step`, switches to game units, and bumps `PROTOCOL_ID`. **Done in wave 2:** the movement port
    (`movement::step`) runs in game units, and `PROTOCOL_ID` is 2.
- **Bug #1693 (WebTransport + WebSocket):**
  - **One server entity carrying both transports, stock lightyear 0.30.1: FAIL.** The cause is not #1693 itself. lightyear_aeronet links each transport through a one-to-one relationship, so Bevy 0.19.1 evicts the first transport's link and WebSocket clients never connect.
  - **With the 20-line `relink_evicted_session` workaround: PASS.** wt+ws, ws+ws and wt+wt each pass 6/6, re-run here for wt+ws.
  - **Two server entities (#1693 proper): not run.** The bug is still present by inspection (`lightyear_inputs-0.30.1/src/server.rs:681`).
  - The workaround pins `lightyear_aeronet =0.30.1` and `aeronet_io =0.21.0`.
- **wasm size** (measured from the build artefacts here, not rebuilt):

  | Build | Raw | gzip -9 |
  |---|---|---|
  | `--release`, after wasm-bindgen (`web/dist/client_bg.wasm`, 37.6 MB of it is the name section) | 79,522,950 bytes | 12,014,506 |
  | `--profile web` (opt-level s, fat LTO, strip), rustc output | 26,479,967 | 7,247,237 |
  | The same after wasm-bindgen | 23,636,661 | 5,962,255 |

  `wasm-opt` was not applied (not installed). **Wave 2:** after `wasm-opt -Oz`, 21,437,158 B raw and 6,738,427 B gzip;
  see the wave-2 client section.
- **Gaps:**
  - ~~Never run in a browser.~~ **Wave 2:** it runs in Chrome 153 over WebTransport (certificate hash) and `ws://`,
    with 0 rollbacks. Firefox and Safari are still untested.
  - Dev-only security: zero netcode key, self-signed 14-day certificate, plain `ws://`.
  - ~~The wasm client's default server address is 127.0.0.1 rather than the page's host.~~ **Wave 2:** it defaults
    to the page's host on port 5888, with `localhost` mapped to `127.0.0.1`.
  - The gamepad was detected but not exercised. Still true in wave 2: the mapping is unit-tested only.

## The slice (`../lab/extract/slice`, from `conker-extract slice`)
| Path | Contents |
|---|---|
| `scene-2d/placements.json` | 46 placements: position, rotation (degrees), scale, dispatch, `model_path` (`[3, m]` or `[4, 0x2D, seg]`), flags, collision, raw record hex |
| `scene-2d/spawns.json` | 20 MP spawn records: position, yaw (`facing_u16 = yaw << 8`), initial camera eye, unknown bytes, floor under each (triangle, height, surface) |
| `scene-2d/collision.obj` / `.json` | 4,299 world-space triangles in build order (terrain, secondary, placement runs); per-face surface word (terrain only); histogram |
| `level-2d/level-2d.gltf` + `.bin` + `textures/*.png` | 48 nodes, 27 meshes, 144 PNG **files**. Wave 1 wrote rows in memory order, so the art looked upside down in a viewer (UVs mapped straight). **Since wave 2 the PNGs are upright with v = 1 − v.** The copy in `../lab/extract/slice` is still wave 1's; `../lab/extract/wave2-check/slice` is the wave-2 output |
| `character-117/bank01-0117.gltf` + `.bin` | skinned mesh, 28 joints, 764 animations; its 4 textures are **PNG buffer views inside the `.bin`**, not files |
| `character-117/clips.json` | per glTF animation: pair, logical ids, frames, duration in ticks (loop on this) |

Player 1 spawns from record 1 (+16 along its facing); the bot spawns from record 0 (-36). Both stand on surface 0x5 at
y = -521, matching the oracle's `floor_y`.

**Wave 2:** the client no longer reads these files. It calls `conker_extract::Slice::from_rom` in memory. The same
GLBs are in `../lab/extract/slice-glb` (`examples/slice_glb.rs`): level 808,364 B, character 12,481,696 B.

## Oracle traces (`../lab/oracle/`, README there)
- **Setup:** the original game (headless recompiled build) runs Deathmatch: scenario 3, scene 0x2D, player 1 + 1 bot.
- **Input:** `CONKER_INPUT_SCRIPT` drives controller 0. The index is the game's controller read, so read N = game frame N.
- **Output:** `CONKER_TRACE_PLAYERS` writes one CSV row per read and player.
  - Row N holds the input fed at read N and the state after frame N-1.
  - Columns: position, y_vel, gravity, xz_speed, target_speed, move_angle, facing, state, floor_y, anim_speed, health, camera eye and look-at.
- **Timing:** the match goes live at read 23 and the scripts start input at read 53.
- **Pinned and repeatable:** `CONKER_FIXED_DT=1`, constant 30 Hz. Every a/b pair is bit-identical in all game columns, for both players.

| Trace (`trajectories/<name>_{a,b}.csv`) | Reads (a / b) | Player 1 |
|---|---|---|
| idle | 241 / 241 | stands still (path 0.0) |
| walk | 271 / 270 | half stick (game 34): speed 20.4, 851.5 units |
| run | 271 / 270 | full stick (game 75): speed 35 after 35 frames, up a ramp, skid (state 33) |
| turn | 361 / 360 | 8 sideways taps: 9216 (50.6°) per tap |
| jump | 301 / 300 | tapped A peaks +166.8, held A (10 reads) peaks +208.1 |
| ledge | 511 / 510 | wall slide (state 122), walks off at read 409, falls ~370 units, lands at read 425 |

**Movement rules measured** (2 VIs per frame):
- **Horizontal:** `pos += 0.5·xz_speed·(cos a, −sin a)`.
- **Speed:**
  - Accelerates +1 per frame toward the target (0.6 × game stick, capped at 35) and decelerates −2.5 per frame on release.
  - Walking goes +0.01 per frame above 20.
- **Airborne:** `y += 0.5·y_vel − 0.275·gravity`, then `y_vel −= gravity`.
  - A jump starts at y_vel 55; gravity is 4 while A is held, then 6.2. (Wave 2: the first airborne step acts as
    y_vel 58.999985.)
  - Falling off a ledge uses gravity 5, and landing zeroes the speed.
- **Turning:** at most 6400 per frame for the move angle and 3072 for facing.
- **Camera-relative stick:** target move angle = camera yaw + atan2(stick_y, stick_x) − 16384 − 256.
  ~~The −256 is unexplained.~~ ~~**Wave 2 (V):** the camera yaw is quantised down to 256 steps, `(yaw & 0xFF00) + stick − 16384`, and
  the spawn camera sits at 16895.996, so the "−256" is that rounding.~~ **Wave 3:** the camera is read at full u16
  precision and the move angle turns by whole 256-steps of the difference, keeping its low byte (see "Rules found in
  wave 2").
- **Deadzone:** |raw| < 8 → 0, otherwise raw − 7·sign(raw).
- **Camera:** look-at = position + 67.5 in y; eye about 247 behind and about 100 up. **Wave 2:** ported exactly
  (conker-camera).
- **Movement state** values seen: 15, 0, 1, 2, 27, 33, 50, 54, 55, 56, 122 (meanings in the README). Wave 2 adds 22
  (crouch), and 118/119/107 (C-down). The state is the low byte of the animation id.

Not measured: how speed and gravity scale when a frame takes 3 VIs, and the source of the drifting stick that player 1
reads when no script is loaded.

**Wave 2 traces**, all pinned, with player 1 against 1 bot:
- `move_*`, 11: speeds, release, jump, crouch, slope, turnrun, and walls 1-3, 5 and 6. Five of them have raw dumps.
- `camera_*`, 6: circle, swing, jump, wall, buttons, ledge.
- `anim_*`: re-runs of the six with raw dumps, plus idle_long (70 s), jump_stand, crouch and creep. Ten of them have
  lldb clock and call captures.
- The wave-1 README's rules are corrected in the wave-2 section ("Rules found in wave 2").

## Cross-agent notes
- **"Player 1 moves in the dm3 runs"** (scene agent): explained by the oracle. With no controller answering, the game
  reads a forward stick drifting from 80 to 67, so the old `lab/work/mp/dm3*` runs are not idle. Use `scripts/idle.txt`.
- **Model 117 plays Conker's clips:** the reference agent had only inferred this. The character agent traced it in
  RAM: byte +6 = 0 makes `func_1505E0C4` load `[02, 0, ..]`.
- **DOS texture numbering:** DOS numbers textures by stream position, skipping the empty ids 1767 and 1768, so DOS
  entry N ≥ 1767 is game id N + 2.
  - Anyone comparing against `../lab/reference` textures must apply this.
  - DOS's format and size for 2261, 4425, 7225 and 7226 describe the texture two ids lower.
  - For 737, 1260, 3994 and 4000, DOS assumes 64×64 where the display lists draw 64×32 or 128×32.
- **Segment-8 render state is a shared gap:** it is unresolved in all three geometry paths (bank 03: 61 calls, bank 04: 60, bank 01: 1,134; 47 level materials).
  - It affects alpha mode and filtering.
  - It needs an oracle display-list capture or a reading of the code that binds the segment.
- **Texture decoding exists twice, privately:** (**resolved in wave 2**: one decoder in extract-gbi, used by both)
  - `texture_png` in extract-level and in extract-character, and conker-extract borrows extract-level's.
  - Consolidate it into extract-gbi, e.g. a `Material` method taking a texture lookup closure, since extract-gbi does not depend on extract-core.
- **Two collision readers:** `extract_scene::parse_mesh` (triangles only, mirrors the game's collision builder) overlaps the extract-gbi walker. Merge them only if a difference shows up.
- **Vertex counts differ from DOS, triangles do not** (dedup policy):
  - Model 117: 160 here against DOS's 141.
  - Bank-03 model 84: 141 here against 203.
- **Scaffold loose ends:**
  - No `license` field in `[workspace.package]`. MIT-ported modules carry attributions; the licence choice is open.
  - Toolchain 1.98.1 installed in `~/.rustup`.
  - `README.md` links to `../RUST_REMAKE_PLAN.md`, which is outside the tree.
  - `Cargo.lock` was changed by four agents, each by adding dependencies to their own crates.
  - `avian3d`, `bevy_enhanced_input` and `gltf-json` are declared in `[workspace.dependencies]` but no crate uses them yet. The glTF writers build JSON with `serde_json`.
    - **Wave 2:** `gltf-json` is now a dev-dependency of conker-extract (GLB validation in tests).
    - `avian3d` and `bevy_enhanced_input` are still unused. Collision queries are conker-sim's own `CollisionWorld`,
      the movement is a kinematic port, and input is read directly.
    - In wave 2, `Cargo.lock` gained conker-camera and conker-anim, and Bevy features for the client (glTF, PNG, free camera).
- **War: Total War** (scenario 9) player models are not observed yet; only Deathmatch's 117 is.
- **Wave 2 contracts between crates:**
  - `Input.camera_yaw` is the eye → look-at heading of the previous tick, truncated to a u16 (conker-camera's
    `yaw_u16()`; only `>> 8` is used). **Wave 3:** it is `Follow::yaw()` = `CameraState::stick_yaw()`, all 16 bits
    used.
  - Each tick: resolve the stick with last tick's camera, `movement::step`, then step the camera with the moved player.
    **Wave 3:** `Follow::tick` does exactly that, with the pad's buttons and stick, then sets `PlayerState::aiming`.
  - The camera's `heading` is `heading_from_facing(facing)`, except in state 122, where the game uses a wall-aligned
    heading (−11.08° in the traces) that the sim does not produce yet. **Wave 3:** the sim produces it
    (`PlayerState::heading`, obj+0x40).
  - The movement state is the animation id. Replicating (state, speed, target) lets every peer run `AnimPlayer`
    deterministically. A late joiner's clock starts from the current state.
  - All three crates are deterministic f32 code (+ − × ÷ and sqrt; the game's own sin/cos/asin), so server, native and
    wasm agree. The smoke test's 0.000000 prediction error is that, end to end.
- **The lab tooling uses lldb on a re-signed copy** of the oracle (`tools/anim_lldb.sh`, in a temp dir). The oracle
  binary in `lab/work` is unchanged and was not rebuilt in wave 2. **Wave 3:** it was rebuilt once before the wave
  (the second scripted player, `CONKER_MP_HUMANS=2`; file date 2026-10-08 23:39) and not during it. The wave-3 lldb
  captures all ran on re-signed temp copies.

## Wave 2 plan (from wave 1) and its outcome
Outcomes:
1. Movement: **done except walls** (ledge, wall traces).
2. Camera: **done except wall avoidance, the aim camera and the blocked-view cut**.
3. Animation: **done**.
4. Surfaces: **not started**; the words met are recorded.
5. Spawns: **assumed**. Record 1 first is measured; the rest is nearest-first (I).
6. Combat: **not started**.

Of the unranked items:
- Chrome: done. Firefox and Safari: not run.
- Texture decoding: consolidated.
- The 3 decode failures: explained (the ROM's own lists).
- The lightyear report and the Total War models: not done.

The plan as written in wave 1:

1. **Movement controller** (`conker-sim`, game units). It can start now: the rules above are measured.
   - Replay `../lab/oracle/scripts/*.txt` and compare `trajectories/*_a.csv` row by row (row N = state after input N-1).
   - Run against `extract_scene::scene_collision`, with a grid or BVH in place of the linear `floor_below`.
   - Start with idle, walk, run and jump, taking camera yaw from the CSV.
   - Open: 3-VI dt scaling, the −256 offset, slopes, and wall sliding (state 122).
2. **Camera.** Needed by 1, since the stick is camera-relative.
   - Known: eye and look-at offsets, and the spawn record's initial eye.
   - Unmeasured: the swing-back behind the player (it makes a held sideways stick run in a circle).
   - Check against the turn and ledge traces.
3. **Animation playback.** Clips and rig are done.
   - Name the logical ids by tracing u16 +0x84 with `CONKER_TRACE_RAW` against the `state` column.
   - Settle root motion (baked into the root channel) before driving locomotion.
   - Loop on `durationTicks`.
4. **Collision surface semantics.** Words known (8 in 0x2D, 176 overall), meanings not.
   - Read the consumers DOS names (`func_150E1D14`, `func_150450CC`, `func_15175958`).
   - Script the oracle onto 0x9 and 0xA faces.
5. **Spawn semantics.** The table is known (20 records).
   - Unverified: the jitter `f·80−40` and `func_150ADA68`.
   - Unknown: which record each player gets. Lead: struct126+0x11B.
6. **Combat.** Nothing traced yet: weapons, damage (health starts at 6), death and respawn, score (not the guessed field), timer.
   - Needs new oracle scripts with encounters.

Also needed, not ranked:
- Run the wasm client in Chrome, Firefox and Safari 26.4+.
- Report the transport-link bug to lightyear.
- Consolidate texture decoding.
- Fix the 3 texture decode failures.
- Observe the Total War player models.
