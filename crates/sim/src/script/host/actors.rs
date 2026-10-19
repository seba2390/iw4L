use super::entities::EntityKind;
use super::natives::engine::entity_id;
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The spawner keys an actor keeps as its own; the rest describe the spawner.
const SPAWNER_ONLY_FIELDS: [&str; 3] = ["count", "spawnflags", "export"];

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "codespawnerforcespawn", |world, receiver, _| {
        spawn_from(world, receiver, true)
    });
    // The zombie scripts' `DoSpawn` and `StalingradSpawn` are Black Ops'
    // spawner spawn and forced spawn.
    registry.register(Method, "codespawnerspawn", |world, receiver, _| {
        spawn_from(world, receiver, false)
    });
    registry.register(Method, "dospawn", |world, receiver, _| {
        spawn_from(world, receiver, false)
    });
}

/// The box a spawn point needs clear, from its origin.
const SPAWN_BOX: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
/// The box a player's sight of a spawn point is traced to, and the points
/// in it, as fractions of its size.
const SPAWN_SIGHT_BOX: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, 0.0], [15.0, 15.0, 72.0]);
const SPAWN_SIGHT_POINTS: [[f32; 3]; 11] = [
    [0.5, 0.5, 0.8],
    [0.5, 0.5, 0.5],
    [0.5, 0.5, 0.2],
    [0.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0],
];
/// What a player's sight of a spawn point stops at.
const SPAWN_SIGHT_MASK: u32 = 0x1801;
/// The fog's limit on that sight, which Black Ops never sets (see the
/// actor sight's), and the share of the distance it takes.
const SPAWN_SIGHT_FOG_SQ: f32 = f32::MAX;
const SPAWN_SIGHT_FOG_SCALE: f32 = 0.685584;

/// Why a spawn did not happen: Black Ops prints these.
fn spawn_refused(world: &mut World, spawner: u64, why: &str) -> Result<Value, String> {
    let name = spawner_name(world, spawner);
    diag::info!(Sim, "couldn't spawn from {name}: {why}");
    Ok(Value::Undefined)
}

fn spawner_name(world: &mut World, spawner: u64) -> String {
    match world
        .resource_mut::<RoundScript>()
        .object_field(spawner, "targetname")
    {
        Value::String(name) => name.to_string(),
        _ => "<unnamed>".into(),
    }
}

/// A live player or a living actor stands where the spawn would put its
/// actor.
fn spawn_point_taken(world: &mut World, origin: [f32; 3]) -> bool {
    let mins: [f32; 3] = std::array::from_fn(|i| origin[i] + SPAWN_BOX.0[i]);
    let maxs: [f32; 3] = std::array::from_fn(|i| origin[i] + SPAWN_BOX.1[i]);
    let candidates: Vec<(u64, Option<u32>)> = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(object, entity)| {
                entity.kind == EntityKind::Actor || runtime.player_client(**object).is_some()
            })
            .map(|(object, _)| (*object, runtime.player_client(*object)))
            .collect()
    };
    candidates.into_iter().any(|(object, client)| {
        let (low, high) = super::triggers::entity_bounds(world, object);
        let at = match world
            .resource_mut::<RoundScript>()
            .object_field(object, "origin")
        {
            Value::Vector(at) => at,
            _ => return false,
        };
        let overlaps = (0..3).all(|i| at[i] + low[i] <= maxs[i] && at[i] + high[i] >= mins[i]);
        if !overlaps {
            return false;
        }
        match client {
            // Black Ops asks for a player that is not dead.
            Some(client) => crate::frame::FrameWorld::from_world(world)
                .client_meta(crate::ClientId(client))
                .is_some_and(|meta| meta.lifecycle == crate::match_state::ClientLifecycle::Alive),
            None => matches!(
                world.resource_mut::<RoundScript>().object_field(object, "health"),
                Value::Int(n) if n > 0
            ),
        }
    })
}

/// An allied player sees the spawn point: a clear sight line from its eye
/// to any of the points over it.
fn spawn_point_seen(world: &mut World, spawner: u64, origin: [f32; 3]) -> bool {
    let players: Vec<u64> = world
        .resource::<RoundScript>()
        .players
        .values()
        .map(|slot| slot.object)
        .collect();
    let tick = world.resource::<crate::step::StepRequest>().tick;
    players.into_iter().any(|player| {
        let allied = matches!(
            world.resource_mut::<RoundScript>().object_field(player, "team"),
            Value::String(team) if team.as_bytes() == b"allies"
        );
        let Some(eye) = super::natives::t5_turret::sentient_view(world, player)
            .filter(|_| allied)
            .map(|view| view.eye)
        else {
            return false;
        };
        let d: [f32; 3] = std::array::from_fn(|i| eye[i] - origin[i]);
        let dist_sq = (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1];
        if dist_sq * SPAWN_SIGHT_FOG_SCALE > SPAWN_SIGHT_FOG_SQ {
            return false;
        }
        let (low, high) = SPAWN_SIGHT_BOX;
        let mut frame = crate::frame::FrameWorld::from_world(world);
        SPAWN_SIGHT_POINTS.iter().any(|frac| {
            let point: [f32; 3] =
                std::array::from_fn(|i| (high[i] - low[i]) * frac[i] + origin[i] + low[i]);
            crate::game_fire::sight_clear(
                &mut frame,
                tick,
                eye,
                point,
                [player, spawner],
                SPAWN_SIGHT_MASK,
            )
        })
    })
}

/// Spawns an actor from a map spawner: it takes the spawner's place and keys,
/// runs its aitype script, and the spawner is told `spawned`.
/// Black Ops' spawner spawn, once a frame per spawner: refused while
/// `ai_disableSpawn` is set, the spawner's count is 0 or the AI limit's
/// actors are in use; unless forced, also while a live player or a living
/// actor stands on the spawn point or an allied player sees it.
fn spawn_from(world: &mut World, spawner: &Value, force: bool) -> Result<Value, String> {
    let spawner_id = entity_id(world, spawner)?;
    let (classname, count) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let classname = runtime
            .entities
            .get(&spawner_id)
            .map(|entity| entity.classname.clone())
            .ok_or("spawner is gone")?;
        (classname, runtime.object_field(spawner_id, "count"))
    };
    let (origin, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let pose = |runtime: &mut RoundScript, name| match runtime.object_field(spawner_id, name) {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        (pose(&mut runtime, "origin"), pose(&mut runtime, "angles"))
    };
    let Some(aitype) = classname.strip_prefix("actor_") else {
        let builtin = if force {
            "CodeSpawnerForceSpawn"
        } else {
            "CodeSpawnerSpawn"
        };
        let name = spawner_name(world, spawner_id);
        return Err(format!(
            "{builtin} can only be called on actor spawners\nattempted to call {builtin} on entity with name '{name}' of type '{classname}' at ({:.0} {:.0} {:.0})\n",
            origin[0], origin[1], origin[2]
        ));
    };
    let now = super::players::now_ms(world);
    if world
        .resource::<RoundScript>()
        .spawner_times
        .get(&spawner_id)
        .is_some_and(|&at| at >= now)
    {
        return Ok(Value::Undefined);
    }
    let disabled = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ai_disableSpawn")
        .is_some_and(|value| value.parse::<f32>().is_ok_and(|n| n != 0.0));
    if disabled {
        return spawn_refused(world, spawner_id, "spawning is off (ai_disableSpawn)");
    }
    if count == Value::Int(0) {
        return spawn_refused(world, spawner_id, "its count is 0");
    }
    let (limit, in_use) = {
        let runtime = world.resource::<RoundScript>();
        let in_use = runtime
            .entities
            .values()
            .filter(|entity| entity.kind == EntityKind::Actor)
            .count();
        (runtime.ai_limit, in_use)
    };
    if limit != 0 && in_use >= limit as usize {
        return Ok(Value::Undefined);
    }
    if !force {
        if spawn_point_taken(world, origin) {
            return spawn_refused(world, spawner_id, "the spawn point would telefrag");
        }
        if spawn_point_seen(world, spawner_id, origin) {
            return spawn_refused(world, spawner_id, "a player can see the spawn point");
        }
    }
    world
        .resource_mut::<RoundScript>()
        .spawner_times
        .insert(spawner_id, now);

    let presence = super::presence::spawn_presence(world, origin)?;
    let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
    let id = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Actor, &classname)?;
        let skipped: Vec<u32> = SPAWNER_ONLY_FIELDS
            .iter()
            .map(|name| runtime.symbol(name))
            .collect();
        let inherited: Vec<(u32, Value)> = runtime
            .objects
            .get(&spawner_id)
            .into_iter()
            .flatten()
            .filter(|(symbol, _)| !skipped.contains(symbol))
            .map(|(symbol, value)| (*symbol, value.clone()))
            .collect();
        if let Some(fields) = runtime.objects.get_mut(&id) {
            fields.extend(inherited);
        }
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "code_classname", Value::string("actor"));
        // Black Ops' actor defaults.
        runtime.set_object_field(id, "movemode", Value::string("stop"));
        runtime.set_object_field(id, "allowpain", Value::Int(1));
        runtime.set_object_field(id, "allowdeath", Value::Int(0));
        for (name, value) in [
            ("goalradius", 4.0),
            ("goalheight", 80.0),
            ("walkdist", 128.0),
            ("badplaceawareness", 0.75),
            ("animtranslationscale", 1.0),
        ] {
            if runtime.object_field(id, name) == Value::Undefined {
                runtime.set_object_field(id, name, Value::Float(value));
            }
        }
        runtime.set_object_field(id, "ignoreme", Value::Int(0));
        runtime.set_object_field(id, "ignoreall", Value::Int(0));
        runtime.set_object_field(id, "safetochangescript", Value::Int(1));
        runtime.set_object_field(id, "pushable", Value::Int(1));
        runtime.set_object_field(id, "nododgemove", Value::Int(0));
        runtime.set_object_field(id, "leanamount", Value::Float(0.0));
        runtime.set_object_field(id, "damageyaw", Value::Int(0));
        runtime.set_object_field(id, "desiredangle", Value::Float(angles[1]));
        runtime.set_object_field(id, "pathgoalpos", Value::Vector(origin));
        runtime.set_object_field(id, "lookaheaddir", Value::Vector([0.0; 3]));
        runtime.set_object_field(id, "isfacingmotion", Value::Int(0));
        runtime.set_object_field(id, "lookaheaddist", Value::Float(0.0));
        if let Value::Int(n) = count {
            runtime.set_object_field(spawner_id, "count", Value::Int(n - 1));
        }
        let entity = runtime.entities.get_mut(&id).ok_or("actor is gone")?;
        entity.presence = Some(presence);
        entity.can_damage = true;
        entity.can_radius_damage = true;
        if let Some(number) = number {
            entity.number = number;
        }
        id
    };

    let now = super::players::now_ms(world);
    let main = format!("aitype/{aitype}::main");
    run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    let dog = matches!(
        world.resource_mut::<RoundScript>().object_field(id, "type"),
        Value::String(kind) if kind.as_bytes() == b"zombie_dog"
    );
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "isdog", Value::Int(dog.into()));
        runtime.set_object_field(id, "delayeddeath", Value::Int(0));
    }
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let model = runtime.object_field(id, "model");
        let entity = &runtime.entities[&id];
        diag::debug!(
            Sim,
            "actor {id}: spawned from {classname} model={model:?} attachments={:?} hidden={}",
            entity.attachments,
            entity.hidden
        );
    }
    let tree = if dog { "zombie_dog" } else { "generic_human" };
    let _ = super::actor_anims::attach(world, id, tree);
    super::actor_brain::begin(world, id)?;
    raise(world, spawner.clone(), "spawned", vec![Value::Object(id)]);
    Ok(Value::Object(id))
}

const ACTOR_DAMAGE: &str = "maps/_callbacksetup::codecallback_actordamage";
const ACTOR_KILLED: &str = "maps/_callbacksetup::codecallback_actorkilled";

/// A hit on an actor goes to the actor damage callback, which finishes it.
pub(crate) fn damage(
    world: &mut World,
    actor: u64,
    hit: &super::entity_damage::EntityHit,
    attacker: Value,
    weapon: &str,
    tag: &str,
) {
    diag::debug!(
        Sim,
        "actor {actor}: hit {} {} by {weapon} at {tag:?}",
        hit.amount,
        hit.means
    );
    let inflictor = hit
        .inflictor
        .map_or_else(|| attacker.clone(), Value::Object);
    let args = vec![
        inflictor,
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::string(weapon),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string(hit.hit_location.unwrap_or_else(|| hit_location(tag))),
        Value::Int(0),
        Value::Int(0),
    ];
    let now = super::players::now_ms(world);
    if let Err(fault) = run_now(world, ACTOR_DAMAGE, Value::Object(actor), args.clone(), now) {
        diag::warn!(Sim, "actor damage callback: {fault:?}");
        let _ = finish_damage(world, actor, &args);
    }
}

/// `FinishActorDamage(inflictor, attacker, damage, flags, means, weapon,
/// point, dir, hitLoc, modelIndex, timeOffset)`, as Black Ops' handler runs
/// it: nothing for no damage; a dead actor is only told
/// `damage_afterdeath`; a living one loses the health, is told `damage`,
/// and at no health left (held at -999 at most) is told `death` before the
/// killed callback runs and the AI dies; with health left it feels pain.
/// Each notify then goes to the face event callback.
fn finish_damage(world: &mut World, actor: u64, args: &[Value]) -> Result<Value, String> {
    let amount = match args.get(2) {
        Some(Value::Int(n)) => *n,
        Some(Value::Float(f)) => *f as i32,
        _ => return Err("damage must be a number".into()),
    };
    if amount <= 0 {
        return Ok(Value::Undefined);
    }
    let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Undefined);
    // The notify's model and tag name an attached model the hit struck (its
    // index); the actor's own body (index 0) names none. Its part is the
    // hit location.
    let model_index = match arg(9) {
        Value::Int(n) => n,
        _ => 0,
    };
    let (model_name, tag_name) = if model_index == 0 {
        (Value::string(""), Value::string(""))
    } else {
        attached_model_tag(world, actor, model_index)
    };
    let notify_args = vec![
        Value::Int(amount),
        arg(1),
        arg(7),
        arg(6),
        arg(4),
        model_name,
        tag_name,
        arg(8),
        arg(3),
    ];
    let before = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "health")
    {
        Value::Int(n) => n,
        Value::Float(f) => f as i32,
        _ => 0,
    };
    if !super::actor_brain::alive(world, actor) {
        raise(
            world,
            Value::Object(actor),
            "damage_afterdeath",
            notify_args,
        );
        face_event(world, actor, "damage_afterdeath");
        return Ok(Value::Undefined);
    }
    let after = (before - amount).max(-999);
    world
        .resource_mut::<RoundScript>()
        .set_object_field(actor, "health", Value::Int(after));
    raise(world, Value::Object(actor), "damage", notify_args);
    face_event(world, actor, "damage");
    if after < 1 {
        face_event(world, actor, "death");
        // The weapon is undefined when the name names none.
        let weapon = match arg(5) {
            Value::String(name)
                if crate::frame::FrameWorld::from_world(world)
                    .weapon_index_by_script_name(&name)
                    .is_some_and(|index| index != 0) =>
            {
                Value::String(name)
            }
            _ => Value::Undefined,
        };
        raise(
            world,
            Value::Object(actor),
            "death",
            vec![arg(1), arg(4), weapon],
        );
        let killed = vec![
            arg(0),
            arg(1),
            Value::Int(amount),
            arg(4),
            arg(5),
            arg(7),
            arg(8),
            Value::Int(0),
        ];
        let now = super::players::now_ms(world);
        if let Err(fault) = run_now(world, ACTOR_KILLED, Value::Object(actor), killed, now) {
            diag::warn!(Sim, "actor killed callback: {fault:?}");
        }
        super::actor_brain::die(world, actor, &arg(1), &arg(4), &arg(5), &arg(7), &arg(8));
    } else {
        super::actor_brain::pain(
            world,
            actor,
            &arg(1),
            amount,
            &arg(4),
            &arg(7),
            &arg(8),
            &arg(5),
        );
    }
    Ok(Value::Undefined)
}

const FACE_EVENT_NOTIFY: &str = "maps/_callbacksetup::codecallback_faceeventnotify";

/// Black Ops tells its face animation callback of an actor's notify.
fn face_event(world: &mut World, actor: u64, name: &str) {
    let now = super::players::now_ms(world);
    if let Err(fault) = run_now(
        world,
        FACE_EVENT_NOTIFY,
        Value::Undefined,
        vec![Value::string(name), Value::Object(actor)],
        now,
    ) {
        diag::warn!(Sim, "face event callback: {fault:?}");
    }
}

/// The model attached at `index` (from 1) and the tag it hangs on, as the
/// damage notify names them; none when nothing hangs there.
fn attached_model_tag(world: &World, actor: u64, index: i32) -> (Value, Value) {
    let runtime = world.resource::<RoundScript>();
    usize::try_from(index - 1)
        .ok()
        .and_then(|slot| runtime.entities.get(&actor)?.attachments.get(slot))
        .map_or_else(
            || (Value::string(""), Value::string("")),
            |attached| (Value::string(&attached.0), Value::string(&attached.1)),
        )
}

pub(crate) fn register_damage(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "finishactordamage",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            finish_damage(world, id, args)
        },
    );
}

/// The hit location a bone falls in.
fn hit_location(tag: &str) -> &'static str {
    let tag = tag.to_ascii_lowercase();
    let side = |left: &'static str, right: &'static str| {
        if tag.ends_with("_le") { left } else { right }
    };
    if tag.contains("head") || tag.contains("helmet") || tag.contains("eye") {
        "head"
    } else if tag.contains("neck") {
        "neck"
    } else if tag.contains("shoulder") || tag.contains("bicep") {
        side("left_arm_upper", "right_arm_upper")
    } else if tag.contains("elbow") {
        side("left_arm_lower", "right_arm_lower")
    } else if tag.contains("wrist") || tag.contains("hand") || tag.contains("finger") {
        side("left_hand", "right_hand")
    } else if tag.contains("hip") {
        side("left_leg_upper", "right_leg_upper")
    } else if tag.contains("knee") {
        side("left_leg_lower", "right_leg_lower")
    } else if tag.contains("ankle") || tag.contains("ball") {
        side("left_foot", "right_foot")
    } else if tag.contains("mainroot") || tag.contains("pelvis") || tag.contains("spinelower") {
        "torso_lower"
    } else {
        "torso_upper"
    }
}
