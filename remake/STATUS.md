# Status after wave 6 (2026-10-10)

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
- Trigger kinds 5 and 18 and the per-frame music volume call are unread; the camera hints (5) and volumes (6) are
  ignored as on 0x30.
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
- Kind 0xE's `func_1507CD64(obj, 0xD)` call is not modelled (no oracle player stood on it in 60 s). Trigger kinds new
  on 0x34 (12: 29 records, 17: 1) are unread; dispatch-2 placements 6 and 9 move by 10 in y in the oracle, static here.
  Camera script zones and volumes as on 0x30 (not ported).

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

Gaps:
- The skull is drawn at its full 32 x 32 (the original grows it from 8 x 8 over 16 frames) and the level still draws
  in an eliminated player's view (the original draws only the sky). The lit button does not pulse.
- During the fade the original hides the radar, score and rank (HUD flags `&= 0xE35F`); the remake keeps them.
- `hits` counts every hit taken from another player (burns and multi-victim blasts too; when the original's
  `gGameState+0x66` counts is untraced); the flame counts no shots; headshots only from rays and aimed shots (bullets
  have no head test in the rules). P3/P4 row colours are inferred (their labels' green and yellow).
- RESTART starts the next match at once (the original fades 17 frames); a finished match with no human left waits for
  one to join. Tied bots share a place (the original orders them by slot; bots are not listed).

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
- 0x30's camera script zones are not ported (`CameraLevel` has 0x2D's only); its 8 action-1 camera volumes are
  ignored, as on 0x2D. The mounted guns are left out. No team modes (Colors' tables are only used for the sides).

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
- Elements, positions and rules: the table in `../lab/hud/README.md` (V unless noted there: the rank's tie rule, the
  scope dots' small turn and the iris centre's source are I; 0x16's R view tints the framebuffer, not reproduced).

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
- The rank on a tie: here everyone with the most kills is 1st; the original showed 1st or 2nd at 0-0 (rule not read).
- The iris's timing comes from one death (health poked to 0: 134 frames, the iris in the last 88). The remake's deaths
  differ (twin guns 151 frames, a grenade 87), so the table is keyed on the frames left before the respawn: a grenade
  death starts it at once and mid-way. Its centre is fixed at the place the original's settled ((150, 91): the dying
  player's head on screen, I) instead of following the head.
- Weapon 0x16's R view (a tinted framebuffer copy) is not drawn; the scope dots don't turn with the zoom.
- In a narrow window the page's `#net` line covers part of the clock, and before the first key press the page's sound
  hint covers the weapon icon (both kept as they are).
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
- **1278 (the killing bullet):** played at the lethal hit (e_w09 read 948); the original plays it at 1004, from a bullet
  routine (`func_151D4DAC`), 970 in e_w39 and never in sq_e_w09: its trigger is I.
- 802 (a bullet passing by) is not played; the in-hand fuse (929 while cooking past 60 ticks left) is not either.
- Remote players' sounds come 4-5 reads after the original's (their state's trip and the interpolation delay); the
  local player's own are on the read.
- A hit's weapon is inferred (wrong when another player holding a matching weapon is nearer than the shooter).
- The original picks the grenade's three landing sounds in a random order; here 1447, 1448, 1449 in turn.
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
