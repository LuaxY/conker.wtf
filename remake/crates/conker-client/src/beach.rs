//! War: Beach's world (`../lab/research/mp-original/beach-presentation.md` §2, §5, §7; the HUD is
//! [`crate::hud`]'s, the sounds [`crate::audio`]'s):
//! - each villager's look: its class's mask hides its model's display lists ([`hidden`]: the pack's
//!   `Character::looks`, each glTF primitive's `extras.list`);
//! - an escapee fades out, 16 alpha a frame, and stays hidden while frozen ([`escape_alpha`]);
//! - the plunger (placements 0, 4, 5) at its y, and the water's scroll ([`WATER`]);
//! - the detonation (`MatchView::cooldown` 0 → 900): a ring ([`ring`], the pack's `effects.gltf`'s `ring`)
//!   and debris at each of the three records, a villager's view cut to the level's camera 0x9002 for
//!   60 frames ([`cut`]), every view shaken ([`DETONATION`]);
//! - the strike (a villager's `Combat::deadline` reaching 0): its red light at the villager over the
//!   40 frames to the blast ([`strike_size`]), its red floor glow ([`glow`]) and spinning streak
//!   ([`streak`]), then the blast's explosion, debris and shake ([`BLAST`]);
//! - a rocket's blast shakes the views near it too ([`BLAST`]), on every map (`rocket_shakes`: the
//!   original's rocket code is shared by every map, C).
//!
//! The strike's explosion (I: the original's 11 sparks, model 0x132, are not drawn): a burst of
//! camera-facing puffs of the pack's `explosion.png` in the colours the original's draws use (det1 DL
//! 318: white, grey, black and dark red, alpha 0x64-0xC7); debris falls at every blast.

use std::collections::BTreeMap;

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::{GltfExtras, GltfMaterialName};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_anim::combat::BLOWN_UP;
use conker_assets::{Character, Light, When};
use conker_rules::beach;
use conker_rules::projectile::ROCKET_SPEED;
use conker_sim::net::{Collision, Combat, MatchView, Player};
use lightyear::prelude::{Interpolated, LocalTimeline, Predicted};

use crate::FollowCamera;
use crate::render::{Model, Rig};

/// The detonation's three records (`func_150FCA30`'s `0x800D9AA0`, the bunkers' charges: (x, y + h/2,
/// z), V fx2 `beach-environment-scratch/out/explosion.csv`): a ring at each, all on the detonation's
/// frame (the sounds keep their own places and delays).
pub const RECORDS: [[f32; 3]; 3] = [
    [-360.0, -290.5, -4165.0],
    [-2515.0, 1547.5, -3282.0],
    [1845.0, 875.5, -3369.0],
];
/// The ring (`func_151541B8`, V fx2): xz scale `.0`·√(t − 1), y scale `.1`, shown for frames 0 to
/// `.2` (V: all three gone after t = 12, whatever their L).
pub const RING: (f32, f32, u32) = (6.6944, 2.609, 12);
/// The level camera 0x9002 a villager's view cuts to for [`CUT`] frames after the detonation (V det1
/// reads 318-377, `det1_camera.csv`): its eye from `EYE`, moving `EYE_STEP` a frame, looking at `AT`.
pub const EYE: [f32; 3] = [1521.0, 1371.0, -1446.0];
pub const EYE_STEP: [f32; 3] = [-0.6661, -1.1393, -1.9437];
pub const AT: [f32; 3] = [1414.43, 1143.14, -1757.01];
pub const CUT: u32 = 60;
/// A shake's six channels (`func_15165628`'s cases, `func_1516489C` a frame; V fx2 `shake.csv`): x, y,
/// z in units, pitch, yaw, roll in degrees: each channel's amplitude and angular speed (rad a frame),
/// both × the envelope ([`Wave::envelope`]: up over `attack` frames, held `hold`, down to 1/`decay`
/// over `decay`); from then on each channel stops at its phase's next sign change.
#[derive(Debug)]
pub struct Wave {
    pub amp: [f32; 6],
    pub w: [f32; 6],
    pub attack: u32,
    pub hold: u32,
    pub decay: u32,
}

/// Case 4, the detonation's (at the first record, 2007 then 9005: [`shake_share`]).
#[allow(clippy::approx_constant)] // 1.5707°: the oracle's yaw amplitude, not π/2.
pub const DETONATION: Wave = Wave {
    amp: [8.0, 17.0, 4.0, 0.7614, 1.5707, 1.2668],
    w: [0.587, 0.906, 0.403, 1.1592, 0.6973, 1.3464],
    attack: 5,
    hold: 3,
    decay: 45,
};
/// Case 5, the strike's blast (at the strike, 506 then 1013) and a rocket's (C, at the blast, 800
/// then 2000).
pub const BLAST: Wave = Wave {
    amp: [15.0, 10.0, 19.0, 0.5397, 0.7679, 0.9881],
    w: [1.045, 0.636, 1.404, 0.4526, 0.485, 0.6843],
    attack: 5,
    hold: 5,
    decay: 25,
};
/// The strike's streak: its length and its three angles' steps (rad a frame, about x, y, z; I: how
/// `func_15140410` builds the quad is not decoded).
pub const STREAK: (f32, [f32; 3]) = (1500.0, [0.285, 0.106, -0.086]);
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

/// Each body's looks (class → hidden display lists: the villagers', 0xF's) and the lists its
/// model always hides (`hidden`), from the pack's characters.
#[derive(Resource)]
struct Looks(Vec<(BTreeMap<u8, u8>, u16)>);

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

/// A ring's xz scale and alpha `t` frames on, `a` its alpha and `l` its L (`a = 200 + rand % 56`,
/// `L = 12 + 4r`): alpha a·(1 − ((t − 1)/L)²), a to t = 1; none once over.
pub fn ring(t: u32, a: f32, l: f32) -> Option<(f32, f32)> {
    let (k, _, last) = RING;
    let s = t.saturating_sub(1) as f32;
    let alpha = a * (1.0 - (s / l).powi(2));
    (t <= last && alpha > 0.0).then(|| (k * s.sqrt(), alpha))
}

impl Wave {
    /// The envelope `k` frames on.
    pub fn envelope(&self, k: u32) -> f32 {
        let (a, h, d) = (self.attack, self.hold, self.decay);
        match k {
            k if k <= a => k as f32 / a as f32,
            k if k <= a + h => 1.0,
            k => (1.0 - (k - a - h) as f32 / d as f32).max(1.0 / d as f32),
        }
    }

    /// The channels `k` frames on: amplitude × envelope × sin φ, φ from 0 stepping w × envelope a
    /// frame (wrapped), a stopped channel 0; none once all six have stopped (V: the stops' frames).
    pub fn at(&self, k: u32) -> Option<[f32; 6]> {
        let end = self.attack + self.hold + self.decay;
        let (mut phase, mut on) = ([0.0f32; 6], [true; 6]);
        for j in 1..=k {
            let e = self.envelope(j);
            for c in 0..6 {
                if on[c] {
                    let before = phase[c].sin();
                    phase[c] = (phase[c] + self.w[c] * e).rem_euclid(std::f32::consts::TAU);
                    on[c] = j < end || before * phase[c].sin() > 0.0;
                }
            }
        }
        let e = self.envelope(k);
        let channel = |c: usize| match on[c] {
            true => self.amp[c] * e * phase[c].sin(),
            false => 0.0,
        };
        on.contains(&true).then(|| std::array::from_fn(channel))
    }
}

/// A shake's share at `d` from its point (`func_15165BB0`): 1 within `near`, down to 0 over `range`.
pub fn shake_share(near: f32, range: f32, d: f32) -> f32 {
    (1.0 - (d - near).max(0.0) / range).max(0.0)
}

/// The strike's floor glow's scale `k` frames after the call, its blast `b` frames after it (V fx2
/// `strike.csv`): 0 at k = 1, +0.04 a frame to 1.2, held to the blast, then −0.12 a frame, gone at
/// the blast + 10. Its quad is ±50 × it.
pub fn glow(k: u32, b: Option<u32>) -> Option<f32> {
    grow(k, b, [0.04, 1.2, 0.12])
}

/// The strike's streak's width (V fx2): 0 at k = 1, +2 a frame to 60, held, then −6 a frame.
pub fn streak(k: u32, b: Option<u32>) -> Option<f32> {
    grow(k, b, [2.0, 60.0, 6.0])
}

fn grow(k: u32, b: Option<u32>, [up, cap, down]: [f32; 3]) -> Option<f32> {
    let at = |k: u32| (up * k.saturating_sub(1) as f32).min(cap);
    match b {
        Some(b) if k > b => Some(at(b) - down * (k - b) as f32).filter(|v| k < b + 10 && *v > 0.0),
        _ => Some(at(k)),
    }
}

/// What this client saw go off, by its own ticks: the detonation (for the camera), the strikes (the
/// HUD's flash reads the local player's), the shakes running, and last tick's rockets.
#[derive(Resource, Default)]
pub struct Blasts {
    pub detonation: Option<u32>,
    pub strikes: Vec<Strike>,
    pub shakes: Vec<Shake>,
    cooldown: Option<u16>,
    deadlines: BTreeMap<u16, u16>,
    rockets: Option<Vec<[f32; 3]>>,
}

/// A shake on every view: its wave from tick `from`, and its point, near and range
/// ([`shake_share`] of the view's distance).
#[derive(Clone, Copy, Debug)]
pub struct Shake {
    pub wave: &'static Wave,
    pub from: u32,
    pub at: Vec3,
    pub near: f32,
    pub range: f32,
}

/// A strike on the villager in `slot` at `at`, called at tick `call`, its blast at `blast`.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub slot: u16,
    pub at: [f32; 3],
    pub call: u32,
    pub blast: Option<u32>,
}

/// The puffs' quad and texture, the strike's glow and streak textures.
#[derive(Resource)]
pub(crate) struct Sprites {
    pub(crate) quad: Handle<Mesh>,
    texture: Handle<Image>,
    glow: Handle<Image>,
    pub(crate) streak: Handle<Image>,
}

/// A detonation ring: its first tick, alpha a and L ([`ring`]), and its own material once its scene
/// is in.
#[derive(Component)]
struct Ring {
    from: u32,
    alpha: f32,
    life: f32,
    material: Option<Handle<StandardMaterial>>,
}

/// A strike's floor glow or its streak: the strike's slot and call tick.
#[derive(Component)]
struct Mark {
    slot: u16,
    call: u32,
    streak: bool,
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
    let looks = characters
        .iter()
        .map(|c| (c.looks.clone(), c.hidden.unwrap_or(0)));
    app.insert_resource(Looks(looks.collect()));
    app.init_resource::<Blasts>();
    app.add_systems(
        Startup,
        |mut meshes: ResMut<Assets<Mesh>>, assets: Res<AssetServer>, mut commands: Commands| {
            commands.insert_resource(Sprites {
                quad: meshes.add(Rectangle::new(1.0, 1.0)),
                texture: assets.load("hud/explosion.png"),
                glow: assets.load("hud/strike_glow.png"),
                streak: assets.load("hud/strike_streak.png"),
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
            (watch, strike_lights, puffs, rings, marks).chain(),
        ),
    );
}

/// Each villager's (each looked) model shows its class's look: the primitives of its hidden lists
/// hide, with those its model always hides (`render`'s `hide_lists`; no model has both today).
#[allow(clippy::type_complexity)]
fn dress(
    looks: Res<Looks>,
    players: Query<(Entity, &Combat, &Model, Option<&Dressed>), With<Rig>>,
    children: Query<&Children>,
    mut parts: Query<(&GltfExtras, &mut Visibility)>,
    mut commands: Commands,
) {
    for (entity, c, model, dressed) in &players {
        let Some((looks, base)) = looks.0.get(model.body).filter(|l| !l.0.is_empty()) else {
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
            let always = list < 16 && base >> list & 1 != 0;
            v.set_if_neq(match always || hidden(looks, c.class, list) {
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

/// An explosion's puffs (white, grey, black and dark red) and its chunks of debris at `at`: `puffs`
/// puffs within `radius`, `chunks` chunks (Beach's strikes and detonation, Tank's shells).
pub(crate) fn burst(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    sprites: &Sprites,
    seed: &mut u32,
    at: Vec3,
    (puffs, radius, chunks): (u32, f32, u32),
) {
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
}

/// The detonation, the strikes and the rockets' blasts as this client sees them ([`Blasts`]): rings,
/// explosions, debris, the strikes' marks and the shakes where they go off.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn watch(
    game: Query<&MatchView>,
    players: Query<(&Player, &Combat), Or<(With<Predicted>, With<Interpolated>)>>,
    timeline: Res<LocalTimeline>,
    sprites: Option<Res<Sprites>>,
    world: Res<Collision>,
    assets: Res<AssetServer>,
    mut blasts: ResMut<Blasts>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
    mut ring_model: Local<Option<Handle<WorldAsset>>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    if let Ok(m) = game.single() {
        rocket_shakes(&mut blasts, &m.rockets, tick);
    }
    let (Some(m), Some(sprites)) = (game.single().ok().filter(|m| m.beach), sprites) else {
        return;
    };
    let ring_model = (ring_model.get_or_insert_with(|| {
        let gltf = conker_assets::effects_gltf(conker_assets::PACK_WEAPONS);
        assets.load(format!("{gltf}#Scene0"))
    }))
    .clone();
    let b = &mut *blasts;
    let mut burst = |at: Vec3, puffs: u32, radius: f32, chunks: u32| {
        burst(
            &mut commands,
            &mut materials,
            &sprites,
            &mut seed,
            at,
            (puffs, radius, chunks),
        );
    };
    let shake = |wave: &'static Wave, at: [f32; 3], near: f32, range: f32| Shake {
        wave,
        from: tick,
        at: Vec3::from_array(at),
        near,
        range,
    };
    let detonation = b.cooldown == Some(0) && m.cooldown > 0;
    if detonation {
        info!("t={tick} beach: the detonation");
        b.detonation = Some(tick);
        b.shakes
            .push(shake(&DETONATION, RECORDS[0], 2007.0, 9005.0));
        for at in RECORDS {
            burst(Vec3::from_array(at), 0, 220.0, 4);
        }
    }
    b.cooldown = Some(m.cooldown);
    let mut called = Vec::new();
    for (p, c) in &players {
        let last = b.deadlines.insert(c.slot, c.deadline);
        if last.is_some_and(|d| d > 0) && c.deadline == 0 && c.team == beach::VILLAGERS {
            info!("t={tick} beach: the strike on slot {}", c.slot);
            let s = Strike {
                slot: c.slot,
                at: p.pos,
                call: tick,
                blast: None,
            };
            b.strikes.push(s);
            called.push(s);
        }
        for s in b.strikes.iter_mut().filter(|s| s.slot == c.slot) {
            if s.blast.is_none() && (c.anim == BLOWN_UP || tick >= s.call + STRIKE) {
                s.blast = Some(tick);
                burst(Vec3::from_array(s.at), 16, 120.0, 3);
                b.shakes.push(shake(&BLAST, s.at, 506.0, 1013.0));
            }
        }
    }
    b.strikes.retain(|s| s.blast.is_none_or(|t| tick < t + 60));
    // The rings: random yaw, a = 200 + rand % 56, L = 12 + 4r.
    let mut rng = conker_rules::Rng::new(u64::from(tick) << 8 | 1);
    for at in RECORDS.into_iter().filter(|_| detonation) {
        let turn = Quat::from_rotation_y(rng.next_f32() * std::f32::consts::TAU);
        commands.spawn((
            Ring {
                from: tick,
                alpha: (200 + rng.next_u32() % 56) as f32,
                life: 12.0 + 4.0 * rng.next_f32(),
                material: None,
            },
            WorldAssetRoot(ring_model.clone()),
            Transform::from_translation(Vec3::from_array(at)).with_rotation(turn),
            Visibility::Hidden,
        ));
    }
    // The strike's marks: the glow on the floor below the villager's y + 100 (+ 5), facing up; the
    // streak at the villager.
    let mut red = |texture: &Handle<Image>| {
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            base_color_texture: Some(texture.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }))
    };
    for s in called {
        let at = Vec3::from_array(s.at);
        let mark = |streak| Mark {
            slot: s.slot,
            call: s.call,
            streak,
        };
        if let Some(floor) = world.0.floor(at.x, at.z, -1e9, at.y + 50.0, None) {
            let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            commands.spawn((
                mark(false),
                Mesh3d(sprites.quad.clone()),
                red(&sprites.glow),
                Transform::from_xyz(at.x, floor.y + 5.0, at.z)
                    .with_rotation(flat)
                    .with_scale(Vec3::ZERO),
            ));
        }
        commands.spawn((
            mark(true),
            Mesh3d(sprites.quad.clone()),
            red(&sprites.streak),
            Transform::from_translation(at).with_scale(Vec3::ZERO),
        ));
    }
}

/// On every map, a rocket gone since last tick shakes the views near where it went ([`BLAST`] at
/// 800/2000: the rocket pool's `func_151C05A4`, C, which every map's rockets run); shakes that are
/// over are dropped.
fn rocket_shakes(b: &mut Blasts, rockets: &[[f32; 3]], tick: u32) {
    let old = b.rockets.replace(rockets.to_vec());
    for at in (old.iter()).flat_map(|old| crate::audio::gone(old, rockets, 2.0 * ROCKET_SPEED)) {
        b.shakes.push(Shake {
            wave: &BLAST,
            from: tick,
            at: Vec3::from_array(at),
            near: 800.0,
            range: 2000.0,
        });
    }
    b.shakes
        .retain(|s| s.wave.at(tick.saturating_sub(s.from)).is_some());
}

/// The rings grow and fade ([`ring`]) on their own copies of the model's material; gone once over.
fn rings(
    timeline: Res<LocalTimeline>,
    mut rings: Query<(Entity, &mut Ring, &mut Transform, &mut Visibility)>,
    children: Query<&Children>,
    mut shown: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    for (e, mut r, mut t, mut v) in &mut rings {
        let Some((xz, alpha)) = ring(tick.saturating_sub(r.from), r.alpha, r.life) else {
            commands.entity(e).despawn();
            continue;
        };
        t.scale = Vec3::new(xz, RING.1, xz);
        v.set_if_neq(match xz > 0.0 {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        if r.material.is_none() {
            for part in children.iter_descendants(e) {
                if let Ok(mut m) = shown.get_mut(part) {
                    let own = match &r.material {
                        Some(own) => own.clone(),
                        None => {
                            let copy = materials.get(&m.0).cloned().unwrap_or_default();
                            r.material.insert(materials.add(copy)).clone()
                        }
                    };
                    m.0 = own;
                }
            }
        }
        if let Some(mut m) = r.material.as_ref().and_then(|m| materials.get_mut(m)) {
            m.base_color.set_alpha(alpha / 255.0);
        }
    }
}

/// The strikes' glows and streaks at their size ([`glow`], [`streak`]); the streak spins
/// ([`STREAK`]). Gone with their size.
fn marks(
    blasts: Res<Blasts>,
    timeline: Res<LocalTimeline>,
    mut marks: Query<(Entity, &Mark, &mut Transform)>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    for (e, m, mut t) in &mut marks {
        let k = tick.saturating_sub(m.call);
        let strike = (blasts.strikes.iter()).find(|s| (s.slot, s.call) == (m.slot, m.call));
        let size = strike.and_then(|s| {
            let b = s.blast.map(|b| b - s.call);
            if m.streak { streak(k, b) } else { glow(k, b) }
        });
        let Some(size) = size else {
            commands.entity(e).despawn();
            continue;
        };
        if m.streak {
            let (length, [x, y, z]) = STREAK;
            let k = k as f32;
            t.rotation = Quat::from_euler(EulerRot::XYZ, x * k, y * k, z * k);
            t.scale = Vec3::new(length, size, 1.0);
        } else {
            t.scale = Vec3::new(100.0 * size, 100.0 * size, 1.0);
        }
    }
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

/// After the follow camera: the detonation's cut for a villager's view, and the shakes on every view
/// ([`Blasts::shakes`], each by its share at the view: x, y, z on the eye, pitch, yaw and roll on its
/// angles; not while the free camera flies).
pub(crate) fn camera(
    blasts: Res<Blasts>,
    timeline: Res<LocalTimeline>,
    own: Query<&Combat, With<FollowCamera>>,
    camera: Single<(&mut Transform, &FreeCameraState)>,
) {
    let (mut transform, free) = camera.into_inner();
    let (Ok(own), false) = (own.single(), free.enabled) else {
        return;
    };
    let tick = timeline.tick().0;
    let k = |from: u32| tick.saturating_sub(from);
    if let Some((eye, look)) = (blasts.detonation)
        .and_then(|at| cut(k(at)))
        .filter(|_| own.team == 0)
    {
        *transform = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
    let mut sum = [0.0f32; 6];
    for s in &blasts.shakes {
        let share = shake_share(s.near, s.range, transform.translation.distance(s.at));
        for (sum, c) in sum.iter_mut().zip(s.wave.at(k(s.from)).unwrap_or_default()) {
            *sum += c * share;
        }
    }
    if sum == [0.0; 6] {
        return;
    }
    let [x, y, z, pitch, yaw, roll] = sum;
    transform.translation += Vec3::new(x, y, z);
    let (y0, x0, z0) = transform.rotation.to_euler(EulerRot::YXZ);
    let [pitch, yaw, roll] = [pitch, yaw, roll].map(f32::to_radians);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, y0 + yaw, x0 + pitch, z0 + roll);
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
    }

    /// The rings against fx2's `explosion.csv` (alpha at t = 0..12, a and L each ring's: 211 and
    /// 12.556, 230 and 12.232, 223 and 12.012; the xz scale the same for all three), gone after 12.
    #[test]
    fn the_rings_follow_the_original() {
        let rows: [(f32, f32, [u8; 13]); 3] = [
            (
                211.0,
                12.556,
                [
                    211, 211, 209, 205, 198, 189, 177, 162, 145, 125, 102, 77, 49,
                ],
            ),
            (
                230.0,
                12.232,
                [
                    230, 230, 228, 223, 216, 205, 191, 174, 155, 132, 106, 77, 44,
                ],
            ),
            (
                223.0,
                12.012,
                [223, 223, 221, 216, 209, 198, 184, 167, 147, 124, 98, 69, 36],
            ),
        ];
        let scales = [
            0.0, 0.0, 6.694, 9.467, 11.595, 13.389, 14.969, 16.398, 17.711, 18.934,
        ];
        for (a, l, alphas) in rows {
            for (t, want) in alphas.into_iter().enumerate() {
                let (xz, alpha) = ring(t as u32, a, l).unwrap();
                assert!((alpha - f32::from(want)).abs() < 1.0, "{a} t={t}: {alpha}");
                if let Some(s) = scales.get(t) {
                    assert!((xz - s).abs() < 0.001, "t={t}: {xz}");
                }
            }
            assert_eq!(ring(13, a, l), None);
        }
        assert!((ring(12, 211.0, 12.556).unwrap().0 - 22.203).abs() < 0.001);
    }

    /// The shakes against fx2's `shake.csv` (frame 0 the creation's): the envelope's amplitudes (x:
    /// 1.6 a frame up to 8, held to frame 8, 7.8222 at 9, 4.8 at 26, 8/45 from 52), and the frames
    /// each channel stops at, its angular speed 0 (detonation: pitch 134, roll 157, z 196, y 197, yaw
    /// 212, x after the dump's last frame 237; the strike's blast: z 38, pitch 55, x 61, roll 108, y
    /// 152, yaw after 181); the distance share.
    #[test]
    fn the_shakes_follow_the_original() {
        let x = |w: &Wave, k| w.amp[0] * w.envelope(k);
        for (k, want) in [
            (0, 0.0),
            (1, 1.6),
            (5, 8.0),
            (8, 8.0),
            (9, 7.8222),
            (26, 4.8),
        ] {
            assert!((x(&DETONATION, k) - want).abs() < 1e-4, "{k}");
        }
        assert!(
            [52, 53, 237]
                .iter()
                .all(|&k| (x(&DETONATION, k) - 8.0 / 45.0).abs() < 1e-4)
        );
        assert!((BLAST.amp[2] * BLAST.envelope(1) - 3.8).abs() < 1e-4);
        assert!((x(&BLAST, 181) - 0.6).abs() < 1e-4);
        let stops = |w: &Wave| -> [u32; 6] {
            std::array::from_fn(|c| {
                (1..400)
                    .find(|&k| w.at(k).is_none_or(|o| o[c] == 0.0))
                    .unwrap()
            })
        };
        assert_eq!(stops(&DETONATION), [238, 197, 196, 134, 212, 157]);
        assert_eq!(stops(&BLAST), [61, 152, 38, 55, 182, 108]);
        assert!(DETONATION.at(237).is_some() && DETONATION.at(238).is_none());
        let o = DETONATION.at(5).unwrap();
        assert!(o[1].abs() <= 17.0 && o[1] != 0.0);
        assert_eq!(
            [0.0, 2007.0, 9005.0 + 2007.0].map(|d| shake_share(2007.0, 9005.0, d)),
            [1.0, 1.0, 0.0]
        );
        assert!((shake_share(506.0, 1013.0, 1012.5) - 0.5).abs() < 1e-6);
    }

    /// A rocket that goes off shakes the views on any map (the watch's rocket half runs before its
    /// War: Beach gate): a [`BLAST`] at its last spot, 800/2000, dropped once over.
    #[test]
    fn a_rocket_shakes_every_map() {
        let mut b = Blasts::default();
        rocket_shakes(&mut b, &[[0.0, 0.0, 0.0]], 10);
        rocket_shakes(&mut b, &[[0.0, 0.0, 200.0]], 11);
        assert!(b.shakes.is_empty(), "a rocket in flight");
        rocket_shakes(&mut b, &[], 12);
        let s = b.shakes[0];
        assert_eq!(
            (s.from, s.at, s.near, s.range),
            (12, Vec3::new(0.0, 0.0, 200.0), 800.0, 2000.0)
        );
        assert_eq!(s.wave.amp, BLAST.amp);
        rocket_shakes(&mut b, &[], 12 + 182);
        assert!(b.shakes.is_empty());
    }

    /// The strike's glow and streak against fx2's `strike.csv` (the call at read 304, the blast at
    /// 345: k = 41): 0 at k = 1, the glow +0.04 and the streak +2 a frame to 1.2 and 60 at k = 31,
    /// held, then −0.12 and −6 a frame, gone at k = 51.
    #[test]
    fn the_strike_marks_follow_the_original() {
        let b = Some(41);
        let near = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-4);
        for (k, g, w) in [
            (1, 0.0, 0.0),
            (2, 0.04, 2.0),
            (16, 0.6, 30.0),
            (31, 1.2, 60.0),
        ] {
            assert!(near(glow(k, None), g) && near(glow(k, b), g), "{k}");
            assert!(near(streak(k, b), w), "{k}");
        }
        for (k, g, w) in [(41, 1.2, 60.0), (42, 1.08, 54.0), (50, 0.12, 6.0)] {
            assert!(near(glow(k, b), g) && near(streak(k, b), w), "{k}");
        }
        assert_eq!((glow(51, b), streak(51, b)), (None, None));
        assert!(near(glow(200, None), 1.2));
    }
}
