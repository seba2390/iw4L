//! Black Ops multiplayer's helicopter builtins as BlackOpsMP.exe's handlers
//! keep them: `spawnhelicopter` makes a helicopter from its vehicle file, the
//! script sets its speed, goal, yaw and tilt in the game's flight, and the
//! game's frame (`PlayerMovement::helicopter_frame`) flies it.

use super::super::args::{arg, float, optional, string, vector};
use super::super::vehicles::{Heli, spawn_vehicle};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::runtime::raise;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use game_api::heli::{HeliAim, HeliMove, HeliShot, HeliWorld};

/// Inches a second in a mile an hour.
const MPH: f32 = 17.6;
/// A vehicle's yaw deceleration by its yaw acceleration when spawned.
const YAW_DECEL_SHARE: f32 = 0.65;

/// The flight of a helicopter the game flies, by its receiver.
fn flight<'a>(world: &'a mut World, receiver: &Value) -> Result<&'a mut HeliMove, String> {
    let Value::Object(id) = receiver else {
        return Err("not an entity".into());
    };
    world
        .resource_mut::<RoundScript>()
        .into_inner()
        .vehicles
        .get_mut(id)
        .and_then(|heli| heli.game.as_mut())
        .ok_or_else(|| "entity is not a helicopter".into())
}

/// `spawnhelicopter(owner, origin, angles, vehicletype, model [, targetname])`.
fn spawn_helicopter(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let owner = world
        .resource::<RoundScript>()
        .player_client_of(arg(args, 0)?)
        .ok_or("Owner entity is not a player")?;
    let (origin, angles) = (vector(args, 1)?, vector(args, 2)?);
    let vehicle = string(args, 3)?;
    let model = string(args, 4)?;
    let Some(motion) = FrameWorld::from_world(world).vehicle_motion(&vehicle) else {
        return Err(format!("vehicle '{vehicle}' not precached"));
    };
    let game = HeliMove {
        origin,
        angles,
        previous_yaw: angles[1],
        def_speed: motion.max_speed,
        def_accel: motion.accel,
        max_speed: motion.max_speed,
        accel: motion.accel,
        decel: motion.accel,
        max_pitch: motion.max_body_pitch,
        max_roll: motion.max_body_roll,
        angle_limit: [
            motion.rot_rate * 0.5,
            motion.rot_rate,
            motion.rot_rate * 0.5,
        ],
        yaw_accel: motion.rot_accel,
        yaw_decel: motion.rot_accel * YAW_DECEL_SHARE,
        yaw_overshoot: 0.1,
        last_yaw_goal: -1.0,
        tilt_speed: 1056.0,
        turning: 0.5,
        hover_radius: 30.0,
        hover_speed: 176.0,
        hover_accel: 88.0,
        turret_rate: motion.turret_rot_rate,
        ..HeliMove::default()
    };
    let spawned = spawn_vehicle(
        world,
        "script_vehicle",
        origin,
        angles,
        &model,
        Some(Heli::flown_by_game(owner, game)),
    )?;
    if let Some(name) = optional(args, 5, string)?
        && let Value::Object(id) = spawned
    {
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "targetname",
            Value::string(&name),
        );
    }
    Ok(spawned)
}

/// `setspeed(speed [, accel [, decel]])`, in miles an hour.
fn set_speed(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let speed = float(args, 0)? * MPH;
    let accel = optional(args, 1, float)?.map(|v| v * MPH);
    let decel = optional(args, 2, float)?.map(|v| v * MPH);
    let heli = flight(world, receiver)?;
    heli.script_speed = true;
    heli.max_speed = speed.max(0.0);
    if let Some(accel) = accel {
        heli.accel = accel;
    }
    if heli.speed < heli.max_speed && heli.max_speed < heli.accel {
        heli.accel = heli.max_speed;
    }
    heli.decel = decel.unwrap_or(heli.accel * 0.5);
    if heli.accel <= 0.0 || heli.decel <= 0.0 {
        heli.accel = MPH;
        heli.decel = MPH;
        return Err("Acceleration/deceleration must be > 0".into());
    }
    if speed < 0.0 {
        return Err("Cannot set negative speed on vehicle".into());
    }
    Ok(Value::Undefined)
}

/// `setvehgoalpos(goal [, stop])`. Its third argument, the map's vehicle path,
/// is a gap: nothing in the multiplayer scripts passes it.
fn set_goal(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let goal = vector(args, 0)?;
    let stop = optional(args, 1, super::super::args::int)?.is_some_and(|v| v != 0);
    if args.len() > 2 {
        return Err("t5.vehicle.heli_path: setvehgoalpos's vehicle path is not ported".into());
    }
    let heli = flight(world, receiver)?;
    heli.script_speed = true;
    heli.goal = goal;
    heli.stop_at_goal = stop;
    if heli.max_speed == 0.0 || heli.accel == 0.0 || heli.decel == 0.0 {
        return Err("Speed and acceleration must not be zero before setting goal pos".into());
    }
    heli.has_goal = true;
    heli.hovering = false;
    heli.stopping = false;
    Ok(Value::Int(1))
}

fn set_max_pitch_roll(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let (pitch, roll) = (float(args, 0)?, float(args, 1)?);
    if pitch < 0.0 {
        return Err("Cannot set negative max pitch".into());
    }
    if roll < 0.0 {
        return Err("Cannot set negative max roll".into());
    }
    let heli = flight(world, receiver)?;
    heli.max_pitch = pitch;
    heli.max_roll = roll;
    Ok(Value::Undefined)
}

/// `setyawspeed(speed, accel [, decel [, overshoot]])`.
fn set_yaw_speed(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let (speed, accel) = (float(args, 0)?, float(args, 1)?);
    let decel = optional(args, 2, float)?.unwrap_or(accel);
    let overshoot = optional(args, 3, float)?;
    if let Some(overshoot) = overshoot
        && !(0.0..=1.0).contains(&overshoot)
    {
        return Err("Overshoot must be in 0 to 1 range".into());
    }
    if speed < 0.0 {
        return Err("Cannot set negative yaw speed on vehicle".into());
    }
    if accel < 0.0 {
        return Err("Cannot set negative yaw acceleration on vehicle".into());
    }
    let heli = flight(world, receiver)?;
    heli.angle_limit[1] = speed;
    heli.yaw_accel = accel;
    heli.yaw_decel = decel;
    if let Some(overshoot) = overshoot {
        heli.yaw_overshoot = overshoot;
    }
    Ok(Value::Undefined)
}

/// `sethoverparams(radius [, speed [, accel]])`.
fn set_hover_params(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let radius = float(args, 0)?;
    let speed = optional(args, 1, float)?;
    let accel = optional(args, 2, float)?;
    let heli = flight(world, receiver)?;
    heli.hover_radius = radius;
    if let Some(speed) = speed {
        heli.hover_speed = speed;
    }
    if let Some(accel) = accel {
        heli.hover_accel = accel;
    }
    Ok(Value::Undefined)
}

/// The match's side of a helicopter's frame.
struct MatchHeliWorld<'w> {
    world: &'w mut World,
    look_at: Option<u64>,
    heli: u64,
}

impl HeliWorld for MatchHeliWorld<'_> {
    fn dvar(&self, name: &str) -> Option<String> {
        self.world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get(&name.to_ascii_lowercase())
            .map(|value| value.to_string())
    }

    fn random_range(&mut self, low: f32, high: f32) -> f32 {
        let random = super::t5::random_unit(self.world);
        low + (high - low) * random
    }

    fn look_at(&mut self) -> Option<[f32; 3]> {
        let target = self.look_at?;
        match super::super::players::entity_field(self.world, target, "origin") {
            Value::Vector(origin) => Some(origin),
            _ => None,
        }
    }

    fn barrel(&mut self) -> Option<[f32; 3]> {
        super::super::presence::tag_world(self.world, self.heli, "tag_barrel")
            .or_else(|| super::super::presence::tag_world(self.world, self.heli, "tag_flash"))
            .map(|(origin, _)| origin)
    }

    fn entity_origin(&mut self, entity: i32) -> Option<[f32; 3]> {
        let object = object_numbered(self.world, entity)?;
        match super::super::players::entity_field(self.world, object, "origin") {
            Value::Vector(origin) => Some(origin),
            _ => None,
        }
    }

    fn sight_clear(&mut self, from: [f32; 3], to: [f32; 3], _target: i32) -> bool {
        super::engine::sight_clear_of_world(self.world, from, to)
    }
}

fn player_eye(world: &mut World, object: u64) -> Option<[f32; 3]> {
    let client = world.resource::<RoundScript>().player_client(object)?;
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(crate::ClientId(client))?;
    Some([
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ])
}

fn object_numbered(world: &World, number: i32) -> Option<u64> {
    let runtime = world.resource::<RoundScript>();
    if let Some((_, slot)) = runtime
        .players
        .iter()
        .find(|(client, _)| **client as i32 == number)
    {
        return Some(slot.object);
    }
    runtime
        .entities
        .iter()
        .find(|(_, entity)| entity.number == number)
        .map(|(id, _)| *id)
}

fn entity_number(world: &World, object: u64) -> Option<i32> {
    let runtime = world.resource::<RoundScript>();
    if let Some(client) = runtime.player_client(object) {
        return Some(client as i32);
    }
    runtime.entities.get(&object).map(|entity| entity.number)
}

/// `fireweapon([tag [, target [, offset]]])`: the barrel tag's shot; it
/// answers the missile a projectile weapon fired.
fn fire_weapon(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let Value::Object(id) = *receiver else {
        return Err("not an entity".into());
    };
    let (heli, weapon) = {
        let runtime = world.resource::<RoundScript>();
        let entry = runtime
            .vehicles
            .get(&id)
            .ok_or("entity is not a helicopter")?;
        (
            entry.game.ok_or("entity is not a helicopter")?,
            entry.weapon,
        )
    };
    let weapon = weapon.ok_or("Invalid weapon specified")?;
    let tag = optional(args, 0, string)?.unwrap_or_else(|| "tag_flash".to_owned());
    let (origin, axis) = super::super::presence::tag_world(world, id, &tag)
        .ok_or_else(|| format!("No tag {tag}"))?;
    let pivot = super::super::presence::tag_world(world, id, "tag_barrel").map(|(at, _)| at);
    let target = match args.get(1) {
        Some(Value::Undefined) | None => None,
        Some(value) => {
            let object = super::engine::entity_id(world, value)?;
            entity_number(world, object)
        }
    };
    let Some(movement) = crate::game_move::rules(&FrameWorld::from_world(world)) else {
        return Ok(Value::Undefined);
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let (missile, gaps) = {
        let mut frame = FrameWorld::from_world(world);
        let mut around = crate::game_fire::MatchFireWorld::new(&mut frame, tick, None);
        let shot = HeliShot {
            entity: around.number_of_object(id),
            weapon,
            origin,
            axis,
            pivot,
            target,
        };
        movement.helicopter_fire(&heli, &shot, &mut around)
    };
    let mut frame = FrameWorld::from_world(world);
    for gap in gaps {
        frame.report_game_gap(gap);
    }
    let Some(number) = missile else {
        return Ok(Value::Undefined);
    };
    let Some(projectile) = frame.projectile_by_number(number) else {
        return Ok(Value::Undefined);
    };
    let object = super::super::weapons::adopt(world, &projectile, "rocket")?;
    Ok(Value::Object(object))
}

/// A frame of a helicopter the game flies; false when this one is not.
pub(crate) fn advance(world: &mut World, id: u64) -> bool {
    let Some(mut heli) = world
        .resource::<RoundScript>()
        .vehicles
        .get(&id)
        .and_then(|heli| heli.game)
    else {
        return false;
    };
    if !world.resource::<RoundScript>().entities.contains_key(&id) {
        world.resource_mut::<RoundScript>().vehicles.remove(&id);
        return true;
    }
    let Some(movement) = crate::game_move::rules(&FrameWorld::from_world(world)) else {
        return true;
    };
    let look_at = world
        .resource::<RoundScript>()
        .vehicles
        .get(&id)
        .and_then(|heli| heli.look_at);
    let result = {
        let mut around = MatchHeliWorld {
            world,
            look_at,
            heli: id,
        };
        movement.helicopter_frame(&mut heli, &mut around)
    };
    let Some((notes, gap)) = result else {
        return true;
    };
    if let Some(gap) = gap {
        FrameWorld::from_world(world).report_game_gap(gap);
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Some(entry) = runtime.vehicles.get_mut(&id) {
        entry.game = Some(heli);
    }
    // The entity stands at whole units; the flight keeps the fraction.
    runtime.set_object_field(
        id,
        "origin",
        Value::Vector(heli.origin.map(|v| v as i32 as f32)),
    );
    runtime.set_object_field(id, "angles", Value::Vector(heli.angles));
    let receiver = Value::Object(id);
    for (said, name) in [
        (notes.near_goal, "near_goal"),
        (notes.goal_yaw, "goal_yaw"),
        (notes.goal, "goal"),
        (notes.turret_rotate_stopped, "turret_rotate_stopped"),
        (notes.turret_on_target, "turret_on_target"),
        (notes.turret_not_on_target, "turret_not_on_target"),
        (notes.turret_on_vistarget, "turret_on_vistarget"),
        (notes.turret_no_vis, "turret_no_vis"),
    ] {
        if said {
            raise(world, receiver.clone(), name, Vec::new());
        }
    }
    true
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "spawnhelicopter", spawn_helicopter);
    registry.register(Method, "setspeed", set_speed);
    registry.register(Method, "setvehgoalpos", set_goal);
    registry.register(Method, "setmaxpitchroll", set_max_pitch_roll);
    registry.register(Method, "setyawspeed", set_yaw_speed);
    registry.register(Method, "sethoverparams", set_hover_params);
    registry.register(Method, "getspeed", |world, receiver, _| {
        Ok(Value::Float(flight(world, receiver)?.speed))
    });
    registry.register(Method, "getspeedmph", |world, receiver, _| {
        Ok(Value::Float(flight(world, receiver)?.speed / MPH))
    });
    registry.register(Method, "resumespeed", |world, receiver, _| {
        flight(world, receiver)?.script_speed = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setneargoalnotifydist", |world, receiver, args| {
        let dist = float(args, 0)?;
        flight(world, receiver)?.near_goal = dist;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturningability", |world, receiver, args| {
        let turning = float(args, 0)?;
        flight(world, receiver)?.turning = turning;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setlookatent", |world, receiver, args| {
        let target = super::engine::entity_id(world, arg(args, 0)?)?;
        let Value::Object(id) = receiver else {
            return Err("not an entity".into());
        };
        if let Some(heli) = world.resource_mut::<RoundScript>().vehicles.get_mut(id) {
            heli.look_at = Some(target);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearlookatent", |world, receiver, _| {
        let Value::Object(id) = receiver else {
            return Err("not an entity".into());
        };
        if let Some(heli) = world.resource_mut::<RoundScript>().vehicles.get_mut(id) {
            heli.look_at = None;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "fireweapon", fire_weapon);
    registry.register(Method, "setvehweapon", |world, receiver, args| {
        let name = string(args, 0)?;
        let weapon = FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .ok_or_else(|| format!("unknown weapon '{name}'"))?;
        let Value::Object(id) = receiver else {
            return Err("not an entity".into());
        };
        if let Some(heli) = world.resource_mut::<RoundScript>().vehicles.get_mut(id) {
            heli.weapon = Some(weapon);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturrettargetvec", |world, receiver, args| {
        let point = vector(args, 0)?;
        flight(world, receiver)?.turret_aim = Some(HeliAim::Point(point));
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturrettargetent", |world, receiver, args| {
        let object = super::engine::entity_id(world, arg(args, 0)?)?;
        let offset = optional(args, 1, vector)?.unwrap_or([0.0; 3]);
        let entity = entity_number(world, object).ok_or("not an entity")?;
        flight(world, receiver)?.turret_aim = Some(HeliAim::Entity { entity, offset });
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearturrettarget", |world, receiver, _| {
        flight(world, receiver)?.turret_aim = None;
        Ok(Value::Undefined)
    });
    // The stage reaches clients for the smoke and damage parts; not drawn yet
    // (`docs/fidelity/t5-mp.md`).
    registry.register(Method, "setdamagestage", |world, receiver, args| {
        let stage = super::super::args::int(args, 0)?;
        if let Value::Object(id) = receiver {
            world.resource_mut::<RoundScript>().set_object_field(
                *id,
                "damage_stage",
                Value::Int(stage),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopfireweapon", |_, _, _| Ok(Value::Undefined));
    // `heliturretsighttrace(from, player, last)`: a clear line from the point
    // to the player's eye.
    registry.register(Method, "heliturretsighttrace", |world, _, args| {
        let from = vector(args, 0)?;
        let object = super::engine::entity_id(world, arg(args, 1)?)?;
        let eye = player_eye(world, object).ok_or("not a player")?;
        Ok(Value::Int(
            super::engine::sight_clear_of_world(world, from, eye).into(),
        ))
    });
    registry.register(Method, "missile_settarget", |world, receiver, args| {
        let Value::Object(id) = *receiver else {
            return Err("not an entity".into());
        };
        let target = match args.first() {
            Some(Value::Undefined) | None => None,
            Some(value) => {
                let object = super::engine::entity_id(world, value)?;
                entity_number(world, object)
            }
        };
        let number = entity_number(world, id).ok_or("not an entity")?;
        crate::game_fire::set_missile_target(&mut FrameWorld::from_world(world), number, target);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setgoalyaw", |world, receiver, args| {
        let yaw = float(args, 0)?;
        flight(world, receiver)?.goal_yaw = Some(yaw);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleargoalyaw", |world, receiver, _| {
        flight(world, receiver)?.goal_yaw = None;
        Ok(Value::Undefined)
    });
    registry.register(Method, "settargetyaw", |world, receiver, args| {
        let yaw = float(args, 0)?;
        flight(world, receiver)?.target_yaw = Some(yaw);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleartargetyaw", |world, receiver, _| {
        flight(world, receiver)?.target_yaw = None;
        Ok(Value::Undefined)
    });
}
