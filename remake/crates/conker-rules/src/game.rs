//! One Deathmatch match, a 30 Hz frame at a time: every player's movement (conker-sim) and combat
//! state, the bullets and grenades, the pickups, the timer and the kills
//! (`../lab/oracle/combat_README.md`; V on the `combat_*` traces, see `tests/combat.rs`).
//!
//! [`Match::step`] is the whole frame and depends only on the match state, the inputs and the
//! static data, so the server runs it authoritatively and a client can run the same code to
//! predict. `Match` is plain data: a rollback is a clone.
//!
//! Frame order (V: it decides who moves on the hit row): pickups (touched with the positions at
//! the frame's start), then each player in slot order (timed actions ending, a new action, the
//! action's work, its movement or knock-back, the grenades' push, then its strikes: melee hits
//! and lock-on, new bullets and grenades), then the bullets (those born this frame included),
//! then the grenades, then the timer. So a melee victim later in the slot order moves on the hit
//! row (V e_w21), a shot or blown-up one from the next row (V e_w09, e_gren 924 → 925).
//!
//! Animation timing: an action begun in frame `t` shows from trace row `t + 1`; one that lasts `n`
//! frames hands over in frame `t + n`.
//!
//! Lag compensation (I; the original is one console and has none): [`Match::step_seen`] takes where
//! each shooter saw each target ([`Seen`]) and every aimed hit test reads that instead of the
//! target's `body.pos`: bullets, rockets, aimed shots, the sniper's ray, the flame stream, the melee
//! strike and its lock-on, each per shooter (so per bullet). Explosions and fires stay where they
//! are: they belong to the world, not to anyone's aim.

use conker_anim::{combat as anim, id};
use conker_sim::collision::{Body, CollisionWorld, Sweep};
use conker_sim::movement::{
    self, BUTTON_A, BUTTON_C_LEFT, BUTTON_C_RIGHT, BUTTON_Z, Input, Other, PlayerState, Side,
    angle_diff, atan2_u16, cos_sin,
};

use crate::Rng;
use crate::damage::{self, DamageType, Facing, Hitbox, in_reach};
use crate::fidget::{self, Fidget};
use crate::flame::{self, Burn};
use crate::gas::{self, Effect, Gas};
use crate::projectile::{
    self, BOLT_STEP, Bullet, Grenade, Rocket, Shot, hits_body, local_to_world, raycast, rise_along,
};
use crate::spawn::{self, Placement, Spawn};
use crate::weapon::{self, Attack, Gives, Pickup, Weapon};
use std::collections::BTreeSet;

pub const BUTTON_B: u16 = 0x4000;
pub const BUTTON_R: u16 = 0x0010;
pub const C_DOWN: u16 = 0x0004;
pub const C_BUTTONS: u16 = 0x000F;

pub const HEALTH: u8 = 6;
/// Immune frames after a (re)spawn (obj+0x125).
pub const SPAWN_IMMUNE: u8 = 150;
/// The match clock in VIs (10 minutes, the default; `0x8008FD78`), −2 a frame.
pub const MATCH_TIME: i32 = 36_000;
/// A timed match's lives (`D_80087260`): only an untimed one counts them.
pub const TIMED_LIVES: u8 = 99;
/// obj+0x104 during a hit state, and while dead from the death's second state.
pub const STUN_HIT: u8 = 0xFE;
pub const STUN_DEAD: u8 = 0xFF;
/// Frames between the clock reaching 0 and the end fade (`func_151E53E8`), and of the fade.
pub const END_DELAY: u32 = 40;
pub const FADE: u32 = 16;
/// Grenade: pull and throw states, the frame of the release (the fuse is the cook timer,
/// [`projectile::COOK`]).
const PULL: u32 = 28;
const THROW: u32 = 28;
const RELEASE: u32 = 14;
/// The hand during the hold, in the thrower's frame (V combat4_dead2: the blast 22.3 behind, 12.2
/// to the side, 60.3 up).
const HOLD_HAND: [f32; 3] = [-22.3, -12.2, 60.3];
/// The sniper's shot leaves 3 frames after Z (V e_w24, k10), its ray reaches this far (I); its aim
/// (and a shot in progress) ends 8 frames after R is let go (V e_w24 1161, ctlw_k10 251).
const AIM_DELAY: u32 = 3;
const AIM_EXIT: u32 = 8;
/// A grenade pushes players out to this distance in xz (V: 31.64 → 32.733 and 32.747 on the
/// throws of ctl_cd and e_gren, the thrower's next frame; I: a generic object push).
const GRENADE_PUSH: f32 = 32.74;
/// Melee swings turn the attacker to the nearest player (`func_1505DADC` from `func_150641D8`, C):
/// within 200 in 3D (`0x80099538` = 40000), ±90° of the facing, ±22.5° of pitch, with a clear line
/// 80 above the feet; an unstunned target beats a nearer stunned one. The move angle snaps onto
/// it, the facing follows a frame later (V e_w21, e_w22 at 65.4: 25090 → 22687).
const LOCK_ON: f32 = 200.0;
const RAY_RANGE: f32 = 20_000.0;
const C_STATE: u32 = 3;
/// A blown-up player's bounces: the most the first landing gives back (half the fall speed, as later
/// ones: V e_gren 28, combat5_sg_e2 7.8), and the least bounce.
const BLAST_BOUNCE: f32 = 28.0;
const BOUNCE_MIN: f32 = 5.0;
const NONE: u32 = u32::MAX;

/// The static data a match needs: spawn records, pickups and their places.
#[derive(Clone, Debug, PartialEq)]
pub struct Arena {
    pub spawns: Vec<Spawn>,
    pub pickups: Vec<(Pickup, [f32; 3])>,
}

impl Arena {
    /// Scene 0x2D Deathmatch (no gas masks); `placement(n)` is placement n's position
    /// (extract-scene).
    pub fn deathmatch(spawns: Vec<Spawn>, placement: impl Fn(u16) -> [f32; 3]) -> Self {
        let pickups = (weapon::PICKUPS_2D.iter())
            .filter(|p| p.gives != Gives::Mask)
            .map(|p| (*p, placement(p.placement)))
            .collect();
        Self { spawns, pickups }
    }
}

/// The original's SET-UP options for Deathmatch (the option block `0x800E0BE0`, read at the match
/// start; `Default` is the game's defaults, `0x8008FE54`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Settings {
    /// Minutes on the clock, 3-10 (opt[3]); `None`: unlimited, no clock, and the lives count.
    pub time: Option<u8>,
    /// Lives, 1-5 (opt[2]); a timed match gives [`TIMED_LIVES`].
    pub lives: u8,
    /// The kills that end the match, 1-10 (opt[6]); 0: unlimited.
    pub kills: u8,
    /// The bots' skill (opt[16], `0x800E0BD0`): 0-4, INBRED, CRAP, NORMAL, BASTARD, EINSTEIN.
    pub skill: u8,
    /// The HUD's radar (opt[17]: HUD flag 0x20).
    pub radar: bool,
    /// War: Total War (scenario 9, scene 0x2D) instead of Deathmatch: not a SET-UP byte but the
    /// scenario, whose free-for-all flag `0x8008FDC8` is 0. Every player's team is its side (0
    /// squirrels, 1 Tediz), the match ends and ranks by team (`func_151E565C`'s team branch,
    /// [`team_places`]) and the kill limit is not read.
    pub total_war: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            time: Some(10),
            lives: 3,
            kills: 0,
            skill: 2,
            radar: true,
            total_war: false,
        }
    }
}

impl Settings {
    /// The clock at the start in VIs (minutes × 3600), 0 untimed.
    pub fn timer(&self) -> i32 {
        self.time.map_or(0, |m| i32::from(m) * 3600)
    }

    /// The lives each player starts with.
    pub fn lives(&self) -> u8 {
        if self.time.is_some() {
            TIMED_LIVES
        } else {
            self.lives
        }
    }
}

/// What a player is doing besides moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub enum Action {
    Free,
    Draw,
    Holster,
    /// Attacking. Automatic weapons stop 7 frames after Z was last held (`last_z`); `cycles` are
    /// the last two cycle starts at which Z was held.
    Fire {
        last_z: u32,
        cycles: [u32; 2],
    },
    Reload {
        frames: u16,
    },
    /// The sniper's aim (R held); Z queues the shot for frame `shot`.
    Aim {
        shot: u32,
    },
    CButton,
    GrenadePull {
        pull: u32,
    },
    GrenadeHold {
        pull: u32,
    },
    GrenadeThrow {
        pull: u32,
        hold: u32,
    },
    /// Step `step` of [`damage::sequence`]`(reaction, lethal)`.
    Hurt {
        reaction: u8,
        lethal: bool,
        step: u8,
    },
}

/// A player: its movement state and everything combat adds (named after the game's fields).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Fighter {
    pub body: PlayerState,
    /// obj+0x128: Deathmatch gives every player its own team.
    pub team: u8,
    /// The side, which picks the model and so the body ([`Hitbox`], conker-sim's `Other::player`):
    /// the Tediz unless a match puts the player with the squirrels.
    pub side: Side,
    /// The records this player respawns on ([`spawn::respawn`]'s `eligible`): every one, as in the
    /// original's Deathmatch, unless the match keeps it in its side's half ([`spawn::TOTAL_WAR_MASKS`]).
    pub spawn_mask: u32,
    /// obj+0x1CA.
    pub health: u8,
    /// obj+0x125: frames left immune to area hits and guarded direct hits; it counts down only
    /// while not stunned. 255 while dead.
    pub immune: u8,
    /// obj+0x104.
    pub stun: u8,
    /// obj+0x84: the animation id. Free players show their locomotion (or armed idle/walk).
    pub anim: u16,
    pub action: Action,
    /// The frame `action` (and `anim`) began.
    pub since: u32,
    /// The held weapon id (MP+0xEB0's pickup gives it), drawn or not.
    pub weapon: Option<u8>,
    pub drawn: bool,
    /// Rounds left before the gun reloads (s126+0x25 for the clips).
    pub rounds: u8,
    /// s126+0x19A.
    pub grenades: u8,
    /// obj+0x109: the knock-back friction of the last hit.
    pub knock_k: u8,
    /// gGameState+0x46+2i: kills of other teams' players, capped at 9999; and gGameState+0x70+0xC*i:
    /// deaths this player caused on its own team, itself included (team kills and suicides).
    pub kills: u16,
    pub suicides: u16,
    /// `0x800E0AD0[i]`: deaths, each a life lost ([`Settings::lives`]).
    pub deaths: u16,
    /// s126+0x1AA: the shots fired (a tie in kills − deaths ranks fewer first): +1 per round, swing
    /// and grenade pulled (C `func_151D5148`, `func_15074840`, `func_15065A5C`; V: twin guns,
    /// chainsaw, grenade; I: the rocket, aimed shot and ray, and the flame counts none).
    pub shots: u16,
    /// gGameState+0x66+0xC*i and +0x6A: the hits this player landed on other teams' players, and
    /// those on the head (the results' accuracy and headshots; `func_151E5FF4`: none on its own team).
    pub hits: u16,
    pub headshots: u16,
    /// A bot plays this slot (the end when no human is left in play).
    pub bot: bool,
    /// The place this player went out on, its last life lost (`0x800E0AB0[i]`); 0 in play.
    pub out: u8,
    /// struct126+0x1B0: the record this player last respawned on (not the match-start one).
    pub last_spawn: Option<usize>,
    /// This frame's and last frame's buttons (C-down and R act one frame late).
    pub buttons: u16,
    pub late: u16,
    /// The frame R stopped being held (one frame late, as R acts): the sniper's aim and shot end
    /// 8 frames later.
    pub r_off: Option<u32>,
    /// Z has been held since a frame with a weapon drawn (it does not crouch).
    pub z_armed: bool,
    /// The drawn idle's fidget (twin guns, weapons 0x23 and 0x16).
    pub fidget: Fidget,
    /// s126+0x19C: the grenade's cook timer while it is in the hand (VIs).
    pub cook: u16,
    /// The frame a grenade went off in this player's hand (s126+0x75 shows it one frame more).
    pub dropped: Option<u32>,
    /// The fire a flame lit on this player ([`flame::Burn`]).
    pub burn: Option<Burn>,
    /// s126+0x46 / 100: frames this player keeps burning (refreshed while the fire lasts).
    pub heat: u16,
    /// Frames left of a burning run's bump off a wall (0x28E).
    pub bump: u16,
    /// The first frame the drawn weapon may fire again (weapon 0x14's s126+0x72, the rocket's
    /// s126+0x19E).
    pub ready: u32,
    /// The match's start item (MP+0x1170, written by the scene's script at the match start; V:
    /// 0x40 on 0x3F, `../lab/oracle/caveman_README.md`): the weapon manager hands it to this
    /// player whenever it holds no weapon (the match start, the frame after a respawn).
    pub start: Option<u8>,
    /// The last frame the club's second swing may start: [`weapon::COMBO_WINDOW`] after this
    /// player's last landed swing (obj+0x107).
    pub combo: u32,
    /// Total War: carrying a gas canister (the script's forced item G+0xFF0 = 0x25, `gas::Effect::
    /// Carry`): no weapon drawn or used, no grenade, 80 % of the speed (`func_15064B94`'s 0x25, C;
    /// V d1: the drive fits 79.4-81.1 %), until delivered or dead.
    pub carrying: bool,
    /// Total War: wearing a gas mask (its pickup holds the slot a weapon's would, MP+0xEB0), which
    /// the gas spares; lost on death, on another pickup, or when the script takes it off.
    pub masked: bool,
    /// s126+0x170: the scope's pitch in degrees (`conker_camera::Scope::pitch`, positive looks
    /// down). Set with `body.scope_yaw` after each camera step; the sniper's ray and weapon 0x14's
    /// shot look along the scope's yaw and pitch while `body.scope_yaw` is set, else along the
    /// facing.
    pub scope_pitch: f32,
}

impl Fighter {
    /// A player (re)spawned at `place` in frame `tick`: full health, 3 grenades, no weapon, 150
    /// immune frames, one frame of animation 0 (V).
    pub fn new(place: &Placement, team: u8, tick: u32) -> Self {
        Self {
            body: PlayerState::spawn(place.pos, place.facing),
            team,
            side: Side::default(),
            spawn_mask: !0,
            health: HEALTH,
            immune: SPAWN_IMMUNE,
            stun: 0,
            anim: id::CREEP,
            action: Action::Free,
            since: tick,
            weapon: None,
            drawn: false,
            rounds: 0,
            grenades: weapon::START_GRENADES,
            knock_k: 0,
            kills: 0,
            suicides: 0,
            deaths: 0,
            shots: 0,
            hits: 0,
            headshots: 0,
            bot: false,
            out: 0,
            last_spawn: None,
            buttons: 0,
            late: 0,
            r_off: None,
            z_armed: false,
            fidget: Fidget::default(),
            cook: 0,
            dropped: None,
            burn: None,
            heat: 0,
            bump: 0,
            ready: 0,
            start: None,
            combo: 0,
            carrying: false,
            masked: false,
            scope_pitch: 0.0,
        }
    }

    /// No one, in a slot nobody plays (a free one, or one a client has not heard of): dead and far
    /// away, so nothing reaches it and it pushes, touches and blocks nothing. The respawn picker
    /// counts it as an enemy, but its distances stop at 1e8 (10,000 units), so the picks don't change.
    pub fn vacant(tick: u32) -> Self {
        let far = Placement {
            index: 0,
            pos: [1e6, -1e6, 1e6],
            facing: 0,
            camera: [0.0; 3],
        };
        Self {
            health: 0,
            ..Self::new(&far, u8::MAX, tick)
        }
    }

    pub fn weapon(&self) -> Option<&'static Weapon> {
        self.weapon.and_then(weapon::weapon)
    }

    /// s126+0x75: 0x80 | the current item (0x26, the grenade, from the pull to 15 frames into the
    /// throw; the canister carried, the mask worn), 0 with nothing; `tick` is the next frame to run
    /// (the trace row).
    pub fn item(&self, tick: u32) -> u8 {
        match self.action {
            _ if self.carrying => 0x80 | gas::CANISTER,
            Action::GrenadePull { .. } | Action::GrenadeHold { .. } => 0xA6,
            Action::GrenadeThrow { .. } if tick <= self.since + RELEASE + 1 => 0xA6,
            _ if self.dropped.is_some_and(|d| tick <= d + 1) => 0xA6,
            _ if self.masked => 0x80 | gas::MASK,
            _ => self.weapon.map_or(0, |w| 0x80 | w),
        }
    }

    /// s126+0x78, the weapon the camera works with (`conker_camera::PlayerView::weapon`): the drawn
    /// weapon from its draw's first frame, 0 from the holster's, 0x26 while a grenade is pulled and
    /// held (V: the combat_ctlw probes and combat_ctl_cd; I: a grenade pulled with a weapon drawn,
    /// every traced pull was unarmed).
    pub fn camera_weapon(&self) -> u8 {
        match self.action {
            Action::GrenadePull { .. } | Action::GrenadeHold { .. } => 0x26,
            Action::Holster | Action::GrenadeThrow { .. } => 0,
            Action::Draw => self.weapon.unwrap_or(0),
            _ if self.drawn => self.weapon.unwrap_or(0),
            _ => 0,
        }
    }

    /// s126+0x11A = 2: the weapon (or grenade) is in the hand (`conker_camera::PlayerView::in_hand`:
    /// a scoped weapon's scope may open). It attaches this many frames into the draw or the pull
    /// (V: the combat_ctlw probes, ctl_cd; the camera agent's camera4 traces) and leaves with the
    /// holster's or throw's first frame. `tick` is the next frame to run, as in [`Self::item`].
    pub fn in_hand(&self, tick: u32) -> bool {
        let attach = |id: u8| match id {
            0x39 | 0x23 | 0x37 => 7,
            0x15 => 8,
            0x18 => 20,
            0x14 => 11,
            0x21 => 12,
            0x22 => 15,
            0x24 => 17,
            0x26 => 18,
            _ => 10, // 0x09, 0x16
        };
        match self.camera_weapon() {
            0 => false,
            id if matches!(
                self.action,
                Action::Draw | Action::GrenadePull { .. } | Action::GrenadeHold { .. }
            ) =>
            {
                let start = match self.action {
                    Action::GrenadePull { pull } | Action::GrenadeHold { pull } => pull,
                    _ => self.since,
                };
                tick > start + attach(id)
            }
            _ => true,
        }
    }

    /// What the follow camera reads from this player (conker-sim's `follow::view` plus the
    /// rules' fields): pass it to `Follow::step_view` after [`Match::step`], with `tick` =
    /// [`Match::tick`].
    pub fn camera_view(&self, tick: u32) -> conker_camera::PlayerView {
        conker_camera::PlayerView {
            weapon: self.camera_weapon(),
            in_hand: self.in_hand(tick),
            twist: self.body.twist,
            stunned: self.stun != 0,
            aim_blocked: self.health == 0,
            ..conker_sim::follow::view(&self.body)
        }
    }

    /// What the follow camera leaves for the next step, once it has stepped with
    /// [`Self::camera_view`]: R's aim gate on the stick (`body.aiming`) and the scope's look
    /// (`body.scope_yaw`, [`Self::scope_pitch`]; 0 while it is shut), which the sniper's ray and
    /// weapon 0x14's shot follow.
    pub fn take_camera(&mut self, follow: &conker_sim::follow::Follow) {
        let scope = follow.cam.scope;
        self.body.aiming = follow.aiming();
        self.body.scope_yaw = scope.on.then_some(scope.yaw);
        self.scope_pitch = if scope.on { scope.pitch } else { 0.0 };
    }

    fn idle_anim(&self) -> u16 {
        match self.weapon() {
            _ if self.carrying => anim::CARRY_IDLE,
            Some(w) if self.drawn => w.idle,
            _ => u16::from(self.body.state),
        }
    }

    fn set(&mut self, action: Action, anim: u16, tick: u32) {
        (self.action, self.anim, self.since) = (action, anim, tick);
    }

    /// Back to normal after a non-lethal hit sequence. The stun clears with it, so the immune
    /// frames count from the next frame (V e_w15 834, 969), except after a blast's recovery 0xF0
    /// (a held substate), where it clears a frame later (V e_gren 1038 → 1069).
    fn recover(&mut self, tick: u32, reaction: u8) {
        if reaction != 0x2B {
            self.stun = 0;
        }
        let b = &mut self.body;
        b.state = movement::state::IDLE;
        b.move_angle = b.facing;
        (b.xz_speed, b.y_vel, b.gravity, b.airborne) =
            (0.0, movement::GROUND_Y_VEL, movement::GRAVITY_FALL, false);
        let anim = self.idle_anim();
        self.set(Action::Free, anim, tick);
    }
}

/// A pickup's state: frames until it is back, and who touched it in which frame (delivered on
/// the next frame for grenades, the one after for a weapon; V pick87, pick57).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickupState {
    pub timer: u16,
    pub taken: Option<(usize, u32)>,
}

/// What happened in a frame (for HUD, sounds, the client's camera on respawn, and the tests).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Hit {
        victim: usize,
        attacker: usize,
        damage: u8,
        lethal: bool,
    },
    /// A bullet left `player`'s gun at `pos` (muzzle flash, sound).
    Shot {
        player: usize,
        pos: [f32; 3],
    },
    Respawn {
        player: usize,
        at: Placement,
    },
    Pickup {
        player: usize,
        pickup: usize,
    },
    Explosion {
        pos: [f32; 3],
        owner: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    /// The end fade (`0x800E0B94` = 7).
    Fading,
    /// The match is over (the game loads the results scene).
    Over,
}

/// What a player's place depends on ([`places`], [`team_places`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Standing {
    /// [`Fighter::team`].
    pub team: u8,
    pub kills: u16,
    pub deaths: u16,
    pub shots: u16,
    /// [`Fighter::out`].
    pub out: u8,
}

impl Standing {
    pub fn of(f: &Fighter) -> Self {
        Self {
            team: f.team,
            kills: f.kills,
            deaths: f.deaths,
            shots: f.shots,
            out: f.out,
        }
    }
}

/// Each player's place, 1 the best, as the original ranks them (`func_151E565C`; 0 for `None`, a
/// free slot): a player out of lives keeps the place it went out on; the others rank among
/// themselves by kills − deaths, those at the kill limit `kill_limit` (once reached) above the
/// rest, then fewer shots (s126+0x1AA) first. Still tied, they share the place (the game's slot
/// rule only orders bots, which have no rank there).
pub fn places(all: &[Option<Standing>], kill_limit: u8) -> Vec<u8> {
    let in_play = || all.iter().flatten().filter(|s| s.out == 0);
    let reached = kill_limit > 0 && in_play().any(|s| s.kills >= u16::from(kill_limit));
    let score = |s: &Standing| {
        let bonus = reached && s.kills >= u16::from(kill_limit);
        i32::from(s.kills) + 1000 * i32::from(bonus) - i32::from(s.deaths)
    };
    let ahead = |a: &Standing, b: &Standing| {
        score(a) > score(b) || (score(a) == score(b) && a.shots < b.shots)
    };
    (all.iter())
        .map(|s| match s {
            None => 0,
            Some(s) if s.out > 0 => s.out,
            Some(s) => 1 + in_play().filter(|o| ahead(o, s)).count() as u8,
        })
        .collect()
}

/// The two teams' places in Total War, [0] the squirrels', [1] the Tediz' (`func_151E565C`'s team
/// branch, `0x800E0AC0`): a team with no member in play is out and keeps the place it went out on,
/// the first of the two out 2nd (an empty team is out from the start, as the original's unused
/// teams are); the others rank by their members' kills, then fewer shots (s126+0x1AA, bots'
/// included). Both on 1st is a DRAW. ponytail: kills in both timings; untimed, the game ranks the
/// teams in play by Σ(lives·6 + health) first (C), which nothing shows: an untimed Total War ends
/// only once a team is out (V `elim`), and the one left is 1st whatever it scored. Two teams out:
/// the one whose last member went out first is 2nd (I; the original checks team 0 first, C).
pub fn team_places(all: &[Option<Standing>]) -> [u8; 2] {
    let members = |t: u8| all.iter().flatten().filter(move |s| s.team == t);
    let sum = |t, f: fn(&Standing) -> u16| members(t).map(|s| u32::from(f(s))).sum::<u32>();
    // A member's `out` is the players' place it went out on: the larger, the earlier.
    let out = |t| {
        (members(t).all(|s| s.out > 0)).then(|| members(t).map(|s| s.out).min().unwrap_or(u8::MAX))
    };
    let ahead = |a, b| {
        let (ka, kb) = (sum(a, |s| s.kills), sum(b, |s| s.kills));
        ka > kb || (ka == kb && sum(a, |s| s.shots) < sum(b, |s| s.shots))
    };
    [0, 1].map(|t| match (out(t), out(1 - t)) {
        (Some(a), Some(b)) => 1 + u8::from(a > b),
        (Some(_), None) => 2,
        (None, Some(_)) => 1,
        (None, None) => 1 + u8::from(ahead(1 - t, t)),
    })
}

/// The winning team of [`team_places`] (0 squirrels, 1 Tediz); `None`: a DRAW, both 1st (the STATS'
/// header `func_151E43DC`; V `to_draw`: 0-0 kills and shots).
pub fn team_result(all: &[Option<Standing>]) -> Option<u8> {
    match team_places(all) {
        [1, 1] => None,
        p => p.iter().position(|&r| r == 1).map(|t| t as u8),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// The next frame to run (= the trace row its result shows on).
    pub tick: u32,
    pub players: Vec<Fighter>,
    pub bullets: Vec<(u32, Bullet)>,
    pub grenades: Vec<(u32, Grenade)>,
    /// Rockets (weapon 0x16) and aimed shots (weapon 0x14) in flight, with their birth frame.
    pub rockets: Vec<(u32, Rocket)>,
    pub shots: Vec<(u32, Shot)>,
    pub pickups: Vec<PickupState>,
    /// VIs left (0 untimed).
    pub timer: i32,
    /// The frame an end condition held ([`Self::ends`]): the clock reached 0, say.
    pub ended_at: Option<u32>,
    pub settings: Settings,
    pub rng: Rng,
    /// Slots whose player left, kept for the next to join ([`Self::join`]): a [`Fighter::vacant`]
    /// that the frame skips. The other players keep their slots.
    pub free: BTreeSet<usize>,
    /// Total War's gas: the props the AI script drives and its countdown ([`Self::apply`]).
    pub gas: Gas,
}

/// Where shooter `s` saw player `v` this frame (`seen(s, v)`; lag compensation): `None` tests `v`
/// where it is.
pub type Seen<'a> = &'a dyn Fn(usize, usize) -> Option<[f32; 3]>;

/// [`Seen`] without lag compensation.
fn here(_: usize, _: usize) -> Option<[f32; 3]> {
    None
}

/// A strike decided in a player's frame, applied once its fighter is released.
enum Strike {
    Bullet(Bullet),
    Melee(DamageType),
    Ray(DamageType, [f32; 3]),
    Grenade(Grenade),
    /// The flame stream's `k`-th frame.
    Flame(DamageType, u32),
    Rocket(Rocket),
    /// An aimed shot (weapon 0x14's, the crossbow's): its kind, where it leaves (y from the eye,
    /// side, the eye's height above the feet) and its step.
    Aimed(DamageType, [f32; 3], f32),
    /// The crossbow's bolt from the hip, from this point along the facing.
    Bolt(DamageType, [f32; 3]),
    /// The shotgun's spheres from this muzzle ([`weapon::SPREAD`]).
    Spread([f32; 3]),
    /// A grenade going off in the hand.
    Blast([f32; 3]),
}

impl Match {
    /// A Deathmatch from the start, as the game places it (`spawn` module): one draw per human on
    /// record 0 at the scene load, then every slot (humans first) on its [`spawn::DEATHMATCH_START`]
    /// record. Each slot is its own team.
    pub fn deathmatch(
        arena: &Arena,
        humans: usize,
        players: usize,
        mut rng: Rng,
        tick: u32,
    ) -> Self {
        for _ in 0..humans {
            spawn::place(&arena.spawns, 0, &mut rng);
        }
        let players = (0..players)
            .map(|i| {
                let place = spawn::place(
                    &arena.spawns,
                    usize::from(spawn::DEATHMATCH_START[i % 16]),
                    &mut rng,
                );
                Fighter::new(&place, i as u8, tick)
            })
            .collect();
        Self::new(arena, players, rng, tick)
    }

    pub fn new(arena: &Arena, players: Vec<Fighter>, rng: Rng, tick: u32) -> Self {
        Self {
            tick,
            players,
            bullets: Vec::new(),
            grenades: Vec::new(),
            rockets: Vec::new(),
            shots: Vec::new(),
            pickups: vec![PickupState::default(); arena.pickups.len()],
            timer: MATCH_TIME,
            ended_at: None,
            settings: Settings::default(),
            rng,
            free: BTreeSet::new(),
            gas: Gas::default(),
        }
    }

    pub fn phase(&self) -> Phase {
        match self.ended_at {
            Some(z) if self.tick > z + END_DELAY + FADE => Phase::Over,
            Some(z) if self.tick > z + END_DELAY => Phase::Fading,
            _ => Phase::Playing,
        }
    }

    /// Hands player `p` weapon `id`, holstered, as collecting its pickup does (MP+0xEB0, so a
    /// mask worn goes).
    pub fn give(&mut self, p: usize, id: u8) {
        let f = &mut self.players[p];
        f.weapon = Some(id);
        f.drawn = false;
        f.masked = false;
        if !matches!(f.action, Action::Hurt { .. } | Action::Free) {
            let anim = f.idle_anim();
            f.set(Action::Free, anim, self.tick);
        }
    }

    /// Player `i` left: its slot stays free until someone joins ([`Self::join`]), so nobody else
    /// changes slot; its projectiles, its pickups not yet delivered and the fires it lit go.
    pub fn remove_player(&mut self, i: usize) {
        self.bullets.retain(|b| b.1.owner != i);
        self.grenades.retain(|g| g.1.owner != i);
        self.rockets.retain(|r| r.1.owner != i);
        self.shots.retain(|s| s.1.owner != i);
        for p in &mut self.pickups {
            p.taken = p.taken.filter(|t| t.0 != i);
        }
        for f in &mut self.players {
            f.burn = f.burn.filter(|b| b.attacker != i);
        }
        self.players[i] = Fighter::vacant(self.tick);
        self.free.insert(i);
    }

    /// The slot the next player to join takes: the lowest free one, else a new one.
    pub fn next_slot(&self) -> usize {
        self.free.first().copied().unwrap_or(self.players.len())
    }

    /// Player `f` joins in [`Self::next_slot`], which it returns.
    pub fn join(&mut self, f: Fighter) -> usize {
        match self.free.pop_first() {
            Some(i) => {
                self.players[i] = f;
                i
            }
            None => {
                self.players.push(f);
                self.players.len() - 1
            }
        }
    }

    /// One frame of player `i` alone, for a client predicting its own player: its frame as
    /// [`Self::step_seen`] runs it (actions, movement or knock-back, death and respawn, the strikes
    /// it makes, aimed at the others where `seen` says it showed them), the fire burning on it, the
    /// match's grenades (a client puts its own there: they push it and blow it up) and its root's
    /// clock. The others stand where they are, bullets, rockets and shots stay, no pickup is
    /// touched and the clock stays: the server decides those.
    pub fn step_player(
        &mut self,
        i: usize,
        arena: &Arena,
        input: &Input,
        world: &CollisionWorld,
        seen: Seen,
    ) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.phase() == Phase::Over {
            return ev;
        }
        let mut late = Vec::new();
        self.player(i, input, arena, world, seen, &mut late, &mut ev);
        self.burn_player(i, &mut ev);
        self.fly_grenades(world, &mut ev);
        let rays = self.late_strikes(late, world, seen, &mut ev);
        if self.players[i].out == 0 {
            animate(&mut self.players[i], world);
        }
        for (s, kind, origin) in rays {
            self.ray(s, kind, origin, world, seen, &mut ev);
        }
        self.tick += 1;
        ev
    }

    /// One frame. `inputs[i]` is player i's (conker-sim's: stick, buttons, last frame's camera);
    /// a player without one this frame (late or gone) runs on a neutral input.
    pub fn step(&mut self, arena: &Arena, inputs: &[Input], world: &CollisionWorld) -> Vec<Event> {
        self.step_seen(arena, inputs, world, &here)
    }

    /// [`Self::step`] with every aimed hit test against the targets where their shooter saw them
    /// (`seen`, a server's lag compensation; see the module docs).
    pub fn step_seen(
        &mut self,
        arena: &Arena,
        inputs: &[Input],
        world: &CollisionWorld,
        seen: Seen,
    ) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.phase() == Phase::Over {
            return ev;
        }
        self.pickups(arena, &mut ev);
        self.gas.step(self.tick);
        let mut late = Vec::new();
        for i in 0..self.players.len() {
            if self.free.contains(&i) {
                continue;
            }
            let input = inputs.get(i).copied().unwrap_or_default();
            self.player(i, &input, arena, world, seen, &mut late, &mut ev);
        }
        self.burn(&mut ev);
        self.fly_bullets(seen, &mut ev);
        self.fly_rockets(world, seen, &mut ev);
        self.fly_shots(world, seen, &mut ev);
        self.fly_grenades(world, &mut ev);
        let rays = self.late_strikes(late, world, seen, &mut ev);
        // The renderer measures every player's root once the frame's updates are done (conker-sim's
        // `animate`; V: 0x14's shot kills in its own update after the victim's, and the victim's
        // death 0x17E drifts from the third row on, combat4_w14 835).
        for (i, f) in self.players.iter_mut().enumerate() {
            if !self.free.contains(&i) && f.out == 0 {
                animate(f, world);
            }
        }
        // The sniper's rays come last: its victim's death clip starts on the next frame, so the
        // drift begins a row later than a shot's and 0x17E plays 76 frames, not 75 (V e_w24 907,
        // combat4_w24; C: `func_150636F0` runs after the objects' updates, I: after the render).
        for (i, kind, origin) in rays {
            self.ray(i, kind, origin, world, seen, &mut ev);
        }
        self.timer = (self.timer - 2).max(0);
        if self.ended_at.is_none() && self.ends() {
            self.ended_at = Some(self.tick);
        }
        self.tick += 1;
        ev
    }

    /// The match with `settings`: its clock, and the lives its players start with.
    pub fn with(self, settings: Settings) -> Self {
        Self {
            timer: settings.timer(),
            settings,
            ..self
        }
    }

    /// Slot `i` holds a player still in play (not free, not out of lives).
    pub fn in_play(&self, i: usize) -> bool {
        !self.free.contains(&i) && self.players[i].out == 0
    }

    /// The original's end conditions (`func_151E565C`, Deathmatch): the clock reached 0 (timed);
    /// a player has the kill limit; once a player is out of lives, fewer than 2 players are left
    /// in play, or no human (the game counts down its players and humans alive as they go out).
    /// Total War (its team branch): the clock, or once a player is out, fewer than 2 teams with a
    /// member in play (C; V `elim`). No kill limit (only the FFA branch reads opt[6], C), and a human
    /// out while a team-mate plays on ends nothing (V `elim_tk`); the branch's "no team that started
    /// with a human is in" falls under the 2 teams. The game's check needs no one out (a one-sided
    /// roster ends at once there, which its team screen prevents); players join the remake's
    /// matches as they go, so it waits for an elimination as Deathmatch's does.
    pub fn ends(&self) -> bool {
        let s = &self.settings;
        let taken = || (0..self.players.len()).filter(|i| !self.free.contains(i));
        let in_play = || {
            taken()
                .filter(|&i| self.in_play(i))
                .map(|i| &self.players[i])
        };
        let out = || {
            taken()
                .filter(|&i| !self.in_play(i))
                .map(|i| &self.players[i])
        };
        let time_up = s.time.is_some() && self.timer == 0;
        if s.total_war {
            let teams = [0, 1].iter().filter(|&&t| in_play().any(|f| f.team == t));
            return time_up || (out().next().is_some() && teams.count() < 2);
        }
        let kills = s.kills > 0 && taken().any(|i| self.players[i].kills >= u16::from(s.kills));
        let alone = out().next().is_some() && in_play().count() < 2;
        let no_human = out().any(|f| !f.bot) && in_play().all(|f| f.bot);
        time_up || kills || alone || no_human
    }

    /// Every slot's place, 1 the best ([`places`]; in Total War its team's, [`team_places`]; free
    /// slots 0).
    pub fn places(&self) -> Vec<u8> {
        let all: Vec<Option<Standing>> = (self.players.iter().enumerate())
            .map(|(i, f)| (!self.free.contains(&i)).then(|| Standing::of(f)))
            .collect();
        if self.settings.total_war {
            let teams = team_places(&all);
            let place = |s: &Standing| teams.get(usize::from(s.team)).copied().unwrap_or(0);
            return all.iter().map(|s| s.as_ref().map_or(0, place)).collect();
        }
        places(&all, self.settings.kills)
    }

    fn pickups(&mut self, arena: &Arena, ev: &mut Vec<Event>) {
        let t = self.tick;
        for (k, (def, pos)) in arena.pickups.iter().enumerate() {
            let p = &mut self.pickups[k];
            if let Some((who, at)) = p.taken {
                match def.gives {
                    Gives::Grenades if t == at + 1 => {
                        let f = &mut self.players[who];
                        f.grenades = (f.grenades + 3).min(weapon::MAX_GRENADES);
                        p.taken = None;
                    }
                    Gives::Weapon(id) if t == at + 2 => {
                        p.taken = None;
                        self.give(who, id);
                    }
                    // The mask takes the weapon's place (V d2: MP+0xEB0 13 → 19, s126+0x75 0xA1 →
                    // 0xA0, as a weapon's two frames after the touch).
                    Gives::Mask if t == at + 2 => {
                        p.taken = None;
                        let f = &mut self.players[who];
                        (f.weapon, f.drawn, f.masked) = (None, false, true);
                    }
                    _ => {}
                }
            }
            let p = &mut self.pickups[k];
            p.timer = p.timer.saturating_sub(1);
            if p.timer > 0 {
                continue;
            }
            // A mask is touched only while the gas counts down (`func_1511DF6C` skips model 0x54 on
            // scene 0x2D while G+0x1FE4 ≤ 0, C), and not by a player wearing one (I).
            let countdown = self.gas.countdown;
            let may = |f: &Fighter| def.gives != Gives::Mask || (countdown > 0 && !f.masked);
            let toucher = self
                .players
                .iter()
                .position(|f| f.health > 0 && may(f) && weapon::touches(*pos, f.body.pos));
            if let Some(i) = toucher {
                *p = PickupState {
                    timer: weapon::PICKUP_RESPAWN,
                    taken: Some((i, t)),
                };
                ev.push(Event::Pickup {
                    player: i,
                    pickup: k,
                });
            }
        }
        // A canister shown is touched as a pickup is, by a player its gate admits: its +0x80 takes
        // the toucher (slot + 1) for the script, and it shrinks away (ponytail: hidden at once; the
        // game's shrink takes 3 frames, V d2 1039 → 1042, which nobody touches it in).
        for (placement, pos) in gas::CANISTERS {
            let Some(&prop) = self.gas.prop(placement).filter(|p| p.shown) else {
                continue;
            };
            let toucher = self.players.iter().position(|f| {
                f.health > 0 && gas::admits(prop.gate, f.team) && weapon::touches(pos, f.body.pos)
            });
            if let (Some(i), Some(p)) = (toucher, self.gas.prop_mut(placement)) {
                (p.flag, p.since, p.shown) = (i as i32 + 1, t, false);
            }
        }
    }

    /// Carries out what the AI script did this frame ([`gas::Effect`]), before [`Self::step`];
    /// returns the hits' events.
    pub fn apply(&mut self, effects: &[Effect]) -> Vec<Event> {
        let t = self.tick;
        let mut ev = Vec::new();
        for &e in effects {
            match e {
                Effect::Show(n) => {
                    if let Some(p) = self.gas.prop_mut(n) {
                        p.shown = true;
                    }
                }
                Effect::Set {
                    placement,
                    flag,
                    gate,
                } => {
                    if let Some(p) = self.gas.prop_mut(placement) {
                        if let Some(v) = flag.filter(|&v| v != p.flag) {
                            (p.flag, p.since) = (v, t);
                        }
                        p.gate = gate.unwrap_or(p.gate);
                    }
                }
                Effect::Countdown(n) => self.gas.countdown = n,
                Effect::Gas(on) => self.gas.gas = on,
                Effect::Hit {
                    slot,
                    kind,
                    attacker,
                } if slot < self.players.len() => {
                    let attacker = attacker.filter(|&a| a < self.players.len());
                    self.hurt(slot, &kind, false, false, 0, attacker, &mut ev);
                }
                Effect::Release(i) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.masked = false;
                    }
                }
                // The canister in hand: holstered, and whatever the player was doing ends (I).
                Effect::Carry(i, true) if self.players.get(i).is_some_and(|f| f.health > 0) => {
                    let f = &mut self.players[i];
                    f.drawn = false;
                    f.carrying = true;
                    if !matches!(f.action, Action::Hurt { .. }) {
                        let anim = f.idle_anim();
                        f.set(Action::Free, anim, t);
                    }
                }
                Effect::Carry(i, false) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.carrying = false;
                    }
                }
                _ => {}
            }
        }
        ev
    }

    /// The gas as at the match start, for a script started afresh: no canister carried, no mask
    /// worn.
    pub fn restart_gas(&mut self) {
        self.gas = Gas::default();
        for f in &mut self.players {
            (f.carrying, f.masked) = (false, false);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn player(
        &mut self,
        i: usize,
        input: &Input,
        arena: &Arena,
        world: &CollisionWorld,
        seen: Seen,
        deferred: &mut Vec<(usize, Strike)>,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        if self.players[i].out > 0 {
            return;
        }
        // The match's start item: the manager hands it over whenever the player holds no weapon
        // (`func_1508DC24` with no pickup held: MP+0x1170; V combat6_e40 1214, the frame after
        // the respawn).
        if let (None, Some(id)) = (self.players[i].weapon, self.players[i].start) {
            self.give(i, id);
        }
        // The other players push this one out of their bodies (conker-sim's `step_among`), those
        // earlier in the slot order where they moved to this frame.
        let others: Vec<Other> = (self.players.iter().enumerate())
            .filter(|&(j, _)| j != i)
            .map(|(_, p)| Other::player(p.side, p.body.pos))
            .collect();
        let melee = |f: &Fighter| {
            f.drawn
                && f.weapon()
                    .is_some_and(|w| matches!(w.attack, Attack::Melee { .. }))
        };
        let lock_to = melee(&self.players[i])
            .then(|| {
                let me = &self.players[i].body;
                self.lock_on(i, me.pos, me.facing, LOCK_ON, 0x40, world, seen)
            })
            .flatten()
            .map(|l| l.1);
        let f = &mut self.players[i];
        let (late, late2) = (f.buttons, f.late);
        (f.late, f.buttons) = (f.buttons, input.buttons);
        f.r_off = if late & BUTTON_R != 0 {
            None
        } else {
            f.r_off.or(Some(t))
        };
        let unaimed = f.r_off.is_some_and(|r| t >= r + AIM_EXIT);
        if f.stun == 0 {
            f.immune = f.immune.saturating_sub(1);
        } else if f.stun == STUN_HIT && f.action == Action::Free {
            f.stun = 0;
        }
        // s126+0x46 drops every frame, hit states included; a fire keeps it up (`Self::burn`).
        let burning = f.heat > 0;
        f.heat = f.heat.saturating_sub(1);
        if let Action::Hurt {
            reaction,
            lethal,
            step,
        } = f.action
        {
            self.hurt_frame(i, reaction, lethal, step, arena, world, &others, ev);
            return;
        }
        let held = |b: u16| input.buttons & b != 0;
        let pressed = |b: u16| held(b) && late & b == 0;
        let late_held = |b: u16| late & b != 0;
        let late_pressed = |b: u16| late_held(b) && late2 & b == 0;
        let w = f.weapon();
        let mut strikes = Vec::new();
        f.z_armed = held(BUTTON_Z) && (f.z_armed || f.drawn);
        // Timed actions end first: a press in their last frame starts the next one (V ctlw_k5: B
        // in the reload's last frame holsters).
        let ended = f.action;
        let n = t - f.since;
        match (f.action, w) {
            (Action::Draw, Some(w)) if n >= u32::from(w.draw.1) => {
                f.drawn = true;
                f.set(Action::Free, w.idle, t);
            }
            (Action::Holster, Some(w)) if n >= u32::from(w.holster.1) => {
                f.drawn = false;
                f.set(Action::Free, id::IDLE, t);
            }
            (Action::Reload { frames }, Some(w)) if n >= u32::from(frames) => {
                f.set(Action::Free, w.idle, t)
            }
            (Action::CButton, Some(w)) if n >= C_STATE => f.set(Action::Free, w.idle, t),
            (Action::GrenadeThrow { .. }, _) if n >= THROW => {
                let anim = f.idle_anim();
                f.set(Action::Free, anim, t);
            }
            (
                Action::Draw
                | Action::Holster
                | Action::Reload { .. }
                | Action::CButton
                | Action::Fire { .. }
                | Action::Aim { .. },
                None,
            ) => f.set(Action::Free, id::IDLE, t),
            _ => {}
        }
        // Then a new action; the frame's own work (a shot on frame 0) follows below. Weapon 0x23
        // with R held starts nothing (V ctlw_k6: B and Z ignored; I: an aim without its own
        // state); weapon 0x16 with R held does not fire (V combat4_w16 980) but holsters (ctlw_k7
        // 190).
        let armed = w.filter(|_| f.drawn);
        let aiming = armed.is_some_and(|w| w.id == 0x23) && late_held(BUTTON_R);
        let no_fire = armed.is_some_and(|w| matches!(w.id, 0x16 | 0x40)) && late_held(BUTTON_R);
        // The club: a Z press in a swing draws the RNG (`func_150641D8`, C) and, within
        // `COMBO_WINDOW` frames of the first swing's hit, chains the second (V combat6_e40 870,
        // 990; 1110: no hit, nothing).
        if let Action::Fire { .. } = f.action
            && armed.is_some_and(|w| w.id == 0x40)
            && pressed(BUTTON_Z)
            && !no_fire
        {
            self.rng.next_u32();
            if f.anim == anim::CLUB_SWING && t <= f.combo {
                let fire = Action::Fire {
                    last_z: t,
                    cycles: [NONE; 2],
                };
                f.set(fire, weapon::CLUB_COMBO.0.0, t);
                f.shots = f.shots.saturating_add(1);
            }
        }
        // The shotgun fires again as soon as its cooldown allows, cutting the long 0x235 short (V
        // combat5_sg3: a shot every 60 frames).
        if matches!(f.action, Action::Fire { .. })
            && armed.is_some_and(|w| matches!(w.attack, Attack::Spread { .. }))
            && pressed(BUTTON_Z)
            && t >= f.ready
        {
            f.action = Action::Free;
        }
        if f.action == Action::Free && !aiming && !f.carrying {
            if late_pressed(C_DOWN) && f.grenades > 0 && armed.is_none_or(|w| w.c_button.is_none())
            {
                f.set(Action::GrenadePull { pull: t }, anim::GRENADE_PULL, t);
                f.cook = projectile::COOK;
                f.shots = f.shots.saturating_add(1);
            } else if let Some(w) = w.filter(|_| pressed(BUTTON_B)) {
                if f.drawn {
                    f.set(Action::Holster, w.holster.0, t);
                } else {
                    f.rounds = w.rounds;
                    f.set(Action::Draw, w.draw.0, t);
                }
            } else if let Some(w) = armed {
                let z = if w.automatic() {
                    held(BUTTON_Z)
                } else {
                    pressed(BUTTON_Z)
                };
                if z && !matches!(w.attack, Attack::Ray { .. }) && t >= f.ready && !no_fire {
                    // A swing draws the RNG once (`func_150641D8`, C; V combat4_e_w21, ctlw_k1
                    // with the game's state): an odd draw swings 0x12A, else 0x129 (0x22's two
                    // choices are both 0x15F).
                    let swing =
                        matches!(w.attack, Attack::Melee { .. }) && self.rng.next_u32() & 1 == 0;
                    let alt =
                        matches!(w.attack, Attack::Spread { .. }) && self.rng.next_u32() & 3 == 0;
                    let anim = match w.attack {
                        _ if swing && w.id == 0x21 => anim::MELEE_SWING_A,
                        Attack::Shot { anim, .. } if late_held(BUTTON_R) => anim.0,
                        Attack::Spread { alt: a, .. } if alt => a.0,
                        _ => w.fire.0,
                    };
                    f.set(
                        Action::Fire {
                            last_z: t,
                            cycles: [NONE; 2],
                        },
                        anim,
                        t,
                    );
                    f.shots = f
                        .shots
                        .saturating_add(u16::from(matches!(w.attack, Attack::Melee { .. })));
                } else if let Some(r) = w.reload.filter(|_| pressed(BUTTON_A)) {
                    f.rounds = w.rounds;
                    f.set(Action::Reload { frames: r.1 }, r.0, t);
                } else if let Some(aim) = w.aim().filter(|_| late_held(BUTTON_R)) {
                    f.set(Action::Aim { shot: NONE }, aim, t);
                } else if let Some(c) = w.c_button.filter(|_| {
                    // Not in a jump or its landing (V combat6_ctlw40 730: the club's C-up ignored).
                    use movement::state::{FALL, JUMP, LAND, RUN_JUMP};
                    input.buttons & C_BUTTONS & !late != 0
                        && !matches!(f.body.state, JUMP | RUN_JUMP | FALL | LAND)
                }) {
                    f.set(Action::CButton, c, t);
                }
            }
        }
        let n = t - f.since;
        match (f.action, w) {
            (Action::Fire { .. }, Some(w)) if w.rounds > 0 && f.rounds == 0 => {
                // The frame after the last round: reload (V ammo_cook 242, e_w23 1031).
                f.rounds = w.rounds;
                f.set(
                    Action::Reload {
                        frames: w.empty_reload,
                    },
                    w.reload.map_or(w.idle, |r| r.0),
                    t,
                );
            }
            (
                Action::Fire {
                    mut last_z,
                    mut cycles,
                },
                Some(w),
            ) => {
                let pos = f.body.pos;
                let (facing, cycle) = (
                    f.body.facing,
                    if let Attack::Bullets { cycle, .. } = w.attack {
                        cycle
                    } else {
                        0
                    },
                );
                if w.automatic() {
                    if held(BUTTON_Z) {
                        last_z = t;
                        if cycle > 0 && n.is_multiple_of(cycle) {
                            cycles = [t, cycles[0]];
                        }
                    }
                } else {
                    cycles = [f.since, NONE];
                }
                match w.attack {
                    Attack::Bullets {
                        kind, muzzles, at, ..
                    } => {
                        for (m, o) in muzzles.iter().zip(at) {
                            if cycles.iter().any(|&c| c != NONE && t == c + o) && f.rounds > 0 {
                                f.rounds -= u8::from(w.rounds > 0);
                                f.shots = f.shots.saturating_add(1);
                                let muzzle = local_to_world(pos, facing, *m);
                                strikes.push(Strike::Bullet(Bullet::fire(
                                    world, i, muzzle, facing, kind,
                                )));
                            }
                        }
                    }
                    Attack::Melee { kind, at } => {
                        let (kind, at) = match f.anim == anim::CLUB_COMBO {
                            true => (weapon::CLUB_COMBO.1, weapon::CLUB_COMBO.2),
                            false => (kind, at),
                        };
                        if n == at {
                            strikes.push(Strike::Melee(kind));
                        }
                    }
                    Attack::Flame { kind, delay } if n >= delay => {
                        strikes.push(Strike::Flame(kind, n - delay))
                    }
                    Attack::Rocket { muzzle, cooldown } if n == 0 => {
                        f.ready = t + cooldown;
                        f.shots = f.shots.saturating_add(1);
                        let at = local_to_world(pos, facing, muzzle);
                        strikes.push(Strike::Rocket(Rocket::fire(world, i, at, facing)));
                    }
                    Attack::Shot {
                        kind,
                        muzzle,
                        anim,
                        cooldown,
                    } if n == 0 && f.anim == anim.0 => {
                        f.ready = t + cooldown;
                        f.shots = f.shots.saturating_add(1);
                        strikes.push(Strike::Aimed(kind, muzzle, projectile::SHOT_STEP));
                    }
                    Attack::Ray { kind, origin } if n == 0 => {
                        f.shots = f.shots.saturating_add(1);
                        strikes.push(Strike::Ray(kind, origin))
                    }
                    // From the hip: the cooldown runs from Z, the bolt leaves `at` frames in.
                    Attack::Bolt { cooldown, .. } if n == 0 && f.anim == w.fire.0 => {
                        f.ready = t + cooldown;
                    }
                    Attack::Bolt {
                        kind, muzzle, at, ..
                    } if n == at && f.anim == w.fire.0 => {
                        f.shots = f.shots.saturating_add(1);
                        let at = local_to_world(pos, facing, muzzle);
                        strikes.push(Strike::Bolt(kind, at));
                    }
                    Attack::Spread {
                        muzzles,
                        alt,
                        cooldown,
                    } if n == 0 => {
                        f.ready = t + cooldown;
                        f.shots = f.shots.saturating_add(1);
                        let m = muzzles[usize::from(f.anim == alt.0)];
                        strikes.push(Strike::Spread(local_to_world(pos, facing, m)));
                    }
                    _ => {}
                }
                let ray = matches!(w.attack, Attack::Ray { .. });
                // Swing 0x129 is a frame shorter than 0x12A (V e_w21 1051-1070).
                let frames = match w.attack {
                    Attack::Spread { alt, .. } if f.anim == alt.0 => u32::from(alt.1),
                    _ if f.anim == anim::CLUB_COMBO => u32::from(weapon::CLUB_COMBO.0.1),
                    _ => u32::from(w.fire.1) - u32::from(f.anim == anim::MELEE_SWING_A),
                };
                let done = if w.automatic() {
                    t >= last_z + 7
                } else {
                    n >= frames || (ray && unaimed)
                };
                if done {
                    let next = match w.aim().filter(|_| late_held(BUTTON_R)) {
                        Some(aim) => (Action::Aim { shot: NONE }, aim),
                        None => (Action::Free, w.idle),
                    };
                    f.set(next.0, next.1, t);
                } else {
                    f.action = Action::Fire { last_z, cycles };
                }
            }
            (Action::Aim { shot }, Some(w)) => {
                if shot == t {
                    f.set(
                        Action::Fire {
                            last_z: t,
                            cycles: [NONE; 2],
                        },
                        w.fire.0,
                        t,
                    );
                    if let Attack::Ray { kind, origin } = w.attack {
                        strikes.push(Strike::Ray(kind, origin));
                    }
                } else if let Attack::Bolt {
                    kind,
                    aimed,
                    cooldown,
                    ..
                } = w.attack
                {
                    // The crossbow shoots on Z's release, in that frame (V combat5_cb4: on odd and
                    // even frames alike), from the scope's eye 11 lower.
                    if late_held(BUTTON_Z) && !held(BUTTON_Z) && t >= f.ready {
                        f.set(
                            Action::Fire {
                                last_z: t,
                                cycles: [NONE; 2],
                            },
                            aimed.0,
                            t,
                        );
                        f.ready = t + cooldown;
                        f.shots = f.shots.saturating_add(1);
                        strikes.push(Strike::Aimed(kind, [-11.0, 0.0, 67.5], BOLT_STEP));
                    } else if unaimed {
                        f.set(Action::Free, w.idle, t);
                    }
                } else if shot == NONE && pressed(BUTTON_Z) {
                    f.action = Action::Aim {
                        shot: t + AIM_DELAY,
                    };
                } else if shot == NONE && unaimed {
                    f.set(Action::Free, w.idle, t);
                }
            }
            (Action::GrenadePull { .. } | Action::GrenadeHold { .. }, _)
                if n > 0 && late_held(C_DOWN) && f.cook <= 2 =>
            {
                // Cooked too long: it goes off in the hand, in the thrower's frame (V
                // combat4_dead2 881; the item shows it a frame more, as after a throw).
                (f.cook, f.grenades) = (0, f.grenades - 1);
                let pos = local_to_world(f.body.pos, f.body.facing, HOLD_HAND);
                strikes.push(Strike::Blast(pos));
                let anim = f.idle_anim();
                f.set(Action::Free, anim, t);
            }
            (Action::GrenadePull { pull }, _) if n >= PULL => {
                if late_held(C_DOWN) {
                    f.cook -= 2;
                }
                f.set(Action::GrenadeHold { pull }, anim::GRENADE_HOLD, t)
            }
            (Action::GrenadePull { .. }, _) if n > 0 && late_held(C_DOWN) => f.cook -= 2,
            (Action::GrenadeHold { .. }, _) if n > 0 && late_held(C_DOWN) => f.cook -= 2,
            (Action::GrenadeHold { pull }, _) if n > 0 && !late_held(C_DOWN) => {
                f.grenades -= 1;
                f.set(
                    Action::GrenadeThrow { pull, hold: n },
                    anim::GRENADE_THROW,
                    t,
                );
            }
            (Action::GrenadeThrow { .. }, _) if n == RELEASE => {
                strikes.push(Strike::Grenade(Grenade::release(
                    i,
                    f.body.pos,
                    f.body.facing,
                    f.cook,
                    t,
                )));
            }
            _ => {}
        }
        // Movement: Z attacks instead of crouching while a weapon is drawn (and a Z begun then, the
        // holster's last frame included, does not crouch until let go, V ctlw_k8 216), and A reloads (guns) or does nothing
        // (sniper, flamethrower) instead of jumping.
        let mut mv = *input;
        if f.drawn || f.z_armed {
            mv.buttons &= !BUTTON_Z;
        }
        if f.drawn && !w.is_some_and(Weapon::jumps) {
            mv.buttons &= !BUTTON_A;
        }
        // A swing locks onto the nearest player in its first frame (that frame's step goes along
        // the new move angle at the frame's speed; the facing stays), then ignores the stick and
        // the C buttons, so the speed brakes 2.5 a frame (V combat4_lock 800-806: 10 → 11 → 8.5 →
        // … → 0 along the lock-on angle; e_w21 841).
        // The hold lasts through the frame the swing ends in (V combat4_lock 819: a row of idle
        // 0x122 at speed 0 before the strafe resumes).
        let melee = w.is_some_and(|w| matches!(w.attack, Attack::Melee { .. }));
        let lock =
            lock_to.filter(|_| melee && matches!(f.action, Action::Fire { .. }) && f.since == t);
        // The move angle holds until the last frame, which snaps it to the facing; the facing
        // turns to it (V e_w21 842), or stays while C-left or C-right is held (V combat4_lock
        // 801-820).
        let mut keep = None;
        if melee && matches!(ended, Action::Fire { .. }) {
            let strafe = mv.buttons & (BUTTON_C_LEFT | BUTTON_C_RIGHT) != 0;
            keep = Some((strafe.then_some(f.body.facing), f.body.move_angle));
            // An A pressed in the hold is spent (V combat6_ctlw40 130: no jump after the club's swing).
            (mv.stick_x, mv.stick_y, mv.buttons) = (0, 0, mv.buttons & !(C_BUTTONS | BUTTON_A));
            f.body.prev_strafe = false;
        }
        let start = f.body.pos;
        // A draw or a holster holds the player still through the frame it ends in (obj+0x89 = 0xFF
        // from `func_15063E84`, C; V combat5_sg_e3 765-774: the stick held through the shotgun's
        // draw moves nothing, the idle shows on its last frame), and so does the shotgun's shot (its
        // case of `func_15065A5C`, C).
        let still = matches!(ended, Action::Draw | Action::Holster)
            || matches!(f.action, Action::Draw | Action::Holster)
            || (matches!(f.action, Action::Fire { .. })
                && w.is_some_and(|w| matches!(w.attack, Attack::Spread { .. })));
        if still {
            f.body.lock = 0xFF;
        }
        f.body.armed = match (f.action, w) {
            _ if f.carrying => gas::CANISTER,
            (Action::Draw, Some(w)) => w.id,
            (Action::Holster, _) => 0,
            (_, Some(w)) if f.drawn => w.id,
            _ => 0,
        };
        if burning {
            burn_run(f, world, t, &others);
        } else {
            f.bump = 0;
            movement::step_among(&mut f.body, &mv, world, f.side, &others);
        }
        if let Some((facing, angle)) = keep {
            f.body.prev_buttons |= input.buttons & BUTTON_A;
            if let Some(facing) = facing {
                f.body.facing = facing;
            }
            f.body.move_angle = if f.action == Action::Free {
                f.body.facing
            } else {
                angle
            };
        }
        if let Some(to) = lock {
            let b = &mut f.body;
            let (c0, s0) = cos_sin(b.move_angle);
            let (c1, s1) = cos_sin(to);
            let (dx, dz) = (b.pos[0] - start[0], b.pos[2] - start[2]);
            // The step's length along `to` instead of the old move angle (a rotation in xz).
            let (fwd, side) = (dx * c0 - dz * s0, dx * s0 + dz * c0);
            b.pos[0] = start[0] + fwd * c1 + side * s1;
            b.pos[2] = start[2] - fwd * s1 + side * c1;
            b.move_angle = to;
        }
        for (_, g) in &self.grenades {
            push_out(&mut f.body, g.pos, world);
        }
        if f.action == Action::Free {
            // Armed locomotion: the weapon's walk or run while the stick asks for 1 or more
            // (`conker_anim::armed_move`, C; V 0x124 on e_w21, 0x15B on e_w22), else the plain
            // gait (V for 0x14: state 1 on e_w14).
            use movement::state::{FALL, JUMP, LAND, RUN_JUMP};
            f.anim = match (w.filter(|_| f.drawn), f.body.state) {
                _ if still => f.idle_anim(),
                // The canister's run and stand (V d1: 215 from the frame after the carry, 216
                // standing; I: the jumps' plain states).
                (_, state @ (JUMP | RUN_JUMP | FALL | LAND)) if f.carrying => u16::from(state),
                (_, state) if f.carrying && state != movement::state::IDLE => anim::CARRY_MOVE,
                // A's jump shows the plain jump states (V ctlw_k1, k3, k4), melee 0x21 its own landing.
                // 0x19D is 14 frames, a frame shorter than the landing's hold (V ctlw_k1 151-165).
                (Some(w), LAND) if w.id == 0x21 && f.body.timer > 1 => anim::MELEE_LAND,
                (Some(w), LAND) if w.id == 0x21 => w.idle,
                (Some(_), state @ (JUMP | RUN_JUMP | FALL | LAND)) => u16::from(state),
                (Some(w), state) => {
                    match conker_anim::armed_move(w.id, f.body.xz_speed, f.body.target_speed) {
                        Some(a) => a,
                        None if state != movement::state::IDLE => u16::from(state),
                        None => f.idle_anim(),
                    }
                }
                _ => f.idle_anim(),
            };
            if f.bump > 0 {
                f.anim = anim::BURN_END;
            } else if burning {
                f.anim = anim::BURNING;
            }
            match w.filter(|w| f.drawn && f.anim == w.idle && fidget::fidgets(w.id)) {
                Some(w) => {
                    // A draw or reload ends after the frame's animation advance, a burst of fire
                    // before it (V e_w09 775, 1069).
                    let late = f.since == t
                        && matches!(
                            ended,
                            Action::Draw | Action::Reload { .. } | Action::CButton
                        );
                    f.anim = f.fidget.step(w.id, f.health, t, late, &mut self.rng);
                }
                None => f.fidget = Fidget::default(),
            }
        } else {
            f.fidget = Fidget::default();
        }
        for s in strikes {
            match s {
                Strike::Ray(..) | Strike::Spread(_) => deferred.push((i, s)),
                s => self.strike(i, s, world, seen, ev),
            }
        }
    }

    fn strike(
        &mut self,
        i: usize,
        s: Strike,
        world: &CollisionWorld,
        seen: Seen,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        let me = self.players[i].body;
        match s {
            Strike::Bullet(b) => {
                ev.push(Event::Shot {
                    player: i,
                    pos: b.pos,
                });
                self.bullets.push((t, b));
            }
            Strike::Grenade(g) => self.grenades.push((t, g)),
            Strike::Rocket(r) => self.rockets.push((t, r)),
            Strike::Blast(pos) => {
                self.players[i].dropped = Some(t);
                let blasts = [
                    (damage::GRENADE_INNER, damage::GRENADE_INNER_R),
                    (damage::GRENADE_OUTER, damage::GRENADE_OUTER_R),
                ];
                let first = ev.len();
                self.blast(pos, i, blasts, world, ev);
                // Hit inside the thrower's frame, its victims' first state is a frame shorter than a
                // thrown grenade's (V combat4_dead2: both respawn 87 frames on, bots3's 88; I: the
                // frame order).
                for e in &ev[first..] {
                    if let Event::Hit { victim, .. } = *e {
                        self.players[victim].since = t.saturating_sub(1);
                    }
                }
            }
            Strike::Aimed(kind, [dy, side, up], step) => {
                // `func_15063FA0`: from the scope's eye moved `side` along its yaw, toward the
                // first thing the scope's line meets, a wall or a body part (s126+0xB8; C, V:
                // probe14's birth point and aim at the wall 1,229 ahead, pitch14's at the floor
                // 606 ahead past player 2's neck, w14's at its head 134.2 ahead).
                let f = &self.players[i];
                let (dir, [sx, sz]) = aim(f);
                let eye = [me.pos[0], me.pos[1] + up, me.pos[2]];
                let wall = raycast(world, eye, dir, RAY_RANGE);
                let d = self
                    .first_part(i, eye, dir, wall, seen)
                    .map_or(wall, |h| h.1);
                let target = core::array::from_fn(|j| eye[j] + dir[j] * d);
                let from = [eye[0] + side * sx, eye[1] + dy, eye[2] + side * sz];
                let shot = Shot::toward(world, i, from, target, kind);
                self.shots.push((t, Shot { step, ..shot }));
            }
            Strike::Bolt(kind, at) => {
                // Aimed at the player it locks onto from the muzzle (`func_1506C460` with the
                // auto-aim flag 0x800BE616 set: `func_1505DADC(bolt, &angle, 0, 0xFF, 0x14)`),
                // rising by the players' height difference over their distance (C; V combat5_cb_e2:
                // 6 bolts' directions, e.g. 23381 for the facing 24066).
                let mut shot = Shot::fire(world, i, at, me.facing, kind);
                if let Some((v, a)) = self.lock_on(i, at, me.facing, 3162.0, 0x14, world, seen) {
                    let p = self.seen_pos(seen, i, v);
                    let (dx, dz) = (p[0] - me.pos[0], p[2] - me.pos[2]);
                    let k = (p[1] - me.pos[1]) / (dx * dx + dz * dz).sqrt().max(1.0);
                    let (c, s) = cos_sin(a);
                    let n = (1.0 + k * k).sqrt();
                    shot.dir = [c / n, k / n, -s / n];
                    shot.wall = raycast(world, at, shot.dir, projectile::FLIGHT_RANGE);
                }
                self.shots.push((
                    t,
                    Shot {
                        step: BOLT_STEP,
                        ..shot
                    },
                ));
            }
            Strike::Spread(muzzle) => {
                // Each sphere in turn (a victim of one is immune or stunned for the next), the
                // shooter left out; the knock-back along the facing, or away from the sphere's
                // centre with flag 0x10.
                let (c, s) = cos_sin(me.facing);
                for (j, (kind, d, r)) in weapon::SPREAD.into_iter().enumerate() {
                    let centre = [muzzle[0] + d * c, muzzle[1], muzzle[2] - d * s];
                    for v in (0..self.players.len()).filter(|&v| v != i) {
                        let p = self.seen_pos(seen, i, v);
                        if !in_reach(centre, r, p, Hitbox::of(self.players[v].side))
                            || (kind.flags & 0x2000 != 0 && !clear(world, muzzle, p))
                        {
                            continue;
                        }
                        let angle = if kind.flags & 0x10 != 0 {
                            away_from(world, centre, p)
                        } else {
                            me.facing
                        };
                        self.hurt(v, &kind, true, j > 0, angle, Some(i), ev);
                    }
                }
            }
            Strike::Flame(kind, k) => {
                // The stream's hits from the nozzle along the facing, cut at the first wall; each
                // knocks away from its own centre; the shooter is never hit (no flag 0x1000).
                let (c, s) = cos_sin(me.facing);
                let dir = [c, 0.0, -s];
                let nozzle = local_to_world(me.pos, me.facing, flame::NOZZLE);
                let wall = raycast(world, nozzle, dir, RAY_RANGE);
                for d in flame::reach(k, wall) {
                    let centre: [f32; 3] = core::array::from_fn(|j| nozzle[j] + dir[j] * d);
                    for v in (0..self.players.len()).filter(|&v| v != i) {
                        let p = self.seen_pos(seen, i, v);
                        let h = Hitbox::of(self.players[v].side);
                        if !in_reach(centre, flame::FLAME_R, p, h) {
                            continue;
                        }
                        let away = away_from(world, centre, p);
                        if self.hurt(v, &kind, true, false, away, Some(i), ev) {
                            self.players[v].burn = Some(Burn::new(i));
                        }
                    }
                }
            }
            Strike::Melee(kind) => {
                // The class table's sphere: forward 2·b_A, up 2·b_C, radius 2·b_E at scale 0.5.
                let [fwd, up, r] = kind.reach.map(|v| 2.0 * f32::from(v));
                let centre = local_to_world(me.pos, me.facing, [fwd, 0.0, up]);
                for v in (0..self.players.len()).filter(|&v| v != i) {
                    let (p, h) = (self.seen_pos(seen, i, v), Hitbox::of(self.players[v].side));
                    if self.players[v].immune == 0
                        && in_reach(centre, r, p, h)
                        && self.hurt(v, &kind, true, false, me.move_angle, Some(i), ev)
                    {
                        self.players[i].combo = t + weapon::COMBO_WINDOW;
                    }
                }
            }
            Strike::Ray(..) => unreachable!("rays are resolved after the frame's updates"),
        }
    }

    /// The strikes resolved once the frame's updates are done: the shotgun's spheres now, from its
    /// animation's event (`func_150FF840` under `func_1506AD30`, C; V combat5_sg_e2: the victim's
    /// knock-back starts the frame after), and the sniper's rays, returned for after the render.
    fn late_strikes(
        &mut self,
        late: Vec<(usize, Strike)>,
        world: &CollisionWorld,
        seen: Seen,
        ev: &mut Vec<Event>,
    ) -> Vec<(usize, DamageType, [f32; 3])> {
        let mut rays = Vec::new();
        for (i, s) in late {
            match s {
                Strike::Ray(kind, origin) => rays.push((i, kind, origin)),
                s => self.strike(i, s, world, seen, ev),
            }
        }
        rays
    }

    /// The sniper's instant ray from player `i` (`func_15081E78`).
    fn ray(
        &mut self,
        i: usize,
        kind: DamageType,
        origin: [f32; 3],
        world: &CollisionWorld,
        seen: Seen,
        ev: &mut Vec<Event>,
    ) {
        let f = &self.players[i];
        let me = f.body;
        let from = local_to_world(me.pos, me.facing, origin);
        // Along the scope's look (C; the camera agent's camera4 traces), else the facing.
        let dir = aim(f).0;
        let wall = raycast(world, from, dir, RAY_RANGE);
        if let Some((v, _, head)) = self.first_part(i, from, dir, wall, seen) {
            let kind = if head { damage::SNIPER_RAY } else { kind };
            // `func_15081E78` skips an immune or stunned living victim (C). The angle: V 0x6300 for
            // the facing 25345 on e_w24; the game's varies by up to 21 for the same aim
            // (combat4_w24 0x6302, combat4_pitch24 0x6313 and 0x6315; I: the rifle's pose).
            if self.hurt(v, &kind, false, true, me.facing & !1, Some(i), ev)
                && head
                && self.enemies(i, v)
            {
                self.players[i].headshots = self.players[i].headshots.saturating_add(1);
            }
        }
    }

    /// Player `v` as shooter `s` saw it ([`Seen`]), else where it is.
    fn seen_pos(&self, seen: Seen, s: usize, v: usize) -> [f32; 3] {
        seen(s, v).unwrap_or(self.players[v].body.pos)
    }

    /// `func_1505DADC(from, &angle, 0, exclude, cone)`: the player (but `i`) a swing (from the
    /// attacker, 200, cone 0x40) or a hip bolt (from its muzzle, 3,162, cone 0x14) locks onto, and
    /// the direction to it.
    #[allow(clippy::too_many_arguments)]
    fn lock_on(
        &self,
        i: usize,
        from: [f32; 3],
        facing: u16,
        range: f32,
        cone: i32,
        world: &CollisionWorld,
        seen: Seen,
    ) -> Option<(usize, u16)> {
        let angle = |x: f32, z: f32| match &world.acos {
            Some(a) => a.angle(x, z),
            None => atan2_u16(z, x),
        };
        let mut best: Option<(f32, u8, usize, u16)> = None;
        for (v, p) in self.players.iter().enumerate() {
            if v == i || p.health == 0 {
                continue;
            }
            let pos = self.seen_pos(seen, i, v);
            let (dx, dy, dz) = (pos[0] - from[0], from[1] - pos[1], from[2] - pos[2]);
            let (xz, d) = (dx * dx + dz * dz, dx * dx + dy * dy + dz * dz);
            let nearer = best.map_or(d < range * range, |b| d < b.0 || (p.stun == 0 && b.1 != 0));
            let to = angle(dx, dz);
            let cone = (((i32::from(to) - i32::from(facing)) >> 8) + cone) & 0xFF < 2 * cone;
            let pitch = ((i32::from(angle(xz.sqrt(), -dy)) >> 8) + 0x10) & 0xFF < 0x20;
            if !(nearer && cone && pitch) {
                continue;
            }
            let eye = [from[0], from[1] + 80.0, from[2]];
            let line = [dx, -dy, -dz];
            let len = d.sqrt();
            if len > 0.0 && raycast(world, eye, line.map(|c| c / len), len) < len {
                continue;
            }
            best = Some((d, p.stun, v, to));
        }
        best.map(|b| (b.2, b.3))
    }

    /// The nearest player's body on the segment, but `owner`'s. Dead bodies stop bullets until
    /// they lie still (I: the game's hit volume follows the pose; a body in [`anim::DEAD`] let 4
    /// of 4 traced bullets through, one 41 frames into the fall let the pistol's through and the
    /// twin gun's lower one not).
    /// The nearest player's ellipsoid ([`projectile::hits_ellipsoid`]) on the segment, but
    /// `owner`'s, and how far along it.
    fn first_ellipsoid(
        &self,
        owner: usize,
        from: [f32; 3],
        dir: [f32; 3],
        len: f32,
        seen: Seen,
    ) -> Option<(usize, f32)> {
        (0..self.players.len())
            .filter(|&v| v != owner && self.players[v].anim != anim::DEAD)
            .filter_map(|v| {
                let (at, h) = (
                    self.seen_pos(seen, owner, v),
                    Hitbox::of(self.players[v].side),
                );
                projectile::hits_ellipsoid(from, dir, len, at, h).map(|d| (v, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    fn first_body(
        &self,
        owner: usize,
        from: [f32; 3],
        dir: [f32; 3],
        len: f32,
        seen: Seen,
    ) -> Option<usize> {
        self.first_hit(owner, from, dir, len, seen).map(|h| h.0)
    }

    /// [`Self::first_body`] and how far along the segment it is hit; crouching bodies are lower.
    fn first_hit(
        &self,
        owner: usize,
        from: [f32; 3],
        dir: [f32; 3],
        len: f32,
        seen: Seen,
    ) -> Option<(usize, f32)> {
        (0..self.players.len())
            .filter(|&v| v != owner && self.players[v].anim != anim::DEAD)
            .filter_map(|v| {
                let f = &self.players[v];
                let crouched = matches!(
                    f.body.state,
                    movement::state::CROUCH | movement::state::CROUCH_WALK
                );
                let at = self.seen_pos(seen, owner, v);
                hits_body(from, dir, len, at, Hitbox::of(f.side), crouched).map(|d| (v, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// `func_15081E78` for an instant ray or an aimed shot: the first player whose head or torso
    /// ([`projectile::hits_parts`]) the segment meets, how far along, and whether it is the head
    /// (type 0x20, it kills; C) or the body (the weapon's own type; V combat4_pitch24).
    fn first_part(
        &self,
        owner: usize,
        from: [f32; 3],
        dir: [f32; 3],
        len: f32,
        seen: Seen,
    ) -> Option<(usize, f32, bool)> {
        (0..self.players.len())
            .filter(|&v| v != owner && self.players[v].anim != anim::DEAD)
            .filter_map(|v| {
                let b = &self.players[v].body;
                let crouched = matches!(
                    b.state,
                    movement::state::CROUCH | movement::state::CROUCH_WALK
                );
                let at = self.seen_pos(seen, owner, v);
                projectile::hits_parts(from, dir, len, at, crouched).map(|(d, h)| (v, d, h))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The fires lit by flames (`func_151905BC` each frame after the players): a hit every 125
    /// VIs, refused while the host is immune or stunned; the host burns while the fire lasts and
    /// [`flame::AFTERBURN`] frames more.
    fn burn(&mut self, ev: &mut Vec<Event>) {
        for v in 0..self.players.len() {
            self.burn_player(v, ev);
        }
    }

    fn burn_player(&mut self, v: usize, ev: &mut Vec<Event>) {
        let Some(mut b) = self.players[v].burn else {
            return;
        };
        let hit = b.step();
        let f = &mut self.players[v];
        (f.burn, f.heat) = ((b.life > 0).then_some(b), flame::AFTERBURN);
        if hit {
            self.hurt(v, &flame::BURN, false, true, 0, Some(b.attacker), ev);
        }
    }

    /// Rockets fly after the players (those fired this frame included): a body in this frame's
    /// reach or the wall at the end of their life sets them off.
    fn fly_rockets(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        for (born, mut r) in std::mem::take(&mut self.rockets) {
            if let Some((_, d)) = self.first_ellipsoid(r.owner, r.pos, r.dir, r.reach(), seen) {
                self.blast(r.at(d), r.owner, ROCKET_BLASTS, world, ev);
            } else if r.advance() {
                self.rockets.push((born, r));
            } else {
                self.blast(r.at(r.reach()), r.owner, ROCKET_BLASTS, world, ev);
            }
        }
    }

    /// Aimed shots and bolts first move on the first odd frame after the one they were fired in
    /// (V combat5_cb2-4, cb_e1: 15 bolts fired on odd and even frames), then every frame.
    fn fly_shots(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        let t = self.tick;
        for (born, mut s) in std::mem::take(&mut self.shots) {
            if born == t || (born + 1 == t && t.is_multiple_of(2)) {
                self.shots.push((born, s));
                continue;
            }
            let reach = s.wall.min(s.step);
            if let Some((v, _, head)) = self.first_part(s.owner, s.pos, s.dir, reach, seen) {
                let kind = if head { damage::SNIPER_RAY } else { s.kind };
                // The knock-back angle is the shot's direction (V combat4_w14: 0x6B13 for
                // (−0.871, 0, −0.491)).
                let a = dir_angle(world, s.dir);
                if self.hurt(v, &kind, false, true, a, Some(s.owner), ev) {
                    let head = head && self.enemies(s.owner, v);
                    let o = &mut self.players[s.owner];
                    o.headshots = o.headshots.saturating_add(u16::from(head));
                    // Its victim's death spin is a frame shorter than the sniper's (V combat4_w14,
                    // combat5_cb_e1: 0x17E for 75 frames, the sniper's 76; I: the frame order); a
                    // body hit's 0xD3 keeps its 30, a lethal one's too (V combat5_cb_e1 907-937,
                    // combat5_cb_e2 1053-1083).
                    if head && self.players[v].health == 0 {
                        self.players[v].since = t.saturating_sub(1);
                    }
                }
            } else if s.advance() {
                self.shots.push((born, s));
            }
        }
    }

    /// Players `a` and `v` are on different teams (obj+0x128): in Deathmatch any two players.
    fn enemies(&self, a: usize, v: usize) -> bool {
        self.players[a].team != self.players[v].team
    }

    /// `func_1505B9C4` on player `v` (guards of `func_1505D024` for direct hits, `func_1505C7D8`'s
    /// health and immunity tests for area hits); `angle` is the source's move angle, or the
    /// direction away from it for types with flag 0x10. Returns whether it took the hit.
    #[allow(clippy::too_many_arguments)]
    fn hurt(
        &mut self,
        v: usize,
        kind: &DamageType,
        area: bool,
        guarded: bool,
        angle: u16,
        attacker: Option<usize>,
        ev: &mut Vec<Event>,
    ) -> bool {
        let t = self.tick;
        let f = &mut self.players[v];
        if f.health == 0 || ((guarded || area) && f.immune != 0) || (guarded && f.stun != 0) {
            return false;
        }
        let damage = kind.amount(area);
        f.health = f.health.saturating_sub(damage);
        let reaction = if area && kind.reaction == 0x21 {
            damage::AREA_STAGGER
        } else {
            kind.reaction
        };
        let b = &mut f.body;
        if kind.aims() {
            b.move_angle = (u16::from(kind.angle) << 8).wrapping_add(angle);
        }
        (b.xz_speed, b.y_vel, b.gravity, b.airborne) =
            (kind.xz, kind.y_vel, kind.gravity, kind.y_vel > 0.0);
        if b.airborne {
            b.floor = None;
        }
        f.knock_k = kind.k;
        f.fidget = Fidget::default();
        if let Some(n) = kind.immune_frames() {
            f.immune = n;
        }
        f.stun = STUN_HIT;
        let lethal = f.health == 0;
        let first = damage::sequence(reaction, lethal)[0].0;
        f.set(
            Action::Hurt {
                reaction,
                lethal,
                step: 0,
            },
            first,
            t,
        );
        // The scorer (`func_151E5FF4`, V on Total War's friendly fire) credits hits and kills on
        // other teams only; a death on the attacker's own team, its own included, counts in +0x70,
        // but not the gas's (reaction 0x2D: V d1, d3, its deliverer's +0x70 stays). A hit with no
        // attacker (the doorways') scores nothing.
        if let Some(attacker) = attacker {
            let enemy = self.enemies(attacker, v);
            let a = &mut self.players[attacker];
            if enemy {
                a.hits = a.hits.saturating_add(1);
            }
            if lethal && enemy {
                a.kills = (a.kills + 1).min(9999);
            } else if lethal && reaction != 0x2D {
                a.suicides += 1;
            }
        }
        let f = &mut self.players[v];
        if lethal {
            (f.weapon, f.drawn) = (None, false);
            (f.carrying, f.masked) = (false, false);
            f.deaths = f.deaths.saturating_add(1);
        }
        // An unowned hit names its victim as the attacker, as a player's own grenade does.
        ev.push(Event::Hit {
            victim: v,
            attacker: attacker.unwrap_or(v),
            damage,
            lethal,
        });
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn hurt_frame(
        &mut self,
        i: usize,
        reaction: u8,
        lethal: bool,
        step: u8,
        arena: &Arena,
        world: &CollisionWorld,
        others: &[Other],
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        let seq = damage::sequence(reaction, lethal);
        let f = &mut self.players[i];
        if reaction == 0x10 && t > f.since && f.burn.is_some() {
            // A burning player's hit state makes it immune for 90 frames, counted once the stun
            // is over (`func_1504CB98` with obj+0x10B & 0x80, C; V e_w15 829 → 830, 924).
            f.immune = 90;
        }
        // Blown up, the first state lasts until the frame after the first landing (a landing sets
        // the floor, which every airborne frame takes).
        let over = if (reaction, lethal, step) == (0x2B, false, 0) {
            f.body.floor.is_some()
        } else {
            t - f.since >= u32::from(seq[usize::from(step)].1)
        };
        if over {
            let step = step + 1;
            if usize::from(step) == seq.len() {
                if lethal && f.deaths >= u16::from(self.settings.lives()) {
                    // Out of lives: it stays down, out of play, on the last place of those still in
                    // play (the game counts places down from its participants, `0x800E0AA9`).
                    let place = (0..self.players.len()).filter(|&j| self.in_play(j)).count();
                    self.players[i].out = place as u8;
                } else if lethal {
                    self.respawn(i, arena, ev);
                } else {
                    f.recover(t, reaction);
                    if f.burn.is_some() {
                        f.anim = anim::BURNING;
                    }
                }
                return;
            }
            f.set(
                Action::Hurt {
                    reaction,
                    lethal,
                    step,
                },
                seq[usize::from(step)].0,
                t,
            );
            if lethal && step == 1 {
                (f.stun, f.immune) = (STUN_DEAD, 255);
            }
        }
        knock(f, reaction, step, world, others);
    }

    fn respawn(&mut self, i: usize, arena: &Arena, ev: &mut Vec<Event>) {
        let team = self.players[i].team;
        // Bodies count where they lie: the picker reads every other team's obj+0x14 with no health
        // test (C `func_15084D70`), and a dead body's stays where it fell until its own respawn (V
        // e_w09 1096, combat4_dead2 968 with both players dead).
        let enemies: Vec<[f32; 2]> = (self.players.iter().enumerate())
            .filter(|&(j, p)| j != i && p.team != team)
            .map(|(_, p)| [p.body.pos[0], p.body.pos[2]])
            .collect();
        let f = &self.players[i];
        let at = spawn::respawn(
            &arena.spawns,
            f.spawn_mask,
            f.last_spawn,
            &enemies,
            &mut self.rng,
        );
        // The respawn draws twice more after the pick (I, one anchor: combat4_dead2's 22 draws
        // between two picks in one frame, 20 of them the first pick's; what draws them is unread).
        self.rng.next_u32();
        self.rng.next_u32();
        let (kills, suicides, deaths, shots, bot) = (f.kills, f.suicides, f.deaths, f.shots, f.bot);
        let (hits, headshots) = (f.hits, f.headshots);
        let (buttons, late, side, spawn_mask, start) =
            (f.buttons, f.late, f.side, f.spawn_mask, f.start);
        // The pad carries over: a button held through the respawn is no new press (V combat4_dead2:
        // C-down held on, no new grenade).
        self.players[i] = Fighter {
            kills,
            suicides,
            deaths,
            shots,
            hits,
            headshots,
            bot,
            buttons,
            late,
            side,
            spawn_mask,
            start,
            last_spawn: Some(at.index),
            ..Fighter::new(&at, team, self.tick)
        };
        ev.push(Event::Respawn { player: i, at });
    }

    fn fly_bullets(&mut self, seen: Seen, ev: &mut Vec<Event>) {
        for (born, mut b) in std::mem::take(&mut self.bullets) {
            // A body stops the bullet even when the hit is refused (dead, stunned or immune).
            if let Some(v) = self.first_body(b.owner, b.pos, b.dir, b.reach(), seen) {
                self.hurt(v, &b.kind, false, true, b.angle, Some(b.owner), ev);
            } else if b.advance() {
                self.bullets.push((born, b));
            }
        }
    }

    fn fly_grenades(&mut self, world: &CollisionWorld, ev: &mut Vec<Event>) {
        let t = self.tick;
        for (born, mut g) in std::mem::take(&mut self.grenades) {
            if t >= g.fuse {
                self.explode(&g, world, ev);
                continue;
            }
            if born < t {
                g.step(world);
            }
            self.grenades.push((born, g));
        }
    }

    /// `func_1505D1C4` twice at the grenade: the inner blast, then the outer one (whose victims
    /// are immune by then if the inner one hit them). Both hit the thrower too and need a clear
    /// line to the victim. (The game also draws its RNG here, for effects; the port's RNG only
    /// serves the spawn picker, whose state the game's hundreds of other draws make unreproducible.)
    fn explode(&mut self, g: &Grenade, world: &CollisionWorld, ev: &mut Vec<Event>) {
        let blasts = [
            (damage::GRENADE_INNER, damage::GRENADE_INNER_R),
            (damage::GRENADE_OUTER, damage::GRENADE_OUTER_R),
        ];
        self.blast(g.pos, g.owner, blasts, world, ev);
    }

    /// `func_151D40D4`'s two area hits at `pos` (the inner one first: its victims are immune to
    /// the outer one), owner included, each needing a clear line to the victim.
    fn blast(
        &mut self,
        pos: [f32; 3],
        owner: usize,
        blasts: [(DamageType, f32); 2],
        world: &CollisionWorld,
        ev: &mut Vec<Event>,
    ) {
        ev.push(Event::Explosion { pos, owner });
        for (kind, r) in blasts {
            for v in 0..self.players.len() {
                let p = self.players[v].body.pos;
                if !in_reach(pos, r, p, Hitbox::of(self.players[v].side)) || !clear(world, pos, p) {
                    continue;
                }
                let away = away_from(world, pos, p);
                self.hurt(v, &kind, true, false, away, Some(owner), ev);
            }
        }
    }
}

/// The rocket's blasts: types 0x16 (R 40, kills) and 0x15 (R 300, 2 damage; V combat4_w16: player
/// 2 hit 20 away in xz and killed, the shooter 123 away blown up for 2).
const ROCKET_BLASTS: [(DamageType, f32); 2] = [
    (damage::ROCKET_INNER, damage::ROCKET_INNER_R),
    (damage::ROCKET_OUTER, damage::ROCKET_OUTER_R),
];

/// The direction from `source` to `p` (knock-backs away from a blast or a flame).
fn away_from(world: &CollisionWorld, source: [f32; 3], p: [f32; 3]) -> u16 {
    match &world.acos {
        Some(a) => a.angle(p[0] - source[0], source[2] - p[2]),
        None => atan2_u16(source[2] - p[2], p[0] - source[0]),
    }
}

/// One frame of the burning run (`func_1504CB98` while s126+0x46 > 0, C; V e_w15 834-879): the
/// speed climbs 4 a frame to 28 along the move angle and the facing sways 10240·sin around it
/// (a 21⅓-frame period on the game's frame counter; the match tick here, I). A wall stops the run
/// for 11 frames (0x28E), after which it goes on along the wall (I: the game's steering of the run,
/// `func_150599C8`, and its bounce direction are not ported, nor the stick and A/B patting it out).
fn burn_run(f: &mut Fighter, world: &CollisionWorld, t: u32, others: &[Other]) {
    let side = f.side;
    let b = &mut f.body;
    if f.bump > 0 {
        f.bump -= 1;
        // The run resumes at 17.9 (`0x8009930C`, C; V e_w15 891).
        b.xz_speed = if f.bump == 0 { 17.9 } else { 0.0 };
        return;
    }
    // The drive is 28 (V e_w15 835+: target 28, the rate e/24 from 0.7 to 7/6).
    b.target_speed = 28.0;
    b.xz_speed = (b.xz_speed + 4.0).min(28.0);
    let old = b.pos;
    let (c, s) = cos_sin(b.move_angle);
    let d = 0.5 * b.xz_speed;
    let mut new = [old[0] + d * c, old[1] - 4.5, old[2] - d * s];
    movement::push_apart(&mut new, side, others);
    let swept = world.sweep(
        old,
        new,
        &Body::player(old[1], new[1]),
        &Sweep::default(),
        -9.0,
    );
    b.pos = swept.pos;
    if let Some(fl) = world.floor(b.pos[0], b.pos[2], old[1], old[1], b.floor.map(|f| f.tri)) {
        (b.pos[1], b.floor, b.floor_y) = (fl.y, Some(fl), fl.y);
    }
    let (mx, mz) = (b.pos[0] - old[0], b.pos[2] - old[2]);
    if (mx * mx + mz * mz).sqrt() < 0.5 * d {
        f.bump = 11;
        if mx != 0.0 || mz != 0.0 {
            b.move_angle = atan2_u16(-mz, mx);
        }
    }
    let phase = ((t * 12) & 0xFF) as f32 * f32::from_bits(0x3CC9_0FDB); // 2π/256, `0x8009921C`
    b.facing = b
        .move_angle
        .wrapping_add((10240.0 * conker_camera::math::sin_level(phase)) as i16 as u16);
}

/// Where a player aims (s126+0x16C and +0x170 through the scope, else its facing, level): the unit
/// direction, and the side vector of its yaw (`(sin, cos)` in x and z).
fn aim(f: &Fighter) -> ([f32; 3], [f32; 2]) {
    use conker_camera::math::{cos, sin};
    match f.body.scope_yaw {
        Some(yaw) => {
            let k = f32::from_bits(0x3C8E_FA35); // π/180
            let (pc, ps) = (cos(f.scope_pitch * k), sin(f.scope_pitch * k));
            let (yc, ys) = (cos(yaw * k), sin(yaw * k));
            ([yc * pc, -ps, -ys * pc], [ys, yc])
        }
        None => {
            let (c, s) = cos_sin(f.body.facing);
            ([c, 0.0, -s], [s, c])
        }
    }
}

/// The game's angle of a direction's xz part (`func_1505A630`).
fn dir_angle(world: &CollisionWorld, d: [f32; 3]) -> u16 {
    match &world.acos {
        Some(a) => a.angle(d[0], -d[2]),
        None => atan2_u16(-d[2], d[0]),
    }
}

/// Advances the root-motion clock with the clip the player shows this frame (conker-sim's
/// `animate`, at conker-anim's rate): the next frame's step goes by the root when the clip has one.
fn animate(f: &mut Fighter, world: &CollisionWorld) {
    let r = conker_anim::request(f.anim, f.body.xz_speed, f.body.target_speed);
    movement::animate(&mut f.body, r.id, r.rate, r.start, world);
}

/// Pushes a player standing within [`GRENADE_PUSH`] of a grenade (and under its 90-unit height)
/// straight out, back onto the floor.
fn push_out(b: &mut PlayerState, g: [f32; 3], world: &CollisionWorld) {
    let (dx, dz) = (b.pos[0] - g[0], b.pos[2] - g[2]);
    let d = (dx * dx + dz * dz).sqrt();
    if d == 0.0 || d >= GRENADE_PUSH || !(b.pos[1]..b.pos[1] + 90.0).contains(&g[1]) {
        return;
    }
    let k = GRENADE_PUSH / d;
    (b.pos[0], b.pos[2]) = (g[0] + dx * k, g[2] + dz * k);
    if !b.airborne
        && let Some(fl) = world.floor(b.pos[0], b.pos[2], b.pos[1], b.pos[1], None)
    {
        (b.pos[1], b.floor, b.floor_y) = (fl.y, Some(fl), fl.y);
    }
}

/// The clear-line test of area hits with flag 0x2000 (`func_1505C7D8` → `func_150AC9C0`, C): from
/// the source raised by half its height (the zeroed fake source object reads as 180 tall in
/// `func_1507C3E0`) to the victim raised by half of s126+0x114 (90 for players); blocked when a
/// triangle comes first.
fn clear(world: &CollisionWorld, source: [f32; 3], victim: [f32; 3]) -> bool {
    let a = [source[0], source[1] + 90.0, source[2]];
    let b = [victim[0], victim[1] + 45.0, victim[2]];
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    len == 0.0 || raycast(world, a, d.map(|c| c / len), len) >= len
}

/// One frame of a hit player's motion: blown through the air (the grenade, V e_gren 925-989), or
/// knocked back on the ground (V e_w09, e_w39, e_w21, e_w23), moving like a step of conker-sim's
/// ground movement (the plane-following step, the fall step, the wall routine, the floor).
///
/// In the air the speed drops 0.1 a frame. Each landing keeps 0.6 of the speed and bounces: the
/// first one out of the blast (step 0) with y velocity 28, later ones with half the landing
/// velocity; the frame after a landing follows the floor's plane. Once a bounce would be below 5
/// (I: 4.56 stopped, 5.875 bounced) the player stays down and slides with the hit's friction.
fn knock(f: &mut Fighter, reaction: u8, step: u8, world: &CollisionWorld, others: &[Other]) {
    let side = f.side;
    let b = &mut f.body;
    // The facing turns first: a root step goes along the turned facing (V e_w22 851-858).
    let turn = |to: u16, facing: u16| {
        let d = angle_diff(to, facing);
        if d.abs() <= 3072 {
            to
        } else {
            facing.wrapping_add((3072 * d.signum()) as u16)
        }
    };
    match damage::facing(reaction) {
        Facing::Attacker => b.facing = turn(b.move_angle.wrapping_add(0x8000), b.facing),
        Facing::Away => b.facing = turn(b.move_angle, b.facing),
        Facing::Flight => b.facing = b.move_angle,
        Facing::Keep => {}
    }
    let old = b.pos;
    let (c, s) = cos_sin(b.move_angle);
    if b.airborne {
        b.xz_speed = (b.xz_speed - 0.1).max(0.0);
        let rise = b.floor.take().map_or(0.0, |fl| {
            rise_along(world.tris[fl.tri as usize].normal, c, s)
        });
        let d = 0.5 * b.xz_speed / (1.0 + rise * rise).sqrt();
        let mut new = [
            old[0] + d * c,
            old[1] + d * rise + 0.5 * b.y_vel - 0.275 * b.gravity,
            old[2] - d * s,
        ];
        movement::push_apart(&mut new, side, others);
        b.y_vel -= b.gravity;
        let swept = world.sweep(
            old,
            new,
            &Body::player(old[1], new[1]),
            &Sweep::default(),
            b.y_vel,
        );
        (b.pos, b.y_vel) = (swept.pos, swept.y_vel);
        if let Some(fl) = world.floor(b.pos[0], b.pos[2], b.pos[1], old[1], None)
            && b.y_vel < 0.0
            && b.pos[1] <= fl.y
        {
            let bounce = if step == 0 {
                (0.5 * -b.y_vel).min(BLAST_BOUNCE)
            } else {
                0.5 * -b.y_vel
            };
            (b.pos[1], b.floor, b.floor_y, b.gravity) =
                (fl.y, Some(fl), fl.y, movement::GRAVITY_FALL);
            b.xz_speed *= 0.6;
            if bounce >= BOUNCE_MIN {
                b.y_vel = bounce;
            } else {
                (b.y_vel, b.airborne) = (0.0, false);
            }
        }
    } else {
        let (d, next) = damage::knockback(b.xz_speed, f.knock_k);
        b.xz_speed = next;
        b.gravity = movement::GRAVITY_FALL;
        // A clip with root motion (the shot death 0x17E) moves the body by its root along the facing
        // instead of by the speed (conker-sim's `root_motion`; V e_w22, e_w24).
        let rooted = movement::root_motion(b, world);
        if !rooted && d > 0.0 {
            let rise = b.floor.map_or(0.0, |fl| {
                rise_along(world.tris[fl.tri as usize].normal, c, s)
            });
            let d = d / (1.0 + rise * rise).sqrt();
            b.pos = [old[0] + d * c, old[1] + d * rise, old[2] - d * s];
        }
        // The fall step and the other bodies' push, as conker-sim's ground step (V e_w22 866+: player
        // 1 and the body push each other).
        let stepped_y = b.pos[1];
        let y_vel = b.y_vel - movement::GRAVITY_FALL;
        let mut fell = b.pos;
        fell[1] += 0.5 * y_vel;
        let shoved = movement::push_apart(&mut fell, side, others);
        if rooted || d > 0.0 || shoved {
            b.pos = fell;
            let pre = b.pos;
            b.pos = world
                .sweep(
                    old,
                    b.pos,
                    &Body::player(old[1], b.pos[1]),
                    &Sweep::default(),
                    y_vel,
                )
                .pos;
            let pushed = b.pos[0] != pre[0] || b.pos[2] != pre[2];
            let prev = if pushed {
                None
            } else {
                b.floor.map(|fl| fl.tri)
            };
            let floor = world.floor(b.pos[0], b.pos[2], stepped_y, old[1], prev);
            if let Some(fl) = floor.filter(|fl| fl.y > stepped_y - movement::SNAP_DOWN) {
                (b.pos[1], b.floor, b.floor_y) = (fl.y, Some(fl), fl.y);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> CollisionWorld {
        let big = 20_000.0;
        CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ])
    }

    fn rec(pos: [i16; 3], yaw: u8) -> Spawn {
        Spawn {
            pos,
            b6: 0,
            yaw,
            camera: [0.0; 3],
            b14: 0,
            b15: 0,
        }
    }

    /// Two players 146.5 apart, player 0 facing player 1 along +x.
    fn duel() -> (Arena, Match, CollisionWorld) {
        let arena = Arena {
            spawns: vec![rec([2000, 0, 0], 0), rec([-5000, 0, 0], 0)],
            pickups: vec![
                (weapon::PICKUPS_2D[0], [0.0, 0.0, 3000.0]),
                (weapon::PICKUPS_2D[2], [0.0, 0.0, -3000.0]),
            ],
        };
        let at = |x: f32, facing| Placement {
            index: 0,
            pos: [x, 0.0, 0.0],
            facing,
            camera: [0.0; 3],
        };
        let mut players = vec![
            Fighter::new(&at(0.0, 0), 0, 0),
            Fighter::new(&at(146.5, 0x8000), 1, 0),
        ];
        for p in &mut players {
            p.immune = 0;
        }
        let m = Match::new(&arena, players, Rng::new(1), 0);
        (arena, m, flat())
    }

    fn run(
        m: &mut Match,
        arena: &Arena,
        world: &CollisionWorld,
        frames: u32,
        buttons: impl Fn(u32) -> u16,
    ) -> Vec<(u32, Event)> {
        let mut out = Vec::new();
        for _ in 0..frames {
            let t = m.tick;
            let inputs = [
                Input {
                    buttons: buttons(t),
                    ..Input::default()
                },
                Input::default(),
            ];
            out.extend(m.step(arena, &inputs, world).into_iter().map(|e| (t, e)));
        }
        out
    }

    /// e_w09's timing: draw 15 frames, a Z tap fires the left gun 4 frames later and hits at
    /// once; the stagger lasts 18 frames and the knock-back is 15.5 + 7.8 + 3.18.
    #[test]
    fn twin_guns_tap() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x09);
        let ev = run(&mut m, &arena, &w, 60, |t| match t {
            0 => BUTTON_B,
            20..=22 => BUTTON_Z,
            _ => 0,
        });
        assert_eq!(m.players[0].anim, anim::TWIN_IDLE);
        let hits: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e.1, Event::Hit { .. }))
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 24);
        assert_eq!(m.players[1].health, 5);
        assert_eq!(m.players[0].rounds, 58);
        // Two rounds fired, one hit (the results' accuracy).
        assert_eq!((m.players[0].shots, m.players[0].hits), (2, 1));
        let x = m.players[1].body.pos[0];
        assert!((x - (146.5 + 15.5 + 7.8 + 3.18)).abs() < 1e-3, "{x}");
    }

    /// Kills, the death sequence, the respawn on the farthest record the player may take (its
    /// `spawn_mask`), the immune countdown; the side and the mask stay.
    #[test]
    fn kill_and_respawn() {
        // 5000 from the killer, not 2000: farther by more than the draw's 500; record 0 alone
        for (mask, record) in [(!0, 1), (0b01, 0)] {
            let (arena, mut m, w) = duel();
            m.give(0, 0x39);
            m.players[1].health = 2;
            m.players[1].side = Side::Squirrels;
            m.players[1].spawn_mask = mask;
            let ev = run(&mut m, &arena, &w, 200, |t| match t {
                0 => BUTTON_B,
                20 => BUTTON_Z,
                _ => 0,
            });
            let hit = ev
                .iter()
                .find(|e| matches!(e.1, Event::Hit { lethal: true, .. }))
                .unwrap();
            assert_eq!(hit.0, 20); // the pistol's bullet leaves on the press frame
            let re = ev
                .iter()
                .find(|e| matches!(e.1, Event::Respawn { .. }))
                .unwrap();
            assert_eq!(re.0, 20 + 151);
            let Event::Respawn { at, .. } = re.1 else {
                unreachable!()
            };
            assert_eq!(at.index, record);
            assert_eq!(m.players[0].kills, 1);
            let p = m.players[1];
            assert_eq!((p.health, p.grenades, p.weapon), (HEALTH, 3, None));
            assert_eq!(p.immune, SPAWN_IMMUNE - (200 - 172) as u8);
            assert_eq!((p.side, p.spawn_mask), (Side::Squirrels, mask));
        }
    }

    /// A quick grenade: pulled a frame after C-down, thrown 29 later, released 14 into the throw,
    /// explodes 120 frames after the release; the thrower is in reach of its own blast.
    #[test]
    fn grenade_quick_throw() {
        let (arena, mut m, w) = duel();
        let ev = run(
            &mut m,
            &arena,
            &w,
            200,
            |t| if t == 10 { C_DOWN } else { 0 },
        );
        assert_eq!(m.players[0].grenades, 2);
        let boom = ev
            .iter()
            .find(|e| matches!(e.1, Event::Explosion { .. }))
            .unwrap();
        // Pulled in frame 11 with C-down already let go: the full 240 VIs = 120 frames from the
        // release, 28 + 1 + 14 frames after the pull, and the blast a frame later.
        assert_eq!(boom.0, 11 + 43 + 120 + 1);
        assert!(ev.iter().any(|e| matches!(e.1, Event::Hit { .. })));
    }

    /// The sniper: R aims a frame late, Z in aim fires 3 frames later (type 0x20 kills at 146.5),
    /// and aim or shot end 8 frames after R stops (V e_w24 1161, ctlw_k10 251).
    #[test]
    fn sniper_aim_and_exit() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x24);
        let input = |t: u32| match t {
            0 => BUTTON_B,
            70..=72 => BUTTON_R | BUTTON_Z,
            60..=99 => BUTTON_R,
            _ => 0,
        };
        let ev = run(&mut m, &arena, &w, 109, input);
        let hit = ev
            .iter()
            .find(|e| matches!(e.1, Event::Hit { lethal: true, .. }))
            .unwrap();
        assert_eq!(hit.0, 73);
        assert_eq!(m.players[0].anim, anim::SNIPER_FIRE); // the shot's 39 frames would end at 112
        run(&mut m, &arena, &w, 1, input);
        assert_eq!(m.players[0].anim, anim::SNIPER_IDLE); // R last held at 99: out at 101 + 8
    }

    /// Melee: one frame into the swing the attacker turns onto the nearest enemy (move angle onto
    /// the direction, facing by as much), and the swing hits 10 frames in (V e_w21, e_w22).
    /// s126+0x78 and +0x11A as the probes show them: the twin guns from the draw's first row,
    /// attached 10 frames later; the grenade 18 frames into the pull, gone with the throw.
    #[test]
    fn camera_weapon_and_hand() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x09);
        run(&mut m, &arena, &w, 1, |_| BUTTON_B);
        let f = m.players[0];
        assert_eq!((f.camera_weapon(), f.in_hand(m.tick)), (0x09, false));
        run(&mut m, &arena, &w, 9, |_| 0);
        assert!(!m.players[0].in_hand(m.tick));
        run(&mut m, &arena, &w, 1, |_| 0);
        assert!(m.players[0].in_hand(m.tick));
        assert_eq!(m.players[0].camera_view(m.tick).weapon, 0x09);
    }

    #[test]
    fn melee_locks_on() {
        let (arena, mut m, w) = duel();
        m.players[1].body.pos = [50.0, 0.0, -40.0];
        m.give(0, 0x21);
        let ev = run(&mut m, &arena, &w, 40, |t| match t {
            0 => BUTTON_B,
            20 => BUTTON_Z,
            _ => 0,
        });
        let b = m.players[0].body;
        assert_eq!(
            (b.move_angle, b.facing),
            (atan2_u16(40.0, 50.0), atan2_u16(40.0, 50.0))
        );
        let hit = ev
            .iter()
            .find(|e| matches!(e.1, Event::Hit { .. }))
            .unwrap();
        assert_eq!((hit.0, m.players[1].health), (30, 5));
        assert_eq!((m.players[0].shots, m.players[0].hits), (1, 1));
    }

    /// `func_1505DADC`: 200 in 3D, ±90° of the facing, ±22.5° of pitch.
    #[test]
    fn lock_on_needs_range_cone_and_pitch() {
        let (_, mut m, w) = duel();
        let mut at = |p: [f32; 3]| {
            m.players[1].body.pos = p;
            let me = &m.players[0].body;
            m.lock_on(0, me.pos, me.facing, LOCK_ON, 0x40, &w, &here)
                .map(|l| l.1)
        };
        assert_eq!(at([199.0, 0.0, 0.0]), Some(0));
        assert_eq!(at([201.0, 0.0, 0.0]), None);
        assert_eq!(at([190.0, 70.0, 0.0]), None); // 202.5 in 3D
        assert_eq!(at([-150.0, 0.0, 0.0]), None); // behind
        assert_eq!(at([150.0, 100.0, 0.0]), None); // 33.7° up
        assert!(at([150.0, 50.0, 0.0]).is_some()); // 18.4° up
    }

    /// Held past its cook timer the grenade goes off in the hand 120 frames into the pull: both
    /// players in its inner blast die, the thrower's own death counts as a suicide, and the item
    /// shows the grenade one frame more (V combat4_dead2).
    #[test]
    fn grenade_cooks_off_in_the_hand() {
        let (arena, mut m, w) = duel();
        let ev = run(
            &mut m,
            &arena,
            &w,
            140,
            |t| if t >= 10 { C_DOWN } else { 0 },
        );
        let hits: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e.1, Event::Hit { lethal: true, .. }))
            .collect();
        assert_eq!((hits.len(), hits[0].0), (2, 11 + 120));
        let p = &m.players[0];
        assert_eq!((p.grenades, p.suicides, p.kills), (2, 1, 1));
        // The pull counts the shot; the own blast is no hit.
        assert_eq!((p.shots, p.hits), (1, 1));
        assert_eq!((p.item(132), p.item(133)), (0xA6, 0));
    }

    /// Instant rays and aimed shots: the head at +64.6, the body lower; a level ray at +64.6 passes
    /// over a crouching player (V combat4_w24 900).
    #[test]
    fn head_body_and_crouch() {
        let (_, mut m, _) = duel();
        let p = m.players[1].body.pos;
        let ray =
            |m: &Match, h: f32| m.first_part(0, [0.0, p[1] + h, p[2]], [1.0, 0.0, 0.0], 1e4, &here);
        let hit = |r: Option<(usize, f32, bool)>| r.map(|h| (h.0, h.2));
        assert_eq!(
            (hit(ray(&m, 64.6)), hit(ray(&m, 36.0))),
            (Some((1, true)), Some((1, false)))
        );
        m.players[1].body.state = movement::state::CROUCH;
        assert_eq!(ray(&m, 64.6), None);
    }

    /// The flamethrower: a Z tap burns for one frame 8 frames later; the victim burns, is immune
    /// for 90 frames once its 5-frame hit is over, refuses the fire's first hit (59 frames on) and
    /// takes the second (63 later), as e_w15 did.
    #[test]
    fn flame_sets_alight() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x15);
        let ev = run(&mut m, &arena, &w, 200, |t| match t {
            0 => BUTTON_B,
            40..=42 => BUTTON_Z,
            _ => 0,
        });
        let hits: Vec<_> = ev
            .iter()
            .filter_map(|e| match e.1 {
                Event::Hit { damage, .. } => Some((e.0, damage)),
                _ => None,
            })
            .collect();
        assert_eq!(hits, [(48, 1), (48 + 59 + 63, 1)]);
        assert_eq!(m.players[1].health, 4);
    }

    /// A grenade pushes a player under it straight out to 32.74 in xz (V ctl_cd, e_gren).
    #[test]
    fn grenade_pushes_players_out() {
        let w = flat();
        let mut b = PlayerState::spawn([0.0, 0.0, 0.0], 0);
        push_out(&mut b, [31.64, 55.8, 0.0], &w);
        assert!(
            (b.pos[0] + 1.1).abs() < 1e-4 && b.pos[1] == 0.0,
            "{:?}",
            b.pos
        );
        push_out(&mut b, [0.0, 95.0, 0.0], &w); // above the head
        assert!((b.pos[0] + 1.1).abs() < 1e-4);
    }

    /// The shotgun at 146.5 kills with its second sphere (0x32, the death spin); the crossbow's hip
    /// bolt (from frame 10 of the fire, 125 a frame) hits the body for 1, and Z's release in R's
    /// aim shoots at once.
    #[test]
    fn shotgun_and_crossbow() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x37);
        let ev = run(&mut m, &arena, &w, 20, |t| match t {
            0 => BUTTON_B,
            14 => BUTTON_Z,
            _ => 0,
        });
        let hit = ev.iter().find_map(|(t, e)| match e {
            Event::Hit { lethal, .. } => Some((*t, *lethal)),
            _ => None,
        });
        assert_eq!(
            (hit, m.players[1].anim),
            (Some((14, true)), anim::DEATH_SHOT)
        );
        let (arena, mut m, w) = duel();
        m.give(0, 0x18);
        let draw = u32::from(weapon::CROSSBOW.draw.1);
        let ev = run(&mut m, &arena, &w, draw + 14, |t| match t {
            0 => BUTTON_B,
            t if t == draw + 1 => BUTTON_Z,
            _ => 0,
        });
        assert!(
            ev.iter()
                .any(|(_, e)| matches!(e, Event::Hit { damage: 1, .. }))
        );
        assert_eq!(m.players[1].anim, anim::HIT_SHOT);
        run(&mut m, &arena, &w, 42, |t| {
            BUTTON_R | if t < draw + 40 { BUTTON_Z } else { 0 }
        });
        let f = &m.players[0];
        assert_eq!((f.anim, f.shots), (anim::CROSSBOW_AIMED, 2));
    }

    /// A blown-up player bounces: half the landing speed, at most 28 off the first landing, keeping
    /// 0.6 of the xz speed; below 5 it stays down (V e_gren 965-989, combat5_sg_e2 810).
    #[test]
    fn blast_bounces() {
        let w = flat();
        let (_, mut m, _) = duel();
        let f = &mut m.players[1];
        (
            f.body.pos,
            f.body.airborne,
            f.body.y_vel,
            f.body.gravity,
            f.body.xz_speed,
        ) = ([0.0, 5.0, 0.0], true, -66.0, 3.4, 16.0);
        knock(f, 0x2B, 0, &w, &[]);
        assert_eq!(
            (f.body.pos[1], f.body.y_vel, f.body.gravity),
            (0.0, BLAST_BOUNCE, movement::GRAVITY_FALL)
        );
        assert!((f.body.xz_speed - 15.9 * 0.6).abs() < 1e-5);
        (f.body.pos[1], f.body.y_vel, f.body.gravity) = (2.0, -12.2, 3.4);
        knock(f, 0x2B, 0, &w, &[]);
        assert!((f.body.y_vel - 7.8).abs() < 1e-5); // 0.5 · |−12.2 − 3.4|
        (f.body.pos[1], f.body.y_vel) = (2.0, -8.0);
        knock(f, 0x2B, 1, &w, &[]);
        assert_eq!(f.body.y_vel, 6.5); // 0.5 · |−8 − 5|
        (f.body.pos[1], f.body.y_vel) = (1.0, -3.0);
        knock(f, 0x2B, 1, &w, &[]);
        assert!(!f.body.airborne && f.body.y_vel == 0.0); // 4 < 5
    }

    /// Weapon 0x23 with R held starts nothing (V ctlw_k6); B in a reload's last frame holsters
    /// (V ctlw_k5).
    #[test]
    fn r_blocks_gun23_and_b_ends_a_reload() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x23);
        run(&mut m, &arena, &w, 40, |t| match t {
            0 => BUTTON_B,
            35 => BUTTON_R | BUTTON_Z | BUTTON_B,
            30..=39 => BUTTON_R,
            _ => 0,
        });
        assert!(m.players[0].action == Action::Free && m.players[0].drawn);
        run(&mut m, &arena, &w, 40, |t| match t {
            40 => BUTTON_A,
            71 => BUTTON_B,
            _ => 0,
        });
        assert_eq!(
            (m.players[0].action, m.players[0].since),
            (Action::Holster, 71)
        );
    }

    /// A Z begun with a weapon drawn, the holster's last frame included, does not crouch until let
    /// go (V ctlw_k8 216); a fresh one does.
    #[test]
    fn z_from_armed_does_not_crouch() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x15);
        run(&mut m, &arena, &w, 72, |t| match t {
            0 | 40 => BUTTON_B,
            65..=70 => BUTTON_Z,
            _ => 0,
        });
        assert!(!m.players[0].drawn && m.players[0].body.state == movement::state::IDLE);
        run(&mut m, &arena, &w, 3, |_| BUTTON_Z);
        assert_eq!(m.players[0].body.state, movement::state::CROUCH);
    }

    #[test]
    fn pickups_deliver_and_come_back() {
        let (arena, mut m, w) = duel();
        m.players[0].body.pos = [0.0, 0.0, 2950.0];
        m.players[1].body.pos = [0.0, 0.0, -2950.0];
        let ev = run(&mut m, &arena, &w, 3, |_| 0);
        assert_eq!(ev.len(), 2);
        assert_eq!(m.players[0].weapon, Some(0x09));
        assert_eq!(m.players[1].grenades, 6);
        m.players[0].body.pos = [0.0, 0.0, 0.0];
        run(&mut m, &arena, &w, weapon::PICKUP_RESPAWN as u32 - 3, |_| 0);
        assert_eq!(m.pickups[0].timer, 1);
        m.players[0].body.pos = [0.0, 0.0, 2950.0];
        let ev = run(&mut m, &arena, &w, 1, |_| 0);
        assert!(matches!(
            ev[0].1,
            Event::Pickup {
                player: 0,
                pickup: 0
            }
        ));
    }

    /// Lag compensation, per bullet: two pistols fire at each other in the same frame; player 0
    /// saw player 1 a step off its line and misses, player 1 saw player 0 where it stands and
    /// hits. Without the hook both hit.
    #[test]
    fn each_bullet_is_tested_where_its_shooter_saw_the_target() {
        let input = |t: u32| Input {
            buttons: match t {
                0 => BUTTON_B,
                20 => BUTTON_Z,
                _ => 0,
            },
            ..Input::default()
        };
        let hits = |seen: Seen| {
            let (arena, mut m, w) = duel();
            m.give(0, 0x39);
            m.give(1, 0x39);
            let mut hits = Vec::new();
            for _ in 0..30 {
                let inputs = [input(m.tick), input(m.tick)];
                for e in m.step_seen(&arena, &inputs, &w, seen) {
                    if let Event::Hit {
                        victim, attacker, ..
                    } = e
                    {
                        hits.push((victim, attacker));
                    }
                }
            }
            hits
        };
        assert_eq!(hits(&here), [(1, 0), (0, 1)]);
        let off = |s: usize, v: usize| (s, v) == (0, 1);
        assert_eq!(
            hits(&|s, v| off(s, v).then_some([146.5, 0.0, 90.0])),
            [(0, 1)]
        );
    }

    /// A melee swing strikes and locks onto the target where its attacker saw it.
    #[test]
    fn melee_reaches_where_the_attacker_saw_the_target() {
        let (arena, mut m, w) = duel();
        m.players[1].body.pos = [400.0, 0.0, 0.0];
        m.give(0, 0x21);
        let seen = |s: usize, v: usize| ((s, v) == (0, 1)).then_some([50.0, 0.0, -40.0]);
        let mut hit = None;
        for _ in 0..40 {
            let t = m.tick;
            let b = match t {
                0 => BUTTON_B,
                20 => BUTTON_Z,
                _ => 0,
            };
            let inputs = [
                Input {
                    buttons: b,
                    ..Input::default()
                },
                Input::default(),
            ];
            let ev = m.step_seen(&arena, &inputs, &w, &seen);
            hit = hit.or(ev
                .iter()
                .find(|e| matches!(e, Event::Hit { .. }))
                .map(|_| t));
        }
        assert_eq!((hit, m.players[1].health), (Some(30), 5));
        assert_eq!(m.players[0].body.move_angle, atan2_u16(40.0, 50.0));
    }

    /// `remove_player`: nobody changes slot; the leaver's projectiles, pickups and fires go, its
    /// slot stays free (the frame skips it) until the next player joins.
    #[test]
    fn a_leaving_player_frees_its_slot() {
        let (arena, mut m, w) = duel();
        let third = m.players[1];
        m.players.push(third);
        m.bullets = (0..3)
            .map(|o| (0, Bullet::fire(&w, o, [0.0; 3], 0, damage::TWIN_BULLET)))
            .collect();
        m.rockets = vec![
            (0, Rocket::fire(&w, 2, [0.0; 3], 0)),
            (0, Rocket::fire(&w, 1, [0.0; 3], 0)),
        ];
        m.pickups[0].taken = Some((2, 5));
        m.pickups[1].taken = Some((1, 5));
        m.players[0].burn = Some(Burn::new(1));
        m.players[2].burn = Some(Burn::new(2));
        m.remove_player(1);
        let owners = |v: &[(u32, Bullet)]| v.iter().map(|b| b.1.owner).collect::<Vec<_>>();
        assert_eq!(owners(&m.bullets), [0, 2]);
        assert_eq!(m.rockets.iter().map(|r| r.1.owner).collect::<Vec<_>>(), [2]);
        assert_eq!(
            (m.pickups[0].taken, m.pickups[1].taken),
            (Some((2, 5)), None)
        );
        assert_eq!(m.players[0].burn, None);
        assert_eq!(m.players[2].burn.map(|b| b.attacker), Some(2));
        m.step(&arena, &[], &w);
        assert_eq!(m.players[1], Fighter::vacant(0), "not stepped");
        assert_eq!((m.next_slot(), m.join(third), m.next_slot()), (1, 1, 3));
        assert_eq!(m.join(third), 3);
    }

    /// A free slot changes no respawn pick: with player 1 gone, player 0 respawns on the records
    /// (and with the RNG draws) it would have if player 1 never was, wherever player 2 stands.
    #[test]
    fn a_free_slot_changes_no_respawn_pick() {
        let arena = Arena {
            spawns: (-4..=4).map(|k| rec([k * 1500, 0, 0], 0)).collect(),
            pickups: Vec::new(),
        };
        let at = |x: f32, team| {
            let place = Placement {
                index: 0,
                pos: [x, 0.0, 0.0],
                facing: 0,
                camera: [0.0; 3],
            };
            Fighter::new(&place, team, 0)
        };
        for x in [-5000.0, -800.0, 0.0, 2500.0, 6000.0] {
            let three = vec![at(0.0, 0), at(x, 1), at(x, 2)];
            let mut left = Match::new(&arena, three, Rng::new(7), 0);
            left.remove_player(1);
            let mut never = Match::new(&arena, vec![at(0.0, 0), at(x, 2)], Rng::new(7), 0);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for _ in 0..4 {
                left.respawn(0, &arena, &mut a);
                never.respawn(0, &arena, &mut b);
            }
            assert_eq!((a, left.rng), (b, never.rng), "player 2 at {x}");
        }
    }

    /// `step_player` predicts a player's own frames exactly as the match runs them: the knock-back
    /// of a hit already taken, the death, the fall and the respawn (V by construction; the client's
    /// prediction).
    #[test]
    fn one_player_frames_match_the_whole_match() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x39);
        m.players[1].health = 4;
        let input = |t: u32| Input {
            buttons: match t {
                0 => BUTTON_B,
                20 | 40 => BUTTON_Z,
                _ => 0,
            },
            ..Input::default()
        };
        let mut compared = 0;
        for _ in 0..260 {
            let t = m.tick;
            let inputs = [input(t), Input::default()];
            let mut solo = m.clone();
            let events = m.step(&arena, &inputs, &w);
            solo.step_player(1, &arena, &inputs[1], &w, &here);
            // The frames the match decides for player 1: hits, and the respawn's pick (the RNG).
            let decided = events.iter().any(|e| {
                matches!(
                    e,
                    Event::Hit { victim: 1, .. } | Event::Respawn { player: 1, .. }
                )
            });
            if !decided {
                assert_eq!(solo.players[1], m.players[1], "frame {t}");
                compared += 1;
            }
        }
        assert!(m.players[1].last_spawn.is_some() && compared > 250);
    }

    /// A client predicts its own grenade with `step_player` (only its own grenades in its match):
    /// the throw, the release's push, the flight and the blast it takes, frame for frame.
    #[test]
    fn own_grenades_are_predicted() {
        let (arena, mut m, w) = duel();
        m.players[1].body.facing = 0x4000;
        let mut compared = 0;
        let mut blasted = false;
        for _ in 0..220 {
            let t = m.tick;
            let throw = Input {
                buttons: if t == 10 { C_DOWN } else { 0 },
                // Then it walks back toward where it lands (-z), into the blast.
                stick_y: if (60..90).contains(&t) { 80 } else { 0 },
                ..Input::default()
            };
            let inputs = [Input::default(), throw];
            let mut solo = m.clone();
            solo.grenades.retain(|g| g.1.owner == 1);
            let events = m.step(&arena, &inputs, &w);
            let predicted = solo.step_player(1, &arena, &inputs[1], &w, &here);
            assert_eq!(solo.players[1], m.players[1], "frame {t}");
            blasted |= predicted
                .iter()
                .any(|e| matches!(e, Event::Hit { victim: 1, .. }));
            assert_eq!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::Hit { victim: 1, .. })),
                predicted
                    .iter()
                    .any(|e| matches!(e, Event::Hit { victim: 1, .. }))
            );
            compared += 1;
        }
        assert!(blasted && compared == 220);
    }

    /// Another player's grenades too, once the client has them (each `Fighting` carries them all):
    /// player 0's throw, the flight and the blast player 1 takes, frame for frame.
    #[test]
    fn others_grenades_are_predicted() {
        let (arena, mut m, w) = duel();
        let mut blasted = false;
        for _ in 0..220 {
            let t = m.tick;
            let throw = Input {
                buttons: if t == 10 { C_DOWN } else { 0 },
                ..Input::default()
            };
            let inputs = [throw, Input::default()];
            let mut solo = m.clone();
            let events = m.step(&arena, &inputs, &w);
            let predicted = solo.step_player(1, &arena, &inputs[1], &w, &here);
            assert_eq!(solo.players[1], m.players[1], "frame {t}");
            let hit = |ev: &[Event]| (ev.iter()).any(|e| matches!(e, Event::Hit { victim: 1, .. }));
            assert_eq!(hit(&events), hit(&predicted), "frame {t}");
            blasted |= hit(&predicted);
        }
        assert!(blasted);
    }

    /// Players without an input this frame still run (their immunity counts down).
    #[test]
    fn missing_inputs_are_neutral() {
        let (arena, mut m, w) = duel();
        m.players[1].immune = 5;
        m.step(&arena, &[], &w);
        assert_eq!((m.tick, m.players[1].immune), (1, 4));
    }

    #[test]
    fn clock_and_end() {
        let (arena, mut m, w) = duel();
        m.timer = 4;
        run(&mut m, &arena, &w, 2, |_| 0);
        assert_eq!((m.timer, m.ended_at), (0, Some(1)));
        run(&mut m, &arena, &w, END_DELAY, |_| 0);
        assert_eq!(m.phase(), Phase::Fading);
        run(&mut m, &arena, &w, FADE, |_| 0);
        assert_eq!(m.phase(), Phase::Over);
        let t = m.tick;
        run(&mut m, &arena, &w, 5, |_| 0);
        assert_eq!(m.tick, t);
    }

    /// A duel where player 0's pistol kills player 1 (health 2) with its bullet in frame 20; its
    /// death sequence ends in frame 171 (as `kill_and_respawn`).
    fn pistol_kill(settings: Settings) -> (Arena, Match, CollisionWorld, Vec<(u32, Event)>) {
        let (arena, m, w) = duel();
        let mut m = m.with(settings);
        m.give(0, 0x39);
        m.players[1].health = 2;
        let ev = run(&mut m, &arena, &w, 200, |t| match t {
            0 => BUTTON_B,
            20 => BUTTON_Z,
            _ => 0,
        });
        (arena, m, w, ev)
    }

    #[test]
    fn an_untimed_match_has_no_clock() {
        let untimed = Settings {
            time: None,
            ..Settings::default()
        };
        let (_, m, _, ev) = pistol_kill(untimed);
        assert_eq!((m.timer, m.ended_at), (0, None));
        assert!(ev.iter().any(|e| matches!(e.1, Event::Respawn { .. })));
        let timed = Settings {
            time: Some(3),
            ..Settings::default()
        };
        assert_eq!(pistol_kill(timed).1.timer, 3 * 3600 - 2 * 200);
    }

    /// The kill limit ends the match on the killing frame; the fade follows as after a time-out.
    #[test]
    fn the_kill_limit_ends_the_match() {
        let limit = |kills| Settings {
            kills,
            ..Settings::default()
        };
        assert_eq!(pistol_kill(limit(1)).1.ended_at, Some(20));
        assert_eq!(pistol_kill(limit(2)).1.ended_at, None);
        let (arena, mut m, w, _) = pistol_kill(limit(1));
        m.tick = 20 + END_DELAY + FADE;
        assert_eq!(m.phase(), Phase::Fading);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.phase(), Phase::Over);
    }

    /// Untimed, the last life lost puts the player out (no respawn) on the last place; the last
    /// one standing ends the match. Timed, lives are 99.
    #[test]
    fn the_last_life_puts_a_player_out() {
        let lives = |lives| Settings {
            time: None,
            lives,
            ..Settings::default()
        };
        let (_, m, _, ev) = pistol_kill(lives(1));
        assert!(!ev.iter().any(|e| matches!(e.1, Event::Respawn { .. })));
        assert_eq!((m.players[1].out, m.players[1].deaths), (2, 1));
        assert_eq!((m.ended_at, m.places()), (Some(171), vec![1, 2]));
        let (_, m, _, ev) = pistol_kill(lives(2));
        assert!(ev.iter().any(|e| matches!(e.1, Event::Respawn { .. })));
        assert_eq!(
            (m.players[1].out, m.players[1].deaths, m.ended_at),
            (0, 1, None)
        );
        assert_eq!(Settings::default().lives(), TIMED_LIVES);
    }

    /// The last human out of lives ends the match, bots still in play.
    #[test]
    fn no_human_left_ends_the_match() {
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            time: None,
            lives: 1,
            ..Settings::default()
        });
        let far = Placement {
            index: 1,
            pos: [-5000.0, 0.0, 0.0],
            facing: 0,
            camera: [0.0; 3],
        };
        m.players.push(Fighter::new(&far, 2, 0));
        for i in [0, 2] {
            m.players[i].bot = true;
        }
        m.give(0, 0x39);
        m.players[1].health = 2;
        run(&mut m, &arena, &w, 200, |t| match t {
            0 => BUTTON_B,
            20 => BUTTON_Z,
            _ => 0,
        });
        assert_eq!((m.players[1].out, m.ended_at), (3, Some(171)));
    }

    /// The original's ranking: kills − deaths, the kill limit's holders first once reached, fewer
    /// shots on a tie, ties shared; players out keep their place; free slots none.
    #[test]
    fn places_rank_kills_minus_deaths_then_shots() {
        let s = |kills, deaths, shots, out| {
            Some(Standing {
                kills,
                deaths,
                shots,
                out,
                ..Standing::default()
            })
        };
        let all = [
            s(2, 1, 9, 0),
            s(1, 0, 4, 0),
            s(1, 0, 2, 0),
            None,
            s(3, 0, 0, 0),
            s(0, 3, 0, 6),
            s(1, 0, 4, 0),
        ];
        assert_eq!(places(&all, 0), [5, 3, 2, 0, 1, 6, 3]);
        // Kill limit 2 reached: player 0 (2 − 1) ranks with the 3-kill player, above the rest.
        assert_eq!(places(&all, 2), [2, 4, 3, 0, 1, 6, 4]);
        // A 0-0 start: everyone shares 1st until someone fires (V: lab/hud kill, 2 shots → 2nd).
        assert_eq!(places(&[s(0, 0, 0, 0), s(0, 0, 0, 0)], 0), [1, 1]);
        assert_eq!(places(&[s(0, 0, 2, 0), s(0, 0, 0, 0)], 0), [2, 1]);
    }

    /// Friendly fire (V Total War's `ff_*`): full damage and the victim's death, but no hit, kill
    /// or headshot for the attacker; the team kill counts in its +0x70 (`suicides`). In Deathmatch
    /// every player is its own team: the same shot scores.
    #[test]
    fn a_team_kill_scores_no_hit_kill_or_headshot() {
        for team in [1, 0] {
            let (_, mut m, w) = duel();
            m.players[1].team = team;
            let mut ev = Vec::new();
            m.ray(0, damage::SNIPER_BODY, [0.0, 0.0, 64.6], &w, &here, &mut ev);
            let (a, v) = (&m.players[0], &m.players[1]);
            assert_eq!((v.health, v.deaths), (0, 1));
            let n = u16::from(team == 1);
            assert_eq!((a.hits, a.headshots, a.kills, a.suicides), (n, n, n, 1 - n));
        }
    }

    /// Total War's places (`func_151E565C`'s team branch): the teams' kills, then fewer shots,
    /// both equal a DRAW; a team with every member out is behind, the first out of two 2nd.
    #[test]
    fn teams_rank_by_kills_then_fewer_shots() {
        let s = |team, kills, shots, out| {
            Some(Standing {
                team,
                kills,
                deaths: 9,
                shots,
                out,
            })
        };
        let all = [s(0, 1, 9, 0), s(1, 2, 0, 0), s(0, 2, 9, 0), None];
        assert_eq!((team_places(&all), team_result(&all)), ([1, 2], Some(0)));
        // Equal kills: fewer shots, bots' included (V `to_shots`).
        assert_eq!(team_places(&[s(0, 1, 3, 0), s(1, 1, 2, 0)]), [2, 1]);
        // Equal both: a DRAW (V `to_draw`).
        let draw = [s(0, 0, 0, 0), s(1, 0, 0, 0)];
        assert_eq!((team_places(&draw), team_result(&draw)), ([1, 1], None));
        // Out, whatever its kills, until its last member is; one member in keeps it in.
        let out = [s(0, 5, 0, 3), s(0, 0, 0, 2), s(1, 0, 9, 0)];
        assert_eq!((team_places(&out), team_result(&out)), ([2, 1], Some(1)));
        assert_eq!(
            team_places(&[s(0, 5, 0, 3), s(0, 0, 0, 0), s(1, 0, 9, 0)]),
            [1, 2]
        );
        // Both out: the first out (its last member on the higher place) is 2nd; an empty team is out.
        assert_eq!(team_places(&[s(0, 0, 0, 2), s(1, 0, 0, 1)]), [2, 1]);
        assert_eq!(team_places(&[s(1, 0, 0, 0)]), [2, 1]);
    }

    /// Total War ends by team: timed, the clock only (the kill limit is not read); untimed, once a
    /// team's last member is out, not while a team-mate plays on (V `elim`, `elim_tk`).
    #[test]
    fn total_war_ends_by_team() {
        let tw = |time, lives| Settings {
            time,
            lives,
            kills: 1,
            total_war: true,
            ..Settings::default()
        };
        let (_, m, _, _) = pistol_kill(tw(Some(3), 3));
        assert_eq!((m.players[0].kills, m.ended_at), (1, None));
        let (_, m, _, _) = pistol_kill(tw(None, 1));
        assert_eq!((m.ended_at, m.places()), (Some(171), vec![1, 2]));
        let (arena, m, w) = duel();
        let mut m = m.with(tw(None, 1));
        let far = Placement {
            index: 1,
            pos: [-5000.0, 0.0, 0.0],
            facing: 0,
            camera: [0.0; 3],
        };
        m.players.push(Fighter::new(&far, 1, 0));
        m.give(0, 0x39);
        m.players[1].health = 2;
        run(&mut m, &arena, &w, 200, |t| match t {
            0 => BUTTON_B,
            20 => BUTTON_Z,
            _ => 0,
        });
        assert_eq!((m.players[1].out, m.ended_at), (3, None));
        assert_eq!(m.places(), [1, 2, 2], "team 0 has the kill");
    }

    /// Total War's gas on the match: the carrier's speed and lock-out, the masks' gate, the
    /// canister's team gate, and the script's hits' scoring (V d1, d2, d3).
    #[test]
    fn gas_canister_masks_and_hits() {
        let (_, mut m, w) = duel();
        // Player 0 stands on mask k19, player 1 on canister 0 (only team 0 may touch it).
        let arena = Arena {
            spawns: vec![rec([2000, 0, 0], 0)],
            pickups: vec![(weapon::PICKUPS_2D[19], [0.0, 0.0, 0.0])],
        };
        m.pickups = vec![PickupState::default()];
        let (canister, at) = gas::CANISTERS[0];
        m.players[1].body.pos = at;
        m.apply(&[
            Effect::Show(canister),
            Effect::Set {
                placement: canister,
                flag: None,
                gate: Some(1),
            },
        ]);
        let idle = [Input::default(); 2];
        for _ in 0..5 {
            m.step(&arena, &idle, &w);
        }
        assert_eq!((m.players[0].masked, m.gas.toucher(0)), (false, None));
        m.players[1].team = 0;
        m.apply(&[Effect::Countdown(300)]);
        for _ in 0..3 {
            m.step(&arena, &idle, &w);
        }
        assert!(m.players[0].masked && m.players[0].item(m.tick) == 0xA0);
        assert_eq!(m.gas.toucher(0), Some(1));
        assert_eq!(m.gas.shown() & 1 << canister, 0);
        m.apply(&[Effect::Release(0)]);
        assert!(!m.players[0].masked);

        // The carrier: holstered, no draw, no crouch, 80 % of the run's 35 (d1: +1 a frame to within
        // 1 of 28, then +0.01; 35 without the canister).
        m.give(1, 0x39);
        m.apply(&[Effect::Carry(1, true)]);
        let run = Input {
            stick_y: 82,
            buttons: BUTTON_B | BUTTON_Z,
            ..Input::default()
        };
        let mut speeds = Vec::new();
        for _ in 0..40 {
            m.step(&arena, &[Input::default(), run], &w);
            speeds.push(m.players[1].body.xz_speed);
        }
        let f = m.players[1];
        assert_eq!(
            (f.action, f.drawn, f.item(m.tick)),
            (Action::Free, false, 0xA5)
        );
        assert_eq!(
            (f.body.state, f.anim),
            (movement::state::RUN, anim::CARRY_MOVE)
        );
        let top = speeds.iter().copied().fold(0.0, f32::max);
        assert!(top > 27.0 && top <= 28.0, "{speeds:?}");
        assert!((speeds[39] - speeds[38] - 0.01).abs() < 1e-4, "{speeds:?}");
        m.apply(&[Effect::Carry(1, false)]);
        let run = Input { buttons: 0, ..run };
        for _ in 0..12 {
            m.step(&arena, &[Input::default(), run], &w);
        }
        assert!(m.players[1].body.xz_speed > 34.0);

        // The gas kills whoever it reaches, credits its deliverer (dead or alive) with the enemies
        // only and costs nobody +0x70; the doorway's 1 a frame credits nobody.
        let far = Placement {
            index: 0,
            pos: [-3000.0, 0.0, 0.0],
            facing: 0,
            camera: [0.0; 3],
        };
        m.players.push(Fighter::new(&far, 1, m.tick));
        m.players[0].health = 0;
        let hit = |slot, kind, attacker| Effect::Hit {
            slot,
            kind,
            attacker,
        };
        let ev = m.apply(&[
            hit(2, damage::DOORWAY, None),
            hit(1, damage::GAS, Some(0)),
            hit(2, damage::GAS, Some(0)),
        ]);
        assert_eq!(ev.len(), 3);
        assert_eq!(m.players[2].anim, anim::GASSED);
        assert!(
            m.players[1..]
                .iter()
                .all(|f| f.health == 0 && f.deaths == 1)
        );
        let d = m.players[0];
        assert_eq!((d.kills, d.hits, d.suicides), (1, 1, 0));
        assert_eq!((m.players[2].hits, m.players[2].suicides), (0, 0));
    }
}
