# Roadmap: Conker multiplayer remake

Where things stand: `RUST_REMAKE_PLAN.md` (status) and `remake/STATUS.md` (details, verification, gaps).
Written 2026-10-10, updated through the controls rounds (2026-10-15). Within each list, order is priority. What is
still missing: `GAPS.md` (the G6 audit, 2026-10-15).

## Completion batches (planned 2026-10-13)
Goal: finish the game. Phase E fixes what is wrong or missing in the modes already done; phase M adds the missing
modes. Run in order, one batch at a time; commit after each. Each batch is 1-3 agents in disjoint crates. A
research agent writes `lab/research/mp-original/<topic>.md`. I read it, then the implementation agents start,
then I check and commit. Agents never commit. Prompts follow the Beach ones:
- lanes by crate;
- ports 5970-5979 and 5980-5989, two apart (the server binds port+1);
- a private `ConkerRecomp.<tag>` copy of the oracle; no commits in the oracle repo;
- V/C/I tags;
- `CARGO_TARGET_DIR=target-net`;
- `assets/` must equal a fresh `conker-extract pack`;
- kill only your own PIDs;
- a reply of at most 25 lines.

Sources: `remake/STATUS.md` sections and the lines of this file named per item.

| Batch | Agents (lanes) | Items | Status |
|---|---|---|---|
| E1 match-flow bugs | rules/server/bots; client | A join or leave mid-match restarts the level AI script (Total War part 3, Colors part 1, this file "Total War done"/"Colors"); both teams out on one frame; Colors untimed + unlimited never ends, and after the final capture it still fades and respawns in the end phase (Colors part 3); Beach's cap of 8 villagers (Beach rules gaps); a waiting player's lethal hit counts (Wave 8); bots treat a free slot as an always-dead human, per-slot `Lifts` reset, `DEV_PRIVATE_KEY` (Wave 7 item 4). Client: an outro restarts for a mid-outro joiner, and roster changes in Over flip the computed winner (Total War part 6); STATS rows overlap with more than 4 humans | done 2026-10-13 (protocol 19). Left: a human joining when no slot is free is a bot to the script until the next match; a client refused by a full Beach stays connected; STATS rows past P4 have no label |
| E2 bots fidelity | research ×2 (DM maps; Beach + club), then conker-bots | 0x30 lift-pair riders (DM 0x30 gaps); 0x34 node 60-62 jump loop (DM 0x34); ledge climb (0x30 node 48, state 235); camping (`func_1508A6FC` mask); club drawn 21% vs 52% (caveman); Beach escapes, 9 vs 5 (Beach rules gaps); `bots.sh` "bot 1 dies" flake; a seated gunner's shots unheard | done 2026-10-13 (protocol 20): the high jump 235 (0x30 ledge), kill gates, rising bodies land, the plunger sinks, only grenades make noise, the drawn flag, the 0x34 stall shortened, the club's spells. Left → E2c: bots stand and fire at targets out of reach (state 8, unaimed shots fly level), so DM deaths dropped (2D 8.8 vs the oracle's 13; 0x30 1.6 vs 9) |
| E2c bots out of reach | rules/bots | State 8: a bot in sight of an enemy out of weapon range stands and fires (2D seed 5: 1,828 shots, 5 hits at 3,634; seed 3: a sniper 65° up); unaimed shots fly level; what ends state 8 in the original; 0x30's low deaths (Bots and movement fidelity, part 2) | done 2026-10-13: bots' bullets follow their aim (twin guns from the eye ray, pistol/0x23 by the pitch byte), the twin guns' head rule. 2D 12.0 deaths (oracle 13), 0x34 7.0 (8). Left: 0x30 4.1 (9): the bots' weapon choice (twin 2,056 / sniper 2,706 frames vs the oracle's 3,384 / 805); obj+0x1D1's steps, the crossbow's pitch, the sniper bot's hit rate |
| E3 mounted guns | rules/sim/server; client | Launch homing, the leap's first frame and brake, e1's knock angle, a hurt gunner's trigger, `Effect::Gun`'s drop spot (Mounted guns: gameplay); the fire reaching the gunner late (server-only); `gun_stop`'s replay, the flash's pale box (Mounted guns: client) | done 2026-10-13 (protocol 22): the hop keeps its speed and homes, the leap's first frame, brake and facing, the aim ray (e1's knock 210.2°), a hurt gunner fires, the gunner's client predicts its fire, `gun_stop`'s replay, HUD alphas blended as the console. Left: the fire clip's recoil, an unpredicted mount (one rollback), e1's knock slide 167.7 vs 140.7, a 0x34 bot camping the gun |
| E4 held weapons | rules/sim; client | C-down with the twin guns or flamethrower; the crossbow's hip push (~5.3 a shot), the shotgun's point-blank sphere, bolts on odd ticks (crossbow/shotgun gaps); hide the weapon on the death frame; the crossbow's string clips and bolt sound; the caveman's grenade clips 0x175/0x16C, the club icon's pixel, a lethal first swing (caveman gaps) | done 2026-10-14 (protocol 33: C-down strafes back with a weapon drawn; the bolt pushes its shooter from the muzzle, first moves on the original's odd/even ticks, its pass-by sound; the point-blank sphere; the scared run 0x2B; the weapon 3 frames after death; 0x175/0x16C and grenade #76 for every class but the Tediz; the icons' pixel; the tommy gun's draw). Left: the crossbow's string clips (not packed), 0x3A for the grenade on 0x1A/0x3F, the dead pose's 0x18A, the pistol head test, the bolt's wall sound |
| E5 Total War + Colors leftovers | research, then rules; client | TW: a human out of lives takes over a team-mate bot; the traitor's +0x70 reset; a carrier can't jump, its camera weapon 0x25, it has no weapon; canister shrink; props' collision by +0x6E; doors at 51; the mask clips' turn and brakes, the muffled mask sound (TW parts 3/5). Colors: capture jingles, the flag sound's 3rd argument, the touch radius 45, the end-phase fade, the home flag's mirrored "S" (Colors part 3) | research done (`war-leftovers.md`); client half done 2026-10-13 (lamp flicker, the pickup shrink, the capture jingles, losers' voices and the radio, fades over the HUD and the end phase's fade, the mask muffle). Rules half done 2026-10-13 (protocol 21): the takeover, the carrier, doors and covers solid while shown (their collision exported), the mask put-on's move, the flag touch by body overlap, the final capture's +39/+55. The mirrored "S" and the traitor reset need no change. Left: the doors' collision a frame behind their drawing; the client doesn't predict the flag's push; a dead bot with lives can't be taken over |
| E6 outros | research, then pack + client | Head nods and turns (op 0x11, channels 0-2), talk flag, bubble grow/shrink, page fades, attachments (op 0x0D: 0x70/0x71, helmets and guns on 88/90), cs5's sounds and smoke, op 9, per-camera FOV, the STATS fade-in, the voice fade-out, Beach's part masks (op 5 0x3C), props 0xFC/0xFD, song 0x8A, cs3's near plane (TW part 6, Colors part 2, Beach pack) | done 2026-10-13: negative-t records dropped, the music on through the outro (ducked by voices), the STATS fade, line heights, bubbles and text fades, heads (88/90/135), masks and props, attachments (`maps/*/outro.gltf`), talk, blends, cs5's sounds, song 0x8A, cs3's near plane. Left: the villagers' head turns; song 0x8A's level; cs5's smoke, the tanks' engines and op 9 (Tank mode) |
| E7 Beach environment | research, then pack + client | Lamp flicker (behaviour 4, on 0x2D too), sweeping ground lights (19), strike decal 0x3103, song 0x4D's positional channels 13/14, the escape loop 0x4A7, explosion sizes and emitter, the shake, the bots' radio record, the plunger sinking, the escape teleport and Tedi ejection reads, 0x2D's gate lifts predicted (Beach gaps). Plus: the sea (placement 1) is water: players swim (state 204) and drift ashore (rules) | done 2026-10-13: the escape loops, the bots' radio heard by all, song 0x4D's stem by distance, the detonation rings and shake, the strike's glow and streak, 0x2D's gate lifts predicted, 0x2B relit with the sweepers. Left: unlit vertices dim with the lamps; flak, far fire, smoke, the strike's sparks; the streak's shape; the rockets' shake on every map (Beach only now); whether Total War's and Colors' bot radio is heard by all (shared code); the sea and swimming → E7b |
| E7b the sea | rules/sim; client | Beach's placement 1 is water: players swim (state 204) and drift ashore (`beach-environment.md` item 8) | done 2026-10-14 (protocol 34; `swim.rs`: water at −539, state 204 at −41.67, the current to the shore, strokes at 40 %, the wade and the jump out; sea and swim1 replays within 0.0004; no weapons or 0x3E in the water; predicted; the splash and water steps; bots never swim, as the original's). Left: ripples and bubbles, the two fixed sea cameras (0x26/0x27), the swimmer's death clip 0x24 |
| E8 visuals | pack/extract; client | Bullet, grenade and rocket models; flame particles; the gun twist on the model; the white quad near record 1; the twin guns' 3 untextured materials (Wave 7 item 1); fades and flash blended like the N64 (display space) | done 2026-10-13 (no protocol change): the rocket, grenade, knife and bolt models posed as the oracle's lists (bullets draw none, as the original's), the flame stream, the twist, the twin guns' textures (extract-gbi), 0x2D's and 0x34's never-drawn class-51 quads out of the levels, rocket shakes on every map; the fades were already display-space. Left: the muzzle flashes, the rocket's trail, the stream's puffs and tongues, the burning player off 0x34, the escapee fade's alpha |
| E9 HUD + audio | client (+ rules/server if the hit's weapon goes on the wire) | done 2026-10-13 (protocol 27 → 28 with M9): the scope hides the own body, the iris follows the head, 0x16's R tint and the dots, the page's overlaps, the fly-by, the fuse, the landing order, 1278 on lethal head shots, the hit's weapon from the server, music after a restart, the radio heard by all (V), 87/134's character.json. Left: Heist cs3's bubbles still squares (the texture is a soft square; how the original draws them is open); the scope dots' start angle (camera); corpses' head pops; the pistol/tommy bullet functions swapped in `game.rs` (C) |
| E10 DM maps + results | research, then sim/rules; client | Camera script zones and volumes on 0x30/0x34; the 0x34 corridor camera; 0x34 boxes and 0x2B placements sinking; kind 0xE's call; trigger kinds 12/17 (0x34) and 5/18 (0x3F); per-frame music volume; 0x2D props 0-1 for 51 frames. Results: skull grow, sky-only eliminated view, button pulse, HUD hidden in the fade, RESTART's 17-frame fade, P3/P4 colours, the accuracy hit rule | done 2026-10-13 (protocol 23; `dm-maps-results.md`): the floor-search zones, 0x34's plates, fire jets, electric floors and cones (kills to the presser), the corridor camera's box, RESTART's phase 3; the out view black over the HUD, the button pulse and the stick, the HUD hidden in the end fade, the restart fade. Most map items needed no port (camera zones, kinds 5/17/18, 0x2D's props). Left: bots linger in the trap zones (3-7 kills a minute vs none); plates/jets/cones unpredicted; two camera stretches; electric sparks; the hit-counting tests |
| E11 around the match | server; web | The team screen with BALANCE; bot caps by human count; a side switch without reconnecting; the real lobby (player counts, the 9/8 mock); pack checks between server and client; `pickups.json` ids; seat humans before bots (Heist's teams and bases follow the slot: M6b) (this file "Missing compared…" 1, "Asset pack") | done 2026-10-14 (protocol 32): humans in the first slots and the bots by team, reseated at each match start; the original's bot caps by human count (bots and team screens) on the server and in the form; the team screen (Tab/Select: switch team in the match, BALANCE) and `team=`; `/info`'s pack fingerprint (clients refuse another pack) and player counts in the browser and lobby; pickups the mode can't give fail the start. Left: Heist's 3 bots still take the bag 14-17 times (hb3 30, ROM path 23-24); the team size's up/down; Race's 2-human cap |
| M1 Heist research | research ×2 (rules; presentation) | Scenario 0 on 0x1A: 4 teams, the bag and vault, the win rule, respawns, the 4 roaming pickups, Wise Guy 59, item 0x19, tommy gun 0x38, HUD, outros, sounds | rules research done (`heist.md`, its M2-M4 split) |
| M2 Vault + DM 0x1A | pack/extract; rules/sim/server | Map 0x1A, character 59, its weapons, roaming pickups; Deathmatch on 0x1A | done 2026-10-13 (protocol 24): map 1a in the pack; Deathmatch on 0x1A (scenario 6): the Wise Guy in its team shirt, the bat, the tommy gun (62 shots in 241 frames as the original), roaming pickups, switching lights, the relight, song 30; bots match the oracle at 3 bots (7/16/4/4 vs 7/17/4/4). Left: the bases' hum zones never stop; radar team colours; the HUD text names; the roam spot picked at the respawn (I) |
| M3 Heist rules | rules/server/bots | The mode from its AI script | done 2026-10-13 (protocol 25, `heist.rs`): `--mode heist` on 1a: 4 teams, the bag (appear, carry at 72%, throw, drop, walk home, deliver), the end rules, script 0xB7's team branch, the outro keyed on a human's team winning (the 2-slot crash fixed), the web form. Runs: deliveries 2 in 300 s with 7 bots (oracle 2); 3-bot pickups half the oracle's. Left: voice lines not sent to clients; a thrown bag passes walls |
| M4 Heist client | client (+ pack) | HUD, effects, outros, sounds | done 2026-10-13 (protocol 26: the voice lines; `heist.rs`): the team pips, skulls, bag icon and radar, the bag and the carrier's clips, the bag's sounds, the voices, the hum stopping, both outros (exact vs the oracle's CSVs), the STATS with the bags column. Left: `bubble_particle.png` decodes opaque (palette IA16?); no `character.json` for 87/134; cs3's backdrop short of 16:9; the blink and the bag's clock per client |
| M5 Raptor research | research ×2 | Scenario 2 on 0x3F: raptor 83 (39 joints), the bite, eggs and baby, bone crossbow 0x41, bone knives 0x3F, the win rule, outros | done (`raptor.md`): the raptors' bite kills every caveman in front, feeding a body +1, eggs to the fire +2, every kill +1, first to 20; the original's raptor bots never feed (kept as the original) |
| M6 Raptor pack + rules | pack; rules/server/bots | done 2026-10-13 (protocol 29; `raptor.rs` in rules and sim): `--mode raptor` on 3f: teams, kills +1, bodies to the nest +1, eggs to the fire +2, the raptor's movement, jump and bites, the bone weapons, script 0xBE's Raptor branch (raptor bots never feed, as the original). Left → M6b: on the server the raptor bots score 6-9 against 11-18 in the ROM test (cavemen win every run; the oracle's raptors won b1); the pistol's head test waits for heads from the model |
| M7 Raptor client | client | Drawing, HUD, outros, sounds | done 2026-10-14 (raptor 83 with its camera and clips, the body in the jaws, the nest, baby, eggs carried and cooked, the splat; the team strip and icons; the sounds; cs2/cs4 at the oracle's 750/375; 17 screens). Left: grey eyes (faces), bone weapons and a raptor death not checked live, the egg carrier's camera |
| M8 Tank research + pack | research; pack | Scene 0x24, scenario 1: tank 119, the 4 power-ups' bits, missiles, posts, wrecks | research done (`tank.md`); pack done 2026-10-13 (map 24, the tank 119: turret joint 2, gun joint 3; the props, power-ups, shell, the sounds identified, song 32 and the countdown song 50) |
| M9 Tank rules | rules/sim/server/bots | done 2026-10-13 (protocol 28): `--mode tank` on 0x24: the hull (t1/c1/f1/p1 traces), turret, shells, deaths and wrecks, the power-ups, the bomb cycle to the frame (c1), bots on the tank's own class data `[0x0F, 119]`. Runs vs b1: bombs 4-6 (7), power-ups 21-26 (27), shell kills about twice the oracle's (the bots' fire rule is I). Left: the scope shot's damage, wreck collision, the class scripts' skipped ops |
| M10 Tank client | client | done 2026-10-13 (`tank.rs`): the tank posed from the state (turret, gun, treads, brakes, flags), wrecks, the follow camera (distances V, the rest fitted), the scope, shells and blasts, doors, the bomb, the countdown and gas, the engine, gears and sounds; the HUD is Deathmatch's plus the power-up icon. Left: per-side treads, hull tilt, engine pitch, muzzle smoke, the carried bomb's look (I) |
| M11 Race oracle + research | one agent alone (oracle rebuild) | done 2026-10-13 (`race.md`): the drop was the stopwatch left timed (the menus force Race untimed); hook `CONKER_MP_RACE=<laps>` (off by default, traces unchanged). Race: up to 8 hoverboards, 6 hp, 3 lives, 5 laps, A jump, B kick, Z power-up (boost canisters, homing missiles), two routes (11 low, 12 elevated), ends when every human finished or out |
| M12 Race pack + rules | pack; rules/sim/server/bots | pack done 2026-10-13 (map 33 with `race.json`: each route's barriers and 33-node lap, the power-ups; character 144, the board on joint 23, helmets by class, the overview maps, the sounds; no music, as the original). Rules done 2026-10-14 (`--mode race --route 11|12 --laps 1-20`: the board, the race table, power-ups, the kick, deaths and respawns, the end; script 0xBC's bots; 7-bot finishes within 2% of the oracle's; protocol 30). Left: the 3-bot route-11 laps 13% slow (missile firing), the Catmull-Rom track |
| M6b bot fidelity follow-ups | rules/bots | Why the server's raptor bots score half the ROM test's (suspects: the server's `roots`/`sink`, cavemen's friendly-fire immunity); Race bots never fire missiles (3-bot route-11 laps 13% slow); Tank shell kills ~2× the oracle's; 3-bot Heist pickups half the oracle's | done 2026-10-14 (raptors have no head: scores 11-20 vs 16-19, oracle 20-14/16-16; query 0xEF answers 0x1F for a raptor; Race bots hold Z (route-11 laps 833 vs 837); Tank's fire rule ported (shell deaths 7-9 vs ~11)). Left: Heist seats bots before humans (→ E11), route-12 3-bot laps 12% fast, Tank shells hit 1.8×, bomb deaths 21 vs 12 |
| M13 Race client | client | Drawing, HUD, sounds | done 2026-10-14 (racers with boards and helmets, the route's barriers, power-ups, missiles, the boost; the board predicted; the HUD, overview map and in-level STATS; the sounds; 13 screens). Left: the effect textures (F3), the original's camera springs and finish cameras, the death blast, the hum's pitch, the race total on the wire |
| M14 characters | research | research done 2026-10-14 (`characters.md`: picks only in Deathmatch and Race, per-map sets, 7 cheat codes, 9 new models on the Tedi's rig, hitbox/body per model) |
| M14a characters: pack | pack | Models 0, 150, 152, 159, 160, 176, 177, 178, 180 (mesh-only, the Tedi's clips); `hitbox`/`body` in every `character.json`; voices | done 2026-10-14 (9 picks mesh-only on the Tedi's rig, `clips_from`, `hitbox`/`body`/`hidden`, voices shared; +2.3 MB; Conker draws model 1; Raptor's splat sound, cs2/cs4 lengths from the 0xFE record, `hidden` for 83/165/22). Client: retarget the Tedi's clips to each pick's bones |
| M14b characters: rules/server/wire | rules/sim/server | `characters.rs` (sets, cheats, class → model, scale); bodies by class; `Join.character`, `--cheats`; the raptor in Temple DM; protocol bump | done 2026-10-14 (`characters.rs`: 22 picks, sets, cheats, `body(class)` = §3's table; bodies by class; `--cheats`, `Join.character` (a `PICKS` index), `MatchView.characters`; the raptor pick on 3f; protocol 31; Race's first match no longer runs before a human). Client must load every allowed model first: until M14c a pick off its map crashes clients |
| M14c characters: client/web | client; web | Draw every allowed model (scale, camera); `--character`; the lobby picker | done 2026-10-14 (every allowed model loaded after the map's, the side's body until then: no crash; the 9 mesh-only picks retargeted; scale 0.6/0.7 and the leaders' aim; `hidden` everywhere; the blink; any pick rides in Race; `--character`, the lobby's Character select and the form's cheats). Left: the 0xD-0xF helmets off 0x33, the 0.6 picks' lift and held size unchecked |
| F1 faces | research, pack | The eyes/mouths on segments 0x0A/0x0B (grey patches today): what sets them, the textures, `faces` in `character.json`; the client binds them in M14c | done 2026-10-14 (the eyes are segments 6/7 on every model, 0x0A/0x0B only Conker's/Neo's mouth and the team looks; the blink `rand%140+10` VIs, open-half-shut-half, frozen at death, none in outros; every glTF draws the open look; `faces` for the ten blinkers). Client in M14c: swap the materials, blend 0's and 150's eyes |
| F2 character materials' alpha | pack | Translucent and alpha-tested character materials; Colors cs5's tank lights | done 2026-10-14 (the character walker follows the render-state calls: 72 materials BLEND/MASK, back faces culled; Colors cs5's lit tank). Left: flag 169's translucent frames |
| F3 Race effects | pack | The countdown lamp flare, the boost flare, the exhaust glow, the missile trail (M13's DRAM addresses), 0x33's materials 53-57 drawn as white panels | done 2026-10-14 (lamp 0x235, boost 0xE21, exhaust 0x923-0x926, trail 0xB39 (128×1) in `hud/`; the white panels are the crowd animated on segments 4-7: bound in `level.gltf`, frames in `race.json` `crowd`). Client: swap the stand-ins, animate the crowd |
| F4 runtime segments | pack | 0x1A's 9 materials bound by `func_15100464`; a sweep of the other maps' unbound runtime segments | done 2026-10-14 (0x1A's two screens (3 frames) and five lamps (lit/dark) in `segments.json`; the other maps bind none; a combiner that reads no texel drops the texture: 31 materials in 15 characters, Conker's head now orange). Client: in F3c |
| F3c effects in the client | client | F3's and F4's handoffs | done 2026-10-14 (`segments.rs`: 0x33's crowd and 0x1A's screens on one clock, 0x1A's lamps with their lights; the boost flare, the exhaust glow, the lamps, the missile trail). Left: 0x1A's lamp/light pairing by position differs (needs a dump), the lamp quads' drift, the trail's second strip |
| L1 Heist and rules leftovers | rules/bots/server | Heist's bag takes (half the oracle's), the thrown bag through walls, item 0x3A for the grenade on 0x1A/0x3F, the dead pose's 0x18A, Race's human cap | done 2026-10-14 (Heist's takes are the oracle's spread: 4 oracle matches 30/20/17/18, ROM path mean 19.7, server 18-21; the bag swept with a 24-unit box against walls; item 0x3A the lobbed grenade in the rules and bots; the dead pose's 0x18A is a 1-in-8 twitch on a first death (not ported: the replays' RNG); Race refuses a third human). Left: 0x3A's client (models 137/186, sounds), the bots' attack share near the carrier (61% vs 75-84%) |
| L2 held leftovers | research, then pack/rules/client | The crossbow's string clips (and other held models' own clips), the head part 0xA for the pistol's head test | research done 2026-10-14 (`held-leftovers.md`: the crossbow #110 has 3 joints and 4 clips picked from the player's clip by `func_15054A94`, the loaded bolt in fire [6, 11), op 41 deletes it; the bone crossbow shares them; head part 0xA is a body-part sphere on a joint (`func_1508114C`): a crouched Tedi head-shot, a standing one missed by a level shot). Next: L2a pack (op 41, the crossbow's rig and clips, `parts`), then L2b client crossbow ∥ L2c rules/server part test; the aim laser (`func_151026BC`) |

Needs you, outside the batches: the real deployment (host and domain, "Wave 7" item 3); Firefox, Safari, background
tabs and a real gamepad (manual); split screen and the other offline options stay out.

## Gap-closing waves (planned 2026-10-14)
Goal: close every "Left" above and the open items of "Wave 8". Same rules as the batches. One pack re-exporter per
wave. Agents don't touch `PROTOCOL_ID`: they say whether the wire or the shared simulation changed, and I bump it
once per wave (with `token.rs`'s doc line). After each wave's commit, `bun run clean:cache` (from `remake/`) deletes
`target-net/debug` before the next wave starts. Visuals that need pack data in a wave where the pack is another lane's
are handoffs only. Some leftovers will close as justified approximations; the deployment and the manual browser
and gamepad tests stay with you.

| Wave | Agents (lanes) | Items | Status |
|---|---|---|---|
| G1 | pack (L2a); Tank (rules/bots + client `tank.rs`); Race (rules/bots + client `race.rs`) | L2a: op 41, the crossbow #110's rig and 4 clips (shared by #47), `parts`/`broad` in `character.json`, 0x3A's models 137/186 and sounds. Tank: shells hit 1.8×, bomb deaths 21 vs 12, the scope shot's damage, wreck collision, the class scripts' skipped ops; per-side treads, hull tilt, engine pitch, muzzle smoke, the carried bomb's look. Race: route-12 3-bot laps 12% fast, the Catmull-Rom track, the race total on the wire; camera springs and finish cameras, the death blast, the hum's pitch | done 2026-10-14 (protocol 35). Pack: `crossbow.gltf` (#110 and #47, 3 clips), op 41's removal, `parts` on 15 models, `lob.gltf` (#137, #186), 0x3A's fuse 0x96, bounces 0x186-0x188 and HUD icon. Tank: the gun's pitch nudged as the original's (hits 26.7% vs 25.6%), the hull's ellipsoid, dead tanks stop shells, C-right's sign, wrecks solid where knocked, op 0x67, the hull's tilt; bomb deaths 16.5 vs 14.8 (the old 21 vs 12 compared mismatched runs). Race: route 12's missile kills (laps 1067-1081 vs 1063-1081), the Catmull-Rom track, `RacerView.total`. Left → G3/G5: Tank's shell deaths 7.5 vs 10.5, the scope shot from the bullet pool, per-side treads (pack: split primitive 13), the engine's pitch, muzzle smoke and the carried bomb (a DL dump first); Race's camera springs and finish cameras, the hum's pitch (sounds can't change rate yet), the death blast (pack handoff), the centre-line bot dying at the chasm in half the seeds, the bots' turn 833 vs 1792, the start kick's speed, the missile's radius 120 (I); `clip_sounds`' remapped model, `MATCH_CLIPS` without 0x175/0x16C |
| G2 | client held (L2b); rules/server part test (L2c); bots | L2b: the crossbow's clips, the loaded bolt, the holster's removal, 0x3A drawn and heard (its fuse, bounces, HUD icon), the aim laser, `clip_sounds`' remapped model, `MATCH_CLIPS`' 0x175/0x16C. L2c: the pose sampler, the server's per-player poses, `func_1508114C`'s part test for the twin guns, pistol, sniper, knives and bolts; the pistol/tommy bullet functions (swapped in `game.rs`?); the bolt's wall sound. Bots: 0x30's weapon choice (deaths 4.1 vs 9), lingering in 0x34's trap zones, a 0x34 bot camping the gun, the crossbow's pitch and the sniper bot's hit rate (E2c), the roam spot picked at the respawn (M2) | done 2026-10-14 (no protocol change). Client: the crossbow's clips, loaded bolt and holster (#110, #47), item 0x3A in the hand and in flight with its fuse, bounces and icon, the bolt silent on walls as the original's, 0x175/0x16C's sounds by class. Part test: the pose sampler, the server's per-player poses (last frame's), `func_1508114C` for the twin guns, pistol, sniper, knives and bolts with the cylinder fallback (pc3's crouched head and standing miss, e_w39 19/19, e_w09 119/122); the pistol/tommy functions weren't swapped. Bots: 0x34's jets noise (jet deaths 1.0 vs 1.5, were 5.8), the mounted guns in the bot tests; 0x30 is 4.9 vs 6.0 deaths over 5 oracle seeds; the gun camping and the roam spot already matched. Left → G3/G5: the crossbow's aimed shot fires on Z's press (the original's on release, with the laser between), the laser's look (dot and beam not packed, sound 1232), the lob fuse's sparks, the thrown 0x3A drawn a few frames late; the bots hold the sniper 2× and 0x23 2.3× as long, twin-gun hits 0.032 hp a shot vs 0.073 (obj+0x1D1's aim steps), the electric floor's stand-offs, the crossbow's pitch and the sniper's hit rate (the ray doesn't count shots); the hit clip's fade-in (I); the oracle draws one frame behind its clock |
| G3 | match flow (server/rules + end screen); net details (server/client net); prediction (sim + client); bots in the modes | Match flow: a joiner with no free slot is a bot to the script, a refused Beach client stays connected, a dead bot with lives can't be taken over, the team size's up/down, STATS labels past P4, the hit-counting tests (E10). Net: Ctrl-C's disconnect, native WebTransport IPv4-only, the native token fetch timeout, tokens that survive a slow load, a renewed certificate without a restart, `/token`'s rate limit, the native HUD's wording, `--wait`'s rollback. Prediction: 0x34's plates/jets/cones, the flag's push, the gun mount, the doors' collision a frame late, the camera hand-off's correction, the local player between ticks at 60 Hz, the drawn camera's hint cut on a different tick. Rules: the crossbow's aimed shot on Z's release, the laser state on the wire. Bots in Deathmatch (G2's left): the sniper and 0x23 held too long, the twin guns' hits (obj+0x1D1's aim steps), the electric floor's stand-offs, the crossbow's pitch and the sniper's hit rate. Bots in the modes: the attack share near the Heist carrier (moved from G2), Tank's shell deaths 7.5 vs 10.5 and the scope shot from the bullet pool, Race's centre-line bot at the chasm, the turn 833 vs 1792, the start kick's speed, the missile's radius | done 2026-10-14 (protocol 36). Match flow: the crossbow already fired on Z's release (G2's claim was wrong; `combat8_xbl` 9/9), the aim laser (`Match::laser`, on 5 frames into the aim, `Combat::laser` on the wire); the hit-counting tests and headshots in `hurt`; a dying bot with lives is taken over (no health test, the lives byte as the original's); a refused client is disconnected with the reason shown; STATS rows P5+; the team size's up/down is covered by `--bots`. Net: Ctrl-C/SIGTERM disconnect (leave seen in 0.07 s), native WebTransport by the address's family, the token fetch's 4 s timeouts, certificates reloaded every 10 s without a restart (`test-reload.sh` 8/8), `/token` 10 per 10 s per address, the native HUD's wording, bots idle before a `--wait` start; tokens already survived a slow load. Prediction: 0x34's plates, jets and cones, the flags' push, the mounted gun's mount, the bolt's push, 0x2D's doors a frame earlier, the hand-off's weight, the hint cut, the local player drawn between ticks. Bots: Deathmatch holds, twin-gun and sniper hits within noise over 13 oracle seeds (no change); Tank's scope shot from the bullet pool, shell deaths 12.4 vs 11.5; Race's chasm deaths 0 (were 5 of 6 seeds), the turn 1792, the missile's radius confirmed. Left → G4/G5: a joiner with no vacant participant stays a bot to the script (the script counts once at its start; justified); the system-roots WebTransport path untested on a real certificate (deploy); the `--wait` start reset unpredicted (one rollback, harmless); observers' rollbacks on others' plate steps, others' bolts and Tank's doors unpredicted, the other players, radar and listener drawn per tick; 0x34's start records by player count (the remake's fixed `(s + 7) % 12` is right for 5 and 16 players only); the electric floor's stand-offs (527 frames a run vs 151); Tank's hits 29.3% vs 25.6%; Race bots' start kick (12 frames, no hop, speed 60); Heist's attack share 71% vs 75-84%; the scope bullet not drawn |
| G4 | research ×2 (effects; cameras, clips and outros); rules and bots (G3's left) | Rules and bots: 0x34's start records by player count, the electric floor's stand-offs, Tank's hit rate, Race bots' start kick, Heist's attack share near the carrier. Effects: muzzle flashes, the rocket's trail, the flame stream's puffs and tongues, the strike's sparks, flak, far fire, smoke, electric sparks, the sea's ripples and bubbles, Heist cs3's bubbles, the burning player off 0x34, the escapee fade's alpha, the trail's second strip, cs5's smoke. Cameras and clips: the sea's fixed cameras 0x26/0x27, the swimmer's death 0x24, 0x34's two camera stretches, the egg carrier's camera, the scope dots' start angle, the 0x18A twitch, corpses' head pops, the gun's recoil clip, e1's knock slide; outros: the villagers' head turns, the tanks' engines and op 9 || done 2026-10-14 (protocol 37). Effects (`effects-g4.md`, client `effects.rs`): muzzle flashes, the rocket's trail, blasts and smoke, the flame's puffs, 0x34's electric glows drawn; recipes for the 3D flash and tracers, the flame's tongues, the strike's sparks, flak, far fire, other smoke, the sea's ripples and bubbles, cs3's bubbles, the burning player, the trail's second strip, cs5's smoke; the escapee fade was right. Cameras and clips (`cameras-clips-g4.md`): the sea's fixed cameras (points hardcoded), the swimmer's death clip, 0x34's two stretches (eye exact over reads 336-777), the scope dots at 177.35°, the 0x18A twitch (`corpses.rs`), head pops on head kills, the scope's recoil kick, e1's knock slide 140.9 vs 140.7, the villagers' head turns, cs5's tank engines; the egg carrier uses the on-foot camera and op 9 was right. Rules and bots: the start records by `random(8)` on 0x34/0x3F and Beach's draws (10 captures), 0x34's floor noises (box frames 527 → 348 vs 232, floor deaths 1.5 → 0.5 vs 0.25), Race bots' 12-frame kick (laps matched). Left → G5: the pack exports in both recipes (0x3CF, 0x11FE, 0x721, 0xED9+, the stump models, sound 0x4A9, the sea camera points); the swimmer's respawn 88 frames after the hit (rules 163), the rules' fixed 0x18A rows, a bullet on a corpse's head, the gun's muzzle recoil; 0x34's bots fire the twin guns 302 shots a run vs 107 (hp drops 28.8 vs 22.0) and fight in the pit; Tank's hits 30.2% vs 27.6% (mid range); Heist's share near the carrier 71% vs 75-84%; several blasts at once on a restart (untested); one's own muzzle flash waits for the server's shot count |
| G5 | G5a: pack (every export below, one pass); rules and bots (G4's left); client polish without new pack data. G5b: client effects; client Tank, Race, Beach and held drawing of G5a's exports | G4's recipes and left. Polish: Tank's per-side treads, engine pitch, muzzle smoke and carried bomb; Race's camera springs and finish cameras, the hum's pitch, the death blast; the crossbow's aim laser (dot, beam, sound 1232; `Combat::laser`), Tank's scope bullet drawn and the lob fuse's sparks; song 0x8A's level, the TW/Colors radio heard by all, helmets 0xD-0xF off 0x33, the 0.6 picks' lift and held size, flag 169's translucent frames, 0x1A's lamp pairing and drift, unlit vertices dimming with the lamps, Heist's radar team colours and HUD names, cs3's backdrop short of 16:9, the blink and the bag's clock per client, 0x34's and 0x3F's background loops not sample-exact || G5a done 2026-10-14 (protocol 38). Pack (2,160 files): the world sprites, the scorch 0x4E5, a shared `effects.gltf` (the ring, sparks, flak, drops, Race's blast), `cameras.json`, sound 1193 and the laser's 1232, the head stumps, the treads per side, the carried bomb, the fuse sparks, helmets 0xD-0xF on any map; the tank shot has no smoke (the flash's life). Rules and bots: the swimmer's 88-frame drown afloat (die1), the 0x18A rows gone (e_w38 31/31), `Combat::popped` (66 traces), the gun's recoil (k2 to 9.9e-5); the parts' sink (far twin-gun damage 1.11 vs 1.09; shots 217 → 177 vs 119 a run, the old 302 vs 107 compared patterned seeds), the arctangent table, the 1.4× pickup reach; Tank's barrel and pivot (hits 29.4% vs 27.6%); Heist's mid-swing take (share 76.1% vs 75.5-83.5%). Polish: pitch on sounds (Race's hums to 2e-5, Tank's engine), Race's look-at law and a fitted eye spring, song 0x8A at 0.75, 0x1A's lamps paired by their light (40/40), seven HUD names, cs3's black clear; the radio and Heist's radar colours were right. G5b: draw the exports, the unlit vertices' flag (pack), the models on a sinking floor (6 below the parts). Left: Race's finish cameras and the lamp quads' drift (not decoded), the 0.6 picks (unchecked), the blink and bag clock per client (by design), 0x34's pit (486 box frames vs 232), twin-gun hits a shot 0.071 vs 0.108, Tank's hull turns while driving (`func_15056B08`), flag 169's XLU frames, the loops' seams. G5b done 2026-10-14 (protocol 39): the 3D flashes and tracers (183-190 then 357-371 long; the bunkers' bursts, Tank's scope bullet derived from the shot), the shared ring and the scorch on every map (grenades none, as the original's), the flame's tongues, the burning player on body joints, sparks, flak, the sea's ripples, cs3's bubbles (0x4B3, blended), Race's death blast and the trail's strip, 0x3A's fuse sparks; no blast across a restart (drawn or heard), one's own flash from the predicted shot; the aim laser and its hum (flicker averaged, `ponytail:`), the treads per side, the carried bomb on the turret, the rumble 1193 (the drive loop never fades), the sea cameras from `cameras.json` (the tunnel hints now active), the head stumps, the racer helmets anywhere, bodies drawn with the floor's sink, `_UNLIT` vertices (97 on 0x2B, as the oracle's) kept through the lamps; the Heist screens' script fixed (a joiner takes a waiting bot's slot). Left: the splash drops (the game's table maps them to #0xBE, an eyeball; how it's drawn not traced), the tanks' exhaust, the cone's and cs5's smoke and clods, Beach's far fire, the blasts' light, bullet impact sparks, the bubbles' sway, flag 169's XLU frames, the loops' seams, the tunnel hints unchecked against the oracle |
| G6 | audit; Chrome check | Every "Left" in `remake/STATUS.md` and this file re-read and closed or justified; every mode joined in headless Chrome with screens; the bone weapons, a raptor death and the 10 s connect watchdog checked in the browser; stragglers fixed | audit done 2026-10-15 (`GAPS.md`; read-only, nothing fixed): 26 things wrong today (the mouse likely can't turn a mounted gun under MODERN, 0x1A's four fixed cameras dropped, the sniper's aimed shot not counted in `shots`, a burning player not refused R, …), 60 not ported or not drawn, the inferred values, 4 decisions and the manual checks. Left: the Chrome check (yours), the fixes |

## Modern controls waves (planned 2026-10-14)
Goal: a modern third-person-shooter scheme, keyboard and mouse first, then gamepads. That means a mouse orbit
camera, a centre crosshair with pitch, separate actions, manual weapon pickup on F, rebinding and an Esc menu with
Settings. The research, the decisions and the details are in `CONTROLS_RESEARCH.md` (§1 for the decisions, §10 for
the waves). Everything new sits behind the MODERN bit in a new `Input.actions` field, and only humans set it; bots
and oracle tests keep the N64 path byte-identical. The "Classic" scheme isn't offered to players. Same rules as the
batches: I bump `PROTOCOL_ID` once per wave that changes the wire or the shared simulation.

| Wave | Items | Protocol | Status |
|---|---|---|---|
| C1 | Input plumbing: `Bindings` + `Settings` data with persistence (localStorage / `settings.json`); pointer lock with raw input and a lock-loss listener; the debug camera moves off C | no | done in R1 |
| C2a | Modern rules: `actions` + `pad()`; `aim_pitch`, the shoulder bit and humans' `m.aims`; move while aiming or firing, the facing on the aim, the scope from the aim, speeds 0.75 / 0.5 | bump | done in R1 |
| C2b | Modern client: the orbit camera, prediction with the new fields, the centre crosshair, server-confirmed hit markers | (C2a's) | done in R2 |
| C2c | Weapon handling: Interact pickup (weapons only), draw on pickup, Fire draws, jump and crouch armed, grenade armed, move while drawing, mounted guns on F, "PRESS F" | bump | done in R2 |
| C3 | Esc menu: Resume / Settings / Leave; rebinding with conflicts; mouse and pad settings; input zeroed while open | no | done in R2 |
| C4 | Polish: the pad look stick, curves and glyphs; the masked upper-body animation; spread; shoulder swap; the blocked-shot marker; aim-assist slowdown | no | done in R3 |

## Controls waves (planned 2026-10-14)
`CONTROLS_RESEARCH.md` implemented: modern TPS controls (keyboard + mouse, Xbox / DualSense), rebinding and an Esc menu.
Everything new is gated on the MODERN action bit; bots and the oracle replays stay on the N64 path, unchanged. Same
rules as the gap-closing waves (one protocol bump per round, by me).

| Round | Agents (lanes) | Items | Status |
|---|---|---|---|
| R1 | C1 client input plumbing; C2a modern rules | C1: `Bindings` and `Settings` (persisted; defaults = today's bits, crouch on C, walk on Shift), pointer lock (raw, lock loss, mouse 4/5), the debug camera off C. C2a: `Input.actions` and `pad()`, `aim_pitch` and the shoulder bit, humans' `m.aims`, the MODERN gate (move while aiming and firing, facing and scope on the aim, speeds 0.75 / 0.5) || done 2026-10-15 (protocol 40). C1: `bindings.rs` (20 actions, 2 keyboard/mouse slots + 1 pad slot, Esc and Start never bound, defaults §7 sending today's bits), `settings.rs` (overrides only, clamped, `.bak`; `$CONKER_SETTINGS` or the config dir), pointer lock on the canvas's press (`unadjustedMovement`, lock loss pauses input, mouse 4/5 blocked; unverified, no browser), native grab, the debug camera on F8, the AZERTY label cache; BALANCE moved to W/S. C2a: `Input.actions` (0x01 jump, 0x02 crouch, 0x04 fire, 0x08 reload, 0x10 left shoulder, 0x20 interact, 0x40 grenade, 0x80 MODERN; holster on B, walk halves the stick) and `aim_pitch` (±14563); `pad()` in `Match::player`, the identity at 0 (679 tests, no expected value edited); humans' `m.aims` from the arm (pivot 67.5, arm 160, shoulder 20; `camera_yaw` = aim − 0x4000); the MODERN gate (move while aiming ×0.75, scoped ×0.5, facing on the aim, the scope from the aim). Left → R2: keyboard C-left/right strafing gone until the orbit camera; pistol, 0x23 and tommy bullets take only the ray's pitch (20 to the side); C-left/right still strafe under MODERN; the mounted gun turns by the N64 scope camera |
| R2 | C2b modern client; C2c weapon handling; C3 Esc menu | C2b: the orbit camera feeding the aim, prediction, the centre crosshair, server-confirmed hit markers. C2c: §4b (Interact pickups, draw on pickup, Fire draws, armed jump and crouch, the armed grenade, moving while drawing, mounted guns F/Q, "PRESS F"). C3: Resume / Settings / Leave, rebinding with conflicts, mouse and pad settings || done 2026-10-15 (protocol 41). C2b: `orbit.rs` (C2a's arm bit for bit with `aim_ray`, slides along its look line, aim zoom 1.5×, the scope from the aim with wheel/D-pad steps; Tank, Race, raptors and mounted guns keep their cameras), windowed clients send MODERN with camera-relative WASD, hold/toggle aim, crouch and walk, every look setting; prediction no worse than the N64 path (0 error, 2 rollbacks both, LAG 75 too); `crosshair.rs` (colour and size, greyed with the wall point when the muzzle is blocked), hit markers from the replicated hit and headshot counts (no wire change); script lines take actions and look deltas. C2c: weapons only by Interact with `can_take()` and the "PRESS F" prompt (the bound key's label), the draw on pickup, Fire draws (a one-frame B pulse; held Fire fires after the draw), reload on Reload only, armed crouch and high jump, the armed grenade (holster, throw, redraw), moving while drawing, mounted guns on F and off Q turning on the aim, bullets to the ray's hit point, no C-strafe under MODERN; `Fighter`'s `actions`, `late_actions`, `throw`, `redraw` on the wire. C3: `menu.rs` (RESUME / SETTINGS / LEAVE MATCH on Esc, Start, focus loss and lock loss; the match runs on with neutral input and the yaw; never over the results), `settings_ui.rs` (4 tabs, every v1 setting, live, saved on APPLY), rebinding (5 s, the opening press ignored, triggers past 0.5, SWAP / REPLACE / CANCEL, Esc and Menu refused, AZERTY labels and pad glyphs), mouse, keyboard and pad navigation. Left → R3: the N64 reticle while aiming, the radar and listener on the N64 camera, pad X both Reload and Interact, the hit marker lost in held fire's flash, the gun across the view's centre while aiming, the shoulder ray inside a wall and the muzzle check walls-only; no mounted-gun prompt, a grenade tap during a draw lost, Fire one frame after the draw; no reset to defaults, one binding context; untested: the browser paths (lock loss, re-lock, localStorage, LEAVE's page), real pads, AZERTY |
| R3 | C4 client feel; C4 rules spread | Pad look stick, curves, glyphs, vibration; masked upper-body aim; shoulder swap; the blocked-shot marker; aim-assist slowdown (pads); hip/aim/scope spread || done 2026-10-15 (protocol 42). Client: the N64 reticle hidden while aiming, the radar and listener on the orbit camera, pad X tap (< 0.25 s) Reload / hold Interact, sub-tick presses always sent, the hit marker outlined, "PRESS F TO MOUNT", one key-label helper, DEFAULTS per settings tab; `upper.rs` (the legs' gait under the gun's aim pose, the spine 0.35 of the pitch and the shoulders the rest; the drawn head within 2.5 of the spheres at ±12°, 17 at 80°); the shoulder swap eased over 0.2 s with the ray switching at once; pads: radial dead zone 0.10 / 0.95, the edge ramp to 1.75× after 0.3 s, aim-assist slowdown 0.4× over an enemy (pads only), vibration on hits (gilrs: native Windows/Linux only), pad glyphs in every prompt. Rules: `spread.rs` (hashed from tick, slot and shot count, never the match's RNG; hip/aimed cones per weapon, `ponytail:` guesses; twin guns hip at 800 48 hits / 28 heads per 100, pistol aimed at 1500 95/88, sniper scoped strafing 50/41), the crossbow's hip bolt on the crosshair under MODERN (no lock-on; bots keep it), the aim ray pulled 8 short of a wall at the shoulder (the client's arm too), a grenade tap kept through a draw, Fire on the draw's last frame (V ctlw_k5). Left: the tommy gun's climb (camera kick?), leading a moving target into empty space, the sniper's aimed shot not counted in `shots`, the layered pose's one-frame switch, real pads and every browser path untested |

## Fix batches (planned 2026-10-15)
Bugs found while playing, fixed before the rest of `GAPS.md`. Same rules as the controls waves.

| Batch | Agents (lanes) | Items | Status |
|---|---|---|---|
| F1 | rules; mounted gun; menu and settings; bullets | The mounted gun under MODERN (mouse turns the gun, not inverted); the sniper's aimed shot counted; the bazooka firing while aiming; Esc closes the menu; a wheel zoom speed setting; which weapons draw a bullet | done 2026-10-15 (protocol 43). Rules: the sniper's ray adds to `shots` (everyone); MODERN's rocket fires with the aim held (N64 still refuses Z with R held, V combat4_w16). Client: the seated look turns from the mouse and right stick, held to the gun's 60° up / 50° down, the sight drawn from the local aim (the server already turned the gun from the aim; the client froze it and sent the mouse as an N64 stick, inverted); Esc at the menu's root resumes on the web too, then "CLICK TO CAPTURE THE MOUSE" and no mouse look until the click; WHEEL ZOOM SPEED 1-10 (default 4 ticks per notch); the shotgun's spray (V) and the pistol's and 0x23's longer tracers (V). Answer: the sniper (a ray), the shotgun (spheres) and melee draw no bullet in the original either; a hit within 300 draws no tracer in either. Left: every browser path, the feel with a real mouse and pad, own tracers wait a round-trip (snapshots only), the sniper's laser and impact sparks |
| F2 | rules | MODERN: a default weapon drawn at spawn; with no weapon, the first one walked over picked up without F | done 2026-10-15 (protocol 44). The start item (0x3F's and `--cavemen`'s club, 0x1A's bat, Raptor's cavemen's club) drawn when handed over, at the start and after each respawn, through the pickup's redraw (Beach's 0x3E and the raptors' club never drawn); `can_take` skips Interact while nothing is held (the server's gate and the PRESS F prompt alike); a start item and the gas mask count as held, so F is still needed there. Left: seen in a running client |

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
  (the client needs `hud/hud.json`).

## Missing compared with the original's multiplayer (checked 2026-10-11)
(2026-10-15: superseded by `GAPS.md`, the G6 audit. Most items below are done; the open ones are listed there.)
Evidence: `lab/research/mp-original/` (menus-and-flow.md, modes-and-maps.md). Our map 0x2D is the **Total War** map:
scenario 9 = Total War, 10 = Colors (scene 0x30), the reverse of the earlier notes (names fixed in the code and docs).
**Done 2026-10-11:** the SET-UP options, the end conditions and the STATS results with RESTART/QUIT; Deathmatch on
0x30 (`--map 30`), on 0x34, the Bunker (`--map 34`), and on 0x3F, the Temple (`--map 3f`); the crossbow (0x18) and
shotgun (0x37). Leftovers:
- results: the skull's grow; the level still drawn in an eliminated player's view; the lit button's pulse; the
  radar/score hidden during the fade; the 17-frame fade before RESTART; when the original counts a hit (accuracy);
  P3/P4 row colours;
- 0x30: 1-2 bots per match ride a pair of lifts back and forth all match (the original's wander between rides); the
  2 mounted machine guns (usable in the original's Deathmatch: mount, Z fires); `bots.sh` fails on this bigger map
  (not every bot hits in time); camera script zones and volumes (as on 0x2D);
- 0x34: the mounted gun; in a corridor near start record 9 the camera sits steeply above the player (not compared
  with the original); a bot can loop jumping between
  nav nodes 60-62 for most of a match; surface kind 0xE's extra call and trigger kinds 12 and 17 are unread; its
  background loop is not sample-exact (song 36 never repeats exactly); its boxes (placements 6-9, dispatch 2) stand
  10 lower in the game while stood on, static here;
- 0x3F: trigger kinds 5 and 18 and the per-frame music volume
  call are unread; the background loop is one 191.8 s loudness period of song 84 (not sample-exact); the oracle crashes
  31 frames after its human dies in scenario 7 (HUD sprintf; 0x2D's scenario 3 too, at read 485 after a bot's
  death); ~~per-map wall placements and hidden props~~ (done);
- crossbow and shotgun: the crossbow's hip shot moves the shooter about 5.3 units a shot in the game (not modelled);
  the held crossbow doesn't animate (string) and its bolts use the sniper's flight sound; the shotgun's point-blank
  sphere never hit in the traces; `--first-read` replays land bolts one frame late;
- the pack: the map exports, the weapons' port and the fixes rewrote `pack.json`, `audio/events.json`,
  `audio/sounds.json`, `hud/hud.json` (additions only), `weapons/deathmatch/held.{bin,gltf,json}` (14 models, new
  order), `maps/*/collision.json` and `maps/{2d,30}/level.gltf`, so pack edits in those files would need re-applying.
  `assets/` equals a fresh four-map export (929 files).
- caveman and club (done, `--cavemen` or 0x3F): the club is drawn 21% of the time by the bots (the game's 52%); the
  caveman's own grenade clips (the Tedi's play); the club's HUD icon one pixel left; a lethal first swing untraced;
  nobody listened to the club and caveman sounds;
- bots: ~~per-map AI scripts~~ (done: `maps/<id>/ai.bin`, the scripts' missing functions, the AI memory grown to
  0x26B0; query 0xF0 now answers the ranking place, which also changes 0x2D). Left: bots don't climb ledges (0x30's
  sniper node 48); with few bots they camp far longer than the game's (a role changes only on death or the leader
  rule); the trigger tests' camera part always answers outside;
1. Around the match:
   - ~~the end of a match and a STATS screen with RESTART and QUIT~~ (done);
   - ~~SET-UP: time 3-10 min or unlimited, lives 1-5 (last one standing when untimed), kill limit, bot skill
     (5 levels), radar on/off~~ (done, as server flags);
   - pause (CONT./RESTART/QUIT): not online (it would freeze everyone);
   - bot caps by human count;
   - real lobby wiring.
2. Deathmatch's other maps, cheapest first: ~~0x30 (no new weapons)~~ (done), ~~0x34 Bunker~~ (done, with the crossbow, shotgun and mounted machine gun), ~~0x3F Temple~~
   (done, with the caveman and item 0x40), 0x1A Vault (+Wise Guy, tommy gun, item 0x19, moving pickups). Per-map character sets and
   cheat characters. The bots' navigation on those maps is unchecked.
3. Team modes:
   - common: team screen with BALANCE, intro and outro cutscenes, team results;
   - in order: Total War (gas masks, canister, gas; same map), Colors (flags, score 1-40), Beach (Frenchys vs Tediz,
     turret, escape), Heist (4 teams, money bag, vault), Raptor (raptor rig, eggs, baby);
   - the win rules aren't in the code found yet: trace them in the oracle first.
   - **Total War done (2026-10-12, `--mode total-war`):** teams, friendly fire, team end and winner, the bots'
     team branch, team HUD strip, radar colours, team STATS (part 1); the gas (part 2): the AI script drives it as in
     the game (its natives became effects the server applies), the two canisters (carried at the 0x25 speed, 80%),
     trigger 5's 10 s countdown and HUD digit, the doorways' damage, the gas kill credited to the deliverer, the 6
     masks, the doors and covers, the gas loop and pickup sounds; the gas visuals (the gassed haze 0xE013, the
     shelter's red alarm light and the canisters' green lights, 0xE016/0xE017: the original has no gas cloud), the mask
     put-on/take-off clips (0x151/0x14E, bank-09 masks #66/#67), the victory outros (cs2 squirrel officer, cs4 stitched
     Tedi: camera, actor, fade, bubble subtitles, voice, from `[6, 0x2D, 2|4]`). Left: the team screen and BALANCE, a
     human out of lives taking over a team-mate bot, the forgiven traitor's +0x70 reset, ~~both teams out on one frame~~
     (done 2026-10-13: team 0 out first, as the game checks it first);
     the shelter lighting is an approximation (the level's light table isn't found); overlays (haze, fade) blend in
     linear light, a little stronger than the N64's; the outro actors' head nods, talk flag, bubble grow and page fades,
     cs4's attachment; the mask clips cut a turn short; a carrier can jump; ~~a join or leave mid-match restarts the
     script~~ (done 2026-10-13); the bots never carry a canister (nor did the game's in 620 s). Test-only `--warp SLOT:READ:X,Y,Z`.
     The oracle needs `CONKER_MP_OPTS=-48=2,-24=4,-23=5` for scenario 9's teams.
   - **Colors mostly done (2026-10-12, `--mode colors --score N`):** the flags, takes, drops, returns, captures,
     score and end from the level's AI script 0xBB; the victory outros (cs1/cs5); the flags drawn at home, dropped and
     carried, with their cloth clips; the HUD strip's captures, the blinking carried-flag icons, the radar's flag
     blips, the capture fade and the take sounds; the bots take and capture (pickup reach from the script's 0x7B).
     The mounted machine guns (0x30 ×2, 0x34 ×1) are done too (mount, the gun sight, fire, overheat, dismount,
     bots on them, Colors recreating them; drawn with the gunner's HUD). Left: the capture jingles, the touch radius (45, I), the
     ~~end-phase fade~~, ~~a join or leave restarting the script~~ (both done 2026-10-13), ~~untimed + unlimited never
     ending~~ (the original's too: its SET-UP allows it, kept).
   - **Beach playable (2026-10-13, `--map 2b --mode beach --score N`):** villagers (14 looks, item 0x3E's speed and
     the bottles) vs at most 2 Tediz (the gun, the sniper and bazooka zones); escapes, the 90 s deadline, villager
     deaths and escapes scoring, the plunger, the strike, from the level's AI script 0xB9; the HUD strip and countdown,
     the escape fade, FRENCHYS WIN, the outros (cs4/cs3), the radio, detonation and strike effects, the lamps and
     water; rockets along the aim. Left: half the bot matches still end later than the original's (more escapes,
     V one run); the lamp flicker, the sweeping ground lights, the strike's ground mark, the outros' masks and song;
     the music's two positional channels; ~~the cap of 8 villagers~~ (done 2026-10-13).
4. Vehicle modes: Tank (tanks, 4 power-ups) and Race (hoverboards, laps 1-20, 2 tracks). The oracle's Race returns
   to the bar after about 1 s, so its start needs fixing first.
5. Not needed online: split screen, 4:3/16:9, hi-res, language, AI-VIEW.

## Wave 7: visuals, browsers, the real deployment
(2026-10-15: superseded by `GAPS.md`, the G6 audit. Most items below are done; the open ones are listed there.)
1. **Visuals (done 2026-10-13, batch E8; left: the muzzle flashes, the rocket's trail, the flame stream's puffs and
   tongues, `remake/STATUS.md` "Visuals: projectiles, flames…"):**
   - ~~flame particles~~;
   - ~~bullet, grenade and rocket models (placeholder spheres today)~~;
   - ~~the gun twist on the model~~;
   - ~~the white quad near record 1~~;
   - ~~the twin guns' 3 untextured materials (partial texture loads, an extract-gbi limit)~~.
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
4. **Wave 6's small leftovers (done 2026-10-13):**
   - ~~the bots treat a free slot between occupied ones as a human who is always dead. `Bots::new` takes a count, so
     the fix is a conker-bots API change that lets the roster skip free slots~~;
   - ~~a per-slot lift reset in conker-bots' `Lifts`. Today the server rebuilds every lift when a joiner takes a free
     slot, so nobody can take a lift for one tick; `Lifts::remove` is unused~~;
   - ~~remove the unused `DEV_PRIVATE_KEY` from conker-sim `net.rs`~~.

## Wave 8
(2026-10-15: superseded by `GAPS.md`, the G6 audit. Most items below are done; the open ones are listed there.)
- Background tabs (throttled tabs and the 3 s timeout) and a real gamepad with two windows.
- Smoothing details:
  - ~~a correction during the camera hand-off restarts its weight (a view jump of up to about 60 units, not seen yet)~~ (done 2026-10-14);
  - ~~the drawn camera's hint cut on a different tick~~ (done 2026-10-14);
  - ~~draw the local player between 30 Hz ticks on 60 Hz displays~~ (done 2026-10-14: the body and the view, by the fixed clock's overstep).
- Leaving and joining details:
  - ~~Ctrl-C in a terminal skips the disconnect~~ (done 2026-10-14: SIGINT, SIGTERM and SIGHUP, client and server);
  - ~~native WebTransport with a digest is IPv4 only (lightyear binds `InAddrAnyV4`; wave 6's `ca=` path is
    dual-stack)~~ (done 2026-10-14: the server address's family);
  - ~~a waiting player's lethal hit still counts for its attacker~~ (done 2026-10-13);
  - the 10 s connect watchdog is untested in a browser.
- Held weapons: hide the weapon on the game's frame at death; C-down with the twin guns or flamethrower drawn; check
  the held knives and revolver from the front.
- Deployment details, from wave 6:
  - ~~reload a renewed certificate without restarting the game server (today each renewal ends the matches in
    progress)~~ (done 2026-10-14: the server watches the files);
  - ~~tokens that survive a slow load (they last 60 s from the ROM pick)~~ (done 2026-10-14: fetched after the
    load since the asset-pack wave);
  - ~~natively: a connect timeout for the token fetch (plain http)~~ (done 2026-10-14); WebTransport without a
    digest or `ca=` now checks the system's roots, untested against a real certificate;
  - ~~rate-limit `/token` if abuse appears~~ (done 2026-10-14: 10 per 10 s per address). The endpoint answers one
    request at a time.
- ~~The native HUD's wording (`Reload to reconnect` is the browser's), and its `connecting…` state~~ (done
  2026-10-14).
- Test setup: with `--wait`, every page takes one rollback at the match start (the start's reset, which the client
  does not predict: harmless, 2026-10-14), ~~and the bots hit humans before the start~~ (done 2026-10-14).

## After wave 5
(2026-10-15: superseded by `GAPS.md`, the G6 audit. Most items below are done; the open ones are listed there.)
1. Whichever of A or B wave 5 did not take.
2. HUD and lobby: ~~the game's own HUD~~ (done), a scoreboard, the join flow, spectators, the match-end screen (scene 0x1D).
3. Audio: MP3 voices, then the sound banks, triggered by combat events.
4. Other modes: War: Total War first (scenario 9, same map; start records and masks known; the code still calls
   it `COLORS_*`), then Colors (scenario 10, scene 0x30), Beach, Heist, Raptor, Tank, Race, each with its own maps and rules, traced from the original the same way.
5. More characters and maps (each mode's teams, classes and scenes).
6. Beyond the original: more than 4 players, matchmaking or room codes, rollback-safe drop-in for spectators.
7. Fidelity details: 3-VI frames, the render-only camera offsets, segment-8 render state, fog and sky from the game,
   the level's duplicate geometry; consolidate the duplicated tables and world queries into conker-sim; refresh the
   stale lab notes.
