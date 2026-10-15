//! Black Ops 2's actor combat: whether an actor sees an entity (`cansee`),
//! which entities are sentient, and the melee swing (`melee`).

use super::super::super::entities::EntityKind;
use super::super::engine::{TraceIgnore, collider_entity, entity_id, entity_trace};
use super::damage;
use super::movement::{actor, field, origin_of};
use super::vector;
use crate::bullet_collision::{ColliderId, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// Where an actor looks from when its model's `TAG_EYE` is not posed: the
/// engine's own fallback, this far above its origin (`t6.actors.sight`).
const EYE_HEIGHT: f32 = 64.0;
/// An actor's sight until scripts change it, as the engine spawns it.
const FOV_COSINE: f32 = 0.01;
const MAX_SIGHT_DIST_SQ: f32 = 8192.0 * 8192.0;
/// The engine's sight trace, near and far of its fog distance.
const SIGHT_MASK: u32 = 0x0280_9801;
/// A zombie's swing: reach, half width and half height of its nine rays.
const MELEE_RANGE: f32 = 64.0;
const MELEE_WIDTH: f32 = 5.0;
const MELEE_HEIGHT: f32 = 30.0;
const MELEE_MASK: u32 = 0x0280_e893;
/// A swing's damage adds `rand() % 5` to its weapon's.
const MELEE_ROLL: u32 = 5;
const DEFAULT_WEAPON: &str = "defaultweapon_mp";
/// The hit locations of IW4L's player collision, by its numbers, which are
/// Modern Warfare 2's; each is taken as Black Ops 2's of the same name, its
/// shield as the riot shield (`t6.player.hit_locations`).
pub(super) const HIT_LOCATIONS: [&str; 20] = [
    "none",
    "helmet",
    "head",
    "neck",
    "torso_upper",
    "torso_lower",
    "right_arm_upper",
    "left_arm_upper",
    "right_arm_lower",
    "left_arm_lower",
    "right_hand",
    "left_hand",
    "right_leg_upper",
    "left_leg_upper",
    "right_leg_lower",
    "left_leg_lower",
    "right_foot",
    "left_foot",
    "gun",
    "riotshield",
];
/// The rays' offsets across the swing, in widths and heights.
const MELEE_RAYS: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [0.5, 0.0],
    [-0.5, 0.0],
    [0.0, 0.5],
    [0.0, -0.5],
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
];

/// The frame each actor last swung in: once a frame, as the engine allows.
#[derive(Resource, Default)]
struct Swings(BTreeMap<u64, i64>);

fn number(world: &mut World, id: u64, name: &str, default: f32) -> f32 {
    match field(world, id, name) {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => default,
    }
}

fn sentient(world: &World, value: &Value) -> bool {
    let runtime = world.resource::<RoundScript>();
    runtime.player_client_of(value).is_some()
        || runtime
            .entity(value)
            .is_some_and(|(_, entity)| entity.kind == EntityKind::Actor)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length > 0.0 {
        v.map(|c| c / length)
    } else {
        v
    }
}

/// Forward, right and up of the actor's angles.
fn axis(world: &mut World, id: u64) -> [[f32; 3]; 3] {
    let angles = match field(world, id, "angles") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

/// How far a trace got, and what it struck.
fn traced(outcome: TraceOutcome) -> (f32, Option<ColliderId>) {
    match outcome {
        TraceOutcome::Hit {
            fraction, collider, ..
        } => (fraction, Some(collider)),
        TraceOutcome::StartSolid { collider, .. } => (0.0, collider),
        TraceOutcome::Miss { .. } | TraceOutcome::Invalid { .. } => (1.0, None),
    }
}

/// An actor's eye (`t6.actors.sight`), a player's view, else the origin.
pub(super) fn eye(world: &mut World, value: &Value) -> Result<[f32; 3], String> {
    let id = entity_id(world, value)?;
    if let Some(client) = world.resource::<RoundScript>().player_client(id) {
        let frame = FrameWorld::from_world(world);
        if let Some(ps) = frame.player(crate::ClientId(client)) {
            return Ok([
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] + ps.view_height_current,
            ]);
        }
    }
    let origin = origin_of(world, id);
    let actor = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| entity.kind == EntityKind::Actor);
    Ok(if actor {
        [origin[0], origin[1], origin[2] + EYE_HEIGHT]
    } else {
        origin
    })
}

/// `cansee`: within the actor's sight distance and field of view, with
/// nothing in the sight line (`t6.actors.sight` for what is not read).
fn can_see(world: &mut World, id: u64, target: &Value) -> Result<bool, String> {
    let from = eye(world, &Value::Object(id))?;
    let to = eye(world, target)?;
    let delta = sub(to, from);
    let distance_sq = dot(delta, delta);
    if distance_sq > number(world, id, "maxsightdistsqrd", MAX_SIGHT_DIST_SQ) {
        return Ok(false);
    }
    let fov = number(world, id, "fovcosine", FOV_COSINE);
    let facing = dot(axis(world, id)[0], delta);
    if fov != 0.0 && (facing < 0.0 || facing * facing < distance_sq * fov * fov) {
        return Ok(false);
    }
    let ignore = TraceIgnore {
        client: world
            .resource::<RoundScript>()
            .player_client_of(target)
            .map(crate::ClientId),
        ..Default::default()
    };
    Ok(traced(entity_trace(world, from, to, SIGHT_MASK, ignore)).0 >= 1.0)
}

/// `melee`: the actor swings at most once a frame, from its eye toward its
/// enemy (or along the direction given, or ahead), over nine rays; the
/// first player hit, else the nearest hit, is struck.
fn melee(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let now = super::super::super::players::now_ms(world);
    let swings = &mut world.get_resource_or_insert_with(Swings::default).0;
    if swings.insert(id, now) == Some(now) {
        return Ok(Value::Undefined);
    }
    let start = eye(world, receiver)?;
    let [forward, right, up] = axis(world, id);
    let enemy = match field(world, id, "enemy") {
        Value::Object(enemy) => Some(Value::Object(enemy)),
        _ => None,
    };
    let given = if args.is_empty() {
        None
    } else {
        Some(vector(args, 0)?)
    };
    let direction = match (&enemy, given) {
        (Some(enemy), given) => {
            let mut delta = sub(eye(world, enemy)?, start);
            if let Some(given) = given {
                let across = (delta[0].hypot(delta[1])) / given[0].hypot(given[1]);
                delta = [given[0] * across, given[1] * across, delta[2]];
            }
            normalize(delta)
        }
        (None, Some(given)) => normalize(given),
        (None, None) => forward,
    };
    let mut struck: Option<Strike> = None;
    for [across, high] in MELEE_RAYS {
        let end: [f32; 3] = std::array::from_fn(|i| {
            start[i]
                + direction[i] * MELEE_RANGE
                + right[i] * MELEE_WIDTH * across
                + up[i] * MELEE_HEIGHT * high
        });
        let (fraction, collider) = traced(entity_trace(
            world,
            start,
            end,
            MELEE_MASK,
            TraceIgnore::default(),
        ));
        let Some(collider) = collider.filter(|_| fraction < 1.0) else {
            continue;
        };
        let strike = Strike {
            fraction,
            entity: collider_entity(world, collider),
            point: std::array::from_fn(|i| start[i] + (end[i] - start[i]) * fraction),
            hitloc: match collider {
                ColliderId::Player { hitloc, .. } => hitloc,
                _ => 0,
            },
        };
        if world
            .resource::<RoundScript>()
            .player_client_of(&strike.entity)
            .is_some()
        {
            struck = Some(strike);
            break;
        }
        if struck
            .as_ref()
            .is_none_or(|nearest| fraction <= nearest.fraction)
        {
            struck = Some(strike);
        }
    }
    let Some(strike) = struck.filter(|strike| !matches!(strike.entity, Value::Undefined)) else {
        return Ok(Value::Undefined);
    };
    // The engine's melee hit: a swing that lands on an actor of the
    // swinging actor's own team does nothing.
    if damage::is_actor(world, &strike.entity)
        && let Value::Object(other) = strike.entity
        && team(world, other) == team(world, id)
    {
        return Ok(Value::Undefined);
    }
    // An entity that takes no damage (an actor corpse) stops the swing and
    // is not hit.
    if let Value::Object(other) = strike.entity
        && world
            .resource::<RoundScript>()
            .entities
            .get(&other)
            .is_some_and(|e| e.kind != EntityKind::Player && !e.can_damage)
    {
        return Ok(Value::Undefined);
    }
    let damage = melee_damage(world, id)?;
    if world
        .resource::<RoundScript>()
        .player_client_of(&strike.entity)
        .is_none()
    {
        return Err(format!(
            "t6.actors.melee_entities: a swing struck an entity that is not a player nor an actor of its team, for {damage}"
        ));
    }
    let hit_location = HIT_LOCATIONS
        .get(usize::from(strike.hitloc))
        .and_then(|name| damage::HIT_LOCATIONS.iter().position(|ours| ours == name))
        .unwrap_or(0);
    damage::damage(
        world,
        &strike.entity,
        damage::Hit {
            inflictor: receiver.clone(),
            attacker: receiver.clone(),
            dir: Some(direction),
            point: Some(strike.point),
            damage,
            dflags: 0,
            means: damage::MOD_MELEE,
            weapon: Some("none".to_owned()),
            hit_location,
        },
    )?;
    Ok(strike.entity)
}

/// An entity's `team` field.
fn team(world: &mut World, id: u64) -> Option<String> {
    match field(world, id, "team") {
        Value::String(team) => Some(team.to_string()),
        _ => None,
    }
}

/// What a swing struck: how far along its ray, where, and the hit location
/// a player's collision reports.
struct Strike {
    fraction: f32,
    entity: Value,
    point: [f32; 3],
    hitloc: u8,
}

/// The actor's weapon's melee damage plus the engine's `rand() % 5`. An
/// actor without a weapon swings the zone's `none` weapon, which the engine
/// keeps at index 0 (the default weapon where a zone has no `none`).
fn melee_damage(world: &mut World, id: u64) -> Result<i32, String> {
    let name = match field(world, id, "weapon") {
        Value::String(name) if !name.is_empty() => name.to_string(),
        _ => "none".to_owned(),
    };
    let weapon = super::weapon::zone_weapon(world, &name)
        .or_else(|| super::weapon::zone_weapon(world, DEFAULT_WEAPON))
        .ok_or_else(|| format!("no weapon '{name}' in the zones, nor the default weapon"))?;
    let roll = (super::next_rand(world) >> 17) % MELEE_ROLL;
    Ok(weapon.melee_damage + roll as i32)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "issentient", |world, _, args| {
        Ok(Value::Int(
            args.first()
                .is_some_and(|value| sentient(world, value))
                .into(),
        ))
    });
    registry.register(Method, "cansee", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let target = args.first().ok_or("cansee expects an entity")?;
        entity_id(world, target)?;
        if args.len() > 1 {
            super::int(args, 1)?;
        }
        Ok(Value::Int(can_see(world, id, target)?.into()))
    });
    registry.register(Method, "melee", melee);
    registry.register(Method, "getweaponmuzzlepoint", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            diag::warn!(
                Sim,
                "t6.weapons.muzzle_point: a non-player's muzzle point is left unset by the engine; answering its origin"
            );
            return Ok(Value::Vector(origin_of(world, id)));
        }
        Ok(Value::Vector(eye(world, receiver)?))
    });
    registry.register(Method, "geteye", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if !sentient(world, receiver) {
            let classname = world
                .resource::<RoundScript>()
                .entities
                .get(&id)
                .map_or_else(String::new, |entity| entity.classname.to_string());
            return Err(format!(
                "getEye must be called on an AI or player, not on a '{classname}'"
            ));
        }
        Ok(Value::Vector(eye(world, receiver)?))
    });
}
