//! The game's arctangent (`func_1505A630` → `func_150484A0`): `acos` from a table, interpolated linearly.
//! The table is game data, so it is read from the player's own ROM (extract-core's `Rom::game_data`, the
//! `.game` data loaded at 0x80082B20), like the collision.
//!
//! The game turns wall pushes (the heading the camera follows in state 122) and the stick into angles with
//! it. Its error against the true arctangent reaches a few angle units, so nothing else reproduces it (V:
//! camera_wall's 34 wall headings, 2-3 units off with an exact arctangent, all equal with this).

/// `D_80085EC0`, `D_80085F3C`, `D_80085FBC`: acos(x) in units of π/65536 for x = i·512, 0x7800 + i·32 and
/// 0x7FE0 + i·8 (in 32767ths), 62 + 64 + 6 entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcosTable(Vec<u16>);

/// Where the table sits in `.game` data (0x80085EC0 − 0x80082B20), and its three parts.
const OFFSET: usize = 0x33A0;
const PARTS: [usize; 3] = [62, 64, 6];

impl AcosTable {
    /// The table itself (the asset pack's `game/acos.json`); None unless it has 132 entries.
    pub fn new(table: Vec<u16>) -> Option<Self> {
        (table.len() == PARTS.iter().sum::<usize>()).then_some(Self(table))
    }

    /// The table from the decompressed `.game` data; None if `data` is too short.
    pub fn from_game_data(data: &[u8]) -> Option<Self> {
        let n: usize = PARTS.iter().sum();
        let bytes = data.get(OFFSET..OFFSET + 2 * n)?;
        Some(Self(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&b| u16::from_be_bytes(b))
                .collect(),
        ))
    }

    /// `func_150485E0`: acos of `a`/32767 for `a` in 0..=0x7FFF, in π/65536 units.
    fn lookup(&self, a: i32) -> i32 {
        let (part, shift, a) = if a >= 0x7FE0 {
            (PARTS[0] + PARTS[1], 3, a - 0x7FE0)
        } else if a >= 0x7800 {
            (PARTS[0], 5, a - 0x7800)
        } else {
            (0, 9, a)
        };
        let i = part + (a >> shift) as usize;
        let (t0, t1) = (self.0[i] as i32, self.0[i + 1] as i32);
        t0 - (((t0 - t1) * (a & ((1 << shift) - 1))) >> shift)
    }

    /// `func_15048360`: the game's `acosf`, its argument truncated to 32767ths.
    pub fn acos(&self, v: f32) -> f32 {
        let i = if 1.0 <= v {
            0x7FFF
        } else if v <= -1.0 {
            -0x7FFF
        } else {
            (v * 32767.0) as i32 as i16 as i32
        };
        let r = self.lookup(i.abs());
        let r = if i < 0 { 0xFFFF - r } else { r } & 0xFFFF;
        r as f32 * f32::from_bits(0x3849_10A4)
    }

    /// `func_150484A0`: the angle in radians, in [0, 2π), from +y towards +x of `(x, y)`.
    pub fn atan(&self, x: f32, y: f32) -> f32 {
        const PI: f32 = f32::from_bits(0x4049_0FDB);
        const HALF_PI: f32 = f32::from_bits(0x3FC9_0FDB);
        const THREE_HALF_PI: f32 = f32::from_bits(0x4096_CBE4);
        const TAU: f32 = f32::from_bits(0x40C9_0FDB);
        if x == 0.0 {
            return if 0.0 <= y { 0.0 } else { PI };
        }
        if y == 0.0 {
            return if 0.0 < x { HALF_PI } else { THREE_HALF_PI };
        }
        let len = (x * x + y * y).sqrt();
        if y < x {
            let a = self.acos(y / len);
            return if x < 0.0 { TAU - a } else { a };
        }
        let mut a = HALF_PI - self.acos(x / len);
        if y < 0.0 {
            a = PI - a;
        }
        if a < 0.0 { a + TAU } else { a }
    }

    /// `func_1505A630`: the u16 angle (the game's `(cos a, -sin a)` convention) of the XZ vector
    /// `(dx, -mdz)`; the caller passes `-dz` as the game does.
    pub fn angle(&self, dx: f32, mdz: f32) -> u16 {
        let r = self.atan(-dx, mdz) * f32::from_bits(0x4622_F983);
        (r as u32).wrapping_add(0x4000) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in table: the true acos, rounded (the game's differs by up to a unit; see the ROM test in
    /// tests/oracle.rs).
    fn table() -> AcosTable {
        let at = |k: usize| {
            let x = (k as f64 / 32767.0).min(1.0);
            (x.acos() / std::f64::consts::PI * 65536.0).round() as u16
        };
        let mut t: Vec<u16> = (0..62).map(|i| at(i * 512)).collect();
        t.extend((0..64).map(|i| at(0x7800 + i * 32)));
        t.extend((0..6).map(|i| at(0x7FE0 + i * 8)));
        AcosTable(t)
    }

    #[test]
    fn axes_and_diagonals() {
        let t = table();
        // The game's convention: +x is 0, -z (north) is 16384.
        assert_eq!(t.angle(1.0, 0.0), 0);
        assert_eq!(t.angle(0.0, 1.0), 16384);
        assert_eq!(t.angle(-1.0, 0.0), 32768);
        assert_eq!(t.angle(0.0, -1.0), 49152);
        for (dx, mdz, want) in [
            (1.0, 1.0, 8192),
            (-1.0, 1.0, 24576),
            (-1.0, -1.0, 40960),
            (1.0, -1.0, 57344),
        ] {
            let a = t.angle(dx, mdz) as i32;
            assert!((a - want).abs() <= 2, "{dx} {mdz}: {a}");
        }
    }
}
