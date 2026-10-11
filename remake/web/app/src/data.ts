// The server browser's and the lobby's data. Mock for now, shaped like the endpoints that will serve
// it later (a server list, then each server's lobby). Only "This machine" is real: the game server on
// this page's host, which play.html connects to.

export type Side = "squirrels" | "tediz";
export type Status = "open" | "full" | "in-game";

export interface Server {
  id: string;
  name: string;
  region: string;
  /** host:port, as play.html's `server` option takes it. */
  address: string;
  map: string;
  mode: string;
  players: number;
  maxPlayers: number;
  /** Round trip, ms. */
  ping: number;
  status: Status;
}

export interface LobbyPlayer {
  id: string;
  name: string;
  side: Side;
  ready: boolean;
  ping: number;
  host: boolean;
}

export interface ChatMessage {
  id: string;
  from: string;
  side?: Side;
  text: string;
}

export interface Lobby {
  server: Server;
  players: LobbyPlayer[];
  chat: ChatMessage[];
}

/** The game server's default port (conker-server --port). */
const GAME_PORT = 5888;

// The landing's own ?server=host:port moves "This machine" (e.g. a server started on another port).
const localAddress =
  new URLSearchParams(location.search).get("server") ?? `${location.hostname}:${GAME_PORT}`;

const MAP = "Arena 0x2D";
const MODE = "Deathmatch";

const MOCK_SERVERS: Omit<Server, "map" | "mode">[] = [
  {
    id: "acorn-front",
    name: "Acorn Front",
    region: "EU West",
    address: "eu-west.conker.example:5888",
    players: 5,
    maxPlayers: 8,
    ping: 34,
    status: "open",
  },
  {
    id: "shc-hq",
    name: "Squirrel High Command",
    region: "EU Central",
    address: "eu-central.conker.example:5888",
    players: 8,
    maxPlayers: 8,
    ping: 41,
    status: "full",
  },
  {
    id: "tediz-bunker",
    name: "Tediz Bunker",
    region: "EU Central",
    address: "eu-central-2.conker.example:5888",
    players: 6,
    maxPlayers: 8,
    ping: 47,
    status: "in-game",
  },
  {
    id: "bad-fur-night",
    name: "Bad Fur Night",
    region: "US East",
    address: "us-east.conker.example:5888",
    players: 3,
    maxPlayers: 8,
    ping: 92,
    status: "open",
  },
  {
    id: "total-war",
    name: "Total War 24/7",
    region: "US West",
    address: "us-west.conker.example:5888",
    players: 7,
    maxPlayers: 8,
    ping: 148,
    status: "in-game",
  },
  {
    id: "rookies",
    name: "Rookies Only (be nice)",
    region: "US East",
    address: "us-east-2.conker.example:5888",
    players: 1,
    maxPlayers: 4,
    ping: 101,
    status: "open",
  },
  {
    id: "beach-landing",
    name: "Beach Landing",
    region: "South America",
    address: "sa-east.conker.example:5888",
    players: 4,
    maxPlayers: 4,
    ping: 186,
    status: "full",
  },
  {
    id: "panther-table",
    name: "The Panther King's Table",
    region: "Asia",
    address: "ap-east.conker.example:5888",
    players: 2,
    maxPlayers: 8,
    ping: 231,
    status: "open",
  },
  {
    id: "cheese-run",
    name: "Great Mighty Cheese Run",
    region: "Oceania",
    address: "au-east.conker.example:5888",
    players: 5,
    maxPlayers: 8,
    ping: 274,
    status: "in-game",
  },
];

const NAMES = [
  "AcornHoarder",
  "Sgt. Nutz",
  "TeddyKruger",
  "BushyTail",
  "StuffingSpiller",
  "PawsOfFury",
  "Bolt99",
  "FurBall",
  "Private Pecan",
  "GrizzlyBizness",
];

const CHAT = [
  "who took the sniper rifle again",
  "Tediz never die, we just respawn",
  "cook it longer next time",
  "gg last round",
];

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const jitter = (ms: number) => Math.max(1, Math.round(ms * (0.85 + Math.random() * 0.3)));

export async function listServers(): Promise<Server[]> {
  await wait(250);
  const local: Server = {
    id: "local",
    name: "This machine",
    region: "Local",
    address: localAddress,
    map: MAP,
    mode: MODE,
    players: 0,
    maxPlayers: 8,
    ping: 1,
    status: "open",
  };
  return [
    local,
    ...MOCK_SERVERS.map((s) => ({ ...s, map: MAP, mode: MODE, ping: jitter(s.ping) })),
  ];
}

export async function getLobby(serverId: string): Promise<Lobby | undefined> {
  const server = (await listServers()).find((s) => s.id === serverId);
  if (!server) return undefined;
  const seed = serverId.length;
  // "This machine" has no lobby service yet: only you (its bots join in the match itself).
  const count = server.id === "local" ? 0 : Math.min(server.players, NAMES.length);
  const players = Array.from({ length: count }, (_, i): LobbyPlayer => ({
    id: `p${i}`,
    name: NAMES[(i + seed) % NAMES.length],
    side: i % 2 ? "tediz" : "squirrels",
    ready: i % 3 !== 2,
    ping: server.ping + ((i * 13) % 40),
    host: i === 0,
  }));
  const chat = players
    .slice(0, CHAT.length)
    .map((p, i): ChatMessage => ({ id: `m${i}`, from: p.name, side: p.side, text: CHAT[i] }));
  return { server, players, chat };
}
