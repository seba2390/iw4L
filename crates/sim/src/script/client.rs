//! A client's own script VM. Black Ops runs client scripts (CSC) on each
//! client, beside the server's scripts: they get a world of their own here,
//! with their own threads, objects and natives, and see only what the
//! client sees (`ClientView`), filled from the server's world each frame.

use bevy_ecs::prelude::{Resource, World};
use std::sync::Arc;

use super::host::entities::EntityKind;
use super::{Fault, MatchScript, NativeRegistry, Program, RoundScript, Value};

/// VM-generic builtins the client VM shares with the server's: they read and
/// write script values only.
const SHARED_BUILTINS: &[(super::Namespace, &str)] = {
    use super::Namespace::Function as F;
    &[
        (F, "abs"),
        (F, "absangleclamp180"),
        (F, "acos"),
        (F, "angleclamp180"),
        (F, "anglestoforward"),
        (F, "anglestoright"),
        (F, "anglestoup"),
        (F, "asin"),
        (F, "atan"),
        (F, "ceil"),
        (F, "closer"),
        (F, "combineangles"),
        (F, "cos"),
        (F, "distance"),
        (F, "distance2d"),
        (F, "distancesquared"),
        (F, "float"),
        (F, "floor"),
        (F, "getarraykeys"),
        (F, "getdvar"),
        (F, "getdvarfloat"),
        (F, "getdvarint"),
        (F, "getsubstr"),
        (F, "int"),
        (F, "isarray"),
        (F, "isdefined"),
        (F, "isstring"),
        (F, "issubstr"),
        (F, "length"),
        (F, "lengthsquared"),
        (F, "lerpfloat"),
        (F, "max"),
        (F, "min"),
        (F, "randomfloat"),
        (F, "randomfloatrange"),
        (F, "randomint"),
        (F, "randomintrange"),
        (F, "setdvar"),
        (F, "sin"),
        (F, "spawnstruct"),
        (F, "sqrt"),
        (F, "strtok"),
        (F, "tan"),
        (F, "tolower"),
        (F, "vectorcross"),
        (F, "vectordot"),
        (F, "vectorlerp"),
        (F, "vectornormalize"),
        (F, "vectortoangles"),
    ]
};

/// What a local client sees of itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct LocalClient {
    /// Its player entity in the client VM.
    pub(crate) object: u64,
    /// Whether the client has had a snapshot.
    pub(crate) has_snapshot: bool,
}

/// What the client VM's natives see each frame.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct ClientView {
    pub(crate) locals: Vec<LocalClient>,
    /// Whether the local client hosts the match (a listen server).
    pub(crate) host: bool,
}

/// What the server's scripts send the clients' scripts.
#[derive(Clone, Debug)]
pub(crate) enum ClientMessage {
    /// A client system's new state (`clientSysSetState`), for one client or
    /// all of them.
    SystemState {
        client: Option<u32>,
        system: Arc<str>,
        state: Arc<str>,
    },
    /// A client exploder the server started (a positive id) or stopped
    /// (a negative one), for every client.
    Exploder(i32),
    /// The local player stepped (its movement type and surface).
    Footstep(&'static str, &'static str),
}

/// A local player's footstep, for its client scripts.
pub(crate) fn footstep(
    server: &mut World,
    client: crate::ClientId,
    kind: &'static str,
    surface: &'static str,
) {
    if client.0 != 0 || !server.contains_resource::<ClientScripts>() {
        return;
    }
    if let Some(mut runtime) = server.get_resource_mut::<RoundScript>() {
        runtime
            .client_messages
            .push(ClientMessage::Footstep(kind, surface));
    }
}

/// A sound the client plays itself (`playsound`).
#[derive(Clone, Debug)]
pub(crate) struct ClientSound {
    pub(crate) alias: String,
    pub(crate) origin: [f32; 3],
}

/// What a client loop sound plays from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoopSource {
    /// A fake entity (`playloopsound(local, fakeent, alias)`).
    Fake(i32),
    /// A client entity (`ent playloopsound(alias)`).
    Entity(u64),
    /// A point emitter (`soundloopemitter(alias, origin)`), by its alias
    /// and origin's bits.
    Emitter(String, [u32; 3]),
}

/// A loop sound the client plays until it stops it.
#[derive(Clone, Debug)]
pub(crate) struct ClientLoop {
    pub(crate) alias: String,
    /// The loop's own number, which names it to the audio while it plays.
    pub(crate) serial: u32,
}

/// What the client VM's frame asks of the client: sounds to play, and the
/// loops it plays.
#[derive(Resource, Default)]
pub(crate) struct ClientOutput {
    pub(crate) sounds: Vec<ClientSound>,
    /// The next sound's id, as `playsound` answers it.
    pub(crate) next_sound: i32,
    pub(crate) loops: std::collections::BTreeMap<LoopSource, ClientLoop>,
    next_loop: u32,
}

impl ClientOutput {
    pub(crate) fn start_loop(&mut self, source: LoopSource, alias: String) {
        if self.loops.get(&source).is_some_and(|l| l.alias == alias) {
            return;
        }
        let serial = self.next_loop & 0x0fff_ffff;
        self.next_loop = self.next_loop.wrapping_add(1);
        self.loops.insert(source, ClientLoop { alias, serial });
    }
}

/// The client's fake entities (`spawnfakeent`): sound emitters of its own,
/// by handle.
#[derive(Resource, Default)]
pub(crate) struct FakeEnts {
    pub(crate) next: i32,
    pub(crate) origins: std::collections::BTreeMap<i32, [f32; 3]>,
}

/// Script loop sound owners of the client's loops; the server's script
/// entities' loops use lower numbers.
const CLIENT_LOOP_OWNER: u32 = 0x4000_0000;

/// The loops the local client plays, where they play from now.
pub(crate) fn loop_rows(server: &mut World) -> Vec<(crate::ScriptModelId, String, [f32; 3])> {
    let Some(mut client) = server.get_resource_mut::<ClientScripts>() else {
        return Vec::new();
    };
    let world = &mut client.world;
    let loops: Vec<(LoopSource, ClientLoop)> = world
        .resource::<ClientOutput>()
        .loops
        .iter()
        .map(|(source, sound)| (source.clone(), sound.clone()))
        .collect();
    let fakes = world
        .get_resource::<FakeEnts>()
        .map(|fakes| fakes.origins.clone())
        .unwrap_or_else(Default::default);
    let mut runtime = world.resource_mut::<RoundScript>();
    loops
        .iter()
        .filter_map(|(source, sound)| {
            let origin = match source {
                LoopSource::Fake(handle) => *fakes.get(handle)?,
                LoopSource::Entity(id) => match runtime.object_field(*id, "origin") {
                    Value::Vector(origin) => origin,
                    _ => return None,
                },
                LoopSource::Emitter(_, bits) => bits.map(f32::from_bits),
            };
            Some((
                crate::ScriptModelId::from_wire(CLIENT_LOOP_OWNER | sound.serial),
                sound.alias.clone(),
                origin,
            ))
        })
        .collect()
}

/// The client VM: its world, and the entries it starts with once the
/// client has a snapshot.
#[derive(Resource)]
pub(crate) struct ClientScripts {
    world: World,
    entries: Vec<String>,
    callbacks: Option<game_api::ClientCallbacks>,
    started: bool,
    stopped: bool,
}

impl NativeRegistry {
    /// The client VM's natives: the VM-generic builtins of `server` and
    /// those `register` adds.
    pub(crate) fn client(
        server: &NativeRegistry,
        register: fn(&mut NativeRegistry),
    ) -> NativeRegistry {
        let mut client = NativeRegistry::empty();
        for &(namespace, name) in SHARED_BUILTINS {
            if let Some(native) = server.get(namespace, name) {
                client.register(namespace, name, native);
            }
        }
        register(&mut client);
        client
    }
}

/// Installs the client VM for the match's local client.
pub(crate) fn install(
    server: &mut World,
    program: Program,
    natives: NativeRegistry,
    entries: Vec<String>,
    callbacks: Option<game_api::ClientCallbacks>,
    level: &super::LevelData,
) -> Result<(), Fault> {
    let mut world = World::new();
    let match_script = MatchScript {
        dvars: server.resource::<MatchScript>().dvars.clone(),
        ..Default::default()
    };
    world.insert_resource(RoundScript::new(&match_script));
    world.insert_resource(match_script);
    world.insert_resource(super::Mechanics::default());
    super::runtime::install_client(
        &mut world,
        Arc::new(program),
        natives,
        &level.entities,
        &level.keys,
    )?;
    let local = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let object = runtime
            .create_entity(EntityKind::Spawned, "player")
            .map_err(|m| client_fault("install", m))?;
        if let Some(entity) = runtime.entities.get_mut(&object) {
            entity.number = 0;
        }
        LocalClient {
            object,
            has_snapshot: false,
        }
    };
    world.insert_resource(ClientView {
        locals: vec![local],
        host: true,
    });
    world.insert_resource(ClientOutput::default());
    server.insert_resource(ClientScripts {
        world,
        entries,
        callbacks,
        started: false,
        stopped: false,
    });
    Ok(())
}

fn client_fault(function: &str, message: impl Into<String>) -> Fault {
    Fault::at(
        &super::Location {
            module: "<client>".into(),
            function: function.into(),
            line: 0,
            column: 0,
        },
        message,
    )
}

/// Runs the client VM's frame after the server's: the client has the
/// snapshot the server just made, its entries start on its first frame,
/// and its threads run.
pub(crate) fn advance_client_scripts(server: &mut World) {
    let Some(mut client) = server.remove_resource::<ClientScripts>() else {
        return;
    };
    let messages = std::mem::take(&mut server.resource_mut::<RoundScript>().client_messages);
    if !client.stopped {
        let (tick, reason) = {
            let request = server.resource::<crate::step::StepRequest>();
            (request.tick, request.reason)
        };
        if reason.advances_authority_world() {
            run_frame(&mut client, tick, reason, messages);
            let sounds = std::mem::take(&mut client.world.resource_mut::<ClientOutput>().sounds);
            for sound in sounds {
                super::host::natives::engine::sound_to_client(
                    server,
                    crate::ClientId(0),
                    &sound.alias,
                    sound.origin,
                );
            }
        }
    }
    server.insert_resource(client);
}

fn run_frame(
    client: &mut ClientScripts,
    tick: crate::Tick,
    reason: crate::StepReason,
    messages: Vec<ClientMessage>,
) {
    let world = &mut client.world;
    for local in &mut world.resource_mut::<ClientView>().locals {
        local.has_snapshot = true;
    }
    world.insert_resource(crate::step::StepRequest::frame(tick, reason));
    if !client.started {
        client.started = true;
        for entry in &client.entries {
            if let Err(fault) = super::runtime::start(world, entry, Value::level(), Vec::new()) {
                diag::warn!(Sim, "csc: client script {entry} not started: {fault}");
            }
        }
    }
    // The server's commands reach the client with its snapshot, before its
    // scripts run.
    for message in messages {
        let Some(callbacks) = client.callbacks else {
            continue;
        };
        let (function, args) = match message {
            ClientMessage::SystemState {
                client: Some(other),
                ..
            } if other != 0 => continue,
            ClientMessage::SystemState { system, state, .. } => (
                callbacks.state_change,
                vec![Value::Int(0), Value::string(&system), Value::string(&state)],
            ),
            ClientMessage::Exploder(id) if id < 0 => {
                (callbacks.deactivate_exploder, vec![Value::Int(-id)])
            }
            ClientMessage::Exploder(id) => (callbacks.activate_exploder, vec![Value::Int(id)]),
            // Black Ops calls it for the first person, never quiet.
            ClientMessage::Footstep(kind, surface) => {
                let player = world.resource::<ClientView>().locals[0].object;
                (
                    callbacks.footstep,
                    vec![
                        Value::Int(0),
                        Value::Object(player),
                        Value::string(kind),
                        Value::string(surface),
                        Value::Int(1),
                        Value::Int(0),
                    ],
                )
            }
        };
        if let Err(fault) = super::runtime::start(world, function, Value::level(), args) {
            diag::warn!(Sim, "csc: {function} not started: {fault}");
        }
    }
    super::runtime::advance_scheduler(world);
    world.remove_resource::<crate::step::StepRequest>();
    if let Some(fault) = world.resource::<RoundScript>().fault.clone() {
        diag::warn!(Sim, "csc: client scripts stopped: {fault}");
        client.stopped = true;
    }
}
