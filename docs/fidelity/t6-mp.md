# Black Ops 2 multiplayer — fidelity ledger

The multiplayer half of Black Ops 2's ledger; zombies and the shared engine
rules are in [`t6.md`](t6.md). Same rule: the port reproduces the original
game from its own data (the `*_mp` zones, the factions, `ui_mp`, `t6mp.exe`),
with no Modern Warfare 2 or Black Ops stand-ins, no guessed constants and no
stubs that pretend to work. An item is closed only when the original
behaviour is in place; it then moves to **Closed** with the pull request that
fixed it.

`unknown!` ids under `t6.` may be listed here or in `t6.md`;
`cargo xtask boundary` reads both.

The goal is a private match between friends who each own the game
([`PROGRESS.md`](../PROGRESS.md), Black Ops II — Multiplayer). Ranked play,
League Play, public matchmaking, CoD TV and the theater's file share are out
of scope: a private match unlocks every item and keeps no progression, as in
Black Ops 2.

Today Free for All and Team Deathmatch run on rules IW4L wrote
([`T6.md`](../T6.md)), not on Black Ops 2's compiled multiplayer scripts. The
scripts (`patch_mp`, the map zones) call about 810 of the builtins Black Ops
2's server lists; about 220 of those have no native yet (counted against
`t6zm.exe`'s builtin list, which `game_t6` carries; `t6mp.exe`'s own list is
not read yet).

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.
Each open section names the issue that works it.

## Waiting on the maintainer

- [ ] **G** `maps/mp/gametypes/_globallogic.gsc` is in no zone of the
      install: every other multiplayer gametype module is in `patch_mp`, but
      this one comes with the patch zone the game downloads at launch
      (`ffotd_*_mp`), as the zombies one does (`t6.scripts.ffotd`). It has to
      be captured from the owner's own running `t6mp.exe` (#676).
- [ ] **G** Revolution, Uprising and Apocalypse maps are not in the owner's
      install; the 14 base maps, Nuketown 2025 (`dlc0_load_mp`) and
      Vengeance's Detour, Cove, Rush and Uplink (`dlc3_load_mp`) can be
      verified.

## Maps and assets (#675, #703)

- [ ] **G** A multiplayer map loads on a zone set IW4L picked; Black Ops 2's
      own order (`code_pre_gfx_mp`, `code_post_gfx_mp`, `common_mp`,
      `common_patch_mp`, `patch_mp`, `ui_mp`, `patch_ui_mp`, the map, its
      `en_` twins) is not checked against `t6mp.exe` (#675).
- [ ] **V** Team characters do not come from the map's factions
      (`faction_*_mp` and their `_sand`, `_wet` and `_snow` variants, picked by
      the map's team set) (#675).
- [ ] **G** Destructibles (`_destructible.gsc`), interactive objects and the
      maps' moving parts (Express's train, Cargo's crane) have not run;
      `setdamagestage` is not implemented (#703).

## Game mode and flow (#677, #682, #701)

- [ ] **G** `t6.scripts.gametypes` (`GameScripts::program`): Black Ops 2
      multiplayer modes other than Free for All (`dm`) and Team Deathmatch
      (`war`) have no rules, so those matches are refused at load.
      *Needs:* Black Ops 2's own gametype scripts (#677, #683–#686).
- [ ] **G** Free for All / Team Deathmatch match flow is IW4L's
      (`sim/src/script/host/t6_gametype.rs`): warmup ends at once, 2 s respawn
      delay, score/time-limit end, restart 100 ticks after the end, Use skips
      the killcam, team balance and friendly-fire block. Score-limit fallbacks
      30 (`dm`) / 75 (`war`) when `scr_<gametype>_scorelimit` is absent
      (`game_t6`, `ModeRules::default_score_limit`) (#682).
- [ ] **G** The match config is not read: `patch_mp` carries
      `default_xboxlive.cfg`, `default_mp_gametypes.cfg`, `default_private.cfg`
      (run last, for a private match) and `mp/gamesettings_<gametype>.cfg`, but
      no config dvars are set; the score limit falls back to the values of the
      match flow above (#677).
- [ ] **G** The map's startup is IW4L-generated (`game_t6/src/startup.rs`:
      a `maps/mp/<map>::main` run on IW4L's VM), not the map's own
      `maps/mp/<map>.gsc`, `_load.gsc` and `_callbacksetup.gsc` (#677).
- [ ] **G** Spawn selection is IW4L's (decoded map spawns, ground checks,
      distance from live players), not `_spawning.gsc`'s influencers (#682).
- [ ] **G** The killcam is IW4L's archived killcam; `_killcam.gsc`'s
      killcam, final killcam and `_spectating.gsc` have not run (#701).
- [ ] **G** The objective builtins (`objective_add` …
      `objective_setinvisibletoall`) and `_gameobjects.gsc` have not run
      (#683, #684).

## Gametypes (#683, #684, #685, #686)

- [ ] **G** Domination (`dom`), Kill Confirmed (`conf`), Hardpoint (`koth`),
      Headquarters (`hq`), Capture the Flag (`ctf`) and One Flag (`oneflag`)
      (#683).
- [ ] **G** Search and Destroy (`sd`) and Demolition (`dem`) (#684).
- [ ] **G** Party games: Gun Game (`gun`), One in the Chamber (`oic`),
      Sharpshooter (`shrp`), Sticks and Stones (`sas`) (#685).
- [ ] **G** Multi-Team (`level.multiteam`: `teamCount` above 2, up to six
      teams, `faction_multiteam_*_mp`) (#686).
- [ ] **I** `mp/gamesettings_*.cfg` also ships `bwars`, `hack`, `hun`,
      `pur`, `res`, `sab`, `tag` and `tdef`, with no gametype script in the
      install; whether any is offered by the private-match menus is not
      checked (#698).

## Player, movement and weapons (#679, #680, #681, #687)

- [ ] **G** The multiplayer player moves by `movement_t6` with the zombies
      values; `t6mp.exe`'s multiplayer movement dvars are not read (#679).
- [ ] **G** The initial loadout picks runnable bullet weapons; unresolved
      attachments are refused (`giveweaponnextattachment`,
      `getweaponattachments`, `weaponisdualwield` are missing) (#680).
- [ ] **G** Lethals, tacticals and equipment (`_weaponobjects.gsc` and each
      item's script) have not run; frag projectiles replicate, their scale
      and lighting unverified (#681).
- [ ] **G** Pick 10 create-a-class (`getloadoutitem`, `getloadoutweapon`,
      `getloadoutperks`, `getloadoutitemref`), perks and wildcards are not
      implemented; IW4L's five-class editor picks a primary, secondary,
      lethal, tactical and one attachment (#687).
- [ ] **G** `t6.weapons.grenade_suicide` (`PlayerMovement::server_events`):
      with grenade suicide on (`enablegrenadesuicide`, the engine's start), a
      grenade held past its fuse does not kill its thrower outright, credited
      to whoever threw it back. Unreachable on zombies: `_zm`'s
      `post_all_players_connected` calls `disablegrenadesuicide` as soon as
      `start_zombie_round_logic` is set, before the players are unfrozen and
      while their lethal grenades still have empty clips. *Needs:* Black Ops
      2's player death, which that rule calls without the damage rules
      (#681).
- [ ] **G** `t6.weapons.gas` (`PlayerMovement::server_events`): a gas weapon's
      flames are not fired. Unreachable on zombies: a scan of the zombies zones
      (`common_zm`, the three maps, their Classic and patch zones) finds no
      weapon of the gas type (4) or class (9). *Needs:* Black Ops 2's flame
      system (#680).

## Scorestreaks (#688–#695)

- [ ] **G** Scorestreak earning, the dpad and `_killstreakrules.gsc` have
      not run; Death Machine and War Machine (`_killstreak_weapons.gsc`)
      (#688).
- [ ] **G** `spawnplane`, `setteamspyplane`, `setteamsatellite` are not
      implemented: UAV, Counter-UAV, Orbital VSAT (#689).
- [ ] **G** Location selection (`beginlocationcomlinkselection`,
      `beginlocationmortarselection`), `launchbomb`, `linktomissile`:
      Hellstorm Missile, Lightning Strike, Warthog (#690).
- [ ] **G** Helicopters (`spawnhelicopter`, `sethoverparams`,
      `setheliheightpatchenabled`, …): Care Package, Stealth Chopper, Escort
      Drone, VTOL Warship, Lodestar (#691).
- [ ] **G** Vehicles (`spawnvehicle`, `usevehicle`, `makevehicleusable`,
      `finishvehicledamage`, …): RC-XD, Dragonfire, A.G.R. (#692).
- [ ] **G** Carried and remote turrets (`iscarryingturret`,
      `maketurretusable`, `remotecontrolturret`, …): Guardian, Sentry Gun
      (#693).
- [ ] **G** Missile drones and targets (`missile_settarget`, `target_set`,
      …): Hunter Killer, Swarm (#694).
- [ ] **G** EMP Systems and the K9 Unit's dogs (#695).

## Client scripts (#678)

- [ ] **V** The multiplayer client scripts (`clientscripts/mp/*.csc`) do not
      run; zombies runs its own on the client VM (#678).

## HUD and menus (#697, #698)

- [ ] **V** The multiplayer HUD is IW4L's bridge (`ui/src/t6_hud.rs`, the
      open part of `t6.hud.code_hud` in [`t6.md`](t6.md)) with Black Ops 2's
      Compass, Scoreboard and RewardSelection widgets; the full
      `HUD_FirstSnapshot_Multiplayer` constructor (`t6.ui.bootstrap`),
      compass contacts and objectives (`t6.ui.render`), score events and
      medals have not run (#697).
- [ ] **V** With `IW4L_T6_LUI_HUD`, a multiplayer match runs Black Ops 2's
      own HUD constructor (`LUI.createMenu.HUD`, `HUD_FirstSnapshot_Multiplayer`)
      in a multiplayer session (the zombies session bit clear as
      `t6mp.exe` starts); offline it builds and updates for 60 frames with
      no fault and draws the weapon panel and the game timer. Not checked on
      screen, so it is not the default. `setupIngameTalkers` draws nothing;
      the objective slots the UI reads (`Engine.GetObjectiveName`: an
      empty slot gives nothing) are not filled from the snapshot (#697).
- [ ] **V** Black Ops 2's multiplayer front end (`T6.CoDRoot`, `T6.main`)
      runs offline in a multiplayer session: the main menu (ONLINE, OPTIONS,
      CAMPAIGN, ZOMBIES, QUIT) with `mus_mp_frontend`; ONLINE shows the
      reset-classes notice a new profile gets (its stats from
      `ddl_mp/stats.ddl`; arrays indexed by enum names), then opens the main
      lobby as far as `checkclasses.lua`, which stops at
      `UIExpression.GetDefaultClassSlot` (`t6.ui.mp_default_classes`: the
      default classes are compiled into `t6mp.exe`'s
      `bg_unlockable_items_mp`, which is not read). The zombies front end's
      launch-patch module has a multiplayer twin in the uncaptured
      multiplayer launch patch, which is not run (`t6.ui.mp_ffotd`). Nothing
      in the library opens it yet (#698).
- [ ] **I** `t6.ui.live` (the online service IW4L stands for): signed in to
      Demonware with multiplayer privileges, the stats fetched
      (`AreStatsFetched`), nothing to promote (`ShouldShowDSPPromotion`,
      `ShouldShowGhostUpsellPopup`, `ShouldShowSPReminder`, the Elite
      registration, welcome and marketing popups), no Elite membership, no
      chat restriction and no probation; a new profile has no new emblems or
      create-a-class items (#698).
- [ ] **G** `t6.ui.mp_progression`: `Engine.GetChallengeInfoForImages` lists no
      challenge (ranked progression is out of scope); with a new profile's
      stats Black Ops 2 writes nothing either, but a profile with challenge
      progress would differ (#698).
- [ ] **V** Multiplayer create-a-class and pause menus are IW4L's layout over
      Black Ops 2 art (`ui/src/t6_menu.rs`; Zombies runs Black Ops 2's own LUI
      front end and pause menu in `game_t6::ui`). Black Ops 2's multiplayer front
      end and Private Match lobby (LUI) have not run; the multiplayer reward
      APIs fail explicitly (`t6.ui.engine_api`) (#698).

## Bots (#696)

- [ ] **G** `addtestclient` and the bot builtins (`addgoal`, `getgoal`,
      `atgoal`, `pressattackbutton`, `botsetdefaultclass`, `getthreats`, …)
      and `maps/mp/bots/_bot*.gsc` have not run (#696).

## Audio (#702)

- [ ] **A** Leader dialog, match music, battle chatter and faction voices
      (`_globallogic_audio.gsc`, `_battlechatter_mp.gsc`, `_music.gsc`) have
      not run; optional missing aliases admit the match as degraded (#702).

## Networking and private matches (#699, #700)

- [ ] **G** No private match on Black Ops 2's scripts: the host's **End
      Match** returns everyone to IW4L's lobby; members' classes and the
      host's game settings do not reach the scripts (#699).
- [ ] **G** No two-machine multiplayer test of Black Ops 2's own rules;
      prediction and lag compensation are not measured (#700).

## Closed

- [x] **G** Hosted lobbies are unlisted and joined by invite code, and the
      host can kick (#663, shared by every game).
