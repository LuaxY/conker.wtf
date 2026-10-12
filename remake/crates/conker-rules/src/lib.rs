//! Match rules not tied to movement, ported from the original: the game's RNG, the multiplayer
//! spawn picker, the collision surface words, and Deathmatch combat (weapons, pickups,
//! projectiles, damage, death, respawn, score, timer: [`game::Match`]). No engine dependency, no
//! I/O: the same code runs on the server and in the wasm client.
//!
//! Tags in the docs: V matched on oracle traces, C read in the recompiled code, I inferred.

pub mod colors;
pub mod damage;
pub mod fidget;
pub mod flame;
pub mod game;
pub mod gas;
#[cfg(feature = "net")]
pub mod net;
pub mod projectile;
pub mod rng;
pub mod spawn;
pub mod surface;
pub mod weapon;

pub use game::{Arena, Event, Fighter, Match, Phase, Settings};
pub use rng::Rng;
pub use spawn::{
    DEATHMATCH_MASKS, DEATHMATCH_START, Placement, Spawn, TOTAL_WAR_MASKS, TOTAL_WAR_START, place,
    respawn,
};
pub use surface::{Surface, sink_target, step_sink};
