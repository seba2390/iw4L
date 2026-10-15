# Environment variables

The `IW4L_*` variables a player, host or tester sets. The game reads `.env`
for unset ones ([`MASTER.md`](MASTER.md) has the precedence). Switches read
"absent, empty, `0`, `false` or `off`" as off. Variables not listed here are
build stamps, render or script debugging, and the `IW4L_TEST_*` knobs of
headless runs; their meaning is beside the code that reads them.

**Games, files and logs**

| Variable | What it does | Default |
|---|---|---|
| `IW4L_GAMES` | Games root: the folder with the game installations (or shortcuts) and `.iw4l-server` files | Windows: the executable's folder; Linux/macOS: required |
| `IW4L_ARTIFACTS_DIR` | Where caches, runs, demos, dumps and logs go | `iw4l-artifacts/` in the working directory |
| `IW4L_SETTINGS_PATH` | Settings file | `~/.config/iw4l/settings.cfg` (`$XDG_CONFIG_HOME`); Windows: `iw4l-artifacts/settings.cfg` |
| `IW4L_PROFILE_PATH` | Script completion profile | `profile.cfg` beside the settings file |
| `IW4L_ACCOUNT_PATH` | Signing key, account ID and player data | `account.dat` beside the settings file |
| `IW4L_CACHE_BUDGET_MIB` | Size budget of the asset cache, in MiB | `4096` |
| `IW4L_LOG` | Log file path | `iw4l-artifacts/logs/<unix-time>.log` |
| `IW4L_LOG_LEVEL` | `error`, `warn`, `info` or `debug`, for both terminal and file | terminal `error`, file `info` |
| `IW4L_SOUND` | `off` or `0` silences match audio | sound on |

**Matches and console**

| Variable | What it does | Default |
|---|---|---|
| `IW4L_GAMETYPE` | Gametype of a `map` launch (`dm`, `war`, … for MW2; `zombies` for Black Ops; `zclassic` for Black Ops 2) and the mode a hosted room advertises | `dm` |
| `IW4L_SCORE_LIMIT` | Score limit of a Modern Warfare 2 match | the mode's own |
| `IW4L_CMDS` | Console script run at startup, like `--cmds` | none |
| `IW4L_CONSOLE_STDIN` | `1` also reads console commands from standard input | off |
| `IW4L_TEST_PLAYER` | Headless test player: `1`, `rounds`, `use`, `down`, `revive`, `watch` ([`BOTS.md`](BOTS.md)) | off |

**Online** ([`MASTER.md`](MASTER.md))

| Variable | What it does | Default |
|---|---|---|
| `IW4L_COMMUNITY` | The `.iw4l-server` descriptor to use (name beside the executable, or an absolute path) | the saved choice, or the only one |
| `IW4L_MASTER_HOST_NAME` | Host a room with this name when launching with `map` | not hosting |
| `IW4L_MASTER_JOIN` | Join the room with this advert ID; excludes `IW4L_MASTER_HOST_NAME` | not joining |
| `IW4L_MASTER_PASSWORD` | Room password for command-line hosting and joining | none |
| `IW4L_MASTER_MAX_PLAYERS` | Room capacity including the host, `2`–`18` | `18` |

**Display and measurement** ([`PERF.md`](PERF.md), [`BENCH.md`](BENCH.md))

| Variable | What it does | Default |
|---|---|---|
| `IW4L_PRESENT_MODE` | `Fifo`, `Mailbox`, `Immediate` or `AutoNoVsync` | `Fifo` |
| `IW4L_PIPELINED_RENDERING` | Switch; off renders in the main frame | on |
| `IW4L_FRAME_LATENCY` | Frames the CPU may run ahead of the GPU | `2` on macOS, `1` elsewhere |
| `IW4L_PERF` | Switch; records a Perfetto trace (Linux and macOS only) | off |
| `IW4L_PERF_FOCUS` | The one owner the trace follows, e.g. `script_model:192` | none |
| `IW4L_BENCH` | Switch; the in-process bench recorder and its reports | off |
| `IW4L_BENCH_FRAMES` | Rows of the per-frame table; `0` turns it off | `36000` |
| `IW4L_BENCH_JOBS` | `assets` writes one load-job row per asset | one row per class |
