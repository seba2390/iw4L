# Headless network impairment harness

`network_impairments` is an owner-approved, asset-free scenario: no game install,
launcher binary, display, GPU, sockets, certificates or master. It builds the
shared engine dependencies but never initializes graphics.
```sh
make network-impairments ARGS='--seed 1234'
cargo run -p approved_tests -- network_impairments --seed 1234
cargo run -p approved_tests -- network_impairments --profile combined --seed 42
cargo run -p approved_tests -- network_impairments --latency-ms 150 --loss-percent 5 --jitter-ms 40 --reorder-percent 10 --reorder-delay-ms 200
```

The default matrix is clean; 100 ms latency; 5% loss; 100 +/- 80 ms jitter;
25% reordering with 150 ms extra delay; combined (100 ms, 10% loss,
+/- 60 ms jitter, 20% reordering with 150 ms extra delay); and 250 ms / two-second
bidirectional blackouts. Delays are **one-way**, independently
sampled in each direction. Loss is per gameplay fragment, not per snapshot.
Reordering adds delay; the report counts actual out-of-order arrivals, not just selections.
Each profile runs twice with the same network seed and compares all measurements.
A 1 ms virtual clock drives 20 Hz commands/snapshots and 60 Hz presentation.
Five clean warmup seconds and five baseline seconds precede the impairment.
Sustained profiles last 30 seconds; blackouts have fixed durations. Then 14 clean
moving-workload seconds measure recovery; one last second drains.
`--duration-ms` accepts multiples of 50 from 5000 to 120000; `--profile`
selects one named profile or `all`. Numeric impairment flags instead select
a custom profile; percentages are 0..100, delays 0..1000 ms.
Production `UdpAuthorityHub` and `UdpClientLink` exchange real encoded
commands, snapshot ACKs, compressed baseline deltas and fragmented snapshots.
The default `--workload busy` changes a local player, a moving remote probe
and 128-192 background entities with additions/removals; `motion` omits the background.
Both include a constant-velocity script-mover probe through the entity-pose sampler.
It checks decoded values/hashes, command deduplication, baseline use, final recovery, control order
and payloads, drained queues, expired partial packets and actual impairments.
Entity/probe motion continues throughout recovery; local input stops at the boundary.
Progress/command-age budget: outage + 1000 ms + twice the maximum link delay.
Command history resends its oldest 16 unacknowledged entries at 20 Hz; this
is synthetic traffic, **not** the live client's render-paced resend policy.

Control uses production queues over an **ideal ordered/reliable channel** with
fixed 200 ms retries, not QUIC's retransmission/congestion algorithms.
Admission is pre-established; lobby/bootstrap, game simulation, prediction,
lag compensation, rendering, the real relay and BO2 Zombies are not tested.
Production presentation measurements/gates: [NETWORK-PRESENTATION.md](NETWORK-PRESENTATION.md).
`run.json` under `iw4l-artifacts/approved-tests/<run>/` records build facts,
seed, parameters, assertions, directional counters, queue peaks, snapshot age
and arrival gaps, command/event age percentiles and clean-tail recovery time.
Transport aggregates include all phases; presentation reports each phase separately.
`--output DIR` chooses a report directory. Times are virtual milliseconds,
**not measured internet RTT, frame time or input-to-photon latency**. Exit 0
means functional checks and deterministic repeats passed, not good playability.
`--require-recovery 1` also gates the recovery goal. Exit 1 means a check failed;
2 means invocation/report I/O failed. A custom 100% loss run must fail.
