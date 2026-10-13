//! The team screen (the team modes: [`MatchView::teams`]), over the match as the original's before
//! one: Tab or a gamepad's Select opens and closes it. While it is open the player stands
//! (`buffer_input` sends no pad): the stick pushed left or right moves it to the team before or
//! after (conker-sim's [`Teams::Switch`], wrapping: it respawns there; the server refuses a team
//! with no room for a human), C-left / C-right (Q / E) move the BALANCE (2 teams; the first
//! human's: bots to team 0, or to team 1), A or B closes it. `team=T` asks for team T once the
//! player is in (the lobby's pick, headless tests).

use crate::{BUTTON_B, C_LEFT, C_RIGHT, Config, FollowCamera, device_input};
use bevy::prelude::*;
use conker_sim::movement::BUTTON_A;
use conker_sim::net::{Combat, JoinChannel, MatchView, PlayerId, Teams};
use lightyear::prelude::*;

/// The stick pushed past this (N64 units) moves the team, as the results' menu.
const PUSH: i8 = 30;

/// Whether the screen is open, `team=` asked for, and the last frame's pad (a push or press acts on
/// its first frame).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Screen {
    pub open: bool,
    asked: bool,
    last: (u16, i8, bool),
}

impl Screen {
    /// One frame of the pad (`buttons`, the stick's x, `toggle`: Tab or Select held), the player on
    /// team `team` of `teams` with the BALANCE at `balance`: what to ask the server.
    pub fn step(
        &mut self,
        (buttons, stick_x, toggle): (u16, i8, bool),
        team: u8,
        teams: u8,
        balance: i8,
    ) -> Option<Teams> {
        let (last, last_x, last_toggle) =
            std::mem::replace(&mut self.last, (buttons, stick_x, toggle));
        let pressed = |bit: u16| buttons & bit != 0 && last & bit == 0;
        let pushed = |x: i8| x.unsigned_abs() > PUSH as u8;
        if toggle && !last_toggle {
            self.open = !self.open;
            return None;
        }
        if !self.open || teams == 0 {
            return None;
        }
        if pressed(BUTTON_A) || pressed(BUTTON_B) {
            self.open = false;
            return None;
        }
        if pushed(stick_x) && !pushed(last_x) {
            let next = if stick_x > 0 {
                team + 1
            } else {
                team + teams - 1
            };
            return Some(Teams::Switch(next % teams));
        }
        match () {
            _ if teams == 2 && pressed(C_LEFT) => Some(Teams::Balance(balance.saturating_add(1))),
            _ if teams == 2 && pressed(C_RIGHT) => Some(Teams::Balance(balance.saturating_sub(1))),
            _ => None,
        }
    }
}

/// The teams' names as the original's rows show them.
fn names(m: &MatchView) -> &'static [&'static str] {
    match () {
        _ if m.heist => &["REDS", "BLUES", "GREENS", "YELLOWS"],
        _ if m.beach => &["FRENCHYS", "TEDIZ"],
        _ if m.raptor => &["UGAS", "RAPTORS"],
        _ => &["SQUIRRELS", "TEDIZ"],
    }
}

/// The screen's text: each team and its players (`players`: slot, team, a bot), this one `YOU`.
pub fn text(m: &MatchView, own: &Combat, players: &[(u16, u8, bool)]) -> String {
    let mut out = String::from("TEAMS (Tab: close)\n");
    for (t, name) in names(m).iter().enumerate().take(usize::from(m.teams)) {
        let mut on: Vec<_> = players.iter().filter(|p| usize::from(p.1) == t).collect();
        on.sort_by_key(|p| p.0);
        let who: Vec<String> = (on.iter())
            .map(|&&(slot, _, bot)| match () {
                _ if slot == own.slot => "YOU".into(),
                _ if bot => "BOT".into(),
                _ => format!("P{}", slot + 1),
            })
            .collect();
        let mark = if usize::from(own.team) == t { '>' } else { ' ' };
        out += &format!("{mark}{name:<9} {}\n", who.join(" "));
    }
    if m.teams == 2 {
        out += &format!("BALANCE {:+} (Q/E)\n", m.balance);
    }
    out + "stick: change team, A: close"
}

pub fn build(app: &mut App, draw: bool) {
    app.init_resource::<Screen>();
    app.add_systems(Update, ask);
    if draw {
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(Startup, |mut commands: Commands| {
            commands.spawn((
                TeamText,
                Text::new(""),
                TextFont {
                    font_size: bevy::text::FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(30.0),
                    left: Val::Percent(30.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    ..default()
                },
            ));
        });
        app.add_systems(Update, show.after(ask));
    }
}

/// The screen's text node (native).
#[cfg(not(target_family = "wasm"))]
#[derive(Component)]
struct TeamText;

/// The local pad on the screen, and `team=`'s ask once the player is in.
#[allow(clippy::too_many_arguments)]
fn ask(
    cfg: Res<Config>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    gamepads: Query<&Gamepad>,
    windows: Query<&Window>,
    game: Query<&MatchView>,
    own: Query<&Combat, With<FollowCamera>>,
    mut screen: ResMut<Screen>,
    mut senders: Query<&mut MessageSender<Teams>>,
) {
    let (Ok(m), Ok(c)) = (game.single(), own.single()) else {
        return;
    };
    let mut asks = Vec::new();
    if let Some(t) = cfg.team.filter(|_| !screen.asked && m.teams > 0) {
        screen.asked = true;
        asks.push(Teams::Switch(t));
    }
    if m.teams == 0 {
        screen.open = false;
    } else if cfg.script.is_none() {
        let pad = (gamepads.iter().next()).filter(|_| windows.iter().any(|w| w.focused));
        let (buttons, stick_x, _) = device_input(keys.as_deref(), mouse.as_deref(), pad);
        let toggle = keys.is_some_and(|k| k.pressed(KeyCode::Tab))
            || pad.is_some_and(|p| p.pressed(GamepadButton::Select));
        asks.extend(screen.step((buttons, stick_x, toggle), c.team, m.teams, m.balance));
    }
    for ask in asks {
        info!("teams: {ask:?}");
        for mut sender in &mut senders {
            sender.send::<JoinChannel>(ask);
        }
    }
}

/// The open screen: a text node natively, the page's `#teams` element in the browser.
#[allow(clippy::type_complexity)]
fn show(
    screen: Res<Screen>,
    game: Query<&MatchView>,
    own: Query<&Combat, With<FollowCamera>>,
    roster: Query<(&PlayerId, &Combat), Or<(With<Predicted>, With<Interpolated>)>>,
    #[cfg(not(target_family = "wasm"))] mut node: Single<
        (&mut Text, &mut Visibility),
        With<TeamText>,
    >,
    #[cfg(target_family = "wasm")] mut shown: Local<String>,
) {
    let text = match (game.single(), own.single()) {
        (Ok(m), Ok(c)) if screen.open && m.teams > 0 => {
            let players: Vec<(u16, u8, bool)> = (roster.iter())
                .map(|(id, p)| (p.slot, p.team, matches!(id.0, PeerId::Local(_))))
                .collect();
            text(m, c, &players)
        }
        _ => String::new(),
    };
    #[cfg(not(target_family = "wasm"))]
    {
        let (text_node, visibility) = &mut *node;
        visibility.set_if_neq(match text.is_empty() {
            true => Visibility::Hidden,
            false => Visibility::Inherited,
        });
        if text_node.0 != text {
            text_node.0 = text;
        }
    }
    #[cfg(target_family = "wasm")]
    if *shown != text {
        let element = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("teams"));
        if let Some(element) = element {
            element.set_text_content(Some(&text));
        }
        *shown = text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_toggles_switches_and_balances() {
        let mut s = Screen::default();
        // Closed: the stick moves the player, not the team.
        assert_eq!(s.step((0, 80, false), 0, 2, 0), None);
        assert_eq!(s.step((0, 0, true), 0, 2, 0), None);
        assert!(s.open);
        // A push right: the next team (wrapping), held: no repeat; left from 0 of 4: team 3.
        assert_eq!(s.step((0, 80, true), 1, 2, 0), Some(Teams::Switch(0)));
        assert_eq!(s.step((0, 80, false), 1, 2, 0), None);
        assert_eq!(s.step((0, 0, false), 0, 4, 0), None);
        assert_eq!(s.step((0, -80, false), 0, 4, 0), Some(Teams::Switch(3)));
        // C-left adds to the BALANCE, C-right takes; only with 2 teams.
        assert_eq!(s.step((C_LEFT, 0, false), 0, 2, 2), Some(Teams::Balance(3)));
        assert_eq!(
            s.step((C_RIGHT, 0, false), 0, 2, 2),
            Some(Teams::Balance(1))
        );
        assert_eq!(s.step((0, 0, false), 0, 4, 0), None);
        assert_eq!(s.step((C_LEFT, 0, false), 0, 4, 0), None);
        // A closes it; Tab again reopens.
        assert_eq!(s.step((BUTTON_A, 0, false), 0, 2, 0), None);
        assert!(!s.open);
        assert_eq!(s.step((0, 0, true), 0, 2, 0), None);
        assert!(s.open);
    }

    #[test]
    fn the_screen_lists_each_team() {
        let m = MatchView {
            teams: 2,
            balance: -1,
            raptor: true,
            ..MatchView::default()
        };
        let own = Combat {
            slot: 1,
            team: 1,
            ..Combat::default()
        };
        let text = text(
            &m,
            &own,
            &[(0, 0, false), (1, 1, false), (2, 0, true), (3, 1, true)],
        );
        assert_eq!(
            text,
            "TEAMS (Tab: close)\n UGAS      P1 BOT\n>RAPTORS   YOU BOT\nBALANCE -1 (Q/E)\nstick: change team, A: close"
        );
    }
}
