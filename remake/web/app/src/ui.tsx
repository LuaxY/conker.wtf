import { Panda, Squirrel } from "lucide-react";
import { useState } from "react";
import type { Side, Status } from "./data";

export const SIDES = {
  squirrels: {
    name: "Squirrels",
    tagline: "The good guys",
    text: "text-squirrels",
    border: "border-squirrels",
  },
  tediz: { name: "Tediz", tagline: "The bad guys", text: "text-tediz", border: "border-tediz" },
} as const;

// Textures of the asset pack the page is served with (assets/, next to it: the operator's own
// export, never in this tree). Missing pack: the icons and the plain background show instead.
const TEXTURES = "assets/maps/2d/textures/";
export const BRICKS = `${TEXTURES}0113-64x64-ci4rgba16-i.png`;
const EMBLEMS = {
  squirrels: `${TEXTURES}0172-64x64-ci4rgba16-i.png`,
  tediz: `${TEXTURES}0605-64x64-ci4rgba16-i.png`,
};

export const button =
  "inline-flex items-center justify-center gap-2 rounded-xl font-stencil uppercase tracking-wide transition active:translate-y-0.5 disabled:cursor-not-allowed disabled:opacity-50";
export const primary = `${button} bg-acorn text-bunker shadow-[0_5px_0_#8a5a0c] hover:brightness-110 active:shadow-[0_2px_0_#8a5a0c]`;
export const field =
  "h-11 w-full rounded-lg border-2 border-mud bg-bunker px-3 text-khaki placeholder:text-dust/70 focus-visible:border-acorn";

/** The side's banner from the map's own textures, over an icon in case the pack is missing. */
export function Emblem({ side, className = "" }: { side: Side; className?: string }) {
  const [failed, setFailed] = useState(false);
  const Icon = side === "squirrels" ? Squirrel : Panda;
  return (
    <span
      aria-hidden
      className={`relative grid shrink-0 place-items-center overflow-hidden bg-mud ${SIDES[side].text} ${className}`}
    >
      <Icon className="size-3/5" />
      {!failed && (
        <img
          src={EMBLEMS[side]}
          alt=""
          onError={() => setFailed(true)}
          className="pixelated absolute inset-0 size-full object-cover"
        />
      )}
    </span>
  );
}

export function Ping({ ms }: { ms: number }) {
  const color = ms < 60 ? "bg-go" : ms < 150 ? "bg-acorn" : "bg-tediz";
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap tabular-nums">
      <span aria-hidden className={`size-2 rounded-full ${color}`} />
      {ms} ms
    </span>
  );
}

const STATUS: Record<Status, [label: string, style: string]> = {
  open: ["Open", "bg-go/15 text-go"],
  full: ["Full", "bg-dust/15 text-dust"],
  "in-game": ["In game", "bg-acorn/15 text-acorn"],
};

export function StatusBadge({ status }: { status: Status }) {
  const [label, style] = STATUS[status];
  return (
    <span
      className={`rounded-md px-2 py-0.5 font-stencil text-xs whitespace-nowrap uppercase ${style}`}
    >
      {label}
    </span>
  );
}

// Face buttons in the Xbox layout's colours.
const PAD_COLORS: Record<string, string> = {
  A: "bg-go text-bunker border-[#3b8f3b] rounded-full min-w-8",
  X: "bg-[#5ca4ff] text-bunker border-[#2f6bb8] rounded-full min-w-8",
};

/** Key caps; lowercase words ("or", "+", "mouse") are drawn as text between them. */
export function Keys({ keys, pad = false }: { keys: string[]; pad?: boolean }) {
  if (keys.length === 0) return <span className="text-sm text-dust">none</span>;
  return keys.map((key, i) =>
    /^[a-z+]+$/.test(key) ? (
      <span key={i} className="text-xs text-dust">
        {key}
      </span>
    ) : (
      <kbd
        key={i}
        className={`inline-flex h-8 items-center justify-center rounded-md border-b-4 px-2 font-stencil text-sm whitespace-nowrap ${(pad && PAD_COLORS[key]) || "border-[#9d9172] bg-khaki text-bunker min-w-8"}`}
      >
        {key}
      </kbd>
    ),
  );
}
