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
//!
//! What the actors do besides their clips (`../lab/research/mp-original/outros-leftovers.md`): a
//! clip change cross-fades ([`script::Actor::blend`]); a head turns by its channels ([`turn`]) and a
//! jaw talks ([`jaw`]) over the sampled pose ([`pose`]); bank-09 attachments hang from their joints
//! and a villager hides its masked display lists ([`dress`]); the outro's props show where it puts
//! them; Beach's cs3 draws with its near plane.

use bevy::app::AnimationSystems;
use bevy::gltf::{Gltf, GltfExtras};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use conker_assets::outro::{self as script, OPEN, Outro, SHUT, Talk, frame, value};
use conker_assets::{character_gltf, outro_gltf};
use conker_rules::Rng;
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

/// Whether the match is a War mode (Total War, Colors, Beach, Heist), whose end plays the winning
/// team's outro (as the server's `results_at` waits for it).
pub fn team_mode(m: &MatchView) -> bool {
    m.total_war || m.colors || m.beach || m.heist
}

/// The [`Outro::winner`] the end plays: the winning team's, Heist's by whether a human's team won
/// (key 0, cutscene 2; else key 1, cutscene 3: the server's `outro`).
fn key(m: &MatchView) -> Option<u8> {
    match m.heist {
        true => Some(u8::from(!m.human_won)),
        false => m.winner,
    }
}

/// Whether the match's results show: over, and past its outro if it has one.
pub fn results(m: &MatchView, playing: &Playing, outros: &Outros) -> bool {
    m.phase == 2 && playing.running(outros).is_none()
}

pub fn build(app: &mut App, outros: Vec<Outro>) {
    let forced = std::env::var("CONKER_OUTRO")
        .ok()
        .and_then(|c| c.parse().ok());
    let particles = outros.iter().any(|o| !o.particles.is_empty());
    app.insert_resource(Outros(outros, forced));
    app.init_resource::<Playing>();
    app.add_systems(FixedUpdate, clock.run_if(not(is_in_rollback)));
    // Headless clients have no asset server (nor actors to dress).
    app.add_systems(Update, dress.run_if(resource_exists::<AssetServer>));
    app.add_systems(
        Update,
        wear.run_if(resource_exists::<crate::render::TeamLooks>),
    );
    if particles {
        let particles = (bubbles, crate::traps::puffs).chain();
        app.add_systems(Update, particles.run_if(resource_exists::<AssetServer>));
    }
    app.add_systems(
        PostUpdate,
        pose.after(AnimationSystems)
            .before(TransformSystems::Propagate),
    );
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
                    let i = (outros.0.iter()).position(|o| Some(o.winner) == key(m));
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
/// and its clips' nodes ([`conker_assets::outro::Actor::clips`]), then what [`dress`] found.
#[derive(Component)]
pub struct Actor {
    outro: usize,
    n: usize,
    gltf: Handle<Gltf>,
    animator: Option<(Entity, Vec<AnimationNodeIndex>)>,
    dressed: Option<Dressed>,
}

/// An actor's head and jaw joints and its attachments (bank-09 entry, joint, the model under it).
struct Dressed {
    head: Option<Entity>,
    jaw: Option<Entity>,
    attached: Vec<(u16, u8, Entity)>,
}

impl Actor {
    fn of<'a>(&self, outros: &'a Outros) -> &'a script::Actor {
        outros.0[self.outro].actors().nth(self.n).unwrap()
    }
}

/// An outro's prop (an index of [`Outros`]): shown while it plays.
#[derive(Component)]
pub struct OutroProp(usize);

/// An outro's light (an index of [`Outros`], the t it comes on).
#[derive(Component)]
pub struct OutroLight(usize, u16);

/// An actor dressed in its team's look (or with none).
#[derive(Component)]
struct Worn;

/// The villagers' head turn is left to their clips: their joint 12's channels compose with the
/// clip in a way the research does not settle (I: `heads_b3.csv`, `heads_b4.csv`).
fn turns_head(a: &script::Actor) -> bool {
    !matches!(a.character.as_str(), "156" | "157")
}

/// A joint's own rotation of `x` then `y` degrees, as the game composes them (`func_15038620`'s
/// rot(x, y, 0); `extract_character::anim::quaternion`'s order).
pub fn turn(x: f32, y: f32) -> Quat {
    Quat::from_rotation_y(y.to_radians()) * Quat::from_rotation_x(x.to_radians())
}

/// The jaw's openness (0-255), skew (−10..9) and degrees wide open at frame `k` (`func_15034860`
/// each frame from the first talk): shut −60 a frame, open +60 and held, chatter ±(rand % 20 + 40)
/// turning at 0 and 255, a new skew (rand % 20 − 10) at 0 if it skews. `rng`: the actor's own (I:
/// the original's `rand` sequence is not reproduced). None before the first talk.
pub fn jaw(talk: &Talk, k: u32, mut rng: Rng) -> Option<(u8, i8, u8)> {
    let first = frame(talk.says.first()?.t);
    let (mut open, mut speed, mut skew, mut degrees) = (0i32, 0i32, 0i32, 0);
    for f in first..=k {
        let say = talk.says.iter().rev().find(|s| frame(s.t) <= f)?;
        degrees = say.degrees;
        match say.state {
            SHUT => open = (open - 60).max(0),
            OPEN => open = (open + 60).min(255),
            _ => {
                let mut pace = || (rng.next_u32() % 20) as i32 + 40;
                if open <= 0 {
                    speed = pace();
                    if talk.skew {
                        skew = (rng.next_u32() % 20) as i32 - 10;
                    }
                } else if open >= 255 {
                    speed = -pace();
                }
                open = (open + speed).clamp(0, 255);
            }
        }
    }
    (k >= first).then_some((open as u8, skew as i8, degrees))
}

/// The jaw's rotation: openness / 255 of its degrees about x, the skew's 1.41° steps about y
/// (`func_15034860`'s overrides: e · 182.044 or 8191.875 units, skew · 257).
fn jaw_turn((open, skew, degrees): (u8, i8, u8)) -> Quat {
    let open = f32::from(open) / 255.0;
    turn(
        open * f32::from(degrees),
        f32::from(skew) * 257.0 * 360.0 / 65536.0 * open,
    )
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
        // Its point light (Heist's white one over the vault's door), shown from its `t`.
        if let Some(l) = &o.light {
            let light = conker_assets::Light {
                at: l.at.map(f32::from),
                rgb: l.rgb,
                size: l.size,
                swing: 0.0,
                step: 0.0,
                when: conker_assets::When::Calm,
                flicker: false,
            };
            commands.spawn((
                OutroLight(outro, l.t),
                crate::lights::point_light(&light, l.size),
                Transform::from_translation(Vec3::from_array(light.at)),
                Visibility::Hidden,
            ));
        }
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
                        dressed: None,
                    },
                    WorldAssetRoot(assets.load(scene)),
                    place(a, 0.0),
                    Visibility::Hidden,
                ))
                .observe(actor_ready);
        }
    }
}

/// Where `a` stands at `t`, facing its way, rolled about its forward axis and pitched about its
/// side one by its channels 6 and 8 (Heist cs3's sinking Wise Guy; I: the axes and signs).
fn place(a: &conker_assets::outro::Actor, t: f32) -> Transform {
    let at = Vec3::from_array(a.place(t)) + Vec3::Y * a.scale * f32::from(a.lift);
    let [roll, pitch] = [&a.roll, &a.pitch].map(|keys| value(keys, t).to_radians());
    Transform::from_translation(at)
        .with_rotation(
            Quat::from_rotation_y(model_yaw(a.facing(t)))
                * Quat::from_rotation_z(roll)
                * Quat::from_rotation_x(pitch),
        )
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

/// While an outro plays (through its STATS): its actors and props show, the actors placed and posed
/// at its clock (a clip change cross-fading), and the players hide; the view is its camera's
/// (unless the free debug camera flies), with its near plane. Through a scope the local player's
/// body and weapon hide in its own view (C `func_1502E4C4`: the view's bit in obj+0x74 while
/// s126+0x197 is set; V scopeB: from the frame the ring shows to the release).
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    playing: Res<Playing>,
    outros: Res<Outros>,
    mut actors: Query<(&Actor, &mut Transform, &mut Visibility), Without<Model>>,
    mut props: Query<(&OutroProp, &mut Visibility), (Without<Actor>, Without<Model>)>,
    mut lights: Query<
        (&OutroLight, &mut Visibility),
        (Without<OutroProp>, Without<Actor>, Without<Model>),
    >,
    mut animators: Query<&mut AnimationPlayer>,
    mut players: Query<
        (&mut Visibility, Option<&crate::FollowCamera>),
        (With<Model>, Without<OutroProp>, Without<OutroLight>),
    >,
    camera: Single<
        (
            &mut Transform,
            &mut Projection,
            &bevy::camera_controller::free_camera::FreeCameraState,
        ),
        Without<Actor>,
    >,
    mut near: Local<Option<f32>>,
) {
    let shown = playing.of(&outros);
    let shows = |outro: usize| match shown.filter(|_| playing.0.is_some_and(|p| p.0 == outro)) {
        Some(_) => Visibility::Inherited,
        None => Visibility::Hidden,
    };
    for (prop, mut visibility) in &mut props {
        visibility.set_if_neq(shows(prop.0));
    }
    for (light, mut visibility) in &mut lights {
        let on = shown.is_some_and(|(o, k)| o.t(k) >= f32::from(light.1));
        visibility.set_if_neq(if on {
            shows(light.0)
        } else {
            Visibility::Hidden
        });
    }
    for (actor, mut transform, mut visibility) in &mut actors {
        visibility.set_if_neq(shows(actor.outro));
        let Some((o, k)) = shown.filter(|_| playing.0.is_some_and(|p| p.0 == actor.outro)) else {
            continue;
        };
        let a = actor.of(&outros);
        *transform = place(a, o.t(k));
        if let Some((animator, nodes)) = &actor.animator
            && let Ok(mut animator) = animators.get_mut(*animator)
        {
            let (clip, time) = a.clip(k);
            let blend = a.blend(k);
            let w = blend.map_or(0.0, |b| b.2);
            let poses = [(clip, time, 1.0 - w)].into_iter().chain(blend);
            let poses: Vec<_> = poses.map(|(c, t, w)| (nodes[c], t, w)).collect();
            let stale: Vec<AnimationNodeIndex> = (animator.playing_animations())
                .map(|(node, _)| *node)
                .filter(|node| poses.iter().all(|p| p.0 != *node))
                .collect();
            for node in stale {
                animator.stop(node);
            }
            for (node, time, weight) in poses {
                animator
                    .play(node)
                    .set_speed(0.0)
                    .set_seek_time(time / conker_sim::TICK_HZ as f32)
                    .set_weight(weight);
            }
        }
    }
    let free = camera.2.enabled;
    for (mut v, own) in &mut players {
        let scoped = own.is_some_and(|c| c.follow.cam.scope.on) && !free;
        v.set_if_neq(match shown.is_some() || scoped {
            true => Visibility::Hidden,
            false => Visibility::Inherited,
        });
    }
    let (mut transform, mut projection, free) = camera.into_inner();
    if let Projection::Perspective(p) = &mut *projection {
        let base = *near.get_or_insert(p.near);
        let want = shown.and_then(|(o, _)| o.near).unwrap_or(base);
        if p.near != want {
            p.near = want;
        }
    }
    if let (Some((o, k)), false) = (shown, free.enabled) {
        let (eye, at) = o.camera(o.t(k));
        *transform = Transform::from_translation(Vec3::from_array(eye))
            .looking_at(Vec3::from_array(at), Vec3::Y);
    }
}

/// Once its scene is in, an actor with a team look (`Actor::team`: Heist cs3's Wise Guy in REDS'
/// shirt) wears it as a player's model does (`render::dress_teams`).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn wear(
    actors: Query<(Entity, &Actor), Without<Worn>>,
    outros: Res<Outros>,
    cfg: Res<crate::Config>,
    looks: Res<crate::render::TeamLooks>,
    children: Query<&Children>,
    mut parts: Query<(
        &bevy::gltf::GltfMaterialName,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, actor) in actors.iter().filter(|(_, a)| a.animator.is_some()) {
        commands.entity(entity).insert(Worn);
        let a = actor.of(&outros);
        let body = (crate::characters(&cfg.map).iter()).position(|c| *c == a.character);
        let Some((name, texture)) = a.team.zip(body).and_then(|(t, b)| looks.of(b, t)) else {
            continue;
        };
        let look = (name, assets.load(texture.to_string()));
        crate::render::wear(entity, look, &children, &mut parts, &mut materials);
    }
}

/// An outro's particles ([`script::Particles`]: Heist cs3's bubbles off the sinking Wise Guy):
/// while an emitter runs, a bubble a frame (I) about its actor, `hud/bubble_particle.png` 3.1-11.8
/// units big (I: its sizes 31-118, V, over 10), rising 1-2 a frame (I) for 50-156 frames (V),
/// fading out ([`crate::traps::puffs`]), added to the view (I).
#[allow(clippy::too_many_arguments)]
fn bubbles(
    playing: Res<Playing>,
    outros: Res<Outros>,
    actors: Query<(&Actor, &Transform)>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut look: Local<Option<(Handle<Mesh>, Handle<Image>)>>,
    mut last: Local<Option<u32>>,
    mut rng: Local<Rng>,
    mut commands: Commands,
) {
    let Some((i, k)) = playing.0.filter(|p| *last != Some(p.1)) else {
        *last = playing.0.map(|p| p.1);
        return;
    };
    *last = Some(k);
    let o = &outros.0[i];
    let t = o.t(k);
    for p in (o.particles.iter()).filter(|p| (f32::from(p.t)..f32::from(p.until)).contains(&t)) {
        let n = usize::from(p.actor);
        let Some((_, at)) = actors.iter().find(|(a, _)| a.outro == i && a.n == n) else {
            continue;
        };
        let (mesh, texture) = look
            .get_or_insert_with(|| {
                let quad = meshes.add(Rectangle::new(1.0, 1.0));
                (quad, assets.load("hud/bubble_particle.png"))
            })
            .clone();
        let mut r = || rng.next_f32();
        let size = (31.0 + 87.0 * r()) / 10.0;
        let at = at.translation + Vec3::new(20.0 * r() - 10.0, 40.0 * r(), 20.0 * r() - 10.0);
        let puff = crate::traps::Puff {
            vel: Vec3::Y * (1.0 + r()),
            fall: 0.0,
            age: 0.0,
            life: 50.0 + 106.0 * r(),
            alpha: 1.0,
        };
        // The texture has no alpha (CI4 over an RGBA16 grey ramp, every entry's alpha bit set;
        // read as IA16 its alphas would be noise): only an additive blend drops its dark rim (I:
        // no display list of a bubble seen).
        let material = StandardMaterial {
            alpha_mode: AlphaMode::Add,
            ..crate::traps::material(&texture, [255; 3], 1.0)
        };
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(material)),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            puff,
        ));
    }
}

/// Once an actor's scene is in: its head and jaw joints are found, its attachments hang hidden
/// under their joints (`outro.gltf`'s `09-<entry>`, loaded after the actors' scenes), and its
/// masked display lists hide (a villager's look, `beach::hidden`'s rule with the outro's mask).
/// Then each frame an attachment shows from its `on` record's time to its `off` one, while its
/// outro plays. The props are spawned once the glTF is in.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dress(
    mut actors: Query<(Entity, &mut Actor)>,
    playing: Res<Playing>,
    outros: Res<Outros>,
    cfg: Res<crate::Config>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut parts: Query<(&GltfExtras, &mut Visibility)>,
    mut shown: Query<&mut Visibility, Without<GltfExtras>>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut models: Local<Option<Handle<Gltf>>>,
    mut props: Local<bool>,
    mut commands: Commands,
) {
    let needs = |o: &Outro| !o.props.is_empty() || o.actors().any(|a| !a.attach.is_empty());
    let all_in = actors.iter().all(|(_, a)| a.animator.is_some());
    if all_in && models.is_none() && outros.0.iter().any(needs) {
        *models = Some(assets.load(outro_gltf(&cfg.map)));
    }
    let g = (models.as_ref()).and_then(|h| {
        gltfs
            .get(h)
            .filter(|_| assets.is_loaded_with_dependencies(h))
    });
    if let (Some(g), false) = (g, *props) {
        *props = true;
        for (n, o) in outros.0.iter().enumerate() {
            for p in &o.props {
                let Some(scene) = g.named_scenes.get(format!("prop-{}", p.placement).as_str())
                else {
                    warn!("outro.gltf: no prop-{}", p.placement);
                    continue;
                };
                let [x, y] = p.rotation;
                commands.spawn((
                    OutroProp(n),
                    WorldAssetRoot(scene.clone()),
                    Transform::from_translation(Vec3::from_array(p.at.map(f32::from)))
                        .with_rotation(turn(x, y))
                        .with_scale(p.scale.map_or(Vec3::ONE, Vec3::from_array)),
                    Visibility::Hidden,
                    crate::lights::Lit,
                ));
            }
        }
    }
    let k = playing.0;
    for (entity, mut actor) in &mut actors {
        let a = actor.of(&outros).clone();
        if actor.dressed.is_none()
            && actor.animator.is_some()
            && (a.attach.is_empty() || g.is_some())
        {
            let joint = |j: u8| {
                let name = format!("joint_{j:02}");
                (children.iter_descendants(entity))
                    .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
            };
            let mut attached = Vec::new();
            for at in a.attach.iter().filter(|at| at.on) {
                if attached
                    .iter()
                    .any(|x: &(u16, u8, Entity)| (x.0, x.1) == (at.entry, at.joint))
                {
                    continue;
                }
                let scene =
                    g.and_then(|g| g.named_scenes.get(format!("09-{:02x}", at.entry).as_str()));
                let (Some(scene), Some(parent)) = (scene, joint(at.joint)) else {
                    warn!(
                        "{}: no attachment {:#x} or joint {}",
                        a.character, at.entry, at.joint
                    );
                    continue;
                };
                let model = commands.spawn((
                    WorldAssetRoot(scene.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ChildOf(parent),
                ));
                attached.push((at.entry, at.joint, model.id()));
            }
            if let Some(mask) = a.mask {
                for e in children.iter_descendants(entity) {
                    let Ok((extras, mut v)) = parts.get_mut(e) else {
                        continue;
                    };
                    let list = serde_json::from_str::<serde_json::Value>(&extras.value).ok();
                    if list
                        .and_then(|x| x.get("list")?.as_u64())
                        .is_some_and(|l| l < 8 && mask >> l & 1 != 0)
                    {
                        *v = Visibility::Hidden;
                    }
                }
            }
            actor.dressed = Some(Dressed {
                head: a.head.as_ref().and_then(|h| joint(h.joint)),
                jaw: a.talk.as_ref().and_then(|t| joint(t.jaw)),
                attached,
            });
        }
        let Some(d) = &actor.dressed else {
            continue;
        };
        let t = k
            .filter(|p| p.0 == actor.outro)
            .map(|(i, k)| outros.0[i].t(k));
        for &(entry, joint, model) in &d.attached {
            let last = (a.attach.iter()).rfind(|x| {
                (x.entry, x.joint) == (entry, joint) && t.is_some_and(|t| f32::from(x.t) <= t)
            });
            if let Ok(mut v) = shown.get_mut(model) {
                v.set_if_neq(match last.is_some_and(|x| x.on) {
                    true => Visibility::Inherited,
                    false => Visibility::Hidden,
                });
            }
        }
    }
}

/// Over the sampled pose (after the animation, before the transforms propagate): a turned head's
/// joint is its channels' rotation from the turn's time (the villagers' aside, [`turns_head`]); a
/// talking jaw's is its openness and skew.
fn pose(
    playing: Res<Playing>,
    outros: Res<Outros>,
    actors: Query<&Actor>,
    mut joints: Query<&mut Transform, Without<Actor>>,
) {
    let Some((o, k)) = playing.of(&outros) else {
        return;
    };
    let t = o.t(k);
    for actor in actors
        .iter()
        .filter(|a| playing.0.is_some_and(|p| p.0 == a.outro))
    {
        let (a, Some(d)) = (actor.of(&outros), &actor.dressed) else {
            continue;
        };
        if let (Some(h), Some(joint)) = (&a.head, d.head)
            && turns_head(a)
            && t >= f32::from(h.t)
            && let Ok(mut j) = joints.get_mut(joint)
        {
            j.rotation = turn(-value(&h.pitch, t), value(&h.yaw, t));
        }
        let seed = (actor.outro as u64) << 8 | actor.n as u64;
        if let (Some(talk), Some(joint)) = (&a.talk, d.jaw)
            && let Some(open) = jaw(talk, k, Rng::new(seed << 1 | 1))
            && let Ok(mut j) = joints.get_mut(joint)
        {
            j.rotation = jaw_turn(open);
        }
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

    /// Through the scope only the local player's own model hides (`func_1502E4C4`), and not while
    /// the free debug camera flies; it shows again once the scope is down.
    #[test]
    fn the_scope_hides_the_own_body() {
        use bevy::camera_controller::free_camera::FreeCameraState;
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<Playing>();
        world.init_resource::<Outros>();
        let mut free = FreeCameraState::default();
        free.enabled = false;
        let camera = (world.spawn((Transform::default(), Projection::default(), free))).id();
        let follow = conker_sim::follow::Follow::spawn(
            [0.0, 100.0, 250.0],
            &conker_sim::movement::PlayerState::spawn([0.0; 3], 0),
        );
        let mut own = crate::FollowCamera::new(follow, 0, 0);
        own.follow.cam.scope.on = true;
        let model = || Model {
            body: 0,
            scene: Entity::PLACEHOLDER,
        };
        let me = world.spawn((Visibility::Inherited, model(), own)).id();
        let other = world.spawn((Visibility::Inherited, model())).id();
        let seen = |world: &mut World| {
            world.run_system_once(show).unwrap();
            [me, other].map(|e| *world.get::<Visibility>(e).unwrap())
        };
        assert_eq!(
            seen(&mut world),
            [Visibility::Hidden, Visibility::Inherited]
        );
        world.get_mut::<FreeCameraState>(camera).unwrap().enabled = true;
        assert_eq!(seen(&mut world), [Visibility::Inherited; 2], "free camera");
        world.get_mut::<FreeCameraState>(camera).unwrap().enabled = false;
        let mut own = world.get_mut::<crate::FollowCamera>(me).unwrap();
        own.follow.cam.scope.on = false;
        assert_eq!(seen(&mut world), [Visibility::Inherited; 2], "scope down");
    }

    /// The pack's outros of a map; None (the test skips) without them.
    fn pack_outros(map: &str) -> Option<Vec<Outro>> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let json = std::fs::read(format!("{dir}/maps/{map}/outros.json")).ok();
        json.map(|j| serde_json::from_slice(&j).unwrap())
    }

    /// A rotation's yaw, pitch and roll in degrees, as `heads.py` reads the oracle's joint matrices.
    fn angles(q: Quat) -> [f32; 3] {
        let r = Mat3::from_quat(q);
        let (x, y, z) = (r.x_axis, r.y_axis, r.z_axis);
        [z.x.atan2(z.z), (-z.y).asin(), x.y.atan2(y.y)].map(f32::to_degrees)
    }

    /// Heist's outros against the oracle (`lab/research/mp-original/heist-presentation-scratch/
    /// outro`): a human's team 1st plays cutscene 2 (325 frames), else 3 (775); frame k is read 459
    /// + k (cs2) or 157 + k (cs3): its clock, each actor's place (feet), facing, scale and roll
    /// (channel 6, about its forward axis: I) as the CSVs', and the fade's alpha.
    #[test]
    fn heist_outros_follow_the_oracle() {
        let Some(outros) = pack_outros("1a") else {
            eprintln!("no pack outros: skipping");
            return;
        };
        let pick = |human_won| {
            let m = MatchView {
                heist: true,
                human_won,
                winner: Some(3),
                ..MatchView::default()
            };
            (outros.iter()).position(|o| Some(o.winner) == key(&m))
        };
        assert_eq!((pick(true), pick(false)), (Some(0), Some(1)));
        let shape: Vec<_> = outros.iter().map(|o| (o.cutscene, o.frames())).collect();
        assert_eq!(shape, [(2, 325), (3, 775)]);
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../lab/research/mp-original/heist-presentation-scratch/outro"
        );
        let csv = |name: String| -> Option<Vec<Vec<String>>> {
            let text = std::fs::read_to_string(format!("{dir}/{name}")).ok()?;
            let rows = text.lines().skip(1);
            Some(
                rows.map(|l| l.split(',').map(String::from).collect())
                    .collect(),
            )
        };
        for (o, n, first) in [(&outros[0], 2, 459), (&outros[1], 3, 157)] {
            let (Some(actors), Some(fades)) = (
                csv(format!("outro_cs{n}_actors.csv")),
                csv(format!("fade_cs{n}.csv")),
            ) else {
                eprintln!("no oracle CSVs: skipping");
                return;
            };
            let mut objects = Vec::new();
            let mut checked = 0;
            for row in &actors {
                let f = |c: usize| row[c].parse::<f32>().unwrap();
                let obj = row[2].clone();
                if !objects.contains(&obj) {
                    objects.push(obj.clone());
                }
                let a = (o.actors())
                    .nth(objects.iter().position(|x| *x == obj).unwrap())
                    .unwrap();
                let k = row[0].parse::<u32>().unwrap() - first;
                if k == 0 || k > o.frames() {
                    continue;
                }
                let t = o.t(k);
                assert_eq!(t, f(1), "cs{n} frame {k}");
                let p = place(a, t);
                let feet = p.translation - Vec3::Y * a.scale * f32::from(a.lift);
                let want = Vec3::new(f(6), f(7), f(8));
                assert!(feet.distance(want) < 0.1, "cs{n} t {t}: {feet} not {want}");
                assert_eq!(p.scale, Vec3::splat(f(22)), "cs{n} t {t}");
                assert!(a.facing(t).abs_diff(row[9].parse().unwrap()) <= 1);
                let roll = angles(p.rotation)[2];
                assert!(
                    (roll - f(24)).abs() < 0.05,
                    "cs{n} t {t}: roll {roll} not {}",
                    f(24)
                );
                checked += 1;
            }
            assert!(checked > 100, "cs{n}: {checked} rows");
            for row in fades
                .iter()
                .filter(|r| r.get(6).is_some_and(|a| !a.is_empty()))
            {
                let k = row[0].parse::<u32>().unwrap() - first;
                let alpha: i32 = row[6].parse().unwrap();
                let got = i32::from(o.fade(o.t(k)));
                assert!(
                    (got - alpha).abs() <= 1,
                    "cs{n} frame {k}: fade {got} not {alpha}"
                );
            }
        }
    }

    /// TW cs4's Conker and Colors cs5's soldiers turn their heads by the channels: joint 3's yaw,
    /// pitch and roll within 0.5° of `heads_tw4.csv` and of the research's cs5 values at t 328.
    #[test]
    fn outro_heads_match_the_oracle() {
        let (Some(tw), Some(colors)) = (pack_outros("2d"), pack_outros("30")) else {
            eprintln!("no pack outros: skipping");
            return;
        };
        let pose = |o: &Outro, n: usize, t: u16| {
            let a = o.actors().nth(n).unwrap();
            let h = a.head.as_ref().filter(|h| t >= h.t).unwrap();
            let t = f32::from(t);
            angles(turn(-value(&h.pitch, t), value(&h.yaw, t)))
        };
        let cases = [
            (&tw[1], 0, 130, [36.0, 0.8, 0.0]),
            (&tw[1], 0, 348, [-20.0, 0.0, 0.0]),
            (&tw[1], 0, 588, [35.0, 0.0, 0.0]),
            (&tw[1], 0, 748, [1.0, 0.0, 0.0]),
            (&colors[1], 0, 328, [67.6, 0.0, 0.0]),
            (&colors[1], 1, 328, [83.0, 0.0, 0.0]),
            (&colors[1], 2, 328, [22.4, 0.0, 0.0]),
        ];
        for (o, n, t, want) in cases {
            let got = pose(o, n, t);
            let near = got.iter().zip(want).all(|(g, w)| (g - w).abs() < 0.5);
            assert!(near, "actor {n} at t {t}: {got:?}, the oracle {want:?}");
        }
    }

    /// TW cs4's Conker talks from t 36 (frame 18): the jaw is shut before, moves at most 60/255
    /// of its 45° a frame, never past them, and is shut 5 frames after his t 153 stop.
    #[test]
    fn talk_moves_the_jaw() {
        let Some(tw) = pack_outros("2d") else {
            eprintln!("no pack outros: skipping");
            return;
        };
        let talk = tw[1].actors().next().and_then(|a| a.talk.as_ref()).unwrap();
        let open = |k| jaw(talk, k, Rng::new(3)).map(|(open, ..)| open);
        assert_eq!((open(17), open(18).is_some()), (None, true));
        let angle = |k| jaw(talk, k, Rng::new(3)).map_or(0.0, |j| angles(jaw_turn(j))[1].abs());
        let angles: Vec<f32> = (18..80).map(angle).collect();
        assert!(angles.iter().all(|&a| a <= 45.01) && angles.iter().any(|&a| a > 20.0));
        assert!(
            angles
                .windows(2)
                .all(|w| (w[1] - w[0]).abs() <= 10.6 + 0.01)
        );
        assert_eq!(open(frame(153) + 5), Some(0));
    }
}
