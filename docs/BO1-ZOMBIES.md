# Black Ops Zombies

Black Ops (2010) Zombies runs on Black Ops' own data and rules: the map's
zones, its server and client zombie scripts, its movement, weapons, HUD menus
and sounds. Kino der Toten plays solo from round 1 to game over, with the box,
doors, power, perks, Pack-a-Punch, power-ups, hellhounds, Nova crawlers, the
Thundergun, Cymbal Monkeys, traps and the teleporter, graded by Black Ops'
own post chain and vision channels (the map's vision, last stand, bleed-out). Milestones and what runs
have shown: [`PROGRESS.md`](PROGRESS.md) (Black Ops — Zombies); every rule still
missing or lent: [`fidelity/t5.md`](fidelity/t5.md). Black Ops II: [`BO2-ZOMBIES.md`](BO2-ZOMBIES.md).

## Start a game

- Solo: the game library, Black Ops → Zombies → Kino der Toten. Directly:
  `IW4L_GAMETYPE=zombies ./target/play/iw4l map t5:zombie_theater` (or
  `IW4L_GAMETYPE=zombies make map ZONE=t5:zombie_theater`). Without
  `IW4L_GAMETYPE=zombies` a Black Ops map is refused at load.
- Headless, as a dedicated server ([`RUN.md`](RUN.md) for `--cmds`):
  `IW4L_SOUND=off IW4L_GAMETYPE=zombies ./target/play/iw4l serve t5:zombie_theater --cmds 'wait world; bot add 1; wait 20s; quit'`
- Co-op host: `set ui_mapname t5:zombie_theater; set ui_gametype zom;
  ui_create_lobby; ui_lobby_privacy` opens the lobby and lists it on the
  master ([`MASTER.md`](MASTER.md)). The match starts once every lobby member
  is connected. Only a host and one joined player on the same Mac are proven.

## Maps

The library offers ten maps (`ZOMBIES_LIBRARY`, `crates/game_t5/src/lib.rs`).

| Map | Zone | State |
|---|---|---|
| Kino der Toten | `t5:zombie_theater` | Played solo; co-op on one machine |
| Five | `t5:zombie_pentagon` | Loads and starts its scripts; its systems are not reached |
| Nacht der Untoten, Verrückt, Shi No Numa, Der Riese | `t5:zombie_cod5_prototype`, `_asylum`, `_sumpf`, `_factory` | Not tried |
| Ascension, Call of the Dead, Shangri-La, Moon | `t5:zombie_cosmodrome`, `zombie_coast`, `zombie_temple`, `zombie_moon` | Not tried |
| Dead Ops Arcade | `t5:zombietron` | Not offered |

## Test player

`IW4L_TEST_PLAYER` drives the first player in headless runs and logs every
five seconds: `1` fights, `rounds` fights with god mode and ammo, `use` also
buys one of each thing (`IW4L_TEST_USE=wall,perk,door,box,…`,
`IW4L_TEST_PERK`), `down` waits to go down, `revive` buys Quick Revive then
goes down, `watch` only logs. `IW4L_TEST_GRAB`, `_THROW` and `_WALK` add
power-up pickups, grenades and walks; `IW4L_TEST_ACTORS`, `_WAITS` and `_VARS`
add fields to the log ([`test_player.rs`](../crates/bots/src/test_player.rs)).

## Reaching a system

`script <module::function> <receiver> [args]` (with `set sv_cheats 1`) runs a
Black Ops script function, the quickest way to one feature. Kino's stage auto
turret, with the player in front of the stage so zombies walk into its arcs:

```
set sv_cheats 1; wait world; god; wait 5s; script common_scripts/utility::flag_set level power_on; move 0 350 120 90 0; wait 15s; script maps/_zombiemode_auto_turret::auto_turret_activate targetname=pf54_auto_turret_trigger; wait 30s; quit
```

A `move` right after `wait world` is lost (the player is not alive yet):
wait a few seconds first. The turret fires only once zombies reach its arcs
(about 15 s into the first round). `fx_dump 1000` logs the effects near the
camera (`def=…`), which shows a muzzle flash a screenshot misses.

The projector's film plays once power is on and the screen is down (about
25 s); look at it from `move 0 350 120 90 0`. A teleport, from the pad by the
projector room:

```
set sv_cheats 1; wait world; god; wait 5s; script common_scripts/utility::flag_set level power_on; wait 5s; move -304 1114 40 90 0; wait 1s; script maps/zombie_theater_teleporter::player_teleporting targetname=trigger_teleport_pad_0 0; wait 40s; quit
```

The teleporter's aftereffect is picked at random only after power on (the
teleporter sets `theaterAftereffectOverride` to -1 then); before that the
client always plays the FOV one. `script maps/zombie_theater_teleporter::teleport_aftereffect_fov player0`
sends one.

`teleport_aftereffect_shellshock_electric` (a 3 s `electrocution`, which plays
`shock/default.shock`) and `teleport_aftereffect_shellshock` (`explosion`)
shellshock the player. The blurred screen blends the saved view over the new
one, so it shows only while the view moves: turn right after the shock.

```
set sv_cheats 1; wait world; god; wait 3s; script maps/zombie_theater_teleporter::teleport_aftereffect_shellshock_electric player0; wait 500ms; mousemove 1500 0; screenshot shock; wait 4s; quit
```

The shocked player walks at 0.4 of their speed and cannot sprint
(`IW4L_TEST_PLAYER=1 IW4L_TEST_WALK=1` logs the speeds).

The map's dynamic entities (`dynents: models=142 … spare_slots=256` in the
log) stand where the map puts them: the bar's bottles and glasses from
`move 1000 430 60 0 10`, the lobby shelf's pitcher and paper stack from
`move 420 -1356 100 0 10`. Shots and blasts launch clutter and break
destructible paper stacks (`t5.dynent.physics`, `t5.weapons.blast_effects`).

Bullets draw Black Ops' beams from the gun's flash: most zombies guns draw a
red tracer on every second bullet and a thin streak on the others (shotguns a
tracer on every pellet). A beam lives only as long as its head takes to reach
the hit at 7500 units a second, so a screenshot catches one only while the
trigger is held:

```
set sv_cheats 1; wait world; god; wait 5s; give weapon/t5:ak74u_zm; wait 3s; press +attack 2; wait 500ms; screenshot tracers; wait 3s; quit
```

## Open question

Does a friend need Modern Warfare 2 installed next to Black Ops? The main menu
and lobby are Modern Warfare 2's, so today the answer is yes; whether the match
itself stands on Black Ops data alone is untested (Black Ops' own front end).
