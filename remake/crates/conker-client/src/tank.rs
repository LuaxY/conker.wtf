//! Tank (`../lab/research/mp-original/tank.md`; this batch's oracle runs in `tank-client-scratch/`):
//! every player a tank, model 119 (the pack's `characters/119` and its `tank` parts):
//! - its turret (joint 2) turned by `PlayerState::turret` about y and its gun (joint 3) pitched by
//!   `PlayerState::pitch` about x, over its clip (C `func_150344A0`; V j1: DL 60 moves joints 2 and 3
//!   only), so the gun points where the rules fire ([`pose`]);
//! - its treads' texture scrolling −7 S units a frame per unit of speed, wrapping at 0x800
//!   (`func_150ED638`, C; [`scroll`]) and its brake lights lit while it brakes (obj+0x6C 22/25, C;
//!   V snd: [`braking`]); its flag by slot is [`crate::render`]'s team look ([`flag`]);
//! - hidden from the frame after its death to its respawn: its wreck stands in (V c1: obj+0x6A,
//!   +0x6B set at 424, the death at 423, cleared at the respawn, 510);
//! - its follow camera behind the hull and the scope's view from the turret ([`camera`]);
//! - the shells (`projectiles.gltf#Scene4` at 1.5, V sh1) and their blasts (Beach's puffs: the
//!   same white, grey, black and dark red sprites, V bm1 DL 363);
//! - the props (`maps/24/props.gltf` and the level's doors): the doors sliding apart along their
//!   own z by [`MatchView::lifts`], the bomb on its post or over its carrier, the wrecks (prop 18
//!   plus the slot) where a tank died; the unused ones hidden ([`owns`]).
//!
//! The HUD's power-up icon and scope are [`crate::hud`]'s, the sounds [`crate::audio`]'s.

use bevy::app::AnimationSystems;
use bevy::gltf::GltfMaterialName;
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::world_serialization::WorldAssetRoot;
use conker_assets::{Character, PACK_WEAPONS, Tank, projectiles_gltf};
use conker_rules::Fighter;
use conker_rules::tank::{BOMB, POST, SCOPE, SHELL_SPEED};
use conker_sim::collision::CollisionWorld;
use conker_sim::follow::Follow;
use conker_sim::movement::{Input, PlayerState, cos_sin};
use conker_sim::net::{Combat, MatchView, Player};
use lightyear::prelude::LocalTimeline;

use crate::render::{Model, Rig, model_yaw};

/// Model 119 (the pack's `characters/119`).
pub const MODEL: &str = "119";
/// The shell's model: `projectiles.gltf` scene 4 at 1.5 (V sh1 DL 42).
const SHELL_SCENE: usize = 4;
const SHELL_SCALE: f32 = 1.5;
/// The treads' scroll: S units a frame per unit of speed, its wrap (C `func_150ED638`), and S units
/// a texel (the vertices' s10.5).
const TREAD_S: f32 = -7.0;
const TREAD_WRAP: f32 = 2048.0;
const S_PER_TEXEL: f32 = 32.0;
/// The brake lights: lit while the speed is past 5 and the target behind it (C: obj+0x6C 22 with
/// speed > 5 and accel ≤ 0, or < −5 and accel ≥ 0).
const BRAKE_SPEED: f32 = 5.0;
/// The follow camera (V t1): behind the hull at rest, 310 back and 159 up, looking at 88.5 over the
/// tank; at full speed (48) 400, 135 and 52; reversing, nearer (V t1 530-549: 293 at −14, 256 at
/// −28, 226 at −40.6). I: straight in the speed's share (−1..1), which follows the speed by a
/// tenth a frame.
/// The camera's heading follows the hull's: this share of the gap a frame standing, plus
/// `follow[1]` at full speed, at least `follow[2]` degrees (I, fitted to t1: 86° → 21.6° in 20
/// frames standing, 15.3° → 0 in 32, about 35° behind turning at speed 39; turning in place the
/// original's lags more, its spring not ported).
const CAM: Behind = Behind {
    rest: [310.0, 159.0, 88.5],
    full: [400.0, 135.0, 52.0],
    top: 48.0,
    share: 0.1,
    follow: [0.067, 0.03, 0.2],
};
/// The camera stops this short of a wall between it and the tank (I).
const CAM_WALL: f32 = 10.0;
/// The scope (power-up 0x02 with R held): the eye 27.5 ahead along the turret and 111.6 up, looking
/// level along the turret's yaw (V pw2 172-200: eye − pos (−27.7, 111.7, 0.7) facing 181.4°, the
/// look a unit along it, level; I: the gun's pitch, which aims high at far targets, not followed),
/// item 0x1F's zoom on opening (`conker_camera::Scope::zoom`); R let go, the camera cuts back
/// behind the hull (V pw2 202: 309.9 back, 159 up).
const SCOPE_EYE: [f32; 2] = [27.5, 111.6];
const SCOPE_ITEM: u8 = 0x1F;
const SCOPE_ZOOM: [f32; 2] = [-38.7, -23.5];
/// The bomb over its carrier (I: item 0x18's look is not found): this high over the tank, at this
/// share of the post's scale.
const CARRIED: (f32, f32) = (190.0, 0.3);
/// The shells' blast (I: the impact `func_15137610` unread; V bm1 DL 363: the death's puffs in the
/// strike's colours): War: Beach's strike's puffs, as many and as wide, without its debris.
const BLAST: (u32, f32, u32) = (16, 120.0, 0);

/// Degrees in game angle units.
const DEG: f32 = 65536.0 / 360.0;

fn deg(a: u16) -> f32 {
    f32::from(a) / DEG
}

fn wrap180(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// `cos_sin` of a heading in degrees.
fn cos_sin_deg(d: f32) -> (f32, f32) {
    cos_sin((d.rem_euclid(360.0) * DEG) as u32 as u16)
}

/// The placements Tank's props system draws (in place of [`crate::render`]'s pickups): the doors
/// (0-7), the bomb (12), the wrecks (18-25) and the props that never show (9, 13-15, 17, 26-33;
/// tank.md §3, §2).
pub fn owns(placement: u32) -> bool {
    matches!(placement, 0..=7 | 9 | 12..=15 | 17..=33)
}

/// The flag's team look for a tank in `slot`: slots 0-3 their own, from 3 up yellow (V j1).
pub fn flag(slot: u16) -> u8 {
    slot.min(3) as u8
}

/// Braking: past [`BRAKE_SPEED`] with the target behind the speed.
pub fn braking(speed: f32, target: f32) -> bool {
    (speed > BRAKE_SPEED && target < speed) || (speed < -BRAKE_SPEED && target > speed)
}

/// The treads' scroll in S units after `frames` frames at `speed` from `s`, wrapped.
pub fn scroll(s: f32, speed: f32, frames: f32) -> f32 {
    (s + TREAD_S * speed * frames).rem_euclid(TREAD_WRAP)
}

/// The turret's and the gun's turn over their clip's pose: the turret about y by its yaw (left of
/// the hull positive, as the rules fire at facing + turret), the gun about x by its pitch (up
/// positive: −x turns +z up).
pub fn turns(turret: f32, pitch: f32) -> (Quat, Quat) {
    (
        Quat::from_rotation_y(turret.to_radians()),
        Quat::from_rotation_x(-pitch.to_radians()),
    )
}

/// The tank's camera state: the heading (degrees) it looks along and the speed's share its
/// distances follow; `None` cuts behind the hull (a spawn, out of the scope).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cam(pub Option<(f32, f32)>);

/// One frame of a tank's view into `follow`'s eye and look-at (in place of the on-foot camera's
/// step: the rules read no camera for a human's tank): the scope's from the turret while power-up
/// 0x02's R is held, else behind the hull, pulled in front of a wall.
pub fn camera(
    follow: &mut Follow,
    cam: &mut Cam,
    f: &Fighter,
    input: &Input,
    world: &CollisionWorld,
) {
    let b = &f.body;
    let c = &mut follow.cam;
    let r = input.buttons & conker_rules::game::BUTTON_R != 0;
    if f.power == SCOPE && r && f.health > 0 {
        let yaw = deg(b.facing) + b.turret;
        let (cy, sy) = cos_sin_deg(yaw);
        let [ahead, up] = SCOPE_EYE;
        c.eye = [b.pos[0] + ahead * cy, b.pos[1] + up, b.pos[2] - ahead * sy];
        c.at = [c.eye[0] + cy, c.eye[1], c.eye[2] - sy];
        c.scope.on = true;
        (c.scope.weapon, c.scope.zoom) = (SCOPE_ITEM, SCOPE_ZOOM);
        cam.0 = None;
        return;
    }
    c.scope.on = false;
    behind(follow, cam, b, &CAM, world);
}

/// A follow camera behind a heading ([`camera`]'s, the raptor's): back, up and the look-at's
/// height over the feet at rest and at full speed (`top`), in the speed's share (−1..1), which
/// follows the speed by `share` a frame; the heading follows the facing by `follow` (the gap's
/// share a frame standing, more at full speed, at least degrees).
pub struct Behind {
    pub rest: [f32; 3],
    pub full: [f32; 3],
    pub top: f32,
    pub share: f32,
    pub follow: [f32; 3],
}

/// One frame of a [`Behind`] camera into `follow`'s eye and look-at, pulled in front of a wall.
pub fn behind(
    follow: &mut Follow,
    cam: &mut Cam,
    b: &PlayerState,
    k: &Behind,
    world: &CollisionWorld,
) {
    let c = &mut follow.cam;
    c.look_offset = 0.0;
    let hull = deg(b.facing);
    let share = (b.xz_speed / k.top).clamp(-1.0, 1.0);
    let (yaw, s) = match cam.0 {
        Some((yaw, s)) => {
            let gap = wrap180(hull - yaw);
            let gain = k.follow[0] + k.follow[1] * share.abs();
            let step = (gap.abs() * gain).max(k.follow[2]).min(gap.abs());
            (yaw + step.copysign(gap), s + (share - s) * k.share)
        }
        None => (hull, share),
    };
    cam.0 = Some((yaw.rem_euclid(360.0), s));
    let [back, up, look] = std::array::from_fn(|i| k.rest[i] + (k.full[i] - k.rest[i]) * s);
    let (cy, sy) = cos_sin_deg(yaw);
    let at = Vec3::new(b.pos[0], b.pos[1] + look, b.pos[2]);
    let mut eye = Vec3::new(b.pos[0] - back * cy, b.pos[1] + up, b.pos[2] + back * sy);
    let (dir, len) = (eye - at).normalize_and_length();
    if let Some(hit) = world.ray(at.to_array(), dir.to_array(), len) {
        eye = at + dir * (hit.dist - CAM_WALL).max(0.0);
    }
    (c.eye, c.at) = (eye.to_array(), at.to_array());
}

/// A tank's parts, once its model is in: its turret and gun joints, its brake lights' part with its
/// two materials (off, lit), and its treads' part with its own material and scroll (S units).
#[derive(Component)]
struct Parts {
    turret: Entity,
    gun: Entity,
    brake: Option<(Entity, [Handle<StandardMaterial>; 2])>,
    treads: Option<(Entity, Handle<StandardMaterial>)>,
    scroll: f32,
}

/// The shells' models and the blasts' state, by this client's ticks.
#[derive(Resource, Default)]
struct Shells {
    scene: Option<Handle<WorldAsset>>,
    models: Vec<Entity>,
    last: Vec<[f32; 3]>,
    seed: u32,
}

/// The props' nodes: placement, entity, and its pose at home (translation, rotation, scale).
#[derive(Default)]
struct Nodes(Vec<(u32, Entity, Option<Transform>)>);

pub fn build(app: &mut App, characters: &[Character], map: &str) {
    let folders = crate::characters(map)
        .into_iter()
        .map(|n| format!("characters/{n}"));
    let looks = (characters.iter().zip(folders)).map(|(c, f)| Some((c.tank.clone()?, f)));
    app.insert_resource(Looks(looks.collect()));
    app.init_resource::<Shells>();
    app.add_systems(Update, (find_parts, lights, shells, props));
    app.add_systems(
        PostUpdate,
        pose.after(AnimationSystems)
            .before(TransformSystems::Propagate),
    );
}

/// Each body's tank parts and its character's folder (model 119's; None for the others), by body
/// ([`crate::characters`]).
#[derive(Resource)]
struct Looks(Vec<Option<(Tank, String)>>);

/// A tank model whose rig is in gets its [`Parts`]: the pack's joints by name, its own copies of the
/// brake lights' and treads' materials.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn find_parts(
    tanks: Query<(Entity, &Model), (With<Rig>, Without<Parts>)>,
    looks: Res<Looks>,
    children: Query<&Children>,
    names: Query<&Name>,
    parts: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, model) in &tanks {
        let Some(Some((tank, folder))) = looks.0.get(model.body) else {
            continue;
        };
        let joint = |j: usize| {
            let name = format!("joint_{j:02}");
            (children.iter_descendants(model.scene))
                .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
        };
        let part = |m: usize| {
            let name = format!("material_{m:02}");
            (children.iter_descendants(model.scene)).find_map(|e| {
                parts
                    .get(e)
                    .ok()
                    .filter(|p| p.0.0 == name)
                    .map(|p| (e, p.1.0.clone()))
            })
        };
        let (Some(turret), Some(gun)) = (joint(tank.turret), joint(tank.gun)) else {
            continue;
        };
        let brake = part(tank.brake).and_then(|(e, m)| {
            let off = materials.get(&m)?.clone();
            let lit = StandardMaterial {
                base_color_texture: Some(assets.load(format!("{folder}/{}", tank.brake_lit))),
                ..off.clone()
            };
            Some((e, [materials.add(off), materials.add(lit)]))
        });
        let treads = part(tank.treads).and_then(|(e, m)| {
            let own = materials.get(&m)?.clone();
            Some((e, materials.add(own)))
        });
        let own = (brake.iter().map(|b| (b.0, &b.1[0]))).chain(treads.iter().map(|t| (t.0, &t.1)));
        for (e, m) in own {
            commands.entity(e).insert(MeshMaterial3d(m.clone()));
        }
        commands.entity(entity).insert(Parts {
            turret,
            gun,
            brake,
            treads,
            scroll: 0.0,
        });
    }
}

/// The turret and gun over the clip's pose, and the model hidden while dead.
fn pose(
    tanks: Query<(&Player, &Combat, &Parts, &Model)>,
    mut joints: Query<&mut Transform>,
    mut shown: Query<&mut Visibility>,
) {
    for (p, c, parts, model) in &tanks {
        let (turret, gun) = turns(p.turret, p.pitch);
        if let Ok(mut t) = joints.get_mut(parts.turret) {
            t.rotation *= turret;
        }
        if let Ok(mut t) = joints.get_mut(parts.gun) {
            t.rotation *= gun;
        }
        if let Ok(mut v) = shown.get_mut(model.scene) {
            v.set_if_neq(match c.health == 0 && c.respawn_in > 0 {
                true => Visibility::Hidden,
                false => Visibility::Inherited,
            });
        }
    }
}

/// The brake lights and the treads' scroll (by the frames this render frame spans).
fn lights(
    time: Res<Time>,
    images: Res<Assets<Image>>,
    mut tanks: Query<(&Player, &mut Parts)>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let frames = time.delta_secs() * conker_sim::TICK_HZ as f32;
    for (p, mut tank) in &mut tanks {
        if let Some((e, [off, lit])) = &tank.brake
            && let Ok(mut m) = parts.get_mut(*e)
        {
            let want = if braking(p.xz_speed, p.target_speed) {
                lit
            } else {
                off
            };
            if m.0 != *want {
                m.0 = want.clone();
            }
        }
        tank.scroll = scroll(tank.scroll, p.xz_speed, frames);
        let Some((_, own)) = &tank.treads else {
            continue;
        };
        let Some(mut m) = materials.get_mut(own) else {
            continue;
        };
        let width = (m.base_color_texture.as_ref())
            .and_then(|t| images.get(t))
            .map_or(16.0, |i| i.width() as f32);
        // ponytail: both treads by the speed; the original's sides differ by the turn (±Δfacing/70).
        m.uv_transform =
            Affine2::from_translation(Vec2::new(tank.scroll / S_PER_TEXEL / width, 0.0));
    }
}

/// The shell's pose: its nose (+z) along its flight.
pub fn shell_pose(yaw: u16, tan: f32) -> Quat {
    let (c, s) = cos_sin(yaw);
    let dir = Vec3::new(c, tan, -s).normalize_or(Vec3::Z);
    Transform::IDENTITY.looking_to(-dir, Vec3::Y).rotation
}

/// The match's shells, one model each, and a blast where one went since last tick.
#[allow(clippy::too_many_arguments)]
fn shells(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    assets: Res<AssetServer>,
    sprites: Option<Res<crate::beach::Sprites>>,
    mut state: ResMut<Shells>,
    mut models: Query<(&mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last_tick: Local<u32>,
    mut commands: Commands,
) {
    let Some(m) = game.iter().find(|m| m.tank) else {
        return;
    };
    let tick = timeline.tick().0;
    let s = &mut *state;
    let scene = (s.scene)
        .get_or_insert_with(|| {
            assets.load(format!(
                "{}#Scene{SHELL_SCENE}",
                projectiles_gltf(PACK_WEAPONS)
            ))
        })
        .clone();
    let shells = &m.tanks.shells;
    while s.models.len() < shells.len() {
        let e = commands.spawn((
            WorldAssetRoot(scene.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        s.models.push(e.id());
    }
    for (i, &e) in s.models.iter().enumerate() {
        let Ok((mut t, mut v)) = models.get_mut(e) else {
            continue;
        };
        match shells.get(i) {
            Some(&(pos, yaw, tan)) => {
                *t = Transform::from_translation(Vec3::from_array(pos))
                    .with_rotation(shell_pose(yaw, tan))
                    .with_scale(Vec3::splat(SHELL_SCALE));
                v.set_if_neq(Visibility::Inherited);
            }
            None => {
                v.set_if_neq(Visibility::Hidden);
            }
        }
    }
    if tick == *last_tick {
        return;
    }
    let frames = tick.saturating_sub(*last_tick).clamp(1, 4) as f32;
    *last_tick = tick;
    let now: Vec<_> = shells.iter().map(|s| s.0).collect();
    let reach = 2.0 * SHELL_SPEED * frames;
    let blasts: Vec<_> = crate::audio::gone(&s.last, &now, reach).collect();
    s.last = now;
    let Some(sprites) = sprites else {
        return;
    };
    for at in blasts {
        let at = Vec3::from_array(at);
        crate::beach::burst(
            &mut commands,
            &mut materials,
            &sprites,
            &mut s.seed,
            at,
            BLAST,
        );
    }
}

/// Tank's props: the doors slide apart along their own z, the bomb stands on its post (or over its
/// carrier), each wreck where its tank died, facing its way; the others stay hidden.
#[allow(clippy::type_complexity)]
fn props(
    game: Query<&MatchView>,
    added: Query<(Entity, &Name), Added<Name>>,
    players: Query<(&Transform, &Combat), (With<Model>, Without<Name>)>,
    mut nodes: Local<Nodes>,
    mut shown: Query<(&mut Transform, &mut Visibility), With<Name>>,
) {
    let found =
        (added.iter()).filter_map(|(e, n)| Some((n.rsplit_once('@')?.1.parse().ok()?, e, None)));
    nodes
        .0
        .extend(found.filter(|n: &(u32, Entity, Option<Transform>)| owns(n.0)));
    let Some(m) = game.iter().find(|m| m.tank) else {
        return;
    };
    let tanks = &m.tanks;
    for (placement, entity, home) in &mut nodes.0 {
        let Ok((mut t, mut v)) = shown.get_mut(*entity) else {
            continue;
        };
        let home = *home.get_or_insert(*t);
        let pose = match *placement {
            n @ 0..8 => {
                let lift = f32::from(m.lifts[n as usize]);
                Some(home.with_translation(home.translation + home.rotation * Vec3::Z * lift))
            }
            p if p == u32::from(BOMB) => match tanks.carrier {
                Some(slot) => (players.iter().find(|(_, c)| c.slot == slot)).map(|(at, _)| {
                    Transform::from_translation(at.translation + Vec3::Y * CARRIED.0)
                        .with_rotation(at.rotation)
                        .with_scale(home.scale * CARRIED.1)
                }),
                None => tanks
                    .bomb
                    .then(|| home.with_translation(Vec3::from_array(POST))),
            },
            p @ 18..26 => (tanks.wrecks.iter().find(|w| u32::from(w.0) + 18 == p)).map(|w| {
                Transform::from_translation(Vec3::from_array(w.1))
                    .with_rotation(Quat::from_rotation_y(model_yaw(w.2)))
                    .with_scale(home.scale)
            }),
            _ => None,
        };
        match pose {
            Some(pose) => {
                if *t != pose {
                    *t = pose;
                }
                v.set_if_neq(Visibility::Inherited);
            }
            None => {
                v.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gun over the turret: pitched up, the muzzle (joint 4, 110.4 along joint 3's z, 10.1 up)
    /// rises; turned left (positive), it swings to +x of the model's +z, where the rules fire
    /// (facing + turret: a positive turn is counter-clockwise from above, `model_yaw`'s sense).
    #[test]
    fn the_turret_and_gun_turn_where_the_rules_fire() {
        let muzzle = Vec3::new(0.0, 10.13, 110.4);
        let (_, up) = turns(0.0, 30.0);
        let lifted = up * muzzle;
        assert!(lifted.y > muzzle.y + 50.0, "{lifted}");
        let (left, _) = turns(90.0, 0.0);
        let turned = left * Vec3::Z;
        assert!((turned - Vec3::X).length() < 1e-5, "{turned}");
        // The rules' shell at facing + 90°: the model's yaw for the facing, then the turret's.
        let facing = 0x4000u16;
        let world = Quat::from_rotation_y(model_yaw(0)) * left * Vec3::Z;
        let (c, s) = cos_sin(facing);
        assert!((world - Vec3::new(c, 0.0, -s)).length() < 1e-4, "{world}");
    }

    /// The treads scroll −7 a frame per unit of speed, wrapping at 0x800; the brake lights light
    /// slowing past 5, either way; the flags by slot, yellow from 3 up; Tank's own placements.
    #[test]
    fn treads_brakes_flags_and_props() {
        assert_eq!(scroll(0.0, 48.0, 1.0), 2048.0 - 336.0);
        assert_eq!(scroll(100.0, -10.0, 2.0), 240.0);
        assert!(braking(30.0, 0.0) && braking(-30.0, 0.0));
        assert!(!braking(47.96, 48.0) && !braking(4.0, 0.0) && !braking(-30.0, -48.0));
        assert_eq!([0, 1, 2, 3, 4, 7].map(flag), [0, 1, 2, 3, 3, 3]);
        assert!([0, 7, 9, 12, 13, 15, 17, 18, 25, 33].into_iter().all(owns));
        assert!(![8, 10, 11, 16, 34].into_iter().any(owns));
    }

    /// The camera (V t1): 310 behind and 159 over the tank at rest, looking at 88.5 over it; 400,
    /// 135 and 52 at full speed once the share caught up; the scope's eye on the turret (V pw2).
    #[test]
    fn the_camera_follows_the_hull_and_the_scope_sits_on_the_turret() {
        let world = CollisionWorld::layered(std::iter::empty());
        let place = conker_rules::spawn::Placement {
            index: 0,
            pos: [1000.0, 0.0, 0.0],
            facing: 0x8000,
            camera: [0.0; 3],
        };
        let mut f = Fighter::new(&place, 0, 0);
        let mut follow = Follow::spawn([0.0; 3], &f.body);
        let mut cam = Cam::default();
        let idle = Input::default();
        camera(&mut follow, &mut cam, &f, &idle, &world);
        let near = |a: [f32; 3], b: [f32; 3]| (Vec3::from(a) - Vec3::from(b)).length() < 0.05;
        assert!(
            near(follow.cam.eye, [1310.0, 159.0, 0.0]),
            "{:?}",
            follow.cam.eye
        );
        assert!(near(follow.cam.at, [1000.0, 88.5, 0.0]));
        f.body.xz_speed = 48.0;
        for _ in 0..200 {
            camera(&mut follow, &mut cam, &f, &idle, &world);
        }
        assert!(
            near(follow.cam.eye, [1400.0, 135.0, 0.0]),
            "{:?}",
            follow.cam.eye
        );
        // Turned 90°: the heading closes on the hull's.
        f.body.facing = 0xC000;
        camera(&mut follow, &mut cam, &f, &idle, &world);
        let yaw = cam.0.unwrap().0;
        assert!(yaw < 270.0 && yaw > 180.0 + 90.0 * 0.06, "{yaw}");
        // The scope: on the turret, along it, zoomed; R let go cuts back behind the hull.
        f.body.facing = 0x8000;
        f.body.turret = 90.0;
        f.power = SCOPE;
        let r = Input {
            buttons: conker_rules::game::BUTTON_R,
            ..idle
        };
        camera(&mut follow, &mut cam, &f, &r, &world);
        assert!(
            near(follow.cam.eye, [1000.0, 111.6, 27.5]),
            "{:?}",
            follow.cam.eye
        );
        assert!(near(follow.cam.at, [1000.0, 111.6, 28.5]));
        assert!(follow.cam.scope.on && follow.cam.scope.zoom == SCOPE_ZOOM);
        f.body.xz_speed = 0.0;
        camera(&mut follow, &mut cam, &f, &idle, &world);
        assert!(!follow.cam.scope.on);
        assert!(
            near(follow.cam.eye, [1310.0, 159.0, 0.0]),
            "{:?}",
            follow.cam.eye
        );
    }

    /// The shell's nose along its flight: level along the yaw, up by its tangent.
    #[test]
    fn the_shell_points_along_its_flight() {
        let nose = shell_pose(0, 0.0) * Vec3::Z;
        assert!((nose - Vec3::X).length() < 1e-5, "{nose}");
        let nose = shell_pose(0x4000, 1.0) * Vec3::Z;
        let want = Vec3::new(0.0, 1.0, -1.0).normalize();
        assert!((nose - want).length() < 1e-4, "{nose}");
    }
}
