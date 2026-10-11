//! The weapon (or grenade) in each player's hand, as the game draws it: a bank-09 model hung from
//! one joint of the character (`joint_09`, the right hand; `joint_04`, the left: the second twin
//! gun and the grenade) and drawn in that joint's frame, unchanged ([`conker_assets::Held`]: the
//! pack's `held.json`, and glTF scene `i` of its `held.gltf` is entry `i`).
//! Its own animation events show and hide it: the draw clip's event puts it in the hand, the
//! holster clip's takes it away. The item is the grenade while its clips play, else the player's
//! weapon; with no event fired yet, a drawn weapon and the held grenade show.

use crate::Anim;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_anim::AnimPlayer;
use conker_anim::combat::{GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW};
use conker_assets::{Held, PACK_WEAPONS, held_gltf};
use conker_rules::net::Fighting;
use conker_rules::weapon::weapon;
use conker_sim::net::Combat;
use lightyear::prelude::Predicted;

#[derive(Resource)]
struct HeldModels(Vec<Held>);

/// A player's held models: (index into [`HeldModels`], the entity under its joint).
#[derive(Component)]
struct Hands(Vec<(usize, Entity)>);

/// The systems that hang the held models on every player's joints and show them.
pub fn build(app: &mut App, held: Vec<Held>) {
    app.insert_resource(HeldModels(held));
    app.add_systems(Update, (hang, show).chain());
}

/// Whether held model `h` is in the hand of a player holding `weapon` (0: none), `drawn` or not,
/// whose animation clock is `clock`. The last of the model's events in this clip that fired (the
/// clock reached its time; V: the oracle's attachment calls) decides. A removal while the weapon
/// stays drawn (weapon 0x14's throws) lasts 2 frames: the game's weapon manager puts it back (V
/// with two players; I: it visits one player a frame).
pub fn shows(h: &Held, weapon_id: u8, drawn: bool, clock: &AnimPlayer) -> bool {
    let grenade = [GRENADE_PULL, GRENADE_HOLD, GRENADE_THROW].contains(&clock.id);
    let item = match grenade {
        true => Some((GRENADE_PULL, GRENADE_THROW)),
        false => weapon(weapon_id).map(|w| (w.draw.0, w.holster.0)),
    };
    let Some((_, away)) = item.filter(|i| h.create.iter().any(|c| c.0 == i.0)) else {
        return false;
    };
    let fired = (h.create.iter().map(|&e| (e, true)))
        .chain(h.remove.iter().map(|&e| (e, false)))
        .filter(|((id, t), _)| *id == clock.id && clock.time >= *t)
        .max_by(|a, b| a.0.1.total_cmp(&b.0.1));
    match fired {
        Some((_, true)) => true,
        Some(((_, t), false)) => clock.id != away && drawn && clock.time - t >= 2.0 * clock.rate,
        None if grenade => clock.id == GRENADE_HOLD,
        None => drawn,
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
        commands.entity(player).insert(Hands(hands));
    }
}

/// Shows or hides each player's held models. The local player's weapon state is its prediction,
/// the clock's own source; the others' is the server's.
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
    mut visibility: Query<&mut Visibility>,
) {
    for (anim, hands, combat, fighting, predicted) in &players {
        let (weapon_id, drawn) = match (fighting, combat) {
            (Some(f), _) if predicted => (f.state.weapon.unwrap_or(0), f.state.drawn),
            (_, Some(c)) => (c.weapon, c.drawn),
            _ => (0, false),
        };
        for &(i, entity) in &hands.0 {
            if let Ok(mut v) = visibility.get_mut(entity) {
                v.set_if_neq(match shows(&held.0[i], weapon_id, drawn, &anim.0) {
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
        let shown = |w, drawn, c: AnimPlayer| shows(&gun, w, drawn, &c);
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
        let thrown = |time| shows(&knife, 0x14, true, &clock(W14_ATTACK, time, 1.0));
        assert_eq!(
            [7.0, 8.0, 9.0, 10.0].map(thrown),
            [true, false, false, true]
        );
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
            0x09, 0x39, 0x23, 0x24, 0x15, 0x21, 0x22, 0x14, 0x16, 0x18, 0x37,
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
            assert!(!f.in_hand(frames - 1) && f.in_hand(frames), "{clip:#x}");
        }
    }
}
