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
//! the two red ones pulsing over the exits, all `calm`) are Bevy point lights all match.
//!
//! The white lamps (0x2D's two in the shelter, 0x2B's two) flicker ([`Flicker`], light behaviour 4,
//! `war-leftovers.md` §6), each flash's start buzzing at the lamp ([`crate::audio`]'s emitters). On
//! 0x2D the room dims while a lamp flickers: each vertex by its light with the lamps' sizes now over
//! its light with them full (I: the room is otherwise drawn as if the lamps saturate it).

use bevy::color::Srgba;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use conker_assets::{Light, Lights, When};
use conker_camera::volume::CameraVolume;
use conker_rules::Rng;
use conker_sim::net::MatchView;
use lightyear::core::timeline::is_in_rollback;

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
/// RSP's light (C, `cbfd-recompiled.md` §6). I: in clip space scaled by the coordinate modifier,
/// taken as world units (unmeasured; the knob if the room looks off).
const FULL: f32 = 2048.0;

/// The share of a light of `size` at squared distance `d2`: min(1, [`FULL`] · size / d²), none from
/// 362·√size (`func_1515D914` keeps d² < size << 17).
pub fn reach(size: f32, d2: f32) -> f32 {
    match d2 < size * 131_072.0 {
        true => (FULL * size / d2).min(1.0),
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

/// Steps the lamps a tick while the match plays (before [`crate::audio`] hears their flashes),
/// headless too.
pub fn flickers(app: &mut App, lights: &Lights, seed: u64) {
    app.insert_resource(Flickers::new(lights, seed));
    app.add_systems(FixedUpdate, flicker.run_if(not(is_in_rollback)));
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
