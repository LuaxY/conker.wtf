//! Per-scene data for Conker's Bad Fur Day (USA): object placements, multiplayer spawn points and
//! static collision.
//!
//! # Scene ids
//! A game scene id (the scene in play, `D_800BE9F0`; 0x2D for the oracle's Deathmatch map) is the
//! entry index in every per-scene bank, unchanged:
//! - The scene loader `func_15007B3C` passes `D_800BE9F0` as-is to `func_150031EC` (bundle
//!   `[0x04, scene]`, ids ≥ 0x45 become 0), `func_150039E0` (placements `[0x0B, scene]` and
//!   `[0x0C, scene, 2]`), `func_15003668` (collision) and `func_15016690` (spawns `[0x0C, scene, 6]`).
//! - Oracle RAM during Deathmatch on scene 0x2D: the placement count `D_800DBEF0` is 46 = 12 + 34,
//!   the record counts of `[0x0B, 0x2D]` and `[0x0C, 0x2D, 2]` (the u8 at 0x800DBF00 holds the 12);
//!   `D_800D2128` = 20 and the 0x1E0 bytes at `*D_800D212C` equal `[0x0C, 0x2D, 6]`.
//!
//! # Sources
//! Placement and collision contracts are ported from DevOldSchool/conkers-bfd-decomp (MIT, commit
//! e2400b27dda6): `docs/rzip-assets.md`, `docs/evidence/assets/models/us_asset_inventory.md` and
//! `scripts/model_assets.py` (`placement_record_manifest`, `placement_transform_matrix`,
//! `packed_four_triangle_indices`, `bank_04_collision_surface_words`). The opcodes the collision
//! builder `func_15001460` walks, the spawn table and the scene-id mapping were read here from the
//! recompiled game code and oracle RAM. Surface-word meanings are not known yet.

use extract_core::{Error, Result, Rom, Table};

/// One placement record.
pub const PLACEMENT_SIZE: usize = 0x44;
/// One spawn-point record.
pub const SPAWN_SIZE: usize = 0x18;
/// Model header size; vertices follow it, and segment 1 points there.
const MODEL_HEADER: usize = 0x28;

fn region(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or(Error::OutOfBounds {
            offset,
            len,
            have: data.len(),
        })
}

fn word(data: &[u8], offset: usize) -> Result<u32> {
    region(data, offset, 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()))
}

/// Splits a table into `N`-byte records as the game does (`size / N`). Up to 15 zero bytes of
/// alignment padding may follow (bank-0B entries have them).
fn records<const N: usize>(data: &[u8]) -> Result<&[[u8; N]]> {
    let (records, rest) = data.as_chunks::<N>();
    if rest.len() >= 16 || rest.iter().any(|&b| b != 0) {
        return Err(Error::Malformed("record table has trailing bytes"));
    }
    Ok(records)
}

/// Child `child` of the scene's bank-0C table; empty when the scene has no such table.
fn scene_child(rom: &Rom, scene: usize, child: usize) -> Result<Vec<u8>> {
    let table = rom.asset(&[0x0C, scene])?;
    if table.is_empty() {
        return Ok(Vec::new());
    }
    Ok(Table::parse(&table)?.payload(child)?.into_owned())
}

/// A 0x44-byte object placement, read by `func_150039E0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// `+0x00`: world position, in the same units as the game's f32 object positions.
    pub position: [i16; 3],
    /// `+0x06`: Euler angles in degrees, applied X, then Z, then Y (`func_151148A8`).
    pub rotation: [i16; 3],
    /// `+0x0C`: 0 = model `[0x03, model]` (records from `[0x0C, scene, 2]`); 1 or 2 = segment
    /// `model` of the scene bundle `[0x04, scene]` (records from `[0x0B, scene]`).
    pub dispatch: u32,
    /// `+0x10`: model index, see `dispatch`.
    pub model: u32,
    /// `+0x20`: per-axis model scale.
    pub scale: [f32; 3],
    /// `+0x32`, copied to the runtime object's `+0x4F`.
    pub flags: u8,
    /// The whole record, for the fields not named yet.
    pub raw: [u8; PLACEMENT_SIZE],
}

impl Placement {
    pub fn parse(raw: &[u8; PLACEMENT_SIZE]) -> Self {
        let h = |o: usize| i16::from_be_bytes([raw[o], raw[o + 1]]);
        let w = |o: usize| u32::from_be_bytes(raw[o..o + 4].try_into().unwrap());
        Self {
            position: [h(0), h(2), h(4)],
            rotation: [h(6), h(8), h(0xA)],
            dispatch: w(0xC),
            model: w(0x10),
            scale: [0x20, 0x24, 0x28].map(|o| f32::from_bits(w(o))),
            flags: raw[0x32],
            raw: *raw,
        }
    }

    /// `func_15003668` adds the model's triangles to the collision unless `(flags & 0x60) == 0x20`.
    pub fn has_collision(&self) -> bool {
        self.flags & 0x60 != 0x20
    }

    /// Model space to world space: scale, rotate about X, Z, then Y, translate (`func_1511490C`).
    pub fn transform(&self, v: [f32; 3]) -> [f32; 3] {
        let [x, y, z]: [f32; 3] = std::array::from_fn(|i| v[i] * self.scale[i]);
        let [ax, ay, az] = self.rotation.map(|d| f32::from(d).to_radians());
        let (s, c) = ax.sin_cos();
        let (y, z) = (y * c - z * s, y * s + z * c);
        let (s, c) = az.sin_cos();
        let (x, y) = (x * c - y * s, x * s + y * c);
        let (s, c) = ay.sin_cos();
        let (x, z) = (x * c + z * s, z * c - x * s);
        let [px, py, pz] = self.position.map(f32::from);
        [x + px, y + py, z + pz]
    }
}

/// Parses a placement table (`[0x0B, scene]` or `[0x0C, scene, 2]`).
pub fn parse_placements(data: &[u8]) -> Result<Vec<Placement>> {
    Ok(records(data)?.iter().map(Placement::parse).collect())
}

/// All placements of `scene` in runtime order: `[0x0B, scene]`, then `[0x0C, scene, 2]`.
pub fn scene_placements(rom: &Rom, scene: usize) -> Result<Vec<Placement>> {
    let mut placements = parse_placements(&rom.asset(&[0x0B, scene])?)?;
    placements.extend(parse_placements(&scene_child(rom, scene, 2)?)?);
    Ok(placements)
}

/// A 0x18-byte multiplayer spawn point from `[0x0C, scene, 6]`: `func_15016690` loads the table
/// into `D_800D212C` (count `D_800D2128`) and the spawn picker `func_15084D70` copies these fields.
///
/// In multiplayer the picker then moves the spawn by `r` along the facing, `x += sin(yaw) * r`,
/// `z += cos(yaw) * r`, with `r = f * 80 - 40` where `f` comes from `func_150ADA68` (presumably a
/// uniform [0, 1) RNG, not checked). Read in `func_15084D70`; two oracle spawns sit 16 and 36
/// units off their records along the facing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnPoint {
    /// `+0x00`: world position (the player's f32 position before the jitter above).
    pub position: [i16; 3],
    /// `+0x06`: copied out by the picker, meaning unknown.
    pub byte_06: u8,
    /// `+0x07`: facing in 1/256 turns; the object's u16 facing angle is `yaw << 8`.
    pub yaw: u8,
    /// `+0x08`: initial camera eye (equals the oracle's camera at match start).
    pub camera: [f32; 3],
    /// `+0x14`: copied out by the picker, meaning unknown.
    pub byte_14: u8,
    /// `+0x15`: copied out (signed) by the picker, meaning unknown.
    pub byte_15: i8,
}

impl SpawnPoint {
    pub fn parse(raw: &[u8; SPAWN_SIZE]) -> Self {
        let h = |o: usize| i16::from_be_bytes([raw[o], raw[o + 1]]);
        let f = |o: usize| f32::from_be_bytes(raw[o..o + 4].try_into().unwrap());
        Self {
            position: [h(0), h(2), h(4)],
            byte_06: raw[6],
            yaw: raw[7],
            camera: [f(8), f(0xC), f(0x10)],
            byte_14: raw[0x14],
            byte_15: raw[0x15] as i8,
        }
    }
}

pub fn parse_spawns(data: &[u8]) -> Result<Vec<SpawnPoint>> {
    Ok(records(data)?.iter().map(SpawnPoint::parse).collect())
}

/// Spawn points of `scene` (`[0x0C, scene, 6]`).
pub fn scene_spawns(rom: &Rom, scene: usize) -> Result<Vec<SpawnPoint>> {
    parse_spawns(&scene_child(rom, scene, 6)?)
}

/// A model's triangles as the collision builder `func_15001460` sees them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Model-space positions of the 16-byte vertices at `+0x28`.
    pub vertices: Vec<[i16; 3]>,
    /// Vertex indices, one triple per display-list triangle, in list order (zero-area ones too).
    pub faces: Vec<[u32; 3]>,
}

/// The 12 five-bit vertex-cache slots of Conker's four-triangle opcodes 0x10-0x1F.
fn tri4(w0: u32, w1: u32) -> [u32; 12] {
    [
        w1 >> 25,
        w1 >> 20,
        w1 >> 15,
        w1 >> 10,
        w1 >> 5,
        w1,
        w0 >> 10,
        w0 >> 5,
        w0,
        w0 >> 23,
        w0 >> 18,
        (w1 >> 30) | (w0 >> 15 & 7) << 2,
    ]
    .map(|s| s & 0x1F)
}

/// The three cache slots of a G_TRI1 word (indices are stored doubled).
fn tri1(w: u32) -> [u32; 3] {
    [w >> 16, w >> 8, w].map(|b| (b & 0xFF) / 2)
}

/// Parses a 0x28-byte-header model (banks 03 and 04): vertices, then the primary display list
/// (header words 0/1) up to its first G_ENDDL. Only G_VTX, G_TRI1, G_TRI2 and 0x10-0x1F make
/// triangles, the set `func_15001460` handles.
pub fn parse_mesh(model: &[u8]) -> Result<Mesh> {
    let (dl, dl_size) = (word(model, 0)? as usize, word(model, 4)? as usize);
    if dl < MODEL_HEADER || !(dl - MODEL_HEADER).is_multiple_of(16) {
        return Err(Error::Malformed(
            "model display list is not after its vertices",
        ));
    }
    let vertices: Vec<[i16; 3]> = region(model, MODEL_HEADER, dl - MODEL_HEADER)?
        .as_chunks::<16>()
        .0
        .iter()
        .map(|v| [0, 2, 4].map(|o| i16::from_be_bytes([v[o], v[o + 1]])))
        .collect();
    let mut cache = [None::<u32>; 32];
    let mut faces = Vec::new();
    for cmd in region(model, dl, dl_size)?.as_chunks::<8>().0 {
        let w0 = u32::from_be_bytes(cmd[..4].try_into().unwrap());
        let w1 = u32::from_be_bytes(cmd[4..].try_into().unwrap());
        let slots: &[u32] = match w0 >> 24 {
            0x01 => {
                let count = (w0 >> 12 & 0xFF) as usize;
                let end = (w0 >> 1 & 0x7F) as usize;
                let offset = (w1 & 0xFF_FFFF) as usize;
                let first = offset / 16;
                if w1 >> 24 != 1
                    || !offset.is_multiple_of(16)
                    || count > end
                    || end > cache.len()
                    || first + count > vertices.len()
                {
                    return Err(Error::Malformed("invalid vertex load"));
                }
                for i in 0..count {
                    cache[end - count + i] = Some((first + i) as u32);
                }
                continue;
            }
            0x05 => &tri1(w0),
            0x06 => &[tri1(w0), tri1(w1)].concat(),
            0x10..=0x1F => &tri4(w0, w1),
            0xDF => break,
            _ => continue,
        };
        for t in slots.as_chunks::<3>().0 {
            let slot = |s: u32| {
                cache
                    .get(s as usize)
                    .copied()
                    .flatten()
                    .ok_or(Error::Malformed("triangle uses an unloaded vertex"))
            };
            faces.push([slot(t[0])?, slot(t[1])?, slot(t[2])?]);
        }
    }
    Ok(Mesh { vertices, faces })
}

/// Static collision of one scene bundle `[0x04, scene]`. `func_15003668` builds triangles from
/// bundle segments 0 and 3; `func_150039BC` installs segment 0's surface table.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// Segment 0.
    pub mesh: Mesh,
    /// One surface word per `mesh` face, from segment 0's tertiary region (header words 4/5:
    /// an 8-byte header whose second word is 5, then the words). None when the model has none.
    pub surfaces: Option<Vec<u32>>,
    /// Segment 3, an extra static layer without a surface table, when present.
    pub secondary: Option<Mesh>,
}

/// Parses a scene bundle's static collision; None when the bundle or its segment 0 is empty.
pub fn parse_terrain(bundle: &[u8]) -> Result<Option<Terrain>> {
    if bundle.is_empty() {
        return Ok(None);
    }
    let segments = Table::parse(bundle)?;
    let primary = segments.raw(0)?;
    if primary.is_empty() {
        return Ok(None);
    }
    let mesh = parse_mesh(primary)?;
    let (offset, size) = (word(primary, 0x10)? as usize, word(primary, 0x14)? as usize);
    let surfaces = if offset == 0 {
        None
    } else {
        let table = region(primary, offset, size)?;
        if size != 8 + 4 * mesh.faces.len() || word(table, 4)? != 5 {
            return Err(Error::Malformed(
                "surface table does not match the terrain faces",
            ));
        }
        Some(
            table[8..]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| u32::from_be_bytes(*w))
                .collect(),
        )
    };
    let secondary = match segments.raw(3) {
        Ok(s) if !s.is_empty() => Some(parse_mesh(s)?),
        _ => None,
    };
    Ok(Some(Terrain {
        mesh,
        surfaces,
        secondary,
    }))
}

/// Where a collision triangle comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Bundle segment 0.
    Terrain,
    /// Bundle segment 3.
    Secondary,
    /// The model of placement `n` (an index into [`scene_placements`]).
    Placement(usize),
}

/// A world-space collision triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub layer: Layer,
    /// The terrain surface word; None where no table is installed (secondary and placements).
    pub surface: Option<u32>,
}

/// A scene's collision in the order `func_15003668` builds it.
#[derive(Debug, Clone, Default)]
pub struct SceneCollision {
    pub triangles: Vec<Triangle>,
    /// Placements that add collision but whose model is missing (scenes 17 and 62).
    pub unresolved: Vec<usize>,
}

/// The placements whose models collide in a Deathmatch on `scene` (V in the oracle's RAM and under lldb,
/// `lab/oracle/walls_README.md`): the game's layer 2, the props with `(+0x4F & 0x60) != 0x20` and `+0x6E`
/// clear (`func_150A6568` and the line query's copy of it), which the floor query, the wall routine and the
/// line query all search. Fewer than [`Placement::has_collision`]: the match sets `+0x4F |= 0x20` on its
/// pickups and hides the other modes' props. 0x2B: War: Beach's (scenario 8, the RAM's `+0x4F`/`+0x6E`;
/// 1's `+0x4F` is 0x51, the first 0x40 prop seen: I, the same set). Other scenes: none.
pub fn deathmatch_solid(scene: usize) -> &'static [usize] {
    match scene {
        0x2D => &[2, 3, 4, 5, 6, 7, 10, 11, 40, 41, 42, 43, 44, 45],
        0x34 => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 29, 30, 31, 32],
        0x2B => &[0, 1, 5],
        _ => &[],
    }
}

/// Builds `scene`'s collision: terrain, secondary layer, then every placement (from
/// [`scene_placements`]) that adds collision, transformed to world space.
pub fn scene_collision(
    rom: &Rom,
    scene: usize,
    placements: &[Placement],
) -> Result<SceneCollision> {
    let bundle = rom.asset(&[0x04, scene])?;
    let mut out = SceneCollision::default();
    let world = |v: [i16; 3]| v.map(f32::from);
    if let Some(t) = parse_terrain(&bundle)? {
        push(
            &mut out.triangles,
            &t.mesh,
            Layer::Terrain,
            t.surfaces.as_deref(),
            world,
        );
        if let Some(s) = &t.secondary {
            push(&mut out.triangles, s, Layer::Secondary, None, world);
        }
    }
    let segments = if bundle.is_empty() {
        None
    } else {
        Some(Table::parse(&bundle)?)
    };
    for (i, p) in placements
        .iter()
        .enumerate()
        .filter(|(_, p)| p.has_collision())
    {
        let model = if p.dispatch == 0 {
            rom.asset(&[0x03, p.model as usize])?.into_owned()
        } else {
            match &segments {
                Some(s) if (p.model as usize) < s.entries.len() => {
                    s.raw(p.model as usize)?.to_vec()
                }
                _ => Vec::new(),
            }
        };
        if model.is_empty() {
            out.unresolved.push(i);
            continue;
        }
        let mesh = parse_mesh(&model)?;
        push(&mut out.triangles, &mesh, Layer::Placement(i), None, |v| {
            p.transform(v.map(f32::from))
        });
    }
    Ok(out)
}

fn push(
    out: &mut Vec<Triangle>,
    mesh: &Mesh,
    layer: Layer,
    surfaces: Option<&[u32]>,
    place: impl Fn([i16; 3]) -> [f32; 3],
) {
    out.extend(mesh.faces.iter().enumerate().map(|(i, face)| Triangle {
        points: face.map(|v| place(mesh.vertices[v as usize])),
        layer,
        surface: surfaces.map(|s| s[i]),
    }));
}

/// The highest triangle under `p` whose height at `p`'s XZ position is at most `p.y + tolerance`
/// (walls, being vertical, never count): `(index, floor height)`.
// ponytail: linear scan over every triangle; fine for offline checks, use a grid/BVH for per-tick queries.
pub fn floor_below(triangles: &[Triangle], p: [f32; 3], tolerance: f32) -> Option<(usize, f32)> {
    triangles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| height_at(&t.points, p[0], p[2]).map(|y| (i, y)))
        .filter(|&(_, y)| y <= p[1] + tolerance)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Height of the triangle's plane at (x, z) when (x, z) lies inside its XZ projection.
fn height_at(t: &[[f32; 3]; 3], x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = t.map(|p| p.map(f64::from));
    let (bx, bz, cx, cz) = (b[0] - a[0], b[2] - a[2], c[0] - a[0], c[2] - a[2]);
    let (px, pz) = (f64::from(x) - a[0], f64::from(z) - a[2]);
    let det = bx * cz - cx * bz;
    if det.abs() < 1e-9 {
        return None;
    }
    let u = (px * cz - cx * pz) / det;
    let v = (bx * pz - px * bz) / det;
    let eps = 1e-9;
    (u >= -eps && v >= -eps && u + v <= 1.0 + eps)
        .then(|| (a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1])) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement_record() -> [u8; PLACEMENT_SIZE] {
        let mut r = [0u8; PLACEMENT_SIZE];
        for (i, v) in [-100i16, -2000, 3000, 90, -45, 30].iter().enumerate() {
            r[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        r[0xC..0x10].copy_from_slice(&2u32.to_be_bytes());
        r[0x10..0x14].copy_from_slice(&7u32.to_be_bytes());
        for (i, s) in [1.5f32, 2.0, 0.5].iter().enumerate() {
            r[0x20 + i * 4..0x24 + i * 4].copy_from_slice(&s.to_be_bytes());
        }
        r[0x32] = 0x11;
        r
    }

    #[test]
    fn parses_placements() {
        let raw = placement_record();
        let mut table = [raw, raw].concat();
        table[PLACEMENT_SIZE + 0x32] = 0x20;
        table.extend([0; 4]); // bank-0B alignment padding
        let p = parse_placements(&table).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].position, [-100, -2000, 3000]);
        assert_eq!(p[0].rotation, [90, -45, 30]);
        assert_eq!((p[0].dispatch, p[0].model, p[0].flags), (2, 7, 0x11));
        assert_eq!(p[0].scale, [1.5, 2.0, 0.5]);
        assert_eq!(p[0].raw, raw);
        assert!(p[0].has_collision() && !p[1].has_collision());

        table.push(1); // non-zero trailing byte
        assert!(parse_placements(&table).is_err());
        assert!(parse_placements(&[0; PLACEMENT_SIZE + 16]).is_err());
        assert!(parse_placements(&[]).unwrap().is_empty());
    }

    #[test]
    fn collision_flag_mask() {
        let mut p = Placement::parse(&placement_record());
        for (flags, collides) in [
            (0x00, true),
            (0x20, false),
            (0x21, false),
            (0x40, true),
            (0x60, true),
            (0xA0, false),
        ] {
            p.flags = flags;
            assert_eq!(p.has_collision(), collides, "flags {flags:#x}");
        }
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn transforms_like_the_game() {
        let mut p = Placement::parse(&placement_record());
        p.position = [10, 20, 30];
        p.scale = [1.0; 3];
        let at = |p: &Placement, v| p.transform(v);
        p.rotation = [0, 90, 0];
        assert!(close(at(&p, [1.0, 0.0, 0.0]), [10.0, 20.0, 29.0])); // +X turns to -Z
        p.rotation = [0, 0, 90];
        assert!(close(at(&p, [1.0, 0.0, 0.0]), [10.0, 21.0, 30.0])); // +X turns to +Y
        p.rotation = [90, 0, 0];
        assert!(close(at(&p, [0.0, 1.0, 0.0]), [10.0, 20.0, 31.0])); // +Y turns to +Z

        // DevOldSchool's explicit matrix (placement_transform_matrix) at arbitrary angles.
        p.rotation = [269, 323, -40];
        p.scale = [2.3, 1.0, 2.76];
        let (x, y, z) = (
            269f64.to_radians(),
            323f64.to_radians(),
            (-40f64).to_radians(),
        );
        let (cx, cy, cz, sx, sy, sz) = (x.cos(), y.cos(), z.cos(), x.sin(), y.sin(), z.sin());
        let r = [
            [cz * cy, -cx * sz * cy + sx * sy, sx * sz * cy + cx * sy],
            [sz, cx * cz, -sx * cz],
            [-cz * sy, cx * sz * sy + sx * cy, -sx * sz * sy + cx * cy],
        ];
        let v = [3.0f64, -7.0, 11.0];
        let s = p.scale.map(f64::from);
        let want: [f32; 3] = std::array::from_fn(|row| {
            ((0..3).map(|c| r[row][c] * s[c] * v[c]).sum::<f64>() + f64::from(p.position[row]))
                as f32
        });
        assert!(close(at(&p, v.map(|c| c as f32)), want));
    }

    #[test]
    fn parses_spawns() {
        let mut r = [0u8; SPAWN_SIZE];
        r[..6].copy_from_slice(&[0x03, 0xE8, 0xFF, 0x9C, 0xF8, 0x30]);
        r[6..8].copy_from_slice(&[0xFD, 0x80]);
        r[8..12].copy_from_slice(&1.5f32.to_be_bytes());
        r[12..16].copy_from_slice(&(-2.0f32).to_be_bytes());
        r[16..20].copy_from_slice(&(-300f32).to_be_bytes());
        r[0x14..0x16].copy_from_slice(&[3, 0xFF]);
        let s = parse_spawns(&[r, r].concat()).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].position, [1000, -100, -2000]);
        assert_eq!((s[0].byte_06, s[0].yaw), (0xFD, 0x80));
        assert_eq!(s[0].camera, [1.5, -2.0, -300.0]);
        assert_eq!((s[0].byte_14, s[0].byte_15), (3, -1));
    }

    #[test]
    fn decodes_packed_triangles() {
        let slots: [u32; 12] = std::array::from_fn(|i| (i as u32 * 7 + 3) % 32);
        // Inverse of the bit layout: w1 holds slots 0-5 and the low 2 bits of slot 11.
        let w1 = (slots[11] & 3) << 30
            | slots[0] << 25
            | slots[1] << 20
            | slots[2] << 15
            | slots[3] << 10
            | slots[4] << 5
            | slots[5];
        // The opcode's low nibble holds bits of slot 9.
        let w0 = 1 << 28
            | slots[9] << 23
            | slots[10] << 18
            | (slots[11] >> 2) << 15
            | slots[6] << 10
            | slots[7] << 5
            | slots[8];
        assert_eq!(tri4(w0, w1), slots);
        assert_eq!(tri1(0x05_02_04_3E), [1, 2, 31]);
    }

    /// A model with `n` vertices (vertex i = (i, 2i, -i)), the display list `dl` and an optional
    /// tertiary region.
    fn model(n: usize, dl: &[[u32; 2]], tertiary: Option<&[u8]>) -> Vec<u8> {
        let dl_offset = MODEL_HEADER + 16 * n;
        let mut m = vec![0; dl_offset];
        m[0..4].copy_from_slice(&(dl_offset as u32).to_be_bytes());
        m[4..8].copy_from_slice(&((dl.len() * 8) as u32).to_be_bytes());
        for i in 0..n {
            let o = MODEL_HEADER + 16 * i;
            for (k, c) in [i as i16, 2 * i as i16, -(i as i16)].iter().enumerate() {
                m[o + 2 * k..o + 2 * k + 2].copy_from_slice(&c.to_be_bytes());
            }
        }
        m.extend(
            dl.iter()
                .flat_map(|[a, b]| [a.to_be_bytes(), b.to_be_bytes()])
                .flatten(),
        );
        if let Some(t) = tertiary {
            let offset = m.len() as u32;
            m[0x10..0x14].copy_from_slice(&offset.to_be_bytes());
            m[0x14..0x18].copy_from_slice(&(t.len() as u32).to_be_bytes());
            m.extend(t);
        }
        m
    }

    /// G_VTX loading `count` vertices from vertex `first` into slots ending at `end`.
    fn vtx(count: u32, end: u32, first: u32) -> [u32; 2] {
        [
            0x0100_0000 | count << 12 | end << 1,
            0x0100_0000 | (first * 16),
        ]
    }

    #[test]
    fn parses_mesh() {
        let tri4_all_zero = [0x1000_0000, 0]; // four triangles on slot 0
        let dl = [
            vtx(4, 4, 1),               // slots 0..4 = vertices 1..5
            [0x0500_0204, 0],           // G_TRI1 slots 0,1,2
            [0xE700_0000, 0],           // ignored
            [0x0600_0406, 0x0000_0204], // G_TRI2 slots (0,2,3), (0,1,2)
            tri4_all_zero,
            [0xDF00_0000, 0],
            [0x0500_0002, 0], // after G_ENDDL: never read
        ];
        let mesh = parse_mesh(&model(6, &dl, None)).unwrap();
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.vertices[5], [5, 10, -5]);
        assert_eq!(
            mesh.faces,
            [
                [1, 2, 3],
                [1, 3, 4],
                [1, 2, 3],
                [1, 1, 1],
                [1, 1, 1],
                [1, 1, 1],
                [1, 1, 1]
            ]
        );

        let unloaded = [vtx(1, 1, 0), [0x0500_0002, 0]];
        assert!(parse_mesh(&model(2, &unloaded, None)).is_err());
        let past_end = [vtx(2, 2, 1)];
        assert!(parse_mesh(&model(2, &past_end, None)).is_err());
        assert!(parse_mesh(&[0; 8]).is_err());
        let mut truncated = model(2, &past_end, None);
        truncated[3] = 0xF0; // display list (and vertices) past the end
        assert!(parse_mesh(&truncated).is_err());
    }

    fn bundle(segments: &[Vec<u8>]) -> Vec<u8> {
        let head = segments.len() * 8;
        let mut table = Vec::new();
        let mut body = Vec::new();
        for (i, s) in segments.iter().enumerate() {
            let last = if i + 1 == segments.len() { 1 << 31 } else { 0 };
            table.extend(((head + body.len()) as u32).to_be_bytes());
            table.extend((s.len() as u32 | last).to_be_bytes());
            body.extend(s);
        }
        [table, body].concat()
    }

    #[test]
    fn parses_terrain() {
        let dl = [vtx(3, 3, 0), [0x0600_0204, 0x0004_0200], [0xDF00_0000, 0]];
        let mut table = vec![0, 0, 0, 0, 0, 0, 0, 5];
        table.extend([0xAAu32, 0x6000_0005].iter().flat_map(|w| w.to_be_bytes()));
        let primary = model(3, &dl, Some(&table));
        let secondary = model(3, &[vtx(3, 3, 0), [0x0500_0204, 0]], None);
        let b = bundle(&[primary.clone(), vec![], vec![], secondary]);
        let t = parse_terrain(&b).unwrap().unwrap();
        assert_eq!(t.mesh.faces, [[0, 1, 2], [2, 1, 0]]);
        assert_eq!(t.surfaces, Some(vec![0xAA, 0x6000_0005]));
        assert_eq!(t.secondary.unwrap().faces, [[0, 1, 2]]);

        let plain = parse_terrain(&bundle(&[model(3, &dl, None), vec![]]))
            .unwrap()
            .unwrap();
        assert_eq!((plain.surfaces, plain.secondary), (None, None));
        assert!(parse_terrain(&[]).unwrap().is_none());
        assert!(
            parse_terrain(&bundle(&[vec![], primary]))
                .unwrap()
                .is_none()
        );

        table[7] = 4; // unknown table header
        assert!(parse_terrain(&bundle(&[model(3, &dl, Some(&table))])).is_err());
        table[7] = 5;
        table.truncate(12); // one word for two faces
        assert!(parse_terrain(&bundle(&[model(3, &dl, Some(&table))])).is_err());
    }

    #[test]
    fn finds_floors() {
        let quad = |y: f32, layer| {
            let (a, b, c, d) = (
                [-10.0, y, -10.0],
                [10.0, y, -10.0],
                [10.0, y, 10.0],
                [-10.0, y, 10.0],
            );
            [[a, b, c], [a, c, d]].map(|points| Triangle {
                points,
                layer,
                surface: None,
            })
        };
        let wall = Triangle {
            points: [[0.0, 0.0, 0.0], [0.0, 50.0, 0.0], [0.0, 0.0, 5.0]],
            layer: Layer::Terrain,
            surface: Some(1),
        };
        let tris: Vec<_> = [quad(0.0, Layer::Terrain), quad(30.0, Layer::Placement(3))]
            .concat()
            .into_iter()
            .chain([wall])
            .collect();
        assert_eq!(floor_below(&tris, [2.0, 5.0, 1.0], 0.0), Some((0, 0.0)));
        assert_eq!(floor_below(&tris, [-1.0, 5.0, 2.0], 0.0), Some((1, 0.0)));
        assert_eq!(floor_below(&tris, [2.0, 30.5, 1.0], 0.0), Some((2, 30.0)));
        assert_eq!(floor_below(&tris, [-1.0, 29.5, 2.0], 1.0), Some((3, 30.0)));
        assert_eq!(floor_below(&tris, [1.0, -1.0, 2.0], 0.0), None);
        assert_eq!(floor_below(&tris, [11.0, 50.0, 0.0], 0.0), None);
        // A sloped triangle: height interpolates.
        let slope = [Triangle {
            points: [[0.0, 0.0, 0.0], [10.0, 10.0, 0.0], [0.0, 0.0, 10.0]],
            layer: Layer::Terrain,
            surface: None,
        }];
        let (_, y) = floor_below(&slope, [5.0, 100.0, 2.0], 0.0).unwrap();
        assert!((y - 5.0).abs() < 1e-5);
    }
}
