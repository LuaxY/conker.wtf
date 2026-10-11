import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { Monitor, RefreshCw, Search, Users } from "lucide-react";
import { motion } from "motion/react";
import { useState } from "react";
import type { Server } from "./data";
import { button, field, Ping, StatusBadge } from "./ui";

const route = getRouteApi("/servers");

const SORTS = {
  ping: { label: "Ping", compare: (a: Server, b: Server) => a.ping - b.ping },
  players: { label: "Players", compare: (a: Server, b: Server) => b.players - a.players },
  name: { label: "Name", compare: (a: Server, b: Server) => a.name.localeCompare(b.name) },
};
type Sort = keyof typeof SORTS;

// One grid for the header and the rows: stacked on phones, columns from md up.
const COLUMNS =
  "grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 md:grid-cols-[minmax(0,2.4fr)_1fr_1.4fr_5rem_5.5rem_6rem]";

export function Servers() {
  const servers = route.useLoaderData();
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("ping");
  const [refreshing, setRefreshing] = useState(false);

  const q = query.trim().toLowerCase();
  const shown = servers
    .filter((s) => [s.name, s.region, s.map, s.mode].some((f) => f.toLowerCase().includes(q)))
    // "This machine" stays on top.
    .sort(
      (a, b) => Number(b.id === "local") - Number(a.id === "local") || SORTS[sort].compare(a, b),
    );
  const online = servers.reduce((n, s) => n + s.players, 0);

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
      <h1 className="cartoon font-display text-5xl">Servers</h1>
      <p className="mt-1 text-dust">
        {servers.length} servers, {online} players online. Pick one to meet its lobby.
      </p>

      <div className="mt-6 flex flex-wrap items-end gap-3">
        <label className="grid min-w-56 flex-1 gap-1">
          <span className="font-stencil text-sm text-dust uppercase">Search</span>
          <span className="relative">
            <Search
              aria-hidden
              className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-dust"
            />
            <input
              type="search"
              name="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Name, region, map or mode"
              className={`${field} pl-9`}
            />
          </span>
        </label>
        <label className="grid gap-1">
          <span className="font-stencil text-sm text-dust uppercase">Sort by</span>
          <select
            name="sort"
            value={sort}
            onChange={(e) => setSort(e.target.value as Sort)}
            className={`${field} w-auto`}
          >
            {Object.entries(SORTS).map(([id, { label }]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          onClick={refresh}
          disabled={refreshing}
          className={`${button} h-11 border-2 border-mud bg-trench px-4 hover:border-acorn`}
        >
          <RefreshCw aria-hidden className={`size-4 ${refreshing ? "animate-spin" : ""}`} />
          {refreshing ? "Refreshing" : "Refresh"}
        </button>
      </div>

      <div
        aria-hidden
        className={`${COLUMNS} mt-8 hidden px-4 font-stencil text-xs text-dust uppercase md:grid`}
      >
        <span>Server</span>
        <span>Region</span>
        <span>Map · mode</span>
        <span>Players</span>
        <span>Ping</span>
        <span>Status</span>
      </div>
      <ul className="mt-2 grid gap-2">
        {shown.map((server) => (
          <li key={server.id}>
            <ServerRow server={server} />
          </li>
        ))}
      </ul>
      {shown.length === 0 && (
        <p className="mt-6 rounded-xl border-2 border-dashed border-mud p-6 text-center text-dust">
          No server matches “{query}”.
        </p>
      )}
    </motion.main>
  );
}

function ServerRow({ server: s }: { server: Server }) {
  const local = s.id === "local";
  const label = `${s.name}, ${s.region}, ${s.players} of ${s.maxPlayers} players, ${s.ping} ms${s.status === "in-game" ? ", in game" : ""}`;
  const cells = (
    <>
      <span className="min-w-0">
        <span className="flex items-center gap-2 font-semibold">
          {local && <Monitor aria-hidden className="size-4 shrink-0 text-acorn" />}
          <span className="md:truncate">{s.name}</span>
        </span>
        {local && <span className="block truncate font-mono text-xs text-dust">{s.address}</span>}
      </span>
      <span className="order-3 text-sm text-dust md:order-none md:text-base md:text-khaki">
        {s.region}
      </span>
      <span className="order-4 text-sm text-dust md:order-none">
        {s.map} · {s.mode}
      </span>
      <span className="order-5 inline-flex items-center gap-1.5 tabular-nums md:order-none">
        <Users aria-hidden className="size-4 text-dust" />
        {s.players}/{s.maxPlayers}
      </span>
      <span className="order-6 md:order-none">
        <Ping ms={s.ping} />
      </span>
      <span className="order-2 justify-self-end md:order-none md:justify-self-start">
        <StatusBadge status={s.status} />
      </span>
    </>
  );
  const box = `${COLUMNS} rounded-xl border-2 bg-trench px-4 py-3`;
  if (s.status === "full") {
    return <div className={`${box} border-mud opacity-60`}>{cells}</div>;
  }
  return (
    <Link
      to="/lobby/$serverId"
      params={{ serverId: s.id }}
      aria-label={label}
      className={`${box} transition hover:-translate-y-0.5 hover:border-acorn hover:bg-mud ${local ? "border-acorn/60" : "border-mud"}`}
    >
      {cells}
    </Link>
  );
}
