//! Black Ops 2's client builtins: sound, ambience and rumble.
//!
//! The client game's sounds go to the local client's sound system as client
//! sound commands (`ClientOutput`), each named by the playback id the
//! builtin answers; `stopsound`, `soundplaying` and `setsoundvolume` name a
//! sound by it, and the sound system's reports say which still play. Its
//! loops play from their fake entity or entity (`ClientOutput::loops`).

use super::super::{float, int, string, vector};
use super::game::{ClientGame, LOCAL_CLIENT, client_entity, local_client, server_object};
use crate::script::client::{ClientOutput, LoopSource};
use crate::script::host::client_scripts::with_server;
use crate::script::host::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The engine's rumble slots end here; slot 0 is no rumble.
const RUMBLE_SLOTS: i32 = 48;

const LOCAL: crate::ClientId = crate::ClientId(LOCAL_CLIENT as u32);

/// Plays a one-shot: its playback id, or -1 for no alias.
pub(super) fn play(
    world: &mut World,
    alias: &str,
    origin: Option<[f32; 3]>,
    volume: f32,
    fade_in_ms: i32,
) -> i32 {
    if alias.is_empty() {
        return -1;
    }
    let mut output = world.resource_mut::<ClientOutput>();
    let id = output.sound_id();
    output.issued.insert(id);
    output
        .sound_commands
        .push(crate::ScriptAudioCommand::ClientSound {
            client: LOCAL,
            id,
            alias: alias.to_owned(),
            origin,
            volume,
            fade_in_ms,
        });
    id
}

/// A volume as the sound builtins clamp it: 1 or more is 1, 0 or less is 0.
fn clamp_volume(volume: f32) -> f32 {
    let mut clamped = volume;
    if 0.0 <= volume - 1.0 {
        clamped = 1.0;
    }
    if 0.0 <= -volume {
        clamped = 0.0;
    }
    clamped
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
/// alias on the world, at the point (without one it is not placed), its
/// volume clamped to 0..1 and faded in over 0 to 32 seconds; answers its
/// playback id.
fn play_sound(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let alias = string(args, 1)?;
    let at = if args.len() > 2 {
        Some(vector(args, 2)?)
    } else {
        None
    };
    let volume = if args.len() > 3 {
        clamp_volume(float(args, 3)?)
    } else {
        1.0
    };
    let fade_in_ms = if args.len() > 4 {
        // The engine keeps the milliseconds in 16 bits.
        i32::from((fade(args, 4, "playsound")? * 1000.0) as i32 as i16)
    } else {
        0
    };
    Ok(Value::Int(play(world, &alias, at, volume, fade_in_ms)))
}

/// `ent playsound(localclientnum, alias, [origin], [volume])`: the alias on
/// the entity, from the point given with exactly three parameters (else the
/// entity's origin), its volume clamped to 0..1 with exactly four; the
/// local client parameter is not read.
fn play_sound_on(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let (id, _) = receiver_entity(world, receiver)?;
    let alias = string(args, 1)?;
    let at = if args.len() == 3 {
        vector(args, 2)?
    } else {
        origin(world, id)
    };
    let volume = if args.len() == 4 {
        clamp_volume(float(args, 3)?)
    } else {
        1.0
    };
    // t6.csc.sound.client_entity: the sound plays on the entity in the
    // engine; IW4L's audio places it once, at the entity's origin.
    Ok(Value::Int(play(world, &alias, Some(at), volume, 0)))
}

/// Sets the source's looping alias (none stops it) and starts it; answers
/// its playback id, or -1 for no alias.
// t6.csc.sound.loop: a loop's fade is checked, not applied.
fn start_loop(world: &mut World, source: LoopSource, alias: &str) -> i32 {
    let mut output = world.resource_mut::<ClientOutput>();
    if alias.is_empty() {
        output.loops.remove(&source);
        return -1;
    }
    output.restart_loop(source, alias.to_owned())
}

fn stop_loop(world: &mut World, source: LoopSource) {
    world.resource_mut::<ClientOutput>().loops.remove(&source);
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
    Ok(Value::Int(start_loop(
        world,
        LoopSource::Fake(fake),
        &alias,
    )))
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
    Ok(Value::Int(start_loop(
        world,
        LoopSource::Entity(id),
        &alias,
    )))
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
    stop_loop(world, LoopSource::Fake(fake));
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
        .resource::<ClientOutput>()
        .loops
        .contains_key(&LoopSource::Entity(id));
    if !looping {
        return Ok(Value::Undefined);
    }
    if !args.is_empty() {
        fade(args, 0, "stoploopsound")?;
    }
    stop_loop(world, LoopSource::Entity(id));
    Ok(Value::Undefined)
}

/// What the sound system last reported of a playback it plays.
fn sound_status(world: &World, id: i32) -> Option<crate::ClientSoundStatus> {
    world
        .resource::<crate::script::client::ClientSoundReports>()
        .statuses
        .get(&id)
        .copied()
        .filter(|status| status.playing)
}

/// `setsoundvolume(id, volume)` / `setsoundvolumerate(id, rate)`: the
/// playback's volume, or how fast (per second) it moves there; the value
/// must be a number.
fn volume(world: &mut World, args: &[Value], builtin: &str) -> Result<Value, String> {
    let value = float(args, 1)?;
    if !value.is_finite() {
        return Err(format!("{builtin} cannot be NAN."));
    }
    let id = int(args, 0)?;
    let command = if builtin == "setsoundvolume" {
        crate::ScriptAudioCommand::ClientSoundVolume {
            client: LOCAL,
            id,
            volume: value,
        }
    } else {
        crate::ScriptAudioCommand::ClientSoundVolumeRate {
            client: LOCAL,
            id,
            rate: value,
        }
    };
    world
        .resource_mut::<ClientOutput>()
        .sound_commands
        .push(command);
    Ok(Value::Undefined)
}

/// Whether two emitter points are the same to the sound system: within
/// about 0.004 units.
fn same_point(bits: &[u32; 3], point: [f32; 3]) -> bool {
    let at = bits.map(f32::from_bits);
    let d = [at[0] - point[0], at[1] - point[1], at[2] - point[2]];
    d[1] * d[1] + d[0] * d[0] + d[2] * d[2] < 1.528_79e-5
}

/// The point emitters the sound system holds.
const POINT_EMITTERS: usize = 384;
/// The line emitters the sound system holds.
const LINE_EMITTERS: usize = 128;

/// A point emitter starts unless one of its alias already plays there, and
/// not when all are in use.
fn start_point_emitter(world: &mut World, alias: String, origin: [f32; 3]) {
    let mut output = world.resource_mut::<ClientOutput>();
    let mut points = 0;
    for source in output.loops.keys() {
        if let LoopSource::Emitter(name, at) = source {
            if *name == alias && same_point(at, origin) {
                return;
            }
            points += 1;
        }
    }
    if points >= POINT_EMITTERS {
        return;
    }
    let source = LoopSource::Emitter(alias.clone(), origin.map(f32::to_bits));
    output.restart_emitter(source, alias);
}

/// A line emitter starts for an alias the sound system has, unless all are
/// in use ("Sound ran out of line emitters").
// t6.csc.sound.line_emitter: the engine keeps a second line of the same
// alias and ends as its own emitter; IW4L keeps one.
fn start_line_emitter(world: &mut World, alias: String, start: [f32; 3], end: [f32; 3]) {
    let known = with_server(world, |server| {
        crate::frame::FrameWorld::from_world(server).script_sound_exists(&alias) == Some(true)
    })
    .unwrap_or(false);
    if !known {
        return;
    }
    let mut output = world.resource_mut::<ClientOutput>();
    let lines = output
        .loops
        .keys()
        .filter(|source| matches!(source, LoopSource::Line(..)))
        .count();
    if lines >= LINE_EMITTERS {
        diag::warn!(Sim, "csc: sound ran out of line emitters");
        return;
    }
    let source = LoopSource::Line(
        alias.clone(),
        start.map(f32::to_bits),
        end.map(f32::to_bits),
    );
    output.restart_emitter(source, alias);
}

/// A one-shot the sound system replays at a point, min to max msec apart.
struct SoundRandom {
    alias: String,
    origin: [f32; 3],
    min_ms: i32,
    max_ms: i32,
    /// When it next plays (client time).
    next_ms: i32,
}

/// The random one-shots, by when each next plays, spaced by the client's
/// shared `HoldRand` state.
#[derive(bevy_ecs::prelude::Resource, Default)]
pub(crate) struct SoundRandoms {
    scheduled: Vec<SoundRandom>,
}

impl SoundRandoms {
    /// Inserts a random at `next_ms`, after those due no later.
    fn insert(&mut self, random: SoundRandom) {
        let at = self
            .scheduled
            .iter()
            .position(|other| random.next_ms < other.next_ms)
            .unwrap_or(self.scheduled.len());
        self.scheduled.insert(at, random);
    }
}

/// The client game's time, in msec.
fn client_time(world: &World) -> i32 {
    world
        .get_resource::<ClientGame>()
        .map_or(0, |game| game.server_time)
}

/// Plays the random one-shots that are due, at their point on the world
/// (one more than 250 msec late is passed over), and schedules each again.
pub(crate) fn play_due_randoms(world: &mut World) {
    let now = client_time(world);
    let due = match world.get_resource_mut::<SoundRandoms>() {
        Some(mut randoms) => {
            let count = randoms
                .scheduled
                .iter()
                .take_while(|random| now >= random.next_ms)
                .count();
            if count == 0 {
                return;
            }
            randoms.scheduled.drain(..count).collect::<Vec<_>>()
        }
        None => return,
    };
    let mut play = Vec::new();
    for mut random in due {
        if now - random.next_ms < 250 {
            play.push((random.alias.clone(), random.origin));
        }
        random.next_ms = super::pure::irand(world, random.min_ms, random.max_ms) + now;
        world
            .get_resource_or_insert_with(SoundRandoms::default)
            .insert(random);
    }
    for (alias, origin) in play {
        self::play(world, &alias, Some(origin), 1.0, 0);
    }
}

/// A command the client sends its sound system's mixer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MixCommand {
    /// A mixer snapshot in a slot (1 the ambience's, 2 the group's).
    Snapshot {
        slot: i32,
        id: u32,
    },
    SideChainDuck {
        id: u32,
    },
    GlobalFutz {
        id: u32,
    },
    /// A reverb room at a priority (1 the level's, 2 shellshock's).
    Reverb {
        priority: i32,
        room: u32,
        dry: f32,
        wet: f32,
        fade_ms: i32,
    },
    DeactivateReverb {
        priority: i32,
        fade_ms: i32,
    },
    Context {
        kind: u32,
        value: u32,
    },
    EntityContext {
        entity: i32,
        kind: u32,
        value: u32,
    },
}

/// The mixer commands the client sent, in order.
#[derive(bevy_ecs::prelude::Resource, Default)]
pub(crate) struct MixCommands(pub(crate) Vec<MixCommand>);

// t6.audio.mix: IW4L's audio does not apply the mixer commands (snapshots,
// side-chain ducks, futz, reverb rooms, contexts); they are kept, and traced.
pub(crate) fn mix_command(world: &mut World, command: MixCommand) {
    if crate::sound_trace::enabled() {
        diag::info!(Sim, "sound trace: mix {command:?}");
    }
    world
        .get_resource_or_insert_with(MixCommands::default)
        .0
        .push(command);
}

/// The sound system's name hash: case-blind, 0 for no name.
pub(crate) fn sound_hash(name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    let hash = name.bytes().fold(0x1505u32, |hash, b| {
        u32::from(b.to_ascii_lowercase()).wrapping_add(hash.wrapping_mul(0x1003f))
    });
    hash.max(1)
}

/// The names the sound driver and banks hold; none without a sound system,
/// whose mixer commands go nowhere.
pub(crate) fn mix_names(world: &World) -> Option<asset_core::SoundMixNames> {
    with_server(world, |server| {
        crate::frame::FrameWorld::from_world(server)
            .sound_mix_names()
            .cloned()
    })
    .flatten()
}

/// A name's hash when the list holds it, else `default`'s.
fn known_or_default(list: &[u32], name: &str, what: &str) -> u32 {
    let id = sound_hash(name);
    if list.contains(&id) {
        id
    } else {
        diag::info!(Sim, "csc: SOUND: Could not find {what} {name}");
        sound_hash("default")
    }
}

fn set_snapshot(world: &mut World, slot: i32, name: &str) {
    let Some(names) = mix_names(world) else {
        return;
    };
    let name = if name.is_empty() { "default" } else { name };
    let id = known_or_default(&names.snapshots, name, "duck");
    mix_command(world, MixCommand::Snapshot { slot, id });
}

/// A context type and value the driver allows, by hash; an empty value is
/// none (0).
pub(crate) fn context(world: &World, kind: &str, value: &str) -> Option<(u32, u32)> {
    let names = mix_names(world)?;
    let (kind_id, value_id) = (sound_hash(kind), sound_hash(value));
    let Some((_, values)) = names.contexts.iter().find(|(k, _)| *k == kind_id) else {
        diag::info!(Sim, "csc: SOUND: Unknown sound context {kind}");
        return None;
    };
    if value_id != 0 && !values.contains(&value_id) {
        diag::info!(Sim, "csc: SOUND: Unknown sound context {value}");
        return None;
    }
    Some((kind_id, value_id))
}

/// A fade in seconds as whole msec, none below 1.
fn fade_ms(seconds: f32) -> i32 {
    ((seconds * 1000.0).round_ties_even() as i32).max(0)
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

/// Client footstep tables, callbacks and per-step sound/FX flags.
#[derive(bevy_ecs::prelude::Resource)]
pub(crate) struct ClientFootstepState {
    pub(crate) ai_tables: std::collections::BTreeMap<u8, String>,
    pub(crate) ai_fx_tables: std::collections::BTreeMap<u8, String>,
    pub(crate) ai_callbacks: std::collections::BTreeSet<u8>,
    pub(crate) do_sound: bool,
    pub(crate) do_fx: bool,
}

impl Default for ClientFootstepState {
    fn default() -> Self {
        Self {
            ai_tables: std::collections::BTreeMap::new(),
            ai_fx_tables: std::collections::BTreeMap::new(),
            ai_callbacks: std::collections::BTreeSet::new(),
            do_sound: true,
            do_fx: true,
        }
    }
}

const FOOTSTEP_STRINGS: [&str; 7] = [
    "sprint",
    "run",
    "walk",
    "crouch_run",
    "crouch_walk",
    "prone",
    "ladder_hand",
];

/// `ent playrumbleonentity(localclientnum, rumble)`: the rumble, by name
/// (any case) or slot among the server's precached rumbles, on the entity
/// for the local client once it has a snapshot.
pub(super) fn play_rumble(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
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
    registry.register(Function, "stopsound", |world, _, args| {
        let id = int(args, 0)?;
        world
            .resource_mut::<ClientOutput>()
            .sound_commands
            .push(crate::ScriptAudioCommand::ClientSoundStop { client: LOCAL, id });
        Ok(Value::Undefined)
    });
    // soundplaying(id): whether the sound system still plays the playback.
    registry.register(Function, "soundplaying", |world, _, args| {
        let id = int(args, 0)?;
        Ok(Value::Int(
            crate::script::client::sound_playing(world, id).into(),
        ))
    });
    // getplaybacktime(id) / getknownlength(id): how far the sound system has
    // played the playback and how long it is, in msec; 0 for a playback it
    // does not play.
    registry.register(Function, "getplaybacktime", |world, _, args| {
        let id = int(args, 0)?;
        Ok(Value::Int(
            sound_status(world, id).map_or(0, |status| status.playback_ms),
        ))
    });
    registry.register(Function, "getknownlength", |world, _, args| {
        let id = int(args, 0)?;
        Ok(Value::Int(
            sound_status(world, id).map_or(0, |status| status.length_ms),
        ))
    });
    registry.register(Function, "setsoundvolume", |world, _, args| {
        volume(world, args, "setsoundvolume")
    });
    registry.register(Function, "setsoundvolumerate", |world, _, args| {
        volume(world, args, "setsoundvolumerate")
    });
    // soundloopemitter(alias, origin) / soundstoploopemitter(alias, origin):
    // a loop the sound system plays at a point.
    registry.register(Function, "soundloopemitter", |world, _, args| {
        let origin = vector(args, 1)?;
        let alias = string(args, 0)?;
        start_point_emitter(world, alias.to_string(), origin);
        Ok(Value::Undefined)
    });
    registry.register(Function, "soundstoploopemitter", |world, _, args| {
        let origin = vector(args, 1)?;
        let alias = string(args, 0)?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .retain(|source, _| {
                !matches!(source, LoopSource::Emitter(name, at)
                if *name == *alias && same_point(at, origin))
            });
        Ok(Value::Undefined)
    });
    // soundlineemitter(alias, start, end) / soundstoplineemitter(alias,
    // start, end): a loop the sound system plays along a line.
    registry.register(Function, "soundlineemitter", |world, _, args| {
        let start = vector(args, 1)?;
        let end = vector(args, 2)?;
        let alias = string(args, 0)?;
        start_line_emitter(world, alias.to_string(), start, end);
        Ok(Value::Undefined)
    });
    registry.register(Function, "soundstoplineemitter", |world, _, args| {
        let start = vector(args, 1)?;
        let end = vector(args, 2)?;
        let alias = string(args, 0)?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .retain(|source, _| {
                !matches!(source, LoopSource::Line(name, from, to)
                if *name == *alias && same_point(from, start) && same_point(to, end))
            });
        Ok(Value::Undefined)
    });
    // setambientsnapshot(name) / setgroupsnapshot(name): the mixer snapshot
    // of the ambience (slot 1) or the group (slot 2); no name is `default`.
    registry.register(Function, "setambientsnapshot", |world, _, args| {
        let name = string(args, 0)?;
        set_snapshot(world, 1, &name);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setgroupsnapshot", |world, _, args| {
        let name = string(args, 0)?;
        set_snapshot(world, 2, &name);
        Ok(Value::Undefined)
    });
    // setambientsidechainduck(name): the ambience's side-chain duck; no
    // name is `default`.
    registry.register(Function, "setambientsidechainduck", |world, _, args| {
        let name = string(args, 0)?;
        let Some(names) = mix_names(world) else {
            return Ok(Value::Undefined);
        };
        let name = if name.is_empty() { "default" } else { &*name };
        let id = known_or_default(&names.ducks, name, "side chain duck");
        mix_command(world, MixCommand::SideChainDuck { id });
        Ok(Value::Undefined)
    });
    // setglobalfutz(name, ...): the futz patch over every sound; an unknown
    // one is `no_gfutz`. Only the name is read.
    registry.register(Function, "setglobalfutz", |world, _, args| {
        let name = string(args, 0)?;
        let Some(names) = mix_names(world) else {
            return Ok(Value::Undefined);
        };
        let mut id = sound_hash(&name);
        if !names.futzes.contains(&id) {
            diag::info!(Sim, "csc: sound: nonexistant gfutz patch {name}");
            id = sound_hash("no_gfutz");
        }
        mix_command(world, MixCommand::GlobalFutz { id });
        Ok(Value::Undefined)
    });
    // setreverb(priority, room, [dry], [wet], [fade]): reads nothing and does
    // nothing outside two to five parameters; dry 1, wet 0.5 and no fade
    // unless given.
    registry.register(Function, "setreverb", |world, _, args| {
        if !(2..=5).contains(&args.len()) {
            return Ok(Value::Undefined);
        }
        let fade = if args.len() == 5 {
            float(args, 4)?
        } else {
            0.0
        };
        let wet = if args.len() >= 4 {
            float(args, 3)?
        } else {
            0.5
        };
        let dry = if args.len() >= 3 {
            float(args, 2)?
        } else {
            1.0
        };
        let room = string(args, 1)?;
        let priority = priority(args, 0)?;
        if mix_names(world).is_some() {
            mix_command(
                world,
                MixCommand::Reverb {
                    priority,
                    room: sound_hash(&room),
                    dry,
                    wet,
                    fade_ms: fade_ms(fade),
                },
            );
        }
        Ok(Value::Undefined)
    });
    // deactivatereverb(priority, [fade]): with another count of parameters
    // it deactivates priority 0 at once.
    registry.register(Function, "deactivatereverb", |world, _, args| {
        let (priority, fade) = if (1..=2).contains(&args.len()) {
            let fade = if args.len() == 2 {
                float(args, 1)?
            } else {
                0.0
            };
            (priority(args, 0)?, fade)
        } else {
            (0, 0.0)
        };
        if mix_names(world).is_some() {
            mix_command(
                world,
                MixCommand::DeactivateReverb {
                    priority,
                    fade_ms: fade_ms(fade),
                },
            );
        }
        Ok(Value::Undefined)
    });
    // setsoundcontext(type, value) / ent setsoundentcontext(type, value):
    // the context the aliases' variants play by, globally or on the entity;
    // an unknown type, or a value its type does not allow, sets nothing.
    registry.register(Function, "setsoundcontext", |world, _, args| {
        let value = string(args, 1)?;
        let kind = string(args, 0)?;
        if let Some((kind, value)) = context(world, &kind, &value) {
            mix_command(world, MixCommand::Context { kind, value });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setsoundentcontext", |world, receiver, args| {
        let value = string(args, 1)?;
        let kind = string(args, 0)?;
        let entity = world
            .resource::<RoundScript>()
            .entity(receiver)
            .map(|(_, entity)| entity.number)
            .ok_or("not an entity")?;
        if let Some((kind, value)) = context(world, &kind, &value) {
            mix_command(
                world,
                MixCommand::EntityContext {
                    entity,
                    kind,
                    value,
                },
            );
        }
        Ok(Value::Undefined)
    });
    // allocatesoundrandoms(count): empties the random one-shots.
    registry.register(Function, "allocatesoundrandoms", |world, _, args| {
        int(args, 0)?;
        world
            .get_resource_or_insert_with(SoundRandoms::default)
            .scheduled
            .clear();
        Ok(Value::Undefined)
    });
    // createsoundrandom(origin, alias, min, max): a one-shot the sound
    // system plays at the point, min to max whole seconds apart.
    registry.register(Function, "createsoundrandom", |world, _, args| {
        let origin = vector(args, 0)?;
        let alias = string(args, 1)?;
        let min_ms = (float(args, 2)?.round_ties_even() as i32).wrapping_mul(1000);
        let max_ms = (float(args, 3)?.round_ties_even() as i32).wrapping_mul(1000);
        let now = client_time(world);
        let next_ms = super::pure::irand(world, min_ms, max_ms) + now;
        world
            .get_resource_or_insert_with(SoundRandoms::default)
            .insert(SoundRandom {
                alias: alias.to_string(),
                origin,
                min_ms,
                max_ms,
                next_ms,
            });
        Ok(Value::Undefined)
    });
    // usefootsteptable(ai_index, name): binds a footstep sound table to an
    // actor type on the client.
    registry.register(Function, "usefootsteptable", |world, _, args| {
        let index = int(args, 0)? as u8;
        let name = string(args, 1)?;
        if index != 0 {
            world
                .get_resource_or_insert_with(ClientFootstepState::default)
                .ai_tables
                .insert(index, name.to_string());
        }
        Ok(Value::Undefined)
    });
    // usefootstepfxtable(ai_index, name): binds a footstep FX table to an
    // actor type on the client.
    registry.register(Function, "usefootstepfxtable", |world, _, args| {
        let index = int(args, 0)? as u8;
        let name = string(args, 1)?;
        if index != 0 {
            world
                .get_resource_or_insert_with(ClientFootstepState::default)
                .ai_fx_tables
                .insert(index, name.to_string());
        }
        Ok(Value::Undefined)
    });
    // scriptcallbackonfootstepevent(ai_index): enables `playaifootstep` on
    // footstep notetracks for the actor type.
    registry.register(
        Function,
        "scriptcallbackonfootstepevent",
        |world, _, args| {
            let index = int(args, 0)? as u8;
            if index != 0 {
                world
                    .get_resource_or_insert_with(ClientFootstepState::default)
                    .ai_callbacks
                    .insert(index);
            }
            Ok(Value::Undefined)
        },
    );
    // getfootstepstrings(): the 7 movement gait names of a footstep table.
    registry.register(Function, "getfootstepstrings", |world, _, _| {
        crate::script::host::arrays::new_array(
            world,
            FOOTSTEP_STRINGS.iter().map(|s| Value::string(s)).collect(),
        )
    });
    // footstepdoeverything() / footstepdonothing() / footstepdosound() /
    // footstepdofootstepfx(): sets the client's footstep sound and FX flags.
    registry.register(Function, "footstepdoeverything", |world, _, _| {
        let mut state = world.get_resource_or_insert_with(ClientFootstepState::default);
        state.do_sound = true;
        state.do_fx = true;
        Ok(Value::Undefined)
    });
    registry.register(Function, "footstepdonothing", |world, _, _| {
        let mut state = world.get_resource_or_insert_with(ClientFootstepState::default);
        state.do_sound = false;
        state.do_fx = false;
        Ok(Value::Undefined)
    });
    registry.register(Function, "footstepdosound", |world, _, _| {
        world
            .get_resource_or_insert_with(ClientFootstepState::default)
            .do_sound = true;
        Ok(Value::Undefined)
    });
    registry.register(Function, "footstepdofootstepfx", |world, _, _| {
        world
            .get_resource_or_insert_with(ClientFootstepState::default)
            .do_fx = true;
        Ok(Value::Undefined)
    });
    // ent setsteptriggersound(alias) / ent clearsteptriggersound(): the
    // alias a sprint or run footstep in the trigger plays.
    registry.register(Method, "setsteptriggersound", |world, receiver, args| {
        let (_, number, _) = super::entity::receiver_entity(world, receiver)?;
        let alias = string(args, 0)?;
        let mut game = world.get_resource_or_insert_with(ClientGame::default);
        if alias.is_empty() {
            game.step_trigger_sounds.remove(&number);
        } else {
            game.step_trigger_sounds.insert(number, alias.to_string());
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearsteptriggersound", |world, receiver, _| {
        let (_, number, _) = super::entity::receiver_entity(world, receiver)?;
        if let Some(mut game) = world.get_resource_mut::<ClientGame>() {
            game.step_trigger_sounds.remove(&number);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "playrumbleonentity", play_rumble);
}
