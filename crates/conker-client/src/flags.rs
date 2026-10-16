//! War: Colors' two flags (`lab/research/mp-original/colors.md` §2): bank-01 #169, the red, and
//! #168, the green (the pack's `characters/169`, `characters/168`), each playing its cloth clip for
//! its state ([`pair`]). A flag at home or let go of stands where the match has it ([`FlagView`]);
//! a carried one is in its carrier's right hand (`joint_09`), drawn in the joint's frame as a held
//! weapon is: the carried clips leave the flag's root unturned, so its pole runs along the hand's
//! axis, which the carry clips hold upright. V: in 0x1AF the joint is at (−7.2, 40, 19.9) from the
//! squirrel's feet, conker-rules' `colors::HELD` (the flag object's mean place in e1). Hidden while a
//! War outro plays.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_anim::combat::{FLAG_RUN, FLAG_WALK};
use conker_assets::character_gltf;
use conker_rules::colors::FLAGS;
use conker_sim::net::{Combat, FlagView, MatchView};

use crate::Anim;
use crate::outro::{Outros, Playing};
use crate::render::{Model, model_yaw};

/// The flags are drawn at half their model units (+0x20/+0x24 of their scene records in bank 0x0E,
/// 0.5; C), with no lift: the players' joints' frame.
const SCALE: f32 = 0.5;
/// A flag at home faces 0x3E00: its scene record's yaw byte 0x3E at +0xC (C; V obj+0x40 = 177.19°,
/// the facing + 90°, in e1 and b1).
const HOME_FACING: u16 = 0x3E00;
/// The carrier's right hand.
const HAND: &str = "joint_09";

/// A flag's model ([`FLAGS`]' `f`): its glTF, and once its scene is in, its `AnimationPlayer` and
/// the nodes of its clips by pair; the pair playing.
#[derive(Component)]
pub struct Flag {
    f: usize,
    gltf: Handle<Gltf>,
    animator: Option<(Entity, Vec<AnimationNodeIndex>)>,
    pair: Option<usize>,
}

/// Flag `f` standing (at home or let go of), turned as its last carrier was (a dropped flag keeps
/// its carrier's turn: V b1 obj+0x40).
#[derive(Component)]
struct Free {
    f: usize,
    turn: Quat,
}

/// A player's two flags in its hand, by flag.
#[derive(Component)]
struct InHand([Entity; 2]);

/// Each flag's clip graph and its nodes, made once (the clips' lengths set once).
#[derive(Resource, Default)]
struct Graphs([Option<(Handle<AnimationGraph>, Vec<AnimationNodeIndex>)>; 2]);

pub fn build(app: &mut App) {
    app.init_resource::<Graphs>();
    app.add_systems(Update, (hang, show).chain());
}

/// The cloth's clip (its pair; glTF animation n is pair n) for a flag carried by a player in clip
/// `carrier`, or not carried: obj+0x84 is 3 at home and 5 let go of (logical ids of pair 3), 4 while
/// carried (pair 0), 1 while the carrier walks (pair 1), 2 while it runs (pair 2) (V e1 320-412, b1).
pub fn pair(carrier: Option<u16>) -> usize {
    match carrier {
        None => 3,
        Some(FLAG_WALK) => 1,
        Some(FLAG_RUN) => 2,
        Some(_) => 0,
    }
}

/// Where a flag not in a hand stands and how it is turned: at home [`HOME_FACING`], let go of
/// `last`.
pub fn place(v: &FlagView, last: Quat) -> (Vec3, Quat) {
    let turn = match v.home {
        true => Quat::from_rotation_y(model_yaw(HOME_FACING)),
        false => last,
    };
    (Vec3::from_array(v.pos), turn)
}

/// Flag `f`'s model, hidden, with `bundle`.
fn spawn(commands: &mut Commands, assets: &AssetServer, f: usize, bundle: impl Bundle) -> Entity {
    let gltf = character_gltf(&FLAGS[f].model.to_string());
    let flag = Flag {
        f,
        gltf: assets.load(&gltf),
        animator: None,
        pair: None,
    };
    let scene = WorldAssetRoot(assets.load(format!("{gltf}#Scene0")));
    (commands.spawn((flag, scene, Visibility::Hidden, bundle)))
        .observe(ready)
        .id()
}

/// In War: Colors, both standing flags, once the players' characters are in.
pub(crate) fn add(
    game: Query<&MatchView>,
    assets: Res<AssetServer>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if *added || !game.single().is_ok_and(|m| m.colors) {
        return;
    }
    *added = true;
    for f in 0..FLAGS.len() {
        let turn = Quat::from_rotation_y(model_yaw(HOME_FACING));
        let at = Transform::from_scale(Vec3::splat(SCALE));
        spawn(&mut commands, &assets, f, (Free { f, turn }, at));
    }
}

/// In War: Colors, once a player's model is in, both flags go under its hand, hidden.
fn hang(
    game: Query<&MatchView>,
    players: Query<Entity, (With<Model>, Without<InHand>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    if players.is_empty() || !game.single().is_ok_and(|m| m.colors) {
        return;
    }
    for player in &players {
        let Some(hand) = (children.iter_descendants(player))
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == HAND))
        else {
            continue;
        };
        let hands = [0, 1].map(|f| {
            spawn(
                &mut commands,
                &assets,
                f,
                (Transform::IDENTITY, ChildOf(hand)),
            )
        });
        commands.entity(player).insert(InHand(hands));
    }
}

/// A flag's scene is in: its `AnimationPlayer` gets the flag's graph, whose clips loop on the
/// game's length (the glTF's last key is a tick short of it).
#[allow(clippy::too_many_arguments)]
fn ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    mut flags: Query<&mut Flag>,
    gltfs: Res<Assets<Gltf>>,
    mut made: ResMut<Graphs>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let Ok(mut flag) = flags.get_mut(ready.entity) else {
        return;
    };
    let animator = (children.iter_descendants(ready.entity)).find(|&e| animators.contains(e));
    let handles = gltfs.get(&flag.gltf).map(|g| g.animations.clone());
    let (Some(animator), Some(handles)) = (animator, handles.filter(|h| h.len() == 4)) else {
        warn!("flag {}: no animation player or not 4 clips", flag.f);
        return;
    };
    let (graph, nodes) = made.0[flag.f].get_or_insert_with(|| {
        for h in &handles {
            if let Some(mut c) = clips.get_mut(h) {
                let d = c.duration();
                c.set_duration(d + 1.0 / conker_sim::TICK_HZ as f32);
            }
        }
        let (graph, nodes) = AnimationGraph::from_clips(handles);
        (graphs.add(graph), nodes)
    });
    commands
        .entity(animator)
        .insert(AnimationGraphHandle(graph.clone()));
    flag.animator = Some((animator, nodes.clone()));
}

/// Each flag where and as the match has it: a standing one at its place, shown unless carried; a
/// carrier's in its hand, playing the clip of the carrier's. ponytail: the cloth plays at a tick a
/// frame, I (the flag object's clip rate is not read).
#[allow(clippy::type_complexity)]
fn show(
    game: Query<&MatchView>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    players: Query<(&Combat, &Transform, &Anim, Option<&InHand>), With<Model>>,
    mut free: Query<(Entity, &mut Free, &mut Transform), Without<Model>>,
    mut flags: Query<(&mut Flag, &mut Visibility)>,
    mut animators: Query<&mut AnimationPlayer>,
) {
    let Ok(m) = game.single() else {
        return;
    };
    let outro = playing.of(&outros).is_some();
    let mut wanted = Vec::new();
    for (entity, mut free, mut transform) in &mut free {
        let v = &m.flags[free.f];
        let carrier = (v.carrier).and_then(|s| players.iter().find(|p| p.0.slot == s));
        if let Some((_, t, ..)) = carrier {
            free.turn = t.rotation;
        }
        (transform.translation, transform.rotation) = place(v, free.turn);
        let held = carrier.is_some_and(|p| p.3.is_some());
        wanted.push((entity, v.shown && !held && !outro, pair(None)));
    }
    for (c, _, anim, hands) in &players {
        for (f, &entity) in hands.iter().flat_map(|h| h.0.iter().enumerate()) {
            let shown = c.flag == Some(f as u8) && m.flags[f].shown;
            wanted.push((entity, shown, pair(Some(anim.0.id))));
        }
    }
    for (entity, shown, pair) in wanted {
        let Ok((mut flag, mut visibility)) = flags.get_mut(entity) else {
            continue;
        };
        visibility.set_if_neq(match shown {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        let pair = shown.then_some(pair);
        if flag.pair == pair {
            continue;
        }
        if let Some((animator, nodes)) = &flag.animator
            && let Ok(mut animator) = animators.get_mut(*animator)
        {
            animator.stop_all();
            if let Some(p) = pair {
                animator.play(nodes[p]).repeat();
            }
            flag.pair = pair;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_anim::combat::{FLAG_GRAB, FLAG_IDLE, FLAG_PLANT};

    /// At home turned as the game's obj+0x40, let go of turned as its carrier was; the cloth's pair by
    /// the carrier's clip (V e1).
    #[test]
    fn flags_stand_and_wave_as_the_game() {
        let home = FlagView {
            home: true,
            ..FlagView::default()
        };
        let (_, turn) = place(&home, Quat::IDENTITY);
        let yaw = turn.to_euler(EulerRot::YXZ).0.to_degrees();
        assert!((yaw - 177.19).abs() < 0.01, "{yaw}");
        let last = Quat::from_rotation_y(1.0);
        assert_eq!(place(&FlagView::default(), last).1, last);
        let clips = [
            None,
            Some(FLAG_GRAB),
            Some(FLAG_IDLE),
            Some(FLAG_WALK),
            Some(FLAG_RUN),
            Some(FLAG_PLANT),
        ];
        assert_eq!(clips.map(pair), [3, 0, 0, 1, 2, 0]);
    }
}
