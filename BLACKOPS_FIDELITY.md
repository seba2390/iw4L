# Black Ops Zombies — fidelity ledger

Rule: the Black Ops port reproduces the original game, driven by the game's own
data (zones, menus, fonts, materials, sound banks, scripts). No hand-made
substitutes, no Modern Warfare 2 stand-ins, no guessed constants, no stubs that
pretend to work. When the real behaviour cannot be recovered, the item goes to
**Waiting on the maintainer** with what is needed, and work moves on to the next
item.

This ledger lists every shortcut found in the port so far. An item is closed
only when the original behaviour is in place; it then moves to **Closed** with
the commit that fixed it. New shortcuts are not added to the code; if one is
found it is logged here first.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.

## Waiting on the maintainer

Items that cannot be closed from the game's data or scripts alone: they need a
reference capture from the original game, or a rule that lives only in the
(encrypted) executable. Agents list them here with exactly what is needed and
move on; they do not work around them.

(none yet)

## HUD, menus and fonts

- [ ] **V** Hand-drawn zombie HUD (`crates/hud/src/zombie_hud.rs`, wired in
      `scorebar.rs` and `weaponbar.rs`): points, ammo, weapon name and grenades
      with made-up positions and Modern Warfare 2 fonts. Replace with Black Ops'
      own zombie HUD menus.
      *Check:* (a) the menus of `ui/hud.txt`, `ui/hud_sp.txt`, `ui/hud_zombie.txt`
      and `ui/hud_coop.txt` are decoded from `code_post_gfx` + `patch` (log
      `t5 hud menus:`); (c) screenshot at round 1 after spawn (`move 1012 -1197 0
      90 0 &`) shows `weaponinfo_zombie`/`dpad_zombie` drawn from the menus;
      `zombie_hud.rs` is deleted.
- [ ] **V** Hand-drawn zombie scoreboard (`scoreboard.rs` `zombie_board`) with
      English literals; the Modern Warfare 2 scoreboard menu is skipped in
      `menus/mod.rs`. Replace with Black Ops' scoreboard menu and strings.
      *Check:* (a) find the Black Ops zombies scoreboard menu or strings in the
      zones; otherwise park it.
- [ ] **V** Black Ops menus are skipped while loading (`fastfile_t5/src/load/menu.rs`);
      Black Ops fonts are not loaded. All HUD text (script hud elems, use hints,
      killfeed, overhead names) draws with Modern Warfare 2 fonts.
      *Check:* (a) log `t5 menus code_post_gfx: 168 menus, 11 lists`, `t5 menus
      patch: 66 menus`, `t5 menus en_code_post_gfx: ... 6 fonts`; (c) script
      hud elems and HUD menus draw with `fonts/*` from `en_code_post_gfx`.
- [ ] **V** `t5_font_scale` in `hud_elems.rs`: script font scales above 4.6 are
      multiplied by 0.25 — a guess. Needs Black Ops' real hud elem font rule.
- [ ] **V** `hud_elems.rs`: in zombies, script material elems skip the
      Modern Warfare 2 font-height floor — unverified workaround.
- [ ] **V** The Modern Warfare 2 code blood overlay (`hud/src/blood.rs`) draws in
      zombies on top of Black Ops' scripted damage overlay (`_gameskill`).
- [ ] **V** Modern Warfare 2 HUD pieces run in zombies unchecked: killfeed,
      reticle, overhead names, weapon-name popup, mantle/breath hints, splash,
      playercard. Each must match what Black Ops zombies shows.
- [ ] **V** HUD materials decode only their first texture, no blend state; which
      materials are HUD is guessed by name prefix (`common_walks.rs`).
- [ ] **V** `setwaypoint` ignores Black Ops' material argument (`host/hud.rs`).
- [ ] **V** `setclientuivisibilityflag` recorded, never used (`natives/t5.rs`).
- [ ] **V** `setrevivehintstring`, `sethintstringforperk` are no-ops.
- [ ] **V** Main menu and lobby are Modern Warfare 2's; the zombies mode and Kino
      are not offered, a solo game needs console commands.

## Rendering and visuals

- [ ] **V** Black Ops vision sets are never loaded (`assets/src/lane/t5.rs` drops
      them): Kino's colour grading, last stand and death visions are missing.
- [ ] **V** `visionsetlaststand` is a no-op.
- [ ] **V** Zone data dropped for T5 maps: dyn ents, fx models, glass, dynamic light.
- [ ] **V** No ragdoll: `startragdoll`/`isragdoll` accepted without effect, dead
      zombies hold their death pose; `launchragdoll` errors on actors.
- [ ] **V** `gib` is a no-op: no thrown body parts.
- [ ] **V** No client scripts (CSC) run: `setclientflag` and `clientsyssetstate`
      are stored but unused — board/riser effects, last stand client effects,
      music states, level notifies (Kino power, lights) are missing.
- [ ] **V** No-ops: `haseyes` (eye glow), `startfadingblur`, `setblur`,
      `settransported`, `vibrate`, `useweaponhidetags`, `setenemymodel`.
- [ ] **V** Recorded but unused: `visionsetlerpratio`, `setplayerrenderoptions`,
      `starttanning`, `setvolfog`, `playfxontagforclients`; `glassradiusdamage` no-op.
- [ ] **V** Not implemented: `setelectrified`, `activateclientexploder`,
      `deactivateclientexploder`, `setdoublevision`, `setculldist`,
      `start3dcinematic`/`pause3dcinematic`/`stop3dcinematic`/
      `getcinematictimeremaining` (projector), `dontinterpolate`,
      `hideviewmodel`/`showviewmodel`, `bloodimpact`, `playweapondeatheffects`.
- [ ] **V** Script hud elem colours: unverified whether Black Ops treats them as
      linear (the round chalk looks darker than the original).

## Audio

- [ ] **A/G** `PlaySound(alias, notify)` raises its notify after a fixed 2 s
      (`SOUND_DONE_MS` in `natives/engine.rs`) instead of the alias length.
- [ ] **A** `playsound` plays once at the entity's origin and does not follow it.
- [ ] **A** Alias lookup tries the Modern Warfare 2 name before the Black Ops one
      (`sim/src/world.rs`); Modern Warfare 2 / MW3 / Black Ops MP banks form the
      base bank (`asset_audio/src/sound_load.rs`).
- [ ] **A** Footstep aliases (`fly_*`) and `uin_transition_zombie_theater` missing.
- [ ] **A** Music states dropped (`_music.gsc` → `clientsyssetstate`).
- [ ] **A** Not implemented: `stopsound`, `setsoundblend`.

## AI and actors

- [ ] **G** `isai` always returns 0 (breaks power-up, spawner, dog and animscript paths).
- [ ] **G** `getaispeciesarray` ignores the species.
- [ ] **G/V** Animscript selection is a heuristic in `actor_brain.rs`; pain is
      never chosen.
- [ ] **G** Guessed constants: melee range 64, eye height 60, melee reach 96,
      arc 60, damage 60, turn rate 360, actor box (`actor_brain.rs`,
      `actor_nav.rs`); `ai_*` dvars incl. `ai_meleeRange` in `session/src/match_apply.rs`.
- [ ] **G** Navigation is A* over path nodes only: no actor-vs-world collision,
      no avoidance, no badplaces; `connectpaths`/`disconnectpaths` are no-ops.
- [ ] **G** `cansee` traces from feet + 60 with no field of view.
- [ ] **G** Zombie melee hits carry no attacker.
- [ ] **G** Actor damage: attacker passed as inflictor; `modelIndex`/`timeOffset`
      zero; the `damage` notify has empty tag and part names.
- [ ] **G** Hit locations guessed from bone names (`actors.rs` `hit_location`).
- [ ] **G** `credit_kill` (`actors.rs`): the engine counts kills/headshots itself;
      `_gameskill` may count them too (double count). Verify against the scripts.
- [ ] **G** `dospawn` never fails.
- [ ] **G** `getallnodes` returns an empty array: `setup_traversals` never runs.
- [ ] **G** No-ops / soft failures: `reacquire*`, `flagenemyunattackable`, AI event
      listeners, threat bias, engagement distances, `pushplayer`, `traversemode`,
      `setailimit`, `watersimenable`, `setexploderid`.
- [ ] **G** Not implemented: `animcustom`, `badplace_cylinder`, `badplace_arc`.
- [ ] **G** No hellhounds, crawlers; Kino traversals and rise-from-ground unverified.

## Player, movement and weapons

- [ ] **G** Player movement and the weapon state machine are Modern Warfare 2's
      (`movement_iw4`, `weapon_iw4`), not Black Ops' (sprint, dive to prone, etc.).
- [ ] **G** Perks: only Modern Warfare 2 perk bits map (`script_player.rs`
      `perk_bits`); Double Tap does nothing; Juggernog needs `setmaxhealth`.
- [ ] **G/V** Last stand uses Modern Warfare 2's last-stand movement and view
      heights; every zombies player is forced into it by the engine
      (`script_player.rs`) rather than by Black Ops' callback; fixed 500 ms
      immunity after going down (`natives/player.rs`).
- [ ] **G** Zombies spawn ignores the stored max health (`script_player.rs`) —
      workaround for spectators copying 0 health into `maxhealth`; real rule unknown.
- [ ] **G** `allowstand`/`allowcrouch`/`allowprone`/`allowlean`/`allowmelee` not enforced.
- [ ] **G** `weapondualwieldweaponname` returns "none" (CZ75 dual wield).
- [ ] **G** Recorded, unused: `setsprintduration`, `setsprintcooldown`,
      `disableweaponcycling`, `setlaststandprevweap`, `setblockweaponpickup`.
- [ ] **G** Hardcoded: `weaponisgasweapon` 0, `depthinwater` 0, `needsrevive` 0,
      `isitempurchased` 1, `getweaponstowedmodel` 0; `disablegrenadesuicide` no-op.
- [ ] **G** `groundtrace` is `bullettrace`.
- [ ] **G** Bullet hits: no surface table; crouch/prone hit locations not separated.
- [ ] **G** Not implemented: `setmaxhealth`, `getplayerviewheight`, `magicgrenade`,
      `magicgrenademanual`, `dropweapon`.
- [ ] **G** Pack-a-Punch, teleporter, debris, traps, box move, nuke / max ammo /
      double points / carpenter unverified.

## Game mode and flow

- [ ] **G** The zombies match runs on Modern Warfare 2 match phases;
      `all_players_connected` ends a Modern Warfare 2 warmup (`iw4_gametype.rs`);
      players are held by an `awaiting_spawn` placement (`players.rs`).
- [ ] **G** Forced `onlinegame 1`, `sv_maxclients 18`, `default_xboxlive.cfg`
      (`match_apply.rs`); Black Ops solo is offline, co-op max is 4.
- [ ] **I** `getdifficulty` "medium", `issaverecentlyloaded` 0.
- [ ] **V** Recorded, unused: `settimescale`, `announcement`, `reviveobituary`, `setmatchflag`.
- [ ] **G/V** No-ops: `createthreatbiasgroup`, `setthreatbias`, `savegame`,
      `ropesetflag`, `reportmtu`, `stopallrumbles`, `sethintlowpriority`,
      `setperkfortrigger`, `setignoreentfortrigger`, `enablelinkto`,
      `setforcenocull`, `sendfaceevent`.
- [ ] **G** 171 builtins answer "not implemented yet" (`NativeRegistry::bind_gaps`,
      listed at match start); the remaining ones are in the log line.
- [ ] **G** Startup script faults: auto turret, `is_in_array` on arrays, `_gameskill`.

## Networking and co-op

- [ ] **G** `numremoteclients` 0; `snapshotacknowledged` not implemented.
- [ ] **V** `startrevive`/`stoprevive` no-ops; "being revived" view not drawn.
- [ ] **I** `islocaltohost` = client 0; `isplayeronsamemachine` = same client.
- [ ] **G** No spectating, bleed-out or respawn between rounds; no two-machine test.

## Closed

(none yet)
