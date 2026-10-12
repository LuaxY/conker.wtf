//! The pack's HUD: `hud/hud.json` and its PNGs, what the original draws over a Deathmatch player's
//! view (C `func_150918EC`, `func_15093B58`, `func_150417AC`; V the oracle's RAM dumps,
//! `lab/hud`).
//!
//! Coordinates are the original's frame, [`Hud::frame`] (292 x 216: the framebuffer it draws in;
//! the VI shows it at [`Tv::offset`] of a 320 x 240 TV picture). Each element keeps its distance
//! from the frame edge or centre named by its [`Anchor`], as the original places it off its
//! viewport's edges and centre, so a wider or narrower window moves it with that edge. Rectangles
//! are `[x, y, width, height]`; colours `[r, g, b, a]`; an [`Image`] is a texel rectangle of a
//! texture in [`Hud::textures`].

use std::collections::BTreeMap;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::audio::safe;
use crate::{Error, Result, invalid, parse_json, to_json};

pub const HUD: &str = "hud/hud.json";
pub const END: &str = "hud/end.json";

pub type Rgba = [u8; 4];

/// The frame edges an element keeps its distance from: x (left, the centre or right), then y
/// (top, the middle or bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor(pub Horizontal, pub Vertical);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Horizontal {
    Left,
    Centre,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vertical {
    Top,
    Middle,
    Bottom,
}

/// A PNG under `hud/`, its size in texels, and whether it is drawn smoothed (scaled in the
/// original: bilinear) or with sharp texels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Texture {
    pub file: String,
    pub size: [u16; 2],
    pub smooth: bool,
}

/// A texel rectangle `[x, y, width, height]` of a texture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub texture: String,
    pub rect: [u16; 4],
}

/// Where the VI shows the frame on a TV (for reference: not read).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tv {
    pub size: [u16; 2],
    pub offset: [u16; 2],
}

/// A character of the font: its PNG (white, the shape in its alpha), drawn `offset` from the pen
/// (x) and the line's top (y); the pen then moves `size[0] - 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Glyph {
    #[serde(rename = "char")]
    pub ch: char,
    pub file: String,
    pub size: [u8; 2],
    pub offset: [i8; 2],
}

/// Health: one piece per point, ceil(health / 2) on the first row and the rest on the second,
/// right-aligned at `right`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub anchor: Anchor,
    pub right: i16,
    pub rows: [i16; 2],
    pub piece: Image,
}

/// The held weapon's icon (drawn or put away), by weapon id in decimal; none while a grenade is
/// in the hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weapon {
    pub anchor: Anchor,
    pub at: [i16; 2],
    pub icons: BTreeMap<u8, Image>,
}

/// While a grenade is in the hand: one icon per grenade left, from `at`, `step` apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grenades {
    pub anchor: Anchor,
    pub at: [i16; 2],
    pub step: i16,
    pub icon: Image,
}

/// A line of text: centred on `at[0]` (`align` "centre") or starting there ("left"), its top at
/// `at[1]`, with an optional shadow drawn first, one texel right and down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub anchor: Anchor,
    pub at: [i16; 2],
    pub align: Align,
    pub colour: Rgba,
    pub shadow: Option<Rgba>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Centre,
}

/// The rank, `(1st)`: [`Text`], with the suffix icons (st, nd, rd, th: textures drawn in the
/// text's colour) placed `offset` from the pen, showing `texels` of the texture, moving the pen
/// `advance` and counting `width` when the line is centred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rank {
    pub text: Text,
    pub suffixes: [String; 4],
    pub offset: [i16; 2],
    pub texels: [u16; 4],
    pub advance: i16,
    pub width: i16,
}

/// The radar: a disc of `size` around `centre` turned with the view (up is where the camera looks),
/// and a `blip`-sized square for every other living player within `range` frame pixels, `scale`
/// pixels per game unit away. A blip on another level (`levels`: the heights between them) gets a
/// `level_mark` bar above it (higher) or below it (lower). Holding a weapon in `hidden_with`
/// keeps a player off it. In War (Total War, Colors) a blip is in its player's team's
/// `team_colours[team]` ([0] the squirrels', [1] the Tediz') instead of `blip_colour`; War:
/// Colors' flags not carried get a blip too, in `flag_colour`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Radar {
    pub anchor: Anchor,
    pub centre: [f32; 2],
    pub size: f32,
    pub disc: Image,
    pub colour: Rgba,
    pub scale: f32,
    pub range: f32,
    pub blip: f32,
    pub blip_colour: Rgba,
    pub levels: Vec<f32>,
    pub level_mark: Rgba,
    pub hidden_with: Vec<u8>,
    pub team_colours: [Rgba; 2],
    pub flag_colour: Rgba,
}

/// War: Total War's team strip, in place of the own kills and rank (the results draw it too): per
/// team, [0] the squirrels and [1] the Tediz, an icon over `rects[team]`, `kills[team]` in a match
/// on the clock or `lives[team]` untimed, and the team's number as `numbers[team]`: its members'
/// kills, untimed their lives left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Strip {
    pub anchor: Anchor,
    pub rects: [[i16; 4]; 2],
    pub kills: [Image; 2],
    pub lives: [Image; 2],
    pub numbers: [Text; 2],
}

/// While the aim camera is on with one of `weapons`: `image` over `rect`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reticle {
    pub anchor: Anchor,
    pub rect: [i16; 4],
    pub image: Image,
    pub colour: Rgba,
    pub weapons: Vec<u8>,
}

/// While looking through a scope: the ring, four mirrored quadrants of `ring` (its centre at the
/// image's bottom right) `quadrant` pixels each around `centre`; `dots` of `dot` at `radius`; and
/// the zoom bar: `back` behind a fill `bar_width` wide from `bar_x`, centred on `centre[1]`, half
/// as tall as `bar_half[0]` plus `bar_half[1]` times the zoom (0 unzoomed, 1 at the weapon's
/// closest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub anchor: Anchor,
    pub centre: [f32; 2],
    pub ring: Image,
    pub quadrant: f32,
    pub dot: Image,
    pub dots: u8,
    pub dot_size: f32,
    pub radius: f32,
    pub colour: Rgba,
    pub back: [f32; 4],
    pub back_colour: Rgba,
    pub bar_x: f32,
    pub bar_width: f32,
    pub bar_half: [f32; 2],
}

/// Dead: the screen goes black but for a circle around `centre`, `radius[n]` pixels wide `n`
/// frames before the respawn (absent past the list's end), drawn as four mirrored quadrants of
/// `image` (opaque outside its circle, its centre at the bottom right) and black around them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Iris {
    pub anchor: Anchor,
    pub centre: [f32; 2],
    pub image: Image,
    pub colour: Rgba,
    pub radius: Vec<u16>,
}

/// War: Total War's gassed screen (`obj(0xE013, p)`, drawn by `func_151D2830`): over the player's
/// whole view and under the rest of the HUD, one quad per layer of `image` (repeating), its colour
/// times the texture's, its alpha the screen's strength times `share`; `texel` texels a frame
/// pixel, texel (k / 4, `t`) at the frame's top left on the screen's k-th frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Haze {
    pub image: Image,
    pub layers: [HazeLayer; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HazeLayer {
    pub colour: [u8; 3],
    pub share: f32,
    pub texel: f32,
    pub t: f32,
}

/// `hud/hud.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hud {
    pub frame: [u16; 2],
    pub tv: Tv,
    pub textures: BTreeMap<String, Texture>,
    pub font: Vec<Glyph>,
    pub health: Health,
    pub weapon: Weapon,
    pub grenades: Grenades,
    pub score: Text,
    pub rank: Rank,
    pub clock: Text,
    pub radar: Radar,
    pub reticle: Reticle,
    pub scope: Scope,
    pub iris: Iris,
    pub strip: Strip,
    /// War: Colors' carried-flag icons, by flag ([0] the red, which the squirrels carry; [1] the
    /// green, the Tediz'): shown while it is carried, blinking.
    pub flags: [Placed; 2],
    /// War: Total War's gas countdown: the seconds left until the gas kills.
    pub countdown: Text,
    pub haze: Haze,
    /// `hud/end.json` ([`End::load`]; not part of `hud.json`).
    #[serde(skip)]
    pub end: Option<End>,
}

impl Hud {
    /// Every image of the HUD, with where it is named (for checks).
    fn images(&self) -> Vec<(String, &Image)> {
        let mut all = vec![
            ("health.piece".into(), &self.health.piece),
            ("grenades.icon".into(), &self.grenades.icon),
            ("radar.disc".into(), &self.radar.disc),
            ("reticle.image".into(), &self.reticle.image),
            ("scope.ring".into(), &self.scope.ring),
            ("scope.dot".into(), &self.scope.dot),
            ("iris.image".into(), &self.iris.image),
            ("haze.image".into(), &self.haze.image),
        ];
        all.extend((self.weapon.icons.iter()).map(|(id, i)| (format!("weapon.icons.{id}"), i)));
        for (name, icons) in [("kills", &self.strip.kills), ("lives", &self.strip.lives)] {
            all.extend((icons.iter().enumerate()).map(|(n, i)| (format!("strip.{name}[{n}]"), i)));
        }
        all.extend((self.flags.iter().enumerate()).map(|(n, f)| (format!("flags[{n}]"), &f.image)));
        all
    }

    /// Reads `hud/hud.json` and checks that every image names a texture of the file and fits in
    /// it, and that every file stays inside `hud/`.
    pub fn load<E: Display>(read: impl FnOnce(&str) -> Result<Vec<u8>, E>) -> Result<Self> {
        let bytes = read(HUD).map_err(|e| Error::Read {
            file: HUD.into(),
            message: format!("{e} (a pack exported before the HUD: export a new one)"),
        })?;
        let hud: Hud = parse_json(HUD, &bytes)?;
        let glyphs = (hud.font.iter().enumerate()).map(|(i, g)| (format!("font[{i}]"), &g.file));
        if let Some((field, _)) = glyphs.into_iter().find(|(_, f)| !safe(f)) {
            return Err(invalid(HUD, field, "a path inside hud/"));
        }
        let suffixes = (hud.rank.suffixes.iter()).map(|s| ("rank.suffixes".to_string(), s));
        if let Some((field, name)) = suffixes
            .into_iter()
            .find(|(_, n)| !hud.textures.contains_key(*n))
        {
            return Err(invalid(HUD, field, format!("no texture {name:?}")));
        }
        check(HUD, &hud.textures, hud.images())?;
        Ok(hud)
    }

    pub fn to_json(&self) -> Vec<u8> {
        to_json(self, 2)
    }

    /// The glyph of `c`, if the font has one.
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.font.iter().find(|g| g.ch == c)
    }
}

/// Checks that every texture's file stays inside `hud/` and every image names one of `textures`
/// and fits in it.
fn check(
    json: &str,
    textures: &BTreeMap<String, Texture>,
    images: Vec<(String, &Image)>,
) -> Result<()> {
    if let Some(name) = textures.iter().find(|(_, t)| !safe(&t.file)).map(|t| t.0) {
        return Err(invalid(
            json,
            format!("textures.{name}"),
            "a path inside hud/",
        ));
    }
    for (field, image) in images {
        let Some(t) = textures.get(&image.texture) else {
            return Err(invalid(
                json,
                field,
                format!("no texture {:?}", image.texture),
            ));
        };
        let [x, y, w, h] = image.rect.map(u32::from);
        if x + w > u32::from(t.size[0]) || y + h > u32::from(t.size[1]) {
            let why = format!("{:?} outside {}'s {:?}", image.rect, image.texture, t.size);
            return Err(invalid(json, format!("{field}.rect"), why));
        }
    }
    Ok(())
}

/// `image` over `rect`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    pub anchor: Anchor,
    pub rect: [i16; 4],
    pub image: Image,
}

/// A choice of the results' menu: `image`, or `lit` while it is the one chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub anchor: Anchor,
    pub rect: [i16; 4],
    pub image: Image,
    pub lit: Image,
}

/// Untimed (lives count): the lives left, the digit `digits[n]` at `at`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lives {
    pub anchor: Anchor,
    pub at: [i16; 2],
    pub digits: Vec<Image>,
}

/// The results (STATS): the `header` and the columns' `icons`; then a row per human player, the
/// first's text top at `rows[0]`, each next `rows[1]` lower: its `labels[n]` (P1, P2...) with its
/// top left at `label` (the first row's), then its place (the rank's suffixes as in `(1st)`),
/// accuracy (hits × 100 / shots, %), kills, headshots and deaths centred on `columns`, in its
/// `colours[n]` over a `shadow` one pixel right and down; and the menu: `restart` and `quit`, the
/// chosen one lit, and the `stick` between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Results {
    pub anchor: Anchor,
    pub header: Placed,
    pub icons: Vec<Placed>,
    pub labels: Vec<Image>,
    pub label: [i16; 2],
    pub rows: [i16; 2],
    pub columns: [i16; 5],
    pub colours: Vec<Rgba>,
    pub shadow: Rgba,
    pub restart: Button,
    pub quit: Button,
    pub stick: Placed,
}

/// War: Total War's results: the columns' icons, the labels and the rows `down` lower than
/// [`Results`] puts them, each row in its team's colour (`results.colours[team]`), the team strip
/// ([`Strip`]) under them, and a header: `draw` when both teams are 1st; with one human, `won` or
/// `lost` (YOU WIN, YOU LOSE: its team's place); with more, `team_won[team]` (the winning team's
/// name and WIN; [0] the squirrels, [1] the Tediz).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Teams {
    pub down: i16,
    pub draw: Placed,
    pub won: [Placed; 2],
    pub lost: [Placed; 2],
    pub team_won: [[Placed; 2]; 2],
}

/// `hud/end.json`: what the end of a match draws, in [`Hud::frame`] like the HUD: the lives left
/// (untimed), the skull over the view of a player out of lives, and the results (Total War's
/// `teams`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct End {
    pub textures: BTreeMap<String, Texture>,
    pub lives: Lives,
    pub skull: Placed,
    pub results: Results,
    pub teams: Teams,
}

impl End {
    /// Every image, with where it is named (for checks).
    fn images(&self) -> Vec<(String, &Image)> {
        let r = &self.results;
        let mut all = vec![
            ("skull.image".into(), &self.skull.image),
            ("results.header.image".into(), &r.header.image),
            ("results.stick.image".into(), &r.stick.image),
        ];
        for (name, b) in [("restart", &r.restart), ("quit", &r.quit)] {
            all.push((format!("results.{name}.image"), &b.image));
            all.push((format!("results.{name}.lit"), &b.lit));
        }
        let digits = self.lives.digits.iter().enumerate();
        all.extend(digits.map(|(n, i)| (format!("lives.digits[{n}]"), i)));
        all.extend(
            (r.icons.iter().enumerate())
                .map(|(n, i)| (format!("results.icons[{n}].image"), &i.image)),
        );
        all.extend((r.labels.iter().enumerate()).map(|(n, i)| (format!("results.labels[{n}]"), i)));
        let t = &self.teams;
        let header = [("draw", &t.draw)].into_iter().chain(
            (t.won.iter().map(|p| ("won", p)))
                .chain(t.lost.iter().map(|p| ("lost", p)))
                .chain(t.team_won.iter().flatten().map(|p| ("team_won", p))),
        );
        all.extend(header.map(|(name, p)| (format!("teams.{name}.image"), &p.image)));
        all
    }

    /// Reads `hud/end.json` and checks it as [`Hud::load`] does `hud.json`.
    pub fn load<E: Display>(read: impl FnOnce(&str) -> Result<Vec<u8>, E>) -> Result<Self> {
        let bytes = read(END).map_err(|e| Error::Read {
            file: END.into(),
            message: format!("{e} (a pack exported before the results: export a new one)"),
        })?;
        let end: End = parse_json(END, &bytes)?;
        if end.lives.digits.len() < 10 {
            return Err(invalid(END, "lives.digits", "10 digits, 0 to 9"));
        }
        check(END, &end.textures, end.images())?;
        Ok(end)
    }

    pub fn to_json(&self) -> Vec<u8> {
        to_json(self, 2)
    }
}
