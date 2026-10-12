//! `maps/<map>/outros.json`: War: Total War's victory outros on its map (`2d`: scene 0x2D's
//! cutscenes 2 and 4, assets `[6, 0x2D, n]`, loader `func_1501D348`; `../lab/research/mp-original/
//! total-war-outro.md`), baked with only what those two use; no other map has one.
//!
//! Times `t` are the cutscene clock's VIs (`D_800C35B0`, 2 a game frame): frame `k` of an outro (0 its
//! first, the remake's first tick of `Phase::Over`) shows t = min(2k, length); a command at t acts on
//! the first frame with t ≤ 2k (V). The STATS come up at frame [`Outro::frames`], over its last
//! picture, the actor's clip still looping.

use serde::{Deserialize, Serialize};

use crate::hud::{Anchor, Rgba};

/// One outro: which win plays it and what it shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outro {
    /// The winning team that plays it (0 the squirrels, 1 the Tediz); a DRAW plays none.
    pub winner: u8,
    /// Its cutscene: `[6, scene, cutscene]`.
    pub cutscene: u8,
    /// VIs (the script's 0xFF record).
    pub length: u16,
    pub camera: Camera,
    pub actor: Actor,
    pub fade: Fade,
    pub voice: Voice,
    pub subtitles: Subtitles,
}

/// The camera (object kind 1, look mode 0): its eye on the path's two points (channel 10, 0-100 %,
/// moves it along: a two-point path is a straight line, V), looking 400 units along yaw channel 7
/// + 90° and pitch channel 8 (degrees); a 50° vertical FOV (ch 9 unused, V).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub path: [[i16; 3]; 2],
    pub along: Vec<Key>,
    pub yaw: Vec<Key>,
    pub pitch: Vec<Key>,
}

/// A channel key (8 bytes: `f32 value, u8 interp, u8 channel, u16 t`): Catmull-Rom from it to the
/// next key (interp 0, `func_150497E0`; a missing neighbour is the end key again), else linear. The
/// value holds before the first key and after the last.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub t: u16,
    pub value: f32,
    pub catmull: bool,
}

/// The one character (object kind 2; its uid's model from bank 0x0E's scene records): where it
/// stands and faces, drawn `scale` big and `lift` model units up (bank 0x11, as a player), and the
/// clip it loops from t 0 (op 2; `rate` = 2 × `ticks` / VIs to its next op 2, V).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    /// `characters/<character>/`: its glTF holds this one clip, `animation` by name.
    pub character: String,
    pub animation: String,
    /// The clip's loop length, 30 Hz ticks.
    pub ticks: u16,
    pub rate: f32,
    pub at: [i16; 3],
    /// u16 game angle (channel 7 + 90°).
    pub facing: u16,
    pub lift: i16,
    pub scale: f32,
}

/// The fade in from `rgb` (op 5 b=1, `func_1517EE40`): opaque at t 0, clear `duration` VIs on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fade {
    pub rgb: [u8; 3],
    pub duration: u16,
}

/// The voice line (op 0x0B: bank 0x16's MP3 `entry`), as a WAV under `audio/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    pub t: u16,
    pub entry: u16,
    pub file: String,
}

/// The speech bubble's pages (op 7): each from its `t`, the last up to `close`. Lines are drawn
/// upper case in the HUD font, black, left-aligned in a block whose left is `text[0]` − ⌈widest /
/// 2⌉, 12 pixels apart around the middle line's top `text[1]`, over the bubble's `quads`; all
/// kept at `anchor` in the HUD's frame (V: the DLs' draws; the carrier's channels 6-8 that place
/// them are not decoded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtitles {
    pub pages: Vec<Page>,
    pub close: u16,
    pub anchor: Anchor,
    pub text: [i16; 2],
    pub quads: Vec<Quad>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub t: u16,
    pub lines: Vec<String>,
}

/// A HUD texture's texels `uv` (x0, y0, x1, y1; x1 < x0 mirrors) drawn at `rect` (x, y, width,
/// height) in `colour`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quad {
    pub texture: String,
    pub rect: [i16; 4],
    pub uv: [f32; 4],
    pub colour: Rgba,
}

/// Channel `keys` at `t`.
pub fn value(keys: &[Key], t: f32) -> f32 {
    let Some(next) = keys.iter().position(|k| f32::from(k.t) > t) else {
        return keys.last().map_or(0.0, |k| k.value);
    };
    if next == 0 {
        return keys[0].value;
    }
    let (a, b) = (keys[next - 1], keys[next]);
    let s = (t - f32::from(a.t)) / f32::from(b.t - a.t);
    if !a.catmull {
        return a.value + (b.value - a.value) * s;
    }
    let p0 = keys[next.saturating_sub(2)].value;
    let p3 = keys.get(next + 1).unwrap_or(&b).value;
    let (p1, p2) = (a.value, b.value);
    // `func_150497E0`'s polynomial.
    let c3 = 0.5 * p3 - 0.5 * p0 + 1.5 * p1 - 1.5 * p2;
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c1 = 0.5 * p2 - 0.5 * p0;
    ((c3 * s + c2) * s + c1) * s + p1
}

impl Outro {
    /// Frames it lasts: the STATS show from this frame.
    pub fn frames(&self) -> u32 {
        u32::from(self.length).div_ceil(2)
    }

    /// The clock at frame `k`.
    pub fn t(&self, k: u32) -> f32 {
        (2 * k).min(u32::from(self.length)) as f32
    }

    /// The camera's eye and look-at at `t` (game units).
    pub fn camera(&self, t: f32) -> ([f32; 3], [f32; 3]) {
        let c = &self.camera;
        let s = value(&c.along, t) / 100.0;
        let [a, b] = c.path.map(|p| p.map(f32::from));
        let eye: [f32; 3] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * s);
        let (yaw, pitch) = (
            (value(&c.yaw, t) + 90.0).to_radians(),
            value(&c.pitch, t).to_radians(),
        );
        let at = [
            eye[0] + 400.0 * pitch.cos() * yaw.cos(),
            eye[1] + 400.0 * pitch.sin(),
            eye[2] - 400.0 * pitch.cos() * yaw.sin(),
        ];
        (eye, at)
    }

    /// The actor's clip time at frame `k`, in clip ticks (one loop is `ticks`).
    pub fn clip_time(&self, k: u32) -> f32 {
        (k as f32 * self.actor.rate) % f32::from(self.actor.ticks)
    }

    /// The fade's alpha at `t`.
    pub fn fade(&self, t: f32) -> u8 {
        let left = 1.0 - t / f32::from(self.fade.duration);
        (255.0 * left.max(0.0)).round() as u8
    }

    /// The page shown at `t`, if any.
    pub fn page(&self, t: f32) -> Option<&Page> {
        let s = &self.subtitles;
        let shown = |p: &&Page| f32::from(p.t) <= t && t < f32::from(s.close);
        s.pages.iter().rev().find(shown)
    }

    /// The frame the voice starts on.
    pub fn voice_frame(&self) -> u32 {
        u32::from(self.voice.t).div_ceil(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: u16, value: f32, catmull: bool) -> Key {
        Key { t, value, catmull }
    }

    #[test]
    fn keys_hold_ease_and_interpolate() {
        let linear = [key(0, -30.0, false), key(650, 7.0, false)];
        assert_eq!(value(&linear, 0.0), -30.0);
        assert!((value(&linear, 2.0) + 30.0 - 37.0 * 2.0 / 650.0).abs() < 1e-4);
        assert_eq!(value(&linear, 700.0), 7.0);
        // Two Catmull-Rom keys ease: a + (b − a)(s/2 + 3s²/2 − s³) (V cs4: 154.0554° at t 2).
        let eased = [key(0, 64.0, true), key(800, 108.0, true)];
        assert!((value(&eased, 2.0) + 90.0 - 154.0554).abs() < 1e-3);
        assert!((value(&eased, 400.0) - 86.0).abs() < 1e-4);
        assert_eq!(value(&[key(1, 12.0, false)], 0.0), 12.0);
        assert_eq!(value(&[], 3.0), 0.0);
    }
}
