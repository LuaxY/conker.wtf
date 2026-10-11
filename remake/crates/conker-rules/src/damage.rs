//! Damage: the game's damage-type entries, what a hit does to the victim (`func_1505B9C4`), the
//! knock-back while stunned and the hit and death sequences (`../lab/oracle/combat_README.md` §4).
//!
//! Only the types Deathmatch's weapons use are ported. Entries are 0x28 bytes in the table at
//! `0x8009A9F8` (C, values V from the oracle's RAM: `tools/combat_types.py`), or in the
//! attacker's class table (melee, class 117 at `0x8020E380`).

use conker_anim::combat as anim;
use conker_sim::movement::Side;

/// One damage-type entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageType {
    /// +0: the reaction id (& 0x7F), which picks the victim's hit or death sequence.
    pub reaction: u8,
    /// +0xF: damage (masked by 7 for direct hits, 0xF for area hits) and, bit 0x40, "keep the
    /// victim's move angle" (no knock-back direction).
    pub damage: u8,
    /// +0x10: knock-back direction, added (<< 8) to the source's move angle.
    pub angle: u8,
    /// +0x14, +0x18, +0x1C: the victim's xz speed, y velocity and gravity after the hit.
    pub xz: f32,
    pub y_vel: f32,
    pub gravity: f32,
    /// +0x20: knock-back friction (obj+0x109).
    pub k: u8,
    /// +0x22: immune frames (obj+0x125) when bit 0x80 is set (`& 0x3F`).
    pub immune: u8,
    /// +0x24: 0x10 knock away from the source, 0x1000 hits the owner too, 0x2000 needs a clear
    /// line from the source.
    pub flags: u16,
    /// Area geometry (+0xA forward, +0xC height, +0xE radius), used by the melee class entries.
    pub reach: [i16; 3],
}

#[allow(clippy::too_many_arguments)] // a table row
const fn ty(
    reaction: u8,
    damage: u8,
    xz: f32,
    y_vel: f32,
    gravity: f32,
    k: u8,
    immune: u8,
    flags: u16,
) -> DamageType {
    DamageType {
        reaction,
        damage,
        angle: 0,
        xz,
        y_vel,
        gravity,
        k,
        immune,
        flags,
        reach: [0; 3],
    }
}

/// Sniper body shot (bots; I: body versus head).
pub const SNIPER_BODY: DamageType = ty(0x21, 0x01, 20.0, 0.0, 6.0, 1, 0x80 | 10, 0x0400);
/// Twin-gun bullet.
pub const TWIN_BULLET: DamageType = ty(0x21, 0x01, 34.0, 0.0, 4.0, 30, 0, 0x0400);
/// The sniper's ray from a human (kills; counted as a headshot).
pub const SNIPER_RAY: DamageType = ty(0x2E, 0x07, 0.0, 0.0, 4.0, 30, 0x80 | 1, 0x0400);
/// One-shot gun 0x23 bullet (kills).
pub const GUN23_BULLET: DamageType = ty(0x31, 0x07, 120.0, 0.0, 4.0, 30, 0x80 | 40, 0x2400);
/// Grenade, outer blast (R 500): 2 damage, blown away.
pub const GRENADE_OUTER: DamageType = ty(0x2B, 0x02, 20.0, 70.0, 3.4, 1, 0x80 | 30, 0x3010);
/// Grenade, inner blast (R 360): kills.
pub const GRENADE_INNER: DamageType = ty(0x2C, 0x07, 0.0, 0.0, 0.0, 0, 0x80 | 30, 0x3010);
/// Pistol bullet.
pub const PISTOL_BULLET: DamageType = ty(0x21, 0x02, 34.0, 0.0, 4.0, 30, 0, 0x2400);
/// Class 117's attack 0x0F: melee 0x21's swing, 90 ahead, 46 up, radius 72 (at scale 0.5).
pub const MELEE_SWING: DamageType = DamageType {
    reach: [45, 23, 36],
    ..ty(0x33, 0x41, 10.0, 0.0, 5.0, 30, 0, 0)
};
/// Class 117's attack 0x0E: melee 0x22's swing (kills), 110 ahead.
pub const BLADE_SWING: DamageType = DamageType {
    reach: [55, 23, 36],
    ..ty(0x2F, 0x07, 10.0, 0.0, 5.0, 30, 0, 0)
};

/// Weapon 0x14's shot on the body (type 0x10 = `0x80099AC4`[2]): 1 damage, the 0xD3 stagger.
pub const SHOT_BODY: DamageType = ty(0x20, 0x01, 20.0, 0.0, 6.0, 1, 0x80 | 10, 0x0400);
/// The rocket's inner (R 40, kills) and outer (R 300, 2 damage) blasts: types 0x16 and 0x15.
pub const ROCKET_INNER: DamageType = ty(0x2C, 0x07, 0.0, 0.0, 0.0, 0, 0x80 | 30, 0x3010);
pub const ROCKET_OUTER: DamageType = ty(0x2B, 0x02, 20.0, 70.0, 3.4, 1, 0x80 | 30, 0x3010);
pub const ROCKET_INNER_R: f32 = 40.0;
pub const ROCKET_OUTER_R: f32 = 300.0;

/// Explosion radii (`0.5 · 4 · entry+0xE`): the outer and inner grenade blasts.
pub const GRENADE_OUTER_R: f32 = 500.0;
pub const GRENADE_INNER_R: f32 = 360.0;

/// Direct hits (`func_1505D024`, flags 0xE0000 for bullets) are refused by immune or stunned victims.
pub const GUARDED: u32 = 0x60000;

impl DamageType {
    /// Health taken: `damage & mask` (7 direct, 0xF area).
    pub fn amount(&self, area: bool) -> u8 {
        self.damage & if area { 0xF } else { 7 }
    }

    /// Whether the hit sets the victim's move angle (else it keeps its own: bit 0x40).
    pub fn aims(&self) -> bool {
        self.damage & 0x40 == 0
    }

    /// obj+0x125 after the hit, when the entry sets it.
    pub fn immune_frames(&self) -> Option<u8> {
        (self.immune & 0x80 != 0).then_some(self.immune & 0x3F)
    }
}

/// One frame of knock-back on the ground (V, `combat_e_w09` and `combat_bot_wall6`): returns the
/// distance to move and the next speed. `s_eff = s − 0.1·k`; the player moves `0.5·s_eff` when
/// that is at least 1 (the wave-2 rule: below speed 1 nobody moves), and keeps `0.6·s_eff`.
pub fn knockback(speed: f32, k: u8) -> (f32, f32) {
    let s = speed - 0.1 * f32::from(k);
    if s <= 0.0 {
        return (0.0, 0.0);
    }
    (if s >= 1.0 { 0.5 * s } else { 0.0 }, 0.6 * s)
}

/// The victim's states after a hit with `reaction`, as (animation id, frames), and whether its
/// facing turns to face the attacker (toward move angle + 0x8000 by at most 3072 a frame) or to
/// the flight (the grenade). V on the encounter traces; reactions never traced non-lethally (or
/// lethally) fall back to the bullets' stagger and death (I).
pub fn sequence(reaction: u8, lethal: bool) -> &'static [(u16, u16)] {
    const STAGGER: &[(u16, u16)] = &[(anim::HIT, 18)];
    const SHOT_DEAD: &[(u16, u16)] = &[(anim::HIT, 18), (anim::DEATH_FALL, 52), (anim::DEAD, 81)];
    match (reaction, lethal) {
        (0x33, false) => &[(anim::HIT_MELEE, 19)],
        (0x33, true) => &[
            (anim::HIT_MELEE, 19),
            (anim::DEATH_FALL, 52),
            (anim::DEAD, 63),
            (anim::DEAD_MELEE_END, 18),
        ],
        (0x2B, false) => &[
            (anim::BLOWN_UP, 42),
            (anim::GET_UP, 42),
            (anim::RECOVER, 30),
        ],
        (0x2B | 0x2C, _) => &[(anim::BLOWN_UP, 88)],
        (0x2E, true) => &[
            (anim::DEATH_SHOT, 76),
            (anim::DEATH_SHOT_FALL, 52),
            (anim::DEAD, 51),
        ],
        (0x2F, true) => &[
            (anim::DEATH_SHOT, 75),
            (anim::DEATH_SHOT_FALL, 52),
            (anim::DEAD, 51),
        ],
        (0x31, true) => &[(anim::DEATH_BLAST, 35), (anim::DEATH_BLAST_FALL, 103)],
        // The flame's hit: the burning that follows is the movement's (`Fighter::burn`).
        (0x10, false) => &[(anim::FLAME_HIT, 5)],
        // Weapon 0x14's body hit: class 117's reaction 0x20 entry, 0xD3 for 30 frames (C).
        (0x20, false) => &[(anim::HIT_SHOT, 30)],
        (_, false) => STAGGER,
        (_, true) => SHOT_DEAD,
    }
}

/// How the victim's facing follows a hit with `reaction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    /// Turns toward move angle + 0x8000 (to face the shooter), at most 3072 a frame (V: bullets,
    /// the one-shot gun, the sniper).
    Attacker,
    /// Turns toward the move angle, at most 3072 a frame, the short way: the blade's death spins
    /// round to face away from the attacker (V e_w22 849-859: 57088 → 22687 in 11 frames), and so
    /// does a player blown up by an outer blast (V combat4_w16 822-832).
    Away,
    /// Takes the move angle at once (wave 3's rule for the blasts; [`facing`] no longer returns
    /// it: a blown-up player turns at 3072 a frame, [`Facing::Away`]).
    Flight,
    /// Stays (V: melee 0x21, the inner blast's kill on combat4_w16 and combat4_dead2; I: the rest).
    Keep,
}

pub fn facing(reaction: u8) -> Facing {
    match reaction {
        0x21 | 0x2E | 0x31 => Facing::Attacker,
        0x2B | 0x2F => Facing::Away,
        _ => Facing::Keep,
    }
}

/// A player's body for the area tests and the bullets' sweep: obj+0xD2 radius, +0xD4 half height
/// and +0xD6 centre above the feet, at scale 0.5 (V: the oracle's RAM; they are half the model's
/// bank-0x11 record's values, C).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hitbox {
    pub radius: f32,
    pub half: f32,
    pub centre: f32,
}

impl Hitbox {
    /// The Tedi's 26, 43 and 35; the squirrel soldier's 36, 64 and 44.
    pub fn of(side: Side) -> Self {
        let [radius, half, centre] = match side {
            Side::Tediz => [26.0, 43.0, 35.0],
            Side::Squirrels => [36.0, 64.0, 44.0],
        };
        Self {
            radius,
            half,
            centre,
        }
    }
}

/// Explosion and melee reach (`func_1505C7D8` with the victim's ellipsoid [`Hitbox`]; C, V on
/// e_gren, e_w21, e_w22): the sphere of radius `r` at `c` against a player `b` standing at `p`.
pub fn in_reach(c: [f32; 3], r: f32, p: [f32; 3], b: Hitbox) -> bool {
    let f = b.half / (r + b.half);
    let dy = (c[1] - (p[1] + b.centre)) * ((b.radius / b.half) * f + (1.0 - f));
    let (dx, dz) = (c[0] - p[0], c[2] - p[2]);
    dx * dx + dy * dy + dz * dz < (b.radius + r) * (b.radius + r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// combat_e_w09 rows 825-830 (k 30) and combat_bot_wall6 (k 1): speeds as the game printed them.
    #[test]
    fn knockback_matches_the_traces() {
        let mut s = 34.0;
        let mut moved = Vec::new();
        for want in ["18.6000004", "9.36000061", "3.81600046", "0.489600301", "0"] {
            let (d, next) = knockback(s, 30);
            moved.push(d);
            assert_eq!(next, want.parse::<f32>().unwrap());
            s = next;
        }
        let third = 0.5 * ("9.36000061".parse::<f32>().unwrap() - 3.0);
        assert_eq!(moved[..3], [15.5, 7.8, third]);
        assert_eq!(moved[3..], [0.0, 0.0]); // 0.816 and below: no move
        assert_eq!(knockback(20.0, 1).1, 11.940_001); // bot_wall6 row 731
        assert_eq!(knockback(10.0, 30), (3.5, 4.200_000_3)); // melee e_w21 row 851
    }

    #[test]
    fn damage_masks_and_flags() {
        assert_eq!(MELEE_SWING.amount(true), 1);
        assert!(!MELEE_SWING.aims() && BLADE_SWING.aims());
        assert_eq!(GRENADE_INNER.amount(true), 7);
        assert_eq!(GUN23_BULLET.amount(false), 7);
        assert_eq!(TWIN_BULLET.immune_frames(), None);
        assert_eq!(GRENADE_OUTER.immune_frames(), Some(30));
        assert_eq!(sequence(0x21, true).iter().map(|s| s.1).sum::<u16>(), 151); // e_w09 945 → 1096
        assert_eq!(sequence(0x33, true).iter().map(|s| s.1).sum::<u16>(), 152); // e_w21 1061 → 1213
    }

    /// e_gren: the victim 433 away in xz, on the same floor, is in the outer blast only. The
    /// squirrel's body reaches 10 further: a blast that misses a Tedi by 5 hits it.
    #[test]
    fn grenade_reach() {
        let blast = [-577.993, -279.645, -238.12];
        let victim = [-219.113_5, -290.902_3, 5.314_9];
        let tedi = Hitbox::of(Side::Tediz);
        assert!(in_reach(blast, GRENADE_OUTER_R, victim, tedi));
        assert!(!in_reach(blast, GRENADE_INNER_R, victim, tedi));
        let at = [GRENADE_OUTER_R + 26.0 + 5.0, -35.0, 0.0];
        assert!(!in_reach([0.0; 3], GRENADE_OUTER_R, at, tedi));
        assert!(in_reach(
            [0.0; 3],
            GRENADE_OUTER_R,
            at,
            Hitbox::of(Side::Squirrels)
        ));
    }
}
