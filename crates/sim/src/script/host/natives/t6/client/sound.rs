//! Black Ops 2's client builtins: sound, ambience and rumble.
//!
//! The client's sounds play through IW4L's sound events on the server's
//! world, heard by the local client alone. A sound the client game starts
//! gets the sound system's next playback id, which `stopsound`,
//! `soundplaying` and `setsoundvolume` name it by.

use super::super::{array_id, float, int, string, vector};
use super::game::{ClientGame, LOCAL_CLIENT, client_entity, local_client, server_object};
use crate::script::host::client_scripts::with_server;
use crate::script::host::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The world's entity number: a sound or rumble at a point.
const WORLD: i32 = 1022;
/// The engine's rumble slots end here; slot 0 is no rumble.
const RUMBLE_SLOTS: i32 = 48;

/// A client entity slot a looping sound plays on: an entity, or a fake
/// entity (`spawnfakeent`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Entity(u64),
    Fake(i32),
}

/// The client game's sounds.
#[derive(Resource, Default)]
struct ClientSounds {
    /// The last playback id the sound system gave out.
    last_id: i32,
    /// The looping alias (its hash) each slot plays.
    loops: BTreeMap<Slot, u32>,
}

/// The sound system's hash of an alias name; 0 is none.
fn alias_hash(name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    let hash = name.bytes().fold(0x1505u32, |hash, byte| {
        hash.wrapping_mul(0x1003f)
            .wrapping_add(byte.to_ascii_lowercase() as i8 as i32 as u32)
    });
    hash.max(1)
}

/// Starts a playback: the sound system's next id, or -1 for no alias.
fn start(world: &mut World, hash: u32) -> i32 {
    if hash == 0 {
        return -1;
    }
    let mut sounds = world.get_resource_or_insert_with(ClientSounds::default);
    // The engine's counter also counts the playbacks it starts itself.
    sounds.last_id = sounds.last_id.wrapping_add(1);
    sounds.last_id
}

/// The alias as a sound event on an entity number (the world's for a
/// point), heard by the local client; an alias the game lacks plays
/// nothing.
fn play(world: &World, alias: &str, number: i32, origin: [f32; 3]) -> Result<(), String> {
    with_server(world, |server| {
        {
            let mut frame = crate::frame::FrameWorld::from_world(server);
            if frame.script_sound_exists(alias) != Some(true) {
                return Ok(());
            }
            if frame.sound_alias_index(alias) == 0 {
                return Err("sound alias configstring table is full".to_owned());
            }
        }
        crate::script::host::natives::engine::sound_to(
            server,
            crate::EventAudience::Client(crate::ClientId(LOCAL_CLIENT as u32)),
            alias,
            number,
            origin,
        );
        Ok(())
    })
    .unwrap_or(Ok(()))
}

/// A method's receiving client entity: its object and, for an entity of the
/// snapshot, its entity number.
fn receiver_entity(world: &World, receiver: &Value) -> Result<(u64, Option<i32>), String> {
    let id = match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind != EntityKind::HudElem => id,
        _ => return Err("not an entity".into()),
    };
    Ok((id, client_entity(world, receiver).map(|(number, _)| number)))
}

fn origin(world: &mut World, id: u64) -> [f32; 3] {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    }
}

/// A fade in seconds as the sound builtins check it: 0 to 32.
fn fade(args: &[Value], index: usize, builtin: &str) -> Result<f32, String> {
    let seconds = float(args, index)?;
    if seconds < 0.0 || seconds > 32.0 {
        return Err(format!(
            "{builtin}: invalid fade value {:.6}. it must be between 0 and 32 seconds.",
            f64::from(seconds)
        ));
    }
    Ok(seconds)
}

/// `playsound(localclientnum, alias, [origin], [volume], [fade])`: the
/// alias at the point (the world's origin without one), its volume clamped
/// to 0..1 and its fade-in 0 to 32 seconds; answers its playback id.
fn play_sound(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let alias = string(args, 1)?;
    let at = if args.len() >= 3 {
        vector(args, 2)?
    } else {
        [0.0; 3]
    };
    if args.len() >= 4 {
        float(args, 3)?;
    }
    if args.len() >= 5 {
        fade(args, 4, "playsound")?;
    }
    // t6.csc.sound.volume: IW4L's sound events carry no volume or fade-in;
    // the sound plays at its alias's own volume.
    let id = start(world, alias_hash(&alias));
    if id != -1 {
        play(world, &alias, WORLD, at)?;
    }
    Ok(Value::Int(id))
}

/// `ent playsound(localclientnum, alias, [origin], [volume])`: the alias on
/// the entity, from the point given with exactly three parameters (else the
/// entity's origin); the local client parameter is not read.
fn play_sound_on(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let (id, snapshot) = receiver_entity(world, receiver)?;
    let alias = string(args, 1)?;
    let at = if args.len() == 3 {
        vector(args, 2)?
    } else {
        origin(world, id)
    };
    if args.len() == 4 {
        float(args, 3)?;
    }
    let playback = start(world, alias_hash(&alias));
    if playback != -1 {
        // t6.csc.sound.client_entity: on an entity only the client has, the
        // sound starts at its origin and does not follow it.
        play(world, &alias, snapshot.unwrap_or(WORLD), at)?;
    }
    Ok(Value::Int(playback))
}

/// Sets the slot's looping alias and starts it; answers its playback id.
// t6.csc.sound.loop: IW4L presents looping sounds only on the server's
// entities, so the client game's loops are not heard; the id still names
// the playback for the scripts that drive it.
fn start_loop(world: &mut World, slot: Slot, alias: &str) -> i32 {
    let hash = alias_hash(alias);
    let mut sounds = world.get_resource_or_insert_with(ClientSounds::default);
    if hash == 0 {
        sounds.loops.remove(&slot);
    } else {
        sounds.loops.insert(slot, hash);
    }
    start(world, hash)
}

fn stop_loop(world: &mut World, slot: Slot) {
    if let Some(mut sounds) = world.get_resource_mut::<ClientSounds>() {
        sounds.loops.remove(&slot);
    }
}

/// `playloopsound(localclientnum, fakeent, alias, [fade])`: the alias
/// looping on the fake entity, faded in over exactly the fourth parameter.
fn play_loop_sound(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let fake = int(args, 1)?;
    let alias = string(args, 2)?;
    if args.len() == 4 {
        fade(args, 3, "playloopsound")?;
    }
    Ok(Value::Int(start_loop(world, Slot::Fake(fake), &alias)))
}

/// `ent playloopsound(alias, [fade])`: the alias looping on the entity,
/// whose origin must be a number.
fn play_loop_sound_on(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let (id, _) = receiver_entity(world, receiver)?;
    let alias = string(args, 0)?;
    if origin(world, id).iter().any(|axis| !axis.is_finite()) {
        let number = world.resource::<RoundScript>().entities[&id].number;
        return Err(format!(
            "playloopsound: entity {number} had NaN position {alias}"
        ));
    }
    if args.len() == 2 {
        fade(args, 1, "playloopsound")?;
    }
    Ok(Value::Int(start_loop(world, Slot::Entity(id), &alias)))
}

/// `stoploopsound(localclientnum, fakeent, [fade])`: stops the fake
/// entity's loop, its fade checked in milliseconds.
fn stop_loop_sound(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let fake = int(args, 1)?;
    if args.len() == 3 {
        let ms = float(args, 2)? * 1000.0;
        if ms < 0.0 || ms > 32000.0 {
            return Err(format!(
                "stoploopsound: invalid fade value {:.6}. it must be between 0 and 32 seconds.",
                f64::from(ms)
            ));
        }
    }
    stop_loop(world, Slot::Fake(fake));
    Ok(Value::Undefined)
}

/// `ent stoploopsound([fade])`: stops the entity's loop; an entity with no
/// loop reads nothing.
fn stop_loop_sound_on(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let (id, _) = receiver_entity(world, receiver)?;
    let looping = world
        .get_resource::<ClientSounds>()
        .is_some_and(|sounds| sounds.loops.contains_key(&Slot::Entity(id)));
    if !looping {
        return Ok(Value::Undefined);
    }
    if !args.is_empty() {
        fade(args, 0, "stoploopsound")?;
    }
    stop_loop(world, Slot::Entity(id));
    Ok(Value::Undefined)
}

/// `setsoundvolume(id, volume)` and `setsoundvolumerate(id, rate)`: the
/// playback's volume and how fast it moves there, which must be a number.
fn volume(args: &[Value], builtin: &str) -> Result<Value, String> {
    if !float(args, 1)?.is_finite() {
        return Err(format!("{builtin} cannot be NAN."));
    }
    int(args, 0)?;
    // t6.csc.sound.volume: IW4L's sound events have no volume to change
    // once played (scripts ramp it every frame).
    Ok(Value::Undefined)
}

/// The reverb priority a name gives.
fn priority(args: &[Value], index: usize) -> Result<i32, String> {
    let name = string(args, index)?;
    if name.eq_ignore_ascii_case("snd_enveffectsprio_level") {
        Ok(1)
    } else if name.eq_ignore_ascii_case("snd_enveffectsprio_shellshock") {
        Ok(2)
    } else {
        Err(
            "priority must be 'snd_enveffectsprio_level' or 'snd_enveffectsprio_shellshock'\n"
                .into(),
        )
    }
}

const AMBIENT: &str = "t6.csc.sound.ambient: IW4L's audio has no ambient rooms or packages";
const REVERB: &str = "t6.csc.sound.reverb: IW4L's audio has no reverb";
const CONTEXT: &str = "t6.csc.sound.context: IW4L's audio has no sound contexts";
const RANDOMS: &str = "t6.csc.sound.randoms: IW4L's audio has no sound randoms";
const STEP_TRIGGER: &str = "t6.csc.sound.step_trigger: IW4L's client game plays no footstep sounds";

/// `ent playrumbleonentity(localclientnum, rumble)`: the rumble, by name
/// (any case) or slot among the server's precached rumbles, on the entity
/// for the local client once it has a snapshot.
fn play_rumble(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let (id, snapshot) = receiver_entity(world, receiver)?;
    local_client(args, 0)?;
    let slots: Vec<(i32, String)> = with_server(world, |server| {
        server
            .resource::<crate::script::MatchScript>()
            .precached
            .iter()
            .filter(|((kind, _), _)| *kind == "rumble")
            .map(|((_, name), &slot)| (slot, name.clone()))
            .collect()
    })
    .unwrap_or_else(Vec::new);
    let slot_name = |slot: i32| {
        slots
            .iter()
            .find(|(index, _)| *index == slot)
            .map_or_else(String::new, |(_, name)| name.clone())
    };
    let (slot, name) = match crate::script::host::args::arg(args, 1)? {
        Value::String(name) => {
            let found = (0..RUMBLE_SLOTS).find(|&slot| slot_name(slot).eq_ignore_ascii_case(name));
            (found.unwrap_or(0), name.to_string())
        }
        _ => {
            let slot = int(args, 1)?;
            (slot, slot_name(slot))
        }
    };
    if slot == 0 {
        return Err(format!("unknown rumble name '{name}'"));
    }
    // t6.csc.rumble.intermission: the client game also plays no rumble
    // during intermission, which IW4L's client game does not see.
    if !world
        .get_resource::<ClientGame>()
        .is_some_and(|game| game.has_snapshot)
    {
        return Ok(Value::Undefined);
    }
    let Some(slot) = (1..RUMBLE_SLOTS).find(|&slot| slot_name(slot) == name) else {
        return Err(format!(
            "Could not play rumble asset '{name}' because it was not registered and loaded."
        ));
    };
    let at = origin(world, id);
    with_server(world, |server| {
        if let Some(server_id) = snapshot.and_then(|number| server_object(server, number)) {
            super::super::super::super::rumble::on_entity(server, server_id, slot, false);
        } else {
            // t6.csc.sound.client_entity: at the origin of an entity only the
            // client has, not following it.
            super::super::super::super::rumble::at_position(server, at, slot);
        }
    });
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "playsound", |world, _, args| {
        play_sound(world, args)
    });
    registry.register(Method, "playsound", play_sound_on);
    registry.register(Function, "playloopsound", |world, _, args| {
        play_loop_sound(world, args)
    });
    registry.register(Method, "playloopsound", play_loop_sound_on);
    registry.register(Function, "stoploopsound", |world, _, args| {
        stop_loop_sound(world, args)
    });
    registry.register(Method, "stoploopsound", stop_loop_sound_on);
    // stopsound(id): stops the playback.
    registry.register(Function, "stopsound", |_, _, args| {
        int(args, 0)?;
        Err("t6.csc.sound.stop: IW4L's sound events cannot stop one playback".into())
    });
    // soundplaying(id): whether a voice still plays the playback.
    registry.register(Function, "soundplaying", |_, _, args| {
        int(args, 0)?;
        // t6.csc.sound.playing: IW4L does not report how long a sound plays
        // and does not present the client game's loops, so every playback
        // answers as finished (scripts poll this every frame).
        Ok(Value::Int(0))
    });
    registry.register(Function, "setsoundvolume", |_, _, args| {
        volume(args, "setsoundvolume")
    });
    registry.register(Function, "setsoundvolumerate", |_, _, args| {
        volume(args, "setsoundvolumerate")
    });
    // soundloopemitter(alias, origin) / soundstoploopemitter(alias, origin):
    // a loop the sound system plays at a point.
    for name in ["soundloopemitter", "soundstoploopemitter"] {
        registry.register(Function, name, |_, _, args| {
            vector(args, 1)?;
            string(args, 0)?;
            Err("t6.csc.sound.loop_emitter: IW4L's audio has no looping sound at a point".into())
        });
    }
    // soundlineemitter(alias, start, end) / soundstoplineemitter(alias,
    // start, end): a loop the sound system plays along a line.
    for name in ["soundlineemitter", "soundstoplineemitter"] {
        registry.register(Function, name, |_, _, args| {
            vector(args, 1)?;
            vector(args, 2)?;
            string(args, 0)?;
            Err("t6.csc.sound.line_emitter: IW4L's audio has no looping sound along a line".into())
        });
    }
    // setambientpackage(hasroom, haspackage, _, defaultroom, room,
    // triggerentnums, [triggerpriorities]): the ambient room and package the
    // local client hears; the third parameter is not read.
    registry.register(Function, "setambientpackage", |_, _, args| {
        int(args, 0)?;
        int(args, 1)?;
        int(args, 3)?;
        string(args, 4)?;
        array_id(args, 5)?;
        if args.len() >= 7 {
            array_id(args, 6)?;
        }
        Err(AMBIENT.into())
    });
    // setambientroomcontextengine(room, context, value, [flag]).
    registry.register(Function, "setambientroomcontextengine", |_, _, args| {
        string(args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        if args.len() >= 4 {
            int(args, 3)?;
        }
        Err(AMBIENT.into())
    });
    // forceambientroom(hasroom, isdefault, [room]): reads nothing and does
    // nothing with fewer than two parameters.
    registry.register(Function, "forceambientroom", |_, _, args| {
        if args.len() <= 1 {
            return Ok(Value::Undefined);
        }
        int(args, 0)?;
        int(args, 1)?;
        if args.len() > 2 {
            string(args, 2)?;
        }
        Err(AMBIENT.into())
    });
    // ent setambienttrigger(): marks the entity as an ambient room trigger.
    registry.register(Method, "setambienttrigger", |_, _, _| Err(AMBIENT.into()));
    // setambientsnapshot(name) / setgroupsnapshot(name): the mixer snapshot
    // of the ambient or the group slot.
    for name in ["setambientsnapshot", "setgroupsnapshot"] {
        registry.register(Function, name, |_, _, args| {
            string(args, 0)?;
            Err("t6.csc.sound.snapshot: IW4L's audio has no mixer snapshots".into())
        });
    }
    // setambientsidechainduck(name): the ambience's side-chain duck.
    registry.register(Function, "setambientsidechainduck", |_, _, args| {
        string(args, 0)?;
        Err("t6.csc.sound.duck: IW4L's audio has no side-chain ducks".into())
    });
    // setglobalfutz(name): the futz (distortion) over every sound.
    registry.register(Function, "setglobalfutz", |_, _, args| {
        string(args, 0)?;
        Err("t6.csc.sound.futz: IW4L's audio has no futz".into())
    });
    // setreverb(priority, room, [dry], [wet], [fade]): reads nothing and does
    // nothing outside two to five parameters.
    registry.register(Function, "setreverb", |_, _, args| {
        if !(2..=5).contains(&args.len()) {
            return Ok(Value::Undefined);
        }
        for index in (2..args.len()).rev() {
            float(args, index)?;
        }
        string(args, 1)?;
        priority(args, 0)?;
        Err(REVERB.into())
    });
    // deactivatereverb(priority, [fade]): with another count of parameters
    // it deactivates priority 0.
    registry.register(Function, "deactivatereverb", |_, _, args| {
        if (1..=2).contains(&args.len()) {
            if args.len() == 2 {
                float(args, 1)?;
            }
            priority(args, 0)?;
        }
        Err(REVERB.into())
    });
    // setsoundcontext(type, value) / ent setsoundentcontext(type, value):
    // the context the aliases' variants play by, globally or on the entity.
    registry.register(Function, "setsoundcontext", |_, _, args| {
        string(args, 1)?;
        string(args, 0)?;
        Err(CONTEXT.into())
    });
    registry.register(Method, "setsoundentcontext", |_, _, args| {
        string(args, 1)?;
        string(args, 0)?;
        Err(CONTEXT.into())
    });
    // allocatesoundrandoms(count) / createsoundrandom(origin, alias, min,
    // max): one-shots the sound system replays at a point, min to max
    // seconds apart.
    registry.register(Function, "allocatesoundrandoms", |_, _, args| {
        int(args, 0)?;
        Err(RANDOMS.into())
    });
    registry.register(Function, "createsoundrandom", |_, _, args| {
        vector(args, 0)?;
        string(args, 1)?;
        float(args, 2)?;
        float(args, 3)?;
        Err(RANDOMS.into())
    });
    // usefootsteptable(ai_index, name): binds a footstep sound table to an
    // actor type on the client.
    registry.register(Function, "usefootsteptable", |_, _, args| {
        int(args, 0)?;
        string(args, 1)?;
        // t6.csc.sound.footsteps: IW4L's client game plays no actor footstep
        // notetracks.
        Ok(Value::Undefined)
    });
    // footstepdoeverything(): footsteps play their sounds and effects for
    // every entity.
    registry.register(Function, "footstepdoeverything", |_, _, _| {
        Err("t6.csc.sound.footsteps: IW4L's client game plays no footsteps".into())
    });
    // ent setsteptriggersound(alias) / ent clearsteptriggersound(): the
    // alias a footstep in the trigger plays.
    registry.register(Method, "setsteptriggersound", |_, _, args| {
        string(args, 0)?;
        Err(STEP_TRIGGER.into())
    });
    registry.register(Method, "clearsteptriggersound", |_, _, _| {
        Err(STEP_TRIGGER.into())
    });
    registry.register(Method, "playrumbleonentity", play_rumble);
}
