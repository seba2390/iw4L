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
+/- 60 ms jitter, 20% reordering with 150 ms extra delay); and a two-second
bidirectional outage halfway through. Delays are **one-way**, independently
sampled in each direction. Loss is per gameplay fragment, not per snapshot.
Reordering adds delay to selected fragments; the report counts actual
out-of-order arrivals, not just selected delays.
Each profile runs twice with the same network seed and compares all measurements.
A 10 ms virtual clock drives 20 Hz commands and snapshots for 30 seconds,
followed by five clean seconds (the last second only drains). No sleeps.
`--duration-ms` accepts multiples of 50 from 5000 to 120000; `--profile`
selects one named profile or `all`. Numeric impairment flags instead select
a custom profile; percentages are 0..100, delays 0..1000 ms.
Production `UdpAuthorityHub` and `UdpClientLink` exchange real encoded
commands, snapshot ACKs, compressed baseline deltas and fragmented snapshots.
The fixture changes a player and 128-192 entities, including entity additions
and removals. It checks decoded values and player-state hashes, command
deduplication, baseline use, final state/command recovery, control event order
and payloads, drained queues, expired partial packets and actual impairments.
The workload stops changing during the clean tail to check final-state recovery.
Progress/command-age budget: outage + 1000 ms + twice the maximum link delay.
Command history resends its oldest 16 unacknowledged entries at 20 Hz; this
is synthetic traffic, **not** the live client's render-paced resend policy.

Control messages use production reliable queues over an **ideal ordered,
reliable channel**: sampled losses add fixed 200 ms retries. That models the
contract provided by QUIC, not QUIC's retransmission/congestion algorithms.
Admission is pre-established; lobby/bootstrap, game simulation, prediction,
lag compensation, rendering, the real relay and BO2 Zombies are not tested.
`run.json` under `iw4l-artifacts/approved-tests/<run>/` records build facts,
seed, parameters, assertions, directional counters, queue peaks, snapshot age
and arrival gaps, command/event age percentiles and clean-tail recovery time.
Counters/percentiles include the recovery tail; budgets are regression checks,
not responsiveness guarantees. Times round up to the 10 ms clock.
`--output DIR` chooses a report directory. Times are virtual milliseconds,
**not measured internet RTT, frame time or input-to-photon latency**. Exit 0
means every profile and deterministic repeat passed; 1 means a scenario
failed; 2 means invocation/report I/O failed. A custom 100% loss run must fail.
