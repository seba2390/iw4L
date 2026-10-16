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

Each game section ends with its technical debt: work that is not a milestone
but has to be paid off at some point. Those lists are plain bullets and do
not count toward the percentage. Figures are from 2026-10-14
(`xtask/boundary/allow.txt` occurrences, `cargo check` and
`cargo clippy --workspace --all-targets` warnings); refresh them when a pull
request pays some of it off.

## Modern Warfare 2 — multiplayer

| Map | Builtins not implemented yet |
|---|---|
| Rust (`mp_rust`, free-for-all) | 0 of 460 |

- [x] Multiplayer maps load, render and collide.
- [x] Players move by Modern Warfare 2's own movement rules.
  - [ ] Step (18 standing, 10 prone), downward snap (9), landing (z ≥ 0.3) and solid-recovery limits come from Modern Warfare 2; today they are IW4L settings ([`provenance/movement-iw4.md`](provenance/movement-iw4.md), [`provenance/solid-recovery.md`](provenance/solid-recovery.md)).
- [x] Weapons fire, reload, aim and take attachments by Modern Warfare 2's rules.
  - [ ] Recoil and look sway follow Modern Warfare 2's response; today it is IW4L policy ([`provenance/recoil-and-sway.md`](provenance/recoil-and-sway.md)).
- [x] Free-for-all and Team Deathmatch run on Modern Warfare 2's own gametype scripts.
  - [ ] Score and time limits come from the gametype scripts; today they come from `IW4L_SCORE_LIMIT` or IW4L constants, and the HUD shows that value.
- [ ] Every other gametype (Search and Destroy, Domination, Sabotage, Headquarters, Capture the Flag, Demolition, …) is verified in a match.
  - [ ] The lobby offers every gametype the installation ships; today it cycles `dm war dom sd koth sab` (no `ctf`, no `dd`).
  - [ ] Domination (`dom`).
  - [ ] Search and Destroy (`sd`).
  - [ ] Sabotage (`sab`).
  - [ ] Headquarters (`koth`).
  - [ ] Capture the Flag (`ctf`).
  - [ ] Demolition (`dd`).
  - [ ] The other gametypes the installation ships (refused today as `gsc.mode.out-of-scope`).
- [x] Create-a-class, perks and progression.
- [x] Killstreaks.
  - [ ] The sentry's turn, lock, sweep and placement rules come from Modern Warfare 2; today they are IW4L values ([`KILLSTREAKS.md`](KILLSTREAKS.md), [`provenance/sentry-placement.md`](provenance/sentry-placement.md)).
- [x] Bots join and fight.
  - [ ] Bots play objectives: capture, plant, defuse and carry flags.
- [x] Online play through a master server.
- [x] Demos, clips and the killcam.
- [x] Modern Warfare 2's HUD, menus and sound.
- [ ] Modern Warfare 2's fidelity ledger ([`fidelity/iw4.md`](fidelity/iw4.md)) has no open item.

### Technical debt

Boundary figures measured on 2026-10-15 from `xtask/boundary/allow.txt`: the
sum of the counts on its `pattern <file> <crate>_iw4:: <n>` lines, grouped by
the crate or file in the path. About 2,700 (2,711) Modern Warfare 2 crate paths
remain in shared crates. The runtime grew out of Modern Warfare 2, and most
shared systems still call its crates directly. One bullet per piece of work:

- `sim` script host (`sim/src/script/host/`): 208.
- `sim` combat: 199 (`combat.rs` 83, `bullet_collision.rs` 39, `damage.rs` 21,
  `item.rs` 14, `penetration.rs`, `equipment.rs` and `corpse.rs` 11 each,
  `weapon_content.rs` 9).
- `sim` world and step: 289 (`world.rs` 93, `step.rs` 90, `script_player.rs`
  61, `snapshot.rs` 13, `content.rs` 12, `frame.rs` 10, `world/content.rs` 10).
  The rest of `sim` holds 88 more (784 in all).
- Render lighting: 284 `lighting_iw4::` paths in `render_*` crates
  (`render_frontend` 186, `render_gpu` 52, `render_backend` 24).
- Render visibility and marks: 140 `dpvs_iw4::` (`render_frontend` 76,
  `render_scene` 30, `render_anim` 20) and 81 `marks_iw4::` (`render_frontend`).
  The `render_*` crates hold 321 other paths (`asset_iw4`, `hud_iw4`, `fx_iw4`, …).
- `hud` 246 (`hud_iw4` 189) and `net` 164 (`playerstate_iw4` 61, `entity_iw4` 47).
- Assets: 563 in `asset_game` (176), `asset_world` (139), `asset_material` (96),
  `assets` (82), `asset_audio` (49) and `asset_anim` (21).
- Clippy and dead code: about 47 warnings in Modern Warfare 2 crates and files
  (`weapon_iw4`, `clipmap_iw4`, `movement_iw4`, `input_iw4`,
  `assets/src/lane/iw4.rs`) and 29 in `fx_iw4` (2026-10-14, not re-run). The
  script-gap ledger (`gamemode_iw4/src/gaps.rs`, `sim/src/script_gaps.rs`) is
  never raised, yet `sim::step` reports it every step.
- `gamemode_iw4` is a `mixed` crate: its `GameModeKind` also carries the zombies
  modes `Zombies` and `Zclassic`.
- Modern Warfare 3 (IW5) maps are recognised but have no section here; their
  569 `fastfile_iw5::` paths (allow.txt; mostly `asset_game`, `assets`,
  `asset_world`) and 65 `AssetNamespace::Iw5` uses (grep of shared crates) are
  debt of the same kind.

## Black Ops — Zombies

Indented items are detail: they say what a milestone's runs have shown and
what is still open (the open rules are in [`fidelity/t5.md`](fidelity/t5.md)).
Only the top-level items count.

| Map | Builtins not implemented yet |
|---|---|
| Kino der Toten (`t5:zombie_theater`) | 151 of 610 |
| Five (`t5:zombie_pentagon`) | 153 of 609 |

- [x] The install is recognised; the map's zones and scripts are read.
- [x] The map loads, renders and collides (Kino der Toten).
  - [x] Static models (all 3326 slots, 58 of them models other zones hold: teddy bears, lobby door tear-ins, perk bottles) and script brush models (window boards, debris) draw and collide.
  - [ ] Zone data still dropped: dynamic entities (398 client and 142 server on Kino).
  - [x] Vision sets and script fog: Kino's own vision draws through Black Ops' post chain (bloom, streaks, S-curve, film), picked and blended by its vision channels: `zombie_theater` from the server at start, `zombie_last_stand` while down and `zombie_death` while bleeding out, back on revive; the client scripts' calls reach the same channels (not yet seen in a run); server and client script fog (`setVolFog`, `setExpFog`, `setWaterFog`) blends and drives the world fog constants (`level_fog_init` at start, `dog_start` / `dog_stop` over 7 s). Open: the mission-failed fade, underwater and weapon-state channels, the post textures' sampler state.
- [x] The game's own zombies scripts start the match and run round 1.
  - [ ] Client scripts (CSC) run on a client VM of their own (Kino's `main` and `localclientconnect`, its one-shot and loop sounds, its effects — 190 createfx `loopfx`/`oneshotfx` effects, tag effects and effect entities — its lights' state and primary light overrides (`makelight`, `_lights.csc`'s `fire_flicker`, `generic_pulsing` and power-on mixer via `lerpvector`), its own entities presented by the client itself (client-only map models, models it spawns, brush models; from the frame they spawn), its triggers (`touch`/`trigger`, `istouching`) and its entities' animation trees, Kino's clocks, sounds by playback id with the sound system's status fed back (`soundplaying`, `getplaybacktime`, `getknownlength`, `stopsound`, volume and rate), the client's view of its own player (health, weapon, ADS, melee, grenade, position, eye, clip ammo, speed), line emitters, jumps and landings, the server's client system states, `clientNotify`s, client exploders, mirrored snapshot entities and `setclientflag` callbacks, vision and fog; 50 of 173 client builtins not implemented yet, so their sound snapshots and contexts do nothing yet).
- [x] The player moves by the game's own movement rules.
  - [x] Walking, sprinting, jumping, stances, the firing flag (`movement_t5`).
  - [ ] Ladders, mantling, swimming, mounted weapons, launches, falling damage and the other movement events.
- [x] Weapons fire, reload, throw and melee by the game's own rules.
  - [x] Bullets (spread, pellets, penetration), melee, frag grenades, rockets (Ray Gun), script blasts.
  - [x] The viewmodel plays the weapon's own animations, picked by the game's client rules.
  - [ ] Grenade launchers, sticky and special grenades, detonation; dual wield (CZ75).
  - [x] Muzzle flashes (view and world) and ejected shells, by Black Ops' client fire rules.
  - [ ] No tracer or view kick; viewmodel blend times and rates are the runtime's.
- [x] Zombies spawn and animate (rising, or at a window).
  - [x] Window spawns (boards torn off, mantle in) and rising from the ground (`do_zombie_rise`: `scripted rise` from `-45` or `-14` below the spot to the floor, then `find_flesh`).
  - [ ] Legless (gibbed) crawlers, ragdoll and thrown gibs missing.
- [x] Zombies path to the player and get through barriers.
  - [x] All ten of Kino's traversal scripts (54 `Begin` nodes: `zombie_mantle_over_40`, `zombie_jump_down_40`, `_72`, `_120`, `_127`, `_190`, `_240`, `_286`, `zombie_wall_crawl_drop` and `zombie_wall_crawl_drop_quick`) run from their start node to their end node.
  - [ ] No actor collision with the world while walking.
- [x] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
- [x] Rounds advance past round 1.
- [x] Wall weapons and the Mystery Box.
  - [x] The box moves when the teddy bear is rolled (the 950 points are refunded, `moving_chest_now` lifts the box away and shows rubble, and `level.chest_index` advances to the next location), and Fire Sale opens all nine chest locations at 10 points for 30 s before hiding the eight temporary chests again (`disabled` triggers are skipped by `use_sites`).
- [ ] Doors and debris.
  - [x] Kino's doors open for points.
  - [ ] Debris (Five) not reached.
- [x] Power and perks.
  - [x] The power switch, buying and drinking a perk (Quick Revive), the perk icon.
  - [x] Juggernog fills the player's health to 250 (`SetMaxHealth`).
  - [x] Each perk's effect verified (`IW4L_TEST_TIMING`, `IW4L_TEST_WALK`, the go-down test): Speed Cola halves the m1911's reload (1800 → 900 ms), Double Tap shortens the Ray Gun's firing state (350 → 250 ms at 50 ms commands), Stamin-Up raises sprint from 281 to 310 units/s and doubles sprint duration from 4050 to 7950 ms (`IW4L_TEST_WALK=sprint`), and with Juggernog four zombie hits of 60 take 250 health to 10 and the fifth downs the player.
  - [x] The drink's stance, melee and weapon-cycling locks (`allowmelee` sets the no-melee pm flag and `disableweaponcycling`/`enableweaponcycling` set and clear the weapon-cycling flag; `no_melee=true no_weapon_cycling=true` while drinking `zombie_perk_bottle_revive` and `false` after).
- [x] Pack-a-Punch and every power-up.
  - [x] Power-ups drop at the score thresholds; each of Kino's six (Max Ammo, Nuke, Double Points, Insta-Kill, Carpenter, Fire Sale) is picked up and runs, with its HUD icon and announcer line.
  - [x] Pack-a-Punch: paid, knuckle crack, the upgraded weapon taken (AK-74u to AK74fu2).
  - [x] Carpenter's far boards snap back into their windows (`dontinterpolate` on an entity flips its own teleport flag).
  - [x] Pack-a-Punch weapon options and camo: `calcweaponoptions` packs Black Ops' full weapon-options word (`(client + 1) << 21`, camo, lens, reticle, reticle colour), and `weaponoptions.csv` from `common_zombie_patch` dresses the 84 upgraded zombie weapons with the Pack-a-Punch silver etched camo (`camo_packapunch_c` / `_spec` / `_env`, slot 15); optic lens tint and custom zombie reticles remain open.
- [x] Going down, solo revive, bleeding out and game over return to the lobby.
  - [x] A lethal hit downs the player; with Quick Revive (`IW4L_TEST_PLAYER=revive`) the solo player holds `m1911_upgraded_zm` and revives after 10 s; a solo game over fails the level and Kino loads again, as Black Ops does.
  - [x] Going down and `reviveplayer` raise Black Ops' stance animation events, clear damage counters and switch `startrevive`/`stoprevive`'s `LAST_STAND_REVIVED` move type (`t5.match.last_stand` closed).
  - [x] `MissionFailed` ramps client blur to 32 over `g_deathDelay` (4000 ms), `setblur` and `startfadingblur` drive player blur, and `deadscreen` owner draw 97 (`CG_DEADQUOTE`) draws `ui_deadquote` with its `death` sound and post-delay fade-out; save-game restore, `com_codeTimeScale` slow-motion and sound-mixer fade remain open (`t5.match.level_failed`).
- [ ] The game's own HUD.
  - [x] Black Ops' HUD menus (weapon info, dpad, points column `competitivemodescores`) with its fonts, layout, safe area and name-hash paint order; script hud elems (round chalk, point popups, game over text); text and icon use hints (`HINT_NOICON`, `HINT_SEAT`, `HINT_ACTIVATE`, `HINT_HEALTH`, `HINT_FRIENDLY`, `HINT_SPECTATOR`, `HINT_TEAMPOT`, weapons).
  - [ ] Code-drawn pieces (reticle, blood, damage feedback), the scoreboard.
- [ ] The game's own front end: menus, lobby and map selection.
  - [x] In a match, Escape opens Black Ops' own pause menu (`pausedmenu`: RESUME CARNAGE, SETTINGS, RESTART LEVEL, QUIT) and its settings menus, run by its own event scripts; a solo match pauses under it.
  - [ ] Main menu and lobby are Modern Warfare 2's; Kino starts from the game library. Sliders draw as IW4L's bar.
- [ ] The game's own sound.
  - [x] Weapon fire, reload, raise and putaway sounds, notetrack sounds, zombie vocals and script sounds, all Black Ops aliases.
  - [x] The client scripts' one-shot sounds (`playsound` on the client VM): Kino's random ambient emitters (`_audio::soundrandom_thread`: wind, metal, rope, whispers, wood) play their aliases to the local client.
  - [x] The client scripts' loop sounds (`playloopsound` on fake entities and client entities, `soundloopemitter`): Kino's fires, steam, chandeliers, power boxes, circuits and projector loop where its client scripts place them.
  - [x] The player's own footsteps: Black Ops' movement footstep events reach the client scripts' `CodeCallback_PlayerFootstep` (`_footsteps.csc`), which plays `fly_step_<sprint|run|walk|…>_plr_<surface>`.
  - [x] The client scripts' sounds by playback id: the local sound system plays each one, stops and fades it by its id (`stopsound`, `setsoundvolume`, `setsoundvolumerate`), and reports back whether it plays, how far and how long (`soundplaying`, `getplaybacktime`, `getknownlength`), which `_music.csc`'s transitions wait on.
  - [x] The player's jumps and landings reach the client scripts' `CodeCallback_PlayerJump` / `CodeCallback_PlayerLand` (`_footsteps.csc`: `fly_step_run_*`, `fly_land_*`), and their line emitters (`soundlineemitter`) play at the point of their line nearest the player (Kino's wind alleys and water pipes).
  - [ ] Gear, other players' footsteps, ambient rooms, sound contexts and snapshots, `setsoundblend`, another player's automatic fire; a server `PlaySound` notify waits a fixed 2 s.
- [ ] The first map's own systems (Kino's teleporter, its wonder weapon and the rest).
  - [x] The auto turrets spawn as Black Ops' `misc_turret`s and answer its turret methods; Kino starts with no turret faults.
  - [x] Hellhound rounds (`_zombiemode_ai_dogs`, `zombie_dog_*` animscripts): dogs spawn with lightning and sound, claim attack spots around the player, chase, leap to bite for 40 damage, die and drop Max Ammo (`full_ammo`) on the last dog.
  - [x] The Thundergun (`_zombiemode_weap_thundergun`), Cymbal Monkey bombs (`_zombiemode_weap_cymbal_monkey`) and electric traps (`_zombiemode_traps`): `thundergun_zm` equips its viewmodel, fires and blasts zombies and hellhounds in its cone; thrown Cymbal Monkeys animate (`%o_monkey_bomb`), reset their fuse (`resetmissiledetonationtime`), generate attractor positions, play their song and detonate; electric traps activate, shock players and electrocute zombies (`setelectrified`).
  - [x] Nova crawlers (`_zombiemode_ai_quad`, `zombie_theater_quad`): crawlers spawn on the roof, drop through Kino's broken ceilings and hunt the player (`allowpitchangle`, `setphysparams`, `gethitenttype`, Black Ops' `calcpathlength`).
  - [x] The teleporter's server side (`zombie_theater_teleporter`): linking the core and pad, teleporting players to the projection room and back to the theater after its timer.
  - [x] The auto turret, bought, runs Black Ops' turret think: it is drawn with its model, picks zombies in its arcs and sight, swings onto them with its convergence, fires the turret weapon's bullets (`isfiringturret`, `turretstatechange`, `shootturret`) and kills them, itself the attacker. Its fire flash and sound are not shown (`t5.turret.fire_event`).
  - [ ] The projector and the teleporter's visuals (client scripts): the projector's film and beam (client effects on the client's own entities), the teleporter's aftereffects (visions, the FOV through `setClientDvar`) and its black-box vision play; its screen's extra camera does not (#361).
- [ ] Every zombies map in the install (Five, Dead Ops Arcade).
  - [ ] Five (`t5:zombie_pentagon`): loads and starts its scripts; its own systems (debris and the rest) are not reached.
  - [ ] Dead Ops Arcade (`zombietron`): not offered by the game library yet.
  - [ ] Nacht der Untoten (`t5:zombie_cod5_prototype`): offered by the library, not tried.
  - [ ] Verrückt (`t5:zombie_cod5_asylum`): offered by the library, not tried.
  - [ ] Shi No Numa (`t5:zombie_cod5_sumpf`): offered by the library, not tried.
  - [ ] Der Riese (`t5:zombie_cod5_factory`): offered by the library, not tried.
  - [ ] Ascension (`t5:zombie_cosmodrome`): offered by the library, not tried.
  - [ ] Call of the Dead (`t5:zombie_coast`): offered by the library, not tried.
  - [ ] Shangri-La (`t5:zombie_temple`): offered by the library, not tried.
  - [ ] Moon (`t5:zombie_moon`): offered by the library, not tried.
- [ ] Co-op: friends join, revive one another and play to game over.
  - [x] A lobby host picks Kino; a joined player spawns at its co-op spawns, gets points and sees the zombies.
  - [ ] Revive, bleeding out, spectating; no two-machine test.
  - [ ] Actor state reaches clients through the shared entity path; its cost with many zombies is not measured.
- [ ] The game's fidelity ledger ([`fidelity/t5.md`](fidelity/t5.md)) has no open item.

### Technical debt

- Boundary (`xtask/boundary/allow.txt`): about 680 Black Ops references
  outside the game crates (`fastfile_t5::` paths and `AssetNamespace::T5`,
  `ZoneGame::T5`, `"t5"` checks), mostly in `assets` (lane/t5.rs),
  `asset_game` (menu_catalog_t5.rs, weapon_catalog/capture_t5.rs),
  `asset_world`, `asset_material`, `asset_model` and `asset_audio`. Each one
  should move behind a `game_api` rule or into `game_t5`/`fastfile_t5`.
- Clippy: 18 warnings in Black Ops code (menu_catalog_t5.rs,
  capture_t5.rs, world_t5.rs, game_t5): disallowed `unwrap_or_default`,
  collapsible `if`s, `chunks_exact` with a constant size.
- Shared crates Black Ops runs on (`sim`, `hud`, `render_*`) carry most of
  the shared debt listed under Shared technical debt below.

## Black Ops II — Zombies

Indented items are detail: they say what a milestone's runs have shown and
what is still open (the open rules are in [`fidelity/t6.md`](fidelity/t6.md)).
Only the top-level items count. Runs are headless, with the test player
(`IW4L_TEST_PLAYER=1|rounds|use|down|revive`; `IW4L_TEST_USE`, `IW4L_TEST_PERK`, `IW4L_TEST_GOTO` and `IW4L_TEST_BUILD` steer the use test, `IW4L_TEST_WIELD`, `IW4L_TEST_PLACE` and `IW4L_TEST_GRAB` add actions, `IW4L_TEST_VARS` logs `level.zombie_vars`; all in [`BOTS.md`](BOTS.md#test-player)), and the console's `script` command calls Black Ops 2's `_zm_score::add_to_player_score` to give it points. A direct launch needs `IW4L_GAMETYPE=zclassic`, for example `IW4L_SOUND=off IW4L_GAMETYPE=zclassic IW4L_TEST_PLAYER=rounds cargo run --profile play -p launcher -- serve t6:zm_nuked --cmds 'wait world; wait 300s; quit'` ([`BO2-ZOMBIES.md`](BO2-ZOMBIES.md)). Classic on every zombies map but Origins runs Black Ops 2's own scripts.

| Map | Builtins not implemented yet |
|---|---|
| Nuketown (`zm_nuked`, Classic) | 0 of 692 |
| TranZit (`zm_transit`, Classic) | 0 of 741 |
| Buried (`zm_buried`, Classic) | 0 of 727 |

| Map | In the owner's install | Scripts | Run |
|---|---|---|---|
| Nuketown (`zm_nuked`) | yes | Black Ops 2's | headless, below |
| TranZit (`zm_transit`) | yes | Black Ops 2's | headless, below |
| Buried (`zm_buried`) | yes | Black Ops 2's | headless, below |
| Die Rise (`zm_highrise`) | no | Black Ops 2's | not yet |
| Mob of the Dead (`zm_prison`) | no | Black Ops 2's | not yet |
| Origins (`zm_tomb`) | no | IW4L's rules | older builds only |

- [x] The install is recognised; the map's zones and scripts are read.
  - [x] Nuketown, TranZit and Buried (the zombies maps in the install), with the FFOTD `_globallogic` captured from the owner's running game.
- [x] The map loads, renders and collides (Nuketown).
  - [x] TranZit and Buried load and collide too, with their Classic zombie spawners (`AddonMapEnts`).
  - [x] The client scripts (CSC) run on a client VM of their own (`0 of 243` client builtins not implemented yet on Nuketown, `0 of 249` on TranZit, `0 of 257` on Buried): client-side map entities (`CG_SpawnEntities`), client fields (`zombie_has_eyes`, `zombie_power_on`), client system states (`statechange`), exploders, fxanim props and wall-buy chalk models (`spawn_weapon_model`).
- [x] The game's own zombies scripts start the match and run round 1.
  - [x] Every builtin the three maps' compiled scripts reference is registered (`0 of 692` on Nuketown, `0 of 741` on TranZit, `0 of 727` on Buried).
- [x] The player moves by the game's own movement rules.
  - [x] Walking, sprinting, jumping and stances (`movement_t6`), shown by the walk test on Nuketown and TranZit (walk up to 190, sprint 285, jump 37.5, crouch view 40, prone 11).
  - [x] Script allowances (`allowstand`, `allowcrouch`, `allowprone`, `allowjump`, `allowlean`, `setstance`) set Black Ops 2's own player flags.
  - [x] Ladders (`PM_LadderMove`, `t6.movement.ladder` closed) and falling damage (server `EV_FALL_DAMAGE`, `MOD_FALLING`), with the client scripts' jump, land and foliage callbacks.
  - [ ] Mantling (`t6.movement.mantle`), the water level (`PM_WaterMove` is ported but the runtime has no water query, so no player swims: `t6.movement.water`), mounted weapons, launches and the movement events and player animations (`t6.movement.events`).
- [x] Weapons fire, reload, throw and melee by the game's own rules.
  - [x] Black Ops 2's weapon facts (damage, ranges, clip and stock, hit-location scales) are read from its zones.
  - [x] Black Ops 2's own weapon state machine (`movement_t6`: fire, burst, rechamber, reload, switch, melee, offhands, riot shield, weapon heat) and server fire (`game_t6/src/fire.rs`: seeded spread, pellets, penetration, damage falloff and hit locations, melee, grenades, rockets) run the weapons: the rounds test reaches round 2 on Nuketown (7 kills, 7 frags thrown) and Buried (12 kills) in 110 s, with reloads and knifing; TranZit sells the Olympia from the wall and it fires; Monkey Bombs are thrown on Buried and a claymore planted on TranZit. Remaining gaps (gun angles, antilag, gas, mounted fire, …) are in the ledger.
  - [x] `knife_zm` is given and knifes by Black Ops 2's melee rules (its range, width and zombies values).
  - [x] An external view model animation no loaded zone carries takes the startup zone's default `void` animation, as Black Ops 2's linker does, so `m1911_upgraded_zm` (the last stand pistol after Pack-a-Punch) is given (`calcweaponoptions`).
  - [x] Thrown and scripted grenades and missiles run Black Ops 2's missile physics, bounces, rest (`stationary`) and explosion surface (`game_t6::missile`, `game_t6::grenade`, `detonate`, `magicgrenadetype`, `makegrenadedud`, `resetmissiledetonationtime`, `forcegrenadethrow`, `setmissileowner`): claymores planted on TranZit (`claymore_zm`) arm on `stationary`, wake on an approaching zombie (`setexcludeteamfortrigger`), pass `shouldaffectweaponobject` and `damageconetrace` and detonate to kill it; Monkey Bombs thrown on Buried (`cymbal_monkey_zm`) generate their attractor ring (`attractor_positions_generated`), draw zombies in and kill them on explosion.
- [x] Zombies spawn and animate (rising, or at a window).
  - [x] Zombies rise on Nuketown and spawn on TranZit and Buried, animated by Black Ops 2's animation state machine (ASD).
  - [x] Pitch orientation (`setpitchorient`, `clearpitchorient`, `allowpitchangle`) aligns crawlers and crater zombies to the ground slope (`t6.actors.pitch` closed; Nuketown crater crawlers pitch to `-28.1°` on the slope and reset to `0°` at `zombie_crawler_standup`), and `gib` dispatches `CodeCallback_GibEvent` (`_zm.csc::on_gib_event`, `csc_gibbed` 1; `t6.actors.gib` closed); ragdoll physics (`t6.actors.ragdoll`) remain open.
- [x] Zombies path to the player and get through barriers.
  - [x] On Nuketown zombies path to the player and reach melee range.
  - [x] Doors, debris and the map's other dynamic-path models (spawnflag 1) cut the zombies' paths while closed, as Black Ops 2 does (TranZit: 74 entities, 523 links; the depot's glass doors no longer draw zombies that cannot pass).
  - [x] TranZit reaches round 2: zombies tear the depot's window boards and come in; its two inert zombies outside wake when the player comes near, as Black Ops 2's `start_inert` does (the test player goes to the nearest zombie after 20 s with none in sight).
  - [x] Boards the zombies tore off are rebuilt by holding use at the window (TranZit: four boards, `rebuild_barrier_reward` 40, 10 points each).
- [x] Shots and melee kill zombies, zombies hurt the player, kills and hits earn points.
  - [x] Shots and the knife kill zombies through Black Ops 2's `G_Damage` and death callbacks; kills and hits earn its points (Nuketown: round 3, 16 kills in 300 s).
  - [x] Zombies hurt the player through Black Ops 2's actor melee and player damage callback.
  - [x] The weapons that fire are Black Ops 2's own (see the weapons milestone); the lethal grenade `giveweapon` gives is the player's offhand and is thrown (`grenade_fire`, `explode`).
  - [x] A zombie's swing that lands on another zombie does nothing, as Black Ops 2's melee hit does.
- [x] Rounds advance past round 1.
  - [x] Nuketown reaches round 3 in the test player's 300 s runs, with round-end stats (`set_global_stat`, `update_playing_utc_time`) reading and writing `statstable_zm.ddl` without faults.
  - [x] A plain launch runs Black Ops 2's own scripts: Nuketown's rounds test (Ray Gun given) reaches round 9 in 12 minutes (158 kills, 12 760 points) with every perk dropped in, and round 7 in 10 minutes grabbing seven power-ups (double points, max ammo, insta-kill, nuke) with `IW4L_TEST_GRAB=1`; a dead actor becomes a corpse in one of `ai_corpseCount` slots and frees its actor slot (before, round 4 stalled once 32 dead actors filled the slots), and a kill pays its points from the actor's `attacker`.
   - [x] TranZit and Buried in a plain launch, rounds test with the Ray Gun and `IW4L_TEST_GRAB=1`, 14 minutes each: TranZit reaches round 8 at 730 s (148 kills, 16 880 points by 850 s, nine power-ups grabbed), Buried at 795 s (144 kills, 11 420 points, six power-ups grabbed); neither stalls, and the only server script fault on either map is `t6.actors.ragdoll`.
- [x] Wall weapons and the Mystery Box.
  - [x] The box takes its 950 and opens (Buried).
  - [x] Wall weapons sell through Black Ops 2's unitriggers on Nuketown, TranZit and Buried (the Olympia: 500 points, the gun changes).
  - [x] Wall weapons filed under a zone not yet active wait for it, as in the game; buying a wall weapon again refills its stock ammo for half price (TranZit: Olympia bought for 500, fired and reloaded, then refilled to 38 stock for 250).
  - [x] The box's weapon rises and is taken (Buried: 950 points, the Time Bomb, through Black Ops 2's box scripts and `useweaponmodel`).
  - [x] The box moves: in ten opens on Buried it gave the Time Bomb, M32, SRM1216, Ray Gun, DSR-50; on the seventh the teddy bear refunded the price (`moving_chest_now`, `box_moving`) and the box reopened in `zone_underground_courthouse2` (Saiga-12).
  - [x] Fire Sale's boxes (every location, 10 points) rise on all six chests (`start_fire_sale`, `zombie_powerup_fire_sale_on`), take 10 points (`500 -> 490`) and give their weapon (`dsr50_zm`).
- [x] Doors and debris.
  - [x] Doors open for points on Nuketown, TranZit and Buried, debris clears on Nuketown and Buried, by the game's scripts through Black Ops 2's use rule (Buried: three doors and two debris piles in one run).
  - [x] Closed doors and debris cut the zombies' paths until they open (Black Ops 2's dynamic paths).
- [x] Power and perks.
  - [x] Quick Revive sells on TranZit (solo: 500 points, the bottle is drunk) through Black Ops 2's use rule.
  - [x] Buried's power switch turns the power on (`power_on`, the perk machines' power notifies) and Double Tap sells after it (2000 points; the perk is given).
  - [x] TranZit's power switch is built at the power station from its three parts (test player: `IW4L_TEST_GOTO=zone_pow IW4L_TEST_BUILD=powerswitch IW4L_TEST_USE=door,piece,build,power,perk`), its lever is thrown, `power_on` is set by the map's 30 s power event and a perk sells after it; Nuketown's perks arrive by vehicle drop across rounds 1–4 (below).
- [x] Pack-a-Punch and every power-up.
  - [x] Pack-a-Punch upgrades a gun on Buried after the power (5000 points; the Olympia becomes `rottweil72_upgraded_zm` and is taken).
  - [x] Each power-up a map includes, dropped by Black Ops 2's `specific_powerup_drop` at the player, is grabbed and works: double points (`zombie_point_scalar` 2), insta-kill, max ammo (stock refilled), nuke (+400), fire sale (Nuketown); carpenter (`carpenter_finished`, +200), free perk, random weapon (Buried).
  - [x] TranZit's `insta_kill_ug` (`zombie_powerup_insta_kill_ug_on`) and the bank teller's `teller_withdrawl` (1100 paid at `bank_teller_transfer_trig`, 1000 picked up from the dropped `teller_withdrawl` power-up) work.
- [x] Going down, solo revive, bleeding out and game over return to the lobby.
  - [x] Hits go through Black Ops 2's `finishplayerdamage` (damage timer, health, the `damage` notify); a lethal hit puts a player down and runs the last stand callback.
  - [x] Solo without Quick Revive: going down runs Black Ops 2's solo death (Nuketown's fall, `end_game`) and the match returns to the lobby on Nuketown, TranZit and Buried without faults (the fall linked to an anchor, `death_throe_zm` held, the rumbles stopped).
  - [x] With Quick Revive (TranZit, `IW4L_TEST_PLAYER=revive`): down at 51 s, Black Ops 2's solo revive stands him up at 61 s; the next down ends the game and the match returns to the lobby.
  - [x] Bleeding out is co-op only (see Co-op); going down applies Black Ops 2's last-stand vision (`visionsetlaststand`, `"zombie_last_stand"`) and fading blur (`startfadingblur`, `3.0 -> 0.0` over 850 ms; `t6.vision.last_stand` and `t6.player.blur` closed).
- [x] The game's own HUD.
  - [x] Black Ops 2's Zombies LUI HUD widgets (`PerksArea`, `PowerUpsArea`, `BuildablesArea`, `CompetitiveScoreboard`, `DPadArea`, Buried's `AmmoAreaZombie` and `TimeBombArea`, `RoundStatus`, `Scoreboard`) run on `game_t6::ui` with timed transitions, LUI client field callbacks (`setupclientfieldcodecallbacks`), `allowroundanimation` (`t6.csc.lui.allow_round_animation` closed), `luinotifyevent` (`score_event`), and round, score, weapon, ammo and offhand HUD updates.
  - [x] Black Ops 2's legacy HUD menus (`ui_mp/hud_zombies.txt`, `ui_mp/hud_<g_gametype>.txt`, `ui_mp/hud_popups.txt`) are captured with their `t6` postfix expressions (`game_t6::menu_expression`), screen placement (`ScrPlace_SetupViewport`, `ScrPlace_ApplyRect`) and UI font choice (`UI_GetFontHandle` in `game_t6::scrplace`), drawing the `cursorhints` ownerdraw (72: localized text, activate and weapon pickup hints; Nuketown wall buy draws `"Hold ^3E^7 for Olympia [Cost: 500]"` and `"Hold ^3E^7 for ammo [Cost: 250]"` in `fonts/720/smallFont`), the weapon reticle (`CG_DrawCrosshair`: `reticle_side_small` and `reticleCenter` from `WeaponDef`), and server `HudElem`s (`CG_Draw2dHudElems`: background `overlay_low_health` on low health and foreground `"GAME OVER"`, `"You Survived 1 Round"` and `"black"` fade at game over; `t6.hud.script_materials` closed) in place of IW4L's HUD bridge.
  - [ ] Remaining HUD menu ownerdraws (directional hit indicator 330, invalid-command hint 113, hold-breath hints 71/311, mantle/turret hints 80/378, revive hints 199, death quote 97, hit-location items), overhead names (`t6.csc.ui.name_fade`) and `^8`/`^9`/`^F`/`^B` text codes remain open (`t6.hud.code_hud`).
- [x] The game's own front end: menus, lobby and map selection.
  - [x] Nuketown, TranZit and Buried start from the game library, with Black Ops 2's loading screens (`loadscreen_<map>_<gametype>_<location>` from the `*_load_zm` zones) and front-end art.
  - [x] Black Ops 2's Zombies front-end LUI (`T6.CoDRoot`, `T6.main`, `T6.Zombie.ffotdzm` in `Scene::front_end`: `MainMenu`, `MainLobby`, `SelectMapZM`, `SelectStartLocZM`, `PrivateOnlineGameLobby`, `PublicGameLobby`, `OptionsMenu`, `QuitPopup`) and in-game pause menu (`class`, `RestartGamePopupZombie`, `OptionsMenu`, `EndGamePopup` in `Scene::new`) run on Black Ops 2's own compiled `.lua` modules, dvar registry (`dvars.rs`), gamer profile defaults (`profile.rs`), DDL stats/gametype settings (`stats.rs`, `zm/gamesettings_*.cfg`) and string tables (`zm/mapsTable.csv`, `zm/gametypesTable.csv`), launching matches and `map_restart`/`disconnect` directly (`t6.hud.menu_font`, `t6.hud.menu_layout`, `t6.hud.ingame_menu` closed).
- [x] The game's own sound.
  - [x] `IW4L_SOUND_TRACE=1` logs every sound the match asks for, headless included (Nuketown, 120 s: round music, the perk jingles and the perk drop's alarm and landing, thunder, zombie vocals; the meteors', perk machines' and Pack-a-Punch's loops, the underscore music).
  - [x] The client scripts' sounds play by playback id on the local client's sound system (`playsound` with Black Ops 2's volume clamp and fade-in, `stopsound`, `setsoundvolume`, `soundplaying`, `getplaybacktime`), and their loops on fake entities and client entities play where the scripts place them; a muted windowed Nuketown run reports them playing and ending (`report id=1 playing=false at=5824ms`).
  - [x] The maps' ambience runs on Black Ops 2's client rules: point and line emitters and the sound randoms (Nuketown, 250 s: `amb_fire_small`, the wind lines and power lines, 1689 `amb_nuked_wind_gust` from 41 randoms at 4–8 s), and the ambient rooms (Nuketown `outdoor` → `int_room` → `truck_room` with each room's tone and reverb; Buried `buried_outdoor` → `buried_warehouse`; TranZit `station_room_standard`). Nuketown, Buried and TranZit raise no `t6.csc.sound.*` fault.
  - [ ] The mixer commands (snapshots, side-chain ducks, futz, reverb rooms, contexts) are sent and traced but not applied by IW4L's audio (`t6.audio.mix`); footsteps and the player's movement sounds (#288) and, with a sound system, the server sounds' reported lengths and rumbles (`t6.sound.notify_length`, #339) are open.
- [x] The first map's own systems (Nuketown's perk drops and its end-game rocket).
  - [x] Quick Revive drops from the sky on its path vehicle (`perk_arrival_vehicle` on `info_vehicle_node`s to `reached_end_node`), lands and sells (500, drunk); the later drops follow the same path at rounds 3–5 on.
  - [x] At game over the rocket intermission plays to `rocket_hit_nuketown` and the match returns to the lobby (the camera calls are kept for the client to present).
- [x] Every zombies map in the install (TranZit, Buried).
  - [x] TranZit and Buried run round 1 with zombies spawning; doors open on TranZit, the box on Buried.
  - [ ] Die Rise, Mob of the Dead and Origins are not in the owner's install: Die Rise and Mob of the Dead have not run on Black Ops 2's own scripts, and Origins runs IW4L's rules.
  - [x] TranZit's bus drives its route on its path nodes once the depot doors open, with the player riding it (depot, diner, forest, on toward the power station in 220 s), its scripts reading tags off its posed model and who stands on it.
  - [x] TranZit's power: the powerswitch built from its three parts, its lever thrown, `power_on` set by the map's 30 s power event, and a perk sold after it.
  - [x] TranZit's riot shield built from its two parts, taken from its table and held (its shield model attached).
  - [x] TranZit's turbine and turret carried and set down by Black Ops 2's placement test (`IW4L_TEST_WIELD=equip_turbine_zm,equip_turret_zm IW4L_TEST_PLACE=1`); the placed turret is spawned, set to `auto_nonai` and powered by the turbine beside it.
  - [x] The placed turret picks zombies, swings onto them, fires Black Ops 2's bullets (penetrating by the zone's depth table) and kills them, itself the attacker.
  - [x] TranZit's Jet Gun (`jetgun_zm`) is built from its four parts, wielded, spun up and overheated (`getcurrentweaponspinlerp`, `setcurrentweaponspinlerp`, `isweaponoverheating`, `setweaponoverheating`, `movement_t6::weapon::update_weapon_heat`), drawing zombies in along the player's `tag_origin` and `tag_flash` and exploding on `jetgun_overheated` on Black Ops 2's own weapon state machine (`t6.weapons.overheat` closed).
  - [x] TranZit's screechers spawn in the fog, jump onto the player (`linkto`) and are knifed off (`screecher_StepBack_death`); the Avogadro spawns after the power event (`avogadro_arrival`) and fights with ranged bolts (`weapon_fired`).
  - [x] Buried's scripts run without faults through 300 s of opening the town (power, doors, debris): Leroy spawns in his cell, the buildable pieces spawn, the maze and the fountain move.
  - [x] Buried's Sloth (Leroy) is freed from his cell with the key (`open_cell`) and fed booze to smash through barriers (`jail_open_anim` -> `run_bezerk` -> `Wait_smash`, root-motion yaw and `isinscriptedstate`); the open-table buildables (`equip_turbine_zm`, `equip_springpad_zm`, `equip_subsurface_zm`, `equip_headchopper_zm`) are built and placed (including wallmount/microwave turret placement via `canplayerplaceturret`); the haunted mansion ghost waves (`_zm_ai_ghost.gsc`) spawn and attack.
  - [ ] The bus's collision model pieces (`setzbarriercolmodel`) are kept, not collided against (`t6.barriers.colmodel`).
- [ ] Co-op: friends join, revive one another and play to game over.
  - [x] With a bot as the second player on Nuketown: he joins and spawns, goes down, bleeds out after 45 s and respawns at the next round; the test player revives him (`player_revived`, the reviver named) each time he goes down; with both down the game ends and the match returns to the lobby.
  - [ ] A second player over the network (two machines) not tried.
- [ ] The game's fidelity ledger ([`fidelity/t6.md`](fidelity/t6.md)) has no open item.

### Technical debt

- Boundary (`xtask/boundary/allow.txt`): about 280 Black Ops II references
  outside the game crates (`fastfile_t6::` paths and `AssetNamespace::T6`,
  `FamilyId::T6`, `ZoneGame::T6` checks), mostly in `assets` (lane/t6.rs),
  `asset_game` and `asset_transport`. The zombies and multiplayer hosts
  (`sim/src/script/host/t6_zombies*`, `t6_gametype.rs`) and `ui/src/t6_menu.rs`
  are Black Ops II code living in shared crates; the zombies host is kept only
  for Origins until it runs Black Ops II's scripts, then it goes.
- Compiler warnings: unused `world`, `direction` and `key` and the unused
  `give`/`take` methods in `sim/src/script/host/t6_zombies/origins_staff.rs`;
  the unused `entity_string` import in `assets/src/lane/t6.rs`.
- Clippy: 88 warnings in Black Ops II code, most in
  `sim/src/script/host/t6_zombies.rs` (36), `ui/src/t6_menu.rs` (11),
  `t6_zombies/origins_staff.rs` (8) and `assets/src/lane/t6.rs` (8).
- Not formatted by `rustfmt --edition 2024`: `t6_zombies.rs` and
  `t6_zombies/origins_staff.rs`.
- Zombies maps need the patch zone Black Ops II downloads at launch
  (`t6.scripts.ffotd`); a machine without a capture in
  `~/.config/iw4l/captured/` or `<artifacts>/captured/` cannot start
  Nuketown, TranZit or Buried, and the capture tool is not in the repository.

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

### Technical debt

- Boundary (`xtask/boundary/allow.txt`): 3 Black Ops III references outside
  the game crates (`asset_transport`, `audio`, `sim`).
- The master protocol's content flags carry four games; Black Ops III maps
  are not gated by asset families or advertised to the master.
- The audio mix bus for Black Ops III is untuned (no trim or compressor).

## Shared technical debt

Debt in the shared runtime that no single game owns.

- Boundary graph (`xtask/boundary/allow.txt`): 22 dependency edges outside
  the layer rules (`render_fx` on `sim`, `net`, `assets` and nine more;
  `replay` on `sim`, `net`, `frame`, `playerstate_iw4`; `weapon_iw4`,
  `fastfile_iw4`, `marks_iw4` and `render_frame` on `asset_iw4`;
  `render_frame` on `render_material`; `clipmap_iw4` on `trace_iw4`) and 25
  crates still in the `mixed` layer, which have to be split into a game and a
  neutral part.
- Boundary patterns not tied to one game: 224 (`render_frame::` 131, `fx::`
  47, `render_fx::` 46), because `render_frame`, `render_fx` and `fx` serve
  every game but are declared `game = "iw4"`. They sit in `render_frontend`
  (113), `render_anim` (44), `render_gpu` (44), `render_backend` (12),
  `render` (7), `console` (2), `session` and `render_scene` (1 each).
- Compiler warnings: 9 uses of Bevy's deprecated `Interaction` in `ui` (6 in
  `launcher.rs`, 3 in `t6_menu.rs`, which the Black Ops II section also
  counts; move to `Hovered`/`Pressed`); 4 function pointer comparisons in
  `game_api`'s derived `PartialEq` (`ViewmodelRules::anim`,
  `font_scales_sent`, `MenuFonts`, `ClientFireRules::sound`); the deprecated
  `fetch_update` in `render_frame/src/skinning.rs`; an unused `name` in
  `audio/src/diagnostics.rs`.
- Clippy: about 1,140 warnings across the workspace, nearly 1,000 of them in
  shared code — `sim` (207), `render_frontend` (161), `asset_game` (98),
  `render_anim` (90), `hud` (72), `render_gpu` (68), `console` (67), `net`
  (66), `fx` (63). The workspace is not clippy clean, and CI does not run
  clippy (`.github/workflows/ci.yml` runs `make publish-check` and
  `cargo check --workspace --all-targets`); CONTRIBUTING.md asks contributors
  to run it.
- Python (`ruff check` and `ruff format`): `scripts/bench-marks.py` has 3
  findings (a shebang on a non-executable file, two `zip` pairs for
  `itertools.pairwise`) and is not formatted. CI runs neither ruff nor rustfmt.
- The headless `serve` launch installs `RuntimeRole::Listen`
  (`bootstrap/src/plugins.rs::add_runtime_plugins`); `RuntimeRole::Dedicated`
  and `NetPlugin::dedicated` exist, but no launch selects them.

<!--
How measured (2026-10-15, checkout iw4L-docs at 7d337727, nothing built):
- Edges: `grep -c '^edge' xtask/boundary/allow.txt` = 22 (render_fx 12, replay 4,
  render_frame 2, clipmap_iw4/fastfile_iw4/marks_iw4/weapon_iw4 1 each);
  `grep -c '^mixed'` = 25.
- Patterns: awk over `^pattern` lines summing column 4 where column 3 is
  `render_frame::`, `render_fx::` or `fx::`, grouped by crate (path field 2).
- Interaction: `grep -rn '\bInteraction\b' crates/ui/src`.
- CI: read `.github/workflows/ci.yml`.
- Dedicated: read `bootstrap/src/launch.rs` (`LaunchMode::Serve` → `Role::Dedicated`
  → `add_runtime_plugins`) and `bootstrap/src/plugins.rs`.
- NOT re-measured: compiler warning counts, clippy counts, ruff findings (kept as
  written). The `*_iw4::` total in the MW2 section is 2,711 by the same awk
  (`$3 ~ /_iw4::$/`), not "about 2,800".
-->
