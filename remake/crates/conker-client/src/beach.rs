//! War: Beach's world (`../lab/research/mp-original/beach-presentation.md` §2, §5, §7; the HUD is
//! [`crate::hud`]'s, the sounds [`crate::audio`]'s):
//! - each villager's look: its class's mask hides its model's display lists ([`hidden`]: the pack's
//!   `Character::looks`, each glTF primitive's `extras.list`);
//! - an escapee fades out, 16 alpha a frame, and stays hidden while frozen ([`escape_alpha`]);
//! - the plunger (placements 0, 4, 5) at its y, and the water's scroll ([`WATER`]);
//! - the detonation (`MatchView::cooldown` 0 → 900): explosions and debris at the three records, a
//!   villager's view cut to the level's camera 0x9002 for 60 frames ([`cut`]), every view shaken
//!   ([`shake`]);
//! - the strike (a villager's `Combat::deadline` reaching 0): its red light at the villager over the
//!   40 frames to the blast ([`strike_size`]), then the blast's explosion and debris.
//!
//! The explosions (I: their sizes over time and the particle emitter are not decoded): each a burst of
//! camera-facing puffs of the pack's `explosion.png` in the colours the original's draws use (det1 DL
//! 318: white, grey, black and dark red, alpha 0x64-0xC7), and a few chunks of debris falling.

use std::collections::BTreeMap;

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::{GltfExtras, GltfMaterialName};
use bevy::math::Affine2;
use bevy::prelude::*;
use conker_anim::combat::BLOWN_UP;
use conker_assets::{Character, Light, When};
use conker_rules::beach;
use conker_sim::net::{Combat, MatchView, Player};
use lightyear::prelude::{Interpolated, LocalTimeline, Predicted};

use crate::FollowCamera;
use crate::render::{Model, Rig};

/// The detonation's three records (`func_150FCA30`'s `0x800D9AA0`, §6-§7: the bunkers' charges).
pub const RECORDS: [[f32; 3]; 3] = [
    [-319.0, -273.0, -3419.0],
    [-2038.0, 1560.0, -2631.0],
    [1519.0, 947.0, -2633.0],
];
/// The level camera 0x9002 a villager's view cuts to for [`CUT`] frames after the detonation (V det1
/// reads 318-377, `det1_camera.csv`): its eye from `EYE`, moving `EYE_STEP` a frame, looking at `AT`.
pub const EYE: [f32; 3] = [1521.0, 1371.0, -1446.0];
pub const EYE_STEP: [f32; 3] = [-0.6661, -1.1393, -1.9437];
pub const AT: [f32; 3] = [1414.43, 1143.14, -1757.01];
pub const CUT: u32 = 60;
/// The shake on every view (`func_151D8868 {1, 100, 8, 0xF, −1}`): 8 units for 100 VIs (I: how it
/// moves the view is not read; here a jitter of up to 8 that dies away).
pub const SHAKE: (f32, u32) = (8.0, 50);
/// The water: placement 1's node and its material 50 (`m50-tex0530`, 32 × 64), whose tile's T origin
/// rises a quarter texel a frame (V: 0x57E at DL 401 → 0x6A9 at DL 700). v = 1 − (t − origin) / 64
/// (`extract_gbi`'s `uv`), so v rises 0.25 / 64 a frame.
pub const WATER: (&str, &str, f32) = ("04-2b-05@1", "m50-tex0530", 0.25 / 64.0);
/// The plunger's props (placements 0, 4, 5: one body, V the groundwork's RAM).
const PLUNGER: [u32; 3] = [0, 4, 5];
/// Frames from the strike's call to its blast (`damage::STRIKE`'s HIT).
const STRIKE: u32 = 40;

/// The explosions' colours (det1 DL 318's prim colours) and their alphas' range.
const PUFFS: [[u8; 3]; 4] = [[255, 255, 255], [128, 128, 128], [0, 0, 0], [0x59, 0, 0]];
const PUFF_ALPHA: [u8; 2] = [0x64, 0xC7];

/// Each body's villager looks (class → hidden display lists), from the pack's characters.
#[derive(Resource)]
struct Looks(Vec<BTreeMap<u8, u8>>);

/// Whether a villager of `class` hides display list `list` of its model (`looks`: its body's;
/// `func_15083384`'s obj+0x94: bit k hides list k).
pub fn hidden(looks: &BTreeMap<u8, u8>, class: u8, list: u64) -> bool {
    looks
        .get(&class)
        .is_some_and(|m| list < 8 && m >> list & 1 != 0)
}

/// An escapee's alpha `k` frames into its freeze (V e1: 255 → 223 at read 103, 111 at 110, 0 by
/// 117: 16 a frame from the frame it stepped in).
pub fn escape_alpha(k: u32) -> u8 {
    255u32.saturating_sub(16 * k) as u8
}

/// The strike's red light's size `k` frames after the call (V ev3 DL 1004-1050: 86 → 9 → 38 → 25
/// by the blast; I: straight between them, a third of the way each).
pub fn strike_size(k: u32) -> f32 {
    const KEYS: [f32; 4] = [86.0, 9.0, 38.0, 25.0];
    let t = (k.min(STRIKE) as f32 / STRIKE as f32) * 3.0;
    let i = (t as usize).min(2);
    KEYS[i] + (KEYS[i + 1] - KEYS[i]) * (t - i as f32)
}

/// A villager's view `k` frames after the detonation: the level camera's eye and look-at for [`CUT`]
/// frames, else none.
pub fn cut(k: u32) -> Option<(Vec3, Vec3)> {
    let step = Vec3::from_array(EYE_STEP) * k as f32;
    (k < CUT).then(|| (Vec3::from_array(EYE) + step, Vec3::from_array(AT)))
}

/// The shake's offset `k` frames after the detonation (I, [`SHAKE`]).
pub fn shake(k: u32) -> Vec3 {
    let (size, frames) = SHAKE;
    if k >= frames {
        return Vec3::ZERO;
    }
    let left = size * (1.0 - k as f32 / frames as f32);
    let k = k as f32;
    Vec3::new((k * 12.9898).sin(), (k * 78.233).sin(), (k * 37.719).sin()) * left
}

/// What this client saw go off, by its own ticks: the detonation (for the camera and the shake) and
/// the strikes (the HUD's flash reads the local player's).
#[derive(Resource, Default)]
pub struct Blasts {
    pub detonation: Option<u32>,
    pub strikes: Vec<Strike>,
    cooldown: Option<u16>,
    deadlines: BTreeMap<u16, u16>,
}

/// A strike on the villager in `slot` at `at`, called at tick `call`, its blast at `blast`.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub slot: u16,
    pub at: [f32; 3],
    pub call: u32,
    pub blast: Option<u32>,
}

/// The puffs' quad and texture.
#[derive(Resource)]
struct Sprites {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
}

/// A puff of an explosion, or a piece of debris (falling): game units and frames.
#[derive(Component)]
struct Puff {
    vel: Vec3,
    fall: bool,
    size: [f32; 2],
    age: f32,
    life: f32,
    alpha: f32,
}

/// The strike's red light.
#[derive(Component)]
struct StrikeLight(u16);

/// An escapee's model, fading: its first freeze, and each primitive's own material (with the
/// original's handle, put back once it is over).
#[derive(Component)]
struct Escaping {
    from: u8,
    materials: Vec<(Entity, Handle<StandardMaterial>, Handle<StandardMaterial>)>,
}

/// The class a player's model is dressed as, and the model.
#[derive(Component)]
struct Dressed(u8, Entity);

pub fn build(app: &mut App, characters: &[Character]) {
    app.insert_resource(Looks(characters.iter().map(|c| c.looks.clone()).collect()));
    app.init_resource::<Blasts>();
    app.add_systems(
        Startup,
        |mut meshes: ResMut<Assets<Mesh>>, assets: Res<AssetServer>, mut commands: Commands| {
            commands.insert_resource(Sprites {
                quad: meshes.add(Rectangle::new(1.0, 1.0)),
                texture: assets.load("hud/explosion.png"),
            });
        },
    );
    app.add_systems(
        Update,
        (
            dress,
            fade_escapees,
            sink_plunger,
            scroll_water,
            (watch, strike_lights, puffs).chain(),
        ),
    );
}

/// Each villager's model shows its class's look: the primitives of its hidden lists hide.
#[allow(clippy::type_complexity)]
fn dress(
    looks: Res<Looks>,
    players: Query<(Entity, &Combat, &Model, Option<&Dressed>), With<Rig>>,
    children: Query<&Children>,
    mut parts: Query<(&GltfExtras, &mut Visibility)>,
    mut commands: Commands,
) {
    for (entity, c, model, dressed) in &players {
        let Some(looks) = looks.0.get(model.body).filter(|l| !l.is_empty()) else {
            continue;
        };
        if dressed.is_some_and(|d| d.0 == c.class && d.1 == model.scene) {
            continue;
        }
        for e in children.iter_descendants(model.scene) {
            let Ok((extras, mut v)) = parts.get_mut(e) else {
                continue;
            };
            let list = serde_json::from_str::<serde_json::Value>(&extras.value).ok();
            let Some(list) = list.and_then(|x| x.get("list")?.as_u64()) else {
                continue;
            };
            v.set_if_neq(match hidden(looks, c.class, list) {
                true => Visibility::Hidden,
                false => Visibility::Inherited,
            });
        }
        commands
            .entity(entity)
            .insert(Dressed(c.class, model.scene));
    }
}

/// An escapee (`Combat::freeze` running) fades out on its own copies of its materials, then hides
/// until its freeze is over; then its materials come back.
#[allow(clippy::type_complexity)]
fn fade_escapees(
    mut players: Query<(Entity, &Combat, &Model, Option<&mut Escaping>)>,
    children: Query<&Children>,
    mut shown: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut visibility: Query<&mut Visibility>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, c, model, escaping) in &mut players {
        match (c.freeze > 0, escaping) {
            (true, None) => {
                let mut own = Vec::new();
                for e in children.iter_descendants(model.scene) {
                    let Ok(mut m) = shown.get_mut(e) else {
                        continue;
                    };
                    let mut copy = materials.get(&m.0).cloned().unwrap_or_default();
                    copy.alpha_mode = AlphaMode::Blend;
                    let copy = materials.add(copy);
                    own.push((e, std::mem::replace(&mut m.0, copy.clone()), copy));
                }
                let from = c.freeze;
                commands.entity(entity).insert(Escaping {
                    from,
                    materials: own,
                });
            }
            (true, Some(esc)) => {
                let k = u32::from(esc.from.saturating_sub(c.freeze)) + 1;
                let alpha = escape_alpha(k);
                for (_, _, m) in &esc.materials {
                    if let Some(mut m) = materials.get_mut(m) {
                        m.base_color.set_alpha(f32::from(alpha) / 255.0);
                    }
                }
                if let Ok(mut v) = visibility.get_mut(model.scene) {
                    v.set_if_neq(match alpha {
                        0 => Visibility::Hidden,
                        _ => Visibility::Inherited,
                    });
                }
            }
            (false, Some(esc)) => {
                for (e, original, _) in &esc.materials {
                    if let Ok(mut m) = shown.get_mut(*e) {
                        m.0 = original.clone();
                    }
                }
                if let Ok(mut v) = visibility.get_mut(model.scene) {
                    v.set_if_neq(Visibility::Inherited);
                }
                commands.entity(entity).remove::<Escaping>();
            }
            (false, None) => {}
        }
    }
}

/// The plunger's props at its y ([`MatchView::plunger`], 392 at home).
fn sink_plunger(
    game: Query<&MatchView>,
    added: Query<(Entity, &Name), Added<Name>>,
    mut props: Local<Vec<(Entity, f32)>>,
    mut transforms: Query<&mut Transform>,
) {
    for (e, name) in &added {
        let placement = name.rsplit_once('@').and_then(|(_, p)| p.parse().ok());
        if let Some(t) = transforms
            .get(e)
            .ok()
            .filter(|_| placement.is_some_and(|p| PLUNGER.contains(&p)))
        {
            props.push((e, t.translation.y));
        }
    }
    let Some(m) = game.single().ok().filter(|m| m.beach) else {
        return;
    };
    for &(e, y) in props.iter() {
        if let Ok(mut t) = transforms.get_mut(e) {
            let y = y + m.plunger - beach::HOME;
            if t.translation.y != y {
                t.translation.y = y;
            }
        }
    }
}

/// The water's texture scrolls ([`WATER`]), on its own copy of the material.
#[allow(clippy::type_complexity)]
fn scroll_water(
    time: Res<Time>,
    added: Query<(Entity, &Name), Added<Name>>,
    children: Query<&Children>,
    parts: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut water: Local<Option<(Handle<StandardMaterial>, Affine2)>>,
    mut commands: Commands,
) {
    let (node, material, step) = WATER;
    for (e, _) in added.iter().filter(|(_, n)| n.as_str() == node) {
        for part in children.iter_descendants(e) {
            if let Ok((name, m)) = parts.get(part)
                && name.0 == material
            {
                let own = water.get_or_insert_with(|| {
                    let m = materials.get(&m.0).cloned().unwrap_or_default();
                    let base = m.uv_transform;
                    (materials.add(m), base)
                });
                commands.entity(part).insert(MeshMaterial3d(own.0.clone()));
            }
        }
    }
    if let Some((handle, base)) = &*water
        && let Some(mut m) = materials.get_mut(handle)
    {
        let frames = time.elapsed_secs() * conker_sim::TICK_HZ as f32;
        let v = (frames * step).rem_euclid(1.0);
        m.uv_transform = Affine2::from_translation(Vec2::new(0.0, v)) * *base;
    }
}

/// The detonation and the strikes as this client sees them ([`Blasts`]): explosions and debris
/// where they go off.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn watch(
    game: Query<&MatchView>,
    players: Query<(&Player, &Combat), Or<(With<Predicted>, With<Interpolated>)>>,
    timeline: Res<LocalTimeline>,
    sprites: Option<Res<Sprites>>,
    mut blasts: ResMut<Blasts>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
    mut commands: Commands,
) {
    let (Some(m), Some(sprites)) = (game.single().ok().filter(|m| m.beach), sprites) else {
        return;
    };
    let tick = timeline.tick().0;
    let b = &mut *blasts;
    let mut burst = |at: Vec3, puffs: u32, radius: f32, chunks: u32| {
        for n in 0..puffs + chunks {
            *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let mut rng = conker_rules::Rng::new(u64::from(*seed) << 8 | 1);
            let mut r = || rng.next_f32();
            let dir = Vec3::new(r() - 0.5, r() * 0.8, r() - 0.5).normalize_or_zero();
            let chunk = n >= puffs;
            let [red, green, blue] = PUFFS[if chunk { 2 } else { (r() * 4.0) as usize % 4 }];
            let [lo, hi] = PUFF_ALPHA.map(f32::from);
            let size = if chunk {
                12.0 + 10.0 * r()
            } else {
                radius * (0.4 + 0.6 * r())
            };
            let puff = Puff {
                vel: if chunk {
                    Vec3::new(dir.x * 30.0, 25.0 + 20.0 * r(), dir.z * 30.0)
                } else {
                    dir * radius * 0.04 * r()
                },
                fall: chunk,
                size: [size * 0.4, size],
                age: 0.0,
                life: if chunk {
                    40.0 + 30.0 * r()
                } else {
                    16.0 + 24.0 * r()
                },
                alpha: (lo + (hi - lo) * r()) / 255.0,
            };
            let material = materials.add(StandardMaterial {
                base_color: Color::srgba_u8(red, green, blue, (puff.alpha * 255.0) as u8),
                base_color_texture: Some(sprites.texture.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            });
            let start = at + dir * radius * 0.5 * r();
            commands.spawn((
                puff,
                Mesh3d(sprites.quad.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(start).with_scale(Vec3::splat(size * 0.4)),
            ));
        }
    };
    if b.cooldown == Some(0) && m.cooldown > 0 {
        info!("t={tick} beach: the detonation");
        b.detonation = Some(tick);
        for at in RECORDS {
            burst(Vec3::from_array(at), 30, 220.0, 4);
        }
    }
    b.cooldown = Some(m.cooldown);
    for (p, c) in &players {
        let last = b.deadlines.insert(c.slot, c.deadline);
        if last.is_some_and(|d| d > 0) && c.deadline == 0 && c.team == beach::VILLAGERS {
            info!("t={tick} beach: the strike on slot {}", c.slot);
            b.strikes.push(Strike {
                slot: c.slot,
                at: p.pos,
                call: tick,
                blast: None,
            });
        }
        for s in b.strikes.iter_mut().filter(|s| s.slot == c.slot) {
            if s.blast.is_none() && (c.anim == BLOWN_UP || tick >= s.call + STRIKE) {
                s.blast = Some(tick);
                burst(Vec3::from_array(s.at), 16, 120.0, 3);
            }
        }
    }
    b.strikes.retain(|s| s.blast.is_none_or(|t| tick < t + 60));
}

/// Each strike's red light at the villager, until its blast.
fn strike_lights(
    blasts: Res<Blasts>,
    timeline: Res<LocalTimeline>,
    mut lights: Query<(Entity, &StrikeLight, &mut PointLight)>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let red = |size| Light {
        at: [0.0; 3],
        rgb: [255, 0, 0],
        size,
        swing: 0.0,
        step: 0.0,
        when: When::Calm,
        flicker: false,
    };
    for (e, StrikeLight(slot), mut light) in &mut lights {
        match blasts
            .strikes
            .iter()
            .find(|s| s.slot == *slot && s.blast.is_none())
        {
            Some(s) => {
                let l = crate::lights::point_light(&red(0.0), strike_size(tick - s.call));
                (light.intensity, light.range) = (l.intensity, l.range);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for s in blasts.strikes.iter().filter(|s| s.blast.is_none()) {
        if !lights.iter().any(|l| l.1.0 == s.slot) {
            let size = strike_size(tick.saturating_sub(s.call));
            commands.spawn((
                StrikeLight(s.slot),
                crate::lights::point_light(&red(size), size),
                Transform::from_translation(Vec3::from_array(s.at) + Vec3::Y * 40.0),
            ));
        }
    }
}

/// The puffs grow, drift (debris falls) and fade, facing the camera; gone at the end of their life.
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
        if p.fall {
            p.vel.y -= 3.0 * frames;
        }
        t.translation += p.vel * frames;
        t.rotation = facing;
        let k = p.age / p.life;
        let [from, to] = p.size;
        t.scale = Vec3::splat(from + (to - from) * k.sqrt());
        if let Some(mut m) = materials.get_mut(&m.0) {
            m.base_color.set_alpha(p.alpha * (1.0 - k * k));
        }
    }
}

/// After the follow camera: the detonation's cut for a villager's view, and the shake on every view
/// (not while the free camera flies).
pub(crate) fn camera(
    blasts: Res<Blasts>,
    timeline: Res<LocalTimeline>,
    own: Query<&Combat, With<FollowCamera>>,
    camera: Single<(&mut Transform, &FreeCameraState)>,
) {
    let (mut transform, free) = camera.into_inner();
    let Some(at) = blasts.detonation.filter(|_| !free.enabled) else {
        return;
    };
    let k = timeline.tick().0.saturating_sub(at);
    if let Some((eye, look)) = cut(k).filter(|_| own.single().is_ok_and(|c| c.team == 0)) {
        *transform = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
    transform.translation += shake(k);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pack's villagers: each class's mask leaves the lists the original draws (V e1 DL 270: a
    /// 156 of mask 0x0E draws lists 0, 4, 5; DL 99: a 157 of 0x0A draws 0, 2, 4; b1: 0x14's 0x18;
    /// use: 0x11's 0x3A), and every class of 0x11-0x20 has a look.
    #[test]
    fn the_looks_hide_their_lists() {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let read = |model: &str, file: &str| {
            std::fs::read(format!("{dir}/characters/{model}/{file}")).ok()
        };
        let (Some(c156), Some(c157), Some(g156)) = (
            read("156", "character.json"),
            read("157", "character.json"),
            read("156", "character.gltf"),
        ) else {
            eprintln!("no villagers in the pack at {dir}: skipping");
            return;
        };
        let looks = |b: &[u8]| serde_json::from_slice::<Character>(b).unwrap().looks;
        let (l156, l157) = (looks(&c156), looks(&c157));
        assert_eq!(l156.len() + l157.len(), 16);
        for class in 0x11..=0x20u8 {
            let looks = if beach::model(class) == 156 {
                &l156
            } else {
                &l157
            };
            assert!(looks.contains_key(&class), "{class:#x}");
        }
        let shows = |looks: &BTreeMap<u8, u8>, class, lists: u64| -> Vec<u64> {
            (0..lists).filter(|&l| !hidden(looks, class, l)).collect()
        };
        assert_eq!(shows(&l156, 0x1B, 6), [0, 4, 5]);
        assert_eq!(shows(&l157, 0x1E, 5), [0, 2, 4]);
        assert_eq!(shows(&l156, 0x14, 6), [0, 1, 2, 5]);
        assert_eq!(shows(&l156, 0x11, 6), [0, 2]);
        assert_eq!(shows(&l157, 0x1F, 5), [0, 1, 3]);
        // The glTF's primitives carry their list, 0-5, the body (list 0) always shown.
        let g: serde_json::Value = serde_json::from_slice(&g156).unwrap();
        let lists: Vec<u64> = (g["meshes"][0]["primitives"].as_array().unwrap().iter())
            .map(|p| p["extras"]["list"].as_u64().unwrap())
            .collect();
        assert_eq!(lists.iter().max(), Some(&5));
        assert!((0x11..=0x1C).all(|c| !hidden(&l156, c, 0)));
    }

    /// The escapee's alpha (V e1), the strike light's keys, the cut's frames (det1_camera.csv: read
    /// 318 the first, 377 the 60th, 378 the player's camera again).
    #[test]
    fn escapes_strikes_and_the_cut_follow_the_original() {
        assert_eq!(
            [0, 2, 9, 15, 16, 30].map(escape_alpha),
            [255, 223, 111, 15, 0, 0]
        );
        assert_eq!([0, 40, 99].map(strike_size), [86.0, 25.0, 25.0]);
        assert!((strike_size(13) - 11.0).abs() < 3.0 && (strike_size(27) - 38.0).abs() < 2.0);
        let (eye, at) = cut(0).unwrap();
        assert_eq!((eye, at), (Vec3::from_array(EYE), Vec3::from_array(AT)));
        let eye = cut(59).unwrap().0;
        assert!(
            eye.distance(Vec3::new(1481.70, 1303.78, -1560.68)) < 0.05,
            "{eye}"
        );
        assert!(cut(60).is_none());
        assert!(shake(0).length() <= SHAKE.0 * 3f32.sqrt() && shake(50) == Vec3::ZERO);
    }
}
