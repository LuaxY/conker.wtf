//! Scene 0x34's pressure plates and traps (research `dm-maps-results.md` §2a-2b).
//!
//! Props 6-9 (placements 6-9) are pressure plates (`func_151177C0`, contact `func_15117D3C`): stood
//! on they sink 6 then 4 to 10 below rest, hold while stood on, and rise 3 a frame once left. On
//! reaching the bottom (`func_15104A80`) plates 6-8 switch on their fire jets (message 0x35) and
//! plate 9 the electric floor (message 0x38), each only if off, owned by the lowest slot on the
//! plate. The traps are the trigger table's kind-12 records (action 9): 27 fire cylinders (arg 4,
//! their id in word 0x1C) and the two boxes of the electric floor (arg 8). Every frame a player
//! inside one that is on takes its hit, credited to the owner: dmg 7, more than the 6 of health,
//! so one touch kills (V trap34, elec34).

use conker_camera::volume::{CameraHint, CameraVolume};

/// The scene with plates and traps.
pub const SCENE: u16 = 0x34;
/// The plates' placements (props 6-9, +0x72 ids 0xF9-0xF6) and their rest y (V box34 RAM).
pub const PLATES: [u16; 4] = [6, 7, 8, 9];
pub const REST: [i16; 4] = [-385, -766, -36, -32];
/// A plate's depth at the bottom, its first frames' sink and its rise (`func_151177C0` on scene
/// 0x34: a1 = 10, t1 = 6, +3).
pub const DEPTH: i16 = 10;
pub const SINK: i16 = 6;
pub const RISE: i16 = 3;
/// A player stands on a plate for its contact when its floor is the plate's and it is at most this
/// far above it (I: V box34, landings set it 3.9 and 1.9 above, not 23.5 and 25.5).
pub const CONTACT: f32 = 5.0;
/// The fire jets each of plates 6-8 switches on (the 9-byte lists at `0x800A2380`/`238C`/`2398`:
/// the records' word 0x1C).
pub const FIRES: [&[u8]; 3] = [
    &[0, 1, 2, 3, 4, 5, 0xF, 0x10, 0x11],
    &[6, 7, 8, 0x18, 0x19, 0x1A, 0x12, 0x13, 0x14],
    &[9, 0xA, 0xB, 0xC, 0xD, 0xE, 0x15, 0x16, 0x17],
];
/// Frames a trap hits once on: a fire jet's emitter lives 0xF0 VIs (`func_151A6068`; V trap34:
/// frames F+3 to F+122 for a press first stood on in frame F); the electric floor's timer is 0x12C
/// VIs and goes off below 0 (`func_151050B0`), 151 frames, switched on after the players of the
/// frame its plate bottoms (V elec34: first hit F+4, last F+154).
pub const FIRE_FRAMES: u16 = 120;
pub const SPARK_FRAMES: u16 = 151;

/// One trap: a kind-12 record of the trigger table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trap {
    /// Its index in the trigger table.
    pub record: usize,
    /// The record's zone (shape 0 cylinder, shape 2 box) and its `+0x16` floor test.
    pub zone: CameraHint,
    /// The electric floor (arg 8), else a fire jet (arg 4).
    pub electric: bool,
    /// Word 0x1C: a fire jet's id in [`FIRES`] (0-26).
    pub id: u8,
    /// The plate (0-3, props 6-9) that switches it on.
    pub plate: usize,
    /// The record's command word (`+0x20`): 1 on the middle jet of each row of three (I: the
    /// one whose emitter plays sound 0x1AA).
    pub command: u32,
}

impl Trap {
    /// A player at `pos` over `floor_y` is inside.
    pub fn contains(&self, pos: [f32; 3], floor_y: f32) -> bool {
        self.zone.contains(pos, pos, floor_y)
    }
}

/// The traps of a trigger table (`[0x0C, scene, 5]`, the pack's `triggers`), in table order: the
/// order of [`Traps::on`]'s bits and the client's.
pub fn parse(table: &[u8]) -> Vec<Trap> {
    let records = table.as_chunks::<0x34>().0;
    (records.iter().enumerate())
        .filter(|(_, r)| r[0x15] >> 2 == 12 && r[0x17] == 9)
        .filter_map(|(record, r)| {
            let word = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
            let (electric, id) = (word(0x18) == 8, word(0x1C) as u8);
            let plate = match electric {
                true => 3,
                false => FIRES.iter().position(|l| l.contains(&id))?,
            };
            Some(Trap {
                record,
                zone: CameraHint {
                    zone: CameraVolume::parse_record(r),
                    on_floor: r[0x16] & 1 != 0,
                    kind: r[0x17],
                    command: 0,
                },
                electric,
                id,
                plate,
                command: word(0x20),
            })
        })
        .collect()
}

/// A pressure plate's state (+0x73 & 3) and its y below rest (+0x12 − rest).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Plate {
    /// 0 up, 2 sinking, 3 down, 1 rising.
    pub state: u8,
    pub dy: i16,
}

impl Plate {
    /// One frame: the update (`func_151177C0`), then the contact routine (`func_15117D3C`), both
    /// with `stood`, the contact flag (+0x4F bit 4) last frame's floor queries set. True on the
    /// frame it reaches the bottom (its callback, `func_15104A80`).
    pub fn step(&mut self, stood: bool) -> bool {
        let mut bottom = false;
        match self.state {
            2 => {
                self.dy -= SINK;
                if self.dy <= -DEPTH {
                    (self.dy, self.state, bottom) = (-DEPTH, 3, true);
                }
            }
            3 => {
                self.dy = -DEPTH;
                if !stood {
                    self.state = 1;
                }
            }
            1 => {
                self.dy += RISE;
                if self.dy >= 0 {
                    (self.dy, self.state) = (0, 0);
                }
            }
            _ => self.dy = 0,
        }
        if self.state == 0 && stood {
            self.state = 2;
        }
        bottom
    }
}

/// The plates and traps of a match (empty off scene 0x34).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traps {
    pub list: Vec<Trap>,
    pub plates: [Plate; 4],
    /// Per trap, frames left: a fire jet hits while above 0; the electric floor while 1 to
    /// [`SPARK_FRAMES`] (it starts one higher: on after the players of its first frame).
    pub left: Vec<u16>,
    /// Per trap, the slot its hits are credited to (the plate's lowest slot when it switched on).
    pub owner: Vec<Option<usize>>,
}

impl Traps {
    /// The traps of `scene` from its trigger table.
    pub fn new(scene: u16, table: &[u8]) -> Self {
        let list = if scene == SCENE {
            parse(table)
        } else {
            Vec::new()
        };
        Self {
            left: vec![0; list.len()],
            owner: vec![None; list.len()],
            list,
            plates: [Plate::default(); 4],
        }
    }

    /// Trap `k` hits this frame.
    pub fn hits(&self, k: usize) -> bool {
        match self.list[k].electric {
            true => (1..=SPARK_FRAMES).contains(&self.left[k]),
            false => self.left[k] > 0,
        }
    }

    /// Bit k: trap k is on ([`Self::hits`]), for the clients (`MatchView::traps`).
    pub fn on(&self) -> u32 {
        (0..self.list.len().min(32))
            .filter(|&k| self.hits(k))
            .fold(0, |acc, k| acc | 1 << k)
    }

    /// The plates' y below rest, for the clients (`MatchView::plates`) and [`seat`].
    pub fn offsets(&self) -> [i8; 4] {
        self.plates.map(|p| p.dy as i8)
    }

    /// The frame's end: the traps' clocks run down, then each plate steps with `stood[k]`, the
    /// lowest slot standing on it this frame; a plate reaching the bottom switches its traps on.
    /// Returns each plate's y change (its riders go with it).
    pub fn step(&mut self, stood: [Option<usize>; 4]) -> [i16; 4] {
        for l in &mut self.left {
            *l = l.saturating_sub(1);
        }
        let mut moved = [0; 4];
        for (k, plate) in self.plates.iter_mut().enumerate() {
            let dy = plate.dy;
            if !plate.step(stood[k].is_some()) {
                moved[k] = plate.dy - dy;
                continue;
            }
            moved[k] = plate.dy - dy;
            for (j, t) in self.list.iter().enumerate() {
                if t.plate == k && self.left[j] == 0 {
                    self.left[j] = if t.electric {
                        SPARK_FRAMES + 1
                    } else {
                        FIRE_FRAMES
                    };
                    self.owner[j] = stood[k];
                }
            }
        }
        moved
    }
}

/// Moves the plates' collision (placements 6-9) to `offsets` below rest (the props carry their
/// models' collision): call before each frame's moves on scene 0x34.
pub fn seat(world: &mut conker_sim::collision::CollisionWorld, offsets: [i8; 4]) {
    for (n, dy) in PLATES.into_iter().zip(offsets) {
        world.shift(n, i32::from(dy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Box34 (V, `dm-maps-results-scratch/out/box34/mem.bin`): props 6-9's y, state and contact
    /// flag every read of a 30 s match with 15 bots. The flag a read shows is what the next
    /// frame's update reads; the plate after it is the next read's.
    #[test]
    fn the_plates_replay_box34() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../lab/research/mp-original/dm-maps-results-scratch/out/box34/mem.bin"
        );
        let Ok(mem) = std::fs::read(path) else {
            return;
        };
        // Per read: 4 + 4 × (+0x10: 8, +0x70: 4, +0x4C: 4), big-endian RAM.
        let rows: Vec<(u32, [(i16, u8, bool); 4])> = mem
            .chunks_exact(68)
            .map(|r| {
                let read = u32::from_le_bytes(r[..4].try_into().unwrap());
                let plate = |k: usize| {
                    let b = &r[4 + 16 * k..20 + 16 * k];
                    (i16::from_be_bytes([b[2], b[3]]), b[11] & 3, b[15] & 4 != 0)
                };
                (read, [0, 1, 2, 3].map(plate))
            })
            .filter(|r| r.0 >= 24)
            .collect();
        let mut plates = [Plate::default(); 4];
        let (mut checked, mut presses) = (0, 0);
        for pair in rows.windows(2) {
            let ((was, now), (read, next)) = (pair[0], pair[1]);
            assert_eq!(was + 1, read);
            for k in 0..4 {
                presses += u32::from(plates[k].step(now[k].2));
                let want = (next[k].0 - REST[k], next[k].1);
                assert_eq!(
                    (plates[k].dy, plates[k].state),
                    want,
                    "plate {k} at read {read}"
                );
                checked += 1;
            }
        }
        // 310 reads; plate 6 bottoms at reads 59, 101; 7 at 88, 129; 9 at 28 (and none of 8).
        assert!(checked == 4 * 310 && presses == 5, "{checked} {presses}");
    }

    /// Sink 6 then 4, hold, rise 3 a frame (V box34 56-72), and the traps a plate switches on.
    #[test]
    fn a_plate_fires_its_traps() {
        let mut p = Plate::default();
        let steps: Vec<(i16, u8, bool)> =
            [true, true, true, true, false, false, false, false, false]
                .map(|s| {
                    let b = p.step(s);
                    (p.dy, p.state, b)
                })
                .to_vec();
        assert_eq!(
            steps,
            [
                (0, 2, false),
                (-6, 2, false),
                (-10, 3, true),
                (-10, 3, false),
                (-10, 1, false),
                (-7, 1, false),
                (-4, 1, false),
                (-1, 1, false),
                (0, 0, false)
            ]
        );
    }
}
