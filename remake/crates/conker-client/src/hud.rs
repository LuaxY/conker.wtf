//! The HUD the original draws over a Deathmatch player's view, from the pack's `hud/` ([`Hud`]:
//! pictures, layout and rules found in the oracle, `lab/hud`): health as chocolate, the held
//! weapon's icon, grenades while one is in the hand, kills and rank, the clock, the radar, the aim
//! reticle, the scope, a mounted gun's sight ([`Sight`]) and the death iris; untimed, the lives
//! left; out of lives, a skull; and once the match is over, the results (`hud/end.json`, [`End`]).
//! War: Total War draws its team strip in place of the kills and rank, the radar's blips by team,
//! its results' header, and a gassed player's haze under the HUD ([`haze`]); War: Colors its strip
//! with the team scores, the carried flags' blinking icons ([`flag_icons`]), the flags on the radar
//! and the capture's fade ([`capture_fade`]); War: Beach its strip with the villagers' head and the
//! team scores, a villager's escape countdown, no radar, the escape's dim and fade in
//! ([`escape_fade`]), the strike's flash ([`strike_flash`]) and its STATS (FRENCHYS WIN, no row
//! for a villager). Every mode: the end phase's fade ([`end_fade`]) and RESTART's
//! ([`restart_fade`]). Fades draw last, over the whole view and the HUD (V c1a DL 456).
//!
//! One implementation, native and in the browser: textured quads (unlit, alpha-blended
//! `StandardMaterial`s) on render layer 1, which a second, orthographic camera draws over the view
//! (no UI or text renderer: the wasm build has neither). The layout is the original's frame scaled
//! to the window by its height (its width when narrower), each part kept at its anchor's distance
//! from the window's edge or centre ([`place`]), as the original places parts off its viewport's
//! edges and centre.

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::batching::NoAutomaticBatching;
use conker_anim::combat::{BLOWN_UP, GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW};
use conker_assets::hud::{
    Align, Anchor, End, Flash, Haze, Horizontal, Hud, Image as Picture, Placed, Rgba, Strip, Text,
    Vertical,
};
use conker_assets::outro::{Outro, tip};
use conker_rules::game::{Standing, places, teams_places};
use conker_rules::net::Fighting;
use conker_rules::projectile::{LOB_HOLD, LOB_PULL};
use conker_sim::net::{Combat, MatchView, Player, PlayerId};
use lightyear::prelude::{Interpolated, LocalTimeline, PeerId, Predicted};

use crate::bindings::{Action, Bindings, KeyLabels};
use crate::connection::NetStatus;
use crate::settings_ui::Glyphs;
use crate::{Anim, FollowCamera};

/// The render layer of the HUD's quads and camera.
const LAYER: usize = 1;
/// Frames into the throw the grenade stays the item in the hand (V: s126+0x75, combat_README §1).
const THROW_IN_HAND: f32 = 15.0;

/// What a quad shows: a texture of [`Hud::textures`], a glyph of [`Hud::font`], or flat colour.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    Texture(String),
    Glyph(usize),
    Solid,
    /// A flat colour that multiplies what is under it (weapon 0x16's R view, [`tint`]).
    Tint,
}

/// One quad: `uv` the texels shown (x0, y0, x1, y1; x1 < x0 mirrors), `rect` where in the frame
/// (x, y, width, height), kept at `anchor`'s distance from the window's edges, times `colour`;
/// higher `z` draws over lower.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub tex: Tex,
    pub uv: [f32; 4],
    pub rect: [f32; 4],
    pub anchor: Anchor,
    pub colour: Rgba,
    pub z: f32,
}

/// What the HUD shows this frame (from the replicated and predicted state, `shown`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub health: u8,
    /// The held weapon (0: none).
    pub weapon: u8,
    /// Grenades left while one is in the hand; `lob`: item 0x3A's (its icon,
    /// [`conker_assets::hud::Grenades::lob_icon`]).
    pub grenades: Option<u8>,
    pub lob: bool,
    pub kills: u16,
    pub rank: usize,
    /// Seconds left on the clock; `None` untimed (the original draws no clock, `func_151DDC20`).
    pub secs: Option<u32>,
    /// The radar is on (SET-UP's radar: HUD flag 0x20).
    pub radar: bool,
    /// Other players for the radar: (right, ahead) of the player in game units, along the view,
    /// their level against the player's (-1 lower, 0, 1 higher) and their blip's colour.
    pub blips: Vec<([f32; 2], i8, Rgba)>,
    /// The reticle is up (R's aim with a weapon of [`conker_assets::hud::Reticle::weapons`]).
    pub reticle: bool,
    /// Looking through a scope: its zoom, 0 opened to 1 at the weapon's closest; its dots turned by
    /// `turn` radians ([`dot_turn`]); weapon 0x16's R view, its red factor ([`tint`]): no dots or
    /// bar, the view under the clock and the ring tinted.
    pub scope: Option<f32>,
    pub turn: f32,
    pub tint: Option<u8>,
    /// On a mounted gun: its sight, in place of the scope's.
    pub gun: Option<Sight>,
    /// Dead: frames to the respawn, and where the iris centres (frame pixels: the head on the
    /// screen, a blast's victim its feet; `None` the pack's `iris.centre`).
    pub respawn_in: Option<u16>,
    pub iris_at: Option<[f32; 2]>,
    /// Untimed: the lives left.
    pub lives: Option<u8>,
    /// Out of lives, and through a takeover's cut: the view black over the HUD, the skull centred
    /// at this half-size in px ([`skull_byte`]).
    pub skull: Option<u8>,
    /// The end phase's `k`th frame from 1: its HUD flags (`&= 0xE35F`, `func_151E53E8`) drop the
    /// radar, kills and rank; the clock, drawn by phase 0's handler (`func_151DDC20`), goes from
    /// k = 2 (V b_kl2).
    pub ending: Option<u32>,
    /// War: Total War: the team strip's numbers, [0] the squirrels', [1] the Tediz': their kills,
    /// untimed (`lives` shown) their lives left; in place of the own kills and rank. War: Colors:
    /// the team scores.
    pub strip: Option<[u32; 2]>,
    /// War: Colors: each flag's carried icon shown this frame (carried, in the blink's shown
    /// phase); `Some` also puts the heads on the strip.
    pub flags: Option<[bool; 2]>,
    /// Black over the whole view and the HUD at this alpha: War: Colors' capture fade, War: Beach's
    /// escape fade, the end phase's.
    pub fade: Option<u8>,
    /// Total War: the gas countdown's seconds ([`countdown`]).
    pub countdown: Option<u32>,
    /// Total War: frames into this player's gassed screen (1-150, [`Combat::haze`]).
    pub haze: Option<u32>,
    /// The match is over: the results' rows ([`rows`]: each with its colour), the menu's choice
    /// (0 RESTART, 1 QUIT), the STATS' alpha ([`stats_alpha`]) and the VI counter (2 a frame: the
    /// chosen button's [`glow`], the [`stick`]).
    pub results: Option<(Vec<Row>, usize, u8, u32)>,
    /// Over in Total War: the results' header.
    pub header: Option<Header>,
    /// War: Beach: its strip ([`Hud::beach`]) in place of Total War's, its STATS header.
    pub beach: bool,
    /// War: Beach: a villager's seconds left to escape (90 down to 0).
    pub escape: Option<u32>,
    /// War: Beach: frames since the strike's blast on this player ([`strike_flash`]).
    pub strike: Option<u32>,
    /// War: Heist: its strip ([`Hud::heist`]) in place of the kills and rank; its STATS.
    pub heist: Option<Bags>,
    /// Tank: `weapon` is the power-up held, its icon one of [`TANK_ICONS`]; the scope has no dots.
    pub tank: bool,
    /// War: Raptor: its strip ([`raptor_strip`]); the carried egg's and the held body's icons
    /// ([`RAPTOR_ICONS`]) shown this frame.
    pub raptor: Option<[bool; 2]>,
    /// Race: its HUD ([`RaceHud`]) in place of the kills and rank; its STATS' rows (each human's
    /// place, total and best lap in VIs, −1 none) in place of the results' columns.
    pub race: Option<RaceHud>,
    pub race_rows: Option<Vec<[i32; 3]>>,
    /// The modern controls' pickup prompt ([`Prompt`]).
    pub prompt: Option<String>,
}

/// War: Raptor's icons (HUD flag 0x4000, C `func_151E89A0`): the carried egg's (`atlas2`) while a
/// caveman carries one, the held body's (`raptor_teams`, the caveman's head) while a raptor holds
/// one, each in the blink's shown phase ([`flag_icons`]): texture, texels, rect (centred, on the
/// top). V h2 DLs 235-247 (the egg: shown 235, 245-247, hidden 240-244), h1 DLs 100-107 (the body:
/// hidden 100, shown 101-107).
pub const RAPTOR_ICONS: [(&str, [u16; 4], [i16; 4]); 2] = [
    ("atlas2", [48, 42, 12, 15], [136, 3, 12, 15]),
    ("raptor_teams", [0, 16, 12, 15], [148, 3, 12, 15]),
];

/// War: Raptor's strip: War: Beach's layout (`beach`), the heads `raptor_teams`' (0xD11): the
/// caveman's (row 16) at x 198 by the cavemen's score, the raptor's (row 0) at x 66 by the raptors'
/// (`func_151E9D18`, C; V h1 DL 40, h2 DL 200).
pub fn raptor_strip(beach: &Strip) -> Strip {
    let mut s = beach.clone();
    for i in s.kills.iter_mut().chain(&mut s.lives) {
        i.texture = "raptor_teams".into();
    }
    s
}

/// Tank's power-up icons at the weapon icon's place, in `atlas2`: the boost (0x01), the scope
/// (0x02), the deadly shells (0x10) and the shield (0x80) (V pw1 DLs 106, 166, 226, 370; pw2 to
/// 1240, gone with the shield at 1250). No count or timer is drawn with them.
pub const TANK_ICONS: [(u8, [u16; 4]); 4] = [
    (conker_rules::tank::BOOST, [0, 25, 22, 14]),
    (conker_rules::tank::SCOPE, [0, 16, 22, 9]),
    (conker_rules::tank::SHELLS, [0, 39, 22, 12]),
    (conker_rules::tank::SHIELD, [0, 4, 22, 12]),
];

/// Race's HUD (flags 0x1096: the lives, the item, the health, the place and lap, the overview map;
/// C `func_150918EC`'s 0x80 and 0x1000 branches; V race-pack-scratch hud-k250, hud-b2, hud-h230).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceHud {
    /// The place (1 the first), the lap on (0 the first) and the laps: the place, "LAP l/n" and
    /// the race clock while racing (`laps ≥ lap + 1`).
    pub place: u8,
    pub lap: i16,
    pub laps: u8,
    /// The race clock and the last lap (VIs; the last shown once > 0).
    pub time: i32,
    pub last: i32,
    /// How many of the item (`Shown::weapon`) are held: one icon each, [`RACE_ITEM_STEP`] apart.
    pub items: u8,
    /// The overview map's texture and its dots: centre (frame px) and colour.
    pub map: Option<String>,
    pub dots: Vec<([f32; 2], Rgba)>,
}

/// Race's item icons go left from the weapon icon's place this far apart (V hud-h230: three
/// missiles at 232, 214, 196 in the split view).
pub const RACE_ITEM_STEP: f32 = 18.0;
/// The place, centred on the view (`func_15042D94(centre, top + 5, 0xC1, "%d", …)` with its
/// suffix), "LAP l/n" (left + 0x1D, top + 0x13), the race clock (`func_150916B4(centre, bottom −
/// 12)`) and the last lap (left + 0x17, top + 0x1E): V hud-b2's "1st", "LAP 1/1", "0:00:00".
const RACE_PLACE: [i16; 2] = [146, 5];
const RACE_LAP: [i16; 2] = [31, 19];
const RACE_CLOCK: [i16; 2] = [146, 204];
const RACE_LAST: [i16; 2] = [25, 30];
/// The overview map: 64 × 64 at the view's top right, its texels × (255, 255, 255, 0x80) (V
/// hud-b2 [226, 13]); the dots 2 × 2 around a point off its centre (right − 32, top + 45).
const RACE_MAP: [f32; 4] = [226.0, 13.0, 64.0, 64.0];
const RACE_DOT: f32 = 2.0;
const RACE_MAP_CENTRE: [f32; 2] = [258.0, 45.0];
/// The overview map's transform: f32 `0x8009DE78` and `0x8009DE74` (quarter pixels), f32
/// `0x8009DE6C` (quarter pixels a unit) (V cd 600: 3.6, 3.4, 0.013; the own dot to the quarter
/// pixel at cd 600 and res 225).
const RACE_MAP_OFFSET: [f32; 2] = [3.6, 3.4];
const RACE_MAP_SCALE: f32 = 0.013;
/// Race's STATS (phase 6, in the level; `func_151EA15C`, C; V res 320): each human's label at
/// (72, 70 + 18k), ": place" centred on x 88 and its times on the label's top + 2: the total at
/// 138, the best lap at 188 between "(" at 168 and ")" at 214.
const RACE_LABEL: [i16; 2] = [72, 70];
const RACE_ROW: i16 = 18;

/// War: Heist's strip: each team's bags, the score limit (the pips per team), whether a team has a
/// participant with lives (else its skull), and the carrying team while the blink shows the "+1"
/// pip and the bag icon ([`flag_icons`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bags {
    pub scores: [u16; 4],
    pub limit: u8,
    pub alive: [bool; 4],
    pub carrying: Option<u8>,
}

/// A results row ([`rows`]): its colour, an index of `results.colours`, and its columns: place,
/// accuracy, kills, headshots, deaths, and Heist's bags (drawn second there).
pub type Row = (usize, [u32; 6]);

/// A mounted gun's sight ([`Hud::gun`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sight {
    /// The view's heading (obj+0x40: the atan2(x, z) of its direction) and its elevation above
    /// the horizontal, in degrees.
    pub heading: f32,
    pub elevation: f32,
    /// The zoom, 0 opened to 1 at the gun's closest.
    pub zoom: f32,
    /// How far the handles slide down with the zoom (`func_151CB970`, `func_151CC1D4`, C: 3 a
    /// degree of the view's mean zoom, clamped to 25; the one-view constants -25 and 0.04).
    pub slide: f32,
    /// Frames since the zoom bar went red ([`bar_colour`]).
    pub bar_red: Option<u32>,
    /// Each barrel's muzzle flash this frame ([`gun_fire`]): its size and alpha.
    pub flash: [Option<([f32; 2], u8)>; 2],
    /// Each handle's kick ([`kick`]), [0] the left and [1] the right: how far it drops, and its
    /// lit picture's alpha (the handle's is 255 less).
    pub kick: [(f32, u8); 2],
}

/// A handle's kick `k` frames after its barrel's shot (`func_151CB970`, C; the shot's message
/// reaches it the next frame): it drops 21 × the sine of a phase stepping 0.505 a frame, until the
/// phase reaches π (6 frames); its lit picture's alpha is 255 − v, v from 0 closing 0.676 of its gap
/// to 255 a frame, in f32 (lit 1 from the 6th frame to the 16th, 0 after).
pub fn kick(k: u32) -> (f32, u8) {
    const STEP: f32 = 0.505;
    if k == 0 {
        return (0.0, 0);
    }
    let end = STEP * k as f32;
    let drop = if end < core::f32::consts::PI {
        21.0 * (end - STEP).sin()
    } else {
        0.0
    };
    let mut v = 0.0_f32;
    for _ in 1..k.min(32) {
        v += (255.0 - v) * f32::from_bits(0x3F2D_0E57);
    }
    (drop, 255 - v as u8)
}

/// The weight that blends a colour of display luminance `l` (0-1) at the N64's `alpha` in Bevy's
/// blend as the console's does: its RDP mixes the gamma-encoded framebuffer, Bevy the linear one,
/// where a faint alpha shows far brighter (the gun flash's glow, alpha 1-25, made its quad a pale
/// box). Exact for black over anything (`1 − lin(1 − a)`: the fades) and for white over black
/// (`lin(a)`: the flash over the night), between them by `l` (I: a bright overlay over a bright
/// scene comes out fainter than the console's).
pub fn display_alpha(l: f32, alpha: f32) -> f32 {
    let lin = Srgba::gamma_function;
    l * lin(alpha) + (1.0 - l) * (1.0 - lin(1.0 - alpha))
}

/// A display colour's luminance, 0-1.
fn luminance([r, g, b]: [u8; 3]) -> f32 {
    (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0
}

/// The HUD's pictures blend as the console's: each texel's alpha by [`display_alpha`], once loaded
/// (the pack's are all RGBA).
fn display_space(
    mut events: MessageReader<AssetEvent<Image>>,
    a: Option<Res<HudAssets>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(a) = a else {
        return;
    };
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        let mut ours = (a.glyphs.iter()).chain(a.textures.values().map(|t| &t.0));
        if ours.any(|h| h.id() == *id)
            && let Some(mut image) = images.get_mut(*id)
            && let Some(data) = image.data.as_mut()
        {
            for p in data.as_chunks_mut::<4>().0 {
                let alpha = display_alpha(luminance([p[0], p[1], p[2]]), f32::from(p[3]) / 255.0);
                p[3] = (alpha * 255.0).round() as u8;
            }
        }
    }
}

/// A flash's alpha `k` frames after its shot (`func_15154C90`, C): its `life` VIs (5-8) lose 2 a
/// frame from the shot's own; it shows while any are left, at most 0x55 a VI over its last 2.
pub fn flash_alpha(alpha: u8, life: u32, k: u32) -> Option<u8> {
    let left = life.checked_sub(2 * k.min(8) + 2)?;
    Some(if left < 3 {
        alpha.min(0x55 * left as u8)
    } else {
        alpha
    })
}

/// The zoom bar's colour `red` frames after it went red, 16 frames after the zoom stopped
/// (`func_151CBC60`, `func_151CC2BC`, C: `bar_flash` back to `bar_colour` over 35 frames).
pub fn bar_colour(n: &conker_assets::hud::Gun, red: Option<u32>) -> Rgba {
    match red {
        Some(k) if k < 35 => std::array::from_fn(|i| {
            let [from, to] = [n.bar_flash[i], n.bar_colour[i]].map(f32::from);
            (to + (from - to) * ((35 - k) as f32 * (1.0 / 35.0))) as u8
        }),
        _ => n.bar_colour,
    }
}

/// The sight's flashes and handle kicks at `tick`, from each barrel's last shot (the tick this
/// client saw it, the gun's shot count then). A shot flashes from its frame (`func_150FB8C0`, C:
/// barrel 0's at the flash's centre, on the right, barrel 1's mirrored; [`flash_alpha`]) and kicks
/// its side's handle ([`kick`]); the flash's size, alpha and life are dice in their ranges (seeded
/// by the count).
pub fn gun_fire(f: &Flash, last: [Option<(u32, u16)>; 2], tick: u32) -> Sight {
    let mut sight = Sight::default();
    for (b, shot) in last.iter().enumerate() {
        let Some((t, n)) = *shot else {
            continue;
        };
        let k = tick.saturating_sub(t);
        sight.kick[1 - b] = kick(k);
        let mut rng = conker_rules::Rng::new(u64::from(n) << 32 | 0x8128_0784);
        let mut dice = |[lo, hi]: [f32; 2]| lo + rng.next_f32() * (hi - lo);
        let size = [dice(f.width), dice(f.height)];
        let [lo, hi] = f.alpha.map(f32::from);
        let alpha = dice([lo, hi + 0.99]) as u8;
        let [lo, hi] = f.life.map(u32::from);
        let life = lo + rng.next_u32() % (hi - lo + 1);
        sight.flash[b] = flash_alpha(alpha, life, k).map(|alpha| (size, alpha));
    }
    sight
}

/// Total War's results header (`func_151E43DC`): DRAW (both teams 1st); with one human, YOU WIN or
/// YOU LOSE (whether its team won); with more, the winning team's name (0 the squirrels, 1 the
/// Tediz) and WIN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Header {
    Draw,
    You(bool),
    Team(u8),
}

fn image(p: &Picture) -> (Tex, [f32; 4]) {
    let [x, y, w, h] = p.rect.map(f32::from);
    (Tex::Texture(p.texture.clone()), [x, y, x + w, y + h])
}

/// `p` at `rect`, mirrored across x and/or y (`flip`).
fn flipped(
    p: &Picture,
    flip: [bool; 2],
    rect: [f32; 4],
    anchor: Anchor,
    colour: Rgba,
    z: f32,
) -> Sprite {
    let (tex, [x0, y0, x1, y1]) = image(p);
    let (x0, x1) = if flip[0] { (x1, x0) } else { (x0, x1) };
    let (y0, y1) = if flip[1] { (y1, y0) } else { (y0, y1) };
    Sprite {
        tex,
        uv: [x0, y0, x1, y1],
        rect,
        anchor,
        colour,
        z,
    }
}

/// A line of text: (the glyph index, a rank suffix, or a space) per character.
enum Item {
    Glyph(usize),
    Suffix(usize),
    Space,
}

/// Pixels a space moves the pen (V: the outros' subtitles, "YOU DID"'s U ends at 191, D starts at
/// 195; the team strip's `" %d"`).
const SPACE: f32 = 4.0;

/// `line`'s items: its characters with a glyph, and its spaces.
fn items(hud: &Hud, line: &str) -> Vec<Item> {
    let glyph = |c: char| hud.font.iter().position(|g| g.ch == c);
    (line.chars())
        .filter_map(|c| match c {
            ' ' => Some(Item::Space),
            c => glyph(c).map(Item::Glyph),
        })
        .collect()
}

/// An item's width as a line is measured: a glyph's width less 1, without its x offset (V: the
/// outros' lines ending in ".").
fn measure(hud: &Hud, item: &Item) -> f32 {
    match item {
        Item::Glyph(g) => f32::from(hud.font[*g].size[0]) - 1.0,
        Item::Suffix(_) => f32::from(hud.rank.width),
        Item::Space => SPACE,
    }
}

/// How far an item moves the pen as it is drawn: a glyph past its x offset too (V: the outros'
/// "DON'T", the ' at 212 and the T at 214).
fn advance(hud: &Hud, item: &Item) -> f32 {
    match item {
        Item::Glyph(g) => f32::from(hud.font[*g].offset[0]) + measure(hud, item),
        Item::Suffix(_) => f32::from(hud.rank.advance),
        Item::Space => SPACE,
    }
}

/// `line`'s measured width.
fn width(hud: &Hud, line: &str) -> f32 {
    items(hud, line).iter().map(|i| measure(hud, i)).sum()
}

pub(crate) fn text(hud: &Hud, line: &str, suffix: Option<usize>, t: &Text, z: f32) -> Vec<Sprite> {
    let mut items = items(hud, line);
    if let Some(s) = suffix {
        // The rank: "(1" suffix ")", or "1" suffix.
        let close = line.ends_with(')').then(|| items.pop()).flatten();
        items.push(Item::Suffix(s));
        items.extend(close);
    }
    let rank = &hud.rank;
    let width: f32 = items.iter().map(|i| measure(hud, i)).sum();
    let [x, top] = t.at.map(f32::from);
    let mut pen = match t.align {
        Align::Left => x,
        Align::Centre => x - (width / 2.0).floor(),
    };
    let mut out = Vec::new();
    for item in &items {
        let (tex, uv, rect) = match item {
            Item::Space => {
                pen += SPACE;
                continue;
            }
            Item::Glyph(g) => {
                let glyph = &hud.font[*g];
                let [w, h] = glyph.size.map(f32::from);
                let [dx, dy] = glyph.offset.map(f32::from);
                (Tex::Glyph(*g), [0.0, 0.0, w, h], [pen + dx, top + dy, w, h])
            }
            Item::Suffix(s) => {
                let [u, v, w, h] = rank.texels.map(f32::from);
                let [dx, dy] = rank.offset.map(f32::from);
                let tex = Tex::Texture(rank.suffixes[*s].clone());
                (tex, [u, v, u + w, v + h], [pen + dx, top + dy, w, h])
            }
        };
        if let Some(shadow) = t.shadow {
            let [x, y, w, h] = rect;
            let rect = [x + 1.0, y + 1.0, w, h];
            let (tex, anchor) = (tex.clone(), t.anchor);
            out.push(Sprite {
                tex,
                uv,
                rect,
                anchor,
                colour: shadow,
                z: z - 0.5,
            });
        }
        out.push(Sprite {
            tex,
            uv,
            rect,
            anchor: t.anchor,
            colour: t.colour,
            z,
        });
        pen += advance(hud, item);
    }
    out
}

/// A race time (`func_150916B4`, C): `vis` VIs (capped at 0x57030) as minutes, ":ss" and ":cc"
/// (hundredths: the VIs past the second × 100 / 60), each left-aligned, at x − 16 (5 more left from
/// 10 minutes), x − 8 and x + 8, white over a black shadow (V hud-b2's "0:00:00" at 130, 138, 154);
/// none (negative) "--", ": --", ": --".
fn race_time(hud: &Hud, vis: i32, [x, y]: [i16; 2], anchor: Anchor, z: f32) -> Vec<Sprite> {
    let vis = vis.min(0x57030);
    let (secs, cc) = (vis / 60, (vis % 60 * 100 / 60).min(99));
    let wide = if secs / 60 >= 10 { 5 } else { 0 };
    let parts = match vis {
        0.. => [
            (secs / 60).to_string(),
            format!(":{:02}", secs % 60),
            format!(":{cc:02}"),
        ],
        _ => ["--".into(), ": --".into(), ": --".into()],
    };
    let mut out = Vec::new();
    for (part, at) in parts.iter().zip([x - 16 - wide, x - 8, x + 8]) {
        let t = Text {
            anchor,
            at: [at, y],
            align: Align::Left,
            colour: [255; 4],
            shadow: Some([0, 0, 0, 255]),
        };
        out.extend(text(hud, part, None, &t, z));
    }
    out
}

/// A picture drawn four times around `centre`, mirrored: `p` is the top-left quarter (its centre
/// at its bottom-right corner), each quarter `size` wide.
fn quarters(
    p: &Picture,
    centre: [f32; 2],
    size: f32,
    anchor: Anchor,
    colour: Rgba,
    z: f32,
) -> Vec<Sprite> {
    let (tex, [x0, y0, x1, y1]) = image(p);
    let [cx, cy] = centre;
    [(false, false), (true, false), (false, true), (true, true)]
        .into_iter()
        .map(|(right, below)| Sprite {
            tex: tex.clone(),
            uv: [
                if right { x1 } else { x0 },
                if below { y1 } else { y0 },
                if right { x0 } else { x1 },
                if below { y0 } else { y1 },
            ],
            rect: [
                if right { cx } else { cx - size },
                if below { cy } else { cy - size },
                size,
                size,
            ],
            anchor,
            colour,
            z,
        })
        .collect()
}

fn solid(rect: [f32; 4], anchor: Anchor, colour: Rgba, z: f32) -> Sprite {
    Sprite {
        tex: Tex::Solid,
        uv: [0.0; 4],
        rect,
        anchor,
        colour,
        z,
    }
}

fn quad(p: &Picture, rect: [f32; 4], anchor: Anchor, colour: Rgba, z: f32) -> Sprite {
    let (tex, uv) = image(p);
    Sprite {
        tex,
        uv,
        rect,
        anchor,
        colour,
        z,
    }
}

/// The rank suffix of `n` (st, nd, rd, th): 1st, 2nd, 3rd, 4th…, 11th-13th.
fn suffix(n: usize) -> usize {
    match (n % 10, n % 100 / 10) {
        (d @ 1..=3, tens) if tens != 1 => d - 1,
        _ => 3,
    }
}

/// The results' rows: one per human player (bots are not listed), in slot order, from everyone's
/// (human, [`Combat`]): its colour (an index of `results.colours`: the row's; in Total War its
/// team's, `func_151EA15C`), then place, accuracy (hits × 100 / shots, truncated, at most 9999, 0
/// without a shot), kills, headshots, deaths (`func_151E43DC`, V: the oracle's STATS). In Total
/// War the place is the team's (`0x800E0AC0[team]`), from the server's `winner` (`MatchView`;
/// `None` a DRAW: both 1st). War: Beach lists no villager (`func_151EA15C` on scenario 8: only rows
/// of a team other than 0; V cs4, cs3, cs4h). War: Heist's place is its team's among the four
/// (`teams_places`, ties sharing it: V cs3b's REDS and BLUES both 2nd), then the bags delivered.
pub fn rows(
    all: &[(bool, &Combat)],
    kill_limit: u8,
    total_war: bool,
    winner: Option<u8>,
    beach: bool,
    heist: bool,
) -> Vec<Row> {
    let mut standings = vec![
        None;
        all.iter()
            .map(|p| usize::from(p.1.slot) + 1)
            .max()
            .unwrap_or(0)
    ];
    for (_, c) in all {
        standings[usize::from(c.slot)] = Some(standing(c));
    }
    let place = if heist {
        let teams = teams_places(&standings, 4);
        let of = |s: &Standing| teams.get(usize::from(s.team)).copied().unwrap_or(0);
        standings.iter().map(|s| s.as_ref().map_or(0, of)).collect()
    } else if total_war {
        let of = |s: &Standing| 1 + u8::from(winner.is_some_and(|w| w != s.team));
        standings.iter().map(|s| s.as_ref().map_or(0, of)).collect()
    } else {
        places(&standings, kill_limit)
    };
    let listed =
        |p: &&(bool, &Combat)| p.0 && !(beach && p.1.team == conker_rules::beach::VILLAGERS);
    let mut humans: Vec<_> = all.iter().filter(listed).map(|p| p.1).collect();
    humans.sort_by_key(|c| c.slot);
    (humans.into_iter().enumerate())
        .map(|(n, c)| {
            let accuracy = (u32::from(c.hits) * 100)
                .checked_div(u32::from(c.shots))
                .unwrap_or(0);
            let colour = if total_war { usize::from(c.team) } else { n };
            let row = [
                u32::from(place[usize::from(c.slot)]),
                accuracy.min(9999),
                u32::from(c.kills),
                u32::from(c.headshots),
                u32::from(c.deaths),
                u32::from(c.bags),
            ];
            (colour, row)
        })
        .collect()
}

/// Total War's results header from everyone's (human, [`Combat`]), the viewer's team and the
/// server's `winner` (`None` a DRAW): the original's rule by the number of humans
/// (`func_151E43DC`; V `to_draw`, `you_win`, `you_lose`, `sq_win`:
/// `lab/research/mp-original/total-war-teams.md` §4, §6).
pub fn header(all: &[(bool, &Combat)], own_team: u8, winner: Option<u8>) -> Header {
    match winner {
        None => Header::Draw,
        Some(t) if all.iter().filter(|p| p.0).count() == 1 => Header::You(t == own_team),
        Some(t) => Header::Team(t),
    }
}

/// Total War's team strip (`func_151E9D18`), or War: Beach's (`t`): each team's icon (`lives`:
/// untimed) and number, kept at `anchor`, at `z`.
fn strip(
    hud: &Hud,
    t: &Strip,
    numbers: [u32; 2],
    lives: bool,
    anchor: Anchor,
    z: f32,
) -> Vec<Sprite> {
    let icons = if lives { &t.lives } else { &t.kills };
    (0..2)
        .flat_map(|team| {
            let rect = t.rects[team].map(f32::from);
            let mut out = vec![quad(&icons[team], rect, anchor, [255; 4], z)];
            let number = Text {
                anchor,
                ..t.numbers[team].clone()
            };
            out.extend(text(hud, &numbers[team].to_string(), None, &number, z));
            out
        })
        .collect()
}

/// The chosen menu button's lit tiles' alpha over its dim ones (`func_15043384`, C; V t5_b, t5_r
/// env alphas 0x7E, 0x6E, 0x9B, 0xCE, 0x3E, 0xCB): the STATS' alpha `a` × (3v + 63) / 256, v a
/// triangle of the VI counter `vi` (`0x800BE9AC`, 2 a frame) over 32 frames: 62 to 251 at a = 255.
pub fn glow(a: u8, vi: u32) -> u8 {
    let t = (2 * vi) & 0x7F;
    let v = if t < 64 { t } else { 127 - t };
    ((u32::from(a) * (3 * v + 63)) >> 8).min(255) as u8
}

/// The stick's frame between the buttons (sprite 0x1A: tile 0x7E7 + frame), one step a frame
/// there and back over 10 frames: n = (`0x80085CD0` >> 1) % 10, 10 − n from 6 (V t5_b,
/// `dm-maps-results-scratch/results/stick.py`; I: that counter is taken as `vi`, its phase not
/// read).
pub fn stick(vi: u32) -> u32 {
    let n = (vi >> 1) % 10;
    if n < 6 { n } else { 10 - n }
}

/// The results over the frozen level: the header, the columns' icons, a row per human, the menu's
/// choice other than `choice` and the stick at its frame (`vi`: [`stick`]); in Total War
/// (`header`) its header over the rest, which goes `down` lower; War: Heist's (`heist`) with its
/// own icons, label and columns, the bags second ([`HeistResults`]). The chosen button is
/// [`sprites`]'.
#[allow(clippy::too_many_arguments)]
fn results(
    hud: &Hud,
    end: &End,
    rows: &[Row],
    (choice, vi): (usize, u32),
    header: Option<Header>,
    beach: bool,
    heist: bool,
    race: Option<&[[i32; 3]]>,
) -> Vec<Sprite> {
    let (r, t, h) = (&end.results, &end.teams, &end.heist);
    let (icons, label_at) = if heist {
        (&h.icons, h.label)
    } else {
        (&r.icons, r.label)
    };
    // Each column's x and its value's index in the row.
    let columns: Vec<(i16, usize)> = if heist {
        h.columns.into_iter().zip([0, 5, 1, 2, 3, 4]).collect()
    } else {
        r.columns.into_iter().zip(0..).collect()
    };
    let white = [255; 4];
    let rect = |r: [i16; 4]| r.map(f32::from);
    let down = if header.is_some() { t.down } else { 0 };
    let team_header: &[Placed] = match header {
        None => &[],
        Some(Header::Draw) => std::slice::from_ref(&t.draw),
        Some(Header::You(true)) => &t.won,
        Some(Header::You(false)) => &t.lost,
        Some(Header::Team(team)) if heist => h.won.get(usize::from(team)).map_or(&[], |p| p),
        Some(Header::Team(team)) => {
            let won = if beach { &t.beach_won } else { &t.team_won };
            won.get(usize::from(team)).map_or(&[], |p| p)
        }
    };
    let mut out: Vec<Sprite> = (std::iter::once(&r.header).chain(team_header))
        .map(|p| quad(&p.image, rect(p.rect), p.anchor, white, 5.0))
        .collect();
    let p = &r.stick;
    let mut s = quad(&p.image, rect(p.rect), p.anchor, white, 5.0);
    let dx = 32.0 * stick(vi) as f32;
    (s.uv[0], s.uv[2]) = (s.uv[0] + dx, s.uv[2] + dx);
    out.push(s);
    for p in icons.iter().filter(|_| race.is_none()) {
        let [x, y, w, h] = p.rect;
        out.push(quad(
            &p.image,
            rect([x, y + down, w, h]),
            p.anchor,
            white,
            5.0,
        ));
    }
    let b = if choice == 0 { &r.quit } else { &r.restart };
    out.push(quad(&b.image, rect(b.rect), b.anchor, white, 5.0));
    // More rows than the original's 4 humans: the 4 rows' height shared out, each row scaled down
    // to fit (around its label's and each column's top), above the menu.
    let s = (4.0 / rows.len() as f32).min(1.0);
    let shrink = |sprites: Vec<Sprite>, [px, py]: [i16; 2]| {
        let [px, py] = [px, py].map(f32::from);
        sprites.into_iter().map(move |mut sp| {
            let [x, y, w, h] = sp.rect;
            sp.rect = [px + (x - px) * s, py + (y - py) * s, w * s, h * s];
            sp
        })
    };
    if let Some(race) = race {
        let s = (4.0 / race.len() as f32).min(1.0);
        for (n, &[place, total, best]) in race.iter().enumerate() {
            let [x, y] = RACE_LABEL;
            let at = [x, y + (f32::from(RACE_ROW) * s * n as f32) as i16];
            let scale = |sprites: Vec<Sprite>| {
                sprites.into_iter().map(move |mut sp| {
                    let [px, py] = at.map(f32::from);
                    let [x, y, w, h] = sp.rect;
                    sp.rect = [px + (x - px) * s, py + (y - py) * s, w * s, h * s];
                    sp
                })
            };
            if let Some(label) = r.labels.get(n) {
                let [_, _, w, h] = label.rect;
                let q = quad(
                    label,
                    rect([at[0], at[1], w as i16, h as i16]),
                    r.anchor,
                    white,
                    5.0,
                );
                out.extend(scale(vec![q]));
            }
            let top = at[1] + 2;
            let t = |x: i16, align: Align| Text {
                anchor: r.anchor,
                at: [x, top],
                align,
                colour: white,
                shadow: Some(r.shadow),
            };
            let place = usize::try_from(place).unwrap_or(0);
            let line = format!("       : {place}");
            let mut row = text(hud, &line, Some(suffix(place)), &t(88, Align::Centre), 5.0);
            row.extend(race_time(hud, total, [138, top], r.anchor, 5.0));
            row.extend(text(hud, "(", None, &t(168, Align::Left), 5.0));
            row.extend(race_time(hud, best, [188, top], r.anchor, 5.0));
            row.extend(text(hud, ")", None, &t(214, Align::Left), 5.0));
            out.extend(scale(row));
        }
        return out;
    }
    for (n, (colour, row)) in rows.iter().enumerate() {
        let dy = (f32::from(r.rows[1]) * s * n as f32) as i16 + down;
        let colour = r.colours.get(*colour).copied().unwrap_or(white);
        let [x, y] = label_at;
        let at = [x, y + dy];
        if let Some(label) = r.labels.get(n) {
            let [_, _, w, h] = label.rect;
            let q = quad(
                label,
                rect([x, y + dy, w as i16, h as i16]),
                r.anchor,
                white,
                5.0,
            );
            out.extend(shrink(vec![q], at));
        } else {
            // The pack's labels stop at P4 (the original's 4 humans): P5 on in the HUD's font, the
            // row's colour, on the row's text top.
            let t = Text {
                anchor: r.anchor,
                at: [x + 1, r.rows[0] + dy],
                align: Align::Left,
                colour,
                shadow: Some(r.shadow),
            };
            out.extend(shrink(text(hud, &format!("P{}", n + 1), None, &t, 5.0), at));
        }
        for &(x, column) in &columns {
            let v = row[column];
            let t = Text {
                anchor: r.anchor,
                at: [x, r.rows[0] + dy],
                align: Align::Centre,
                colour,
                shadow: Some(r.shadow),
            };
            let (line, suffix) = match column {
                0 => (v.to_string(), Some(suffix(v as usize))),
                1 => (format!("{v}%"), None),
                _ => (v.to_string(), None),
            };
            out.extend(shrink(text(hud, &line, suffix, &t, 5.0), t.at));
        }
    }
    out
}

/// Total War's gassed screen `k` frames in (object 0xE013, draw `func_151D2830`, update
/// `func_151D275C`): each of [`Haze::layers`] over the whole view (the iris's `far` margin around the
/// frame), its texels from the frame's top-left scrolling k/4 a frame, under the HUD; alpha min(4k,
/// 128) up to k = 132, then 8 less a frame, times the layer's share.
fn haze(h: &Haze, k: u32) -> Vec<Sprite> {
    let alpha = (4 * k)
        .min(128)
        .min(128u32.saturating_sub(8 * k.saturating_sub(132)));
    let (tex, far, s) = (image(&h.image).0, 10_000.0, k as f32 / 4.0);
    (h.layers.iter().zip([0.5, 0.6]))
        .map(|(l, z)| {
            let [r, g, b] = l.colour;
            let (lo, hi) = (-far * l.texel, far * l.texel);
            Sprite {
                tex: tex.clone(),
                uv: [lo + s, lo + l.t, hi + s, hi + l.t],
                rect: [-far, -far, 2.0 * far, 2.0 * far],
                anchor: Anchor(Horizontal::Centre, Vertical::Middle),
                colour: [r, g, b, (alpha as f32 * l.share) as u8],
                z,
            }
        })
        .collect()
}

/// A War outro's picture at frame `k` ([`Outro`]), in place of the HUD: its fade over the whole
/// view and, over it for its first frames, the match's end fade still falling 16 a frame from 255
/// two frames before the outro's clock 0 (`func_151ED1E0`, 8 a VI; V `fills_tw4.txt` DL 458: 0xAF);
/// then its speech bubble ([`Outro::bubble`]) where the bubble is then, its quads scaled about the
/// tail's tip ([`tip`]), and its page's lines upper case in black at the bubble's text alpha,
/// left-aligned in a block centred on the text's x around its middle line's top, each line below
/// the last by its own height ([`line_height`]).
pub fn outro(hud: &Hud, o: &Outro, k: u32) -> Vec<Sprite> {
    let t = o.t(k);
    let far = 10_000.0;
    let [r, g, b] = o.fade.rgb;
    let whole = [-far, -far, 2.0 * far, 2.0 * far];
    let centre = Anchor(Horizontal::Centre, Vertical::Middle);
    let mut out = vec![solid(whole, centre, [r, g, b, o.fade(t)], 9.0)];
    let end = 255u32.saturating_sub(16 * (k + 2));
    if end > 0 {
        out.push(solid(whole, centre, [0, 0, 0, end as u8], FADE_Z));
    }
    let (Some(s), Some(bubble)) = (&o.subtitles, o.bubble(k)) else {
        return out;
    };
    let (anchor, at, quads) = s.place(t);
    let ([tx, ty], scale) = (tip(quads), bubble.scale);
    for (n, q) in quads.iter().enumerate() {
        let [x, y, w, h] = q.rect.map(f32::from);
        out.push(Sprite {
            tex: Tex::Texture(q.texture.clone()),
            uv: q.uv,
            rect: [
                tx + (x - tx) * scale,
                ty + (y - ty) * scale,
                w * scale,
                h * scale,
            ],
            anchor,
            colour: q.colour,
            z: 5.0 + 0.1 * n as f32,
        });
    }
    if bubble.alpha == 0 {
        return out;
    }
    let lines: Vec<String> = (bubble.page.lines.iter())
        .map(|l| l.to_uppercase())
        .collect();
    let widest = lines.iter().map(|l| width(hud, l)).fold(0.0, f32::max);
    let [x, y] = at.map(f32::from);
    let mut top = y - 6.0 * (lines.len() - 1) as f32;
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            top += line_height(hud, line);
        }
        let t = Text {
            anchor,
            at: [(x - (widest / 2.0).ceil()) as i16, top as i16],
            align: Align::Left,
            colour: [0, 0, 0, bubble.alpha],
            shadow: None,
        };
        out.extend(text(hud, line, None, &t, 6.0));
    }
    out
}

/// How far a bubble's line sits below the one before (`func_150428D4`): its tallest glyph's top
/// offset and height, at least 12 if it holds a space (V: TW cs4's "ABOVE-AVERAGE" 11).
fn line_height(hud: &Hud, line: &str) -> f32 {
    (items(hud, line).iter())
        .map(|i| match i {
            Item::Glyph(g) => f32::from(hud.font[*g].offset[1]) + f32::from(hud.font[*g].size[1]),
            Item::Suffix(_) => 0.0,
            Item::Space => 12.0,
        })
        .fold(0.0, f32::max)
}

/// The HUD's quads for `s`, in the frame (the original's draws: `lab/hud/README.md`).
pub fn sprites(hud: &Hud, s: &Shown) -> Vec<Sprite> {
    // Untimed matches count lives (the lives digit, the strip's lives); Colors' strip has the
    // heads too (HUD flag 0x2000: `func_151E9D18`'s icon row 0, C; V e0 DL 360).
    let untimed = s.lives.is_some();
    let heads = untimed || s.flags.is_some();
    let white = [255; 4];
    let raptor = s.raptor.map(|_| raptor_strip(&hud.beach.strip));
    let team_strip = match &raptor {
        _ if s.beach => &hud.beach.strip,
        Some(r) => r,
        None => &hud.strip,
    };
    if let (Some((rows, choice, alpha, vi)), Some(end)) = (&s.results, &hud.end) {
        let mut out = results(
            hud,
            end,
            rows,
            (*choice, *vi),
            s.header,
            s.beach,
            s.heist.is_some(),
            s.race_rows.as_deref(),
        );
        // War: the strip under the rows, live (`func_151EA15C`; V 0x800E0BD3 = 2).
        if let Some(n) = s.strip {
            out.extend(strip(hud, team_strip, n, heads, end.results.anchor, 5.0));
        }
        for q in &mut out {
            q.colour[3] = (u32::from(q.colour[3]) * u32::from(*alpha) / 255) as u8;
        }
        // The chosen button: its dim tiles opaque even while the STATS fade in, its lit ones
        // pulsing over them ([`glow`]; V t5_b, t5_r: env alpha 0xFF under them in every dump).
        let b = if *choice == 0 {
            &end.results.restart
        } else {
            &end.results.quit
        };
        let at = b.rect.map(f32::from);
        out.push(quad(&b.image, at, b.anchor, white, 5.0));
        let lit = [255, 255, 255, glow(*alpha, *vi)];
        out.push(quad(&b.lit, at, b.anchor, lit, 5.1));
        return out;
    }
    let mut out = s.haze.map_or_else(Vec::new, |k| haze(&hud.haze, k));
    // A fade over everything, the iris included: the original fills the view after the HUD (V c1a
    // DL 456: the radar, heads, clock and S digits, then the fill).
    if let Some(alpha) = s.fade.filter(|&a| a > 0) {
        out.push(black(alpha, FADE_Z));
    }
    // Untimed: the lives left; out of them, the skull.
    if let (Some(n), Some(end)) = (s.lives, &hud.end) {
        let l = &end.lives;
        let digit = &l.digits[usize::from(n).min(9)];
        let [_, _, w, h] = digit.rect.map(f32::from);
        let [x, y] = l.at.map(f32::from);
        out.push(quad(digit, [x, y, w, h], l.anchor, white, 3.0));
    }
    // Out: the view's 3D pass skipped (`func_150195A0`), the iris an opaque black over the HUD and
    // its text, then the skull on its rect's centre (`func_1507DB6C`; V a_solo DL 384: the sky,
    // the lives digit, score and rank, the black, the skull). Only the War strip shows over it (I:
    // the split screen's text list, flushed after the iris, V t1d).
    if let (Some(half), Some(end)) = (s.skull, &hud.end) {
        let k = &end.skull;
        out.push(black(255, OUT_Z));
        let [x, y, w, h] = k.rect.map(f32::from);
        let (cx, cy, half) = (x + w / 2.0, y + h / 2.0, f32::from(half));
        let rect = [cx - half, cy - half, 2.0 * half, 2.0 * half];
        out.push(quad(&k.image, rect, k.anchor, white, OUT_Z + 0.5));
    }
    // Radar: the disc, then a blip per player in range, with a bar on another level's.
    let r = &hud.radar;
    let [cx, cy] = r.centre;
    let half = r.size / 2.0;
    let radar = s.radar && s.skull.is_none() && s.ending.is_none();
    if radar {
        out.push(quad(
            &r.disc,
            [cx - half, cy - half, r.size, r.size],
            r.anchor,
            r.colour,
            1.0,
        ));
    }
    for &([right, ahead], level, colour) in s.blips.iter().filter(|_| radar) {
        // The game adds the offset in quarter pixels, truncated (C `func_15093B58`).
        let q = |v: f32| (v * r.scale * 4.0).trunc() / 4.0;
        let (x, y) = (q(right), -q(ahead));
        if x * x + y * y >= r.range * r.range {
            continue;
        }
        let b = r.blip;
        let (bx, by) = (cx + x - b / 2.0, cy + y - b / 2.0);
        out.push(solid([bx, by, b, b], r.anchor, colour, 2.0));
        if level != 0 {
            let y = if level > 0 { by - b / 2.0 } else { by + b };
            out.push(solid([bx, y, b, b / 2.0], r.anchor, r.level_mark, 2.0));
        }
    }
    // Health: ceil(h / 2) pieces on the first row, the rest on the second, ending at `right`.
    let h = &hud.health;
    let [_, _, pw, ph] = h.piece.rect.map(f32::from);
    for (row, n) in [s.health.div_ceil(2), s.health / 2].into_iter().enumerate() {
        for k in 0..n {
            let x = f32::from(h.right) - pw * f32::from(k + 1);
            let rect = [x, f32::from(h.rows[row]), pw, ph];
            out.push(quad(&h.piece, rect, h.anchor, white, 3.0));
        }
    }
    // The weapon's icon, or the grenades while one is in the hand.
    match s.grenades {
        Some(n) => {
            let g = &hud.grenades;
            let (icon, at) = match (&g.lob_icon, g.lob_at) {
                (Some(i), Some(at)) if s.lob => (i, at),
                _ => (&g.icon, g.at),
            };
            let [_, _, w, h] = icon.rect.map(f32::from);
            for k in 0..n {
                let x = f32::from(at[0] + g.step * i16::from(k));
                out.push(quad(
                    icon,
                    [x, f32::from(at[1]), w, h],
                    g.anchor,
                    white,
                    3.0,
                ));
            }
        }
        None => {
            let w = &hud.weapon;
            let power = (TANK_ICONS.iter().find(|i| s.tank && i.0 == s.weapon)).map(|i| Picture {
                texture: "atlas2".into(),
                rect: i.1,
            });
            let icon = match s.tank {
                true => power.as_ref(),
                false => w.icons.get(&s.weapon),
            };
            // Race: one icon per item held. The club's is drawn a pixel right of the others, the
            // bat's a pixel lower (`func_150918EC`, C).
            let n = s.race.as_ref().map_or(1, |r| r.items);
            let nudge = match (s.tank, s.weapon) {
                (false, 0x40) => [1.0, 0.0],
                (false, 0x19) => [0.0, 1.0],
                _ => [0.0; 2],
            };
            for (k, icon) in icon.into_iter().cycle().take(n.into()).enumerate() {
                let [_, _, iw, ih] = icon.rect.map(f32::from);
                let [x, y] = [0, 1].map(|j| f32::from(w.at[j]) + nudge[j]);
                let x = x - RACE_ITEM_STEP * k as f32;
                out.push(quad(icon, [x, y, iw, ih], w.anchor, white, 3.0));
            }
        }
    }
    // Kills and rank (HUD flags 0x400, 0x800; none in Heist), or Total War's team strip (0x200
    // timed, 0x100 untimed: C `func_150918EC`, `func_151E89A0`), and the clock.
    match s.strip {
        Some(n) => {
            // Out: over the black, its numbers' shadows too.
            let z = if s.skull.is_some() { OUT_Z + 1.0 } else { 5.0 };
            out.extend(strip(hud, team_strip, n, heads, team_strip.anchor, z));
        }
        None if s.ending.is_none() && s.heist.is_none() && s.race.is_none() => {
            out.extend(text(hud, &s.kills.to_string(), None, &hud.score, 5.0));
            out.extend(text(
                hud,
                &format!("({})", s.rank),
                Some(suffix(s.rank)),
                &hud.rank.text,
                5.0,
            ));
        }
        None => {}
    }
    // Colors: each carried flag's icon (HUD flag 0x8000), blinking.
    for (f, _) in s.flags.iter().flatten().enumerate().filter(|f| *f.1) {
        let p = &hud.flags[f];
        out.push(quad(&p.image, p.rect.map(f32::from), p.anchor, white, 5.0));
    }
    // Heist: each team's pips (the "+1" blinking while it carries) or, with no participant left,
    // its skull; the bag's icon with the "+1" (`func_151E966C`; V hud1 DL 341, hud2 DL 300).
    if let Some(b) = &s.heist {
        let h = &hud.heist;
        let size = |p: &Picture| [p.rect[2] as i16, p.rect[3] as i16];
        let ([pw, ph], [sw, sh]) = (size(&h.pip), size(&h.skull));
        for (t, &colour) in h.colours.iter().enumerate() {
            let c = h.centre + h.step * t as i16;
            if !b.alive[t] {
                let rect = [c - sw / 2, h.y - 6, sw, sh].map(f32::from);
                out.push(quad(&h.skull, rect, h.anchor, colour, 5.0));
                continue;
            }
            let filled = b.scores[t] + u16::from(b.carrying == Some(t as u8));
            let limit = i16::from(b.limit);
            for i in 0..limit {
                let x = c - h.pip_step / 2 * limit + h.pip_step * i;
                let colour = if i < filled as i16 { colour } else { h.empty };
                let rect = [x, h.y, pw, ph].map(f32::from);
                out.push(quad(&h.pip, rect, h.anchor, colour, 5.0));
            }
        }
        if b.carrying.is_some() {
            let p = &h.bag;
            out.push(quad(&p.image, p.rect.map(f32::from), p.anchor, white, 5.0));
        }
    }
    // Raptor: the carried egg's and the held body's icons.
    let top = Anchor(Horizontal::Centre, Vertical::Top);
    for (&(texture, rect, at), _) in RAPTOR_ICONS
        .iter()
        .zip(s.raptor.iter().flatten())
        .filter(|i| *i.1)
    {
        let p = Picture {
            texture: texture.into(),
            rect,
        };
        out.push(quad(&p, at.map(f32::from), top, white, 5.0));
    }
    // Race: the overview map and its dots; while racing, the place, the lap and the race clock;
    // the last lap.
    if let Some(r) = &s.race {
        let right = Anchor(Horizontal::Right, Vertical::Top);
        let left = Anchor(Horizontal::Left, Vertical::Top);
        if let Some(map) = &r.map {
            let p = Picture {
                texture: map.clone(),
                rect: [0, 0, 64, 64],
            };
            out.push(quad(&p, RACE_MAP, right, [255, 255, 255, 0x80], 4.0));
            let d = RACE_DOT / 2.0;
            for &([x, y], colour) in &r.dots {
                out.push(solid(
                    [x - d, y - d, RACE_DOT, RACE_DOT],
                    right,
                    colour,
                    4.5,
                ));
            }
        }
        if i16::from(r.laps) > r.lap {
            let place = usize::from(r.place.max(1));
            let t = Text {
                anchor: top,
                at: RACE_PLACE,
                align: Align::Centre,
                colour: white,
                shadow: Some([0, 0, 0, 255]),
            };
            out.extend(text(hud, &place.to_string(), Some(suffix(place)), &t, 5.0));
            let lap = format!("LAP {}/{}", r.lap.max(0) + 1, r.laps);
            let t = Text {
                anchor: left,
                at: RACE_LAP,
                ..t
            };
            out.extend(text(hud, &lap, None, &t, 5.0));
            let bottom = Anchor(Horizontal::Centre, Vertical::Bottom);
            out.extend(race_time(hud, r.time, RACE_CLOCK, bottom, 5.0));
        }
        if r.last > 0 {
            out.extend(race_time(hud, r.last, RACE_LAST, left, 5.0));
        }
    }
    if let Some(secs) = s.secs.filter(|_| s.ending.is_none_or(|k| k < 2)) {
        let clock = format!("{}:{:02}", secs / 60, secs % 60);
        out.extend(text(hud, &clock, None, &hud.clock, 5.0));
    }
    if let Some(n) = s.countdown {
        out.extend(text(hud, &n.to_string(), None, &hud.countdown, 5.0));
    }
    if let Some(n) = s.escape {
        out.extend(text(hud, &n.to_string(), None, &hud.beach.escape, 5.0));
    }
    // The pickup prompt, under the view's centre (the crosshair's), white on a shadow.
    if let Some(line) = &s.prompt {
        let t = Text {
            anchor: Anchor(Horizontal::Centre, Vertical::Middle),
            at: [(hud.frame[0] / 2) as i16, (hud.frame[1] / 2) as i16 + 40],
            align: Align::Centre,
            colour: white,
            shadow: Some([0, 0, 0, 255]),
        };
        out.extend(text(hud, line, None, &t, 5.0));
    }
    // Beach: the strike's flash, the iris's quarters in red then black around the view's centre.
    if let Some(colour) = s.strike.and_then(strike_flash) {
        let i = &hud.iris;
        out.extend(quarters(
            &i.image,
            STRIKE_CENTRE,
            192.0,
            i.anchor,
            colour,
            8.0,
        ));
    }
    // Aiming: the reticle; through a scope: the ring, its dots and the zoom bar.
    let t = &hud.reticle;
    if s.reticle {
        out.push(quad(
            &t.image,
            t.rect.map(f32::from),
            t.anchor,
            t.colour,
            6.0,
        ));
    }
    if let Some(zoom) = s.scope {
        let c = &hud.scope;
        let [cx, cy] = c.centre;
        out.extend(quarters(
            &c.ring, c.centre, c.quadrant, c.anchor, c.colour, 6.5,
        ));
        // Weapon 0x16: the view copied over itself in strips times (R, 0xFF, 0), after the 3D
        // scene, the health, weapon icon and radar, before the clock, score and ring (C, V w16B
        // DL 230: combiner TEXEL0 × PRIM, no blend); no bar or dots.
        if let Some(red) = s.tint {
            let all = [-10_000.0, -10_000.0, 20_000.0, 20_000.0];
            out.push(Sprite {
                tex: Tex::Tint,
                ..solid(all, c.anchor, [red, 255, 0, 255], 4.0)
            });
        } else {
            out.push(solid(c.back, c.anchor, c.back_colour, 6.0));
            let half = c.bar_half[0] + c.bar_half[1] * zoom.clamp(0.0, 1.0);
            out.push(solid(
                [c.bar_x, cy - half, c.bar_width, 2.0 * half],
                c.anchor,
                c.colour,
                6.5,
            ));
            // Each dot's rect: its centre ± half its size, truncated to quarter pixels (V scopeB
            // DL 230, exact). The tank's scope has none (V pw2 DLs 172-195).
            let (d, q) = (c.dot_size / 2.0, |v: f32| (v * 4.0).trunc() / 4.0);
            for k in (0..c.dots).filter(|_| !s.tank) {
                let a = core::f32::consts::TAU * f32::from(k) / f32::from(c.dots) + s.turn;
                let (x, y) = (cx + c.radius * a.sin(), cy + c.radius * a.cos());
                let [x0, y0] = [q(x - d), q(y - d)];
                let rect = [x0, y0, q(x + d) - x0, q(y + d) - y0];
                out.push(quad(&c.dot, rect, c.anchor, c.colour, 6.5));
            }
        }
    }
    // On a mounted gun (`func_151CC840`, `func_151CC524`, `func_151CB5FC`, `func_151C9F38`): the
    // handles, each over its lit picture, and the flash under the rest of the HUD; the zoom bar,
    // the sight and its tapes over it.
    if let Some(g) = &s.gun {
        let n = &hud.gun;
        let a = n.anchor;
        for (h, (drop, lit)) in n.handles.iter().zip(g.kick) {
            let [x, y, w, hh] = h.rect;
            let rect = [x, y + ((g.slide + drop) * 4.0).floor() / 4.0, w, hh];
            out.push(flipped(&n.lit, h.flip, rect, a, [255, 255, 255, lit], 0.2));
            out.push(flipped(
                &h.image,
                h.flip,
                rect,
                a,
                [255, 255, 255, 255 - lit],
                0.3,
            ));
        }
        for (barrel, flash) in g.flash.iter().enumerate() {
            let Some(([w, h], alpha)) = *flash else {
                continue;
            };
            let f = &n.flash;
            let [x, y] = f.centre;
            let x = if barrel == 0 {
                x
            } else {
                f32::from(hud.frame[0]) - x
            };
            let rect = [x - w / 2.0, y - h / 2.0, w, h];
            let flip = [barrel != 0, false];
            out.push(flipped(
                &f.image,
                flip,
                rect,
                a,
                [255, 255, 255, alpha],
                0.4,
            ));
        }
        out.push(solid(n.back, a, n.back_colour, 6.0));
        let (cy, half) = (
            f32::from(hud.frame[1]) / 2.0,
            n.bar_half[0] + n.bar_half[1] * g.zoom.clamp(0.0, 1.0),
        );
        let bar = [n.bar_x, cy - half, n.bar_width, 2.0 * half];
        out.push(solid(bar, a, bar_colour(n, g.bar_red), 6.5));
        for p in &n.sight {
            out.push(flipped(&p.image, p.flip, p.rect, a, n.colour, 6.5));
        }
        // Each tape `repeats` pictures long, `per_degree` × its angle texels in (in 1/32 texels, as
        // the RDP's S10.5, and within a picture: it repeats).
        for (t, angle) in n.tapes.iter().zip([g.heading, g.elevation]) {
            let (tex, [x0, y0, x1, y1]) = image(&t.image);
            let along = t.rect[2] > t.rect[3];
            let period = if along { x1 - x0 } else { y1 - y0 };
            let off = ((t.per_degree * angle * 32.0).round() / 32.0).rem_euclid(period);
            let long = period * t.repeats;
            let uv = match along {
                true => [x0 + off, y0, x0 + off + long, y1],
                false => [x0, y0 + off, x1, y0 + off + long],
            };
            out.push(Sprite {
                tex,
                uv,
                rect: t.rect,
                anchor: a,
                colour: n.tape_colour,
                z: 6.5,
            });
        }
    }
    // Dead: black but for a circle (absent past the iris's list), over everything.
    let i = &hud.iris;
    if let Some(&radius) = s.respawn_in.and_then(|n| i.radius.get(usize::from(n))) {
        let centre = s.iris_at.unwrap_or(i.centre);
        let ([cx, cy], r, far) = (centre, f32::from(radius), 10_000.0);
        let z = 9.0;
        out.extend(quarters(&i.image, centre, r, i.anchor, i.colour, z));
        for rect in [
            [-far, -far, 2.0 * far, far + cy - r],
            [-far, cy + r, 2.0 * far, far],
            [-far, cy - r, far + cx - r, 2.0 * r],
            [cx + r, cy - r, far, 2.0 * r],
        ] {
            out.push(solid(rect, i.anchor, i.colour, z));
        }
    }
    out
}

/// `rect` (frame pixels) in a `window`-sized target: the frame scaled by `window`'s height (its
/// width when narrower), the rectangle keeping its anchor's distance from the window's edge or
/// centre. Window pixels, y down.
pub fn place(frame: [u16; 2], anchor: Anchor, rect: [f32; 4], window: Vec2) -> [f32; 4] {
    let frame = Vec2::new(f32::from(frame[0]), f32::from(frame[1]));
    let s = (window / frame).min_element();
    let [x, y, w, h] = rect;
    let x = match anchor.0 {
        Horizontal::Left => x * s,
        Horizontal::Centre => window.x / 2.0 + (x - frame.x / 2.0) * s,
        Horizontal::Right => window.x - (frame.x - x) * s,
    };
    let y = match anchor.1 {
        Vertical::Top => y * s,
        Vertical::Middle => window.y / 2.0 + (y - frame.y / 2.0) * s,
        Vertical::Bottom => window.y - (frame.y - y) * s,
    };
    [x, y, w * s, h * s]
}

/// A window point (pixels, y down) in frame pixels, for a centred anchor ([`place`]'s inverse).
pub fn frame_point(frame: [u16; 2], window: Vec2, at: Vec2) -> [f32; 2] {
    let frame = Vec2::new(f32::from(frame[0]), f32::from(frame[1]));
    let s = (window / frame).min_element();
    (frame / 2.0 + (at - window / 2.0) / s).to_array()
}

/// The scope dots' turn (radians) at the view's zoom (`cam+0x19C`, `+0x1A0`): their mean F within
/// −50..0, (F + 50) × 0.02 turns (one view's constants; two views' −62 and 1/62; C, V scopeB DL 230).
pub fn dot_turn(zoom: [f32; 2]) -> f32 {
    const LOW: f32 = -50.0;
    let f = ((zoom[0] + zoom[1]) / 2.0).clamp(LOW, 0.0);
    (f - LOW) * 0.02 * core::f32::consts::TAU
}

/// Weapon 0x16's R view: its red factor `n` frames after the scope opened (`func_151A5070`): 0x90,
/// +4 a frame up to 0xFF, −4 a frame back down to 0x90, and again (V w16B 200-229: 0x90, 0x94, …,
/// 0xFF, 0xFB).
pub fn tint(n: u32) -> u8 {
    let m = n % 56;
    match m <= 28 {
        true => (0x90 + 4 * m).min(0xFF) as u8,
        false => (0xFF - 4 * (m - 28)).max(0x90) as u8,
    }
}

/// The HUD's data and loaded pictures.
#[derive(Resource)]
struct HudAssets {
    hud: Hud,
    textures: HashMap<String, (Handle<Image>, [u16; 2])>,
    glyphs: Vec<Handle<Image>>,
    quad: Handle<Mesh>,
}

/// The camera that draws the HUD's layer over the view.
#[derive(Component)]
struct HudCamera;

/// A quad of this frame's HUD (respawned every frame).
#[derive(Component)]
struct HudSprite;

/// Loads the HUD's pictures (`hud/` under the pack's root) and draws the HUD every frame.
pub fn build(app: &mut App, hud: Hud, pickups: Vec<conker_assets::Pickup>) {
    app.insert_resource(Pickups(pickups));
    app.init_resource::<Prompt>();
    app.add_systems(Update, prompt.before(draw));
    app.add_systems(
        Startup,
        move |assets: Res<AssetServer>,
              mut meshes: ResMut<Assets<Mesh>>,
              mut commands: Commands| {
            // The haze's and the gun's tapes' textures wrap (G_TX_WRAP, mask 6; cm 0); the others
            // clamp.
            let load = |file: &str, smooth: bool, wrap: bool| -> Handle<Image> {
                let mut sampler = if smooth {
                    ImageSamplerDescriptor::linear()
                } else {
                    ImageSamplerDescriptor::nearest()
                };
                if wrap {
                    sampler.address_mode_u = ImageAddressMode::Repeat;
                    sampler.address_mode_v = ImageAddressMode::Repeat;
                }
                let sampler = ImageSampler::Descriptor(sampler);
                let settings = move |s: &mut ImageLoaderSettings| s.sampler = sampler.clone();
                assets
                    .load_builder()
                    .with_settings(settings)
                    .load(format!("hud/{file}"))
            };
            let end = hud.end.iter().flat_map(|e| &e.textures);
            let textures = (hud.textures.iter().chain(end))
                .map(|(name, t)| {
                    let mut tapes = hud.gun.tapes.iter().map(|t| &t.image.texture);
                    let wrap = *name == hud.haze.image.texture || tapes.any(|t| t == name);
                    (name.clone(), (load(&t.file, t.smooth, wrap), t.size))
                })
                .collect();
            let glyphs = hud
                .font
                .iter()
                .map(|g| load(&g.file, false, false))
                .collect();
            let quad = meshes.add(Rectangle::new(1.0, 1.0));
            commands.insert_resource(HudAssets {
                hud: hud.clone(),
                textures,
                glyphs,
                quad,
            });
        },
    );
    app.add_systems(
        Update,
        (display_space, draw.after(crate::crosshair::update)),
    );
}

/// The HUD's camera: orthographic, one unit a window pixel, over the view (into `target` when
/// drawing off screen).
pub fn camera(commands: &mut Commands, target: Option<Handle<Image>>) {
    let mut camera = commands.spawn((
        HudCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::WindowSize,
            near: 0.0,
            far: 100.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        RenderLayers::layer(LAYER),
        Transform::from_xyz(0.0, 0.0, 50.0),
    ));
    if let Some(image) = target {
        camera.insert(RenderTarget::Image(image.into()));
    }
}

/// Total War's gas countdown as the game prints it, from [`MatchView::countdown`] (G+0x1FE4 after the
/// frame): the AI script prints `(t + 59) / 60` of the countdown `t` before its frame's decrement
/// (0xBA 0x3DFE `command(0xF000, 1, G+0x1FE4)`, then `timer10` at 0x3E06; `func_1509DFC4` case 1),
/// so `t` is `countdown + 1`, on the frames after the delivery's (300). The kill's frame prints 1 and
/// leaves 0, which is no countdown here (I: a frame short).
pub fn countdown(countdown: i32) -> Option<u32> {
    (1..300)
        .contains(&countdown)
        .then(|| (countdown + 1 + 59) as u32 / 60)
}

/// War: Colors' carried-flag icons show while the VI counter (`0x800BE9AC`, 2 a frame) has its
/// low 5 bits at 11 or more: 21 of every 32 VIs; the carriers' radar blips show in the other 11
/// (`0x800E0BCC` masks them; C `func_15093B58`; V e0, e1: 18, 26, 30, 12 shown, 0, 10, 2 not).
pub fn flag_icons(vi: u32) -> bool {
    vi & 31 >= 11
}

/// War: Colors' fade around a capture's respawn (`command(0x50BB, 0x12, …, vis, out)`) `frames`
/// into it: to black over `vis` VIs, or back. I: the curve is the outro's (`func_1517EF00`, 255 −
/// e·255/d with e the VIs gone), its call here not read.
pub fn capture_fade(out: bool, vis: u16, frames: u32) -> u8 {
    let step = (2 * frames * 255 / u32::from(vis).max(1)).min(255) as u8;
    if out { step } else { 255 - step }
}

/// The fades' z, over every other part of the HUD.
const FADE_Z: f32 = 9.5;
/// The out view's black: over the HUD, the reticle, scope and gun sight included (6.0-6.5), under
/// the strike's flash, the iris and the fades.
const OUT_Z: f32 = 7.0;

/// Black over the whole view at `alpha`.
fn black(alpha: u8, z: f32) -> Sprite {
    let far = 10_000.0;
    let centre = Anchor(Horizontal::Centre, Vertical::Middle);
    solid(
        [-far, -far, 2.0 * far, 2.0 * far],
        centre,
        [0, 0, 0, alpha],
        z,
    )
}

/// The end phase's fade (phase 7, `func_151E4EE8` → `func_151ED1E0`) on its `k`th frame from 1:
/// black, alpha +16 a frame (`D_8008FD74` 8 × 2 VIs) to opaque (V c1b DL 428, 434, 436: 0x70, 0xD0,
/// 0xF0 on phase 7's frames 7, 13, 15). The STATS or the outro cut it.
pub fn end_fade(k: u32) -> u8 {
    (16 * k).min(255) as u8
}

/// RESTART's fade (phase 3): the end phase's again, over the frozen level without the STATS, from 0
/// as the server counts `restart_in` down from 17 (V t5_r: A at read R, the STATS on R+1, then
/// 16k on R+1+k, 255 on R+17, the new match on R+18; C `func_151E4EE8`, `func_151ED1E0`).
pub fn restart_fade(m: &MatchView) -> Option<u8> {
    (m.phase == 3).then(|| end_fade(17u32.saturating_sub(u32::from(m.restart_in))))
}

/// The STATS' alpha on their `k`th frame from 1: +4 a VI from 0 (V tw4 DL 858-880, cs1 990-1002;
/// I: every mode's STATS, the C site not found).
pub fn stats_alpha(k: u32) -> u8 {
    (8 * k).min(255) as u8
}

/// War: Beach's escape on the escapee's own view (`command(p, 0x8B, …)`, `beach-presentation.md`
/// §2): for the freeze's last 30 frames a fade to black over 30 VIs, which the script re-sends every
/// frame, so the view stays at its first step, alpha 0x11 (V e1 DL 235-250: elapsed 2 of 30; the
/// original's bug, kept: the view only dims); `since` frames after the respawn, the fade in from
/// black over 100 VIs (V e1 DL 252, 254, 256, 270: 0xF5, 0xEB, 0xE1, 0x99).
pub fn escape_fade(freeze: u8, since: Option<u32>) -> Option<u8> {
    match (freeze, since) {
        (1..=30, _) => Some(capture_fade(true, 30, 1)),
        (0, Some(k)) if k < 50 => Some(capture_fade(false, 100, k)),
        _ => None,
    }
}

/// The out view's skull byte (`0x800D18A4[i]`) `frames` on: +4 a frame up to 60 while `out`, −4 a
/// frame to 0 once taken over (V t1: 4 on the out frame, 52 at the takeover 13 frames on, 0 13
/// frames after, when the view cuts in, no fade). The skull's half-size in px is (byte + 4) / 4.
pub fn skull_byte(byte: u8, out: bool, frames: u32) -> u8 {
    let d = (4 * frames.min(15)) as u8;
    match out {
        true => byte.saturating_add(d).min(60),
        false => byte.saturating_sub(d),
    }
}

/// Where the strike's flash is centred (V ev3 DL 1046-1060).
pub const STRIKE_CENTRE: [f32; 2] = [146.0, 83.0];

/// The strike's flash on its victim's view `k` frames after the blast: the iris's quarters (I8
/// 64 × 64), up to 384 wide, red with alpha 0xF8 → 0xC2, then black, fading (V ev3 DL 1046-1060; I:
/// 7 frames each, straight between).
pub fn strike_flash(k: u32) -> Option<Rgba> {
    const HALF: u32 = 7;
    let lerp = |a: f32, b: f32, k: u32| (a + (b - a) * k as f32 / HALF as f32) as u8;
    match k {
        0..HALF => Some([255, 0, 0, lerp(248.0, 194.0, k)]),
        HALF..14 => Some([0, 0, 0, lerp(194.0, 0.0, k - HALF)]),
        _ => None,
    }
}

/// What ranks a player (conker-rules' [`places`]).
pub fn standing(c: &Combat) -> Standing {
    Standing {
        team: c.team,
        kills: c.kills,
        deaths: c.deaths,
        shots: c.shots,
        out: c.out,
        score: c.score,
    }
}

/// What the HUD shows of the local player (`own`), the others and the match; `view`: the modern
/// controls' orbit camera drew the frame looking along it (the radar turns with it; its crosshair
/// replaces the reticle).
#[allow(clippy::type_complexity)]
fn shown(
    hud: &Hud,
    own: (&Player, &Combat, &FollowCamera, &Anim),
    everyone: &Query<(&Player, &Combat, Has<Predicted>), Or<(With<Predicted>, With<Interpolated>)>>,
    m: &MatchView,
    vi: u32,
    race_map: Option<&str>,
    view: Option<Vec3>,
) -> Shown {
    let (p, c, camera, anim) = own;
    let clock = &anim.0;
    // Item 0x3A's from its pull to its throw (V held-pack-scratch p1a, p3f DLs 110, 126; gone by
    // 140, 8 frames into the throw).
    let lob = [LOB_PULL, LOB_HOLD].contains(&clock.id);
    let in_hand = lob
        || [GRENADE_PULL, GRENADE_HOLD].contains(&clock.id)
        || (clock.id == GRENADE_THROW && clock.time < THROW_IN_HAND);
    let (eye, at) = camera.drawn.view(&camera.follow);
    let look = view.unwrap_or(at - eye);
    let ahead = Vec2::new(look.x, look.z).normalize_or(Vec2::NEG_Y);
    let right = Vec2::new(-ahead.y, ahead.x);
    let level = |y: f32| hud.radar.levels.iter().filter(|&&l| l <= y).count();
    // War colours a blip by its team (C `func_15093B58`: the name colours by obj+0x328); Heist's
    // four (`0x800AB62C`).
    let r = &hud.radar;
    let team_colours = if m.heist {
        &hud.heist.radar[..]
    } else {
        &r.team_colours
    };
    let colour = |o: &Combat| {
        let team = team_colours.get(usize::from(o.team));
        (team.filter(|_| crate::outro::team_mode(m)).copied()).unwrap_or(r.blip_colour)
    };
    let blip = |pos: [f32; 3], colour| {
        let d = Vec2::new(pos[0] - p.pos[0], pos[2] - p.pos[2]);
        let higher = level(pos[1]).cmp(&level(p.pos[1])) as i8;
        ([d.dot(right), d.dot(ahead)], higher, colour)
    };
    // Colors: a carrier's blip blinks against the icons; a flag not carried is a blip. Heist: the
    // bag's carrier the same, the free bag a steady blip.
    let icons = flag_icons(vi);
    let carrier = m.cash.carrier.filter(|_| m.heist);
    // Raptor's egg carriers, their blips too (C `0x800E0BCC`'s mask; I not seen).
    let eggs: Vec<u16> = (m.raptors.eggs.iter())
        .filter_map(|e| e.1)
        .filter(|_| m.raptor)
        .collect();
    let mut blips: Vec<_> = (everyone.iter())
        .filter(|(_, o, predicted)| !predicted && o.health > 0)
        .filter(|(_, o, _)| !hud.radar.hidden_with.contains(&o.weapon))
        .filter(|(_, o, _)| {
            (o.flag.is_none() && carrier != Some(o.slot) && !eggs.contains(&o.slot)) || !icons
        })
        .map(|(o, oc, _)| blip(o.pos, colour(oc)))
        .collect();
    let lying = m.flags.iter().filter(|f| f.shown && f.carrier.is_none());
    blips.extend((lying.filter(|_| m.colors)).map(|f| blip(f.pos, r.flag_colour)));
    if m.heist && m.cash.shown && carrier.is_none() {
        blips.push(blip(m.cash.pos, hud.heist.cash));
    }
    // Race: the other racers in play on the overview map (C: `func_150859AC(i, 3) > 0`).
    let dots: Vec<[f32; 3]> = (everyone.iter())
        .filter(|(_, o, predicted)| !predicted && o.out == 0 && m.race)
        .map(|(o, _, _)| o.pos)
        .collect();
    let scope = camera.follow.cam.scope;
    // On a mounted gun its scope is the gun's sight; the weapon is the gun's (no icon: V one-view
    // DLs). Its tapes read the look: the modern controls' (`view`, the aim's) or the scope's.
    let (heading, elevation) = view.map_or((scope.yaw + 90.0, -scope.pitch), |v| {
        (v.x.atan2(v.z).to_degrees(), v.y.asin().to_degrees())
    });
    let gun = (scope.on && scope.weapon == conker_camera::GUN).then(|| Sight {
        heading,
        elevation,
        zoom: scope.zoom[0] / conker_camera::zoom_limit(conker_camera::GUN),
        slide: -3.0 * ((scope.zoom[0] + scope.zoom[1]) / 2.0).clamp(-25.0, 0.0),
        ..Sight::default()
    });
    // The place among everyone, this player first; Total War's strip: each team's kills, untimed
    // its lives left (`func_151E9D18`).
    let others = (everyone.iter()).filter(|o| !o.2).map(|o| o.1);
    let combats: Vec<_> = std::iter::once(c).chain(others).collect();
    let all: Vec<_> = combats.iter().map(|o| Some(standing(o))).collect();
    let mut strip = [0; 2];
    // Heist: a team with a participant in play shows its pips (V hud2 DL 300: an empty team's
    // skull; hud3 DL 200: an eliminated one's).
    let mut alive = [false; 4];
    for o in combats.iter().filter(|o| o.out == 0) {
        if let Some(a) = alive.get_mut(usize::from(o.team)) {
            *a = true;
        }
    }
    let carrying = (combats.iter())
        .find(|o| carrier == Some(o.slot))
        .map(|o| o.team)
        .filter(|_| icons);
    for o in &combats {
        if let Some(n) = strip.get_mut(usize::from(o.team)) {
            *n += u32::from(if m.time == 0 {
                u16::from(o.lives_left)
            } else {
                o.kills
            });
        }
    }
    Shown {
        health: c.health,
        // Carrying the bag or an egg, or a raptor: no weapon icon (V hud1 DL 341, back at 401;
        // h2 DLs 235-405, back at 420; h1 DL 40).
        weapon: if gun.is_some() {
            conker_camera::GUN
        } else if m.tank {
            c.power
        } else if carrier == Some(c.slot)
            || eggs.contains(&c.slot)
            || c.class == conker_rules::raptor::CLASS
        {
            0
        } else {
            c.weapon
        },
        grenades: in_hand.then_some(c.grenades),
        lob,
        kills: c.kills,
        rank: usize::from(places(&all, m.kill_limit)[0]),
        secs: (m.time > 0).then_some(m.timer.max(0) as u32 / 60),
        radar: m.radar && !m.beach && !m.race,
        blips,
        reticle: view.is_none()
            && p.aiming
            && hud.reticle.weapons.contains(&c.weapon)
            && gun.is_none(),
        scope: (scope.on && gun.is_none()).then(|| {
            let weapon = if m.tank { scope.weapon } else { c.weapon };
            scope.zoom[0] / conker_camera::zoom_limit(weapon)
        }),
        turn: dot_turn(scope.zoom),
        tint: None,
        gun,
        respawn_in: (c.health == 0 && c.respawn_in > 0).then_some(c.respawn_in),
        iris_at: None,
        lives: (m.time == 0 && !m.colors && !m.beach && !m.raptor).then_some(c.lives_left),
        skull: None,
        ending: None,
        strip: match m.colors || m.beach || m.raptor {
            true => Some(m.scores.map(u32::from)),
            false => m.total_war.then_some(strip),
        },
        flags: (m.colors).then(|| m.flags.map(|f| icons && f.carrier.is_some())),
        fade: None,
        countdown: countdown(m.countdown),
        haze: (c.haze > 0).then_some(u32::from(c.haze)),
        results: None,
        header: None,
        beach: m.beach,
        escape: (m.beach && c.team == conker_rules::beach::VILLAGERS)
            .then_some(u32::from(c.deadline) / 60),
        strike: None,
        heist: m.heist.then_some(Bags {
            scores: m.heist_scores,
            limit: m.score_limit,
            alive,
            carrying,
        }),
        tank: m.tank,
        raptor: m
            .raptor
            .then(|| [!eggs.is_empty(), m.raptors.prey != 0].map(|carried| carried && icons)),
        race: (m.race.then(|| race_hud(m, c, p.pos, &dots, race_map))).flatten(),
        race_rows: None,
        prompt: None,
    }
}

/// Race's STATS rows: each human (by slot) its place, total and best lap. The total is the
/// server's (the race clock at the last lap's crossing; −1 for a racer the end rule finished short
/// of the laps).
pub fn race_rows(m: &MatchView, humans: &[u16]) -> Vec<[i32; 3]> {
    let mut rows: Vec<_> = (m.racing.racers.iter())
        .filter(|r| humans.contains(&r.slot))
        .map(|r| (r.slot, [i32::from(r.place), r.total, r.best]))
        .collect();
    rows.sort_by_key(|r| r.0);
    rows.into_iter().map(|r| r.1).collect()
}

/// Race's HUD for the racer `c` at `pos`, the others' at `others` (`dots`).
fn race_hud(
    m: &MatchView,
    c: &Combat,
    pos: [f32; 3],
    others: &[[f32; 3]],
    map: Option<&str>,
) -> Option<RaceHud> {
    let r = m.racing.racers.iter().find(|r| r.slot == c.slot)?;
    let mut dots = vec![(race_dot(pos), [0, 255, 0, 255])];
    dots.extend(others.iter().map(|&o| (race_dot(o), [255, 0, 0, 255])));
    Some(RaceHud {
        place: r.place,
        lap: r.lap,
        laps: m.racing.laps,
        time: r.time,
        last: r.last,
        items: c.rounds,
        map: map.filter(|_| c.out == 0).map(str::to_owned),
        dots,
    })
}

/// A racer's dot on the overview map (`func_150918EC`, C): its centre in quarter pixels,
/// [`RACE_MAP_CENTRE`] plus [`RACE_MAP_OFFSET`] plus x (across) and z (down) ×
/// [`RACE_MAP_SCALE`], truncated.
pub fn race_dot([x, _, z]: [f32; 3]) -> [f32; 2] {
    let q = |c: f32, off: f32, v: f32| (c * 4.0 + off + (v * RACE_MAP_SCALE).trunc()).trunc() / 4.0;
    [
        q(RACE_MAP_CENTRE[0], RACE_MAP_OFFSET[0], x),
        q(RACE_MAP_CENTRE[1], RACE_MAP_OFFSET[1], z),
    ]
}

/// The pack's pickups (`pickups.json`), for the [`Prompt`]; War: Raptor's are its `raptor.json`'s.
#[derive(Resource, Clone, Debug, Default)]
pub struct Pickups(pub Vec<conker_assets::Pickup>);

/// The modern controls' pickup prompt this frame ([`pickup_prompt`]).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Prompt(pub Option<String>);

/// Decision 12's prompt: "PRESS <key> TO PICK UP" while `f` stands on a weapon's pickup, shown, that
/// it takes only with Interact: conker-rules' `can_take`, the server's gate, with and without the
/// press (a human's reach; `pickups` the pack's, the roaming ones where `m` has them now).
pub fn pickup_prompt(
    pickups: &[conker_assets::Pickup],
    m: &MatchView,
    f: &conker_rules::Fighter,
    key: &str,
) -> Option<String> {
    use conker_rules::weapon::{Gives, REACH};
    let s = crate::match_settings(m);
    let waits = |p: &conker_assets::Pickup| {
        let conker_assets::Gives::Weapon(id) = p.gives else {
            return false;
        };
        let taken = p.placement < 64 && m.taken >> p.placement & 1 != 0;
        let pos = (m.roaming.iter())
            .find(|r| r.0 == p.placement)
            .map_or(p.position, |r| r.1);
        let take = |interact| {
            conker_rules::game::can_take(
                &s,
                Gives::Weapon(id),
                pos,
                f,
                REACH,
                m.countdown,
                Some(interact),
            )
        };
        !taken && !take(false) && take(true)
    };
    pickups
        .iter()
        .any(waits)
        .then(|| format!("PRESS {key} TO PICK UP"))
}

/// Decision 19's prompt: "PRESS <key> TO MOUNT" while `f` could get on a free mounted gun of `m`
/// with Interact (conker-rules' `can_mount`, the server's gate).
pub fn mount_prompt(m: &MatchView, f: &conker_rules::Fighter, key: &str) -> Option<String> {
    let free =
        |g: &&conker_sim::net::GunView| g.gunner.is_none() && g.state == conker_rules::gun::FREE;
    (m.guns.iter().filter(free))
        .any(|g| conker_rules::game::can_mount(f, g.pos, true))
        .then(|| format!("PRESS {key} TO MOUNT"))
}

/// An action's binding as a prompt shows it, uppercase (the HUD font has no lowercase), as the
/// settings screen names it (`settings_ui::bound_label`: a key as the player's layout types it once
/// pressed, C1's [`KeyLabels`]): with `pad`'s glyphs its pad button first (the player is on the pad),
/// else its first key or mouse slot first.
pub fn key_label(
    bindings: &Bindings,
    a: Action,
    labels: Option<&KeyLabels>,
    pad: Option<Glyphs>,
) -> String {
    use crate::settings_ui::{Bound, bound_label};
    let slots = bindings.get(a);
    let key = slots.keys.iter().flatten().next().map(|&k| Bound::Key(k));
    let button = slots.pad.map(Bound::Pad);
    let first = match pad {
        Some(_) => button.or(key),
        None => key.or(button),
    };
    match first {
        Some(v) => bound_label(Some(v), labels, pad.unwrap_or(Glyphs::Xbox)),
        None => crate::settings_ui::action_label(a),
    }
}

/// The local player's [`Prompt`] ([`pickup_prompt`], else [`mount_prompt`]), with the key of the
/// device in use (the pad's glyphs: C3's vendor detection and Button prompts setting).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn prompt(
    own: Query<(&Player, &Fighting), With<FollowCamera>>,
    game: Query<&MatchView>,
    pickups: Res<Pickups>,
    nest: Option<Res<crate::raptor::Nest>>,
    (bindings, labels, using, settings): (
        Res<Bindings>,
        Option<Res<KeyLabels>>,
        Option<Res<crate::bindings::UsingPad>>,
        Res<crate::settings::Settings>,
    ),
    pads: Query<(&Gamepad, Option<&Name>)>,
    mut out: ResMut<Prompt>,
) {
    let line = match (own.single(), game.single()) {
        (Ok((p, f)), Ok(m)) => {
            let list = match nest.as_ref().filter(|_| m.raptor) {
                Some(n) => &n.data.pickups,
                None => &pickups.0,
            };
            let glyphs = (using.is_some_and(|u| u.0))
                .then(|| crate::settings_ui::pad_glyphs(settings.pad.prompts, pads.iter().next()));
            let key = key_label(&bindings, Action::Interact, labels.as_deref(), glyphs);
            let f = f.fighter(p.0);
            pickup_prompt(list, m, &f, &key).or_else(|| mount_prompt(m, &f, &key))
        }
        _ => None,
    };
    out.set_if_neq(Prompt(line));
}

/// This frame's HUD: the quads of [`sprites`] for the local player while connected, placed in the
/// HUD camera's target ([`place`]); their materials are kept for reuse.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    assets: Option<Res<HudAssets>>,
    camera: Single<&Camera, With<HudCamera>>,
    status: Res<NetStatus>,
    own: Query<(&Player, &Combat, &FollowCamera, &Anim)>,
    everyone: Query<(&Player, &Combat, Has<Predicted>), Or<(With<Predicted>, With<Interpolated>)>>,
    game: Query<&MatchView>,
    roster: Query<(&PlayerId, &Combat), Or<(With<Predicted>, With<Interpolated>)>>,
    menu: Res<crate::results::Menu>,
    (playing, outros, blasts, fighting, view, heads, joints, course, crosshair): (
        Res<crate::outro::Playing>,
        Res<crate::outro::Outros>,
        Option<Res<crate::beach::Blasts>>,
        Query<&Fighting, With<FollowCamera>>,
        Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<HudCamera>)>,
        Query<&crate::held::Twisted, With<FollowCamera>>,
        Query<&GlobalTransform>,
        Option<Res<crate::race::Course>>,
        Option<Res<crate::crosshair::Quads>>,
    ),
    timeline: Res<LocalTimeline>,
    (mut fading, mut escaped, mut ended, prompt, esc_menu, orbit): (
        Local<Option<(Option<(bool, u16, u32)>, Option<u32>)>>,
        Local<(u8, Option<u32>)>,
        Local<Option<u32>>,
        Res<Prompt>,
        Option<Res<crate::menu::Quads>>,
        Option<Res<crate::orbit::Orbit>>,
    ),
    (mut fired, mut zoomed, mut stats, mut skull, mut opened): (
        Local<(Option<(usize, u16)>, [Option<(u32, u16)>; 2])>,
        Local<(u32, f32, bool, Option<u32>)>,
        Local<Option<u32>>,
        Local<(u32, u8)>,
        Local<Option<u32>>,
    ),
    old: Query<Entity, With<HudSprite>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<HashMap<(Tex, [u32; 4], Rgba), Handle<StandardMaterial>>>,
    mut commands: Commands,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let (Some(a), Some(window)) = (assets, camera.logical_viewport_size()) else {
        return;
    };
    let (Ok(own), Ok(m)) = (own.single(), game.single()) else {
        return;
    };
    if *status != NetStatus::Connected {
        return;
    }
    let quads = match (playing.running(&outros), restart_fade(m)) {
        (Some((o, k)), _) => outro(&a.hud, o, k),
        // RESTART: only its fade over the frozen level (the skull's clock kept, for the next match).
        (None, Some(alpha)) => {
            skull.0 = timeline.tick().0;
            vec![black(alpha, FADE_Z)]
        }
        (None, None) => {
            let tick = timeline.tick().0;
            let map = (course.as_ref())
                .and_then(|c| c.route(m.racing.route))
                .map(|r| r.map.as_str());
            let looking = orbit.as_ref().and_then(|o| o.view).map(|v| v.1);
            let mut shown = shown(&a.hud, own, &everyone, m, 2 * tick, map, looking);
            shown.prompt = prompt.0.clone();
            // Weapon 0x16's R view: its tint from the frame the scope opened.
            let scope = own.2.follow.cam.scope;
            *opened = (scope.on && scope.weapon == 0x16).then(|| opened.unwrap_or(tick));
            shown.tint = opened.map(|at| tint(tick - at));
            // Dead: the iris round the head on the screen, every frame (`func_1507E1D0`,
            // `func_1517F814`: joint 12 × (0, 30 × the scale, 0); C, V deathC), a blast's victim
            // round its position (V; I: why).
            if shown.respawn_in.is_some() {
                let head = (heads.single().ok()).and_then(|t| t.1.and_then(|h| joints.get(h).ok()));
                let point = match head {
                    Some(j) if own.1.anim != BLOWN_UP => {
                        j.translation() + j.rotation() * Vec3::Y * 30.0 * crate::render::MODEL_SCALE
                    }
                    _ => Vec3::from(own.0.pos),
                };
                let at = (view.single().ok()).and_then(|(c, t)| c.world_to_viewport(t, point).ok());
                shown.iris_at = at.map(|at| frame_point(a.hud.frame, window, at));
            }
            // Colors' fade from the tick this client saw it change (the match's tick is not sent);
            // the one already there when this client first sees the match is not replayed.
            match *fading {
                Some((last, _)) if last == m.fade => {}
                before => {
                    *fading = Some((m.fade, before.map(|_| tick)));
                    if let (Some(_), Some((out, vis, _))) = (before, m.fade) {
                        let read = crate::script_read(tick, own.2.since, Some(m)).unwrap_or(0);
                        info!(
                            "capture fade: out {out} over {vis} VIs from tick {tick} (read {read})"
                        );
                    }
                }
            }
            shown.fade = match *fading {
                Some((Some((out, vis, _)), Some(at))) if m.phase == 0 => {
                    Some(capture_fade(out, vis, tick.saturating_sub(at)))
                }
                _ => None,
            };
            // Beach: the escape's dim, then its fade in from the respawn (the freeze's end); the
            // strike's flash from its blast.
            let freeze = own.1.freeze;
            if escaped.0 > 0 && freeze == 0 {
                escaped.1 = Some(tick);
            }
            escaped.0 = freeze;
            let since = escaped.1.map(|t| tick.saturating_sub(t));
            if let Some(alpha) = escape_fade(freeze, since).filter(|_| m.beach && m.phase == 0) {
                shown.fade = Some(alpha);
            }
            // The end phase's fade from the tick this client sees it.
            if m.phase == 1 && ended.is_none() {
                let read = crate::script_read(tick, own.2.since, Some(m)).unwrap_or(0);
                info!("end fade from tick {tick} (read {read})");
            }
            *ended = (m.phase == 1).then(|| ended.unwrap_or(tick));
            if let Some(at) = *ended {
                let k = tick.saturating_sub(at) + 1;
                (shown.fade, shown.ending) = (Some(end_fade(k)), Some(k));
            }
            // Out: the skull from the tick this client sees it, shrinking after a takeover; the
            // results clear it.
            let (at, byte) = &mut *skull;
            *byte = match m.phase {
                2 => 0,
                _ => skull_byte(*byte, own.1.out > 0, tick.wrapping_sub(*at)),
            };
            *at = tick;
            shown.skull = (*byte > 0).then(|| (*byte + 4) / 4);
            let strikes = blasts.iter().flat_map(|b| &b.strikes);
            shown.strike = (strikes.filter(|s| s.slot == own.1.slot))
                .find_map(|s| s.blast)
                .map(|t| tick.saturating_sub(t));
            // The own gun's shots, by barrel, on the tick the prediction fires each (the fighter's
            // shot count and its mount's next barrel; a rollback that takes one back clears them).
            let manned = (fighting.single().ok()).and_then(|f| {
                let g = f
                    .state
                    .gun
                    .filter(|g| g.state != conker_rules::gun::LAUNCH)?;
                Some((usize::from(g.gun), f.state.shots, g.barrel ^ 1))
            });
            match (manned, fired.0) {
                (Some((k, shots, b)), Some((last, n))) if k == last && shots >= n => {
                    if shots > n {
                        fired.1[usize::from(b)] = Some((tick, shots));
                    }
                }
                _ => fired.1 = [None; 2],
            }
            fired.0 = manned.map(|(k, shots, _)| (k, shots));
            if let Some(g) = &mut shown.gun {
                let Sight { flash, kick, .. } = gun_fire(&a.hud.gun.flash, fired.1, tick);
                (g.flash, g.kick) = (flash, kick);
                // The zoom bar goes red 16 frames after the zoom stops, with its last step's sound.
                let (at, last, moving, red) = &mut *zoomed;
                if tick != *at {
                    let moved = g.zoom != *last;
                    if *moving && !moved {
                        *red = Some(tick + 16);
                    }
                    (*at, *last, *moving) = (tick, g.zoom, moved);
                }
                g.bar_red = red.and_then(|r| tick.checked_sub(r));
            } else {
                *zoomed = Default::default();
            }
            // Race: its STATS in the level from the results (phase 6: no HUD); a human out ends it
            // as Deathmatch's (result 2, V b1).
            let raced = m.race && m.racing.results.is_some() && m.racing.result == 1;
            let in_level = raced && m.phase == 0 && m.racing.results.is_some_and(|r| tick >= r);
            *stats = (m.phase == 2 || in_level).then(|| stats.unwrap_or(tick));
            if let Some(at) = *stats {
                let all: Vec<_> = (roster.iter())
                    .map(|(id, c)| (!matches!(id.0, PeerId::Local(_)), c))
                    .collect();
                let teams = crate::outro::team_mode(m);
                let rows = rows(&all, m.kill_limit, teams, m.winner, m.beach, m.heist);
                // Race's fade in from 0x10 (V res 271-320).
                let alpha = stats_alpha(tick - at + 1 + u32::from(raced));
                shown.results = Some((rows, menu.choice, alpha, 2 * tick));
                if raced {
                    let humans: Vec<u16> = (all.iter()).filter(|p| p.0).map(|p| p.1.slot).collect();
                    shown.race_rows = Some(race_rows(m, &humans));
                }
                // Raptor's team headers are not in the pack: "YOU WIN" or "YOU LOSE" (I).
                shown.header = teams.then(|| match header(&all, own.1.team, m.winner) {
                    Header::Team(t) if m.raptor => Header::You(t == own.1.team),
                    h => h,
                });
            }
            let mut quads = sprites(&a.hud, &shown);
            quads.extend(crosshair.iter().flat_map(|c| c.0.iter().cloned()));
            quads
        }
    };
    // The Esc menu over all of it.
    let menu_quads = esc_menu.iter().flat_map(|q| q.sprites.iter().cloned());
    for s in quads.into_iter().chain(menu_quads) {
        let key = (s.tex.clone(), s.uv.map(f32::to_bits), s.colour);
        let material = cache.entry(key).or_insert_with(|| {
            let (texture, size) = match &s.tex {
                Tex::Texture(name) => a.textures[name].clone(),
                Tex::Glyph(g) => (a.glyphs[*g].clone(), a.hud.font[*g].size.map(u16::from)),
                Tex::Solid | Tex::Tint => (Handle::default(), [1, 1]),
            };
            let texture = (!matches!(s.tex, Tex::Solid | Tex::Tint)).then_some(texture);
            let size = Vec2::from(size.map(f32::from));
            let [x0, y0, x1, y1] = s.uv;
            let [r, g, b, alpha] = s.colour;
            let alpha = display_alpha(luminance([r, g, b]), f32::from(alpha) / 255.0);
            materials.add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b).with_alpha(alpha),
                base_color_texture: texture,
                uv_transform: Affine2::from_scale_angle_translation(
                    Vec2::new(x1 - x0, y1 - y0) / size,
                    0.0,
                    Vec2::new(x0, y0) / size,
                ),
                unlit: true,
                alpha_mode: match s.tex {
                    Tex::Tint => AlphaMode::Multiply,
                    _ => AlphaMode::Blend,
                },
                cull_mode: None,
                ..default()
            })
        });
        let [x, y, w, h] = place(a.hud.frame, s.anchor, s.rect, window);
        commands.spawn((
            HudSprite,
            // Batched quads drew with the batch's first material in WebGL2 (V: Chrome, glyphs
            // showed the chocolate).
            NoAutomaticBatching,
            Mesh3d(a.quad.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(
                x + w / 2.0 - window.x / 2.0,
                window.y / 2.0 - y - h / 2.0,
                s.z,
            )
            .with_scale(Vec3::new(w, h, 1.0)),
            RenderLayers::layer(LAYER),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pack: env `CONKER_ASSETS`, else `<repo>/assets`.
    fn pack_dir() -> std::path::PathBuf {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        dir.into()
    }

    /// The pack's HUD; None (the test skips) without one.
    fn pack_hud() -> Option<Hud> {
        let dir = pack_dir();
        let mut hud = Hud::load(|p| std::fs::read(dir.join(p))).ok();
        if let Some(h) = &mut hud {
            h.end = End::load(|p| std::fs::read(dir.join(p))).ok();
        }
        if hud.is_none() {
            eprintln!("no pack HUD at {}: skipping", dir.display());
        }
        hud
    }

    /// The frame rectangles of `s`'s quads of `tex` (in draw order) and their texels.
    fn rects(hud: &Hud, s: &Shown, tex: &Tex) -> Vec<([f32; 4], [f32; 4])> {
        (sprites(hud, s).into_iter())
            .filter(|q| &q.tex == tex)
            .map(|q| (q.rect, q.uv))
            .collect()
    }

    fn texture(name: &str) -> Tex {
        Tex::Texture(name.into())
    }

    /// x0, y0, x1, y1 of the text's quads (glyphs and rank suffixes; not the shadows).
    fn glyphs(hud: &Hud, s: &Shown) -> Vec<[f32; 4]> {
        (sprites(hud, s).into_iter())
            .filter(|q| match &q.tex {
                Tex::Glyph(_) => true,
                Tex::Texture(name) => name.starts_with("rank_"),
                Tex::Solid | Tex::Tint => false,
            })
            .filter(|q| q.colour == [255; 4])
            .map(|q| {
                [
                    q.rect[0],
                    q.rect[1],
                    q.rect[0] + q.rect[2],
                    q.rect[1] + q.rect[3],
                ]
            })
            .collect()
    }

    fn state() -> Shown {
        Shown {
            health: 6,
            rank: 1,
            secs: Some(9 * 60 + 48),
            radar: true,
            ..Shown::default()
        }
    }

    /// The layout at the frame's own size against the original's draws in the oracle's RAM dumps
    /// (`lab/hud`: idle_360, hp_70..150, w*_88, gren_70, radar_62/120/150, w0_300, scope24_250,
    /// hp_260); positions in the 292 x 216 frame.
    #[test]
    #[allow(clippy::excessive_precision)] // the trace's values, as it prints them
    fn layout_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else { return };
        // Health: rows of ceil(h / 2) and floor(h / 2) pieces ending at x = 29, y 5 and 10.
        let rows = |h: u8| {
            let pieces = rects(
                &hud,
                &Shown {
                    health: h,
                    ..state()
                },
                &texture("atlas"),
            );
            [5.0, 10.0].map(|y| {
                let row: Vec<f32> = (pieces.iter())
                    .filter(|p| p.0[1] == y)
                    .map(|p| p.0[0])
                    .collect();
                (!row.is_empty()).then(|| (row.iter().copied().fold(f32::MAX, f32::min), row.len()))
            })
        };
        assert_eq!(rows(6), [Some((14.0, 3)), Some((14.0, 3))]);
        assert_eq!(rows(5), [Some((14.0, 3)), Some((19.0, 2))]);
        assert_eq!(rows(4), [Some((19.0, 2)), Some((19.0, 2))]);
        assert_eq!(rows(3), [Some((19.0, 2)), Some((24.0, 1))]);
        assert_eq!(rows(2), [Some((24.0, 1)), Some((24.0, 1))]);
        assert_eq!(rows(1), [Some((24.0, 1)), None]);
        assert_eq!(rows(0), [None, None]);
        // The weapon's icon at (260, 5), its cell of the atlas.
        for (weapon, atlas, cell) in [
            (0x09, "atlas", [0.0, 4.0, 22.0, 16.0]),
            (0x16, "atlas", [25.0, 4.0, 47.0, 15.0]),
            (0x14, "atlas", [0.0, 16.0, 22.0, 25.0]),
            (0x15, "atlas", [25.0, 25.0, 47.0, 39.0]),
            (0x22, "atlas", [0.0, 39.0, 22.0, 51.0]),
            (0x23, "atlas", [25.0, 39.0, 47.0, 51.0]),
            (0x21, "atlas", [0.0, 53.0, 22.0, 63.0]),
            (0x24, "atlas", [25.0, 53.0, 47.0, 63.0]),
            (0x39, "atlas2", [25.0, 42.0, 47.0, 54.0]),
        ] {
            let s = Shown {
                health: 0,
                weapon,
                ..state()
            };
            let [x0, y0, x1, y1] = cell;
            assert_eq!(
                rects(&hud, &s, &texture(atlas)),
                [([260.0, 5.0, x1 - x0, y1 - y0], cell)]
            );
        }
        // Grenades in the hand: 5 x 20 at x 275, 264, 253, instead of the weapon.
        let s = Shown {
            health: 0,
            weapon: 0x09,
            grenades: Some(3),
            ..state()
        };
        let g: Vec<f32> = rects(&hud, &s, &texture("grenade"))
            .iter()
            .map(|r| r.0[0])
            .collect();
        assert_eq!(g, [275.0, 264.0, 253.0]);
        assert!(rects(&hud, &s, &texture("atlas")).is_empty());
        // Item 0x3A's: the bomb, 11 x 14 at x 247, 236, 225 and y 6 (V held-pack-scratch p1a DL 110).
        let lob = Shown { lob: true, ..s };
        let g: Vec<[f32; 4]> = (rects(&hud, &lob, &texture("grenade")).iter())
            .map(|r| r.0)
            .collect();
        assert_eq!(g, [247.0, 236.0, 225.0].map(|x| [x, 6.0, 11.0, 14.0]));
        // Score "0", rank "(1st)", clock "9:48" (and its shadow, one right and down).
        let text = glyphs(&hud, &state());
        assert_eq!(text[0], [135.0, 6.0, 143.0, 17.0]);
        assert_eq!(
            text[1..5],
            [
                [149.0, 6.0, 153.0, 18.0],
                [152.0, 6.0, 158.0, 16.0],
                [157.0, 2.0, 172.0, 17.0],
                [169.0, 6.0, 173.0, 17.0],
            ]
        );
        assert_eq!(
            text[5..],
            [
                [135.0, 198.0, 143.0, 209.0],
                [142.0, 200.0, 146.0, 207.0],
                [145.0, 198.0, 153.0, 209.0],
                [152.0, 198.0, 160.0, 209.0],
            ]
        );
        let shadow = (sprites(&hud, &state()).into_iter()).find(|q| q.colour == [0, 0, 0, 255]);
        assert_eq!(shadow.unwrap().rect, [136.0, 199.0, 8.0, 11.0]);
        let s = Shown {
            kills: 1,
            rank: 2,
            secs: Some(9 * 60 + 52),
            ..state()
        };
        let text = glyphs(&hud, &s);
        assert_eq!(text[0], [136.0, 6.0, 142.0, 16.0]);
        assert_eq!((text[1][0], text[2][0], text[4][0]), (148.0, 151.0, 169.0));
        assert_eq!(text[8], [151.0, 198.0, 158.0, 208.0]);
        // Radar: the disc, and blips as radar_62, _120 and _150 drew them (the bot from the trace
        // row after the frame, the view from the camera's eye and look-at).
        let disc = rects(&hud, &state(), &texture("radar"));
        assert_eq!(disc[0].0, [235.0, 21.0, 48.0, 48.0]);
        let (eye, at) = ([-2254.88037, 6262.70264], [-2267.0, 6016.0]);
        let ahead = Vec2::new(at[0] - eye[0], at[1] - eye[1]).normalize();
        let right = Vec2::new(-ahead.y, ahead.x);
        for (bot, x, y) in [
            ([-2252.60693, 5665.60303], 258.0, 42.25),
            ([-1937.19641, 4710.85205], 259.75, 37.5),
            ([-1822.73743, 4626.8916], 260.5, 37.25),
        ] {
            let d = Vec2::new(bot[0] + 2267.0, bot[1] - 6016.0);
            let s = Shown {
                blips: vec![([d.dot(right), d.dot(ahead)], 0, hud.radar.blip_colour)],
                ..state()
            };
            let blip = (sprites(&hud, &s).into_iter()).find(|q| q.tex == Tex::Solid);
            assert_eq!(blip.unwrap().rect, [x, y, 2.0, 2.0]);
        }
        // Aiming: the reticle; scoped (the sniper rifle opened): the ring's top-left quarter and the
        // zoom bar (the original's 77..138.75: its quarter pixels).
        let s = Shown {
            reticle: true,
            ..state()
        };
        assert_eq!(
            rects(&hud, &s, &texture("reticle"))[0].0,
            [134.0, 96.0, 24.0, 24.0]
        );
        let s = Shown {
            scope: Some(0.5),
            ..state()
        };
        let ring = rects(&hud, &s, &texture("scope"));
        assert_eq!(ring[0], ([96.0, 58.0, 50.0, 50.0], [0.0, 0.0, 64.0, 64.0]));
        assert_eq!(
            ring[3],
            ([146.0, 108.0, 50.0, 50.0], [64.0, 64.0, 0.0, 0.0])
        );
        let bar = (sprites(&hud, &s).into_iter())
            .filter(|q| q.tex == Tex::Solid)
            .nth(1);
        assert_eq!(bar.unwrap().rect, [54.0, 77.0, 4.0, 62.0]);
        // Dead, 54 frames before the respawn (hp_260): a circle of 120 around (150, 91).
        let s = Shown {
            respawn_in: Some(54),
            ..state()
        };
        let iris = rects(&hud, &s, &texture("iris"));
        assert_eq!(iris[0].0, [30.0, -29.0, 120.0, 120.0]);
    }

    /// The scope's dots turn with the zoom (V scopeB read 200: zoom −27.8, −22.937 → 177.35°;
    /// clamped at the sniper's closest), each rect truncated to quarter pixels; weapon 0x16's R
    /// view: the tint at z 4 (over the radar, health and icon, under the clock and ring), its red
    /// a triangle 0x90-0xFF by 4 (V w16B 200, 201, 229), no dots or bar; the iris round its centre.
    #[test]
    fn the_dots_turn_0x16_tints_and_the_iris_follows() {
        // The camera's zoom as 0x23's scope opens (one view's angles).
        let p = conker_camera::PlayerView {
            weapon: 0x23,
            in_hand: true,
            ..conker_camera::PlayerView::new([0.0; 3], 0.0, 0)
        };
        let mut cam = conker_camera::CameraState::spawn([0.0, 100.0, 247.0], &p);
        cam.step(&p, conker_camera::R, |_, _| Some(0.0));
        let turn = dot_turn(cam.scope.zoom).to_degrees();
        assert!((turn - 177.35).abs() < 0.01, "{turn}");
        assert_eq!(dot_turn([-77.4, -47.0]), 0.0);
        assert_eq!(
            [0, 1, 27, 28, 29, 56, 57].map(tint),
            [0x90, 0x94, 0xFC, 0xFF, 0xFB, 0x90, 0x94]
        );
        let window = Vec2::new(1280.0, 720.0);
        assert_eq!(
            frame_point([292, 216], window, window / 2.0),
            [146.0, 108.0]
        );
        let Some(hud) = pack_hud() else { return };
        let scoped = Shown {
            scope: Some(0.0),
            turn: dot_turn([-27.8, -22.937_294]),
            ..state()
        };
        let dots = rects(&hud, &scoped, &texture("scope_dot"));
        assert_eq!(dots.len(), 12);
        assert_eq!(dots[0].0, [145.5, 50.0, 6.0, 6.0]);
        let r = Shown {
            tint: Some(0x90),
            ..scoped
        };
        let quads = sprites(&hud, &r);
        let tint: Vec<_> = quads.iter().filter(|q| q.tex == Tex::Tint).collect();
        assert_eq!(
            (tint.len(), tint[0].colour, tint[0].z),
            (1, [0x90, 255, 0, 255], 4.0)
        );
        assert!(rects(&hud, &r, &texture("scope_dot")).is_empty());
        assert_eq!(rects(&hud, &r, &texture("scope")).len(), 4);
        assert!(
            !quads.iter().any(|q| q.tex == Tex::Solid && q.z >= 6.0),
            "no bar"
        );
        let dead = Shown {
            respawn_in: Some(54),
            iris_at: Some([100.0, 80.0]),
            ..state()
        };
        assert_eq!(
            rects(&hud, &dead, &texture("iris"))[0].0,
            [-20.0, -40.0, 120.0, 120.0]
        );
    }

    #[test]
    fn anchors_keep_their_edges_distance() {
        let frame = [292, 216];
        let rect = [260.0, 5.0, 22.0, 12.0];
        let at = |h, v, window: Vec2| place(frame, Anchor(h, v), rect, window);
        // At the frame's shape it is the frame scaled.
        let square = Vec2::new(584.0, 432.0);
        assert_eq!(
            at(Horizontal::Right, Vertical::Top, square),
            [520.0, 10.0, 44.0, 24.0]
        );
        assert_eq!(
            at(Horizontal::Left, Vertical::Bottom, square),
            [520.0, 10.0, 44.0, 24.0]
        );
        // A wide window scales by height and keeps the right edge's distance (12 frame pixels).
        let wide = Vec2::new(1920.0, 432.0);
        assert_eq!(
            at(Horizontal::Right, Vertical::Top, wide),
            [1920.0 - 64.0, 10.0, 44.0, 24.0]
        );
        assert_eq!(
            at(Horizontal::Centre, Vertical::Top, wide)[0],
            960.0 + 114.0 * 2.0
        );
        // A narrow one scales by width and keeps the bottom's distance.
        let narrow = Vec2::new(292.0, 600.0);
        assert_eq!(
            at(Horizontal::Left, Vertical::Bottom, narrow),
            [260.0, 389.0, 22.0, 12.0]
        );
    }

    /// The results' rows: humans only, in slot order; the place among everyone (bots too);
    /// accuracy as the oracle's STATS showed it (2 hits of 22 shots: 9%).
    #[test]
    fn rows_list_the_humans() {
        let c = |slot, kills, deaths, shots, hits| Combat {
            slot,
            kills,
            deaths,
            shots,
            hits,
            ..Combat::default()
        };
        let (bot, p1, p2) = (c(0, 3, 0, 10, 5), c(1, 2, 1, 22, 2), c(2, 0, 1, 0, 0));
        assert_eq!(
            rows(
                &[(false, &bot), (true, &p2), (true, &p1)],
                0,
                false,
                None,
                false,
                false
            ),
            [(0, [2, 9, 2, 0, 1, 0]), (1, [3, 0, 0, 0, 1, 0])]
        );
    }

    /// Total War: a human's place is its team's, its colour its team's; the header by the number
    /// of humans (`func_151E43DC`).
    #[test]
    fn total_war_rows_and_header_go_by_team() {
        let c = |slot, team, kills, shots| Combat {
            slot,
            team,
            kills,
            shots,
            ..Combat::default()
        };
        // Tediz 2 kills (the bot's), squirrels 1: the Tediz win (the server's winner).
        let (tedi, squirrel, bot) = (c(0, 1, 0, 3), c(1, 0, 1, 1), c(2, 1, 2, 5));
        let all = [(true, &tedi), (true, &squirrel), (false, &bot)];
        assert_eq!(
            rows(&all, 0, true, Some(1), false, false),
            [(1, [1, 0, 0, 0, 0, 0]), (0, [2, 0, 1, 0, 0, 0])]
        );
        assert_eq!(header(&all, 0, Some(1)), Header::Team(1));
        let alone = [(false, &tedi), (true, &squirrel), (false, &bot)];
        assert_eq!(
            (header(&alone, 0, Some(1)), header(&alone, 1, Some(1))),
            (Header::You(false), Header::You(true))
        );
        // A DRAW (V `to_draw`): both 1st.
        let (a, b) = (c(0, 0, 0, 0), c(1, 1, 0, 0));
        let both = [(true, &a), (true, &b)];
        assert_eq!(header(&both, 0, None), Header::Draw);
        assert_eq!(rows(&both, 0, true, None, false, false)[1].1[0], 1);
    }

    /// The out view's skull (V t1 425-450): its byte 4 on the out frame, +4 a frame to 52 at the
    /// takeover 13 frames on, then −4 a frame, 0 (the view back) 13 frames later; at most 60.
    #[test]
    fn the_out_skull_grows_then_shrinks() {
        let walk = |from: u8, out: bool| {
            (0..13)
                .scan(from, |b, _| {
                    *b = skull_byte(*b, out, 1);
                    Some(*b)
                })
                .collect::<Vec<u8>>()
        };
        let grown = walk(0, true);
        assert_eq!((grown[0], grown[12]), (4, 52));
        let shrunk = walk(52, false);
        assert_eq!((shrunk[0], shrunk[11], shrunk[12]), (48, 4, 0));
        assert_eq!(
            (skull_byte(52, true, 2), skull_byte(0, true, u32::MAX)),
            (60, 60)
        );
        // Its side on the k-th frame out (V a_solo, DL 384-399): 2k + 4 px, 32 from k = 14.
        let sides: Vec<u8> = (0..16)
            .scan(0, |b, _| {
                *b = skull_byte(*b, true, 1);
                Some(2 * ((*b + 4) / 4))
            })
            .collect();
        let want: Vec<u8> = (0..16).map(|k| (2 * k + 4).min(32)).collect();
        assert_eq!(sides, want);
    }

    /// Out, in one view (V a_solo DL 384: the sky, the lives digit, score and rank, the black, the
    /// skull): the black over every part of the HUD, the skull over it, the end phase's fade over
    /// both; Total War's strip over the black (I).
    #[test]
    fn the_out_view_is_black_over_the_hud() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            return;
        };
        let out = Shown {
            skull: Some(16),
            lives: Some(0),
            reticle: true,
            scope: Some(0.5),
            fade: Some(0x40),
            ..state()
        };
        let quads = sprites(&hud, &out);
        let z = |colour: Rgba| quads.iter().find(|q| q.colour == colour).unwrap().z;
        let (black, fade) = (z([0, 0, 0, 255]), z([0, 0, 0, 0x40]));
        let skull = (quads.iter().find(|q| q.tex == texture("skull")))
            .unwrap()
            .z;
        assert!(black < skull && skull < fade);
        let under = quads.iter().filter(|q| q.z < black).count();
        assert_eq!(under, quads.len() - 3, "all but the black, skull and fade");
        let tw = Shown {
            strip: Some([1, 0]),
            ..out
        };
        let strip = (sprites(&hud, &tw).into_iter()).filter(|q| q.tex == texture("teams"));
        assert!(strip.map(|q| q.z).eq([black + 1.0; 2]));
    }

    /// The end phase (V b_kl2): no radar, kills or rank from its first frame, the clock gone from
    /// the second; health and the weapon stay (HUD flags 0x10, 0x4), Total War's strip too (0x200).
    #[test]
    fn the_end_phase_hides_the_radar_kills_and_rank() {
        let Some(hud) = pack_hud() else { return };
        let playing = Shown {
            weapon: 0x09,
            ..state()
        };
        let first = Shown {
            ending: Some(1),
            ..playing.clone()
        };
        let second = Shown {
            ending: Some(2),
            ..playing.clone()
        };
        assert_eq!(rects(&hud, &playing, &texture("radar")).len(), 1);
        assert!(rects(&hud, &first, &texture("radar")).is_empty());
        let clock = glyphs(&hud, &first);
        assert!(clock.len() == 4 && clock.iter().all(|g| g[1] >= 198.0));
        assert!(glyphs(&hud, &second).is_empty());
        assert_eq!(rects(&hud, &second, &texture("atlas")).len(), 6 + 1);
        let tw = Shown {
            strip: Some([2, 4]),
            ..second
        };
        assert_eq!(rects(&hud, &tw, &texture("teams")).len(), 2);
    }

    /// The chosen button's glow, 62 to 251 over 32 frames at a = 255 (V t5_b, t5_r env alphas 0x3E,
    /// 0x6E, 0x9B, 0xCB, 0xCE; 0x7E at a = 0x80), its dim tiles opaque under it while the rest fade
    /// in; the stick there and back over 10 frames.
    #[test]
    fn the_stats_button_pulses_and_the_stick_steps() {
        // vi = 2·tick: v = 0, 15, 16, 31, 47, 48, 63 at ticks 0, 28, 4, 24, 20, 12, 16.
        assert_eq!(
            [0, 28, 4, 24, 20, 12, 16].map(|t| glow(255, 2 * t)),
            [62, 107, 110, 155, 203, 206, 251]
        );
        assert_eq!((glow(0x80, 32), glow(255, 64)), (0x7E, 62));
        let frames: Vec<u32> = (0..10).map(|t| stick(2 * t)).collect();
        assert_eq!(frames, [0, 1, 2, 3, 4, 5, 4, 3, 2, 1]);
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            return;
        };
        let over = Shown {
            results: Some((vec![(0, [1, 0, 0, 0, 0, 0])], 0, 0x80, 32)),
            ..state()
        };
        let quads = sprites(&hud, &over);
        let alpha = |name: &str| -> Vec<u8> {
            (quads.iter().filter(|q| q.tex == texture(name)))
                .map(|q| q.colour[3])
                .collect()
        };
        assert_eq!(
            [alpha("restart"), alpha("restart_lit"), alpha("quit")],
            [[255], [0x7E], [0x80]]
        );
        assert!(alpha("stats") == [0x80] && alpha("quit_lit").is_empty());
        // Tick 3: the stick's frame 3, 96 texels into its strip.
        let over = Shown {
            results: Some((vec![], 1, 255, 6)),
            ..state()
        };
        assert_eq!(
            rects(&hud, &over, &texture("stick")),
            [([132.0, 164.0, 32.0, 32.0], [96.0, 0.0, 128.0, 32.0])]
        );
    }

    /// RESTART's fade from a constructed `MatchView` (phase 3, `restart_in` 17 down to 1): 0, then
    /// +16 a frame, 255 on the last (V t5_r: A at read 300, DL 302 0x20, DL 310 0xA0); none in the
    /// other phases.
    #[test]
    fn restart_fades_from_restart_in() {
        let m = |phase, restart_in| MatchView {
            phase,
            restart_in,
            ..MatchView::default()
        };
        let fades: Vec<_> = (1..=17).rev().map(|n| restart_fade(&m(3, n))).collect();
        let want: Vec<_> = (0..17).map(|k| Some(end_fade(k))).collect();
        assert_eq!(fades, want);
        assert_eq!(
            (want[1], want[15], want[16]),
            (Some(16), Some(240), Some(255))
        );
        assert_eq!([0, 1, 2].map(|p| restart_fade(&m(p, 10))), [None; 3]);
    }

    /// Four humans' rows in red, blue, green and yellow by port (C `func_151EA15C`: the table
    /// 0x800ABA90). Ties (`func_151E565C`, C, V t5_b): humans in slots 0-1, bots in 2-3, all 0-0,
    /// shots 0, 5, 0, 0: P1 1st, P2 4th; equal score and shots share a place, bots get none.
    #[test]
    fn rows_colour_by_port_and_share_ties() {
        let c = |slot, shots| Combat {
            slot,
            shots,
            ..Combat::default()
        };
        let all = [c(0, 0), c(1, 5), c(2, 0), c(3, 0)];
        let with = |humans: usize| -> Vec<Row> {
            let all: Vec<_> = (all.iter().enumerate())
                .map(|(n, c)| (n < humans, c))
                .collect();
            rows(&all, 0, false, None, false, false)
        };
        let four = with(4);
        assert!(four.iter().map(|r| r.0).eq(0..4));
        assert!(four.iter().map(|r| r.1[0]).eq([1, 4, 1, 1]));
        assert!(with(2).iter().map(|r| r.1[0]).eq([1, 4]));
        if let Some(end) = pack_hud().and_then(|h| h.end) {
            let rgb = [
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 255, 0, 255],
            ];
            assert_eq!(end.results.colours, rgb);
        }
    }

    /// The lives digit, the skull and the results where the oracle drew them (`lab/research/
    /// mp-original/menus-and-flow.md`, "Match end and STATS"); the results replace the HUD.
    #[test]
    fn the_end_matches_the_originals_draws() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            eprintln!("no pack hud/end.json: skipping");
            return;
        };
        let lives = Shown {
            lives: Some(1),
            ..state()
        };
        let tex = texture("lives");
        let one = ([7.0, 6.0, 5.0, 7.0], [8.0, 0.0, 13.0, 7.0]);
        assert_eq!(rects(&hud, &lives, &tex), [one]);
        let out = Shown {
            skull: Some(16),
            ..state()
        };
        assert!(rects(&hud, &out, &texture("radar")).is_empty());
        assert_eq!(
            rects(&hud, &out, &texture("skull"))[0].0,
            [130.0, 92.0, 32.0, 32.0]
        );
        let small = Shown {
            skull: Some(2),
            ..out
        };
        assert_eq!(
            rects(&hud, &small, &texture("skull"))[0].0,
            [144.0, 106.0, 4.0, 4.0]
        );
        let over = Shown {
            results: Some((
                vec![(0, [1, 9, 2, 0, 1, 0]), (1, [3, 0, 0, 0, 1, 0])],
                1,
                255,
                0,
            )),
            ..state()
        };
        let at = |name: &str| -> Vec<[f32; 4]> {
            (rects(&hud, &over, &texture(name)).into_iter())
                .map(|r| r.0)
                .collect()
        };
        assert_eq!(at("stats"), [[116.0, 4.0, 64.0, 32.0]]);
        assert_eq!(at("restart"), [[51.0, 164.0, 64.0, 32.0]]);
        assert_eq!(at("quit_lit"), [[181.0, 164.0, 64.0, 32.0]]);
        assert_eq!(at("stick"), [[132.0, 164.0, 32.0, 32.0]]);
        let icons = [
            [129.0, 49.0, 16.0, 16.0],
            [158.0, 49.0, 16.0, 16.0],
            [177.0, 49.0, 32.0, 16.0],
        ];
        assert_eq!(
            (at("stat_icons"), at("stat_deaths")),
            (icons.into(), vec![[218.0, 49.0, 16.0, 15.0]])
        );
        assert_eq!(
            at("players"),
            [[60.0, 64.0, 16.0, 16.0], [60.0, 79.0, 16.0, 16.0]]
        );
        assert!(at("atlas").is_empty() && at("restart_lit").is_empty());
        // P1's "1st 9% 2 0 1" in red: 7 quads (the suffix one), each over a black shadow.
        let red = (sprites(&hud, &over).into_iter()).filter(|q| q.colour == [255, 0, 0, 255]);
        assert_eq!(red.count(), 7);
    }

    /// More than the original's 4 humans: the rows share the 4 rows' height, each scaled down, so
    /// 8 end no lower than 4 do (Total War's lower layout here) with half their height.
    #[test]
    fn eight_rows_fit_in_four() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            return;
        };
        // The text and labels' lowest bottom and tallest quad, with n rows.
        let block = |n: usize| {
            let over = Shown {
                results: Some((
                    (0..n).map(|n| (n % 2, [1, 99, 22, 0, 11, 0])).collect(),
                    0,
                    255,
                    0,
                )),
                header: Some(Header::Draw),
                ..state()
            };
            let text: Vec<[f32; 4]> = (sprites(&hud, &over).into_iter())
                .filter(|q| match &q.tex {
                    Tex::Glyph(_) => true,
                    Tex::Texture(name) => name.starts_with("rank_") || name == "players",
                    Tex::Solid | Tex::Tint => false,
                })
                .map(|q| q.rect)
                .collect();
            let bottom = text.iter().map(|r| r[1] + r[3]).fold(0.0, f32::max);
            (bottom, text.iter().map(|r| r[3]).fold(0.0, f32::max))
        };
        let ((b4, h4), (b8, h8)) = (block(4), block(8));
        assert!(
            b8 <= b4 && h8 <= h4 / 2.0,
            "4 rows {b4} {h4}, 8 rows {b8} {h8}"
        );
    }

    /// The pack's labels stop at P4: rows 5 and 6 get "P5" and "P6" in the HUD's font, in their
    /// row's colour (white past the 4 colours), where the labels go, each in its own row's band.
    #[test]
    fn rows_past_four_are_labelled() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            return;
        };
        let over = Shown {
            results: Some((
                (0..6).map(|n| (n, [1, 99, 22, 0, 11, 0])).collect(),
                0,
                255,
                0,
            )),
            ..state()
        };
        let all = sprites(&hud, &over);
        let labels = (all.iter())
            .filter(|q| matches!(&q.tex, Tex::Texture(t) if t == "players"))
            .count();
        // The label column (x 60-76), white glyphs only (not their black shadows).
        let tops: Vec<f32> = (all.iter())
            .filter(|q| matches!(q.tex, Tex::Glyph(_)) && q.colour == [255; 4])
            .filter(|q| q.rect[0] >= 60.0 && q.rect[0] + q.rect[2] <= 76.0)
            .map(|q| q.rect[1])
            .collect();
        // Rows 4/6 of the 15-pixel step apart: P5's band from y 104, P6's from 114.
        let band = |lo: f32| {
            tops.iter()
                .filter(|&&y| (lo..lo + 10.0).contains(&y))
                .count()
        };
        assert_eq!((labels, tops.len(), band(104.0), band(114.0)), (4, 4, 2, 2));
    }

    /// Total War's gas countdown: `(t + 59) / 60` of t = countdown + 1 (5 from 299 down to 240, 1
    /// from 59), none on the delivery's frame (300) or without one; the digit white, centred on x
    /// 150, its line's top at 86.
    #[test]
    fn gas_countdown_prints_as_the_game() {
        assert_eq!(
            [300, 299, 240, 239, 60, 59, 1, 0].map(countdown),
            [
                None,
                Some(5),
                Some(5),
                Some(4),
                Some(2),
                Some(1),
                Some(1),
                None
            ]
        );
        let Some(hud) = pack_hud() else {
            return;
        };
        let s = Shown {
            strip: Some([0, 0]),
            secs: None,
            countdown: Some(5),
            ..state()
        };
        let g = hud.font.iter().find(|g| g.ch == '5').unwrap();
        let [w, h] = g.size.map(f32::from);
        let [dx, dy] = g.offset.map(f32::from);
        let x = 150.0 - ((w - 1.0) / 2.0).floor() + dx;
        assert_eq!(glyphs(&hud, &s), [[x, 86.0 + dy, x + w, 86.0 + dy + h]]);
    }

    /// Total War's gassed screen against the original's draws (`total-war-effects.md` §2, e1's
    /// `hudwalk` at DL 1080, k = 8: st (2, 32) and (2, 0) at the frame's top-left, steps 1 and 0.5,
    /// env 00ff0020 and ff000010; e2's fade 0x70 … 0x10), under everything else.
    #[test]
    fn gas_haze_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else { return };
        let haze = |k| Shown {
            haze: Some(k),
            ..state()
        };
        let quads: Vec<Sprite> = sprites(&hud, &haze(8))
            .into_iter()
            .filter(|q| q.tex == texture("haze"))
            .collect();
        let at_origin = |q: &Sprite| {
            let step = (q.uv[2] - q.uv[0]) / q.rect[2];
            (
                step,
                [q.uv[0] - q.rect[0] * step, q.uv[1] - q.rect[1] * step],
            )
        };
        assert_eq!(
            quads
                .iter()
                .map(|q| (at_origin(q), q.colour))
                .collect::<Vec<_>>(),
            [
                ((1.0, [2.0, 32.0]), [0, 255, 0, 32]),
                ((0.5, [2.0, 0.0]), [255, 0, 0, 16])
            ]
        );
        let mut others = (sprites(&hud, &haze(8)).into_iter()).filter(|q| q.tex != texture("haze"));
        assert!(others.all(|q| q.z > quads[1].z && quads[1].z > quads[0].z));
        let alpha = |k| sprites(&hud, &haze(k))[0].colour[3];
        assert_eq!(
            [1, 2, 32, 132, 134, 136, 146, 148, 150].map(alpha),
            [4, 8, 128, 128, 0x70, 0x60, 0x10, 0, 0]
        );
    }

    /// Tank's HUD (`../lab/research/mp-original/tank-client-scratch/draws_*`): Deathmatch's with
    /// the power-up's icon at the weapon's place from `atlas2` (pw1 DLs 106 boost, 190 scope, 250
    /// shells, 346 shield), none without one (s0 200); through the scope (pw2 DLs 172-195) the
    /// back, the ring and the bar ([54, 67.5, 4, 80.75], within the client's unrounded half
    /// pixel), no dots. The countdown and the gas haze are Total War's (bm1 DLs 62-300, 335-360).
    #[test]
    fn tank_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else { return };
        let tank = |power| Shown {
            weapon: power,
            tank: true,
            ..state()
        };
        let icon = |power| rects(&hud, &tank(power), &texture("atlas2"));
        assert_eq!(
            icon(0x01),
            [([260.0, 5.0, 22.0, 14.0], [0.0, 25.0, 22.0, 39.0])]
        );
        assert_eq!(
            icon(0x02),
            [([260.0, 5.0, 22.0, 9.0], [0.0, 16.0, 22.0, 25.0])]
        );
        assert_eq!(
            icon(0x10),
            [([260.0, 5.0, 22.0, 12.0], [0.0, 39.0, 22.0, 51.0])]
        );
        assert_eq!(
            icon(0x80),
            [([260.0, 5.0, 22.0, 12.0], [0.0, 4.0, 22.0, 16.0])]
        );
        assert!(icon(0).is_empty() && icon(0x09).is_empty());
        let zoom = -27.8 / conker_camera::zoom_limit(0x1F); // on opening (V tank_pw2 171)
        let scoped = Shown {
            scope: Some(zoom),
            ..tank(0x02)
        };
        assert!(rects(&hud, &scoped, &texture("scope_dot")).is_empty());
        assert_eq!(rects(&hud, &scoped, &texture("scope")).len(), 4);
        let solid: Vec<_> = (sprites(&hud, &scoped).into_iter())
            .filter(|q| q.tex == Tex::Solid)
            .map(|q| (q.rect, q.colour[3]))
            .collect();
        assert_eq!(solid[0], ([52.0, 46.0, 8.0, 124.0], 65));
        let (bar, alpha) = solid[1];
        let want = [54.0, 67.5, 4.0, 80.75];
        assert!(
            bar.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.6),
            "{bar:?}"
        );
        assert_eq!(alpha, 0x82);
    }

    /// Race's HUD where the original drew it (race-pack-scratch/runs: hud-b2, one view, the grid
    /// of a 1-lap race: "1st", "LAP 1/1", "0:00:00", the map; hud-h230's three missiles, at the
    /// split view's 232 + 28, one more pixel up); no kills, rank or clock.
    #[test]
    fn race_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else { return };
        let race = |r: RaceHud| Shown {
            secs: None,
            radar: false,
            weapon: conker_rules::race::MISSILE,
            race: Some(r),
            ..state()
        };
        let grid = RaceHud {
            place: 1,
            lap: 0,
            laps: 1,
            items: 3,
            map: Some("race_map_low".into()),
            ..RaceHud::default()
        };
        assert_eq!(
            glyphs(&hud, &race(grid.clone())),
            [
                [136.0, 5.0, 142.0, 15.0],
                [141.0, 1.0, 156.0, 16.0],
                [10.0, 20.0, 18.0, 30.0],
                [17.0, 20.0, 27.0, 30.0],
                [26.0, 20.0, 35.0, 30.0],
                [38.0, 19.0, 44.0, 29.0],
                [43.0, 19.0, 48.0, 29.0],
                [47.0, 19.0, 53.0, 29.0],
                [130.0, 204.0, 138.0, 215.0],
                [138.0, 206.0, 142.0, 213.0],
                [141.0, 204.0, 149.0, 215.0],
                [148.0, 204.0, 156.0, 215.0],
                [154.0, 206.0, 158.0, 213.0],
                [157.0, 204.0, 165.0, 215.0],
                [164.0, 204.0, 172.0, 215.0],
            ]
        );
        let s = race(grid.clone());
        assert_eq!(
            rects(&hud, &s, &texture("race_map_low")),
            [([226.0, 13.0, 64.0, 64.0], [0.0, 0.0, 64.0, 64.0])]
        );
        let icons = rects(&hud, &s, &texture("atlas")).into_iter().map(|r| r.0);
        let icons: Vec<_> = icons.filter(|r| r[2] == 14.0).collect();
        assert_eq!(
            icons,
            [
                [260.0, 5.0, 14.0, 12.0],
                [242.0, 5.0, 14.0, 12.0],
                [224.0, 5.0, 14.0, 12.0]
            ]
        );
        // The race clock past 10 minutes 5 more to the left; the last lap at (25, 30); finished,
        // no place, lap or clock.
        let late = RaceHud {
            time: 10 * 3600 + 61 * 60 + 59,
            ..grid.clone()
        };
        let g = glyphs(&hud, &race(late));
        assert_eq!(g[8][0], 125.0);
        let last = RaceHud {
            lap: 1,
            last: 61 * 60 + 30,
            ..grid
        };
        assert_eq!(glyphs(&hud, &race(last))[0], [9.0, 30.0, 15.0, 40.0]);
        // The STATS in the level (V res 320): "STATS", the label at (72, 70), ':' at 88, the 1 and
        // its suffix at (100, 68), the total from 122, "(" at 168, the best from 172, ")" at 214.
        if hud.end.is_none() {
            return;
        }
        let stats = Shown {
            results: Some((Vec::new(), 0, 255, 0)),
            race_rows: Some(vec![[1, 56, 110]]),
            ..race(RaceHud::default())
        };
        assert_eq!(
            rects(&hud, &stats, &texture("stats"))[0].0,
            [116.0, 4.0, 64.0, 32.0]
        );
        assert_eq!(
            rects(&hud, &stats, &texture("players"))[0].0,
            [72.0, 70.0, 16.0, 16.0]
        );
        assert!(rects(&hud, &stats, &texture("stat_icons")).is_empty());
        let x: Vec<f32> = glyphs(&hud, &stats).iter().map(|g| g[0]).collect();
        assert_eq!(
            x,
            [
                88.0, 95.0, 100.0, 122.0, 130.0, 133.0, 140.0, 146.0, 149.0, 156.0, 168.0, 172.0
            ]
            .into_iter()
            .chain([180.0, 183.0, 190.0, 196.0, 199.0, 206.0, 214.0])
            .collect::<Vec<_>>()
        );
        assert_eq!(glyphs(&hud, &stats)[2], [100.0, 68.0, 115.0, 83.0]);
    }

    /// Total War's strip where the original drew it (the gas run's DL 3000: timed, Tediz 4,
    /// squirrels 2), no kills or rank; its results: the team header, the rest 40 lower, rows in
    /// their team's colour (`lab/research/mp-original/total-war-teams.md` §6).
    #[test]
    fn total_war_matches_the_originals_draws() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            eprintln!("no pack hud/end.json: skipping");
            return;
        };
        let at =
            |s: &Shown, name: &str| -> Vec<([f32; 4], [f32; 4])> { rects(&hud, s, &texture(name)) };
        let timed = Shown {
            strip: Some([2, 4]),
            ..state()
        };
        assert_eq!(
            at(&timed, "teams"),
            [
                ([198.0, 196.0, 16.0, 16.0], [0.0, 16.0, 16.0, 32.0]),
                ([66.0, 196.0, 16.0, 16.0], [16.0, 16.0, 32.0, 32.0]),
            ]
        );
        let grey = (sprites(&hud, &timed).into_iter()).filter(|q| q.colour == [192, 192, 192, 255]);
        let grey: Vec<_> = grey.map(|q| q.rect).collect();
        assert_eq!(grey, [[215.0, 197.0, 7.0, 10.0], [83.0, 197.0, 8.0, 11.0]]);
        // The clock's four glyphs only: no kills or rank.
        let white = glyphs(&hud, &timed);
        assert!(white.len() == 4 && white.iter().all(|g| g[1] >= 198.0));
        let untimed = Shown {
            strip: Some([1, 0]),
            lives: Some(1),
            secs: None,
            ..state()
        };
        let uv: Vec<_> = at(&untimed, "teams").into_iter().map(|r| r.1).collect();
        assert_eq!(uv, [[0.0, 0.0, 16.0, 16.0], [16.0, 0.0, 32.0, 16.0]]);
        // Over: SQUIRRELS WIN, the columns' icons at y 89, the labels at 104, P1 (a Tedi) in blue
        // at 106, P2 (a squirrel) in red at 121; the strip in the results' anchor.
        let over = |header| Shown {
            results: Some((
                vec![(1, [2, 0, 0, 0, 0, 0]), (0, [1, 50, 1, 0, 0, 0])],
                0,
                255,
                0,
            )),
            header: Some(header),
            ..timed.clone()
        };
        let rect = |s: &Shown, name: &str| -> Vec<[f32; 4]> {
            at(s, name).into_iter().map(|r| r.0).collect()
        };
        let won = over(Header::Team(0));
        assert_eq!(rect(&won, "stats"), [[116.0, 4.0, 64.0, 32.0]]);
        assert_eq!(
            (rect(&won, "squirrels"), rect(&won, "win")),
            (
                vec![[77.0, 34.0, 96.0, 32.0]],
                vec![[172.0, 34.0, 64.0, 32.0]]
            )
        );
        assert_eq!(
            rect(&won, "stat_icons"),
            [
                [129.0, 89.0, 16.0, 16.0],
                [158.0, 89.0, 16.0, 16.0],
                [177.0, 89.0, 32.0, 16.0],
            ]
        );
        assert_eq!(rect(&won, "stat_deaths"), [[218.0, 89.0, 16.0, 15.0]]);
        assert_eq!(
            rect(&won, "players"),
            [[60.0, 104.0, 16.0, 16.0], [60.0, 119.0, 16.0, 16.0]]
        );
        let first = |colour: Rgba| {
            (sprites(&hud, &won).into_iter())
                .find(|q| q.colour == colour && matches!(q.tex, Tex::Glyph(_)))
                .map(|q| q.rect[1])
        };
        assert_eq!(
            (first([0, 0, 255, 255]), first([255, 0, 0, 255])),
            (Some(106.0), Some(121.0))
        );
        let strip = (sprites(&hud, &won).into_iter()).filter(|q| q.tex == texture("teams"));
        assert!(
            strip
                .map(|q| q.anchor)
                .eq([Anchor(Horizontal::Centre, Vertical::Middle); 2])
        );
        assert_eq!(
            rect(&over(Header::Team(1)), "tediz"),
            [[81.0, 34.0, 64.0, 32.0]]
        );
        assert_eq!(
            rect(&over(Header::Team(1)), "win"),
            [[151.0, 34.0, 64.0, 32.0]]
        );
        assert_eq!(
            rect(&over(Header::Draw), "draw"),
            [[116.0, 34.0, 64.0, 32.0]]
        );
        let you = over(Header::You(false));
        assert_eq!(
            (rect(&you, "you"), rect(&you, "lose"), rect(&you, "win")),
            (
                vec![[86.0, 34.0, 64.0, 32.0]],
                vec![[146.0, 34.0, 64.0, 32.0]],
                vec![]
            )
        );
    }

    /// War: Colors' HUD where the original drew it (`lab/research/mp-original/colors-scratch/hw`:
    /// e0 DL 360, a squirrel carrying the red flag, timed, 0-0; e1 DL 325, both flags carried): the
    /// heads on the strip, the scores in grey, the red flag's icon at x 236 and the green's at 40;
    /// the icons' blink; the capture's fade.
    #[test]
    fn colors_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else {
            return;
        };
        let carrying = Shown {
            strip: Some([0, 0]),
            flags: Some([true, false]),
            ..state()
        };
        assert_eq!(
            rects(&hud, &carrying, &texture("teams")),
            [
                ([198.0, 196.0, 16.0, 16.0], [0.0, 0.0, 16.0, 16.0]),
                ([66.0, 196.0, 16.0, 16.0], [16.0, 0.0, 32.0, 16.0]),
                ([236.0, 196.0, 16.0, 16.0], [16.0, 16.0, 32.0, 32.0]),
            ]
        );
        let grey =
            (sprites(&hud, &carrying).into_iter()).filter(|q| q.colour == [192, 192, 192, 255]);
        let grey: Vec<_> = grey.map(|q| q.rect).collect();
        assert_eq!(grey, [[215.0, 197.0, 8.0, 11.0], [83.0, 197.0, 8.0, 11.0]]);
        let both = Shown {
            flags: Some([true, true]),
            ..carrying.clone()
        };
        assert!(
            rects(&hud, &both, &texture("teams"))
                .contains(&([40.0, 196.0, 16.0, 16.0], [0.0, 16.0, 16.0, 32.0]))
        );
        let blink = Shown {
            flags: Some([false, false]),
            ..carrying
        };
        assert_eq!(rects(&hud, &blink, &texture("teams")).len(), 2);
        assert_eq!((0..32).filter(|&vi| flag_icons(vi)).count(), 21);
        assert!([18, 26, 30, 12].map(flag_icons) == [true; 4]);
        assert!([0, 10, 2].map(flag_icons) == [false; 3]);
        // Out over 60 VIs (30 frames), in over 100 (50).
        assert_eq!(
            [0, 15, 30, 40].map(|k| capture_fade(true, 60, k)),
            [0, 127, 255, 255]
        );
        assert_eq!(
            [0, 25, 50].map(|k| capture_fade(false, 100, k)),
            [255, 128, 0]
        );
        // A fade covers the HUD and the iris (V c1a DL 456); the end phase's (V c1b DL 428-436).
        let faded = Shown {
            fade: Some(0x77),
            respawn_in: Some(1),
            ..both
        };
        let quads = sprites(&hud, &faded);
        let top = (quads.iter()).max_by(|a, b| a.z.total_cmp(&b.z)).unwrap();
        assert_eq!(top.colour, [0, 0, 0, 0x77]);
        assert_eq!(
            quads.iter().filter(|q| q.z >= 9.0).count(),
            1 + 8,
            "the iris under it"
        );
        assert_eq!(
            [1, 7, 13, 15, 16, 20].map(end_fade),
            [16, 0x70, 0xD0, 0xF0, 255, 255]
        );
    }

    /// War: Beach's HUD where the original drew it (`lab/research/mp-original/
    /// beach-presentation-scratch/hw/det1_dl_318`: a villager alone, 81 s to escape, Tediz 0,
    /// villagers 1): the heads of `beach_teams` (the Tedi's row 0 at x 66, the villager's row 16 at
    /// 198), the scores in grey, the countdown white at the top centre; no radar, no icon for 0x3E;
    /// a Tedi has no countdown. The escape's dim and fade in, the strike's flash (V e1 DL 245-270,
    /// ev3 DL 1046-1060); STATS: FRENCHYS WIN, no villager human's row.
    #[test]
    fn beach_matches_the_originals_draws() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            eprintln!("no pack hud/end.json: skipping");
            return;
        };
        let villager = Shown {
            strip: Some([1, 0]),
            beach: true,
            escape: Some(81),
            weapon: 0x3E,
            radar: false,
            secs: None,
            ..state()
        };
        assert_eq!(
            rects(&hud, &villager, &texture("beach_teams")),
            [
                ([198.0, 196.0, 16.0, 16.0], [0.0, 16.0, 16.0, 32.0]),
                ([66.0, 196.0, 16.0, 16.0], [0.0, 0.0, 16.0, 16.0]),
            ]
        );
        assert!(rects(&hud, &villager, &texture("teams")).is_empty());
        let grey =
            (sprites(&hud, &villager).into_iter()).filter(|q| q.colour == [192, 192, 192, 255]);
        let grey: Vec<_> = grey.map(|q| q.rect).collect();
        assert_eq!(grey, [[215.0, 197.0, 6.0, 10.0], [83.0, 197.0, 8.0, 11.0]]);
        assert_eq!(
            glyphs(&hud, &villager),
            [[140.0, 6.0, 148.0, 17.0], [147.0, 6.0, 153.0, 16.0]]
        );
        assert!(rects(&hud, &villager, &texture("radar")).is_empty());
        // The atlas' only quads are health's pieces (5 wide): no weapon icon.
        let atlas = (rects(&hud, &villager, &texture("atlas")).into_iter()).chain(rects(
            &hud,
            &villager,
            &texture("atlas2"),
        ));
        assert!(atlas.map(|r| r.0[2]).all(|w| w == 5.0));
        let tedi = Shown {
            escape: None,
            weapon: 0x24,
            ..villager.clone()
        };
        assert!(glyphs(&hud, &tedi).is_empty());
        // The escape: 0x11 for the freeze's last 30 frames (the re-sent fade), then 100 VIs in.
        assert_eq!(
            [31, 30, 1].map(|f| escape_fade(f, None)),
            [None, Some(0x11), Some(0x11)]
        );
        assert_eq!(
            [2, 4, 6, 20, 50].map(|k| escape_fade(0, Some(k))),
            [Some(0xF5), Some(0xEB), Some(0xE1), Some(0x99), None]
        );
        assert_eq!(
            [0, 7, 13, 14].map(strike_flash),
            [
                Some([255, 0, 0, 248]),
                Some([0, 0, 0, 194]),
                Some([0, 0, 0, 27]),
                None
            ]
        );
        // STATS: the villagers (team 0) win on score; only the Tedi human has a row.
        let c = |slot, team, score| Combat {
            slot,
            team,
            score,
            ..Combat::default()
        };
        let (frenchy, tedi, bot) = (c(0, 0, 3), c(1, 1, 1), c(2, 0, 3));
        let all = [(true, &frenchy), (true, &tedi), (false, &bot)];
        assert_eq!(
            rows(&all, 0, true, Some(0), true, false),
            [(1, [2, 0, 0, 0, 0, 0])]
        );
        assert_eq!(header(&all, 1, Some(0)), Header::Team(0));
        let over = |header| Shown {
            results: Some((rows(&all, 0, true, Some(0), true, false), 0, 255, 0)),
            header: Some(header),
            ..villager.clone()
        };
        let rect = |s: &Shown, name: &str| -> Vec<[f32; 4]> {
            (rects(&hud, s, &texture(name)).into_iter())
                .map(|r| r.0)
                .collect()
        };
        let won = over(Header::Team(0));
        assert_eq!(
            (rect(&won, "frenchys"), rect(&won, "win")),
            (
                vec![[65.0, 34.0, 96.0, 32.0]],
                vec![[151.0, 34.0, 64.0, 32.0]]
            )
        );
        assert!(rect(&won, "squirrels").is_empty() && rect(&won, "players").len() == 1);
        assert_eq!(
            rect(&over(Header::Team(1)), "tediz"),
            [[81.0, 34.0, 64.0, 32.0]]
        );
        assert_eq!(
            rect(&over(Header::Draw), "draw"),
            [[116.0, 34.0, 64.0, 32.0]]
        );
        assert_eq!(rect(&won, "beach_teams").len(), 2);
    }

    /// War: Raptor's HUD where the original drew it (`lab/research/mp-original/
    /// raptor-client-scratch`): h1 DL 40 (a raptor at the start: the strip's heads of
    /// `raptor_teams`, the raptor's row 0 at x 66, the caveman's row 16 at 198, no weapon icon), DL
    /// 101 (holding a body, the blink shown: the caveman's head at [148, 3]); h2 DL 235 (a caveman
    /// carrying an egg, shown: `atlas2`'s egg at [136, 3], no weapon icon), DL 240 (hidden: no
    /// icon). STATS (s1 DL 560): the raptor's row in blue (team 1), 1st, the strip under the rows.
    #[test]
    fn raptor_matches_the_originals_draws() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            eprintln!("no pack hud/end.json: skipping");
            return;
        };
        let raptor = Shown {
            strip: Some([0, 1]),
            raptor: Some([false, false]),
            weapon: 0,
            ..state()
        };
        assert_eq!(
            rects(&hud, &raptor, &texture("raptor_teams")),
            [
                ([198.0, 196.0, 16.0, 16.0], [0.0, 16.0, 16.0, 32.0]),
                ([66.0, 196.0, 16.0, 16.0], [0.0, 0.0, 16.0, 16.0]),
            ]
        );
        assert!(rects(&hud, &raptor, &texture("teams")).is_empty());
        let grey =
            (sprites(&hud, &raptor).into_iter()).filter(|q| q.colour == [192, 192, 192, 255]);
        let x: Vec<_> = grey.map(|q| q.rect[0]).collect();
        assert_eq!(x, [215.0, 83.0]);
        let atlas = |s: &Shown| {
            let a = rects(&hud, s, &texture("atlas")).into_iter();
            a.chain(rects(&hud, s, &texture("atlas2")))
                .collect::<Vec<_>>()
        };
        assert!(atlas(&raptor).iter().all(|r| r.0[2] == 5.0));
        let body = Shown {
            raptor: Some([false, true]),
            ..raptor.clone()
        };
        assert_eq!(
            rects(&hud, &body, &texture("raptor_teams"))[2],
            ([148.0, 3.0, 12.0, 15.0], [0.0, 16.0, 12.0, 31.0])
        );
        let egg = Shown {
            raptor: Some([true, false]),
            ..raptor.clone()
        };
        let eggs: Vec<_> = (atlas(&egg).into_iter())
            .filter(|r| r.0[2] != 5.0)
            .collect();
        assert_eq!(eggs, [([136.0, 3.0, 12.0, 15.0], [48.0, 42.0, 60.0, 57.0])]);
        assert_eq!(rects(&hud, &raptor, &texture("raptor_teams")).len(), 2);
        // STATS: the human raptor's team won.
        let c = |slot, team| Combat {
            slot,
            team,
            ..Combat::default()
        };
        let (human, bot) = (c(0, 1), c(1, 0));
        let all = [(true, &human), (false, &bot)];
        let rows = rows(&all, 0, true, Some(1), false, false);
        assert_eq!(rows, [(1, [1, 0, 0, 0, 0, 0])]);
        let over = Shown {
            results: Some((rows, 0, 255, 0)),
            header: Some(Header::You(true)),
            ..raptor.clone()
        };
        assert_eq!(rects(&hud, &over, &texture("raptor_teams")).len(), 2);
    }

    /// War: Heist's strip where the original drew it (`lab/research/mp-original/
    /// heist-presentation-scratch/hud`, 3 bags to win): hud1 DL 341 (REDS carry in the blink's
    /// shown phase: their "+1" pip and the bag icon), 340 (its hidden phase: neither), 401 (REDS
    /// 1 bag); hud2 DL 300 (GREENS and YELLOWS empty: their skulls), 1000 (BLUES carry); hud3 DL
    /// 200 (BLUES out: their skull). No kills or rank, the clock white. STATS (`outro/stats_cs2_
    /// dl_790`, `stats_cs3b_dl_970`): YOU WIN with one human, GREENS WIN with two (both 2nd, the
    /// tie shared), the columns at 89, the rows in their team's colour, the bags second.
    #[test]
    fn heist_matches_the_originals_draws() {
        let Some(hud) = pack_hud().filter(|h| h.end.is_some()) else {
            eprintln!("no pack hud/end.json: skipping");
            return;
        };
        let (grey, red, blue, green, yellow) = (
            [64, 64, 64, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [0, 255, 0, 255],
            [255, 255, 0, 255],
        );
        let xs = [
            [24.0, 32.0, 40.0],
            [97.0, 105.0, 113.0],
            [170.0, 178.0, 186.0],
            [243.0, 251.0, 259.0],
        ];
        let colours = [red, blue, green, yellow];
        // The DL's pips: (x, colour), each 7 x 7 at y 200 of texels [14, 16, 7, 7].
        let pips = |s: &Shown| -> Vec<(f32, Rgba)> {
            (sprites(&hud, s).into_iter())
                .filter(|q| q.tex == texture("pips"))
                .map(|q| {
                    assert_eq!(&q.rect[1..], [200.0, 7.0, 7.0]);
                    assert_eq!(q.uv, [14.0, 16.0, 21.0, 23.0]);
                    (q.rect[0], q.colour)
                })
                .collect()
        };
        let expect = |filled: [usize; 4], alive: [bool; 4]| -> Vec<(f32, Rgba)> {
            (0..4)
                .filter(|&t| alive[t])
                .flat_map(|t| {
                    (xs[t].iter().enumerate())
                        .map(move |(i, &x)| (x, if i < filled[t] { colours[t] } else { grey }))
                })
                .collect()
        };
        let skulls = |s: &Shown| -> Vec<([f32; 4], Rgba)> {
            (sprites(&hud, s).into_iter())
                .filter(|q| q.tex == texture("atlas2") && q.uv == [48.0, 20.0, 64.0, 36.0])
                .map(|q| (q.rect, q.colour))
                .collect()
        };
        let bag = ([142.0, 3.0, 12.0, 11.0], [50.0, 26.0, 62.0, 37.0]);
        let carrying = Shown {
            heist: Some(Bags {
                limit: 3,
                alive: [true; 4],
                carrying: Some(0),
                ..Bags::default()
            }),
            ..state()
        };
        assert_eq!(pips(&carrying), expect([1, 0, 0, 0], [true; 4]));
        assert!(rects(&hud, &carrying, &texture("atlas")).contains(&bag));
        // The clock's glyphs only: no kills, no rank.
        let text = glyphs(&hud, &carrying);
        assert!(!text.is_empty() && text.iter().all(|g| g[1] >= 198.0));
        let hidden = Shown {
            heist: Some(Bags {
                carrying: None,
                ..carrying.heist.unwrap()
            }),
            ..carrying.clone()
        };
        assert_eq!(pips(&hidden), expect([0; 4], [true; 4]));
        assert!(!rects(&hud, &hidden, &texture("atlas")).contains(&bag));
        let scored = Shown {
            heist: Some(Bags {
                scores: [1, 0, 0, 0],
                ..hidden.heist.unwrap()
            }),
            ..hidden.clone()
        };
        assert_eq!(pips(&scored), expect([1, 0, 0, 0], [true; 4]));
        let two = [true, true, false, false];
        let hud2 = Shown {
            heist: Some(Bags {
                limit: 3,
                alive: two,
                carrying: Some(1),
                ..Bags::default()
            }),
            ..state()
        };
        assert_eq!(pips(&hud2), expect([0, 1, 0, 0], two));
        assert_eq!(
            skulls(&hud2),
            [
                ([174.0, 194.0, 16.0, 16.0], green),
                ([247.0, 194.0, 16.0, 16.0], yellow)
            ]
        );
        let out = [true, false, true, true];
        let hud3 = Shown {
            heist: Some(Bags {
                limit: 3,
                alive: out,
                ..Bags::default()
            }),
            ..state()
        };
        assert_eq!(pips(&hud3), expect([0; 4], out));
        assert_eq!(skulls(&hud3), [([101.0, 194.0, 16.0, 16.0], blue)]);
        // The radar: hud1 DL 296's free bag under P1, white [258, 44, 2, 2]; hud2 DL 1060's BLUES
        // carrier (hidden phase) 4040ff.
        let radar = |blips| {
            let s = Shown {
                blips,
                ..carrying.clone()
            };
            (sprites(&hud, &s).into_iter())
                .filter(|q| q.rect[2] == 2.0)
                .map(|q| (q.rect, q.colour))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            radar(vec![([0.0, 0.0], 0, hud.heist.cash)]),
            [([258.0, 44.0, 2.0, 2.0], [255; 4])]
        );
        assert_eq!(hud.heist.radar[1], [64, 64, 255, 255]);
        // STATS: each row's text in its colour (x0, y0, x1, y1), its shadow 1 right and down.
        let c = |slot, team, score| Combat {
            slot,
            team,
            score,
            bags: score,
            ..Combat::default()
        };
        let row_text = |s: &Shown, colour: Rgba| -> Vec<[f32; 4]> {
            (sprites(&hud, s).into_iter())
                .filter(|q| q.colour == colour)
                .filter(|q| {
                    matches!(&q.tex, Tex::Glyph(_))
                        || q.tex == texture("rank_st")
                        || q.tex == texture("rank_nd")
                })
                .map(|q| {
                    [
                        q.rect[0],
                        q.rect[1],
                        q.rect[0] + q.rect[2],
                        q.rect[1] + q.rect[3],
                    ]
                })
                .collect()
        };
        let rect = |s: &Shown, name: &str| -> Vec<[f32; 4]> {
            (rects(&hud, s, &texture(name)).into_iter())
                .map(|r| r.0)
                .collect()
        };
        let over = |all: &[(bool, &Combat)], header| Shown {
            results: Some((rows(all, 0, true, Some(0), false, true), 0, 255, 0)),
            header: Some(header),
            ..hidden.clone()
        };
        // cs2: REDS (the one human) delivered the one bag.
        let (p1, b1, b2, b3) = (c(0, 0, 1), c(1, 1, 0), c(2, 2, 0), c(3, 3, 0));
        let all = [(true, &p1), (false, &b1), (false, &b2), (false, &b3)];
        assert_eq!(header(&all, 0, Some(0)), Header::You(true));
        let cs2 = over(&all, Header::You(true));
        assert_eq!(
            (rect(&cs2, "you"), rect(&cs2, "win")),
            (
                vec![[86.0, 34.0, 64.0, 32.0]],
                vec![[146.0, 34.0, 64.0, 32.0]]
            )
        );
        assert_eq!(
            [
                rect(&cs2, "stat_deaths"),
                rect(&cs2, "stat_icons"),
                rect(&cs2, "players")
            ],
            [
                vec![[103.0, 89.0, 16.0, 16.0], [222.0, 89.0, 16.0, 15.0]],
                vec![
                    [133.0, 89.0, 16.0, 16.0],
                    [162.0, 89.0, 16.0, 16.0],
                    [181.0, 89.0, 32.0, 16.0]
                ],
                vec![[34.0, 104.0, 16.0, 16.0]],
            ]
        );
        assert_eq!(
            row_text(&cs2, red),
            [
                [76.0, 106.0, 82.0, 116.0],
                [81.0, 102.0, 96.0, 117.0],
                [108.0, 106.0, 114.0, 116.0],
                [136.0, 106.0, 144.0, 117.0],
                [143.0, 108.0, 153.0, 118.0],
                [167.0, 106.0, 175.0, 117.0],
                [197.0, 106.0, 205.0, 117.0],
                [227.0, 106.0, 235.0, 117.0],
            ]
        );
        assert_eq!(
            row_text(&cs2, [0, 0, 0, 255])[0],
            [77.0, 107.0, 83.0, 117.0]
        );
        assert!(rect(&cs2, "teams").is_empty(), "no strip");
        // cs3b: REDS and BLUES humans, GREENS' bot delivered: both rows 2nd.
        let (p1, p2, b2) = (c(0, 0, 0), c(1, 1, 0), c(2, 2, 1));
        let all = [(true, &p1), (true, &p2), (false, &b2), (false, &b3)];
        assert_eq!(
            rows(&all, 0, true, Some(2), false, true),
            [(0, [2, 0, 0, 0, 0, 0]), (1, [2, 0, 0, 0, 0, 0])]
        );
        assert_eq!(header(&all, 0, Some(2)), Header::Team(2));
        let cs3b = over(&all, Header::Team(2));
        assert_eq!(
            (rect(&cs3b, "greens"), rect(&cs3b, "win")),
            (
                vec![[65.0, 34.0, 96.0, 32.0]],
                vec![[151.0, 34.0, 64.0, 32.0]]
            )
        );
        assert_eq!(rect(&cs3b, "players").len(), 2);
        let place = |colour| row_text(&cs3b, colour)[..3].to_vec();
        assert_eq!(
            [place(red), place(blue)],
            [
                [
                    [75.0, 106.0, 82.0, 116.0],
                    [81.0, 102.0, 96.0, 117.0],
                    [107.0, 106.0, 115.0, 117.0]
                ],
                [
                    [75.0, 121.0, 82.0, 131.0],
                    [81.0, 117.0, 96.0, 132.0],
                    [107.0, 121.0, 115.0, 132.0]
                ],
            ]
        );
    }

    /// A mounted gun's sight against the original's one view (`lab/research/mp-original/
    /// mounted-gun-scratch/one-view`: P1 on gun B, yaw 15872): every gun draw of each DL is one of
    /// ours, its rect within a quarter pixel, its colour (the handles' and flash's alpha), its
    /// mirroring and its tapes' texels (modulo a picture). 125: at rest, heading 108°, level. 221:
    /// barrel 0's flash (the right, its size the DL's) 2 frames after its shot, its handle kicking;
    /// 223: barrel 1's on its shot's frame (mirrored), the right handle 4 frames in; 225: barrel 1's
    /// at alpha 0, the left 2 frames in, the right 6, heading 177.19° at 40° up; 240: the left 5 in,
    /// the right at rest but lit 1 (221 and 223 turn up: their tapes are not compared).
    #[test]
    fn gun_matches_the_originals_draws() {
        let Some(hud) = pack_hud() else {
            return;
        };
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../lab/research/mp-original/mounted-gun-scratch/one-view"
        );
        let names = [
            ("802287a8", "gun_handle_lit"),
            ("802297b8", "gun_handle"),
            ("80231940", "gun_flash"),
            ("80207448", "gun_corner"),
            ("8020aaf0", "gun_tick_end"),
            ("8021cab8", "gun_tick_side"),
            ("8022a7c8", "reticle"),
            ("802262e0", "gun_tape_elevation"),
            ("8022abd8", "gun_tape_heading"),
        ];
        let sight = |heading, elevation, flash, kick| Sight {
            heading,
            elevation,
            flash,
            kick,
            ..Sight::default()
        };
        let (b0, b1) = (
            Some(([102.25, 181.75], 0x55)),
            Some(([95.25, 196.75], 0xF9)),
        );
        let cases = [
            ("hud125", sight(108.0, 0.0, [None; 2], [kick(0); 2]), true),
            (
                "hud221",
                sight(177.19, 0.0, [b0, None], [kick(0), kick(2)]),
                false,
            ),
            (
                "hud223",
                sight(177.19, 0.0, [None, b1], [kick(0), kick(4)]),
                false,
            ),
            (
                "hud225",
                sight(
                    177.19,
                    40.0,
                    [None, b1.map(|(size, _)| (size, 0))],
                    [kick(2), kick(6)],
                ),
                true,
            ),
            (
                "hud240",
                sight(177.19, 40.0, [None; 2], [kick(5), kick(10)]),
                true,
            ),
        ];
        for (dump, gun, tapes) in cases {
            let flashes = gun.flash.iter().flatten().count();
            let Ok(json) = std::fs::read(format!("{dir}/{dump}/draws.json")) else {
                eprintln!("no {dump}: skipping");
                return;
            };
            let draws: Vec<serde_json::Value> = serde_json::from_slice(&json).unwrap();
            let ours = sprites(
                &hud,
                &Shown {
                    gun: Some(gun),
                    ..state()
                },
            );
            let mut seen = 0;
            for d in &draws {
                let texture = d["texture"].as_str().unwrap();
                let env = u32::from_str_radix(d["env"].as_str().unwrap(), 16).unwrap();
                let env = env.to_be_bytes();
                let solid = d["combine"][0].as_str().unwrap().starts_with("(0-0)*0+ENV");
                let tex = match names.iter().find(|n| texture.starts_with(n.0)) {
                    _ if solid && [[255, 0, 0, 65], [255, 255, 0, 90]].contains(&env) => Tex::Solid,
                    Some((_, name)) => Tex::Texture((*name).into()),
                    None => continue,
                };
                let name = match &tex {
                    Tex::Texture(n) => n.as_str(),
                    _ => "",
                };
                if name.starts_with("gun_tape") && !tapes {
                    continue;
                }
                let colour = match name {
                    "gun_handle" | "gun_handle_lit" | "gun_flash" => [255, 255, 255, env[3]],
                    _ => env,
                };
                let r: [f32; 4] = serde_json::from_value(d["rect"].clone()).unwrap();
                let rect = [r[0], r[1], r[2] - r[0], r[3] - r[1]];
                let st: [f32; 2] = serde_json::from_value(d["st"].clone()).unwrap();
                let step: [f32; 2] = serde_json::from_value(d["step"].clone()).unwrap();
                let near = |a: f32, b: f32| {
                    let d = (a - b).rem_euclid(16.0);
                    d.min(16.0 - d) < 0.05
                };
                let texels = |q: &Sprite| match name {
                    "gun_tape_heading" => near(q.uv[0], st[0]),
                    "gun_tape_elevation" => near(q.uv[1], st[1]),
                    "" => true,
                    _ => [0, 1]
                        .iter()
                        .all(|&i| (step[i] < 0.0) == (q.uv[i + 2] < q.uv[i])),
                };
                let found = ours.iter().any(|q| {
                    q.tex == tex
                        && q.colour == colour
                        && (0..4).all(|i| (q.rect[i] - rect[i]).abs() <= 0.25)
                        && texels(q)
                });
                assert!(
                    found,
                    "{dump}: {name} {r:?} {env:?} st {st:?} step {step:?}"
                );
                seen += 1;
            }
            // 4 handle pictures, 2 bars and 9 sight pictures, the tapes and the flashes.
            assert_eq!(seen, 15 + 2 * usize::from(tapes) + flashes, "{dump}");
        }
    }

    /// Bevy's linear blend with [`display_alpha`]'s weight lands where the N64's display-space
    /// blend does: a black fade over any scene to 8/255 (28/255 unweighted), white over black. The
    /// flash's faint glow (alpha 25 and less) shows at 10% over black, as on the console, where
    /// the plain weight gave 35% (the pale box).
    #[test]
    fn hud_alphas_blend_as_the_console() {
        let (lin, show) = (Srgba::gamma_function, Srgba::gamma_function_inverse);
        for b in [0.1, 0.2, 0.5, 0.9] {
            for a in [0.1, 0.25, 0.5, 0.75, 0.9] {
                let fade = show(lin(b) * (1.0 - display_alpha(0.0, a)));
                assert!(
                    (fade - b * (1.0 - a)).abs() < 0.03,
                    "black {a} over {b}: {fade}"
                );
            }
        }
        for a in [0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!((show(display_alpha(1.0, a)) - a).abs() < 1e-5);
        }
        let glow = 25.0 / 255.0;
        assert!((show(display_alpha(1.0, glow)) - glow).abs() < 1e-5 && show(glow) > 0.34);
    }

    /// A shot flashes from its frame and kicks its side's handle from the next: barrel 0 the right.
    #[test]
    fn gun_shots_flash_and_kick_their_side() {
        let Some(hud) = pack_hud() else {
            return;
        };
        let f = &hud.gun.flash;
        let shot = |t| gun_fire(f, [Some((100, 7)), None], t);
        let [Some(([w, h], alpha)), None] = shot(100).flash else {
            panic!("no flash");
        };
        assert!((80.0..=120.0).contains(&w) && (160.0..=200.0).contains(&h) && alpha >= 200);
        assert_eq!(shot(100).kick, [(0.0, 0); 2]);
        let later = (101..=120).map(shot);
        assert!((later.clone()).all(|s| s.flash[1].is_none() && s.kick[0] == (0.0, 0)));
        assert!((later.clone()).skip(3).all(|s| s.flash[0].is_none()));
        assert!(later.map(|s| s.kick[1]).eq((1..=20).map(kick)));
        let left = gun_fire(f, [None, Some((100, 8))], 101);
        assert_eq!(left.kick, [kick(1), (0.0, 0)]);
        assert!(left.flash[0].is_none() && left.flash[1].is_some());
    }

    /// The handles' kick and the flash's fade against the one-view DLs (the handles' drop and their
    /// lit pictures' alpha; the flash's alpha): 221-225 the right handle 2, 4 and 6 frames after its
    /// shot, 225 and 240 the left 2 and 5; barrel 0's flash (life 7) 2 frames after its shot in
    /// 221, barrel 1's (life 6) on its shot's frame in 223 and 2 frames after in 225.
    #[test]
    fn gun_kicks_and_flashes_fade_as_the_original() {
        let quarter = |(drop, lit): (f32, u8)| ((drop * 4.0).floor() / 4.0, lit);
        let v = [1, 2, 4, 5, 6, 7, 16, 17].map(|k| quarter(kick(k)));
        assert_eq!(
            v.map(|k| k.0),
            [0.0, 10.0, 20.75, 18.75, 12.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(v.map(|k| k.1), [255, 0x53, 0x09, 0x03, 0x01, 1, 1, 0]);
        assert_eq!(kick(0), (0.0, 0));
        assert_eq!(flash_alpha(0xD0, 7, 2), Some(0x55));
        assert_eq!(flash_alpha(0xF9, 6, 0), Some(0xF9));
        assert_eq!(flash_alpha(0xF9, 6, 1), Some(0xAA));
        assert_eq!(flash_alpha(0xF9, 6, 2), Some(0));
        assert_eq!(flash_alpha(0xD0, 7, 4), None);
        assert_eq!(flash_alpha(0xD0, 8, 4), None);
    }

    /// The zoom bar's colour after the zoom stops, against the original's (V `mounted-gun-scratch/
    /// gc/gcA`: 29 and 33 of 35 frames left, ff2b00e2 and ff0e00f5).
    #[test]
    fn the_zoom_bar_flashes_red_and_fades() {
        let Some(hud) = pack_hud() else {
            return;
        };
        let g = &hud.gun;
        assert_eq!(bar_colour(g, None), g.bar_colour);
        assert_eq!(bar_colour(g, Some(0)), g.bar_flash);
        assert_eq!(bar_colour(g, Some(6)), [0xFF, 0x2B, 0x00, 0xE2]);
        assert_eq!(bar_colour(g, Some(2)), [0xFF, 0x0E, 0x00, 0xF5]);
        assert_eq!(bar_colour(g, Some(35)), g.bar_colour);
    }

    /// TW cs4's bubble and fills (`outros-leftovers.md` §3, §7, §8): the bubble opens at frame 8 and
    /// is 1/3, 2/3 and all of its 146 px on the next three; the match's end fade still covers frame
    /// 3 at 0xAF (DL 458) and is gone at 14; the first page's lines sit 11 then 12 apart (its middle
    /// line has no space), the third page's 12 and 12; the STATS fade in 8 a frame.
    #[test]
    fn outro_bubble_lines_and_fades() {
        let (Some(hud), Some(json)) = (
            pack_hud(),
            std::fs::read(pack_dir().join("maps/2d/outros.json")).ok(),
        ) else {
            eprintln!("no pack outros: skipping");
            return;
        };
        let outros: Vec<Outro> = serde_json::from_slice(&json).unwrap();
        let cs4 = &outros[1];
        let bubble = |k| {
            let quads: Vec<_> = (outro(&hud, cs4, k).into_iter())
                .filter(|q| q.tex == Tex::Texture("bubble".into()) && q.colour == [255; 4])
                .collect();
            quads.first().map(|q| q.rect[2])
        };
        let widths: Vec<_> = (8..12).map(bubble).collect();
        let third = 146.0 / 3.0;
        assert_eq!(widths, [None, Some(third), Some(2.0 * third), Some(146.0)]);
        let fills = |k| -> Vec<u8> {
            (outro(&hud, cs4, k).into_iter())
                .filter(|q| q.tex == Tex::Solid)
                .map(|q| q.colour[3])
                .collect()
        };
        assert_eq!(fills(3), [cs4.fade(6.0), 0xAF]);
        assert_eq!((fills(13), fills(14)), (vec![0, 15], vec![0]));
        let tops = |k| -> Vec<f32> {
            let mut tops: Vec<f32> = (outro(&hud, cs4, k).into_iter())
                .filter(|q| matches!(q.tex, Tex::Glyph(_)))
                .map(|q| q.rect[1])
                .collect();
            tops.sort_by(f32::total_cmp);
            tops.dedup();
            tops
        };
        // The glyphs' own top offsets (the apostrophe, the comma) aside: each line's tops.
        let first = tops(40);
        assert!(
            first.contains(&(first[0] + 11.0)) && first.contains(&(first[0] + 23.0)),
            "{first:?}"
        );
        let third_page = tops(200);
        assert!(
            third_page.contains(&(third_page[0] + 12.0))
                && third_page.contains(&(third_page[0] + 24.0)),
            "{third_page:?}"
        );
        assert_eq!([1, 2, 31, 32, 40].map(stats_alpha), [8, 16, 248, 255, 255]);
    }

    /// The outros' pages against the original's draws (one capture's I8 glyph rects): every glyph
    /// where the original drew it, cs4's first page's last two lines a pixel higher (its middle line
    /// has no space: 11 high). Total War: `lab/research/mp-original/total-war-outro-scratch/hw/
    /// cs*b_dl_*`, one capture a page (drawn 20 frames into it, its text in). Colors' cs1:
    /// `colors-outro-scratch/hw/cs1_dl_N` (frame N − 436), each of the bubble's places.
    #[test]
    fn outro_subtitles_match_the_originals_draws() {
        let research = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../lab/research/mp-original"
        );
        let load = |map: &str| -> Option<Vec<Outro>> {
            let json = std::fs::read(pack_dir().join(format!("maps/{map}/outros.json"))).ok()?;
            Some(serde_json::from_slice(&json).unwrap())
        };
        let (Some(hud), Some(war), Some(colors)) = (pack_hud(), load("2d"), load("30")) else {
            eprintln!("no pack outros: skipping");
            return;
        };
        let sorted = |mut r: Vec<[f32; 4]>| {
            r.sort_by(|a, b| a.partial_cmp(b).unwrap());
            r
        };
        // (outro, frame, capture)
        let mut captures = vec![];
        for (o, reads) in war.iter().zip([[482, 560, 650, 760], [470, 600, 700, 800]]) {
            let pages = &o.subtitles.as_ref().unwrap().pages;
            for (page, read) in pages.iter().zip(reads) {
                let file = format!("total-war-outro-scratch/hw/cs{}b_dl_{read}", o.cutscene);
                captures.push((o, conker_assets::outro::frame(page.t) + 20, file));
            }
        }
        for n in [461, 536, 636, 666, 756] {
            let file = format!("colors-outro-scratch/hw/cs1_dl_{n}");
            captures.push((&colors[0], n - 436, file));
        }
        for (o, k, file) in captures {
            let Ok(draws) = std::fs::read(format!("{research}/{file}/draws.json")) else {
                eprintln!("no {file}: skipping");
                return;
            };
            let draws: Vec<serde_json::Value> = serde_json::from_slice(&draws).unwrap();
            let want = (draws.iter())
                .filter(|d| d["format"] == "I8")
                .map(|d| serde_json::from_value::<[f32; 4]>(d["rect"].clone()).unwrap());
            let got = (outro(&hud, o, k).into_iter())
                .filter(|q| matches!(q.tex, Tex::Glyph(_)))
                .map(|q| {
                    [
                        q.rect[0],
                        q.rect[1],
                        q.rect[0] + q.rect[2],
                        q.rect[1] + q.rect[3],
                    ]
                });
            let page = format!("cutscene {} frame {k} ({file})", o.cutscene);
            assert_eq!(sorted(got.collect()), sorted(want.collect()), "{page}");
        }
    }

    /// The modern pickup prompt agrees with the server's gate (`can_take`): on a weapon's pickup,
    /// shown, it asks for Interact; not on grenades, a taken one, one out of reach, nor in Race
    /// (automatic there), nor with empty hands (taken by touch). The key is Interact's, as the
    /// layout types it once pressed.
    #[test]
    fn the_pickup_prompt_follows_the_gate_and_the_binding() {
        use conker_assets::{Gives, Pickup};
        let place = conker_rules::Placement {
            index: 0,
            pos: [0.0, 0.0, 2950.0],
            facing: 0,
            camera: [0.0; 3],
        };
        let empty = conker_rules::Fighter::new(&place, 0, 0);
        let f = conker_rules::Fighter {
            weapon: Some(0x39),
            ..empty
        };
        let at = |gives| Pickup {
            placement: 12,
            gives,
            position: [0.0, 0.0, 3000.0],
            respawn: None,
            roams: Vec::new(),
        };
        let (gun, grenades) = ([at(Gives::Weapon(0x09))], [at(Gives::Grenades)]);
        let m = MatchView::default();
        let line = Some("PRESS F TO PICK UP".to_string());
        assert_eq!(pickup_prompt(&gun, &m, &f, "F"), line);
        assert_eq!(pickup_prompt(&gun, &m, &empty, "F"), None);
        assert_eq!(pickup_prompt(&grenades, &m, &f, "F"), None);
        let taken = MatchView {
            taken: 1 << 12,
            ..MatchView::default()
        };
        assert_eq!(pickup_prompt(&gun, &taken, &f, "F"), None);
        let race = MatchView {
            race: true,
            ..MatchView::default()
        };
        assert_eq!(pickup_prompt(&gun, &race, &f, "F"), None);
        let mut far = f;
        far.body.pos[2] = 2800.0;
        assert_eq!(pickup_prompt(&gun, &m, &far, "F"), None);
        let bindings = Bindings::default();
        let interact_label = |b, l| key_label(b, Action::Interact, l, None);
        assert_eq!(interact_label(&bindings, None), "F");
        let mut labels = KeyLabels::default();
        labels.0.insert(
            KeyCode::KeyF,
            bevy::input::keyboard::Key::Character("g".into()),
        );
        assert_eq!(interact_label(&bindings, Some(&labels)), "G");
    }

    /// Decision 19's prompt agrees with the server's gate (`can_mount`): by a free gun, not a manned
    /// or leaving one, nor out of its reach. On the pad, prompts show its glyphs (C3's).
    #[test]
    fn the_mount_prompt_follows_the_gate_and_prompts_show_the_pads_glyphs() {
        use conker_rules::gun::{FREE, LEAVING};
        use conker_sim::net::GunView;
        let place = conker_rules::Placement {
            index: 0,
            pos: [0.0, 0.0, 250.0],
            facing: 0,
            camera: [0.0; 3],
        };
        let f = conker_rules::Fighter::new(&place, 0, 0);
        let gun = |state, gunner| MatchView {
            guns: vec![GunView {
                pos: [0.0, 0.0, 300.0],
                state,
                gunner,
                ..GunView::default()
            }],
            ..MatchView::default()
        };
        let line = Some("PRESS F TO MOUNT".to_string());
        assert_eq!(mount_prompt(&gun(FREE, None), &f, "F"), line);
        assert_eq!(mount_prompt(&gun(FREE, Some(1)), &f, "F"), None);
        assert_eq!(mount_prompt(&gun(LEAVING, None), &f, "F"), None);
        let mut far = f;
        far.body.pos[2] = 0.0;
        assert_eq!(mount_prompt(&gun(FREE, None), &far, "F"), None);
        let b = Bindings::default();
        let label = |g| key_label(&b, Action::Interact, None, g);
        assert_eq!(label(None), "F");
        assert_eq!(label(Some(Glyphs::Xbox)), "X");
        assert_eq!(label(Some(Glyphs::PlayStation)), "SQUARE");
        assert_eq!(key_label(&b, Action::Fire, None, None), "MOUSE 1");
        assert_eq!(key_label(&b, Action::Mute, None, Some(Glyphs::Xbox)), "M");
    }
}
