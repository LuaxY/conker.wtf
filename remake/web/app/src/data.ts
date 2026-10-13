// The server browser's and the lobby's data: the game servers the dev web server (`server.ts`,
// `bun run serve`) started, from its /api/servers. server.ts validates against the tables below, so
// this module touches no browser global when it loads.

export type Side = "squirrels" | "tediz";

export type Mode =
  | "deathmatch"
  | "total-war"
  | "colors"
  | "beach"
  | "heist"
  | "tank"
  | "raptor"
  | "race";
/** `conker-server --mode`; whether the mode has a kill limit, lives to lose and a score target,
 * and its own limits (War: Heist's 1-5 bags, default 3, no unlimited; War: Raptor's 1-40, default
 * 20, no unlimited); Race's laps and route instead of a clock. `maxBots`: the most bots with 1-4
 * humans in, as the server caps them (the original's bots screen: Deathmatch 10/8/4/4 players,
 * Tank and Race 8/4/4/4; its team screen's rows in the War modes, the humans spread over the
 * teams); `--bots` takes the first. */
export interface ModeInfo {
  name: string;
  kills: boolean;
  lives: boolean;
  score: boolean;
  maxScore?: number;
  defaultScore?: number;
  maxBots: [number, number, number, number];
  laps?: boolean;
}
export const MODES: Record<Mode, ModeInfo> = {
  deathmatch: { name: "Deathmatch", kills: true, lives: true, score: false, maxBots: [9, 6, 1, 0] },
  "total-war": {
    name: "War: Total War",
    kills: false,
    lives: true,
    score: false,
    maxBots: [11, 10, 1, 0],
  },
  colors: { name: "War: Colors", kills: false, lives: false, score: true, maxBots: [11, 10, 1, 0] },
  beach: { name: "War: Beach", kills: false, lives: false, score: true, maxBots: [7, 6, 5, 4] },
  heist: {
    name: "War: Heist",
    kills: false,
    lives: true,
    score: true,
    maxScore: 5,
    defaultScore: 3,
    maxBots: [7, 6, 1, 0],
  },
  tank: { name: "Tank", kills: true, lives: true, score: false, maxBots: [7, 2, 1, 0] },
  raptor: {
    name: "War: Raptor",
    kills: false,
    lives: true,
    score: true,
    maxScore: 40,
    defaultScore: 20,
    maxBots: [7, 6, 3, 2],
  },
  race: {
    name: "Race",
    kills: false,
    lives: true,
    score: false,
    maxBots: [7, 2, 1, 0],
    laps: true,
  },
};

export type MapId = "1a" | "24" | "2b" | "2d" | "30" | "33" | "34" | "3f";
/** `conker-server --map`, and the modes each map plays: a new mode goes into its maps' lists. */
export const MAPS: Record<MapId, { name: string; modes: Mode[]; note?: string }> = {
  "1a": {
    name: "Vault 0x1A",
    modes: ["deathmatch", "heist"],
    note: "Everyone is the Wise Guy, holding the bat. Heist: 4 teams, 1-5 bags.",
  },
  "24": {
    name: "Tank 0x24",
    modes: ["tank"],
    note: "Everyone is a tank, free for all; the bomb gasses everyone outside a bunker.",
  },
  "2b": {
    name: "Beach 0x2B",
    modes: ["deathmatch", "beach"],
    note: "Beach: the villagers escape, at most 2 Tediz shoot (6 villagers against them, more with BALANCE).",
  },
  "2d": { name: "Arena 0x2D", modes: ["deathmatch", "total-war"] },
  "30": { name: "Colors 0x30", modes: ["deathmatch", "colors"] },
  "33": {
    name: "Race 0x33",
    modes: ["race"],
    note: "Hoverboards, free for all: laps of the low road or the elevated one, canisters and missiles.",
  },
  "34": { name: "Bunker 0x34", modes: ["deathmatch"] },
  "3f": {
    name: "Temple 0x3F",
    modes: ["deathmatch", "raptor"],
    note: "The bots are cavemen on this map. Raptor: cavemen against at most 2 raptors (a Tediz pick plays a raptor).",
  },
};

/** The character select's 22 picks (conker-rules `characters::PICKS`), by the index `--character`
 * and play.html's `character=` take (the client also takes the name, lower-case with dashes). */
export const PICKS = [
  "Squirrel",
  "Sergeant",
  "Tedi",
  "Tediz Leader",
  "Wise Guy",
  "Weasel",
  "Caveman",
  "Racer",
  "Racer 76",
  "Racer 78",
  "Raptor",
  "Villager",
  "Bat",
  "Bat 2",
  "Racer 77",
  "Conker",
  "Neo Conker",
  "Villager Woman",
  "Zombie",
  "Zombie 2",
  "Gregg",
  "Gregg 2",
] as const;

/** The cheats that add picks (`--cheats`), each with the picks it adds (bit k: pick k). */
export const CHEATS = {
  EASTEREGGSRUS: 0x10000,
  WELLYTOP: 0x8000,
  CHINDITVICTORY: 0x30,
  EATBOX: 0x43c0,
  BEEFCURTAINS: 0xe0800,
  RUSTYSHERIFFSBADGE: 0xa,
  BILLYMILLROUNDABOUT: 0x300000,
} as const;
export type Cheat = keyof typeof CHEATS;

/** Each map's select (bit k: pick k): its Deathmatch's, Race's on 0x33. None on 0x2B (no Deathmatch
 * in the original) and 0x24 (Tank). */
const SELECTS: Partial<Record<MapId, number>> = {
  "1a": 0x831,
  "2d": 0x815,
  "30": 0x815,
  "33": 0x4380,
  "34": 0x815,
  "3f": 0x5c0,
};

/** The picks a player may play on `map` in `mode` with `cheats` on (bit k: pick k), as the game
 * server's `characters()`: 0 in the team modes and Tank, which have no select. */
export function allowed(map: MapId, mode: Mode, cheats: readonly Cheat[]): number {
  const base = mode === "deathmatch" || mode === "race" ? (SELECTS[map] ?? 0) : 0;
  return base && cheats.reduce((mask, c) => mask | CHEATS[c], base);
}

/** The bots' skill (`--skill`), as the original's SET-UP screen names them. */
export const SKILLS = ["inbred", "crap", "normal", "bastard", "einstein"] as const;
export type Skill = (typeof SKILLS)[number];

export interface Settings {
  /** 0-15. */
  bots: number;
  skill: Skill;
  /** Minutes on the clock, 3-10, or none. */
  time: number | "unlimited";
  /** 1-5, each player's lives: only without a clock. */
  lives: number;
  /** 0-10, the kill limit (0: none): only in modes that have one. */
  kills: number;
  /** The score to win, 1-40, or none: only in modes that have one (War: Colors, War: Beach, War:
   * Raptor without none; War: Heist's bags 1-5). */
  score: number | "unlimited";
  radar: boolean;
  /** Race: 1-20 laps, and the route (`--route`: 11 the low road, 12 the elevated one). */
  laps: number;
  route: Route;
  /** The cheats on (`--cheats`): more picks where the map and mode have a select. */
  cheats: Cheat[];
}

export type Route = 11 | 12;
export const ROUTES: Record<Route, string> = { 11: "Low road", 12: "Elevated" };

/** What POST /api/servers takes. */
export interface NewServer {
  name: string;
  map: MapId;
  mode: Mode;
  settings: Settings;
}

export type Status = "starting" | "running" | "exited";

/** What GET /api/servers lists. */
export interface GameServer extends NewServer {
  id: string;
  /** The game port (WebTransport and WebSocket); its /token and /info are on port + 1. */
  port: number;
  /** host:port, as play.html's `server` option takes it (the host this page was loaded from). */
  address: string;
  /** The WebTransport certificate's SHA-256 (play.html's `digest`): a new one per start. */
  digest: string;
  status: Status;
  /** The exit code once exited (128 + the signal's number when killed). */
  code: number | null;
  /** ISO time. */
  started: string;
  /** The game server's own /info, when it answered: its map, protocol, its pack's fingerprint,
   * and the humans and bots in its match now. */
  info?: { map: string; protocol: number; pack: string; humans: number; bots: number };
}

/** The players in a server's match, from its /info ("1 human, 3 bots"); undefined when it didn't answer. */
export function players({ info }: GameServer): string | undefined {
  if (!info) return undefined;
  const count = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
  return `${count(info.humans, "human")}, ${count(info.bots, "bot")}`;
}

/** `s` within `mode`'s limits: fewer bots, and its default score for one out of its range. */
export function fit(s: Settings, mode: Mode): Settings {
  const { maxScore, defaultScore, maxBots } = MODES[mode];
  const over = maxScore !== undefined && (s.score === "unlimited" || s.score > maxScore);
  return {
    ...s,
    bots: Math.min(s.bots, maxBots[0]),
    score: over ? (defaultScore ?? maxScore) : s.score,
  };
}

/** The settings that apply to this server's mode, as label and value. */
export function settingRows({ map, mode, settings: s }: NewServer): [string, string][] {
  const { laps } = MODES[mode];
  const rows: [string, string][] = [["Bots", s.bots ? `${s.bots}, ${s.skill}` : "none"]];
  if (laps) rows.push(["Laps", String(s.laps)], ["Route", ROUTES[s.route]]);
  else rows.push(["Time", s.time === "unlimited" ? "unlimited" : `${s.time} min`]);
  if ((laps || s.time === "unlimited") && MODES[mode].lives) rows.push(["Lives", String(s.lives)]);
  if (MODES[mode].kills) rows.push(["Kill limit", s.kills ? String(s.kills) : "none"]);
  if (MODES[mode].score) rows.push(["Score", String(s.score)]);
  rows.push(["Radar", s.radar ? "on" : "off"]);
  if (s.cheats.length && allowed(map, mode, [])) rows.push(["Cheats", String(s.cheats.length)]);
  return rows;
}

/** The servers, or null on a site without the dev web server (a plain static server). */
export async function listServers(): Promise<GameServer[] | null> {
  const res = await fetch("api/servers", { cache: "no-store" }).catch(() => null);
  if (!res?.ok || !res.headers.get("content-type")?.includes("json")) return null;
  return (await res.json()) as GameServer[];
}

export async function getServer(id: string): Promise<GameServer | undefined> {
  return (await listServers())?.find((s) => s.id === id);
}

async function send(path: string, init: RequestInit): Promise<Response> {
  const res = await fetch(path, init);
  if (res.ok) return res;
  const body = (await res.json().catch(() => null)) as { error?: string } | null;
  throw new Error(body?.error ?? `${res.status} ${res.statusText}`);
}

/** Starts a game server; resolves once it is listening. */
export async function createServer(server: NewServer): Promise<GameServer> {
  const res = await send("api/servers", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(server),
  });
  return (await res.json()) as GameServer;
}

/** Kills a game server (or clears an exited one). */
export async function killServer(id: string): Promise<void> {
  await send(`api/servers/${encodeURIComponent(id)}`, { method: "DELETE" });
}
