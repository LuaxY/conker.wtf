import { getRouteApi, Link } from "@tanstack/react-router";
import { ArrowLeft, Crosshair } from "lucide-react";
import { motion } from "motion/react";
import { MAPS, MODES, settingRows } from "./data";
import { usePlayer } from "./store";
import { Emblem, field, primary, SIDES, StatusBadge } from "./ui";

const route = getRouteApi("/lobby/$serverId");

export function Lobby() {
  const server = route.useLoaderData();
  const { side, transport, set } = usePlayer();
  // play.html's options (web/README.md): the client rejects any it does not know. The digest is this
  // server's own, since several run at once (dist/digest.txt names one at most).
  const play = `play.html?${new URLSearchParams({ server: server.address, transport, side, digest: server.digest })}`;
  const facts = [
    ["Map", MAPS[server.map].name],
    ["Mode", MODES[server.mode].name],
    ...settingRows(server),
  ];

  return (
    <motion.main
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      className="mx-auto w-full max-w-6xl px-4 py-8"
    >
      <Link
        to="/servers"
        className="inline-flex items-center gap-1 text-sm text-dust hover:text-khaki"
      >
        <ArrowLeft aria-hidden className="size-4" />
        All servers
      </Link>

      <header className="mt-3 grid gap-4 rounded-2xl border-2 border-mud bg-trench p-5">
        <div className="min-w-0">
          <p className="flex items-center gap-3 font-stencil text-sm tracking-widest text-acorn uppercase">
            Lobby <StatusBadge status={server.status} code={server.code} />
          </p>
          <h1 className="cartoon font-display text-4xl break-words sm:text-5xl">{server.name}</h1>
          <p className="font-mono text-sm text-dust">{server.address}</p>
        </div>
        <dl className="flex flex-wrap gap-x-6 gap-y-2">
          {facts.map(([term, value]) => (
            <div key={term}>
              <dt className="font-stencil text-xs text-dust uppercase">{term}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </header>

      <section
        aria-labelledby="you"
        className="mt-6 max-w-xl rounded-2xl border-2 border-acorn/70 bg-trench p-5"
      >
        <h2 id="you" className="cartoon font-display text-3xl">
          You
        </h2>
        <fieldset className="mt-3">
          <legend className="font-stencil text-sm text-dust uppercase">Side</legend>
          <div className="mt-1 grid grid-cols-2 gap-2">
            {(["squirrels", "tediz"] as const).map((id) => (
              <label
                key={id}
                className={`grid cursor-pointer justify-items-center gap-1 rounded-xl border-2 border-mud p-3 text-center has-checked:bg-mud has-focus-visible:outline-3 has-focus-visible:outline-acorn ${id === "squirrels" ? "has-checked:border-squirrels" : "has-checked:border-tediz"}`}
              >
                <input
                  type="radio"
                  name="side"
                  value={id}
                  checked={side === id}
                  onChange={() => set({ side: id })}
                  className="sr-only"
                />
                <Emblem side={id} className="size-12 rounded-lg" />
                <span>
                  <span className={`block font-display text-xl ${SIDES[id].text}`}>
                    {SIDES[id].name}
                  </span>
                  <span className="block text-xs text-dust">{SIDES[id].tagline}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
        <label className="mt-4 block">
          <span className="font-stencil text-sm text-dust uppercase">Connection</span>
          <select
            name="transport"
            value={transport}
            onChange={(e) => set({ transport: e.target.value === "ws" ? "ws" : "wt" })}
            className={`${field} mt-1`}
          >
            <option value="wt">WebTransport (UDP)</option>
            <option value="ws">WebSocket (TCP)</option>
          </select>
        </label>
        <div className="mt-5 grid gap-3">
          {server.status === "running" ? (
            <a href={play} className={`${primary} py-4 text-lg`}>
              <Crosshair aria-hidden className="size-5" />
              Join game
            </a>
          ) : (
            <button type="button" disabled className={`${primary} py-4 text-lg`}>
              Server {server.status}
            </button>
          )}
          <p className="text-sm text-dust md:hidden">The game needs a keyboard or a gamepad.</p>
        </div>
      </section>
    </motion.main>
  );
}
