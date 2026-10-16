//! The diagnostics panel, top right: the Diagnostics action (the key under Esc, ` on QWERTY, ² on
//! AZERTY; not while the Esc menu holds the input) shows and hides it. Its text refreshes 4 times a
//! second, not every frame (the page's DOM would churn): the frame rate and times, the frames'
//! work, the process or page memory, the connection, the match and the renderer. In the browser it
//! is the page's `#diag` element (the wasm build has no UI or font renderer; it shows over the
//! loading screen too), natively a text node in Bevy's default font (ASCII only).
//!
//! Work: the main world's update (its `First` to this panel in `Last`) and the render world's
//! `Render` schedule, each in ms a frame and as a share of the frame time. The browser has no CPU
//! figure for a page: both run one after the other on its one thread, so their sum is how busy that
//! thread is (the rest waits for the next animation frame). Natively the render runs on its own
//! thread while the next update runs (Bevy's pipelined rendering), so the larger of the two bounds
//! the frame. With vsync, that render also waits for the next swapchain image (`prepare_windows`'
//! acquire), so it tracks the frame more than the work (WebGL2's acquire doesn't wait): natively,
//! the process's CPU (all its threads, as a share of every core) is the work's figure; it and the
//! resident memory are sysinfo's (Bevy's `SystemInformationDiagnosticsPlugin`, refreshed in the
//! background about every 200 ms). In the browser, memory is the wasm heap (its linear memory,
//! which never shrinks) and, in Chrome only, the page's JS heap (`performance.memory`).

use crate::Config;
use crate::bindings::{Action, Controls};
use crate::connection::NetStatus;
use bevy::ecs::system::SystemParam;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use conker_sim::net::PlayerId;
use lightyear::prediction::diagnostics::PredictionMetrics;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// How often the text refreshes (s).
const REFRESH: f64 = 0.25;

/// The render world's work since the panel last read it (ns; natively on the render thread).
static RENDER_NS: AtomicU64 = AtomicU64::new(0);

/// When this world's frame work began (the main and the render world each have theirs).
#[derive(Resource)]
struct Began(Instant);

/// Whether the panel shows, the frames since its last refresh and the rollbacks' count then.
#[derive(Resource, Default)]
struct Diag {
    on: bool,
    frames: Frames,
    rollbacks: u32,
}

/// Frames: their count, and in seconds their total and worst time and the work in them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Frames {
    n: u32,
    time: f64,
    worst: f64,
    update: f64,
    render: f64,
}

impl Frames {
    fn add(&mut self, dt: f64, update: f64) {
        self.n += 1;
        self.time += dt;
        self.worst = self.worst.max(dt);
        self.update += update;
    }

    /// The `fps` and `work` lines (at least one frame).
    fn lines(&self) -> String {
        let n = f64::from(self.n);
        let ms = |s: f64| s / n * 1e3;
        let share = |s: f64| s / self.time * 1e2;
        format!(
            "fps      {:.0}, {:.1} ms a frame, worst {:.1}\n\
             work     update {:.1} ms {:.0}%, render {:.1} ms {:.0}%",
            n / self.time,
            ms(self.time),
            self.worst * 1e3,
            ms(self.update),
            share(self.update),
            ms(self.render),
            share(self.render),
        )
    }
}

/// The panel's text node (native).
#[cfg(not(target_family = "wasm"))]
#[derive(Component)]
struct Panel;

/// What the panel reads besides the frames and the memory.
#[derive(SystemParam)]
struct Sources<'w, 's> {
    cfg: Res<'w, Config>,
    status: Res<'w, NetStatus>,
    pings: Query<'w, 's, &'static PingManager, With<Client>>,
    interpolation: Res<'w, InterpolationConfig>,
    timeline: Res<'w, LocalTimeline>,
    metrics: Option<Res<'w, PredictionMetrics>>,
    players: Query<'w, 's, (), With<PlayerId>>,
    entities: Query<'w, 's, ()>,
    window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    adapter: Option<Res<'w, RenderAdapterInfo>>,
}

impl Sources<'_, '_> {
    fn rollbacks(&self) -> u32 {
        self.metrics.as_ref().map_or(0, |m| m.rollbacks)
    }

    /// The connection's, the match's and the renderer's lines; `rate`: rollbacks a second.
    fn lines(&self, rate: f64) -> String {
        let cfg = &self.cfg;
        // Its first sentence: a failure goes on with a long hint.
        let status = self.status.text(cfg).0;
        let status = status.split(". ").next().unwrap_or_default();
        let ping = self.pings.single().map_or("-".into(), |p| {
            let [rtt, jitter] = [p.rtt(), p.jitter()].map(|d| d.as_millis());
            format!("rtt {rtt} ms, jitter {jitter} ms")
        });
        let window = self.window.single().map_or("-".into(), |w| {
            let (x, y) = (w.physical_width(), w.physical_height());
            format!("{x}x{y} at scale {}", w.scale_factor())
        });
        let gpu =
            (self.adapter.as_ref()).map_or("-".into(), |a| format!("{} ({:?})", a.name, a.backend));
        format!(
            "net      {status}\n\
             client   id {}, tick {}, rollbacks {} ({rate:.1}/s)\n\
             ping     {ping}, interpolation >= {} ms\n\
             match    map {}, {} players, {} entities\n\
             window   {window}\n\
             gpu      {gpu}",
            cfg.id,
            self.timeline.tick().0,
            self.rollbacks(),
            self.interpolation.min_delay.as_millis(),
            cfg.map,
            self.players.count(),
            self.entities.count(),
        )
    }
}

/// The process's CPU and resident memory (sysinfo's; none until its first refresh).
#[cfg(not(target_family = "wasm"))]
fn memory(store: &bevy::diagnostic::DiagnosticsStore) -> String {
    use bevy::diagnostic::SystemInformationDiagnosticsPlugin as Sys;
    let value = |p| store.get(&p).and_then(|d| d.value());
    match (value(Sys::PROCESS_CPU_USAGE), value(Sys::PROCESS_MEM_USAGE)) {
        (Some(cpu), Some(gib)) => {
            format!(
                "process  cpu {cpu:.0}% of all cores, resident {:.0} MiB",
                gib * 1024.0
            )
        }
        _ => "process  -".into(),
    }
}

/// The wasm heap and, in Chrome, the JS heap (used / allocated).
#[cfg(target_family = "wasm")]
fn memory() -> String {
    use js_sys::{ArrayBuffer, Reflect, WebAssembly};
    use wasm_bindgen::{JsCast, JsValue};
    let mib = |bytes: f64| bytes / 1048576.0;
    let buffer = wasm_bindgen::memory()
        .unchecked_into::<WebAssembly::Memory>()
        .buffer();
    let wasm = buffer.unchecked_into::<ArrayBuffer>().byte_length() as f64;
    let mut out = format!("memory   wasm {:.0} MiB", mib(wasm));
    let get = |v: &JsValue, key: &str| Reflect::get(v, &key.into()).ok();
    let js =
        (get(&js_sys::global(), "performance").and_then(|p| get(&p, "memory"))).and_then(|m| {
            Some((
                get(&m, "usedJSHeapSize")?.as_f64()?,
                get(&m, "totalJSHeapSize")?.as_f64()?,
            ))
        });
    if let Some((used, total)) = js {
        out += &format!(", JS heap {:.0} / {:.0} MiB", mib(used), mib(total));
    }
    out
}

fn begin(mut began: ResMut<Began>) {
    began.0 = Instant::now();
}

fn end_render(began: Res<Began>) {
    RENDER_NS.fetch_add(began.0.elapsed().as_nanos() as u64, Ordering::Relaxed);
}

/// The Diagnostics action toggles the panel; while it shows, each frame counts and every
/// [`REFRESH`] its text is rewritten (natively an empty text hides the node, as `#diag:empty`
/// does). While hidden, the counts start over.
fn panel(
    controls: Controls,
    time: Res<Time<Real>>,
    began: Res<Began>,
    mut diag: ResMut<Diag>,
    sources: Sources,
    #[cfg(not(target_family = "wasm"))] store: Res<bevy::diagnostic::DiagnosticsStore>,
    #[cfg(not(target_family = "wasm"))] mut node: Single<(&mut Text, &mut Visibility), With<Panel>>,
) {
    let toggled = controls.just_pressed(Action::Diagnostics);
    diag.on ^= toggled;
    let rollbacks = sources.rollbacks();
    let text = if diag.on {
        diag.frames
            .add(time.delta_secs_f64(), began.0.elapsed().as_secs_f64());
        if diag.frames.time < REFRESH {
            return;
        }
        diag.frames.render = RENDER_NS.swap(0, Ordering::Relaxed) as f64 * 1e-9;
        let rate = f64::from(rollbacks.saturating_sub(diag.rollbacks)) / diag.frames.time;
        #[cfg(not(target_family = "wasm"))]
        let memory = memory(&store);
        #[cfg(target_family = "wasm")]
        let memory = memory();
        let text = format!("{}\n{memory}\n{}", diag.frames.lines(), sources.lines(rate));
        (diag.frames, diag.rollbacks) = (Frames::default(), rollbacks);
        text
    } else {
        *diag = Diag {
            rollbacks,
            ..default()
        };
        RENDER_NS.store(0, Ordering::Relaxed);
        if !toggled {
            return;
        }
        String::new()
    };
    #[cfg(not(target_family = "wasm"))]
    {
        let (node, visibility) = &mut *node;
        visibility.set_if_neq(match text.is_empty() {
            true => Visibility::Hidden,
            false => Visibility::Inherited,
        });
        node.0 = text;
    }
    #[cfg(target_family = "wasm")]
    {
        let diag = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("diag"));
        if let Some(diag) = diag {
            diag.set_text_content(Some(&text));
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        Panel,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            padding: UiRect::all(Val::Px(6.0)),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// The panel (windowed clients: after the render plugins, which bring the render world).
pub fn build(app: &mut App) {
    app.init_resource::<Diag>();
    app.insert_resource(Began(Instant::now()));
    app.add_systems(First, begin);
    app.add_systems(Last, panel);
    #[cfg(not(target_family = "wasm"))]
    {
        app.add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin);
        app.add_systems(Startup, spawn_panel);
    }
    let render = app.sub_app_mut(RenderApp);
    render.insert_resource(Began(Instant::now()));
    render.add_systems(
        Render,
        (
            begin.in_set(RenderSystems::ExtractCommands),
            end_render.in_set(RenderSystems::PostCleanup),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_read_as_rate_times_and_shares() {
        let mut f = Frames::default();
        f.add(0.010, 0.002);
        f.add(0.030, 0.004);
        f.render = 0.010;
        assert_eq!(
            f.lines(),
            "fps      50, 20.0 ms a frame, worst 30.0\n\
             work     update 3.0 ms 15%, render 5.0 ms 25%"
        );
    }
}
