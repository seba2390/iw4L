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

/// A sound the client plays itself (`playsound`).
#[derive(Clone, Debug)]
pub(crate) struct ClientSound {
    pub(crate) alias: String,
    pub(crate) origin: [f32; 3],
}

/// What the client VM's frame asks of the client: sounds to play.
#[derive(Resource, Default)]
pub(crate) struct ClientOutput {
    pub(crate) sounds: Vec<ClientSound>,
    /// The next sound's id, as `playsound` answers it.
    pub(crate) next_sound: i32,
}

/// The client VM: its world, and the entries it starts with once the
/// client has a snapshot.
#[derive(Resource)]
pub(crate) struct ClientScripts {
    world: World,
    entries: Vec<String>,
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
    if !client.stopped {
        let (tick, reason) = {
            let request = server.resource::<crate::step::StepRequest>();
            (request.tick, request.reason)
        };
        if reason.advances_authority_world() {
            run_frame(&mut client, tick, reason);
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

fn run_frame(client: &mut ClientScripts, tick: crate::Tick, reason: crate::StepReason) {
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
    super::runtime::advance_scheduler(world);
    world.remove_resource::<crate::step::StepRequest>();
    if let Some(fault) = world.resource::<RoundScript>().fault.clone() {
        diag::warn!(Sim, "csc: client scripts stopped: {fault}");
        client.stopped = true;
    }
}
