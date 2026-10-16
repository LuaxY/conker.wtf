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
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
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

/// What changes of a match's plates and traps, per plate: a plate's traps switch on together
/// ([`Traps::step`]), so they share one clock and one owner. A client predicts it
/// (`net::Fighting::traps`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct State {
    pub plates: [Plate; 4],
    pub left: [u16; 4],
    pub owner: [Option<usize>; 4],
}

impl State {
    /// The plates' y below rest ([`seat`]).
    pub fn offsets(&self) -> [i8; 4] {
        self.plates.map(|p| p.dy as i8)
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
        self.state().offsets()
    }

    /// The plates, and each plate's traps' clock and owner (its first trap's).
    pub fn state(&self) -> State {
        let first = |k| self.list.iter().position(|t| t.plate == k);
        State {
            plates: self.plates,
            left: std::array::from_fn(|k| first(k).map_or(0, |j| self.left[j])),
            owner: std::array::from_fn(|k| first(k).and_then(|j| self.owner[j])),
        }
    }

    /// Puts `s` back: the plates, and every trap its plate's clock and owner.
    pub fn set_state(&mut self, s: State) {
        self.plates = s.plates;
        for (j, t) in self.list.iter().enumerate() {
            (self.left[j], self.owner[j]) = (s.left[t.plate], s.owner[t.plate]);
        }
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
            "/../../lab/research/mp-original/dm-maps-results-scratch/out/box34/mem.bin"
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

    /// A client's frame (`Match::step_player`) runs the traps as the whole match does: the plate
    /// sinks under its player and carries it, its jets kill it, a cone burns it, and the plates'
    /// state (`Traps::state`, what `Fighting` carries) is the match's, frame for frame.
    #[test]
    fn a_players_own_frame_rides_plates_and_takes_traps() {
        use crate::spawn::Placement;
        use crate::{Arena, Event, Fighter, Match, Rng};
        use conker_sim::collision::{CollisionWorld, Layer};
        use conker_sim::movement::Input;
        let square = |x0: f32, x1: f32, z0: f32, z1: f32, y: f32| {
            [
                [[x0, y, z0], [x0, y, z1], [x1, y, z1]],
                [[x0, y, z0], [x1, y, z1], [x1, y, z0]],
            ]
        };
        // Ground tiles: a floor's triangle is kept while the body stays over it, so the plate and the
        // cone are holes in the ground, as in the level.
        let inside = |x, z, r: [i32; 4]| (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&z);
        let (plate, cone) = ([-100, 100, -400, -200], [500, 700, -300, -200]);
        let tile =
            |[x0, x1, z0, z1]: [i32; 4], y| square(x0 as f32, x1 as f32, z0 as f32, z1 as f32, y);
        let ground = (-3..9).flat_map(|i| (-40..4).map(move |k| (100 * i, 100 * k)));
        let ground = ground.filter(|&(x, z)| !inside(x, z, plate) && !inside(x, z, cone));
        let tiles = (ground.flat_map(|(x, z)| tile([x, x + 100, z, z + 100], 0.0)))
            .map(|t| (t, 5, Layer::Terrain))
            .chain(tile(cone, 5.0).map(|t| (t, 0xE, Layer::Terrain)))
            .chain(tile(plate, 10.0).map(|t| (t, 5, Layer::Placement(6))));
        let mut w = CollisionWorld::layered(tiles);
        // A fire jet of plate 6 (id 0) at z −800: a cylinder of radius 68 from 2 below the floor.
        let mut r = [0u8; 0x34];
        for (k, v) in [0i16, -2, -800, 68, 300, 237].into_iter().enumerate() {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        (r[0x15], r[0x16], r[0x17], r[0x1B]) = (12 << 2, 1, 9, 4);
        let arena = Arena::default();
        // Player 1 runs −z from `x` (camera yaw 0, facing and stick up), player 0 idles far away.
        let run = |x: f32, w: &mut CollisionWorld| {
            let at = |x: f32| Placement {
                index: 0,
                pos: [x, 0.0, 0.0],
                facing: 0x4000,
                camera: [0.0; 3],
            };
            let mut players = vec![Fighter::new(&at(-250.0), 0, 0), Fighter::new(&at(x), 1, 0)];
            players.iter_mut().for_each(|p| p.immune = 0);
            let mut m = Match::new(&arena, players, Rng::new(1), 0);
            m.traps = Traps::new(SCENE, &r);
            let input = Input {
                stick_y: 80,
                ..Input::default()
            };
            let (mut pressed, mut hits) = (false, 0);
            for _ in 0..100 {
                let t = m.tick;
                seat(w, m.traps.offsets());
                let mut solo = m.clone();
                let events = m.step(&arena, &[Input::default(), input], w);
                let predicted = solo.step_player(1, &arena, &input, w, &|_, _| None);
                assert_eq!(solo.players[1], m.players[1], "frame {t}");
                assert_eq!(solo.traps.state(), m.traps.state(), "frame {t}");
                let hit =
                    |ev: &[Event]| (ev.iter()).any(|e| matches!(e, Event::Hit { victim: 1, .. }));
                assert_eq!(hit(&predicted), hit(&events), "frame {t}");
                hits += usize::from(hit(&events));
                pressed |=
                    m.traps.plates[0].dy == -DEPTH && m.players[1].body.pos[1] == 10.0 - 10.0;
            }
            (pressed, hits, m.players[1])
        };
        let (pressed, hits, f) = run(0.0, &mut w);
        assert!(pressed && hits == 1 && f.health == 0, "{f:?}");
        let (_, hits, f) = run(600.0, &mut w);
        assert!(hits >= 1 && f.burn.is_some() || hits >= 2, "{f:?}");
    }
}
