//! War: Heist (scenario 0, scene 0x1A): four teams fetch the cash bag from the vault's centre to
//! their own base. The level's AI script (`[0x14, 0xB7]`'s team branch, run by conker-bots) holds
//! the bag's life in its globals (G+0x1EC8..+0x1EDC, the timer G+0x1ED4): it creates and destroys
//! it, notices a pickup, makes a carrier that was hit, died or threw it let go, sends the bag
//! walking home (its behaviour scripts 4 and 5 on the nav graph, conker-bots' walker) and scores a
//! carrier inside its team's zone. The match keeps what the engine keeps: the bag (shown, carried,
//! thrown, falling, sliding, standing or walking), the carrier's grab, carry and throw, and the
//! four team scores (`gas::Effect`'s Heist variants, `Match::apply`). Research and runs h1-h5,
//! bots1, unt1: `lab/research/mp-original/heist.md`, `heist-presentation.md`.

use conker_sim::collision::{Body, CollisionWorld, Sweep};
use conker_sim::movement::{self, Other, PlayerState};

/// REDS, BLUES, GREENS, YELLOWS (team row 0, C).
pub const TEAMS: usize = 4;
/// The bag as a carried object ([`crate::Fighter::flag`]: Colors' flags are 0 and 1).
pub const CASH: u8 = 2;
/// Its handle's object index (`obj(0x2011)` answers 0x2010) and its scene uid.
pub const HANDLE: u16 = 0x10;
pub const UID: u16 = 0x11;
/// Model 87 at scale 0.4, standing at the vault's centre (`[0x0E, 0x1A]` record 0, C, V h1).
pub const MODEL: u16 = 87;
pub const HOME: [f32; 3] = [-396.0, -17.0, -152.0];
/// The bag target's default (opt[4]: 1-5, C).
pub const SCORE: u8 = 3;
/// Each team's start and respawn records (script 0x26's init, cmd 0x1F's masks; C, V).
pub const RECORDS: [[u8; 4]; TEAMS] = [
    [0, 8, 9, 10],
    [1, 11, 12, 13],
    [2, 14, 15, 16],
    [3, 17, 18, 19],
];
/// Frames a thrown bag can't be touched from the release (obj+0x104 254 until then: V h2 395 →
/// 453), and one let go of on a hit or a death (V h3 377 → 380).
pub const THROWN: u32 = 58;
pub const DROPPED: u32 = 3;
/// The carrier's clips (V h1, h2): the grab (26 frames), the stand, walk and run (above 14, as the
/// flag's), the throw (20 frames, the bag leaves the hand 15 in: V h2 B 380 → 395).
pub const GRAB: u16 = 0x114;
pub const IDLE: u16 = 0x115;
pub const WALK: u16 = 0x116;
pub const RUN: u16 = 0x117;
pub const THROW: u16 = 0x118;
/// The throw: the bag leaves 49.5 ahead of the carrier's feet and 42.7 up, flies 9 a frame along
/// the facing, rising 18.4 less 3 a frame (V h2 395-411: lands 194 ahead), then slides 5, 5, then
/// 0.9 of the last until under 0.5 (244 ahead at 434).
const HAND: [f32; 2] = [49.5, 42.7];
const FLY: f32 = 9.0;
const RISE: f32 = 18.4;
const GRAVITY: f32 = 3.0;
const SLIDE: f32 = 5.0;
/// The thrown bag's box in the wall routine: held 24 off a wall, sliding along it (V bt1200: stops
/// at x -1324.9 flying, bt1100: at -1324.6 sliding, the vault's ring wall at -1349; I the height).
pub const SWEPT: Body = Body {
    half_width: 24,
    radius: 24,
    height: 48.0,
    center_y: 24.0,
};
/// Where the carrier holds it, ahead of and above its feet (I: the client draws it on the joint).
const HELD: [f32; 2] = [10.0, 40.0];
/// The walk home (class 0x57's script 5, `op 0x57` at speed 35): from 5 a frame, 2.5 more each, to
/// 17.5, turning at most 3840 a frame (V h3 510-518, bag_timeline 453-462; I the gain's shape).
pub const WALK_FROM: f32 = 5.0;
pub const WALK_GAIN: f32 = 2.5;
pub const WALK_TOP: f32 = 17.5;
pub const TURN: u16 = 3840;

/// What the bag is doing (the wire's `CashView::state`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub enum State {
    /// On the floor, still (clip 0).
    #[default]
    Stand,
    /// In a carrier's hands (clips 6, 5).
    Carried,
    /// Thrown, in the air (clip 9).
    Flight,
    /// Let go of on a hit or a death (clips 7, 8).
    Fall,
    /// Landed from a throw, sliding on (clip 10).
    Slide,
    /// Walking home along the nav graph (clip 3).
    Walk,
}

/// The bag as the engine keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cash {
    /// Created by the script and not destroyed since.
    pub shown: bool,
    /// obj+0x13D: the carrier's slot.
    pub carrier: Option<usize>,
    pub pos: [f32; 3],
    /// Its facing (u16 angle units).
    pub yaw: u16,
    pub state: State,
    /// The state's first frame.
    pub since: u32,
    /// The first frame it may be touched (obj+0x104 back to 0).
    pub touchable_at: u32,
    /// +0x25C bit 0x40: thrown, until the script's command 1 (query 0xA3).
    pub thrown: bool,
    /// The last carrier (a delivery's credit, [`crate::Fighter::bags`]).
    pub last: Option<usize>,
    /// Flight: the y step of its next frame.
    rise: f32,
}

impl Cash {
    /// The script's create (`obj(0x2011)`, then command 2): at home, standing, touchable.
    pub fn new(t: u32) -> Self {
        Self {
            shown: true,
            pos: HOME,
            since: t,
            ..Self::default()
        }
    }

    /// Touchable in frame `t`: shown, not carried, past its untouchable frames, not in the air.
    pub fn touchable(&self, t: u32) -> bool {
        self.shown
            && self.carrier.is_none()
            && t >= self.touchable_at
            && matches!(self.state, State::Stand | State::Walk | State::Slide)
    }

    /// May walk in frame `t` (the walker's steps apply): touchable and on its feet.
    pub fn walks(&self, t: u32) -> bool {
        self.touchable(t) && matches!(self.state, State::Stand | State::Walk)
    }

    /// Taken by player `i` in frame `t`.
    pub(crate) fn take(&mut self, i: usize, t: u32) {
        (self.carrier, self.last) = (Some(i), Some(i));
        (self.state, self.since) = (State::Carried, t);
    }

    /// Let go of by its carrier at `pos` facing `facing` in frame `t`: thrown (B), else dropped.
    pub(crate) fn let_go(&mut self, pos: [f32; 3], facing: u16, thrown: bool, t: u32) {
        let (c, s) = movement::cos_sin(facing);
        let [ahead, up] = if thrown { HAND } else { HELD };
        self.pos = [pos[0] + ahead * c, pos[1] + up, pos[2] - ahead * s];
        (self.carrier, self.yaw, self.since, self.thrown) = (None, facing, t, thrown);
        (self.state, self.rise) = match thrown {
            true => (State::Flight, RISE),
            false => (State::Fall, 0.0),
        };
        self.touchable_at = t + if thrown { THROWN } else { DROPPED };
    }

    /// Held by a carrier at `pos` facing `facing`.
    pub(crate) fn held(&mut self, pos: [f32; 3], facing: u16) {
        let (c, s) = movement::cos_sin(facing);
        let [ahead, up] = HELD;
        self.pos = [pos[0] + ahead * c, pos[1] + up, pos[2] - ahead * s];
        self.yaw = facing;
    }

    /// One frame of its flight, fall or slide in frame `t`: the flight and the slide go through the
    /// wall routine ([`SWEPT`]).
    pub(crate) fn step(&mut self, world: &CollisionWorld, t: u32) {
        if !self.shown || self.carrier.is_some() || self.since == t {
            return;
        }
        let (c, s) = movement::cos_sin(self.yaw);
        let [x, y, z] = self.pos;
        match self.state {
            State::Flight => {
                let new = [x + FLY * c, y + self.rise, z - FLY * s];
                let swept = world.sweep(self.pos, new, &SWEPT, &Sweep::default(), self.rise);
                self.rise = swept.y_vel - GRAVITY;
                self.pos = swept.pos;
                let [x1, y1, z1] = swept.pos;
                if let Some(f) = world.floor(x1, z1, y, y, None)
                    && y1 <= f.y
                {
                    self.pos[1] = f.y;
                    (self.state, self.since) = (State::Slide, t);
                }
            }
            State::Slide => {
                let n = (t - self.since).saturating_sub(2);
                let v = SLIDE * 0.9f32.powi(n as i32);
                if v < 0.5 {
                    (self.state, self.since) = (State::Stand, t);
                    return;
                }
                let new = [x + v * c, y, z - v * s];
                self.pos = world
                    .sweep(self.pos, new, &SWEPT, &Sweep::default(), 0.0)
                    .pos;
                let [x1, _, z1] = self.pos;
                if let Some(f) = world.floor(x1, z1, y, y, None) {
                    self.pos[1] = f.y;
                }
            }
            // Down on the floor at once; it shows its fall for the untouchable frames (V h3: clips 7
            // and 8 377-379, walking from 380).
            State::Fall => {
                if let Some(f) = world.floor(x, z, y, y, None) {
                    self.pos[1] = f.y;
                }
                if t >= self.touchable_at {
                    (self.state, self.since) = (State::Stand, t);
                }
            }
            _ => {}
        }
    }
}

/// The bag's body: a squirrel's with its radius 29 (C obj+0xD2; I the rest). It pushes the players
/// out of it (V h1: held off at 350 from it).
pub fn body(pos: [f32; 3]) -> Other {
    Other {
        radius: 29,
        ..crate::characters::body(4).at(pos)
    }
}

/// The touch test, as Colors' flag's ([`crate::colors::touches`]).
pub fn touches(cash: &Cash, player: &PlayerState, me: Other) -> bool {
    let [x, y, z] = player.pos;
    let low = y + 0.5 * (movement::GROUND_Y_VEL - movement::GRAVITY_FALL);
    !player.airborne
        && body(cash.pos).overlaps(&Other {
            pos: [x, low, z],
            ..me
        })
}

/// The carrier's clip by its ground speed (as the flag's, [`crate::colors::carry_clip`]).
pub fn carry_clip(speed: f32) -> u16 {
    if speed < 1.0 {
        IDLE
    } else if speed > 14.0 {
        RUN
    } else {
        WALK
    }
}

/// The bag and the four team scores.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heist {
    pub cash: Cash,
    /// Each team's bags S (`0x800D213C + 0x1C·team + 0xC`): the script's command 0x62.
    pub scores: [u16; TEAMS],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throw along +x (facing 0) from the origin on a floor at 0 (V h2): untouchable for 58 frames, in the
    /// air for 16, 194 ahead on landing, sliding to about 244.
    #[test]
    fn a_throw_flies_lands_and_slides() {
        let floor = [
            [-1000.0, 0.0, -1000.0],
            [3000.0, 0.0, 0.0],
            [-1000.0, 0.0, 1000.0],
        ];
        let world = CollisionWorld::new([(floor, 0)]);
        let mut c = Cash::new(0);
        c.take(0, 10);
        c.let_go([0.0; 3], 0, true, 100);
        assert!(c.thrown && !c.touchable(157) && c.touchable_at == 158);
        let mut landed = None;
        for t in 101..200 {
            c.step(&world, t);
            if c.state == State::Slide && landed.is_none() {
                landed = Some((t, c.pos[0]));
            }
        }
        let (t, x) = landed.unwrap();
        assert_eq!(t, 116);
        assert!((x - 193.5).abs() < 0.01, "{x}");
        assert_eq!(c.state, State::Stand);
        assert!((c.pos[0] - 243.5).abs() < 1.0, "{:?}", c.pos);
        assert!(c.touchable(158) && c.walks(158));
    }

    /// A throw at a wall 150 ahead (V bt1200, bt1100: the bag is held 24 off the vault's wall and
    /// slides along it, in the air and on the floor).
    #[test]
    fn a_throw_stops_at_a_wall() {
        let big = 1000.0;
        let world = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 0),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 0),
            // facing back at the thrower
            (
                [
                    [150.0, -100.0, -500.0],
                    [150.0, 400.0, 500.0],
                    [150.0, 400.0, -500.0],
                ],
                0,
            ),
            (
                [
                    [150.0, -100.0, -500.0],
                    [150.0, -100.0, 500.0],
                    [150.0, 400.0, 500.0],
                ],
                0,
            ),
        ]);
        let mut c = Cash::new(0);
        c.take(0, 10);
        c.let_go([0.0; 3], 0xFB00, true, 100);
        for t in 101..200 {
            c.step(&world, t);
            assert!(c.pos[0] < 127.0, "{t}: {:?}", c.pos);
        }
        assert_eq!(c.state, State::Stand);
        assert!(c.pos[0] > 125.9 && c.pos[2] > 29.0, "{:?}", c.pos);
    }
}
