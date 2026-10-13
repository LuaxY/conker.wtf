//! The flamethrower 0x15: its stream of area hits and the burn it leaves on a victim
//! (`../lab/oracle/combat_README.md` §3.5; V on `combat_e_w15` and `combat4_e_w15` unless tagged).
//!
//! **Stream** (`func_1519C948` → `func_1519CDB0`, C): while the trigger is held the weapon keeps a
//! chain of flame points; every frame it hits with damage type 0x1C (radius 80) at the nozzle and at
//! even steps along the chain, one hit per chain segment. The flames start 8 frames after Z and stop
//! with the fire state (V: Z at 820, 850, 940, 970 → first hits at 828, 858, 948, 978; held to 910 →
//! last at 916). The chain's length and point count by frame are the traced ones (its particle
//! dynamics, `func_1519A9A4`, are not ported: I), clipped at the first wall ahead.
//!
//! **Burn** (`func_151D2F90` → `func_1518F7C4` → `func_151905BC`, C): a hit leaves a fire on the
//! victim for 125 frames. It counts VIs from 5 and every 125 hits its host with type 0x21 (1 damage,
//! refused while the host is immune or stunned: 887 refused, 950 landed). While it burns the host
//! runs (s126+0x46 = 1000, −100 a frame once the fire is out).

use crate::damage::DamageType;

/// The nozzle in the shooter's frame (forward, side, up; V: the first frame's hit at 828 and 858).
pub const NOZZLE: [f32; 3] = [73.4, 9.9, 33.4];
/// Frames from the Z press to the first flames.
pub const DELAY: u32 = 8;
/// The stream's forward length and hit count on its k-th frame (V e_w15 858-873); from the 15th
/// frame on it stays at full length.
const LENGTH: [f32; 16] = [
    21.9, 33.1, 87.0, 161.9, 194.5, 299.8, 342.4, 384.9, 427.5, 469.9, 512.2, 554.6, 597.0, 638.9,
    680.9, 688.9,
];
const HITS: [u8; 6] = [2, 2, 3, 4, 4, 5];

/// The stream's hit centres on its `k`-th frame, as distances ahead of the nozzle, the stream cut at
/// `wall` (the distance to the first wall ahead).
pub fn reach(k: u32, wall: f32) -> Vec<f32> {
    let k = k as usize;
    let len = LENGTH[k.min(LENGTH.len() - 1)].min(wall);
    let n = usize::from(HITS[k.min(HITS.len() - 1)]);
    (0..n).map(|j| len * j as f32 / (n - 1) as f32).collect()
}

/// A fire burning on a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Burn {
    /// VIs counted toward the next burn hit (+0x94; the period +0x90 is 125).
    pub acc: u16,
    /// Frames the fire has left.
    pub life: u16,
    /// Who lit it (the hits' attacker; none for a cone's, scene 0x34's kind-0xE floors).
    pub attacker: Option<usize>,
}

/// The burn hit's period (VIs) and the fire's life (frames).
pub const BURN_PERIOD: u16 = 125;
pub const BURN_LIFE: u16 = 125;
/// s126+0x46 after the fire is out: the host keeps burning this many frames.
pub const AFTERBURN: u16 = 10;

impl Burn {
    pub fn new(attacker: Option<usize>) -> Self {
        Self {
            acc: 5,
            life: BURN_LIFE,
            attacker,
        }
    }

    /// One frame: true when it hits its host now. The fire goes out after its last frame.
    pub fn step(&mut self) -> bool {
        self.life = self.life.saturating_sub(1);
        self.acc += 2;
        if self.acc >= BURN_PERIOD {
            self.acc -= BURN_PERIOD;
            return true;
        }
        false
    }
}

/// Type 0x1C, the flame (area, radius 80): 1 damage, knocked away at 30 with a hop, 10 immune
/// frames; the victim's sequence is the flame hit 0x69.
pub const FLAME: DamageType = DamageType {
    reaction: 0x10,
    damage: 0x21,
    angle: 0,
    xz: 30.0,
    y_vel: 5.0,
    gravity: 4.0,
    k: 1,
    immune: 0x80 | 10,
    flags: 0x0036,
    reach: [0, 0, 40],
};
pub const FLAME_R: f32 = 80.0;

/// Type 0x21, the burn's hit (direct, guarded): 1 damage, no knock-back, the bullets' stagger.
pub const BURN: DamageType = DamageType {
    reaction: 0x21,
    damage: 0x01,
    angle: 0,
    xz: 0.0,
    y_vel: 0.0,
    gravity: 6.0,
    k: 1,
    immune: 0x80 | 10,
    flags: 0,
    reach: [0, 0, 40],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// e_w15's stream: 2 hits on its first frame 21.9 apart, 5 on its 8th (97 apart: 865 is
    /// 70.0, 167.4, 264.4, 360.5, 454.9 ahead), cut by a wall.
    #[test]
    fn stream_grows() {
        assert_eq!(reach(0, 1e9), vec![0.0, 21.9]);
        let r = reach(7, 1e9);
        assert_eq!(r.len(), 5);
        assert!((r[1] - 96.2).abs() < 0.1 && r[4] == 384.9);
        assert_eq!(reach(40, 300.0)[4], 300.0);
    }

    /// combat4_e_w15: lit at 828 (+0x94 = 5 there), it hits at 887 and 950, and burns 125 frames.
    #[test]
    fn burn_ticks() {
        let mut b = Burn::new(Some(0));
        let hits: Vec<u32> = (828..1000).filter(|_| b.life > 0 && b.step()).collect();
        assert_eq!(hits, [887, 950]);
        let mut b = Burn::new(Some(0));
        assert_eq!(
            (828..1000)
                .take_while(|_| {
                    b.step();
                    b.life > 0
                })
                .count(),
            124
        );
    }
}
