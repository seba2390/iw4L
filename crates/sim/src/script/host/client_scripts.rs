//! A local client's script VM (Black Ops 2's `.csc`). It runs in its own
//! runtime, with the client's builtins, after the server's scripts each
//! authority tick; what it reads of the match is the server's state as the
//! local client's snapshot carries it (`t6.csc.snapshot`).

use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

use crate::script::host::mechanics::Mechanics;
use crate::script::{Fault, Location, MatchScript, NativeRegistry, Program, RoundScript, Value};

/// The client's runtime, held by the server's world between ticks.
#[derive(Resource)]
pub(crate) struct ClientScripts {
    world: World,
    setup: game_api::ClientScriptProgram,
    started: bool,
    /// A fault that stopped the client's scripts.
    stopped: bool,
}

/// The client game's own callbacks, by role, as the client's world holds
/// them for its game.
#[derive(Resource, Clone, Default)]
pub(crate) struct ClientCallbacks {
    pub(crate) local_client_connect: Option<String>,
    pub(crate) player_spawned: Option<String>,
    pub(crate) entity_spawned: Option<String>,
    pub(crate) entity_shutdown: Option<String>,
    pub(crate) state_change: Option<String>,
    pub(crate) activate_exploder: Option<String>,
    pub(crate) deactivate_exploder: Option<String>,
}

/// The server's world while the client's scripts run: the client game reads
/// and plays through it. Present only inside `advance`, which holds the
/// server's world exclusively for the call.
#[derive(Resource)]
struct ServerWorld(*mut World);

// SAFETY: the pointer is only dereferenced on the thread that runs
// `advance`, while `advance` holds the server's world exclusively and does not
// touch it.
unsafe impl Send for ServerWorld {}
unsafe impl Sync for ServerWorld {}

/// Runs `f` on the server's world from inside the client's scripts.
pub(crate) fn with_server<R>(client: &World, f: impl FnOnce(&mut World) -> R) -> Option<R> {
    let pointer = client.get_resource::<ServerWorld>()?.0;
    // SAFETY: see `ServerWorld`; the client's world is a separate world, and
    // nothing else reaches the server's world until `advance` returns.
    Some(f(unsafe { &mut *pointer }))
}

/// Runs `f` on the server's world and the client's together.
pub(crate) fn with_both<R>(
    client: &mut World,
    f: impl FnOnce(&mut World, &mut World) -> R,
) -> Option<R> {
    let pointer = client.get_resource::<ServerWorld>()?.0;
    // SAFETY: as `with_server`.
    Some(f(unsafe { &mut *pointer }, client))
}

/// Marks a runtime as a client's, for what it reports.
#[derive(Resource)]
pub(crate) struct ClientRuntime;

/// Reads the client's runtime when this world is one.
pub(crate) fn is_client(world: &World) -> bool {
    world.contains_resource::<ClientRuntime>()
}

fn location(function: &str) -> Location {
    Location {
        module: "<client>".into(),
        function: function.into(),
        line: 0,
        column: 0,
    }
}

/// Installs the client's program beside the server's; it starts on the
/// first authority tick.
pub(crate) fn install(
    server: &mut World,
    mut setup: game_api::ClientScriptProgram,
    natives: NativeRegistry,
) -> Result<(), Fault> {
    let rules = setup.program.rules;
    let program = std::mem::replace(&mut setup.program, empty_program(rules));
    let mut unbound: Vec<_> = program
        .natives
        .iter()
        .filter(|b| natives.get(b.namespace, b.name).is_none())
        .map(|b| b.name)
        .collect();
    unbound.sort_unstable();
    unbound.dedup();
    if !unbound.is_empty() {
        return Err(Fault::at(
            &location("install"),
            format!(
                "{} unbound client natives: {}",
                unbound.len(),
                unbound.join(" ")
            ),
        ));
    }
    let program = std::sync::Arc::new(program);
    let match_script = MatchScript::default();
    let mut round = RoundScript::new(&match_script);
    round.natives = program
        .natives
        .iter()
        .map(|b| natives.get(b.namespace, b.name).unwrap())
        .collect();
    round.program = Some(program.clone());
    round.objects.insert(0, BTreeMap::new());
    round.objects.insert(1, BTreeMap::new());
    round.objects.insert(2, BTreeMap::new());
    round.next_object = 3;
    round.next_entity_number = 18;
    round.tables = server.resource::<RoundScript>().tables.clone();
    round.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    round.loading = true;
    let mut world = World::new();
    world.insert_resource(match_script);
    world.insert_resource(round);
    world.insert_resource(Mechanics::default());
    world.insert_resource(natives);
    world.insert_resource(ClientRuntime);
    let callbacks = &setup.callbacks;
    world.insert_resource(ClientCallbacks {
        local_client_connect: callbacks.local_client_connect.clone(),
        player_spawned: callbacks.player_spawned.clone(),
        entity_spawned: callbacks.entity_spawned.clone(),
        entity_shutdown: callbacks.entity_shutdown.clone(),
        state_change: callbacks.state_change.clone(),
        activate_exploder: callbacks.activate_exploder.clone(),
        deactivate_exploder: callbacks.deactivate_exploder.clone(),
    });
    server.insert_resource(ClientScripts {
        world,
        setup,
        started: false,
        stopped: false,
    });
    Ok(())
}

fn empty_program(rules: asset_core::FamilyId) -> Program {
    Program {
        impure_scripts: false,
        functions: Vec::new(),
        names: BTreeMap::new(),
        modules: Vec::new(),
        symbols: Vec::new(),
        symbol_ids: BTreeMap::new(),
        natives: Vec::new(),
        rules,
        field_name: None,
        ordered_strings: true,
    }
}

/// Steps the client's scripts one authority tick, after the server's.
pub(crate) fn advance(world: &mut World) {
    let Some(request) = world.get_resource::<crate::step::StepRequest>() else {
        return;
    };
    if !request.reason.advances_authority_world() || !world.contains_resource::<ClientScripts>() {
        return;
    }
    let mirror = request.mirror();
    let now = i64::from(mirror.tick.0) * i64::from(crate::MATCH_TICK_MS);
    let mut client = world.remove_resource::<ClientScripts>().unwrap();
    if !client.stopped {
        // The client reads the same dvars as the server: one process.
        let dvars = world.resource::<MatchScript>().dvars.clone();
        client.world.resource_mut::<MatchScript>().dvars = dvars;
        client.world.insert_resource(mirror);
        client
            .world
            .insert_resource(ServerWorld(std::ptr::from_mut(world)));
        let result = step(&mut client, now);
        client.world.remove_resource::<ServerWorld>();
        client.world.remove_resource::<crate::step::StepRequest>();
        if let Err(fault) = result.and_then(|()| {
            client
                .world
                .resource::<RoundScript>()
                .fault
                .clone()
                .map_or(Ok(()), Err)
        }) {
            diag::warn!(Sim, "csc: client scripts stopped: {fault}");
            diag::script_boundary(
                "csc_stopped",
                &format!(" fault=\"{}\"", fault.message.replace('"', "'")),
            );
            client.stopped = true;
        }
    }
    world.insert_resource(client);
}

/// The client game's frame: its start on the first, then on each the
/// game's snapshot (`ClientSync`), then the client's scripts.
fn step(client: &mut ClientScripts, now: i64) -> Result<(), Fault> {
    if !client.started {
        client.started = true;
        start(client, now)?;
    } else if let Some(sync) = client.world.resource::<NativeRegistry>().client_sync() {
        sync(&mut client.world);
    }
    super::natives::engine::deliver_timed_notifies(&mut client.world);
    crate::script::advance_scheduler(&mut client.world);
    super::mechanics::advance_client_mechanics(&mut client.world, now);
    if let Some(present) = client.world.resource::<NativeRegistry>().client_present() {
        present(&mut client.world);
    }
    Ok(())
}

/// The client game's start: the level's structs, the actor types'
/// precaches, then the game's start functions, each run to its first wait.
fn start(client: &mut ClientScripts, now: i64) -> Result<(), Fault> {
    let world = &mut client.world;
    run_to_end(
        world,
        crate::script::runtime::STRUCT_INIT,
        Value::level(),
        Vec::new(),
        now,
    )?;
    let create = "codescripts/struct::createstruct";
    for pairs in &client.setup.structs {
        let made =
            crate::script::runtime::call_now(world, create, Value::level(), Vec::new(), now)?;
        let Value::Object(id) = made else {
            continue;
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        for (key, ty, value) in pairs {
            let value = super::entities::typed_key(game_key_type(*ty), value);
            runtime.set_object_field(id, key, value);
        }
    }
    for (name, index) in &client.setup.precaches {
        crate::script::runtime::run_now(
            world,
            name,
            Value::level(),
            vec![Value::Int(*index)],
            now,
        )?;
    }
    for name in &client.setup.starts {
        crate::script::runtime::run_now(world, name, Value::level(), Vec::new(), now)?;
    }
    world.resource_mut::<RoundScript>().started = true;
    Ok(())
}

fn run_to_end(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
    now: i64,
) -> Result<(), Fault> {
    if !world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|program| program.names.contains_key(name))
    {
        return Ok(());
    }
    crate::script::runtime::call_now(world, name, receiver, args, now).map(|_| ())
}

fn game_key_type(ty: game_api::KeyType) -> super::entities::KeyType {
    match ty {
        game_api::KeyType::Int => super::entities::KeyType::Int,
        game_api::KeyType::Float => super::entities::KeyType::Float,
        game_api::KeyType::String => super::entities::KeyType::String,
        game_api::KeyType::Vector => super::entities::KeyType::Vector,
    }
}
