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
    /// The actor's script object.
    pub id: u64,
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
                id,
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
    int(field(runtime, id, name)).or_else(|| {
        // `_laststand::player_is_in_laststand` tests `IsDefined(self.revivetrigger)`;
        // a solo down without a revive life sets `self.intermission` before
        // `revive_trigger_spawn`.
        (owner != ScriptOwner::Level && name == "laststand").then(|| {
            if int(field(runtime, id, "intermission")) == Some(1) {
                Some(1)
            } else {
                match field(runtime, id, "revivetrigger")? {
                    Value::Object(_) => Some(1),
                    _ => Some(0),
                }
            }
        })?
    })
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
    /// The `use_elec_switch` trigger (TranZit's
    /// `powerswitch_buildable_trigger_power`), while shown.
    Power,
    /// A perk machine's `zombie_vending` trigger.
    Perk,
    /// Pack-a-Punch: the `zombie_vending` trigger that sells
    /// `specialty_weapupgrade`.
    PackAPunch,
    /// A window whose boards are down: its rebuild unitrigger stub
    /// (`level.exterior_goals[i].unitrigger_stub`, while registered).
    Rebuild,
    /// A buildable's part lying where it spawned: its unitrigger stub
    /// (`level.buildable_stubs[i].buildablezone.pieces[j].unitrigger`); the
    /// site's weapon is the buildable's `equipname`.
    Piece,
    /// A buildable's table not yet built (`level.buildable_stubs[i]` with
    /// `built` 0); the site's weapon is its `equipname`.
    Build,
    /// A persistent buildable's table once built (`persistent` 1, `built`
    /// 1): `bptrigger_think_persistent` gives its weapon on use.
    Take,
    /// The bank teller's transfer trigger (`level.bank_teller_transfer_trig`)
    /// or the `teller_withdrawl` power-up it drops.
    Teller,
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
    /// A radius trigger's height: it stands on its origin.
    pub height: Option<f32>,
    /// A trigger the player's body must touch (`trigger_use_touch`), not
    /// one reached from the eye.
    pub touch: bool,
    /// What the scripts charge: `zombie_cost` on a door (1000 when unset;
    /// a door the power opens is left out), debris or the box, `cost` on a
    /// wall weapon's stub.
    pub cost: Option<i32>,
    /// The wall weapon's name, or the perk a machine sells.
    pub weapon: Option<String>,
    /// The zone a unitrigger stub was filed under (`in_zone`): only stubs of
    /// the active zones (and those of none) are offered to a player.
    pub zone: Option<String>,
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
    let unitrigger_type = string(field(runtime, id, "script_unitrigger_type"));
    let parent = object(field(runtime, id, "origin_parent"));
    let origin = parent
        .and_then(|parent| {
            let base = vector(field(runtime, parent, "origin"))?;
            Some([base[0], base[1], base[2] + 35.0])
        })
        .or_else(|| vector(field(runtime, id, "origin")))?;
    let angles = parent
        .and_then(|parent| vector(field(runtime, parent, "angles")))
        .or_else(|| vector(field(runtime, id, "angles")))
        .unwrap_or([0.0; 3]);
    Some(UseSite {
        kind,
        object: id,
        origin,
        angles,
        brush: runtime.entities.get(&id).and_then(|entity| entity.brush),
        height: runtime
            .entities
            .get(&id)
            .and_then(|entity| entity.cylinder)
            .map(|(_, height)| height)
            .or_else(|| {
                if parent.is_some() {
                    Some(16.0)
                } else {
                    unitrigger_type
                        .as_deref()
                        .is_some_and(|t| t.contains("radius"))
                        .then_some(48.0)
                }
            }),
        touch: runtime
            .entities
            .get(&id)
            .is_some_and(|entity| &*entity.classname == "trigger_use_touch")
            || unitrigger_type
                .as_deref()
                .is_some_and(|t| t.contains("box")),
        cost: int(field(runtime, id, cost)),
        weapon: string(field(runtime, id, "zombie_weapon_upgrade")),
        zone: string(field(runtime, id, "in_zone")),
    })
}

fn weapon_cost(runtime: &RoundScript, weapon: &str) -> Option<i32> {
    let Value::Array(weapons) = field(runtime, 0, "zombie_weapons")? else {
        return None;
    };
    let entry = runtime
        .arrays
        .get(&weapons)?
        .get(&crate::script::ArrayKey::String(weapon.into()))?
        .clone();
    int(field(runtime, object(Some(entry))?, "cost"))
}

pub(crate) fn use_sites(world: &World) -> Vec<UseSite> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    let mut sites: Vec<UseSite> = runtime
        .entities
        .keys()
        .filter(|id| {
            runtime.live(id)
                && !int(field(runtime, **id, "trigger_off")).is_some_and(|off| off != 0)
                && !int(field(runtime, **id, "_door_open")).is_some_and(|open| open != 0)
        })
        .filter_map(|&id| {
            let kind = match string(field(runtime, id, "targetname"))?.as_str() {
                "zombie_door" => UseKind::Door,
                "zombie_debris" => UseKind::Debris,
                "weapon_upgrade" => UseKind::WallBuy,
                // TranZit's lever: zm_transit_power's electricswitch shows it
                // once the powerswitch buildable is built.
                "use_elec_switch" | "powerswitch_buildable_trigger_power" => UseKind::Power,
                "zombie_vending" => UseKind::Perk,
                "zombie_vending_upgrade" => UseKind::PackAPunch,
                _ => return None,
            };
            if kind == UseKind::WallBuy {
                let mut found = site(runtime, kind, id, "cost")?;
                if found.cost.is_none() {
                    found.cost = found
                        .weapon
                        .as_deref()
                        .and_then(|w| weapon_cost(runtime, w));
                }
                return Some(found);
            }
            if kind == UseKind::PackAPunch {
                let found = site(runtime, kind, id, "cost")?;
                found.cost?;
                return Some(found);
            }
            if kind == UseKind::Power
                && runtime
                    .entities
                    .get(&id)
                    .is_some_and(|entity| entity.hidden && entity.shown_to == 0)
            {
                return None;
            }
            // _zm_perks' vending_trigger_think keeps the price on the
            // trigger (`cost`); the perk is its `script_noteworthy`.
            if kind == UseKind::Perk {
                let mut found = site(runtime, kind, id, "cost")?;
                found.weapon = string(field(runtime, id, "script_noteworthy"));
                if found.weapon.as_deref() == Some("specialty_weapupgrade") {
                    found.kind = UseKind::PackAPunch;
                }
                return Some(found);
            }
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
    for goal in items(runtime, field(runtime, 0, "exterior_goals")) {
        let Some(stub) =
            object(Some(goal)).and_then(|g| object(field(runtime, g, "unitrigger_stub")))
        else {
            continue;
        };
        if int(field(runtime, stub, "registered")).is_some_and(|on| on != 0) {
            sites.extend(site(runtime, UseKind::Rebuild, stub, "cost"));
        }
    }
    for waiter in &runtime.waiters {
        if &*waiter.name == "all_boards_repaired"
            && let Value::Object(trigger) = waiter.receiver
            && runtime.live(&trigger)
        {
            sites.extend(site(runtime, UseKind::Rebuild, trigger, "cost"));
        }
    }
    for stub in items(runtime, field(runtime, 0, "buildable_stubs")) {
        let Some(stub) = object(Some(stub)) else {
            continue;
        };
        let equipname = object(field(runtime, stub, "bound_to_buildable"))
            .and_then(|b| string(field(runtime, b, "equipname")))
            .or_else(|| string(field(runtime, stub, "equipname")));
        if int(field(runtime, stub, "built")) == Some(0)
            && int(field(runtime, stub, "registered")) != Some(0)
            && let Some(mut table) = site(runtime, UseKind::Build, stub, "cost")
        {
            table.weapon = equipname.clone();
            sites.push(table);
        }
        if int(field(runtime, stub, "built")) == Some(1)
            && int(field(runtime, stub, "registered")) != Some(0)
            && let Some(mut table) = site(runtime, UseKind::Take, stub, "cost")
        {
            table.weapon = equipname.clone();
            sites.push(table);
        }
        let Some(zone) = object(field(runtime, stub, "buildablezone")) else {
            continue;
        };
        for piece in items(runtime, field(runtime, zone, "pieces")) {
            let Some(piece_id) = object(Some(piece)) else {
                continue;
            };
            let Some(trigger) = object(field(runtime, piece_id, "unitrigger")) else {
                continue;
            };
            if let Some(mut part) = site(runtime, UseKind::Piece, trigger, "cost") {
                part.weapon =
                    string(field(runtime, piece_id, "buildablename")).or_else(|| equipname.clone());
                if !sites.iter().any(|s| s.object == part.object) {
                    sites.push(part);
                }
            }
        }
    }
    let chests = items(runtime, field(runtime, 0, "chests"));
    for chest in chests.into_iter().filter_map(|c| object(Some(c))) {
        if int(field(runtime, chest, "hidden")) == Some(1) {
            continue;
        }
        let stub = object(field(runtime, chest, "unitrigger_stub")).unwrap_or(chest);
        if let Some(mut box_site) = site(runtime, UseKind::Box, stub, "cost") {
            box_site.cost = int(field(runtime, chest, "zombie_cost"))
                .or_else(|| int(field(runtime, chest, "old_cost")));
            sites.push(box_site);
        }
    }
    let teller_drop = items(runtime, field(runtime, 0, "active_powerups"))
        .into_iter()
        .filter_map(|p| object(Some(p)))
        .find(|&p| {
            runtime.live(&p)
                && string(field(runtime, p, "powerup_name")).as_deref() == Some("teller_withdrawl")
        });
    if let Some(drop_id) = teller_drop {
        if let Some(mut drop_site) = site(runtime, UseKind::Teller, drop_id, "cost") {
            drop_site.cost = Some(0);
            drop_site.touch = true;
            sites.push(drop_site);
        }
    } else if let Some(teller) = object(field(runtime, 0, "bank_teller_transfer_trig"))
        && runtime.live(&teller)
        && !int(field(runtime, teller, "trigger_off")).is_some_and(|off| off != 0)
        && let Some(mut teller_site) = site(runtime, UseKind::Teller, teller, "cost")
    {
        teller_site.cost = Some(1000 + int(field(runtime, 0, "ta_tellerfee")).unwrap_or(100));
        sites.push(teller_site);
    }
    sites
}

/// An entry of a level array field (`level.<array>[key]`) as text, for a
/// test player's log.
pub(crate) fn level_array_entry(world: &World, array: &str, key: &str) -> Option<String> {
    let runtime = world.get_resource::<RoundScript>()?;
    let (owner, key) = match key.strip_prefix("player.") {
        Some(rest) => (super::entity_damage::player_object_id(world, 0)?, rest),
        None => (0, key),
    };
    let Some(Value::Array(mut id)) = field(runtime, owner, array) else {
        return None;
    };
    // `team.key` reads a nested array (`level.zombie_vars["allies"][key]`).
    let mut parts = key.split('.').peekable();
    let mut value = Value::Undefined;
    while let Some(part) = parts.next() {
        value = runtime
            .arrays
            .get(&id)?
            .get(&crate::script::ArrayKey::String(part.into()))?
            .clone();
        if parts.peek().is_some() {
            let Value::Array(inner) = value else {
                return None;
            };
            id = inner;
        }
    }
    Some(match value {
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::String(text) => text.to_string(),
        other => format!("{other:?}"),
    })
}

/// A script object's field as text (`-` when undefined), for a test
/// player's log.
pub(crate) fn object_field_text(world: &World, id: u64, name: &str) -> String {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return "-".to_owned();
    };
    if let Some(csc_field) = name.strip_prefix("csc_") {
        let Some(client_scripts) =
            world.get_resource::<super::client_scripts::ClientScripts>()
        else {
            return "-".to_owned();
        };
        let Some(client_runtime) = client_scripts.world.get_resource::<RoundScript>() else {
            return "-".to_owned();
        };
        if csc_field == "gibcount" {
            return match field(client_runtime, 0, "gibcount") {
                Some(Value::Int(n)) => n.to_string(),
                _ => "-".to_owned(),
            };
        }
        let Some(number) = runtime.entities.get(&id).map(|e| e.number) else {
            return "-".to_owned();
        };
        let Some(client_game) = client_scripts
            .world
            .get_resource::<super::natives::t6::client::game::ClientGame>()
        else {
            return "-".to_owned();
        };
        let Some(cent) = client_game.entities.get(&number) else {
            return "-".to_owned();
        };
        return match field(client_runtime, cent.object, csc_field) {
            Some(Value::Int(n)) => n.to_string(),
            Some(Value::String(s)) => s.to_string(),
            _ => "-".to_owned(),
        };
    }
    match field(runtime, id, name) {
        None | Some(Value::Undefined) => "-".to_owned(),
        Some(Value::Int(n)) => n.to_string(),
        Some(Value::Float(f)) => format!("{f:.1}"),
        Some(Value::String(text)) => text.to_string(),
        Some(Value::Vector(v)) => format!("({:.1} {:.0} {:.0})", v[0], v[1], v[2]),
        Some(Value::Object(object)) => format!("object {object}"),
        Some(other) => format!("{other:?}"),
    }
}

/// The receivers of the waits pending on a notify name, for a test
/// player's log.
pub(crate) fn waits_on(world: &World, name: &str) -> Vec<String> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    runtime
        .waiters
        .iter()
        .filter(|w| &*w.name == name)
        .map(|w| match &w.receiver {
            Value::Object(id) => runtime.entities.get(id).map_or_else(
                || format!("object {id}"),
                |entity| format!("{} {id}", entity.classname),
            ),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The origin, yaw and entity number of the entity a level field holds,
/// for a test player.
pub(crate) fn level_entity_pose(world: &World, name: &str) -> Option<([f32; 3], f32, i32)> {
    let runtime = world.get_resource::<RoundScript>()?;
    let id = object(field(runtime, 0, name))?;
    let origin = vector(field(runtime, id, "origin"))?;
    let yaw = vector(field(runtime, id, "angles")).map_or(0.0, |a| a[1]);
    Some((origin, yaw, runtime.entities.get(&id)?.number))
}

/// A player's script field as text, for a test player's log.
pub(crate) fn player_field_text(world: &World, client: ClientId, name: &str) -> String {
    match super::entity_damage::player_object_id(world, client.0) {
        Some(id) => object_field_text(world, id, name),
        None => "-".to_owned(),
    }
}

/// Whether a level flag is set (`level.flag[name]`, as `flag_set` keeps it).
pub(crate) fn level_flag(world: &World, name: &str) -> bool {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return false;
    };
    let Some(Value::Array(flags)) = field(runtime, 0, "flag") else {
        return false;
    };
    runtime
        .arrays
        .get(&flags)
        .and_then(|array| array.get(&crate::script::ArrayKey::String(name.into())))
        .and_then(|value| int(Some(value.clone())))
        .is_some_and(|on| on != 0)
}

/// Whether `level.zones[name]` is active (`is_active`): a zone a player is
/// in, or one next to it by an open way.
pub(crate) fn zone_is_enabled(world: &World, name: &str) -> bool {
    zone_flag(world, name, "is_enabled")
}

pub(crate) fn zone_is_active(world: &World, name: &str) -> bool {
    zone_flag(world, name, "is_active")
}

fn zone_flag(world: &World, name: &str, flag: &str) -> bool {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return false;
    };
    let Some(Value::Array(zones)) = field(runtime, 0, "zones") else {
        return false;
    };
    runtime
        .arrays
        .get(&zones)
        .and_then(|array| array.get(&crate::script::ArrayKey::String(name.into())))
        .and_then(|zone| object(Some(zone.clone())))
        .and_then(|zone| int(field(runtime, zone, flag)))
        .is_some_and(|on| on != 0)
}

/// What `_zm_unitrigger` holds for a stub, for a test player's log: the
/// stub's own fields (`registered`, `in_zone`, `test_radius_sq`, its box,
/// how many per-player triggers it has), the zones `main` reads
/// (`level.active_zone_names`) and the sizes of `level._unitriggers`' lists.
pub(crate) fn unitrigger_state(world: &World, stub: u64) -> String {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return String::new();
    };
    let show = |value: Option<Value>| match value {
        None | Some(Value::Undefined) => "undefined".to_owned(),
        Some(Value::String(text)) => text.to_string(),
        Some(Value::Int(n)) => n.to_string(),
        Some(Value::Float(f)) => format!("{f:.1}"),
        Some(Value::Array(_)) => {
            let entries: Vec<String> = items(runtime, value)
                .into_iter()
                .map(|item| match item {
                    Value::Object(id) if runtime.entities.contains_key(&id) => format!(
                        "{}{} at {:?}",
                        runtime.entities[&id].classname,
                        if runtime.live(&id) { "" } else { " (gone)" },
                        vector(field(runtime, id, "origin")).map(|v| v.map(|c| c.round()))
                    ),
                    other => format!("{other:?}"),
                })
                .collect();
            format!("{entries:?}")
        }
        Some(other) => format!("{other:?}"),
    };
    let units = object(field(runtime, 0, "_unitriggers"));
    let list = |name: &str| units.map_or(0, |u| items(runtime, field(runtime, u, name)).len());
    let active: Vec<String> = items(runtime, field(runtime, 0, "active_zone_names"))
        .into_iter()
        .filter_map(|zone| string(Some(zone)))
        .collect();
    format!(
        "registered={} in_zone={} test_radius_sq={} width={} length={} height={} per_player={} playertrigger={} | active zones {active:?} | stubs {} dynamic {} pool {} largest_radius={} | origin {:?}",
        show(field(runtime, stub, "registered")),
        show(field(runtime, stub, "in_zone")),
        show(field(runtime, stub, "test_radius_sq")),
        show(field(runtime, stub, "script_width")),
        show(field(runtime, stub, "script_length")),
        show(field(runtime, stub, "script_height")),
        show(field(runtime, stub, "trigger_per_player")),
        show(field(runtime, stub, "playertrigger")),
        list("trigger_stubs"),
        list("dynamic_stubs"),
        show(units.and_then(|u| field(runtime, u, "trigger_pool"))),
        show(units.and_then(|u| field(runtime, u, "largest_radius"))),
        vector(field(runtime, stub, "origin")),
    )
}

/// Whether a script object still lives (a door's trigger is deleted or
/// turned off once it opens).
pub(crate) fn object_lives(world: &World, id: u64) -> bool {
    world.get_resource::<RoundScript>().is_some_and(|runtime| {
        runtime.live(&id)
            && !int(field(runtime, id, "trigger_off")).is_some_and(|off| off != 0)
            && !int(field(runtime, id, "_door_open")).is_some_and(|open| open != 0)
    })
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
    player_volumes(world, None)
}

/// The player volumes of one zone, enabled or not, for a test player.
pub(crate) fn zone_player_volumes(
    world: &World,
    zone: &str,
) -> Vec<([f32; 3], Option<u32>, Option<u32>)> {
    player_volumes(world, Some(zone))
}

fn player_volumes(world: &World, only: Option<&str>) -> Vec<([f32; 3], Option<u32>, Option<u32>)> {
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
                && string(field(runtime, **id, "targetname")).is_some_and(|zone| match only {
                    Some(only) => zone == only,
                    None => enabled(&zone),
                })
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
