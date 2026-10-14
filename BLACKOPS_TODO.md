# Black Ops Zombies — roadmap

Goal: play Call of Duty: Black Ops (2010) Zombies in IW4L, hosted by one player
and joinable by friends on macOS, Linux and Windows. First target map: Kino der
Toten (`zombie_theater`), then Five (`zombie_pentagon`) and Dead Ops Arcade
(`zombietron`).

This is an outline, not a specification. Each step names an outcome and how we
know it is reached; the details are expected to change as we learn. Every step
ends in a commit and a push.

## Status (2026-10-15)

Kino loads and runs Black Ops' own zombie scripts (`_zombiemode` and
everything it pulls in): zombies spawn behind the windows, tear the boards,
climb through and run at the player, drawn with their animations. Builtins the
runtime lacks are reported per call site instead of stopping the match. A
Kino starts raises two script faults, the `is_in_array` pair in
`movie_reels_init`, which Black Ops' own VM raises too (it refuses `==` on
arrays). Its map turrets spawn and answer Black Ops' turret methods; their
think (aiming, firing) is not ported (`t5.turret.think`). Hellhound rounds
spawn, chase the player to claimed attack spots, bite for 40 damage and drop
Max Ammo on the last dog. The Thundergun equips its viewmodel, fires and
blasts zombies and hellhounds in its cone; thrown Cymbal Monkeys animate,
reset their fuse, generate attractor positions, play their song and detonate;
and electric traps activate, shock players and electrocute zombies. Nova
crawlers drop through Kino's broken ceilings and hunt the player, and the
teleporter carries players to the projection room and back.

Black Ops' client scripts (CSC) now run on a client VM of their own
(`script::client`, its own world, threads and natives, fed after the
server's frame): Kino's client `main` starts its music, ambient, fx and
weapon threads; the server's client system states, `clientNotify`s, client
exploders, mirrored snapshot entities (`entityspawned`, `playerspawned`,
`entityshutdown`, `zombie_eye_callback`) and `setclientflag` changes reach
them; its random ambient one-shots and its loop emitters (fires, steam,
chandeliers, power boxes, the projector) play, and the player's footsteps
play through `CodeCallback_PlayerFootstep`. `localclientconnect(0)` runs
after `main`, and the client's effects play for the local client: Kino's
190 createfx effects (`loopfx` replaying when `isfxplaying` says the last
one died, `t5.csc.fx_lifetime`), tag effects and effect entities. Its
lights (`makelight` from the map's compiled primary lights, typed by
`radiant/clientkeys.txt`) and models (`setmodel`, `hide`, `show`, `moveto`,
`rotate*`, drawn through server twins) run; light changes are not drawn yet
(`t5.csc.light_render`). Its sounds carry playback ids: the local sound
system plays, stops and fades them by id and reports back whether they play,
how far and how long, which `_music.csc` waits on. Still unrun on the
client: line emitters, ambient rooms, sound contexts and snapshots and the
client's view of its player (`t5.csc.run`).

Since the game boundary (docs/ARCHITECTURE.md) Black Ops runs only on Black
Ops' rules and data, and the rules IW4L had borrowed from Modern Warfare 2 are
off until Black Ops' own are recovered (each is an `unknown!` id in
[`docs/fidelity/t5.md`](docs/fidelity/t5.md)). The player moves on Black Ops'
own movement (`movement_t5`; ladders, mantling, swimming and its movement
events are still open) and Black Ops' weapon rules: bullet shots, melee
blows, rockets (the Ray Gun), thrown grenades and script blasts run Black
Ops' own server rules (spread, pellets, penetration, melee traces, missiles,
grenade bounces and fuses, blasts, damage); grenade launchers, sticky and
special grenades are still open (`t5.weapons.fire`, `t5.weapons.grenade`).
A zombies player goes
down on every lethal hit and the zombie scripts run last stand, solo Quick
Revive self-revive (`m1911_upgraded_zm` in hand), `startrevive`/`stoprevive`
and game over (`t5.match.last_stand` closed); a solo
game over fails the level as Black Ops does, blurs the view to 32 over the
death delay, draws any `ui_deadquote` (`CG_DEADQUOTE`), and reloads Kino
(`t5.match.level_failed`: no save games, timescale ramp or sound-mixer fade;
#156). Every hit blurs the view through `_gameskill`'s `startfadingblur`.
The test player (`IW4L_TEST_PLAYER`, #148) plays Kino's rounds, buys, revives
and goes down by itself; going down and `reviveplayer` raise Black Ops'
stance events (#149). There is
no code-drawn HUD yet (`t5.hud.code_hud`); Black Ops' HUD menus (weapon
info, dpad, points column; in its name-hash order and safe area, #151, #153)
and use hints with their icons, and the zombie scripts' hud elems (round
chalk and intro, point popups, game over), are drawn by its own rules, and there are no
Modern Warfare 2 sounds. Weapons sound by Black Ops' client rules: a
player's fire, reload, raise and putaway events reach every client, which
plays the weapon's own player or world sound; another player's automatic
fire waits on Black Ops' fire sound simulation (`t5.audio.fire_autosim`),
and shots show no flash, shell or tracer yet (`t5.weapons.fire_effects`).
The first-person viewmodel plays the animation Black Ops' client picks for
each weapon animation number, so reload and other notetrack sounds play;
its blends and rates are still the runtime's (`t5.viewmodel.playback`).
Pack-a-Punch and all six of Kino's power-ups work through Black Ops' own
scripts, now that its weapon state builtins and weapon notifies follow its
own rules. Footsteps, music states and most client effects wait on client
scripts (CSC), which the runtime does not run yet.
The work now is
recovering those rules from Black Ops. Start it from the game library
(Black Ops → Zombies → Kino der Toten), or headless:

```bash
IW4L_SOUND=off IW4L_GAMETYPE=zombies ./target/play/iw4l serve t5:zombie_theater --cmds 'wait world; bot add 1; wait 20s; quit'
```

Co-op hosting goes through the same lobby (`set ui_mapname t5:zombie_theater;
set ui_gametype zom; ui_create_lobby; ui_lobby_privacy`).

## What we start from

- The T5 reader already opens every asset in the zombie zones (`common_zombie`,
  `zombie_theater`, `zombie_pentagon`, the singleplayer `common` and
  `code_post_gfx`).
- The zombie game logic ships as readable GSC/CSC source inside those zones.
- About half of the engine builtins the zombie scripts call already exist.
  The largest missing area is AI actors, which the runtime refuses today.
- Hosting, relaying and cross-platform builds already exist for multiplayer.

## Steps

### 0. Baseline on this Mac
- [x] Build IW4L on Apple silicon (M3 Pro, Metal; first `play` build 3m11s).
- [x] Run an MW2 map and a Black Ops multiplayer map with bots (`mp_rust`, `t5:mp_nuked`).
      Since the game boundary (docs/ARCHITECTURE.md) a Black Ops multiplayer map
      is refused at load: it ran Modern Warfare 2's gametype scripts
      (`t5.scripts.mp_gametypes` in `docs/fidelity/t5.md`).

Done when both maps are playable locally.

### 1. Load a zombie map as a world
- [x] Load Kino's singleplayer collision; static props whose models live in
      another zone are reported, not fatal.
- [x] Spawn a player into Kino and walk around (temporarily under the MP
      free-for-all rules, using Kino's own player spawns and characters).

Done: Kino loads, players and bots spawn and move on correct collision.
Left for later steps: path data, the zombie common zones, a zombies game mode.

### 2. One actor on the floor
- [x] Load the zombie common zones (`common_zombie`, its patch, the singleplayer
      `common` and `code_post_gfx`) next to the map: animations, weapons, scripts.
- [x] T5 animations carry root motion; scripts can query any clip
      (`getmovedelta`, `getangledelta`, notetracks).
- [x] Load animation trees from their `.atr` source and resolve `%anim` references
      (`generic_human`: 7379 nodes).
- [x] Introduce an actor entity: spawn from a map spawner, run its aitype and
      character scripts, attach its models.
- [x] Render actors on clients with their animation state: each tick the
      weighted part of the actor's tree is published on its model, and the
      zombie bodies and heads (with their materials) join the client's models.

Done when a zombie stands in Kino playing an animation.

### 3. Script-driven animation
- [x] Implement the animation-tree builtins (set/knob/flagged/restart/limited,
      clear, timing queries).
- [x] Deliver notetracks to scripts. Synced loops are not phase-locked yet.
- [x] Run the zombie animscript init and dispatch `stop`; a spawned zombie idles
      on `ai_zombie_idle_v1_delta`.
- [x] Dispatch `move`, `combat`, `scripted` and traversals as well as `stop`.
- [x] `death`.
- [ ] Custom animation (`AnimCustom`).

Done when the real zombie animscripts run and the zombie animates in place.

### 4. Movement and navigation
- [x] Animation modes and orientation modes drive actor movement and turning
      from the animations' root motion, kept on the ground by traces.
- [x] Extract path nodes and links (Kino: 1568 nodes); plan paths over them.
- [x] Goal API and its notifies (`goal`, `bad_path`), path node queries.
      A Kino zombie now walks from its spawn to its window.
- [ ] Actor-vs-world collision while walking, and dynamic path blocking
      (`ConnectPaths`/`DisconnectPaths`).

Done when a zombie chases the player around Kino.

### 5. Traversals and barriers
- [x] Traversal links run their animscripts (mantles, jump-downs, wall drops).
- [x] Scripted animation (`AnimScripted`): a zombie tears all six boards off
      its window, mantles in, walks down the stairs and reaches the player.
- [ ] Verify every Kino traversal and the rise-from-ground spawn.

Done when zombies come through windows and use every traversal in Kino.

### 6. Combat
- [x] Zombie melee damages players (and kills a lone player).
- [x] Damage on actors runs the actor damage/killed callbacks (points, gib
      checks) and finishes through `FinishActorDamage`; killed zombies play
      their death animscript and are removed, and rounds advance.
- [x] Player bullets hit actors on their animated bone boxes with the right
      hit locations (the zombies match leaves warmup when every player is
      connected, so shots trace entities).
- [ ] Ragdoll (`startragdoll` on a dying actor; dead zombies hold their death
      pose for now) and thrown gib pieces (heads already come off).

Done when a single zombie can be fought and killed with correct feedback.

### 7. Zombies game mode
- [x] Run `_zombiemode` and the Kino map scripts on the host (zombies game
      mode, T5 zombie builtin catalog, singleplayer code callbacks).
- [x] Zombie weapons (`*_zm`) are registered under their script names.
- [x] Players spawn through the zombie scripts and receive their loadout
      (the M1911 in first person, 500 points).
- [ ] Close the remaining builtin gaps the scripts report while a round runs
      (left at startup: the footstep aliases and
      `uin_transition_zombie_theater`, which only the scripts' no-client-script
      fallbacks ask for).
- [x] Rounds, zombie spawning, points for hits and kills, wall weapons (use
      triggers answer a player looking at them within use range), doors.
- [x] Power switch and perks (Quick Revive bought, drunk in first person,
      icon on the HUD; stance and melee locks during the drink are not
      enforced yet). Script models play `AnimScripted` clips once.
- [ ] Game over: a downed solo player without Quick Revive ends the game
      with the "You Survived N Rounds" screen (T5 SetText/SetHintString
      values fill the string's `&&1`..), then `MissionFailed()` loads the
      level again after a second plus the death delay, as Black Ops does
      (`t5.match.level_failed`). The text is a script text hud elem (not
      drawn yet).
- [ ] Debris, Pack-a-Punch, teleporter, the remaining power-ups.
- [x] Last stand: every zombies player goes down; solo with Quick Revive
      revives on their own (Mustang & Sally in hand), the next down ends
      the game (`t5.match.last_stand` closed).
- [x] Mystery box: bought at its random start location, the weapons spin
      and the drawn weapon is taken (a SPAS-12 in the test run).
- [x] Power-ups drop from killed zombies at the score thresholds and are
      picked up by walking over them (Insta-Kill verified).
- [x] Zombie sounds: the zombie and singleplayer zones' aliases join the
      sound bank, scripts' bare `zmb_*` names play as T5 aliases, and
      `PlaySound(alias, notify)` raises its done-notify (after a fixed 2s;
      footstep aliases from the singleplayer sound banks are still missing).
- [ ] Zombie HUD and menus: Black Ops' own HUD menu lists (`ui/hud.txt`,
      `ui/hud_sp.txt`, `ui/hud_zombie.txt`, `ui/hud_coop.txt` from
      `code_post_gfx` + `patch`) now draw with Black Ops' fonts and
      materials (weapon info, dpad). Waiting on the maintainer (see
      `docs/fidelity/t5.md`): the points column and the scoreboard (drawn by
      the executable), script text hud elems (font/size rule), several owner
      draws. The use hint is still the Modern Warfare 2 one; the lobby menus do
      not offer the zombies mode yet.
- [ ] Client scripts, or host-side substitutes for what they show.

Done when a solo game of Kino can be played from round 1 until death.

### 8. Co-op
- [x] A lobby host can pick a Black Ops zombie map (`zom` mode); the match
      waits for every lobby member before play starts, both get their
      points, and players start on Kino's separate co-op spawn points.
      Joined clients see the zombies animate and attack (two processes on
      one Mac through a local master).
- [ ] Replicate actor state to clients efficiently (it works through the
      existing entity path; not measured).
- [ ] Revive: a downed co-op player is revived by a teammate holding use
      (the "being revived" view is not drawn). Off with last stand.
- [ ] Spectating, bleed-out and respawn between rounds for multiple players.
- [ ] Host and join through the master across macOS, Linux and Windows.

Done when two or more machines play a full Kino game together.

### 9. More content and polish
- [ ] Hellhounds and crawlers.
- [ ] Five and Dead Ops Arcade.
- [ ] Audio, effects and visual fidelity passes; performance with many zombies.
- [ ] Make `crates/asset_audio/src/wma_t5/format.bin` (the Black Ops audio
      decoder's entropy codebooks and exponent band widths, read by
      `wma_t5/entropy.rs`) a readable generated Rust table instead of a binary
      blob, so a change to it reads in a diff; keep its generator and where the
      values come from in the commit. If its numbers trip `publish-check`'s
      offset scan, exempt exactly that file with the reason, as for
      `crates/gsc_t7/src/names.rs`.

## Waiting on the maintainer

Items that need a reference capture from the original game or a rule only the
executable holds are listed, with exactly what is needed, under "Waiting on the
maintainer" in `docs/fidelity/t5.md`: the zombies points column and
scoreboard, script text hud elems, `UI_FONT_DEFAULT`, several HUD owner draws,
weapon info details, engine dvar defaults, vision-set grading and Double Tap's
fire-rate factor.

The first of them, the blanked executables, is answered: the CEG-filled
`BlackOps.exe` and `BlackOpsMP.exe` from a Windows install are available
locally, so player movement, the weapon state machine, the last-stand entry,
the HUD's font and layout rules and the engine dvar defaults are now recovered
from Black Ops' own executable, one rule at a time.

## Open questions

- How much engine behaviour beyond the scripts (state transitions, path
  following, melee ranges) needs reference measurements from the original game.
- What the actor state on the wire should look like.
- Whether friends will need MW2 installed alongside Black Ops, or whether the
  zombie mode can stand on Black Ops data alone.
