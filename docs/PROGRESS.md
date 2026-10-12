# Progress

How far each game is from its goal, as a list of milestones. The table at the
top of [`README.md`](../README.md) is generated from this file: each game's
percentage is its ticked milestones over all of them, every milestone weighing
the same.

Rules:

* A milestone is ticked (`- [x]`) only when a run shows it working on the
  game's own rules and data, as [`ARCHITECTURE.md`](ARCHITECTURE.md) requires;
  a rule lent from another game or written by IW4L in its place does not count.
* The pull request that reaches a milestone ticks it, and one that breaks it
  unticks it, in the same pull request. Then `cargo xtask progress --write`
  regenerates the README table; CI (`cargo xtask progress`) fails when the
  table and this file disagree.
* Milestones are not reworded to fit the work done. A new milestone may be
  added when a goal turns out to need it; that lowers the percentage, which
  is the point.

The zombies games share one list, so their numbers compare.

Each game also lists, per map, how many of the builtins its scripts name the
runtime does not implement yet: the `gsc: builtin coverage … N of M builtins
not implemented yet` line a match logs when its scripts install. The README's
Builtins column shows the implemented share (M − N of M). A pull request that
changes a count updates its row.

## Modern Warfare 2 — multiplayer

| Map | Builtins not implemented yet |
|---|---|
| Rust (`mp_rust`, free-for-all) | 0 of 460 |

- [x] Multiplayer maps load, render and collide.
- [x] Players move by Modern Warfare 2's own movement rules.
- [x] Weapons fire, reload, aim and take attachments by Modern Warfare 2's rules.
- [x] Free-for-all and Team Deathmatch run on Modern Warfare 2's own gametype scripts.
- [ ] Every other gametype (Search and Destroy, Domination, Sabotage, Headquarters, Capture the Flag, Demolition, …) is verified in a match.
- [x] Create-a-class, perks and progression.
- [x] Killstreaks.
- [x] Bots join and fight.
- [x] Online play through a master server.
- [x] Demos, clips and the killcam.
- [x] Modern Warfare 2's HUD, menus and sound.
- [ ] Modern Warfare 2's fidelity ledger ([`fidelity/iw4.md`](fidelity/iw4.md)) has no open item.

## Black Ops — Zombies

Indented items are detail: they say what a milestone's runs have shown and
what is still open (the open rules are in [`fidelity/t5.md`](fidelity/t5.md)).
Only the top-level items count.

| Map | Builtins not implemented yet |
|---|---|
| Kino der Toten (`t5:zombie_theater`) | 192 of 610 |
| Five (`t5:zombie_pentagon`) | 194 of 609 |

- [x] The install is recognised; the map's zones and scripts are read.
- [x] The map loads, renders and collides (Kino der Toten).
  - [ ] Zone data still dropped: dynamic entities, fx models, glass, dynamic lights; some static models whose model lives in another zone.
  - [ ] Vision sets (Kino's grading, last stand, death) are never loaded.
- [x] The game's own zombies scripts start the match and run round 1.
  - [ ] Client scripts (CSC) do not run; about 170 builtins still answer "not implemented yet".
- [x] The player moves by the game's own movement rules.
  - [x] Walking, sprinting, jumping, stances, the firing flag (`movement_t5`).
  - [ ] Ladders, mantling, swimming, mounted weapons, launches, falling damage and the other movement events.
- [x] Weapons fire, reload, throw and melee by the game's own rules.
  - [x] Bullets (spread, pellets, penetration), melee, frag grenades, rockets (Ray Gun), script blasts.
  - [x] The viewmodel plays the weapon's own animations, picked by the game's client rules.
  - [ ] Grenade launchers, sticky and special grenades, detonation, the Thundergun; dual wield (CZ75).
  - [ ] No muzzle flash, shell or tracer; viewmodel blend times and rates are the runtime's.
- [x] Zombies spawn and animate (rising, or at a window).
  - [x] Window spawns: boards torn off, mantle in.
  - [ ] Rising from the ground unverified; crawlers, ragdoll and thrown gibs missing.
- [x] Zombies path to the player and get through barriers.
  - [ ] No actor collision with the world while walking; traversal coverage unverified.
- [x] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
- [x] Rounds advance past round 1.
- [x] Wall weapons and the Mystery Box.
  - [ ] The box moving (teddy bear) and Fire Sale unverified.
- [ ] Doors and debris.
  - [x] Kino's doors open for points.
  - [ ] Debris (Five) not reached.
- [x] Power and perks.
  - [x] The power switch, buying and drinking a perk (Quick Revive), the perk icon.
  - [ ] Each perk's effect verified (Juggernog's health, Speed Cola, Double Tap, Stamin-Up); the drink's stance and melee locks.
- [ ] Pack-a-Punch and every power-up.
  - [x] Power-ups drop at the score thresholds; Insta-Kill picked up and working.
  - [ ] Pack-a-Punch, Max Ammo, Nuke, Double Points, Carpenter, Fire Sale unverified.
- [ ] Going down, solo revive, bleeding out and game over return to the lobby.
  - [x] A lethal hit downs the player; a solo game over fails the level and Kino loads again, as Black Ops does.
  - [ ] Solo Quick Revive self-revive unverified; down and revive animations; the client's fade-out, blur and dead quote.
- [ ] The game's own HUD.
  - [x] Black Ops' HUD menus (weapon info, dpad) with its fonts and layout; script hud elems (round chalk, point popups, game over text); text use hints.
  - [ ] The points column, hint icons, code-drawn pieces (reticle, blood, damage feedback), the scoreboard, menu paint order.
- [ ] The game's own front end: menus, lobby and map selection.
  - [ ] Main menu and lobby are Modern Warfare 2's; Kino starts from the game library.
- [ ] The game's own sound.
  - [x] Weapon fire, reload, raise and putaway sounds, notetrack sounds, zombie vocals and script sounds, all Black Ops aliases.
  - [ ] Footsteps and gear, music states, `stopsound` and `setsoundblend`, another player's automatic fire; a `PlaySound` notify waits a fixed 2 s.
- [ ] The first map's own systems (Kino's teleporter, its wonder weapon and the rest).
  - [ ] Teleporter and projector, the Thundergun, monkey bombs, hellhound rounds, traps, the auto turret.
- [ ] Every zombies map in the install (Five, Dead Ops Arcade).
- [ ] Co-op: friends join, revive one another and play to game over.
  - [x] A lobby host picks Kino; a joined player spawns at its co-op spawns, gets points and sees the zombies.
  - [ ] Revive, bleeding out, spectating; no two-machine test.
- [ ] The game's fidelity ledger ([`fidelity/t5.md`](fidelity/t5.md)) has no open item.

## Black Ops II — Zombies

| Map | Builtins not implemented yet |
|---|---|
| Nuketown (`t6:zm_nuked`) | — (its scripts are refused at install: `t6.scripts.ffotd`) |

- [x] The install is recognised; the map's zones and scripts are read.
- [x] The map loads, renders and collides (Nuketown).
- [x] The game's own zombies scripts start the match and run round 1 (with `IW4L_T6_OWN_SCRIPTS=1`).
- [ ] The player moves by the game's own movement rules.
- [ ] Weapons fire, reload, throw and melee by the game's own rules.
- [x] Zombies spawn and animate (rising, or at a window).
- [ ] Zombies path to the player and get through barriers.
- [ ] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
- [x] Rounds advance past round 1.
- [ ] Wall weapons and the Mystery Box.
- [ ] Doors and debris.
- [ ] Power and perks.
- [ ] Pack-a-Punch and every power-up.
- [ ] Going down, solo revive, bleeding out and game over return to the lobby.
- [ ] The game's own HUD.
- [ ] The game's own front end: menus, lobby and map selection.
- [ ] The game's own sound.
- [ ] The first map's own systems (Nuketown's perk drops and its end-game rocket).
- [ ] Every zombies map in the install (TranZit, Buried).
- [ ] Co-op: friends join, revive one another and play to game over.
- [ ] The game's fidelity ledger ([`fidelity/t6.md`](fidelity/t6.md)) has no open item.

## Black Ops III — Zombies

| Map | Builtins not implemented yet |
|---|---|
| Shadows of Evil | — (the map does not load yet) |

- [x] The install is recognised; the map's zones and scripts are read.
- [ ] The map loads, renders and collides (Shadows of Evil).
- [ ] The game's own zombies scripts start the match and run round 1.
- [ ] The player moves by the game's own movement rules.
- [ ] Weapons fire, reload, throw and melee by the game's own rules.
- [ ] Zombies spawn and animate (rising, or at a window).
- [ ] Zombies path to the player and get through barriers.
- [ ] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
- [ ] Rounds advance past round 1.
- [ ] Wall weapons and the Mystery Box.
- [ ] Doors and debris.
- [ ] Power and perks.
- [ ] Pack-a-Punch and every power-up.
- [ ] Going down, solo revive, bleeding out and game over return to the lobby.
- [ ] The game's own HUD.
- [ ] The game's own front end: menus, lobby and map selection.
- [ ] The game's own sound.
- [ ] The first map's own systems (Shadows of Evil's rituals, beast mode and the rest).
- [ ] Every zombies map in the install.
- [ ] Co-op: friends join, revive one another and play to game over.
- [ ] The game's fidelity ledger ([`fidelity/t7.md`](fidelity/t7.md)) has no open item.
