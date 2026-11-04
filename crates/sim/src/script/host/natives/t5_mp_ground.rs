//! A ground vehicle under its driver: Black Ops' RC-XD. `rc_car_medium` is a
//! rigid body in Black Ops (`isNitrous`); here it rolls along the ground
//! (`t5.mp.rcxd_physics`). What the driver's command asks of it is Black Ops'
//! own: in the `view` camera mode the car turns toward where its driver looks,
//! backs up when that is behind it, and brakes into a sharp turn.

use super::engine::{entity_trace, trace_ignore};
use crate::bullet_collision::{MASK_PLAYER_SOLID, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::host::entities::EntityKind;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const TICK: f32 = 0.05;
/// How far above the ground the wheels' trace starts and below it looks.
const GROUND_UP: f32 = 18.0;
const GROUND_DOWN: f32 = 40.0;
/// How fast it falls where no ground is under it.
const FALL_STEP: f32 = 12.0;
/// Where its body is from its origin.
const BODY_HALF_WIDTH: f32 = 10.0;
const BODY_HALF_LENGTH: f32 = 14.0;
const BODY_HEIGHT: f32 = 10.0;
/// A stick at full throw.
const FULL_STICK: f32 = 127.0;
/// The `view` camera mode's index among Black Ops' `cameraMode`s.
const CAMERA_MODE_VIEW: u8 = 2;
/// The stick's dead zone and the scale past it.
const STICK_DEAD_ZONE: f32 = 40.0;
const STICK_SCALE: f32 = 1.0 / 88.0;
/// A forward move at or below this asks for reverse gas.
const REVERSE_STICK: i8 = -41;
/// The car backs up when its wanted yaw is more than this, plus this much
/// per stick throw, off its nose; more when it is faster than this.
const REVERSE_ARC: f32 = 100.0;
const REVERSE_ARC_FAST: f32 = 130.0;
const REVERSE_ARC_THROW: f32 = 55.0;
const REVERSE_ARC_FAST_SPEED: f32 = 90.0;
/// Moving faster than this the other way, it steers the other way.
const AGAINST_SPEED: f32 = 25.0;
/// A turn sharper than this, faster than `autoHandbrakeMinSpeed`, brakes.
const BRAKE_TURN: f32 = 30.0;

/// Black Ops' vehicle classes, by a vehicle file's `type`.
const VEHICLE_CLASSES: [&str; 7] = [
    "4 wheel",
    "motorcycle",
    "tank",
    "plane",
    "boat",
    "artillery",
    "helicopter",
];

/// A driven ground vehicle's frame.
#[derive(Clone, Copy, Debug, Default)]
struct GroundDrive {
    /// Along its heading, backwards below zero.
    speed: f32,
    throttle: f32,
    /// The front wheels' angle, radians, left above zero.
    wheel_angle: f32,
    /// How fast it turns, degrees a second.
    yaw_rate: f32,
    /// What the wheels are on, when they are on anything.
    surface: Option<&'static str>,
    sliding: bool,
    peeling: bool,
}

/// Each driven ground vehicle.
#[derive(Resource, Default)]
struct GroundDrives(BTreeMap<u64, GroundDrive>);

/// What a client knows of a vehicle in its snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ClientVehicle {
    pub(crate) vehicle_type: String,
    pub(crate) class: &'static str,
    pub(crate) max_speed: f32,
    pub(crate) max_reverse_speed: f32,
    pub(crate) speed: f32,
    pub(crate) throttle: f32,
    pub(crate) surface: Option<&'static str>,
    pub(crate) sliding: bool,
    pub(crate) peeling: bool,
    /// The driver's client number.
    pub(crate) driver: Option<u32>,
}

/// The vehicle fields a client's snapshot carries.
pub(crate) fn client_vehicle(world: &mut World, id: u64) -> Option<ClientVehicle> {
    let Value::String(kind) = world
        .resource_mut::<RoundScript>()
        .object_field(id, "vehicletype")
    else {
        return None;
    };
    let motion = FrameWorld::from_world(world).vehicle_motion(&kind);
    let drive = world
        .get_resource::<GroundDrives>()
        .and_then(|drives| drives.0.get(&id).copied())
        // A car nobody has driven yet stands still on nothing.
        .unwrap_or(GroundDrive {
            speed: 0.0,
            throttle: 0.0,
            wheel_angle: 0.0,
            yaw_rate: 0.0,
            surface: None,
            sliding: false,
            peeling: false,
        });
    let driver = super::t5_mp_heli::driver_of(world, id);
    let (max_speed, reverse_scale, class) =
        motion.map_or((0.0, 0.0, 0), |m| (m.max_speed, m.reverse_scale, m.class));
    Some(ClientVehicle {
        vehicle_type: kind.to_string(),
        class: VEHICLE_CLASSES
            .get(usize::from(class))
            .copied()
            .unwrap_or(VEHICLE_CLASSES[0]),
        max_speed,
        max_reverse_speed: max_speed * reverse_scale,
        speed: drive.speed,
        throttle: drive.throttle,
        surface: drive.surface,
        sliding: drive.sliding,
        peeling: drive.peeling,
        driver,
    })
}

fn vec_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// What the driver's command asks of a car in the `view` camera mode.
struct ViewControls {
    /// The yaw the car turns toward.
    yaw: f32,
    /// The stick's throw past its dead zone, 0 to 1.
    throw: f32,
    /// Forward or reverse gas.
    gas: f32,
}

/// The `view` mode's read of the command: the side move does nothing; the
/// car turns toward the driver's view, and the forward move past its dead
/// zone opens the gas, forward or back. Black Ops also turns the wanted yaw
/// half round for a back stick and flips its gas, which its body reads
/// (#896); with the body a stand-in, the view's yaw and the stick's sign are
/// kept (IW4L's own choice).
fn view_controls(forward: i8, view_yaw: f32) -> ViewControls {
    let x = f32::from(forward).abs();
    let throw = if x >= STICK_DEAD_ZONE {
        ((x - STICK_DEAD_ZONE) * STICK_SCALE).min(1.0)
    } else {
        0.0
    };
    ViewControls {
        yaw: math_iw4::angle_normalize_360(view_yaw),
        throw,
        gas: if forward > REVERSE_STICK { 1.0 } else { -1.0 },
    }
}

/// How a car turns toward a yaw.
struct Steering {
    /// -1 (full right) to 1 (full left).
    steer: f32,
    /// Gas, backwards below zero.
    gas: f32,
    brake: bool,
}

/// Black Ops' drive toward a yaw: a yaw far behind is reached backing up,
/// with twice the lock; moving the other way fast, the steer flips; a sharp
/// turn at speed brakes unless the car already turns that way.
fn steer_toward(
    controls: &ViewControls,
    body_yaw: f32,
    speed: f32,
    yaw_rate: f32,
    motion: &game_api::heli::VehicleMotion,
) -> Steering {
    let mut delta = math_iw4::angle_subtract(controls.yaw, body_yaw);
    let slack = if speed > REVERSE_ARC_FAST_SPEED {
        REVERSE_ARC_FAST
    } else {
        REVERSE_ARC
    };
    let reversing = delta.abs() > REVERSE_ARC_THROW * controls.throw + slack;
    if reversing {
        delta -= delta.signum() * 180.0;
    }
    let direction = if reversing { -1.0 } else { 1.0 };
    let gas = direction * controls.gas;
    if gas != 0.0
        && ((speed > AGAINST_SPEED && reversing) || (speed < -AGAINST_SPEED && !reversing))
    {
        delta = -delta;
    }
    let side = if delta > 0.0 {
        1.0
    } else if delta < 0.0 {
        -1.0
    } else {
        0.0
    };
    let mut steer = side * direction;
    let mut lock = motion.steer_angle_max.to_degrees();
    if reversing {
        lock *= 2.0;
    }
    if delta.abs() < lock {
        steer *= delta.abs() / lock;
    }
    let turning_away = (delta < 0.0 && yaw_rate > 0.0) || (delta > 0.0 && yaw_rate < 0.0);
    Steering {
        steer: steer.clamp(-1.0, 1.0),
        gas,
        brake: delta.abs() > BRAKE_TURN
            && speed.abs() > motion.auto_handbrake_min_speed
            && !turning_away,
    }
}

/// The frame of every ground vehicle with a driver.
fn drive(world: &mut World) {
    super::t5_mp_heli::end_ground_seats(world);
    let vehicles: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(id, entity)| {
                entity.kind == EntityKind::Vehicle && !runtime.vehicles.contains_key(id)
            })
            .map(|(id, _)| *id)
            .collect()
    };
    for id in vehicles {
        let Some(client) = super::t5_mp_heli::driver_of(world, id) else {
            continue;
        };
        let Value::String(kind) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "vehicletype")
        else {
            continue;
        };
        let Some(motion) = FrameWorld::from_world(world).vehicle_motion(&kind) else {
            continue;
        };
        let Some((forward, _, view_yaw)) = super::t5_mp_heli::driver_command(world, client) else {
            continue;
        };
        let origin = vec_field(world, id, "origin");
        let mut angles = vec_field(world, id, "angles");
        let last = world
            .get_resource_or_insert_with(GroundDrives::default)
            .0
            .get(&id)
            .copied();
        let mut speed = last.map_or(0.0, |drive| drive.speed);
        let yaw_rate = last.map_or(0.0, |drive| drive.yaw_rate);
        let mut wheel_angle = last.map_or(0.0, |drive| drive.wheel_angle);
        // Other camera modes read the command their own way; only `view` (the
        // RC-XD's) is ported.
        let controls = if motion.camera_mode == CAMERA_MODE_VIEW {
            view_controls(forward, view_yaw)
        } else {
            ViewControls {
                yaw: angles[1],
                throw: (f32::from(forward) / FULL_STICK).abs(),
                gas: f32::from(forward).signum(),
            }
        };
        let steering = steer_toward(&controls, angles[1], speed, yaw_rate, &motion);
        // Stand-in: how far the gas opens is the stick's throw, until the
        // body reads Black Ops' gas (#896).
        let throttle = steering.gas * controls.throw;
        let top_speed = if throttle < 0.0 {
            motion.max_speed * motion.reverse_scale
        } else {
            motion.max_speed
        };
        let gravity = super::super::actor_nav::dvar_f32(world, "g_gravity", 800.0);
        let (wanted, step) = if steering.brake {
            (0.0, motion.tire_fric_brake.max(0.0) * gravity * TICK)
        } else {
            (throttle * top_speed, motion.accel.max(1.0) * TICK)
        };
        speed += (wanted - speed).clamp(-step, step);
        // The front wheels swing toward the steer at `nitrous_steer_speed`
        // locks a second, and the car turns on them as a bicycle would, as
        // far as its side grip lets it.
        let target_angle = steering.steer * motion.steer_angle_max;
        let swing = motion.steer_speed.max(0.0) * motion.steer_angle_max * TICK;
        wheel_angle += (target_angle - wheel_angle).clamp(-swing, swing);
        let wheelbase = (motion.body_length - 2.0 * motion.wheel_radius).max(1.0);
        let mut turn = speed * wheel_angle.tan() / wheelbase;
        let grip = motion.tire_fric_side.max(0.0) * gravity / speed.abs().max(1.0);
        let sliding = turn.abs() > grip;
        turn = turn.clamp(-grip, grip);
        let mut new_yaw_rate = turn.to_degrees();
        let held_yaw = angles[1];
        angles[1] = math_iw4::angle_normalize_360(angles[1] + new_yaw_rate * TICK);
        let (forward_axis, _, _) = math_iw4::angle_vectors([0.0, angles[1], 0.0]);
        let mut next: [f32; 3] =
            std::array::from_fn(|i| origin[i] + forward_axis[i] * speed * TICK);
        let ignore = trace_ignore(world, Some(&Value::Object(id)));
        // Its nose and its flanks, at wheel height.
        let (sin, cos) = angles[1].to_radians().sin_cos();
        let blocked = [0.0f32, BODY_HALF_WIDTH, -BODY_HALF_WIDTH]
            .into_iter()
            .any(|side| {
                let offset = [-sin * side, cos * side];
                let reach = BODY_HALF_LENGTH * speed.signum();
                let from = [
                    origin[0] + offset[0],
                    origin[1] + offset[1],
                    origin[2] + BODY_HEIGHT,
                ];
                let to = [
                    next[0] + offset[0] + cos * reach,
                    next[1] + offset[1] + sin * reach,
                    next[2] + BODY_HEIGHT,
                ];
                matches!(
                    entity_trace(world, from, to, MASK_PLAYER_SOLID, ignore),
                    TraceOutcome::Hit { .. } | TraceOutcome::StartSolid { .. }
                )
            });
        if blocked && speed != 0.0 {
            // A car that cannot roll cannot turn either.
            speed = 0.0;
            next = origin;
            angles[1] = held_yaw;
            new_yaw_rate = 0.0;
        }
        let top = [next[0], next[1], next[2] + GROUND_UP];
        let bottom = [next[0], next[1], next[2] - GROUND_DOWN];
        let mut surface = None;
        match entity_trace(world, top, bottom, MASK_PLAYER_SOLID, ignore) {
            TraceOutcome::Hit {
                fraction,
                normal,
                collider,
                ..
            } => {
                surface = Some(super::engine::surface_name(collider));
                next[2] = top[2] + (bottom[2] - top[2]) * fraction;
                let along = normal[0] * cos + normal[1] * sin;
                let across = normal[1] * cos - normal[0] * sin;
                angles[0] = along.atan2(normal[2]).to_degrees();
                angles[2] = across.atan2(normal[2]).to_degrees();
            }
            TraceOutcome::StartSolid { .. } => {}
            _ => next[2] -= FALL_STEP,
        }
        world
            .get_resource_or_insert_with(GroundDrives::default)
            .0
            .insert(
                id,
                GroundDrive {
                    speed,
                    throttle,
                    wheel_angle,
                    yaw_rate: new_yaw_rate,
                    surface,
                    sliding,
                    peeling: throttle.abs() >= 1.0 && speed.abs() < motion.peel_out_max_speed,
                },
            );
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(next));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.on_tick(drive);
}
