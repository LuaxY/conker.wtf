//! Root motion: the clips that move the player by their root's z track instead of its speed (the creep 0,
//! weapon 0x16's attack 0x77, the shot death 0x17E and seven more on character 117; descriptor flag 0x40).
//!
//! The game (C: `func_150A81D0`, `func_1502DB84`, `func_15059C84`; V on anim_creep's 174 creeping frames,
//! the loop point included, to 0.00025):
//! - The renderer measures the root's z at the clock's time before the frame's advance (`D_80088554`, in
//!   1/32768 model units: `(a << 10) + (b − a)·round(frac·1024)` from the two stored frames around the time,
//!   and in the last stretch from the last frame toward the loop value), keeps it (obj+0x1F8) and its
//!   advance (obj+0x1F4), and flags the advance (obj+0x1FC bit 0). A clock that wrapped first takes back the
//!   loop's travel; the first render of a newly requested clip takes its position only.
//! - The next frame's movement moves the player along its facing by half that advance (the model's scale
//!   0.5) instead of by its speed, so the creep's step trails its clock by two frames.
//!
//! The tracks are data from the player's ROM (`conker_extract::Slice::root_clips`: per clip `logical`,
//! `duration`, `step`, `z` and `end`, field for field a [`RootTrack`]), like the arctangent table:
//! [`crate::collision::CollisionWorld::roots`].

/// One clip's root-z track (`extract_character::RootMotion` with the clip's ids and timing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RootTrack {
    /// The animation ids that play the clip.
    pub ids: Vec<u16>,
    /// Length in ticks, and ticks between stored frames.
    pub duration: u16,
    pub step: u16,
    /// The root z channel per stored frame, raw (32 per model unit), and its value at the loop point.
    pub z: Vec<i32>,
    pub end: i16,
}

impl RootTrack {
    /// The root's z at clock time `t` in model units, as the renderer computes it (C: 0x150A8A18 for the
    /// fraction, 0x150A8D74 for the root z channel).
    pub fn z_at(&self, t: f32) -> f32 {
        let (n, last, step) = (
            self.z.len(),
            f32::from(self.duration) - 1.0,
            f32::from(self.step),
        );
        if n == 0 {
            return 0.0;
        }
        let (a, b, q) = if last <= t {
            // Past the last stored frame: toward the loop value.
            (self.z[n - 1], i32::from(self.end), t)
        } else {
            let key = floor(last / step) * step;
            if t < key {
                let q = t / step;
                let i = (floor(q) as usize).min(n - 2);
                (self.z[i], self.z[i + 1], q)
            } else {
                // The last stored frame sits at duration − 1: a shortened stretch.
                (self.z[n - 2], self.z[n - 1], (t - key) / (last - key))
            }
        };
        let frac = ((q - floor(q)) * 1024.0).round_ties_even() as i32;
        ((a << 10) + (b - a) * frac) as f32 / 32768.0
    }

    /// The loop's travel in model units (`D_80088558 · 0.03125`).
    pub fn travel(&self) -> f32 {
        f32::from(self.end) * 0.03125
    }
}

/// `floor` for the clock's non-negative times (and their quotients), as `floor.w.s` there.
fn floor(v: f32) -> f32 {
    (v as i32) as f32
}

/// The player's animation clock as far as root motion needs it, and the renderer's root fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(
    feature = "net",
    derive(serde::Serialize, serde::Deserialize, bevy::reflect::Reflect)
)]
pub struct Root {
    /// The animation playing (obj+0x84) and its clock time (clip ticks).
    pub id: u16,
    pub time: f32,
    /// obj+0x1F8: the root's z at the last render; obj+0x1F4: its advance then.
    pub at: f32,
    pub advance: f32,
    /// obj+0x1FC bits 0 (the next movement takes the advance), 1 (a new clip: its first render only
    /// takes its position) and 2 (the clock wrapped in its last advance).
    pub moves: bool,
    pub fresh: bool,
    pub wrapped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_like_the_renderer() {
        // Clip 0 of character 117 (the creep): 45 ticks, a frame every 4, the root ending at 4736.
        let creep = RootTrack {
            ids: vec![0],
            duration: 45,
            step: 4,
            z: vec![
                0, 683, 1867, 1971, 2036, 2186, 2628, 3575, 4577, 4432, 4104, 4506,
            ],
            end: 4736,
        };
        // anim_creep row 55: obj+0x1F8 = 2.4803772 at time 0.465 (round(0.11625 · 1024) = 119).
        assert_eq!(creep.z_at(0.465), 2.480_377_2);
        assert_eq!(creep.z_at(0.0), 0.0);
        assert_eq!(creep.z_at(4.0), 683.0 / 32.0);
        // Past tick 44 toward the loop value; the travel comes back at the wrap.
        assert_eq!(creep.z_at(44.5), (4506.0 + 230.0 * 0.5) / 32.0);
        assert_eq!(creep.travel(), 148.0);
    }
}
