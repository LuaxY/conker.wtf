//! Each other human's pseudonym over its head ([`Pseudonym`]: bots have none, so a tag tells a
//! human from a bot), as quads for the HUD's renderer ([`Quads`], which [`crate::hud`] draws with
//! its own): the HUD's font at half size up close, shrinking with the distance as the bodies do,
//! white on its shadow. None far off, while the player is dead or out of lives, nor behind a wall
//! (a tag through one would show where the player hides).

use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::prelude::*;
use conker_assets::hud::{Align, Anchor, Horizontal, Hud, Rgba, Text, Vertical};
use conker_rules::characters::body;
use conker_rules::projectile::raycast;
use conker_sim::net::{Collision, Combat, MatchView, Player, Pseudonym};
use lightyear::prelude::Interpolated;

use crate::hud::{Sprite, frame_point, text};

/// This frame's tag quads (frame pixels, centred).
#[derive(Resource, Clone, Debug, Default)]
pub struct Quads(pub Vec<Sprite>);

/// The HUD's frame and font, which the tags are drawn in.
#[derive(Resource)]
pub struct Font(Hud);

/// The tags' size against the HUD's own text, up to [`NEAR`] from the eye (game units); further,
/// smaller by `NEAR / distance` (as the bodies shrink), and none past [`FAR`].
const SCALE: f32 = 0.5;
const NEAR: f32 = 400.0;
const FAR: f32 = 1600.0;
/// Game units from the top of the head (the hitbox's) to the tag's bottom.
const ABOVE: f32 = 12.0;
/// Under the rest of the HUD (its shadow 0.5 lower).
const Z: f32 = 0.6;
const WHITE: Rgba = [255, 255, 255, 255];
const SHADOW: Rgba = [0, 0, 0, 200];

/// This frame's [`Quads`], once the view's camera has moved and before the HUD draws them.
pub fn update(
    font: Res<Font>,
    world: Res<Collision>,
    game: Query<&MatchView>,
    players: Query<(&Player, &Combat, &Pseudonym), With<Interpolated>>,
    view: Single<(&Camera, &Transform), With<FreeCameraState>>,
    mut quads: ResMut<Quads>,
) {
    quads.0.clear();
    let (camera, transform) = *view;
    let live = game.iter().any(|m| m.phase == 0);
    let (Some(window), true) = (camera.logical_viewport_size(), live) else {
        return;
    };
    let hud = &font.0;
    let line = (hud.font.iter()).fold(0.0, |h, g| f32::max(h, f32::from(g.size[1])));
    let eye = transform.translation;
    let style = Text {
        anchor: Anchor(Horizontal::Centre, Vertical::Middle),
        at: [0, 0],
        align: Align::Centre,
        colour: WHITE,
        shadow: Some(SHADOW),
    };
    for (p, c, name) in &players {
        if c.health == 0 || c.out > 0 {
            continue;
        }
        let head = body(c.class).hitbox;
        let feet = crate::render::stands(Vec3::from(p.pos), p.sink);
        let top = feet + Vec3::Y * (head.centre + head.half + ABOVE);
        let (to, len) = ((top - eye).normalize_or_zero(), eye.distance(top));
        if len > FAR || raycast(&world.0, eye.to_array(), to.to_array(), len) < len {
            continue;
        }
        let Ok(at) = camera.world_to_viewport(&GlobalTransform::from(*transform), top) else {
            continue;
        };
        // Laid out at the origin, then scaled and moved to the point (its bottom there) in
        // fractional frame pixels: `Text::at`'s whole ones would step as the player moves.
        let [x, y] = frame_point(hud.frame, window, at);
        let k = SCALE * (NEAR / len).min(1.0);
        let glyphs = text(hud, &name.0.to_uppercase(), None, &style, Z);
        quads.0.extend(glyphs.into_iter().map(|mut s| {
            let [rx, ry, w, h] = s.rect;
            s.rect = [x + rx * k, y + (ry - line) * k, w * k, h * k];
            s
        }));
    }
}

/// The tags in the HUD `hud`'s frame and font (windowed clients).
pub fn build(app: &mut App, hud: Hud) {
    app.insert_resource(Font(hud));
    app.init_resource::<Quads>();
    app.add_systems(Update, update.after(crate::beach::camera));
}
