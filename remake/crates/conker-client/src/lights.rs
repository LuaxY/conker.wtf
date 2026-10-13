//! War: Total War's lights (the pack's `maps/<map>/lights.json`, [`Lights`];
//! `lab/research/mp-original/total-war-effects.md` §1, §3): CBFD point lights the gas switches. The
//! original lights the level's vertex colours with them: a vertex's colour × min(1, ambient + Σ rgb
//! × [`reach`]). The remake's level is unlit vertex colour, so (I, the light table is open):
//! - the room, the scenes' vertices inside trigger 0 (the script's shelter), takes that light while
//!   the countdown runs (its white lamps off, the red light on, `0xE016`), and its own colours back
//!   after (the lamps' light assumed to saturate it, as the unlit level draws everything);
//! - each other light while on (the red one, a shown canister's, `0xE017`, a carrier's) is a Bevy
//!   point light too, for the characters and the lit models.
//!
//! War: Beach's lights (`maps/2b/lights.json`, `beach-presentation.md` §7: the two white lamps and
//! the two red ones pulsing over the exits, all `calm`) are Bevy point lights all match, for the
//! characters; with the sweeping ground lights ([`Sweepers`]) and a strike's red light they light the
//! level as the original does ([`relight`]).
//!
//! The white lamps (0x2D's two in the shelter, 0x2B's two) flicker ([`Flicker`], light behaviour 4,
//! `war-leftovers.md` §6), each flash's start buzzing at the lamp ([`crate::audio`]'s emitters). On
//! 0x2D the room dims while a lamp flickers: each vertex by its light with the lamps' sizes now over
//! its light with them full (I: the room is otherwise drawn as if the lamps saturate it).

use std::collections::{HashMap, HashSet};

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::color::Srgba;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use conker_assets::{Light, Lights, When};
use conker_camera::volume::CameraVolume;
use conker_rules::Rng;
use conker_sim::collision::CollisionWorld;
use conker_sim::net::{Collision, MatchView};
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::LocalTimeline;

/// The level's and Total War's props' scene roots: what the room's light tints.
#[derive(Component)]
pub struct Lit;

/// A vertex of the room: its mesh, index, world position and display colour.
type Vertex = (AssetId<Mesh>, usize, Vec3, [f32; 3]);

/// The lights, the room (trigger 0) and its vertices once read.
#[derive(Resource)]
struct Room {
    lights: Lights,
    zone: Option<CameraVolume>,
    vertices: Option<Vec<Vertex>>,
}

/// A point light of this many units-squared × size is full: `(att / 16) / (2·d² / 65536)` of the
/// RSP's light (C, `cbfd-recompiled.md` §6), in world units (V 2b, `beach-environment-scratch/
/// relight/`: the clip space × the coordinate modifier is a rotation; a lamp's share 0.4916 at 456).
const FULL: f32 = 2048.0;

/// The share of a light of `size` at squared distance `d2`: min(1, [`FULL`] · size / d²).
pub fn share(size: f32, d2: f32) -> f32 {
    (FULL * size / d2).min(1.0)
}

/// [`share`], none from 362·√size (`func_1515D914` keeps d² < size << 17 for 0x2D's room).
pub fn reach(size: f32, d2: f32) -> f32 {
    match d2 < size * 131_072.0 {
        true => share(size, d2),
        false => 0.0,
    }
}

/// Whether `l` shines: the countdown running or not, `props` the match's shown props (a carrier's
/// shines on each carrier).
pub fn on(l: &Light, countdown: bool, props: u32) -> bool {
    match l.when {
        When::Calm => !countdown,
        When::Countdown => countdown,
        When::Shown(n) => props >> n & 1 != 0,
        When::Carried => true,
    }
}

/// `l`'s size `frames` frames on: trunc(size + swing · sin φ), φ stepping `step` a frame
/// (`func_15163CF8`, `func_15163B98`, C; any start phase).
pub fn size(l: &Light, frames: f32) -> f32 {
    (l.size + l.swing * (l.step * frames).sin()).trunc()
}

/// `l` as a Bevy point light `size` big: as bright as the sun (2500 lux) where it is full, out to
/// 362·√size; no shadows.
pub fn point_light(l: &Light, size: f32) -> PointLight {
    let [r, g, b] = l.rgb;
    PointLight {
        color: Color::srgb_u8(r, g, b),
        intensity: 4.0 * core::f32::consts::PI * 2500.0 * FULL * size,
        range: 362.0 * size.sqrt(),
        shadow_maps_enabled: false,
        ..default()
    }
}

/// A burst starts on a steady lamp with this chance a frame (`0x800A6AD4`, C).
pub const FLICKER_CHANCE: f32 = 0.005;

/// A flickering lamp's size (light behaviour 4: `func_151607A4`, `func_15160954`, C, a frame of dt
/// 1): steady at full until, with [`FLICKER_CHANCE`] a frame, a burst of `rand % 5 + 1` flashes; a
/// flash puts the size to 0 and waits `rand·28 + 1` frames (counting down while ≥ 0), then grows
/// `full / (rand·12 + 8)` a frame until past full, which ends it (and starts the next).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flicker {
    pub size: f32,
    step: f32,
    wait: f32,
    left: u16,
}

impl Flicker {
    pub fn new(full: f32) -> Self {
        Self {
            size: full,
            step: 0.0,
            wait: 0.0,
            left: 0,
        }
    }

    /// One frame; true when a flash starts (its buzz).
    pub fn tick(&mut self, full: f32, rng: &mut Rng) -> bool {
        if self.left == 0 {
            if rng.next_f32() >= FLICKER_CHANCE {
                return false;
            }
            self.left = (rng.next_u32() % 5 + 1) as u16;
        } else if self.wait >= 0.0 {
            self.wait -= 1.0;
            return false;
        } else {
            self.size += self.step;
            if self.size <= full {
                return false;
            }
            self.size = full;
            self.left -= 1;
            if self.left == 0 {
                return false;
            }
        }
        self.size = 0.0;
        self.step = full / (rng.next_f32() * 12.0 + 8.0);
        self.wait = rng.next_f32() * 28.0 + 1.0;
        true
    }
}

/// A flickering light ([`Light::flicker`]): its index in [`Lights::lights`], place, full size and
/// flicker, and whether a flash started this tick.
pub struct Lamp {
    pub light: usize,
    pub at: [f32; 3],
    full: f32,
    pub flicker: Flicker,
    pub flashed: bool,
}

/// The map's flickering lamps, a tick at a time ([`flicker`]), with the client's own random numbers
/// (the original's are each machine's own too, I).
#[derive(Resource)]
pub struct Flickers {
    rng: Rng,
    pub lamps: Vec<Lamp>,
}

impl Flickers {
    pub fn new(lights: &Lights, seed: u64) -> Self {
        let lamps = (lights.lights.iter().enumerate())
            .filter(|(_, l)| l.flicker)
            .map(|(light, l)| Lamp {
                light,
                at: l.at,
                full: l.size,
                flicker: Flicker::new(l.size),
                flashed: false,
            })
            .collect();
        Self {
            rng: Rng::new(seed.rotate_left(17) | 1),
            lamps,
        }
    }

    /// Light `i`'s size if it flickers: the light's size byte, trunc(size).
    pub fn size(&self, i: usize) -> Option<f32> {
        let lamp = self.lamps.iter().find(|l| l.light == i);
        lamp.map(|l| l.flicker.size.trunc())
    }

    /// Whether a lamp is not at its full size.
    fn flickering(&self) -> bool {
        self.lamps.iter().any(|l| l.flicker.size < l.full)
    }
}

/// Steps the lamps a tick while the match plays (before [`crate::audio`] hears their flashes), and
/// War: Beach's sweepers, headless too.
pub fn flickers(app: &mut App, lights: &Lights, seed: u64) {
    app.insert_resource(Flickers::new(lights, seed));
    app.insert_resource(Sweepers::new(seed));
    app.add_systems(FixedUpdate, (flicker, sweep).run_if(not(is_in_rollback)));
}

/// War: Beach's sweeping ground lights (light behaviour 19: the level emitter `func_150EAB10`,
/// `func_150EAE24` a frame; `beach-environment.md` §1, C + V fx2 `sweep.csv`; constants, as the
/// emitter's ROM record is not located): a white light of [`SWEEP_SIZE`] from one end to the other,
/// x [`SWEEP_X`] at z −2119 + 10458·r (each end its own r), at 48.83 + 117.19·r a frame, for |AB| /
/// speed frames, on the ground below 2316 ([`ground`]); one spawns each time the rate's sum
/// (+0.01172 + 0.0391·r a frame) passes 1.
pub const SWEEP_X: [f32; 2] = [4108.0, -3796.0];
pub const SWEEP_SIZE: f32 = 10.0;
const SWEEP_Z: [f32; 2] = [-2119.0, 10458.0];
const SWEEP_SPEED: [f32; 2] = [48.83, 117.19];
const SWEEP_RATE: [f32; 2] = [0.01172, 0.0391];
/// The sweepers' ray: down from y 2316 to −1084 (`func_15046C80`), else at 2316.
const SWEEP_RAY: [f32; 2] = [2316.0, -1084.0];

/// A sweeper: where, its velocity (x, z a frame) and its frames left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweeper {
    pub at: [f32; 3],
    pub v: [f32; 2],
    pub life: f32,
}

/// The sweepers, with the client's own random numbers (I, as the lamps').
#[derive(Resource)]
pub struct Sweepers {
    rng: Rng,
    rate: f32,
    pub lights: Vec<Sweeper>,
}

impl Sweepers {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed.rotate_left(29) | 1),
            rate: 0.0,
            lights: Vec::new(),
        }
    }

    /// One frame: each light moves, ages and finds its ground (`ground(x, z)`), freed at ≤ 0 left;
    /// then the emitter's spawns.
    pub fn tick(&mut self, ground: impl Fn(f32, f32) -> f32) {
        for l in &mut self.lights {
            (l.at[0], l.at[2], l.life) = (l.at[0] + l.v[0], l.at[2] + l.v[1], l.life - 1.0);
            l.at[1] = ground(l.at[0], l.at[2]);
        }
        self.lights.retain(|l| l.life > 0.0);
        let mut r = || self.rng.next_f32();
        let mut rate = self.rate + SWEEP_RATE[0] + SWEEP_RATE[1] * r();
        while rate > 1.0 {
            rate -= 1.0;
            let [a, b] = SWEEP_X.map(|x| Vec2::new(x, SWEEP_Z[0] + SWEEP_Z[1] * r()));
            let (from, to) = if r() < 0.5 { (a, b) } else { (b, a) };
            let speed = SWEEP_SPEED[0] + SWEEP_SPEED[1] * r();
            let v = (to - from).normalize() * speed;
            self.lights.push(Sweeper {
                at: [from.x, ground(from.x, from.y), from.y],
                v: v.to_array(),
                life: from.distance(to) / speed,
            });
        }
        self.rate = rate;
    }
}

/// The ground a sweeper lights at (x, z): the floor below [`SWEEP_RAY`]'s top, down to its bottom,
/// else the top.
pub fn ground(world: &CollisionWorld, x: f32, z: f32) -> f32 {
    let [top, bottom] = SWEEP_RAY;
    (world.floor(x, z, -1e9, top - 50.0, None))
        .map(|f| f.y)
        .filter(|&y| y >= bottom)
        .unwrap_or(top)
}

/// The sweepers' frame on War: Beach.
fn sweep(
    game: Query<&MatchView>,
    world: Res<Collision>,
    timeline: Res<LocalTimeline>,
    mut sweepers: ResMut<Sweepers>,
) {
    if game.single().is_ok_and(|m| m.beach) {
        sweepers.tick(|x, z| ground(&world.0, x, z));
        for l in (sweepers.lights.iter()).filter(|l| SWEEP_X.contains(&l.at[0])) {
            let ([x, y, z], [vx, vz]) = (l.at, l.v);
            let t = timeline.tick().0;
            debug!(
                "t={t} sweeper at ({x:.0}, {y:.0}, {z:.0}), ({vx:.1}, {vz:.1}) a frame for {:.1}",
                l.life
            );
        }
    }
}

/// Each lamp's frame (none once the match is over).
pub fn flicker(game: Query<&MatchView>, mut flickers: ResMut<Flickers>) {
    let playing = game.single().is_ok_and(|m| m.phase < 2);
    let Flickers { rng, lamps } = &mut *flickers;
    for l in lamps {
        l.flashed = playing && l.flicker.tick(l.full, rng);
    }
}

/// The systems: the room's tint and the placed lights (a carrier's is [`crate::held`]'s). The room
/// is 0x2D's shelter (trigger 0), the map with the gas's alarm light.
pub fn build(app: &mut App, lights: Lights, triggers: &[u8]) {
    let alarm = lights.lights.iter().any(|l| l.when == When::Countdown);
    let zone = (triggers.as_chunks::<0x34>().0.first())
        .filter(|_| alarm)
        .map(CameraVolume::parse_record);
    let placed: Vec<(usize, Light)> = (lights.lights.iter().cloned().enumerate())
        .filter(|(_, l)| l.when != When::Carried)
        .collect();
    app.insert_resource(Room {
        lights,
        zone,
        vertices: None,
    });
    app.add_systems(Startup, move |mut commands: Commands| {
        for (i, l) in &placed {
            commands.spawn((
                Placed(l.clone(), *i),
                point_light(l, l.size),
                Transform::from_translation(Vec3::from(l.at)),
                Visibility::Hidden,
            ));
        }
    });
    app.add_systems(Update, (tint_room, shine));
    app.add_systems(PostUpdate, relight);
}

/// A light of [`Lights::lights`] at its place, and its index there.
#[derive(Component)]
struct Placed(Light, usize);

/// Shows each placed light while it shines, at its size this frame (a frame a tick: the red light's
/// 2.05 s pulse; a lamp's flicker): Total War's by [`on`], War: Beach's calm ones all match (Total
/// War's calm ones tint the room instead).
fn shine(
    game: Query<&MatchView>,
    time: Res<Time>,
    flickers: Res<Flickers>,
    mut lights: Query<(&Placed, &mut PointLight, &mut Visibility)>,
) {
    let Ok(m) = game.single() else { return };
    let frames = time.elapsed_secs() * conker_sim::TICK_HZ as f32;
    for (Placed(l, i), mut light, mut v) in &mut lights {
        let now = flickers.size(*i).unwrap_or_else(|| size(l, frames));
        let shines = now > 0.0
            && match l.when {
                When::Calm => m.beach,
                _ => m.total_war && on(l, m.countdown > 0, m.props),
            };
        v.set_if_neq(match shines {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        if shines {
            let p = point_light(l, now);
            (light.intensity, light.range) = (p.intensity, p.range);
        }
    }
}

/// While the countdown runs (from the delivery's frame to the kill's, V e1 DL 806/1103), the room's
/// vertices: colour × min(1, ambient + Σ shining light × [`reach`]), in the N64's display values;
/// outside it, while a lamp flickers, colour × that light with the lamps' sizes now over it with
/// them full (I); else their colours back. The vertices are read on the first such frame (long
/// after `render::shade_vertex_colors` decoded them).
#[allow(clippy::too_many_arguments)]
fn tint_room(
    game: Query<&MatchView>,
    roots: Query<Entity, With<Lit>>,
    children: Query<&Children>,
    shapes: Query<(&Mesh3d, &GlobalTransform)>,
    time: Res<Time>,
    flickers: Res<Flickers>,
    mut room: ResMut<Room>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tinted: Local<bool>,
) {
    let Ok(m) = game.single() else { return };
    let alarm = m.total_war && m.countdown > 0;
    let flickering = !alarm && flickers.flickering();
    if !alarm && !flickering && !*tinted {
        return;
    }
    let Room {
        lights,
        zone: Some(zone),
        vertices,
    } = &mut *room
    else {
        return;
    };
    let vertices = vertices.get_or_insert_with(|| {
        let mut out = Vec::new();
        let shapes = (roots.iter())
            .flat_map(|r| children.iter_descendants(r))
            .filter_map(|e| shapes.get(e).ok());
        for (mesh, at) in shapes {
            let Some(mesh_data) = meshes.get(&mesh.0) else {
                continue;
            };
            let (
                Some(VertexAttributeValues::Float32x3(ps)),
                Some(VertexAttributeValues::Float32x4(cs)),
            ) = (
                mesh_data.attribute(Mesh::ATTRIBUTE_POSITION),
                mesh_data.attribute(Mesh::ATTRIBUTE_COLOR),
            )
            else {
                continue;
            };
            for (i, (p, c)) in ps.iter().zip(cs).enumerate() {
                let p = at.transform_point(Vec3::from(*p));
                if zone.contains(p.to_array()) {
                    let c = [c[0], c[1], c[2]].map(Srgba::gamma_function_inverse);
                    out.push((mesh.id(), i, p, c));
                }
            }
        }
        let ys = out.iter().map(|v| v.2.y);
        let (lo, hi) = ys.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
        info!("the room: {} vertices, y {lo} to {hi}", out.len());
        out
    });
    let frames = time.elapsed_secs() * conker_sim::TICK_HZ as f32;
    // Each shining light's place, colour, size now and full.
    let shining: Vec<(Vec3, [f32; 3], f32, f32)> = (lights.lights.iter().enumerate())
        .filter(|(_, l)| l.when != When::Carried && on(l, alarm, m.props))
        .map(|(i, l)| {
            let full = size(l, frames);
            let now = flickers.size(i).unwrap_or(full);
            (
                Vec3::from(l.at),
                l.rgb.map(|c| f32::from(c) / 255.0),
                now,
                full,
            )
        })
        .collect();
    let ambient = lights.ambient.map(|c| f32::from(c) / 255.0);
    let lit = |p: Vec3, k: usize, now: bool| {
        let each = shining.iter().map(|(at, rgb, size, full)| {
            rgb[k] * reach(if now { *size } else { *full }, at.distance_squared(p))
        });
        (ambient[k] + each.sum::<f32>()).min(1.0)
    };
    for &(id, i, p, base) in vertices.iter() {
        let Some(mut mesh) = meshes.get_mut(id) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(cs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        else {
            continue;
        };
        let shade: [f32; 3] = std::array::from_fn(|k| match (alarm, flickering) {
            (true, _) => lit(p, k, true),
            (false, true) => lit(p, k, true) / lit(p, k, false),
            (false, false) => 1.0,
        });
        for k in 0..3 {
            cs[i][k] = Srgba::gamma_function(base[k] * shade[k]);
        }
    }
    *tinted = alarm || flickering;
}

/// The lights a view of War: Beach takes: the 10 nearest to the point 1000 in front of its camera
/// (`func_1515D914`, C; V: 6-9 alive, all taken).
const VIEW_LIGHTS: usize = 10;

/// A relit shape: its mesh, its vertices' positions in its own frame and their display colours.
type Shape = (AssetId<Mesh>, Vec<Vec3>, Vec<[f32; 3]>);

/// War: Beach's level as the original lights it (V `beach-environment-scratch/relight/`: every terrain
/// draw has G_LIGHTING, the pack's colours are the drawn ones, a far vertex is ambient × colour): each
/// vertex of the unlit draws under [`Lit`], colour × min(1, ambient + Σ rgb × [`share`]) over the
/// [`VIEW_LIGHTS`] lights shining this frame (the lamps at their flicker's size, the red ones
/// pulsing, the sweepers, a strike's red light). Each mesh's colours are read once
/// `render::shade_vertex_colors` has decoded them (its `Added` event, read here after `Update`).
// ponytail: a mesh two nodes share (0x2B's 03-5b, 28 vertices) is lit at the first node only; copy
// it per node if that shows.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn relight(
    mut added: MessageReader<AssetEvent<Mesh>>,
    game: Query<&MatchView>,
    roots: Query<Entity, With<Lit>>,
    children: Query<&Children>,
    parts: Query<(&Mesh3d, &GlobalTransform, &MeshMaterial3d<StandardMaterial>)>,
    camera: Query<&GlobalTransform, With<FreeCameraState>>,
    (time, timeline, flickers, sweepers, blasts): (
        Res<Time>,
        Res<LocalTimeline>,
        Res<Flickers>,
        Res<Sweepers>,
        Res<crate::beach::Blasts>,
    ),
    room: Res<Room>,
    materials: Res<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut decoded: Local<HashSet<AssetId<Mesh>>>,
    mut shapes: Local<HashMap<Entity, Shape>>,
) {
    for event in added.read() {
        if let AssetEvent::Added { id } = event {
            decoded.insert(*id);
        }
    }
    let (Ok(m), Ok(eye)) = (game.single(), camera.single()) else {
        return;
    };
    if !m.beach {
        return;
    }
    for e in roots.iter().flat_map(|r| children.iter_descendants(r)) {
        let Ok((mesh, _, material)) = parts.get(e) else {
            continue;
        };
        let unlit = materials.get(&material.0).is_some_and(|m| m.unlit);
        let taken = |id| shapes.values().any(|s| s.0 == id);
        if shapes.contains_key(&e) || !unlit || !decoded.contains(&mesh.id()) || taken(mesh.id()) {
            continue;
        }
        let Some(data) = meshes.get(&mesh.0) else {
            continue;
        };
        if let (
            Some(VertexAttributeValues::Float32x3(ps)),
            Some(VertexAttributeValues::Float32x4(cs)),
        ) = (
            data.attribute(Mesh::ATTRIBUTE_POSITION),
            data.attribute(Mesh::ATTRIBUTE_COLOR),
        ) {
            let ps = ps.iter().map(|p| Vec3::from(*p)).collect();
            let cs = (cs.iter())
                .map(|c| [c[0], c[1], c[2]].map(Srgba::gamma_function_inverse))
                .collect();
            shapes.insert(e, (mesh.id(), ps, cs));
        }
    }
    // The lights this frame, the nearest to the view's point first.
    let frames = time.elapsed_secs() * conker_sim::TICK_HZ as f32;
    let tick = timeline.tick().0;
    let rgb = |c: [u8; 3]| c.map(|c| f32::from(c) / 255.0);
    let placed = (room.lights.lights.iter().enumerate()).map(|(i, l)| {
        let now = flickers.size(i).unwrap_or_else(|| size(l, frames));
        (Vec3::from(l.at), rgb(l.rgb), now)
    });
    let swept = (sweepers.lights.iter()).map(|l| (Vec3::from(l.at), [1.0; 3], SWEEP_SIZE));
    let strikes = (blasts.strikes.iter().filter(|s| s.blast.is_none())).map(|s| {
        let at = Vec3::from(s.at) + Vec3::Y * 40.0;
        let size = crate::beach::strike_size(tick.saturating_sub(s.call));
        (at, [1.0, 0.0, 0.0], size)
    });
    let mut lights: Vec<(Vec3, [f32; 3], f32)> = placed
        .chain(swept)
        .chain(strikes)
        .filter(|l| l.2 > 0.0)
        .collect();
    let point = eye.translation() + eye.forward() * 1000.0;
    lights.sort_by(|a, b| {
        a.0.distance_squared(point)
            .total_cmp(&b.0.distance_squared(point))
    });
    lights.truncate(VIEW_LIGHTS);
    let ambient = rgb(room.lights.ambient);
    for (e, (id, ps, base)) in shapes.iter() {
        let (Ok((_, at, _)), Some(mut mesh)) = (parts.get(*e), meshes.get_mut(*id)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(cs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        else {
            continue;
        };
        for ((c, p), base) in cs.iter_mut().zip(ps).zip(base) {
            let lit = lit(*base, at.transform_point(*p), ambient, &lights);
            c[..3].copy_from_slice(&lit.map(Srgba::gamma_function));
        }
    }
}

/// A vertex of display colour `base` at `p`, lit (display values): base × min(1, ambient + Σ rgb ×
/// [`share`]) over `lights` (place, rgb, size).
pub fn lit(
    base: [f32; 3],
    p: Vec3,
    ambient: [f32; 3],
    lights: &[(Vec3, [f32; 3], f32)],
) -> [f32; 3] {
    let mut shade = ambient;
    for (at, rgb, size) in lights {
        let s = share(*size, at.distance_squared(p));
        for k in 0..3 {
            shade[k] += rgb[k] * s;
        }
    }
    std::array::from_fn(|k| base[k] * shade[k].min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A light is full within 45·√size, gone from 362·√size; each kind shines when it should.
    #[test]
    fn reach_and_switches() {
        let red = 85.0f32;
        assert_eq!(reach(red, (45.0 * red.sqrt()).powi(2)), 1.0);
        assert!(reach(red, (362.0 * red.sqrt() - 1.0).powi(2)) > 0.0);
        assert_eq!(reach(red, (362.1 * red.sqrt()).powi(2)), 0.0);
        let light = |when| Light {
            at: [0.0; 3],
            rgb: [255, 0, 0],
            size: red,
            swing: 55.0,
            step: 0.1021,
            when,
            flicker: false,
        };
        let shines = |when, countdown, props| on(&light(when), countdown, props);
        assert!(shines(When::Calm, false, 0) && !shines(When::Calm, true, 0));
        assert!(shines(When::Countdown, true, 0) && !shines(When::Countdown, false, 0));
        assert!(shines(When::Shown(27), false, 1 << 27) && !shines(When::Shown(27), true, 1 << 28));
        // The red light's size: 85 + 55 sin φ, truncated, 30 to 140.
        assert_eq!(size(&light(When::Countdown), 0.0), 85.0);
        let sizes = (0..62).map(|f| size(&light(When::Countdown), f as f32));
        let (lo, hi) = sizes.fold((f32::MAX, 0.0f32), |(lo, hi), s| (lo.min(s), hi.max(s)));
        assert_eq!((lo, hi), (30.0, 139.0));
    }

    /// The 2b relight against the oracle frame (`beach-environment-scratch/relight/out_shade_260.txt`):
    /// a lamp's share 0.4916 at 456 (size 50; 0.3849 at 283, size 15); a far vertex of colour 111
    /// is ambient × colour, (48, 61, 92) (the original's (49, 62, 93): the far lights' tails); one of
    /// 180 49 from a sweeper saturates.
    #[test]
    fn the_beach_is_lit_as_the_original() {
        assert!((share(50.0, 456.0 * 456.0) - 0.4916).abs() < 0.002);
        assert!((share(15.0, 283.0 * 283.0) - 0.3849).abs() < 0.002);
        let ambient = [112.0, 141.0, 213.0].map(|c: f32| c / 255.0);
        let far = lit(
            [111.0 / 255.0; 3],
            Vec3::new(3085.0, 1816.0, 3901.0),
            ambient,
            &[],
        );
        assert_eq!(far.map(|c| (c * 255.0) as u8), [48, 61, 92]);
        let sweeper = (
            Vec3::new(197.0, -234.0 + 49.0, 4303.0),
            [1.0; 3],
            SWEEP_SIZE,
        );
        let near = lit(
            [180.0 / 255.0; 3],
            Vec3::new(197.0, -234.0, 4303.0),
            ambient,
            &[sweeper],
        );
        assert_eq!(near.map(|c| (c * 255.0).round() as u8), [180; 3]);
    }

    /// The sweepers' generator (`beach-environment.md` §1): each from one end's x to the other's, z
    /// in −2119..8339, 48.83-166.02 a frame, for |AB| / speed frames; one about every 32 frames.
    /// Then fx2's light 0x801BF040 (`sweep.csv`): at read 212 (4108, −1937.2), (−115.25, 104.04) a
    /// frame, 68.58 frames left; (−3728.8, 5137.2) and 0.58 left at 280, gone at 281.
    #[test]
    fn sweepers_cross_the_beach() {
        let mut s = Sweepers::new(3);
        let mut born = 0;
        for _ in 0..32_000 {
            s.tick(|_, _| 0.0);
            for l in s.lights.iter().filter(|l| SWEEP_X.contains(&l.at[0])) {
                born += 1;
                let speed = Vec2::from(l.v).length();
                assert!((48.83..=166.02).contains(&speed), "{speed}");
                assert!((-2119.0..=8339.0).contains(&l.at[2]));
                let to = if l.at[0] == SWEEP_X[0] { 1 } else { 0 };
                let x = l.at[0] + l.v[0] * l.life;
                assert!((x - SWEEP_X[to]).abs() < 0.5, "{x}");
                assert!((-2119.5..=8339.5).contains(&(l.at[2] + l.v[1] * l.life)));
            }
        }
        assert!((950..1050).contains(&born), "{born}");
        s.rate = f32::MIN;
        s.lights = vec![Sweeper {
            at: [4108.0, 0.0, -1937.2],
            v: [-115.25, 104.04],
            life: 68.58,
        }];
        for _ in 212..280 {
            s.tick(|_, _| 0.0);
        }
        let l = s.lights[0];
        assert!(
            (l.at[0] + 3728.8).abs() < 1.0 && (l.at[2] - 5137.2).abs() < 1.0,
            "{l:?}"
        );
        assert!((l.life - 0.58).abs() < 1e-3);
        s.tick(|_, _| 0.0);
        assert!(s.lights.is_empty());
    }

    /// A sweeper's ground on the pack's 2b collision (fx2 `sweep.csv`'s ray: −282.6 at (765.8,
    /// 1079.8), 17.2 at (797.9, 2424.6), −233.3 at (1704.8, −28.5); off the map at (4108, −1937.2),
    /// where the light stays at 2316).
    #[test]
    fn sweepers_light_the_ground() {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let dir = std::path::Path::new(&dir);
        if !dir.join(conker_assets::MANIFEST).exists() {
            eprintln!("no asset pack in {}: skipping", dir.display());
            return;
        }
        let read = |p: &str| std::fs::read(dir.join(p));
        let characters = crate::characters("2b");
        let slice =
            conker_assets::Slice::load("2b", &characters, conker_assets::PACK_WEAPONS, read);
        let world = conker_sim::net::scene_world(&slice.unwrap(), 0x2B);
        for (x, z, y) in [
            (765.8, 1079.8, -282.6),
            (797.9, 2424.6, 17.2),
            (1704.8, -28.5, -233.3),
            (4108.0, -1937.2, 2316.0),
        ] {
            let g = ground(&world, x, z);
            assert!((g - y).abs() < 1.0, "({x}, {z}): {g}");
        }
    }

    /// A lamp's flicker against `war-leftovers.md` §6 (C `func_151607A4`, `func_15160954`): bursts
    /// start from full with 0.005 a steady frame and have 1-5 flashes; a flash holds the size at 0
    /// for 3-30 frames (its own, then `rand·28 + 1` counted down while ≥ 0), then grows back over
    /// 7-20 frames between 0 and full (`full / (rand·12 + 8)` a frame, past full ends it); a burst
    /// ends at full; one buzz a flash.
    #[test]
    fn lamps_flicker_in_bursts() {
        let full = 55.0;
        let (mut rng, mut f) = (Rng::new(7), Flicker::new(full));
        let (mut steady, mut bursts, mut flashes) = (0u32, Vec::<u32>::new(), 0u32);
        let mut runs: Vec<[u32; 2]> = Vec::new();
        for _ in 0..200_000 {
            let idle = f.left == 0;
            let flash = f.tick(full, &mut rng);
            steady += u32::from(idle);
            if flash {
                flashes += 1;
                if idle {
                    bursts.push(0);
                }
                *bursts.last_mut().unwrap() += 1;
                runs.push([0, 0]);
            }
            if let Some(run) = runs.last_mut() {
                if f.size == 0.0 {
                    run[0] += 1;
                } else if f.size < full {
                    run[1] += 1;
                }
            }
            assert!(f.left > 0 || f.size == full, "a burst ends at full");
        }
        runs.pop();
        assert_eq!(flashes as usize, runs.len() + 1, "one buzz a flash");
        assert!(bursts.iter().all(|n| (1..=5).contains(n)));
        assert!((1..=5).all(|n| bursts.contains(&n)));
        let rate = bursts.len() as f32 / steady as f32;
        assert!((0.0045..0.0055).contains(&rate), "{rate}");
        let range = |k: usize| {
            let v = runs.iter().map(|r| r[k]);
            (v.clone().min().unwrap(), v.max().unwrap())
        };
        let ((dark_lo, dark_hi), (up_lo, up_hi)) = (range(0), range(1));
        assert!((3..=4).contains(&dark_lo) && (29..=30).contains(&dark_hi));
        assert!((7..=8).contains(&up_lo) && (19..=20).contains(&up_hi));
    }
}
