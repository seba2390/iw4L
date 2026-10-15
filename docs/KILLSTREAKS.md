# Killstreak script and engine ownership

Loaded IW4 GSC defines reward use, activation delays, duration, ownership and
team policy. The Rust simulation supplies the native operations and executes
the script scheduler; the client presents replicated state. This file covers
EMP and Sentry. The other rewards (UAV, care package, Predator, Harrier, Pave
Low, AC-130, Chopper Gunner, nuke, …) run on the loaded scripts too, through
the vehicle natives (`sim/src/script/host/vehicles.rs`) and missile guidance
(`guidance.rs`).

## EMP

Scripts select the active vehicles and turrets to affect, call radius damage,
notify rocket deaths, maintain the jammed team/player state, and request sounds
and vision effects. An EMP blast is ordinary script radius damage in the engine.
There is no engine EMP rule exempting the caller's vehicles.

The radius-damage native queues a blast. The simulation finds damageable targets
in range and applies damage. Vehicle damage passes through a script callback;
other script entities lose health and receive damage/death notifications.
Ownership exemptions and friendly-damage refunds belong to the loaded handlers.
The EMP-jammed native updates player flags; the HUD consumes the replicated flag.

## Sentry

Scripts create the turret, assign its owner/team and model, carry/place it,
switch its mode, handle health and lifetime, and schedule bursts, heat and sounds.
The placement native supplies a collision-checked support pose.

Rust acquires eligible hostile players within range and configured arcs, checks
visibility, turns the firing direction, determines target lock, and raises turret
state events. The shooting native traces a bullet along that firing direction
and submits damage and impact events.

The rates below are IW4L values (`sim/src/script/host/turrets.rs`), not
recovered from Modern Warfare 2. Firing direction turns at 360 degrees per
second and locks within 2 degrees.
The gun receives a rotation relative to the placement base. That pose is shared
by muzzle/collision queries and client rendering through replicated model state;
the support and acquisition arc base keep their placement orientation.
An idle sentry sweeps at 30 degrees per second, bounded by its configured left
and right arcs and an IW4L cap of 90 degrees on either side. Carried and offline
turrets stop scanning and release the gun rotation.
