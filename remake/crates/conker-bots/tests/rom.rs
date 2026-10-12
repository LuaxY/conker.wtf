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
    let mut bots = Bots::new(assets, 2, &[], 2, Rng::default()).unwrap();
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
    // The target: the third capture ends the match on its frame.
    assert_eq!(m.colors.scores, [3, 0]);
    assert_eq!(m.ended_at, Some(captures[2].at));
}

/// War: Colors with bots, as the server runs it (7 bots, the squirrels in the even slots, and an
/// idle human, a Tedi): a collector takes the enemy flag within 5,400 frames (here a Tedi, the
/// green flag, at 2,398; V bots1: a squirrel, the red flag, 1,719 frames in). The collectors' way
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
    let arena = Arena { spawns, pickups };
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
    let mut bots = Bots::new(assets, 8, &slots, DEFAULT_SKILL, rng).unwrap();
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
    use conker_sim::movement::{BUTTON_Z, Input};
    let Some(rom) = rom() else { return };
    let arena = Arena {
        spawns: Vec::new(),
        pickups: Vec::new(),
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
    let mut hits = Vec::new();
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
            }
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
    let p = m.players[1].body.pos;
    let d = ((p[0] - 2216.0).powi(2) + (p[2] - 749.0).powi(2)).sqrt();
    assert!((110.0..170.0).contains(&d), "{d}");
    eprintln!(
        "e1: {} shots (V 13), P2 knocked {d:.1} (V 140.7)",
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
    };
    let mut m = Match::new(&arena, players, Rng::default(), 0).with(Settings {
        beach: true,
        ..Settings::default()
    });
    let mut bots = Bots::new(assets, 3, &[], 2, Rng::default()).unwrap();
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
