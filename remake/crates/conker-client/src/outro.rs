//! War's victory outros (`../lab/research/mp-original/total-war-outro.md`, `colors-outro.md`): once
//! the match is over in a War mode ([`team_mode`]), the map's cutscene for the winning team (the
//! server's [`MatchView::winner`], as the STATS rank the teams; a DRAW plays none) from the pack's
//! `outros.json` ([`Outro`]): its cameras and their cuts, its actors (each at its place or along its
//! path, turning, playing its clips), the fade in, the speech bubble's pages ([`crate::hud`]) and the
//! voice line ([`crate::audio`]), with no player and no HUD; then the STATS over its last picture,
//! the actors' clips still running. Frame k counts the client's ticks from the outro's start, which
//! the server's restart countdown tells ([`RESULTS`] after the outro's end, `conker-server`'s
//! `results_at`): a client that joins mid-outro starts it there.
//!
//! `CONKER_OUTRO=<cutscene>` (debug, native): any match's end plays that outro of the map, whatever
//! the mode and the winner (the server's results don't wait for it).

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::character_gltf;
use conker_assets::outro::Outro;
use conker_sim::net::{MatchView, RESULTS};
use lightyear::prelude::*;

use crate::render::{Model, model_yaw};

/// The map's outros (Total War's on `2d`, Colors' on `30`) and the one `CONKER_OUTRO` forces.
#[derive(Resource, Clone, Debug, Default)]
pub struct Outros(pub Vec<Outro>, pub Option<u8>);

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

/// Whether the match is a War mode (Total War, Colors, Beach), whose end plays the winning team's
/// outro (as the server's `results_at` waits for it).
pub fn team_mode(m: &MatchView) -> bool {
    m.total_war || m.colors || m.beach
}

/// Whether the match's results show: over, and past its outro if it has one.
pub fn results(m: &MatchView, playing: &Playing, outros: &Outros) -> bool {
    m.phase == 2 && playing.running(outros).is_none()
}

pub fn build(app: &mut App, outros: Vec<Outro>) {
    let forced = std::env::var("CONKER_OUTRO")
        .ok()
        .and_then(|c| c.parse().ok());
    app.insert_resource(Outros(outros, forced));
    app.init_resource::<Playing>();
    app.add_systems(FixedUpdate, clock.run_if(not(is_in_rollback)));
}

/// Whether the match's end plays an outro.
fn plays(m: &MatchView, outros: &Outros) -> bool {
    team_mode(m) || outros.1.is_some()
}

/// Over, the winning team's outro (or the forced one, from its start) at the server's frame of it,
/// then a frame a tick; none otherwise.
fn clock(game: Query<&MatchView>, outros: Res<Outros>, mut playing: ResMut<Playing>) {
    let over = game.iter().find(|m| m.phase == 2 && plays(m, &outros));
    playing.0 = match (over, playing.0) {
        (None, _) => None,
        (Some(_), Some((i, k))) => Some((i, k + 1)),
        (Some(m), None) => {
            let (i, k) = match outros.1 {
                Some(c) => ((outros.0.iter()).position(|o| o.cutscene == c), 0),
                None => {
                    let i = (outros.0.iter()).position(|o| Some(o.winner) == m.winner);
                    (
                        i,
                        start(i.map_or(0, |i| outros.0[i].frames()), m.restart_in),
                    )
                }
            };
            if let Some(o) = i.map(|i| &outros.0[i]) {
                info!(
                    "outro: cutscene {}, {} frames, from {k}",
                    o.cutscene,
                    o.frames()
                );
            }
            i.map(|i| (i, k))
        }
    };
}

/// The frame of an outro `frames` long that the server is at, `restart_in` ticks before the next
/// match ([`RESULTS`] of them after the outro).
fn start(frames: u32, restart_in: u16) -> u32 {
    frames.saturating_sub(u32::from(restart_in).saturating_sub(RESULTS))
}

/// An outro's actor: the outro (an index of [`Outros`]) and the actor (of [`Outro::actors`]), its
/// glTF (held: the clips are looked up in it once the scene is in) and then its `AnimationPlayer`
/// and its clips' nodes ([`conker_assets::outro::Actor::clips`]).
#[derive(Component)]
pub struct Actor {
    outro: usize,
    n: usize,
    gltf: Handle<Gltf>,
    animator: Option<(Entity, Vec<AnimationNodeIndex>)>,
}

impl Actor {
    fn of<'a>(&self, outros: &'a Outros) -> &'a conker_assets::outro::Actor {
        outros.0[self.outro].actors().nth(self.n).unwrap()
    }
}

/// In a War mode, every outro's actors, hidden, once the players' characters are in (one glTF load
/// at a time, see `render::load_characters`), drawn as a player is (the model's lift and scale).
pub(crate) fn add_actors(
    game: Query<&MatchView>,
    outros: Res<Outros>,
    assets: Res<AssetServer>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if *added || !game.single().is_ok_and(|m| plays(m, &outros)) {
        return;
    }
    *added = true;
    for (outro, o) in outros.0.iter().enumerate() {
        for (n, a) in o.actors().enumerate() {
            let gltf = character_gltf(&a.character);
            let scene = format!("{gltf}#Scene0");
            commands
                .spawn((
                    Actor {
                        outro,
                        n,
                        gltf: assets.load(gltf),
                        animator: None,
                    },
                    WorldAssetRoot(assets.load(scene)),
                    place(a, 0.0),
                    Visibility::Hidden,
                ))
                .observe(actor_ready);
        }
    }
}

/// Where `a` stands at `t`, facing its way.
fn place(a: &conker_assets::outro::Actor, t: f32) -> Transform {
    let at = Vec3::from_array(a.place(t)) + Vec3::Y * a.scale * f32::from(a.lift);
    Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_y(model_yaw(a.facing(t))))
        .with_scale(Vec3::splat(a.scale))
}

/// The actor's scene is in: its `AnimationPlayer` gets the graph of its clips, each looping on its
/// length (the glTF's ends a tick earlier).
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
    let a = actor.of(&outros);
    let gltf = gltfs.get(&actor.gltf);
    let handles: Option<Vec<_>> = (a.clips())
        .map(|(name, ticks)| {
            let clip = gltf?.named_animations.get(name)?;
            if let Some(mut c) = clips.get_mut(clip) {
                c.set_duration(f32::from(ticks) / conker_sim::TICK_HZ as f32);
            }
            Some(clip.clone())
        })
        .collect();
    let animator = (children.iter_descendants(ready.entity)).find(|&e| animators.contains(e));
    let (Some(handles), Some(animator)) = (handles, animator) else {
        warn!("{}: no animation {} or player", a.character, a.animation);
        return;
    };
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    commands
        .entity(animator)
        .insert(AnimationGraphHandle(graphs.add(graph)));
    actor.animator = Some((animator, nodes));
}

/// While an outro plays (through its STATS): its actors show, placed and posed at its clock, and the
/// players hide; the view is its camera's (unless the free debug camera flies).
#[allow(clippy::type_complexity)]
pub(crate) fn show(
    playing: Res<Playing>,
    outros: Res<Outros>,
    mut actors: Query<(&Actor, &mut Transform, &mut Visibility), Without<Model>>,
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
    for (actor, mut transform, mut visibility) in &mut actors {
        let on = shown.filter(|_| playing.0.is_some_and(|p| p.0 == actor.outro));
        visibility.set_if_neq(match on {
            Some(_) => Visibility::Inherited,
            None => Visibility::Hidden,
        });
        let Some((o, k)) = on else {
            continue;
        };
        let a = actor.of(&outros);
        *transform = place(a, o.t(k));
        if let Some((animator, nodes)) = &actor.animator
            && let Ok(mut animator) = animators.get_mut(*animator)
        {
            let (clip, time) = a.clip(k);
            if !animator.is_playing_animation(nodes[clip]) {
                animator.stop_all();
            }
            let time = time / conker_sim::TICK_HZ as f32;
            animator
                .play(nodes[clip])
                .set_speed(0.0)
                .set_seek_time(time);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Over from its first tick, a client starts the outro at 0; joining 100 ticks into it, at 100;
    /// once past it, at its end (the STATS).
    #[test]
    fn a_late_client_joins_the_outro_where_the_server_is() {
        let at = |left: u32| start(400, (left + RESULTS) as u16);
        assert_eq!((at(400), at(300), start(400, 30)), (0, 100, 400));
    }
}
