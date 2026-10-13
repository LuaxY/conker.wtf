//! War: Heist's bag (`../lab/research/mp-original/heist-presentation.md` §2): bank-01 #87 (the
//! pack's `characters/87`) at scale 0.4, playing the clip of what it does ([`clip`]): standing,
//! in its carrier's hands, thrown, sliding, let go of, walking home. Free, it stands where the
//! match has it (`CashView`) over its shadow; carried, it is drawn in its carrier's right hand
//! (`joint_09`) at its own scale, its clip's root placing it (V: `bag_root_world = clipRoot(t) ·
//! Scale(0.4) · normalise(joint_09)`), but for the grab's first 9 frames, where it still lies.
//! Hidden while a War outro plays. Its clip's sounds are [`crate::audio`]'s, from [`Bag`].
//!
//! The clip's clock is this client's: a state counts from the tick this client saw it change (the
//! match's tick is not sent), the stand cycle from there too.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::character_gltf;
use conker_rules::heist::{GRAB, MODEL, RUN, WALK};
use conker_sim::net::{Collision, Combat, MatchView};
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::{Interpolated, LocalTimeline, Predicted};

use crate::Anim;
use crate::outro::{Outros, Playing};
use crate::render::{MODEL_SCALE, Model, model_yaw};

/// Model 87's scale (conker-rules' `heist::MODEL`; C).
const SCALE: f32 = 0.4;
/// The carrier's right hand.
const HAND: &str = "joint_09";
/// Each logical clip's glTF animation (its pair; C: the pack's `logicalIds`): 0, 9 and 10 share
/// pair 0, 5 and 6 pair 6.
const PAIRS: [usize; 11] = [0, 1, 3, 2, 4, 6, 6, 7, 8, 0, 0];
/// Each pair's length in ticks (the glTF's `durationTicks`).
const TICKS: [f32; 9] = [26.0, 46.0, 21.0, 28.0, 58.0, 15.0, 34.0, 40.0, 20.0];
/// The shadow on the floor: alpha 0xA0 (V) and a radius (I: its size is not read; the bag's body
/// is 29).
const SHADOW: f32 = 24.0;
const SHADOW_ALPHA: f32 = 160.0 / 255.0;

/// The bag's logical clip and time (ticks) `k` ticks into its state (`CashView::state`: 0
/// standing, 1 carried, 2 thrown, 3 let go of, 4 sliding, 5 walking), carried by a player in clip
/// `carrier` (id, time); `slid`: ticks since its slide started, for a stand still untouchable
/// after it. V `heist-presentation-scratch/bag/bag_timeline.txt` (h1, h2, h3).
pub fn clip(state: u8, k: u32, carrier: Option<(u16, f32)>, slid: Option<u32>) -> (u8, f32) {
    let wrap = |c: u8, t: f32| (c, t % TICKS[PAIRS[usize::from(c)]]);
    match (state, carrier) {
        // The grab: clip 6 where it lies, at the hand 10 in with clip 5 at 1, then held at 8;
        // then the carrier's idle (6), walk (7) and run (8) at their times, 6 for anything else
        // (the throw, a hit).
        (1, Some((GRAB, g))) if g < 9.5 => (6, g),
        (1, Some((GRAB, g))) if g < 10.5 => (5, 1.0),
        (1, Some((GRAB, _))) => (5, 8.0),
        (1, Some((WALK, t))) => wrap(7, t),
        (1, Some((RUN, t))) => wrap(8, t),
        (1, Some((_, t))) => wrap(6, t),
        (1, None) => (6, 0.0),
        // Thrown: clip 9 from 1, held at 8; landed: 9 at 9, then 10 looping, through the stand
        // while it can't be touched.
        (2, _) => (9, (k + 1).min(8) as f32),
        (4, _) if k == 0 => (9, 9.0),
        (4, _) => wrap(10, k as f32),
        (0, _) if slid.is_some() => wrap(10, slid.unwrap_or(0) as f32),
        // Let go of: clip 7, then 8 at rate 0.8.
        (3, _) if k == 0 => (7, 1.0),
        (3, _) => (8, 0.8 * k as f32),
        // Walking home at rate 1.3.
        (5, _) => wrap(3, 1.3 * (k + 1) as f32),
        // Standing: clip 0 for 100 frames, then 4 once (57), over and over (C; V).
        _ => match k % 157 {
            c if c < 100 => wrap(0, (c + 1) as f32),
            c => (4, (c - 99) as f32),
        },
    }
}

/// The bag this tick ([`advance`]): its state and since as last seen and the tick this client saw
/// them, the tick it saw a slide start, and where it last lay (position, facing).
#[derive(Resource, Default)]
pub struct Bag {
    seen: Option<(u8, u32, u32)>,
    slide: Option<u32>,
    lay: ([f32; 3], u16),
    /// Its clip and time this tick and last (`None`: not shown).
    pub clip: Option<(u8, f32)>,
    pub was: Option<(u8, f32)>,
    /// Where it is drawn free (position, facing), or the carrier's slot whose hand holds it.
    pub at: ([f32; 3], u16),
    pub hand: Option<u16>,
    /// Its state this tick and last (the throw's sound: carried, then thrown).
    pub state: Option<u8>,
    pub was_state: Option<u8>,
}

/// The bag's model and its shadow.
#[derive(Component)]
struct Model87 {
    gltf: Handle<Gltf>,
    animator: Option<(Entity, Vec<AnimationNodeIndex>)>,
}

#[derive(Component)]
struct Shadow;

pub fn build(app: &mut App, windowed: bool) {
    app.init_resource::<Bag>();
    app.add_systems(
        FixedUpdate,
        advance.after(crate::tick_anims).run_if(not(is_in_rollback)),
    );
    if windowed {
        app.add_systems(Update, (add, draw).chain());
    }
}

/// The bag's clip this tick, from the match's bag and its carrier's clip.
#[allow(clippy::type_complexity)]
pub fn advance(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    players: Query<(&Combat, &Anim), Or<(With<Predicted>, With<Interpolated>)>>,
    mut bag: ResMut<Bag>,
) {
    let tick = timeline.tick().0;
    (bag.was, bag.was_state) = (bag.clip, bag.state);
    let cash = game.iter().find(|m| m.heist).map(|m| m.cash);
    let Some(c) = cash.filter(|c| c.shown) else {
        (bag.clip, bag.state, bag.hand, bag.seen) = (None, None, None, None);
        return;
    };
    let at = match bag.seen {
        Some((s, since, at)) if (s, since) == (c.state, c.since) => at,
        _ => {
            bag.seen = Some((c.state, c.since, tick));
            debug!(
                "BAG t={tick} state={} pos={:?} carrier={:?}",
                c.state, c.pos, c.carrier
            );
            tick
        }
    };
    match c.state {
        4 => bag.slide = Some(at),
        0 if !c.touchable => {}
        _ => bag.slide = None,
    }
    let slid = (bag.slide)
        .filter(|_| c.state == 0)
        .map(|s| tick.saturating_sub(s));
    let carrier = (players.iter())
        .find(|(p, _)| Some(p.slot) == c.carrier)
        .map(|(_, a)| (a.0.id, a.0.time));
    let clip = clip(c.state, tick.saturating_sub(at), carrier, slid);
    if c.state != 1 {
        bag.lay = (c.pos, c.yaw);
    }
    let lying = matches!(carrier, Some((GRAB, g)) if g < 9.5);
    bag.hand = c.carrier.filter(|_| c.state == 1 && !lying);
    bag.at = if c.state == 1 {
        bag.lay
    } else {
        (c.pos, c.yaw)
    };
    (bag.clip, bag.state) = (Some(clip), Some(c.state));
}

/// In War: Heist, the bag's model and shadow, hidden.
fn add(
    game: Query<&MatchView>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if *added || !game.iter().any(|m| m.heist) {
        return;
    }
    *added = true;
    let gltf = character_gltf(&MODEL.to_string());
    let model = Model87 {
        gltf: assets.load(&gltf),
        animator: None,
    };
    let scene = WorldAssetRoot(assets.load(format!("{gltf}#Scene0")));
    (commands.spawn((model, scene, Transform::default(), Visibility::Hidden))).observe(ready);
    commands.spawn((
        Shadow,
        Mesh3d(meshes.add(Circle::new(SHADOW))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK.with_alpha(SHADOW_ALPHA),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// The bag's scene is in: its `AnimationPlayer` gets every clip.
fn ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    mut models: Query<&mut Model87>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let Ok(mut model) = models.get_mut(ready.entity) else {
        return;
    };
    let animator = (children.iter_descendants(ready.entity)).find(|&e| animators.contains(e));
    let handles = gltfs.get(&model.gltf).map(|g| g.animations.clone());
    let (Some(animator), Some(handles)) = (animator, handles.filter(|h| h.len() >= TICKS.len()))
    else {
        warn!("bag: no animation player or too few clips");
        return;
    };
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    (commands.entity(animator)).insert(AnimationGraphHandle(graphs.add(graph)));
    model.animator = Some((animator, nodes));
}

/// The bag where and as [`Bag`] has it: free at its place over its shadow, or in its carrier's
/// hand, posed at its clip's time.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    bag: Res<Bag>,
    world: Res<Collision>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    players: Query<(Entity, &Combat), With<Model>>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut model: Query<
        (
            Entity,
            &Model87,
            &mut Transform,
            &mut Visibility,
            Option<&ChildOf>,
        ),
        Without<Shadow>,
    >,
    mut shadow: Query<(&mut Transform, &mut Visibility), With<Shadow>>,
    mut animators: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    let (Ok((entity, m, mut transform, mut visibility, parent)), Ok(mut s)) =
        (model.single_mut(), shadow.single_mut())
    else {
        return;
    };
    let clip = bag.clip.filter(|_| playing.of(&outros).is_none());
    let hand = bag.hand.and_then(|slot| {
        let (player, _) = players.iter().find(|(_, c)| c.slot == slot)?;
        (children.iter_descendants(player))
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == HAND))
    });
    let shown = clip.is_some() && (bag.hand.is_none() || hand.is_some());
    let show = |shown: bool| match shown {
        true => Visibility::Inherited,
        false => Visibility::Hidden,
    };
    visibility.set_if_neq(show(shown));
    s.1.set_if_neq(show(shown && bag.state != Some(1)));
    let (pos, yaw) = bag.at;
    match hand {
        Some(hand) => {
            if parent.map(ChildOf::parent) != Some(hand) {
                commands.entity(entity).insert(ChildOf(hand));
            }
            // The hand's scale (the model's) is not the bag's.
            *transform = Transform::from_scale(Vec3::splat(SCALE / MODEL_SCALE));
        }
        None => {
            if parent.is_some() {
                commands.entity(entity).remove::<ChildOf>();
            }
            *transform = Transform::from_translation(Vec3::from_array(pos))
                .with_rotation(Quat::from_rotation_y(model_yaw(yaw)))
                .with_scale(Vec3::splat(SCALE));
            let [x, y, z] = pos;
            let floor = world.0.floor(x, z, -1e9, y + 5.0, None).map_or(y, |f| f.y);
            *s.0 = Transform::from_xyz(x, floor + 1.0, z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        }
    }
    let (Some((c, time)), Some((animator, nodes))) = (clip, &m.animator) else {
        return;
    };
    let Ok(mut animator) = animators.get_mut(*animator) else {
        return;
    };
    let node = nodes[PAIRS[usize::from(c)]];
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

#[cfg(test)]
mod tests {
    use super::*;
    use conker_rules::heist::{IDLE, THROW};

    /// The bag's clips against the oracle's (`bag/bag_timeline.txt`, reads 328-640; k from each
    /// state's first read).
    #[test]
    fn bag_clips_follow_the_oracle() {
        let near = |a: (u8, f32), b: (u8, f32)| a.0 == b.0 && (a.1 - b.1).abs() < 1e-4;
        let cases = [
            // Arrived at 518: clip 0 from 1, wrapping at 26 (543); clip 4 once from 100.
            (clip(0, 0, None, None), (0, 1.0)),
            (clip(0, 24, None, None), (0, 25.0)),
            (clip(0, 25, None, None), (0, 0.0)),
            (clip(0, 100, None, None), (4, 1.0)),
            (clip(0, 156, None, None), (4, 57.0)),
            (clip(0, 157, None, None), (0, 1.0)),
            // Touched at 334: the grab's 1-9 (343: clip 5 at 1, then 8), idle from 361.
            (clip(1, 0, Some((GRAB, 1.0)), None), (6, 1.0)),
            (clip(1, 8, Some((GRAB, 9.0)), None), (6, 9.0)),
            (clip(1, 9, Some((GRAB, 10.0)), None), (5, 1.0)),
            (clip(1, 10, Some((GRAB, 11.0)), None), (5, 8.0)),
            (clip(1, 25, Some((GRAB, 26.0)), None), (5, 8.0)),
            (clip(1, 27, Some((IDLE, 1.0)), None), (6, 1.0)),
            (clip(1, 46, Some((IDLE, 20.0)), None), (6, 20.0)),
            // The throw from 381, the release at 395, landing at 411, sliding from 412.
            (clip(1, 47, Some((THROW, 1.0)), None), (6, 1.0)),
            (clip(1, 60, Some((THROW, 14.0)), None), (6, 14.0)),
            (clip(2, 0, None, None), (9, 1.0)),
            (clip(2, 7, None, None), (9, 8.0)),
            (clip(2, 15, None, None), (9, 8.0)),
            (clip(4, 0, None, None), (9, 9.0)),
            (clip(4, 1, None, None), (10, 1.0)),
            (clip(4, 25, None, None), (10, 25.0)),
            (clip(4, 26, None, None), (10, 0.0)),
            // Still sliding's clip at 452 (41 in), untouchable; walking from 453 at 1.3.
            (clip(0, 18, None, Some(41)), (10, 15.0)),
            (clip(5, 0, None, None), (3, 1.3)),
            (clip(5, 9, None, None), (3, 13.0)),
            // Let go of on a hit at 507: clip 7, then 8 at 0.8 (508, 509).
            (clip(3, 0, None, None), (7, 1.0)),
            (clip(3, 1, None, None), (8, 0.8)),
            (clip(3, 2, None, None), (8, 1.6)),
            // Carried walking and running: the carrier's time round the bag's clip.
            (clip(1, 0, Some((WALK, 41.0)), None), (7, 1.0)),
            (clip(1, 0, Some((RUN, 5.0)), None), (8, 5.0)),
        ];
        for (n, (got, want)) in cases.into_iter().enumerate() {
            assert!(near(got, want), "case {n}: {got:?}, not {want:?}");
        }
    }
}
