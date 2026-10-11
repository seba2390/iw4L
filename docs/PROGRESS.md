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

## Modern Warfare 2 — multiplayer

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

- [x] The install is recognised; the map's zones and scripts are read.
- [x] The map loads, renders and collides (Kino der Toten).
- [x] The game's own zombies scripts start the match and run round 1.
- [x] The player moves by the game's own movement rules.
- [x] Weapons fire, reload, throw and melee by the game's own rules.
- [x] Zombies spawn and animate (rising, or at a window).
- [x] Zombies path to the player and get through barriers.
- [x] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
- [x] Rounds advance past round 1.
- [x] Wall weapons and the Mystery Box.
- [ ] Doors and debris.
- [x] Power and perks.
- [ ] Pack-a-Punch and every power-up.
- [ ] Going down, solo revive, bleeding out and game over return to the lobby.
- [ ] The game's own HUD.
- [ ] The game's own front end: menus, lobby and map selection.
- [x] The game's own sound.
- [ ] The first map's own systems (Kino's teleporter, its wonder weapon and the rest).
- [ ] Every zombies map in the install (Five, Dead Ops Arcade).

## Black Ops II — Zombies

- [x] The install is recognised; the map's zones and scripts are read.
- [x] The map loads, renders and collides (Nuketown).
- [x] The game's own zombies scripts start the match and run round 1 (with `IW4L_T6_OWN_SCRIPTS=1`).
- [ ] The player moves by the game's own movement rules.
- [ ] Weapons fire, reload, throw and melee by the game's own rules.
- [x] Zombies spawn and animate (rising, or at a window).
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
- [ ] The first map's own systems (Nuketown's perk drops and its end-game rocket).
- [ ] Every zombies map in the install (TranZit, Buried).

## Black Ops III — Zombies

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
