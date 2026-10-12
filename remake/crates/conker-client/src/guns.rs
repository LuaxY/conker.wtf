//! The mounted machine guns of maps 0x30 and 0x34 (`../lab/research/mp-original/mounted-gun.md`
//! §3, §4.4): model 138 at half scale where [`MatchView::guns`] has each, turned to its yaw, its
//! barrels (`joint_04`) pitched down by its pitch. While a gun fires (state 4) its fire clip loops
//! and, on 0x30, its belt of rounds scrolls; at rest it stands in its model's rest pose. No flash
//! on the model: the original draws only the gunner's HUD flash. The local gunner's gun turns with
//! its own predicted scope (the server's lags a round trip) and hides while its sight is up: the
//! HUD's handles stand in for it ([`crate::hud`]).

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::{Gltf, GltfMaterialName};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::character_gltf;
use conker_rules::gun;
use conker_rules::net::Fighting;
use conker_sim::net::{MatchView, Player};

use crate::render::model_yaw;
use crate::{Config, FollowCamera};

/// Bank 01's mounted gun (the pack's `characters/138`).
const MODEL: &str = "138";
/// Drawn at half its model units, as the players (STATUS, the pack's handoff).
const SCALE: f32 = 0.5;
/// The barrels' pivot, which the pitch turns.
const PIVOT: &str = "joint_04";
/// The belt of rounds' material (`textures/character-15.png`).
const BELT: &str = "material_15";
/// On 0x30, while the fire clip plays, the belt's 12 vertices' T moves −40 (1/32 texel) a VI
/// (`func_15061B4C` → `func_15062FC0(…, 12, 0, VIs × −40)`, C): 2.5 of its 16 texels a frame. Its
/// sampler repeats along T.
const BELT_STEP: f32 = -2.5 / 16.0;

/// Gun `k` of [`MatchView::guns`]: its glTF, its rig once its scene is in, and what it showed last.
#[derive(Component)]
pub struct Gun {
    k: usize,
    gltf: Handle<Gltf>,
    rig: Option<Rig>,
    pitch: f32,
    firing: bool,
    belt: f32,
}

/// A gun's posed parts: its `AnimationPlayer` and clip, the pivot, every joint's rest pose, and its
/// own copy of the belt's material with that material's own uv transform.
struct Rig {
    animator: Entity,
    clip: AnimationNodeIndex,
    pivot: Entity,
    rest: Vec<(Entity, Transform)>,
    belt: Option<(Handle<StandardMaterial>, Affine2)>,
}

/// The fire clip's graph and node, made once (its length set once).
#[derive(Resource, Default)]
struct Graph(Option<(Handle<AnimationGraph>, AnimationNodeIndex)>);

pub fn build(app: &mut App) {
    app.init_resource::<Graph>();
    app.add_systems(Update, show);
    app.add_systems(
        PostUpdate,
        pose.after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    );
}

/// The gun's turn: its model's rest pose faces +z.
pub fn turn(yaw: u16) -> Quat {
    Quat::from_rotation_y(model_yaw(yaw))
}

/// The barrels' pitch on `joint_04`, `pitch` degrees down (the joint's frame is the model's).
pub fn tilt(pitch: f32) -> Quat {
    Quat::from_rotation_x(pitch.to_radians())
}

/// The match's guns, once the players' characters are in (one glTF load at a time, see
/// `render::load_characters`).
pub(crate) fn add(
    game: Query<&MatchView>,
    assets: Res<AssetServer>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    let Some(m) = game.single().ok().filter(|m| !m.guns.is_empty()) else {
        return;
    };
    if *added {
        return;
    }
    *added = true;
    let gltf = character_gltf(MODEL);
    for k in 0..m.guns.len() {
        let model = Gun {
            k,
            gltf: assets.load(&gltf),
            rig: None,
            pitch: 0.0,
            firing: false,
            belt: 0.0,
        };
        let scene = WorldAssetRoot(assets.load(format!("{gltf}#Scene0")));
        let at = Transform::from_scale(Vec3::splat(SCALE));
        (commands.spawn((model, scene, at, Visibility::Hidden))).observe(ready);
    }
}

/// A gun's scene is in: its `AnimationPlayer` gets the fire clip (looping on the game's 12 ticks:
/// the glTF's last key is a tick short), its joints' rest poses are kept, its belt gets a material
/// of its own to scroll.
#[allow(clippy::too_many_arguments)]
fn ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    joints: Query<(&Name, &Transform)>,
    belts: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    mut guns: Query<&mut Gun>,
    gltfs: Res<Assets<Gltf>>,
    mut made: ResMut<Graph>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Ok(mut gun) = guns.get_mut(ready.entity) else {
        return;
    };
    let parts: Vec<Entity> = children.iter_descendants(ready.entity).collect();
    let animator = parts.iter().copied().find(|&e| animators.contains(e));
    let rest: Vec<_> = (parts.iter())
        .filter_map(|&e| joints.get(e).ok().map(|(n, t)| (e, n, *t)))
        .filter(|(_, n, _)| n.starts_with("joint_"))
        .collect();
    let pivot = rest.iter().find(|j| j.1.as_str() == PIVOT).map(|j| j.0);
    let clip = gltfs
        .get(&gun.gltf)
        .and_then(|g| g.animations.first().cloned());
    let (Some(animator), Some(pivot), Some(clip)) = (animator, pivot, clip) else {
        warn!("gun {}: no animation player, {PIVOT} or clip", gun.k);
        return;
    };
    let (graph, node) = made.0.get_or_insert_with(|| {
        if let Some(mut c) = clips.get_mut(&clip) {
            let d = c.duration();
            c.set_duration(d + 1.0 / conker_sim::TICK_HZ as f32);
        }
        let (graph, nodes) = AnimationGraph::from_clips([clip]);
        (graphs.add(graph), nodes[0])
    });
    let mut belt = None;
    for &e in &parts {
        if let Ok((name, material)) = belts.get(e)
            && name.0 == BELT
        {
            let own = belt.get_or_insert_with(|| {
                let m = materials.get(&material.0).cloned().unwrap_or_default();
                let base = m.uv_transform;
                (materials.add(m), base)
            });
            commands.entity(e).insert(MeshMaterial3d(own.0.clone()));
        }
    }
    commands
        .entity(animator)
        .insert(AnimationGraphHandle(graph.clone()));
    gun.rig = Some(Rig {
        animator,
        clip: *node,
        pivot,
        rest: rest.into_iter().map(|(e, _, t)| (e, t)).collect(),
        belt,
    });
}

/// Each gun where and as the match has it (the local gunner's from its predicted scope), its fire
/// clip and belt while it fires.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show(
    game: Query<&MatchView>,
    local: Query<(&Player, &Fighting, &FollowCamera)>,
    free: Query<&FreeCameraState>,
    cfg: Res<Config>,
    time: Res<Time>,
    mut guns: Query<(&mut Gun, &mut Transform, &mut Visibility)>,
    mut animators: Query<&mut AnimationPlayer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(m) = game.single() else {
        return;
    };
    let free = free.single().is_ok_and(|f| f.enabled);
    for (mut gun, mut transform, mut visibility) in &mut guns {
        let Some(v) = m.guns.get(gun.k) else {
            continue;
        };
        let (mut yaw, mut pitch, mut state, mut hidden) = (v.yaw, v.pitch, v.state, false);
        if let Ok((p, f, camera)) = local.single()
            && let Some(mount) = f.state.gun.filter(|g| usize::from(g.gun) == gun.k)
            && f.state.seated()
        {
            (yaw, pitch, state) = (p.facing, f.state.scope_pitch, mount.state);
            hidden = camera.follow.cam.scope.on && !free;
        }
        transform.translation = Vec3::from_array(v.pos);
        transform.rotation = turn(yaw);
        visibility.set_if_neq(match hidden {
            true => Visibility::Hidden,
            false => Visibility::Inherited,
        });
        gun.pitch = pitch;
        let firing = state == gun::FIRING;
        if let Some(rig) = &gun.rig
            && firing != gun.firing
            && let Ok(mut animator) = animators.get_mut(rig.animator)
        {
            animator.stop_all();
            if firing {
                animator.play(rig.clip).repeat();
            }
            gun.firing = firing;
        }
        if firing && cfg.map == "30" {
            let belt = gun.belt + BELT_STEP * conker_sim::TICK_HZ as f32 * time.delta_secs();
            gun.belt = belt.rem_euclid(1.0);
        }
        if let Some((belt, base)) = gun.rig.as_ref().and_then(|r| r.belt.as_ref())
            && let Some(mut material) = materials.get_mut(belt)
        {
            let scrolled = Affine2::from_translation(Vec2::new(0.0, gun.belt)) * *base;
            if material.uv_transform != scrolled {
                material.uv_transform = scrolled;
            }
        }
    }
}

/// After the clips: a gun at rest stands in its rest pose (a stopped clip leaves its last pose),
/// and every gun's barrels pitch about the pivot.
fn pose(guns: Query<&Gun>, mut joints: Query<&mut Transform>) {
    for gun in &guns {
        let Some(rig) = &gun.rig else {
            continue;
        };
        if !gun.firing {
            for &(e, rest) in &rig.rest {
                if let Ok(mut t) = joints.get_mut(e) {
                    *t = rest;
                }
            }
        }
        if let Ok(mut pivot) = joints.get_mut(rig.pivot) {
            pivot.rotation = tilt(gun.pitch) * pivot.rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model's barrels where the rules shoot from: the gun turned and scaled, `joint_04` pitched,
    /// each barrel joint's muzzle (±13, 0, 200) is `gun::muzzle` of that barrel: barrel 0 is
    /// `joint_05`, at the model's +x, the gun's −side (the gunner's left); a positive pitch looks down.
    #[test]
    fn the_barrels_pose_as_the_rules_shoot() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/characters/138/character.gltf"
        );
        let Ok(json) = std::fs::read(path) else {
            eprintln!("no pack gun model: skipping");
            return;
        };
        let gltf: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let joint = |name: &str| {
            let nodes = gltf["nodes"].as_array().unwrap();
            let node = nodes.iter().find(|n| n["name"] == name).unwrap();
            let t: [f32; 3] = serde_json::from_value(node["translation"].clone()).unwrap();
            Vec3::from_array(t)
        };
        let (pivot, barrels) = (joint(PIVOT), [joint("joint_05"), joint("joint_07")]);
        let at = [170.0, 560.0, 5990.0];
        for yaw in [0, 15872, 40960, 48128] {
            for pitch in [-60.0, 0.0, 30.0, 50.0] {
                for b in 0..2u8 {
                    let x = if b == 0 { 13.0 } else { -13.0 };
                    let muzzle = barrels[usize::from(b)] + Vec3::new(x, 0.0, 200.0);
                    let local = SCALE * (pivot + tilt(pitch) * muzzle);
                    let drawn = Vec3::from_array(at) + turn(yaw) * local;
                    let rules = Vec3::from_array(gun::muzzle(at, yaw, pitch, b));
                    assert!(
                        drawn.distance(rules) < 0.05,
                        "yaw {yaw} pitch {pitch} barrel {b}: {drawn} vs {rules}"
                    );
                }
            }
        }
        assert!((tilt(30.0) * Vec3::Z).y < 0.0);
    }
}
