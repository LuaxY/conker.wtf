//! The HUD the original draws over a Deathmatch player's view, from the pack's `hud/` ([`Hud`]:
//! pictures, layout and rules found in the oracle, `lab/hud`): health as chocolate, the held
//! weapon's icon, grenades while one is in the hand, kills and rank, the clock, the radar, the aim
//! reticle, the scope and the death iris.
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
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::batching::NoAutomaticBatching;
use conker_anim::combat::{GRENADE_HOLD, GRENADE_PULL, GRENADE_THROW};
use conker_assets::hud::{Align, Anchor, Horizontal, Hud, Image as Picture, Rgba, Text, Vertical};
use conker_sim::net::{Combat, MatchView, Player};
use lightyear::prelude::{Interpolated, Predicted};

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
    /// Seconds left on the clock.
    pub secs: u32,
    /// Other players for the radar: (right, ahead) of the player in game units, along the view,
    /// and their level against the player's (-1 lower, 0, 1 higher).
    pub blips: Vec<([f32; 2], i8)>,
    /// The reticle is up (R's aim with a weapon of [`conker_assets::hud::Reticle::weapons`]).
    pub reticle: bool,
    /// Looking through a scope: its zoom, 0 opened to 1 at the weapon's closest.
    pub scope: Option<f32>,
    /// Dead: frames to the respawn.
    pub respawn_in: Option<u16>,
}

fn image(p: &Picture) -> (Tex, [f32; 4]) {
    let [x, y, w, h] = p.rect.map(f32::from);
    (Tex::Texture(p.texture.clone()), [x, y, x + w, y + h])
}

/// A line of text: (the glyph index, or a rank suffix) per character.
enum Item {
    Glyph(usize),
    Suffix(usize),
}

fn text(hud: &Hud, line: &str, suffix: Option<usize>, t: &Text, z: f32) -> Vec<Sprite> {
    let glyph = |c: char| hud.font.iter().position(|g| g.ch == c);
    let mut items: Vec<Item> = line.chars().filter_map(glyph).map(Item::Glyph).collect();
    if let Some(s) = suffix {
        // The rank: "(1" suffix ")".
        let close = items.pop();
        items.push(Item::Suffix(s));
        items.extend(close);
    }
    let rank = &hud.rank;
    let advance = |i: &Item| match i {
        Item::Glyph(g) => f32::from(hud.font[*g].size[0]) - 1.0,
        Item::Suffix(_) => f32::from(rank.advance),
    };
    let width: f32 = (items.iter())
        .map(|i| match i {
            Item::Suffix(_) => f32::from(rank.width),
            _ => advance(i),
        })
        .sum();
    let [x, top] = t.at.map(f32::from);
    let mut pen = match t.align {
        Align::Left => x,
        Align::Centre => x - (width / 2.0).floor(),
    };
    let mut out = Vec::new();
    for item in &items {
        let (tex, uv, rect) = match item {
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
        pen += advance(item);
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

/// The HUD's quads for `s`, in the frame (the original's draws: `lab/hud/README.md`).
pub fn sprites(hud: &Hud, s: &Shown) -> Vec<Sprite> {
    let mut out = Vec::new();
    let quad = |p: &Picture, rect: [f32; 4], anchor, colour, z| {
        let (tex, uv) = image(p);
        Sprite {
            tex,
            uv,
            rect,
            anchor,
            colour,
            z,
        }
    };
    let white = [255; 4];
    // Radar: the disc, then a blip per player in range, with a bar on another level's.
    let r = &hud.radar;
    let [cx, cy] = r.centre;
    let half = r.size / 2.0;
    out.push(quad(
        &r.disc,
        [cx - half, cy - half, r.size, r.size],
        r.anchor,
        r.colour,
        1.0,
    ));
    for &([right, ahead], level) in &s.blips {
        // The game adds the offset in quarter pixels, truncated (C `func_15093B58`).
        let q = |v: f32| (v * r.scale * 4.0).trunc() / 4.0;
        let (x, y) = (q(right), -q(ahead));
        if x * x + y * y >= r.range * r.range {
            continue;
        }
        let b = r.blip;
        let (bx, by) = (cx + x - b / 2.0, cy + y - b / 2.0);
        out.push(solid([bx, by, b, b], r.anchor, r.blip_colour, 2.0));
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
    // Kills, rank and clock.
    out.extend(text(hud, &s.kills.to_string(), None, &hud.score, 5.0));
    // st, nd, rd, th: 1st, 2nd, 3rd, 4th…, 11th-13th.
    let suffix = match (s.rank % 10, s.rank % 100 / 10) {
        (d @ 1..=3, tens) if tens != 1 => d - 1,
        _ => 3,
    };
    out.extend(text(
        hud,
        &format!("({})", s.rank),
        Some(suffix),
        &hud.rank.text,
        5.0,
    ));
    let clock = format!("{}:{:02}", s.secs / 60, s.secs % 60);
    out.extend(text(hud, &clock, None, &hud.clock, 5.0));
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
            let load = |file: &str, smooth: bool| -> Handle<Image> {
                let sampler = if smooth {
                    ImageSampler::linear()
                } else {
                    ImageSampler::nearest()
                };
                let settings = move |s: &mut ImageLoaderSettings| s.sampler = sampler.clone();
                assets
                    .load_builder()
                    .with_settings(settings)
                    .load(format!("hud/{file}"))
            };
            let textures = (hud.textures.iter())
                .map(|(name, t)| (name.clone(), (load(&t.file, t.smooth), t.size)))
                .collect();
            let glyphs = hud.font.iter().map(|g| load(&g.file, false)).collect();
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
    let blips = (everyone.iter())
        .filter(|(_, o, predicted)| !predicted && o.health > 0)
        .filter(|(_, o, _)| !hud.radar.hidden_with.contains(&o.weapon))
        .map(|(o, _, _)| {
            let d = Vec2::new(o.pos[0] - p.pos[0], o.pos[2] - p.pos[2]);
            let higher = level(o.pos[1]).cmp(&level(p.pos[1])) as i8;
            ([d.dot(right), d.dot(ahead)], higher)
        })
        .collect();
    let scope = camera.follow.cam.scope;
    Shown {
        health: c.health,
        weapon: c.weapon,
        grenades: in_hand.then_some(c.grenades),
        kills: c.kills,
        rank: 1 + everyone
            .iter()
            .filter(|(_, o, _)| o.kills > c.kills)
            .count(),
        secs: m.timer.max(0) as u32 / 60,
        blips,
        reticle: p.aiming && hud.reticle.weapons.contains(&c.weapon),
        scope: scope
            .on
            .then(|| scope.zoom[0] / conker_camera::zoom_limit(c.weapon)),
        respawn_in: (c.health == 0 && c.respawn_in > 0).then_some(c.respawn_in),
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
    let shown = shown(&a.hud, own, &everyone, m);
    for s in sprites(&a.hud, &shown) {
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

    /// The pack's HUD (env `CONKER_ASSETS`, else `<repo>/assets`); None (the test skips) without
    /// one.
    fn pack_hud() -> Option<Hud> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let dir = std::path::PathBuf::from(dir);
        let hud = Hud::load(|p| std::fs::read(dir.join(p))).ok();
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
            secs: 9 * 60 + 48,
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
            secs: 9 * 60 + 52,
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
                blips: vec![([d.dot(right), d.dot(ahead)], 0)],
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
}
