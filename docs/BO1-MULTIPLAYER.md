# Black Ops multiplayer

Black Ops' multiplayer maps run Black Ops' own multiplayer scripts
(`common_mp`, `patch_mp` and the map's zone) on its own movement and weapons.
The goal is private matches between friends who each own the game; public
matchmaking is out of scope. Progress: [`PROGRESS.md`](PROGRESS.md) (Black
Ops — Multiplayer); open rules: [`fidelity/t5-mp.md`](fidelity/t5-mp.md).

## Start a match

Free-for-all (`dm`), Team Deathmatch (`war`, Black Ops' `tdm`),
Domination (`dom`), Headquarters (`koth`), Capture the Flag (`ctf`), Search
and Destroy (`sd`), Sabotage (`sab`) and Demolition (`dem`) run;
every other gametype is refused (`t5.scripts.mp_gametypes`).

```bash
IW4L_GAMETYPE=dm make map ZONE=t5:mp_nuked
```

On connect the scripts open Black Ops' team and class menus; a pick in
them goes to the scripts' `menuresponse`. Most class-menu items do not
paint yet (create-a-class, #650); a preset class can also be picked from
the console:

```
script maps/mp/gametypes/_globallogic_ui::menuclass player0 smg_mp
```

In Team Deathmatch pick a team first:
`script maps/mp/gametypes/_globallogic_ui::menuautoassign player0`.

Presets: `smg_mp`, `cqb_mp`, `assault_mp`, `lmg_mp`, `sniper_mp`.

The host's console sets the scripts' limits, read each second as in Black
Ops: `set scr_dm_scorelimit 100`, `set scr_dm_timelimit 2` (Team
Deathmatch: `scr_tdm_…`).

Bots: Black Ops adds its own (Combat Training rules) from these dvars,
read when the match starts. Offline, before launch:
`IW4L_SCRIPT_DVARS="set splitscreen_playerCount 1; set splitscreen_botEnemies 3; set splitscreen_botFriends 2"`.
In a hosted private match, from the console:
`set scr_bots_managed_spawn 1; set scr_bots_managed_all 6`. IW4L's own
bots drive them (#709); `bot add N` also works. A headless
check: `--cmds "wait world; wait 3s; script … smg_mp; wait 10s; screenshot x; finish_run"`.

## Weapons

Every base weapon fires, reloads and aims on Black Ops' own weapon
definitions, with its attachment variants. An underbarrel grenade
launcher, flamethrower or Masterkey is switched to with action slot 3
(D-pad left), as the class scripts bind it; dual-wield pistols and SMGs
fire the right gun with fire and the left with aim. With cheats on, the
console gives a weapon by its family and attachments:

```
give weapon/t5:famas gl
give weapon/t5:l96a1 vzoom
```

Use the `t5:` prefix: several Black Ops names (`famas`, `ak47`, `aug`)
are also Modern Warfare 2 weapons. Dropped weapons still follow the
runtime's own item rules (#720).

## Play with friends

Host an unlisted lobby and hand your friends its invite code
([`MASTER.md`](MASTER.md), Private lobbies). Each friend needs their own
Black Ops install and the same IW4L build.

## Maps

The 14 base maps of the install, each played to the end of a Team
Deathmatch with Black Ops' bots (`IW4L_SCRIPT_DVARS="set splitscreen_playerCount 1; set splitscreen_botEnemies 3; set splitscreen_botFriends 2; set scr_tdm_timelimit 1"`):

| Map | Loads | Played to the end | Script faults |
|---|---|---|---|
| `mp_array` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_cairo` | yes | yes | none |
| `mp_cosmodrome` | yes | yes | none |
| `mp_cracked` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_crisis` | yes | yes | none |
| `mp_duga` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_firingrange` | yes | yes | none |
| `mp_hanoi` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_havoc` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_mountain` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_nuked` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_radiation` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_russianbase` | yes | yes | `testspawnpoint` (tactical insertion, #649) |
| `mp_villa` | yes | yes | none |

A tactical insertion fault depends on the bots' random loadouts, so it
shows on some runs and not others. Destructibles and map interactives are
#669; DLC maps are not in the owner's install.

## HUD

The kill feed is Black Ops' own: each obituary prints in the bottom-left
game message window with the weapon's or the means of death's kill icon and
the names in the team colours the team-set scripts pick. The compass shows
the map turning with the view around the player's arrow.

## Not there yet

Parts of the multiplayer HUD (#651: the killcam and hardcore-list menus
paint all the time, the compass shows only its map and the player's arrow), client scripts (#645), the
class menu's paint,
create-a-class (#650), killstreaks (#656), Black Ops' own bot behaviour (#709), the objective and
round gametypes (#653, #654) — see the issues labelled `game: Black Ops 1`,
`mode: Multiplayer`.
