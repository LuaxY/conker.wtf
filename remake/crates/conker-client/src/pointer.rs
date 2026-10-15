//! The mouse pointer (CONTROLS_RESEARCH.md §6, §8). In the browser a press on the canvas takes the
//! pointer lock with raw counts (`unadjustedMovement`; a plain lock where that is not supported),
//! but not while the Esc menu is open (only its RESUME does, [`lock`]); losing it (Esc: the
//! browser's) raises [`Paused`], which opens the menu, and resets Bevy's grab mode (winit does not
//! see the loss). Without the lock (the menu closed by Esc, the match's start) the mouse turns
//! nothing until a click on the canvas takes it. Mouse back / forward don't leave the page.
//! Natively a click while playing grabs the cursor (locked, hidden), and so does the menu's
//! closing; losing the focus, Esc or the open menu lets it go. Either way the mouse's counts are
//! the device's (no acceleration or smoothing), as the R-held mouse aim reads them.

use bevy::prelude::*;

/// The input is paused (sent neutral, but for the camera's yaw): the Esc menu holds it, as it sets
/// this each frame ([`crate::menu`]). In the browser the pointer lock's loss raises it (the menu
/// opens on it).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paused(pub bool);

/// RESUME's click or key: the pointer lock again (the browser; natively the cursor follows
/// [`Paused`]). Within the click's user activation; Chrome refuses it for 1.25 s after an Esc.
pub fn lock() {
    #[cfg(target_family = "wasm")]
    web::relock();
}

/// The page holds the pointer lock (the browser; natively false).
pub fn locked() -> bool {
    #[cfg(target_family = "wasm")]
    let locked = web::LOCKED.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_family = "wasm"))]
    let locked = false;
    locked
}

/// A lock request was refused since the last call (the browser's `pointerlockerror`).
pub fn refused() -> bool {
    #[cfg(target_family = "wasm")]
    let refused = web::REFUSED.swap(false, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_family = "wasm"))]
    let refused = false;
    refused
}

pub fn build(app: &mut App, windowed: bool) {
    app.init_resource::<Paused>();
    if !windowed {
        return;
    }
    #[cfg(target_family = "wasm")]
    {
        web::listen();
        app.add_systems(PreUpdate, web::sync.after(bevy::input::InputSystems));
    }
    #[cfg(not(target_family = "wasm"))]
    app.add_systems(Update, grab);
}

/// Natively: a click while playing locks and hides the cursor, and so does the Esc menu's closing;
/// the window losing the focus, Esc or the open menu frees it. The debug camera grabs it itself
/// (holding the right button).
#[cfg(not(target_family = "wasm"))]
#[allow(clippy::too_many_arguments)]
fn grab(
    mut windows: Query<
        (&Window, &mut bevy::window::CursorOptions),
        With<bevy::window::PrimaryWindow>,
    >,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    playing: Query<(), With<crate::FollowCamera>>,
    free: Query<&bevy::camera_controller::free_camera::FreeCameraState>,
    paused: Res<Paused>,
    mut was: Local<bool>,
) {
    use bevy::window::CursorGrabMode;
    let resumed = std::mem::replace(&mut *was, paused.0) && !paused.0;
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    if free.iter().any(|c| c.enabled) {
        return;
    }
    let press = resumed || mouse.get_just_pressed().next().is_some();
    let lock = match () {
        _ if !window.focused || paused.0 || keys.just_pressed(KeyCode::Escape) => false,
        _ if press && !playing.is_empty() => true,
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

    /// The page holds the pointer lock; it lost it since the last frame; a request was refused;
    /// the Esc menu is open (as [`Paused`] was last frame).
    pub static LOCKED: AtomicBool = AtomicBool::new(false);
    static LOST: AtomicBool = AtomicBool::new(false);
    pub static REFUSED: AtomicBool = AtomicBool::new(false);
    static MENU: AtomicBool = AtomicBool::new(false);

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
                        match wasm_bindgen_futures::JsFuture::from(promise).await {
                            Err(e) if not_supported(&e) => canvas.request_pointer_lock(),
                            Err(_) => REFUSED.store(true, Relaxed),
                            Ok(_) => {}
                        }
                    });
                }
            }
            Err(e) if not_supported(&e) => canvas.request_pointer_lock(),
            Err(_) => {}
        }
    }

    fn canvas() -> Option<web_sys::Element> {
        web_sys::window()?.document()?.get_element_by_id("game")
    }

    /// RESUME's request ([`super::lock`]).
    pub fn relock() {
        if let Some(canvas) = canvas() {
            lock(&canvas);
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
            if doc.pointer_lock_element().is_none() && !MENU.load(Relaxed) {
                lock(&target);
            }
        });
        on(&document, "pointerlockerror", |_| {
            REFUSED.store(true, Relaxed)
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

    /// A loss raises [`Paused`] (the menu opens on it) and puts Bevy's grab mode back to none (a
    /// lock Bevy took, the debug camera's, would otherwise still read as locked); the menu's state
    /// for the canvas's presses. Without the lock the mouse's counts are dropped: they are the
    /// cursor's moving over the page (the menu closed by Esc, before the first click), not a look.
    pub fn sync(
        mut paused: ResMut<Paused>,
        mut cursors: Query<&mut CursorOptions>,
        motion: Option<ResMut<bevy::input::mouse::AccumulatedMouseMotion>>,
    ) {
        if let Some(mut motion) = motion.filter(|_| !LOCKED.load(Relaxed)) {
            motion.delta = Vec2::ZERO;
        }
        MENU.store(paused.0, Relaxed);
        if LOST.swap(false, Relaxed) {
            info!("pointer lock lost: the menu opens");
            for mut cursor in &mut cursors {
                if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
                    cursor.grab_mode = CursorGrabMode::None;
                    cursor.visible = true;
                }
            }
            paused.set_if_neq(Paused(true));
        }
    }
}
