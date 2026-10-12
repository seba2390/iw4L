//! What a test player may read of the script world: the living actors and a
//! few integer fields, without touching the running scripts.

use crate::ClientId;
use crate::script::RoundScript;
use crate::script::Value;
use bevy_ecs::prelude::World;

/// A living actor: where it stands, its health, and the script model that
/// carries its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorSighting {
    pub origin: [f32; 3],
    pub health: i32,
    pub body: Option<crate::ScriptModelId>,
}

/// Whose field a test player reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptOwner {
    Level,
    Player(ClientId),
}

fn field(runtime: &RoundScript, id: u64, name: &str) -> Option<Value> {
    let symbol = runtime
        .program
        .as_ref()?
        .symbol_ids
        .get(name)
        .copied()
        .or_else(|| runtime.dynamic_symbols.get(name).copied())?;
    runtime.objects.get(&id)?.get(&symbol).cloned()
}

fn int(value: Option<Value>) -> Option<i32> {
    match value? {
        Value::Int(n) => Some(n),
        Value::Float(f) => Some(f as i32),
        _ => None,
    }
}

pub(crate) fn living_actors(world: &World) -> Vec<ActorSighting> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    runtime
        .entities
        .iter()
        .filter(|(id, entity)| {
            entity.kind == super::entities::EntityKind::Actor && runtime.live(id)
        })
        .filter_map(|(&id, entity)| {
            let health = int(field(runtime, id, "health"))?;
            let Some(Value::Vector(origin)) = field(runtime, id, "origin") else {
                return None;
            };
            (health > 0).then_some(ActorSighting {
                origin,
                health,
                body: entity.presence,
            })
        })
        .collect()
}

pub(crate) fn script_int(world: &World, owner: ScriptOwner, name: &str) -> Option<i32> {
    let runtime = world.get_resource::<RoundScript>()?;
    let id = match owner {
        ScriptOwner::Level => 0,
        ScriptOwner::Player(client) => super::entity_damage::player_object_id(world, client.0)?,
    };
    int(field(runtime, id, name))
}

/// What a use site is, by the name Black Ops 2's scripts find it under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseKind {
    /// A `zombie_door` trigger.
    Door,
    /// A `zombie_debris` trigger.
    Debris,
    /// A wall weapon's unitrigger stub (`level._spawned_wallbuys[i].trigger_stub`).
    WallBuy,
    /// The shown box's unitrigger stub (`level.chests[level.chest_index].unitrigger_stub`).
    Box,
    /// The `use_elec_switch` trigger.
    Power,
}

/// Something a player buys or uses by pressing use near it: a trigger
/// entity of the map, or a unitrigger stub the scripts make a trigger from
/// when a player comes near.
#[derive(Clone, Debug, PartialEq)]
pub struct UseSite {
    pub kind: UseKind,
    /// The script object (entity or stub struct).
    pub object: u64,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// The brush model a trigger entity's volume is.
    pub brush: Option<u32>,
    /// What the scripts charge: `zombie_cost` on a door (1000 when unset;
    /// a door the power opens is left out), debris or the box, `cost` on a
    /// wall weapon's stub.
    pub cost: Option<i32>,
    /// The wall weapon's name.
    pub weapon: Option<String>,
}

/// What a door or debris costs when the map does not say.
const BLOCKER_COST: i32 = 1000;

fn string(value: Option<Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.to_string()),
        _ => None,
    }
}

fn vector(value: Option<Value>) -> Option<[f32; 3]> {
    match value? {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}

fn object(value: Option<Value>) -> Option<u64> {
    match value? {
        Value::Object(id) => Some(id),
        _ => None,
    }
}

fn items(runtime: &RoundScript, value: Option<Value>) -> Vec<Value> {
    let Some(Value::Array(id)) = value else {
        return Vec::new();
    };
    runtime
        .arrays
        .get(&id)
        .map_or_else(Vec::new, |array| array.values().cloned().collect())
}

fn site(runtime: &RoundScript, kind: UseKind, id: u64, cost: &str) -> Option<UseSite> {
    Some(UseSite {
        kind,
        object: id,
        origin: vector(field(runtime, id, "origin"))?,
        angles: vector(field(runtime, id, "angles")).unwrap_or([0.0; 3]),
        brush: runtime.entities.get(&id).and_then(|entity| entity.brush),
        cost: int(field(runtime, id, cost)),
        weapon: string(field(runtime, id, "zombie_weapon_upgrade")),
    })
}

pub(crate) fn use_sites(world: &World) -> Vec<UseSite> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    let mut sites: Vec<UseSite> = runtime
        .entities
        .keys()
        .filter(|id| runtime.live(id))
        .filter_map(|&id| {
            let kind = match string(field(runtime, id, "targetname"))?.as_str() {
                "zombie_door" => UseKind::Door,
                "zombie_debris" => UseKind::Debris,
                "use_elec_switch" => UseKind::Power,
                _ => return None,
            };
            // A door the power opens is not bought.
            if kind == UseKind::Door
                && string(field(runtime, id, "script_noteworthy"))
                    .is_some_and(|noteworthy| noteworthy.starts_with("electric_"))
            {
                return None;
            }
            let mut found = site(runtime, kind, id, "zombie_cost")?;
            // _zm_blockers' door_init and debris_init: 1000 unless the map
            // sets `zombie_cost`.
            if kind != UseKind::Power {
                found.cost.get_or_insert(BLOCKER_COST);
            }
            Some(found)
        })
        .collect();
    for wallbuy in items(runtime, field(runtime, 0, "_spawned_wallbuys")) {
        let Some(stub) =
            object(Some(wallbuy)).and_then(|w| object(field(runtime, w, "trigger_stub")))
        else {
            continue;
        };
        sites.extend(site(runtime, UseKind::WallBuy, stub, "cost"));
    }
    let chests = items(runtime, field(runtime, 0, "chests"));
    if let Some(chest) = int(field(runtime, 0, "chest_index"))
        .and_then(|index| chests.get(usize::try_from(index).ok()?).cloned())
        .and_then(|chest| object(Some(chest)))
        && let Some(stub) = object(field(runtime, chest, "unitrigger_stub"))
        && let Some(mut box_site) = site(runtime, UseKind::Box, stub, "cost")
    {
        box_site.cost = int(field(runtime, chest, "zombie_cost"));
        sites.push(box_site);
    }
    sites
}

/// Whether a script object still lives (a door's trigger is deleted once
/// it opens).
pub(crate) fn object_lives(world: &World, id: u64) -> bool {
    world
        .get_resource::<RoundScript>()
        .is_some_and(|runtime| runtime.live(&id))
}

/// The classnames of the live use triggers whose origin lies within `reach`
/// of `point`: what a unitrigger stub made for a player who came near.
pub(crate) fn use_triggers_near(world: &World, point: [f32; 3], reach: f32) -> Vec<String> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    runtime
        .entities
        .iter()
        .filter(|(id, entity)| {
            runtime.live(id)
                && matches!(
                    &*entity.classname,
                    "trigger_use"
                        | "trigger_use_touch"
                        | "trigger_radius_use"
                        | "trigger_box_use"
                        | "trigger_radius"
                        | "trigger_box"
                )
        })
        .filter(|(id, _)| {
            vector(field(runtime, **id, "origin")).is_some_and(|origin| {
                (0..3).map(|i| (origin[i] - point[i]).powi(2)).sum::<f32>() <= reach * reach
            })
        })
        .map(|(_, entity)| entity.classname.to_string())
        .collect()
}

/// The volumes of the zones the scripts have enabled (`player_volume`
/// entities whose zone, their targetname, `level.zones` marks
/// `is_enabled`): where Black Ops 2 lets a player stand. Each is the
/// volume's origin, brush model and trigger model.
pub(crate) fn enabled_player_volumes(world: &World) -> Vec<([f32; 3], Option<u32>, Option<u32>)> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    let zones = field(runtime, 0, "zones");
    let enabled = |zone: &str| {
        let Some(Value::Array(id)) = zones else {
            return false;
        };
        runtime
            .arrays
            .get(&id)
            .and_then(|array| array.get(&crate::script::ArrayKey::String(zone.into())))
            .and_then(|zone| object(Some(zone.clone())))
            .and_then(|zone| int(field(runtime, zone, "is_enabled")))
            .is_some_and(|on| on != 0)
    };
    runtime
        .entities
        .iter()
        .filter(|(id, _)| runtime.live(id))
        .filter(|(id, _)| {
            string(field(runtime, **id, "script_noteworthy")).as_deref() == Some("player_volume")
                && string(field(runtime, **id, "targetname")).is_some_and(|zone| enabled(&zone))
        })
        .filter_map(|(id, entity)| {
            Some((
                vector(field(runtime, *id, "origin"))?,
                entity.brush,
                entity.trigger_model,
            ))
        })
        .collect()
}
