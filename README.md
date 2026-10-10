# IW4L

IW4L is an open-source runtime for several Call of Duty games, written in Rust with
[Bevy](https://bevy.org/): Modern Warfare 2 (2009), Black Ops (2010), Black Ops II
(2012) and Black Ops III (2015), with Modern Warfare 3 (2011) maps recognised. Point it
at copies you already own and it loads each installation's maps, models, textures,
weapons, sounds and scripts into its own engine, where you can play what is implemented,
then inspect or change how it works.

Every game runs on **its own** rules and data. Black Ops plays by Black Ops' rules,
Black Ops II by Black Ops II's, and so on; no game borrows another's movement, weapons,
HUD or scripts. A rule a game does not have yet is listed in that game's
[fidelity ledger](docs/fidelity/) instead of being filled in from another game. CI
enforces the separation on every pull request ([game boundary](docs/ARCHITECTURE.md)).

<p align="center">
  <img src="docs/screenshots/bomb-plant.jpg" width="49%" alt="Bomb planting in IW4L">
  <img src="docs/screenshots/tanker-explosion.jpg" width="49%" alt="Tanker explosion in IW4L">
</p>

## What you can try

Launching without a map opens the [game library](docs/FRONTEND.md): it needs no game
data, finds or scans for installed games, and gives each its own menus, settings and,
where the game has them, zombies maps.

| Game | State |
|---|---|
| Modern Warfare 2 | Multiplayer maps play: movement, combat, killstreaks, classes, bots, demos, online through a master ([run guide](docs/RUN.md)). |
| Black Ops | Zombies (Kino der Toten first) runs Black Ops' own zombie scripts with Black Ops movement; co-op hosting. Roadmap: [`BLACKOPS_TODO.md`](BLACKOPS_TODO.md). Multiplayer maps are refused until their gametypes have their own rules. |
| Black Ops II | Experimental [FFA, TDM and Classic zombies](docs/T6.md), with native weapons, [in-match classes, settings and HUD](docs/MULTIPLAYER-UI.md). Its own compiled scripts are read and decoded (`gsc_t6`); running them in place of IW4L's interim rules is the work in progress ([ledger](docs/fidelity/t6.md)). |
| Black Ops III | Shadows of Evil's compiled scripts are read and translated (`gsc_t7`); its world is in progress, not playable yet ([ledger](docs/fidelity/t7.md)). |
| Modern Warfare 3 | Maps are recognised and refused until MW3's own rules exist. |

Gameplay remains incomplete everywhere; expect missing behaviour, bugs and desyncs.
APIs, configuration, caches and the wire protocol change between commits; multiplayer
peers must run the same build.

## Windows: prebuilt release

1. Download `iw4l-windows.zip` from [Releases](../../releases) and extract it into an
   empty writable folder. The archive password is `t.me/contextrot`.
2. Launch `iw4l.exe`. It finds MW2 in your Steam libraries and creates a
   `Modern Warfare 2.lnk` shortcut next to itself. For an install outside Steam, create
   that shortcut to your MW2 folder yourself.

For online play, put the `.iw4l-server` file you received from a server operator next
to `iw4l.exe`. With it the game finds that master and updates itself on launch. Details:
[Windows guide](docs/WINDOWS.md).

## Build and run

Install Rust through rustup and GNU Make. [Build dependencies](docs/BUILD.md) cover
Linux's C/C++ toolchain and system libraries, and macOS's Xcode command line tools.
[Windows instructions](docs/WINDOWS.md) cover building and arranging a portable folder.

You need your own installation of each game you want to load. IW4L distributes no game
assets and reads installations without patching or replacing their files; how a game's
data and scripts are read is in [`DECODING.md`](docs/DECODING.md). Caches, demos and logs go under `iw4l-artifacts/`; Linux settings use a separate
configuration directory described in the [run guide](docs/RUN.md).

From the repository root:

```bash
cp .env.example .env
# Edit .env: set IW4L_GAMES to the folder containing your game installations.
make map mp_boneyard CMDS='wait world; spawn 0; force_match_start; bot add 3'
```

This builds the optimized `play` profile and starts a local Modern Warfare 2 match with
three bots. `force_match_start` skips the warmup that otherwise freezes movement. Other
games' maps take their game's prefix:

```bash
cargo run --profile play -p launcher                         # the game library
IW4L_GAMETYPE=zombies  cargo run --profile play -p launcher -- map t5:zombie_theater
IW4L_GAMETYPE=zclassic cargo run --profile play -p launcher -- map t6:zm_nuked
cargo run --profile play -p launcher -- map t6:mp_raid
```

On native Windows, use `powershell -ExecutionPolicy Bypass -File .\scripts\play.ps1`
from the repository root. It builds and launches `target/play/iw4l.exe`, preserving
the repository's `iw4l-artifacts/` settings and caches. Append `map t6:mp_raid`
to load BO2 directly. Unoptimized `target/debug/iw4l.exe` is unsuitable for frame-rate tests.

## Inside the engine

| Area | Implementation |
|---|---|
| Assets | Native FastFile readers convert game data into a shared intermediate representation. |
| Shaders | Each game's own shader bytecode is translated to WGSL: Direct3D 9 Shader Model 3 (MW2, Black Ops) and Direct3D 11 Shader Model 5 (Black Ops II). |
| Scripts | GSC source from the game's zones is compiled; Black Ops II and III's compiled script modules are decoded. |
| Rendering | World geometry, models and effects feed one sorted draw-surface list. |
| Simulation | Server authority, client prediction and replay share one simulation step over explicit Bevy ECS state. |
| Networking | Custom UDP traffic; a QUIC master provides browsing and relaying. The host simulates the match. |

## Where to go next

- [Run guide](docs/RUN.md): console commands, classes and demo playback; [master setup](docs/MASTER.md) for playtests.
- [Rendering](docs/RENDER.md) and [simulation](docs/SIM-STEP.md): inspect the engine's implementation.
- [Map loading](docs/MAP-LOAD.md), [GSC runtime](docs/GSC-RUNTIME.md) and [bot AI](docs/BOTS.md): starting points for experiments and modifications.
- [Game boundary](docs/ARCHITECTURE.md), [how games are read](docs/DECODING.md) and the per-game [fidelity ledgers](docs/fidelity/): what each game has of its own and what it still lacks.
- [Documentation index](docs/INDEX.md), [contributing](CONTRIBUTING.md) and [security reports](SECURITY.md).

This whole project is written by LLMs.

## Acknowledgements and license

Thank you to all contributors for code, bug reports, testing and feedback.
Special thanks to **ju1cedr1nker** and **silvernote03** for QA testing weapons,
maps, attachments and other gameplay features.

[OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) and its [iw4x-x64
fork](https://github.com/iw4x-x64/oat) informed asset layouts;
[IW4x](https://github.com/iw4x/iw4x-client) informed asset and protocol behavior;
[KisakCOD](https://github.com/SwagSoftware/KisakCOD) informed engine structure.
[Ghidra](https://github.com/NationalSecurityAgency/ghidra) was used to inspect the
original binaries.

Movement implementation history and source boundaries are recorded in
[its provenance note](docs/provenance/movement-iw4.md).

IW4L's source is licensed under [Apache 2.0](LICENSE). Preserve required attribution and
bundled font license texts when redistributing; see [NOTICE](NOTICE). Original game
assets and trademarks belong to their owners. IW4L is unaffiliated with them.
