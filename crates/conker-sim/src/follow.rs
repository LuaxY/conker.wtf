//! The follow camera in the simulation loop: conker-camera's port of the game's camera, fed from the
//! player state and the collision world. A player's whole tick is [`Follow::tick`]: the move (with last
//! tick's [`Follow::yaw`] in [`crate::movement::Input::camera_yaw`]), then the camera, then the aim
//! camera's gate on the player's stick ([`PlayerState::aiming`]). The camera is part of the simulation:
//! the server needs one per player as well (closed loop: V on 28 oracle traces, `tests/oracle.rs`).
//!
//! The level answers the camera's queries ([`CameraWorld`]): the floor under the eye (the player's floor
//! query), the eye's sweep (the player's wall routine, [`CollisionWorld::sweep`], moving a camera-sized
//! body), and the level's camera data ([`CollisionWorld::camera`]: volumes, the zones where the sweep
//! skips the placements, and the hints that cut the camera), and the wall ray (the line query,
//! [`CollisionWorld::ray`]).

use crate::collision::{Body, CollisionWorld, Sweep};
use crate::movement::{BUTTON_C_LEFT, BUTTON_C_RIGHT, Input, PlayerState, deadzone, state};
use conker_camera::{CameraState, CameraWorld, HEAD_STANDING, PlayerView, RayHit};

/// Head height while crouch-walking (`struct126+0x114`, V: the camera agent's traces).
const HEAD_CROUCHED: f32 = 40.0;

/// What the camera reads from the player: its heading (obj+0x40, square to the wall while pushing one),
/// head height, sink and speed. The match rules add what they know ([`Follow::step_view`]).
pub fn view(p: &PlayerState) -> PlayerView {
    PlayerView {
        head: if p.state == state::CROUCH_WALK {
            HEAD_CROUCHED
        } else {
            HEAD_STANDING
        },
        heading: p.heading,
        sink: p.sink,
        speed: p.xz_speed,
        swim: p.swimming.then_some(p.water),
        ..PlayerView::new(p.pos, p.floor_y, p.facing)
    }
}

/// A player's follow camera, with the last-triangle cache of its floor query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follow {
    pub cam: CameraState,
    pub floor: Option<u32>,
}

/// The level as one camera tick sees it.
struct Level<'a> {
    world: &'a CollisionWorld,
    floor: &'a mut Option<u32>,
    /// `cam+0x84` bit 31 this tick, set with the volume test.
    skip_objects: bool,
}

impl CameraWorld for Level<'_> {
    fn floor_below(&mut self, eye: [f32; 3], start_y: f32) -> Option<f32> {
        let f = self
            .world
            .floor(eye[0], eye[2], eye[1], start_y, *self.floor);
        *self.floor = f.map(|f| f.tri).or(*self.floor);
        f.map(|f| f.y)
    }

    /// The full query, the last triangle neither used nor kept (V c34 643: the eye 40 over a ledge at
    /// y 79 that the floor triangle's fast path, still holding the eye, would have hidden).
    fn floor_at(&mut self, eye: [f32; 3], start_y: f32) -> Option<f32> {
        (self.world.floor(eye[0], eye[2], eye[1], start_y, None)).map(|f| f.y)
    }

    /// The game's line query (V: the 2,527 rays of the camera captures, bit for bit).
    fn ray(&mut self, origin: [f32; 3], dir: [f32; 3], len: f32) -> Option<RayHit> {
        let hit = self.world.ray(origin, dir, len)?;
        Some(RayHit {
            dist: hit.dist,
            point: hit.point,
            tri: self.world.tris[hit.tri as usize]
                .v
                .map(|p| p.map(|c| c as i16)),
        })
    }

    fn sweep(&mut self, from: [f32; 3], to: [f32; 3], size: [f32; 2]) -> Option<[f32; 3]> {
        self.sweep_eye(from, to, size, false)
    }

    /// The eye moved like an object of the camera's size (V: the 1,745 sweeps of the camera captures, bit
    /// for bit, `tests/oracle.rs`); None when no layer pushed it.
    fn sweep_eye(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        size: [f32; 2],
        long_step: bool,
    ) -> Option<[f32; 3]> {
        let opts = Sweep {
            any_slope: true,
            long_step,
            no_objects: self.skip_objects,
            ..Sweep::default()
        };
        let swept = self
            .world
            .sweep(from, to, &Body::camera(size[0], size[1]), &opts, 0.0);
        (swept.hits > 0).then_some(swept.pos)
    }

    /// The water under the eye (C: `func_1512A390`'s query fills `cam+0x360`).
    fn water_below(&mut self, at: [f32; 3]) -> Option<f32> {
        let w = self.world.water(at[0], at[2]);
        (w != crate::collision::NO_FLOOR).then_some(w)
    }

    fn in_volume(&mut self, eye: [f32; 3]) -> bool {
        let Some(level) = &self.world.camera else {
            return true;
        };
        self.skip_objects = level.skips_objects(eye);
        level.in_volume(eye)
    }
}

impl Follow {
    /// The camera of a player that just spawned, from the spawn record's eye; the first step cuts
    /// behind the player (V: oracle reads 23-24).
    pub fn spawn(eye: [f32; 3], p: &PlayerState) -> Self {
        Self {
            cam: CameraState::spawn(eye, &view(p)),
            floor: None,
        }
    }

    /// One tick, after the player's (the game's order), with the stick centred: [`step_input`](Self::step_input).
    pub fn step(&mut self, p: &PlayerState, buttons: u16, world: &CollisionWorld) {
        self.step_pad(view(p), buttons, [0, 0], world);
    }

    /// One tick, after the player's, with the input the player stepped with: its buttons (R starts the aim
    /// camera, C strafes) and its stick (it aims while R is held).
    pub fn step_input(&mut self, p: &PlayerState, input: &Input, world: &CollisionWorld) {
        self.step_blocked(p, input, world, false);
    }

    /// [`step_input`](Self::step_input) for a player that may not start the aim camera (`aim_blocked`: R
    /// is refused at health 0, C; the match rules know the health, the movement does not).
    pub fn step_blocked(
        &mut self,
        p: &PlayerState,
        input: &Input,
        world: &CollisionWorld,
        aim_blocked: bool,
    ) {
        self.step_view(
            PlayerView {
                aim_blocked,
                ..view(p)
            },
            input,
            world,
        );
    }

    /// [`step_input`](Self::step_input) with what the match rules know of the player: start from
    /// [`view`] and set [`PlayerView::weapon`] (the drawn weapon, 0x26 while a grenade is in hand),
    /// [`PlayerView::in_hand`], [`PlayerView::twist`], [`PlayerView::stunned`] and
    /// [`PlayerView::dead`] (health 0; or [`PlayerView::aim_blocked`]). They give R's aim camera with a
    /// weapon, the sniper's scope ([`conker_camera::Scope`], whose look angles are the shot's
    /// direction) and the twin guns' stiffer follow camera. [`PlayerView::strafing`] comes from the
    /// input's buttons.
    pub fn step_view(&mut self, view: PlayerView, input: &Input, world: &CollisionWorld) {
        let stick = [deadzone(input.stick_x), deadzone(input.stick_y)];
        self.step_pad(view, input.buttons, stick, world);
    }

    /// Whether the aim camera or the scope holds the player's stick next tick ([`PlayerState::aiming`];
    /// V: camera4_w24 501-542, the scope stops a run like R's aim and frees it the tick after release).
    pub fn aiming(&self) -> bool {
        (self.cam.aim.on && self.cam.aim.phase != 0) || self.cam.scope.on
    }

    /// One whole tick in the game's order: the player moves with `input` (its `camera_yaw` should be
    /// [`yaw`](Self::yaw) of this camera), then the camera steps, then the player learns whether the
    /// aim camera is on ([`PlayerState::aiming`], read by its next step).
    pub fn tick(&mut self, p: &mut PlayerState, input: &Input, world: &CollisionWorld) {
        crate::movement::step(p, input, world);
        self.step_input(p, input, world);
        p.aiming = self.aiming();
    }

    fn step_pad(&mut self, view: PlayerView, buttons: u16, stick: [i8; 2], world: &CollisionWorld) {
        let Self { cam, floor } = self;
        let p = PlayerView {
            // struct126+0x1B3 (I: C-left or C-right held, as the camera agent verified).
            strafing: buttons & (BUTTON_C_LEFT | BUTTON_C_RIGHT) != 0,
            ..view
        };
        let mut level = Level {
            world,
            floor,
            skip_objects: false,
        };
        let hints = world.camera.as_ref();
        if cam.cut {
            // A spawn: its first frame's player update tests the hints at the spawn (V east 24).
            cam.fixed = hints.and_then(|c| c.fixed(p.pos, p.pos, p.floor_y));
        }
        let before = cam.prev_pos;
        cam.step_pad(&p, buttons, stick, &mut level);
        // Next frame's player update tests the hints with this frame's move (`func_150A11C4`, before
        // the move), and the camera reads them at the end of that frame.
        cam.hint_cut = hints.is_some_and(|c| c.hint_cut(before, p.pos, p.floor_y));
        cam.fixed = hints.and_then(|c| c.fixed(before, p.pos, p.floor_y));
    }

    /// The camera angle the next input carries ([`crate::movement::Input::camera_yaw`]):
    /// [`CameraState::stick_yaw`].
    pub fn yaw(&self) -> u16 {
        self.cam.stick_yaw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_cuts_behind_and_aims_one_step_below_the_facing() {
        // Oracle player 1: record 1's eye, standing at its spawn on the flat floor.
        let world = CollisionWorld::new([
            (
                [
                    [-5000., -521., 3000.],
                    [0., -521., 3000.],
                    [-5000., -521., 9000.],
                ],
                5,
            ),
            (
                [
                    [0., -521., 3000.],
                    [0., -521., 9000.],
                    [-5000., -521., 9000.],
                ],
                5,
            ),
        ]);
        let p = PlayerState::spawn([-2267.0, -521.0, 6016.0], 16896);
        let mut f = Follow::spawn([-2328.0, -300.0, 6563.0], &p);
        f.step(&p, 0, &world);
        assert_eq!(f.cam.at, [-2267.0, -453.5, 6016.0]); // oracle read 24
        assert_eq!(f.cam.eye[1], -421.0);
        assert_eq!(f.yaw(), 511); // 2.81 degrees: forward aims at 16895, the move steps to 16640 (the "-256")
        assert!(f.floor.is_some());
    }

    #[test]
    fn tick_freezes_the_player_while_r_aims() {
        // camera_buttons 533-566: R pressed while running; the stick moves the player again on the tick
        // the aim camera spends in phase 0, before it pops.
        let world = CollisionWorld::new([
            (
                [
                    [-5000., -521., 3000.],
                    [0., -521., 3000.],
                    [-5000., -521., 9000.],
                ],
                5,
            ),
            (
                [
                    [0., -521., 3000.],
                    [0., -521., 9000.],
                    [-5000., -521., 9000.],
                ],
                5,
            ),
        ]);
        let mut p = PlayerState::spawn([-2267.0, -521.0, 6016.0], 16896);
        let mut f = Follow::spawn([-2328.0, -300.0, 6563.0], &p);
        f.step(&p, 0, &world);
        let input = |f: &Follow, buttons| Input {
            stick_x: 82,
            buttons,
            camera_yaw: f.yaw(),
            ..Input::default()
        };
        let r = conker_camera::R;
        f.tick(&mut p, &input(&f, 0), &world);
        f.tick(&mut p, &input(&f, r), &world);
        assert!(p.aiming && f.cam.aim.on);
        let pos = p.pos;
        let mut ticks = 0;
        while p.aiming {
            let held = if ticks < 29 { r } else { 0 };
            f.tick(&mut p, &input(&f, held), &world);
            ticks += 1;
        }
        // 30 ticks of zoom (60 VIs), one releasing, then the phase-0 tick frees the stick.
        assert_eq!(ticks, 31);
        assert!(f.cam.aim.on && (p.pos[0] - pos[0]).abs() < 10.0);
        f.tick(&mut p, &input(&f, 0), &world);
        assert!(!f.cam.aim.on && p.xz_speed > 0.0);
    }

    #[test]
    fn a_blocked_player_cannot_start_the_aim_camera() {
        let world = CollisionWorld::new([(
            [[-5000., 0., -5000.], [-5000., 0., 5000.], [5000., 0., 0.]],
            5,
        )]);
        let p = PlayerState::spawn([0.0; 3], 0);
        let r = Input {
            buttons: conker_camera::R,
            ..Input::default()
        };
        let mut blocked = Follow::spawn([0.0, 100.0, 247.0], &p);
        blocked.step(&p, 0, &world);
        let mut free = blocked;
        blocked.step_blocked(&p, &r, &world, true);
        free.step_blocked(&p, &r, &world, false);
        assert!(!blocked.aiming() && free.aiming());
    }

    #[test]
    fn crossing_a_trip_wire_cuts_the_camera_the_next_tick() {
        let mut world = CollisionWorld::new([(
            [[-5000., 0., -5000.], [-5000., 0., 5000.], [5000., 0., 0.]],
            5,
        )]);
        // One hint: a trip wire (type 0, shape 3) across z = 0, kind 4, command 30.
        let mut hint = [0u8; 0x34];
        hint[6..12].copy_from_slice(&[3, 232, 1, 244, 3, 232]); // 1000 wide, 500 high
        (hint[0x15], hint[0x17], hint[0x23]) = (3, 4, 30);
        world.camera = Some(conker_camera::volume::CameraLevel::new(0, &hint, &[]));
        let mut p = PlayerState::spawn([0.0, 0.0, -30.0], 0xC000);
        let mut f = Follow::spawn([0.0, 100.0, -277.0], &p);
        let mut cuts = vec![];
        for z in [-30.0, -10.0, 10.0, 30.0, 50.0] {
            p.pos[2] = z;
            f.step(&p, 0, &world);
            cuts.push(f.cam.hint_cut);
        }
        // The move from -10 to 10 crosses it; the next step consumes the cut.
        assert_eq!(cuts, [false, false, true, false, false]);
    }

    #[test]
    fn a_spawn_in_a_fixed_hint_cuts_onto_its_camera() {
        let mut world = CollisionWorld::new([(
            [
                [-20000., -463., -20000.],
                [-20000., -463., 20000.],
                [20000., -463., 0.],
            ],
            5,
        )]);
        // Beach's east beach (V east 24): a box hint (type 0, shape 2) of kind 5, point 0, reach 751.
        let mut hint = [0u8; 0x34];
        for (k, v) in [2471i16, -1000, 7827, 500, 2000, 500].iter().enumerate() {
            hint[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        (hint[0x15], hint[0x17]) = (2, 5);
        hint[0x1C..0x20].copy_from_slice(&751u32.to_be_bytes());
        world.camera = Some(conker_camera::volume::CameraLevel::new(
            0x2B,
            &hint,
            &[[2024, 277, 9703]],
        ));
        let mut p = PlayerState::spawn([2471.0, -463.0, 7827.0], 27392);
        let mut f = Follow::spawn([2922.0, -261.0, 8004.0], &p);
        f.step(&p, 0, &world);
        let target = [0x4510_2A31, 0xC369_4390, 0x4605_133E]; // (2306.64, -233.26, 8516.81)
        assert_eq!(f.cam.eye.map(f32::to_bits), target);
        assert!(f.cam.fixed.is_some());
        p.pos[0] += 1000.0;
        f.step(&p, 0, &world);
        assert!(f.cam.fixed.is_none());
    }

    #[test]
    fn step_view_passes_the_weapon_to_the_camera() {
        let world = CollisionWorld::new([(
            [[-5000., 0., -5000.], [-5000., 0., 5000.], [5000., 0., 0.]],
            5,
        )]);
        let p = PlayerState::spawn([0.0; 3], 0);
        let mut f = Follow::spawn([0.0, 100.0, 247.0], &p);
        let input = Input::default();
        f.step_view(
            PlayerView {
                weapon: 0x09,
                ..view(&p)
            },
            &input,
            &world,
        );
        assert_eq!(f.cam.weapon, 0x09);
    }

    #[test]
    fn the_scope_holds_the_stick_until_released() {
        let world = CollisionWorld::new([(
            [[-5000., 0., -5000.], [-5000., 0., 5000.], [5000., 0., 0.]],
            5,
        )]);
        let p = PlayerState::spawn([0.0; 3], 0);
        let mut f = Follow::spawn([0.0, 100.0, 247.0], &p);
        let sniper = PlayerView {
            weapon: 0x24,
            in_hand: true,
            ..view(&p)
        };
        let r = Input {
            buttons: conker_camera::R,
            stick_x: 75,
            ..Input::default()
        };
        f.step_view(sniper, &r, &world);
        assert!(f.cam.scope.on && !f.cam.aim.on && f.aiming());
        f.step_view(sniper, &Input::default(), &world);
        assert!(!f.aiming());
    }

    #[test]
    fn crouch_walking_lowers_the_head() {
        let mut p = PlayerState::spawn([0.0; 3], 0);
        assert_eq!(view(&p).head, HEAD_STANDING);
        p.state = state::CROUCH_WALK;
        assert_eq!(view(&p).head, 40.0);
    }
}
