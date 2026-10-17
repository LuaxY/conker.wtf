// Starts the wasm client as soon as the page loads. The client asks the game server's /info which
// map it plays, fetches the asset pack (its data files, then Bevy its glTF models) from assets/ next
// to this page, and, once the level is drawn, its connect token from the game server (a token lasts 60 s).
// Push-to-talk's microphone and the others' voices are voice.js's, the text chat's box and lines chat.js's.
// The loading overlay (#intro) stays up until the player is in the match: this script fills its bar
// while the game downloads, then the client reports each step through window.conkerLoading
// (conker_client::web::loading) and hides it once its player arrived, or a problem leaves the
// connection to the menu's page.
// Options are read from the page URL by the client itself:
//   ?transport=wt|ws  &server=host:port (default: this page's host, port 5888)  &digest=HEX  &id=N  &script
//   &lag=MS  &log=DIRECTIVES  &token-url=URL (see README.md)
// On an http page without ?digest=, this script reads the dev WebTransport certificate hash from
// dist/digest.txt (`conker-server --digest-file web/dist/digest.txt`) and passes it to the client
// for start() only, so a reload after a server restart uses the new one. Over https the
// certificate is checked as any site's, and WebSocket uses wss://.
import init, { chat_done, start, voice_frame } from "./dist/client.js";
import { chat } from "./chat.js";
import { voice } from "./voice.js";

// Push-to-talk (voice.js) and the text chat (chat.js): the client calls them from start() on.
voice(voice_frame);
chat(chat_done);

const intro = document.getElementById("intro");
const status = document.getElementById("status");
const detail = document.getElementById("detail");
const bar = intro.querySelector(".choc");
const pieces = [...bar.children];
const show = (text, error = false) => {
  status.textContent = text;
  status.className = error ? "error" : "";
};

// A step: the bubble's words, the line under it, the bar's fill (0-1, it never goes back; 1 fades
// the overlay out).
let filled = 0;
window.conkerLoading = (step, what, progress) => {
  if (progress >= 1) {
    intro.classList.add("done");
    setTimeout(() => (intro.hidden = true), 400);
    return;
  }
  show(step);
  detail.textContent = what;
  filled = Math.max(filled, progress);
  pieces.forEach((p, i) => p.classList.toggle("on", i < Math.round(filled * pieces.length)));
  bar.setAttribute("aria-valuenow", Math.round(filled * 100));
};

// The controls, one tip at a time while it loads.
const TIPS = [
  "Click the game to capture the mouse; Esc opens the menu.",
  "WASD or the arrows move, Space jumps, hold Left Shift to walk.",
  "Left mouse fires, right mouse aims; the wheel zooms a scope.",
  "R reloads, Q draws or puts away your weapon, C crouches.",
  "G throws a grenade: hold it to cook it.",
  "Tab opens the team screen in the team modes.",
  "Hold V to talk: everyone hears you, only your team in the team modes.",
  "T chats with everyone, Y with your team; Enter sends, Esc cancels.",
  "M mutes the sound and the voices, H shows the HUD as text.",
  "The key under Esc shows diagnostics, F8 the free camera.",
  "Gamepad: A jumps, X reloads, Y draws, B crouches, the triggers aim and fire.",
  "The Esc menu's Settings rebind the keys, the mouse and the gamepad.",
];
// In a random order (Fisher-Yates), each once before any comes back.
for (let i = TIPS.length - 1; i > 0; i--) {
  const j = Math.floor(Math.random() * (i + 1));
  [TIPS[i], TIPS[j]] = [TIPS[j], TIPS[i]];
}
const tip = document.getElementById("tip");
let tips = 0;
tip.textContent = TIPS[0];
const turn = setInterval(() => {
  if (intro.hidden) return clearInterval(turn);
  tip.classList.add("out");
  setTimeout(() => {
    tip.textContent = TIPS[++tips % TIPS.length];
    tip.classList.remove("out");
  }, 300);
}, 4000);

// The game's download fills the bar's first 28 % (a compressed one has no known size: its MB, the
// bar creeping). A clone is counted, so the browser still caches the compiled wasm by its URL.
const DOWNLOAD = 0.28;
const wasm = async () => {
  // Next to client.js, wherever the import points (deploy/build.sh rewrites both to v/HASH/).
  const res = await fetch(new URL("client_bg.wasm", new URL("./dist/client.js", import.meta.url)));
  const reader = res.ok && res.clone().body?.getReader();
  const size = res.headers.get("content-encoding") ? 0 : Number(res.headers.get("content-length"));
  const mb = (n) => (n / 1e6).toFixed(1);
  // Not over the client's own steps, should the count end after the client started.
  const step = (what, part) => filled <= DOWNLOAD && conkerLoading("Waking up the Tediz…", what, DOWNLOAD * part);
  (async () => {
    for (let got = 0; reader; ) {
      const { done, value } = await reader.read();
      if (done) break;
      got += value.byteLength;
      const of = size ? ` / ${mb(size)}` : "";
      step(`downloading the game: ${mb(got)}${of} MB`, size ? got / size : got / (got + 30e6));
    }
    step("starting the game", 1);
  })().catch(() => {}); // init() reports a failed download
  return res;
};

const url = new URL(location.href);
const digest = async () => {
  const webSocket = ["ws", "websocket"].includes(url.searchParams.get("transport"));
  if (location.protocol === "https:" || webSocket || url.searchParams.has("digest")) return null;
  const res = await fetch("dist/digest.txt", { cache: "no-store" }).catch(() => null);
  return res?.ok ? (await res.text()).trim() : null;
};

const t0 = performance.now();
const hash = await digest();
// The client reads its options from the address once, in start(): the digest goes there only for
// that call, so the address (and a reload) never keeps a stale one.
const address = location.href;
if (hash) {
  const options = new URL(address);
  options.searchParams.set("digest", hash);
  history.replaceState(null, "", options);
}
try {
  await init({ module_or_path: wasm() });
  await start();
  console.info(`game downloaded, asset pack fetched and client started in ${Math.round(performance.now() - t0)} ms`);
  // Under the overlay: the keys (the diagnostics' too) reach the game while it loads.
  document.getElementById("game").focus();
} catch (e) {
  show(`Bugger. Could not start: ${e?.message ?? e}. Reload to try again.`, true);
} finally {
  history.replaceState(null, "", address);
}
