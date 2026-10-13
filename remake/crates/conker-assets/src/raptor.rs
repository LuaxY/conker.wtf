//! `maps/<map>/raptor.json`: what War: Raptor (scenario 2, on `3f` only) adds to its map: the
//! placements it hides and shows, its pickups, the teams' spawn records, the zones of its script
//! (`[0x14, 0xBE]`, the map's `ai.bin`), the baby, the eggs and the body as the scene's character
//! records place them, and the raptor's rig and bites (`../lab/research/mp-original/raptor.md`).
//! Tags in the docs: V seen in the oracle, C read in code or ROM, I inferred.

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{Error, Pickup, Result, invalid, parse_json, to_json};

/// `raptor.json`'s path, relative to the pack root.
pub fn raptor_json(map: &str) -> String {
    format!("maps/{map}/raptor.json")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Raptor {
    /// The placements the match hides from its start (V r4: +0x6E set from read 24 to the end):
    /// the cooked eggs (until shown), three never shown (4-6) and Deathmatch's weapons (7-14). The
    /// level draws every placement but Deathmatch's hidden ones; the rest of the Raptor's
    /// (`cooked`, its bone weapons 15-22) are `props.gltf`'s, in place.
    pub hidden: Vec<u16>,
    /// The cooked eggs (bundle segment `[4, 0x3F, 4]`, prop ids 0xFF − k): a delivered egg's splat
    /// shows the next one in turn (k = G[0x24F8], then +1 & 3) and moves it to the egg's x and z,
    /// its y kept (C script 0xBE 0x476A-0x48C4; V r4).
    pub cooked: Vec<u16>,
    /// The pickups in the manager's order (k0-k9, `G[0xEA4]` = 10), on their placements: only
    /// cavemen take them (V r11).
    pub pickups: Vec<Pickup>,
    /// Each team's spawn records (`spawns.json`), by team rank: team 0 the cavemen (UGAS, east),
    /// team 1 the raptors (west, by the nest) (V; conker-rules' `RAPTOR_START_3F`).
    pub spawns: [Vec<usize>; 2],
    /// Trigger records (`triggers.json` indices) the script tests: a carrier inside `delivery`
    /// scores the egg; a raptor with a body inside one of `feed` feeds the baby (inside the first,
    /// the nest's ledge, turned to 340° first); a participant on the ground inside one of `hazard`
    /// is hurt and thrown up; `jumps` are the raptor bots' jump zones (C script 0xBE).
    pub delivery: usize,
    pub feed: Vec<usize>,
    pub hazard: Vec<usize>,
    pub jumps: Vec<usize>,
    /// The baby raptor, the body a standing bite leaves in the jaws (made at the kill, this record
    /// its look and scale: its place is the jaws'), and the three eggs.
    pub baby: Record,
    pub body: Record,
    pub eggs: Vec<Record>,
    pub raptor: Rig,
}

/// A character record of the scene (`[0x0E, scene]`, 0x30 bytes each).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Its character folder (the bank-01 model at +4).
    pub character: String,
    /// +0x28: the script's handle is 0x2000 | uid.
    pub uid: u8,
    /// +6: where it is made (the game puts the baby on the floor under or over it: y −10, V r3).
    pub at: [i16; 3],
    /// +0xC: its facing in 1/256 turns (the u16 facing is `yaw << 8`; V r3).
    pub yaw: u8,
    /// +0xD: the behaviour it starts in (V the baby's 43 and the eggs' 1; the kill sets the body's
    /// to 16, not this record's 1).
    pub start: u8,
    /// +0x20: its draw scale (of model units).
    pub scale: f32,
}

/// The raptor (class 9): its rig's joints (indices into the glTF skin), its draw scale and its
/// bites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub character: String,
    /// Model units to world (V obj+0x14C, `raptor-pack-scratch` s1; the body follows `carry` at
    /// 0.7 within 1.8 units, r2; not found in the ROM). No model lift (its bank-0x11 record's is 0).
    pub scale: f32,
    /// The skull (I: 316 of its vertices), the lower jaw (C: the model's special joint 0) and the
    /// joint a held body's origin sits on (V r2: joint 6's origin, the body turned with the raptor).
    pub head: usize,
    pub jaw: usize,
    pub carry: usize,
    /// Its class data's attack table (`[0x0F, 83]`, header word 1; 0x28-byte entries), the
    /// entries `hits` name (C `func_1505C7D8`, `raptor.md` §2.3).
    pub attacks: Vec<Attack>,
    /// Its clips' attack events (op 7, `func_1506BEDC`): from `t` (clip ticks) of logical clip
    /// `clip`, the area test of `attacks[attack]` runs every frame for `window` frames (obj+0xD0 =
    /// attack + 1, +0x114 = window); `attack` none ends it.
    pub hits: Vec<Hit>,
}

/// An attack entry: the fields the area test reads (C `func_1505C7D8`, with the raptor's scale
/// s): the area's centre `distance` × s / 0.125 / 2 ahead, turned `angle` × 256 from the facing,
/// `height` × s / 0.25 up; its radius `radius` × 4 × s (plus the victim's +0xD2); `damage`;
/// the victim's reaction (`reaction`: 0x46 a kill, 0x1C knocked up) and sound word (`sound`, 0:
/// none); `flags` (+0x24; C: with 0x800 the attack stays on after a hit, with 0x2000 a victim
/// behind a wall is spared). `raw` is the whole entry (hex), for the fields not named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attack {
    pub reaction: u8,
    pub sound: u32,
    pub angle: u8,
    pub distance: u8,
    pub height: i8,
    pub radius: u8,
    pub damage: u8,
    pub flags: u32,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub clip: u16,
    pub t: f32,
    pub attack: Option<usize>,
    pub window: u8,
}

impl Raptor {
    /// Reads `maps/<map>/raptor.json` (`read` as [`crate::Slice::load`]'s) and checks that every
    /// hit names an attack of the file.
    pub fn load<E: Display>(
        map: &str,
        read: impl FnOnce(&str) -> Result<Vec<u8>, E>,
    ) -> Result<Self> {
        let file = raptor_json(map);
        let bytes = read(&file).map_err(|e| Error::Read {
            file: file.clone(),
            message: e.to_string(),
        })?;
        let raptor: Raptor = parse_json(&file, &bytes)?;
        let attacks = raptor.raptor.attacks.len();
        if let Some(i) =
            (raptor.raptor.hits.iter()).position(|h| h.attack.is_some_and(|a| a >= attacks))
        {
            return Err(invalid(
                &file,
                format!("raptor.hits[{i}].attack"),
                format!("not below {attacks}"),
            ));
        }
        Ok(raptor)
    }

    pub fn to_json(&self) -> Vec<u8> {
        to_json(self, 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gives;

    fn sample() -> Raptor {
        let record = |character: &str, uid, at, scale| Record {
            character: character.into(),
            uid,
            at,
            yaw: 0x2D,
            start: 1,
            scale,
        };
        Raptor {
            hidden: (0..15).collect(),
            cooked: vec![0, 1, 2, 3],
            pickups: vec![Pickup {
                placement: 15,
                gives: Gives::Weapon(0x41),
                position: [5078.0, 637.0, -117.0],
                respawn: Some(240),
                roams: vec![],
            }],
            spawns: [(8..16).collect(), (0..8).collect()],
            delivery: 0,
            feed: vec![1, 0x13],
            hazard: vec![3, 0xD],
            jumps: vec![4, 5, 6, 7],
            baby: record("165", 0x11, [-6131, -113, -2072], 0.45),
            body: record("22", 0x13, [-6491, -26, -2533], 0.51),
            eggs: vec![record("171", 0x14, [-7008, 209, -3044], 0.2)],
            raptor: Rig {
                character: "83".into(),
                scale: 0.7,
                head: 5,
                jaw: 6,
                carry: 6,
                attacks: vec![Attack {
                    reaction: 0x46,
                    sound: 0,
                    angle: 0,
                    distance: 80,
                    height: -2,
                    radius: 30,
                    damage: 1,
                    flags: 0x800,
                    raw: "46".into(),
                }],
                hits: vec![
                    Hit {
                        clip: 49,
                        t: 11.0,
                        attack: Some(0),
                        window: 4,
                    },
                    Hit {
                        clip: 19,
                        t: 20.0,
                        attack: None,
                        window: 0,
                    },
                ],
            },
        }
    }

    #[test]
    fn write_then_load_gives_the_same_raptor() {
        let raptor = sample();
        let json = raptor.to_json();
        let read = |path: &str| match path == "maps/3f/raptor.json" {
            true => Ok(json.clone()),
            false => Err(format!("no {path}")),
        };
        assert_eq!(Raptor::load("3f", read).unwrap(), raptor);
        assert_eq!(Raptor::load("3f", read).unwrap().to_json(), json);
    }

    #[test]
    fn a_hit_past_the_attacks_is_refused() {
        let mut raptor = sample();
        raptor.raptor.hits[0].attack = Some(1);
        let json = raptor.to_json();
        let error = Raptor::load("3f", |_| Ok::<_, String>(json.clone())).unwrap_err();
        assert_eq!(
            error.to_string(),
            "maps/3f/raptor.json: raptor.hits[0].attack: not below 1"
        );
    }
}
