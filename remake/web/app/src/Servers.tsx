import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { LogIn, Play, RefreshCw, X } from "lucide-react";
import { motion } from "motion/react";
import { type FormEvent, type ReactNode, useState } from "react";
import {
  createServer,
  fit,
  type GameServer,
  killServer,
  MAPS,
  type MapId,
  MODES,
  type NewServer,
  type Settings,
  settingRows,
  SKILLS,
} from "./data";
import { SkullIcon, StatusBadge } from "./ui";

const route = getRouteApi("/servers");
const LABEL = "font-stencil text-sm tracking-wider text-dust uppercase";

// One grid for the header and the rows: stacked on phones, columns from md up.
const COLUMNS =
  "grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 md:grid-cols-[minmax(0,1.5fr)_1.1fr_1.8fr_6.5rem_12rem]";

export function Servers() {
  const servers = route.useLoaderData();
  const router = useRouter();
  const [refreshing, setRefreshing] = useState(false);

  const refresh = async () => {
    setRefreshing(true);
    await router.invalidate({ sync: true });
    setRefreshing(false);
  };

  return (
    <motion.main
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      className="mx-auto w-full max-w-6xl px-4 py-10"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="chrome font-display text-6xl [--c:#00c0f0]">Servers</h1>
          <p className="mt-2 text-dust">
            {servers === null
              ? "No server list on this site."
              : `${servers.filter((s) => s.status === "running").length} running on this machine. Open one's lobby to join, or run your own.`}
          </p>
        </div>
        <button type="button" onClick={refresh} disabled={refreshing} className="pill">
          <RefreshCw aria-hidden className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
          {refreshing ? "Refreshing" : "Refresh"}
        </button>
      </div>

      <div className="hazard mt-6" />

      {servers === null ? (
        <p className="bubble mt-10 max-w-2xl">
          No server list here, mate. It and "Create server" come from the dev web server (
          <code className="font-mono text-sm">cd remake/web/app && bun run serve</code>), which this
          site is not served by. You can still{" "}
          <a href="play.html" className="font-bold text-tediz-deep underline">
            play on this site's own game server
          </a>{" "}
          (port 5888).
        </p>
      ) : (
        <>
          <div
            aria-hidden
            className={`${COLUMNS} mt-8 hidden px-5 font-stencil text-xs tracking-wider text-dust uppercase ${servers.length ? "md:grid" : ""}`}
          >
            <span>Server</span>
            <span>Map · mode</span>
            <span>Settings</span>
            <span>Status</span>
          </div>
          <ul className="mt-3 grid gap-4">
            {servers.map((server) => (
              <li key={server.id}>
                <ServerRow server={server} />
              </li>
            ))}
          </ul>
          {servers.length === 0 && (
            <p className="bubble mt-6 max-w-xl">
              Nobody's here. Even the Tediz went home. Run one below, you lazy git.
            </p>
          )}
          <CreateServer />
        </>
      )}
    </motion.main>
  );
}

function ServerRow({ server: s }: { server: GameServer }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const exited = s.status === "exited";

  const kill = async () => {
    setBusy(true);
    try {
      await killServer(s.id);
    } finally {
      await router.invalidate({ sync: true });
      setBusy(false);
    }
  };

  return (
    <div className={`${COLUMNS} board px-5 py-4 ${exited ? "opacity-70 grayscale" : ""}`}>
      <span className="min-w-0">
        <span className="block font-display text-xl tracking-wide text-amber md:truncate">
          {s.name}
        </span>
        <span className="block truncate font-mono text-xs text-dust">{s.address}</span>
      </span>
      <span className="order-3 col-span-2 text-sm text-bone md:order-none md:col-span-1">
        {MAPS[s.map].name} · {MODES[s.mode].name}
      </span>
      <span className="order-4 col-span-2 text-sm text-dust md:order-none md:col-span-1">
        {settingRows(s)
          .map(([label, value]) => `${label} ${value}`)
          .join(" · ")}
      </span>
      <span className="order-2 justify-self-end md:order-none md:justify-self-start">
        <StatusBadge status={s.status} code={s.code} />
      </span>
      <span className="order-5 col-span-2 mt-2 flex gap-2 md:order-none md:col-span-1 md:mt-0 md:justify-self-end">
        {s.status === "running" && (
          <Link
            to="/lobby/$serverId"
            params={{ serverId: s.id }}
            aria-label={`Lobby of ${s.name}`}
            className="pill"
          >
            <LogIn aria-hidden className="size-4" />
            Lobby
          </Link>
        )}
        <button
          type="button"
          onClick={kill}
          disabled={busy}
          aria-label={`${exited ? "Clear" : "Kill"} ${s.name}`}
          className="pill pill-red"
        >
          {exited ? <X aria-hidden className="size-4" /> : <SkullIcon />}
          {exited ? "Clear" : "Kill"}
        </button>
      </span>
    </div>
  );
}

const DEFAULT: NewServer = {
  name: "Bad Fur Day",
  map: "2d",
  mode: "deathmatch",
  settings: { bots: 3, skill: "normal", time: 10, lives: 3, kills: 0, score: 20, radar: true },
};

const range = (lo: number, hi: number) => Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
const TIMES: [Settings["time"], string][] = [
  ...range(3, 10).map((n): [number, string] => [n, `${n} min`]),
  ["unlimited", "Unlimited"],
];

/** The game server's settings (conker-server's flags), and Run. */
function CreateServer() {
  const router = useRouter();
  const [form, setForm] = useState(DEFAULT);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [started, setStarted] = useState("");
  const map = MAPS[form.map];
  const s = form.settings;
  const setting = (change: Partial<Settings>) =>
    setForm({ ...form, settings: { ...s, ...change } });

  const run = async (e: FormEvent) => {
    e.preventDefault();
    setPending(true);
    setError("");
    setStarted("");
    try {
      const server = await createServer(form);
      setStarted(`${server.name} is up at ${server.address}. Go on then.`);
      await router.invalidate({ sync: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <section aria-labelledby="create" className="board mt-12 p-6 sm:p-8">
      <p className="font-stencil text-sm tracking-widest text-lime uppercase">Set-up</p>
      <h2 id="create" className="cartoon font-display text-4xl text-amber">
        Create server
      </h2>
      <p className="mt-1 text-sm text-dust">
        Starts a game server on this machine. Several can run at once, each on its own port.
      </p>
      <form onSubmit={run} className="mt-6 grid gap-x-4 gap-y-5 sm:grid-cols-2 lg:grid-cols-4">
        <label className="grid content-start gap-1 sm:col-span-2">
          <span className={LABEL}>Name</span>
          <input
            name="name"
            required
            maxLength={32}
            autoComplete="off"
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
            className="field"
          />
        </label>
        <Select
          label="Map"
          value={form.map}
          options={(Object.keys(MAPS) as MapId[]).sort().map((id) => [id, MAPS[id].name])}
          onChange={(id) => {
            const modes = MAPS[id].modes;
            const mode = modes.includes(form.mode) ? form.mode : modes[0];
            setForm({ ...form, map: id, mode, settings: fit(s, mode) });
          }}
        />
        <Select
          label="Mode"
          value={form.mode}
          options={map.modes.map((m) => [m, MODES[m].name])}
          onChange={(mode) => setForm({ ...form, mode, settings: fit(s, mode) })}
        />
        <Select
          label="Bots"
          value={s.bots}
          options={range(0, MODES[form.mode].maxBots ?? 15).map((n) => [n, n ? String(n) : "None"])}
          onChange={(bots) => setting({ bots })}
        />
        <Select
          label="Bot skill"
          value={s.skill}
          options={SKILLS.map((k) => [k, k[0].toUpperCase() + k.slice(1)])}
          onChange={(skill) => setting({ skill })}
          hint={s.bots ? undefined : "No bots"}
        />
        <Select
          label="Time"
          value={s.time}
          options={TIMES}
          onChange={(time) => setting({ time })}
        />
        <Select
          label="Lives"
          value={s.lives}
          options={range(1, 5).map((n) => [n, String(n)])}
          onChange={(lives) => setting({ lives })}
          hint={
            !MODES[form.mode].lives
              ? `Never lost in ${MODES[form.mode].name}`
              : s.time === "unlimited"
                ? undefined
                : "Only with unlimited time"
          }
        />
        <Select
          label="Kill limit"
          value={s.kills}
          options={range(0, 10).map((n) => [n, n ? String(n) : "None"])}
          onChange={(kills) => setting({ kills })}
          hint={MODES[form.mode].kills ? undefined : `None in ${MODES[form.mode].name}`}
        />
        <Select
          label="Score"
          value={s.score}
          options={[
            ...range(1, MODES[form.mode].maxScore ?? 40).map((n): [number, string] => [
              n,
              String(n),
            ]),
            ...(MODES[form.mode].maxScore
              ? []
              : [["unlimited", "Unlimited"] as [Settings["score"], string]]),
          ]}
          onChange={(score) => setting({ score })}
          hint={MODES[form.mode].score ? undefined : `None in ${MODES[form.mode].name}`}
        />
        <label className="grid content-start gap-1">
          <span className={LABEL}>Radar</span>
          <span className="field flex items-center gap-3">
            <input
              type="checkbox"
              name="radar"
              checked={s.radar}
              onChange={(e) => setting({ radar: e.target.checked })}
              className="size-5 accent-lime"
            />
            On the HUD
          </span>
        </label>
        {map.note && <p className="text-sm text-dust sm:col-span-2 lg:col-span-4">{map.note}</p>}
        <div className="flex flex-wrap items-center gap-4 sm:col-span-2 lg:col-span-4">
          <button type="submit" disabled={pending} className="cta">
            <Play aria-hidden className="size-5" />
            {pending ? "Starting…" : "Run it"}
          </button>
          <output className="text-sm text-lime">{started}</output>
        </div>
        {error && (
          <pre
            role="alert"
            className="overflow-x-auto rounded-xl border-3 border-tediz bg-black/70 p-4 text-sm whitespace-pre-wrap text-tediz sm:col-span-2 lg:col-span-4"
          >
            {error}
          </pre>
        )}
      </form>
    </section>
  );
}

/** A labelled select over typed values; with a hint, it is off (the setting doesn't apply). */
function Select<T extends string | number>({
  label,
  value,
  options,
  onChange,
  hint,
}: {
  label: string;
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
  hint?: string;
}): ReactNode {
  return (
    <label className="grid content-start gap-1">
      <span className={LABEL}>{label}</span>
      <select
        value={String(value)}
        disabled={hint !== undefined}
        onChange={(e) => {
          const found = options.find(([v]) => String(v) === e.target.value);
          if (found) onChange(found[0]);
        }}
        className="field"
      >
        {options.map(([v, text]) => (
          <option key={String(v)} value={String(v)}>
            {text}
          </option>
        ))}
      </select>
      {hint && <span className="text-xs text-dust">{hint}</span>}
    </label>
  );
}
