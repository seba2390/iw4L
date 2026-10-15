# Map load: what lives where

Three floors: **disk** (`asset_transport`), **format** (`fastfile_*` → the
`asset_iw4` IR), **products** (`asset_*` + `assets`). `assets` uses transport for
disk access and its source lanes to compile format captures into products;
`fastfile_*` knows the format and opens no files. Installing the match is
`session`, not here.

```text
console/ui `map` → session::lifecycle   SessionSwapRequest, tear the previous
                     occupancy down, then MapLoadApproved{request_id}
  → assets::match_load    start_match_load onto load_pool() (MatchLoadBusy),
                          poll_match_load → PreparedMatchReady
  → session::match_apply  prepare install → Staging → OnEnter(Live) → scene + SimWorld
```

Cancellation is `MatchLoadAbort(request_id)`: the walk returns
`MatchLoadOutcome::Canceled` at its next checkpoint, throwing nothing away.
Failure is `MapLoadFailed`, from discovery *and* install; a request ends once.
Startup and later load failures return to the main menu with a map error dialog;
OK or Escape dismisses it. The log keeps the full error.

The main world has three match states: `Absent`, `Loading`, and `Live`.
`ScopeControl` resolves requests once in `Last`, with teardown taking priority over
install and install over begin-load. Forced transitions also retire a replaced
`Loading` scope. Load tasks and sound-bank composition belong to `Loading`;
prepared catalogs, simulation, animation, scene plans and replay buffers belong
to `Live`. Fallible preparation completes before requesting `Live`.

`OnEnter` installs resources in registration order, derives publications, then
announces the installed identity. `WorldGeneration` is a nonzero scoped resource;
`WorldStamp` is its copied value in diagnostics and render frames. On exit,
release systems and entity removal hooks run while resources still exist. The
transition schedule then removes registered resources in reverse order and hands
them to `Retiring`. Match systems use `InMatch` on their main/fixed schedules;
render extraction reads optional main-world resources and clears missing frames.

`RoundPhase` exists only under `Live`. A changed server round serial forces a
fresh `Playing` scope even when the phase stays `Playing`. Simulation separately
owns `MatchScript` and `RoundScript`; only `RoundScript::new(&MatchScript)` creates
round state. Frontend menu locals and menu stacks have app owners separate from
their scoped in-match equivalents.

## The walk: `assets::session_load::load_prepared_match`

The single entry point; the composition root sees no `fastfile_*` and no
`AssetSink`. Inside are parallel walks on `load_pool()`: the archive, startup
materials, `common_mp`, the IW5/T5/T6 weapon bundles, language zones. `common_mp`
goes **before** the map — its techset tables are needed before
`build_world_draw`. Output is `MatchLoadOutcome::Ready` with a `PreparedMatch`
(world, one material population, clip, weapons, catalogs and `PreparedMap`).

A walk enqueues its own image plan the moment it is ready, and the catalog keeps
one row per image name. Two plans resolving the *same* archive entry into the
same payload share one decode; a name three games spell alike but fill
differently is an override, not a duplicate, and `load_jobs.csv`
([`BENCH.md`](BENCH.md)) counts the two apart. The per-game adapter is
`assets::lane` (`ZoneGame` → iw4/iw5/t5/t6; anything else, such as Black Ops III,
gets `UnreadLane`: every capability missing, an empty world). A lane gap is a
typed `LaneGap`, never silence.

`ZoneLane::load_common_mp` returns owned common builders (weapon, material, FPV,
world-model, projectile, animation, effect) with diagnostics and refusals; a lane
that needs assembled donors keeps a consumed-once `CommonFamilyCompiler` there.
The T6 lane keeps its captures private and owns donor lookup, technique linking
and weapon dressing; missing donors are typed refusals. `ZoneLane::load_world`
borrows the common `FilmVisionCatalog`; a map vision (or its parse error)
overrides the common one without fallback. These are CPU contracts, not shader,
media or GPU readiness. The common cache keys builders by path (`CommonKey`), so
in-place edits to source files need a rebuilt common set.

Installing it is `session::match_apply`. Preflight builds a `MatchInstallPlan`
(mode, doors, objectives, scene conversion, drawable world) and publishes nothing
until it hands one over. Commit publishes it, boots the sim and writes
`frame::InstalledMatch`. The authority then bakes bot navigation on `load_pool()`
and reports `BotNavigationReady` with the world generation; the walk graph is
cached (`nav`, keyed by content digest, schema and hull). Session owns readiness:
dedicated advancement needs navigation; listen advancement also needs
generation-matched rendering and audio unless headless (explicit silent audio
counts; failed audio does not). Old reports cannot release a newer world.

`MatchLoadOutcome::Ready` is the CPU package, not render-ready. The loading
screen also holds for GPU images, every queued pipeline and the `first_person`
stage: first-person materials and compositions (`render_anim::PreparedFpv`) and
body, world-weapon, map-model and projectile materials (`PreparedModelMaterials`).
Remote kits and dropped items are built at install. `load ledger:` in the log is
one line per load: what was handed over or compiled new against an earlier load.

## Cache, and poking it: `iw4l-artifacts/cache/<kind>/<prefix>/<key>`

Content-addressed leaf in `asset_transport::artifact_cache` (`cache_get` /
`cache_put`, `fnv1a64`). A miss is silent — the caller computes the value anyway
— and a hit must be the **same bytes** a miss would have written. The key names
every input; if the encoder changed, bump the format word. Live kinds: `mips`,
`wgsl`, `wgsl_dxbc` (BO2 shaders), `localize`, `nav` and `xwma_pcm` (a miss runs
the native Rust T5 WMA2 decoder). `IW4L_GAMES` holds the game trees; no folder
name is hardcoded. Live it is `make map mp_boneyard` ([`RUN.md`](RUN.md)), with
stages in `LoadProgress`.
