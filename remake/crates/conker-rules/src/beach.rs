//! War: Beach (scenario 8, scene 0x2B; `../lab/research/mp-original/beach.md`): the villagers (team 0,
//! the squirrels' side) run for the two tunnels under the Tediz' bunkers, the Tediz (team 1, at most
//! [`MAX_TEDIZ`]) hold the bunker tops and the mounted gun. The level's AI script 0xB9 is the referee,
//! as Colors' is: the escapes, the plunger's event, the weapon zones and the 90-second deadline are
//! its, through [`crate::gas::Effect`]s. The match keeps what the engine does: the villagers' item
//! 0x3E ([`drive`]), the bottles, the plunger's prop ([`Plunger`]), the class re-roll at each respawn,
//! a villager's death scoring for the Tediz and the score end (`Match`).

/// The teams ([`crate::Fighter::team`]).
pub const VILLAGERS: u8 = 0;
pub const TEDIZ: u8 = 1;
/// Team row 2's byte +7: the Tediz are capped at 2 participants, bots included (C).
pub const MAX_TEDIZ: usize = 2;
/// The villagers: at most 8 (`func_151E1744`, C), so a full match is 8 villagers vs 2 Tediz.
pub const MAX_VILLAGERS: usize = 8;
/// The villagers' classes at the match start, the k-th villager's (V x1: `1e 14 1d 1f`; I: by the
/// villagers' order), and the Tediz' (model 117).
pub const CLASSES: [u8; 4] = [0x1E, 0x14, 0x1D, 0x1F];
pub const TEDI_CLASS: u8 = 3;
/// The villagers' item (MP+0x1170, their start item): no attack, the speed of [`drive`].
pub const ITEM: u8 = 0x3E;
/// A bottle's boost (s126+0x12C) in VIs, which item 0x3E drains by 2 a frame (V spr: 600).
pub const BOOST: u16 = 600;
/// A bottle is back 60 VIs after its touch (MP+0xEAC = 0x3C, the script at 0x18EC; V wz 702 → 732).
pub const BOTTLE_RESPAWN: u16 = 30;
/// obj+0xAA with item 0x3E: 75 % of the drive, 150 % while the boost lasts (V spd, spr: 26.25 and
/// 52.5 at the top; a crouch-walk's 40 % is the movement's own, conker-sim's `CROUCH_PERCENT`).
pub const SLOW: u8 = 75;
pub const FAST: u8 = 150;
/// Weapon 0x16's fire clip on scene 0x2B in MP: 0x207, not 0x77 (`func_15065A5C` case 0x16, C; V
/// bots1: the bazooka Tedi plays it and stays within 25 of its spot), which has no root motion, so
/// the shooter is not pushed back (the pack: 0x77's moves it 91.6 over the clip). I: Deathmatch on
/// 0x2B too; the match only knows Beach.
pub const ROCKET_FIRE: u16 = 0x207;

/// The model a class shows: 156 below 0x1D, else 157 (`func_1507D1D8`, C); the Tediz' 117.
pub fn model(class: u8) -> u16 {
    match class {
        TEDI_CLASS => 117,
        c if c < 0x1D => 156,
        _ => 157,
    }
}

/// Each respawn on 0x2B re-rolls a class in 0x11-0x20 (`func_1507D1D8`, C): 0x11 + (rand & 0xF).
pub fn reroll(class: u8, rng: &mut crate::Rng) -> u8 {
    match class {
        0x11..=0x20 => 0x11 + (rng.next_u32() & 0xF) as u8,
        c => c,
    }
}

/// Item 0x3E's handler on a villager (`func_15065A5C` case 0x3E, C), before the movement: obj+0x83 is
/// kept at 1 or more (so the frame's end leaves obj+0xAA), and the boost buys 150 % while it lasts
/// 2 VIs, else 75 % (and it empties). Returns obj+0xAA.
pub fn drive(timer: &mut u8, boost: &mut u16) -> u8 {
    if *timer == 0 {
        *timer = 1;
    }
    match boost.checked_sub(2) {
        Some(left) => {
            *boost = left;
            FAST
        }
        None => {
            *boost = 0;
            SLOW
        }
    }
}

/// The plunger, prop 0 (placement 0, id 0xFF, the script's handle 0x30FF): stood on, or held by the
/// script (+0x4F bit 4), it sinks [`SINK`] a frame to [`DEPTH`] below home; let go, it springs back
/// a tenth of the way a frame, truncated, and settles 9 low (V wz 901-917, plg 958-980). The script
/// fires the detonation once it stops going down below home − 50 (0xB9 at 0x1964).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Plunger {
    /// prop+0x12, its y (`query(0x30FF, 0x1E)`).
    pub y: f32,
    /// +0x4F bit 4 as the script last set it (command 0xD): held down through the cooldown.
    pub held: bool,
    /// The last player to stand on it (`query(0x30FF, 0x17)`, the presser).
    pub toucher: Option<usize>,
}

pub const HOME: f32 = 392.0;
pub const SINK: f32 = 4.0;
pub const DEPTH: f32 = 60.0;
/// Its top (placement 0's collision, a flat 76 × 158 at y 508): who stands there stands on it.
/// ponytail: the collision is static, so the presser stays at 508 while the plunger sinks under it
/// (the original's goes down with it, V ev3: 448); move the placement's triangles if that matters.
pub const TOP: [[f32; 2]; 2] = [[-1508.0, -1432.0], [2268.0, 2426.0]];
pub const TOP_Y: f32 = 508.0;

impl Default for Plunger {
    fn default() -> Self {
        Self {
            y: HOME,
            held: false,
            toucher: None,
        }
    }
}

impl Plunger {
    /// Whether a player at `pos`, on the ground, stands on the top.
    pub fn under(pos: [f32; 3]) -> bool {
        let [x, z] = TOP;
        (x[0]..=x[1]).contains(&pos[0])
            && (z[0]..=z[1]).contains(&pos[2])
            && (pos[1] - TOP_Y).abs() < 1.0
    }

    /// One frame, `stood` the first player standing on it.
    pub fn step(&mut self, stood: Option<usize>) {
        if stood.is_some() {
            self.toucher = stood;
        }
        self.y = if stood.is_some() || self.held {
            (self.y - SINK).max(HOME - DEPTH)
        } else {
            self.y + ((HOME - self.y) * 0.1).trunc()
        };
    }
}

/// Where a Tedi inside the villagers' beaches (triggers 0x26 and 0x27, type 0 kind 5) is put: by the
/// mounted gun's seat (V kz 61 → 63, 201 → 202; esc 552 → 553; the engine's choice is unread).
pub const EJECT: [(usize, [f32; 3]); 2] = [
    (0x26, [-217.0, -387.0, -3621.0]),
    (0x27, [-495.0, -440.0, -3617.0]),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// V spr/spd: 75 % with no boost, a bottle's 600 VIs buy 300 frames of 150 %.
    #[test]
    fn the_item_drives_and_the_boost_drains() {
        let (mut timer, mut boost) = (0, 0);
        assert_eq!((drive(&mut timer, &mut boost), timer), (SLOW, 1));
        boost = BOOST;
        let fast = (0..400)
            .take_while(|_| drive(&mut timer, &mut boost) == FAST)
            .count();
        assert_eq!((fast, boost), (300, 0));
        assert_eq!(model(0x1C), 156);
        assert_eq!((model(0x1D), model(TEDI_CLASS)), (157, 117));
    }

    /// 392 → 332 in 15 frames stood on; let go, it springs back and settles at 383 (V wz, plg).
    #[test]
    fn the_plunger_sinks_and_springs_back() {
        let mut p = Plunger::default();
        let ys: Vec<f32> = (0..16)
            .map(|_| {
                p.step(Some(3));
                p.y
            })
            .collect();
        assert_eq!((ys[14], ys[15], p.toucher), (332.0, 332.0, Some(3)));
        for _ in 0..60 {
            p.step(None);
        }
        assert_eq!(p.y, 383.0);
        p.held = true;
        p.step(None);
        assert_eq!(p.y, 379.0);
        assert!(
            Plunger::under([-1470.0, 508.0, 2350.0]) && !Plunger::under([-1470.0, 412.0, 2350.0])
        );
    }
}
