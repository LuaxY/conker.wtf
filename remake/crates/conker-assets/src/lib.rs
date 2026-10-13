//! The asset pack: what a multiplayer match needs from the game, extracted once from the ROM
//! (`conker-extract pack`) into files that are easy to edit, and read back here into the values
//! the runtime extracted in memory before ([`Slice`]).
//!
//! Layout, relative to the pack root (the pack's own `README.md` documents every file):
//! - `pack.json`: the [`Manifest`];
//! - `maps/<map>/`: `level.gltf` (+ `level.bin`, `textures/*.png`), `collision.json`,
//!   `spawns.json`, `pickups.json`, `triggers.json`, `nav.json`, `ai.bin`, `lights.json`,
//!   `outros.json` ([`outro::Outro`]); on `2d`, `props.gltf` (+ `props.bin`);
//! - `characters/<character>/`: `character.gltf` (+ `.bin`, PNGs), `character.json`, one folder
//!   per character;
//! - `weapons/<set>/`: `held.gltf` (+ `.bin`, PNGs), `held.json`;
//! - `game/`: `acos.json`, `sink.json`, `class.bin`;
//! - `audio/`: `sounds.json`, `events.json` and WAV files ([`audio::Audio`]);
//! - `hud/`: `hud.json` and PNGs ([`hud::Hud`]).
//!
//! No file system here: [`Slice::load`] reads through a closure, so the same code runs where the
//! files are on disk and in a browser that fetched them first ([`Slice::files`] lists them). The
//! glTF files are not read here; Bevy's glTF loader loads them by path ([`level_gltf`] etc.).

pub mod audio;
pub mod hud;
pub mod outro;

use std::fmt::Display;
use std::io::{self, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The pack format this build reads and writes ([`Manifest::format`]).
pub const FORMAT: u32 = 1;
/// The manifest's path.
pub const MANIFEST: &str = "pack.json";
/// The folders of what a Deathmatch reads: its map, its characters (bank-01 models: the squirrels'
/// soldier, 128, the Tediz' Tedi, 117, then the cavemen's, 136: conker-sim's `Side` order) and its
/// weapon set.
pub const PACK_MAP: &str = "2d";
pub const PACK_CHARACTERS: [&str; 3] = ["128", "117", "136"];
pub const PACK_WEAPONS: &str = "deathmatch";
/// Entries of the game's acos table (`.game` data at 0x80085EC0: 62 + 64 + 6).
pub const ACOS_LEN: usize = 132;
/// Bytes of one trigger record.
const TRIGGER: usize = 0x34;
/// The navigation graph's "no neighbour".
const NO_NODE: u8 = 0xFF;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{file}: {message}")]
    Read { file: String, message: String },
    #[error("{file}: {source}")]
    Json {
        file: String,
        source: serde_json::Error,
    },
    #[error("{file}: {field}: {message}")]
    Invalid {
        file: String,
        field: String,
        message: String,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn invalid(file: &str, field: impl Display, message: impl Display) -> Error {
    Error::Invalid {
        file: file.into(),
        field: field.to_string(),
        message: message.to_string(),
    }
}

/// `pack.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`FORMAT`] when written; a pack of another format is refused.
    pub format: u32,
    /// SHA-1 of the ROM it was extracted from (`.z64` byte order).
    pub source_rom_sha1: String,
    /// The folders under `maps/`, `characters/` and `weapons/`.
    pub maps: Vec<String>,
    pub characters: Vec<String>,
    pub weapons: Vec<String>,
    /// The folders under `audio/` that hold sound files; empty (or absent, in a pack exported
    /// before there was sound): the pack has no sound.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<String>,
    /// Whether the pack has the HUD (`hud/`; absent in a pack exported before it).
    #[serde(default)]
    pub hud: bool,
}

impl Manifest {
    pub fn to_json(&self) -> Vec<u8> {
        to_json(self, 1)
    }
}

/// The glTF files, relative to the pack root (Bevy loads them by path, with their `.bin` and PNGs).
pub fn level_gltf(map: &str) -> String {
    format!("maps/{map}/level.gltf")
}

/// War: Total War's props on its map (`2d` only): glTF scene 0 the props its Deathmatch hides, named
/// as the level's nodes; scenes 1 and 2 the canister's and the gas mask's models at the origin.
pub fn props_gltf(map: &str) -> String {
    format!("maps/{map}/props.gltf")
}

pub fn character_gltf(character: &str) -> String {
    format!("characters/{character}/character.gltf")
}

pub fn held_gltf(weapons: &str) -> String {
    format!("weapons/{weapons}/held.gltf")
}

/// Where a collision triangle comes from (extract-scene's `Layer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// The scene bundle's segment 0 (its triangles carry surface words).
    Terrain,
    /// Its segment 3.
    Secondary,
    /// The model of placement `n`, in world space.
    Placement(usize),
}

/// A world-space collision triangle (extract-scene's `Triangle`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub layer: Layer,
    /// The terrain's surface word; None on the other layers.
    pub surface: Option<u32>,
}

/// A multiplayer spawn record (extract-scene's `SpawnPoint`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub position: [i16; 3],
    pub byte_06: u8,
    /// Facing in 1/256 turns (the u16 facing is `yaw << 8`).
    pub yaw: u8,
    /// The camera eye at (re)spawn.
    #[serde(serialize_with = "ints3")]
    pub camera: [f32; 3],
    pub byte_14: u8,
    pub byte_15: i8,
}

/// What a pickup gives (conker-rules' `Gives`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gives {
    Weapon(u8),
    /// +3 grenades.
    Grenades,
    /// A gas mask (War: Total War).
    Mask,
}

/// A pickup on the map: its placement, what it gives and where it is (conker-rules'
/// `Arena::pickups`, in the multiplayer weapon manager's order).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pickup {
    pub placement: u16,
    pub gives: Gives,
    #[serde(serialize_with = "ints3")]
    pub position: [f32; 3],
}

/// Ids and length of one character animation, in the character glTF's animation order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    /// Pair index in the character's bank-02 entry.
    pub pair: usize,
    /// Logical animation ids routed to the pair (what the game plays: u16 +0x84 of a character).
    pub logical: Vec<u16>,
    /// Stored keyframes.
    pub frames: usize,
    /// Length in 30 Hz ticks. Loop on this: the glTF animation ends at `(duration - 1) / 30` s.
    pub duration_ticks: u16,
}

impl ClipInfo {
    /// Its animation's name in the character glTF: `pair_NNNN_anim_IIII` (the pair, then the first
    /// logical id, decimal). The client finds animations by it, so a re-export may reorder them.
    pub fn animation(&self) -> String {
        let mut name = format!("pair_{:04}", self.pair);
        if let Some(id) = self.logical.first() {
            name += &format!("_anim_{id:04}");
        }
        name
    }
}

/// A character clip that moves the character by its root: the data of
/// `conker_sim::root::RootTrack { ids: logical, duration, step, z, end }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RootClip {
    pub logical: Vec<u16>,
    pub duration: u16,
    pub step: u16,
    /// The root z channel per stored frame (32 per model unit) and at the loop point.
    pub z: Vec<i32>,
    pub end: i16,
}

/// A model a character holds (extract-character's `Held`): drawn with the character's joint
/// `joint` while the last of its events that fired created it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Held {
    /// Bank-09 entry (the held glTF's mesh `09-<entry>-<part>`, hex).
    pub entry: usize,
    pub joint: u8,
    /// (logical animation id, clip time in ticks) of every event that creates it, and removes it.
    pub create: Vec<(u16, f32)>,
    pub remove: Vec<(u16, f32)>,
}

/// `maps/<map>/lights.json`: the level's point lights the client draws (War: Total War's on `2d`,
/// none elsewhere) and the ambient light its lit draws start from (CBFD's lighting: a lit vertex's
/// colour times the ambient plus each light's colour times min(1, `size` × 2048 / d²), the vertex
/// `d` units from it).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lights {
    pub ambient: [u8; 3],
    pub lights: Vec<Light>,
}

/// A point light at `at`, of `rgb`, `size + swing × sin φ` big, φ stepping `step` a frame (from
/// any phase), shining `when`; `flicker`: it flickers in bursts (light behaviour 4, `func_151607A4`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Light {
    #[serde(serialize_with = "ints3")]
    pub at: [f32; 3],
    pub rgb: [u8; 3],
    pub size: f32,
    pub swing: f32,
    pub step: f32,
    pub when: When,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flicker: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// Outside War: Total War's gas countdown.
    Calm,
    /// During the countdown.
    Countdown,
    /// While the prop on this placement shows.
    Shown(u16),
    /// On every player carrying a gas canister (`at` is the carrier's).
    Carried,
}

/// `characters/<character>/character.json`: a character's model lift, its clips (one per glTF
/// animation) and the root-motion ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    /// Model units the model is drawn above the player (scaled with it).
    pub model_lift: i16,
    pub clips: Vec<ClipInfo>,
    #[serde(rename = "root_motion")]
    pub root_clips: Vec<RootClip>,
    /// War: Beach's villager looks drawn with this model: class → the display lists it hides (bit
    /// k: the glTF primitives whose `extras.list` is k). Empty (absent) for the other characters.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub looks: std::collections::BTreeMap<u8, u8>,
}

/// What a match on one map, with its characters and one weapon set, reads from the pack: the
/// runtime values of `conker_extract::Slice` (without its GLBs, which are files here) and the
/// tables the server read from the ROM besides.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    /// `maps/<map>/collision.json`: the collision in build order (terrain, secondary, placements).
    pub collision: Vec<Triangle>,
    /// `maps/<map>/spawns.json`.
    pub spawns: Vec<SpawnPoint>,
    /// `maps/<map>/pickups.json`.
    pub pickups: Vec<Pickup>,
    /// `maps/<map>/triggers.json`, as the ROM's table `[0x0C, scene, 5]` (0x34-byte records) that
    /// `CameraLevel::new`, `Lifts::new` and conker-bots' `Assets::triggers` read.
    pub triggers: Vec<u8>,
    /// `maps/<map>/nav.json`, as the ROM's navigation graph `[0x19, scene]` (`Assets::nav`).
    pub nav: Vec<u8>,
    /// `maps/<map>/lights.json`.
    pub lights: Lights,
    /// `maps/<map>/outros.json`: War: Total War's victory outros (on `2d`; none elsewhere).
    pub outros: Vec<outro::Outro>,
    /// The characters, in the order [`Slice::load`] was given their folders.
    pub characters: Vec<Character>,
    /// `weapons/<set>/held.json`: glTF scene `i` of the held glTF is `held[i]`.
    pub held: Vec<Held>,
    /// `game/acos.json`: the game's acos table ([`ACOS_LEN`] entries).
    pub acos: Vec<u16>,
    /// `game/sink.json`: per surface kind, the sink table (depth, change with speed, speed %).
    pub sink: [[i8; 3]; 32],
    /// `maps/<map>/ai.bin` (`[0x14, id]`, the scene's multiplayer AI script) and `game/class.bin`
    /// (`[0x0F, 0]`, the characters' class data): conker-bots' `Assets::ai` and `Assets::class`.
    pub ai: Vec<u8>,
    pub class: Vec<u8>,
}

/// The data files of a slice, relative to the pack root, in [`Slice::files`] order: the manifest,
/// the map's five, the weapon set's, the game's two, the map's AI script, the game's class data,
/// the map's lights and outros, then one per character.
fn data_files(map: &str, characters: &[&str], weapons: &str) -> Vec<String> {
    let mut files = vec![
        MANIFEST.into(),
        format!("maps/{map}/collision.json"),
        format!("maps/{map}/spawns.json"),
        format!("maps/{map}/pickups.json"),
        format!("maps/{map}/triggers.json"),
        format!("maps/{map}/nav.json"),
        format!("weapons/{weapons}/held.json"),
        "game/acos.json".into(),
        "game/sink.json".into(),
        format!("maps/{map}/ai.bin"),
        "game/class.bin".into(),
        format!("maps/{map}/lights.json"),
        format!("maps/{map}/outros.json"),
    ];
    files.extend(
        characters
            .iter()
            .map(|c| format!("characters/{c}/character.json")),
    );
    files
}

/// `collision.json`: runs of consecutive triangles of one layer.
#[derive(Serialize, Deserialize)]
struct Run {
    layer: Layer,
    triangles: Vec<Face>,
}

/// A triangle of a run: its three points and its surface word (hex string, or null).
#[derive(Serialize, Deserialize)]
struct Face(
    #[serde(serialize_with = "points")] [[f32; 3]; 3],
    #[serde(with = "word")] Option<u32>,
);

/// One trigger record, every field the ROM's records carry (`+0x24..+0x34` is filled at run time
/// and zero in the ROM).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Trigger {
    /// `+0x0`: the bottom centre; `+0x6`: half-width, height, half-depth.
    center: [i16; 3],
    size: [i16; 3],
    /// `+0xC`, `+0x10`: degrees.
    pitch: f32,
    yaw: f32,
    /// `+0x14`: 1 = switched off.
    off: u8,
    /// `+0x15 >> 2` and `+0x15 & 3`.
    kind: u8,
    shape: u8,
    /// `+0x16`, `+0x17`, then the words at `+0x18`, `+0x1C`, `+0x20`.
    flags: u8,
    action: u8,
    arg: u32,
    word_1c: u32,
    command: u32,
}

impl Trigger {
    fn parse(r: &[u8; TRIGGER]) -> Self {
        let h = |o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
        let w = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
        Self {
            center: [h(0), h(2), h(4)],
            size: [h(6), h(8), h(0xA)],
            pitch: f32::from_bits(w(0xC)),
            yaw: f32::from_bits(w(0x10)),
            off: r[0x14],
            kind: r[0x15] >> 2,
            shape: r[0x15] & 3,
            flags: r[0x16],
            action: r[0x17],
            arg: w(0x18),
            word_1c: w(0x1C),
            command: w(0x20),
        }
    }

    fn bytes(&self) -> [u8; TRIGGER] {
        let mut r = [0; TRIGGER];
        for (k, v) in self.center.iter().chain(&self.size).enumerate() {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        r[0xC..0x10].copy_from_slice(&self.pitch.to_be_bytes());
        r[0x10..0x14].copy_from_slice(&self.yaw.to_be_bytes());
        r[0x14..0x18].copy_from_slice(&[
            self.off,
            self.kind << 2 | self.shape,
            self.flags,
            self.action,
        ]);
        for (o, v) in [(0x18, self.arg), (0x1C, self.word_1c), (0x20, self.command)] {
            r[o..o + 4].copy_from_slice(&v.to_be_bytes());
        }
        r
    }
}

/// One navigation node (`[0x19, scene]`: a u16 count, a zero u16, 16-byte nodes, zero padding to
/// 8 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Node {
    pos: [i16; 3],
    band: u8,
    id: u8,
    half: u8,
    /// Neighbour node indices, 255 = none.
    next: [u8; 5],
    barrier: u8,
}

fn parse_nav(bytes: &[u8]) -> Option<Vec<Node>> {
    let count = usize::from(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]));
    let body = bytes.get(4..4 + 16 * count)?;
    Some(
        body.as_chunks::<16>()
            .0
            .iter()
            .map(|r| Node {
                pos: [0, 2, 4].map(|o| i16::from_be_bytes([r[o], r[o + 1]])),
                band: r[6],
                id: r[7],
                half: r[8],
                next: [r[9], r[10], r[11], r[12], r[13]],
                barrier: r[14],
            })
            .collect(),
    )
}

fn nav_bytes(nodes: &[Node]) -> Vec<u8> {
    let mut out = (nodes.len() as u16).to_be_bytes().to_vec();
    out.extend([0, 0]);
    for n in nodes {
        out.extend(n.pos.iter().flat_map(|v| v.to_be_bytes()));
        out.extend([n.band, n.id, n.half]);
        out.extend(n.next);
        out.extend([n.barrier, 0]);
    }
    out.resize(out.len().next_multiple_of(8), 0);
    out
}

fn parse_json<T: DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|source| Error::Json {
        file: file.into(),
        source,
    })
}

impl Slice {
    /// The data files [`Slice::load`] reads, relative to the pack root (`pack.json` first).
    pub fn files(map: &str, characters: &[&str], weapons: &str) -> Vec<String> {
        data_files(map, characters, weapons)
    }

    /// Reads the slice of `map`, `characters` and `weapons` (folder names in `pack.json`):
    /// `read(path)` returns the bytes of the pack file at `path` (relative to the pack root).
    /// Errors name the file and, where the format allows, the field.
    pub fn load<E: Display>(
        map: &str,
        characters: &[&str],
        weapons: &str,
        mut read: impl FnMut(&str) -> Result<Vec<u8>, E>,
    ) -> Result<Self> {
        let files = data_files(map, characters, weapons);
        let [
            manifest,
            collision,
            spawns,
            pickups,
            triggers,
            nav,
            held,
            acos,
            sink,
            ai,
            class,
            lights,
            outros,
        ] = &files[..13]
        else {
            unreachable!("data_files lists 13 files before the characters")
        };
        let mut bytes = |file: &str| {
            read(file).map_err(|e| Error::Read {
                file: file.into(),
                message: e.to_string(),
            })
        };
        let m: Manifest = parse_json(manifest, &bytes(manifest)?)?;
        if m.format != FORMAT {
            let why = format!(
                "{}, this build reads {FORMAT}: re-export the pack",
                m.format
            );
            return Err(invalid(manifest, "format", why));
        }
        let wanted = (characters.iter()).map(|&c| ("characters", &m.characters, c));
        for (field, names, name) in [("maps", &m.maps, map), ("weapons", &m.weapons, weapons)]
            .into_iter()
            .chain(wanted)
        {
            if !names.iter().any(|n| n == name) {
                return Err(invalid(manifest, field, format!("no {name:?}")));
            }
        }

        let runs: Vec<Run> = parse_json(collision, &bytes(collision)?)?;
        let records: Vec<Trigger> = parse_json(triggers, &bytes(triggers)?)?;
        for (i, t) in records.iter().enumerate() {
            if t.kind >= 64 || t.shape >= 4 {
                let why = format!("kind {} (0-63) and shape {} (0-3)", t.kind, t.shape);
                return Err(invalid(triggers, format!("[{i}]"), why));
            }
        }
        let nodes: Vec<Node> = parse_json(nav, &bytes(nav)?)?;
        if nodes.len() > usize::from(NO_NODE) {
            return Err(invalid(nav, "(graph)", "at most 255 nodes"));
        }
        for (i, n) in nodes.iter().enumerate() {
            if let Some(k) = n
                .next
                .iter()
                .find(|&&k| k != NO_NODE && usize::from(k) >= nodes.len())
            {
                return Err(invalid(nav, format!("[{i}].next"), format!("no node {k}")));
            }
        }
        let acos_table: Vec<u16> = parse_json(acos, &bytes(acos)?)?;
        if acos_table.len() != ACOS_LEN {
            let why = format!("{ACOS_LEN} entries, not {}", acos_table.len());
            return Err(invalid(acos, "(table)", why));
        }
        Ok(Self {
            collision: (runs.into_iter())
                .flat_map(|r| {
                    let layer = r.layer;
                    r.triangles
                        .into_iter()
                        .map(move |Face(points, surface)| Triangle {
                            points,
                            layer,
                            surface,
                        })
                })
                .collect(),
            spawns: parse_json(spawns, &bytes(spawns)?)?,
            pickups: parse_json(pickups, &bytes(pickups)?)?,
            triggers: records.iter().flat_map(Trigger::bytes).collect(),
            nav: nav_bytes(&nodes),
            characters: (files[13..].iter())
                .map(|f| parse_json(f, &bytes(f)?))
                .collect::<Result<_>>()?,
            lights: parse_json(lights, &bytes(lights)?)?,
            outros: parse_json(outros, &bytes(outros)?)?,
            held: parse_json(held, &bytes(held)?)?,
            acos: acos_table,
            sink: parse_json(sink, &bytes(sink)?)?,
            ai: bytes(ai)?,
            class: bytes(class)?,
        })
    }

    /// The slice's data files ([`Slice::files`] but `pack.json`), as [`Slice::load`] reads them;
    /// `characters` names the folders of [`Slice::characters`], in order. Fails if the trigger
    /// table or the navigation graph holds bytes the pack format does not keep (they are written
    /// decoded and re-encoded on load).
    pub fn write(
        &self,
        map: &str,
        characters: &[&str],
        weapons: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        assert_eq!(
            characters.len(),
            self.characters.len(),
            "a folder per character"
        );
        let files = data_files(map, characters, weapons);
        let [
            _,
            collision,
            spawns,
            pickups,
            triggers,
            nav,
            held,
            acos,
            sink,
            ai,
            class,
            lights,
            outros,
        ] = files[..13]
            .to_vec()
            .try_into()
            .expect("13 files before the characters");
        let mut runs: Vec<Run> = Vec::new();
        for t in &self.collision {
            let face = Face(t.points, t.surface);
            match runs.last_mut() {
                Some(run) if run.layer == t.layer => run.triangles.push(face),
                _ => runs.push(Run {
                    layer: t.layer,
                    triangles: vec![face],
                }),
            }
        }
        let (chunks, rest) = self.triggers.as_chunks::<TRIGGER>();
        let records: Vec<Trigger> = chunks.iter().map(Trigger::parse).collect();
        if !rest.is_empty()
            || records
                .iter()
                .flat_map(Trigger::bytes)
                .ne(self.triggers.iter().copied())
        {
            return Err(invalid(
                &triggers,
                "(table)",
                "holds bytes the pack does not keep",
            ));
        }
        let nodes = parse_nav(&self.nav).unwrap_or_default();
        if nav_bytes(&nodes) != self.nav {
            return Err(invalid(
                &nav,
                "(graph)",
                "holds bytes the pack does not keep",
            ));
        }
        let mut out = vec![
            (collision, to_json(&runs, 3)),
            (spawns, to_json(&self.spawns, 2)),
            (pickups, to_json(&self.pickups, 2)),
            (triggers, to_json(&records, 2)),
            (nav, to_json(&nodes, 1)),
            (held, to_json(&self.held, 2)),
            (acos, to_json(&self.acos, 1)),
            (sink, to_json(&self.sink, 1)),
            (ai, self.ai.clone()),
            (class, self.class.clone()),
            (lights, to_json(&self.lights, 2)),
            (outros, to_json(&self.outros, 3)),
        ];
        let character_files = files[13..].iter().cloned();
        out.extend(character_files.zip(self.characters.iter().map(|c| to_json(c, 2))));
        Ok(out)
    }
}

/// An f32 written as an integer when it is one (and not -0.0), else as the shortest decimal that
/// reads back to the same bits: exact either way.
struct Num(f32);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        if x.fract() == 0.0 && x.abs() < 1e9 && !(x == 0.0 && x.is_sign_negative()) {
            s.serialize_i32(x as i32)
        } else {
            s.serialize_f32(x)
        }
    }
}

fn ints3<S: Serializer>(v: &[f32; 3], s: S) -> Result<S::Ok, S::Error> {
    v.map(Num).serialize(s)
}

fn points<S: Serializer>(v: &[[f32; 3]; 3], s: S) -> Result<S::Ok, S::Error> {
    v.map(|p| p.map(Num)).serialize(s)
}

/// A surface word as `"0x0000000a"`, or null.
mod word {
    use super::{Deserialize, Deserializer, Serializer};
    use serde::de::Error as _;

    pub fn serialize<S: Serializer>(v: &Option<u32>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(w) => s.serialize_str(&format!("{w:#010x}")),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
        let Some(text) = Option::<String>::deserialize(d)? else {
            return Ok(None);
        };
        let word = text
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok());
        word.map(Some).ok_or_else(|| {
            D::Error::custom(format!("surface word {text:?}: expected 0x and hex digits"))
        })
    }
}

/// JSON, pretty down to `depth` levels of nesting and compact below them, so each record of a
/// long list (a triangle, a node, a clip) is one line. Floats print as serde_json prints them: the
/// shortest decimal that reads back to the same bits.
pub fn to_json<T: Serialize + ?Sized>(value: &T, depth: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let formatter = Pretty {
        level: 0,
        depth,
        has_value: false,
    };
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    value.serialize(&mut ser).expect("pack values serialise");
    out.push(b'\n');
    out
}

/// serde_json's `PrettyFormatter`, compact deeper than `depth`.
struct Pretty {
    level: usize,
    depth: usize,
    has_value: bool,
}

impl Pretty {
    fn newline<W: ?Sized + Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(b"\n")?;
        w.write_all(&b"  ".repeat(self.level))
    }

    fn open<W: ?Sized + Write>(&mut self, w: &mut W, bracket: &[u8]) -> io::Result<()> {
        self.level += 1;
        self.has_value = false;
        w.write_all(bracket)
    }

    fn close<W: ?Sized + Write>(&mut self, w: &mut W, bracket: &[u8]) -> io::Result<()> {
        self.level -= 1;
        if self.has_value && self.level < self.depth {
            self.newline(w)?;
        }
        w.write_all(bracket)
    }

    fn item<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if !first {
            w.write_all(b",")?;
        }
        if self.level <= self.depth {
            self.newline(w)?;
        }
        Ok(())
    }
}

impl serde_json::ser::Formatter for Pretty {
    fn begin_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.open(w, b"[")
    }

    fn end_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"]")
    }

    fn begin_array_value<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.item(w, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, _: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.open(w, b"{")
    }

    fn end_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"}")
    }

    fn begin_object_key<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.item(w, first)
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(if self.level <= self.depth {
            b": "
        } else {
            b":"
        })
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, _: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample() -> Slice {
        let record = |kind: u8, pitch: f32, yaw: f32| {
            Trigger {
                center: [-100, 20, 3000],
                size: [50, 120, 80],
                pitch,
                yaw,
                off: 0,
                kind,
                shape: 3,
                flags: 1,
                action: 0x2D,
                arg: 7,
                word_1c: 0x1D73_4849,
                command: 30,
            }
            .bytes()
        };
        let node = |next: [u8; 5]| Node {
            pos: [1, -2, 3],
            band: 1,
            id: 9,
            half: 40,
            next,
            barrier: 0,
        };
        Slice {
            collision: vec![
                Triangle {
                    points: [[-2267.0, -521.0, 6016.0], [0.0, -0.0, 1.0], [2.0, 3.0, 4.0]],
                    layer: Layer::Terrain,
                    surface: Some(0x0010_000A),
                },
                Triangle {
                    points: [
                        [0.1, 113.399994, -7.5e-8],
                        [1e10, 3.0, 4.0],
                        [5.0, 6.0, 7.0],
                    ],
                    layer: Layer::Placement(10),
                    surface: None,
                },
                Triangle {
                    points: [[1.0; 3]; 3],
                    layer: Layer::Terrain,
                    surface: Some(5),
                },
            ],
            spawns: vec![SpawnPoint {
                position: [-2267, -521, 6016],
                byte_06: 0xFF,
                yaw: 64,
                camera: [-2001.5, -400.0, 6200.25],
                byte_14: 0,
                byte_15: -3,
            }],
            pickups: vec![
                Pickup {
                    placement: 12,
                    gives: Gives::Weapon(0x09),
                    position: [1.0, 2.0, 3.0],
                },
                Pickup {
                    placement: 33,
                    gives: Gives::Grenades,
                    position: [-1.0, 0.0, 3.5],
                },
            ],
            triggers: [record(1, 6.1e-5, 333.8), record(8, 0.0, -0.0)].concat(),
            nav: nav_bytes(&[node([1, NO_NODE, NO_NODE, NO_NODE, NO_NODE]), node([0; 5])]),
            lights: Lights {
                ambient: [50, 50, 52],
                lights: [When::Countdown, When::Shown(27)]
                    .map(|when| Light {
                        at: [-149.0, -1421.0, 0.5],
                        rgb: [255, 0, 0],
                        size: 85.0,
                        swing: 55.0,
                        step: 0.1021,
                        when,
                        flicker: when == When::Shown(27),
                    })
                    .into(),
            },
            outros: vec![sample_outro()],
            characters: vec![
                Character {
                    model_lift: 42,
                    clips: vec![],
                    root_clips: vec![],
                    looks: [(0x11, 0x3A), (0x1C, 0x0C)].into(),
                },
                Character {
                    model_lift: 43,
                    clips: vec![ClipInfo {
                        pair: 3,
                        logical: vec![0, 0x77],
                        frames: 12,
                        duration_ticks: 23,
                    }],
                    root_clips: vec![RootClip {
                        logical: vec![0x77],
                        duration: 23,
                        step: 2,
                        z: vec![0, -32, 4577],
                        end: -9,
                    }],
                    looks: Default::default(),
                },
            ],
            held: vec![Held {
                entry: 0x1A,
                joint: 9,
                create: vec![(0x7B, 15.0)],
                remove: vec![(0x7C, 13.999998)],
            }],
            acos: (0..ACOS_LEN as u16).collect(),
            sink: std::array::from_fn(|k| [k as i8, -(k as i8), 90]),
            ai: vec![1, 2, 3],
            class: vec![],
        }
    }

    fn sample_outro() -> outro::Outro {
        use outro::*;
        let key = |t, value, catmull| Key { t, value, catmull };
        Outro {
            winner: 1,
            cutscene: 4,
            length: 800,
            camera: Camera {
                path: vec![[-2849, -481, 6019], [-2848, -481, 5907]],
                along: vec![key(0, 0.0, false), key(800, 100.0, false)],
                yaw: vec![key(0, 64.0, true), key(800, 108.0, true)],
                pitch: vec![key(1, 12.0, false)],
                from: 0,
            },
            cuts: vec![Camera {
                path: vec![[-331, -190, 3569]],
                along: vec![],
                yaw: vec![key(800, 243.0, false)],
                pitch: vec![],
                from: 800,
            }],
            actor: Actor {
                character: "90".into(),
                animation: "pair_0045_anim_0056".into(),
                ticks: 51,
                rate: 1.2,
                at: [-3000, -521, 5948],
                facing: 182,
                lift: 0,
                scale: 0.5,
                once: false,
                path: vec![],
                along: vec![],
                yaw: vec![],
                plays: vec![],
            },
            cast: vec![Actor {
                character: "40".into(),
                animation: "pair_0002_anim_0004".into(),
                ticks: 26,
                rate: 1.04,
                at: [-140, -420, 2895],
                facing: 16384,
                lift: 0,
                scale: 0.5,
                once: true,
                path: vec![[-140, -420, 2895], [-166, -395, 3148]],
                along: vec![key(0, 0.0, true), key(1100, 100.0, true)],
                yaw: vec![key(0, 0.0, false)],
                plays: vec![Play {
                    t: 50,
                    animation: "pair_0001_anim_0002".into(),
                    ticks: 30,
                    rate: 0.5,
                    once: false,
                }],
            }],
            fade: Fade {
                rgb: [0; 3],
                duration: 25,
            },
            voice: Some(Voice {
                t: 30,
                entry: 0xB3,
                file: "maps/2d/voice-b3.wav".into(),
            }),
            subtitles: Some(Subtitles {
                pages: vec![Page {
                    t: 16,
                    lines: vec!["are victorious !".into(), " ".into()],
                }],
                close: 745,
                anchor: hud::Anchor(hud::Horizontal::Left, hud::Vertical::Top),
                text: [71, 28],
                quads: vec![Quad {
                    texture: "bubble".into(),
                    rect: [124, 1, 173, 57],
                    uv: [63.0, 0.21875, -0.359375, 62.75],
                    colour: [255; 4],
                }],
                moves: vec![Place {
                    t: 100,
                    anchor: hud::Anchor(hud::Horizontal::Left, hud::Vertical::Top),
                    text: [100, 25],
                    quads: vec![],
                }],
            }),
        }
    }

    const CHARACTERS: [&str; 2] = ["128", "117"];

    fn files(slice: &Slice) -> BTreeMap<String, Vec<u8>> {
        let manifest = Manifest {
            format: FORMAT,
            source_rom_sha1: "00".into(),
            maps: vec!["2d".into()],
            characters: CHARACTERS.map(String::from).into(),
            weapons: vec!["deathmatch".into()],
            audio: vec![],
            hud: false,
        };
        let mut files: BTreeMap<_, _> = slice
            .write("2d", &CHARACTERS, "deathmatch")
            .unwrap()
            .into_iter()
            .collect();
        files.insert(MANIFEST.into(), manifest.to_json());
        files
    }

    fn load(files: &BTreeMap<String, Vec<u8>>) -> Result<Slice> {
        Slice::load("2d", &CHARACTERS, "deathmatch", |p| {
            files.get(p).cloned().ok_or("missing")
        })
    }

    fn bits(s: &Slice) -> Vec<u32> {
        (s.collision
            .iter()
            .flat_map(|t| t.points.into_iter().flatten()))
        .chain(s.spawns.iter().flat_map(|p| p.camera))
        .chain(s.pickups.iter().flat_map(|p| p.position))
        .chain(
            s.held
                .iter()
                .flat_map(|h| h.create.iter().chain(&h.remove).map(|e| e.1)),
        )
        .map(f32::to_bits)
        .collect()
    }

    #[test]
    fn write_then_load_gives_the_same_slice_bit_for_bit() {
        let slice = sample();
        let files = files(&slice);
        assert_eq!(
            files.len(),
            Slice::files("2d", &CHARACTERS, "deathmatch").len()
        );
        let back = load(&files).unwrap();
        assert_eq!(back, slice);
        // Each character in its folder, the file as before (`root_motion`).
        let tedi = String::from_utf8(files["characters/117/character.json"].clone()).unwrap();
        assert!(tedi.starts_with("{\n  \"model_lift\": 43,"), "{tedi}");
        assert!(
            tedi.contains("\"root_motion\": [\n    {\"logical\":[119]"),
            "{tedi}"
        );
        let squirrel = String::from_utf8(files["characters/128/character.json"].clone()).unwrap();
        assert!(
            squirrel.starts_with("{\n  \"model_lift\": 42,"),
            "{squirrel}"
        );
        assert_eq!(bits(&back), bits(&slice));
        // Integers stay integers, -0.0 keeps its sign, and one triangle per line.
        let collision = String::from_utf8(files["maps/2d/collision.json"].clone()).unwrap();
        assert!(
            collision.contains("[[-2267,-521,6016],[0,-0.0,1],[2,3,4]],\"0x0010000a\"]"),
            "{collision}"
        );
        assert!(
            collision.contains("[[0.1,113.399994,-7.5e-8],[10000000000.0,3,4],[5,6,7]],null]"),
            "{collision}"
        );
        assert_eq!(collision.matches("\"layer\"").count(), 3);
    }

    #[test]
    fn pretty_to_a_depth_then_compact() {
        let v = BTreeMap::from([("a", vec![[1, 2], [3, 4]]), ("b", vec![])]);
        let text = |depth| String::from_utf8(to_json(&v, depth)).unwrap();
        assert_eq!(text(0), "{\"a\":[[1,2],[3,4]],\"b\":[]}\n");
        assert_eq!(text(1), "{\n  \"a\": [[1,2],[3,4]],\n  \"b\": []\n}\n");
        assert_eq!(
            text(2),
            "{\n  \"a\": [\n    [1,2],\n    [3,4]\n  ],\n  \"b\": []\n}\n"
        );
    }

    #[test]
    fn errors_name_the_file_and_the_field() {
        let base = files(&sample());
        let error = |path: &str, edit: &dyn Fn(String) -> Option<String>| {
            let mut f = base.clone();
            match edit(String::from_utf8(f[path].clone()).unwrap()) {
                Some(text) => f.insert(path.into(), text.into()),
                None => f.remove(path),
            };
            load(&f).unwrap_err().to_string()
        };
        let e = error("maps/2d/spawns.json", &|t| {
            Some(t.replace("\"yaw\"", "\"jaw\""))
        });
        assert!(
            e.starts_with("maps/2d/spawns.json: missing field `yaw`"),
            "{e}"
        );
        let e = error("maps/2d/collision.json", &|t| {
            Some(t.replace("0x0010000a", "10000a"))
        });
        assert!(
            e.starts_with("maps/2d/collision.json: surface word \"10000a\""),
            "{e}"
        );
        assert_eq!(
            error("maps/2d/pickups.json", &|_| None),
            "maps/2d/pickups.json: missing"
        );
        let e = error("game/acos.json", &|_| Some("[1, 2]".into()));
        assert_eq!(e, "game/acos.json: (table): 132 entries, not 2");
        let node =
            r#"[{"pos":[0,0,0],"band":0,"id":0,"half":0,"next":[1,255,255,255,255],"barrier":0}]"#;
        let e = error("maps/2d/nav.json", &|_| Some(node.into()));
        assert_eq!(e, "maps/2d/nav.json: [0].next: no node 1");
        let e = error("maps/2d/triggers.json", &|t| {
            Some(t.replacen("\"kind\": 1", "\"kind\": 64", 1))
        });
        assert_eq!(
            e,
            "maps/2d/triggers.json: [0]: kind 64 (0-63) and shape 3 (0-3)"
        );
        let e = error(MANIFEST, &|t| {
            Some(t.replace("\"format\": 1", "\"format\": 2"))
        });
        assert_eq!(
            e,
            "pack.json: format: 2, this build reads 1: re-export the pack"
        );
        let e = error(MANIFEST, &|t| Some(t.replace("117", "0")));
        assert_eq!(e, "pack.json: characters: no \"117\"");
        let e = error("characters/128/character.json", &|_| None);
        assert_eq!(e, "characters/128/character.json: missing");
    }

    #[test]
    fn tables_the_pack_cannot_keep_are_refused() {
        let mut slice = sample();
        slice.triggers[0x30] = 1; // run-time bytes
        assert!(slice.write("2d", &CHARACTERS, "deathmatch").is_err());
        let mut slice = sample();
        slice.nav.push(0);
        assert!(slice.write("2d", &CHARACTERS, "deathmatch").is_err());
    }
}
