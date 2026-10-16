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
use std::sync::Arc;

/// The owner and team scripts gave entities other than players and actors.
#[derive(Resource, Default)]
pub(super) struct OwnerTeams(pub(super) BTreeMap<u64, (Option<u64>, Option<Arc<str>>)>);

/// Each actor's gib flags, as the engine sends them in its gib event.
#[derive(Resource, Default)]
struct Gibs(BTreeMap<u64, u32>);

/// Pending gib events `(entity_number, flags)` for the client script VM
/// (`CodeCallback_GibEvent`).
#[derive(Resource, Default)]
pub(crate) struct GibEvents(pub(crate) Vec<(i32, u32)>);

/// The entity, refused for players and actors as SetOwner and SetTeam
/// refuse them.
fn owned(world: &World, receiver: &Value, builtin: &str) -> Result<u64, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    match world.resource::<RoundScript>().entities[&id].kind {
        EntityKind::Player | EntityKind::Actor => {
            Err(format!("{builtin} can not be used for players or actors"))
        }
        _ => Ok(id),
    }
}

/// Each actor's pitch settings: oriented to its slope, and its pitch angle
/// allowed.
#[derive(Resource, Default)]
pub(super) struct ActorPitch(pub(super) BTreeMap<u64, (bool, bool)>);

fn actor(world: &World, receiver: &Value) -> Option<u64> {
    let id = super::super::engine::entity_id(world, receiver).ok()?;
    (world.resource::<RoundScript>().entities[&id].kind == EntityKind::Actor).then_some(id)
}

/// Black Ops 2's LUI notify events, checked and delivered to the client's LUI menus.
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
    let name = match &args[0] {
        Value::LocalizedString(s) => s.to_string(),
        Value::String(s) => s.to_string(),
        _ => string(args, 0)?.to_string(),
    };
    let data: Vec<i32> = args[2.min(args.len())..]
        .iter()
        .map(|arg| match arg {
            Value::Int(n) => *n,
            _ => 0,
        })
        .collect();
    if let Some(mut frame) = crate::frame::FrameWorld::try_from_world(world) {
        let notify = game_api::ui::LuiNotify { name, data };
        match client {
            Some(id) => frame
                .client_meta_mut(crate::world::ClientId(id))
                .push_lui_notify(notify),
            None => {
                for id in frame.client_ids_sorted() {
                    frame.client_meta_mut(id).push_lui_notify(notify.clone());
                }
            }
        }
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

    // gscm_allowlean: the player's no-lean flag.
    registry.register(Method, "allowlean", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        };
        let allowed = !matches!(args.first(), Some(Value::Int(0)));
        crate::game_move::restrict(
            &mut crate::frame::FrameWorld::from_world(world),
            crate::ClientId(client),
            game_api::movement::MoveRestriction::Lean,
            allowed,
        );
        Ok(Value::Undefined)
    });

    registry.register(Method, "setowner", |world, receiver, args| {
        let id = owned(world, receiver, "SetOwner")?;
        let owner = super::super::engine::entity_id(
            world,
            args.first().ok_or("SetOwner expects an entity")?,
        )?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "owner", Value::Object(owner));
        world
            .get_resource_or_insert_with(OwnerTeams::default)
            .0
            .entry(id)
            .or_default()
            .0 = Some(owner);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setteam", |world, receiver, args| {
        let id = owned(world, receiver, "SetTeam")?;
        let team = string(args, 0)?;
        let Some(team_idx) = super::team_index(&team) else {
            return Err(format!("'{team}' is an illegal team string"));
        };
        let classname = world.resource::<RoundScript>().entities[&id]
            .classname
            .clone();
        world.resource_mut::<RoundScript>().set_object_field(
            id,
            "team",
            Value::String(team.clone().into()),
        );
        if super::HINT_CLASSES.contains(&&*classname) {
            world
                .get_resource_or_insert_with(super::EntityFlags::default)
                .trigger_team
                .insert(id, team_idx);
        }
        world
            .get_resource_or_insert_with(OwnerTeams::default)
            .0
            .entry(id)
            .or_default()
            .1 = Some(team.into());
        Ok(Value::Undefined)
    });
    // gscm_getentitytype: the entity's type (`ent->s.eType`).
    registry.register(Method, "getentitytype", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let entity = &world.resource::<RoundScript>().entities[&id];
        let kind = match &entity.kind {
            EntityKind::Player => 1,
            EntityKind::Corpse { .. } => 2,
            EntityKind::Item(_) => 3,
            EntityKind::Missile(_) => 4,
            EntityKind::Vehicle => 14,
            EntityKind::Actor => 16,
            EntityKind::ActorCorpse => 18,
            EntityKind::Map | EntityKind::Spawned => match &*entity.classname {
                "script_model" | "script_brushmodel" | "script_origin" => 6,
                "sound_blend" => 7,
                "fx" => 8,
                "loopfx" => 9,
                "primary_light" => 10,
                "misc_turret" => 11,
                "helicopter" => 12,
                "plane" => 13,
                c if c.starts_with("script_vehicle") => 14,
                c if c.starts_with("actor_") => 17,
                c if c.starts_with("zbarrier") => 20,
                _ => 0,
            },
            EntityKind::HudElem => 0,
        };
        Ok(Value::Int(kind))
    });
    // gscm_gib(type, tags): "freeze" sets bit 8 and "up" bit 9, each tag
    // (0 to 7) its own bit, for the engine's gib event.
    registry.register(Method, "gib", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let mut flags = match args.first() {
            Some(Value::String(kind)) => match &**kind {
                "freeze" => 0x100,
                "up" => 0x200,
                _ => 0,
            },
            _ => 0,
        };
        let tags = super::super::super::arrays::array_values(
            world,
            args.get(1).ok_or("Parameter must be an array")?,
        )?;
        for (index, tag) in tags.iter().enumerate() {
            match tag {
                Value::Int(bit @ 0..=7) => flags |= 1 << bit,
                Value::Int(_) => {
                    return Err(
                        "Gib tag array passed to 'Gib' contains value out of range 0 -> 7".into(),
                    );
                }
                _ => {
                    return Err(format!(
                        "Array passed to gib contained member [{index}] - valid types are int."
                    ));
                }
            }
        }
        let number = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .map_or(-1, |e| e.number);
        if number >= 0 {
            world
                .get_resource_or_insert_with(GibEvents::default)
                .0
                .push((number, flags));
        }
        world
            .get_resource_or_insert_with(Gibs::default)
            .0
            .insert(id, flags);
        Ok(Value::Undefined)
    });

    // gsc_activateclientexploder / deactivateclientexploder: the engine sends
    // the exploder's number to every client, negated to stop it.
    registry.register(Function, "activateclientexploder", |world, _, args| {
        let number = super::int(args, 0)?;
        world
            .get_resource_or_insert_with(super::ClientExploders::default)
            .0
            .push(number);
        Ok(Value::Undefined)
    });
    registry.register(Function, "deactivateclientexploder", |world, _, args| {
        let number = super::int(args, 0)?;
        world
            .get_resource_or_insert_with(super::ClientExploders::default)
            .0
            .push(-number);
        Ok(Value::Undefined)
    });
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
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Value::Vector(angles) = runtime.object_field(id, "angles") {
            runtime.set_object_field(id, "angles", Value::Vector([0.0, angles[1], angles[2]]));
        }
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
    registry.register(Method, "snapshotacknowledged", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        }
        super::int(args, 0)?;
        Ok(Value::Int(1))
    });
    registry.register(Method, "launchragdoll", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        super::vector(args, 0)?;
        if args.len() > 1 {
            string(args, 1)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "isragdoll", |world, receiver, _| {
        super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "bloodimpact", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        let kind = string(args, 0)?;
        if !matches!(kind.as_str(), "normal" | "hero" | "none") {
            return Err(
                "BloodImpact() only takes \"normal\", \"hero\" or \"none\" as parameters.\n".into(),
            );
        }
        Ok(Value::Int(1))
    });
    registry.register(Method, "sendfaceevent", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "recordkillmodifier", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "dropscavengeritem", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        }
        let name = string(args, 0)?;
        if super::weapon::weapon_index(world, &name)? == 0 {
            return Err(format!(
                "Scavenger Item \"{name}\" does not exist or has not been pre-cached."
            ));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "dropitem", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        }
        let name = string(args, 0)?;
        if super::weapon::weapon_index(world, &name)? == 0 {
            return Err(format!(
                "Could not drop Item \"{name}\" does not exist or has not been pre-cached."
            ));
        }
        if args.len() > 1 {
            string(args, 1)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setexpfog", |_, _, args| {
        let start = number(args.first()).ok_or("setExpFog expects numbers")?;
        if start < 0.0 {
            return Err("setExpFog: startDist must be greater or equal to 0".into());
        }
        let halfway = number(args.get(1)).ok_or("setExpFog expects numbers")?;
        if halfway <= 0.0 {
            return Err("setExpFog: halfwayDist must be greater than 0".into());
        }
        for i in 2..6 {
            number(args.get(i)).ok_or("setExpFog expects numbers")?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "getfogsettings", |world, _, _| {
        super::super::super::arrays::new_array(world, vec![Value::Float(0.0); 17])
    });
    registry.register(Function, "setslowmotion", |_, _, args| {
        if args.is_empty() {
            return Err("SetSlowMotion requires at least 1 parameter.".into());
        }
        number(args.first()).ok_or("SetSlowMotion expects a start timescale")?;
        if args.len() > 1 {
            number(args.get(1)).ok_or("SetSlowMotion expects an end timescale")?;
        }
        if args.len() > 2 {
            number(args.get(2)).ok_or("SetSlowMotion expects a duration")?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setclientnamemode", |_, _, args| {
        let mode = string(args, 0)?;
        if !matches!(mode.as_str(), "auto_change" | "manual_change") {
            return Err("Unknown mode".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setmapcenter", |world, _, args| {
        let center = super::vector(args, 0)?;
        world.resource_mut::<RoundScript>().engine.map_center = center;
        Ok(Value::Undefined)
    });
    registry.register(Function, "positionwouldtelefrag", |world, _, args| {
        let pos = super::vector(args, 0)?;
        let mins = [pos[0] - 15.0, pos[1] - 15.0, pos[2]];
        let maxs = [pos[0] + 15.0, pos[1] + 15.0, pos[2] + 70.0];
        Ok(Value::Int(telefrags(world, mins, maxs).into()))
    });
    registry.register(Function, "boundswouldtelefrag", |world, _, args| {
        let mins = super::vector(args, 0)?;
        let maxs = super::vector(args, 1)?;
        Ok(Value::Int(telefrags(world, mins, maxs).into()))
    });
    registry.register(Function, "getreflectionlocs", |world, _, _| {
        super::super::super::arrays::new_array(world, Vec::new())
    });
    registry.register(Method, "getreflectionorigin", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let origin = match super::super::super::players::entity_field(world, id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        Ok(Value::Vector(origin))
    });
    registry.register(Function, "getlocalprofileint", |_, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(
            matches!(name.as_str(), "cg_mature" | "cg_blood").into(),
        ))
    });
    registry.register(Function, "getdvarcolorred", |world, _, args| {
        dvar_color(world, args, 0)
    });
    registry.register(Function, "getdvarcolorgreen", |world, _, args| {
        dvar_color(world, args, 1)
    });
    registry.register(Function, "getdvarcolorblue", |world, _, args| {
        dvar_color(world, args, 2)
    });
    registry.register(Function, "physicsexplosionsphere", |_, _, args| {
        super::vector(args, 0)?;
        let outer = super::int(args, 1)? & 0xffff;
        let inner = super::float(args, 2)?;
        if inner < 0.0 {
            return Err("Radius is negative".into());
        }
        if (outer as f32) < inner {
            return Err("Inner radius is outside the outer radius".into());
        }
        super::float(args, 3)?;
        if args.len() > 4 {
            super::float(args, 4)?;
        }
        if args.len() > 5 {
            super::float(args, 5)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "physicsjetthrust", |_, _, args| {
        super::vector(args, 0)?;
        super::vector(args, 1)?;
        super::int(args, 2)?;
        super::float(args, 3)?;
        super::float(args, 4)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "physicslaunch", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        if args.len() == 2 {
            super::vector(args, 0)?;
            super::vector(args, 1)?;
        }
        Ok(Value::Undefined)
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
    registry.register(Function, "addspawnpoints", |world, _, args| {
        let team = super::team(args, 0)?;
        let entries = super::super::super::arrays::array_values(
            world,
            args.get(1).ok_or("parameter 2 does not exist")?,
        )?;
        let mut points = Vec::with_capacity(entries.len());
        for entry in &entries {
            let id = super::super::engine::entity_id(world, entry)
                .map_err(|_| String::from("contents of spawnpoints array must be entities"))?;
            points.push(id);
        }
        let mut spawns = world.get_resource_or_insert_with(SpawnPoints::default);
        let list = spawns.0.entry(team).or_default();
        if list.len() + points.len() > 200 {
            return Err("Adding to many spawn points to spawnpoint system.  Max: 200\n".into());
        }
        list.extend(points);
        Ok(Value::Undefined)
    });
    registry.register(Function, "clearspawnpoints", |world, _, _| {
        world
            .get_resource_or_insert_with(SpawnPoints::default)
            .0
            .clear();
        Ok(Value::Undefined)
    });
    registry.register(Method, "placespawnpoint", |world, receiver, _| {
        super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "placespawnpoint", |world, _, args| {
        if let Some(arg) = args.first() {
            super::super::engine::entity_id(world, arg)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setspawnpointrandomvariation", |_, _, args| {
        super::float(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "predictspawnpoint", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        super::vector(args, 0)?;
        if args.len() > 1 {
            super::vector(args, 1)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "getsortedspawnpoints", |world, _, args| {
        let team = super::team(args, 0)?;
        super::team(args, 1)?;
        super::int(args, 2)?;
        if args.len() > 3 {
            let id = super::super::engine::entity_id(world, super::arg(args, 3)?)?;
            if world.resource::<RoundScript>().player_client(id).is_none() {
                return Err("getsortedspawnpoints: Entity must be a player".into());
            }
            if args.len() == 5 {
                super::int(args, 4)?;
            }
        }
        let list: Vec<Value> = world
            .get_resource::<SpawnPoints>()
            .and_then(|s| s.0.get(&team))
            .map_or_else(Vec::new, |pts| {
                pts.iter().copied().map(Value::Object).collect()
            });
        super::super::super::arrays::new_array(world, list)
    });
    registry.register(Function, "addsphereinfluencer", |world, _, args| {
        let kind = super::int(args, 0)?;
        if !(0..=7).contains(&kind) {
            return Err("USAGE: addsphereinfluencer type must be a value from 0 to 6\n".into());
        }
        super::vector(args, 1)?;
        super::float(args, 2)?;
        super::float(args, 3)?;
        super::int(args, 4)?;
        string(args, 5)?;
        if args.len() > 6 {
            let curve = super::int(args, 6)?;
            if !(0..=4).contains(&curve) {
                return Err(
                    "USAGE: addsphereinfluencer curve must be a value from 0 to 4\n".into(),
                );
            }
        }
        if args.len() > 7 {
            super::float(args, 7)?;
        }
        if args.len() > 8 {
            super::super::engine::entity_id(world, super::arg(args, 8)?)?;
        }
        let mut inf = world.get_resource_or_insert_with(Influencers::default);
        let id = inf.next;
        inf.next += 1;
        inf.live.insert(id);
        Ok(Value::Int(id))
    });
    registry.register(Function, "addcylinderinfluencer", |world, _, args| {
        let kind = super::int(args, 0)?;
        if !(0..=6).contains(&kind) {
            return Err("USAGE: addcylinderinfluencer type must be a value from 0 to 6\n".into());
        }
        super::vector(args, 1)?;
        super::vector(args, 2)?;
        super::vector(args, 3)?;
        super::float(args, 4)?;
        super::float(args, 5)?;
        super::float(args, 6)?;
        super::int(args, 7)?;
        string(args, 8)?;
        if args.len() > 9 {
            let curve = super::int(args, 9)?;
            if !(0..=4).contains(&curve) {
                return Err(
                    "USAGE: addcylinderinfluencer curve must be a value from 0 to 4\n".into(),
                );
            }
        }
        if args.len() > 10 {
            super::float(args, 10)?;
        }
        if args.len() > 11 {
            super::super::engine::entity_id(world, super::arg(args, 11)?)?;
        }
        let mut inf = world.get_resource_or_insert_with(Influencers::default);
        let id = inf.next;
        inf.next += 1;
        inf.live.insert(id);
        Ok(Value::Int(id))
    });
    registry.register(Function, "removeinfluencer", |world, _, args| {
        let id = super::int(args, 0)?;
        let removed = world
            .get_resource_or_insert_with(Influencers::default)
            .live
            .remove(&id);
        if !removed {
            return Err("Trying to remove influencer that does not exist\n".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "enableinfluencer", |world, _, args| {
        let id = super::int(args, 0)?;
        super::int(args, 1)?;
        let exists = world
            .get_resource::<Influencers>()
            .is_some_and(|inf| inf.live.contains(&id));
        if !exists {
            return Err("Trying to enable influencer that does not exist\n".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setinfluencerteammask", |world, _, args| {
        let id = super::int(args, 0)?;
        super::int(args, 1)?;
        let exists = world
            .get_resource::<Influencers>()
            .is_some_and(|inf| inf.live.contains(&id));
        if !exists {
            return Err(
                "Trying to set influencer team mask on an influencer that does not exist\n".into(),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "snapshotacknowledged", |world, _, args| {
        super::array(world, args, 0)?;
        Ok(Value::Int(1))
    });
    registry.register(Function, "getreflectionorigin", |_, _, args| {
        Ok(Value::Vector(super::vector(args, 0)?))
    });
    registry.register(Function, "announcement", |_, _, args| {
        if args.len() < 2 {
            return Err("Announcement requires at least 2 parameters".into());
        }
        super::int(args, args.len() - 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setteamhasmeat", |_, _, args| {
        super::team(args, 0)?;
        super::int(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "obituary", |world, _, args| {
        super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        if let Some(attacker @ Value::Object(_)) = args.get(1) {
            let _ = super::super::engine::entity_id(world, attacker);
        }
        string(args, 2)?;
        string(args, 3)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "addtestclient", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "skillupdate", |_, _, args| {
        super::int(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getassignedteam", |world, _, args| {
        let id = super::super::engine::entity_id(world, super::arg(args, 0)?)
            .map_err(|_| String::from("getAssignedTeam Error: param 1 is not an entity."))?;
        let team = match world.resource_mut::<RoundScript>().object_field(id, "team") {
            Value::String(team) => team,
            _ => "allies".into(),
        };
        Ok(Value::String(team))
    });
    for name in ["kick", "ban", "sethostmigrationstatus"] {
        registry.register(Function, name, |_, _, args| {
            super::int(args, 0)?;
            Ok(Value::Undefined)
        });
    }
    for name in [
        "endlobby",
        "disablezombies",
        "enablezombies",
        "starthostmigration",
        "print",
        "println",
        "print3d",
        "line",
        "box",
        "circle",
        "debugstar",
        "assert",
        "assertmsg",
        "adddebugcommand",
        "execdevgui",
        "fprintln",
        "fprintfields",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Undefined));
    }
    registry.register(Function, "openfile", |_, _, args| {
        string(args, 0)?;
        string(args, 1)?;
        Ok(Value::Int(-1))
    });
    registry.register(Function, "closefile", |_, _, args| {
        super::int(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "freadln", |_, _, args| {
        super::int(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "fgetarg", |_, _, args| {
        super::int(args, 0)?;
        super::int(args, 1)?;
        Ok(Value::string(""))
    });
}

#[derive(Resource, Default)]
struct SpawnPoints(BTreeMap<usize, Vec<u64>>);

#[derive(Resource)]
struct Influencers {
    next: i32,
    live: std::collections::BTreeSet<i32>,
}

impl Default for Influencers {
    fn default() -> Self {
        Self {
            next: 1,
            live: std::collections::BTreeSet::new(),
        }
    }
}

fn telefrags(world: &mut World, mins: [f32; 3], maxs: [f32; 3]) -> bool {
    let overlaps =
        |lo: [f32; 3], hi: [f32; 3]| (0..3).all(|i| lo[i] <= maxs[i] && hi[i] >= mins[i]);
    let frame = crate::frame::FrameWorld::from_world(world);
    for id in frame.client_ids_sorted() {
        let alive = frame
            .client_meta(id)
            .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive);
        if alive && let Some(ps) = frame.player(id) {
            let o = ps.origin;
            if overlaps(
                [o[0] - 15.0, o[1] - 15.0, o[2]],
                [o[0] + 15.0, o[1] + 15.0, o[2] + 70.0],
            ) {
                return true;
            }
        }
    }
    false
}

fn dvar_color(world: &World, args: &[Value], index: usize) -> Result<Value, String> {
    let name = string(args, 0)?.to_ascii_lowercase();
    let value = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(&name)
        .and_then(|text| text.split_whitespace().nth(index))
        .and_then(|part| part.parse::<f32>().ok())
        .map_or(0.0, |v| v.clamp(0.0, 1.0));
    Ok(Value::Float(value))
}
