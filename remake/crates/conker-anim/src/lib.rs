//! Which clip the player shows, how fast, and where in it: the original game's rules, re-derived
//! from its code (the recompiled functions named below) and checked against oracle traces
//! (`../lab/oracle/anim_README.md`). No engine dependency.
//!
//! The game's movement state (player object byte +0x85) is the low byte of the current animation
//! id (u16 +0x84): the state machine and the animation are one variable, so "state -> clip" is
//! the identity. What this crate adds is how the game picks the grounded state from speed
//! ([`Ground`]), the rate and blend each state plays with ([`request`]), and the clock
//! ([`AnimPlayer`]). Times are in clip ticks (30 per second); one call of [`AnimPlayer::tick`] is
//! one 30 Hz game frame (2 VIs).
//!
//! Root motion: the game moves the player by speed (0.5 x obj+0x3C per frame), except in
//! [`id::CREEP`], where it moves him by the clip's root z instead (x0.5 model scale, along the
//! facing, two frames late). Every other player clip keeps its root x/z still; renderers should
//! drop root x/z and keep root y (the bob). A spawn plays [`SPAWN`]; the frame after, the game
//! still cuts instead of blending (obj+0x2FF = 0): pass that request with `blend: 0.0`.

/// Logical animation ids of the player character (bank-0x0F routes; the clip for an id comes
/// from the extractor). Names are what the traces show.
pub mod id {
    /// Creeping or turning on the spot (effective speed 1..6). Its clip moves the player by root
    /// motion (see the README), unlike the others.
    pub const CREEP: u16 = 0;
    /// Slow walk, and walking to a stop (effective speed 6..14).
    pub const WALK: u16 = 1;
    /// The walk/run cycle above effective speed 14 (half stick walks with it too).
    pub const RUN: u16 = 2;
    pub const IDLE: u16 = 15;
    /// Z held, standing (or too fast to crouch-walk).
    pub const CROUCH: u16 = 22;
    pub const CROUCH_WALK: u16 = 27;
    /// Stick released above speed 18: held 17 frames.
    pub const SKID: u16 = 33;
    /// The run while an enemy's drawn weapon is near (effective speed 5 and up; conker-sim's
    /// `PlayerState::scared`).
    pub const SCARED_RUN: u16 = 43;
    pub const JUMP: u16 = 50;
    /// After a standing jump or a fall: held 15 frames.
    pub const LAND: u16 = 54;
    pub const RUN_JUMP: u16 = 55;
    /// Walked off a ledge (more than 60 units above the floor).
    pub const FALL: u16 = 56;
    pub const WALL_PUSH: u16 = 122;
    /// A with Z held: the high jump (conker-sim's `HIGH_JUMP_Y_VEL`), its clip shared with 404 and
    /// 741 (pair 21, 54 frames).
    pub const HIGH_JUMP: u16 = 235;
    /// War: Beach's sea (conker-sim's `swim`): wading in the shallows, swimming at the surface (and
    /// deep), a stroke (A) at the surface (and deep), and the hold (Z) under it.
    pub const WADE: u16 = 139;
    pub const SWIM: u16 = 204;
    pub const SWIM_DEEP: u16 = 209;
    pub const STROKE: u16 = 34;
    pub const STROKE_DEEP: u16 = 206;
    pub const SWIM_HOLD: u16 = 203;
}

/// Multiplayer combat ids (u16 obj+0x84) from the oracle's combat traces
/// (`../lab/oracle/combat_README.md` §7, V unless noted). Unlike the locomotion ids their low
/// byte is not a movement state. Frame counts (how long the game holds each one-shot) live with
/// the rules that use them, in conker-rules.
pub mod combat {
    /// Twin guns (weapon 0x09): draw 15 frames, armed idle, holster 22, fire while Z is held,
    /// reload 34, armed locomotion (bots; its rate follows speed, not fitted).
    pub const TWIN_DRAW: u16 = 0x7B;
    pub const TWIN_IDLE: u16 = 0x7F;
    pub const TWIN_HOLSTER: u16 = 0x7D;
    pub const TWIN_FIRE: u16 = 0x7C;
    pub const TWIN_RELOAD: u16 = 0x11A;
    pub const TWIN_MOVE: u16 = 0x12B;
    /// Pistol (0x39): draw 15, idle, holster 15, fire 2 per press, reload 60.
    pub const PISTOL_DRAW: u16 = 0x1EE;
    pub const PISTOL_IDLE: u16 = 0x1EA;
    pub const PISTOL_HOLSTER: u16 = 0x1EF;
    pub const PISTOL_FIRE: u16 = 0x1EB;
    pub const PISTOL_RELOAD: u16 = 0x21E;
    /// Tommy gun (0x38): draw 15 at 1.4 (V e4 combat7_e_w38 761-775).
    pub const TOMMY_DRAW: u16 = 0x1DD;
    /// The one-shot gun (0x23): draw 26, idle, holster 28, fire 28 per press, a C button 3,
    /// reload 31 (A) or 79 (after the sixth shot).
    pub const GUN23_DRAW: u16 = 0x163;
    pub const GUN23_IDLE: u16 = 0x165;
    pub const GUN23_HOLSTER: u16 = 0x164;
    pub const GUN23_FIRE: u16 = 0x166;
    pub const GUN23_C: u16 = 0x162;
    pub const GUN23_RELOAD: u16 = 0x26E;
    /// Sniper rifle (0x24): draw 53, idle, aim (R held), fire 39, holster 29, a C button 3.
    pub const SNIPER_DRAW: u16 = 0x152;
    pub const SNIPER_IDLE: u16 = 0x153;
    pub const SNIPER_AIM: u16 = 0x157;
    pub const SNIPER_FIRE: u16 = 0x156;
    pub const SNIPER_HOLSTER: u16 = 0x155;
    pub const SNIPER_C: u16 = 0x159;
    /// Flamethrower (0x15): draw 34, idle, holster 25, fire while Z is held.
    pub const FLAME_DRAW: u16 = 0x168;
    pub const FLAME_IDLE: u16 = 0xE2;
    pub const FLAME_HOLSTER: u16 = 0xE4;
    pub const FLAME_FIRE: u16 = 0xE7;
    /// Melee 0x21: draw 16, idle, holster 20, two swings (19 and 20 frames), armed walk (rate
    /// follows speed, not fitted).
    pub const MELEE_DRAW: u16 = 0x120;
    pub const MELEE_IDLE: u16 = 0x122;
    pub const MELEE_HOLSTER: u16 = 0x121;
    pub const MELEE_SWING_A: u16 = 0x129;
    pub const MELEE_SWING: u16 = 0x12A;
    pub const MELEE_WALK: u16 = 0x124;
    /// Melee 0x21's landing after A's jump (the others land with the plain [`super::id::LAND`]).
    pub const MELEE_LAND: u16 = 0x19D;
    /// Melee 0x22 (one hit kills): draw 21, idle, holster 21, swing 19, walk (rate not fitted).
    pub const BLADE_DRAW: u16 = 0x15D;
    pub const BLADE_IDLE: u16 = 0x15A;
    pub const BLADE_HOLSTER: u16 = 0x15E;
    pub const BLADE_SWING: u16 = 0x15F;
    pub const BLADE_WALK: u16 = 0x15B;
    /// Weapon 0x14: draw 15, idle, Z 14 (0xF7 plain, 0x12B with R held: the aimed shot; V
    /// combat4_w14), holster 15.
    pub const W14_DRAW: u16 = 0x102;
    pub const W14_IDLE: u16 = 0xF9;
    pub const W14_ATTACK: u16 = 0xF7;
    pub const W14_AIMED: u16 = 0x12B;
    pub const W14_HOLSTER: u16 = 0x103;
    /// Weapon 0x16, the rocket launcher: draw 39, idle (held until B), holster 29 (`combat_e_w16`),
    /// Z fires, 28 (`combat4_ctlw_k7`; its clip moves the player back 46.5 by root motion).
    pub const W16_FIRE: u16 = 0x77;
    pub const W16_DRAW: u16 = 0x76;
    pub const W16_IDLE: u16 = 0xBD;
    pub const W16_HOLSTER: u16 = 0x78;
    /// The crossbow (0x18): draw 64, idle, holster 34; Z from the hip 29; R's aim, and its shot on
    /// Z's release, 29 (V combat5_ctlw18, cb3).
    pub const CROSSBOW_DRAW: u16 = 0xFB;
    pub const CROSSBOW_IDLE: u16 = 0xC5;
    pub const CROSSBOW_HOLSTER: u16 = 0xC6;
    pub const CROSSBOW_FIRE: u16 = 0xFC;
    pub const CROSSBOW_AIM: u16 = 0x10A;
    pub const CROSSBOW_AIMED: u16 = 0x10C;
    /// War: Raptor's bone crossbow (0x41): draw 64, fire 29 (conker-rules' weapon table; V r10).
    pub const BONE_CROSSBOW_DRAW: u16 = 0x2C8;
    pub const BONE_CROSSBOW_FIRE: u16 = 0x2C9;
    /// The shotgun (0x37): draw 13, idle, holster 13 (rate 1.4); Z plays 0x1F4 (45) or, one shot
    /// in four, 0x235 (71) (V combat5_ctlw37, sg3).
    pub const SHOTGUN_DRAW: u16 = 0x1F7;
    pub const SHOTGUN_IDLE: u16 = 0x1F3;
    pub const SHOTGUN_HOLSTER: u16 = 0x1F8;
    pub const SHOTGUN_FIRE: u16 = 0x1F4;
    pub const SHOTGUN_FIRE_B: u16 = 0x235;
    /// Item 0x40, the club (scene 0x3F's start item): draw 15, idle, holster 15, the swing 30 and,
    /// after a hit, the second swing 27; walk and run picked by speed (V combat6_ctlw40, e40).
    pub const CLUB_DRAW: u16 = 0x2C2;
    pub const CLUB_IDLE: u16 = 0x10F;
    pub const CLUB_HOLSTER: u16 = 0x2C3;
    pub const CLUB_SWING: u16 = 0x113;
    pub const CLUB_COMBO: u16 = 0x119;
    pub const CLUB_WALK: u16 = 0x112;
    pub const CLUB_RUN: u16 = 0x111;
    /// Grenade (C-down): pull 28, hold while held, throw 28.
    pub const GRENADE_PULL: u16 = 0x176;
    pub const GRENADE_HOLD: u16 = 0x177;
    pub const GRENADE_THROW: u16 = 0x16B;
    /// Hit by a bullet or the sniper's body shot: 18 frames.
    pub const HIT: u16 = 0x11D;
    /// Hit by weapon 0x14's shot on the body (class 117's reaction 0x20: 30 frames, C).
    pub const HIT_SHOT: u16 = 0xD3;
    /// Hit by melee 0x21 or the club's swing: 19 frames.
    pub const HIT_MELEE: u16 = 0x11B;
    /// Hit by the club's second swing: 18 frames.
    pub const HIT_CLUB: u16 = 0x11C;
    /// Blown up by a grenade (42 frames, 88 when lethal), getting up (42), recovering (30).
    pub const BLOWN_UP: u16 = 0xF3;
    pub const GET_UP: u16 = 0xF4;
    pub const RECOVER: u16 = 0xF0;
    /// Deaths: the fall (52) and lying dead (51-81), with melee 0x21's last 18 frames.
    pub const DEATH_FALL: u16 = 0x58;
    pub const DEAD: u16 = 0x59;
    pub const DEAD_MELEE_END: u16 = 0x18A;
    /// The sniper's and melee 0x22's kill (75-76 frames), then 52.
    pub const DEATH_SHOT: u16 = 0x17E;
    pub const DEATH_SHOT_FALL: u16 = 0x16F;
    /// The one-shot gun's kill (35), then 103.
    pub const DEATH_BLAST: u16 = 0x19C;
    pub const DEATH_BLAST_FALL: u16 = 0x13;
    /// Total War's gas kill (86, then [`DEATH_SHOT_FALL`]), the doorway's hit (59) and death (87),
    /// and the carried canister's run and stand (V d1, d2).
    pub const GASSED: u16 = 0x16E;
    pub const DOORWAY_HIT: u16 = 0xF1;
    pub const DOORWAY_DEAD: u16 = 0xF6;
    pub const CARRY_MOVE: u16 = 0xD7;
    pub const CARRY_IDLE: u16 = 0xD8;
    /// War: Colors' flag: the grab (17 frames), the carry's stand, walk (speed up to 14) and run
    /// (`func_15064B94`'s 0x1B0/0x1B1, C), the put-down on B (39), the plant in the capture zone
    /// (30), the scorer's cheer (49) and the other team's pose through the freeze (V: obj+0x84 in
    /// e0, e1, b1; conker-rules' `colors::Pose`).
    pub const FLAG_GRAB: u16 = 0xD5;
    pub const FLAG_IDLE: u16 = 0x1AF;
    pub const FLAG_WALK: u16 = 0x1B0;
    pub const FLAG_RUN: u16 = 0x1B1;
    pub const FLAG_PUT_DOWN: u16 = 0x28D;
    pub const FLAG_PLANT: u16 = 0x22D;
    pub const CHEER: u16 = 0x19A;
    pub const LOSE: u16 = 0x2E7;
    /// The mounted gun's gunner (conker-rules' `gun`): seated (16 frames, looped) and while the gun
    /// fires (6); the jump onto it plays [`crate::id::JUMP`] at rate 1.3 (V m1, m2, c10m; C
    /// `func_15064A14`).
    pub const GUN_SEATED: u16 = 0x1A1;
    pub const GUN_FIRING: u16 = 0x1A4;
    /// Total War's gas mask put on (state 81) and taken off (state 78), at 3 ticks a frame (V e2:
    /// anim speed 3, clock 3 … 57 over the 19 frames the rules hold them).
    pub const MASK_ON: u16 = 0x151;
    pub const MASK_OFF: u16 = 0x14E;
    /// Flame hit (5), then the burning run while the fire lasts, and its bump off a wall (11).
    pub const FLAME_HIT: u16 = 0x69;
    pub const BURNING: u16 = 0x4C;
    pub const BURN_END: u16 = 0x28E;
    /// Twin guns and weapons 0x23 and 0x16 fidget in their drawn idle (conker-rules' `fidget`).
    pub const TWIN_FIDGET: u16 = 0x144;
    pub const GUN23_FIDGET: u16 = 0x22A;
    pub const W16_FIDGET: u16 = 0x229;
    /// Armed walk and run (`func_15064B94`, C): the walk while the stick asks for 1 or more, the
    /// run from a target of 20 (melee 0x21: 15). [`armed_move`] picks them.
    pub const TWIN_WALK: u16 = 0x7E;
    pub const TWIN_RUN: u16 = 0x22C;
    pub const PISTOL_WALK: u16 = 0x1EC;
    pub const PISTOL_RUN: u16 = 0x1ED;
    pub const GUN23_WALK: u16 = 0x161;
    pub const GUN23_RUN: u16 = 0x162;
    pub const SNIPER_WALK: u16 = 0x158;
    pub const SNIPER_RUN: u16 = 0x159;
    /// The flamethrower's one armed gait (it plays at the runs' rate, V camera4_w15).
    pub const FLAME_WALK: u16 = 0xED;
    pub const MELEE_STEP: u16 = 0x123;
    pub const BLADE_RUN: u16 = 0x15C;
    pub const W16_WALK: u16 = 0xE8;
    pub const GRENADE_WALK: u16 = 0x178;
    pub const GRENADE_RUN: u16 = 0x179;
    pub const CROSSBOW_WALK: u16 = 0xEC;
    pub const CROSSBOW_RUN: u16 = 0xF8;
    pub const SHOTGUN_WALK: u16 = 0x1F5;
    pub const SHOTGUN_RUN: u16 = 0x1F6;
    /// Clip lengths (ticks) of the drawn idles and their fidgets: the rules pick the next idle on
    /// the clip's last tick (the ROM's durations, tests/rom.rs).
    pub const IDLE_CLIPS: [(u16, u16); 6] = [
        (TWIN_IDLE, 52),
        (TWIN_FIDGET, 102),
        (GUN23_IDLE, 30),
        (GUN23_FIDGET, 60),
        (W16_IDLE, 30),
        (W16_FIDGET, 51),
    ];
}

/// No clip yet (+0x84 before the first animation).
pub const NONE: u16 = 0xFFFF;

/// What `func_150838EC` requests when a player spawns.
pub const SPAWN: Request = Request {
    id: id::CREEP,
    rate: 1.0,
    blend: 0.0,
    start: 0.0,
};

/// One call of the game's set-animation function (`func_1505E650`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub id: u16,
    /// Clip ticks per frame.
    pub rate: f32,
    /// Cross-fade length in frames (< 1: cut).
    pub blend: f32,
    /// Clip time a restart begins at.
    pub start: f32,
}

/// `func_1504C0E8`'s pace: 0.4 speed + 0.6 target speed (`D_800991DC`, `D_800991E0`).
pub fn effective_speed(speed: f32, target: f32) -> f32 {
    speed * 0.4 + target * 0.6
}

/// What the game asks for in state `id` at this speed (obj+0x3C after this frame's acceleration)
/// and target speed (obj+0x44). Ids the player never showed in the traces get rate 1, blend 4 (I).
pub fn request(id: u16, speed: f32, target: f32) -> Request {
    use combat::{
        BLADE_RUN, BLADE_WALK, CROSSBOW_RUN, CROSSBOW_WALK, FLAME_WALK, GRENADE_RUN, GRENADE_WALK,
        GUN23_RUN, GUN23_WALK, MELEE_STEP, PISTOL_RUN, SHOTGUN_RUN, SNIPER_RUN, SNIPER_WALK,
        TWIN_RUN, W16_WALK,
    };
    use id::*;
    let e = effective_speed(speed, target);
    // (rate, blend frames, start); constants are .game data words (tests/rom.rs) or immediates.
    let (rate, blend, start) = match id {
        CREEP => (e * 0.125 + 0.1, 15.0, 0.0),
        WALK => (e * 0.125 + 0.3, 4.0, 0.0),
        RUN => (e / 23.0 + 0.8, 4.0, 0.0),
        // `func_1504C0E8` (C; V combat5_sg_e2 766: 1.7833 at e 21.4).
        SCARED_RUN => (e / 12.0, 4.0, 0.0),
        WALL_PUSH => (e / 15.0 + 0.6, 4.0, 0.0), // func_15064B94
        CROUCH_WALK => (target / 15.0 + 0.5, 5.0, 0.0),
        CROUCH => (1.5, 6.0, 0.0),
        SKID => (0.5, 3.0, 0.0),
        FALL => (0.5, 4.0, 0.0),
        IDLE => (1.0, 3.0, 0.0),      // func_1507F640
        JUMP => (1.0, 2.0, 0.0),      // func_1505F298
        RUN_JUMP => (0.8, 2.0, 15.0), // func_1505F298
        LAND => (1.0, 2.0, 0.0),      // func_1505A9AC
        // func_1505F298: `*0x80099660` (V q2za, q2s5w: 1.08000004 on every row of 235).
        HIGH_JUMP => (1.08, 2.0, 0.0),
        // The sea (`func_1505F298`'s swim branch, its rates at 0x80099638..44, C; V sea, swim1
        // obj+0x4C): the wade's e/18 + 0.2 (`func_1507BDB0`). SWIM_DEEP's first frames play at 2
        // (V, not modelled); SWIM_HOLD's rate is untraced (I: 1).
        WADE => (e / 18.0 + 0.2, 4.0, 0.0),
        SWIM => (0.9, 10.0, 0.0),
        SWIM_DEEP => (1.2, 10.0, 0.0),
        STROKE | STROKE_DEEP => (1.2, 9.0, 0.0),
        SWIM_HOLD => (1.0, 7.0, 0.0),
        // Combat (obj+0x4C on the combat traces; blend lengths untraced, I: 4).
        combat::TWIN_DRAW
        | combat::TWIN_HOLSTER
        | combat::PISTOL_DRAW
        | combat::PISTOL_HOLSTER
        | combat::TOMMY_DRAW
        | combat::MELEE_DRAW
        | combat::MELEE_HOLSTER
        | combat::BLADE_DRAW
        | combat::BLADE_HOLSTER
        | combat::SHOTGUN_DRAW
        | combat::SHOTGUN_HOLSTER => (1.4, 4.0, 0.0),
        combat::MELEE_SWING_A | combat::MELEE_SWING | combat::BLADE_SWING => (1.3, 4.0, 0.0),
        combat::HIT_MELEE | combat::HIT_CLUB => (1.5, 4.0, 0.0),
        combat::BLOWN_UP => (1.2, 4.0, 0.0),
        combat::GET_UP => (1.6, 4.0, 0.0),
        combat::RECOVER | combat::W14_DRAW | combat::W14_HOLSTER | combat::BURN_END => {
            (2.0, 4.0, 0.0)
        }
        combat::FLAME_HIT => (1.1, 4.0, 0.0),
        combat::MASK_ON | combat::MASK_OFF => (3.0, 4.0, 0.0),
        // The burning run: e/24 (`func_15064B94`, C; V 7/6 at speed 28 on e_w15).
        combat::BURNING => (e / 24.0, 4.0, 0.0),
        // Armed locomotion (`func_15064B94`, C): e counts the target at the weapon's speed
        // percentage (obj+0xAA); walks e/16 + 0.6, runs e/22 + 0.4, melee 0x21's e/27; the twin
        // guns' and the pistol's walks keep rate 1 in multiplayer. V: 0x124 at targets 19.2
        // (e_w21) and 35 (bots3), 69 rows; 0x15B at 19.2 (e_w22). The clip's first frame plays e/27
        // for 0x124 (not modelled). The bots' TWIN_MOVE 0x12B plays at 1 (V, 1,708 rows).
        combat::MELEE_WALK => (armed_e(id, speed, target) / 27.0, 4.0, 0.0),
        MELEE_STEP | BLADE_WALK | SNIPER_WALK | GUN23_WALK | W16_WALK | GRENADE_WALK
        | CROSSBOW_WALK => (armed_e(id, speed, target) * 0.0625 + 0.6, 4.0, 0.0),
        BLADE_RUN | SNIPER_RUN | GUN23_RUN | TWIN_RUN | PISTOL_RUN | GRENADE_RUN | FLAME_WALK
        | CROSSBOW_RUN | SHOTGUN_RUN => (armed_e(id, speed, target) / 22.0 + 0.4, 4.0, 0.0),
        // The club's walk and run (its case of `func_15064B94`, C): e/8 + 0.3 and e/20 + 0.3 with
        // the full target (V combat6_e40 801-820, ctlw40 951-985; the strafe's 65 % on 761-763).
        combat::CLUB_WALK => (effective_speed(speed, target) * 0.125 + 0.3, 4.0, 0.0),
        combat::CLUB_RUN => (effective_speed(speed, target) / 20.0 + 0.3, 4.0, 0.0),
        _ => (1.0, 4.0, 0.0),
    };
    Request {
        id,
        rate,
        blend,
        start,
    }
}

/// [`request`] with the player's speed percentage `percent` (obj+0xAA, 0 meaning 100): the armed
/// walks and runs count the target at it (C `func_15064B94`). Pass the percentage from before
/// this frame's update (the game sets it with the id after taking the rate, so a run's first frame
/// plays at 100 %), or 65 (conker-sim's `STRAFE_PERCENT`) on a frame that strafes, the first
/// included. V: the camera4 probes (86 rows of each weapon's run), the strafes of combat_ctlw_k6,
/// k10 and combat4_lock, e_w21 and e_w22's walks. Other ids: [`request`].
pub fn request_at(id: u16, speed: f32, target: f32, percent: u8) -> Request {
    let share = if percent == 0 {
        1.0
    } else {
        f32::from(percent) * f32::from_bits(0x3C23_D70A) // 0.01
    };
    let e = speed * 0.4 + target * share * 0.6;
    let rate = match id {
        combat::MELEE_WALK => e / 27.0,
        combat::MELEE_STEP
        | combat::BLADE_WALK
        | combat::SNIPER_WALK
        | combat::GUN23_WALK
        | combat::W16_WALK
        | combat::GRENADE_WALK
        | combat::CROSSBOW_WALK => e * 0.0625 + 0.6,
        combat::BLADE_RUN
        | combat::SNIPER_RUN
        | combat::GUN23_RUN
        | combat::TWIN_RUN
        | combat::PISTOL_RUN
        | combat::GRENADE_RUN
        | combat::FLAME_WALK
        | combat::CROSSBOW_RUN
        | combat::SHOTGUN_RUN => e / 22.0 + 0.4,
        combat::CLUB_WALK => e * 0.125 + 0.3,
        combat::CLUB_RUN => e / 20.0 + 0.3,
        _ => return request(id, speed, target),
    };
    Request {
        id,
        rate,
        blend: 4.0,
        start: 0.0,
    }
}

/// The effective speed an armed locomotion id's rate uses: the target counts at the weapon's speed
/// percentage, which `func_15064B94` sets with the id (C: 80 for most, 110 for melee 0x21's, 30
/// for weapon 0x16's walk; none in multiplayer for the twin guns, the pistol and the shotgun).
fn armed_e(id: u16, speed: f32, target: f32) -> f32 {
    let percent = match id {
        combat::MELEE_STEP | combat::MELEE_WALK => 1.1,
        combat::W16_WALK => 0.3,
        combat::TWIN_RUN | combat::PISTOL_RUN | combat::SHOTGUN_RUN => 1.0,
        _ => 0.8,
    };
    speed * 0.4 + target * percent * 0.6
}

/// The armed walk or run `func_15064B94` picks for the drawn weapon (s126+0x78) while the stick
/// asks for `target` ≥ 1 (C); `None` for the weapons that walk with the plain gait (0x14) or below
/// a target of 1. The club goes by the player's `speed` instead: walking from 1, running from 18
/// (C; V combat6_ctlw40 968).
pub fn armed_move(weapon: u8, speed: f32, target: f32) -> Option<u16> {
    use combat::*;
    if weapon == 0x40 {
        return (speed >= 1.0).then_some(if speed >= 18.0 { CLUB_RUN } else { CLUB_WALK });
    }
    if target < 1.0 {
        return None;
    }
    let run = target >= 20.0;
    Some(match weapon {
        0x09 if run => TWIN_RUN,
        0x09 => TWIN_WALK,
        0x39 if run => PISTOL_RUN,
        0x39 => PISTOL_WALK,
        0x23 if run => GUN23_RUN,
        0x23 => GUN23_WALK,
        0x24 if run => SNIPER_RUN,
        0x24 => SNIPER_WALK,
        0x22 if run => BLADE_RUN,
        0x22 => BLADE_WALK,
        0x21 if target >= 15.0 => MELEE_WALK,
        0x21 => MELEE_STEP,
        0x26 if run => GRENADE_RUN,
        0x26 => GRENADE_WALK,
        0x15 => FLAME_WALK,
        0x16 => W16_WALK,
        0x18 if run => CROSSBOW_RUN,
        0x18 => CROSSBOW_WALK,
        0x37 if run => SHOTGUN_RUN,
        0x37 => SHOTGUN_WALK,
        _ => return None,
    })
}

/// The grounded choice of `func_1504C0E8` (the Deathmatch subset: no water, carried objects,
/// weapons or slopes). Holds its one bit of memory, s126+0x37.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    /// Crouch-walking allowed: set when Z is held below speed 3, cleared above speed 4 without Z.
    pub crouch_walk_ok: bool,
}

impl Ground {
    /// The state (= animation id) for this frame, once the frame's speed is known. `target` is
    /// the speed the frame drives toward: obj+0x44, except that the game passes 0 while the stick
    /// points against the motion (that is how reversing at speed skids). `height` is the height
    /// above the floor (obj+0x28, 0 when grounded). The caller holds [`id::SKID`] for 17 frames
    /// and [`id::LAND`] for 15 (obj+0x83) instead of calling this, and decides jumps, landings and
    /// wall pushes itself.
    pub fn choose(&mut self, speed: f32, target: f32, z_held: bool, height: f32) -> u16 {
        use id::*;
        if z_held {
            if speed < 3.0 {
                self.crouch_walk_ok = true;
            }
            return if target > 2.6 && self.crouch_walk_ok {
                CROUCH_WALK
            } else {
                CROUCH
            };
        }
        if speed > 4.0 {
            self.crouch_walk_ok = false;
        }
        let e = effective_speed(speed, target);
        let mut state = if e > 14.0 {
            RUN
        } else if e > 6.0 || (target < 1.0 && speed > 1.0) {
            WALK
        } else if e > 1.0 {
            CREEP
        } else {
            IDLE
        };
        if height > 60.0 {
            state = FALL;
        }
        if speed > 18.0 && target == 0.0 && height == 0.0 {
            state = SKID;
        }
        state
    }
}

/// The animation clock of one character: `func_1505E0C4` (start) and `func_1507BDB0` (advance)
/// at a constant 2 VIs per frame. Field names follow the game's state at obj+0x2D0.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimPlayer {
    /// Current clip (obj+0x84).
    pub id: u16,
    /// Ticks into the clip (obj+0xB4), in `0..duration`.
    pub time: f32,
    /// Ticks per frame (obj+0x4C).
    pub rate: f32,
    /// Clip length in ticks; the clock loops on it.
    pub duration: f32,
    /// Cross-fade source: the clip and time that were showing when the blend began.
    pub prev: u16,
    pub prev_time: f32,
    /// Source weight out of 1023; the blend is over at <= 0.
    pub weight: i16,
    /// Weight lost per VI (500 / blend frames).
    pub step: i16,
}

impl Default for AnimPlayer {
    fn default() -> Self {
        Self {
            id: NONE,
            time: 0.0,
            rate: 0.0,
            duration: 0.0,
            prev: NONE,
            prev_time: 0.0,
            weight: 0,
            step: 0,
        }
    }
}

impl AnimPlayer {
    /// Requests a clip (`func_1505E650`). The same id only changes the rate; another id
    /// restarts at `req.start` and cross-fades from the pose that was showing. `duration` is the
    /// clip's length in ticks (the extractor's `duration_ticks`).
    pub fn play(&mut self, req: Request, duration: u16) {
        self.rate = req.rate;
        if req.id == self.id {
            return;
        }
        let old = self.weight;
        // A blend still mostly showing its source (>= 600) keeps that source.
        let mut snapshot = old < 600;
        if req.blend >= 1.0 {
            self.step = 500 / req.blend as i16;
            if old > 0 && self.prev == req.id {
                // Back to the clip we were fading from: fade from here, for the time we had left.
                self.weight = 1023 - old;
                snapshot = true;
            } else if snapshot {
                self.weight = 1023;
            }
        } else {
            self.weight = 0;
        }
        if snapshot {
            // ponytail: the game snapshots the blended pose; mid-blend, this keeps only the clip.
            self.prev = self.id;
            self.prev_time = self.time;
        }
        self.id = req.id;
        self.time = req.start;
        self.duration = duration as f32;
    }

    /// Advances one frame: time += rate, looping on the duration (every player clip loops; the
    /// state machine leaves the one-shots before their end). The fade loses 2 steps.
    pub fn advance(&mut self) {
        let t = self.time + self.rate;
        self.time = if t >= self.duration {
            t - self.duration
        } else {
            t
        };
        if self.weight > 0 {
            self.weight -= self.step * 2;
        }
    }

    /// One game frame: the state's request, then the clock.
    pub fn tick(&mut self, req: Request, duration: u16) {
        self.play(req, duration);
        self.advance();
    }

    /// How much of the cross-fade source to show, 0..1 (I: the game's pose mixer is unread;
    /// weight / 1023 assumed linear).
    pub fn prev_weight(&self) -> f32 {
        self.weight.max(0) as f32 / 1023.0
    }
}

#[cfg(test)]
mod tests {
    use super::id::*;
    use super::*;

    /// A traced f32 (printed with %.9g, an exact round trip).
    fn f(s: &str) -> f32 {
        s.parse().unwrap()
    }

    /// Armed rates at the player's speed percentage (obj+0x4C on camera4_w15 155 and 131,
    /// combat_ctlw_k6 271 and 273, combat4_lock 825).
    #[test]
    fn armed_rates_count_the_speed_percentage() {
        use combat::*;
        let rate = |id, v: f32, t: f32, p| request_at(id, v, t, p).rate;
        assert_eq!(rate(FLAME_WALK, 25.0, 35.0, 80), f("1.61818171"));
        assert_eq!(rate(FLAME_WALK, 1.0, 35.0, 0), f("1.37272727"));
        assert_eq!(rate(GUN23_RUN, 1.0, 34.0, 65), f("1.02090907"));
        assert_eq!(rate(GUN23_RUN, 3.0, 34.0, 65), f("1.05727267"));
        assert_eq!(rate(MELEE_WALK, 5.0, 34.0, 65), f("0.56518513"));
        assert_eq!(
            request_at(id::RUN, 1.0, 35.0, 80),
            request(id::RUN, 1.0, 35.0)
        );
    }

    #[test]
    fn rates_match_trace_values() {
        // (state, obj+0x3C, obj+0x44) -> obj+0x4C, from anim_run, anim_walk, anim_crouch, anim_ledge.
        for (id, v, t, rate) in [
            (RUN, "1", "35", "1.73043478"),
            (RUN, "35", "35", "2.3217392"),
            (RUN, "20", "20.4000015", "1.68000007"),
            (WALK, "17.9000015", "0", "1.19500017"),
            (CREEP, "1", "7.80000019", "0.735000074"),
            (CROUCH_WALK, "0", "35", "2.83333325"),
            (WALL_PUSH, "14", "35", "2.37333345"),
            (WALL_PUSH, "11.5", "0", "0.906666696"),
        ] {
            assert_eq!(request(id, f(v), f(t)).rate, f(rate), "{id} {v} {t}");
        }
        assert_eq!(request(RUN_JUMP, 35.0, 35.0).start, 15.0);
    }

    #[test]
    fn combat_rates_match_the_combat_traces() {
        // obj+0x4C on combat_e_w09, e_w21, e_gren and ctlw_k1 (combat_README.md §7).
        for (id, rate) in [
            (combat::TWIN_DRAW, "1.39999998"),
            (combat::TWIN_FIRE, "1"),
            (combat::MELEE_SWING, "1.29999995"),
            (combat::HIT_MELEE, "1.5"),
            (combat::BLOWN_UP, "1.20000005"),
            (combat::GET_UP, "1.60000002"),
            (combat::HIT, "1"),
        ] {
            assert_eq!(request(id, 0.0, 0.0).rate, f(rate), "{id:#x}");
        }
    }

    /// (speed, target, obj+0x4C) on combat_e_w21 / bots3 (0x124) and combat_e_w22 (0x15B).
    #[test]
    fn armed_walk_rates() {
        for (id, v, t, rate) in [
            (combat::MELEE_WALK, "3", "19.2000008", "0.513778"),
            (combat::MELEE_WALK, "14", "19.2000008", "0.676741"),
            (combat::MELEE_WALK, "3", "35", "0.9"),
            (combat::MELEE_WALK, "14", "35", "1.062963"),
            (combat::BLADE_WALK, "3", "19.2000008", "1.251"),
            (combat::BLADE_WALK, "14", "19.2000008", "1.526"),
            (combat::TWIN_MOVE, "10", "35", "1"),
            (combat::BURNING, "28", "28", "1.16666663"), // e_w15 834+
        ] {
            let got = request(id, f(v), f(t)).rate;
            assert!((got - f(rate)).abs() < 2e-6, "{id:#x} {v} {t}: {got}");
        }
    }

    #[test]
    fn armed_moves() {
        assert_eq!(armed_move(0x21, 0.0, 19.2), Some(combat::MELEE_WALK)); // e_w21
        assert_eq!(armed_move(0x21, 0.0, 10.0), Some(combat::MELEE_STEP));
        assert_eq!(armed_move(0x22, 0.0, 19.2), Some(combat::BLADE_WALK)); // e_w22
        assert_eq!(armed_move(0x22, 0.0, 35.0), Some(combat::BLADE_RUN));
        assert_eq!(armed_move(0x09, 0.0, 35.0), Some(combat::TWIN_RUN));
        assert_eq!(armed_move(0x14, 0.0, 35.0), None);
        assert_eq!(armed_move(0x09, 0.0, 0.5), None);
        // The club by speed (combat6_ctlw40: 0x112 from speed 1, 0x111 at 18; idle below 1).
        assert_eq!(armed_move(0x40, 17.0, 35.0), Some(combat::CLUB_WALK));
        assert_eq!(armed_move(0x40, 18.0, 35.0), Some(combat::CLUB_RUN));
        assert_eq!(armed_move(0x40, 0.5, 0.0), None);
        // Its rates: e/8 + 0.3 walking (e_w40 801: 1.79), e/20 + 0.3 running (bot 65: 1.605 at 90 %).
        assert!((request_at(combat::CLUB_WALK, 1.0, 19.2, 0).rate - 1.79).abs() < 1e-5);
        assert!((request_at(combat::CLUB_RUN, 18.0, 35.0, 90).rate - 1.605).abs() < 1e-5);
        let r = request(combat::BLADE_RUN, 35.0, 35.0).rate;
        assert!((r - ((14.0 + 16.8) / 22.0 + 0.4)).abs() < 1e-6);
    }

    #[test]
    fn ground_thresholds() {
        let mut g = Ground::default();
        assert_eq!(g.choose(0.0, 0.0, false, 0.0), IDLE);
        assert_eq!(g.choose(1.0, 4.2, false, 0.0), CREEP); // e 2.92
        assert_eq!(g.choose(4.0, 7.8, false, 0.0), WALK); // e 6.28
        assert_eq!(g.choose(5.0, 20.4, false, 0.0), RUN); // e 14.24
        assert_eq!(g.choose(1.5, 0.0, false, 0.0), WALK); // released, still moving
        assert_eq!(g.choose(0.5, 0.0, false, 0.0), IDLE);
        assert_eq!(g.choose(32.5, 0.0, false, 0.0), SKID);
        assert_eq!(g.choose(35.0, 35.0, false, 80.0), FALL);
    }

    #[test]
    fn crouch_walk_needs_a_slow_crouch_first() {
        let mut g = Ground::default();
        assert_eq!(g.choose(10.0, 35.0, true, 0.0), CROUCH);
        assert_eq!(g.choose(2.0, 35.0, true, 0.0), CROUCH_WALK);
        assert_eq!(g.choose(2.0, 2.0, true, 0.0), CROUCH);
        g.choose(5.0, 35.0, false, 0.0);
        assert!(!g.crouch_walk_ok);
    }

    #[test]
    fn clock_restarts_loops_and_keeps_time_on_same_id() {
        let mut p = AnimPlayer::default();
        p.tick(request(RUN, 1.0, 35.0), 28);
        assert_eq!((p.id, p.time), (RUN, f("1.73043478")));
        p.time = 27.0;
        p.tick(request(RUN, 35.0, 35.0), 28);
        assert_eq!(p.time, 27.0 + f("2.3217392") - 28.0);
        p.tick(request(RUN_JUMP, 35.0, 35.0), 60);
        assert_eq!(p.time, 15.8);
    }

    #[test]
    fn blend_rules_from_the_ledge_clock() {
        // anim_ledge_clock.csv: weights after each frame.
        let mut p = AnimPlayer::default();
        p.tick(request(RUN, 35.0, 35.0), 28);
        p.weight = 0;
        p.tick(request(FALL, 35.0, 35.0), 16); // step 125: 1023 -> 773
        assert_eq!((p.prev, p.weight), (RUN, 773));
        for _ in 0..14 {
            p.tick(request(FALL, 35.0, 35.0), 16);
        }
        assert_eq!((p.time, p.weight), (7.5, -227));
        p.tick(request(RUN, 35.0, 35.0), 28);
        assert_eq!((p.prev, p.prev_time, p.weight), (FALL, 7.5, 773));
        // >= 600: land keeps the fall as its source and the weight, with its own step (250).
        p.tick(request(LAND, 0.0, 0.0), 22);
        assert_eq!((p.prev, p.prev_time, p.weight), (FALL, 7.5, 273));
        // Back to the clip being faded from: weight = 1023 - old.
        let mut q = AnimPlayer::default();
        q.tick(request(RUN, 35.0, 35.0), 28);
        q.weight = 0;
        q.tick(request(WALL_PUSH, 14.0, 35.0), 43);
        q.tick(request(WALL_PUSH, 14.0, 35.0), 43); // 523 after this frame
        q.tick(request(RUN, 18.0, 35.0), 28);
        assert_eq!((q.prev, q.weight), (WALL_PUSH, 1023 - 523 - 250));
    }
}
