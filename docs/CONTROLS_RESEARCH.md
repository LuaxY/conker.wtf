# Modern controls research (2026-10-14)

Goal: replace the N64 control scheme with a modern third-person-shooter (TPS) one. Keyboard + mouse first, then
Xbox / DualSense pads, with rebinding and an in-game Esc menu with Settings. Research done and every decision taken (§1); nothing is built yet.
Paths are under `crates/`. Two research rounds; the sources are in §11.

## 1. Decisions so far

| # | Question | Answer |
|---|---|---|
| 1 | Move while aiming and while shooting? | **Yes** |
| 2 | Bullets follow the crosshair up and down (pitch)? | **Yes** (§3) |
| 3 | Keep a player-selectable "Classic N64" scheme? | **No.** The N64 code path stays inside for the bots and the oracle tests only. |
| 4 | Aim hold or toggle? | **Hold, like CoD:** RMB / LT. Scopes step their zoom on the wheel (§3). |
| 5 | Separate actions for fire, crouch, jump, reload…? | **Yes** (§4) |
| 6 | Rebinding and mouse settings | **Yes:** every action on keyboard, mouse buttons and pad; invert Y, sensitivity… (§5) |
| 7 | In-game menu | **Esc → Resume / Settings / Leave match** (§6) |
| 8 | Jump while armed? | **Yes** |
| 9 | Crouch while armed? | **Yes** |
| 10 | Fire while unarmed? | **Draws the weapon** |
| 11 | Picking up a weapon | **Draws it at once** (F2: so is the start item, at the match start and after each respawn; Beach's 0x3E is no weapon and stays as it is) |
| 12 | Weapon pickup | **Manual, with an Interact key (F)**, no longer automatic when walking over it (F2: but taken by touch with empty hands, no prompt; a gas mask counts as held. F3: holding only the start item counts as empty hands) |
| 13 | Grenade while armed? | **Holsters the weapon and throws** |
| 14 | Legs slide when strafing (no strafe clips) | **Accepted** |

Decided 2026-10-14 (the user took every recommendation):

| # | Question | Answer |
|---|---|---|
| 15 | Toggle aim | An option in Settings; hold is the default |
| 16 | Speed while aiming | 0.75 of the run; 0.5 when scoped |
| 17 | Menu UI | The HUD's own quad renderer (§6) |
| 18 | Manual pickups | Only on-foot weapons. Grenade packs, gas masks, canisters, flags, the bag, eggs, the bomb and the Tank, Race and Beach items stay automatic (§4b). F2: only while a weapon is held (`can_take`); F3: a weapon other than the start item |
| 19 | Mounted guns | F gets on, Q gets off |
| 20 | Picking up a weapon while holding another | The old one vanishes, as today. Dropping it needs a new world item: later |
| 21 | Move while drawing or holstering | Yes (the original holds you still 13-64 frames) |
| 22 | Fire held through a draw | Fires as soon as the weapon is ready |
| 23 | After a grenade throw | The weapon is drawn again |
| 24 | Holster + draw refills the magazine | Kept, as the original's |

## 2. What exists today

- **`Input`** (`conker-sim/src/movement.rs:156`): `{ stick_x: i8, stick_y: i8, buttons: u16, camera_yaw: u16 }`.
  - The stick is in raw N64 units.
  - `buttons` is the N64 bitmask: A 0x8000, B 0x4000, Z 0x2000, R 0x0010, C 0x000F.
  - It is lightyear's native input with lag compensation, at `PROTOCOL_ID = 36` (when written; 42 after R3). File:line references are from before R1.
  - Bits 0x1000, 0x0800-0x0100, 0x0080, 0x0040 and 0x0020 are never read by any crate, and the server copies `Input`
    as-is.
  - **But recorded oracle traces do set some of them:** `lab/oracle/trajectories/combat_buttons.csv` has L and the
    D-pad, and `camera_buttons.csv` has L. So the new actions must not reuse those bits (§4).
- **Keyboard and mouse** (`device_input`, `conker-client/src/lib.rs:386-462`): hard-coded tables.
  - The mouse steers only while R is held (`mouse_aim`). Nothing turns the camera.
  - There is no pointer lock, no settings and no persistence. The lobby (React) keeps only `conker-player` in
    localStorage.
  - Other hard-coded keys: C = debug camera (`render.rs:56`), H = text HUD, M = mute, Tab = team screen.
  - No Esc handler and no pause menu.
- **Gamepad:** read through bevy_gilrs (the Gamepad API on the web), first pad only.
  - The right stick is thresholded into the C buttons.
  - The triggers are read as plain buttons.
- **Movement** is already camera-relative (move angle = `camera_yaw` + stick angle).
  - **While R aims, the stick is zeroed** (`movement.rs:815-819`).
  - Scoped, the drive is 0.1 (`982-990`).
  - **Moving while firing already works** with the twin guns, pistol and tommy gun (full speed, `armed_walk`), but the
    facing follows the run, so you shoot where you run.
- **Vertical aim:** the twin guns and pistol fire **perfectly level** (`projectile.rs:294`).
  - The twin muzzles are about 41 units up, the pistol's 51.5.
  - The tommy gun is level, plus a recoil climb.
  - Humans get no vertical help. A Tedi standing on a ledge about half his height above you can't be hit (the bullets
    pass under his feet), nor can one well below you.
  - Bots already shoot along a real aim ray: `game.rs:2828-2846` sends the bullet from the muzzle to the first body
    part or wall on `m.aims[i]`. **Filling `m.aims` for humans gives pitch with no new rules code.**
- **Camera:** an exact port of the N64 follow camera, and part of the simulation. The server runs one per player for the
  aim gate and the scope. The Tank, Raptor and Race client-only cameras show how to render a different camera while
  it keeps running (`FollowCamera.tank`).
- **Animation:** one clip plus a cross-fade, with no layers. The upper-body twist only rotates the spine
  (`held.rs:284-311`). Bevy 0.19.1 supports masked clips (`add_clip_with_mask`).
  - **There are no sidestep or backpedal clips**, so an armed strafe already slides the legs.
  - The server's hit spheres are posed from the rules' single clip.
- **UI:**
  - The real HUD is textured quads in the original's font, drawn the same way on web and native (`hud.rs`). The font
    has 95 glyphs and **no lowercase letters**.
  - The browser build deliberately leaves out bevy_ui (about 1.5 MB gzip); its text HUD goes into DOM elements.
  - The existing menus (team screen, results RESTART/QUIT) navigate by hand on N64 bits, with no clicks.

## 3. Aiming like CoD / CS

- **CoD:**
  - Right-click aims down the sights, with a Hold/Toggle option per device. Hold is the default (per guides; no
    official page says so).
  - Moving while aiming runs at about 64-84 % of base speed (BO6 numbers: assault rifles 75 %, pistols 84 %).
  - Hip-fire shoots in a random cone that aiming collapses.
- **CS2:** there is no aiming down sights. Right-click on the AWP or SSG cycles zoom 1 → zoom 2 → unzoomed.
  - The AWP moves at 50 % speed while scoped.
  - Moving destroys accuracy, so players counter-strafe.
- **For us:** hold RMB to aim; it can't also cycle CS-style zoom.
  - The over-the-shoulder view zooms in.
  - Scoped weapons open at zoom 1, and the **wheel steps the zoom** (D-pad up/down on a pad). The wheel is free
    because there is one weapon slot, and this matches the original's C-up/down zoom.
- **Speeds:** hip-fire at full speed, aiming at 0.75 of the run, scoped at 0.5 (about the existing walk scale,
  41/80).
  - Today's armed strafe is 34 × 0.65 = 22.1, against a twin-gun run of 35; 0.75 × 35 ≈ 26.
- **Accuracy:**
  - Hip-fire uses a spread cone, and aiming tightens it. Moving while scoped adds spread.
  - Keep damage, rate of fire and ammo ROM-exact, and balance mouse precision with spread only.
- **Crosshair:** fixed at the screen centre. The aim ray goes from the camera; the shot goes from the muzzle to the
  ray's hit point. If the muzzle is blocked, grey the crosshair and mark the real impact point.
  - Hit and headshot markers must come from the server, because the client tests hits against cylinders and the server
    against posed spheres.
- **Aim on the wire:** `camera_yaw` (u16) doubles as aim yaw, and a new `aim_pitch: i16` is added.
  - The server rebuilds the ray from position, yaw, pitch and shoulder side, fills `m.aims[i]`, and clamps the pitch.
  - **The shoulder side goes on the wire from the start** (one bit in C2's bump), because the ray's start depends on
    it. Otherwise shoulder swap costs a second protocol bump.
  - **The ray starts at the point on the camera line nearest the weapon** (Lyra's `CameraTowardsFocus`), not at the
    camera, so walls between the camera and the player are never hit. That is the query the server runs. Ignore the
    shooter's own body.
  - The camera arm stays simple (collision only slides it along its look line), so the server can rebuild the exact
    ray without the client's camera.
  - In the modern scheme the scope's yaw and pitch come from this aim too, not from the N64 scope camera.
  - An alternative from round 2 sends a relative "look stick" (`look_x/look_y: i8`) through the N64 aim camera. It
    changes less but is coarse (i8 steps) and keeps the N64 camera's lag. **The absolute aim is better for a CoD/CS
    feel.** Both bump the protocol.

## 4. Separate actions

- **A dedicated `actions: u8` field in `Input`:** JUMP, CROUCH, FIRE, RELOAD, SHOULDER, plus MODERN, which gates all
  the new behaviour.
  - The struct changes anyway for `aim_pitch`, so this costs nothing extra.
  - It doesn't use the spare `buttons` bits, because recorded oracle traces set L and the D-pad (§2).
  - Bots and traces leave it at 0.
- **One mapping function, `pad(&Fighter, mode, &Input) -> Input`, at the top of `Match::player`** (`game.rs:2034`).
  - It turns the actions into N64 bits from the rules' own armed state.
  - It runs before the Tank, Race and Raptor dispatch, on both the server and the client's prediction.
  - It is the identity while `actions` is 0, so the bots and oracle tests are unchanged.
  - A mispredicted draw is corrected by rollback. That removes the risk of keys whose meaning depends on the weapon
    state.
- Keep **R** (aim) and the **C** bits as N64 bits so the cameras need no change.
- **Contextual actions that stay on one key:**
  - Q = "Holster / drop": draw/holster, flag put-down, bag throw, mounted-gun stop.
  - The raptor bite and race kick stay on their own buttons.
- **C-down's three jobs split naturally:**
  - Grenade → G.
  - Step back → S.
  - Zoom out → wheel.

## 4b. Weapon handling (decisions 8-13)

All of it sits behind the MODERN gate; bots stay on the N64 path. Files: `conker-rules/src/game.rs` (G),
`weapon.rs` (W), `conker-sim/src/movement.rs` (M).

- **Pickups today:**
  - Pickups run in `Match::pickups()` (G:1507), at the top of `step()` (G:1196), **before** the players and
    before `pad()`.
  - A touch is a horizontal radius of 100 × reach / 64 (reach 64 for humans) plus a height test (`weapon::touches`,
    W:831).
  - The weapon arrives 2 frames after the touch and respawns after 120 frames.
  - **Bots fetch weapons by walking over them,** so they must keep the automatic pickup.
- **Holding a weapon and touching another today:** `give()` (G:1062) replaces it silently. The old one vanishes, the
  new one arrives holstered, and a grenade pull or hold is cancelled. Walking over your own gun takes it again.
- **Things touched automatically, besides weapons:**
  - Grenade packs (the only ammo; there are no health pickups) and gas masks.
  - Canisters, the Colors flags and the Heist bag.
  - Raptor eggs and the Tank bomb.
  - The Tank, Race and Beach items.
  - Mounted guns, which are predicted on the client.
- **Manual pickup:**
  - `pickups()` reads `inputs[i].actions` itself, since it runs before `pad()`.
  - An edge-triggered Interact needs a stored "previous actions" byte on `Fighter`.
  - Only on-foot `Gives::Weapon` (including Raptor's bone weapons) is gated.
  - The "PRESS F" prompt goes in `hud.rs`. Use a shared rules helper so the prompt and the gate always agree:
    pickups are decided by the server, and the client already knows the pack's pickups and `MatchView.taken`.
  - Size: medium.
- **Draw on pickup:** start the draw right after `give()` in `pickups()`. Size: small.
- **Fire draws:** `pad()` maps FIRE to a B press while a weapon is held, holstered and the player is free.
  - A draw also refills the magazine, as in the original.
  - If FIRE stays held, the gun fires when the draw ends.
  - Size: small.
- **Jump while armed:** no armed jump clip exists. The gun stays in hand with the unarmed jump clip, which is
  already what the 5 weapons that can jump do (`Weapon::jumps`, W:107).
  - Change: the reload at G:2366 reads RELOAD instead of A, and G:2633 stops removing A. Size: small.
- **Crouch while armed:** **no armed-crouch clip is known.** The fallback is the unarmed crouch clip with the gun in
  hand, until the C4 upper-body mask.
  - Changes:
    - The sim's three `armed` gates (M:966, M:1102, M:1152-1156) are lifted under MODERN.
    - The rules feed the movement Z from CROUCH (G:2630).
    - The armed animation match (G:2751) gains a crouch case.
  - The hit volumes follow by themselves: the crouched cylinder and the posed spheres.
  - Crouch + Jump while armed becomes the high jump.
  - Size: medium.
- **Grenade while armed:** `pad()` sends B (holster) and, once free, C-down.
  - Extra cost: the holster's 13 to 34 frames plus about 2.
  - Holding G needs no new state; a tap needs a latch on `Fighter`.
  - Holstering first keeps us on traced behaviour (every traced pull was unarmed).
  - Size: small to medium.

## 5. Rebinding and settings

- **No new library:** `bevy_enhanced_input` 0.26 is pinned but unused.
  - Its bindings are immutable, so rebinding means respawning them.
  - It has no UI and no persistence.
  - Networking it would replace `Input`.
  - `device_input`'s two `(input, bit)` arrays already are a binding table: make them data (a `Bindings` resource),
    and return the look delta as well.
- **Slots:**
  - Two keyboard/mouse slots per action (primary and alternate), and one pad slot.
  - Mouse buttons 1-5 and wheel up/down are bindable.
  - Shift, Ctrl and Alt bind as plain keys; no chords.
  - **Esc and the pad's Menu button can't be bound:** on the web the browser owns Esc (it releases pointer lock), and
    the menu must always be reachable.
- **Conflicts:** detect them at bind time and offer Swap / Replace / Cancel. The same key in different contexts
  (on foot vs Tank or Race) is fine.
- **Accessibility (Game Accessibility Guidelines, XAG 107):**
  - Remap everything.
  - Invert Y per stick.
  - Sensitivity ±50 % or more.
  - Hold/toggle options.
  - Every prompt shows the current binding.
- **AZERTY:**
  - Bevy's `KeyCode` is the physical key, so WASD is ZQSD on AZERTY automatically.
  - Labels: cache `KeyCode → logical key` from every key press, which works on web and native.
  - `navigator.keyboard.getLayoutMap()` exists only in Chrome/Edge; use it as a fallback for keys not pressed yet.
- **Pad glyphs:** pick them from `Gamepad::vendor_id()` (054c = PlayStation, 045e = Xbox) or the name, and fall back to
  Xbox. Add a setting: Auto / Xbox / PlayStation.
- **Persistence:**
  - One serde `Settings` struct with a `version`, saved as JSON (`serde_json` is already a dependency).
  - Web: localStorage (add web-sys's `Storage` feature). Native: `config_dir()/settings.json`.
  - Store only the binding overrides, keyed by action names, so new actions get their defaults.
  - If parsing fails, fall back to defaults and keep a `.bak` copy.

### v1 settings

| Setting | Default | Range | Device |
|---|---|---|---|
| Look sensitivity | 1.5 (CS scale, 0.022°/count) | 0.1-10 | Mouse |
| Aim / scope sensitivity multiplier (FOV-normalised) | 1.0 | 0.1-4.0 | Mouse |
| Invert Y | Off | On/Off | Mouse |
| Look sensitivity | 1.0 (about 450°/s yaw, 270°/s pitch) | 0.5-2.0 | Pad |
| Aim sensitivity multiplier | 1.0 | 0.1-4.0 | Pad |
| Invert Y (look stick) | Off | On/Off | Pad |
| Stick layout | Default | Default / Southpaw | Pad |
| Stick dead zone (each stick) | 0.10 | 0-0.5 | Pad |
| Vibration | On | On/Off | Pad |
| Button prompts | Auto | Auto / Xbox / PlayStation | Pad |
| Aim mode | Hold | Hold / Toggle | Each device |
| Crouch mode | Hold | Hold / Toggle | Each device |
| Walk mode | Hold | Hold / Toggle | Keyboard & mouse, pad |
| Crosshair colour | Green | 5 presets | All |
| Crosshair size | 1.0 | 0.5-2.0 | All |
| Bindings | Tables in §7 | Any input except Esc / Menu | Each device |

- **Later:** split zoom multipliers, monitor-distance coefficient, response-curve choice, aim-assist modes, button-layout
  presets, the full CS crosshair editor, settings import/export, Keyboard Lock.
- **Never:** mouse acceleration or smoothing. Always request raw input on the web.

## 6. In-game Esc menu

- **Opening it on the web:** the browser uses Esc to release pointer lock, and the page may never see the keydown. So
  open the menu when the lock is lost (`pointerlockchange` with `document.pointerLockElement == null`).
  - winit has no such listener, and Bevy still reports `Locked` after Esc. Add a listener of about 15 lines that
    pauses input and resets `grab_mode`.
- **Opening it elsewhere:**
  - Native: Esc, or the window losing focus.
  - Pad: the Menu / Options button. A pad press isn't a browser gesture, so resuming from a pad must work without
    pointer lock.
- **Resume:**
  - It needs a click, a user gesture.
  - Chrome refuses a re-lock for 1.25 s after an Esc unlock (`kEffectiveUserEscapeDuration`).
  - If `pointerlockerror` fires, keep the menu open.
- **The match keeps running** (multiplayer), as an overlay like CoD and CS2.
  - Send neutral input but keep the yaw.
  - The character stands still and stays vulnerable.
  - Add a `menu.open` arm where the free camera and team screen already zero the input (`lib.rs:1230`).
  - Gate the team screen and the C/H/M toggles.
  - Close the menu when the results screen shows (it reads the input that is sent).
- **Settings screen:**
  - Tabs: Keyboard & Mouse, Controller, Mouse/Look, Gameplay. Rows are grouped Movement / Combat / Camera / UI.
  - Rebinding prompt: "PRESS A KEY OR BUTTON — ESC CANCELS, BACKSPACE CLEARS", with a 5 s timeout. Ignore the click that
    opened it. A trigger counts once pushed past 0.5.
  - Pad navigation: D-pad or stick moves the focus, A/Cross confirms, B/Circle goes back, LB/RB switch tabs. Arrows,
    Enter and Esc use the same focus.
  - Settings apply live and are saved on Apply.
- **UI technology:**

  | Option | Gain | Cost |
  |---|---|---|
  | **(c) the HUD's quad renderer (recommended)** | No new dependencies, same on web and native, the original's look | Focus and click rectangles by hand (the team screen's pattern); labels in uppercase |
  | (a) bevy_ui on both | Clicks and focus navigation (`bevy_input_focus`) for free | +1.5 MB gzip on the web build |
  | (b) HTML overlay | Best forms | Web only, so native needs a second menu, plus a wasm↔JS bridge |

## 7. Recommended default bindings

The keyboard defaults are browser-safe: **Ctrl+W closes the tab in Chrome and Firefox**, and `preventDefault` can't stop
it, so crouch can't be on Ctrl. Alt-walk plus Tab gives Alt+Tab. That is why this table moves crouch and walk off
Ctrl and Alt, unlike round 1.

**Keyboard + mouse**

| Action | Primary | Alternate | Notes |
|---|---|---|---|
| Move | W A S D (physical; ZQSD on AZERTY) | Arrows | Camera-relative |
| Look | Mouse (pointer lock, raw) | – | |
| Fire | LMB | – | If unarmed: draws the weapon |
| Aim / scope | RMB (hold) | – | Toggle option |
| Scope zoom in / out | Wheel up / down | – | While scoped |
| Jump | Space | – | |
| Crouch | C | – | Not Ctrl; the debug camera moves off C |
| High jump | C + Space | – | Crouch then jump, as the original |
| Reload | R | – | |
| Holster / drop (flag, bag) | Q | – | |
| Interact (pick up a weapon) | F | E | Picking up draws the weapon |
| Grenade | G | – | |
| Shoulder swap | X | Middle mouse | Was V; V is push-to-talk (2026-10-17) |
| Push-to-talk | V (hold) | – | Everyone hears you; your team only in the team modes |
| Chat: everyone / team | T / Y | – | The page's box (browser); also over the Esc menu and the results |
| Walk | Left Shift (hold) | – | Not Alt; the original has no sprint |
| Scoreboard / teams | Tab (hold) | – | |
| Menu | Esc (fixed) | – | Opens on lock loss |
| Mute | M | – | Moves into the binding table, like H |
| Mouse 4 / 5 | Unbound | – | Bindable; needs a `mouseup` `preventDefault` in Chrome |
| Ctrl / Alt / Meta / F-keys | Unbound | – | Bindable with a warning |

**Gamepad (Xbox / DualSense)**

| Action | Xbox | PS5 |
|---|---|---|
| Move | LS | LS |
| Look | RS | RS |
| Fire | RT | R2 |
| Aim / scope (hold) | LT | L2 |
| Jump | A | Cross |
| Crouch | B | Circle |
| Reload | X (tap) | Square (tap) |
| Interact | X (hold, CoD-style) | Square (hold) |
| Holster / drop | Y | Triangle |
| Grenade | LB | L1 |
| Shoulder swap | RB | R1 |
| Scope zoom | D-pad up / down | D-pad up / down |
| Walk toggle | L3 | L3 |
| Scoreboard / menu | View / Menu | Create / Options |

**Pad feel:**
- Radial dead zone 0.10, outer zone 0.95.
- Response curve exponent 2.
- An acceleration ramp at full deflection.
- Aim assist is slowdown only (0.3-0.5) and for pads only. Mouse aim is never assisted.
- No gyro: the Gamepad API excludes motion sensors.

## 8. Browser notes for the implementation

- **Pointer lock:** winit's web `set_cursor_lock` passes no options. We need our own web-sys
  `requestPointerLock({ unadjustedMovement: true })` in the click frame, falling back on `NotSupportedError`. The same
  listener handles lock loss (§6).
- **Default browser actions:**
  - winit already blocks the defaults of keydown, wheel, contextmenu and pointerdown, and Bevy blocks Tab, Space,
    arrows and F5.
  - **Mouse 4/5 still navigate back and forward** (winit has no `mouseup` listener). Add one that calls
    `preventDefault` for buttons 3 and 4.
  - Firefox always opens its menu on Shift+right-click.
- **Keyboard Lock** (Chrome/Edge, fullscreen only) would make Esc and Ctrl ordinary keys. Leave it for later.
- **To verify when building it:**
  - Chrome scaling `movementX` by devicePixelRatio, which would mean a sensitivity calibration.
  - Middle-click autoscroll being blocked.
  - Whether Chrome 131 shipped a pointer-lock permission prompt. The two rounds read the same Chrome blog post
    opposite ways.

## 9. Risks

- **Mouse precision against weapons tuned for N64 lock-on.** Tune spread, not damage, and watch the headshot rate.
- **Legs slide when strafing or backpedalling,** because there are no such clips. The original's armed strafe does it
  too. Real clips would need new animation work.
- **Upper-body aim over walking legs** needs masked clips on the client. The server's hit spheres stay posed from the
  single clip, so they differ slightly from what's drawn.
- **Hit feedback must come from the server** (cylinders on the client, spheres on the server).
- **Oracle tests** survive only if everything new is gated on MODERN; bots never set it.
- **Each protocol bump** is yours, once per wave (`ROADMAP.md:67-69`).

## 10. Suggested waves (small batches)

| Wave | Content | Touches | Protocol |
|---|---|---|---|
| C1 input plumbing (**done 2026-10-14**, `STATUS.md`; the browser parts unverified) | `Bindings` + `Settings` data with persistence (defaults = today's bits, crouch on C, walk on Shift); pointer lock (raw input, lock-loss listener); debug camera off C (now F8). No orbit camera yet: it only makes sense once aim follows it | client, web | no |
| C2a modern rules (**done 2026-10-14**, `STATUS.md`; `actions` is a u8: holster rides B, walk the stick; the server needed no change) | `actions` field + `pad()` mapping; `aim_pitch` + shoulder bit + humans' `m.aims`; MODERN gate (move while aiming/firing, facing on the aim, scope from the aim, speeds 0.75 / 0.5). Testable headless with bots | sim, rules, server | **bump** |
| C2b modern client (**done 2026-10-15**, `STATUS.md`; no wire change: hit markers from `Combat::hits` / `headshots`; the browser's pointer lock unverified) | Orbit camera feeding `camera_yaw` / `aim_pitch`; prediction with the new fields; centre crosshair; server-confirmed hit markers | client | (same bump) |
| C2c weapon handling (**done 2026-10-15**, `STATUS.md`; `Fighter` gains 4 replicated fields; the client must send GRENADE and INTERACT as actions) | Interact pickup (weapons only), draw on pickup, Fire draws, jump/crouch armed, grenade armed then redraw, move while drawing, mounted guns on F / off Q (§4b); the "PRESS F" prompt | rules, sim, client HUD | **bump** |
| C3 Esc menu (**done 2026-10-15**, `STATUS.md`; the menu holds `Paused`, which already zeroes the input; one binding context; the browser and real pads unverified) | Resume / Settings / Leave; rebinding capture with conflicts; mouse and pad settings; input zeroed while open | client | no |
| C4 polish (client half **done 2026-10-15**, `STATUS.md` "Controls C4: the client's feel": masked upper-body aim bent to the pitch, shoulder ease, pad radial dead zone / edge ramp / pad-only assist / rumble, glyph prompts, the mount prompt, DEFAULTS; no wire change. Rules half **done 2026-10-15**, `STATUS.md` "Controls C4: spread…": per-weapon hip/aimed/scope cones hashed from tick, slot and shot, no RNG; the hip bolt aims instead of locking on; the shoulder ray short of a wall; a grenade tap kept through a draw; Fire in the draw's last frame. No wire field, but modern shots changed: worth a bump) | Pad look stick, curves, glyphs; masked upper-body animation; spread; shoulder swap; blocked-shot marker; aim-assist slowdown | client, rules | no (shoulder bit already in C2) |

## 11. Sources

**Round 1: TPS camera and aim**
- Lyra mirror: https://github.com/mpursley/Lyra_Inventory
- https://uhiyama-lab.com/en/notes/ue/camera-spring-arm-guide/
- https://docs.unity3d.com/Packages/com.unity.cinemachine@3.1/manual/CinemachineThirdPersonFollow.html
- https://docs.unity3d.com/Packages/com.unity.cinemachine@2.10/manual/Cinemachine3rdPersonAim.html
- https://devforum.roblox.com/t/tlou2-style-tps-game-aim-visibility-in-tps-system-camera-center-hitscan/4764713
- https://www.gamedeveloper.com/disciplines/doing-thumbstick-dead-zones-right
- https://www.gamesfirst.com/articles/2004/conker-live-reloaded
- https://www.gamespot.com/articles/halo-pc-hands-on/1100-6028228/

**Round 2: CoD/CS aim, settings, rebinding, browser, menu**
- CoD: https://blog.activision.com/call-of-duty/2019-10/Getting-Started-in-Modern-Warfare-Controls-and-Settings-PC,
  https://support.activision.com/au/en/black-ops-cold-war/articles/black-ops-cold-war-accessibility,
  https://www.callofduty.com/patchnotes/2024/08/call-of-duty-black-ops-6-open-beta-patch-notes
- CS2: https://csdb.gg/guides/advanced-mechanics-guide/, https://csdb.gg/command/zoom-sensitivity-ratio/,
  https://totalcsgo.com/commands/categories/crosshair
- Accessibility: https://gameaccessibilityguidelines.com/allow-controls-to-be-remapped-reconfigured,
  https://devdocs.xbox.com/build/game-principles/accessibility/xag-deep-dives/xag-107-input.md
- Reserved shortcuts: https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/ui/browser_command_controller.cc,
  https://bugzilla.mozilla.org/show_bug.cgi?id=1052569, https://forum.babylonjs.com/t/how-prevent-default-of-c-w-t-n/44117
- Pointer and keyboard lock: https://w3c.github.io/pointerlock/,
  https://developer.mozilla.org/en-US/docs/Web/API/Element/requestPointerLock,
  https://web.dev/articles/disable-mouse-acceleration,
  https://developer.chrome.com/docs/capabilities/web-apis/keyboard-lock,
  https://github.com/mrdoob/three.js/blob/dev/examples/misc_controls_pointerlock.html
- Layouts and pads: https://developer.mozilla.org/docs/Web/API/Keyboard/getLayoutMap,
  https://developer.mozilla.org/en-US/docs/Web/API/Gamepad/id, https://w3c.github.io/gamepad/
- Mouse 4/5: https://bugzilla.mozilla.org/show_bug.cgi?id=1720538
