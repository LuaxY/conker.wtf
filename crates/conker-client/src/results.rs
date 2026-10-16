//! The end of a match: the results over the frozen level ([`crate::hud`] draws them; in War: Total
//! War after the winner's outro, [`crate::outro`]), and their
//! menu. RESTART votes for the next match (conker-sim's [`Restart`]: the server starts it once every
//! human voted, or on its own after its countdown, past RESTART's 17-frame fade: phase 3,
//! [`crate::hud::restart_fade`], where the menu, shown no more, ignores the pad); QUIT leaves: the
//! browser goes back to the landing page, the native client exits (`connection::leave`). The stick
//! pushed left or right, A or Jump; the pointer over a button lights it, a click on it or Enter
//! chooses ([`button_at`], [`Menu::press`]: [`crate::menu`] reads the mouse and keys, the pointer
//! freed meanwhile, [`crate::pointer::Free`]).

use bevy::prelude::*;
use conker_assets::hud::Hud;
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
/// restart, a click or Enter waiting for the next tick (it chooses as A does), and the last frame's
/// pad (a push or press acts on its first frame).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Menu {
    pub choice: usize,
    pub voted: bool,
    pub press: bool,
    last: (u16, i8),
}

/// The results' button under `at` (window pixels in a `window`-sized target): 0 RESTART, 1 QUIT.
pub fn button_at(hud: &Hud, window: Vec2, at: Vec2) -> Option<usize> {
    let r = &hud.end.as_ref()?.results;
    [&r.restart, &r.quit].iter().position(|b| {
        let rect = b.rect.map(f32::from);
        let [x, y, w, h] = crate::hud::place(hud.frame, b.anchor, rect, window);
        (x..x + w).contains(&at.x) && (y..y + h).contains(&at.y)
    })
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
    // The modern controls' Jump confirms as A does, and so does a click or Enter.
    let jump = input.0.actions & conker_sim::movement::action::JUMP != 0;
    let buttons = buttons | if jump || menu.press { BUTTON_A } else { 0 };
    menu.press = false;
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

    /// The pointer finds RESTART and QUIT where they are drawn: in a 1280×720 window the 292×216
    /// frame scales by 10/3, centred (RESTART at frame x 51-115, QUIT 181-245, y 164-196).
    #[test]
    fn the_pointer_finds_the_buttons() {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into());
        let read = |p: &str| std::fs::read(std::path::Path::new(&dir).join(p));
        let (Ok(mut hud), Ok(end)) = (Hud::load(read), conker_assets::hud::End::load(read)) else {
            eprintln!("no pack HUD in {dir}: skipping");
            return;
        };
        hud.end = Some(end);
        let at = |x, y| button_at(&hud, Vec2::new(1280.0, 720.0), Vec2::new(x, y));
        assert_eq!((at(430.0, 600.0), at(860.0, 600.0)), (Some(0), Some(1)));
        assert_eq!((at(640.0, 600.0), at(430.0, 400.0)), (None, None));
    }
}
