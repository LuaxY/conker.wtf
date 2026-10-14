//! War: Raptor (`../lab/research/mp-original/raptor.md`; this batch's oracle runs in
//! `raptor-client-scratch/`): the raptors (model 83, the pack's `characters/83`) at their scale
//! ([`SCALE`]) and clip rates ([`rate`]), their camera ([`camera`]); the match's things, from
//! `maps/3f/raptor.json` and [`RaptorView`]:
//! - a body (22) in a raptor's jaws while it holds one: joint 6's origin, turned with the raptor
//!   (V r2), playing its clip 12; a dropped one where the match has it (clip 13) until it goes;
//! - the baby (165, on 83's clips) on the floor at its record, and its feed ([`baby_clip`]), with
//!   the fed body's way from the raptor's jaws into the baby's ([`fed_body`]);
//! - the three eggs (171) where the match has them, or in their carrier's hands (`HELD`: 40 ahead,
//!   38 up);
//! - the cooked eggs (placements `cooked`) shown in turn where an egg splats, with a splash of
//!   puffs ([`SPLAT`]), and the placements the mode hides (`hidden`).
//!
//! Everything hides while an outro plays. Clip clocks are this client's: a feed counts from the
//! tick this client saw it, a drop too; what was already there at first sight counts as done.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::character_gltf;
use conker_assets::raptor::Raptor;
use conker_rules::Fighter;
use conker_rules::raptor::{DROPPED, EATS, EGG_IDLE, EGG_RUN, EGG_WALK, HELD};
use conker_sim::collision::CollisionWorld;
use conker_sim::follow::Follow;
use conker_sim::net::{Collision, Combat, MatchView};
use conker_sim::raptor::{TOP, clip};
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::LocalTimeline;

use crate::outro::{Outros, Playing};
use crate::render::{MODEL_SCALE, Model, model_yaw};
use crate::tank::{Behind, Cam};
use crate::{Anim, Clips};

/// The raptor's character folder: a player's body on 3f ([`crate::characters`]).
pub const MODEL: &str = "83";
/// Model units to world (V obj+0x14C, the pack's `raptor.scale`); no model lift.
pub const SCALE: f32 = 0.7;
/// The baby's and the body's character folders (`raptor.json`'s records: their clip sounds,
/// [`crate::audio`]).
pub const BABY: &str = "165";
pub const BODY: &str = "22";
/// An egg's splat's sounds (C script 0xBE 0x476A-0x48C4).
pub const SPLAT_SOUNDS: [u16; 2] = [0x1A6, 0x434];
/// The joint a held body sits on (`raptor.carry`; V r2), the raptor's and the baby's.
const JAW: &str = "joint_06";
/// The body's clips (22 has no `character.json`: its glTF's names, loop lengths): held in the
/// jaws (12), dropped (13), falling from the jaws into the nest (7) (V r3).
const BODY_CLIPS: [(u16, &str); 3] = [
    (12, "pair_0012_anim_0012"),
    (13, "pair_0013_anim_0013"),
    (7, "pair_0007_anim_0007"),
];
const HELD_CLIP: u16 = 12;
const DROP_CLIP: u16 = 13;
const FALL_CLIP: u16 = 7;
const BODY_TICKS: [(u16, u32); 3] = [(12, 11), (13, 28), (7, 37)];
/// The baby's idle (pair 0, as the raptor's 0) and its feed (V r3, the nest drop at 152): clip 10
/// for a frame from 37, 7 from 38, 8 from 60 (it holds the body from 49), eating (56) from 99 to
/// [`EATS`].
const BABY_IDLE: u16 = 55;
const BABY_FEED: [(u32, u16); 4] = [(37, 10), (38, 7), (60, 8), (99, 56)];
/// The fed body (V r3): in the raptor's jaws for 24 frames, falling to the baby's for 25, in them
/// from 49 (clip 13 from 60), eaten at 99.
const FED_FALL: u32 = 24;
const FED_BABY: u32 = 49;
const FED_DROP: u32 = 60;
const FED_GONE: u32 = 99;
/// The splat's effect (0xE02F, its look not read): a few small puffs (I).
const SPLAT: (u32, f32, u32) = (8, 40.0, 0);

/// The raptor's clip rates (V r1, r8: obj+0x4C): idle 0.5 a frame (1 its first), braking 0.1 +
/// speed / 14, running 0.5 + speed / 43.4, the bites, the jump, the drop and the respawn's 0.8, the
/// rest 1 (I the hurts 12 and 23).
pub fn rate(id: u16, speed: f32, first: bool) -> f32 {
    match id {
        clip::IDLE if first => 1.0,
        clip::IDLE => 0.5,
        clip::SLOW => 0.1 + speed / 14.0,
        clip::RUN => 0.5 + speed / 43.4,
        clip::BITE | clip::JUMP | clip::RUN_BITE | clip::DROP | clip::RESPAWN => 0.8,
        _ => 1.0,
    }
}

/// An egg carrier's rate (V r4): idle 1, walking e / 6 + 0.4 and running e / 16 + 0.5 of the
/// effective speed (the target capped at 25.2, as Heist's carrier's); `None` for its other clips.
pub fn carry_rate(id: u16, speed: f32, target: f32) -> Option<f32> {
    let e = conker_anim::effective_speed(speed, target.min(25.2));
    match id {
        EGG_IDLE => Some(1.0),
        EGG_WALK => Some(e / 6.0 + 0.4),
        EGG_RUN => Some(e / 16.0 + 0.5),
        _ => None,
    }
}

/// The raptor's camera (V r1): 340 behind and 150 over its feet at rest, looking at 129 over them;
/// at full speed ([`TOP`]) 410, 146 and 124, the distance a frame or two behind the speed; the
/// heading follows the facing by 7.5 % of the gap a frame (9 % at full speed), at least 0.25° (I,
/// fitted to r1: 63-85° behind turning at 4-5.6° a frame). Not modelled: its rise to ~225 while
/// turning, its lower look in a jump.
pub const CAM: Behind = Behind {
    rest: [340.0, 150.0, 129.0],
    full: [410.0, 146.0, 124.0],
    top: TOP,
    share: 0.5,
    follow: [0.075, 0.015, 0.25],
};

/// One frame of a raptor's view ([`crate::tank::behind`] with [`CAM`]). The raptor's stick is the
/// camera's (`target_angle` adds the input's camera yaw, [`Follow::yaw`]): its yaw is the heading's.
pub fn camera(follow: &mut Follow, cam: &mut Cam, f: &Fighter, world: &CollisionWorld) {
    follow.cam.scope.on = false;
    crate::tank::behind(follow, cam, &f.body, &CAM, Some(world));
    if let Some((heading, _)) = cam.0 {
        follow.cam.yaw = (heading - 90.0).rem_euclid(360.0);
        follow.cam.stick_override = conker_camera::NO_OVERRIDE;
    }
}

/// The baby's clip and time `k` frames into a feed (`None`: none), else its idle at `idle` frames
/// (its clock at 0.5, the raptor's idle's). Each feed clip from 1 at 1 a frame (I), looping.
pub fn baby_clip(k: Option<u32>, idle: u32, clips: &Clips) -> (u16, f32) {
    let wrap = |c: u16, t: f32| (c, t % f32::from(clips.duration(c)));
    let feed = k
        .filter(|&k| k < EATS)
        .and_then(|k| BABY_FEED.iter().rev().find(|f| f.0 <= k).map(|f| (k, f)));
    match feed {
        Some((k, &(from, c))) => wrap(c, (k - from + 1) as f32),
        None => wrap(BABY_IDLE, 0.5 * idle as f32),
    }
}

/// Where the fed body is `k` frames into a feed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fed {
    /// In the feeding raptor's jaws.
    Jaws,
    /// This share of its fall from them to the baby's (I: a straight line).
    Falling(f32),
    /// In the baby's jaws.
    Baby,
}

/// The fed body `k` frames into a feed: where, its clip and time; `None` eaten.
pub fn fed_body(k: u32) -> Option<(Fed, u16, f32)> {
    let wrap = |c: u16, t: u32| {
        let n = BODY_TICKS.iter().find(|b| b.0 == c).map_or(1, |b| b.1);
        (c, (t % n) as f32)
    };
    let (at, (c, t)) = match k {
        k if k < FED_FALL => (Fed::Jaws, wrap(HELD_CLIP, k + 1)),
        k if k < FED_BABY => {
            let share = (k - FED_FALL + 1) as f32 / (FED_BABY - FED_FALL) as f32;
            (Fed::Falling(share), wrap(FALL_CLIP, k - FED_FALL + 1))
        }
        k if k < FED_DROP => (Fed::Baby, wrap(HELD_CLIP, k - FED_BABY + 1)),
        k if k < FED_GONE => (Fed::Baby, wrap(DROP_CLIP, k - FED_DROP + 1)),
        _ => return None,
    };
    Some((at, c, t))
}

/// War: Raptor's state on this client: `raptor.json`, the baby's clips (83's), the feed and drops
/// as first seen, the cooked eggs last seen; this tick's baby clip, fed body and splats (for the
/// sound, [`crate::audio`]).
#[derive(Resource)]
pub struct Nest {
    pub data: Raptor,
    clips: Clips,
    seen: bool,
    fed: Option<(u32, u32)>,
    drops: Vec<(u32, u32)>,
    cooked: [Option<[f32; 3]>; 4],
    /// The baby's clip and time this tick and last.
    pub baby: Option<(u16, f32)>,
    pub baby_was: Option<(u16, f32)>,
    /// Frames into the feed while its body is shown.
    pub fed_k: Option<u32>,
    /// The fed body's clip and time this tick and last.
    pub body: Option<(u16, f32)>,
    pub body_was: Option<(u16, f32)>,
    /// Where an egg splatted this tick.
    pub splats: Vec<[f32; 3]>,
}

impl Nest {
    /// A placement the mode draws ([`props`]): the cooked eggs and the hidden ones.
    pub fn owns(&self, placement: u32) -> bool {
        u16::try_from(placement).is_ok_and(|p| self.data.hidden.contains(&p))
    }

    /// Frames since a drop of match tick `tick`, as this client saw it.
    fn dropped(&self, tick: u32, now: u32) -> Option<u32> {
        let seen = self.drops.iter().find(|d| d.0 == tick)?.1;
        Some(now.saturating_sub(seen))
    }
}

pub fn build(app: &mut App, windowed: bool, raptor: Option<Raptor>, clips: Clips) {
    let Some(data) = raptor else {
        return;
    };
    app.insert_resource(Nest {
        data,
        clips,
        seen: false,
        fed: None,
        drops: Vec::new(),
        cooked: [None; 4],
        baby: None,
        baby_was: None,
        fed_k: None,
        body: None,
        body_was: None,
        splats: Vec::new(),
    });
    app.add_systems(
        FixedUpdate,
        advance.after(crate::tick_anims).run_if(not(is_in_rollback)),
    );
    if windowed {
        app.add_systems(FixedUpdate, splat.after(advance));
        app.add_systems(
            Update,
            (add, draw_baby, draw_eggs, draw_bodies, props, hide_bitten).chain(),
        );
    }
}

/// The tick's feed, drops and splats from the match.
pub fn advance(game: Query<&MatchView>, timeline: Res<LocalTimeline>, nest: Option<ResMut<Nest>>) {
    let (Some(mut n), Some(m)) = (nest, game.iter().find(|m| m.raptor)) else {
        return;
    };
    let tick = timeline.tick().0;
    let r = &m.raptors;
    // At first sight a feed or drop is over, the cooked eggs already there.
    let first = !n.seen;
    n.seen = true;
    n.fed = match (r.fed, n.fed) {
        (Some(f), Some(seen)) if seen.0 == f => Some(seen),
        (Some(f), _) if first => Some((f, tick.saturating_sub(EATS))),
        (Some(f), _) => {
            debug!("FEED t={tick} fed={f}");
            Some((f, tick))
        }
        (None, _) => None,
    };
    let old = std::mem::take(&mut n.drops);
    n.drops = (r.drops.iter())
        .map(|d| match old.iter().find(|o| o.0 == d.2) {
            Some(&o) => o,
            None if first => (d.2, tick.saturating_sub(DROPPED)),
            None => (d.2, tick),
        })
        .collect();
    n.splats = (0..4)
        .filter(|&k| !first && r.cooked[k] != n.cooked[k])
        .filter_map(|k| r.cooked[k])
        .collect();
    for at in &n.splats {
        debug!("SPLAT t={tick} at={at:?}");
    }
    n.cooked = r.cooked;
    let k = n.fed.map(|(_, at)| tick.saturating_sub(at));
    n.baby_was = n.baby;
    n.baby = Some(baby_clip(k, tick, &n.clips));
    n.fed_k = k.filter(|&k| k < FED_GONE);
    n.body_was = n.body;
    n.body = n.fed_k.and_then(fed_body).map(|b| (b.1, b.2));
}

/// An egg's splat: [`SPLAT`]'s puffs where it landed.
fn splat(
    nest: Res<Nest>,
    sprites: Option<Res<crate::beach::Sprites>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
    mut commands: Commands,
) {
    let Some(sprites) = sprites else {
        return;
    };
    for &at in &nest.splats {
        let at = Vec3::from_array(at);
        crate::beach::burst(
            &mut commands,
            &mut materials,
            &sprites,
            &mut seed,
            at,
            SPLAT,
        );
    }
}

/// A model posed by its clips (by id: its glTF's animation names), once its scene is in.
#[derive(Component)]
struct Posed {
    gltf: Handle<Gltf>,
    clips: Vec<(u16, String)>,
    animator: Option<(Entity, Vec<(u16, AnimationNodeIndex)>)>,
}

#[derive(Component)]
struct Baby;

#[derive(Component)]
struct Egg(usize);

/// One of the bodies drawn (in the jaws, dropped, fed).
#[derive(Component)]
struct Body;

/// The baby, the eggs and the body's glTF, once a Raptor match is on.
fn add(
    game: Query<&MatchView>,
    nest: Res<Nest>,
    world: Res<Collision>,
    assets: Res<AssetServer>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if *added || !game.iter().any(|m| m.raptor) {
        return;
    }
    *added = true;
    let d = &nest.data;
    let b = &d.baby;
    let gltf = character_gltf(&b.character);
    let [x, y, z] = b.at.map(f32::from);
    // On the floor under or over its record (V r3: y −10).
    let y = (world.0.floor(x, z, -1e9, y + 200.0, None)).map_or(y, |f| f.y);
    let baby = Posed {
        gltf: assets.load(&gltf),
        clips: (nest.clips.0.iter()).map(|c| (c.0, c.1.clone())).collect(),
        animator: None,
    };
    commands
        .spawn((
            Baby,
            baby,
            WorldAssetRoot(assets.load(format!("{gltf}#Scene0"))),
            Transform::from_xyz(x, y, z)
                .with_rotation(Quat::from_rotation_y(model_yaw(u16::from(b.yaw) << 8)))
                .with_scale(Vec3::splat(b.scale)),
            Visibility::Hidden,
        ))
        .observe(ready);
    for (i, e) in d.eggs.iter().enumerate() {
        let scene = format!("{}#Scene0", character_gltf(&e.character));
        commands.spawn((
            Egg(i),
            WorldAssetRoot(assets.load(scene)),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// A posed model's scene is in: its `AnimationPlayer` gets the clips its glTF has.
fn ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    mut models: Query<&mut Posed>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let Ok(mut model) = models.get_mut(ready.entity) else {
        return;
    };
    let animator = (children.iter_descendants(ready.entity)).find(|&e| animators.contains(e));
    let (Some(animator), Some(g)) = (animator, gltfs.get(&model.gltf)) else {
        warn!("raptor: a model without an animation player");
        return;
    };
    let (ids, handles): (Vec<u16>, Vec<_>) = (model.clips.iter())
        .filter_map(|c| Some((c.0, g.named_animations.get(c.1.as_str())?.clone())))
        .unzip();
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    (commands.entity(animator)).insert(AnimationGraphHandle(graphs.add(graph)));
    model.animator = Some((animator, ids.into_iter().zip(nodes).collect()));
}

/// Shows a posed model's clip at its time (ticks).
fn pose(model: &Posed, animators: &mut Query<&mut AnimationPlayer>, clip: u16, time: f32) {
    let Some((animator, nodes)) = &model.animator else {
        return;
    };
    let (Ok(mut animator), Some(&(_, node))) = (
        animators.get_mut(*animator),
        nodes.iter().find(|n| n.0 == clip),
    ) else {
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

fn show(shown: bool) -> Visibility {
    match shown {
        true => Visibility::Inherited,
        false => Visibility::Hidden,
    }
}

/// The joint named `name` under `entity`.
fn joint(
    entity: Entity,
    name: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    (children.iter_descendants(entity)).find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
}

/// The baby at its clip ([`Nest::baby`]).
fn draw_baby(
    nest: Res<Nest>,
    game: Query<&MatchView>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    mut baby: Query<(&Posed, &mut Visibility), With<Baby>>,
    mut animators: Query<&mut AnimationPlayer>,
) {
    let Ok((model, mut v)) = baby.single_mut() else {
        return;
    };
    let shown = game.iter().any(|m| m.raptor) && playing.of(&outros).is_none();
    v.set_if_neq(show(shown));
    if let Some((c, t)) = nest.baby.filter(|_| shown) {
        pose(model, &mut animators, c, t);
    }
}

/// The eggs where the match has them, a carried one in its carrier's hands.
#[allow(clippy::type_complexity)]
fn draw_eggs(
    nest: Res<Nest>,
    game: Query<&MatchView>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    players: Query<(Entity, &Combat), With<Model>>,
    mut eggs: Query<(
        Entity,
        &Egg,
        &mut Transform,
        &mut Visibility,
        Option<&ChildOf>,
    )>,
    mut commands: Commands,
) {
    let m = game.iter().find(|m| m.raptor);
    let outro = playing.of(&outros).is_some();
    for (entity, egg, mut t, mut v, parent) in &mut eggs {
        let Some((shown, carrier, pos)) = m.map(|m| m.raptors.eggs[egg.0]) else {
            v.set_if_neq(Visibility::Hidden);
            continue;
        };
        let holder = carrier.and_then(|s| players.iter().find(|p| p.1.slot == s).map(|p| p.0));
        v.set_if_neq(show(
            shown && !outro && (carrier.is_none() || holder.is_some()),
        ));
        let record = &nest.data.eggs[egg.0];
        let scale = Vec3::splat(record.scale);
        let pose = match holder {
            Some(h) => {
                if parent.map(ChildOf::parent) != Some(h) {
                    commands.entity(entity).insert(ChildOf(h));
                }
                // In the world's units under the caveman's model scale.
                Transform::from_translation(Vec3::new(0.0, HELD[1], HELD[0]) / MODEL_SCALE)
                    .with_scale(scale / MODEL_SCALE)
            }
            None => {
                if parent.is_some() {
                    commands.entity(entity).remove::<ChildOf>();
                }
                let yaw = model_yaw(u16::from(record.yaw) << 8);
                Transform::from_translation(Vec3::from_array(pos))
                    .with_rotation(Quat::from_rotation_y(yaw))
                    .with_scale(scale)
            }
        };
        t.set_if_neq(pose);
    }
}

/// Where a body is drawn: under a player at an offset (turned with it), or in the world.
#[derive(Clone, Copy)]
enum At {
    Under(Entity, Vec3),
    World(Vec3, Quat),
}

/// The bodies: in each raptor's jaws while it holds one, the fed one on its way ([`fed_body`]),
/// the dropped ones; a pool of models, the spare ones hidden.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_bodies(
    nest: Res<Nest>,
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    assets: Res<AssetServer>,
    players: Query<(Entity, &Combat, &Anim, &GlobalTransform), With<Model>>,
    baby: Query<(Entity, &GlobalTransform), With<Baby>>,
    children: Query<&Children>,
    names: Query<&Name>,
    globals: Query<&GlobalTransform>,
    mut bodies: Query<
        (
            Entity,
            &Posed,
            &mut Transform,
            &mut Visibility,
            Option<&ChildOf>,
        ),
        With<Body>,
    >,
    mut animators: Query<&mut AnimationPlayer>,
    mut pool: Local<Vec<Entity>>,
    mut jaws_at: Local<Option<Vec3>>,
    mut commands: Commands,
) {
    let Some(m) = game.iter().find(|m| m.raptor) else {
        return;
    };
    let tick = timeline.tick().0;
    let record = &nest.data.body;
    // A jaw's place under its player (last frame's pose).
    let jaw = |e: Entity, g: &GlobalTransform| {
        let j = globals.get(joint(e, JAW, &children, &names)?).ok()?;
        Some((
            g.affine().inverse().transform_point3(j.translation()),
            j.translation(),
        ))
    };
    let mut drawn: Vec<(At, u16, f32)> = Vec::new();
    let held = (tick % 11) as f32;
    for (e, c, _, g) in &players {
        if m.raptors.prey >> c.slot & 1 != 0
            && let Some((local, _)) = jaw(e, g)
        {
            drawn.push((At::Under(e, local), HELD_CLIP, held));
        }
    }
    let baby_jaw = (baby.single().ok()).and_then(|(e, g)| {
        let j = globals.get(joint(e, JAW, &children, &names)?).ok()?;
        Some((j.translation(), g.rotation()))
    });
    if let Some((at, c, t)) = nest.fed_k.and_then(fed_body) {
        let feeder = (players.iter())
            .find(|p| p.1.class == conker_rules::raptor::CLASS && p.2.0.id == clip::NEST);
        match at {
            Fed::Jaws => {
                if let Some((e, local, world)) =
                    feeder.and_then(|(e, _, _, g)| jaw(e, g).map(|j| (e, j.0, j.1)))
                {
                    *jaws_at = Some(world);
                    drawn.push((At::Under(e, local), c, t));
                }
            }
            Fed::Falling(share) => {
                if let Some((to, turn)) = baby_jaw {
                    let from = jaws_at.unwrap_or(to + Vec3::Y * 300.0);
                    drawn.push((At::World(from.lerp(to, share), turn), c, t));
                }
            }
            Fed::Baby => {
                if let Some((to, turn)) = baby_jaw {
                    drawn.push((At::World(to, turn), c, t));
                }
            }
        }
    }
    for &(pos, facing, at) in &m.raptors.drops {
        let k = nest.dropped(at, tick).unwrap_or(0);
        let turn = Quat::from_rotation_y(model_yaw(facing));
        let t = (k + 1) % BODY_TICKS[1].1;
        drawn.push((At::World(Vec3::from_array(pos), turn), DROP_CLIP, t as f32));
    }
    if playing.of(&outros).is_some() {
        drawn.clear();
    }
    let gltf = character_gltf(&record.character);
    while pool.len() < drawn.len() {
        let model = Posed {
            gltf: assets.load(&gltf),
            clips: BODY_CLIPS.map(|c| (c.0, c.1.to_string())).to_vec(),
            animator: None,
        };
        let scene = WorldAssetRoot(assets.load(format!("{gltf}#Scene0")));
        let body = (Body, model, scene, Transform::default(), Visibility::Hidden);
        pool.push(commands.spawn(body).observe(ready).id());
    }
    for (i, &e) in pool.iter().enumerate() {
        let Ok((entity, model, mut t, mut v, parent)) = bodies.get_mut(e) else {
            continue;
        };
        let Some(&(at, c, time)) = drawn.get(i) else {
            v.set_if_neq(Visibility::Hidden);
            continue;
        };
        v.set_if_neq(Visibility::Inherited);
        let (under, place) = match at {
            At::Under(p, local) => (Some(p), Transform::from_translation(local)),
            At::World(pos, turn) => (None, Transform::from_translation(pos).with_rotation(turn)),
        };
        match under {
            Some(p) if parent.map(ChildOf::parent) != Some(p) => {
                commands.entity(entity).insert(ChildOf(p));
            }
            None if parent.is_some() => {
                commands.entity(entity).remove::<ChildOf>();
            }
            _ => {}
        }
        // Its own scale in the world, under a raptor's too.
        let scale = record.scale / if under.is_some() { SCALE } else { 1.0 };
        t.set_if_neq(place.with_scale(Vec3::splat(scale)));
        pose(model, &mut animators, c, time);
    }
}

/// A bitten caveman is not drawn: the body in the jaws stands in (V `raptor-client-scratch` run b22b:
/// 136's 160 vertices drawn at DLs 70 and 85, none from 95, the kill's frame, where 22 draws; b22:
/// none at 101 and 130).
fn hide_bitten(
    game: Query<&MatchView>,
    players: Query<(&Combat, &Anim, &Model)>,
    mut shown: Query<&mut Visibility>,
) {
    if !game.iter().any(|m| m.raptor) {
        return;
    }
    for (c, anim, model) in &players {
        if let Ok(mut v) = shown.get_mut(model.scene) {
            v.set_if_neq(
                match c.health == 0 && anim.0.id == conker_rules::damage::BITTEN {
                    true => Visibility::Hidden,
                    false => Visibility::Inherited,
                },
            );
        }
    }
}

/// The placements the mode owns ([`Nest::owns`]): a cooked egg where the match shows it, the rest
/// hidden.
fn props(
    nest: Res<Nest>,
    game: Query<&MatchView>,
    added: Query<(Entity, &Name), Added<Name>>,
    mut nodes: Local<Vec<(u32, Entity)>>,
    mut shown: Query<(&mut Transform, &mut Visibility), With<Name>>,
) {
    let found = (added.iter()).filter_map(|(e, n)| Some((n.rsplit_once('@')?.1.parse().ok()?, e)));
    nodes.extend(found.filter(|n: &(u32, Entity)| nest.owns(n.0)));
    let Some(m) = game.iter().find(|m| m.raptor) else {
        return;
    };
    let cooked = &nest.data.cooked;
    for &(placement, entity) in nodes.iter() {
        let Ok((mut t, mut v)) = shown.get_mut(entity) else {
            continue;
        };
        let k = cooked.iter().position(|&c| u32::from(c) == placement);
        match k.and_then(|k| m.raptors.cooked.get(k).copied().flatten()) {
            Some(pos) => {
                let to = Vec3::from_array(pos);
                if t.translation != to {
                    t.translation = to;
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

    fn pack_dir() -> std::path::PathBuf {
        std::env::var("CONKER_ASSETS").map_or_else(
            |_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets"),
            Into::into,
        )
    }

    /// 83's clips as the pack has them (`characters/83/character.json`), else the loop lengths the
    /// tests read: idle 42, 7 23, 8 32, 10 28, 56 46 (pairs 0, 5, 6, 7, 9).
    fn clips() -> Clips {
        match std::fs::read(pack_dir().join("characters/83/character.json")) {
            Ok(json) => {
                let c: conker_assets::Character = serde_json::from_slice(&json).unwrap();
                Clips::new(&c.clips)
            }
            Err(_) => Clips(
                [(7, 23), (8, 32), (10, 28), (55, 42), (56, 46)]
                    .map(|(c, n)| (c, String::new(), n))
                    .to_vec(),
            ),
        }
    }

    /// The pack agrees with this module: `raptor.json`'s scale, characters and carry joint; 22's
    /// clips by their glTF names (their logical ids and loop lengths); the floor under the baby's
    /// record at y −10, where the oracle's baby stands (V r3).
    #[test]
    fn the_pack_agrees() {
        let dir = pack_dir();
        let read = |p: &str| std::fs::read(dir.join(p));
        let Ok(r) = Raptor::load("3f", read) else {
            eprintln!("no pack maps/3f/raptor.json: skipping");
            return;
        };
        assert_eq!(r.raptor.scale, SCALE);
        assert_eq!(
            (r.baby.character.as_str(), r.body.character.as_str()),
            (BABY, BODY)
        );
        assert_eq!(format!("joint_{:02}", r.raptor.carry), JAW);
        let gltf: serde_json::Value =
            serde_json::from_slice(&read("characters/22/character.gltf").unwrap()).unwrap();
        let animations = gltf["animations"].as_array().unwrap();
        for ((id, name), (_, ticks)) in BODY_CLIPS.iter().zip(BODY_TICKS) {
            let a = animations.iter().find(|a| a["name"] == *name).unwrap();
            let ids = a["extras"]["logicalIds"].as_array().unwrap();
            assert!(ids.contains(&(*id).into()), "{name}");
            assert_eq!(a["extras"]["durationTicks"], ticks, "{name}");
        }
        let slice = conker_assets::Slice::load(
            "3f",
            &crate::characters("3f"),
            conker_assets::PACK_WEAPONS,
            read,
        )
        .unwrap();
        let world = conker_sim::net::scene_world(&slice, 0x3F);
        let [x, y, z] = r.baby.at.map(f32::from);
        let floor = world.floor(x, z, -1e9, y + 200.0, None).unwrap().y;
        assert!((floor + 10.0).abs() < 1.0, "{floor}");
    }

    /// The raptor's rates (V r1, r8) and the egg carrier's (V r4 262-330: walk 3.02 at speed 1.5,
    /// run 1.8013 at 14.25 and 1.0615 at 22.46 let go).
    #[test]
    fn rates_follow_the_oracle() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert_eq!(rate(clip::IDLE, 0.0, true), 1.0);
        assert_eq!(rate(clip::IDLE, 0.0, false), 0.5);
        assert!(near(rate(clip::RUN, 35.0, false), 0.5 + 35.0 * 0.023_041_5));
        assert!(near(rate(clip::SLOW, 14.0, false), 1.1));
        assert_eq!(rate(clip::RUN_BITE, 30.0, false), 0.8);
        assert_eq!(rate(clip::NEST, 0.0, false), 1.0);
        assert!(near(carry_rate(EGG_WALK, 1.5, 35.0).unwrap(), 3.02));
        assert!(near(carry_rate(EGG_RUN, 14.25, 35.0).unwrap(), 1.8013));
        assert!(near(carry_rate(EGG_RUN, 22.46, 0.0).unwrap(), 1.0615));
        assert_eq!(carry_rate(EGG_IDLE, 0.0, 0.0), Some(1.0));
        assert_eq!(carry_rate(clip::RUN, 0.0, 0.0), None);
    }

    /// The feed against r3 (nest drop at 152): the baby idle to 188, clip 10 at 189, 7 from 190,
    /// 8 from 212, 56 from 251, idle from 308; the body in the raptor's jaws (12) to 175, falling
    /// (7) from 176, in the baby's (12) from 201, 13 from 212 (the oracle's next frame), gone at
    /// 251.
    #[test]
    fn the_feed_follows_the_oracle() {
        let c = clips();
        let baby = |read: u32| baby_clip(Some(read - 152), 0, &c).0;
        assert_eq!(
            [188, 189, 190, 211, 212, 250, 251, 307, 308].map(baby),
            [55, 10, 7, 7, 8, 8, 56, 56, 55]
        );
        assert_eq!(baby_clip(Some(38), 0, &c), (7, 1.0));
        assert_eq!(baby_clip(None, 10, &c), (55, 5.0));
        let body = |read: u32| fed_body(read - 152).map(|b| (b.0, b.1));
        assert_eq!(body(175), Some((Fed::Jaws, 12)));
        assert!(matches!(body(176), Some((Fed::Falling(_), 7))));
        assert_eq!(body(200), Some((Fed::Falling(1.0), 7)));
        assert_eq!(body(201), Some((Fed::Baby, 12)));
        assert_eq!(body(212), Some((Fed::Baby, 13)));
        assert_eq!(body(250), Some((Fed::Baby, 13)));
        assert_eq!(body(251), None);
    }

    /// The raptor's camera: 340 behind and 150 over at rest, looking at 129 over its feet; at full
    /// speed, once caught up, 410, 146 and 124; its heading follows the facing; the stick reads the
    /// heading (forward along it: `Follow::yaw` is the facing less a quarter turn).
    #[test]
    fn the_camera_sits_behind_and_the_stick_reads_its_heading() {
        let world = CollisionWorld::layered(std::iter::empty());
        let place = conker_rules::spawn::Placement {
            index: 0,
            pos: [100.0, 0.0, 0.0],
            facing: 0x4000, // along −z
            camera: [0.0; 3],
        };
        let mut f = Fighter::new(&place, 0, 0);
        let mut follow = Follow::spawn([0.0; 3], &f.body);
        let mut cam = Cam::default();
        camera(&mut follow, &mut cam, &f, &world);
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 0.05);
        assert!(
            near(follow.cam.eye, [100.0, 150.0, 340.0]),
            "{:?}",
            follow.cam.eye
        );
        assert!(
            near(follow.cam.at, [100.0, 129.0, 0.0]),
            "{:?}",
            follow.cam.at
        );
        assert!(
            follow.yaw().abs_diff(0x4000 - 0x4000) <= 1,
            "{}",
            follow.yaw()
        );
        f.body.xz_speed = TOP;
        for _ in 0..30 {
            camera(&mut follow, &mut cam, &f, &world);
        }
        assert!(
            near(follow.cam.eye, [100.0, 146.0, 410.0]),
            "{:?}",
            follow.cam.eye
        );
        // Turned a quarter: the heading closes the gap, 7.5-9 % a frame.
        f.body.facing = 0;
        camera(&mut follow, &mut cam, &f, &world);
        let heading = cam.0.unwrap().0;
        assert!((heading - (90.0 - 90.0 * 0.09)).abs() < 0.01, "{heading}");
        let want = ((heading - 90.0) * 65536.0 / 360.0) as i32 as u16;
        assert!(follow.yaw().abs_diff(want) <= 1, "{} {want}", follow.yaw());
    }

    /// An egg carrier keeps the on-foot camera (V carry2 231-400, cameras-clips-g4-scratch/scope:
    /// mode 8, `s126+0x78` and `cam+0x23E` 0, head 90): at rest 247 behind and 100 over the feet,
    /// looking at 61.5 over them (0.75 × 90 and the nest floor's sink −6; carry2 246-258); R opens
    /// the unarmed aim, 90 from the look-at, lifted 25 (V carry3 354-380).
    #[test]
    fn the_egg_carrier_keeps_the_on_foot_camera() {
        let place = conker_rules::spawn::Placement {
            index: 0,
            pos: [-7247.095, 209.0, -3095.0],
            facing: 21760,
            camera: [0.0; 3],
        };
        let mut f = Fighter::new(&place, 0, 0);
        (f.class, f.flag) = (
            conker_rules::raptor::CAVEMAN,
            Some(conker_rules::raptor::EGG),
        );
        f.body.sink = -6;
        let p = crate::camera_view(&f, 0);
        assert_eq!((p.weapon, p.head), (0, 90.0));
        let mut cam = conker_camera::CameraState::spawn([0.0; 3], &p);
        let step = |cam: &mut conker_camera::CameraState, buttons, n| {
            for _ in 0..n {
                cam.step(&p, buttons, |_, _| Some(209.0));
            }
            let (e, a) = (cam.view_eye(), cam.view_at());
            let d = (e[0] - a[0]).hypot(e[2] - a[2]);
            [d, e[1] - 209.0, a[1] - 209.0]
        };
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 0.01);
        let rest = step(&mut cam, 0, 60);
        assert!(near(rest, [247.0, 100.0, 61.5]), "{rest:?}");
        let aimed = step(&mut cam, conker_camera::R, 40);
        assert!(cam.aim.on && cam.weapon == 0);
        assert!(near(aimed, [90.0, 86.5, 86.5]), "{aimed:?}");
    }
}
