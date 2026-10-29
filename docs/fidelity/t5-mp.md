# Black Ops multiplayer — fidelity ledger

The multiplayer half of Black Ops' ledger; zombies and the shared engine
rules are in [`t5.md`](t5.md). Same rule: the port reproduces the original
game from its own data (the `*_mp` zones, `ui_mp`, `BlackOpsMP.exe`), with no
Modern Warfare 2 or Black Ops II stand-ins, no guessed constants and no stubs
that pretend to work. An item is closed only when the original behaviour is
in place; it then moves to **Closed** with the pull request that fixed it.

`unknown!` ids under `t5.` may be listed here or in `t5.md`;
`cargo xtask boundary` reads both.

The goal is a private match between friends who each own the game
([`PROGRESS.md`](../PROGRESS.md), Black Ops — Multiplayer). Ranked play,
public matchmaking, CoD Points, contracts and the theater's file share are out
of scope: a private match unlocks every item and keeps no progression, as in
Black Ops.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.
Each open section names the issue that works it.

## Waiting on the maintainer

- [ ] **G** DLC maps (First Strike, Escalation, Annihilation, Rezurrection)
      are not in the owner's install; only the 14 base maps can be verified.

## Maps and assets (#643, #669)

- [ ] **G** A multiplayer map (`mp_*`) is refused before its zones load:
      the match needs a gametype, and every gametype but `zom` is refused
      (`t5.scripts.mp_gametypes`). The multiplayer zone set
      (`code_pre_gfx_mp`, `code_post_gfx_mp`, `common_mp`, `ui_mp`,
      `patch_mp`, `mp_<map>`) has not been loaded in a run.
- [ ] **G** Destructibles (`_destructible.gsc`: cars, barrels), interactive
      objects, elevators, rotating objects, shutters and minefields have not
      run; `setdamagestage` is not implemented.

## Game mode and flow (#644, #646, #667)

- [ ] **G** `t5.scripts.mp_gametypes` (`GameScripts::program`,
      `GameModes::mode`): Black Ops multiplayer gametypes (everything but
      `zom`) have no script program, so a Black Ops multiplayer match is
      refused at load (`gsc: refused … stage=program
      unknown=t5.scripts.mp_gametypes`). Until that step they ran Modern
      Warfare 2's gametype scripts and natives on the Black Ops map.
      *Needs:* Black Ops' multiplayer startup (`_callbacksetup.gsc`, the
      gametype script, `maps/mp/<map>.gsc`), its builtins bound to Black Ops
      natives, and Black Ops' match phases.
- [ ] **G** Spawn selection: the spawn influencer builtins
      (`addsphereinfluencer`, `addcylinderinfluencer`, …) and
      `_spawning.gsc`'s scoring have not run.
- [ ] **G** The objective builtins (`objective_add`, `objective_delete`,
      `objective_icon`, `objective_position`, `objective_onentity`,
      `objective_team`) are not implemented.
- [ ] **G** `sendranks`, `setplayerteamrank` are not implemented;
      `ghost`, `boundswouldtelefrag`, `testspawnpoint`, `killserver`,
      `ban`, `endparty`, `endlobby`, `sethostmigrationstatus` are refused.
- [ ] **G** Killcam, final killcam and spectating (`_killcam.gsc`,
      `_spectating.gsc`) have not run.

## Gametypes (#653, #654, #655)

- [ ] **G** Free-for-all (`dm`) and Team Deathmatch (`tdm`).
- [ ] **G** Domination (`dom`), Headquarters (`koth`), Capture the Flag
      (`ctf`): `_gameobjects.gsc` has not run.
- [ ] **G** Search and Destroy (`sd`), Sabotage (`sab`), Demolition
      (`dem`): rounds, the bomb, side switch.
- [ ] **G** Wager matches: One in the Chamber (`oic`), Gun Game (`gun`),
      Sticks and Stones (`hlnd`), Sharpshooter (`shrp`);
      `getwagergametypelist` is refused.
- [ ] **I** `twar` and `sur` ship in `common_mp` but are not offered by
      Black Ops' private match; they stay refused.

## Player, movement and weapons (#647, #648, #649, #650)

- [ ] **G** Dive to prone, ladders and mantles on a multiplayer map (#461),
      fall damage and the multiplayer movement dvars' defaults.
- [ ] **G** Multiplayer weapons: attachment variants, underbarrel grenade
      launcher, flamethrower and Masterkey (`alt` weapons), dual wield,
      the ballistic knife's pickup, the crossbow's explosive bolts, weapon
      drop and pickup.
- [ ] **G** Lethal, tactical and equipment items (`_weaponobjects.gsc`):
      Semtex, tomahawk, concussion, Willy Pete, Nova gas, decoy, C4,
      tactical insertion, camera spike, jammer, motion sensor;
      `makegrenadedud` is refused.
- [ ] **G** Create-a-class: `getcustomclassloadoutitem`,
      `getcustomclassmodifier` and `getpregameclass` are refused; custom
      classes have nowhere to live; perks and Pro perks have not run.

## Killstreaks (#656–#662)

- [ ] **G** Killstreak earning and the dpad (`_hardpoints.gsc`,
      `_killstreakrules.gsc`); the action slot button builtins are refused.
- [ ] **G** `spawnplane` is not implemented: spy plane, counter-spy plane,
      Blackbird, napalm and Rolling Thunder planes.
- [ ] **G** `launchbomb`, `spawnnapalmgroundflame`, `spawntimedfx` are
      refused; the location selector has not run.
- [ ] **G** Carried turrets (`carryturret`, `stopcarryturret`,
      `canplayerplaceturret`, `setturretcarried`, `setturrettype`) are
      refused; `spawnturret`, `setturrettargetent`, `setturrettargetvec`,
      `clearturrettarget` are not implemented.
- [ ] **G** Vehicles: `spawnvehicle`, `usevehicle`, `setspeed`,
      `getspeed`, `finishvehicledamage`, `finishvehicleradiusdamage`,
      `getvehoccupants`, `getoccupantseat` and the other vehicle builtins
      are refused or missing; the client's wheel queries
      (`getwheelsurface`, `iswheelcolliding`, …) are missing.
- [ ] **G** Helicopters: `spawnhelicopter`, `setvehgoalpos`,
      `sethoverparams`, `setyawspeed`, `setmaxpitchroll`,
      `setheliheightlock` and the heli turret traces are missing or refused.
- [ ] **G** `linkguidedmissilecamera` / `unlinkguidedmissilecamera`
      (Valkyrie rockets) and `spawnactor` (attack dogs) are refused.

## Client scripts (#645)

- [ ] **V** The multiplayer client scripts (`clientscripts/mp/*.csc`) have
      not run. Missing client builtins they call: `set_filter_pass_*`,
      `set_overlay_*`, `animateui`, `setsaveddvar`,
      `setinfraredvisionset`, `setstunned`, `shellshock`,
      `setcompassicon`, `map_material`, the vehicle queries.

## HUD and menus (#651, #664)

- [ ] **V** The multiplayer HUD from `ui_mp` (compass and minimap,
      obituaries, hit markers, score popups, medals, team score, TAB
      scoreboard) has not been drawn.
- [ ] **V** Black Ops' multiplayer front end and Private Match lobby menus
      (`ui_mp`, `patch_ui_mp`) are not reachable from the game library.

## Bots (#652)

- [ ] **G** `addtestclient` and the bot builtins (`setscriptgoal`,
      `clearscriptgoal`, `hasscriptgoal`, `pressattackbutton`,
      `pressusebutton`, `throwbuttonpressed`, `getthreat`,
      `setscriptenemy`, `clearscriptenemy`, `getlookaheaddir`,
      `getlookaheaddist`) are refused.

## Audio (#668)

- [ ] **A** Leader dialog, match music and battle chatter
      (`_globallogic_audio.gsc`, `_battlechatter_mp.gsc`) have not run;
      `playbattlechattertoteam` is refused.

## Networking and private matches (#663, #665, #666)

- [ ] **G** Lobbies are listed on the master for anyone connected to it;
      there is no unlisted lobby, invite code or kick (#663).
- [ ] **G** No private match: the end of a match does not return members
      to the lobby, classes do not reach the host, the host's game settings
      do not reach the scripts.
- [ ] **G** No two-machine multiplayer test; prediction and lag
      compensation of Black Ops' movement and weapons are unmeasured.

## Closed
