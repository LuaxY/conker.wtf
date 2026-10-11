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
