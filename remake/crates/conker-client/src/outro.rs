//! War: Total War's victory outro (`../lab/research/mp-original/total-war-outro.md`): once the
//! match is over, the winning team's cutscene ([`team_result`], as the STATS rank the teams; a DRAW
//! plays none) from the pack's `outros.json` ([`Outro`]): its camera path, its one actor looping its
//! clip, the fade in, the speech bubble's pages ([`crate::hud`]) and the voice line
//! ([`crate::audio`]), with no player and no HUD; then the STATS over its last picture, the actor
//! still looping. Frame k counts the client's ticks since it saw the match over (the server's results
//! wait as long, `conker-server`'s `results_at`).

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::character_gltf;
use conker_assets::outro::Outro;
use conker_rules::game::team_result;
use conker_sim::net::{Combat, MatchView};
use lightyear::prelude::*;

use crate::render::{Model, model_yaw};

/// The map's outros (none but on `2d`).
#[derive(Resource, Clone, Debug, Default)]
pub struct Outros(pub Vec<Outro>);

/// The outro this match's end plays (an index of [`Outros`]) and its frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Playing(pub Option<(usize, u32)>);

impl Playing {
    /// The outro and frame, through its STATS.
    pub fn of<'a>(&self, outros: &'a Outros) -> Option<(&'a Outro, u32)> {
        self.0.and_then(|(i, k)| Some((outros.0.get(i)?, k)))
    }

    /// The outro and frame while it runs (before its STATS).
    pub fn running<'a>(&self, outros: &'a Outros) -> Option<(&'a Outro, u32)> {
        self.of(outros).filter(|(o, k)| *k < o.frames())
    }
}

/// Whether the match's results show: over, and past its outro if it has one.
pub fn results(m: &MatchView, playing: &Playing, outros: &Outros) -> bool {
    m.phase == 2 && playing.running(outros).is_none()
}

pub fn build(app: &mut App, outros: Vec<Outro>) {
    app.insert_resource(Outros(outros));
    app.init_resource::<Playing>();
    app.add_systems(FixedUpdate, clock.run_if(not(is_in_rollback)));
}

/// Over in Total War, the winning team's outro from the tick the client sees it, a frame a tick;
/// none otherwise.
#[allow(clippy::type_complexity)]
fn clock(
    game: Query<&MatchView>,
    roster: Query<&Combat, Or<(With<Predicted>, With<Interpolated>)>>,
    outros: Res<Outros>,
    mut playing: ResMut<Playing>,
) {
    let over = game.iter().any(|m| m.phase == 2 && m.total_war);
    playing.0 = match (over, playing.0) {
        (false, _) => None,
        (true, Some((i, k))) => Some((i, k + 1)),
        (true, None) => {
            let all: Vec<_> = roster
                .iter()
                .map(|c| Some(crate::hud::standing(c)))
                .collect();
            let winner = team_result(&all);
            let i = (outros.0.iter()).position(|o| Some(o.winner) == winner);
            if let Some(o) = i.map(|i| &outros.0[i]) {
                info!("outro: cutscene {}, {} frames", o.cutscene, o.frames());
            }
            i.map(|i| (i, 0))
        }
    };
}

/// An outro's actor (an index of [`Outros`]), its glTF (held: the clip is looked up in it once the
/// scene is in) and then its `AnimationPlayer` and clip's node.
#[derive(Component)]
pub struct Actor(usize, Handle<Gltf>, Option<(Entity, AnimationNodeIndex)>);

/// In Total War, every outro's actor, hidden, once the players' characters are in (one glTF load at
/// a time, see `render::load_characters`): where it stands, drawn as a player is (the model's lift
/// and scale).
pub(crate) fn add_actors(
    game: Query<&MatchView>,
    outros: Res<Outros>,
    assets: Res<AssetServer>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if *added || !game.single().is_ok_and(|m| m.total_war) {
        return;
    }
    *added = true;
    for (i, o) in outros.0.iter().enumerate() {
        let a = &o.actor;
        let at = Vec3::from_array(a.at.map(f32::from)) + Vec3::Y * a.scale * f32::from(a.lift);
        let gltf = character_gltf(&a.character);
        let scene = format!("{gltf}#Scene0");
        commands
            .spawn((
                Actor(i, assets.load(gltf), None),
                WorldAssetRoot(assets.load(scene)),
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(model_yaw(a.facing)))
                    .with_scale(Vec3::splat(a.scale)),
                Visibility::Hidden,
            ))
            .observe(actor_ready);
    }
}

/// The actor's scene is in: its `AnimationPlayer` gets the graph of its one clip, which loops on
/// the clip's length (the glTF's ends a tick earlier).
#[allow(clippy::too_many_arguments)]
fn actor_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    mut actors: Query<&mut Actor>,
    outros: Res<Outros>,
    gltfs: Res<Assets<Gltf>>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    let Ok(mut actor) = actors.get_mut(ready.entity) else {
        return;
    };
    let a = &outros.0[actor.0].actor;
    let clip = gltfs
        .get(&actor.1)
        .and_then(|g| g.named_animations.get(a.animation.as_str()));
    let animator = (children.iter_descendants(ready.entity)).find(|&e| animators.contains(e));
    let (Some(clip), Some(animator)) = (clip, animator) else {
        warn!("{}: no animation {} or player", a.character, a.animation);
        return;
    };
    if let Some(mut c) = clips.get_mut(clip) {
        c.set_duration(f32::from(a.ticks) / conker_sim::TICK_HZ as f32);
    }
    let (graph, node) = AnimationGraph::from_clip(clip.clone());
    commands
        .entity(animator)
        .insert(AnimationGraphHandle(graphs.add(graph)));
    actor.2 = Some((animator, node));
}

/// While an outro plays (through its STATS): its actor shows, posed at its clip time, and the
/// players hide; the view is its camera's (unless the free debug camera flies).
#[allow(clippy::type_complexity)]
pub(crate) fn show(
    playing: Res<Playing>,
    outros: Res<Outros>,
    mut actors: Query<(&Actor, &mut Visibility), Without<Model>>,
    mut animators: Query<&mut AnimationPlayer>,
    mut players: Query<&mut Visibility, With<Model>>,
    camera: Single<
        (
            &mut Transform,
            &bevy::camera_controller::free_camera::FreeCameraState,
        ),
        Without<Actor>,
    >,
) {
    let shown = playing.of(&outros);
    for (actor, mut visibility) in &mut actors {
        let on = shown.filter(|_| playing.0.is_some_and(|p| p.0 == actor.0));
        visibility.set_if_neq(match on {
            Some(_) => Visibility::Inherited,
            None => Visibility::Hidden,
        });
        if let (Some((o, k)), Some((animator, node))) = (on, actor.2)
            && let Ok(mut animator) = animators.get_mut(animator)
        {
            let time = o.clip_time(k) / conker_sim::TICK_HZ as f32;
            animator.play(node).set_speed(0.0).set_seek_time(time);
        }
    }
    for mut v in &mut players {
        v.set_if_neq(match shown {
            Some(_) => Visibility::Hidden,
            None => Visibility::Inherited,
        });
    }
    let (mut transform, free) = camera.into_inner();
    if let (Some((o, k)), false) = (shown, free.enabled) {
        let (eye, at) = o.camera(o.t(k));
        *transform = Transform::from_translation(Vec3::from_array(eye))
            .looking_at(Vec3::from_array(at), Vec3::Y);
    }
}
