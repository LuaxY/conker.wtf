//! The level materials the game binds at run time (CBFD's per-scene switch in `func_1510B9D0`):
//! panels showing their frames in turn on one clock (0x33's crowd, `race.json`'s `crowd`; 0x1A's
//! two computer screens, `segments.json`'s `screens`; [`frame`]), and 0x1A's lamps, lit or dark with
//! their switching light ([`crate::lights::Switch`]). Each material is found by name as the level
//! spawns (one per panel or lamp, drawn by nothing else) and its texture swapped in place, its alpha
//! mode kept; the frames sample as the glTF's own texture does.

use bevy::gltf::GltfMaterialName;
use bevy::image::ImageLoaderSettings;
use bevy::prelude::*;
use conker_assets::Lamp;
use conker_assets::race::Crowd;
use lightyear::prelude::LocalTimeline;

use crate::lights::Flickers;

/// The game runs at 2 VIs a frame: the panels' clock.
const VIS_PER_TICK: u32 = 2;

/// Where a bound material's frame comes from: a panel's clock (a frame every `step` VIs), or
/// switching light `k`'s lamp (frame 0 on, 1 off).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clock {
    Every(u8),
    Lamp(usize),
}

/// A material to bind: its name in `level.gltf`, its frames (relative to the map's folder) and its
/// clock.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    pub material: String,
    pub frames: Vec<String>,
    pub clock: Clock,
}

/// The map's folder and the materials it binds.
#[derive(Resource)]
struct Wants {
    folder: String,
    list: Vec<Want>,
}

/// The materials a map binds: its panels, then each lamp's materials.
pub fn wants(screens: Option<&Crowd>, lamps: &[Lamp]) -> Vec<Want> {
    let panels = screens.into_iter().flat_map(|c| {
        (c.panels.iter()).map(|p| Want {
            material: p.material.clone(),
            frames: p.frames.clone(),
            clock: Clock::Every(c.step),
        })
    });
    let lamps = lamps.iter().enumerate().flat_map(|(k, l)| {
        (l.materials.iter()).map(move |m| Want {
            material: m.clone(),
            frames: vec![l.on.clone(), l.off.clone()],
            clock: Clock::Lamp(k),
        })
    });
    panels.chain(lamps).collect()
}

/// The frame a material shows at `vis` VIs: a panel's `(vis / step) % frames` (one clock for all,
/// C `func_150D765C`, `func_15100464`; the phase is free), a lamp's 0 while its light `on(k)`, else 1.
pub fn frame(clock: Clock, vis: u32, frames: usize, on: impl Fn(usize) -> bool) -> usize {
    match clock {
        Clock::Every(step) => (vis / u32::from(step)) as usize % frames,
        Clock::Lamp(k) => usize::from(!on(k)),
    }
}

pub fn build(app: &mut App, map: &str, screens: Option<&Crowd>, lamps: &[Lamp]) {
    let list = wants(screens, lamps);
    if list.is_empty() {
        return;
    }
    let folder = format!("maps/{map}");
    app.insert_resource(Wants { folder, list });
    app.add_systems(Update, show);
}

/// Binds each wanted material as the level spawns, then shows its frame.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show(
    wants: Res<Wants>,
    timeline: Res<LocalTimeline>,
    flickers: Option<Res<Flickers>>,
    added: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>), Added<GltfMaterialName>>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut bound: Local<Vec<(Handle<StandardMaterial>, Vec<Handle<Image>>, Clock)>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (name, look) in &added {
        let Some(want) = wants.list.iter().find(|w| w.material == name.0) else {
            continue;
        };
        if bound.iter().any(|b| b.0 == look.0) {
            continue;
        }
        let sampler = (materials.get(&look.0))
            .and_then(|m| images.get(m.base_color_texture.as_ref()?))
            .map(|i| i.sampler.clone())
            .unwrap_or_default();
        let frames = want.frames.iter().map(|f| {
            let sampler = sampler.clone();
            (assets.load_builder())
                .with_settings(move |s: &mut ImageLoaderSettings| s.sampler = sampler.clone())
                .load(format!("{}/{f}", wants.folder))
        });
        bound.push((look.0.clone(), frames.collect(), want.clock));
    }
    let vis = timeline.tick().0.wrapping_mul(VIS_PER_TICK);
    // A light not known yet is lit, as `level.gltf` draws it.
    let on = |k: usize| (flickers.as_ref()).is_none_or(|f| f.switches.get(k).is_none_or(|s| s.on));
    for (look, frames, clock) in bound.iter() {
        let shown = &frames[frame(*clock, vis, frames.len(), on)];
        let now = materials
            .get(look)
            .and_then(|m| m.base_color_texture.as_ref());
        if now != Some(shown)
            && let Some(mut m) = materials.get_mut(look)
        {
            m.base_color_texture = Some(shown.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_assets::race::Panel;

    fn panel(material: &str, frames: [&str; 3]) -> Panel {
        let frames = frames.map(|f| format!("textures/{f}.png")).to_vec();
        let material = material.into();
        Panel { material, frames }
    }

    /// 0x1A's screens (C `func_15100464`, V `seg1a.py`'s dumps): 1547 → 1548 → 1549 every 8 VIs (4
    /// frames) on one clock, segment 3 (`m71`) a frame ahead of segment 2 (`m69`).
    #[test]
    fn the_vault_screens_loop_with_one_ahead() {
        let screens = Crowd {
            step: 8,
            panels: vec![
                panel("m69-seg02000000", ["1547", "1548", "1549"]),
                panel("m71-seg03000000", ["1548", "1549", "1547"]),
            ],
        };
        let wants = wants(Some(&screens), &[]);
        let at = |w: &Want, tick: u32| {
            let i = frame(w.clock, tick * VIS_PER_TICK, w.frames.len(), |_| true);
            w.frames[i][9..13].to_string()
        };
        let shown: Vec<_> = (0..4)
            .map(|k| [&wants[0], &wants[1]].map(|w| at(w, 4 * k)))
            .collect();
        assert_eq!(
            shown,
            [
                ["1547", "1548"],
                ["1548", "1549"],
                ["1549", "1547"],
                ["1547", "1548"]
            ]
        );
        // The frame holds 4 match frames.
        assert_eq!(at(&wants[0], 3), "1547");
        // 0x33's crowd: a frame every 4 VIs, 2 match frames, over its 5.
        let crowd = (0..6).map(|t| frame(Clock::Every(4), t * VIS_PER_TICK, 5, |_| true));
        assert_eq!(crowd.collect::<Vec<_>>(), [0, 0, 1, 1, 2, 2]);
    }

    /// 0x1A's lamps: each switching light k's materials draw `on` while it is on, `off` while it is
    /// off (C `func_15100340`: the byte at 0x800BE500 + k toggles with the light).
    #[test]
    fn the_vault_lamps_follow_their_lights() {
        let lamp = |materials: &[&str]| Lamp {
            materials: materials.iter().map(|m| m.to_string()).collect(),
            on: "textures/0364.png".into(),
            off: "textures/3197.png".into(),
        };
        let lamps = [
            lamp(&["m30-seg04000000", "m31-seg04000000"]),
            lamp(&["m75-seg05000000"]),
        ];
        let wants = wants(None, &lamps);
        let clocks: Vec<_> = wants.iter().map(|w| w.clock).collect();
        assert_eq!(clocks, [Clock::Lamp(0), Clock::Lamp(0), Clock::Lamp(1)]);
        // Light 0 off, light 1 on.
        let on = |k: usize| k == 1;
        let shown: Vec<_> = (wants.iter())
            .map(|w| w.frames[frame(w.clock, 0, 2, on)].as_str())
            .collect();
        assert_eq!(
            shown,
            [
                "textures/3197.png",
                "textures/3197.png",
                "textures/0364.png"
            ]
        );
    }
}
