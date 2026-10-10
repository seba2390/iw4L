# Reading a game: data, scripts, engine rules

How IW4L learns what each game does, and why none of it ends up in the
repository. The rule it serves: [`ARCHITECTURE.md`](ARCHITECTURE.md) (every
game by its own rules; CI checks the code, never game files).

**What may be used.** Only an installation the user owns, read in place and
never modified or redistributed. Research is for interoperability: knowing a
format well enough to load data the user already has. The repository holds the
code that reads a format and the ledgers ([`fidelity/`](fidelity/)) — never a
game file, an extracted asset or script, an executable, a memory image, or a
retail address. `make publish-check` refuses addresses and decompiler names;
[`CONTRIBUTING.md`](../CONTRIBUTING.md) keeps game data out of issues and PRs.

**Data.** Each game's zone container has its own format crate
(`fastfile_iw4`, `_iw5`, `_t5`, `_t6`, `_t7`): header, cipher and compression,
asset list, then the assets walked by that game's load plan. Layouts are
checked against the files: every pointer resolves, every asset of a zone walks.

**Scripts.** MW2, MW3 and Black Ops ship GSC source in their zones; IW4L
compiles it (`gsc`). Black Ops 2 and 3 ship compiled modules: `gsc_t6` and
`gsc_t7` read their headers and tables and decode the bytecode, which is then
translated onto IW4L's script IR. A layout is accepted only when the module
agrees with itself — every table ends where the next begins, every function
decodes to its end, every call site the import table lists is an instruction
start, every string reference falls inside an operand.

**Engine rules** (movement, weapon timing, what an opcode does) live in the
game's executable. Its code is studied in the owner's copy only: on disk, or —
where the executable only becomes readable once it runs — from the running
game in a local Windows VM on the owner's machine (Parallels, driven with
`prlctl`), with the image kept on that machine. Disassembly is Capstone
(Python, with `pefile`) or Ghidra. A value from code is cross-checked against
the game's own data before it lands, e.g. Black Ops 2's opcode operand layouts
against all 9 044 zombies functions.

**References** (OpenAssetTools, IW4x, KisakCOD, community notes — credited in
`README.md`) only suggest where to look; a value is written down as measured
only from the game itself ([`CONTEXT.md`](../CONTEXT.md), glossary).

**Tools.** Steam / `steamcmd` for the owner's install; throwaway Rust probes
and Python scripts outside the tree (an artifact under `context/` keeps them);
Capstone, `pefile`, Ghidra; Parallels for the Windows-only games.
