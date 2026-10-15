# Modern Warfare 2 — fidelity ledger

Rule: Modern Warfare 2 matches run on Modern Warfare 2's own rules and data.
No other game's behaviour stands in, no guessed constant, no stub that
pretends to work. A rule that cannot be recovered is an `unknown!` in
`game_iw4` (or one of the IW4 rule crates) and is listed here with its id;
`cargo xtask boundary` refuses an `unknown!` id that is missing here.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.

## Open

- [ ] **G** `iw4.weapons.notifies`: Modern Warfare 2 has no weapon-notify rule
      of its own, so its matches get the shared path's notifies
      (`notify_weapon_changes` in `sim/src/script/host/weapons.rs`), including
      `weapon_change_complete`, a Black Ops notify.
- [ ] **G** `iw4.scripts.t6_equipment`: every Modern Warfare 2 match starts
      `iw4l_t6/equipment`, a Black Ops 2 equipment script written by IW4L
      (`assets/src/t6_equipment.gsc.txt`, always inserted by the match walk and
      started by `Iw4Startup`), and sets `t6lethal`/`t6tactical` on players;
      foreign offhands map to Modern Warfare 2 stand-ins (`willy_pete` →
      `smoke_grenade_mp`, … in `sim/src/script/host/players.rs`).
      Cross-family composition, an IW4L feature
      ([FAMILIES.md](../FAMILIES.md)), not Modern Warfare 2.

## Closed

- [x] **G** Builtins Modern Warfare 2's scripts bind that only Black Ops'
      native files implemented. The shared ones (`clamp`, `log`,
      `sighttracepassed`, `getassignedteam`, the weapon queries) are served
      from `natives/shared.rs`; Black Ops' no-op, recorded and refused tables
      (`enablelinkto`, `resettimeout`, `setarchive`, `announcement`,
      `addtestclient`, `stopallrumbles`, `vibrate`, …) are not bound for
      Modern Warfare 2 any more: the registry is per game
      (`session/src/games.rs::natives`) and Modern Warfare 2's compiled
      scripts call none of them (no builtin gap at match start).
