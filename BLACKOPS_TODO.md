# Black Ops Zombies — roadmap

Goal: play Call of Duty: Black Ops (2010) Zombies in IW4L, hosted by one player
and joinable by friends on macOS, Linux and Windows. First target map: Kino der
Toten (`zombie_theater`), then Five (`zombie_pentagon`) and Dead Ops Arcade
(`zombietron`).

This is an outline, not a specification. Each step names an outcome and how we
know it is reached; the details are expected to change as we learn. Every step
ends in a commit and a push.

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

Done when both maps are playable locally.

### 1. Load a zombie map as a world
- [x] Load Kino's singleplayer collision; static props whose models live in
      another zone are reported, not fatal.
- [x] Spawn a player into Kino and walk around (temporarily under the MP
      free-for-all rules, using Kino's own player spawns and characters).

Done: Kino loads, players and bots spawn and move on correct collision.
Left for later steps: path data, the zombie common zones, a zombies game mode.

### 2. One actor on the floor
- [ ] Load the zombie common zones (`common_zombie`, its patch) next to the map.
- [ ] Load animation trees from their `.atr` source and resolve `%anim` references.
- [ ] Introduce an actor entity: spawn from a map spawner, run its aitype and
      character scripts, attach its models.
- [ ] Render actors on host and client.

Done when a zombie stands in Kino playing an animation.

### 3. Script-driven animation
- [ ] Implement the animation-tree builtins (set/knob/flagged/restart/limited,
      clear, timing queries).
- [ ] Deliver notetracks to scripts and keep synced loops in phase.
- [ ] Run the actor state machine and dispatch the zombie animscripts.

Done when the real zombie animscripts run and the zombie animates in place.

### 4. Movement and navigation
- [ ] Animation modes and orientation modes drive actor movement and turning,
      with collision and gravity.
- [ ] Extract path nodes and links; plan paths over them.
- [ ] Goal API and its notifies (`goal`, `bad_path`), path queries, dynamic
      path blocking.

Done when a zombie chases the player around Kino.

### 5. Traversals and barriers
- [ ] Traversal links run their animscripts (mantles, jump-downs, wall drops).
- [ ] Scripted animation overrides (rise from the ground, tearing boards).

Done when zombies come through windows and use every traversal in Kino.

### 6. Combat
- [ ] Zombie melee damages players.
- [ ] Player weapons hit actors with hit locations (headshots).
- [ ] Death, corpses, ragdoll and gibbing.

Done when a single zombie can be fought and killed with correct feedback.

### 7. Zombies game mode
- [ ] Run `_zombiemode` and the Kino map scripts on the host.
- [ ] Rounds, spawning, points, doors and debris, wall weapons, mystery box,
      perks, power, Pack-a-Punch, power-ups, last stand.
- [ ] Zombie HUD and menus.
- [ ] Client scripts, or host-side substitutes for what they show.

Done when a solo game of Kino can be played from round 1 until death.

### 8. Co-op
- [ ] Replicate actor state to clients efficiently.
- [ ] Revive, spectating and respawn between rounds for multiple players.
- [ ] Host and join through the master across macOS, Linux and Windows.

Done when two or more machines play a full Kino game together.

### 9. More content and polish
- [ ] Hellhounds and crawlers.
- [ ] Five and Dead Ops Arcade.
- [ ] Audio, effects and visual fidelity passes; performance with many zombies.

## Open questions

- How much engine behaviour beyond the scripts (state transitions, path
  following, melee ranges) needs reference measurements from the original game.
- What the actor state on the wire should look like.
- Whether friends will need MW2 installed alongside Black Ops, or whether the
  zombie mode can stand on Black Ops data alone.
