# BO2 Zombies survival

Select Black Ops II > Zombies > a map > Start Match. This remains an early
survival runtime using your installed assets, not complete BO2 Zombies.
Direct launch: `IW4L_GAMETYPE=zclassic`, then `scripts/play.ps1 map t6:zm_nuked`.

Host rules live in `sim/script/host/t6_zombies.rs`. NPCs use authored zombie
spawn sites and initial player starts, collision-tested path nodes, staggered
replanning, crowd separation, native rise/walk/run/attack clips and timed melee.
Unsupported vertical spawn transitions are rejected. Wooden windows use authored
outside/inside endpoints, board tearing, queued entry and nearby native boards.
Hold Use on the inside to rebuild; repairs earn bounded points each round.
Authored priced doors remove their linked models/collision and reconnect routes.

Survivors start with M1911 (Origins: Mauser) and 500 points. Use nearby
wall buys for weapons/ammo. Hits/kills earn points. The box has owner-only pickup.
Two guns are retained; Mule Kick permits three and removes the extra gun on downing.
Pack-a-Punch takes an available native upgraded variant through a five-second
hand-in and owner-only pickup. Unsupported variants are refused without charging.
Attachment rerolls and full machine/access behavior remain incomplete.

Jugger-Nog, Speed Cola and Quick Revive have health/reload/revival effects.
Solo Quick Revive costs 500, supports recovery, and allows three purchases.
Basic additions: Double Tap doubles bullet damage to zombies; Stamin-Up grants
unlimited sprint; Deadshot improves hip accuracy; PhD blocks explosion/fall damage.
Their full fire-rate, speed, aim-assist and dive effects remain incomplete.
Electric Cherry, Vulture Aid and Who's Who effects remain Work in Progress.
Native perk-machine models and owned HUD icons load from the installation.
Power gates purchases; generic authored switches work on Die Rise and Buried.
Origins' six generators use paid proximity capture/refunds and local machine power;
all six gate Pack-a-Punch. Capture attackers/recapture, TranZit assembly and Mob afterlife are unfinished.
Co-op supports held-use teammate revival through separate clients; no split-screen.

| Map | Zone |
|---|---|
| TranZit | `t6:zm_transit` |
| Nuketown Zombies | `t6:zm_nuked` |
| Die Rise | `t6:zm_highrise` |
| Mob of the Dead | `t6:zm_prison` |
| Buried | `t6:zm_buried` |
| Origins | `t6:zm_tomb` |

All six have prepared geometry/collision/assets; complete gameplay is not established.
Native Nuketown verified combat, points, rounds, wall buys, box pickup and a priced
house door. TranZit verified Depot rendering, tearing/entry, repairs and solo revival.
Die Rise/Buried verified power switches; Origins verified Mauser spawn, six generators and shared shovel pickups.
Pack-a-Punch purchases and additional perk effects need broader native verification.
Transport, buildables, special enemies, quests, scripted events, Nuketown perk arrival,
map-specific progression, traversal and full round scaling remain unfinished.
Town/Farm/Bus Depot variants, Grief and Turned need submode/location handling.
