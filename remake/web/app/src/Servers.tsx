import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { LogIn, RefreshCw, X } from "lucide-react";
import { motion } from "motion/react";
import { useState } from "react";
import { CreateServer } from "./CreateServer";
import { type GameServer, killServer, MAPS, MODES, players, settingRows } from "./data";
import { SkullIcon, StatusBadge, useRefresh } from "./ui";

const route = getRouteApi("/servers");
const adminRoute = getRouteApi("/admin");

// One grid for the header and the rows: stacked on phones, columns from md up.
const COLUMNS =
  "grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 md:grid-cols-[minmax(0,1.5fr)_1.1fr_1.8fr_6.5rem_12rem]";

/** The unlinked admin page (/#/admin): the same list with Kill and Clear, and no Create server. */
export function Admin() {
  return <Servers admin />;
}

export function Servers({ admin = false }: { admin?: boolean }) {
  const all = (admin ? adminRoute : route).useLoaderData();
  // Players see the live ones only: without Clear, an exited one would stay listed for good.
  const servers = admin ? all : (all?.filter((s) => s.status !== "exited") ?? null);
  const router = useRouter();
  const [refreshing, setRefreshing] = useState(false);
  useRefresh();

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
          <h1 className="chrome font-display text-6xl [--c:#00c0f0]">
            {admin ? "Admin" : "Servers"}
          </h1>
          <p className="mt-2 text-dust">
            {servers === null
              ? "No server list on this site."
              : `${servers.filter((s) => s.status === "running").length} running. Open a lobby to join, or run your own below.`}
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
                <ServerRow server={server} admin={admin} />
              </li>
            ))}
          </ul>
          {servers.length === 0 && (
            <p className="bubble mt-6 max-w-xl">
              Nobody's here. Even the Tediz went home.{admin ? "" : " Run one below, you lazy git."}
            </p>
          )}
          {!admin && <CreateServer />}
        </>
      )}
    </motion.main>
  );
}

function ServerRow({ server: s, admin }: { server: GameServer; admin: boolean }) {
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
      <span className="min-w-0 font-display text-xl tracking-wide text-amber md:truncate">
        {s.name}
      </span>
      <span className="order-3 col-span-2 text-sm text-bone md:order-none md:col-span-1">
        {MAPS[s.map].name} · {MODES[s.mode].name}
        {players(s) && <span className="block text-lime">{players(s)}</span>}
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
        {admin && (
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
        )}
      </span>
    </div>
  );
}
