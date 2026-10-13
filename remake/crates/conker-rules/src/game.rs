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
use crate::beach::{self, Plunger};
use crate::characters;
use crate::colors::{self, Colors, Pose};
use crate::damage::{self, DamageType, Facing, Hitbox, in_reach};
use crate::fidget::{self, Fidget};
use crate::flame::{self, Burn};
use crate::gas::{self, Effect, Gas};
use crate::gun::{self, Gun, Mount};
use crate::heist::{self, Heist};
use crate::projectile::{
    self, BOLT_STEP, Bullet, Grenade, Rocket, Shot, hits_body, local_to_world, raycast, rise_along,
};
use crate::race::{self, Race};
use crate::raptor::{self, Raptor};
use crate::spawn::{self, Placement, Spawn};
use crate::tank::{self, Tank};
use crate::traps::{self, Traps};
use crate::weapon::{self, Attack, Gives, Pickup, Weapon};
use std::collections::BTreeSet;

pub const BUTTON_B: u16 = 0x4000;
pub const BUTTON_R: u16 = 0x0010;
pub const C_DOWN: u16 = 0x0004;
pub const C_BUTTONS: u16 = 0x000F;

pub const HEALTH: u8 = 6;
/// Immune frames after a (re)spawn (obj+0x125).
pub const SPAWN_IMMUNE: u8 = 150;
/// Total War's takeover ([`Effect::TakeOver`]): it comes this many frames after the out frame,
/// once the out view's skull byte has grown 4 a frame past 0x32 (V t1: out 424, swap 437), and
/// leaves the human this many immune frames (obj+0x125 = 90, C; V t1 438: 89).
pub const TAKEOVER_DELAY: u32 = 13;
pub const TAKEOVER_IMMUNE: u8 = 90;
/// The match clock in VIs (10 minutes, the default; `0x8008FD78`), −2 a frame.
pub const MATCH_TIME: i32 = 36_000;
/// A timed match's lives (`D_80087260`): only an untimed one counts them.
pub const TIMED_LIVES: u8 = 99;
/// obj+0x104 during a hit state, and while dead from the death's second state.
pub const STUN_HIT: u8 = 0xFE;
pub const STUN_DEAD: u8 = 0xFF;
/// A bullet that meets a body this high above the feet or higher hits the head, part 0xA (I:
/// fitted to oracle ohit, 0x30: 12 head hits at +47.3 to +60.6, the living body's at +47.2 or less).
pub const BULLET_HEAD: f32 = 47.25;
/// Frames from the end's latch ([`Match::ended_at`]) to the end fade (`func_151E53E8`, phase 7), and
/// of the fade (V `menus-and-flow.md`: latch 119, phase 7 at 158; Colors e0 383/422/438).
pub const END_DELAY: u32 = 39;
pub const FADE: u32 = 16;
/// The scare's reach, squared (`0x80099530`, V RAM), and how long it lasts (s126+0x4A).
const SCARE_D2: f32 = 30000.0;
const SCARE_FRAMES: u8 = 20;

/// A bolt's push body (obj+0xE4 5, +0xE8 0, +0xEC 1; V e4 cbp2): radius, height, centre.
const BOLT_BODY: [i16; 3] = [5, 5, 0];

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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arena {
    pub spawns: Vec<Spawn>,
    pub pickups: Vec<(Pickup, [f32; 3])>,
    /// The pickups that move (0x1A's first four; none elsewhere).
    pub roaming: weapon::Roaming,
    /// Frames a taken pickup is away, if not [`weapon::PICKUP_RESPAWN`] (0x1A's
    /// [`weapon::VAULT_RESPAWN`]).
    pub respawn: Option<u16>,
    /// Race: the route's lap ([`race::Track`]; empty elsewhere).
    pub track: race::Track,
}

impl Arena {
    /// Scene 0x2D Deathmatch (no gas masks); `placement(n)` is placement n's position
    /// (extract-scene).
    pub fn deathmatch(spawns: Vec<Spawn>, placement: impl Fn(u16) -> [f32; 3]) -> Self {
        let pickups = (weapon::PICKUPS_2D.iter())
            .filter(|p| p.gives != Gives::Mask)
            .map(|p| (*p, placement(p.placement)))
            .collect();
        Self {
            spawns,
            pickups,
            ..Default::default()
        }
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
    /// War: Colors (scenario 10, scene 0x30; [`crate::colors`]): capture the flag. Teams are the
    /// sides, as in Total War; [`colors::LIVES`] lives never lost; the match ends on the clock or
    /// at [`Self::score`] captures, and the teams rank by captures first ([`team_places`]).
    pub colors: bool,
    /// Colors' score target (opt[18]): 1-40, 0 unlimited (default [`colors::SCORE`]); Beach's too.
    pub score: u8,
    /// War: Beach (scenario 8, scene 0x2B; [`crate::beach`]): the villagers (team 0, the squirrels)
    /// escape, the Tediz (team 1) shoot. Teams are the sides; [`colors::LIVES`] lives never lost;
    /// the team scores ([`Colors::scores`]) go up for an escape, the detonation and each villager's
    /// death, and the match ends on the clock or at [`Self::score`]; teams rank as Colors' do.
    pub beach: bool,
    /// War: Heist (scenario 0, scene 0x1A; [`crate::heist`]): four teams (REDS, BLUES, GREENS,
    /// YELLOWS) fetch the bag home. Lives as Deathmatch's; the match ends on the clock, at
    /// [`Self::score`] bags (1-5, default [`heist::SCORE`]) or, untimed, once fewer than 2 teams
    /// are in play; teams rank by bags, kills, fewer shots ([`teams_places`]).
    pub heist: bool,
    /// Tank (scenario 1, scene 0x24; [`crate::tank`]): every player a tank, free for all; ends and
    /// ranks as Deathmatch (the clock, [`Self::kills`], lives untimed).
    pub tank: bool,
    /// War: Raptor (scenario 2, scene 0x3F; [`crate::raptor`]): the cavemen (team 0, Side::Cavemen)
    /// against the raptors (team 1, class [`raptor::CLASS`]). The team scores ([`Colors::scores`])
    /// go up for a kill, a fed body and a delivered egg; the match ends on the clock, at
    /// [`Self::score`] (1-40, default [`raptor::SCORE`]) or as Heist's untimed; lives as
    /// Deathmatch's; teams rank as Heist's.
    pub raptor: bool,
    /// Race (scenarios 11 and 12, scene 0x33; [`crate::race`]): up to 8 racers on hoverboards,
    /// [`Self::laps`] laps (opt[7], 1-20) of the route ([`Self::high`]: scenario 12's elevated
    /// one), untimed with the lives; the end and places are the race's.
    pub race: bool,
    pub laps: u8,
    pub high: bool,
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
            colors: false,
            score: colors::SCORE,
            beach: false,
            heist: false,
            tank: false,
            raptor: false,
            race: false,
            laps: race::LAPS,
            high: false,
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
        if self.colors || self.beach {
            colors::LIVES
        } else if self.time.is_some() {
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
    /// Total War's gas mask going on (`on`: state 81, clip 0x151) or coming off (state 78, 0x14E)
    /// for [`gas::MASK_FRAMES`] frames: the player's stick and buttons do nothing from the second.
    Mask {
        on: bool,
    },
    /// War: Colors' flag and capture poses ([`colors::Pose`]): the stick and buttons do nothing
    /// while they last.
    Pose(Pose),
}

/// A player: its movement state and everything combat adds (named after the game's fields).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Fighter {
    pub body: PlayerState,
    /// obj+0x128: Deathmatch gives every player its own team.
    pub team: u8,
    /// The side: the team and base the server keys (conker-sim's `Side`), the Tediz unless a match
    /// puts the player with the squirrels or the cavemen. The body is the class's ([`Self::class`]).
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
    /// The frame it went out (its last life's respawn moment): Total War's takeover waits 13
    /// frames from it ([`Effect::TakeOver`]).
    pub out_at: u32,
    /// Lives over the match's ([`Settings::lives`]): a takeover's, the bot's left.
    pub extra: u16,
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
    /// Total War: the frame the AI script's `obj(0xE013, p)` started this player's gassed screen
    /// (`gas::Effect::Haze`), which runs its course whatever the player does (kept through a
    /// respawn).
    pub gassed: Option<u32>,
    /// War: Colors: the flag carried (its index in [`colors::FLAGS`]; obj+0x13C = the flag + 100):
    /// no weapon drawn or used, A and Z do nothing, B puts it down, 0.8 of the speed
    /// (`PlayerState::holding`), until the script makes it let go ([`colors::Flag`]).
    pub flag: Option<u8>,
    /// War: Heist: the bags this player delivered (the STATS' column).
    pub bags: u16,
    /// s126+0x170: the scope's pitch in degrees (`conker_camera::Scope::pitch`, positive looks
    /// down). Set with `body.scope_yaw` after each camera step; the sniper's ray and weapon 0x14's
    /// shot look along the scope's yaw and pitch while `body.scope_yaw` is set, else along the
    /// facing.
    pub scope_pitch: f32,
    /// On a mounted gun ([`gun`]): jumping onto it, then seated, until A, its death or a hit that
    /// throws it off.
    pub gun: Option<Mount>,
    /// The record's class (+0; [`characters`]: its model, scale and bodies): the scenario's (the Tedi's
    /// 3 by default, the squirrel's 4, the caveman's 0xB, the Wise Guy's 1 on 0x1A, the tank's 8, the
    /// raptor's 9, Race's 0xB-0xF) or a human's pick's; War: Beach's villager looks re-rolled at each
    /// respawn ([`beach::model`]). And s126+0x12C, a bottle's boost in VIs ([`beach::drive`]; Tank's
    /// power-up timer in VIs, or 0x10's shells left).
    pub class: u8,
    pub boost: u16,
    /// The tommy gun's climb while firing (s126+0x1A2/0x1A4, [`weapon::Climb`]).
    pub climb: weapon::Climb,
    /// Tank: the power-up held (s126+0x128's bit, [`crate::tank::SHIELD`]..; 0 none), its timer
    /// or shells in [`Self::boost`].
    pub power: u8,
    /// Tank: a bot's aim error s126+0x12A (0 or 1, the AI's draw): its gun's pitch solves for the
    /// target 400 nearer (`func_150ED298`'s last argument, C), the turret 10° off (the bot's input).
    /// The server's alone: not on the wire (a client predicts only its human, always 0).
    #[cfg_attr(feature = "net", serde(skip))]
    pub aim_error: u8,
    /// War: Raptor: a body (character 22) in the raptor's jaws (+0x2E4[0]), from its standing
    /// bite's kill to the drop or the nest; kept through hits and its death, gone at its respawn.
    pub prey: bool,
    /// Race: the racer's board and race-table entry ([`race::Ride`]).
    pub ride: race::Ride,
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
            out_at: 0,
            extra: 0,
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
            gassed: None,
            flag: None,
            bags: 0,
            scope_pitch: 0.0,
            gun: None,
            class: beach::TEDI_CLASS,
            boost: 0,
            climb: weapon::Climb::default(),
            power: 0,
            aim_error: 0,
            prey: false,
            ride: race::Ride::default(),
        }
    }

    /// The body the area tests and the bullets' sweep take ([`Hitbox`]): its class's.
    pub fn hitbox(&self) -> Hitbox {
        characters::body(self.class).hitbox
    }

    /// The body it pushes the others with (conker-sim's `Other`): its class's.
    pub fn other(&self) -> Other {
        characters::body(self.class).at(self.body.pos)
    }

    /// This slot's player in `body`'s object (`func_15062800` swaps the two objects and puts each
    /// slot's ids back, C): the body and everything it holds and does from `body`; who plays the
    /// slot, its record and its pad from `self` (the shots too: s126+0x1AA is put back).
    fn in_body(&self, body: &Fighter) -> Fighter {
        Fighter {
            team: self.team,
            side: self.side,
            spawn_mask: self.spawn_mask,
            kills: self.kills,
            suicides: self.suicides,
            deaths: self.deaths,
            shots: self.shots,
            hits: self.hits,
            headshots: self.headshots,
            bot: self.bot,
            out: self.out,
            out_at: self.out_at,
            extra: self.extra,
            buttons: self.buttons,
            late: self.late,
            r_off: self.r_off,
            z_armed: self.z_armed,
            start: self.start,
            gassed: self.gassed,
            class: self.class,
            bags: self.bags,
            ..*body
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

    /// On a mounted gun's seat (not jumping onto it).
    pub fn seated(&self) -> bool {
        self.gun.is_some_and(|m| m.state != gun::LAUNCH)
    }

    /// s126+0x75: 0x80 | the current item (0x26, the grenade, from the pull to 15 frames into the
    /// throw; the canister carried, the mask worn), 0 with nothing; `tick` is the next frame to run
    /// (the trace row).
    pub fn item(&self, tick: u32) -> u8 {
        match self.action {
            _ if self.flag.is_some() => 0,
            _ if self.carrying => 0x80 | gas::CANISTER,
            Action::GrenadePull { .. } | Action::GrenadeHold { .. } => 0xA6,
            Action::GrenadeThrow { .. } if tick <= self.since + RELEASE + 1 => 0xA6,
            _ if self.dropped.is_some_and(|d| tick <= d + 1) => 0xA6,
            _ if self.masked => 0x80 | gas::MASK,
            _ => self.weapon.map_or(0, |w| 0x80 | w),
        }
    }

    /// Frames into the gassed screen ([`Self::gassed`]) as of frame `tick` (the next to run): 1 in
    /// the frame after the script's call, up to [`gas::HAZE_FRAMES`]; `None` before or after.
    pub fn haze(&self, tick: u32) -> Option<u32> {
        let k = tick.checked_sub(self.gassed? + 1)?;
        (1..=gas::HAZE_FRAMES).contains(&k).then_some(k)
    }

    /// s126+0x78, the weapon the camera works with (`conker_camera::PlayerView::weapon`): the drawn
    /// weapon from its draw's first frame, 0 from the holster's, 0x26 while a grenade is pulled and
    /// held (V: the combat_ctlw probes and combat_ctl_cd; I: a grenade pulled with a weapon drawn,
    /// every traced pull was unarmed).
    pub fn camera_weapon(&self) -> u8 {
        match self.action {
            _ if self.seated() => conker_camera::GUN,
            // The canister is the item the camera reads (s126+0x78 = 0x25, V t2 1046).
            _ if self.carrying => gas::CANISTER,
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
            0x39 | 0x38 | 0x23 | 0x37 => 7, // 0x38: V e4 combat7_e_w38 761→768
            0x15 => 8,
            0x18 => 20,
            0x14 => 11,
            0x21 => 12,
            0x22 => 15,
            0x19 => 14, // the bat: its held model's draw event (I: no s126+0x11A trace)
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
            attached: self.seated(),
            gun: self.gun.filter(|_| self.seated()).map(|m| m.at),
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
            _ if self.seated() => anim::GUN_SEATED,
            _ if self.flag == Some(heist::CASH) => heist::IDLE,
            _ if self.flag == Some(raptor::EGG) => raptor::EGG_IDLE,
            _ if self.flag.is_some() => anim::FLAG_IDLE,
            _ if self.carrying => anim::CARRY_IDLE,
            Some(w) if self.drawn => w.idle,
            _ => u16::from(self.body.state),
        }
    }

    pub(crate) fn set(&mut self, action: Action, anim: u16, tick: u32) {
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PickupState {
    pub timer: u16,
    pub taken: Option<(usize, u32)>,
    /// A roaming pickup's spot ([`weapon::Roaming`]; `None`: not yet placed, or not roaming).
    pub spot: Option<u8>,
    /// War: Heist with at most 4 players (flag 1, [`Match::unique`]): who holds its weapon (it is
    /// gone while held), and where it lies after its holder's death, for `stay` more frames.
    pub holder: Option<usize>,
    pub drop: Option<[f32; 3]>,
    pub stay: u16,
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
    /// Total War: the out human `player` took over team-mate bot `from`'s body ([`Effect::TakeOver`]).
    TakeOver {
        player: usize,
        from: usize,
    },
    /// War: Heist: the script's voice line `id` (a sound id) at `player` ([`Effect::Voice`]).
    Voice {
        player: usize,
        id: u16,
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
    /// War: Colors: the player's team's score ([`Colors::scores`]), 0 in the other modes.
    pub score: u16,
}

impl Standing {
    pub fn of(f: &Fighter) -> Self {
        Self {
            team: f.team,
            kills: f.kills,
            deaths: f.deaths,
            shots: f.shots,
            out: f.out,
            score: 0,
        }
    }
}

/// Each player's place, 1 the best, as the original ranks them (`func_151E565C`; 0 for `None`, a
/// free slot): a player out of lives keeps the place it went out on; the others rank among
/// themselves by kills − deaths, those at the kill limit `kill_limit` (once reached) above the
/// rest, then fewer shots (s126+0x1AA) first. Still tied, they share the place: the game's third
/// clause (the lower slot first, for `t < s` below `0x80082FA0` = humans − 1) never fires (C; V
/// t5_b `0x800E0AC0` = 01 03 00 00). Bots count among the others but get no place there.
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

/// The two teams' places in Total War and Colors, [0] the squirrels', [1] the Tediz'
/// (`func_151E565C`'s team branch, `0x800E0AC0`): a team with no member in play is out and keeps
/// the place it went out on, the first of the two out 2nd (an empty team is out from the start, as
/// the original's unused teams are); the others rank by their score (Colors' captures,
/// [`Standing::score`]; metric 8, C, V t1, t3), then their members' kills, then fewer shots
/// (s126+0x1AA, bots' included). Both on 1st is a DRAW (V t0). ponytail: kills in both timings; untimed, the game ranks the
/// teams in play by Σ(lives·6 + health) first (C), which nothing shows: an untimed Total War ends
/// only once a team is out (V `elim`), and the one left is 1st whatever it scored. Two teams out:
/// the one whose last member went out first is 2nd (on one frame team 0's, which the game checks
/// first: [`Match::step_seen`], C).
pub fn team_places(all: &[Option<Standing>]) -> [u8; 2] {
    let p = teams_places(all, 2);
    [p[0], p[1]]
}

/// [`team_places`] for `n` teams (Heist's 4, `func_151E565C` metric case 0): the teams in play
/// rank by score (bags), kills, fewer shots, ties sharing a place; below them the out ones, the
/// later out the better (V unt1: [2, 3, 1, 4]; bots1 timed: [4, 1, 2, 3], GREENS's 13 kills over
/// YELLOWS's 10 on 1 bag each).
pub fn teams_places(all: &[Option<Standing>], n: u8) -> Vec<u8> {
    let members = |t: u8| all.iter().flatten().filter(move |s| s.team == t);
    let sum = |t, f: fn(&Standing) -> u16| members(t).map(|s| u32::from(f(s))).sum::<u32>();
    // A member's `out` is the players' place it went out on: the larger, the earlier.
    let out = |t| {
        (members(t).all(|s| s.out > 0)).then(|| members(t).map(|s| s.out).min().unwrap_or(u8::MAX))
    };
    let score = |t| members(t).map(|s| s.score).max().unwrap_or(0);
    let ahead = |a, b| {
        let (ka, kb) = (sum(a, |s| s.kills), sum(b, |s| s.kills));
        let tie = ka == kb && sum(a, |s| s.shots) < sum(b, |s| s.shots);
        score(a) > score(b) || (score(a) == score(b) && (ka > kb || tie))
    };
    let count = |f: &dyn Fn(u8) -> bool| (0..n).filter(|&u| f(u)).count() as u8;
    let in_play = count(&|u| out(u).is_none());
    (0..n)
        .map(|t| match out(t) {
            Some(a) => in_play + 1 + count(&|u| out(u).is_some_and(|b| b < a)),
            None => 1 + count(&|u| u != t && out(u).is_none() && ahead(u, t)),
        })
        .collect()
}

/// The winning team of [`team_places`] (0 squirrels, 1 Tediz); `None`: a DRAW, both 1st (the STATS'
/// header `func_151E43DC`; V `to_draw`: 0-0 kills and shots).
pub fn team_result(all: &[Option<Standing>]) -> Option<u8> {
    teams_result(all, 2)
}

/// [`team_result`] for `n` teams: the one team 1st, `None` when several share it.
pub fn teams_result(all: &[Option<Standing>], n: u8) -> Option<u8> {
    let p = teams_places(all, n);
    match p.iter().filter(|&&r| r == 1).count() {
        1 => p.iter().position(|&r| r == 1).map(|t| t as u8),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// The next frame to run (= the trace row its result shows on).
    pub tick: u32,
    /// A frame of the match's live parity: the oracle's read 23, where its match goes live (1 by
    /// default: the replays' ticks are the reads; the server sets the tick standing for read 23).
    /// Bolts and aimed shots first move an even number of frames after it ([`Self::first_move`]).
    pub live: u32,
    pub players: Vec<Fighter>,
    pub bullets: Vec<(u32, Bullet)>,
    pub grenades: Vec<(u32, Grenade)>,
    /// Rockets (weapon 0x16) and aimed shots (weapon 0x14) in flight, with their birth frame.
    pub rockets: Vec<(u32, Rocket)>,
    pub shots: Vec<(u32, Shot)>,
    pub pickups: Vec<PickupState>,
    /// VIs left (0 untimed).
    pub timer: i32,
    /// The frame the original latches the end (`0x800E0C20`, [`Self::ends`]): its check runs after
    /// the AI script and before the players, so a script's score (Colors' capture, Beach's escape)
    /// latches on its own frame, and what a frame's step brings (the clock at 0, a kill, a player
    /// out, Beach's death points) on the next.
    pub ended_at: Option<u32>,
    pub settings: Settings,
    pub rng: Rng,
    /// Slots whose player left, kept for the next to join ([`Self::join`]): a [`Fighter::vacant`]
    /// that the frame skips. The other players keep their slots.
    pub free: BTreeSet<usize>,
    /// Slots whose player joined and has not played yet (the server's `Waiting`, kept as it
    /// joined): nothing hurts them ([`Self::hurt`]), so no hit on them scores.
    pub waiting: BTreeSet<usize>,
    /// Total War's gas: the props the AI script drives and its countdown ([`Self::apply`]).
    pub gas: Gas,
    /// War: Colors' flags and scores ([`Self::apply`]).
    pub colors: Colors,
    /// War: Heist's bag and scores ([`Self::apply`]).
    pub heist: Heist,
    /// Per slot, s126+0x1AF: how far it reaches for a pickup or canister ([`weapon::touches`]);
    /// [`weapon::REACH`] for a slot not listed. The bots' AI script sets a bot's each frame.
    pub reach: Vec<u8>,
    /// Per slot, a bot's aim ray while it aims (s126+0x13C, its eye, and +0x130, the unit direction
    /// of its aim's yaw +0x16C and pitch +0x170); `None` for a slot not listed. The twin guns'
    /// bullets head from the muzzle to the first body part or wall on it, the pistol's and 0x23's
    /// take its pitch ([`Self::strike`]).
    pub aims: Vec<Option<([f32; 3], [f32; 3])>>,
    /// The scene's mounted guns ([`gun::spawn`]; none in a client's copy: their touch and fire are
    /// the server's, a gunner's own frame is predicted from its [`Fighter::gun`]).
    pub guns: Vec<Gun>,
    /// War: Beach's plunger (stepped in Beach only).
    pub plunger: Plunger,
    /// Scene 0x34's pressure plates and traps (`Traps::new`; none elsewhere, and none in a
    /// client's copy: their hits are the server's).
    pub traps: Traps,
    /// Tank's shells, bomb and wrecks ([`crate::tank`]).
    pub tank: Tank,
    /// War: Raptor's eggs, cooked eggs, baby and dropped bodies ([`crate::raptor`]).
    pub raptor: Raptor,
    /// Race's missiles, finishes and end ([`crate::race`]).
    pub race: Race,
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
            live: 1,
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
            waiting: BTreeSet::new(),
            gas: Gas::default(),
            colors: Colors::default(),
            heist: Heist::default(),
            reach: Vec::new(),
            aims: Vec::new(),
            guns: Vec::new(),
            plunger: Plunger::default(),
            traps: Traps::default(),
            tank: Tank::default(),
            raptor: Raptor::default(),
            race: Race::default(),
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
        if !matches!(
            f.action,
            Action::Hurt { .. } | Action::Free | Action::Pose(_)
        ) {
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
            f.burn = f.burn.filter(|b| b.attacker != Some(i));
        }
        self.let_go(i);
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
        self.fire_gun(i, world, seen, &mut ev);
        self.burn_player(i, &mut ev);
        self.fly_grenades(world, &mut ev);
        let rays = self.late_strikes(late, world, seen, &mut ev);
        if self.players[i].out == 0 && !self.settings.tank && !self.settings.race {
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
        // The scores at the frame's start: the script's (applied before the step) latch in their own
        // frame (V Colors e0 383, Beach end1 212), Beach's death points (scored in the step) in the
        // next with everything else the step brings (V bots1 8126; [`Self::ended_at`]).
        let scores = self.colors.scores;
        let bags = self.heist.scores;
        let was_in: Vec<bool> = (0..self.players.len()).map(|i| self.in_play(i)).collect();
        // Colors: the script's respawns (command 0x82), the flags' fall, then their touch.
        for i in 0..self.players.len().min(32) {
            if self.colors.respawns & 1 << i != 0 && !self.free.contains(&i) {
                self.respawn(i, arena, &mut ev);
            }
        }
        self.colors.respawns = 0;
        for f in &mut self.colors.flags {
            if f.released != Some(self.tick) {
                f.fall(world);
            }
        }
        self.heist.cash.step(world, self.tick);
        self.pickups(arena, &mut ev);
        if self.settings.raptor {
            self.touch_eggs();
        }
        if self.settings.tank {
            self.touch_bomb();
            tank::doors(&mut self.gas);
        } else {
            self.gas.step();
        }
        self.trap_hits(&mut ev);
        for g in &mut self.guns {
            g.leave = g.leave.saturating_sub(1);
        }
        let mut late = Vec::new();
        for i in 0..self.players.len() {
            if self.free.contains(&i) {
                continue;
            }
            let input = inputs.get(i).copied().unwrap_or_default();
            self.player(i, &input, arena, world, seen, &mut late, &mut ev);
        }
        // Race: no cone hits (0x33 has kind-0xE floor on its ramps; I: a board's height above
        // the floor, obj+0x28, is never 0 while it hovers).
        if !self.settings.race {
            self.cones(&mut ev);
        }
        self.mounted_guns(world, seen, &mut ev);
        if self.settings.beach {
            let stood = (self.players.iter().enumerate()).position(|(i, f)| {
                !self.free.contains(&i)
                    && f.health > 0
                    && !f.body.airborne
                    && Plunger::under(f.body.pos, self.plunger.y)
            });
            let (y, old) = (self.plunger.y, Plunger::top(self.plunger.y));
            self.plunger.step(stood);
            // Who is on its top goes with it (V b0-b8: the presser at the prop's y + 116 on the
            // same read), on the collision the next frame's `beach::seat` moves.
            let top = Plunger::top(self.plunger.y);
            for (i, f) in self.players.iter_mut().enumerate() {
                let b = &mut f.body;
                if !self.free.contains(&i)
                    && !b.airborne
                    && Plunger::under(b.pos, y)
                    && (b.pos[1] - old).abs() < 1.0
                {
                    (b.pos[1], b.floor_y) = (top, top);
                    if let Some(fl) = &mut b.floor {
                        fl.y = top;
                    }
                }
            }
        }
        if !self.traps.list.is_empty() {
            self.plates(world);
        }
        self.burn(&mut ev);
        self.fly_bullets(seen, &mut ev);
        self.fly_rockets(world, seen, &mut ev);
        self.fly_shots(world, seen, &mut ev);
        self.fly_shells(world, seen, &mut ev);
        if self.settings.race {
            self.fly_missiles(arena, &mut ev);
            self.race_end();
        }
        self.fly_grenades(world, &mut ev);
        let rays = self.late_strikes(late, world, seen, &mut ev);
        // The renderer measures every player's root once the frame's updates are done (conker-sim's
        // `animate`; V: 0x14's shot kills in its own update after the victim's, and the victim's
        // death 0x17E drifts from the third row on, combat4_w14 835).
        for (i, f) in self.players.iter_mut().enumerate() {
            if !self.free.contains(&i)
                && f.out == 0
                && !self.settings.tank
                && !self.settings.race
                && f.class != raptor::CLASS
            {
                animate(f, world);
            }
        }
        // The sniper's rays come last: its victim's death clip starts on the next frame, so the
        // drift begins a row later than a shot's and 0x17E plays 76 frames, not 75 (V e_w24 907,
        // combat4_w24; C: `func_150636F0` runs after the objects' updates, I: after the render).
        for (i, kind, origin) in rays {
            self.ray(i, kind, origin, world, seen, &mut ev);
        }
        for f in &mut self.colors.flags {
            if let Some(c) = f.carrier.and_then(|c| self.players.get(c)) {
                f.pos = colors::held(c.body.pos, c.body.facing);
            }
        }
        if let Some(c) = (self.heist.cash.carrier).and_then(|c| self.players.get(c)) {
            self.heist.cash.held(c.body.pos, c.body.facing);
        }
        if self.settings.raptor {
            self.hold_eggs();
        }
        // Total War, out on one frame: the game places the teams at the frame's end, team 0 first
        // (`func_151E565C`'s loop at 0x151E5DD0, C), so of two teams out together the Tediz win
        // whatever the slots: the frame's places go to team 0's first, the worst first.
        if self.settings.total_war {
            let mut fresh: Vec<usize> = (0..self.players.len())
                .filter(|&i| was_in[i] && !self.in_play(i) && !self.free.contains(&i))
                .collect();
            let mut outs: Vec<u8> = fresh.iter().map(|&i| self.players[i].out).collect();
            outs.sort_unstable_by(|a, b| b.cmp(a));
            fresh.sort_by_key(|&i| self.players[i].team);
            for (i, out) in fresh.into_iter().zip(outs) {
                self.players[i].out = out;
            }
        }
        self.timer = (self.timer - 2).max(0);
        // Race: the end fade comes after the results' 40 s in the level.
        let raced =
            (self.race.results).is_some_and(|r| self.tick + 1 + END_DELAY >= r + race::RESULTS_FOR);
        if self.ended_at.is_none() {
            if raced || self.score_end(&scores) || (self.settings.heist && self.score_end(&bags)) {
                self.ended_at = Some(self.tick);
            } else if self.ends() {
                self.ended_at = Some(self.tick + 1);
            }
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

    /// The slot of gun `k`'s gunner (jumping on or seated).
    pub fn gunner(&self, k: usize) -> Option<usize> {
        (self.players.iter()).position(|f| f.gun.is_some_and(|m| usize::from(m.gun) == k))
    }

    /// Gun `k`'s state (gun+0x232, [`gun::FREE`]..).
    pub fn gun_state(&self, k: usize) -> u8 {
        let mount = self.gunner(k).and_then(|i| self.players[i].gun);
        self.guns[k].state(mount.as_ref())
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
        // Colors: the clock, or a team's score reaching the target (latched on the capture's frame,
        // V e0 383; no score end at 0, and untimed then nothing ends it, I). Beach's the same
        // (`func_151E565C`'s metric for scenario 8, C; V end1, end2, bots1).
        if s.colors || s.beach {
            return time_up || self.score_end(&self.colors.scores);
        }
        // Heist's: Total War's with its bags (untimed: fewer than 2 teams with a participant left,
        // V unt1).
        // Raptor's: Heist's with the team scores (I untimed, as Heist's shared branch).
        if s.total_war || s.heist || s.raptor {
            let teams = (0..heist::TEAMS as u8).filter(|&t| in_play().any(|f| f.team == t));
            let bags = (s.heist && self.score_end(&self.heist.scores))
                || (s.raptor && self.score_end(&self.colors.scores));
            return time_up || bags || (out().next().is_some() && teams.count() < 2);
        }
        let kills = s.kills > 0 && taken().any(|i| self.players[i].kills >= u16::from(s.kills));
        let alone = out().next().is_some() && in_play().count() < 2;
        let no_human = out().any(|f| !f.bot) && in_play().all(|f| f.bot);
        time_up || kills || alone || no_human
    }

    /// Colors, Beach, Heist and Raptor: a team's score `scores` reaches the target (none at 0).
    fn score_end(&self, scores: &[u16]) -> bool {
        let (s, target) = (&self.settings, u16::from(self.settings.score));
        (s.colors || s.beach || s.heist || s.raptor)
            && target > 0
            && scores.iter().any(|&n| n >= target)
    }

    /// The teams the match ranks by: Heist's 4, the other War modes' 2, none in Deathmatch.
    pub fn teams(&self) -> u8 {
        let s = &self.settings;
        match () {
            _ if s.heist => heist::TEAMS as u8,
            _ if s.total_war || s.colors || s.beach || s.raptor => 2,
            _ => 0,
        }
    }

    /// Every slot's place, 1 the best ([`places`]; in the War modes its team's,
    /// [`team_places`]; free slots 0).
    pub fn places(&self) -> Vec<u8> {
        if self.settings.race {
            return (self.players.iter().enumerate())
                .map(|(i, f)| {
                    if self.free.contains(&i) {
                        0
                    } else {
                        f.ride.place
                    }
                })
                .collect();
        }
        let all = self.standings();
        if self.teams() > 0 {
            let teams = teams_places(&all, self.teams());
            let place = |s: &Standing| teams.get(usize::from(s.team)).copied().unwrap_or(0);
            return all.iter().map(|s| s.as_ref().map_or(0, place)).collect();
        }
        places(&all, self.settings.kills)
    }

    /// Every slot's [`Standing`] (`None` for a free one), Colors' and Beach's with its team's score.
    pub fn standings(&self) -> Vec<Option<Standing>> {
        let score = |f: &Fighter| match () {
            _ if self.settings.heist => self.heist.scores.get(usize::from(f.team)).copied(),
            _ if self.settings.colors || self.settings.beach || self.settings.raptor => {
                self.colors.scores.get(usize::from(f.team)).copied()
            }
            _ => None,
        };
        (self.players.iter().enumerate())
            .map(|(i, f)| {
                (!self.free.contains(&i)).then(|| Standing {
                    score: score(f).unwrap_or(0),
                    ..Standing::of(f)
                })
            })
            .collect()
    }

    /// Colors: player `i` lets go of the flag it carries, which falls from its hand.
    fn let_go(&mut self, i: usize) {
        let t = self.tick;
        let Some(f) = self.players.get_mut(i) else {
            return;
        };
        let Some(k) = f.flag.take() else {
            return;
        };
        if k == raptor::EGG {
            return self.let_go_egg(i);
        }
        // Heist: the throw's release (B), else a drop.
        if k == heist::CASH {
            let thrown = f.action == Action::Pose(Pose::Throw);
            if self.heist.cash.carrier == Some(i) {
                (self.heist.cash).let_go(f.body.pos, f.body.facing, thrown, t);
            }
            return;
        }
        let at = colors::held(f.body.pos, f.body.facing);
        if let Some(flag) =
            (self.colors.flags.get_mut(usize::from(k))).filter(|g| g.carrier == Some(i))
        {
            flag.carrier = None;
            (flag.pos, flag.fall, flag.released, flag.dropped) =
                (at, Some(colors::FALL), Some(t), true);
        }
    }

    /// The prop on `placement` the script drives: the gas's, and Tank's bomb.
    fn prop_mut(&mut self, placement: u16) -> Option<&mut gas::Prop> {
        match placement {
            tank::BOMB if self.settings.tank => Some(&mut self.tank.bomb),
            n => self.gas.prop_mut(n),
        }
    }

    /// Where pickup k stands: where its holder died, its roaming spot, else its place.
    pub fn pickup_at(&self, arena: &Arena, k: usize) -> [f32; 3] {
        let p = self.pickups.get(k);
        match (p.and_then(|p| p.drop), p.and_then(|p| p.spot)) {
            (Some(at), _) => at,
            (_, Some(s)) => arena.roaming.spots[usize::from(s)],
            _ => arena.pickups[k].1,
        }
    }

    /// The roaming pickups are unique (flag 1, `G[0x11F4]`): War: Heist with at most 4 players
    /// (0xB7's init, C; V h4a). Else (flag 3) one respawns 150 frames after its take, whoever holds
    /// it.
    pub fn unique(&self) -> bool {
        self.settings.heist && self.players.len() - self.free.len() <= 4
    }

    fn pickups(&mut self, arena: &Arena, ev: &mut Vec<Event>) {
        let t = self.tick;
        let unique = self.unique();
        for (k, (def, _)) in arena.pickups.iter().enumerate() {
            let p = &mut self.pickups[k];
            if let Some((who, at)) = p.taken {
                match def.gives {
                    Gives::Grenades if t == at + 1 => {
                        let f = &mut self.players[who];
                        f.grenades = (f.grenades + 3).min(weapon::MAX_GRENADES);
                        p.taken = None;
                    }
                    // Beach's bottle: 600 VIs of boost, the item kept (a Tedi's never drains: V wz).
                    Gives::Weapon(beach::ITEM) if t == at + 2 && self.settings.beach => {
                        p.taken = None;
                        self.players[who].boost = beach::BOOST;
                    }
                    // Race: the script's pickup loop counts the item on its next frame.
                    Gives::Weapon(id) if t == at + 1 && self.settings.race => {
                        p.taken = None;
                        self.race_take(who, id);
                    }
                    Gives::Weapon(id) if t == at + 2 => {
                        p.taken = None;
                        self.give(who, id);
                    }
                    // Tank: the power-up's bit and timer, replacing the one held (V p1, b1).
                    Gives::Power(bit) if t == at + 2 => {
                        p.taken = None;
                        let f = &mut self.players[who];
                        (f.power, f.boost) = (bit, tank::power_timer(bit));
                    }
                    // The mask takes the weapon's place (V d2: MP+0xEB0 13 → 19, s126+0x75 0xA1 →
                    // 0xA0, as a weapon's two frames after the touch) and goes on (V e2: state 81
                    // from that frame; I: not while hurt).
                    Gives::Mask if t == at + 2 => {
                        p.taken = None;
                        let f = &mut self.players[who];
                        (f.weapon, f.drawn, f.masked) = (None, false, true);
                        if !matches!(f.action, Action::Hurt { .. }) {
                            f.set(Action::Mask { on: true }, anim::MASK_ON, t);
                        }
                    }
                    _ => {}
                }
            }
            let respawn = arena.respawn.unwrap_or(weapon::PICKUP_RESPAWN);
            // Flag 1: gone while its holder holds the weapon (delivered or not); the holder's death
            // drops it where it died, 50 up, 4 VIs on, for 300 VIs; any other loss brings it back
            // after 300 VIs on a random spot (`func_1508DAEC`, C).
            if let Some(h) = self.pickups[k].holder.filter(|_| k < arena.roaming.count) {
                let f = &self.players[h];
                let id = match def.gives {
                    Gives::Weapon(id) => Some(id),
                    _ => None,
                };
                let gone = self.free.contains(&h);
                let p = &mut self.pickups[k];
                if p.taken.is_some() || (!gone && f.health > 0 && f.weapon == id) {
                    continue;
                }
                p.holder = None;
                (p.drop, p.timer) = match !gone && f.health == 0 {
                    true => (
                        Some([f.body.pos[0], f.body.pos[1] + 50.0, f.body.pos[2]]),
                        2,
                    ),
                    false => (None, respawn),
                };
            }
            let p = &mut self.pickups[k];
            let back = p.timer == 1;
            p.timer = p.timer.saturating_sub(1);
            if p.timer > 0 {
                continue;
            }
            // Where the holder died, it stays its 300 VIs, then moves on.
            let mut moves = false;
            if p.drop.is_some() {
                if back {
                    p.stay = respawn;
                } else if p.stay <= 1 {
                    (p.drop, p.stay, moves) = (None, 0, true);
                } else {
                    p.stay -= 1;
                }
            }
            // A roaming pickup moves at each appearance: the first frame, then each return.
            let p = self.pickups[k];
            if k < arena.roaming.count && p.drop.is_none() && (back || moves || p.spot.is_none()) {
                let held: Vec<_> = self.pickups.iter().map(|p| p.spot).collect();
                self.pickups[k].spot = arena.roaming.pick(&held, &mut self.rng);
            }
            let pos = self.pickup_at(arena, k);
            let p = &mut self.pickups[k];
            // A mask is touched only while the gas counts down (`func_1511DF6C` skips model 0x54 on
            // scene 0x2D while G+0x1FE4 ≤ 0, C), and not by a player wearing one (I).
            let countdown = self.gas.countdown;
            // War: Raptor's raptors take nothing (V r11; I the rule: class 9 holds no item).
            let may = |f: &Fighter| {
                f.class != raptor::CLASS
                    && (def.gives != Gives::Mask || (countdown > 0 && !f.masked))
            };
            let reach = |i: usize| self.reach.get(i).copied().unwrap_or(weapon::REACH);
            let toucher = (self.players.iter().enumerate()).position(|(i, f)| {
                f.health > 0 && may(f) && weapon::touches(pos, f.body.pos, reach(i))
            });
            if let Some(i) = toucher {
                *p = PickupState {
                    timer: match self.settings.beach {
                        true => beach::BOTTLE_RESPAWN,
                        false => respawn,
                    },
                    taken: Some((i, t)),
                    spot: p.spot,
                    holder: (unique && k < arena.roaming.count).then_some(i),
                    ..PickupState::default()
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
            let reach = |i: usize| self.reach.get(i).copied().unwrap_or(weapon::REACH);
            let toucher = (self.players.iter().enumerate()).position(|(i, f)| {
                f.health > 0
                    && gas::admits(prop.gate, f.team)
                    && weapon::touches(pos, f.body.pos, reach(i))
            });
            if let (Some(i), Some(p)) = (toucher, self.gas.prop_mut(placement)) {
                (p.flag, p.shown) = (i as i32 + 1, false);
            }
        }
        // Colors: a flag is touched by a player of the team that takes it, alive, holding nothing,
        // not held (obj+0x89) nor stunned (`func_150597FC`, C); it grabs it at once (state 213,
        // V e1 326 → 327) and holds it from then on (obj+0x13C; the script hears of it next).
        // ponytail: the actions that hold the player (draws, grenades, hits, poses) stand in for
        // obj+0x89's other setters.
        for k in 0..colors::FLAGS.len() {
            let flag = self.colors.flags[k];
            if !flag.touchable(t) {
                continue;
            }
            let toucher = self.players.iter().position(|f| {
                f.health > 0
                    && f.flag.is_none()
                    && !f.carrying
                    && f.stun == 0
                    && f.body.lock == 0
                    && matches!(
                        f.action,
                        Action::Free
                            | Action::Fire { .. }
                            | Action::Aim { .. }
                            | Action::Reload { .. }
                    )
                    && colors::takes(k, f.team)
                    && colors::touches(&flag, &f.body, f.other())
            });
            if let Some(i) = toucher {
                let f = &mut self.players[i];
                (f.flag, f.drawn) = (Some(k as u8), false);
                let (clip, ..) = Pose::Grab.clip();
                f.set(Action::Pose(Pose::Grab), clip, t);
                let flag = &mut self.colors.flags[k];
                (flag.carrier, flag.fall) = (Some(i), None);
            }
        }
        // Heist: the bag is touched as a flag is, by anyone (V h1 334: the grab 0x114 at once, the
        // bat put away; the script hears of it next frame).
        if self.settings.heist && self.heist.cash.touchable(t) {
            let cash = self.heist.cash;
            let toucher = self.players.iter().enumerate().position(|(i, f)| {
                !self.free.contains(&i)
                    && f.health > 0
                    && f.flag.is_none()
                    && f.stun == 0
                    && f.body.lock == 0
                    && matches!(
                        f.action,
                        Action::Free
                            | Action::Fire { .. }
                            | Action::Aim { .. }
                            | Action::Reload { .. }
                    )
                    && heist::touches(&cash, &f.body, f.other())
            });
            if let Some(i) = toucher {
                let f = &mut self.players[i];
                (f.flag, f.drawn) = (Some(heist::CASH), false);
                let (clip, ..) = Pose::Snatch.clip();
                f.set(Action::Pose(Pose::Snatch), clip, t);
                self.heist.cash.take(i, t);
            }
        }
    }

    /// Carries out what the AI script did this frame ([`gas::Effect`]), before [`Self::step`];
    /// returns the hits' events. Over, nothing: the game's script stops with the level (Colors'
    /// final capture would otherwise fade and respawn everyone through the results).
    pub fn apply(&mut self, effects: &[Effect]) -> Vec<Event> {
        if self.phase() == Phase::Over {
            return Vec::new();
        }
        let t = self.tick;
        let mut ev = Vec::new();
        for &e in effects {
            match e {
                Effect::Show(n) => {
                    if let Some(p) = self.prop_mut(n) {
                        p.shown = true;
                    }
                }
                Effect::Set {
                    placement,
                    flag,
                    gate,
                } => {
                    if let Some(p) = self.prop_mut(placement) {
                        p.flag = flag.unwrap_or(p.flag);
                        p.gate = gate.unwrap_or(p.gate);
                    }
                }
                // Tank: the plant starts it; its bunker is its bomber's.
                Effect::Countdown(n) => {
                    if self.settings.tank && n > 0 && self.gas.countdown <= 0 {
                        let at = self.tank.last.and_then(|c| self.players.get(c));
                        self.tank.planted = at.map(|f| tank::bunker(f.body.pos));
                    }
                    self.gas.countdown = n;
                }
                Effect::Gas(on) => self.gas.gas = on,
                // Tank: the detonation (the script's 0x2B with 0x29) kills, immune or not.
                Effect::Hit { slot, attacker, .. }
                    if self.settings.tank && slot < self.players.len() =>
                {
                    let attacker = attacker.filter(|&a| a < self.players.len());
                    self.tank_hit(slot, attacker, 0, true, true, &mut ev);
                }
                Effect::Hit {
                    slot,
                    kind,
                    attacker,
                } if slot < self.players.len() => {
                    let attacker = attacker.filter(|&a| a < self.players.len());
                    self.hurt(slot, &kind, false, false, 0, attacker, &mut ev);
                }
                // The mask comes off with its clip (V e2: state 78 from the frame the script deletes
                // it; I: not while hurt).
                Effect::Release(i) => {
                    if let Some(f) = self.players.get_mut(i) {
                        if f.masked && !matches!(f.action, Action::Hurt { .. }) {
                            f.set(Action::Mask { on: false }, anim::MASK_OFF, t);
                        }
                        f.masked = false;
                    }
                }
                Effect::Haze(i) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.gassed = Some(t);
                    }
                }
                // Tank: the bomb on the tank (the script's 0x4B, item 0x18), off it (0x4C).
                Effect::Carry(i, on) if self.settings.tank && i < self.players.len() => {
                    self.players[i].carrying = on && self.players[i].health > 0;
                    match on {
                        true => self.tank.carrier = Some(i),
                        false => {
                            self.tank.carrier = self.tank.carrier.filter(|&c| c != i);
                            self.tank.last = Some(i);
                        }
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
                Effect::Flag(k, shown) if k < colors::FLAGS.len() => {
                    if let Some(c) = self.colors.flags[k].carrier
                        && let Some(f) = self.players.get_mut(c)
                    {
                        f.flag = None;
                    }
                    self.colors.flags[k] = colors::Flag {
                        shown,
                        pos: colors::FLAGS[k].home,
                        ..colors::Flag::default()
                    };
                }
                Effect::LetGo(k) if k == usize::from(heist::CASH) => {
                    if let Some(c) = self.heist.cash.carrier {
                        self.let_go(c);
                    }
                }
                Effect::LetGo(k) => {
                    if let Some(c) = self.colors.flags.get(k).and_then(|f| f.carrier) {
                        self.let_go(c);
                    }
                }
                Effect::Settle(k) if k == usize::from(heist::CASH) => {
                    self.heist.cash.thrown = false
                }
                Effect::Settle(k) => {
                    if let Some(f) = self.colors.flags.get_mut(k) {
                        f.dropped = false;
                    }
                }
                // Heist: a bag delivered; its last carrier, on the team, delivered it (the script
                // let it go first, command 7).
                Effect::Score(team, n) if self.settings.heist => {
                    if let Some(s) = self.heist.scores.get_mut(usize::from(team)) {
                        *s = s.saturating_add(n);
                    }
                    let last = self.heist.cash.last.and_then(|c| self.players.get_mut(c));
                    if let Some(f) = last.filter(|f| f.team == team) {
                        f.bags = f.bags.saturating_add(n);
                    }
                }
                Effect::Score(team, n) => {
                    if let Some(s) = self.colors.scores.get_mut(usize::from(team)) {
                        *s = s.saturating_add(n);
                    }
                }
                Effect::Cash(shown) => {
                    if let Some(f) = (self.heist.cash.carrier).and_then(|c| self.players.get_mut(c))
                    {
                        f.flag = None;
                    }
                    self.heist.cash = match shown {
                        true => heist::Cash::new(t),
                        false => heist::Cash::default(),
                    };
                }
                Effect::CashAt(pos, yaw, walking) if self.heist.cash.walks(t) => {
                    let c = &mut self.heist.cash;
                    let state = if walking {
                        heist::State::Walk
                    } else {
                        heist::State::Stand
                    };
                    if c.state != state {
                        c.since = t;
                    }
                    (c.pos, c.yaw, c.state) = (pos, yaw, state);
                }
                Effect::Voice(player, id) => ev.push(Event::Voice { player, id }),
                Effect::Raptor(op) => self.raptor_op(op),
                Effect::Race(op) => self.race_op(op),
                // The capture's poses take over whatever the player was doing, a hit's included
                // (I; nobody is hurt through them: the script keeps everyone immune).
                Effect::Pose(i, pose) => {
                    if let Some(f) = self.players.get_mut(i).filter(|f| f.health > 0) {
                        let (clip, ..) = pose.clip();
                        f.set(Action::Pose(pose), clip, t);
                    }
                }
                Effect::Immune(i, n) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.immune = n;
                    }
                }
                Effect::AimError(i, e) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.aim_error = e;
                    }
                }
                Effect::Hold(i, n) => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.body.lock = n;
                    }
                }
                Effect::Respawn(i) if i < 32 => self.colors.respawns |= 1 << i,
                Effect::Fade(out, vis) => self.colors.fade = Some((out, vis, t)),
                // Colors' capture respawns everyone (command 0x82) before it recreates the guns in
                // the same script frame (V bots1 3073: the gunner seated on 3072, at its base on
                // 3073), so its gunner is already gone: the respawn, queued, wins. A gunner left on
                // a recreated gun would fall from the seat (I: unseen).
                Effect::Gun(k) if k < self.guns.len() => {
                    self.guns[k] = Gun::new(self.guns[k].place);
                    for f in &mut self.players {
                        if f.gun.is_some_and(|m| usize::from(m.gun) == k) {
                            f.gun = None;
                            (f.body.airborne, f.body.floor) = (true, None);
                        }
                    }
                }
                // Beach's (the other scripts send none of these but 0x1F, whose records the
                // match's own masks already are).
                Effect::Start(i, record) if self.settings.beach && record < 32 => {
                    if let Some(f) = self.players.get_mut(i) {
                        f.spawn_mask = 1 << record;
                    }
                }
                // A weapon taken goes holstered (the frame's action ends with it); the same one
                // stays as it is.
                Effect::Give(i, w) if self.players.get(i).is_some_and(|f| f.weapon != Some(w)) => {
                    match w {
                        0 => {
                            let f = &mut self.players[i];
                            (f.weapon, f.drawn) = (None, false);
                        }
                        w => self.give(i, w),
                    }
                }
                Effect::Strike(i) if i < self.players.len() => {
                    self.hurt(i, &damage::STRIKE, false, false, 0, None, &mut ev);
                }
                Effect::Plunger(held) => self.plunger.held = held,
                Effect::Place(i, pos) => {
                    if let Some(f) = self.players.get_mut(i) {
                        let b = &mut f.body;
                        (b.pos, b.xz_speed, b.y_vel) = (pos, 0.0, 0.0);
                        (b.airborne, b.floor) = (true, None);
                    }
                }
                Effect::TakeOver(i, j) if self.takes_over(i, j) => {
                    let (h, b) = (self.players[i], self.players[j]);
                    let left = self.lives_left(j);
                    self.players[i] = Fighter {
                        out: 0,
                        immune: TAKEOVER_IMMUNE,
                        extra: (h.deaths + left).saturating_sub(self.settings.lives().into()),
                        ..h.in_body(&b)
                    };
                    self.players[j] = Fighter {
                        out: h.out,
                        out_at: t,
                        ..b.in_body(&h)
                    };
                    // Heist: the bag goes with the body (0x40E2 makes the human the carrier).
                    if self.heist.cash.carrier == Some(j) {
                        self.heist.cash.carrier = Some(i);
                    }
                    ev.push(Event::TakeOver { player: i, from: j });
                }
                _ => {}
            }
        }
        ev
    }

    /// Lives player `i` has left: the match's and its [`Fighter::extra`] less its deaths; none once
    /// out (a bot whose body was taken over keeps its deaths, V t1 438: lives 0).
    pub fn lives_left(&self, i: usize) -> u16 {
        let f = &self.players[i];
        let left = (u16::from(self.settings.lives()) + f.extra).saturating_sub(f.deaths);
        if f.out > 0 { 0 } else { left }
    }

    /// [`Effect::TakeOver`]`(i, j)` goes ahead: `i` out [`TAKEOVER_DELAY`] frames, `j` its living
    /// team-mate bot in play (`func_1508E89C` refuses before the skull is grown, C; the script
    /// picks `j` and sends it every frame from the out frame's next).
    fn takes_over(&self, i: usize, j: usize) -> bool {
        let n = self.players.len();
        if i >= n || j >= n || i == j || !self.in_play(j) || self.free.contains(&i) {
            return false;
        }
        let (h, b) = (&self.players[i], &self.players[j]);
        h.out > 0
            && self.tick >= h.out_at + TAKEOVER_DELAY
            && b.bot
            && b.health > 0
            && b.team == h.team
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
        // A killed player lets its pickup go 3 frames after the killing hit, and the weapon
        // manager (`func_1508DC24`) swaps its weapon for the start item (MP+0x1170), none or
        // holstered: it takes actions 0x1D/0x1E away and clears s126+0x75, +0x78 and +0x11A (V e4
        // dth: the twin guns held through 790-792, gone at 793; bots3's two deaths alike). A
        // start item held is kept until the respawn (V combat6_e40: the club, 1062 → 1213).
        let f = &mut self.players[i];
        if let Action::Hurt {
            lethal: true, step, ..
        } = f.action
            && (step > 0 || t - f.since >= 3)
            && f.weapon != f.start
        {
            (f.weapon, f.drawn, f.body.armed) = (f.start, false, 0);
        }
        if self.settings.tank {
            return self.tank_player(i, input, arena, world, ev);
        }
        if self.settings.race {
            return self.race_player(i, input, arena, world, ev);
        }
        // The match's start item: the manager hands it over whenever the player holds no weapon
        // (`func_1508DC24` with no pickup held: MP+0x1170; V combat6_e40 1214, the frame after
        // the respawn).
        if let (None, Some(id)) = (self.players[i].weapon, self.players[i].start) {
            self.give(i, id);
        }
        // The raptor (War: Raptor's, a pick on 0x3F) keeps the club in the slot but never draws
        // it (V: 0xC0); its frame is its own in any mode (V: its movement on every map).
        if self.players[i].class == raptor::CLASS {
            return self.raptor_player(i, input, arena, world, seen, ev);
        }
        // The other players push this one out of their bodies (conker-sim's `step_among`), those
        // earlier in the slot order where they moved to this frame, and so do the mounted guns (a
        // gunner and one jumping on move by their own rules) and Colors' uncarried flags.
        let flags = (self.colors.flags.iter()).filter(|f| f.shown && f.carrier.is_none());
        let cash = Some(self.heist.cash).filter(|c| c.shown && c.carrier.is_none());
        // A bolt sitting where it was fired pushes too, its shooter included: an object of radius 5,
        // centre 0, vertical scale 1, no owner (`func_1505D6F0`, C; V e4 cbp2: the shooter moved
        // (1.843, 2.152) then (1.577, 1.841) by the bolt at its muzzle, cb3's drift of ~5.3 a hip
        // shot). ponytail: the client does not predict it (its bolts are not in `Fighting`), the
        // server's correction moves it.
        let bolts = (self.shots.iter())
            .filter(|(born, s)| {
                s.step == projectile::BOLT_STEP && *born < t && t <= self.first_move(*born)
            })
            .map(|(_, s)| Other::new(s.pos, BOLT_BODY));
        // An enemy with a weapon drawn (its s126+0x78) within the scare's reach on the push's
        // distance (`func_1505D6F0`, C) scares this player below.
        let me = self.players[i].other();
        let scare = (self.players.iter().enumerate()).any(|(j, p)| {
            j != i && p.body.armed != 0 && self.enemies(i, j) && me.within(&p.other(), SCARE_D2)
        });
        let others: Vec<Other> = (self.players.iter().enumerate())
            .filter(|&(j, _)| j != i)
            .map(|(_, p)| p.other())
            .chain(self.guns.iter().map(|g| gun::body(g.place.pos)))
            .chain(flags.map(|f| colors::body(f.pos)))
            .chain(cash.map(|c| heist::body(c.pos)))
            .chain(bolts)
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
        // Colors: a pose lets go of the carried flag on its frame (the put-down 20 frames in, the
        // plant 19: V b1 370, e0 402).
        if let Action::Pose(pose) = self.players[i].action
            && pose.clip().2 == Some(t - self.players[i].since)
        {
            self.let_go(i);
        }
        let f = &mut self.players[i];
        // obj+0xF4 bit 0x10000 clears at every frame's start (`func_1504A730`), a hit's too.
        f.body.walled = false;
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
            // A hurt gunner's trigger takes Z and B as on the seat (V h1: Z held fires through
            // state 29, h2: let go there, the fire runs its 20 frames out, h3: first pressed there,
            // it fires).
            let trigger = (
                input.buttons & BUTTON_Z != 0,
                input.buttons & BUTTON_B != 0 && late & BUTTON_B == 0,
            );
            self.hurt_frame(
                i, reaction, lethal, step, trigger, arena, world, &others, ev,
            );
            return;
        }
        let held = |b: u16| input.buttons & b != 0;
        // A mounted gun's gunner: the jump onto it (0x32 for 19 frames, then 0x38) until it comes
        // down on the seat (V m2 61 → 83, e1 41 → 66), then the seat, which turns with the scope and
        // where Z, B and A are all the buttons do (`crate::gun`).
        if let Some(mut m) = f.gun {
            if m.state == gun::LAUNCH {
                movement::hop(&mut f.body, m.to, world);
                f.immune = 255;
                f.anim = match t - f.since < gun::LAUNCH_CLIP {
                    true => id::JUMP,
                    false => id::FALL,
                };
                let yaw = f.body.move_angle;
                if f.body.pos[1] < m.at[1] + gun::MOUNT_HEIGHT && f.body.y_vel < 0.0 {
                    // Level: an unmanned gun's pitch has decayed by then (30 frames of state 5).
                    movement::sit(&mut f.body, gun::seat(m.at, yaw, 0), yaw);
                    (m.state, f.immune) = (gun::IDLE, 0);
                }
                f.gun = Some(m);
                return;
            }
            let yaw = (f.body.scope_yaw).map_or(f.body.facing, |y| {
                (y * (65536.0 / 360.0)).round() as i32 as u16
            });
            f.body.prev_buttons = input.buttons;
            // A jumps back off, immune 20 frames; the gun waits 30 before anyone takes it (V m2
            // 1601-1630). In multiplayer the facing is 0xB000 at z > 0, else 0x3000, whatever the
            // gun's yaw, and the jump goes 0x1200 left of it (`func_1505F298`, C; V m2 and l3 on
            // z > 0, l1 on z < 0 with the gun turned 90°).
            if held(BUTTON_A) && late & BUTTON_A == 0 {
                let facing: u16 = if f.body.pos[2] > 0.0 { 0xB000 } else { 0x3000 };
                let angle = facing.wrapping_add(0x1200);
                let me = f.other();
                movement::leap(&mut f.body, angle, world, me, &others);
                (f.gun, f.immune) = (None, 20);
                f.set(Action::Free, id::JUMP, t);
                if let Some(g) = self.guns.get_mut(usize::from(m.gun)) {
                    g.leave = gun::LEAVE;
                }
                return;
            }
            m.step(held(BUTTON_Z), held(BUTTON_B) && late & BUTTON_B == 0);
            f.anim = match m.state {
                gun::FIRING => anim::GUN_FIRING,
                _ => anim::GUN_SEATED,
            };
            let seat = gun::seat(m.at, yaw, gun::pitch10(f.scope_pitch));
            movement::sit(&mut f.body, seat, yaw);
            f.gun = Some(m);
            return;
        }
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
            (Action::Mask { .. }, _) if n >= gas::MASK_FRAMES => {
                let anim = f.idle_anim();
                f.set(Action::Free, anim, t);
            }
            (Action::Pose(pose), _) if n >= pose.clip().1 => {
                let anim = f.idle_anim();
                f.set(Action::Free, anim, t);
            }
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
        let no_fire = armed.is_some_and(|w| w.id == 0x16 || w.club()) && late_held(BUTTON_R);
        // The club: a Z press in a swing draws the RNG (`func_150641D8`, C) and, within
        // `COMBO_WINDOW` frames of the first swing's hit, chains the second (V combat6_e40 870,
        // 990; 1110: no hit, nothing).
        if let Action::Fire { .. } = f.action
            && armed.is_some_and(|w| w.club())
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
        // Colors: holding a flag, B puts it down and nothing else starts (V b1, h2, e1).
        // Heist: B throws the bag (V h2: clip 0x118 from 381, B on 380).
        if f.flag.is_some_and(|k| k != raptor::EGG) && f.action == Action::Free && pressed(BUTTON_B)
        {
            let pose = match f.flag == Some(heist::CASH) {
                true => Pose::Throw,
                false => Pose::PutDown,
            };
            let (clip, ..) = pose.clip();
            f.set(Action::Pose(pose), clip, t);
        }
        if f.action == Action::Free && !aiming && !f.carrying && f.flag.is_none() {
            // A villager's item 0x3E throws no grenade (V beach-gunner gren: C-down held 60-100, the
            // pull never starts, 3 kept; I: where the game refuses it). Nor does a player with a
            // weapon drawn (s126+0x78 ≠ 0, `func_1504CB98` at 0x1504CF9C, C): its C-down steps back
            // with the C strafe (V e4 cd09, cd15: twin guns 0x22C, flamethrower 0xED while held;
            // combat6_ctlw40 880: the club).
            if late_pressed(C_DOWN)
                && f.grenades > 0
                && armed.is_none()
                && f.weapon != Some(beach::ITEM)
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
                        Attack::Rocket { .. } if self.settings.beach => beach::ROCKET_FIRE,
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
        let tommy = matches!(f.action, Action::Fire { .. }) && w.is_some_and(|w| w.id == 0x38);
        f.climb.step(tommy && held(BUTTON_Z));
        if tommy {
            f.anim = weapon::tommy_clip(f.body.xz_speed, f.body.target_speed);
        }
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
                        // The tommy gun also fires on the press, and has no clip (V h4: Z held
                        // 160-400, s126+0x1AA 1 at read 161, then +1 at 164, 168, … 404: 62).
                        let press = w.id == 0x38 && n == 0;
                        for (m, o) in muzzles.iter().zip(at) {
                            if (press || cycles.iter().any(|&c| c != NONE && t == c + o))
                                && (f.rounds > 0 || w.rounds == 0)
                            {
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
                        strikes.push(Strike::Rocket(Rocket::fire(world, i, at, aim(f).0)));
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
        // The mask's clips: the first frame is a full move with the stick, then the stick and
        // buttons are let go and the state held (obj+0x83 = 0xFF, C; 2 here lapses the frame after
        // the clip): the speed brakes 2.5 a frame and the facing turns 3072 a frame to the frozen
        // move angle, which no idle snaps (V e2: speed 3 → 4 → 1.5 → 0, facing to 42496; no jump,
        // fire or move in e3).
        if matches!(f.action, Action::Mask { .. }) && t > f.since {
            (mv.stick_x, mv.stick_y, mv.buttons) = (0, 0, 0);
            f.body.timer = 2;
        }
        // Colors: A and Z do nothing holding a flag (V h2 Z 400, e1 A 350), nor carrying a canister
        // (V t2 1080, 1130, 1140); a pose holds the player from its first frame (obj+0x89 = 255, V e1
        // grab, b1 put-down).
        if f.flag.is_some() || f.carrying {
            mv.buttons &= !(BUTTON_A | BUTTON_Z);
        }
        if matches!(f.action, Action::Pose(_)) {
            (mv.stick_x, mv.stick_y, mv.buttons) = (0, 0, 0);
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
        // s126+0x4A: 20 while an armed enemy is near this unarmed player, else one less a frame
        // (`func_1504BC38`, C; V e4 sge2a: player 2's from player 1's draw at 760 on).
        f.body.scared = match scare && f.body.armed == 0 {
            true => SCARE_FRAMES,
            false => f.body.scared.saturating_sub(1),
        };
        f.body.holding = f.flag.is_some();
        f.body.heavy = matches!(f.flag, Some(heist::CASH | raptor::EGG));
        // Beach's item 0x3E drives a villager, but for the crouch's 40 % (V spd: 14; I: its handler
        // skipped in the crouch states, the boost kept).
        use movement::state::{CROUCH, CROUCH_WALK};
        let crouched = matches!(f.body.state, CROUCH | CROUCH_WALK);
        if self.settings.beach
            && f.team == beach::VILLAGERS
            && f.weapon == Some(beach::ITEM)
            && !crouched
        {
            f.body.speed_percent = beach::drive(&mut f.body.timer, &mut f.boost);
        }
        if burning {
            burn_run(f, world, t, &others);
        } else {
            f.bump = 0;
            let me = f.other();
            movement::step_among(&mut f.body, &mv, world, me, &others);
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
                (_, state @ (JUMP | RUN_JUMP | FALL | LAND)) if f.flag.is_some() => {
                    u16::from(state)
                }
                _ if f.flag == Some(heist::CASH) => heist::carry_clip(f.body.xz_speed),
                _ if f.flag == Some(raptor::EGG) => raptor::carry_clip(f.body.xz_speed),
                _ if f.flag.is_some() => colors::carry_clip(f.body.xz_speed),
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
                    // The bat walks and runs as the club (V h3).
                    let id = if w.club() { 0x40 } else { w.id };
                    match conker_anim::armed_move(id, f.body.xz_speed, f.body.target_speed) {
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
            Strike::Bullet(mut b) => {
                // A bot's twin-gun bullet leaves the muzzle toward the first body part or wall its
                // aim ray meets (`func_151C62D0` → `func_151D5174` for a bot, s126+0x84, then
                // `func_151C4B0C`; C), not along its facing: V obul, the bullets' pitch lies
                // between the muzzle's and the eye's to the aim. 0x23's and (I) the tommy gun's
                // (`func_150FE320`, `func_150FE860`) keep the facing and take the pitch byte
                // obj+0x1D1 (`func_15145740`, C), which follows a bot's aim pitch +0x170 (V o34p:
                // 0x23 within 3.4°, in 1.1° steps; the steps and the lag are not ported). The
                // pistol's (`func_150FED30`) takes `func_150FF2D4`'s, untraced: as 0x23's here (I).
                let ray = self.aims.get(i).copied().flatten();
                if let Some((eye, dir)) = ray {
                    let to: [f32; 3] = if b.kind == damage::TWIN_BULLET {
                        let wall = raycast(world, eye, dir, RAY_RANGE);
                        let d = self
                            .first_part(i, eye, dir, wall, seen)
                            .map_or(wall, |h| h.1);
                        core::array::from_fn(|j| eye[j] + dir[j] * d - b.pos[j])
                    } else {
                        let h = (1.0 - dir[1] * dir[1]).sqrt();
                        [b.dir[0] * h, dir[1], b.dir[2] * h]
                    };
                    let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
                    if len > 0.0 {
                        let dir = to.map(|c| c / len);
                        b = Bullet::aimed(world, i, b.pos, dir, b.speed, b.kind);
                    }
                }
                // The tommy gun's climb lifts its bullets (`func_1504CB98`, C; I: added to the
                // bullet's own pitch).
                let climb = self.players[i].climb.pitch();
                if climb > 0.0 && self.players[i].weapon == Some(0x38) {
                    let h = b.dir[0].hypot(b.dir[2]).max(f32::EPSILON);
                    let (sin, cos) = (b.dir[1].atan2(h) + climb).sin_cos();
                    let dir = [b.dir[0] / h * cos, sin, b.dir[2] / h * cos];
                    b = Bullet::aimed(world, i, b.pos, dir, b.speed, b.kind);
                }
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
                // Each sphere in turn (a victim one leaves immune is out of the next; none tests
                // the stun, so 0x32 lands on the stagger of the muzzle's 0x3A, which leaves no
                // immunity: V e4 combat7_sgpb 790), the shooter left out; the knock-back along the
                // facing, or away from the sphere's centre with flag 0x10.
                let (c, s) = cos_sin(me.facing);
                for (kind, d, r) in weapon::SPREAD {
                    let centre = [muzzle[0] + d * c, muzzle[1], muzzle[2] - d * s];
                    for v in (0..self.players.len()).filter(|&v| v != i) {
                        let p = self.seen_pos(seen, i, v);
                        if !in_reach(centre, r, p, self.players[v].hitbox())
                            || (kind.flags & 0x2000 != 0 && !clear(world, muzzle, p))
                        {
                            continue;
                        }
                        let angle = if kind.flags & 0x10 != 0 {
                            away_from(world, centre, p)
                        } else {
                            me.facing
                        };
                        self.hurt(v, &kind, true, false, angle, Some(i), ev);
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
                        let h = self.players[v].hitbox();
                        if !in_reach(centre, flame::FLAME_R, p, h) {
                            continue;
                        }
                        let away = away_from(world, centre, p);
                        if self.hurt(v, &kind, true, false, away, Some(i), ev) {
                            self.players[v].burn = Some(Burn::new(Some(i)));
                        }
                    }
                }
            }
            Strike::Melee(kind) => {
                // The class table's sphere: forward 2·b_A, up 2·b_C, radius 2·b_E at scale 0.5.
                let [fwd, up, r] = kind.reach.map(|v| 2.0 * f32::from(v));
                let centre = local_to_world(me.pos, me.facing, [fwd, 0.0, up]);
                for v in (0..self.players.len()).filter(|&v| v != i) {
                    let (p, h) = (self.seen_pos(seen, i, v), self.players[v].hitbox());
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
    pub(crate) fn seen_pos(&self, seen: Seen, s: usize, v: usize) -> [f32; 3] {
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
                let (at, h) = (self.seen_pos(seen, owner, v), self.players[v].hitbox());
                projectile::hits_ellipsoid(from, dir, len, at, h).map(|d| (v, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The first body the segment meets (crouching bodies are lower) and whether the hit is on
    /// the head: at [`BULLET_HEAD`] or more above the feet, [`projectile::CROUCH_DROP`] less
    /// when crouched.
    fn first_body(
        &self,
        owner: usize,
        from: [f32; 3],
        dir: [f32; 3],
        len: f32,
        seen: Seen,
    ) -> Option<(usize, bool)> {
        (0..self.players.len())
            .filter(|&v| v != owner && self.players[v].anim != anim::DEAD)
            .filter_map(|v| {
                let f = &self.players[v];
                let crouched = matches!(
                    f.body.state,
                    movement::state::CROUCH | movement::state::CROUCH_WALK
                );
                let at = self.seen_pos(seen, owner, v);
                let drop = if crouched {
                    projectile::CROUCH_DROP
                } else {
                    0.0
                };
                hits_body(from, dir, len, at, f.hitbox(), crouched)
                    .map(|d| (v, d, from[1] + dir[1] * d - at[1] >= BULLET_HEAD - drop))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(v, _, head)| (v, head))
    }

    /// `func_15081E78` for an instant ray or an aimed shot: the first player whose head or torso
    /// ([`projectile::hits_parts`]) the segment meets, how far along, and whether it is the head
    /// (type 0x20, it kills; C) or the body (the weapon's own type; V combat4_pitch24). A raptor
    /// has no head part: every bolt and knife on one hits for 1 (V raptor b1r: 103 of 103; the
    /// remake's head test made 9 of the raptor bots' 21 deaths one-shot kills).
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
                let raptor = self.players[v].class == raptor::CLASS;
                projectile::hits_parts(from, dir, len, at, crouched)
                    .map(|(d, h)| (v, d, h && !raptor))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Scene 0x34's traps at the frame's start (each player's trigger loop before its move,
    /// `func_150A11C4` → `func_1515F270`): a player inside a trap that is on takes its hit,
    /// credited to the trap's owner; a direct hit, refused while immune or stunned, at the
    /// victim's own facing (C; V trap34 and elec34: the first touch kills, the presser credited).
    fn trap_hits(&mut self, ev: &mut Vec<Event>) {
        let on: Vec<usize> = (0..self.traps.list.len())
            .filter(|&k| self.traps.hits(k))
            .collect();
        for k in on {
            let t = self.traps.list[k];
            let owner = self.traps.owner[k].filter(|o| !self.free.contains(o));
            let kind = match t.electric {
                true => damage::TRAP_SPARK,
                false => damage::TRAP_FIRE,
            };
            for v in 0..self.players.len() {
                let (f, b) = (&self.players[v], &self.players[v].body);
                if self.free.contains(&v) || f.out > 0 || !t.contains(b.pos, b.floor_y) {
                    continue;
                }
                let facing = b.facing;
                self.hurt(v, &kind, false, true, facing, owner, ev);
            }
        }
    }

    /// Scene 0x34's burning cones (floor kind 0xE): a living player on one at its move's end
    /// takes the cone's hit, unguarded and unowned, and catches fire (`func_1504CB98`'s end with
    /// obj+0x28 = 0, C; V cone34: hit in its first frame there, burn ticks with no attacker).
    fn cones(&mut self, ev: &mut Vec<Event>) {
        for v in 0..self.players.len() {
            let (f, b) = (&self.players[v], &self.players[v].body);
            let on = b
                .floor
                .is_some_and(|fl| fl.surface & 0x1F == 0xE && fl.y == b.pos[1]);
            if self.free.contains(&v) || f.out > 0 || f.health == 0 || b.airborne || !on {
                continue;
            }
            let facing = b.facing;
            if self.hurt(v, &damage::CONE, false, false, facing, None, ev) {
                self.players[v].burn = Some(Burn::new(None));
            }
        }
    }

    /// Scene 0x34's plates at the frame's end: the props' update of the next frame, with the
    /// contacts this frame's floor queries set (the lowest slot on each, `traps::CONTACT`). A
    /// plate that moves takes its riders with it (`func_1511473C`), on the collision the next
    /// frame's `traps::seat` moves; one reaching the bottom switches its traps on.
    fn plates(&mut self, world: &CollisionWorld) {
        let on = |f: &Fighter| {
            (f.body.floor)
                .filter(|fl| f.body.pos[1] - fl.y <= traps::CONTACT)
                .and_then(|fl| world.placement(fl.tri))
        };
        let stood = traps::PLATES.map(|n| {
            (0..self.players.len())
                .find(|&i| !self.free.contains(&i) && on(&self.players[i]) == Some(n))
        });
        let moved = self.traps.step(stood);
        for (n, dy) in traps::PLATES.into_iter().zip(moved).filter(|m| m.1 != 0) {
            let dy = f32::from(dy);
            for f in &mut self.players {
                let b = &mut f.body;
                if let Some(fl) = b
                    .floor
                    .as_mut()
                    .filter(|fl| world.placement(fl.tri) == Some(n))
                    && !b.airborne
                {
                    fl.y += dy;
                    (b.pos[1], b.floor_y) = (b.pos[1] + dy, b.floor_y + dy);
                }
            }
        }
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
            self.hurt(v, &flame::BURN, false, true, 0, b.attacker, ev);
        }
    }

    /// Rockets fly after the players (those fired this frame included): a body in this frame's
    /// reach or the wall at the end of their life sets them off.
    fn fly_rockets(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        // Scene 0x2B's rockets (kind 3, `func_15070144`, C; V beach-gunner r3: 0x26 took a villager
        // 6 → 3) blow up with type 0x26 instead of 0x15 (`func_151C1654`).
        let blasts = match self.settings.beach {
            true => [
                ROCKET_BLASTS[0],
                (damage::ROCKET_OUTER_2B, damage::ROCKET_OUTER_R),
            ],
            false => ROCKET_BLASTS,
        };
        for (born, mut r) in std::mem::take(&mut self.rockets) {
            if let Some((_, d)) = self.first_ellipsoid(r.owner, r.pos, r.dir, r.reach(), seen) {
                self.blast(r.at(d), r.owner, blasts, world, ev);
            } else if r.advance() {
                self.rockets.push((born, r));
            } else {
                self.blast(r.at(r.reach()), r.owner, blasts, world, ev);
            }
        }
    }

    /// The frame a bolt or an aimed shot fired in frame `born` first moves: the first after it an
    /// even number of frames from [`Self::live`]. The game's object pass `func_150619A8` activates
    /// a new object (obj+0x2FA) on even reads, and its update moves it from the next frame on (V
    /// combat5_cb2-4, cb_e1: 15 bolts fired on odd and even frames, all first moving on odd reads;
    /// e4 cbgate: +0x2FA set at 142 for a bolt of 141, its first move at 143).
    pub fn first_move(&self, born: u32) -> u32 {
        born + 1 + (born + 1).wrapping_sub(self.live) % 2
    }

    /// Aimed shots and bolts sit until [`Self::first_move`], then move every frame.
    fn fly_shots(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        let t = self.tick;
        for (born, mut s) in std::mem::take(&mut self.shots) {
            if t < self.first_move(born) {
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
    pub(crate) fn enemies(&self, a: usize, v: usize) -> bool {
        self.players[a].team != self.players[v].team
    }

    /// `func_1505B9C4` on player `v` (guards of `func_1505D024` for direct hits, `func_1505C7D8`'s
    /// health and immunity tests for area hits); `angle` is the source's move angle, or the
    /// direction away from it for types with flag 0x10. Returns whether it took the hit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hurt(
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
        let out = f.health == 0 || self.waiting.contains(&v);
        if out || ((guarded || area) && f.immune != 0) || (guarded && f.stun != 0) {
            return false;
        }
        let damage = kind.amount(area);
        f.health = f.health.saturating_sub(damage);
        let lethal = f.health == 0;
        // A gunner stays seated through a hit unless it dies (0x17E, falling off) or the hit throws
        // it up faster than 30 (`func_1505B9C4`, C; V e2); one jumping onto a gun comes off (I).
        let seated = f.seated();
        let stays = seated && !lethal && kind.y_vel <= 30.0;
        let reaction = match () {
            _ if seated && lethal => 0x2E,
            _ if area && kind.reaction == 0x21 => damage::AREA_STAGGER,
            _ => kind.reaction,
        };
        if !stays {
            f.gun = None;
            let b = &mut f.body;
            if kind.aims() {
                b.move_angle = (u16::from(kind.angle) << 8).wrapping_add(angle);
            }
            (b.xz_speed, b.y_vel, b.gravity, b.airborne) = (
                kind.xz,
                kind.y_vel,
                kind.gravity,
                kind.y_vel > 0.0 || seated,
            );
            if b.airborne {
                b.floor = None;
            }
        }
        f.knock_k = kind.k;
        f.fidget = Fidget::default();
        if let Some(n) = kind.immune_frames() {
            f.immune = n;
        }
        f.stun = STUN_HIT;
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
                // War: Raptor: every kill is +1 for the killer's team (the rules' bit 0x200, C;
                // V b1, b2), latched with the frame's other step scores.
                let team = usize::from(a.team);
                if self.settings.raptor
                    && let Some(s) = self.colors.scores.get_mut(team)
                {
                    *s = s.saturating_add(1);
                }
            } else if lethal && reaction != 0x2D {
                a.suicides += 1;
            }
        }
        let f = &mut self.players[v];
        if lethal {
            (f.carrying, f.masked) = (false, false);
            f.deaths = f.deaths.saturating_add(1);
            if reaction == 0x2C {
                self.villager_down(v);
            }
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
        (z, b): (bool, bool),
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
        // the floor, which every airborne frame takes); the flame's hit until one past its 5 frames
        // that bounces less than the first landing's most (V e_w15: the flame's hop of 5 down at
        // 831, over at 833; V cone34, cone34k: 0x69 through the cone's hop and its first bounce of
        // 28, over at 113, the frame after the next landing's, I: one frame early, at 112).
        let landed = f.body.floor.is_some();
        let over = match (reaction, lethal, step) {
            (0x2B, false, 0) => landed,
            (0x10, _, 0) => {
                landed && f.body.y_vel < BLAST_BOUNCE && t - f.since >= u32::from(seq[0].1)
            }
            _ => t - f.since >= u32::from(seq[usize::from(step)].1),
        };
        if over {
            let step = step + 1;
            let dead = matches!(reaction, 0x2C | damage::GATE | damage::CONE_DEATH);
            if lethal && step == 1 && !dead {
                self.villager_down(i);
            }
            let f = &mut self.players[i];
            if usize::from(step) == seq.len() {
                if (reaction, lethal) == (0x47, false) {
                    // Beach's strike: its blast (I: no one else is hurt).
                    self.hurt(i, &damage::DETONATION, false, false, 0, None, ev);
                } else if lethal {
                    self.down(i, arena, ev);
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
            // A death that starts grounded on a kind 0xE or 0x13 floor plays 0x2D8, not the fall
            // (`func_1507CD64(obj, 0xD)` from 0x1504D0D0, C; rare: the cone's hop carries its
            // victim off, V cone34k).
            let cone = (f.body.floor).is_some_and(|fl| matches!(fl.surface & 0x1F, 0xE | 0x13));
            if f.anim == anim::DEATH_FALL && cone && !f.body.airborne {
                let dead = Action::Hurt {
                    reaction: damage::CONE_DEATH,
                    lethal: true,
                    step: 0,
                };
                f.set(dead, damage::CONE_DEATH_CLIP, t);
            }
        }
        // Hit on its seat, a gunner stays there, its trigger working (`func_15065A5C`'s case 0x1B
        // runs through the hit states; V h1-h3). A kill gate's dead lie still.
        let f = &mut self.players[i];
        if reaction == damage::GATE {
            return;
        }
        match f.gun {
            Some(mut m) => {
                m.step(z, b);
                let yaw = f.body.facing;
                let seat = gun::seat(m.at, yaw, gun::pitch10(f.scope_pitch));
                movement::sit(&mut f.body, seat, yaw);
                f.gun = Some(m);
            }
            None => knock(f, reaction, step, world, others),
        }
    }

    /// A death's end: on its last life (Deathmatch, Total War) it stays down, out of play, on the
    /// last place of those still in play (the game counts places down from its participants,
    /// `0x800E0AA9`); else it respawns.
    pub(crate) fn down(&mut self, i: usize, arena: &Arena, ev: &mut Vec<Event>) {
        let lives = !self.settings.colors && !self.settings.beach;
        if lives && self.lives_left(i) == 0 {
            let place = (0..self.players.len()).filter(|&j| self.in_play(j)).count();
            (self.players[i].out, self.players[i].out_at) = (place as u8, self.tick);
        } else {
            self.respawn(i, arena, ev);
        }
    }

    /// A kill gate's crossing (`func_150A11C4` → `func_15055C88` → `func_1507CD64(p, 9)`, C;
    /// `crate::lift`), the living and the dead alike: a death with no killer, and on Beach a
    /// villager's point for the Tediz at once (V b0 2231 → 2232: bots1's point with no kill). A
    /// corpse whose point is still to come scores it now, and only now (V b0 7560 → 7562). A bot is
    /// back on its record the same frame (V b0-b8: the next read shows it there with hp 6); a human
    /// lies still where it crossed, its clip kept, for [`damage::GATE_FRAMES`] (V fall6). Returns
    /// where a bot went. In Deathmatch only the death counts: no kill and no +0x70 for anyone (V: a
    /// drop through 0x2D's 0x4E, scenario 3: `0x800E0AD0` +1, gGameState unchanged, back in 8).
    pub fn fall_kill(&mut self, i: usize, arena: &Arena) -> Option<Placement> {
        let t = self.tick;
        let f = &self.players[i];
        let pending = matches!(f.action, Action::Hurt { reaction, lethal: true, step: 0 } if reaction != 0x2C);
        if f.health > 0 || pending {
            self.villager_down(i);
        }
        let f = &mut self.players[i];
        if f.health > 0 {
            f.health = 0;
            f.deaths = f.deaths.saturating_add(1);
            (f.weapon, f.drawn) = (None, false);
            (f.carrying, f.masked) = (false, false);
        }
        if f.bot {
            let mut ev = Vec::new();
            self.down(i, arena, &mut ev);
            return ev.into_iter().find_map(|e| match e {
                Event::Respawn { at, .. } => Some(at),
                _ => None,
            });
        }
        let anim = f.anim;
        f.set(
            Action::Hurt {
                reaction: damage::GATE,
                lethal: true,
                step: 0,
            },
            anim,
            t,
        );
        (f.stun, f.immune) = (STUN_DEAD, 255);
        (f.body.y_vel, f.body.xz_speed) = (0.0, 0.0);
        None
    }

    /// Beach: a villager's death registers (`func_1507CD64` with the rules' bit 0x20, C): a point
    /// for the Tediz. That is where the lethal sequence's first state ends, or at once for a
    /// reaction 0x2C's (V bots1: 35 frames after hp 0 for the mounted gun's 0x31, 84-90 for a
    /// blast, at once for the strike's and the detonation's; I: the hook).
    fn villager_down(&mut self, i: usize) {
        if self.settings.beach && self.players[i].team == beach::VILLAGERS {
            let s = &mut self.colors.scores[usize::from(beach::TEDIZ)];
            *s = s.saturating_add(1);
        }
    }

    fn respawn(&mut self, i: usize, arena: &Arena, ev: &mut Vec<Event>) {
        self.let_go(i);
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
        let (hits, headshots, gassed, extra, bags) =
            (f.hits, f.headshots, f.gassed, f.extra, f.bags);
        let (buttons, late, side, spawn_mask, start) =
            (f.buttons, f.late, f.side, f.spawn_mask, f.start);
        // Beach re-rolls a villager's class (I: after the pick's draws); the boost goes (V wz).
        let class = match self.settings.beach {
            true => beach::reroll(f.class, &mut self.rng),
            false => f.class,
        };
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
            gassed,
            extra,
            class,
            bags,
            last_spawn: Some(at.index),
            ..Fighter::new(&at, team, self.tick)
        };
        ev.push(Event::Respawn { player: i, at });
    }

    /// The mounted guns, after the players (their objects' slots follow; `func_150BDB70`, C): a free
    /// gun launches the first player in slot order that touches it; a manned one turns with its
    /// gunner and fires ([`Self::fire_gun`]).
    fn mounted_guns(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        let t = self.tick;
        for k in 0..self.guns.len() {
            let gunner = self.gunner(k);
            let g = &mut self.guns[k];
            let Some(i) = gunner else {
                g.pitch10 -= g.pitch10 >> 3;
                // The touch (`func_150BDB70` → `func_1505327C`, C): alive, unstunned, free to act,
                // not in the scope, carrying nothing (I: the actions that hold a player stand in for
                // obj+0x89, as for the flags).
                let at = g.place.pos;
                let toucher = self.players.iter().position(|f| {
                    f.health > 0
                        && f.stun == 0
                        && f.gun.is_none()
                        && f.body.scope_yaw.is_none()
                        && f.flag.is_none()
                        && !f.carrying
                        && f.body.lock == 0
                        && matches!(
                            f.action,
                            Action::Free
                                | Action::Fire { .. }
                                | Action::Aim { .. }
                                | Action::Reload { .. }
                        )
                        && gun::touches(at, &f.other())
                });
                if let (Some(i), 0) = (toucher, g.leave) {
                    // The jump keeps the speed the player came with, along the gun's yaw, and homes
                    // on the seat as last drawn (joint 3's matrix, C `func_1505327C`; V t1 132-156).
                    // b30 4896-4916 and m2 62-82 home on the gun's origin: the oracle had not drawn
                    // that gun yet, its matrices still zero. The barrels keep their turn (I).
                    let (yaw, seat) = (g.yaw, gun::seat(at, g.yaw, g.pitch10));
                    let barrel = if g.shots > 0 { g.last ^ 1 } else { 0 };
                    let mount = Mount {
                        to: [seat[0], seat[2]],
                        barrel,
                        rng: u64::from(self.rng.next_u32()).max(1),
                        ..Mount::new(k, at)
                    };
                    let f = &mut self.players[i];
                    (f.gun, f.drawn, f.immune) = (Some(mount), false, 255);
                    f.set(Action::Free, id::JUMP, t);
                    let b = &mut f.body;
                    (b.state, b.airborne, b.floor) = (movement::state::JUMP, true, None);
                    (b.y_vel, b.gravity) = (gun::LAUNCH_Y_VEL, gun::LAUNCH_GRAVITY);
                    (b.target_speed, b.move_angle) = (0.0, yaw);
                }
                continue;
            };
            let f = &self.players[i];
            if !f.seated() {
                continue;
            }
            (g.yaw, g.pitch10) = (f.body.facing, gun::pitch10(f.scope_pitch));
            if let Some(b) = self.fire_gun(i, world, seen, ev) {
                let g = &mut self.guns[k];
                (g.shots, g.last) = (g.shots.wrapping_add(1), b);
            }
        }
    }

    /// Player `i`'s mounted gun's fire this frame, after the players (`func_150FC614`, C; V e1, and
    /// h1: a hurt gunner's too): [`Mount::fire`]'s shot, if any, and its barrel. The server's
    /// [`Self::step`] and the gunner's client ([`Self::step_player`]) both run it. A human's shot
    /// heads from the muzzle for the first body part or wall the scope's ray meets past
    /// [`gun::sight`], else leaves from there along the ray (`func_151C229C` → `func_151C4B0C`, the
    /// twin guns' aim cast; C). V e1: the first shot meets player 2's body 146 along the ray and
    /// knocks it at 210.4°, the next ones the floor 334 along (from the eye as the fire clip lifts
    /// it, 5 up: not modelled, I). A bot's goes along the barrel ([`gun::barrel_dir`]).
    pub fn fire_gun(
        &mut self,
        i: usize,
        world: &CollisionWorld,
        seen: Seen,
        ev: &mut Vec<Event>,
    ) -> Option<u8> {
        let f = &mut self.players[i];
        let mut m = f.gun.filter(|_| f.seated())?;
        let b = m.fire();
        f.gun = Some(m);
        let b = b?;
        let f = &self.players[i];
        let (at, yaw) = (m.at, f.body.facing);
        let pitch = (f.scope_pitch + 180.0).rem_euclid(360.0) - 180.0;
        let muzzle = gun::muzzle(at, yaw, pitch, b);
        let (from, dir) = if f.bot {
            let toe = 1.0 + 6.0 * self.rng.next_f32();
            (muzzle, gun::barrel_dir(yaw, pitch, b, toe))
        } else {
            let (d, _) = aim(f);
            let p = gun::sight(at, yaw, pitch, b, conker_camera::gun_eye(at, yaw, pitch), d);
            let wall = raycast(world, p, d, RAY_RANGE);
            let hit = (self.first_part(i, p, d, wall, seen)).map_or(wall, |h| h.1);
            let v: [f32; 3] = core::array::from_fn(|j| p[j] + d[j] * hit - muzzle[j]);
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            match hit < RAY_RANGE && n > 0.0 {
                true => (muzzle, v.map(|c| c / n)),
                false => (p, d),
            }
        };
        let bullet = Bullet::aimed(world, i, from, dir, gun::SPEED, damage::MG_BULLET);
        ev.push(Event::Shot {
            player: i,
            pos: muzzle,
        });
        self.bullets.push((self.tick, bullet));
        // s126+0x1AA counts them (C).
        let f = &mut self.players[i];
        f.shots = f.shots.saturating_add(1);
        Some(b)
    }

    fn fly_bullets(&mut self, seen: Seen, ev: &mut Vec<Event>) {
        for (born, mut b) in std::mem::take(&mut self.bullets) {
            // A body stops the bullet even when the hit is refused (dead, stunned or immune).
            if let Some((v, head)) = self.first_body(b.owner, b.pos, b.dir, b.reach(), seen) {
                // `func_151D4DAC`: a hit on the head (part 0xA) takes 1 health at once, stunned
                // or not, and below 3 kills with type 0x20, which no stun or immunity refuses (C;
                // V ohit: stunned hits at +55 take 4 → 3 → 2 → dead, at +40 nothing). Bullets that
                // test body parts have a head (`func_151C229C`'s flag 8): the twin guns' and the
                // pistol's (its spawner `func_150FED30` sets it, C; V e_w39: a bullet pops the
                // corpse's head at read 970); not 0x23's, the tommy gun's (I: `func_150FE860`) or,
                // in multiplayer, the mounted gun's (C; V e1: its hit takes 2).
                // ponytail: the pistol's head stays off: [`BULLET_HEAD`]'s height (fitted to the
                // twin guns') would make e_w39's level body hits at +48.6 (2 each, V) head hits;
                // on with the model's head part (part 0xA) instead of a height.
                let f = &mut self.players[v];
                let head = head && b.kind == damage::TWIN_BULLET;
                if head && f.health > 0 && !self.waiting.contains(&v) {
                    if f.health >= 3 {
                        f.health -= 1;
                    } else {
                        let (o, k) = (b.owner, damage::SNIPER_RAY);
                        if self.hurt(v, &k, false, false, b.angle, Some(o), ev)
                            && self.enemies(o, v)
                        {
                            self.players[o].headshots = self.players[o].headshots.saturating_add(1);
                        }
                        continue;
                    }
                }
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
                if !in_reach(pos, r, p, self.players[v].hitbox()) || !clear(world, pos, p) {
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
    let me = f.other();
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
    movement::push_apart(&mut new, me, others);
    let swept = world.sweep(
        old,
        new,
        &Body::player(old[1], new[1]),
        &Sweep::default(),
        -9.0,
    );
    b.pos = swept.pos;
    let prev = world.last_floor(old, b.floor_y, b.floor);
    if let Some(fl) = world.floor(b.pos[0], b.pos[2], old[1], old[1], prev) {
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
    let id = characters::clip(f.class, f.anim);
    let r = conker_anim::request(id, f.body.xz_speed, f.body.target_speed);
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
    let me = f.other();
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
        movement::push_apart(&mut new, me, others);
        b.y_vel -= b.gravity;
        let swept = world.sweep(
            old,
            new,
            &Body::player(old[1], new[1]),
            &Sweep::default(),
            b.y_vel,
        );
        (b.pos, b.y_vel) = (swept.pos, swept.y_vel);
        let floor = world.floor(b.pos[0], b.pos[2], b.pos[1], old[1], None);
        if let Some(fl) = floor
            && b.y_vel >= 0.0
            && b.pos[1] <= fl.y
        {
            // Rising into a slope: onto it, still flying (I: the hurt movement's rise is unread; no
            // oracle body is ever more than 23 below its floor, V bots-beach-club §1.5). Without it
            // the body sinks below the floor's 50 reach and falls forever.
            (b.pos[1], b.floor_y) = (fl.y, fl.y);
        } else if let Some(fl) = floor
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
        let shoved = movement::push_apart(&mut fell, me, others);
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
                world.last_floor(old, b.floor_y, b.floor)
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
            ..Default::default()
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
            (m.players[1].side, m.players[1].class) = (Side::Squirrels, 4);
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

    /// A killed player holds its pickup's weapon 3 frames past the killing hit, then the start
    /// item, holstered, or none (V e4 dth: the twin guns drawn through 790-792, gone at 793); a
    /// start item drawn stays drawn (V combat6_e40, the club).
    #[test]
    fn a_killed_player_keeps_its_weapon_three_frames() {
        for (start, then) in [(None, (None, false)), (Some(0x40), (Some(0x40), false))] {
            let (arena, mut m, w) = duel();
            m.give(1, 0x09);
            (m.players[1].drawn, m.players[1].start, m.players[1].health) = (true, start, 1);
            assert!(m.hurt(
                1,
                &damage::GUN23_BULLET,
                false,
                false,
                0,
                Some(0),
                &mut Vec::new()
            ));
            let mut held = Vec::new();
            for _ in 0..6 {
                run(&mut m, &arena, &w, 1, |_| 0);
                held.push((m.players[1].weapon, m.players[1].drawn));
            }
            let want = [(Some(0x09), true); 3].into_iter().chain([then; 3]);
            assert_eq!(held, want.collect::<Vec<_>>(), "{start:?}");
        }
        let (arena, mut m, w) = duel();
        m.give(1, 0x40);
        (m.players[1].drawn, m.players[1].start, m.players[1].health) = (true, Some(0x40), 1);
        assert!(m.hurt(
            1,
            &damage::GUN23_BULLET,
            false,
            false,
            0,
            Some(0),
            &mut Vec::new()
        ));
        run(&mut m, &arena, &w, 60, |_| 0);
        assert_eq!(
            (m.players[1].weapon, m.players[1].drawn),
            (Some(0x40), true)
        );
    }

    /// C-down with the twin guns or the flamethrower drawn pulls no grenade: it steps back with the
    /// C strafe (the move angle swings 6400 a frame toward the back), the weapon's run while held
    /// (V e4 cd09 280-321: 0x22C, 16896 → 10496 → 4096 → 63232; cd15: 0xED). Holstered, it pulls.
    #[test]
    fn c_down_with_a_weapon_drawn_steps_back() {
        for (id, run_anim) in [(0x09, anim::TWIN_RUN), (0x15, anim::FLAME_WALK)] {
            let (arena, mut m, w) = duel();
            m.give(0, id);
            m.players[0].body.facing = 16896;
            m.players[0].body.move_angle = 16896;
            run(&mut m, &arena, &w, 1, |_| BUTTON_B);
            run(&mut m, &arena, &w, 40, |_| 0);
            assert!(m.players[0].drawn);
            let mut angles = Vec::new();
            for _ in 0..10 {
                run(&mut m, &arena, &w, 1, |_| C_DOWN);
                let f = m.players[0];
                assert_eq!((f.action, f.anim, f.grenades), (Action::Free, run_anim, 3));
                angles.push(f.body.move_angle);
            }
            assert_eq!(angles[..3], [10496, 4096, 63232], "{id:#x}");
            assert_eq!(angles[9], 16896u16.wrapping_add(0x8000));
            // Holstered (B, then the holster's frames), C-down pulls.
            run(&mut m, &arena, &w, 1, |_| BUTTON_B);
            run(&mut m, &arena, &w, 40, |_| 0);
            run(&mut m, &arena, &w, 3, |_| C_DOWN);
            assert!(matches!(m.players[0].action, Action::GrenadePull { .. }));
        }
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

    /// A death that starts grounded on a cone's floor (kind 0xE) plays 0x2D8, anywhere else the
    /// fall (`func_1507CD64`'s cause 0xD from 0x1504D0D0, C).
    #[test]
    fn a_death_on_a_cone_plays_0x2d8() {
        for (surface, clip) in [(0xE, damage::CONE_DEATH_CLIP), (5, anim::DEATH_FALL)] {
            let (arena, mut m, _) = duel();
            let big = 20_000.0;
            let w = CollisionWorld::new([
                (
                    [[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]],
                    surface,
                ),
                (
                    [[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]],
                    surface,
                ),
            ]);
            let dying = Action::Hurt {
                reaction: 0x22,
                lethal: true,
                step: 0,
            };
            let f = &mut m.players[1];
            f.health = 0;
            f.set(dying, anim::HIT, 0);
            f.body.floor = w.floor(146.5, 0.0, 0.0, 0.0, None);
            let mut anims = Vec::new();
            for _ in 0..30 {
                run(&mut m, &arena, &w, 1, |_| 0);
                anims.push(m.players[1].anim);
            }
            assert!(anims.contains(&clip), "{surface:#x}: {anims:x?}");
            assert_eq!(
                anims.contains(&anim::DEATH_FALL),
                surface == 5,
                "{anims:x?}"
            );
        }
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

    /// A hip bolt pushes its shooter while it sits at the muzzle: fired from Z at 131 (born 141, odd
    /// to the live frame) it first moves at 143, and the shooter moves (1.843, 2.152) in 142 and
    /// (1.577, 1.841) in 143 (V combat5_cb3 rows 143-144, e4 cbp2: −2247, 6015 facing 16896 to
    /// −2243.580, 6018.993); a bolt born on an even frame moves the next and pushes once.
    #[test]
    fn a_hip_bolt_pushes_its_shooter() {
        let (arena, mut m, w) = duel();
        m.give(0, 0x18);
        let b = &mut m.players[0].body;
        (b.facing, b.move_angle) = (16896, 16896);
        run(&mut m, &arena, &w, 131, |t| u16::from(t == 0) * BUTTON_B);
        let mut at = Vec::new();
        for _ in 0..5 {
            run(&mut m, &arena, &w, 1, |t| u16::from(t == 131) * BUTTON_Z);
            at.push(m.players[0].body.pos);
        }
        // The rows after frames 131..=135; then 141..=144.
        assert!(at.iter().all(|p| *p == [0.0, 0.0, 0.0]), "{at:?}");
        run(&mut m, &arena, &w, 5, |_| 0);
        let mut at = Vec::new();
        for _ in 0..4 {
            run(&mut m, &arena, &w, 1, |_| 0);
            at.push(m.players[0].body.pos);
        }
        let near = |p: [f32; 3], x: f32, z: f32| (p[0] - x).abs() < 1e-3 && (p[2] - z).abs() < 1e-3;
        assert!(near(at[0], 0.0, 0.0), "{at:?}");
        assert!(near(at[1], 1.843, 2.152), "{at:?}");
        assert!(near(at[2], 1.843 + 1.577, 2.152 + 1.841), "{at:?}");
        assert_eq!((at[2], at[3][1]), (at[3], 0.0));
        // Born on an even frame (live moved by one): a single push.
        assert_eq!((m.first_move(141), m.first_move(142)), (143, 143));
        m.live = 0;
        assert_eq!((m.first_move(141), m.first_move(142)), (142, 144));
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

    /// A body knocked up a slope rising 0.45 a unit (0x2B's (837, −107, 1852) → (888, 2.1, 2090))
    /// rides onto it instead of sinking under it, and comes down on it (the mounted gun's 0x31; V
    /// bound: no oracle body is more than 23 below its floor; bots-beach-club §1.5, history B).
    #[test]
    fn a_knock_up_a_slope_stays_on_it() {
        let w = CollisionWorld::new([
            (
                [
                    [-500.0, -900.0, -2000.0],
                    [-500.0, 1800.0, 4000.0],
                    [500.0, 1800.0, 4000.0],
                ],
                5,
            ),
            (
                [
                    [-500.0, -900.0, -2000.0],
                    [500.0, 1800.0, 4000.0],
                    [500.0, -900.0, -2000.0],
                ],
                5,
            ),
        ]);
        let (_, mut m, _) = duel();
        let f = &mut m.players[1];
        let b = &mut f.body;
        (b.pos, b.airborne, b.y_vel, b.gravity) = ([0.0, 0.0, 0.0], true, 12.0, 3.4);
        (b.xz_speed, b.move_angle, b.floor) = (30.0, 0xC000, None);
        for _ in 0..60 {
            knock(f, 0x31, 1, &w, &[]);
            let b = &f.body;
            let fl = w.floor(b.pos[0], b.pos[2], 1e4, 1e4, None).unwrap().y;
            assert!(b.pos[1] >= fl - 1.0, "{:?} under {fl}", b.pos);
            if !b.airborne {
                break;
            }
        }
        assert!(
            !f.body.airborne && f.body.pos[2] < 100.0,
            "{:?}",
            f.body.pos
        );
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

    /// War: Beach's bazooka Tedi (V beach-gunner r1, a bots1 replay): its rocket leaves along its aim
    /// (aim 255.086°, 6.921° down → (−0.255493, −0.120507, 0.959271) at read 411), not level along
    /// its facing, so it reaches the villagers 3,000 away and 1,000 below the east top; pressing Z
    /// it fires every 60 frames (s126+0x19E = 120 VIs; r1 349, 411, 473), playing 0x207, and its
    /// outer blast takes 3.
    #[test]
    fn a_bots_rocket_follows_its_aim() {
        let (arena, mut m, w) = duel();
        m.settings.beach = true;
        m.give(0, 0x16);
        let mut born = Vec::new();
        for t in 0..200 {
            (m.players[0].body.scope_yaw, m.players[0].scope_pitch) = (Some(255.08606), 6.921_387);
            let buttons = match t {
                0 => BUTTON_B,
                50.. if t % 2 == 0 => BUTTON_Z,
                _ => 0,
            };
            let inputs = [
                Input {
                    buttons,
                    ..Input::default()
                },
                Input::default(),
            ];
            m.step(&arena, &inputs, &w);
            born.extend(m.rockets.iter().filter(|r| r.0 == t).map(|r| (t, r.1.dir)));
            if born.last().is_some_and(|b| b.0 == t) {
                assert_eq!(m.players[0].anim, beach::ROCKET_FIRE);
            }
        }
        let at: Vec<u32> = born.iter().map(|b| b.0).collect();
        assert_eq!(at, [50, 110, 170]);
        let want = [-0.255_492_75, -0.120_507_4, 0.959_271_3];
        assert!(
            born.iter()
                .all(|b| b.1.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-4))
        );
        // On 0x2B its outer blast is type 0x26, 3 damage (V beach-gunner r3: a villager 6 → 3).
        m.rockets = vec![(
            m.tick,
            Rocket::fire(&w, 0, [146.5, 10.0, 150.0], [0.0, -1.0, 0.0]),
        )];
        m.step(&arena, &[Input::default(); 2], &w);
        assert_eq!(m.players[1].health, 3);
    }

    /// A bot's twin-gun bullets head for what its aim ray meets (`func_151D5174`, C): a target on a
    /// ledge 415 up and 3,600 away (2D seed 5's stand) is hit. Without the ray they fly level along
    /// the facing and pass under the ledge. The pistol's and 0x23's take the ray's pitch on the
    /// facing (`func_15145740`), which also reaches it.
    #[test]
    fn a_bots_bullets_follow_its_aim_ray() {
        let (y, a, b, big) = (415.0, 3400.0, 3800.0, 20_000.0);
        let quad = |y: f32, a: f32, b: f32, c: f32, d: f32| {
            [
                ([[a, y, c], [a, y, d], [b, y, d]], 5),
                ([[a, y, c], [b, y, d], [b, y, c]], 5),
            ]
        };
        let shoot = |weapon: u8, aimed: bool| {
            let (arena, mut m, _) = duel();
            let w = CollisionWorld::new(
                quad(0.0, -big, big, -big, big)
                    .into_iter()
                    .chain(quad(y, a, b, -200.0, 200.0)),
            );
            m.players[1].body.pos = [3600.0, 415.0, 0.0];
            m.give(0, weapon);
            let (mut first, mut hit) = (None, None);
            for t in 0..120 {
                let eye = [0.0f32, 67.5, 0.0];
                let d = [3600.0, 415.0 + 40.0 - eye[1], 0.0];
                let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
                m.aims = vec![aimed.then_some((eye, d.map(|c| c / n))), None];
                let buttons = match t {
                    0 => BUTTON_B,
                    20.. if weapon == 0x09 || t % 4 == 0 => BUTTON_Z,
                    _ => 0,
                };
                let inputs = [
                    Input {
                        buttons,
                        ..Input::default()
                    },
                    Input::default(),
                ];
                let ev = m.step(&arena, &inputs, &w);
                first = first.or(m.bullets.first().map(|b| b.1.dir));
                hit = hit.or(ev
                    .iter()
                    .any(|e| matches!(e, Event::Hit { victim: 1, .. }))
                    .then_some(t));
            }
            (first.expect("a bullet"), hit)
        };
        let (dir, hit) = shoot(0x09, true);
        assert!(dir[1] > 0.1 && hit.is_some(), "{dir:?} {hit:?}");
        let (dir, hit) = shoot(0x09, false);
        assert!(dir[1] == 0.0 && hit.is_none(), "{dir:?} {hit:?}");
        let pitch = 387.5 / (3600.0f32 * 3600.0 + 387.5 * 387.5).sqrt();
        for weapon in [0x39, 0x23] {
            let (dir, hit) = shoot(weapon, true);
            assert!((dir[1] - pitch).abs() < 1e-4, "{weapon:#x} {dir:?}");
            assert!(
                dir[2].abs() < 1e-4 && hit.is_some(),
                "{weapon:#x} {dir:?} {hit:?}"
            );
        }
    }

    /// `func_151D4DAC`'s head hits, V oracle ohit (0x30): a stunned player at 4 hit on the head
    /// (+55, +60, +61) goes 3, 2, then dead (1328-1333); body hits on it at +40 do nothing; an
    /// unstunned head hit takes 2 (2379: 3 → 1). The pistol's bullets have none here (see
    /// `fly_bullets`: V e_w39's body hits at +48.6).
    #[test]
    fn a_bullet_on_the_head_hurts_through_the_stun() {
        let shot = |m: &mut Match, arena: &Arena, w: &CollisionWorld, h: f32| {
            let kind = if h == 60.5 {
                damage::PISTOL_BULLET
            } else {
                damage::TWIN_BULLET
            };
            let b = Bullet::fire(w, 0, [50.0, h, 0.0], 0, kind);
            m.bullets = vec![(m.tick, b)];
            m.step(arena, &[Input::default(); 2], w);
            (m.players[1].health, m.players[1].stun)
        };
        let (arena, mut m, w) = duel();
        m.players[1].health = 5;
        assert_eq!(shot(&mut m, &arena, &w, 40.0), (4, STUN_HIT));
        assert_eq!(shot(&mut m, &arena, &w, 40.0), (4, STUN_HIT));
        assert_eq!(shot(&mut m, &arena, &w, 55.0).0, 3);
        assert_eq!(shot(&mut m, &arena, &w, 60.0).0, 2);
        assert_eq!(shot(&mut m, &arena, &w, 61.0).0, 0);
        let reaction = 0x2E;
        assert!(
            matches!(m.players[1].action, Action::Hurt { reaction: r, lethal: true, .. } if r == reaction)
        );
        assert_eq!((m.players[0].kills, m.players[0].headshots), (1, 1));

        let (arena, mut m, w) = duel();
        m.players[1].health = 3;
        assert_eq!(shot(&mut m, &arena, &w, 55.0), (1, STUN_HIT));
        let (arena, mut m, w) = duel();
        assert_eq!(shot(&mut m, &arena, &w, 60.5), (4, STUN_HIT));
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
            (0, Rocket::fire(&w, 2, [0.0; 3], [1.0, 0.0, 0.0])),
            (0, Rocket::fire(&w, 1, [0.0; 3], [1.0, 0.0, 0.0])),
        ];
        m.pickups[0].taken = Some((2, 5));
        m.pickups[1].taken = Some((1, 5));
        m.players[0].burn = Some(Burn::new(Some(1)));
        m.players[2].burn = Some(Burn::new(Some(2)));
        m.remove_player(1);
        let owners = |v: &[(u32, Bullet)]| v.iter().map(|b| b.1.owner).collect::<Vec<_>>();
        assert_eq!(owners(&m.bullets), [0, 2]);
        assert_eq!(m.rockets.iter().map(|r| r.1.owner).collect::<Vec<_>>(), [2]);
        assert_eq!(
            (m.pickups[0].taken, m.pickups[1].taken),
            (Some((2, 5)), None)
        );
        assert_eq!(m.players[0].burn, None);
        assert_eq!(m.players[2].burn.and_then(|b| b.attacker), Some(2));
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
            ..Default::default()
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
                // The scare goes by player 0's weapon as its update this frame left it.
                solo.players[1].body.scared = m.players[1].body.scared;
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
        assert_eq!((m.timer, m.ended_at), (0, Some(2)));
        run(&mut m, &arena, &w, END_DELAY + 1, |_| 0);
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

    /// The kill limit latches the end the frame after the killing frame; the fade follows as after a
    /// time-out.
    #[test]
    fn the_kill_limit_ends_the_match() {
        let limit = |kills| Settings {
            kills,
            ..Settings::default()
        };
        assert_eq!(pistol_kill(limit(1)).1.ended_at, Some(21));
        assert_eq!(pistol_kill(limit(2)).1.ended_at, None);
        let (arena, mut m, w, _) = pistol_kill(limit(1));
        m.tick = 21 + END_DELAY + FADE;
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
        assert_eq!((m.ended_at, m.places()), (Some(172), vec![1, 2]));
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
        assert_eq!((m.players[1].out, m.ended_at), (3, Some(172)));
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
        // Humans in slots 0-1, bots in 2-3, all 0-0: the shots part P1 and P2, not the slot (V t5_b).
        assert_eq!(
            places(
                &[s(0, 0, 0, 0), s(0, 0, 5, 0), s(0, 0, 0, 0), s(0, 0, 0, 0)],
                0
            )[..2],
            [1, 4]
        );
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
                score: 0,
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
        // Colors: the score first, whatever the kills (V t1: S 1-0 beats the Tediz' 3 kills), then
        // the kills (t3: 0-0, the Tediz' 3 kills win), all equal a DRAW (t0).
        let scored = |team, kills, score| {
            Some(Standing {
                score,
                ..s(team, kills, 0, 0).unwrap()
            })
        };
        let t1 = [scored(0, 0, 1), scored(1, 3, 0), scored(1, 0, 0)];
        assert_eq!((team_places(&t1), team_result(&t1)), ([1, 2], Some(0)));
        let t3 = [scored(0, 0, 0), scored(1, 3, 0)];
        assert_eq!(team_result(&t3), Some(1));
        let t0 = [scored(0, 0, 2), scored(1, 0, 2)];
        assert_eq!(team_result(&t0), None);
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
        assert_eq!((m.ended_at, m.places()), (Some(172), vec![1, 2]));
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

    /// Total War, both teams' last members out on one frame: team 0 goes out first (the game checks
    /// it first), so the Tediz win though the squirrel fell in the later slot.
    #[test]
    fn total_war_both_out_on_one_frame() {
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            time: None,
            lives: 1,
            total_war: true,
            ..Settings::default()
        });
        (m.players[0].team, m.players[1].team) = (1, 0);
        for v in [0, 1] {
            m.hurt(
                v,
                &damage::GUN23_BULLET,
                false,
                false,
                0,
                None,
                &mut Vec::new(),
            );
        }
        run(&mut m, &arena, &w, 200, |_| 0);
        assert!(m.ended_at.is_some() && m.players.iter().all(|f| f.out > 0));
        assert_eq!(team_result(&m.standings()), Some(1));
        assert_eq!(m.places(), [1, 2]);
    }

    /// A player that joined and has not played yet (the server's `Waiting`) takes no hit, so its
    /// attacker scores nothing.
    #[test]
    fn a_waiting_player_takes_no_hit() {
        let (_, mut m, _) = duel();
        m.waiting.insert(1);
        let mut ev = Vec::new();
        assert!(!m.hurt(1, &damage::GUN23_BULLET, false, false, 0, Some(0), &mut ev));
        let (a, v) = (&m.players[0], &m.players[1]);
        assert_eq!((a.kills, a.hits, v.deaths, ev.len()), (0, 0, 0, 0));
    }

    /// Over, the script's effects do nothing (Colors' final capture respawns no one, scores no more).
    #[test]
    fn nothing_applies_once_over() {
        let (_, mut m, _) = duel();
        m.ended_at = Some(0);
        m.tick = END_DELAY + FADE + 1;
        m.apply(&[Effect::Respawn(1), Effect::Score(0, 1)]);
        assert_eq!((m.colors.respawns, m.colors.scores), (0, [0, 0]));
    }

    /// Colors: lives are never lost; the score target ends the match on the capture's frame, the
    /// clock otherwise, and the teams rank by score first; untimed with no target nothing ends it.
    /// A flag's taker grabs it, then holds it: no weapon, Z and A do nothing, 0.8 of the speed.
    #[test]
    fn colors_ends_ranks_and_carries() {
        let colors = |time, score| Settings {
            time,
            score,
            colors: true,
            ..Settings::default()
        };
        let (_, m, _, ev) = pistol_kill(colors(None, 0));
        assert_eq!(
            (m.players[1].deaths, m.players[1].out, m.ended_at),
            (1, 0, None)
        );
        assert!(
            ev.iter()
                .any(|e| matches!(e.1, Event::Respawn { player: 1, .. }))
        );
        assert_eq!(m.places(), [1, 2], "the kill, at 0-0");
        let (arena, m, w) = duel();
        let mut m = m.with(colors(Some(3), 2));
        m.apply(&[Effect::Score(1, 1)]);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!((m.ended_at, m.places()), (None, vec![2, 1]));
        m.apply(&[Effect::Score(1, 1)]);
        let t = m.tick;
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.ended_at, Some(t));
        let (arena, m, w) = duel();
        let mut m = m.with(colors(Some(3), 0));
        m.timer = 2;
        run(&mut m, &arena, &w, 1, |_| 0);
        assert!(m.ended_at.is_some());
        // The carry: player 0 (team 0) on the red flag, a weapon drawn.
        let (arena, m, w) = duel();
        let mut m = m.with(colors(None, 0));
        m.give(0, 0x39);
        run(
            &mut m,
            &arena,
            &w,
            30,
            |t| if t == 0 { BUTTON_B } else { 0 },
        );
        assert!(m.players[0].drawn);
        m.apply(&[Effect::Flag(0, true)]);
        m.colors.flags[0].pos = m.players[0].body.pos;
        m.colors.flags[1] = m.colors.flags[0];
        run(&mut m, &arena, &w, 1, |_| 0);
        let f = &m.players[0];
        assert_eq!((f.flag, f.drawn, f.anim), (Some(0), false, anim::FLAG_GRAB));
        assert_eq!(m.players[1].flag, None, "the green flag is team 0's own");
        let mut speeds = Vec::new();
        for t in 0..60 {
            let stick = Input {
                stick_y: 80,
                buttons: if t % 2 == 0 { BUTTON_Z | BUTTON_A } else { 0 },
                ..Input::default()
            };
            m.step(&arena, &[stick, Input::default()], &w);
            speeds.push(m.players[0].body.xz_speed);
        }
        let f = &m.players[0];
        assert_eq!(
            (f.shots, f.body.airborne, f.anim),
            (0, false, anim::FLAG_RUN)
        );
        assert_eq!(
            speeds[15..19],
            [0.0, 0.75, 1.5, 2.25],
            "17 frames held, then +0.75"
        );
        assert!(speeds.iter().all(|&s| s <= 28.0), "{speeds:?}");
    }

    /// Colors' flag is a body (V e0, e1 `touch.py`): it pushes a player 0.4 of the overlap a frame
    /// (e0 404: 7.1 off the falling flag) out to the radii's sum, 60 from a squirrel, 53 from a
    /// Tedi; it is taken on the overlap, both on the floor: not at 60.0, at 59.
    #[test]
    fn the_flag_touch_is_the_bodies_overlap() {
        let flag = |pos| colors::Flag {
            shown: true,
            pos,
            ..colors::Flag::default()
        };
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            colors: true,
            ..Settings::default()
        });
        m.players[1].body.pos = [0.0, 0.0, 3000.0];
        (m.players[0].side, m.players[0].class) = (Side::Squirrels, 4);
        // The green flag (team 1's to take), e0's: 31 off and 43.4 up.
        m.colors.flags[1] = flag([31.0, 43.4, -3.8]);
        run(&mut m, &arena, &w, 1, |_| 0);
        let p = m.players[0].body.pos;
        assert!(
            (p[0] + 7.1).abs() < 0.7 && (p[2] - 0.8).abs() < 0.2,
            "{p:?}"
        );
        for (class, sum) in [(3, 53.0), (4, 60.0)] {
            m.players[0].class = class;
            m.players[0].body.pos = [0.0; 3];
            m.colors.flags[1] = flag([31.0, 0.0, 0.0]);
            run(&mut m, &arena, &w, 40, |_| 0);
            let p = m.players[0].body.pos;
            assert!((p[0] + sum - 31.0).abs() < 0.06, "{class} {p:?}");
            assert_eq!(m.players[0].flag, None);
        }
        // Pushed to its rest off the red flag, team 0's to take, it takes nothing (e0).
        m.colors.flags.swap(0, 1);
        run(&mut m, &arena, &w, 40, |_| 0);
        assert_eq!(m.players[0].flag, None);
        // At 60.0 no, at 59 yes; not while it falls (30 up) or the player is in the air.
        m.colors.flags[1].shown = false;
        for ([x, y], fall, airborne, grab) in [
            ([60.0, 0.0], None, false, false),
            ([10.0, 30.0], Some(1.0), false, false),
            ([40.0, 0.0], None, true, false),
            ([59.0, 0.0], None, false, true),
        ] {
            m.players[0].body.pos = [0.0; 3];
            m.players[0].body.airborne = airborne;
            m.colors.flags[0] = colors::Flag {
                fall,
                ..flag([x, y, 0.0])
            };
            m.step(&arena, &[Input::default(); 2], &w);
            assert_eq!(m.players[0].flag.is_some(), grab, "{x} {fall:?} {airborne}");
        }
    }

    /// The mask's put-on as e2's P2 (pulling back at full stick from standing, facing 16896, the
    /// mask delivered on frame 1003): its first frame is a full move (speed 3 → 4, the move angle
    /// +6400), then the speed brakes 2.5 a frame and the facing turns 3072 a frame to the frozen
    /// move angle, the position still from 1006 (V e2 reads 1001-1011).
    #[test]
    fn the_mask_put_on_replays_e2() {
        let (_, mut m, w) = duel();
        let arena = Arena {
            spawns: vec![rec([2000, 0, 0], 0)],
            pickups: vec![(weapon::PICKUPS_2D[19], [0.0, 0.0, 3000.0])],
            ..Default::default()
        };
        m.pickups = vec![PickupState {
            timer: 600,
            taken: Some((0, 1)),
            ..Default::default()
        }];
        m.players[1].body.pos = [0.0, 0.0, -3000.0];
        let b = &mut m.players[0].body;
        (b.facing, b.move_angle) = (16896, 16896);
        // Full stick toward 49152, past where the move angle stops (I: e2 doesn't log P2's stick).
        let pull = Input {
            stick_y: -82,
            camera_yaw: 0,
            ..Input::default()
        };
        let mut rows = Vec::new();
        for _ in 1001..=1011 {
            m.step(&arena, &[pull, Input::default()], &w);
            let b = m.players[0].body;
            rows.push((b.xz_speed, b.move_angle, b.facing, b.pos));
        }
        assert_eq!(m.players[0].action, Action::Mask { on: true });
        let speeds: Vec<f32> = rows.iter().map(|r| r.0).collect();
        assert_eq!(speeds[..6], [1.0, 2.0, 3.0, 4.0, 1.5, 0.0]);
        let moves: Vec<u16> = rows.iter().map(|r| r.1).collect();
        assert_eq!(moves[..4], [23296, 29696, 36096, 42496]);
        assert!(moves[4..].iter().all(|&a| a == 42496), "{moves:?}");
        let facings: Vec<u16> = rows.iter().map(|r| r.2).collect();
        assert_eq!(
            facings[..9],
            [
                19968, 23040, 26112, 29184, 32256, 35328, 38400, 41472, 42496
            ]
        );
        assert!(rows[5..].iter().all(|r| r.3 == rows[5].3), "{rows:?}");
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
            ..Default::default()
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
        // Player 0 runs off the mask as it touches it: delivered two frames later, it goes on (state
        // 81, clip 0x151) for 19 frames. That frame moves with the stick (speed 3: the run's third
        // frame), then the stick, A and Z do nothing: it brakes 2.5 and stays, idle once the clip
        // ends (e2's timing: [`the_mask_put_on_replays_e2`]; e3).
        let run = Input {
            stick_y: 82,
            ..Input::default()
        };
        for _ in 0..3 {
            m.step(&arena, &[run, Input::default()], &w);
        }
        let f = m.players[0];
        assert!(f.masked && f.item(m.tick) == 0xA0);
        assert_eq!(
            (f.action, f.anim),
            (Action::Mask { on: true }, anim::MASK_ON)
        );
        assert_eq!((f.since, f.body.xz_speed), (m.tick - 1, 3.0));
        assert_eq!(m.gas.toucher(0), Some(1));
        assert_eq!(m.gas.shown() & 1 << canister, 0);
        let pressing = Input {
            buttons: BUTTON_A | BUTTON_Z | BUTTON_B,
            ..run
        };
        let mut speeds = Vec::new();
        for _ in 1..gas::MASK_FRAMES {
            m.step(&arena, &[pressing, Input::default()], &w);
            speeds.push(m.players[0].body.xz_speed);
            assert_eq!(m.players[0].anim, anim::MASK_ON);
        }
        assert_eq!(speeds[..3], [0.5, 0.0, 0.0]);
        assert_eq!(m.players[0].body.pos[1], 0.0);
        m.step(&arena, &[Input::default(); 2], &w);
        let f = m.players[0];
        assert_eq!(
            (f.action, f.body.state),
            (Action::Free, movement::state::IDLE)
        );
        // The script takes it off: state 78, clip 0x14E, as long.
        m.apply(&[Effect::Release(0)]);
        let f = m.players[0];
        assert!(!f.masked);
        assert_eq!(
            (f.action, f.anim),
            (Action::Mask { on: false }, anim::MASK_OFF)
        );
        for _ in 0..gas::MASK_FRAMES {
            m.step(&arena, &[pressing, Input::default()], &w);
            assert_eq!(m.players[0].anim, anim::MASK_OFF);
        }
        m.step(&arena, &[Input::default(); 2], &w);
        assert_eq!(m.players[0].action, Action::Free);
        // The gassed screen's start outlives a respawn.
        m.apply(&[Effect::Haze(0)]);
        let haze = m.tick;
        m.respawn(0, &arena, &mut Vec::new());
        assert_eq!(m.players[0].gassed, Some(haze));
        let f = m.players[0];
        let k = [1, 2, 150, 151].map(|k| f.haze(haze + 1 + k));
        assert_eq!(
            (f.haze(haze + 1), k),
            (None, [Some(1), Some(2), Some(150), None])
        );

        // The carrier: holstered, no draw, no crouch, no jump, 80 % of the run's 35 (d1: +1 a frame
        // to within 1 of 28, then +0.01; 35 without the canister); the camera reads 0x25 (V t2).
        m.give(1, 0x39);
        m.apply(&[Effect::Carry(1, true)]);
        let mut speeds = Vec::new();
        for k in 0..40 {
            let run = Input {
                stick_y: 82,
                buttons: BUTTON_B | BUTTON_Z | if k % 2 == 0 { BUTTON_A } else { 0 },
                ..Input::default()
            };
            m.step(&arena, &[Input::default(), run], &w);
            speeds.push(m.players[1].body.xz_speed);
            assert_eq!(m.players[1].body.pos[1], 0.0, "no jump");
        }
        let f = m.players[1];
        assert_eq!(
            (f.action, f.drawn, f.item(m.tick), f.camera_weapon()),
            (Action::Free, false, 0xA5, gas::CANISTER)
        );
        assert_eq!(
            (f.body.state, f.anim),
            (movement::state::RUN, anim::CARRY_MOVE)
        );
        let top = speeds.iter().copied().fold(0.0, f32::max);
        assert!(top > 27.0 && top <= 28.0, "{speeds:?}");
        assert!((speeds[39] - speeds[38] - 0.01).abs() < 1e-4, "{speeds:?}");
        m.apply(&[Effect::Carry(1, false)]);
        let f = m.players[1];
        assert_eq!((f.weapon, f.camera_weapon()), (Some(0x39), 0));
        let run = Input {
            stick_y: 82,
            ..Input::default()
        };
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

    /// Total War's takeover (V t1: out 424, swap 437): from 13 frames after the out frame the
    /// script's TakeOver puts the human in its team-mate bot's body where it stands (its momentum,
    /// health and lives: speed 12 → 9.5), 90 frames immune; the bot's slot gets the corpse and is
    /// out on the human's place; each slot keeps its stats. Not before, nor into an enemy.
    #[test]
    fn an_out_human_takes_over_a_bot() {
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            time: None,
            lives: 2,
            total_war: true,
            ..Settings::default()
        });
        let far = Placement {
            index: 0,
            pos: [-3000.0, 0.0, 0.0],
            facing: 0,
            camera: [0.0; 3],
        };
        m.players.push(Fighter {
            bot: true,
            health: 4,
            kills: 3,
            ..Fighter::new(&far, 0, 0)
        });
        m.players[0].deaths = 1;
        m.apply(&[Effect::Hit {
            slot: 0,
            kind: damage::GAS,
            attacker: Some(1),
        }]);
        while m.players[0].out == 0 {
            run(&mut m, &arena, &w, 1, |_| 0);
        }
        let (t0, place, corpse) = (m.players[0].out_at, m.players[0].out, m.players[0].body.pos);
        assert_eq!((m.tick, m.lives_left(0)), (t0 + 1, 0));
        while m.tick < t0 + TAKEOVER_DELAY {
            assert!(m.apply(&[Effect::TakeOver(0, 2)]).is_empty());
            run(&mut m, &arena, &w, 1, |_| 0);
        }
        assert!(m.apply(&[Effect::TakeOver(0, 1)]).is_empty(), "an enemy");
        m.players[2].body.xz_speed = 12.0;
        let bot = m.players[2];
        let ev = m.apply(&[Effect::TakeOver(0, 2)]);
        assert_eq!(ev, [Event::TakeOver { player: 0, from: 2 }]);
        let (h, b) = (m.players[0], m.players[2]);
        assert_eq!((h.body, h.health, h.out, h.immune), (bot.body, 4, 0, 90));
        assert_eq!(
            (h.deaths, h.kills, h.bot, m.lives_left(0)),
            (2, 0, false, 2)
        );
        assert_eq!(
            (b.body.pos, b.health, b.out, b.out_at),
            (corpse, 0, place, m.tick)
        );
        assert_eq!((b.deaths, b.kills, b.bot, m.lives_left(2)), (0, 3, true, 0));
        run(&mut m, &arena, &w, 1, |_| 0);
        let h = m.players[0];
        assert_eq!((h.body.xz_speed, h.immune), (9.5, 89));
        assert!(m.ended_at.is_none());
    }

    /// The gas's kill ends as any kill, 86 + 52 + 55 frames on: untimed with 1 life the victim is
    /// out and Total War ends; with a life left it respawns.
    #[test]
    fn a_gas_death_puts_out_or_respawns() {
        for lives in [1, 2] {
            let (arena, m, w) = duel();
            let mut m = m.with(Settings {
                time: None,
                lives,
                total_war: true,
                ..Settings::default()
            });
            m.apply(&[Effect::Hit {
                slot: 1,
                kind: damage::GAS,
                attacker: Some(0),
            }]);
            let ev = run(&mut m, &arena, &w, 200, |_| 0);
            let respawn = (ev.iter()).find(|e| matches!(e.1, Event::Respawn { .. }));
            let end = (m.players[1].out, m.ended_at, respawn.map(|e| e.0));
            match lives {
                1 => assert_eq!(end, (2, Some(194), None)),
                _ => assert_eq!(end, (0, None, Some(193))),
            }
        }
    }

    /// A mounted gun on the flat: both players touch it, the first in slot order jumps on and the
    /// other is refused while it is manned; the gunner fires from both barrels and leaves with A,
    /// and the gun waits 30 frames before the other takes it; that one's death throws it off with
    /// no wait.
    #[test]
    fn a_mounted_gun_takes_one_gunner() {
        let (arena, mut m, w) = duel();
        let place = gun::Place {
            pos: [0.0, 0.0, 300.0],
            yaw: 0,
        };
        m.guns = vec![Gun::new(place)];
        (m.players[0].body.pos, m.players[1].body.pos) = ([0.0, 0.0, 250.0], [0.0, 0.0, 350.0]);
        let state = |m: &Match| m.gun_state(0);
        let step = |m: &mut Match, b: u16| {
            let inputs = [
                Input {
                    buttons: b,
                    ..Input::default()
                },
                Input::default(),
            ];
            m.step(&arena, &inputs, &w)
        };
        step(&mut m, 0);
        assert_eq!(
            (state(&m), m.players[0].anim, m.players[1].gun),
            (gun::LAUNCH, id::JUMP, None)
        );
        let mut n = 1;
        while !m.players[0].seated() {
            step(&mut m, 0);
            n += 1;
            assert!(n < 40 && m.players[1].gun.is_none());
        }
        assert_eq!((n, m.players[0].immune, m.players[1].gun), (26, 0, None));
        assert_eq!(m.players[0].body.pos, gun::seat(place.pos, 0, 0));
        assert_eq!(m.players[0].camera_weapon(), conker_camera::GUN);
        step(&mut m, 0);
        assert_eq!(
            (state(&m), m.players[0].anim),
            (gun::IDLE, anim::GUN_SEATED)
        );
        let shots = (0..30)
            .flat_map(|_| step(&mut m, BUTTON_Z))
            .filter(|e| matches!(e, Event::Shot { .. }))
            .count();
        assert!((4..=8).contains(&shots), "{shots}");
        assert_eq!(
            (m.guns[0].shots as usize, m.players[0].shots as usize),
            (shots, shots)
        );
        assert_eq!(
            (state(&m), m.players[0].anim),
            (gun::FIRING, anim::GUN_FIRING)
        );
        step(&mut m, BUTTON_A);
        assert_eq!(
            (m.players[0].gun, m.players[0].immune, state(&m)),
            (None, 20, gun::LEAVING)
        );
        for _ in 0..29 {
            step(&mut m, 0);
            assert_eq!((state(&m), m.players[1].gun), (gun::LEAVING, None));
        }
        step(&mut m, 0);
        assert_eq!(m.players[1].gun.map(|g| g.state), Some(gun::LAUNCH));
        while !m.players[1].seated() {
            step(&mut m, 0);
        }
        m.players[1].health = 1;
        m.apply(&[Effect::Hit {
            slot: 1,
            kind: damage::MG_BULLET,
            attacker: Some(0),
        }]);
        assert_eq!(
            (m.players[1].gun, state(&m), m.players[1].anim),
            (None, gun::FREE, anim::DEATH_SHOT)
        );
    }

    /// A gun on the flat with player 0 seated on it, and a step with player 0's buttons.
    fn seated_gunner() -> (Arena, Match, CollisionWorld, gun::Place) {
        let (arena, mut m, w) = duel();
        let place = gun::Place {
            pos: [0.0, 0.0, 300.0],
            yaw: 0,
        };
        m.guns = vec![Gun::new(place)];
        m.players[0].body.pos = [0.0, 0.0, 250.0];
        while !m.players[0].seated() {
            m.step(&arena, &[Input::default(); 2], &w);
        }
        (arena, m, w, place)
    }

    fn pressing(b: u16) -> [Input; 2] {
        let p0 = Input {
            buttons: b,
            ..Input::default()
        };
        [p0, Input::default()]
    }

    /// A gunner hit on its seat keeps its trigger (`func_15065A5C`'s case 0x1B runs through the hit
    /// states; V h1-h3 read for read: Z first pressed in state 29 fires, Z held fires on with the
    /// budget −2 a frame, the gun firing). Colors' capture respawns everyone (command 0x82) before
    /// it recreates the guns in the same script frame (V bots1 3073: the gunner seated on 3072, at
    /// its base on 3073): the gunner ends at a spawn record, not at the seat, the gun free.
    #[test]
    fn a_hurt_gunner_fires_and_a_capture_respawns_it() {
        let (arena, mut m, w, place) = seated_gunner();
        m.apply(&[Effect::Hit {
            slot: 0,
            kind: damage::MG_BULLET,
            attacker: Some(1),
        }]);
        let hurt = |m: &Match| matches!(m.players[0].action, Action::Hurt { .. });
        assert!(hurt(&m) && m.players[0].seated());
        let shots = m.guns[0].shots;
        for n in 1..=10 {
            m.step(&arena, &pressing(BUTTON_Z), &w);
            let g = m.players[0].gun.unwrap();
            assert!(hurt(&m) && m.players[0].seated());
            assert_eq!((g.state, g.budget), (gun::FIRING, gun::BUDGET - 2 * n));
        }
        assert!(m.guns[0].shots > shots);
        m.apply(&[Effect::Respawn(0), Effect::Gun(0)]);
        m.step(&arena, &pressing(0), &w);
        let p = m.players[0].body.pos;
        assert_eq!((m.players[0].gun, m.gun_state(0)), (None, gun::FREE));
        assert!((p[0] - place.pos[0]).abs() > 1000.0, "{p:?}");
    }

    /// The gunner's client predicts its gun's fire: its own frame ([`Match::step_player`], among the
    /// guns at home as the client has them, its own match's draws) fires the server's shots on the
    /// same ticks, and its fighter (the mount's fire process, the shot count) is the server's
    /// every tick.
    #[test]
    fn the_gunners_client_predicts_its_shots() {
        let (arena, mut m, w, place) = seated_gunner();
        let mut fired = 0;
        for t in 0..60 {
            let inputs = pressing(if t < 40 { BUTTON_Z } else { 0 });
            let mut c = Match::new(&arena, m.players.clone(), Rng::new(99), m.tick);
            c.guns = vec![Gun::new(place)];
            let shots = |ev: Vec<Event>| {
                (ev.into_iter())
                    .filter(|e| matches!(e, Event::Shot { player: 0, .. }))
                    .count()
            };
            let predicted = shots(c.step_player(0, &arena, &inputs[0], &w, &here));
            let served = shots(m.step(&arena, &inputs, &w));
            assert_eq!((predicted, c.players[0]), (served, m.players[0]), "{t}");
            fired += served;
        }
        assert!(fired >= 5 && usize::from(m.players[0].shots) == fired);
    }

    /// War: Beach (`beach.md` §2): a villager's death scores for the Tediz where it registers (the
    /// mounted gun's 35 frames after hp 0, a blast's 88, the strike's blast at once: V bots1 732 →
    /// 767, x1 2772 → 2812), latched a frame later (V bots1 8125 → 8126); a Tedi's scores nothing;
    /// an escape's point latches in its own frame (V end1 212) and its respawn takes the record the
    /// script named; lives are never lost.
    #[test]
    fn beach_scores_and_ends() {
        let beach = |score| Settings {
            beach: true,
            score,
            ..Settings::default()
        };
        let hit = |slot, kind, attacker| Effect::Hit {
            slot,
            kind,
            attacker,
        };
        // The mounted gun's kill: the point when DEATH_BLAST (35 frames) ends.
        let (arena, m, w) = duel();
        let mut m = m.with(beach(1));
        m.players[0].health = 2;
        let t = m.tick;
        m.apply(&[hit(0, damage::MG_BULLET, Some(1))]);
        assert_eq!((m.players[0].health, m.players[1].kills), (0, 1));
        run(&mut m, &arena, &w, 35, |_| 0);
        assert_eq!(m.colors.scores, [0, 0]);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!((m.colors.scores, m.ended_at), ([0, 1], Some(t + 36)));
        assert_eq!(m.places(), [2, 1]);
        // A blast's: when BLOWN_UP (88) ends, with the respawn; never out of lives.
        let (arena, m, w) = duel();
        let mut m = m.with(beach(0));
        m.players[0].deaths = 10;
        m.players[0].health = 2;
        m.apply(&[hit(0, damage::GRENADE_OUTER, Some(1))]);
        let ev = run(&mut m, &arena, &w, 89, |_| 0);
        assert_eq!(m.colors.scores, [0, 1]);
        let respawn = ev.iter().find(|e| matches!(e.1, Event::Respawn { .. }));
        assert_eq!((respawn.map(|e| e.0), m.players[0].out), (Some(88), 0));
        // The deadline's strike: HIT 40 frames, then the blast kills and scores at once; the
        // respawn 88 frames on (V 89), the class re-rolled.
        let (arena, m, w) = duel();
        let mut m = m.with(beach(0));
        m.players[0].class = 0x14;
        m.apply(&[Effect::Strike(0)]);
        assert_eq!((m.players[0].anim, m.players[0].immune), (anim::HIT, 36));
        run(&mut m, &arena, &w, 40, |_| 0);
        assert_eq!((m.players[0].health, m.colors.scores), (6, [0, 0]));
        run(&mut m, &arena, &w, 1, |_| 0);
        let f = m.players[0];
        assert_eq!(
            (f.health, f.anim, m.colors.scores),
            (0, anim::BLOWN_UP, [0, 1])
        );
        let ev = run(&mut m, &arena, &w, 89, |_| 0);
        let respawn = ev.iter().find(|e| matches!(e.1, Event::Respawn { .. }));
        assert_eq!(respawn.map(|e| e.0), Some(40 + 88));
        assert!((0x11..=0x20).contains(&m.players[0].class));
        // The detonation on a Tedi: dead, no credit, no point.
        let (arena, m, w) = duel();
        let mut m = m.with(beach(0));
        let kind = damage::script_type(0x29).unwrap();
        m.apply(&[hit(1, kind, None)]);
        run(&mut m, &arena, &w, 89, |_| 0);
        let (v, k) = (m.players[1], m.players[0]);
        assert_eq!((v.deaths, k.kills, m.colors.scores), (1, 0, [0, 0]));
        assert_eq!(v.health, HEALTH, "back 88 frames on (V 87-91)");
        // An escape: the next start record, the team's point, the respawn there; the end latches
        // in the same frame.
        let (arena, m, w) = duel();
        let mut m = m.with(beach(1));
        let t = m.tick;
        m.apply(&[Effect::Start(0, 1), Effect::Score(0, 1), Effect::Respawn(0)]);
        let ev = run(&mut m, &arena, &w, 1, |_| 0);
        assert!(
            ev.iter()
                .any(|e| matches!(e.1, Event::Respawn { player: 0, at } if at.index == 1))
        );
        assert_eq!((m.players[0].spawn_mask, m.ended_at), (0b10, Some(t)));
        assert_eq!(m.places(), [1, 2]);
    }

    /// Item 0x3E on a villager (V spd, spr): 75 % of the run (26.25), 150 % for the 300 frames of a
    /// bottle's 600 VIs (52.5), the crouch-walk's 40 % (14); no draw, no attack. A bottle is back
    /// 30 frames after its touch; a Tedi takes one for nothing. The weapon zones' item comes and goes.
    #[test]
    fn beach_items() {
        let (mut arena, m, w) = duel();
        arena.pickups = vec![(weapon::PICKUPS_2B[0], [0.0, 0.0, 3000.0])];
        let mut m = Match::new(&arena, m.players, Rng::new(1), 0).with(Settings {
            beach: true,
            ..Settings::default()
        });
        m.players[0].start = Some(beach::ITEM);
        m.players[1].body.pos = [5000.0, 0.0, 0.0];
        let drive = |m: &mut Match, frames, buttons| {
            let stick = Input {
                stick_y: 80,
                buttons,
                ..Input::default()
            };
            for _ in 0..frames {
                m.step(&arena, &[stick, Input::default()], &w);
            }
            m.players[0].body.xz_speed
        };
        assert!((drive(&mut m, 60, BUTTON_B) - 26.25).abs() < 0.01);
        assert_eq!(
            (m.players[0].weapon, m.players[0].drawn),
            (Some(beach::ITEM), false)
        );
        m.players[0].boost = beach::BOOST;
        assert!((drive(&mut m, 60, 0) - 52.5).abs() < 0.01);
        assert!((drive(&mut m, 240, 0) - 52.5).abs() < 0.01);
        assert_eq!(m.players[0].boost, 0);
        assert!((drive(&mut m, 60, 0) - 26.25).abs() < 0.01);
        run(&mut m, &arena, &w, 40, |_| 0);
        drive(&mut m, 120, BUTTON_Z);
        let b = m.players[0].body;
        assert_eq!(b.state, movement::state::CROUCH_WALK);
        assert!((b.xz_speed - 14.0).abs() < 0.01, "{}", b.xz_speed);
        assert_eq!(m.players[0].shots, 0);
        // C-down throws no grenade (V beach-gunner gren).
        run(&mut m, &arena, &w, 40, |t| u16::from(t % 2 == 0) * C_DOWN);
        assert_eq!((m.players[0].grenades, m.players[0].shots), (3, 0));
        // The bottle: on it, the boost 2 frames on; back 30 frames after the touch.
        m.players[0].body.pos = [0.0, 0.0, 3000.0];
        let t = m.tick;
        run(&mut m, &arena, &w, 3, |_| 0);
        assert_eq!(m.players[0].boost, beach::BOOST - 2);
        assert_eq!(m.pickups[0].timer, 30 - 2);
        m.players[0].body.pos = [0.0, 0.0, 0.0];
        m.players[1].body.pos = [0.0, 0.0, 3000.0];
        run(&mut m, &arena, &w, 30 - 2, |_| 0);
        assert_eq!(m.pickups[0].taken.map(|p| (p.0, p.1 - t)), Some((1, 30)));
        run(&mut m, &arena, &w, 2, |_| 0);
        assert_eq!(
            (m.players[1].boost, m.players[1].weapon),
            (beach::BOOST, None)
        );
        // The zones' gives.
        m.apply(&[Effect::Give(1, 0x16)]);
        assert_eq!(m.players[1].weapon, Some(0x16));
        m.apply(&[Effect::Give(1, 0)]);
        assert_eq!(m.players[1].weapon, None);
    }

    /// The plunger sinks 4 a frame under a player on its top, to 332 (V wz 901-917), its presser
    /// with it, 508 to 448 (V b0-b8: the prop's y + 116 on the same read), and the script's hold
    /// keeps it down.
    #[test]
    fn beach_plunger() {
        let (arena, m, _) = duel();
        let (top, [x, z]) = (beach::TOP_Y, beach::TOP);
        let quad = |y: f32, [x0, x1]: [f32; 2], [z0, z1]: [f32; 2], layer| {
            let [a, b, c, d] = [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]];
            [([a, b, c], 5, layer), ([a, c, d], 5, layer)]
        };
        let mut w = CollisionWorld::layered(
            [
                quad(
                    0.0,
                    [-3000.0, 6000.0],
                    [-1000.0, 5000.0],
                    conker_sim::collision::Layer::Terrain,
                ),
                quad(top, x, z, conker_sim::collision::Layer::Placement(0)),
            ]
            .concat(),
        );
        let mut m = m.with(Settings {
            beach: true,
            ..Settings::default()
        });
        m.players[0].body = PlayerState::spawn([-1470.0, top, 2350.0], 0);
        m.players[1].body.pos = [5000.0, 0.0, 0.0];
        let mut ys = Vec::new();
        for _ in 0..16 {
            beach::seat(&mut w, m.plunger.y);
            run(&mut m, &arena, &w, 1, |_| 0);
            ys.push((m.plunger.y, m.players[0].body.pos[1]));
        }
        assert_eq!(m.plunger.toucher, Some(0));
        assert_eq!(
            (ys[0], ys[14], ys[15]),
            ((388.0, 504.0), (332.0, 448.0), (332.0, 448.0))
        );
        m.players[0].body.pos = [-1000.0, 0.0, 2350.0];
        m.apply(&[Effect::Plunger(true)]);
        run(&mut m, &arena, &w, 20, |_| 0);
        assert_eq!(m.plunger.y, 332.0);
        m.apply(&[Effect::Plunger(false)]);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.plunger.y, 338.0);
    }

    /// The bat (V h3): B draws 0x10E for 15 frames; Z swings the club's 0x113, the hit landing 10
    /// frames in for 1.
    #[test]
    fn bat_swings_as_the_club() {
        let (arena, mut m, w) = duel();
        m.players[1].body.pos[0] = 60.0;
        m.give(0, 0x19);
        let ev = run(&mut m, &arena, &w, 2, |t| if t == 0 { BUTTON_B } else { 0 });
        assert!(ev.is_empty() && m.players[0].anim == 0x10E);
        let ev = run(
            &mut m,
            &arena,
            &w,
            40,
            |t| if t == 20 { BUTTON_Z } else { 0 },
        );
        let hits: Vec<_> = (ev.iter())
            .filter(|(_, e)| matches!(e, Event::Hit { victim: 1, .. }))
            .map(|(t, _)| *t)
            .collect();
        assert_eq!((hits, m.players[1].health), (vec![30], HEALTH - 1));
    }

    /// The tommy gun (V h4): Z held 160-400 fires 62 bullets (the press and every 4 frames from 3
    /// in), with no clip; the climb goes 40, 120, 240, 335, 375 and to 0 once Z is let go.
    #[test]
    fn tommy_gun_fires_and_climbs() {
        let (arena, mut m, w) = duel();
        m.players[1].body.pos[2] = 3000.0;
        m.give(0, 0x38);
        run(
            &mut m,
            &arena,
            &w,
            20,
            |t| if t == 0 { BUTTON_B } else { 0 },
        );
        let mut climbs = Vec::new();
        for _ in 0..250 {
            run(
                &mut m,
                &arena,
                &w,
                1,
                |t| if t <= 260 { BUTTON_Z } else { 0 },
            );
            climbs.push(m.players[0].climb.climb);
        }
        assert_eq!(m.players[0].shots, 62);
        assert_eq!(climbs[..5], [40, 120, 240, 335, 375]);
        assert_eq!((climbs[240], climbs[241]), (0x1E00, 0));
        assert_eq!(m.players[0].action, Action::Free);
    }

    /// 0x1A's roaming pickups (V dm6): the first frame puts k0-k3 on four spots of 0-7; one taken
    /// at t is back at t + 150 on a spot no pickup stood on.
    #[test]
    fn vault_pickups_roam() {
        let (_, m, w) = duel();
        let arena = Arena {
            pickups: (weapon::PICKUPS_1A.iter().zip(weapon::SPOTS_1A))
                .map(|(p, s)| (*p, s.map(f32::from)))
                .collect(),
            roaming: weapon::Roaming::vault(),
            respawn: Some(weapon::VAULT_RESPAWN),
            ..Default::default()
        };
        let mut m = Match::new(&arena, m.players, Rng::new(1), 0);
        run(&mut m, &arena, &w, 1, |_| 0);
        let spots = |m: &Match| {
            m.pickups[..4]
                .iter()
                .map(|p| p.spot.unwrap())
                .collect::<Vec<_>>()
        };
        let first = spots(&m);
        assert!(first.iter().all(|&s| s < 8));
        assert!((1..4).all(|k| !first[..k].contains(&first[k])), "{first:?}");
        m.players[0].body.pos = m.pickup_at(&arena, 2);
        let ev = run(&mut m, &arena, &w, 1, |_| 0);
        assert!(ev.iter().any(|e| e.1
            == Event::Pickup {
                player: 0,
                pickup: 2
            }));
        m.players[0].body.pos = [9000.0, 0.0, 9000.0];
        run(&mut m, &arena, &w, 149, |_| 0);
        assert_eq!((m.pickups[2].timer, spots(&m)), (1, first.clone()));
        run(&mut m, &arena, &w, 1, |_| 0);
        let back = spots(&m);
        assert_eq!(m.pickups[2].timer, 0);
        assert!(!first.contains(&back[2]), "{first:?} {back:?}");
        assert_eq!((back[0], back[1], back[3]), (first[0], first[1], first[3]));
    }

    /// Heist with at most 4 players (flag 1, V h4a): a roaming pickup is gone while its holder
    /// holds the weapon; the holder's death drops it 50 above where it died, 2 frames on, for 150
    /// frames, then it moves to a spot.
    #[test]
    fn heist_pickups_are_unique() {
        let (_, m, w) = duel();
        let arena = Arena {
            pickups: (weapon::PICKUPS_1A.iter().zip(weapon::SPOTS_1A))
                .map(|(p, s)| (*p, s.map(f32::from)))
                .collect(),
            roaming: weapon::Roaming::vault(),
            respawn: Some(weapon::VAULT_RESPAWN),
            ..Default::default()
        };
        let mut m = Match::new(&arena, m.players, Rng::new(1), 0).with(Settings {
            heist: true,
            ..Settings::default()
        });
        assert!(m.unique());
        m.players[1].body.pos = [-9000.0, 0.0, -9000.0];
        run(&mut m, &arena, &w, 1, |_| 0);
        m.players[0].body.pos = m.pickup_at(&arena, 2);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.pickups[2].holder, Some(0));
        m.players[0].body.pos = [9000.0, 0.0, 9000.0];
        run(&mut m, &arena, &w, 300, |_| 0);
        assert!(m.pickups[2].timer > 0, "gone while held");
        m.players[0].health = 0;
        run(&mut m, &arena, &w, 2, |_| 0);
        let at = [9000.0, 50.0, 9000.0];
        assert_eq!((m.pickups[2].timer, m.pickup_at(&arena, 2)), (0, at));
        run(&mut m, &arena, &w, 149, |_| 0);
        assert_eq!(m.pickup_at(&arena, 2), at);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert!(m.pickups[2].drop.is_none() && m.pickup_at(&arena, 2) != at);
    }

    /// Heist (V h1, h2, h3): the bag is grabbed on the touch (0x114, 26 frames), carried at 0.75 a
    /// frame up to 25.2 with no weapon; B throws it 15 frames in, untouchable for 58 frames; a
    /// script let-go drops it, touchable 3 frames on; a delivery scores the team and its carrier.
    #[test]
    fn heist_grabs_carries_throws_and_scores() {
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            heist: true,
            score: 2,
            ..Settings::default()
        });
        m.players[1].body.pos = [0.0, 0.0, 3000.0];
        m.apply(&[Effect::Cash(true)]);
        m.heist.cash.pos = [40.0, 0.0, 0.0];
        run(&mut m, &arena, &w, 1, |_| 0);
        let f = m.players[0];
        assert_eq!(
            (f.flag, f.anim, f.item(m.tick)),
            (Some(heist::CASH), heist::GRAB, 0)
        );
        assert_eq!(m.heist.cash.carrier, Some(0));
        let mut speeds = Vec::new();
        for _ in 0..60 {
            let stick = Input {
                stick_y: 80,
                buttons: BUTTON_Z,
                ..Input::default()
            };
            m.step(&arena, &[stick, Input::default()], &w);
            speeds.push(m.players[0].body.xz_speed);
        }
        assert_eq!(
            speeds[24..27],
            [0.0, 0.75, 1.5],
            "26 frames held, then +0.75"
        );
        let top = speeds.iter().copied().fold(0.0, f32::max);
        assert!(top > 24.75 && top <= 25.2 + 1e-4, "{top}");
        assert_eq!((m.players[0].shots, m.players[0].anim), (0, heist::RUN));
        // B: the throw, the bag away 15 frames on, flying along the facing.
        let b = m.tick;
        run(
            &mut m,
            &arena,
            &w,
            16,
            |t| if t == b { BUTTON_B } else { 0 },
        );
        let c = m.heist.cash;
        assert_eq!(
            (c.carrier, c.state, c.thrown),
            (None, heist::State::Flight, true)
        );
        assert_eq!((c.since, c.touchable_at), (b + 15, b + 15 + heist::THROWN));
        assert_eq!(m.players[0].flag, None);
        m.apply(&[Effect::Settle(usize::from(heist::CASH))]);
        assert!(!m.heist.cash.thrown);
        // Taken again once the throw's clip is over, then let go of by the script (a hit):
        // touchable 3 frames on.
        run(&mut m, &arena, &w, 6, |_| 0);
        let t = m.tick;
        m.heist.cash = heist::Cash::new(t);
        m.heist.cash.pos = m.players[0].body.pos;
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.heist.cash.carrier, Some(0));
        m.apply(&[Effect::LetGo(usize::from(heist::CASH))]);
        let c = m.heist.cash;
        assert_eq!(
            (c.state, c.touchable_at),
            (heist::State::Fall, m.tick + heist::DROPPED)
        );
        // A delivery: the script lets go, scores team 0, destroys the bag.
        let t = m.tick;
        m.apply(&[Effect::Score(0, 1), Effect::Cash(false)]);
        assert_eq!((m.heist.scores, m.players[0].bags), ([1, 0, 0, 0], 1));
        assert!(!m.heist.cash.shown);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.ended_at, None);
        m.apply(&[Effect::Score(0, 1)]);
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.ended_at, Some(t + 1), "latched on the second bag's frame");
        assert_eq!(m.places(), [1, 2]);
    }

    /// Heist's four teams (V bots1 time-out: S [0, 2, 1, 1], GREENS's 13 kills over YELLOWS's 10,
    /// ranks [4, 1, 2, 3]; unt1 elimination: YELLOWS, BLUES, REDS out in that order, [2, 3, 1, 4]).
    #[test]
    fn heist_ranks_four_teams() {
        let st = |team, score, kills, out| {
            Some(Standing {
                team,
                score,
                kills,
                out,
                ..Standing::default()
            })
        };
        let bots1 = [
            st(0, 0, 9, 0),
            st(1, 2, 5, 0),
            st(2, 1, 13, 0),
            st(3, 1, 10, 0),
        ];
        assert_eq!(teams_places(&bots1, 4), [4, 1, 2, 3]);
        assert_eq!(teams_result(&bots1, 4), Some(1));
        let unt1 = [
            st(0, 0, 0, 2),
            st(1, 0, 0, 3),
            st(2, 0, 0, 0),
            st(3, 0, 0, 4),
        ];
        assert_eq!(teams_places(&unt1, 4), [2, 3, 1, 4]);
        // Two teams equal on everything share 1st: no winner.
        let tie = [
            st(0, 1, 0, 0),
            st(1, 1, 0, 0),
            st(2, 0, 0, 0),
            st(3, 0, 0, 0),
        ];
        assert_eq!(
            (teams_places(&tie, 4), teams_result(&tie, 4)),
            (vec![1, 1, 3, 3], None)
        );
    }

    /// Heist untimed: once a player is out and fewer than 2 teams have someone in play, the match
    /// latches the next frame (V unt1 2526 → 2527); the takeover hands the bag to the human.
    #[test]
    fn heist_eliminates_and_hands_the_bag_over() {
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            heist: true,
            time: None,
            lives: 1,
            ..Settings::default()
        });
        m.players[1].out = 2;
        let t = m.tick;
        run(&mut m, &arena, &w, 1, |_| 0);
        assert_eq!(m.ended_at, Some(t + 1));
        // The takeover: human 0 out, its team-mate bot 1 carries the bag.
        let (arena, m, w) = duel();
        let mut m = m.with(Settings {
            heist: true,
            time: None,
            ..Settings::default()
        });
        m.players[1].team = 0;
        m.players[1].bot = true;
        (m.players[0].out, m.players[0].out_at) = (2, 0);
        m.players[1].flag = Some(heist::CASH);
        m.heist.cash.take(1, 0);
        m.tick = TAKEOVER_DELAY;
        let ev = m.apply(&[Effect::TakeOver(0, 1)]);
        assert_eq!(ev, [Event::TakeOver { player: 0, from: 1 }]);
        assert_eq!(
            (m.heist.cash.carrier, m.players[0].flag),
            (Some(0), Some(heist::CASH))
        );
        let _ = (arena, w);
    }
}
