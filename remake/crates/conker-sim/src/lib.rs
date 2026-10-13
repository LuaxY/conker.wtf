//! Shared fixed-tick multiplayer simulation, in the original game's units (see [`movement`]).
//!
//! The server and the clients run the same [`movement::step`] on the same [`collision::CollisionWorld`]
//! (each built from its own copy of the ROM). Math is limited to `+ - * / sqrt` so native (server)
//! and wasm (client) produce bit-identical results.

pub mod collision;
pub mod follow;
pub mod movement;
#[cfg(feature = "net")]
pub mod net;
pub mod root;
pub mod tank;
pub mod trig;

/// Simulation rate. Conker's game logic and animation clips run at 30 Hz (one frame = 2 VIs).
pub const TICK_HZ: f64 = 30.0;
