//! Network protocol shared by `conker-server` and `conker-client` (lightyear 0.30).
//!
//! Each crate adds its own lightyear role (`client`/`server`) and transports; this module only
//! registers what both sides must agree on: the input type ([`Input`], one per tick, its camera yaw
//! included), the replicated player state ([`Player`], predicted by its owner), each player's
//! combat state ([`Combat`]), the match ([`MatchView`]), the side a client asks for when it joins
//! ([`Join`]) and what a client showed when it took an input ([`Saw`]). conker-rules adds the fighter (`conker_rules::net::Fighting`, predicted with
//! the player): the server runs the match, each client predicts its own player with the same rules.

use crate::collision::{CollisionWorld, Layer};
use crate::movement::{Input, PlayerState, Side, angle_diff};
use crate::root::RootTrack;
use crate::trig::AcosTable;
use bevy::ecs::entity::MapEntities;
use bevy::prelude::*;
use conker_camera::volume::CameraLevel;
use lightyear::prelude::input::InputConfig;
use lightyear::prelude::input::native::InputPlugin;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

/// Protocol version = simulation version: bump on any change to the protocol or to `movement::step`.
/// 2: game-unit movement (`movement::step`), N64 input with the camera yaw.
/// 3: the game's wall routine, the full 16-bit stick yaw, and `PlayerState`'s new fields (heading,
/// turning, aiming, prev_strafe, sink, speed_percent).
/// 4: Deathmatch combat on the server (conker-rules): `Rules`, [`Combat`], [`MatchView`].
/// 5: wave 4's movement: `PlayerState`'s `root`, `twist` and `scope_yaw`; pushing (`step_among`), root
/// motion, the aimed gun's twist, the scope's facing, the walls' zone gate, the armed C-strafe rules.
/// 6: wave 4's netcode: conker-rules' `Fighting` (each player's fighter, predicted by its owner),
/// [`Saw`], and the rockets and aimed shots in [`MatchView`]. Wave 3's `Rules` masks are gone: the
/// fighter carries them.
/// 7: wave 5: a player's `Fighting` goes to its own client only; [`Combat`] gains `slot`, `team` and
/// `stun`, what the other clients' prediction reads of it.
/// 8: wave 6: each `Fighting` carries every grenade in flight, not only its owner's (each client
/// predicts the others' grenades too), and a player who leaves frees its slot without moving the
/// others down (`Fighting.slot` and [`Combat::slot`] stay put).
/// 9: sides: a client asks for one when it joins ([`Join`]), every player's goes out in
/// [`Combat::side`] (it picks the model and the body), and the fighter carries it.
/// 10: each side starts and respawns in its own base: the fighter carries the records it respawns
/// on (conker-rules' `Fighter::spawn_mask`), so its owner predicts the server's pick.
/// 11: the match's SET-UP options and its end: [`MatchView`] carries the settings and the restart
/// countdown, [`Combat`] what ranks a player (deaths, shots, the place it went out on, lives left),
/// and a client votes to play again with [`Restart`].
/// 12: the cavemen, a third [`Side`] (bots on scene 0x3F), and a match's start item: the fighter
/// carries it (conker-rules' `Fighter::start`, back after each respawn) and the club's combo window.
/// 13: War: Total War on scene 0x2D: [`MatchView::total_war`] (and in the settings the fighter's
/// rules run with); [`Combat::team`] is then the player's side.
/// 14: Total War's gas: [`MatchView`]'s `countdown`, `gas` and `props`, [`Combat`]'s `carrying` and
/// `masked` (and the fighter's: the carrier's speed and lock-out are predicted).
/// 15: the gas's look: [`Combat::haze`] (the fighter's start of it), and the fighter's mask clips
/// (conker-rules' `Action::Mask`: their lock-out is predicted).
/// 16: War: Colors on scene 0x30: [`MatchView`]'s `colors`, `score_limit`, `scores`, `flags`
/// ([`FlagView`]) and `fade`, [`Combat`]'s `flag` and `score`, `PlayerState::holding` (the
/// carrier's speed is predicted) and the fighter's flag and poses (conker-rules' `Action::Pose`).
/// 17: the mounted guns on scenes 0x30 and 0x34: [`MatchView::guns`] ([`GunView`]), and the
/// fighter's mount (conker-rules' `Fighter::gun`: the jump on, the seat and the trigger are predicted).
/// 18: War: Beach on scene 0x2B: [`MatchView`]'s `beach`, `holders`, `plunger` and `cooldown`,
/// [`Combat`]'s `class`, `freeze` and `deadline`, and the fighter's class and boost (the villagers'
/// item 0x3E speed is predicted).
/// 19: [`MatchView::winner`] (the outro and the STATS go by the server's), and over, `restart_in`
/// counts through the outro too (a client that joins mid-outro starts it there).
/// 20: the high jump (movement state 235, predicted) and `PlayerState::walled`.
/// 21: Total War's takeover (the fighter's `out_at` and `extra` lives) and the doors' and covers'
/// heights ([`MatchView::lifts`], their collision predicted).
/// 22: the fighter's mount carries the jump on's seat and the gun's fire process (conker-rules'
/// `Mount`: the local gunner's shots are predicted).
pub const PROTOCOL_ID: u64 = 22;
/// The scene of a pack map: its folder name is the scene in hex (`2d` = 0x2D, `30` = 0x30).
pub fn map_scene(map: &str) -> Option<u16> {
    u16::from_str_radix(map, 16).ok()
}
/// Over, the results' ticks before the next match starts anyway (the original's STATS screen
/// quits on its own after about 40 s).
pub const RESULTS: u32 = 40 * 30;
/// UDP (WebTransport) and TCP (WebSocket) share this port number.
pub const DEFAULT_PORT: u16 = 5888;

/// The client that owns a player entity.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PlayerId(pub PeerId);

/// Authoritative player state; predicted on the owning client, interpolated on the others.
/// `floor.tri` indexes the collision world, so server and clients must build it from the same asset
/// pack ([`scene_world`]: the browser fetches the server's own, from the site that serves the page).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Deref, DerefMut)]
pub struct Player(pub PlayerState);

/// A player's combat state, server-authoritative (not predicted): the HUD's numbers and the
/// animation the rules chose, sent to every client. With the body ([`Player`]) it is all the other
/// clients get of a player: its fighter (conker-rules' `Fighting`) goes to its own client only.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Combat {
    /// The player's slot in the server's match, its team (Deathmatch: one each; Total War: its side,
    /// 0 the squirrels, 1 the Tediz) and its hit stun
    /// (obj+0x104): with `health` and `immune`, what another client's frame reads of it (a lock-on,
    /// a hit, a respawn pick).
    pub slot: u16,
    pub team: u8,
    pub stun: u8,
    /// The player's side: its model, and the body the others' frames push against and hit.
    pub side: Side,
    pub health: u8,
    /// The held weapon's id (0: none), whether it is drawn, and the rounds left in its clip.
    pub weapon: u8,
    pub drawn: bool,
    pub rounds: u8,
    pub grenades: u8,
    /// Kills of other teams' players (the game's score), and deaths the player caused on its own
    /// team, its own included (+0x70: team kills and suicides).
    pub kills: u16,
    pub suicides: u16,
    /// Frames left immune after a (re)spawn.
    pub immune: u8,
    /// The animation id the rules play (obj+0x84): weapon, hit and death states, or the movement's.
    pub anim: u16,
    /// Frames until the respawn while dying or dead, else 0.
    pub respawn_in: u16,
    /// Respawns (and Total War takeovers) so far: the client's camera cuts behind the player when
    /// it changes.
    pub lives: u16,
    /// Deaths and shots fired (with `kills`, the player's place: conker-rules' `places`), the
    /// place it went out on with its last life (0 in play), and the lives it has left.
    pub deaths: u16,
    pub shots: u16,
    pub out: u8,
    pub lives_left: u8,
    /// Hits landed and those on the head (the results' accuracy and headshots).
    pub hits: u16,
    pub headshots: u16,
    /// Total War: carrying a gas canister (item 0x25; animations 0xD7 moving, 0xD8 standing in
    /// `anim`), wearing a gas mask (item 0x20).
    pub carrying: bool,
    pub masked: bool,
    /// Total War: frames into this player's gassed screen (1-150; 0 none), which its own client
    /// draws over its view (conker-rules' `Fighter::gassed`).
    pub haze: u8,
    /// War: Colors: the flag carried (0 the red, model 169, which the squirrels take; 1 the green,
    /// 168, the Tediz'; conker-rules' `colors::FLAGS`), shown in the hand; the carry's clips are in
    /// `anim` (conker-anim's `FLAG_*`).
    pub flag: Option<u8>,
    /// War: Colors: the player's team's score (what its place ranks by first: conker-rules'
    /// `Standing::score`). War: Beach's too.
    pub score: u16,
    /// War: Beach: the record's class (conker-rules' `beach::model`: 156 below 0x1D, 157 from it, the
    /// Tediz' 3 is 117), re-rolled at each respawn; frames left of the escape's freeze (G+0x224C: the
    /// script's 150 for a human, 30 for a bot, from the frame after it stepped into a tunnel; the
    /// player respawns on the next villager record as it ends); the 90 s deadline in VIs (G+0x1574,
    /// human villagers only: the HUD's countdown is `deadline / 60`; 0 struck).
    pub class: u8,
    pub freeze: u8,
    pub deadline: u16,
}

/// War: Colors' flag as the clients draw it (conker-rules' `colors::Flag`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct FlagView {
    /// Drawn: the script created it (300 frames after the start, 300 after it was destroyed) and
    /// has not destroyed it since; on the radar unless carried.
    pub shown: bool,
    /// The carrier's slot (the flag is in its hand: `pos` follows it).
    pub carrier: Option<u16>,
    /// At home, never let go of since its creation (else, not carried, dropped where it lies).
    pub home: bool,
    /// May be touched now: shown, not carried, 50 frames past its last release.
    pub touchable: bool,
    pub pos: [f32; 3],
}

/// A mounted gun as the clients draw it (conker-rules' `gun`): model 138 at scale 0.5 at `pos`
/// turned `yaw`, its barrels (joint 4) pitched `pitch` degrees, positive down.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct GunView {
    pub pos: [f32; 3],
    pub yaw: u16,
    pub pitch: f32,
    /// gun+0x232: 1 free, 2 a player jumping on, 3 manned, 4 firing, 5 the 30 frames after a
    /// dismount, 0xB stopped (B or the overheat, 25 frames).
    pub state: u8,
    /// The gunner's slot, from the jump on.
    pub gunner: Option<u16>,
    /// Shots fired since the match (or the capture that recreated it) started, and the last one's
    /// barrel (0 joint 5, `gun::muzzle`'s −side; 1 joint 7).
    pub shots: u16,
    pub barrel: u8,
    /// s126+0x199: set by Z, cleared once let go with the lock off (the release's sound).
    pub trigger: bool,
}

/// The match, on one replicated entity.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MatchView {
    /// The clock in VIs (60 per second), counting down from `time` minutes.
    pub timer: i32,
    /// The settings (conker-rules' `Settings`): minutes on the clock (0 unlimited: no clock, lives
    /// count), lives, the kill limit (0 none), the radar.
    pub time: u8,
    pub lives: u8,
    pub kill_limit: u8,
    pub radar: bool,
    /// War: Total War, not Deathmatch (conker-rules' `Settings::total_war`): teams are the sides,
    /// the match ranks them (`team_places`, `team_result`), no kill limit.
    pub total_war: bool,
    /// Over: ticks before the next match starts anyway (every human voting [`Restart`] starts it
    /// sooner): [`RESULTS`] once the results show, more through a War outro before them.
    pub restart_in: u16,
    /// War: the winning team (0 the squirrels, 1 the Tediz; conker-rules' `team_result`), fixed
    /// once over; `None` a DRAW, or not a War mode.
    pub winner: Option<u8>,
    /// 0 playing, 1 the end fade, 2 over (the results).
    pub phase: u8,
    /// The tick the match started at and the oracle read it stands for (scripted clients key their
    /// script on it; `None` until the server's `--wait` players have joined).
    pub start: Option<u32>,
    pub first_read: u32,
    pub bullets: Vec<[f32; 3]>,
    pub grenades: Vec<[f32; 3]>,
    /// Weapon 0x16's rockets and weapon 0x14's aimed shots in flight.
    pub rockets: Vec<[f32; 3]>,
    pub shots: Vec<[f32; 3]>,
    /// Bit n: the pickup on placement n is taken (back after 120 frames). Total War's gas masks
    /// are pickups on placements 21-26, touched only while `countdown` runs.
    pub taken: u64,
    /// Total War's gas (conker-rules' `gas`): frames until it kills (300 from the delivery; 0 none;
    /// the HUD's seconds are `countdown / 60 + 1`, the game's `(t + 59) / 60` of t = countdown + 1,
    /// from the frame after the delivery to the kill's), its sound looping (from 29 frames before
    /// the kill to 89 after), and bit n: the prop on placement n shown, for 0 and 1 (the shelter's
    /// doors: shown at the delivery, hidden 51 frames after they start rising once the gas is
    /// over), 2-7 (the covers by the masks: hidden 13 frames after they start rising at the
    /// delivery, back at the gas's end) and 27, 28 (the canisters, at home);
    /// `lifts`: the doors' and covers' heights above home (conker-rules' `gas::seat` puts their
    /// collision there).
    pub countdown: i32,
    pub gas: bool,
    pub props: u32,
    pub lifts: [i16; 8],
    /// War: Colors (conker-rules' `Settings::colors`): teams are the sides, the score target
    /// (`score_limit`, 0 none), each team's captures ([0] squirrels, [1] Tediz), the flags, and the
    /// screen's fade around the respawn after a capture: out (to black) or back in, over that many
    /// VIs, from that tick.
    pub colors: bool,
    pub score_limit: u8,
    pub scores: [u16; 2],
    pub flags: [FlagView; 2],
    pub fade: Option<(bool, u16, u32)>,
    /// The map's mounted guns (scene 0x30's two, 0x34's one, 0x2B's one; none elsewhere).
    pub guns: Vec<GunView>,
    /// War: Beach (conker-rules' `Settings::beach`, `beach`): teams are the sides (0 the villagers,
    /// 1 the Tediz), the score target is `score_limit` and `scores` the teams'; the weapon zones'
    /// holders (the sniper's on the west bunker top, the bazooka's on the east top; slots); the
    /// plunger's y (392 at home, 332 down) and the frames left of its cooldown (900 from the
    /// detonation's frame: it stays down until 20 are left).
    pub beach: bool,
    pub holders: [Option<u16>; 2],
    pub plunger: f32,
    pub cooldown: u16,
}

/// What a client showed of the other players when it sampled its input for `tick`: their states
/// interpolated at server tick `at` plus `overstep` (lightyear's interpolation time). The server's
/// lag compensation rewinds its hit tests to exactly that (conker-rules' `Match::step_seen`);
/// lightyear's own `InterpolationDelay` folds in the client's fixed-step overstep, a fraction of a
/// tick off.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Saw {
    pub tick: u32,
    pub at: u32,
    pub overstep: f32,
}

/// A client's first message: the side it asks for (`None`: the server's pick, the side with fewer
/// players). The server puts the player in the match when it arrives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Join {
    pub side: Option<Side>,
}

/// Over, the results' RESTART: this client's player wants the next match now.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Restart;

/// The channel [`Join`] and [`Restart`] travel on: reliable.
pub struct JoinChannel;

/// The channel [`Saw`] travels on, with the inputs: unreliable (a lost one falls back to
/// `InterpolationDelay`) and unordered (a sequenced channel dropped about 5% of them, V: 150 ms each
/// way, the reports of two ticks crossing).
pub struct SawChannel;

/// The scene's collision, shared by the systems that step players.
#[derive(Resource, Deref)]
pub struct Collision(pub CollisionWorld);

/// A map's world (`scene`: [`map_scene`]) as the server and every client build it from the asset pack (its characters
/// in [`Side`] order, `conker_assets::PACK_CHARACTERS`): the collision in its order with its wall
/// layers (`floor.tri` indexes it, so both sides must agree), the game's arctangent table, the
/// level's camera data (its trigger table), the players' root-motion clips (the Tedi's: both sides
/// play the same clips, V) and the sink table.
pub fn scene_world(pack: &conker_assets::Slice, scene: u16) -> CollisionWorld {
    let mut world = CollisionWorld::layered(pack.collision.iter().map(|t| {
        let layer = match t.layer {
            conker_assets::Layer::Terrain => Layer::Terrain,
            conker_assets::Layer::Secondary => Layer::Secondary,
            conker_assets::Layer::Placement(i) => Layer::Placement(i as u16),
        };
        (t.points, t.surface.unwrap_or(0), layer)
    }));
    world.acos = AcosTable::new(pack.acos.clone());
    world.camera = Some(CameraLevel::new(scene, &pack.triggers));
    // Weapon 0x16's Z, the shot death's drift.
    world.roots = (pack.characters[Side::Tediz as usize].root_clips.iter())
        .map(|c| RootTrack {
            ids: c.logical.clone(),
            duration: c.duration,
            step: c.step,
            z: c.z.clone(),
            end: c.end,
        })
        .collect();
    world.sink = Some(pack.sink);
    world
}

/// A remote player between two server states: position and facing blend (the facing the short way
/// round), everything else is `b`'s.
pub fn lerp(a: &PlayerState, b: &PlayerState, t: f32) -> PlayerState {
    let pos = core::array::from_fn(|i| a.pos[i] + (b.pos[i] - a.pos[i]) * t);
    let turn = (angle_diff(b.facing, a.facing) as f32 * t) as i32;
    PlayerState {
        pos,
        facing: a.facing.wrapping_add(turn as u16),
        ..*b
    }
}

impl Ease for Player {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| Player(lerp(&start, &end, t)))
    }
}

impl MapEntities for Input {
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Inputs carry the client's interpolation delay: the server rewinds its hit tests to it.
        app.add_plugins(InputPlugin::<Input> {
            config: InputConfig {
                lag_compensation: true,
                ..default()
            },
        });
        app.component::<PlayerId>().replicate();
        app.component::<Player>()
            .replicate()
            .predict()
            .add_linear_interpolation();
        app.component::<Combat>().replicate();
        app.component::<MatchView>().replicate();
        app.add_channel::<JoinChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Join>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Restart>()
            .add_direction(NetworkDirection::ClientToServer);
        app.add_channel::<SawChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Saw>()
            .add_direction(NetworkDirection::ClientToServer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::state;

    #[test]
    fn interpolation_blends_position_and_facing_and_takes_the_end_state() {
        let a = PlayerState::spawn([0.0, 0.0, 0.0], 65000);
        let mut b = PlayerState::spawn([2.0, 1.0, -4.0], 500);
        b.state = state::RUN;
        b.xz_speed = 35.0;
        let mid = Player::interpolating_curve_unbounded(Player(a), Player(b)).sample_unchecked(0.5);
        assert_eq!(mid.pos, [1.0, 0.5, -2.0]);
        // 65000 -> 500 is +1036 through 0, not -64500.
        assert_eq!(mid.facing, 65518);
        assert_eq!(
            (mid.state, mid.xz_speed, mid.move_angle),
            (state::RUN, 35.0, 500)
        );
    }
}
