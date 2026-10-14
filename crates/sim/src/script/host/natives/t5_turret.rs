//! Black Ops' map turrets (`misc_turret`) and its turret entity methods, as
//! its handlers keep them: the spawn reads the turret from the map's keys
//! and its weapon (`game_api::turret::TurretRules`); the methods set its
//! mode, team, arcs, convergence and drop pitch and read what the turret
//! keeps. The turret's own think, and a player using it, are not run
//! (`t5.turret.think`, `t5.turret.use`).

use super::super::args::{arg, float, string};
use super::super::entities::EntityKind;
use super::engine::entity_id;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use game_api::turret::{TurretRules, TurretSpawn};
use std::collections::BTreeMap;

/// `setmode`'s bits: aims for the AI, aims by itself.
const MODE_AI: u32 = 1;
const MODE_AUTO: u32 = 2;
/// `startfiring`.
const FIRING: u32 = 4;
/// Usable by a player, until `maketurretunusable`.
const USABLE: u32 = 0x1000;
/// `setturretignoregoals`.
const IGNORE_GOALS: u32 = 0x2000;
/// `turretfiredisable`.
const FIRE_DISABLED: u32 = 0x4000;
/// The think has a target (`getturrettarget`).
const HAS_TARGET: u32 = 0x40;
/// `setturretteam`'s teams.
const TEAM_AXIS: i32 = 1;
const TEAM_ALLIES: i32 = 2;

/// What the engine keeps for a turret.
#[derive(Clone, Debug)]
struct Turret {
    spawned: TurretSpawn,
    /// `settargetentity`.
    manual_target: Option<u64>,
    /// The think's target, with [`HAS_TARGET`].
    target: Option<u64>,
    /// The think's firing state (`isfiringturret`).
    firing: bool,
    /// A player is on the turret (`getturretowner`).
    in_use: bool,
    /// `setturretowner`.
    owner: Option<u64>,
}

#[derive(Resource, Default)]
struct Turrets(BTreeMap<u64, Turret>);

fn rules(world: &mut World) -> Option<TurretRules> {
    FrameWorld::from_world(world)
        .bootstrap_ref()
        .mode
        .and_then(|mode| mode.turrets)
}

/// Spawns the level's map turrets as the game's turret spawn does, once its
/// entities are in. A turret the game refuses is reported and left a plain
/// entity.
pub(crate) fn spawn_map_turrets(world: &mut World) {
    world.insert_resource(Turrets::default());
    let Some(rules) = rules(world) else {
        return;
    };
    let ids: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Map && &*e.classname == rules.classname)
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        if world.resource::<Turrets>().0.len() == rules.max {
            diag::warn!(
                Sim,
                "G_SpawnTurret: max number of turrets ({}) exceeded",
                rules.max
            );
            return;
        }
        match spawn(world, &rules, id) {
            Ok(turret) => {
                world.resource_mut::<Turrets>().0.insert(id, turret);
            }
            Err(message) => diag::warn!(Sim, "turret spawn: {message}"),
        }
    }
}

fn spawn(world: &mut World, rules: &TurretRules, id: u64) -> Result<Turret, String> {
    let weapon_name = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "weaponinfo")
    {
        Value::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
        _ => return Err("no weaponinfo specified for turret".into()),
    };
    let mut keys = BTreeMap::new();
    for key in [
        "rightarc",
        "leftarc",
        "toparc",
        "bottomarc",
        "yawconvergencetime",
        "convergencetime",
        "pitchconvergencetime",
        "suppressiontime",
        "maxrange",
        "accuracy",
        "aispread",
        "playerspread",
    ] {
        let value = match world.resource_mut::<RoundScript>().object_field(id, key) {
            Value::String(text) => {
                super::iw4::atof(&String::from_utf8_lossy(text.as_bytes())) as f32
            }
            Value::Float(n) => n,
            Value::Int(n) => n as f32,
            _ => continue,
        };
        keys.insert(key, value);
    }
    let frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, &weapon_name)
        .ok()
        .and_then(|index| frame.content().game_weapon(index).cloned());
    let spawned = (rules.spawn)(weapon.as_ref(), &weapon_name, &|key| {
        keys.get(key.to_ascii_lowercase().as_str()).copied()
    })?;
    let mut runtime = world.resource_mut::<RoundScript>();
    if matches!(
        runtime.object_field(id, "health"),
        Value::Undefined | Value::Int(0)
    ) {
        runtime.set_object_field(id, "health", Value::Int(spawned.default_health));
    }
    Ok(Turret {
        spawned,
        manual_target: None,
        target: None,
        firing: false,
        in_use: false,
        owner: None,
    })
}

/// The turret a method runs on, with the engine's error for any other
/// entity.
fn turret(world: &World, receiver: &Value, message: &str) -> Result<u64, String> {
    let id = entity_id(world, receiver)?;
    if world
        .get_resource::<Turrets>()
        .is_some_and(|t| t.0.contains_key(&id))
    {
        return Ok(id);
    }
    Err(message.replace(
        "{classname}",
        &world.resource::<RoundScript>().entities[&id].classname,
    ))
}

const NOT_A_TURRET: &str = "entity type '{classname}' is not a turret";

fn edit(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut Turret),
) -> Result<Value, String> {
    let id = turret(world, receiver, NOT_A_TURRET)?;
    change(world.resource_mut::<Turrets>().0.get_mut(&id).unwrap());
    Ok(Value::Undefined)
}

fn read(world: &World, receiver: &Value) -> Result<Turret, String> {
    let id = turret(world, receiver, NOT_A_TURRET)?;
    Ok(world.resource::<Turrets>().0[&id].clone())
}

fn flags(world: &mut World, receiver: &Value, set: u32, clear: u32) -> Result<Value, String> {
    edit(world, receiver, |t| {
        t.spawned.flags = (t.spawned.flags & !clear) | set
    })
}

fn object(world: &World, id: Option<u64>) -> Value {
    id.filter(|id| world.resource::<RoundScript>().live(id))
        .map_or(Value::Undefined, Value::Object)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "startfiring", |w, r, _| flags(w, r, FIRING, 0));
    registry.register(Method, "stopfiring", |w, r, _| flags(w, r, 0, FIRING));
    registry.register(Method, "maketurretusable", |w, r, _| flags(w, r, USABLE, 0));
    registry.register(Method, "maketurretunusable", |w, r, _| {
        flags(w, r, 0, USABLE)
    });
    registry.register(Method, "turretfiredisable", |w, r, _| {
        flags(w, r, FIRE_DISABLED, 0)
    });
    registry.register(Method, "turretfireenable", |w, r, _| {
        flags(w, r, 0, FIRE_DISABLED)
    });
    registry.register(Method, "setturretignoregoals", |world, receiver, args| {
        let on = super::super::args::int(args, 0)? != 0;
        if on {
            flags(world, receiver, IGNORE_GOALS, 0)
        } else {
            flags(world, receiver, 0, IGNORE_GOALS)
        }
    });
    registry.register(Method, "setmode", |world, receiver, args| {
        turret(world, receiver, NOT_A_TURRET)?;
        let (set, clear) = match string(args, 0)?.as_str() {
            "auto_ai" => (MODE_AI | MODE_AUTO, 0),
            "manual" => (0, MODE_AI | MODE_AUTO),
            "manual_ai" => (MODE_AI, MODE_AUTO),
            "auto_nonai" => (MODE_AUTO, MODE_AI),
            _ => return Err("Error setting the mode of a turret.\n".into()),
        };
        flags(world, receiver, set, clear)
    });
    registry.register(Method, "setturretteam", |world, receiver, args| {
        let team = string(args, 0)?;
        turret(world, receiver, NOT_A_TURRET)?;
        let team = if team.eq_ignore_ascii_case("axis") {
            TEAM_AXIS
        } else if team.eq_ignore_ascii_case("allies") {
            TEAM_ALLIES
        } else {
            return Err(format!(
                "unknown team '{team}', should be 'axis' or 'allies'\n"
            ));
        };
        edit(world, receiver, |t| t.spawned.team = team)
    });
    // One argument sets the yaw time; a second names "yaw" or "pitch".
    registry.register(Method, "setconvergencetime", |world, receiver, args| {
        turret(world, receiver, NOT_A_TURRET)?;
        let axis = if args.len() > 1 {
            match string(args, 1)?.as_str() {
                "yaw" => 1,
                "pitch" => 0,
                _ => return Err("Convergence type should be either 'pitch' or 'yaw'".into()),
            }
        } else {
            1
        };
        let ms = (f64::from(float(args, 0)?) * 1000.0) as i32;
        edit(world, receiver, |t| t.spawned.convergence_ms[axis] = ms)
    });
    fn arc(
        world: &mut World,
        receiver: &Value,
        args: &[Value],
        set: fn(&mut Turret, f32),
    ) -> Result<Value, String> {
        let id = turret(world, receiver, "entity is not a turret")?;
        let degrees = float(args, 0)?;
        set(
            world.resource_mut::<Turrets>().0.get_mut(&id).unwrap(),
            degrees,
        );
        Ok(Value::Undefined)
    }
    registry.register(Method, "setrightarc", |w, r, a| {
        arc(w, r, a, |t, d| t.spawned.arcs.right = (-d).min(0.0))
    });
    registry.register(Method, "setleftarc", |w, r, a| {
        arc(w, r, a, |t, d| t.spawned.arcs.left = d.max(0.0))
    });
    registry.register(Method, "settoparc", |w, r, a| {
        arc(w, r, a, |t, d| t.spawned.arcs.top = (-d).min(0.0))
    });
    registry.register(Method, "setbottomarc", |w, r, a| {
        arc(w, r, a, |t, d| t.spawned.arcs.bottom = d.max(0.0))
    });
    registry.register(Method, "setdefaultdroppitch", |world, receiver, args| {
        if args.len() != 1 {
            return Err("illegal call to setdefaultdroppitch()\n".into());
        }
        let id = turret(world, receiver, "entity is not a turret")?;
        let pitch = float(args, 0)?;
        world
            .resource_mut::<Turrets>()
            .0
            .get_mut(&id)
            .unwrap()
            .spawned
            .drop_pitch = pitch;
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "restoredefaultdroppitch",
        |world, receiver, args| {
            if !args.is_empty() {
                return Err("illegal call to restoredefaultdroppitch()\n".into());
            }
            // A non-turret is left alone.
            if turret(world, receiver, "").is_err() {
                entity_id(world, receiver)?;
                return Ok(Value::Undefined);
            }
            Err(unrun(world, |rules| rules.think))
        },
    );
    registry.register(Method, "shootturret", |world, receiver, _| {
        turret(world, receiver, NOT_A_TURRET)?;
        Err(unrun(world, |rules| rules.think))
    });
    registry.register(Method, "stopshootturret", |world, receiver, _| {
        turret(world, receiver, NOT_A_TURRET)?;
        // Clears the entity's own shooting flag, which only a shot sets.
        Ok(Value::Undefined)
    });
    registry.register(Method, "settargetentity", |world, receiver, args| {
        turret(world, receiver, NOT_A_TURRET)?;
        let target = entity_id(world, arg(args, 0)?)?;
        edit(world, receiver, |t| t.manual_target = Some(target))
    });
    registry.register(Method, "cleartargetentity", |world, receiver, _| {
        edit(world, receiver, |t| t.manual_target = None)
    });
    registry.register(Method, "setturretowner", |world, receiver, args| {
        turret(world, receiver, NOT_A_TURRET)?;
        let owner = entity_id(world, arg(args, 0)?)?;
        edit(world, receiver, |t| t.owner = Some(owner))
    });
    registry.register(Method, "getturretowner", |world, receiver, _| {
        let turret = read(world, receiver)?;
        // A mounted turret with no owner answers the world.
        Ok(if turret.in_use {
            match turret.owner {
                Some(owner) => object(world, Some(owner)),
                None => object(world, world.resource::<RoundScript>().engine.world),
            }
        } else {
            Value::Undefined
        })
    });
    registry.register(Method, "getturrettarget", |world, receiver, _| {
        let turret = read(world, receiver)?;
        Ok(if turret.spawned.flags & HAS_TARGET != 0 {
            object(world, turret.target)
        } else {
            Value::Undefined
        })
    });
    registry.register(Method, "isfiringturret", |world, receiver, _| {
        Ok(Value::Int(read(world, receiver)?.firing.into()))
    });
    // Black Ops' handler for it does nothing.
    registry.register(Method, "setturretaccuracy", |_, _, _| Ok(Value::Undefined));
}

/// The error a call the game's turret rules do not run fails with.
fn unrun(world: &mut World, which: fn(&TurretRules) -> &'static game_api::Unknown) -> String {
    match rules(world) {
        Some(rules) => {
            let gap = which(&rules);
            format!("{}: {}", gap.id, gap.what)
        }
        None => "this mode has no turrets".into(),
    }
}
