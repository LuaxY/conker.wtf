//! The mounted machine guns (model 138 at scale 0.5): two on scene 0x30 (Deathmatch and War:
//! Colors), one on 0x34, one on 0x2B (War: Beach). A player who touches a free gun jumps onto its
//! seat, turns it with the scope (weapon 0x1B, conker-camera), fires 252-a-frame bullets of type 0x2C
//! from its two barrels while Z is held, overheats after 300 frames of fire, and leaves with A or its
//! death. Research and runs
//! m1, m2, e1, e2, b30, b34: `../lab/research/mp-original/mounted-gun.md`.
//!
//! The gunner's side ([`Mount`], on the [`crate::Fighter`]: the trigger and the fire process) is
//! predicted with the rest of its frame; the guns themselves ([`Gun`], `Match::guns`: the touch,
//! the dismount's wait, the shots the clients see) are the server's.

use crate::projectile::local_to_world;
use conker_sim::movement::Other;

/// The gun's states (gun+0x232, V): free, a player jumping on, manned idle, firing, the dismount's
/// wait, stopped (B or overheat).
pub const FREE: u8 = 1;
pub const LAUNCH: u8 = 2;
pub const IDLE: u8 = 3;
pub const FIRING: u8 = 4;
pub const LEAVING: u8 = 5;
pub const STOPPED: u8 = 0xB;

/// The fire budget (s126+0x24) in VIs, −2 a frame of Z; 0 overheats (V m2 795).
pub const BUDGET: u16 = 600;
/// The lock after B or an overheat (s126+0x19E) in VIs, −2 a frame: Z does nothing meanwhile.
pub const LOCK: u8 = 150;
/// Frames of fire after Z (V m2 111-130), stopped (V m2 291-316) and of the dismount (V m2 1601-1630).
pub const FIRE_ON: u8 = 20;
pub const STOP: u8 = 25;
pub const LEAVE: u8 = 30;
/// The bullet's speed a frame (+0x1A8, V e1).
pub const SPEED: f32 = 252.0;

/// The launch (`func_1505327C`, C; V m2, e1): y velocity and gravity, the frames clip 0x32 plays
/// before the fall's 0x38, and the mount once below the gun's origin + 50 and falling.
pub const LAUNCH_Y_VEL: f32 = 44.0;
pub const LAUNCH_GRAVITY: f32 = 3.2;
pub const LAUNCH_CLIP: u32 = 19;
pub const MOUNT_HEIGHT: f32 = 50.0;

/// The seat (joint 3) from the origin, along the yaw, to the side and up (V m2, e1 to 0.01; I: the
/// game's seat also moves with the barrels' pitch, a few units, V m2 1600).
pub const SEAT: [f32; 3] = [-62.96, 0.0, 72.145];
/// The barrels' pivot (joint 4), ahead and up, and a muzzle from it before the pitch: barrel joint
/// (j5, j7) × (±13, 0, 200) at scale 0.5 (C). Barrel 0 is on the −side (V e1: barrel 1's bullets
/// lean to −side, toward the sight).
pub const PIVOT: [f32; 2] = [61.42, 52.265];
pub const MUZZLE: [f32; 3] = [124.62, 22.175, 57.94];
/// The gun's body (obj+0xE4/0xE8/0xEC, V m2: radius 70, centre 70 up, vertical scale 70/99): it pushes
/// every player but its gunner out ([`body`]), and a player inside it touches it.
const BODY: (i16, i16, u32) = (70, 70, 0x3F35_0296);

/// A gun's place: its origin (+0x2C) and home yaw (+0x78).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub pos: [f32; 3],
    pub yaw: u16,
}

/// The guns of `scene` (V §1: runtime objects in the slots after the players, the script's handles
/// 0x2012 and 0x2013, scene uids 0x13 and 0x14): 0x30's A (south) and B (north), 0x34's one, and
/// 0x2B's (War: Beach, uid 0x11, the script's 0x2010; V `beach.md` §5: 60 from 0x30's A).
pub fn places(scene: usize) -> &'static [Place] {
    const S30: [Place; 2] = [
        Place {
            pos: [-322.0, -387.4, -3621.0],
            yaw: 48128,
        },
        Place {
            pos: [170.0, 544.0, 5938.0],
            yaw: 15872,
        },
    ];
    const S34: [Place; 1] = [Place {
        pos: [2428.0, 645.0, 537.0],
        yaw: 40960,
    }];
    const S2B: [Place; 1] = [Place {
        pos: [-337.0, -387.1, -3561.0],
        yaw: 48896,
    }];
    match scene {
        0x30 => &S30,
        0x34 => &S34,
        0x2B => &S2B,
        _ => &[],
    }
}

/// The match's guns on `scene`, free at home.
pub fn spawn(scene: usize) -> Vec<Gun> {
    places(scene).iter().map(|&p| Gun::new(p)).collect()
}

/// A gun as the server keeps it (its gunner is the player whose [`Mount::gun`] it is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gun {
    pub place: Place,
    /// +0x7A: the gunner's facing; kept when it leaves (C).
    pub yaw: u16,
    /// +0x2E4: the barrels' pitch × 10 (positive down); it decays by an eighth a frame unmanned.
    pub pitch10: i32,
    /// Frames left of the dismount's wait (state 5): nobody takes it meanwhile.
    pub leave: u8,
    /// Shots fired, and the last one's barrel (the clients' flash).
    pub shots: u16,
    pub last: u8,
}

impl Gun {
    pub fn new(place: Place) -> Self {
        Self {
            place,
            yaw: place.yaw,
            pitch10: 0,
            leave: 0,
            shots: 0,
            last: 0,
        }
    }

    /// gun+0x232 with its gunner's mount (if any).
    pub fn state(&self, gunner: Option<&Mount>) -> u8 {
        match gunner {
            Some(m) => m.state,
            None if self.leave > 0 => LEAVING,
            None => FREE,
        }
    }
}

/// A player on a gun (obj+0x65 = the gun's slot + 1): the gun's state as its gunner drives it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Mount {
    /// Its index in `Match::guns`, and its origin.
    pub gun: u8,
    pub at: [f32; 3],
    /// Where the jump on homes (s126+0x28/0x2C, C `func_1505327C`): the seat's x and z at the touch.
    pub to: [f32; 2],
    /// [`LAUNCH`] until seated, then [`IDLE`], [`FIRING`] or [`STOPPED`].
    pub state: u8,
    /// Frames left of [`FIRING`] after Z, or of [`STOPPED`].
    pub timer: u8,
    pub budget: u16,
    pub lock: u8,
    /// s126+0x199: Z fired it, until let go with the lock off (the release's sound).
    pub trigger: bool,
    /// The gun's fire process (`func_150FC614`, [`Mount::fire`]): running, its timer, the next
    /// barrel, and its draws' state (the game's shared `random_float`; the mount's own here, seeded
    /// at the touch, so the gunner's client predicts its shots).
    pub firing: bool,
    pub next: f32,
    pub barrel: u8,
    pub rng: u64,
}

impl Mount {
    pub fn new(gun: usize, at: [f32; 3]) -> Self {
        Self {
            gun: gun as u8,
            at,
            to: [at[0], at[2]],
            state: LAUNCH,
            timer: 0,
            budget: BUDGET,
            lock: 0,
            trigger: false,
            firing: false,
            next: 0.0,
            barrel: 0,
            rng: 1,
        }
    }

    /// One frame of the fire process after its gunner's (`func_150FC614`, C; V e1): off unless
    /// [`FIRING`]; then a shot in the first frame and one each time the timer, losing 1 a frame,
    /// drops below 0, which then gains 4 to 8. The barrel of this frame's shot, if any; the barrels
    /// take turns.
    pub fn fire(&mut self) -> Option<u8> {
        if self.state != FIRING {
            self.firing = false;
            return None;
        }
        if !self.firing {
            (self.firing, self.next) = (true, 0.0);
        }
        self.next -= 1.0;
        if self.next >= 0.0 {
            return None;
        }
        let mut rng = crate::Rng::new(self.rng);
        self.next += 4.0 + 4.0 * rng.next_f32();
        self.rng = rng.state;
        let b = self.barrel;
        self.barrel ^= 1;
        Some(b)
    }

    /// One seated frame of the trigger (`func_15065A5C`'s case 0x1B, C; V m2): the lock runs down; B
    /// stops the gun (and refills it); Z held with the lock off fires, spending the budget, which
    /// overheats it at 2; let go, the fire carries on [`FIRE_ON`] frames.
    pub fn step(&mut self, z: bool, b: bool) {
        self.lock = self.lock.saturating_sub(2);
        if b {
            self.stop();
        } else if self.lock == 0 && z {
            (self.trigger, self.state, self.timer) = (true, FIRING, FIRE_ON);
            if self.budget <= 2 {
                self.stop();
            } else {
                self.budget -= 2;
            }
        } else {
            if self.lock == 0 {
                self.trigger = false;
            }
            if matches!(self.state, FIRING | STOPPED) {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.state = IDLE;
                }
            }
        }
    }

    fn stop(&mut self) {
        (self.state, self.timer, self.budget, self.lock) = (STOPPED, STOP, BUDGET, LOCK);
    }
}

pub use conker_camera::{gun_pitch10 as pitch10, gun_tilt as tilt};

/// Where the gun at `at` turned `yaw`, its barrels at `pitch10` (+0x2E4), seats its gunner. The
/// whole gun tilts about its origin by [`tilt`], and the seat (joint 3) with it.
pub fn seat(at: [f32; 3], yaw: u16, pitch10: i32) -> [f32; 3] {
    local_to_world(at, yaw, tilted(tilt(pitch10), SEAT))
}

/// A point (ahead, side, up) of the gun's frame, the gun tilted `t` degrees nose down.
fn tilted(t: f32, [f, s, u]: [f32; 3]) -> [f32; 3] {
    let t = t.to_radians();
    let (c, sn) = (t.cos(), t.sin());
    [f * c + u * sn, s, u * c - f * sn]
}

/// The barrels pitched `pitch` degrees down: a point (ahead, side, up) of the barrels' frame from the
/// pivot, in the gun's (ahead, side, up) from its origin.
fn pitched(pitch: f32, [f, s, u]: [f32; 3]) -> [f32; 3] {
    let p = pitch.to_radians();
    let (pc, ps) = (p.cos(), p.sin());
    [PIVOT[0] + f * pc + u * ps, s, PIVOT[1] + u * pc - f * ps]
}

/// Barrel `b`'s muzzle: the gun tilted by [`tilt`], the barrels by the rest of `pitch`.
pub fn muzzle(at: [f32; 3], yaw: u16, pitch: f32, b: u8) -> [f32; 3] {
    barrel(at, yaw, pitch, b, MUZZLE)
}

/// A point (ahead, side, up) of barrel `b`'s frame from the pivot (barrel 0's side negated).
fn barrel(at: [f32; 3], yaw: u16, pitch: f32, b: u8, [f, s, u]: [f32; 3]) -> [f32; 3] {
    let s = if b == 0 { -s } else { s };
    let t = tilt(pitch10(pitch));
    local_to_world(at, yaw, tilted(t, pitched(pitch - t, [f, s, u])))
}

/// Where a human's shot from barrel `b` starts its aim (`func_150FB8C0`, C): the scope's ray from
/// `eye` along the unit `d` (s126+0x13C, +0x130) crossed with the plane through the muzzle and the
/// joint's (±50, 0, 200) and (±13, 50, 220) (tables 0x800A1E04 and 0x800A1E1C, V k3), across the
/// barrel and leaning 22° forward. The shot heads from the muzzle for what the ray meets from
/// there on, else leaves from this point along `d` (`Match`'s fire; I: neither e1's hit nor its
/// later shots tell this point apart from the eye's).
pub fn sight(at: [f32; 3], yaw: u16, pitch: f32, b: u8, eye: [f32; 3], d: [f32; 3]) -> [f32; 3] {
    let m = muzzle(at, yaw, pitch, b);
    let [f, s, u] = MUZZLE;
    let q = barrel(at, yaw, pitch, b, [f + 50.0, s, u - 20.0]);
    let dot = |a: [f32; 3], c: [f32; 3]| a[0] * c[0] + a[1] * c[1] + a[2] * c[2];
    let n: [f32; 3] = core::array::from_fn(|j| q[j] - m[j]);
    let t = dot(n, core::array::from_fn(|j| m[j] - eye[j])) / dot(n, d);
    core::array::from_fn(|j| eye[j] + d[j] * t)
}

/// A bot's shot (it has no camera): along barrel `b`, toed in by `toe` (1 to 7 units in 100, C: the
/// AI's aim point (±(12 − 6r), 0, 300) against the muzzle's (±13, 0, 200)).
pub fn barrel_dir(yaw: u16, pitch: f32, b: u8, toe: f32) -> [f32; 3] {
    let side = if b == 0 { toe } else { -toe };
    let o = local_to_world([0.0; 3], yaw, pitched(pitch, [0.0, 0.0, 0.0]));
    let d = local_to_world([0.0; 3], yaw, pitched(pitch, [100.0, side, 0.0]));
    let v = [d[0] - o[0], d[1] - o[1], d[2] - o[2]];
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / n)
}

/// The gun at `at` as a body the players are pushed out of (`func_1505D6F0`, C; V p1: a Tedi put 40
/// in front of a manned gun on its slope comes to rest 81.65 from the origin, the 3-D gap 93). The
/// gunner is not (its obj+0x65 skips the push), nor one jumping on (s126+0x27, C `func_1505327C`).
pub fn body(at: [f32; 3]) -> Other {
    let (radius, lift, y_scale) = BODY;
    Other {
        pos: at,
        radius,
        lift,
        y_scale: f32::from_bits(y_scale),
    }
}

/// The touch: the player overlaps the gun's body. The gun's own update pushes against the players
/// (`func_15059140` → `func_1505D6F0`) and takes the bit its overlap test sets (C; V t1: a Tedi on
/// the floor 91 from the origin does not touch, 88 does, after its own push to 88.55; the gap is
/// 89.5 there).
pub fn touches(at: [f32; 3], player: &Other) -> bool {
    body(at).overlaps(player)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// m2 (gun B, seated from read 84; Z at 101-110, 171-230, 401-440 and 471-830, B at 291 and 421,
    /// Z 321-326 in the lock): the carry-over, B's stop and lock, the burst B cuts and its fire
    /// resuming as the lock ends, the overheat and its lock, the release.
    #[test]
    fn m2_trigger() {
        let mut m = Mount::new(1, places(0x30)[1].pos);
        m.state = IDLE;
        let held = |t: u32, a: u32, b: u32| (a..=b).contains(&t);
        let mut rows = Vec::new();
        for t in 84..=1000u32 {
            let z = [
                (101, 110),
                (171, 230),
                (321, 326),
                (401, 440),
                (471, 830),
                (901, 906),
            ]
            .iter()
            .any(|&(a, b)| held(t, a, b));
            m.step(z, t == 291 || t == 421);
            rows.push((t, m.state, m.budget, m.lock, m.trigger));
        }
        let at = |t: u32| rows[(t - 84) as usize];
        let state = |t: u32| at(t).1;
        assert_eq!(
            (state(100), state(101), state(129), state(130)),
            (IDLE, FIRING, FIRING, IDLE)
        );
        assert_eq!(at(130).2, 580);
        assert_eq!((state(249), state(250), at(250).2), (FIRING, IDLE, 460));
        assert_eq!((state(291), at(291).3, at(291).2), (STOPPED, LOCK, BUDGET));
        assert_eq!(
            (state(315), state(316), at(365).3, at(366).3),
            (STOPPED, IDLE, 2, 0)
        );
        assert!((321..=326).all(|t| state(t) == IDLE));
        assert_eq!(
            (state(420), state(421), state(495), state(496)),
            (FIRING, STOPPED, IDLE, FIRING)
        );
        assert_eq!((state(794), state(795), at(794).2), (FIRING, STOPPED, 2));
        assert_eq!(
            (state(819), state(820), at(869).3, at(870).3),
            (STOPPED, IDLE, 2, 0)
        );
        assert_eq!((at(869).4, at(870).4), (true, false));
        assert_eq!((state(901), state(926)), (FIRING, IDLE));
    }

    /// The seat at home yaw (V m2 83, e1 66) and the muzzles' sides.
    #[test]
    fn seat_and_barrels() {
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02);
        let [a, b] = [places(0x30)[1], places(0x34)[0]];
        assert!(near(seat(a.pos, a.yaw, 0), [166.91, 616.14, 6000.88]));
        assert!(near(seat(b.pos, b.yaw, 0), [2472.52, 717.14, 492.48]));
        // m2's seat as the pitch (+0x2E4) moves: reads 1406, 1430, 1482 and 1510.
        for (p10, at) in [
            (39, [166.921555, 616.336609, 6000.66309]),
            (500, [167.099411, 619.327881, 5997.04248]),
            (-9, [166.899826, 615.950256, 6001.10547]),
            (-600, [166.678955, 611.733215, 6005.60107]),
        ] {
            assert!(near(seat(a.pos, a.yaw, p10), at), "{p10}");
        }
        // Level, at yaw 0 (forward +x, side +z): both 186 ahead, 110.2 up, 22.2 to either side.
        let m = [0, 1].map(|k| muzzle([0.0; 3], 0, 0.0, k));
        assert!(near(m[0], [186.04, 110.205, -22.175]) && near(m[1], [186.04, 110.205, 22.175]));
        let d = barrel_dir(0, 0.0, 1, 3.0);
        assert!(d[2] < 0.0 && d[1] == 0.0);
    }
}
