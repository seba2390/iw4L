//! Black Ops' map turrets (`misc_turret`) and its turret entity methods, as
//! its handlers keep them: the spawn reads the turret from the map's keys
//! and its weapon (`game_api::turret::TurretRules`); the methods set its
//! mode, team, arcs, convergence and drop pitch and read what the turret
//! keeps. The game's own think runs every think interval and its shot fires
//! for `shootturret`, both through the match's fire world; the sentients
//! they search are kept in the game's slot order here. `useby` puts a
//! player on a turret (or takes them off it): the game's turret use clamps
//! their view and holds their stance, and while they stay on it, alive,
//! the game's think for its user aims the gun with their view and fires it.

use super::super::args::{arg, float, string};
use super::super::entities::EntityKind;
use super::engine::entity_id;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use game_api::turret::{GameTurret, TurretRules, TurretState};
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

/// What the engine keeps for a turret: the game's turret, with the
/// entities it names kept as script objects between thinks.
#[derive(Clone, Debug)]
struct Turret {
    game: GameTurret,
    /// `settargetentity`.
    manual_target: Option<u64>,
    /// The think's target, with [`HAS_TARGET`].
    target: Option<u64>,
    /// A player is on the turret (`getturretowner`).
    in_use: bool,
    /// The client on the turret, and the stance they mounted in.
    user: Option<(u32, game_api::movement::Stance)>,
    /// `setturretowner`.
    owner: Option<u64>,
}

#[derive(Resource, Clone, Default)]
pub(crate) struct Turrets(BTreeMap<u64, Turret>);

/// The game's sentient pool: the players and living actors, each in the
/// lowest slot free when it joined.
#[derive(Resource, Clone, Default)]
pub(crate) struct Sentients(Vec<Option<u64>>);

pub(crate) fn rules(world: &mut World) -> Option<TurretRules> {
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
    present(world, id)?;
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
    let index = crate::script_player::weapon_named(&frame, &weapon_name).ok();
    let weapon = index.and_then(|index| frame.content().game_weapon(index).cloned());
    let [two_barrels] =
        super::super::presence::tags_present(world, id, ["tag_flash_2"]).unwrap_or([false]);
    let spawned = (rules.spawn)(
        weapon.as_ref(),
        &weapon_name,
        &|key| keys.get(key.to_ascii_lowercase().as_str()).copied(),
        &|tag| tag == "tag_flash_2" && two_barrels,
    )?;
    let mut runtime = world.resource_mut::<RoundScript>();
    if matches!(
        runtime.object_field(id, "health"),
        Value::Undefined | Value::Int(0)
    ) {
        runtime.set_object_field(id, "health", Value::Int(spawned.default_health));
    }
    Ok(Turret {
        game: GameTurret {
            entity: runtime.entities[&id].number,
            weapon: index.unwrap_or(0),
            spawn: spawned,
            state: TurretState {
                cursor: -1,
                ..TurretState::default()
            },
        },
        manual_target: None,
        target: None,
        in_use: false,
        user: None,
        owner: None,
    })
}

/// The turret's entity drawn with its map model, posed and traced like
/// any other model entity, as the game's turret spawn sets its model.
fn present(world: &mut World, id: u64) -> Result<(), String> {
    let origin = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    };
    let presence = super::super::presence::spawn_presence(world, origin)?;
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.presence = Some(presence);
    }
    Ok(())
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
        t.game.spawn.flags = (t.game.spawn.flags & !clear) | set
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
        edit(world, receiver, |t| t.game.spawn.team = team)
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
        edit(world, receiver, |t| t.game.spawn.convergence_ms[axis] = ms)
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
        arc(w, r, a, |t, d| t.game.spawn.arcs.right = (-d).min(0.0))
    });
    registry.register(Method, "setleftarc", |w, r, a| {
        arc(w, r, a, |t, d| t.game.spawn.arcs.left = d.max(0.0))
    });
    registry.register(Method, "settoparc", |w, r, a| {
        arc(w, r, a, |t, d| t.game.spawn.arcs.top = (-d).min(0.0))
    });
    registry.register(Method, "setbottomarc", |w, r, a| {
        arc(w, r, a, |t, d| t.game.spawn.arcs.bottom = d.max(0.0))
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
            .game
            .spawn
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
            Err(unrun(world, |rules| rules.restore_drop_pitch))
        },
    );
    registry.register(Method, "shootturret", |world, receiver, _| {
        let id = turret(world, receiver, NOT_A_TURRET)?;
        if let Some(rules) = rules(world) {
            run(world, id, rules.shoot);
        }
        Ok(Value::Undefined)
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
        Ok(if turret.game.spawn.flags & HAS_TARGET != 0 {
            object(world, turret.target)
        } else {
            Value::Undefined
        })
    });
    registry.register(Method, "isfiringturret", |world, receiver, _| {
        Ok(Value::Int(
            (read(world, receiver)?.game.state.firing != 0).into(),
        ))
    });
    // Black Ops' handler for it does nothing.
    registry.register(Method, "setturretaccuracy", |_, _, _| Ok(Value::Undefined));
    // The player on the turret asks to get off; any other player gets on a
    // turret no one uses, which hears `trigger` first.
    registry.register(Method, "useby", |world, receiver, args| {
        let id = turret(world, receiver, NOT_A_TURRET)?;
        let player = entity_id(world, arg(args, 0)?)?;
        let client = world
            .resource::<RoundScript>()
            .player_client(player)
            .ok_or("not a player entity")?;
        let used_by = world.resource::<Turrets>().0[&id].user;
        if used_by.is_some_and(|(user, _)| user == client) {
            stop_using(world, id);
            return Ok(Value::Undefined);
        }
        crate::script::runtime::raise(
            world,
            Value::Object(id),
            "trigger",
            vec![Value::Object(player)],
        );
        if used_by.is_none() {
            start_using(world, id, client, player);
        }
        Ok(Value::Undefined)
    });
}

/// The game's turret use: the player takes the turret's view clamp and
/// stance, the turret takes them as its owner.
fn start_using(world: &mut World, id: u64, client: u32, player: u64) {
    let Some(rules) = rules(world) else {
        return;
    };
    let Some(user) = user_of(world, client, player) else {
        return;
    };
    let mut mount = None;
    with_turret(world, id, |turret, around| {
        mount = Some((rules.mount)(turret, &user, around));
        Vec::new()
    });
    let Some(mount) = mount else {
        return;
    };
    crate::game_move::mount_turret(
        &mut FrameWorld::from_world(world),
        crate::ClientId(client),
        Some(&mount),
    );
    if let Some(turret) = world.resource_mut::<Turrets>().0.get_mut(&id) {
        turret.in_use = true;
        turret.owner = Some(player);
        turret.user = Some((client, user.stance));
    }
    crate::script::runtime::raise(world, Value::Object(id), "turretownerchange", Vec::new());
}

/// The player gets off the turret: they take back the stance they mounted
/// in, and the turret has no owner and stops firing.
fn stop_using(world: &mut World, id: u64) {
    let Some((client, stance)) = world
        .resource_mut::<Turrets>()
        .0
        .get_mut(&id)
        .and_then(|t| {
            t.in_use = false;
            t.owner = None;
            t.game.state.manned = false;
            t.game.state.user_firing = false;
            t.game.state.fire_latched = false;
            t.game.state.fire_sound_ms = 0;
            t.user.take()
        })
    else {
        return;
    };
    let mut frame = FrameWorld::from_world(world);
    crate::game_move::mount_turret(&mut frame, crate::ClientId(client), None);
    let stance = match stance {
        game_api::movement::Stance::Prone => "prone",
        game_api::movement::Stance::Crouch => "crouch",
        game_api::movement::Stance::Stand => "stand",
    };
    crate::game_move::set_stance(&mut frame, crate::ClientId(client), stance);
    crate::script::runtime::raise(world, Value::Object(id), "turretownerchange", Vec::new());
}

/// The player on the turret, as the game's turret rules read them; `None`
/// once they are gone.
fn user_of(world: &mut World, client: u32, player: u64) -> Option<game_api::turret::TurretUser> {
    let number = world
        .resource::<RoundScript>()
        .entities
        .get(&player)?
        .number;
    crate::game_move::turret_user(
        &mut FrameWorld::from_world(world),
        crate::ClientId(client),
        number,
    )
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

/// Runs a turret rule (its think, its shot) on one turret against the
/// match, keeping what it changed; gaps it reaches are reported.
fn run(world: &mut World, id: u64, rule: game_api::turret::TurretThinkFn) {
    with_turret(world, id, |turret, around| rule(turret, around));
}

/// Runs a game rule on one turret against the match, keeping what it
/// changed; gaps it reaches are reported.
fn with_turret(
    world: &mut World,
    id: u64,
    rule: impl FnOnce(
        &mut GameTurret,
        &mut crate::game_fire::MatchFireWorld<'_, '_>,
    ) -> Vec<&'static game_api::Unknown>,
) {
    let Some(mut turret) = world
        .get_resource::<Turrets>()
        .and_then(|turrets| turrets.0.get(&id).cloned())
    else {
        return;
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let gaps = {
        let mut frame = FrameWorld::from_world(world);
        let mut around = crate::game_fire::MatchFireWorld::new(&mut frame, tick, None);
        turret.game.entity = around.number_of_object(id);
        turret.game.state.manual_target = turret.manual_target.map(|o| around.number_of_object(o));
        turret.game.state.owner = turret.owner.map(|o| around.number_of_object(o));
        let gaps = rule(&mut turret.game, &mut around);
        turret.target = turret.game.state.target.and_then(|n| around.object(n));
        gaps
    };
    let mut frame = FrameWorld::from_world(world);
    for gap in gaps {
        frame.report_game_gap(gap);
    }
    if let Some(turrets) = world.get_resource_mut::<Turrets>().as_mut()
        && let Some(kept) = turrets.0.get_mut(&id)
    {
        kept.game = turret.game;
        kept.target = turret.target;
    }
}

/// Each tick (the game's think interval): turrets whose entity is gone are
/// dropped, the sentient pool takes in and lets go, and every turret thinks
/// in entity number order.
pub(crate) fn advance(world: &mut World) {
    let Some(rules) = rules(world) else {
        return;
    };
    if world
        .get_resource::<Turrets>()
        .is_none_or(|turrets| turrets.0.is_empty())
    {
        return;
    }
    debug_assert_eq!(rules.think_ms, crate::MATCH_TICK_MS as i32);
    let gone: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        world
            .resource::<Turrets>()
            .0
            .keys()
            .filter(|id| !runtime.live(id))
            .copied()
            .collect()
    };
    for id in gone {
        world.resource_mut::<Turrets>().0.remove(&id);
    }
    sync_sentients(world, rules.sentients);
    let mut order: Vec<(i32, u64)> = {
        let runtime = world.resource::<RoundScript>();
        world
            .resource::<Turrets>()
            .0
            .keys()
            .map(|id| (runtime.entities[id].number, *id))
            .collect()
    };
    order.sort_unstable();
    for (_, id) in order {
        run(world, id, rules.think);
        let Some((client, _)) = world.resource::<Turrets>().0.get(&id).and_then(|t| t.user) else {
            continue;
        };
        let player = world.resource::<Turrets>().0[&id].owner;
        let alive = player.is_some_and(|player| world.resource::<RoundScript>().live(&player))
            && FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .is_some_and(|ps| ps.health > 0);
        match player
            .filter(|_| alive)
            .and_then(|player| user_of(world, client, player))
        {
            Some(user) => with_turret(world, id, |turret, around| {
                (rules.user_think)(turret, &user, around)
            }),
            None => stop_using(world, id),
        }
    }
}

/// The pool lets go of the sentients that left (dead actors, removed
/// entities) and gives each new one the lowest free slot, in the order they
/// joined.
fn sync_sentients(world: &mut World, size: usize) {
    let live: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        let mut live: Vec<u64> = runtime
            .entities
            .iter()
            .filter(|(id, entity)| entity.kind == EntityKind::Player && runtime.live(id))
            .map(|(id, _)| *id)
            .collect();
        live.extend(
            super::super::sightings::living_actors(world)
                .into_iter()
                .map(|actor| actor.id),
        );
        live.sort_unstable();
        live
    };
    let mut pool = world
        .remove_resource::<Sentients>()
        .unwrap_or(Sentients(Vec::new()));
    pool.0.resize(size, None);
    for slot in &mut pool.0 {
        if slot.is_some_and(|object| live.binary_search(&object).is_err()) {
            *slot = None;
        }
    }
    for object in live {
        if pool.0.contains(&Some(object)) {
            continue;
        }
        if let Some(slot) = pool.0.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(object);
        }
    }
    world.insert_resource(pool);
}

/// The sentient in a pool slot.
pub(crate) fn sentient_in(world: &World, slot: usize) -> Option<u64> {
    world
        .get_resource::<Sentients>()?
        .0
        .get(slot)
        .copied()
        .flatten()
}

/// The sentients in the pool.
pub(crate) fn sentients(world: &World) -> Vec<u64> {
    let Some(pool) = world.get_resource::<Sentients>() else {
        return Vec::new();
    };
    pool.0.iter().flatten().copied().collect()
}

/// What a turret's search reads off a sentient.
pub(crate) struct SentientView {
    pub(crate) client: Option<u32>,
    pub(crate) presence: Option<crate::ScriptModelId>,
    pub(crate) number: i32,
    pub(crate) team: Option<String>,
    pub(crate) ignore_me: bool,
    pub(crate) origin: [f32; 3],
    pub(crate) eye: [f32; 3],
    pub(crate) mins: [f32; 3],
    pub(crate) maxs: [f32; 3],
    pub(crate) pm_type: Option<i32>,
}

/// A player's eye at its view height; an actor's at its `tag_eye`, else
/// 64 units over its origin, as Black Ops' actor eye falls back.
pub(crate) fn sentient_view(world: &mut World, object: u64) -> Option<SentientView> {
    use super::super::players::entity_field;
    let field_vector = |world: &mut World, name: &str| match entity_field(world, object, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let (client, presence, number) = {
        let runtime = world.resource::<RoundScript>();
        let entity = runtime.entities.get(&object)?;
        (
            runtime.player_client(object),
            entity.presence,
            entity.number,
        )
    };
    let origin = field_vector(world, "origin");
    let team = match entity_field(world, object, "team") {
        Value::String(team) => Some(String::from_utf8_lossy(team.as_bytes()).into_owned()),
        _ => None,
    };
    let ignore_me = matches!(entity_field(world, object, "ignoreme"), Value::Int(n) if n != 0);
    let (mins, maxs) = super::super::triggers::entity_bounds(world, object);
    let (eye, pm_type) = match client {
        Some(client) => {
            let frame = FrameWorld::from_world(world);
            let ps = frame.player(crate::ClientId(client))?;
            (
                [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ],
                Some(ps.pm_type),
            )
        }
        None => (
            super::super::presence::tag_world(world, object, "tag_eye")
                .map_or([origin[0], origin[1], origin[2] + 64.0], |(at, _)| at),
            None,
        ),
    };
    Some(SentientView {
        client,
        presence,
        number,
        team,
        ignore_me,
        origin,
        eye,
        mins,
        maxs,
        pm_type,
    })
}

/// A turret's loop sound, which the snapshot carries while it is set.
pub(crate) fn set_loop_sound(world: &mut World, object: u64, alias: Option<&str>) {
    if let Some(entity) = world
        .resource_mut::<RoundScript>()
        .entities
        .get_mut(&object)
    {
        entity.loop_sound = alias.map(Into::into);
    }
}

/// A sound alias at a turret's origin.
pub(crate) fn play_sound(world: &mut World, object: u64, alias: &str) {
    let (origin, _) = turret_pose(world, object);
    if let Err(fault) = super::engine::play_sound_at(world, origin, alias) {
        diag::warn!(Sim, "turret sound {alias} not played: {fault}");
    }
}

/// An effect on a tag of a turret's model, for every client.
pub(crate) fn tag_effect(world: &mut World, object: u64, effect: &str, tag: &str) {
    if let Err(fault) = super::engine::effect_on_tag(
        world,
        effect,
        &Value::Object(object),
        tag,
        false,
        crate::EventAudience::All,
    ) {
        diag::warn!(Sim, "turret effect {effect} not played: {fault}");
    }
}

/// A camera shake from a turret's shot.
pub(crate) fn shake_camera(
    world: &mut World,
    scale: f32,
    duration_ms: i32,
    origin: [f32; 3],
    radius: f32,
) {
    super::scene_effects::shake_camera(world, scale, duration_ms, origin, radius);
}

/// A turret's origin and base angles.
pub(crate) fn turret_pose(world: &mut World, object: u64) -> ([f32; 3], [f32; 3]) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let mut vector = |name: &str| match runtime.object_field(object, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    (vector("origin"), vector("angles"))
}

/// A tag on a turret's model with its gun at these angles, posed by the
/// game's turret controller.
pub(crate) fn turret_tag(
    world: &mut World,
    object: u64,
    tag: &str,
    gun_angles: [f32; 3],
) -> Option<super::super::presence::TagPose> {
    let rules = rules(world)?;
    let controls = bone_controls(world, object, &rules, gun_angles);
    super::super::presence::tag_pose_controlled(world, object, tag, &controls).flatten()
}

/// The bone controls of a turret's gun angles, on the bones its model has.
fn bone_controls(
    world: &mut World,
    object: u64,
    rules: &TurretRules,
    gun_angles: [f32; 3],
) -> Vec<(&'static str, [f32; 3])> {
    let present = super::super::presence::tags_present(
        world,
        object,
        ["tag_aim_pitch", "tag_aim", "tag_aim_animated", "tag_flash"],
    )
    .unwrap_or([false; 4]);
    let has = |tag: &str| match tag {
        "tag_aim_pitch" => present[0],
        "tag_aim" => present[1],
        "tag_aim_animated" => present[2],
        "tag_flash" => present[3],
        _ => false,
    };
    (rules.bone_controls)(gun_angles, &has)
        .into_iter()
        .filter(|(tag, _)| has(tag))
        .collect()
}

/// Every turret's gun angles, by its entity: the game's turret controller
/// poses its model with them ([`TurretRules::bone_controls`]).
pub(crate) fn turret_guns(world: &World, only: Option<u64>) -> Vec<(u64, [f32; 3])> {
    let Some(turrets) = world.get_resource::<Turrets>() else {
        return Vec::new();
    };
    turrets
        .0
        .iter()
        .filter(|(id, _)| only.is_none_or(|only| only == **id))
        .map(|(id, turret)| (*id, turret.game.state.gun_angles))
        .collect()
}
