//! The characters a human may pick (`lab/research/mp-original/characters.md`): the menu's 22
//! entries ([`PICKS`]), the set each map's Deathmatch and Race offer with the cheats' additions
//! ([`allowed`], [`pick`]), and what a class is: its model ([`model`]), its scale ([`scale`]) and
//! its bodies ([`body`]). The team modes and Tank have no select: their classes are the scenario's.

use crate::damage::Hitbox;
use conker_anim::combat as anim;
use conker_sim::movement::Other;

/// The menu's entries (`D_800AB57C`, 8 bytes each: +6 the class, +5 the model; V): the squirrel,
/// the Sergeant, the Tedi, the Tediz Leader, the Wise Guy, the weasel guard, the caveman, the four
/// racers (no helmet, helmets 0x76, 0x78; 0xE is 0x77's), the raptor, the villager man, the two
/// bats (unreachable), Conker, Neo Conker, the villager woman, the two zombies and Gregg twice.
pub const PICKS: [(u8, u16); 22] = [
    (4, 128),
    (0x25, 176),
    (3, 117),
    (0x26, 177),
    (1, 59),
    (2, 152),
    (0xB, 136),
    (0xC, 144),
    (0xD, 144),
    (0xE, 144),
    (9, 83),
    (0x18, 156),
    (0x23, 155),
    (0x24, 154),
    (0xF, 144),
    (0, 0),
    (0x10, 150),
    (0x20, 157),
    (0x21, 159),
    (0x22, 160),
    (0x27, 178),
    (0x28, 180),
];

/// The cheats that add picks (`func_151EE184`'s codes, bank `[0x1C,0,0]` strings 0x90-0xA2; C
/// `func_151E0B70`): the code, its bit in `D_800E9D00`, and the picks it ORs into every set.
pub const CHEATS: [(&str, u32, u32); 7] = [
    ("EASTEREGGSRUS", 0x10, 0x10000),
    ("WELLYTOP", 0x20, 0x8000),
    ("CHINDITVICTORY", 0x40, 0x30),
    ("EATBOX", 0x80, 0x43C0),
    ("BEEFCURTAINS", 0x100, 0xE0800),
    ("RUSTYSHERIFFSBADGE", 0x10000, 0xA),
    ("BILLYMILLROUNDABOUT", 0x20000, 0x30_0000),
];

/// A scene's select: its set's picks (`0x8008FEA4`) and the defaults by controller (`0x8008FEC4`),
/// by the scenario row's +5 (V): Deathmatch on 0x2D, 0x34 and 0x30, on 0x1A, on 0x3F, and Race on
/// 0x33. None on 0x24 (Tank: its row's class 8 turns the picks off, C) and 0x2B (no Deathmatch
/// in the original).
fn set(scene: u16) -> Option<(u32, [u8; 4])> {
    match scene {
        0x2D | 0x34 | 0x30 => Some((0x815, [0, 2, 0xB, 4])),
        0x1A => Some((0x831, [4, 5, 0, 0xB])),
        0x3F => Some((0x5C0, [6, 7, 8, 0xA])),
        0x33 => Some((0x4380, [7, 8, 9, 0xE])),
        _ => None,
    }
}

/// Bit k: pick k may be played on `scene` (its Deathmatch, Race on 0x33) with the `cheats` bits
/// on. 0: no select.
pub fn allowed(scene: u16, cheats: u32) -> u32 {
    let added = CHEATS.iter().filter(|c| cheats & c.1 != 0);
    set(scene).map_or(0, |(mask, _)| added.fold(mask, |m, c| m | c.2))
}

/// The pick a human in `slot` plays on `scene` when it asks for `asked`: that one if allowed,
/// else the set's default for `slot & 3` (the original's defaults go by controller; no check that
/// two players picked the same, C). None: the scene has no select.
pub fn pick(scene: u16, cheats: u32, slot: usize, asked: u8) -> Option<u8> {
    let (_, defaults) = set(scene)?;
    let ok = asked < 32 && allowed(scene, cheats) & 1 << asked != 0;
    Some(if ok { asked } else { defaults[slot & 3] })
}

/// The model a class plays (`func_150169A0`'s switch, C; V every pick). None: the unused 5-7.
pub fn model(class: u8) -> Option<u16> {
    Some(match class {
        0 => 0,
        1 => 59,
        2 => 152,
        3 => 117,
        4 => 128,
        8 | 0xA => 119,
        9 => 83,
        0xB => 136,
        0xC..=0xF => 144,
        0x10 => 150,
        0x11..=0x1C => 156,
        0x1D..=0x20 => 157,
        0x21 => 159,
        0x22 => 160,
        0x23 => 155,
        0x24 => 154,
        0x25 => 176,
        0x26 => 177,
        0x27 => 178,
        0x28 => 180,
        _ => return None,
    })
}

/// A class's scale (the spawn record's +0x20/+0x24; V obj+0x14C): 0.6 for the Sergeant and the
/// Tediz Leader, 0.7 for the raptor, else 0.5.
pub fn scale(class: u8) -> f32 {
    match class {
        0x25 | 0x26 => 0.6,
        9 => 0.7,
        _ => 0.5,
    }
}

/// The clip class `class` plays for the rules' `id` (C, not seen): the Sergeant and the Tediz
/// Leader (their looks set s126+0x1B2, `func_15083384`) hold weapon 0x16 their own way, its drawn
/// idle 0x2C5 (`func_1507F640`), its walk 0x2C6 (`func_15064B94`) and its shot 0x2C7, on 0x2B too
/// (`func_15065A5C`), and sit the mounted gun as 0x34F and 0x350 (`func_15064A14`). Everyone but
/// the Tediz (models 117 and 177) pulls and throws the grenade as 0x175 and 0x16C (the same
/// animations, their events putting bank-09 #76 in the hand, not #75: `func_15065A5C`'s case
/// 0x26, C; V e4 sqgren: the squirrel, combat6_ctlw40c: the caveman). The rules keep the canonical
/// ids; the root motion and the drawing go by this one (0x2C7 has no root track: no push back).
pub fn clip(class: u8, id: u16) -> u16 {
    let tedi = matches!(model(class), Some(117 | 177));
    match (class, id) {
        (_, anim::GRENADE_PULL) if !tedi => 0x175,
        (_, anim::GRENADE_THROW) if !tedi => 0x16C,
        (0x25 | 0x26, anim::W16_IDLE) => 0x2C5,
        (0x25 | 0x26, anim::W16_WALK) => 0x2C6,
        (0x25 | 0x26, anim::W16_FIRE | crate::beach::ROCKET_FIRE) => 0x2C7,
        (0x25 | 0x26, anim::GUN_SEATED) => 0x34F,
        (0x25 | 0x26, anim::GUN_FIRING) => 0x350,
        _ => id,
    }
}

/// A model's bank-0x11 record at full size (C; the pack's `character.json` `hitbox` and `body`):
/// +0x20/+0x22/+0x24, the hitbox's radius, half height and centre; +0x1A/+0x1C/+0x1E, the push
/// body's radius, height and centre. None for the tank (119), whose bodies are its own
/// (`tank::BODY`, conker-sim's `tank::body`).
fn record(model: u16) -> Option<([i16; 3], [i16; 3])> {
    Some(match model {
        0 | 150 => ([49, 100, 87], [51, 97, 91]),
        59 | 128 | 136 | 156 | 176 | 177 => ([73, 128, 88], [60, 133, 66]),
        83 => ([179, 289, 179], [178, 290, 180]),
        117 | 178 | 180 => ([53, 87, 71], [47, 91, 66]),
        144 => ([93, 161, 88], [60, 133, 66]),
        152 => ([73, 128, 88], [34, 133, 66]),
        154 => ([120, 128, 88], [119, 121, 88]),
        155 => ([184, 184, 11], [89, 133, 66]),
        157 => ([73, 128, 88], [57, 133, 66]),
        159 => ([51, 112, 75], [40, 117, 64]),
        160 => ([47, 112, 88], [30, 114, 66]),
        _ => return None,
    })
}

/// A class's bodies at its scale: the area tests' and bullets' [`Hitbox`] (obj+0xD2/D4/D6) and the
/// push body (obj+0xE4/E6/E8: radius, height, centre; [`Body::at`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub hitbox: Hitbox,
    pub push: [i16; 3],
}

impl Body {
    /// The push body at `pos` (conker-sim's `Other`).
    pub fn at(&self, pos: [f32; 3]) -> Other {
        Other::new(pos, self.push)
    }
}

/// Class `class`'s bodies: its model's record × its scale, each truncated in f32 (C; V the
/// oracle's obj+0xD2.. for all 22 picks: 0.7 × 290 is 203). ponytail: a class with no record (the
/// tank, the unused 5-7) gets the squirrel's, the body its side gave it before; the tank's own
/// tests never read it.
pub fn body(class: u8) -> Body {
    let (hitbox, push) = model(class)
        .and_then(record)
        .unwrap_or(([73, 128, 88], [60, 133, 66]));
    let s = scale(class);
    let k = |v: i16| (f32::from(v) * s) as i16;
    let [radius, half, centre] = hitbox.map(|v| f32::from(k(v)));
    Body {
        hitbox: Hitbox {
            radius,
            half,
            centre,
        },
        push: push.map(k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// characters.md §3: each pick's class, model, scale, hitbox and body at the class's scale.
    #[test]
    fn every_pick_as_the_oracle() {
        #[rustfmt::skip]
        let table: [(u8, u16, f32, [i16; 3], [i16; 3]); 22] = [
            (4, 128, 0.5, [36, 64, 44], [30, 66, 33]),
            (0x25, 176, 0.6, [43, 76, 52], [36, 79, 39]),
            (3, 117, 0.5, [26, 43, 35], [23, 45, 33]),
            (0x26, 177, 0.6, [43, 76, 52], [36, 79, 39]),
            (1, 59, 0.5, [36, 64, 44], [30, 66, 33]),
            (2, 152, 0.5, [36, 64, 44], [17, 66, 33]),
            (0xB, 136, 0.5, [36, 64, 44], [30, 66, 33]),
            (0xC, 144, 0.5, [46, 80, 44], [30, 66, 33]),
            (0xD, 144, 0.5, [46, 80, 44], [30, 66, 33]),
            (0xE, 144, 0.5, [46, 80, 44], [30, 66, 33]),
            (9, 83, 0.7, [125, 202, 125], [124, 203, 126]),
            (0x18, 156, 0.5, [36, 64, 44], [30, 66, 33]),
            (0x23, 155, 0.5, [92, 92, 5], [44, 66, 33]),
            (0x24, 154, 0.5, [60, 64, 44], [59, 60, 44]),
            (0xF, 144, 0.5, [46, 80, 44], [30, 66, 33]),
            (0, 0, 0.5, [24, 50, 43], [25, 48, 45]),
            (0x10, 150, 0.5, [24, 50, 43], [25, 48, 45]),
            (0x20, 157, 0.5, [36, 64, 44], [28, 66, 33]),
            (0x21, 159, 0.5, [25, 56, 37], [20, 58, 32]),
            (0x22, 160, 0.5, [23, 56, 44], [15, 57, 33]),
            (0x27, 178, 0.5, [26, 43, 35], [23, 45, 33]),
            (0x28, 180, 0.5, [26, 43, 35], [23, 45, 33]),
        ];
        for (k, &(class, m, s, hit, push)) in table.iter().enumerate() {
            assert_eq!(PICKS[k], (class, m), "pick {k:#x}");
            assert_eq!((model(class), scale(class)), (Some(m), s), "pick {k:#x}");
            let b = body(class);
            let h = b.hitbox;
            assert_eq!(
                [h.radius, h.half, h.centre],
                hit.map(f32::from),
                "pick {k:#x}"
            );
            assert_eq!(b.push, push, "pick {k:#x}");
        }
        // The push body as the oracle's RAM has it (+0xEC = radius / height).
        assert_eq!(body(9).at([1.0; 3]), conker_sim::raptor::body([1.0; 3]));
    }

    /// A 0.6 pick pushes with its 36-radius body: two Sergeants 10 apart part by 0.4 of 62.
    #[test]
    fn a_sergeant_pushes_wider() {
        let me = body(0x25).at([0.0; 3]);
        assert_eq!((me.radius, me.lift, me.y_scale), (36, 39, 36.0 / 79.0));
        let mut pos = [0.0; 3];
        assert!(conker_sim::movement::push_apart(
            &mut pos,
            me,
            &[body(0x25).at([10.0, 0.0, 0.0])]
        ));
        assert!((pos[0] + 24.8).abs() < 1e-4, "{pos:?}");
        let mut pos = [0.0; 3];
        conker_sim::movement::push_apart(
            &mut pos,
            body(3).at([0.0; 3]),
            &[body(3).at([10.0, 0.0, 0.0])],
        );
        assert!((pos[0] + 14.4).abs() < 1e-4, "the Tedi's 23 + 23: {pos:?}");
    }

    #[test]
    fn the_sergeants_bazooka_and_gun_clips() {
        let ids = [0xBD, 0xE8, 0x77, 0x207, 0x1A1, 0x1A4, 0x76];
        for class in [0x25, 0x26] {
            let got = ids.map(|id| clip(class, id));
            assert_eq!(got, [0x2C5, 0x2C6, 0x2C7, 0x2C7, 0x34F, 0x350, 0x76]);
        }
        for class in [0, 3, 4, 9] {
            assert_eq!(ids.map(|id| clip(class, id)), ids);
        }
    }

    #[test]
    fn models_by_class() {
        let models: Vec<Option<u16>> = (0..=0x29).map(model).collect();
        assert_eq!(
            models[..5],
            [Some(0), Some(59), Some(152), Some(117), Some(128)]
        );
        assert_eq!(models[5..8], [None; 3]);
        assert_eq!(models[8..0xD], [119, 83, 119, 136, 144].map(Some));
        assert_eq!(models[0x11..=0x1C], [Some(156); 12]);
        assert_eq!(models[0x1D..=0x20], [Some(157); 4]);
        assert_eq!(
            models[0x21..=0x28],
            [159, 160, 155, 154, 176, 177, 178, 180].map(Some)
        );
        assert_eq!(models[0x29], None);
        // The villagers' looks agree with War: Beach's.
        for class in 0x11..=0x20 {
            assert_eq!(model(class), Some(crate::beach::model(class)));
        }
    }

    #[test]
    fn sets_and_cheats() {
        let bits = |m: u32| (0..22).filter(|k| m & 1 << k != 0).collect::<Vec<u8>>();
        for scene in [0x2D, 0x34, 0x30] {
            assert_eq!(bits(allowed(scene, 0)), [0, 2, 4, 0xB]);
        }
        assert_eq!(bits(allowed(0x1A, 0)), [0, 4, 5, 0xB]);
        assert_eq!(bits(allowed(0x3F, 0)), [6, 7, 8, 0xA]);
        assert_eq!(bits(allowed(0x33, 0)), [7, 8, 9, 0xE]);
        for scene in [0x24, 0x2B] {
            assert_eq!(allowed(scene, !0), 0, "{scene:#x}: no select");
        }
        // Each cheat adds its picks to every set.
        let adds = [
            vec![0x10],
            vec![0xF],
            vec![4, 5],
            vec![6, 7, 8, 9, 0xE],
            vec![0xB, 0x11, 0x12, 0x13],
            vec![1, 3],
            vec![0x14, 0x15],
        ];
        for ((_, bit, _), add) in CHEATS.iter().zip(adds) {
            let new = allowed(0x2D, *bit) & !allowed(0x2D, 0);
            assert_eq!(
                bits(new),
                add.into_iter()
                    .filter(|k| ![0, 2, 4, 0xB].contains(k))
                    .collect::<Vec<_>>()
            );
            assert_eq!(allowed(0x24, *bit), 0);
        }
        // All of them: everything but the Tedi's 2 and the squirrel's 0 on 0x3F, the raptor
        // only there, the bats never.
        let all = CHEATS.iter().fold(0, |m, c| m | c.1);
        assert_eq!(
            bits(allowed(0x3F, all)),
            [
                1, 3, 4, 5, 6, 7, 8, 9, 0xA, 0xB, 0xE, 0xF, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15
            ]
        );
        assert_eq!(allowed(0x2D, all) & 1 << 0xA, 0);
        assert_eq!(allowed(0x2D, all) & 0x3000, 0);
    }

    #[test]
    fn a_refused_pick_takes_the_slots_default() {
        assert_eq!(pick(0x2D, 0, 0, 2), Some(2));
        assert_eq!(pick(0x2D, 0, 1, 2), Some(2), "two may pick the same");
        assert_eq!(
            pick(0x2D, 0, 7, 0xF),
            Some(4),
            "Conker needs WELLYTOP: slot 7's default"
        );
        assert_eq!(pick(0x2D, 0x20, 7, 0xF), Some(0xF));
        assert_eq!(pick(0x3F, 0, 5, 0xA), Some(0xA));
        assert_eq!(pick(0x3F, 0, 4, 99), Some(6));
        assert_eq!(pick(0x33, 0, 2, 0), Some(9));
        assert_eq!(pick(0x24, !0, 0, 0), None);
    }
}
