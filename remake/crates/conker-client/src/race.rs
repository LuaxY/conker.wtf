//! Race (`../lab/research/mp-original/race.md`; this batch's oracle runs in
//! `race-client-scratch/`): scenarios 11 and 12 on map 33, every player a racer on a hoverboard
//! (conker-sim's `board`, conker-rules' `race`), from the pack's `maps/33/race.json` ([`Course`])
//! and [`RaceView`]:
//! - the route the match names (`RaceView::route`): its lap of nav nodes is the prediction's track
//!   (the steering's clamp, the autopilot's point, the respawns), and the barriers it does not show
//!   are moved out of the client's collision as the server's (20000 up) and hidden ([`props`]);
//! - the racers: the human (class 0xB) is 136, the bots (0xC-0xF) 144 in their helmets; the board
//!   (`props.gltf` scene `board.scene`) on joint 23 and a bot's helmet on joint 12, all three
//!   hidden from the death to the respawn, with the blast where it died ([`gear`]);
//!   their clips ([`clip`], [`rate`]); the canister's boost flare ([`flares`]);
//! - the power-ups (placements 4-10) spinning, shrinking when taken and growing back with an
//!   overshoot ([`power_scale`]);
//! - the missiles in flight (`projectiles.gltf#Scene4` at 1.0, V m1 DL 224), nose along their way;
//! - the race camera behind the board, farther back on the grid ([`camera`]).
//!
//! The HUD is [`crate::hud`]'s, the sounds [`crate::audio`]'s.

use std::collections::HashMap;

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_assets::race::{Race, Route};
use conker_assets::{PACK_WEAPONS, projectiles_gltf, props_gltf};
use conker_rules::Fighter;
use conker_rules::net::Fighting;
use conker_rules::race::{DEAD, HUMAN, Track};
use conker_sim::board::KICK;
use conker_sim::follow::Follow;
use conker_sim::net::{Collision, Combat, MatchView, Player};
use lightyear::prelude::{LocalTimeline, Predicted};

use crate::MatchArena;
use crate::render::{Model, Rig};
use crate::tank::{Behind, Cam};

/// The bots' character folder: a racer's body on 33 ([`crate::characters`]).
pub const MODEL: &str = "144";
/// The barriers not shown are moved this far up, as the server's (they stay in the world).
const AWAY: i32 = 20_000;
/// The clips (logical ids, the Tedi's table; the pack's M12 notes): riding 5, knocked up 0x54, the
/// kick 0x354, dead 0x170.
pub const RIDE: u16 = 5;
pub const KNOCKED: u16 = 0x54;
pub const KICKING: u16 = 0x354;
pub const DEATH: u16 = 0x170;
/// The power-ups' placements (canisters 4, 5, 9; missiles 6, 7, 8, 10) and the barriers' (0-3).
const POWER_UPS: std::ops::RangeInclusive<u32> = 4..=10;
const BARRIERS: std::ops::RangeInclusive<u32> = 0..=3;
/// A power-up spins this many degrees a frame (V L1).
const SPIN: f32 = 3.03;
/// A taken power-up shrinks over 4 frames, then hides (V L1 541-544); back, it grows through these
/// shares of its size (V L1 551-557: 1.82, 3.64, 5.46, 7.0, 7.16, 7.01, 7.0 of 7).
const SHRINK: [f32; 3] = [0.75, 0.5, 0.25];
const GROW: [f32; 6] = [0.26, 0.52, 0.78, 1.0, 1.0229, 1.0014];
/// The missile: `projectiles.gltf` scene 4 at 1.0 (#0x33, V m1 DL 224).
const MISSILE_SCENE: usize = 4;
const MISSILE_SCALE: f32 = 1.0;
/// The death's blast, 55 over the racer (V idle 377): the smoke and fire shells (smoke 200 wide at
/// 1.94 to 5.83) and the 9 debris, as [`crate::beach::burst`]'s puffs and chunks (I).
const BLAST_UP: f32 = 55.0;
const BLAST: (u32, f32, u32) = (10, 300.0, 9);
/// The canister's boost flare (V w3c DLs 480-510: a white star quad at alpha 0x6B-0x71, 348-399
/// wide, 74-76 tall, over the racer 33-90 up). I: the strike's streak facing the camera, at its
/// middle (its texture, `801b7770`, is not in the pack).
const FLARE: [f32; 2] = [370.0, 75.0];
const FLARE_UP: f32 = 60.0;
const FLARE_ALPHA: u8 = 0x6E;

/// A racer's class: the human's (0xB) or a bot's (0xC-0xF).
pub fn racer(class: u8) -> bool {
    (HUMAN..=0xF).contains(&class)
}

/// The clip a racer shows for the rules' animation (5 riding, 84 kicking or knocked up, 112
/// dead): state 84 is the kick's clip while its kick runs, else the knock's; the death's 0x170.
pub fn clip(anim: u16, kicking: bool) -> u16 {
    match anim {
        a if a == u16::from(KICK) && kicking => KICKING,
        a if a == u16::from(KICK) => KNOCKED,
        a if a == u16::from(DEAD) => DEATH,
        a => a,
    }
}

/// A racer's clip rate: the human's kick at 1.35, the rest at 1 (C: the Tedi's rate table).
pub fn rate(clip: u16, class: u8) -> f32 {
    match clip == KICKING && class == HUMAN {
        true => 1.35,
        false => 1.0,
    }
}

/// The race camera behind the board (C: fit at rest 280 behind, 198 up, looking 195 over the
/// racer; at full speed 426, 232, 195; top 170, share 0.13; follow 0.045, 0.25, 0.1).
pub const CAM: Behind = Behind {
    rest: [280.0, 198.0, 195.0],
    full: [426.0, 232.0, 195.0],
    top: 170.0,
    share: 0.13,
    follow: [0.045, 0.25, 0.1],
};
/// On the grid, before the go (race.md §1, V c_fwd: 500 behind, 180 up, looking at 120 over).
pub const GRID: Behind = Behind {
    rest: [500.0, 180.0, 120.0],
    full: [500.0, 180.0, 120.0],
    ..CAM
};

/// One frame of a racer's view at frame `tick` ([`crate::tank::behind`] with [`GRID`] before its
/// go, then [`CAM`]), through walls (C `func_15123A54`: a board's camera casts no wall ray). The
/// board's stick steers itself (conker-sim's `board`: no camera yaw), so the stick needs no
/// override.
pub fn camera(follow: &mut Follow, cam: &mut Cam, f: &Fighter, tick: u32) {
    follow.cam.scope.on = false;
    let law = if tick < f.ride.go { &GRID } else { &CAM };
    crate::tank::behind(follow, cam, &f.body, law, None);
}

/// Race's warm-up: before its match starts the server steps nobody (conker-server's `warm_up`),
/// so neither does the prediction (the board would drift off its grid record).
pub fn warm_up(m: &MatchView, tick: u32) -> bool {
    m.race && !m.start.is_some_and(|s| s != u32::MAX && tick >= s)
}

/// Race's data on this client: `race.json` and the nav graph's bytes (the track); the route set up
/// once the match names it.
#[derive(Resource)]
pub struct Course {
    pub data: Race,
    nav: Vec<u8>,
    pub route: Option<u8>,
}

impl Course {
    /// The route of scenario `route` (11, 12).
    pub fn route(&self, route: u8) -> Option<&Route> {
        self.data.routes.iter().find(|r| r.scenario == route)
    }

    /// Whether level placement `p` is a barrier the route hides.
    pub fn hidden(&self, route: &Route, p: u32) -> bool {
        u16::try_from(p).is_ok_and(|p| self.data.barriers.contains(&p) && !route.shown.contains(&p))
    }
}

pub fn build(app: &mut App, windowed: bool, race: Option<Race>, nav: Vec<u8>) {
    let Some(data) = race else {
        return;
    };
    app.insert_resource(Course {
        data,
        nav,
        route: None,
    });
    app.add_systems(FixedPreUpdate, course);
    if windowed {
        app.init_resource::<Missiles>();
        app.add_systems(Update, (hang, gear, flares, props, missiles));
    }
}

/// The route the match names: its track for the prediction and its hidden barriers out of the
/// collision (as the server's), once.
fn course(
    game: Query<&MatchView>,
    mut course: ResMut<Course>,
    mut arena: ResMut<MatchArena>,
    mut world: ResMut<Collision>,
) {
    let Some(m) = game.iter().find(|m| m.race) else {
        return;
    };
    let id = m.racing.route;
    if course.route == Some(id) {
        return;
    }
    let Some(route) = course.route(id).cloned() else {
        return;
    };
    arena.0.track = Track::new(&course.nav, &route.lap);
    for &b in &course.data.barriers {
        world
            .0
            .shift(b, if route.shown.contains(&b) { 0 } else { AWAY });
    }
    info!(
        "race: route {id}, {} nodes a lap, barriers {:?} shown",
        arena.0.track.nodes.len(),
        route.shown
    );
    course.route = Some(id);
}

/// A racer's board and helmet are under its joints; whether it was dead last frame.
#[derive(Component)]
struct Gear(bool);

/// Once a racer's model is in, the board on its joint and its class's helmet on its own
/// (`props.gltf` in the joint's frame, model units, as a held weapon: V k1, cls). The glTF loads
/// after the characters' (one GLB at a time, `render::load_characters`).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hang(
    players: Query<(Entity, &Combat), (With<Rig>, Without<Gear>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    course: Res<Course>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut gltf: Local<Option<Handle<Gltf>>>,
    mut commands: Commands,
) {
    for (player, c) in players.iter().filter(|p| racer(p.1.class)) {
        let handle = gltf.get_or_insert_with(|| assets.load(props_gltf("33")));
        let Some(g) =
            (gltfs.get(&*handle)).filter(|_| assets.is_loaded_with_dependencies(&*handle))
        else {
            return;
        };
        let joint = |j: u8| {
            let name = format!("joint_{j:02}");
            (children.iter_descendants(player))
                .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
        };
        let d = &course.data;
        let parts = std::iter::once(d.board).chain(d.heads.get(&c.class).copied());
        for a in parts {
            if let (Some(scene), Some(joint)) = (g.scenes.get(a.scene), joint(a.joint)) {
                let part = (WorldAssetRoot(scene.clone()), Transform::IDENTITY);
                commands.spawn((part, ChildOf(joint)));
            }
        }
        commands.entity(player).insert(Gear(false));
    }
}

/// The death on the board (V idle DLs 377-463): the racer, board and helmet hidden from the death
/// to the respawn, and where it died, 55 up, the blast (I: [`crate::beach::burst`]'s puffs for the
/// smoke and fire shells, its chunks for the 9 debris). The local racer's death is its
/// prediction's.
#[allow(clippy::type_complexity)]
fn gear(
    mut racers: Query<(
        &Combat,
        &Player,
        &Model,
        &mut Gear,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    sprites: Option<Res<crate::beach::Sprites>>,
    mut shown: Query<&mut Visibility>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
    mut commands: Commands,
) {
    for (c, p, model, mut gear, fighting, predicted) in &mut racers {
        let dead = match fighting {
            Some(f) if predicted => f.state.health == 0,
            _ => c.health == 0,
        };
        if let Ok(mut v) = shown.get_mut(model.scene) {
            v.set_if_neq(match dead {
                true => Visibility::Hidden,
                false => Visibility::Inherited,
            });
        }
        if dead
            && !gear.0
            && let Some(sprites) = &sprites
        {
            let at = Vec3::from_array(p.pos) + Vec3::Y * BLAST_UP;
            crate::beach::burst(&mut commands, &mut materials, sprites, &mut seed, at, BLAST);
        }
        gear.0 = dead;
    }
}

/// A racer's boost flare.
#[derive(Component)]
struct Flare;

/// While a canister's boost runs, its racer's flare shows ([`FLARE`]; the local racer's boost is
/// its prediction's).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn flares(
    game: Query<&MatchView>,
    racers: Query<
        (
            Entity,
            &Combat,
            &Transform,
            Option<&Fighting>,
            Has<Predicted>,
        ),
        With<Gear>,
    >,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    sprites: Option<Res<crate::beach::Sprites>>,
    mut quads: Query<(&mut Transform, &mut Visibility), (With<Flare>, Without<Gear>)>,
    mut made: Local<HashMap<Entity, Entity>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let (Some(m), Some(sprites)) = (game.iter().find(|m| m.race), sprites) else {
        return;
    };
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    for (racer, c, at, fighting, predicted) in &racers {
        let boost = match fighting {
            Some(f) if predicted => f.state.boost,
            _ => (m.racing.racers.iter().find(|r| r.slot == c.slot)).map_or(0, |r| r.boost),
        };
        let flare = *made.entry(racer).or_insert_with(|| {
            let material = materials.add(StandardMaterial {
                base_color: Color::srgba_u8(255, 255, 255, FLARE_ALPHA),
                base_color_texture: Some(sprites.streak.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            });
            let quad = (Mesh3d(sprites.quad.clone()), MeshMaterial3d(material));
            let place = Transform::from_xyz(0.0, FLARE_UP, 0.0)
                .with_scale(Vec3::new(FLARE[0], FLARE[1], 1.0));
            (commands.spawn((Flare, quad, place, Visibility::Hidden, ChildOf(racer)))).id()
        });
        let Ok((mut t, mut v)) = quads.get_mut(flare) else {
            continue;
        };
        v.set_if_neq(match boost > 0 {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        t.rotation = at.rotation.inverse() * facing;
    }
}

/// A power-up's share of its size: `taken` frames since it was taken (`None` not taken), `back`
/// frames since it came back; `None` hidden.
pub fn power_scale(taken: Option<u32>, back: Option<u32>) -> Option<f32> {
    match (taken, back) {
        (Some(k), _) => SHRINK.get(k as usize).copied(),
        (None, Some(k)) => Some(GROW.get(k as usize).copied().unwrap_or(1.0)),
        (None, None) => Some(1.0),
    }
}

/// A power-up's spin `tick` frames into the match, about its own y.
pub fn spin(tick: u32) -> Quat {
    Quat::from_rotation_y((SPIN * (tick % 360_000) as f32).to_radians())
}

/// Whether Race's props system draws placement `p` (in place of [`crate::render`]'s pickups).
pub fn owns(p: u32) -> bool {
    BARRIERS.contains(&p) || POWER_UPS.contains(&p)
}

/// One level node Race draws: placement, entity, its pose at home, and since when it is taken or
/// back (this client's ticks).
struct Node {
    placement: u32,
    entity: Entity,
    home: Option<Transform>,
    taken: Option<u32>,
    back: Option<u32>,
}

/// The barriers the route hides stay hidden; the power-ups spin, shrink away when taken and grow
/// back.
#[allow(clippy::type_complexity)]
fn props(
    game: Query<&MatchView>,
    course: Res<Course>,
    timeline: Res<LocalTimeline>,
    added: Query<(Entity, &Name), Added<Name>>,
    mut nodes: Local<Vec<Node>>,
    mut shown: Query<(&mut Transform, &mut Visibility), With<Name>>,
) {
    let found = (added.iter())
        .filter_map(|(e, n)| Some((n.rsplit_once('@')?.1.parse::<u32>().ok()?, e)))
        .filter(|(p, _)| owns(*p));
    nodes.extend(found.map(|(placement, entity)| Node {
        placement,
        entity,
        home: None,
        taken: None,
        back: None,
    }));
    let Some(m) = game.iter().find(|m| m.race) else {
        return;
    };
    let Some(route) = course.route(m.racing.route) else {
        return;
    };
    let tick = timeline.tick().0;
    for n in nodes.iter_mut() {
        let Ok((mut t, mut v)) = shown.get_mut(n.entity) else {
            continue;
        };
        let home = *n.home.get_or_insert(*t);
        let scale = match n.placement {
            p if BARRIERS.contains(&p) => (!course.hidden(route, p)).then_some(1.0),
            p => {
                let taken = p < 64 && m.taken >> p & 1 != 0;
                match (taken, n.taken) {
                    (true, None) => n.taken = Some(tick),
                    (false, Some(_)) => (n.taken, n.back) = (None, Some(tick)),
                    _ => {}
                }
                let since = |at: Option<u32>| at.map(|a| tick.saturating_sub(a));
                power_scale(since(n.taken), since(n.back))
            }
        };
        let Some(scale) = scale else {
            v.set_if_neq(Visibility::Hidden);
            continue;
        };
        v.set_if_neq(Visibility::Inherited);
        let pose = match POWER_UPS.contains(&n.placement) {
            true => home
                .with_rotation(home.rotation * spin(tick))
                .with_scale(home.scale * scale),
            false => home,
        };
        if *t != pose {
            *t = pose;
        }
    }
}

/// The missiles' models and where each was last tick.
#[derive(Resource, Default)]
struct Missiles {
    scene: Option<Handle<WorldAsset>>,
    models: Vec<Entity>,
    last: Vec<[f32; 3]>,
}

/// A missile's pose from `from` to `to`: its nose (+z) along the way, level when it did not move.
pub fn missile_pose(from: [f32; 3], to: [f32; 3]) -> Quat {
    let dir = (Vec3::from(to) - Vec3::from(from)).normalize_or_zero();
    match dir == Vec3::ZERO {
        true => Quat::IDENTITY,
        false => Transform::IDENTITY.looking_to(-dir, Vec3::Y).rotation,
    }
}

/// The match's missiles, one model each, nose along the way they went since last frame.
fn missiles(
    game: Query<&MatchView>,
    assets: Res<AssetServer>,
    mut state: ResMut<Missiles>,
    mut models: Query<(&mut Transform, &mut Visibility)>,
    mut commands: Commands,
) {
    let Some(m) = game.iter().find(|m| m.race) else {
        return;
    };
    let s = &mut *state;
    let scene = (s.scene)
        .get_or_insert_with(|| {
            let gltf = projectiles_gltf(PACK_WEAPONS);
            assets.load(format!("{gltf}#Scene{MISSILE_SCENE}"))
        })
        .clone();
    let flying = &m.racing.missiles;
    while s.models.len() < flying.len() {
        let e = commands.spawn((
            WorldAssetRoot(scene.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        s.models.push(e.id());
    }
    for (i, &e) in s.models.iter().enumerate() {
        let Ok((mut t, mut v)) = models.get_mut(e) else {
            continue;
        };
        match flying.get(i) {
            Some(&(pos, _)) => {
                let from = s.last.get(i).copied().unwrap_or(pos);
                let turn = match from == pos {
                    true => t.rotation,
                    false => missile_pose(from, pos),
                };
                *t = Transform::from_translation(Vec3::from(pos))
                    .with_rotation(turn)
                    .with_scale(Vec3::splat(MISSILE_SCALE));
                v.set_if_neq(Visibility::Inherited);
            }
            None => {
                v.set_if_neq(Visibility::Hidden);
            }
        }
    }
    s.last = flying.iter().map(|f| f.0).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rules' animations show the pack's clips: 84 the kick's while it runs, else the knock's;
    /// 112 the death's; riding as is. Racers are classes 0xB-0xF.
    #[test]
    fn racers_show_their_clips() {
        assert_eq!(clip(5, false), RIDE);
        assert_eq!(clip(84, true), 0x354);
        assert_eq!(clip(84, false), 0x54);
        assert_eq!(clip(112, false), 0x170);
        assert!([0xB, 0xC, 0xF].into_iter().all(racer));
        assert!(![0, 8, 9, 0x10, 0x14].into_iter().any(racer));
        // The human's kick plays at 1.35, the rest at 1 (C).
        assert_eq!(rate(KICKING, HUMAN), 1.35);
        assert_eq!(rate(KICKING, 0xC), 1.0);
        assert_eq!(rate(RIDE, HUMAN), 1.0);
    }

    /// A taken power-up shrinks over 4 frames and hides (V L1 541-544); back, it grows with the
    /// overshoot (V L1 551-557, of 7: 1.82, 3.64, 5.46, 7.0, 7.16, 7.01, 7.0); it spins 3.03° a frame.
    #[test]
    fn power_ups_shrink_grow_and_spin() {
        let taken: Vec<_> = (0..5).map(|k| power_scale(Some(k), None)).collect();
        assert_eq!(taken, [Some(0.75), Some(0.5), Some(0.25), None, None]);
        let back: Vec<_> = (0..7)
            .map(|k| power_scale(None, Some(k)).unwrap() * 7.0)
            .collect();
        let want = [1.82, 3.64, 5.46, 7.0, 7.16, 7.01, 7.0];
        assert!(
            back.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.01),
            "{back:?}"
        );
        assert_eq!(power_scale(None, None), Some(1.0));
        let turned = spin(10) * Vec3::Z;
        let want = Quat::from_rotation_y(30.3f32.to_radians()) * Vec3::Z;
        assert!((turned - want).length() < 1e-4);
        assert!((0..=10).all(owns) && !owns(11));
    }

    /// The missile's nose along its way.
    #[test]
    fn a_missile_points_along_its_way() {
        let nose = missile_pose([0.0; 3], [10.0, 0.0, 0.0]) * Vec3::Z;
        assert!((nose - Vec3::X).length() < 1e-5, "{nose}");
        assert_eq!(missile_pose([1.0; 3], [1.0; 3]), Quat::IDENTITY);
    }
}
