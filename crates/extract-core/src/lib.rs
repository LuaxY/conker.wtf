//! Container layer for Conker's Bad Fur Day (USA): ROM validation, the asset table, Rare's rzip,
//! the compressed `.game` code/data, and the texture blob.
//!
//! Sources: the table and rzip contract follows DevOldSchool/conkers-bfd-decomp `docs/rzip-assets.md`
//! (MIT, commit e2400b27dda6) and `func_1502B224` (mkst/conker decomp); the `.game` block table is
//! ported from CBFD-Recompiled `recomp/unpack_rom.py` (MIT, (c) 2026 Sean Ciaschi). Every rule was
//! re-verified on the US ROM (`lab/research/assets-probe`, and this crate's `CONKER_ROM` tests).

use std::borrow::Cow;
use std::ops::Range;

/// SHA-1 of the only supported ROM, in big-endian .z64 order.
pub const US_SHA1: &str = "4cbadd3c4e0729dec46af64ad018050eada4f47a";
/// Top-level asset table: 30 entries; 0..=0x1C are banks (nested tables), 0x1D is a raw 0x90-byte item.
pub const ASSET_TABLE: usize = 0xAB1950;
/// Number of banks in the top-level asset table.
pub const BANK_COUNT: usize = 29;
/// Number of files in the texture blob (texture id = blob index).
pub const TEXTURE_COUNT: usize = 7762;

const GAME_ARCHIVE: Range<usize> = 0x42450..0x19EA88;
const GAME_XOR: u32 = 0x8039_CCCA;
const GAME_DATA_VRAM: usize = 0x8008_2B20;
const TEXTURE_SIZES_VRAM: usize = 0x8009_1D20; // u16 compressed sizes, walked by func_15003570
const TEXTURE_BLOB: Range<usize> = 0x1A37E0..ASSET_TABLE;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not an N64 ROM (first bytes {0:02x?})")]
    UnknownByteOrder([u8; 4]),
    #[error("not Conker's Bad Fur Day (USA): SHA-1 {0}, expected {US_SHA1}")]
    WrongRom(String),
    #[error("out of bounds: {len} bytes at {offset:#x}, data is {have} bytes")]
    OutOfBounds {
        offset: usize,
        len: usize,
        have: usize,
    },
    #[error("malformed data: {0}")]
    Malformed(&'static str),
    #[error("no entry {index} (table has {count})")]
    NoEntry { index: usize, count: usize },
    #[error("inflate failed: {0}")]
    Inflate(String),
    #[error("rzip size mismatch: header says {expected}, inflated {actual}")]
    SizeMismatch { expected: usize, actual: usize },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn get(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or(Error::OutOfBounds {
            offset,
            len,
            have: data.len(),
        })
}

fn be32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        get(data, offset, 4)?.try_into().unwrap(),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// Big-endian (native N64), `80 37 12 40`.
    Z64,
    /// 16-bit byte-swapped, `37 80 40 12`.
    V64,
    /// 32-bit little-endian, `40 12 37 80`.
    N64,
}

/// Rewrites `rom` in place into big-endian .z64 order; returns the order it was in.
pub fn normalize(rom: &mut [u8]) -> Result<ByteOrder> {
    let Some(&[a, b, c, d]) = rom.get(..4) else {
        return Err(Error::OutOfBounds {
            offset: 0,
            len: 4,
            have: rom.len(),
        });
    };
    let order = match [a, b, c, d] {
        [0x80, 0x37, 0x12, 0x40] => ByteOrder::Z64,
        [0x37, 0x80, 0x40, 0x12] => ByteOrder::V64,
        [0x40, 0x12, 0x37, 0x80] => ByteOrder::N64,
        head => return Err(Error::UnknownByteOrder(head)),
    };
    match order {
        ByteOrder::Z64 => {}
        ByteOrder::V64 => rom
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .for_each(|w| w.swap(0, 1)),
        ByteOrder::N64 => rom
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|w| w.reverse()),
    }
    Ok(order)
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    sha1::Sha1::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Inflates Rare's rzip: a big-endian u32 decompressed size (bit 31 ignored, as the game does)
/// followed by a raw deflate stream. Bytes after the end of the stream are ignored.
pub fn inflate_rzip(data: &[u8]) -> Result<Vec<u8>> {
    let expected = (be32(data, 0)? & 0x7FFF_FFFF) as usize;
    // One byte of headroom: a longer stream stops there instead of growing without bound.
    let out =
        miniz_oxide::inflate::decompress_to_vec_with_limit(&data[4..], expected.saturating_add(1))
            .map_err(|e| Error::Inflate(e.to_string()))?;
    if out.len() != expected {
        return Err(Error::SizeMismatch {
            expected,
            actual: out.len(),
        });
    }
    Ok(out)
}

/// One 8-byte asset-table entry: `{u32 offset from the table start, u32 size and flags}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub offset: u32,
    pub size_flags: u32,
}

impl Entry {
    /// Stored size in bytes (bits 0-27).
    pub fn size(self) -> usize {
        (self.size_flags & 0x0FFF_FFFF) as usize
    }

    /// Rzip-compressed: bits 28-30 equal 1 (the test in `func_1502B224`).
    pub fn is_compressed(self) -> bool {
        self.size_flags & 0x7000_0000 == 0x1000_0000
    }

    /// Bit 31 marks a table's last entry, which is still a real item.
    pub fn is_last(self) -> bool {
        self.size_flags >> 31 == 1
    }
}

/// An asset table at the start of `data`: `first offset / 8` entries whose offsets are relative to
/// `data`. Tables nest: an entry's payload can itself be a table.
#[derive(Debug, Clone)]
pub struct Table<'a> {
    pub data: &'a [u8],
    pub entries: Vec<Entry>,
}

impl<'a> Table<'a> {
    /// Parses the table and checks that every entry lies inside `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let first = be32(data, 0)? as usize;
        if first == 0 || !first.is_multiple_of(8) {
            return Err(Error::Malformed(
                "table's first offset is not a non-zero multiple of 8",
            ));
        }
        let entries = (0..first / 8)
            .map(|i| {
                let e = Entry {
                    offset: be32(data, i * 8)?,
                    size_flags: be32(data, i * 8 + 4)?,
                };
                get(data, e.offset as usize, e.size())?;
                Ok(e)
            })
            .collect::<Result<_>>()?;
        Ok(Self { data, entries })
    }

    fn entry(&self, index: usize) -> Result<Entry> {
        self.entries.get(index).copied().ok_or(Error::NoEntry {
            index,
            count: self.entries.len(),
        })
    }

    /// Stored bytes of entry `index` (still compressed if it is rzip).
    pub fn raw(&self, index: usize) -> Result<&'a [u8]> {
        let e = self.entry(index)?;
        get(self.data, e.offset as usize, e.size())
    }

    /// Payload of entry `index`, inflated if it is rzip.
    pub fn payload(&self, index: usize) -> Result<Cow<'a, [u8]>> {
        let raw = self.raw(index)?;
        Ok(if self.entry(index)?.is_compressed() {
            Cow::Owned(inflate_rzip(raw)?)
        } else {
            Cow::Borrowed(raw)
        })
    }
}

/// Follows `path` through nested tables from `data`; each step takes an entry's payload.
fn load<'a>(data: &'a [u8], path: &[usize]) -> Result<Cow<'a, [u8]>> {
    let mut cur = Cow::Borrowed(data);
    for &index in path {
        cur = match cur {
            Cow::Borrowed(b) => Table::parse(b)?.payload(index)?,
            Cow::Owned(v) => Cow::Owned(Table::parse(&v)?.payload(index)?.into_owned()),
        };
    }
    Ok(cur)
}

/// `.game` code: word 0 of the archive is the data's offset; words 1.. are code-block offsets XORed
/// with 0x8039CCCA, ended by a raw 0. Consecutive offsets bound each rzip block (one 4 KiB page).
pub fn game_code(archive: &[u8]) -> Result<Vec<u8>> {
    let mut starts = Vec::new();
    for i in 1.. {
        match be32(archive, i * 4)? {
            0 => break,
            word => starts.push((word ^ GAME_XOR) as usize),
        }
    }
    let mut code = Vec::new();
    for w in starts.windows(2) {
        let block = archive
            .get(w[0]..w[1])
            .ok_or(Error::Malformed(".game code block out of bounds"))?;
        code.extend(inflate_rzip(block)?);
    }
    Ok(code)
}

/// `.game` data (loaded at vram 0x80082B20): one rzip block at the offset in the archive's word 0.
pub fn game_data(archive: &[u8]) -> Result<Vec<u8>> {
    let offset = be32(archive, 0)? as usize;
    inflate_rzip(
        archive
            .get(offset..)
            .ok_or(Error::Malformed(".game data offset out of bounds"))?,
    )
}

/// The texture blob: consecutive rzip files with no table in the ROM. Their compressed sizes come
/// from a u16 table in `.game` data. Texture id = blob index.
#[derive(Debug, Clone)]
pub struct Textures<'a> {
    blob: &'a [u8],
    files: Vec<(usize, usize)>, // (offset in blob, compressed size)
}

impl<'a> Textures<'a> {
    /// `sizes` holds one big-endian u16 compressed size per file; size 0 is an empty slot.
    pub fn new(blob: &'a [u8], sizes: &[u8]) -> Result<Self> {
        let mut pos = 0;
        let files: Vec<_> = sizes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&s| {
                let file = (pos, u16::from_be_bytes(s) as usize);
                pos += file.1;
                file
            })
            .collect();
        get(blob, 0, pos)?;
        Ok(Self { blob, files })
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Decompressed bytes of texture `id`; empty for an empty slot.
    pub fn get(&self, id: usize) -> Result<Vec<u8>> {
        let &(offset, len) = self.files.get(id).ok_or(Error::NoEntry {
            index: id,
            count: self.files.len(),
        })?;
        if len == 0 {
            return Ok(Vec::new());
        }
        inflate_rzip(&self.blob[offset..offset + len])
    }
}

/// A validated Conker's Bad Fur Day (USA) ROM in big-endian .z64 order.
pub struct Rom(Vec<u8>);

impl Rom {
    /// Normalises the byte order (.z64, .v64 or .n64) and checks the SHA-1 against [`US_SHA1`].
    pub fn new(mut bytes: Vec<u8>) -> Result<Self> {
        normalize(&mut bytes)?;
        let sha1 = sha1_hex(&bytes);
        if sha1 != US_SHA1 {
            return Err(Error::WrongRom(sha1));
        }
        Ok(Self(bytes))
    }

    /// The whole ROM, big-endian.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// The top-level asset table at 0xAB1950 (offsets relative to it).
    pub fn asset_table(&self) -> Result<Table<'_>> {
        Table::parse(&self.0[ASSET_TABLE..])
    }

    /// The table of bank `bank` (0..[`BANK_COUNT`]).
    pub fn bank(&self, bank: usize) -> Result<Table<'_>> {
        Table::parse(self.asset_table()?.raw(bank)?)
    }

    /// The payload at `path` = `[bank, index, sub-index, ...]`; every step is inflated if it is rzip,
    /// so a compressed entry that holds a table can be walked into.
    pub fn asset(&self, path: &[usize]) -> Result<Cow<'_, [u8]>> {
        load(&self.0[ASSET_TABLE..], path)
    }

    /// Decompressed `.game` code (2,072,880 bytes).
    pub fn game_code(&self) -> Result<Vec<u8>> {
        game_code(&self.0[GAME_ARCHIVE])
    }

    /// Decompressed `.game` data (vram 0x80082B20).
    pub fn game_data(&self) -> Result<Vec<u8>> {
        game_data(&self.0[GAME_ARCHIVE])
    }

    /// The texture blob at 0x1A37E0, indexed by the size table in `.game` data.
    pub fn textures(&self) -> Result<Textures<'_>> {
        let data = self.game_data()?;
        let sizes = get(
            &data,
            TEXTURE_SIZES_VRAM - GAME_DATA_VRAM,
            TEXTURE_COUNT * 2,
        )?;
        Textures::new(&self.0[TEXTURE_BLOB], sizes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miniz_oxide::deflate::compress_to_vec;

    fn rzip(payload: &[u8]) -> Vec<u8> {
        let mut out = (payload.len() as u32).to_be_bytes().to_vec();
        out.extend(compress_to_vec(payload, 6));
        out
    }

    fn table(items: &[(&[u8], u32)]) -> Vec<u8> {
        let mut head = Vec::new();
        let mut body = Vec::new();
        let base = items.len() * 8;
        for (i, (bytes, flags)) in items.iter().enumerate() {
            let last = if i + 1 == items.len() { 0x8000_0000 } else { 0 };
            head.extend(((base + body.len()) as u32).to_be_bytes());
            head.extend((bytes.len() as u32 | flags << 28 | last).to_be_bytes());
            body.extend_from_slice(bytes);
        }
        head.extend(body);
        head
    }

    #[test]
    fn normalizes_byte_orders() {
        let z64 = [0x80, 0x37, 0x12, 0x40, 1, 2, 3, 4];
        for (mut rom, order) in [
            (z64.to_vec(), ByteOrder::Z64),
            (vec![0x37, 0x80, 0x40, 0x12, 2, 1, 4, 3], ByteOrder::V64),
            (vec![0x40, 0x12, 0x37, 0x80, 4, 3, 2, 1], ByteOrder::N64),
        ] {
            assert_eq!(normalize(&mut rom).unwrap(), order);
            assert_eq!(rom, z64);
        }
        assert!(matches!(
            normalize(&mut [1, 2, 3, 4]),
            Err(Error::UnknownByteOrder(_))
        ));
        assert!(matches!(
            normalize(&mut [0x80]),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn rejects_other_roms() {
        let rom = vec![0x80, 0x37, 0x12, 0x40, 0, 0, 0, 0];
        assert!(matches!(Rom::new(rom), Err(Error::WrongRom(_))));
    }

    #[test]
    fn inflates_rzip() {
        let payload = b"conker conker conker squirrel".repeat(10);
        let mut packed = rzip(&payload);
        assert_eq!(inflate_rzip(&packed).unwrap(), payload);

        packed[0] |= 0x80; // bit 31 of the size is ignored
        packed.extend([0xAA; 16]); // trailing padding is ignored
        assert_eq!(inflate_rzip(&packed).unwrap(), payload);

        packed[3] -= 1; // stream longer than the header says
        assert!(inflate_rzip(&packed).is_err());
        packed[3] += 2; // shorter
        assert!(matches!(
            inflate_rzip(&packed),
            Err(Error::SizeMismatch { .. })
        ));
        assert!(matches!(
            inflate_rzip(&[0, 0, 0, 4, 0xFF, 0xFF]),
            Err(Error::Inflate(_))
        ));
        assert!(matches!(
            inflate_rzip(&[0, 0]),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn parses_and_walks_nested_tables() {
        let inner = table(&[(b"abc", 0), (&rzip(b"deep"), 1)]);
        let outer = table(&[(b"", 0), (&inner, 0), (&rzip(&inner), 1)]);

        let t = Table::parse(&outer).unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(
            t.entries.iter().map(|e| e.is_last()).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(t.entries[2].size(), rzip(&inner).len());
        assert!(t.entries[2].is_compressed() && !t.entries[1].is_compressed());
        assert_eq!(t.payload(0).unwrap().len(), 0);
        assert_eq!(t.raw(1).unwrap(), &inner[..]);

        // Raw table inside a raw table stays borrowed; a compressed table is inflated, then walked.
        assert!(matches!(
            load(&outer, &[1, 0]).unwrap(),
            Cow::Borrowed(b"abc")
        ));
        assert_eq!(&*load(&outer, &[1, 1]).unwrap(), b"deep");
        assert_eq!(&*load(&outer, &[2, 0]).unwrap(), b"abc");
        assert_eq!(&*load(&outer, &[2, 1]).unwrap(), b"deep");
        assert!(matches!(
            load(&outer, &[3]),
            Err(Error::NoEntry { index: 3, count: 3 })
        ));
    }

    #[test]
    fn rejects_bad_tables() {
        assert!(matches!(
            Table::parse(&[0, 0, 0, 0]),
            Err(Error::Malformed(_))
        ));
        assert!(matches!(
            Table::parse(&[0, 0, 0, 12]),
            Err(Error::Malformed(_))
        ));
        // Entry claims 0x10 bytes past an 8-byte table.
        let t = [0, 0, 0, 8, 0x80, 0, 0, 0x10];
        assert!(matches!(Table::parse(&t), Err(Error::OutOfBounds { .. })));
        // Table header itself truncated.
        assert!(matches!(
            Table::parse(&[0, 0, 0, 16, 0, 0, 0, 0]),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn unpacks_game_archive() {
        let pages = [vec![1u8; 4096], vec![2u8; 300]];
        let data = b"game data".to_vec();
        let blocks: Vec<_> = pages.iter().map(|p| rzip(p)).collect();
        // Word 0, three XORed offsets (two block starts and the end of code), then the 0 terminator.
        let s0 = 20;
        let s1 = s0 + blocks[0].len();
        let s2 = s1 + blocks[1].len();
        let data_offset = s2 + 3; // padding before the data block
        let mut archive = Vec::new();
        for word in [
            data_offset as u32,
            s0 as u32 ^ GAME_XOR,
            s1 as u32 ^ GAME_XOR,
            s2 as u32 ^ GAME_XOR,
            0,
        ] {
            archive.extend(word.to_be_bytes());
        }
        archive.extend(blocks.concat());
        archive.extend([0; 3]);
        archive.extend(rzip(&data));
        archive.extend([0; 7]);

        assert_eq!(game_code(&archive).unwrap(), pages.concat());
        assert_eq!(game_data(&archive).unwrap(), data);
    }

    #[test]
    fn indexes_texture_blob() {
        let (a, b) = (rzip(&[7; 2080]), rzip(b"tex2"));
        let blob = [a.clone(), b.clone(), vec![0; 15]].concat();
        let sizes: Vec<u8> = [a.len() as u16, 0, b.len() as u16]
            .iter()
            .flat_map(|s| s.to_be_bytes())
            .collect();
        let tex = Textures::new(&blob, &sizes).unwrap();
        assert_eq!(tex.len(), 3);
        assert_eq!(tex.get(0).unwrap(), [7; 2080]);
        assert!(tex.get(1).unwrap().is_empty());
        assert_eq!(tex.get(2).unwrap(), b"tex2");
        assert!(matches!(tex.get(3), Err(Error::NoEntry { .. })));
        assert!(Textures::new(&blob[..a.len()], &sizes).is_err());
    }
}
