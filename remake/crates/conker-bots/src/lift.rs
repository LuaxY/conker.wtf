//! The level's lifts (C `func_150A11C4` case 8, V on the oracle's bot matches): trigger records of
//! type 8 (`+0x15 >> 2`) in `[0x0C, scene, 5]` send a player standing inside them to spawn record
//! `+0x18` of scene `+0x17`, through the placement path (`func_15172E7C` → `func_15172D28`, one
//! RNG draw of jitter, the record's facing). Bots go at once; a human's screen fades first (I: the
//! move then waits for the fade, `0x800DD2B0`; not timed here, so humans go at once too).
//!
//! The navigation graph links the bottom and top of each lift (nodes 105-108 and 106-107 on 0x2D),
//! so bots walk into them as part of a path.
//!
//! ponytail: a level rule, kept here because the bots need it and conker-rules lacks it; it belongs
//! in `Match::step` (each player's update tests the triggers before it moves).

use conker_camera::volume::CameraVolume;
use conker_rules::game::{Arena, Match};
use conker_rules::spawn::{self, Placement};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lift {
    pub zone: CameraVolume,
    /// `+0x15 & 3`: 3 (every lift of 0x2D) is a gate the player's last move crosses.
    pub shape: u8,
    /// `+0x16 & 1`: only while standing (feet within 5 of the floor).
    pub floor: bool,
    /// `+0x17`, `+0x18`: where it goes.
    pub scene: u8,
    pub record: usize,
}

/// The type-8 records of a trigger table that lead within `scene`, minus disabled ones (`+0x14`).
pub fn parse(triggers: &[u8], scene: u8) -> Vec<Lift> {
    triggers
        .as_chunks::<0x34>()
        .0
        .iter()
        .filter(|r| r[0x15] >> 2 == 8 && r[0x14] != 1 && r[0x17] == scene)
        .map(|r| Lift {
            zone: CameraVolume::parse_record(r),
            shape: r[0x15] & 3,
            floor: r[0x16] & 1 != 0,
            scene: r[0x17],
            record: u32::from_be_bytes([r[0x18], r[0x19], r[0x1A], r[0x1B]]) as usize,
        })
        .collect()
}

impl Lift {
    /// `func_150A1DA0` for a player whose last move went from `old` to `pos`, its floor at `floor_y`.
    pub fn takes(&self, old: [f32; 3], pos: [f32; 3], floor_y: f32) -> bool {
        (!self.floor || pos[1] as i32 as f32 - floor_y <= 5.0)
            && if self.shape == 3 {
                self.gate(old, pos)
            } else {
                self.zone.contains(pos)
            }
    }

    /// Shape 3: the move crosses the box's front plane (its z = 0, turned by the yaw then the
    /// pitch) from behind, within the half-width and between 0 and the height. Offsets from the
    /// centre are truncated to s16 as the game does.
    fn gate(&self, old: [f32; 3], pos: [f32; 3]) -> bool {
        let c = self.zone.center.map(f32::from);
        let s16 = |v: f32| v as i32 as i16 as f32;
        let [dx, dy, dz] = std::array::from_fn(|k| s16(pos[k] - c[k]));
        let [ox, oy, oz] = std::array::from_fn(|k| s16(old[k] - c[k]));
        let ([ps, pc], [ys, yc]) = (self.zone.pitch, self.zone.yaw);
        let [sx, sy, _] = self.zone.size.map(f32::from);
        let (zr1, zr0) = (dz * yc + dx * ys, oz * yc + ox * ys);
        let (z1, z0) = (s16(zr1 * pc - dy * ps), s16(zr0 * pc - oy * ps));
        if !(z0 < 0.0 && z1 >= 0.0) {
            return false;
        }
        let t = z0 / (z0 - z1);
        let (x1, x0) = (s16(dx * yc - dz * ys), s16(ox * yc - oz * ys));
        if (x0 + t * (x1 - x0)).abs() >= sx {
            return false;
        }
        let (y1, y0) = (s16(dy * pc + zr1 * ps), s16(oy * pc + zr0 * ps));
        let y = y0 + t * (y1 - y0);
        (0.0..sy).contains(&y)
    }
}

/// The level's lifts and every player's position before the last [`Match::step`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lifts {
    pub lifts: Vec<Lift>,
    old: Vec<[f32; 3]>,
}

impl Lifts {
    pub fn new(triggers: &[u8], scene: u8) -> Self {
        Self {
            lifts: parse(triggers, scene),
            old: Vec::new(),
        }
    }

    /// Before [`Match::step`] (the game tests the triggers at the start of each player's update):
    /// every living player whose last move took it into a lift goes onto the lift's record.
    /// Returns those moved, with where they went.
    pub fn ride(&mut self, m: &mut Match, arena: &Arena) -> Vec<(usize, Placement)> {
        let mut moved = Vec::new();
        for p in 0..m.players.len() {
            let f = &m.players[p];
            let old = self.old.get(p).copied().unwrap_or(f.body.pos);
            let lift = self
                .lifts
                .iter()
                .find(|l| l.takes(old, f.body.pos, f.body.floor_y));
            if let Some(l) = lift.filter(|_| f.health != 0) {
                let place = spawn::place(&arena.spawns, l.record, &mut m.rng);
                let b = &mut m.players[p].body;
                b.pos = place.pos;
                b.floor_y = place.pos[1];
                (b.facing, b.move_angle) = (place.facing, place.facing);
                moved.push((p, place));
            }
        }
        self.old = m.players.iter().map(|f| f.body.pos).collect();
        moved
    }

    /// Player `i` left the match (the ones after it move down a slot).
    pub fn remove(&mut self, i: usize) {
        if i < self.old.len() {
            self.old.remove(i);
        }
    }
}
