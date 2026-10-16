//! The fighter on the network (feature `net`): conker-server replicates every player's
//! [`Fighter`], and the owning client predicts its own with [`crate::Match::step_player`] (its
//! knock-back, death and respawn included, every grenade in flight: its own and the others',
//! scene 0x34's plates and traps, and Total War's doors and covers).

use bevy::prelude::*;
use conker_sim::movement::PlayerState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::projectile::{self, Grenade, Shot};
use crate::{Fighter, Match, gas, traps};

/// A player's fighter, its slot in the server's match and every grenade in flight, whoever threw
/// it (with its birth frame: grenades push players and blow them up, so each client flies them all
/// in its own frames and predicts the blast of another player's grenade). The body travels in
/// conker-sim's `Player` (predicted and interpolated); here it is a placeholder, so
/// [`Fighting::fighter`] takes it back. Scene 0x34's plates and traps and Total War's sliding doors
/// and covers (None elsewhere) ride along: they carry, stop and kill whoever stands on, by or in
/// them, so each client steps them in its own frames.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Fighting {
    pub slot: u16,
    pub state: Fighter,
    pub grenades: Vec<(u32, Grenade)>,
    pub traps: Option<traps::State>,
    pub sliding: Option<[gas::Prop; 8]>,
    /// The crossbow bolts sitting where they were fired (birth frame, position): until they first
    /// move they push everyone, their shooter included.
    pub bolts: Vec<(u32, [f32; 3])>,
}

impl Fighting {
    pub fn new(slot: usize, f: &Fighter) -> Self {
        Self {
            slot: slot as u16,
            state: Fighter {
                body: PlayerState::spawn([0.0; 3], 0),
                ..*f
            },
            grenades: Vec::new(),
            traps: None,
            sliding: None,
            bolts: Vec::new(),
        }
    }

    /// Player `slot` of `m`, with the match's grenades, traps and sliding props.
    pub fn of(m: &Match, slot: usize) -> Self {
        Self {
            grenades: m.grenades.clone(),
            traps: (!m.traps.list.is_empty()).then(|| m.traps.state()),
            sliding: m.settings.total_war.then(|| m.gas.sliding()),
            bolts: (m.shots.iter())
                .filter(|(born, s)| {
                    s.step == projectile::BOLT_STEP && m.tick <= m.first_move(*born)
                })
                .map(|&(born, s)| (born, s.pos))
                .collect(),
            ..Self::new(slot, &m.players[slot])
        }
    }

    /// The fighter with its body.
    pub fn fighter(&self, body: PlayerState) -> Fighter {
        Fighter { body, ..self.state }
    }

    /// [`Self::bolts`] as the match's shots: only their push is the client's (their flight and
    /// hits are the server's), so they carry nothing else.
    pub fn shots(&self) -> Vec<(u32, Shot)> {
        let bolt = |pos| Shot {
            pos,
            dir: [0.0; 3],
            wall: 0.0,
            owner: usize::from(self.slot),
            kind: crate::damage::SHOT_BODY,
            step: projectile::BOLT_STEP,
        };
        (self.bolts.iter())
            .map(|&(born, pos)| (born, bolt(pos)))
            .collect()
    }
}

/// Registers [`Fighting`]: replicated, predicted by its owner. Server and client call it right
/// after conker-sim's `ProtocolPlugin` (the registration order is the protocol).
pub fn register(app: &mut App) {
    app.component::<Fighting>().replicate().predict();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spawn::Placement;

    #[test]
    fn the_body_travels_apart() {
        let place = Placement {
            index: 0,
            pos: [1.0, 2.0, 3.0],
            facing: 7,
            camera: [0.0; 3],
        };
        let f = Fighter::new(&place, 3, 9);
        let net = Fighting::new(2, &f);
        assert_eq!((net.slot, net.state.body.pos), (2, [0.0; 3]));
        assert_eq!(net.fighter(f.body), f);
    }
}
