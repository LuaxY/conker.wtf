//! The effects the original draws with its 2D sprites, and the rocket's streak
//! (`../lab/research/mp-original/effects-g4.md`: oracle runs twin, rocket and flame on 0x2D, slot
//! 0 at (−2247, −521, 6015) facing 16896). Tags: V the oracle's RAM and display lists, C its code,
//! I inferred.
//!
//! A 2D sprite ([`Particle`], `func_15130280`/`func_151303EC`, C) is a camera-facing square of
//! [`WIDTH`] × its size, unlit and alpha-blended, depth-tested without writing depth (the
//! original's texrect at its own depth). Its life counts VIs, 2 a frame.
//! - The guns' flashes ([`flash`]): a player's shot count (`Combat::shots`) going up with a gun
//!   drawn; the rocket launcher's pair ([`rocket_flashes`]).
//! - The rocket's glow on it ([`glow`]), its trail ([`trail`]) and its streak ([`streak`]); its
//!   blast's smoke ([`blast`]) where it, or a grenade (I), goes out of the match's lists.
//! - The flamethrower's puffs ([`puff`]) while its fire clip plays.
//!
//! I: not drawn, what the pack lacks (the guns' 3D flash 0x3CF, the bullets' tracers 0x11FE, the
//! flame's tongues 0xBA7, the blast's ring outside Beach, its light and scorch mark).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use conker_anim::combat::FLAME_FIRE;
use conker_rules::Rng;
use conker_rules::weapon::{Attack, weapon};
use conker_sim::net::{Combat, MatchView};
use lightyear::prelude::LocalTimeline;

use crate::projectiles::FLAME;
use crate::render::Model;

/// A 2D sprite's world width per unit of its size (I: the texrect's half-size in pixels is
/// size / w × 16.949 × the viewport's height / 216; at the remake's 50° vertical FOV).
pub const WIDTH: f32 = 0.1464;

/// The textures: `hud/race_exhaust_0..3.png` (kinds 0x60-0x63: 0x923-0x926), `hud/explosion.png`
/// (kind 0x27: 0x78A), `puff.png` (kind 0x28: 0xBA6, packed with 0x34's traps), `hud/fire.png`
/// (kind 0x6C: 0x71F), `hud/shell.png` (0x1042).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tex {
    Exhaust(usize),
    Explosion,
    Puff,
    Fire,
    Shell,
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
        self.vel.y += self.fall * frames;
        self.pos += self.vel * frames;
        self.vel *= self.drag.powf(frames);
        if self.life < self.grow[0] {
            self.size *= self.grow[1].powf(2.0 * frames);
        }
        self.life > 0.0
    }
}

fn flips(rng: &mut Rng) -> [bool; 2] {
    [rng.next_u32() & 1 == 1, rng.next_u32() & 1 == 1]
}

fn exhaust(rng: &mut Rng) -> Tex {
    Tex::Exhaust((rng.next_u32() & 3) as usize)
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

/// Particles other modules make (0x34's arc glows), spawned by [`watch`].
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
    *carry += rng.next_f32() * 0.351 + 0.05;
    let n = carry.floor().max(0.0);
    *carry -= n;
    n as u32
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

/// A rocket seen in flight: its position, launch point, first frame, the trail's carry and its
/// glow's and streak's entities.
struct Rocket {
    pos: Vec3,
    launch: Vec3,
    born: u32,
    carry: f32,
    glow: Entity,
    streak: Entity,
}

#[derive(Resource)]
struct Looks {
    quad: Handle<Mesh>,
    exhaust: [Handle<Image>; 4],
    explosion: Handle<Image>,
    puff: Handle<Image>,
    fire: Handle<Image>,
    shell: Handle<Image>,
}

impl Looks {
    fn texture(&self, t: Tex) -> Handle<Image> {
        match t {
            Tex::Exhaust(k) => self.exhaust[k].clone(),
            Tex::Explosion => self.explosion.clone(),
            Tex::Puff => self.puff.clone(),
            Tex::Fire => self.fire.clone(),
            Tex::Shell => self.shell.clone(),
        }
    }
}

/// What the triggers saw last: each player's shot count and flame carry, the rockets, the
/// grenades, the frame.
#[derive(Resource, Default)]
struct Seen {
    shots: HashMap<Entity, u16>,
    flames: HashMap<Entity, f32>,
    rockets: Vec<Rocket>,
    grenades: Vec<Vec3>,
    smokes: Vec<Smoke>,
    tick: u32,
    rng: Rng,
}

pub fn build(app: &mut App) {
    app.init_resource::<Seen>();
    app.init_resource::<Queue>();
    app.add_systems(Startup, setup);
    app.add_systems(Update, (watch, rocket_streaks, sprites).chain());
}

fn setup(mut meshes: ResMut<Assets<Mesh>>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(Looks {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        exhaust: std::array::from_fn(|k| assets.load(format!("hud/race_exhaust_{k}.png"))),
        explosion: assets.load("hud/explosion.png"),
        // ponytail: 0x34's copy; G5 moves 0xBA6 to a shared folder.
        puff: assets.load("maps/34/traps/puff.png"),
        fire: assets.load("hud/fire.png"),
        shell: assets.load("hud/shell.png"),
    });
}

fn spawn(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    looks: &Looks,
    s: Particle,
) -> Entity {
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE.with_alpha(0.0),
        base_color_texture: Some(looks.texture(s.tex)),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    let at = Transform::from_translation(s.pos + s.offset).with_scale(Vec3::ZERO);
    (commands.spawn((s, Mesh3d(looks.quad.clone()), MeshMaterial3d(material), at))).id()
}

/// The triggers, once a frame: shots, rockets (new, flying, gone), grenades gone, flames.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn watch(
    game: Query<&MatchView>,
    players: Query<(Entity, &Combat, &Transform), With<Model>>,
    timeline: Res<LocalTimeline>,
    looks: Option<Res<Looks>>,
    mut seen: ResMut<Seen>,
    mut queue: ResMut<Queue>,
    mut glows: Query<&mut Particle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let (Some(looks), s) = (looks, &mut *seen) else {
        return;
    };
    for p in queue.0.drain(..) {
        spawn(&mut commands, &mut materials, &looks, p);
    }
    if tick == s.tick {
        return;
    }
    let frames = tick.saturating_sub(s.tick).clamp(1, 30);
    s.tick = tick;
    let mut new = Vec::new();
    for (e, combat, at) in &players {
        let local = |[f, side, u]: [f32; 3]| at.translation + at.rotation * Vec3::new(-side, u, f);
        let rocket =
            weapon(combat.weapon).is_some_and(|w| matches!(w.attack, Attack::Rocket { .. }));
        let last = s.shots.insert(e, combat.shots).filter(|_| combat.drawn);
        let shot = last.map_or(0, |l| combat.shots.saturating_sub(l)).min(4);
        for n in (0..shot).map(|k| combat.shots - k) {
            for p in flash_points(combat.weapon, n) {
                new.push(match rocket {
                    true => rocket_flash(&mut s.rng, local(p)),
                    false => flash(&mut s.rng, local(p)),
                });
            }
        }
        let carry = s.flames.entry(e).or_default();
        if combat.anim == FLAME_FIRE {
            let push = at.rotation * Vec3::new(0.0, PUFF_PUSH[1], PUFF_PUSH[0]);
            for _ in 0..frames {
                for _ in 0..puff_count(carry, &mut s.rng) {
                    new.push(puff(&mut s.rng, local(FLAME.nozzle), push));
                }
            }
        }
    }
    let (rockets, grenades) = game.single().map_or((Vec::new(), Vec::new()), |m| {
        let v = |l: &Vec<[f32; 3]>| l.iter().map(|p| Vec3::from_array(*p)).collect::<Vec<_>>();
        (v(&m.rockets), v(&m.grenades))
    });
    // Each list keeps its order (new ones last): a rocket is the next within reach of where it
    // was (as `projectiles::fly`).
    let mut next = rockets.iter().copied().peekable();
    let mut kept = Vec::new();
    for mut r in std::mem::take(&mut s.rockets) {
        match next.next_if(|p| p.distance(r.pos) <= 400.0 * frames as f32) {
            Some(p) => {
                let step = (p - r.pos) / frames as f32;
                r.carry += p.distance(r.pos);
                while r.carry >= TRAIL_STEP {
                    r.carry -= TRAIL_STEP;
                    new.push(trail(&mut s.rng, p, step));
                }
                if let Ok(mut g) = glows.get_mut(r.glow) {
                    g.pos = p;
                }
                r.pos = p;
                kept.push(r);
            }
            None => {
                commands.entity(r.glow).despawn();
                commands.entity(r.streak).despawn();
                // I: the columns rise (V rocket DL 125), the struck face's normal not looked up.
                new.extend(blast(&mut s.rng, r.pos, Vec3::Y));
                let left = smoke_frames(&mut s.rng);
                s.smokes.push(Smoke {
                    at: r.pos,
                    left,
                    carry: 0.0,
                });
            }
        }
    }
    for p in next {
        let glow = spawn(&mut commands, &mut materials, &looks, glow(&mut s.rng, p));
        let [r, g, b, a] = STREAK_RGBA;
        let streak = commands.spawn((
            Mesh3d(meshes.add(streak_mesh([p, p], Vec3::ZERO))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba_u8(r, g, b, a),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })),
            Transform::IDENTITY,
        ));
        kept.push(Rocket {
            pos: p,
            launch: p,
            born: tick,
            carry: 0.0,
            glow,
            streak: streak.id(),
        });
    }
    s.rockets = kept;
    // A grenade gone from the list went off where it was (I: `func_150D728C`, which fills the
    // rocket's numbers with `func_151BFC40`, is taken to be the grenade's).
    for g in &s.grenades {
        if !grenades
            .iter()
            .any(|n| n.distance(*g) <= 60.0 * frames as f32)
        {
            new.extend(blast(&mut s.rng, *g, Vec3::Y));
        }
    }
    s.grenades = grenades;
    for m in &mut s.smokes {
        for _ in 0..frames.min(m.left.ceil() as u32) {
            m.carry += s.rng.next_f32() * 0.0879 + 0.1;
            while m.carry > 1.0 {
                m.carry -= 1.0;
                new.push(smoke(&mut s.rng, m.at));
            }
        }
        m.left -= frames as f32;
    }
    s.smokes.retain(|m| m.left > 0.0);
    s.shots.retain(|e, _| players.contains(*e));
    s.flames.retain(|e, _| players.contains(*e));
    for p in new {
        spawn(&mut commands, &mut materials, &looks, p);
    }
}

/// The streaks' quads, each frame toward the eye.
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
        let ends = streak(r.launch, r.pos, tick.saturating_sub(r.born) as f32);
        if let Ok(m) = streaks.get(r.streak)
            && let Some(mut mesh) = meshes.get_mut(&m.0)
        {
            *mesh = streak_mesh(ends, eye);
        }
    }
}

/// A quad from `head` to `tail` facing `eye`, [`STREAK_WIDTH`] wide, opaque at the head.
fn streak_mesh([head, tail]: [Vec3; 2], eye: Vec3) -> Mesh {
    let side = (head - tail).cross(eye - head).normalize_or_zero() * STREAK_WIDTH / 2.0;
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![head - side, head + side, tail + side, tail - side],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
        ],
    )
    .with_inserted_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]))
}

/// The sprites move, face the camera and fade; gone at the end of their life.
#[allow(clippy::type_complexity)]
fn sprites(
    time: Res<Time>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
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
        let w = s.size * WIDTH;
        let flip = s.flip.map(|f| if f { -1.0 } else { 1.0 });
        *t = Transform::from_translation(s.pos + s.offset)
            .with_rotation(facing)
            .with_scale(Vec3::new(w * flip[0], w * flip[1], 1.0));
        if let Some(mut m) = materials.get_mut(&m.0) {
            let [r, g, b] = s.rgb();
            m.base_color = Color::srgba(r, g, b, s.alpha() / 255.0);
        }
    }
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
}
