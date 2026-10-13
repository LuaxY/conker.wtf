# Conker multiplayer asset pack

This folder holds what a multiplayer match needs from *Conker's Bad Fur Day*: game data as JSON,
models as glTF with PNG textures, sound as WAV files, the HUD as PNGs and JSON, and two script
tables as raw bytes.
`conker-extract pack` wrote it from the ROM named in `pack.json` (and each `audio/maps/<map>/background.wav`
from a recording of the game, `--background <map>=<wav>`); the game server reads it and serves it to the
browsers, so nobody needs a ROM to play. It is Rare's data: keep it private, and out of the remake's
source tree.

You may edit any file. A pack whose files don't parse is refused at load, with the file and the field
in the message. `conker-extract pack` refuses to overwrite a pack, so your edits are safe; with
`--force` it writes a fresh one over it. `--map 2d --map 30 --map 34 --map 3f --map 2b` exports the five maps (default: 2d only).

## Layout

| Path | What it is |
|---|---|
| `pack.json` | `format` (the pack format; a server reads only its own), `source_rom_sha1` (for reference), the folders under `maps/`, `characters/` and `weapons/`, `audio`: the folders under `audio/` that hold sound (absent in a pack without sound), and `hud`: whether the pack has `hud/` |
| `maps/<map>/level.gltf`, `level.bin`, `textures/*.png` | A map as the client draws it: a node per model instance (the scene's own segments, then its placements, less those the map's Deathmatch hides). `<map>` is the scene in hex: `2d` (scene 0x2D, the Deathmatch map), `30` (scene 0x30, the original's Deathmatch scenario 5 and Colors' map), `34` (scene 0x34, the Bunker, Deathmatch scenario 4), `3f` (scene 0x3F, the Temple, Deathmatch scenario 7 and Raptor's map), `2b` (scene 0x2B, the Beach, War: Beach's map, scenario 8) |
| `maps/2d/props.gltf`, `props.bin` | War: Total War's props the Deathmatch hides: scene 0 the shelter's doors (placements 0, 1), the gas masks (21-26) and canisters (27, 28) in place, nodes named as the level's; scene 1 the canister at the origin (in a carrier's hands). Textures in `textures/` with the level's |
| `maps/<map>/collision.json` | What players stand on and run into, in the game's build order |
| `maps/<map>/spawns.json` | The multiplayer spawn records (20 on 0x2D, 25 on 0x30, 12 on 0x34, 16 on 0x3F, 12 on 0x2B) |
| `maps/<map>/pickups.json` | The weapon and grenade pickups (19 on 0x2D and War: Total War's 6 gas masks, 17 on 0x30, 16 on 0x34, 10 on 0x3F, and on 0x2B War: Beach's 2 vitamin bottles, which give the villagers' item 0x3E) |
| `maps/<map>/triggers.json` | The scene's trigger records: camera volumes and hints, bot zones, lifts |
| `maps/<map>/nav.json` | The bots' navigation graph |
| `maps/<map>/ai.bin` | The bots' AI script for the map (game bytecode, raw): each map has its own |
| `maps/<map>/lights.json` | The level's lights the client draws: War: Total War's gas lights on 0x2D, War: Beach's night lights on 0x2B, none on the others |
| `maps/<map>/outros.json` | The War victory outros: Total War's on 0x2D (the original's cutscenes 2 and 4), Colors' on 0x30 (cutscenes 1 and 5), Beach's on 0x2B (cutscenes 4 and 3), none on the others |
| `maps/<map>/outro.gltf`, `outro.bin` | The models the outros show (2d, 30, 2b): glTF scenes named `09-<entry>` (hex: a bank-09 attachment, in its joint's frame, character model units) and `prop-<placement>` (a placement the level leaves out, at the origin: the outro places it). Textures in `textures/` |
| `characters/117/character.gltf`, `.bin`, `textures/*.png` | Character 117, the Tedi: the Tediz side's player (and every Deathmatch player in the original). Skinned mesh, 28 joints, 764 animations |
| `characters/128/character.gltf`, `.bin`, `textures/*.png` | Character 128, the squirrel soldier (SHC): the squirrels side's player. The same 28-joint skeleton with its own proportions, and the same 764 animations |
| `characters/136/character.gltf`, `.bin`, `textures/*.png` | Character 136, the caveman (class 0xB): the bots of scene 0x3F's Deathmatch. The same skeleton and animations |
| `characters/156/`, `characters/157/` `character.gltf`, `.bin`, `textures/*.png` | Characters 156 and 157, the Beach's villagers (War: Beach's team 0; classes 0x11-0x1C the male 156, 0x1D-0x20 the female 157), packed with map 2b. The same skeleton and animations, with a `character.json` that has their `looks` |
| `characters/135/`, `characters/40/`, `characters/88/`, `characters/90/` `character.gltf`, `.bin`, `textures/*.png` | Characters 135 (a squirrel officer), 40 (a Tedi tank), 88 (a squirrel soldier) and 90 (a stitched Tedi): the outros' actors, each with the clips they play (`outros.json` says which); no `character.json` |
| `characters/169/`, `characters/168/` `character.gltf`, `.bin`, `textures/*.png` | War: Colors' red flag (169) and green flag (168), packed with map 30: 9 joints, the cloth's 4 clips (glTF animation n is pair n: 3 standing, at home or let go of; 0 carried; 1 the carrier walking; 2 running), drawn at half their model units like the players; carried, in the carrier's right hand (`joint_09`); no `character.json` |
| `characters/138/` `character.gltf`, `.bin`, `textures/*.png` | The mounted machine gun, packed with maps 30, 34 and 2b: 12 joints, drawn at half its model units like the players, turned to the gun's yaw. `joint_03` the seat (the gunner sits there); `joint_04` the barrels' and the camera's pivot, which takes the pitch (the gunner's eye is `joint_04` × (0, 116, 130)); `joint_05` and `joint_07` the barrels, their muzzles at (13, 0, 200) and (−13, 0, 200) from them; `joint_06`, `joint_08` the barrel tips; `joint_09`-`joint_11` a side part. One glTF animation, `pair_0000_anim_0001` (logical clip 1): the 12-frame fire loop, played while the gun fires. `textures/character-15.png` (material 15) is the ammunition belt, which scrolls along its S while the fire loop plays: −40 × the frame's VIs per frame on map 30, in the game's texture-scroll units (I: 1/32 texel); map 34's belt scroll is not exported. No `character.json` |
| `characters/<id>/character.json` | Its model lift, one entry per glTF animation, the root-motion clips, and the villagers' looks |
| `weapons/deathmatch/held.gltf`, `.bin`, `textures/*.png` | The weapons and the grenade in a player's hand (14 models: the twin guns are two), and War: Total War's gas mask in the hand and on the head: glTF scene `i` is `held.json[i]` |
| `weapons/deathmatch/held.json` | For each: the joint it hangs from and the animation events that show and hide it |
| `game/acos.json` | The game's arc-cosine table |
| `game/sink.json` | The sink table: how far players sink into each kind of surface |
| `game/class.bin` | The characters' behaviour scripts (game bytecode, raw) |
| `audio/sounds.json` | Every sound a match plays, by the game's sound id: its WAV file and how the game plays it |
| `audio/events.json` | Which game event plays which sound, at what volume, from where |
| `audio/common/*.wav` | Sounds anyone makes: weapons, footsteps, hits, pickups, explosions (16-bit mono PCM, the game's own rates) |
| `audio/characters/<id>/*.wav` | Sounds only that character makes: its voice |
| `audio/maps/<map>/*.wav` | The map's own sounds: `background.wav` (the background noise, a recording of the game's own output, stereo, looped whole) and, on 0x2D, its two lamps' buzz (0x2B's two lamps play the same sounds, kept there) and the outros' voice lines (`voice-b1.wav`, `voice-b3.wav`: the game's MP3s, bank 0x16 entries 0xB1 and 0xB3; on 0x30 `voice-d0.wav`, entry 0xD0; on 0x2B `voice-163.wav` and `voice-161.wav`, and `voice-1a8.wav`, War: Beach's escape radio, and `song-8a.wav`, Beach cs4's song (a recording, `--song`); on 0x30 `jingle-1a4.wav` and `jingle-1a7.wav`, War: Colors' capture jingles, entries 0x1A4 and 0x1A7) |
| `hud/hud.json` | What the client draws over the view: where each part of the HUD goes, its pictures, colours and rules |
| `hud/*.png` | The HUD's pictures, the game's own textures: `atlas.png` and `atlas2.png` (weapon icons, the chocolate health pieces), `grenade.png`, `radar.png`, `reticle.png`, `scope.png` and `scope_dot.png`, `iris.png` (the death circle; War: Beach's strike flash), `rank_st/nd/rd/th.png`, `teams.png` (War's team icons and Colors' flags), `beach_teams.png` (War: Beach's villager and Tedi heads), `haze.png` (its gassed screen), `bubble.png` and `tail.png` (the outros' speech bubble), and a mounted gun's gunner view: `gun_corner.png`, `gun_tick_side.png`, `gun_tick_end.png` (its sight, with `reticle.png`), `gun_tape_heading.png`, `gun_tape_elevation.png`, `gun_handle.png`, `gun_handle_lit.png` and `gun_flash.png`; the explosions' sprites, not drawn yet (`explosion.png`, `smoke.png`, `fire.png`, and `shell.png`, War: Beach's falling shell) |
| `hud/font/NN.png` | The game's font, one PNG per character (`hud.json` `font` says which) |
| `hud/end.json` | What the end of a match draws: the lives left (untimed matches), the skull over the view of a player out of lives, the results (STATS) and their RESTART / QUIT menu |
| `hud/` `lives.png`, `skull.png`, `stats.png`, `restart.png`, `restart_lit.png`, `quit.png`, `quit_lit.png`, `stick.png`, `stat_icons.png`, `stat_deaths.png`, `players.png`, `squirrels.png`, `tediz.png`, `frenchys.png` (War: Beach's villagers), `you.png`, `win.png`, `lose.png`, `draw.png` | `end.json`'s pictures, the game's own textures |

## Units and conventions

- **Distances** are game units, in the original's world coordinates, **y up**. The map spans about
  7,000 × 13,000 units; a player is about 90 tall. glTF files use the same numbers (1 glTF unit =
  1 game unit).
- **Collision coordinates** are exact 32-bit floats. Most are whole numbers and are written as
  integers; the rest are written as the shortest decimal that reads back to the same float. The game
  rounds them to integers, but the face normal comes from the exact values.
- **Facing angles** in `spawns.json`: `yaw` is 1/256 of a turn, and the game's 16-bit facing is
  `yaw × 256`. The game moves along (cos a, −sin a) in (x, z).
- **Trigger angles** (`pitch`, `yaw` in `triggers.json`) are degrees.
- **Time** is 30 Hz ticks: clip durations, event times.
- **Characters** are in model units. The game draws a player at half scale, lifted
  `0.5 × model_lift` above its position (`model_lift` in `character.json`).

## The files

### `maps/<map>/collision.json`
A list of runs. Each run has a `layer` and its `triangles`:
- `"terrain"`: the level's ground and walls. Each triangle carries a surface word (below).
- `"secondary"`: a second wall layer.
- `{"placement": N}`: the collision of placed model N, already in world space: only the placements the map's
  Deathmatch collides with (not its pickups or hidden props), and on 0x2D the shelter's doors (0, 1), which only
  War: Total War's match shows. Each is a floor and a wall object; the game moves War: Total War's doors and
  covers (0-7) with their props, and takes them away while hidden.

Each triangle is `[[x, y, z], [x, y, z], [x, y, z], surface]`, where `surface` is `"0x…"` or `null`.
The point order sets which side faces up (the normal is (p1 − p0) × (p2 − p0)).

**Keep the order:** the game numbers triangles in this order, and the server and every client must
agree on it. Each placement's run is one wall object; it must stay contiguous.

Surface words: bits 0-4 are the kind (it indexes `sink.json`; on 0x2D 0, 1, 3, 4, 5, 9 and 0xA,
where 0xA slows players; 0x30 adds 0x11, which only has its own footsteps; 0x34 adds 6, with its own footsteps, and 0xE, silent, where the sink table puts players 100 units down; the original's extra call on it is untraced; 0x3F adds 7, the stream bed in the west, with its own footsteps, which halves players' speed and sinks them up to 22 units, less when running). Bits 28-30 are the slide class (6: an idle player slides down).

Collision is JSON, not glTF: a glTF split by surface word would lose the build order, and a float
written in text reads back exactly here.

### `maps/<map>/spawns.json`
One record per spawn point, in record order (the match rules pick them by index):
- `position`: integer x, y, z.
- `yaw`: facing, 1/256 turn.
- `camera`: the camera eye at (re)spawn.
- `byte_06`, `byte_14`, `byte_15`: kept as the game has them. `byte_06` is an angle in 1/256 turn.

### `maps/<map>/pickups.json`
The pickups in the game's weapon-manager order:
- `placement`: the placed model that draws it: the level glTF's node named `…@N` (its
  `extras.placement` is N too). The client hides that node while the pickup is taken.
- `gives`: `{"weapon": id}` (the weapon id in decimal: 9 is 0x09, the twin guns), `"grenades"` (+3) or
  `"mask"` (a gas mask: War: Total War only, Deathmatch leaves them out; their placements 21-26 are
  hidden props, not in the level).
- `position`: where a player touches it, within 100 units horizontally.

Move a pickup by changing its `position` and the matching level node together. On 0x30 and 0x2B the
game moves its pickups from their placed models' spots (by 2 to 344 units; 0x2B's bottles about 70,
mostly up); `position` and the level nodes are where the game moved them. 0x2B's bottles give item
0x3E (`{"weapon": 62}`), which no Deathmatch rule knows yet; the game brings them back after 60 VIs,
not the 240 of the others.

### `maps/<map>/triggers.json`
The scene's trigger table. These records are 0x34 bytes in the game, and each record keeps every
field:
- `center` (bottom centre) and `size` (half-width, height, half-depth); `pitch` and `yaw` in
  degrees.
- `off`: 1 = switched off.
- `kind`: 0 camera hint, 1 camera volume, 3 zone, 8 lift.
- `shape`: 0 upright cylinder, 2 oriented box, 3 a plane crossed one way (the hint's trip wire, the lifts' gates).
- `flags`: bit 0, only while standing on the floor.
- `action`:
  - a camera volume: 0 turns the camera's wall checks off, 6 on, 2 sets a ceiling;
  - a hint: its kind;
  - a lift: the target scene (0x2D = 45, 0x30 = 48, 0x2B = 43; 0x34 and 0x3F have none).
- `arg`: a lift's target spawn record.
- `command`: a hint's camera command (30 cuts behind the player).
- `word_1c`: kept as is.

Zones of kind 3 with action 11 are crossed straight by the bots' paths. On 0x2D, records 88 and 89 (0x58,
0x59) are where the camera's sweep skips placed objects; that is the map's camera script, which refers
to them by index, so keep the order.

### `maps/<map>/nav.json`
The bots' waypoints, in index order:
- `pos`;
- `band`: the height band;
- `id`: the AI script names a node by this id;
- `half`: half the walkway's width;
- `next`: up to 5 neighbour indices, 255 = none;
- `barrier`: 1 marks a wall line, which bots may not cross, and is never walked to.

There are at most 255 nodes.

### `maps/<map>/lights.json`
The original's renderer lights some of the level (on 0x2D its tunnels, towers and the shelter
under the middle; the client draws the rest as the vertex colours say): a lit point's colour is its
vertex colour times `ambient` plus, per light, the light's `rgb` times min(1, size × 2048 / d²), d
units away. The level's own light table is not exported; this file holds the lights War: Total
War's gas switches, which the client draws, and on 0x2B War: Beach's (its night's ambient, two
white lamps that flicker and the exits' two red lights pulsing):
- `at`: where it is (unused for `carried`), `rgb` its colour;
- `size`, `swing`, `step`: its size is `size + swing × sin φ`, φ stepping `step` (radians) a
  frame; the light reaches `362 × √size` units;
- `when`: `calm` (outside the gas countdown: the shelter's lamps; on 0x2B, which has none, the
  whole match), `countdown` (during it: the shelter's red alarm), `{"shown": N}` (while the prop on
  placement N is shown: a gas canister at home), `carried` (on each player carrying a canister);
- `flicker` (if present, true: the lamps of 0x2D's shelter and 0x2B): it flickers in bursts
  (light behaviour 4): each frame while steady a burst of 1-5 flashes starts with probability
  0.005; a flash puts the size to 0, waits `rand × 28 + 1` frames (counting down while ≥ 0), then
  grows `size / (rand × 12 + 8)` a frame back to `size`, and the next flash starts; each flash's
  start plays the map's emitter at the lamp (its buzz).

### `maps/<map>/outros.json`
The War victory outros, one per winning team (`winner`: 0 the squirrels (on 0x2B the villagers), 1
the Tediz; a DRAW plays none), from the original's cutscene `cutscene` of the map's scene (Total
War's 2 and 4 on 0x2D, Colors' 1 and 5 on 0x30, Beach's 4 and 3 on 0x2B; the tanks' smoke and
texture swap are not exported). Times `t` are the cutscene's clock, 60 a second (2 a tick): tick k of the
outro shows t = min(2k, `length`), a command at t from the first tick with t ≤ 2k (tick 1 at the
earliest); the results come up after it, over its last picture. Fields an outro doesn't use are left
out (Total War's are the simplest case).
- `camera`: the eye moves along `path` (one point or more, Catmull-Rom, by arc length) by `along`
  (0-100 %), looking along `yaw` + 90° and `pitch` (degrees); 50° vertical field of view. Each
  channel is keys `{t, value, catmull}`: Catmull-Rom to the next key when `catmull`, else linear;
  the value holds outside the keys. `cuts`: the cameras it cuts to, each from its `from`;
- `actor`, then `cast`: the characters on screen (`character` under `characters/`, its glTF's
  `animation`, `ticks` long), standing at `at` facing `facing` (the game's 16-bit angle), or
  walking `path` by `along` and turning by `yaw` (facing = `yaw` + 90°), drawn `scale` big and
  `lift` model units up. Each plays its clip from tick 1, `rate` clip ticks a tick, looping, or
  `once` (it holds its last tick), then each of `plays` from its `t` (the same clip again goes on
  where it was; another cross-fades from the old one's pose over `blend` ticks: its weight 1023
  less 2 × (500 / `blend`) a tick from the switch). Optional:
  - `head`: from `t` the `joint`'s own rotation is pitch −`pitch` and yaw `yaw` (degrees, keys as
    the camera's; the game's rot(−pitch, yaw, 0)), in place of the clip's;
  - `talk`: the `jaw` joint, each of `says` from its `t`: `state` 0 shuts (openness −60 a tick, of
    255), 1 chatters (±40-59 a tick between 0 and 255, a new random speed at each end), 2 opens
    (+60 a tick, held); the jaw turns openness / 255 × `degrees` about x and, with `skew`, a random
    −10..9 × 1.41° × openness / 255 about y (picked when shut);
  - `mask`: display lists hidden (bit k hides the primitives whose `extras.list` is k);
  - `attach`: from each `t`, model `09-<entry>` of `outro.gltf` drawn in `joint`'s frame (`on`), or
    no longer;
- `fade`: the picture fades in from `rgb` over `duration`: alpha 255 − elapsed · 255 / `duration`,
  in integers;
- `voice`: `file` (under `audio/`) plays at `t`;
- `song`: Beach cs4's song 0x8A (`entry`), `file` (under `audio/`, a recording of the game's own
  output at the map song's volume, 15 s, once) plays at `t`, as recorded: nothing ducks it, it ducks
  nothing (only with `conker-extract pack --song`);
- `subtitles`: the speech bubble, `pages` (each `lines` from its `t`, the last up to `close`),
  drawn in the HUD's frame at `anchor`: the bubble's `quads` (`texture` under `hud.json`'s
  textures, `rect` x, y, width, height, `uv` its texels, x1 < x0 mirrored, `colour`), then the lines
  upper case in the HUD font, black, left-aligned in a block whose left is `text[0]` − ⌈widest / 2⌉
  and 12 pixels a line around the middle line's top at `text[1]`. From each of `moves`' `t`, its
  `anchor`, `text` and `quads` instead. The bubble grows from 1/3 to full over 3 ticks about its
  tail's tip (the tail quad's bottom corner on the side it points to), its text fading in over 10
  ticks from the third; a page turn fades the old text out over 10 ticks, then the new one in; the
  close fades the text out, then shrinks the bubble 0.25 a tick till it is gone. Alpha
  ⌊25.5 n⌋ for n of 10;
- `props`: `outro.gltf`'s `prop-<placement>` shown for the outro at `at`, turned `rotation[0]`
  degrees about x, then `rotation[1]` about y;
- `sounds`: sound `id` (`sounds.json`) at the actor `actor` (an index of `actor`, then `cast`),
  following it, from `t` at `cents`; with `stop` it loops till then, fading in linearly up to
  `fade_in` and out from `fade_out` to `stop` when given; full volume (0x7F7F), panned by place;
- `near`: the camera's near plane (Beach's cs3: 30).

### `characters/<id>/character.json`
- `model_lift`: model units the model is drawn above the player (43 for 117, 42 for 128, 49 for 136).
- `clips`: one entry per animation of `character.gltf`. The client finds a clip's animation by its
  name, `pair_NNNN_anim_IIII`: its `pair`, then its first `logical` id, in decimal.
  - `pair`: the clip in the ROM;
  - `logical`: the animation ids the game plays it under;
  - `frames`: the stored keyframes (for reference: not read);
  - `duration_ticks`: the loop length, in ticks.
- `root_motion`: the clips that move the player by their root, such as the creep, weapon 0x16's
  attack and the shot death:
  - `z`: the root's z per stored frame, 32 per model unit;
  - `end`: its value at the loop point;
  - `step`: ticks between stored frames.
- `looks` (156 and 157 only): War: Beach's villager classes drawn with this model (class in
  decimal, 17-32, re-drawn at each spawn) → the display lists the class hides: bit k set hides the
  glTF primitives whose `extras.list` is k. 156 has lists 0-5 (0 the body), 157 lists 0-4.

`character.gltf` keeps each display list of the model in its own primitives (`extras.list`: its
index; one per list and material), so a look can hide some.

The characters play the same animations, as in the game (Conker's clips), so their `clips` and
`root_motion` are the same. Every player moves by 117's `root_motion` (the others' are for reference); each
model is drawn with its own `model_lift` and animated by its own glTF's animations, found by `clips`.
The bodies that shots, blasts and pushes test (the squirrel's and the caveman's are larger) are the game's, built into
the server and the clients, not in the pack.

### `weapons/deathmatch/held.json`
One entry per glTF scene of `held.gltf`, in order:
- `entry`: the model, whose meshes are named `09-<entry>-<part>` in hex (the crossbow, a child
  object: `01-6e-<list>`, its bank-01 model in its bind pose; for reference: glTF scene `i` is
  entry `i`).
- `joint`: the character joint it hangs from (`joint_09`, the right hand; `joint_04`, the left;
  `joint_12`, the head).
- `create` and `remove`: `[animation id, tick]` events that show and hide it. An event fires once
  the animation's clock reaches its tick (the game reads a clip's events in order and stops at the
  first not yet due, so a tick here is never below an earlier event's of the same clip).
- The gas mask (War: Total War): entry 67 in the hand while its put-on (animation 337) plays,
  entry 66 on the head while it is worn after that; neither while it comes off (animation 334).

### `game/acos.json`, `game/sink.json`
- `acos.json`: acos(x) in units of π/65536, at 132 sample points (x = i·512, then 0x7800 + i·32,
  then 0x7FE0 + i·8, in 32767ths).
- `sink.json`: per surface kind (0-31), `[depth, change with speed, speed percent]`.

### `audio/sounds.json`
`mix_rate` (22050) and `sounds`, by sound id:
- `file`: the WAV, relative to `audio/` (named by the id, in the folder of who makes it);
- `rate`: the rate it was decoded at (for reference: the WAV's own header is what plays);
- `volume` (0-127) and `pan` (0-127, 64 centre): the sample's own, multiplied into every event's;
- `envelope`: `attack_us` up to `attack_volume`, then `decay_us` to `decay_volume` (`-1`: holds),
  `release_us` to fade out once stopped (microseconds, levels 0-127);
- `loop`: `start`..`end` in samples repeats `count` times (`null`: until the sound is stopped), or
  `null` (plays once);
- `detune_after_attack`: cents the pitch moves by once the attack is over, or `null`.

### `audio/events.json`
Volumes are out of 32767; distances in game units; times in 30 Hz ticks. Every `play` has:
- `sounds`: candidates, one picked at random;
- `volume`: for a sound from another player or the world; `own`: the volume of the listener's own
  player's sound, played unpositioned (centred), or `null` (positional like the others);
- `near`, `far`: full volume within `near` of the listener, silent from `far`, linear in between;
- `channel`: `a` or `b` (a player's sound on a channel stops its previous one there; `b` is the
  voice), or `free`;
- `chance` (plays when it is at least a random byte: 255 always), `if_idle` (skipped while the
  channel plays), `pitch` (a random offset in cents between the two), `ticks` (a looping sound
  stops after that many ticks, or `null`).

Sections:
- `maps.<map>`: `background` (looped for the whole match) and `emitters` (a flickering lamp's buzz:
  it plays as each flash of the lamp at `at` starts, `lights.json`'s `flicker`);
- `characters.<id>`: the sound events of each clip (`clip` = its logical animation id, as
  `character.json`; `time` in ticks): `play`, `footstep` (a step of that kind, below) or `stop` (a
  channel);
- `footsteps`: per surface kind (bits 0-4 of the floor's surface word, `collision.json`) the
  candidates for step kinds 0-4, played with `play`, louder with speed up to `full_speed`;
- `cues`: what the game's code plays besides the clips: `on` names the game event (a shot, a hit,
  an explosion, a pickup, a weapon in hand…) with its weapon id in decimal, `character` limits it to
  one character, `then` plays when a firing loop ends; `note` says where it comes from. Every cue
  whose `on` (and `character`) matches plays: a head shot plays two. War: Total War's: `gas` (a loop
  while the gas is out) and `carry` (a gas canister taken: `canister` 0 or 1, `bot` a bot's). War:
  Colors': `flag` (a flag taken: `flag` 0 the red, 1 the green, `bot` a bot's: one of its sounds)
  and `lose` (each player of the team that did not score, 30 frames after a capture: `team` the
  loser's, one of its sounds at it). `carry` and `flag` are radio lines: a human's `radio_click`
  with `end` false at once, the line `delay` later, `radio_click` with `end` true once it has
  played (a bot's: the line only); none while the same `canister` or `flag` spoke in the last 300
  frames. The mounted guns
  (maps 30 and 34): `gun` (at the gun, its state going `from` → `to`: 1 free, 2 a player jumping
  on, 3 manned, 4 firing, 11 stopped by B or overheating, 5 left; 3 → 4 starts the firing loop on
  channel `a`; a looping sound plays until the gun's state changes again), `gun_stop` (at the gunner, when its firing stops), `gun_zoom` (the gunner's zoom,
  heard by the gunner only: `step` 0 while it moves, a loop; 1 when it stops; 2 once step 1's sound
  has ended), `gun_aim` (the gunner's aim: `step` 0 while it keeps changing, from its 10th VI of
  change, a loop; 1 when it stops); and character 128's `firing` with weapon 27 (0x1B, the mounted
  gun), the squirrel gunner's battle cry. There is no sound per shot. War: Beach's (map 0x2B):
  `plunger_ready` (a villager walking into the plunger's voice trigger while it is ready) and
  `plunger` (one setting it off), each its radio voice per character, wrapped in `radio_click` as
  `carry` (at most one per player in 300 frames); `detonation` (three records, each at its `at`, `delay` after the blast); `debris` (each
  piece landing); `strike_call` (the artillery strike called on a villager whose escape countdown
  ran out, and the villager's voice) and `strike_blast` (40 frames later, at the same spot).
- A cue's `at` (if present) is where it plays instead of the event's place, and `delay` (if
  present) the ticks it waits after the event.
- `maps.<map>.lines` (if present): voice lines the map's mode plays, unpositioned, the music ducked
  while one plays: on 0x2B `escape`, the radio a human villager hears when it escapes (`note` says
  how the music ducks); on 0x30 `jingle-squirrels` and `jingle-tediz`, the capture jingles.

### `hud/hud.json`
Positions are in the original's frame, `frame`: 292 × 216 pixels, the picture the game draws
(`tv`, for reference: a TV showed it at `offset` of a 320 × 240 screen). The client scales the frame to the window by its
height (by its width when the window is narrower than 292:216), and each part keeps its distance
from the edge or centre its `anchor` names (`["left"|"centre"|"right", "top"|"middle"|"bottom"]`),
as the game places each part off its screen's edges and centre: on a wide window the health stays at
the left edge and the radar at the right one. Rectangles are `[x, y, width, height]`, colours
`[red, green, blue, alpha]` (0-255; alpha 255 is opaque), and a picture is
`{"texture": name, "rect": [x, y, width, height]}`: a rectangle of a texture, in its texels.
- `textures`: name → `file` (under `hud/`), `size` (its texels: keep it the PNG's), `smooth`
  (drawn smoothed, as the game scales it; else sharp texels).
- `font`: one entry per glyph: `char`, its `file`, `size` and `offset` from the pen and the line's
  top; the pen moves `offset[0] + size[0] - 1` after it. The first glyph of a character is the one drawn (the
  game's list has two `ê` and two `ô`).
- `health`: one `piece` per point of health (6 at most): ceil(health / 2) on the row at `rows[0]`,
  the rest on the row at `rows[1]`, both ending at `right`.
- `weapon`: the held weapon's icon at `at`, whether the weapon is drawn or put away; `icons` by
  weapon id in decimal (9 is 0x09, the twin guns). None while a grenade is in the hand.
- `grenades`: while a grenade is in the hand, one `icon` per grenade left, the first at `at`, each
  next `step` pixels along.
- `score` (kills), `clock` (minutes:seconds left) and `rank` (`(1st)`): text at `at`, centred on
  it or starting there (`align`), in `colour`, with a `shadow` colour drawn one pixel right and down
  first (or `null`). The rank's suffixes are the textures in `suffixes` (st, nd, rd, th), drawn
  `offset` from the pen showing `texels` of them; the pen moves `advance` after one and a centred
  line counts `width` for it.
- `radar`: the `disc` picture `size` pixels wide around `centre`, in `colour`; it turns with the
  view (up is where the camera looks). Every other living player within `range` pixels, at `scale`
  pixels per game unit, is a `blip`-pixel square in `blip_colour`; one on another level (`levels`:
  the heights between levels) gets a `level_mark` bar above the square if higher, below if lower.
  A player holding a weapon in `hidden_with` is not shown. In War (Total War, Colors) a blip is in
  its player's team's `team_colours` entry instead (the first the squirrels', the second the
  Tediz'). In War: Colors a flag not carried is a blip too, in `flag_colour`, and a carrier's blip
  shows only while the `flags` icons are blinked off.
- `reticle`: while aiming (R) with one of `weapons`: the `image` over `rect`, in `colour`.
- `scope`: while looking through a scope: the `ring` picture is a quarter (its centre at its bottom
  right corner), drawn four times mirrored, `quadrant` pixels each, around `centre`; `dots` copies
  of `dot`, `dot_size` wide, on a circle of `radius`; the zoom bar: `back` in `back_colour`, and a
  bar `bar_width` wide from `bar_x`, centred on the scope, half as tall as `bar_half[0]` plus
  `bar_half[1]` times the zoom (0 opened, 1 at the weapon's closest), in `colour` like the ring.
- `iris`: while dead the screen goes black but for a circle around `centre`: `radius[n]` pixels
  `n` frames before the respawn, nothing while the list has no entry. The `image` is a quarter of the
  circle's edge (opaque outside it), drawn four times mirrored, in `colour`.
- `strip`: War: Total War's team strip, drawn instead of `score` and `rank` (and under the results):
  per team (the first entry of each list the squirrels', the second the Tediz') its icon over
  `rects`, from `kills` in a match on the clock or `lives` in one without, and its number as
  `numbers` (text as above): its players' kills, or without a clock their lives left. War: Colors
  draws it too, with the `lives` icons and each team's captures as its number.
- `flags`: War: Colors' carried-flag icons, per flag (the first the red, which the squirrels carry,
  the second the green, the Tediz'): its `image` over `rect` while the flag is carried, blinking
  (hidden while the frame's VI count, 2 a frame, is under 11 in its low 5 bits).
- `countdown`: War: Total War's gas countdown (text as above): the seconds left until the gas kills.
- `haze`: War: Total War's gassed screen, over the whole view of a player caught outside the
  shelter, under the rest of the HUD, for 150 frames (fading in over 32, out from the 132nd): per
  layer, `image` repeated, `texel` texels a pixel, starting at texel (frame / 4, `t`) and drifting
  a quarter texel a frame, in `colour` at the screen's strength times `share`.
- `gun`: a mounted gun's gunner view (maps 30 and 34), the rest of the HUD as usual. A sprite is
  its `image` over `rect`, mirrored across x and/or y as `flip` says. Everything blends as usual
  (alpha over the picture beneath); a grey picture (the sight, the tapes) is its grey times the
  colour, for colour and alpha alike:
  - `sight`, in `colour`: the reticle, the four ticks and the four corners;
  - `tapes`, in `tape_colour`: [0] the heading's, [1] the elevation's: `image` repeated `repeats`
    times along `rect`'s long side, starting `per_degree` × the angle texels in (the heading, the
    view direction's atan2(x, z), and the elevation above the horizontal, in degrees);
  - `handles` [0] the left, [1] the right, as they are, and `lit`, the same handle lit by the
    muzzle flash, crossfaded with them while the gun fires: at rest the handle's alpha is 255 and
    `lit`'s 0; on a shot, on the side that fired, `lit` goes to 0x53 and the handle to 0xAC (their
    sum stays 255), then `lit` fades out (V: 0x09, then 0x01, two frames apart); that side also drops
    10-21 pixels (not exported);
  - the zoom bar, as `scope`'s: `back` in `back_colour`, a bar `bar_width` wide from `bar_x`,
    centred on y 108, in `bar_colour`; 16 frames after the zoom stops it turns `bar_flash`, then
    fades back to `bar_colour` over 35 frames;
  - `flash`, after each shot: `image` centred on `centre` for barrel 0 (`joint_05`, at +x: the flash
    on the right, where the right handle drops), mirrored across
    the frame's centre for barrel 1, its size random in `width` × `height`, its alpha random in
    `alpha`, for a random `life` VIs (2 a frame, from its own): over its last 2 VIs its alpha is at
    most 0x55 a VI left (V: alpha 0xF9 on its shot's frame, 0 two frames later).
- `beach`: War: Beach's (map 0x2B), the rest as usual: its `strip`, as `strip` above with the
  villagers' head first and the Tediz' second (the same picture in `kills` and `lives`; the numbers
  are the teams' scores), and a villager's `escape` countdown (text as above: the seconds left to
  reach an exit, 90 down to 0; none for a Tedi).

### `hud/end.json`
The same frame, anchors, rectangles, colours and pictures as `hud.json`, with its own `textures`.
- `lives`: in a match without a clock (lives count), the lives left: the picture `digits[n]` for
  n lives, at `at`.
- `skull`: over the view of a player out of lives, the `image` over `rect`.
- `results`: when the match is over, over the frozen level: the `header` and the columns' `icons`
  (accuracy, kills, headshots, deaths); a row per human player (bots are not listed), its text's
  top at `rows[0]` for the first, `rows[1]` lower for each next: the player's `labels[n]` (P1,
  P2...) with its top left at `label` (moved down with the row), then its place (`1st`, with
  `hud.json`'s rank suffixes), its accuracy (hits × 100 / shots, %), kills, headshots and deaths,
  each centred on one of `columns`, in the player's `colours[n]` over `shadow` drawn one pixel
  right and down; the menu's `restart` and `quit` (`image`, or `lit` while chosen) and the
  `stick` between them.
- `teams`: War: Total War's results: the columns' icons, the labels and the rows `down` pixels lower,
  each row in its team's colour (`results.colours`: the first the squirrels', the second the
  Tediz'), `hud.json`'s `strip` under them, and a header: `draw` when both teams are first; with
  one human player, `won` or `lost` (YOU WIN, YOU LOSE); with more, the winning team's
  `team_won` entry (the first the squirrels', the second the Tediz': its name and WIN). War: Beach's
  the same with `beach_won` (the first the villagers', FRENCHYS WIN; the second the Tediz') and
  `hud.json`'s `beach.strip`, and rows for its Tedi players only.

## Editing

- **JSON:** any text editor. Keep the field names; numbers are plain decimals.
- **PNG textures:** any image editor. Keep the file name and the size, and keep the alpha channel
  where there is one.
- **glTF models** (`level.gltf`, `props.gltf`, `character.gltf`, `held.gltf`): in Blender, *File > Import >
  glTF 2.0*. Export with *Format: glTF Separate (.gltf + .bin + textures)*, *Include > Custom
  Properties* on, to the same file name and folder. Keep:
  - node and mesh names: `joint_NN`, the held meshes `09-…` (the crossbow's `01-6e-…`), and the level's and props' `…@N` nodes (the client
    hides node `…@N` while the pickup on placement N is taken, and War: Total War's props as its match says);
  - the character's animations' names (`pair_NNNN_anim_IIII`): the client finds each clip by its
    name, so their order may change;
  - one scene per held model, in `held.json`'s order.

  Moving level geometry changes only what is drawn: players collide with `collision.json`.
- **Sounds:** to replace a sound, save your sound over its WAV (the file `sounds.json` names for its
  id) as a WAV file: mono, 16-bit PCM, at any rate (the file's rate is the one played; the
  original's are 8-28 kHz). If the sound loops (`loop` in `sounds.json`), set `start` and `end` to
  your file's loop points in samples, or `loop` to `null` to play it once. To change what an event
  plays, edit its `sounds` (any id in `sounds.json`); to add a sound, add an entry to `sounds.json`
  with its `file` under `audio/`. `background.wav` loops whole: keep its end matching its start.
- **The HUD:** to restyle a part, paint over its PNG (keep the size, or set the new one in
  `hud.json` `textures`, and move the `rect`s that cut pictures from it), or point its picture at
  another texture or rectangle. Move a part with its `at`, `centre`, `rect` or `right`, recolour
  it with its colours, or anchor it elsewhere. The glyphs and the rank suffixes are white: their
  colour is the text's colour (paint them white, the shape in the alpha). The other pictures are
  drawn as they are, times their colour (white keeps them as painted). A new character for the
  font is a new `font` entry and PNG.

## When edits take effect

The game server reads the data files of its map (`conker-server --map 2d|30|34|3f`) when it starts:
restart it after an edit. The clients ask the server which map it plays and load that one. Pages and native
clients read the pack (the HUD too) when they start: reload them (bypassing the browser's cache if the static server
sends no `Cache-Control`). The server and its clients must read the same pack: each client predicts its
player on the same collision as the server, in the same order. Fields marked "for reference" are not
read.

The pack format is versioned (`pack.json` `format`). A server or page of another format refuses the
pack and says so; export a fresh one then.
