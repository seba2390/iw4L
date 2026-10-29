# Black Ops multiplayer

Black Ops' multiplayer maps run Black Ops' own multiplayer scripts
(`common_mp`, `patch_mp` and the map's zone) on its own movement and weapons.
The goal is private matches between friends who each own the game; public
matchmaking is out of scope. Progress: [`PROGRESS.md`](PROGRESS.md) (Black
Ops — Multiplayer); open rules: [`fidelity/t5-mp.md`](fidelity/t5-mp.md).

## Start a match

Free-for-all (`dm`) and Team Deathmatch (`war`, Black Ops' `tdm`) run;
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

Presets: `smg_mp`, `cqb_mp`, `assault_mp`, `lmg_mp`, `sniper_mp`. A headless
check: `--cmds "wait world; wait 3s; script … smg_mp; wait 10s; screenshot x; finish_run"`.

## Play with friends

Host an unlisted lobby and hand your friends its invite code
([`MASTER.md`](MASTER.md), Private lobbies). Each friend needs their own
Black Ops install and the same IW4L build.

## Maps

The 14 base maps of the install: `mp_array`, `mp_cairo`, `mp_cosmodrome`,
`mp_cracked`, `mp_crisis`, `mp_duga`, `mp_firingrange`, `mp_hanoi`,
`mp_havoc`, `mp_mountain`, `mp_nuked`, `mp_radiation`, `mp_russianbase`,
`mp_villa`. Their state per map is in [`PROGRESS.md`](PROGRESS.md).

## Not there yet

Parts of the multiplayer HUD (#651: the killcam and hardcore-list menus
paint all the time, the compass is not drawn), client scripts (#645), the
class menu's paint,
create-a-class (#650), killstreaks (#656), bots (#652), the objective and
round gametypes (#653, #654) — see the issues labelled `game: Black Ops 1`,
`mode: Multiplayer`.
