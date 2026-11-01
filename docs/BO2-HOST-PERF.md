# BO2 Zombies: local host measurement

Measured 2026-11-01: Linux 7.2.8, Ryzen 7 7800X3D (8C/16T), 64 GB RAM,
RTX 4080/Vulkan. Optimized `play` build of
`20977c8d0e296b03c7757da70aecbc990361b39c` (the #755 integration tip,
including #752), not a measurement of every later revision.
Nuketown `t6:zm_nuked`, `zclassic`, installed native scripts; module-set
fingerprint `c5b6ee1523d40931c011770323b110489d95dceb0ac6ce266eac23c3289c1172`.

## Method

Windowed listen host, 960x540, VSync on, sound off, warm cache, timescale 1;
no concurrent builds or injected system load. The no-window `serve` pilot
stalled before map readiness and is excluded: **not dedicated-server results**.
Local rendering shares the machine. God mode prevents the benchmark ending.
`IW4L_PERF=1` on the host; overhead included, not calibrated away. Each
measured window follows about 60 seconds of active-match warm-up.

Combat uses the existing `rounds` test player with `IW4L_TEST_WALK=1`.
The synthetic crowd uses `watch`, then starts 18 additional installed
`maps/mp/zombies/_zm::round_spawning_test` threads at 30 seconds. They run
native spawning/AI, but this is **not a normal high-round population**.
Counts below are five-second observations, not a per-tick population census.
Co-op uses two game processes and a private TLS relay on loopback, both
players stationary and invulnerable. It includes real transport work but no WAN.

## Results
Combat/co-op single trials, crowd repeated once; times in ms, nearest-rank percentiles over
`Step` spans between explicit marks. All traces passed the error/drop/overwrite gate.

| Workload | Window / steps | Zombies observed | Step mean / p95 / p99 / max | Full FixedUpdate p99 |
|---|---|---|---|---|
| Solo combat, rounds 1-3 | 120.04 s / 2401 | 0-8 | 3.09 / 5.00 / 5.70 / 8.55 | 6.44 |
| Synthetic crowd, round 1 | 90.04 s / 1801 | 20-29 | 7.98 / 10.02 / 11.13 / 13.68 | 11.90 |
| Synthetic crowd, independent repeat | 90.05 s / 1801 | 20-29 | 8.17 / 10.24 / 11.36 / 13.87 | 12.15 |
| Host + joined client, round 1 | 120.03 s / 2400 | 7 | 4.36 / 5.69 / 6.31 / 8.02 | 7.20 |

`Step` measures the authority simulation phase, not frame rendering or network
latency. `FixedUpdate` includes surrounding fixed-schedule work, not all rendering/I/O.
All trials sustained approximately 20 ticks/s, with no measured Step or FixedUpdate
over 50 ms. Step-start gap p99/max: combat 66.39/70.05 ms, crowd worst 61.50/62.96 ms,
co-op 51.19/52.85 ms. Cheap computation does not imply uniform windowed dispatch.

Co-op sent 2114 snapshots / 2,438,204 encoded bytes: 17.61 updates/s, 20.31 kB/s,
excluding relay/TLS overhead. The shared pacer held 286 updates; worst ten-second
window sent 120 and held 80. It combines a 25 kB/s budget and stale-ACK backoff;
the counter does not separate them. **Pacing holds, not measured packet loss**.

Compute headroom is substantial for this workload, not a maximum-capacity bound.
Four-player/high-round combat, long-session stability and WAN latency/loss remain unmeasured.
No runtime fix was made; no script-execution or match-install faults appeared.
