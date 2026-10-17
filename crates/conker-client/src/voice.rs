//! Push-to-talk ([`Voice`], [`Heard`]): while the PushToTalk action (V) is held, the page's
//! `voice.js` encodes the microphone (WebCodecs Opus, 20 ms packets) and hands each packet to
//! [`push`] (the page's `voice_frame`); [`send`] sends them to the server, which relays them to the
//! players who hear this one. [`hear`] gives each relayed packet to the page to play and notes when
//! its speaker last talked ([`Talking`]): `tags` draws a speech bubble over a talking player, and
//! one on the HUD while this client's own packets go out. Mute (M) mutes the voices too. Natively
//! there is no microphone and nothing plays; the bubbles still show.

use crate::bindings::{Action, Controls};
use bevy::prelude::*;
use conker_sim::net::{Heard, Voice, VoiceChannel};
use lightyear::prelude::{Client, MessageReceiver, MessageSender};
use std::collections::HashMap;
use std::sync::Mutex;

/// Seconds a player shows as talking after its last packet.
pub const HOLD: f32 = 0.25;

/// The packets the page encoded since the last frame ([`push`]).
static OUT: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// One packet from the page's encoder, sent with the next frame.
pub fn push(packet: Vec<u8>) {
    if let Ok(mut out) = OUT.lock() {
        out.push(packet);
    }
}

/// When each slot's player last talked, and this client's own last packet (`Time<Real>` seconds).
#[derive(Resource, Default)]
pub struct Talking {
    heard: HashMap<u16, f32>,
    own: Option<f32>,
}

impl Talking {
    /// Whether the player in `slot` talks at `now`.
    pub fn talks(&self, slot: u16, now: f32) -> bool {
        self.heard.get(&slot).is_some_and(|&t| now - t < HOLD)
    }

    /// Whether this client's own voice goes out at `now`.
    pub fn own(&self, now: f32) -> bool {
        self.own.is_some_and(|t| now - t < HOLD)
    }
}

/// The page's `conkerVoice[method](...args)` (`voice.js`).
#[cfg(target_family = "wasm")]
fn page(method: &str, args: &[wasm_bindgen::JsValue]) {
    crate::web::page("conkerVoice", method, args);
}

/// The key pressed or released: the page's microphone on or off; Mute (M): the voices too.
fn keys(controls: Controls, mut held: Local<bool>, mut muted: Local<bool>) {
    let on = controls.pressed(Action::PushToTalk);
    if on != *held {
        *held = on;
        #[cfg(target_family = "wasm")]
        page("talk", &[on.into()]);
    }
    if controls.just_pressed(Action::Mute) {
        *muted = !*muted;
        #[cfg(target_family = "wasm")]
        page("mute", &[(*muted).into()]);
    }
}

/// The page's packets since the last frame, to the server.
fn send(
    mut clients: Query<&mut MessageSender<Voice>, With<Client>>,
    time: Res<Time<Real>>,
    mut talking: ResMut<Talking>,
) {
    let packets = OUT.lock().map(|mut o| std::mem::take(&mut *o));
    let (Ok(packets), Ok(mut sender)) = (packets, clients.single_mut()) else {
        return;
    };
    if !packets.is_empty() {
        talking.own = Some(time.elapsed_secs());
    }
    for packet in packets {
        sender.send::<VoiceChannel>(Voice(packet));
    }
}

/// The relayed packets: to the page to play, and their speakers' times. Every frame (lightyear
/// drops what a frame leaves unread).
fn hear(
    mut clients: Query<&mut MessageReceiver<Heard>, With<Client>>,
    time: Res<Time<Real>>,
    mut talking: ResMut<Talking>,
) {
    let now = time.elapsed_secs();
    for mut heard in &mut clients {
        for h in heard.receive() {
            talking.heard.insert(h.slot, now);
            #[cfg(target_family = "wasm")]
            page(
                "play",
                &[
                    h.slot.into(),
                    js_sys::Uint8Array::from(&h.packet[..]).into(),
                ],
            );
        }
    }
}

/// Push-to-talk (windowed clients).
pub fn build(app: &mut App) {
    app.init_resource::<Talking>();
    app.add_systems(Update, (keys, send, hear.before(crate::tags::update)));
}
