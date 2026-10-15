# Build dependencies

Rust stable via rustup (`rust-toolchain.toml` selects the stable channel),
plus a C toolchain and the Bevy system libs. IW4L enables both `x11` and
`wayland` (`Cargo.toml`), so the Wayland headers stay required. Gamepad input
and rumble (`bevy_gilrs`) link libudev, so the udev headers (`systemd-devel` /
`libudev-dev`) are required too.

```bash
# Fedora
sudo dnf install gcc-c++ pkgconf-pkg-config libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel

# Debian / Ubuntu
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev

# Arch / Manjaro
sudo pacman -S gcc pkgconf libx11 alsa-lib systemd wayland libxkbcommon libxcursor libxrandr libxi
```

Upstream list (other distros, GPU Vulkan drivers): Bevy
[`linux_dependencies.md`](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

What is **not** needed for a native Linux build: `clang` / `llvm` / `lld` /
`glibc-static` — plain `gcc-c++` links it. `llvm` (the `llvm-lib` tool) is
only required for the Windows cross build, which `make release` runs on an
x86_64 Linux host ([`DEPLOY.md`](DEPLOY.md)):

```bash
sudo dnf install llvm        # Fedora: provides llvm-lib
make setup-windows           # rustup target + cargo-xwin, once
```

macOS needs only the Xcode command line tools (`xcode-select --install`)
and rustup: winit, wgpu (Metal), CoreAudio and gilrs link system frameworks,
and the `x11` / `wayland` features compile to nothing. Menu and
`make map mp_boneyard` run on Apple silicon (M2 Max, M4 Pro, Metal). Rendering
is pipelined on every platform (`IW4L_PIPELINED_RENDERING=0` serialises it);
macOS asks for a frame latency of 2 (`IW4L_FRAME_LATENCY=1` for 1).

Native Windows needs the MSVC toolchain (Visual Studio Build Tools, "Desktop
development with C++") and rustup's default `x86_64-pc-windows-msvc`;
`scripts/play.ps1` builds `launcher` and `iw4l-master` and starts the game.
Nix users get the toolchain and libraries from `flake.nix` (`nix develop`, or
direnv through `.envrc`, which also exports `.env`).

Game data: the Windows depot of a Steam copy, `steamcmd
+@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/MW2 +login
<user> +app_update 10190 +quit`, then `IW4L_GAMES=~/Games`. Then follow
`README.md` (Build and run); portable Windows is [`WINDOWS.md`](WINDOWS.md).

Match metadata uses the Git commit count as its numeric build number. Release
builders can set `IW4L_BUILD_NUMBER`; source archives need this variable. The
package version remains the textual build version.

CI (`.github/workflows/`) runs `cargo check --workspace --all-targets` on Linux
and macOS (no link, no Windows), `make publish-check`, `cargo xtask boundary
--enforce`, `cargo xtask progress` and, on pull requests, `cargo xtask
ledger-guard`; it has no game data and runs no clippy, fmt or test step. The
boundary check keeps each game's rules in that game's crates
([`ARCHITECTURE.md`](ARCHITECTURE.md)); a new crate needs a
`[package.metadata.iw4l] layer` or the check refuses it.
