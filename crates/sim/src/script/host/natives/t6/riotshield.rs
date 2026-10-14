//! Black Ops 2's riot shield builtins: the shield model a player carries
//! (`attachshieldmodel`), his weapon's view model variant
//! (`setheldweaponmodel`), where a shield he plants would stand
//! (`canplaceriotshield`), its placement hints, and an entity's centroid.

use super::{int, string};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use crate::script_player;
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What the engine keeps on a player's client for his shield
/// (`t6.riotshield.client`: kept, not presented).
#[derive(Default)]
struct Shield {
    /// The tag `attachshieldmodel` attached it to; set with the client's
    /// shield flag, cleared by `detachshieldmodel`.
    tag: Option<Arc<str>>,
    /// `setplacementhint(0)`: the client's can't-place flag.
    cannot_place: bool,
    /// `setriotshieldfailhint`'s events on the player's entity.
    fail_hints: u32,
}

#[derive(Resource, Default)]
struct Shields(BTreeMap<u32, Shield>);

fn shield(world: &mut World, client: u32) -> &mut Shield {
    world
        .get_resource_or_insert_with(Shields::default)
        .into_inner()
        .0
        .entry(client)
        .or_default()
}

/// The player a method runs on, with the engine's error for anything else.
fn player(world: &World, receiver: &Value) -> Result<(u64, u32), String> {
    let runtime = world.resource::<RoundScript>();
    let (id, entity) = runtime.entity(receiver).ok_or("not an entity")?;
    runtime
        .player_client(id)
        .map(|client| (id, client))
        .ok_or_else(|| format!("entity {} is not a player", entity.number))
}

fn shield_player(world: &World, receiver: &Value) -> Result<(u64, u32), String> {
    player(world, receiver).map_err(|error| match error.as_str() {
        "not an entity" => error,
        _ => "This function only supports player entities.".into(),
    })
}

/// The view models a weapon has: `gunXModel`, 16 variants.
const VIEW_MODEL_VARIANTS: i32 = 16;

/// `canplaceriotshield`'s spot: 20 ahead of the standing eye, pitched 20
/// down, traced down by the standing view height; then a 100-wide box
/// down the same line must land on the world, with ground at least this
/// level.
const PLACE_AHEAD: f32 = 20.0;
const PLACE_PITCH: f32 = 20.0;
const PLACE_BOX: f32 = 50.0;
const PLACE_MIN_NORMAL_Z: f32 = 0.7;
const PLACE_MIN_FRACTION: f32 = 0.0001;
const PLACE_MASK: u32 = 0x0281_8091;
const STANDING_VIEW_HEIGHT: &str = "player_standingviewheight";
const STANDING_VIEW_HEIGHT_DEFAULT: i32 = 60;
const PITCH_MAX: (&str, f32) = ("riotshield_deploy_pitch_max", 20.0);
const ROLL_MAX: (&str, f32) = ("riotshield_deploy_roll_max", 25.0);
const ZDIFF_MAX: (&str, f32) = ("riotshield_deploy_zdiff_max", 30.0);

pub(super) fn dvar_f32(world: &World, (name, default): (&str, f32)) -> f32 {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .map_or(default, |value| super::atof(value))
}

pub(super) fn dvar_i32(world: &World, name: &str, default: i32) -> i32 {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .map_or(default, |value| super::atoi(value))
}

pub(super) fn forward(pitch: f32, yaw: f32) -> [f32; 3] {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    [cp * cy, cp * sy, -sp]
}

pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| (b[i] - a[i]) * t + a[i])
}

fn angle_normalize_180(angle: f32) -> f32 {
    let wrapped = angle - 360.0 * (angle / 360.0).floor();
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

/// The engine's `vectoangles` pitch and yaw (each 0..360).
pub(super) fn vector_angles(v: [f32; 3]) -> [f32; 2] {
    if v[0] == 0.0 && v[1] == 0.0 {
        return [if -v[2] < 0.0 { 270.0 } else { 90.0 }, 0.0];
    }
    let yaw = v[1].atan2(v[0]).to_degrees();
    let pitch = -v[2].atan2(v[0].hypot(v[1])).to_degrees();
    [
        pitch + if pitch < 0.0 { 360.0 } else { 0.0 },
        yaw + if yaw < 0.0 { 360.0 } else { 0.0 },
    ]
}

/// The engine's `AxisToAngles`: pitch and yaw from the forward axis, roll
/// from the left and up axes turned back by that yaw and pitch.
pub(super) fn axis_to_angles(axis: [[f32; 3]; 3]) -> [f32; 3] {
    let [pitch, yaw] = vector_angles(axis[0]);
    let (s, c) = (-yaw).to_radians().sin_cos();
    let left_y = axis[1][0] * s + axis[1][1] * c;
    let flat = axis[1][0] * c - axis[1][1] * s;
    let (s, c) = (-pitch).to_radians().sin_cos();
    let turned = [axis[1][2] * s + flat * c, left_y, axis[1][2] * c - flat * s];
    let roll = if turned[0] == 0.0 && turned[1] == 0.0 {
        if -turned[2] >= 0.0 { 90.0 } else { -90.0 }
    } else {
        -turned[2].atan2(turned[0].hypot(turned[1])).to_degrees()
    };
    let roll = if left_y >= 0.0 {
        -roll
    } else if roll < 0.0 {
        roll + 180.0
    } else {
        roll - 180.0
    };
    [pitch, yaw, roll]
}

/// The riot shield's placement test: where it would stand in front of the
/// player and how it would be turned, and whether it may go there.
fn placement(world: &mut World, client: u32) -> Option<(bool, [f32; 3], [f32; 3])> {
    let (origin, yaw) = FrameWorld::from_world(world)
        .player(crate::ClientId(client))
        .map(|ps| (ps.origin, ps.viewangles[1]))?;
    let standing = dvar_i32(world, STANDING_VIEW_HEIGHT, STANDING_VIEW_HEIGHT_DEFAULT) as f32;
    let ahead = forward(PLACE_PITCH, yaw);
    let start =
        [0, 1, 2].map(|i| [origin[0], origin[1], origin[2] + standing][i] + ahead[i] * PLACE_AHEAD);
    let end = [start[0], start[1], start[2] - standing];
    let mut angles = [0.0, yaw, 0.0];
    let line = super::super::engine::trace(world, start, end, [0.0; 3], [0.0; 3], PLACE_MASK);
    let mut spot = lerp(start, end, line.fraction);
    if line.fraction >= 1.0 {
        return Some((false, spot, angles));
    }
    let half = [PLACE_BOX; 3];
    let block = super::super::engine::trace(world, start, end, half.map(|h| -h), half, PLACE_MASK);
    spot = lerp(start, end, block.fraction);
    if block.allsolid != 0
        || block.startsolid != 0
        || block.fraction < PLACE_MIN_FRACTION
        || block.normal[2] < PLACE_MIN_NORMAL_Z
    {
        return Some((false, spot, angles));
    }
    spot[2] -= PLACE_BOX;
    if block.move_trace().entity != game_api::movement::ENTITY_WORLD {
        return Some((false, spot, angles));
    }
    let up = block.normal;
    let left = cross(up, ahead);
    angles = axis_to_angles([cross(left, up), left, up]);
    angles[0] = angle_normalize_180(angles[0]);
    angles[2] = angle_normalize_180(angles[2]);
    let fits = angles[0].abs() <= dvar_f32(world, PITCH_MAX)
        && angles[2].abs() <= dvar_f32(world, ROLL_MAX)
        && (origin[2] - spot[2]).abs() <= dvar_f32(world, ZDIFF_MAX);
    Some((fits, spot, angles))
}

/// An entity's box: an actor's hull, a script mover's or trigger's own.
fn local_bounds(world: &World, id: u64) -> Result<([f32; 3], [f32; 3]), String> {
    use super::super::super::entities::EntityKind;
    let runtime = world.resource::<RoundScript>();
    match runtime.entities.get(&id).map(|entity| &entity.kind) {
        Some(EntityKind::Player) => Err(
            "getcentroid: t6.entity.player_bounds: a player's box (his stance's mins and maxs) is not kept"
                .into(),
        ),
        Some(EntityKind::Actor) => {
            let (radius, floor, top) = world
                .get_resource::<super::actor::Actors>()
                .and_then(|actors| actors.0.get(&id))
                .map_or_else(|| super::actor::Actor::default().hull, |actor| actor.hull);
            Ok(([-radius, -radius, floor], [radius, radius, top]))
        }
        _ => Ok(super::bounds(world, id)),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    // gscm_attachshieldmodel: attach(model, tag) on a player, then the
    // client's shield flag and tag.
    registry.register(Method, "attachshieldmodel", |world, receiver, args| {
        let (id, client) = shield_player(world, receiver)?;
        let tag = super::attach::attach(world, id, args)?;
        shield(world, client).tag = Some(tag);
        Ok(Value::Undefined)
    });
    // gscm_detachshieldmodel: the flag and tag cleared, then detach.
    registry.register(Method, "detachshieldmodel", |world, receiver, args| {
        let (id, client) = shield_player(world, receiver)?;
        shield(world, client).tag = None;
        super::attach::detach(world, id, args)
    });
    // gscm_setheldweaponmodel: his current weapon's view model variant, or
    // 0 when the weapon has no model there.
    registry.register(Method, "setheldweaponmodel", |world, receiver, args| {
        let (_, client) = player(world, receiver)?;
        let id = crate::ClientId(client);
        let weapon = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weapon);
        let variant = int(args, 0)?;
        if !(0..VIEW_MODEL_VARIANTS).contains(&variant) {
            return Err(format!(
                "SetHeldWeaponModel called with an invalid model variant: {variant}, valid is 0-15"
            ));
        }
        let name = script_player::weapon_name(&FrameWorld::from_world(world), weapon);
        let has_model = super::weapon::zone_weapon(world, &name).is_some_and(|w| {
            w.view_models
                .get(variant as usize)
                .is_some_and(|model| !model.is_empty())
        });
        let model = if has_model { variant as u8 } else { 0 };
        script_player::set_weapon_model(&mut FrameWorld::from_world(world), id, weapon, model);
        Ok(Value::Undefined)
    });
    // gscm_canplaceriotshield(event): ["result"], and when it may go there
    // ["origin"] and ["angles"].
    registry.register(Method, "canplaceriotshield", |world, receiver, args| {
        let (_, client) = player(world, receiver)?;
        string(args, 0)?;
        let (fits, origin, angles) =
            placement(world, client).unwrap_or((false, [0.0; 3], [0.0; 3]));
        let mut entries = vec![("result", Value::Int(fits.into()))];
        if fits {
            entries.push(("origin", Value::Vector(origin)));
            entries.push(("angles", Value::Vector(angles)));
        }
        super::super::engine::keyed_array(world, entries)
    });
    registry.register(Method, "setplacementhint", |world, receiver, args| {
        let (_, client) = player(world, receiver)?;
        let can_place = int(args, 0)? != 0;
        shield(world, client).cannot_place = !can_place;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setriotshieldfailhint", |world, receiver, _| {
        let (_, client) = player(world, receiver)?;
        shield(world, client).fail_hints += 1;
        Ok(Value::Undefined)
    });
    // gscm_getcentroid: the middle of its box, turned by its angles, from
    // its origin.
    registry.register(Method, "getcentroid", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let (mins, maxs) = local_bounds(world, id)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let vector = |value: Value| match value {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let origin = vector(runtime.object_field(id, "origin"));
        let angles = vector(runtime.object_field(id, "angles"));
        let middle = [0, 1, 2].map(|i| (mins[i] + maxs[i]) * 0.5);
        let (fwd, right, up) = angle_axes(angles);
        let left = right.map(|r| -r);
        Ok(Value::Vector([0, 1, 2].map(|i| {
            fwd[i] * middle[0] + left[i] * middle[1] + up[i] * middle[2] + origin[i]
        })))
    });
    // gsc_deployriotshield(owner, shield_ent): spawns the deployed shield's
    // retrieve trigger and returns it.
    registry.register(Namespace::Function, "deployriotshield", |world, _, args| {
        let owner = super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        let shield_ent = super::super::engine::entity_id(world, super::arg(args, 1)?)?;
        if world
            .resource::<RoundScript>()
            .player_client(owner)
            .is_none()
        {
            return Err("Owner entity is not a player".into());
        }
        let origin = match world
            .resource_mut::<RoundScript>()
            .object_field(shield_ent, "origin")
        {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        super::spawn_trigger(
            world,
            "trigger_radius_use",
            &[
                Value::string("trigger_radius_use"),
                Value::Vector(origin),
                Value::Int(0),
                Value::Float(72.0),
                Value::Float(96.0),
            ],
        )
    });
}

/// The engine's `AngleVectors`: forward, right, up.
fn angle_axes(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    (
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    )
}
