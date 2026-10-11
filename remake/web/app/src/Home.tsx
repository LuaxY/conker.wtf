import { Link } from "@tanstack/react-router";
import {
  Bot,
  Crosshair,
  Eye,
  Footprints,
  Gamepad2,
  Globe,
  Keyboard,
  type LucideIcon,
  Swords,
} from "lucide-react";
import { motion } from "motion/react";
import { BRICKS, Emblem, Keys, SIDES, primary } from "./ui";

interface Control {
  action: string;
  note: string;
  keys: string[];
  pad: string[];
}

// web/README.md's controls table, in players' words.
const CONTROLS: { group: string; icon: LucideIcon; rows: Control[] }[] = [
  {
    group: "Move",
    icon: Footprints,
    rows: [
      {
        action: "Move",
        note: "Relative to the camera.",
        keys: ["W", "A", "S", "D", "or", "Arrows"],
        pad: ["Left stick"],
      },
      {
        action: "Walk",
        note: "Hold Alt (Option on a Mac) while moving, or tilt the stick halfway.",
        keys: ["Alt", "+", "move"],
        pad: ["Left stick", "halfway"],
      },
      {
        action: "Jump",
        note: "With a gun drawn, it reloads instead.",
        keys: ["Space"],
        pad: ["A"],
      },
      {
        action: "Strafe",
        note: "Sidestep left or right, standing or on the run.",
        keys: ["Q", "or", "E"],
        pad: ["Right stick ← →"],
      },
    ],
  },
  {
    group: "Combat",
    icon: Crosshair,
    rows: [
      {
        action: "Draw / holster",
        note: "Walk over a weapon to pick it up, then draw it.",
        keys: ["B"],
        pad: ["X"],
      },
      {
        action: "Fire",
        note: "Unarmed, it crouches. A melee swing locks onto the nearest player.",
        keys: ["Left click", "or", "F", "or", "Shift"],
        pad: ["RT", "or", "LT", "or", "LB"],
      },
      {
        action: "Aim / scope",
        note: "Hold to zoom in, or to look through a sniper scope.",
        keys: ["Right click", "or", "R"],
        pad: ["RB"],
      },
      {
        action: "Grenade",
        note: "Hold to cook it. Not too long: it goes off in your hand.",
        keys: ["G"],
        pad: ["Right stick ↓"],
      },
    ],
  },
  {
    group: "Camera",
    icon: Eye,
    rows: [
      {
        action: "Follow camera",
        note: "Stays behind you on its own. No key turns it, as in the original.",
        keys: [],
        pad: [],
      },
      {
        action: "Aim with the mouse",
        note: "While aiming, the mouse turns your aim or the scope's look.",
        keys: ["Right click", "+", "mouse"],
        pad: ["RB", "+", "Left stick"],
      },
      {
        action: "Free camera",
        note: "Toggles the debug fly-through camera.",
        keys: ["C"],
        pad: [],
      },
      {
        action: "Mute",
        note: "Toggles the sound. In the browser it starts with your first key press or click.",
        keys: ["M"],
        pad: [],
      },
    ],
  },
];

const FEATURES = [
  {
    icon: Swords,
    title: "Pick a side",
    text: "Squirrels or Tediz, one map, every weapon on the floor.",
  },
  { icon: Bot, title: "Bots included", text: "The original's AI fills the empty slots." },
  { icon: Globe, title: "No install", text: "Rust and WebAssembly, right in the browser." },
];

export function Home() {
  return (
    <motion.main initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <section className="relative overflow-hidden border-b-4 border-mud">
        <div
          aria-hidden
          className="pixelated absolute inset-0 opacity-30"
          style={{ backgroundImage: `url(${BRICKS})`, backgroundSize: "160px" }}
        />
        <div
          aria-hidden
          className="absolute inset-0 bg-linear-to-b from-bunker/50 via-bunker/80 to-bunker"
        />
        <div className="relative mx-auto grid max-w-6xl items-center gap-12 px-4 py-14 md:grid-cols-[1.25fr_1fr] md:py-24">
          <div>
            <p className="font-stencil text-sm tracking-widest text-acorn uppercase">
              Multiplayer Deathmatch, remade
            </p>
            <h1 className="cartoon mt-3 font-display text-6xl leading-[0.95] sm:text-7xl lg:text-8xl">
              <span className="text-squirrels">Squirrels</span>{" "}
              <span className="text-4xl text-khaki sm:text-5xl">vs</span>{" "}
              <span className="text-tediz">Tediz</span>
            </h1>
            <p className="mt-6 max-w-xl text-lg text-khaki/90">
              The N64's foulest-mouthed Deathmatch, rebuilt in Rust and played in your browser. Grab
              a gun off the floor, cook a grenade, and settle the war one furry frag at a time. An
              authoritative server keeps everyone honest.
            </p>
            <div className="mt-8 flex flex-wrap items-center gap-x-6 gap-y-4">
              <Link to="/servers" className={`${primary} px-8 py-4 text-xl`}>
                <Crosshair aria-hidden className="size-6" />
                Join game
              </Link>
              <p className="text-sm text-dust">Keyboard and mouse, or a gamepad.</p>
            </div>
            <p className="mt-6 rounded-lg border-2 border-acorn/50 bg-acorn/10 p-3 text-sm md:hidden">
              Playing needs a keyboard or a gamepad. You can browse the servers here, then join from
              a computer.
            </p>
          </div>

          <div className="flex items-start justify-center gap-3 sm:gap-6">
            {(["squirrels", "tediz"] as const).map((side, i) => (
              <motion.figure
                key={side}
                initial={{ opacity: 0, y: 30, rotate: 0 }}
                animate={{ opacity: 1, y: 0, rotate: i ? 5 : -5 }}
                transition={{ type: "spring", delay: 0.15 + i * 0.15 }}
                className="grid justify-items-center gap-3"
              >
                <Emblem
                  side={side}
                  className={`size-32 rounded-2xl border-4 shadow-[8px_8px_0_#000] sm:size-44 ${SIDES[side].border}`}
                />
                <figcaption className="text-center">
                  <span className={`cartoon block font-display text-3xl ${SIDES[side].text}`}>
                    {SIDES[side].name}
                  </span>
                  <span className="font-stencil text-xs tracking-widest text-dust uppercase">
                    {SIDES[side].tagline}
                  </span>
                </figcaption>
              </motion.figure>
            ))}
          </div>
        </div>
      </section>

      <section
        aria-label="Features"
        className="mx-auto grid max-w-6xl gap-4 px-4 py-10 sm:grid-cols-3"
      >
        {FEATURES.map(({ icon: Icon, title, text }) => (
          <div key={title} className="flex gap-3 rounded-xl border-2 border-mud bg-trench p-4">
            <Icon aria-hidden className="mt-0.5 size-6 shrink-0 text-acorn" />
            <div>
              <h2 className="font-stencil uppercase">{title}</h2>
              <p className="text-sm text-dust">{text}</p>
            </div>
          </div>
        ))}
      </section>

      <section aria-labelledby="controls" className="mx-auto max-w-6xl px-4 pt-6 pb-16">
        <h2 id="controls" className="cartoon font-display text-5xl">
          Controls
        </h2>
        <p className="mt-2 max-w-2xl text-dust">
          Keyboard and mouse, or any standard gamepad (Xbox layout shown). Click into the game once
          so it gets the keys; a pad works while the game's window has focus.
        </p>
        <div className="mt-8 grid gap-6 lg:grid-cols-3">
          {CONTROLS.map(({ group, icon: Icon, rows }) => (
            <section
              key={group}
              aria-labelledby={`controls-${group}`}
              className="rounded-2xl border-2 border-mud bg-trench p-5"
            >
              <h3
                id={`controls-${group}`}
                className="flex items-center gap-2 font-display text-3xl text-acorn"
              >
                <Icon aria-hidden className="size-6" />
                {group}
              </h3>
              <ul className="mt-2">
                {rows.map((row) => (
                  <li key={row.action} className="border-t border-mud py-3 first:border-0">
                    <p className="font-stencil uppercase">{row.action}</p>
                    <p className="text-sm text-dust">{row.note}</p>
                    {row.keys.length + row.pad.length > 0 && (
                      <div className="mt-2 grid gap-1.5">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <Keyboard aria-hidden className="mr-1 size-4 text-dust" />
                          <span className="sr-only">Keyboard and mouse:</span>
                          <Keys keys={row.keys} />
                        </div>
                        <div className="flex flex-wrap items-center gap-1.5">
                          <Gamepad2 aria-hidden className="mr-1 size-4 text-dust" />
                          <span className="sr-only">Gamepad:</span>
                          <Keys keys={row.pad} pad />
                        </div>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </section>
    </motion.main>
  );
}
