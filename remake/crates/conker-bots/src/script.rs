//! The game's level-script machine (`func_150ADAF0`, hand-written MIPS; C), which runs the multiplayer
//! AI: the bytecode is the scene's script asset (`[0x14, 0xBA]` for the Deathmatch AI), read from
//! the player's ROM. Its globals are the multiplayer struct (`*0x800D23B0`), so the game's C helpers
//! and the script share them ([`Globals`]).
//!
//! An instruction is a u16: the opcode in bits 0-5, up to three operands whose modes sit in bits
//! 13-15, 10-12 and 7-9 (0 a local `fp + s16`, 1 a global, 2 the address of script data, 3/4/6 an
//! immediate, 32-bit when bit 6 is set, 5 the result register), each followed by its s16 or s32.

/// One loaded script: the asset (code and data) and its entry points.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    code: Vec<u8>,
    /// Header +8: start of the data area (strings, tables).
    data: usize,
    /// Entry points of modes 0-4 (+0x20, then header +0xE..+0x14): 1 starts the level, 2 runs a frame.
    entries: [usize; 5],
}

/// Operand counts per opcode (`0x80087358`: 1 → none, 2 → one, 3 → two, else three).
const OPERANDS: [u8; 31] = [
    0, 3, 3, 3, 3, 3, 2, 2, 2, 3, 3, 3, 3, 3, 2, 1, 2, 2, 3, 3, 3, 3, 3, 3, 1, 1, 1, 1, 0, 3, 3,
];
/// Bytes of the machine's stack (`0x800D3860 + 0x17FC`).
const STACK: usize = 0x17FC;
/// Instructions one run may take before it is called a hang.
const BUDGET: u32 = 2_000_000;

impl Script {
    pub fn parse(asset: &[u8]) -> Option<Self> {
        let h = |o: usize| {
            Some(usize::from(u16::from_be_bytes([
                *asset.get(o)?,
                *asset.get(o + 1)?,
            ])))
        };
        let entries = [0x20, h(0xE)?, h(0x10)?, h(0x12)?, h(0x14)?];
        Some(Self {
            code: asset.to_vec(),
            data: h(8)?,
            entries,
        })
    }

    fn u16(&self, pc: usize) -> Result<u16, Fault> {
        let b = self.code.get(pc..pc + 2).ok_or(Fault::Pc(pc))?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i32(&self, pc: usize) -> Result<i32, Fault> {
        let b = self.code.get(pc..pc + 4).ok_or(Fault::Pc(pc))?;
        Ok(i32::from_be_bytes(b.try_into().unwrap()))
    }
}

/// The script's global memory, big-endian like the game's (C helpers write bytes and halfwords that
/// the script reads as words).
#[derive(Clone, Debug, PartialEq)]
pub struct Globals(pub Vec<u8>);

impl Globals {
    pub fn new(size: usize) -> Self {
        Self(vec![0; size])
    }
    fn at<const N: usize>(&self, off: i32) -> [u8; N] {
        let o = off as usize;
        self.0
            .get(o..o + N)
            .map_or([0; N], |b| b.try_into().unwrap())
    }
    fn put(&mut self, off: i32, b: &[u8]) {
        let o = off as usize;
        if let Some(d) = self.0.get_mut(o..o + b.len()) {
            d.copy_from_slice(b);
        }
    }
    pub fn i32(&self, off: i32) -> i32 {
        i32::from_be_bytes(self.at(off))
    }
    pub fn set_i32(&mut self, off: i32, v: i32) {
        self.put(off, &v.to_be_bytes());
    }
    /// Word `i` of the array at `off` (the script's `G[off][i]`).
    pub fn get(&self, off: i32, i: usize) -> i32 {
        self.i32(off + 4 * i as i32)
    }
    pub fn set(&mut self, off: i32, i: usize, v: i32) {
        self.set_i32(off + 4 * i as i32, v);
    }
    pub fn i8(&self, off: i32) -> i8 {
        self.at::<1>(off)[0] as i8
    }
    pub fn set_i8(&mut self, off: i32, v: i8) {
        self.put(off, &[v as u8]);
    }
    pub fn f32(&self, off: i32) -> f32 {
        f32::from_be_bytes(self.at(off))
    }
    pub fn set_f32(&mut self, off: i32, v: f32) {
        self.put(off, &v.to_be_bytes());
    }
}

/// What the script calls out to (the native table `0x800886A4`, by index): 3/4 objects, 5 queries
/// and 6 commands on a handle (`args[0]`, type in bits 12-15) with a code (`args[1]`), 7 a random
/// number below `args[0]`. `args[0]` is the last value pushed.
pub trait Host {
    fn native(&mut self, g: &mut Globals, index: i32, args: &[i32]) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Pc(usize),
    Opcode(usize, u16),
    Mode(usize, u8),
    Stack,
    Hang,
}

/// Where an operand lives.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Loc {
    Local(i32),
    Global(i32),
    Data(i32),
    Imm,
    Result,
}

/// The machine's stack and result register; one per script, kept across runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    stack: Vec<u8>,
    result: i32,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            stack: vec![0; STACK],
            result: 0,
        }
    }
}

/// A pushed address of script data (only the debug prints take one).
pub const DATA_TAG: i32 = 0x4000_0000;

impl Machine {
    fn word(&self, off: i32) -> Result<i32, Fault> {
        let o = usize::try_from(off).map_err(|_| Fault::Stack)?;
        let b = self.stack.get(o..o + 4).ok_or(Fault::Stack)?;
        Ok(i32::from_be_bytes(b.try_into().unwrap()))
    }

    fn set_word(&mut self, off: i32, v: i32) -> Result<(), Fault> {
        let o = usize::try_from(off).map_err(|_| Fault::Stack)?;
        let b = self.stack.get_mut(o..o + 4).ok_or(Fault::Stack)?;
        b.copy_from_slice(&v.to_be_bytes());
        Ok(())
    }

    fn load(&self, s: &Script, g: &Globals, loc: Loc, imm: i32) -> Result<i32, Fault> {
        Ok(match loc {
            Loc::Local(o) => self.word(o)?,
            Loc::Global(o) => g.i32(o),
            Loc::Data(o) => {
                let a = s.data + o as usize;
                s.i32(a).unwrap_or(0)
            }
            Loc::Imm => imm,
            Loc::Result => self.result,
        })
    }

    fn store(&mut self, g: &mut Globals, loc: Loc, v: i32) -> Result<(), Fault> {
        match loc {
            Loc::Local(o) => self.set_word(o, v)?,
            Loc::Global(o) => g.set_i32(o, v),
            Loc::Result => self.result = v,
            Loc::Data(_) | Loc::Imm => {}
        }
        Ok(())
    }

    /// Runs entry `mode` until its outermost function returns.
    pub fn run(
        &mut self,
        s: &Script,
        g: &mut Globals,
        host: &mut impl Host,
        mode: usize,
    ) -> Result<(), Fault> {
        let top = STACK as i32;
        let (mut sp, mut fp) = (top, top);
        let mut pc = s.entries[mode];
        let mut pushed = 0;
        for _ in 0..BUDGET {
            let start = pc;
            let w = s.u16(pc)?;
            pc += 2;
            let op = usize::from(w & 0x3F);
            let n = *OPERANDS.get(op).ok_or(Fault::Opcode(start, w))?;
            if op == 0 {
                return Err(Fault::Opcode(start, w));
            }
            let mut wide = w & 0x40 != 0;
            // (location, value, mode) of each operand; a data operand's value is its offset.
            let mut ops = [(Loc::Imm, 0i32, 0u8); 3];
            for (k, shift) in [13, 10, 7].into_iter().take(usize::from(n)).enumerate() {
                let mode = ((w >> shift) & 7) as u8;
                let (loc, imm) = match mode {
                    3 | 4 | 6 if wide => {
                        wide = false;
                        pc += 4;
                        (Loc::Imm, s.i32(pc - 4)?)
                    }
                    _ => {
                        let v = i32::from(s.u16(pc)? as i16);
                        pc += 2;
                        match mode {
                            0 => (Loc::Local(fp + v), 0),
                            1 => (Loc::Global(v), 0),
                            2 => (Loc::Data(v), v),
                            3 | 4 | 6 => (Loc::Imm, v),
                            5 => (Loc::Result, 0),
                            _ => return Err(Fault::Mode(start, mode)),
                        }
                    }
                };
                let value = match loc {
                    Loc::Data(_) => imm,
                    _ => self.load(s, g, loc, imm)?,
                };
                ops[k] = (loc, value, mode);
            }
            let [(a, va, ma), (b, vb, _), (_, vc, _)] = ops;
            let arith = |f: fn(i32, i32) -> i32| f(vb, vc);
            match op {
                1 => self.store(g, a, arith(i32::wrapping_add))?,
                2 => self.store(g, a, arith(i32::wrapping_sub))?,
                3 => self.store(g, a, arith(i32::wrapping_mul))?,
                4 => self.store(g, a, if vc == 0 { 0 } else { vb.wrapping_div(vc) })?,
                5 => self.store(g, a, if vc == 0 { 0 } else { vb.wrapping_rem(vc) })?,
                6 => self.store(g, a, vb.wrapping_neg())?,
                7 => self.store(g, a, i32::from(vb == 0))?,
                8 => self.store(g, a, !vb)?,
                9 => self.store(g, a, vb & vc)?,
                10 => self.store(g, a, vb | vc)?,
                11 => self.store(g, a, vb ^ vc)?,
                12 => self.store(g, a, vb.wrapping_shl(vc as u32 & 31))?,
                13 => self.store(g, a, vb >> (vc as u32 & 31))?,
                14 => self.store(g, a, vb)?,
                15 => pc = va as usize,
                16 if va == 0 => pc = vb as usize,
                17 if va != 0 => pc = vb as usize,
                16 | 17 => {}
                18 => self.store(g, a, i32::from(vb < vc))?,
                19 => self.store(g, a, i32::from(vb > vc))?,
                20 => self.store(g, a, i32::from(vb <= vc))?,
                21 => self.store(g, a, i32::from(vb >= vc))?,
                22 => self.store(g, a, i32::from(vb == vc))?,
                23 => self.store(g, a, i32::from(vb != vc))?,
                24 => {
                    sp -= 4;
                    pushed += 1;
                    let v = if ma == 2 { DATA_TAG | va } else { va };
                    self.set_word(sp, v)?;
                }
                25 if ma == 6 => {
                    let args: Vec<i32> = (0..pushed)
                        .map(|k| self.word(sp + 4 * k))
                        .collect::<Result<_, _>>()?;
                    pushed = 0;
                    self.result = host.native(g, va, &args);
                }
                25 => {
                    pushed = 0;
                    sp -= 4;
                    self.set_word(sp, pc as i32)?;
                    pc = va as usize;
                }
                26 | 28 => {
                    if op == 26 {
                        self.result = va;
                    }
                    sp = fp;
                    fp = self.word(sp).unwrap_or(top);
                    sp += 4;
                    pc = self.word(sp).unwrap_or(0) as usize;
                    sp += 4;
                    pushed = 0;
                    if sp >= top {
                        return Ok(());
                    }
                }
                27 => {
                    sp -= 4;
                    self.set_word(sp, fp)?;
                    fp = sp;
                    sp -= va;
                }
                29 | 30 => {
                    let at = match b {
                        Loc::Local(o) => Loc::Local(o + 4 * vc),
                        Loc::Global(o) => Loc::Global(o + 4 * vc),
                        Loc::Data(o) => Loc::Data(o + 4 * vc),
                        _ => return Err(Fault::Mode(start, 0xFF)),
                    };
                    if op == 29 {
                        let v = self.load(s, g, at, 0)?;
                        self.store(g, a, v)?;
                    } else {
                        self.store(g, at, va)?;
                    }
                }
                _ => return Err(Fault::Opcode(start, w)),
            }
        }
        Err(Fault::Hang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assembles a script: entry 1 at +0x26 (+0x20 is `enter 0; leave`), data after the code.
    fn script(code: &[u16]) -> Script {
        let mut a = vec![0u8; 0x26];
        a[0x20..0x26].copy_from_slice(&[0x60, 0x1B, 0, 0, 0, 0x1C]);
        for w in code {
            a.extend_from_slice(&w.to_be_bytes());
        }
        let data = a.len() as u16;
        a[8..10].copy_from_slice(&data.to_be_bytes());
        a[0xE..0x10].copy_from_slice(&0x26u16.to_be_bytes());
        Script::parse(&a).unwrap()
    }

    struct Log(Vec<(i32, Vec<i32>)>);
    impl Host for Log {
        fn native(&mut self, _: &mut Globals, index: i32, args: &[i32]) -> i32 {
            self.0.push((index, args.to_vec()));
            args.iter().sum()
        }
    }

    // Instruction words: op | A << 13 | B << 10 | C << 7 (modes: 0 local, 1 global, 3 imm, 5 R, 6 native).
    const fn w(op: u16, a: u16, b: u16, c: u16) -> u16 {
        op | a << 13 | b << 10 | c << 7
    }

    #[test]
    fn arithmetic_loops_and_natives() {
        // G[0] = 0; for i in 0..5 { G[0] += i }; R = native5(7, G[0]); G[4] = R
        let s = script(&[
            w(27, 3, 0, 0),
            8, // enter 8
            w(14, 1, 3, 0),
            0,
            0, // mov G[0], #0
            w(14, 0, 3, 0),
            (-4i16) as u16,
            0, // mov L[-4], #0
            // loop @0x36:
            w(18, 0, 0, 3),
            (-8i16) as u16,
            (-4i16) as u16,
            5, // lt L[-8], L[-4], #5
            w(16, 0, 3, 0),
            (-8i16) as u16,
            0x58, // jz L[-8], @0x58
            w(1, 1, 1, 0),
            0,
            0,
            (-4i16) as u16, // add G[0], G[0], L[-4]
            w(1, 0, 0, 3),
            (-4i16) as u16,
            (-4i16) as u16,
            1, // add L[-4], L[-4], #1
            w(15, 3, 0, 0),
            0x36, // jmp @0x36
            // @0x58:
            w(24, 1, 0, 0),
            0, // push G[0]
            w(24, 3, 0, 0),
            7, // push #7
            w(25, 6, 0, 0),
            5, // call native5
            w(14, 1, 5, 0),
            4,
            0,              // mov G[4], R
            w(28, 0, 0, 0), // leave
        ]);
        let (mut g, mut m, mut log) = (Globals::new(16), Machine::default(), Log(Vec::new()));
        m.run(&s, &mut g, &mut log, 1).unwrap();
        assert_eq!(g.i32(0), 10);
        assert_eq!(log.0, [(5, vec![7, 10])]);
        assert_eq!(g.i32(4), 17);
    }

    #[test]
    fn calls_return_and_index() {
        // f(x) = x * 3 at @0x46; G[8][2] = f(4); G[0] = G[8][2]
        let s = script(&[
            w(27, 3, 0, 0),
            0, // enter 0
            w(24, 3, 0, 0),
            4, // push #4
            w(25, 3, 0, 0),
            0x46, // call @0x46
            w(30, 5, 1, 3),
            0,
            8,
            2, // stx R, G[8], #2
            w(29, 1, 1, 3),
            0,
            8,
            2,              // ldx G[0], G[8], #2
            w(28, 0, 0, 0), // leave
            0,              // pad to @0x46
            // @0x46: enter 4; mul L[-4], L[+8], #3; ret L[-4]
            w(27, 3, 0, 0),
            4,
            w(3, 0, 0, 3),
            (-4i16) as u16,
            8,
            3,
            w(26, 0, 0, 0),
            (-4i16) as u16,
        ]);
        let (mut g, mut m, mut log) = (Globals::new(32), Machine::default(), Log(Vec::new()));
        m.run(&s, &mut g, &mut log, 1).unwrap();
        assert_eq!((g.get(8, 2), g.i32(0)), (12, 12));
        assert!(log.0.is_empty());
    }
}
