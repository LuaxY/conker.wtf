//! The eyes' blink (`../lab/research/mp-original/faces.md`; the pack's `character.json` `faces`):
//! every player whose character blinks (Conker, Neo, the squirrel, the Sergeant, the villagers, the
//! caveman, the racer, the raptor), bots too, swaps its eye materials' texture to the half and the
//! shut look and back ([`Blink`]), both eyes and both segments together, on its own copies of the
//! materials (as `render::wear` dresses the team looks; their alpha modes kept: Conker's and Neo's
//! eyes blend). At health 0 the eyes hold until the respawn. The outros' actors and War: Raptor's
//! baby are not players: they keep the glTF's open eyes.

use bevy::gltf::GltfMaterialName;
use bevy::image::ImageLoaderSettings;
use bevy::prelude::*;
use conker_assets::{Character, FaceSegment};
use conker_rules::net::Fighting;
use conker_sim::net::{Combat, MatchView};
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::{LocalTimeline, Predicted};

use crate::render::{Model, Rig};

/// Each body's face segments ([`crate::character_files`]' order) and its folder.
#[derive(Resource)]
struct Faces(Vec<(String, Vec<FaceSegment>)>);

pub fn build(app: &mut App, characters: &[Character], names: &[&str]) {
    let faces = (characters.iter().zip(names)).map(|(c, n)| (n.to_string(), c.faces.clone()));
    app.insert_resource(Faces(faces.collect()));
    app.add_systems(Update, (dress, show).chain());
    app.add_systems(FixedUpdate, blink.run_if(not(is_in_rollback)));
}

/// The blink (C `func_1507E2B0`, `func_1502EEF4`; V c1, c4): open for `open` more frames, then
/// half, shut and half for a frame each (`step` 1-3), then open again for ⌊T/2⌋ frames, T =
/// `rand() % 140 + 10` VIs at 2 VIs a frame (5-74 frames).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blink {
    open: u16,
    step: u8,
}

impl Blink {
    pub fn new(rand: u32) -> Self {
        Self {
            open: ((rand % 140 + 10) / 2) as u16,
            step: 0,
        }
    }

    /// The look this frame: 0 open, 1 half shut, 2 shut (the segment's `textures`).
    pub fn look(self) -> usize {
        [0, 1, 2, 1][usize::from(self.step)]
    }

    /// The next frame; `rand` draws the next open span's T.
    pub fn tick(&mut self, rand: impl FnOnce() -> u32) {
        match self.step {
            0 if self.open > 1 => self.open -= 1,
            0 => self.step = 1,
            3 => *self = Self::new(rand()),
            _ => self.step += 1,
        }
    }
}

/// A slot's own `rand()` (an LCG): every client draws the same blinks for it.
fn rand(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    *seed >> 16
}

/// A slot's blink `ticks` frames into the match, and its random state then.
fn blink_at(slot: u16, ticks: u32) -> (Blink, u32) {
    let mut seed = 0x5EED_0000 ^ u32::from(slot);
    let mut blink = Blink::new(rand(&mut seed));
    for _ in 0..ticks {
        blink.tick(|| rand(&mut seed));
    }
    (blink, seed)
}

/// A blinking player's eyes: its blink and the random state it draws from (`synced` once set
/// from the match start), the look shown, and each eye material's copy on its model with its
/// three looks.
#[derive(Component)]
pub(crate) struct Eyes {
    blink: Blink,
    seed: u32,
    synced: bool,
    shown: usize,
    materials: Vec<(Handle<StandardMaterial>, [Handle<Image>; 3])>,
}

/// Once a blinker's model is in: its eye materials on copies of their own, and its blink.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dress(
    faces: Res<Faces>,
    players: Query<(Entity, &Combat, &Model), (With<Rig>, Without<Eyes>)>,
    children: Query<&Children>,
    mut parts: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    for (entity, c, model) in &players {
        let Some((folder, segments)) = faces.0.get(model.body).filter(|f| !f.1.is_empty()) else {
            continue;
        };
        let mut done: Vec<String> = Vec::new();
        let mut eyes = Vec::new();
        for segment in segments {
            for m in &segment.materials {
                // Two segments drawing one material give it the same looks (Conker's 12).
                let name = format!("material_{m:02}");
                if done.contains(&name) {
                    continue;
                }
                let mut copy: Option<Handle<StandardMaterial>> = None;
                for e in children.iter_descendants(model.scene) {
                    if let Ok((n, mut material)) = parts.get_mut(e)
                        && n.0 == name
                    {
                        let own = copy.get_or_insert_with(|| {
                            let own = materials.get(&material.0).cloned().unwrap_or_default();
                            materials.add(own)
                        });
                        material.0 = own.clone();
                    }
                }
                let Some(copy) = copy else {
                    continue;
                };
                // The looks sample as the glTF's own eye texture does.
                let sampler = (materials.get(&copy))
                    .and_then(|m| images.get(m.base_color_texture.as_ref()?))
                    .map(|i| i.sampler.clone())
                    .unwrap_or_default();
                let looks = segment.textures.clone().map(|t| {
                    let sampler = sampler.clone();
                    (assets.load_builder())
                        .with_settings(move |s: &mut ImageLoaderSettings| {
                            s.sampler = sampler.clone()
                        })
                        .load(format!("characters/{folder}/{t}"))
                });
                eyes.push((copy, looks));
                done.push(name);
            }
        }
        let (blink, seed) = blink_at(c.slot, 0);
        commands.entity(entity).insert(Eyes {
            blink,
            seed,
            synced: false,
            shown: 0,
            materials: eyes,
        });
    }
}

/// Each live tick, every blinker's blink steps, unless it is dead (the eyes hold to the respawn:
/// V k1; the local player's health is its prediction's). Once the match start is known (the
/// server's `--wait` replays), each blink is set to as far into the match as it is: a slot's
/// blinks follow the match's frames, so a scripted screenshot sees the same one each run.
fn blink(
    mut players: Query<(&Combat, Option<&Fighting>, Has<Predicted>, &mut Eyes)>,
    game: Query<&MatchView>,
    timeline: Res<LocalTimeline>,
) {
    let tick = timeline.tick().0;
    let start = game.iter().find_map(|m| m.start).filter(|&s| s <= tick);
    for (c, fighting, predicted, mut eyes) in &mut players {
        if let Some(s) = start.filter(|_| !eyes.synced) {
            (eyes.blink, eyes.seed) = blink_at(c.slot, tick - s);
            eyes.synced = true;
        }
        let health = match fighting {
            Some(f) if predicted => f.state.health,
            _ => c.health,
        };
        if health == 0 {
            continue;
        }
        let Eyes { blink, seed, .. } = &mut *eyes;
        let was = blink.look();
        blink.tick(|| rand(seed));
        if blink.look() != was {
            debug!("blink: slot {} look {} at t={tick}", c.slot, blink.look());
        }
    }
}

/// Each eye material shows its blink's look.
fn show(mut players: Query<&mut Eyes>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for mut eyes in &mut players {
        let look = eyes.blink.look();
        if eyes.shown == look {
            continue;
        }
        for (material, looks) in &eyes.materials {
            if let Some(mut m) = materials.get_mut(material) {
                m.base_color_texture = Some(looks[look].clone());
            }
        }
        eyes.shown = look;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V c1: blinks start at reads 66, 132, 141 and 199, open 63, 6 and 55 frames between them
    /// (⌊T/2⌋ for T 127, 13, 111): half, shut, half a frame each, then open.
    #[test]
    fn a_blink_follows_the_oracle() {
        // rand() values giving T = 127, 13, 111 (T = rand % 140 + 10).
        let mut t = [117u32, 3, 101].into_iter();
        let mut blink = Blink { open: 1, step: 0 };
        let mut looks = Vec::new();
        for _ in 66..=202 {
            blink.tick(|| t.next().unwrap_or(1000));
            looks.push(blink.look());
        }
        let starts: Vec<usize> = (1..looks.len())
            .filter(|&k| looks[k] == 1 && looks[k - 1] == 0)
            .map(|k| 66 + k)
            .collect();
        assert_eq!(&looks[..4], [1, 2, 1, 0]);
        assert_eq!(starts, [132, 141, 199]);
        // T from rand: 10-149 VIs, 5-74 frames.
        assert_eq!((Blink::new(0).open, Blink::new(139).open), (5, 74));
    }
}
