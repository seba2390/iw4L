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

- [ ] **G** A multiplayer map (`mp_*`) takes the multiplayer zone set:
      `code_post_gfx_mp`, `common_mp`, the map, then `patch_mp` over it for
      scripts, weapons, effects and models; `code_pre_gfx_mp`,
      `code_post_gfx_mp`, `patch_mp`, `ui_mp` and `patch_ui_mp` for menus;
      the `en_` twins of `code_post_gfx_mp`, `common_mp`, `patch_mp`,
      `ui_mp` and the map for strings (#643). Black Ops' own zone load
      order (`code_pre_gfx_mp` first, the localized zones between) is not
      checked against `BlackOpsMP.exe`.
- [ ] **G** `spawncollision` (the maps' patch scripts' collision-only
      models, `collision_wall_*` / `collision_geo_*`) spawns a solid script
      model carrying the model; whether players and bullets collide with it
      as in Black Ops is not checked.
- [ ] **V** On Firing Range a jeep stands above the sandbags it should rest
      on (seen from the pre-match spectator view); not yet traced to the
      model, its destructible script or the map's entity data (#669).
- [ ] **G** Destructibles (`_destructible.gsc`: cars, barrels), interactive
      objects, elevators, rotating objects, shutters and minefields have not
      run; `setdamagestage` is not implemented.

## Game mode and flow (#644, #646, #667)

- [ ] **G** `t5.scripts.mp_gametypes` (`GameScripts::program`,
      `GameModes::mode`): Free-for-all (`dm`) and Team Deathmatch (`tdm`,
      the lobby's `war`) run Black Ops' own startup — the gametype's
      `main`, the map's `main`, then `_callbacksetup`'s
      `CodeCallback_StartGameType` (#644). Every other multiplayer gametype
      has no script program and is refused at load (`gsc: refused …
      stage=program unknown=t5.scripts.mp_gametypes`). *Needs:* the
      objective and round builtins (#653, #654, #655).
- [ ] **G** `t5.mp.second_chance` (`ModeRules::downed`): a downed
      multiplayer player (Second Chance) is briefly immune and dies when
      hurt, by the runtime's last stand. *Needs:* Black Ops' multiplayer
      last stand from `BlackOpsMP.exe` (#650).
- [ ] **I** The engine dvars a multiplayer match starts with
      (`MULTIPLAYER_ENGINE_DVARS`: `zombiemode` 0, `ui_multiplayer` 1,
      `systemlink` 0, `onlinegame` 0, `cg_usingClientScripts` 1,
      `cg_mature` 1, `cg_blood` 1; a hosted match adds `onlinegame` 1 and
      `xblive_privatematch` 1) are taken from the zombies list and the
      menus' rows; `BlackOpsMP.exe`'s registrations are not read yet.
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

- [ ] **G** The builtins the multiplayer server binds beyond the shared T5
      catalog (`mp_catalog.rs`, 187 functions and methods read from
      `BlackOpsMP.exe`'s function, player, entity, helicopter and vehicle
      tables) are named for the compiler; most have no native yet. Other
      method tables (hud elems, actors, turrets) are not read.
- [ ] **I** `allowspectateteam("localplayers", …)` (players on the
      spectator's own machine) is kept like a team; every machine here has
      one player, so it never changes whom a spectator may follow.

- [ ] **G** `map_restart` on a multiplayer map reinstalls the scripts but
      the scene never reports spawned again, so a command after it waits
      out the console's gate (#646).

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

- [ ] **G** The multiplayer player moves by `movement_t5` with
      `zombiemode` 0 and `BlackOpsMP.exe`'s own registered defaults where
      they differ from `BlackOps.exe` (`bg_fallDamageMaxHeight` 300,
      `bg_prone_yawcap` 45, `player_breath_hold_lerp` 1,
      `player_sliding_wishspeed` 4000). Dive to prone (hold the stance key
      while sprinting), ladders and mantles on a multiplayer map are not
      verified in a run yet (#461, #647); a dump shows only the shared
      player state, not the dive's flags.
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

- [ ] **V** The multiplayer client scripts (`clientscripts/mp/<map>`,
      `clientscripts/mp/_callbacks`) compile against Black Ops' client
      catalog plus the multiplayer client's own function and entity method
      tables (`mp_client_catalog.rs`, 48 builtins from `BlackOpsMP.exe`)
      and run on `mp_nuked`. Not implemented yet, each faulting the thread
      that calls it: `map_material`, `set_filter_pass_*`, `set_overlay_*`
      (the scope and guided-missile filters, #645); `animateui` (menu item
      animations, #651); `setlocalradarenabled`, `setlocalradarposition`
      and the scrambler methods (radar, jammer, #649, #657); the extra
      camera builtins (camera spike, #649); `hasperk` (#650);
      `gettotalammo` (#648); the vehicle queries (#660).
- [ ] **V** `t5.csc.saved_dvars`: `setsaveddvar` keeps the client's dvar,
      but nothing reads the ones the multiplayer scripts set (wind,
      `r_outdoorfeather`).

## HUD and menus (#651, #664)

- [ ] **V** The multiplayer HUD lists load as `CG_LoadHudMenus` does
      (`ui_mp/hud_hardcore`, `hud_spectator`, `hud`, `hud_<g_gametype>`,
      `hud_popups`, `hud_demo`, from `common_mp`), and their expressions
      run on Black Ops' own functions (`team`, `otherteam`, `timeleft`,
      `secondsascountdown`, `scoreatrank`, `select`, `min`, `max`,
      `toupper`, `getlocalclientnum`, `isvisibilitybitset`); an item with
      animation states paints its loaded state's colours. Open (#651):
      the menus of `hud_hardcore` that only a killcam, the location
      selector, the camera spike or a guided missile should show
      (`killcam`, `killcam_*_text`, `spike_cam`, `fullscreenmap`,
      `missile_fuel`) paint all the time: their menus and items carry no
      visibility expression, owner-draw flag or hidden window flag, and the
      client's rule that hides them is not found in `BlackOpsMP.exe` yet.
      The compass owner draws (`compass_old`), obituaries, hit markers,
      score popups, medals and the end-of-match scoreboard are not drawn as
      Black Ops draws them; `gametypename`, `isintermission`,
      `spectatingclient`, `isinguidedmissile`, `isdemoplaying` and the
      wager functions are not answered, so the items they gate do not draw.
- [ ] **V** `t5.hud.visibility_bits` (`GameCodeHud::visibility_bit`):
      Black Ops' client rebuilds 64 HUD visibility bits a frame; the team
      bits (38–41), the scoreboard (44), the popups' bit (45), the
      location selector (22, never open here), demo playback (20, never in
      a match) and the player-flag bits (25, 47, 48) are answered. The
      script UI flags (0–3), the vehicle, remote-view and guided-missile
      bits and the rest are not, and bit 45 does not test the scoped sniper
      view that hides popups.
- [ ] **V** `t5.hud.killcam_visibility`: during a killcam the killcam
      bits (4, 5) and the popups' bit are not answered (#667).
- [ ] **V** The scripts' class menu (`changeclass`, from `ui_mp`) opens on
      connect and its responses (`custom1`…, the presets) reach the
      scripts' `menuresponse`, but most of its items do not paint: the
      create-a-class functions (`getitemindex`, `isitemlocked`,
      `isitemnew`, `getitemname`, `getitemimage`, `getdefaultclassslot`,
      `getcacitemindex`, `getitemequippedattachment`, …) read the item
      table and the custom classes, which do not exist yet (#650).
- [ ] **V** Black Ops' multiplayer front end and Private Match lobby menus
      (`ui_mp`, `patch_ui_mp`) are not reachable from the game library.

## Bots (#652, #709)

- [ ] **G** `t5.bots.host_ai`: test clients join through Black Ops' own
      `_bot.gsc` (`addtestclient`; their team and class by `menuresponse`,
      their loadout by `bot_give_loadout`), but IW4L's host bots move, aim
      and shoot them, not Black Ops' engine bot code. The bot natives keep
      what the scripts set: `setscriptgoal`/`hasscriptgoal`, the scripted
      enemy (`getthreat` answers only that), the straight line to the goal
      for `getlookaheaddist`/`getlookaheaddir`; the host bots ignore the
      goal and `pressusebutton`/`pressattackbutton`, so bots do not pick up
      care packages or revive. Bots fire rarely and seldom land kills on
      Black Ops' weapons (#709).

## Audio (#668)

- [ ] **A** Leader dialog, match music and battle chatter
      (`_globallogic_audio.gsc`, `_battlechatter_mp.gsc`) have not run;
      `playbattlechattertoteam` is refused.

## Networking and private matches (#663, #665, #666)

- [ ] **G** No private match: the end of a match does not return members
      to the lobby, classes do not reach the host, the host's game settings
      do not reach the scripts.
- [ ] **G** No two-machine multiplayer test; prediction and lag
      compensation of Black Ops' movement and weapons are unmeasured.

## Closed

- [x] **G** Hosted lobbies were listed on the master for anyone connected
      to it: now unlisted by default, joined by invite code, with host kick
      (#663, #672).
- [x] **V** The HUD's menu-response hook turned a `changeclass` response
      `customN` into the runtime's own class slot; Black Ops' class menu now
      answers its scripts like every other menu (#651).
- [x] **G** A Black Ops weapon named like a Modern Warfare 2 one
      (`mp5k_mp`, `aug_mp`, `famas_mp`, `spas12_mp`, …) resolved to Modern
      Warfare 2's, so every class's primary spawned with no ammunition; a
      game running its own weapons now resolves its scripts' names to its
      own (#646).
- [x] **G** The host's console and menus did not set the multiplayer
      server's dvars, so a match's `scr_<gametype>_scorelimit` and
      `timelimit` could not be changed; Black Ops' host runs the server in
      its own process, as here (#646).
- [x] **G** `addtestclient` and the bot natives were refused: Black Ops'
      `_bot.gsc` now adds its bots (#652).
- [x] **G** Free-for-all and Team Deathmatch on `mp_nuked` run on Black
      Ops' own gametype scripts to the end: a kill scores, the time limit
      (FFA) and the score limit (TDM) end the match on Black Ops' end
      screen (winner, placings or team scores) and every client goes to
      intermission. Leaving intermission for the lobby is #665 (#646).
