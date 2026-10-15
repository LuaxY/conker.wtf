import { Camera, Panda, Skull, Squirrel } from "lucide-react";
import { useRouter } from "@tanstack/react-router";
import { type CSSProperties, type ReactNode, useEffect, useState } from "react";
import type { Side, Status } from "./data";

export const SIDES = {
  squirrels: {
    name: "Squirrels",
    tagline: "The good guys. Allegedly.",
    text: "text-squirrels",
    deep: "bg-squirrels-deep",
  },
  tediz: {
    name: "Tediz",
    tagline: "Evil teddies. Still cuddly.",
    text: "text-tediz",
    deep: "bg-tediz-deep",
  },
} as const;

// Images of the asset pack the page is served with (assets/, next to it: the operator's own export,
// never in this tree). Every one has a fallback for a site without the pack. ../../DESIGN.md lists them.
const TEXTURES = "assets/maps/2d/textures/";
const texture = (name: string) => `${TEXTURES}${name}-ci4rgba16-i.png`;
export const BRICKS = texture("0113-64x64");
/** The Total War briefing's taped battle plan, THEM vs US, in four tiles: top left to bottom right. */
export const PLAN = ["3596", "3597", "3598", "3599"].map((n) => texture(`${n}-64x64`));
export const PLATES = { squirrels: texture("0729-64x64"), tediz: texture("1858-64x64") };
// Each side's banner is two 32×64 textures side by side in the map: its left and right halves.
const BANNERS = { squirrels: ["0730", "0731"], tediz: ["0606", "0607"] };
export const DANGER = `${TEXTURES}1667-32x64-ci8rgba16-i.png`;
const SKULL = "assets/hud/skull.png";

/** One of the pack's images, or `fallback` when the pack is missing. Always decorative. */
export function Sprite({
  src,
  fallback,
  className = "",
  style,
}: {
  src: string;
  fallback: ReactNode;
  className?: string;
  style?: CSSProperties;
}) {
  const [failed, setFailed] = useState(false);
  if (failed) return fallback;
  return (
    <img
      src={src}
      alt=""
      onError={() => setFailed(true)}
      className={`pixelated ${className}`}
      style={style}
    />
  );
}

/** A 16:9 screenshot, `assets/screenshots/<path>.jpg` (`maps/2d`, `modes/total-war`): taken from the
 * game, so it lives in the pack, never in this tree. A gray box until someone drops it in. */
export function Shot({ path }: { path: string }) {
  return (
    <span className="block aspect-video overflow-hidden rounded-xl border-2 border-black bg-[#4a4a4a]">
      <Sprite
        src={`assets/screenshots/${path}.jpg`}
        className="size-full object-cover [image-rendering:auto]"
        fallback={
          <span className="grid size-full place-items-center bg-[repeating-linear-gradient(-45deg,#555_0_10px,#4a4a4a_10px_20px)] text-[#9a9a9a]">
            <Camera aria-hidden className="size-7" />
          </span>
        }
      />
    </span>
  );
}

const ICONS = { squirrels: Squirrel, tediz: Panda };

/** The side's banner from the map, both halves (64×64): the winged "S", or the "T" between lightning bolts. */
export function Banner({ side, className = "" }: { side: Side; className?: string }) {
  const [failed, setFailed] = useState(false);
  const Icon = ICONS[side];
  return (
    <span aria-hidden className={`grid aspect-square shrink-0 ${SIDES[side].text} ${className}`}>
      {failed ? (
        <span
          className={`grid size-full place-items-center rounded-b-xl border-3 border-black ${SIDES[side].deep}`}
        >
          <Icon className="h-auto w-1/2" />
        </span>
      ) : (
        <span className="grid size-full grid-cols-2 drop-shadow-[4px_4px_0_#000]">
          {BANNERS[side].map((n) => (
            <img
              key={n}
              src={`${TEXTURES}${n}-32x64-ci8rgba16-i.png`}
              alt=""
              onError={() => setFailed(true)}
              className="pixelated size-full"
            />
          ))}
        </span>
      )}
    </span>
  );
}

export function SkullIcon({ className = "size-5" }: { className?: string }) {
  return (
    <Sprite
      src={SKULL}
      fallback={<Skull aria-hidden className={className} />}
      className={`[image-rendering:auto] ${className}`}
    />
  );
}

const STATUS: Record<Status, [label: string, style: string]> = {
  starting: ["Warming up", "border-amber text-amber"],
  running: ["Running", "border-lime text-lime shadow-[0_0_10px_rgb(86_255_0/0.4)]"],
  exited: ["Snuffed it", "border-dust/60 text-dust"],
};

export function StatusBadge({ status, code }: { status: Status; code: number | null }) {
  const [label, style] = STATUS[status];
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border-2 bg-black/60 px-2.5 py-0.5 font-stencil text-xs tracking-wider whitespace-nowrap uppercase ${style}`}
    >
      {status === "exited" ? (
        <SkullIcon className="size-4" />
      ) : (
        <span
          aria-hidden
          className={`size-2 rounded-full bg-current ${status === "starting" ? "animate-pulse" : ""}`}
        />
      )}
      {label}
      {status === "exited" && code !== null && ` (${code})`}
    </span>
  );
}

// Face buttons in the Xbox layout's colours.
const PAD_COLORS: Record<string, string> = {
  A: "bg-[#5fd35f] text-ink border-[#2c7a2c] rounded-full min-w-9",
  X: "bg-[#6cb0ff] text-ink border-[#2f6bb8] rounded-full min-w-9",
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
        className={`inline-flex h-9 items-center justify-center rounded-lg border-2 border-b-[5px] px-2 font-display text-sm tracking-wide whitespace-nowrap shadow-[0_2px_0_#000] ${(pad && PAD_COLORS[key]) || "min-w-9 border-[#8d7f5c] bg-bone text-night"}`}
      >
        {key}
      </kbd>
    ),
  );
}

/** Reloads the route's data (the servers' /info: who is playing) every `ms` while shown. */
export function useRefresh(ms = 5000) {
  const router = useRouter();
  useEffect(() => {
    const timer = setInterval(() => void router.invalidate(), ms);
    return () => clearInterval(timer);
  }, [router, ms]);
}
