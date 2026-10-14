//! The in-game Esc menu (CONTROLS_RESEARCH.md §6, decision 17): RESUME / SETTINGS / LEAVE MATCH
//! over the running match, in the HUD's own quads and font ([`Quads`], which [`crate::hud`] draws),
//! with the settings screen and rebinding ([`crate::settings_ui`]).
//! - **Opens** on Esc, the pad's Menu (Start), natively the window losing the focus, in the browser
//!   the pointer lock's loss (the browser's Esc, which the page may never see: it raises
//!   [`Paused`]). Only while playing, never over the results; they close it (they read the input
//!   that is sent).
//! - **While open** it holds [`Paused`], and the match goes on: `buffer_input` sends a neutral stick
//!   and no buttons, the camera's yaw kept; [`crate::bindings::Controls`] sees no devices (the
//!   team screen, which the menu closes, H, M, F8, the pad's look); the orbit camera stands; the
//!   native cursor is free. It stays open until the press that closed it is let go
//!   ([`Page::Closing`]: a RESUME click fires no shot).
//! - **Navigation:** the mouse (hover and click on the last frame's rectangles, [`Quads::hit`]);
//!   the arrows, Enter and Esc; the pad's D-pad or left stick, A, B, LB / RB for the tabs. The wheel
//!   moves the focus; a held direction repeats ([`Repeat`]).
//! - **RESUME:** natively at once (the cursor grabbed again, [`crate::pointer`]); in the browser a
//!   click or key asks for the pointer lock (a user gesture) and the menu closes once it holds
//!   (refused, as Chrome does for 1.25 s after an Esc: CLICK RESUME AGAIN); the pad's resumes
//!   without it. Esc doesn't resume in the browser: it is the lock's, and may reach the page late.
//! - **LEAVE MATCH:** natively an exit (`connection::leave` disconnects first); in the browser
//!   netcode's disconnect, then the landing page ([`LANDING`]) once it went out.
//! - **Settings** apply live, APPLY saves them ([`crate::settings::save`]; offscreen runs never do),
//!   BACK drops what wasn't applied.
//! - `keys=READ:KEY,…` presses keys at script reads, for screenshots ([`press_keys`]).

use crate::bindings::{Action, Bindings, KeyLabels};
use crate::hud::{Sprite, Tex};
use crate::pointer::Paused;
use crate::settings::Settings;
use crate::settings_ui::{
    self as ui, Bound, Capture, Captured, Glyphs, Held, Resolve, Row, Slot, Tab,
};
use crate::{Config, FollowCamera};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use conker_assets::hud::{Align, Anchor, Horizontal, Hud, Rgba, Text, Vertical};
use conker_sim::net::MatchView;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

/// Where LEAVE MATCH goes in the browser (the results' QUIT's page).
pub const LANDING: &str = "index.html";
/// The main page's choices.
pub const MAIN: [&str; 3] = ["RESUME", "SETTINGS", "LEAVE MATCH"];
/// A conflict's choices.
pub const CHOICES: [&str; 3] = ["SWAP", "REPLACE", "CANCEL"];
/// Rows a tab shows at once.
pub const VISIBLE: usize = 18;
/// Seconds a held direction waits before it repeats, then between repeats.
const REPEAT_DELAY: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.08;
/// Seconds APPLY's SAVED shows.
const SAVED_SECS: f64 = 2.0;

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub page: Page,
    /// Browser: RESUME asked for the pointer lock; the menu closes once it holds.
    pub resuming: bool,
    /// Browser: a lock was refused (`pointerlockerror`).
    pub refused: bool,
    /// Browser: leaving, for the landing page from this time.
    leaving: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Page {
    #[default]
    Closed,
    /// Closed, but the press that closed it is still held (not the game's).
    Closing,
    /// The focus among [`MAIN`].
    Main(usize),
    Settings(Box<Sheet>),
}

/// The settings screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub tab: Tab,
    /// The focused item ([`items`]) and its column (a Keyboard & Mouse slot; APPLY, BACK).
    pub focus: usize,
    pub column: usize,
    /// The first row shown.
    pub scroll: usize,
    pub capture: Option<Capture>,
    pub conflict: Option<Conflict>,
    /// When APPLY saved.
    saved: Option<f64>,
    /// The settings and bindings as last applied: BACK goes back to them.
    applied: (Settings, Bindings),
}

/// A captured input that another slot holds: SWAP, REPLACE or CANCEL.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub action: Action,
    pub slot: Slot,
    pub input: Bound,
    pub with: Vec<(Action, Slot)>,
    /// The focus among [`CHOICES`].
    pub focus: usize,
}

/// What the focus moves through on a tab: the tabs, its rows but the groups' titles, APPLY / BACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Tabs,
    Row(usize),
    Buttons,
}

pub fn items(tab: Tab) -> Vec<Item> {
    let rows = (ui::rows(tab).into_iter().enumerate())
        .filter(|(_, r)| !matches!(r, Row::Group(_)))
        .map(|(i, _)| Item::Row(i));
    std::iter::once(Item::Tabs)
        .chain(rows)
        .chain([Item::Buttons])
        .collect()
}

/// One navigation step (keyboard, pad, wheel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    PrevTab,
    NextTab,
    /// The pad's Menu: closes the menu.
    Menu,
}

/// What the menu asks of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Resume,
    Leave,
    /// APPLY: save the settings.
    Save,
}

/// What a rectangle of the layout stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Main(usize),
    Tab(Tab),
    /// An item and its column.
    Item(usize, usize),
    /// A setting's row: its item, one step down (its `<`) or up.
    Step(usize, i32),
    Choice(usize),
}

/// What opens or closes the menu this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Signals {
    pub esc: bool,
    /// The pad's Menu (Start).
    pub start: bool,
    pub lock_lost: bool,
    pub focus_lost: bool,
    /// The local player is in the match.
    pub playing: bool,
    pub results: bool,
    /// Nothing that closes the menu is held (a closing menu closes).
    pub released: bool,
    /// Browser: the pointer lock holds (a resuming menu closes).
    pub locked: bool,
}

impl Menu {
    /// The game's input is the menu's (from the opening press until the closing one is let go).
    pub fn open(&self) -> bool {
        self.page != Page::Closed
    }

    /// Opens or closes on the frame's signals; true if it opened (the Esc or Menu press is spent).
    pub fn signals(&mut self, f: &Signals) -> bool {
        if f.results || !f.playing {
            (self.page, self.resuming) = (Page::Closed, false);
            return false;
        }
        if self.page == Page::Closing && f.released {
            self.page = Page::Closed;
        }
        if self.resuming && f.locked {
            (self.page, self.resuming) = (Page::Closing, false);
        }
        let shut = matches!(self.page, Page::Closed | Page::Closing);
        if shut && (f.esc || f.start || f.lock_lost || f.focus_lost) {
            *self = Menu {
                page: Page::Main(0),
                ..Menu::default()
            };
            return true;
        }
        false
    }

    /// One step of the focus or a choice; settings change live in `s` and `b`.
    pub fn nav(
        &mut self,
        nav: Nav,
        now: f64,
        s: &mut Settings,
        b: &mut Bindings,
    ) -> Option<Effect> {
        match &mut self.page {
            Page::Closed | Page::Closing => None,
            Page::Main(focus) => match nav {
                Nav::Up => {
                    *focus = focus.saturating_sub(1);
                    None
                }
                Nav::Down => {
                    *focus = (*focus + 1).min(MAIN.len() - 1);
                    None
                }
                Nav::Confirm if *focus == 0 => Some(Effect::Resume),
                Nav::Confirm if *focus == 1 => {
                    self.page = Page::Settings(Box::new(Sheet::new(s, b)));
                    None
                }
                Nav::Confirm => Some(Effect::Leave),
                Nav::Back | Nav::Menu => Some(Effect::Resume),
                _ => None,
            },
            Page::Settings(sheet) => {
                let free = sheet.conflict.is_none();
                let back_button = nav == Nav::Confirm && sheet.item() == Item::Buttons;
                let back = free && (nav == Nav::Back || back_button && sheet.column == 1);
                if !back && nav != Nav::Menu {
                    return sheet.nav(nav, now, s, b);
                }
                (*s, *b) = sheet.applied.clone();
                match nav {
                    Nav::Menu => {
                        self.page = Page::Main(0);
                        Some(Effect::Resume)
                    }
                    _ => {
                        self.page = Page::Main(1);
                        None
                    }
                }
            }
        }
    }

    /// A capture's frame (false without one: the frame's navigation goes on). An input another
    /// slot holds opens its [`Conflict`].
    pub fn capture(&mut self, now: f64, held: &Held, s: &mut Settings, b: &mut Bindings) -> bool {
        let Page::Settings(sheet) = &mut self.page else {
            return false;
        };
        let Some(c) = &mut sheet.capture else {
            return false;
        };
        let (action, slot) = (c.action, c.slot);
        match c.step(now, held) {
            None => return true,
            Some(Captured::Cancel) => {}
            Some(Captured::Clear) => ui::set(b, action, slot, None),
            Some(Captured::Input(input)) => {
                let with = ui::conflicts(b, action, slot, input);
                match with.is_empty() {
                    true => ui::set(b, action, slot, Some(input)),
                    false => {
                        sheet.conflict = Some(Conflict {
                            action,
                            slot,
                            input,
                            with,
                            focus: 0,
                        })
                    }
                }
            }
        }
        sheet.capture = None;
        s.bindings = b.overrides();
        true
    }

    /// The pointer over `t`: the focus goes there.
    pub fn hover(&mut self, t: Target) {
        match (&mut self.page, t) {
            (Page::Main(focus), Target::Main(i)) => *focus = i,
            (Page::Settings(sh), Target::Choice(i)) => {
                if let Some(c) = &mut sh.conflict {
                    c.focus = i;
                }
            }
            (Page::Settings(sh), Target::Item(i, column)) if sh.idle() => sh.focus(i, column),
            (Page::Settings(sh), Target::Step(i, _)) if sh.idle() => sh.focus(i, 0),
            _ => {}
        }
    }

    /// A click on `t`: a tab opens, a setting steps, else the focus goes there and confirms.
    pub fn click(
        &mut self,
        t: Target,
        now: f64,
        s: &mut Settings,
        b: &mut Bindings,
    ) -> Option<Effect> {
        if let Page::Settings(sh) = &mut self.page
            && sh.idle()
        {
            match t {
                Target::Tab(tab) => return sh.set_tab(tab),
                Target::Step(i, dir) => {
                    sh.focus(i, 0);
                    if let Item::Row(r) = sh.item()
                        && let Row::Value(f) = ui::rows(sh.tab)[r]
                    {
                        f.step(s, dir);
                    }
                    return None;
                }
                _ => {}
            }
        }
        self.hover(t);
        self.nav(Nav::Confirm, now, s, b)
    }
}

impl Sheet {
    fn new(s: &Settings, b: &Bindings) -> Self {
        Self {
            tab: Tab::Keys,
            focus: 0,
            column: 0,
            scroll: 0,
            capture: None,
            conflict: None,
            saved: None,
            applied: (s.clone(), b.clone()),
        }
    }

    pub fn item(&self) -> Item {
        items(self.tab)[self.focus]
    }

    /// No capture or conflict over the screen.
    fn idle(&self) -> bool {
        self.capture.is_none() && self.conflict.is_none()
    }

    /// The slot a binding's column stands for.
    pub fn slot(&self) -> Slot {
        match self.tab {
            Tab::Pad => Slot::Pad,
            _ => Slot::Key(self.column),
        }
    }

    /// An item's columns: a Keyboard & Mouse binding's two slots, APPLY and BACK.
    fn columns(&self, item: Item) -> usize {
        match item {
            Item::Row(r) if self.tab == Tab::Keys => match ui::rows(self.tab)[r] {
                Row::Bind(_) => 2,
                _ => 1,
            },
            Item::Buttons => 2,
            _ => 1,
        }
    }

    fn set_tab<T>(&mut self, tab: Tab) -> Option<T> {
        (self.tab, self.focus, self.column, self.scroll) = (tab, 0, 0, 0);
        None
    }

    fn turn(&mut self, dir: i32) {
        let i = Tab::ALL.iter().position(|&t| t == self.tab).unwrap_or(0) as i32;
        self.set_tab::<()>(Tab::ALL[(i + dir).rem_euclid(Tab::ALL.len() as i32) as usize]);
    }

    fn focus(&mut self, focus: usize, column: usize) {
        self.focus = focus.min(items(self.tab).len() - 1);
        self.column = column.min(self.columns(self.item()) - 1);
        // The list follows the focused row (its group's title kept above a group's first row).
        if let Item::Row(r) = self.item() {
            let rows = ui::rows(self.tab);
            let top = match r.checked_sub(1).map(|t| rows[t]) {
                Some(Row::Group(_)) => r - 1,
                _ => r,
            };
            self.scroll = self.scroll.min(top).max((r + 1).saturating_sub(VISIBLE));
        }
    }

    fn nav(&mut self, nav: Nav, now: f64, s: &mut Settings, b: &mut Bindings) -> Option<Effect> {
        if let Some(c) = &mut self.conflict {
            match nav {
                Nav::Up | Nav::Left => c.focus = c.focus.saturating_sub(1),
                Nav::Down | Nav::Right => c.focus = (c.focus + 1).min(CHOICES.len() - 1),
                Nav::Confirm => {
                    let c = self.conflict.take()?;
                    let how = [Resolve::Swap, Resolve::Replace].get(c.focus);
                    if let Some(&how) = how {
                        ui::bind(b, c.action, c.slot, c.input, how);
                        s.bindings = b.overrides();
                    }
                }
                Nav::Back => self.conflict = None,
                _ => {}
            }
            return None;
        }
        let dir = if nav == Nav::Right { 1 } else { -1 };
        let step = (self.column as i32 + dir).max(0) as usize;
        match (nav, self.item()) {
            (Nav::Up, _) => self.focus(self.focus.saturating_sub(1), self.column),
            (Nav::Down, _) => self.focus(self.focus + 1, self.column),
            (Nav::PrevTab, _) => self.turn(-1),
            (Nav::NextTab, _) | (Nav::Confirm, Item::Tabs) => self.turn(1),
            (Nav::Left | Nav::Right, Item::Tabs) => self.turn(dir),
            (Nav::Left | Nav::Right, Item::Row(r)) => match ui::rows(self.tab)[r] {
                Row::Value(f) => f.step(s, dir),
                _ => self.focus(self.focus, step),
            },
            (Nav::Left | Nav::Right, Item::Buttons) => self.focus(self.focus, step),
            (Nav::Confirm, Item::Row(r)) => match ui::rows(self.tab)[r] {
                Row::Value(f) => f.step(s, 1),
                Row::Bind(a) => self.capture = Some(Capture::new(a, self.slot(), now)),
                Row::Group(_) => {}
            },
            // APPLY (BACK is the menu's).
            (Nav::Confirm, Item::Buttons) => {
                (self.applied, self.saved) = ((s.clone(), b.clone()), Some(now));
                return Some(Effect::Save);
            }
            _ => {}
        }
        None
    }
}

/// A held direction: once as it is pushed, then after [`REPEAT_DELAY`] every [`REPEAT_EVERY`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Repeat {
    held: Option<Nav>,
    next: f64,
}

impl Repeat {
    pub fn step(&mut self, held: Option<Nav>, now: f64) -> Option<Nav> {
        if held != self.held {
            (self.held, self.next) = (held, now + REPEAT_DELAY);
            return held;
        }
        if held.is_some() && now >= self.next {
            self.next = now + REPEAT_EVERY;
            return held;
        }
        None
    }
}

/// The menu's quads for the HUD to draw, and its rectangles (frame pixels, centre-anchored) for the
/// pointer.
#[derive(Resource, Clone, Debug, Default)]
pub struct Quads {
    pub sprites: Vec<Sprite>,
    pub hits: Vec<([f32; 4], Target)>,
}

impl Quads {
    /// The target under `p` (frame pixels), the last drawn first.
    pub fn hit(&self, [x, y]: [f32; 2]) -> Option<Target> {
        (self.hits.iter().rev())
            .find(|([rx, ry, w, h], _)| (*rx..rx + w).contains(&x) && (*ry..ry + h).contains(&y))
            .map(|h| h.1)
    }
}

const CENTRE: Anchor = Anchor(Horizontal::Centre, Vertical::Middle);
const WHITE: Rgba = [255; 4];
const AMBER: Rgba = [248, 168, 32, 255];
const GREY: Rgba = [160, 160, 160, 255];
const BAR: Rgba = [248, 168, 32, 70];
/// Over the HUD and its fades (9.5): the dim, the focus bars, the text (its shadow 0.5 under);
/// a capture's or conflict's box over them.
const Z: f32 = 10.0;
const Z_BAR: f32 = 10.2;
const Z_TEXT: f32 = 11.0;
const Z_BOX: f32 = 12.0;
const Z_BOX_BAR: f32 = 12.2;
const Z_BOX_TEXT: f32 = 13.0;
/// The rows' first top, step and text scale.
const ROW_TOP: f32 = 24.0;
const ROW_STEP: f32 = 9.0;
const SMALL: f32 = 0.6;
/// The tabs' left edges and widths.
const TABS: [[f32; 2]; 4] = [[4.0, 96.0], [102.0, 64.0], [168.0, 60.0], [230.0, 58.0]];

/// Quads in the HUD's frame, centre-anchored (so [`crate::hud::frame_point`] inverts them).
struct Paint<'a> {
    hud: &'a Hud,
    out: Quads,
}

impl Paint<'_> {
    fn fill(&mut self, rect: [f32; 4], colour: Rgba, z: f32) {
        self.out.sprites.push(Sprite {
            tex: Tex::Solid,
            uv: [0.0; 4],
            rect,
            anchor: CENTRE,
            colour,
            z,
        });
    }

    /// `line` in upper case (a character without a glyph as `?`), its top at `y`, from `x` or
    /// centred on it, at `scale`.
    fn text(
        &mut self,
        line: &str,
        [x, y]: [f32; 2],
        centre: bool,
        scale: f32,
        colour: Rgba,
        z: f32,
    ) {
        let font = &self.hud.font;
        let line: String = (line.to_uppercase().chars())
            .map(|c| match c == ' ' || font.iter().any(|g| g.ch == c) {
                true => c,
                false => '?',
            })
            .collect();
        let t = Text {
            anchor: CENTRE,
            at: [x as i16, y as i16],
            align: if centre { Align::Centre } else { Align::Left },
            colour,
            shadow: Some([0, 0, 0, colour[3]]),
        };
        for mut s in crate::hud::text(self.hud, &line, None, &t, z) {
            let [sx, sy, w, h] = s.rect;
            s.rect = [
                x + (sx - x) * scale,
                y + (sy - y) * scale,
                w * scale,
                h * scale,
            ];
            self.out.sprites.push(s);
        }
    }

    fn hit(&mut self, rect: [f32; 4], t: Target) {
        self.out.hits.push((rect, t));
    }

    /// A choice: its bar when focused, its label, its rectangle.
    fn button(&mut self, label: &str, rect: [f32; 4], lit: bool, t: Target, z: [f32; 2]) {
        let [x, y, w, h] = rect;
        if lit {
            self.fill(rect, BAR, z[0]);
        }
        let colour = if lit { AMBER } else { WHITE };
        self.text(
            label,
            [x + w / 2.0, y + (h - 9.0) / 2.0],
            true,
            0.7,
            colour,
            z[1],
        );
        self.hit(rect, t);
    }
}

/// What the menu shows: the labels its rows need.
pub struct Look<'a> {
    pub settings: &'a Settings,
    pub bindings: &'a Bindings,
    pub labels: Option<&'a KeyLabels>,
    pub glyphs: Glyphs,
    pub now: f64,
}

/// The open menu's quads and rectangles (none closed or closing).
pub fn layout(menu: &Menu, hud: &Hud, look: &Look) -> Quads {
    let mut p = Paint {
        hud,
        out: Quads::default(),
    };
    let far = 10_000.0;
    match &menu.page {
        Page::Closed | Page::Closing => return p.out,
        Page::Main(focus) => {
            p.fill([-far, -far, 2.0 * far, 2.0 * far], [0, 0, 0, 150], Z);
            p.text("MENU", [146.0, 56.0], true, 1.0, AMBER, Z_TEXT);
            for (i, label) in MAIN.iter().enumerate() {
                let rect = [86.0, 80.0 + 18.0 * i as f32, 120.0, 16.0];
                p.button(label, rect, i == *focus, Target::Main(i), [Z_BAR, Z_TEXT]);
            }
            if menu.refused {
                p.text(
                    "CLICK RESUME AGAIN",
                    [146.0, 140.0],
                    true,
                    SMALL,
                    GREY,
                    Z_TEXT,
                );
            }
        }
        Page::Settings(sheet) => {
            p.fill([-far, -far, 2.0 * far, 2.0 * far], [0, 0, 0, 240], Z);
            sheet_layout(&mut p, sheet, look);
        }
    }
    p.out
}

fn sheet_layout(p: &mut Paint, sh: &Sheet, look: &Look) {
    let (s, b, g) = (look.settings, look.bindings, look.glyphs);
    let items = items(sh.tab);
    let focused = items[sh.focus];
    for (tab, [x, w]) in Tab::ALL.into_iter().zip(TABS) {
        let rect = [x, 3.0, w, 13.0];
        let on = tab == sh.tab;
        if on {
            let tint = if focused == Item::Tabs {
                BAR
            } else {
                [255, 255, 255, 40]
            };
            p.fill(rect, tint, Z_BAR);
        }
        let colour = if on { AMBER } else { WHITE };
        p.text(tab.label(), [x + w / 2.0, 6.0], true, SMALL, colour, Z_TEXT);
        p.hit(rect, Target::Tab(tab));
    }
    p.fill([4.0, 18.0, 284.0, 1.0], [255, 255, 255, 80], Z_BAR);
    let rows = ui::rows(sh.tab);
    let shown = rows.iter().enumerate().skip(sh.scroll).take(VISIBLE);
    for (k, (r, row)) in shown.enumerate() {
        let y = ROW_TOP + ROW_STEP * k as f32;
        let Some(item) = items.iter().position(|&i| i == Item::Row(r)) else {
            if let Row::Group(name) = row {
                p.text(name, [6.0, y], false, 0.55, AMBER, Z_TEXT);
            }
            continue;
        };
        let lit = item == sh.focus;
        if lit {
            p.fill([4.0, y - 1.0, 284.0, ROW_STEP], [255, 255, 255, 30], Z_BAR);
        }
        let cell = |p: &mut Paint, text: &str, [x, w]: [f32; 2], on: bool, t: Target| {
            if on {
                p.fill([x, y - 1.0, w, ROW_STEP], BAR, Z_BAR);
            }
            let colour = if on { AMBER } else { WHITE };
            p.text(text, [x + w / 2.0, y], true, SMALL, colour, Z_TEXT);
            p.hit([x, y - 1.0, w, ROW_STEP], t);
        };
        match *row {
            Row::Bind(a) => {
                p.text(&ui::action_label(a), [14.0, y], false, SMALL, WHITE, Z_TEXT);
                p.hit(
                    [4.0, y - 1.0, 138.0, ROW_STEP],
                    Target::Item(item, sh.column),
                );
                let slots: &[(Slot, [f32; 2])] = match sh.tab {
                    Tab::Pad => &[(Slot::Pad, [143.0, 136.0])],
                    _ => &[(Slot::Key(0), [143.0, 66.0]), (Slot::Key(1), [213.0, 66.0])],
                };
                for (column, &(slot, at)) in slots.iter().enumerate() {
                    let text = ui::bound_label(ui::get(b, a, slot), look.labels, g);
                    cell(
                        p,
                        &text,
                        at,
                        lit && column == sh.column,
                        Target::Item(item, column),
                    );
                }
            }
            Row::Value(f) => {
                p.text(f.label(), [14.0, y], false, SMALL, WHITE, Z_TEXT);
                p.hit([4.0, y - 1.0, 138.0, ROW_STEP], Target::Step(item, 1));
                cell(p, "<", [142.0, 12.0], false, Target::Step(item, -1));
                cell(p, &f.text(s), [154.0, 114.0], lit, Target::Step(item, 1));
                cell(p, ">", [268.0, 12.0], false, Target::Step(item, 1));
            }
            Row::Group(_) => {}
        }
    }
    if rows.len() > VISIBLE {
        let track = ROW_STEP * VISIBLE as f32;
        let (top, n) = (ROW_TOP - 1.0, rows.len() as f32);
        p.fill([289.0, top, 2.0, track], [255, 255, 255, 40], Z_BAR);
        let thumb = [
            289.0,
            top + track * sh.scroll as f32 / n,
            2.0,
            track * VISIBLE as f32 / n,
        ];
        p.fill(thumb, AMBER, Z_BAR);
    }
    let buttons = focused == Item::Buttons;
    for (i, label) in ["APPLY", "BACK"].iter().enumerate() {
        let rect = [91.0 + 60.0 * i as f32, 187.0, 50.0, 14.0];
        let lit = buttons && sh.column == i;
        p.button(
            label,
            rect,
            lit,
            Target::Item(items.len() - 1, i),
            [Z_BAR, Z_TEXT],
        );
    }
    if sh.saved.is_some_and(|t| look.now - t < SAVED_SECS) {
        p.text("SAVED", [236.0, 190.0], true, SMALL, GREY, Z_TEXT);
    }
    let hint = "ENTER / A: CHANGE   ESC / B: BACK   LB / RB: TABS";
    p.text(hint, [146.0, 205.0], true, 0.45, GREY, Z_TEXT);
    if let Some(c) = &sh.capture {
        p.fill([36.0, 72.0, 220.0, 64.0], [0, 0, 0, 255], Z_BOX);
        p.fill([36.0, 72.0, 220.0, 1.0], AMBER, Z_BOX_BAR);
        let title = format!("{} - {}", ui::action_label(c.action), c.slot.label());
        p.text(&title, [146.0, 78.0], true, SMALL, AMBER, Z_BOX_TEXT);
        let (press, keys) = ui::PROMPT.split_once(" - ").unwrap_or((ui::PROMPT, ""));
        p.text(press, [146.0, 92.0], true, 0.7, WHITE, Z_BOX_TEXT);
        p.text(keys, [146.0, 106.0], true, SMALL, WHITE, Z_BOX_TEXT);
        let left = c.left(look.now).to_string();
        p.text(&left, [146.0, 120.0], true, SMALL, GREY, Z_BOX_TEXT);
    }
    if let Some(c) = &sh.conflict {
        p.fill([22.0, 70.0, 248.0, 70.0], [0, 0, 0, 255], Z_BOX);
        p.fill([22.0, 70.0, 248.0, 1.0], AMBER, Z_BOX_BAR);
        let input = ui::bound_label(Some(c.input), look.labels, g);
        let line = format!("{input} IS ALREADY BOUND TO");
        p.text(&line, [146.0, 77.0], true, SMALL, WHITE, Z_BOX_TEXT);
        let with: Vec<String> = (c.with.iter())
            .map(|(a, slot)| format!("{} ({})", ui::action_label(*a), slot.label()))
            .collect();
        p.text(
            &with.join(", "),
            [146.0, 90.0],
            true,
            SMALL,
            AMBER,
            Z_BOX_TEXT,
        );
        for (i, label) in CHOICES.iter().enumerate() {
            let rect = [58.0 + 60.0 * i as f32, 116.0, 56.0, 14.0];
            let z = [Z_BOX_BAR, Z_BOX_TEXT];
            p.button(label, rect, c.focus == i, Target::Choice(i), z);
        }
    }
}

/// The HUD's font and frame; whether APPLY writes the settings (a window's run).
#[derive(Resource)]
struct Setup {
    hud: Hud,
    save: bool,
}

/// The last frame's [`Paused`] (as the menu left it), focus, pointer and held direction.
#[derive(Default)]
struct Last {
    paused: bool,
    focused: bool,
    cursor: Option<[f32; 2]>,
    repeat: Repeat,
}

/// Drawn clients (windowed and offscreen) only; `save`: APPLY writes the settings (a window's).
pub fn build(app: &mut App, hud: Hud, save: bool) {
    app.init_resource::<Menu>();
    app.insert_resource(Setup { hud, save });
    app.init_resource::<Quads>();
    let scripted = |cfg: Res<Config>| !cfg.keys.is_empty();
    app.add_systems(
        PreUpdate,
        (press_keys.run_if(scripted), run)
            .chain()
            .after(bevy::input::InputSystems),
    );
}

/// `keys=READ:KEY,…`: the keys by read (`KeyCode` names: `Escape`, `ArrowDown`, `Enter`).
pub fn parse_keys(text: &str) -> Option<Vec<(u32, KeyCode)>> {
    let key = |p: &str| {
        let (read, key) = p.split_once(':')?;
        Some((
            read.trim().parse().ok()?,
            crate::bindings::variant(key.trim())?,
        ))
    };
    let mut keys = text.split(',').map(key).collect::<Option<Vec<_>>>()?;
    keys.sort_by_key(|k| k.0);
    Some(keys)
}

/// `keys=`: each key pressed for a frame once the script's read reaches its own (the clock of
/// `at=`'s screenshot; unscripted, counted from the player's start as a script's).
fn press_keys(
    cfg: Res<Config>,
    timeline: Res<LocalTimeline>,
    camera: Query<&FollowCamera>,
    game: Query<&MatchView>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut next: Local<(usize, Option<KeyCode>)>,
) {
    if let Some(k) = next.1.take() {
        keys.release(k);
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let read = crate::script_read(timeline.tick().0, camera.since, game.iter().next());
    if let Some(&(at, k)) = cfg.keys.get(next.0)
        && read.is_some_and(|r| r >= at)
    {
        info!("keys: {k:?} at read {at}");
        keys.press(k);
        *next = (next.0 + 1, Some(k));
    }
}

/// The frame's navigation (with whether the pad gave it): held directions (arrows, D-pad, left
/// stick), the wheel, Enter / A, Esc / B (`esc`: Esc goes back), LB / RB, the pad's Menu.
fn navs(
    keys: &ButtonInput<KeyCode>,
    pad: Option<&Gamepad>,
    wheel: f32,
    esc: bool,
    repeat: &mut Repeat,
    now: f64,
) -> Vec<(Nav, bool)> {
    use GamepadButton as P;
    use KeyCode as K;
    let stick = pad.map_or(Vec2::ZERO, |p| p.left_stick());
    let held = |k, b| keys.pressed(k) || pad.is_some_and(|p| p.pressed(b));
    let dir = [
        (Nav::Up, held(K::ArrowUp, P::DPadUp) || stick.y > 0.5),
        (Nav::Down, held(K::ArrowDown, P::DPadDown) || stick.y < -0.5),
        (Nav::Left, held(K::ArrowLeft, P::DPadLeft) || stick.x < -0.5),
        (
            Nav::Right,
            held(K::ArrowRight, P::DPadRight) || stick.x > 0.5,
        ),
    ];
    let dir = dir.into_iter().find(|d| d.1).map(|d| d.0);
    let mut out: Vec<(Nav, bool)> = repeat
        .step(dir, now)
        .map(|d| (d, false))
        .into_iter()
        .collect();
    match wheel {
        w if w > 0.0 => out.push((Nav::Up, false)),
        w if w < 0.0 => out.push((Nav::Down, false)),
        _ => {}
    }
    if keys.any_just_pressed([K::Enter, K::NumpadEnter]) {
        out.push((Nav::Confirm, false));
    }
    if esc && keys.just_pressed(K::Escape) {
        out.push((Nav::Back, false));
    }
    let pads = [
        (P::South, Nav::Confirm),
        (P::East, Nav::Back),
        (P::LeftTrigger, Nav::PrevTab),
        (P::RightTrigger, Nav::NextTab),
        (P::Start, Nav::Menu),
    ];
    for (b, nav) in pads {
        if pad.is_some_and(|p| p.just_pressed(b)) {
            out.push((nav, true));
        }
    }
    out
}

/// The menu's frame: opens or closes it, steers it (a capture, the pointer, the keys and pad),
/// carries out its choices and lays out its quads.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run(
    (mut menu, mut quads, setup): (ResMut<Menu>, ResMut<Quads>, Res<Setup>),
    (time, keys, mouse, scroll): (
        Res<Time<Real>>,
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<AccumulatedMouseScroll>,
    ),
    pads: Query<(&Gamepad, Option<&Name>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (playing, game, outro, outros, mut paused): (
        Query<(), With<FollowCamera>>,
        Query<&MatchView>,
        Res<crate::outro::Playing>,
        Res<crate::outro::Outros>,
        ResMut<Paused>,
    ),
    (mut settings, mut bindings, mut teams, labels): (
        ResMut<Settings>,
        ResMut<Bindings>,
        ResMut<crate::teams::Screen>,
        Option<Res<KeyLabels>>,
    ),
    mut last: Local<Last>,
    (clients, mut exit, mut commands): (
        Query<Entity, (With<Client>, With<Connected>)>,
        MessageWriter<AppExit>,
        Commands,
    ),
) {
    use KeyCode as K;
    let now = time.elapsed_secs_f64();
    let window = windows.single().ok();
    let focused = window.is_some_and(|w| w.focused);
    let pad = pads.iter().next().filter(|_| focused);
    let gamepad = pad.map(|p| p.0);
    let web = cfg!(target_family = "wasm");
    let signals = Signals {
        esc: keys.just_pressed(K::Escape),
        start: gamepad.is_some_and(|p| p.just_pressed(GamepadButton::Start)),
        lock_lost: paused.0 && !last.paused,
        focus_lost: !web && last.focused && !focused,
        playing: !playing.is_empty(),
        results: (game.iter()).any(|m| crate::outro::results(m, &outro, &outros)),
        released: mouse.get_pressed().len() == 0
            && !keys.any_pressed([K::Enter, K::NumpadEnter, K::Escape])
            && gamepad.is_none_or(|p| p.digital().get_pressed().len() == 0),
        locked: crate::pointer::locked(),
    };
    // Natively the click that brings the window back is not the menu's (it could be LEAVE MATCH).
    let clicks = web || last.focused;
    last.focused = focused;
    let opened = menu.signals(&signals);
    if opened {
        teams.open = false;
        info!("menu: open");
    }
    if crate::pointer::refused() && menu.open() {
        menu.refused = true;
    }
    if menu.leaving.is_some_and(|at| now >= at) {
        menu.leaving = None;
        #[cfg(target_family = "wasm")]
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href(LANDING);
        }
    }
    // Settings change on copies, written back if they did (their change detection).
    let (mut s, mut b) = (settings.clone(), bindings.clone());
    let mut effects = Vec::new();
    if !opened && menu.open() {
        let held = Held {
            keys: keys.get_pressed().copied().collect(),
            mouse: mouse.get_pressed().copied().collect(),
            wheel: scroll.delta.y,
            pad: gamepad.map(ui::held_pad).unwrap_or_default(),
        };
        if !menu.capture(now, &held, &mut s, &mut b) {
            let cursor = window.and_then(|w| {
                let at = w.cursor_position()?;
                Some(crate::hud::frame_point(setup.hud.frame, w.size(), at))
            });
            let target = cursor.and_then(|c| quads.hit(c));
            if let Some(t) = target.filter(|_| cursor != last.cursor) {
                menu.hover(t);
            }
            last.cursor = cursor;
            let click = clicks && mouse.just_pressed(MouseButton::Left);
            if let Some(t) = target.filter(|_| click) {
                effects.extend(menu.click(t, now, &mut s, &mut b).map(|e| (e, false)));
            }
            // In the browser Esc is the pointer lock's: it goes back, but doesn't resume.
            let esc = !(web && matches!(menu.page, Page::Main(_)));
            let wheel = scroll.delta.y;
            for (nav, by_pad) in navs(&keys, gamepad, wheel, esc, &mut last.repeat, now) {
                effects.extend(menu.nav(nav, now, &mut s, &mut b).map(|e| (e, by_pad)));
            }
        }
    }
    for (effect, by_pad) in effects {
        match effect {
            Effect::Resume if web && !by_pad => {
                crate::pointer::lock();
                (menu.resuming, menu.refused) = (true, false);
            }
            Effect::Resume => {
                menu.page = Page::Closing;
                info!("menu: resume");
            }
            Effect::Leave => {
                info!("menu: leave match");
                if web {
                    for entity in &clients {
                        commands.trigger(Disconnect { entity });
                    }
                    menu.leaving = Some(now + crate::connection::LEAVE_DELAY);
                } else {
                    exit.write(AppExit::Success);
                }
            }
            Effect::Save => {
                if setup.save {
                    crate::settings::save(&s);
                }
                info!("settings: applied");
            }
        }
    }
    settings.set_if_neq(s);
    bindings.set_if_neq(b);
    last.paused = menu.open();
    paused.set_if_neq(Paused(last.paused));
    let glyphs = match pad {
        Some((p, name)) => ui::glyphs(
            settings.pad.prompts,
            p.vendor_id(),
            name.map_or("", |n| n.as_str()),
        ),
        None => ui::glyphs(settings.pad.prompts, None, ""),
    };
    let look = Look {
        settings: &settings,
        bindings: &bindings,
        labels: labels.as_deref(),
        glyphs,
        now,
    };
    *quads = layout(&menu, &setup.hud, &look);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Bind, Controls, device_input};
    use bevy::ecs::system::SystemState;

    fn playing() -> Signals {
        Signals {
            playing: true,
            ..Signals::default()
        }
    }

    #[test]
    fn the_menu_opens_on_each_source_and_never_over_the_results() {
        let sources: [fn(&mut Signals); 4] = [
            |s| s.esc = true,
            |s| s.start = true,
            |s| s.lock_lost = true,
            |s| s.focus_lost = true,
        ];
        for source in sources {
            let mut m = Menu::default();
            let mut f = playing();
            assert!(!m.signals(&f) && !m.open());
            source(&mut f);
            assert!(m.signals(&f) && m.page == Page::Main(0));
            // Not again while open (the press goes to the navigation).
            assert!(!m.signals(&f));
            // Not over the results, nor before the player is in; the results close it.
            let mut over = f;
            over.results = true;
            assert!(!m.signals(&over) && !m.open());
            let mut out = f;
            out.playing = false;
            assert!(!m.signals(&out) && !m.open());
        }
    }

    #[test]
    fn the_closing_press_and_the_browsers_lock_keep_it_open() {
        let (mut s, mut b) = (Settings::default(), Bindings::default());
        let mut m = Menu::default();
        m.signals(&Signals {
            esc: true,
            ..playing()
        });
        // Esc again resumes (natively); the menu waits for it to be let go.
        assert_eq!(m.nav(Nav::Back, 0.0, &mut s, &mut b), Some(Effect::Resume));
        m.page = Page::Closing;
        assert!(m.open());
        m.signals(&playing());
        assert!(m.open(), "Esc still held");
        m.signals(&Signals {
            released: true,
            ..playing()
        });
        assert!(!m.open());
        // The browser: RESUME waits for the lock (refused: still open), then for the click.
        m.signals(&Signals {
            lock_lost: true,
            ..playing()
        });
        m.resuming = true;
        m.signals(&playing());
        assert_eq!(m.page, Page::Main(0));
        m.signals(&Signals {
            locked: true,
            ..playing()
        });
        assert_eq!((m.page.clone(), m.resuming), (Page::Closing, false));
    }

    #[test]
    fn the_game_sees_no_devices_while_it_is_open() {
        let mut world = World::new();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::Space);
        world.insert_resource(keys);
        world.insert_resource(Bindings::default());
        world.insert_resource(Paused(false));
        let mut state = SystemState::<Controls>::new(&mut world);
        let read = |world: &World, state: &mut SystemState<Controls>| {
            let c = state.get(world).unwrap();
            let input = device_input(&c.bindings, &c.devices(1.0, Vec2::ONE)).input;
            (input, c.pressed(Action::Jump))
        };
        let (input, jump) = read(&world, &mut state);
        assert!(input.stick_y > 0 && input.buttons != 0 && jump);
        // The menu holds Paused while open (closing too: the press that closed it).
        world.insert_resource(Paused(true));
        let (input, jump) = read(&world, &mut state);
        assert_eq!((input, jump), (Default::default(), false));
    }

    fn settings_page<'a>(m: &'a mut Menu, s: &mut Settings, b: &mut Bindings) -> &'a mut Sheet {
        *m = Menu {
            page: Page::Main(0),
            ..Menu::default()
        };
        m.nav(Nav::Down, 0.0, s, b);
        m.nav(Nav::Confirm, 0.0, s, b);
        match &mut m.page {
            Page::Settings(sh) => sh,
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn the_focus_moves_through_tabs_rows_and_buttons() {
        let (mut s, mut b) = (Settings::default(), Bindings::default());
        let mut m = Menu::default();
        let sh = settings_page(&mut m, &mut s, &mut b);
        assert_eq!((sh.tab, sh.item()), (Tab::Keys, Item::Tabs));
        // On the tabs, left / right switch them (wrapping); LB / RB anywhere.
        sh.nav(Nav::Left, 0.0, &mut s, &mut b);
        assert_eq!(sh.tab, Tab::Gameplay);
        sh.nav(Nav::NextTab, 0.0, &mut s, &mut b);
        assert_eq!(sh.tab, Tab::Keys);
        // Down: the first binding (the group's title skipped), its primary; right: its alternate.
        sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        assert_eq!(sh.item(), Item::Row(1));
        assert_eq!(ui::rows(Tab::Keys)[1], Row::Bind(Action::MoveForward));
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        assert_eq!((sh.column, sh.slot()), (1, Slot::Key(1)));
        // Down past Walk: Fire (Combat's title skipped), the column kept.
        for _ in 0..7 {
            sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        }
        assert_eq!(
            ui::rows(Tab::Keys)[match sh.item() {
                Item::Row(r) => r,
                i => panic!("{i:?}"),
            }],
            Row::Bind(Action::Fire)
        );
        assert_eq!(sh.column, 1);
        // The list scrolls to the focus: the last row shown, then APPLY / BACK.
        for _ in 0..30 {
            sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        }
        assert_eq!((sh.item(), sh.column), (Item::Buttons, 1));
        assert_eq!(sh.scroll, ui::rows(Tab::Keys).len() - VISIBLE);
        sh.nav(Nav::Left, 0.0, &mut s, &mut b);
        assert_eq!(sh.column, 0);
        // Back up to the first row: its group's title shown again.
        for _ in 0..(items(Tab::Keys).len() - 2) {
            sh.nav(Nav::Up, 0.0, &mut s, &mut b);
        }
        assert_eq!((sh.item(), sh.scroll), (Item::Row(1), 0));
        // A Controller row has one column; a value row steps with left / right.
        sh.nav(Nav::NextTab, 0.0, &mut s, &mut b);
        sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        assert_eq!((sh.column, sh.slot()), (0, Slot::Pad));
        sh.nav(Nav::NextTab, 0.0, &mut s, &mut b);
        sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        assert!((s.mouse.sensitivity - 1.6).abs() < 1e-5, "applied live");
        // The main page: up / down, clamped.
        let mut m = Menu {
            page: Page::Main(0),
            ..Menu::default()
        };
        m.nav(Nav::Up, 0.0, &mut s, &mut b);
        m.nav(Nav::Down, 0.0, &mut s, &mut b);
        m.nav(Nav::Down, 0.0, &mut s, &mut b);
        m.nav(Nav::Down, 0.0, &mut s, &mut b);
        assert_eq!(m.page, Page::Main(2));
        assert_eq!(
            m.nav(Nav::Confirm, 0.0, &mut s, &mut b),
            Some(Effect::Leave)
        );
    }

    #[test]
    fn held_directions_repeat() {
        let mut r = Repeat::default();
        assert_eq!(r.step(Some(Nav::Down), 0.0), Some(Nav::Down));
        assert_eq!(r.step(Some(Nav::Down), 0.3), None);
        assert_eq!(r.step(Some(Nav::Down), 0.4), Some(Nav::Down));
        assert_eq!(r.step(Some(Nav::Down), 0.45), None);
        assert_eq!(r.step(Some(Nav::Down), 0.49), Some(Nav::Down));
        assert_eq!(r.step(None, 0.5), None);
        assert_eq!(r.step(Some(Nav::Up), 0.5), Some(Nav::Up));
    }

    #[test]
    fn rebinding_through_the_menu_settles_its_conflicts() {
        let (mut s, mut b) = (Settings::default(), Bindings::default());
        let d = Bindings::default();
        let mut m = Menu::default();
        let sh = settings_page(&mut m, &mut s, &mut b);
        // Jump's primary: four rows below Move Forward.
        for _ in 0..5 {
            sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        }
        sh.nav(Nav::Confirm, 1.0, &mut s, &mut b);
        let c = sh.capture.expect("a capture");
        assert_eq!((c.action, c.slot), (Action::Jump, Slot::Key(0)));
        let enter = Held {
            keys: vec![KeyCode::Enter],
            ..Held::default()
        };
        let w = Held {
            keys: vec![KeyCode::KeyW],
            ..Held::default()
        };
        // The Enter that opened it is still held, then let go; W is Move Forward's.
        assert!(m.capture(1.0, &enter, &mut s, &mut b));
        assert!(m.capture(1.1, &Held::default(), &mut s, &mut b));
        assert!(m.capture(1.2, &w, &mut s, &mut b));
        let Page::Settings(sh) = &mut m.page else {
            panic!()
        };
        let c = sh.conflict.clone().expect("a conflict");
        assert_eq!(c.with, vec![(Action::MoveForward, Slot::Key(0))]);
        assert_eq!(b, d, "nothing bound before the choice");
        // CANCEL leaves it all; again, SWAP gives Move Forward the Space.
        sh.nav(Nav::Right, 1.3, &mut s, &mut b);
        sh.nav(Nav::Right, 1.3, &mut s, &mut b);
        sh.nav(Nav::Confirm, 1.3, &mut s, &mut b);
        assert!(sh.conflict.is_none() && b == d);
        sh.conflict = Some(c.clone());
        sh.nav(Nav::Confirm, 1.4, &mut s, &mut b);
        let w_key = Bound::Key(Bind::Key(KeyCode::KeyW));
        let space = Bound::Key(Bind::Key(KeyCode::Space));
        assert_eq!(ui::get(&b, Action::Jump, Slot::Key(0)), Some(w_key));
        assert_eq!(ui::get(&b, Action::MoveForward, Slot::Key(0)), Some(space));
        assert_eq!(Bindings::with(&s.bindings), b, "the settings follow");
        // REPLACE: Move Forward keeps only its alternate.
        let mut b2 = d.clone();
        sh.conflict = Some(c);
        sh.nav(Nav::Down, 1.5, &mut s, &mut b2);
        sh.nav(Nav::Confirm, 1.5, &mut s, &mut b2);
        assert_eq!(ui::get(&b2, Action::MoveForward, Slot::Key(0)), None);
        // Backspace clears; the 5 s run out cancels.
        sh.nav(Nav::Confirm, 2.0, &mut s, &mut b2);
        let backspace = Held {
            keys: vec![KeyCode::Backspace],
            ..Held::default()
        };
        assert!(m.capture(2.1, &backspace, &mut s, &mut b2));
        assert_eq!(ui::get(&b2, Action::Jump, Slot::Key(0)), None);
        let Page::Settings(sh) = &mut m.page else {
            panic!()
        };
        sh.nav(Nav::Confirm, 3.0, &mut s, &mut b2);
        assert!(m.capture(8.0, &Held::default(), &mut s, &mut b2));
        let Page::Settings(sh) = &m.page else {
            panic!()
        };
        assert!(sh.capture.is_none() && sh.conflict.is_none());
        assert!(
            !m.capture(8.1, &Held::default(), &mut s, &mut b2),
            "none left"
        );
    }

    #[test]
    fn apply_saves_and_back_drops_the_rest() {
        let (mut s, mut b) = (Settings::default(), Bindings::default());
        let mut m = Menu::default();
        let sh = settings_page(&mut m, &mut s, &mut b);
        sh.set_tab::<()>(Tab::Look);
        sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        sh.focus(items(Tab::Look).len() - 1, 0);
        assert_eq!(
            sh.nav(Nav::Confirm, 5.0, &mut s, &mut b),
            Some(Effect::Save)
        );
        let applied = s.clone();
        assert!((applied.mouse.sensitivity - 1.6).abs() < 1e-5);
        // A change after APPLY, then BACK: back to what was applied, on the main page.
        sh.focus(1, 0);
        sh.nav(Nav::Right, 6.0, &mut s, &mut b);
        b.set(Action::Fire, Default::default());
        assert_ne!(s, applied);
        assert_eq!(m.nav(Nav::Back, 7.0, &mut s, &mut b), None);
        assert_eq!(
            (m.page.clone(), &s, &b),
            (Page::Main(1), &applied, &Bindings::default())
        );
        // What APPLY saves loads back (a file under $CONKER_SETTINGS's directory, else the temp dir).
        let base = std::env::var_os("CONKER_SETTINGS")
            .map(std::path::PathBuf::from)
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("conker-test-menu-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut saved = s.clone();
        let mut rebound = Bindings::default();
        ui::set(
            &mut rebound,
            Action::Jump,
            Slot::Key(0),
            Some(Bound::Key(Bind::Mouse(MouseButton::Forward))),
        );
        saved.bindings = rebound.overrides();
        crate::settings::save_file(&path, &saved).unwrap();
        let back = crate::settings::load_file(&path);
        assert_eq!(
            (back.mouse.sensitivity, Bindings::with(&back.bindings)),
            (saved.mouse.sensitivity, rebound)
        );
        std::fs::remove_dir_all(dir).unwrap();
        // The pad's Menu in the settings drops the rest too, and resumes.
        let sh = settings_page(&mut m, &mut s, &mut b);
        sh.set_tab::<()>(Tab::Look);
        sh.nav(Nav::Down, 0.0, &mut s, &mut b);
        sh.nav(Nav::Right, 0.0, &mut s, &mut b);
        assert_eq!(m.nav(Nav::Menu, 1.0, &mut s, &mut b), Some(Effect::Resume));
        assert_eq!(s, applied);
    }

    #[test]
    fn the_pointer_hits_what_is_drawn() {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let Ok(hud) = Hud::load(|p| std::fs::read(std::path::Path::new(&dir).join(p))) else {
            eprintln!("no pack HUD in {dir}: skipping");
            return;
        };
        let (mut s, mut b) = (Settings::default(), Bindings::default());
        let quads = |m: &Menu, s: &Settings, b: &Bindings| {
            let look = Look {
                settings: s,
                bindings: b,
                labels: None,
                glyphs: Glyphs::Xbox,
                now: 0.0,
            };
            layout(m, &hud, &look)
        };
        let mut m = Menu::default();
        assert!(quads(&m, &s, &b).sprites.is_empty());
        m.page = Page::Main(0);
        let q = quads(&m, &s, &b);
        assert!(q.sprites.iter().all(|s| s.z >= Z));
        // A 1280×720 window: the frame scaled by its height, centred.
        let window = Vec2::new(1280.0, 720.0);
        let point = |q: &Quads, at: Vec2| q.hit(crate::hud::frame_point(hud.frame, window, at));
        assert_eq!(point(&q, Vec2::new(640.0, 360.0)), Some(Target::Main(1)));
        assert_eq!(point(&q, Vec2::new(10.0, 10.0)), None);
        m.hover(Target::Main(2));
        assert_eq!(m.page, Page::Main(2));
        // SETTINGS clicked: the tabs, then a slot and a setting's arrows.
        assert_eq!(m.click(Target::Main(1), 0.0, &mut s, &mut b), None);
        let q = quads(&m, &s, &b);
        let hits: Vec<Target> = q.hits.iter().map(|h| h.1).collect();
        assert!(hits.contains(&Target::Tab(Tab::Gameplay)));
        assert!(
            hits.contains(&Target::Item(1, 1)),
            "Move Forward's alternate"
        );
        m.click(Target::Tab(Tab::Look), 0.0, &mut s, &mut b);
        m.click(Target::Step(1, -1), 0.0, &mut s, &mut b);
        assert!((s.mouse.sensitivity - 1.4).abs() < 1e-5);
        let Page::Settings(sh) = &m.page else {
            panic!()
        };
        assert_eq!((sh.tab, sh.focus), (Tab::Look, 1));
        // A slot clicked opens its capture; its box has no rectangles (a click is an input).
        m.click(Target::Tab(Tab::Keys), 0.0, &mut s, &mut b);
        m.click(Target::Item(1, 1), 0.0, &mut s, &mut b);
        let Page::Settings(sh) = &m.page else {
            panic!()
        };
        assert_eq!(
            sh.capture.map(|c| (c.action, c.slot)),
            Some((Action::MoveForward, Slot::Key(1)))
        );
        let q = quads(&m, &s, &b);
        assert!(q.sprites.iter().any(|s| s.z >= Z_BOX));
    }

    #[test]
    fn keys_parse_by_read() {
        assert_eq!(
            parse_keys("60:Enter, 40:Escape,60:ArrowDown"),
            Some(vec![
                (40, KeyCode::Escape),
                (60, KeyCode::Enter),
                (60, KeyCode::ArrowDown)
            ])
        );
        assert_eq!(parse_keys("40:Nope"), None);
        assert_eq!(parse_keys("x:Enter"), None);
    }
}
