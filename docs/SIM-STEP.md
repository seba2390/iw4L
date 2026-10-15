# `sim::step` — the one funnel

```rust
pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: StepReason,
) -> TickResult
```

`crates/sim/src/carrier.rs`. Everything able to change the authoritative world
enters through `TickInput`, including recorded shot-sample provenance. The tick
publishes a `TickResult` containing the snapshot, per-client action outcomes,
script effects and presentation metadata. `try_step` returns
`Result<TickResult, script::Fault>`; `step` panics on script failure. Authority
requires a loaded, started GSC program.

## One function, two callers

Server authority (`net::authority`) and client prediction (`net::client::predict`)
call this same `step` (mostly via `try_step`). They differ only in `StepReason`:

| reason | meaning |
|---|---|
| `AuthorityFrame` | the authoritative advance — the only reason `advances_authority_world()` holds |
| `PredictNew` | the local client running ahead of the server |
| `Replay` | prediction re-running commands the server has not acknowledged yet |

Humans and bots both arrive through `TickInput.cmds`. Demo, theater and killcam
play recorded snapshots back and never call `step` ([`ENTITIES.md`](ENTITIES.md)).

* **State is explicit.** `&mut SimWorld` owns the `bevy_ecs` world, so two of
  them step side by side (authority and prediction) without touching each
  other. Replacing `SimContent` retires body caches and collision
  history/claims; reinstalling the same owner preserves them.
* **Time is an argument.** `tick` and `msec` arrive per call instead of living
  in the world, so re-running a tick means passing it again.
* **Input is canonical.** `TickInput::canonicalize()` sorts commands and
  actions by `ClientId`, so the same set of inputs is the same input whatever
  order the network delivered it in.
* **Publication is closed.** Networking consumes the returned snapshot and
  effects without draining the simulation world. Each action reports applied,
  accepted for script execution, or refused; absence of a rejection is no verdict.

`SimWorld::adopt_snapshot` installs an authoritative snapshot;
`SimWorld::adopt_prediction_snapshot` reconciles the local client against one.
Both are in `carrier.rs` and answer with an `AdoptReport` (`adopt.rs`). See
[`GSC-RUNTIME.md`](GSC-RUNTIME.md) for script execution.
