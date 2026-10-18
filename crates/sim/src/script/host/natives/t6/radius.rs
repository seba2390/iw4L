//! Black Ops 2's radius damage: `radiusdamage` as `G_RadiusDamage` deals it
//! to the players and actors in range, each scaled by how much of it the
//! blast sees.

use super::super::super::entities::EntityKind;
use super::super::engine::{TraceIgnore, entity_trace};
use super::damage::{self, Hit, MEANS_OF_DEATH};
use super::{float, string, vector};
use crate::bullet_collision::{PLAYER_MAXS, PLAYER_MINS, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// `radiusdamage`'s means of death when it names none: explosive.
const MOD_EXPLOSIVE: usize = 15;
/// Radius damage's flags: radius, and no knockback.
const DFLAGS_RADIUS: i32 = 5;
/// The blast's sight to what it damages.
const VISIBILITY_MASK: u32 = 0x0080_2013;
/// A player is seen at its middle and this far to either side.
const PLAYER_SIDE: f32 = 15.0;
/// The damage aims this far above the target's origin.
const AIM_HEIGHT: f32 = 24.0;

/// Something a blast can damage: a player or an actor.
struct Target {
    value: Value,
    origin: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    /// A player's eye, where its visibility is sampled up to.
    eye: Option<[f32; 3]>,
    ignore: TraceIgnore,
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|c| c * s)
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        scale(v, 1.0 / length)
    } else {
        v
    }
}

fn clear(world: &mut World, from: [f32; 3], to: [f32; 3], ignore: TraceIgnore) -> bool {
    !matches!(
        entity_trace(world, from, to, VISIBILITY_MASK, ignore),
        TraceOutcome::Hit { .. } | TraceOutcome::StartSolid { .. }
    )
}

/// How much of a target a blast sees. An actor: its bounds' middle and the
/// corners of their face toward the blast, all of it if any is in sight. A
/// player: its middle and four points around it up to its eye, a third for
/// each seen up to three, all of it from four.
fn visibility(
    world: &mut World,
    target: &Target,
    blast: [f32; 3],
    cone: Option<(f32, [f32; 3])>,
) -> f32 {
    let centre: [f32; 3] = std::array::from_fn(|i| (target.mins[i] + target.maxs[i]) * 0.5);
    let points: [[f32; 3]; 5] = if let Some(eye) = target.eye {
        let middle = [
            (target.origin[0] + eye[0]) * 0.5,
            (target.origin[1] + eye[1]) * 0.5,
            (target.origin[2] + eye[2]) * 0.5,
        ];
        let half = (eye[2] - target.origin[2]) * 0.5;
        let across = normalize([
            blast[0] - target.origin[0],
            blast[1] - target.origin[1],
            0.0,
        ]);
        let side = [-across[1] * PLAYER_SIDE, across[0] * PLAYER_SIDE, 0.0];
        [
            middle,
            add(add(middle, side), [0.0, 0.0, half]),
            add(add(middle, side), [0.0, 0.0, -half]),
            add(add(middle, scale(side, -1.0)), [0.0, 0.0, half]),
            add(add(middle, scale(side, -1.0)), [0.0, 0.0, -half]),
        ]
    } else {
        let mut dir = normalize(std::array::from_fn(|i| blast[i] - centre[i]));
        if dir[0] * dir[0] + dir[1] * dir[1] < 0.001 {
            dir = [1.0, 0.0, 0.0];
        }
        let right = normalize([-dir[1], dir[0], 0.0]);
        let up = [
            right[1] * dir[2] - right[2] * dir[1],
            right[2] * dir[0] - right[0] * dir[2],
            right[0] * dir[1] - right[1] * dir[0],
        ];
        let half: [f32; 3] = std::array::from_fn(|i| target.maxs[i] - centre[i]);
        let wide = (half[0] * right[0]).abs() + (half[1] * right[1]).abs();
        let high = (half[1] * up[1]).abs() + (half[0] * up[0]).abs() + (half[2] * up[2]).abs();
        let (r, u) = (scale(right, wide), scale(up, high));
        [
            centre,
            add(add(centre, r), u),
            add(add(centre, scale(r, -1.0)), u),
            add(add(centre, r), scale(u, -1.0)),
            add(add(centre, scale(r, -1.0)), scale(u, -1.0)),
        ]
    };
    let seen = points
        .iter()
        .filter(|&&point| {
            if let Some((cos, dir)) = cone {
                let v = normalize(std::array::from_fn(|i| point[i] - blast[i]));
                if cos > dir[0] * v[0] + dir[1] * v[1] + dir[2] * v[2] {
                    return false;
                }
            }
            clear(world, blast, point, target.ignore)
        })
        .count();
    match (target.eye, seen) {
        (_, 0) => 0.0,
        (None, _) => 1.0,
        (Some(_), n) if n < 4 => n as f32 * 0.333_333_34,
        _ => 1.0,
    }
}

fn targets(world: &mut World) -> Vec<Target> {
    let mut out = Vec::new();
    let ignore_players = world
        .resource::<RoundScript>()
        .engine
        .players_ignore_radius_damage;
    if !ignore_players {
        let clients: Vec<(u32, u64)> = world
            .resource::<RoundScript>()
            .players
            .iter()
            .map(|(client, slot)| (*client, slot.object))
            .collect();
        for (client, object) in clients {
            let frame = FrameWorld::from_world(world);
            let Some(ps) = frame.player(crate::ClientId(client)) else {
                continue;
            };
            if ps.health <= 0 {
                continue;
            }
            let origin = ps.origin;
            out.push(Target {
                value: Value::Object(object),
                origin,
                mins: add(origin, PLAYER_MINS),
                maxs: add(origin, PLAYER_MAXS),
                eye: Some(add(origin, [0.0, 0.0, ps.view_height_current])),
                ignore: TraceIgnore {
                    client: Some(crate::ClientId(client)),
                    ..Default::default()
                },
            });
        }
    }
    let actors: Vec<(u64, Option<crate::ScriptModelId>)> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, entity)| (*id, entity.presence))
        .collect();
    for (id, presence) in actors {
        let origin = super::movement::origin_of(world, id);
        let health = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "health")
        {
            Value::Int(n) => n,
            Value::Float(f) => f as i32,
            _ => 0,
        };
        if health <= 0 {
            continue;
        }
        let (mins, maxs) = super::movement::hull(world, id);
        out.push(Target {
            value: Value::Object(id),
            origin,
            mins: add(origin, [mins[0], mins[1], 0.0]),
            maxs: add(origin, maxs),
            eye: None,
            ignore: TraceIgnore {
                model: presence,
                ..Default::default()
            },
        });
    }
    out
}

/// The squared distance from a point to a box.
fn distance_sq(point: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> f32 {
    (0..3)
        .map(|i| {
            let d = (mins[i] - point[i]).max(0.0).max(point[i] - maxs[i]);
            d * d
        })
        .sum()
}

/// `radiusdamage(origin, range, max, min, [attacker], [mod], [weapon])`;
/// the method form's entity is the inflictor and is spared.
fn radius_damage(
    world: &mut World,
    inflictor: Option<Value>,
    args: &[Value],
) -> Result<Value, String> {
    let blast = vector(args, 0)?;
    let range = float(args, 1)?.max(1.0);
    let max = float(args, 2)?;
    let min = float(args, 3)?;
    let attacker = super::super::super::players::damage_entity(world, args.get(4));
    let means = match args.get(5) {
        Some(Value::Undefined) | None => MOD_EXPLOSIVE,
        Some(_) => {
            let name = string(args, 5)?;
            MEANS_OF_DEATH
                .iter()
                .position(|m| *m == name)
                .ok_or_else(|| format!("Unknown means of death \"{name}\"\n"))?
        }
    };
    let weapon = match args.get(6) {
        Some(Value::Undefined) | None => None,
        Some(_) => Some(string(args, 6)?),
    };
    for target in targets(world) {
        if inflictor.as_ref() == Some(&target.value) {
            continue;
        }
        let dist_sq = distance_sq(blast, target.mins, target.maxs);
        if dist_sq >= range * range {
            continue;
        }
        let amount = (1.0 - dist_sq.sqrt() / range) * (max - min) + min;
        let seen = visibility(world, &target, blast, None);
        if seen <= 0.0 {
            continue;
        }
        let dir = [
            target.origin[0] - blast[0],
            target.origin[1] - blast[1],
            target.origin[2] - blast[2] + AIM_HEIGHT,
        ];
        damage::damage(
            world,
            &target.value,
            Hit {
                inflictor: inflictor.clone().unwrap_or(Value::Undefined),
                attacker: attacker.clone(),
                dir: Some(dir),
                point: Some(blast),
                damage: (seen * amount) as i32,
                dflags: DFLAGS_RADIUS,
                means,
                weapon: weapon.clone(),
                hit_location: 0,
                located: false,
                time_offset: 0,
            },
        )?;
    }
    // Other entities that take damage are not damaged yet: say so.
    let others = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(id, entity)| {
            entity.kind != EntityKind::Actor
                && inflictor.as_ref() != Some(&Value::Object(**id))
                && entity.presence.is_some()
        })
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let mut struck = 0;
    for id in others {
        let takes = !matches!(
            world
                .resource_mut::<RoundScript>()
                .object_field(id, "takedamage"),
            Value::Int(0) | Value::Undefined
        );
        let origin = super::movement::origin_of(world, id);
        if takes && distance_sq(blast, origin, origin) < range * range {
            struck += 1;
        }
    }
    if struck > 0 {
        return Err(format!(
            "t6.damage.entities: the blast reached {struck} entities that are neither players nor actors"
        ));
    }
    Ok(Value::Undefined)
}

/// `damageconetrace(origin, [ignore], [cone_dir], [cone_angle])`: how much
/// of the receiver `CanDamage` sees from `origin` (`gscm_damageconetrace`).
fn damage_cone_trace(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    let origin = vector(args, 0)?;
    let extra = super::super::engine::trace_ignore(world, args.get(1));
    let cone = if args.len() >= 3 {
        let dir = normalize(vector(args, 2)?);
        let angle = if args.len() >= 4 {
            float(args, 3)?
        } else {
            65.0
        };
        Some((angle.to_radians().cos(), dir))
    } else {
        None
    };
    let target = if let Some(client) = world.resource::<RoundScript>().player_client(id) {
        let frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player(crate::ClientId(client)) else {
            return Ok(Value::Float(0.0));
        };
        let at = ps.origin;
        Target {
            value: receiver.clone(),
            origin: at,
            mins: add(at, PLAYER_MINS),
            maxs: add(at, PLAYER_MAXS),
            eye: Some(add(at, [0.0, 0.0, ps.view_height_current])),
            ignore: TraceIgnore {
                client: Some(crate::ClientId(client)),
                other_client: extra.client,
                model: extra.model,
            },
        }
    } else {
        let at = super::movement::origin_of(world, id);
        let presence = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .and_then(|e| e.presence);
        let (mins, maxs) = if world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .is_some_and(|e| e.kind == EntityKind::Actor)
        {
            let (mins, maxs) = super::movement::hull(world, id);
            ([mins[0], mins[1], 0.0], maxs)
        } else {
            super::super::super::triggers::entity_bounds(world, id)
        };
        Target {
            value: receiver.clone(),
            origin: at,
            mins: add(at, mins),
            maxs: add(at, maxs),
            eye: None,
            ignore: TraceIgnore {
                client: extra.client,
                other_client: None,
                model: presence.or(extra.model),
            },
        }
    };
    Ok(Value::Float(visibility(world, &target, origin, cone)))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "radiusdamage", |world, _, args| {
        radius_damage(world, None, args)
    });
    registry.register(Method, "radiusdamage", |world, receiver, args| {
        radius_damage(world, Some(receiver.clone()), args)
    });
    registry.register(Method, "damageconetrace", damage_cone_trace);
}
