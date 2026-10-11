//! The game's random number generator, bit-exact (C, V on the oracle: see `tests/oracle.rs`).
//!
//! One 64-bit state word at `0x800885B0`, of which only the low 33 bits survive a step (the
//! 33-bit LFSR Rare also used in GoldenEye and Perfect Dark). `random_u32` (0x150ADA20) steps it
//! and returns the low 32 bits; `random_float` (0x150ADA68) steps it and scales the low 16 bits
//! to [0, 1]. `func_150ADACC(a)` seeds it with `a + 1`.
//!
//! The original advances this one generator from about 600 call sites (AI, particles, sounds,
//! spawns...), many times per frame, so a remake cannot reproduce its sequence; what is exact is
//! the algorithm and how many draws each ported rule takes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    pub state: u64,
}

impl Rng {
    /// The game's seed: `func_150ADACC(0x81280783)`, the constant loaded with `lui`/`ori` (so
    /// sign-extended to 64 bits), plus 1. Called by `func_15007830` and `func_1501905C` when the
    /// front-end phase (`0x800E0B94`) is 2 (C). The oracle's multiplayer boot never seeds: the
    /// state there is whatever boot left.
    pub const GAME_SEED: u64 = 0xFFFF_FFFF_8128_0784;

    pub const fn new(state: u64) -> Self {
        Self { state }
    }

    /// `random_u32` (0x150ADA20).
    pub fn next_u32(&mut self) -> u32 {
        let s = self.state;
        let a = ((s << 63) >> 31 | (s << 31) >> 32) ^ ((s << 44) >> 32);
        self.state = ((a >> 20) & 0xFFF) ^ a;
        self.state as u32
    }

    /// `random_float` (0x150ADA68): `(low 16 bits) * f32(0x37800080)`, i.e. about n / 65535.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() & 0xFFFF) as f32 * f32::from_bits(0x3780_0080)
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new(Self::GAME_SEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// States read on the oracle (lab/oracle/trajectories/spawn_h1b1_lldb.txt): one draw each.
    #[test]
    fn steps_like_the_game() {
        for (before, after) in [
            (0xb485_a22e, 0x0060_3111),
            (0xd459_3932, 0xf9bf_b302),
            (0xf9bf_b302, 0x87ef_f1ff),
            (0x1_bce8_0331, 0x1_5e47_147c),
        ] {
            let mut r = Rng::new(before);
            r.next_u32();
            assert_eq!(r.state, after, "{before:#x}");
        }
    }

    #[test]
    fn float_is_the_low_half_scaled() {
        let mut r = Rng::new(0xd459_3932);
        let f = r.next_f32();
        assert_eq!(f, (0xb302 as f32) * f32::from_bits(0x3780_0080));
        assert!((0.0..=1.0).contains(&f));
        assert_eq!(Rng::new(0).next_f32(), 0.0); // 0 is a fixed point
    }

    #[test]
    fn seed_keeps_33_bits() {
        let mut a = Rng::default();
        let mut b = Rng::new(Rng::GAME_SEED & 0x1_FFFF_FFFF);
        assert_eq!(a.next_u32(), b.next_u32());
        assert_eq!(a, b);
        assert!(a.state < 1 << 33);
    }
}
