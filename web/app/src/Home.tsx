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
import { useState } from "react";
import { Banner, BRICKS, Keys, PLAN } from "./ui";

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
    text: "Squirrels or Tediz. Every weapon's on the floor.",
    color: "text-amber",
  },
  {
    icon: Bot,
    title: "Bots included",
    text: "The original's AI fills the empty slots.",
    color: "text-squirrels",
  },
  {
    icon: Globe,
    title: "No install",
    text: "Rust and WebAssembly, right in the browser.",
    color: "text-[#ff9a9a]",
  },
];

/** The Total War briefing's battle plan, taped up between the two sides' banners. */
function BattlePlan() {
  const [failed, setFailed] = useState(false);
  return (
    <figure className="relative mx-auto w-fit p-8 sm:p-12">
      {failed ? (
        <div className="grid size-64 -rotate-2 place-items-center border-3 border-black bg-bone font-hand text-4xl text-ink shadow-[8px_8px_0_#000] sm:size-96 sm:text-5xl">
          <p className="text-center">
            Them
            <br />
            <span className="text-2xl">vs</span>
            <br />
            Us
          </p>
        </div>
      ) : (
        <div className="grid size-64 -rotate-2 grid-cols-2 shadow-[8px_8px_0_#000] sm:size-96">
          {PLAN.map((src) => (
            <img
              key={src}
              src={src}
              alt=""
              onError={() => setFailed(true)}
              className="pixelated size-full"
            />
          ))}
        </div>
      )}
      <Banner side="tediz" className="absolute top-0 right-0 size-16 rotate-6 sm:size-32" />
      <Banner side="squirrels" className="absolute bottom-0 left-0 size-16 -rotate-6 sm:size-32" />
      <figcaption className="sr-only">
        The Squirrels' battle plan against the Tediz, from the game's Total War briefing.
      </figcaption>
    </figure>
  );
}

export function Home() {
  return (
    <motion.main initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <section className="relative overflow-hidden border-b-3 border-black">
        <div
          aria-hidden
          className="pixelated absolute inset-0 opacity-40"
          style={{ backgroundImage: `url(${BRICKS})`, backgroundSize: "192px" }}
        />
        <div
          aria-hidden
          className="absolute inset-0 bg-[radial-gradient(ellipse_at_70%_40%,rgb(248_168_32/0.18),transparent_60%),linear-gradient(to_bottom,rgb(18_12_8/0.6),rgb(18_12_8/0.85)_70%,var(--color-night))]"
        />
        <div className="relative mx-auto grid max-w-6xl items-center gap-10 px-4 py-12 md:grid-cols-[1.15fr_1fr] md:py-20">
          <div>
            <p className="font-stencil text-sm tracking-widest text-amber uppercase">
              Multiplayer Deathmatch, remade
            </p>
            <h1 className="cartoon mt-3 font-display text-6xl leading-[0.95] sm:text-7xl lg:text-8xl">
              <span className="bob inline-block text-squirrels">Squirrels</span>{" "}
              <span className="inline-block -rotate-6 font-hand text-4xl text-bone normal-case [-webkit-text-stroke:0] [text-shadow:3px_3px_0_#000] sm:text-5xl">
                vs
              </span>{" "}
              <span className="bob inline-block text-tediz [animation-delay:-1.2s]">Tediz</span>
            </h1>
            <p className="mt-6 max-w-xl text-lg text-bone/90">
              The N64's foulest-mouthed Deathmatch, rebuilt in Rust and played in your browser. Grab
              a gun off the floor, cook a grenade, and settle the war one furry frag at a time. An
              authoritative server keeps everyone honest. Mostly.
            </p>
            <div className="mt-8 flex flex-wrap items-center gap-x-6 gap-y-4">
              <Link to="/servers" className="cta">
                <Crosshair aria-hidden className="size-6" />
                Join game
              </Link>
              <p className="text-sm text-dust">Keyboard and mouse, or a gamepad.</p>
            </div>
            <p className="bubble mt-8 text-sm md:hidden">
              Playing needs a keyboard or a gamepad. Have a look round the servers here, then join
              from a computer.
            </p>
          </div>
          <BattlePlan />
        </div>
      </section>

      <section aria-labelledby="tonight" className="mx-auto max-w-6xl px-4 py-12">
        <div className="chalkboard">
          <h2 id="tonight" className="chalk font-hand text-3xl text-bone">
            On tonight
          </h2>
          <ul className="mt-4 grid gap-6 md:grid-cols-3">
            {FEATURES.map(({ icon: Icon, title, text, color }) => (
              <li
                key={title}
                className="border-dashed border-white/25 not-first:border-t-2 not-first:pt-6 md:not-first:border-t-0 md:not-first:border-l-2 md:not-first:pt-0 md:not-first:pl-6"
              >
                <h3 className={`chalk flex items-center gap-2 font-display text-2xl ${color}`}>
                  <Icon aria-hidden className="size-6" />
                  {title}
                </h3>
                <p className="chalk mt-1 font-hand text-bone/90">{text}</p>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section aria-labelledby="controls" className="mx-auto max-w-6xl px-4 pt-6 pb-16">
        <div className="hazard -mx-4 mb-10 sm:mx-0" />
        <h2 id="controls" className="chrome font-display text-5xl">
          Controls
        </h2>
        <p className="mt-2 max-w-2xl text-dust">
          Keyboard and mouse, or any standard gamepad (Xbox layout shown). Click into the game once
          so it gets the keys; a pad works while the game's window has focus.
        </p>
        <div className="mt-8 grid gap-8 lg:grid-cols-3">
          {CONTROLS.map(({ group, icon: Icon, rows }) => (
            <section key={group} aria-labelledby={`controls-${group}`} className="board p-6">
              <h3
                id={`controls-${group}`}
                className="cartoon flex items-center gap-2 font-display text-3xl text-amber"
              >
                <Icon aria-hidden className="size-6" />
                {group}
              </h3>
              <ul className="mt-2">
                {rows.map((row) => (
                  <li
                    key={row.action}
                    className="border-t-2 border-dashed border-oak py-3 first:border-0"
                  >
                    <p className="font-display tracking-wide text-bone uppercase">{row.action}</p>
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
