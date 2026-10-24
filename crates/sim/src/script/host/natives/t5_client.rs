//! Black Ops' client VM natives (its client scripts, CSC), read from its
//! client function and method tables. A PC client has one local client, 0.

use super::super::args::{arg, float, int, optional, string, vector};
use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::client::present::CLIENT_ENTITY_BASE;
use crate::script::client::{
    ClientEffect, ClientEffectTable, ClientLight, ClientLights, ClientModels, ClientOutput,
    ClientSoundReports, ClientTagEffect, ClientTagEvent, ClientView, FakeEnts, LocalPlayerView,
    LoopSource, SnapshotSpeeds,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    super::t5_vision::register_client(registry);
    register_random(registry);
    register_extra_camera(registry);
    register_ui3d(registry);
    // Its entities' animation trees: `useAnimTree`, the `SetAnim` family and
    // `clearAnim` on the client's own copy of the match's trees.
    super::super::actor_anims::register(registry);
    // `playSound(local, alias, [origin], [volume])`: a sound the client
    // plays at a point (a local one without), answering its playback id.
    registry.register(Function, "playsound", |world, _, args| {
        local_index(args)?;
        let alias = string(args, 1)?;
        let origin = optional(args, 2, vector)?;
        let volume = sound_volume(args, 3)?;
        Ok(play(world, alias, origin, volume))
    });
    // `ent playSound(local, alias, [origin], [volume])`: at the entity's
    // origin unless exactly an origin follows the alias.
    registry.register(Method, "playsound", |world, receiver, args| {
        local_index(args)?;
        let id = super::engine::entity_id(world, receiver)?;
        let alias = string(args, 1)?;
        let origin = match args.get(2) {
            Some(_) if args.len() == 3 => vector(args, 2)?,
            _ => match super::super::players::entity_field(world, id, "origin") {
                Value::Vector(origin) => origin,
                _ => [0.0; 3],
            },
        };
        let volume = if args.len() == 4 {
            sound_volume(args, 3)?
        } else {
            1.0
        };
        Ok(play(world, alias, Some(origin), volume))
    });
    register_sound_ids(registry);
    register_weapon_models(registry);
    register_profile(registry);
    // `player earthQuake(scale, duration, origin, radius)` shakes the local
    // client's view; nothing is checked.
    registry.register(Method, "earthquake", |world, _, args| {
        let quake = crate::ScriptEarthquake {
            id: 0,
            scale: float(args, 0)?,
            duration_ms: super::t5::earthquake_ms(float(args, 1)?),
            origin: vector(args, 2)?,
            radius: float(args, 3)?,
            start_ms: 0,
        };
        world.resource_mut::<ClientOutput>().earthquakes.push(quake);
        Ok(Value::Undefined)
    });
    // Every local client is active; there is only client 0.
    registry.register(Function, "getlocalplayers", |world, _, _| {
        let players = locals(world).into_iter().map(Value::Object).collect();
        new_array(world, players)
    });
    registry.register(Function, "getlocalplayer", |world, _, args| {
        let local = local_index(args)?;
        Ok(locals(world)
            .get(local)
            .map_or(Value::Undefined, |&object| Value::Object(object)))
    });
    registry.register(Function, "clienthassnapshot", |world, _, args| {
        let local = local_index(args)?;
        let view = world.resource::<ClientView>();
        Ok(Value::Int(
            view.locals
                .get(local)
                .is_some_and(|client| client.has_snapshot)
                .into(),
        ))
    });
    registry.register(Function, "getmaxlocalclients", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "localclientactive", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "issplitscreen", |_, _, _| Ok(Value::Int(0)));
    // The local client's host flag: set on the listen server's client.
    registry.register(Function, "issplitscreenhost", |world, _, args| {
        let _ = int(args, 0)?;
        Ok(Value::Int(world.resource::<ClientView>().host.into()))
    });
    registry.register(Method, "issplitscreenhost", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let is_local = locals(world).contains(&id);
        Ok(Value::Int(
            (is_local && world.resource::<ClientView>().host).into(),
        ))
    });
    // The client's entities a key names (`getent(local, value, key)`).
    registry.register(Function, "getent", |world, _, args| {
        if args.len() != 3 {
            return Err("illegal call to getent()\n".into());
        }
        local_index(args)?;
        let ids = super::engine::matching_entities(world, &args[1..])?;
        super::engine::single(ids, "getent")
    });
    registry.register(Function, "getentarray", |world, _, args| {
        local_index(args)?;
        let ids = if args.len() == 1 {
            let runtime = world.resource::<RoundScript>();
            let world_ent = runtime.engine.world;
            runtime
                .entities
                .iter()
                .filter(|(id, e)| e.kind != EntityKind::HudElem && Some(**id) != world_ent)
                .map(|(id, _)| *id)
                .collect()
        } else {
            super::engine::matching_entities(world, &args[1..])?
        };
        super::engine::objects(world, ids)
    });
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Method, "getentnum", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Function, "getentnum", |world, _, args| {
        local_index(args)?;
        let (_, ent) = entity(world, super::super::args::arg(args, 1)?)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Method, "getorigin", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let origin = match super::super::players::entity_field(world, id, "origin") {
            Value::Vector(origin) => origin,
            _ => [0.0; 3],
        };
        Ok(Value::Vector(origin))
    });
    registry.register(Method, "isplayer", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int((ent.kind == EntityKind::Player).into()))
    });
    registry.register(Method, "islocalplayer", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(locals(world).contains(&id).into()))
    });
    registry.register(Method, "getlocalclientnumber", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let index = locals(world)
            .into_iter()
            .position(|local| local == id)
            .ok_or_else(|| "You can only call GetLocalClientNumber on players.".to_owned())?;
        Ok(Value::Int(index as i32))
    });
    registry.register(Method, "isspectating", |world, receiver, _| {
        super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "isai", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int((ent.kind == EntityKind::Actor).into()))
    });
    registry.register(Method, "isalive", |world, receiver, _| {
        is_alive(world, receiver)
    });
    registry.register(Function, "isalive", |world, _, args| {
        let ent = super::super::args::arg(args, 0)?;
        if !matches!(ent, Value::Object(_)) {
            return Err("IsAlive param isn't an entity.".into());
        }
        is_alive(world, ent)
    });
    registry.register(Method, "hasdobj", |world, receiver, args| {
        if args.len() != 1 {
            return Err("HasDObj: Must be passed the local client number".into());
        }
        local_index(args)?;
        let (id, is_player) = {
            let (id, ent) = entity(world, receiver)?;
            (id, ent.kind == EntityKind::Player)
        };
        let has_model = matches!(
            super::super::players::entity_field(world, id, "model"),
            Value::String(s) if !s.is_empty()
        );
        Ok(Value::Int((is_player || has_model).into()))
    });
    registry.register(Method, "clientflag", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let bit = int(args, 0)?;
        if !(0..16).contains(&bit) {
            return Err(format!("ClientFlag: Index {bit} out of range (0 - 15)\n"));
        }
        let flags = match super::super::players::entity_field(world, id, "serverflags") {
            Value::Int(flags) => flags,
            _ => 0,
        };
        Ok(Value::Int((flags >> bit) & 1))
    });
    registry.register(Method, "haseyes", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let eyes = match super::super::players::entity_field(world, id, "haseyes") {
            Value::Int(eyes) => eyes != 0,
            _ => false,
        };
        Ok(Value::Int(eyes.into()))
    });
    // The client's real time: its frames' time.
    registry.register(Function, "getrealtime", |world, _, _| {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        Ok(Value::Int(
            (u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS)) as i32,
        ))
    });
    registry.register(Function, "spawnfakeent", |world, _, args| {
        local_index(args)?;
        let mut fakes = world.get_resource_or_insert_with(FakeEnts::default);
        let handle = fakes.next;
        fakes.next += 1;
        fakes.origins.insert(handle, [0.0; 3]);
        Ok(Value::Int(handle))
    });
    registry.register(Function, "setfakeentorg", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        let origin = vector(args, 2)?;
        if let Some(at) = world
            .get_resource_or_insert_with(FakeEnts::default)
            .origins
            .get_mut(&handle)
        {
            *at = origin;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "deletefakeent", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        world
            .get_resource_or_insert_with(FakeEnts::default)
            .origins
            .remove(&handle);
        Ok(Value::Undefined)
    });
    // `spawn(local, origin, classname)`: an entity of the client's own.
    registry.register(Function, "spawn", |world, _, args| {
        local_index(args)?;
        let origin = vector(args, 1)?;
        let classname = optional(args, 2, string)?.unwrap_or_else(String::new);
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        let ent_type = if matches!(classname.as_str(), "script_model" | "script_origin") {
            "scriptmover"
        } else {
            "general"
        };
        runtime.set_object_field(id, "type", Value::string(ent_type));
        runtime.set_object_field(id, "model", Value::string(""));
        runtime.set_object_field(id, "serverflags", Value::Int(0));
        runtime.set_object_field(id, "haseyes", Value::Int(0));
        Ok(Value::Object(id))
    });
    // `playLoopSound(local, fakeent, alias, fade)`: a loop at a fake
    // entity; `stopLoopSound(local, fakeent, fade)` ends it.
    registry.register(Function, "playloopsound", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        let alias = string(args, 2)?;
        loop_fade(args, 3, "playloopsound")?;
        let id = world
            .resource_mut::<ClientOutput>()
            .start_loop(LoopSource::Fake(handle), alias);
        Ok(Value::Int(id))
    });
    registry.register(Function, "stoploopsound", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        loop_fade(args, 2, "stoploopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .remove(&LoopSource::Fake(handle));
        Ok(Value::Undefined)
    });
    // `ent playLoopSound(alias, fade)`, `ent stopLoopSound(fade)`.
    registry.register(Method, "playloopsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let alias = string(args, 0)?;
        loop_fade(args, 1, "playloopsound")?;
        let id = world
            .resource_mut::<ClientOutput>()
            .start_loop(LoopSource::Entity(id), alias);
        Ok(Value::Int(id))
    });
    // `ent isPlayingLoopSound()`: whether the entity holds a loop sound; a
    // receiver that is not an entity is printed and answers false.
    registry.register(Method, "isplayingloopsound", |world, receiver, _| {
        let Ok(id) = super::engine::entity_id(world, receiver) else {
            diag::warn!(Sim, "csc: isplayingloopsound: not an entity");
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(
            world
                .resource::<ClientOutput>()
                .loops
                .contains_key(&LoopSource::Entity(id))
                .into(),
        ))
    });
    registry.register(Method, "stoploopsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        loop_fade(args, 0, "stoploopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .remove(&LoopSource::Entity(id));
        Ok(Value::Undefined)
    });
    // `soundLoopEmitter(alias, origin)`: a loop at a point, until
    // `soundStopLoopEmitter(alias, origin)`.
    registry.register(Function, "soundloopemitter", |world, _, args| {
        let source = emitter(args, "soundloopemitter")?;
        let LoopSource::Emitter(alias, _) = &source else {
            unreachable!()
        };
        let alias = alias.clone();
        world
            .resource_mut::<ClientOutput>()
            .start_loop(source, alias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "soundstoploopemitter", |world, _, args| {
        let source = emitter(args, "soundstoploopemitter")?;
        world.resource_mut::<ClientOutput>().loops.remove(&source);
        Ok(Value::Undefined)
    });
    // `soundLineEmitter(alias, start, end)`: a loop along a segment, at its
    // point nearest the listener, until `soundStopLineEmitter` names the
    // same alias and ends (within a few thousandths of a unit). The sound
    // system holds 128.
    registry.register(Function, "soundlineemitter", |world, _, args| {
        let (alias, start, end) = line_emitter(args, "soundlineemitter")?;
        let mut output = world.resource_mut::<ClientOutput>();
        let lines = output
            .loops
            .keys()
            .filter(|source| matches!(source, LoopSource::Line(..)))
            .count();
        if lines >= LINE_EMITTERS {
            diag::warn!(Sim, "csc: ERROR: Sound ran out of line emitters");
            return Ok(Value::Undefined);
        }
        let source = LoopSource::Line(
            alias.clone(),
            start.map(f32::to_bits),
            end.map(f32::to_bits),
        );
        output.start_loop(source, alias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "soundstoplineemitter", |world, _, args| {
        let (alias, start, end) = line_emitter(args, "soundstoplineemitter")?;
        let near = |a: [u32; 3], b: [f32; 3]| {
            let a = a.map(f32::from_bits);
            (a[2] - b[2]) * (a[2] - b[2])
                + (a[1] - b[1]) * (a[1] - b[1])
                + (a[0] - b[0]) * (a[0] - b[0])
                < LINE_EMITTER_MATCH_SQ
        };
        world
            .resource_mut::<ClientOutput>()
            .loops
            .retain(|source, _| {
                !matches!(source, LoopSource::Line(name, from, to)
                if *name == alias && near(*from, start) && near(*to, end))
            });
        Ok(Value::Undefined)
    });
    // Whether the entity is under water: the runtime takes Black Ops'
    // players as dry (`t5.movement.water`).
    registry.register(Method, "underwater", |world, receiver, _| {
        super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(0))
    });
    register_effects(registry);
    register_player_view(registry);
    register_models(registry);
    register_lights(registry);
    // `ent delete()`: a client-only entity notifies `death` and
    // `entityshutdown`, then is freed.
    registry.register(Method, "delete", |world, receiver, _| {
        let (id, number) = {
            let (id, ent) = entity(world, receiver)?;
            (id, ent.number)
        };
        if number < CLIENT_ENTITY_BASE {
            return Err("delete: the client can only delete its own entities".into());
        }
        crate::script::runtime::raise(world, Value::Object(id), "death", Vec::new());
        crate::script::runtime::raise(world, Value::Object(id), "entityshutdown", Vec::new());
        let mut output = world.resource_mut::<ClientOutput>();
        output.fx_entities.remove(&id);
        output.loops.remove(&LoopSource::Entity(id));
        world.resource_mut::<RoundScript>().request_delete(id);
        Ok(Value::Undefined)
    });
    // `getSystemTime([gmt])`: the machine's clock as (hour, minute,
    // second), local time unless a non-zero argument asks for GMT.
    registry.register(Function, "getsystemtime", |_, _, args| {
        let gmt = args.len() == 1 && int(args, 0)? != 0;
        let (hour, minute, second) = if gmt {
            let now = chrono::Utc::now();
            clock(&now)
        } else {
            let now = chrono::Local::now();
            clock(&now)
        };
        Ok(Value::Vector([hour as f32, minute as f32, second as f32]))
    });
    // `setClientDvar(name, value)`: one of the local client's dvars.
    registry.register(Function, "setclientdvar", |world, _, args| {
        let name = super::player::text(arg(args, 0)?)?;
        if name.is_empty() {
            return Err("SetClientDvar: unknown dvar name".into());
        }
        let value = super::player::text(arg(args, 1)?)?;
        if value.is_empty() {
            return Err("SetClientDvar: unknown dvar value".into());
        }
        world
            .resource_mut::<ClientOutput>()
            .client_dvars
            .push((name.to_string(), value.to_string()));
        Ok(Value::Undefined)
    });
    registry.register(Function, "ispc", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "isps3", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isxenon", |_, _, _| Ok(Value::Int(0)));
}

/// Black Ops' client random: one generator for the whole client, which
/// starts from a fixed state and which its own effects and sounds also draw
/// from (`t5.csc.random`).
#[derive(bevy_ecs::prelude::Resource)]
struct ClientRandom(u32);

const CLIENT_RANDOM_START: u32 = 0x89ab_cdef;

/// The generator's next 15 bits.
fn client_random(world: &mut World) -> u32 {
    let state = &mut world
        .get_resource_or_insert_with(|| ClientRandom(CLIENT_RANDOM_START))
        .0;
    *state = state.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    *state >> 17
}

/// An integer in `min..max`: the span scaled by the 15 bits.
fn client_random_int(world: &mut World, min: i32, max: i32) -> i32 {
    let bits = i64::from(client_random(world));
    let span = i64::from(max.wrapping_sub(min));
    (((span * bits) >> 15) as i32).wrapping_add(min)
}

/// A float in `min..max`, as its x87 code computes it.
fn client_random_float(world: &mut World, min: f32, max: f32) -> f32 {
    let bits = f64::from(client_random(world));
    ((f64::from(max) - f64::from(min)) * bits * f64::from(1.0f32 / 32768.0) + f64::from(min)) as f32
}

fn register_random(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "randomint", |world, _, args| {
        let max = int(args, 0)?;
        if max < 1 {
            return Err("RandomInt parm must be positive integer.\n".into());
        }
        Ok(Value::Int(client_random_int(world, 0, max)))
    });
    registry.register(Function, "randomintrange", |world, _, args| {
        let (min, max) = (int(args, 0)?, int(args, 1)?);
        if max <= min {
            return Err("RandomIntRange range must be positive integer.\n".into());
        }
        Ok(Value::Int(client_random_int(world, min, max)))
    });
    registry.register(Function, "randomfloat", |world, _, args| {
        let max = float(args, 0)?;
        Ok(Value::Float(client_random_float(world, 0.0, max)))
    });
    registry.register(Function, "randomfloatrange", |world, _, args| {
        let (min, max) = (float(args, 0)?, float(args, 1)?);
        if max < min {
            return Err("Scr_RandomFloatRange range must be positive float.\n".into());
        }
        Ok(Value::Float(client_random_float(world, min, max)))
    });
}

fn clock(time: &impl chrono::Timelike) -> (u32, u32, u32) {
    (time.hour(), time.minute(), time.second())
}

/// The client's effect slots: its effect configstrings 1 to 195.
const EFFECT_IDS: std::ops::RangeInclusive<i32> = 1..=0xc3;

fn register_effects(registry: &mut NativeRegistry) {
    use Namespace::Function;
    // `loadFx(name)`: the index the server registered the effect at, looked
    // up without case.
    registry.register(Function, "loadfx", |world, _, args| {
        let name = string(args, 0)?;
        if name
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fx/"))
        {
            return Err("effect name should start after the 'fx' folder.".into());
        }
        let table = world.resource::<ClientEffectTable>();
        match table.indices.get(&name.to_ascii_lowercase()) {
            Some(&index) => Ok(Value::Int(index)),
            None => Err(format!(
                "loadedfx could not find effect {name} - ensure that effect is precached in the server script, and included in appropriate CSVs.\n"
            )),
        }
    });
    // `playFx(local, id, origin, [forward], [up], [begin])`: an effect at a
    // point, answering its handle. Its optional last argument, in seconds,
    // is the time it begins at, not a delay from now.
    registry.register(Function, "playfx", |world, _, args| {
        if !(3..=6).contains(&args.len()) {
            return Err("Incorrect number of parameters".into());
        }
        local_index(args)?;
        let now = now_ms(world);
        let id = effect_id(args, 1)?;
        let origin = vector(args, 2)?;
        let (axis, begin) = match args.len() {
            3 => (math_iw4::angles_to_axis(DEFAULT_EFFECT_ANGLES), now),
            4 => {
                let forward = direction(world, args, 3, id, "forward")?;
                let angles = math_iw4::vect_to_angles(forward).map(math_iw4::angle_normalize_360);
                (math_iw4::angles_to_axis(angles), now)
            }
            _ => {
                let forward = direction(world, args, 3, id, "forward")?;
                let up = direction(world, args, 4, id, "up")?;
                let begin = if args.len() > 5 {
                    seconds_to_msec(float(args, 5)?)
                } else {
                    now
                };
                (forward_up_axis(forward, up)?, begin)
            }
        };
        Ok(Value::Int(play_effect(world, id, origin, axis, begin)))
    });
    // `playFxOnTag(local, id, entity, tag)`: an effect bolted to the
    // entity's tag, answering its handle.
    registry.register(Function, "playfxontag", |world, _, args| {
        if args.len() != 4 {
            return Err("Incorrect number of parameters for playfxontag".into());
        }
        local_index(args)?;
        let raw_id = int(args, 1)?;
        let (entity, _) = entity(world, arg(args, 2)?)?;
        let id = effect_id(args, 1).map(|_| raw_id)?;
        let tag = string(args, 3)?;
        let now = now_ms(world);
        let (name, life) = effect_name_life(world, id);
        let mut output = world.resource_mut::<ClientOutput>();
        let handle = output.effect_handle();
        if let Some(name) = name {
            let effect = ClientTagEffect {
                name,
                entity,
                tag,
                end_ms: now.saturating_add(life),
            };
            output.tag_events.push(ClientTagEvent {
                effect: effect.clone(),
                stop: false,
            });
            output.tag_effects.insert(handle, effect);
        }
        Ok(Value::Int(handle))
    });
    // `PlayViewmodelFX(local, id, tag)`: an effect bolted to the local
    // player's viewmodel tag, answering its handle. The effect index is not
    // checked.
    registry.register(Function, "playviewmodelfx", |world, _, args| {
        if args.len() != 3 {
            return Err("PlayViewmodelFX() called with wrong params.\n".into());
        }
        local_index(args)?;
        let id = int(args, 1)?;
        let tag = string(args, 2)?;
        if tag.is_empty() {
            return Err("PlayViewmodelFX(): unable to find viewmodel tag.\n".into());
        }
        let now = now_ms(world);
        let axis = math_iw4::angles_to_axis(DEFAULT_EFFECT_ANGLES);
        let handle = play_effect(world, id, [0.0; 3], axis, now);
        if let Some(effect) = world
            .resource_mut::<ClientOutput>()
            .effects
            .get_mut(&handle)
        {
            effect.bolt = Some(crate::EffectBolt::Viewmodel { tag });
        }
        Ok(Value::Int(handle))
    });
    // `spawnFx(local, id, origin, delay, [forward], [up])`: an effect
    // entity, which plays once `triggerFx` starts it; the delay is read
    // and unused.
    registry.register(Function, "spawnfx", |world, _, args| {
        if !(4..=6).contains(&args.len()) {
            return Err("Incorrect number of parameters".into());
        }
        local_index(args)?;
        let id = effect_id(args, 1)?;
        let origin = vector(args, 2)?;
        float(args, 3)?;
        let axis = match args.len() {
            4 => math_iw4::angles_to_axis(DEFAULT_EFFECT_ANGLES),
            5 => {
                let forward = direction(world, args, 4, id, "forward")?;
                let angles = math_iw4::vect_to_angles(forward).map(math_iw4::angle_normalize_360);
                math_iw4::angles_to_axis(angles)
            }
            _ => {
                let forward = direction(world, args, 4, id, "forward")?;
                let up = direction(world, args, 5, id, "up")?;
                forward_up_axis(forward, up)?
            }
        };
        let angles = math_iw4::axis_to_angles(axis);
        let (name, _) = effect_name_life(world, id);
        let mut runtime = world.resource_mut::<RoundScript>();
        let Ok(entity) = runtime.create_entity(EntityKind::Spawned, "") else {
            return Ok(Value::Undefined);
        };
        runtime.set_object_field(entity, "origin", Value::Vector(origin));
        runtime.set_object_field(entity, "angles", Value::Vector(angles));
        runtime.set_object_field(entity, "type", Value::string("NA"));
        runtime.set_object_field(entity, "model", Value::string(""));
        runtime.set_object_field(entity, "serverflags", Value::Int(0));
        runtime.set_object_field(entity, "haseyes", Value::Int(0));
        world.resource_mut::<ClientOutput>().fx_entities.insert(
            entity,
            ClientEffect {
                name: name.unwrap_or_else(String::new),
                origin,
                forward: axis[0],
                up: axis[2],
                begin_ms: None,
                end_ms: Some(0),
                end: crate::EffectEnd::Playing,
                bolt: None,
            },
        );
        Ok(Value::Object(entity))
    });
    // `triggerFx(entity, [begin])`: starts an effect entity now, or at the
    // time it names in seconds.
    registry.register(Function, "triggerfx", |world, _, args| {
        if args.is_empty() || args.len() >= 3 {
            return Err("Incorrect number of parameters".into());
        }
        let (entity, _) = entity(world, arg(args, 0)?)?;
        let now = now_ms(world);
        let begin = if args.len() == 2 {
            seconds_to_msec(float(args, 1)?)
        } else {
            now
        };
        let life = match world.resource::<ClientOutput>().fx_entities.get(&entity) {
            Some(effect) => world.resource::<ClientEffectTable>().life(&effect.name),
            None => return Err("entity wasn't created with 'newFx'".into()),
        };
        if let Some(effect) = world
            .resource_mut::<ClientOutput>()
            .fx_entities
            .get_mut(&entity)
        {
            effect.begin_ms = Some(begin);
            effect.end_ms = Some(begin.saturating_add(life).max(now));
        }
        Ok(Value::Undefined)
    });
    // `stopFx(local, handle)` and `deleteFx(local, handle, [immediate])`
    // kill the effect a handle names.
    registry.register(Function, "stopfx", |world, _, args| {
        if args.len() != 2 {
            return Err("Incorrect number of parameters for IsFXPlaying".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        world.resource_mut::<ClientOutput>().kill_effect(handle);
        Ok(Value::Undefined)
    });
    registry.register(Function, "deletefx", |world, _, args| {
        if !(2..=3).contains(&args.len()) {
            return Err("Incorrect number of parameters for DeleteFX".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        if args.len() > 2 {
            int(args, 2)?;
        }
        world.resource_mut::<ClientOutput>().kill_effect(handle);
        Ok(Value::Undefined)
    });
    // `isFxPlaying(local, handle)`: whether the effect is still alive.
    registry.register(Function, "isfxplaying", |world, _, args| {
        if args.len() != 2 {
            return Err("Incorrect number of parameters for IsFXPlaying".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        let now = now_ms(world);
        let alive = world
            .resource_mut::<ClientOutput>()
            .effect_alive(handle, now);
        Ok(Value::Int(alive.into()))
    });
}

/// The local client's own player state.
fn player_view(world: &World) -> &LocalPlayerView {
    &world.resource::<ClientView>().locals[0].player
}

/// A player-state builtin's one argument, the local client.
fn one_local(args: &[Value], refusal: &str) -> Result<(), String> {
    if args.len() != 1 {
        return Err(refusal.into());
    }
    local_index(args)?;
    Ok(())
}

fn register_player_view(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getlocalclienthealth", |world, _, args| {
        local_index(args)?;
        Ok(Value::Int(player_view(world).health))
    });
    registry.register(Function, "getlocalclientmaxhealth", |world, _, args| {
        local_index(args)?;
        Ok(Value::Int(player_view(world).max_health))
    });
    registry.register(Function, "getcurrentweapon", |world, _, args| {
        one_local(args, "GetCurrentWeapon() called with wrong params.\n")?;
        Ok(Value::string(&player_view(world).weapon))
    });
    // Aiming at all: the weapon has begun to raise to its sights.
    registry.register(Function, "isads", |world, _, args| {
        one_local(args, "IsADS() called with wrong params.\n")?;
        Ok(Value::Int(player_view(world).ads.into()))
    });
    registry.register(Function, "ismeleeing", |world, _, args| {
        one_local(args, "IsMeleeing() called with wrong params.\n")?;
        Ok(Value::Int(player_view(world).meleeing.into()))
    });
    registry.register(Function, "isthrowinggrenade", |world, _, args| {
        one_local(args, "IsThrowingGrenade() called with wrong params.\n")?;
        Ok(Value::Int(player_view(world).throwing_grenade.into()))
    });
    // On a turret: the runtime's Black Ops players never mount one
    // (`t5.turret.use`).
    registry.register(Function, "isonturret", |_, _, args| {
        one_local(args, "isonturret() called with wrong params.\n")?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getlocalclientpos", |world, _, args| {
        local_index(args)?;
        Ok(Value::Vector(player_view(world).origin))
    });
    registry.register(Function, "getlocalclientangles", |world, _, args| {
        local_index(args)?;
        Ok(Value::Vector(player_view(world).angles))
    });
    registry.register(Function, "getlocalclienteyepos", |world, _, args| {
        local_index(args)?;
        Ok(Value::Vector(player_view(world).eye))
    });
    // `getWeaponAmmoClip(local, weapon)`: the clip of a weapon the player
    // holds, 0 for any other.
    registry.register(Function, "getweaponammoclip", |world, _, args| {
        if args.len() != 2 {
            return Err("GetWeaponAmmoClip() called with wrong params.\n".into());
        }
        local_index(args)?;
        let weapon = string(args, 1)?;
        Ok(Value::Int(
            player_view(world).clips.get(&weapon).copied().unwrap_or(0),
        ))
    });
    // `ent getSpeed()`: the local player's velocity, any other entity's
    // move between its last two snapshots, per second.
    registry.register(Method, "getspeed", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        if locals(world).contains(&id) {
            let v = player_view(world).velocity;
            return Ok(Value::Float(
                (v[2] * v[2] + v[1] * v[1] + v[0] * v[0]).sqrt(),
            ));
        }
        Ok(Value::Float(
            world
                .get_resource::<SnapshotSpeeds>()
                .and_then(|speeds| speeds.0.get(&id).copied())
                .unwrap_or(0.0),
        ))
    });
}

fn register_models(registry: &mut NativeRegistry) {
    use Namespace::Method;
    // `ent setModel(name)`: a script mover's model, a precached one or a
    // brush model (`*N`); "" clears it.
    registry.register(Method, "setmodel", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let is_mover = matches!(
            super::super::players::entity_field(world, id, "type"),
            Value::String(kind) if &*kind == "scriptmover"
        );
        if !is_mover {
            return Err("SetModel can only be called on a script mover\n".into());
        }
        let model = string(args, 0)?;
        if !model.is_empty()
            && !model.starts_with('*')
            && !world
                .resource::<ClientModels>()
                .names
                .contains(&model.to_ascii_lowercase())
        {
            return Err(format!("model '{model}' not precached\n"));
        }
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "model", Value::string(&model));
        Ok(Value::Undefined)
    });
    // `ent isTouching(other)`: a client trigger and a player's or an
    // actor's box.
    registry.register(Method, "istouching", |world, receiver, args| {
        let a = super::engine::entity_id(world, receiver)?;
        let (b, _) = entity(world, arg(args, 0)?)?;
        let touching = crate::script::client::triggers::is_touching(world, a, b)?;
        Ok(Value::Int(touching.into()))
    });
    // `ent hide()` / `ent show()`: the entity's no-draw flag.
    registry.register(Method, "hide", |world, receiver, _| {
        set_hidden(world, receiver, true)
    });
    registry.register(Method, "show", |world, receiver, _| {
        set_hidden(world, receiver, false)
    });
    // `ent moveTo(point, time, [accel], [decel])`.
    registry.register(Method, "moveto", |world, receiver, args| {
        let to = vector(args, 0)?;
        let (time, accel, decel) = move_times(args)?;
        let id = super::engine::entity_id(world, receiver)?;
        let from = entity_vector(world, id, "origin");
        move_entity(
            world,
            receiver,
            "origin",
            from,
            to,
            (time, accel, decel),
            "movedone",
        )
    });
    // `ent rotatePitch|Yaw|Roll(delta, time, [accel], [decel])`: from the
    // entity's angles now.
    registry.register(Method, "rotatepitch", |world, receiver, args| {
        rotate_axis(world, receiver, args, 0)
    });
    registry.register(Method, "rotateyaw", |world, receiver, args| {
        rotate_axis(world, receiver, args, 1)
    });
    registry.register(Method, "rotateroll", |world, receiver, args| {
        rotate_axis(world, receiver, args, 2)
    });
}

fn set_hidden(world: &mut World, receiver: &Value, hidden: bool) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.hidden = hidden;
    }
    Ok(Value::Undefined)
}

fn entity_vector(world: &mut World, id: u64, field: &str) -> [f32; 3] {
    match super::super::players::entity_field(world, id, field) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// A client move's total, accel and decel times in seconds, as Black Ops'
/// client mover reads them: a total under 1 ms is 1 ms, and accel plus decel
/// may overrun the total by its float rounding.
fn move_times(args: &[Value]) -> Result<(f32, f32, f32), String> {
    let mut time = float(args, 1)?;
    if time <= 0.0 {
        return Err("total time must be positive".into());
    }
    if time < 0.001 {
        time = 0.001;
    }
    let (accel, decel) = if args.len() < 3 {
        (0.0, 0.0)
    } else {
        let accel = float(args, 2)?;
        if accel < 0.0 {
            return Err("accel time must be nonnegative".into());
        }
        let decel = if args.len() > 3 {
            let decel = float(args, 3)?;
            if decel < 0.0 {
                return Err("decel time must be nonnegative".into());
            }
            decel
        } else {
            0.0
        };
        (accel, decel)
    };
    if time < accel + decel {
        let stretched = time * 1.000_000_5;
        if stretched < accel + decel {
            return Err("accel time plus decel time is greater than total time".into());
        }
        time = stretched;
    }
    Ok((time, accel, decel))
}

fn move_entity(
    world: &mut World,
    receiver: &Value,
    field: &'static str,
    from: [f32; 3],
    to: [f32; 3],
    (time, accel, decel): (f32, f32, f32),
    done: &'static str,
) -> Result<Value, String> {
    let duration = (time * 1000.0).round().max(1.0) as i64;
    super::engine::start_motion(
        world,
        receiver,
        field,
        super::super::mechanics::MotionPath::Linear {
            from,
            to,
            accel,
            decel,
        },
        duration,
        done,
    )
}

fn rotate_axis(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let delta = float(args, 0)?;
    let times = move_times(args)?;
    let from = entity_vector(world, id, "angles");
    let mut to = from;
    to[axis] += delta;
    move_entity(world, receiver, "angles", from, to, times, "rotatedone")
}

fn register_lights(registry: &mut NativeRegistry) {
    use Namespace::Method;
    // `ent makeLight(pl)`: the entity becomes the map's primary light `pl`,
    // taking its color (normalized by its brightest channel, which is its
    // intensity), exponent, radius, cone, direction and origin.
    registry.register(Method, "makelight", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let index = int(args, 0)?;
        let light = usize::try_from(index)
            .ok()
            .and_then(|index| world.resource::<ClientLights>().map.get(index).cloned())
            .ok_or_else(|| format!("makelight: the map has no primary light {index}"))?;
        let c = light.color;
        let mut brightest = c[0];
        if brightest < c[1] {
            brightest = c[1];
        }
        if brightest < c[2] {
            brightest = c[2];
        }
        let (intensity, color) = if brightest == 0.0 {
            (0.0, [1.0; 3])
        } else {
            let scale = 1.0 / brightest;
            (brightest, [c[0] * scale, c[1] * scale, c[2] * scale])
        };
        let mut bytes = pack_color(color);
        bytes[3] = light.exponent;
        let back = light.direction.map(|d| -d);
        let angles = math_iw4::vect_to_angles(back);
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(light.origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "type", Value::string("NA"));
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.hidden = true;
        }
        world.resource_mut::<ClientLights>().lights.insert(
            id,
            ClientLight {
                index: index as usize,
                color: bytes,
                intensity,
                radius: light.radius,
                cos_half_fov_outer: light.cos_half_fov_outer,
                cos_half_fov_inner: light.cos_half_fov_inner,
                direction: None,
                origin: None,
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "getlightcolor", |world, receiver, _| {
        let light = light(world, receiver, LIGHT_ONLY_PERIOD)?;
        Ok(Value::Vector(std::array::from_fn(|i| {
            f32::from(light.color[i]) * 0.003_921_568_9
        })))
    });
    registry.register(Method, "setlightcolor", |world, receiver, args| {
        let color = vector(args, 0)?;
        light_mut(world, receiver, |light| {
            let exponent = light.color[3];
            light.color = pack_color(color);
            light.color[3] = exponent;
            Ok(())
        })
    });
    registry.register(Method, "getlightintensity", |world, receiver, _| {
        Ok(Value::Float(light(world, receiver, LIGHT_ONLY)?.intensity))
    });
    registry.register(Method, "setlightintensity", |world, receiver, args| {
        let intensity = float(args, 0)?;
        light_mut(world, receiver, |light| {
            if intensity < -0.001 {
                return Err("intensity must be >= 0".into());
            }
            light.intensity = intensity.max(0.0);
            Ok(())
        })
    });
    registry.register(Method, "getlightradius", |world, receiver, _| {
        Ok(Value::Float(light(world, receiver, LIGHT_ONLY)?.radius))
    });
    // `setLightRadius(r)`: at most the radius the map was compiled with.
    registry.register(Method, "setlightradius", |world, receiver, args| {
        let radius = float(args, 0)?;
        let compiled = compiled_light(world, receiver)?.radius;
        light_mut(world, receiver, |light| {
            if radius < -0.001 {
                return Err("radius must be >= 0".into());
            }
            if compiled + 0.001 < radius {
                return Err(format!(
                    "radius must be less than the bsp radius ({})",
                    format_g(f64::from(compiled))
                ));
            }
            let clamped = if radius - compiled < 0.0 {
                radius
            } else {
                compiled
            };
            light.radius = if -radius < 0.0 { clamped } else { 0.0 };
            Ok(())
        })
    });
    // The cone angles in degrees, from their stored half-angle cosines.
    registry.register(Method, "getlightfovinner", |world, receiver, _| {
        let cos = light(world, receiver, LIGHT_ONLY)?.cos_half_fov_inner;
        Ok(Value::Float(fov_degrees(cos)))
    });
    registry.register(Method, "getlightfovouter", |world, receiver, _| {
        let cos = light(world, receiver, LIGHT_ONLY)?.cos_half_fov_outer;
        Ok(Value::Float(fov_degrees(cos)))
    });
    // `setLightFovRange(outer, [inner])`: no wider than the compiled cone.
    registry.register(Method, "setlightfovrange", |world, receiver, args| {
        let compiled = compiled_light(world, receiver)?;
        let outer = float(args, 0)?;
        let inner = if args.len() == 2 {
            Some(float(args, 1)?)
        } else {
            None
        };
        light_mut(world, receiver, |light| {
            if outer < 0.999 || 136.001 <= outer {
                return Err("outer fov must be in the range of 1 to 136".into());
            }
            let cos_outer = half_cos(outer);
            if cos_outer < compiled.cos_half_fov_outer - 0.001 {
                return Err(
                    "outer fov cannot be larger than the fov when the map was compiled".into(),
                );
            }
            let mut outer_cos = if cos_outer - 1.0 < 0.0 {
                cos_outer
            } else {
                1.0
            };
            if 0.0 <= compiled.cos_half_fov_outer - cos_outer {
                outer_cos = compiled.cos_half_fov_outer;
            }
            let inner_cos = match inner {
                Some(inner) => {
                    if inner < -0.001 || outer + 0.001 <= inner {
                        return Err("inner fov must be in the range of 0 to outer fov".into());
                    }
                    let cos_inner = half_cos(inner);
                    let capped = if cos_inner - 1.0 < 0.0 {
                        cos_inner
                    } else {
                        1.0
                    };
                    if (outer_cos + 0.001) - cos_inner < 0.0 {
                        capped
                    } else {
                        outer_cos + 0.001
                    }
                }
                None => {
                    if compiled.cos_half_fov_inner - (outer_cos + 0.001) < 0.0 {
                        compiled.cos_half_fov_inner
                    } else {
                        outer_cos + 0.001
                    }
                }
            };
            light.cos_half_fov_outer = outer_cos;
            light.cos_half_fov_inner = inner_cos;
            Ok(())
        })
    });
    registry.register(Method, "getlightexponent", |world, receiver, _| {
        Ok(Value::Int(i32::from(
            light(world, receiver, LIGHT_ONLY)?.color[3],
        )))
    });
    registry.register(Method, "setlightexponent", |world, receiver, args| {
        let exponent = int(args, 0)?;
        light_mut(world, receiver, |light| {
            if !(0..=100).contains(&exponent) {
                return Err("exponent must be in the range from 0 to 100".into());
            }
            light.color[3] = exponent as u8;
            Ok(())
        })
    });
}

/// What a light builtin says of an entity that is not a light; the color
/// getter's message ends in a period.
const LIGHT_ONLY: &str = "Function can only be called on a 'light' entity";
const LIGHT_ONLY_PERIOD: &str = "Function can only be called on a 'light' entity.";

fn light(world: &World, receiver: &Value, refusal: &str) -> Result<ClientLight, String> {
    let (id, _) = entity(world, receiver)?;
    world
        .resource::<ClientLights>()
        .lights
        .get(&id)
        .cloned()
        .ok_or_else(|| refusal.to_owned())
}

fn light_mut(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut ClientLight) -> Result<(), String>,
) -> Result<Value, String> {
    let (id, _) = entity(world, receiver)?;
    let mut lights = world.resource_mut::<ClientLights>();
    let light = lights.lights.get_mut(&id).ok_or(LIGHT_ONLY)?;
    change(light)?;
    Ok(Value::Undefined)
}

/// The primary light a light entity was made from, as the map was compiled.
fn compiled_light(
    world: &World,
    receiver: &Value,
) -> Result<crate::script::MapPrimaryLight, String> {
    let index = light(world, receiver, LIGHT_ONLY)?.index;
    Ok(world.resource::<ClientLights>().map[index].clone())
}

/// A color as the client packs it into bytes: each channel times 255,
/// rounded to nearest, kept to 0..255.
fn pack_color(color: [f32; 3]) -> [u8; 4] {
    let byte = |c: f32| {
        let n = (f64::from(c * 255.0) + 9.313_225_746_154_785e-10).round_ties_even() as i32;
        if n < 0xff { n.max(0) as u8 } else { 0xff }
    };
    [byte(color[0]), byte(color[1]), byte(color[2]), 0]
}

/// The cosine of half a cone angle in degrees.
fn half_cos(degrees: f32) -> f32 {
    f64::from(degrees * 0.017_453_292 * 0.5).cos() as f32
}

/// A cone angle in degrees from the cosine of its half.
fn fov_degrees(cos: f32) -> f32 {
    (f64::from(cos).acos() as f32) * 2.0 * 57.295_776
}

/// A number as C's `%g` prints it.
fn format_g(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-4..6).contains(&exponent) {
        let text = format!("{:.5e}", value);
        let (mantissa, power) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let power: i32 = power.parse().unwrap_or(0);
        let sign = if power < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", power.abs());
    }
    let decimals = (5 - exponent).max(0) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// An effect's orientation when the script gives none: straight up.
const DEFAULT_EFFECT_ANGLES: [f32; 3] = [270.0, 0.0, 0.0];

/// The client's time now: the level time of its snapshot.
fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

/// Seconds as the client rounds them to msec.
pub(super) fn seconds_to_msec(seconds: f32) -> i32 {
    (f64::from(seconds * 1000.0) + 9.313_225_746_154_785e-10).round_ties_even() as i32
}

fn effect_id(args: &[Value], at: usize) -> Result<i32, String> {
    let id = int(args, at)?;
    if !EFFECT_IDS.contains(&id) {
        return Err(format!("CScr_PlayFX: invalid effect id {id}"));
    }
    Ok(id)
}

/// The effect registered at an index, and how long it lives.
fn effect_name_life(world: &World, id: i32) -> (Option<String>, i32) {
    let table = world.resource::<ClientEffectTable>();
    let name = table.names.get(&id).cloned();
    let life = name.as_deref().map_or(0, |name| table.life(name));
    (name, life)
}

/// A direction argument, normalized; a zero one is the script's error.
fn direction(
    world: &World,
    args: &[Value],
    at: usize,
    id: i32,
    which: &str,
) -> Result<[f32; 3], String> {
    let v = vector(args, at)?;
    let length = (v[1] * v[1] + v[2] * v[2] + v[0] * v[0]).sqrt();
    let scale = 1.0 / if 0.0 <= -length { 1.0 } else { length };
    if length == 0.0 {
        let effect = world
            .resource::<ClientEffectTable>()
            .names
            .get(&id)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "not successfully loaded".into());
        return Err(format!(
            "playFx called with (0 0 0) {which} direction (effect = {effect})\n"
        ));
    }
    Ok(v.map(|c| c * scale))
}

/// The axis of a forward and an up direction: up made square to forward.
fn forward_up_axis(forward: [f32; 3], up: [f32; 3]) -> Result<[[f32; 3]; 3], String> {
    let along = -(up[2] * forward[2] + up[1] * forward[1] + up[0] * forward[0]);
    let up = [
        forward[0] * along + up[0],
        along * forward[1] + up[1],
        forward[2] * along + up[2],
    ];
    let length = (up[2] * up[2] + up[0] * up[0] + up[1] * up[1]).sqrt();
    let scale = 1.0 / if 0.0 <= -length { 1.0 } else { length };
    if length == 0.0 {
        return Err(
            "forward and up vectors are the same direction or exact opposite directions".into(),
        );
    }
    let up = up.map(|c| c * scale);
    let left = [
        up[1] * forward[2] - up[2] * forward[1],
        up[2] * forward[0] - forward[2] * up[0],
        forward[1] * up[0] - up[1] * forward[0],
    ];
    Ok([forward, left, up])
}

/// Plays an effect at a point from `begin`, answering its handle: it is
/// alive until the local client's effect system says it has run out.
fn play_effect(
    world: &mut World,
    id: i32,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    begin: i32,
) -> i32 {
    let (name, _) = effect_name_life(world, id);
    let mut output = world.resource_mut::<ClientOutput>();
    let handle = output.effect_handle();
    if let Some(name) = name {
        output.effects.insert(
            handle,
            ClientEffect {
                name,
                origin,
                forward: axis[0],
                up: axis[2],
                begin_ms: Some(begin),
                end_ms: None,
                end: crate::EffectEnd::Playing,
                bolt: None,
            },
        );
    }
    handle
}

/// Plays a client sound, answering its playback id.
fn play(world: &mut World, alias: String, origin: Option<[f32; 3]>, volume: f32) -> Value {
    let mut output = world.resource_mut::<ClientOutput>();
    let id = output.sound_id();
    output.issued.insert(id);
    output
        .sound_commands
        .push(crate::ScriptAudioCommand::ClientSound {
            client: crate::ClientId(0),
            id,
            alias,
            origin,
            volume,
            fade_in_ms: 0,
        });
    Value::Int(id)
}

/// A sound's volume argument as the client clamps it: 1 or more is 1, and
/// 0 or less is 0.
fn sound_volume(args: &[Value], at: usize) -> Result<f32, String> {
    let Some(volume) = optional(args, at, float)? else {
        return Ok(1.0);
    };
    let mut clamped = volume;
    if 0.0 <= volume - 1.0 {
        clamped = 1.0;
    }
    if 0.0 <= -volume {
        clamped = 0.0;
    }
    Ok(clamped)
}

fn register_sound_ids(registry: &mut NativeRegistry) {
    use Namespace::Function;
    // `soundPlaying(id)`: whether the sound system still plays it; a sound
    // played this frame plays until the sound system says otherwise, and
    // with no sound system nothing plays.
    registry.register(Function, "soundplaying", |world, _, args| {
        if args.len() != 1 {
            return Err("soundplaying takes one parameter.".into());
        }
        let id = int(args, 0)?;
        Ok(Value::Int(sound_playing(world, id).into()))
    });
    registry.register(Function, "getplaybacktime", |world, _, args| {
        if args.len() != 1 {
            return Err("getplaybacktime takes a playback id.".into());
        }
        let id = int(args, 0)?;
        Ok(Value::Int(
            sound_status(world, id).map_or(0, |status| status.playback_ms),
        ))
    });
    registry.register(Function, "getknownlength", |world, _, args| {
        if args.len() != 1 {
            return Err("GetKnownLength takes a playback id.".into());
        }
        let id = int(args, 0)?;
        Ok(Value::Int(
            sound_status(world, id).map_or(0, |status| status.length_ms),
        ))
    });
    registry.register(Function, "stopsound", |world, _, args| {
        if args.len() != 1 {
            return Err("stopsound takes one parameter.".into());
        }
        let id = int(args, 0)?;
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientSoundStop {
                client: crate::ClientId(0),
                id,
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "setsoundvolume", |world, _, args| {
        if args.len() != 2 {
            return Err("setsoundvolume takes two parameters.".into());
        }
        let volume = float(args, 1)?;
        if !volume.is_finite() {
            return Err("setsoundvolume cannot be NAN.".into());
        }
        let id = int(args, 0)?;
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientSoundVolume {
                client: crate::ClientId(0),
                id,
                volume,
            },
        );
        Ok(Value::Undefined)
    });
    // The mix snapshot setters: the group, ambient, health and level fade
    // slots (2, 1, 8 and 10). The health and level fade weights are clamped
    // to 0..1; the others are put at full weight.
    macro_rules! snapshot_setter {
        ($name:literal, $slot:literal, $weighted:literal, $arity:literal) => {
            registry.register(Function, $name, |world, _, args| {
                set_snapshot(world, args, $slot, $weighted, $arity)
            });
        };
    }
    // `setReverb(priority, room, [dry 1], [wet 0.5], [fade 0])` and
    // `deactivateReverb(priority, [fade 0])`: a wrong argument count or
    // priority is printed, not raised; the fade is taken to msec (rounded,
    // no less than 0).
    registry.register(Function, "setreverb", |world, _, args| {
        if !(2..=5).contains(&args.len()) {
            diag::warn!(Sim, "csc: setreverb: Incorrect number of parameters");
            return Ok(Value::Undefined);
        }
        let fade = optional(args, 4, float)?.unwrap_or(0.0);
        let wet = optional(args, 3, float)?.unwrap_or(0.5);
        let dry = optional(args, 2, float)?.unwrap_or(1.0);
        let room = string(args, 1)?.to_string();
        let priority = reverb_priority(&string(args, 0)?);
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientReverb {
                client: crate::ClientId(0),
                priority,
                room: Some((room, dry, wet)),
                fade_ms: reverb_fade_ms(fade),
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "deactivatereverb", |world, _, args| {
        let fade = match args.len() {
            1 => 0.0,
            2 => float(args, 1)?,
            _ => {
                diag::warn!(Sim, "csc: deactivatereverb: Incorrect number of params.");
                return Ok(Value::Undefined);
            }
        };
        let fade_ms = reverb_fade_ms(fade);
        let priority = reverb_priority(&string(args, 0)?);
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientReverb {
                client: crate::ClientId(0),
                priority,
                room: None,
                fade_ms,
            },
        );
        Ok(Value::Undefined)
    });
    // Black Ops' own message names the wrong builtin.
    registry.register(Function, "setsoundcontext", |world, _, args| {
        if args.len() != 2 {
            return Err("setAmbientSnapshot takes two parameters.".into());
        }
        let value = string(args, 1)?.to_string();
        let context = string(args, 0)?.to_string();
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientSoundContext {
                client: crate::ClientId(0),
                context,
                value,
            },
        );
        Ok(Value::Undefined)
    });
    snapshot_setter!(
        "setgroupsnapshot",
        2,
        false,
        "setGroupSnapshot takes one parameter."
    );
    snapshot_setter!(
        "setambientsnapshot",
        1,
        false,
        "setAmbientSnapshot takes one parameter."
    );
    snapshot_setter!(
        "sethealthsnapshot",
        8,
        true,
        "setHealthSnapshot takes two parameters."
    );
    snapshot_setter!(
        "setlevelfadesnapshot",
        10,
        true,
        "setLevelFadeSnapshot takes two parameters."
    );
    registry.register(Function, "setsoundvolumerate", |world, _, args| {
        if args.len() != 2 {
            return Err("setsoundvolumerate takes two parameters.".into());
        }
        let rate = float(args, 1)?;
        if !rate.is_finite() {
            return Err("setsoundvolumerate cannot be NAN.".into());
        }
        let id = int(args, 0)?;
        world.resource_mut::<ClientOutput>().sound_commands.push(
            crate::ScriptAudioCommand::ClientSoundVolumeRate {
                client: crate::ClientId(0),
                id,
                rate,
            },
        );
        Ok(Value::Undefined)
    });
}

/// A reverb priority: the level's (`snd_enveffectsprio_level`, 1) or the
/// shellshock's (2); any other name is printed and taken as the level's.
fn reverb_priority(name: &str) -> u8 {
    if name.eq_ignore_ascii_case("snd_enveffectsprio_level") {
        1
    } else if name.eq_ignore_ascii_case("snd_enveffectsprio_shellshock") {
        2
    } else {
        {
            diag::warn!(
                Sim,
                "csc: priority must be 'snd_enveffectsprio_level' or 'snd_enveffectsprio_shellshock'"
            );
            1
        }
    }
}

/// A reverb fade in seconds as msec: rounded, and 0 when below 1.
fn reverb_fade_ms(seconds: f32) -> i32 {
    let ms = (f64::from(seconds * 1000.0) + f64::from(2.0f32.powi(-30))).round_ties_even();
    let ms = if ms.is_finite() && ms.abs() < 2_147_483_648.0 {
        ms as i32
    } else {
        i32::MIN
    };
    if ms < 1 { 0 } else { ms }
}

/// Puts a mix snapshot in one of the client's snapshot slots.
fn set_snapshot(
    world: &mut World,
    args: &[Value],
    slot: u8,
    weighted: bool,
    arity: &str,
) -> Result<Value, String> {
    if args.len() != 1 + usize::from(weighted) {
        return Err(arity.into());
    }
    let value = if weighted {
        let weight = float(args, 1)?;
        let mut value = weight;
        if 0.0 <= weight - 1.0 {
            value = 1.0;
        }
        if 0.0 <= -weight {
            value = 0.0;
        }
        value
    } else {
        1.0
    };
    let name = string(args, 0)?.to_string();
    world.resource_mut::<ClientOutput>().sound_commands.push(
        crate::ScriptAudioCommand::ClientSnapshot {
            client: crate::ClientId(0),
            slot,
            name,
            value,
        },
    );
    Ok(Value::Undefined)
}

fn sound_status(world: &World, id: i32) -> Option<crate::ClientSoundStatus> {
    world
        .resource::<ClientSoundReports>()
        .statuses
        .get(&id)
        .copied()
}

fn sound_playing(world: &World, id: i32) -> bool {
    crate::script::client::sound_playing(world, id)
}

/// A loop's fade: 0 to 32 seconds (the fade itself is not applied yet).
fn loop_fade(args: &[Value], at: usize, native: &str) -> Result<(), String> {
    if let Some(fade) = optional(args, at, super::super::args::float)?
        && !(0.0..=32.0).contains(&fade)
    {
        return Err(format!(
            "{native}: invalid fade value {fade:.6}. it must be between 0 and 32 seconds."
        ));
    }
    Ok(())
}

/// A point emitter's alias and origin, which must be its two arguments.
fn emitter(args: &[Value], native: &str) -> Result<LoopSource, String> {
    if args.len() != 2 {
        return Err(format!(
            "{native} has {} parameters.  There should be two.",
            args.len()
        ));
    }
    let alias = string(args, 0)?;
    let origin = vector(args, 1)?;
    Ok(LoopSource::Emitter(alias, origin.map(f32::to_bits)))
}

/// The line emitters the client's sound system holds.
const LINE_EMITTERS: usize = 128;

/// How near a line emitter's ends must be to stop it (squared units).
const LINE_EMITTER_MATCH_SQ: f32 = 1.528_790_02e-5;

/// A line emitter's alias and ends, which must be its three arguments.
fn line_emitter(args: &[Value], native: &str) -> Result<(String, [f32; 3], [f32; 3]), String> {
    if args.len() != 3 {
        return Err(format!(
            "{native} has {} parameters.  There should be three.",
            args.len()
        ));
    }
    let start = vector(args, 1)?;
    let end = vector(args, 2)?;
    Ok((string(args, 0)?, start, end))
}

/// The extra camera's field of view while it is on, until a script sets
/// one (`cg_fovExtraCam`'s default).
const EXTRA_CAMERA_FOV: f32 = 65.0;

/// The extra camera this frame: its entity's origin and angles (Black Ops
/// reads the camera entity's lerp origin and angles each frame) and the
/// field of view.
pub(crate) fn extra_camera(world: &mut World) -> Option<game_api::ExtraCamera> {
    let (id, fov) = world.resource::<ClientOutput>().extra_camera?;
    Some(game_api::ExtraCamera {
        origin: entity_vector(world, id, "origin"),
        angles: entity_vector(world, id, "angles"),
        fov,
    })
}

/// Black Ops' extra camera: one client entity whose view the renderer draws
/// for the materials that show it. Each native takes its arguments in the
/// game's order: the count, then the local client.
fn register_extra_camera(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Method, "isextracam", |world, receiver, args| {
        if args.len() != 1 {
            return Err("isextracam() called with wrong params.\n".into());
        }
        local_index(args)?;
        let (id, _) = entity(world, receiver)?;
        let output = &mut world.resource_mut::<ClientOutput>().extra_camera;
        if output.is_some() {
            return Err("There can be only one extra camera active in the level.".into());
        }
        *output = Some((id, EXTRA_CAMERA_FOV));
        Ok(Value::Undefined)
    });
    registry.register(Function, "stopextracam", |world, _, args| {
        if args.len() != 1 {
            return Err("CScr_StopExtraCam() called with wrong params.\n".into());
        }
        local_index(args)?;
        world.resource_mut::<ClientOutput>().extra_camera = None;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setextracamfov", |world, _, args| {
        if args.len() != 2 {
            return Err("CScr_SetExtraCamFov() called with wrong params.\n".into());
        }
        let fov = float(args, 1)?;
        local_index(args)?;
        match &mut world.resource_mut::<ClientOutput>().extra_camera {
            Some((_, camera_fov)) => {
                *camera_fov = fov;
                Ok(Value::Undefined)
            }
            None => {
                Err("CScr_SetExtraCamFov(), There is no extra cam active in the level.\n".into())
            }
        }
    });
    registry.register(Function, "extracamconfig", |_, _, _| {
        Err("extracamconfig is deprecated".into())
    });
    // The client's `HasCollectible(n)` reads the same dvar as the server's.
    registry.register(Function, "hascollectible", |world, _, args| {
        super::t5_zombie::has_collectible(world, args)
    });
    // `gettagorigin(tag)` / `gettagangles(tag)` / `gettagforwardvector(tag)`:
    // bone world origin, angles, or forward axis on a mirrored or client
    // entity's DObj.
    registry.register(Method, "gettagorigin", |world, receiver, args| {
        Ok(Value::Vector(client_tag_pose(world, receiver, args)?.0))
    });
    registry.register(Method, "gettagangles", |world, receiver, args| {
        Ok(Value::Vector(math_iw4::axis_to_angles(
            client_tag_pose(world, receiver, args)?.1,
        )))
    });
    registry.register(Method, "gettagforwardvector", |world, receiver, args| {
        Ok(Value::Vector(client_tag_pose(world, receiver, args)?.1[0]))
    });
    // `createDynEntAndLaunch(localClientNum, model, pos, angles, hitpos, force, [fx], [mature])`:
    // spawns a dynamic clutter piece (such as a thrown zombie gib) and
    // applies a directional impulse at `hitpos`.
    registry.register(Function, "createdynentandlaunch", |world, _, args| {
        if args.len() < 6 {
            return Err(
                "CreateDynEntAndLaunch called with invalid params. CreateDynEntAndLaunch( <local client num>, <model>, <pos>, <angles>, <hitpos>, <force>, <fx>, <mature> )"
                    .into(),
            );
        }
        local_index(args)?;
        let model = string(args, 1)?;
        let pos = vector(args, 2)?;
        let angles = vector(args, 3)?;
        let hit_pos = vector(args, 4)?;
        let force = vector(args, 5)?;
        let fx = if args.len() > 6 && !matches!(args[6], Value::Undefined) {
            let id = int(args, 6)?;
            if id != 0 {
                effect_name_life(world, id).0
            } else {
                None
            }
        } else {
            None
        };
        if args.len() > 7 {
            let _ = int(args, 7)?;
        }
        diag::info!(
            Sim,
            "t5 client createdynentandlaunch model={} pos=({:.1},{:.1},{:.1}) force=({:.1},{:.1},{:.1}) fx={:?}",
            model,
            pos[0],
            pos[1],
            pos[2],
            force[0],
            force[1],
            force[2],
            fx
        );
        world
            .resource_mut::<ClientOutput>()
            .spawn_dyn_ent(crate::SpawnedDynEntLaunch {
                model,
                pos,
                angles,
                hit_pos,
                force,
                fx,
            });
        Ok(Value::Undefined)
    });
    // `launchRagdoll(dir, [bone])`: client-side ragdoll impulse.
    registry.register(Method, "launchragdoll", |world, receiver, args| {
        let _ = entity(world, receiver)?;
        if args.is_empty() {
            return Err(
                "LaunchRagdoll called with 0 params. Pick from:\n LaunchRagdoll( <vector dir>, <string bone> );\nLaunchRagdoll( <vector dir> );"
                    .into(),
            );
        }
        let _ = vector(args, 0)?;
        if args.len() > 1 {
            let _ = string(args, 1)?;
        }
        Ok(Value::Undefined)
    });
}

fn client_tag_pose(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<([f32; 3], [[f32; 3]; 3]), String> {
    let (id, _) = entity(world, receiver)?;
    let tag = string(args, 0)?;
    let origin = match super::super::players::entity_field(world, id, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let angles = match super::super::players::entity_field(world, id, "angles") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    if let Some(dobjs) = world.get_resource::<crate::script::client::ClientEntityDobjs>()
        && let Some(dobj) = dobjs.0.get(&id)
    {
        let matrix = dobj.tag_world_matrix(&tag).or_else(|| {
            if tag.eq_ignore_ascii_case("j_helmet") {
                dobj.tag_world_matrix("j_head")
            } else {
                None
            }
        });
        if let Some(matrix) = matrix {
            let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
            return Ok((
                matrix.w_axis.truncate().to_array(),
                [
                    axis(matrix.x_axis),
                    axis(matrix.y_axis),
                    axis(matrix.z_axis),
                ],
            ));
        }
    }
    let local = match tag.to_ascii_lowercase().as_str() {
        "tag_origin" => [0.0, 0.0, 0.0],
        "j_helmet" | "j_head" | "j_neck" | "tag_eye" => [0.0, 0.0, 58.0],
        "j_spinelower" | "j_spineupper" | "j_spine4" => [0.0, 0.0, 38.0],
        "j_elbow_le" | "j_shoulder_le" => [0.0, 14.0, 46.0],
        "j_elbow_ri" | "j_shoulder_ri" => [0.0, -14.0, 46.0],
        "j_knee_le" | "j_hip_le" => [0.0, 8.0, 18.0],
        "j_knee_ri" | "j_hip_ri" => [0.0, -8.0, 18.0],
        _ => [0.0, 0.0, 36.0],
    };
    let axis = math_iw4::angles_to_axis(angles);
    let pos = std::array::from_fn(|i| {
        origin[i] + axis[0][i] * local[0] + axis[1][i] * local[1] + axis[2][i] * local[2]
    });
    Ok((pos, axis))
}

/// The local client a native names: Black Ops refuses any but 0.
fn local_index(args: &[Value]) -> Result<usize, String> {
    let local = int(args, 0)?;
    if local != 0 {
        return Err(format!(
            "Trying to get a local client index for a client '{local}' that is not a local client."
        ));
    }
    Ok(0)
}

fn locals(world: &World) -> Vec<u64> {
    world
        .resource::<ClientView>()
        .locals
        .iter()
        .map(|client| client.object)
        .collect()
}

fn entity<'a>(
    world: &'a World,
    receiver: &Value,
) -> Result<(u64, &'a super::super::entities::ScriptEntity), String> {
    world
        .resource::<RoundScript>()
        .entity(receiver)
        .ok_or_else(|| "not an entity".into())
}

fn is_alive(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let alive = matches!(
        super::super::players::entity_field(world, id, "type"),
        Value::String(kind) if matches!(&*kind, "player" | "actor" | "vehicle" | "plane")
    );
    if !alive {
        return Err("IsAlive called on the wrong kind of entity".into());
    }
    Ok(Value::Int(1))
}

/// `ui3dsetwindow(<window id>, <x>, <y>, <width>, <height>)`: Black Ops
/// places a window of its 3D UI texture, as fractions of the texture's size.
/// The corner is clamped into the texture and the size to what is left of
/// it; a window id past the six is ignored.
fn register_ui3d(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "ui3dsetwindow", |world, _, args| {
        if args.len() != 5 {
            return Err("USAGE: ui3dsetwindow(<window id>, <x>, <y>, <width>, <height>)\n".into());
        }
        let window = int(args, 0)?;
        let [x, y, w, h] = [1, 2, 3, 4].map(|i| float(args, i));
        let (x, y, w, h) = (x?, y?, w?, h?);
        let Some(window) = usize::try_from(window)
            .ok()
            .filter(|&window| window < game_api::UI3D_WINDOWS)
        else {
            return Ok(Value::Undefined);
        };
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let place = [x, y, w.min(1.0 - x).max(0.0), h.min(1.0 - y).max(0.0)];
        world
            .resource_mut::<ClientOutput>()
            .ui3d_windows
            .get_or_insert_default()[window] = place;
        Ok(Value::Undefined)
    });
}

/// The earthquakes the client's scripts started this frame reach the local
/// client's view.
pub(crate) fn deliver_earthquakes(client: &mut World, server: &mut World) {
    let quakes = std::mem::take(&mut client.resource_mut::<ClientOutput>().earthquakes);
    for quake in quakes {
        super::scene_effects::shake_camera(
            server,
            quake.scale,
            quake.duration_ms,
            quake.origin,
            quake.radius,
        );
    }
}

/// The server's weapon by its script name, with its world model and hide
/// tags; `None` for an unknown name (and "none", weapon 0).
pub(super) fn weapon_model(world: &mut World, name: &str) -> Option<(String, Vec<String>)> {
    let frame = crate::frame::FrameWorld::from_world(world);
    let weapon = frame
        .weapon_index_by_script_name(name)
        .filter(|&weapon| weapon != 0)?;
    let (model, tags) = frame.weapon_world_model(weapon).unwrap_or(("", &[]));
    Some((model.to_owned(), tags.to_vec()))
}

/// An entity's parts show again, then the weapon's hide tags hide, as
/// `useWeaponHideTags` does it on the server and the client alike: a
/// missing argument or an unknown weapon is an error.
pub(super) fn use_weapon_hide_tags(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    tags: impl FnOnce(&mut World, &str) -> Option<Vec<String>>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("useweaponhidetags <weaponName>.\n".into());
    }
    let name = string(args, 0)?;
    let Some(tags) = tags(world, &name) else {
        return Err(format!(
            "useweaponhidetags called with unknown weapon name {name}\n"
        ));
    };
    let id = super::engine::entity_id(world, receiver)?;
    if std::env::var_os("IW4L_PROBE").is_some() { eprintln!("PROBE useweaponhidetags {name} id={id} tags={tags:?} client={}", crate::script::host::client_scripts::is_client(world)); }
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.part_ops.push(("".into(), false));
        entity
            .part_ops
            .extend(tags.iter().map(|tag| (std::sync::Arc::from(tag.as_str()), true)));
    }
    Ok(Value::Undefined)
}

/// The box's and the client's weapon models: their world model and the
/// parts they hide.
fn register_weapon_models(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    // `GetWeaponModel(weapon, [variant], [viewmodel])`: the weapon's world
    // model; an unknown weapon other than "none" is printed and answers "".
    registry.register(Function, "getweaponmodel", |world, _, args| {
        let name = string(args, 0)?;
        let model = super::super::client_scripts::with_server(world, |server| {
            weapon_model(server, &name)
        })
        .flatten();
        let Some((model, _)) = model else {
            if !name.is_empty() && name != "none" {
                diag::info!(Sim, "csc: unknown weapon '{name}' in getWeaponModel");
            }
            return Ok(Value::string(""));
        };
        if std::env::var_os("IW4L_PROBE").is_some() { eprintln!("PROBE csc getweaponmodel {name} -> {model}"); }
        Ok(Value::string(&model))
    });
    registry.register(Method, "useweaponhidetags", |world, receiver, args| {
        use_weapon_hide_tags(world, receiver, args, |world, name| {
            super::super::client_scripts::with_server(world, |server| weapon_model(server, name))
                .flatten()
                .map(|(_, tags)| tags)
        })
    });
}

fn alternate_aim(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let (call, verb) = if on {
        ("UseAlternateAim", "UseAlternateAimParams")
    } else {
        ("ClearAlternateAim", "ClearAlternateAimParams")
    };
    let (id, ent) = entity(world, receiver)?;
    if ent.kind != EntityKind::Player {
        return Err(format!("You can only call {call} on players."));
    }
    if !locals(world).contains(&id) {
        return Err(format!("{verb} had no effect..."));
    }
    world.resource_mut::<ClientOutput>().alternate_aim = on;
    Ok(Value::Undefined)
}

/// Deadshot's aim, collectibles and the input mappings.
fn register_profile(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    // `player useAlternateAimParams()` / `clearAlternateAimParams()`: the
    // local client named takes the alternate aim parameters, or lets them
    // go; any other player is an error.
    registry.register(Method, "usealternateaimparams", |world, receiver, _| {
        alternate_aim(world, receiver, true)
    });
    registry.register(Method, "clearalternateaimparams", |world, receiver, _| {
        alternate_aim(world, receiver, false)
    });
    // `SetCollectible(index)`: marks the collectible at that 1-based index
    // in the profile's `bg_collectibles`; an index past the string's end
    // marks nothing.
    registry.register(Function, "setcollectible", |world, _, args| {
        let index = int(args, 0)?;
        let held = world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get("bg_collectibles")
            .map_or(String::new(), |value| value.to_string());
        let mut marks: Vec<u8> = held.into_bytes();
        marks.truncate(49);
        if let Some(mark) = usize::try_from(index - 1)
            .ok()
            .and_then(|at| marks.get_mut(at))
        {
            *mark = b'1';
        }
        let marks = String::from_utf8_lossy(&marks).into_owned();
        world
            .resource_mut::<ClientOutput>()
            .client_dvars
            .push(("bg_collectibles".into(), marks));
        Ok(Value::Undefined)
    });
    // `ForceGameModeMappings(localClientNum, mode)`: a pad's binds for the
    // mode; without a pad nothing but the key state it clears.
    registry.register(Function, "forcegamemodemappings", |_, _, args| {
        if args.len() != 2 {
            return Err("Incorrect number of parameters passed to ForceGameModeMappings()\nUSAGE: ForceGameModeMappings( <localClientNum>, <modeName>)\n".into());
        }
        int(args, 0)?;
        string(args, 1)?;
        Ok(Value::Undefined)
    });
}
