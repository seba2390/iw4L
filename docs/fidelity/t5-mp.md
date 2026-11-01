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
- [ ] **G** `t5.mp.second_chance` (`ModeRules::downed`): going down
      (Second Chance) does not drop a deployed weapon or reset the weapon
      state as Black Ops' damage does, and the 500 ms timer it starts is
      taken as the runtime's damage immunity; its stance animation event
      is the zombies movement's (#650).
- [ ] **I** The engine dvars a multiplayer match starts with
      (`MULTIPLAYER_ENGINE_DVARS`: `zombiemode` 0, `ui_multiplayer` 1,
      `systemlink` 0, `onlinegame` 0, `cg_usingClientScripts` 1,
      `cg_mature` 1, `cg_blood` 1; a hosted match adds `onlinegame` 1 and
      `xblive_privatematch` 1) are taken from the zombies list and the
      menus' rows; `BlackOpsMP.exe`'s registrations are not read yet.
      Before them the match runs the `set` lines of the configs Black
      Ops' Private Match entry runs (`default_xboxlive.cfg`, which runs
      `default_mp_gametypes.cfg`, `dvar_defaults.cfg`,
      `default_private.cfg`); their `reset`, `restoreDvars`,
      `resetCustomGametype` and `if ( … )` lines (client stats, menus)
      are not run, and the front end itself is #664.
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
- [ ] **G** A weapon dropped by `dropitem` goes from the player's middle to
      `tag_weapon_right` on their model as it stands unanimated: the server
      keeps no animated pose of a player to read the tag from, and it takes the
      player's yaw alone for angles. The held weapon's options (camo) stay
      behind; the item's model is the plain world model. `dropitem`'s fallback
      for a name that is an item but no weapon is not ported: nothing in the
      scripts passes one.
- [ ] **G** An item's ground: an item resting on something that later moves
      does not fall as BlackOpsMP.exe's item frame has it fall; the item's frame
      keeps only whether it rests. A smoke or flash class weapon on the ground
      skips Black Ops' test of the player's secondary offhand type (no script
      drops one). `GAME_CANT_GET_PRIMARY_WEAP_MESSAGE` is not printed when a use
      cannot take a weapon, and the pickup's sound comes from the runtime's
      pickup events, whose client side is not compared with BlackOpsMP.exe's
      events 0xb and 0xc.
- [ ] **G** `t5.mp.shared_ammo_cap`: a weapon with a `sharedAmmoCap` on the
      ground logs this gap and the player's ammo room ignores the cap.
- [ ] **V** The use hint of a weapon on the ground (`weapon + 7` in the cursor
      hint) was not seen on screen in a run; the pick up itself was.
- [ ] **V** `t5.csc.total_ammo`: the client's `gettotalammo` answers the
      weapon's stock and clip; Black Ops sums the clips of every held weapon
      that shares the weapon's ammo cap instead, which is not applied.
- [ ] **G** Lethal, tactical and equipment items (`_weaponobjects.gsc`):
      Semtex, tomahawk, concussion, Willy Pete, Nova gas, decoy, C4,
      tactical insertion, camera spike, jammer, motion sensor;
      `makegrenadedud` is refused.
- [ ] **G** `t5.mp.perk_effects`: the perks Black Ops' multiplayer
      code tests that have no effect here yet (setting one logs the gap):
      `_twogrenades` (a second grenade picked up);
      `_grenadepulldeath`, `_pin_back`, `_loudenemies`,
      `_noname`, `_quieter` (footsteps the AI hears, and no AI hears here);
      its client's `_reconnaissance`, `_showenemyequipment`,
      `_bulletflinch`, `_gas_mask`. `specialty_fastads` divides the
      aim-down-sight lerp by `perk_weapAdsMultiplier` except for two
      weapons the engine looks up, which are not identified, so it is not
      applied. Lightweight's `specialty_movefaster` is tested only by
      vehicle code (#660). The scripts' own perk rules apply (#650).
- [ ] **G** Create-a-class: a player's custom classes live in Black Ops'
      stats (`getdstat("cacLoadouts", "customclassN", slot)` in a hosted
      match, `onlinegame` 1). The listen host's own profile keeps Black
      Ops' two stats buffers (`profile_stats.dat`) and its client receives
      them when it first reads one; a profile with none, or an older
      `STATS_VERSION` than `stat_version` (10), gets Black Ops' stats
      reset (`mp/stats_init.cfg`, Combat Training forced on). Open: a
      remote client's stats are never sent to the host (two-machine play
      sends none, #729), so its integer stats read 0 and `setdstat`
      writes nothing; editing a class (`ui_mp`'s create-a-class commands)
      is #729. Offline (`onlinegame` 0) the scripts read
      `GetLoadoutItemFromProfile`, which is 0 on PC as in Black Ops.
      `calcweaponoptions (class, slot)` still reads the runtime's own
      class slots for the camo.
- [ ] **I** Black Ops' host reads a player's two stats files from its
      online storage, and the reset's upload fills them; which client
      buffer the reset's upload lands in on the server's side is not
      read, so a fresh profile's server buffer starts empty and the
      reset's writes (all to the client buffer, Combat Training being
      forced on) reach only the client buffer.
- [ ] **I** `unknown!t5.mp.stats_init_command`,
      `unknown!t5.mp.stats_init_condition`: the stats reset runs
      `mp/stats_init.cfg`'s and `mp/playerstats_reset.cfg`'s commands
      (`equipdefaultclass`, `equipdefaultglobalitems`, `statsetbyname`,
      `statwriteddl`, `if ( dvarbool( … ) )`); any other command or
      condition is logged and skipped. The class names
      (`setFromLocString customclassN …`) and `clanName` are the client's
      dvars and are not set. `equipdefaultclass`'s swap of a perk for its
      unlocked Pro version is not applied (the reset unlocks Pro perks
      only after it).
- [ ] **I** The bytes `setdstat` changed are recorded per client, as
      Black Ops records them, but nothing sends them back to the client
      or saves them (no stats upload in a private match here).
- [ ] **G** The custom game mode class table that
      `getcustomclassloadoutitem` and `getcustomclassmodifier` read is
      never filled (no custom game settings reach the host); it reads as
      Black Ops' zeroed table. `getpregameclass` answers undefined: no
      pregame lobby runs.

## Killstreaks (#656–#662)

- [ ] **G** The action slot button builtins
      (`actionSlotOneButtonPressed` … `Four`) read a slot as pressed for the
      server tick its press arrives in and the next: the client sends a
      press, not the slot's held state, where BlackOpsMP.exe reads bits
      18–15 of the client's current and previous button words. A slot held
      down reads as pressed for those two ticks only.
- [ ] **G** `t5.mp.target_offset`: `Target_Set`'s aim offset is read but not
      kept, and its config string (offset, materials, flags) is not sent:
      nothing that aims at a target or draws one is ported;
      `Target_IsInCircle` (the lock-on's screen test), `Target_SetShader`
      and the reticle lock-on builtins are refused or missing.
- [ ] **V** `t5.hud.compass_fire_pings`: an enemy's unsuppressed shots are
      not pinged on the compass (`compassping_enemyfiring`,
      `compassSoundPingFadeTime`); Black Ops pings them from the client's
      weapon-fire events for teammates, and for enemies while a spy plane
      or satellite is up or a client flag is clear (not identified).
- [ ] **V** `t5.hud.compass_jammers`: the local player's Jammers
      (`compass_scrambler_large`, owner draw 325) are kept from
      `addFriendlyScrambler` but not drawn; the nearest enemy Jammer
      (`setNearestEnemyScrambler`) is not kept
      (`t5.csc.nearest_enemy_scrambler`). Jamming fades the enemy icons.
- [ ] **V** The compass radar follows every connected player the runtime's
      snapshot carries; Black Ops' client sees only the players in its
      snapshot plus one other client's packed position a frame, so an
      enemy out of view pings later there than here. Players riding a
      vehicle are pinged at their own place, not hidden. The counter-spy
      plane's static (`compass_static`) is drawn as the runtime draws a HUD
      material, laid like the map; its look in Black Ops (an animated noise)
      is not compared.
- [ ] **G** `spawntimedfx` is refused (the artillery script's).
- [ ] **V** `t5.mp.strike_visuals`: the client's Rolling Thunder bombs
      (`_airstrike.csc`'s `callStrike_bomb`) stop at the client's
      `bulletTrace`, which the client scripts cannot run (no collision world
      on the client VM), and the planes' compass icons (`setCompassIcon`)
      are not set; an entity's burnt look (`startTanning`, its entity flag
      0x200000) is kept but not drawn.
- [ ] **G** `t5.weapons.missile` (multiplayer): a missile's frame (its
      trace, impact, explosion, water and glass) runs by the runtime's Black
      Ops missile think, which is the zombies executable's; BlackOpsMP.exe's
      own frame is ported only for its velocity (acceleration, curvature,
      homing, the owner steering type 6, and its life). Guided types 3, 4
      and 5 are not steered, and a player's lock-on
      target is not passed to their missile, so a Strela flies unguided; a
      `launchbomb` missile that accelerates or curves is a gap.
- [ ] **V** `t5.hud.location_selector`: the location selector's full map
      draws the map and the selector (and the mortar strikes placed); its
      player arrow, friendlies, enemies, vehicles, grid and radar effects
      (owner draws 175–190, 313, 318, 320, 326, 327, 333, 360) are not
      drawn. The full map's owner draws are moved by the menu's origin,
      which the menu's layout shows (its map then sits centred with its
      bars) but the menu paint's call was not read. Black Ops cancels the
      selector on any key not bound to `+attack`, `weapnext`, `+reload` or
      `+sprint`; the runtime reads the keys by the commands they press, so
      a movement key does not cancel it and `weapnext` does nothing.
- [ ] **G** `t5.mp.turret_weapon_type`: a turret weapon of the fourth
      weapon type does not shoot.
- [ ] **G** `t5.mp.entity_owner_bits`: `SetTeam` and `SetOwner` keep the
      entity's `team` and `owner` script fields where Black Ops writes the
      team and owning client into the entity's state; the SAM Turret's
      target filter (another team, not its owner's) reads those fields.
- [ ] **G** A carried turret's placement (`canPlayerPlaceTurret`) traces
      from 60 units over the player's origin without their lean and view
      bob, and its box and feet traces meet the world but not players or
      other entities; the carried turret stays solid (Black Ops clears its
      contents while carried and sets them back when it is put down).
- [ ] **V** A turret's entity flags (its type, carried, just put down,
      firing) are kept but not sent: the clients do not draw a carried
      turret's look. The turret's shot event on the clients (flash, shake,
      fire sound) is the zombies client's MG42 event; BlackOpsMP.exe's
      client handler is not compared. `iscarryingturret`'s client flag is
      kept but nothing ported reads it. A carried turret's gun points at
      its unfilled drop pitch (-90, barrels up): its first think's drop-pitch
      trace is `t5.turret.drop_pitch`.
- [ ] **G** A turret's search passes by no player on a turret or vehicle and
      no player riding a vehicle its owner drives (none can be here yet).
- [ ] **G** Vehicles: `setturrettargetent`, `setturrettargetvec` and
      `clearturrettarget` (vehicle turret methods), `spawnvehicle`,
      `usevehicle`, `setspeed`,
      `getspeed`, `finishvehicledamage`, `finishvehicleradiusdamage`,
      `getvehoccupants`, `getoccupantseat` and the other vehicle builtins
      are refused or missing; the client's wheel queries
      (`getwheelsurface`, `iswheelcolliding`, …) are missing.
- [ ] **G** `setvehgoalpos`'s third argument, the map's vehicle path
      (`t5.vehicle.heli_path`), errors; nothing in the multiplayer scripts
      passes it. A vehicle path's nodes blend an unset speed and look ahead by
      distance as BlackOpsMP.exe's setup appears to; the blend's float result
      is lost to the decompiler, so the formula is mine. `setswitchnode`,
      `setwaitnode`, `drivepath` and the path's `on_node` speed notifies are not
      ported; a path for a land vehicle's physics is #660.
- [ ] **V** The Chopper Gunner's view sits at `tag_gunner<n>` by the runtime's
      linked view, without the client's fake arms and the door's animation
      (`clientscripts/mp/_helicopter_player.csc`, `chopperGunnerOpenDoor`);
      `tag_flash_gunner<n>` is read with the model at rest. Its gun's arcs
      (`gunnerWeapon`'s arcs) are not applied. `firegunnerweapon`,
      `setgunnertargetent` and `setgunnertargetvec` remain refused; no
      multiplayer script reaches them.
- [ ] **G** A pilot's controls: Black Ops' `+vehiclemoveup` and
      `+vehiclemovedown` buttons have no binds in the runtime, so `+gostand`
      climbs and `+movedown` descends; `+attack` fires where its
      `+vehicleattack` would. A gamepad's right stick and the free look,
      invert and top-down camera dvars are not read. The camera rides
      `tag_player` by the runtime's linked view: it does not take the client's
      head sway, recoil or view lerp (`CG_VehicleView`), and the player's body
      stays where it stood.
- [ ] **G** A piloted helicopter's slide along what it hits keeps one surface a
      step where BlackOpsMP.exe's keeps every surface met in the frame and
      slides along their crease; the shots a pilot's gun aims along meet the
      world only, not entities.
- [ ] **G** A helicopter's turret aims and fires by BlackOpsMP.exe's rules, but
      its gun does not pose on the model: `fireweapon` takes the gun's world
      angles as body plus turret angles and swings the muzzle about
      `tag_barrel`, where Black Ops reads the turned tags, so the gun model
      does not point at what it shoots. `heliturretdogtrace` is missing,
      `stopfireweapon` does nothing (only the bomb weapon's flag is cleared in
      BlackOpsMP.exe), the client's vehicle fire event (flash, sound) is not
      sent, `t5.vehicle.heli_weapon_type` covers its bomb, gas and grenade
      weapons, and `setdamagestage` only keeps the stage in a script field
      (the client's damage effects are not drawn).
- [ ] **G** A helicopter flying to a goal has no `veh_collision` from its trace
      against the world and solid entities, its touch of other entities
      (`touch`), avoiding other helicopters (flag 0x400), and the state it packs
      for clients (engine and rotor throttle in the entity state). Rotor spin and
      exhaust are not verified: the Care Package's helicopter draws, but its
      blades were not seen turning. `finishvehicleradiusdamage` was not seen in
      a run, and the entity's script `health` field stands for BlackOpsMP.exe's
      entity health.
- [ ] **V** A flying vehicle has no compass icon yet (`compassIcon` in its
      vehicle file is not read).
- [ ] **G** The use search takes a script-made usable entity (a crate) at its
      origin when its entity has no bounds here; BlackOpsMP.exe takes the
      middle of its absolute bounds. Its turret, vehicle (`vehicle_useRadius`)
      and weapon object usables are not searched. The use hint of a crate was
      not seen on screen in a run, only the capture itself.
- [ ] **G** The client's cycle from an emptied weapon (the no-ammo event) reads
      the zombies name `suppressAmmoReserveDisplay` for the byte
      BlackOpsMP.exe's field table calls `cancelAutoHolsterWhenEmpty`; a dual
      wield's other hand and an alternate mode's return to its base weapon are
      not read in this path.
- [ ] **V** A crate may rest tilted: the runtime's `physicslaunch` body, not
      compared with Black Ops' on a Care Package.
- [ ] **V** Attack Dogs (#758): a real bullet on a dog was not fired in a run;
      the dog's posed bone collision with body hits was seen, and the damage
      path was driven through the multiplayer actor damage callback. The
      `dog_bite` shellshock was not seen, since a bite (150 damage) kills a
      100-health player outright. Dog step sounds were heard only as calls
      to `playDogstep` (sound off in the runs).
- [ ] **G** `t5.mp.dog_threat`: a dog's choice of enemy keeps the executable's
      filters and most of its score, but leaves out the accuracy-by-distance
      term, the threat bias groups, the 200 for a target actor, a target's
      need for a path node within 200 and the attacker limit dvar. It also
      does not read a client flag at offset 0x28e4 of the target, or apply the
      seated-in-a-vehicle check. Sound does not make a dog notice anyone: only
      sight does.
- [ ] **G** `t5.mp.dog_lean`: the run's lean animations never play. The lean
      field is read on the client, and what writes it on the server is not read.
- [ ] **G** `t5.mp.dog_enemy_model`: `setenemymodel` only checks its argument,
      so every viewer sees the friendly model, whatever their team. Black
      Ops' handler stores the model in the entity state, and the client draws
      it instead of the friendly one when the entity's team is none or not
      the viewer's (the followed player's when spectating), unless the
      entity is the viewer's own. Needs a model chosen per viewer at draw
      time; collision keeps the friendly model.
- [ ] **G** `t5.mp.dog_hazard`: a dog's check for a damaging volume ahead of it,
      which turns it, is not ported; nor is the lock on changing script during
      `dog_jump`.
- [ ] **G** `t5.mp.dog_restart`: a dog state's restart resets its animation's
      time at once. The executable clones the playing node and fades the old
      one out over the blend, so the restart pops. `move_walk`'s per-frame
      restart is ported as read; no script line sets `a.movement` to walk.
- [ ] **G** The dog's compass icon (`compassping_dog`, with #651) is not done.
      A flash grenade does not flash a dog: the executable starts its
      `dog_flashed` script from its own flash code, which is not read.
      `setflashbangimmunity` (no multiplayer script calls it) is not
      registered.
- [ ] **G** `heliturretdogtrace`: the handler first retries what blocked the
      trace last time, a cache this does not keep; any block answers 1, which
      the scripts only compare with 0.
- [ ] **G** A player killed while their camera is on a Valkyrie rocket: Black
      Ops' `player_die` runs the killed callback and then stops for a player
      with `EF2_REMOTE_CONTROL` (no body, no spectator state, no death
      animation event), which the runtime's death does not do.
- [ ] **G** A gamepad's right stick steering a Valkyrie rocket: the client's
      guided missile axes are read from `BlackOpsMP.exe`'s mouse path only.
- [ ] **G** A Valkyrie rocket has no health of its own in the runtime:
      `_tvguidedmissile.gsc`'s `MissileDamageWatcher` (`damage` on the
      missile) is not verified.
- [ ] **V** The Valkyrie's `sw4_2d_overlay_scanline_transparent` overlay is
      drawn as two premultiplied HUD quads (its lines baked, its noise as it
      is) where Black Ops draws one quad with their sum; the alpha of one
      darkens the other a little more than in Black Ops. The shader's
      `gameTime` is the client's time here.
- [ ] **V** While the camera is on a Valkyrie rocket Black Ops' client hides
      hud elements with `hidewhileremotecontrolling` (the objective icons of
      Search and Destroy, Demolition and `_gameobjects.gsc`) and selects its
      filter type 4; neither is ported.
- [ ] **V** The compass while a Valkyrie rocket flies: the player's arrow
      stays at the player; `BlackOpsMP.exe`'s compass was not read for this
      case.

## Client scripts (#645)

- [ ] **V** The multiplayer client scripts (`clientscripts/mp/<map>`,
      `clientscripts/mp/_callbacks`) compile against Black Ops' client
      catalog plus the multiplayer client's own function and entity method
      tables (`mp_client_catalog.rs`, 48 builtins from `BlackOpsMP.exe`)
      and run on `mp_nuked`. Not implemented yet, each faulting the thread
      that calls it: `map_material`, `set_filter_pass_*`, `set_overlay_*`
      (the scope and guided-missile filters, #645); `animateui` (menu item
      animations, #651); `setlocalradarenabled`, `setlocalradarposition`
      (the motion sensor, #649); the extra
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
      animation states paints its loaded state's colours. A menu paints
      only while Black Ops' visibility masks allow it (every bit of its
      show mask set, none outside its allowed mask), so the killcam,
      location selector, camera spike and guided missile menus no longer
      paint all the time. Open (#651): `t5.hud.mp_visibility_bits` — the
      bits not answered yet (a UI menu open, scoped aim, the spike and
      guided missile cameras, the demo player's own states, …) are taken as
      clear for a menu's masks; the menu window's static flags that hide it
      while the screen flashes or a UI menu is open are not applied.
      Hit markers, score popups, medals and the end-of-match scoreboard
      are not drawn as Black Ops draws them; `gametypename`, `isintermission`,
      `spectatingclient`, `isinguidedmissile`, `isdemoplaying` and the
      wager functions are not answered, so the items they gate do not draw.
- [ ] **V** The compass (`compass_old`) draws its map, the player's
      arrow, the enemies the radar shows (170), the spy plane's sweep line
      (332) and the counter-spy plane's static (328). Not drawn yet:
      friendlies (owner draw 158), objectives (152), the grid (317, 319),
      the killstreak icons (helicopters, planes, dogs, turrets, tanks,
      guided missiles, artillery: 146–156) and the Jammer, flicker and
      fake-fire effects (325, 312, 376). The map's frame is
      rebuilt on the client from the map's corner entities and `northyaw`
      as `_compass.gsc`'s `setupMiniMap` and `setminimap` compute it;
      Black Ops sends it (and the map's material) in a config string. The
      compass's fade (`hud_fade_compass`, 0 by default: no fade) and the
      static/jam fade are not applied, and the turned map is drawn by the
      runtime's rotated-texture quad, not compared with Black Ops'
      renderer command. With `compassRotation` 0 the arrow would turn,
      which the runtime does not draw (a gap). The server's `setminimap`
      is the shared one, which re-frames the corners it is given.
- [ ] **V** `t5.hud.compass_mounted`: a player mounted on a vehicle is not
      drawn on the compass.
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
- [ ] **V** `t5.hud.mp_visibility_bits` (`GameCodeHud::visibility_bit`):
      Black Ops' client rebuilds 64 HUD visibility bits a frame; the script
      UI flags (0–3), the match flags (4–16), demo playback (19, 20, 31,
      32: never in a match; 37 is never set), the killcam (21), the
      location selector (22, 30: never open yet, #658), the player-flag
      bits (25, 47, 48, and 28 when the player flags set it), the vehicle
      view (27), the teams (38–41), the compass (42), the objective icon
      and obituary dvars (43, 46), the scoreboard (44) and the popups' bit
      (45) are answered. The rest are not (a UI menu open 24, scoped aim
      33, the camera spike and guided missile 28/34/50, the flash 23, …),
      and bit 45 does not test the scoped sniper view that hides popups.
- [ ] **V** `t5.hud.killcam_visibility`: during a killcam the popups' bit
      (45) is not answered (#667).
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

- [x] **G** Attack Dogs (#758): `setanimstate`'s 47 states and their rules from
      BlackOpsMP.exe's table run on the tree `Dog_CreateAnims` builds; a dog
      is hurt through `maps/mp/gametypes/_callbacksetup`'s actor damage
      callback, regenerates by `_dogs.gsc`, dies by `dog_death` into a corpse
      that holds its death animation at its end (the table's corpse column),
      and `dog_leave` (`clearentitytarget`, the exit node, the delete) runs at
      the end of `scr_dog_time`. The client's `GetAnimStateCategory` reads
      the state's category from the same table, and a `dogstep_` note goes to
      `clientscripts/mp/_dogs::playDogstep` per paw, as the multiplayer client
      parses it. `specialty_nottargetedbyai` is tested by the dog's choice of
      enemy and by the Sentry Gun's search.

- [x] **G** The Chopper Gunner (`helicopter_gunner_mp`): `attachpath` and
      `startpath` run BlackOpsMP.exe's vehicle path: the chain of
      `info_vehicle_node`s with their speed, look ahead and rotate flag, the step
      toward a point ahead, the place on a segment by the planes bisecting its
      joints, `reached_node`, `reached_end_node`, and `setspeed` and `resumespeed`
      on a path. A seated gunner holding attack fires `gunnerWeapon` from
      `tag_flash_gunner<n>` at its `fireTime` (with its overheat), credited to
      them. With the Gunship and the shoot-down this completes #661.

- [x] **G** The Gunship (`helicopter_player_firstperson_mp`): `usevehicle` seats
      the player with their body left behind (`EF2_REMOTE_CONTROL`, the vehicle
      view flag), and `enter_vehicle` and `exit_vehicle` go both ways. A
      helicopter with no goal runs BlackOpsMP.exe's player move: the vehicle
      file's speed and tilt, `vehHelicopter*` dvar defaults, the `steerGraph`
      raw file, turning to the pilot's view, the height lock's altitude
      (`setheliheightlock`, `isinsideheliheightlock`), the jitter
      (`setjitterparams`), `veh_collision` and `veh_predictedcollision`; `turret_fire`
      goes to it while its seat 0 player holds attack with the gun's fire time
      run out and the gun aims where they look. `setviewclamp`, `resetviewclamp`,
      `returnplayercontrol`, `getoccupantseat`, `getseatoccupant`,
      `getvehoccupants`, `getvehicleoccupied`, `isremotecontrolling`,
      `throwbuttonpressed`, and `magicbullet` with a vehicle's owner standing for
      the vehicle are in place. A helicopter can be shot down:
      `isvehicleimmunetodamage`, `finishvehicledamage` with the
      `vehicle_damage_zone_*` shares, and `heli_damage_monitor` sends it down its
      crash path (#661).

- [x] **G** `dropitem`, `itemweaponsetammo` and picking a dropped weapon up ran
      the runtime's own item rules, and `_weapons.gsc`'s `watchPickup` faulted
      on the item's class in every match with kills. They are BlackOpsMP.exe's:
      an item's class is `weapon_<name>`; an alternate mode drops its base
      weapon; a weapon with no ammo or a clip-only one with an empty clip is
      taken with no drop; the item carries two slots (the weapon and its
      alternate, or the left hand's weapon), the reserve being what the other
      held weapons on that ammo could not hold; `itemweaponsetammo(clip, stock
      [, slot])` cuts the clip to the clip size and has Black Ops' errors. At
      `g_maxDroppedWeapons` (16) the weapon farthest from every living player
      goes (`avoidDropCleanup` ones stay). An item falls by the four drop
      speeds, stops on what it meets, lands on its side on a surface facing up
      unless `nodrop` contents refuse it, and is freed against a wall it cannot
      slide along. Its owner cannot take it for a second. A touch (36 units
      flat, 88 up, 18 down) takes the ammo the player has room for and takes
      any at all frees the item; `dropscavengeritem` packs go only to a player
      with `specialty_scavenger`, who hear `scavenger`. Use takes the weapon in
      the use search (`player_useRadius`, ranked by distance and facing,
      `g_useholdspawndelay` 500; `g_useholdtime` 250 only on the reload-use
      button), lays down the primary in hand at the item's place when two are
      held, sends `touch` and `trigger` (player, the weapon laid down), and
      raises the new weapon. Not during last stand or holding a carried
      killstreak weapon (#720).

- [x] **G** A player's rocket spawned by the zombies rules and the SAM
      Turret launched no missile. Both now go through BlackOpsMP.exe's
      missile spawn: a heat-seeker (guided type 3) climbs 0.3 off its aim
      from 10 units right of its shooter, a remote-guided one (6) starts 13
      units out; a missile that accelerates, curves or is guided is moved
      by its think (heat-seekers and type 4 fall), any other flies straight
      at the weapon's speed; a curving one takes a random sideways push of
      up to its `projectileCurvature`. Its think speeds it along its facing
      to its speed over `timeToAccelerate`, adds the push, and a homing
      missile (type 1, the SAM's) steers by the turn radius that reaches
      its target, no tighter than `maxSteeringAccel` allows, and climbs
      toward the rate that arrives with it, then faces along its velocity.
      The SAM's missile is its owner's and its turret hears `missile_fire`
      (the missile, the weapon, its lock-on target) (#738, #659).

- [x] **G** Carried turrets (`carryturret`, `stopcarryturret`,
      `canplayerplaceturret`, `setturretcarried`, `setturrettype`,
      `setturrethint`, `setscanningpitch`, `iscarryingturret`) were refused
      and `spawnturret` missing; the action slot button builtins were
      refused. They are BlackOpsMP.exe's: `spawnturret(classname, origin,
      weaponinfo)` spawns a turret with no keys and no team (at most 32);
      `setturrettype` makes it a Sentry Gun or a SAM Turret ("%s, Unknown
      turret type." printed); `setturretteam` also takes `free`;
      `carryturret` links the turret in front of the player and marks them
      carrying it (weapon flag 0x400000, which multiplayer keeps for this
      and zombies for forced animations); `canPlayerPlaceTurret` runs Black
      Ops' placement (the `turret_placement_*` dvars: a box dropped ahead
      of the eye onto ground no steeper than 0.7, then four feet traced down,
      the turret sunk and tilted to them) and keeps the "cannot place" flag
      the hint reads. The multiplayer turret think is its own: a turret not
      aiming by itself drops its gun; the Sentry Gun searches the 48-slot
      sentient pool for the next enemy (or teamless) sentient, not its
      owner, not spectating, dead or holding `specialty_nottargetedbyai`, in
      range, within its arcs at its eye led by `turret_TargetLeadBias` and in
      sight (`turret_SentryTargetTime` 1000), aims with convergence and
      fires; the SAM Turret takes the first lock-on target of another team,
      not its owner's, that turrets may acquire (`turret_KillstreakTargetTime`
      4000) and starts firing only once on its aim; with no target both sweep
      their arcs by the weapon's `scanSpeed`, `scanAccel` and
      `scanPauseTime` (`turret_scan_start` / `turret_scan_stop`). The shot
      is the turret's own, with its AI spread and no aim refinement (#659).

- [x] **G** The Napalm Strike and Rolling Thunder stopped at `SpawnPlane`
      (missing) and the napalm's ground flames (`spawnNapalmGroundFlame`,
      refused); `startTanning` and `stopBurning` faulted on a corpse. They are
      BlackOpsMP.exe's: `SpawnPlane(owner, classname, origin, [flags])`
      spawns the classname and makes it a plane whose entity state names the
      owner ("Owner entity is not a player", "unable to spawn \"%s\"
      entity"), which the client scripts see as a `plane` with that owner;
      `spawnNapalmGroundFlame(origin, fx, angles, [seconds])` turns the
      entity into the effect at the whole-unit origin for 10 seconds (or
      the time given) and frees it; `startTanning` / `stopBurning` set and
      clear any entity's burnt flag. Both strikes kill where they are called
      (#658).

- [x] **G** `launchbomb` was refused. It is BlackOpsMP.exe's: a player
      only ("LaunchBomb: entity must be a player entity"), three arguments
      ("illegal call to LaunchBomb(<weapon>, <position>, <velocity>)"), a
      known weapon ("Invalid weapon name %s") of a bullet, projectile, gas
      or bomb type ("LaunchBomb only support bullet, bomb, gas and
      projectile weapons"); the missile spawns at the position falling under
      `bg_gravity` with the velocity (cut to whole units), pointed along it,
      classname `rocket`, drawn once clear of its owner. `bg_gravity` (800)
      is now set for multiplayer scripts, which aim the mortar by it
      (#738).

- [x] **G** The location selector did not open:
      `beginLocation…Selection` were accepted and ignored. They now open
      Black Ops' own (`natives/t5_location.rs`): the selector type
      (generic, artillery, airstrike, mortar, napalm, comlink), its radius
      as a share of the map's height in fifteenths (0.15 without one; "Radius
      of location selector must be greater than zero"), and
      `precacheLocationSelector`'s 15 materials. The client's
      `fullscreenmap` shows while it is open (visibility bits 22 and 30); its
      cursor moves by `cg_mapLocationSelectionCursorSpeed`, an airstrike's
      or napalm's direction turns while `+attack` is held
      (`cg_mapLocationSelectionRotationSpeed`) and confirms on release, any
      other press confirms once, and the server gives the scripts
      `confirm_location` with the place and the yaw (twice the byte, an
      int) (#658).

- [x] **V** Black Ops' multiplayer HUD menus all painted at once: the
      menu's visibility masks (`menuDef_t` show bits and allowed bits,
      checked by the client's HUD menu paint) were not read, so the killcam,
      `fullscreenmap`, `spike_cam` and `missile_fuel` menus painted over the
      match. The client visibility bits now also answer the script UI flags
      (0–3), the match flags (`setMatchFlag`, 4–16, now Black Ops' own with
      "GScr_SetMatchFlag: Couldn't find a matching bit for flag name : %s "),
      the killcam (21), the vehicle view (27), the compass rule (42) and the
      objective icon and obituary dvars (43, 46) (#651).

- [x] **G** The spy plane, counter-spy plane and Blackbird had no effect:
      `setTeamSpyplane` / `setTeamSatellite` kept only the last call, the
      players' `hasSpyplane` / `hasSatellite` were not read, `Target_Set`
      and `Target_Remove` were refused, `setClientUIVisibilityFlag` set
      nothing and the client's scrambler methods were missing. Each team's
      levels and the players' own now give every player its radar levels
      each frame as Black Ops' frame end does; the compass sweeps for
      enemies (`compassRadarUpdateTime`, `…FastTime` at level 2; Ghost
      hides from it), fades their pings (`compassRadarPingFadeTime`), shows
      them live with their heading under a satellite
      (`compassping_enemydirectional`, `compassStaticImageUpdateTime`,
      `compassSatellitePingFadeTime`), draws the sweep line, and hides the
      enemies under an enemy counter-spy plane (`setEnemyGlobalScrambler`)
      with the map's static over it (#657).

- [x] **G** Attack Helicopter (`helicopter_comlink_mp`): called in on its
      selector, flies from the map's edge, hovers, turns its turret toward
      players (`setturrettargetent`, `turret_on_target`) and shoots them with
      `cobra_20mm_comlink_mp`, credited to its owner; `setvehweapon`,
      `fireweapon`, `missile_settarget` and `heliturretsighttrace` are Black
      Ops' (#661).

- [x] **G** Care Package (`supply_drop_mp`): the marker's throw empties the
      weapon and the client cycles back (`weapon_change` ends the marker's
      wait); `spawnhelicopter` flies `heli_supplydrop_mp` by BlackOpsMP.exe's
      goal move (speed, braking to its goal, hover, yaw, tilt, `near_goal`,
      `goal_yaw`, `goal`) from its vehicle file's defaults; the crate falls and
      a player captures it by holding use (a usable script model in the use
      search, `player_useRadius` 128). Script reads of mixed-case engine dvars
      through the fire world now find them: they were silently taking the
      caller's default (#661).

- [x] **G** Valkyrie Rockets (`m220_tow_mp`, guided type 6): the launch names
      the missile to its owner (`weapFlags` 0x200), `linkguidedmissilecamera`
      sets `EF2_REMOTE_CONTROL` and 0x40000, the owner's view follows the
      missile's interpolated origin and angles and holds its last pose until
      `unlinkguidedmissilecamera`, the client keeps its own angles still
      (`cl_preventTvGuidedSpin`) and sends the mouse's move as the command's
      look axes, the missile turns, rolls, boosts on the secondary offhand
      button (`missileTVGuided*`), moves twice a frame as `BlackOpsMP.exe`'s
      think does, burns `projectileLifetime` seconds of the held weapon's fuel
      (twice as fast boosting) and detonates on a second press of attack.
      Every multiplayer missile now explodes when `projectileLifetime` runs
      out, at most a minute; `detonate` explodes one at once;
      `ismissileinsideheightlock` tests `heli_height_lock`; the HUD draws the
      scanline overlay's shader rather than its raw line map. The player's
      own frame while steering is the executable's branch (no move, weapon
      running) instead of a gap. Its delivery by Care Package is #661.

- [x] **G** Killstreak earning and the dpad (`_hardpoints.gsc`,
      `_killstreakrules.gsc`, `_killstreak_weapons.gsc`) run on Black Ops'
      own scripts: a kill streak earns the profile's killstreak, its
      weapon sits on dpad slot 4 (`setActionSlot`, now Black Ops' own),
      the button raises it and the killstreak starts (#656).

- [x] **G** `specialty_bulletpenetration` (Hardened) and
      `specialty_shellshock` had no effect, and `shellshock` kept a
      longer running shock as zombies' does. Bullets of a player with
      Hardened go `perk_bulletPenetrationMultiplier` times as deep (and
      the exit depth with them), and Black Ops multiplayer's `shellshock`
      replaces the running shock and shortens it by
      `perk_shellShockReduction` for a player with the perk (#650).

- [x] **G** Second Chance never downed a Black Ops player: the runtime
      tested Modern Warfare 2's perk bit. A lethally hurt player now goes
      down as `BlackOpsMP.exe`'s damage decides: holding
      `specialty_pistoldeath`, out of water and not down already; the
      scripts' last stand runs, and the next lethal hit kills (#650).

- [x] **G** Black Ops multiplayer's perks never reached its movement, and
      its sprint ran on zombies' constants: `setSprintDuration` and
      `setSprintCooldown` did nothing. The scripts' perk bits now fill the
      player state's two perk words; sprint runs for the length and
      recharges after the pause the scripts set per body type, and
      `specialty_longersprint`, `_unlimitedsprint`, `_sprintrecovery`,
      `_fallheight`, `_fastmantle`, `_fastreload`, `_fastweaponswitch`,
      `_fastmeleerecovery` and `_bulletaccuracy` change it (the weapon
      timers, the client's viewmodel rates, the hip spread and the
      crosshair) as `BlackOpsMP.exe` does with its `perk_*` dvar
      defaults; a Rapid Fire weapon fires at `perk_weapRateMultiplier`.
      No perk was ever set: `scr_game_perks`, which the class script
      tests, comes from the configs Black Ops' Private Match entry runs,
      and those now run (#650).

- [x] **G** No profile kept Black Ops' stats, so a custom class spawned
      empty. The listen host's profile now keeps them, and a fresh
      profile gets Black Ops' stats reset (`mp/stats_init.cfg`): each
      custom class is filled from Black Ops' default classes as
      `mp/statsTable.csv` lists them, and the killstreaks are the
      default ones (#650).

- [x] **G** `getdstat` and `setdstat` kept a plain map of paths. They now
      walk Black Ops' own `ddl_mp/stats.ddl` (captured from
      `code_post_gfx_mp`, newest version) over each client's two
      0xa000-byte stats buffers with the executable's path rules, error
      strings, version check, ranges, buffer choice by member permission
      and Combat Training, and string and 64-bit forms (#650).

- [x] **G** `getloadoutitemfromprofile` answered from the runtime's own
      class slots; it now answers 0 for every slot as Black Ops' PC
      executable does, and `getcustomclassloadoutitem`,
      `getcustomclassmodifier` and `getpregameclass` (refused) answer as
      Black Ops does with its own item and modifier names (#650).

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
