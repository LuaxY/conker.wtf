//! The armed idle's fidget (`func_1507F640` → `func_1507EFD0`, C; V on `combat4_e_w09` with the
//! game's RNG state fed at each pick, and on `combat_e_w23`'s idle without it).
//!
//! In multiplayer only the twin guns, weapon 0x23 and weapon 0x16 vary their drawn idle (the
//! other weapons and the unarmed player keep one idle clip). The idle is re-picked when the player
//! enters it and each time its clip reaches its last tick:
//! - a timer (s126+0x5A) counts 2 VIs a frame in the idle (not on the frame it enters it);
//! - a pick compares it with a random threshold (`func_1507F4C0`, one draw): 180-239 VIs in mode
//!   0, 60-119 in mode 1, 0-29 in mode 2 (s126+0x64);
//! - past it, the mode steps (`func_1507EEF4`: 0 → 1; 1 draws once and goes to 2 after 3 picks;
//!   2 draws once and goes back to 1 after 8-11) and the weapon's two sets are the candidates
//!   (its list holds the plain set 5 times and the fidget once, but duplicates collapse); else
//!   set 0 (the plain idle);
//! - candidates among the last 5 picks (s126+0x5F) are dropped; if none is left, the newer half
//!   of those that were played come back (`func_1507EC38`); 2 or more left: one draw picks;
//! - in modes 1 and 2 every pick restarts the timer, so a short idle clip (weapon 0x23's, 30
//!   frames) almost never reaches mode 1's threshold again.
//!
//! Health below 2 skips the threshold (no draw, the plain set).

use conker_anim::combat::{self as anim, IDLE_CLIPS};

use crate::Rng;

/// (weapon id, plain set, fidget set, plain idle, fidget) for the weapons that fidget
/// (`0x80086C24` lists 5, 9 and 10; set → animation through `0x8009B8B0`).
const WEAPONS: [(u8, u8, u8, u16, u16); 3] = [
    (0x09, 1, 2, anim::TWIN_IDLE, anim::TWIN_FIDGET),
    (0x23, 29, 30, anim::GUN23_IDLE, anim::GUN23_FIDGET),
    (0x16, 31, 32, anim::W16_IDLE, anim::W16_FIDGET),
];

pub fn fidgets(weapon: u8) -> bool {
    WEAPONS.iter().any(|w| w.0 == weapon)
}

/// The clip's length in frames (these idles play at rate 1).
fn clip(anim: u16) -> u32 {
    IDLE_CLIPS
        .iter()
        .find(|c| c.0 == anim)
        .map_or(30, |c| u32::from(c.1))
}

/// The idle-variation state (struct126 +0x58..+0x65), reset whenever the player leaves the idle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Fidget {
    /// In the idle (s126+0x58).
    pub active: bool,
    /// VIs since the last pick that restarted it (s126+0x5A).
    pub timer: u16,
    /// s126+0x64 and the picks counted in it (s126+0x65).
    pub mode: u8,
    pub count: u8,
    /// The last 5 picks, newest first (s126+0x5F).
    pub recent: [u8; 5],
    /// The idle animation showing.
    pub anim: u16,
    /// The frame of the next pick (the clip's last tick).
    pub next: u32,
}

impl Fidget {
    /// One frame of `weapon`'s drawn idle at frame `t`; `late` is true when the idle began after
    /// the frame's animation advance (a draw or reload ending), false when the state machine
    /// entered it first (a burst of fire ending). Returns the idle animation.
    pub fn step(&mut self, weapon: u8, health: u8, t: u32, late: bool, rng: &mut Rng) -> u16 {
        if !self.active {
            *self = Fidget {
                active: true,
                ..Fidget::default()
            };
            self.pick(weapon, health, rng);
            // A clip begun before the frame's advance shows tick 1 on the next row.
            self.next = t + clip(self.anim) - if late { 1 } else { 2 };
        } else {
            self.timer = self.timer.saturating_add(2);
            if t == self.next {
                let old = self.anim;
                self.pick(weapon, health, rng);
                // The same clip keeps looping; another one starts at tick 0.
                self.next = t + clip(self.anim) - u32::from(self.anim != old);
            }
        }
        self.anim
    }

    fn pick(&mut self, weapon: u8, health: u8, rng: &mut Rng) {
        let Some(&(_, plain, fidget, plain_anim, fidget_anim)) =
            WEAPONS.iter().find(|w| w.0 == weapon)
        else {
            return;
        };
        let candidates: &[u8] = if health >= 2 && self.timer >= self.threshold(rng) {
            self.step_mode(rng);
            &[plain, fidget]
        } else {
            &[0]
        };
        let (out, cleared) = choose(candidates, &self.recent);
        let set = if out.len() >= 2 {
            out[(rng.next_u32() % out.len() as u32) as usize]
        } else {
            out[0]
        };
        self.count = self.count.wrapping_add(1);
        if self.mode != 0 {
            self.timer = 0;
        }
        if cleared && let Some(r) = self.recent.iter_mut().rev().find(|r| **r == set) {
            *r = 0;
        }
        self.recent = [
            set,
            self.recent[0],
            self.recent[1],
            self.recent[2],
            self.recent[3],
        ];
        self.anim = if set == fidget {
            fidget_anim
        } else {
            plain_anim
        };
    }

    /// `func_1507F4C0`.
    fn threshold(&self, rng: &mut Rng) -> u16 {
        let (base, span) = match self.mode {
            0 => (180, 60),
            1 => (60, 60),
            _ => (0, 30),
        };
        (rng.next_u32() % span + base) as u16
    }

    /// `func_1507EEF4`.
    fn step_mode(&mut self, rng: &mut Rng) {
        match self.mode {
            0 => (self.mode, self.count) = (1, 0),
            1 => {
                rng.next_u32();
                if self.count >= 3 {
                    (self.mode, self.count) = (2, 0);
                }
            }
            _ => {
                if u32::from(self.count) >= (rng.next_u32() & 3) + 8 {
                    (self.mode, self.count) = (1, 0);
                }
            }
        }
    }
}

/// `func_1507EC38`: the candidates (duplicates collapsed) not among the recent picks; when none is
/// left, the newer half of the recent picks that are candidates, newest last (and true).
fn choose(candidates: &[u8], recent: &[u8; 5]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    for &c in candidates {
        if !out.contains(&c) && !recent.contains(&c) {
            out.push(c);
        }
    }
    if !out.is_empty() {
        return (out, false);
    }
    let found: Vec<u8> = recent
        .iter()
        .copied()
        .filter(|&r| r != 0 && candidates.contains(&r))
        .collect();
    if found.is_empty() {
        return (vec![0], false);
    }
    let n = (found.len() / 2).max(1);
    (found.iter().rev().take(n).copied().collect(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// combat4_e_w09 (twin guns): the picks at frames 1069 (entering the idle after a burst),
    /// 1119, 1171, 1223, 1324, 1375 and 1476 with the RNG word the game had there, and the idle
    /// rows they produced (0x7F, then 0x144 at 1224 and 1376).
    #[test]
    fn twin_guns_fidget_like_the_game() {
        let mut f = Fidget::default();
        let states: [(u32, u64, u16); 7] = [
            (1069, 0x0203_24a7, 0x7F),
            (1119, 0xe70d_3ea3, 0x7F),
            (1171, 0xe01c_fdc3, 0x7F),
            (1223, 0x1_c058_5866, 0x144),
            (1324, 0xe66f_47a4, 0x7F),
            (1375, 0x1_1b01_4613, 0x144),
            (1476, 0x1_148f_c78a, 0x7F),
        ];
        let mut rng = Rng::default();
        for t in 1069..1480 {
            if let Some(s) = states.iter().find(|s| s.0 == t) {
                rng = Rng::new(s.1);
                let anim = f.step(0x09, 6, t, false, &mut rng);
                assert_eq!((t, anim), (t, s.2));
                continue;
            }
            assert!(t != f.next, "unexpected pick at {t}");
            f.step(0x09, 6, t, false, &mut rng);
        }
        assert_eq!((f.mode, f.recent[..2].to_vec()), (2, vec![1, 2]));
    }

    /// combat_e_w23: weapon 0x23's 30-frame idle, entered as a reload ends at 1110, picks at 1139,
    /// 1169, 1199 and 1229, where the timer (238) passes mode 0's threshold unless it drew 239;
    /// from then on every pick restarts the timer, so while the 30-frame clip shows it stays at or
    /// below 60, mode 1's least threshold.
    #[test]
    fn short_idle_restarts_its_timer() {
        for seed in 1..50 {
            let mut f = Fidget::default();
            let mut rng = Rng::new(seed);
            for t in 1110..1500 {
                let anim = f.step(0x23, 6, t, true, &mut rng);
                if t == 1229 && f.mode == 1 {
                    assert_eq!(f.timer, 0);
                }
                assert!(t < 1229 || f.mode == 0 || anim != 0x165 || f.timer <= 60);
            }
        }
    }

    #[test]
    fn choose_falls_back_to_the_newer_half() {
        assert_eq!(choose(&[1, 1, 1, 1, 1, 2], &[0; 5]), (vec![1, 2], false));
        assert_eq!(choose(&[1, 2], &[2, 0, 0, 0, 0]), (vec![1], false));
        assert_eq!(choose(&[1, 2], &[1, 2, 0, 0, 0]), (vec![2], true));
        assert_eq!(choose(&[0], &[1, 2, 0, 0, 0]), (vec![0], false));
        assert_eq!(choose(&[0], &[1, 2, 3, 4, 5]), (vec![0], false));
    }
}
