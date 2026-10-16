//! The projectiles in flight and the flamethrower's stream as the original draws them
//! (`lab/research/mp-original/visuals-scratch`: oracle runs on 0x2D, slot 0 given the weapon by
//! a poke; twin, rocket, gren4, knife, bolt2, flame). Tags: V the oracle's display lists and RAM,
//! I inferred. The game rebuilds a display list every other frame while its objects move every
//! frame, so a model's change between two lists is two frames' (V: the rocket 400, `ROCKET_SPEED`
//! a frame 200); the rates here are a frame's.
//! - Bullets: no model (V twin DLs 104-110: no batch along their path).
//! - The others' models ([`conker_assets::projectiles_gltf`]) with +z along their flight, the step
//!   between two views of the [`MatchView`] (a projectile shows from its second, I): the rocket at
//!   0.047, nose ahead (V rocket DLs 120-124); the thrown grenade at 0.5, pointing along the ground
//!   and rolling forward 32.7° a frame about its side from 32.7° up (V gren4 DLs 119-129, its frames
//!   16-26: within 0.6°), until its first bounce, then settling on its side along the throw (V gren4
//!   DLs 129-159: −98° on its last frame in the air, then −59°, −17°, … to 3°; here a decay,
//!   [`SETTLE`]); weapon 0x14's knife at 0.5,
//!   rolling forward 30° a frame from 40° up (V knife DLs 121-132); the crossbow's bolt at 0.5,
//!   turning 30° a frame about its length from 60° (V bolt2 DLs 151-158). A knife and a bolt share
//!   the shots' list: the crossbow's are bolts ([`look`]; the shooter is the player nearest where
//!   it appears). On 0x1A and 0x3F the grenades are item 0x3A's ([`LOB`]): its rig at clip 0 (its
//!   empty pair's pose, the bent fuse), at its own scale 1.0, 28 above the object (its obj+0xC8),
//!   upright with +z along the throw, never turning, through the bounces and at rest (V
//!   `held-client-scratch` l1a DLs 133-220, l3f DLs 140-200).
//! - The flame stream (V flame DLs 106-170, the stream's 15 records at 0x8021DFF0 and its ribbon's
//!   batches): while the fire clip plays, from [`conker_rules::flame::DELAY`] frames on, a flame
//!   point a frame leaves the nozzle ([`NOZZLE`]); the ribbon runs from the nozzle through the
//!   points, youngest first, [`FLAME`]'s width each, facing the eye, `flame.png` along it.
//!
//! The guns' flashes, the rocket's glow, streak and trail, the blasts and the stream's puffs are
//! [`crate::effects`]'. I: not drawn, the stream's tongues (ribbons of 0xBA7, not in the pack).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_anim::combat::FLAME_FIRE;
use conker_assets::{PACK_WEAPONS, flame_png, projectiles_gltf};
use conker_rules::flame::DELAY;
use conker_rules::projectile::raycast;
use conker_sim::net::{Collision, Combat, MatchView};
use lightyear::prelude::LocalTimeline;

use crate::held::{LOB as LOB_RIG, Lob, Rigged, pose_rig, rig_ready};
use crate::render::{MODEL_SCALE, Model};

/// What a projectile's model is: a glTF scene of `projectiles.gltf`, or item 0x3A's rig
/// ([`Rigged`]) showing its list `k` in clip 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Scene(usize),
    Lob(usize),
}

/// How a projectile is drawn: its model, its scale, how far above its position (model units),
/// its pitch (degrees up from its flight) and roll (about its length) on its first frame, and
/// their change a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub shape: Shape,
    pub scale: f32,
    pub lift: f32,
    pub pitch: [f32; 2],
    pub roll: [f32; 2],
}

pub const GRENADE: Look = Look {
    shape: Shape::Scene(0),
    scale: MODEL_SCALE,
    lift: 0.0,
    pitch: [32.7, -32.7],
    roll: [0.0; 2],
};
pub const ROCKET: Look = Look {
    shape: Shape::Scene(1),
    scale: 0.047,
    lift: 0.0,
    pitch: [0.0; 2],
    roll: [0.0; 2],
};
pub const KNIFE: Look = Look {
    shape: Shape::Scene(2),
    scale: MODEL_SCALE,
    lift: 0.0,
    pitch: [40.0, -30.0],
    roll: [0.0; 2],
};
/// The bolt's scene (also the crossbow's loaded bolt, [`crate::held`]).
pub const BOLT_SCENE: usize = 3;
pub const BOLT: Look = Look {
    shape: Shape::Scene(BOLT_SCENE),
    scale: MODEL_SCALE,
    lift: 0.0,
    pitch: [0.0; 2],
    roll: [60.0, -30.0],
};
/// Item 0x3A's thrown grenade, its rig's list `k` (V l1a, l3f: its joint 0 at scale 1.0, 28.0
/// above the object, obj+0xC8).
pub const fn lob(k: usize) -> Look {
    Look {
        shape: Shape::Lob(k),
        scale: 1.0,
        lift: 28.0,
        pitch: [0.0; 2],
        roll: [0.0; 2],
    }
}

/// A landed grenade's pitch keeps this much of itself a frame (I: V's damped wobble, −98° at the
/// bounce, then −59°, −17°, −18°, 7°, 12°, −3° every other frame).
pub const SETTLE: f32 = 0.775;

/// The look of list `k` of the [`MatchView`] (the shots, the grenades, the rockets) fired with
/// `weapon`; `lob`: item 0x3A's list where C-down lobs it ([`Lob`]).
pub fn look(k: usize, weapon: u8, lob: Option<usize>) -> Look {
    match (k, lob) {
        (0, _) if weapon == 0x18 => BOLT,
        (0, _) => KNIFE,
        (1, Some(list)) => self::lob(list),
        (1, None) => GRENADE,
        _ => ROCKET,
    }
}

/// A projectile seen in flight.
struct Flight {
    entity: Entity,
    pos: Vec3,
    /// Its motion a frame, once seen twice.
    vel: Option<Vec3>,
    look: Look,
    /// The way its model points: a grenade's along the ground, the others' along their flight.
    heading: Vec3,
    born: u32,
    /// A grenade's first bounce: its frame and pitch then.
    landed: Option<(u32, f32)>,
}

#[derive(Resource, Default)]
pub(crate) struct Flights {
    lists: [Vec<Flight>; 3],
    seen: [Vec<[f32; 3]>; 3],
    tick: u32,
    scenes: Vec<Handle<WorldAsset>>,
}

/// A projectile's model (scene and pose set by [`fly`]).
#[derive(Component)]
pub struct Projectile;

pub fn build(app: &mut App) {
    app.init_resource::<Flights>();
    app.add_systems(Update, pose_lobs);
}

/// Item 0x3A's thrown grenades show their rig's clip 0 (the game's once thrown, V p1a: clip 0,
/// clock 0 from the throw's first frame).
fn pose_lobs(
    lobs: Query<&Rigged, With<Projectile>>,
    mut animators: Query<&mut AnimationPlayer>,
    mut joints: Query<&mut Transform>,
) {
    for rig in &lobs {
        pose_rig(rig, (0, 0.0), &mut animators, &mut joints);
    }
}

/// The pose of a projectile with look `l`, moving along `dir`, `age` frames after it was first
/// seen; a grenade that bounced `landed` = (frames ago, pitch then) settles.
pub fn pose(l: &Look, dir: Vec3, age: f32, landed: Option<(f32, f32)>) -> Quat {
    let base = Transform::IDENTITY.looking_to(-dir, Vec3::Y).rotation;
    let pitch = match landed {
        Some((since, at)) => at * SETTLE.powf(since),
        None => l.pitch[0] + l.pitch[1] * age,
    };
    let roll = l.roll[0] + l.roll[1] * age;
    base * Quat::from_rotation_x(-pitch.to_radians()) * Quat::from_rotation_z(roll.to_radians())
}

/// Follows the [`MatchView`]'s shots, grenades and rockets: each list keeps its order (new ones go
/// last), so each projectile is the next of the new list within reach of where it was; then poses
/// their models.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn fly(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    assets: Res<AssetServer>,
    lob: Res<Lob>,
    mut flights: ResMut<Flights>,
    mut models: Query<(&mut Transform, &mut Visibility), With<Projectile>>,
    players: Query<(&Transform, &Combat), (With<Model>, Without<Projectile>)>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let f = &mut *flights;
    if f.scenes.is_empty() {
        let path = projectiles_gltf(PACK_WEAPONS);
        f.scenes = (0..4)
            .map(|i| assets.load(format!("{path}#Scene{i}")))
            .collect();
    }
    let empty = [Vec::new(), Vec::new(), Vec::new()];
    let lists = game.single().map_or(empty, |m| {
        [m.shots.clone(), m.grenades.clone(), m.rockets.clone()]
    });
    let frames = tick.saturating_sub(f.tick).max(1) as f32;
    for (k, new) in lists.iter().enumerate() {
        if f.seen[k] == *new {
            continue;
        }
        let reach = [2.0 * conker_rules::projectile::BOLT_STEP, 60.0, 400.0][k] * frames;
        let mut next = new.iter().map(|p| Vec3::from_array(*p)).peekable();
        let mut kept = Vec::new();
        for mut fl in std::mem::take(&mut f.lists[k]) {
            match next.next_if(|p| p.distance(fl.pos) <= reach) {
                Some(p) => {
                    let vel = (p - fl.pos) / frames;
                    // Falling, then not: a bounce (the views' steps are uneven, so not a rise).
                    let bounced = (fl.vel).is_some_and(|v| k == 1 && v.y < 0.0 && vel.y >= 0.0);
                    if bounced && fl.landed.is_none() {
                        // It settles from its last frame in the air, two before the bounce shows.
                        let at = tick.saturating_sub(2);
                        let age = at.saturating_sub(fl.born) as f32;
                        fl.landed = Some((at, pose_pitch(&fl.look, age)));
                    }
                    fl.vel = Some(vel);
                    let along = if k == 1 { vel.with_y(0.0) } else { vel };
                    if along.length() > 0.5 {
                        fl.heading = along;
                    }
                    fl.pos = p;
                    kept.push(fl);
                }
                None => commands.entity(fl.entity).despawn(),
            }
        }
        for p in next {
            let shooter = players.iter().min_by(|a, b| {
                (a.0.translation.distance(p)).total_cmp(&b.0.translation.distance(p))
            });
            let entity = commands.spawn((
                Projectile,
                Transform::from_translation(p),
                Visibility::Hidden,
            ));
            kept.push(Flight {
                entity: entity.id(),
                pos: p,
                vel: None,
                look: look(k, shooter.map_or(0, |s| s.1.weapon), lob.0),
                heading: Vec3::Z,
                born: tick,
                landed: None,
            });
        }
        f.lists[k] = kept;
        f.seen[k] = new.clone();
    }
    f.tick = tick;
    for list in &f.lists {
        for fl in list {
            if fl.vel.is_none() {
                continue;
            }
            let l = fl.look;
            let age = tick.saturating_sub(fl.born) as f32;
            let landed = fl.landed.map(|(at, p)| (tick.saturating_sub(at) as f32, p));
            if let Ok((mut t, mut v)) = models.get_mut(fl.entity) {
                if *v == Visibility::Hidden {
                    let mut e = commands.entity(fl.entity);
                    match l.shape {
                        Shape::Scene(s) => e.insert(WorldAssetRoot(f.scenes[s].clone())),
                        Shape::Lob(k) => e
                            .insert(Rigged::spawn(LOB_RIG, k, &assets))
                            .observe(rig_ready),
                    };
                    *v = Visibility::Inherited;
                }
                *t = Transform::from_translation(fl.pos + Vec3::Y * l.lift)
                    .with_rotation(pose(&l, fl.heading, age, landed))
                    .with_scale(Vec3::splat(l.scale));
            }
        }
    }
}

/// A grenade's pitch `age` frames into its flight, wrapped to ±180°.
fn pose_pitch(l: &Look, age: f32) -> f32 {
    (l.pitch[0] + l.pitch[1] * age + 180.0).rem_euclid(360.0) - 180.0
}

/// The flame stream's numbers (V flame DLs 106-130; frames as above).
pub struct Flame {
    /// The nozzle in the player's frame (forward, side, up), where the ribbon starts.
    pub nozzle: [f32; 3],
    /// The stream's turn left of the facing (u16 units: 4.8°).
    pub turn: u16,
    /// A point's speed along the stream a frame, its rise a frame at first, the rise it tends to
    /// and the share of the gap it closes a frame.
    pub speed: f32,
    pub rise: [f32; 3],
    /// The ribbon's width at the nozzle, at a point on its first frame, and its growth a frame.
    pub width: [f32; 3],
    /// A point's alpha (of 255), the frames it keeps it, and the frames it then fades over.
    pub alpha: f32,
    pub hold: f32,
    pub fade: f32,
    /// The texture's repeat along the stream (units) and its scroll a frame (repeats).
    pub repeat: f32,
    pub scroll: f32,
    /// T across the ribbon, from 0 to this (as the jets', `traps::FLAME`).
    pub across: f32,
}

pub const FLAME: Flame = Flame {
    nozzle: [74.1, 9.9, 52.0],
    turn: 870,
    speed: 41.7,
    rise: [9.1, 16.5, 0.16],
    width: [16.0, 16.0, 6.6],
    alpha: 200.0,
    hold: 9.0,
    fade: 7.0,
    repeat: 2048.0 / 9.4,
    scroll: -234.0 / 2048.0,
    across: 31.0 / 32.0,
};

/// A point of the stream; `room` is how far it may still go before the first wall ahead of the
/// nozzle when it left (I: as the rules' hits, `conker_rules::flame::reach`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub room: f32,
}

impl Point {
    /// Its width and alpha (0-1).
    pub fn look(&self) -> (f32, f32) {
        let width = FLAME.width[1] + FLAME.width[2] * (self.age - 1.0);
        let fade = ((self.age - FLAME.hold) / FLAME.fade).clamp(0.0, 1.0);
        (width, FLAME.alpha / 255.0 * (1.0 - fade))
    }

    pub fn alive(&self) -> bool {
        self.age < FLAME.hold + FLAME.fade && self.room >= 0.0
    }
}

/// One frame of a stream: its points move and age, then, firing, a new one leaves `nozzle` along
/// `dir` (horizontal, unit) with a wall `wall` ahead.
pub fn step_stream(points: &mut Vec<Point>, emit: Option<(Vec3, Vec3, f32)>) {
    for p in points.iter_mut() {
        p.pos += p.vel;
        p.vel.y += (FLAME.rise[1] - p.vel.y) * FLAME.rise[2];
        p.age += 1.0;
        p.room -= FLAME.speed;
    }
    points.retain(Point::alive);
    if let Some((nozzle, dir, wall)) = emit {
        let vel = dir * FLAME.speed + Vec3::Y * FLAME.rise[0];
        points.push(Point {
            pos: nozzle + vel,
            vel,
            age: 1.0,
            room: wall - FLAME.speed,
        });
    }
}

/// Past the stream's reach (V: 16 frames of 41.7).
const RANGE: f32 = 1000.0;

/// A player's flame stream: its points, the frames its fire clip has played, the frame it last
/// stepped and its ribbon's mesh.
#[derive(Component)]
pub(crate) struct Stream {
    points: Vec<Point>,
    firing: u32,
    tick: u32,
    ribbon: Entity,
    mesh: Handle<Mesh>,
}

/// The players' flame streams: stepped every frame, drawn as a ribbon from the nozzle.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn flames(
    players: Query<(Entity, &Combat, &Transform, Option<&mut Stream>), With<Model>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    world: Res<Collision>,
    timeline: Res<LocalTimeline>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut texture: Local<Option<Handle<Image>>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let eye = camera.iter().next().map_or(Vec3::ZERO, |c| c.translation());
    for (player, combat, at, stream) in players {
        let Some(mut s) = stream else {
            if combat.weapon == 0x15 {
                let texture = texture.get_or_insert_with(|| load_flame(&assets)).clone();
                let mesh = meshes.add(ribbon(&[], Vec3::ZERO, eye, 0.0));
                let ribbon = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: Some(texture),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        cull_mode: None,
                        ..default()
                    })),
                    Transform::IDENTITY,
                ));
                let ribbon = ribbon.id();
                (commands.entity(player)).insert(Stream {
                    points: Vec::new(),
                    firing: 0,
                    tick,
                    ribbon,
                    mesh,
                });
            }
            continue;
        };
        let local = |[f, s, u]: [f32; 3]| at.translation + at.rotation * Vec3::new(-s, u, f);
        let nozzle = local(FLAME.nozzle);
        let turn = Quat::from_rotation_y(f32::from(FLAME.turn) * std::f32::consts::TAU / 65536.0);
        let dir = turn * (at.rotation * Vec3::Z);
        let wall = || raycast(&world.0, nozzle.to_array(), dir.to_array(), RANGE);
        for _ in 0..tick.saturating_sub(s.tick).min(30) {
            let fire = combat.anim == FLAME_FIRE;
            s.firing = if fire { s.firing + 1 } else { 0 };
            let emit = (fire && s.firing > DELAY).then(|| (nozzle, dir, wall()));
            step_stream(&mut s.points, emit);
        }
        s.tick = tick;
        let firing = s.firing > DELAY;
        if let Some(mut mesh) = meshes.get_mut(&s.mesh) {
            let scroll = FLAME.scroll * tick as f32;
            *mesh = ribbon(&s.points, nozzle, eye, scroll)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, ribbon_colors(&s.points, firing));
        }
        if s.points.is_empty() && combat.weapon != 0x15 {
            commands.entity(s.ribbon).despawn();
            commands.entity(player).remove::<Stream>();
        }
    }
}

fn load_flame(assets: &AssetServer) -> Handle<Image> {
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.address_mode_u = ImageAddressMode::Repeat;
    sampler.address_mode_v = ImageAddressMode::Repeat;
    let sampler = ImageSampler::Descriptor(sampler);
    assets
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| s.sampler = sampler.clone())
        .load(flame_png(PACK_WEAPONS))
}

/// The ribbon's vertex colours: white, the nozzle's pair at the stream's alpha while it fires,
/// then each point's.
fn ribbon_colors(points: &[Point], firing: bool) -> Vec<[f32; 4]> {
    let nozzle = if firing { FLAME.alpha / 255.0 } else { 0.0 };
    let alphas = std::iter::once(nozzle).chain(points.iter().rev().map(|p| p.look().1));
    alphas.flat_map(|a| [[1.0, 1.0, 1.0, a]; 2]).collect()
}

/// The ribbon through `nozzle` then the points, youngest first: two vertices a point, its width
/// apart, square to the stream and to the eye; U along it (a repeat every [`Flame::repeat`] units,
/// scrolled by `scroll`), V across.
pub fn ribbon(points: &[Point], nozzle: Vec3, eye: Vec3, scroll: f32) -> Mesh {
    let line: Vec<(Vec3, f32)> = std::iter::once((nozzle, FLAME.width[0]))
        .chain(points.iter().rev().map(|p| (p.pos, p.look().0)))
        .collect();
    let (mut positions, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let mut along = 0.0;
    for (i, &(p, width)) in line.iter().enumerate() {
        let next = line[(i + 1).min(line.len() - 1)].0;
        let prev = line[i.saturating_sub(1)].0;
        if i > 0 {
            along += p.distance(prev);
        }
        let side = (next - prev).cross(eye - p).normalize_or_zero() * width / 2.0;
        let u = along / FLAME.repeat + scroll;
        positions.extend([p - side, p + side]);
        uvs.extend([[u, 0.0], [u, FLAME.across]]);
        if i > 0 {
            let k = 2 * i as u32;
            indices.extend([k - 2, k - 1, k, k, k - 1, k + 1]);
        }
    }
    let n = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; n])
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The models' poses against the oracle's matrices: the knife 40° up, then 20° down two frames
    /// on (V knife DLs 121/123: z (−0.054, 0.643, −0.764), then (−0.067, −0.342, −0.937), flying
    /// (−0.07, 0, −0.998)), its side axis up × flight; the bolt's x at 60° then 0° (V bolt2 151/155).
    #[test]
    fn projectiles_turn_as_the_original() {
        let dir = Vec3::new(-0.0706, 0.0, -0.9975);
        let z = |q: Quat| q * Vec3::Z;
        assert!(z(pose(&KNIFE, dir, 0.0, None)).distance(Vec3::new(-0.054, 0.643, -0.764)) < 0.01);
        assert!(z(pose(&KNIFE, dir, 2.0, None)).distance(Vec3::new(-0.067, -0.342, -0.937)) < 0.01);
        assert!((pose(&KNIFE, dir, 5.0, None) * Vec3::X).distance(Vec3::Y.cross(dir)) < 1e-4);
        let bolt = Vec3::new(-0.049, 0.0, -0.999).normalize();
        let x = |age| pose(&BOLT, bolt, age, None) * Vec3::X;
        assert!((x(0.0).y.asin().to_degrees() - 60.0).abs() < 0.1);
        assert!(x(2.0).y.abs() < 1e-4 && z(pose(&BOLT, bolt, 2.0, None)).distance(bolt) < 1e-4);
        // The grenade along the ground (V gren4 DLs 119/121, its frames 16 and 18), then lying along
        // its throw; its frame 26's pitch (V DL 129: −98.1°) is where it settles from.
        let g = |age| z(pose(&GRENADE, bolt, age, None));
        assert!(g(16.0).distance(Vec3::new(0.032, -0.757, 0.652)) < 0.01);
        assert!(g(18.0).distance(Vec3::new(0.047, 0.281, 0.958)) < 0.01);
        assert!((pose_pitch(&GRENADE, 26.0) + 98.1).abs() < 1.0);
        let rest = pose(&GRENADE, bolt, 0.0, Some((40.0, -98.0)));
        assert!(z(rest).distance(bolt) < 0.01);
        assert_eq!(look(0, 0x18, None), BOLT);
        assert_eq!(look(0, 0x14, Some(0)), KNIFE);
        // Item 0x3A's grenade on 0x1A and 0x3F: upright, +z along the throw, never turning (V l1a
        // DLs 133-220: joint 0's z (−0.992, 0, 0.122), its throw's heading, y up).
        assert_eq!(look(1, 0, None), GRENADE);
        let l = look(1, 0, Some(1));
        assert_eq!((l.shape, l.scale, l.lift), (Shape::Lob(1), 1.0, 28.0));
        let heading = Vec3::new(-0.992, 0.0, 0.122).normalize();
        for (age, landed) in [(3.0, None), (40.0, Some((10.0, pose_pitch(&l, 30.0))))] {
            let q = pose(&l, heading, age, landed);
            assert!(z(q).distance(heading) < 1e-5 && (q * Vec3::Y).distance(Vec3::Y) < 1e-5);
        }
    }

    /// The stream against flame DL 130's records: a point 41.7 a frame along the stream, rising
    /// 9.1, then 10.3, and 12.1 on its fourth frame (V 9.07, 10.25, 12.25; 14.73 on its ninth); 16
    /// wide on its first frame, +6.6 a frame; alpha 200 for 9 frames, then 28.6 less a frame, gone
    /// after 16; the ribbon a pair of vertices a point and the nozzle's.
    #[test]
    fn the_flame_stream_follows_the_records() {
        let mut points = Vec::new();
        let emit = Some((Vec3::ZERO, Vec3::NEG_Z, RANGE));
        step_stream(&mut points, emit);
        assert_eq!(points[0].pos.z, -FLAME.speed);
        for _ in 0..3 {
            step_stream(&mut points, emit);
        }
        let first = points[0];
        assert_eq!((first.age, points.len()), (4.0, 4));
        assert!((first.vel.y - 12.2).abs() < 0.3, "{}", first.vel.y);
        assert!((first.look().0 - 35.8).abs() < 0.01);
        for _ in 0..20 {
            step_stream(&mut points, emit);
        }
        assert_eq!(points.len(), 15);
        let alphas: Vec<_> = points
            .iter()
            .map(|p| (p.look().1 * 255.0).round())
            .collect();
        assert_eq!(
            alphas[..8],
            [29.0, 57.0, 86.0, 114.0, 143.0, 171.0, 200.0, 200.0]
        );
        step_stream(&mut points, None);
        assert_eq!(points.len(), 14);
        let mesh = ribbon(&points, Vec3::ZERO, Vec3::new(0.0, 100.0, 300.0), 0.0);
        assert_eq!(mesh.count_vertices(), 30);
        // A wall 200 ahead: no point goes past it.
        let mut points = Vec::new();
        for _ in 0..20 {
            step_stream(&mut points, Some((Vec3::ZERO, Vec3::NEG_Z, 200.0)));
        }
        assert!(points.iter().all(|p| p.pos.z >= -200.0) && points.len() == 4);
    }
}
