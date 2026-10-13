//! Black Ops 2 builtins whose handlers keep a flag or check their
//! arguments: actors' pitch orientation, the load-time turret and vehicle
//! precaches, a player's snapshot acknowledgement, local and team sounds,
//! shell shock, the UI's notify events, and the client's exploders, fog
//! and cull distance. What IW4L does not apply of them is listed in
//! `docs/fidelity/t6.md`.

use super::super::super::entities::EntityKind;
use super::string;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// Each actor's pitch settings: oriented to its slope, and its pitch angle
/// allowed (`t6.actors.pitch`: kept, not applied).
#[derive(Resource, Default)]
struct ActorPitch(BTreeMap<u64, (bool, bool)>);

fn actor(world: &World, receiver: &Value) -> Option<u64> {
    let id = super::super::engine::entity_id(world, receiver).ok()?;
    (world.resource::<RoundScript>().entities[&id].kind == EntityKind::Actor).then_some(id)
}

/// Black Ops 2's LUI notify events, checked and kept: IW4L's server
/// sends no UI commands (`t6.ui.lui_notify`).
#[derive(Resource, Default)]
pub(crate) struct LuiNotifies(pub(crate) Vec<(Option<u32>, Vec<Value>)>);

/// The engine's LUINotifyEvent check: an event name, then the count of
/// the parameters that follow, each a localized string or an integer.
fn lui_notify(world: &mut World, client: Option<u32>, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Error need at least one argument for LUINotifyEvent.".into());
    }
    if args.len() > 1 {
        let count = match args[1] {
            Value::Int(n) => n,
            _ => return Err("LUINotifyEvent expects a parameter count".into()),
        };
        if args.len() as i64 != i64::from(count) + 2 {
            return Err(format!(
                "Argument and parameter count mismatch. args: {} paramters: {}",
                args.len(),
                i64::from(count) + 2
            ));
        }
    }
    if args[2.min(args.len())..]
        .iter()
        .any(|arg| !matches!(arg, Value::Int(_) | Value::LocalizedString(_)))
    {
        return Err("GScr_LUINotifyEvent: Expected Istrings or integers only.".into());
    }
    world
        .get_resource_or_insert_with(LuiNotifies::default)
        .0
        .push((client, args.to_vec()));
    Ok(Value::Undefined)
}

fn lui_player(world: &World, receiver: &Value) -> Result<u32, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    world
        .resource::<RoundScript>()
        .player_client(id)
        .ok_or_else(|| "LUINotifyEvent: entity must be a player entity".into())
}

const VOLFOG_USAGE: &str = "Incorrect number of parameters\nUSAGE: setVolFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <transition time>)\nOR:    SetVolFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <fogColorScale>, <sunFogRed>, <sunFogGreen>, <sunFogBlue>, <sunFogDirX>, <sunFogDirY>, <sunFogDirZ>, <sunFogStartAng>, <sunFogEndAng>, <transition time>)\n";

fn number(value: Option<&Value>) -> Option<f32> {
    match value? {
        Value::Int(n) => Some(*n as f32),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// A player's sentient fields, cleared when its entity connects, as the
/// engine's sentient has them.
const SENTIENT_FIELDS: [&str; 7] = [
    "threatbias",
    "ignoreme",
    "ignoreall",
    "ignoreforfriendlyfire",
    "ignorerandombulletdamage",
    "turretinvulnerability",
    "inmeleecharge",
];

fn clear_sentient(world: &mut World, player: u64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    for field in SENTIENT_FIELDS {
        runtime.set_object_field(player, field, Value::Int(0));
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.on_connect(clear_sentient);

    // gsc_activateclientexploder: the engine sends the exploder's number to
    // every client (`t6.fx.client_exploders`: not sent).
    registry.register(
        Function,
        "activateclientexploder",
        |_, _, args| match args.first() {
            Some(Value::Int(_)) => Ok(Value::Undefined),
            _ => Err("activateclientexploder expects an exploder number".into()),
        },
    );
    // gsc_setculldist: the clients' draw distance (`t6.vision.fog`: not sent).
    registry.register(Function, "setculldist", |_, _, args| {
        number(args.first()).ok_or("setculldist expects a distance")?;
        Ok(Value::Undefined)
    });
    // gsc_setvolfog: the engine's argument checks (`t6.vision.fog`: the fog
    // is not sent).
    registry.register(Function, "setvolfog", |_, _, args| {
        if args.len() != 8 && args.len() != 18 {
            return Err(VOLFOG_USAGE.into());
        }
        let values = args
            .iter()
            .map(|arg| number(Some(arg)))
            .collect::<Option<Vec<f32>>>()
            .ok_or("setVolFog expects numbers")?;
        if values[0] < 0.0 {
            return Err("setExpFog: startDist must be greater or equal to 0".into());
        }
        if values[1] <= 0.0 {
            return Err("setVolFog: halfwayDist must be greater than 0".into());
        }
        if values[2] < 0.0 {
            return Err("setVolFog: halfwayHeight must be greater or equal to 0".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "luinotifyevent", |world, _, args| {
        lui_notify(world, None, args)
    });
    registry.register(Method, "luinotifyevent", |world, receiver, args| {
        let client = lui_player(world, receiver)?;
        lui_notify(world, Some(client), args)
    });
    registry.register(
        Method,
        "luinotifyeventtospectators",
        |world, receiver, args| {
            let client = lui_player(world, receiver)?;
            lui_notify(world, Some(client), args)
        },
    );

    registry.register(Method, "setpitchorient", |world, receiver, _| {
        let id = actor(world, receiver).ok_or("not an actor")?;
        world
            .get_resource_or_insert_with(ActorPitch::default)
            .0
            .entry(id)
            .or_default()
            .0 = true;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearpitchorient", |world, receiver, _| {
        let id = actor(world, receiver).ok_or("not an actor")?;
        world
            .get_resource_or_insert_with(ActorPitch::default)
            .0
            .entry(id)
            .or_default()
            .0 = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowpitchangle", |world, receiver, args| {
        let Some(id) = actor(world, receiver) else {
            return Err("AllowPitchAngle() called on non-actor\n".into());
        };
        let on = !matches!(args.first(), Some(Value::Int(0)));
        world
            .get_resource_or_insert_with(ActorPitch::default)
            .0
            .entry(id)
            .or_default()
            .1 = on;
        Ok(Value::Undefined)
    });
    // gsc_precacheturret: before the level's first wait; the turret's
    // weapon is registered.
    registry.register(Function, "precacheturret", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err(
                "precacheTurret must be called before any wait statements in the level script\n"
                    .into(),
            );
        }
        let name = string(args, 0)?;
        super::weapon::weapon_index(world, &name)?;
        Ok(Value::Undefined)
    });
    // gsc_precachevehicle: before the level's first wait
    // (`t6.vehicles.definitions`: the vehicle's definition is not checked).
    registry.register(Function, "precachevehicle", |world, _, args| {
        string(args, 0)?;
        if !world.resource::<RoundScript>().loading {
            return Err(
                "precacheVehicle must be called before any wait statements in the level script\n"
                    .into(),
            );
        }
        Ok(Value::Undefined)
    });
    // gscm_getsnapshotackindex: the last snapshot the player's client
    // acknowledged; IW4L's clients are taken to have every one.
    registry.register(Method, "getsnapshotackindex", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        }
        let tick = world.resource::<crate::step::StepRequest>().tick;
        Ok(Value::Int(i32::try_from(tick.0).unwrap_or(i32::MAX)))
    });
    // gscm_playlocalsound: the sound for that player alone.
    registry.register(Method, "playlocalsound", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        };
        let alias = string(args, 0)?;
        let origin = match super::super::super::players::entity_field(world, id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        super::super::engine::sound_to_client(world, crate::ClientId(client), &alias, origin);
        Ok(Value::Undefined)
    });
    // gscm_playsoundtoteam(alias, team, [player]): the sound at the entity
    // for that team's players, but the one named.
    registry.register(Method, "playsoundtoteam", |world, receiver, args| {
        let alias = string(args, 0)?;
        let team = string(args, 1)?;
        if super::team_index(&team).is_none() {
            return Err(format!("'{team}' is an illegal team string"));
        }
        let skip = match args.get(2) {
            Some(value) => Some(
                world
                    .resource::<RoundScript>()
                    .player_client_of(value)
                    .ok_or("entity is not a player")?,
            ),
            None => None,
        };
        let id = super::super::engine::entity_id(world, receiver)?;
        let origin = match super::super::super::players::entity_field(world, id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let clients: Vec<(u32, u64)> = world
            .resource::<RoundScript>()
            .players
            .iter()
            .map(|(client, slot)| (*client, slot.object))
            .collect();
        for (client, object) in clients {
            if Some(client) == skip {
                continue;
            }
            let on_team = matches!(
                world.resource_mut::<RoundScript>().object_field(object, "team"),
                Value::String(own) if *own == *team
            );
            if on_team {
                super::super::engine::sound_to_client(
                    world,
                    crate::ClientId(client),
                    &alias,
                    origin,
                );
            }
        }
        Ok(Value::Undefined)
    });
    // gscm_shellshock(name, duration, [...]): a precached shock, the
    // duration 0 to 60 s; kept for the player's client to present
    // (`t6.vision.shellshock`: no shock is shown).
    registry.register(Method, "shellshock", |world, receiver, args| {
        let name = string(args, 0)?.to_ascii_lowercase();
        if !world
            .resource::<crate::script::MatchScript>()
            .precached
            .contains_key(&("shellshock", name.clone()))
        {
            return Err(format!("shellshock '{name}' was not precached\n"));
        }
        let duration = number(args.get(1)).ok_or("shellshock expects a duration")?;
        let ms = (duration * 1000.0).round();
        if !(0.0..=60_000.0).contains(&ms) {
            return Err(format!("duration {} should be >= 0 and <= 60", ms * 0.001));
        }
        super::super::player::present(world, receiver, "shellshock", args)
    });
}
