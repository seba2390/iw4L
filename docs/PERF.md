# Native Perfetto

Runtime observation is `iw4l-artifacts/runs/<uuid>/trace.pftrace`, on Linux and
macOS only: on Windows the recorder compiles to nothing and `IW4L_PERF=1`
records no trace. Recording is off unless `IW4L_PERF=1` (`make bench` /
`make bench-live` / `make scenario` / `make chaos` / `make lifecycle-*` force
it on). Where the *run* spent its time, rather than one frame, is
[`BENCH.md`](BENCH.md). CPU sampling with Linux `perf` is `make bench-perf`
(Linux only; `perf_event_paranoid` ≤ 2).

```bash
make bench-live                            # record + compact p50/p90/p95/p99/hot-path report
make bench-overhead                        # 10 alternating on/off pairs + paired 95% CI
make bench-overhead VAR=IW4L_BENCH         # the same, for the in-process bench recorder
cargo xtask perf-overhead --pairs 10 --bin target/play/iw4l --zone mp_highrise --cmds 'wait world; wait 8s; quit' --focus script_model:192
make scenario                              # scripted match: IW4L_PERF=1, then cargo xtask scenario
make chaos                                 # RPG splash run: IW4L_PERF=1, then cargo xtask chaos
make lifecycle-swap                        # occupancy: IW4L_PERF=1, then cargo xtask live swap
make lifecycle-all                         # run all five occupancy recipes and read them back
cargo xtask bench                          # rerun the report on the newest run
cargo xtask bench iw4l-artifacts/runs/<uuid>
cargo xtask scenario                       # scripted-match report on newest .pftrace
cargo xtask chaos                          # RPG-splash report on newest .pftrace
cargo xtask net-feel                       # input-to-photon report on newest .pftrace
cargo xtask live swap                      # lifecycle occupancy on newest .pftrace
cargo xtask query slices.sql
cargo xtask query <uuid> render.sql         # render topic pack
IW4L_PERF_FOCUS=script_model:192 make map mp_highrise CMDS='wait world; wait 5s; quit'
cargo xtask query <uuid> render_owner.sql   # selected owner plan → DrawIndexed
cargo xtask query "SELECT name, COUNT(*) FROM slice GROUP BY name"
```

Each run has `trace.pftrace` + `manifest.json`. Optional reports are
**offline** from Trace Processor (`xtask/perfetto/*.sql`). The runner fetches
https://get.perfetto.dev/trace_processor if `TRACE_PROCESSOR` / PATH are empty.
The in-process ring is 32 MiB. Every reader rejects Perfetto errors, drops, and
overwrites; buffer exhaustion fails the read rather than reporting a partial trace.

Pin the run UUID and verify the manifest printed by every reader: zone, role,
workload, and git must match the claim. The implicit newest run is a convenience,
not a citation. A state-only question may use console `dump [name]`,
which writes one `iw4l-artifacts/dumps/*.txt`; it contains no history or timing.

Per-item overlay/draw spans are forbidden. `IW4L_PERF_FOCUS` admits one owner;
without it there is no owner census. Semantic events stay rare (`player_tick`,
death, feel, …).

## Without a trace: the host's ten-second summaries

A host logs, every ten seconds, each peer's snapshots (`snapshots to …`:
sent, held by byte budget or stale ACK, bytes per second, average and largest
packet, entities in the last one) and its authority steps (`authority
steps: …`: average and longest step against the 50 ms tick). Black Ops
Zombies co-op, Kino round 5 (`IW4L_SCRIPT_DVARS='set mutator_quickStart 1'`),
host and one joined client both headless on one M1 Pro (16 GiB), over the
first minute as zombies come in (#257):

| window | snapshots (held) | bytes/s | average / largest | entities | step avg / longest |
|---|---|---|---|---|---|
| 0–10 s | 196 (1) | 4351 | 221 / 2403 | 506 | 4.6 / 231 ms |
| 10–20 s | 200 (0) | 8386 | 419 / 1607 | 512 | 4.4 / 6 ms |
| 20–30 s | 200 (0) | 12835 | 641 / 1994 | 515 | 21.7 / 75 ms |
| 30–40 s | 199 (1) | 16988 | 853 / 2432 | 520 | 40.7 / 96 ms |
| 40–50 s | 189 (11) | 20451 | 1082 / 3254 | 524 | 47.3 / 122 ms |
| 50–60 s | 200 (0) | 13756 | 687 / 1260 | 524 | 44.6 / 83 ms |

With a crowd the snapshots near the 25 kB/s rate and the host's step nears
its 50 ms tick (the joined client runs on the same machine).
The first blocking pacing gate is counted: byte credit is checked before
stale-ACK backoff. Historical held counts above combine both gates; they do
not identify byte-budget pressure alone and are not packet-loss measurements.
