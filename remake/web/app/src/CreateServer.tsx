import { Link, useRouter } from "@tanstack/react-router";
import { Play } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { type CSSProperties, type FormEvent, type ReactNode, useState } from "react";
import {
  allowed,
  CHEATS,
  type Cheat,
  createServer,
  fit,
  type GameServer,
  MAPS,
  type MapId,
  MODES,
  type Mode,
  type NewServer,
  ROUTES,
  type Route,
  type Settings,
  SKILLS,
  type Skill,
} from "./data";
import { Shot } from "./ui";

const LABEL = "font-stencil text-sm tracking-wider text-dust uppercase";
/** What appears when the map or mode changes slides in: quick, soft landing, no exit. */
const REVEAL = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.2, ease: [0.23, 1, 0.32, 1] },
} as const;

// The Deathmatch arenas first, then the maps built round their own mode.
const MAP_ORDER: MapId[] = ["2d", "30", "34", "3f", "1a", "2b", "24", "33"];

const DEFAULT: NewServer = {
  name: "Bad Fur Day",
  map: "2d",
  mode: "deathmatch",
  settings: {
    bots: 3,
    skill: "normal",
    time: 10,
    lives: 3,
    kills: 0,
    score: 20,
    radar: true,
    laps: 5,
    route: 11,
    cheats: [],
  },
};

const BRAINS: Record<Skill, string> = {
  inbred: "They shoot walls.",
  crap: "They shoot. Mostly the floor.",
  normal: "They shoot back.",
  bastard: "They shoot you in the back.",
  einstein: "They shoot you before you see them.",
};

const range = (lo: number, hi: number) => Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
const numbers = (lo: number, hi: number) =>
  range(lo, hi).map((n): [number, string] => [n, String(n)]);

/** The game server's settings (conker-server's flags), only those the map and mode use, and Run. */
export function CreateServer() {
  const router = useRouter();
  const [form, setForm] = useState(DEFAULT);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [started, setStarted] = useState<GameServer | null>(null);
  const s = form.settings;
  const mode = MODES[form.mode];
  const untimed = mode.laps || s.time === "unlimited";
  const setting = (change: Partial<Settings>) =>
    setForm({ ...form, settings: { ...s, ...change } });
  const pick = (map: MapId, next: Mode) =>
    setForm({ ...form, map, mode: next, settings: fit(s, next) });

  const run = async (e: FormEvent) => {
    e.preventDefault();
    setPending(true);
    setError("");
    setStarted(null);
    try {
      setStarted(await createServer(form));
      await router.invalidate({ sync: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPending(false);
    }
  };
  const [why, ...log] = error.split("\n");

  return (
    <section aria-labelledby="create" className="board mt-12 p-5 sm:p-8">
      <p className="font-stencil text-sm tracking-widest text-lime uppercase">Set-up</p>
      <h2 id="create" className="cartoon font-display text-4xl text-amber sm:text-5xl">
        Create server
      </h2>
      <p className="mt-1 text-dust">
        Pick a map, pick a mode, set the house rules. Bots fill the empty seats.
      </p>

      <form onSubmit={run} className="mt-8 grid gap-10">
        <Step n={1} title="Pick a map">
          <div className="grid grid-cols-2 gap-3 sm:gap-4 lg:grid-cols-4">
            {MAP_ORDER.map((id) => (
              <MapCard
                key={id}
                id={id}
                checked={form.map === id}
                onChange={() =>
                  pick(id, MAPS[id].modes.includes(form.mode) ? form.mode : MAPS[id].modes[0])
                }
              />
            ))}
          </div>
        </Step>

        <Step n={2} title="Pick a mode">
          <div className="grid gap-4 md:grid-cols-2">
            <AnimatePresence initial={false}>
              {MAPS[form.map].modes.map((id) => (
                <ModeCard
                  key={id}
                  id={id}
                  checked={form.mode === id}
                  onChange={() => pick(form.map, id)}
                />
              ))}
            </AnimatePresence>
          </div>
          {MAPS[form.map].modes.length === 1 && (
            <p className="font-hand text-dust">The only game in town on this map.</p>
          )}
        </Step>

        <Step n={3} title="House rules">
          <div className="grid gap-4 md:grid-cols-2">
            <AnimatePresence initial={false}>
              <Tile
                key="bots"
                label="Bots"
                readout={s.bots || "None"}
                hint="They step aside as your mates join."
              >
                <ChocBar
                  value={s.bots}
                  max={mode.maxBots[0]}
                  onChange={(bots) => setting({ bots })}
                />
              </Tile>
              <Tile
                key="skill"
                label="Bot brains"
                hint={s.bots ? BRAINS[s.skill] : "No bots, no brains."}
                disabled={!s.bots}
              >
                <Chips
                  name="skill"
                  value={s.skill}
                  options={SKILLS.map((k) => [k, k])}
                  onChange={(skill) => setting({ skill })}
                />
              </Tile>
              {mode.laps ? (
                <Tile key="laps" label="Laps" readout={s.laps}>
                  <Slider
                    label="Laps"
                    value={s.laps}
                    min={1}
                    max={20}
                    onChange={(laps) => setting({ laps })}
                  />
                </Tile>
              ) : (
                <Tile
                  key="time"
                  label="Time limit"
                  readout={s.time === "unlimited" ? "None" : `${s.time} min`}
                  hint={
                    s.time === "unlimited"
                      ? "Unlimited. Pace yourself."
                      : mode.lives
                        ? "Lives are endless while the clock runs."
                        : undefined
                  }
                >
                  <Chips
                    name="time"
                    value={s.time}
                    options={[...numbers(3, 10), ["unlimited", "No limit"]]}
                    onChange={(time) => setting({ time })}
                  />
                </Tile>
              )}
              {mode.laps && (
                <Tile key="route" label="Route">
                  <Chips
                    name="route"
                    value={s.route}
                    options={([11, 12] as const).map((r): [Route, string] => [r, ROUTES[r]])}
                    onChange={(route) => setting({ route })}
                  />
                </Tile>
              )}
              {untimed && mode.lives && (
                <Tile key="lives" label="Lives" hint="As many as you think you can get away with.">
                  <Chips
                    name="lives"
                    value={s.lives}
                    options={numbers(1, 5)}
                    onChange={(lives) => setting({ lives })}
                  />
                </Tile>
              )}
              {mode.kills && (
                <Tile
                  key="kills"
                  label="Kill limit"
                  hint={s.kills ? `First to ${s.kills} wins.` : "No limit. Keep shooting."}
                >
                  <Chips
                    name="kills"
                    value={s.kills}
                    options={[[0, "None"], ...numbers(1, 10)]}
                    onChange={(kills) => setting({ kills })}
                  />
                </Tile>
              )}
              {mode.score && (
                <Tile
                  key={`score-${form.mode}`}
                  label={form.mode === "heist" ? "Bags to win" : "Score to win"}
                  readout={s.score === "unlimited" ? "None" : s.score}
                  hint={
                    s.score === "unlimited"
                      ? "No limit. Pace yourself."
                      : form.mode === "heist"
                        ? `First gang to bank ${s.score} wins.`
                        : `First team to ${s.score} wins.`
                  }
                >
                  {mode.maxScore && mode.maxScore <= 5 ? (
                    <Chips
                      name="score"
                      value={s.score}
                      options={numbers(1, mode.maxScore)}
                      onChange={(score) => setting({ score })}
                    />
                  ) : (
                    // Past the top end: no limit, where the mode has one.
                    <Slider
                      label="Score to win"
                      value={s.score === "unlimited" ? 41 : s.score}
                      min={1}
                      max={mode.maxScore ?? 41}
                      text={s.score === "unlimited" ? "No limit" : undefined}
                      onChange={(n) => setting({ score: n > 40 ? "unlimited" : n })}
                    />
                  )}
                </Tile>
              )}
              <Tile
                key="radar"
                label="Radar"
                hint={s.radar ? "Everyone's a blip on the HUD." : "No radar. Use your ears."}
              >
                <Chips
                  name="radar"
                  value={s.radar}
                  options={[
                    [true, "On"],
                    [false, "Off"],
                  ]}
                  onChange={(radar) => setting({ radar })}
                />
              </Tile>
              {allowed(form.map, form.mode, []) !== 0 && (
                <motion.details
                  key="cheats"
                  {...REVEAL}
                  className="rounded-2xl border-2 border-dashed border-oak p-4 md:col-span-2"
                >
                  <summary className="cursor-pointer font-hand text-lg text-dust marker:text-lime">
                    Psst… cheat codes{s.cheats.length > 0 && ` (${s.cheats.length} on)`}
                  </summary>
                  <fieldset className="mt-3 min-w-0">
                    <legend className="sr-only">Cheats</legend>
                    <div className="flex flex-wrap gap-2">
                      {(Object.keys(CHEATS) as Cheat[]).map((code) => (
                        <label key={code} className="chip">
                          <input
                            type="checkbox"
                            name="cheats"
                            value={code}
                            checked={s.cheats.includes(code)}
                            onChange={(e) =>
                              setting({
                                cheats: e.target.checked
                                  ? [...s.cheats, code]
                                  : s.cheats.filter((c) => c !== code),
                              })
                            }
                            className="sr-only"
                          />
                          {code}
                        </label>
                      ))}
                    </div>
                    <p className="mt-3 text-sm text-dust">
                      Each one adds characters to pick in the lobby.
                    </p>
                  </fieldset>
                </motion.details>
              )}
            </AnimatePresence>
          </div>
        </Step>

        <Step n={4} title="Name it, run it">
          <div className="flex flex-wrap items-end gap-4">
            <label className="grid min-w-0 flex-1 basis-64 gap-1 sm:max-w-md">
              <span className={LABEL}>Server name</span>
              <input
                name="name"
                required
                maxLength={32}
                pattern="[\p{L}\p{N} _.,'!?\-]+"
                title="Letters, numbers, spaces and _.,'!?-"
                autoComplete="off"
                spellCheck={false}
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                className="field"
              />
            </label>
            <button type="submit" disabled={pending} className="cta">
              <Play aria-hidden className="size-5" />
              {pending ? "Starting…" : "Run it"}
            </button>
          </div>
          <output className="block">
            {started && (
              <span className="bubble mt-4 block max-w-xl">
                {started.name}'s up. Go on then:{" "}
                <Link
                  to="/lobby/$serverId"
                  params={{ serverId: started.id }}
                  className="font-bold text-tediz-deep underline"
                >
                  get stuck in
                </Link>
                .
              </span>
            )}
          </output>
          {error && (
            <div role="alert" className="mt-4 grid max-w-3xl gap-3">
              <p className="bubble max-w-xl">That didn't work: {why}</p>
              {log.length > 0 && (
                <details>
                  <summary className="cursor-pointer text-sm text-dust">The server's log</summary>
                  <pre className="mt-2 overflow-x-auto rounded-xl border-3 border-tediz bg-black/70 p-4 text-sm whitespace-pre-wrap text-tediz">
                    {log.join("\n")}
                  </pre>
                </details>
              )}
            </div>
          )}
        </Step>
      </form>
    </section>
  );
}

/** A numbered step of the form: a real sequence, map → mode → rules → run. */
function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <fieldset className="grid min-w-0 gap-4">
      <legend className="mb-4 flex items-center gap-3">
        <span
          aria-hidden
          className="grid size-11 shrink-0 -rotate-6 place-items-center rounded-full border-3 border-black bg-amber font-display text-2xl text-night shadow-[3px_3px_0_#000]"
        >
          {n}
        </span>
        <span className="font-display text-2xl tracking-wide text-bone sm:text-3xl">{title}</span>
      </legend>
      {children}
    </fieldset>
  );
}

/** The lime "Picked!" tag on the chosen card. */
function Picked() {
  return (
    <span
      aria-hidden
      className="absolute -top-2 -right-2 scale-90 rotate-6 rounded-full border-2 border-black bg-lime px-2.5 py-0.5 font-display text-sm text-moss opacity-0 shadow-[2px_2px_0_#000] transition-[opacity,scale] duration-150 ease-snap group-has-checked:scale-100 group-has-checked:opacity-100"
    >
      Picked!
    </span>
  );
}

// A card is a radio's label: the whole card is the hit target, the radio stays native.
const CARD =
  "group relative cursor-pointer rounded-2xl border-3 border-black bg-black/40 shadow-[4px_4px_0_#000] transition-[translate,rotate,scale,border-color,box-shadow] duration-150 ease-snap select-none hover:-translate-y-0.5 active:scale-[0.98] has-checked:-translate-y-1 has-checked:-rotate-1 has-checked:border-amber has-checked:shadow-[6px_6px_0_#000,0_0_18px_rgb(248_168_32/0.35)] has-focus-visible:outline-3 has-focus-visible:outline-offset-2 has-focus-visible:outline-lime";

function MapCard({ id, checked, onChange }: { id: MapId; checked: boolean; onChange: () => void }) {
  const { name, note, modes } = MAPS[id];
  return (
    <label className={`${CARD} grid content-start gap-2.5 p-2 pb-3`}>
      <input
        type="radio"
        name="map"
        value={id}
        checked={checked}
        onChange={onChange}
        className="sr-only"
      />
      <span className="relative block">
        <Shot path={`maps/${id}`} />
        <span className="cartoon absolute bottom-1 left-2.5 font-display text-2xl text-amber sm:text-3xl">
          {name}
        </span>
        <Picked />
      </span>
      <span className="px-1 text-xs text-pretty text-bone/90 sm:text-sm">{note}</span>
      <span className="flex flex-wrap gap-1 px-1">
        {modes.map((m) => (
          <span
            key={m}
            className="rounded-full border-2 border-black bg-oak px-2 py-0.5 text-[0.7rem] font-bold text-bone"
          >
            {MODES[m].name}
          </span>
        ))}
      </span>
    </label>
  );
}

function ModeCard({ id, checked, onChange }: { id: Mode; checked: boolean; onChange: () => void }) {
  return (
    <motion.label
      {...REVEAL}
      className={`${CARD} grid grid-cols-[8rem_1fr] items-center gap-4 p-2 pr-4 sm:grid-cols-[11rem_1fr]`}
    >
      <input
        type="radio"
        name="mode"
        value={id}
        checked={checked}
        onChange={onChange}
        className="sr-only"
      />
      <span className="relative block">
        <Shot path={`modes/${id}`} />
        <Picked />
      </span>
      <span className="grid gap-1">
        <span className="font-display text-xl tracking-wide text-amber sm:text-2xl">
          {MODES[id].name}
        </span>
        <span className="text-sm text-pretty text-bone/90">{MODES[id].note}</span>
      </span>
    </motion.label>
  );
}

/** One house rule: its label (and the value, where the control doesn't spell it out), and a cheeky hint. */
function Tile({
  label,
  readout,
  hint,
  disabled = false,
  children,
}: {
  label: string;
  readout?: ReactNode;
  hint?: string;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <motion.div {...REVEAL} className="rounded-2xl border-2 border-black bg-black/35 p-4">
      <fieldset disabled={disabled} className="min-w-0 transition-opacity disabled:opacity-50">
        <legend className="mb-3 flex w-full items-baseline justify-between gap-3">
          <span className={LABEL}>{label}</span>
          {readout !== undefined && (
            <span className="font-display text-2xl leading-none text-amber tabular-nums">
              {readout}
            </span>
          )}
        </legend>
        {children}
        {hint && <p className="mt-3 font-hand text-sm text-dust">{hint}</p>}
      </fieldset>
    </motion.div>
  );
}

/** A row of lit pills, one radio each. */
function Chips<T extends string | number | boolean>({
  name,
  value,
  options,
  onChange,
}: {
  name: string;
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {options.map(([v, text]) => (
        <label key={String(v)} className="chip">
          <input
            type="radio"
            name={name}
            value={String(v)}
            checked={v === value}
            onChange={() => onChange(v)}
            className="sr-only"
          />
          {text}
        </label>
      ))}
    </div>
  );
}

/** The bots as the HUD's chocolate bar: rows of 6 squares, lit up to the count. */
function ChocBar({
  value,
  max,
  onChange,
}: {
  value: number;
  max: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <label className="chip">
        <input
          type="radio"
          name="bots"
          value={0}
          checked={value === 0}
          onChange={() => onChange(0)}
          className="sr-only"
        />
        None
      </label>
      <div className="grid grid-cols-6 gap-1 rounded-lg border-3 border-black bg-[#241005] p-1 shadow-[3px_3px_0_#000]">
        {range(1, max).map((k) => (
          <label key={k} className={`choc ${k <= value ? "choc-on" : ""}`}>
            <input
              type="radio"
              name="bots"
              value={k}
              checked={value === k}
              onChange={() => onChange(k)}
              className="sr-only"
            />
            <span className="sr-only">{k}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

/** A native range, filled amber up to the thumb. */
function Slider({
  label,
  value,
  min,
  max,
  text,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  text?: string;
  onChange: (value: number) => void;
}) {
  const fill = { "--fill": `${((value - min) / (max - min)) * 100}%` } as CSSProperties;
  return (
    <input
      type="range"
      aria-label={label}
      aria-valuetext={text}
      min={min}
      max={max}
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      className="slider"
      style={fill}
    />
  );
}
