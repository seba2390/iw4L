# Contributing

IW4L is a personal, experimental project. It is open so that it can be read,
built and fixed — not because it comes with an obligation to serve every
request. Decisions are the maintainer's.

## Bug reports

Reports are welcome, with no promised fix date. A report is prioritised against
what the project actually claims:

> A failure in a scenario the project says works outweighs a missing feature it
> never promised.

`README.md` says what is claimed. "Add everything the original had" is not a
roadmap item.

The Bug report issue form asks for what a useful report contains, and nothing
more:

* the release tag or commit
* OS and GPU
* which game's data you pointed IW4L at, and which map
* the steps, and what happened instead

That is the whole list. Do not attach your `.env`, a memory dump, or an archive
of the game — none of those will be read, and the last one cannot be accepted.

## Picking up work

Open work is split into issues of a few hours to two days each:
[open tasks](https://github.com/seba2390/iw4L/issues?q=is%3Aissue+is%3Aopen+label%3A%22status%3A+help+wanted%22).
Filter by `game: …` (Modern Warfare 2, Black Ops 1, Black Ops 2, `all`),
`mode: …` (Multiplayer, Zombies, `all`) and `area: …`. Each issue names its
source (a ledger id in `docs/fidelity/`, a milestone in `docs/PROGRESS.md`), an
acceptance list and a command that shows it working.

* `status: help wanted` — free to take. Comment that you are taking it; a
  maintainer assigns it and sets `status: in progress`. Agents with write
  access claim it themselves, as [`AGENTS.md`](AGENTS.md) describes.
* `status: in progress` — someone has it. A claim with no activity for seven
  days goes back to `status: help wanted`.
* `status: blocked` — waits on the issues under "Depends on".

Stopping halfway is fine: say what is done and what is left in a comment.

## Changes

Every change goes through a pull request into `main`: create a branch for
the task, commit there, push the branch, open the pull request with
`Closes #<issue>`. Nobody pushes to `main` directly; it is protected and
accepts merged pull requests whose checks pass.

* **Small and self-contained** — a fix, a crash, a wrong constant, a doc
  correction: open it directly, no issue needed.
* **Anything larger** — take an existing issue, or open one with the Task
  form first. A new crate, a new subsystem or a change to how data flows
  between `sim`, `render_frontend` and `net` is discussed in its issue before
  the branch grows: a large branch that arrives unannounced is likely to be
  turned down for reasons that have nothing to do with its quality.

Before opening a pull request, run what CI runs:

```bash
make publish-check                       # the tracked tree is the product and nothing else
cargo check --workspace --all-targets    # no rustc warning (CI: Linux and macOS)
cargo xtask lints --enforce              # clippy/rustc warnings never grow (xtask/lints/allow.txt)
cargo fmt --all -- --check
ruff check scripts && ruff format --check scripts
cargo xtask boundary --enforce           # each game runs on its own crates (docs/ARCHITECTURE.md)
cargo xtask progress                     # README table matches docs/PROGRESS.md
cargo xtask ledger-guard --base origin/main
cargo xtask port-graph check --ci        # scripts/port_graph.py still parses the tree
```

`cargo xtask lints` counts on whatever platform it runs; CI counts on Linux
with the toolchain `rust-toolchain.toml` pins, and a few cfg'd files count
differently on macOS. A warning you fixed leaves a stale entry: lower the list
with `cargo xtask lints --update` in the same pull request. `--update` never
raises a count — a new warning is fixed, not listed.

`CONTEXT.md` documents how the maintainer works — artifacts, iterations, agent
clones. It is a maintainer's workflow, not a requirement for contributing: you
do not need `make mr`, the iteration naming, or anything under `context/` to
change the runtime.

## Game data

Never attach, commit or link original game files — maps, weapons, sounds,
executables. That holds for issues, pull requests and release assets alike.
IW4L reads data from an installation each user already owns, and the project is
not a distribution channel for it.

## Written by an LLM

This whole project is written by an LLM.
