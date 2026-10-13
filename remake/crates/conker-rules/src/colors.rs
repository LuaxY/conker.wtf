//! War: Colors (scenario 10, scene 0x30): capture the flag. The level's AI script (`[0x14, 0xBB]`,
//! run by conker-bots) holds the flags' state machine in its globals (G+0x2014..+0x2070, the
//! capture's timers G+0x2698, +0x26A0, +0x1EE0): it creates and destroys the flags, notices a
//! pickup, makes a carrier that was hit, died or put its flag down let go, scores a carrier inside
//! its team's zone and runs the freeze, the respawn and the flags' return after a capture. The
//! match keeps what the engine keeps: the flags (shown, carried, falling, touched), the carrier's
//! grab, carry and put-down, the team scores and the poses the script asks for (`gas::Effect`'s
//! Colors variants, `Match::apply`). Research and runs e0, e1, b1, h1, h2, bots1:
//! `../lab/research/mp-original/colors.md`.

use conker_anim::combat as anim;
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{self, Other, PlayerState, Side};

/// A flag: the handle the script names it by (its object index, `0x20nn`), the scene uid its
/// create native `obj(0x20uu)` takes, its model (bank 01) and its home (C, V e1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagDef {
    pub handle: u16,
    pub uid: u16,
    pub model: u16,
    pub home: [f32; 3],
}

/// The flags in the script's order (G+0x2014[f]): f = 0 the red flag, the Tediz' in the south
/// base, which the squirrels (team 0) take; f = 1 the green, the squirrels' in the north, which
/// the Tediz take ([`takes`]).
pub const FLAGS: [FlagDef; 2] = [
    FlagDef {
        handle: 0x1B,
        uid: 0x1C,
        model: 169,
        home: [-330.0, 410.0, -3803.0],
    },
    FlagDef {
        handle: 0x1A,
        uid: 0x1B,
        model: 168,
        home: [131.0, 1342.0, 6144.0],
    },
];

/// The score target's default (opt[18]: 1-40, 0 unlimited; C).
pub const SCORE: u8 = 20;
/// Lives in Colors: 6, never lost, timed or not (the rules flags 0x1; V u1, e0).
pub const LIVES: u8 = 6;
/// Frames a released flag can't be touched: +0x25C bit 8 is back 50 frames after the release's
/// (V b1 371 → 421).
pub const UNTOUCHABLE: u32 = 50;
/// The carried flag from its carrier, along the facing, to the side and up: the hand's joint while
/// carrying (I: its mean over V e1 349-382, the clip sways it a few units).
pub const HELD: [f32; 3] = [19.0, 7.5, 38.0];
/// A released flag falls 1.65 in its first frame, 3 more each next one, onto the floor (V b1
/// 370-376: 455.11 → 453.46 → 448.81 … → 410.12).
pub const FALL: f32 = 1.65;
pub const GRAVITY: f32 = 3.0;

/// Player of `team` may take flag `f`: the other team's (`func_150597FC`: model 0xA9 needs
/// obj+0x128 = 0, 0xA8 any other; C, V h2: no owner's touch).
pub fn takes(f: usize, team: u8) -> bool {
    (team == 0) == (f == 0)
}

/// A flag's body, a squirrel's: radius 30, centre 33 up, scale 0x3EE8BA2F (V e1 RAM). It pushes
/// the players out of it as theirs do (V e0 403-421: to 60 from a squirrel, e1 to 53 from a Tedi).
pub fn body(pos: [f32; 3]) -> Other {
    Other::player(Side::Squirrels, pos)
}

/// The touch test, before the player moves that frame: the overlap the engine's push finds
/// (`func_1505D408`'s touch bits, C), with the player where the push measures it, a ground frame's
/// fall step below its floor, and both on the floor (+0x28 = 0). So the push's own rest, 59.97 out,
/// takes nothing (V e0: held at 60.0 off the red flag once touchable, no grab).
pub fn touches(flag: &Flag, player: &PlayerState, side: Side) -> bool {
    let [x, y, z] = player.pos;
    let low = y + 0.5 * (movement::GROUND_Y_VEL - movement::GRAVITY_FALL);
    flag.fall.is_none()
        && !player.airborne
        && body(flag.pos).overlaps(&Other::player(side, [x, low, z]))
}

/// Where a carrier at `pos` facing `facing` holds its flag ([`HELD`]).
pub fn held(pos: [f32; 3], facing: u16) -> [f32; 3] {
    let (c, s) = conker_sim::movement::cos_sin(facing);
    let [fwd, side, up] = HELD;
    [
        pos[0] + fwd * c + side * s,
        pos[1] + up,
        pos[2] - fwd * s + side * c,
    ]
}

/// A flag as the engine keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flag {
    /// Created by the script and not destroyed since: drawn (and, not carried, on the radar).
    pub shown: bool,
    /// obj+0x13D: the carrier's slot.
    pub carrier: Option<usize>,
    /// Where it stands, falls or is held.
    pub pos: [f32; 3],
    /// Falling: the y step of its next frame.
    pub fall: Option<f32>,
    /// The frame its carrier last let go of it (since its creation).
    pub released: Option<u32>,
    /// obj+0x2E4: just let go of, until the script reads it and clears it (command 0x23).
    pub dropped: bool,
}

impl Flag {
    /// Touchable in frame `t`: shown, not carried, and [`UNTOUCHABLE`] frames past a release.
    pub fn touchable(&self, t: u32) -> bool {
        self.shown && self.carrier.is_none() && self.released.is_none_or(|r| t > r + UNTOUCHABLE)
    }

    /// At home: shown and never let go of since its creation.
    pub fn home(&self) -> bool {
        self.shown && self.carrier.is_none() && self.released.is_none()
    }

    /// One frame of its fall, onto the floor under it.
    pub(crate) fn fall(&mut self, world: &CollisionWorld) {
        let Some(step) = self.fall else {
            return;
        };
        let [x, y, z] = self.pos;
        let y1 = y - step;
        match world.floor(x, z, y, y, None) {
            Some(f) if y1 <= f.y => (self.pos[1], self.fall) = (f.y, None),
            _ => (self.pos[1], self.fall) = (y1, Some(step + GRAVITY)),
        }
    }
}

/// The flags, the scores and the capture's fade.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Colors {
    /// [`FLAGS`]' flags, in order.
    pub flags: [Flag; 2],
    /// Each team's score S (the participant record's +0xC, `0x800D213C + 0x1C·team`): the
    /// script's command 0x62 adds a capture.
    pub scores: [u16; 2],
    /// The screen's fade around the respawn after a capture (`command(0x50BB, 0x12, 0, 0, 0, n,
    /// out)`): out (to black) or back in, over `n` VIs, from this frame.
    pub fade: Option<(bool, u16, u32)>,
    /// Slots the script respawns (command 0x82), at the start of the frame's step.
    pub respawns: u32,
}

/// What the capture's script has a player do (command 0x45, s126+0x7B, and the carrier's own put-down
/// on B): the clip, the frames it holds the player (stick and buttons do nothing) and the frame the
/// carried flag leaves the hand (V obj+0x84 in e0, e1, b1; I that 0x45 plays them, its field stays 0
/// in RAM, but each starts on its call's frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub enum Pose {
    /// The touch's grab (V e1 327-343).
    Grab,
    /// B: the flag put down (V b1 351-389, let go at 370).
    PutDown,
    /// 0x45 9: the scorer plants the flag in its zone (V e0 383, let go at 402).
    Plant,
    /// 0x45 0xA: the scorer's cheer, 30 frames after (V e0 413; bots1 49 frames).
    Cheer,
    /// 0x45 0xC: the other team's, through the freeze to the respawn (V e0 413; I its end).
    Lose,
    /// War: Heist: the bag's grab (V h1 334-359).
    Snatch,
    /// Heist: B throws the bag (V h2 381-400, the release on 395).
    Throw,
}

impl Pose {
    pub fn clip(self) -> (u16, u32, Option<u32>) {
        match self {
            Pose::Grab => (anim::FLAG_GRAB, 17, None),
            Pose::PutDown => (anim::FLAG_PUT_DOWN, 39, Some(20)),
            Pose::Plant => (anim::FLAG_PLANT, 30, Some(19)),
            Pose::Cheer => (anim::CHEER, 49, None),
            Pose::Lose => (anim::LOSE, 60, None),
            Pose::Snatch => (crate::heist::GRAB, 26, None),
            Pose::Throw => (crate::heist::THROW, 20, Some(15)),
        }
    }

    /// Command 0x45's argument.
    pub fn of(n: i32) -> Option<Self> {
        match n {
            9 => Some(Pose::Plant),
            0xA => Some(Pose::Cheer),
            0xC => Some(Pose::Lose),
            _ => None,
        }
    }
}

/// The carried flag's clip by the ground speed (`func_15064B94` with model 0xA8/0xA9 held: run above
/// 14, else walk, standing below 1; C, V e1 344-382).
pub fn carry_clip(speed: f32) -> u16 {
    if speed < 1.0 {
        anim::FLAG_IDLE
    } else if speed > 14.0 {
        anim::FLAG_RUN
    } else {
        anim::FLAG_WALK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The red flag goes to the squirrels, the green to the Tediz; a flag let go of falls as in b1.
    #[test]
    fn flags_take_and_fall() {
        assert!(takes(0, 0) && !takes(0, 1) && takes(1, 1) && !takes(1, 0));
        // The floor at 410 (the world's vertices are whole units; b1's is 410.12).
        let floor = [
            [-400.0, 410.0, -3900.0],
            [-200.0, 410.0, -3900.0],
            [-300.0, 410.0, -3700.0],
        ];
        let world = CollisionWorld::new([(floor, 0)]);
        let mut f = Flag {
            shown: true,
            pos: [-307.02, 455.11, -3789.7],
            fall: Some(FALL),
            ..Flag::default()
        };
        let mut ys = Vec::new();
        while f.fall.is_some() {
            f.fall(&world);
            ys.push((f.pos[1] * 100.0).round() / 100.0);
        }
        assert_eq!(ys, [453.46, 448.81, 441.16, 430.51, 416.86, 410.0]);
        assert!(f.touchable(0) && f.home());
        f.released = Some(370);
        assert!(!f.touchable(420) && f.touchable(421));
    }
}
