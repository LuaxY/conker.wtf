// The dev web server (`bun run serve`): serves web/ as `python3 -m http.server --directory web` did,
// and starts, lists and kills game servers for the landing at /api/servers (web/README.md). A
// tool that runs processes on request, so it listens on loopback only unless --host says otherwise:
// on a host, only behind Caddy and its password (deploy/README.md). Its game servers live in memory
// and die with it.
//
//   bun run serve [--port 8080] [--host localhost] [--game-ports 5900-5949]
//                 [--server ../../target-net/release/conker-server] [--assets ../../assets]
//                 [--cert PEM --cert-key PEM --token-key FILE --public IP...]   (production)
import { closeSync, mkdtempSync, openSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import {
  CHEATS,
  type Cheat,
  type GameServer,
  MAPS,
  type MapId,
  type Mode,
  MODES,
  NAMES,
  type NewServer,
  SKILLS,
} from "./src/data";

const WEB = resolve(import.meta.dir, "..");
const ROOT = resolve(WEB, "..");
const { values: args } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    port: { type: "string", default: "8080" },
    host: { type: "string", default: "localhost" },
    "game-ports": { type: "string", default: "5900-5949" },
    server: { type: "string", default: join(ROOT, "target-net/release/conker-server") },
    assets: { type: "string", default: join(ROOT, "assets") },
    // Production: each game server gets the domain's certificate (no digest), the token key, its
    // token endpoint on loopback (proxied below) and the addresses the tokens name.
    cert: { type: "string" },
    "cert-key": { type: "string" },
    "token-key": { type: "string" },
    public: { type: "string", multiple: true },
  },
});
const PROD = args.cert !== undefined;
if (PROD && !(args["cert-key"] && args["token-key"] && args.public?.length))
  throw new Error("--cert needs --cert-key, --token-key and --public");
const BINARY = resolve(args.server);
const ASSETS = resolve(args.assets);
const [LO, HI] = args["game-ports"].split("-").map(Number);
if (!(LO > 0 && HI > LO && HI < 65536)) throw new Error("--game-ports: LO-HI, e.g. 5900-5949");
/** Seconds a game server may take to answer /info (it reads the asset pack first). */
const START_SECS = 30;
/** Minutes a running game server may go without a human in its match before it is stopped. */
const IDLE_MINUTES = 5;

interface Running {
  record: Omit<GameServer, "address" | "info">;
  proc: Bun.Subprocess;
  dir: string;
  /** When its /info last counted a human (its start until one joins). */
  lastHuman: number;
}
const servers = new Map<string, Running>();
let nextId = 1;

const fail = (status: number, error: string) => Response.json({ error }, { status });
const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const int = (v: unknown, lo: number, hi: number): v is number =>
  Number.isInteger(v) && (v as number) >= lo && (v as number) <= hi;
const isMap = (v: unknown): v is MapId => typeof v === "string" && Object.hasOwn(MAPS, v);
const isMode = (v: unknown): v is Mode => typeof v === "string" && Object.hasOwn(MODES, v);
const isSkill = (v: unknown): v is (typeof SKILLS)[number] => SKILLS.some((s) => s === v);
const isCheat = (v: unknown): v is Cheat => typeof v === "string" && Object.hasOwn(CHEATS, v);
// The name never reaches the command line; still, nothing a shell or a page would read as code.
const NAME = /^[\p{L}\p{N} _.,'!?-]{1,32}$/u;
const badName = NAMES.find((n) => !NAME.test(n));
if (badName !== undefined) throw new Error(`src/data.ts NAMES: "${badName}" fails NAME`);

/** Every value whitelisted: the request, or what is wrong with it. */
function validate(body: unknown): NewServer | string {
  if (!isObject(body) || !isObject(body.settings)) return "expected {name, map, mode, settings}";
  const { name, map, mode } = body;
  const { bots, skill, time, lives, kills, score, radar, laps, route, cheats } = body.settings;
  if (typeof name !== "string" || !NAME.test(name))
    return "name: 1-32 letters, digits, spaces or _.,'!?-";
  if (!isMap(map)) return `map: one of ${Object.keys(MAPS).sort().join(", ")}`;
  if (!isMode(mode) || !MAPS[map].modes.includes(mode))
    return `mode: on map ${map}, one of ${MAPS[map].modes.join(", ")}`;
  if (!int(bots, 0, 15)) return "settings.bots: 0-15";
  if (!isSkill(skill)) return `settings.skill: one of ${SKILLS.join(", ")}`;
  if (time !== "unlimited" && !int(time, 3, 10)) return 'settings.time: 3-10 or "unlimited"';
  if (!int(lives, 1, 5)) return "settings.lives: 1-5";
  if (!int(kills, 0, 10)) return "settings.kills: 0-10";
  if (score !== "unlimited" && !int(score, 1, 40)) return 'settings.score: 1-40 or "unlimited"';
  const { name: modeName, maxScore, maxBots } = MODES[mode];
  if (maxScore !== undefined && (score === "unlimited" || score > maxScore))
    return `settings.score: 1-${maxScore} in ${modeName}`;
  if (bots > maxBots[0]) return `settings.bots: 0-${maxBots[0]} in ${modeName}`;
  if (typeof radar !== "boolean") return "settings.radar: true or false";
  if (!int(laps, 1, 20)) return "settings.laps: 1-20";
  if (route !== 11 && route !== 12) return "settings.route: 11 or 12";
  if (!Array.isArray(cheats) || !cheats.every(isCheat) || new Set(cheats).size < cheats.length)
    return `settings.cheats: distinct codes of ${Object.keys(CHEATS).join(", ")}`;
  return {
    name,
    map,
    mode,
    settings: { bots, skill, time, lives, kills, score, radar, laps, route, cheats },
  };
}

/** Whether TCP `port` and `port + 1` are free (a conker-server holds UDP `port` with its TCP). */
function free(port: number): boolean {
  try {
    for (const p of [port, port + 1])
      Bun.listen({ hostname: "::", port: p, socket: { data() {} } }).stop(true);
    return true;
  } catch {
    return false;
  }
}

/** The first even port of the range whose pair is free; synchronous, so two requests never share one. */
function pickPort(): number | undefined {
  const live = [...servers.values()].filter((s) => s.record.status !== "exited");
  const taken = new Set(live.map((s) => s.record.port));
  for (let p = LO + (LO % 2); p + 1 <= HI; p += 2) if (!taken.has(p) && free(p)) return p;
}

async function info(port: number, ms: number): Promise<GameServer["info"]> {
  const res = await fetch(`http://127.0.0.1:${port + 1}/info`, {
    signal: AbortSignal.timeout(ms),
  }).catch(() => null);
  return res?.ok ? ((await res.json()) as GameServer["info"]) : undefined;
}

function spawn({ name, map, mode, settings: s }: NewServer, port: number): Running {
  const dir = mkdtempSync(join(tmpdir(), "conker-server-"));
  const log = openSync(join(dir, "server.log"), "a");
  // An argv array, never a shell line; every value is whitelisted above.
  const argv = Object.entries({
    assets: ASSETS,
    map,
    mode,
    port,
    ...(PROD
      ? {
          cert: args.cert,
          "cert-key": args["cert-key"],
          "token-key": args["token-key"],
          "token-addr": `127.0.0.1:${port + 1}`,
        }
      : { "digest-file": join(dir, "digest.txt") }),
    bots: s.bots,
    skill: s.skill,
    lives: s.lives,
    radar: s.radar ? "on" : "off",
    // Race is untimed, without a kill limit or score: its laps and route instead.
    ...(MODES[mode].laps
      ? { laps: s.laps, route: s.route }
      : { time: s.time, kills: s.kills, score: s.score }),
    ...(s.cheats.length ? { cheats: s.cheats.join(",") } : {}),
  })
    .flatMap(([flag, value]) => [`--${flag}`, String(value)])
    .concat((args.public ?? []).flatMap((ip) => ["--public", ip]));
  const proc = Bun.spawn([BINARY, ...argv], {
    cwd: ROOT,
    env: { ...process.env, NO_COLOR: "1" },
    stdin: "ignore",
    // A file, not a pipe: nobody reads a pipe, and a full one would stall the server.
    stdout: log,
    stderr: log,
  });
  closeSync(log);
  const record: Running["record"] = {
    id: String(nextId++),
    name,
    map,
    mode,
    settings: s,
    port,
    digest: "",
    status: "starting",
    code: null,
    started: new Date().toISOString(),
  };
  const running = { record, proc, dir, lastHuman: Date.now() };
  servers.set(record.id, running);
  console.log(`game server ${record.id} "${name}": pid ${proc.pid}, port ${port}, ${dir}`);
  void proc.exited.then((code) => {
    Object.assign(record, { status: "exited", code });
    console.log(`game server ${record.id} "${name}" exited (${code})`);
  });
  return running;
}

/** Waits until the server answers /info (it writes its digest just before): false if it died or took too long. */
async function started({ record }: Running): Promise<boolean> {
  const until = Date.now() + START_SECS * 1000;
  while (record.status === "starting" && Date.now() < until) {
    if (await info(record.port, 500)) return true;
    await Bun.sleep(150);
  }
  return false;
}

/** SIGTERM, then SIGKILL after 3 s; then its temp files go. */
async function stop({ proc, dir }: Running) {
  proc.kill("SIGTERM");
  const timeout = Bun.sleep(3000).then(() => "timeout" as const);
  if ((await Promise.race([proc.exited, timeout])) === "timeout") {
    proc.kill("SIGKILL");
    await proc.exited;
  }
  rmSync(dir, { recursive: true, force: true });
}

/** The log's last lines ("" once a DELETE removed it). */
function tail({ dir }: Running): string {
  try {
    return readFileSync(join(dir, "server.log"), "utf8")
      .trimEnd()
      .split("\n")
      .slice(-15)
      .join("\n");
  } catch {
    return "";
  }
}

const view = ({ record }: Running, host: string, live?: GameServer["info"]): GameServer => ({
  ...record,
  address: `${host}:${record.port}`,
  info: live,
});

const api = {
  "/api/servers": {
    async GET(req: Request) {
      const host = new URL(req.url).hostname;
      const list = [...servers.values()].map(async (s) =>
        view(s, host, s.record.status === "running" ? await info(s.record.port, 300) : undefined),
      );
      return Response.json(await Promise.all(list));
    },
    async POST(req: Request) {
      // JSON only: a cross-site page can't send it without a CORS preflight, which this server never allows.
      if (!req.headers.get("content-type")?.startsWith("application/json"))
        return fail(415, "send JSON (content-type: application/json)");
      const body = validate(await req.json().catch(() => null));
      if (typeof body === "string") return fail(400, body);
      const port = pickPort();
      if (port === undefined) return fail(503, `no free port pair in ${LO}-${HI}`);
      const s = spawn(body, port);
      if (await started(s)) {
        if (!PROD) s.record.digest = readFileSync(join(s.dir, "digest.txt"), "utf8").trim();
        s.record.status = "running";
        const live = await info(port, 500);
        return Response.json(view(s, new URL(req.url).hostname, live), { status: 201 });
      }
      const why =
        s.record.status === "exited" ? `exited (${s.record.code})` : `took over ${START_SECS} s`;
      const log = tail(s);
      servers.delete(s.record.id);
      await stop(s);
      return fail(500, `conker-server ${why} before answering /info:\n${log}`);
    },
  },
  "/api/servers/:id": {
    async DELETE(req: Bun.BunRequest<"/api/servers/:id">) {
      const s = servers.get(req.params.id);
      if (!s) return fail(404, `no game server ${req.params.id}`);
      servers.delete(req.params.id);
      await stop(s);
      return new Response(null, { status: 204 });
    },
  },
  // The game server's connect token and /info (the lobby's `token-url`), from its token endpoint
  // on loopback, with the page's address for its rate limit (Caddy's X-Forwarded-For).
  "/api/servers/:id/:endpoint": {
    async GET(req: Bun.BunRequest<"/api/servers/:id/:endpoint">) {
      const { id, endpoint } = req.params;
      const s = servers.get(id);
      if (!s || s.record.status !== "running" || !["token", "info"].includes(endpoint))
        return fail(404, `no game server ${id} ${endpoint}`);
      const forwarded = req.headers.get("x-forwarded-for");
      const url = `http://127.0.0.1:${s.record.port + 1}/${endpoint}${new URL(req.url).search}`;
      const res = await fetch(url, {
        headers: forwarded ? { "x-forwarded-for": forwarded } : {},
        signal: AbortSignal.timeout(5000),
      }).catch(() => null);
      return res ?? fail(502, `game server ${id} is not answering`);
    },
  },
};

/** web/ as http.server serves it: files (the assets/ symlink followed), a folder's index.html. */
async function staticFile(req: Request): Promise<Response> {
  const notFound = new Response("Not found\n", { status: 404 });
  try {
    const path = decodeURIComponent(new URL(req.url).pathname);
    if (path.split("/").includes("..")) return notFound;
    const file = Bun.file(join(WEB, path.endsWith("/") ? `${path}index.html` : path));
    return (await file.exists()) ? new Response(file) : notFound;
  } catch {
    return notFound;
  }
}

const loopback = ["localhost", "127.0.0.1", "::1"].includes(args.host);
const hosts = args.host === "localhost" ? ["127.0.0.1", "::1"] : [args.host];
const listening: ReturnType<typeof Bun.serve>[] = [];
for (const hostname of hosts) {
  try {
    listening.push(
      Bun.serve({ hostname, port: Number(args.port), routes: api, fetch: staticFile }),
    );
  } catch (e) {
    // Loopback over IPv6 may be off: IPv4's is enough.
    if (hostname !== "::1") throw e;
  }
}
for (const s of listening) console.log(`serving ${WEB} on ${s.url}`);
if (!loopback)
  console.warn(
    `\nWARNING: listening on ${args.host}, not loopback. Anyone who reaches this port can start and kill\n` +
      "game servers on this machine. This dev tool has no authentication: never expose it to the internet.\n",
  );
console.log(`game servers: ${BINARY}, ports ${LO}-${HI}, assets ${ASSETS}`);

// Every 15 s, a running game server without a human (bots don't count) for IDLE_MINUTES stops, as a
// DELETE would; an /info that doesn't answer counts no human.
setInterval(() => {
  for (const s of servers.values()) {
    if (s.record.status !== "running") continue;
    void info(s.record.port, 2000).then((live) => {
      if (live?.humans) s.lastHuman = Date.now();
      else if (
        Date.now() - s.lastHuman >= IDLE_MINUTES * 60_000 &&
        servers.get(s.record.id) === s
      ) {
        servers.delete(s.record.id);
        console.log(
          `game server ${s.record.id} "${s.record.name}": no human for ${IDLE_MINUTES} min, stopped`,
        );
        void stop(s);
      }
    });
  }
}, 15_000);

// Ctrl-C or a SIGTERM stops every game server first.
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, async () => {
    console.log(`\n${signal}: stopping ${servers.size} game server(s)`);
    for (const s of listening) s.stop(true);
    await Promise.all([...servers.values()].map(stop));
    process.exit(0);
  });
// Any other exit (a crash): at least no process is left behind.
process.on("exit", () => {
  for (const { proc } of servers.values()) proc.kill("SIGKILL");
});
