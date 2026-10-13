//! Characters, rigs and animation clips of Conker's Bad Fur Day (USA), as glTF.
//!
//! # The multiplayer character
//! In the oracle's Deathmatch (scenario 3, class 3, scene 0x2D) player 1 and the three bots are
//! all bank-01 model [`DEATHMATCH_PLAYER`] = 117 (DevOldSchool: "Tediz, variant 2"), animated by
//! Conker's clips, bank-02 entry 0. Verified in oracle RAM (`lab/work/mp/dm3/*.bin`,
//! `dm3one/dl_300.bin`, `dm3t/*.bin`; character pool at 0x800CC2D0, stride 0x32C):
//! - byte +4 = 117 for every participant, and the representation list at +0x2C4 (count +0x2C8) is
//!   `[117]`: one model, no level-of-detail swap;
//! - byte +6 = 0: `func_1505E0C4` loads clips from `[0x02, byte +6, segment]`. It is the route
//!   group's first member (`func_1503D5F0`; 117 is in group 0, whose first member is Conker), as
//!   in the no-bot dump `dm3solo` (scene 0x1D) where models 0, 181, 58 and 112 have byte +6 = 0,
//!   181, 58 and 112;
//! - u16 +0x84 is the logical animation playing: 122 or 89 for the idle player 1, 2, 124, 285,
//!   338, 339, 343, 344 for the bots.
//!
//! # Sources
//! Formats are ported from DevOldSchool/conkers-bfd-decomp (MIT, commit e2400b27dda6), see each
//! module. Display lists go through `extract-gbi`.

mod anim;
mod gltf;
mod held;
mod model;

pub use anim::{
    Channel, Clip, FPS, Frame, JOINT_SCALE, Layout, ROOT_SCALE, RootMotion, Route, SCALE_ONE,
    animation_entry, ff_duration, parse_routes, quaternion,
};
pub use gltf::Gltf;
pub use held::{Held, action_records, due, held_models, parse_attachment, parse_effect};
pub use model::{Joint, Model, bind_pivots, matrix_slot, parse_joints, parse_model};

use extract_core::{Rom, Table};

/// Bank-01 model of every Deathmatch participant in the oracle.
pub const DEATHMATCH_PLAYER: usize = 117;
/// Bank-01 model of the squirrels' soldier (the SHC): War: Total War' team 0, and the first character
/// of the multiplayer menu (V: the oracle's obj+4). It plays [`DEATHMATCH_PLAYER`]'s clips: its
/// byte +6 is 0 too, though 128 is in no route group (V; what sets it is unread).
pub const SQUIRREL_SOLDIER: usize = 128;
/// Bank holding the logical-id route tables, indexed like bank 02.
pub const ROUTE_BANK: usize = 0x0F;
/// Where `.game` data is loaded.
const GAME_DATA_VRAM: usize = 0x8008_2B20;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] extract_core::Error),
    #[error(transparent)]
    Gbi(#[from] extract_gbi::Error),
    #[error("malformed data: {0}")]
    Malformed(&'static str),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn get(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or(Error::Malformed("range outside its data"))
}

fn be32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        get(data, offset, 4)?.try_into().unwrap(),
    ))
}

/// A character model with every clip its animation entry holds.
#[derive(Debug)]
pub struct Character {
    /// Bank-01 entry.
    pub entry: usize,
    /// Bank-02 and bank-0x0F entry ([`animation_entry`]).
    pub anim_entry: usize,
    pub model: Model,
    /// Every pair of the animation entry, in pair order (including empty ones).
    pub clips: Vec<Clip>,
}

impl Character {
    pub fn load(rom: &Rom, entry: usize) -> Result<Self> {
        let game_data = rom.game_data()?;
        let model = parse_model(&rom.asset(&[1, entry])?, &game_data)?;
        let anim_entry = animation_entry(&game_data, entry)?;
        let clips = load_clips(rom, &game_data, anim_entry)?;
        Ok(Self {
            entry,
            anim_entry,
            model,
            clips,
        })
    }

    /// Clips that drive every channel slot of this rig and have frames: the glTF animations.
    pub fn playable_clips(&self) -> impl Iterator<Item = &Clip> {
        let slots = self.model.anim_slots();
        self.clips
            .iter()
            .filter(move |c| !c.frames.is_empty() && c.joints >= slots)
    }
}

/// Every (descriptor, stream) pair of bank-02 entry `anim_entry`, with the logical ids routed to
/// each pair by the bank-0x0F table of its group ([`animation_entry`], as `func_1503D660` loads
/// it). `game_data` is the decompressed `.game` data.
pub fn load_clips(rom: &Rom, game_data: &[u8], anim_entry: usize) -> Result<Vec<Clip>> {
    let anims = rom.asset(&[2, anim_entry])?;
    if anims.is_empty() {
        return Ok(Vec::new());
    }
    let route_entry = animation_entry(game_data, anim_entry)?;
    let routes = parse_routes(&rom.asset(&[ROUTE_BANK, route_entry])?)?;
    let table = Table::parse(&anims)?;
    if !table.entries.len().is_multiple_of(2) {
        return Err(Error::Malformed("odd animation segment count"));
    }
    (0..table.entries.len() / 2)
        .map(|pair| {
            let logical: Vec<u16> = (0..routes.len() as u16)
                .filter(|&id| routes[id as usize].pair == pair)
                .collect();
            let (descriptor, stream) = (table.payload(2 * pair)?, table.payload(2 * pair + 1)?);
            let ff = if descriptor.get(3) == Some(&0xFF) && !stream.is_empty() {
                ff_duration(game_data, anim_entry, &logical)?
            } else {
                255
            };
            Clip::decode(pair, logical, &descriptor, &stream, ff)
        })
        .collect()
}
