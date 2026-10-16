//! Bullets and grenades as the game moves them (`lab/oracle/combat_README.md` §3.1-3.2), and
//! the two world queries they need, written over conker-sim's public collision data: a ray cast
//! against the static triangles and the player's cylinder.

use conker_sim::collision::{Body, CollisionWorld, Sweep, Triangle};
use conker_sim::movement::{atan2_u16, cos_sin};

use crate::damage::{DamageType, Hitbox};
use crate::weapon::{BULLET_RANGE, BULLET_SPEED};

/// A point given in the frame of a body at `pos` facing `angle` (forward, side, up) in world space.
/// Forward is the game's `(cos a, −sin a)`, side `(sin a, cos a)`.
pub fn local_to_world(pos: [f32; 3], angle: u16, [f, s, u]: [f32; 3]) -> [f32; 3] {
    let (c, sn) = cos_sin(angle);
    [pos[0] + f * c + s * sn, pos[1] + u, pos[2] - f * sn + s * c]
}

/// The floor's rise per unit of horizontal motion along `(c, −s)` (as conker-sim's movement steps
/// along a slope).
pub fn rise_along([nx, ny, nz]: [f32; 3], c: f32, s: f32) -> f32 {
    if ny == 0.0 {
        0.0
    } else {
        (nz * s - nx * c) / ny
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Möller-Trumbore: the distance along `d` from `o` to the triangle, if it is hit ahead.
fn ray_triangle(t: &Triangle, o: [f32; 3], d: [f32; 3]) -> Option<f32> {
    let v = t.v.map(|p| p.map(|c| c as f32));
    let (e1, e2) = (sub(v[1], v[0]), sub(v[2], v[0]));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det == 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, v[0]);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let w = dot(d, q) * inv;
    if w < 0.0 || u + w > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t >= 0.0).then_some(t)
}

/// Distance from `from` along the unit vector `dir` to the first collision triangle, at most
/// `max` (I: the game's own ray, `func_15081690`, is unread; its bullet lives match this within
/// the numbers in `tests/combat.rs`). Candidates come from the world's grid sampled every 64
/// units, which its 32-unit cell margin makes exhaustive.
pub fn raycast(world: &CollisionWorld, from: [f32; 3], dir: [f32; 3], max: f32) -> f32 {
    let mut best = max;
    let mut t = 0.0f32;
    loop {
        let (x, z) = (from[0] + dir[0] * t, from[2] + dir[2] * t);
        for &k in world.near(x as i32, z as i32) {
            if let Some(d) = ray_triangle(&world.tris[k as usize], from, dir)
                && d < best
            {
                best = d;
            }
        }
        if t >= best {
            return best;
        }
        t = (t + 64.0).min(best);
    }
}

/// The cylinder of a player `h` standing at `body` (radius obj+0xD2, from obj+0xD6 − obj+0xD4 to
/// obj+0xD6 + obj+0xD4 above the feet: the Tedi's 26, −8 to 78; I for the bullet sweep), its top
/// [`CROUCH_DROP`] lower while `crouched` (I: combat4_w24's ray at +64.6 passed over a crouching
/// Tedi): where the segment from `p` along the unit `dir`, `len` long, enters it, if it does.
pub fn hits_body(
    p: [f32; 3],
    dir: [f32; 3],
    len: f32,
    body: [f32; 3],
    h: Hitbox,
    crouched: bool,
) -> Option<f32> {
    let top = h.centre + h.half - if crouched { CROUCH_DROP } else { 0.0 };
    let (rx, rz) = (p[0] - body[0], p[2] - body[2]);
    let a = dir[0] * dir[0] + dir[2] * dir[2];
    let b = rx * dir[0] + rz * dir[2];
    let c = rx * rx + rz * rz - h.radius * h.radius;
    let t = if c <= 0.0 {
        0.0
    } else {
        let disc = b * b - a * c;
        if a == 0.0 || disc < 0.0 || b >= 0.0 {
            return None;
        }
        (-b - disc.sqrt()) / a
    };
    let y = p[1] + dir[1] * t - body[1];
    (t <= len && (h.centre - h.half..=top).contains(&y)).then_some(t)
}

/// The body parts `func_15081E78` tests the sniper's ray and weapon 0x14's shot against (part 0xA
/// is the head), as spheres on the player's axis: (height above the feet, radius). I: fitted to the
/// traced shots 18.4 off the axis: head hits at +64.6 (e_w24, combat4_pitch24 1103), a body hit at
/// +36.6 (pitch24 953), nothing at +52.6 and +7.6 (pitch24 883, 1023). A crouching player's parts
/// are [`CROUCH_DROP`] lower (I: combat4_w24's level ray at +64.6 passed over one).
pub const HEAD_PART: [f32; 2] = [66.0, 20.0];
pub const TORSO_PART: [f32; 2] = [35.0, 22.0];
pub const CROUCH_DROP: f32 = 26.0;

/// Where the segment from `p` along the unit `dir`, `len` long, enters the sphere at `c`.
pub fn hits_sphere(p: [f32; 3], dir: [f32; 3], len: f32, c: [f32; 3], r: f32) -> Option<f32> {
    let o = sub(p, c);
    let (b, cc) = (dot(o, dir), dot(o, o) - r * r);
    if cc <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - cc;
    if disc < 0.0 || b >= 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t <= len).then_some(t)
}

/// The head's part: the hit record's +0x58 for a head sphere (`func_151D4DAC`, `func_15081E78`).
pub const HEAD: u8 = 0xA;

/// One body-part sphere where the last frame drew it: its joint matrix · its offset (world space),
/// its radius × the scale, its joint (matrix slot) and part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    pub centre: [f32; 3],
    pub radius: f32,
    pub joint: u8,
    pub part: u8,
}

/// A player's body parts for this frame's hit tests (`lab/research/mp-original/held-leftovers.md`
/// §2; [`crate::game::Match::parts`]): the broad sphere (radius and centre height above the feet,
/// × the scale), the joints the head pop hides (obj+0x9C), the spheres in the table's order, and
/// the feet they were placed at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyParts {
    pub broad: [f32; 2],
    pub hidden: u32,
    pub spheres: Vec<Sphere>,
    pub origin: [f32; 3],
}

/// `func_151452C4`: how far along the ray from `o` along the unit `d` it meets the sphere at `c`
/// of radius `r`: where it enters, or where it leaves when `o` is inside; none behind `o`.
pub fn ray_sphere(o: [f32; 3], d: [f32; 3], c: [f32; 3], r: f32) -> Option<f32> {
    let f = sub(c, o);
    let b = dot(f, d);
    let q = dot(f, f) - b * b;
    if r * r < q {
        return None;
    }
    let s = (r * r - q).sqrt();
    let t = if b < s { b + s } else { b - s };
    (t >= 0.0).then_some(t)
}

impl BodyParts {
    /// `func_1508114C`: the ray from `p` along the unit `dir` against the player whose feet are at
    /// `at`, its spheres moved by `shift` (where its shooter saw it, less [`Self::origin`]). The broad
    /// sphere first: a miss is no hit. Then each sphere in the table's order but the hidden
    /// joints': a hit replaces the best one if it is the first, or nearer and of a part ≥ the
    /// best's (so a head sphere, once best, keeps it against a nearer body sphere). The distance
    /// along the ray and the part; the caller compares the distance with its reach.
    pub fn hit(
        &self,
        p: [f32; 3],
        dir: [f32; 3],
        at: [f32; 3],
        shift: [f32; 3],
    ) -> Option<(f32, u8)> {
        let [r, h] = self.broad;
        ray_sphere(p, dir, [at[0], at[1] + h, at[2]], r)?;
        let mut best: Option<(f32, u8)> = None;
        for s in &self.spheres {
            // `1 << joint` on a 32-bit word: MIPS shifts by the low 5 bits.
            if self.hidden >> (s.joint & 31) & 1 != 0 {
                continue;
            }
            let c = core::array::from_fn(|i| s.centre[i] + shift[i]);
            if let Some(t) = ray_sphere(p, dir, c, s.radius)
                && best.is_none_or(|(d, part)| t < d && s.part >= part)
            {
                best = Some((t, s.part));
            }
        }
        best
    }
}

/// The first of [`HEAD_PART`] and [`TORSO_PART`] of a player at `body` that the segment meets:
/// how far along, and whether it is the head.
pub fn hits_parts(
    p: [f32; 3],
    dir: [f32; 3],
    len: f32,
    body: [f32; 3],
    crouched: bool,
) -> Option<(f32, bool)> {
    let drop = if crouched { CROUCH_DROP } else { 0.0 };
    let part =
        |[h, r]: [f32; 2]| hits_sphere(p, dir, len, [body[0], body[1] + h - drop, body[2]], r);
    match (part(HEAD_PART), part(TORSO_PART)) {
        (Some(h), Some(t)) if t < h => Some((t, false)),
        (Some(h), _) => Some((h, true)),
        (None, t) => t.map(|t| (t, false)),
    }
}

/// The area tests' ellipsoid around a player `h` standing at `body` (`func_1505C7D8` for a source
/// of no size: radius obj+0xD2, half height obj+0xD4, centre obj+0xD6 above the feet): where the
/// segment from `p` along the unit `dir`, `len` long, enters it. A rocket goes off there (V
/// combat4_w16: 119.05 along its path, this gives 119.07).
pub fn hits_ellipsoid(
    p: [f32; 3],
    dir: [f32; 3],
    len: f32,
    body: [f32; 3],
    h: Hitbox,
) -> Option<f32> {
    let k = h.radius / h.half;
    let o = [
        p[0] - body[0],
        (p[1] - body[1] - h.centre) * k,
        p[2] - body[2],
    ];
    let d = [dir[0], dir[1] * k, dir[2]];
    let (a, b, c) = (dot(d, d), dot(o, d), dot(o, o) - h.radius * h.radius);
    if c <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - a * c;
    if a == 0.0 || disc < 0.0 || b >= 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    (t <= len).then_some(t)
}

/// A bullet (the pool updated by `func_151C2734`): 300 units a frame along the shooter's facing, or
/// the mounted gun's 252 along its aim ([`Bullet::aimed`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bullet {
    pub pos: [f32; 3],
    /// Velocity per frame (+0x114), its direction and length (+0x1A8).
    pub vel: [f32; 3],
    pub dir: [f32; 3],
    pub speed: f32,
    /// Frames left (+0x19C): the distance to the wall it will hit, over the speed.
    pub life: f32,
    /// The knock-back angle it gives (`atan(direction) − 0x4000 | 1`).
    pub angle: u16,
    pub owner: usize,
    pub kind: DamageType,
    /// It tests body parts (`func_151C229C`'s flag 8, [`BodyParts`]): the twin guns' and the
    /// pistol's; not 0x23's, the tommy gun's or the mounted gun's (C). Set by the shooter.
    pub parts: bool,
}

impl Bullet {
    pub fn fire(
        world: &CollisionWorld,
        owner: usize,
        muzzle: [f32; 3],
        angle: u16,
        kind: DamageType,
    ) -> Self {
        let (c, s) = cos_sin(angle);
        let dir = [c, 0.0, -s];
        Self {
            pos: muzzle,
            vel: dir.map(|d| d * BULLET_SPEED),
            dir,
            speed: BULLET_SPEED,
            life: raycast(world, muzzle, dir, BULLET_RANGE) / BULLET_SPEED,
            angle: angle | 1,
            owner,
            kind,
            parts: false,
        }
    }

    /// A bullet from `muzzle` along the unit `dir` (any pitch) at `speed` a frame; its knock-back
    /// angle is the direction's in xz.
    pub fn aimed(
        world: &CollisionWorld,
        owner: usize,
        muzzle: [f32; 3],
        dir: [f32; 3],
        speed: f32,
        kind: DamageType,
    ) -> Self {
        let angle = match &world.acos {
            Some(a) => a.angle(dir[0], -dir[2]),
            None => atan2_u16(-dir[2], dir[0]),
        };
        Self {
            pos: muzzle,
            vel: dir.map(|d| d * speed),
            dir,
            speed,
            life: raycast(world, muzzle, dir, BULLET_RANGE) / speed,
            angle: angle | 1,
            owner,
            kind,
            parts: false,
        }
    }

    /// How far this frame's sweep reaches.
    pub fn reach(&self) -> f32 {
        self.life.min(1.0) * self.speed
    }

    /// The step after a sweep that hit nothing; false once spent.
    pub fn advance(&mut self) -> bool {
        self.pos = [
            self.pos[0] + self.vel[0],
            self.pos[1] + self.vel[1],
            self.pos[2] + self.vel[2],
        ];
        self.life -= 1.0;
        self.life > 0.0
    }
}

/// Weapon 0x16's rocket (the pool `func_151C1180` updates; V combat4_ctlw_k7, combat4_w16): it
/// leaves the muzzle along the shooter's aim (s126+0x16C yaw, +0x170 pitch: its facing, level,
/// without a scope; V beach-gunner r1: a bot's 31 rockets left along its aim to 0.3°, up to 37°
/// down) at 200 a frame, flying from its first frame, with a life of (distance to the wall ahead) /
/// 200 like the bullets. A body within the frame's reach, or the wall at the end of its life, sets
/// it off ([`crate::damage::ROCKET_INNER`], `_OUTER`): k7's rockets went off 6 frames after Z on the
/// wall 1,229.6 ahead (life 6.148), w16's on player 2 in the frame of the shot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rocket {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub life: f32,
    pub owner: usize,
}

pub const ROCKET_SPEED: f32 = 200.0;
/// How far a rocket or an aimed shot looks for a wall (I).
pub const FLIGHT_RANGE: f32 = 20_000.0;

impl Rocket {
    pub fn fire(world: &CollisionWorld, owner: usize, muzzle: [f32; 3], dir: [f32; 3]) -> Self {
        Self {
            pos: muzzle,
            dir,
            life: raycast(world, muzzle, dir, FLIGHT_RANGE) / ROCKET_SPEED,
            owner,
        }
    }

    /// How far this frame's sweep reaches.
    pub fn reach(&self) -> f32 {
        self.life.min(1.0) * ROCKET_SPEED
    }

    /// Where it is `d` along this frame's sweep.
    pub fn at(&self, d: f32) -> [f32; 3] {
        core::array::from_fn(|i| self.pos[i] + self.dir[i] * d)
    }

    /// The step after a sweep that hit no body; false when it reached the wall (it explodes there,
    /// at `at(reach())` before the step).
    pub fn advance(&mut self) -> bool {
        if self.life <= 1.0 {
            return false;
        }
        self.pos = self.at(ROCKET_SPEED);
        self.life -= 1.0;
        true
    }
}

/// Weapon 0x14's aimed shot (`func_15063FA0` type 2 → `func_1506C460`): an object at speed 150,
/// so 75 a frame, from 27 to the side of the scope's eye toward what the scope looks at, moving
/// from the frame after the shot; a body on its step takes the hit (`func_15081E78`, kind 2), a
/// wall ends it. V combat4_probe14 (born exactly 27 to the side, aimed at the wall 1,229 ahead,
/// 75 a frame, the wall reached on the 17th frame), combat4_w14 (fired at 830, player 2 hit at 832
/// at 146.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    /// Distance to the wall ahead.
    pub wall: f32,
    pub owner: usize,
    pub kind: DamageType,
    /// Units a frame: [`SHOT_STEP`], or the crossbow's [`BOLT_STEP`].
    pub step: f32,
}

pub const SHOT_STEP: f32 = 75.0;
/// The crossbow's bolt: an object at speed 250 (V combat5_cb2-4: 125 a frame).
pub const BOLT_STEP: f32 = 125.0;

impl Shot {
    pub fn fire(
        world: &CollisionWorld,
        owner: usize,
        muzzle: [f32; 3],
        angle: u16,
        kind: DamageType,
    ) -> Self {
        let (c, s) = cos_sin(angle);
        let dir = [c, 0.0, -s];
        Self {
            pos: muzzle,
            dir,
            wall: raycast(world, muzzle, dir, FLIGHT_RANGE),
            owner,
            kind,
            step: SHOT_STEP,
        }
    }

    /// A shot from `muzzle` toward `target` (the point the scope looks at: `s126+0xB8` when
    /// `s126+0x109` is set, C `func_15063FA0`).
    pub fn toward(
        world: &CollisionWorld,
        owner: usize,
        muzzle: [f32; 3],
        target: [f32; 3],
        kind: DamageType,
    ) -> Self {
        let d = sub(target, muzzle);
        let len = dot(d, d).sqrt();
        let dir = if len > 0.0 {
            d.map(|c| c / len)
        } else {
            [1.0, 0.0, 0.0]
        };
        Self {
            pos: muzzle,
            dir,
            wall: raycast(world, muzzle, dir, FLIGHT_RANGE),
            owner,
            kind,
            step: SHOT_STEP,
        }
    }

    /// The step after a sweep that hit no body; false once it reached the wall.
    pub fn advance(&mut self) -> bool {
        if self.wall <= self.step {
            return false;
        }
        self.pos = core::array::from_fn(|i| self.pos[i] + self.dir[i] * self.step);
        self.wall -= self.step;
        true
    }
}

/// Throw: y velocity 50, gravity 4, along the facing, from the hand (V e_gren row 805).
pub const THROW_Y_VEL: f32 = 50.0;
pub const THROW_SPEED: f32 = 41.0;
pub const COOK_HOLD: u32 = 60;
/// The cook timer (s126+0x19C, VIs): 240 on the pull's first frame, −2 on each later frame C-down
/// is still held (one frame late) before the throw; the grenade explodes in the hand at 0, and the
/// thrown one takes the timer as its fuse (obj+0x2E6, −2 a frame). C `func_15065A5C`,
/// `func_15073FA0`; V combat4_cook2 (every write), the fuses 236, 234, 146, 106, 66 and 40 of
/// e_gren, ctl_cd, combat4_cook (holds of 20 and 40), ctl_cdhold and ammo_cook, and combat4_dead2's
/// grenade going off in the hand 120 frames into the pull.
pub const COOK: u16 = 240;

/// The throw speed for a cook timer at the release (`func_15073FA0`, C): `40 + (240 − cook)/4`,
/// so 41 after a tap, 90 after 100 frames (V: 41, 83.5 and 90 on e_gren, ctl_cdhold, ammo_cook).
pub fn cooked_speed(cook: u16) -> f32 {
    f32::from(COOK - cook.min(COOK)) * 60.0 / 240.0 + 40.0
}

/// The speed after `hold` frames in the hold state, for C-down held through the pull (wave 3's
/// fit; [`cooked_speed`] is the game's rule).
pub fn throw_speed(hold: u32) -> f32 {
    if hold < COOK_HOLD {
        THROW_SPEED
    } else {
        (83.5 + 0.5 * (hold - COOK_HOLD) as f32).min(90.0)
    }
}
pub const GRENADE_GRAVITY: f32 = 4.0;
/// The hand at the release, in the thrower's frame (forward, side, up).
pub const HAND: [f32; 3] = [-0.982, -31.624, 55.794];

/// Item 0x3A, the grenade C-down pulls on scenes 0x1A and 0x3F instead of 0x26 (`func_1504CB98`
/// sets s126+0x19B = 5 there, C; V c3a, c3f): its pull (22 frames), hold and throw (19) clips; in
/// the hand from the pull (models 137 on 0x1A, 186 on 0x3F), it leaves it on the throw's first
/// frame, lobbed ([`Grenade::lob`]), its timer running on from the pull: it goes off 120 frames
/// after the pull wherever it is (V c3a 102 → 222, 502 → 622, 762 → 882).
pub const LOB: u8 = 0x3A;
pub const LOB_PULL: u16 = 0x1F9;
pub const LOB_HOLD: u16 = 0x1FA;
pub const LOB_THROW: u16 = 0x1FC;
pub const LOB_PULL_FRAMES: u32 = 22;
pub const LOB_THROW_FRAMES: u32 = 19;
/// Its hand at the release (forward, side, up): a squirrel's (class 1, V c3a's three throws) and
/// a Tedi's (class 3, V c3f); I: the other classes as the squirrel.
pub const LOB_HAND: [f32; 3] = [9.0, 27.4, 62.4];
pub const LOB_HAND_TEDI: [f32; 3] = [4.6, 27.4, 41.9];

/// Item 0x3A's hand for a thrower of `class`.
pub fn lob_hand(class: u8) -> [f32; 3] {
    match crate::characters::model(class) {
        Some(117 | 177) => LOB_HAND_TEDI,
        _ => LOB_HAND,
    }
}
/// A landing that would bounce less than this settles it (I: V bounces of 9.67 and up, rests
/// from 6.16 down).
const LOB_SETTLE: f32 = 8.0;

/// A thrown grenade (class 0x7E, `func_15103E40`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Grenade {
    pub pos: [f32; 3],
    pub y_vel: f32,
    pub speed: f32,
    pub angle: u16,
    /// 0 flying, 1 bouncing, 2 settled (no more bounces).
    pub state: u8,
    /// The floor triangle it landed on last frame: the next step follows its plane.
    pub floor: Option<u32>,
    pub owner: usize,
    /// The frame it explodes in.
    pub fuse: u32,
}

impl Grenade {
    /// Leaves the hand of a player at `pos` facing `facing` in frame `t` with the cook timer
    /// `cook` (VIs): speed [`cooked_speed`]; the fuse runs out `cook / 2` frames later and it goes
    /// off the frame after (V combat4_cook: 146 at 116, the blast at 190).
    pub fn release(owner: usize, pos: [f32; 3], facing: u16, cook: u16, t: u32) -> Self {
        Self {
            speed: cooked_speed(cook),
            fuse: t + u32::from(cook / 2) + 1,
            ..Self::throw(owner, pos, facing, 0, 0)
        }
    }

    /// Leaves the hand of a player at `pos` facing `facing` after `hold` frames of holding.
    pub fn throw(owner: usize, pos: [f32; 3], facing: u16, hold: u32, fuse: u32) -> Self {
        Self {
            pos: local_to_world(pos, facing, HAND),
            y_vel: THROW_Y_VEL,
            speed: throw_speed(hold),
            angle: facing,
            state: 0,
            floor: None,
            owner,
            fuse,
        }
    }

    /// Item 0x3A leaving the `hand` of a player at `pos` facing `facing` in frame `t` with the
    /// timer `cook` of the frame before (V c3a, c3f): at 30° with speed `((240 − cook)·0.875 +
    /// 40)/2` (45.375 after 30 frames' hold, 89.125 after 80), already a frame along; the timer
    /// runs out `cook / 2 − 1` frames on.
    pub fn lob(
        owner: usize,
        pos: [f32; 3],
        facing: u16,
        hand: [f32; 3],
        cook: u16,
        t: u32,
        world: &CollisionWorld,
    ) -> Self {
        let s = (f32::from(COOK - cook.min(COOK)) * 0.875 + 40.0) / 2.0;
        let mut g = Self {
            pos: local_to_world(pos, facing, hand),
            y_vel: 0.5 * s,
            speed: s * 3f32.sqrt() / 2.0,
            angle: facing,
            state: 0,
            floor: None,
            owner,
            fuse: (t + u32::from(cook / 2)).saturating_sub(1),
        };
        g.step_lob(world);
        g
    }

    /// One frame of item 0x3A's grenade (V c3a, c3f): as [`Self::step`]'s fall, landings and
    /// floor planes, but no drag: the speed halves on each landing, and a landing that would
    /// bounce under [`LOB_SETTLE`] settles it; settled, the speed halves each frame (it moves
    /// while 1 or more). A wall takes 0.7 of the speed and turns it back about the wall (V c3a
    /// 590: 77.19 → 23.16; I: the sweep's push as the wall's normal).
    pub fn step_lob(&mut self, world: &CollisionWorld) {
        let old = self.pos;
        let (c, s) = cos_sin(self.angle);
        let rise =
            (self.floor.take()).map_or(0.0, |t| rise_along(world.tris[t as usize].normal, c, s));
        let d = match self.speed >= 1.0 {
            true => 0.5 * self.speed / (1.0 + rise * rise).sqrt(),
            false => 0.0,
        };
        let y = old[1] + d * rise + 0.5 * self.y_vel - 0.275 * GRENADE_GRAVITY;
        let new = [old[0] + d * c, y, old[2] - d * s];
        let body = Body {
            half_width: 8,
            radius: 8,
            height: 16.0,
            center_y: 8.0,
        };
        let opts = Sweep {
            dd3: true,
            ..Sweep::default()
        };
        self.pos = world.sweep(old, new, &body, &opts, self.y_vel).pos;
        let (px, pz) = (self.pos[0] - new[0], self.pos[2] - new[2]);
        let push = (px * px + pz * pz).sqrt();
        let into = (c * px - s * pz) / push.max(f32::EPSILON);
        if push > 0.01 && into < 0.0 {
            let (nx, nz) = (px / push, pz / push);
            self.angle = atan2_u16(-(-s - 2.0 * into * nz), c - 2.0 * into * nx);
            self.speed *= 0.3;
        }
        match world.floor(self.pos[0], self.pos[2], self.pos[1], old[1], None) {
            Some(f) if self.pos[1] < f.y => {
                self.pos[1] = f.y;
                self.floor = Some(f.tri);
                self.speed *= 0.5;
                let bounce = 0.5 * self.y_vel.abs() + 2.0;
                if self.state == 2 || bounce < LOB_SETTLE {
                    (self.state, self.y_vel) = (2, -4.0);
                } else {
                    (self.state, self.y_vel) = (1, bounce);
                }
            }
            _ => self.y_vel -= GRENADE_GRAVITY,
        }
    }

    /// One frame (V e_gren: the flight and three bounces). In the air the speed drops 0.5 before
    /// the move, then 1.4 a frame once it has bounced; below speed 1 it does not move sideways
    /// (as players). Landing below the floor bounces with `0.5·|y_vel| + 2`; the first landing
    /// after the speed reached 0 settles it, and from then on it rests on the floor. The frame
    /// after a landing it moves along the floor's plane, as a grounded player does. Walls push it
    /// with the game's routine (I: its body is unread, 8 units here).
    pub fn step(&mut self, world: &CollisionWorld) {
        if self.state == 0 {
            self.speed -= 0.5;
        } else {
            self.speed = (self.speed - 1.4).max(0.0);
        }
        let old = self.pos;
        let (c, s) = cos_sin(self.angle);
        let rise = self
            .floor
            .take()
            .map_or(0.0, |t| rise_along(world.tris[t as usize].normal, c, s));
        let d = if self.speed >= 1.0 {
            0.5 * self.speed / (1.0 + rise * rise).sqrt()
        } else {
            0.0
        };
        let y = old[1] + d * rise + 0.5 * self.y_vel - 0.275 * GRENADE_GRAVITY;
        let new = [old[0] + d * c, y, old[2] - d * s];
        let body = Body {
            half_width: 8,
            radius: 8,
            height: 16.0,
            center_y: 8.0,
        };
        self.pos = world
            .sweep(
                old,
                new,
                &body,
                &Sweep {
                    dd3: true,
                    ..Sweep::default()
                },
                self.y_vel,
            )
            .pos;
        let floor = world.floor(self.pos[0], self.pos[2], self.pos[1], old[1], None);
        match floor {
            Some(f) if self.pos[1] < f.y => {
                self.pos[1] = f.y;
                self.floor = Some(f.tri);
                if self.state == 2 {
                    self.y_vel = -4.0;
                } else {
                    self.y_vel = 0.5 * self.y_vel.abs() + 2.0;
                    if self.state == 0 {
                        self.speed = (self.speed - 1.4).max(0.0);
                    }
                    self.state = if self.speed == 0.0 { 2 } else { 1 };
                }
            }
            _ => self.y_vel -= GRENADE_GRAVITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters;

    fn flat() -> CollisionWorld {
        let big = 10_000.0;
        CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
            // a wall 1000 east of the origin, facing west
            (
                [
                    [1000.0, -100.0, -500.0],
                    [1000.0, 400.0, -500.0],
                    [1000.0, 400.0, 500.0],
                ],
                5,
            ),
            (
                [
                    [1000.0, -100.0, -500.0],
                    [1000.0, 400.0, 500.0],
                    [1000.0, -100.0, 500.0],
                ],
                5,
            ),
        ])
    }

    /// The fuses the traces show at the release and the speeds the dumps show for them.
    #[test]
    fn cooked_grenades() {
        assert_eq!([236, 66, 40].map(cooked_speed), [41.0, 83.5, 90.0]);
        let g = Grenade::release(0, [0.0; 3], 0, 146, 116);
        assert_eq!((g.fuse, g.speed), (190, 63.5)); // combat4_cook: released at 116, blast at 190
    }

    #[test]
    fn throw_speeds() {
        assert_eq!(
            [1, 59, 60, 73, 200].map(throw_speed),
            [41.0, 41.0, 83.5, 90.0, 90.0]
        );
    }

    #[test]
    fn frames_and_rays() {
        // facing 0 is +x; side is +z
        assert_eq!(
            local_to_world([1.0, 2.0, 3.0], 0, [10.0, 5.0, 1.0]),
            [11.0, 3.0, 8.0]
        );
        let w = flat();
        assert_eq!(
            raycast(&w, [0.0, 50.0, 0.0], [1.0, 0.0, 0.0], 1500.0),
            1000.0
        );
        assert_eq!(
            raycast(&w, [0.0, 50.0, 0.0], [-1.0, 0.0, 0.0], 1500.0),
            1500.0
        );
        assert_eq!(
            raycast(&w, [0.0, 50.0, 0.0], [0.0, -1.0, 0.0], 1500.0),
            50.0
        );
        let b = Bullet::fire(&w, 0, [0.0, 50.0, 0.0], 0, crate::damage::TWIN_BULLET);
        assert_eq!((b.life, b.angle), (1000.0 / 300.0, 1));
    }

    /// combat4_ctlw_k7: life 6.148 at the first update, the wall reached in the 7th frame.
    #[test]
    fn rocket_flies_to_the_wall() {
        let w = flat();
        let mut r = Rocket::fire(&w, 0, [0.0, 50.0, 0.0], [1.0, 0.0, 0.0]);
        assert_eq!(r.life, 5.0);
        let frames = (0..10).take_while(|_| r.advance()).count();
        assert_eq!((frames, r.at(r.reach())), (4, [1000.0, 50.0, 0.0]));
        let mut s = Shot::fire(&w, 0, [0.0, 50.0, 0.0], 0, crate::damage::SHOT_BODY);
        assert_eq!((0..20).take_while(|_| s.advance()).count(), 13); // 13·75 = 975 < 1000
    }

    /// `func_151452C4`: the entry ahead, the exit from inside, nothing behind or beside.
    #[test]
    fn ray_sphere_as_the_game() {
        let x = [1.0, 0.0, 0.0];
        assert_eq!(ray_sphere([0.0; 3], x, [10.0, 0.0, 0.0], 2.0), Some(8.0));
        assert_eq!(
            ray_sphere([9.0, 0.0, 0.0], x, [10.0, 0.0, 0.0], 2.0),
            Some(3.0)
        );
        assert_eq!(ray_sphere([13.0, 0.0, 0.0], x, [10.0, 0.0, 0.0], 2.0), None);
        assert_eq!(ray_sphere([0.0; 3], x, [10.0, 3.0, 0.0], 2.0), None);
    }

    /// `func_1508114C`: the broad sphere first, then table order, a hit replacing the best only if
    /// nearer and of a part ≥ the best's; hidden joints skipped.
    #[test]
    fn parts_keep_the_table_order() {
        let sphere = |x: f32, joint, part| Sphere {
            centre: [x, 50.0, 0.0],
            radius: 5.0,
            joint,
            part,
        };
        let at = [100.0, 0.0, 0.0];
        let ray = |p: &BodyParts| p.hit([0.0, 50.0, 0.0], [1.0, 0.0, 0.0], at, [0.0; 3]);
        let mut p = BodyParts {
            broad: [60.0, 50.0],
            hidden: 0,
            spheres: vec![sphere(110.0, 12, HEAD), sphere(95.0, 0, 0)],
            origin: at,
        };
        assert_eq!(ray(&p), Some((105.0, HEAD))); // the nearer body sphere doesn't replace the head
        p.spheres.reverse();
        assert_eq!(ray(&p), Some((90.0, 0))); // nor the farther head the body
        p.hidden = 1;
        assert_eq!(ray(&p), Some((105.0, HEAD)));
        p.broad = [60.0, 150.0]; // the broad sphere missed: no hit at all
        assert_eq!(ray(&p), None);
    }

    /// combat4_pitch24 from the scope's eye 18.4 off player 2's axis, 142 ahead, 2.9 below its
    /// feet: level → head; 11.9° down → body; 5.1° and 23.8° down → nothing.
    #[test]
    fn parts() {
        let shot = |deg: f32| {
            let (s, c) = deg.to_radians().sin_cos();
            hits_parts(
                [0.0, 64.6, 0.0],
                [c, -s, 0.0],
                500.0,
                [142.0, 0.0, 18.4],
                false,
            )
            .map(|h| h.1)
        };
        assert_eq!(
            [0.0, 5.1, 11.9, 23.8].map(shot),
            [Some(true), None, Some(false), None]
        );
        let level = [1.0, 0.0, 0.0];
        assert_eq!(
            hits_parts([0.0, 64.6, 0.0], level, 500.0, [142.0, 0.0, 18.4], true),
            None
        );
    }

    /// combat4_w16: the rocket from (−118.26, −229.44, 100.55) along (−0.7573, 0, −0.6531) went
    /// off 119.05 along it, at player 2 standing at (−219.11, −290.90, 5.31).
    #[test]
    fn ellipsoid() {
        let (p, dir, body) = (
            [-118.26236, -229.4438, 100.55014],
            [-0.757_27, 0.0, -0.653_1],
            [-219.11353, -290.90234, 5.3149147],
        );
        let h = characters::body(3).hitbox;
        let t = hits_ellipsoid(p, dir, 200.0, body, h).unwrap();
        assert!((t - 119.05).abs() < 0.1, "{t}");
        assert_eq!(hits_ellipsoid(p, dir, 100.0, body, h), None);
        assert_eq!(hits_ellipsoid(p, [0.0, 1.0, 0.0], 500.0, body, h), None);
    }

    #[test]
    fn cylinder() {
        let dir = [1.0, 0.0, 0.0];
        let tedi = |from: [f32; 3], dir, len, at| {
            hits_body(from, dir, len, at, characters::body(3).hitbox, false)
        };
        assert_eq!(
            tedi([0.0, 40.0, 0.0], dir, 300.0, [146.0, 0.0, 20.0]),
            Some(146.0 - 276.0f32.sqrt())
        );
        assert_eq!(tedi([0.0, 40.0, 0.0], dir, 300.0, [146.0, 0.0, 37.7]), None); // e_w09's right gun
        assert_eq!(tedi([0.0, 40.0, 0.0], dir, 100.0, [146.0, 0.0, 0.0]), None); // out of reach
        assert_eq!(tedi([0.0, 140.0, 0.0], dir, 300.0, [146.0, 0.0, 0.0]), None); // over the head
        assert_eq!(
            tedi([0.0, 40.0, 0.0], [-1.0, 0.0, 0.0], 300.0, [146.0, 0.0, 0.0]),
            None
        ); // behind
        // The squirrel is wider and taller (radius 36, −20 to 108), and lower crouching (to 82).
        let squirrel = |from, crouched| {
            hits_body(
                from,
                dir,
                300.0,
                [146.0, 0.0, 30.0],
                characters::body(4).hitbox,
                crouched,
            )
        };
        assert_eq!(tedi([0.0, 40.0, 0.0], dir, 300.0, [146.0, 0.0, 30.0]), None);
        assert!(squirrel([0.0, 40.0, 0.0], false).is_some());
        assert!(squirrel([0.0, 100.0, 0.0], false).is_some());
        assert!(squirrel([0.0, 100.0, 0.0], true).is_none());
    }

    /// e_gren: the first airborne frames from the hand (lldb rows 805-807).
    #[test]
    fn grenade_flight() {
        let w = flat();
        let mut g = Grenade {
            pos: [0.0, 500.0, 0.0],
            y_vel: 50.0,
            speed: 41.0,
            angle: 0,
            state: 0,
            floor: None,
            owner: 0,
            fuse: 0,
        };
        g.step(&w);
        assert_eq!(
            (g.pos[0], g.pos[1], g.y_vel, g.speed),
            (20.25, 523.9, 46.0, 40.5)
        );
        // bounce: 0.5·|y_vel| + 2, and the 1.4 friction on the first landing
        let mut g = Grenade {
            pos: [0.0, 10.0, 0.0],
            y_vel: -54.0,
            speed: 28.0,
            ..g
        };
        g.step(&w);
        assert_eq!((g.pos[1], g.y_vel, g.speed, g.state), (0.0, 29.0, 26.1, 1));
        let mut g = Grenade {
            pos: [0.0, 1.0, 0.0],
            y_vel: -12.5,
            speed: 0.0,
            state: 1,
            ..g
        };
        g.step(&w);
        assert_eq!((g.y_vel, g.state), (8.25, 2));
    }

    /// Item 0x3A's lob against c3a's first throw (30 frames' hold: timer 182, the player 2 above
    /// the floor facing along x): a frame along at the release (19.648, 10.243 up), speed 39.296
    /// and y velocity 18.69 after it; landings 15, 26 and 32 frames on (the last settles it), the
    /// speed halving at each, at rest 436.56 ahead; the blast 90 frames after the release.
    #[test]
    fn a_lob_flies_bounces_and_rests() {
        let big = 10_000.0;
        let w = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ]);
        let hand = local_to_world([0.0, 2.0, 0.0], 0, LOB_HAND);
        let mut g = Grenade::lob(1, [0.0, 2.0, 0.0], 0, LOB_HAND, 182, 131, &w);
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(g.pos[0] - hand[0], 19.648) && near(g.pos[1] - hand[1], 10.244));
        assert!(near(g.speed, 39.296) && near(g.y_vel, 18.688), "{g:?}");
        assert_eq!((g.fuse, g.owner), (221, 1));
        let start = g.pos[0];
        let mut landings = Vec::new();
        for k in 1..60 {
            let v = g.speed;
            g.step_lob(&w);
            if g.speed < v && g.state != 2 || g.state == 2 && landings.len() < 3 {
                landings.push((k, g.state));
            }
        }
        assert_eq!(landings, [(15, 1), (26, 1), (32, 2)]);
        assert!(
            (g.pos[0] - start - 436.56).abs() < 0.1 && g.pos[1] == 0.0,
            "{g:?}"
        );
    }
}
