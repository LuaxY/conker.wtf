//! What a character holds: Deathmatch's weapons and the grenade are bank-09 attachment models that
//! the character's own animation events put in its hand and take away.
//!
//! Read in the code (C) and checked on the oracle (V: twin guns, flamethrower, grenade):
//! - A route's events (`func_1506AD30`; route +4 is their offset from the table's payload +0x10):
//!   12-byte records, the clip time (f32 ticks), the opcode in the second word's low byte and an
//!   argument, until a time of 900 or more. Opcode 105 creates the attachments of action `argument`
//!   (`func_15072DD8`), 106 removes them (`func_15072E38`), and 41 deletes the actor's first child
//!   object (`func_1506DBD4`: `func_15072208(actor, 0)`, then `func_15054A5C`; the holster 0xC6's at
//!   t14, the only op-41 route in the Tedi's table; V `lab/research/mp-original/held-leftovers.md`
//!   hol: the crossbow gone at read 393). An event fires in the frame the clip's
//!   clock reaches its time (V: f32 clock, twin guns' draw t 15 at 15.4, holster t 14 at 15.4 after
//!   13.999998, flamethrower t 9 at 9.0). The dispatcher stops at the first record not yet due, so
//!   a record fires at the latest time of those before it ([`due`]; V: the mask's take-off 0x14E,
//!   whose t 59 record holds back the t 24 and t 55 ones after it,
//!   `lab/research/mp-original/total-war-effects.md` §4).
//! - An action (`func_15083568`): a header at `0x80086CC4 + 8·(action − 1)` (records pointer, u8
//!   count), then 16-byte records: byte 3 the kind (1 and 2 make a bank-09 attachment,
//!   `func_15030AF4`; 0 a child object of class byte 0, the bank-01 model of that entry,
//!   `func_15083AC8`: the crossbow's 0x6E), byte 0 the entry, byte 1 the parent's matrix slot
//!   (`func_15030AF4` → +2).
//! - The renderer (`func_150311C4`, `func_15031070`) binds segment 3 to the parent's matrix in that
//!   slot (obj+0x1D4 + slot·0x40) and the model's lists load it as their only matrix: the model is
//!   drawn in the joint's frame, with no offset, rotation or scale of its own (V: the RAM's display
//!   list for twin guns 26/27 on slots 4/9, flamethrower 48 on 9, grenade 75 on 4).

use extract_core::Rom;
use extract_gbi::{Ir, walk};

use crate::anim::data_at;
use crate::{ROUTE_BANK, Result, animation_entry, be32, parse_routes};

/// The action headers (`func_15083568`).
const ACTIONS: usize = 0x8008_6CC4;
/// Event opcodes: create and remove an action's attachments, and delete the first child object.
const CREATE: u32 = 105;
const REMOVE: u32 = 106;
const REMOVE_CHILD: u32 = 41;
/// Where an attachment's vertices start (segment 1).
const VERTICES: usize = 0x18;
/// Segment 8 while an attachment draws opaque: the renderer's render-state lists in `.game` data
/// (`func_150311C4`; 0x80082FC0 while it fades). V: RAM, EF + DF lists at +0x40, +0x50, +0x100.
pub(crate) const RENDER_STATES: usize = 0x8008_3140;

/// A model a character holds: drawn with the character's joint matrix while the last of its events
/// that fired created it.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    /// Bank-09 entry, or bank-01 entry of a child object.
    pub entry: usize,
    /// A child object (kind 0): a bank-01 model with its own rig and clips, which the game poses from
    /// its parent's clip (`func_15054A94`, `lab/research/mp-original/held-leftovers.md` §1).
    pub object: bool,
    /// The character's matrix slot (joint) it is drawn with.
    pub joint: u8,
    /// (logical animation id, clip time in ticks) of every event that creates it, and removes it
    /// (a child object's: its action's op 106 and every op 41).
    pub create: Vec<(u16, f32)>,
    pub remove: Vec<(u16, f32)>,
}

/// The models that the clips `clips` (logical animation ids) of bank-01 character `entry` create,
/// with every create and remove event the character's clips have for them, by action.
pub fn held_models(rom: &Rom, entry: usize, clips: &[u16]) -> Result<Vec<Held>> {
    let game_data = rom.game_data()?;
    let anims = animation_entry(&game_data, entry)?;
    let table = rom.asset(&[ROUTE_BANK, animation_entry(&game_data, anims)?])?;
    // (logical id, time, creates, action); op 41's action is None: any child object's.
    let mut events = Vec::new();
    for (id, route) in parse_routes(&table)?.iter().enumerate() {
        if route.aux_offset == 0 {
            continue;
        }
        let (mut at, mut last) = (route.aux_offset as usize + 0x10, 0.0);
        loop {
            let time = f32::from_bits(be32(&table, at)?);
            if time >= 900.0 {
                break;
            }
            let time = due(&mut last, time);
            let op = be32(&table, at + 4)? & 0xFF;
            if op == CREATE || op == REMOVE {
                events.push((id as u16, time, op == CREATE, Some(be32(&table, at + 8)?)));
            } else if op == REMOVE_CHILD {
                events.push((id as u16, time, false, None));
            }
            at += 12;
        }
    }
    let mut actions: Vec<u32> = events
        .iter()
        .filter(|e| e.2 && clips.contains(&e.0))
        .filter_map(|e| e.3)
        .collect();
    actions.sort_unstable();
    actions.dedup();
    let mut held = Vec::new();
    for action in actions {
        for r in action_records(&game_data, action)? {
            let object = r[3] == 0;
            let of = |creates: bool| {
                let e = (events.iter())
                    .filter(|e| e.2 == creates)
                    .filter(|e| e.3 == Some(action) || (object && e.3.is_none()));
                e.map(|e| (e.0, e.1)).collect()
            };
            held.push(Held {
                entry: r[0] as usize,
                object,
                joint: r[1],
                create: of(true),
                remove: of(false),
            });
        }
    }
    Ok(held)
}

/// The 16-byte records of action `action` (`func_15083568`: byte 0 the entry, 1 the matrix slot,
/// 3 the kind), whatever creates it (a clip's event, or a script: Race's board and helmets).
pub fn action_records(game_data: &[u8], action: u32) -> Result<Vec<[u8; 16]>> {
    let header = data_at(game_data, ACTIONS + 8 * (action as usize - 1), 8)?;
    let records = data_at(
        game_data,
        be32(header, 0)? as usize,
        16 * header[4] as usize,
    )?;
    Ok(records.as_chunks::<16>().0.to_vec())
}

/// When a route's next event fires: at its time, or at the latest time of the records before it
/// (`last`, which it updates), since `func_1506AD30` stops at the first record not yet due (C).
pub fn due(last: &mut f32, time: f32) -> f32 {
    *last = last.max(time);
    *last
}

/// The part lists of a bank-09 attachment model (`func_1502FE10`): header words 0/1 are the part
/// table's offset and size (vertices run from +0x18 up to it), 2/3 a joint table, 4/5 the CBFD
/// normals. Segment 0 addresses the model, segment 1 its vertices, segment 8 the renderer's render
/// states in `game_data` (the decompressed `.game` data); each part is walked on its own.
pub fn parse_attachment(data: &[u8], game_data: &[u8]) -> Result<Vec<Ir>> {
    let (table, size) = (be32(data, 0)? as usize, be32(data, 4)? as usize);
    (0..size)
        .step_by(4)
        .map(|k| walk_model(data, game_data, be32(data, table + k)?))
        .collect()
}

/// A bank-09 effect model with one display list (the detonation's ring 0x157, the spark 0x132):
/// header word 0 is the list's offset, word 1 its size (I, from their bytes); addresses as
/// [`parse_attachment`]'s.
pub fn parse_effect(data: &[u8], game_data: &[u8]) -> Result<Ir> {
    walk_model(data, game_data, be32(data, 0)?)
}

fn walk_model(data: &[u8], game_data: &[u8], list: u32) -> Result<Ir> {
    let states = game_data.get(RENDER_STATES - crate::GAME_DATA_VRAM..);
    Ok(walk(list, |a| {
        let off = (a & 0xFF_FFFF) as usize;
        match a >> 24 {
            0 => data.get(off..),
            1 => data.get(VERTICES + off..),
            8 => states?.get(off..),
            _ => None,
        }
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_vertices_follow_the_header() {
        // Header (part table at 0x28, one part), one vertex at 0x18, the part's list at 0x30:
        // load slot 0 of segment 3, one vertex from segment 1, end.
        let mut m = vec![0u8; 0x48];
        for (k, w) in [0x28u32, 4, 0, 0, 0, 0x8000_0000].iter().enumerate() {
            m[4 * k..4 * k + 4].copy_from_slice(&w.to_be_bytes());
        }
        m[0x18..0x1A].copy_from_slice(&(-7i16).to_be_bytes());
        m[0x28..0x2C].copy_from_slice(&0x30u32.to_be_bytes());
        let list: [(u32, u32); 3] = [
            (0xDA38_0003, 0x0300_0000),
            (0x0100_1002, 0x0100_0000),
            (0xDF00_0000, 0),
        ];
        for (i, (w0, w1)) in list.iter().enumerate() {
            m[0x30 + 8 * i..0x34 + 8 * i].copy_from_slice(&w0.to_be_bytes());
            m[0x34 + 8 * i..0x38 + 8 * i].copy_from_slice(&w1.to_be_bytes());
        }
        let parts = parse_attachment(&m, &[]).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].vertices[0].position[0], -7);
        assert_eq!(parts[0].vertices[0].matrix, Some(0x0300_0000));
    }
}
