//! The modern controls' centre crosshair and hit markers (CONTROLS_RESEARCH.md §3; lane C2b), as
//! quads for the HUD's renderer ([`Quads`], which [`crate::hud`] draws with its own).
//! - The crosshair sits at the view's centre, where the aim ray goes ([`crate::orbit`]), in the
//!   settings' colour and size. If the weapon's muzzle can't reach the point the ray hits (a wall
//!   between them), it greys, and the wall's point the shot would hit is marked ([`blocked`]).
//! - A hit marker shows for [`MARKED`] seconds when the server counts a hit of the local player's
//!   (`Combat::hits`, a headshot `Combat::headshots`, in red): the server tests hits on posed
//!   spheres, the client only on cylinders, so the markers are the server's.

use crate::FollowCamera;
use crate::hud::{Sprite, Tex, frame_point};
use crate::orbit::{self, Orbit};
use crate::settings::{Colour, Settings};
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::prelude::*;
use conker_assets::hud::{Anchor, Horizontal, Rgba, Vertical};
use conker_rules::net::Fighting;
use conker_rules::projectile::{local_to_world, raycast};
use conker_rules::weapon::{Attack, Weapon};
use conker_sim::collision::CollisionWorld;
use conker_sim::net::{Collision, Combat, MatchView, Player};

/// This frame's crosshair and marker quads (frame pixels, centred).
#[derive(Resource, Clone, Debug, Default)]
pub struct Quads(pub Vec<Sprite>);

/// The HUD's frame (`Hud::frame`), which the quads are placed in.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Frame([u16; 2]);

/// How long a hit marker shows, seconds.
pub const MARKED: f32 = 0.3;
/// How far the aim ray looks for a wall (the rules' bullets').
const RANGE: f32 = 20_000.0;
/// Over the reticle and the scope (6-6.5), under the out view's black (7).
const Z: f32 = 6.8;
const GREY: Rgba = [150, 150, 150, 200];

/// The preset's colour.
pub fn colour(c: Colour) -> Rgba {
    match c {
        Colour::Red => [255, 50, 50, 230],
        Colour::Green => [0, 255, 0, 230],
        Colour::Yellow => [255, 255, 0, 230],
        Colour::Blue => [70, 130, 255, 230],
        Colour::Cyan => [0, 255, 255, 230],
    }
}

/// Where a weapon's shots leave (forward, side, up from the feet); none for the melee ones and
/// the flame.
pub fn muzzle(w: &Weapon) -> Option<[f32; 3]> {
    match w.attack {
        Attack::Bullets { muzzles, .. } => muzzles.first().copied(),
        Attack::Spread { muzzles, .. } => Some(muzzles[0]),
        Attack::Ray { origin, .. } => Some(origin),
        Attack::Rocket { muzzle, .. }
        | Attack::Shot { muzzle, .. }
        | Attack::Bolt { muzzle, .. } => Some(muzzle),
        Attack::Melee { .. } | Attack::Flame { .. } | Attack::None => None,
    }
}

/// The aim ray from `start` along `dir` meets the first wall at the crosshair's point; a shot from
/// `muzzle` toward that point that hits a wall first: where it does.
pub fn blocked(world: &CollisionWorld, start: Vec3, dir: Vec3, muzzle: Vec3) -> Option<Vec3> {
    let point = start + dir * raycast(world, start.to_array(), dir.to_array(), RANGE);
    let (to, d) = ((point - muzzle).normalize_or_zero(), muzzle.distance(point));
    let wall = raycast(world, muzzle.to_array(), to.to_array(), d);
    (to != Vec3::ZERO && wall < d - 1.0).then(|| muzzle + to * wall)
}

/// A solid quad of half size `half` centred on frame point `[x, y]`.
fn dot([x, y]: [f32; 2], half: [f32; 2], colour: Rgba) -> Sprite {
    Sprite {
        tex: Tex::Solid,
        uv: [0.0; 4],
        rect: [x - half[0], y - half[1], 2.0 * half[0], 2.0 * half[1]],
        anchor: Anchor(Horizontal::Centre, Vertical::Middle),
        colour,
        z: Z,
    }
}

/// The crosshair at the frame's centre `c`, `size` times the default: four bars round a gap.
pub fn cross(c: [f32; 2], size: f32, colour: Rgba) -> Vec<Sprite> {
    let (gap, len, thick) = (1.2 * size, 2.4 * size, 0.45 * size);
    let mid = gap + len / 2.0;
    [(-mid, 0.0), (mid, 0.0), (0.0, -mid), (0.0, mid)]
        .map(|(dx, dy)| {
            let half = if dx != 0.0 {
                [len / 2.0, thick / 2.0]
            } else {
                [thick / 2.0, len / 2.0]
            };
            dot([c[0] + dx, c[1] + dy], half, colour)
        })
        .to_vec()
}

/// A hit marker at `c`: four diagonal ticks of three squares each, fading over [`MARKED`] (`age`
/// seconds old); white, a headshot's red.
pub fn marker(c: [f32; 2], size: f32, age: f32, head: bool) -> Vec<Sprite> {
    let alpha = (255.0 * (1.0 - age / MARKED).clamp(0.0, 1.0)) as u8;
    let colour = if head {
        [255, 40, 40, alpha]
    } else {
        [255, 255, 255, alpha]
    };
    ticks(c, size, 0.35 * size, colour, Z)
}

/// The marker's black outline, under it ([`marker`]'s ticks, wider; they join): the white stays
/// readable over held fire's muzzle flash.
pub fn outline(c: [f32; 2], size: f32, age: f32) -> Vec<Sprite> {
    let alpha = (255.0 * (1.0 - age / MARKED).clamp(0.0, 1.0)) as u8;
    ticks(c, size, 0.6 * size, [0, 0, 0, alpha], Z - 0.05)
}

/// Four diagonal ticks of three squares of half size `half` round `c`.
fn ticks(c: [f32; 2], size: f32, half: f32, colour: Rgba, z: f32) -> Vec<Sprite> {
    let corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    (corners.iter())
        .flat_map(|&(sx, sy)| {
            [2.6f32, 3.4, 4.2].map(|d| {
                let k = d * size * core::f32::consts::FRAC_1_SQRT_2;
                let mut q = dot([c[0] + sx * k, c[1] + sy * k], [half; 2], colour);
                q.z = z;
                q
            })
        })
        .collect()
}

/// The local player's hit counts seen last, and the marker showing (since, a headshot).
#[derive(Default)]
pub struct Marks {
    last: Option<(u16, u16)>,
    shown: Option<(f32, bool)>,
}

impl Marks {
    /// The server's counts `now` (hits, headshots) at `t`: a hit more starts a marker (a match's
    /// restart zeroes them: no marker).
    fn see(&mut self, now: (u16, u16), t: f32) {
        if let Some((hits, heads)) = self.last.replace(now)
            && now.0 > hits
        {
            self.shown = Some((t, now.1 > heads));
        }
    }
}

/// This frame's [`Quads`], after the orbit camera's view and before the HUD draws them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update(
    time: Res<Time<Real>>,
    frame: Res<Frame>,
    (orbit, settings, world): (Res<Orbit>, Res<Settings>, Res<Collision>),
    game: Query<&MatchView>,
    own: Query<(&Player, &Combat, &FollowCamera, Option<&Fighting>)>,
    view: Single<(&Camera, &Transform, &FreeCameraState)>,
    fixed: Res<Time<Fixed>>,
    mut marks: Local<Marks>,
    mut quads: ResMut<Quads>,
) {
    quads.0.clear();
    let (camera, transform, free) = *view;
    let (Ok((p, c, cam, fighting)), Some(window)) = (own.single(), camera.logical_viewport_size())
    else {
        return;
    };
    let t = time.elapsed_secs();
    marks.see((c.hits, c.headshots), t);
    let live = game.iter().any(|m| m.phase == 0);
    if !orbit.on || free.enabled || !live {
        return;
    }
    let size = settings.crosshair.size;
    let centre = frame_point(frame.0, window, window / 2.0);
    if let Some((at, head)) = marks.shown {
        match t - at < MARKED {
            true => {
                quads.0.extend(outline(centre, size, t - at));
                quads.0.extend(marker(centre, size, t - at, head));
            }
            false => marks.shown = None,
        }
    }
    let seated = fighting.is_some_and(|f| f.state.gun.is_some());
    if c.health == 0 || c.out > 0 || cam.tank.is_some() || seated || cam.follow.cam.scope.on {
        return;
    }
    // The muzzle where the drawn body stands, turned to the aim (the shot's frame faces it).
    let f = fixed.overstep_fraction().clamp(0.0, 1.0);
    let feet = cam.drawn.between(cam.drawn.frame(p, &cam.follow), f).body;
    let held = fighting.and_then(|f| f.state.weapon().filter(|_| f.state.drawn));
    let at = held.and_then(muzzle).and_then(|m| {
        let muzzle = Vec3::from(local_to_world(feet.to_array(), orbit.aim_yaw(), m));
        let (start, _) = orbit::arm(&orbit, feet, &world.0);
        blocked(&world.0, start, orbit.look().0, muzzle)
    });
    let shown = at.and_then(|w| {
        camera
            .world_to_viewport(&GlobalTransform::from(*transform), w)
            .ok()
    });
    let colour = colour(settings.crosshair.colour);
    match shown {
        Some(w) => {
            quads.0.extend(cross(centre, size, GREY));
            quads.0.push(dot(
                frame_point(frame.0, window, w),
                [0.6 * size; 2],
                colour,
            ));
        }
        None => quads.0.extend(cross(centre, size, colour)),
    }
}

/// The crosshair and markers for the HUD's frame `frame` (windowed clients).
pub fn build(app: &mut App, frame: [u16; 2]) {
    app.insert_resource(Frame(frame));
    app.init_resource::<Quads>();
    app.add_systems(Update, update.after(crate::orbit::camera));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_shows_for_a_hit_the_server_counted() {
        let mut m = Marks::default();
        m.see((3, 1), 0.0);
        assert_eq!(m.shown, None, "the counts already there are no hit");
        m.see((4, 1), 1.0);
        assert_eq!(m.shown, Some((1.0, false)));
        m.see((5, 2), 2.0);
        assert_eq!(m.shown, Some((2.0, true)));
        m.shown = None;
        m.see((0, 0), 3.0);
        assert_eq!(m.shown, None, "a restart");
        let quads = marker([146.0, 108.0], 1.0, 0.15, true);
        assert_eq!(quads.len(), 12);
        assert!(quads.iter().all(|q| q.colour == [255, 40, 40, 127]));
        assert_eq!(cross([146.0, 108.0], 2.0, GREY).len(), 4);
    }

    /// The aim ray from the camera's line hits a far wall; a wall between the muzzle and that point
    /// (the muzzle below a ledge the line passes over) blocks the shot there.
    /// The marker's outline: black, under it, each square wider round the same centre.
    #[test]
    fn the_marker_is_outlined_under_it() {
        let (m, o) = (
            marker([10.0, 20.0], 2.0, 0.0, false),
            outline([10.0, 20.0], 2.0, 0.0),
        );
        assert_eq!(m.len(), o.len());
        for (m, o) in m.iter().zip(&o) {
            assert_eq!((o.colour, o.z < m.z), ([0, 0, 0, 255], true));
            assert!(o.rect[2] > m.rect[2] && o.rect[3] > m.rect[3]);
            let mid = |r: [f32; 4]| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
            assert_eq!(mid(o.rect), mid(m.rect));
        }
    }

    #[test]
    fn a_wall_between_the_muzzle_and_the_crosshairs_point_blocks_it() {
        let big = 5000.0;
        let wall = |x: f32, top: f32| {
            [
                ([[x, -big, -big], [x, top, -big], [x, -big, big]], 5),
                ([[x, top, -big], [x, top, big], [x, -big, big]], 5),
            ]
        };
        let world = CollisionWorld::new(wall(1000.0, big).into_iter().chain(wall(100.0, 50.0)));
        let (start, dir) = (Vec3::new(0.0, 67.5, 0.0), Vec3::X);
        let low = Vec3::new(10.0, 40.0, 0.0);
        let hit = blocked(&world, start, dir, low).expect("the ledge blocks");
        assert!((hit.x - 100.0).abs() < 1e-3, "{hit}");
        assert_eq!(
            blocked(&world, start, dir, Vec3::new(10.0, 60.0, 0.0)),
            None
        );
        // Every weapon that shoots has a muzzle; the melee ones none.
        let twin = conker_rules::weapon::weapon(0x09).unwrap();
        assert_eq!(muzzle(twin).map(|m| m[2] > 40.0), Some(true));
        assert_eq!(muzzle(conker_rules::weapon::weapon(0x21).unwrap()), None);
    }
}
