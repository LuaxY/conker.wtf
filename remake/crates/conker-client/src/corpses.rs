//! What a player's death shows that the client derives from the rules' clip and [`Combat`]: the
//! dead pose's twitch (clip 0x18A), the head pop and the shotgun's and chainsaw's cut.
//!
//! **The twitch** (`func_15072A7C`, C). A clip's clock calls its callback (clock+0) each frame its
//! time reaches the loop's last tick (`func_1507BDB0`), and 0x59's (route runtime 0x39, table
//! 0x8008630C) is the twitch: 63 frames into the dead pose, then every 64 while it lies (0x59 loops,
//! 64 ticks at rate 1). Each call adds 1 to s126+0x1A9 (0 again at the respawn) and plays 0x18A
//! (rate 1, blend 4) if `(random_u32() & 7) < count`: a first call twitches 1 time in 8, the 8th
//! always. 0x18A (7 ticks, looping) has no callback: once it plays, the body twitches to the
//! respawn. V `heist-leftovers-scratch/twitch.out`: 63 frames in, e_w38, e_w21 and sg_e2 twitch,
//! e_w39, e40 and e40c don't, sgpb's 51 frames make no call. V out1 (P2 out of lives,
//! `cameras-clips-g4-scratch/clips/`): 0x59 from read 241, the count 1 at 304 with no twitch, 2 at
//! 368 with 0x18A (its time 0), which then loops to the end (1790). The game draws from its shared
//! generator, which the replays can't feed on that frame; here a hash of the player's slot, its
//! death count and the call's number decides, the same on every client.
//!
//! **The head pop** (`func_1507DE4C`, C): an object of kind 1 (obj+0 = 1, every player: V e_w39
//! reads 100, 971, 1100) loses actions 0x44's and 0x23's attachments (`func_150836CC`: a cigar and a
//! helmet on joint 12, none in multiplayer, I) and sets obj+0x9C |= [`POP`]; the draw zeroes the
//! 3×3 of each set joint's matrix (`func_15034728`), so their vertices collapse. Other kinds: 0x5A,
//! 0x74, 0x7A |= 0xFF8, 0x9F, 0xA0 |= 0xF000. It runs on a bullet's head hit below 3 health
//! (`func_151D4DAC`, the dead too) and on a ray's or projectile's first head hit of a life
//! (`func_15081E78`, s126+0x1AD). V: set on the lethal read, the clip 0x17E: combat4_w24 1004 (the
//! sniper), combat5_cb_e1 987 (a bolt), combat_bots3 1515, 1550 (twin guns); on a corpse:
//! combat_e_w39 971, combat_e_w09 1005 (in 0x58), bot_long2 5756 (in 0x11D); 0 again at the
//! respawn (cb_e1 1165). The server's `Combat::popped` carries it (a corpse's pop too); the lethal
//! death spin (0x17E, the rules' reaction 0x2E) pops here as well, but the shotgun's and the
//! chainsaw's (0x2F), which cut the body in two instead ([`HALVED_BY`]).
//!
//! **The cut** (`func_15194AB4`, C; V combat5_sg_e1 791 to the respawn 969, sg_e3 801, combat7_sgpb
//! 791, combat_e_w22 849): obj+0x9C |= 0xFFFE, joints 1-15 (`joint_01`, the upper body, and all
//! under it) collapse, and the model's stump shows ([`crate::held::worn`]).

use bevy::app::AnimationSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use conker_anim::combat::{DEAD, DEAD_MELEE_END, DEATH_SHOT};
use conker_sim::net::Combat;

use crate::held::Twisted;

/// The twitch's first call, in frames into the dead pose, and the calls' interval (0x59's loop).
pub const TWITCH_AT: u32 = 63;
pub const TWITCH_EVERY: u32 = 64;

/// The joints a player's head pop hides (obj+0x9C bits): `joint_12`, the head, and the three
/// under it in every player's rig.
pub const POP: u32 = 0xF000;

/// The weapons whose death spin (reaction 0x2F: the shotgun's kill sphere, the chainsaw's swing)
/// is no head pop: `func_15194AB4` sets obj+0x9C |= 0xFFFE (joints 1-15) and hangs a stump (action
/// 0x73: bank-09 #0x59 on joint 0 for model 117; 0x72: #0x58 on joint 1 for 128), C; V
/// combat5_sg_e1 791, combat_e_w22 849.
pub const HALVED_BY: [u8; 2] = [0x37, 0x22];

/// The scale the cut draws joints 1-15 at: next to nothing, not 0, so the stump hung on `joint_01`
/// (model 128's) can undo it ([`crate::held`] hangs it at the reciprocal).
// ponytail: the original zeroes each joint's 3×3 at the draw (`func_15034728`); whether 128's stump,
// drawn with joint 1's matrix, survives that is not traced (I: drawn, the pack has it to be seen).
pub const CUT: f32 = 1e-4;

/// A player's death as it shows: frames in the dead pose (the rules' [`DEAD`] or its fixed
/// [`DEAD_MELEE_END`] rows), whether it twitches, whether its head popped, whether it was cut in two.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Corpse {
    pub frames: u32,
    pub twitching: bool,
    pub popped: bool,
    pub halved: bool,
}

impl Corpse {
    /// One tick of player `c` whose rules show clip `id`: the clip to draw. In the dead pose, 0x59
    /// until a call twitches, 0x18A from then on (the rules' fixed 0x18A rows too: the draw
    /// decides). The pop from the lethal death spin to the respawn, the cut from the shotgun's or
    /// the chainsaw's.
    pub fn shown(&mut self, id: u16, c: &Combat) -> u16 {
        let cut = c.anim == DEATH_SHOT && HALVED_BY.contains(&c.hit);
        if c.health > 0 {
            (self.popped, self.halved) = (false, false);
        } else if cut {
            self.halved = true;
        } else if c.popped || c.anim == DEATH_SHOT {
            self.popped = true;
        }
        if id != DEAD && id != DEAD_MELEE_END {
            (self.frames, self.twitching) = (0, false);
            return id;
        }
        let n = self.frames;
        self.frames += 1;
        if n >= TWITCH_AT && (n - TWITCH_AT).is_multiple_of(TWITCH_EVERY) {
            let k = (n - TWITCH_AT) / TWITCH_EVERY + 1;
            self.twitching |= twitches(c.slot, c.deaths, k);
        }
        if self.twitching { DEAD_MELEE_END } else { DEAD }
    }
}

/// Call `k`'s draw for player `slot`'s death number `deaths`: `(r & 7) < k`, `r` a SplitMix64 hash
/// of the three.
pub fn twitches(slot: u16, deaths: u16, k: u32) -> bool {
    let z = (u64::from(slot) << 48 | u64::from(deaths) << 32 | u64::from(k))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) & 7 < u64::from(k)
}

/// The popped heads and the cut bodies hide once the clips have posed them.
pub fn build(app: &mut App) {
    app.add_systems(
        PostUpdate,
        hide_heads
            .after(AnimationSystems)
            .before(TransformSystems::Propagate),
    );
}

/// A popped player's head (`joint_12`, [`Twisted`]'s) at scale 0, the three joints under it with
/// it ([`POP`]); a cut body's `joint_01` at [`CUT`], joints 2-15 with it; each scale back once it
/// no longer is. The original collapses each joint on its own origin, here on the head's (the upper
/// body's): the same nothing drawn (I).
fn hide_heads(players: Query<(&Corpse, &Twisted)>, mut joints: Query<&mut Transform>) {
    for (corpse, twisted) in &players {
        for (joint, hidden, gone) in [
            (Some(twisted.0), corpse.halved, CUT),
            (twisted.1, corpse.popped, 0.0),
        ] {
            let Some(Ok(mut t)) = joint.map(|j| joints.get_mut(j)) else {
                continue;
            };
            if hidden {
                t.scale = Vec3::splat(gone);
            } else if t.scale == Vec3::splat(gone) {
                t.scale = Vec3::ONE;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_anim::combat::{DEATH_FALL, DEATH_SHOT_FALL, HIT, HIT_MELEE};
    use conker_anim::id::IDLE;

    /// The clips a corpse of slot 1's death `deaths` draws for the rules' `ids`, a tick each.
    fn drawn(deaths: u16, ids: &[(u16, u32)]) -> Vec<u16> {
        let mut corpse = Corpse::default();
        let c = Combat {
            slot: 1,
            deaths,
            ..Default::default()
        };
        let ids = ids.iter().flat_map(|&(id, n)| (0..n).map(move |_| id));
        ids.map(|id| corpse.shown(id, &Combat { anim: id, ..c }))
            .collect()
    }

    /// A death number whose first `k` draws are `hits`.
    fn death(hits: &[bool]) -> u16 {
        (1..u16::MAX)
            .find(|&d| (1..).zip(hits).all(|(k, &h)| twitches(1, d, k) == h))
            .unwrap()
    }

    /// The k-th call twitches k times in 8 (8000 deaths), the 8th always.
    #[test]
    fn a_first_death_twitches_one_time_in_eight() {
        let deaths = (0..8u16).flat_map(|slot| (1..=1000u16).map(move |d| (slot, d)));
        for k in 1..=7 {
            let hits = deaths.clone().filter(|&(s, d)| twitches(s, d, k)).count();
            let rate = hits as f64 / 8000.0;
            assert!(
                (rate - f64::from(k) / 8.0).abs() < 0.015,
                "call {k}: {rate}"
            );
        }
        assert!(deaths.clone().all(|(s, d)| twitches(s, d, 8)));
    }

    /// Melee 0x21's kill as the rules send it (0x11B 19, 0x58 52, 0x59 63, 0x18A 18): a hit draws
    /// 0x18A from 63 frames into 0x59 (V e_w38: 0x59 at 940, 0x18A at 1003), a miss keeps 0x59
    /// over the fixed rows (V e_w39, e40). A shot's 51 frames make no call (V sgpb).
    #[test]
    fn the_twitch_comes_63_frames_into_the_dead_pose() {
        let rows = [
            (HIT_MELEE, 19),
            (DEATH_FALL, 52),
            (DEAD, 63),
            (DEAD_MELEE_END, 18),
            (IDLE, 1),
        ];
        let hit = drawn(death(&[true]), &rows);
        assert_eq!(hit[..71], drawn(1, &rows[..2]));
        assert!(hit[71..134].iter().all(|&c| c == DEAD));
        assert!(hit[134..152].iter().all(|&c| c == DEAD_MELEE_END));
        assert_eq!(hit[152], IDLE);
        let miss = drawn(death(&[false]), &rows);
        assert!(miss[71..152].iter().all(|&c| c == DEAD));
        for d in 1..200 {
            let shot = drawn(d, &[(DEATH_SHOT, 76), (DEATH_SHOT_FALL, 52), (DEAD, 51)]);
            assert!(!shot.contains(&DEAD_MELEE_END));
        }
    }

    /// Out of lives, the body lies in 0x59 and draws again every 64 frames with the count up (V
    /// out1: no twitch at 63, 0x18A at 127, then to the end).
    #[test]
    fn a_body_that_lies_draws_again() {
        let ids = [(DEATH_FALL, 52), (DEAD, 1000)];
        let late = drawn(death(&[false, true]), &ids);
        let first = late.iter().position(|&c| c == DEAD_MELEE_END);
        assert_eq!(first, Some(52 + 127));
        assert!(late[179..].iter().all(|&c| c == DEAD_MELEE_END));
        for d in 1..200 {
            let at = drawn(d, &ids).iter().position(|&c| c == DEAD_MELEE_END);
            assert!(
                at.is_some_and(|n| n <= 52 + 63 + 7 * 64),
                "death {d}: {at:?}"
            );
        }
    }

    /// Two clients drawing the same player's deaths agree, tick by tick.
    #[test]
    fn every_client_draws_the_same() {
        let ids = [(DEATH_FALL, 52), (DEAD, 600), (IDLE, 1)];
        for d in 1..100 {
            assert_eq!(drawn(d, &ids), drawn(d, &ids));
        }
        let firsts: Vec<_> = (1..100).map(|d| drawn(d, &ids)[52 + 63]).collect();
        assert!(firsts.contains(&DEAD) && firsts.contains(&DEAD_MELEE_END));
    }

    /// The pop from the lethal death spin's tick (V combat4_w24 1004) through the fall and the
    /// lying, to the respawn (V cb_e1 1165); never on the shotgun's or the chainsaw's spin, nor on a
    /// body kill.
    #[test]
    fn a_head_kill_pops_to_the_respawn() {
        let popped = |rows: &[(u8, u16, u8)]| {
            let mut corpse = Corpse::default();
            let rows = rows.iter().map(|&(health, anim, hit)| {
                corpse.shown(
                    anim,
                    &Combat {
                        health,
                        anim,
                        hit,
                        ..Default::default()
                    },
                );
                corpse.popped
            });
            rows.collect::<Vec<_>>()
        };
        let sniper = [
            (6, IDLE, 0),
            (0, DEATH_SHOT, 0x24),
            (0, DEATH_SHOT_FALL, 0x24),
            (0, DEAD, 0x24),
            (6, IDLE, 0x24),
        ];
        assert_eq!(popped(&sniper), [false, true, true, true, false]);
        for w in HALVED_BY {
            assert!(!popped(&[(0, DEATH_SHOT, w), (0, DEAD, w)]).contains(&true));
        }
        let body = [
            (3, HIT, 0x39),
            (0, HIT, 0x39),
            (0, DEATH_FALL, 0x39),
            (0, DEAD, 0x39),
        ];
        assert!(!popped(&body).contains(&true));
    }

    /// The shotgun's and the chainsaw's kill (their 0x17E, reaction 0x2F) cuts the body from the
    /// lethal tick to the respawn (V combat5_sg_e1: 0xFFFE from read 791, its spin, to the respawn
    /// read 969), with no pop; any other kill never cuts.
    #[test]
    fn a_shotgun_or_chainsaw_kill_cuts_to_the_respawn() {
        let shown = |rows: &[(u8, u16, u8)]| {
            let mut corpse = Corpse::default();
            let rows = rows.iter().map(|&(health, anim, hit)| {
                let c = Combat {
                    health,
                    anim,
                    hit,
                    ..Default::default()
                };
                corpse.shown(anim, &c);
                (corpse.halved, corpse.popped)
            });
            rows.collect::<Vec<_>>()
        };
        for w in HALVED_BY {
            let sg_e1 = [
                (6, IDLE, 0),
                (0, DEATH_SHOT, w),
                (0, DEATH_SHOT_FALL, w),
                (0, DEAD, w),
                (6, IDLE, w),
            ];
            let cut = [(false, false), (true, false), (true, false), (true, false)];
            assert_eq!(shown(&sg_e1), [&cut[..], &[(false, false)]].concat());
        }
        let sniper = [(0, DEATH_SHOT, 0x24), (0, DEAD, 0x24)];
        assert!(shown(&sniper).iter().all(|&(cut, pop)| !cut && pop));
    }

    /// In every player model's rig of the pack, `joint_12` and the joints under it are [`POP`]'s.
    #[test]
    fn the_pop_hides_the_head_joints() {
        let dir = std::env::var("CONKER_ASSETS").map_or_else(
            |_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets"),
            Into::into,
        );
        for model in [
            0, 59, 117, 128, 136, 144, 150, 152, 156, 157, 159, 160, 176, 177, 178, 180,
        ] {
            let dir = dir.join(format!("characters/{model}"));
            let (Ok(gltf), Ok(bin)) = (
                std::fs::read(dir.join("character.gltf")),
                std::fs::read(dir.join("character.bin")),
            ) else {
                eprintln!("no pack character {model}: skipping");
                return;
            };
            let rig = conker_assets::pose::Skeleton::parse(&gltf, &bin).unwrap();
            let under = |mut j: usize| loop {
                match (j, rig.parents[j]) {
                    (12, _) => return true,
                    (_, Some(p)) => j = p,
                    (_, None) => return false,
                }
            };
            let mask: u32 = (0..rig.parents.len())
                .filter(|&j| under(j))
                .map(|j| 1 << j)
                .sum();
            assert_eq!(mask, POP, "model {model}");
        }
    }
}
