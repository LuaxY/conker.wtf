import { getRouteApi, Link } from "@tanstack/react-router";
import { ArrowLeft, Check, Circle, CircleCheck, Crosshair, Crown, Send } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { ChatMessage, LobbyPlayer, Side } from "./data";
import { usePlayer } from "./store";
import { button, Emblem, field, Ping, primary, SIDES } from "./ui";

const route = getRouteApi("/lobby/$serverId");

export function Lobby() {
  const { server, players, chat } = route.useLoaderData();
  const { name, side, transport, set } = usePlayer();
  const [ready, setReady] = useState(false);

  const me: LobbyPlayer = {
    id: "me",
    name: name.trim() || "You",
    side,
    ready,
    ping: server.ping,
    host: players.length === 0,
  };
  const everyone = [...players, me];
  // play.html's options (web/README.md): the client rejects any it does not know.
  const play = `play.html?${new URLSearchParams({ server: server.address, transport, side })}`;

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

      <header className="mt-3 flex flex-wrap items-end justify-between gap-4 rounded-2xl border-2 border-mud bg-trench p-5">
        <div className="min-w-0">
          <p className="font-stencil text-sm tracking-widest text-acorn uppercase">Lobby</p>
          <h1 className="cartoon font-display text-4xl break-words sm:text-5xl">{server.name}</h1>
          <p className="font-mono text-sm text-dust">{server.address}</p>
        </div>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-2 sm:flex">
          {[
            ["Map", server.map],
            ["Mode", server.mode],
            ["Region", server.region],
            ["Players", `${everyone.length}/${server.maxPlayers}`],
          ].map(([term, value]) => (
            <div key={term}>
              <dt className="font-stencil text-xs text-dust uppercase">{term}</dt>
              <dd>{value}</dd>
            </div>
          ))}
          <div>
            <dt className="font-stencil text-xs text-dust uppercase">Ping</dt>
            <dd>
              <Ping ms={server.ping} />
            </dd>
          </div>
        </dl>
      </header>

      <div className="mt-6 grid gap-6 lg:grid-cols-[1fr_22rem]">
        <section
          aria-labelledby="you"
          className="rounded-2xl border-2 border-acorn/70 bg-trench p-5 lg:col-start-2 lg:row-start-1"
        >
          <h2 id="you" className="cartoon font-display text-3xl">
            You
          </h2>
          <label className="mt-3 block">
            <span className="font-stencil text-sm text-dust uppercase">Name</span>
            <input
              name="name"
              autoComplete="off"
              value={name}
              onChange={(e) => set({ name: e.target.value })}
              maxLength={16}
              placeholder="Conker"
              className={`${field} mt-1`}
            />
          </label>
          <fieldset className="mt-4">
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
            <button
              type="button"
              aria-pressed={ready}
              onClick={() => setReady(!ready)}
              className={`${button} border-2 py-3 ${ready ? "border-go bg-go/15 text-go" : "border-mud bg-bunker hover:border-go"}`}
            >
              {ready ? (
                <CircleCheck aria-hidden className="size-5" />
              ) : (
                <Circle aria-hidden className="size-5" />
              )}
              I'm ready
            </button>
            {server.status === "full" ? (
              <button type="button" disabled className={`${primary} py-4 text-lg`}>
                Server full
              </button>
            ) : (
              <a href={play} className={`${primary} py-4 text-lg`}>
                <Crosshair aria-hidden className="size-5" />
                Join game
              </a>
            )}
            <p className="text-sm text-dust md:hidden">The game needs a keyboard or a gamepad.</p>
          </div>
        </section>

        <section
          aria-label="Players"
          className="grid content-start gap-6 sm:grid-cols-2 lg:col-start-1 lg:row-span-2 lg:row-start-1"
        >
          {(["squirrels", "tediz"] as const).map((id) => (
            <Team key={id} side={id} players={everyone.filter((p) => p.side === id)} />
          ))}
        </section>

        <Chat initial={chat} me={me} />
      </div>
    </motion.main>
  );
}

function Team({ side, players }: { side: Side; players: LobbyPlayer[] }) {
  return (
    <section
      aria-labelledby={`team-${side}`}
      className={`overflow-hidden rounded-2xl border-2 bg-trench ${SIDES[side].border}`}
    >
      <header className="flex items-center gap-3 border-b-2 border-mud p-4">
        <Emblem side={side} className="size-14 rounded-lg" />
        <div>
          <h2 id={`team-${side}`} className={`cartoon font-display text-3xl ${SIDES[side].text}`}>
            {SIDES[side].name}
          </h2>
          <p className="text-sm text-dust">{SIDES[side].tagline}</p>
        </div>
        <span className="ml-auto font-stencil text-2xl">
          {players.length}
          <span className="sr-only"> players</span>
        </span>
      </header>
      <ul>
        {players.map((p) => (
          <li
            key={p.id}
            className={`flex items-center gap-3 border-t border-mud px-4 py-3 first:border-0 ${p.id === "me" ? "bg-mud" : ""}`}
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate font-semibold">{p.name}</span>
              <span className="flex gap-2 text-xs">
                {p.id === "me" && <span className="text-acorn">you</span>}
                {p.host && (
                  <span className="inline-flex items-center gap-1 text-acorn">
                    <Crown aria-hidden className="size-3.5" />
                    host
                  </span>
                )}
              </span>
            </span>
            {p.ready ? (
              <span className="inline-flex items-center gap-1 text-sm text-go">
                <Check aria-hidden className="size-4" />
                Ready
              </span>
            ) : (
              <span className="text-sm text-dust">Not ready</span>
            )}
            <span className="w-16 text-right text-sm">
              <Ping ms={p.ping} />
            </span>
          </li>
        ))}
        {players.length === 0 && <li className="px-4 py-3 text-sm text-dust">Nobody yet.</li>}
      </ul>
    </section>
  );
}

function Chat({ initial, me }: { initial: ChatMessage[]; me: LobbyPlayer }) {
  const [messages, setMessages] = useState(initial);
  const [text, setText] = useState("");
  const log = useRef<HTMLOListElement>(null);
  useEffect(() => {
    log.current?.scrollTo({ top: log.current.scrollHeight });
  }, [messages]);

  return (
    <section
      aria-labelledby="chat"
      className="rounded-2xl border-2 border-mud bg-trench p-5 lg:col-start-2 lg:row-start-2"
    >
      <h2 id="chat" className="cartoon font-display text-2xl">
        Chat
      </h2>
      <ol
        ref={log}
        aria-live="polite"
        className="mt-2 grid h-44 content-start gap-1.5 overflow-y-auto text-sm"
      >
        {messages.map((m) => (
          <li key={m.id}>
            <span className={`font-semibold ${m.side ? SIDES[m.side].text : "text-acorn"}`}>
              {m.from}:
            </span>{" "}
            {m.text}
          </li>
        ))}
        {messages.length === 0 && <li className="text-dust">Nobody has said anything yet.</li>}
      </ol>
      <form
        className="mt-3 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (!text.trim()) return;
          setMessages([
            ...messages,
            { id: `me${messages.length}`, from: me.name, side: me.side, text: text.trim() },
          ]);
          setText("");
        }}
      >
        <label className="flex-1">
          <span className="sr-only">Message</span>
          <input
            name="message"
            value={text}
            onChange={(e) => setText(e.target.value)}
            maxLength={200}
            placeholder="Say something"
            className={field}
          />
        </label>
        <button type="submit" aria-label="Send" className={`${primary} px-3`}>
          <Send aria-hidden className="size-5" />
        </button>
      </form>
    </section>
  );
}
