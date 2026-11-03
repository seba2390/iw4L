//! Black Ops multiplayer's equipment builtins: `SpawnTimedFX` (Willy Pete's
//! smoke and Nova gas clouds), `TestSpawnPoint` and `IsTouchingVolume` (the
//! tactical insertion).

use super::super::args::{int, optional, string, vector};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, Value};
use bevy_ecs::prelude::World;
use game_api::{Unknown, unknown};

/// How long a timed effect's entity lasts when the script names no time,
/// in seconds.
const TIMED_FX_SECONDS: i32 = 10;
/// The box `TestSpawnPoint` fits a player in, and what it must not start in.
const SPAWN_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const SPAWN_MAXS: [f32; 3] = [15.0, 15.0, 70.0];
const SPAWN_MASK: u32 = 0x0081_0011;

/// `launch` only reaches a missile the game runs (Black Ops throws any
/// entity); no script of the base game launches another.
fn launch_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.launch_entity",
        "launch on an entity that is not a missile does nothing",
        "Black Ops' launch on any entity"
    )
}

/// The client's automatic fire keeps firing only while shots keep coming; a
/// burst's shots all arrive at once here.
fn fake_burst_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.fakefire_shots",
        "fakeFire's shots arrive together, so a burst sounds shorter than its count",
        "Black Ops' shot count on the fake fire event, played by the client's automatic fire"
    )
}

const ENTITY_WORLD: i32 = 0x3fe;
const ENTITY_NONE: i32 = 0x3ff;

fn timed_fx_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.timed_fx",
        "a SpawnTimedFX effect plays once where it was spawned; Black Ops keeps its entity for the given time, so a client that only then comes to see it starts the effect",
        "Black Ops' timed effect entity, replayed for clients that see it late"
    )
}

/// The clients whose camera spike view is toggled on (`setCameraSpikeActive`).
#[derive(bevy_ecs::prelude::Resource, Default)]
struct CameraSpikeToggles(std::collections::BTreeSet<u32>);

/// Whether the server has the client's camera spike toggled on.
pub(crate) fn camera_spike_toggled(world: &World, client: u32) -> bool {
    world
        .get_resource::<CameraSpikeToggles>()
        .is_some_and(|toggles| toggles.0.contains(&client))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    // `setCameraSpikeActive(toggle)`: the player's camera spike view, unless
    // the weapon is a scoped one held at aim.
    registry.register(Method, "setcameraspikeactive", |world, receiver, args| {
        let client = super::player::client_of(world, receiver)
            .map_err(|_| String::from("USAGE: Must be called on a client"))?;
        if args.is_empty() {
            return Err("USAGE: <toggle> not specified".into());
        }
        let on = int(args, 0)? != 0;
        let mut toggles = world.get_resource_or_insert_with(CameraSpikeToggles::default);
        if on {
            toggles.0.insert(client.0);
        } else {
            toggles.0.remove(&client.0);
        }
        Ok(Value::Undefined)
    });
    // `launch(velocity[, spin])` on a missile: it falls again from where it is.
    registry.register(Method, "launch", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let number = world.resource::<crate::script::RoundScript>().entities[&id].number;
        if args.is_empty() || args.len() > 2 {
            return Err("Incorrect number of parameters".into());
        }
        let velocity = vector(args, 0)?;
        if velocity.iter().any(|c| !c.is_finite()) {
            return Err(format!(
                "invalid velocity parameter in launch command: {} {} {}",
                velocity[0], velocity[1], velocity[2]
            ));
        }
        let spin = if args.len() > 1 {
            Some(vector(args, 1)?)
        } else {
            None
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        if !crate::game_fire::launch_game_missile(
            &mut FrameWorld::from_world(world),
            tick,
            number,
            velocity,
            spin,
        ) {
            let gap = launch_unknown();
            diag::warn!(Sim, "unknown!{}: {}", gap.id, gap.what);
        }
        Ok(Value::Undefined)
    });
    // `isOnGround([any entity but the world])`: a player's own rule, or what a
    // missile rests on: the world always, another entity unless the argument
    // asks for the world.
    registry.register(Method, "isonground", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let number = world.resource::<crate::script::RoundScript>().entities[&id].number;
        let frame = FrameWorld::from_world(world);
        if let Some((_, ground)) = crate::game_fire::game_missile_motion(&frame, number) {
            let world_only = matches!(args.first(), Some(Value::Int(n)) if *n != 0);
            let on = match ground {
                Some(ENTITY_WORLD) => true,
                Some(ENTITY_NONE) | None => false,
                Some(_) => !world_only,
            };
            return Ok(Value::Int(on.into()));
        }
        super::player::is_on_ground(world, receiver)
    });
    // `getVelocity` of a missile the game runs: its trajectory's.
    registry.register(Method, "getvelocity", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let number = world.resource::<crate::script::RoundScript>().entities[&id].number;
        let frame = FrameWorld::from_world(world);
        if let Some((velocity, _)) = crate::game_fire::game_missile_motion(&frame, number) {
            return Ok(Value::Vector(velocity));
        }
        super::engine::velocity_of(world, id)
    });
    // `fakeFire(owner, origin, weapon, shots)`: the weapon's fire sound at the
    // origin, by the client's automatic fire simulation; no shot is fired.
    registry.register(Method, "fakefire", |world, receiver, args| {
        if args.len() != 4 {
            return Err(
                "FakeFire( <owner>, <origin>, <weapon>, <shot count> ) takes 4 parameters".into(),
            );
        }
        let id = super::engine::entity_id(world, receiver)?;
        let number = world.resource::<crate::script::RoundScript>().entities[&id].number;
        super::player::client_of(world, &args[0])
            .map_err(|_| String::from("Owner entity is not a player"))?;
        let origin = vector(args, 1)?;
        let weapon = string(args, 2)?;
        let Some(weapon_id) = FrameWorld::from_world(world).weapon_index_by_script_name(&weapon)
        else {
            return Err(format!("Invalid weapon name {weapon}"));
        };
        let shots = int(args, 3)?.max(1);
        if shots > 1 {
            let gap = fake_burst_unknown();
            diag::info!(Sim, "game gap {}: {}", gap.id, gap.what);
        }
        let mut frame = FrameWorld::from_world(world);
        for _ in 0..shots {
            frame.push_script_audio(crate::ScriptAudioCommand::TurretFire {
                entity: number,
                weapon: weapon_id,
                barrel: 0,
                origin,
            });
        }
        Ok(Value::Undefined)
    });
    // `makeGrenadeDud`: the grenade's fuse no longer explodes it.
    registry.register(Method, "makegrenadedud", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let entity = &world.resource::<crate::script::RoundScript>().entities[&id];
        let crate::script::host::entities::EntityKind::Missile(_) = entity.kind else {
            return Err("MakeGrenadeDud: entity must be a grenade entity".into());
        };
        let number = entity.number;
        crate::game_fire::make_game_missile_dud(
            &mut crate::frame::FrameWorld::from_world(world),
            number,
        );
        Ok(Value::Undefined)
    });

    registry.register(Function, "spawntimedfx", spawn_timed_fx);
    registry.register(Function, "testspawnpoint", |world, _, args| {
        let at = vector(args, 0)?;
        let t = super::engine::trace(world, at, at, SPAWN_MINS, SPAWN_MAXS, SPAWN_MASK);
        Ok(Value::Int((t.startsolid == 0 && t.allsolid == 0).into()))
    });
    registry.register(Method, "istouchingvolume", |world, receiver, args| {
        let own = super::engine::entity_id(world, receiver)?;
        let origin = vector(args, 0)?;
        let (lo, hi) = (vector(args, 1)?, vector(args, 2)?);
        let mut mins: [f32; 3] = core::array::from_fn(|i| origin[i] + lo[i]);
        let mut maxs: [f32; 3] = core::array::from_fn(|i| origin[i] + hi[i]);
        // Grown in height to at least its width, as Black Ops' touch boxes
        // are.
        let height = maxs[2] - mins[2];
        let x = maxs[0] - mins[0];
        let y = maxs[1] - mins[1];
        let width = if x - y < 0.0 { y } else { x };
        if height < width {
            let grow = (width - height) * 0.5;
            mins[2] -= grow;
            maxs[2] += grow;
        }
        let hit = super::super::triggers::box_touches(world, own, mins, maxs);
        Ok(Value::Int(hit.into()))
    });
}

/// `SpawnTimedFX(weapon, origin, [dir], [seconds])`: the weapon's explosion
/// effect played at `origin` (snapped to whole units) facing `dir` (up when
/// none), its entity kept `seconds` (10 when none).
fn spawn_timed_fx(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let name = string(args, 0)?;
    let origin = vector(args, 1)?;
    let mut dir = optional(args, 2, vector)?.unwrap_or([0.0, 0.0, 1.0]);
    let _seconds = optional(args, 3, int)?.unwrap_or(TIMED_FX_SECONDS);
    let mut frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, &name)?;
    let effect = frame
        .game_weapon(weapon)
        .and_then(|w| w.explosion_effect.clone())
        .ok_or_else(|| format!("weapon '{name}' has no explosion effect"))?;
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    dir = dir.map(|v| v / len);
    let at = origin.map(|v| v as i32 as f32);
    let index = frame.effect_name_index(&effect);
    frame.report_game_gap(timed_fx_unknown());
    super::engine::play_fx_at(world, index, at, dir, [0.0; 3]);
    Ok(Value::Undefined)
}
