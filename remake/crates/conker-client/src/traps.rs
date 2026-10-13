//! Scene 0x34's pressure plates and traps as the original draws them (`../lab/research/mp-original/
//! dm-maps-results.md` §2a-2b; the rules are `conker_rules::traps`', the sounds [`crate::audio`]'s
//! through [`sounding`]). Tags: C code, V the oracle (run fx34: plate 6 pressed at read 120, plate 9
//! at 250, DL dumps 170, 300 and 330), I inferred.
//! - The plates (placements 6-9) at rest + [`MatchView::plates`] ([`plate`]).
//! - A fire jet on (bit k of [`MatchView::traps`], [`fires`]): its flame quad (`func_151A6350`): upright,
//!   turned to the camera, the jet's radius each side of its centre and its height tall, the pack's
//!   `maps/34/traps/flame.png` (blob 0x916) with its S up the jet, scrolling ([`FLAME`]), alpha the
//!   texture's × [`flicker`] at the base fading to none at the top, blended (C, V DL 170). Its
//!   particles (`func_151A6600`): `puff.png` (0xBA6) sprites rising from the base ([`PUFFS`]).
//! - The electric floor on ([`electric`]): bolts (`func_15104C44`) from [`BOLT_TOP`] down to the
//!   pit's floor, and arcs (`func_151050B0`) on each box's base ([`corners`]; I: between points of
//!   two of its edges, where V's arcs are shorter, about 150 long, their ends inside the box):
//!   ribbons of 9 points ([`split`]) across `arc.png` (0x753)'s first row, tinted by the env colour,
//!   blended (C, V DLs 300, 330).
//! - A burning player (a cone's, the flamethrower's): the jets' particles about its body (I: the
//!   original's burning look is not traced).
//!
//! I: not drawn, the electric floor's sparks (texrects of blobs 0x904, 0x908 and 0x1042 in DLs 300,
//! 330), the jets' light (`func_1516284C`).

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use conker_anim::combat::{BURN_END, BURNING, FLAME_HIT};
use conker_camera::volume::CameraVolume;
use conker_rules::Rng;
use conker_rules::traps::{PLATES, Trap};
use conker_sim::net::MatchView;

use crate::Anim;
use crate::render::Model;

/// The flame quad's texture (C `func_151A6068`, `func_151A6350`; V DL 170, with the tile's 0.5
/// scale): it repeats every `unit` of the game's S units up the jet, its S spans `span` (+ r ×
/// `span_r`) and scrolls `scroll` (+ r × `scroll_r`) a frame; across, T runs from 0 to `across` of it.
pub struct Flame {
    pub unit: f32,
    pub span: [f32; 2],
    pub scroll: [f32; 2],
    pub across: f32,
}

pub const FLAME: Flame = Flame {
    unit: 2048.0,
    span: [2700.0, 1300.0],
    scroll: [-380.0, 80.0],
    across: 31.0 / 32.0,
};

/// The jets' particles (`func_151A6600`, C): per frame `rate` (+ r × `rate_r`) of them; each lives
/// `life` + 0..15 frames (I: frames), its alpha `alpha` + 0..100 (V: the texrects' env alpha) fading
/// to none (I), its size `size` + r × `size_r` hundredths of the texture's 16 (I: V 15-42 units at
/// 1,220 in DL 170), rising at `speed` + r × `speed_r` between 42° and 90° up, slowed by `fall` + r
/// × `fall_r` a frame (I: the field's use).
pub struct Puffs {
    pub rate: [f32; 2],
    pub life: f32,
    pub alpha: f32,
    pub size: [f32; 2],
    pub speed: [f32; 2],
    pub fall: [f32; 2],
}

pub const PUFFS: Puffs = Puffs {
    rate: [0.393, 0.437],
    life: 25.0,
    alpha: 100.0,
    size: [0.98, 2.05],
    speed: [10.0, 10.0],
    fall: [0.626, 0.284],
};

/// The electric floor's env colours: its arcs' and its bolts' (C, V).
pub const ARC: [u8; 3] = [0x7E, 0xF9, 0xFF];
pub const BOLT: [u8; 3] = [0x61, 0xF2, 0xFF];
/// The bolts' top (V DL 330: every bolt starts there) and the pit's floor under it (V).
pub const BOLT_TOP: [f32; 3] = [303.0, 39.0, 2591.0];
const FLOOR_Y: f32 = -399.0;
/// The bolts' feet: up to this far from the top's x, z (I; V 203 and 376), at least half of it (C).
const BOLT_REACH: f32 = 400.0;
/// The arcs are drawn this far above their box's base (I: the base is 2 under the pit's floor, which
/// would hide them; V their ends at y −401).
const ARC_LIFT: f32 = 4.0;
/// Per frame: an arc per box `0.04 + 0.07r`, a bolt `0.197 + 0.3r` (C).
const ARC_RATE: [f32; 2] = [0.04, 0.07];
const BOLT_RATE: [f32; 2] = [0.197, 0.3];
/// The ribbons' half-widths (V: about 11 for an arc, 45 for a bolt) and lives in frames (I).
const ARC_WIDTH: f32 = 11.0;
const BOLT_WIDTH: f32 = 45.0;
const ARC_LIFE: f32 = 10.0;
const BOLT_LIFE: f32 = 5.0;
/// A ribbon's points move this much about their place each frame (I: `func_151070F8` puts them on
/// the straight line; V the drawn ones zigzag).
const JITTER: f32 = 20.0;
/// `func_151070F8`'s spread: each midpoint at 0.5 ± this of its span (C).
const SPREAD: f32 = 0.325;

/// The traps of the map (in `conker_rules::traps::parse`'s order: bit k is `list[k]`).
#[derive(Resource)]
struct Traps(Vec<Trap>);

/// The pack's textures and the shared meshes.
#[derive(Resource)]
struct Look {
    quad: Handle<Mesh>,
    sprite: Handle<Mesh>,
    flame: Handle<Image>,
    puff: Handle<Image>,
    arc: Handle<Image>,
}

/// A lit jet's flame quad (trap `k`): its scroll and span in S units, its scroll a frame, the base
/// alpha's phase (a byte) and step a frame, its particles due.
#[derive(Component)]
struct Jet {
    k: usize,
    scroll: f32,
    span: f32,
    speed: f32,
    phase: f32,
    step: f32,
    due: f32,
}

/// A particle: game units and frames.
#[derive(Component)]
struct Puff {
    vel: Vec3,
    fall: f32,
    age: f32,
    life: f32,
    alpha: f32,
}

/// An arc or a bolt: its points, half-width and life.
#[derive(Component)]
struct Ribbon {
    points: [Vec3; 9],
    width: f32,
    age: f32,
    life: f32,
}

/// Plate `k` (0-3) of a level node named `model@placement`.
pub fn plate(name: &str) -> Option<usize> {
    let placement: u16 = name.rsplit_once('@')?.1.parse().ok()?;
    PLATES.iter().position(|&p| p == placement)
}

/// The fire jets on for `bits` (indices into `list`).
pub fn fires(list: &[Trap], bits: u32) -> impl Iterator<Item = usize> + '_ {
    (0..list.len().min(32)).filter(move |&k| bits >> k & 1 != 0 && !list[k].electric)
}

/// The electric floor's boxes on for `bits` (indices into `list`).
pub fn electric(list: &[Trap], bits: u32) -> impl Iterator<Item = usize> + '_ {
    (0..list.len().min(32)).filter(move |&k| bits >> k & 1 != 0 && list[k].electric)
}

/// A trap's sound while it is on: `trap` (its index), sound `id` at `at`, its volume and
/// distances, and the random pitch's mask (cents).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrapSound {
    pub trap: usize,
    pub id: u16,
    pub at: [f32; 3],
    pub volume: u16,
    pub near: u16,
    pub far: u16,
    pub pitch: u32,
}

/// The sounds of the traps on for `bits`: a jet whose command is 1 (the middle of each row of
/// three) plays 0x1AA at its record, 16384, near 750, far 1000, `rand & 0x300` cents (C
/// `func_151A6068`; V fx34: records 28, 31, 64 at the press, stopped with their emitters 120 frames
/// on); the electric floor 0x236 at (303, −417, 2591), 16384, near 1000, far 1500, once for its two
/// boxes (C `func_15104FF8`; V fx34, stopped 150 frames on). Both loop (record flag 8).
pub fn sounding(list: &[Trap], bits: u32) -> impl Iterator<Item = TrapSound> + '_ {
    let fire = fires(list, bits)
        .filter(|&k| list[k].command == 1)
        .map(|k| TrapSound {
            trap: k,
            id: 0x1AA,
            at: list[k].zone.zone.center.map(f32::from),
            volume: 0x4000,
            near: 750,
            far: 1000,
            pitch: 0x300,
        });
    let spark = electric(list, bits).next().map(|k| TrapSound {
        trap: k,
        id: 0x236,
        at: [303.0, -417.0, 2591.0],
        volume: 0x4000,
        near: 1000,
        far: 1500,
        pitch: 0,
    });
    fire.chain(spark)
}

/// The base's alpha out of 255 for its phase (`func_151A6600`: sin((phase − 0x40) & 0xFF) × 63 +
/// 192, the byte a turn; C, V 225 in DL 170).
pub fn flicker(phase: u8) -> f32 {
    let a = f32::from(phase.wrapping_sub(0x40)) / 256.0 * std::f32::consts::TAU;
    a.sin() * 63.0 + 192.0
}

/// A box's base corners in order round it (local (±x, 0, ±z) turned back by its pitch and yaw, as
/// [`CameraVolume::contains`] turns world offsets into the box's frame).
pub fn corners(v: &CameraVolume) -> [Vec3; 4] {
    let ([ps, pc], [ys, yc]) = (v.pitch, v.yaw);
    let [sx, _, sz] = v.size.map(f32::from);
    let c = Vec3::from_array(v.center.map(f32::from));
    [(-sx, -sz), (sx, -sz), (sx, sz), (-sx, sz)].map(|(x, z)| {
        let zr = z * pc;
        let dy = -z * ps;
        c + Vec3::new(x * yc + zr * ys, dy, zr * yc - x * ys)
    })
}

/// `func_151070F8`: the points between `p[lo]` and `p[hi]` by halving, each midpoint on the line at
/// 0.5 ± [`SPREAD`] of its span (C).
pub fn split(p: &mut [Vec3; 9], lo: usize, hi: usize, r: &mut impl FnMut() -> f32) {
    if hi - lo < 2 {
        return;
    }
    let mid = lo + (hi - lo) / 2;
    let t = 0.5 - SPREAD + 2.0 * SPREAD * r();
    p[mid] = p[lo] + (p[hi] - p[lo]) * t;
    split(p, lo, mid, r);
    split(p, mid, hi, r);
}

pub fn build(app: &mut App, triggers: &[u8]) {
    app.insert_resource(Traps(conker_rules::traps::parse(triggers)));
    app.add_systems(Startup, setup);
    app.add_systems(Update, (jets, sparks, burning, puffs, ribbons).chain());
    // After the level's nodes are put back at rest (render's `show_pickups`, in Update).
    app.add_systems(PostUpdate, sink_plates.before(TransformSystems::Propagate));
}

fn setup(mut meshes: ResMut<Assets<Mesh>>, assets: Res<AssetServer>, mut commands: Commands) {
    let load = |name: &str, wrap: bool| -> Handle<Image> {
        let mut sampler = ImageSamplerDescriptor::linear();
        if wrap {
            sampler.address_mode_u = ImageAddressMode::Repeat;
            sampler.address_mode_v = ImageAddressMode::Repeat;
        }
        let sampler = ImageSampler::Descriptor(sampler);
        assets
            .load_builder()
            .with_settings(move |s: &mut ImageLoaderSettings| s.sampler = sampler.clone())
            .load(format!("maps/34/traps/{name}.png"))
    };
    // The flame quad: x ±1, y 0 (the base) to 1; U up the jet, V across; opaque at the base.
    let across = FLAME.across;
    let quad = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, across], [0.0, 0.0], [1.0, 0.0], [1.0, across]],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
        ],
    )
    .with_inserted_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]));
    commands.insert_resource(Look {
        quad: meshes.add(quad),
        sprite: meshes.add(Rectangle::new(1.0, 1.0)),
        flame: load("flame", true),
        puff: load("puff", false),
        arc: load("arc", false),
    });
}

fn material(texture: &Handle<Image>, rgb: [u8; 3], alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgba_u8(rgb[0], rgb[1], rgb[2], 255).with_alpha(alpha),
        base_color_texture: Some(texture.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    }
}

/// The plates at rest + their offsets ([`MatchView::plates`]).
fn sink_plates(
    game: Query<&MatchView>,
    added: Query<(Entity, &Name), Added<Name>>,
    mut plates: Local<Vec<(Entity, usize, f32)>>,
    mut transforms: Query<&mut Transform>,
) {
    for (e, name) in &added {
        if let (Some(k), Ok(t)) = (plate(name), transforms.get(e)) {
            plates.push((e, k, t.translation.y));
        }
    }
    let Ok(m) = game.single() else {
        return;
    };
    for &(e, k, y) in plates.iter() {
        if let Ok(mut t) = transforms.get_mut(e) {
            let y = y + f32::from(m.plates[k]);
            if t.translation.y != y {
                t.translation.y = y;
            }
        }
    }
}

/// The lit jets' flame quads (spawned as a jet lights, gone as it goes off), turned to the camera
/// about the vertical, scrolling and flickering; their particles.
#[allow(clippy::too_many_arguments)]
fn jets(
    time: Res<Time>,
    game: Query<&MatchView>,
    traps: Res<Traps>,
    look: Res<Look>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    mut jets: Query<(
        Entity,
        &mut Jet,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rng: Local<Rng>,
    mut commands: Commands,
) {
    let bits = game.single().map_or(0, |m| m.traps);
    let lit: Vec<usize> = fires(&traps.0, bits).collect();
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    for (e, mut jet, mut t, m) in &mut jets {
        if !lit.contains(&jet.k) {
            commands.entity(e).despawn();
            continue;
        }
        let zone = &traps.0[jet.k].zone.zone;
        jet.scroll = (jet.scroll + jet.speed * frames).rem_euclid(FLAME.unit);
        jet.phase = (jet.phase + jet.step * frames).rem_euclid(256.0);
        let to = eye - t.translation;
        t.rotation = Quat::from_rotation_y(to.x.atan2(to.z));
        if let Some(mut m) = materials.get_mut(&m.0) {
            let scale = Vec2::new(jet.span / FLAME.unit, 1.0);
            let shift = Vec2::new(jet.scroll / FLAME.unit, 0.0);
            m.uv_transform = Affine2::from_scale_angle_translation(scale, 0.0, shift);
            m.base_color.set_alpha(flicker(jet.phase as u8) / 255.0);
        }
        jet.due += (PUFFS.rate[0] + PUFFS.rate[1] * rng.next_f32()) * frames;
        while jet.due > 1.0 {
            jet.due -= 1.0;
            let mut r = || rng.next_f32();
            let (angle, d) = (r() * TAU, r() * f32::from(zone.size[0]));
            let at = t.translation + Vec3::new(angle.cos() * d, 0.0, angle.sin() * d);
            let (yaw, up) = (r() * TAU, (42.0 + 48.0 * r()).to_radians());
            let speed = PUFFS.speed[0] + PUFFS.speed[1] * r();
            let dir = Vec3::new(up.cos() * yaw.cos(), up.sin(), up.cos() * yaw.sin());
            let size = 16.0 * (PUFFS.size[0] + PUFFS.size[1] * r());
            let fall = PUFFS.fall[0] + PUFFS.fall[1] * r();
            let puff = Puff {
                vel: dir * speed,
                fall,
                age: 0.0,
                life: PUFFS.life + (rng.next_u32() & 0xF) as f32,
                alpha: (PUFFS.alpha + (rng.next_u32() % 101) as f32) / 255.0,
            };
            commands.spawn((
                Mesh3d(look.sprite.clone()),
                MeshMaterial3d(materials.add(material(&look.puff, [255; 3], puff.alpha))),
                Transform::from_translation(at).with_scale(Vec3::splat(size)),
                puff,
            ));
        }
    }
    for k in lit {
        if jets.iter().any(|j| j.1.k == k) {
            continue;
        }
        let zone = &traps.0[k].zone.zone;
        let [radius, height, _] = zone.size.map(f32::from);
        let base = Vec3::from_array(zone.center.map(f32::from));
        let jet = Jet {
            k,
            scroll: 0.0,
            span: FLAME.span[0] + FLAME.span[1] * rng.next_f32(),
            speed: FLAME.scroll[0] + FLAME.scroll[1] * rng.next_f32(),
            phase: f32::from(rng.next_u32() as u8),
            step: 2.0 * (4 + (rng.next_u32() & 3)) as f32,
            due: 0.0,
        };
        commands.spawn((
            Mesh3d(look.quad.clone()),
            MeshMaterial3d(materials.add(material(&look.flame, [255; 3], 1.0))),
            Transform::from_translation(base).with_scale(Vec3::new(radius, height, 1.0)),
            jet,
        ));
    }
}

/// The electric floor's bolts and arcs while it is on.
#[allow(clippy::too_many_arguments)]
fn sparks(
    time: Res<Time>,
    game: Query<&MatchView>,
    traps: Res<Traps>,
    look: Res<Look>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    mut due: Local<Vec<f32>>,
    mut rng: Local<Rng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let bits = game.single().map_or(0, |m| m.traps);
    let boxes: Vec<usize> = electric(&traps.0, bits).collect();
    if boxes.is_empty() {
        due.clear();
        return;
    }
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    due.resize(boxes.len() + 1, 0.0);
    let rng = &mut *rng;
    let mut spawn =
        |rng: &mut Rng, ends: [Vec3; 2], rgb: [u8; 3], alpha: f32, width: f32, life: f32| {
            let mut points = [ends[0]; 9];
            points[8] = ends[1];
            split(&mut points, 0, 8, &mut || rng.next_f32());
            commands.spawn((
                Mesh3d(meshes.add(strip(&points, width, eye))),
                MeshMaterial3d(materials.add(material(&look.arc, rgb, alpha / 255.0))),
                Transform::IDENTITY,
                Ribbon {
                    points,
                    width,
                    age: 0.0,
                    life,
                },
            ));
        };
    // The bolts: from the top to the pit's floor, half to all of [`BOLT_REACH`] off (C, I reach).
    due[0] += (BOLT_RATE[0] + BOLT_RATE[1] * rng.next_f32()) * frames;
    while due[0] > 1.0 {
        due[0] -= 1.0;
        let angle = rng.next_f32() * TAU;
        let d = (0.5 + 0.5 * rng.next_f32()) * BOLT_REACH;
        let top = Vec3::from_array(BOLT_TOP);
        let foot = Vec3::new(top.x + angle.cos() * d, FLOOR_Y, top.z + angle.sin() * d);
        let alpha = (100 + rng.next_u32() % 156) as f32;
        spawn(rng, [top, foot], BOLT, alpha, BOLT_WIDTH, BOLT_LIFE);
    }
    // The arcs: per box, from a point of edge i (0-2) to one of edge i + 1 or i + 2 (I).
    for (n, &k) in boxes.iter().enumerate() {
        due[n + 1] += (ARC_RATE[0] + ARC_RATE[1] * rng.next_f32()) * frames;
        while due[n + 1] > 1.0 {
            due[n + 1] -= 1.0;
            let c = corners(&traps.0[k].zone.zone);
            let i = ((rng.next_f32() * 3.0) as usize).min(2);
            let j = (i + 1 + ((rng.next_f32() * 2.0) as usize).min(1)) % 4;
            let on = |e: usize, t: f32| c[e] + (c[(e + 1) % 4] - c[e]) * t + Vec3::Y * ARC_LIFT;
            let ends = [on(i, rng.next_f32()), on(j, rng.next_f32())];
            let alpha = 55.0 + 145.0 * rng.next_f32();
            spawn(rng, ends, ARC, alpha, ARC_WIDTH, ARC_LIFE);
        }
    }
}

/// A burning player (the flame hit, burning, its end: as the sounds' `burning`) gives off the
/// jets' particles about its body, one a frame (I).
fn burning(
    time: Res<Time>,
    look: Res<Look>,
    players: Query<(&Anim, &Transform), With<Model>>,
    mut due: Local<f32>,
    mut rng: Local<Rng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    *due += time.delta_secs() * conker_sim::TICK_HZ as f32;
    let n = due.floor();
    *due -= n;
    for (anim, at) in &players {
        if ![FLAME_HIT, BURNING, BURN_END].contains(&anim.0.id) {
            continue;
        }
        for _ in 0..n as u32 {
            let mut r = || rng.next_f32() * 2.0 - 1.0;
            let at = at.translation + Vec3::new(25.0 * r(), 45.0 + 35.0 * r(), 25.0 * r());
            let puff = Puff {
                vel: Vec3::new(r(), 3.0 + r(), r()),
                fall: 0.0,
                age: 0.0,
                life: 20.0 + 5.0 * r(),
                alpha: 0.6,
            };
            commands.spawn((
                Mesh3d(look.sprite.clone()),
                MeshMaterial3d(materials.add(material(&look.puff, [255; 3], puff.alpha))),
                Transform::from_translation(at).with_scale(Vec3::splat(24.0 + 8.0 * r())),
                puff,
            ));
        }
    }
}

/// The particles rise, slow and fade, facing the camera; gone at the end of their life.
#[allow(clippy::type_complexity)]
fn puffs(
    time: Res<Time>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    mut puffs: Query<(
        Entity,
        &mut Puff,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    for (e, mut p, mut t, m) in &mut puffs {
        p.age += frames;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.fall * frames;
        t.translation += p.vel * frames;
        t.rotation = facing;
        if let Some(mut m) = materials.get_mut(&m.0) {
            m.base_color.set_alpha(p.alpha * (1.0 - p.age / p.life));
        }
    }
}

/// The arcs and bolts: each frame a ribbon through their points moved about ([`JITTER`]), facing
/// the camera; gone at the end of their life.
fn ribbons(
    time: Res<Time>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    mut ribbons: Query<(Entity, &mut Ribbon, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: Local<Rng>,
    mut commands: Commands,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    let eye = camera.single().map_or(Vec3::ZERO, |c| c.translation());
    for (e, mut ribbon, mesh) in &mut ribbons {
        ribbon.age += frames;
        if ribbon.age >= ribbon.life {
            commands.entity(e).despawn();
            continue;
        }
        let mut r = || (rng.next_f32() * 2.0 - 1.0) * JITTER;
        let mut p = ribbon.points;
        for q in &mut p[1..8] {
            *q += Vec3::new(r(), r(), r());
        }
        if let Some(mut m) = meshes.get_mut(&mesh.0) {
            *m = strip(&p, ribbon.width, eye);
        }
    }
}

/// A ribbon three vertices across each point (U 0, ½, 1 on the texture's first row), `width` each
/// side, its breadth square to the line and to the eye.
fn strip(p: &[Vec3; 9], width: f32, eye: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(27);
    let mut uvs = Vec::with_capacity(27);
    for (i, &q) in p.iter().enumerate() {
        let along = p[(i + 1).min(8)] - p[i.saturating_sub(1)];
        let side = along.cross(eye - q).normalize_or_zero() * width;
        for (k, v) in [q - side, q, q + side].into_iter().enumerate() {
            positions.push(v.to_array());
            uvs.push([k as f32 / 2.0, 1.0 / 32.0]);
        }
    }
    let mut indices = Vec::with_capacity(8 * 12);
    for i in 0..8u16 {
        let (a, b) = (3 * i, 3 * i + 3);
        for s in 0..2 {
            indices.extend([a + s, a + s + 1, b + s + 1, a + s, b + s + 1, b + s]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 27])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U16(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind-12 record (action 9): arg 4 a fire cylinder with its id, arg 8 an electric box.
    fn record(
        center: [i16; 3],
        size: [i16; 3],
        shape: u8,
        arg: u32,
        id: u32,
        command: u32,
    ) -> [u8; 0x34] {
        let mut r = [0u8; 0x34];
        for (k, v) in center.iter().chain(&size).enumerate() {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        (r[0x15], r[0x17]) = (12 << 2 | shape, 9);
        r[0x18..0x1C].copy_from_slice(&arg.to_be_bytes());
        r[0x1C..0x20].copy_from_slice(&id.to_be_bytes());
        r[0x20..0x24].copy_from_slice(&command.to_be_bytes());
        r
    }

    /// Scene 0x34's records 27-29 (plate 6's first row), a record that is no trap, the electric box 43.
    fn table() -> Vec<u8> {
        [
            record([-2442, -382, 3257], [68, 312, 237], 0, 4, 0, 0),
            record([-2587, -379, 3247], [68, 312, 237], 0, 4, 1, 1),
            [0; 0x34],
            record([-2710, -379, 3231], [68, 312, 237], 0, 4, 2, 0),
            record([301, -401, 2624], [984, 81, 492], 2, 8, 0, 0),
        ]
        .concat()
    }

    /// The bits light the jets and the electric floor they name; the middle jet of the row and the
    /// floor sound; the plates' nodes.
    #[test]
    fn the_bits_say_what_is_drawn_and_heard() {
        let list = conker_rules::traps::parse(&table());
        assert_eq!(
            list.iter().map(|t| t.record).collect::<Vec<_>>(),
            [0, 1, 3, 4]
        );
        assert_eq!(fires(&list, 0b0101).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(electric(&list, 0b0101).count(), 0);
        assert_eq!(electric(&list, 0b1000).collect::<Vec<_>>(), [3]);
        assert_eq!(fires(&list, 0b1000).count(), 0);
        let ids = |bits| {
            sounding(&list, bits)
                .map(|s| (s.trap, s.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(0b0111), [(1, 0x1AA)]);
        assert_eq!(ids(0b0101), []);
        assert_eq!(ids(0b1010), [(1, 0x1AA), (3, 0x236)]);
        let s = sounding(&list, 0b0010).next().unwrap();
        assert_eq!(
            (s.at, s.near, s.far),
            ([-2587.0, -379.0, 3247.0], 750, 1000)
        );
        assert_eq!(
            ["04-34-06@6", "04-34-07@9", "04-34-08@10", "03-56@13", "x"].map(plate),
            [Some(0), Some(3), None, None, None]
        );
    }

    /// The boxes' base corners (record 43 square to the axes; record 82 turned: just inside each
    /// corner is in its box, just outside is not), the flicker's range, the midpoints on the line.
    #[test]
    fn corners_flicker_and_splits() {
        let list = conker_rules::traps::parse(&table());
        let c = corners(&list[3].zone.zone);
        assert_eq!(c[0], Vec3::new(301.0 - 984.0, -401.0, 2624.0 - 492.0));
        assert_eq!(c[2], Vec3::new(301.0 + 984.0, -401.0, 2624.0 + 492.0));
        let mut r82 = record([216, -269, 2294], [177, 81, 520], 2, 8, 0, 0);
        r82[0xC..0x10].copy_from_slice(&19.6f32.to_be_bytes());
        r82[0x10..0x14].copy_from_slice(&89.4f32.to_be_bytes());
        let v = CameraVolume::parse_record(&r82);
        let c = corners(&v);
        let centre = c.iter().sum::<Vec3>() / 4.0;
        for p in c {
            let near = |out: f32| (p + (p - centre) * out + Vec3::Y * 3.0).to_array();
            assert!(v.contains(near(-0.05)) && !v.contains(near(0.05)), "{p}");
        }
        assert!(((c[1] - c[0]).length() - 354.0).abs() < 0.5);
        assert!(((c[2] - c[1]).length() - 1040.0).abs() < 0.5);
        let flickers = [0x40u8, 0x80, 0xC0, 0].map(|p| flicker(p).round());
        assert_eq!(flickers, [192.0, 255.0, 192.0, 129.0]);
        let mut p = [Vec3::ZERO; 9];
        p[8] = Vec3::new(800.0, 0.0, 0.0);
        let mut n = 0.0;
        split(&mut p, 0, 8, &mut || {
            n = 1.0 - n;
            n
        });
        assert!(p.iter().all(|q| q.y == 0.0 && q.z == 0.0));
        assert!(p.windows(2).all(|w| w[0].x < w[1].x));
        assert!((p[4].x - 800.0 * (0.5 + SPREAD)).abs() < 1e-3);
    }
}
