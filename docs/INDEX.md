# `docs/` — the one-page map

Start with [`AGENTS.md`](../AGENTS.md) (rules and how to take an issue) or
[`CONTRIBUTING.md`](../CONTRIBUTING.md). Open work is in the
[issues](https://github.com/seba2390/iw4L/issues), by `game:`, `mode:` and `area:` label.
Files here aim for ≤50 lines; `PROGRESS.md`, `fidelity/*` and `GSC-RUNTIME.md` are long by design.

| file | about | when to read |
|---|---|---|
| **Status** | | |
| [`PROGRESS.md`](PROGRESS.md) | each game's milestones and technical debt; the README table is generated from it (`cargo xtask progress --write`) | a pull request reaches or breaks a milestone |
| [`fidelity/`](fidelity/) | per-game ledgers (`iw4`, `t5`, `t6`, `t7`, `iw5`): every rule a game does not have yet, by `unknown!` id | before adding or closing a gap |
| [`provenance/`](provenance/) | where MW2 movement, recoil, sentry, FX and solid-recovery code came from | authorship and source boundaries |
| **Use** | | |
| [`BUILD.md`](BUILD.md) | build dependencies for Linux, macOS, native Windows (MSVC) and Nix; what CI checks | before your first build |
| [`RUN.md`](RUN.md) | `make map`, `--cmds`, controls frozen until `Playing`, `force_match_start`, the verb list and the traps | before your first live run |
| [`ENV.md`](ENV.md) | user-facing `IW4L_*` environment variables: what each does, its default | setting an env var or `.env` |
| [`WINDOWS.md`](WINDOWS.md) | portable player folder, self-update, community descriptors | building and running on Windows |
| [`FRONTEND.md`](FRONTEND.md) | game library: installed games, folder scan, per-game menus, zombies maps, loading screens | launching without a map |
| [`THEATER.md`](THEATER.md) | saved films and clips, previews, editor controls, camera keys, cut export | playing or editing a recording |
| [Approved scenarios](../crates/approved_tests/README.md) | gameplay lifecycle and two clients through the dev master | repeatable end-to-end checks |
| **Per game** | | |
| [`BO1-ZOMBIES.md`](BO1-ZOMBIES.md) | Black Ops Zombies: starting solo, headless and co-op, library maps, test-player variables | playing or extending BO1 Zombies |
| [`BO2-ZOMBIES.md`](BO2-ZOMBIES.md) | Black Ops II Zombies: starting, proven and untested maps, FFOTD capture, Origins | playing or extending BO2 Zombies |
| [`T6.md`](T6.md) | Black Ops II multiplayer bring-up and remaining gaps | loading a BO2 multiplayer map |
| [`MULTIPLAYER-UI.md`](MULTIPLAYER-UI.md) | Black Ops II multiplayer pause menu, saved classes, settings and HUD | editing a BO2 loadout during a match |
| [`KILLSTREAKS.md`](KILLSTREAKS.md) | MW2 EMP and sentry script/engine boundaries | killstreak damage or sentry rotation |
| [`KILLCAM.md`](KILLCAM.md) | archived cameras, life clipping, sound and FX transitions | projectile or autonomous weapon killcams |
| [`SKILL.md`](SKILL.md) | local Elo ratings, rated modes, account persistence and migration | skill updates or account sync |
| **Operate** | | |
| [`DEPLOY.md`](DEPLOY.md) | `make release` / `publish` / `deploy`: servers, hashed `.zst`, master by SHA | shipping a release |
| [`MASTER.md`](MASTER.md) | your own master over ssh: `cargo xtask master install`, certificates, what to hand players | standing up a relay |
| [`SERVER-ASSETS.md`](SERVER-ASSETS.md) | optional games, join requirements, disabled-family loading | hosting for players with fewer games |
| **Measure** | | |
| [`PERF.md`](PERF.md) | native Perfetto tracing (Linux/macOS), the manifest, `IW4L_PERF`, SQL | why a frame took 80 ms |
| [`BENCH.md`](BENCH.md) | `make bench`: map-load waterfall, frame time as a span tree, counters, the run package | where a run spent its time |
| [`FFI.md`](FFI.md) | native data ownership, lifetimes, nullability, Perfetto adapters | changing a native boundary |
| **Engine** | | |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | game boundary: layers, `game_api` traits, `unknown!` instead of another game's rule, `make boundary` | adding a game, a `boundary` failure |
| [`DECODING.md`](DECODING.md) | how a game's data, scripts and rules are read from the owner's install | before reverse-engineering a rule |
| [`MAP-LOAD.md`](MAP-LOAD.md) | `session` → `assets` → install transaction, lanes by `ZoneGame`, the artifact cache | a zone won't load, an asset went missing |
| [`ENTITIES.md`](ENTITIES.md) | the MW2 entity layout, where entity code lives, `TickInput` → `TickResult` | gameplay, networking, replay |
| [`SIM-STEP.md`](SIM-STEP.md) | `sim::step` for authority and prediction, `StepReason`, determinism | touching the step or prediction |
| [`GSC-RUNTIME.md`](GSC-RUNTIME.md) | script runtime reference: language, IR, compiler, BO2/BO3 translators, VM and natives, per-game startup | gameplay or script execution |
| [`GSC-POSTFX.md`](GSC-POSTFX.md) | script vision, color correction, blur, DoF and bloom, per game | debugging GSC post effects |
| [`RENDER.md`](RENDER.md) | the render crates, `IR → cull → drawsurf list → tess → material → SM3/SM5 → WGSL → wgpu` | the picture, techsets, lighting |
| [`MATERIALS.md`](MATERIALS.md) | compiled material state, preparation generations, adaptation provenance | material preparation |
| [`POSTFX.md`](POSTFX.md) | per-family film, bloom and highlight profiles; what is not drawn yet | comparing presentation between games |
| [`PRESENTATION.md`](PRESENTATION.md) | fire product receipts, UI lookup owners, editor capabilities | presentation ownership or retries |
| [`ANIM.md`](ANIM.md) | `anim_iw4`, `xmodel_runtime`, `asset_anim`, `render_anim` and who picks the clip | viewmodel, skeleton, bone hits |
| [`WEAPONS.md`](WEAPONS.md) | weapon configuration, consumer projections, host rules, cross-family FPV | attachments, combat, FPV |
| [`FAMILIES.md`](FAMILIES.md) | family identity, native dependencies, map-selected characters, FPV combinations | cross-family composition |
| [`AUDIO.md`](AUDIO.md) | AudioRuntime threads, fixed slots, per-game mix buses, what is missing | sound ownership or playback |
| [`CUES.md`](CUES.md) | compiled cue semantics, spatial/routing requirements, media decode policy | cue or media preparation |
| [`BOTS.md`](BOTS.md) | host AI pipeline, query budget, routes, objectives, the test player's `IW4L_TEST_*` variables | bot behaviour, scripted test runs |
