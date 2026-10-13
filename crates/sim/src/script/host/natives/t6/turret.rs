//! Black Ops 2's turrets (`spawnturret` and its methods), as the engine's
//! handlers keep them: the turret's state, the carried turret a player sets
//! down (`carryturret`, `canplayerplaceturret`) and the modes, arcs and
//! targets the scripts give it.

use super::riotshield::{axis_to_angles, cross, dvar_f32, dvar_i32, forward, lerp};
use super::{float, int, string};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The turret's mode bits: aims for the AI, aims by itself.
pub(super) const MODE_AI: u32 = 1;
pub(super) const MODE_AUTO: u32 = 2;
/// `startfiring`.
const FIRING: u32 = 4;
/// Usable by a player, until `maketurretunusable`.
const USABLE: u32 = 0x1000;
/// `setturrettype`'s kinds.
const TYPE_WALLMOUNT: u32 = 0x20000;
pub(super) const TYPE_TOW: u32 = 0x40000;
pub(super) const TYPE_SENTRY: u32 = 0x40_0000;
/// The entity's own turret flags: its type, carried, set down.
const ENT_SENTRY: u32 = 1;
const ENT_TOW: u32 = 2;
const ENT_CARRIED: u32 = 8;
const ENT_SET_DOWN: u32 = 0x20;
const ENT_WALLMOUNT: u32 = 0x80;
/// The weapon class a turret's weapon must have.
const WEAPCLASS_TURRET: i32 = 7;
/// A turret's contents when spawned, carried, and once put down.
const CONTENTS_SPAWNED: i32 = 0x0020_0004;
const CONTENTS_PUT_DOWN: i32 = 0x0003_76c0;
const CONTENTS_USABLE: i32 = 0x0020_0000;
/// A spawned turret's box, health and default drop pitch.
const MINS: [f32; 3] = [-32.0, -32.0, 0.0];
const MAXS: [f32; 3] = [32.0, 32.0, 56.0];
const HEALTH: i32 = 100;
const DROP_PITCH: f32 = -90.0;

/// What the engine keeps for a turret (`t6.turret.state`).
#[derive(Clone, Debug)]
pub(super) struct Turret {
    pub(super) weapon: Arc<str>,
    pub(super) def: Arc<asset_core::ZoneWeapon>,
    pub(super) flags: u32,
    pub(super) ent_flags: u32,
    /// 0 idle, 2 aiming to fire, 1 firing.
    pub(super) state: i32,
    pub(super) drop_pitch: f32,
    /// Pitch and yaw convergence, ms.
    pub(super) convergence_ms: [i32; 2],
    pub(super) team: usize,
    pub(super) owner: Option<u64>,
    /// `settargetentity`.
    pub(super) manual_target: Option<u64>,
    /// The entity's firing flag, set by `shootturret` and cleared by
    /// `stopshootturret`.
    pub(super) shooting: bool,
    pub(super) aim: super::sentry::Aim,
}

#[derive(Resource, Default)]
pub(super) struct Turrets(pub(super) BTreeMap<u64, Turret>);

/// What a player's client is told about the turret he carries
/// (`t6.turret.client`: kept, not sent).
#[derive(Resource, Default)]
struct Carriers(BTreeMap<u32, Carrier>);

#[derive(Default)]
struct Carrier {
    carrying: bool,
    cannot_place: bool,
}

/// The turret a method runs on, with the engine's error for any other
/// entity.
fn turret(world: &World, receiver: &Value) -> Result<u64, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    if world
        .get_resource::<Turrets>()
        .is_some_and(|turrets| turrets.0.contains_key(&id))
    {
        return Ok(id);
    }
    let classname = world.resource::<RoundScript>().entities[&id]
        .classname
        .clone();
    Err(format!("entity type '{classname}' is not a turret"))
}

fn edit(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut Turret),
) -> Result<Value, String> {
    let id = turret(world, receiver)?;
    if let Some(turret) = world.resource_mut::<Turrets>().0.get_mut(&id) {
        change(turret);
    }
    Ok(Value::Undefined)
}

fn carrier(world: &mut World, client: u32) -> &mut Carrier {
    world
        .get_resource_or_insert_with(Carriers::default)
        .into_inner()
        .0
        .entry(client)
        .or_default()
}

/// The player a method runs on, with the engine's errors.
fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    let runtime = world.resource::<RoundScript>();
    let (id, entity) = runtime.entity(receiver).ok_or("not an entity")?;
    runtime
        .player_client(id)
        .ok_or_else(|| format!("entity {} is not a player", entity.number))
}

fn set_contents(world: &mut World, id: u64, contents: i32) {
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.contents = contents;
        entity.solid = contents != 0;
    }
}

/// `canplayerplaceturret`'s spot (the engine's sentry placement): 47 ahead
/// of the standing eye (raised by `turretplacement_traceOffset`) pitched 20
/// down, a box of `turret_placement_trace_mins`..`maxs` traced down to the
/// feet's height; ground of normal z 0.7 or more on the world, turned to
/// it, then levelled on its four corners.
fn placement(world: &mut World, client: u32) -> (bool, [f32; 3], [f32; 3]) {
    let Some((origin, yaw)) = FrameWorld::from_world(world)
        .player(crate::ClientId(client))
        .map(|ps| (ps.origin, ps.viewangles[1]))
    else {
        return (false, [0.0; 3], [0.0; 3]);
    };
    let standing = dvar_i32(world, "player_standingviewheight", 60) as f32;
    let mut eye = [origin[0], origin[1], origin[2] + standing];
    if dvar_i32(world, "turretplacement_usetraceoffset", 1) != 0 {
        eye[2] += dvar_f32(world, ("turretplacement_traceoffset", 12.0));
    }
    let mask = if dvar_i32(world, "tu11_zombie_turret_placement_ignores_bodies", 1) != 0 {
        PLACE_MASK_IGNORING_BODIES
    } else {
        PLACE_MASK
    };
    let mut angles = [0.0, yaw, 0.0];
    let ahead = forward(PLACE_PITCH, yaw);
    let start = [0, 1, 2].map(|i| eye[i] + ahead[i] * PLACE_AHEAD);
    let end = [start[0], start[1], start[2] - (eye[2] - origin[2])];
    let mins = dvar_f32(world, ("turret_placement_trace_mins", -42.0));
    let maxs = dvar_f32(world, ("turret_placement_trace_maxs", 42.0));
    let block = super::super::engine::trace(world, start, end, [mins; 3], [maxs; 3], mask);
    let mut spot = lerp(start, end, block.fraction);
    if block.allsolid != 0
        || block.startsolid != 0
        || block.fraction < PLACE_MIN_FRACTION
        || block.normal[2] < PLACE_MIN_NORMAL_Z
    {
        spot[2] = origin[2];
        return (false, spot, angles);
    }
    spot[2] -= maxs;
    if block.move_trace().entity != game_api::movement::ENTITY_WORLD {
        return (false, spot, angles);
    }
    let up = block.normal;
    let left = cross(up, ahead);
    let mut axis = [cross(left, up), left, up];
    angles = axis_to_angles(axis);
    if !level(world, &mut axis, &mut spot, mask) {
        return (false, spot, angles);
    }
    (true, spot, axis_to_angles(axis))
}

/// The engine's levelling of a placed turret: a line down 20 from 10 above
/// each corner (17 ahead and behind, 20 to each side); a corner that meets
/// nothing takes its opposite's drop. It sinks by the larger of the two
/// diagonals' mean drop, and its up axis becomes the corners' normal. Two
/// corners or more over nothing refuse it.
fn level(world: &mut World, axis: &mut [[f32; 3]; 3], spot: &mut [f32; 3], mask: u32) -> bool {
    let [fwd, left, up] = *axis;
    let at = |base: [f32; 3], v: [f32; 3], s: f32| [0, 1, 2].map(|i| base[i] + v[i] * s);
    let c0 = at(at(at(*spot, up, 10.0), left, 20.0), fwd, 17.0);
    let c1 = at(c0, fwd, -34.0);
    let c2 = at(c1, left, -40.0);
    let c3 = at(c2, fwd, 34.0);
    let mut corners = [c0, c1, c2, c3];
    let mut drops = [0.0f32; 4];
    let mut met = [false; 4];
    let mut misses = 0;
    for (i, corner) in corners.iter_mut().enumerate() {
        let below = at(*corner, up, -20.0);
        let line = super::super::engine::trace(world, *corner, below, [0.0; 3], [0.0; 3], mask);
        if line.fraction >= 1.0 {
            misses += 1;
        } else {
            drops[i] = line.fraction * 20.0 - 10.0;
            met[i] = true;
        }
        *corner = at(*corner, up, -drops[i]);
    }
    if !met[0] {
        drops[0] = drops[2];
    }
    if !met[1] {
        drops[1] = drops[3];
    }
    if !met[2] {
        drops[2] = drops[0];
    }
    if !met[3] {
        drops[3] = drops[1];
    }
    let sink = ((drops[0] + drops[2]) * 0.5).max((drops[1] + drops[3]) * 0.5);
    *spot = at(*spot, up, -sink);
    let [c0, c1, c2, c3] = corners;
    let normal = cross(
        [0, 1, 2].map(|i| c0[i] - c2[i]),
        [0, 1, 2].map(|i| c1[i] - c3[i]),
    );
    let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
    let length = if -length >= 0.0 { 1.0 } else { length };
    let up = normal.map(|v| v / length);
    let left = cross(up, fwd);
    *axis = [cross(left, up), left, up];
    misses < 2
}

const PLACE_AHEAD: f32 = 47.0;
const PLACE_PITCH: f32 = 20.0;
const PLACE_MIN_NORMAL_Z: f32 = 0.7;
const PLACE_MIN_FRACTION: f32 = 0.0001;
const PLACE_MASK: u32 = 0x0281_8011;
const PLACE_MASK_IGNORING_BODIES: u32 = 0x0081_0011;

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // gsc_spawnturret(classname, origin, weapon): a turret entity of the
    // turret weapon, which must be one and precached.
    registry.register(Function, "spawnturret", |world, _, args| {
        let classname = string(args, 0)?;
        let origin = super::vector(args, 1)?;
        let weapon = string(args, 2)?;
        let loading = world.resource::<RoundScript>().loading;
        let index = super::weapon::weapon_index(world, &weapon)?;
        let zone = super::weapon::zone_weapon(world, &weapon);
        let Some(def) = zone.filter(|w| w.weap_class == WEAPCLASS_TURRET) else {
            return Err(format!("Could not load turret '{weapon}'"));
        };
        if index == 0 && !loading {
            return Err(format!("turret '{weapon}' not precached"));
        }
        let presence = super::super::super::presence::spawn_presence(world, origin)?;
        let number = FrameWorld::from_world(world).gentity_number(presence);
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(super::EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        runtime.set_object_field(id, "health", Value::Int(HEALTH));
        let entity = runtime.entities.get_mut(&id).unwrap();
        entity.presence = Some(presence);
        entity.contents = CONTENTS_SPAWNED;
        entity.solid = true;
        if let Some(number) = number {
            entity.number = number;
        }
        let to_ms = |seconds: f32| (seconds.max(0.0) * 1000.0 + 0.5) as i32;
        let convergence = def.turret_convergence;
        world
            .get_resource_or_insert_with(Turrets::default)
            .0
            .insert(
                id,
                Turret {
                    weapon: weapon.into(),
                    aim: super::sentry::Aim::new(&def),
                    def,
                    flags: MODE_AI | MODE_AUTO | USABLE,
                    ent_flags: 0,
                    state: 0,
                    drop_pitch: DROP_PITCH,
                    convergence_ms: convergence.map(to_ms),
                    team: 0,
                    owner: None,
                    manual_target: None,
                    shooting: false,
                },
            );
        world
            .get_resource_or_insert_with(super::EntityBounds::default)
            .0
            .insert(id, (MINS, MAXS));
        Ok(Value::Object(id))
    });
    // gscm_setturrettype: sentry, tow, microwave or wallmount.
    registry.register(Method, "setturrettype", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let kind = string(args, 0)?;
        let Some(turret) = world
            .get_resource_mut::<Turrets>()
            .and_then(|turrets| turrets.into_inner().0.get_mut(&id))
        else {
            return Err("setturrettype: t6.turret.type_on_entity: the engine writes a turret's type through an entity without one".into());
        };
        turret.flags &= !(TYPE_WALLMOUNT | TYPE_TOW | TYPE_SENTRY);
        turret.ent_flags &= !(ENT_SENTRY | ENT_TOW);
        let (flag, ent) = match kind.to_ascii_lowercase().as_str() {
            "sentry" => (TYPE_SENTRY, ENT_SENTRY),
            "tow" => (TYPE_TOW, ENT_TOW),
            "microwave" | "wallmount" => (TYPE_WALLMOUNT, ENT_WALLMOUNT),
            _ => return Err(format!("{kind}, Unknown turret type.")),
        };
        turret.flags |= flag;
        turret.ent_flags |= ent;
        Ok(Value::Undefined)
    });
    // gscm_setturretcarried: carried, or set down.
    registry.register(Method, "setturretcarried", |world, receiver, args| {
        let carried = int(args, 0)? != 0;
        edit(world, receiver, |t| {
            if carried {
                t.ent_flags |= ENT_CARRIED;
            } else {
                t.ent_flags = t.ent_flags & !ENT_CARRIED | ENT_SET_DOWN;
            }
        })
    });
    // gscm_setturretowner: not on a player or an actor.
    registry.register(Method, "setturretowner", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let kind = world.resource::<RoundScript>().entities[&id].kind.clone();
        if matches!(kind, super::EntityKind::Player | super::EntityKind::Actor) {
            return Err("SetOwner can not be used for players or actors".into());
        }
        let owner = super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        if let Some(turret) = world
            .get_resource_mut::<Turrets>()
            .and_then(|turrets| turrets.into_inner().0.get_mut(&id))
        {
            turret.owner = Some(owner);
        }
        Ok(Value::Undefined)
    });
    // gscm_carryturret(turret, offset, angles): the turret goes not solid
    // and is linked to the player there; his client carries it.
    registry.register(Method, "carryturret", |world, receiver, args| {
        let client = player(world, receiver)?;
        let carried = super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        let offset = super::vector(args, 1)?;
        let angles = super::vector(args, 2)?;
        set_contents(world, carried, 0);
        carrier(world, client).carrying = true;
        let linkto = NativeRegistry::engine_services()
            .get(Method, "linkto")
            .ok_or("carryturret: no engine linkto")?;
        linkto(
            world,
            &Value::Object(carried),
            &[
                receiver.clone(),
                Value::string(""),
                Value::Vector(offset),
                Value::Vector(angles),
            ],
        )?;
        Ok(Value::Undefined)
    });
    // gscm_stopcarryturret(turret, [origin], [angles]): unlinked, put there,
    // and given its put-down contents.
    registry.register(Method, "stopcarryturret", |world, receiver, args| {
        let client = player(world, receiver)?;
        let carried = super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        let unlink = NativeRegistry::engine_services()
            .get(Method, "unlink")
            .ok_or("stopcarryturret: no engine unlink")?;
        unlink(world, &Value::Object(carried), &[])?;
        if args.len() > 1 {
            let origin = super::vector(args, 1)?;
            world.resource_mut::<RoundScript>().set_object_field(
                carried,
                "origin",
                Value::Vector(origin),
            );
        }
        if args.len() > 2 {
            let angles = super::vector(args, 2)?;
            world.resource_mut::<RoundScript>().set_object_field(
                carried,
                "angles",
                Value::Vector(angles),
            );
        }
        set_contents(world, carried, CONTENTS_PUT_DOWN);
        carrier(world, client).carrying = false;
        Ok(Value::Undefined)
    });
    // gscm_canplayerplaceturret([turret]): ["result"], ["origin"],
    // ["angles"]; the client is told when it cannot go there.
    registry.register(Method, "canplayerplaceturret", |world, receiver, args| {
        let client = player(world, receiver)?;
        let wallmount = match args.first() {
            None | Some(Value::Undefined) => false,
            Some(value) => {
                let id = super::super::engine::entity_id(world, value)?;
                world
                    .get_resource::<Turrets>()
                    .and_then(|turrets| turrets.0.get(&id))
                    .is_some_and(|t| t.ent_flags & ENT_WALLMOUNT != 0)
            }
        };
        if wallmount {
            return Err("canplayerplaceturret: t6.turret.wallmount: a wall-mounted turret's placement test is not implemented yet".into());
        }
        let (fits, origin, angles) = placement(world, client);
        carrier(world, client).cannot_place = !fits;
        super::super::engine::keyed_array(
            world,
            vec![
                ("result", Value::Int(fits.into())),
                ("origin", Value::Vector(origin)),
                ("angles", Value::Vector(angles)),
            ],
        )
    });
    // gscm_makeunusable: a vehicle aside, the entity's use contents go.
    registry.register(Method, "makeunusable", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.contents &= !CONTENTS_USABLE;
        }
        super::super::super::physics::usable(world, receiver)?.enabled = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "maketurretunusable", |world, receiver, _| {
        edit(world, receiver, |t| t.flags &= !USABLE)
    });
    // gscm_setmode: auto_ai, manual, manual_ai or auto_nonai.
    registry.register(Method, "setmode", |world, receiver, args| {
        let id = turret(world, receiver)?;
        let mode = string(args, 0)?;
        let (clear, set) = match mode.as_str() {
            "auto_ai" => (0, MODE_AI | MODE_AUTO),
            "manual" => (MODE_AI | MODE_AUTO, 0),
            "manual_ai" => (MODE_AUTO, MODE_AI),
            "auto_nonai" => (MODE_AI, MODE_AUTO),
            _ => return Err("Error setting the mode of a turret.\n".into()),
        };
        let turret = world
            .resource_mut::<Turrets>()
            .into_inner()
            .0
            .get_mut(&id)
            .unwrap();
        turret.flags = turret.flags & !clear | set;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setdefaultdroppitch", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let pitch = float(args, 0)?;
        match world
            .get_resource_mut::<Turrets>()
            .and_then(|turrets| turrets.into_inner().0.get_mut(&id))
        {
            Some(turret) => turret.drop_pitch = pitch,
            None => return Err("entity is not a turret".into()),
        }
        Ok(Value::Undefined)
    });
    // gscm_setconvergencetime(seconds, ["pitch" | "yaw"]): yaw unless told.
    registry.register(Method, "setconvergencetime", |world, receiver, args| {
        let id = turret(world, receiver)?;
        let axis = match args.get(1) {
            None => 1,
            Some(_) => match string(args, 1)?.as_str() {
                "yaw" => 1,
                "pitch" => 0,
                _ => return Err("Convergence type should be either 'pitch' or 'yaw'".into()),
            },
        };
        let ms = (float(args, 0)? * 1000.0) as i32;
        world
            .resource_mut::<Turrets>()
            .into_inner()
            .0
            .get_mut(&id)
            .unwrap()
            .convergence_ms[axis] = ms;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturretteam", |world, receiver, args| {
        let id = turret(world, receiver)?;
        let team = super::team(args, 0)?;
        world
            .resource_mut::<Turrets>()
            .into_inner()
            .0
            .get_mut(&id)
            .unwrap()
            .team = team;
        Ok(Value::Undefined)
    });
    registry.register(Method, "settargetentity", |world, receiver, args| {
        let id = turret(world, receiver)?;
        let target = super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        world
            .resource_mut::<Turrets>()
            .into_inner()
            .0
            .get_mut(&id)
            .unwrap()
            .manual_target = Some(target);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleartargetentity", |world, receiver, _| {
        edit(world, receiver, |t| t.manual_target = None)
    });
    registry.register(Method, "startfiring", |world, receiver, _| {
        edit(world, receiver, |t| t.flags |= FIRING)
    });
    registry.register(Method, "stopfiring", |world, receiver, _| {
        edit(world, receiver, |t| t.flags &= !FIRING)
    });
    registry.register(Method, "stopshootturret", |world, receiver, _| {
        edit(world, receiver, |t| t.shooting = false)
    });
    // gscm_shootturret: one fire of the turret's weapon, nobody using it.
    registry.register(Method, "shootturret", |world, receiver, _| {
        let id = turret(world, receiver)?;
        super::sentry::shoot(world, id)?;
        edit(world, receiver, |t| t.shooting = true)
    });
    registry.register(Method, "isfiringturret", |world, receiver, _| {
        let id = turret(world, receiver)?;
        Ok(Value::Int(
            (world.resource::<Turrets>().0[&id].state != 0).into(),
        ))
    });
    registry.on_tick(forget_deleted);
    registry.on_tick(super::sentry::think);
}

/// A deleted turret's state goes with it.
fn forget_deleted(world: &mut World) {
    let gone: Vec<u64> = match world.get_resource::<Turrets>() {
        Some(turrets) => {
            let runtime = world.resource::<RoundScript>();
            turrets
                .0
                .keys()
                .filter(|id| !runtime.entities.contains_key(id))
                .copied()
                .collect()
        }
        None => return,
    };
    let mut turrets = world.resource_mut::<Turrets>();
    for id in gone {
        turrets.0.remove(&id);
    }
}
