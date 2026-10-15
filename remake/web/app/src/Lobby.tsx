import { getRouteApi, Link } from "@tanstack/react-router";
import { ArrowLeft, Crosshair } from "lucide-react";
import { motion } from "motion/react";
import { useState } from "react";
import { allowed, MAPS, MODES, PICKS, players, settingRows } from "./data";
import { usePlayer } from "./store";
import { Banner, PLATES, SIDES, StatusBadge, useRefresh } from "./ui";

const route = getRouteApi("/lobby/$serverId");
/** Heist's teams, in the order of play.html's `team=` (the original's team screen names). */
const HEIST_TEAMS = ["Reds", "Blues", "Greens", "Yellows"];

export function Lobby() {
  const server = route.useLoaderData();
  const { side, transport, character, set } = usePlayer();
  useRefresh();
  // The picks this server's map and mode allow with its cheats (none: no select); a pick not among
  // them is not asked for (the server would give the slot's default anyway).
  const picks = allowed(server.map, server.mode, server.settings.cheats);
  const pick = character !== null && picks & (1 << character) ? character : null;
  // Heist's four teams (the side picks the team in the other War modes): play.html's `team=`.
  const [team, setTeam] = useState("");
  // play.html's options (web/README.md): the client rejects any it does not know. The digest is this
  // server's own, since several run at once (dist/digest.txt names one at most); none in production
  // (the domain's certificate). The token and /info come through the web server, which knows the port.
  const options: Record<string, string> = {
    server: server.address,
    transport,
    side,
    "token-url": `/api/servers/${server.id}/token`,
  };
  if (server.digest) options.digest = server.digest;
  if (pick !== null) options.character = String(pick);
  if (server.mode === "heist" && team) options.team = team;
  const play = `play.html?${new URLSearchParams(options)}`;
  const facts = [
    ["Map", MAPS[server.map].name],
    ["Mode", MODES[server.mode].name],
    ["Playing", players(server) ?? "not answering"],
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
        className="inline-flex items-center gap-1 text-sm text-dust hover:text-bone"
      >
        <ArrowLeft aria-hidden className="size-4" />
        All servers
      </Link>

      <div className="mt-4 grid items-start gap-8 lg:grid-cols-[1fr_1.1fr]">
        <header className="board grid gap-5 p-6 sm:p-8">
          <div className="min-w-0">
            <p className="flex flex-wrap items-center gap-3 font-stencil text-sm tracking-widest text-lime uppercase">
              Lobby <StatusBadge status={server.status} code={server.code} />
            </p>
            <h1 className="cartoon mt-2 font-display text-4xl break-words text-amber sm:text-5xl">
              {server.name}
            </h1>
            <p className="font-mono text-sm text-dust">{server.address}</p>
          </div>
          <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {facts.map(([term, value]) => (
              <div key={term} className="rounded-xl border-2 border-black bg-black/45 px-3 py-2">
                <dt className="font-stencil text-xs tracking-wider text-dust uppercase">{term}</dt>
                <dd className="font-display tracking-wide text-bone uppercase">{value}</dd>
              </div>
            ))}
          </dl>
        </header>

        <section aria-labelledby="you" className="board p-6 sm:p-8">
          <h2 id="you" className="chrome font-display text-4xl [--c:#b070ff]">
            Who are you?
          </h2>
          <fieldset className="mt-4">
            <legend className="font-stencil text-sm tracking-wider text-dust uppercase">
              Side
            </legend>
            <div className="mt-2 grid grid-cols-2 gap-3 sm:gap-4">
              {(["squirrels", "tediz"] as const).map((id) => (
                <label
                  key={id}
                  className={`group relative grid cursor-pointer justify-items-center gap-2 overflow-hidden rounded-2xl border-3 border-black p-3 text-center shadow-[4px_4px_0_#000] transition hover:-translate-y-0.5 has-checked:-translate-y-1 has-checked:outline-3 has-checked:outline-offset-2 has-focus-visible:outline-3 has-focus-visible:outline-offset-2 has-focus-visible:outline-lime sm:p-4 ${SIDES[id].deep} ${id === "squirrels" ? "has-checked:outline-squirrels" : "has-checked:outline-tediz"}`}
                >
                  <span
                    aria-hidden
                    className="pixelated absolute inset-0 opacity-70 brightness-75"
                    style={{ backgroundImage: `url(${PLATES[id]})`, backgroundSize: "192px" }}
                  />
                  <input
                    type="radio"
                    name="side"
                    value={id}
                    checked={side === id}
                    onChange={() => set({ side: id })}
                    className="sr-only"
                  />
                  <Banner
                    side={id}
                    className="relative size-16 transition group-has-checked:rotate-3 sm:size-32"
                  />
                  <span className="relative">
                    <span
                      className={`cartoon block font-display text-2xl sm:text-3xl ${SIDES[id].text}`}
                    >
                      {SIDES[id].name}
                    </span>
                    <span className="block text-xs text-bone [text-shadow:0_1px_2px_#000] sm:text-sm">
                      {SIDES[id].tagline}
                    </span>
                  </span>
                  <span
                    aria-hidden
                    className="invisible relative rounded-full border-2 border-black bg-lime px-3 font-display text-sm text-moss group-has-checked:visible"
                  >
                    That's you
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
          {picks !== 0 && (
            <label className="mt-6 block">
              <span className="font-stencil text-sm tracking-wider text-dust uppercase">
                Character
              </span>
              <select
                name="character"
                value={pick ?? ""}
                onChange={(e) => set({ character: e.target.value ? Number(e.target.value) : null })}
                className="field mt-1"
              >
                <option value="">The server's choice</option>
                {PICKS.map((name, k) =>
                  picks & (1 << k) ? (
                    <option key={name} value={k}>
                      {name}
                    </option>
                  ) : null,
                )}
              </select>
            </label>
          )}
          {server.mode === "heist" && (
            <label className="mt-6 block">
              <span className="font-stencil text-sm tracking-wider text-dust uppercase">Team</span>
              <select
                name="team"
                value={team}
                onChange={(e) => setTeam(e.target.value)}
                className="field mt-1"
              >
                <option value="">The server's choice</option>
                {HEIST_TEAMS.map((name, k) => (
                  <option key={name} value={k}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
          )}
          <label className="mt-6 block">
            <span className="font-stencil text-sm tracking-wider text-dust uppercase">
              Connection
            </span>
            <select
              name="transport"
              value={transport}
              onChange={(e) => set({ transport: e.target.value === "ws" ? "ws" : "wt" })}
              className="field mt-1"
            >
              <option value="wt">WebTransport (UDP)</option>
              <option value="ws">WebSocket (TCP)</option>
            </select>
          </label>
          <div className="mt-8 grid gap-3">
            {server.status === "running" ? (
              <a href={play} className="cta">
                <Crosshair aria-hidden className="size-6" />
                Join game
              </a>
            ) : (
              <button type="button" disabled className="cta">
                Server {server.status}
              </button>
            )}
            <p className="text-sm text-dust md:hidden">The game needs a keyboard or a gamepad.</p>
          </div>
        </section>
      </div>
    </motion.main>
  );
}
