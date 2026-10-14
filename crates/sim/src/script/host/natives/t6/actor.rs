//! Black Ops 2's actors: what `spawnactor` makes of an actor spawner, the
//! engine's 32 actor slots, and the builtins that ask about actors.

use super::super::super::arrays::new_array;
use super::super::super::entities::EntityKind;
use super::super::engine::{entity_id, trace};
use super::{ActorSpecies, SPECIES, float, int, string, vector};
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const ACTOR_SLOTS: usize = 32;
/// An actor's species until its aitype names another.
const DEFAULT_SPECIES: usize = 2;
/// The hull and mask an actor is dropped to the floor with as it spawns.
const DROP_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const DROP_MAXS: [f32; 3] = [15.0, 15.0, 30.0];
const DROP_MASK: u32 = 0x0282_0011;
/// The contents the engine gives an actor as it spawns; its bullet trace
/// stops on them.
const ACTOR_CONTENTS: i32 = 0x8000;
const DROP_DEPTH: f32 = 128.0;
const START_HEALTH: i32 = 100;
const ANIMSCRIPTS: &str = "maps/mp/animscripts";
/// The stances `allowedstances` names, by bit.
const STANCES: [&str; 3] = ["stand", "crouch", "prone"];

/// An actor field's value as the engine spawns the actor.
#[derive(Clone, Copy)]
enum Field {
    Int(i32),
    Float(f32),
    Vector,
}

/// The engine's plain actor fields that scripts read, with the values its
/// actor spawn leaves in them (the rest of the actor is zeroed).
const SPAWN_FIELDS: [(&str, Field); 84] = [
    ("accuracy", Field::Float(0.2)),
    ("lookforward", Field::Vector),
    ("lookright", Field::Vector),
    ("lookup", Field::Vector),
    ("sightlatency", Field::Int(0)),
    ("ignoreclosefoliage", Field::Int(0)),
    ("interval", Field::Float(0.0)),
    ("teammovewaittime", Field::Int(0)),
    ("damagetaken", Field::Int(0)),
    ("damagedir", Field::Vector),
    ("damageyaw", Field::Int(0)),
    ("proneok", Field::Int(0)),
    ("walkdist", Field::Float(0.0)),
    ("desiredangle", Field::Float(0.0)),
    ("pacifist", Field::Int(0)),
    ("newenemyreactionpos", Field::Vector),
    ("newenemyreaction", Field::Int(0)),
    ("newenemyreactiondistsq", Field::Float(562500.0)),
    ("goodenemyonly", Field::Int(0)),
    ("ignoreexplosionevents", Field::Int(0)),
    ("ignoresuppression", Field::Int(0)),
    ("suppressionwait", Field::Int(0)),
    ("suppressionduration", Field::Int(0)),
    ("suppressionstarttime", Field::Int(0)),
    ("suppressionmeter", Field::Float(0.0)),
    ("dontavoidplayer", Field::Int(0)),
    ("grenadeawareness", Field::Float(0.0)),
    ("grenadeammo", Field::Int(0)),
    ("grenadethrowback", Field::Int(0)),
    ("highlyawareradius", Field::Float(0.0)),
    ("allowpain", Field::Int(0)),
    ("blockingpain", Field::Int(0)),
    ("allowdeath", Field::Int(0)),
    ("diequietly", Field::Int(0)),
    ("minpaindamage", Field::Int(0)),
    ("forceragdollimmediate", Field::Int(0)),
    ("delayeddeath", Field::Int(0)),
    ("providecoveringfire", Field::Int(0)),
    ("doingambush", Field::Int(0)),
    ("canflank", Field::Int(0)),
    ("iswounded", Field::Int(0)),
    ("alertlevelint", Field::Int(1)),
    ("dropweapon", Field::Int(1)),
    ("drawoncompass", Field::Int(1)),
    ("activatecrosshair", Field::Int(1)),
    ("goalradius", Field::Float(4.0)),
    ("goalheight", Field::Float(80.0)),
    ("goalpos", Field::Vector),
    ("nodeoffsetpos", Field::Vector),
    ("ignoreforfixednodesafecheck", Field::Int(0)),
    ("fixednode", Field::Int(0)),
    ("fixednodesaferadius", Field::Float(64.0)),
    ("pathrandompercent", Field::Int(0)),
    ("usechokepoints", Field::Int(0)),
    ("stopanimdistsq", Field::Float(0.0)),
    ("pathenemylookahead", Field::Float(0.0)),
    ("pathenemyfightdist", Field::Float(0.0)),
    ("meleeattackdist", Field::Float(0.0)),
    ("usecombatscriptatcover", Field::Int(0)),
    ("safetochangescript", Field::Int(1)),
    ("keepclaimednode", Field::Int(0)),
    ("keepclaimednodeifvalid", Field::Int(0)),
    ("coversearchinterval", Field::Int(-1)),
    ("nododgemove", Field::Int(0)),
    ("leanamount", Field::Float(0.0)),
    ("badplaceawareness", Field::Float(0.0)),
    ("nogrenadereturnthrow", Field::Int(0)),
    ("goodshootpos", Field::Vector),
    ("goodshootposvalid", Field::Int(0)),
    ("flashbangimmunity", Field::Int(0)),
    ("lookaheaddir", Field::Vector),
    ("lookaheaddist", Field::Float(0.0)),
    ("prevanimdelta", Field::Vector),
    ("exposedduration", Field::Int(5000)),
    ("requestarrivalnotify", Field::Int(0)),
    ("finalaccuracy", Field::Float(-6969.0)),
    ("noattackeraccuracymod", Field::Int(0)),
    ("facemotion", Field::Int(0)),
    ("relativedir", Field::Int(0)),
    ("gunblockedbywall", Field::Int(0)),
    ("fixedlinkyawonly", Field::Int(0)),
    ("weaponaccuracy", Field::Float(-6969.0)),
    ("goalangle", Field::Vector),
    ("animtranslationscale", Field::Float(0.0)),
];

/// What the engine keeps for each actor beside its entity.
#[derive(Clone, Debug)]
pub(super) struct Actor {
    /// Hull radius, then its floor and top above the origin.
    pub(super) hull: (f32, f32, f32),
    pub(super) stances: u32,
    /// Whether the free camera may lock on to the actor.
    pub(super) free_camera_lock_on: bool,
}

impl Default for Actor {
    fn default() -> Self {
        Self {
            hull: (DROP_MAXS[0], DROP_MINS[2], 48.0),
            stances: 0b111,
            free_camera_lock_on: false,
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct Actors(pub(super) BTreeMap<u64, Actor>);

fn with_actor<T>(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut Actor) -> T,
) -> Result<T, String> {
    let id = actor(world, receiver)?;
    Ok(change(
        world
            .get_resource_or_insert_with(Actors::default)
            .into_inner()
            .0
            .entry(id)
            .or_default(),
    ))
}

fn live_actors(world: &World) -> usize {
    let runtime = world.resource::<RoundScript>();
    runtime
        .entities
        .values()
        .filter(|entity| entity.kind == EntityKind::Actor)
        .count()
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// The animscripts an actor's species runs: the dog's, the zombie's, the
/// zombie dog's; the big dog has none.
fn animscript_prefix(species: usize) -> Result<&'static str, String> {
    match species {
        0 => Ok("dog_"),
        2 => Ok("zm_"),
        3 => Ok("zm_dog_"),
        _ => Err(format!("species {} has no animscripts", SPECIES[species])),
    }
}

/// Lowers a new actor onto the floor below its spawner, as far as 128 units.
fn drop_to_floor(world: &mut World, id: u64) {
    let origin = vector_field(world, id, "origin");
    let start = [origin[0], origin[1], origin[2] + 1.0];
    let end = [origin[0], origin[1], origin[2] - DROP_DEPTH];
    let hit = trace(world, start, end, DROP_MINS, DROP_MAXS, DROP_MASK);
    if hit.startsolid != 0 {
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "origin", Value::Vector(start));
        return;
    }
    let landed = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * hit.fraction);
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "origin", Value::Vector(landed));
}

fn spawn_actor(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let spawner = entity_id(world, receiver)?;
    let (classname, targetname, origin) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let classname = runtime.entities[&spawner].classname.clone();
        let targetname = match runtime.object_field(spawner, "targetname") {
            Value::String(name) => name.to_string(),
            _ => "<unnamed>".into(),
        };
        let origin = match runtime.object_field(spawner, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        (classname, targetname, origin)
    };
    let Some(aitype) = classname.strip_prefix("actor_").map(str::to_owned) else {
        return Err(format!(
            "dospawn can only be called on actor spawners\nattempted to call dospawn on entity with name '{targetname}' of type '{classname}' at ({:.0} {:.0} {:.0})\n",
            origin[0], origin[1], origin[2]
        ));
    };
    if !args.is_empty() {
        int(args, 0)?;
    }
    let name = if args.len() > 1 {
        Value::String(string(args, 1)?.into())
    } else {
        Value::Undefined
    };
    if live_actors(world) >= ACTOR_SLOTS {
        diag::info!(Sim, "spawnactor: Actor allocation failed");
        return Ok(Value::Undefined);
    }

    let presence = super::super::super::presence::spawn_presence(world, origin)?;
    let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
    let id = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Actor, &classname)?;
        for (name, field) in SPAWN_FIELDS {
            let value = match field {
                Field::Int(n) => Value::Int(n),
                Field::Float(f) => Value::Float(f),
                Field::Vector => Value::Vector([0.0; 3]),
            };
            runtime.set_object_field(id, name, value);
        }
        let fields: Vec<(u32, Value)> = runtime
            .objects
            .get(&spawner)
            .into_iter()
            .flatten()
            .map(|(symbol, value)| (*symbol, value.clone()))
            .collect();
        if let Some(own) = runtime.objects.get_mut(&id) {
            own.extend(fields);
        }
        runtime.set_object_field(id, "targetname", name);
        runtime.set_object_field(id, "health", Value::Int(START_HEALTH));
        runtime.set_object_field(id, "maxhealth", Value::Int(START_HEALTH));
        let entity = runtime.entities.get_mut(&id).ok_or("actor is gone")?;
        entity.presence = Some(presence);
        entity.can_damage = true;
        entity.can_radius_damage = true;
        entity.contents = ACTOR_CONTENTS;
        if let Some(number) = number {
            entity.number = number;
        }
        id
    };
    drop_to_floor(world, id);
    world
        .get_resource_or_insert_with(Actors::default)
        .0
        .insert(id, Actor::default());

    let now = super::super::super::players::now_ms(world);
    let main = format!("aitype/{aitype}::main");
    run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    let species = match world.resource_mut::<RoundScript>().object_field(id, "type") {
        Value::String(kind) => SPECIES
            .iter()
            .position(|known| known.as_bytes() == kind.as_bytes())
            .unwrap_or(DEFAULT_SPECIES),
        _ => DEFAULT_SPECIES,
    };
    world
        .get_resource_or_insert_with(ActorSpecies::default)
        .0
        .insert(id, species);
    // The species' read-only fields: dogs are the dog and the zombie dog.
    let mut runtime = world.resource_mut::<RoundScript>();
    let is = |yes: bool| Value::Int(yes.into());
    runtime.set_object_field(id, "isdog", is(species == 0 || species == 3));
    runtime.set_object_field(id, "isbigdog", is(species == 1));
    let init = format!("{ANIMSCRIPTS}/{}init::main", animscript_prefix(species)?);
    run_now(world, &init, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{init}: {fault:?}"))?;

    diag::debug!(
        Sim,
        "actor {id}: spawned from {classname}, species {species}"
    );
    raise(world, receiver.clone(), "spawned", vec![Value::Object(id)]);
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Value::Int(count) = runtime.object_field(spawner, "count")
        && count > 0
    {
        runtime.set_object_field(spawner, "count", Value::Int(count - 1));
    }
    Ok(Value::Object(id))
}

/// An actor method's receiver.
fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("not an actor".into()),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Method, "spawnactor", spawn_actor);
    registry.register(
        Method,
        "setfreecameralockonallowed",
        |world, receiver, args| {
            let allowed = int(args, 0)? != 0;
            with_actor(world, receiver, |actor| actor.free_camera_lock_on = allowed)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Function, "getfreeactorcount", |world, _, _| {
        Ok(Value::Int(
            ACTOR_SLOTS.saturating_sub(live_actors(world)) as i32
        ))
    });
    registry.register(Function, "isai", |world, _, args| {
        let is = args.first().is_some_and(|value| {
            matches!(
                world.resource::<RoundScript>().entity(value),
                Some((_, entity)) if entity.kind == EntityKind::Actor
            )
        });
        Ok(Value::Int(is.into()))
    });
    registry.register(Function, "getcorpsearray", |world, _, _| {
        let corpses: Vec<Value> = world
            .resource::<RoundScript>()
            .entities
            .iter()
            .filter(|(_, entity)| matches!(entity.kind, EntityKind::Corpse { .. }))
            .map(|(id, _)| Value::Object(*id))
            .collect();
        new_array(world, corpses)
    });
    // Their handler is empty.
    registry.register(Method, "setengagementmindist", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Method, "setengagementmaxdist", |_, _, _| {
        Ok(Value::Undefined)
    });
    // Which of the aitype's characters the client draws; IW4L's client takes
    // the models the character script set.
    registry.register(Method, "setcharacterindex", |world, receiver, args| {
        let id = entity_id(world, receiver).map_err(|_| String::from("not an entity"))?;
        int(args, 0)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("SetCharacterIndex called on incompatible entity type\n".into());
        }
        Ok(Value::Undefined)
    });
    // Aim assist does not exist on IW4L's client (`t6.actors.aim_assist`).
    for name in ["enableaimassist", "disableaimassist"] {
        registry.register(Method, name, |world, receiver, _| {
            let id = entity_id(world, receiver)?;
            let runtime = world.resource::<RoundScript>();
            let entity = &runtime.entities[&id];
            if entity.kind != EntityKind::Actor && entity.brush.is_none() {
                return Err(
                    "Currently only supported on entities with brush models or actors".into(),
                );
            }
            Ok(Value::Undefined)
        });
    }
    // Ownership only decides who the actor's collision lets through.
    registry.register(Method, "clearentityowner", |world, receiver, _| {
        actor(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setphysparams", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("setphysparams must be called on an AI only.".into());
        }
        let hull = (float(args, 0)?, float(args, 1)?, float(args, 2)?);
        with_actor(world, receiver, |actor| actor.hull = hull)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowedstances", |world, receiver, args| {
        actor(world, receiver)?;
        let mut stances = 0;
        for index in 0..args.len() {
            let name = string(args, index)?;
            match STANCES.iter().position(|stance| *stance == name) {
                Some(bit) => stances |= 1 << bit,
                None => return Err(format!("invalid stance '{name}' in allowedStances()\n")),
            }
        }
        if stances == 0 {
            with_actor(world, receiver, |actor| actor.stances = 0b111)?;
            return Err("no allowed stances given".into());
        }
        with_actor(world, receiver, |actor| actor.stances = stances)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isstanceallowed", |world, receiver, args| {
        actor(world, receiver)?;
        let name = string(args, 0)?;
        let Some(bit) = STANCES
            .iter()
            .position(|stance| *stance == name)
            .map(|b| 1u32 << b)
        else {
            return Err(format!("invalid stance '{name}' in isStanceAllowed()\n"));
        };
        let stances = with_actor(world, receiver, |actor| actor.stances)?;
        Ok(Value::Int((stances & bit != 0).into()))
    });
    registry.register(Method, "settalktospecies", |world, receiver, args| {
        actor(world, receiver)?;
        for i in 0..args.len() {
            string(args, i)?;
        }
        Ok(Value::Undefined)
    });
    // Moves the actor; its path is planned again from there.
    registry.register(Method, "forceteleport", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let origin = vector(args, 0)?;
        let angles = if args.len() > 1 {
            Some(vector(args, 1)?)
        } else {
            None
        };
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "origin", Value::Vector(origin));
            if let Some(angles) = angles {
                runtime.set_object_field(id, "angles", Value::Vector(angles));
            }
        }
        super::movement::replan(world, id);
        Ok(Value::Int(1))
    });
    registry.register(Method, "clearenemy", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "enemy", Value::Undefined);
        runtime.set_object_field(id, "favoriteenemy", Value::Undefined);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cleargoalvolume", |world, receiver, _| {
        actor(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setflashbanged", |world, receiver, args| {
        actor(world, receiver)?;
        if args.len() != 1 {
            if args.len() != 2 {
                return Ok(Value::Undefined);
            }
            float(args, 1)?;
        }
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setspawnerteam", |world, receiver, args| {
        let id = entity_id(world, receiver).map_err(|_| String::from("not an entity"))?;
        let is_spawner = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .is_some_and(|e| {
                matches!(e.kind, EntityKind::Map | EntityKind::Spawned)
                    && e.classname.starts_with("actor_")
            });
        if !is_spawner {
            return Err("setspawnerteam can only be applied to AI spawners".into());
        }
        let team = super::team(args, 0)?;
        let name = match team {
            1 => "axis",
            2 => "allies",
            4 => "neutral",
            _ => "none",
        };
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "team", Value::string(name));
        Ok(Value::Undefined)
    });
}
