//! Black Ops 2's `vibrate`: a script mover's angles swing on a sine
//! trajectory, then return to where they started.

use super::{float, vector};
use crate::script::host::mechanics::{Mechanics, Motion, MotionPath};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

const MOVERS: [&str; 4] = [
    "script_brushmodel",
    "script_model",
    "script_origin",
    "light",
];

fn normalize_180(angle: f32) -> f32 {
    let turns = angle * 0.002_777_777_8;
    (turns - (turns + 0.5).floor()) * 360.0
}

fn normalize_360(angle: f32) -> f32 {
    let turns = angle * 0.002_777_777_8;
    let angle = (turns - turns.floor()) * 360.0;
    if angle - 360.0 >= 0.0 {
        angle - 360.0
    } else {
        angle
    }
}

fn sin_cos(degrees: f32) -> (f32, f32) {
    let radians = f64::from(degrees * 0.017_453_292);
    (radians.sin() as f32, radians.cos() as f32)
}

/// Forward and left of a set of angles.
fn forward_left(angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (sy, cy) = sin_cos(angles[1]);
    let (sp, cp) = sin_cos(angles[0]);
    let (sr, cr) = sin_cos(angles[2]);
    (
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
    )
}

fn mover_id(world: &World, receiver: &Value) -> Result<u64, String> {
    let id = match receiver {
        Value::Object(id) if world.resource::<RoundScript>().entities.contains_key(id) => *id,
        _ => return Err("not an entity".into()),
    };
    let classname = world.resource::<RoundScript>().entities[&id]
        .classname
        .clone();
    if !MOVERS.contains(&&*classname) {
        return Err(format!(
            "entity {id} is not a script_brushmodel, script_model, script_origin, or light"
        ));
    }
    Ok(id)
}

fn vibrate(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = mover_id(world, receiver)?;
    let direction = vector(args, 0)?;
    let amplitude = float(args, 1)?;
    let period = float(args, 2)?;
    let time = float(args, 3)?;
    let length =
        (direction[1] * direction[1] + direction[0] * direction[0] + direction[2] * direction[2])
            .sqrt();
    let scale = 1.0 / if -length >= 0.0 { 1.0 } else { length };
    let delta = direction.map(|c| c * scale * amplitude);
    let angles = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "angles")
    {
        Value::Vector(angles) => angles,
        _ => [0.0; 3],
    };
    let (forward, left) = forward_left(angles);
    let now = super::super::super::players::now_ms(world);
    let phase_ms = now - i64::from((time * -1000.0) as i32);
    let period_ms = (period * 1000.0) as i32;
    let path = MotionPath::Sine {
        base: angles,
        delta: [
            forward[1] * delta[1] + forward[0] * delta[0] + forward[2] * delta[2],
            0.0,
            -(left[1] * delta[1] + left[0] * delta[0] + left[2] * delta[2]),
        ],
        phase_ms,
        period_ms,
        end: [
            normalize_180(angles[0]),
            normalize_360(angles[1]),
            normalize_180(angles[2]),
        ],
    };
    world.resource_mut::<Mechanics>().start(
        id,
        Motion {
            field: "angles",
            path,
            start_ms: now,
            duration_ms: phase_ms + i64::from(period_ms) - now,
            done: "rotatedone",
        },
    );
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "vibrate", vibrate);
    registry.register(Method, "movegravity", |world, receiver, args| {
        let id = mover_id(world, receiver)?;
        let velocity = vector(args, 0)?;
        if velocity.iter().any(|c| !c.is_finite()) {
            return Err(format!(
                "invalid velocity parameter in movegravity command: {} {} {}",
                velocity[0], velocity[1], velocity[2]
            ));
        }
        let time = float(args, 1)?;
        let from = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let now = super::super::super::players::now_ms(world);
        world.resource_mut::<Mechanics>().start(
            id,
            Motion {
                field: "origin",
                path: MotionPath::Ballistic { from, velocity },
                start_ms: now,
                duration_ms: i64::from((time * 1000.0) as i32),
                done: "movedone",
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "rotatevelocity", |world, receiver, args| {
        mover_id(world, receiver)?;
        let velocity = vector(args, 0)?;
        let mut time = float(args, 1)?;
        if time <= 0.0 {
            return Err("total time must be positive".into());
        }
        if time < 0.001 {
            time = 0.001;
        }
        let accel = if args.len() >= 3 {
            let a = float(args, 2)?;
            if a < 0.0 {
                return Err("accel time must be nonnegative".into());
            }
            a
        } else {
            0.0
        };
        let decel = if args.len() >= 4 {
            let d = float(args, 3)?;
            if d < 0.0 {
                return Err("decel time must be nonnegative".into());
            }
            d
        } else {
            0.0
        };
        if time < accel + decel {
            let scaled = time * 1.000_000_5;
            if scaled < accel + decel {
                return Err("accel time plus decel time is greater than total time".into());
            }
            time = scaled;
        }
        let rotate = NativeRegistry::engine_services()
            .get(Method, "rotatevelocity")
            .ok_or("rotatevelocity: no engine rotate")?;
        let mut forwarded = vec![Value::Vector(velocity), Value::Float(time)];
        if args.len() >= 3 {
            forwarded.push(Value::Float(accel));
        }
        if args.len() >= 4 {
            forwarded.push(Value::Float(decel));
        }
        rotate(world, receiver, &forwarded)
    });
    registry.register(Method, "setcontents", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let contents = super::int(args, 0)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let entity = runtime.entities.get_mut(&id).ok_or("not an entity")?;
        let previous = match entity.contents {
            0 if entity.solid => crate::bullet_collision::CONTENTS_SOLID as i32,
            c => c,
        };
        entity.contents = contents;
        entity.solid = contents != 0;
        Ok(Value::Int(previous))
    });
    registry.register(Method, "launch", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)
            .map_err(|_| String::from("not an entity"))?;
        let velocity = vector(args, 0)?;
        if velocity.iter().any(|c| !c.is_finite()) {
            return Err(format!(
                "invalid velocity parameter in launch command: {} {} {}",
                velocity[0], velocity[1], velocity[2]
            ));
        }
        if args.len() > 1 {
            vector(args, 1)?;
        }
        let from = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let now = super::super::super::players::now_ms(world);
        world.resource_mut::<Mechanics>().start(
            id,
            Motion {
                field: "origin",
                path: MotionPath::Ballistic { from, velocity },
                start_ms: now,
                duration_ms: 10_000,
                done: "movedone",
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "useby", |world, receiver, args| {
        let _id = super::super::engine::entity_id(world, receiver)?;
        let user = super::super::engine::entity_id(
            world,
            args.first().ok_or("parameter 1 does not exist")?,
        )?;
        crate::script::runtime::raise(
            world,
            receiver.clone(),
            "trigger",
            vec![Value::Object(user)],
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "istouchingswept", |world, receiver, args| {
        let a = super::super::engine::entity_id(world, receiver)?;
        let b = super::super::engine::entity_id(
            world,
            args.first().ok_or("parameter 1 does not exist")?,
        )?;
        let min_z = if args.len() > 1 {
            Some(float(args, 1)?)
        } else {
            None
        };
        let max_z = if args.len() > 2 {
            Some(float(args, 2)?)
        } else {
            None
        };
        let hit = super::super::super::triggers::is_touching_swept(world, a, b, min_z, max_z)?;
        Ok(Value::Int(hit.into()))
    });
}
