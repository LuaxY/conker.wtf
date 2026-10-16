//! The body parts where each player's model is drawn (`lab/research/mp-original/held-leftovers.md`
//! §2), for a server's [`Match::parts`]: every player's animation clock driven from its fighter as
//! the client drives a remote player's, posed on the pack's glTF ([`conker_assets::pose`]), and its
//! model's spheres ([`conker_assets::Parts`]) placed on the joints: pos + Ry(facing + 90°) · scale ·
//! (joint · offset + (0, lift, 0)), radius × scale (V pc3: the head 177.7 along the pistol's ray, as
//! the oracle's joint matrices give). [`Poses::place`] before the step (the game tests the last
//! draw's matrices, C/V pc3), [`Poses::tick`] after it.

use std::collections::HashMap;
use std::fmt::Display;

use conker_anim::{AnimPlayer, NONE, request_at};
use conker_assets::pose::{self, Skeleton};
use conker_assets::{Character, Parts};
use conker_sim::movement::{STRAFE_PERCENT, cos_sin};

use crate::characters;
use crate::game::{Action, Event, Fighter, Match};
use crate::projectile::{BodyParts, Sphere};
use crate::{gun, heist, raptor};

/// A model with parts: its skeleton and clips, its spheres, its lift, and its clips by logical id
/// (id, glTF animation, loop length in ticks), sorted.
#[derive(Clone)]
struct Model {
    skeleton: Skeleton,
    parts: Parts,
    lift: f32,
    clips: Vec<(u16, String, u16)>,
}

impl Model {
    fn clip(&self, id: u16) -> Option<&(u16, String, u16)> {
        let k = self.clips.binary_search_by_key(&id, |c| c.0).ok()?;
        Some(&self.clips[k])
    }
}

/// Each model's poses and each slot's clock (with its speed percentage before its last tick).
#[derive(Clone, Default)]
pub struct Poses {
    models: HashMap<u16, Model>,
    pub clocks: Vec<(AnimPlayer, u8)>,
}

/// The models a class may play on `scene` with the `cheats` bits: the sides', the scene's own
/// (1a's Wise Guy, 3f's raptor, 2b's villagers, 33's racers) and its select's picks.
pub fn models(scene: u16, cheats: u32) -> Vec<u16> {
    let picks = characters::allowed(scene, cheats);
    let mut out: Vec<u16> = [117, 128, 136, 59, 83, 156, 157, 144]
        .into_iter()
        .filter(|&m| match m {
            59 => scene == 0x1A,
            83 => scene == 0x3F,
            156 | 157 => scene == 0x2B,
            144 => scene == 0x33,
            _ => true,
        })
        .chain(
            (characters::PICKS.iter().enumerate())
                .filter_map(|(k, p)| (picks >> k & 1 != 0).then_some(p.1)),
        )
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

impl Poses {
    /// The parts and poses of `models` from the pack (`read(path)`: a pack file's bytes). A model
    /// without a folder or without `parts` gets none (the cylinder); a mesh-only pick (its
    /// `clips_from`) is posed by that folder's clips.
    pub fn load<E: Display>(
        models: &[u16],
        read: impl Fn(&str) -> Result<Vec<u8>, E>,
    ) -> conker_assets::Result<Self> {
        let file = |path: String| {
            read(&path).map_err(|e| conker_assets::Error::Read {
                file: path,
                message: e.to_string(),
            })
        };
        let json = |folder: &str| -> conker_assets::Result<Option<Character>> {
            let path = format!("characters/{folder}/character.json");
            let Ok(bytes) = file(path.clone()) else {
                return Ok(None);
            };
            conker_assets::parse_json(&path, &bytes).map(Some)
        };
        let mut skeletons: HashMap<String, Skeleton> = HashMap::new();
        let mut out = Self::default();
        for &m in models {
            let folder = m.to_string();
            let Some(c) = json(&folder)? else { continue };
            let Some(parts) = c.parts.clone() else {
                continue;
            };
            let from = c.clips_from.clone().unwrap_or_else(|| folder.clone());
            let clips = match &c.clips_from {
                Some(f) => json(f)?.map_or_else(Vec::new, |o| o.clips),
                None => c.clips.clone(),
            };
            for f in [&from, &folder] {
                if !skeletons.contains_key(f) {
                    let gltf = file(conker_assets::character_gltf(f))?;
                    let bin = file(format!("characters/{f}/character.bin"))?;
                    skeletons.insert(f.clone(), Skeleton::parse(&gltf, &bin)?);
                }
            }
            let own = skeletons[&folder].clone();
            let skeleton = match from == folder {
                true => own,
                false => own.posed_by(&skeletons[&from]),
            };
            let mut ids: Vec<(u16, String, u16)> = Vec::new();
            for clip in &clips {
                for &id in &clip.logical {
                    if !ids.iter().any(|c| c.0 == id) {
                        ids.push((id, clip.animation(), clip.duration_ticks));
                    }
                }
            }
            ids.sort_unstable_by_key(|c| c.0);
            out.models.insert(
                m,
                Model {
                    skeleton,
                    parts,
                    lift: f32::from(c.model_lift),
                    clips: ids,
                },
            );
        }
        Ok(out)
    }

    /// Whether `model` has parts here.
    pub fn has(&self, model: u16) -> bool {
        self.models.contains_key(&model)
    }

    fn model(&self, f: &Fighter) -> Option<&Model> {
        self.models.get(&characters::model(f.class)?)
    }

    /// One frame of every player's clock, after the step: the rules' animation at its request (the
    /// armed walks' rates count the speed percentage before the tick, 65 on a strafing tick), the
    /// first a cut, the jump onto a mounted gun at 1.3, Heist's carrier and War: Raptor's rates;
    /// the class's own clip's length (`characters::clip`). As conker-client's `tick_anims` for a
    /// remote player (Race and Tank have no parts-tested hits: not driven). A clip a hit in the
    /// step's `events` started is not advanced until the next frame: the hit lands after the
    /// victim's own update (V: 43 of 46 traced 0x11D starts show time 0, a clip the player's own
    /// update starts at its rate). And it fades in afresh from the pose showing, not as the way
    /// back to a clip being faded from (I; V e_w09: advanced at once, the hit reaction's flung hand
    /// stops 6 of the right gun's bullets the game lets by, fading back 3 others).
    pub fn tick(&mut self, m: &Match, events: &[Event]) {
        self.clocks.resize(m.players.len(), Default::default());
        for (i, (f, clock)) in m.players.iter().zip(&mut self.clocks).enumerate() {
            let Some(model) = self
                .models
                .get(&characters::model(f.class).unwrap_or(u16::MAX))
            else {
                continue;
            };
            let (p, id) = (&f.body, f.anim);
            let percent = if p.prev_strafe {
                STRAFE_PERCENT
            } else {
                clock.1
            };
            let mut req = request_at(id, p.xz_speed, p.target_speed, percent);
            if clock.0.id == NONE {
                req.blend = 0.0;
            }
            if id == conker_anim::id::JUMP && f.gun.is_some_and(|g| g.state == gun::LAUNCH) {
                req.rate = 1.3;
            }
            let e = || conker_anim::effective_speed(p.xz_speed, p.target_speed.min(25.2));
            match id {
                heist::WALK => req.rate = e() / 6.0 + 0.4,
                heist::RUN => req.rate = e() / 26.0 + 0.5,
                _ => {}
            }
            match f.class {
                raptor::CLASS => req.rate = raptor_rate(id, p.xz_speed, clock.0.id != id),
                raptor::CAVEMAN => match id {
                    raptor::EGG_IDLE => req.rate = 1.0,
                    raptor::EGG_WALK => req.rate = e() / 6.0 + 0.4,
                    raptor::EGG_RUN => req.rate = e() / 16.0 + 0.5,
                    _ => {}
                },
                _ => {}
            }
            let length = model.clip(characters::clip(f.class, id)).map_or(1, |c| c.2);
            // The clip's length, for a clock seeded mid-clip too (`play` sets it on a new clip).
            clock.0.duration = f32::from(length);
            let hit =
                (events.iter()).any(|e| matches!(*e, Event::Hit { victim, .. } if victim == i));
            match hit && clock.0.id != id {
                true => {
                    clock.0.weight = 0;
                    clock.0.play(req, length);
                }
                false => clock.0.tick(req, length),
            }
            clock.1 = p.speed_percent;
        }
    }

    /// Every player's body parts where its clock poses it now ([`Match::parts`]); none in Race and
    /// Tank or without models (the fitted volumes stand in), for a free slot or a player out.
    pub fn place(&self, m: &mut Match) {
        if m.settings.race || m.settings.tank || self.models.is_empty() {
            m.parts.clear();
            return;
        }
        m.parts = (m.players.iter().enumerate())
            .map(|(i, f)| {
                let clock = &self.clocks.get(i)?.0;
                let out = m.free.contains(&i) || f.out > 0;
                (!out).then(|| self.parts(f, clock)).flatten()
            })
            .collect();
    }

    fn parts(&self, f: &Fighter, clock: &AnimPlayer) -> Option<BodyParts> {
        let model = self.model(f)?;
        let sk = &model.skeleton;
        let name = |id: u16| {
            model
                .clip(characters::clip(f.class, id))
                .map_or("", |c| &c.1)
        };
        let mut local = sk.sample(name(clock.id), clock.time);
        let w = clock.prev_weight();
        if w > 0.0 && clock.prev != clock.id && clock.prev != NONE {
            local = pose::blend(&local, &sk.sample(name(clock.prev), clock.prev_time), w);
        }
        // The creep clip's walk is in its root's z (the raptor's braking and running bite too),
        // which the game turns into motion: the root stays at rest (the client's `pin_creep_root`).
        let pinned: &[u16] = match f.class {
            raptor::CLASS => &[
                conker_sim::raptor::clip::SLOW,
                conker_sim::raptor::clip::RUN_BITE,
            ],
            _ => &[conker_anim::id::CREEP],
        };
        if pinned.contains(&clock.id) || pinned.contains(&clock.prev) {
            local[0].t[2] = sk.rest[0].t[2];
        }
        // The aimed gun's twist turns the upper body (`joint_01`) about the body's up axis; the
        // head (`joint_12`) keeps the legs' facing (the client's `held::twist`).
        if f.body.twist != 0 && local.len() > 12 {
            let h = -f32::from(f.body.twist) * core::f32::consts::PI / 65536.0;
            let turn = [0.0, h.sin(), 0.0, h.cos()];
            let spine = local[1].r;
            local[1].r = pose::qmul(turn, spine);
            let back = pose::qmul(pose::qmul(pose::qinv(spine), pose::qinv(turn)), spine);
            local[12].r = pose::qmul(back, local[12].r);
        }
        let joints = sk.model(&local);
        let scale = characters::scale(f.class);
        let (c, s) = cos_sin(f.body.facing);
        let pos = f.body.pos;
        // The floor's sink (obj+0xCC) lowers the joints, after the scale (V n34s2: −6 on kind 0xA,
        // the idle Tedi's head 6 lower than on kind 5, and a twin-gun bullet at +45.5 meets it).
        let sink = f32::from(f.body.sink);
        // Model +z is the facing (cos a, −sin a), model +x is (−sin a, −cos a).
        let place = |[x, y, z]: [f32; 3]| {
            let (x, y, z) = (x * scale, (y + model.lift) * scale, z * scale);
            [
                pos[0] - x * s + z * c,
                pos[1] + sink + y,
                pos[2] - x * c - z * s,
            ]
        };
        let spheres = (model.parts.spheres.iter())
            .filter_map(|&[j, part, r, x, y, z]| {
                let at = joints.get(j as usize)?.apply([x, y, z].map(f32::from));
                Some(Sphere {
                    centre: place(at),
                    radius: f32::from(r) * scale,
                    joint: j as u8,
                    part: part as u8,
                })
            })
            .collect();
        // obj+0x9C: the head pop hides joints 12-15 on every model (`func_1507DE4C`, C; V e_w39
        // 971), the shotgun's and the chainsaw's kill (0x2F) joints 1-15 (`func_15194AB4`, C; V
        // sg_e1, e_w22).
        let halved = matches!(
            f.action,
            Action::Hurt {
                reaction: 0x2F,
                lethal: true,
                ..
            }
        );
        let hidden = match () {
            _ if halved => 0xFFFE,
            _ if f.popped => 0xF000,
            _ => 0,
        };
        Some(BodyParts {
            broad: model.parts.broad.map(|v| f32::from(v) * scale),
            hidden,
            spheres,
            origin: pos,
        })
    }
}

/// The raptor's clip rates (V r1-r4; conker-client's `raptor::rate`): its idle at 1 on the first
/// frame, then 0.5; braking 0.1 + speed/14; running 0.5 + speed/43.4; its bites, jump, drop and
/// respawn at 0.8.
fn raptor_rate(id: u16, speed: f32, first: bool) -> f32 {
    use conker_sim::raptor::clip;
    match id {
        clip::IDLE if first => 1.0,
        clip::IDLE => 0.5,
        clip::SLOW => 0.1 + speed / 14.0,
        clip::RUN => 0.5 + speed / 43.4,
        clip::BITE | clip::JUMP | clip::RUN_BITE | clip::DROP | clip::RESPAWN => 0.8,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage;
    use crate::game::Arena;
    use crate::projectile::{Bullet, HEAD};
    use crate::{Placement, Rng};
    use conker_sim::collision::CollisionWorld;
    use conker_sim::movement::Input;

    /// The Tedi's poses from the pack (`CONKER_ASSETS`, else `<repo>/assets`); None without it.
    fn tedi() -> Option<Poses> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into());
        let dir = std::path::PathBuf::from(dir);
        let poses = Poses::load(&[117], |p| std::fs::read(dir.join(p))).ok()?;
        poses.has(117).then_some(poses)
    }

    fn clock(id: u16, time: f32) -> AnimPlayer {
        AnimPlayer {
            id,
            time,
            ..AnimPlayer::default()
        }
    }

    /// V pc3 (`held-leftovers.md` §2): P1's pistol ray, level along (−0.049, 0, −0.999), against P2
    /// (a Tedi 200 ahead, facing 49408): crouched (0x16 at tick 16) the head, part 0xA on joint 12,
    /// 177.7 along, and the bullet takes 6 → 3; standing (0xF at tick 22) it passes beside the neck
    /// (the cylinder hits it). Without the pack: skipped.
    #[test]
    fn the_pistol_meets_the_crouched_head_and_passes_the_standing_neck() {
        let Some(mut poses) = tedi() else {
            eprintln!("no pack: skipping");
            return;
        };
        let p2 = Placement {
            index: 0,
            pos: [-2256.8135, -521.0, 5815.2409],
            facing: 49408,
            camera: [0.0; 3],
        };
        let p1 = Placement {
            pos: [-2247.0, -521.0, 6015.0],
            facing: 16896,
            ..p2
        };
        let players = vec![Fighter::new(&p1, 0, 0), Fighter::new(&p2, 1, 0)];
        let dir = [-0.049_067_546, 0.0, -0.998_795_5];
        let crouched = [-2_231.066, -469.752_93, 6_024.150_4];
        let standing = [-2_230.745_8, -469.527_16, 6_025.231_4];
        let parts = |poses: &Poses, id, t| poses.parts(&players[1], &clock(id, t)).unwrap();
        let hit = parts(&poses, 22, 16.0).hit(crouched, dir, p2.pos, [0.0; 3]);
        eprintln!("pc3: the crouched head {hit:?}");
        assert!(
            matches!(hit, Some((d, HEAD)) if (d - 177.7).abs() < 0.3),
            "{hit:?}"
        );
        let parts_standing = parts(&poses, 15, 22.0);
        assert_eq!(parts_standing.hit(standing, dir, p2.pos, [0.0; 3]), None);

        // The bullet in a match: the crouched P2 takes the head's 1 and the pistol's 2.
        let big = 20_000.0;
        let y = -521.0;
        let world = CollisionWorld::new([
            ([[-big, y, -big], [-big, y, big], [big, y, big]], 5),
            ([[-big, y, -big], [big, y, big], [big, y, -big]], 5),
        ]);
        let arena = Arena::default();
        let mut m = Match::new(&arena, players.clone(), Rng::new(1), 0);
        m.players[1].immune = 0;
        poses.clocks = vec![Default::default(), (clock(22, 16.0), 0)];
        poses.place(&mut m);
        let shot = |at, m: &mut Match| {
            let b = Bullet::fire(&world, 0, at, 0, damage::PISTOL_BULLET);
            m.bullets = vec![(
                m.tick,
                Bullet {
                    dir,
                    vel: dir.map(|d| d * b.speed),
                    parts: true,
                    ..b
                },
            )];
            let mut input = [Input::default(); 2];
            input[1].buttons = conker_sim::movement::BUTTON_Z; // P2 holds the crouch
            m.step(&arena, &input, &world);
            m.players[1].health
        };
        assert_eq!(shot(crouched, &mut m), 3);
        // Standing, the same pistol misses (the cylinder, without the poses, hits for 2).
        let mut m = Match::new(&arena, players.clone(), Rng::new(1), 0);
        m.players[1].immune = 0;
        poses.clocks = vec![Default::default(), (clock(15, 22.0), 0)];
        poses.place(&mut m);
        assert_eq!(shot(standing, &mut m), 6);
        let mut m = Match::new(&arena, players, Rng::new(1), 0);
        m.players[1].immune = 0;
        assert_eq!(shot(standing, &mut m), 4);
    }
}
