// The server browser's and the lobby's data: the game servers the dev web server (`server.ts`,
// `bun run serve`) started, from its /api/servers. server.ts validates against the tables below, so
// this module touches no browser global when it loads.

export type Side = "squirrels" | "tediz";

export type Mode = "deathmatch" | "total-war" | "colors" | "beach" | "heist" | "tank";
/** `conker-server --mode`; whether the mode has a kill limit, lives to lose and a score target,
 * and its own limits (War: Heist's 1-5 bags, default 3, no unlimited; Heist and Tank: at most 7
 * bots). */
export interface ModeInfo {
  name: string;
  kills: boolean;
  lives: boolean;
  score: boolean;
  maxScore?: number;
  defaultScore?: number;
  maxBots?: number;
}
export const MODES: Record<Mode, ModeInfo> = {
  deathmatch: { name: "Deathmatch", kills: true, lives: true, score: false },
  "total-war": { name: "War: Total War", kills: false, lives: true, score: false },
  colors: { name: "War: Colors", kills: false, lives: false, score: true },
  beach: { name: "War: Beach", kills: false, lives: false, score: true },
  heist: {
    name: "War: Heist",
    kills: false,
    lives: true,
    score: true,
    maxScore: 5,
    defaultScore: 3,
    maxBots: 7,
  },
  tank: { name: "Tank", kills: true, lives: true, score: false, maxBots: 7 },
};

export type MapId = "1a" | "24" | "2b" | "2d" | "30" | "34" | "3f";
/** `conker-server --map`, and the modes each map plays: a new mode goes into its maps' lists. */
export const MAPS: Record<MapId, { name: string; modes: Mode[]; note?: string }> = {
  "1a": {
    name: "Vault 0x1A",
    modes: ["deathmatch", "heist"],
    note: "Everyone is the Wise Guy, holding the bat. Heist: 4 teams, at most 7 bots, 1-5 bags.",
  },
  "24": {
    name: "Tank 0x24",
    modes: ["tank"],
    note: "Everyone is a tank, free for all; the bomb gasses everyone outside a bunker. At most 7 bots.",
  },
  "2b": { name: "Beach 0x2B", modes: ["deathmatch", "beach"] },
  "2d": { name: "Arena 0x2D", modes: ["deathmatch", "total-war"] },
  "30": { name: "Colors 0x30", modes: ["deathmatch", "colors"] },
  "34": { name: "Bunker 0x34", modes: ["deathmatch"] },
  "3f": { name: "Temple 0x3F", modes: ["deathmatch"], note: "The bots are cavemen on this map." },
};

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
  /** The score to win, 1-40, or none: only in modes that have one (War: Colors, War: Beach; War:
   * Heist's bags 1-5). */
  score: number | "unlimited";
  radar: boolean;
}

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
  /** The game server's own /info, when it answered. */
  info?: { map: string; protocol: number };
}

/** `s` within `mode`'s limits: fewer bots, and its default score for one out of its range. */
export function fit(s: Settings, mode: Mode): Settings {
  const { maxScore, defaultScore, maxBots } = MODES[mode];
  const over = maxScore !== undefined && (s.score === "unlimited" || s.score > maxScore);
  return {
    ...s,
    bots: Math.min(s.bots, maxBots ?? s.bots),
    score: over ? (defaultScore ?? maxScore) : s.score,
  };
}

/** The settings that apply to this server's mode, as label and value. */
export function settingRows({ mode, settings: s }: NewServer): [string, string][] {
  const rows: [string, string][] = [
    ["Bots", s.bots ? `${s.bots}, ${s.skill}` : "none"],
    ["Time", s.time === "unlimited" ? "unlimited" : `${s.time} min`],
  ];
  if (s.time === "unlimited" && MODES[mode].lives) rows.push(["Lives", String(s.lives)]);
  if (MODES[mode].kills) rows.push(["Kill limit", s.kills ? String(s.kills) : "none"]);
  if (MODES[mode].score) rows.push(["Score", String(s.score)]);
  rows.push(["Radar", s.radar ? "on" : "off"]);
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
