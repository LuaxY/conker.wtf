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

use crate::render::MODEL_SCALE;
use crate::{Anim, Config};
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_anim::AnimPlayer;
use conker_anim::combat::{GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW, MASK_OFF, MASK_ON};
use conker_assets::{Held, Light, PACK_WEAPONS, held_gltf, props_gltf};
use conker_rules::net::Fighting;
use conker_rules::weapon::weapon;
use conker_sim::net::{Combat, MatchView, Player};
use lightyear::prelude::Predicted;

#[derive(Resource)]
struct HeldModels(Vec<Held>);

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

/// The systems that hang the held models on every player's joints and show them.
pub fn build(app: &mut App, held: Vec<Held>, carried: Option<Light>) {
    app.insert_resource(HeldModels(held));
    app.insert_resource(CarryLight(carried));
    app.add_systems(Update, (hang, show, hang_gas, show_gas).chain());
    app.add_systems(
        PostUpdate,
        twist
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    );
}

/// A player's upper body (`joint_01`) and head (`joint_12`), which the twist turns.
#[derive(Component)]
pub(crate) struct Twisted(Entity, pub(crate) Option<Entity>);

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

/// Once a player's model is in the world, every held model goes under its joint, hidden. The glTF
/// loads after the character's (two GLBs loading at once overflowed the IO thread's stack, see
/// `render::load_characters`).
#[allow(clippy::too_many_arguments)]
fn hang(
    players: Query<Entity, (With<Anim>, Without<Hands>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    held: Res<HeldModels>,
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
                let hand = commands.spawn((
                    WorldAssetRoot(g.scenes[i].clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ChildOf(joint(h.joint)?),
                ));
                Some((i, hand.id()))
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
        Option<&Combat>,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    held: Res<HeldModels>,
    game: Query<&conker_sim::net::MatchView>,
    mut visibility: Query<&mut Visibility>,
) {
    let race = game.iter().any(|m| m.race);
    for (anim, hands, combat, fighting, predicted) in &players {
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
            let shown = !raptor
                && match h.create.iter().any(|c| c.0 == MASK_ON) {
                    _ if racer => clip_shows(h, &anim.0),
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
