# Headless presentation recovery

The approved `network_impairments` scenario feeds decoded production-session
snapshots into production `RemoteProxy`, `FrameClock` and the script-mover/actor
pose sampler at 60 Hz. Players retain each accepted snapshot; entities adopt
the frame's newest, matching live reconciliation. No window/GPU is initialized.

```sh
make network-impairments ARGS='--samples 1 --seed 1234'
make network-impairments ARGS='--profile burst --workload motion --require-recovery 1'
make network-impairments ARGS='--profile latency --samples 1 --require-recovery 1 --recovery-budget-ms 1000'
make network-impairments ARGS='--profile clean --require-baseline-smoothness 1'
```

`busy` adds 128-192 changing background entities; `motion` omits them. Both
use one transport client, two player rows and one linear script-mover probe.
Player/entity probes move along X at 120 units/s: independent position/time truth.
Motion/payloads continue during recovery; times are virtual, not CPU timings.
Link model and phases: [NETWORK-TESTING.md](NETWORK-TESTING.md).

## Measurements
Schema 2 `run.json` separates `remote_player` / `entity`, each with warmup,
baseline, impairment and recovery. It reports unchanged-position sample intervals
(`freeze_ms`, longest run), backwards motion, clock regressions, delays/ages,
world/reference/step errors. Only the player proxy exposes starvation. Entity reference
error is against clock minus 50 ms, not invented interpolator provenance.
Motion age comes from the known trajectory: it is **not input latency or RTT**.
Continuous metrics/optional samples include boundary-crossing holds and jumps.
Pacing counters distinguish byte-budget holds from stale-ACK holds: the first
blocking gate wins. Sent snapshot/encoded-byte counters are checked against the link.
`--samples 1` includes labelled per-sample arrays, clock adjustments and hold counters.

## Recovery Goal
Normal means no unavailable/starved/backwards motion, reference/step error
<=baseline maxima+0.25/+0.5 units, age/delay <=baseline p95+20/+10 ms, for a full
second. Recovery measures return to the observed baseline, not baseline quality.
`sustained_normal_from_ms` starts the final uninterrupted normal run through observation end;
the first confirmed normal window and subsequent bad samples are also reported.
No such run means `null`, not zero. Observation ends 14 seconds after restoration.
The default comparison goal is recovery starting within 1000 ms; change it with
`--recovery-budget-ms`. `recovery_goal_met` is separate from functional `passed`.
Use `--require-recovery 1` to make that goal affect the exit status and CI.
Baseline smoothness is separate: no holds/backwards motion and reference/step
error <=0.25/0.5 units. `--require-baseline-smoothness 1` gates it independently.
Targets are explicit engineering comparisons, **not AAA playability certification**.

Known examples calibrate both samplers. Baseline bounds, phase isolation,
monotonic clock and the trajectory oracle are regression checks.
This samples player and actor-style entity positions, not native AI, animation,
camera correction, prediction, hit registration, full ECS scheduling/fixed overstep,
rendered frames, actual sockets/QUIC congestion or impaired live BO2 gameplay.
