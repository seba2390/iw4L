//! A ground vehicle under its driver: Black Ops' RC-XD. `rc_car_medium` is a
//! rigid body in Black Ops (`isNitrous`); here it rolls along the ground by the
//! file's top speed and acceleration, stopped by what it hits
//! (`t5.mp.rcxd_physics`).

use super::engine::{entity_trace, trace_ignore};
use crate::bullet_collision::{MASK_PLAYER_SOLID, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::host::entities::EntityKind;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const TICK: f32 = 0.05;
/// Degrees a second at full lock and speed.
const STEER_RATE: f32 = 130.0;
/// The speed under which steering fades away.
const STEER_FULL_SPEED: f32 = 120.0;
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
/// Under this share of its top speed a full throttle spins the wheels.
const PEEL_OUT_FRACTION: f32 = 0.25;
/// Over this share of its top speed a full lock slides the back wheels.
const SLIDE_FRACTION: f32 = 0.5;

/// A driven ground vehicle's frame.
#[derive(Clone, Copy, Debug, Default)]
struct GroundDrive {
    /// Along its heading, backwards below zero.
    speed: f32,
    throttle: f32,
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
        let Some((forward, right)) = super::t5_mp_heli::driver_sticks(world, client) else {
            continue;
        };
        let origin = vec_field(world, id, "origin");
        let mut angles = vec_field(world, id, "angles");
        let mut speed = world
            .get_resource_or_insert_with(GroundDrives::default)
            .0
            .get(&id)
            .map_or(0.0, |drive| drive.speed);
        let top_speed = if forward < 0 {
            motion.max_speed * motion.reverse_scale
        } else {
            motion.max_speed
        };
        let wanted = f32::from(forward) / FULL_STICK * top_speed;
        let step = motion.accel.max(1.0) * TICK;
        speed += (wanted - speed).clamp(-step, step);
        let steer = f32::from(right) / FULL_STICK
            * STEER_RATE
            * TICK
            * (speed / STEER_FULL_SPEED).clamp(-1.0, 1.0);
        angles[1] -= steer;
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
            speed = 0.0;
            next = origin;
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
                    throttle: f32::from(forward) / FULL_STICK,
                    surface,
                    sliding: speed.abs() > motion.max_speed * SLIDE_FRACTION
                        && f32::from(right).abs() >= FULL_STICK,
                    peeling: f32::from(forward).abs() >= FULL_STICK
                        && speed.abs() < motion.max_speed * PEEL_OUT_FRACTION,
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
