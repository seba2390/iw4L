use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::script::host;
use crate::script::{
    Fault, LevelData, Location, NativeRegistry, Program, RoundScript, Thread, ThreadState, Value,
};

use super::{INSTRUCTION_BUDGET, execute, new_thread};

/// Runs during install, before the map's `script_struct` blocks are appended to `level.struct`.
pub const STRUCT_INIT: &str = "codescripts/struct::initstructs";
/// Black Ops 3's engine hands each map `script_struct` to this function
/// (`struct::CreateStruct`, hash 0x4c6908e2; its executable looks the hash up).
pub const CREATE_STRUCT: &str = "scripts/codescripts/struct::#4c6908e2";

pub(crate) fn copy_state(source: &World, target: &mut World) {
    target.insert_resource(source.resource::<RoundScript>().clone());
    target.insert_resource(source.resource::<crate::script::MatchScript>().clone());
    target.insert_resource(source.resource::<host::mechanics::Mechanics>().clone());
    target.insert_resource(source.resource::<NativeRegistry>().clone());
    for entity in source.iter_entities() {
        if let Some(thread) = entity.get::<Thread>() {
            target.spawn(thread.clone());
        }
    }
}

pub(crate) fn reset_round(world: &mut World) {
    let ids: Vec<_> = world
        .query_filtered::<Entity, bevy_ecs::query::With<Thread>>()
        .iter(world)
        .collect();
    for id in ids {
        world.despawn(id);
    }
    let round = RoundScript::new(world.resource::<crate::script::MatchScript>());
    world.insert_resource(round);
    world.insert_resource(host::mechanics::Mechanics::default());
    world.remove_resource::<host::actor_nav::LinkEdits>();
}

pub(crate) fn reset(world: &mut World) {
    world.insert_resource(crate::script::MatchScript::default());
    reset_round(world);
}

pub(crate) fn install(
    world: &mut World,
    program: Program,
    natives: NativeRegistry,
    level: LevelData,
) -> Result<(), Fault> {
    let plan = host::restart::RestartPlan {
        absent_effects: level.absent_effects,
        natives: natives.clone(),
        entities: Arc::new(level.entities),
        tables: Arc::new(level.tables),
        keys: Arc::new(level.keys),
        entity_fields: Arc::new(level.entity_fields),
        layouts: level.layouts,
        barriers: level.barriers,
        weapons: level.weapons,
        raw_files: level.raw_files,
        entries: Vec::new(),
        schemas: level.schemas,
        player_data_defaults: level.player_data_defaults.map(Arc::new),
        primary_lights: Arc::new(level.primary_lights),
        dyn_ents: Arc::new(level.dyn_ents),
    };
    install_level(world, Arc::new(program), plan)
}

/// Installs a client VM's program in its own world: its program, natives
/// and the map's structs and entities as the client knows them; no
/// persistent data.
pub(crate) fn install_client(
    world: &mut World,
    program: Arc<Program>,
    natives: NativeRegistry,
    entities: &[Vec<(String, String)>],
    keys: &BTreeMap<String, host::entities::KeyType>,
    entity_fields: Option<&BTreeMap<String, host::entities::KeyType>>,
) -> Result<(), Fault> {
    let location = Location {
        module: "<runtime>".into(),
        function: "install_client".into(),
        line: 0,
        column: 0,
    };
    let bound = program
        .natives
        .iter()
        .map(|b| {
            natives
                .get(b.namespace, b.name)
                .ok_or_else(|| Fault::at(&location, format!("unbound native {}", b.name)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.client = true;
    runtime.program = Some(program.clone());
    runtime.natives = bound;
    runtime.objects.insert(0, BTreeMap::new());
    runtime.objects.insert(1, BTreeMap::new());
    runtime.objects.insert(2, BTreeMap::new());
    runtime.next_object = 3;
    runtime.next_entity_number = 0x400;
    runtime.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    world.insert_resource(natives);
    if let Some(&function) = program.names.get(STRUCT_INIT) {
        let mut thread = new_thread(world, &program, function, Value::Object(0), Vec::new())
            .map_err(|m| Fault::at(&location, m))?;
        world.resource_mut::<RoundScript>().budget = INSTRUCTION_BUDGET;
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, 0);
        if let Some(fault) = world.resource::<RoundScript>().fault.clone() {
            return Err(fault);
        }
        world.resource_mut::<RoundScript>().started = false;
    }
    world
        .resource_mut::<RoundScript>()
        .spawn_map_entities(entities, keys, entity_fields)
        .map_err(|m| Fault::at(&location, m))?;
    Ok(())
}

pub(crate) fn install_level(
    world: &mut World,
    program: Arc<Program>,
    plan: host::restart::RestartPlan,
) -> Result<(), Fault> {
    let natives = plan.natives.clone();
    let location = Location {
        module: "<runtime>".into(),
        function: "install".into(),
        line: 0,
        column: 0,
    };
    if world.resource::<RoundScript>().program.is_some() {
        return Err(Fault::at(
            &location,
            "program already installed; reset the match before replacing scripts",
        ));
    }
    let mut unbound: Vec<_> = program
        .natives
        .iter()
        .filter(|b| natives.get(b.namespace, b.name).is_none())
        .map(|b| b.name)
        .collect();
    unbound.sort_unstable();
    if !unbound.is_empty() {
        return Err(Fault::at(
            &location,
            format!("{} unbound natives: {}", unbound.len(), unbound.join(" ")),
        ));
    }
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_schemas(plan.schemas.clone())
        .map_err(|error| Fault::at(&location, format!("persistent data schema: {error:?}")))?;
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_defaults(plan.player_data_defaults.clone());
    let bound = program
        .natives
        .iter()
        .map(|b| natives.get(b.namespace, b.name).unwrap())
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.program = Some(program.clone());
    runtime.natives = bound;
    runtime.objects.insert(0, BTreeMap::new());
    runtime.objects.insert(1, BTreeMap::new());
    runtime.objects.insert(2, BTreeMap::new());
    runtime.next_object = 3;
    runtime.next_entity_number = playerstate_iw4::GENTITY_SPAWN_BASE;
    runtime.tables = plan.tables.clone();
    runtime.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    runtime.loading = true;
    world.insert_resource(natives);
    if let Some(&function) = program.names.get(STRUCT_INIT) {
        let mut thread = new_thread(world, &program, function, Value::Object(0), Vec::new())
            .map_err(|m| Fault::at(&location, m))?;
        world.resource_mut::<RoundScript>().budget = INSTRUCTION_BUDGET;
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, 0);
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(fault) = runtime.fault.clone() {
            return Err(fault);
        }
        if thread.state != ThreadState::Complete {
            return Err(Fault::at(&location, format!("{STRUCT_INIT} must not wait")));
        }
        runtime.started = false;
    }
    let entities = plan.entities.clone();
    let keys = plan.keys.clone();
    let entity_fields = plan.entity_fields.clone();
    world.resource_mut::<RoundScript>().restart = Some(Arc::new(plan));
    let handed = world
        .resource_mut::<RoundScript>()
        .spawn_map_entities(&entities, &keys, entity_fields.as_ref().as_ref())
        .map_err(|m| Fault::at(&location, m))?;
    if let Some(&function) = program.names.get(CREATE_STRUCT) {
        for id in handed {
            let mut thread = new_thread(
                world,
                &program,
                function,
                Value::Object(0),
                vec![Value::Object(id)],
            )
            .map_err(|m| Fault::at(&location, m))?;
            world.resource_mut::<RoundScript>().budget = INSTRUCTION_BUDGET;
            thread.state = ThreadState::Runnable;
            execute(world, &program, &mut thread, 0);
            let runtime = world.resource::<RoundScript>();
            if let Some(fault) = runtime.fault.clone() {
                return Err(fault);
            }
            if thread.state != ThreadState::Complete {
                return Err(Fault::at(
                    &location,
                    format!("{CREATE_STRUCT} must not wait"),
                ));
            }
        }
        world.resource_mut::<RoundScript>().started = false;
    }
    host::natives::t5_turret::spawn_map_turrets(world);
    host::presence::initialize_map_models(world);
    host::natives::t6::init_actor_spawners(world)?;
    host::actors::run_aitype_spawners(world)?;
    Ok(())
}
