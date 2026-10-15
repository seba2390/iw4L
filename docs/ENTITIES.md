# World entities and the funnel

## There is one funnel

```text
human ─┐                                         ┌─→ client presentation
bot    ┴─→ TickInput ─→ sim::step ─→ TickResult ─┤   (snapshot)
                                                 └─→ demo / theater / killcam
```

`sim::step` returns a `TickResult` that carries the `Snapshot`; its signature,
callers and `StepReason`s are in [`SIM-STEP.md`](SIM-STEP.md) (code:
`crates/sim/src/carrier.rs`). Authority and client prediction step the sim;
demo, theater and killcam play recorded snapshots back and never call `step`.
`msec` is the step length; time enters **only** as an argument.

## What can be an entity at all — fixed by the data (`entity_iw4`)

The wiring is ours, the taxonomy comes with the data. The table is Modern
Warfare 2's layout; the crate decodes it and leaves transport to `net`.

| | |
|---|---|
| `EntityState` | replicated snapshot slot |
| `Centity` | client-side slot (pose plus snapshot anchors) |
| `ClientState` | per-client slot (team, `name[16]`) |
| `ET_PLAYER` / `ET_PLAYER_CORPSE` / `ET_ITEM` / `ET_MISSILE` | entity types |
| `Trajectory` | `evaluate_trajectory`: gravity 400, delta 800 |
| events | the entity-event ring, an 11-bit sequence |
| `ENTITYNUM_NONE` | `0x7FF` in `playerstate_iw4`/`trace_iw4`; T5 and T6 use 1023 |
| `playerstate_iw4` | player and input: the two ends of the funnel |

## Where everything else lives

* `sim/` — `gentity spawn world snapshot damage bullet* missile item corpse
  match_state score rules world_objects identities` (`ScriptModelId` is the one
  identity of a script model; a raw `u32` is not a second one);
* `frame` — `AppScreen`, `RuntimeRole`, `LaunchIdentity` and `ScopePlugin`:
  match/round states, scoped resource registration and entity retirement;
* `session` — standing the match up and tearing it down through `ScopeControl`;
  `net` — wire, deltas, prediction; `bots` — host-only controllers that observe
  through a sensor adapter, walk a ClipMap-baked graph and enter the same
  `TickInput`.

`net/transport/meta_wire.rs` encodes and parses metadata messages. The pure
`world_object_wire.rs` codec validates complete world-object updates;
`world_sync.rs` owns mutable baselines and periodic full synchronization.
Frame decoding commits its prepared world update after reliable and service
sections succeed, so a refused frame leaves the world baseline intact. Snapshots
publish the **semantics** of DObj composition, never runtime trees or posed
vertices ([`ANIM.md`](ANIM.md)).
