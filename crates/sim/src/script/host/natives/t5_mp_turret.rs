//! Black Ops multiplayer's killstreak turrets as BlackOpsMP.exe's handlers
//! keep them: `spawnturret` spawns a turret with no keys, `setturrettype`
//! makes it a Sentry Gun or a SAM Turret, its team may be `free`, and a
//! player carries it (`carryturret`, linked in front of them) until they put
//! it down where `canplayerplaceturret` finds room. The dpad's action slot
//! buttons read as pressed for the tick a press arrives and the next.

use super::super::args::{arg, float, int, optional, string, vector};
use super::engine::entity_id;
use super::t5_turret::{NOT_A_TURRET, edit_mp, is_turret};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::host::entities::EntityKind;
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// Turret flags `setturrettype` sets.
const FLAG_SENTRY: u32 = 0x1_0000;
const FLAG_TOW: u32 = 0x2_0000;
/// The entity flags the clients see: the turret's type (1 sentry, 2 SAM),
/// carried, just put down.
const EF_SENTRY: u32 = 0x1;
const EF_TOW: u32 = 0x2;
const EF_CARRIED: u32 = 0x8;
const EF_PLACED: u32 = 0x20;

/// The player's weapon flags: carrying a turret, and it cannot be put
/// down here.
const CARRYING_TURRET: u32 = 0x40_0000;
const TURRET_CANNOT_PLACE: u32 = 0x80_0000;
/// `pm_flags` while mantling.
const PMF_MANTLE: u32 = 0x4;

/// The placement traces' content mask.
const PLACEMENT_MASK: u32 = 0x0281_8011;
/// The placement eye: 60 units over the player's origin.
const PLACEMENT_EYE_HEIGHT: f32 = 60.0;

/// `iscarryingturret`'s flag on each client; nothing ported reads it.
#[derive(Resource, Default)]
pub(crate) struct CarryingTurret(BTreeMap<u32, bool>);

fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    let id = entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })
}

fn weap_flag(world: &mut World, client: u32, set: u32, clear: u32) {
    if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
        ps.weap_flags = (ps.weap_flags & !clear) | set;
    }
}

fn bool_arg(args: &[Value], index: usize) -> Result<bool, String> {
    Ok(int(args, index)? != 0)
}

/// `spawnturret(classname, origin, weaponinfo)`.
fn spawn_turret(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let classname = string(args, 0)?;
    let origin = vector(args, 1)?;
    let weapon = string(args, 2)?;
    let id = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        id
    };
    super::t5_turret::spawn_scripted(world, id, &weapon)?;
    Ok(Value::Object(id))
}

/// `turret setTurretType("sentry" | "tow")`.
fn set_turret_type(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let kind = string(args, 0)?;
    let (flag, entity_flag) = match kind.as_str() {
        "sentry" => (FLAG_SENTRY, EF_SENTRY),
        "tow" => (FLAG_TOW, EF_TOW),
        _ => (0, 0),
    };
    edit_mp(world, receiver, NOT_A_TURRET, |turret, _| {
        turret.spawn.flags = (turret.spawn.flags & !(FLAG_SENTRY | FLAG_TOW)) | flag;
        turret.state.entity_flags =
            (turret.state.entity_flags & !(EF_SENTRY | EF_TOW)) | entity_flag;
    })?;
    if flag == 0 {
        // Printed, not raised.
        diag::warn!(Sim, "{kind}, Unknown turret type.");
    }
    Ok(Value::Undefined)
}

/// `turret setTurretTeam("axis" | "allies" | "free")`.
fn set_turret_team(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let team = string(args, 0)?;
    let team = match team.as_str() {
        "axis" => Some(1),
        "allies" => Some(2),
        "free" => Some(0),
        _ => None,
    };
    let mut known = true;
    edit_mp(world, receiver, NOT_A_TURRET, |turret, _| match team {
        Some(team) => turret.spawn.team = team,
        None => known = false,
    })?;
    if !known {
        return Err(format!(
            "unknown team '{}', should be 'axis' or 'allies' or 'free'\n",
            string(args, 0)?
        ));
    }
    Ok(Value::Undefined)
}

/// `turret setTurretOwner(player)`: not for players or actors; any other
/// entity takes the owner, a turret as the entity its searches pass by.
fn set_turret_owner(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    if world.resource::<RoundScript>().entities[&id].kind == EntityKind::Player {
        return Err("SetOwner can not be used for players or actors".into());
    }
    let owner = entity_id(world, arg(args, 0)?)?;
    if is_turret(world, id) {
        edit_mp(world, receiver, NOT_A_TURRET, |_, kept| *kept = Some(owner))?;
    }
    Ok(Value::Undefined)
}

/// `turret setTurretCarried(carried)`: the clients see it carried, or just
/// put down.
fn set_turret_carried(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let carried = bool_arg(args, 0)?;
    edit_mp(world, receiver, NOT_A_TURRET, |turret, _| {
        let flags = &mut turret.state.entity_flags;
        *flags = if carried {
            *flags | EF_CARRIED
        } else {
            (*flags & !EF_CARRIED) | EF_PLACED
        };
    })
}

/// `turret setScanningPitch(pitch)`, with the drop pitch's argument error.
fn set_scanning_pitch(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("illegal call to setdefaultdroppitch()\n".into());
    }
    let pitch = float(args, 0)?;
    edit_mp(world, receiver, "entity is not a turret", |turret, _| {
        turret.state.scanning_pitch = pitch;
    })
}

/// `player carryTurret(turret, offset, angles)`: the turret is linked to
/// the player at the offset and angles, and the player carries it.
fn carry_turret(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let player_id = entity_id(world, receiver)?;
    let turret = entity_id(world, arg(args, 0)?)?;
    let offset = vector(args, 1)?;
    let angles = vector(args, 2)?;
    weap_flag(world, client, CARRYING_TURRET, 0);
    if turret == player_id {
        return Ok(Value::Undefined);
    }
    if let Some(entity) = world
        .resource_mut::<RoundScript>()
        .entities
        .get_mut(&turret)
    {
        entity.linked_to = Some(crate::script::host::entities::Link {
            parent: player_id,
            origin: offset,
            angles,
            tag: None,
            tag_offset: Some([0.0; 3]),
        });
    }
    Ok(Value::Undefined)
}

/// `player stopCarryTurret(turret, [origin], [angles])`: the turret is
/// unlinked where it is, or set down at the origin and angles given.
fn stop_carry_turret(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let turret = entity_id(world, arg(args, 0)?)?;
    let origin = optional(args, 1, vector)?;
    let angles = optional(args, 2, vector)?;
    let (at, facing) = super::t5_turret::turret_pose(world, turret);
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Some(entity) = runtime.entities.get_mut(&turret) {
        entity.linked_to = None;
    }
    runtime.set_object_field(turret, "origin", Value::Vector(origin.unwrap_or(at)));
    runtime.set_object_field(turret, "angles", Value::Vector(angles.unwrap_or(facing)));
    weap_flag(world, client, 0, CARRYING_TURRET);
    Ok(Value::Undefined)
}

/// `player isCarryingTurret(carrying)`: sets the client's flag.
fn is_carrying_turret(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let carrying = bool_arg(args, 0)?;
    world
        .get_resource_or_insert_with(CarryingTurret::default)
        .0
        .insert(client, carrying);
    Ok(Value::Undefined)
}

/// `player setTurretHint(placeable)`: the client shows whether the carried
/// turret can be put down.
fn set_turret_hint(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver)?;
    if bool_arg(args, 0)? {
        weap_flag(world, client, 0, TURRET_CANNOT_PLACE);
    } else {
        weap_flag(world, client, TURRET_CANNOT_PLACE, 0);
    }
    Ok(Value::Undefined)
}

/// `player canPlayerPlaceTurret()`: `["result"]`, `["origin"]` and
/// `["angles"]` of the spot in front of the player; the client's hint
/// follows the result.
fn can_player_place_turret(
    world: &mut World,
    receiver: &Value,
    _: &[Value],
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let Some(ps) = FrameWorld::from_world(world)
        .player(ClientId(client))
        .cloned()
    else {
        return Err(format!("entity {client} is not a player"));
    };
    let mut eye = ps.origin;
    eye[2] += PLACEMENT_EYE_HEIGHT;
    let placement = place(world, ps.origin, eye, ps.viewangles[1]);
    let result = placement.ok && ps.pm_flags & PMF_MANTLE == 0;
    if result {
        weap_flag(world, client, 0, TURRET_CANNOT_PLACE);
    } else {
        weap_flag(world, client, TURRET_CANNOT_PLACE, 0);
    }
    super::engine::keyed_array(
        world,
        vec![
            ("result", Value::Int(result.into())),
            ("origin", Value::Vector(placement.origin)),
            ("angles", Value::Vector(placement.angles)),
        ],
    )
}

struct Placement {
    ok: bool,
    origin: [f32; 3],
    angles: [f32; 3],
}

fn dvar(world: &World, name: &str, default: f32) -> f32 {
    crate::script::host::actor_nav::dvar_f32(world, name, default)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The game's turret placement: a box of `turret_placement_trace_mins` /
/// `_maxs` dropped from `turret_placement_trace_dist` (plus the cannon's
/// safety radius) ahead of the eye, pitched down by
/// `turret_placement_trace_pitch`, to the player's feet; it must land on
/// ground no steeper than `turret_placement_trace_min_normal`, and its four
/// feet must find ground below them.
fn place(world: &mut World, origin: [f32; 3], eye: [f32; 3], yaw: f32) -> Placement {
    let mut placement = Placement {
        ok: false,
        origin: [0.0; 3],
        angles: [0.0, yaw, 0.0],
    };
    let distance = dvar(world, "turret_placement_trace_radius_canon_safety", 5.0)
        + dvar(world, "turret_placement_trace_dist", 42.0);
    let pitch = dvar(world, "turret_placement_trace_pitch", 20.0);
    let (forward, _, _) = math_iw4::angle_vectors([pitch, yaw, 0.0]);
    let start: [f32; 3] = std::array::from_fn(|i| forward[i] * distance + eye[i]);
    let end = [start[0], start[1], start[2] - (eye[2] - origin[2])];
    let low = dvar(world, "turret_placement_trace_mins", -30.0);
    let high = dvar(world, "turret_placement_trace_maxs", 30.0);
    let trace = super::engine::trace(world, start, end, [low; 3], [high; 3], PLACEMENT_MASK);
    let fraction = trace.fraction;
    placement.origin = std::array::from_fn(|i| (end[i] - start[i]) * fraction + start[i]);
    let min_normal = dvar(world, "turret_placement_trace_min_normal", 0.7);
    if trace.startsolid != 0
        || trace.allsolid != 0
        || fraction < 0.0001
        || trace.normal[2] < min_normal
    {
        placement.origin[2] = origin[2];
        return placement;
    }
    placement.origin[2] -= high;
    let normal = trace.normal;
    let left = cross(normal, forward);
    let mut axis = [cross(left, normal), left, normal];
    placement.angles = math_iw4::axis_to_angles(axis);
    if feet(world, &mut axis, &mut placement.origin) {
        placement.angles = math_iw4::axis_to_angles(axis);
        placement.ok = true;
    }
    placement
}

/// The four feet: from `turret_placement_feet_offset` about the spot, each
/// traced down `turret_placement_feet_trace_dist_z`; the spot sinks by the
/// deeper diagonal's mean (a foot that finds nothing takes its diagonal
/// partner's depth) and the turret tilts to the feet. Fails when two or
/// more find nothing.
fn feet(world: &mut World, axis: &mut [[f32; 3]; 3], origin: &mut [f32; 3]) -> bool {
    let offset = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("turret_placement_feet_offset")
        .and_then(|text| {
            let v: Vec<f32> = text
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        })
        .unwrap_or([17.0, 20.0, 10.0]);
    let depth = dvar(world, "turret_placement_feet_trace_dist_z", 20.0);
    let [forward, left, up] = *axis;
    let mut foot = [[0.0f32; 3]; 4];
    foot[0] = std::array::from_fn(|i| {
        up[i] * offset[2] + left[i] * offset[1] + forward[i] * offset[0] + origin[i]
    });
    foot[1] = std::array::from_fn(|i| forward[i] * (offset[0] * -2.0) + foot[0][i]);
    foot[2] = std::array::from_fn(|i| left[i] * (offset[1] * -2.0) + foot[1][i]);
    foot[3] = std::array::from_fn(|i| forward[i] * (offset[0] * 2.0) + foot[2][i]);
    let mut misses = 0;
    let mut hit = [false; 4];
    let mut height = [0.0f32; 4];
    for i in 0..4 {
        let end = std::array::from_fn(|j| up[j] * -depth + foot[i][j]);
        let trace = super::engine::trace(world, foot[i], end, [0.0; 3], [0.0; 3], PLACEMENT_MASK);
        if trace.fraction >= 1.0 {
            misses += 1;
        } else {
            hit[i] = true;
            height[i] = depth * trace.fraction - offset[2];
        }
        let sink = -height[i];
        for j in 0..3 {
            foot[i][j] += sink * up[j];
        }
    }
    if !hit[0] {
        height[0] = height[2];
    }
    if !hit[1] {
        height[1] = height[3];
    }
    if !hit[2] {
        height[2] = height[0];
    }
    if !hit[3] {
        height[3] = height[1];
    }
    let mut sink = (height[0] + height[2]) * 0.5;
    let other = (height[1] + height[3]) * 0.5;
    if sink - other < 0.0 {
        sink = other;
    }
    for j in 0..3 {
        origin[j] += -sink * up[j];
    }
    let a: [f32; 3] = std::array::from_fn(|j| foot[1][j] - foot[3][j]);
    let b: [f32; 3] = std::array::from_fn(|j| foot[0][j] - foot[2][j]);
    let n = [
        a[2] * b[1] - a[1] * b[2],
        a[0] * b[2] - a[2] * b[0],
        a[1] * b[0] - a[0] * b[1],
    ];
    let mut length = (n[2] * n[2] + n[1] * n[1] + n[0] * n[0]).sqrt();
    if 0.0 <= -length {
        length = 1.0;
    }
    let inverse = 1.0 / length;
    let up = n.map(|x| x * inverse);
    let left = cross(up, forward);
    let forward = cross(left, up);
    *axis = [forward, left, up];
    misses < 2
}

/// `actionSlot<N>ButtonPressed()`.
fn action_slot_pressed(world: &mut World, receiver: &Value, slot: usize) -> Result<Value, String> {
    let client = player(world, receiver)?;
    Ok(Value::Int(
        crate::script::host::controls::action_slot_pressed(world, client, slot).into(),
    ))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "spawnturret", spawn_turret);
    registry.register(Method, "setturrettype", set_turret_type);
    registry.register(Method, "setturretteam", set_turret_team);
    registry.register(Method, "setturretowner", set_turret_owner);
    registry.register(Method, "setturretcarried", set_turret_carried);
    registry.register(Method, "setscanningpitch", set_scanning_pitch);
    registry.register(Method, "carryturret", carry_turret);
    registry.register(Method, "stopcarryturret", stop_carry_turret);
    registry.register(Method, "iscarryingturret", is_carrying_turret);
    registry.register(Method, "setturrethint", set_turret_hint);
    registry.register(Method, "canplayerplaceturret", can_player_place_turret);
    registry.register(Method, "actionslotonebuttonpressed", |w, r, _| {
        action_slot_pressed(w, r, 0)
    });
    registry.register(Method, "actionslottwobuttonpressed", |w, r, _| {
        action_slot_pressed(w, r, 1)
    });
    registry.register(Method, "actionslotthreebuttonpressed", |w, r, _| {
        action_slot_pressed(w, r, 2)
    });
    registry.register(Method, "actionslotfourbuttonpressed", |w, r, _| {
        action_slot_pressed(w, r, 3)
    });
}
