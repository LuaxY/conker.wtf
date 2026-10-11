//! Deathmatch weapons and pickups (`../lab/oracle/combat_README.md` §1-3; V on the `combat_e_*`
//! encounters and the `combat_ctlw_k*` control probes unless tagged).
//!
//! Frame numbers count from the frame the attack starts (the frame of the Z press; its animation
//! shows from the next trace row).

use conker_anim::combat as anim;

use crate::damage::{self, DamageType};

/// What an attack delivers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attack {
    /// Bullets at 300 a frame from `muzzles[i]` (forward, side, up in the shooter's frame, V for
    /// the traced poses), `at[i]` frames into the attack. With a `cycle`, the pattern repeats
    /// every `cycle` frames while Z is held at the cycle's start; otherwise once per press.
    Bullets {
        kind: DamageType,
        muzzles: &'static [[f32; 3]],
        at: &'static [u32],
        cycle: u32,
    },
    /// The class table's sphere `at` frames into the swing (`func_1504CB98` → `func_1505C7D8`).
    Melee { kind: DamageType, at: u32 },
    /// An instant ray along the facing from `origin` (`func_15081E78`), 3 frames after Z in aim.
    Ray { kind: DamageType, origin: [f32; 3] },
    /// No attack.
    None,
    /// The flamethrower's stream ([`crate::flame`]): area hits of `kind`, `delay` frames after Z.
    Flame { kind: DamageType, delay: u32 },
    /// A rocket ([`crate::projectile::Rocket`]) from `muzzle` on the Z press, then nothing until
    /// `cooldown` frames have passed (s126+0x19E = 120, C). V combat4_ctlw_k7, combat4_w16.
    Rocket { muzzle: [f32; 3], cooldown: u32 },
    /// An aimed shot ([`crate::projectile::Shot`]): only with R held, Z plays `anim` and fires from
    /// `muzzle`; a head hit kills (type 0x20), a body hit is `kind`; then nothing for `cooldown`
    /// frames (s126+0x72 = 40, C). Without R, Z plays the weapon's plain `fire` (no hit). V
    /// combat4_w14, combat4_probe14.
    Shot {
        kind: DamageType,
        muzzle: [f32; 3],
        anim: (u16, u16),
        cooldown: u32,
    },
    /// The crossbow's bolt ([`crate::projectile::Shot`], 125 a frame): from the hip, Z plays the
    /// weapon's `fire` and looses it `at` frames in from `muzzle`, level along the facing; R holds
    /// the aim (`aim`), and Z's release shoots at once (`aimed`) from the scope's eye 11 lower
    /// toward what the scope looks at. A head hit kills (type 0x20), a body hit is `kind`; Z does
    /// nothing for `cooldown` frames after a shot's Z (s126+0x19E = 60 VIs, C). V combat5_cb2-4,
    /// cb_e1.
    Bolt {
        kind: DamageType,
        muzzle: [f32; 3],
        at: u32,
        aim: u16,
        aimed: (u16, u16),
        cooldown: u32,
    },
    /// The shotgun: on the Z press, four area hits along the facing from the muzzle
    /// (`func_150FF840`, [`SPREAD`]); then nothing for `cooldown` frames (s126+0x19E = 120 VIs, C).
    /// One shot in four (an RNG draw: `& 3 == 0`, C `func_15065A5C`) plays `alt` from `muzzles[1]`
    /// instead of `fire` from `muzzles[0]` (the hand's pose). V combat5_sg3 (20 picks), sg_e1-e3.
    Spread {
        muzzles: [[f32; 3]; 2],
        alt: (u16, u16),
        cooldown: u32,
    },
}

/// The shotgun's spheres in order: type, distance from the muzzle along the facing, radius
/// (`0.5 · 4 · entry+0xE`). The muzzle's (0x3A) is the only one an immune or stunned player takes
/// (not guarded, C); the others need a clear line from the muzzle (flag 0x2000).
pub const SPREAD: [(DamageType, f32, f32); 4] = [
    (damage::SHOTGUN_MUZZLE, 0.0, 40.0),
    (damage::SHOTGUN_KILL, 88.0, 120.0),
    (damage::SHOTGUN_BLAST, 383.0, 240.0),
    (damage::SHOTGUN_EDGE, 810.0, 360.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weapon {
    pub id: u8,
    /// (animation, frames).
    pub draw: (u16, u16),
    pub idle: u16,
    pub holster: (u16, u16),
    /// Frames 0: the fire state lasts while Z is held, plus 6 frames.
    pub fire: (u16, u16),
    /// The armed locomotion the oracle's bots showed (the twin guns' 0x12B, V bots3); players walk
    /// with `conker_anim::armed_move`.
    pub walk: Option<u16>,
    pub attack: Attack,
    /// Rounds before the gun reloads by itself (0: never; s126+0x25 for the clips).
    pub rounds: u8,
    /// A's reload (guns only), and the one after the last round when it differs.
    pub reload: Option<(u16, u16)>,
    pub empty_reload: u16,
    /// The C buttons play this 3-frame state instead of strafing or throwing a grenade.
    pub c_button: Option<u16>,
}

impl Weapon {
    /// A jumps with the melee weapons and 0x14; it reloads the guns and does nothing with the
    /// sniper rifle, the flamethrower and 0x16 (V `combat_ctlw_k*` read 131).
    pub fn jumps(&self) -> bool {
        matches!(self.id, 0x14 | 0x21 | 0x22 | 0x40)
    }

    /// Automatic: Z held keeps firing (the twin guns, the flamethrower).
    pub fn automatic(&self) -> bool {
        self.fire.1 == 0
    }

    /// R's aim state, for the weapons that have one (the sniper rifle, the crossbow).
    pub fn aim(&self) -> Option<u16> {
        match self.attack {
            Attack::Ray { .. } => Some(anim::SNIPER_AIM),
            Attack::Bolt { aim, .. } => Some(aim),
            _ => None,
        }
    }
}

/// The twin guns' muzzles (e_w09 reads 824 and 826): the left gun 20.6 behind and 20.8 to the
/// side, the right one 14.6 behind and 19.3 to the other side, about 41 up.
const TWIN_MUZZLES: [[f32; 3]; 2] = [[-20.648, 20.788, 41.519], [-14.549, -19.277, 40.682]];

pub const TWIN_GUNS: Weapon = Weapon {
    id: 0x09,
    draw: (anim::TWIN_DRAW, 15),
    idle: anim::TWIN_IDLE,
    holster: (anim::TWIN_HOLSTER, 22),
    fire: (anim::TWIN_FIRE, 0),
    walk: Some(anim::TWIN_MOVE),
    attack: Attack::Bullets {
        kind: damage::TWIN_BULLET,
        muzzles: &TWIN_MUZZLES,
        at: &[4, 6],
        cycle: 5,
    },
    rounds: 60,
    reload: Some((anim::TWIN_RELOAD, 34)),
    empty_reload: 34,
    c_button: None,
};

pub const PISTOL: Weapon = Weapon {
    id: 0x39,
    draw: (anim::PISTOL_DRAW, 15),
    idle: anim::PISTOL_IDLE,
    holster: (anim::PISTOL_HOLSTER, 15),
    fire: (anim::PISTOL_FIRE, 2),
    walk: None,
    attack: Attack::Bullets {
        kind: damage::PISTOL_BULLET,
        muzzles: &[[-9.892, 15.549, 51.548]],
        at: &[0],
        cycle: 0,
    },
    rounds: 30,
    reload: Some((anim::PISTOL_RELOAD, 60)),
    empty_reload: 60,
    c_button: None,
};

/// Weapon 0x23: one bullet that kills. It reloads after 6 shots (s126+0x1AB), for 79 frames
/// (A: 31; I: the reload's length depends on the rounds spent, 2 points traced). Its muzzle
/// sphere (type 0x23, R 40) is not modelled.
pub const GUN23: Weapon = Weapon {
    id: 0x23,
    draw: (anim::GUN23_DRAW, 26),
    idle: anim::GUN23_IDLE,
    holster: (anim::GUN23_HOLSTER, 28),
    fire: (anim::GUN23_FIRE, 28),
    walk: None,
    attack: Attack::Bullets {
        kind: damage::GUN23_BULLET,
        muzzles: &[[17.458, 6.885, 51.698]],
        at: &[0],
        cycle: 0,
    },
    rounds: 6,
    reload: Some((anim::GUN23_RELOAD, 31)),
    empty_reload: 79,
    c_button: Some(anim::GUN23_C),
};

/// The sniper rifle: R aims, Z in aim fires a ray from the aim camera's look-at, 67.5 above the
/// feet, along the facing (V combat4_w24: a world hit 1,453 ahead on the axis at +67.5, a head hit
/// at +67.6). A head hit (part 0xA of `func_15081E78`) kills with type 0x20, a body hit is type
/// 0x12 for 1 (C; which heights are the head is I, [`HEAD`]). The muzzle sphere (0x1E, R 40, 3
/// frames later) is not modelled.
pub const SNIPER: Weapon = Weapon {
    id: 0x24,
    draw: (anim::SNIPER_DRAW, 53),
    idle: anim::SNIPER_IDLE,
    holster: (anim::SNIPER_HOLSTER, 29),
    fire: (anim::SNIPER_FIRE, 39),
    walk: None,
    attack: Attack::Ray {
        kind: damage::SNIPER_BODY,
        origin: [0.0, 0.0, 67.5],
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: Some(anim::SNIPER_C),
};

/// The flamethrower: a stream of area hits (type 0x1C) that sets its victims burning
/// ([`crate::flame`]).
pub const FLAMETHROWER: Weapon = Weapon {
    id: 0x15,
    draw: (anim::FLAME_DRAW, 34),
    idle: anim::FLAME_IDLE,
    holster: (anim::FLAME_HOLSTER, 25),
    fire: (anim::FLAME_FIRE, 0),
    walk: None,
    attack: Attack::Flame {
        kind: crate::flame::FLAME,
        delay: crate::flame::DELAY,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: None,
};

pub const MELEE: Weapon = Weapon {
    id: 0x21,
    draw: (anim::MELEE_DRAW, 16),
    idle: anim::MELEE_IDLE,
    holster: (anim::MELEE_HOLSTER, 20),
    // ponytail: 2 of 9 traced swings play MELEE_SWING_A (19 frames, same hit frame); the rule is unread.
    fire: (anim::MELEE_SWING, 20),
    walk: Some(anim::MELEE_WALK),
    attack: Attack::Melee {
        kind: damage::MELEE_SWING,
        at: 10,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: None,
};

pub const BLADE: Weapon = Weapon {
    id: 0x22,
    draw: (anim::BLADE_DRAW, 21),
    idle: anim::BLADE_IDLE,
    holster: (anim::BLADE_HOLSTER, 21),
    fire: (anim::BLADE_SWING, 19),
    walk: Some(anim::BLADE_WALK),
    attack: Attack::Melee {
        kind: damage::BLADE_SWING,
        at: 8,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: None,
};

/// Weapon 0x14: with R held, Z fires an aimed shot (0x12B, 14 frames; V combat4_w14 killed with a
/// head hit at 146.5); without R, Z plays 0xF7 and hits nothing (V e_w14 at 65 and 37 units).
pub const W14: Weapon = Weapon {
    id: 0x14,
    draw: (anim::W14_DRAW, 15),
    idle: anim::W14_IDLE,
    holster: (anim::W14_HOLSTER, 15),
    fire: (anim::W14_ATTACK, 14),
    walk: None,
    attack: Attack::Shot {
        kind: damage::SHOT_BODY,
        muzzle: [0.0, 27.0, 67.5],
        anim: (anim::W14_AIMED, 14),
        cooldown: 40,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: None,
};

/// Weapon 0x16, the rocket launcher: B draws in 39 frames, the idle holds until B again, holster 29
/// (V `combat_e_w16`); Z fires a rocket and plays 0x77 for 28 frames (`combat4_ctlw_k7`; its root
/// motion moving the player back 46.5 is conker-sim's).
pub const W16: Weapon = Weapon {
    id: 0x16,
    draw: (anim::W16_DRAW, 39),
    idle: anim::W16_IDLE,
    holster: (anim::W16_HOLSTER, 29),
    fire: (anim::W16_FIRE, 28),
    attack: Attack::Rocket {
        muzzle: [3.658, 11.912, 64.408],
        cooldown: 120,
    },
    ..W14
};

/// The crossbow (0x18): see [`Attack::Bolt`]. A does nothing; the C buttons play the run for 3
/// frames (V combat5_ctlw18).
pub const CROSSBOW: Weapon = Weapon {
    id: 0x18,
    draw: (anim::CROSSBOW_DRAW, 64),
    idle: anim::CROSSBOW_IDLE,
    holster: (anim::CROSSBOW_HOLSTER, 34),
    fire: (anim::CROSSBOW_FIRE, 29),
    walk: None,
    attack: Attack::Bolt {
        kind: damage::SHOT_BODY,
        muzzle: [10.259, -7.948, 45.225],
        at: 10,
        aim: anim::CROSSBOW_AIM,
        aimed: (anim::CROSSBOW_AIMED, 29),
        cooldown: 30,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: Some(anim::CROSSBOW_RUN),
};

/// The shotgun (0x37): see [`Attack::Spread`]. One shot per Z press; a new one may cut the long
/// 0x235 short (V combat5_sg3). R and A do nothing, C-down throws a grenade (V combat5_ctlw37).
pub const SHOTGUN: Weapon = Weapon {
    id: 0x37,
    draw: (anim::SHOTGUN_DRAW, 13),
    idle: anim::SHOTGUN_IDLE,
    holster: (anim::SHOTGUN_HOLSTER, 13),
    fire: (anim::SHOTGUN_FIRE, 45),
    walk: None,
    attack: Attack::Spread {
        muzzles: [[57.535, 7.572, 45.99], [59.756, 12.117, 46.046]],
        alt: (anim::SHOTGUN_FIRE_B, 71),
        cooldown: 60,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: None,
};

/// Item 0x40, the club: scene 0x3F's start item (`Fighter::start`). Z swings 0x113 (30 frames; the
/// class sphere 0x0C 10 frames in, after locking onto the nearest player); a Z press within
/// [`COMBO_WINDOW`] frames of its hit chains [`CLUB_COMBO`]; every Z press draws the RNG once
/// (`func_150641D8`, C). Draw, holster and swings hold the movement (obj+0x89 = 0xFF: the melee
/// brake). R held: Z does nothing. A jumps; the C buttons strafe with the walk (no grenade). V
/// combat6_ctlw40, e40 (`../lab/oracle/caveman_README.md`).
pub const CLUB: Weapon = Weapon {
    id: 0x40,
    draw: (anim::CLUB_DRAW, 15),
    idle: anim::CLUB_IDLE,
    holster: (anim::CLUB_HOLSTER, 15),
    fire: (anim::CLUB_SWING, 30),
    walk: None,
    attack: Attack::Melee {
        kind: damage::CLUB_SWING,
        at: 10,
    },
    rounds: 0,
    reload: None,
    empty_reload: 0,
    c_button: Some(anim::CLUB_WALK),
};

/// The club's second swing: (animation, frames), its sphere and the frame it lands (V combat6_e40
/// 871-897: the hit 11 frames in).
pub const CLUB_COMBO: ((u16, u16), DamageType, u32) =
    ((anim::CLUB_COMBO, 27), damage::CLUB_COMBO, 11);

/// Frames after a club hit in which Z chains [`CLUB_COMBO`]: obj+0x107 is 60 at the hit, −1 a
/// frame, and the combo needs 40 (C `func_150641D8`; V e40: a press 20 frames on chains, I: one
/// frame more might too).
pub const COMBO_WINDOW: u32 = 20;

pub fn weapon(id: u8) -> Option<&'static Weapon> {
    Some(match id {
        0x09 => &TWIN_GUNS,
        0x39 => &PISTOL,
        0x23 => &GUN23,
        0x24 => &SNIPER,
        0x15 => &FLAMETHROWER,
        0x21 => &MELEE,
        0x22 => &BLADE,
        0x14 => &W14,
        0x16 => &W16,
        0x18 => &CROSSBOW,
        0x37 => &SHOTGUN,
        0x40 => &CLUB,
        _ => return None,
    })
}

/// The head's height above the feet: the centre of [`crate::projectile::HEAD_PART`].
pub const HEAD: f32 = crate::projectile::HEAD_PART[0];

/// Bullet speed, units per frame.
pub const BULLET_SPEED: f32 = 300.0;
/// Bullet range when no wall is closer. Every traced bullet ended at a wall, the longest after 10.96
/// frames (3,287 units, V: the bots agent's `bot_bullets_lldb.txt`), so the range is the next wall:
/// this only bounds the ray (as the sniper's).
pub const BULLET_RANGE: f32 = 20_000.0;

/// What a pickup gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gives {
    Weapon(u8),
    /// +3 grenades, at most 6.
    Grenades,
}

/// A pickup: the scene placement it sits on and what it gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pickup {
    pub placement: u16,
    pub gives: Gives,
}

const fn pick(placement: u16, id: u8) -> Pickup {
    Pickup {
        placement,
        gives: if id == 0 {
            Gives::Grenades
        } else {
            Gives::Weapon(id)
        },
    }
}

/// Scene 0x2D's 19 pickups in the MP weapon manager's order (k = MP+0xEB0 values; V from the
/// RAM, `func_1508DC24`). k ≥ 12 mirror k − 12 on the other base. Placements 21-28 and 37 start
/// hidden and are unused in Deathmatch.
pub const PICKUPS_2D: [Pickup; 19] = [
    pick(12, 0x09),
    pick(31, 0x21),
    pick(33, 0),
    pick(34, 0x22),
    pick(13, 0x14),
    pick(38, 0x39),
    pick(16, 0x23),
    pick(17, 0x16),
    pick(18, 0x15),
    pick(19, 0x15),
    pick(29, 0x24),
    pick(30, 0x24),
    pick(14, 0x09),
    pick(32, 0x21),
    pick(35, 0),
    pick(36, 0x22),
    pick(15, 0x14),
    pick(39, 0x39),
    pick(20, 0x23),
];

/// Scene 0x30's 17 pickups (Deathmatch and Colors alike), in the manager's order (V from the RAM,
/// `../lab/oracle/map30`: pickup k is the prop whose id is k + 1).
pub const PICKUPS_30: [Pickup; 17] = [
    pick(1, 0x16),
    pick(2, 0x22),
    pick(3, 0x24),
    pick(4, 0x24),
    pick(5, 0x23),
    pick(6, 0x23),
    pick(7, 0x21),
    pick(8, 0x21),
    pick(9, 0x09),
    pick(10, 0x09),
    pick(11, 0x14),
    pick(17, 0),
    pick(12, 0x15),
    pick(13, 0),
    pick(14, 0),
    pick(15, 0x14),
    pick(16, 0),
];

/// Where scene 0x30's pickups stand (the props' positions in the RAM, the same from 3 to 57 s and
/// in both scenarios: V): 2-344 units off their placements, unlike 0x2D's (how the game moves
/// them is unread).
pub const SPOTS_30: [[i16; 3]; 17] = [
    [-1368, 370, 2198],
    [-241, 352, -290],
    [-2611, 1474, -3289],
    [2286, 2411, 5595],
    [455, -853, -7379],
    [-667, 80, 9697],
    [-836, 1466, 6261],
    [787, 596, -3845],
    [-188, -1343, -7175],
    [-20, -396, 9524],
    [572, -434, 4557],
    [-2628, -371, 2792],
    [1948, -346, 1560],
    [2415, -661, -892],
    [-891, -843, -6249],
    [-1602, -862, -1691],
    [691, 99, 8577],
];

/// Scene 0x34's 16 pickups (Deathmatch, scenario 4), in the manager's order: pickup k is the prop
/// whose id is k + 1, and each stands on its placement (V from the RAM at 3, 10 and 57 s,
/// `../lab/oracle/map34`). 0x18 and 0x37 are the crossbow and the shotgun.
pub const PICKUPS_34: [Pickup; 16] = [
    pick(13, 0x24),
    pick(14, 0x23),
    pick(15, 0x16),
    pick(16, 0x22),
    pick(17, 0x16),
    pick(18, 0x15),
    pick(25, 0x39),
    pick(19, 0x18),
    pick(20, 0x14),
    pick(21, 0x09),
    pick(22, 0x09),
    pick(23, 0x21),
    pick(24, 0x23),
    pick(26, 0x37),
    pick(27, 0),
    pick(28, 0),
];

/// Scene 0x3F's 10 pickups (Deathmatch, scenario 7), in the manager's order: the weapons are the
/// props with ids 14-21 (placements 7-14), the grenades ids 4 and 5 (placements 23, 24); each
/// stands on its placement (V from the RAM, `../lab/oracle/map3f`).
pub const PICKUPS_3F: [Pickup; 10] = [
    pick(7, 0x22),
    pick(8, 0x23),
    pick(9, 0x24),
    pick(10, 0x09),
    pick(11, 0x21),
    pick(12, 0x37),
    pick(13, 0x39),
    pick(14, 0x15),
    pick(23, 0),
    pick(24, 0),
];

/// A scene's pickups, and their spots when the game moves them off their placements (empty: on
/// the placements), if known.
pub fn pickups(scene: u16) -> Option<(&'static [Pickup], &'static [[i16; 3]])> {
    match scene {
        0x2D => Some((&PICKUPS_2D, &[])),
        0x30 => Some((&PICKUPS_30, &SPOTS_30)),
        0x34 => Some((&PICKUPS_34, &[])),
        0x3F => Some((&PICKUPS_3F, &[])),
        _ => None,
    }
}

/// Touch radius (`100 · s126+0x1AF / 64`, with +0x1AF = 64; C `func_1511DF6C`).
pub const PICKUP_RADIUS: f32 = 100.0;
/// A taken pickup is back after 240 VIs (MP+0xEAC) = 120 frames (V `combat_pick57`).
pub const PICKUP_RESPAWN: u16 = 120;
pub const MAX_GRENADES: u8 = 6;
pub const START_GRENADES: u8 = 3;

/// The touch test, before the player moves that frame: within 100 horizontally and the pickup's
/// height within 100 of the player's 90-unit body (C; the extra `prop.y < obj+0x17C` test is
/// left out, I).
pub fn touches(pickup: [f32; 3], player: [f32; 3]) -> bool {
    let (dx, dz) = (pickup[0] - player[0], pickup[2] - player[2]);
    dx * dx + dz * dz < PICKUP_RADIUS * PICKUP_RADIUS
        && pickup[1] - PICKUP_RADIUS < player[1] + 90.0
        && player[1] < pickup[1] + PICKUP_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        for p in PICKUPS_2D.iter().chain(&PICKUPS_30).chain(&PICKUPS_34) {
            if let Gives::Weapon(id) = p.gives {
                assert_eq!(weapon(id).unwrap().id, id);
            }
        }
        // the mirrored half gives the same as the first
        for k in 0..7 {
            assert_eq!(PICKUPS_2D[k].gives, PICKUPS_2D[k + 12].gives, "{k}");
        }
        assert!(
            MELEE.jumps() && W14.jumps() && !W16.jumps() && !TWIN_GUNS.jumps() && !SNIPER.jumps()
        );
        assert!(TWIN_GUNS.automatic() && FLAMETHROWER.automatic() && !PISTOL.automatic());
        assert!(matches!(W16.attack, Attack::Rocket { .. }) && W16.fire.1 == 28);
    }

    /// combat_pick57: touched at xz 93.0 (read 154), not at 110.5 (read 153).
    #[test]
    fn touch_radius() {
        let p = [-2016.0, -435.0, 7507.0];
        assert!(touches(p, [-2016.0 + 93.0, -435.0, 7507.0]));
        assert!(!touches(p, [-2016.0 + 110.5, -435.0, 7507.0]));
        assert!(!touches(p, [-2016.0, -435.0 + 150.0, 7507.0]));
    }
}
