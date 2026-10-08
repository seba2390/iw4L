# BO2 Zombies content

Zombies gameplay is work in progress. The launcher keeps its Zombies entry marked
Work in Progress. The runtime does not yet implement waves, zombie AI, points,
doors, barrier repair, wall purchases, the mystery box, perks, power,
Pack-a-Punch, revival or map-specific quests and transport.

The T6 asset loader selects Zombies common, patch, UI and language zones for
`zm_*` maps. Weapons are prepared from the selected map rather than `common_mp`;
the cache includes the map identity. Native `_zm` weapon names and upgraded
variants retain their identities. Missing dependencies remain explicit refusals.
Capturing an upgraded weapon does not implement its Pack-a-Punch interaction.

Main map content:

| Map | Zone |
|---|---|
| TranZit | `t6:zm_transit` |
| Nuketown Zombies | `t6:zm_nuked` |
| Die Rise | `t6:zm_highrise` |
| Mob of the Dead | `t6:zm_prison` |
| Buried | `t6:zm_buried` |
| Origins | `t6:zm_tomb` |

Map preparation has been checked for world geometry, collision and authored
player starts. This does not verify every visual, weapon, submode or interaction.
Town, Farm, Bus Depot, Grief and Turned need explicit submode/location handling;
TranZit streaming and scripted transport are also unfinished.

For a developer content preview, use `scripts/play.ps1 map t6:zm_nuked` with
`IW4L_GAMES` pointing at the installed games. This runs the existing T6 FFA
runtime over the Zombies content; it is not a Zombies match.
