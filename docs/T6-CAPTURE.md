# Capture BO2 Zombies' launch scripts

Needs Python 3.10+ and your own original BO2 Zombies installation. Start it
through Steam, connect online and wait for its launch patch to finish loading.
Leave it at the menu or in a local/private game, not a public multiplayer match.
The tool reads memory only: it never injects code, modifies the game, disables
protections or downloads/redistributes a patch. Captures stay on your machine.

## Linux / Proton

Find the original game's PID with `pgrep -af t6zm.exe` (not IW4L's PID), then:

```sh
cargo xtask t6-capture --pid 12345
# Equivalent without building Rust:
python3 scripts/t6_capture.py --pid 12345
```

The default capture root is `$XDG_CONFIG_HOME/iw4l/captured`, falling back to
`~/.config/iw4l/captured`. `--output PATH` chooses a different capture root.
If Linux denies reading a sibling process, do not disable ptrace restrictions
system-wide. An owner who chooses to elevate this one read can run:

```sh
sudo python3 scripts/t6_capture.py --pid 12345 --output "$HOME/.config/iw4l/captured"
```

An elevated run drops to the game's owner before saving files.

## Windows
Use 64-bit Python. In PowerShell, find the PID with `Get-Process t6zm` and run
from the repository, choosing your IW4L artifacts folder explicitly:

```powershell
py -3 scripts/t6_capture.py --pid 12345 --output C:\iw4l\iw4l-artifacts\captured
```

## Result and limits

The tool saves `maps/mp/gametypes_zm/_globallogic.gsc` under `scripts_zm/`.
Add `--module path/to/module.gsc` for other missing modules. It rejects malformed, changing or
already-linked imports, unsafe names and conflicting copies; existing files
are never replaced. Without the required module it saves nothing and exits
unsuccessfully. Restart IW4L and start your installed Zombies map; IW4L still
verifies and links the complete program before admitting a match.

If no intact copy remains, retry after restarting the original game and letting
its patch load. If that game cannot launch, cannot download the patch, or keeps
only modified copies, this scanner cannot recover it. The tool does not
reconstruct linked bytecode. Never upload captures or memory dumps to an issue.
