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
* A milestone may carry indented detail items (`  - [ ]`): what its runs
  have shown and what is still open. They do not count toward the
  percentage; the README shows them as steps, so smaller progress shows too.
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
  - [x] Static models (3268 of 3326 slots) and script brush models (window boards, debris) draw.
  - [ ] Zone data still dropped: dynamic entities, fx models, glass, dynamic lights; some static models whose model lives in another zone.
  - [ ] Vision sets (Kino's grading, last stand, death) are never loaded.
- [x] The game's own zombies scripts start the match and run round 1.
  - [ ] Client scripts (CSC) do not run; 192 of 610 builtins still answer "not implemented yet" on Kino (see the table above).
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
  - [x] Juggernog fills the player's health to 250 (`SetMaxHealth`).
  - [ ] Each perk's effect verified (Speed Cola, Double Tap, Stamin-Up); the drink's stance and melee locks.
- [x] Pack-a-Punch and every power-up.
  - [x] Power-ups drop at the score thresholds; each of Kino's six (Max Ammo, Nuke, Double Points, Insta-Kill, Carpenter, Fire Sale) is picked up and runs, with its HUD icon and announcer line.
  - [x] Pack-a-Punch: paid, knuckle crack, the upgraded weapon taken (AK-74u to AK74fu2).
  - [ ] Carpenter's boards skip `dontinterpolate`; the upgraded weapon's camo and the Pack-a-Punch weapon options are unverified.
- [ ] Going down, solo revive, bleeding out and game over return to the lobby.
  - [x] A lethal hit downs the player; with Quick Revive (`IW4L_TEST_PLAYER=revive`) the solo player holds `m1911_upgraded_zm` and revives after 10 s; a solo game over fails the level and Kino loads again, as Black Ops does.
  - [x] Going down and `reviveplayer` raise Black Ops' stance animation events, clear damage counters and switch `startrevive`/`stoprevive`'s `LAST_STAND_REVIVED` move type (`t5.match.last_stand` closed).
  - [ ] The client's fade-out, blur and dead quote (`t5.match.level_failed`).
- [ ] The game's own HUD.
  - [x] Black Ops' HUD menus (weapon info, dpad, points column `competitivemodescores`) with its fonts, layout, safe area and name-hash paint order; script hud elems (round chalk, point popups, game over text); text and icon use hints (`HINT_NOICON`, `HINT_SEAT`, `HINT_ACTIVATE`, `HINT_HEALTH`, `HINT_FRIENDLY`, `HINT_SPECTATOR`, `HINT_TEAMPOT`, weapons).
  - [ ] Code-drawn pieces (reticle, blood, damage feedback), the scoreboard.
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

Indented items are detail: they say what a milestone's runs have shown and
what is still open (the open rules are in [`fidelity/t6.md`](fidelity/t6.md)).
Only the top-level items count. Runs are headless, with the test player
(`IW4L_TEST_PLAYER=1|rounds|use|down|revive`; `IW4L_TEST_USE` and `IW4L_TEST_PERK` pick what the use test buys, and the console's `script` command calls Black Ops 2's `_zm_score::add_to_player_score` to give it points) and `IW4L_T6_OWN_SCRIPTS=1`.

| Map | Builtins not implemented yet |
|---|---|
| Nuketown (`zm_nuked`, Classic) | 146 of 692 |
| TranZit (`zm_transit`, Classic) | 156 of 741 |
| Buried (`zm_buried`, Classic) | 159 of 727 |

- [x] The install is recognised; the map's zones and scripts are read.
  - [x] Nuketown, TranZit and Buried (the zombies maps in the install), with the FFOTD `_globallogic` captured from the owner's running game.
- [x] The map loads, renders and collides (Nuketown).
  - [x] TranZit and Buried load and collide too, with their Classic zombie spawners (`AddonMapEnts`).
  - [ ] The client scripts (CSC) do not run: no client fields, exploders, fx animations or the wall-buy chalk.
- [x] The game's own zombies scripts start the match and run round 1 (with `IW4L_T6_OWN_SCRIPTS=1`).
  - [ ] Builtins still answering "not implemented yet" on every map (see the table above).
- [ ] The player moves by the game's own movement rules.
  - [ ] Movement is Modern Warfare 2's, lent by the owner (`t6.movement.player`); Black Ops 2's own (`movement_t6`) is not started.
  - [ ] Script allowances (stances) are kept but not applied.
- [ ] Weapons fire, reload, throw and melee by the game's own rules.
  - [x] Black Ops 2's weapon facts (damage, ranges, clip and stock, hit-location scales) are read from its zones.
  - [ ] The weapon state machine (fire, reload, switch, melee, grenades) is Modern Warfare 2's (`t6.weapons.state_machine`).
  - [x] `knife_zm` is given (Black Ops 2's melee type is kept; the lent weapons hold it as melee only).
  - [x] An external view model animation no loaded zone carries takes the startup zone's default `void` animation, as Black Ops 2's linker does, so `m1911_upgraded_zm` (the last stand pistol after Pack-a-Punch) is given (`calcweaponoptions`).
  - [x] Thrown and scripted grenades and missiles run Black Ops 2's missile physics, bounces, rest (`stationary`) and explosion surface (`game_t6::missile`, `game_t6::grenade`, `detonate`, `magicgrenadetype`, `makegrenadedud`, `resetmissiledetonationtime`, `forcegrenadethrow`, `setmissileowner`): claymores planted on TranZit (`claymore_zm`) arm on `stationary`, wake on an approaching zombie (`setexcludeteamfortrigger`), pass `shouldaffectweaponobject` and `damageconetrace` and detonate to kill it; Monkey Bombs thrown on Buried (`cymbal_monkey_zm`) generate their attractor ring (`attractor_positions_generated`), draw zombies in and kill them on explosion.
- [x] Zombies spawn and animate (rising, or at a window).
  - [x] Zombies rise on Nuketown and spawn on TranZit and Buried, animated by Black Ops 2's animation state machine (ASD).
  - [ ] Pitch orientation (`setpitchorient`, `allowpitchangle`) is kept, not applied (`t6.actors.pitch`); gibs and ragdoll are missing.
- [x] Zombies path to the player and get through barriers.
  - [x] On Nuketown zombies path to the player and reach melee range.
  - [x] Doors, debris and the map's other dynamic-path models (spawnflag 1) cut the zombies' paths while closed, as Black Ops 2 does (TranZit: 74 entities, 523 links; the depot's glass doors no longer draw zombies that cannot pass).
  - [x] TranZit reaches round 2: zombies tear the depot's window boards and come in; its two inert zombies outside wake when the player comes near, as Black Ops 2's `start_inert` does (the test player goes to the nearest zombie after 20 s with none in sight).
  - [x] Boards the zombies tore off are rebuilt by holding use at the window (TranZit: four boards, `rebuild_barrier_reward` 40, 10 points each).
- [x] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
  - [x] Shots and the knife kill zombies through Black Ops 2's `G_Damage` and death callbacks; kills and hits earn its points (Nuketown: round 3, 16 kills in 300 s).
  - [x] Zombies hurt the player through Black Ops 2's actor melee and player damage callback.
  - [ ] The weapons that fire are Modern Warfare 2's (see the weapons milestone); the knife stabs with `knife_zm` and the held gun's melee damage, its trace by Black Ops 2's melee mask; the lethal grenade `giveweapon` gives is the player's offhand and is thrown (`grenade_fire`, `explode`).
  - [x] A zombie's swing that lands on another zombie does nothing, as Black Ops 2's melee hit does.
- [x] Rounds advance past round 1.
  - [x] Nuketown reaches round 3 in the test player's 300 s runs, with round-end stats (`set_global_stat`, `update_playing_utc_time`) reading and writing `statstable_zm.ddl` without faults.
- [x] Wall weapons and the Mystery Box.
  - [x] The box takes its 950 and opens (Buried).
  - [x] Wall weapons sell through Black Ops 2's unitriggers on Nuketown, TranZit and Buried (the Olympia: 500 points, the gun changes).
  - [ ] Wall weapons filed under a zone not yet active wait for it, as in the game; their ammo buys are not shown.
  - [x] The box's weapon rises and is taken (Buried: 950 points, the Time Bomb, through Black Ops 2's box scripts and `useweaponmodel`).
  - [x] The box moves: in ten opens on Buried it gave the Time Bomb, M32, SRM1216, Ray Gun, DSR-50; on the seventh the teddy bear refunded the price (`moving_chest_now`, `box_moving`) and the box reopened in `zone_underground_courthouse2` (Saiga-12).
  - [ ] Fire Sale's boxes (every location, 10 points) not shown; the Fire Sale power-up itself sets `zombie_powerup_fire_sale_on`.
- [x] Doors and debris.
  - [x] Doors open for points on Nuketown, TranZit and Buried, debris clears on Nuketown and Buried, by the game's scripts through Black Ops 2's use rule (Buried: three doors and two debris piles in one run).
  - [x] Closed doors and debris cut the zombies' paths until they open (Black Ops 2's dynamic paths).
- [x] Power and perks.
  - [x] Quick Revive sells on TranZit (solo: 500 points, the bottle is drunk) through Black Ops 2's use rule.
  - [x] Buried's power switch turns the power on (`power_on`, the perk machines' power notifies) and Double Tap sells after it (2000 points; the perk is given).
  - [ ] TranZit's power switch is built at the power station from its three parts (test player: `IW4L_TEST_GOTO=zone_pow IW4L_TEST_BUILD=powerswitch IW4L_TEST_USE=door,piece,build,…`); throwing its lever after the build is not yet run. Nuketown's perks arrive by vehicle drop (below).
- [x] Pack-a-Punch and every power-up.
  - [x] Pack-a-Punch upgrades a gun on Buried after the power (5000 points; the Olympia becomes `rottweil72_upgraded_zm` and is taken).
  - [x] Each power-up a map includes, dropped by Black Ops 2's `specific_powerup_drop` at the player, is grabbed and works: double points (`zombie_point_scalar` 2), insta-kill, max ammo (stock refilled), nuke (+400), fire sale (Nuketown); carpenter (`carpenter_finished`, +200), free perk, random weapon (Buried).
  - [ ] TranZit's own (`insta_kill_ug`, `minigun`) and the bank's `teller_withdrawl` not shown; Nuketown's Pack-a-Punch arrives by perk drop.
- [x] Going down, solo revive, bleeding out and game over return to the lobby.
  - [x] Hits go through Black Ops 2's `finishplayerdamage` (damage timer, health, the `damage` notify); a lethal hit puts a player down and runs the last stand callback.
  - [x] Solo without Quick Revive: going down runs Black Ops 2's solo death (Nuketown's fall, `end_game`) and the match returns to the lobby on Nuketown, TranZit and Buried without faults (the fall linked to an anchor, `death_throe_zm` held, the rumbles stopped).
  - [x] With Quick Revive (TranZit, `IW4L_TEST_PLAYER=revive`): down at 51 s, Black Ops 2's solo revive stands him up at 61 s; the next down ends the game and the match returns to the lobby.
  - [ ] Bleeding out is co-op only (see Co-op); the last stand vision and blur are not drawn (`t6.vision.last_stand`, `t6.player.blur`).
- [ ] The game's own HUD.
  - [ ] Black Ops 2's LUI menus and HUD are DenisToxic's `game_t6::ui` work.
- [ ] The game's own front end: menus, lobby and map selection.
  - [ ] Nuketown, TranZit and Buried start from the game library; the menus are not Black Ops 2's.
- [ ] The game's own sound.
  - [ ] Not verified (headless runs have sound off).
- [x] The first map's own systems (Nuketown's perk drops and its end-game rocket).
  - [x] Quick Revive drops from the sky on its path vehicle (`perk_arrival_vehicle` on `info_vehicle_node`s to `reached_end_node`), lands and sells (500, drunk); the later drops follow the same path at rounds 3–5 on.
  - [x] At game over the rocket intermission plays to `rocket_hit_nuketown` and the match returns to the lobby (the camera calls are kept for the client to present).
- [ ] Every zombies map in the install (TranZit, Buried).
  - [x] TranZit and Buried run round 1 with zombies spawning; doors open on TranZit, the box on Buried.
  - [x] TranZit's bus drives its route on its path nodes once the depot doors open, with the player riding it (depot, diner, forest, on toward the power station in 220 s), its scripts reading tags off its posed model and who stands on it.
  - [x] TranZit's power: the powerswitch built from its three parts, its lever thrown, `power_on` set by the map's 30 s power event, and a perk sold after it.
  - [x] TranZit's riot shield built from its two parts, taken from its table and held (its shield model attached).
  - [x] TranZit's turbine and turret carried and set down by Black Ops 2's placement test (`IW4L_TEST_WIELD=equip_turbine_zm,equip_turret_zm IW4L_TEST_PLACE=1`); the placed turret is spawned, set to `auto_nonai` and powered by the turbine beside it.
  - [x] The placed turret picks zombies, swings onto them, fires Black Ops 2's bullets (penetrating by the zone's depth table) and kills them, itself the attacker.
  - [ ] The jet gun is not started.
  - [x] Buried's scripts run without faults through 300 s of opening the town (power, doors, debris): Leroy spawns in his cell, the buildable pieces spawn, the maze and the fountain move.
  - [ ] Buried's own systems are not played through (freeing Leroy, building, the maze, the ghosts); the bus's collision model pieces (`setzbarriercolmodel`) are kept, not collided against.
- [ ] Co-op: friends join, revive one another and play to game over.
  - [x] With a bot as the second player on Nuketown: he joins and spawns, goes down, bleeds out after 45 s and respawns at the next round; the test player revives him (`player_revived`, the reviver named) each time he goes down; with both down the game ends and the match returns to the lobby.
  - [ ] A second player over the network (two machines) not tried.
- [ ] The game's fidelity ledger ([`fidelity/t6.md`](fidelity/t6.md)) has no open item.

## Black Ops III — Zombies

| Map | Builtins not implemented yet |
|---|---|
| Shadows of Evil | — (the map does not load yet) |

| Map | Builtins not implemented yet |
|---|---|
| Shadows of Evil (`zm_zod`, Classic) | 1166 of 1244 |

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
