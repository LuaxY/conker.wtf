//! War: Total War's gas (scene 0x2D): the props the level's AI script drives, and what the script
//! does to the match ([`Effect`]). The script (`[0x14, 0xBA]`, run by conker-bots) holds the whole
//! state machine in its globals: the canisters' timers, their carriers, the countdown, the
//! deliverer, the masks' removal. The match keeps what the engine keeps: the props (shown, +0x80,
//! +0x84), the canisters' touch, the masks' pickup (`Gives::Mask`), the carrier's item and the
//! hits; `Match::apply` takes the effects before the step. Research and runs d1-d3:
//! `../lab/research/mp-original/total-war-gas.md`.

use crate::damage::DamageType;

/// The props the script drives, by placement (prop k is placement k), with the id its handles name
/// them by (`0x30nn`: +0x72, the placement record's byte 0x33, C): the underground room's two
/// doors (0, 1), the covers beside the masks (2-7) and the canisters (27, 28).
pub const PROPS: [(u16, u8); 10] = [
    (0, 0xFF),
    (1, 0xFE),
    (2, 0xFD),
    (3, 0xFC),
    (4, 0xFB),
    (5, 0xFA),
    (6, 0xF9),
    (7, 0xF8),
    (27, 0x1A),
    (28, 0x1B),
];

/// The canisters, canister 0 first (G+0x1FC8: handles 0x301A, 0x301B), and where they stand (their
/// placements; V: the props' RAM). Canister 0 is in the Tediz' base and only team 0 touches it,
/// canister 1 in the squirrels' and only team 1 (the gates the script sets, [`admits`]; V d1, d2).
pub const CANISTERS: [(u16, [f32; 3]); 2] =
    [(27, [-4144.0, 40.0, 5368.0]), (28, [3898.0, 40.0, -5566.0])];

/// The carried canister's item (s126+0x75 = 0xA5, +0x78 = 0x25 while carried; the script's forced
/// item G+0xFF0) and the mask's (s126+0x75 = 0xA0; G+0x1174 of pickups k19-k24). V d1, d2.
pub const CANISTER: u8 = 0x25;
pub const MASK: u8 = 0x20;

/// Frames the mask's put-on (state 81) and take-off (78) hold the player: 19, idle on the 20th (V
/// e2: rows 1004-1022 and 1193-1211; their clips end on that frame at 3 ticks a frame).
pub const MASK_FRAMES: u32 = 19;

/// Frames the gassed screen runs ([`Effect::Haze`]): its timer goes 2 a frame and the object goes
/// at 301 (`func_151D275C`, C; V e1: drawn in 150 frames).
pub const HAZE_FRAMES: u32 = 150;

/// A prop as the engine keeps it for the script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prop {
    /// +0x6E clear: drawn, and a canister may be touched.
    pub shown: bool,
    /// +0x80: a canister's toucher (its slot + 1) until the script clears it; a door or a cover
    /// hides some frames after the script sets it to 1 ([`hide_after`]).
    pub flag: i32,
    /// +0x84: who may touch a canister ([`admits`]).
    pub gate: i32,
    /// The frame `flag` last changed.
    pub since: u32,
}

/// The gas's props and the script's state the match reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gas {
    /// [`PROPS`]' props, in order.
    pub props: [Prop; PROPS.len()],
    /// G+0x1FE4 after the script's frame: frames until the gas kills, 300 from the delivery's frame,
    /// 0 none (the HUD prints `(t + 59) / 60` of command F 1's `t`, which is this plus one, on the
    /// frames after the delivery's up to the kill's). Masks may be taken while it is above 0.
    pub countdown: i32,
    /// The gas is out: its looping sound 0x74CF runs (G+0x1FEC), from countdown 29 until the
    /// after-timer ends 89 frames after the kill.
    pub gas: bool,
}

impl Default for Gas {
    /// The match's start: doors and covers shown, the canisters hidden (V d2 read 23).
    fn default() -> Self {
        Self {
            props: PROPS.map(|(n, _)| Prop {
                shown: n < 8,
                ..Prop::default()
            }),
            countdown: 0,
            gas: false,
        }
    }
}

impl Gas {
    pub fn prop(&self, placement: u16) -> Option<&Prop> {
        let k = PROPS.iter().position(|p| p.0 == placement)?;
        self.props.get(k)
    }

    pub fn prop_mut(&mut self, placement: u16) -> Option<&mut Prop> {
        let k = PROPS.iter().position(|p| p.0 == placement)?;
        self.props.get_mut(k)
    }

    /// Bit n: the prop on placement n is shown.
    pub fn shown(&self) -> u32 {
        (PROPS.iter().zip(&self.props))
            .filter(|(_, p)| p.shown)
            .fold(0, |bits, ((n, _), _)| bits | 1 << n)
    }

    /// Canister `c`'s toucher (its slot), until the script takes it.
    pub fn toucher(&self, c: usize) -> Option<usize> {
        let flag = self.prop(CANISTERS.get(c)?.0)?.flag;
        usize::try_from(flag - 1).ok()
    }

    /// The doors and covers hide [`hide_after`] frames after their +0x80 went to 1.
    pub(crate) fn step(&mut self, t: u32) {
        for ((n, _), p) in PROPS.iter().zip(&mut self.props) {
            if hide_after(*n).is_some_and(|a| p.flag == 1 && t >= p.since + a) {
                p.shown = false;
            }
        }
    }
}

/// Frames from a door's or cover's +0x80 going to 1 until it hides (V d2, d3: the covers 1064 →
/// 1076, the doors 1452 → 1502; 51 at the match start, d2 23 → 74; I: the props' own animation,
/// unread).
fn hide_after(placement: u16) -> Option<u32> {
    match placement {
        0 | 1 => Some(50),
        2..=7 => Some(12),
        _ => None,
    }
}

/// A canister's team gate +0x84 lets a player of `team` (obj+0x128) touch it (`func_1511DF6C`, C):
/// 1 team 0 only, 2 the others only, 3 the even teams; 0 anyone.
pub fn admits(gate: i32, team: u8) -> bool {
    match gate {
        1 => team == 0,
        2 => team != 0,
        3 => team & 1 == 0,
        _ => true,
    }
}

/// What the AI script did this frame that the match carries out (`conker_bots::Bots::tick` returns
/// them, slots for its participants; `Match::apply`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Prop command 0 (`command(0x30nn, 0)`): the prop on this placement shows (a canister back
    /// home, a door shut, a cover back).
    Show(u16),
    /// Prop command 4 (`command(0x30nn, 4, _, flag, gate)`, 0x7FFFFFFF keeping a field): +0x80 (a
    /// door's or cover's state; 0 takes a canister's toucher) and +0x84 (a canister's gate).
    Set {
        placement: u16,
        flag: Option<i32>,
        gate: Option<i32>,
    },
    /// G+0x1FE4 after the frame ([`Gas::countdown`]).
    Countdown(i32),
    /// The gas sound 0x74CF started (object native 3) or stopped (native 4 on its handle).
    Gas(bool),
    /// Player commands 0x2A and 0x2B (`func_1505D024` with no guard flag: it lands on a living
    /// victim, immune or stunned): the doorway's hit (type 0x3B, no attacker) or the gas's (0x1F,
    /// the deliverer's, who may be dead by then); the script's angle is 0.
    Hit {
        slot: usize,
        kind: DamageType,
        attacker: Option<usize>,
    },
    /// Command F 0xC (`func_1508DAEC(p, 0)`, 0x40EC): the player lets go of its pickup item, the
    /// mask (the script then sets G+0xEB0 = −1).
    Release(usize),
    /// G+0xFF0, the forced item, is (or is no longer) the canister: the player carries it.
    Carry(usize, bool),
    /// `obj(0xE013, p)` (`func_151D26C0`, 0xBA at countdown 29 for each player outside trigger 0,
    /// masked or not): the player's gassed screen starts (its own view only; the client draws it).
    Haze(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Covers hide 12 frames after +0x80 goes to 1 however often the script sends it, doors 50; a
    /// show brings either back.
    #[test]
    fn doors_and_covers_hide_after_their_flag() {
        let mut g = Gas::default();
        assert_eq!(g.shown(), 0xFF);
        for (n, t) in [(0, 10), (2, 20)] {
            let p = g.prop_mut(n).unwrap();
            (p.flag, p.since) = (1, t);
        }
        for t in 10..=60 {
            g.step(t);
            assert_eq!(g.prop(2).unwrap().shown, t < 32, "cover at {t}");
            assert_eq!(g.prop(0).unwrap().shown, t < 60, "door at {t}");
        }
        assert_eq!(g.shown(), 0xFF & !0b101);
        g.prop_mut(27).unwrap().shown = true;
        assert_eq!(g.shown(), 0xFA | 1 << 27);
        assert!(
            (admits(1, 0), admits(1, 1), admits(2, 1), admits(2, 0)) == (true, false, true, false)
        );
    }
}
