//! `maps/<map>/race.json`: what Race (scenarios 11 and 12, on `33` only) adds to its map: the two
//! routes (the barriers each shows, one lap of track nodes, the overview map), the racers'
//! characters, and the board and helmets hung on them (`props.gltf`'s scenes)
//! (`../lab/research/mp-original/race.md`). Its power-ups are `pickups.json`'s, its AI script
//! `ai.bin` (`[0x14, 0xBC]`). Tags in the docs: V seen in the oracle, C read in code or ROM, I
//! inferred.

use std::collections::BTreeMap;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, invalid, parse_json, to_json};

/// `race.json`'s path, relative to the pack root.
pub fn race_json(map: &str) -> String {
    format!("maps/{map}/race.json")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Race {
    /// The route barriers (bundle segment `[4, 0x33, 4]`, prop ids 0xFF − k): solid while shown
    /// (`collision.json`'s placement layers), drawn by `level.gltf` (nodes `model@placement`).
    pub barriers: Vec<u16>,
    /// Scenario 11's route (the low one), then scenario 12's (the elevated one).
    pub routes: Vec<Route>,
    /// Class → character folder: the human racers' (0xB) and the bots' (0xC-0xF, by slot 0xD,
    /// 0xE, 0xF, 0xC, then again; V b2, cls).
    pub racers: BTreeMap<u8, String>,
    /// The hoverboard under every racer from its spawn to its death (script 0xBC's player command
    /// 0x78 → action 0x79, made again at each respawn; V k1, b2, cls).
    pub board: Attached,
    /// The helmet a bot class wears (C `func_15083384`, V cls); absent: none.
    pub heads: BTreeMap<u8, Attached>,
}

/// One route: the barriers it shows (the others hidden, prop +0x6E, from the match's start; V L1,
/// L2), one lap of the track, and its overview map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub scenario: u8,
    pub shown: Vec<u16>,
    /// Nav nodes (`nav.json` indices) in racing order, from the start line's node: a lap passes
    /// each once (the game's link pick by route, C `func_150888A8`; V L1, L2).
    pub lap: Vec<u8>,
    /// The overview map's texture in `hud.json` (C `func_150918EC`; V k1, b2).
    pub map: String,
}

/// A model hung on a racer: `props.gltf`'s glTF scene `scene`, drawn in joint `joint`'s frame
/// (character model units, as `held.json`'s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    pub scene: usize,
    pub joint: u8,
}

impl Race {
    /// Reads `maps/<map>/race.json` (`read` as [`crate::Slice::load`]'s) and checks that each
    /// route shows only barriers and that a lap passes a node once.
    pub fn load<E: Display>(
        map: &str,
        read: impl FnOnce(&str) -> Result<Vec<u8>, E>,
    ) -> Result<Self> {
        let file = race_json(map);
        let bytes = read(&file).map_err(|e| Error::Read {
            file: file.clone(),
            message: e.to_string(),
        })?;
        let race: Race = parse_json(&file, &bytes)?;
        for (i, r) in race.routes.iter().enumerate() {
            if let Some(p) = r.shown.iter().find(|p| !race.barriers.contains(p)) {
                let field = format!("routes[{i}].shown");
                return Err(invalid(&file, field, format!("{p} is not a barrier")));
            }
            if let Some(k) = (1..r.lap.len()).find(|&k| r.lap[..k].contains(&r.lap[k])) {
                let field = format!("routes[{i}].lap[{k}]");
                return Err(invalid(&file, field, "passed twice"));
            }
        }
        Ok(race)
    }

    pub fn to_json(&self) -> Vec<u8> {
        to_json(self, 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Race {
        let at = |scene, joint| Attached { scene, joint };
        Race {
            barriers: vec![0, 1, 2, 3],
            routes: vec![
                Route {
                    scenario: 11,
                    shown: vec![0, 3],
                    lap: vec![19, 89, 20, 18],
                    map: "race_map_low".into(),
                },
                Route {
                    scenario: 12,
                    shown: vec![1, 2],
                    lap: vec![19, 89, 58, 18],
                    map: "race_map_high".into(),
                },
            ],
            racers: [(0xB, "136".to_string()), (0xC, "144".to_string())].into(),
            board: at(0, 23),
            heads: [(0xD, at(1, 12))].into(),
        }
    }

    #[test]
    fn write_then_load_gives_the_same_race() {
        let race = sample();
        let json = race.to_json();
        let read = |path: &str| match path == "maps/33/race.json" {
            true => Ok(json.clone()),
            false => Err(format!("no {path}")),
        };
        assert_eq!(Race::load("33", read).unwrap(), race);
        assert_eq!(Race::load("33", read).unwrap().to_json(), json);
    }

    #[test]
    fn a_route_showing_a_non_barrier_or_a_lap_passing_a_node_twice_is_refused() {
        let refused = |race: Race| {
            let json = race.to_json();
            let error = Race::load("33", |_| Ok::<_, String>(json.clone())).unwrap_err();
            error.to_string()
        };
        let mut race = sample();
        race.routes[1].shown.push(4);
        assert_eq!(
            refused(race),
            "maps/33/race.json: routes[1].shown: 4 is not a barrier"
        );
        let mut race = sample();
        race.routes[0].lap.push(89);
        assert_eq!(
            refused(race),
            "maps/33/race.json: routes[0].lap[4]: passed twice"
        );
    }
}
