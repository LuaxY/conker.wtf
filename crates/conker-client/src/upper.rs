//! The modern controls' upper body (docs/CONTROLS_RESEARCH.md §9; lane C4). While the local player
//! aims or fires a gun, its legs play their own clip (the run, walk or stand under it) and its upper
//! body (`joint_01` and everything under it: the spine, arms, guns and head) the weapon's aim pose
//! or its action (fire, reload, draw), through masked clips ([`mask_groups`], Bevy's
//! `add_clip_with_mask`; [`aim_layers`] picks them). The upper body then bends to the aim's pitch
//! ([`bend`]): the spine [`SPINE_SHARE`] of it, the shoulders the rest, so the guns point along the
//! aim ray, below the crosshair, instead of across it. The yaw is the facing's (the rules turn a
//! modern player to the aim while aiming or firing).
//!
//! Only the local player: the others' aim pitch is not on the wire. The server's hit spheres stay
//! posed from the rules' single clip (§9, accepted): the bend moves the drawn head at most
//! [`head_drift`]`(AIM_PITCH_MAX)` (about 17 units at 80°, inside the head sphere's 20).

use bevy::animation::AnimationTargetId;
use bevy::gltf::{Gltf, GltfNode};
use bevy::prelude::*;
use conker_rules::weapon::{Weapon, weapon};

/// The mask groups: the upper body (`joint_01`'s tree) and the rest (the hips, legs and tail).
pub const UPPER: u32 = 0;
pub const LOWER: u32 = 1;
/// The upper body's root joint.
pub const UPPER_JOINT: &str = "joint_01";
/// The spine's share of the aim's pitch; the shoulders turn the rest.
pub const SPINE_SHARE: f32 = 0.35;

/// A masked twin of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The legs only (masked out of [`UPPER`]).
    Legs,
    /// The upper body only (masked out of [`LOWER`]).
    Arms,
}

impl Layer {
    pub fn mask(self) -> u64 {
        match self {
            Layer::Legs => 1 << UPPER,
            Layer::Arms => 1 << LOWER,
        }
    }
}

/// The weapons that shoot (the aim's): every weapon with a muzzle.
fn guns() -> impl Iterator<Item = &'static Weapon> {
    (0..=u8::MAX)
        .filter_map(weapon)
        .filter(|w| crate::crosshair::muzzle(w).is_some())
}

/// The masked twins clip `id` gets: the legs' for the gaits (plain and armed) and the guns'
/// idles; the arms' for the guns' idle, fire, aim, reload, draw and holster.
pub fn layers(id: u16) -> Vec<Layer> {
    use conker_anim::id::*;
    let plain = [
        CREEP,
        WALK,
        RUN,
        IDLE,
        CROUCH,
        CROUCH_WALK,
        SKID,
        SCARED_RUN,
        JUMP,
        LAND,
        RUN_JUMP,
        FALL,
        HIGH_JUMP,
    ];
    let armed = (0..=u8::MAX).flat_map(|w| {
        [(1.0, 1.0), (10.0, 16.0), (30.0, 30.0)]
            .map(|(speed, target)| conker_anim::armed_move(w, speed, target))
    });
    let legs =
        plain.contains(&id) || armed.flatten().any(|a| a == id) || guns().any(|w| w.idle == id);
    let arms = guns().any(|w| {
        let reload = w.reload.map(|r| r.0);
        [w.idle, w.fire.0, w.draw.0, w.holster.0].contains(&id)
            || w.aim() == Some(id)
            || reload == Some(id)
    });
    [(legs, Layer::Legs), (arms, Layer::Arms)]
        .into_iter()
        .filter_map(|(on, l)| on.then_some(l))
        .collect()
}

/// Every joint of the glTF's skeleton (from `joint_00`, by the target names Bevy's loader gives an
/// animated glTF's nodes: the names' path from the scene's root) in its mask group.
pub fn mask_groups(graph: &mut AnimationGraph, gltf: &Gltf, nodes: &Assets<GltfNode>) {
    fn walk(
        graph: &mut AnimationGraph,
        nodes: &Assets<GltfNode>,
        node: &Handle<GltfNode>,
        path: &mut Vec<Name>,
        upper: bool,
    ) {
        let Some(n) = nodes.get(node) else {
            return;
        };
        let upper = upper || n.name == UPPER_JOINT;
        path.push(Name::new(n.name.clone()));
        let group = if upper { UPPER } else { LOWER };
        graph.add_target_to_mask_group(AnimationTargetId::from_names(path.iter()), group);
        for child in &n.children {
            walk(graph, nodes, child, path, upper);
        }
        path.pop();
    }
    if let Some(root) = gltf.named_nodes.get("joint_00") {
        walk(graph, nodes, root, &mut Vec::new(), false);
    }
}

/// The local modern player's two layers, (clip, time) each: legs, upper body; none, the whole body
/// plays its clip `cur` (neither a gait nor a gun's: a hit, a throw, a swim…).
/// - An armed action (`arms` but not `legs`: fire, reload, draw…) plays on the upper body; aiming
///   (R, the weapon drawn) the aim pose `aim`; else none.
/// - The legs play `cur` if it is a gait or stand (`legs`), else `under` (the gait the player's
///   speed asks for, or the stand).
pub fn aim_layers(
    cur: (u16, f32),
    aim: (u16, f32),
    aiming: bool,
    under: (u16, f32),
    legs: impl Fn(u16) -> bool,
    arms: impl Fn(u16) -> bool,
) -> Option<[(u16, f32); 2]> {
    let upper = match (legs(cur.0), arms(cur.0), aiming) {
        (false, true, _) => cur,
        (true, _, true) => aim,
        _ => return None,
    };
    let lower = if legs(cur.0) { cur } else { under };
    Some([lower, upper])
}

/// A weapon's aim pose: its R aim clip (the sniper rifle's, the crossbow's), else its fire clip's
/// first frame (the arms out, the guns forward).
pub fn aim_pose(w: &Weapon) -> (u16, f32) {
    (w.aim().unwrap_or(w.fire.0), 0.0)
}

/// The legs' gait under an action at the body's speeds (`speed`, `target`, conker-anim's): the
/// weapon's armed walk or run, else the plain gait's `state`; none standing.
pub fn gait(w: &Weapon, state: u8, speed: f32, target: f32) -> Option<u16> {
    use conker_sim::movement::state::{RUN, WALK};
    let id = if w.club() { 0x40 } else { w.id };
    conker_anim::armed_move(id, speed, target)
        .or_else(|| (target >= 1.0 && matches!(state, WALK | RUN)).then_some(u16::from(state)))
}

/// The bend of an aim pitched `a` radians (up positive): the spine's and the shoulders' new local
/// rotations, from the hips' (`hips`), the spine's and a shoulder's posed ones: each turned about
/// the model's right axis (its x: the model faces +z), the spine by [`SPINE_SHARE`].
pub fn bend(hips: Quat, spine: Quat, shoulder: Quat, a: f32) -> (Quat, Quat) {
    let turn = |frame: Quat, angle: f32| Quat::from_axis_angle(frame.inverse() * Vec3::X, angle);
    let spine = turn(hips, -a * SPINE_SHARE) * spine;
    let shoulder = turn(hips * spine, -a * (1.0 - SPINE_SHARE)) * shoulder;
    (spine, shoulder)
}

/// The local player's aim pitch (radians, up positive) while its upper body plays a layer
/// ([`aim_layers`], `render`'s `pose_players`); none, unbent.
#[derive(Resource, Default)]
pub struct Bend(pub Option<f32>);

/// Bends the local player's upper body to [`Bend`] after the clips posed it ([`bend`]): `joint_01`
/// in `joint_00`'s frame, its shoulders `joint_02` and `joint_07` (the 28-joint bodies' arms; other
/// skeletons only bend the spine).
pub(crate) fn bend_upper(
    pitch: Res<Bend>,
    own: Query<&crate::held::Twisted, With<crate::FollowCamera>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut joints: Query<&mut Transform>,
) {
    let (Some(a), Ok(t)) = (pitch.0, own.single()) else {
        return;
    };
    let spine = t.0;
    let Some(hips) = (parents.get(spine).ok()).and_then(|p| joints.get(p.parent()).ok()) else {
        return;
    };
    let hips = hips.rotation;
    let arms: Vec<Entity> = (children.get(spine).into_iter().flatten().copied())
        .filter(|&c| {
            names
                .get(c)
                .is_ok_and(|n| ["joint_02", "joint_07"].contains(&n.as_str()))
        })
        .collect();
    let Ok(pose) = joints.get(spine).map(|j| j.rotation) else {
        return;
    };
    for &arm in &arms {
        if let Ok(mut j) = joints.get_mut(arm) {
            j.rotation = bend(hips, pose, j.rotation, a).1;
        }
    }
    if let Ok(mut j) = joints.get_mut(spine) {
        j.rotation = bend(hips, pose, Quat::IDENTITY, a).0;
    }
}

/// How far the bend moves the drawn head from where the single clip (the server's spheres) has it,
/// at an aim pitch of `pitch` game units: the head's point (`joint_12` + 30 up, the death iris's) is
/// about 70 model units from `joint_01` (the squirrel's 39.7 and 3.6, and the 30), drawn at half.
pub fn head_drift(pitch: i16) -> f32 {
    let lever = Vec2::new(39.7 + 30.0, 3.6).length() * crate::render::MODEL_SCALE;
    let a = f32::from(pitch).abs() / 65536.0 * std::f32::consts::TAU * SPINE_SHARE;
    2.0 * lever * (a / 2.0).sin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_anim::combat::{SNIPER_AIM, TWIN_FIRE, TWIN_IDLE, TWIN_RUN, TWIN_WALK};

    #[test]
    fn the_gaits_and_the_guns_clips_get_their_twins() {
        assert_eq!(layers(conker_anim::id::RUN), vec![Layer::Legs]);
        assert_eq!(layers(TWIN_RUN), vec![Layer::Legs]);
        assert_eq!(layers(TWIN_IDLE), vec![Layer::Legs, Layer::Arms]);
        assert_eq!(layers(TWIN_FIRE), vec![Layer::Arms]);
        assert_eq!(layers(SNIPER_AIM), vec![Layer::Arms]);
        // Hits, deaths and the melee weapons' swings play whole.
        assert!(layers(conker_anim::combat::MELEE_SWING).is_empty());
        assert_eq!((Layer::Legs.mask(), Layer::Arms.mask()), (1, 2));
    }

    /// Aiming over a run: the run's legs, the aim pose's arms; firing on the move: the gait's legs
    /// under the fire clip; standing aimed: the stand's legs; a plain run unaimed, or a hit aimed, whole.
    #[test]
    fn the_layers_put_the_aim_or_the_action_over_the_legs() {
        let twin = weapon(0x09).unwrap();
        let legs = |id| layers(id).contains(&Layer::Legs);
        let arms = |id| layers(id).contains(&Layer::Arms);
        let aim = aim_pose(twin);
        assert_eq!(aim, (TWIN_FIRE, 0.0));
        let walk = gait(twin, conker_sim::movement::state::RUN, 30.0, 30.0).unwrap();
        assert_eq!(walk, TWIN_RUN);
        let under = (walk, 4.0);
        let run = (TWIN_WALK, 7.0);
        assert_eq!(
            aim_layers(run, aim, true, under, legs, arms),
            Some([run, aim])
        );
        assert_eq!(aim_layers(run, aim, false, under, legs, arms), None);
        let fire = (TWIN_FIRE, 2.0);
        assert_eq!(
            aim_layers(fire, aim, false, under, legs, arms),
            Some([under, fire])
        );
        let idle = (TWIN_IDLE, 9.0);
        assert_eq!(
            aim_layers(idle, aim, true, under, legs, arms),
            Some([idle, aim])
        );
        let hit = (0x60, 1.0);
        assert_eq!(aim_layers(hit, aim, true, under, legs, arms), None);
        // Standing: no gait; the pistol walks with the plain gait.
        assert_eq!(
            gait(twin, conker_sim::movement::state::IDLE, 0.0, 0.0),
            None
        );
        let pistol = weapon(0x39).unwrap();
        assert!(gait(pistol, conker_sim::movement::state::RUN, 30.0, 30.0).is_some());
    }

    /// The bend turns the upper body about the model's right axis whatever the hips' pose: the
    /// spine by its share of the pitch, the arm below it by all of it; up is up.
    #[test]
    fn the_bend_pitches_the_spine_by_its_share_and_the_arms_by_the_aim() {
        let hips = Quat::from_euler(EulerRot::XYZ, 0.1, -0.3, 0.05);
        let (spine, shoulder) = (Quat::from_rotation_z(0.2), Quat::from_rotation_x(-0.4));
        let a = 0.6;
        let (s2, sh2) = bend(hips, spine, shoulder, a);
        // Each joint's turn in the model's frame: about its x, up (+z toward +y) by −angle.
        let turned = |from: Quat, to: Quat, angle: f32| {
            (to * from.inverse()).angle_between(Quat::from_rotation_x(-angle)) < 1e-4
        };
        assert!(turned(hips * spine, hips * s2, a * SPINE_SHARE));
        assert!(turned(hips * spine * shoulder, hips * s2 * sh2, a));
        assert!((Quat::from_rotation_x(-a) * Vec3::Z).y > 0.0, "up");
        // The head's drift at the aim's limit (80°): inside the server's head sphere (radius 20).
        let drift = head_drift(conker_sim::movement::AIM_PITCH_MAX);
        assert!(
            drift > 15.0 && drift < conker_rules::projectile::HEAD_PART[1],
            "{drift}"
        );
    }
}
