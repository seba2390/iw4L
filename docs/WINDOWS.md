# Portable Windows folder

Players download `iw4l-windows.zip` from a GitHub release (none is published yet;
build from source meanwhile, [`BUILD.md`](BUILD.md)); it holds the executable.
A server operator hands out the `.iw4l-server` descriptor separately.
`make release` packs every `.iw4l-server` in the root folder unchanged into `iw4l-windows-community.zip`; `make launcher windows` writes the same under `dist/windows/`.
The archive password is `t.me/contextrot`. Extract into a dedicated writable folder:

```text
IW4L/
├── iw4l.exe
├── Modern Warfare 2.lnk
├── Black Ops.lnk
├── Black Ops II.lnk
├── Modern Warfare 3.lnk
├── community.iw4l-server    optional
└── iw4l-artifacts/          created on first launch: saves, caches, demos, logs
```

`iw4l.exe licenses` prints the licence and notice texts compiled into the executable.
The runtime reads the installations those shortcuts point to. Every game is
optional; the game library lists what it finds. For each missing title,
`iw4l.exe` checks the Steam libraries and creates its shortcut when exactly one
install qualifies. Existing shortcuts and titles already found in the
configured folders are preserved. If several installs qualify, or a game lives
outside Steam, use the library's Scan a Folder for Games or Game Installations,
or create the shortcut yourself.

Launch `iw4l.exe`. Before starting the game or contacting QUIC, it checks its
community's HTTPS manifest. An unchanged executable starts normally. An update
is downloaded, decompressed with a size limit and verified against SHA-256.
A temporary copy of the same executable waits for the original process, replaces
it and restarts with the original arguments. Failed installation or spawning
restores the previous executable. There is no permanent second executable.

The temporary files are `iw4l.update.exe`, `iw4l.previous.exe` and a helper in
`%TEMP%`; the next verified startup removes the backup and helper
(`iw4l.update-helper` records cleanup). `iw4l-artifacts/iw4l.update.lock`
serializes update checks. Each check logs DNS/TLS settings, HTTP results,
download progress, verification and handoff to the console and
`iw4l-artifacts/logs/`. DNS/connect waits are limited to 8 s, each HTTP request
to 180 s. If checking or downloading fails, the reason is logged and the
installed game starts with the selected community; online play still needs a
reachable master and a compatible version. Without a community descriptor,
local development launches without checking.

A descriptor pins a public CA and the master's TLS name for both HTTPS and QUIC.
Use a descriptor from a trusted source: its operator can distribute executable
updates. One adjacent `.iw4l-server` is selected automatically. With several,
choose under Options → Community Servers → Apply & Restart; the choice is saved
beside the executable in `iw4l.community.json`. With no choice, the menu opens
offline; `IW4L_COMMUNITY` overrides the saved choice. Invalid adjacent
descriptors are skipped. See [`MASTER.md`](MASTER.md).

On Windows, the executable directory is the working directory; `.env` and
`IW4L_GAMES` can override game discovery. Publishing: [`DEPLOY.md`](DEPLOY.md).
