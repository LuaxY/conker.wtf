//! The level's camera volumes: inside one the follow camera's wall ray and eye sweep are off
//! (`cam+0x84` bit 0x40000). Each frame, before the camera steps, the player update
//! (`func_1504A730` → `func_150A11C4` → `func_150A104C`) tests last frame's view eye against them.
//!
//! The volumes are trigger records of 0x34 bytes in the scene's asset `[0x0C, scene, 5]`, loaded at
//! scene start (`func_15017930`, sines filled in by `func_150A0D14`); the camera keeps the records of
//! type 1 (`+0x15 >> 2`) whose action (`+0x17`) is 0, 2 or 6, in table order.

use crate::math::{DEG_TO_RAD, cos_level, sin_level};

/// One camera volume record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraVolume {
    /// `+0x0`: position of the box's bottom centre.
    pub center: [i16; 3],
    /// `+0x6`: half-width (x), height (y, upward from the centre), half-depth (z), in the box's frame.
    pub size: [i16; 3],
    /// (sin, cos) of the pitch (`+0xC`) and of the yaw (`+0x10`), degrees (`+0x24..+0x30`).
    pub pitch: [f32; 2],
    pub yaw: [f32; 2],
    /// `+0x15 & 3`: 0 an upright cylinder (11 of scene 0x2D's 49), 2 an oriented box (38).
    pub shape: u8,
    /// A cylinder is turned only when an angle is above 0.01 degrees (`D_8009F644`).
    pub turned: bool,
    /// `+0x17`: inside, 0 turns the camera's wall checks off, 6 back on, 2 sets a ceiling
    /// (`cam+0x364` = top of the box; none on scene 0x2D).
    pub action: u8,
}

impl CameraVolume {
    /// The camera's volumes in a scene's trigger table (`[0x0C, scene, 5]`), in table order.
    pub fn parse_table(table: &[u8]) -> Vec<Self> {
        table
            .as_chunks::<0x34>()
            .0
            .iter()
            .filter(|r| r[0x15] >> 2 == 1 && matches!(r[0x17], 0 | 2 | 6))
            .map(Self::parse_record)
            .collect()
    }

    /// Any record of the trigger table (`*0x800D3098`, indexed by the level scripts' zone ids): the
    /// same zone test applies (`func_150A2D84` → `func_150A1DA0`).
    pub fn parse_record(r: &[u8; 0x34]) -> Self {
        let h = |o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
        let f = |o: usize| f32::from_be_bytes(r[o..o + 4].try_into().unwrap());
        let angle = |a: f32| {
            let r = a * DEG_TO_RAD;
            [sin_level(r), cos_level(r)]
        };
        Self {
            center: [h(0), h(2), h(4)],
            size: [h(6), h(8), h(0xA)],
            pitch: angle(f(0xC)),
            yaw: angle(f(0x10)),
            shape: r[0x15] & 3,
            turned: 0.01 < f(0xC) || 0.01 < f(0x10),
            action: r[0x17],
        }
    }

    /// Whether every offset from the centre fits the s16 the game truncates it to. One that does
    /// not (a body fallen out of the world, which the game never has) is in no volume, instead of
    /// wrapping into one (I; bots-beach-club §1.1: an escape fired at y −525,400).
    pub fn fits(&self, p: [f32; 3]) -> bool {
        (0..3).all(|k| (p[k] - f32::from(self.center[k])).abs() <= f32::from(i16::MAX))
    }

    /// `func_150A1DA0` for a camera (its `+0x16 & 1` floor test always passes for the eye): the offset
    /// from the centre, truncated to integers, turned by the yaw then the pitch, is strictly inside the
    /// box's half-width and half-depth, or the cylinder's radius (`func_150AD960`'s octagonal distance
    /// `|x| + |z| - min/2`), and between 0 and the height (exclusive).
    ///
    /// ponytail: the sphere (shape 1) and shape 3 read false; no camera volume of scene 0x2D uses them.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        if !self.fits(p) {
            return false;
        }
        let d: [i16; 3] = std::array::from_fn(|k| (p[k] - f32::from(self.center[k])) as i32 as i16);
        let [dx, dy, dz] = d.map(f32::from);
        let ([ps, pc], [ys, yc]) = (self.pitch, self.yaw);
        let [sx, sy, sz] = self.size.map(i32::from);
        let turn = |v: f32| v as i32 as i16 as i32;
        let zr = dz * yc + dx * ys;
        let (x, y, z) = if self.shape == 2 || self.turned {
            (
                turn(dx * yc - dz * ys),
                turn(dy * pc + zr * ps),
                turn(zr * pc - dy * ps),
            )
        } else {
            (i32::from(d[0]), i32::from(d[1]), i32::from(d[2]))
        };
        let across = match self.shape {
            0 => {
                let (ax, az) = (x.abs(), z.abs());
                ax + az - (ax.min(az) >> 1) < sx
            }
            2 => x < sx && -sx < x && z < sz && -sz < z,
            _ => false,
        };
        across && 0 < y && y < sy
    }

    /// `func_150A1DA0`'s shape 3 (C; V on ledge, where it fires once, at read 410): a trip wire. Turned
    /// like a box, the plane z = 0 was crossed from below to above between `old` and `new`, where it was
    /// crossed within the half-width (x) and between 0 and the height (y). Integers and the f32 order
    /// as in the game.
    pub fn crossed(&self, old: [f32; 3], new: [f32; 3]) -> bool {
        if !self.fits(old) || !self.fits(new) {
            return false;
        }
        let ([ps, pc], [ys, yc]) = (self.pitch, self.yaw);
        let t16 = |v: f32| i32::from(v as i32 as i16);
        let turn = |p: [f32; 3]| {
            let [dx, dy, dz] = std::array::from_fn(|k| {
                f32::from((p[k] - f32::from(self.center[k])) as i32 as i16)
            });
            let zr = dz * yc + dx * ys;
            [
                t16(dx * yc - dz * ys),
                t16(dy * pc + zr * ps),
                t16(zr * pc - dy * ps),
            ]
        };
        let ([x0, y0, z0], [x1, y1, z1]) = (turn(old), turn(new));
        if z0 >= 0 || z1 < 0 {
            return false;
        }
        let t = z0 as f32 / (z0 - z1) as f32;
        let x = (x0 as f32 + t * (x1 - x0) as f32).abs();
        let y = y0 as f32 + t * (y1 - y0) as f32;
        x < f32::from(self.size[0]) && 0.0 <= y && y < f32::from(self.size[1])
    }
}

/// A camera hint: a trigger record of type 0 (`+0x15 >> 2`) tested against the player's position, not
/// the eye. Each frame the player update (`func_150A11C4`, before the move) sends the camera every hint
/// the player is in; the camera reads the last one that same tick (`func_15128CB0`, `func_1512E8E0`,
/// skipped while R aims). C, V: scene 0x2D has one, record 82, a trip wire (shape 3) at the plateau's
/// east edge, kind 4 with command 30: the cut at ledge read 410 (wave 3 took it for a kind-5 hint; the
/// kind-5 path is not on 0x2D).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraHint {
    pub zone: CameraVolume,
    /// `+0x16 & 1`: only counts while the player is within 5 of its floor.
    pub on_floor: bool,
    /// `+0x17` (`cam+0x6FC`): the hint's kind.
    pub kind: u8,
    /// `+0x20` (`cam+0x704`): the command `func_1512FE00` runs (any kind but 0xA); 30 cuts.
    pub command: u32,
}

/// [`CameraHint::command`] that cuts behind the player (`func_1512FE00` case 30 → `func_15128774`).
pub const HINT_CUT: u32 = 30;

/// [`CameraHint::kind`] of the fixed cameras ([`Fixed`]).
pub const HINT_FIXED: u8 = 5;

/// [`Fixed::point`]s on scene 0x2B, the first two of `*0x800DC020` (the scene's asset `[0x0C, scene,
/// 8]`, 0x18-byte records, s16 x, y, z first), as the oracle read them at `cam+0x288` (V swim1 24-100
/// and 450-482: the sea's east and west sides, hints 0x26 and 0x27). Not in the pack yet; none elsewhere.
const SEA_POINTS: [[i16; 3]; 2] = [[2024, 277, 9703], [-2290, 728, 10913]];

/// A kind-5 hint's camera, mode 0x40 (`func_1512E8E0` → `func_15129934`, C; V swim1 450-482, east
/// 24-660): the eye springs toward the line from the hint's point to the look-at, `reach` short of the
/// look-at ([`crate::CameraState::fixed`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fixed {
    /// `cam+0x27C` (from `cam+0x288`): the point.
    pub point: [f32; 3],
    /// `cam+0x278`.
    pub reach: f32,
}

impl Fixed {
    /// A hint record's fixed camera: kind 5, its point `+0x18` (`cam+0x718` → `cam+0x272`) in
    /// `points`, its reach `+0x1C` (`cam+0x71C` → `cam+0x278`).
    ///
    /// ponytail: command 0 only (0x2B's two). 5 and 0x11 (Heist's ring wires 0x40-0x43, whose points
    /// the pack lacks) also cut onto the point when it changes, and a command's low nibble keeps the
    /// eye at the point's height: not ported.
    fn parse(r: &[u8; 0x34], points: &[[i16; 3]]) -> Option<Self> {
        let word = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
        let point = points.get(word(0x18) as usize)?;
        (r[0x17] == HINT_FIXED && word(0x20) == 0).then(|| Self {
            point: point.map(f32::from),
            reach: word(0x1C) as f32,
        })
    }
}

impl CameraHint {
    /// Type 0, not kind 0xC (ignored by `func_150A11C4`), not switched off (`+0x14` = 1).
    fn is_hint(r: &[u8; 0x34]) -> bool {
        r[0x15] >> 2 == 0 && r[0x17] != 0xC && r[0x14] != 1
    }

    /// The hints of a trigger table (`[0x0C, scene, 5]`), in table order: type 0, not kind 0xC (ignored
    /// by `func_150A11C4`), not switched off (`+0x14` = 1).
    pub fn parse_table(table: &[u8]) -> Vec<Self> {
        table
            .as_chunks::<0x34>()
            .0
            .iter()
            .filter(|r| Self::is_hint(r))
            .map(|r| Self {
                zone: CameraVolume::parse_record(r),
                on_floor: r[0x16] & 1 != 0,
                kind: r[0x17],
                command: u32::from_be_bytes(r[0x20..0x24].try_into().unwrap()),
            })
            .collect()
    }

    /// `func_150A1DA0` for the player at the start of a frame: at `pos` (`obj+0x14`), last frame at
    /// `old` (`obj+0x2C`), its floor `floor_y` (`obj+0x180`).
    pub fn contains(&self, old: [f32; 3], pos: [f32; 3], floor_y: f32) -> bool {
        if self.on_floor && (pos[1] as i32 as f32) - floor_y > 5.0 {
            return false;
        }
        match self.zone.shape {
            3 => self.zone.crossed(old, pos),
            _ => self.zone.contains(pos),
        }
    }
}

/// `func_150A104C`'s bit 0x40000 for an eye: the last volume containing it decides (action 0 sets
/// it, 6 clears it, 2 leaves it).
pub fn in_volume(volumes: &[CameraVolume], eye: [f32; 3]) -> bool {
    volumes
        .iter()
        .rev()
        .find(|v| v.action != 2 && v.contains(eye))
        .is_some_and(|v| v.action == 0)
}

/// A scene's camera data from its trigger table (`[0x0C, scene, 5]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraLevel {
    /// The camera volumes ([`in_volume`]).
    pub volumes: Vec<CameraVolume>,
    /// Zones where the scene's camera script (`func_15103430`, each tick before the camera steps) sets
    /// `cam+0x84` bit 31 while last tick's view eye is inside one (level command type 4, op 6, arg
    /// 0x9000: `func_150A04D8` → `func_150A2D84` on `cam+0x2F8`): the eye's sweep then skips the
    /// placement objects (C; V: every flag of the camera captures, see conker-sim's tests).
    pub skip_objects: Vec<CameraVolume>,
    /// The camera hints ([`CameraLevel::hint_cut`]).
    pub hints: Vec<CameraHint>,
    /// Not the camera's: the type-1 records of action 1 or 3 (`kind`), player zones that switch
    /// the floor query's same-triangle fast path off or on ([`CameraLevel::full_floor`]).
    pub floor_zones: Vec<CameraHint>,
    /// One per hint, in [`hints`](Self::hints)' order: its fixed camera, if any (kind 5, [`Fixed`]).
    pub fixed: Vec<Option<Fixed>>,
}

impl CameraLevel {
    /// The zones of scene 0x2D's script: triggers 0x58 and 0x59, around the two raised bases (its other
    /// branch, triggers 0x53/0x54/0x56/0x57 for bit 29, is not ported: no trace enters them).
    pub fn new(scene: u16, table: &[u8]) -> Self {
        let records = table.as_chunks::<0x34>().0;
        let zones: &[usize] = if scene == 0x2D { &[0x58, 0x59] } else { &[] };
        let points: &[[i16; 3]] = if scene == 0x2B { &SEA_POINTS } else { &[] };
        Self {
            volumes: CameraVolume::parse_table(table),
            skip_objects: zones
                .iter()
                .filter_map(|&i| records.get(i).map(CameraVolume::parse_record))
                .collect(),
            hints: CameraHint::parse_table(table),
            floor_zones: records
                .iter()
                .filter(|r| r[0x15] >> 2 == 1 && matches!(r[0x17], 1 | 3) && r[0x14] == 0)
                .map(|r| CameraHint {
                    zone: CameraVolume::parse_record(r),
                    on_floor: r[0x16] & 1 != 0,
                    kind: r[0x17],
                    command: 0,
                })
                .collect(),
            fixed: (records.iter().filter(|r| CameraHint::is_hint(r)))
                .map(|r| Fixed::parse(r, points))
                .collect(),
        }
    }

    /// Whether the player's floor is searched in full this frame (obj+0xF8 bit 2 clear), at the
    /// frame's start at `pos` over `floor_y`: the last floor zone containing it decides, action 1
    /// yes, action 3 no; outside them the multiplayer scenes set the bit (`func_150829D8`), so no.
    /// The player's trigger loop (`func_150A11C4`) runs it; the bit gates the fast path
    /// (`func_150A3FC4`) in `func_1504CB98` and `func_15058898` (C; V z30, z34: 18,688 and 18,704
    /// rows, 1 and 39 off, the latter `s126+0x56` ≠ 0, I).
    pub fn full_floor(&self, pos: [f32; 3], floor_y: f32) -> bool {
        (self.floor_zones.iter().rev())
            .find(|z| z.contains(pos, pos, floor_y))
            .is_some_and(|z| z.kind == 1)
    }

    /// Whether the hints cut the camera this frame ([`crate::CameraState::hint_cut`]): the last hint
    /// the player is in has a cut command (I: the hint bookkeeping that can veto it, `cam+0x6D2` and
    /// `+0x6D4`, stays clear in Deathmatch). `old`, `pos` and `floor_y` as for
    /// [`CameraHint::contains`]: the player's position before last frame's move and after it.
    pub fn hint_cut(&self, old: [f32; 3], pos: [f32; 3], floor_y: f32) -> bool {
        self.hints
            .iter()
            .rev()
            .find(|h| h.contains(old, pos, floor_y))
            .is_some_and(|h| h.kind != 0xA && h.command == HINT_CUT)
    }

    /// The fixed camera of the last hint the player is in (`func_15128CB0` keeps the last one
    /// `func_150A11C4` queued, in table order), if it has one ([`Fixed`]; arguments as for
    /// [`hint_cut`](Self::hint_cut)).
    pub fn fixed(&self, old: [f32; 3], pos: [f32; 3], floor_y: f32) -> Option<Fixed> {
        let i = (self.hints.iter()).rposition(|h| h.contains(old, pos, floor_y))?;
        *self.fixed.get(i)?
    }

    pub fn in_volume(&self, eye: [f32; 3]) -> bool {
        in_volume(&self.volumes, eye)
    }

    pub fn skips_objects(&self, eye: [f32; 3]) -> bool {
        self.skip_objects.iter().any(|z| z.contains(eye))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(center: [i16; 3], size: [i16; 3], yaw: f32, action: u8) -> [u8; 0x34] {
        shaped(center, size, yaw, action, 6)
    }

    fn shaped(center: [i16; 3], size: [i16; 3], yaw: f32, action: u8, kind: u8) -> [u8; 0x34] {
        let mut r = [0u8; 0x34];
        for (k, v) in center.iter().chain(&size).enumerate() {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        r[0x10..0x14].copy_from_slice(&yaw.to_be_bytes());
        (r[0x15], r[0x17]) = (kind, action);
        r
    }

    /// A type-0 trip wire (shape 3) at the origin, unturned, with a kind and a command.
    fn wire(kind: u8, command: u32) -> [u8; 0x34] {
        let mut r = shaped([0, 0, 0], [100, 50, 20], 0.0, kind, 3);
        r[0x20..0x24].copy_from_slice(&command.to_be_bytes());
        r
    }

    #[test]
    fn a_trip_wire_fires_once_crossed_forward_within_its_bounds() {
        let h = CameraHint::parse_table(&wire(4, HINT_CUT))[0];
        assert_eq!((h.kind, h.command, h.zone.shape), (4, HINT_CUT, 3));
        let crossed = |old: [f32; 3], new: [f32; 3]| h.contains(old, new, 0.0);
        assert!(crossed([0.0, 10.0, -10.0], [0.0, 10.0, 10.0]));
        assert!(crossed([0.0, 10.0, -1.0], [0.0, 10.0, 0.0])); // landing on the plane counts
        assert!(!crossed([0.0, 10.0, 10.0], [0.0, 10.0, -10.0])); // backward
        assert!(!crossed([0.0, 10.0, 5.0], [0.0, 10.0, 10.0])); // already past
        // Where it crosses: x within the half-width, y in [0, height).
        assert!(crossed([-190.0, 10.0, -10.0], [190.0, 10.0, 10.0]));
        assert!(!crossed([90.0, 10.0, -10.0], [110.0, 10.0, 10.0]));
        assert!(crossed([0.0, -10.0, -10.0], [0.0, 10.0, 10.0]));
        assert!(!crossed([0.0, 50.0, -10.0], [0.0, 50.0, 10.0]));
        assert!(!crossed([0.0, -65_526.0, -10.0], [0.0, -65_526.0, 10.0]));
    }

    #[test]
    fn the_last_hint_the_player_is_in_decides_the_cut() {
        let level = |table: Vec<u8>| CameraLevel::new(0x2D, &table);
        let (old, new) = ([0.0, 10.0, -10.0], [0.0, 10.0, 10.0]);
        assert!(level(wire(4, HINT_CUT).to_vec()).hint_cut(old, new, 0.0));
        assert!(!level(wire(4, 0).to_vec()).hint_cut(old, new, 0.0));
        assert!(!level(wire(0xA, HINT_CUT).to_vec()).hint_cut(old, new, 0.0));
        assert!(!level([wire(4, HINT_CUT), wire(4, 0)].concat()).hint_cut(old, new, 0.0));
        assert!(level([wire(4, 0), wire(4, HINT_CUT)].concat()).hint_cut(old, new, 0.0));
        // Kind 0xC and switched-off records are not hints; nor are the camera volumes (type 1).
        let mut off = wire(4, HINT_CUT);
        off[0x14] = 1;
        assert!(CameraHint::parse_table(&[wire(0xC, HINT_CUT), off].concat()).is_empty());
        assert!(CameraHint::parse_table(&record([0, 0, 0], [100, 50, 20], 0.0, 0)).is_empty());
    }

    #[test]
    fn the_last_hint_the_player_is_in_decides_the_fixed_camera() {
        let fixed = |point: u32, command: u32| {
            let mut r = wire(HINT_FIXED, command);
            r[0x18..0x1C].copy_from_slice(&point.to_be_bytes());
            r[0x1C..0x20].copy_from_slice(&707u32.to_be_bytes());
            r
        };
        let (old, new) = ([0.0, 10.0, -10.0], [0.0, 10.0, 10.0]);
        let sea = |table: Vec<u8>| CameraLevel::new(0x2B, &table).fixed(old, new, 0.0);
        let west = Fixed {
            point: [-2290.0, 728.0, 10913.0],
            reach: 707.0,
        };
        assert_eq!(sea(fixed(1, 0).to_vec()), Some(west));
        assert_eq!(sea(fixed(2, 0).to_vec()), None); // no such point
        assert_eq!(sea(fixed(1, 5).to_vec()), None); // command 5: not ported
        assert_eq!(sea([fixed(1, 0), wire(4, 0)].concat()), None);
        assert_eq!(sea([wire(4, 0), fixed(1, 0)].concat()), Some(west));
        let elsewhere = CameraLevel::new(0x2D, &fixed(0, 0));
        assert_eq!(elsewhere.fixed(old, new, 0.0), None);
    }

    #[test]
    fn box_bounds_are_strict_and_turn_with_the_yaw() {
        let v = CameraVolume::parse_table(&record([0, 0, 0], [100, 50, 20], 0.0, 0))[0];
        assert!(v.contains([99.9, 1.0, 0.0]));
        assert!(!v.contains([100.0, 1.0, 0.0]) && !v.contains([0.0, 0.5, 0.0]));
        assert!(!v.contains([0.0, 50.0, 0.0]) && !v.contains([0.0, 10.0, 20.0]));
        // An offset past s16 does not wrap back in: −65,526 truncates to 10.
        assert!(!v.contains([0.0, -65_526.0, 0.0]) && v.contains([0.0, 10.0, 0.0]));
        // Turned 90 degrees: the long side now runs along z.
        let v = CameraVolume::parse_table(&record([0, 0, 0], [100, 50, 20], 90.0, 0))[0];
        assert!(v.contains([0.0, 10.0, 90.0]) && !v.contains([90.0, 10.0, 0.0]));
    }

    #[test]
    fn cylinders_use_the_octagonal_distance() {
        let v = CameraVolume::parse_table(&shaped([0, 0, 0], [100, 50, 0], 0.0, 0, 4))[0];
        assert_eq!(v.shape, 0);
        assert!(v.contains([99.0, 1.0, 0.0]) && !v.contains([100.0, 1.0, 0.0]));
        // 60 + 60 - 30 = 90 < 100, though the true distance is 84.9; 70 + 70 - 35 = 105.
        assert!(v.contains([60.0, 1.0, 60.0]) && !v.contains([70.0, 1.0, 70.0]));
        assert!(!v.contains([0.0, 50.0, 0.0]) && !v.contains([0.0, -1.0, 0.0]));
    }

    #[test]
    fn the_last_containing_volume_decides() {
        let on = record([0, 0, 0], [100, 50, 100], 0.0, 0);
        let off = record([50, 0, 0], [20, 50, 20], 0.0, 6);
        let other = {
            let mut r = on;
            r[0x15] = 2;
            r
        }; // type 0: not a camera volume
        let table: Vec<u8> = [on, off, other].concat();
        let vols = CameraVolume::parse_table(&table);
        assert_eq!(vols.len(), 2);
        assert!(in_volume(&vols, [0.0, 10.0, 0.0]));
        assert!(!in_volume(&vols, [50.0, 10.0, 0.0]));
        assert!(!in_volume(&vols, [500.0, 10.0, 0.0]));
    }
}
