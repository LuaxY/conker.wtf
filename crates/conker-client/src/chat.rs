//! Text chat ([`Chat`], [`Said`]): the ChatAll action (T) opens the page's chat box to everyone,
//! ChatTeam (Y) to the team (free for all, everyone), while playing, over the Esc menu and over
//! the results, but not while a rebind waits for its key nor on the lost connection's page. The
//! keys are read from the devices themselves: the menu holds [`crate::bindings::Controls`]'.
//! The page's `chat.js` has the box: Enter sends its line through [`done`] (the page's
//! `chat_done`), Esc or a click elsewhere closes it, and either gives the canvas the keys back.
//! While it is open the player stands ([`open`]); the pointer lock it lets go of opens no menu,
//! and its Enter takes it back. [`hear`] gives each relayed line to the page's list. Natively
//! there is no box: the lines go to the log.

use bevy::prelude::*;
use conker_sim::net::{Chat, ChatChannel, Said};
use lightyear::prelude::{Client, MessageReceiver, MessageSender};
use std::sync::Mutex;

/// The open box: to the team only, and whether it let go of the pointer lock (Enter takes it
/// back).
static OPEN: Mutex<Option<(bool, bool)>> = Mutex::new(None);
/// The lines the page sent since the last frame ([`done`]).
static OUT: Mutex<Vec<Chat>> = Mutex::new(Vec::new());

/// The chat box is open: it has the keys.
pub fn open() -> bool {
    OPEN.lock().is_ok_and(|o| o.is_some())
}

/// The page's box closed: on Enter with its line (sent with the next frame) and the pointer lock
/// back if the box let go of it; on Esc or a click elsewhere with none.
pub fn done(line: Option<String>) {
    let Some((team, relock)) = OPEN.lock().ok().and_then(|mut o| o.take()) else {
        return;
    };
    let Some(text) = line else {
        return;
    };
    if relock {
        crate::pointer::lock();
    }
    if let Ok(mut out) = OUT.lock()
        && !text.trim().is_empty()
    {
        out.push(Chat { team, text });
    }
}

/// A chat key opens the box (the page's `open(team)`; it lets go of the pointer lock). Before the
/// menu reads the keys: a rebind's capture still waits for this frame's.
#[cfg(target_family = "wasm")]
fn keys(
    bindings: Res<crate::bindings::Bindings>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    menu: Res<crate::menu::Menu>,
    game: Query<(), With<conker_sim::net::MatchView>>,
) {
    use crate::bindings::{Action, Devices};
    use crate::menu::Page;
    let d = Devices {
        keys: Some(&keys),
        mouse: Some(&mouse),
        ..Devices::default()
    };
    let team = match () {
        _ if bindings.just_pressed(Action::ChatTeam, &d) => true,
        _ if bindings.just_pressed(Action::ChatAll, &d) => false,
        _ => return,
    };
    let busy = match &menu.page {
        Page::Lost(_) => true,
        Page::Settings(sheet) => sheet.capture.is_some(),
        _ => false,
    };
    if busy || game.is_empty() || open() {
        return;
    }
    if let Ok(mut o) = OPEN.lock() {
        *o = Some((team, crate::pointer::locked()));
    }
    crate::web::page("conkerChat", "open", &[team.into()]);
}

/// The page's lines since the last frame, to the server.
fn send(mut clients: Query<&mut MessageSender<Chat>, With<Client>>) {
    let lines = OUT.lock().map(|mut o| std::mem::take(&mut *o));
    let (Ok(lines), Ok(mut sender)) = (lines, clients.single_mut()) else {
        return;
    };
    for line in lines {
        sender.send::<ChatChannel>(line);
    }
}

/// The relayed lines: to the page's list, and the log. Every frame (lightyear drops what a frame
/// leaves unread).
fn hear(mut clients: Query<&mut MessageReceiver<Said>, With<Client>>) {
    for mut said in &mut clients {
        for s in said.receive() {
            let to = if s.team { " (team)" } else { "" };
            info!("chat{to}: {}: {}", s.name, s.text);
            #[cfg(target_family = "wasm")]
            crate::web::page(
                "conkerChat",
                "show",
                &[s.name.into(), s.team.into(), s.text.into()],
            );
        }
    }
}

/// Text chat (windowed clients).
pub fn build(app: &mut App) {
    app.add_systems(Update, (send, hear));
    #[cfg(target_family = "wasm")]
    app.add_systems(
        PreUpdate,
        keys.after(bevy::input::InputSystems)
            .before(crate::menu::run),
    );
}
