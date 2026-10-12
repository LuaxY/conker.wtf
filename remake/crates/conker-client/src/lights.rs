//! War: Total War's lights (the pack's `maps/<map>/lights.json`, [`Lights`];
//! `lab/research/mp-original/total-war-effects.md` §1, §3): CBFD point lights the gas switches. The
//! original lights the level's vertex colours with them: a vertex's colour × min(1, ambient + Σ rgb
//! × [`reach`]). The remake's level is unlit vertex colour, so (I, the light table is open):
//! - the room, the scenes' vertices inside trigger 0 (the script's shelter), takes that light while
//!   the countdown runs (its white lamps off, the red light on, `0xE016`), and its own colours back
//!   after (the lamps' light assumed to saturate it, as the unlit level draws everything);
//! - each other light while on (the red one, a shown canister's, `0xE017`, a carrier's) is a Bevy
//!   point light too, for the characters and the lit models.

use bevy::color::Srgba;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use conker_assets::{Light, Lights, When};
use conker_camera::volume::CameraVolume;
use conker_sim::net::MatchView;

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

/// The systems: the room's tint and the placed lights (a carrier's is [`crate::held`]'s).
pub fn build(app: &mut App, lights: Lights, triggers: &[u8]) {
    let zone = (triggers.as_chunks::<0x34>().0.first()).map(CameraVolume::parse_record);
    let placed: Vec<Light> = (lights.lights.iter())
        .filter(|l| !matches!(l.when, When::Calm | When::Carried))
        .cloned()
        .collect();
    app.insert_resource(Room {
        lights,
        zone,
        vertices: None,
    });
    app.add_systems(Startup, move |mut commands: Commands| {
        for l in &placed {
            commands.spawn((
                Placed(l.clone()),
                point_light(l, l.size),
                Transform::from_translation(Vec3::from(l.at)),
                Visibility::Hidden,
            ));
        }
    });
    app.add_systems(Update, (tint_room, shine));
}

/// A light of [`Lights::lights`] at its place.
#[derive(Component)]
struct Placed(Light);

/// Shows each placed light while it shines, at its size this frame.
fn shine(
    game: Query<&MatchView>,
    time: Res<Time>,
    mut lights: Query<(&Placed, &mut PointLight, &mut Visibility)>,
) {
    let Ok(m) = game.single() else { return };
    let frames = time.elapsed_secs() * 60.0;
    for (Placed(l), mut light, mut v) in &mut lights {
        let shines = m.total_war && on(l, m.countdown > 0, m.props);
        v.set_if_neq(match shines {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        if shines {
            let p = point_light(l, size(l, frames));
            (light.intensity, light.range) = (p.intensity, p.range);
        }
    }
}

/// While the countdown runs (from the delivery's frame to the kill's, V e1 DL 806/1103), the room's
/// vertices: colour × min(1, ambient + Σ shining light × [`reach`]), in the N64's display values;
/// after, their colours back. The vertices are read on the first such frame (long after
/// `render::shade_vertex_colors` decoded them).
#[allow(clippy::too_many_arguments)]
fn tint_room(
    game: Query<&MatchView>,
    roots: Query<Entity, With<Lit>>,
    children: Query<&Children>,
    shapes: Query<(&Mesh3d, &GlobalTransform)>,
    time: Res<Time>,
    mut room: ResMut<Room>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tinted: Local<bool>,
) {
    let Ok(m) = game.single() else { return };
    let alarm = m.total_war && m.countdown > 0;
    if !alarm && !*tinted {
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
    let frames = time.elapsed_secs() * 60.0;
    let shining: Vec<(Vec3, [f32; 3], f32)> = (lights.lights.iter())
        .filter(|l| l.when != When::Carried && on(l, alarm, m.props))
        .map(|l| {
            (
                Vec3::from(l.at),
                l.rgb.map(|c| f32::from(c) / 255.0),
                size(l, frames),
            )
        })
        .collect();
    let ambient = lights.ambient.map(|c| f32::from(c) / 255.0);
    for &(id, i, p, base) in vertices.iter() {
        let Some(mut mesh) = meshes.get_mut(id) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(cs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        else {
            continue;
        };
        let shade: [f32; 3] = std::array::from_fn(|k| match alarm {
            true => (ambient[k]
                + (shining.iter())
                    .map(|(at, rgb, size)| rgb[k] * reach(*size, at.distance_squared(p)))
                    .sum::<f32>())
            .min(1.0),
            false => 1.0,
        });
        for k in 0..3 {
            cs[i][k] = Srgba::gamma_function(base[k] * shade[k]);
        }
    }
    *tinted = alarm;
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
}
