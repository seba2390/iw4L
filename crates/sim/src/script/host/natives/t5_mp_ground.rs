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

/// Each driven ground vehicle's speed along its heading.
#[derive(Resource, Default)]
struct GroundDrives(BTreeMap<u64, f32>);

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
            .copied()
            .unwrap_or(0.0);
        let wanted = f32::from(forward) / FULL_STICK * motion.max_speed;
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
        match entity_trace(world, top, bottom, MASK_PLAYER_SOLID, ignore) {
            TraceOutcome::Hit {
                fraction, normal, ..
            } => {
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
            .insert(id, speed);
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(next));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.on_tick(drive);
}
