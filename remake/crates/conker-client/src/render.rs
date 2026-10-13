//! What the windowed client draws: the asset pack's level (scene 0x2D) and both sides' characters
//! (the squirrel soldier, 128, and the Tedi, 117), which Bevy's glTF loader reads from the pack
//! (natively its folder, in the browser `assets/` next to the page; [`Slice`] carries the data
//! files), one skinned character per networked player, its side's ([`Combat::side`]), posed by its
//! animation clock ([`crate::Anim`]) with its own clips, the view of the
//! follow camera ([`crate::FollowCamera`]) and a free debug camera. Combat adds the match's bullets,
//! aimed shots, grenades and rockets (placeholder spheres: the pack has no models for them), hides the level's pickup
//! models while taken (they are the level glTF's placement nodes); War: Total War adds its props
//! ([`props_scene`]), shown as the match says, and its victory outro ([`crate::outro`]). The HUD is
//! [`crate::hud`]'s;
//! [`TEXT_HUD_KEY`] shows the old text readout too ([`crate::hud_text`]; in the browser, the page's
//! `#hud` element).
//!
//! The glTF materials already carry the N64 render state the extractor read (wrap/mirror/clamp,
//! point/bilinear filtering, alpha mode, unlit for draws without normals), so Bevy's loader maps
//! them as is. Units are the game's: the map spans about 7,000 x 13,000 units.

use crate::connection::NetStatus;
use crate::{Anim, BodyClips, Config, FollowCamera, body, hud_text};
use bevy::app::AnimationSystems;
use bevy::asset::AssetMetaCheck;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState};
use bevy::color::Srgba;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::Gltf;
use bevy::mesh::VertexAttributeValues;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use conker_anim::id;
use conker_assets::{Slice, character_gltf, level_gltf, props_gltf};
use conker_rules::gas;
use conker_sim::net::{Combat, MatchView, Player};
use core::f32::consts::{FRAC_PI_2, TAU};

/// The level's scene and a side's character glTF (asset paths in the pack).
pub fn level_scene(map: &str) -> String {
    format!("{}#Scene0", level_gltf(map))
}

/// War: Total War's props (its map's `props.gltf`): the shelter's doors, the gas masks and canisters.
pub fn props_scene(map: &str) -> String {
    format!("{}#Scene0", props_gltf(map))
}

/// Spawn record whose camera the window shows until our player arrives (V: oracle player 1's).
pub const FIRST_SPAWN: usize = 1;
/// The game draws players at half their model units (V: obj+0x14C/+0x150 = 0.5, anim_README).
pub const MODEL_SCALE: f32 = 0.5;
/// The character's root joint; its z travel in the creep clip is root motion (see [`pin_creep_root`]).
const ROOT_JOINT: &str = "joint_00";
/// Toggles the free debug camera (WASD/QE fly, hold the right mouse button to look, Shift = fast).
pub const DEBUG_CAMERA_KEY: KeyCode = KeyCode::KeyC;
/// Sky and fog colour (I: the extractor reads no fog or sky state yet).
const SKY: Color = Color::srgb(0.55, 0.68, 0.85);

/// A game angle (u16, 65,536 per turn) in radians. The game moves along (cos a, −sin a) in
/// (x, z) (V: oracle traces).
fn angle(game: u16) -> f32 {
    game as f32 * TAU / 65536.0
}

/// Yaw about +y that turns the character, whose rest pose faces +z, to a game facing angle.
pub fn model_yaw(facing: u16) -> f32 {
    angle(facing) + FRAC_PI_2
}

/// N64 vertex colours are display values that the RDP multiplies with raw texels; Bevy reads glTF
/// `COLOR_0` as linear and multiplies with linearised texels. Decoding the colour once makes the
/// sRGB output equal texel × shade, as on the console (I: CRT-era art, no gamma pipeline).
pub fn shade_to_linear(color: &mut [f32; 4]) {
    for c in &mut color[..3] {
        *c = Srgba::gamma_function(*c);
    }
}

/// Model units the game draws each body's model above the player's position, scaled with the model
/// (the pack's `model_lift`), by body ([`crate::characters`]): the s16 at +6 of the model's
/// bank-0x11 record (43 for 117, 42 for 128, 49 for 136, 41 and 40 for the villagers 156 and 157),
/// copied to obj+0xC8 at spawn (`func_150837D4`); the object matrix is at (x, y + obj+0x150 ×
/// obj+0xC8, z) (`func_1502DF38`). C, and V: the oracle's joint matrices equal the clips' roots
/// lifted by 43 (idle, run, crouch); obj+0xC8 is 42.0 for a squirrel.
#[derive(Resource)]
struct ModelLift(Vec<f32>);

/// Each body's team looks (the pack's `Character::teams`, the Wise Guy's; None for the others): the
/// glTF material's name (extract-character names material i `material_ii`, C) and each team's
/// texture, by body ([`crate::characters`]).
#[derive(Resource)]
struct TeamLooks(Vec<Option<(String, Vec<String>)>>);

/// The team a player's model is dressed as, and the model.
#[derive(Component)]
struct TeamDressed(u8, Entity);

/// A body's character: its scene and its clips' graph.
struct Look {
    scene: Handle<WorldAsset>,
    graph: Handle<AnimationGraph>,
    /// Animation id → graph node, for the ids in its [`crate::Clips`].
    nodes: Vec<(u16, AnimationNodeIndex)>,
}

impl Look {
    fn node(&self, id: u16) -> Option<AnimationNodeIndex> {
        self.nodes.iter().find(|n| n.0 == id).map(|n| n.1)
    }
}

/// The bodies' characters ([`crate::characters`]), once loaded.
#[derive(Resource)]
struct Characters(Vec<Look>);

impl Characters {
    fn of(&self, body: usize) -> &Look {
        &self.0[body]
    }
}

/// A player entity that has its model: its body ([`crate::body`]) and the model's scene root.
#[derive(Component)]
pub(crate) struct Model {
    pub(crate) body: usize,
    pub(crate) scene: Entity,
}

/// The HUD's text (native).
#[cfg(not(target_family = "wasm"))]
#[derive(Component)]
struct Hud;

/// The posed parts of a player's model: its body, its `AnimationPlayer` and root joint (with its
/// rest z).
#[derive(Component)]
pub(crate) struct Rig {
    body: usize,
    animator: Entity,
    root: Entity,
    root_z: f32,
}

/// Windowed setup: Bevy's default plugins with the pack as the asset folder, the level, the cameras
/// and the player models.
pub fn build(app: &mut App, slice: Slice, cfg: &Config) {
    let offscreen = cfg.offscreen;
    let first = &slice.spawns[FIRST_SPAWN];
    let (eye, look) = match cfg.view {
        Some(v) => (Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5])),
        None => (
            Vec3::from_array(first.camera),
            Vec3::from_array(first.position.map(f32::from)) + Vec3::Y * 67.5,
        ),
    };
    let free = cfg.view.is_some();
    let level = level_scene(&cfg.map);
    let lift = |c: &conker_assets::Character| f32::from(c.model_lift);
    app.insert_resource(ModelLift(slice.characters.iter().map(lift).collect()));
    let bodies = crate::characters(&cfg.map);
    let teams = (slice.characters.iter().zip(bodies)).map(|(c, name)| {
        let t = c.teams.as_ref()?;
        let textures = t.textures.iter().map(|f| format!("characters/{name}/{f}"));
        Some((format!("material_{:02}", t.material), textures.collect()))
    });
    app.insert_resource(TeamLooks(teams.collect()));
    app.add_systems(Update, dress_teams);
    crate::beach::build(app, &slice.characters);
    crate::projectiles::build(app);
    let carried = (slice.lights.lights.iter())
        .find(|l| l.when == conker_assets::When::Carried)
        .cloned();
    crate::lights::build(app, slice.lights, &slice.triggers);
    crate::held::build(app, slice.held, carried);
    crate::flags::build(app);
    crate::guns::build(app);
    // The pack has no `.meta` files: don't ask for one before each file.
    let mut assets = AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..default()
    };
    // Natively the pack's folder (absolute); in the browser `assets/`, relative to the page.
    if let Some(dir) = &cfg.assets {
        assets.file_path = dir.clone();
    }
    let window = WindowPlugin {
        primary_window: Some(Window {
            title: "Conker multiplayer".into(),
            canvas: Some("#game".into()),
            fit_canvas_to_parent: true,
            // Screenshot runs neither take the keyboard (typing elsewhere would fly the free
            // camera) nor hide behind other windows (macOS doesn't draw occluded windows).
            focused: cfg.screenshot.is_none(),
            window_level: match cfg.screenshot {
                Some(_) => bevy::window::WindowLevel::AlwaysOnTop,
                None => default(),
            },
            position: cfg.position.map_or(WindowPosition::Automatic, |[x, y]| {
                WindowPosition::At(IVec2::new(x, y))
            }),
            ..default()
        }),
        ..default()
    };
    if offscreen {
        // No window: the camera draws into an image, ticked like a headless client.
        let tick = core::time::Duration::from_secs_f64(1.0 / conker_sim::TICK_HZ);
        app.add_plugins((
            DefaultPlugins
                .set(crate::log_plugin(cfg))
                .set(assets)
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    close_when_requested: false,
                    ..window
                })
                .disable::<bevy::winit::WinitPlugin>(),
            bevy::app::ScheduleRunnerPlugin::run_loop(tick / 2),
            FreeCameraPlugin,
        ));
    } else {
        app.add_plugins((
            DefaultPlugins
                .set(crate::log_plugin(cfg))
                .set(assets)
                .set(window),
            FreeCameraPlugin,
        ));
    }
    // Keep simulating and talking to the server while unfocused.
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(ClearColor(SKY));
    app.init_resource::<TextHud>();
    app.insert_resource(GlobalAmbientLight {
        brightness: 400.0,
        ..default()
    });
    app.add_systems(
        Startup,
        (
            move |mut commands: Commands,
                  assets: Res<AssetServer>,
                  images: ResMut<Assets<Image>>| {
                let target = offscreen.then(|| offscreen_target(&mut commands, images));
                setup_scene(&mut commands, &assets, &level, eye, look, free, target)
            },
            #[cfg(not(target_family = "wasm"))]
            setup_combat,
        ),
    );
    let ready = resource_exists::<Characters>;
    app.add_systems(
        Update,
        (
            load_characters.run_if(not(ready)),
            shade_vertex_colors,
            toggle_debug_camera,
            rebuild_models,
            add_player_models.run_if(ready),
            place_players,
            pose_players.run_if(ready),
            follow_camera,
            crate::outro::show,
            crate::beach::camera,
            crate::projectiles::fly,
            crate::projectiles::flames,
            add_props.run_if(ready),
            crate::outro::add_actors.run_if(ready),
            crate::flags::add.run_if(ready),
            crate::guns::add.run_if(ready),
            show_pickups,
            toggle_text_hud,
            update_hud,
            log_loaded,
        )
            .chain(),
    );
    app.add_systems(
        PostUpdate,
        pin_creep_root
            .after(AnimationSystems)
            .before(TransformSystems::Propagate),
    );
}

/// The bodies' characters, one after the other once the level has loaded: each glTF, then its
/// scene and clips from the loaded asset, each clip found by its animation's name (its body's
/// [`crate::Clips`]). A glTF load polls other pending IO tasks from inside its texture tasks on
/// the same thread, so a second load of a GLB (the level, or the character again through hundreds
/// of labelled clip requests) nested inside it and overflowed the IO thread's stack in debug builds
/// (V: lldb, `load_gltf` within `execute_global_scope<ImageOrPath>`, with the GLBs' embedded
/// images).
#[allow(clippy::too_many_arguments)]
fn load_characters(
    mut commands: Commands,
    assets: Res<AssetServer>,
    clips: Res<BodyClips>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut animations: ResMut<Assets<AnimationClip>>,
    mut loads: Local<Vec<Handle<Gltf>>>,
    mut looks: Local<Vec<Look>>,
    cfg: Res<Config>,
) {
    let level = assets.get_handle_untyped(level_scene(&cfg.map));
    if !level.is_some_and(|h| assets.is_loaded_with_dependencies(&h)) {
        return;
    }
    let bodies = crate::characters(&cfg.map);
    let (body, path) = (looks.len(), character_gltf(bodies[looks.len()]));
    if loads.len() == looks.len() {
        loads.push(assets.load(&path));
    }
    let handle = &loads[looks.len()];
    let Some(gltf) = gltfs
        .get(handle)
        .filter(|_| assets.is_loaded_with_dependencies(handle))
    else {
        return;
    };
    let mut ids = Vec::new();
    let mut handles: Vec<Handle<AnimationClip>> = Vec::new();
    for (id, name, ticks) in &clips.of(body).0 {
        let Some(handle) = gltf.named_animations.get(name.as_str()) else {
            warn!("{path}: no animation {name} (animation id {id:#x})");
            continue;
        };
        // The clocks loop on the game's duration; the glTF clips end a tick earlier.
        // ponytail: the last tick holds the last key instead of blending back to the first.
        if let Some(mut clip) = animations.get_mut(handle) {
            clip.set_duration(f32::from(*ticks) / conker_sim::TICK_HZ as f32);
        }
        ids.push(*id);
        handles.push(handle.clone());
    }
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    looks.push(Look {
        scene: gltf.scenes[0].clone(),
        graph: graphs.add(graph),
        nodes: ids.into_iter().zip(nodes).collect(),
    });
    if looks.len() == bodies.len() {
        commands.insert_resource(Characters(std::mem::take(&mut *looks)));
    }
}

/// The image an `offscreen` client draws into (the size of the windows), and its screenshots' source.
#[derive(Resource, Clone)]
pub struct Offscreen(pub Handle<Image>);

fn offscreen_target(commands: &mut Commands, mut images: ResMut<Assets<Image>>) -> Handle<Image> {
    let format = bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb;
    let mut image = Image::new_target_texture(1280, 720, format, None);
    image.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::COPY_SRC;
    let handle = images.add(image);
    commands.insert_resource(Offscreen(handle.clone()));
    handle
}

fn setup_scene(
    commands: &mut Commands,
    assets: &AssetServer,
    level: &str,
    eye: Vec3,
    look: Vec3,
    free: bool,
    target: Option<Handle<Image>>,
) {
    commands.spawn((
        crate::lights::Lit,
        WorldAssetRoot(assets.load(level.to_string())),
    ));
    crate::hud::camera(commands, target.clone());
    // Only the character and the 27 level materials with normals are lit (I: direction and levels).
    commands.spawn((
        DirectionalLight {
            illuminance: 2500.0,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let mut camera = commands.spawn((
        Camera3d::default(),
        // ponytail: no tonemapping LUTs in the build (wasm size); N64 colours need none anyway.
        Tonemapping::None,
        // The view's vertical angle ([`fov`]); I: near/far for a ~70-unit character on a
        // ~13,000-unit map.
        Projection::Perspective(PerspectiveProjection {
            fov: conker_camera::VIEW_ANGLES[1].to_radians(),
            near: 4.0,
            far: 40_000.0,
            ..default()
        }),
        DistanceFog {
            color: SKY,
            falloff: FogFalloff::Linear {
                start: 8_000.0,
                end: 25_000.0,
            },
            ..default()
        },
        FreeCamera {
            walk_speed: 1500.0,
            run_speed: 6000.0,
            ..default()
        },
        Transform::from_translation(eye).looking_at(look, Vec3::Y),
    ));
    if let Some(image) = target {
        camera.insert(bevy::camera::RenderTarget::Image(image.into()));
        // The HUD draws into it too (natively: the wasm build has no UI).
        #[cfg(not(target_family = "wasm"))]
        camera.insert(IsDefaultUiCamera);
    }
    // The free camera starts on only with `view=`.
    camera.queue(move |mut entity: EntityWorldMut| {
        if let Some(mut state) = entity.get_mut::<FreeCameraState>() {
            state.enabled = free;
        }
    });
}

/// The HUD (native).
#[cfg(not(target_family = "wasm"))]
fn setup_combat(mut commands: Commands) {
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            padding: UiRect::all(Val::Px(6.0)),
            ..default()
        },
    ));
}

/// See [`shade_to_linear`]. Only meshes from the pack's glTF models exist besides the
/// projectiles' spheres (which have no vertex colours), so every new mesh is converted.
fn shade_vertex_colors(
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event
            && let Some(mut mesh) = meshes.get_mut(*id)
            && let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        {
            colors.iter_mut().for_each(shade_to_linear);
        }
    }
}

fn toggle_debug_camera(
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<(&Transform, &mut FreeCameraState)>,
) {
    if !keys.just_pressed(DEBUG_CAMERA_KEY) {
        return;
    }
    let (transform, mut state) = camera.into_inner();
    state.enabled = !state.enabled;
    // Start flying from where the follow camera is, not where the free camera was left.
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    (state.yaw, state.pitch, state.velocity) = (yaw, pitch, Vec3::ZERO);
    info!("debug camera {}", if state.enabled { "on" } else { "off" });
}

/// Players get their animation clock in the fixed tick ([`crate::Anim`]); give each its body's
/// model ([`crate::body`]), lifted by its own model lift.
#[allow(clippy::type_complexity)]
fn add_player_models(
    players: Query<(Entity, &Combat), (With<Anim>, Without<Model>)>,
    characters: Res<Characters>,
    lift: Res<ModelLift>,
    mut commands: Commands,
) {
    for (entity, combat) in &players {
        let body = body(combat);
        let scene = commands
            .spawn((
                WorldAssetRoot(characters.of(body).scene.clone()),
                Transform::from_xyz(0.0, MODEL_SCALE * lift.0[body], 0.0)
                    .with_scale(Vec3::splat(MODEL_SCALE)),
                ChildOf(entity),
            ))
            .observe(rig_ready)
            .id();
        commands.entity(entity).insert((
            Transform::default(),
            Visibility::default(),
            Model { body, scene },
        ));
    }
}

/// A player whose body changed (War: Beach's villagers re-roll their class, and so maybe their
/// model, at each respawn) loses its model, rig and held models: [`add_player_models`] gives it
/// the new one.
fn rebuild_models(players: Query<(Entity, &Combat, &Model)>, mut commands: Commands) {
    for (entity, combat, model) in &players {
        if body(combat) != model.body {
            commands.entity(model.scene).despawn();
            commands
                .entity(entity)
                .remove::<(Model, Rig, crate::held::Hands)>();
        }
    }
}

/// Heist's team looks ([`TeamLooks`]): a Wise Guy's model draws team `team & 3`'s texture (REDS,
/// BLUES, GREENS, YELLOWS: the shirt) in the team material, on its own copy of it (V runs tm4, tm8:
/// the game binds segment 0x0A per team and changes nothing else, `../lab/oracle/map1a/teams/`; V
/// slots 0-3 in Deathmatch too, where the server's teams are the join order).
#[allow(clippy::type_complexity)]
fn dress_teams(
    looks: Res<TeamLooks>,
    players: Query<(Entity, &Combat, &Model, Option<&TeamDressed>), With<Rig>>,
    children: Query<&Children>,
    mut parts: Query<(
        &bevy::gltf::GltfMaterialName,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, c, model, dressed) in &players {
        let team = c.team & 3;
        let Some((name, texture)) = (looks.0.get(model.body).and_then(Option::as_ref))
            .and_then(|(name, t)| Some((name, t.get(usize::from(team))?)))
        else {
            continue;
        };
        if dressed.is_some_and(|d| d.0 == team && d.1 == model.scene) {
            continue;
        }
        let texture: Handle<Image> = assets.load(texture);
        for e in children.iter_descendants(model.scene) {
            if let Ok((n, mut m)) = parts.get_mut(e)
                && n.0 == *name
            {
                let mut own = materials.get(&m.0).cloned().unwrap_or_default();
                own.base_color_texture = Some(texture.clone());
                m.0 = materials.add(own);
            }
        }
        commands
            .entity(entity)
            .insert(TeamDressed(team, model.scene));
    }
}

/// The model's scene is in: hook its `AnimationPlayer` to its body's clips and find the root joint.
#[allow(clippy::too_many_arguments)]
fn rig_ready(
    ready: On<WorldInstanceReady>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    joints: Query<(&Name, &Transform)>,
    models: Query<&Model>,
    characters: Res<Characters>,
    mut commands: Commands,
) {
    let Ok(player) = parents.get(ready.entity).map(ChildOf::parent) else {
        return;
    };
    // A model replaced before its scene came in.
    let Some(body) = (models.get(player).ok())
        .filter(|m| m.scene == ready.entity)
        .map(|m| m.body)
    else {
        return;
    };
    let mut animator = None;
    let mut root = None;
    for entity in children.iter_descendants(ready.entity) {
        if animators.contains(entity) {
            animator = Some(entity);
        }
        if let Ok((name, transform)) = joints.get(entity)
            && name.as_str() == ROOT_JOINT
        {
            root = Some((entity, transform.translation.z));
        }
    }
    let (Some(animator), Some((root, root_z))) = (animator, root) else {
        warn!("character model without an AnimationPlayer or {ROOT_JOINT}");
        return;
    };
    commands
        .entity(animator)
        .insert(AnimationGraphHandle(characters.of(body).graph.clone()));
    commands.entity(player).insert(Rig {
        body,
        animator,
        root,
        root_z,
    });
}

/// Each player where its state is; the local player where and as its correction draws it
/// ([`crate::smooth::Drawn`]).
// ponytail: players move in 30 Hz steps; add lightyear frame interpolation for smooth rendering
// above 30 fps.
fn place_players(
    mut players: Query<(&Player, Option<&FollowCamera>, &mut Transform), With<Model>>,
) {
    for (p, camera, mut transform) in &mut players {
        let (pos, facing) = camera.map_or((p.pos, p.facing), |c| {
            (c.drawn.body(p.pos), c.drawn.facing(p.facing))
        });
        transform.translation = Vec3::from_array(pos);
        transform.rotation = Quat::from_rotation_y(model_yaw(facing));
    }
}

/// Shows each player's clock: its clip at its time, cross-faded with the previous clip.
fn pose_players(
    players: Query<(&Anim, &Rig)>,
    mut animators: Query<&mut AnimationPlayer>,
    characters: Res<Characters>,
) {
    for (anim, rig) in &players {
        let Ok(mut animator) = animators.get_mut(rig.animator) else {
            continue;
        };
        let character = characters.of(rig.body);
        let a = &anim.0;
        let w = a.prev_weight();
        let current = character.node(a.id);
        let prev = character.node(a.prev).filter(|_| w > 0.0 && a.prev != a.id);
        let stale: Vec<AnimationNodeIndex> = animator
            .playing_animations()
            .map(|(node, _)| *node)
            .filter(|node| Some(*node) != current && Some(*node) != prev)
            .collect();
        for node in stale {
            animator.stop(node);
        }
        for (node, time, weight) in [(current, a.time, 1.0 - w), (prev, a.prev_time, w)] {
            if let Some(node) = node {
                animator
                    .play(node)
                    .set_speed(0.0)
                    .set_seek_time(time / conker_sim::TICK_HZ as f32)
                    .set_weight(weight);
            }
        }
    }
}

/// The creep clip carries its walk in the root's z (143 model units); the game turns that into
/// the player's motion instead of drawing it (V: conker-anim), so the root stays at rest here.
fn pin_creep_root(players: Query<(&Anim, &Rig)>, mut joints: Query<&mut Transform>) {
    for (anim, rig) in &players {
        if (anim.0.id == id::CREEP || anim.0.prev == id::CREEP)
            && let Ok(mut root) = joints.get_mut(rig.root)
        {
            root.translation.z = rig.root_z;
        }
    }
}

/// The view is the follow camera's: from its view eye to its view look-at point (y up, game units;
/// the aim camera's lift included); after a correction, the drawn body's camera's
/// ([`crate::smooth::Drawn::view`]). Through a scope its field of view narrows by the zoom
/// ([`fov`]).
#[allow(clippy::type_complexity)]
fn follow_camera(
    player: Query<&FollowCamera>,
    camera: Single<
        (&mut Transform, &mut Projection, &FreeCameraState),
        (Without<Model>, Without<crate::projectiles::Projectile>),
    >,
) {
    let (mut camera, mut projection, free) = camera.into_inner();
    if let (false, Ok(follow)) = (free.enabled, player.single()) {
        let (eye, at) = follow.drawn.view(&follow.follow);
        *camera = Transform::from_translation(eye).looking_at(at, Vec3::Y);
        if let Projection::Perspective(p) = &mut *projection {
            let fov = fov(&follow.follow.cam.scope).to_radians();
            if p.fov != fov {
                p.fov = fov;
            }
        }
    }
}

/// The view's vertical field of view in degrees: the view's angle, plus the zoom through a scope.
/// Every scope's view (`func_15126378` → `func_1510B128`) is a perspective of the view's angles
/// (`+0x6C`, `+0x70`) plus the zoom (`cam+0x19C`, `+0x1A0`), and leaving it zeroes the zoom
/// (`func_15127EB8`) (C; V gcA's projections: 82.42° × 50° unzoomed, 56.04° × 34° at the gun's
/// zoom −26.37, −16).
pub fn fov(scope: &conker_camera::Scope) -> f32 {
    let zoom = if scope.on { scope.zoom[1] } else { 0.0 };
    conker_camera::VIEW_ANGLES[1] + zoom
}

/// In War: Total War, its props, once the characters are in (one glTF load at a time, see
/// [`load_characters`]).
fn add_props(
    game: Query<&MatchView>,
    assets: Res<AssetServer>,
    cfg: Res<Config>,
    mut added: Local<bool>,
    mut commands: Commands,
) {
    if !*added && game.single().is_ok_and(|m| m.total_war) {
        *added = true;
        let props = WorldAssetRoot(assets.load(props_scene(&cfg.map)));
        commands.spawn((crate::lights::Lit, props));
    }
}

/// The level glTF draws every placement, pickups included, as a node named `model@placement`, and
/// so do War: Total War's props: a taken pickup's node shrinks away ([`shrink`]) and hides until it
/// is back, a roaming one (0x1A's, [`MatchView::roaming`]) standing where it is now; in Total War,
/// a prop the gas drives ([`gas::PROPS`]) shows while [`MatchView::props`] says so (a canister
/// taken shrinks too, the doors and covers slide by [`MatchView::lifts`] and just go).
#[allow(clippy::type_complexity)]
fn show_pickups(
    game: Query<&MatchView>,
    timeline: Res<lightyear::prelude::LocalTimeline>,
    names: Query<(Entity, &Name), Added<Name>>,
    mut nodes: Local<Vec<(u32, Entity, Option<(Vec3, f32)>, Option<u32>)>>,
    mut shown: Query<(&mut Visibility, &mut Transform)>,
) {
    nodes.extend(
        (names.iter())
            .filter_map(|(e, name)| Some((name.rsplit_once('@')?.1.parse().ok()?, e, None, None))),
    );
    let Ok(m) = game.single() else {
        return;
    };
    let tick = timeline.tick().0;
    for (placement, entity, base, at) in nodes.iter_mut() {
        let Ok((mut v, mut t)) = shown.get_mut(*entity) else {
            continue;
        };
        let taken = *placement < 64 && m.taken >> *placement & 1 != 0;
        let driven = m.total_war && gas::PROPS.iter().any(|p| u32::from(p.0) == *placement);
        let gone = driven && m.props >> *placement & 1 == 0;
        let canister = gas::CANISTERS.iter().any(|c| u32::from(c.0) == *placement);
        // Hidden since tick `at`; one already hidden at first sight, or a door, just hides.
        let first = base.is_none();
        let (full, y) = *base.get_or_insert((t.scale, t.translation.y));
        let lift = match *placement {
            n @ 0..8 if driven => f32::from(m.lifts[n as usize]),
            _ => 0.0,
        };
        // A roaming one stands where the match has it (a pickup position, as the node's own).
        let roams = m.roaming.iter().find(|r| u32::from(r.0) == *placement);
        let to = roams.map_or(t.translation.with_y(y + lift), |r| Vec3::from(r.1));
        if t.translation != to {
            if roams.is_some() {
                debug!("t={tick} pickup on placement {placement} at {to}");
            }
            t.translation = to;
        }
        *at = match (taken || gone, *at) {
            (false, _) => None,
            (true, None) if first || !(taken || canister) => Some(tick.wrapping_sub(3)),
            (true, None) => Some(tick),
            (true, at) => at,
        };
        let scale = at.map_or(Some(1.0), |a| shrink(tick.wrapping_sub(a)));
        v.set_if_neq(match scale {
            Some(_) => Visibility::Inherited,
            None => Visibility::Hidden,
        });
        let scale = full * scale.unwrap_or(1.0);
        if t.scale != scale {
            t.scale = scale;
        }
    }
}

/// A pickup's scale `k` ticks after it is taken (`func_1511DF6C` state 3: −0.26 of its size a
/// frame; V d1 713-716), None once it is gone.
pub fn shrink(k: u32) -> Option<f32> {
    (k < 3).then(|| 1.0 - 0.26 * (k + 1) as f32)
}

/// Shows the old text readout ([`hud_text`]) over the HUD ([`crate::hud`]), for debugging.
pub const TEXT_HUD_KEY: KeyCode = KeyCode::KeyH;

/// Whether the text readout is on ([`TEXT_HUD_KEY`] toggles it).
#[derive(Resource, Default)]
struct TextHud(bool);

fn toggle_text_hud(keys: Res<ButtonInput<KeyCode>>, mut on: ResMut<TextHud>) {
    if keys.just_pressed(TEXT_HUD_KEY) {
        on.0 = !on.0;
    }
}

/// While connected, the local player's numbers and the clock as text if [`TextHud`] is on: a text
/// node natively, the page's `#hud` element in the browser (the wasm build has no UI or font
/// renderer). Otherwise, natively, where the connection stands ([`NetStatus`]: loading,
/// connecting, failed or lost, with the reason).
#[allow(clippy::too_many_arguments)]
fn update_hud(
    player: Query<&Combat, With<FollowCamera>>,
    game: Query<&MatchView>,
    status: Res<NetStatus>,
    cfg: Res<Config>,
    on: Res<TextHud>,
    #[cfg(not(target_family = "wasm"))] mut hud: Single<(&mut Text, &mut Visibility), With<Hud>>,
    #[cfg(target_family = "wasm")] mut shown: Local<String>,
) {
    let text = match (player.single(), game.single()) {
        (Ok(c), Ok(m)) if *status == NetStatus::Connected && on.0 => hud_text(c, m),
        _ if *status == NetStatus::Connected => String::new(),
        // The page's `#net` line says where the connection stands (a failure too).
        _ if cfg!(target_family = "wasm") => String::new(),
        _ => status.text(&cfg).0,
    };
    #[cfg(not(target_family = "wasm"))]
    {
        let (text_node, visibility) = &mut *hud;
        visibility.set_if_neq(match text.is_empty() {
            true => Visibility::Hidden,
            false => Visibility::Inherited,
        });
        if text_node.0 != text {
            text_node.0 = text;
        }
    }
    #[cfg(target_family = "wasm")]
    if *shown != text {
        let hud = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("hud"));
        if let Some(hud) = hud {
            hud.set_text_content(Some(&text));
        }
        *shown = text;
    }
}

/// Logs when the level and the characters have finished loading (load time on native and in the
/// browser).
fn log_loaded(
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    mut pending: Local<Option<Vec<String>>>,
    cfg: Res<Config>,
) {
    let pending = pending.get_or_insert_with(|| {
        let characters = crate::characters(&cfg.map).into_iter();
        let characters = characters.map(|c| format!("{}#Scene0", character_gltf(c)));
        std::iter::once(level_scene(&cfg.map))
            .chain(characters)
            .collect()
    });
    pending.retain(|path| {
        let done = assets
            .get_handle_untyped(path)
            .is_some_and(|h| assets.is_loaded_with_dependencies(&h));
        if done {
            let secs = time.startup().elapsed().as_secs_f32();
            info!("{path} loaded {secs:.2} s after the app started");
        }
        !done
    });
}

/// Saves the window (or the [`Offscreen`] image) to `path` (native: a file; browser: a download).
pub fn screenshot(commands: &mut Commands, path: String) {
    commands.queue(move |world: &mut World| {
        let shot = shot(world);
        world.spawn(shot).observe(save_to_disk(path));
    });
}

/// [`screenshot`], then exits.
pub fn screenshot_then_exit(commands: &mut Commands, path: String) {
    commands.queue(move |world: &mut World| {
        let shot = shot(world);
        world.spawn(shot).observe(save_to_disk(path)).observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    });
}

fn shot(world: &World) -> Screenshot {
    world
        .get_resource::<Offscreen>()
        .map_or_else(Screenshot::primary_window, |o| {
            Screenshot::image(o.0.clone())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pickup taken shrinks by 0.26 of its size a frame, then hides (V d1 713-716).
    #[test]
    fn a_taken_pickup_shrinks_over_3_frames() {
        let scales = [0, 1, 2, 3, 9].map(shrink);
        let want = [Some(0.74), Some(0.48), Some(0.22), None, None];
        for (s, w) in scales.iter().zip(want) {
            assert!(
                s.zip(w).map_or(s == &w, |(s, w)| (s - w).abs() < 1e-6),
                "{scales:?}"
            );
        }
    }

    #[test]
    fn model_yaw_turns_plus_z_to_the_game_direction() {
        for facing in [0u16, 16384, 16896, 40000] {
            let forward = Quat::from_rotation_y(model_yaw(facing)) * Vec3::Z;
            let a = angle(facing);
            assert!(
                forward.abs_diff_eq(Vec3::new(a.cos(), 0.0, -a.sin()), 1e-5),
                "{facing}"
            );
        }
    }

    /// gcA's projections (`lab/research/mp-original/mounted-gun-scratch/gc`): cot(fovy/2) 2.1445
    /// unzoomed, 3.2707 at the gun's zoom (−26.373, −16.0) 16 C-up frames in: 50° and 34°.
    #[test]
    fn a_scope_narrows_the_view_by_its_zoom() {
        let mut scope = conker_camera::Scope {
            on: true,
            zoom: [-26.373, -26.373 * 50.0 / conker_camera::VIEW_ANGLES[0]],
            ..default()
        };
        let cot = |deg: f32| 1.0 / (deg / 2.0).to_radians().tan();
        assert!((cot(fov(&scope)) - 3.2707).abs() < 1e-3, "{}", fov(&scope));
        scope.on = false;
        assert!((cot(fov(&scope)) - 2.1445).abs() < 1e-3);
    }

    #[test]
    fn shade_decodes_rgb_only() {
        let mut c = [1.0, 0.5, 0.0, 0.5];
        shade_to_linear(&mut c);
        assert_eq!([c[0], c[2], c[3]], [1.0, 0.0, 0.5]);
        // Re-encoded by the sRGB surface, an untextured unlit draw shows the raw N64 value.
        assert!((Srgba::gamma_function_inverse(c[1]) - 0.5).abs() < 1e-6);
        assert!((c[1] - 0.214).abs() < 1e-3);
    }
}
