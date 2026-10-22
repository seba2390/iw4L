# Black Ops 2 Zombies

Classic (`zclassic`) runs Black Ops 2's own compiled zombies scripts (server GSC
and client CSC) on every zombies map but Origins, with its own movement and
weapons (`movement_t6`). What runs and what is open: [PROGRESS.md](PROGRESS.md)
(Black Ops II — Zombies) and the ledger [fidelity/t6.md](fidelity/t6.md).

## Start

From the game library: Black Ops II → Zombies → a map → START MATCH (the page
lists the installed maps). A direct launch needs the gametype, as the default
is Free for All (`make map ZONE=t6:zm_nuked` and `scripts/play.ps1` read it too):

```bash
IW4L_GAMETYPE=zclassic cargo run --profile play -p launcher -- map t6:zm_nuked
IW4L_SOUND=off IW4L_GAMETYPE=zclassic IW4L_TEST_PLAYER=rounds cargo run --profile play \
  -p launcher -- serve t6:zm_nuked --cmds 'wait world; wait 300s; quit'   # headless
```

The test player (`IW4L_TEST_PLAYER=1|rounds|use|down|revive`) is steered by
`IW4L_TEST_USE`, `_PERK`, `_GOTO`, `_BUILD`, `_WIELD`, `_PLACE`, `_GRAB` and logs
`level.zombie_vars` with `_VARS`; all of them are in [BOTS.md](BOTS.md#test-player).

`IW4L_TIMESCALE=<1..64>` runs a headless server's game clock faster than real
time; ticks, script waits and console `wait Ns` count game time, and the log
says each minute how fast the machine kept up (a Nuketown rounds run averages
about 2x to 3x).

Every map needs `maps/mp/gametypes_zm/_globallogic`, which Black Ops 2 downloads
at launch and keeps only in memory. Without a capture of that patch zone
(`ffotd_*_zm*.ff`) in `~/.config/iw4l/captured/` or `<artifacts>/captured/` the
game's scripts are refused (`t6.scripts.ffotd`). The capture tool is not in the
repository, so a machine without a capture cannot start these maps yet: the
library's Zombies page then greys out every Black Ops 2 map that needs it
(Origins, on IW4L's rules, does not) and names the file and the folders it is
read from. Copying a capture you made from your own game into one of those
folders makes the maps startable at the next visit to the page.

## Maps

| Map | Zone | Scripts | Runs |
|---|---|---|---|
| Nuketown Zombies | `t6:zm_nuked` | Black Ops 2's | proven headless |
| TranZit | `t6:zm_transit` | Black Ops 2's | proven headless |
| Buried | `t6:zm_buried` | Black Ops 2's | proven headless |
| Die Rise | `t6:zm_highrise` | Black Ops 2's | not yet run |
| Mob of the Dead | `t6:zm_prison` | Black Ops 2's | not yet run |
| Origins | `t6:zm_tomb` | IW4L's rules | older builds only |

TranZit's Survival runs on Bus Depot, Town and Farm (Library → Zombies, or
`IW4L_GAMETYPE=zstandard IW4L_ZM_LOCATION=town|farm|transit` on the command
line); the rounds test reaches round 2 on each. Grief runs on TranZit's Town
and Farm and Buried's Borough (`IW4L_GAMETYPE=zgrief`, `IW4L_ZM_LOCATION=street`
for Borough). Add a bot once the level has started (`wait world; wait 20s;
bot add 1`): a bot is no party member, and one that connects while the level
counts its players keeps it waiting, as Black Ops 2 does. Players split over
the two teams only with `party_autoteams` 1 (`IW4L_SCRIPT_DVARS='set
party_autoteams 1'`), and a bot always lands on `allies`
(`t6.session.party_teams`). Turned runs on Buried's Borough
(`IW4L_GAMETYPE=zcleansed IW4L_ZM_LOCATION=street t6:zm_buried`): players start
as zombies and the one who takes the cure turns human. TranZit's Diner Turned
is refused, its script being in no installed zone
(`t6.scripts.zm_gametype_zone`).

## Origins (IW4L's rules)

Origins runs IW4L's survival host (`crates/sim/src/script/host/t6_zombies.rs`,
`t6_zombies/`) until it runs Black Ops 2's scripts; its rules are the ledger's
*Origins* items. Older Windows builds showed the six generators, captured for
points, gating Pack-a-Punch; digs paying points, zombies and weapons; staff
upgrades and the first quest step; and rain and snow on the round schedule.
