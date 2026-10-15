# `bots` — host AI

The bots are IW4L's own AI; Modern Warfare 2 ships none. Authority tick:
observation → memory → utility → task → route → motor → UserCmd.

`bot dummy [N]` creates bots with no controller or perception queries. They keep
their aim and weapon, accept `bot tp/give/fire`, and take damage and respawn normally.
They retain normal bot loadouts/perks; `bot hold off` leaves dummies idle.
Bots submit `TickInput.cmds` like players — [`SIM-STEP.md`](SIM-STEP.md).

| file | owns |
|---|---|
| `observation.rs` / `memory.rs` | what a bot may know, and for how long |
| `sensor.rs` | visibility probes and the rotation that spends them |
| `controller.rs` / `task.rs` / `intent.rs` | scores, task choice, stages |
| `nav.rs` | the baked graph, A\*, resumable route requests |
| `query.rs` | the shared collision budget every subsystem draws on |
| `weapon.rs` / `motor.rs` | the weapon channel; aim and movement |
| `plugin.rs` | the tick systems and the 96-query `TRACE_QUOTA` |
| `roster.rs` / `unique_loadout.rs` | slots, names, `bot` command queues; weapon groups |

## The three rules that shape the rest

**A negative needs a probe that ran.** Visibility answers seen, evaluated and
not seen, or not evaluated. Only the second ends a contact; the third holds it,
briefly and without coordinates. Firing needs a positive observation of that
client, fresh on the tick the command goes out.

**A denied query is not a collision.** Perception, connectors, execution and
combat share one 96-query quota. A query that ran reports what it found even on
the last token; one that never ran comes back as denial, so no refusal is ever
fabricated from an empty budget.

**Work survives a tick that is granted nothing.** A route request owns its
phase — approach, attachments, search, validation — and every cursor inside it
is resumable, so a zero allocation repeats nothing and A\* keeps its frontier
under the shared 2048-expansion quota. Answers that follow from graph topology
alone are cached per request key until the graph changes; collision-dependent
refusals are rechecked.

## Movement and combat

The graph bakes world-aligned tiles at 48 units, refined to 24 near missing
support or a height change, with directed Walk, Drop and BreakGlass edges that
keep their entry and exit through execution. A fight moves to a *position*: one
baked support per sector around the threat, checked for line of fire and
exposure, then reached through the ordinary route executor — reachability is
never inferred from a component label.

Bots chase the compass objectives their team sees, except `defend` icons and pings;
the controller scores capture, plant, defend and defuse against travel time.

`HostController::decision()` exposes scores, stage and objective; the periodic
aggregate adds per-subsystem query counts and stuck bots. Both read live from a
running match, which is where bot behaviour is judged: `make map mp_boneyard`
([`RUN.md`](RUN.md)) and watch the aggregate.

## Test player

`test_player.rs`: `IW4L_TEST_PLAYER` drives the first non-bot player in headless runs
(the Black Ops and Black Ops 2 zombies checks use it) and logs it every 5 s: `1`
fights, `rounds` adds god mode and ammo, `use` buys what the map offers, `down`/`revive`
test going down, `watch` only logs. `IW4L_TEST_USE`, `_PERK`, `_GOTO`, `_BUILD` steer
the use test; `_WIELD`, `_PLACE`, `_THROW`, `_GRAB`, `_REVIVE`, `_RIDE`, `_WALK` add
actions; `_ACTORS`, `_WAITS`, `_VARS` add log fields.
