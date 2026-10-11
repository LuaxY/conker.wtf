//! Collision surface words: one u32 per terrain face (`extract_scene::Triangle::surface`), which the
//! player's floor query copies to obj+0x184 each frame. The game reads three fields of it (C):
//! - bits 0-4, the [kind](Surface::kind) (`func_1510F8CC` is `word & 0x1F`). It indexes the sink table
//!   ([`SINK`]), picks impact effects when a projectile hits the face (`func_150E1D14` →
//!   `func_150E1AB0`), and kinds 0xE and 0x13 call `func_1507CD64(obj, 0xD)` where every other kind
//!   calls it with 0. Of scene 0x2D's kinds (0, 1, 3, 4, 5, 9, 0xA), only 0xA has a sink entry.
//! - bits 28-30, the [slide class](Surface::slide), copied to `0x800CC27C` by the movement code
//!   (`func_1504CB98`). Scene 0x2D's 0x60000005 faces (class 6) are the steep sides of the two
//!   central structures (normal.y 0.35-0.79): an idle player slides down them.
//! - bits 12-15 (`func_15051558` tests them against 3 and 4; meaning unread): 0 on every 0x2D word.
//!
//! Untraced: what the 0xA kind's obj+0xAA byte drives, kinds 0xE/0x13's `func_1507CD64` call, and
//! classes other than 0 and 6.

/// A face's surface word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Surface(pub u32);

impl Surface {
    /// Bits 0-4: the material kind.
    pub const fn kind(self) -> usize {
        (self.0 & 0x1F) as usize
    }

    /// Bits 28-30, with 7 read as 0 (C, `func_1504CB98`). Non-zero means slippery: while the player
    /// is within 5 units of the floor, `func_1505A3A8` zeroes one of its scale terms (C; I: stick
    /// control), and in the oracle an idle player on a class-6 face slides downhill (V,
    /// `lab/oracle/surface_README.md`).
    pub const fn slide(self) -> u8 {
        match (self.0 >> 28) & 7 {
            7 => 0,
            c => c as u8,
        }
    }

    /// The kind's [`SINK`] entry.
    pub const fn sink(self) -> [i8; 3] {
        SINK[self.kind()]
    }
}

/// Per kind, three signed bytes at `0x80099140` in `.game` data (C, `func_1504BA38`; the ROM test
/// checks them):
/// 0. the depth the player sinks to (obj+0xCE, negative is down),
/// 1. when non-zero, that depth scales with speed: `trunc(b1 · 0.1 · (b0 − speed))`,
/// 2. when non-zero, stored in obj+0xAA if that byte is 0 (consumer untraced; 90 while skidding on
///    kind 0xA in the oracle).
///
/// No ROM word has a kind above 0x13 (`tests/rom.rs`); entries 0x14-0x1F are the next table's bytes,
/// kept because `& 0x1F` reaches them.
pub const SINK: [[i8; 3]; 32] = {
    let mut t = [[0; 3]; 32];
    t[0x07] = [45, -5, 50];
    t[0x0A] = [-6, 0, 90];
    t[0x0B] = [-6, 0, 90];
    t[0x0C] = [44, -6, 40];
    t[0x0E] = [-100, 0, 0];
    t[0x0F] = [44, -4, 80];
    t[0x12] = [44, -4, 80];
    t[0x13] = [-100, 0, 30];
    t[0x14] = [0, 0, -64];
    t[0x15] = [0, 64, 0];
    t[0x16] = [-1, -1, -128];
    t[0x17] = [0, -96, 0];
    t[0x18] = [96, 0, -128];
    t[0x1A] = [-32, 0, 32];
    t[0x1C] = [-1, -1, -1];
    t[0x1D] = [-1, -1, -1];
    t[0x1E] = [-1, -1, 2];
    t[0x1F] = [-79, 2, -75];
    t
};

/// `func_1504BA38`, once per frame before the move: the sink depth (obj+0xCE) the player heads for
/// on `surface` (the floor word of the previous frame, obj+0x184), given the speed (obj+0x3C) and
/// the height above the floor (obj+0x28). More than 10 above the floor it is 0.
pub fn sink_target(surface: Surface, speed: f32, height: f32) -> i16 {
    if 10.0 < height {
        return 0;
    }
    let [depth, per_speed, _] = surface.sink();
    if per_speed == 0 {
        return depth.into();
    }
    let scale = f32::from(per_speed) * f32::from_bits(0x3DCC_CCCD); // 0.1
    (scale * (f32::from(depth) - speed)) as i32 as i16
}

/// `func_1505250C`, later in the same frame: the sink (obj+0xCC) steps toward the target by a
/// twelfth of the gap plus one, without overshooting. The camera's look-at height adds it (C,
/// `func_15124C38`; V: 61.5 above the feet at rest on kind 0xA, 67.5 elsewhere). The player's
/// position is not moved.
pub fn step_sink(sink: i16, target: i16) -> i16 {
    let (cc, ce) = (i32::from(sink), i32::from(target));
    if cc < ce {
        ((cc + (ce - cc) / 12 + 1) as i16).min(target)
    } else if ce < cc {
        ((cc + (ce - cc) / 12 - 1) as i16).max(target)
    } else {
        sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields() {
        let slide = Surface(0x6000_0005);
        assert_eq!((slide.kind(), slide.slide()), (5, 6));
        assert_eq!(Surface(0x7000_000A).slide(), 0); // class 7 reads as 0
        assert_eq!((Surface(0xA).kind(), Surface(0xA).slide()), (0xA, 0));
        assert_eq!(Surface(0x25).kind(), 5); // only the low 5 bits
    }

    #[test]
    fn scene_2d_kinds() {
        for word in [0x0, 0x1, 0x3, 0x4, 0x5, 0x9, 0x6000_0005] {
            assert_eq!(sink_target(Surface(word), 20.0, 0.0), 0, "{word:#x}");
        }
        assert_eq!(sink_target(Surface(0xA), 35.0, 0.0), -6);
        assert_eq!(sink_target(Surface(0xA), 0.0, 10.5), 0); // airborne
        assert_eq!(sink_target(Surface(0xA), 0.0, 10.0), -6);
    }

    #[test]
    fn speed_scaled_kinds() {
        // kind 0xC: -6 · 0.1 · (44 - speed), truncated toward zero
        assert_eq!(sink_target(Surface(0xC), 0.0, 0.0), -26);
        assert_eq!(sink_target(Surface(0xC), 35.0, 0.0), -5);
        assert_eq!(sink_target(Surface(0xC), 50.0, 0.0), 3);
    }

    #[test]
    fn sink_steps() {
        // V on surface_a: 0, -1, ..., -6 on consecutive frames
        let seq: Vec<i16> = std::iter::successors(Some(0), |&cc| Some(step_sink(cc, -6)))
            .take(7)
            .collect();
        assert_eq!(seq, [0, -1, -2, -3, -4, -5, -6]);
        assert_eq!(step_sink(-6, -6), -6);
        assert_eq!(step_sink(0, -100), -9); // a twelfth of the gap, plus one
        assert_eq!(step_sink(-100, 0), -91);
        assert_eq!(step_sink(-1, 0), 0); // no overshoot
    }
}
