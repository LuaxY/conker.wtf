//! The world effects the original draws with its sprites and its bank-09 models
//! (`../lab/research/mp-original/effects-g4.md` §0-§14, `effects-g5-scratch/NOTES.md`; the pack's
//! `hud/*.png` and `weapons/deathmatch/effects.gltf`; oracle runs twin, rocket and flame on 0x2D,
//! slot 0 at (−2247, −521, 6015) facing 16896). Tags: V the oracle's RAM and display lists, C its
//! code, I inferred.
//!
//! - A 2D sprite ([`Particle`], `func_15130280`/`func_151303EC`, C) is a camera-facing square of
//!   [`WIDTH`] × its size, or a quad lying on a surface (a ripple, a scorch), unlit and
//!   alpha-blended, depth-tested without writing depth. Its life counts VIs, 2 a frame.
//! - A 3D sprite ([`Streak`]) is a camera-facing quad between two points with an alpha at each end
//!   (the guns' 3D flash, the tracers, the fuse's sparks, the flame's tongues).
//! - A model piece ([`Chunk`]) is one of `effects.gltf`'s scenes thrown, spun and faded (the
//!   blasts' ring, the explosive death's sparks, Beach's flak, Race's death).
//!
//! The triggers:
//! - a player's shot count going up ([`Combat::shots`]; the local player's predicted one): the
//!   guns' flashes ([`flash`], [`rocket_flash`]), the twin guns' 3D flash ([`flash_3d`]), the other
//!   bullet guns' star ([`star`]), the shotgun's spray ([`spray`]); a tank's flashes
//!   ([`tank_flash`]) and its scope's bullet.
//! - the match's lists: the bullets' tracers ([`tracer`], per gun [`tracer_back`]); the rocket's glow, trail and streak; the
//!   shells' glow; where a rocket, grenade or shell leaves them, the blast ([`blast`], [`ring`]),
//!   with the scorch ([`scorch`]) and the smoke left ([`smoke`]) where a rocket or shell struck a
//!   face; item 0x3A's fuse sparks ([`fuse`]) in flight. Not across a match's end or restart
//!   ([`live`]).
//! - a player's clip: the flame's puffs and tongues ([`puff`], [`tongue`]), the burning flames
//!   ([`flame`]), the explosive death's sparks ([`sparks`]), a swimmer's ripples ([`ripple`]; its
//!   [`drops`] not drawn: the pack's model is an eyeball), item 0x3A's fuse in the hand; a racer's death ([`race_blast`]) and its
//!   board's trail ([`trail_mesh`]).
//! - War: Beach: the flak ([`flak`]) and the bunkers' tracers ([`bunker_tracer`]).
//!
//! I: not drawn, the blasts' light, the bullets' impact sparks, the tanks' exhaust and the cone's
//! smoke (their recipes not read), Beach's real fire (its spawner not found), Heist cs3's bubbles'
//! sway, Colors cs5's clods.

use std::collections::{HashMap, VecDeque};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use conker_anim::combat::{
    BLOWN_UP, BURN_END, BURNING, FLAME_FIRE, FLAME_HIT, GRENADE_HOLD, GRENADE_PULL,
};
use conker_anim::id::{STROKE, STROKE_DEEP};
use conker_assets::{EFFECTS, PACK_WEAPONS, effects_gltf};
use conker_rules::Rng;
use conker_rules::net::Fighting;
use conker_rules::tank::{SCOPE, SCOPE_EYE, SHELL_SPEED};
use conker_rules::weapon::{Attack, BULLET_SPEED, weapon};
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::cos_sin;
use conker_sim::net::{Collision, Combat, MatchView};
use lightyear::prelude::{LocalTimeline, Predicted};

use crate::Anim;
use crate::held::Lob;
use crate::projectiles::FLAME;
use crate::render::Model;

/// A 2D sprite's world width per unit of its size (I: the texrect's half-size in pixels is
/// size / w × 16.949 × the viewport's height / 216; at the remake's 50° vertical FOV).
pub const WIDTH: f32 = 0.1464;

/// The textures (`hud/`): `race_exhaust_0..3` (kinds 0x60-0x63: 0x923-0x926), `explosion` (0x27:
/// 0x78A), `puff` (0x28: 0xBA6), `fire` (0x6C: 0x71F), `shell` (0x1042), `smoke_grey` (0x16/0x29:
/// 0x721), `scorch` (0x4F: 0x4E5), `ripple_00..09` (0x38), `flame_a0..4`/`flame_b0..4` (0x13,
/// 0x14), `bubble_particle` (0x4B3), `gun_flash_3d` (0x5F: 0x3CF), `tracer` (0x41: 0x11FE),
/// `fuse_spark` (0x42: 0x1041), `flame_tongue` (0xBA7), `race_trail` (0xB39).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    Exhaust(usize),
    Explosion,
    Puff,
    Fire,
    Shell,
    SmokeGrey,
    Scorch,
    Ripple(usize),
    /// The set (a or b) and its frame.
    Flame(usize, usize),
    Bubble,
    GunFlash,
    Tracer,
    FuseSpark,
    Tongue,
    Trail,
}

impl Tex {
    const ONE: [Tex; 12] = [
        Tex::Explosion,
        Tex::Puff,
        Tex::Fire,
        Tex::Shell,
        Tex::SmokeGrey,
        Tex::Scorch,
        Tex::Bubble,
        Tex::GunFlash,
        Tex::Tracer,
        Tex::FuseSpark,
        Tex::Tongue,
        Tex::Trail,
    ];

    fn all() -> impl Iterator<Item = Tex> {
        (0..4)
            .map(Tex::Exhaust)
            .chain(Tex::ONE)
            .chain((0..10).map(Tex::Ripple))
            .chain((0..10).map(|k| Tex::Flame(k / 5, k % 5)))
    }

    fn path(self) -> String {
        let name = match self {
            Tex::Exhaust(k) => return format!("hud/race_exhaust_{k}.png"),
            Tex::Ripple(k) => return format!("hud/ripple_{k:02}.png"),
            Tex::Flame(set, k) => return format!("hud/flame_{}{k}.png", ["a", "b"][set]),
            Tex::Explosion => "explosion",
            Tex::Puff => "puff",
            Tex::Fire => "fire",
            Tex::Shell => "shell",
            Tex::SmokeGrey => "smoke_grey",
            Tex::Scorch => "scorch",
            Tex::Bubble => "bubble_particle",
            Tex::GunFlash => "gun_flash_3d",
            Tex::Tracer => "tracer",
            Tex::FuseSpark => "fuse_spark",
            Tex::Tongue => "flame_tongue",
            Tex::Trail => "race_trail",
        };
        format!("hud/{name}.png")
    }

    /// The texture `age` VIs on at `rate` frames a VI: a flame loops its 5, a ripple plays its 10
    /// once.
    pub fn at(self, age: f32, rate: f32) -> Tex {
        let k = (age * rate) as usize;
        match self {
            Tex::Flame(set, _) => Tex::Flame(set, k % 5),
            Tex::Ripple(_) => Tex::Ripple(k.min(9)),
            t => t,
        }
    }
}

/// One of the original's 2D sprites: game units, frames, and its life in VIs.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub tex: Tex,
    pub pos: Vec3,
    /// Its motion a frame, the change of its y a frame, and what its motion keeps a frame.
    pub vel: Vec3,
    pub fall: f32,
    pub drag: f32,
    /// Where it is drawn from its position (+0x4C).
    pub offset: Vec3,
    pub size: f32,
    pub life: f32,
    /// Its alpha (of 255), and below `fade[0]` VIs at most life × `fade[1]`.
    pub alpha: f32,
    pub fade: [f32; 2],
    /// Its colour class (prim) and the texel's weight, life × `[1]` / 255 below `[0]` VIs (env).
    pub tint: Option<([f32; 3], [f32; 2])>,
    /// S and T flipped.
    pub flip: [bool; 2],
    /// Below `grow[0]` VIs its size is × `grow[1]` a VI.
    pub grow: [f32; 2],
    /// The texel's tint.
    pub colour: [f32; 3],
    /// Its height over its width (the flames' 16 × 32 texels: 2).
    pub tall: f32,
    /// VIs since it was made, and its texture's frames a VI ([`Tex::at`]; 0: one texture).
    pub age: f32,
    pub rate: f32,
    /// Lying on a surface with this turn (a ripple, a scorch), `size` then its world width; else
    /// facing the camera.
    pub flat: Option<Quat>,
    /// At this entity's place (a burning player's joint).
    pub follow: Option<Entity>,
    /// The most its y motion reaches a frame (Heist cs3's bubbles).
    pub top: f32,
}

impl Particle {
    fn new(tex: Tex, pos: Vec3, size: f32, life: f32, alpha: f32, fade: [f32; 2]) -> Self {
        Self {
            tex,
            pos,
            vel: Vec3::ZERO,
            fall: 0.0,
            drag: 1.0,
            offset: Vec3::ZERO,
            size,
            life,
            alpha,
            fade,
            tint: None,
            flip: [false; 2],
            grow: [0.0, 1.0],
            colour: [1.0; 3],
            tall: 1.0,
            age: 0.0,
            rate: 0.0,
            flat: None,
            follow: None,
            top: f32::INFINITY,
        }
    }

    /// Its alpha (of 255) now.
    pub fn alpha(&self) -> f32 {
        match self.life < self.fade[0] {
            true => self.alpha.min(self.life * self.fade[1]),
            false => self.alpha,
        }
    }

    /// Its colour: (TEXEL0 − PRIM) × ENV + PRIM (C), here texel × (env + prim × (1 − env)).
    pub fn rgb(&self) -> [f32; 3] {
        let Some((prim, [start, rate])) = self.tint else {
            return self.colour;
        };
        let env = if self.life < start {
            (self.life * rate / 255.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // ponytail: exact for white texels; the RDP's sum needs a custom material.
        prim.map(|p| env + p * (1.0 - env))
    }

    /// `frames` frames on (`func_151303EC`, V rocket DLs 125-130): 2 VIs of life a frame, the
    /// y change, the move, then the drag. False once its life is out.
    pub fn step(&mut self, frames: f32) -> bool {
        self.life -= 2.0 * frames;
        self.age += 2.0 * frames;
        self.vel.y = (self.vel.y + self.fall * frames).min(self.top);
        self.pos += self.vel * frames;
        self.vel *= self.drag.powf(frames);
        if self.life < self.grow[0] {
            self.size *= self.grow[1].powf(2.0 * frames);
        }
        self.life > 0.0
    }
}

/// A 3D sprite: a quad facing the eye from `ends[0]` to `ends[1]` (its head when it moves), S
/// `u[k]` and vertex alpha `alpha[k]` (of 255) at each, `half` its half-width in world units.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Streak {
    pub tex: Tex,
    pub ends: [Vec3; 2],
    pub u: [f32; 2],
    pub alpha: [f32; 2],
    pub half: f32,
    /// Both ends' motion a frame and its y change a frame.
    pub vel: Vec3,
    pub fall: f32,
    /// Its life in VIs, and below `fade[0]` each alpha at most life × `fade[1]`.
    pub life: f32,
    pub fade: [f32; 2],
    /// T flipped.
    pub flip: bool,
    /// If not 0, `ends[0]` this many frames of its motion behind its head.
    pub trail: f32,
}

impl Streak {
    fn new(tex: Tex, ends: [Vec3; 2], half: f32, alpha: [f32; 2], life: f32) -> Self {
        Self {
            tex,
            ends,
            u: [0.0, 1.0],
            alpha,
            half,
            vel: Vec3::ZERO,
            fall: 0.0,
            life,
            fade: [0.0; 2],
            flip: false,
            trail: 0.0,
        }
    }

    /// `frames` frames on; false once its life is out.
    pub fn step(&mut self, frames: f32) -> bool {
        self.life -= 2.0 * frames;
        self.vel.y += self.fall * frames;
        self.ends = self.ends.map(|e| e + self.vel * frames);
        if self.trail > 0.0 {
            self.ends[0] = self.ends[1] - self.vel * self.trail;
        }
        self.life > 0.0
    }

    /// The vertex alphas now.
    pub fn alphas(&self) -> [f32; 2] {
        let fading = self.life < self.fade[0];
        (self.alpha).map(|a| {
            if fading {
                a.min(self.life * self.fade[1])
            } else {
                a
            }
        })
    }
}

/// How a [`Chunk`] grows and fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    /// Its scale, its alpha (and the fade) until its life is out.
    Fly,
    /// The ring (`func_151C04F8`, Race's smoke shell `func_150D6388`, C): `t` frames on, scale
    /// k·√t, alpha ⌊a − a·t²/l²⌋; gone when t > l.
    Shell { k: f32, l: f32, a: f32 },
    /// Race's fire shell (`func_150D6434`, C): φ = t·π/l, scale s·sin φ, alpha ⌊⌊a⌋·sin φ⌋.
    Fire { l: f32, s: f32, a: f32 },
}

/// One of `effects.gltf`'s scenes ([`EFFECTS`]) thrown: y += vy + g/2, vy += g a frame (no
/// collision), its angles turning by `spin` (radians a frame about x, y, z).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Chunk {
    pub scene: usize,
    pub pos: Vec3,
    pub vel: Vec3,
    pub g: f32,
    pub turn: Quat,
    pub spin: Vec3,
    pub scale: f32,
    pub law: Law,
    pub alpha: f32,
    pub fade: [f32; 2],
    pub life: f32,
    /// Frames since it was made.
    pub t: f32,
    /// Drawn blended even when opaque (Beach's flak).
    pub blend: bool,
}

impl Chunk {
    fn new(scene: usize, pos: Vec3, scale: f32, life: f32, alpha: f32, fade: [f32; 2]) -> Self {
        Self {
            scene,
            pos,
            vel: Vec3::ZERO,
            g: 0.0,
            turn: Quat::IDENTITY,
            spin: Vec3::ZERO,
            scale,
            law: Law::Fly,
            alpha,
            fade,
            life,
            t: 0.0,
            blend: false,
        }
    }

    /// `frames` frames on: its scale and alpha (of 255), none once gone.
    pub fn step(&mut self, frames: f32) -> Option<(f32, f32)> {
        self.t += frames;
        self.life -= 2.0 * frames;
        self.pos += self.vel * frames + Vec3::Y * (self.g * frames * frames / 2.0);
        self.vel.y += self.g * frames;
        let [x, y, z] = (self.spin * frames).to_array();
        self.turn *= Quat::from_euler(EulerRot::XYZ, x, y, z);
        let t = self.t;
        match self.law {
            Law::Fly => (self.life > 0.0).then(|| {
                let fading = self.life < self.fade[0];
                let alpha = if fading {
                    self.alpha.min(self.life * self.fade[1])
                } else {
                    self.alpha
                };
                (self.scale, alpha)
            }),
            Law::Shell { k, l, a } => {
                (t <= l).then(|| (k * t.sqrt(), (a - a * t * t / (l * l)).floor()))
            }
            Law::Fire { l, s, a } => (t < l).then(|| {
                let phi = t * PI / l;
                (s * phi.sin(), (a.floor() * phi.sin()).floor())
            }),
        }
    }
}

/// `effects.gltf`'s scenes ([`EFFECTS`]).
const RING: usize = 0;
const SPARK: usize = 1;
const FLAK: usize = 2;
const DROP: usize = 3;
const FIRE: usize = 4;

fn flips(rng: &mut Rng) -> [bool; 2] {
    [rng.next_u32() & 1 == 1, rng.next_u32() & 1 == 1]
}

fn exhaust(rng: &mut Rng) -> Tex {
    Tex::Exhaust((rng.next_u32() & 3) as usize)
}

/// A random byte as an angle (256 a turn).
fn byte_turn(rng: &mut Rng) -> f32 {
    (rng.next_u32() & 0xFF) as f32 / 256.0 * TAU
}

/// The unit vector at `yaw` (from +z toward +x) and `pitch` up.
fn polar(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    )
}

/// Random angles on each axis (rand × 360°).
fn any_turn(rng: &mut Rng) -> Quat {
    let mut r = || rng.next_f32() * TAU;
    Quat::from_euler(EulerRot::XYZ, r(), r(), r())
}

/// A spin about x and z of rand × `span` − `span` / 2 degrees a frame each.
fn spin_xz(rng: &mut Rng, span: f32) -> Vec3 {
    let mut r = || (rng.next_f32() * span - span / 2.0).to_radians();
    Vec3::new(r(), 0.0, r())
}

/// An emitter's count a frame: its carry goes up by `add`, one each time it passes 1.
pub fn emit(carry: &mut f32, add: f32) -> u32 {
    *carry += add;
    let n = carry.floor().max(0.0);
    *carry -= n;
    n as u32
}

/// A gun's flash (`func_15103254` from `func_15102B38`, C; V twin DLs 104-110: kinds 0x60 and
/// 0x63, sizes 365-518, lives 7, 5, 3, 1 with alpha 255 then 85): a random one of the four
/// blobs, size rand × 270 + 359, life (rand & 3) + 7 VIs, alpha 255, below 3 VIs life × 85.
pub fn flash(rng: &mut Rng, at: Vec3) -> Particle {
    let life = (rng.next_u32() & 3) as f32 + 7.0;
    let size = rng.next_f32() * 270.0 + 359.0;
    let tex = exhaust(rng);
    let flip = flips(rng);
    Particle {
        flip,
        ..Particle::new(tex, at, size, life, 255.0, [3.0, 85.0])
    }
}

/// The twin guns' flashes in the shooter's frame (forward, side, up), barrel 0 then 1 (V twin
/// DLs 104/105: at (−2228.99, −458.66, 5974.02) and (−2269.32, −476.56, 5966.6), Z held).
pub const TWIN_FLASHES: [[f32; 3]; 2] = [[40.05, 20.0, 62.34], [49.44, -19.92, 44.44]];

/// The joints holding the twin guns, barrel 0 then 1 (the right hand, the left).
pub const TWIN_JOINTS: [usize; 2] = [9, 4];

/// The rocket launcher's two flashes, ahead of and behind the tube (V rocket DL 120: kinds 0x61
/// and 0x60 at (−2237.35, −461.59, 5971.62) and (−2231.96, −452.61, 6058.5)).
pub const ROCKET_FLASHES: [[f32; 3]; 2] = [[42.85, 11.76, 59.41], [-44.19, 12.89, 68.39]];

/// The rocket launcher's flash (`func_151C0698`, C; V rocket DLs 120-122: sizes 637 and 744,
/// alphas 249 and 211): size rand × 204 + 601, life rand % 3 + 9 VIs, alpha rand % 56 + 200.
pub fn rocket_flash(rng: &mut Rng, at: Vec3) -> Particle {
    let life = (rng.next_u32() % 3) as f32 + 9.0;
    let alpha = (rng.next_u32() % 56) as f32 + 200.0;
    let size = rng.next_f32() * 204.0 + 601.0;
    let tex = exhaust(rng);
    let flip = flips(rng);
    Particle {
        flip,
        ..Particle::new(tex, at, size, life, alpha, [3.0, 85.0])
    }
}

/// Where shot `n` (1 the first) of `weapon`'s flashes show in the shooter's frame: the twin guns'
/// barrels in turn (V), else the bullet's muzzle (I); the rocket launcher's pair.
pub fn flash_points(weapon_id: u8, n: u16) -> Vec<[f32; 3]> {
    let Some(w) = weapon(weapon_id) else {
        return Vec::new();
    };
    let i = usize::from(n.saturating_sub(1));
    match w.attack {
        _ if weapon_id == 0x09 => vec![TWIN_FLASHES[i % 2]],
        Attack::Bullets { muzzles, .. } => vec![muzzles[i % muzzles.len()]],
        Attack::Spread { muzzles, .. } => vec![muzzles[0]],
        Attack::Rocket { .. } => ROCKET_FLASHES.to_vec(),
        _ => Vec::new(),
    }
}

/// The twin guns' 3D flash (kind 0x5F, `gun_flash_3d.png`; C the recipe, V twin2 DLs 104-112:
/// 29.6-65.3 long, 29.4-34.3 wide): from the 2D flash's point along the barrel `dir` (the hand
/// joint's −y), 41.5 (83 joint units at the hand's 0.5) × (rand × 1.001 + 0.604) long, half-width
/// rand × 5.9 + 14.1 world units, S 31 at the muzzle and 0 at the far end, T flipped at random,
/// alpha 255, life (rand & 3) + 6 VIs.
pub fn flash_3d(rng: &mut Rng, at: Vec3, dir: Vec3) -> Streak {
    let life = (rng.next_u32() & 3) as f32 + 6.0;
    let len = 41.5 * (rng.next_f32() * 1.001 + 0.604);
    let half = rng.next_f32() * 5.9 + 14.1;
    Streak {
        u: [1.0, 0.0],
        flip: rng.next_u32() & 1 == 1,
        ..Streak::new(Tex::GunFlash, [at, at + dir * len], half, [255.0; 2], life)
    }
}

/// The blobs in the shotgun's spray (V shotgun2: 10 in each of two shots).
pub const SPRAY: usize = 10;

/// The shotgun's spray (V shotgun2 DLs 99-101 and 219-221, two shots, each blob born at the 2D
/// flash's point and moved once by the first list; I the laws, fitted to those 20): [`SPRAY`]
/// blobs of the four along the barrel `dir` at rand × 19 + 20 a frame (V 20.5-38.7), plus up to 8
/// each way across and up (V ±7.8); y change −(rand × 0.55 + 0.04) a frame (V −0.04..−0.58);
/// size rand × 141 + 80 (V 80-221), life rand % 12 + 10 VIs (V 10-21), alpha rand % 100 + 155
/// (V 156-254), below 7 VIs life × 36 (V); random flips.
pub fn spray(rng: &mut Rng, at: Vec3, dir: Vec3) -> Vec<Particle> {
    let side = dir.cross(Vec3::Y).normalize_or_zero();
    let up = side.cross(dir).normalize_or_zero();
    (0..SPRAY)
        .map(|_| {
            let life = (rng.next_u32() % 12) as f32 + 10.0;
            let alpha = (rng.next_u32() % 100) as f32 + 155.0;
            let size = rng.next_f32() * 141.0 + 80.0;
            let speed = rng.next_f32() * 19.0 + 20.0;
            let across = side * (rng.next_f32() * 16.0 - 8.0) + up * (rng.next_f32() * 16.0 - 8.0);
            let fall = -(rng.next_f32() * 0.55 + 0.04);
            let tex = exhaust(rng);
            let flip = flips(rng);
            Particle {
                vel: dir * speed + across,
                fall,
                flip,
                ..Particle::new(tex, at, size, life, alpha, [7.0, 36.0])
            }
        })
        .collect()
}

/// The star's rays' angle off the barrel (V rifle DL 100: forward 0.26-0.32 of each).
pub const STAR_TILT: f32 = 73.0;

/// The other bullet guns' 3D flash (V rifle DL 100, weapon 0x39; I for the others): six rays 60°
/// apart around the barrel `dir`, [`STAR_TILT`]° off it, 39-51 long, half-width 5.7-11.7, alpha
/// 111-237 each, gone two frames on.
pub fn star(rng: &mut Rng, at: Vec3, dir: Vec3) -> Vec<Streak> {
    let side = dir.any_orthonormal_vector();
    let other = dir.cross(side);
    let start = rng.next_f32() * TAU;
    let (s, c) = STAR_TILT.to_radians().sin_cos();
    (0..6)
        .map(|i| {
            let a = start + i as f32 * TAU / 6.0;
            let ray = dir * c + (side * a.cos() + other * a.sin()) * s;
            let len = rng.next_f32() * 12.0 + 39.0;
            let half = rng.next_f32() * 6.0 + 5.7;
            let alpha = (rng.next_u32() % 127) as f32 + 111.0;
            Streak {
                u: [1.0, 0.0],
                ..Streak::new(Tex::GunFlash, [at, at + ray * len], half, [alpha; 2], 3.0)
            }
        })
        .collect()
}

/// A tracer's tail behind its bullet, in frames of its motion: on the first list it is in, then
/// on (V twin2: 183-190 long, then 357-371, at 300 a frame).
pub const TRACER_BACK: [f32; 2] = [0.62, 1.22];

/// [`TRACER_BACK`] for a bullet of `weapon` (V laneD pistol, gun23: their first list at 300 from
/// the muzzle, 280 and 302 long, so from the muzzle; at 900, 546 and 589); the twin guns' for the
/// others (I: the tommy gun's and the mounted gun's not seen).
pub fn tracer_back(weapon: u8) -> [f32; 2] {
    match weapon {
        0x39 => [0.93, 1.82],
        0x23 => [1.01, 1.96],
        _ => TRACER_BACK,
    }
}

/// A bullet's tracer's look (kind 0x41, `tracer.png`; V twin2: head alphas 124-238, rifle 222;
/// widths 56-63.6): its head's alpha and its half-width.
pub fn tracer_look(rng: &mut Rng) -> [f32; 2] {
    [
        (rng.next_u32() % 115) as f32 + 124.0,
        rng.next_f32() * 4.0 + 28.0,
    ]
}

/// A bullet's tracer (V twin2, rifle): a quad from the bullet (S 0, its alpha) back along its
/// motion `vel` a frame to a tail (S 63, alpha 0), `back` frames of it behind ([`tracer_back`]);
/// moving with it between ticks.
pub fn tracer(head: Vec3, vel: Vec3, back: f32, [alpha, half]: [f32; 2]) -> Streak {
    Streak {
        u: [1.0, 0.0],
        vel,
        ..Streak::new(
            Tex::Tracer,
            [head - vel * back, head],
            half,
            [0.0, alpha],
            f32::INFINITY,
        )
    }
}

/// Tank's scope bullet (`tank::SCOPE_EYE`, the bullet pool's at [`BULLET_SPEED`] a frame, I its
/// tracer the bullets'): from `from` along `dir` (level), `reach` to the wall it meets.
pub fn scope_tracer(rng: &mut Rng, from: Vec3, dir: Vec3, reach: f32) -> Streak {
    let vel = dir * BULLET_SPEED;
    Streak {
        trail: TRACER_BACK[1],
        life: 2.0 * reach / BULLET_SPEED,
        ..tracer(from, vel, TRACER_BACK[0], tracer_look(rng))
    }
}

/// Tank's muzzle joint (`tank.muzzle`) and its 3D flash's start and end in its frame.
pub const TANK_MUZZLE: usize = 4;
pub const TANK_FLASH: [[f32; 3]; 2] = [[0.0, 12.0, 120.0], [0.0, 27.0, 215.0]];

/// A tank's shot (C `g5-pack-scratch/tank/NOTES.md`; V k1: a quad 228 long and 88 wide): the 3D
/// flash from `start` to start + (end − start) × (rand × 2.512 + 3.59), half-width rand × 30 + 40
/// world units, life rand % 5 + 5 VIs, alpha rand % 56 + 200; and at its start a 2D flash of the
/// four blobs, size rand × 510 + 1296, life one more, the same alpha, below 3 VIs life × 85.
pub fn tank_flash(rng: &mut Rng, start: Vec3, end: Vec3) -> (Streak, Particle) {
    let k = rng.next_f32() * 2.512 + 3.59;
    let half = rng.next_f32() * 30.0 + 40.0;
    let life = (rng.next_u32() % 5) as f32 + 5.0;
    let alpha = (rng.next_u32() % 56) as f32 + 200.0;
    let streak = Streak {
        u: [1.0, 0.0],
        ..Streak::new(
            Tex::GunFlash,
            [start, start + (end - start) * k],
            half,
            [alpha; 2],
            life,
        )
    };
    let size = rng.next_f32() * 510.0 + 1296.0;
    let tex = exhaust(rng);
    let flip = flips(rng);
    let flash = Particle {
        flip,
        ..Particle::new(tex, start, size, life + 1.0, alpha, [3.0, 85.0])
    };
    (streak, flash)
}

/// A shell's glow (C `tank/NOTES.md`): one of the four blobs, size rand × 800 + 1700, alpha 255,
/// on the shell while it flies.
pub fn shell_glow(rng: &mut Rng, at: Vec3) -> Particle {
    let tex = exhaust(rng);
    let size = rng.next_f32() * 800.0 + 1700.0;
    let flip = flips(rng);
    Particle {
        flip,
        ..Particle::new(tex, at, size, f32::INFINITY, 255.0, [0.0; 2])
    }
}

/// The rocket's glow (`func_151B8908`, C; V rocket DLs 120-124: kind 0x63, size 1289.8, at the
/// rocket): a random one of the four blobs, size rand × 500 + 900, alpha 255, while it flies.
pub fn glow(rng: &mut Rng, at: Vec3) -> Particle {
    let tex = Tex::Exhaust((rng.next_u32() & 3) as usize);
    let size = rng.next_f32() * 500.0 + 900.0;
    let flip = flips(rng);
    Particle {
        flip,
        ..Particle::new(tex, at, size, f32::INFINITY, 255.0, [0.0; 2])
    }
}

/// The trail's spacing along the rocket's path (I: V 2, 2, 2, 3, 2 a frame at 200 a frame).
pub const TRAIL_STEP: f32 = 90.0;

/// A puff of the rocket's trail (`func_151B86F4`, C; V rocket DLs 121-125: lives 21-40, sizes
/// 77-129, z motion −120 to −148 behind a rocket at −200): explosion.png at the rocket, moving
/// with it at rand × 0.157 + 0.604 of its motion plus a random push up to 9.45, y change
/// (rand × 157 − 151) / 1000 a frame, size rand × 55 + 75, life rand % 26 + 25 VIs, alpha
/// rand % 101 + 100, below 10 VIs life × 25; it darkens below 25 VIs (prim black, env life × 10).
pub fn trail(rng: &mut Rng, at: Vec3, vel: Vec3) -> Particle {
    let yaw = (rng.next_u32() & 0xFF) as f32 / 256.0 * std::f32::consts::TAU;
    let pitch = ((rng.next_u32() % 65) as f32 - 32.0) / 256.0 * std::f32::consts::TAU;
    let push = rng.next_f32() * 9.45;
    let dir = Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    );
    let share = (rng.next_f32() * 157.0 + 604.0) / 1000.0;
    let fall = (rng.next_f32() * 157.0 - 151.0) / 1000.0;
    let size = rng.next_f32() * 55.0 + 75.0;
    let life = (rng.next_u32() % 26) as f32 + 25.0;
    let alpha = (rng.next_u32() % 101) as f32 + 100.0;
    Particle {
        vel: dir * push + vel * share,
        fall,
        tint: Some(([0.0; 3], [25.0, 10.0])),
        flip: flips(rng),
        ..Particle::new(Tex::Explosion, at, size, life, alpha, [10.0, 25.0])
    }
}

/// The blast's colour classes (V RAM 0x8008F8E8: black, dark red, white, grey).
const CLASSES: [[u8; 3]; 4] = [[0, 0, 0], [89, 0, 0], [255, 255, 255], [128, 128, 128]];

/// A blast's smoke (`func_151BFC40`'s numbers through `func_1514F8F8` and `func_151A2C24`, C; V
/// rocket DL 125: 3 columns of 8-10 explosion.png puffs, the first 54-64 a frame up and the
/// smallest, each next one slower and bigger): 3 + rand % 5 columns along `up` (the struck
/// face's normal toward the shooter) spread by up to 494 across, × rand × 0.0252 + 0.0609; each
/// of n = 7 + rand % 4 puffs, the i-th at (1 − i/n) of that and of size lo + span × i/n (lo
/// rand × 53 + 45, span rand × 414 + 203 a column), drawn up to 15 off, life rand % 16 + 37
/// VIs, alpha rand % 101 + 100 (below 12 VIs life × 21), y change rand × 0.391 − 0.398, 0.7744
/// of its motion kept a frame (0.88 a VI), toward a random class below 20 VIs (env life × 12).
pub fn blast(rng: &mut Rng, at: Vec3, up: Vec3) -> Vec<Particle> {
    let up = up.normalize_or(Vec3::Y);
    let side = up.any_orthonormal_vector();
    let other = up.cross(side);
    let mut puffs = Vec::new();
    for _ in 0..3 + rng.next_u32() % 5 {
        let k = rng.next_f32() * 0.0252 + 0.0609;
        let turn = (rng.next_u32() & 0xFF) as f32 / 256.0 * std::f32::consts::TAU;
        let across = rng.next_f32() * 494.0;
        let vel = (up * 1000.0 + (side * turn.cos() + other * turn.sin()) * across) * k;
        let lo = rng.next_f32() * 53.0 + 45.0;
        let span = rng.next_f32() * 414.0 + 203.0;
        let n = 7 + rng.next_u32() % 4;
        for i in 0..n {
            let f = i as f32 / n as f32;
            let life = (rng.next_u32() % 16) as f32 + 37.0;
            let (yaw, pitch) = (rng.next_u32() & 0xFF, rng.next_u32() & 0xFF);
            let [yaw, pitch] = [yaw, pitch].map(|a| a as f32 / 256.0 * std::f32::consts::TAU);
            let offset = Vec3::new(
                pitch.cos() * yaw.sin(),
                pitch.sin(),
                pitch.cos() * yaw.cos(),
            ) * rng.next_f32()
                * 15.0;
            let class = CLASSES[(rng.next_u32() & 3) as usize].map(|c| f32::from(c) / 255.0);
            let fall = rng.next_f32() * 0.391 - 0.398;
            let alpha = (rng.next_u32() % 101) as f32 + 100.0;
            puffs.push(Particle {
                vel: vel * (1.0 - f),
                fall,
                drag: 0.7744,
                offset,
                tint: Some((class, [20.0, 12.0])),
                flip: flips(rng),
                ..Particle::new(Tex::Explosion, at, lo + span * f, life, alpha, [12.0, 21.0])
            });
        }
    }
    puffs
}

/// The rockets' and grenades' ring's growth (`func_151C04F8`'s 0x3F974EB9, C; I the law Race's
/// smoke shell's, [`Law::Shell`]).
pub const RING_GROWTH: f32 = 1.182;

/// A blast's ring (model 0x157, `effects.gltf` scene 0, a sphere of radius 100; C): L = 14 + 4
/// rand frames, a = rand % 56 + 200, a random yaw.
pub fn ring(rng: &mut Rng, at: Vec3) -> Chunk {
    let l = rng.next_f32() * 4.0 + 14.0;
    let a = (rng.next_u32() % 56) as f32 + 200.0;
    Chunk {
        law: Law::Shell {
            k: RING_GROWTH,
            l,
            a,
        },
        turn: Quat::from_rotation_y(rng.next_f32() * TAU),
        ..Chunk::new(RING, at, 0.0, f32::INFINITY, a, [0.0; 2])
    }
}

/// A rocket's or shell's scorch (kind 0x4F, `func_151C0360` → `func_1513C650`, C): a black decal
/// on the struck face (normal `up`), half-width rand × 75 + 75 (V a quad 271 wide), alpha
/// rand % 56 + 200 × the texel's, random flips, life rand % 205 + 299 (+20) VIs; I: the fade below
/// 20 VIs at 12, 2 off the face.
pub fn scorch(rng: &mut Rng, at: Vec3, up: Vec3) -> Particle {
    let half = rng.next_f32() * 75.0 + 75.0;
    let alpha = (rng.next_u32() % 56) as f32 + 200.0;
    let life = (rng.next_u32() % 205) as f32 + 319.0;
    Particle {
        flat: Some(Quat::from_rotation_arc(Vec3::Z, up)),
        offset: up * 2.0,
        flip: flips(rng),
        colour: [0.0; 3],
        ..Particle::new(Tex::Scorch, at, 2.0 * half, life, alpha, [20.0, 12.0])
    }
}

/// The face a rocket or shell moving `vel` a frame from `at` struck within two frames (I: its last
/// place in the list is short of it): the point and the face's normal, turned against the motion.
pub fn struck(world: &CollisionWorld, at: Vec3, vel: Vec3) -> Option<(Vec3, Vec3)> {
    let len = 2.0 * vel.length();
    let hit = (len > 0.0)
        .then(|| world.ray(at.to_array(), vel.to_array(), len))
        .flatten()?;
    let n = Vec3::from_array(world.tris[hit.tri as usize].normal);
    Some((
        Vec3::from_array(hit.point),
        if n.dot(vel) > 0.0 { -n } else { n },
    ))
}

/// The smoke a rocket's blast leaves (`func_151C0644` → `func_150E83AC`: a type-0x28 emitter for
/// rand % 62 + 120 VIs at the blast, C; V rocket DLs 125-140): its frames, and a puff's chance a
/// frame, rand × 0.0879 + 0.1.
pub fn smoke_frames(rng: &mut Rng) -> f32 {
    ((rng.next_u32() % 62) as f32 + 120.0) / 2.0
}

/// The smoke's colours (V rocket hw_135: prim (0x24, 0x22, 0x11), env (0x7B, 0x93, 0xAA),
/// (PRIM − ENV) × TEXEL0 + ENV, alpha TEXEL0 × ENV_A).
pub const SMOKE_PRIM: [u8; 3] = [0x24, 0x22, 0x11];
pub const SMOKE_ENV: [u8; 3] = [0x7B, 0x93, 0xAA];

/// A puff of that smoke (`func_150E8470`, C; V rocket DLs 135-140: size 105.52 and 87.96, alphas
/// 191 and 222, rising 0.08 and 0.1 more a frame): fire.png at the blast, size rand × 71 + 80,
/// life rand % 34 + 141 VIs, alpha rand % 156 + 100, below 115 VIs at most life × 2 and its size
/// × 1.0224 a VI; its rise grows (rand × 40 + 68) / 1000 a frame.
pub fn smoke(rng: &mut Rng, at: Vec3) -> Particle {
    let flip = flips(rng);
    let alpha = (rng.next_u32() % 156) as f32 + 100.0;
    let life = (rng.next_u32() % 34) as f32 + 141.0;
    let size = rng.next_f32() * 71.0 + 80.0;
    let fall = (rng.next_f32() * 40.0 + 68.0) / 1000.0;
    // ponytail: the lerp's middle; (PRIM − ENV) × TEXEL0 + ENV needs a custom material.
    let colour = std::array::from_fn(|i| f32::from(SMOKE_PRIM[i] / 2 + SMOKE_ENV[i] / 2) / 255.0);
    Particle {
        fall,
        flip,
        colour,
        grow: [115.0, 1.0224],
        ..Particle::new(Tex::Fire, at, size, life, alpha, [115.0, 2.0])
    }
}

/// A glow at an end of 0x34's electric arcs and bolts (`effects-g4.md` §8; V elec, elec2: shell.png
/// (0x1042) × the arc's colour, alpha 62-245 a glow, W 158-357 on the floor's arcs and 692.3 on
/// the bolts, living as long as its arc): here alpha and size uniform in those ranges (I).
pub fn arc_glow(rng: &mut Rng, at: Vec3, rgb: [u8; 3], bolt: bool, frames: f32) -> Particle {
    let alpha = (rng.next_u32() % 184) as f32 + 62.0;
    let size = if bolt {
        692.3
    } else {
        rng.next_f32() * 199.0 + 158.0
    };
    Particle {
        colour: rgb.map(|c| f32::from(c) / 255.0),
        ..Particle::new(Tex::Shell, at, size, 2.0 * frames, alpha, [0.0; 2])
    }
}

/// Particles other modules make (0x34's arc glows, Heist cs3's bubbles), spawned by [`watch`].
#[derive(Resource, Default)]
pub struct Queue(pub Vec<Particle>);

/// A smoke emitter: where, its frames left and its carry.
struct Smoke {
    at: Vec3,
    left: f32,
    carry: f32,
}

/// The flame's puffs' push in the player's frame (forward, up): the gun joint's (0, −71, 96) at
/// half scale (I: fitted to V flame DLs 120-130's three newborn puffs, (3.2, 3.9) a frame on
/// average, against 0.08 of this).
pub const PUFF_PUSH: [f32; 2] = [35.5, 48.0];

/// A flame puff (`func_15199C34`'s second emitter, params 0x8021DE90 + 0xE0.., C; V flame DLs
/// 120-130): puff.png at the nozzle, pushed along `push` plus up to 43 across, × rand × 0.06 +
/// 0.05; y change rand × 0.178 − 0.25 a frame; size rand × 120 + 134, life rand % 21 + 60 VIs,
/// alpha rand % 101 + 155, below 23 VIs life × 11.
pub fn puff(rng: &mut Rng, at: Vec3, push: Vec3) -> Particle {
    let life = (rng.next_u32() % 21) as f32 + 60.0;
    let alpha = (rng.next_u32() % 101) as f32 + 155.0;
    let size = rng.next_f32() * 120.0 + 134.0;
    let side = push.normalize().any_orthonormal_vector();
    let other = push.normalize().cross(side);
    let k = rng.next_f32() * 0.06 + 0.05;
    let turn = (rng.next_u32() & 0xFF) as f32 / 256.0 * std::f32::consts::TAU;
    let across = (side * turn.cos() + other * turn.sin()) * rng.next_f32() * 43.0;
    let fall = rng.next_f32() * 0.178 - 0.25;
    Particle {
        vel: (push + across) * k,
        fall,
        ..Particle::new(Tex::Puff, at, size, life, alpha, [23.0, 11.0])
    }
}

/// The puffs a frame: their count goes up by rand × 0.351 + 0.05, one each time it passes 1.
pub fn puff_count(carry: &mut f32, rng: &mut Rng) -> u32 {
    emit(carry, rng.next_f32() * 0.351 + 0.05)
}

/// A tongue's tail, in frames of its motion behind it (I: V's ribbons of 2-5 quads about 10 long).
pub const TONGUE_TRAIL: f32 = 3.0;

/// A flame tongue (the stream's first emitter → `func_15147DA0`, C; V flame DL 130: 8-17 wide,
/// env alpha 0xA5-0xC4), carry += rand × 0.403 + 0.204 a frame: `flame_tongue.png` at the nozzle,
/// pushed along `push` (the puffs') plus up to 50 across, × rand × 0.1 + 0.1; rising
/// rand × 0.308 + 0.311 more a frame; half-width rand × 6 + 4, life rand % 22 + 50 VIs, alpha rand × 175 + 80,
/// below 22 VIs life × 11. I: one quad from [`TONGUE_TRAIL`] frames back, S along it.
pub fn tongue(rng: &mut Rng, at: Vec3, push: Vec3) -> Streak {
    let life = (rng.next_u32() % 22) as f32 + 50.0;
    let half = rng.next_f32() * 6.0 + 4.0;
    let side = push.normalize().any_orthonormal_vector();
    let other = push.normalize().cross(side);
    let turn = byte_turn(rng);
    let across = (side * turn.cos() + other * turn.sin()) * rng.next_f32() * 50.0;
    let vel = (push + across) * (rng.next_f32() * 0.1 + 0.1);
    let fall = rng.next_f32() * 0.308 + 0.311;
    let alpha = rng.next_f32() * 175.0 + 80.0;
    Streak {
        vel,
        fall,
        trail: TONGUE_TRAIL,
        fade: [22.0, 11.0],
        ..Streak::new(
            Tex::Tongue,
            [at - vel * TONGUE_TRAIL, at],
            half,
            [alpha; 2],
            life,
        )
    }
}

/// A burning player's flames at once at most, and the joints they ride (I: 0-15).
pub const BURN_MAX: usize = 25;
pub const BURN_JOINTS: u32 = 16;
/// A flame's frames a VI (I).
pub const FLAME_RATE: f32 = 0.23;

/// A burning player's flame (`func_151D2F90`'s, carry += rand × 0.608 + 0.698 a frame, at most
/// [`BURN_MAX`], each on a random joint; C, V cone: quads 6-34 wide and 13-53 tall): `flame_a` or
/// `flame_b`'s 5 frames, alpha 80-160, life 12-60 VIs, random flips; I: 6-34 wide uniform, twice as
/// tall, [`FLAME_RATE`].
pub fn flame(rng: &mut Rng) -> Particle {
    let set = (rng.next_u32() & 1) as usize;
    let alpha = (rng.next_u32() % 81) as f32 + 80.0;
    let life = (rng.next_u32() % 49) as f32 + 12.0;
    let wide = rng.next_f32() * 28.0 + 6.0;
    Particle {
        tall: 2.0,
        rate: FLAME_RATE,
        flip: flips(rng),
        ..Particle::new(
            Tex::Flame(set, 0),
            Vec3::ZERO,
            wide / WIDTH,
            life,
            alpha,
            [0.0; 2],
        )
    }
}

/// The explosive death's sparks' start over the feet (V: the player's y + 33).
pub const SPARKS_UP: f32 = 33.0;

/// The explosive death's sparks (`func_15152190`, C, V fx: model 0x132, `effects.gltf` scene 1):
/// 8 + rand % 5 at `at`, yaw a random byte, 23-55 of 256 up, 10 + 9 rand a frame, g −1.004 + 0.254
/// rand, life 40 + rand % 21 VIs, scale 0.07 + 0.07 rand, random angles, spinning ±23.45° a frame
/// about x and z, below 12 VIs alpha 21 × life.
pub fn sparks(rng: &mut Rng, at: Vec3) -> Vec<Chunk> {
    (0..8 + rng.next_u32() % 5)
        .map(|_| {
            let yaw = byte_turn(rng);
            let up = (23 + rng.next_u32() % 33) as f32 / 256.0 * TAU;
            let speed = rng.next_f32() * 9.0 + 10.0;
            let g = rng.next_f32() * 0.254 - 1.004;
            let life = (rng.next_u32() % 21) as f32 + 40.0;
            let scale = rng.next_f32() * 0.07 + 0.07;
            Chunk {
                vel: polar(yaw, up) * speed,
                g,
                turn: any_turn(rng),
                spin: spin_xz(rng, 46.9),
                ..Chunk::new(SPARK, at, scale, life, 255.0, [12.0, 21.0])
            }
        })
        .collect()
}

/// Beach's flak flash's prim (V fx: 0xFF8E13; I: drawn as its tint).
pub const FLAK_PRIM: [f32; 3] = [1.0, 0x8E as f32 / 255.0, 0x13 as f32 / 255.0];

/// War: Beach's flak (`func_150E9890`, C; one emitter, carry += 0.02154 + 0.02815 rand a frame):
/// over the sea in its box (u ±2140, h 0-697, w ±4170 about (89, 878, 2235)), size factor
/// f = 0.996 + 2.323 rand. The flash: `smoke_grey.png`, life T = 20 + rand % 31 VIs, alpha 255 × life / T,
/// size (808 + 608 rand) × f growing × 1.028 a VI (V 3153 → 3922 over 4 frames), random flips. 5-12
/// puffs (model 0xE5, `effects.gltf` scene 2, always blended): random yaw, 0°-90° up, 16.4 + 20.3
/// rand a frame, g −2.06 + 0.99 rand, spin rand × 101 − 49° about x and z, scale (0.1016 + 0.3047
/// rand) × f, life 29 + rand % 41 VIs, alpha 180 + rand % 76, below 20 VIs 12 × life.
pub fn flak(rng: &mut Rng) -> (Particle, Vec<Chunk>) {
    let u = rng.next_f32() * 4280.0 - 2140.0;
    let h = rng.next_f32() * 697.0;
    let w = rng.next_f32() * 8340.0 - 4170.0;
    let at = Vec3::new(
        89.0 - 0.999 * u + 0.0454 * w,
        878.0 + h,
        2235.0 - 0.999 * w - 0.0454 * u,
    );
    let f = rng.next_f32() * 2.323 + 0.996;
    let life = (rng.next_u32() % 31) as f32 + 20.0;
    let size = (rng.next_f32() * 608.0 + 808.0) * f;
    let flash = Particle {
        colour: FLAK_PRIM,
        grow: [f32::INFINITY, 1.028],
        flip: flips(rng),
        ..Particle::new(
            Tex::SmokeGrey,
            at,
            size,
            life,
            255.0,
            [f32::INFINITY, 255.0 / life],
        )
    };
    let puffs = (0..5 + rng.next_u32() % 8)
        .map(|_| {
            let dir = polar(byte_turn(rng), rng.next_f32() * FRAC_PI_2);
            let speed = rng.next_f32() * 20.3 + 16.4;
            let g = rng.next_f32() * 0.99 - 2.06;
            let spin = Vec3::new(
                rng.next_f32() * 101.0 - 49.0,
                0.0,
                rng.next_f32() * 101.0 - 49.0,
            );
            let scale = (rng.next_f32() * 0.3047 + 0.1016) * f;
            let life = (rng.next_u32() % 41) as f32 + 29.0;
            let alpha = (rng.next_u32() % 76) as f32 + 180.0;
            Chunk {
                vel: dir * speed,
                g,
                spin: spin * (PI / 180.0),
                turn: any_turn(rng),
                blend: true,
                ..Chunk::new(FLAK, at, scale, life, alpha, [20.0, 12.0])
            }
        })
        .collect();
    (flash, puffs)
}

/// War: Beach's three north bunkers' tracer spawners (`func_151C4644`, C, V RAM): where, the yaw
/// and pitch they aim at and their tracers' life in frames.
pub const BUNKERS: [([f32; 3], f32, f32, f32); 3] = [
    ([1049.0, 1595.0, -3835.0], 6.1925, 0.1117, 60.19),
    ([-1872.0, 1663.0, -4391.0], 0.2304, 0.1047, 66.35),
    ([-299.0, 1326.0, -4622.0], 6.196, 0.0872, 66.35),
];

/// A bunker's bursts: it waits 210 + rand % 231 VIs, then fires for 60 + rand % 71 VIs, carry +=
/// 0.25 + 0.09375 rand a frame, its aim swaying by sin φ × A (A rand × 0.0596, φ + 0.2608 a frame)
/// in yaw and pitch (C). I: the first burst at once.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bunker {
    wait: f32,
    left: f32,
    carry: f32,
    amp: [f32; 2],
    phase: [f32; 2],
}

impl Bunker {
    /// One frame: the yaw and pitch offsets of the tracers it fires.
    pub fn frame(&mut self, rng: &mut Rng) -> Vec<[f32; 2]> {
        if self.left <= 0.0 {
            self.wait -= 2.0;
            if self.wait > 0.0 {
                return Vec::new();
            }
            self.wait = (rng.next_u32() % 231) as f32 + 210.0;
            self.left = (rng.next_u32() % 71) as f32 + 60.0;
            self.amp = [rng.next_f32() * 0.0596, rng.next_f32() * 0.0596];
            self.phase = [rng.next_f32() * TAU, rng.next_f32() * TAU];
        }
        self.left -= 2.0;
        self.phase = self.phase.map(|p| p + 0.2608);
        let aim = [0, 1].map(|k| self.phase[k].sin() * self.amp[k]);
        (0..emit(&mut self.carry, rng.next_f32() * 0.09375 + 0.25))
            .map(|_| aim)
            .collect()
    }
}

/// A bunker's tracer (`func_151C43E0`/`func_151C455C`, C, V fx): from bunker `k` along its aim
/// plus `turn` (yaw, pitch), unit (sin yaw cos pitch, −sin pitch, cos yaw cos pitch), 191 a frame,
/// half-width 55 + 40 rand, 250 + 300 rand long, alpha 200 + rand % 56 at the head and 0 at the
/// tail, `tracer.png` along it, for the spawner's life.
pub fn bunker_tracer(rng: &mut Rng, k: usize, [dy, dp]: [f32; 2]) -> Streak {
    let (at, yaw, pitch, life) = BUNKERS[k];
    let (yaw, pitch) = (yaw + dy, pitch + dp);
    let dir = Vec3::new(
        yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    let half = rng.next_f32() * 40.0 + 55.0;
    let len = rng.next_f32() * 300.0 + 250.0;
    let alpha = (rng.next_u32() % 56) as f32 + 200.0;
    let head = Vec3::from_array(at);
    Streak {
        u: [1.0, 0.0],
        vel: dir * 191.0,
        ..Streak::new(
            Tex::Tracer,
            [head - dir * len, head],
            half,
            [0.0, alpha],
            2.0 * life,
        )
    }
}

/// The stroke clip's frames where a hand meets the water (op 90 → `func_151AABC4`, C), its hand
/// joint each (I: the right then the left).
pub const RIPPLE_AT: [f32; 2] = [10.8, 26.4];
pub const RIPPLE_HANDS: [usize; 2] = [9, 4];
/// A ripple's frames a VI: each of its 10 held 2 frames (V).
pub const RIPPLE_RATE: f32 = 0.25;

/// A swimmer's ripple (`func_151DBCBC(5, 40, 150, …)` → `func_1513C73C`, C, V swim): a flat 80 ×
/// 80 quad on the water, a random turn and flips, `ripple_00..09` in order, alpha 150, 40 VIs.
pub fn ripple(rng: &mut Rng, at: Vec3) -> Particle {
    let turn = rng.next_f32() * TAU;
    Particle {
        flat: Some(Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_z(turn)),
        offset: Vec3::Y,
        rate: RIPPLE_RATE,
        flip: flips(rng),
        ..Particle::new(Tex::Ripple(0), at, 80.0, 40.0, 150.0, [0.0; 2])
    }
}

/// A splash drop's fall a frame (I: about 1.7).
pub const DROP_FALL: f32 = -1.7;

/// A swimmer's splash drops (`func_15153F18` → `func_151DA6F8`, C, V swim: model 0xBE,
/// `effects.gltf` scene 3, not spawned: that model is an eyeball): 3 + rand % 4, random yaw, 53°-90° up, 8.04 + 3.45 rand a frame,
/// scale 0.791 + 0.203 rand, life 30 + rand % 21 (+16) VIs; I: below 12 VIs alpha 21 × life.
pub fn drops(rng: &mut Rng, at: Vec3) -> Vec<Chunk> {
    (0..3 + rng.next_u32() % 4)
        .map(|_| {
            let dir = polar(byte_turn(rng), (rng.next_f32() * 37.0 + 53.0).to_radians());
            let speed = rng.next_f32() * 3.45 + 8.04;
            let scale = rng.next_f32() * 0.203 + 0.791;
            let life = (rng.next_u32() % 21) as f32 + 46.0;
            Chunk {
                vel: dir * speed,
                g: DROP_FALL,
                ..Chunk::new(DROP, at, scale, life, 255.0, [12.0, 21.0])
            }
        })
        .collect()
}

/// Item 0x3A's fuse: the hand joint holding it (the left) and its lift in flight
/// (`projectiles::lob`'s).
pub const FUSE_HAND: usize = 4;
pub const FUSE_LIFT: f32 = 28.0;
/// A fuse spark's fall a frame (I: "dropping a little").
pub const FUSE_FALL: f32 = -0.5;

/// A burst of item 0x3A's fuse sparks (kind 0x42, carry += 0.129 + rand × 0.117 a frame; V
/// `g5-pack-scratch/laser` lob): 8 + rand % 6 streaks of `fuse_spark.png` from the fuse, a random
/// direction at 11-15 a frame, one frame's travel long, half-width 2.75-4.75, alpha 0 at the tail
/// and 255 at the head, life 15 + rand % 11 VIs, below 8 VIs life × 31; and a `shell.png` flash
/// (kind 0x2B): size rand × 60 + 80, life rand % 5 + 4.
pub fn fuse(rng: &mut Rng, at: Vec3) -> (Vec<Streak>, Particle) {
    let sparks = (0..8 + rng.next_u32() % 6)
        .map(|_| {
            let dir = polar(
                rng.next_f32() * TAU,
                (rng.next_f32() * 2.0 - 1.0) * FRAC_PI_2,
            );
            let vel = dir * (rng.next_f32() * 4.0 + 11.0);
            let half = rng.next_f32() * 2.0 + 2.75;
            let life = (rng.next_u32() % 11) as f32 + 15.0;
            Streak {
                vel,
                fall: FUSE_FALL,
                trail: 1.0,
                fade: [8.0, 31.0],
                ..Streak::new(Tex::FuseSpark, [at, at + vel], half, [0.0, 255.0], life)
            }
        })
        .collect();
    let size = rng.next_f32() * 60.0 + 80.0;
    let life = (rng.next_u32() % 5) as f32 + 4.0;
    (
        sparks,
        Particle::new(Tex::Shell, at, size, life, 255.0, [0.0; 2]),
    )
}

/// Heist cs3's bubbles (`effects-g4.md` §10, V cs3 hw_455: blob 0x4B3, flips, TEXEL0 × PRIM_A):
/// emitter A's W 31-70, life 50-100 VIs, below 20 VIs 12 × life, rising +0.073 to +0.143 more a
/// frame up to 3.5; B's (`b`) W 41-103, life 106-156, below 30 VIs 8 × life, up to 4.0; alpha
/// 100-200. I: uniform; their sway not drawn.
pub fn bubble(rng: &mut Rng, at: Vec3, b: bool) -> Particle {
    let mut r = || rng.next_f32();
    let (wide, life, fade, top) = match b {
        true => (41.0 + 62.0 * r(), 106.0 + 50.0 * r(), [30.0, 8.0], 4.0),
        false => (31.0 + 39.0 * r(), 50.0 + 50.0 * r(), [20.0, 12.0], 3.5),
    };
    let fall = r() * 0.07 + 0.073;
    let alpha = r() * 100.0 + 100.0;
    Particle {
        fall,
        top,
        flip: flips(rng),
        ..Particle::new(Tex::Bubble, at, wide, life, alpha, fade)
    }
}

/// A racer's death blast's centre over its feet (pack-readme).
pub const RACE_BLAST_UP: f32 = 50.0;
/// Race's death: the board's three pieces, then the racer's (`effects.gltf` scenes).
const PIECES: [(usize, bool); 9] = [
    (5, true),
    (6, true),
    (7, true),
    (11, false),
    (10, false),
    (9, false),
    (9, false),
    (8, false),
    (8, false),
];

/// The way a dead racer's pieces are thrown (pack-readme, C `func_150D5A6C`): the normal of the
/// face its last move `d` from `prev` crosses within 397, turned against it; else back along its
/// move; else up.
pub fn race_normal(world: &CollisionWorld, prev: Vec3, d: Vec3) -> Vec3 {
    let Some(dir) = d.try_normalize() else {
        return Vec3::Y;
    };
    let hit = world.ray(prev.to_array(), dir.to_array(), 397.0);
    hit.map_or(-dir, |h| {
        let n = Vec3::from_array(world.tris[h.tri as usize].normal);
        if n.dot(d) > 0.0 { -n } else { n }
    })
}

/// Race's death blast (effect 0x19, `func_150D5A6C`, C; V `g5-pack-scratch/race` d1): at `at`,
/// thrown along `n`: [`PIECES`] at scale 0.5 with random angles, each (U·a + V·b + 1000·N)·f with
/// (a, b) = R·(sin(θ − 64), sin θ) for a byte θ; the board's R rand × 759, f rand × 0.0132 +
/// 0.0298, g rand × 0.648 − 1.397, spin ±19.222° about x and z, life 30 + byte % 21; the racer's
/// R rand × 723, f rand × 0.02 + 0.0052, g rand × 0.626 − 1.089, spin ±20.225°, life 40 + byte %
/// 41; below 12 VIs alpha 21 × life. Then the fire shell ([`Law::Fire`]: L 8 + 3 rand, S 1.305 +
/// 0.403 rand, A 100 + 50 rand, spin ±13.148°) and the smoke shell (the ring, [`Law::Shell`]: k
/// 1.37457, L 15 + 4 rand, a 200 + byte % 56, spin ±4.256°).
pub fn race_blast(rng: &mut Rng, at: Vec3, n: Vec3) -> Vec<Chunk> {
    let u = match n.y.abs() > 0.999 {
        true => Vec3::X,
        false => Vec3::Y.cross(n).normalize(),
    };
    let v = u.cross(n);
    let mut out: Vec<Chunk> = (PIECES.iter())
        .map(|&(scene, board)| {
            let theta = (rng.next_u32() & 0xFF) as f32;
            let mut r = || rng.next_f32();
            let (reach, f, g, spin, life) = match board {
                true => (
                    759.0 * r(),
                    0.0132 * r() + 0.0298,
                    0.648 * r() - 1.397,
                    38.444,
                    30,
                ),
                false => (
                    723.0 * r(),
                    0.02 * r() + 0.0052,
                    0.626 * r() - 1.089,
                    40.45,
                    40,
                ),
            };
            let life = life + rng.next_u32() % if board { 21 } else { 41 };
            let angle = |b: f32| b / 256.0 * TAU;
            let (a, b) = (
                reach * angle(theta - 64.0).sin(),
                reach * angle(theta).sin(),
            );
            Chunk {
                vel: (u * a + v * b + n * 1000.0) * f,
                g,
                spin: spin_xz(rng, spin),
                turn: any_turn(rng),
                ..Chunk::new(scene, at, 0.5, life as f32, 255.0, [12.0, 21.0])
            }
        })
        .collect();
    let (l, s, a) = (
        rng.next_f32() * 3.0 + 8.0,
        rng.next_f32() * 0.403 + 1.305,
        rng.next_f32() * 50.0 + 100.0,
    );
    out.push(Chunk {
        law: Law::Fire { l, s, a },
        spin: spin_xz(rng, 26.296),
        turn: any_turn(rng),
        ..Chunk::new(FIRE, at, 0.0, f32::INFINITY, a, [0.0; 2])
    });
    let (l, a) = (
        rng.next_f32() * 4.0 + 15.0,
        (rng.next_u32() % 56) as f32 + 200.0,
    );
    out.push(Chunk {
        law: Law::Shell { k: 1.37457, l, a },
        spin: spin_xz(rng, 8.512),
        turn: any_turn(rng),
        ..Chunk::new(RING, at, 0.0, f32::INFINITY, a, [0.0; 2])
    });
    out
}

/// A racer's board trail (`func_150D4300`, `effects-g4.md` §13, C): a point a frame at the
/// exhaust, at most [`TRAIL_POINTS`], its width eased toward [`TRAIL_WIDTH`] (I: × 0.35 of the
/// gap a frame; the board's other states' 20 and 10 not told apart).
pub const TRAIL_POINTS: usize = 25;
pub const TRAIL_WIDTH: f32 = 50.0;

/// The trail's length at `speed` a frame: 10 × (speed − 10), 0-600 (C).
pub fn trail_cap(speed: f32) -> f32 {
    (10.0 * (speed - 10.0)).clamp(0.0, 600.0)
}

/// The board trail's ribbon through `points` (the exhaust first) up to `cap` along it, `width`
/// wide at the head tapering to 0 at the tail, its side square to it and to the eye, U across
/// (`race_trail.png`'s cross-section); none under two points.
pub fn trail_mesh(points: &[Vec3], cap: f32, width: f32, eye: Vec3) -> Option<Mesh> {
    let mut along = vec![0.0];
    for w in points.windows(2) {
        along.push(along[along.len() - 1] + w[0].distance(w[1]));
    }
    let n = along.iter().take_while(|&&l| l <= cap).count();
    let total = along[n.max(1) - 1];
    if n < 2 || total <= 0.0 {
        return None;
    }
    let (mut pos, mut uv) = (Vec::new(), Vec::new());
    for (i, &p) in points[..n].iter().enumerate() {
        let tangent = points[i.saturating_sub(1)] - points[(i + 1).min(n - 1)];
        let s = along[i] / total;
        let side = tangent.cross(eye - p).normalize_or_zero() * width / 2.0 * (1.0 - s);
        pos.extend([p - side, p + side]);
        uv.extend([[0.0, s], [1.0, s]]);
    }
    let index = (0..n as u16 - 1).flat_map(|i| {
        let k = 2 * i;
        [k, k + 1, k + 3, k, k + 3, k + 2]
    });
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; pos.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U16(index.collect())),
    )
}

/// The rocket's streak (3D sprite callbacks 0x1B `func_151B85AC` and 0x19 `func_151B863C`, C; V
/// rocket DLs 120-125: head z 5810 → 5411 → 5011, tail 5961 → 5861 → 5761): a quad facing the
/// eye from the rocket back to a tail that leaves the launch point at a quarter of its speed,
/// [`STREAK_WIDTH`] wide, (255, 230, 182) × alpha 200 at the head, 0 at the tail.
pub fn streak(launch: Vec3, head: Vec3, frames: f32) -> [Vec3; 2] {
    let dir = (head - launch).normalize_or_zero();
    let tail = launch + dir * 50.0 * frames;
    [
        head,
        if tail.distance(launch) < head.distance(launch) {
            tail
        } else {
            head
        },
    ]
}

pub const STREAK_WIDTH: f32 = 18.0;
const STREAK_RGBA: [u8; 4] = [255, 230, 182, 200];

/// Whether what leaves the match's lists went off: not across the end or a restart (the rules
/// freeze when it is over, and the restart empties the lists at once).
pub fn live(last: u8, now: u8) -> bool {
    last < 2 && now < 2
}

/// A list's places matched to the last ones seen (`old`), both in order with the new ones last
/// (as `projectiles::fly`): for each old one, its index in `now` if the next is within `reach`;
/// and the index of the first new one.
pub fn follow(old: &[Vec3], now: &[Vec3], reach: f32) -> (Vec<Option<usize>>, usize) {
    let mut i = 0;
    let kept = (old.iter())
        .map(|o| {
            let hit = now.get(i).filter(|p| p.distance(*o) <= reach).map(|_| i);
            i += usize::from(hit.is_some());
            hit
        })
        .collect();
    (kept, i)
}

/// A rocket seen in flight: its position and motion a frame, launch point, first frame, the
/// trail's carry and its glow's and streak's entities.
struct Rocket {
    pos: Vec3,
    vel: Vec3,
    launch: Vec3,
    born: u32,
    carry: f32,
    glow: Entity,
    streak: Entity,
}

/// A shell seen in flight, its motion a frame and its glow.
struct Shell {
    pos: Vec3,
    vel: Vec3,
    glow: Entity,
}

#[derive(Resource)]
struct Looks {
    quad: Handle<Mesh>,
    textures: HashMap<Tex, Handle<Image>>,
    scenes: Vec<Handle<WorldAsset>>,
}

impl Looks {
    fn texture(&self, t: Tex) -> Handle<Image> {
        self.textures.get(&t).cloned().unwrap_or_default()
    }
}

/// What the triggers saw last: each player's shot count and flame carries (puffs, tongues), the
/// rockets, the grenades (and their fuses' carry), the shells, the smokes, the tracers' entities
/// and each bullet's tracer look by (owner, birth), the match's phase, the frame.
#[derive(Resource, Default)]
struct Seen {
    shots: HashMap<Entity, u16>,
    flames: HashMap<Entity, [f32; 2]>,
    rockets: Vec<Rocket>,
    grenades: Vec<(Vec3, f32)>,
    shells: Vec<Shell>,
    smokes: Vec<Smoke>,
    tracers: Vec<Entity>,
    bullets: HashMap<(u16, u32), [f32; 2]>,
    phase: u8,
    tick: u32,
    rng: Rng,
}

pub fn build(app: &mut App) {
    app.init_resource::<Seen>();
    app.init_resource::<Queue>();
    app.add_systems(Startup, setup);
    app.add_systems(
        Update,
        (
            watch,
            bodies,
            sea,
            rocket_streaks,
            sprites,
            streaks,
            chunks,
            boards,
        )
            .chain(),
    );
}

fn setup(mut meshes: ResMut<Assets<Mesh>>, assets: Res<AssetServer>, mut commands: Commands) {
    let gltf = effects_gltf(PACK_WEAPONS);
    commands.insert_resource(Looks {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        textures: Tex::all().map(|t| (t, assets.load(t.path()))).collect(),
        scenes: (0..EFFECTS.len())
            .map(|i| assets.load(format!("{gltf}#Scene{i}")))
            .collect(),
    });
}

/// The materials a [`Chunk`]'s scene draws with, its own copies once the scene is in.
#[derive(Component, Default)]
struct Owned(Vec<Handle<StandardMaterial>>);

/// What spawns the effects.
#[derive(SystemParam)]
struct Fx<'w, 's> {
    looks: Res<'w, Looks>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    commands: Commands<'w, 's>,
}

impl Fx<'_, '_> {
    fn look(&mut self, tex: Tex, alpha: f32) -> MeshMaterial3d<StandardMaterial> {
        MeshMaterial3d(self.materials.add(StandardMaterial {
            base_color: Color::WHITE.with_alpha(alpha),
            base_color_texture: Some(self.looks.texture(tex)),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }))
    }

    fn sprite(&mut self, p: Particle) -> Entity {
        let look = self.look(p.tex, 0.0);
        let at = Transform::from_translation(p.pos + p.offset).with_scale(Vec3::ZERO);
        let quad = Mesh3d(self.looks.quad.clone());
        self.commands.spawn((p, quad, look, at)).id()
    }

    fn streak(&mut self, s: Streak) -> Entity {
        let mesh = Mesh3d(self.meshes.add(quad_mesh(
            s.ends,
            0.0,
            Vec3::ZERO,
            [0.0; 2],
            [0.0; 2],
            false,
        )));
        let look = self.look(s.tex, 1.0);
        self.commands
            .spawn((s, mesh, look, Transform::IDENTITY))
            .id()
    }

    fn chunk(&mut self, c: Chunk) -> Entity {
        let scene = WorldAssetRoot(self.looks.scenes[c.scene].clone());
        let at = Transform::from_translation(c.pos).with_scale(Vec3::ZERO);
        let hidden = Visibility::Hidden;
        self.commands
            .spawn((c, Owned::default(), scene, at, hidden))
            .id()
    }

    /// A blast where a rocket (`vel` its motion), shell or grenade (`vel` none) went at `at`: the
    /// columns and the ring; where a rocket or shell struck a face, there, with the scorch and the
    /// smoke left (C: the grenade's has neither).
    fn explode(&mut self, s: &mut Seen, world: &CollisionWorld, at: Vec3, vel: Option<Vec3>) {
        let face = vel.and_then(|v| struck(world, at, v));
        let at = face.map_or(at, |f| f.0);
        for p in blast(&mut s.rng, at, Vec3::Y) {
            self.sprite(p);
        }
        self.chunk(ring(&mut s.rng, at));
        if let Some((point, up)) = face {
            self.sprite(scorch(&mut s.rng, point, up));
            let left = smoke_frames(&mut s.rng);
            s.smokes.push(Smoke {
                at,
                left,
                carry: 0.0,
            });
        }
    }
}

/// A character's joint `k` (`joint_NN` under its scene) and where it is.
#[derive(SystemParam)]
struct Joints<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    places: Query<'w, 's, &'static GlobalTransform>,
}

impl Joints<'_, '_> {
    fn find(&self, scene: Entity, k: usize) -> Option<Entity> {
        let name = format!("joint_{k:02}");
        (self.children.iter_descendants(scene))
            .find(|&e| self.names.get(e).is_ok_and(|n| n.as_str() == name))
    }

    fn at(&self, scene: Entity, k: usize) -> Option<GlobalTransform> {
        self.find(scene, k)
            .and_then(|e| self.places.get(e).ok().copied())
    }
}

/// The triggers from the match and the shots, once a frame: the shots' flashes, the flame's puffs
/// and tongues, the bullets' tracers, the rockets, shells and grenades (in flight, gone), the
/// smoke left.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn watch(
    game: Query<&MatchView>,
    players: Query<(
        Entity,
        &Combat,
        &Transform,
        &Model,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    timeline: Res<LocalTimeline>,
    world: Res<Collision>,
    lob: Res<Lob>,
    joints: Joints,
    mut seen: ResMut<Seen>,
    mut queue: ResMut<Queue>,
    mut glows: Query<&mut Particle>,
    mut tracers: Query<(&mut Streak, &mut Visibility)>,
    mut fx: Fx,
) {
    let tick = timeline.tick().0;
    let s = &mut *seen;
    for p in queue.0.drain(..) {
        fx.sprite(p);
    }
    let Ok(m) = game.single() else {
        return;
    };
    if tick == s.tick {
        return;
    }
    let frames = tick.saturating_sub(s.tick).clamp(1, 30);
    let k = frames as f32;
    s.tick = tick;
    let live = live(s.phase, m.phase);
    s.phase = m.phase;
    let world = &world.0;
    // The shells: their glows follow them; one gone went off where it was.
    let shells: Vec<(Vec3, Vec3)> = (m.tanks.shells.iter())
        .map(|&(p, yaw, tan)| {
            let (c, sn) = cos_sin(yaw);
            (Vec3::from_array(p), Vec3::new(c, tan, -sn) * SHELL_SPEED)
        })
        .collect();
    let old = std::mem::take(&mut s.shells);
    let at: Vec<_> = old.iter().map(|x| x.pos).collect();
    let now: Vec<_> = shells.iter().map(|x| x.0).collect();
    let (kept, first) = follow(&at, &now, 2.0 * SHELL_SPEED * k);
    for (mut sh, i) in old.into_iter().zip(kept) {
        match i {
            Some(i) => {
                (sh.pos, sh.vel) = shells[i];
                if let Ok(mut g) = glows.get_mut(sh.glow) {
                    g.pos = sh.pos;
                }
                s.shells.push(sh);
            }
            None => {
                fx.commands.entity(sh.glow).despawn();
                if live {
                    fx.explode(s, world, sh.pos, Some(sh.vel));
                }
            }
        }
    }
    for &(pos, vel) in &shells[first..] {
        let glow = fx.sprite(shell_glow(&mut s.rng, pos));
        s.shells.push(Shell { pos, vel, glow });
    }
    let new_shells = shells.len() - first;
    for (e, c, at, model, fighting, predicted) in &players {
        let count = match fighting {
            Some(f) if predicted => f.state.shots.max(c.shots),
            _ => c.shots,
        };
        let last = s.shots.get(&e).copied();
        s.shots.insert(
            e,
            match (live, last) {
                (true, Some(l)) => l.max(count),
                _ => count,
            },
        );
        let shot = match last {
            Some(l) if live && (c.drawn || m.tank) => count.saturating_sub(l).min(4),
            _ => 0,
        };
        let local = |[f, side, u]: [f32; 3]| at.translation + at.rotation * Vec3::new(-side, u, f);
        let facing = at.rotation * Vec3::Z;
        for n in (0..shot).map(|j| count - j) {
            if m.tank {
                let Some(muzzle) = joints.at(model.scene, TANK_MUZZLE) else {
                    continue;
                };
                let [a, b] = TANK_FLASH.map(|p| muzzle.transform_point(Vec3::from_array(p)));
                let (streak, flash) = tank_flash(&mut s.rng, a, b);
                fx.streak(streak);
                fx.sprite(flash);
                if c.power == SCOPE && new_shells == 0 {
                    let dir = (muzzle.rotation() * Vec3::Z)
                        .with_y(0.0)
                        .normalize_or(facing);
                    let [ahead, up] = SCOPE_EYE;
                    let from = at.translation + dir * ahead + Vec3::Y * up;
                    let far = 300.0 * BULLET_SPEED;
                    let hit = world.ray(from.to_array(), dir.to_array(), far);
                    let reach = hit.map_or(far, |h| h.dist);
                    fx.streak(scope_tracer(&mut s.rng, from, dir, reach));
                }
                continue;
            }
            let attack = weapon(c.weapon).map(|w| w.attack);
            for p in flash_points(c.weapon, n) {
                let p = local(p);
                match attack {
                    Some(Attack::Rocket { .. }) => fx.sprite(rocket_flash(&mut s.rng, p)),
                    _ => fx.sprite(flash(&mut s.rng, p)),
                };
                match attack {
                    _ if c.weapon == 0x09 => {
                        // The barrel runs along the hand joint's −y (the flame's points do too).
                        let hand = TWIN_JOINTS[usize::from(n.saturating_sub(1)) % 2];
                        let dir = (joints.at(model.scene, hand))
                            .map(|j| (j.rotation() * Vec3::NEG_Y).normalize_or_zero())
                            .filter(|d| d.dot(facing) > 0.5)
                            .unwrap_or(facing);
                        fx.streak(flash_3d(&mut s.rng, p, dir));
                    }
                    Some(Attack::Bullets { .. } | Attack::Spread { .. }) => {
                        for ray in star(&mut s.rng, p, facing) {
                            fx.streak(ray);
                        }
                    }
                    _ => {}
                }
                if let Some(Attack::Spread { .. }) = attack {
                    for b in spray(&mut s.rng, p, facing) {
                        fx.sprite(b);
                    }
                }
            }
        }
        let carry = s.flames.entry(e).or_default();
        if c.anim == FLAME_FIRE {
            let push = at.rotation * Vec3::new(0.0, PUFF_PUSH[1], PUFF_PUSH[0]);
            let nozzle = local(FLAME.nozzle);
            for _ in 0..frames {
                for _ in 0..puff_count(&mut carry[0], &mut s.rng) {
                    fx.sprite(puff(&mut s.rng, nozzle, push));
                }
                for _ in 0..emit(&mut carry[1], s.rng.next_f32() * 0.403 + 0.204) {
                    fx.streak(tongue(&mut s.rng, nozzle, push));
                }
            }
        }
    }
    // The bullets' tracers, a pool of quads; a seated gunner's are the mounted gun's.
    let mut looks = HashMap::new();
    for (i, b) in m.bullets.iter().enumerate() {
        let key = (b.owner, b.born);
        let first = !s.bullets.contains_key(&key);
        let look = *s
            .bullets
            .entry(key)
            .or_insert_with(|| tracer_look(&mut s.rng));
        looks.insert(key, look);
        let weapon = match m.guns.iter().any(|g| g.gunner == Some(b.owner)) {
            true => 0,
            false => (players.iter().find(|p| p.1.slot == b.owner)).map_or(0, |p| p.1.weapon),
        };
        let t = tracer(
            Vec3::from_array(b.pos),
            Vec3::from_array(b.vel),
            tracer_back(weapon)[usize::from(!first)],
            look,
        );
        match s.tracers.get(i).and_then(|&e| tracers.get_mut(e).ok()) {
            Some((mut streak, mut v)) => {
                *streak = t;
                v.set_if_neq(Visibility::Inherited);
            }
            None => s.tracers.push(fx.streak(t)),
        }
    }
    s.bullets = looks;
    for &e in s.tracers.iter().skip(m.bullets.len()) {
        if let Ok((_, mut v)) = tracers.get_mut(e) {
            v.set_if_neq(Visibility::Hidden);
        }
    }
    // The rockets: glow, trail and streak; one gone went off where it was.
    let rockets: Vec<Vec3> = m.rockets.iter().map(|p| Vec3::from_array(*p)).collect();
    let old = std::mem::take(&mut s.rockets);
    let at: Vec<_> = old.iter().map(|r| r.pos).collect();
    let (kept, first) = follow(&at, &rockets, 400.0 * k);
    for (mut r, i) in old.into_iter().zip(kept) {
        match i {
            Some(i) => {
                let p = rockets[i];
                r.vel = (p - r.pos) / k;
                r.carry += p.distance(r.pos);
                while r.carry >= TRAIL_STEP {
                    r.carry -= TRAIL_STEP;
                    fx.sprite(trail(&mut s.rng, p, r.vel));
                }
                if let Ok(mut g) = glows.get_mut(r.glow) {
                    g.pos = p;
                }
                r.pos = p;
                s.rockets.push(r);
            }
            None => {
                fx.commands.entity(r.glow).despawn();
                fx.commands.entity(r.streak).despawn();
                if live {
                    fx.explode(s, world, r.pos, Some(r.vel));
                }
            }
        }
    }
    for &p in &rockets[first..] {
        let glow = fx.sprite(glow(&mut s.rng, p));
        let [r, g, b, a] = STREAK_RGBA;
        let streak = fx.commands.spawn((
            Mesh3d(fx.meshes.add(quad_mesh(
                [p, p],
                0.0,
                Vec3::ZERO,
                [0.0; 2],
                [0.0; 2],
                false,
            ))),
            MeshMaterial3d(fx.materials.add(StandardMaterial {
                base_color: Color::srgba_u8(r, g, b, a),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })),
            Transform::IDENTITY,
        ));
        let streak = streak.id();
        s.rockets.push(Rocket {
            pos: p,
            vel: Vec3::ZERO,
            launch: p,
            born: tick,
            carry: 0.0,
            glow,
            streak,
        });
    }
    // The grenades: one gone went off where it was (I: `func_150D728C` with the rocket's columns);
    // item 0x3A's fuse sparks while it flies.
    let grenades: Vec<Vec3> = m.grenades.iter().map(|p| Vec3::from_array(*p)).collect();
    let old = std::mem::take(&mut s.grenades);
    let at: Vec<_> = old.iter().map(|g| g.0).collect();
    let (kept, first) = follow(&at, &grenades, 60.0 * k);
    for ((pos, carry), i) in old.into_iter().zip(kept) {
        match i {
            Some(i) => s.grenades.push((grenades[i], carry)),
            None if live => fx.explode(s, world, pos, None),
            None => {}
        }
    }
    s.grenades
        .extend(grenades[first..].iter().map(|&p| (p, 0.0)));
    if lob.0.is_some() {
        for (pos, carry) in &mut s.grenades {
            for _ in 0..frames {
                for _ in 0..emit(carry, s.rng.next_f32() * 0.117 + 0.129) {
                    let (sparks, flash) = fuse(&mut s.rng, *pos + Vec3::Y * FUSE_LIFT);
                    for spark in sparks {
                        fx.streak(spark);
                    }
                    fx.sprite(flash);
                }
            }
        }
    }
    for m in &mut s.smokes {
        for _ in 0..frames.min(m.left.ceil() as u32) {
            m.carry += s.rng.next_f32() * 0.0879 + 0.1;
            while m.carry > 1.0 {
                m.carry -= 1.0;
                fx.sprite(smoke(&mut s.rng, m.at));
            }
        }
        m.left -= k;
    }
    s.smokes.retain(|m| m.left > 0.0);
    s.shots.retain(|e, _| players.contains(*e));
    s.flames.retain(|e, _| players.contains(*e));
}

/// What [`bodies`] saw last: the frame, each player's clip and its time, its flames' carry and
/// their ends, its fuse's carry, a racer's death and place.
#[derive(Default)]
struct Bodies {
    tick: u32,
    rng: Rng,
    anims: HashMap<Entity, (u16, f32)>,
    burning: HashMap<Entity, (f32, Vec<u32>)>,
    fuses: HashMap<Entity, f32>,
    racers: HashMap<Entity, (bool, Vec3)>,
}

/// Whether a looping clip's time went past `mark` from `last` to `now`.
pub fn crossed(last: f32, now: f32, mark: f32) -> bool {
    match now >= last {
        true => last < mark && mark <= now,
        false => last < mark || mark <= now,
    }
}

/// The triggers from the players' clips, once a frame: the burning flames, the explosive death's
/// sparks, the swimmers' ripples, item 0x3A's fuse in the hand, a racer's death.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn bodies(
    game: Query<&MatchView>,
    players: Query<(
        Entity,
        &Combat,
        &Transform,
        &Model,
        Option<&Anim>,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    timeline: Res<LocalTimeline>,
    world: Res<Collision>,
    lob: Res<Lob>,
    joints: Joints,
    mut state: Local<Bodies>,
    mut fx: Fx,
) {
    let Ok(m) = game.single() else {
        return;
    };
    let tick = timeline.tick().0;
    let b = &mut *state;
    if tick == b.tick {
        return;
    }
    let frames = tick.saturating_sub(b.tick).clamp(1, 30);
    b.tick = tick;
    for (e, c, at, model, anim, fighting, predicted) in &players {
        let (id, time) = anim.map_or((c.anim, 0.0), |a| (a.0.id, a.0.time));
        let (last_id, last_time) = b.anims.insert(e, (id, time)).unwrap_or((id, time));
        if [FLAME_HIT, BURNING, BURN_END].contains(&id) {
            let (carry, ends) = b.burning.entry(e).or_default();
            ends.retain(|&t| t > tick);
            for _ in 0..frames {
                for _ in 0..emit(carry, b.rng.next_f32() * 0.608 + 0.698) {
                    let joint = (b.rng.next_u32() % BURN_JOINTS) as usize;
                    let f = flame(&mut b.rng);
                    if ends.len() < BURN_MAX {
                        ends.push(tick + (f.life / 2.0).ceil() as u32);
                        fx.sprite(Particle {
                            pos: at.translation + Vec3::Y * 60.0,
                            follow: joints.find(model.scene, joint),
                            ..f
                        });
                    }
                }
            }
        }
        if id == BLOWN_UP && last_id != BLOWN_UP && c.health == 0 {
            for spark in sparks(&mut b.rng, at.translation + Vec3::Y * SPARKS_UP) {
                fx.chunk(spark);
            }
        }
        if (id == STROKE || id == STROKE_DEEP) && last_id == id {
            for (mark, hand) in RIPPLE_AT.into_iter().zip(RIPPLE_HANDS) {
                let Some(hand) = joints
                    .at(model.scene, hand)
                    .filter(|_| crossed(last_time, time, mark))
                else {
                    continue;
                };
                let h = hand.translation();
                let water = world.0.water(h.x, h.z);
                if (water - h.y).abs() > 300.0 {
                    continue;
                }
                let on = Vec3::new(h.x, water, h.z);
                // ponytail: no [`drops`]: the pack's #0xBE (`drop`) is an eyeball (texture 0x2782), so
                // §9's model id 0x35 → #0xBE needs a recheck before they show.
                fx.sprite(ripple(&mut b.rng, on));
            }
        }
        if lob.0.is_some()
            && [GRENADE_PULL, GRENADE_HOLD].contains(&id)
            && let Some(hand) = joints.at(model.scene, FUSE_HAND)
        {
            let carry = b.fuses.entry(e).or_default();
            for _ in 0..frames {
                for _ in 0..emit(carry, b.rng.next_f32() * 0.117 + 0.129) {
                    let (sparks, flash) = fuse(&mut b.rng, hand.translation());
                    for spark in sparks {
                        fx.streak(spark);
                    }
                    fx.sprite(flash);
                }
            }
        }
        if m.race {
            let dead = match fighting {
                Some(f) if predicted => f.state.health == 0,
                _ => c.health == 0,
            };
            let (was, prev) =
                (b.racers.insert(e, (dead, at.translation))).unwrap_or((dead, at.translation));
            if dead && !was {
                let n = race_normal(&world.0, prev, at.translation - prev);
                let o = at.translation + Vec3::Y * RACE_BLAST_UP;
                for piece in race_blast(&mut b.rng, o, n) {
                    fx.chunk(piece);
                }
            }
        }
    }
    b.anims.retain(|e, _| players.contains(*e));
    b.burning.retain(|e, _| players.contains(*e));
    b.fuses.retain(|e, _| players.contains(*e));
    b.racers.retain(|e, _| players.contains(*e));
}

/// War: Beach's sky and bunkers: the flak and the tracers, once a frame.
fn sea(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    mut state: Local<(u32, Rng, f32, [Bunker; 3])>,
    mut fx: Fx,
) {
    if !game.iter().any(|m| m.beach) {
        return;
    }
    let tick = timeline.tick().0;
    let (last, rng, carry, bunkers) = &mut *state;
    if tick == *last {
        return;
    }
    let frames = tick.saturating_sub(*last).clamp(1, 30);
    *last = tick;
    for _ in 0..frames {
        for _ in 0..emit(carry, rng.next_f32() * 0.02815 + 0.02154) {
            let (flash, puffs) = flak(rng);
            fx.sprite(flash);
            for p in puffs {
                fx.chunk(p);
            }
        }
        for (k, b) in bunkers.iter_mut().enumerate() {
            for aim in b.frame(rng) {
                fx.streak(bunker_tracer(rng, k, aim));
            }
        }
    }
}

/// The rockets' streaks, each frame toward the eye.
fn rocket_streaks(
    seen: Res<Seen>,
    timeline: Res<LocalTimeline>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    streaks: Query<&Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    let tick = timeline.tick().0;
    for r in &seen.rockets {
        let [head, tail] = streak(r.launch, r.pos, tick.saturating_sub(r.born) as f32);
        if let Ok(m) = streaks.get(r.streak)
            && let Some(mut mesh) = meshes.get_mut(&m.0)
        {
            let half = STREAK_WIDTH / 2.0;
            *mesh = quad_mesh([tail, head], half, eye, [0.0, 255.0], [0.0, 1.0], false);
        }
    }
}

/// A quad from `a` to `b` facing `eye`, `half` each side, S `u` and alpha `alphas` (of 255) at
/// each end, T across (flipped).
fn quad_mesh(
    [a, b]: [Vec3; 2],
    half: f32,
    eye: Vec3,
    alphas: [f32; 2],
    [ua, ub]: [f32; 2],
    flip: bool,
) -> Mesh {
    let side = (b - a).cross(eye - a).normalize_or_zero() * half;
    let [t0, t1] = if flip { [1.0, 0.0] } else { [0.0, 1.0] };
    let [aa, ab] = alphas.map(|x| x / 255.0);
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![a - side, a + side, b + side, b - side],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[ua, t0], [ua, t1], [ub, t1], [ub, t0]],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![
            [1.0, 1.0, 1.0, aa],
            [1.0, 1.0, 1.0, aa],
            [1.0, 1.0, 1.0, ab],
            [1.0, 1.0, 1.0, ab],
        ],
    )
    .with_inserted_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]))
}

/// The sprites move, face the camera (or lie flat), follow their joint, play their frames and
/// fade; gone at the end of their life.
#[allow(clippy::type_complexity)]
fn sprites(
    time: Res<Time>,
    looks: Res<Looks>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    places: Query<&GlobalTransform>,
    mut sprites: Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    for (e, mut s, mut t, m) in &mut sprites {
        if !s.step(frames) {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(j) = s.follow.and_then(|j| places.get(j).ok()) {
            s.pos = j.translation();
        }
        let flip = s.flip.map(|f| if f { -1.0 } else { 1.0 });
        let (turn, w) = match s.flat {
            Some(flat) => (flat, s.size),
            None => (facing, s.size * WIDTH),
        };
        *t = Transform::from_translation(s.pos + s.offset)
            .with_rotation(turn)
            .with_scale(Vec3::new(w * flip[0], w * s.tall * flip[1], 1.0));
        if let Some(mut m) = materials.get_mut(&m.0) {
            let [r, g, b] = s.rgb();
            m.base_color = Color::srgba(r, g, b, s.alpha() / 255.0);
            if s.rate > 0.0 {
                let frame = looks.texture(s.tex.at(s.age, s.rate));
                if m.base_color_texture.as_ref() != Some(&frame) {
                    m.base_color_texture = Some(frame);
                }
            }
        }
    }
}

/// The 3D sprites move and fade, facing the eye; gone at the end of their life.
fn streaks(
    time: Res<Time>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    mut streaks: Query<(Entity, &mut Streak, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    for (e, mut s, m) in &mut streaks {
        if !s.step(frames) {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&m.0) {
            *mesh = quad_mesh(s.ends, s.half, eye, s.alphas(), s.u, s.flip);
        }
    }
}

/// The model pieces fly, spin, grow and fade on their own copies of their scene's materials
/// (an opaque one drawn blended once its alpha is below 255); gone at their end.
#[allow(clippy::type_complexity)]
fn chunks(
    time: Res<Time>,
    mut pieces: Query<(
        Entity,
        &mut Chunk,
        &mut Owned,
        &mut Transform,
        &mut Visibility,
    )>,
    children: Query<&Children>,
    mut shown: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    for (e, mut c, mut own, mut t, mut v) in &mut pieces {
        let Some((scale, alpha)) = c.step(frames) else {
            commands.entity(e).despawn();
            continue;
        };
        *t = Transform::from_translation(c.pos)
            .with_rotation(c.turn)
            .with_scale(Vec3::splat(scale));
        if own.0.is_empty() {
            for part in children.iter_descendants(e) {
                if let Ok(mut m) = shown.get_mut(part) {
                    let copy = materials.get(&m.0).cloned().unwrap_or_default();
                    m.0 = materials.add(copy);
                    own.0.push(m.0.clone());
                }
            }
            if own.0.is_empty() {
                continue;
            }
            v.set_if_neq(Visibility::Inherited);
        }
        for h in &own.0 {
            if let Some(mut m) = materials.get_mut(h) {
                m.base_color.set_alpha(alpha / 255.0);
                if alpha < 255.0 || c.blend {
                    m.alpha_mode = AlphaMode::Blend;
                }
            }
        }
    }
}

/// A racer's board trail: its points, width and length, its place a frame ago, its ribbon.
#[derive(Default)]
struct Board {
    points: VecDeque<Vec3>,
    width: f32,
    cap: f32,
    last: Option<Vec3>,
    ribbon: Option<Entity>,
}

/// Race: each living racer's board trail ([`trail_mesh`]), a point a frame at its exhaust, drawn
/// each frame from the exhaust where it is now.
#[allow(clippy::type_complexity)]
fn boards(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    racers: Query<
        (
            Entity,
            &Combat,
            &Transform,
            Option<&Fighting>,
            Has<Predicted>,
        ),
        With<Model>,
    >,
    mut ribbons: Query<(&Mesh3d, &mut Visibility), Without<Model>>,
    mut state: Local<(u32, HashMap<Entity, Board>)>,
    mut fx: Fx,
) {
    if !game.iter().any(|m| m.race) {
        return;
    }
    let tick = timeline.tick().0;
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    let (last, boards) = &mut *state;
    let frames = tick.saturating_sub(*last).min(30) as f32;
    *last = tick;
    for (e, c, at, fighting, predicted) in &racers {
        let dead = match fighting {
            Some(f) if predicted => f.state.health == 0,
            _ => c.health == 0,
        };
        let b = boards.entry(e).or_default();
        let exhaust = at.transform_point(Vec3::from_array(crate::race::EXHAUST_AT));
        if frames > 0.0 {
            let speed = b.last.map_or(0.0, |l| l.distance(at.translation) / frames);
            b.last = Some(at.translation);
            b.cap = trail_cap(speed);
            if dead {
                b.points.clear();
                b.width = 0.0;
            } else {
                b.points.push_front(exhaust);
                b.points.truncate(TRAIL_POINTS);
                b.width += (TRAIL_WIDTH - b.width) * (1.0 - 0.65f32.powf(frames));
            }
        }
        let ribbon = *b.ribbon.get_or_insert_with(|| {
            let mesh = Mesh3d(fx.meshes.add(Rectangle::new(1.0, 1.0)));
            let look = fx.look(Tex::Trail, 1.0);
            let hidden = Visibility::Hidden;
            fx.commands
                .spawn((mesh, look, Transform::IDENTITY, hidden))
                .id()
        });
        let Ok((mesh, mut v)) = ribbons.get_mut(ribbon) else {
            continue;
        };
        let points: Vec<Vec3> = std::iter::once(exhaust)
            .chain(b.points.iter().copied())
            .collect();
        let made = trail_mesh(&points, b.cap, b.width, eye).filter(|_| !dead);
        v.set_if_neq(match made.is_some() {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        if let Some(made) = made
            && let Some(mut m) = fx.meshes.get_mut(&mesh.0)
        {
            *m = made;
        }
    }
    boards.retain(|e, b| {
        let alive = racers.contains(*e);
        if let Some(r) = b.ribbon.filter(|_| !alive) {
            fx.commands.entity(r).despawn();
        }
        alive
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_rules::projectile::local_to_world;

    /// The slot-0 pose of the oracle's twin, rocket and flame runs.
    const AT: [f32; 3] = [-2247.0, -521.0, 6015.0];
    const FACING: u16 = 16896;

    fn world(p: [f32; 3]) -> Vec3 {
        Vec3::from_array(local_to_world(AT, FACING, p))
    }

    fn length(s: &Streak) -> f32 {
        s.ends[0].distance(s.ends[1])
    }

    /// The flashes where the oracle drew them, and their lives: 4 or 5 frames, alpha 255 then
    /// 170 or 85 on the last (V twin DLs 104-108: 4, 2 at 0xAA; 7, 5, 3, 1 at 0x55).
    #[test]
    fn flashes_as_the_original() {
        let v = [
            (TWIN_FLASHES[0], [-2228.99, -458.66, 5974.02]),
            (TWIN_FLASHES[1], [-2269.32, -476.56, 5966.6]),
            (ROCKET_FLASHES[0], [-2237.35, -461.59, 5971.62]),
            (ROCKET_FLASHES[1], [-2231.96, -452.61, 6058.5]),
        ];
        for (local, at) in v {
            assert!(
                world(local).distance(Vec3::from_array(at)) < 0.1,
                "{local:?}"
            );
        }
        assert_eq!(flash_points(0x09, 1), vec![TWIN_FLASHES[0]]);
        assert_eq!(flash_points(0x09, 2), vec![TWIN_FLASHES[1]]);
        assert_eq!(flash_points(0x16, 1), ROCKET_FLASHES.to_vec());
        assert!(flash_points(0x21, 1).is_empty());
        let mut rng = Rng::default();
        for _ in 0..200 {
            let mut f = flash(&mut rng, Vec3::ZERO);
            assert!((359.0..629.0).contains(&f.size) && (7.0..=10.0).contains(&f.life));
            let mut alphas = vec![f.alpha()];
            while f.step(1.0) {
                alphas.push(f.alpha());
            }
            let last = *alphas.last().unwrap();
            assert!(matches!(alphas.len(), 4 | 5) && (last == 85.0 || last == 170.0));
            assert!(alphas[..alphas.len() - 1].iter().all(|&a| a == 255.0));
            let r = rocket_flash(&mut rng, Vec3::ZERO);
            assert!((601.0..805.0).contains(&r.size) && (200.0..256.0).contains(&r.alpha));
        }
        // The flash's width: 365.64 → 53.5 units (V twin DL 110's texrect, I at 50°).
        assert!((365.64 * WIDTH - 53.5).abs() < 0.1);
    }

    /// The 3D flashes: the twin guns' 29.6-65.3 long and 14.7-17.2 half-wide from the 2D flash's
    /// point (V twin2 DLs 104-112), 3-4 frames; the rifle's six rays 73° off the barrel, 39-51
    /// long, gone in two frames (V rifle DLs 100-101); Tank's 228 long and 88 wide on a joint at
    /// half scale (V k1).
    #[test]
    fn the_3d_flashes_as_the_original() {
        let mut rng = Rng::default();
        let dir = Vec3::new(0.0, 0.2, 1.0).normalize();
        let (mut lo, mut hi) = (f32::MAX, 0.0f32);
        for _ in 0..500 {
            let mut f = flash_3d(&mut rng, Vec3::ONE, dir);
            (lo, hi) = (lo.min(length(&f)), hi.max(length(&f)));
            assert!(f.ends[0] == Vec3::ONE && (14.1..=20.0).contains(&f.half));
            assert_eq!((f.u, f.alphas()), ([1.0, 0.0], [255.0; 2]));
            let frames = (1..).take_while(|_| f.step(1.0)).count() + 1;
            assert!(matches!(frames, 3..=5), "{frames}");
        }
        assert!(
            lo < 29.6 && hi > 65.3 && lo > 25.0 && hi < 66.7,
            "{lo} {hi}"
        );
        let rays = star(&mut rng, Vec3::ZERO, Vec3::Z);
        assert_eq!(rays.len(), 6);
        for mut r in rays {
            let along = (r.ends[1] - r.ends[0]).normalize();
            assert!((0.26..0.32).contains(&along.z) && (39.0..=51.0).contains(&length(&r)));
            assert!((5.7..=11.7).contains(&r.half) && (111.0..=237.0).contains(&r.alpha[0]));
            assert!(r.step(1.0) && !r.step(1.0));
        }
        // Tank: (0, 12, 120) to (0, 27, 215) at the joint's 0.5, × 3.59-6.1.
        let at = |p: [f32; 3]| Vec3::from_array(p) * 0.5;
        let (mut lo, mut hi) = (f32::MAX, 0.0f32);
        for _ in 0..500 {
            let (s, f) = tank_flash(&mut rng, at(TANK_FLASH[0]), at(TANK_FLASH[1]));
            (lo, hi) = (lo.min(length(&s)), hi.max(length(&s)));
            assert!((40.0..=70.0).contains(&s.half) && (200.0..=255.0).contains(&s.alpha[0]));
            assert!((5.0..=9.0).contains(&s.life) && f.life == s.life + 1.0);
            assert!((1296.0..=1806.0).contains(&f.size) && f.pos == s.ends[0]);
        }
        assert!(
            lo < 228.0 && hi > 228.0 && lo > 172.0 && hi < 294.0,
            "{lo} {hi}"
        );
        let g = shell_glow(&mut rng, Vec3::ZERO);
        assert!((1700.0..=2500.0).contains(&g.size) && g.alpha() == 255.0);
    }

    /// The tracers: 186 behind a bullet on its first list and 366 on (V twin2: 183-190, 357-371
    /// at 300 a frame), alpha at the head, 0 at the tail, moving with it; the scope's to its wall;
    /// Beach's bunkers' 250-550 long at 191 a frame, bursting about 0.3 a frame.
    #[test]
    fn the_tracers_as_the_original() {
        let mut rng = Rng::default();
        let vel = Vec3::new(0.0, 0.0, -300.0);
        let look = tracer_look(&mut rng);
        let mut first = tracer(Vec3::ZERO, vel, TRACER_BACK[0], look);
        assert!(
            (183.0..=190.0).contains(&length(&first)),
            "{}",
            length(&first)
        );
        let next = tracer(Vec3::ZERO, vel, TRACER_BACK[1], look);
        assert!((357.0..=371.0).contains(&length(&next)) && next.ends[1] == Vec3::ZERO);
        assert!(first.alpha[0] == 0.0 && (124.0..=238.0).contains(&first.alpha[1]));
        assert!((28.0..=32.0).contains(&first.half) && first.u == [1.0, 0.0]);
        assert!(first.step(0.5) && first.ends[1].z == -150.0);
        let mut scope = scope_tracer(&mut rng, Vec3::ZERO, Vec3::X, 900.0);
        assert_eq!(
            (1..10).take_while(|_| scope.step(1.0)).count(),
            2,
            "three frames to 900"
        );
        let mut b = Bunker::default();
        let mut shots = Vec::new();
        for _ in 0..3000 {
            shots.extend(b.frame(&mut rng));
        }
        let rate = shots.len() as f32 / 3000.0;
        // Bursts of 30-65 frames every 105-220 + 30-65 at 0.25-0.34 a frame.
        assert!((0.03..0.15).contains(&rate), "{rate}");
        assert!(
            shots
                .iter()
                .all(|a| a[0].abs() <= 0.0596 && a[1].abs() <= 0.0596)
        );
        let t = bunker_tracer(&mut rng, 0, [0.0; 2]);
        assert!((250.0..=550.0).contains(&length(&t)) && (t.vel.length() - 191.0).abs() < 0.01);
        assert!(t.vel.z > 0.95 * 191.0 && t.vel.y < 0.0 && (t.life - 120.38).abs() < 0.01);
    }

    /// The shotgun's spray (V shotgun2, two shots of 10: 20.5-38.7 a frame along the barrel,
    /// ±7.8 across, sizes 80-221, lives 10-21 VIs, alphas 156-254, gone in at most 10 frames) and
    /// the pistol's and 0x23's tracers from the muzzle (V laneD pistol 280 then 546, gun23 302
    /// then 589 at 300 a frame; the twin guns' kept).
    #[test]
    fn the_shotgun_spray_and_the_guns_tracers_as_the_original() {
        let mut rng = Rng::default();
        let dir = Vec3::NEG_Z;
        for _ in 0..200 {
            let blobs = spray(&mut rng, Vec3::ZERO, dir);
            assert_eq!(blobs.len(), SPRAY);
            for mut b in blobs {
                let ahead = b.vel.dot(dir);
                let across = b.vel - dir * ahead;
                assert!(
                    (20.0..39.0).contains(&ahead) && across.x.abs() <= 8.0 && across.y.abs() <= 8.0
                );
                assert!((80.0..221.0).contains(&b.size) && (10.0..=21.0).contains(&b.life));
                assert!((155.0..=254.0).contains(&b.alpha) && (-0.59..=-0.04).contains(&b.fall));
                let frames = (1..30).take_while(|_| b.step(1.0)).count();
                assert!(frames <= 10 && b.alpha() <= 36.0, "{frames}");
            }
        }
        let vel = Vec3::new(0.0, 0.0, -300.0);
        let look = tracer_look(&mut rng);
        let len = |w: u8, k: usize| length(&tracer(Vec3::ZERO, vel, tracer_back(w)[k], look));
        assert!((len(0x39, 0) - 280.0).abs() < 2.0 && (len(0x39, 1) - 546.0).abs() < 2.0);
        assert!((len(0x23, 0) - 302.0).abs() < 2.0 && (len(0x23, 1) - 589.0).abs() < 2.0);
        assert_eq!([0x09, 0x38, 0].map(tracer_back), [TRACER_BACK; 3]);
    }

    /// The restart empties the rocket and grenade lists at once: nothing goes off then, nor at the
    /// end (G4's note); in play, a rocket gone goes off and the others keep their places.
    #[test]
    fn a_restart_sets_nothing_off() {
        let old = [Vec3::ZERO, Vec3::X * 1000.0];
        let (kept, first) = follow(&old, &[], 400.0);
        assert_eq!((kept, first), (vec![None, None], 0));
        assert!(!live(2, 0) && !live(3, 0) && !live(2, 3) && !live(1, 2));
        assert!(live(0, 0) && live(0, 1));
        let now = [Vec3::X * 1200.0, Vec3::Z * 5.0];
        let (kept, first) = follow(&old, &now, 400.0);
        assert_eq!((kept, first), (vec![None, Some(0)], 1));
    }

    /// The blast's columns (V rocket DL 125: column 1's sizes 76.31 + 73.86 i for 8 puffs, its
    /// motion 53.33 up for the first, × 1/8 for the last; 0.7744 of it kept a frame, DLs 125-128).
    #[test]
    fn the_blast_as_the_original() {
        let mut rng = Rng::default();
        for _ in 0..50 {
            let puffs = blast(&mut rng, Vec3::ZERO, Vec3::Y);
            assert!((21..=70).contains(&puffs.len()), "{}", puffs.len());
            let first = &puffs[0];
            assert!((60.0..90.0).contains(&first.vel.y) && (45.0..98.0).contains(&first.size));
            assert!(
                puffs
                    .iter()
                    .all(|p| p.offset.length() <= 15.0 && p.life >= 37.0)
            );
            assert!(puffs.iter().all(|p| (-0.398..-0.006).contains(&p.fall)));
        }
        let mut p = blast(&mut rng, Vec3::ZERO, Vec3::Y)[0];
        p.vel = Vec3::new(-8.33, 53.33, 3.8);
        p.fall = -0.27;
        p.pos = Vec3::new(-2306.38, -387.72, 4787.55);
        p.step(1.0);
        assert!((p.pos.y + 334.66).abs() < 0.01 && (p.vel.y - 41.09).abs() < 0.01);
        p.step(1.0);
        p.step(1.0);
        assert!((p.pos.y + 262.5).abs() < 0.05 && (p.vel.x + 3.87).abs() < 0.01);
        // The smoke it leaves (V DLs 135-140: size 105.52 kept above 115 VIs, rising 0.08 more a
        // frame), growing × 1.0224 a VI and fading below 115 VIs; 60-90 frames of it.
        let mut m = smoke(&mut rng, Vec3::ZERO);
        let size = m.size;
        m.step(1.0);
        assert!(m.size == size && (0.068..0.108).contains(&m.vel.y) && m.alpha() >= 100.0);
        m.life = 114.0;
        m.step(1.0);
        assert!((m.size / size - 1.0224f32.powi(2)).abs() < 1e-4 && m.alpha() <= 224.0);
        assert!((60.0..=90.5).contains(&smoke_frames(&mut rng)));
        // 0x34's arc glows: the arc's colour, its life, a constant alpha.
        let mut g = arc_glow(&mut rng, Vec3::ZERO, [0x7E, 0xF9, 0xFF], false, 10.0);
        assert!((158.0..357.0).contains(&g.size) && (62.0..=245.0).contains(&g.alpha));
        assert_eq!((g.rgb()[0], g.life), (0x7E as f32 / 255.0, 20.0));
        let a = g.alpha();
        assert!((0..9).all(|_| g.step(1.0)) && !g.step(1.0) && g.alpha() == a);
    }

    /// The ring: 14-18 frames, scale 1.182·√t (radius 100 × it), alpha a to 0; the scorch: 150-300
    /// wide (V 271), lying on the face, 160-262 frames, black.
    #[test]
    fn the_ring_and_scorch_as_the_original() {
        let mut rng = Rng::default();
        for _ in 0..100 {
            let mut r = ring(&mut rng, Vec3::ZERO);
            let Law::Shell { l, a, .. } = r.law else {
                panic!("{:?}", r.law);
            };
            assert!((14.0..=18.0).contains(&l) && (200.0..=255.0).contains(&a));
            assert_eq!(r.step(1.0), Some((RING_GROWTH, (a - a / (l * l)).floor())));
            let steps = (0..30).take_while(|_| r.step(1.0).is_some()).count() + 1;
            assert_eq!(steps, l.floor() as usize);
            let up = Vec3::new(0.0, 0.0, -1.0);
            let s = scorch(&mut rng, Vec3::ZERO, up);
            assert!((150.0..=300.0).contains(&s.size) && (319.0..=523.0).contains(&s.life));
            assert!((s.flat.unwrap() * Vec3::Z).distance(up) < 1e-5 && s.rgb() == [0.0; 3]);
            assert!((200.0..=255.0).contains(&s.alpha) && s.offset == up * 2.0);
        }
    }

    /// The trail: its puffs ride at 0.6-0.76 of the rocket's motion (V rocket DL 125: z −120 to
    /// −148 at −200), darken from 25 VIs and fade from 10; 2.2 a frame at 200 a frame.
    #[test]
    fn the_rocket_trail_as_the_original() {
        let mut rng = Rng::default();
        let vel = Vec3::new(0.0, 0.0, -200.0);
        for _ in 0..200 {
            let mut t = trail(&mut rng, Vec3::ZERO, vel);
            assert!((-162.0..-111.0).contains(&t.vel.z), "{}", t.vel.z);
            assert!((25.0..=50.0).contains(&t.life) && (100.0..=200.0).contains(&t.alpha));
            assert!((75.0 * WIDTH..130.0 * WIDTH).contains(&(t.size * WIDTH)));
            t.life = 20.0;
            assert_eq!(t.rgb(), [200.0 / 255.0; 3]);
            t.life = 8.0;
            assert_eq!(t.alpha(), t.alpha.min(200.0));
        }
        assert!((200.0 / TRAIL_STEP - 2.2).abs() < 0.03);
        // The streak's tail at a quarter of the head's speed (V DLs 120 and 122: 5810/5961, then
        // 5411/5861).
        let launch = Vec3::new(0.0, 0.0, 6011.0);
        let [h, t] = streak(launch, Vec3::new(0.0, 0.0, 5411.0), 3.0);
        assert_eq!((h.z, t.z), (5411.0, 5861.0));
        assert_eq!(streak(launch, launch, 0.0), [launch; 2]);
    }

    /// The flame's puffs: 0.23 a frame, lives 60-80, sizes 134-254, alphas 155-255, and their
    /// mean push (3.2 forward, 3.9 up a frame, V flame DLs 120-130's newborns).
    #[test]
    fn the_flame_puffs_as_the_original() {
        let mut rng = Rng::default();
        let (mut carry, mut n, mut sum) = (0.0, 0, Vec3::ZERO);
        let push = Vec3::new(0.0, PUFF_PUSH[1], PUFF_PUSH[0]);
        for _ in 0..3000 {
            for _ in 0..puff_count(&mut carry, &mut rng) {
                let p = puff(&mut rng, Vec3::ZERO, push);
                assert!((60.0..=80.0).contains(&p.life) && (155.0..=255.0).contains(&p.alpha));
                assert!((134.0..254.0).contains(&p.size) && (-0.25..-0.07).contains(&p.fall));
                (n, sum) = (n + 1, sum + p.vel);
            }
        }
        assert!((n as f32 / 3000.0 - 0.2255).abs() < 0.02, "{n}");
        let mean = sum / n as f32;
        assert!(
            (mean.z - 2.84).abs() < 0.3 && (mean.y - 3.84).abs() < 0.3,
            "{mean}"
        );
    }

    /// The tongues (0.41 a frame, 50-71 VIs, 8-20 wide, rising faster each frame, their tail 3
    /// frames back) and the burning flames (about one a frame, 6-34 wide and twice as tall, 5
    /// frames looping, alphas 80-160, 12-60 VIs).
    #[test]
    fn the_tongues_and_flames_as_the_original() {
        let mut rng = Rng::default();
        let push = Vec3::new(0.0, PUFF_PUSH[1], PUFF_PUSH[0]);
        let (mut carry, mut n) = (0.0, 0);
        for _ in 0..3000 {
            for _ in 0..emit(&mut carry, rng.next_f32() * 0.403 + 0.204) {
                let mut t = tongue(&mut rng, Vec3::ZERO, push);
                assert!((50.0..=71.0).contains(&t.life) && (4.0..=10.0).contains(&t.half));
                assert!((80.0..=255.0).contains(&t.alpha[0]) && t.ends[1] == Vec3::ZERO);
                let rise = t.vel.y;
                t.step(1.0);
                assert!((0.311..=0.62).contains(&(t.vel.y - rise)));
                assert!((t.ends[1] - t.ends[0] - t.vel * TONGUE_TRAIL).length() < 1e-3);
                n += 1;
            }
        }
        assert!((n as f32 / 3000.0 - 0.4055).abs() < 0.02, "{n}");
        let (mut carry, mut n) = (0.0, 0);
        for _ in 0..1000 {
            n += emit(&mut carry, rng.next_f32() * 0.608 + 0.698);
        }
        assert!((n as f32 / 1000.0 - 1.002).abs() < 0.03, "{n}");
        for _ in 0..200 {
            let f = flame(&mut rng);
            let wide = f.size * WIDTH;
            assert!((6.0..=34.0).contains(&wide) && f.tall == 2.0);
            assert!((80.0..=160.0).contains(&f.alpha) && (12.0..=60.0).contains(&f.life));
        }
        let f = Tex::Flame(1, 0);
        assert_eq!(
            (f.at(0.0, 0.23), f.at(5.0, 0.23), f.at(22.0, 0.23)),
            (f, Tex::Flame(1, 1), f)
        );
    }

    /// The explosive death's sparks (8-12, 32°-77° up at 10-19 a frame, 20-30 frames) and Beach's
    /// flak (its flash × 1.028 a VI: V 3153 → 3922 over 4 frames; 5-12 puffs over the sea).
    #[test]
    fn the_sparks_and_flak_as_the_original() {
        let mut rng = Rng::default();
        for _ in 0..100 {
            let sparks = sparks(&mut rng, Vec3::ZERO);
            assert!((8..=12).contains(&sparks.len()));
            for s in sparks {
                let up = (s.vel.y / s.vel.length()).asin().to_degrees();
                assert!((32.0..=78.0).contains(&up) && (10.0..=19.0).contains(&s.vel.length()));
                assert!((40.0..=60.0).contains(&s.life) && (0.07..=0.14).contains(&s.scale));
                assert!(s.spin.x.abs() <= 23.45f32.to_radians() + 1e-6 && s.spin.y == 0.0);
            }
            let (mut f, puffs) = flak(&mut rng);
            assert!((5..=12).contains(&puffs.len()) && puffs.iter().all(|p| p.blend));
            assert!((878.0..=1575.0).contains(&f.pos.y) && (f.alpha() - 255.0).abs() < 0.01);
            let size = f.size;
            (0..4).for_each(|_| _ = f.step(1.0));
            assert!((f.size / size - 3922.0 / 3153.0).abs() < 0.01);
            assert!((f.alpha() - 255.0 * f.life / (f.life + 8.0)).abs() < 0.01);
        }
        // y += vy + g/2, vy += g (the readme's law).
        let mut c = Chunk {
            vel: Vec3::Y * 10.0,
            g: -1.0,
            ..Chunk::new(SPARK, Vec3::ZERO, 1.0, 40.0, 255.0, [12.0, 21.0])
        };
        c.step(1.0);
        assert_eq!((c.pos.y, c.vel.y), (9.5, 9.0));
        c.life = 6.0;
        assert_eq!(c.step(1.0), Some((1.0, 84.0)));
    }

    /// The ripples (80 wide, flat, 10 frames of 4 VIs, 40 VIs, alpha 150) and drops (3-6), at the
    /// stroke's two marks; cs3's bubbles.
    #[test]
    fn the_ripples_and_bubbles_as_the_original() {
        let mut rng = Rng::default();
        let mut r = ripple(&mut rng, Vec3::ZERO);
        assert!((r.flat.unwrap() * Vec3::Z).distance(Vec3::Y) < 1e-5 && r.size == 80.0);
        let mut seen = vec![r.tex.at(r.age, r.rate)];
        while r.step(1.0) {
            seen.push(r.tex.at(r.age, r.rate));
            assert_eq!(r.alpha(), 150.0);
        }
        let frames: Vec<_> = (0..10).flat_map(|k| [Tex::Ripple(k); 2]).collect();
        assert_eq!(seen, frames);
        for _ in 0..50 {
            let d = drops(&mut rng, Vec3::ZERO);
            assert!((3..=6).contains(&d.len()));
            assert!(
                d.iter()
                    .all(|d| d.vel.y / d.vel.length() > 0.79 && d.life >= 46.0)
            );
        }
        assert!(crossed(10.0, 11.0, 10.8) && !crossed(11.0, 12.0, 10.8));
        assert!(crossed(40.0, 2.0, 1.0) && !crossed(26.4, 27.0, 26.4));
        for b in [false, true] {
            let mut p = bubble(&mut rng, Vec3::ZERO, b);
            assert!((31.0..=103.0).contains(&p.size) && (100.0..=200.0).contains(&p.alpha));
            (0..100).for_each(|_| _ = p.step(1.0));
            assert!(p.vel.y <= if b { 4.0 } else { 3.5 });
        }
    }

    /// Race's death: the board's three pieces and the racer's six, thrown along the normal at
    /// (1000·f) 29.8-43 and 5.2-25.2 a frame, the fire and smoke shells; the board trail's length
    /// and taper.
    #[test]
    fn the_race_blast_and_trail_as_the_original() {
        let mut rng = Rng::default();
        for _ in 0..50 {
            let out = race_blast(&mut rng, Vec3::ZERO, Vec3::Y);
            let scenes: Vec<_> = out.iter().map(|c| c.scene).collect();
            assert_eq!(scenes, [5, 6, 7, 11, 10, 9, 9, 8, 8, FIRE, RING]);
            for c in &out[..3] {
                assert!((29.8..=43.0).contains(&c.vel.y) && (30.0..=50.0).contains(&c.life));
            }
            for c in &out[3..9] {
                assert!((5.2..=25.2).contains(&c.vel.y) && (40.0..=80.0).contains(&c.life));
                assert!(c.scale == 0.5 && (-1.089..=-0.463).contains(&c.g));
            }
            let mut fire = out[9];
            let Law::Fire { l, s, .. } = fire.law else {
                panic!();
            };
            let peak = (0..12)
                .filter_map(|_| fire.step(1.0))
                .map(|x| x.0)
                .fold(0.0, f32::max);
            assert!(peak <= s && peak > 0.9 * s && (8.0..=11.0).contains(&l));
        }
        assert_eq!(
            [trail_cap(5.0), trail_cap(40.0), trail_cap(90.0)],
            [0.0, 300.0, 600.0]
        );
        let points: Vec<_> = (0..25).map(|i| Vec3::Z * -50.0 * i as f32).collect();
        let eye = Vec3::Y * 1000.0;
        let mesh = trail_mesh(&points, 300.0, 50.0, eye).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!();
        };
        assert_eq!(pos.len(), 14);
        let pos: Vec<_> = pos.iter().map(|p| Vec3::from_array(*p)).collect();
        assert!((pos[0].distance(pos[1]) - 50.0).abs() < 1e-3 && pos[12] == pos[13]);
        assert!(trail_mesh(&points[..1], 300.0, 50.0, eye).is_none());
    }

    /// Item 0x3A's fuse: bursts of 8-13 sparks (0.19 a frame), 11-15 a frame out, alpha 0 at the
    /// tail and 255 at the head down to life × 31 below 8 VIs, 15-25 VIs; the shell flash 80-140.
    #[test]
    fn the_fuse_as_the_original() {
        let mut rng = Rng::default();
        let (mut carry, mut n) = (0.0, 0);
        for _ in 0..3000 {
            n += emit(&mut carry, rng.next_f32() * 0.117 + 0.129);
        }
        assert!((n as f32 / 3000.0 - 0.1875).abs() < 0.01, "{n}");
        for _ in 0..100 {
            let (sparks, flash) = fuse(&mut rng, Vec3::ZERO);
            assert!((8..=13).contains(&sparks.len()));
            assert!((80.0..=140.0).contains(&flash.size) && (4.0..=8.0).contains(&flash.life));
            for mut s in sparks {
                assert!((11.0..=15.0).contains(&s.vel.length()) && s.alpha == [0.0, 255.0]);
                assert!((15.0..=25.0).contains(&s.life) && (2.75..=4.75).contains(&s.half));
                s.life = 4.0;
                assert_eq!(s.alphas(), [0.0, 124.0]);
            }
        }
    }
}
