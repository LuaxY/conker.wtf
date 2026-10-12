//! The HUD the original draws over a Deathmatch player's view, from the pack's `hud/` ([`Hud`]:
//! pictures, layout and rules found in the oracle, `lab/hud`): health as chocolate, the held
//! weapon's icon, grenades while one is in the hand, kills and rank, the clock, the radar, the aim
//! reticle, the scope and the death iris; untimed, the lives left; out of lives, a skull; and once
//! the match is over, the results (`hud/end.json`, [`End`]). War: Total War draws its team strip in
//! place of the kills and rank, the radar's blips by team, its results' header, and a gassed
//! player's haze under the HUD ([`haze`]).
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
use conker_anim::combat::{GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW};
use conker_assets::hud::{
    Align, Anchor, End, Haze, Horizontal, Hud, Image as Picture, Placed, Rgba, Text, Vertical,
};
use conker_assets::outro::Outro;
use conker_rules::game::{Standing, places, team_places, team_result};
use conker_sim::net::{Combat, MatchView, Player, PlayerId};
use lightyear::prelude::{Interpolated, PeerId, Predicted};

use crate::connection::NetStatus;
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
    /// Grenades left while one is in the hand.
    pub grenades: Option<u8>,
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
    /// Looking through a scope: its zoom, 0 opened to 1 at the weapon's closest.
    pub scope: Option<f32>,
    /// Dead: frames to the respawn.
    pub respawn_in: Option<u16>,
    /// Untimed: the lives left.
    pub lives: Option<u8>,
    /// Out of lives: a skull over the view, no radar.
    pub out: bool,
    /// War: Total War: the team strip's numbers, [0] the squirrels', [1] the Tediz': their kills,
    /// untimed (`lives` shown) their lives left; in place of the own kills and rank.
    pub strip: Option<[u32; 2]>,
    /// Total War: the gas countdown's seconds ([`countdown`]).
    pub countdown: Option<u32>,
    /// Total War: frames into this player's gassed screen (1-150, [`Combat::haze`]).
    pub haze: Option<u32>,
    /// The match is over: the results' rows ([`rows`]: each with its colour) and the menu's choice
    /// (0 RESTART, 1 QUIT).
    pub results: Option<(Vec<Row>, usize)>,
    /// Over in Total War: the results' header.
    pub header: Option<Header>,
}

/// A results row ([`rows`]): its colour, an index of `results.colours`, and its five columns.
pub type Row = (usize, [u32; 5]);

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

fn text(hud: &Hud, line: &str, suffix: Option<usize>, t: &Text, z: f32) -> Vec<Sprite> {
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
/// War the place is the team's ([`team_places`], `0x800E0AC0[team]`).
pub fn rows(all: &[(bool, &Combat)], kill_limit: u8, total_war: bool) -> Vec<Row> {
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
    let place = if total_war {
        let teams = team_places(&standings);
        let of = |s: &Standing| teams.get(usize::from(s.team)).copied().unwrap_or(0);
        standings.iter().map(|s| s.as_ref().map_or(0, of)).collect()
    } else {
        places(&standings, kill_limit)
    };
    let mut humans: Vec<_> = all.iter().filter(|p| p.0).map(|p| p.1).collect();
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
            ];
            (colour, row)
        })
        .collect()
}

/// Total War's results header from everyone's (human, [`Combat`]) and the viewer's team: the
/// original's rule by the number of humans (`func_151E43DC`; V `to_draw`, `you_win`, `you_lose`,
/// `sq_win`: `lab/research/mp-original/total-war-teams.md` §4, §6).
pub fn header(all: &[(bool, &Combat)], own_team: u8) -> Header {
    let standings: Vec<_> = all.iter().map(|p| Some(standing(p.1))).collect();
    match team_result(&standings) {
        None => Header::Draw,
        Some(t) if all.iter().filter(|p| p.0).count() == 1 => Header::You(t == own_team),
        Some(t) => Header::Team(t),
    }
}

/// Total War's team strip (`func_151E9D18`): each team's icon (`lives`: untimed) and number, kept
/// at `anchor`.
fn strip(hud: &Hud, numbers: [u32; 2], lives: bool, anchor: Anchor) -> Vec<Sprite> {
    let t = &hud.strip;
    let icons = if lives { &t.lives } else { &t.kills };
    (0..2)
        .flat_map(|team| {
            let rect = t.rects[team].map(f32::from);
            let mut out = vec![quad(&icons[team], rect, anchor, [255; 4], 5.0)];
            let number = Text {
                anchor,
                ..t.numbers[team].clone()
            };
            out.extend(text(hud, &numbers[team].to_string(), None, &number, 5.0));
            out
        })
        .collect()
}

/// The results over the frozen level: the header, the columns' icons, a row per human, the menu
/// with `choice` lit; in Total War (`header`) its header over the rest, which goes `down` lower.
fn results(
    hud: &Hud,
    end: &End,
    rows: &[Row],
    choice: usize,
    header: Option<Header>,
) -> Vec<Sprite> {
    let (r, t) = (&end.results, &end.teams);
    let white = [255; 4];
    let rect = |r: [i16; 4]| r.map(f32::from);
    let down = if header.is_some() { t.down } else { 0 };
    let team_header: &[Placed] = match header {
        None => &[],
        Some(Header::Draw) => std::slice::from_ref(&t.draw),
        Some(Header::You(true)) => &t.won,
        Some(Header::You(false)) => &t.lost,
        Some(Header::Team(team)) => t.team_won.get(usize::from(team)).map_or(&[], |p| p),
    };
    let mut out: Vec<Sprite> = (std::iter::once(&r.header).chain(team_header))
        .chain([&r.stick])
        .map(|p| quad(&p.image, rect(p.rect), p.anchor, white, 5.0))
        .collect();
    for p in &r.icons {
        let [x, y, w, h] = p.rect;
        out.push(quad(
            &p.image,
            rect([x, y + down, w, h]),
            p.anchor,
            white,
            5.0,
        ));
    }
    for (k, b) in [&r.restart, &r.quit].into_iter().enumerate() {
        let image = if k == choice { &b.lit } else { &b.image };
        out.push(quad(image, rect(b.rect), b.anchor, white, 5.0));
    }
    for (n, (colour, row)) in rows.iter().enumerate() {
        let dy = r.rows[1] * n as i16 + down;
        if let Some(label) = r.labels.get(n) {
            let [_, _, w, h] = label.rect;
            let [x, y] = r.label;
            out.push(quad(
                label,
                rect([x, y + dy, w as i16, h as i16]),
                r.anchor,
                white,
                5.0,
            ));
        }
        let colour = r.colours.get(*colour).copied().unwrap_or(white);
        for (column, (&x, &v)) in r.columns.iter().zip(row).enumerate() {
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
            out.extend(text(hud, &line, suffix, &t, 5.0));
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

/// A War outro's picture at `t` ([`Outro`]), in place of the HUD: its fade over the whole view,
/// and the page shown in its speech bubble where the bubble is then (its quads, then the lines upper
/// case in black, left-aligned in a block centred on the text's x, 12 apart around its middle line's
/// top).
pub fn outro(hud: &Hud, o: &Outro, t: f32) -> Vec<Sprite> {
    let far = 10_000.0;
    let [r, g, b] = o.fade.rgb;
    let whole = [-far, -far, 2.0 * far, 2.0 * far];
    let centre = Anchor(Horizontal::Centre, Vertical::Middle);
    let mut out = vec![solid(whole, centre, [r, g, b, o.fade(t)], 9.0)];
    let (Some(s), Some(page)) = (&o.subtitles, o.page(t)) else {
        return out;
    };
    let (anchor, at, quads) = s.place(t);
    for (n, q) in quads.iter().enumerate() {
        out.push(Sprite {
            tex: Tex::Texture(q.texture.clone()),
            uv: q.uv,
            rect: q.rect.map(f32::from),
            anchor,
            colour: q.colour,
            z: 5.0 + 0.1 * n as f32,
        });
    }
    let lines: Vec<String> = page.lines.iter().map(|l| l.to_uppercase()).collect();
    let widest = lines.iter().map(|l| width(hud, l)).fold(0.0, f32::max);
    let [x, y] = at.map(f32::from);
    let first = y - 6.0 * (lines.len() - 1) as f32;
    for (n, line) in lines.iter().enumerate() {
        let t = Text {
            anchor,
            at: [
                (x - (widest / 2.0).ceil()) as i16,
                (first + 12.0 * n as f32) as i16,
            ],
            align: Align::Left,
            colour: [0, 0, 0, 255],
            shadow: None,
        };
        out.extend(text(hud, line, None, &t, 6.0));
    }
    out
}

/// The HUD's quads for `s`, in the frame (the original's draws: `lab/hud/README.md`).
pub fn sprites(hud: &Hud, s: &Shown) -> Vec<Sprite> {
    // Untimed matches count lives (the lives digit, the strip's lives).
    let untimed = s.lives.is_some();
    if let (Some((rows, choice)), Some(end)) = (&s.results, &hud.end) {
        let mut out = results(hud, end, rows, *choice, s.header);
        // Total War: the strip under the rows, live (`func_151EA15C`; V 0x800E0BD3 = 2).
        if let Some(n) = s.strip {
            out.extend(strip(hud, n, untimed, end.results.anchor));
        }
        return out;
    }
    let mut out = s.haze.map_or_else(Vec::new, |k| haze(&hud.haze, k));
    let white = [255; 4];
    // Untimed: the lives left; out of them, the skull.
    if let (Some(n), Some(end)) = (s.lives, &hud.end) {
        let l = &end.lives;
        let digit = &l.digits[usize::from(n).min(9)];
        let [_, _, w, h] = digit.rect.map(f32::from);
        let [x, y] = l.at.map(f32::from);
        out.push(quad(digit, [x, y, w, h], l.anchor, white, 3.0));
    }
    if let (true, Some(end)) = (s.out, &hud.end) {
        let k = &end.skull;
        out.push(quad(&k.image, k.rect.map(f32::from), k.anchor, white, 3.0));
    }
    // Radar: the disc, then a blip per player in range, with a bar on another level's.
    let r = &hud.radar;
    let [cx, cy] = r.centre;
    let half = r.size / 2.0;
    let radar = s.radar && !s.out;
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
            let [_, _, w, h] = g.icon.rect.map(f32::from);
            for k in 0..n {
                let x = f32::from(g.at[0] + g.step * i16::from(k));
                out.push(quad(
                    &g.icon,
                    [x, f32::from(g.at[1]), w, h],
                    g.anchor,
                    white,
                    3.0,
                ));
            }
        }
        None => {
            let w = &hud.weapon;
            if let Some(icon) = w.icons.get(&s.weapon) {
                let [_, _, iw, ih] = icon.rect.map(f32::from);
                let [x, y] = w.at.map(f32::from);
                out.push(quad(icon, [x, y, iw, ih], w.anchor, white, 3.0));
            }
        }
    }
    // Kills and rank (HUD flags 0x400, 0x800), or Total War's team strip (0x200 timed, 0x100
    // untimed: C `func_150918EC`, `func_151E89A0`), and the clock.
    match s.strip {
        Some(n) => out.extend(strip(hud, n, untimed, hud.strip.anchor)),
        None => {
            out.extend(text(hud, &s.kills.to_string(), None, &hud.score, 5.0));
            out.extend(text(
                hud,
                &format!("({})", s.rank),
                Some(suffix(s.rank)),
                &hud.rank.text,
                5.0,
            ));
        }
    }
    if let Some(secs) = s.secs {
        let clock = format!("{}:{:02}", secs / 60, secs % 60);
        out.extend(text(hud, &clock, None, &hud.clock, 5.0));
    }
    if let Some(n) = s.countdown {
        out.extend(text(hud, &n.to_string(), None, &hud.countdown, 5.0));
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
        out.push(solid(c.back, c.anchor, c.back_colour, 6.0));
        let half = c.bar_half[0] + c.bar_half[1] * zoom.clamp(0.0, 1.0);
        out.push(solid(
            [c.bar_x, cy - half, c.bar_width, 2.0 * half],
            c.anchor,
            c.colour,
            6.5,
        ));
        out.extend(quarters(
            &c.ring, c.centre, c.quadrant, c.anchor, c.colour, 6.5,
        ));
        let d = c.dot_size;
        for k in 0..c.dots {
            let a = core::f32::consts::TAU * f32::from(k) / f32::from(c.dots);
            let (x, y) = (cx + c.radius * a.sin(), cy - c.radius * a.cos());
            out.push(quad(
                &c.dot,
                [x - d / 2.0, y - d / 2.0, d, d],
                c.anchor,
                c.colour,
                6.5,
            ));
        }
    }
    // Dead: black but for a circle (absent past the iris's list), over everything.
    let i = &hud.iris;
    if let Some(&radius) = s.respawn_in.and_then(|n| i.radius.get(usize::from(n))) {
        let ([cx, cy], r, far) = (i.centre, f32::from(radius), 10_000.0);
        let z = 9.0;
        out.extend(quarters(&i.image, i.centre, r, i.anchor, i.colour, z));
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
pub fn build(app: &mut App, hud: Hud) {
    app.add_systems(
        Startup,
        move |assets: Res<AssetServer>,
              mut meshes: ResMut<Assets<Mesh>>,
              mut commands: Commands| {
            // The haze's texture wraps (G_TX_WRAP, mask 6); the others clamp.
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
                    let wrap = *name == hud.haze.image.texture;
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
    app.add_systems(Update, draw);
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

/// What the HUD shows of the local player (`own`), the others and the match.
#[allow(clippy::type_complexity)]
fn shown(
    hud: &Hud,
    own: (&Player, &Combat, &FollowCamera, &Anim),
    everyone: &Query<(&Player, &Combat, Has<Predicted>), Or<(With<Predicted>, With<Interpolated>)>>,
    m: &MatchView,
) -> Shown {
    let (p, c, camera, anim) = own;
    let clock = &anim.0;
    let in_hand = [GRENADE_PULL, GRENADE_HOLD].contains(&clock.id)
        || (clock.id == GRENADE_THROW && clock.time < THROW_IN_HAND);
    let (eye, at) = camera.drawn.view(&camera.follow);
    let ahead = Vec2::new(at.x - eye.x, at.z - eye.z).normalize_or(Vec2::NEG_Y);
    let right = Vec2::new(-ahead.y, ahead.x);
    let level = |y: f32| hud.radar.levels.iter().filter(|&&l| l <= y).count();
    // Total War colours a blip by its team (C `func_15093B58`: the name colours by obj+0x328).
    let r = &hud.radar;
    let colour = |o: &Combat| {
        let team = r.team_colours.get(usize::from(o.team));
        (team.filter(|_| m.total_war).copied()).unwrap_or(r.blip_colour)
    };
    let blips = (everyone.iter())
        .filter(|(_, o, predicted)| !predicted && o.health > 0)
        .filter(|(_, o, _)| !hud.radar.hidden_with.contains(&o.weapon))
        .map(|(o, oc, _)| {
            let d = Vec2::new(o.pos[0] - p.pos[0], o.pos[2] - p.pos[2]);
            let higher = level(o.pos[1]).cmp(&level(p.pos[1])) as i8;
            ([d.dot(right), d.dot(ahead)], higher, colour(oc))
        })
        .collect();
    let scope = camera.follow.cam.scope;
    // The place among everyone, this player first; Total War's strip: each team's kills, untimed
    // its lives left (`func_151E9D18`).
    let others = (everyone.iter()).filter(|o| !o.2).map(|o| o.1);
    let combats: Vec<_> = std::iter::once(c).chain(others).collect();
    let all: Vec<_> = combats.iter().map(|o| Some(standing(o))).collect();
    let mut strip = [0; 2];
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
        weapon: c.weapon,
        grenades: in_hand.then_some(c.grenades),
        kills: c.kills,
        rank: usize::from(places(&all, m.kill_limit)[0]),
        secs: (m.time > 0).then_some(m.timer.max(0) as u32 / 60),
        radar: m.radar,
        blips,
        reticle: p.aiming && hud.reticle.weapons.contains(&c.weapon),
        scope: scope
            .on
            .then(|| scope.zoom[0] / conker_camera::zoom_limit(c.weapon)),
        respawn_in: (c.health == 0 && c.respawn_in > 0).then_some(c.respawn_in),
        lives: (m.time == 0).then_some(c.lives_left),
        out: c.out > 0,
        strip: m.total_war.then_some(strip),
        countdown: countdown(m.countdown),
        haze: (c.haze > 0).then_some(u32::from(c.haze)),
        results: None,
        header: None,
    }
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
    (playing, outros): (Res<crate::outro::Playing>, Res<crate::outro::Outros>),
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
    let quads = match playing.running(&outros) {
        Some((o, k)) => outro(&a.hud, o, o.t(k)),
        None => {
            let mut shown = shown(&a.hud, own, &everyone, m);
            if m.phase == 2 {
                let all: Vec<_> = (roster.iter())
                    .map(|(id, c)| (!matches!(id.0, PeerId::Local(_)), c))
                    .collect();
                let teams = crate::outro::team_mode(m);
                shown.results = Some((rows(&all, m.kill_limit, teams), menu.choice));
                shown.header = teams.then(|| header(&all, own.1.team));
            }
            sprites(&a.hud, &shown)
        }
    };
    for s in quads {
        let key = (s.tex.clone(), s.uv.map(f32::to_bits), s.colour);
        let material = cache.entry(key).or_insert_with(|| {
            let (texture, size) = match &s.tex {
                Tex::Texture(name) => a.textures[name].clone(),
                Tex::Glyph(g) => (a.glyphs[*g].clone(), a.hud.font[*g].size.map(u16::from)),
                Tex::Solid => (Handle::default(), [1, 1]),
            };
            let texture = (s.tex != Tex::Solid).then_some(texture);
            let size = Vec2::from(size.map(f32::from));
            let [x0, y0, x1, y1] = s.uv;
            let [r, g, b, alpha] = s.colour;
            materials.add(StandardMaterial {
                base_color: Color::srgba_u8(r, g, b, alpha),
                base_color_texture: texture,
                uv_transform: Affine2::from_scale_angle_translation(
                    Vec2::new(x1 - x0, y1 - y0) / size,
                    0.0,
                    Vec2::new(x0, y0) / size,
                ),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
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
                Tex::Solid => false,
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
            rows(&[(false, &bot), (true, &p2), (true, &p1)], 0, false),
            [(0, [2, 9, 2, 0, 1]), (1, [3, 0, 0, 0, 1])]
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
        // Tediz 2 kills (the bot's), squirrels 1: the Tediz win.
        let (tedi, squirrel, bot) = (c(0, 1, 0, 3), c(1, 0, 1, 1), c(2, 1, 2, 5));
        let all = [(true, &tedi), (true, &squirrel), (false, &bot)];
        assert_eq!(
            rows(&all, 0, true),
            [(1, [1, 0, 0, 0, 0]), (0, [2, 0, 1, 0, 0])]
        );
        assert_eq!(header(&all, 0), Header::Team(1));
        let alone = [(false, &tedi), (true, &squirrel), (false, &bot)];
        assert_eq!(
            (header(&alone, 0), header(&alone, 1)),
            (Header::You(false), Header::You(true))
        );
        // Equal kills and shots: a DRAW (V `to_draw`).
        let (a, b) = (c(0, 0, 0, 0), c(1, 1, 0, 0));
        assert_eq!(header(&[(true, &a), (true, &b)], 0), Header::Draw);
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
            out: true,
            ..state()
        };
        assert!(rects(&hud, &out, &texture("radar")).is_empty());
        assert_eq!(
            rects(&hud, &out, &texture("skull"))[0].0,
            [130.0, 92.0, 32.0, 32.0]
        );
        let over = Shown {
            results: Some((vec![(0, [1, 9, 2, 0, 1]), (1, [3, 0, 0, 0, 1])], 1)),
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
            results: Some((vec![(1, [2, 0, 0, 0, 0]), (0, [1, 50, 1, 0, 0])], 0)),
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

    /// The outros' pages against the original's draws (one capture's I8 glyph rects): every glyph
    /// where the original drew it, but on cs4's first page, whose last two lines the original draws
    /// a pixel higher (open). Total War: `lab/research/mp-original/total-war-outro-scratch/hw/
    /// cs*b_dl_*`, one capture a page. Colors' cs1: `colors-outro-scratch/hw/cs1_dl_N` (t = 2 (N −
    /// 436)), each of the bubble's places, its text settled (around a move it fades out and in).
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
        // (outro, t, capture)
        let mut captures = vec![];
        for (o, reads) in war.iter().zip([[482, 560, 650, 760], [470, 600, 700, 800]]) {
            let pages = &o.subtitles.as_ref().unwrap().pages;
            for (page, read) in pages.iter().zip(reads) {
                let file = format!("total-war-outro-scratch/hw/cs{}b_dl_{read}", o.cutscene);
                captures.push((o, f32::from(page.t), file));
            }
        }
        for n in [461, 536, 636, 666, 756] {
            let file = format!("colors-outro-scratch/hw/cs1_dl_{n}");
            captures.push((&colors[0], (2 * (n - 436)) as f32, file));
        }
        for (o, t, file) in captures {
            let Ok(draws) = std::fs::read(format!("{research}/{file}/draws.json")) else {
                eprintln!("no {file}: skipping");
                return;
            };
            let draws: Vec<serde_json::Value> = serde_json::from_slice(&draws).unwrap();
            let first = o.page(t) == o.subtitles.as_ref().and_then(|s| s.pages.first());
            let want = (draws.iter()).filter(|d| d["format"] == "I8").map(|d| {
                let r: [f32; 4] = serde_json::from_value(d["rect"].clone()).unwrap();
                match (o.cutscene, first, r[1] > 25.0) {
                    (4, true, true) => [r[0], r[1] + 1.0, r[2], r[3] + 1.0],
                    _ => r,
                }
            });
            let got = (outro(&hud, o, t).into_iter())
                .filter(|q| matches!(q.tex, Tex::Glyph(_)) && q.colour == [0, 0, 0, 255])
                .map(|q| {
                    [
                        q.rect[0],
                        q.rect[1],
                        q.rect[0] + q.rect[2],
                        q.rect[1] + q.rect[3],
                    ]
                });
            let page = format!("cutscene {} t {t} ({file})", o.cutscene);
            assert_eq!(sorted(got.collect()), sorted(want.collect()), "{page}");
        }
    }
}
