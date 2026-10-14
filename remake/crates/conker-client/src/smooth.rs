//! What the screen shows of the local player after a server correction. A rollback moves the
//! predicted body at once: the simulation goes on from the server's state, exactly. The drawing
//! instead starts where (and as it faces) the body was drawn and blends to the predicted body over
//! [`BLEND`] live
//! ticks or more (at most [`MAX_STEP`] a tick), and a camera of its own follows the drawn body
//! meanwhile, then hands the view back to the follow camera over [`HANDOFF`] ticks (lightyear's
//! visual correction, for the model and the camera together). The follow camera itself, which the rules and the input read, is untouched. A jump
//! longer than [`SNAP`] (a respawn on another record, a lift ride) is drawn at once.

use crate::FollowCamera;
use bevy::prelude::*;
use conker_rules::Fighter;
use conker_sim::collision::CollisionWorld;
use conker_sim::follow::Follow;
use conker_sim::movement::{Input, PlayerState, angle_diff};
use conker_sim::net::Player;
use lightyear::prelude::LocalTimeline;

/// Live ticks a correction takes at least to blend away, and the most it moves the drawn body in
/// one (game units; a run is 35 a tick).
pub const BLEND: u32 = 8;
pub const MAX_STEP: f32 = 20.0;
/// Live ticks the drawn body's camera takes to hand the view back to the follow camera.
pub const HANDOFF: u32 = 8;
/// Corrections longer than this (game units) are drawn at once: less than the two closest spawn
/// records of scene 0x2D (673, V: the slice's spawns), more than a grenade's knock-back (about 400)
/// and the longest correction the browser runs saw (318).
pub const SNAP: f32 = 600.0;

/// The local player as drawn ([`crate::FollowCamera::drawn`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Drawn {
    /// The predicted position and facing the last live tick (or correction) left.
    pub last: Option<([f32; 3], u16)>,
    /// Drawn minus predicted position, and facing (game angle units).
    pub offset: Vec3,
    pub turn: f32,
    /// Live ticks left to blend the offset away, then for the camera's hand-off.
    pub blend: u32,
    pub handoff: u32,
    /// The camera following the drawn body, until the hand-off ends.
    pub camera: Option<Follow>,
    /// What was drawn when the last live tick started, for the frames between ticks
    /// ([`between`](Self::between)); none across a cut.
    pub prev: Option<Frame>,
}

/// What a frame draws of the local player: the body where and as it faces, and the view (eye,
/// look-at).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub body: Vec3,
    pub facing: u16,
    pub eye: Vec3,
    pub at: Vec3,
}

impl Drawn {
    /// A rollback left the predicted body at `pos` facing `facing`; `follow` is the follow camera
    /// as drawn so far. Returns the correction (how far the body moved and turned), if it did.
    pub fn correct(&mut self, pos: [f32; 3], facing: u16, follow: &Follow) -> Option<(Vec3, i32)> {
        let (last, last_facing) = self.last.replace((pos, facing))?;
        let jump = Vec3::from_array(pos) - Vec3::from_array(last);
        let turn = angle_diff(facing, last_facing);
        if jump == Vec3::ZERO && turn == 0 {
            return None;
        }
        if jump.length() > SNAP {
            self.reset(pos, facing);
        } else {
            self.offset -= jump;
            self.turn -= turn as f32;
            self.blend = BLEND.max((self.offset.length() / MAX_STEP).ceil() as u32);
            // During a hand-off the view keeps its weight (it climbs back over the blend).
            if self.camera.is_none() {
                (self.camera, self.handoff) = (Some(*follow), HANDOFF);
            }
        }
        Some((jump, turn))
    }

    /// Draw `pos` and `facing` as they are from now on (a respawn's camera cut).
    pub fn reset(&mut self, pos: [f32; 3], facing: u16) {
        *self = Self {
            last: Some((pos, facing)),
            ..default()
        };
    }

    /// One live tick, after the follow camera's: [`tick`](Self::tick), then [`camera`](Self::camera)
    /// steps as `follow` did, with the rules' view of `f` at `tick` but the drawn body. A cut of
    /// `follow` (a respawn the server decided, a lift ride) ends the correction.
    pub fn step(
        &mut self,
        f: &Fighter,
        tick: u32,
        follow: &Follow,
        input: &Input,
        world: &CollisionWorld,
    ) {
        self.tick(f.body.pos, f.body.facing);
        let mut view = crate::camera_view(f, tick);
        (view.pos, view.floor_y) = (self.body(view.pos), view.floor_y + self.offset.y);
        let Some(own) = &mut self.camera else {
            return;
        };
        own.step_view(view, input, world);
        // The drawn body crosses a hint on another tick than the predicted one: cut with `follow`,
        // and take or leave a fixed camera (Beach's sea) with it.
        (own.cam.hint_cut, own.cam.fixed) = (follow.cam.hint_cut, follow.cam.fixed);
        let at = |f: &Follow| Vec3::from_array(f.cam.at);
        if at(own).distance(at(follow)) > SNAP {
            self.reset(f.body.pos, f.body.facing);
        }
    }

    /// One live tick that left the predicted body at `pos` facing `facing`: this tick's share of
    /// the offset and the turn goes (and the camera's weight climbs back), or of the camera's
    /// hand-off once they are gone.
    pub fn tick(&mut self, pos: [f32; 3], facing: u16) {
        self.last = Some((pos, facing));
        if self.blend > 0 {
            self.offset -= self.offset / self.blend as f32;
            self.turn -= self.turn / self.blend as f32;
            self.blend -= 1;
            self.handoff = (self.handoff + 1).min(HANDOFF);
        } else if self.handoff > 0 {
            self.handoff -= 1;
            if self.handoff == 0 {
                self.camera = None;
            }
        }
    }

    /// Where to draw a body predicted at `pos`.
    pub fn body(&self, pos: [f32; 3]) -> [f32; 3] {
        (Vec3::from_array(pos) + self.offset).to_array()
    }

    /// Which way to draw a body predicted facing `facing`.
    pub fn facing(&self, facing: u16) -> u16 {
        facing.wrapping_add(self.turn.round() as i32 as u16)
    }

    /// What to draw of a body predicted as `p`, with the follow camera `follow`.
    pub fn frame(&self, p: &PlayerState, follow: &Follow) -> Frame {
        let (eye, at) = self.view(follow);
        let body = Vec3::from_array(self.body(p.pos));
        let facing = self.facing(p.facing);
        Frame {
            body,
            facing,
            eye,
            at,
        }
    }

    /// The frame `f` of the way (the fixed clock's overstep) from [`prev`](Self::prev) to `now`:
    /// 30 Hz ticks drawn smoothly on faster displays, the view with the body.
    pub fn between(&self, now: Frame, f: f32) -> Frame {
        let Some(p) = self.prev else {
            return now;
        };
        let turn = angle_diff(now.facing, p.facing) as f32 * f;
        Frame {
            body: p.body.lerp(now.body, f),
            facing: p.facing.wrapping_add(turn.round() as i32 as u16),
            eye: p.eye.lerp(now.eye, f),
            at: p.at.lerp(now.at, f),
        }
    }

    /// The view (eye, look-at) to draw with the follow camera `follow`: the drawn body's camera,
    /// handed back to `follow` over [`HANDOFF`] ticks once the offset is gone (its weight is
    /// `handoff / HANDOFF`).
    pub fn view(&self, follow: &Follow) -> (Vec3, Vec3) {
        let view = |f: &Follow| {
            let c = &f.cam;
            (
                Vec3::from_array(c.view_eye()),
                Vec3::from_array(c.view_at()),
            )
        };
        let (eye, at) = view(follow);
        let Some(own) = &self.camera else {
            return (eye, at);
        };
        let (own_eye, own_at) = view(own);
        let w = self.handoff as f32 / HANDOFF as f32;
        (eye.lerp(own_eye, w), at.lerp(own_at, w))
    }
}

/// After lightyear's rollback (`PreUpdate`): a rollback that moved the local player's predicted
/// body is a correction ([`Drawn::correct`]; `CORRECTION` lines, debug).
pub fn note_corrections(
    timeline: Res<LocalTimeline>,
    mut own: Query<(&Player, &mut FollowCamera)>,
) {
    for (p, mut cam) in &mut own {
        let cam = &mut *cam;
        let corrected = cam.drawn.correct(p.pos, p.facing, &cam.follow);
        // A tank's view follows the predicted hull ([`crate::tank::camera`]).
        if cam.tank.is_some() {
            cam.drawn.camera = None;
        }
        if let Some((by, turn)) = corrected {
            let t = timeline.tick().0;
            let (x, y, z, d) = (by.x, by.y, by.z, by.length());
            debug!("CORRECTION t={t} by={x},{y},{z} jump={d} turn={turn}");
        }
    }
}

/// `DRAWN` lines (debug, every frame): the local player's predicted position, drawn position and
/// facing, the follow camera's eye and look-at, and the drawn view's.
pub fn log_drawn(timeline: Res<LocalTimeline>, own: Query<(&Player, &FollowCamera)>) {
    let xyz = |v: [f32; 3]| v.map(f64::from).map(|c| c.to_string()).join(",");
    for (p, cam) in &own {
        let (eye, at) = cam.drawn.view(&cam.follow);
        let c = &cam.follow.cam;
        debug!(
            "DRAWN t={} pos={} drawn={} facing={} dfacing={} eye={} at={} veye={} vat={}",
            timeline.tick().0,
            xyz(p.pos),
            xyz(cam.drawn.body(p.pos)),
            p.facing,
            cam.drawn.facing(p.facing),
            xyz(c.view_eye()),
            xyz(c.view_at()),
            xyz(eye.to_array()),
            xyz(at.to_array()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn follow() -> Follow {
        Follow::spawn([0.0, 100.0, 250.0], &PlayerState::spawn([0.0; 3], 0))
    }

    impl Drawn {
        fn correct_at(&mut self, pos: [f32; 3], follow: &Follow) -> Option<Vec3> {
            self.correct(pos, 0, follow).map(|c| c.0)
        }
    }

    #[test]
    fn a_turn_blends_away_the_short_way() {
        let mut d = Drawn::default();
        d.reset([0.0; 3], 65000);
        // Turned 1,036 forward through 0 by the correction: drawn still at 65,000.
        assert_eq!(
            d.correct([0.0; 3], 500, &follow()),
            Some((Vec3::ZERO, 1036))
        );
        assert_eq!(d.facing(500), 65000);
        for _ in 0..BLEND / 2 {
            d.tick([0.0; 3], 500);
        }
        assert_eq!(d.facing(500), 65518);
        for _ in 0..BLEND / 2 {
            d.tick([0.0; 3], 500);
        }
        assert_eq!((d.facing(500), d.turn), (500, 0.0));
    }

    #[test]
    fn a_correction_blends_away_in_blend_ticks_at_an_even_pace() {
        let mut d = Drawn::default();
        assert_eq!(d.correct_at([0.0; 3], &follow()), None); // the first position is no correction
        d.tick([0.0; 3], 0);
        // A rollback puts the body 80 units back along x: the drawing stays where it was.
        assert_eq!(
            d.correct_at([-80.0, 0.0, 0.0], &follow()),
            Some(Vec3::new(-80.0, 0.0, 0.0))
        );
        assert_eq!(d.body([-80.0, 0.0, 0.0]), [0.0; 3]);
        assert!(d.camera.is_some());
        let mut drawn = Vec::new();
        for _ in 0..BLEND {
            d.tick([-80.0, 0.0, 0.0], 0);
            drawn.push(d.body([-80.0, 0.0, 0.0])[0]);
        }
        assert_eq!(
            drawn,
            [-10.0, -20.0, -30.0, -40.0, -50.0, -60.0, -70.0, -80.0]
        );
        assert_eq!(d.offset, Vec3::ZERO);
        // The camera hands the view back over HANDOFF more ticks, then goes.
        for _ in 0..HANDOFF - 1 {
            d.tick([-80.0, 0.0, 0.0], 0);
        }
        assert!(d.camera.is_some());
        d.tick([-80.0, 0.0, 0.0], 0);
        assert!(d.camera.is_none());
    }

    #[test]
    fn a_second_correction_adds_and_restarts_the_blend() {
        let mut d = Drawn::default();
        d.reset([0.0; 3], 0);
        d.correct_at([0.0, 0.0, 40.0], &follow());
        d.tick([0.0, 0.0, 40.0], 0);
        d.tick([0.0, 0.0, 40.0], 0);
        assert_eq!(d.offset, Vec3::new(0.0, 0.0, -30.0));
        d.correct_at([0.0, 0.0, 50.0], &follow());
        assert_eq!(d.offset, Vec3::new(0.0, 0.0, -40.0));
        assert_eq!((d.blend, d.handoff), (BLEND, HANDOFF));
    }

    #[test]
    fn a_long_correction_moves_the_drawing_at_most_max_step_a_tick() {
        let mut d = Drawn::default();
        d.reset([0.0; 3], 0);
        d.correct_at([0.0, 0.0, 334.0], &follow());
        assert_eq!(d.blend, 17);
        let mut z = d.body([0.0, 0.0, 334.0])[2];
        for _ in 0..17 {
            d.tick([0.0, 0.0, 334.0], 0);
            let next = d.body([0.0, 0.0, 334.0])[2];
            assert!(next - z <= MAX_STEP && next > z);
            z = next;
        }
        assert_eq!(z, 334.0);
    }

    #[test]
    fn a_long_jump_is_drawn_at_once() {
        let mut d = Drawn::default();
        d.reset([0.0; 3], 0);
        d.correct_at([0.0, 0.0, 40.0], &follow());
        assert_eq!(
            d.correct_at([700.0, 0.0, 40.0], &follow()),
            Some(Vec3::new(700.0, 0.0, 0.0))
        );
        assert_eq!((d.offset, d.blend), (Vec3::ZERO, 0));
        assert!(d.camera.is_none());
    }

    #[test]
    fn a_correction_during_the_hand_off_keeps_the_views_weight() {
        let mut d = Drawn::default();
        d.reset([0.0; 3], 0);
        d.correct_at([0.0, 0.0, 40.0], &follow());
        for _ in 0..BLEND + HANDOFF - 2 {
            d.tick([0.0, 0.0, 40.0], 0);
        }
        assert_eq!((d.blend, d.handoff), (0, 2));
        // The follow camera 60 units off the drawn body's: the view is a quarter of the way over.
        let moved = Follow::spawn(
            [60.0, 100.0, 250.0],
            &PlayerState::spawn([60.0, 0.0, 0.0], 0),
        );
        let before = d.view(&moved);
        d.correct_at([0.0, 0.0, 50.0], &moved);
        assert_eq!(d.view(&moved), before);
        // The weight climbs back over the blend, then the hand-off starts again.
        d.tick([0.0, 0.0, 50.0], 0);
        assert_eq!(d.handoff, 3);
        for _ in 1..BLEND {
            d.tick([0.0, 0.0, 50.0], 0);
        }
        assert_eq!((d.blend, d.handoff), (0, HANDOFF));
    }

    #[test]
    fn the_drawn_bodys_camera_cuts_on_the_follow_cameras_tick() {
        // A trip wire across z = 0 that cuts the camera (conker-sim's follow tests).
        let mut world = CollisionWorld::new([(
            [[-5000., 0., -5000.], [-5000., 0., 5000.], [5000., 0., 0.]],
            5,
        )]);
        let mut hint = [0u8; 0x34];
        hint[6..12].copy_from_slice(&[3, 232, 1, 244, 3, 232]);
        (hint[0x15], hint[0x17], hint[0x23]) = (3, 4, 30);
        world.camera = Some(conker_camera::volume::CameraLevel::new(0, &hint));
        let place = conker_rules::spawn::Placement {
            index: 0,
            pos: [0.0, 0.0, -50.0],
            facing: 0xC000,
            camera: [0.0, 100.0, -297.0],
        };
        let mut f = Fighter::new(&place, 0, 0);
        let mut follow = Follow::spawn(place.camera, &f.body);
        let mut d = Drawn::default();
        d.reset(f.body.pos, f.body.facing);
        // A correction leaves the drawing 40 behind: it crosses the wire two ticks later.
        f.body.pos[2] = -10.0;
        d.correct(f.body.pos, f.body.facing, &follow);
        let mut cuts = vec![];
        for z in [-10.0, 10.0, 30.0, 50.0] {
            f.body.pos[2] = z;
            follow.step_view(crate::camera_view(&f, 0), &Input::default(), &world);
            d.step(&f, 0, &follow, &Input::default(), &world);
            let own = d.camera.expect("the drawn body's camera");
            cuts.push((follow.cam.hint_cut, own.cam.hint_cut));
        }
        assert_eq!(cuts.iter().filter(|c| c.0).count(), 1);
        assert!(cuts.iter().all(|c| c.0 == c.1), "{cuts:?}");
    }

    #[test]
    fn frames_between_ticks_are_drawn_the_overstep_of_the_way() {
        let mut d = Drawn::default();
        let at = |z: f32, facing: u16| {
            let p = PlayerState::spawn([0.0, 0.0, z], facing);
            d.frame(&p, &Follow::spawn([0.0, 100.0, z + 250.0], &p))
        };
        let (prev, now) = (at(0.0, 65000), at(-35.0, 500));
        assert_eq!(d.between(now, 0.5), now); // nothing before the first tick
        d.prev = Some(prev);
        assert_eq!(d.between(now, 0.0), prev);
        let half = d.between(now, 0.5);
        assert_eq!((half.body.z, half.eye.z), (-17.5, prev.eye.z - 17.5));
        assert_eq!(half.facing, 65518); // the short way, towards 0
        d.reset([0.0; 3], 0);
        assert_eq!(d.prev, None);
    }

    #[test]
    fn the_view_is_the_follow_cameras_without_a_correction() {
        let f = follow();
        let d = Drawn::default();
        let c = &f.cam;
        assert_eq!(
            d.view(&f),
            (
                Vec3::from_array(c.view_eye()),
                Vec3::from_array(c.view_at())
            )
        );
    }
}
