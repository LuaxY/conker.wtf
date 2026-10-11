# Conker multiplayer asset pack

This folder holds what a multiplayer match needs from *Conker's Bad Fur Day*: game data as JSON,
models as glTF with PNG textures, sound as WAV files, the HUD as PNGs and JSON, and two script
tables as raw bytes.
`conker-extract pack` wrote it from the ROM named in `pack.json` (and `audio/maps/2d/background.wav`
from a recording of the game, `--background`); the game server reads it and serves it to the
browsers, so nobody needs a ROM to play. It is Rare's data: keep it private, and out of the remake's
source tree.

You may edit any file. A pack whose files don't parse is refused at load, with the file and the field
in the message. `conker-extract pack` refuses to overwrite a pack, so your edits are safe; with
`--force` it writes a fresh one over it.

## Layout

| Path | What it is |
|---|---|
| `pack.json` | `format` (the pack format; a server reads only its own), `source_rom_sha1` (for reference), the folders under `maps/`, `characters/` and `weapons/`, `audio`: the folders under `audio/` that hold sound (absent in a pack without sound), and `hud`: whether the pack has `hud/` |
| `maps/2d/level.gltf`, `level.bin`, `textures/*.png` | Scene 0x2D (the Deathmatch map) as the client draws it: a node per model instance (the scene's own segments, then its placements) |
| `maps/2d/collision.json` | What players stand on and run into, in the game's build order |
| `maps/2d/spawns.json` | The 20 multiplayer spawn records |
| `maps/2d/pickups.json` | The 19 weapon and grenade pickups |
| `maps/2d/triggers.json` | The scene's trigger records: camera volumes and hints, bot zones, lifts |
| `maps/2d/nav.json` | The bots' navigation graph |
| `characters/117/character.gltf`, `.bin`, `textures/*.png` | Character 117, the Tedi: the Tediz side's player (and every Deathmatch player in the original). Skinned mesh, 28 joints, 764 animations |
| `characters/128/character.gltf`, `.bin`, `textures/*.png` | Character 128, the squirrel soldier (SHC): the squirrels side's player. The same 28-joint skeleton with its own proportions, and the same 764 animations |
| `characters/<id>/character.json` | Its model lift, one entry per glTF animation, and the root-motion clips |
| `weapons/deathmatch/held.gltf`, `.bin`, `textures/*.png` | The weapons and the grenade in a player's hand (11 models: the twin guns are two): glTF scene `i` is `held.json[i]` |
| `weapons/deathmatch/held.json` | For each: the joint it hangs from and the animation events that show and hide it |
| `game/acos.json` | The game's arc-cosine table |
| `game/sink.json` | The sink table: how far players sink into each kind of surface |
| `game/ai.bin`, `game/class.bin` | The bots' AI script and the characters' behaviour scripts (game bytecode, raw) |
| `audio/sounds.json` | Every sound a match plays, by the game's sound id: its WAV file and how the game plays it |
| `audio/events.json` | Which game event plays which sound, at what volume, from where |
| `audio/common/*.wav` | Sounds anyone makes: weapons, footsteps, hits, pickups, explosions (16-bit mono PCM, the game's own rates) |
| `audio/characters/<id>/*.wav` | Sounds only that character makes: its voice |
| `audio/maps/2d/*.wav` | The map's own sounds: `background.wav` (the background noise, a recording of the game's own output, stereo, looped whole) and its bubbling spots |
| `hud/hud.json` | What the client draws over the view: where each part of the HUD goes, its pictures, colours and rules |
| `hud/*.png` | The HUD's pictures, the game's own textures: `atlas.png` and `atlas2.png` (weapon icons, the chocolate health pieces), `grenade.png`, `radar.png`, `reticle.png`, `scope.png` and `scope_dot.png`, `iris.png` (the death circle), `rank_st/nd/rd/th.png` |
| `hud/font/NN.png` | The game's font, one PNG per character (`hud.json` `font` says which) |

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

### `maps/2d/collision.json`
A list of runs. Each run has a `layer` and its `triangles`:
- `"terrain"`: the level's ground and walls. Each triangle carries a surface word (below).
- `"secondary"`: a second wall layer.
- `{"placement": N}`: the collision of placed model N, already in world space.

Each triangle is `[[x, y, z], [x, y, z], [x, y, z], surface]`, where `surface` is `"0x…"` or `null`.
The point order sets which side faces up (the normal is (p1 − p0) × (p2 − p0)).

**Keep the order:** the game numbers triangles in this order, and the server and every client must
agree on it. Placements 10 and 11 are wall objects; their runs must stay contiguous.

Surface words: bits 0-4 are the kind (it indexes `sink.json`; on this map 0, 1, 3, 4, 5, 9 and 0xA,
where 0xA slows players). Bits 28-30 are the slide class (6: an idle player slides down).

Collision is JSON, not glTF: a glTF split by surface word would lose the build order, and a float
written in text reads back exactly here.

### `maps/2d/spawns.json`
One record per spawn point, in record order (the match rules pick them by index):
- `position`: integer x, y, z.
- `yaw`: facing, 1/256 turn.
- `camera`: the camera eye at (re)spawn.
- `byte_06`, `byte_14`, `byte_15`: kept as the game has them. `byte_06` is an angle in 1/256 turn.

### `maps/2d/pickups.json`
The pickups in the game's weapon-manager order:
- `placement`: the placed model that draws it: the level glTF's node named `…@N` (its
  `extras.placement` is N too). The client hides that node while the pickup is taken.
- `gives`: `{"weapon": id}` (the weapon id in decimal: 9 is 0x09, the twin guns) or `"grenades"` (+3).
- `position`: where a player touches it, within 100 units horizontally.

Move a pickup by changing its `position` and the matching level node together.

### `maps/2d/triggers.json`
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
  - a lift: the target scene (0x2D = 45).
- `arg`: a lift's target spawn record.
- `command`: a hint's camera command (30 cuts behind the player).
- `word_1c`: kept as is.

Zones of kind 3 with action 11 are crossed straight by the bots' paths. Records 88 and 89 (0x58,
0x59) are where the camera's sweep skips placed objects; that is the map's camera script, which refers
to them by index, so keep the order.

### `maps/2d/nav.json`
The bots' waypoints, in index order:
- `pos`;
- `band`: the height band;
- `id`: the AI script names a node by this id;
- `half`: half the walkway's width;
- `next`: up to 5 neighbour indices, 255 = none;
- `barrier`: 1 marks a wall line, which bots may not cross, and is never walked to.

There are at most 255 nodes.

### `characters/<id>/character.json`
- `model_lift`: model units the model is drawn above the player (43 for 117, 42 for 128).
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

Both characters play the same animations, as in the game (Conker's clips), so their `clips` and
`root_motion` are the same. Every player moves by 117's `root_motion` (128's is for reference); each
model is drawn with its own `model_lift` and animated by its own glTF's animations, found by `clips`.
The bodies that shots, blasts and pushes test (the squirrel's is larger) are the game's, built into
the server and the clients, not in the pack.

### `weapons/deathmatch/held.json`
One entry per glTF scene of `held.gltf`, in order:
- `entry`: the model, whose meshes are named `09-<entry>-<part>` in hex (for reference: glTF scene
  `i` is entry `i`).
- `joint`: the character joint it hangs from (`joint_09`, the right hand; `joint_04`, the left).
- `create` and `remove`: `[animation id, tick]` events that show and hide it.

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
- `maps.2d`: `background` (looped for the whole match) and `emitters` (spots that play in bursts by
  themselves: each tick a burst starts with `start_chance`);
- `characters.<id>`: the sound events of each clip (`clip` = its logical animation id, as
  `character.json`; `time` in ticks): `play`, `footstep` (a step of that kind, below) or `stop` (a
  channel);
- `footsteps`: per surface kind (bits 0-4 of the floor's surface word, `collision.json`) the
  candidates for step kinds 0-4, played with `play`, louder with speed up to `full_speed`;
- `cues`: what the game's code plays besides the clips: `on` names the game event (a shot, a hit,
  an explosion, a pickup, a weapon in hand…) with its weapon id in decimal, `character` limits it to
  one character, `then` plays when a firing loop ends; `note` says where it comes from. Every cue
  whose `on` (and `character`) matches plays: a head shot plays two.

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
  top; the pen moves `size[0] - 1` after it. The first glyph of a character is the one drawn (the
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
  A player holding a weapon in `hidden_with` is not shown.
- `reticle`: while aiming (R) with one of `weapons`: the `image` over `rect`, in `colour`.
- `scope`: while looking through a scope: the `ring` picture is a quarter (its centre at its bottom
  right corner), drawn four times mirrored, `quadrant` pixels each, around `centre`; `dots` copies
  of `dot`, `dot_size` wide, on a circle of `radius`; the zoom bar: `back` in `back_colour`, and a
  bar `bar_width` wide from `bar_x`, centred on the scope, half as tall as `bar_half[0]` plus
  `bar_half[1]` times the zoom (0 opened, 1 at the weapon's closest), in `colour` like the ring.
- `iris`: while dead the screen goes black but for a circle around `centre`: `radius[n]` pixels
  `n` frames before the respawn, nothing while the list has no entry. The `image` is a quarter of the
  circle's edge (opaque outside it), drawn four times mirrored, in `colour`.

## Editing

- **JSON:** any text editor. Keep the field names; numbers are plain decimals.
- **PNG textures:** any image editor. Keep the file name and the size, and keep the alpha channel
  where there is one.
- **glTF models** (`level.gltf`, `character.gltf`, `held.gltf`): in Blender, *File > Import >
  glTF 2.0*. Export with *Format: glTF Separate (.gltf + .bin + textures)*, *Include > Custom
  Properties* on, to the same file name and folder. Keep:
  - node and mesh names: `joint_NN`, the held meshes `09-…`, and the level's `…@N` nodes (the client
    hides node `…@N` while the pickup on placement N is taken);
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

The game server reads the data files when it starts: restart it after an edit. Pages and native
clients read the pack (the HUD too) when they start: reload them (bypassing the browser's cache if the static server
sends no `Cache-Control`). The server and its clients must read the same pack: each client predicts its
player on the same collision as the server, in the same order. Fields marked "for reference" are not
read.

The pack format is versioned (`pack.json` `format`). A server or page of another format refuses the
pack and says so; export a fresh one then.
