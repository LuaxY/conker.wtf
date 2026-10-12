//! The bots' data from the player's ROM (skips without `CONKER_ROM`): scene 0x2D's navigation graph
//! and the multiplayer AI script.
// Trace values are copied as the oracle printed them (%.9g).
#![allow(clippy::excessive_precision)]

use conker_bots::nav::Graph;
use conker_bots::script::{Globals, Host, Machine, Script};

fn rom() -> Option<extract_core::Rom> {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return None;
    };
    Some(extract_core::Rom::new(std::fs::read(path).unwrap()).unwrap())
}

#[test]
fn navigation_graph_of_0x2d() {
    let Some(rom) = rom() else { return };
    let g = Graph::parse(
        &rom.asset(&[0x19, 0x2D]).unwrap(),
        &rom.asset(&[0x0C, 0x2D, 5]).unwrap(),
    )
    .unwrap();
    assert_eq!(g.nodes.len(), 143);
    // The band bounds the game derived in RAM (D_800D2360: -690, -130.5, 828.5, 100000).
    for (y, band) in [
        (-690.5, 0),
        (-690.0, 1),
        (-131.0, 1),
        (-130.5, 2),
        (828.0, 2),
        (828.5, 3),
    ] {
        assert_eq!(g.band(y), band, "y {y}");
    }
    // Node 0 as dumped from the oracle's RAM.
    let n = g.nodes[0];
    assert_eq!(
        (n.pos, n.band, n.half, n.next),
        ([1944, -521, -5718], 1, 0x5B, [0x4F, 0x0B, 0x09, 0x02, 0xFF])
    );
    assert_eq!(g.by_id(0x42), 65);
}

struct Count(usize);
impl Host for Count {
    fn native(&mut self, _: &mut Globals, _: i32, _: &[i32]) -> i32 {
        self.0 += 1;
        0
    }
}

#[test]
fn the_ai_script_runs() {
    let Some(rom) = rom() else { return };
    let s = Script::parse(&rom.asset(&[0x14, 0xBA]).unwrap()).unwrap();
    let (mut g, mut m, mut host) = (Globals::new(0x2600), Machine::default(), Count(0));
    m.run(&s, &mut g, &mut host, 1).unwrap();
    m.run(&s, &mut g, &mut host, 2).unwrap();
    assert!(host.0 > 10, "{} native calls", host.0);
}

/// The lifts of 0x2D: gates at the foot and top of each tower, to records 2-5 (V: the oracle's
/// bots3 p1 rides up between reads 604 and 605, p3 down between 1591 and 1592).
#[test]
fn lifts_of_0x2d() {
    let Some(rom) = rom() else { return };
    let lifts = conker_bots::lift::parse(&rom.asset(&[0x0C, 0x2D, 5]).unwrap(), 0x2D);
    let spawns = extract_scene::scene_spawns(&rom, 0x2D).unwrap();
    assert_eq!(
        lifts.iter().map(|l| l.record).collect::<Vec<_>>(),
        [4, 5, 3, 2]
    );
    let at = |old: [f32; 3], p: [f32; 3]| {
        lifts
            .iter()
            .find(|l| l.takes(old, p, p[1]))
            .map(|l| l.record)
    };
    let rows = [
        [678.039368, 77.8320312, -3882.42334],
        [670.80188, 82.1132812, -3867.11719],
        [663.564392, 86.421875, -3851.81104],
    ];
    assert_eq!(
        (at(rows[0], rows[1]), at(rows[1], rows[2])),
        (None, Some(4))
    );
    // Row 605 is record 4's placement (jitter up to 40 along its facing) plus one frame's move.
    let r = spawns[4].position.map(f32::from);
    let d = ((777.90094 - r[0]).powi(2) + (-3892.6311 - r[2]).powi(2)).sqrt();
    assert!(
        d < 40.0 + 18.0 && (1254.0 - r[1]).abs() < 5.0,
        "record 4 at {r:?}"
    );
    let top = [
        [897.60144, 1235.40234, -3967.76343],
        [908.899902, 1232.72266, -3980.86157],
    ];
    assert_eq!(at(top[0], top[1]), Some(5));
    // Walking back out the way it came in does not trigger.
    assert_eq!(at(rows[2], rows[1]), None);
}

/// Total War (player query 0xD5 = 0): the script's team branch puts each participant on its team
/// (G+0xE64 = team · 2), shares the enemies out among a team (`func_1508BF14`: a bot's threat is on
/// the other team or none) and calls no native the port lacks, the gas canister's timers run out
/// included; a human with 4 team kills and suicides is every bot's enemy (0x4CFE) until it dies
/// (0xB52 zeroes its count).
#[test]
fn total_war_runs_the_team_branch() {
    use conker_bots::{Assets, Bots, brain::g};
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, Settings};
    use conker_sim::collision::CollisionWorld;
    use conker_sim::movement::Input;
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x2D).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let graph = Graph::parse(assets.nav, assets.triggers).unwrap();
    let at = |node: usize, team| {
        let pos = graph.nodes[node].pos.map(f32::from);
        let place = Placement {
            index: 0,
            pos,
            facing: 0,
            camera: [0.0; 3],
        };
        Fighter::new(&place, team, 0)
    };
    let arena = Arena {
        spawns: Vec::new(),
        pickups: Vec::new(),
    };
    let players = vec![at(0, 0), at(40, 1), at(80, 0), at(120, 1)];
    let settings = Settings {
        total_war: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(settings);
    let mut bots = Bots::new(assets, 4, &[1, 2, 3], 2, Rng::default()).unwrap();
    let (world, mut inputs) = (CollisionWorld::default(), vec![Input::default(); 4]);
    let mut threats = 0;
    for _ in 0..1000 {
        bots.tick(&m, &arena, &world, &mut inputs);
        let g = bots.globals();
        for p in 1..4 {
            let e = g.get(g::THREAT, p);
            assert!(e == -1 || g.get(g::TEAM, e as usize) != g.get(g::TEAM, p));
            threats += usize::from(e != -1);
        }
    }
    let team = |b: &Bots, p| b.globals().get(g::TEAM, p);
    assert_eq!(
        (0..4).map(|p| team(&bots, p)).collect::<Vec<_>>(),
        [0, 2, 0, 2]
    );
    assert!(threats > 0);
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    m.players[0].suicides = 4;
    bots.tick(&m, &arena, &world, &mut inputs);
    assert_eq!(team(&bots, 0), 1, "the bots of both teams hunt it");
    m.players[0].health = 0;
    bots.tick(&m, &arena, &world, &mut inputs);
    m.players[0].health = 6;
    bots.tick(&m, &arena, &world, &mut inputs);
    assert_eq!(team(&bots, 0), 0, "forgiven at its death");
}

/// Scene 0x2D's collision as Deathmatch has it (oracle.rs' `world`).
fn world(rom: &extract_core::Rom) -> conker_sim::collision::CollisionWorld {
    use conker_sim::collision::{CollisionWorld, Layer};
    let placements = extract_scene::scene_placements(rom, 0x2D).unwrap();
    let collision = extract_scene::scene_collision(rom, 0x2D, &placements).unwrap();
    let solid = extract_scene::deathmatch_solid(0x2D);
    let triangles = (collision.triangles.iter())
        .filter(|t| !matches!(t.layer, extract_scene::Layer::Placement(i) if !solid.contains(&i)));
    let mut w = CollisionWorld::layered(triangles.map(|t| {
        let layer = match t.layer {
            extract_scene::Layer::Terrain => Layer::Terrain,
            extract_scene::Layer::Secondary => Layer::Secondary,
            extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
        };
        (t.points, t.surface.unwrap_or(0), layer)
    }));
    w.acos = conker_sim::trig::AcosTable::from_game_data(&rom.game_data().unwrap());
    w
}

/// The gas props' ids (placement byte 0x33, the script's `0x30nn` handles) and the canisters'
/// places, and the masks' ids k + 1 (`conker_rules::gas`, `weapon::PICKUPS_2D`).
#[test]
fn gas_props_of_0x2d() {
    use conker_rules::{gas, weapon};
    let Some(rom) = rom() else { return };
    let placements = extract_scene::scene_placements(&rom, 0x2D).unwrap();
    for (n, id) in gas::PROPS {
        assert_eq!(placements[usize::from(n)].raw[0x33], id, "placement {n}");
    }
    for (n, at) in gas::CANISTERS {
        assert_eq!(placements[usize::from(n)].position.map(f32::from), at);
    }
    for (k, p) in weapon::PICKUPS_2D.iter().enumerate().skip(19) {
        assert_eq!(p.gives, weapon::Gives::Mask);
        assert_eq!(
            placements[usize::from(p.placement)].raw[0x33] as usize,
            k + 1
        );
    }
}

/// War: Total War's gas, the script on the match with no bot (humans placed where the oracle's
/// runs d1-d3 had them, `lab/research/mp-original/total-war-gas.md`): canister 0 appears 300 +
/// random(150) script ticks in (one every other frame), the player of team 0 who touches it carries
/// it from the next frame (or the one after), the delivery in the shelter starts the 300-frame countdown, the doorway
/// hurts 1 a frame in its last 90, the gas kills whoever is outside the shelter and unmasked on the
/// 300th, crediting the deliverer with the enemies only, the masks come off 89 frames later and the
/// canister is back home 300 + random(150) ticks after the delivery.
#[test]
fn total_war_gas() {
    use conker_bots::{Assets, Bots};
    use conker_rules::game::{Arena, Event};
    use conker_rules::gas::CANISTERS;
    use conker_rules::{Fighter, Match, Placement, Rng, Settings, Spawn, weapon};
    use conker_sim::movement::{Input, PlayerState};
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x2D).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom);
    let placements = extract_scene::scene_placements(&rom, 0x2D).unwrap();
    let spawns = (extract_scene::scene_spawns(&rom, 0x2D).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let arena = Arena {
        spawns,
        pickups: (weapon::PICKUPS_2D.iter())
            .map(|p| {
                (
                    *p,
                    placements[usize::from(p.placement)].position.map(f32::from),
                )
            })
            .collect(),
    };
    // 0: team 0, the deliverer, waiting in the shelter (trigger 5, d1's p0); 1 and 2 outside, of
    // team 1 and 0; 3 in the doorway (trigger 0x50, d2's p0); 4 on mask k23 (placement 25, d2's p1).
    let shelter = [-106.0, -1448.0, -71.0];
    let roster = [
        (shelter, 0),
        ([3195.0, 1.0, -5861.0], 1),
        ([-1231.0, 1381.0, 3741.0], 0),
        ([313.0, -1546.0, 1012.0], 1),
        ([-2289.0, -2.0, 6865.0], 1),
    ];
    let place = |pos| Placement {
        index: 0,
        pos,
        facing: 0,
        camera: [0.0; 3],
    };
    let players = roster.map(|(pos, team)| Fighter::new(&place(pos), team, 0));
    let settings = Settings {
        total_war: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players.to_vec(), Rng::default(), 0).with(settings);
    for f in &mut m.players {
        f.immune = 0;
    }
    let mut bots = Bots::new(assets, 5, &[], 2, Rng::default()).unwrap();
    let inputs = [Input::default(); 5];
    let (canister, home) = CANISTERS[0];
    let (mut shown, mut touched, mut carried, mut delivered) = (None, None, None, None);
    let (mut back, mut sound, mut unmasked) = (None, Vec::new(), None);
    let mut doors = Vec::new();
    let mut hits = Vec::new();
    for t in 0..3000 {
        let effects = bots.tick(&m, &arena, &world, &mut [Input::default(); 5]);
        let masked = m.players[4].masked;
        let mut ev = m.apply(&effects);
        if unmasked.is_none() && masked && !m.players[4].masked {
            unmasked = Some(t);
        }
        let up = m.gas.prop(canister).unwrap().shown;
        if shown.is_none() && up {
            // Onto the canister: touched in this frame's step.
            shown = Some(t);
            m.players[0].body = PlayerState::spawn(home, 0);
        }
        if carried.is_none() && m.players[0].carrying {
            // Carried: back into the shelter.
            carried = Some(t);
            m.players[0].body = PlayerState::spawn(shelter, 0);
        }
        if delivered.is_none() && m.gas.countdown == 300 {
            delivered = Some(t);
            assert!(!m.players[0].carrying, "the canister goes at the delivery");
        }
        if delivered.is_some() && back.is_none() && up {
            back = Some(t);
        }
        if sound.last().is_none_or(|s: &(u32, bool)| s.1 != m.gas.gas) {
            sound.push((t, m.gas.gas));
        }
        let lids = m.gas.shown() & 0xFF;
        if doors.last().is_none_or(|d: &(u32, u32)| d.1 != lids) {
            doors.push((t, lids));
        }
        ev.extend(m.step(&arena, &inputs, &world));
        if touched.is_none() && m.gas.toucher(0).is_some() {
            touched = Some(t);
            assert_eq!(m.gas.toucher(0), Some(0));
        }
        for e in &ev {
            if let Event::Hit {
                victim,
                damage,
                lethal,
                ..
            } = *e
            {
                hits.push((t, victim, damage, lethal));
            }
        }
        bots.observe(&ev, &m);
    }
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    let shown = shown.expect("canister 0 shown");
    assert!((600..=902).contains(&shown), "{shown}");
    // Carried from the next frame (V d1 1039 → 1040), or the one after: the script ticks one
    // canister's timer a frame, by turns, and returns before the touches while it runs (0x4338).
    assert_eq!(touched, Some(shown));
    assert!(
        carried.is_some_and(|c| c - shown <= 2),
        "{carried:?} {shown}"
    );
    let d = delivered.expect("delivered");
    let back = back.expect("canister 0 back") - d;
    assert!((600..=902).contains(&back), "{back}");
    // The doorway: 1 a frame from countdown 89 on, killing in 6; the gas on the 300th frame.
    let doorway: Vec<_> = hits
        .iter()
        .filter(|h| h.1 == 3)
        .map(|h| (h.0 - d, h.2, h.3))
        .collect();
    let want: Vec<_> = (211..217).map(|t| (t, 1, t == 216)).collect();
    assert_eq!(doorway, want);
    let gassed: Vec<_> = hits
        .iter()
        .filter(|h| h.1 != 3)
        .map(|h| (h.0 - d, h.1, h.3))
        .collect();
    assert_eq!(gassed, [(300, 1, true), (300, 2, true)]);
    assert_eq!(unmasked.map(|u| u - d), Some(389));
    assert_eq!(
        sound
            .iter()
            .map(|s| (s.0.saturating_sub(d), s.1))
            .collect::<Vec<_>>(),
        [(0, false), (271, true), (389, false)]
    );
    // The doors (bits 0, 1) and covers (2-7), taken at each frame's start (a hide made in a step
    // shows on the next; V d2: doors hidden 23 → 74, shown at the delivery 1063 + 1, covers hidden
    // 1064 → 1076, back with the masks' removal, the doors hidden 50 frames later).
    assert_eq!(
        doors,
        [
            (0, 0xFF),
            (51, 0xFC),
            (d + 1, 0xFF),
            (d + 14, 0x03),
            (d + 389, 0xFF),
            (d + 440, 0xFC)
        ]
    );
    let score = |i: usize| {
        let f = &m.players[i];
        (f.kills, f.hits, f.suicides, f.deaths)
    };
    assert_eq!(score(0), (1, 1, 0, 0), "the enemy only");
    assert_eq!(
        (score(1), score(2), score(3)),
        ((0, 0, 0, 1), (0, 0, 0, 1), (0, 0, 0, 1))
    );
}
