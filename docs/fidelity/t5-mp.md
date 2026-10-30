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
      `GameModes::mode`): Free-for-all (`dm`), Team Deathmatch (`tdm`, the
      lobby's `war`), Domination (`dom`), Headquarters (`koth`), Capture
      the Flag (`ctf`), Search and Destroy (`sd`), Sabotage (`sab`) and
      Demolition (`dem`, the lobby's `dd`) run Black Ops' own startup — the
      gametype's `main`, the map's `main`, then `_callbacksetup`'s
      `CodeCallback_StartGameType` (#644, #653, #654). The wager gametypes
      have no script program and are refused at load (`gsc: refused …
      stage=program unknown=t5.scripts.mp_gametypes`) (#655).
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

- [ ] **G** An objective following an entity (`objective_onentity`) is
      drawn at the entity's current origin; Black Ops' per-frame copy of
      that origin into the objective is not located in `BlackOpsMP.exe`, so
      when it stops following (`objective_state` empty or invisible) it
      keeps the position it was last given, not the entity's last one.
      `objective_current`, `objective_setvisibletoplayer`,
      `objective_setinvisibletoplayer`, `objective_setvisibletoall`,
      `objective_setinvisibletoall`, `objective_setsize` and
      `objective_setcolor` are not implemented.
- [ ] **G** `map_restart` on a multiplayer map reinstalls the scripts but
      the scene never reports spawned again, so a command after it waits
      out the console's gate (#646).

## Gametypes (#653, #654, #655)

- [ ] **G** Free-for-all (`dm`) and Team Deathmatch (`tdm`).
- [ ] **G** Demolition's second target, overtime and the side switch are
      not verified (#654).
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
- [ ] **G** `dropitem` and dropped-weapon pickup (`_weapons.gsc`'s
      `dropWeaponForDeath`, `itemweaponsetammo`) run the runtime's own item
      rules. Black Ops' drop is read from `BlackOpsMP.exe`: an alternate
      mode drops its base weapon, a weapon with no ammo is taken without a
      drop, the launch speeds are the same four dvars with the same
      defaults (10, 10, 5, 100), and the item is then moved to the dropping
      player's weapon tag (traced from the player's centre) — the tag
      placement, the item's ammo checks (`itemweaponsetammo`'s clip capped
      at the clip size, its hand index 0–1 and error texts) and Black Ops'
      pickup are not in place (#720).
- [ ] **V** `t5.csc.total_ammo`: the client's `gettotalammo` answers the
      weapon's stock and clip; Black Ops sums the clips of every held weapon
      that shares the weapon's ammo cap instead, which is not applied.
- [ ] **G** Lethal, tactical and equipment items (`_weaponobjects.gsc`):
      Semtex, tomahawk, concussion, Willy Pete, Nova gas, decoy, C4,
      tactical insertion, camera spike, jammer, motion sensor;
      `makegrenadedud` is refused.
- [ ] **G** `t5.mp.perk_effects`: the perks Black Ops' multiplayer
      movement and weapon code tests (`specialty_bulletaccuracy`,
      `_bulletpenetration`, `_fallheight`, `_fastmantle`, `_fastreload`,
      `_fastweaponswitch`, `_grenadepulldeath`, `_longersprint`,
      `_pin_back`, `_quieter`, `_scavenger`, `_shellshock`,
      `_sprintrecovery`, `_twogrenades`, `_unlimitedsprint`, found by their
      bit tests in `BlackOpsMP.exe`; others may be read elsewhere) are kept
      but have no effect in the runtime's movement and weapons; setting one
      logs the gap. The scripts' own perk rules (`hasperk` in the gametype,
      damage and killstreak scripts) apply (#650).
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
      camera builtins (camera spike, #649); the vehicle queries (#660).
- [ ] **V** `t5.csc.saved_dvars`: `setsaveddvar` keeps the client's dvar,
      but nothing reads the ones the multiplayer scripts set (wind,
      `r_outdoorfeather`); Black Ops' refusal of a dvar that does not exist
      or lacks the saved flag is not applied (the client has no dvar
      registry with flags).
- [ ] **I** `t5.csc.server_time`: `getservertime` answers the client's
      time; Black Ops subtracts a field of the client's current snapshot
      from it, which is not identified yet.

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
      The compass owner draws (`compass_old`), hit markers, score
      popups, medals and the end-of-match scoreboard are not drawn as
      Black Ops draws them; `gametypename`, `isintermission`,
      `spectatingclient`, `isinguidedmissile`, `isdemoplaying` and the
      wager functions are not answered, so the items they gate do not draw.
- [ ] **V** `t5.hud.game_messages`: of Black Ops' three game message
      windows (`gamemessages`, `boldgamemessages`, `subtitles`) only the
      obituary lines are drawn; script `iprintln` and `iprintlnbold`,
      connect and disconnect notices and subtitles are not, and the
      channel filters (`con_gameMsgWindow%dFilter`: `gamenotify obituary`,
      `boldgame`, `subtitle`) are not read as dvars. A window keeps every
      line until it ends; Black Ops' cap on the lines a window keeps
      beyond the four it shows is not read.
- [ ] **V** `t5.hud.obituary_popups`: an obituary naming the local player
      does not open Black Ops' killed / killed-by player card
      (`playercard`, `playerkilled` in `hud_popups`: `CGAME_KILLED`,
      `CGAME_KILLEDBY`, the team-kill strings, rank icon and clan tag),
      and a Second Chance revive (`syrette_mp`) does not centre-print
      `CGAME_YOUREVIVED` / `CGAME_YOUWEREREVIVED`.
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

- [x] **V** The kill feed was Modern Warfare 2's and hidden under Black
      Ops' code HUD. Black Ops multiplayer's own now draws in
      `hud_hardcore`'s `gamemessages` window: its `obituary` builtin's
      means of death (melee, head shot, crush, falling, suicide and
      trigger hurt show a means icon, an impact kill shows the grenade
      round icon unless the weapon's impact type shows the weapon) or the
      weapon's kill icon with its ratio and flip (`hud_obit_death_suicide`
      when it has none), names in `^8` (own team, the scripts'
      `g_TeamColor_Allies`) and `^9` (enemy, `g_TeamColor_Axis`), a line
      with no attacker ending in `CGAME_QUAKE_SUICIDE`, and window 0's
      5 s, four lines, 0.25 s scroll and fade in and 0.5 s fade out (#651).

- [x] **G** `setperk`, `unsetperk`, `hasperk`, `clearperks` and
      `getperks` took the zombies' perk table and Modern Warfare 2's script
      perks, so a multiplayer perk could set a zombies perk's movement rule
      or none at all. They now keep Black Ops multiplayer's own 52-perk
      table, one bit per perk (`Unknown perk: <name>` for any other), and the
      client's `hasperk` reads any player's perks (#650).
- [x] **G** Multiplayer weapons fire, reload and aim by Black Ops' rules:
      on `mp_nuked` each of the attachment variants (reflex, ACOG, variable
      zoom, speed reloader), the underbarrel grenade launcher, flamethrower
      and Masterkey (switched to with action slot 3, as `_class.gsc` binds
      `altMode`), dual wield (Makarov, Skorpion), the explosive crossbow, the
      ballistic knife, the L96A1, SPAS, M72 LAW, China Lake and Python fire
      and reload on the game's own weapon definitions; the client's
      `gettotalammo` is implemented (#648).

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
- [x] **G** Domination, Headquarters and Capture the Flag run on Black Ops'
      own scripts (`_gameobjects.gsc`) with the objective builtins
      (`objective_add`, `objective_state`, `objective_icon`,
      `objective_position`, `objective_onentity`, `objective_team`,
      `objective_delete`): on `mp_nuked` a flag capture scores 5, a returned
      enemy flag 1 and a held HQ 30 (#653).
- [x] **G** Search and Destroy, Sabotage and Demolition run on Black Ops'
      own scripts: on `mp_nuked` the S&D bomb is picked up, planted (+500,
      45 s timer) and detonates, winning the round, and round 2 starts;
      the Sabotage bomb is picked up and planted on the enemy site and its
      detonation scores; a Demolition attacker plants at A (+100) (#654).
- [x] **G** `istouchingswept` was refused, so `_gameobjects`'
      `shouldBeReset` faulted when a dropped bomb was checked against the
      minefield and hurt triggers. It now runs Black Ops' own test: the
      non-volume entity's box (the receiver's when neither is a volume),
      stretched to the given heights and grown to at least its width in
      height, against the other; it had taken Black Ops II's, which pads the
      box by a unit and answers false when neither is a volume.
- [x] **G** The objective builtins were Modern Warfare 2's: they took a
      `"done"` and `"failed"` state, an entity as `objective_add`'s
      position, a player or `"free"` as `objective_team`'s team, reset the
      icon and position on `objective_add`, and kept following an entity
      after `objective_state` hid it. Black Ops' own now run: four states,
      whole-unit positions, `objective_add` keeping the slot's icon and
      position unless given, `objective_state` empty or invisible and
      `objective_position` ending the follow, and Black Ops' icon-name and
      team errors.
- [x] **V** The client's `setsaveddvar` took any value; it now refuses a
      localized string and an invalid dvar name and turns the value's
      double quotes to single ones, as Black Ops' client does.
- [x] **G** Every base map loads and plays a Team Deathmatch to its end
      with Black Ops' bots; the only script fault is `testspawnpoint`
      when a bot carries a tactical insertion (#649) (#670).
