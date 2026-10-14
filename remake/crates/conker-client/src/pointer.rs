//! The mouse pointer (CONTROLS_RESEARCH.md §6, §8). In the browser a press on the canvas takes the
//! pointer lock with raw counts (`unadjustedMovement`; a plain lock where that is not supported);
//! losing it (Esc: the browser's) pauses the input ([`Paused`]) until a press takes it again, and
//! resets Bevy's grab mode (winit does not see the loss). Mouse back / forward don't leave the page.
//! Natively a click while playing grabs the cursor (locked, hidden); losing the focus or Esc lets it
//! go. Either way the mouse's counts are the device's (no acceleration or smoothing), as the R-held
//! mouse aim reads them.

use bevy::prelude::*;

/// The input is paused (sent neutral, but for the camera's yaw): in the browser, the pointer lock
/// was lost and no press has taken it again. Round 2's Esc menu opens on it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paused(pub bool);

pub fn build(app: &mut App, windowed: bool) {
    app.init_resource::<Paused>();
    if !windowed {
        return;
    }
    #[cfg(target_family = "wasm")]
    {
        web::listen();
        app.add_systems(PreUpdate, web::sync);
    }
    #[cfg(not(target_family = "wasm"))]
    app.add_systems(Update, grab);
}

/// Natively: a click while playing locks and hides the cursor; the window losing the focus, or Esc,
/// frees it. The debug camera grabs it itself (holding the right button).
#[cfg(not(target_family = "wasm"))]
fn grab(
    mut windows: Query<
        (&Window, &mut bevy::window::CursorOptions),
        With<bevy::window::PrimaryWindow>,
    >,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    playing: Query<(), With<crate::FollowCamera>>,
    free: Query<&bevy::camera_controller::free_camera::FreeCameraState>,
) {
    use bevy::window::CursorGrabMode;
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    if free.iter().any(|c| c.enabled) {
        return;
    }
    let lock = match () {
        _ if !window.focused || keys.just_pressed(KeyCode::Escape) => false,
        _ if mouse.get_just_pressed().next().is_some() && !playing.is_empty() => true,
        _ => return,
    };
    let mode = if lock {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if cursor.grab_mode != mode {
        cursor.grab_mode = mode;
        cursor.visible = !lock;
        info!("cursor {}", if lock { "grabbed" } else { "freed" });
    }
}

#[cfg(target_family = "wasm")]
mod web {
    use super::Paused;
    use bevy::prelude::*;
    use bevy::window::{CursorGrabMode, CursorOptions};
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};

    /// The page holds the pointer lock; it lost it since the last frame.
    static LOCKED: AtomicBool = AtomicBool::new(false);
    static LOST: AtomicBool = AtomicBool::new(false);

    fn not_supported(e: &JsValue) -> bool {
        let name = js_sys::Reflect::get(e, &"name".into()).ok();
        name.and_then(|n| n.as_string()).as_deref() == Some("NotSupportedError")
    }

    /// `requestPointerLock({ unadjustedMovement: true })` (web-sys's takes no options), else the
    /// plain lock where raw input is not supported. Chrome answers with a promise, others with
    /// nothing (they ignore the option).
    fn lock(canvas: &web_sys::Element) {
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&options, &"unadjustedMovement".into(), &true.into());
        let request = js_sys::Reflect::get(canvas, &"requestPointerLock".into())
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
        let Some(request) = request else {
            return;
        };
        match request.call1(canvas, &options) {
            Ok(answer) => {
                if let Ok(promise) = answer.dyn_into::<js_sys::Promise>() {
                    let canvas = canvas.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let refused = wasm_bindgen_futures::JsFuture::from(promise).await;
                        if refused.is_err_and(|e| not_supported(&e)) {
                            canvas.request_pointer_lock();
                        }
                    });
                }
            }
            Err(e) if not_supported(&e) => canvas.request_pointer_lock(),
            Err(_) => {}
        }
    }

    fn on(target: &web_sys::EventTarget, name: &str, f: impl FnMut(web_sys::Event) + 'static) {
        let f = Closure::<dyn FnMut(web_sys::Event)>::new(f);
        let _ = target.add_event_listener_with_callback(name, f.as_ref().unchecked_ref());
        f.forget();
    }

    /// The canvas's press, the lock's changes and mouse back / forward's release.
    pub fn listen() {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(canvas) = document.get_element_by_id("game") else {
            warn!("pointer lock: no #game canvas");
            return;
        };
        let (doc, target) = (document.clone(), canvas.clone());
        on(&canvas, "pointerdown", move |_| {
            if doc.pointer_lock_element().is_none() {
                lock(&target);
            }
        });
        let doc = document.clone();
        on(&document, "pointerlockchange", move |_| {
            let locked = doc.pointer_lock_element().is_some();
            LOCKED.store(locked, Relaxed);
            if !locked {
                LOST.store(true, Relaxed);
            }
        });
        // Buttons 3 and 4 go back and forward on their release. winit cancels `pointerdown`, which
        // may keep the `mouseup` from coming: cancel `pointerup` too.
        for name in ["mouseup", "pointerup"] {
            on(&window, name, |e| {
                let m = e.dyn_ref::<web_sys::MouseEvent>();
                if m.is_some_and(|m| matches!(m.button(), 3 | 4)) {
                    e.prevent_default();
                }
            });
        }
    }

    /// [`Paused`] from the lock; on a loss, Bevy's grab mode back to none (a lock Bevy took, the
    /// debug camera's, would otherwise still read as locked).
    pub fn sync(mut paused: ResMut<Paused>, mut cursors: Query<&mut CursorOptions>) {
        let lost = LOST.swap(false, Relaxed);
        if lost {
            info!("pointer lock lost: input paused until a click");
            for mut cursor in &mut cursors {
                if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
                    cursor.grab_mode = CursorGrabMode::None;
                    cursor.visible = true;
                }
            }
        }
        let now = !LOCKED.load(Relaxed) && (lost || paused.0);
        paused.set_if_neq(Paused(now));
    }
}
