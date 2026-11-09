# Ghidra on the Black Ops executables

How to get **readable** decompiler output from Black Ops II (and, partly, Black
Ops), on either of the maintainer's Macs. Read [`DECODING.md`](DECODING.md)
first for what may be studied and why nothing from it lands in this repository.

Everything below was measured on 2026-11-06 against the owner's own installs.
"Measured" means: the same 931 functions decompiled before and after each step,
counting unnamed functions, unnamed globals, raw `+ 0x…` offsets and register
artifacts (`in_`/`unaff_`/`extraout_` variables).

## The rules

* Only the owner's installs (`~/Games/blackops`, `~/Games/blackops2`) and the
  memory image already on the machine. No leaked builds, PDBs or map files, no
  cloud decompilers. KisakCOD is therefore not used as a source: it was
  reconstructed with debug PDBs.
* Everything Ghidra produces stays in `~/iw4l-re/` on the Mac: unpacked
  executables, projects, decompiled C, name lists. None of it is committed, and
  no address or decompiler name (`FUN_…`, `DAT_…`) goes into the tree
  (`make publish-check` refuses both).
* The kit has its own projects. Do not point it at, or modify, the older shared
  ones (`~/iw4l-re/t6vm/ghidra`, `~/iw4l-re/ghidra-full`, `~/iw4l-re/t5`) or the
  `t6vm/tools/dec.sh` cache other agents read.

## What you work with: `~/iw4l-re/kit`

A small folder **outside** the repository (about 1 MB without its build output):

```
~/iw4l-re/kit/
  setup.sh        installs / checks Ghidra, Java, OpenSSL, the Python venv, the game files
  build_t6.sh     builds the readable Black Ops II project (proj/t6)
  dec.sh          decompiles addresses from it into out/<program>/<addr>.c
  gh.sh           runs Ghidra headless: 10 GB heap, 4 cores, one job at a time
  data/           what the build applies; the knowledge you add goes here
    names_everything.tsv   every known name: <addr> <name> <kind>
    structs.tsv            struct layouts (struct, size, offset, field, type, bytes)
    type_rules.tsv         which function returns / global holds which struct
    callconv_list.txt      functions whose register arguments and float returns get fixed
  inputs/         sources of data/: builtin renames, dvar defaults, script constants
  scripts/        the Python helpers; scripts/ghidra/ the Ghidra scripts
  proj/ copies/ out/ logs/ venv/   build output, made per Mac, never copied
```

## Set up a Mac

**On a Mac that already has the kit:** `cd ~/iw4l-re/kit && ./setup.sh`. It
says `ready`, or marks what is missing with `!!`.

**On the other Mac** (the kit has to be copied over once, then kept in sync):

1. Install Homebrew, and the games the way [`BUILD.md`](BUILD.md) says (the
   Windows depot through `steamcmd`): Black Ops II into `~/Games/blackops2`,
   Black Ops into `~/Games/blackops` if you work on it. Elsewhere: set
   `IW4L_BO2_DIR` / `IW4L_BO1_DIR` before running the kit.
2. Copy the kit from the Mac that has it. Either over the network (on the
   source Mac: System Settings → General → Sharing → Remote Login on):
   ```sh
   mkdir -p ~/iw4l-re
   rsync -a --exclude '/proj' --exclude '/copies' --exclude '/out' --exclude '/logs' --exclude '/venv' \
     <source-mac>.local:iw4l-re/kit/ ~/iw4l-re/kit/
   ```
   or by hand: on the source Mac
   `cd ~/iw4l-re && zip -r kit.zip kit -x 'kit/proj/*' 'kit/copies/*' 'kit/out/*' 'kit/logs/*' 'kit/venv/*'`,
   AirDrop `kit.zip`, and unzip it into `~/iw4l-re/` on this one.
3. `cd ~/iw4l-re/kit && ./setup.sh` — installs `ghidra`, `openjdk@21` and
   `openssl@3` with Homebrew, creates `venv/` with `pefile` and `capstone`, and
   checks the game files and `data/`.
4. `./build_t6.sh` (below). The projects are rebuilt on each Mac, not copied:
   they are gigabytes, and a build takes about five minutes.

**Keeping the two Macs in sync.** Only `data/` (and `inputs/`) carry knowledge.
After adding names or structs on one Mac, copy `data/` to the other with the
same `rsync` (just `data/`) and run `./build_t6.sh fixes` there. Edit `data/` on
one Mac at a time; the copy overwrites.

## Build the readable Black Ops II project

```sh
cd ~/iw4l-re/kit
./build_t6.sh              # t6zm (Zombies): about 5 minutes
./build_t6.sh vt t6mp      # optional: Multiplayer, names carried over from t6zm
./build_t6.sh vt t6sp      # optional: campaign
```

A good build prints, for t6zm, about 4,300 functions created from data
pointers, about 3,300 functions named, 6 struct types with 2 getters and 2
globals typed, and about 380 calling-convention fixes applied (`logs/` has the
full output). `proj/t6/t6.gpr` then holds `t6zm.unpacked.exe` (and the others).
Decompile:

```sh
./dec.sh <addr> <addr> ...         # Zombies; writes out/t6zm/<addr>.c
./dec.sh -p t6mp -f list.txt       # Multiplayer, one address per line
```

Addresses are the same as in the memory image, the notes and the older
decompile cache: the unpacked code is byte-for-byte the code in the image.
`dec.sh` opens the project read-only, so decompiling never changes it; twenty
functions take a few seconds. To browse
instead, run `ghidraRun` and open `~/iw4l-re/kit/proj/t6/t6.gpr`, but close the
window before the next `dec.sh` or build: a project opens in one place at a time.

## What the build does, best payoff first

1. **Use the real game files instead of the memory dump.** `scripts/unsteamstub.py`
   removes Steam's copy protection from the owner's BO2 executables, offline
   with OpenSSL; no Windows or Steamless needed. The code inside is
   byte-for-byte the code in the memory image, so every address already in the
   notes still works. Ghidra understands the real file better: imports, sections
   and runtime library functions get names, and calls to non-functions drop 98%.
   The default analysis decodes a few real function starts wrongly;
   `DecompList.java` repairs them when you decompile one.
2. **Load all the names we already know.** The game carries lists of its own
   functions with their names: script builtins, console commands, settings
   (dvars), menu and UI functions, field callbacks. `scripts/find_name_tables.py`
   finds those tables (533 in BO2) and `scripts/tables_to_names.py` turns them
   into 1,771 function names on top of the existing builtin renames;
   `NameDvarGlobals.java` names 2,705 dvar globals. Together, the share of calls
   that show a real name goes from **3% to 25%**, and unnamed globals drop 23%.
3. **Copy names between the BO2 builds.** Version Tracking matches the Zombies
   exe against Multiplayer and the campaign. With functions found from data
   pointers first and looser matching (`SetLooseVTOptions.java`), about 2,400
   names reach Multiplayer automatically (`./build_t6.sh vt t6mp`), 99.7% of them
   correct when checked against Multiplayer's own builtin tables, naming 82% of
   its builtin handlers. Ghidra's default options alone reach only 13%. The
   campaign (`vt t6sp`) has not been measured with these options yet; with the
   default ones it got 427 names.
4. **Fix how values are passed between functions.** The compiler passes some
   arguments in registers and returns floats in XMM0; Ghidra misses both, so
   calls lose arguments and float functions look like they return nothing.
   `CallConv.java` fixes only what the calling code confirms: a register
   argument when callers set it, an XMM0 return when most callers read it (15 of
   15 checked by hand were right). Junk register variables roughly **halve**
   (−52%). It runs on the functions in `data/callconv_list.txt` and their callees,
   not the whole program; that was not tried.
5. **Add struct layouts.** The game also describes some of its own data layouts:
   weapons, AI actors, sentients, player and entity state.
   `scripts/make_structs.py` builds them into `data/structs.tsv`. A layout only
   helps where code is told a pointer has that type, so `data/type_rules.tsv`
   types the two weapon getters' return values: across their 605 callers that
   turns about 1,800 raw offsets into named fields (−21%).

## Adding what you learn

* **A name:** append `<addr>\t<name>\tfunc` to `data/names_everything.tsv`
  (`data`, `dword`, `float`, `ushort` for globals), then `./build_t6.sh fixes`.
  `fixes` is safe to rerun, but it only fills names Ghidra made up: a
  function that already has a name keeps it. To change a name that is already
  applied, rename it in the Ghidra window, or rebuild from scratch
  (`rm -rf proj && ./build_t6.sh`).
* **A struct type:** add the layout to `data/structs.tsv` and a line to
  `data/type_rules.tsv`: `ret <addr> <Type>*` (a function returns it),
  `param <addr> <index> <Type>*`, or `global <addr> <Type> <count>`. Prefer a
  getter or global that hands the struct out; it types every caller at once.
* **Register arguments for an area you work on:** add its function addresses to
  `data/callconv_list.txt`, then `./build_t6.sh fixes`.
* Then copy `data/` to the other Mac.

## What not to do

* **Don't turn on Ghidra's extra analyses** (Decompiler Parameter ID, Aggressive
  Instruction Finder). Measured: register artifacts +32%, `extraout_` variables
  6 → 305, twice the stray code, 17 minutes per binary.
* **Don't guess a struct type from the offsets a function reads.** The weapon
  struct has 861 fields in 4 KB and matches almost anything; a path struct got
  typed as a weapon.
* **Don't run two Ghidra jobs at once, or more than 4 cores.** A build needs a
  10 GB heap; two jobs on an 18 GB Mac run out of memory. `gh.sh` queues jobs and
  caps the cores (BSim and Parameter ID otherwise start about ten decompilers).
  Raise `CPUS` only when nobody else is using the Mac.
* **Don't copy `proj/` between Macs.** Rebuild it.

## Black Ops (BO1)

`BlackOps.exe` is not protected, but most of its strings are missing from the
file on disk (filled in while the game runs), so the name tables and dvar names
that drive step 2 are not there. Default analysis names only runtime library
functions (1,303). BO1 naming needs a memory dump from the running game first
(the Windows VM, as for BO2's image). Until then, BSim can match BO2 functions to
BO1: 171 of 825 sampled BO2 functions found a match at similarity ≥ 0.7 (126 also
pass an instruction-sequence check), which tells where known BO2 struct fields
sit in BO1. `scripts/ghidra/BSimPair.java` does it; check each pair below 0.8.

## The full report

`~/iw4l-re/investigation/REPORT.md` on the Mac that ran the measurements: every
question, the before/after tables and diffs, and the scripts used for measuring
(`count_patterns.py`, `compare.py`, `callee_names.py` are also in the kit).
