//! The end of a match: the results over the frozen level ([`crate::hud`] draws them; in War: Total
//! War after the winner's outro, [`crate::outro`]), and their
//! menu. RESTART votes for the next match (conker-sim's [`Restart`]: the server starts it once every
//! human voted, or on its own after its countdown, past RESTART's 17-frame fade: phase 3,
//! [`crate::hud::restart_fade`], where the menu, shown no more, ignores the pad); QUIT leaves: the
//! browser goes back to the landing page (`index.html`), the native client exits.

use bevy::prelude::*;
use conker_sim::movement::{BUTTON_A, Input};
use conker_sim::net::{JoinChannel, MatchView, Restart};
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;

/// The stick pushed left or right past this (N64 units) toggles the menu's choice (V: the oracle's
/// STATS, `0x8008FE2C`).
const PUSH: i8 = 30;

/// A choice of the results' menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Restart,
    Quit,
}

/// The results' menu: the lit choice (0 RESTART, lit first; 1 QUIT), whether this player voted to
/// restart, and the last frame's pad (a push or press acts on its first frame).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Menu {
    pub choice: usize,
    pub voted: bool,
    last: (u16, i8),
}

impl Menu {
    /// One frame of the pad (`buttons`, the stick's x): a push left or right toggles the choice,
    /// A chooses (RESTART once).
    pub fn step(&mut self, buttons: u16, stick_x: i8) -> Option<Pick> {
        let (last, last_x) = std::mem::replace(&mut self.last, (buttons, stick_x));
        let pushed = |x: i8| x.unsigned_abs() > PUSH as u8;
        if pushed(stick_x) && !pushed(last_x) {
            self.choice ^= 1;
        }
        if buttons & BUTTON_A == 0 || last & BUTTON_A != 0 {
            return None;
        }
        match self.choice {
            0 if self.voted => None,
            0 => {
                self.voted = true;
                Some(Pick::Restart)
            }
            _ => Some(Pick::Quit),
        }
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<Menu>();
    app.add_systems(FixedUpdate, menu.run_if(not(is_in_rollback)));
}

/// Over (past a Total War outro), the local pad drives the menu; before, the menu waits (fresh, but
/// for the pad: a button held into the results is no press).
fn menu(
    game: Query<&MatchView>,
    (playing, outros): (Res<crate::outro::Playing>, Res<crate::outro::Outros>),
    own: Query<&ActionState<Input>, With<InputMarker<Input>>>,
    mut menu: ResMut<Menu>,
    mut senders: Query<&mut MessageSender<Restart>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(input) = own.single() else {
        return;
    };
    let Input {
        buttons, stick_x, ..
    } = input.0;
    if !game
        .iter()
        .any(|m| crate::outro::results(m, &playing, &outros))
    {
        *menu = Menu::default();
        menu.last = (buttons, stick_x);
        return;
    }
    match menu.step(buttons, stick_x) {
        Some(Pick::Restart) => {
            info!("results: RESTART");
            for mut sender in &mut senders {
                sender.send::<JoinChannel>(Restart);
            }
        }
        Some(Pick::Quit) => {
            info!("results: QUIT");
            #[cfg(target_family = "wasm")]
            if let Some(w) = web_sys::window() {
                let _ = w.location().set_href("index.html");
                return;
            }
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_moves_on_pushes_and_picks_on_presses() {
        // A held from the match is no press; let go, pressed: RESTART, once.
        let mut m = Menu {
            last: (BUTTON_A, 0),
            ..Menu::default()
        };
        assert_eq!(m.step(BUTTON_A, 0), None);
        assert_eq!(m.step(0, 0), None);
        assert_eq!(m.step(BUTTON_A, 0), Some(Pick::Restart));
        assert_eq!((m.step(0, 0), m.step(BUTTON_A, 0)), (None, None));
        assert!(m.voted);
        // A push toggles to QUIT (held: no repeat), the next push back to RESTART.
        assert_eq!((m.step(0, 80), m.step(0, 80), m.choice), (None, None, 1));
        assert_eq!(m.step(BUTTON_A, 0), Some(Pick::Quit));
        m.step(0, -31);
        assert_eq!(m.choice, 0);
        m.step(0, 0);
        m.step(0, 30);
        assert_eq!(m.choice, 0);
    }
}
