//! The mounted machine guns of maps 0x30 and 0x34 (`../lab/research/mp-original/mounted-gun.md`
//! §3, §4.4): model 138 at half scale where [`MatchView::guns`] has each, turned to its yaw, its
//! barrels (`joint_04`) pitched down by its pitch (the whole gun by `gun::tilt` of it). While a gun fires (state 4) its fire clip loops
//! and its belt of rounds scrolls; at rest it stands in its model's rest pose. No flash
//! on the model: the original draws only the gunner's HUD flash. The local gunner's gun turns with
//! its own predicted scope (the server's lags a round trip) and hides while its sight is up: the
//! HUD's handles stand in for it ([`crate::hud`]).

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::{Gltf, GltfMaterialName};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_anim::{AnimPlayer, Request};
use conker_assets::character_gltf;
use conker_assets::pose::Skeleton;
use conker_rules::gun;
use conker_rules::net::Fighting;
use conker_sim::net::{MatchView, Player};

use crate::render::model_yaw;
use crate::{Config, FollowCamera};

/// Bank 01's mounted gun (the pack's `characters/138`).
const MODEL: &str = "138";
/// Drawn at half its model units, as the players (STATUS, the pack's handoff).
const SCALE: f32 = 0.5;
/// The model's root, which the gun's tilt turns.
const ROOT: &str = "joint_00";
/// The barrels' pivot, which the rest of the pitch turns.
const PIVOT: &str = "joint_04";
/// The belt of rounds' material (`textures/character-15.png`, 64 × 16, repeating along T).
const BELT: &str = "material_15";

/// How far the belt's texture moves a frame while the fire clip plays (`func_15061B4C`, C), as a
/// share of its 16 texels along v. Its texel row t falls, so v rises (v = 1 − t / 16,
/// `extract_gbi::Texture::uv`; the uv transform adds to the mesh's v): the rounds run toward the
/// belt's top, the receiver's end, whose T is the largest. On 0x30 `func_15062FC0(…, 12, 0, VIs ×
/// −40)` moves the belt's 12 vertices' T by −40/32 texel a VI, 2 VIs a frame. Elsewhere (0x34)
/// `func_15062D10` moves the belt's 3 mip tiles' (64×16, 32×8, 16×4) ult by 16, 8 and 4 quarter
/// texels a frame, wrapping at their heights (V gcA DLs 130-132, 190: level 0's 0x002, 0x012,
/// 0x022, 0x032): level 0's 4 texels.
fn belt_step(map: &str) -> f32 {
    let texels = if map == "30" {
        40.0 * 2.0 / 32.0
    } else {
        16.0 / 4.0
    };
    texels / 16.0
}

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

/// A gun's posed parts: its `AnimationPlayer` and clip, the root and the pivot, every joint's rest
/// pose, and its own copy of the belt's material with that material's own uv transform.
struct Rig {
    animator: Entity,
    clip: AnimationNodeIndex,
    root: Entity,
    pivot: Entity,
    rest: Vec<(Entity, Transform)>,
    belt: Option<(Handle<StandardMaterial>, Affine2)>,
}

/// The model's files, which [`load`] reads on the maps with guns, and its fire clip (logical clip
/// 1, a 12-frame loop; clip 0, the rest pose, has no keys).
const FILES: [&str; 2] = [
    "characters/138/character.gltf",
    "characters/138/character.bin",
];
const FIRE_CLIP: &str = "pair_0000_anim_0001";
const FIRE_FRAMES: u16 = 12;

/// The fire clip's pose each frame as the scope's eye takes it
/// ([`conker_camera::PlayerView::gun_kick`]): `joint_00`'s and `joint_04`'s turns about x (degrees
/// nose down) and `joint_04`'s slide ahead (model units) from the rest pose. Empty without the
/// model: no recoil.
#[derive(Resource, Clone, Debug, Default)]
pub struct FireClip(pub Vec<[f32; 3]>);

impl FireClip {
    pub fn parse(gltf: &[u8], bin: &[u8]) -> conker_assets::Result<Self> {
        let sk = Skeleton::parse(gltf, bin)?;
        let pitch = |q: [f32; 4]| 2.0 * q[0].atan2(q[3]).to_degrees();
        let frame = |f: u16| {
            let p = sk.sample(FIRE_CLIP, f32::from(f));
            [pitch(p[0].r), pitch(p[4].r), p[4].t[2] - sk.rest[4].t[2]]
        };
        Ok(Self((0..FIRE_FRAMES).map(frame).collect()))
    }
}

/// The pack files [`load`] reads on `map`: the gun model's where the map has guns.
pub fn files(map: &str) -> Vec<String> {
    let scene = conker_sim::net::map_scene(map).map_or(0, usize::from);
    match gun::places(scene).is_empty() {
        true => Vec::new(),
        false => FILES.map(String::from).into(),
    }
}

/// The fire clip from the pack ([`files`]), on the maps with guns.
pub fn load(map: &str, mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Option<FireClip> {
    let [gltf, bin] = <[String; 2]>::try_from(files(map)).ok()?;
    FireClip::parse(&read(&gltf)?, &read(&bin)?).ok()
}

/// The local gunner's gun as drawn, for its scope's eye: every frame the gun asks for its fire clip
/// while it fires (state 4), else its rest (clip 0), at rate 1 with a 2-frame cross-fade
/// (`func_1505841C` → `func_1505E650(gun, id, 1.0, 2.0, …)`, V a1), and the eye takes the pose
/// drawn the frame before (V k2: the gun fires from read 101; the eye is at rest at 101 and 102, at
/// the clip's frame 3 at 105, frame 0 at 108).
#[derive(Clone, Copy, Debug, Default)]
pub struct Recoil {
    clock: AnimPlayer,
    drawn: [f32; 3],
}

impl Recoil {
    /// One tick with the gun `firing` or not: the kick drawn the tick before.
    pub fn step(&mut self, firing: bool, clip: &FireClip) -> [f32; 3] {
        let eye = self.drawn;
        let c = &mut self.clock;
        c.advance();
        let id = u16::from(firing);
        let req = Request {
            id,
            rate: 1.0,
            blend: 2.0,
            start: 0.0,
        };
        c.play(req, FIRE_FRAMES);
        // I: the game's pose mixer is unread (conker-anim's linear weight): V k2's eye is up to 0.85
        // off on the fades' reads.
        let pose = |id: u16, t: f32| match id {
            1 => clip.0.get(t as usize).copied().unwrap_or_default(),
            _ => [0.0; 3],
        };
        let w = c.prev_weight();
        let (from, to) = (pose(c.prev, c.prev_time), pose(c.id, c.time));
        self.drawn = core::array::from_fn(|i| from[i] * w + to[i] * (1.0 - w));
        eye
    }
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
    let joint = |name: &str| rest.iter().find(|j| j.1.as_str() == name).map(|j| j.0);
    let (root, pivot) = (joint(ROOT), joint(PIVOT));
    let clip = gltfs
        .get(&gun.gltf)
        .and_then(|g| g.animations.first().cloned());
    let (Some(animator), Some(root), Some(pivot), Some(clip)) = (animator, root, pivot, clip)
    else {
        warn!(
            "gun {}: no animation player, {ROOT}, {PIVOT} or clip",
            gun.k
        );
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
        root,
        pivot,
        rest: rest.into_iter().map(|(e, _, t)| (e, t)).collect(),
        belt,
    });
}

/// Each gun where and as the match has it (the local gunner's from its predicted scope), its fire
/// clip and belt ([`belt_step`]) while it fires.
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
        if firing {
            let step = belt_step(&cfg.map) * conker_sim::TICK_HZ as f32;
            let belt = gun.belt + step * time.delta_secs();
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
/// and every gun tilts about its root by `gun::tilt` and its barrels pitch the rest about the pivot.
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
        let t = gun::tilt(gun::pitch10(gun.pitch));
        if let Ok(mut root) = joints.get_mut(rig.root) {
            root.rotation = tilt(t) * root.rotation;
        }
        if let Ok(mut pivot) = joints.get_mut(rig.pivot) {
            pivot.rotation = tilt(gun.pitch - t) * pivot.rotation;
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
                    let t = gun::tilt(gun::pitch10(pitch));
                    let local = SCALE * (tilt(t) * (pivot + tilt(pitch - t) * muzzle));
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

    /// V k2 (e1 under lldb; 0x34's gun at yaw 0xA000, pitched 12°, firing reads 101-179): the
    /// scope's eye per read, the pack's fire clip through [`Recoil`] and
    /// `conker_camera::gun_eye_kicked`. At rest and while the clip loops (every read 105-180, a
    /// 6-frame cycle) within 0.1 (the rest pose's own 0.07); the fades in (103, 104: 0.46, 0.57)
    /// and out (181-183: 0.85, 0.48, 0.1) within 0.9 (I, the pose mixer).
    #[test]
    fn the_fire_clip_kicks_the_scopes_eye_as_k2() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/");
        let Some(clip) = load("34", |p| std::fs::read(format!("{dir}{p}")).ok()) else {
            eprintln!("no pack gun model: skipping");
            return;
        };
        let v = [
            (100, [2330.659, 739.789, 634.340]),
            (101, [2330.659, 739.789, 634.340]),
            (102, [2330.662, 739.796, 634.338]),
            (103, [2332.242, 742.598, 632.758]),
            (104, [2332.707, 741.434, 632.293]),
            (105, [2332.307, 741.148, 632.693]),
            (106, [2332.542, 742.781, 632.458]),
            (107, [2333.676, 746.025, 631.324]),
            (108, [2334.595, 748.123, 630.405]),
            (109, [2333.614, 744.838, 631.386]),
            (110, [2332.541, 741.059, 632.459]),
            (180, [2334.595, 748.123, 630.405]),
            (181, [2333.932, 745.466, 631.068]),
            (182, [2332.322, 742.718, 632.678]),
            (183, [2330.738, 739.920, 634.262]),
            (184, [2330.659, 739.789, 634.340]),
        ];
        let at = |t: u32| v.iter().find(|r| r.0 == t).map(|r| r.1);
        let mut recoil = Recoil::default();
        for t in 90..=190 {
            let kick = recoil.step((101..=179).contains(&t), &clip);
            let eye = conker_camera::gun_eye_kicked([2428.0, 645.0, 537.0], 0xA000, 12.0, kick);
            let (want, near) = match t {
                105..=180 => (at(105 + (t - 105) % 6), 0.1),
                103 | 104 | 181..=183 => (at(t), 0.9),
                _ => (at(t).or(at(100)), 0.1),
            };
            let want = Vec3::from_array(want.unwrap());
            let off = Vec3::from_array(eye).distance(want);
            assert!(off < near, "read {t}: {eye:?} vs {want}");
        }
    }

    /// The belt moves 2.5 of its 16 texels a frame on 0x30 and 4 elsewhere (C), its v rising:
    /// toward the belt's top end (the receiver's, the most T), whose v is the least in the glTF.
    #[test]
    fn the_belt_feeds_the_gun() {
        assert_eq!([belt_step("30"), belt_step("34")], [2.5 / 16.0, 0.25]);
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/characters/138"
        );
        let (Ok(json), Ok(bin)) = (
            std::fs::read(format!("{dir}/character.gltf")),
            std::fs::read(format!("{dir}/character.bin")),
        ) else {
            eprintln!("no pack gun model: skipping");
            return;
        };
        let gltf: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let floats = |i: &serde_json::Value, n: usize| -> Vec<f32> {
            let a = &gltf["accessors"][i.as_u64().unwrap() as usize];
            let v = &gltf["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
            let at = (v["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0))
                as usize;
            let count = a["count"].as_u64().unwrap() as usize * n;
            (bin[at..at + 4 * count].chunks(4))
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect()
        };
        let prims = gltf["meshes"][0]["primitives"].as_array().unwrap();
        let belt = prims.iter().find(|p| p["material"] == 15).unwrap();
        let (pos, uv) = (
            floats(&belt["attributes"]["POSITION"], 3),
            floats(&belt["attributes"]["TEXCOORD_0"], 2),
        );
        let top = (0..pos.len() / 3)
            .max_by(|&a, &b| pos[3 * a + 1].total_cmp(&pos[3 * b + 1]))
            .unwrap();
        let least = uv.chunks(2).map(|t| t[1]).fold(f32::MAX, f32::min);
        assert!(
            uv[2 * top + 1] < least + 0.2,
            "{:?}",
            &uv[2 * top..2 * top + 2]
        );
    }
}
