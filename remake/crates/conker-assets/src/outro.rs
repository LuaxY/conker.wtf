//! `maps/<map>/outros.json`: the War victory outros on a map, baked with only what they use (loader
//! `func_1501D348`, assets `[6, scene, n]`): War: Total War's on `2d` (scene 0x2D's cutscenes 2 and 4,
//! `../lab/research/mp-original/total-war-outro.md`) and War: Colors' on `30` (scene 0x30's 1 and 5,
//! `../lab/research/mp-original/colors-outro.md`); no other map has one.
//!
//! Times `t` are the cutscene clock's VIs (`D_800C35B0`, 2 a game frame): frame `k` of an outro (0 its
//! first, the remake's first tick of `Phase::Over`) shows t = min(2k, length); a command at t acts on
//! the first frame with t ≤ 2k, the scripts' ones from frame 1 (V). The STATS come up at frame
//! [`Outro::frames`], over its last picture, the actors' clips still looping.
//!
//! Total War's single camera and actor are the general shape's simplest case: what they don't use
//! (`cuts`, `cast`, an actor's `path`/`yaw`/`plays`, the bubble's `moves`) is left out of their JSON.

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
    /// The cameras it cuts to (op 3), in order, each from its `from`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cuts: Vec<Camera>,
    pub actor: Actor,
    /// Its other actors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<Actor>,
    pub fade: Fade,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<Voice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<Subtitles>,
}

/// A camera (object kind 1, look mode 0): its eye on its path ([`path_at`], channel 10 the share of
/// it; one point: there), looking 400 units along yaw channel 7 + 90° and pitch channel 8 (degrees);
/// a 50° vertical FOV (ch 9 unused, V).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub path: Vec<[i16; 3]>,
    pub along: Vec<Key>,
    pub yaw: Vec<Key>,
    pub pitch: Vec<Key>,
    /// The t it is cut to (0: the first camera).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub from: u16,
}

fn is_zero(t: &u16) -> bool {
    *t == 0
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

/// A character (object kind 2; its uid's model from bank 0x0E's scene records), drawn `scale` big
/// and `lift` model units up (bank 0x11, as a player). It stands `at`, facing `facing`, unless it
/// walks a `path` (channel 10 the share of it, `along`) or turns (`yaw`: channel 7, facing = ch7 +
/// 90°). It loops its first clip from frame 1 (op 2; `rate` = 2 × `ticks` / VIs to its next op 2,
/// V), or plays it `once` and holds its last key (tick `ticks` − 1), then each of `plays` from its
/// time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    /// `characters/<character>/`: its glTF holds the clips its outros play, `animation` by name.
    pub character: String,
    pub animation: String,
    /// The clip's loop length, 30 Hz ticks.
    pub ticks: u16,
    pub rate: f32,
    /// Its place and facing (a u16 game angle) at t 0.
    pub at: [i16; 3],
    pub facing: u16,
    pub lift: i16,
    pub scale: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub once: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<[i16; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub along: Vec<Key>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yaw: Vec<Key>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plays: Vec<Play>,
}

/// A later op 2: from `t` the actor loops `animation` (`ticks` long) at `rate`, or plays it `once`,
/// from its start unless it already plays it (`func_15029BB8`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Play {
    pub t: u16,
    pub animation: String,
    pub ticks: u16,
    pub rate: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub once: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
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
/// kept at `anchor` in the HUD's frame; from each of `moves`' t, at its place instead (V: the DLs'
/// draws; the carrier's channels 6-8 that place them are not decoded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtitles {
    pub pages: Vec<Page>,
    pub close: u16,
    pub anchor: Anchor,
    pub text: [i16; 2],
    pub quads: Vec<Quad>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<Place>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub t: u16,
    pub lines: Vec<String>,
}

/// Where the bubble and its text are from `t` ([`Subtitles`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub t: u16,
    pub anchor: Anchor,
    pub text: [i16; 2],
    pub quads: Vec<Quad>,
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
    catmull([p0, a.value, b.value, p3], s)
}

/// `func_150497E0`'s polynomial through p1 (s 0) and p2 (s 1).
fn catmull([p0, p1, p2, p3]: [f32; 4], s: f32) -> f32 {
    let c3 = 0.5 * p3 - 0.5 * p0 + 1.5 * p1 - 1.5 * p2;
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c1 = 0.5 * p2 - 0.5 * p0;
    ((c3 * s + c2) * s + c1) * s + p1
}

/// The u step of a path segment's arc-length samples (`D_800969E8`): 100 from 0.01, summed in f32.
const STEP: f32 = 0.01;

/// The point `percent` (0-100) of a path's length along it (`func_15020388` measures it,
/// `func_1501FC8C` walks it): segments through its points, Catmull-Rom (a missing neighbour is the
/// end point again) where the segment's first point's flag is 0 (all these outros' points),
/// measured by 100 chords; a point is placed by arc length between the chords' ends. One point:
/// there.
pub fn path_at(points: &[[i16; 3]], percent: f32) -> [f32; 3] {
    let p: Vec<[f32; 3]> = points.iter().map(|p| p.map(f32::from)).collect();
    let n = p.len();
    if n < 2 {
        return p.first().copied().unwrap_or_default();
    }
    let curve = |i: usize, u: f32| {
        let at = |j: usize| p[j.min(n - 1)];
        let c = [at(i.saturating_sub(1)), at(i), at(i + 1), at(i + 2)];
        std::array::from_fn(|a| catmull(c.map(|q| q[a]), u))
    };
    let dist = |a: [f32; 3], b: [f32; 3]| {
        let d: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
        let d2 = d[0] * d[0] + (d[1] * d[1] + d[2] * d[2]);
        if d2 != 0.0 { d2.sqrt() } else { 0.0 }
    };
    // Each segment's chords' running lengths from the path's start: [start, after chord 1, ...].
    let mut start = 0.0f32;
    let tables: Vec<Vec<f32>> = (0..n - 1)
        .map(|i| {
            let mut table = vec![start];
            let (mut u, mut last) = (STEP, p[i]);
            while u <= 1.0 {
                let here = curve(i, u);
                start += dist(last, here);
                table.push(start);
                (last, u) = (here, u + STEP);
            }
            table
        })
        .collect();
    let s = start * percent.clamp(0.0, 100.0) * 0.01;
    let i = (tables.iter())
        .position(|t| s <= *t.last().unwrap_or(&0.0))
        .unwrap_or(n - 2);
    let table = &tables[i];
    let j = (0..table.len() - 1)
        .find(|&j| table[j] <= s && s <= table[j + 1])
        .unwrap_or(table.len() - 2);
    let (a, b) = (curve(i, j as f32 * STEP), curve(i, (j + 1) as f32 * STEP));
    let span = table[j + 1] - table[j];
    let f = if span != 0.0 {
        (s - table[j]) / span
    } else {
        0.0
    };
    std::array::from_fn(|k| (b[k] - a[k]) * f + a[k])
}

/// A u16 game angle from channel 7 (degrees): ch7 + 90°, truncated (V).
fn facing(ch7: f32) -> u16 {
    ((ch7 + 90.0) * 65536.0 / 360.0) as i32 as u16
}

/// The first frame a command at `t` acts on.
fn frame(t: u16) -> u32 {
    u32::from(t).div_ceil(2).max(1)
}

impl Camera {
    /// Its eye and look-at at `t` (game units).
    pub fn view(&self, t: f32) -> ([f32; 3], [f32; 3]) {
        let eye = path_at(&self.path, value(&self.along, t));
        let (yaw, pitch) = (
            (value(&self.yaw, t) + 90.0).to_radians(),
            value(&self.pitch, t).to_radians(),
        );
        let at = [
            eye[0] + 400.0 * pitch.cos() * yaw.cos(),
            eye[1] + 400.0 * pitch.sin(),
            eye[2] - 400.0 * pitch.cos() * yaw.sin(),
        ];
        (eye, at)
    }
}

impl Actor {
    /// Its place at `t` (game units, its feet).
    pub fn place(&self, t: f32) -> [f32; 3] {
        match self.path.len() {
            0 | 1 => self.at.map(f32::from),
            _ => path_at(&self.path, value(&self.along, t)),
        }
    }

    /// Its facing (u16 game angle) at `t`.
    pub fn facing(&self, t: f32) -> u16 {
        match self.yaw.is_empty() {
            true => self.facing,
            false => facing(value(&self.yaw, t)),
        }
    }

    /// Every clip it plays: (animation, ticks), its first first.
    pub fn clips(&self) -> impl Iterator<Item = (&str, u16)> {
        let first = std::iter::once((self.animation.as_str(), self.ticks));
        first.chain(self.plays.iter().map(|p| (p.animation.as_str(), p.ticks)))
    }

    /// Its clip at frame `k` (an index of [`Self::clips`]) and the clip's time, in clip ticks: a clip
    /// steps `rate` a frame from its first frame and loops (or holds its last key), and goes on
    /// where it was when played again while it plays.
    pub fn clip(&self, k: u32) -> (usize, f32) {
        let step = |time: f32, frames: u32, rate: f32, ticks: u16, once: bool| {
            let time = time + frames as f32 * rate;
            match once {
                true => time.min(f32::from(ticks) - 1.0),
                false => time % f32::from(ticks),
            }
        };
        // The clip, its first frame and its time the frame before.
        let (mut current, mut from, mut time) = (0, 1, 0.0);
        let mut clip = (&self.animation, self.ticks, self.rate, self.once);
        for (n, p) in self.plays.iter().enumerate() {
            let at = frame(p.t);
            if at > k {
                break;
            }
            time = step(time, at - from, clip.2, clip.1, clip.3);
            if p.animation != *clip.0 {
                time = 0.0;
            }
            (current, from) = (n + 1, at);
            clip = (&p.animation, p.ticks, p.rate, p.once);
        }
        let steps = (k + 1).saturating_sub(from);
        (current, step(time, steps, clip.2, clip.1, clip.3))
    }
}

impl Subtitles {
    /// The bubble's place at `t`: its anchor, text place and quads.
    pub fn place(&self, t: f32) -> (Anchor, [i16; 2], &[Quad]) {
        match self.moves.iter().rev().find(|m| f32::from(m.t) <= t) {
            Some(m) => (m.anchor, m.text, &m.quads),
            None => (self.anchor, self.text, &self.quads),
        }
    }
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

    /// The camera at `t`: the last cut to by then.
    pub fn view(&self, t: f32) -> &Camera {
        let cut = self.cuts.iter().rev().find(|c| f32::from(c.from) <= t);
        cut.unwrap_or(&self.camera)
    }

    /// The camera's eye and look-at at `t` (game units).
    pub fn camera(&self, t: f32) -> ([f32; 3], [f32; 3]) {
        self.view(t).view(t)
    }

    /// Its actors: `actor`, then `cast`.
    pub fn actors(&self) -> impl Iterator<Item = &Actor> {
        std::iter::once(&self.actor).chain(&self.cast)
    }

    /// The first actor's clip time at frame `k`, in clip ticks ([`Actor::clip`]).
    pub fn clip_time(&self, k: u32) -> f32 {
        self.actor.clip(k).1
    }

    /// The fade's alpha at `t`: 255 − elapsed·255/duration in integers, as the game's `func_1517EF00`.
    pub fn fade(&self, t: f32) -> u8 {
        let step = (t as u32 * 255 / u32::from(self.fade.duration).max(1)).min(255);
        (255 - step) as u8
    }

    /// The page shown at `t`, if any.
    pub fn page(&self, t: f32) -> Option<&Page> {
        let s = self.subtitles.as_ref()?;
        let shown = |p: &&Page| f32::from(p.t) <= t && t < f32::from(s.close);
        s.pages.iter().rev().find(shown)
    }

    /// The frame the voice starts on.
    pub fn voice_frame(&self) -> Option<u32> {
        (self.voice.as_ref()).map(|v| u32::from(v.t).div_ceil(2))
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

    /// Two points: a straight line, by length; one: the point; ends: the ends.
    #[test]
    fn paths_go_by_length() {
        let line = [[0, 0, 0], [100, 0, -200]];
        let mid = path_at(&line, 50.0);
        assert!((mid[0] - 50.0).abs() < 1e-3 && (mid[2] + 100.0).abs() < 1e-3);
        assert_eq!(path_at(&line, 100.0), [100.0, 0.0, -200.0]);
        assert_eq!(path_at(&line, -5.0), [0.0, 0.0, 0.0]);
        assert_eq!(path_at(&[[3, 4, 5]], 30.0), [3.0, 4.0, 5.0]);
        let bend = [[0, 0, 0], [100, 0, 0], [100, 0, 100]];
        let end = path_at(&bend, 100.0);
        assert!((end[0] - 100.0).abs() < 1e-3 && (end[2] - 100.0).abs() < 1e-3);
        let half = path_at(&bend, 50.0);
        assert!(
            (half[0] - 100.0).abs() < 2.0 && half[2].abs() < 2.0,
            "{half:?}"
        );
    }

    /// A clip loops from frame 1; a new clip starts over, the same one goes on.
    #[test]
    fn clips_switch_at_their_frames() {
        let play = |t, animation: &str| Play {
            t,
            animation: animation.into(),
            ticks: 10,
            rate: 2.0,
            once: false,
        };
        let mut a = Actor {
            character: "90".into(),
            animation: "a".into(),
            ticks: 10,
            rate: 1.0,
            at: [0; 3],
            facing: 0,
            lift: 0,
            scale: 0.5,
            once: false,
            path: vec![],
            along: vec![],
            yaw: vec![],
            plays: vec![play(9, "b"), play(20, "b")],
        };
        assert_eq!(a.clip(0), (0, 0.0));
        assert_eq!(a.clip(1), (0, 1.0));
        assert_eq!(a.clip(4), (0, 4.0));
        // t 9 acts on frame 5, from the clip's start: one step a frame.
        assert_eq!(a.clip(5), (1, 2.0));
        assert_eq!(a.clip(7), (1, 6.0));
        // t 20 (frame 10) plays b again: it goes on.
        assert_eq!(a.clip(10), (2, 2.0));
        a.plays.clear();
        assert_eq!(a.clip(12), (0, 2.0));
        // Once: it holds its last key.
        a.once = true;
        assert_eq!((a.clip(9), a.clip(12)), ((0, 9.0), (0, 9.0)));
    }
}
