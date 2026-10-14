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
        ..Default::default()
    };
    let players = vec![at(0, 0), at(40, 1), at(80, 0), at(120, 1)];
    let settings = Settings {
        total_war: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(settings);
    let mut bots = Bots::new(assets, &[0], &[1, 2, 3], 2, Rng::default()).unwrap();
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

/// A scene's collision as Deathmatch has it (oracle.rs' `world`).
fn world(rom: &extract_core::Rom, scene: usize) -> conker_sim::collision::CollisionWorld {
    use conker_sim::collision::{CollisionWorld, Layer};
    let placements = extract_scene::scene_placements(rom, scene).unwrap();
    let collision = extract_scene::scene_collision(rom, scene, &placements).unwrap();
    let solid = extract_scene::deathmatch_solid(scene);
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

/// War: Total War's takeover, the script on the match (`t1`'s roster: humans 0 and 1, bots 2 and
/// 3, teams 0, 1, 0, 1; untimed, 2 lives): the human out of lives leaves the participants, the
/// script picks its team-mate bot and the match takes the swap 13 frames after the out frame (V t1:
/// out 424, swap 437); the bot is out from then on and the script asks no more.
#[test]
fn total_war_takeover() {
    use conker_bots::{Assets, Bots};
    use conker_rules::game::{Arena, Event};
    use conker_rules::gas::Effect;
    use conker_rules::{Fighter, Match, Placement, Rng, Settings, Spawn, damage};
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
    let world = world(&rom, 0x2D);
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
        pickups: Vec::new(),
        ..Default::default()
    };
    let place = |pos| Placement {
        index: 0,
        pos,
        facing: 0,
        camera: [0.0; 3],
    };
    let roster = [
        ([-106.0, -1448.0, -71.0], 0),
        ([3195.0, 1.0, -5861.0], 1),
        ([-1231.0, 1381.0, 3741.0], 0),
        ([313.0, -1546.0, 1012.0], 1),
    ];
    let players = roster.map(|(pos, team)| Fighter::new(&place(pos), team, 0));
    let settings = Settings {
        time: None,
        lives: 2,
        total_war: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players.to_vec(), Rng::default(), 0).with(settings);
    for (k, f) in m.players.iter_mut().enumerate() {
        (f.bot, f.immune) = (k >= 2, 0);
    }
    m.players[0].deaths = 1;
    let mut bots = Bots::new(assets, &[0, 1], &[2, 3], 2, Rng::default()).unwrap();
    m.apply(&[Effect::Hit {
        slot: 0,
        kind: damage::GAS,
        attacker: Some(1),
    }]);
    let (mut took, mut asked) = (Vec::new(), Vec::new());
    for t in 0..400u32 {
        let mut inputs = [Input::default(); 4];
        let effects = bots.tick(&m, &arena, &world, &mut inputs);
        asked.extend((effects.iter()).filter_map(|e| match e {
            Effect::TakeOver(i, j) => Some((t, *i, *j)),
            _ => None,
        }));
        let mut ev = m.apply(&effects);
        ev.extend(m.step(&arena, &inputs, &world));
        took.extend((ev.iter()).filter_map(|e| match *e {
            Event::TakeOver { player, from } => Some((t, player, from)),
            _ => None,
        }));
        bots.observe(&ev, &m);
    }
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?} {}",
        bots.unknown,
        bots.faults
    );
    let out = m.players[0].out_at;
    assert_eq!(took, [(out + 13, 0, 2)]);
    assert_eq!(asked, took, "the brain's own gate is the match's");
    // The bot is out on the human's place, the last of 4.
    assert_eq!(
        (m.players[0].out, m.lives_left(0), m.players[2].out),
        (0, 2, 4)
    );
}

/// War: Total War's gas, the script on the match with no bot (humans placed where the oracle's
/// runs d1-d3 had them, `lab/research/mp-original/total-war-gas.md`): canister 0 appears 300 +
/// random(150) script ticks in (one every other frame), the player of team 0 who touches it carries
/// it from the next frame (or the one after), the delivery in the shelter starts the 300-frame countdown, the doorway
/// hurts 1 a frame in its last 90, the gas kills whoever is outside the shelter and unmasked on the
/// 300th, crediting the deliverer with the enemies only, the gassed screen starts for those outside
/// 29 frames before, the masks come off 89 frames later and the canister is back home 300 +
/// random(150) ticks after the delivery.
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
    let world = world(&rom, 0x2D);
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
        ..Default::default()
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
    let mut bots = Bots::new(assets, &[0, 1, 2, 3, 4], &[], 2, Rng::default()).unwrap();
    let inputs = [Input::default(); 5];
    let (canister, home) = CANISTERS[0];
    let (mut shown, mut touched, mut carried, mut delivered) = (None, None, None, None);
    let (mut back, mut sound, mut unmasked) = (None, Vec::new(), None);
    let mut doors = Vec::new();
    let (mut hits, mut hazes) = (Vec::new(), Vec::new());
    for t in 0..3000 {
        let effects = bots.tick(&m, &arena, &world, &mut [Input::default(); 5]);
        for e in &effects {
            if let conker_rules::gas::Effect::Haze(slot) = e {
                hazes.push((t, *slot));
            }
        }
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
    // The gassed screen: at countdown 29, with the gas's sound, for everyone outside the shelter
    // (trigger 0), masked (4) or not; not the deliverer inside (0), nor the doorway's dead (3).
    let hazes: Vec<_> = hazes.iter().map(|h| (h.0 - d, h.1)).collect();
    assert_eq!(hazes, [(271, 1), (271, 2), (271, 4)]);
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

/// War: Colors (script 0xBB on scene 0x30, two humans, no bot; `lab/research/mp-original/
/// colors.md`): the flags appear 300 frames after live; the squirrel beside the red flag grabs it
/// (17 frames) and the Tedi beside the green one; B puts the red down 20 frames in, untouchable for
/// 50; let lie, it is gone 900 frames after and home 300 later. Carried into the squirrels' zone
/// (trigger 0xE) it scores on the capture's frame and the plant lets go 19 frames in. The first
/// capture finds the Tedi carrying the green flag: destroyed at once, and while it waits to
/// respawn the script's flag routine returns early on every other call, so the after-capture
/// timer runs at half speed (respawn at +150, flags home at +450; C, the routine at 0x41D4).
/// The second is plain (V bots1): the cheer and the freeze at +30, everyone respawned at +90, both
/// flags home at +390. The third reaches the score target and ends the match on its frame.
#[test]
fn colors_flags_and_captures() {
    use conker_bots::{Assets, Bots};
    use conker_rules::colors::Pose;
    use conker_rules::game::{Action, BUTTON_B, Event, Phase};
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, Settings, Spawn};
    use conker_sim::movement::{Input, PlayerState};
    /// A capture's timeline, in frames from its own.
    #[derive(Debug, Default)]
    struct Capture {
        at: u32,
        cancelled: Option<u32>,
        let_go: Option<u32>,
        cheer: Option<u32>,
        still: Vec<u32>,
        respawns: Vec<(u32, usize)>,
        home: Option<u32>,
        fades: Vec<u32>,
        fading: Option<u32>,
        over: Option<u32>,
    }
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x30).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom, 0x30);
    let spawns = (extract_scene::scene_spawns(&rom, 0x30).unwrap().iter())
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
        pickups: Vec::new(),
        ..Default::default()
    };
    // Beside the flags as e1's P1 and P2 were teleported (42 units off), and the squirrels' zone
    // on its floor.
    let (by_red, by_green) = ([-288.0, 410.07, -3803.0], [172.6, 1342.0, 6144.0]);
    let zone_y = world.floor(211.0, 9287.0, -300.0, -300.0, None).unwrap().y;
    let zone = [211.0, zone_y, 9287.0];
    let place = |pos| Placement {
        index: 0,
        pos,
        facing: 0,
        camera: [0.0; 3],
    };
    let mut players = vec![
        Fighter::new(&place(by_red), 0, 0),
        Fighter::new(&place(by_green), 1, 0),
    ];
    for (f, mask) in players.iter_mut().zip(conker_rules::spawn::COLORS_MASKS_30) {
        f.spawn_mask = mask;
    }
    let settings = Settings {
        colors: true,
        score: 3,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(settings);
    let mut bots = Bots::new(assets, &[0, 1], &[], 2, Rng::default()).unwrap();
    let held = |m: &Match, i: usize| m.players[i].flag;
    let (mut shown, mut grabbed, mut released) = (None, None, Vec::new());
    let (mut retaken, mut gone, mut back, mut carried) = (None, None, None, None);
    let mut captures: Vec<Capture> = Vec::new();
    for t in 0..6000u32 {
        let effects = bots.tick(&m, &arena, &world, &mut [Input::default(); 2]);
        let before = m.colors.scores[0];
        let mut ev = m.apply(&effects);
        if m.colors.scores[0] > before {
            assert_eq!(m.players[0].action, Action::Pose(Pose::Plant));
            captures.push(Capture {
                at: t,
                ..Capture::default()
            });
        }
        let red = m.colors.flags[0];
        if shown.is_none() && red.shown {
            shown = Some(t);
        }
        if gone.is_some() && back.is_none() && red.home() {
            back = Some(t);
        }
        // Both home: before the step, which may take the red one.
        if let Some(c) = captures.last_mut()
            && c.home.is_none()
            && t > c.at + 100
            && m.colors.flags.iter().all(|f| f.home())
        {
            c.home = Some(t - c.at);
        }
        // B 30 frames into the carry, and again once retaken; then off to the base. Once the red
        // flag is home again, onto it and into the zone, for each capture.
        let mut inputs = [Input::default(); 2];
        if [grabbed, retaken]
            .iter()
            .any(|g| g.is_some_and(|g| t == g + 30))
        {
            inputs[0].buttons = BUTTON_B;
        }
        if released.len() == 2 && released[1] + 1 == t {
            m.players[0].body = PlayerState::spawn([192.0, 17.0, 8300.0], 0);
        }
        let free = m.players[0].action == Action::Free;
        if back.is_some() && red.home() && held(&m, 0).is_none() && free {
            m.players[0].body = PlayerState::spawn(by_red, 0);
        }
        // ponytail: carried 120 frames first (a real run takes some 450): the script's
        // after-capture timer goes on 100 frames past the flags' return (G+0x1EE0 down to -100),
        // and a capture before that waits for it.
        if back.is_some() && held(&m, 0).is_some() && free && carried.is_some_and(|c| t >= c + 120)
        {
            m.players[0].body = PlayerState::spawn(zone, 0);
        }
        let had = held(&m, 0);
        ev.extend(m.step(&arena, &inputs, &world));
        if had.is_none() && held(&m, 0).is_some() {
            carried = Some(t);
        }
        let f0 = &m.players[0];
        if grabbed.is_none() && held(&m, 0) == Some(0) {
            grabbed = Some(t);
            assert_eq!(
                (f0.action, f0.anim),
                (Action::Pose(Pose::Grab), Pose::Grab.clip().0)
            );
        }
        if captures.is_empty() && held(&m, 0).is_none() && red.carrier.is_some() {
            released.push(t);
            assert_eq!(f0.action, Action::Pose(Pose::PutDown));
            assert!(m.colors.flags[0].dropped && !m.colors.flags[0].touchable(t + 50));
        }
        if released.len() == 1 && retaken.is_none() && held(&m, 0).is_some() {
            retaken = Some(t);
        }
        if released.len() == 2 && gone.is_none() && !m.colors.flags[0].shown {
            gone = Some(t);
        }
        if let Some(c) = captures.last_mut() {
            let k = t - c.at;
            if c.cancelled.is_none() && k < 10 && held(&m, 1).is_none() && !m.colors.flags[1].shown
            {
                c.cancelled = Some(k);
            }
            if c.let_go.is_none() && m.colors.flags[0].released == Some(t) {
                c.let_go = Some(k);
            }
            if c.cheer.is_none() && m.players[0].action == Action::Pose(Pose::Cheer) {
                c.cheer = Some(k);
                assert_eq!(m.players[1].action, Action::Pose(Pose::Lose));
            }
            // Held: +0x89 = 200 from the script each frame (the step counts it down one).
            if m.players.iter().all(|f| f.body.lock >= 199) {
                c.still.push(k);
            }
            for e in &ev {
                if let Event::Respawn { player, .. } = e {
                    c.respawns.push((k, *player));
                }
            }
            if m.colors.fade.is_some_and(|f| f.2 == t) {
                c.fades.push(k);
            }
            if c.fading.is_none() && m.phase() == Phase::Fading {
                c.fading = Some(k);
            }
            if m.phase() == Phase::Over {
                c.over = Some(k);
            }
        }
        bots.observe(&ev, &m);
        if m.phase() == Phase::Over {
            break;
        }
    }
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?} {}",
        bots.unknown,
        bots.faults
    );
    // Live + 300 (V e1 322/323 with live at 23), grabbed by the squirrel beside it at once.
    let shown = shown.expect("red shown");
    assert!((299..=301).contains(&shown), "{shown}");
    assert_eq!(grabbed, Some(shown));
    // The grab's 17 frames, then the carry; B 30 frames in lets go 20 frames later (V b1 350 →
    // 370), and the flag is untouchable 50 frames: retaken on the 51st, standing on it (V b1 421).
    let g = grabbed.unwrap();
    assert_eq!(released.first(), Some(&(g + 30 + 20)));
    assert_eq!(retaken, Some(released[0] + 51));
    // Let lie: gone 900 frames on (the script's 450 ticks, from its next look), home 300 after.
    let gone = gone.expect("gone") - released[1];
    assert!((900..=904).contains(&gone), "{gone}");
    assert_eq!(back.map(|b| b - released[1] - gone), Some(300));
    // The captures.
    assert_eq!(captures.len(), 3, "{captures:?}");
    let (c1, c2) = (&captures[0], &captures[1]);
    assert!(c1.cancelled.is_some_and(|c| c <= 2), "{c1:?}");
    assert_eq!((c1.let_go, c1.cheer), (Some(19), Some(30)), "{c1:?}");
    assert_eq!(c1.respawns, [(150, 0), (150, 1)], "{c1:?}");
    assert_eq!(c1.home, Some(450), "{c1:?}");
    assert_eq!(
        (c2.cancelled, c2.let_go, c2.cheer),
        (None, Some(19), Some(30)),
        "{c2:?}"
    );
    assert_eq!(c2.still, (31..90).collect::<Vec<_>>(), "{c2:?}");
    assert_eq!(c2.respawns, [(90, 0), (90, 1)], "{c2:?}");
    assert_eq!(c2.home, Some(390), "{c2:?}");
    // The target: the third capture latches the end on its frame (V c1b 382 → 383): no capture
    // fade nor respawn, the poses at +30, the end fade at +39 (V 422) and the outro at +55 (V 438).
    assert_eq!(m.colors.scores, [3, 0]);
    assert_eq!(m.ended_at, Some(captures[2].at));
    let c3 = &captures[2];
    assert_eq!(
        (c3.cheer, c3.fading, c3.over),
        (Some(30), Some(39), Some(55)),
        "{c3:?}"
    );
    assert!(c3.fades.is_empty() && c3.respawns.is_empty(), "{c3:?}");
    assert_eq!(c2.fades.first(), Some(&61), "{c2:?}");
}

/// War: Colors through a roster change mid-match: a human leaves while the red flag is carried and
/// another joins in its slot; the script goes on (its globals and the flags as they were: both
/// still carried, nothing hidden for a fresh start's 300 frames), only the changing participant
/// starts over.
#[test]
fn colors_keeps_its_flags_through_a_join_and_a_leave() {
    use conker_bots::brain::g;
    use conker_bots::{Assets, Bots};
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, Settings, Spawn};
    use conker_sim::movement::Input;
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x30).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom, 0x30);
    let spawns = (extract_scene::scene_spawns(&rom, 0x30).unwrap().iter())
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
        pickups: Vec::new(),
        ..Default::default()
    };
    let place = |pos| Placement {
        index: 0,
        pos,
        facing: 0,
        camera: [0.0; 3],
    };
    // The squirrel beside the red flag (as in `colors_flags_and_captures`), two Tediz by the green.
    let (by_red, by_green) = ([-288.0, 410.07, -3803.0], [172.6, 1342.0, 6144.0]);
    let mut players = vec![
        Fighter::new(&place(by_red), 0, 0),
        Fighter::new(&place(by_green), 1, 0),
        Fighter::new(&place(by_green), 1, 0),
    ];
    for (f, mask) in players.iter_mut().zip(conker_rules::spawn::COLORS_MASKS_30) {
        f.spawn_mask = mask;
    }
    let settings = Settings {
        colors: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(settings);
    let mut bots = Bots::new(assets, &[0, 1, 2], &[], 2, Rng::default()).unwrap();
    let (mut grabbed, mut green_by) = (None, None);
    for t in 0..1200u32 {
        if grabbed.is_some_and(|g| t == g + 40) {
            bots.leave(2);
        }
        if grabbed.is_some_and(|g| t == g + 80) {
            bots.join(2, false);
        }
        let effects = bots.tick(&m, &arena, &world, &mut [Input::default(); 3]);
        let ev = m.apply(&effects);
        let mut ev2 = m.step(&arena, &[Input::default(); 3], &world);
        ev2.extend(ev);
        bots.observe(&ev2, &m);
        // Vacant, participant 2 is of no team (no human takes a bot over by it); back, a Tedi.
        if let Some(k) = grabbed.map(|g| t - g).filter(|&k| k == 60 || k == 100) {
            let team = bots.globals().get(g::TEAM, 2);
            assert_eq!(team, if k == 60 { -1 } else { 1 }, "{k} after the grab");
        }
        let [red, green] = m.colors.flags;
        if grabbed.is_none() && red.carrier == Some(0) {
            grabbed = Some(t);
        }
        if let Some(g) = grabbed.filter(|&g| t > g + 20) {
            assert_eq!(red.carrier, Some(0), "frame {t}, {} after the grab", t - g);
            assert_eq!(green.carrier, green_by, "frame {t}");
            assert!(red.shown && green.shown, "frame {t}");
        }
        if grabbed.is_some_and(|g| t == g + 20) {
            green_by = green.carrier;
        }
    }
    assert!(grabbed.is_some_and(|g| g < 400), "{grabbed:?}");
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?} {}",
        bots.unknown,
        bots.faults
    );
}

/// War: Colors with bots, as the server runs it (7 bots, the squirrels in the even slots, and an
/// idle human, a Tedi): a collector takes the enemy flag within 5,400 frames (here a Tedi, the
/// green flag, at 2,292; V bots1: a squirrel, the red flag, 1,719 frames in). The collectors' way
/// to each flag passes a melee pickup (7 at node 109, 6 by the green flag's ledge): a bot's reach
/// is the script's, 0 out of state 6 (V, the oracle's s126+0x1AF), so it leaves it. Without the
/// reach it took it, turned back to fetch its preferred weapon (state 6), and no flag was taken
/// in 9,900 frames.
#[test]
fn colors_bots_take_a_flag() {
    use conker_bots::lift::Lifts;
    use conker_bots::{Assets, Bots, DEFAULT_SKILL};
    use conker_rules::{Arena, Fighter, Match, Rng, Settings, Spawn, spawn, weapon};
    use conker_sim::movement::{Input, Side};
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x30).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom, 0x30);
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(&rom, 0x30).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let (picks, spots) = weapon::pickups(0x30).unwrap();
    let pickups = (picks.iter().zip(spots))
        .map(|(p, at)| (*p, at.map(f32::from)))
        .collect();
    let arena = Arena {
        spawns,
        pickups,
        ..Default::default()
    };
    // The server's places: each side's k-th player on its k-th start record.
    let starts = spawn::starts(0x30).unwrap();
    let mut rng = Rng::default();
    let mut members = [0; 2];
    let players: Vec<Fighter> = (0..8)
        .map(|slot| {
            let team = if slot == 7 { 1 } else { slot % 2 };
            let record = starts.sides[team][members[team]];
            members[team] += 1;
            let place = spawn::place(&arena.spawns, usize::from(record), &mut rng);
            Fighter {
                side: [Side::Squirrels, Side::Tediz][team],
                class: [4, 3][team],
                spawn_mask: starts.side_masks[team],
                bot: slot < 7,
                ..Fighter::new(&place, team as u8, 0)
            }
        })
        .collect();
    let settings = Settings {
        colors: true,
        ..Settings::default()
    };
    let mut m = Match::new(&arena, players, rng, 0).with(settings);
    let slots: Vec<usize> = (0..7).collect();
    let mut bots = Bots::new(assets, &[7], &slots, DEFAULT_SKILL, rng).unwrap();
    let mut lifts = Lifts::new(assets.triggers, 0x30);
    let mut taken = None;
    for t in 0..5400 {
        for (slot, _) in lifts.ride(&mut m, &arena) {
            bots.lifted(slot);
        }
        let mut inputs = vec![Input::default(); 8];
        let effects = bots.tick(&m, &arena, &world, &mut inputs);
        let mut ev = m.apply(&effects);
        bots.scopes(&mut m);
        ev.extend(m.step(&arena, &inputs, &world));
        bots.observe(&ev, &m);
        if let Some(c) = m.colors.flags.iter().find_map(|f| f.carrier) {
            taken = Some((t, c));
            break;
        }
    }
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    let (t, carrier) = taken.expect("a flag taken");
    assert!(carrier < 7, "by a bot, at {t}: slot {carrier}");
}

/// The mounted guns' bodies on scene 0x30 against the oracle (`mounted-gun-scratch/body/`, Tedis):
/// p1, a player put 40 in front of gun B (manned) at read 100 on its slope, pushed out to rest
/// 81.65 from the origin (the 3-D gap 93); w1, one skidding into gun B's side at 17.5 a frame from
/// read 438, stopped 89.498 out; t1, gun A free: a player put on its floor 91 from the origin does
/// not touch it, one 88 out does (pushed to 88.55 first).
#[test]
fn mounted_gun_bodies() {
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, gun};
    use conker_sim::movement::{Input, state};
    let Some(rom) = rom() else { return };
    let world = world(&rom, 0x30);
    let arena = Arena {
        spawns: Vec::new(),
        pickups: Vec::new(),
        ..Default::default()
    };
    let fighter = |pos, facing, team| {
        let place = Placement {
            index: 0,
            pos,
            facing,
            camera: [0.0; 3],
        };
        Fighter::new(&place, team, 0)
    };
    let near = |a: [f32; 3], b: [f32; 3], d: f32| a.iter().zip(b).all(|(x, y)| (x - y).abs() < d);
    // P1 on gun B's seat, P2 at `p2`; P2's rows as the match steps with idle pads.
    let manned = |p2: Fighter, frames: u32| {
        let b = gun::places(0x30)[1];
        let mut p1 = fighter(gun::seat(b.pos, b.yaw, 0), b.yaw, 0);
        p1.gun = Some(gun::Mount {
            state: gun::IDLE,
            ..gun::Mount::new(1, b.pos)
        });
        let mut m = Match::new(&arena, vec![p1, p2], Rng::default(), 0);
        m.guns = gun::spawn(0x30);
        (0..frames)
            .map(|_| {
                m.step(&arena, &[Input::default(); 2], &world);
                assert_eq!(m.players[1].gun, None);
                m.players[1].body.pos
            })
            .collect::<Vec<_>>()
    };
    // On the slope the floor under it rises 0.7 above the oracle's on rows 102-110 (the floor
    // routine's, I), which shifts its path by up to 0.19; it comes to rest where the oracle's does.
    let p1 = manned(fighter([170.0, 544.0, 5898.0], 17152, 1), 30);
    for (read, at, d) in [
        (101, [170.0, 531.035, 5882.553], 0.01),
        (102, [170.0, 523.875, 5872.809], 0.75),
        (105, [170.0, 515.285, 5860.361], 0.2),
        (110, [170.0, 512.391, 5856.788], 0.75),
        (130, [170.0, 512.391, 5856.354], 0.01),
    ] {
        let got = p1[read - 101];
        assert!(near(got, at, d), "p1 {read}: {got:?}, want {at:?}");
    }
    let mut p2 = fighter([233.3426, 544.0, 6015.0386], 34465, 1);
    (p2.body.state, p2.body.xz_speed, p2.body.timer) = (state::SKID, 17.5, 10);
    let w1 = manned(p2, 20);
    for (read, at) in [
        (439, [225.9417, 544.0, 6016.2534]),
        (441, [214.8402, 544.0, 6018.0757]),
        (443, [208.6728, 544.0, 6019.0879]),
        (444, [207.4395, 544.0, 6019.2910]),
        (458, [207.4404, 544.0, 6019.2910]),
    ] {
        let got = w1[read - 439];
        assert!(near(got, at, 0.01), "w1 {read}: {got:?}, want {at:?}");
    }
    // t1: gun A free, nobody on it.
    for (x, touched, after) in [(-231.0, false, -231.0), (-234.0, true, -233.4457)] {
        let p = fighter([x, -387.3711, -3621.0], 17152, 1);
        let far = fighter([-436.0, -914.41, -5917.0], 17152, 0);
        let mut m = Match::new(&arena, vec![far, p], Rng::default(), 0);
        m.guns = gun::spawn(0x30);
        m.step(&arena, &[Input::default(); 2], &world);
        let f = &m.players[1];
        assert_eq!(f.gun.is_some(), touched, "t1 at {x}");
        assert!(
            (f.body.pos[0] - after).abs() < 0.01,
            "t1 at {x}: {:?}",
            f.body.pos
        );
    }
}

/// The mounted guns against the oracle (`lab/research/mp-original/mounted-gun.md` §5.4), each P1
/// where the run's poke put it (on the floor). m2 (gun B, scene 0x30): P1 jumps on at read 61, the
/// gun's body pushing it out first (to 66.84 from the origin; here 64.66, from the floor where the
/// poke left it 16 above, I): 0x32 for 19 rows, then 0x38,
/// the ceiling stops the rise at row 72 (687, velocity 0), the seat at 83, 0x1A1 from 84. e1 (scene
/// 0x34): the jump from read 41 lands at 66; seated, 12° down, Z from read 100 hits P2 300 ahead
/// once in 60 frames, on the first shot (6 → 4, 0x19C), and flings it about 140 back.
#[test]
fn mounted_guns_m2_and_e1() {
    use conker_rules::game::Event;
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, gun};
    use conker_sim::movement::{BUTTON_A, BUTTON_Z, Input};
    let Some(rom) = rom() else { return };
    let arena = Arena {
        spawns: Vec::new(),
        pickups: Vec::new(),
        ..Default::default()
    };
    let place = |pos, facing| Placement {
        index: 0,
        pos,
        facing,
        camera: [0.0; 3],
    };
    let near = |a: [f32; 3], b: [f32; 3], d: f32| a.iter().zip(b).all(|(x, y)| (x - y).abs() < d);
    // Script 0xBB walks a bot to node 3 or 0x50 for gun 0x2012 or 0x2013 (its +0x13C at
    // G+0x25AC[0] or [1]): guns A and B, [`gun::places`]' order.
    let graph = conker_bots::nav::Graph::parse(
        &rom.asset(&[0x19, 0x30]).unwrap(),
        &rom.asset(&[0x0C, 0x30, 5]).unwrap(),
    )
    .unwrap();
    for (id, g) in [(3, 0), (0x50, 1)] {
        let (n, at) = (graph.at(graph.by_id(id)), gun::places(0x30)[g].pos);
        let d = ((n[0] - at[0]).powi(2) + (n[2] - at[2]).powi(2)).sqrt();
        assert!(d < 400.0, "node {id:#x} {n:?}, gun {g} {at:?}");
    }
    let world = world(&rom, 0x30);
    let players = vec![
        Fighter::new(&place([170.0, 544.0, 5990.0], 50176), 0, 61),
        Fighter::new(&place([-436.0, -914.41, -5917.0], 17152), 1, 61),
    ];
    let mut m = Match::new(&arena, players, Rng::default(), 61);
    m.guns = gun::spawn(0x30);
    let mut rows = std::collections::BTreeMap::new();
    while m.tick <= 84 {
        let t = m.tick;
        m.step(&arena, &[Input::default(); 2], &world);
        let f = &m.players[0];
        rows.insert(
            t,
            (f.body.pos, f.body.y_vel, f.anim, f.gun.map(|g| g.state)),
        );
    }
    assert_eq!(
        (rows[&61].1, rows[&61].2, rows[&61].3),
        (44.0, 0x32, Some(gun::LAUNCH))
    );
    assert!((rows[&62].0[1] - 565.12).abs() < 0.01, "{:?}", rows[&62]);
    assert!(
        (rows[&72].0[1] - 687.0).abs() < 0.5 && rows[&72].1 == 0.0,
        "{:?}",
        rows[&72]
    );
    assert_eq!(
        (rows[&79].2, rows[&80].2, rows[&82].3),
        (0x32, 0x38, Some(gun::LAUNCH))
    );
    assert_eq!(
        (rows[&83].2, rows[&83].3, rows[&84].2),
        (0x38, Some(gun::IDLE), 0x1A1)
    );
    assert!(
        near(rows[&83].0, [166.91, 616.14, 6000.88], 0.01),
        "{:?}",
        rows[&83]
    );
    // The leap off (V m2: the barrels 60° up from read 1560, A at 1601-1602): facing 0xB000 at z > 0,
    // 15 along 0xC200 plus the gun's push on the press's frame, then the brake 0.875 a frame to the
    // landing. Gun A turned 90° (V l1, z < 0): facing 0x3000, the same jump along 0x4200.
    let leap = |m: &mut Match, yaw: f32, pitch: f32, a: u32| {
        let mut rows = std::collections::BTreeMap::new();
        while m.tick <= a + 14 {
            let t = m.tick;
            if m.players[0].seated() {
                (m.players[0].body.scope_yaw, m.players[0].scope_pitch) = (Some(yaw), pitch);
            }
            let buttons = if t == a || t == a + 1 { BUTTON_A } else { 0 };
            let input = Input {
                buttons,
                ..Input::default()
            };
            m.step(&arena, &[input, Input::default()], &world);
            let b = &m.players[0].body;
            rows.insert(t, (b.pos, b.xz_speed, b.facing, b.airborne));
        }
        rows
    };
    let rows = leap(&mut m, 87.1875, -60.0, 1601);
    assert!(near(
        rows[&1600].0,
        [166.678955, 611.733215, 6005.60107],
        0.02
    ));
    for (t, at, speed) in [
        (1601, [167.34343, 626.385742, 6022.86865], 30.0),
        (1602, [168.057983, 638.285706, 6037.41357], 29.125),
        (1608, [171.894455, 647.555786, 6115.50635], 23.875),
        (1614, [174.95813, 545.22583, 6177.86914], 18.625),
    ] {
        assert!(
            near(rows[&t].0, at, 0.05) && rows[&t].1 == speed,
            "{t} {:?}",
            rows[&t]
        );
        assert_eq!(rows[&t].2, 0xB000);
    }
    assert!(near(rows[&1615].0, [175.3936, 543.949219, 6186.7334], 0.05) && !rows[&1615].3);
    let players = vec![
        Fighter::new(&place(gun::places(0x30)[0].pos, 0), 0, 150),
        Fighter::new(&place([-436.0, -914.41, -5917.0], 17152), 1, 150),
    ];
    let mut m = Match::new(&arena, players, Rng::default(), 150);
    m.guns = gun::spawn(0x30);
    m.players[0].gun = Some(gun::Mount {
        state: gun::IDLE,
        ..gun::Mount::new(0, m.guns[0].place.pos)
    });
    let rows = leap(&mut m, 354.375, 0.0, 201);
    assert!(near(
        rows[&200].0,
        [-384.657104, -315.227295, -3627.17114],
        0.05
    ));
    assert!(near(
        rows[&201].0,
        [-392.251312, -298.095581, -3644.44141],
        0.05
    ));
    assert!(near(
        rows[&208].0,
        [-397.918335, -276.310883, -3737.677],
        0.05
    ));
    assert_eq!((rows[&201].2, rows[&208].1), (0x3000, 23.875));
    assert!(near(rows[&215].0, [-401.41745, -388.0, -3808.90356], 0.05) && !rows[&215].3);

    let world = self::world(&rom, 0x34);
    let players = vec![
        Fighter::new(&place([2465.0, 645.0, 500.0], 65280), 0, 41),
        Fighter {
            immune: 0,
            ..Fighter::new(&place([2216.0, 645.0, 749.0], 16640), 1, 41)
        },
    ];
    let mut m = Match::new(&arena, players, Rng::default(), 41);
    m.guns = gun::spawn(0x34);
    let (mut hits, mut knock, mut later) = (Vec::new(), None, Vec::new());
    while m.tick <= 170 {
        let t = m.tick;
        if m.players[0].seated() {
            // Read 100's scope: the stick held up at 75-78 pitched it 12° down (gun+0x2E4 = 120).
            (m.players[0].body.scope_yaw, m.players[0].scope_pitch) = (Some(225.0), 12.0);
        }
        let z = if (101..=160).contains(&t) {
            BUTTON_Z
        } else {
            0
        };
        let inputs = [
            Input {
                buttons: z,
                ..Input::default()
            },
            Input::default(),
        ];
        for e in m.step(&arena, &inputs, &world) {
            if let Event::Hit {
                victim: 1, damage, ..
            } = e
            {
                hits.push((t, damage, m.players[1].anim));
                knock = Some(m.players[1].body.move_angle);
            }
        }
        if t >= 110 {
            later.extend(m.bullets.iter().filter(|b| b.0 == t).map(|b| b.1.dir));
        }
        if t == 65 || t == 66 {
            assert_eq!(m.players[0].seated(), t == 66);
        }
    }
    // The seat tilted with the 12° pitch (V e1 99; firing, the clip's recoil moves it 0.25 more).
    let seat = [2471.8904, 717.911, 493.1097];
    assert!(
        near(m.players[0].body.pos, seat, 0.01),
        "{:?}",
        m.players[0].body.pos
    );
    assert!(m.guns[0].shots >= 8, "{}", m.guns[0].shots);
    assert_eq!((hits, m.players[1].health), (vec![(101, 2, 0x19C)], 4));
    // The first shot heads for where the scope's ray meets player 2's body: knocked at 210.4° (V
    // 0x9595), not along the barrel (~220°). Later ones head for the floor where the ray meets it:
    // from V's eye (k1, read 115: the fire clip lifts it 5 units) the point V's bullets cross,
    // (2102.5, 675.39, 862.5); from the gun at rest, V's two barrels' directions to 0.011.
    let knock = knock.unwrap();
    assert!(knock.abs_diff(0x9595) < 40, "{knock:#x}");
    let dirs = [
        [-0.748279, -0.196937, 0.633478],
        [-0.632179, -0.208411, 0.746267],
    ];
    let off = |v: [f32; 3], d: [f32; 3]| (0..3).map(|j| (v[j] - d[j]).abs()).fold(0.0, f32::max);
    assert!(!later.is_empty());
    for d in &later {
        assert!(off(dirs[0], *d).min(off(dirs[1], *d)) < 0.011, "{d:?}");
    }
    let (at, eye) = (gun::places(0x34)[0].pos, [2333.61426, 744.83844, 631.38562]);
    let ray = [-0.691665113, -0.207842022, 0.691665471];
    for b in [0, 1] {
        let p = gun::sight(at, 0xA000, 12.0, b, eye, ray);
        let hit = conker_rules::projectile::raycast(&world, p, ray, 20_000.0);
        let to: [f32; 3] = core::array::from_fn(|j| p[j] + ray[j] * hit);
        assert!(near(to, [2102.5, 675.39, 862.5], 0.1), "{b} {to:?}");
    }
    let p = m.players[1].body.pos;
    let d = ((p[0] - 2216.0).powi(2) + (p[2] - 749.0).powi(2)).sqrt();
    assert!((110.0..170.0).contains(&d), "{d}");
    eprintln!(
        "e1: {} shots (V 13), P2 knocked {d:.1} at {knock:#x} (V 140.7 at 0x9595)",
        m.guns[0].shots
    );
}

/// War: Beach's script 0xB9 on the match (`beach.md`): two human villagers and a human Tedi, idle.
/// p1 stands on the plunger (the detonation: the villagers' point per Tedi, the Tedi killed with no
/// credit), then steps into tunnel 0xD (the freeze of 150 frames, its point, the next villager
/// record of the shared cycle [0, 1, 2, 11]); p0's 90 s deadline strikes it 2748 frames after the
/// script's first (V x1, bots1: read 2772 from read 24), and its death 40 frames on is the Tediz'
/// point. No native goes unanswered.
#[test]
fn beach_script_runs_the_match() {
    use conker_bots::{Assets, Bots};
    use conker_rules::gas::Effect;
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, Settings, Spawn, beach};
    use conker_sim::movement::{Input, Side};
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x2B).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom, 0x2B);
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(&rom, 0x2B).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let at = |record: usize| Placement {
        index: record,
        pos: spawns[record].pos.map(f32::from),
        facing: 0,
        camera: [0.0; 3],
    };
    let villager = |record, class| Fighter {
        side: Side::Squirrels,
        start: Some(beach::ITEM),
        class,
        ..Fighter::new(&at(record), 0, 0)
    };
    let players = vec![
        villager(1, 0x1E),
        villager(0, 0x14),
        Fighter {
            class: beach::TEDI_CLASS,
            ..Fighter::new(&at(3), 1, 0)
        },
    ];
    let arena = Arena {
        spawns,
        pickups: Vec::new(),
        ..Default::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(Settings {
        beach: true,
        ..Settings::default()
    });
    let mut bots = Bots::new(assets, &[0, 1, 2], &[], 2, Rng::default()).unwrap();
    let mut inputs = [Input::default(); 3];
    let (mut detonation, mut escape, mut strike, mut records) = (None, None, None, [0u8; 2]);
    let mut point = None;
    while m.tick < 2800 {
        let t = m.tick;
        match t {
            100 => m.players[1].body.pos = [-1470.0, beach::TOP_Y, 2350.0],
            // Node 4, inside trigger 0xD under the west bunker.
            400 => m.players[1].body.pos = [-1606.0, -1411.0, -3944.0],
            _ => {}
        }
        let effects = bots.tick(&m, &arena, &world, &mut inputs);
        for e in &effects {
            match *e {
                Effect::Start(p, r) if t == 0 && p < 2 => records[p] = r,
                Effect::Hit { slot: 2, .. } => detonation = detonation.or(Some(t)),
                Effect::Respawn(1) => escape = escape.or(Some(t)),
                Effect::Strike(p) => strike = strike.or(Some((t, p))),
                _ => {}
            }
        }
        m.apply(&effects);
        bots.scopes(&mut m);
        let before = m.colors.scores[1];
        let ev = m.step(&arena, &inputs, &world);
        if m.colors.scores[1] > before {
            point = point.or(Some(t));
        }
        bots.observe(&ev, &m);
    }
    let cycle = [0u8, 1, 2, 11];
    let k = cycle.iter().position(|&r| r == records[0]).unwrap();
    assert_eq!(
        records[1],
        cycle[(k + 1) % 4],
        "the villagers share the cycle"
    );
    let detonation = detonation.expect("the detonation");
    assert!((110..130).contains(&detonation), "{detonation}");
    let tedi = m.players[2];
    assert_eq!(
        (tedi.deaths, m.players[1].kills, m.players[1].suicides),
        (1, 0, 0)
    );
    // The freeze: seen inside at 400, out 150 frames on (V esc/end1: 62 → 212) on the next record.
    assert_eq!(escape, Some(550));
    assert_eq!(
        m.players[1].last_spawn,
        Some(usize::from(cycle[(k + 2) % 4]))
    );
    assert_eq!(strike, Some((48 + 2700, 0)));
    assert_eq!(point, Some(48 + 2700 + 40));
    assert_eq!(m.colors.scores, [2, 1]);
    assert!(bots.unknown.is_empty(), "{:?}", bots.unknown);
    assert_eq!(bots.faults, 0);
}

/// War: Beach through a roster change: villager 1 leaves at 1000 and a new human villager takes its
/// slot at 1100. The script goes on: villager 0's 90 s deadline runs from the match start (its strike
/// at 48 + 2700, as in `beach_script_runs_the_match`; a restarted script would strike at 1100 + 2748),
/// the joiner's from its own appear (G+0x1574 = 5400 48 frames in), and the leaver's is gone.
#[test]
fn beach_keeps_its_deadlines_through_a_join_and_a_leave() {
    use conker_bots::brain::g;
    use conker_bots::{Assets, Bots};
    use conker_rules::gas::Effect;
    use conker_rules::{Arena, Fighter, Match, Placement, Rng, Settings, Spawn, beach};
    use conker_sim::movement::{Input, Side};
    let Some(rom) = rom() else { return };
    let data: Vec<Vec<u8>> = (Assets::paths(0x2B).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let world = world(&rom, 0x2B);
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(&rom, 0x2B).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let at = |record: usize| Placement {
        index: record,
        pos: spawns[record].pos.map(f32::from),
        facing: 0,
        camera: [0.0; 3],
    };
    let villager = |record, class| Fighter {
        side: Side::Squirrels,
        start: Some(beach::ITEM),
        class,
        ..Fighter::new(&at(record), 0, 0)
    };
    let players = vec![
        villager(1, 0x1E),
        villager(0, 0x14),
        Fighter {
            class: beach::TEDI_CLASS,
            ..Fighter::new(&at(3), 1, 0)
        },
    ];
    let arena = Arena {
        spawns: spawns.clone(),
        pickups: Vec::new(),
        ..Default::default()
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(Settings {
        beach: true,
        ..Settings::default()
    });
    let mut bots = Bots::new(assets, &[0, 1, 2], &[], 2, Rng::default()).unwrap();
    let mut inputs = [Input::default(); 3];
    let (mut strikes, mut deadline) = (Vec::new(), None);
    while m.tick < 2800 {
        let t = m.tick;
        match t {
            1000 => {
                m.remove_player(1);
                bots.leave(1);
            }
            1100 => {
                assert_eq!(m.join(villager(2, 0x1D)), 1);
                bots.join(1, false);
            }
            1200 => deadline = Some(bots.globals().get(g::TIMER3, 1)),
            _ => {}
        }
        let effects = bots.tick(&m, &arena, &world, &mut inputs);
        for e in &effects {
            if let Effect::Strike(p) = *e {
                strikes.push((t, p));
            }
        }
        m.apply(&effects);
        let ev = m.step(&arena, &inputs, &world);
        bots.observe(&ev, &m);
    }
    assert_eq!(strikes, [(48 + 2700, 0)]);
    // Set 48 frames in (1148, as the others' at 48), 2 less a frame since.
    assert_eq!(deadline, Some(5400 - 2 * (1200 - 1148 - 1)));
    assert!(bots.unknown.is_empty(), "{:?}", bots.unknown);
    assert_eq!(bots.faults, 0);
}

/// A Deathmatch as `examples/bot_match.rs` plays it: an idle human in slot 0 and `bots` bots on
/// `scene` with the scene's own AI script, from `seed`; `each` sees the match after every frame,
/// with the slots a lift moved in it.
fn deathmatch(
    rom: &extract_core::Rom,
    scene: usize,
    bots: usize,
    seed: u64,
    frames: u32,
    each: impl FnMut(&conker_rules::Match, &[usize]),
) -> conker_bots::Bots {
    play(rom, scene, bots, seed, frames, None, each)
}

/// [`deathmatch`], or a War mode's `settings`, as the server has them: Heist on 0x1A (slot i on
/// team i & 3, its team's start records and respawn mask), Raptor on 0x3F (slots 4 and 5 of each
/// 8 the raptors, team 1 on records 0-7, the others the cavemen with the club on 8-15; the mode's
/// pickups); the script's effects applied.
fn play(
    rom: &extract_core::Rom,
    scene: usize,
    bots: usize,
    seed: u64,
    frames: u32,
    settings: Option<conker_rules::Settings>,
    mut each: impl FnMut(&conker_rules::Match, &[usize]),
) -> conker_bots::Bots {
    use conker_bots::lift::Lifts;
    use conker_bots::{Assets, Bots, DEFAULT_SKILL};
    use conker_rules::{Arena, Fighter, Match, Rng, Spawn, spawn, weapon};
    use conker_sim::movement::{Input, Side};
    let data: Vec<Vec<u8>> = (Assets::paths(scene).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let mut world = world(rom, scene);
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(rom, scene).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let placements = extract_scene::scene_placements(rom, scene).unwrap();
    let heist = settings.is_some_and(|s| s.heist);
    let raptor = settings.is_some_and(|s| s.raptor);
    let (picks, spots) = match raptor {
        true => (&weapon::PICKUPS_3F_RAPTOR[..], &[][..]),
        false => weapon::pickups(scene as u16).unwrap(),
    };
    let pickups = (picks.iter().enumerate())
        .filter(|(_, p)| p.gives != weapon::Gives::Mask)
        .map(|(k, p)| {
            let at = spots.get(k).copied();
            let at = at.unwrap_or(placements[usize::from(p.placement)].position);
            (*p, at.map(f32::from))
        })
        .collect();
    // 0x1A's roaming pickups, and everyone the Wise Guy with the bat (as the server has them).
    let vault = scene == 0x1A;
    let arena = Arena {
        spawns,
        pickups,
        roaming: if vault {
            weapon::Roaming::vault()
        } else {
            Default::default()
        },
        respawn: vault.then_some(weapon::VAULT_RESPAWN),
        lob: matches!(scene, 0x1A | 0x3F),
        ..Default::default()
    };
    let rng = Rng { state: seed };
    let starts = spawn::starts(scene as u16).unwrap();
    let mut r = rng;
    spawn::place(&arena.spawns, 0, &mut r);
    let mut members = [0; 2];
    let fighters = (0..=bots)
        .map(|i| {
            if raptor {
                use conker_rules::raptor;
                let team = u8::from(matches!(i & 7, 4 | 5));
                let k = members[usize::from(team)];
                members[usize::from(team)] += 1;
                let at = usize::from(starts.sides[usize::from(team)][k % 8]);
                return Fighter {
                    side: [Side::Cavemen, Side::Tediz][usize::from(team)],
                    start: Some(weapon::CLUB.id),
                    class: [raptor::CAVEMAN, raptor::CLASS][usize::from(team)],
                    spawn_mask: starts.side_masks[usize::from(team)],
                    ..Fighter::new(&spawn::place(&arena.spawns, at, &mut r), team, 23)
                };
            }
            let records = conker_rules::heist::RECORDS[i & 3];
            let (at, team, mask) = match heist {
                true => (
                    usize::from(records[i / 4]),
                    (i & 3) as u8,
                    records.iter().fold(0, |m, &r| m | 1 << r),
                ),
                false => (usize::from(starts.deathmatch[i % 16]), i as u8, !0),
            };
            Fighter {
                start: vault.then_some(weapon::BAT.id),
                class: if vault { 1 } else { 3 },
                spawn_mask: mask,
                ..Fighter::new(&spawn::place(&arena.spawns, at, &mut r), team, 23)
            }
        })
        .collect();
    let mut m = Match::new(&arena, fighters, r, 23);
    if let Some(s) = settings {
        m = m.with(s);
    }
    // 0x34's plates and traps, as the server has them.
    m.traps = conker_rules::traps::Traps::new(scene as u16, assets.triggers);
    let slots: Vec<usize> = (1..=bots).collect();
    let mut ai = Bots::new(assets, &[0], &slots, DEFAULT_SKILL, rng).unwrap();
    let mut lifts = Lifts::new(assets.triggers, scene as u8);
    for _ in 0..frames {
        let mut inputs = vec![Input::default(); bots + 1];
        let lifted: Vec<usize> = (lifts.ride(&mut m, &arena).into_iter())
            .map(|(slot, _)| slot)
            .collect();
        for &slot in &lifted {
            ai.lifted(slot);
        }
        if !m.traps.list.is_empty() {
            conker_rules::traps::seat(&mut world, m.traps.offsets());
        }
        let fx = ai.tick(&m, &arena, &world, &mut inputs);
        let mut ev = m.apply(&fx);
        ai.scopes(&mut m);
        ev.extend(m.step(&arena, &inputs, &world));
        ai.observe(&ev, &m);
        each(&m, &lifted);
    }
    ai
}

/// 0x30's lift pair (4 ↔ 7) on its own script 0xBB: no bot rides more than 6 times in a row less
/// than 60 frames apart (`bots-fidelity.md` Q1: run on 0xBA, whose unarmed-pickup list holds node
/// 29 inside lift 4's box, bots rode back and forth all match; on 0xBB a ride lands on either side
/// of the graph's split and the bot leaves, after 2-6 bounces at worst, as the oracle's bounce too).
#[test]
fn lift_pair_riders_leave() {
    let Some(rom) = rom() else { return };
    let mut rides: Vec<Vec<u32>> = vec![Vec::new(); 16];
    let bots = deathmatch(&rom, 0x30, 15, 3, 3000, |m, lifted| {
        for &slot in lifted {
            rides[slot].push(m.tick);
        }
    });
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    assert!(rides.iter().any(|r| !r.is_empty()), "no one rode");
    for (slot, r) in rides.iter().enumerate() {
        let (mut run, mut most) = (1, 1);
        for w in r.windows(2) {
            run = if w[1] - w[0] < 60 { run + 1 } else { 1 };
            most = most.max(run);
        }
        assert!(
            most <= 6,
            "slot {slot}: {most} quick rides in a row at {r:?}"
        );
    }
}

/// 0x34's corridor by nodes 60-62 (x > 2800, −4150 < z < −2550, y < 200) on its own script 0xBD:
/// no bot stays there 300 frames jumping (`bots-fidelity.md` Q4: on 0xBA, whose unarmed-pickup
/// list names the bare corridor nodes 31 and 61, bots walked in, cooled, wandered and jumped there
/// over and over; the oracle crosses it in at most 74 reads, with no state 55).
#[test]
fn no_jump_loop_in_the_0x34_corridor() {
    use conker_sim::movement::state::RUN_JUMP;
    let Some(rom) = rom() else { return };
    let mut stay = [(0u32, false); 4];
    let bots = deathmatch(&rom, 0x34, 3, 1, 4500, |m, _| {
        for (slot, f) in m.players.iter().enumerate().skip(1) {
            let [x, y, z] = f.body.pos;
            let (frames, jumped) = &mut stay[slot];
            if x > 2800.0 && -4150.0 < z && z < -2550.0 && y < 200.0 {
                *frames += 1;
                *jumped |= f.body.state == RUN_JUMP;
                assert!(*frames <= 300 || !*jumped, "slot {slot} at {}", m.tick);
            } else {
                (*frames, *jumped) = (0, false);
            }
        }
    });
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
}

/// 0x34 with 15 bots for the oracle's 60 s (`s4_h1b15`, 1,737 frames): the bots press the plates
/// on their way and the traps kill some (script 0xBD neither seeks nor avoids them). The oracle's:
/// 14 presses, no one in a trap while on (conker-rules' `the_plates_hold_the_0x34_floors`); seeds
/// 1-4 here: 3-10 presses, 3-7 kills, the bots 2-3 times as many frames in the traps' zones (I: the
/// bots' paths, not the traps).
#[test]
fn bots_set_off_the_0x34_traps() {
    let Some(rom) = rom() else { return };
    let (mut presses, mut kills, mut alive) = (0, 0, [true; 16]);
    let mut was = [0u8; 4];
    let bots = deathmatch(&rom, 0x34, 15, 1, 1737, |m, _| {
        let state = m.traps.plates.map(|p| p.state);
        presses += (0..4).filter(|&k| was[k] == 2 && state[k] == 3).count();
        was = state;
        for (p, f) in m.players.iter().enumerate() {
            let b = &f.body;
            let inside = (0..m.traps.list.len())
                .any(|k| m.traps.left[k] > 0 && m.traps.list[k].contains(b.pos, b.floor_y));
            kills += u32::from(alive[p] && f.health == 0 && inside);
            alive[p] = f.health > 0;
        }
    });
    eprintln!("0x34: {presses} presses, {kills} trap kills");
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    assert!(presses > 0 && kills > 0, "{presses} {kills}");
}

/// 0x2D, seed 5: no bot stands (within 20 in x and z) over 3,000 frames, the oracle's longest
/// being 2,975 (`bot_long3`). Its bullets once flew level along the facing, so p1, in AI state 8
/// on the idle human 3,634 away and 415 up a ledge, stood and fired for the rest of the match
/// (1,828 shots, 5 hits); they now follow its aim ray (`Match::aims`) and kill it.
#[test]
fn no_endless_stand_below_a_ledge_on_0x2d() {
    let Some(rom) = rom() else { return };
    let mut stay = [([0f32; 3], 0u32); 4];
    let bots = deathmatch(&rom, 0x2D, 3, 5, 4500, |m, _| {
        for (slot, f) in m.players.iter().enumerate().skip(1) {
            let (at, since) = &mut stay[slot];
            let p = f.body.pos;
            if (p[0] - at[0]).abs() >= 20.0
                || (p[2] - at[2]).abs() >= 20.0
                || (p[1] - at[1]).abs() >= 60.0
            {
                (*at, *since) = (p, m.tick);
            }
            assert!(
                m.tick - *since <= 3000,
                "slot {slot} stands at {at:?} from {since}"
            );
        }
    });
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
}

/// 0x1A's Deathmatch (scenario 6) on Heist's script 0xB7, its FFA branch, with 3 bots for dm6's
/// 119 s (3,572 frames). Every bot starts and respawns with the bat, moves, fights and goes for the
/// pickups; the tommy gun (k0) is taken and fired. The script finds pickup k's node as
/// G[0x14F4][G[0x10F0][k]] (C, 0x2240): the node under its spot. The oracle's 3-bot runs (dm6,
/// dm3w, dm3v; 1 idle human): 6-10 pickups, 7-17 hits (the victims' health drops), 4-5 kills,
/// 4-5 deaths, the bat never swung (V: 15 swings only in a 7-bot run, dm7); here, seed 1: 15
/// pickups (10 of the roaming k0-k3), 14 tommy-gun shots, 4 kills.
#[test]
fn vault_deathmatch_bots_fight() {
    use conker_bots::brain::g;
    let Some(rom) = rom() else { return };
    let mut from = [None::<[f32; 3]>; 4];
    let (mut moved, mut pickups, mut tommy) = ([0f32; 4], 0, 0);
    let (mut shots, mut taken, mut back) = ([0u16; 4], [None; 8], [None::<u32>; 4]);
    let (mut spots, mut last, mut stats) = (Vec::new(), Vec::new(), Vec::new());
    let bots = deathmatch(&rom, 0x1A, 3, 1, 3572, |m, _| {
        for (k, p) in m.pickups.iter().enumerate() {
            pickups += u32::from(k < 4 && p.taken.is_some() && p.taken != taken[k]);
            taken[k] = p.taken;
        }
        for (i, f) in m.players.iter().enumerate() {
            let p = f.body.pos;
            match from[i] {
                Some(q) => moved[i] += (p[0] - q[0]).hypot(p[2] - q[2]).min(100.0),
                None => assert_eq!(f.weapon, Some(0x19), "{i} at the start"),
            }
            from[i] = Some(p);
            tommy += f.shots.saturating_sub(shots[i]) * u16::from(f.weapon == Some(0x38));
            shots[i] = f.shots;
            // The bat again on the frames after each respawn.
            match (back[i], f.health) {
                (None, 0) => back[i] = Some(0),
                (Some(n), h) if h > 0 => {
                    assert!(n < 2 || f.weapon == Some(0x19), "{i} at {}", m.tick);
                    back[i] = (n < 2).then_some(n + 1);
                }
                _ => {}
            }
        }
        // The spots the bots' last frame saw (`Bots::tick` mirrors before the step).
        last = std::mem::replace(&mut spots, m.pickups.iter().map(|p| p.spot).collect());
        stats = (m.players.iter())
            .map(|f| (f.shots, f.hits, f.kills, f.deaths))
            .collect();
    });
    eprintln!(
        "0x1A: {pickups} roaming pickups, {tommy} tommy-gun shots; moved {moved:?}; (shots, \
         hits, kills, deaths) {stats:?}"
    );
    assert!(
        bots.unknown.is_empty() && bots.faults == 0,
        "{:?}",
        bots.unknown
    );
    assert!(moved[1..].iter().all(|&d| d > 10_000.0), "{moved:?}");
    assert!(shots[1..].iter().all(|&s| s > 0), "{shots:?}");
    assert!(pickups > 0 && tommy > 0, "{pickups} {tommy}");
    // The script's spot nodes (`G[0x14F4]`, 0x381A): 0-7 where k0-k3 roam, then k4-k7's 8-11.
    const NODES: [i32; 12] = [
        0x2E, 0x33, 0x23, 0x38, 5, 0xF, 0x16, 0x1B, 0x79, 0x7A, 0x7B, 0x7C,
    ];
    let gl = bots.globals();
    assert_eq!(last.len(), 8);
    for (k, spot) in last.iter().enumerate() {
        let spot = spot.map_or(k + 4, usize::from);
        let node = gl.get(g::NODE_OF, gl.get(g::SPOT, k) as usize);
        assert_eq!(node, NODES[spot], "k{k}");
        assert_eq!(gl.get(g::HOLDER, k), -1, "k{k}");
    }
}

/// War: Heist on 0x1A with the real 0xB7 (7 bots, 1 idle human, two teams of 2): the script
/// creates the bag 210-299 frames in, bots fetch it and carry it at 25.2 at most, the voices and
/// the scores come as effects and events, and no native is unknown (V bots1: the bag at 257, ~30
/// pickups and 2 deliveries in 300 s).
#[test]
fn heist_bots_fetch_and_deliver() {
    use conker_rules::heist::{CASH, State};
    let Some(rom) = rom() else { return };
    let (mut created, mut pickups, mut carrier) = (None, 0, None);
    let (mut top, mut walked, mut scores) = (0f32, 0, [0u16; 4]);
    let heist = conker_rules::Settings {
        heist: true,
        score: conker_rules::heist::SCORE,
        ..Default::default()
    };
    let bots = play(&rom, 0x1A, 7, 1, 9000, Some(heist), |m, _| {
        let c = m.heist.cash;
        if c.shown && created.is_none() {
            created = Some(m.tick);
        }
        if c.carrier.is_some() && c.carrier != carrier {
            pickups += 1;
        }
        carrier = c.carrier;
        // Past the grab (26 frames), the carry's own speed (a hit's knock-back drops the bag next).
        if let Some(f) = c.carrier.map(|i| &m.players[i]) {
            assert_eq!(f.flag, Some(CASH));
            let hit = matches!(f.action, conker_rules::game::Action::Hurt { .. });
            if m.tick > c.since + 30 && !hit {
                top = top.max(f.body.xz_speed);
            }
        }
        walked += u32::from(c.state == State::Walk);
        scores = m.heist.scores;
    });
    eprintln!(
        "heist: bag at {created:?}, {pickups} pickups, carried at {top} at most, {walked} frames \
         walking, scores {scores:?}; unknown {:?}",
        bots.unknown
    );
    let t = created.expect("the bag is created") - 23;
    assert!((210..300).contains(&t), "{t}");
    assert!(pickups >= 5, "{pickups}");
    assert!(top <= 25.2 + 1e-3, "{top}");
    assert!(walked > 0);
    assert!(bots.unknown.is_empty(), "{:?}", bots.unknown);
    assert_eq!(bots.faults, 0);
}

/// War: Raptor on 0x3F, script 0xBE's team branch: 1 idle human caveman, 5 caveman bots, 2
/// raptor bots (oracle b1's roster: b1 raptors 20-14 at 8510; b1r 16-16 at 9688, the raptors' 71
/// bites and the 103 hits they took, all for 1). Seeds 1-3, to 20 or 9000 frames: the kills a 1000
/// frames each way are the oracle's within its two runs' spread (b1 2.35 raptors' and 1.65
/// cavemen's, b1r 1.63 and 1.63; before, with the bolts' head kills, 1.33 and 2.4), the raptor bots
/// bite about as often (b1r 7.2 a 1000), and no hit on a raptor takes more than 1.
#[test]
fn raptor_bots_play() {
    use conker_rules::raptor::CLASS;
    let Some(rom) = rom() else { return };
    let raptor = conker_rules::Settings {
        raptor: true,
        score: conker_rules::raptor::SCORE,
        ..Default::default()
    };
    let (mut frames, mut kills, mut bites) = (0u32, [0u32; 2], [0u32; 2]);
    for seed in 1..=3 {
        let mut scores = [0u16; 2];
        let (mut drops, mut carried, mut speed, mut tick) = (0u32, 0u32, 0f32, 0u32);
        let mut health: Vec<u8> = Vec::new();
        let bots = play(&rom, 0x3F, 7, seed, 9000, Some(raptor), |m, _| {
            use conker_rules::colors::Pose;
            use conker_rules::game::Action;
            scores = m.colors.scores;
            tick = m.tick;
            for (i, f) in m.players.iter().enumerate() {
                match f.action {
                    Action::Pose(Pose::Bite) if f.since == m.tick - 1 => bites[0] += 1,
                    Action::Pose(Pose::RunBite) if f.since == m.tick - 1 => bites[1] += 1,
                    Action::Pose(Pose::Drop) if f.since == m.tick - 1 => drops += 1,
                    _ => {}
                }
                if f.class == CLASS {
                    speed = speed.max(f.body.xz_speed);
                    let was = health.get(i).copied().unwrap_or(f.health);
                    assert!(
                        was <= f.health + 1 || f.health > 0 && was == 0,
                        "{seed}: {i} {was} → {}",
                        f.health
                    );
                }
            }
            health = m.players.iter().map(|f| f.health).collect();
            carried += m.raptor.eggs.iter().filter(|e| e.carrier.is_some()).count() as u32;
        });
        assert!(bots.unknown.is_empty(), "{:?}", bots.unknown);
        assert_eq!(bots.faults, 0);
        eprintln!(
            "raptor {seed}: scores {scores:?} at {tick}, bites {bites:?}, drops {drops}, egg frames {carried}, \
             raptor top speed {speed}"
        );
        frames += tick - 23;
        kills[0] += u32::from(scores[0]);
        kills[1] += u32::from(scores[1]);
    }
    let rate = |n: u32| n as f32 * 1000.0 / frames as f32;
    let (cavemen, raptors, bit) = (rate(kills[0]), rate(kills[1]), rate(bites[0] + bites[1]));
    eprintln!(
        "raptor: a 1000 frames, cavemen's kills {cavemen:.2}, raptors' {raptors:.2}, bites {bit:.2}"
    );
    assert!((1.4..=2.4).contains(&raptors), "{raptors}");
    assert!((1.4..=2.0).contains(&cavemen), "{cavemen}");
    assert!((6.0..=9.0).contains(&bit), "{bit}");
}

/// Tank on 0x24 as the server has it (`humans` then `bots`, slot i on record i, from slot 4 on
/// 0x10 + i − 4; the power-ups roaming tank.md §3's 6 spots, back 150 frames after a take).
fn tank_match(
    rom: &extract_core::Rom,
    humans: &[usize],
    bots: &[usize],
    seed: u64,
) -> (
    conker_rules::Match,
    conker_rules::Arena,
    conker_sim::collision::CollisionWorld,
    conker_bots::Bots,
) {
    use conker_bots::{Assets, Bots, DEFAULT_SKILL};
    use conker_rules::weapon::{Gives, Pickup, Roaming};
    use conker_rules::{Arena, Fighter, Match, Rng, Spawn, spawn};
    let scene = 0x24;
    let data: Vec<Vec<u8>> = (Assets::paths(scene).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(rom, scene).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let spots = [
        [2504.0, -108.0, 2585.0],
        [-2512.0, -58.0, -2450.0],
        [-3920.0, 100.0, 942.0],
        [3736.0, 78.0, -1061.0],
        [-3593.0, -1034.0, -5799.0],
        [3455.0, -1047.0, 5932.0],
    ]
    .map(|[x, y, z]: [f32; 3]| [x, y + 60.0, z]);
    let powers = [(16, 0x80), (8, 0x01), (10, 0x10), (11, 0x02)];
    let arena = Arena {
        spawns,
        pickups: (powers.iter().enumerate())
            .map(|(k, &(placement, bit))| {
                let gives = Gives::Power(bit);
                (Pickup { placement, gives }, spots[k])
            })
            .collect(),
        roaming: Roaming {
            count: 4,
            spots: spots.to_vec(),
        },
        respawn: Some(150),
        ..Default::default()
    };
    // Seeded as the server (`GAME_SEED ^ seed`): a tiny state's first draws are near 0.
    let mut r = Rng::new(Rng::GAME_SEED ^ seed);
    let fighters = (0..humans.len() + bots.len())
        .map(|i| {
            let record = if i < 4 { i } else { 0x10 + i - 4 };
            let at = spawn::place(&arena.spawns, record, &mut r);
            Fighter {
                bot: bots.contains(&i),
                ..Fighter::new(&at, i as u8, 23)
            }
        })
        .collect();
    let m = Match::new(&arena, fighters, r, 23).with(conker_rules::Settings {
        tank: true,
        ..Default::default()
    });
    let ai = Bots::new(
        assets,
        humans,
        bots,
        DEFAULT_SKILL,
        Rng::new(Rng::GAME_SEED ^ seed),
    )
    .unwrap();
    (m, arena, world(rom, scene), ai)
}

/// One Tank frame as the server runs it: the doors' collision, the AI, its effects, the step.
fn tank_frame(
    m: &mut conker_rules::Match,
    arena: &conker_rules::Arena,
    world: &mut conker_sim::collision::CollisionWorld,
    ai: &mut conker_bots::Bots,
    inputs: &mut [conker_sim::movement::Input],
) -> Vec<conker_rules::game::Event> {
    conker_rules::tank::seat(world, m.gas.lifts());
    let fx = ai.tick(m, arena, world, inputs);
    let mut ev = m.apply(&fx);
    ai.scopes(m);
    ev.extend(m.step(arena, inputs, world));
    ai.observe(&ev, m);
    ev
}

/// Script 0xB8's bomb against oracle c1 (two humans): the bomb shows 300-450 frames in; touched,
/// it rides its toucher; carried into bunker E it is planted: the doors shut 231 frames on (c1
/// 123 → 354), the gas haze 271 on, the blast 300 on (423) kills the tank outside and gives its
/// kill to the bomber, the doors open again 389-390 on (512) and the bomb is back 300-450 after
/// the plant (519). No native unknown.
#[test]
fn tank_bomb_as_c1() {
    use conker_rules::tank::{BUNKERS, POST};
    let Some(rom) = rom() else { return };
    let (mut m, arena, mut world, mut ai) = tank_match(&rom, &[0, 1], &[], 1);
    let mut inputs = vec![conker_sim::movement::Input::default(); 2];
    let mut shown = None;
    while shown.is_none() && m.tick < 1000 {
        tank_frame(&mut m, &arena, &mut world, &mut ai, &mut inputs);
        shown = m.tank.bomb.shown.then_some(m.tick - 1);
    }
    let shown = shown.expect("the bomb shows") - 23;
    assert!((300..450).contains(&shown), "{shown}");
    let put = |m: &mut conker_rules::Match, i: usize, pos: [f32; 3]| {
        let b = &mut m.players[i].body;
        (b.pos, b.airborne, b.floor, b.y_vel) = (pos, true, None, 0.0);
    };
    put(&mut m, 0, [POST[0], -600.0, POST[2]]);
    put(&mut m, 1, [-2169.0, -140.0, -2462.0]);
    for _ in 0..3 {
        tank_frame(&mut m, &arena, &mut world, &mut ai, &mut inputs);
    }
    assert!(!m.tank.bomb.shown);
    assert_eq!(m.tank.carrier, Some(0));
    assert!(m.players[0].carrying);
    put(&mut m, 0, BUNKERS[3]);
    let mut plant = None;
    let (mut doors, mut haze, mut blast, mut open, mut back) = (None, None, None, None, None);
    while (back.is_none() || open.is_none()) && m.tick < 3000 {
        tank_frame(&mut m, &arena, &mut world, &mut ai, &mut inputs);
        let t = m.tick - 1;
        if plant.is_none() && m.gas.countdown == 300 {
            plant = Some(t);
        }
        if doors.is_none() && m.gas.props[0].flag == 1 {
            doors = Some(t);
        }
        if haze.is_none() && m.players[1].gassed.is_some() {
            haze = Some(t);
        }
        if blast.is_none() && m.players[1].health == 0 {
            blast = Some(t);
        }
        if blast.is_some() && open.is_none() && m.gas.props[0].flag == 0 {
            open = Some(t);
        }
        if plant.is_some() && back.is_none() && m.tank.bomb.shown {
            back = Some(t);
        }
    }
    let p = plant.expect("planted");
    eprintln!(
        "tank c1: shown {shown}, plant {p}, doors {doors:?}, haze {haze:?}, blast {blast:?}, open {open:?}, back {back:?}; unknown {:?}",
        ai.unknown
    );
    assert_eq!(m.tank.planted, Some(3));
    assert_eq!(doors, Some(p + 231));
    assert_eq!(haze, Some(p + 271));
    assert_eq!(blast, Some(p + 300));
    assert!(
        matches!(open, Some(o) if (p + 389..=p + 390).contains(&o)),
        "{open:?}"
    );
    assert!(
        matches!(back, Some(b) if (p + 300..p + 450).contains(&b)),
        "{back:?}"
    );
    assert_eq!((m.players[0].kills, m.players[0].health), (1, 6));
    assert_eq!(m.players[1].deaths, 1);
    assert!(ai.unknown.is_empty(), "{:?}", ai.unknown);
    assert_eq!(ai.faults, 0);
}

/// One Tank match of b1's roster (7 bots, an idle human) on `seed`, 9000 frames as the oracle's
/// reads 23-9023: `[shots, hits of 1, shell deaths, blast deaths, plants, takes, longest carry]`
/// (the carry still on at the end counts to there). No native is unknown, no script faults.
fn tank_bots_run(rom: &extract_core::Rom, seed: u64) -> [u32; 7] {
    use conker_rules::tank::CLIP_BOMB;
    let (mut m, arena, mut world, mut ai) = tank_match(rom, &[0], &[1, 2, 3, 4, 5, 6, 7], seed);
    let mut inputs = vec![conker_sim::movement::Input::default(); 8];
    let (mut plants, mut taken, mut shells, mut bomb_kills, mut hits) = (0, 0, 0, 0, 0);
    let (mut counting, mut carrier, mut took, mut longest) = (false, None, 0, 0);
    for _ in 0..9000 {
        let before: Vec<u8> = m.players.iter().map(|f| f.health).collect();
        let ev = tank_frame(&mut m, &arena, &mut world, &mut ai, &mut inputs);
        hits += (m.players.iter().zip(&before))
            .filter(|(f, b)| f.health + 1 == **b && f.body.state != CLIP_BOMB)
            .count() as u32;
        if m.gas.countdown > 0 && !counting {
            plants += 1;
        }
        counting = m.gas.countdown > 0;
        if m.tank.carrier.is_some() && m.tank.carrier != carrier {
            taken += 1;
            took = m.tick;
        }
        carrier = m.tank.carrier;
        if carrier.is_some() {
            longest = longest.max(m.tick - took);
        }
        for e in ev {
            if let conker_rules::game::Event::Hit {
                victim,
                lethal: true,
                ..
            } = e
                && before[victim] > 0
            {
                match m.players[victim].body.state {
                    CLIP_BOMB => bomb_kills += 1,
                    _ => shells += 1,
                }
            }
        }
    }
    let shots = m.players.iter().map(|f| u32::from(f.shots)).sum();
    assert!(ai.unknown.is_empty(), "{:?}", ai.unknown);
    assert_eq!(ai.faults, 0);
    [shots, hits, shells, bomb_kills, plants, taken, longest]
}

/// Script 0xB8's bots against the oracle's ten runs to read 9023 (7 bots, an idle human: b1, h6
/// unpoked, h1-h5 and h7-h9 with the RNG poked; G1 `tank-leftovers-scratch`), seeds 1-4 here: they
/// fetch the bomb and plant it, its blasts and their shells kill, no native is unknown. The
/// oracle's: shots 312-419 (350 a run), hits of 1 25.6% of them (19.6-29.5%), shell deaths 5-14
/// (10.5), blast deaths 11-23 (14.8), plants 2-5 (3.8), its longest carry 1197 frames (each ends
/// planted or with its carrier's death; the remake's carriers into bunker E once parked outside
/// its trigger for good). Left: the remake's shell deaths run low for its hits (7.5 over seeds
/// 1-12, the same 88 hits a run as the oracle's 89.5).
#[test]
fn tank_bots_bomb_and_fight() {
    let Some(rom) = rom() else { return };
    let runs: Vec<[u32; 7]> = (1..=4).map(|seed| tank_bots_run(&rom, seed)).collect();
    let sum = |k: usize| runs.iter().map(|r| r[k]).sum::<u32>() as f32;
    let n = runs.len() as f32;
    let (shots, hits) = (sum(0) / n, sum(1) / sum(0));
    let (shells, blasts, plants) = (sum(2) / n, sum(3) / n, sum(4) / n);
    eprintln!(
        "tank bots, seeds 1-4 [shots, hits, shell deaths, blast deaths, plants, takes, longest \
         carry]: {runs:?}; a run: {shots:.0} shots, hits {:.1}%, shell deaths {shells:.1}, blast \
         deaths {blasts:.1}, plants {plants:.1}",
        hits * 100.0
    );
    assert!(
        runs.iter().all(|r| r[4] >= 1 && r[5] >= r[4] && r[3] >= 1),
        "{runs:?}"
    );
    assert!(runs.iter().all(|r| r[6] < 1500), "{runs:?}");
    assert!((310.0..=400.0).contains(&shots), "shots {shots}");
    assert!((0.22..=0.30).contains(&hits), "hits {hits}");
    assert!((5.0..=14.0).contains(&shells), "shell deaths {shells}");
    assert!((10.0..=20.0).contains(&blasts), "blast deaths {blasts}");
    assert!((3.0..=5.0).contains(&plants), "plants {plants}");
}

/// Race on 0x33 as the server sets it up: slot 0 an idle human with 99 lives (the oracle's L1/L2
/// poke), `bots` bots on script 0xBC, 5 laps on route 11 (`high`: 12). Runs to the results (or
/// `frames`); returns the match, the bots, each racer's lap ticks (`ride.lap` going up), the
/// human's death ticks and the missiles fired.
fn race_run(
    rom: &extract_core::Rom,
    high: bool,
    bots: usize,
    seed: u64,
    frames: u32,
) -> (
    conker_rules::Match,
    conker_bots::Bots,
    Vec<Vec<u32>>,
    Vec<u32>,
    usize,
) {
    use conker_bots::{Assets, Bots, DEFAULT_SKILL};
    use conker_rules::game::Event;
    use conker_rules::weapon::{Gives, Pickup};
    use conker_rules::{Arena, Fighter, Match, Rng, Spawn, race, spawn};
    // `race.json`'s laps (node indices from the start's).
    const LAPS: [[u8; 33]; 2] = [
        [
            19, 89, 20, 21, 22, 23, 24, 25, 84, 26, 86, 27, 28, 0, 1, 82, 2, 3, 4, 5, 6, 7, 8, 9,
            10, 11, 12, 13, 14, 15, 16, 17, 18,
        ],
        [
            19, 89, 20, 21, 22, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
        ],
    ];
    let scene = 0x33;
    let data: Vec<Vec<u8>> = (Assets::paths(scene).iter())
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &data[0],
        triggers: &data[1],
        ai: &data[2],
        class: &data[3],
    };
    let spawns: Vec<Spawn> = (extract_scene::scene_spawns(rom, scene).unwrap().iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let placements = extract_scene::scene_placements(rom, scene).unwrap();
    // `pickups.json`: canisters on placements 4, 5 and 9, missiles on 6, 7, 8 and 10.
    let (c, k) = (race::CANISTER, race::MISSILE);
    let pickups = [(4, c), (5, c), (6, k), (7, k), (8, k), (9, c), (10, k)]
        .map(|(placement, id)| {
            let at = placements[usize::from(placement)].position.map(f32::from);
            let gives = Gives::Weapon(id);
            (Pickup { placement, gives }, at)
        })
        .to_vec();
    let arena = Arena {
        spawns,
        pickups,
        respawn: Some(race::PICKUP_RESPAWN),
        track: race::Track::new(&data[0], &LAPS[usize::from(high)]),
        ..Default::default()
    };
    // The barriers the route hides (`race.json`: 11 shows 0 and 3, 12 shows 1 and 2).
    let mut world = world(rom, scene);
    for b in if high { [0, 3] } else { [1, 2] } {
        world.shift(b, 20_000);
    }
    let starts = conker_rules::spawn::starts(scene as u16).unwrap();
    let mut r = Rng { state: seed };
    let fighters = (0..=bots)
        .map(|i| {
            let at = spawn::place(&arena.spawns, usize::from(starts.deathmatch[i]), &mut r);
            Fighter {
                bot: i > 0,
                class: if i > 0 {
                    race::bot_class(i)
                } else {
                    race::HUMAN
                },
                extra: if i > 0 { 0 } else { 96 },
                ride: race::Ride::new(race::GO, i > 0),
                ..Fighter::new(&at, i as u8, 23)
            }
        })
        .collect();
    let mut m = Match::new(&arena, fighters, r, 23).with(conker_rules::Settings {
        race: true,
        time: None,
        laps: race::LAPS,
        high,
        ..Default::default()
    });
    let slots: Vec<usize> = (1..=bots).collect();
    let mut ai = Bots::new(assets, &[0], &slots, DEFAULT_SKILL, Rng { state: seed }).unwrap();
    let mut laps = vec![Vec::new(); bots + 1];
    let (mut deaths, mut shots) = (Vec::new(), 0);
    while m.race.results.is_none() && m.tick < frames {
        let mut inputs = vec![conker_sim::movement::Input::default(); bots + 1];
        let before: Vec<i8> = m.players.iter().map(|f| f.ride.lap).collect();
        let fx = ai.tick(&m, &arena, &world, &mut inputs);
        let mut ev = m.apply(&fx);
        ai.scopes(&mut m);
        ev.extend(m.step(&arena, &inputs, &world));
        ai.observe(&ev, &m);
        for (i, f) in m.players.iter().enumerate() {
            if f.ride.lap > before[i] {
                laps[i].push(m.tick);
            }
        }
        shots += ev
            .iter()
            .filter(|e| matches!(e, Event::Shot { .. }))
            .count();
        if (ev.iter()).any(|e| {
            matches!(
                e,
                Event::Hit {
                    victim: 0,
                    lethal: true,
                    ..
                }
            )
        }) {
            deaths.push(m.tick);
        }
    }
    (m, ai, laps, deaths, shots)
}

/// Script 0xBC races its bots, one idle human with 99 lives as the oracle's L1/L2 (3 bots: laps
/// 837 frames on route 11, 1063-1081 on route 12; finishes 4178-4230 and 5170-5190 frames after
/// the go; the human's first 3 deaths) and L7e/L7f (7 bots: finishes 4165-4323 and 4762-4880
/// frames after the go, means 4249 and 4814): no native unknown, the bots finish (a few may run
/// out of lives), the results follow, the 3 bots' steady laps stay within 2% of L1's and L2's,
/// they fire missiles, and the 7 bots' mean finish stays within 5%. Route 12's leader is knocked
/// up by the missile from placement 6 and blown up by the script (L2: 3 times, out at 4128).
/// Route 11 runs seed 2: L1's bot kicks the idle human at 305-316 (21 frames before its first
/// death), and the bots' kicks wait for the script's `random(0x78)` timers; seed 1's bots don't.
#[test]
fn race_bots_finish() {
    use conker_rules::race;
    let Some(rom) = rom() else { return };
    for (high, bots, seed, oracle, finish) in [
        (false, 3, 2, 837, 0),
        (true, 3, 1, 1066, 0),
        (true, 3, 2, 1066, 0),
        (false, 7, 2, 0, 4249),
        (true, 7, 2, 0, 4814),
    ] {
        let (m, ai, laps, deaths, shots) = race_run(&rom, high, bots, seed, 9000);
        let go = race::GO;
        for (i, f) in m.players.iter().enumerate() {
            let gaps: Vec<u32> = laps[i].windows(2).map(|w| w[1] - w[0]).collect();
            eprintln!(
                "route {} slot {i}: order {} out {} deaths {} laps {:?} (gaps {gaps:?})",
                11 + u8::from(high),
                f.ride.order,
                f.out,
                f.deaths,
                laps[i].iter().map(|t| t - go).collect::<Vec<_>>(),
            );
        }
        assert!(ai.unknown.is_empty(), "{:?}", ai.unknown);
        assert_eq!(ai.faults, 0);
        assert!(m.race.results.is_some());
        let done = (m.players.iter())
            .filter(|f| f.bot && f.ride.order > 0)
            .count();
        assert!(done + 2 >= bots, "{done} of {bots} finished");
        // The idle human (facing the line, 99 lives) dies at reads 378, 599 and 845 (V L1).
        if !high && bots == 3 {
            let reads: Vec<u32> = deaths.iter().take(3).map(|t| t + 23).collect();
            for (r, o) in reads.iter().zip([378, 599, 845]) {
                assert!(r.abs_diff(o) <= 8, "human deaths {reads:?}");
            }
        }
        // The bots fire their missiles (V L1: 928 and 938, L2), so they take canisters again.
        eprintln!("route {}: {shots} missiles", 11 + u8::from(high));
        assert!(shots > 0);
        // The 3 bots lap as L1's and L2's within 2% (the median of the last 3: a death's lap is
        // longer).
        if oracle > 0 {
            for l in laps.iter().skip(1) {
                let mut last: Vec<u32> = l[l.len() - 4..].windows(2).map(|w| w[1] - w[0]).collect();
                last.sort();
                let lap = last[1];
                assert!(
                    lap.abs_diff(oracle) * 100 <= oracle * 2,
                    "lap {lap} vs {oracle}"
                );
            }
        }
        // Route 12's leader blown up out of the race (V L2 4128).
        if high && bots == 3 {
            assert!(m.players.iter().any(|f| f.bot && f.out > 0), "no bot out");
        }
        // The 7 bots' mean finish after the go within 5% of L7e's, L7f's.
        if finish > 0 {
            let done: Vec<u32> = (laps.iter().skip(1))
                .filter_map(|l| l.get(usize::from(race::LAPS)).map(|t| t - go))
                .collect();
            let mean = done.iter().sum::<u32>() / done.len().max(1) as u32;
            assert!(
                mean.abs_diff(finish) * 100 <= finish * 5,
                "finish {mean} vs {finish}"
            );
        }
    }
}

/// War: Heist's takes with 3 bots and 1 idle human, as the oracle's (V hb3/hb 30 takes, hb2 20,
/// hb_0BADF00D 17, hb_00C0FFEE 18 in 9000 frames: the game's RNG poked at read 22 makes each a new
/// match). hb3's 30 is the high end of the original's own spread, not the port's rate.
#[test]
fn heist_takes_as_the_oracles() {
    let Some(rom) = rom() else { return };
    let heist = conker_rules::Settings {
        heist: true,
        score: conker_rules::heist::SCORE,
        ..Default::default()
    };
    let takes: Vec<u32> = (1..=4)
        .map(|seed| {
            let (mut takes, mut carrier) = (0, None);
            play(&rom, 0x1A, 3, seed, 9000, Some(heist), |m, _| {
                let c = m.heist.cash.carrier;
                takes += u32::from(c.is_some() && carrier.is_none());
                carrier = c;
            });
            takes
        })
        .collect();
    let mean = takes.iter().sum::<u32>() as f32 / takes.len() as f32;
    eprintln!("heist takes by seed {takes:?}, mean {mean} (oracle 30 20 17 18, mean 21.25)");
    assert!(
        (16.0..=30.0).contains(&mean) && takes.iter().all(|&t| t >= 10),
        "{takes:?}"
    );
}

/// The thrown bag against the vault's ring wall (V bt1200: P1 throws from (-1200, 16, -152) facing
/// 0x8500; the bag flies into the wall at 404 and is held at x -1324.9; bt1100, from -1100: it lands
/// short and slides into it, held at -1324.6), through the rules' sweep body.
#[test]
fn heist_bag_stops_at_the_vault_wall() {
    use conker_sim::collision::Sweep;
    let Some(rom) = rom() else { return };
    let w = world(&rom, 0x1A);
    let body = &conker_rules::heist::SWEPT;
    let fly = w.sweep(
        [-1320.3, 120.4, -135.5],
        [-1329.2, 113.9, -134.4],
        body,
        &Sweep::default(),
        -8.0,
    );
    let slide = w.sweep(
        [-1323.4, 19.0, -122.8],
        [-1325.9, 19.0, -122.5],
        body,
        &Sweep::default(),
        0.0,
    );
    assert!((fly.pos[0] + 1324.9).abs() < 0.1, "{:?}", fly.pos);
    assert!((slide.pos[0] + 1324.6).abs() < 0.1, "{:?}", slide.pos);
    // Along the wall the move goes on (V: z -135.5 -> -135.2 flying, -122.8 -> -122.7 sliding).
    assert!(
        fly.pos[2] > -135.5 && slide.pos[2] > -122.8,
        "{:?} {:?}",
        fly.pos,
        slide.pos
    );
}
