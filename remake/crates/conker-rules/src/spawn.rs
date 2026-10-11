//! Multiplayer spawn points: which record each player gets, the jitter, and the respawn choice.
//!
//! The picker is `func_15084D70` (C; `func_150ADA68`, which earlier notes took for the picker, is the
//! RNG's `random_float`). Two paths reach it in a match (C, V on the oracle: `tests/oracle.rs` and
//! `lab/oracle/spawn_README.md`):
//! - **Placement** (flag `0x800D18A8` clear): one RNG draw moves the record's point along its
//!   facing ([`place`]). Three callers use it:
//!   - the scene load (`func_150054C4`), once per human, always on record 0, before the match;
//!   - the level script at match start (`func_15097A8C` → `func_15172D28`), once per participant in
//!     slot order, on the record `func_15085410` stored in struct126+0x11B ([`DEATHMATCH_START`],
//!     [`COLORS_START`]);
//!   - `func_15172E7C` during the match, which moves a player onto records 2-5, the raised and
//!     lowered pairs near (-1100, 3700) and (800, -3900) (I: lifts or teleports; trigger untraced).
//! - **Respawn after a death** (`func_1507D1D8` sets the flag): every eligible record but the
//!   player's previous one gets a score and the best wins, with no jitter ([`respawn`]).
//!
//! So at match start the RNG takes one draw per human, then one per participant.
//!
//! Record bytes (0x18 per record, `[0x0C, scene, 6]`): +0 position s16×3, +6 [`Spawn::b6`],
//! +7 yaw (facing = yaw << 8), +8 camera eye f32×3, +0x14 and +0x15 (see the fields), +0x16 a
//! redirect index that the picker follows only when its third argument is set, which multiplayer
//! clears before use (C).

use conker_camera::math::{cos, sin};

use crate::Rng;

/// One spawn record, field for field as `extract_scene::SpawnPoint` (position, byte_06, yaw,
/// camera, byte_14, byte_15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spawn {
    pub pos: [i16; 3],
    /// +6, an angle in 1/256 turns: `func_15085430` stores it in obj+0xB8 as degrees, see
    /// [`b6_degrees`] (C; what obj+0xB8 drives is untraced). 0, 1, 6, -1, -3 or -8 on 0x2D.
    pub b6: u8,
    /// +7: facing in 1/256 turns.
    pub yaw: u8,
    /// +8: the camera eye at (re)spawn.
    pub camera: [f32; 3],
    /// +0x14: copied to an output that `func_15085430` never reads (C). 0 on every 0x2D record.
    pub b14: u8,
    /// +0x15: stored in struct126+0x23 (C, meaning untraced). 0 on every 0x2D record.
    pub b15: i8,
}

/// Where a player (re)appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub index: usize,
    /// obj+0x14/18/1C (and the old position +0x2C): the integer point as f32. y is the record's;
    /// the floor query settles the player on the next frame.
    pub pos: [f32; 3],
    /// obj+0x7A facing = +0x76 move angle = +0x78.
    pub facing: u16,
    pub camera: [f32; 3],
}

impl Spawn {
    fn placement(&self, index: usize, pos: [i16; 3]) -> Placement {
        Placement {
            index,
            pos: pos.map(f32::from),
            facing: u16::from(self.yaw) << 8,
            camera: self.camera,
        }
    }
}

/// Scene 0x2D Deathmatch (scenario 3): the record each participant slot (gObjects index: humans
/// first, then bots) is placed on at match start. V on the oracle for slots 0-15 (16 is the
/// game's maximum), in every layout tried (1-2 humans, 0-15 bots). The script alternates the
/// north records (1, 13-19) and the south ones (0, 6-12).
pub const DEATHMATCH_START: [u8; 16] = [1, 0, 13, 6, 14, 7, 15, 8, 16, 9, 17, 10, 18, 11, 19, 12];

/// Scene 0x2D War: Colors (scenario 9): the k-th member of team 0 starts on `COLORS_START[0][k]`
/// (the south records), of team 1 on `COLORS_START[1][k]` (north). V for k = 0, 1 (2 humans + 2
/// bots); the rest I, by analogy with Deathmatch's lists.
pub const COLORS_START: [[u8; 8]; 2] =
    [[0, 6, 7, 8, 9, 10, 11, 12], [1, 13, 14, 15, 16, 17, 18, 19]];

/// The `eligible` masks of [`respawn`]: Deathmatch keeps all records (V), Colors' level script
/// clears each team's opposite side, leaving both teams records 2-5 (V on the oracle, set before
/// the first respawn; when exactly is I). Index by team (0, or 1 for any other team).
pub const DEATHMATCH_MASKS: [u32; 2] = [!0, !0];
pub const COLORS_MASKS: [u32; 2] = [0xFFF0_1FFD, 0xFFFF_E03E];

/// The jitter radius: the point moves by `r = f * 80 - 40` along the record's facing.
pub const JITTER: f32 = 40.0;

/// The placement path: the record moved by one RNG draw along its facing (`x += sin(a)·r`,
/// `z += cos(a)·r`, truncated to integers), op for op as `func_15084D70` does it. Like the game,
/// the index keeps its low 12 bits and falls back to record 0 when out of range.
pub fn place(spawns: &[Spawn], index: usize, rng: &mut Rng) -> Placement {
    let index = if index & 0xFFF < spawns.len() {
        index & 0xFFF
    } else {
        0
    };
    let s = &spawns[index];
    let angle = (f32::from(s.yaw) * f32::from_bits(0x4049_0FDB)) * (1.0 / 128.0);
    let r = rng.next_f32() * 80.0 - JITTER;
    let [x, y, z] = s.pos;
    let x = (f32::from(x) + sin(angle) * r) as i32 as i16;
    let z = (f32::from(z) + cos(angle) * r) as i32 as i16;
    s.placement(index, [x, y, z])
}

/// The respawn path. Candidates are the records whose bit is set in `eligible` (the team's mask:
/// all ones in Deathmatch; `0x800D2120` for team 0, `0x800D2124` otherwise), in index order,
/// except `last` (the record this player last respawned on, struct126+0x1B0; `None` before the
/// first respawn), which takes no draw. Each candidate scores its squared XZ distance to the
/// nearest enemy (`enemies`: every participant on another team, wherever it stands; capped at
/// 1e8, so anything beyond 10,000 units counts as equally far) plus (500·f)² from one draw; the
/// first highest score wins.
///
/// When every candidate is skipped the game keeps its list's first entry, which this mirrors;
/// an empty list (no eligible record) reads uninitialised stack in the game: index 0 here.
/// (The game also keeps the old record in Heist with a timer, scenario 0: not modelled.)
pub fn respawn(
    spawns: &[Spawn],
    eligible: u32,
    last: Option<usize>,
    enemies: &[[f32; 2]],
    rng: &mut Rng,
) -> Placement {
    let list = (0..spawns.len()).filter(|&i| eligible >> (i & 31) & 1 != 0);
    let (mut index, mut best) = (list.clone().next().unwrap_or(0), 0.0f32);
    for i in list.filter(|&i| Some(i) != last) {
        let [x, _, z] = spawns[i].pos.map(f32::from);
        let mut near = f32::from_bits(0x4CBE_BC20); // 1e8
        for &[ex, ez] in enemies {
            let (dx, dz) = (ex - x, ez - z);
            let d = dx * dx + dz * dz;
            if d < near {
                near = d;
            }
        }
        let r = rng.next_f32() * 500.0;
        let score = near + r * r;
        if best < score {
            (index, best) = (i, score);
        }
    }
    spawns[index].placement(index, spawns[index].pos)
}

/// obj+0xB8 from record byte +6: `(b·360) >> 8` degrees, wrapped into (-180, 180].
pub fn b6_degrees(b6: u8) -> f32 {
    let mut d = ((i32::from(b6) * 360) >> 8) as f32;
    while 180.0 < d {
        d -= 360.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(pos: [i16; 3], yaw: u8) -> Spawn {
        Spawn {
            pos,
            b6: 0,
            yaw,
            camera: [0.0; 3],
            b14: 0,
            b15: 0,
        }
    }

    /// Scene 0x2D record 1 with the RNG state the oracle had (spawn_h1b1_lldb.txt): player 1's
    /// well-known spawn (-2267, -521, 6016) facing 16896.
    #[test]
    fn place_matches_player_one() {
        let spawns = [rec([2005, -521, -6199], 193), rec([-2283, -521, 6017], 66)];
        let mut rng = Rng::new(0xd459_3932);
        let p = place(&spawns, 1, &mut rng);
        assert_eq!((p.pos, p.facing), ([-2267.0, -521.0, 6016.0], 16896));
        assert_eq!(rng.state, 0xf9bf_b302);
        let p = place(&spawns, 0, &mut rng); // the bot, next draw
        assert_eq!(p.pos, [1969.0, -521.0, -6198.0]);
    }

    #[test]
    fn respawn_prefers_far_records_and_skips_the_last() {
        let spawns = [
            rec([0, 0, 0], 0),
            rec([1000, 0, 0], 0),
            rec([5000, 0, 0], 0),
        ];
        let enemy = [[0.0, 0.0]];
        let mut rng = Rng::new(0x1234_5678);
        assert_eq!(respawn(&spawns, !0, None, &enemy, &mut rng).index, 2);
        let mut rng = Rng::new(0x1234_5678);
        assert_eq!(respawn(&spawns, !0, Some(2), &enemy, &mut rng).index, 1);
        // the skipped record takes no draw: two candidates, two draws
        let mut a = Rng::new(0x1234_5678);
        a.next_u32();
        a.next_u32();
        assert_eq!(rng, a);
        // beyond 10,000 units every record is as far as any other: the draw decides
        let far = [rec([20_000, 0, 0], 0), rec([30_000, 0, 0], 0)];
        let picks: Vec<usize> = (1..40)
            .map(|s| respawn(&far, !0, None, &enemy, &mut Rng::new(s)).index)
            .collect();
        assert!(picks.contains(&0) && picks.contains(&1));
        // masks and the all-skipped case
        let mut rng = Rng::new(1);
        assert_eq!(respawn(&spawns, 0b011, None, &enemy, &mut rng).index, 1);
        assert_eq!(respawn(&spawns, 0b100, Some(2), &enemy, &mut rng).index, 2);
        assert_eq!(respawn(&spawns, 0, None, &enemy, &mut rng).index, 0);
        // no draw takes place when nothing is a candidate
        let mut b = rng;
        respawn(&spawns, 0b100, Some(2), &[], &mut b);
        assert_eq!(b, rng);
    }

    #[test]
    fn out_of_range_index_is_record_0() {
        let spawns = [rec([100, 0, 0], 0), rec([200, 0, 0], 0)];
        assert_eq!(place(&spawns, 7, &mut Rng::new(1)).index, 0);
        assert_eq!(place(&spawns, 0x1001, &mut Rng::new(1)).index, 1); // low 12 bits
    }

    #[test]
    fn b6_wraps() {
        assert_eq!(b6_degrees(0), 0.0);
        assert_eq!(b6_degrees(6), 8.0); // 2160 >> 8
        assert_eq!(b6_degrees(248), -12.0); // 348 - 360
        assert_eq!(b6_degrees(128), 180.0);
    }
}
