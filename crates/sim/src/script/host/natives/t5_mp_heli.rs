//! Black Ops multiplayer's helicopter builtins as BlackOpsMP.exe's handlers
//! keep them: `spawnhelicopter` makes a helicopter from its vehicle file, the
//! script sets its speed, goal, yaw and tilt in the game's flight, and the
//! game's frame (`PlayerMovement::helicopter_frame`) flies it.

use super::super::args::{arg, float, int, optional, string, vector};
use super::super::arrays::new_array;
use super::super::players::{LinkView, PlayerLink};
use super::super::vehicles::{Heli, spawn_vehicle};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::runtime::raise;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use game_api::heli::{HeliAim, HeliMove, HeliPilot, HeliShot, HeliWorld};
use game_api::movement::ScriptFlag;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Inches a second in a mile an hour.
const MPH: f32 = 17.6;
/// A vehicle's yaw deceleration by its yaw acceleration when spawned.
const YAW_DECEL_SHARE: f32 = 0.65;
/// What stops a helicopter (its entity's clip mask).
const HELI_CLIP_MASK: u32 = 0x0080_0400;
/// A vehicle's seats.
const SEATS: usize = 11;
/// A remote controller's view sits this far over the vehicle's `tag_player`.
const REMOTE_EYE_UP: f32 = 60.0;
/// The seats with guns, the driver's first.
const GUNNER_SEATS: usize = 5;
/// The weapon definition's fields a gunner's gun reads.
const DEF_FIRE_TIME: usize = 0x3ac;
const DEF_HEAT: usize = 0x50;
const DEF_HEAT_RATE: usize = 0x54;
const DEF_COOL_RATE: usize = 0x58;
const DEF_COOL_RESUME: usize = 0x5c;
const HEAT_FULL: f32 = 100.0;
/// How far ahead of their view a pilot's gun aims, and what stops the aim.
const AIM_RANGE: f32 = 10240.0;
/// A splash's share of the blended damage zones.
const SPLASH_SHARE: f32 = 0.7;
const AIM_MASK: u32 = 0x0280_e893;

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
        piloted: game_api::heli::PilotedFlight {
            motion,
            ..Default::default()
        },
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
    if let Value::Object(vehicle) = receiver {
        let rate = accel.unwrap_or(speed.max(MPH) * 0.5);
        super::t5_mp_vehpath::set_path_speed(world, *vehicle, speed.max(0.0), rate);
    }
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

    fn sweep(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
    ) -> Option<(f32, [f32; 3])> {
        let own = self
            .world
            .resource::<RoundScript>()
            .entities
            .get(&self.heli)
            .and_then(|entity| entity.presence);
        let hit = FrameWorld::from_world(self.world).trace_world_hull_except(
            from,
            to,
            mins,
            maxs,
            HELI_CLIP_MASK,
            &|owner| owner.script_model() == own,
        );
        (hit.startsolid == 0).then_some((hit.fraction, hit.normal))
    }

    fn height_lock_at(&mut self, at: [f32; 3], reach: f32) -> Option<f32> {
        height_locks(self.world)
            .into_iter()
            .find_map(|(mins, maxs)| {
                let inside = (0..2).all(|i| mins[i] <= at[i] && at[i] <= maxs[i]);
                (inside && (at[2] - maxs[2]).abs() <= reach).then_some(maxs[2])
            })
    }
}

/// The world boxes of the map's height lock volumes.
fn height_locks(world: &mut World) -> Vec<([f32; 3], [f32; 3])> {
    let volumes: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(_, entity)| &*entity.classname == "heli_height_lock")
            .map(|(id, _)| *id)
            .collect()
    };
    volumes
        .into_iter()
        .map(|volume| {
            let (mins, maxs) = super::super::triggers::entity_bounds(world, volume);
            let place = super::engine::vector_field(world, volume, "origin");
            (
                std::array::from_fn(|i| mins[i] + place[i]),
                std::array::from_fn(|i| maxs[i] + place[i]),
            )
        })
        .collect()
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
    let driver = seats(world).seats.get(&id).and_then(|row| row[0]);
    let (missile, gaps) = {
        let mut frame = FrameWorld::from_world(world);
        let mut around = crate::game_fire::MatchFireWorld::new(&mut frame, tick, None);
        let entity = around.number_of_object(id);
        let shot = HeliShot {
            entity,
            weapon,
            origin,
            axis,
            pivot,
            target,
            attacker: driver.map_or(entity, |client| client as i32),
            aimed: None,
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

/// A weapon's `fireTime`, in milliseconds, from its definition.
fn weapon_fire_ms(frame: &FrameWorld, weapon: u32) -> i32 {
    frame
        .content()
        .game_weapon(weapon)
        .and_then(|game| game.def.get(DEF_FIRE_TIME..DEF_FIRE_TIME + 4))
        .map_or(0, |bytes| {
            i32::from_le_bytes(bytes.try_into().expect("four bytes"))
        })
}

fn def_f32(frame: &FrameWorld, weapon: u32, at: usize) -> f32 {
    frame
        .content()
        .game_weapon(weapon)
        .and_then(|game| game.def.get(at..at + 4))
        .map_or(0.0, |bytes| {
            f32::from_le_bytes(bytes.try_into().expect("four bytes"))
        })
}

/// A gunner seat's gun: its fire time and how hot it is.
#[derive(Clone, Copy, Debug, Default)]
struct GunnerGun {
    fire_ms: i32,
    heat: f32,
    overheated: bool,
}

/// The vehicle a player sits in, and the seats of each vehicle.
#[derive(bevy_ecs::prelude::Resource, Default)]
struct VehicleSeats {
    seats: BTreeMap<u64, [Option<u32>; SEATS]>,
    /// What is left of a vehicle's fire time, in milliseconds.
    fire_ms: BTreeMap<u64, i32>,
    guns: BTreeMap<(u64, usize), GunnerGun>,
}

/// BlackOpsMP.exe's gunner think: a seated gunner holding attack fires the
/// seat's `gunnerWeapon` from `tag_flash_gunner<n>` where they look, as their
/// own shot, until the gun's heat stops it.
fn gunner_frame(world: &mut World, id: u64, heli: &HeliMove) {
    let row = seats(world)
        .seats
        .get(&id)
        .copied()
        .unwrap_or([None; SEATS]);
    let name_end = heli
        .piloted
        .motion
        .gunner_weapon
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(32);
    let Ok(name) = std::str::from_utf8(&heli.piloted.motion.gunner_weapon[..name_end]) else {
        return;
    };
    let name = name.to_owned();
    if name.is_empty() {
        return;
    }
    let Some(weapon) = FrameWorld::from_world(world).weapon_index_by_script_name(&name) else {
        return;
    };
    let frame_ms = crate::MATCH_TICK_MS as i32;
    for (seat, client) in row.iter().enumerate().take(GUNNER_SEATS).skip(1) {
        let Some(client) = *client else {
            continue;
        };
        let (held, view, fire_ms, heat_on, gain, cool, resume) = {
            let mut frame = FrameWorld::from_world(world);
            let id_client = crate::ClientId(client);
            let held = crate::script_player::buttons(&mut frame, id_client)
                & crate::script::command_buttons("+attack")
                != 0;
            let view = frame.player(id_client).map(|ps| ps.viewangles);
            (
                held,
                view,
                weapon_fire_ms(&frame, weapon),
                frame
                    .content()
                    .game_weapon(weapon)
                    .and_then(|game| game.def.get(DEF_HEAT..DEF_HEAT + 4))
                    .is_some_and(|bytes| bytes != [0; 4]),
                def_f32(&frame, weapon, DEF_HEAT_RATE),
                def_f32(&frame, weapon, DEF_COOL_RATE),
                def_f32(&frame, weapon, DEF_COOL_RESUME),
            )
        };
        let gun = seats(world).guns.entry((id, seat)).or_default();
        gun.fire_ms = (gun.fire_ms - frame_ms).max(0);
        if heat_on {
            let per_frame = frame_ms as f32 * 0.001;
            if !held || gun.overheated {
                gun.heat = (gun.heat - per_frame * cool).max(0.0);
                if gun.heat <= resume {
                    gun.overheated = false;
                }
            } else {
                gun.heat += per_frame * gain;
                if HEAT_FULL <= gun.heat {
                    gun.overheated = true;
                    gun.heat = HEAT_FULL;
                }
            }
        }
        let gun = *gun;
        if !held || gun.fire_ms > 0 || gun.overheated {
            continue;
        }
        let tag = format!("tag_flash_gunner{seat}");
        // The doors' animation is the client's: the server has the tag at rest.
        let pose = super::super::presence::tag_world(world, id, &tag)
            .or_else(|| super::super::presence::tag_world_bound(world, id, &tag));
        let Some((origin, axis)) = pose else {
            continue;
        };
        let Some(view) = view else {
            continue;
        };
        let Some(movement) = crate::game_move::rules(&FrameWorld::from_world(world)) else {
            continue;
        };
        let (forward, _, _) = math_iw4::angle_vectors(view);
        let aimed = std::array::from_fn(|i| origin[i] + forward[i] * AIM_RANGE);
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let gaps = {
            let mut frame = FrameWorld::from_world(world);
            let mut around = crate::game_fire::MatchFireWorld::new(&mut frame, tick, None);
            let entity = around.number_of_object(id);
            let shot = HeliShot {
                entity,
                weapon,
                origin,
                axis,
                pivot: None,
                target: None,
                attacker: client as i32,
                aimed: Some(aimed),
            };
            movement.helicopter_fire(heli, &shot, &mut around).1
        };
        let mut frame = FrameWorld::from_world(world);
        for gap in gaps {
            frame.report_game_gap(gap);
        }
        if let Some(gun) = seats(world).guns.get_mut(&(id, seat)) {
            gun.fire_ms = fire_ms;
        }
    }
}

fn seats(world: &mut World) -> &mut VehicleSeats {
    world
        .get_resource_or_insert_with(VehicleSeats::default)
        .into_inner()
}

fn vehicle_id(receiver: &Value) -> Result<u64, String> {
    match receiver {
        Value::Object(id) => Ok(*id),
        _ => Err("not an entity".into()),
    }
}

fn seat_of(world: &World, client: u32) -> Option<(u64, usize)> {
    world
        .get_resource::<VehicleSeats>()?
        .seats
        .iter()
        .find_map(|(vehicle, seats)| {
            seats
                .iter()
                .position(|seat| *seat == Some(client))
                .map(|seat| (*vehicle, seat))
        })
}

/// `vehicle usevehicle(player, seat)`: the player takes the seat. A remote
/// control vehicle leaves their body where it stands: their view rides its
/// `tag_player`, turning with it by the file's `viewInfluence`, and it hears
/// their controls. Both hear `enter_vehicle` (seat, the other).
fn use_vehicle(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let vehicle = vehicle_id(receiver)?;
    let object = super::engine::entity_id(world, arg(args, 0)?)?;
    let client = world
        .resource::<RoundScript>()
        .player_client(object)
        .ok_or("usevehicle: not a player")?;
    let seat = int(args, 1)? as usize;
    if seat >= SEATS {
        return Err("usevehicle: no such seat".into());
    }
    let id = crate::ClientId(client);
    if seats(world)
        .seats
        .get(&vehicle)
        .is_some_and(|row| row[seat] == Some(client))
    {
        return Ok(Value::Undefined);
    }
    if let Some((old_vehicle, old_seat)) = seat_of(world, client)
        && let Some(row) = seats(world).seats.get_mut(&old_vehicle)
    {
        row[old_seat] = None;
    }
    let motion = flight(world, receiver)?.piloted.motion;
    seats(world).seats.entry(vehicle).or_default()[seat] = Some(client);
    if motion.remote_control {
        let mut frame = FrameWorld::from_world(world);
        crate::game_move::set_script_flag(&mut frame, id, ScriptFlag::RemoteVehicle, true);
    }
    // A gunner rides their seat's tag; the driver's view sits over `tag_player`.
    let tag = if seat == 0 {
        "tag_player".to_owned()
    } else {
        format!("tag_gunner{seat}")
    };
    let (_, axis) = super::super::players::link_parent_pose(world, vehicle, Some(&tag));
    let restore_view = FrameWorld::from_world(world)
        .player(id)
        .map(|ps| ps.viewangles);
    super::super::players::link_player(
        world,
        client,
        PlayerLink {
            parent: vehicle,
            tag: Some(Arc::from(tag.as_str())),
            origin: if seat == 0 {
                [0.0, 0.0, REMOTE_EYE_UP]
            } else {
                [0.0; 3]
            },
            angles: [0.0; 3],
            view: LinkView::WeaponDelta,
            clamp: None,
            parent_axis: axis,
            view_fraction: if seat == 0 {
                motion.view_influence
            } else {
                1.0
            },
            use_tag_angles: false,
            link_axis: math_iw4::angles_to_axis([0.0; 3]),
            restore_view,
        },
    );
    let player = Value::Object(object);
    raise(
        world,
        player.clone(),
        "enter_vehicle",
        vec![Value::Int(seat as i32), receiver.clone()],
    );
    raise(
        world,
        receiver.clone(),
        "enter_vehicle",
        vec![Value::Int(seat as i32), player],
    );
    Ok(Value::Undefined)
}

/// Takes a player out of a seat: their controls come back, both hear
/// `exit_vehicle` (seat, the other).
fn leave_seat(world: &mut World, vehicle: u64, seat: usize, client: u32) {
    if let Some(row) = seats(world).seats.get_mut(&vehicle) {
        row[seat] = None;
    }
    let id = crate::ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    crate::game_move::set_script_flag(&mut frame, id, ScriptFlag::RemoteVehicle, false);
    let player = super::super::players::player_object(world, client);
    if world
        .resource::<RoundScript>()
        .entities
        .contains_key(&vehicle)
    {
        raise(
            world,
            player.clone(),
            "exit_vehicle",
            vec![Value::Int(seat as i32), Value::Object(vehicle)],
        );
        raise(
            world,
            Value::Object(vehicle),
            "exit_vehicle",
            vec![Value::Int(seat as i32), player],
        );
    }
}

/// `vehicle setviewclamp(player, right, left, up, down)`: arcs from 0 to 180.
fn set_view_clamp(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if !(3..=5).contains(&args.len()) {
        return Err("SetViewClamp:  Does not have manditory parameters".into());
    }
    let object = super::engine::entity_id(world, arg(args, 0)?)?;
    let client = world
        .resource::<RoundScript>()
        .player_client(object)
        .ok_or("SetViewClamp: not a player")?;
    let arc = |index: usize| -> Result<f32, String> {
        Ok(optional(args, index, float)?
            .unwrap_or(180.0)
            .clamp(0.0, 180.0))
    };
    let clamp = [-arc(3)?, arc(4)?, -arc(1)?, arc(2)?];
    if let Some(link) = world
        .resource_mut::<RoundScript>()
        .players
        .get_mut(&client)
        .and_then(|slot| slot.link.as_mut())
    {
        link.clamp = Some(clamp);
    }
    Ok(Value::Undefined)
}

/// The pilot's controls this frame; their command's sticks and keys.
fn pilot_controls(world: &mut World, client: u32) -> Option<HeliPilot> {
    let id = crate::ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    let held = crate::script_player::buttons(&mut frame, id);
    let view = frame.player(id)?.viewangles;
    let command = frame
        .ecs()
        .resource::<crate::step::StepRequest>()
        .input
        .cmds
        .iter()
        .rev()
        .find(|command| command.client == id)
        .map(|command| command.command);
    let time_ms = frame.entity_kernel().level_time_ms();
    Some(HeliPilot {
        forward: command.map_or(0, |cmd| cmd.forwardmove),
        right: command.map_or(0, |cmd| cmd.rightmove),
        yaw_axis: command.map_or(0, |cmd| cmd.remote_control[1] as i8),
        climb: held & crate::script::command_buttons("+gostand") != 0,
        descend: held & crate::script::command_buttons("+movedown") != 0,
        view,
        time_ms,
    })
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
        if let Some(row) = seats(world).seats.remove(&id) {
            for (seat, client) in row.iter().enumerate() {
                if let Some(client) = client {
                    leave_seat(world, id, seat, *client);
                    super::super::players::unlink_player(world, *client);
                }
            }
        }
        return true;
    }
    let Some(movement) = crate::game_move::rules(&FrameWorld::from_world(world)) else {
        return true;
    };
    // A seat ends with the player's link (`unlink`).
    let row = seats(world)
        .seats
        .get(&id)
        .copied()
        .unwrap_or([None; SEATS]);
    for (seat, client) in row.iter().enumerate() {
        if let Some(client) = client
            && world
                .resource::<RoundScript>()
                .players
                .get(client)
                .is_none_or(|slot| slot.link.is_none())
        {
            leave_seat(world, id, seat, *client);
        }
    }
    // BlackOpsMP.exe's path frame comes first when the path has started.
    let mut path_frame = None;
    if super::t5_mp_vehpath::is_running(world, id) {
        heli.on_path = true;
        path_frame = super::t5_mp_vehpath::frame(world, id);
        if let Some(path) = &path_frame {
            heli.velocity = std::array::from_fn(|i| (path.origin[i] - heli.origin[i]) * 20.0);
            heli.origin = path.origin;
            heli.angles = path.angles;
            heli.speed = path.speed;
        }
    } else {
        heli.on_path = false;
    }
    let driver = seats(world).seats.get(&id).and_then(|row| row[0]);
    heli.piloted.pilot = driver.and_then(|client| pilot_controls(world, client));
    gunner_frame(world, id, &heli);
    // BlackOpsMP.exe's fire think: attack held with the fire time run out says
    // `turret_fire` to the vehicle; its `fireweapon` starts the fire time.
    let left = seats(world).fire_ms.entry(id).or_insert(0);
    *left = (*left - crate::MATCH_TICK_MS as i32).max(0);
    let ready = *left <= 0;
    // The gun aims where the pilot looks, up to 10240 ahead of their view.
    if let Some(client) = driver {
        let frame = FrameWorld::from_world(world);
        let id_client = crate::ClientId(client);
        let view = frame
            .client_meta(id_client)
            .and_then(|meta| meta.linked_weapon_view);
        if let (Some(view), Some(pilot)) = (view, heli.piloted.pilot) {
            let (forward, _, _) = math_iw4::angle_vectors(pilot.view);
            let end: [f32; 3] = std::array::from_fn(|i| view.origin[i] + forward[i] * AIM_RANGE);
            let hit = frame.trace_world(view.origin, end, [0.0; 3], [0.0; 3], AIM_MASK);
            heli.turret_aim = Some(HeliAim::Point(std::array::from_fn(|i| {
                view.origin[i] + (end[i] - view.origin[i]) * hit.fraction
            })));
        }
    }
    if ready
        && let Some(client) = driver
        && heli_has_weapon(world, id)
    {
        let held = crate::script_player::buttons(
            &mut FrameWorld::from_world(world),
            crate::ClientId(client),
        );
        if held & crate::script::command_buttons("+attack") != 0 {
            let player = super::super::players::player_object(world, client);
            raise(world, Value::Object(id), "turret_fire", vec![player]);
        }
    }
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
    if let Some(path) = path_frame {
        for node in path.reached {
            raise(
                world,
                receiver.clone(),
                "reached_node",
                vec![Value::Object(node)],
            );
        }
        if path.ended {
            raise(world, receiver.clone(), "reached_end_node", Vec::new());
        }
    }
    if let Some(normal) = notes.collision {
        raise(
            world,
            receiver.clone(),
            "veh_collision",
            vec![Value::Vector(normal), Value::Vector(heli.velocity)],
        );
    }
    if let Some(normal) = notes.predicted_collision {
        raise(
            world,
            receiver.clone(),
            "veh_predictedcollision",
            vec![Value::Vector(normal), Value::Vector(heli.velocity)],
        );
    }
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

fn heli_has_weapon(world: &World, id: u64) -> bool {
    world
        .resource::<RoundScript>()
        .vehicles
        .get(&id)
        .is_some_and(|heli| heli.weapon.is_some())
}

/// The smallest distance from a place to a side of the height lock it is in;
/// 0 outside them all.
fn height_lock_depth(world: &mut World, origin: [f32; 3]) -> i32 {
    for (mins, maxs) in height_locks(world) {
        if (0..2).all(|i| mins[i] <= origin[i] && origin[i] <= maxs[i]) {
            let distance = (origin[0] - mins[0])
                .min(maxs[0] - origin[0])
                .min(origin[1] - mins[1])
                .min(maxs[1] - origin[1]);
            return distance as i32;
        }
    }
    0
}

/// `vehicle isvehicleimmunetodamage(flags, means, weapon)`: every vehicle file
/// in the multiplayer zones takes all six kinds of damage, so what is left of
/// BlackOpsMP.exe's test is the means it never takes.
fn immune_to_damage(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let means = string(args, 1)?;
    let remote = flight(world, receiver)?.piloted.motion.remote_control;
    let immune = match &*means {
        "MOD_PISTOL_BULLET"
        | "MOD_RIFLE_BULLET"
        | "MOD_GRENADE"
        | "MOD_GRENADE_SPLASH"
        | "MOD_PROJECTILE"
        | "MOD_IMPACT"
        | "MOD_PROJECTILE_SPLASH"
        | "MOD_EXPLOSIVE" => false,
        "MOD_BURNED" => !remote,
        _ => true,
    };
    Ok(Value::Int(i32::from(immune)))
}

/// The share of damage a vehicle takes by the side a point lies to: its
/// `vehicle_damage_zone_*` dvars, a splash blending the two sides it lies
/// between; a splash from under it takes the under zone's.
fn damage_share(world: &World, heli: &HeliMove, point: Option<[f32; 3]>, splash: bool) -> f32 {
    let dvar = |name: &str, default: f32| {
        world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get(name)
            .and_then(|value| value.to_string().parse().ok())
            .unwrap_or(default)
    };
    let (front, side, rear) = (
        dvar("vehicle_damage_zone_front", 1.0),
        dvar("vehicle_damage_zone_side", 1.2),
        dvar("vehicle_damage_zone_rear", 2.0),
    );
    let (delta_x, delta_y) = point.map_or((1.0, 0.0), |at| {
        (at[0] - heli.origin[0], at[1] - heli.origin[1])
    });
    let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
    let scale = if length <= 0.0 { 1.0 } else { 1.0 / length };
    let (forward, right, _) = math_iw4::angle_vectors(heli.angles);
    let along = forward[0] * scale * delta_x + forward[1] * scale * delta_y;
    let across = right[0] * scale * delta_x + right[1] * scale * delta_y;
    let sideways = along.abs() < across.abs();
    let dominant = if sideways { across } else { along };
    if !splash {
        if !sideways {
            return if dominant >= 0.0 { front } else { rear };
        }
        return side;
    }
    let (near, far) = if sideways {
        (side, if along < 0.0 { rear } else { front })
    } else {
        (if dominant >= 0.0 { front } else { rear }, side)
    };
    ((1.0 - dominant.abs()) * far + near * dominant.abs()) * SPLASH_SHARE
}

/// `vehicle finishvehicledamage(inflictor, attacker, damage, flags, means,
/// weapon, point, dir, hitloc, offset, under, modelindex, part, friendly)`: the
/// damage scaled by side comes off the entity's health and it hears `damage`.
fn finish_damage(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let damage = int(args, 2)?;
    if damage <= 0 {
        return Ok(Value::Undefined);
    }
    let attacker = args.get(1).cloned().unwrap_or(Value::Undefined);
    let means = string(args, 4)?;
    let point = optional(args, 6, vector)?;
    let dir = args.get(7).cloned().unwrap_or(Value::Undefined);
    let heli = *flight(world, receiver)?;
    let splash = matches!(&*means, "MOD_GRENADE_SPLASH" | "MOD_PROJECTILE_SPLASH");
    let dealt = (damage as f32 * damage_share(world, &heli, point, splash)) as i32;
    let weapon = args.get(5).cloned().unwrap_or(Value::Undefined);
    apply_damage(world, receiver, dealt, attacker, dir, point, &means, weapon);
    Ok(Value::Undefined)
}

/// `vehicle finishvehicleradiusdamage(inflictor, attacker, damage, inner,
/// outer, flags, means, weapon, point, radius, cone, conedir, offset)`.
fn finish_radius_damage(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let attacker = args.get(1).cloned().unwrap_or(Value::Undefined);
    let (inner, outer) = (float(args, 3)?, float(args, 4)?);
    let means = string(args, 6)?;
    let point = optional(args, 8, vector)?;
    let radius = float(args, 9)?;
    let heli = *flight(world, receiver)?;
    let distance = point.map_or(0.0, |at| {
        (0..3)
            .map(|i| (at[i] - heli.origin[i]) * (at[i] - heli.origin[i]))
            .sum::<f32>()
            .sqrt()
    });
    if radius <= 0.0 || distance >= radius {
        return Ok(Value::Undefined);
    }
    let at_range = inner + (outer - inner) * distance / radius;
    let dealt = (at_range * damage_share(world, &heli, point, true)) as i32;
    let weapon = args.get(7).cloned().unwrap_or(Value::Undefined);
    apply_damage(
        world,
        receiver,
        dealt,
        attacker,
        Value::Undefined,
        point,
        &means,
        weapon,
    );
    Ok(Value::Undefined)
}

fn apply_damage(
    world: &mut World,
    receiver: &Value,
    dealt: i32,
    attacker: Value,
    dir: Value,
    point: Option<[f32; 3]>,
    means: &str,
    weapon: Value,
) {
    let Value::Object(id) = receiver else {
        return;
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Value::Int(health) = runtime.object_field(*id, "health") {
        runtime.set_object_field(*id, "health", Value::Int(health - dealt));
    }
    raise(
        world,
        receiver.clone(),
        "damage",
        vec![
            Value::Int(dealt),
            attacker,
            dir,
            point.map_or(Value::Undefined, Value::Vector),
            Value::string(means),
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            weapon,
        ],
    );
}

fn register_seats(registry: &mut NativeRegistry) {
    registry.register(Method, "attachpath", |world, receiver, args| {
        let vehicle = vehicle_id(receiver)?;
        let node = super::engine::entity_id(world, arg(args, 0)?)?;
        super::t5_mp_vehpath::attach(world, vehicle, node)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "startpath", |world, receiver, _| {
        super::t5_mp_vehpath::start(world, vehicle_id(receiver)?)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isvehicleimmunetodamage", immune_to_damage);
    registry.register(Method, "finishvehicledamage", finish_damage);
    registry.register(Method, "finishvehicleradiusdamage", finish_radius_damage);
    // `+speed_throw` presses aim and throw together; BlackOpsMP.exe's handler
    // reads the throw half.
    registry.register(Method, "throwbuttonpressed", |world, receiver, _| {
        let object = super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(object) else {
            return Err("not a player".into());
        };
        let held = crate::script_player::buttons(
            &mut FrameWorld::from_world(world),
            crate::ClientId(client),
        );
        Ok(Value::Int(i32::from(
            held & crate::script::command_buttons("+speed_throw") != 0,
        )))
    });
    registry.register(Method, "usevehicle", use_vehicle);
    registry.register(Method, "setviewclamp", set_view_clamp);
    registry.register(Method, "resetviewclamp", |world, _, args| {
        let object = super::engine::entity_id(world, arg(args, 0)?)?;
        let client = world
            .resource::<RoundScript>()
            .player_client(object)
            .ok_or("ResetViewClamp: Player not on vehicle.")?;
        if let Some(link) = world
            .resource_mut::<RoundScript>()
            .players
            .get_mut(&client)
            .and_then(|slot| slot.link.as_mut())
        {
            link.clamp = None;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "returnplayercontrol", |world, receiver, _| {
        let heli = flight(world, receiver)?;
        heli.has_goal = false;
        heli.hovering = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "makevehicleunusable", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Method, "makevehicleusable", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "setjitterparams", |world, receiver, args| {
        let amplitude = vector(args, 0)?;
        let min = optional(args, 1, float)?.unwrap_or(0.5);
        let max = optional(args, 2, float)?.unwrap_or(1.0);
        let jitter = &mut flight(world, receiver)?.piloted.jitter;
        jitter.amplitude = amplitude;
        jitter.min_ms = (min * 1000.0) as i32;
        jitter.max_ms = (max * 1000.0) as i32;
        jitter.next_ms = 0;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setheliheightlock", |world, receiver, args| {
        flight(world, receiver)?.height_lock = int(args, 0)? != 0;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isinsideheliheightlock", |world, receiver, _| {
        let origin = flight(world, receiver)?.origin;
        Ok(Value::Int(height_lock_depth(world, origin)))
    });
    registry.register(Method, "getoccupantseat", |world, receiver, args| {
        let vehicle = vehicle_id(receiver)?;
        let object = super::engine::entity_id(world, arg(args, 0)?)?;
        let Some(client) = world.resource::<RoundScript>().player_client(object) else {
            return Ok(Value::Undefined);
        };
        Ok(match seat_of(world, client) {
            Some((at, seat)) if at == vehicle => Value::Int(seat as i32),
            _ => Value::Undefined,
        })
    });
    registry.register(Method, "getseatoccupant", |world, receiver, args| {
        let vehicle = vehicle_id(receiver)?;
        let seat = int(args, 0)? as usize;
        let client = seats(world)
            .seats
            .get(&vehicle)
            .and_then(|row| row.get(seat).copied().flatten());
        Ok(client.map_or(Value::Undefined, |client| {
            super::super::players::player_object(world, client)
        }))
    });
    registry.register(Method, "getvehoccupants", |world, receiver, _| {
        let vehicle = vehicle_id(receiver)?;
        let row = seats(world)
            .seats
            .get(&vehicle)
            .copied()
            .unwrap_or([None; SEATS]);
        let list: Vec<Value> = row
            .iter()
            .flatten()
            .map(|client| super::super::players::player_object(world, *client))
            .collect();
        new_array(world, list)
    });
    registry.register(Method, "getvehicleoccupied", |world, receiver, _| {
        let object = super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(object) else {
            return Err("not a player".into());
        };
        Ok(seat_of(world, client).map_or(Value::Undefined, |(vehicle, _)| Value::Object(vehicle)))
    });
    registry.register(Method, "isremotecontrolling", |world, receiver, _| {
        let object = super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(object) else {
            return Err("not a player".into());
        };
        let frame = FrameWorld::from_world(world);
        let remote = crate::game_move::status(&frame, crate::ClientId(client))
            .is_some_and(|status| status.remote_control);
        Ok(Value::Int(i32::from(remote)))
    });
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    register_seats(registry);
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
    registry.register(Method, "resumespeed", |world, receiver, args| {
        if let Value::Object(vehicle) = receiver {
            let rate = optional(args, 0, float)?.map_or(MPH, |rate| rate * MPH);
            super::t5_mp_vehpath::resume_path_speed(world, *vehicle, rate);
        }
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
