//! The weapon (or grenade) in each player's hand, as the game draws it: a bank-09 model hung from
//! one joint of the character (`joint_09`, the right hand; `joint_04`, the left: the second twin
//! gun and the grenade) and drawn in that joint's frame, unchanged ([`conker_assets::Held`]: the
//! pack's `held.json`, and glTF scene `i` of its `held.gltf` is entry `i`).
//! Its own animation events show and hide it: the draw clip's event puts it in the hand, the
//! holster clip's takes it away. The item is the grenade while its clips play, else the player's
//! weapon; with no event fired yet, a drawn weapon and the held grenade show.
//! In War: Total War a carrier holds the gas canister ([`GAS_JOINT`]) and shines its green light
//! (the pack's carried light, [`crate::lights`]), and the gas mask (bank-09 #67 in the hand, #66 on
//! the head) follows its own clips ([`mask_shows`]).
//! The aimed gun's twist ([`conker_sim::movement::PlayerState::twist`]) turns the upper body
//! (`joint_01`, the arms and the guns with it) toward the aim; the head keeps the legs' facing
//! ([`twist`]).
//! The child objects ([`RIGS`]: the crossbows, item 0x3A's grenade) are drawn from their own rig
//! ([`Rigged`]), posed from the holder's state as `func_15054A94` does (C;
//! `../lab/research/mp-original/held-leftovers.md` §1): the crossbow by its holder's clip
//! ([`crossbow_pose`], its loaded bolt [`loaded_bolt`]) in the hand's frame, item 0x3A's grenade by
//! the frames since its pull ([`lob_pose`]) at its own scale 1.0, in the hand's frame without the
//! hand's scale (V `held-client-scratch` p1a DLs 110, 126: its root in the normalised hand frame is
//! its clip's root; the crossbow's joint 0 is at the hand's scale, cbs DLs 100-233).
//! The models the game's code hangs with no event ([`worn`]): a shotgun's or chainsaw's victim's
//! stump ([`crate::corpses`]' cut) and the racer classes' helmets in any mode.

use crate::render::MODEL_SCALE;
use crate::{Anim, Config};
use bevy::gltf::{Gltf, GltfExtras};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_anim::AnimPlayer;
use conker_anim::combat::{
    BONE_CROSSBOW_DRAW, BONE_CROSSBOW_FIRE, CROSSBOW_DRAW, CROSSBOW_FIRE, CROSSBOW_HOLSTER,
    GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW, MASK_OFF, MASK_ON,
};
use conker_assets::{
    Held, Light, PACK_WEAPONS, RIGS, held_gltf, projectiles_gltf, props_gltf, rig_gltf,
};
use conker_rules::net::Fighting;
use conker_rules::projectile::{COOK, LOB_HOLD, LOB_PULL};
use conker_rules::weapon::weapon;
use conker_sim::net::{Combat, MatchView, Player};
use lightyear::prelude::Predicted;

#[derive(Resource)]
struct HeldModels(Vec<Held>);

/// Item 0x3A's grenade on this map: its [`RIGS`] list (0 #137 on 0x1A, 1 #186 on 0x3F: the
/// thrown object is 0xBA on 0x3F, 0x89 elsewhere, C); `None` where C-down pulls item 0x26.
#[derive(Resource, Clone, Copy)]
pub(crate) struct Lob(pub Option<usize>);

/// [`RIGS`]' entries: the crossbows' rig and item 0x3A's.
pub(crate) const CROSSBOW: usize = 0;
pub(crate) const LOB: usize = 1;
/// A rig's clips 0-3 (glTF animation names; clip 0 its empty pair's pose, the descriptor's bases:
/// the crossbow's rest, the grenade's bent fuse).
const RIG_CLIPS: [&str; 4] = [
    "pair_0000_anim_0000",
    "pair_0001_anim_0001",
    "pair_0002_anim_0002",
    "pair_0003_anim_0003",
];
/// The crossbow's fire and holster clips' lengths (ticks): the copied clock stops a tick short
/// (`func_1507C324`: min(parent time, length − 1), C).
const XBOW_FIRE_TICKS: f32 = 31.0;
const XBOW_HOLSTER_TICKS: f32 = 36.0;
/// The loaded bolt hangs from the crossbow's `joint_02` (action 0x51, bank-09 #0x35, drawn with
/// that joint's matrix: V cbs DL 147) while the fire clip's clock is in this range.
const BOLT_JOINT: &str = "joint_02";
const LOADED: std::ops::Range<f32> = 6.0..11.0;
/// Item 0x3A's grenade plays its pull (clip 2) for this many frames after the pull's first, then
/// its hold (clip 1, looping on its 30 ticks) (V held-pack-scratch p1a: clip 2 at 0-22 on reads
/// 102-124, clip 1 from 0 on 125).
const LOB_PULL_TICKS: u32 = 22;
const LOB_HOLD_TICKS: u32 = 30;

/// The crossbow's clip and time (ticks) for its holder in clip `anim` at `clock`
/// (`func_15054A94`, C; V `held-leftovers-scratch` cb): the draw shows clip 0 (the bind pose),
/// the fire clip 2 and the holster clip 3 at the holder's clock, anything else (the idle, the
/// moves, the aim and the aimed shot) clip 1, a one-tick pose.
pub fn crossbow_pose(anim: u16, clock: f32) -> (usize, f32) {
    match anim {
        CROSSBOW_DRAW | BONE_CROSSBOW_DRAW => (0, 0.0),
        CROSSBOW_FIRE | BONE_CROSSBOW_FIRE => (2, clock.min(XBOW_FIRE_TICKS - 1.0)),
        CROSSBOW_HOLSTER => (3, clock.min(XBOW_HOLSTER_TICKS - 1.0)),
        _ => (1, 0.0),
    }
}

/// Whether the crossbow holds its loaded bolt: its holder's fire clip's clock in [6, 11) (action
/// 0x51 made at 6, gone at 11 as the bolt leaves; C; V cbd/cbs: drawn in DLs 145-149).
pub fn loaded_bolt(anim: u16, clock: f32) -> bool {
    matches!(anim, CROSSBOW_FIRE | BONE_CROSSBOW_FIRE) && LOADED.contains(&clock)
}

/// Item 0x3A's grenade's clip and time `f` frames after the pull's first: the pull (clip 2) at f
/// up to 22, then the hold (clip 1) from 0, a tick a frame.
pub fn lob_pose(f: u32) -> (usize, f32) {
    match f.checked_sub(LOB_PULL_TICKS + 1) {
        None => (2, f as f32),
        Some(k) => (1, (k % LOB_HOLD_TICKS) as f32),
    }
}

/// Frames since item 0x3A's pull, from its cook timer ([`COOK`] at the pull, 2 VIs less a frame).
pub fn since_pull(cook: u16) -> u32 {
    u32::from(COOK.saturating_sub(cook) / 2)
}

/// A child object drawn from its own rig: `rig` its [`RIGS`] entry, `list` the draw list it
/// shows; once its scene is in ([`rig_ready`]), its animator with clips 0-3, every joint's rest
/// (a clip keys only some of them: the others are at rest) and the crossbow's loaded bolt. `gltf`
/// keeps the rig's glTF (its clips) loaded.
#[derive(Component)]
pub(crate) struct Rigged {
    rig: usize,
    list: usize,
    gltf: Handle<Gltf>,
    animator: Option<(Entity, Vec<AnimationNodeIndex>)>,
    rest: Vec<(Entity, Transform)>,
    bolt: Option<Entity>,
}

impl Rigged {
    /// The rig's scene, and the component for list `list` of [`RIGS`] entry `rig`.
    pub(crate) fn spawn(rig: usize, list: usize, assets: &AssetServer) -> impl Bundle {
        let gltf = rig_gltf(PACK_WEAPONS, RIGS[rig].0);
        (
            WorldAssetRoot(assets.load(format!("{gltf}#Scene0"))),
            Self {
                rig,
                list,
                gltf: assets.load(gltf),
                animator: None,
                rest: Vec::new(),
                bolt: None,
            },
        )
    }
}

/// A rig's scene is in: the other list's primitives hide, its `AnimationPlayer` gets clips 0-3,
/// its joints' rest is kept, and the crossbow's loaded bolt (`projectiles.gltf`'s bolt) goes under
/// its `joint_02`, hidden.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rig_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    parts: Query<(Option<&Name>, &Transform, Option<&GltfExtras>)>,
    animators: Query<(), With<AnimationPlayer>>,
    mut rigs: Query<&mut Rigged>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let Ok(mut rig) = rigs.get_mut(ready.entity) else {
        return;
    };
    let mut animator = None;
    for e in children.iter_descendants(ready.entity) {
        animator = animator.or(animators.contains(e).then_some(e));
        let Ok((name, t, extras)) = parts.get(e) else {
            continue;
        };
        let list = extras.and_then(|x| {
            let v = serde_json::from_str::<serde_json::Value>(&x.value).ok()?;
            v.get("list")?.as_u64()
        });
        if list.is_some_and(|l| l != rig.list as u64) {
            commands.entity(e).insert(Visibility::Hidden);
        }
        let Some(name) = name.filter(|n| n.starts_with("joint_")) else {
            continue;
        };
        rig.rest.push((e, *t));
        if rig.rig == CROSSBOW && name.as_str() == BOLT_JOINT {
            let bolt = format!(
                "{}#Scene{}",
                projectiles_gltf(PACK_WEAPONS),
                crate::projectiles::BOLT_SCENE
            );
            let bolt = (WorldAssetRoot(assets.load(bolt)), Visibility::Hidden);
            rig.bolt = Some(commands.spawn((bolt, Transform::IDENTITY, ChildOf(e))).id());
        }
    }
    let clips = gltfs.get(&rig.gltf).and_then(|g| {
        (RIG_CLIPS.iter())
            .map(|c| g.named_animations.get(*c).cloned())
            .collect::<Option<Vec<_>>>()
    });
    let (Some(animator), Some(clips)) = (animator, clips) else {
        warn!("{}: no animation player, or clips missing", RIGS[rig.rig].0);
        return;
    };
    let (graph, nodes) = AnimationGraph::from_clips(clips);
    (commands.entity(animator)).insert(AnimationGraphHandle(graphs.add(graph)));
    rig.animator = Some((animator, nodes));
}

/// Shows rig `rig` in clip `clip` at `time` (ticks): every joint at rest, then the clip's keys.
pub(crate) fn pose_rig(
    rig: &Rigged,
    (clip, time): (usize, f32),
    animators: &mut Query<&mut AnimationPlayer>,
    joints: &mut Query<&mut Transform>,
) {
    for (joint, rest) in &rig.rest {
        if let Ok(mut t) = joints.get_mut(*joint) {
            t.set_if_neq(*rest);
        }
    }
    let Some((animator, nodes)) = &rig.animator else {
        return;
    };
    let (Ok(mut animator), Some(&node)) = (animators.get_mut(*animator), nodes.get(clip)) else {
        return;
    };
    let stale: Vec<_> = (animator.playing_animations())
        .map(|(n, _)| *n)
        .filter(|&n| n != node)
        .collect();
    for n in stale {
        animator.stop(n);
    }
    (animator.play(node))
        .set_speed(0.0)
        .set_seek_time(time / conker_sim::TICK_HZ as f32);
}

/// A player's held models: (index into [`HeldModels`], the entity under its joint).
#[derive(Component)]
pub(crate) struct Hands(Vec<(usize, Entity)>);

/// The light on a canister's carrier (`lights.json`'s carried one; none on other maps).
#[derive(Resource)]
struct CarryLight(Option<Light>);

/// A player's gas canister (War: Total War), under its joint, and its carried light.
#[derive(Component)]
struct GasHands(Entity, Option<Entity>);

/// Where the canister (`props.gltf`'s scene 1) hangs: the root joint (`joint_00`), upright with its
/// middle between the hands of the carry clips (0xD8's first frame: hands at x ±31, 43 up, 26
/// forward); offset in the joint's frame (model units). I: no clip creates it (no create event in
/// any route table's 0xD7, 0xD8, 0x4E, 0x51), the game's weapon manager draws item 0x25 by a path
/// not read; joint and offset fitted.
pub const GAS_JOINT: (u8, [f32; 3]) = (0, [0.0, -30.0, 24.0]);

/// The systems that hang the held models on every player's joints and show them; `lob`: item
/// 0x3A's [`Lob`] list on this map.
pub fn build(app: &mut App, held: Vec<Held>, carried: Option<Light>, lob: Option<usize>) {
    app.insert_resource(HeldModels(held));
    app.insert_resource(CarryLight(carried));
    app.insert_resource(Lob(lob));
    app.add_systems(Update, (hang, show, pose, hang_gas, show_gas).chain());
    app.add_systems(
        PostUpdate,
        (twist, unscale_lobs)
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    );
}

/// Item 0x3A's grenade in the hand is drawn at its own scale (1.0), not the hand's: its root's
/// scale undoes its ancestors' (the player's 0.5 and the clip's: V p1a DL 110, a hand at 0.269).
fn unscale_lobs(
    rigs: Query<(Entity, &Rigged, &Visibility, Has<ChildOf>)>,
    parents: Query<&ChildOf>,
    mut transforms: Query<&mut Transform>,
) {
    for (hand, rig, v, held) in &rigs {
        if rig.rig != LOB || !held || *v == Visibility::Hidden {
            continue;
        }
        let scale: f32 = (parents.iter_ancestors(hand))
            .map(|e| transforms.get(e).map_or(1.0, |t| t.scale.x))
            .product();
        if let Ok(mut t) = transforms.get_mut(hand) {
            t.set_if_neq(Transform::from_scale(Vec3::splat(scale.recip())));
        }
    }
}

/// A player's upper body (`joint_01`) and head (`joint_12`), which the twist turns.
#[derive(Component)]
pub(crate) struct Twisted(pub(crate) Entity, pub(crate) Option<Entity>);

/// The twist as a turn about the body's up axis: the upper body aims along `facing − twist`, as
/// the camera does (V twin: `joint_01`'s yaw on the root's 5.3° at twist 0, 54.9° at 9000 (49.4°),
/// −9.3° at −2660; the arms and guns, its children, within 1.6°; the head, `joint_12`, not
/// following: 7.5°, 17.2° and 28.2° on the root, its own drift a look, I).
pub fn twist_turn(twist: i16) -> Quat {
    Quat::from_rotation_y(-f32::from(twist) * std::f32::consts::TAU / 65536.0)
}

/// Turns each player's upper body by its twist, after the clips posed it, and the head back.
fn twist(players: Query<(&Player, &Twisted)>, mut joints: Query<&mut Transform>) {
    for (p, t) in &players {
        if p.0.twist == 0 {
            continue;
        }
        let Ok(mut spine) = joints.get_mut(t.0) else {
            continue;
        };
        let (pose, turn) = (spine.rotation, twist_turn(p.0.twist));
        spine.rotation = turn * pose;
        if let Some(Ok(mut head)) = t.1.map(|h| joints.get_mut(h)) {
            head.rotation = pose.inverse() * turn.inverse() * pose * head.rotation;
        }
    }
}

/// Whether held model `h` is in the hand of a player holding `weapon` (0: none), `drawn` or not,
/// whose animation clock is `clock`. The last of the model's events in this clip that fired (the
/// clock reached its time; V: the oracle's attachment calls) decides. A removal while the weapon
/// stays drawn (weapon 0x14's throws) lasts 2 frames: the game's weapon manager puts it back (V
/// with two players; I: it visits one player a frame). The grenade's clips are class `class`'s
/// own ([`conker_rules::characters::clip`]: a Tedi's put #75 in the hand, everyone else's #76).
pub fn shows(h: &Held, weapon_id: u8, drawn: bool, clock: &AnimPlayer, class: u8) -> bool {
    let grenade = [GRENADE_PULL, GRENADE_HOLD, GRENADE_THROW].contains(&clock.id);
    let own = |id| conker_rules::characters::clip(class, id);
    let (id, item) = match grenade {
        true => (own(clock.id), Some((own(GRENADE_PULL), own(GRENADE_THROW)))),
        false => (clock.id, weapon(weapon_id).map(|w| (w.draw.0, w.holster.0))),
    };
    let Some((_, away)) = item.filter(|i| h.create.iter().any(|c| c.0 == i.0)) else {
        return false;
    };
    let fired = (h.create.iter().map(|&e| (e, true)))
        .chain(h.remove.iter().map(|&e| (e, false)))
        .filter(|((e, t), _)| *e == id && clock.time >= *t)
        .max_by(|a, b| a.0.1.total_cmp(&b.0.1));
    match fired {
        Some((_, true)) => true,
        Some(((_, t), false)) => id != away && drawn && clock.time - t >= 2.0 * clock.rate,
        None if grenade => clock.id == GRENADE_HOLD,
        None => drawn,
    }
}

/// Whether held model `h` is in the hand by its own clip's events alone (Race's kick: the fist,
/// #0x34, made at t5 and gone at t30 of clip 0x354): the last of its events in the playing clip
/// that fired is a create.
pub fn clip_shows(h: &Held, clock: &AnimPlayer) -> bool {
    (h.create.iter().map(|&e| (e, true)))
        .chain(h.remove.iter().map(|&e| (e, false)))
        .filter(|((id, t), _)| *id == clock.id && clock.time >= *t)
        .max_by(|a, b| a.0.1.total_cmp(&b.0.1))
        .is_some_and(|(_, create)| create)
}

/// Whether gas-mask model `h` (its create events are in the put-on clip [`MASK_ON`]) shows on a
/// player wearing a mask or not (`masked`) whose animation clock is `clock`. In the put-on the last
/// of its put-on events that fired decides (the hand's create at t26: frame 9); in the take-off
/// ([`MASK_OFF`]) nothing shows (neither of its creates fires, `extract_character::due`); else a
/// wearer has the models whose last put-on event is a create (the head, from the idle after the
/// put-on: V frame 20 on e2; I: its create at t59 never fires, the path that draws it is not read).
pub fn mask_shows(h: &Held, masked: bool, clock: &AnimPlayer) -> bool {
    let created = |at: f32| {
        (h.create.iter().map(|&e| (e, true)))
            .chain(h.remove.iter().map(|&e| (e, false)))
            .filter(|((id, t), _)| *id == MASK_ON && *t <= at)
            .max_by(|a, b| a.0.1.total_cmp(&b.0.1))
            .is_some_and(|(_, create)| create)
    };
    match clock.id {
        MASK_ON => created(clock.time),
        MASK_OFF => false,
        _ => masked && created(f32::INFINITY),
    }
}

/// Whether held model `h` shows on a player of class `class`, cut in two or not (`halved`), if it
/// is one the game's code hangs with no event: a head stump on its model's cut body (#0x59 on 117,
/// #0x58 on 128: `func_15194AB4`, C; V sg_e1, e_w22) to the respawn, a racer class's helmet
/// (#0x5A, #0x5C, #0x5B on 0xD, 0xE, 0xF: `func_15083384`, C) in any mode; None for the others.
pub fn worn(h: &Held, class: u8, halved: bool) -> Option<bool> {
    let model = conker_rules::characters::model(class);
    Some(match h.entry {
        0x59 => halved && model == Some(117),
        0x58 => halved && model == Some(128),
        0x5A => class == 0xD,
        0x5C => class == 0xE,
        0x5B => class == 0xF,
        _ => return None,
    })
}

/// The [`RIGS`] entry and list of held model `h`, if it is a child object with its own rig.
fn rig_of(h: &Held) -> Option<(usize, usize)> {
    (RIGS.iter().enumerate())
        .find_map(|(r, (_, e))| Some((r, e.iter().position(|&e| e == h.entry)?)))
}

/// Once a player's model is in the world, every held model goes under its joint, hidden: a child
/// object as its rig ([`Rigged`]; item 0x3A's grenade only where C-down pulls it), the others as
/// `held.gltf`'s scene. The glTF loads after the character's (two GLBs loading at once overflowed
/// the IO thread's stack, see `render::load_characters`).
#[allow(clippy::too_many_arguments)]
fn hang(
    players: Query<Entity, (With<Anim>, Without<Hands>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    held: Res<HeldModels>,
    lob: Res<Lob>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut gltf: Local<Option<Handle<Gltf>>>,
    mut commands: Commands,
) {
    for player in &players {
        let joints: Vec<(Entity, &Name)> = children
            .iter_descendants(player)
            .filter_map(|e| Some((e, names.get(e).ok()?)))
            .collect();
        let joint = |j: u8| {
            let name = format!("joint_{j:02}");
            joints.iter().find(|(_, n)| n.as_str() == name).map(|j| j.0)
        };
        if joint(0).is_none() {
            continue;
        }
        let handle = gltf.get_or_insert_with(|| assets.load(held_gltf(PACK_WEAPONS)));
        let Some(g) = gltfs
            .get(&*handle)
            .filter(|_| assets.is_loaded_with_dependencies(&*handle))
        else {
            return;
        };
        let hands = (held.0.iter().enumerate())
            .filter_map(|(i, h)| {
                // A stump on a joint the cut collapses undoes it ([`crate::corpses::CUT`]).
                let cut = [0x58, 0x59].contains(&h.entry) && (1..16).contains(&h.joint);
                let scale = if cut {
                    crate::corpses::CUT.recip()
                } else {
                    1.0
                };
                let at = (
                    Transform::from_scale(Vec3::splat(scale)),
                    Visibility::Hidden,
                    ChildOf(joint(h.joint)?),
                );
                let hand = match rig_of(h) {
                    Some((LOB, list)) if lob.0 != Some(list) => return None,
                    Some((rig, list)) => {
                        let rig = Rigged::spawn(rig, list, &assets);
                        commands.spawn((rig, at)).observe(rig_ready).id()
                    }
                    None => commands
                        .spawn((WorldAssetRoot(g.scenes[i].clone()), at))
                        .id(),
                };
                Some((i, hand))
            })
            .collect();
        let mut player = commands.entity(player);
        player.insert(Hands(hands));
        if let Some(spine) = joint(1) {
            player.insert(Twisted(spine, joint(12)));
        }
    }
}

/// In War: Total War, once a player's held models hang, its canister goes under its joint
/// ([`GAS_JOINT`]), hidden, at the world's size (the props are not drawn at the players' half), and
/// the carried light at the player's position (V e1: on the carrier's object).
#[allow(clippy::too_many_arguments)]
fn hang_gas(
    players: Query<Entity, (With<Hands>, Without<GasHands>)>,
    game: Query<&MatchView>,
    children: Query<&Children>,
    names: Query<&Name>,
    cfg: Res<Config>,
    carried: Res<CarryLight>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut gltf: Local<Option<Handle<Gltf>>>,
    mut commands: Commands,
) {
    if players.is_empty() || !game.single().is_ok_and(|m| m.total_war) {
        return;
    }
    let handle = gltf.get_or_insert_with(|| assets.load(props_gltf(&cfg.map)));
    let Some(g) = gltfs
        .get(&*handle)
        .filter(|_| assets.is_loaded_with_dependencies(&*handle))
    else {
        return;
    };
    let name = format!("joint_{:02}", GAS_JOINT.0);
    for player in &players {
        let Some(root) = (children.iter_descendants(player))
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
        else {
            continue;
        };
        let at = Transform::from_translation(Vec3::from(GAS_JOINT.1));
        let part = (commands.spawn((
            WorldAssetRoot(g.scenes[1].clone()),
            at.with_scale(Vec3::splat(MODEL_SCALE.recip())),
            Visibility::Hidden,
            ChildOf(root),
        )))
        .id();
        let light = (carried.0.as_ref()).map(|l| {
            let light = (crate::lights::point_light(l, l.size), Visibility::Hidden);
            commands
                .spawn((light, Transform::IDENTITY, ChildOf(player)))
                .id()
        });
        commands.entity(player).insert(GasHands(part, light));
    }
}

/// Shows the canister and its light while its player carries it (the server's [`Combat`]: not
/// predicted).
fn show_gas(players: Query<(&Combat, &GasHands)>, mut visibility: Query<&mut Visibility>) {
    for (c, hands) in &players {
        for part in [Some(hands.0), hands.1].into_iter().flatten() {
            if let Ok(mut v) = visibility.get_mut(part) {
                v.set_if_neq(match c.carrying {
                    true => Visibility::Inherited,
                    false => Visibility::Hidden,
                });
            }
        }
    }
}

/// Shows or hides each player's held models. The local player's weapon and mask state is its
/// prediction, the clock's own source; the others' is the server's.
#[allow(clippy::type_complexity)]
fn show(
    players: Query<(
        &Anim,
        &Hands,
        &crate::corpses::Corpse,
        Option<&Combat>,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    held: Res<HeldModels>,
    game: Query<&conker_sim::net::MatchView>,
    mut visibility: Query<&mut Visibility>,
) {
    let race = game.iter().any(|m| m.race);
    for (anim, hands, corpse, combat, fighting, predicted) in &players {
        let (weapon_id, drawn, masked) = match (fighting, combat) {
            (Some(f), _) if predicted => {
                (f.state.weapon.unwrap_or(0), f.state.drawn, f.state.masked)
            }
            (_, Some(c)) => (c.weapon, c.drawn, c.masked),
            _ => (0, false, false),
        };
        // A raptor holds nothing (its clips are not the weapons' events'); a racer (anyone in
        // Race) holds what its clip makes (the kick's fist, [`clip_shows`]).
        let class = combat.map_or(0, |c| c.class);
        let raptor = class == conker_rules::raptor::CLASS;
        let racer = combat.is_some() && race;
        for &(i, entity) in &hands.0 {
            let h = &held.0[i];
            // Item 0x3A's grenade (made by code, no events: `hang` hangs this map's only) from
            // the pull's first frame to the throw's first (V p1a: s126+0x75 0xBA, the object a
            // child until 0x1FC).
            let lob = rig_of(h).is_some_and(|r| r.0 == LOB);
            let worn = worn(h, class, corpse.halved);
            let shown = !raptor
                && match h.create.iter().any(|c| c.0 == MASK_ON) {
                    _ if racer => clip_shows(h, &anim.0),
                    _ if lob => [LOB_PULL, LOB_HOLD].contains(&anim.0.id),
                    _ if worn.is_some() => worn == Some(true),
                    true => mask_shows(h, masked, &anim.0),
                    false => shows(h, weapon_id, drawn, &anim.0, class),
                };
            if let Ok(mut v) = visibility.get_mut(entity) {
                v.set_if_neq(match shown {
                    true => Visibility::Inherited,
                    false => Visibility::Hidden,
                });
            }
        }
    }
}

/// Poses each player's shown child objects: the crossbow by its clip (and its loaded bolt), item
/// 0x3A's grenade by the frames since its pull (the local player's predicted cook timer, the
/// others' replicated one).
#[allow(clippy::type_complexity)]
fn pose(
    players: Query<(
        &Anim,
        &Hands,
        Option<&Combat>,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    rigs: Query<(&Rigged, &Visibility)>,
    mut bolts: Query<&mut Visibility, Without<Rigged>>,
    mut animators: Query<&mut AnimationPlayer>,
    mut joints: Query<&mut Transform>,
) {
    for (anim, hands, combat, fighting, predicted) in &players {
        let cook = match (fighting, combat) {
            (Some(f), _) if predicted => f.state.cook,
            (_, Some(c)) => c.cook,
            _ => COOK,
        };
        let a = &anim.0;
        for &(_, hand) in &hands.0 {
            let Some((rig, _)) = rigs.get(hand).ok().filter(|r| *r.1 != Visibility::Hidden) else {
                continue;
            };
            let clip = match rig.rig {
                CROSSBOW => crossbow_pose(a.id, a.time),
                _ => lob_pose(since_pull(cook)),
            };
            pose_rig(rig, clip, &mut animators, &mut joints);
            if let Some(Ok(mut b)) = rig.bolt.map(|b| bolts.get_mut(b)) {
                b.set_if_neq(match loaded_bolt(a.id, a.time) {
                    true => Visibility::Inherited,
                    false => Visibility::Hidden,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_anim::combat::{TWIN_DRAW, TWIN_FIRE, TWIN_HOLSTER, W14_ATTACK};
    use conker_anim::request_at;
    use conker_rules::beach::TEDI_CLASS;
    use conker_rules::game::Action;
    use conker_rules::{Fighter, Placement};

    fn clock(id: u16, time: f32, rate: f32) -> AnimPlayer {
        AnimPlayer {
            id,
            time,
            rate,
            ..default()
        }
    }

    /// The twin guns' right gun and weapon 0x14's knife (events as extracted, ROM-tested in
    /// extract-character), through a draw, firing, a throw and the holster.
    #[test]
    fn events_show_and_hide() {
        let gun = Held {
            entry: 27,
            joint: 9,
            create: vec![(TWIN_DRAW, 15.0)],
            remove: vec![(TWIN_HOLSTER, 14.0)],
        };
        let shown = |w, drawn, c: AnimPlayer| shows(&gun, w, drawn, &c, TEDI_CLASS);
        assert!(!shown(0x09, false, clock(TWIN_DRAW, 13.999998, 1.4)));
        assert!(shown(0x09, false, clock(TWIN_DRAW, 15.4, 1.4)));
        assert!(shown(0x09, true, clock(TWIN_FIRE, 3.0, 1.0)));
        assert!(shown(0x09, true, clock(TWIN_HOLSTER, 13.999998, 1.4)));
        assert!(!shown(0x09, true, clock(TWIN_HOLSTER, 15.4, 1.4)));
        assert!(!shown(0x09, false, clock(conker_anim::id::IDLE, 3.0, 1.0)));
        // Another weapon, none (dead), or the grenade in the hand.
        assert!(!shown(0x39, true, clock(TWIN_FIRE, 3.0, 1.0)));
        assert!(!shown(0, false, clock(TWIN_FIRE, 3.0, 1.0)));
        assert!(!shown(0x09, true, clock(GRENADE_HOLD, 3.0, 1.0)));

        let knife = Held {
            entry: 33,
            joint: 9,
            create: vec![(0x102, 24.0)],
            remove: vec![(W14_ATTACK, 8.0), (0x103, 19.0)],
        };
        let thrown = |time| {
            shows(
                &knife,
                0x14,
                true,
                &clock(W14_ATTACK, time, 1.0),
                TEDI_CLASS,
            )
        };
        assert_eq!(
            [7.0, 8.0, 9.0, 10.0].map(thrown),
            [true, false, false, true]
        );
    }

    /// The grenade in the hand is the class's own: a Tedi's #75 from 0x176, a caveman's #76 from
    /// 0x175 (events as extracted; V e4 sqgren, combat6_ctlw40c: the pull and throw ids).
    #[test]
    fn each_class_holds_its_own_grenade() {
        let grenade = |entry, pull, throw| Held {
            entry,
            joint: 4,
            create: vec![(pull, 19.0)],
            remove: vec![(throw, 15.0)],
        };
        let (tediz, others) = (grenade(75, 0x176, 0x16B), grenade(76, 0x175, 0x16C));
        let held = |class, c: AnimPlayer| [&tediz, &others].map(|h| shows(h, 0, false, &c, class));
        let cave = conker_rules::raptor::CAVEMAN;
        assert_eq!(
            held(TEDI_CLASS, clock(GRENADE_PULL, 19.0, 1.0)),
            [true, false]
        );
        assert_eq!(held(cave, clock(GRENADE_PULL, 19.0, 1.0)), [false, true]);
        assert_eq!(held(cave, clock(GRENADE_HOLD, 3.0, 1.0)), [false, true]);
        assert_eq!(held(cave, clock(GRENADE_THROW, 15.0, 1.0)), [false, false]);
    }

    /// The crossbow's clip from its holder's (`held-leftovers.md` §1's recipe; V cb: 0 in the
    /// draw, 2 in the hip shot, 1 through the aim and the aimed shot, 3 in the holster), the
    /// copied clock stopping a tick short; the loaded bolt for clocks 6 to 11 of the fire.
    #[test]
    fn the_crossbow_follows_its_holder() {
        use conker_anim::combat::{CROSSBOW_AIM, CROSSBOW_AIMED, CROSSBOW_IDLE};
        assert_eq!(crossbow_pose(CROSSBOW_DRAW, 30.0), (0, 0.0));
        assert_eq!(crossbow_pose(BONE_CROSSBOW_DRAW, 30.0), (0, 0.0));
        assert_eq!(crossbow_pose(CROSSBOW_FIRE, 9.0), (2, 9.0));
        assert_eq!(crossbow_pose(BONE_CROSSBOW_FIRE, 9.0), (2, 9.0));
        assert_eq!(crossbow_pose(CROSSBOW_FIRE, 30.5), (2, 30.0));
        assert_eq!(crossbow_pose(CROSSBOW_HOLSTER, 7.0), (3, 7.0));
        assert_eq!(crossbow_pose(CROSSBOW_HOLSTER, 35.9), (3, 35.0));
        for id in [
            CROSSBOW_AIM,
            CROSSBOW_AIMED,
            CROSSBOW_IDLE,
            conker_anim::id::RUN,
        ] {
            assert_eq!(crossbow_pose(id, 12.0), (1, 0.0));
        }
        let loaded: Vec<f32> = (0..30)
            .map(|t| t as f32)
            .filter(|&t| loaded_bolt(CROSSBOW_FIRE, t))
            .collect();
        assert_eq!(loaded, [6.0, 7.0, 8.0, 9.0, 10.0]);
        assert!(loaded_bolt(BONE_CROSSBOW_FIRE, 6.0) && !loaded_bolt(CROSSBOW_FIRE, 11.0));
        assert!(!loaded_bolt(CROSSBOW_AIMED, 8.0));
    }

    /// Item 0x3A's grenade against p1a (`held-pack-scratch/out/anim_p1a.out`): its pull (clip 2)
    /// at 0-22 on reads 102-124, then its hold (clip 1) from 0, looping on 30; the frames since
    /// the pull from the cook timer (240 at the pull, 2 less a frame).
    #[test]
    fn the_lobbed_grenade_follows_its_pull() {
        let read = |r: u32| lob_pose(r - 102);
        assert_eq!(read(102), (2, 0.0));
        assert_eq!(read(124), (2, 22.0));
        assert_eq!(read(125), (1, 0.0));
        assert_eq!(read(131), (1, 6.0));
        assert_eq!(read(125 + 30), (1, 0.0));
        assert_eq!(since_pull(COOK), 0);
        assert_eq!(since_pull(COOK - 2 * 23), 23);
    }

    /// The gas mask's models (events as extracted, ROM-tested in extract-character) at the put-on's
    /// rate: the hand from frame 9 to the put-on's end, the head from the idle after it until the
    /// take-off; nothing in the take-off (V: e2's attachment calls).
    #[test]
    fn mask_in_hand_then_on_head() {
        let hand = Held {
            entry: 67,
            joint: 9,
            create: vec![(MASK_OFF, 59.0), (MASK_ON, 26.0)],
            remove: vec![(MASK_OFF, 59.0), (MASK_ON, 59.0)],
        };
        let head = Held {
            entry: 66,
            joint: 12,
            create: vec![(MASK_ON, 59.0)],
            remove: vec![(MASK_OFF, 24.0)],
        };
        let mut c = AnimPlayer::default();
        let frames: Vec<[bool; 2]> = (0..conker_rules::gas::MASK_FRAMES)
            .map(|_| {
                c.tick(conker_anim::request(MASK_ON, 0.0, 0.0), 999);
                [&hand, &head].map(|h| mask_shows(h, true, &c))
            })
            .collect();
        let first = frames.iter().position(|s| s[0]).map(|i| i + 1);
        assert_eq!(first, Some(9));
        assert!(frames[8..].iter().all(|&s| s == [true, false]));
        let idle = clock(conker_anim::id::IDLE, 3.0, 1.0);
        assert_eq!(
            [&hand, &head].map(|h| mask_shows(h, true, &idle)),
            [false, true]
        );
        assert_eq!(
            [&hand, &head].map(|h| mask_shows(h, false, &idle)),
            [false, false]
        );
        for time in [3.0, 30.0, 57.0] {
            let off = clock(MASK_OFF, time, 3.0);
            assert_eq!(
                [&hand, &head].map(|h| mask_shows(h, true, &off)),
                [false, false]
            );
        }
    }

    /// The pack's draw events at conker-anim's draw rates fire on the frame the rules attach each
    /// weapon and the grenade (`Fighter::in_hand`, V: s126+0x11A on 47 traces). Needs the asset
    /// pack (`CONKER_ASSETS`, else `<repo>/assets`); skips without it.
    #[test]
    fn draw_events_fire_on_the_attach_frame() {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let dir = std::path::Path::new(&dir);
        if !dir.join(conker_assets::MANIFEST).exists() {
            eprintln!("no asset pack in {}: skipping", dir.display());
            return;
        }
        use conker_assets::{PACK_CHARACTERS, PACK_MAP, Slice};
        let read = |p: &str| std::fs::read(dir.join(p));
        let slice = Slice::load(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS, read).unwrap();
        let place = Placement {
            index: 0,
            pos: [0.0; 3],
            facing: 0,
            camera: [0.0; 3],
        };
        let draws = [
            0x09, 0x39, 0x23, 0x24, 0x15, 0x21, 0x22, 0x14, 0x16, 0x18, 0x37, 0x40, 0x19, 0x38,
        ]
        .map(|id| {
            let w = weapon(id).unwrap();
            (Some(id), w.draw.0, Action::Draw)
        });
        let pull = (None, GRENADE_PULL, Action::GrenadePull { pull: 0 });
        for (id, clip, action) in draws.into_iter().chain([pull]) {
            let t = (slice.held.iter().flat_map(|h| &h.create))
                .find(|c| c.0 == clip)
                .unwrap()
                .1;
            let mut c = AnimPlayer::default();
            let frames = (1..)
                .find(|_| {
                    c.tick(request_at(clip, 0.0, 0.0, 0), 999);
                    c.time >= t
                })
                .unwrap();
            let f = Fighter {
                weapon: id,
                action,
                ..Fighter::new(&place, 0, 0)
            };
            assert!(
                !f.in_hand(frames - 1) && f.in_hand(frames),
                "{clip:#x}: {frames}"
            );
        }
    }

    /// The pack's eventless tail (`held.json` 24-28): the stumps on their own model's cut body
    /// only (#0x59 on 117, class 3; #0x58 on 128, class 4; none on Conker's 0), the helmets on
    /// their racer class anywhere (#0x5A, #0x5C, #0x5B on 0xD-0xF; none on the human's 0xB or the
    /// bare 0xC); in Race the racers' own clips decide, and these have none (`race.rs` hangs the
    /// helmets there). The other models are the events'.
    #[test]
    fn stumps_and_helmets_hang_by_model_and_class() {
        let at = |entry, joint| Held {
            entry,
            joint,
            create: vec![],
            remove: vec![],
        };
        let [s117, s128] = [at(0x59, 0), at(0x58, 1)];
        let helmets = [at(0x5A, 12), at(0x5C, 12), at(0x5B, 12)];
        let shown = |h: &Held, class, halved| worn(h, class, halved).unwrap();
        assert!(shown(&s117, 3, true) && !shown(&s117, 3, false) && !shown(&s117, 4, true));
        assert!(shown(&s128, 4, true) && !shown(&s128, 4, false) && !shown(&s128, 3, true));
        assert!(!shown(&s117, 0, true) && !shown(&s128, 0, true));
        for (k, h) in helmets.iter().enumerate() {
            let on: Vec<u8> = (0..0x29).filter(|&c| shown(h, c, false)).collect();
            assert_eq!(on, [0xD + k as u8]);
            assert!(!clip_shows(h, &clock(conker_anim::id::RUN, 3.0, 1.0)));
        }
        assert_eq!(worn(&at(27, 9), 0xD, true), None);
    }

    /// The twisted upper body faces `facing − twist` (the camera's aim), 49.4° at the limit; the
    /// head's counter-turn keeps it on the legs' facing whatever the spine's pose.
    #[test]
    fn the_twist_turns_the_upper_body_to_the_aim() {
        use crate::render::model_yaw;
        let (facing, twist) = (16896u16, 9000i16);
        let body = Quat::from_rotation_y(model_yaw(facing));
        let aim = Quat::from_rotation_y(model_yaw(facing.wrapping_sub(twist as u16)));
        let upper = body * twist_turn(twist);
        assert!((upper * Vec3::Z).distance(aim * Vec3::Z) < 1e-5);
        assert!((twist_turn(twist).to_scaled_axis().y.to_degrees() + 49.44).abs() < 0.01);
        let pose = Quat::from_euler(EulerRot::XYZ, 0.2, 0.3, -0.1);
        let head = Quat::from_rotation_x(0.4);
        let turn = twist_turn(twist);
        let twisted = turn * pose * (pose.inverse() * turn.inverse() * pose * head);
        assert!((twisted * Vec3::Z).distance(pose * head * Vec3::Z) < 1e-5);
    }
}
