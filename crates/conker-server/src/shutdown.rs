//! Leaving cleanly: Ctrl-C, SIGTERM (systemd's stop, the web app's kill), SIGHUP and `--seconds`
//! first stop the netcode server, which sends every client its disconnect, and exit
//! [`LEAVE_DELAY`] later, once the packets have gone out: the clients see the server go at once,
//! not on their timeout (a WebTransport session has no close of its own when a process dies).

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

/// Seconds the server keeps running after its stop, so the disconnects go out.
const LEAVE_DELAY: f64 = 0.2;

pub fn build(app: &mut App) {
    // Bevy's Ctrl-C handler, set here with SIGTERM and SIGHUP (ctrlc's `termination`), and even
    // when the shell started the server ignoring SIGINT (a script's background job), where Bevy's
    // own plugin stands down.
    use bevy::app::TerminalCtrlCHandlerPlugin as CtrlC;
    if let Err(e) = ctrlc::set_handler(CtrlC::gracefully_exit) {
        warn!("no Ctrl-C/SIGTERM handler: {e}");
    }
    app.add_systems(Update, CtrlC::exit_on_flag);
    // Before this frame's sends: `Stop` writes the disconnects into each link, and the next
    // frame's receive unlinks the server (its transports close).
    app.add_systems(PostUpdate, stop_first.before(ConnectionSystems::Send));
}

/// Holds the first exit request: stops the server, and lets the exit through [`LEAVE_DELAY`] later.
fn stop_first(
    mut exits: ResMut<Messages<AppExit>>,
    time: Res<Time<Real>>,
    server: Single<Entity, With<Server>>,
    mut leaving: Local<Option<(f64, AppExit)>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    if let Some((at, exit)) = leaving.clone() {
        exits.clear();
        if now >= at {
            exits.write(exit);
        }
        return;
    }
    let Some(exit) = exits.drain().last() else {
        return;
    };
    info!("stopping: disconnecting every client first");
    commands.trigger(Stop { entity: *server });
    *leaving = Some((now + LEAVE_DELAY, exit));
}
