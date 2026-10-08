# BO2 Zombies survival

Select Black Ops II > Zombies > a map > Start Match. This is an early survival
build, not retail-complete Zombies. Only your installed map assets are used.
For direct launch, set `IW4L_GAMETYPE=zclassic` and run
`scripts/play.ps1 map t6:zm_nuked`. Zombies maps require `zclassic`.

Host rules live in `sim/script/host/t6_zombies.rs`: NPC entities, collision-aware
path-node pursuit, rounds, points, survivor inventory and nearby use purchases.
NPCs do not consume player slots. Their models and native animation clips load
through the T6 asset lane; world entities and HUD use existing replication.

Survivors start with the native M1911 and 500 points. Hits and kills earn points.
Use at authored wall-buy locations to buy a weapon or refill owned ammunition.
The mystery box charges for a roll; wait for it, then use again to take the gun.
Two weapons are retained; another purchase replaces the held weapon.
Pack-a-Punch resolves an available native upgraded variant of the held weapon.
Missing or unsupported weapons are refused without charging points.

Jugger-Nog and Speed Cola use health/reload effects. Quick Revive supports solo
recovery for 500 points, at most three purchases, and faster teammate revival.
Other perks remain Work in Progress.
Co-op teammates can hold use near a downed player to revive them.
The current co-op path uses separate clients; split-screen is not implemented.

| Map | Zone |
|---|---|
| TranZit | `t6:zm_transit` |
| Nuketown Zombies | `t6:zm_nuked` |
| Die Rise | `t6:zm_highrise` |
| Mob of the Dead | `t6:zm_prison` |
| Buried | `t6:zm_buried` |
| Origins | `t6:zm_tomb` |

Map geometry, collision and player starts were prepared for all six maps.
This does not establish complete survival compatibility on all six.
Native Nuketown runs verified walking body/head models, bullet and knife damage,
points, round-two progression, wall buys, insufficient-funds refusal, mystery-box
spin/pickup, downing and held-use teammate revival.
Navigation, spawn distribution and round scaling are initial implementations.
Window entry, barrier repair, map power, doors, quests, special enemies, buildables,
transport, perk arrival and original HUD fidelity remain unfinished.
Town, Farm, Bus Depot, Grief and Turned need submode/location handling.
