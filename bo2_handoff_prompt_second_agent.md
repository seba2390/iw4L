# Black Ops 2 (T6) Zombies — Second Parallel Agent: Client Effects, Vision, Lights & Hooks (#294 + #295 + #296)

Your mission is to implement and ship **Group 7 (`#294 + #295 + #296`)** in `seba2390/iw4L`: Black Ops 2's client-script (`.csc`) vision lerps, LUT banks, burn/electrified overlays, world fog banks and fog volumes, filter passes, shader constants, shellshock, client primary lights/mixers/flashlight, viewmodel FX and FX lifetimes, and map ropes/gump/LOD/sonar/aim hooks.

**CRITICAL — You are running in parallel on the same Mac as another agent:**
1. **Work in your own worktree:** use `/Users/sebastianmadsen/GitHubProjects/iw4L/context/mrs/bo2` (or `/Users/sebastianmadsen/GitHubProjects/iw4L`), **NOT** `/Users/sebastianmadsen/GitHubProjects/iw4l-t6-power` (the other agent is actively working on `#287 + #289`, `#282 + #283`, `#284 + #285` in `iw4l-t6-power`).
2. **Scope strictly to `#294`, `#295`, and `#296`:**
   - **Touch:** `crates/sim/src/script/host/natives/t6/client/{effects.rs,entity.rs,mod.rs}`, `crates/sim/src/script/host/natives/t6/{effect.rs,extras.rs}`, `crates/assets/src/lane/t6.rs`, `crates/render_fx/`, `crates/render_frontend/`, `crates/render_gpu/`, `crates/lighting_iw4/`, `docs/fidelity/t6.md`, `docs/PROGRESS.md`.
   - **Do NOT touch** (unless coordinating at merge time): `crates/movement_t6/**`, `crates/game_t6/src/{movement.rs,fire.rs,fire_sound.rs,viewmodel.rs}`, `crates/sim/src/script/host/natives/t6/{weapon.rs,paths.rs,combat.rs}` — the other agent owns those.
   - Note on `t6.movement.shellshock` (listed on #294): if the other agent is editing `crates/movement_t6/` and `crates/game_t6/src/movement.rs`, implement `GameVision::shellshock` (`t6.vision.shellshock`) in `crates/game_t6/src/lib.rs` first and only touch `movement_t6` after checking `git diff origin/main` or leave `t6.movement.shellshock` for last once `#287 + #289` merges.
3. **CPU & Process Contention:**
   - Always cap cargo jobs at `-j 2` or `-j 4`:
     ```sh
     cargo build --profile play -p launcher -j 3
     ```
   - Before starting a headless or windowed game run, check `pgrep -f "iw4l"` so you never run two game instances at the same time.
4. **Strict Rules:**
   - **Be short:** 1–3 sentences per message. No preamble, no recaps.
   - **No Copilot credentials:** never put `Co-authored-by: Copilot ...` or any Copilot trailers/mentions in commits or PR bodies.
   - **No viewing images:** you are **NOT** allowed to call `view_image` or look at pictures. Use `diag::info!` logs and headless/windowed log assertions.
   - **Zero shortcuts or approximations:** read every rule from Black Ops 2's binary (`/t6zm_image.bin`, base `0x400000` via PyGhidra MCP or `~/iw4l-re/t6vm/`) and cross-check against BO2's own zone data.

---

## 1. Step-by-Step Startup & Claiming

1. **Claim `#294`, `#295`, and `#296` before your first edit:**
   ```sh
   for N in 294 295 296; do
     gh issue edit "$N" -R seba2390/iw4L --add-assignee @me \
       --remove-label "status: help wanted" --add-label "status: in progress"
     gh issue comment "$N" -R seba2390/iw4L --body "Claimed on branch t6/csc-vision-lights-fx."
     gh issue view "$N" -R seba2390/iw4L --comments
   done
   ```
2. **Prepare your worktree (`/Users/sebastianmadsen/GitHubProjects/iw4L/context/mrs/bo2`):**
   ```sh
   cd /Users/sebastianmadsen/GitHubProjects/iw4L/context/mrs/bo2
   git fetch origin
   git checkout -B t6/csc-vision-lights-fx origin/main
   ```
3. **Commit frequently & rebase safely:**
   - Make local WIP commits (`git commit --no-verify -m "wip: ..."`) as each piece compiles.
   - Other agents merge into `origin/main` frequently. **Never** run `git reset --soft origin/main` after `git fetch`; always squash onto your merge-base (`git reset --soft $(git merge-base HEAD origin/main)`) and then `git rebase origin/main`.

---

## 2. The Three Issues in Your Group (`#294 + #295 + #296`)

You can ship all three in one PR (`Closes #294, Closes #295, Closes #296`) or split `#296` / `#294` / `#295` into incremental PRs if faster.

### Issue #294 — *BO2 Zombies: Vision lerps, LUTs, fog banks, overlays and shellshock*
- **Ledger IDs in `docs/fidelity/t6.md`:**
  - `t6.csc.vision.lerp` (line 1080) — `setvisionlerp` / two-visionset lerp by fraction in `natives/t6/client/effects.rs`.
  - `t6.csc.vision.lut` (line 1084) — colour LUT bank selection (`setlutbank` / vision LUT).
  - `t6.csc.vision.burn` (line 1087) — `setburn` overlay (used by TranZit lava).
  - `t6.csc.vision.electrified` (line 1090) — `setelectrified` overlay.
  - `t6.csc.fog.world_bank` (line 1093) — world fog bank switching (`setworldfogactivebank`).
  - `t6.csc.fog.volumes` (line 1096) — `GfxWorld` fog volumes and script fog bank switches.
  - `t6.vision.fog` (line 531) — server `setvolfog` and `setculldist` (`natives/t6/extras.rs`) reaching the client renderer.
  - `t6.csc.filter.pass` (line 1100) — post filter pass builtins (`setfilterpassmaterial`, `setfilterpassenabled`, `setfilterpassconstant`, `setfilterpassquads`).
  - `t6.csc.shader.constants` (line 1105) — `setshaderconstant` / per-entity script shader constants.
  - `t6.vision.shellshock` (line 1229) — `GameVision::shellshock` in `crates/game_t6/src/lib.rs` (BO2 `.shock` format and screen/vision blend).
  - `t6.movement.shellshock` (line 729) — shellshock movement scale (coordinate before touching `movement_t6`).
- **Reference:** Black Ops 1 (`t5`) wired its visionsets and film constants in commit `8d1c39b9` (#200). Inspect `git show 8d1c39b9` for how `crates/render_frontend/` and `crates/render_gpu/` consume game vision/film/fog state.

### Issue #295 — *BO2 Zombies: Client lights, light mixers, flashlight and effect lifetimes*
- **Ledger IDs in `docs/fidelity/t6.md`:**
  - `t6.csc.light.primary` (line 1125) — `makelight` and `ComWorld` primary light records on the client (`natives/t6/client/effects.rs`).
  - `t6.csc.light.render` (line 1129) — rendering client script dynamic light overrides (`setlightcolor`, `setlightintensity`, `setlightradius`, `setlightfovrange`, etc.).
  - `t6.csc.light.mixer` (line 1133) — client light mixer evaluation and rendering (`_light_mixer.csc` on Buried/TranZit).
  - `t6.csc.light.mixer_models` (line 1138) — `CS_MODELS` table on the client for light mixer model swaps.
  - `t6.csc.player.flashlight` (line 1036) — player flashlight (`natives/t6/client/entity.rs`).
  - `t6.csc.fx.viewmodel` (line 1069) — bolting client effects (`playviewmodelfx`) to the viewmodel DObj.
  - `t6.csc.fx.lifetime` (line 1073) — `isfxplaying` reporting live FX instance lifetimes back to the client VM instead of a fixed guess.
  - `t6.csc.fx.bullet_tracer` (line 1077) — standalone script `bullettracer`.
  - `t6.fx.event_axis` (line 79) — server `playfx` event axis packing (`natives/t6/effect.rs`).
- **Reference:** Black Ops 1 (`t5`) wired client primary lights and dynamic light rendering in commit `7e00e9b8` (#188). Inspect `git show 7e00e9b8` to follow the exact pattern in `crates/render_fx/` and `crates/lighting_iw4/`.

### Issue #296 — *BO2 Zombies: Ropes, gump streaming, LOD scale, sonar and alternate aim hooks*
- **Ledger IDs in `docs/fidelity/t6.md`:**
  - `t6.csc.ropes.lookup` (line 1056) & `t6.csc.ropes.position` (line 1059) — `getrope` and `ropegetposition` (`natives/t6/client/entity.rs`).
  - `t6.csc.gump.stream` (line 1119) & `t6.csc.gump.hide_alpha` (line 1122) — `loadgump` and `sethidegumpalpha` (`natives/t6/client/effects.rs`).
  - `t6.csc.render.lod_scale_rigid` (line 1066) — `setlodscalerigid` (`natives/t6/client/entity.rs`).
  - `t6.csc.spectate.third_person_render` (line 1062) — `setthirdperson` / spectate render flag.
  - `t6.csc.weapons.sonar_attachment` (line 1021) — MMS sonar overlay (`setsonarattachmentenabled`).
  - `t6.csc.player.alternate_aim` (line 1032) — alternate aim parameters.
- **Approach for #296:** First check which of these builtins the compiled `.csc` scripts of Nuketown (`zm_nuked`), TranZit (`zm_transit`), and Buried (`zm_buried`) actually call (via `/Users/sebastianmadsen/iw4l-re/refs/t6-scripts/ZM/` or `IW4L_GSC_NATIVE_TRACE`). Implement the ones called by the maps, and close/narrow or document any unused ones in `docs/fidelity/t6.md`.

---

## 3. Reverse-Engineering & Testing Resources

- **PyGhidra MCP server:** binary `/t6zm_image.bin` (base `0x400000`; load `mcp_pyghidra-mcp_decompile_function`, `mcp_pyghidra-mcp_search_strings`, `mcp_pyghidra-mcp_list_xrefs`).
- **Client builtin tables & decompiles:**
  - `/tmp/t6csc/cl_funcs.txt` and `/tmp/t6csc/cl_meths.txt` map every client CSC builtin name to its handler address in `t6zm_image.bin`!
  - `sh ~/iw4l-re/t6vm/tools/dec.sh 0xADDR` or `~/iw4l-re/t6venv/bin/python ~/iw4l-re/t6vm/t6dis.py 0xADDR 0xLEN`.
  - Decompiled BO2 Zombies GSC & CSC scripts: `/Users/sebastianmadsen/iw4l-re/refs/t6-scripts/ZM/`.
  - Zone search & dump tools: `~/iw4l-re/zgrep/target/release/{zgrep,zdump,zraw}`. Zones live in `/Users/sebastianmadsen/Games/Call of Duty Black Ops II/zone/all/` and `/Users/sebastianmadsen/Collector/assets/BO2/`.
- **Headless & muted-windowed runs:**
  - Headless fast run:
    ```sh
    IW4L_TIMESCALE=4 sh ~/iw4l-re/t6vm/tools/run_plain.sh <your-worktree> zm_transit /tmp/t6-csc-test.log 'set sv_cheats 1; wait world; wait 60s; quit' 60
    ```
  - Muted windowed run (for render/postfx/light log verification without disturbing the owner):
    ```sh
    set -a; . "$HOME/GitHubProjects/iw4L/.env"; set +a
    IW4L_ARTIFACTS_DIR="$HOME/GitHubProjects/iw4l-bo2-artifacts" \
    IW4L_SETTINGS_PATH="$HOME/iw4l-re/t6vm/runs/settings.cfg" \
    IW4L_PROFILE_PATH="$HOME/iw4l-re/t6vm/runs/profile.cfg" \
    IW4L_SOUND=off IW4L_GAMETYPE=zclassic IW4L_LOG=/tmp/t6-win.log \
    ./target/play/iw4l map t6:zm_transit --cmds 'set sv_cheats 1; wait world; wait 30s; quit'
    ```

---

## 4. Pre-Push Gate Checklist (Run Before Every Push/PR)

1. `cargo xtask progress --write` (keeps `README.md` matching `docs/PROGRESS.md`).
2. `cargo xtask boundary --update` (if `allow.txt` shrank), then `cargo xtask boundary --enforce`.
   - Add no new `FamilyId::<game>`, `AssetNamespace::<game>` or `<iw4 crate>::` mentions in shared code.
   - Every `t6.*` id in code must be in `docs/fidelity/t6.md`.
3. `rustfmt --edition 2024 $(git diff --name-only origin/main...HEAD | grep '\.rs$')`
4. `make publish-check`: no untracked files staged, no `#[test]`, no retail hex addresses (`0x005f...`) or `FUN_00...` in tracked files.
5. `cargo clippy --profile play --no-deps -j 3 -p <touched crates>`: no new warnings in touched crates, and no `unwrap_or_default` in `sim`.
6. Rebase onto `origin/main` right before pushing; if `README.md` or `docs/fidelity/t6.md` conflicts with the other agent's merged PR, keep both sides' closed/updated ledger items and re-run `cargo xtask progress --write`.
7. Open the PR with `Closes #294`, `Closes #295`, `Closes #296`, watch `gh pr checks <n> -R seba2390/iw4L --watch --interval 20`, and squash-merge (`gh pr merge <n> -R seba2390/iw4L --squash --delete-branch`).
