# Design brief: a site that feels like Conker's Bad Fur Day

Research 2026-10-12. "exact" = read literally from the pack's PNG/JSON; "approx" = sampled from a JPEG or a
quantised colour count. Paths in this brief are relative to `remake/web/`. `assets/` is the user's own pack: it is
loaded at run time and is never committed.

## 1. Direction

Each screen is a place in the game. **Home is the pub**: the Cock and Plucker at night, all dark wood, amber lamp light
and rain. The game uses the pub as its menu hub. **Servers is the war room**: khaki stencil walls, riveted plates,
hazard stripes. **Lobby is the briefing**: the sergeant's taped "THEM vs US" battle plan, and you pick a side. Borrow
the menu's glossy wordmarks and lime-lit pill buttons, the box art's amber bevelled letters, speech bubbles and the
red grawlix (details in §3-§4). Avoid flat generic dark UI, pure `#000` pages,
uniform geometric sans type, glassmorphism, and copying or tracing the real logo (do a homage, not a copy).

## 2. Palette

| Token | Hex | Role | Source |
|---|---|---|---|
| `night` | `#120c08` | page background | NA box art's black field, warmed (chosen) |
| `cellar` | `#2a1606` | panel background | pub door and sign wood, approx (`#1a0e04` to `#491b05`) |
| `oak` | `#4a2508` | raised wood, panel inner border | pub sign average `#491b05`, approx |
| `plaster` | `#7a5410` | decoration only (2.9:1 on night) | pub walls `#705000` to `#805000`, approx |
| `bone` | `#f0e8d0` | body text | `stat_deaths.png` highlights, quantised |
| `dust` | `#b9ad8d` | secondary text | current token, kept |
| `amber` | `#f8a820` | primary accent, CTA fill, logo-style face | box logo highlight, approx |
| `fur` | `#e07000` | Conker orange, face shade | box art fur and logo, approx |
| `script` | `#e05010` | marker-script accents | the box's "Conker's", approx |
| `slate` / `lilac` | `#3a3f68` / `#8880b0` | title outline and extrusion / bevel light | box logo bevel, approx |
| `choc-1..4` | `#8c3908 #a54a08 #bd6329 #ce734a` | chocolate bars: shadow to highlight | `assets/hud/atlas.png` [49,5,5,4], **exact** |
| `lime` | `#56ff00` (glow `#b6ff00`) | lit or selected state, focus ring, "running" | `quit_lit.png`, **exact** |
| `moss` | `#123f00` | text of an unlit pill | `quit.png`, **exact** |
| `squirrels` | `#78ff16` | Squirrels text and blips on dark | `hud.json` radar `team_colours[0]`, **exact** |
| `squirrels-deep` | `#386818` (banner `#284010`) | Squirrels fills | textures 0729 and 0730, quantised |
| `tediz` | `#ff3737` | Tediz text and blips on dark | `hud.json` radar `team_colours[1]`, **exact** |
| `tediz-deep` | `#900000` (plate `#782820`) | Tediz fills | `teams.png` flag and texture 1858, quantised |
| `khaki` / `olive` | `#787058` / `#383820` | stencil wall / stencil ink | texture 0726, quantised |
| `hazard` | `#d8c800` | hazard stripes (with `night`) | texture 2300, quantised |
| `stats` / `stats-deep` | `#705090` / `#302040` | purple chrome headings | `stats.png` letters, quantised |
| `bubble` / `ink` | `#ffffff` / `#111111` | speech bubbles | `bubble.png` (white, shaded `#c8c8c8`) |
| menu chrome | red `#f00000`, green `#00d000`, blue `#0020f0`, cyan `#00c0f0`, gold `#f09000`, purple `#9000d0` | one colour per nav word | Spriters "Main Menu Text & Icons" sheet, approx |
| `p1..p4` | `#ff0000 #0000ff #00ff00 #ffff00` | player chips (decoration only: P2 is 2.3:1 on night) | `end.json` `results.colours`, **exact** |

Map the current `index.css` tokens like this: bunker→night, trench→cellar, mud→oak, khaki→bone, acorn→amber,
go→lime. Replace squirrels `#8cc63f` with `#78ff16` and tediz `#ff5b4f` with `#ff3737`: these are the game's own radar
colours.

Contrast (WCAG 2.x, computed):

| Verdict | Pairs (ratio) |
|---|---|
| AA for body text | bone on night/cellar/oak 15.9/14.1/11.0 · dust on night/cellar 8.7/7.8 · amber on night/cellar 9.8/8.7 · night on amber 9.8 · lime on night/cellar/oak 14.5/12.9/10.1 · moss on bone 9.9 · moss on lime 9.0 · squirrels on night/cellar 14.9/13.3 · tediz on night/cellar 5.4/4.8 · bone on tediz-deep/squirrels-deep/slate/stats 7.9/5.4/8.2/5.3 · ink on bubble/hazard 18.9/11.0 · script on night 4.9 |
| Large text only (≥24px, or ≥18.66px bold) | tediz on oak 3.75 · amber on choc-1 3.9 · ink on khaki 3.8 · script on cellar 4.4 |
| Decoration only | olive on khaki 2.4 · plaster on night 2.9 · p2 blue on night 2.3 |

## 3. Typography

All picks are verified on npm (`5.3.0`) and on api.fontsource.org. Load at most 3 families.

| Role | Pick (package, licence) | Alternates | Notes |
|---|---|---|---|
| Display: titles, nav, buttons | `@fontsource/luckiest-guy` (Apache-2.0, already installed) | `@fontsource/titan-one`, `@fontsource/sigmar-one` (OFL) | Closest to the bouncy menu sprites. Uppercase only: its lowercase is a second set of capitals |
| Hand: speech bubbles, script accents | `@fontsource/gloria-hallelujah` (OFL) | `@fontsource/kalam` (700), `@fontsource/patrick-hand-sc` | Loopy monoline marker, the nearest to the box's "Conker's". Caps in bubbles |
| Stencil: war labels, table heads, badges | `@fontsource/black-ops-one` (OFL, already installed) | `@fontsource/saira-stencil-one`, `@fontsource/stardos-stencil` (700) | Short labels only |
| Body | `ui-rounded, system-ui, sans-serif` (nothing to download) | `@fontsource-variable/nunito`, `@fontsource-variable/fredoka` | Add Nunito only if the system font looks off |
| Big numbers | The game's glyphs in `assets/hud/font/` at run time | `@fontsource/sniglet` (800), `@fontsource/short-stack` | Fallback when the pack is missing: Luckiest Guy |

Styling:
- **Box-art title (homage)**: Luckiest Guy, `color: amber`, `-webkit-text-stroke: .06em slate`,
  `paint-order: stroke fill`, an extrusion
  `text-shadow: .04em .04em 0 slate, .08em .08em 0 slate, .12em .12em 0 slate, .16em .2em 0 #000`, and
  `rotate: -3deg`. For a short accent above it ("Conker's"-style), use Gloria Hallelujah in `script` with no stroke,
  rotated -6deg. Extend the existing `.cartoon` class rather than adding a new one.
- **Chrome menu word** (GAME1/OPTIONS look, for nav items and section titles ≥24px):
  `background: linear-gradient(180deg, #fff6c0 0 10%, var(--c) 35%, color-mix(in srgb, var(--c), #000 45%) 75%, #ffd040);`
  with `background-clip: text; color: transparent; filter: drop-shadow(.05em .07em 0 #000)`. Wrap it in
  `@supports (background-clip: text)` and give it a solid `var(--c)` fallback. Use one colour per word, as the game
  does: Home in gold, Servers in cyan, Lobby in purple.
- Letter-spacing: display 0.02em, stencil 0.08em, script 0.
- Avoid: display or stencil type for paragraphs, gradients on text under 24px, more than two display treatments per
  screen, and faux-bold fonts that have only one weight.

## 4. Shapes, textures, motion

- **Pill button** (the game's TOTAL WAR / SET-UP / RESTART / QUIT pills): `rounded-full`, a 3px `#000` border,
  `inset 0 0 0 2px #d8d8d8, inset 0 -3px 0 2px #888`. Unlit: `bone` fill with `moss` text. Lit (hover, focus,
  selected): `night` fill, `lime` text and ring, and `0 0 10px lime` with a pulsing alpha (the game pulses the lit
  glow's alpha: `lab/research/mp-original/menus-and-flow.md`). Use the pill for secondary actions and options
  (map, mode, skill, radar ON/OFF, Lobby, Kill).
- **CTA** (Join game, Run): Luckiest Guy on `amber` with `night` text, a 3px black border, a hard
  `0 6px 0 choc-1` shadow. When pressed it moves 4px down and the shadow collapses.
- **Y/N round pills** (from the menu sheet): for any yes/no toggle.
- **Panels**. "Pub board": `cellar` fill, 3px black border, a 2px `oak` inner highlight on the top and left, a hard
  `6px 6px 0 #000` shadow, 12px radius, and four brass rivets drawn with a CSS `radial-gradient`. "War plate" (team
  cards): a runtime plate texture under a `linear-gradient(rgb(0 0 0 / .45))` overlay; fallback a flat `*-deep` fill.
- **Speech bubble** (CSS, for empty states, hints, errors and the loading line): white,
  `border-radius: 50% / 45%`, an inner shade `inset -6px -8px 0 #e0e0e0`, `ink` text in Gloria Hallelujah caps, and
  an `::after` triangle tail. It matches the game's bubbles (Total War briefing, cutscenes).
- **Dividers**: hazard tape `repeating-linear-gradient(-45deg, hazard 0 10px, night 10px 20px)`, 8px tall. A
  chocolate bar: up to 6 bevelled squares in `choc-1..4`, which double as a meter (players/max, bots 0-15 in rows of
  6 like the HUD). The battle plan's dashed route as an inline SVG.
- **CSS backgrounds**: `night`, plus a warm `radial-gradient` lamp glow at the top (amber at 6-8%), plus a vignette.
  Optional rain: 1px `bone` streaks at 4% opacity, slanted 100deg (the pub exterior is rainy).
- **Runtime textures**: all are 2d-map files, always present in a pack. Draw them `image-rendering: pixelated` at
  integer scales (3× or 4×).

| Path (`assets/maps/2d/textures/`) | Size | What it shows | Use |
|---|---|---|---|
| `0113-64x64-ci4rgba16-i.png` | 64×64 | dark olive stone bricks (current) | page background, under a 70% `night` overlay |
| `1734-64x64-ci4rgba16-i.png` | 64×64 | sandy khaki bricks | lighter backdrop for the dark chrome word sprites |
| `0726-64x64-ci4rgba16-i.png` | 64×64 | khaki wall, olive band, stencilled winged "S" | Squirrels header band (`repeat-x`) |
| `0599-64x64-ci4rgba16-i.png` | 64×64 | khaki wall, oxblood band, Tediz lightning roundel | Tediz header band |
| `0728-64x64-ci4rgba16-i.png` | 64×64 | khaki wall, plain olive band | neutral section band (Servers) |
| `0729-64x64-ci4rgba16-i.png` | 64×64 | green riveted steel plate with X rivets | Squirrels card (Lobby picker) |
| `1858-64x64-ci4rgba16-i.png` | 64×64 | red riveted steel plate | Tediz card |
| `0730-32x64-ci8rgba16-i.png` | 32×64 | green banner with a winged "S" | hanging pennant on the Squirrels card |
| `0606-32x64-ci8rgba16-i.png` | 32×64 | red banner with a lightning bolt | hanging pennant on the Tediz card |
| `3596-64x64-ci4rgba16-i.png`, `3597-64x64-ci4rgba16-i.png`, `3598-64x64-ci4rgba16-i.png`, `3599-64x64-ci4rgba16-i.png` | 4 × 64×64 | taped paper battle plan, laid out 2×2 in that order (3596 3597 / 3598 3599): "THEM" (teddy, gun) vs "US" (helmeted squirrel stick figure) | Home hero and Lobby art (128×128 at 3× = 384px) |
| `1667-32x64-ci8rgba16-i.png` | 32×64 | "DANGER" poster with a gas-mask skull | 404 and error states |
| `2300-32x32-ci8rgba16-i.png` | 32×32 | yellow and black hazard stripes under steel | divider (the CSS version is preferred) |
| `1633-32x64-ci8rgba16-i.png` | 32×64 | orange wooden crate | backdrop for the "Create server" box |
| `0172-64x64-ci4rgba16-i.png` / `0605-64x64-ci4rgba16-i.png` | 64×64 | Squirrel "S" stencil / Tediz lightning badge (current) | keep for the large emblems |

  Optional, and only if the pack exported map 34 (the default pack is 2d only): the green camo
  `assets/maps/34/textures/1606-32x64-ci8rgba16-i.png`. Fall back to a CSS camo.
- **Motion**: hero words bob ±2px/±1deg over 2.4s (my suggestion; not seen in the game); the lit glow pulses over
  1.2s; a press lasts 80ms. **Iris wipe** into `play.html`: a `clip-path: circle()` closing to the centre, then
  opening (the game's death iris, `lab/hud/README.md`). **Skull pop** on Kill: `skull.png` grows from 8 to 32px in
  about 0.5s (the elimination skull, 16 frames). A slot-machine roll on changing counters (the game's cash pickup
  rolls `$` signs). Under `prefers-reduced-motion: reduce`, drop the bob, rain, pulse, iris and roll, and use a fade
  of 150ms or nothing.

## 5. Game sprites usable at run time (`assets/hud/`)

Draw sprites marked `smooth: false` in `hud.json`/`end.json` pixelated. Leave the smooth ones bilinear (skull, radar,
bubble, iris, scope). **T** marks a text image: give it `alt="<word>"`, or `alt=""` next to visible or `sr-only` text.
The dark chrome words (squirrels, tediz, stats, win, lose, draw, you) are near-black letters with light rims, so they
disappear on `night`. Put them on khaki or plaster (`1734` bricks), or add `filter: drop-shadow(0 0 2px bone)`.

| Path | Size | Shows | Use |
|---|---|---|---|
| `squirrels.png` | 96×32 | **T** "SQUIRRELS" dark chrome | Lobby side picker title |
| `tediz.png` | 64×32 | **T** "TEDIZ" | Lobby side picker title |
| `teams.png` | 32×32 | four 16×16 cells: (0,0) SHC green helmet, (16,0) Tedi head, (0,16) Squirrels green/cream striped flag, (16,16) Tediz red flag | team icons in server rows, the picker and the score (the game's own MP HUD icons). Draw at 48px with `background-position` |
| `stats.png` | 64×32 | **T** "STATS" purple chrome | decorative heading (server facts, a future results page) |
| `win.png` `lose.png` `draw.png` `you.png` | 64×32 each | **T** WIN / LOSE / DRAW / YOU | 404 = YOU LOSE; a successful Run = YOU WIN |
| `restart.png` `restart_lit.png` `quit.png` `quit_lit.png` | 64×32 each | **T** pill buttons, dim and lime-lit | play.html's disconnect overlay (Restart = reconnect, Quit = Servers); also the model for the CSS pill |
| `players.png` | 32×32 | **T** P1 red, P2 blue, P3 green, P4 yellow (16×16 cells) | player slots |
| `font/00.png`…`font/94.png` | 4-12 × 5-11 each | **T** the HUD font, white (map in `hud.json` `font`) | big numbers (server count, bots, kill limit, clock), tinted with CSS `mask-image` + `background-color`. Put `aria-label` on the container |
| `lives.png` | 32×32 | **T** white 5×7 digits 0-9 (rects in `end.json` `lives`) | tiny counters |
| `skull.png` | 32×32 | chrome skull (smooth) | Kill button icon, "exited" status, 404 |
| `stat_icons.png` | 32×32 | (0,0) skull, (16,0) green crosshair, (0,16) 32×16 headshot | kill-limit field, the Aim controls row |
| `stat_deaths.png` | 32×32 | (0,0) alarm clock, (16,0) money bag, (0,16) RIP tombstone 16×15, (16,16) green item (unidentified) | Time and Lives fields, "exited" |
| `radar.png` | 32×32 | grey disc with a view wedge (smooth) | Radar ON/OFF field; a loading spinner with a red `#ff3737` blip |
| `atlas.png` | 64×64 | weapon icons (rects in `hud.json` `weapon.icons`); chocolate piece at [49,5,5,4] | controls table icons; an optional real chocolate piece |
| `grenade.png` | 32×32 | green grenade, 5×20 HUD icon at [6,8,5,20] | the Grenade controls row |
| `stick.png` | 32×32 | grey N64 analog stick | header of the controls table's pad column |
| `rank_st/nd/rd/th.png` | 16×16 | **T** white ordinal suffixes | optional |
| `bubble.png` + `tail.png` | 64×64 + 16×16 | white speech bubble and tail (smooth) | optional: the CSS bubble covers this |

Every runtime image needs a fallback when the pack is missing: reuse `Emblem`'s `onError` pattern in `app/src/ui.tsx`
(an icon or plain text instead).

## 6. Copy & tone

Cheeky, British, a bit crude, never cruel. Short lines, mostly in speech bubbles.

| Where | Line |
|---|---|
| Empty server list | "Nobody's here. Even the Tediz went home. Start one, you lazy git." |
| Run button / success | "Run it!" → "YOU WIN" sprite + "Server's up. Go on then." |
| Kill button (no confirmation) | "Kill it" (title: "No take-backs.") → skull pop → "Dead. Gregg's got it now." |
| Exited status | "Snuffed it" |
| Join game | "Get stuck in" |
| Bot skill hint | "Inbred: they shoot walls. Einstein: they shoot you." |
| Lives field | "As many as you think you can get away with." |
| Kill limit: unlimited | "Unlimited. Pace yourself." |
| Radar off | "No radar. Use your ears." |
| Side picker | Squirrels: "The good guys. Allegedly." · Tediz: "Evil teddies. Still cuddly." |
| play.html loading line | "Waking up the Tediz…", "Sharpening the chainsaw…", "Counting the chocolate…" |
| Connection error | "Can't reach the server. It's probably hungover." + Restart / Quit pills |
| 404 | DANGER poster + "YOU LOSE" + "Wrong turn, mate. Back to the pub." |
| Footer disclaimer (styled like the box's ADVISORY strip) | "ADVISORY: a fan remake. Not Rare's, not Nintendo's. No Rumble Pak needed." |
| Swearing | grawlix in `tediz` red, e.g. "What the ☠@⚡!" (the game censors swearing with red flashing symbols) |

## 7. Accessibility guardrails

- Body text: `bone` or `dust` on `night`, `cellar` or `oak` only. `tediz` red on `oak`, `script` and `ink`-on-khaki
  only at ≥24px. Chrome gradient text only at ≥24px, always with a solid fallback.
- Focus: `outline: 3px solid lime; outline-offset: 2px` (14.5:1 on night). On lime or amber fills, use a `night`
  outline. Focus also gets the "lit" look, but never colour alone: the outline always shows.
- Red vs green is the worst pairing for colour blindness. Always pair the team colour with the name and a shape (helmet
  or "S" vs teddy head or lightning).
- Motion: everything in §4's motion list respects `prefers-reduced-motion`. Nothing flashes more than 3 times per
  second (keep the grawlix static, with no flashing).
- Sprites: text images get `alt` equal to their word, or `alt=""` with `sr-only` text. Textures are CSS backgrounds or
  `alt=""`. Grawlix: `<span role="img" aria-label="censored">`. Pill and CTA targets ≥44px tall. Native `<button>` and
  `<select>` underneath every skin.

## 8. References

- https://en.wikipedia.org/wiki/Conker%27s_Bad_Fur_Day: the NA box art (File:Conkersbfdbox.jpg; black field, orange
  marker "Conker's", amber "BAD FUR DAY" with a slate bevel, the N64 red strip, the ADVISORY strip) and an in-game
  shot (File:CBFD-graphics.jpg). The EU box was not checked.
- The Spriters Resource (live site 403, read via Wayback): the game page
  https://web.archive.org/web/20251202121345/https://www.spriters-resource.com/nintendo_64/conkersbadfurday/ and its
  "Main Menu Text & Icons" sheet (GAME1-3, CHAPTERS, OPTIONS, MULTI, mode words, pills, Y/N)
  https://web.archive.org/web/20251202121351/https://www.spriters-resource.com/nintendo_64/conkersbadfurday/asset/62742/
- Conker wiki (read via its API): https://conker.fandom.com/wiki/Conker%27s_Bad_Fur_Day (censor symbols, chocolate,
  modes); /wiki/The_Cock_and_Plucker (the pub as menu hub, exterior shot); /wiki/Multi (Total War HUD shot);
  /wiki/Total_War (the briefing with the battle plan and a speech bubble); /wiki/Tediz; /wiki/SHC;
  /wiki/Anti-Gravity_Chocolate; /wiki/Gregg_the_Grim_Reaper; /wiki/Cash (tone and gags).
- https://www.timeextension.com/games/n64/conkers_bad_fur_day: 12 screenshots (speech bubbles, the 4-player HUD).
- Fonts: https://fontsource.org/fonts/luckiest-guy, /titan-one, /sigmar-one, /gloria-hallelujah, /kalam,
  /patrick-hand-sc, /black-ops-one, /saira-stencil-one, /stardos-stencil, /sniglet, /short-stack, /nunito, /fredoka.
  Each was checked on api.fontsource.org and registry.npmjs.org.
- Local: `lab/research/mp-original/menus-and-flow.md` (pills, lit-glow pulse, STATS layout),
  `lab/hud/README.md` (HUD, iris, skull), `assets/hud/hud.json` and `end.json`, `assets/README.md`.
- Not used: MobyGames, textures-resource.com and TCRF returned 403. I did not research late-N64 Rare UI
  (Banjo-Tooie, Perfect Dark, Jet Force Gemini).

## 9. What the site took from this (2026-10-12)

- **Used**: the palette tokens (`app/src/index.css` `@theme`, the game's radar colours for the sides), Luckiest Guy +
  Black Ops One + Gloria Hallelujah, the system rounded font for body text, the pill (`.pill`, lit = hover, focus or
  current page), the CTA (`.cta`), the pub board with rivets (`.board`), the speech bubble (`.bubble`), hazard tape,
  the chrome menu words (`.chrome`: Servers in cyan, Controls in gold, the lobby's question in light purple `#b070ff`, as
  `#9000d0` fails contrast on wood), the box-art homage logo, textures `0113`, `3596`-`3599`, `0730`, `0606`, `0729`,
  `1858`, `1667`, `assets/hud/skull.png`, the cheeky copy (empty list, statuses, taglines, 404, loading lines), and on
  `play.html` the iris, the chocolate loader and the bubble.
- **Skipped**: the dark chrome text sprites (`squirrels.png`, `stats.png`, `win.png`...: they vanish on the dark wood),
  the HUD font digits, the iris wipe on the landing (it would break middle-click on "Join game"), the skull pop, the rain
  and the grawlix. Screenshots stay out of git: they show the pack's textures.
