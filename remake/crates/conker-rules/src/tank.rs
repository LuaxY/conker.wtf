//! Tank (scenario 1, scene 0x24): every player a tank (model 119, 6 hp), free for all, ranked by
//! kills as Deathmatch. The hull drives by conker-sim's `tank`; here the gun (its pitch, the shell
//! on Z), the hits, the deaths and wrecks, the power-ups, and the gas bomb's props, which the AI
//! script (0xB8, run by conker-bots) drives as Total War's gas ([`crate::gas`]): the bomb on its
//! post (placement 12: shown, touched, its toucher in +0x80), the bunkers' doors (placements 0-7:
//! sliding open while +0x80 is 0, shut while 1), the countdown and the gas, the detonation's hits.
//! Research: `../lab/research/mp-original/tank.md` (V t1, f1-f3, p1, c1, b1).

use crate::damage::{Hitbox, MG_BULLET};
use crate::game::{Action, Arena, Event, Match, Seen};
use crate::gas::{Gas, Prop};
use crate::projectile::{Bullet, hits_sphere, raycast, rise_along};
use crate::weapon::BULLET_SPEED;
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{BUTTON_A, BUTTON_Z, Input, Other, atan2_u16, cos_sin};
use conker_sim::tank::{self as hull, Drive, KNOCK, PITCH_MAX, PITCH_MIN};

/// Every Tank player's class (model 119; V b1, s0).
pub const CLASS: u8 = 8;
/// Z fires again 34 frames after a shot (the fire clip's length; V f3); a press, not a hold.
pub const REFIRE: u32 = 34;
/// A bot's fire clip runs 51 frames, and its presses in it fire nothing (V tf2: 40 whole bot
/// clips, all 51; the 34 presses 41-42 frames in ignored; I why, f3's human clip runs 34).
pub const BOT_REFIRE: u32 = 51;
/// The shell (`func_1506C460`; V c1 61-72): 296.5 along the gun's yaw (half of it a frame), its
/// pitch's share up; falling as a player does (`y += vy/2 − 0.275·5`, `vy −= 5`); gone at a wall
/// or the ground. It starts 128 ahead and 113 up (V c1 61: 128.2, 112.6).
pub const SHELL_SPEED: f32 = 296.5;
pub const SHELL_GRAVITY: f32 = 5.0;
pub const MUZZLE: [f32; 2] = [128.0, 113.0];
/// The scope's shot (power-up 0x02, R held; `func_15070300` → `func_151C0698`): no shell, the
/// bullet pool's, from the scope camera's eye, 27.3 ahead and 111.6 up, level along the turret (V
/// sc5 69: the eye at (7127.7, −789.4, 670.7) over P1 at (7155, −901, 670) facing west, its look
/// level; sc3: 150 off the line, 4 misses; sc4: a target 245 above, 4 misses). It flies as the
/// pool's bullets, 300 a frame, from the frame after Z (V sc1, sc5: the target 600 ahead loses 1 hp
/// two frames after the press, a shell's three).
pub const SCOPE_EYE: [f32; 2] = [27.3, 111.6];
/// A tank's body for the shells (model 119 has no body parts, so `func_15081690` takes
/// `func_15081C20`): the ellipsoid of radius obj+0xD2 across and obj+0xD4 up, centred obj+0xD6
/// over the feet (V f2, b1 RAM: 113, 90, 52). V f2: 98 off the line hits, 148 misses; b1 to read
/// 9023: it calls 58 of the 60 hits (a cylinder of 150 from the feet to 180 calls 81).
pub const BODY: Hitbox = Hitbox {
    radius: 113.0,
    half: 90.0,
    centre: 52.0,
};
/// The hit clip (7): 15 frames, the knock-back the first 13 (V f1 44-58).
pub const HIT_FRAMES: u8 = 15;
pub const KNOCK_FRAMES: u8 = 13;
/// A dead tank is back (on a record, 6 hp, 150 immune) on its 88th frame after the killing hit (V
/// f1 248 → 336, c1 423 → 510; tank.md's +91 is the teleport's read in b1's sampled RAM, I).
pub const RESPAWN: u8 = 88;
/// Its wreck (prop 18 + slot, `[3, 95]`) shows where it died for 330 frames (V b1 1836 → 2166).
pub const WRECK_FRAMES: u32 = 330;
/// The tank's clips (obj+0x84): 1 the fire clip, 6 the bomb's death, 7 a shell's hit or death.
pub const CLIP_FIRE: u8 = 1;
pub const CLIP_BOMB: u8 = 6;
pub const CLIP_HIT: u8 = 7;
/// The power-ups' bits (s126+0x128; pickups 0x3001-0x3004) and what s126+0x12C starts at: the
/// shield's 1800 VIs (immune), the boost's 600 VIs of A held, 3 one-shot shells; the scope's (R
/// with it: stopped) has no timer seen (I: kept until replaced or dead).
pub const SHIELD: u8 = 0x80;
pub const BOOST: u8 = 0x01;
pub const SHELLS: u8 = 0x10;
pub const SCOPE: u8 = 0x02;
pub fn power_timer(bit: u8) -> u16 {
    match bit {
        SHIELD => 1800,
        BOOST => 600,
        SHELLS => 3,
        _ => 0,
    }
}
/// The bomb's prop (placement 12, handle 0x3005) on its post (V: the ROM's placement).
pub const BOMB: u16 = 12;
pub const POST: [f32; 3] = [-20.0, -532.0, -108.0];
/// The bunkers, triggers 0x11-0x14 (S, W, N, E): their centres (tank.md §4.1).
pub const BUNKERS: [[f32; 3]; 4] = [
    [-830.0, -1555.0, -7062.0],
    [-7415.0, -952.0, -707.0],
    [779.0, -1562.0, 7030.0],
    [7393.0, -905.0, 704.0],
];
/// The doors slide 8 a frame over 350 (V b1 23-67).
pub const DOOR_OPEN: i16 = 350;
pub const DOOR_STEP: i16 = 8;
/// The best target for the gun's pitch: within 35° of the turret, nearer than 7000 (C); the pitch
/// turns at most 4° a frame toward it, else relaxes ×0.9.
const AIM_CONE: f32 = 35.0;
const AIM_RANGE: f32 = 7000.0;
const PITCH_RATE: f32 = 4.0;

const DEG: f32 = 65536.0 / 360.0;

/// A shell in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Shell {
    pub owner: usize,
    pub pos: [f32; 3],
    /// Its yaw (u16), horizontal speed and vertical velocity.
    pub angle: u16,
    pub speed: f32,
    pub y_vel: f32,
    /// Power-up 0x10's: kills at once.
    pub deadly: bool,
    /// The frame it was fired (it flies from the next).
    pub born: u32,
}

/// A wreck where a tank died, until a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wreck {
    pub pos: [f32; 3],
    pub facing: u16,
    pub until: u32,
}

/// The match's Tank state besides the players and the gas's props.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tank {
    pub shells: Vec<Shell>,
    /// The bomb on its post, as the engine keeps a prop for the script: shown, its toucher + 1.
    pub bomb: Prop,
    /// Who carries the bomb (the script's 0x4B, until its 0x4C), and who last let it go.
    pub carrier: Option<usize>,
    pub last: Option<usize>,
    /// The bunker it went off in (0-3, S W N E), from its plant until the next.
    pub planted: Option<u8>,
    /// Per slot, its last wreck.
    pub wrecks: Vec<Option<Wreck>>,
    /// This frame's bots whose class script set their mover's accel and brake ×10 (op 0x67,
    /// obj+0x247/+0x248; [`crate::gas::Effect::TankRates`]); the others' are 0x32 each.
    pub rates: Vec<(usize, [u8; 2])>,
    /// This frame's bots with a target (s126+0x129, its script's enemy): where their gun aims, the
    /// target's feet led by its speed ([`crate::gas::Effect::TankAim`]).
    pub aims: Vec<(usize, [f32; 3])>,
    /// The scope's shots in flight ([`SCOPE_EYE`]), with the frame each was fired.
    pub bullets: Vec<(u32, Bullet)>,
}

/// A tank's touch of a power-up or the bomb (`func_1511DF6C`): a bot's (s126+0x84 set) reaches
/// 1.4 times as far (`0x800A3304`, C; V h1: every tank's s126+0x1AF is 64, the bots' +0x84 1).
/// ponytail: the original does so for every mode's bots; Race and Tank port it.
pub fn touches(pickup: [f32; 3], f: &crate::Fighter, reach: u8) -> bool {
    let scale = if f.bot { crate::race::AUTO_REACH } else { 1.0 };
    crate::weapon::within(pickup, f.body.pos, 100.0 * f32::from(reach) / 64.0 * scale)
}

/// The doors' frame: toward open while +0x80 is 0 (from the match start), shut while 1. Their
/// offsets ride in [`Prop::lift`] (0 shut, [`DOOR_OPEN`] open).
pub fn doors(gas: &mut Gas) {
    for p in &mut gas.props[..8] {
        p.lift = match p.flag {
            0 => (p.lift + DOOR_STEP).min(DOOR_OPEN),
            _ => (p.lift - DOOR_STEP).max(0),
        };
    }
}

/// The doors' collision (the pack's, shut at home): solid while shut, out of reach otherwise.
/// ponytail: no slide (the collision only shifts in y); a sliding door is open.
pub fn seat(world: &mut CollisionWorld, lifts: [i16; 8]) {
    for (n, lift) in (0..8).zip(lifts) {
        world.shift(n, if lift == 0 { 0 } else { 20_000 });
    }
}

/// The bunker nearest `pos` (0-3).
pub fn bunker(pos: [f32; 3]) -> u8 {
    let d = |b: &[f32; 3]| (b[0] - pos[0]).powi(2) + (b[2] - pos[2]).powi(2);
    (0..4)
        .min_by(|&a, &b| d(&BUNKERS[a]).total_cmp(&d(&BUNKERS[b])))
        .unwrap_or(0) as u8
}

/// Degrees within ±180.
fn wrap180(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn deg(a: u16) -> f32 {
    f32::from(a) / DEG
}

fn to_u16(d: f32) -> u16 {
    (d.rem_euclid(360.0) * DEG) as u32 as u16
}

/// The hull's pitch in degrees along its facing on its floor (obj+0xB8, up positive; V t1: it
/// follows the ground). The client tilts the hull by it.
pub fn hull_pitch(b: &conker_sim::movement::PlayerState, world: &CollisionWorld) -> f32 {
    let (c, s) = cos_sin(b.facing);
    let rise = (b.floor)
        .filter(|_| !b.airborne)
        .map_or(0.0, |f| rise_along(world.tris[f.tri as usize].normal, c, s));
    let a = atan2_u16(rise, 1.0);
    wrap180(deg(a))
}

/// `func_150ED298`: how far short (+) or past (−) a point `d` ahead (at least 100) and `h` above
/// the muzzle a shell fired at `pitch` degrees comes down, at the later time its height is `h`
/// (148.25 a frame each way, 1.25·t² of fall: C, RAM 0x800A159C/A0); out of its reach, the pitch
/// it lacks × 1000.
fn range_error(d: f32, h: f32, pitch: f32) -> f32 {
    const STEP: f32 = 0.5 * SHELL_SPEED;
    let (s, c) = pitch.to_radians().sin_cos();
    let vy = STEP * s;
    let disc = vy * vy - 5.0 * h;
    if disc <= 0.0 {
        let need = ((5.0 * h).sqrt() / STEP).min(1.0).asin().to_degrees();
        return (need - pitch) * 1000.0;
    }
    let t = (vy + disc.sqrt()) / 2.5;
    if t > 0.0 {
        d.max(100.0) - t * STEP * c
    } else {
        5000.0
    }
}

/// Where the segment from `p` along the unit `dir`, `len` long, enters the [`BODY`] of a tank at
/// `at` (`func_15081C20`: y scaled by radius / half about the centre, then `func_151452C4`'s
/// ray-sphere; a start inside hits at 0).
fn hits_hull(p: [f32; 3], dir: [f32; 3], len: f32, at: [f32; 3]) -> Option<f32> {
    let k = BODY.radius / BODY.half;
    let s = |v: [f32; 3]| [v[0], v[1] * k, v[2]];
    let d = s(dir);
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let c = s([at[0], at[1] + BODY.centre, at[2]]);
    hits_sphere(s(p), d.map(|v| v / n), len * n, c, BODY.radius).map(|t| t / n)
}

impl Shell {
    /// One frame of flight: returns where it was.
    fn fly(&mut self) -> [f32; 3] {
        let from = self.pos;
        let (c, s) = cos_sin(self.angle);
        self.pos[0] += 0.5 * self.speed * c;
        self.pos[2] -= 0.5 * self.speed * s;
        self.pos[1] += 0.5 * self.y_vel - 0.275 * SHELL_GRAVITY;
        self.y_vel -= SHELL_GRAVITY;
        from
    }
}

impl Match {
    /// Tank: player `i`'s frame in the slot order (in place of the on-foot one): the hit clip or
    /// death, the power-up's timer, the hull's drive, the turret and gun, then the shell on Z.
    pub(crate) fn tank_player(
        &mut self,
        i: usize,
        input: &Input,
        arena: &Arena,
        world: &CollisionWorld,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        // A wreck pushes as a tank while it shows, once its slot is back (until then the body
        // lying there does; V sc2: a tank at 48 held 120-150 off P2's wreck, 88-192 frames after
        // the death; sc1: through its spot at 351, the wreck gone).
        let wrecks = (self.wrecks().into_iter())
            .filter(|w| self.players.get(w.0).is_some_and(|p| p.health > 0))
            .map(|w| hull::body(w.1));
        let others: Vec<Other> = (self.players.iter().enumerate())
            .filter(|&(j, _)| j != i && !self.free.contains(&j))
            .map(|(_, p)| hull::body(p.body.pos))
            .chain(wrecks)
            .collect();
        // Where the others are, for the gun's aim: living tanks.
        let targets: Vec<[f32; 3]> = (self.players.iter().enumerate())
            .filter(|&(j, p)| j != i && !self.free.contains(&j) && p.health > 0)
            .map(|(_, p)| p.body.pos)
            .collect();
        let f = &mut self.players[i];
        let late = f.buttons;
        (f.late, f.buttons) = (f.buttons, input.buttons);
        f.immune = f.immune.saturating_sub(1);
        if let Action::Hurt {
            reaction,
            lethal,
            step,
        } = f.action
        {
            let step = step.saturating_add(1);
            f.action = Action::Hurt {
                reaction,
                lethal,
                step,
            };
            if lethal {
                f.immune = 255;
                // A shell's kill knocks it a frame (V f1 249), then it lies still.
                if step == 1 && f.body.xz_speed > 0.0 {
                    hull::step(&mut f.body, input, world, &others, Drive::Knock);
                }
                f.body.xz_speed = 0.0;
                // Its wreck is where it lies after that (V sc2: prop 19 at 279 on P2's spot).
                if step == 1
                    && let Some(Some(w)) = self.tank.wrecks.get_mut(i)
                {
                    w.pos = f.body.pos;
                }
                if step >= RESPAWN {
                    self.down(i, arena, ev);
                    let f = &mut self.players[i];
                    (f.anim, f.body.state) = (0, 0);
                }
                return;
            }
            match step {
                ..=KNOCK_FRAMES => hull::step(&mut f.body, input, world, &others, Drive::Knock),
                HIT_FRAMES.. => f.set(Action::Free, 0, t),
                _ => f.body.xz_speed = 0.0,
            }
            if step < HIT_FRAMES {
                return;
            }
        }
        // The power-up held (`func_150ED748`): the shield keeps it immune while its timer runs
        // (VIs, 2 a frame), 0x10 goes with its last shell.
        match f.power {
            SHIELD if f.boost > 0 => {
                f.immune = 255;
                f.boost = f.boost.saturating_sub(2);
                if f.boost == 0 {
                    (f.power, f.immune) = (0, 0);
                }
            }
            SHELLS if f.boost == 0 => f.power = 0,
            _ => {}
        }
        let held = |b: u16| input.buttons & b != 0;
        let boost = f.power == BOOST && f.boost > 0;
        if boost && held(BUTTON_A) {
            f.boost = f.boost.saturating_sub(2);
            if f.boost == 0 {
                f.power = 0;
            }
        }
        let scope = f.power == SCOPE && held(crate::game::BUTTON_R);
        let fired = held(BUTTON_Z) && late & BUTTON_Z == 0 && t >= f.ready;
        if fired {
            f.ready = t + if f.bot { BOT_REFIRE } else { REFIRE };
            f.shots = f.shots.saturating_add(1);
        }
        // The mover's rates: a bot's every frame (its class script's, else 0x32 each), the fire
        // clip's for a human, else the stick's ([`Drive::Stick`]).
        let clip = t < f.ready;
        let script = (self.tank.rates.iter()).find_map(|&(j, r)| (j == i).then_some(r));
        let rates = match (f.bot, script) {
            (true, Some(r)) => Some(r),
            _ if f.bot || clip => Some([hull::CLIP_RATE; 2]),
            _ => None,
        };
        let drive = Drive::Stick {
            boost,
            scope,
            rates,
            fired,
        };
        hull::step(&mut f.body, input, world, &others, drive);
        // A bot's turret closes an eighth of the gap to its aim (the input's camera yaw) a frame.
        if f.bot {
            let want = wrap180(deg(input.camera_yaw) - deg(f.body.facing) - f.body.turret);
            f.body.turret = (f.body.turret + want * 0.125).rem_euclid(360.0);
        }
        // The gun's pitch (`func_150ED748`): + 0.001 a frame per unit its shell falls short of its
        // target ([`range_error`], from the gun's pitch over the hull's), else back to level ×0.9.
        // A human's is the best tank within the turret's cone, its aim 40 over the feet, at most 4°
        // a frame. A bot's is its script's target (s126+0x129), led as its turret's
        // ([`Tank::aims`]), its aim at the feet, unbounded, and its aim error aims 400 nearer (C;
        // V b1, h1, h2: the bots' shells come down at their target's feet, error or not).
        let short = if f.bot {
            400.0 * f32::from(f.aim_error)
        } else {
            0.0
        };
        let b = &f.body;
        let yaw = deg(b.facing) + b.turret;
        let muzzle = b.pos[1] + MUZZLE[1];
        let flat = |p: &[f32; 3]| ((p[0] - b.pos[0]).powi(2) + (p[2] - b.pos[2]).powi(2)).sqrt();
        let best = match f.bot {
            true => (self.tank.aims.iter()).find_map(|&(j, p)| (j == i).then(|| (flat(&p), p[1]))),
            false => (targets.iter())
                .filter_map(|p| {
                    let d = flat(p);
                    let off = wrap180(deg(atan2_u16(b.pos[2] - p[2], p[0] - b.pos[0])) - yaw);
                    let off = off.abs();
                    (off < AIM_CONE && d < AIM_RANGE)
                        .then(|| ((AIM_CONE - off) / d.max(1.0), d, p[1]))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, d, y)| (d, y)),
        };
        let hull_pitch = hull_pitch(b, world);
        let pitch = match best {
            Some((d, y)) if f.bot => {
                b.pitch + 0.001 * range_error(d - short, y - muzzle, b.pitch + hull_pitch)
            }
            Some((d, y)) => {
                let up = 0.001 * range_error(d, y + 40.0 - muzzle, b.pitch + hull_pitch);
                b.pitch + up.clamp(-PITCH_RATE, PITCH_RATE)
            }
            None => b.pitch * 0.9,
        };
        f.body.pitch = pitch.clamp(PITCH_MIN, PITCH_MAX);
        let state = if clip { CLIP_FIRE } else { 0 };
        (f.body.state, f.anim) = (state, u16::from(state));
        if fired && scope {
            let b = &f.body;
            let (c, s) = cos_sin(to_u16(yaw));
            let [ahead, up] = SCOPE_EYE;
            let from = [b.pos[0] + ahead * c, b.pos[1] + up, b.pos[2] - ahead * s];
            let shot = Bullet::aimed(world, i, from, [c, 0.0, -s], BULLET_SPEED, MG_BULLET);
            self.tank.bullets.push((t, shot));
            ev.push(Event::Shot {
                player: i,
                pos: from,
            });
        } else if fired {
            let b = &f.body;
            let angle = to_u16(yaw);
            let (c, s) = cos_sin(angle);
            let (pc, ps) = cos_sin(to_u16(b.pitch + hull_pitch));
            let deadly = f.power == SHELLS && f.boost > 0;
            if deadly {
                f.boost -= 1;
            }
            let pos = [
                b.pos[0] + MUZZLE[0] * c,
                b.pos[1] + MUZZLE[1],
                b.pos[2] - MUZZLE[0] * s,
            ];
            self.tank.shells.push(Shell {
                owner: i,
                pos,
                angle,
                speed: SHELL_SPEED * pc,
                y_vel: SHELL_SPEED * ps,
                deadly,
                born: t,
            });
            ev.push(Event::Shot { player: i, pos });
        }
    }

    /// The shells, after the players (those fired this frame fly from the next): each hits the
    /// first tank its frame's path passes (where its owner saw it), else a wall or the ground.
    pub(crate) fn fly_shells(&mut self, world: &CollisionWorld, seen: Seen, ev: &mut Vec<Event>) {
        let t = self.tick;
        // The bots' script rates and aims were this frame's (the players stepped before).
        self.tank.rates.clear();
        self.tank.aims.clear();
        let mut shells = std::mem::take(&mut self.tank.shells);
        shells.retain_mut(|s| {
            if s.born == t {
                return true;
            }
            let from = s.fly();
            let seg = [s.pos[0] - from[0], s.pos[1] - from[1], s.pos[2] - from[2]];
            let len = (seg[0] * seg[0] + seg[1] * seg[1] + seg[2] * seg[2]).sqrt();
            let dir = seg.map(|v| v / len);
            let wall = raycast(world, from, dir, len);
            // A dead tank stops shells too, unhurt (its obj+0xF8 keeps 0x40; V b1 1912).
            let hit = (0..self.players.len())
                .filter(|&v| v != s.owner && !self.free.contains(&v))
                .filter_map(|v| {
                    let at = seen(s.owner, v).unwrap_or(self.players[v].body.pos);
                    hits_hull(from, dir, wall, at).map(|d| (d, v))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, v)) = hit {
                self.tank_hit(v, Some(s.owner), s.angle, s.deadly, false, ev);
                return false;
            }
            wall >= len
        });
        shells.append(&mut self.tank.shells);
        self.tank.shells = shells;
        // The scope's shots: the first tank on the frame's sweep, else on to the wall.
        let mut bullets = std::mem::take(&mut self.tank.bullets);
        bullets.retain_mut(|(born, b)| {
            if *born == t {
                return true;
            }
            let hit = (0..self.players.len())
                .filter(|&v| v != b.owner && !self.free.contains(&v))
                .filter_map(|v| {
                    let at = seen(b.owner, v).unwrap_or(self.players[v].body.pos);
                    hits_hull(b.pos, b.dir, b.reach(), at).map(|d| (d, v))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, v)) = hit {
                self.tank_hit(v, Some(b.owner), b.angle, false, false, ev);
                return false;
            }
            b.advance()
        });
        self.tank.bullets = bullets;
    }

    /// Tank: a hit on player `v` (`func_1505D024`): a shell's (1 damage, `deadly` all; not through
    /// immunity; knocked 50 along `angle`) or the bomb's (`bomb`: all, immune or not). A kill goes
    /// to `attacker` when another (no suicide for the bomb's own bomber, V b1 8589).
    pub(crate) fn tank_hit(
        &mut self,
        v: usize,
        attacker: Option<usize>,
        angle: u16,
        deadly: bool,
        bomb: bool,
        ev: &mut Vec<Event>,
    ) -> bool {
        let t = self.tick;
        let f = &mut self.players[v];
        if f.health == 0 || self.waiting.contains(&v) || (!bomb && f.immune != 0) {
            return false;
        }
        let damage = if deadly || bomb { f.health } else { 1 };
        f.health -= damage;
        let lethal = f.health == 0;
        let clip = if bomb { CLIP_BOMB } else { CLIP_HIT };
        if !bomb {
            (f.body.move_angle, f.body.xz_speed) = (angle, KNOCK);
        }
        f.set(
            Action::Hurt {
                reaction: clip,
                lethal,
                step: 0,
            },
            u16::from(clip),
            t,
        );
        f.body.state = clip;
        if lethal {
            f.immune = 255;
            f.deaths = f.deaths.saturating_add(1);
            f.carrying = false;
            let wreck = Wreck {
                pos: f.body.pos,
                facing: f.body.facing,
                until: t + WRECK_FRAMES,
            };
            if self.tank.wrecks.len() <= v {
                self.tank.wrecks.resize(v + 1, None);
            }
            self.tank.wrecks[v] = Some(wreck);
        }
        if let Some(a) = attacker.filter(|&a| a < self.players.len()) {
            let enemy = self.enemies(a, v);
            let p = &mut self.players[a];
            if enemy {
                p.hits = p.hits.saturating_add(1);
            }
            if lethal && enemy {
                p.kills = (p.kills + 1).min(9999);
            } else if lethal && !bomb {
                p.suicides += 1;
            }
        }
        ev.push(Event::Hit {
            victim: v,
            attacker: attacker.unwrap_or(v),
            damage,
            lethal,
        });
        true
    }

    /// Tank: the bomb on its post touched by a living tank (as a canister: its +0x80 takes the
    /// toucher + 1 for the script, and it hides; V c1 43).
    pub(crate) fn touch_bomb(&mut self) {
        if !self.tank.bomb.shown || self.tank.bomb.flag != 0 {
            return;
        }
        let reach = |i: usize| self.reach.get(i).copied().unwrap_or(crate::weapon::REACH);
        let toucher = (self.players.iter().enumerate()).position(|(i, f)| {
            !self.free.contains(&i) && f.health > 0 && touches(POST, f, reach(i))
        });
        if let Some(i) = toucher {
            (self.tank.bomb.flag, self.tank.bomb.shown) = (i as i32 + 1, false);
        }
    }

    /// The wrecks showing at frame `t`: slot, where, facing.
    pub fn wrecks(&self) -> Vec<(usize, [f32; 3], u16)> {
        (self.tank.wrecks.iter().enumerate())
            .filter_map(|(k, w)| {
                w.filter(|w| self.tick < w.until)
                    .map(|w| (k, w.pos, w.facing))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Fighter, Settings};
    use crate::spawn::{Placement, Spawn};
    use conker_sim::movement::BUTTON_C_RIGHT;

    fn flat() -> CollisionWorld {
        let big = 20_000.0;
        CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ])
    }

    /// Two tanks facing +x, `gap` apart, `side` off the line.
    fn duel(gap: f32, side: f32) -> (Match, Arena) {
        let rec = |pos: [i16; 3]| Spawn {
            pos,
            b6: 0,
            yaw: 0,
            camera: [0.0; 3],
            b14: 0,
            b15: 0,
        };
        let arena = Arena {
            spawns: vec![rec([-3000, 0, -3000]), rec([3000, 0, 3000])],
            ..Arena::default()
        };
        let at = |pos: [f32; 3]| Placement {
            index: 0,
            pos,
            facing: 0,
            camera: [0.0; 3],
        };
        let players = vec![
            Fighter {
                immune: 0,
                ..Fighter::new(&at([0.0; 3]), 0, 0)
            },
            Fighter {
                immune: 0,
                ..Fighter::new(&at([gap, 0.0, side]), 1, 0)
            },
        ];
        let m = Match::new(&arena, players, crate::Rng::default(), 0).with(Settings {
            tank: true,
            ..Settings::default()
        });
        (m, arena)
    }

    fn press(b: u16) -> Input {
        Input {
            buttons: b,
            ..Input::default()
        }
    }

    /// V f1: Z fires on the press (clip 1 for 34 frames); the tank 600 ahead is hit on the third
    /// frame after, loses 1 hp and is knocked 50 along the shell, back in control after 15 frames.
    #[test]
    fn shells_hit_as_f1() {
        let world = flat();
        let (mut m, arena) = duel(600.0, 0.0);
        m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
        assert_eq!(m.tank.shells.len(), 1);
        assert_eq!(m.players[0].body.state, CLIP_FIRE);
        let s = m.tank.shells[0];
        assert_eq!((s.pos[0], s.pos[1]), (128.0, 113.0));
        for k in 1..=3 {
            assert_eq!(m.players[1].health, 6, "frame {k}");
            m.step(&arena, &[Input::default(); 2], &world);
        }
        let v = &m.players[1];
        assert_eq!(
            (v.health, v.body.state, v.body.xz_speed),
            (5, CLIP_HIT, KNOCK)
        );
        assert!(m.tank.shells.is_empty());
        m.step(&arena, &[Input::default(); 2], &world);
        assert!((m.players[1].body.pos[0] - 624.95).abs() < 0.01);
        for _ in 0..14 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert_eq!(m.players[1].body.state, 0);
        assert_eq!(m.players[0].hits, 1);
    }

    /// V f2 (the shell's line 98, 148 and 197 off the target's feet, 60 over them): only 98 hits.
    #[test]
    fn shells_hit_as_f2() {
        let world = flat();
        for (side, hit) in [(98.0, true), (148.0, false), (197.0, false)] {
            let (mut m, arena) = duel(600.0, side);
            m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
            for _ in 0..8 {
                m.step(&arena, &[Input::default(); 2], &world);
            }
            assert_eq!(m.players[1].health < 6, hit, "side {side}");
        }
    }

    /// V f3: Z held fires once; pressed again it fires only from 34 frames on (a bot's, 51).
    #[test]
    fn refire_after_34() {
        let world = flat();
        let (mut m, arena) = duel(5000.0, 3000.0);
        for _ in 0..6 {
            m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
        }
        assert_eq!(m.players[0].shots, 1);
        for t in 6..34 {
            let z = if t % 2 == 0 { BUTTON_Z } else { 0 };
            m.step(&arena, &[press(z), Input::default()], &world);
        }
        assert_eq!(m.players[0].shots, 1);
        m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
        assert_eq!(m.players[0].shots, 2);
        // A bot's clip: a press 42 frames on fires nothing, one at 51 fires (V tf2).
        let (mut m, arena) = duel(5000.0, 3000.0);
        m.players[0].bot = true;
        for t in 0..=BOT_REFIRE {
            let z = if matches!(t, 0 | 42 | BOT_REFIRE) {
                BUTTON_Z
            } else {
                0
            };
            m.step(&arena, &[press(z), Input::default()], &world);
            assert_eq!(m.players[0].shots, 1 + u16::from(t == BOT_REFIRE), "{t}");
        }
    }

    /// V sc5: scoped (0x02, R held), Z fires no shell; the tank 600 ahead loses 1 hp two frames
    /// after the press (a shell's three) with the hit clip and the knock. V sc3: 150 off the line,
    /// a miss.
    #[test]
    fn the_scope_shot_as_sc5() {
        let world = flat();
        let scoped = press(crate::game::BUTTON_R);
        for (side, hit) in [(0.0, true), (150.0, false)] {
            let (mut m, arena) = duel(600.0, side);
            (m.players[0].power, m.players[0].boost) = (SCOPE, power_timer(SCOPE));
            let fire = press(BUTTON_Z | crate::game::BUTTON_R);
            m.step(&arena, &[fire, Input::default()], &world);
            assert!(m.tank.shells.is_empty());
            assert_eq!(m.players[0].body.state, CLIP_FIRE);
            m.step(&arena, &[scoped, Input::default()], &world);
            assert_eq!(m.players[1].health, 6);
            for _ in 0..3 {
                m.step(&arena, &[scoped, Input::default()], &world);
            }
            let v = &m.players[1];
            assert_eq!(v.health < 6, hit, "side {side}");
            if hit {
                assert_eq!((v.health, v.body.state), (5, CLIP_HIT));
                assert_eq!(m.players[0].hits, 1);
            }
        }
        // The hit lands on the second frame: the knock's first step comes on the third.
        let (mut m, arena) = duel(600.0, 0.0);
        (m.players[0].power, m.players[0].boost) = (SCOPE, power_timer(SCOPE));
        m.step(
            &arena,
            &[press(BUTTON_Z | crate::game::BUTTON_R), Input::default()],
            &world,
        );
        m.step(&arena, &[scoped, Input::default()], &world);
        m.step(&arena, &[scoped, Input::default()], &world);
        assert_eq!(m.players[1].health, 5);
        assert!(m.tank.bullets.is_empty());
    }

    /// V f1 248 → 336: the kill (one-shot 0x10 here, V p1) leaves a wreck for 330 frames; the tank
    /// is back on a record with 6 hp and 150 immune on its 88th frame; the kill is the shooter's.
    #[test]
    fn deaths_respawn_after_88() {
        let world = flat();
        let (mut m, arena) = duel(600.0, 0.0);
        (m.players[0].power, m.players[0].boost) = (SHELLS, power_timer(SHELLS));
        m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
        for _ in 0..3 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        let hit = m.tick - 1;
        assert_eq!(m.players[1].health, 0);
        assert_eq!((m.players[0].kills, m.players[1].deaths), (1, 1));
        assert_eq!((m.players[0].power, m.players[0].boost), (SHELLS, 2));
        assert_eq!(m.wrecks().len(), 1);
        while m.players[1].health == 0 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert_eq!(m.tick - 1 - hit, u32::from(RESPAWN));
        assert_eq!(m.players[1].immune, 150);
        assert_ne!(m.players[1].body.pos[0], 624.95);
        let until = hit + WRECK_FRAMES;
        while m.tick < until {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert!(m.wrecks().is_empty());
    }

    /// V sc2: the wreck lies where the knock left the body, and once the slot is back it stops a
    /// tank driving at it 120-150 short.
    #[test]
    fn wrecks_stand_as_tanks() {
        let world = flat();
        let (mut m, arena) = duel(600.0, 0.0);
        (m.players[0].power, m.players[0].boost) = (SHELLS, power_timer(SHELLS));
        m.step(&arena, &[press(BUTTON_Z), Input::default()], &world);
        while m.players[1].health > 0 || m.wrecks().is_empty() {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        m.step(&arena, &[Input::default(); 2], &world);
        let at = m.wrecks()[0].1;
        assert_eq!(at, m.players[1].body.pos);
        while m.players[1].health == 0 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        let ahead = Input {
            stick_y: 82,
            ..Input::default()
        };
        for _ in 0..60 {
            m.step(&arena, &[ahead, Input::default()], &world);
        }
        let gap = at[0] - m.players[0].body.pos[0];
        assert!((110.0..160.0).contains(&gap), "{gap}");
    }

    /// V p1: the shield keeps the tank immune for 900 frames; C-right turns the turret clockwise.
    #[test]
    fn shield_and_turret() {
        let world = flat();
        let (mut m, arena) = duel(600.0, 0.0);
        (m.players[1].power, m.players[1].boost) = (SHIELD, power_timer(SHIELD));
        m.step(
            &arena,
            &[press(BUTTON_Z | BUTTON_C_RIGHT), Input::default()],
            &world,
        );
        for _ in 0..5 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert_eq!(m.players[1].health, 6);
        assert_eq!(m.players[0].body.turret, 357.0);
        for _ in 0..900 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert_eq!((m.players[1].power, m.players[1].immune), (0, 0));
    }

    /// The doors open 8 a frame from the start, shut while +0x80 is 1.
    #[test]
    fn doors_slide() {
        let mut gas = Gas::default();
        for _ in 0..44 {
            doors(&mut gas);
        }
        assert_eq!(gas.lifts(), [DOOR_OPEN; 8]);
        gas.props[3].flag = 1;
        doors(&mut gas);
        assert_eq!(gas.lifts()[3], DOOR_OPEN - DOOR_STEP);
    }
}
