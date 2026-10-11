# Conker's Bad Fur Day modding plan

- ROM: `Conker's Bad Fur Day (USA).z64`, big-endian, game code `NFUE` (US, rev 0), 64 MB,
  SHA-1 `4cbadd3c4e0729dec46af64ad018050eada4f47a` (matches what the recomp and the decomp expect).
- Engine: Rare's N64 engine, native MIPS (R4300), `.game` code segment compressed with Rare's gzip variant.
- Anti-cheat / online: none. Offline single-player (multiplayer is local only). The user owns the dump.
  Ship `.nrm`/`.rtz`/patches only; never commit or share the ROM.
- Host machine: macOS 26.6.2, Apple Silicon. Xcode not installed (CLT only), so the game itself can't be
  built here; the prebuilt release is used. Homebrew llvm/lld 23, cmake, ninja, pkgconf installed (checked 2026-10-08).
  Python 3.9.6.

## Community route
- **CBFD-Recompiled** (https://github.com/sciaschi/CBFD-Recompiled), MIT/GPL-3, `V0.1.5` (2026-10-01).
  N64Recomp + N64ModernRuntime + RT64 (Metal on macOS). Playable to the end. Built on the mkst decomp.
  - Mods: `.nrm` (RecompModTool). `RECOMP_PATCH` replaces a function, `RECOMP_HOOK` / `RECOMP_HOOK_RETURN`
    run code before/after it. Config options via `recomp_get_config_*`; logging via `recomp_printf`.
  - Texture packs: RT64 `.rtz` (RT64 dev mode F1 inspector shows hashes and has **Replace**); GLideN64 `.htc` too.
  - Asset-only ROM hacks (e.g. uncensored patch) load via **Add ROM**; code-changing hacks are refused.
  - Example mods in `mods/`: `skip_intro`, `skip_cutscenes`, `cheats` (memory writes from a per-frame hook).
- **mkst/conker** decomp (https://github.com/mkst/conker): 1365/5916 functions matched upstream. Source of
  names/addresses (`conker.us.yaml`, symbol files) and of the recomp's `mods/syms/*.toml`.
- Not usable here: ConkerBFDReloaded (no macOS build; widescreen/free-camera fork), Conker64Recomp (doesn't run
  the game).

## Chosen route
Loader-API mods: `.nrm` mods for CBFD-Recompiled (code/behaviour), `.rtz` packs (art). Idea: **TBD**.

## Lab (set up, verified 2026-10-08)
- **Use `lab/release-fix/ConkerRecompiled-mod-test-AppleSilicon/`.** The V0.1.5 macOS release (`lab/release/`)
  runs the game but crashes with SIGBUS in `apply_regenlist` as soon as any code mod loads: its `__TEXT` isn't
  writable, so runtime function patching dies (issue #81, open). The fix build is the CI artifact of branch
  `test/macos-88` (run 37676460212, sha `9435b5dd`): entitlements `allow-jit`, `allow-unsigned-executable-memory`,
  `disable-executable-page-protection`, `disable-library-validation`, and `__TEXT` maxprot rwx. Switch back to an
  official release once one ships with the fix.
- `portable.txt` **next to `ConkerRecomp.app`** (RecompFrontend checks the bundle's parent folder on macOS)
  makes that folder the data folder: stored ROM `conker.n64.us.1.0.z64`, `mods/`, `mods.json`, `saves/`,
  configs. Nothing goes to `~/Library/Application Support`.
- Enable mods without the GUI: `mods.json` -> `"enabled_mods": ["<mod id>"], "mod_order": ["<mod id>"]`.
- Test run (no launcher, quits by itself, log on stdout):
  `ConkerRecomp.app/Contents/MacOS/ConkerRecomp --seconds 20 --window 960x720 > ../smoke.log 2>&1`
  (`--rom <path>` the first time, `--headless` for no window, but headless uses `Contents/MacOS/conker_data`).
  A healthy run ends with `[host] N seconds elapsed, ~57*N VIs; quitting`.
- Verified: vanilla V0.1.5 boots (Metal, M2 Max, 1420 VIs/25 s); fix build + prebuilt `conker_cheats.nrm`
  (`RECOMP_HOOK_RETURN`) loads and runs (1149 VIs/20 s, exit 0). Release + fix build were not quarantined
  (downloaded with `gh`), so no Gatekeeper prompt.
- Mod toolchain (`brew install llvm lld cmake ninja`): clone CBFD-Recompiled **at `9435b5dd`** (the build in
  the lab), or whichever release first ships the #81 fix. `V0.1.5...9435b5dd` touches no `mods/syms/` or
  `conker/` files (only `conker.toml` among the game files), and the prebuilt V0.1.5 Cheats `.nrm` loads on it,
  so the syms are compatible. Init only `tools/N64Recomp`, apply `recomp/n64recomp.patch` (as `build.sh` does), build
  `RecompModTool`, then `sh mods/build_mod.sh mods/<mod>`. Needs `conker/conker/include/2.0L` (`conker` submodule).

## Vertical slice (idea-independent)
1. Oracle: prebuilt Cheats `.nrm` loads and runs (done). Still to see in play: health/lives actually pinned.
2. Toolchain proof: own mod with `RECOMP_HOOK_RETURN("func_1501BBB8")` that `recomp_printf`s once; build,
   install, see the line in stdout.
3. Then the real idea.

## Unknowns / gotchas
- Hooks: functions with jump tables can't be hooked; hooking a function `conker.toml` already hooks drops that
  hook. `func_1501BBB8` (reads the controllers once per frame) is the per-frame hook.
- Names ending in an address (`resetSlotState_150104F0`) are AI guesses. Ground truth for addresses: mkst
  syms, US GameShark code lists, RAM inspection in game.
- `.game` is compressed in the ROM: REA on the raw `.z64` only sees the boot/init code. Decompress first
  (decomp's `make -C conker extract`) before static analysis of game code.
- Upstream HEAD moves daily; rebuild mods if the game's function layout changes between releases.
