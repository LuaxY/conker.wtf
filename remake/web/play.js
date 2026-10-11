// Starts the wasm client as soon as the page loads. The client asks the game server's /info which
// map it plays, fetches the asset pack (its data files, then Bevy its glTF models) from assets/ next
// to this page, and, once the level is drawn, its connect token from the game server (a token lasts 60 s).
// Options are read from the page URL by the client itself:
//   ?transport=wt|ws  &server=host:port (default: this page's host, port 5888)  &digest=HEX  &id=N  &script
//   &lag=MS  &log=DIRECTIVES  &token-url=URL (see README.md)
// On an http page without ?digest=, this script reads the dev WebTransport certificate hash from
// dist/digest.txt (`conker-server --digest-file web/dist/digest.txt`) and passes it to the client
// for start() only, so a reload after a server restart uses the new one. Over https the
// certificate is checked as any site's, and WebSocket uses wss://.
import init, { start } from "./dist/client.js";

const status = document.getElementById("status");
const show = (text, error = false) => {
  status.textContent = text;
  status.className = error ? "error" : "";
};

const url = new URL(location.href);
const digest = async () => {
  const webSocket = ["ws", "websocket"].includes(url.searchParams.get("transport"));
  if (location.protocol === "https:" || webSocket || url.searchParams.has("digest")) return null;
  const res = await fetch("dist/digest.txt", { cache: "no-store" }).catch(() => null);
  return res?.ok ? (await res.text()).trim() : null;
};

await init();
show("Loading the asset pack…");
const hash = await digest();
const t0 = performance.now();
// The client reads its options from the address once, in start(): the digest goes there only for
// that call, so the address (and a reload) never keeps a stale one.
const address = location.href;
if (hash) {
  const options = new URL(address);
  options.searchParams.set("digest", hash);
  history.replaceState(null, "", options);
}
try {
  await start();
  console.info(`asset pack fetched and client started in ${Math.round(performance.now() - t0)} ms`);
  document.getElementById("intro").hidden = true;
  document.getElementById("game").focus();
} catch (e) {
  show(`Could not start: ${e?.message ?? e}. Reload to try again.`, true);
} finally {
  history.replaceState(null, "", address);
}
