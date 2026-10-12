//! Sound, as the original plays it in a match (`../lab/audio/README.md`): the asset pack's
//! `audio/sounds.json` and `audio/events.json` ([`conker_assets::audio`]) say which game event plays
//! which sound; this module finds the events in what the client has of the match and plays them.
//!
//! - **Events.** The server sends none, so each tick ([`hear`], after the animation clocks) reads
//!   them from the replicated state: a player's clock passing a clip event's time (the local
//!   player's predicted clock, the others' from [`Combat::anim`]; `held.json`'s rule, [`passes`]);
//!   its clip, weapon and model in hand starting or ending a cue's condition (firing, holding,
//!   burning); its fire clip starting (a shot) or its scope coming up; its health dropping (a hit:
//!   [`hit_weapon`] reads the weapon from the victim's first reaction); the match ([`MatchView`]: a
//!   pickup taken, a rocket gone, weapon 0x14's shots, War: Total War's gas); a gas canister taken
//!   (its player's [`Combat::carrying`] going on), War: Colors' flag taken ([`Combat::flag`]); the grenades in flight (the local fighter
//!   carries them all: launch, bounces, fuse and blast); the background and the map's bubbling spots;
//!   a War outro's voice line at its frame (unpositioned, at [`MIX`]; its volume is I).
//! - **Rules** (`events.json`): the listener is the local player. Its own sounds with `own` play
//!   centred at that volume, the rest by distance (full within `near`, none from `far`, linear
//!   between, silent below [`QUIET`]) and pan (along the follow camera's right). A sound on channel
//!   a or b stops its owner's last one there; `chance`, `if_idle`, `pitch` (cents, as the playback
//!   rate); a loop plays until its cue's condition ends, a stop event, its owner's next sound on the
//!   channel or `ticks`. Every sound started logs a `SOUND` line (debug), headless clients too
//!   (without a device a sound that does not loop is over at once, so `if_idle` always passes).
//! - **Playback** (windowed): rodio's mixer, each sound a [`Playing`] source with its loop points,
//!   envelope and pitch, at the two ear volumes the tick sets ([`MIX`] for effects, the background
//!   recording at 1.0, equal-power pan). The device opens at once natively and on the first key
//!   press or click in the browser (a page may start audio only after a gesture), then every WAV
//!   loads ([`WavLoader`]). M mutes ([`MUTE_KEY`]).

use crate::{Anim, FollowCamera, held};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;
use conker_anim::AnimPlayer;
use conker_anim::combat::{
    BLOWN_UP, BURN_END, BURNING, DEATH_BLAST, DEATH_SHOT, FLAME_HIT, GRENADE_HOLD, GRENADE_PULL,
    GRENADE_THROW, HIT_CLUB, HIT_MELEE, HIT_SHOT,
};
use conker_assets::audio::{
    Audio, Background, Channel, ClipAction, ClipSound, Cue, Emitter, On, Play, Sound,
};
use conker_assets::{Held, Manifest, PACK_CHARACTERS, Pickup};
use conker_rules::Rng;
use conker_rules::net::Fighting;
use conker_rules::projectile::{Grenade, ROCKET_SPEED, SHOT_STEP};
use conker_rules::weapon::weapon;
use conker_sim::movement::Side;
use conker_sim::net::{Combat, MatchView, Player, PlayerId};
use core::f32::consts::FRAC_PI_2;
use core::time::Duration;
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::*;
use rodio::{ChannelCount, SampleRate, Source};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

/// Sound effects against the background recording, which plays at 1.0: the original's mix, measured
/// 0.56-0.67 (`../lab/audio`, V `pick57`, `ctl_cd`; centre pan's 0.707 apart, which the pan law
/// adds back).
pub const MIX: f32 = 0.6;
/// A positional sound below this volume (out of 32767) is silent (`func_1000A420`).
pub const QUIET: f32 = 401.0;
/// Toggles the sound.
pub const MUTE_KEY: KeyCode = KeyCode::KeyM;
/// Every [`Playing`] is stereo.
const STEREO: ChannelCount = ChannelCount::new(2).unwrap();
/// The background's voice id (the game's sound ids start at 1).
const BACKGROUND: u16 = 0;
/// A War outro `n`'s voice line ([`crate::outro`]) plays as id `VOICE - n` (the game's
/// sound ids end at 1762).
const VOICE: u16 = u16::MAX;
/// Ticks a loop with `ticks` fades out over once they have run (killing bullets' 1278, C
/// `func_1000EBC4`; the length is I).
const FADE: u32 = 12;
/// Weapon ids the cues use for the grenade, the rocket launcher and weapon 0x14.
const GRENADE: u8 = 0x26;
const ROCKETS: u8 = 0x16;
const SHOTS: u8 = 0x14;
/// The browser's hint until the first key press or click.
#[cfg(target_family = "wasm")]
const HINT: &str = "Press a key or click for sound";

/// Whether a pack has sound: `pack.json`'s `audio` list is not empty.
pub fn has_sound(manifest: &[u8]) -> bool {
    serde_json::from_slice::<Manifest>(manifest).is_ok_and(|m| !m.audio.is_empty())
}

/// The pack's sound data, the held models (Holding cues) and the pickups (Pickup cues), with each
/// side's clip events sorted by clip.
#[derive(Resource)]
struct Pack {
    audio: Audio,
    held: Vec<Held>,
    pickups: Vec<Pickup>,
    clips: [Vec<ClipSound>; 3],
    background: Option<Background>,
    emitters: Vec<Emitter>,
}

/// Registers sound: the tick's events and voices ([`hear`], headless too) and, windowed, the device,
/// the WAV loader and the mute key. `map`: whose ambience plays. `seed`: the client's own random
/// sounds.
pub fn build(
    app: &mut App,
    mut audio: Audio,
    held: Vec<Held>,
    pickups: Vec<Pickup>,
    windowed: bool,
    map: &str,
    seed: u64,
) {
    let mut clips = PACK_CHARACTERS.map(|c| audio.events.characters.remove(c).unwrap_or_default());
    for c in &mut clips {
        c.sort_by_key(|e| e.clip); // stable: each clip's events stay in time order
    }
    let map = audio.events.maps.get(map);
    let background = map.and_then(|m| m.background.first().cloned());
    let emitters = map.map_or(Vec::new(), |m| m.emitters.clone());
    let bursts = vec![Burst::default(); emitters.len()];
    app.insert_resource(Pack {
        audio,
        held,
        pickups,
        clips,
        background,
        emitters,
    });
    app.insert_resource(Sounds {
        rng: Rng::new(seed | 1),
        bursts,
        ..default()
    });
    app.add_systems(
        FixedUpdate,
        hear.after(crate::tick_anims).run_if(not(is_in_rollback)),
    );
    if windowed {
        app.init_asset::<Pcm>().init_asset_loader::<WavLoader>();
        app.add_systems(
            Update,
            (open.run_if(not(resource_exists::<Output>)), mute, running),
        );
        #[cfg(target_family = "wasm")]
        note(HINT);
    }
}

/// Whose a sound is: a player's (channels a and b are per player, by [`Combat::slot`]), a grenade's
/// (birth tick, thrower), one of weapon 0x14's shots in flight (its index), or nobody's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Player(u16),
    Grenade(u32, usize),
    Shot(usize),
    World,
}

/// Where a sound starts from and whose it is: `follows` a player's slot (it moves with it), `tag` the
/// cue whose condition keeps it playing.
#[derive(Clone, Copy)]
struct Src {
    owner: Owner,
    pos: [f32; 3],
    follows: Option<u16>,
    tag: Option<usize>,
}

impl Src {
    fn player(p: &Seen) -> Self {
        Self {
            owner: Owner::Player(p.slot),
            pos: p.pos,
            follows: Some(p.slot),
            tag: None,
        }
    }

    fn at(pos: [f32; 3]) -> Self {
        Self {
            owner: Owner::World,
            pos,
            follows: None,
            tag: None,
        }
    }
}

/// What the sound rules read of a player in a tick; last tick's is its [`Heard`].
#[derive(Clone, Copy)]
struct Seen {
    slot: u16,
    side: Side,
    pos: [f32; 3],
    clock: AnimPlayer,
    /// The weapon held (0: none) and drawn: the local player's prediction, the others' [`Combat`].
    weapon: u8,
    drawn: bool,
    health: u8,
    /// [`Combat::anim`]: the rules' animation, a hit's first reaction on the tick it lands.
    reaction: u16,
    scope: bool,
    local: bool,
    /// The floor's surface kind (bits 0-4 of its word) and the horizontal speed: footsteps.
    floor: Option<u8>,
    speed: f32,
    /// Total War: carrying its team's gas canister, and whether it is a bot ([`On::Carry`]).
    carry: Option<(u8, bool)>,
    /// War: Colors: carrying flag f, and whether it is a bot ([`On::Flag`]).
    flag: Option<(u8, bool)>,
}

/// A player as the last tick saw it.
#[derive(Component, Clone, Copy)]
struct Heard(Seen);

/// The listener: the local player (its slot and position) and its follow camera's right (xz).
#[derive(Clone, Copy)]
struct Ear {
    slot: u16,
    pos: [f32; 3],
    right: [f32; 2],
}

impl Ear {
    fn new(slot: u16, pos: [f32; 3], camera: &FollowCamera) -> Self {
        let (eye, at) = camera.drawn.view(&camera.follow);
        let ahead = at - eye;
        let right = Vec2::new(-ahead.z, ahead.x).normalize_or_zero();
        Self {
            slot,
            pos,
            right: right.to_array(),
        }
    }
}

/// A sound playing (or, without a device, as if it were): what it is and how loud, its mixer
/// controls once it is on the mixer.
struct Voice {
    id: u16,
    owner: Owner,
    channel: Channel,
    tag: Option<usize>,
    pos: [f32; 3],
    follows: Option<u16>,
    /// Unpositioned stereo at `volume` (the background recording).
    flat: bool,
    /// Volumes out of 32767 (footsteps scaled by speed), and the distances.
    volume: f32,
    own: Option<f32>,
    near: f32,
    far: f32,
    /// [`MIX`] × the sample's own volume (1 for the background).
    base: f32,
    speed: f32,
    looped: bool,
    /// The tick a loop with `ticks` ends, and the tick its fade began.
    until: Option<u32>,
    fade: Option<u32>,
    started: u32,
    stopped: bool,
    control: Option<Arc<Control>>,
}

impl Voice {
    /// Its volume out of 1 and pan (0 left, 0.5 centre, 1 right) for `ear` (`func_1000A420`): the
    /// listener's own player's sounds with `own` centred at that volume; the others by distance
    /// (linear from `near` to `far`) and the side of the camera they are on.
    fn level(&self, ear: &Ear) -> (f32, f32) {
        if self.flat {
            return (self.volume / 32767.0, 0.5);
        }
        if let (Some(own), true) = (self.own, self.follows == Some(ear.slot)) {
            return (own / 32767.0, 0.5);
        }
        let d: [f32; 3] = core::array::from_fn(|i| self.pos[i] - ear.pos[i]);
        let dist = dist2(self.pos, ear.pos).sqrt();
        let near = if self.far > self.near {
            ((self.far - dist) / (self.far - self.near)).clamp(0.0, 1.0)
        } else {
            f32::from(dist <= self.near)
        };
        let volume = self.volume * near;
        if volume < QUIET {
            return (0.0, 0.5);
        }
        let xz = (d[0] * d[0] + d[2] * d[2]).sqrt();
        let side = if xz < 1.0 {
            0.0
        } else {
            (d[0] * ear.right[0] + d[2] * ear.right[1]) / xz
        };
        (volume / 32767.0, 0.5 + 0.5 * side)
    }

    /// The two ears' volumes on the mixer at `tick`.
    fn ears(&self, ear: &Ear, tick: u32, muted: bool) -> [f32; 2] {
        let (volume, pan) = self.level(ear);
        let fade = self
            .fade
            .map_or(1.0, |f| 1.0 - tick.saturating_sub(f) as f32 / FADE as f32);
        let v = if muted {
            0.0
        } else {
            self.base * volume * fade
        };
        if self.flat {
            return [v, v];
        }
        let a = pan * FRAC_PI_2;
        [v * a.cos(), v * a.sin()]
    }
}

/// An emitter's burst (`func_151607A4`): bubbles left, and the tick the next may play.
#[derive(Clone, Copy, Default)]
struct Burst {
    left: u32,
    next: u32,
}

/// A cue's sound due later (a gun's case, a body part landing), or (`end`) the end of a cue held by
/// a condition: its sounds stop and its `then` plays.
struct Later {
    tick: u32,
    cue: usize,
    src: Src,
    end: bool,
}

/// The sound state: the voices, sounds due later, the emitters' bursts, and last tick's projectiles
/// and pickups (what changed is an event).
#[derive(Resource, Default)]
struct Sounds {
    rng: Rng,
    voices: Vec<Voice>,
    later: Vec<Later>,
    bursts: Vec<Burst>,
    /// Last tick's grenades (the local fighter's: all in flight), their bounces so far, rockets,
    /// shots and taken pickups; false until the first tick with a match.
    grenades: Vec<(u32, Grenade)>,
    bounces: Vec<((u32, usize), usize)>,
    rockets: Vec<[f32; 3]>,
    shots: Vec<[f32; 3]>,
    taken: u64,
    /// Total War's gas out last tick ([`On::Gas`]).
    gas: bool,
    primed: bool,
    /// The tick a rocket last went off (a blast's hit is a rocket's if one did just before).
    rocket_at: Option<u32>,
    background: bool,
    /// This tick's players (last tick's, this tick's), reused.
    players: Vec<(Seen, Seen)>,
    muted: bool,
}

/// What a tick's rules read besides the sound state.
struct Cx<'a> {
    pack: &'a Pack,
    tick: u32,
    /// The oracle read the tick stands for in `--wait` replays, else -1 (logs).
    read: i64,
    ear: Ear,
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// A side's character folder (cues name it).
fn character(side: Side) -> &'static str {
    PACK_CHARACTERS[side as usize]
}

/// Whether a clip event at `time` fires this tick: the clock passed it (`held.json`'s rule, V on the
/// oracle: an event at 1.0 fires as the clock goes from 0.43, or 0.009, past it). A new clip from its
/// start (this tick's time less the rate) through this tick's time; the same clip after last tick's
/// time through this one's, round the loop when it wrapped.
fn passes(was: &AnimPlayer, is: &AnimPlayer, time: f32) -> bool {
    if is.id != was.id {
        return (is.time - is.rate..=is.time).contains(&time);
    }
    if is.time >= was.time {
        was.time < time && time <= is.time
    } else {
        time > was.time || time <= is.time
    }
}

/// Whether held weapon `w`'s model is in the player's hand ([`held::shows`]).
fn holding(held: &[Held], w: u8, p: &Seen) -> bool {
    let draw = weapon(w).map(|w| w.draw.0);
    held.iter()
        .filter(|h| h.create.iter().any(|c| Some(c.0) == draw))
        .any(|h| held::shows(h, p.weapon, p.drawn, &p.clock))
}

fn burning(p: &Seen) -> bool {
    [FLAME_HIT, BURNING, BURN_END].contains(&p.clock.id)
}

/// The weapon that hit `victim`, which the server does not send, from its first reaction (its
/// [`Combat::anim`] as the hit lands, conker-rules' `damage::sequence`): 0x11B melee 0x21 or the
/// club 0x40 (the nearest holding it drawn), 0x11C the club's second swing, 0xD3
/// weapon 0x14's shot or the crossbow's bolt, 0x69 the flame, 0x19C weapon 0x23; a blast (0xF3) is a
/// rocket's when one went off this tick or the last (`rocket`), else the shotgun's or a grenade's;
/// the kill 0x17E is the sniper's, melee 0x22's, the crossbow's or the shotgun's, and the stagger
/// 0x11D a bullet's or the shotgun's (or a lethal shot or burn): the nearest other player holding
/// one of those drawn decides (a burning victim: the flame), else weapon 0x14, a grenade, the sniper
/// rifle and the twin guns. I: wrong when another player holds such a weapon nearer than the
/// shooter.
fn hit_weapon(victim: &Seen, burnt: bool, others: &[(Seen, Seen)], rocket: bool) -> u8 {
    let nearest = |ids: &[u8]| {
        let d = |p: &Seen| dist2(p.pos, victim.pos);
        (others.iter().map(|o| &o.1))
            .filter(|o| o.slot != victim.slot && o.drawn && ids.contains(&o.weapon))
            .min_by(|a, b| d(a).total_cmp(&d(b)))
            .map(|o| o.weapon)
    };
    match victim.reaction {
        HIT_MELEE => nearest(&[0x40]).unwrap_or(0x21),
        HIT_CLUB => 0x40,
        HIT_SHOT => nearest(&[SHOTS, 0x18]).unwrap_or(SHOTS),
        FLAME_HIT => 0x15,
        DEATH_BLAST => 0x23,
        BLOWN_UP if rocket => ROCKETS,
        BLOWN_UP => nearest(&[0x37]).unwrap_or(GRENADE),
        DEATH_SHOT => nearest(&[0x24, 0x22, 0x18, 0x37]).unwrap_or(0x24),
        _ if burnt => 0x15,
        _ => nearest(&[0x09, 0x39, 0x24, SHOTS, 0x37]).unwrap_or(0x09),
    }
}

/// The projectiles of last tick's list `old` that `new` no longer has. Lists keep their order (new
/// ones go last), so each one left is the next of `new` within `step` of where it was.
fn gone(old: &[[f32; 3]], new: &[[f32; 3]], step: f32) -> impl Iterator<Item = [f32; 3]> {
    let mut next = new.iter().peekable();
    old.iter().copied().filter(move |o| {
        let kept = next.peek().is_some_and(|n| dist2(**n, *o) <= step * step);
        if kept {
            next.next();
        }
        !kept
    })
}

impl Sounds {
    /// A random number in `lo..=hi`.
    fn between(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.rng.next_u32() % (hi - lo).max(0).unsigned_abs().saturating_add(1)) as i32
    }

    /// Starts one of `sounds` as `play` says, from `src` (unless its chance or a busy channel says
    /// no), stopping its owner's last sound on the same channel; volumes × `scale`. Logs it.
    fn start(&mut self, cx: &Cx, play: &Play, sounds: &[u16], scale: f32, src: Src) {
        if play.chance < 255 && self.rng.next_u32() as u8 > play.chance {
            return;
        }
        let mine = |v: &Voice| !v.stopped && v.owner == src.owner && v.channel == play.channel;
        if play.channel != Channel::Free {
            if play.if_idle && self.voices.iter().any(mine) {
                return;
            }
            self.stop(mine);
        }
        if sounds.is_empty() {
            return;
        }
        let id = sounds[self.rng.next_u32() as usize % sounds.len()];
        let Some(sound) = cx.pack.audio.sounds.sounds.get(&id) else {
            return;
        };
        let cents = self.between(i32::from(play.pitch[0]), i32::from(play.pitch[1]));
        self.push(
            cx,
            Voice {
                id,
                owner: src.owner,
                channel: play.channel,
                tag: src.tag,
                pos: src.pos,
                follows: src.follows,
                flat: false,
                volume: f32::from(play.volume) * scale,
                own: play.own.map(|v| f32::from(v) * scale),
                near: f32::from(play.near),
                far: f32::from(play.far),
                base: MIX * f32::from(sound.volume) / 127.0,
                speed: (cents as f32 / 1200.0).exp2(),
                looped: sound.looped.is_some(),
                until: play.ticks.map(|t| cx.tick + u32::from(t)),
                fade: None,
                started: cx.tick,
                stopped: false,
                control: None,
            },
        );
    }

    fn push(&mut self, cx: &Cx, v: Voice) {
        let (volume, pan) = v.level(&cx.ear);
        debug!(
            "SOUND t={} read={} id={} p={:?} vol={:.0} gain={volume:.3} pan={pan:.2}",
            cx.tick, cx.read, v.id, v.owner, v.volume
        );
        self.voices.push(v);
    }

    /// Stops the sounds `which` picks (each fades out over its envelope's release).
    fn stop(&mut self, which: impl Fn(&Voice) -> bool) {
        for v in self.voices.iter_mut().filter(|v| !v.stopped && which(v)) {
            v.stopped = true;
            if let Some(c) = &v.control {
                c.stop.store(true, Relaxed);
            }
        }
    }

    /// Plays every cue `matches` picks for player `p` (its character's or anyone's), from `src`.
    fn cues(&mut self, cx: &Cx, p: &Seen, src: Src, matches: impl Fn(&On) -> bool) {
        for cue in &cx.pack.audio.events.cues {
            if matches(&cue.on)
                && cue
                    .character
                    .as_deref()
                    .is_none_or(|c| c == character(p.side))
            {
                self.start(cx, &cue.play, &cue.play.sounds, 1.0, src);
            }
        }
    }

    /// A cue held by a condition: it starts as the condition begins; the tick after it ends (its
    /// sound's callback sees it then, V e_w09: the twin guns' tail a read after the fire clip), its
    /// sounds stop and `then` plays where the player was.
    fn hold(&mut self, cx: &Cx, i: usize, cue: &Cue, was: bool, is: bool, p: &Seen) {
        let src = Src::player(p);
        if is && !was {
            let tagged = Src {
                tag: Some(i),
                ..src
            };
            self.start(cx, &cue.play, &cue.play.sounds, 1.0, tagged);
        } else if was && !is {
            let (tick, src) = (
                cx.tick + 1,
                Src {
                    follows: None,
                    ..src
                },
            );
            self.later.push(Later {
                tick,
                cue: i,
                src,
                end: true,
            });
        }
    }

    /// The sounds due by now ([`Later`]), before this tick's events.
    fn due(&mut self, cx: &Cx) {
        let mut k = 0;
        while k < self.later.len() {
            if self.later[k].tick > cx.tick {
                k += 1;
                continue;
            }
            let Later {
                cue: i, src, end, ..
            } = self.later.swap_remove(k);
            let cue = &cx.pack.audio.events.cues[i];
            if !end {
                self.start(cx, &cue.play, &cue.play.sounds, 1.0, src);
                continue;
            }
            self.stop(|v| v.owner == src.owner && v.tag == Some(i) && v.started < cx.tick);
            for then in &cue.then {
                self.start(cx, then, &then.sounds, 1.0, src);
            }
        }
    }

    /// Player `is`'s sounds this tick (`was`: last tick's): its clip's events, its cues' conditions,
    /// a shot, the scope, a hit.
    fn player(&mut self, cx: &Cx, was: &Seen, is: &Seen, all: &[(Seen, Seen)]) {
        let me = Src::player(is);
        let clips = &cx.pack.clips[is.side as usize];
        let first = clips.partition_point(|c| c.clip < is.clock.id);
        for e in clips[first..].iter().take_while(|c| c.clip == is.clock.id) {
            if !passes(&was.clock, &is.clock, e.time) {
                continue;
            }
            match &e.action {
                ClipAction::Play(p) => self.start(cx, p, &p.sounds, 1.0, me),
                ClipAction::Footstep(kind) => self.footstep(cx, *kind, is),
                ClipAction::Stop(c) => self.stop(|v| v.owner == me.owner && v.channel == *c),
            }
        }
        if let (Some((canister, bot)), None) = (is.carry, was.carry) {
            self.cues(cx, is, me, |on| *on == On::Carry { canister, bot });
        }
        if let (Some((flag, bot)), None) = (is.flag, was.flag) {
            self.cues(cx, is, me, |on| *on == On::Flag { flag, bot });
        }
        let fire =
            |w: u8, p: &Seen| p.weapon == w && weapon(w).is_some_and(|w| w.fire.0 == p.clock.id);
        for (i, cue) in cx.pack.audio.events.cues.iter().enumerate() {
            if !cue
                .character
                .as_deref()
                .is_none_or(|c| c == character(is.side))
            {
                continue;
            }
            match cue.on {
                On::Firing { weapon, ref clips } => {
                    let on = |p: &Seen| p.weapon == weapon && clips.contains(&p.clock.id);
                    self.hold(cx, i, cue, on(was), on(is), is);
                }
                // The grenade's hiss: from 4 ticks into the throw (V e_gren, sq_e_gren, ctlw_k*:
                // 1299 at the throw's read + 4, stopped by the blast), then with the grenade
                // ([`Self::grenades`]); a throw that never leaves the hand fades out.
                On::Holding { weapon: GRENADE } => {
                    let (was, is) = (&was.clock, &is.clock);
                    if is.id == GRENADE_THROW && passes(was, is, 4.0 * is.rate) {
                        let at = Src { tag: Some(i), ..me };
                        self.start(cx, &cue.play, &cue.play.sounds, 1.0, at);
                        let new = |v: &&mut Voice| v.tag == Some(i) && v.started == cx.tick;
                        if let Some(v) = self.voices.last_mut().filter(new) {
                            v.until = Some(cx.tick + 40);
                        }
                    }
                }
                On::Holding { weapon } => {
                    let held = &cx.pack.held;
                    self.hold(
                        cx,
                        i,
                        cue,
                        holding(held, weapon, was),
                        holding(held, weapon, is),
                        is,
                    );
                }
                On::Burning => self.hold(cx, i, cue, burning(was), burning(is), is),
                On::Shot { weapon } if fire(weapon, is) && !fire(weapon, was) => {
                    self.start(cx, &cue.play, &cue.play.sounds, 1.0, me);
                }
                On::Casing { weapon, delay } if fire(weapon, is) && !fire(weapon, was) => {
                    let tick = cx.tick + u32::from(delay);
                    self.later.push(Later {
                        tick,
                        cue: i,
                        src: me,
                        end: false,
                    });
                }
                On::Scope { weapon }
                    if is.local && is.weapon == weapon && is.scope && !was.scope =>
                {
                    self.start(cx, &cue.play, &cue.play.sounds, 1.0, me);
                }
                _ => {}
            }
        }
        if is.health < was.health {
            let lethal = is.health == 0;
            let rocket = self.rocket_at.is_some_and(|t| t + 1 >= cx.tick);
            let w = hit_weapon(is, burning(was), all, rocket);
            debug!(
                "HIT t={} read={} p={} weapon={w:#04x} lethal={lethal}",
                cx.tick, cx.read, is.slot
            );
            self.cues(cx, is, me, |on| {
                matches!(*on, On::Hit { weapon, lethal: l } if weapon == w && l.is_none_or(|l| l == lethal))
            });
            if lethal {
                self.gibs(cx, w, is);
            }
            // A grenade cooked too long goes off in the thrower's hand: no grenade flies.
            if w == GRENADE && [GRENADE_PULL, GRENADE_HOLD].contains(&was.clock.id) {
                self.world(cx, is.pos, |on| *on == On::Explosion { weapon: GRENADE });
            }
        }
    }

    /// A footstep of `kind` on the player's floor (`func_10011EB8`), louder with speed.
    fn footstep(&mut self, cx: &Cx, kind: u8, p: &Seen) {
        let steps = &cx.pack.audio.events.footsteps;
        let surface = p.floor.and_then(|f| steps.surfaces.get(&f));
        let Some(sounds) = surface.and_then(|s| s.get(usize::from(kind))) else {
            return;
        };
        let full = steps.full_speed.max(1.0);
        let scale = 0.5 + 0.5 * p.speed.clamp(0.0, full) / full;
        self.start(cx, &steps.play, sounds, scale, Src::player(p));
    }

    /// Body parts landing after a gory death by `w`: 6-12 of them, near the victim, over 2 s (I: the
    /// count and spread seen on the oracle, not read).
    fn gibs(&mut self, cx: &Cx, w: u8, p: &Seen) {
        let cues = &cx.pack.audio.events.cues;
        for (i, _) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Gibs { weapon: w }) {
            for _ in 0..self.between(6, 12) {
                let (dx, dz) = (self.between(-200, 200), self.between(-200, 200));
                let pos = [p.pos[0] + dx as f32, p.pos[1], p.pos[2] + dz as f32];
                let tick = cx.tick + self.between(10, 60) as u32;
                self.later.push(Later {
                    tick,
                    cue: i,
                    src: Src::at(pos),
                    end: false,
                });
            }
        }
    }

    /// The grenades in flight (`now`; last tick's in `self.grenades`), whose sounds follow them: the
    /// thrower's hiss (its Holding cue, from the throw) goes with its grenade as it leaves the hand;
    /// its first landing plays the Flight cue and the next ones the Bounce cue's sounds in turn,
    /// none past them (V e_gren, sq_e_gren, ctlw_k*: 1447, 1448, 1449 at the three landings 27, 42
    /// and 50 ticks after the launch; the original picks the three in a random order); the fuse's
    /// last 60 ticks; the blast once one is gone, which stops them.
    fn grenades(&mut self, cx: &Cx, now: &[(u32, Grenade)]) {
        let cues = &cx.pack.audio.events.cues;
        for &(birth, g) in now {
            let owner = Owner::Grenade(birth, g.owner);
            let src = Src {
                owner,
                ..Src::at(g.pos)
            };
            let was = self
                .grenades
                .iter()
                .find(|o| (o.0, o.1.owner) == (birth, g.owner));
            let thrower = Owner::Player(g.owner as u16);
            for v in &mut self.voices {
                let hiss = v
                    .tag
                    .is_some_and(|i| cues[i].on == On::Holding { weapon: GRENADE });
                if was.is_none() && v.owner == thrower && hiss {
                    (v.owner, v.follows, v.until) = (owner, None, None);
                }
                if v.owner == owner {
                    v.pos = g.pos;
                }
            }
            let landed = was.is_some_and(|w| w.1.y_vel < 0.0 && g.y_vel > 0.0);
            let n = match self.bounces.iter_mut().find(|b| b.0 == (birth, g.owner)) {
                Some(b) => {
                    b.1 += usize::from(landed);
                    b.1
                }
                None if landed => {
                    self.bounces.push(((birth, g.owner), 1));
                    1
                }
                None => 0,
            };
            for (i, cue) in cues.iter().enumerate() {
                let sounds = match cue.on {
                    On::Flight {
                        weapon: GRENADE, ..
                    } if landed && n == 1 => Some(&cue.play.sounds[..]),
                    On::Bounce { weapon: GRENADE } if landed && n > 1 => {
                        cue.play.sounds.get(n - 2).map(core::slice::from_ref)
                    }
                    On::Fuse { weapon: GRENADE }
                        if cx.tick + 60 >= g.fuse
                            && !self
                                .voices
                                .iter()
                                .any(|v| v.owner == owner && v.tag == Some(i)) =>
                    {
                        Some(&cue.play.sounds[..])
                    }
                    _ => None,
                };
                if let Some(sounds) = sounds {
                    self.start(
                        cx,
                        &cue.play,
                        sounds,
                        1.0,
                        Src {
                            tag: Some(i),
                            ..src
                        },
                    );
                }
            }
        }
        let old = std::mem::take(&mut self.grenades);
        for (birth, g) in old
            .iter()
            .filter(|o| !now.iter().any(|n| (n.0, n.1.owner) == (o.0, o.1.owner)))
        {
            let owner = Owner::Grenade(*birth, g.owner);
            self.stop(|v| v.owner == owner);
            self.bounces.retain(|b| b.0 != (*birth, g.owner));
            self.world(cx, g.pos, |on| *on == On::Explosion { weapon: GRENADE });
        }
        self.grenades = old;
        self.grenades.clear();
        self.grenades.extend_from_slice(now);
    }

    /// Plays every cue `matches` picks at `pos`, nobody's.
    fn world(&mut self, cx: &Cx, pos: [f32; 3], matches: impl Fn(&On) -> bool) {
        for cue in (cx.pack.audio.events.cues.iter()).filter(|c| matches(&c.on)) {
            self.start(cx, &cue.play, &cue.play.sounds, 1.0, Src::at(pos));
        }
    }

    /// The match's sounds: weapon 0x14's shots (one sound a tick in flight, the wall's when one is
    /// gone without hurting anyone near it), pickups taken (by the nearest player), Total War's gas
    /// (at the listener while it is out), the background (from the first tick of play to the match's
    /// end) and the map's bubbling spots.
    fn match_sounds(&mut self, cx: &Cx, m: &MatchView, players: &[(Seen, Seen)]) {
        let cues = &cx.pack.audio.events.cues;
        for (i, _) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Gas) {
            let tagged = |v: &Voice| v.tag == Some(i);
            match (self.gas, m.gas) {
                (false, true) => {
                    let src = Src {
                        follows: Some(cx.ear.slot),
                        tag: Some(i),
                        ..Src::at(cx.ear.pos)
                    };
                    self.start(cx, &cues[i].play, &cues[i].play.sounds, 1.0, src);
                }
                (true, false) => self.stop(tagged),
                _ => {}
            }
        }
        self.gas = m.gas;
        let old = std::mem::take(&mut self.shots);
        for (k, &pos) in m.shots.iter().enumerate() {
            for cue in cues
                .iter()
                .filter(|c| matches!(c.on, On::Flight { weapon: SHOTS, .. }))
            {
                let sounds = &cue.play.sounds;
                if let Some(s) = sounds.get(cx.tick as usize % sounds.len().max(1)) {
                    let src = Src {
                        owner: Owner::Shot(k),
                        ..Src::at(pos)
                    };
                    self.start(cx, &cue.play, core::slice::from_ref(s), 1.0, src);
                }
            }
        }
        for pos in gone(&old, &m.shots, 2.0 * SHOT_STEP) {
            let hurt =
                |p: &(Seen, Seen)| p.1.health < p.0.health && dist2(p.1.pos, pos) < 150.0 * 150.0;
            if !players.iter().any(hurt) {
                self.world(cx, pos, |on| *on == On::Impact { weapon: SHOTS });
            }
        }
        self.shots = old;
        self.shots.clone_from(&m.shots);
        let taken = m.taken & !self.taken;
        self.taken = m.taken;
        for pickup in
            (cx.pack.pickups.iter()).filter(|p| p.placement < 64 && taken >> p.placement & 1 != 0)
        {
            let d = |p: &Seen| dist2(p.pos, pickup.position);
            let Some(p) = players
                .iter()
                .map(|p| p.1)
                .min_by(|a, b| d(a).total_cmp(&d(b)))
            else {
                continue;
            };
            let gives = pickup.gives;
            self.cues(cx, &p, Src::player(&p), |on| *on == On::Pickup { gives });
        }
        match (m.phase, &cx.pack.background) {
            (0, Some(b)) if !self.background => {
                self.background = true;
                let v = Voice {
                    id: BACKGROUND,
                    owner: Owner::World,
                    channel: Channel::Free,
                    tag: None,
                    pos: cx.ear.pos,
                    follows: None,
                    flat: true,
                    volume: f32::from(b.volume),
                    own: None,
                    near: 0.0,
                    far: 0.0,
                    base: 1.0,
                    speed: 1.0,
                    looped: true,
                    until: None,
                    fade: None,
                    started: cx.tick,
                    stopped: false,
                    control: None,
                };
                self.push(cx, v);
            }
            (2, _) => self.stop(|v| v.id == BACKGROUND),
            _ => {}
        }
        if m.phase < 2 {
            for (i, e) in cx.pack.emitters.iter().enumerate() {
                let Some(mut b) = self.bursts.get(i).copied() else {
                    break;
                };
                if b.left == 0 && (self.rng.next_f32()) < e.start_chance {
                    b = Burst {
                        left: self.between(1, 5) as u32,
                        next: cx.tick,
                    };
                }
                if b.left > 0 && cx.tick >= b.next {
                    let at = e.at.map(f32::from);
                    self.start(cx, &e.play, &e.play.sounds, 1.0, Src::at(at));
                    b.left -= 1;
                    b.next = cx.tick + (self.between(1, 29) + self.between(8, 20)) as u32;
                }
                self.bursts[i] = b;
            }
        }
    }

    /// Each voice where its player is, its `ticks` and fade run out, on the mixer once its samples
    /// are in (a loop waits for them; a sound that does not loop plays only from its first tick),
    /// at this tick's ear volumes; finished ones go.
    fn upkeep(&mut self, cx: &Cx, players: &[(Seen, Seen)], out: Option<(&Output, &Assets<Pcm>)>) {
        for v in &mut self.voices {
            if let Some(p) = (v.follows).and_then(|s| players.iter().find(|p| p.1.slot == s)) {
                v.pos = p.1.pos;
            }
            if v.until.is_some_and(|u| cx.tick >= u) && v.fade.is_none() {
                v.fade = Some(cx.tick);
            }
            if v.fade.is_some_and(|f| cx.tick >= f + FADE) {
                v.stopped = true;
            }
            if v.stopped {
                if let Some(c) = &v.control {
                    c.stop.store(true, Relaxed);
                }
                continue;
            }
            let ears = v.ears(&cx.ear, cx.tick, self.muted);
            if v.control.is_none()
                && (v.looped || v.started == cx.tick)
                && let Some((out, pcms)) = out
                && let Some(pcm) = out.sounds.get(&v.id).and_then(|h| pcms.get(h))
            {
                let sound = cx.pack.audio.sounds.sounds.get(&v.id);
                v.control = Some(out.play(pcm, sound, v.speed, ears));
            }
            if let Some(c) = &v.control {
                for (a, e) in c.ears.iter().zip(ears) {
                    a.store(e.to_bits(), Relaxed);
                }
            }
        }
        self.voices.retain(|v| {
            !v.stopped && (v.control.as_ref()).map_or(v.looped, |c| !c.done.load(Relaxed))
        });
    }
}

/// The tick's sounds (see the module doc): each player's against last tick ([`Heard`]), the
/// grenades', the match's, then every voice's volumes. Nothing plays until the local player (the
/// listener) is in.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hear(
    timeline: Res<LocalTimeline>,
    pack: Res<Pack>,
    mut sounds: ResMut<Sounds>,
    output: Option<Res<Output>>,
    pcms: Option<Res<Assets<Pcm>>>,
    game: Query<&MatchView>,
    mut players: Query<(
        Entity,
        &Player,
        &Combat,
        &Anim,
        Option<&Fighting>,
        Has<Predicted>,
        Option<&mut Heard>,
        Option<&PlayerId>,
    )>,
    listener: Query<(&Player, &Combat, &FollowCamera)>,
    (playing, outros): (Res<crate::outro::Playing>, Res<crate::outro::Outros>),
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let s = &mut *sounds;
    let mut all = std::mem::take(&mut s.players);
    all.clear();
    let mut grenades: &[(u32, Grenade)] = &[];
    for (entity, p, c, anim, fighting, predicted, heard, id) in &mut players {
        let (weapon, drawn) = match fighting {
            Some(f) if predicted => (f.state.weapon.unwrap_or(0), f.state.drawn),
            _ => (c.weapon, c.drawn),
        };
        if let Some(f) = fighting.filter(|_| predicted) {
            grenades = &f.grenades;
        }
        let bot = id.is_some_and(|i| matches!(i.0, PeerId::Local(_)));
        let seen = Seen {
            slot: c.slot,
            side: c.side,
            pos: p.pos,
            clock: anim.0,
            weapon,
            drawn,
            health: c.health,
            reaction: c.anim,
            scope: p.scope_yaw.is_some(),
            local: predicted,
            floor: p.floor.map(|f| (f.surface & 0x1F) as u8),
            speed: p.xz_speed,
            carry: (c.carrying).then_some((c.team, bot)),
            flag: c.flag.map(|f| (f, bot)),
        };
        match heard {
            Some(mut h) => {
                all.push((h.0, seen));
                h.0 = seen;
            }
            None => {
                commands.entity(entity).insert(Heard(seen));
            }
        }
    }
    let (Ok(m), Ok((p, c, camera))) = (game.single(), listener.single()) else {
        s.players = all;
        return;
    };
    let read = m
        .start
        .filter(|&st| st != u32::MAX && tick >= st)
        .map_or(-1, |st| i64::from(m.first_read + (tick - st)));
    let cx = Cx {
        pack: &pack,
        tick,
        read,
        ear: Ear::new(c.slot, p.pos, camera),
    };
    if !s.primed {
        s.primed = true;
        (s.rockets, s.shots, s.taken) = (m.rockets.clone(), m.shots.clone(), m.taken);
        s.grenades = grenades.to_vec();
    }
    let old = std::mem::take(&mut s.rockets);
    for pos in gone(&old, &m.rockets, 2.0 * ROCKET_SPEED) {
        s.rocket_at = Some(tick);
        s.world(&cx, pos, |on| *on == On::Explosion { weapon: ROCKETS });
    }
    s.rockets = old;
    s.rockets.clone_from(&m.rockets);
    s.due(&cx);
    for (was, is) in &all {
        s.player(&cx, was, is, &all);
    }
    s.grenades(&cx, grenades);
    s.match_sounds(&cx, m, &all);
    if let Some((i, _)) =
        (playing.0).filter(|&(i, k)| outros.0.get(i).is_some_and(|o| Some(k) == o.voice_frame()))
    {
        let v = Voice {
            id: VOICE - i as u16,
            owner: Owner::World,
            channel: Channel::Free,
            tag: None,
            pos: cx.ear.pos,
            follows: None,
            flat: true,
            volume: 32767.0,
            own: None,
            near: 0.0,
            far: 0.0,
            base: MIX,
            speed: 1.0,
            looped: false,
            until: None,
            fade: None,
            started: tick,
            stopped: false,
            control: None,
        };
        s.push(&cx, v);
    }
    let out = output.as_deref().zip(pcms.as_deref());
    s.upkeep(&cx, &all, out);
    s.players = all;
}

/// The sound device (rodio's mixer) and every sound's samples by id (the background as
/// [`BACKGROUND`]), loading.
#[derive(Resource)]
struct Output {
    device: rodio::MixerDeviceSink,
    sounds: HashMap<u16, Handle<Pcm>>,
}

impl Output {
    /// Puts a sound on the mixer at `speed` × its rate, starting at these ear volumes.
    fn play(&self, pcm: &Pcm, sound: Option<&Sound>, speed: f32, ears: [f32; 2]) -> Arc<Control> {
        let control = Arc::new(Control::default());
        for (a, e) in control.ears.iter().zip(ears) {
            a.store(e.to_bits(), Relaxed);
        }
        let playing = Playing::new(pcm, sound, speed, control.clone());
        self.device.mixer().add(playing);
        control
    }
}

/// Opens the sound device and loads every sound: natively at once; in the browser on the first key
/// press or click (Chrome lets an AudioContext made after a user gesture run), clearing the hint.
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pack: Res<Pack>,
    outros: Res<crate::outro::Outros>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    mut tried: Local<bool>,
) {
    let gesture =
        keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    if *tried || (cfg!(target_family = "wasm") && !gesture) {
        return;
    }
    *tried = true;
    #[cfg(target_family = "wasm")]
    note("");
    let device = rodio::DeviceSinkBuilder::from_default_device()
        .map(|b| b.with_error_callback(|e| warn!("sound: {e}")))
        .and_then(|b| b.open_stream());
    let mut device = match device {
        Ok(d) => d,
        Err(e) => {
            warn!("sound: no output device ({e}): the match plays silent");
            return;
        }
    };
    device.log_on_drop(false);
    let config = device.config();
    info!(
        "sound: output open, {} channels at {} Hz",
        config.channel_count(),
        config.sample_rate()
    );
    let load = |file: &str| assets.load::<Pcm>(format!("audio/{file}"));
    let mut sounds: HashMap<u16, Handle<Pcm>> = (pack.audio.sounds.sounds.iter())
        .map(|(id, s)| (*id, load(&s.file)))
        .collect();
    if let Some(b) = &pack.background {
        sounds.insert(BACKGROUND, load(&b.file));
    }
    for (n, o) in outros.0.iter().enumerate() {
        if let Some(v) = &o.voice {
            sounds.insert(VOICE - n as u16, load(&v.file));
        }
    }
    commands.insert_resource(Output { device, sounds });
}

/// Set as the mixer first pulls a sound's samples: the output runs.
static PULLED: AtomicBool = AtomicBool::new(false);

/// Logs once that the output runs (in the browser, that the AudioContext started).
fn running(mut said: Local<bool>) {
    if !*said && PULLED.load(Relaxed) {
        *said = true;
        info!("sound: output running");
    }
}

/// M mutes and unmutes (the page's `#sound` line says so in the browser).
fn mute(keys: Res<ButtonInput<KeyCode>>, mut sounds: ResMut<Sounds>) {
    if keys.just_pressed(MUTE_KEY) {
        sounds.muted = !sounds.muted;
        info!("sound {}", if sounds.muted { "muted" } else { "on" });
        #[cfg(target_family = "wasm")]
        note(if sounds.muted { "Sound muted (M)" } else { "" });
    }
}

/// The page's `#sound` line.
#[cfg(target_family = "wasm")]
fn note(text: &str) {
    let line = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("sound"));
    if let Some(line) = line {
        line.set_text_content(Some(text));
    }
}

/// A WAV file's samples: 16-bit PCM, interleaved.
#[derive(Asset, TypePath)]
pub struct Pcm {
    rate: u32,
    channels: u16,
    samples: Arc<[i16]>,
}

/// Reads `.wav` files into [`Pcm`].
#[derive(Default, TypePath)]
pub struct WavLoader;

impl AssetLoader for WavLoader {
    type Asset = Pcm;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<Pcm, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a 16-bit PCM WAV");
        wav(&bytes).ok_or_else(bad)
    }

    fn extensions(&self) -> &[&str] {
        &["wav"]
    }
}

/// A RIFF WAVE file's samples, 16-bit PCM only (the pack's).
fn wav(b: &[u8]) -> Option<Pcm> {
    if b.get(..4)? != b"RIFF" || b.get(8..12)? != b"WAVE" {
        return None;
    }
    let u16_at = |s: &[u8], i: usize| Some(u16::from_le_bytes(s.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |s: &[u8], i: usize| Some(u32::from_le_bytes(s.get(i..i + 4)?.try_into().ok()?));
    let (mut format, mut i) = (None, 12);
    while i + 8 <= b.len() {
        let len = u32_at(b, i + 4)? as usize;
        let body = &b[i + 8..(i + 8).saturating_add(len).min(b.len())];
        match &b[i..i + 4] {
            b"fmt " => {
                format = Some((
                    u16_at(body, 0)?,
                    u16_at(body, 2)?,
                    u32_at(body, 4)?,
                    u16_at(body, 14)?,
                ))
            }
            b"data" => {
                let (pcm, channels, rate, bits) = format?;
                if pcm != 1 || bits != 16 || channels == 0 || rate == 0 {
                    return None;
                }
                let samples = body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| i16::from_le_bytes(*s));
                return Some(Pcm {
                    rate,
                    channels,
                    samples: samples.collect(),
                });
            }
            _ => {}
        }
        i += 8 + len + len % 2;
    }
    None
}

/// A playing sound's controls, shared with its [`Playing`] on the mixer's thread: each ear's volume
/// (f32 bits), stop (fade out over the release), done (it ended).
#[derive(Default)]
struct Control {
    ears: [AtomicU32; 2],
    stop: AtomicBool,
    done: AtomicBool,
}

/// An ADSR envelope in frames and levels out of 1 ([`conker_assets::audio::Envelope`]): linear ramps
/// (I: the game's envelope mixer's curve is not read).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Env {
    attack: f32,
    attack_level: f32,
    /// None: holds the attack level.
    decay: Option<f32>,
    decay_level: f32,
    release: f32,
}

impl Env {
    const FLAT: Self = Self {
        attack: 0.0,
        attack_level: 1.0,
        decay: None,
        decay_level: 1.0,
        release: 0.0,
    };

    fn new(e: &conker_assets::audio::Envelope, frames_per_us: f32) -> Self {
        let frames = |us: i32| us.max(0) as f32 * frames_per_us;
        Self {
            attack: frames(e.attack_us),
            attack_level: f32::from(e.attack_volume) / 127.0,
            decay: (e.decay_us >= 0).then(|| frames(e.decay_us)),
            decay_level: f32::from(e.decay_volume) / 127.0,
            release: frames(e.release_us),
        }
    }

    fn level(&self, age: f32) -> f32 {
        if age < self.attack {
            return self.attack_level * age / self.attack;
        }
        let t = age - self.attack;
        match self.decay {
            Some(d) if t < d => self.attack_level + (self.decay_level - self.attack_level) * t / d,
            Some(_) => self.decay_level,
            None => self.attack_level,
        }
    }
}

/// A sound on the mixer: its samples (a loop's stretch repeating until it is stopped), through its
/// envelope, as two channels at its [`Control`]'s ear volumes (smoothed over about 20 ms), at its
/// rate × the pitch's speed. ponytail: a loop's `count` is not kept (no sound of the pack has one).
struct Playing {
    samples: Arc<[i16]>,
    channels: usize,
    rate: SampleRate,
    frame: usize,
    end: usize,
    loop_start: Option<usize>,
    env: Env,
    age: u32,
    /// The age and level the stop began its release at.
    release: Option<(u32, f32)>,
    ears: [f32; 2],
    smooth: f32,
    right: Option<f32>,
    control: Arc<Control>,
}

impl Playing {
    /// `sound` None: the background (loops whole, no envelope).
    fn new(pcm: &Pcm, sound: Option<&Sound>, speed: f32, control: Arc<Control>) -> Self {
        let channels = usize::from(pcm.channels);
        let frames = pcm.samples.len() / channels;
        let rate = (pcm.rate as f32 * speed).max(1.0);
        let (looped, env) = match sound {
            Some(s) => (
                (s.looped.as_ref()).map(|l| (l.start as usize, (l.end as usize).min(frames))),
                Env::new(&s.envelope, rate / 1e6),
            ),
            None => (Some((0, frames)), Env::FLAT),
        };
        let looped = looped.filter(|l| l.0 < l.1);
        Self {
            samples: pcm.samples.clone(),
            channels,
            rate: SampleRate::new(rate.round() as u32).unwrap_or(SampleRate::MIN),
            frame: 0,
            end: looped.map_or(frames, |l| l.1),
            loop_start: looped.map(|l| l.0),
            env,
            age: 0,
            release: None,
            ears: control
                .ears
                .each_ref()
                .map(|e| f32::from_bits(e.load(Relaxed))),
            smooth: 1.0 - (-1.0 / (rate * 0.02)).exp(),
            right: None,
            control,
        }
    }

    fn done(&mut self) -> Option<f32> {
        self.control.done.store(true, Relaxed);
        None
    }
}

impl Drop for Playing {
    fn drop(&mut self) {
        self.control.done.store(true, Relaxed);
    }
}

impl Iterator for Playing {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        if self.age == 0 {
            PULLED.store(true, Relaxed);
        }
        let mut level = self.env.level(self.age as f32);
        if self.release.is_none() && self.control.stop.load(Relaxed) {
            self.release = Some((self.age, level));
        }
        if let Some((at, from)) = self.release {
            let t = (self.age - at) as f32;
            if t >= self.env.release {
                return self.done();
            }
            level = from * (1.0 - t / self.env.release);
        }
        if self.frame >= self.end {
            match self.loop_start {
                Some(start) => self.frame = start,
                None => return self.done(),
            }
        }
        let i = self.frame * self.channels;
        let left = f32::from(self.samples[i]) / 32768.0;
        let right = match self.channels {
            1 => left,
            _ => f32::from(self.samples[i + 1]) / 32768.0,
        };
        for (ear, target) in self.ears.iter_mut().zip(&self.control.ears) {
            *ear += (f32::from_bits(target.load(Relaxed)) - *ear) * self.smooth;
        }
        self.frame += 1;
        self.age += 1;
        self.right = Some(right * level * self.ears[1]);
        Some(left * level * self.ears[0])
    }
}

impl Source for Playing {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        STEREO
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(id: u16, time: f32, rate: f32) -> AnimPlayer {
        AnimPlayer {
            id,
            time,
            rate,
            ..default()
        }
    }

    /// The oracle's footsteps (e_w09: clip 2's event at 1.0 fired from 0.426 and from 0.0087, the
    /// one at 15.0 from 14.33, rate 1.73) and a clip's first tick.
    #[test]
    fn clip_events_fire_as_the_clock_passes_them() {
        let pass = |a: f32, b: f32, t: f32| passes(&clock(2, a, 1.73), &clock(2, b, 1.73), t);
        assert!(pass(0.426, 2.156, 1.0) && pass(0.0087, 1.7387, 1.0) && pass(14.33, 16.06, 15.0));
        assert!(!pass(1.0, 2.73, 1.0), "fired last tick");
        assert!(
            pass(27.5, 1.2, 1.0) && pass(27.5, 1.2, 28.0),
            "round the loop"
        );
        let new = |t: f32| passes(&clock(0x7F, 40.0, 1.0), &clock(0x58, 1.0, 1.0), t);
        assert!(new(0.0) && new(1.0) && !new(2.0));
    }

    #[test]
    fn a_wav_reads_back() {
        let samples: [i16; 4] = [0, -1, 32767, -32768];
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        b.extend(b"LIST\x03\0\0\0abc\0"); // odd length: padded
        b.extend(b"fmt \x10\0\0\0\x01\0\x02\0\x44\xAC\0\0\x10\xB1\x02\0\x04\0\x10\0");
        b.extend(b"data\x08\0\0\0");
        b.extend(&data);
        let pcm = wav(&b).unwrap();
        assert_eq!(
            (pcm.rate, pcm.channels, &pcm.samples[..]),
            (44100, 2, &samples[..])
        );
        b[46] = 8; // 8-bit
        assert!(wav(&b).is_none());
        assert!(wav(b"RIFF").is_none());
    }

    /// A loop after an intro (sound 719: 1569..14371), a stop's release, and the envelope.
    #[test]
    fn loops_keep_their_intro_and_stop_after_the_release() {
        let pcm = Pcm {
            rate: 1_000_000,
            channels: 1,
            samples: (0..6).map(|i| i * 1000).collect(),
        };
        let mut sound = Sound {
            file: String::new(),
            rate: 0,
            volume: 127,
            pan: 64,
            envelope: conker_assets::audio::Envelope {
                attack_us: 0,
                attack_volume: 127,
                decay_us: -1,
                decay_volume: 127,
                release_us: 2,
            },
            looped: Some(conker_assets::audio::Loop {
                start: 2,
                end: 5,
                count: None,
            }),
            detune_after_attack: None,
        };
        let control = Arc::new(Control::default());
        control
            .ears
            .iter()
            .for_each(|e| e.store(1f32.to_bits(), Relaxed));
        let mut p = Playing::new(&pcm, Some(&sound), 1.0, control.clone());
        let left: Vec<f32> = (0..9)
            .map(|_| (p.next().unwrap() * 32768.0).round())
            .collect();
        // Frames 0 1 2 3 4 2 3 4 2 …, each as left then right.
        assert_eq!(
            left.iter().step_by(2).copied().collect::<Vec<_>>(),
            [0.0, 1000.0, 2000.0, 3000.0, 4000.0]
        );
        control.stop.store(true, Relaxed);
        let rest: Vec<f32> = p.by_ref().collect();
        assert_eq!(
            rest.len(),
            1 + 2 * 2,
            "the pending right sample, then 2 frames of release"
        );
        assert!(control.done.load(Relaxed));
        // Sound 176: decays to 0 over 287.5 ms.
        sound.envelope.decay_volume = 0;
        sound.envelope.decay_us = 100;
        let env = Env::new(&sound.envelope, 1.0);
        assert_eq!(
            [0.0, 50.0, 100.0, 500.0].map(|a| env.level(a)),
            [1.0, 0.5, 0.0, 0.0]
        );
    }

    #[test]
    fn hits_name_their_weapon() {
        let at = |slot, x: f32, weapon| Seen {
            slot,
            side: Side::Tediz,
            pos: [x, 0.0, 0.0],
            clock: AnimPlayer::default(),
            weapon,
            drawn: true,
            health: 6,
            reaction: 0,
            scope: false,
            local: false,
            floor: None,
            speed: 0.0,
            carry: None,
            flag: None,
        };
        let victim = Seen {
            reaction: conker_anim::combat::HIT,
            ..at(0, 0.0, 0)
        };
        let all = [
            (at(1, 500.0, 0x39), at(1, 500.0, 0x39)),
            (at(2, 900.0, 0x09), at(2, 900.0, 0x09)),
        ];
        assert_eq!(
            hit_weapon(&victim, false, &all, false),
            0x39,
            "the nearest gun"
        );
        assert_eq!(hit_weapon(&victim, false, &all[1..], false), 0x09);
        assert_eq!(
            hit_weapon(&victim, true, &[], false),
            0x15,
            "a burning victim"
        );
        let blast = Seen {
            reaction: BLOWN_UP,
            ..victim
        };
        assert_eq!(
            (
                hit_weapon(&blast, false, &all, true),
                hit_weapon(&blast, false, &all, false)
            ),
            (ROCKETS, GRENADE)
        );
        let kill = Seen {
            reaction: DEATH_SHOT,
            ..victim
        };
        assert_eq!(hit_weapon(&kill, false, &all, false), 0x24);
    }

    #[test]
    fn projectiles_gone_are_the_ones_not_carried_on() {
        let old = [[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [5000.0, 0.0, 0.0]];
        let new = [[150.0, 0.0, 0.0], [5100.0, 0.0, 0.0], [0.0, 9.0, 0.0]];
        assert_eq!(
            gone(&old, &new, 400.0).collect::<Vec<_>>(),
            [[1000.0, 0.0, 0.0]]
        );
        assert_eq!(gone(&old, &[], 400.0).count(), 3);
    }
}
