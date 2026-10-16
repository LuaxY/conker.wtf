//! The crossbows' aim laser ([`Combat::laser`]; one's own [`Own`], the prediction's
//! `Match::laser`), as the original draws it (C `func_151026BC` → `func_15100600`, effect type 3;
//! V `lab/research/mp-original/world-g5-scratch` las): its dot (`hud/strike_glow.png`) lying on
//! the surface the scope's line meets, in every view, and its beam (`hud/strike_streak.png`, S
//! wrapping along it and scrolling) from the crossbow to the dot in the other players' views only;
//! both the holder's colour with the texture's alpha × the laser's ([`looks`]).

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use conker_sim::net::{Collision, Combat};
use lightyear::prelude::{LocalTimeline, Predicted};

/// One's own laser this tick (the prediction's `Match::laser`; lib's `predict`).
#[derive(Resource, Default)]
pub struct Own(pub Option<[f32; 3]>);

/// Where the beam starts, in the right hand's (`joint_09`'s) frame (the effect's offset table
/// 0x800A21A0, entry 3, C).
const START: Vec3 = Vec3::new(-6.0, -25.0, 119.0);

/// The beam's S: one wrap of the 128-texel streak every this many units (V las: about 25 S units
/// of 1/32 texel a unit, 29721 over 1194).
const WRAP: f32 = 128.0 * 32.0 / 25.0;

/// A player's laser on: the tick it came on, its dot and beam.
#[derive(Component)]
struct Shining {
    since: u32,
    dot: Entity,
    beam: Entity,
}

pub fn build(app: &mut App) {
    app.add_systems(
        PostUpdate,
        shine.after(bevy::transform::TransformSystems::Propagate),
    );
}

/// The laser `k` frames after it came on (V las, every other frame: the effect steps on even
/// reads): its alpha (0-255: 0xFA, then 0xF5, 0xF3, … 0xCF by k = 16), the beam's width (4, 12,
/// 16, 22, … 40) and the dot's (1, 7, 7, 11, … about 20), each easing 0.2 of its gap a step.
/// ponytail: the original's alpha wanders 0xA5-0xE3 and the dot's width ±3 after that (a random
/// walk not read); both settle here at their means.
pub fn looks(k: u32) -> (f32, f32, f32) {
    let left = 0.8f32.powi((k / 2) as i32);
    (196.0 + 54.0 * left, 40.0 - 36.0 * left, 20.0 - 19.0 * left)
}

/// The laser's colour for a holder of `class` (`func_151026BC`, C, by the holder's model: 128's
/// green (0x32, 0xFF, 0); 117's red (0xFF, 0, 0)). ponytail: the other models take the switch's
/// default, green unless the holder's record +0x78 is 0x3B (I); V las's default holder drew red,
/// so red here.
pub fn colour(class: u8) -> Color {
    match conker_rules::characters::model(class) {
        Some(128) => Color::srgb_u8(0x32, 0xFF, 0),
        _ => Color::srgb_u8(0xFF, 0, 0),
    }
}

/// The beam's S offset (in wraps) `k` frames on: the original's swings back and forth about one
/// wrap (V las: s at the dot −7452 → −1477 by k = 22 → −9946). ponytail: a sine fitted to that.
pub fn scroll(k: u32) -> f32 {
    1.03 * (std::f32::consts::TAU * k as f32 / 70.0 - 0.42).sin()
}

/// The dot's facing: the normal of the collision triangle it lies on (within 2 units), else back
/// toward `from` (a body part).
fn facing(world: &Collision, dot: Vec3, from: Vec3) -> Vec3 {
    let tris = world.0.near(dot.x as i32, dot.z as i32).iter();
    let on = tris.map(|&k| &world.0.tris[k as usize]).find(|t| {
        let n = Vec3::from_array(t.normal);
        n.dot(dot - Vec3::from_array(t.v[0].map(|c| c as f32)))
            .abs()
            < 2.0
    });
    on.map_or((from - dot).normalize_or(Vec3::Y), |t| {
        Vec3::from_array(t.normal)
    })
}

#[allow(clippy::too_many_arguments)]
fn shine(
    own: Res<Own>,
    timeline: Res<LocalTimeline>,
    world: Res<Collision>,
    players: Query<(Entity, &Combat, Has<Predicted>, Option<&Shining>)>,
    children: Query<&Children>,
    joints: Query<(&Name, &GlobalTransform)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut parts: Query<(
        &mut Transform,
        &mut Visibility,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Res<AssetServer>,
    mut quad: Local<Option<Handle<Mesh>>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let quad = quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0)));
    let eye = camera.iter().next().map(|c| c.translation());
    for (player, c, predicted, shining) in &players {
        let laser = if predicted {
            own.0
        } else {
            c.laser.map(|p| p.map(f32::from))
        };
        let (Some(dot), Some(s)) = (laser, shining) else {
            if let Some(s) = shining.filter(|_| laser.is_none()) {
                commands.entity(s.dot).despawn();
                commands.entity(s.beam).despawn();
                commands.entity(player).remove::<Shining>();
            } else if laser.is_some() {
                let mut part = |png: &'static str, wrap: bool| {
                    let mut sampler = ImageSamplerDescriptor::linear();
                    if wrap {
                        sampler.address_mode_u = ImageAddressMode::Repeat;
                        sampler.address_mode_v = ImageAddressMode::Repeat;
                    }
                    let sampler = ImageSampler::Descriptor(sampler);
                    let texture = (assets.load_builder())
                        .with_settings(move |s: &mut ImageLoaderSettings| {
                            s.sampler = sampler.clone()
                        })
                        .load(png);
                    let material = materials.add(StandardMaterial {
                        base_color_texture: Some(texture),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        cull_mode: None,
                        ..default()
                    });
                    let at = (Transform::from_scale(Vec3::ZERO), Visibility::Hidden);
                    (commands.spawn((Mesh3d(quad.clone()), MeshMaterial3d(material), at))).id()
                };
                let (dot, beam) = (
                    part("hud/strike_glow.png", false),
                    part("hud/strike_streak.png", true),
                );
                commands.entity(player).insert(Shining {
                    since: tick,
                    dot,
                    beam,
                });
            }
            continue;
        };
        let k = tick.saturating_sub(s.since);
        let (alpha, beam_width, dot_width) = looks(k);
        let tint = colour(c.class).with_alpha(alpha / 255.0);
        let dot = Vec3::from_array(dot);
        let hand = (children.iter_descendants(player))
            .filter_map(|e| joints.get(e).ok())
            .find(|j| j.0.as_str() == "joint_09")
            .map(|j| j.1.transform_point(START));
        let n = facing(&world, dot, hand.or(eye).unwrap_or(dot + Vec3::Y));
        if let Ok((mut t, mut shown, m)) = parts.get_mut(s.dot) {
            *t = Transform::from_translation(dot + n)
                .with_rotation(Quat::from_rotation_arc(Vec3::Z, n))
                .with_scale(Vec3::splat(dot_width));
            *shown = Visibility::Visible;
            if let Some(mut m) = materials.get_mut(&m.0) {
                m.base_color = tint;
            }
        }
        let Ok((mut t, mut shown, m)) = parts.get_mut(s.beam) else {
            continue;
        };
        let (Some(from), Some(eye), false) = (hand, eye, predicted) else {
            *shown = Visibility::Hidden;
            continue;
        };
        // Along the beam, its face turned to the eye.
        let (along, len) = ((dot - from).normalize_or(Vec3::X), from.distance(dot));
        let side = along.cross(eye - from).normalize_or(Vec3::Y);
        let rotation = Quat::from_mat3(&Mat3::from_cols(along, side, along.cross(side)));
        *t = Transform::from_translation((from + dot) / 2.0)
            .with_rotation(rotation)
            .with_scale(Vec3::new(len, beam_width, 1.0));
        *shown = Visibility::Visible;
        if let Some(mut m) = materials.get_mut(&m.0) {
            m.base_color = tint;
            m.uv_transform = Affine2::from_translation(Vec2::new(scroll(k), 0.0))
                * Affine2::from_scale(Vec2::new(len / WRAP, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against las (DLs 259-300, one step every two reads): the alpha 0xFA, 0xF5, 0xF3, 0xF2, 0xF2,
    /// 0xD9, 0xD0, 0xCD, 0xCE, the beam's width 4, 12, 16, 22, 24, 28, 30, 32, 34 … 40 and the
    /// dot's 1, 7, 7, 11, 11, 15, 13, 15, 19 … about 20; the beam's S wraps every 164 units (1194
    /// long, 29721 S units at k = 22).
    #[test]
    fn the_laser_eases_in_as_las() {
        let alpha = [0xFA, 0xF5, 0xF3, 0xF2, 0xF2, 0xD9, 0xD0, 0xCD, 0xCE];
        let beam = [
            4, 12, 16, 22, 24, 28, 30, 32, 34, 34, 36, 36, 38, 38, 38, 38, 38, 40, 40, 40,
        ];
        let dot = [1, 7, 7, 11, 11, 15, 13, 15, 19];
        for (step, &w) in beam.iter().enumerate() {
            let (a, b, d) = looks(2 * step as u32 + 1);
            assert!((b - w as f32).abs() <= 2.0, "beam {step}: {b} {w}");
            if let Some(&v) = alpha.get(step) {
                assert!((a - v as f32).abs() <= 24.0, "alpha {step}: {a} {v}");
            }
            if let Some(&v) = dot.get(step) {
                assert!((d - v as f32).abs() <= 3.5, "dot {step}: {d} {v}");
            }
        }
        assert!((1194.0 / WRAP * 128.0 * 32.0 - 29721.0).abs() < 500.0);
        // The scroll swings up from k = 0 to its top near k = 22, about one wrap.
        assert!(scroll(0) < 0.0 && (scroll(22) - 1.03).abs() < 0.05);
    }

    /// The colour by the holder's model (C): 128 (class 4) green, 117 (class 3) red.
    #[test]
    fn the_laser_is_green_on_model_128() {
        assert_eq!(colour(4), Color::srgb_u8(0x32, 0xFF, 0));
        assert_eq!(colour(3), Color::srgb_u8(0xFF, 0, 0));
    }
}
