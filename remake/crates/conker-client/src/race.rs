//! Race (`../lab/research/mp-original/race.md`; this batch's oracle runs in
//! `race-client-scratch/`): scenarios 11 and 12 on map 33, every player a racer on a hoverboard
//! (conker-sim's `board`, conker-rules' `race`), from the pack's `maps/33/race.json` ([`Course`])
//! and [`RaceView`]:
//! - the route the match names (`RaceView::route`): its lap of nav nodes is the prediction's track
//!   (the steering's clamp, the autopilot's point, the respawns), and the barriers it does not show
//!   are moved out of the client's collision as the server's (20000 up) and hidden ([`props`]);
//! - the racers: every player, the human (class 0xB) 136 or its pick, the bots (0xC-0xF) 144 in
//!   their helmets; the board (`props.gltf` scene `board.scene`) on joint 23 and a helmet (classes
//!   0xD-0xF) on joint 12, all three
//!   hidden from the death to the respawn, with the blast where it died ([`gear`]);
//!   their clips ([`clip`], [`rate`]); the exhaust glow and the canister's boost flare
//!   ([`flares`]);
//! - the effects the pack has (`hud/race_*.png`): the countdown lamps after the go ([`lamps`]),
//!   the missiles' trails ([`trails`]); the crowd's panels cheer in [`crate::segments`];
//! - the power-ups (placements 4-10) spinning, shrinking when taken and growing back with an
//!   overshoot ([`power_scale`]);
//! - the missiles in flight (`projectiles.gltf#Scene4` at 1.0, V m1 DL 224), nose along their way;
//! - the race camera behind the board, farther back on the grid ([`camera`]).
//!
//! The HUD is [`crate::hud`]'s, the sounds [`crate::audio`]'s.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::gltf::Gltf;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use conker_assets::race::{Race, Route};
use conker_assets::{PACK_WEAPONS, projectiles_gltf, props_gltf};
use conker_camera::DT;
use conker_camera::math::spring;
use conker_rules::Fighter;
use conker_rules::net::Fighting;
use conker_rules::race::{DEAD, Track};
use conker_sim::board::KICK;
use conker_sim::follow::Follow;
use conker_sim::movement::PlayerState;
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
/// The canister's boost flare (V w3c DLs 480-510: `hud/race_boost.png`, white × its alpha × 0x6B-0x71,
/// 348-399 wide, 74-76 tall, over the racer 33-90 up, −6…19 ahead and 24-26 to its right: the
/// racer's frame faces +z, so its right is −x), facing the camera (I).
const FLARE: [f32; 2] = [370.0, 75.0];
const FLARE_AT: [f32; 3] = [-24.0, 60.0, 0.0];
const FLARE_ALPHA: f32 = 0x6E as f32 / 255.0;
/// The exhaust glow (V w1, w3c: `hud/race_exhaust_{0..3}.png`, one a frame, × env alpha 0xE1-0xFD;
/// 84-91 × 78-85, 7-35 behind, 22-25 to the right, 42-99 up while boosting), facing the camera. I:
/// that size always (17-178 wide without a boost, its law not decoded).
const EXHAUST: [f32; 2] = [88.0, 82.0];
const EXHAUST_AT: [f32; 3] = [-24.0, 60.0, -25.0];
const EXHAUST_ALPHA: f32 = 0xF5 as f32 / 255.0;
/// The countdown lamps' lower two (V cd 142-285, it 206-270: `hud/race_lamp.png` green × its alpha,
/// 845-857 square, facing the camera). The top one and every red stage stay at alpha 0, so only
/// these show, from the go for about this many frames, flickering 0x11-0x84 ([`lamp_alpha`]; I: the
/// law, and their centres' drift toward the camera is not drawn).
const LAMPS: [[f32; 3]; 2] = [[282.0, 1489.0, -7538.0], [290.0, 1365.0, -7545.0]];
const LAMP_SIZE: f32 = 850.0;
const LAMP_FRAMES: u32 = 70;
/// A missile's trail (V w1 DLs 218-240): a ribbon through its last 7 frames, 58 wide, with
/// `hud/race_trail.png` across it, white × its alpha × 1 − n/6 at point n from the missile
/// ([`ribbon`]). I: facing the camera; the second, narrower strip is not drawn.
const TRAIL_POINTS: usize = 7;
const TRAIL_WIDTH: f32 = 58.0;
/// A missile this far from its last place is another one (the list's order changed).
const TRAIL_JUMP: f32 = 600.0;

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

/// A racer's clip rate: a human's kick at 1.35, whatever it picked, the rest at 1 (C: the Tedi's
/// rate table; V anim1/2: the bots' kick at 1; V `characters-scratch/m14c` kick7: a human on pick 7,
/// class 0xC, model 144, kicks at 1.35).
pub fn rate(clip: u16, human: bool) -> f32 {
    match clip == KICKING && human {
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
/// go, then [`CAM`] with its heights sprung, [`heights`]), through walls (C `func_15123A54`: a
/// board's camera casts no wall ray). The board's stick steers itself (conker-sim's `board`: no
/// camera yaw), so the stick needs no override.
pub fn camera(follow: &mut Follow, cam: &mut Cam, f: &Fighter, tick: u32) {
    follow.cam.scope.on = false;
    let law = if tick < f.ride.go { &GRID } else { &CAM };
    crate::tank::behind(follow, cam, &f.body, law, None);
    let c = &mut follow.cam;
    match cam.1.as_mut() {
        Some(s) if tick >= f.ride.go => [c.eye[1], c.at[1]] = heights(s, &f.body, c.eye[1]),
        _ => cam.1 = Some([c.eye[1], 0.0, c.at[1], 0.0]),
    }
}

/// The look-at's lift over the board's floor: mode 8's 0.75 of the head (130, fit) and the view's
/// 123 (C `func_150D1530`'s look offset).
const LOOK: f32 = 220.5;

/// The board camera's heights after the go, `s` = [eye, its velocity, look-at, its velocity]: the
/// eye springs (4, 8) toward `eye` ([`CAM`]'s height over the racer; I: a fit, V g5c r1 480-850 rms
/// 12.9 against 42.4 unsprung); the look-at springs as mode 8's (4, 8, doubled going down) toward
/// the floor + air · min(air / 300, 1) + [`LOOK`] (C `conker_camera`'s look-at height; V r1 rms 4.1
/// against 21.3 at the racer + 195).
fn heights(s: &mut [f32; 4], b: &PlayerState, eye: f32) -> [f32; 2] {
    let [e, ev, at, av] = s;
    spring(e, eye, ev, 4.0, 8.0, DT);
    let air = b.pos[1] - b.floor_y;
    let look = b.floor_y + air * (air / 300.0).clamp(0.0, 1.0) + LOOK;
    let (k, damp) = if look < *at { (8.0, 16.0) } else { (4.0, 8.0) };
    spring(at, look, av, k, damp, DT);
    [*e, *at]
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
        app.add_systems(Update, (hang, gear, flares, props, missiles, lamps, trails));
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
pub(crate) struct Gear(bool);

/// Once a racer's model is in (every player in Race, any pick), the board on its joint and its
/// class's helmet on its own (`props.gltf` in the joint's frame, model units, as a held weapon: V
/// k1, cls). The helmets go by class (0xD-0xF, C `func_15083384`): any other pick rides bare. The
/// glTF loads after the characters' (one GLB at a time, `render::load_characters`).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hang(
    players: Query<(Entity, &Combat), (With<Rig>, Without<Gear>)>,
    game: Query<&MatchView>,
    children: Query<&Children>,
    names: Query<&Name>,
    course: Res<Course>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut gltf: Local<Option<Handle<Gltf>>>,
    mut commands: Commands,
) {
    if !game.iter().any(|m| m.race) {
        return;
    }
    for (player, c) in &players {
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

/// A racer's boost flare or exhaust glow.
#[derive(Component)]
struct Flare;

/// A frame's draw for effect `k` (an exhaust's frame, a lamp's flicker), the same on every client.
fn noise(tick: u32, k: u32) -> u32 {
    let x = tick.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    (x ^ x >> 15).wrapping_mul(0x2C1B_3C6D) >> 16
}

/// The exhaust glow's frame (0-3) for racer `slot` at frame `tick`: one at random each frame (V
/// runs such as 924, 923, 923, 925; I: the draw).
pub fn exhaust_frame(tick: u32, slot: u16) -> usize {
    (noise(tick, u32::from(slot)) % 4) as usize
}

/// Lamp `lamp`'s alpha at frame `tick` (`go` the boards' go, 0 not yet known): 0x11-0x84 of 255
/// for [`LAMP_FRAMES`] from the go, else `None` (not drawn).
pub fn lamp_alpha(go: u32, tick: u32, lamp: u32) -> Option<f32> {
    (go > 0 && (go..go + LAMP_FRAMES).contains(&tick))
        .then(|| (0x11 + noise(tick, 0x100 + lamp) % 0x74) as f32 / 255.0)
}

/// Every racer's exhaust glow while it is alive ([`EXHAUST`]), and its boost flare while a
/// canister's boost runs ([`FLARE`]; the local racer's boost is its prediction's), facing the
/// camera.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn flares(
    game: Query<&MatchView>,
    racers: Query<(
        Entity,
        &Combat,
        &Transform,
        &Gear,
        Option<&Fighting>,
        Has<Predicted>,
    )>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    sprites: Option<Res<crate::beach::Sprites>>,
    (assets, timeline): (Res<AssetServer>, Res<LocalTimeline>),
    mut quads: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (With<Flare>, Without<Gear>),
    >,
    mut made: Local<HashMap<Entity, [Entity; 2]>>,
    // The exhaust's four frames, then the flare.
    mut looks: Local<Vec<Handle<StandardMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let (Some(m), Some(sprites)) = (game.iter().find(|m| m.race), sprites) else {
        return;
    };
    if looks.is_empty() {
        let mut look = |file: String, alpha: f32| {
            materials.add(crate::traps::material(&assets.load(file), [255; 3], alpha))
        };
        let glows = (0..4).map(|f| look(format!("hud/race_exhaust_{f}.png"), EXHAUST_ALPHA));
        *looks = glows.collect();
        looks.push(look("hud/race_boost.png".into(), FLARE_ALPHA));
    }
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    let tick = timeline.tick().0;
    for (racer, c, at, gear, fighting, predicted) in &racers {
        let boost = match fighting {
            Some(f) if predicted => f.state.boost,
            _ => (m.racing.racers.iter().find(|r| r.slot == c.slot)).map_or(0, |r| r.boost),
        };
        let pair = *made.entry(racer).or_insert_with(|| {
            let mut quad = |look: &Handle<StandardMaterial>, [w, h]: [f32; 2], at: Vec3| {
                let place = Transform::from_translation(at).with_scale(Vec3::new(w, h, 1.0));
                let quad = (Mesh3d(sprites.quad.clone()), MeshMaterial3d(look.clone()));
                (commands.spawn((Flare, quad, place, Visibility::Hidden, ChildOf(racer)))).id()
            };
            [
                quad(&looks[4], FLARE, Vec3::from(FLARE_AT)),
                quad(&looks[0], EXHAUST, Vec3::from(EXHAUST_AT)),
            ]
        });
        let glow = &looks[exhaust_frame(tick, c.slot)];
        for (e, shown) in pair.into_iter().zip([boost > 0, !gear.0]) {
            let Ok((mut t, mut v, mut look)) = quads.get_mut(e) else {
                continue;
            };
            v.set_if_neq(match shown {
                true => Visibility::Inherited,
                false => Visibility::Hidden,
            });
            t.rotation = at.rotation.inverse() * facing;
            if e == pair[1] && look.0 != *glow {
                look.0 = glow.clone();
            }
        }
    }
}

/// The countdown lamps after the go ([`LAMPS`]), facing the camera.
#[allow(clippy::too_many_arguments)]
fn lamps(
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    sprites: Option<Res<crate::beach::Sprites>>,
    assets: Res<AssetServer>,
    mut made: Local<Vec<(Entity, Handle<StandardMaterial>)>>,
    mut quads: Query<(&mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let (Some(m), Some(sprites)) = (game.iter().find(|m| m.race), sprites) else {
        return;
    };
    if made.is_empty() {
        let texture = assets.load("hud/race_lamp.png");
        for at in LAMPS {
            let look = materials.add(crate::traps::material(&texture, [0, 255, 0], 0.0));
            let place =
                Transform::from_translation(Vec3::from(at)).with_scale(Vec3::splat(LAMP_SIZE));
            let quad = (Mesh3d(sprites.quad.clone()), MeshMaterial3d(look.clone()));
            made.push((commands.spawn((quad, place, Visibility::Hidden)).id(), look));
        }
    }
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    let tick = timeline.tick().0;
    for (k, (e, look)) in made.iter().enumerate() {
        let alpha = lamp_alpha(m.racing.go, tick, k as u32);
        if let Ok((mut t, mut v)) = quads.get_mut(*e) {
            v.set_if_neq(match alpha {
                Some(_) => Visibility::Inherited,
                None => Visibility::Hidden,
            });
            t.rotation = facing;
        }
        if let Some(a) = alpha
            && let Some(mut look) = materials.get_mut(look)
        {
            look.base_color.set_alpha(a);
        }
    }
}

/// A missile's trail: its places, the newest first, and its ribbon once it has two.
#[derive(Default)]
struct Trail {
    points: Vec<Vec3>,
    drawn: Option<(Entity, Handle<Mesh>)>,
}

/// A trail's ribbon through `points` (the newest first), [`TRAIL_WIDTH`] across and facing `eye`:
/// u 0 → 1 across it, white × 1 − n/6 at point n.
pub fn ribbon(points: &[Vec3], eye: Vec3) -> Mesh {
    let n = points.len();
    let (mut at, mut uv, mut rgba) = (Vec::new(), Vec::new(), Vec::new());
    for (k, &p) in points.iter().enumerate() {
        let along = points[k.saturating_sub(1)] - points[(k + 1).min(n - 1)];
        let side = along.cross(eye - p).normalize_or_zero() * TRAIL_WIDTH / 2.0;
        let alpha = 1.0 - k as f32 / (TRAIL_POINTS - 1) as f32;
        for (edge, u) in [(p - side, 0.0), (p + side, 1.0)] {
            at.push(edge.to_array());
            uv.push([u, 0.5]);
            rgba.push([1.0, 1.0, 1.0, alpha]);
        }
    }
    let quads = (0..n.saturating_sub(1) as u32).flat_map(|k| {
        let a = 2 * k;
        [a, a + 1, a + 2, a + 1, a + 3, a + 2]
    });
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, at)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, rgba)
    .with_inserted_indices(Indices::U32(quads.collect()))
}

/// Each missile's trail ([`TRAIL_POINTS`] of its places, [`ribbon`]).
#[allow(clippy::too_many_arguments)]
fn trails(
    game: Query<&MatchView>,
    camera: Query<&GlobalTransform, (With<Camera3d>, With<FreeCameraState>)>,
    assets: Res<AssetServer>,
    mut trails: Local<Vec<Trail>>,
    mut look: Local<Option<Handle<StandardMaterial>>>,
    mut shown: Query<&mut Visibility>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut commands: Commands,
) {
    let (Some(m), Ok(eye)) = (game.iter().find(|m| m.race), camera.single()) else {
        return;
    };
    let eye = eye.translation();
    let look = (look.get_or_insert_with(|| {
        let texture = assets.load("hud/race_trail.png");
        materials.add(crate::traps::material(&texture, [255; 3], 1.0))
    }))
    .clone();
    let flying = &m.racing.missiles;
    let n = trails.len().max(flying.len());
    trails.resize_with(n, Trail::default);
    for (i, trail) in trails.iter_mut().enumerate() {
        match (flying.get(i).map(|f| Vec3::from(f.0)), trail.points.first()) {
            (None, _) => trail.points.clear(),
            (Some(at), Some(&last)) if at == last => {}
            (Some(at), Some(&last)) if at.distance(last) > TRAIL_JUMP => trail.points = vec![at],
            (Some(at), _) => {
                debug!("race: missile {i} at {at}");
                trail.points.insert(0, at);
                trail.points.truncate(TRAIL_POINTS);
            }
        }
        let show = trail.points.len() >= 2;
        match (&trail.drawn, show) {
            (Some((e, mesh)), _) => {
                if let Ok(mut v) = shown.get_mut(*e) {
                    v.set_if_neq(match show {
                        true => Visibility::Inherited,
                        false => Visibility::Hidden,
                    });
                }
                if show && let Some(mut mesh) = meshes.get_mut(mesh) {
                    *mesh = ribbon(&trail.points, eye);
                }
            }
            (None, true) => {
                let mesh = meshes.add(ribbon(&trail.points, eye));
                let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(look.clone())));
                trail.drawn = Some((e.id(), mesh));
            }
            (None, false) => {}
        }
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
    use bevy::mesh::VertexAttributeValues;

    /// The board camera's heights through a jump (V g5c r1 reads 545-575, the human racing route 11:
    /// its y, floor, speed, then the drawn eye's y (the CSV's + 123) and the look-at's): sprung, they
    /// stay within 45 and 12 of the oracle's where [`CAM`] unsprung is 129 and 40 off.
    #[test]
    fn the_board_camera_springs_its_heights_as_the_oracle() {
        let r1 = [
            (716.2, 674.3, 79.0, 895.1, 884.4),
            (718.6, 676.8, 80.0, 901.9, 887.2),
            (719.1, 679.4, 81.0, 908.5, 890.1),
            (717.6, 682.0, 82.0, 914.7, 892.9),
            (714.3, 684.4, 83.0, 919.9, 895.6),
            (709.0, 675.0, 84.0, 923.8, 897.8),
            (701.9, 660.6, 85.0, 923.4, 896.9),
            (692.8, 645.6, 86.0, 922.4, 892.3),
            (681.9, 630.5, 87.0, 920.2, 884.4),
            (669.0, 615.3, 88.0, 916.9, 873.7),
            (654.3, 600.0, 89.0, 913.2, 861.1),
            (637.6, 584.2, 90.0, 909.4, 846.8),
            (619.1, 568.7, 91.0, 906.0, 831.4),
            (598.7, 552.5, 92.0, 903.2, 815.1),
            (576.3, 549.0, 93.0, 900.6, 799.9),
            (552.1, 549.0, 94.0, 897.6, 787.3),
            (549.1, 549.1, 95.0, 893.6, 778.3),
            (549.2, 549.2, 96.0, 888.1, 772.6),
            (549.7, 549.2, 97.0, 881.1, 769.7),
            (551.1, 549.3, 98.0, 872.5, 769.7),
            (554.3, 549.3, 99.0, 862.2, 769.7),
            (559.3, 549.3, 100.0, 850.3, 769.8),
            (566.0, 549.3, 101.0, 838.1, 769.8),
            (574.5, 549.2, 102.0, 826.7, 769.9),
            (582.7, 549.2, 103.0, 817.0, 770.2),
            (589.0, 549.1, 104.0, 809.4, 770.6),
            (593.4, 550.8, 105.0, 804.0, 771.2),
            (595.9, 553.9, 106.0, 800.6, 772.0),
            (596.5, 556.9, 107.0, 799.0, 773.1),
            (595.2, 560.1, 108.0, 798.6, 774.4),
            (592.0, 563.4, 109.0, 799.0, 775.9),
        ];
        let mut s = [r1[0].3, 0.0, r1[0].4, 0.0];
        let (mut sprung, mut flat) = ([0f32; 2], [0f32; 2]);
        for &(y, floor_y, speed, eye, at) in &r1[1..] {
            let b = PlayerState {
                pos: [0.0, y, 0.0],
                floor_y,
                ..PlayerState::spawn([0.0; 3], 0)
            };
            let up = y + CAM.rest[1] + (CAM.full[1] - CAM.rest[1]) * (speed / CAM.top).min(1.0);
            let [e, a] = heights(&mut s, &b, up);
            sprung = [
                sprung[0].max((e - eye).abs()),
                sprung[1].max((a - at).abs()),
            ];
            flat = [
                flat[0].max((up - eye).abs()),
                flat[1].max((y + CAM.rest[2] - at).abs()),
            ];
        }
        assert!(sprung[0] < 45.0 && sprung[1] < 12.0, "{sprung:?}");
        assert!(flat[0] > 125.0 && flat[1] > 40.0, "{flat:?}");
    }

    /// The rules' animations show the pack's clips: 84 the kick's while it runs, else the knock's;
    /// 112 the death's; riding as is.
    #[test]
    fn racers_show_their_clips() {
        assert_eq!(clip(5, false), RIDE);
        assert_eq!(clip(84, true), 0x354);
        assert_eq!(clip(84, false), 0x54);
        assert_eq!(clip(112, false), 0x170);
        // A human's kick plays at 1.35 (any pick, V kick7), a bot's and the rest at 1.
        assert_eq!(rate(KICKING, true), 1.35);
        assert_eq!(rate(KICKING, false), 1.0);
        assert_eq!(rate(RIDE, true), 1.0);
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

    /// The exhaust's frame is one of four, drawn anew each frame (V runs 924, 923, 923, 925 …);
    /// the lamps show only for 70 frames from the go, at 0x11-0x84 (and not before the go is known).
    #[test]
    fn exhaust_and_lamps_follow_the_handoff() {
        let frames: Vec<_> = (0..400).map(|t| exhaust_frame(t, 3)).collect();
        assert!((0..4).all(|f| frames.contains(&f)) && frames.iter().all(|&f| f < 4));
        assert_ne!(
            frames,
            (0..400).map(|t| exhaust_frame(t, 4)).collect::<Vec<_>>()
        );
        let go = 210;
        assert_eq!(lamp_alpha(go, go - 1, 0), None);
        assert_eq!(lamp_alpha(go, go + LAMP_FRAMES, 1), None);
        assert_eq!(lamp_alpha(0, 30, 0), None);
        let alphas: Vec<f32> = (go..go + LAMP_FRAMES)
            .flat_map(|t| [0, 1].map(|k| lamp_alpha(go, t, k).unwrap()))
            .collect();
        let (lo, hi) = (0x11 as f32 / 255.0, 0x84 as f32 / 255.0);
        assert!(alphas.iter().all(|&a| (lo..=hi).contains(&a)), "{alphas:?}");
        assert!(alphas.iter().any(|&a| a < 0.2) && alphas.iter().any(|&a| a > 0.4));
    }

    /// The trail (V w1 218-240): 7 points, 58 wide across the view, u 0 → 1 across, alpha 255,
    /// 212, 169, 127, 84, 42, 0 from the missile back.
    #[test]
    fn a_trail_fades_behind_its_missile() {
        let points: Vec<_> = (0..7)
            .map(|k| Vec3::new(200.0 * k as f32, 0.0, 0.0))
            .collect();
        let eye = Vec3::new(0.0, 1000.0, 0.0);
        let mesh = ribbon(&points, eye);
        let Some(VertexAttributeValues::Float32x4(rgba)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("no colours");
        };
        let alphas = rgba.iter().step_by(2).map(|c| c[3] * 255.0);
        let want = [255.0, 212.0, 169.0, 127.0, 84.0, 42.0, 0.0];
        assert!(alphas.zip(want).all(|(a, b)| (a - b).abs() <= 1.0));
        let Some(VertexAttributeValues::Float32x3(at)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        // Seen from above, the ribbon spans z: 58 wide at every point.
        for pair in at.chunks(2) {
            assert!((Vec3::from(pair[0]).distance(Vec3::from(pair[1])) - 58.0).abs() < 1e-3);
            assert_eq!(pair[0][1], 0.0);
        }
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no uvs");
        };
        assert_eq!((uv[0][0], uv[1][0]), (0.0, 1.0));
        assert_eq!(mesh.indices().map(|i| i.len()), Some(6 * 6));
    }

    /// The missile's nose along its way.
    #[test]
    fn a_missile_points_along_its_way() {
        let nose = missile_pose([0.0; 3], [10.0, 0.0, 0.0]) * Vec3::Z;
        assert!((nose - Vec3::X).length() < 1e-5, "{nose}");
        assert_eq!(missile_pose([1.0; 3], [1.0; 3]), Quat::IDENTITY);
    }
}
