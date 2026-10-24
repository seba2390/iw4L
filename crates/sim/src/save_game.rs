//! Black Ops' saved games, kept in memory as its zombies mode keeps them: the
//! whole match at the end of the frame a script asked for the save in, put
//! back in place of the level when `loadgame_continue` loads it again.
//!
//! A save is the server's match: its state (without the level's content),
//! its players, missiles, movers and items, every script thread and the
//! script state, the match's dvars and the engine's turret, sentient,
//! mechanics, path-link and screen-overlay state. The client's scripts are not in it: a
//! loaded level starts them afresh, as Black Ops restarts its client.

use std::sync::{Arc, Mutex};

use bevy_ecs::prelude::{Entity, Resource, World};

use crate::frame::{
    PlayerRows, clear_payload_rows, collect_dropped_items, collect_players, collect_projectiles,
    collect_script_movers, spawn_payload_rows, state_entity,
};
use crate::script::host::actor_nav::LinkEdits;
use crate::script::host::mechanics::Mechanics;
use crate::script::host::natives::t5_turret::{Sentients, Turrets};
use crate::script::host::natives::t6::flame_overlay::FlameOverlays;
use crate::script::{MatchScript, RoundScript, Thread};
use crate::world::{SimState, Tick};

/// The code callback a loaded save runs on the level.
const SAVE_RESTORED: &str = "maps/mp/gametypes/_callbacksetup::codecallback_saverestored";

/// How long after a load `isSaveRecentlyLoaded` answers 1, in milliseconds
/// of the system clock.
const RECENTLY_LOADED_MS: u128 = 2000;

pub(crate) struct SavedGame {
    /// `mapname` when the save was made.
    map: String,
    id: i32,
    tick: Tick,
    sim: SimState,
    players: PlayerRows,
    projectiles: Vec<crate::ProjectileState>,
    movers: Vec<crate::gentity::ScriptMoverGentity>,
    items: Vec<crate::item::DroppedItem>,
    threads: Vec<Thread>,
    round: RoundScript,
    match_script: MatchScript,
    turrets: Option<Turrets>,
    sentients: Option<Sentients>,
    mechanics: Option<Mechanics>,
    link_edits: Option<LinkEdits>,
    flame_overlays: Option<FlameOverlays>,
}

/// The engine's saves, which outlive a level load as Black Ops' in-memory
/// saves do.
#[derive(Default)]
struct Saves {
    /// The save a load restores: the last one committed.
    current: Option<Arc<SavedGame>>,
    /// `SaveGameNoCommit`'s save, until `CommitSave` takes it.
    uncommitted: Option<Arc<SavedGame>>,
    next_id: i32,
    /// `loadgame_continue` asked for the current save.
    load: bool,
}

static SAVES: Mutex<Saves> = Mutex::new(Saves {
    current: None,
    uncommitted: None,
    next_id: 0,
    load: false,
});

fn saves() -> std::sync::MutexGuard<'static, Saves> {
    SAVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A save asked for this frame, written at its end.
#[derive(Resource)]
struct SaveRequest {
    id: i32,
    name: String,
    commit: bool,
    /// The save has been written (`isSaveSuccessful`).
    written: bool,
}

/// When a save was loaded, by the system clock.
#[derive(Resource)]
struct SaveLoaded {
    at: std::time::Instant,
}

fn map_name(world: &World) -> String {
    world
        .resource::<MatchScript>()
        .dvars
        .get("mapname")
        .cloned()
        .unwrap_or_else(String::new)
}

/// `SaveGame` / `SaveGameNoCommit`: asks for a save at the end of this frame
/// under `<mapname>` ("levelstart") or `<mapname>-<name>` ("" is "auto"),
/// and answers its id.
pub(crate) fn request(world: &mut World, name: &str, commit: bool) -> Result<i32, String> {
    if world
        .get_resource::<SaveRequest>()
        .is_some_and(|request| !request.written)
    {
        return Err("Attempting to save multiple times in a single frame.\n".into());
    }
    let runtime = world.resource::<RoundScript>();
    if runtime.reloading || runtime.pending_restart.is_some() {
        return Err("Attempting to save during a restart.\n".into());
    }
    let map = map_name(world);
    let name = match name {
        "" => format!("{map}-auto"),
        "levelstart" => map,
        name => format!("{map}-{name}"),
    };
    let id = {
        let mut saves = saves();
        saves.next_id = saves.next_id.wrapping_add(1);
        saves.next_id
    };
    world.insert_resource(SaveRequest {
        id,
        name,
        commit,
        written: false,
    });
    Ok(id)
}

/// `CommitSave(id)`: makes `SaveGameNoCommit`'s save the one a load
/// restores.
pub(crate) fn commit(id: i32) -> Result<(), String> {
    let mut saves = saves();
    let Some(uncommitted) = saves.uncommitted.as_ref() else {
        if saves.current.as_ref().is_some_and(|save| save.id == id) {
            return Ok(());
        }
        return Err("Save Commit attempted with no valid committable save available".into());
    };
    if uncommitted.id != id {
        return Err(format!(
            "Save Commit attempted on incorrect save currentsave:{id}, commit:{}",
            uncommitted.id
        ));
    }
    saves.current = saves.uncommitted.take();
    Ok(())
}

/// `IsSaveSuccessful`: the last save asked for has been written.
pub(crate) fn successful(world: &World) -> bool {
    match world.get_resource::<SaveRequest>() {
        Some(request) => request.written,
        None => saves().current.is_some(),
    }
}

/// `IsSaveRecentlyLoaded`: under two seconds of the system clock since a
/// load.
pub(crate) fn recently_loaded(world: &World) -> bool {
    world
        .get_resource::<SaveLoaded>()
        .is_some_and(|loaded| loaded.at.elapsed().as_millis() < RECENTLY_LOADED_MS)
}

/// The end of a frame: writes the save asked for in it.
pub(crate) fn write_requested(world: &mut World) {
    let Some(request) = world.get_resource::<SaveRequest>() else {
        return;
    };
    if request.written {
        world.remove_resource::<SaveRequest>();
        return;
    }
    let (id, name, commit) = (request.id, request.name.clone(), request.commit);
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let threads = world
        .query::<&Thread>()
        .iter(world)
        .cloned()
        .collect::<Vec<_>>();
    note_unsaved(world);
    let save = Arc::new(SavedGame {
        map: map_name(world),
        id,
        tick,
        sim: world
            .get::<SimState>(state_entity(world))
            .expect("the match has its state")
            .save_copy(),
        players: collect_players(world),
        projectiles: collect_projectiles(world),
        movers: collect_script_movers(world),
        items: collect_dropped_items(world),
        threads,
        round: world.resource::<RoundScript>().clone(),
        match_script: world.resource::<MatchScript>().clone(),
        turrets: world.get_resource::<Turrets>().cloned(),
        sentients: world.get_resource::<Sentients>().cloned(),
        mechanics: world.get_resource::<Mechanics>().cloned(),
        link_edits: world.get_resource::<LinkEdits>().cloned(),
        flame_overlays: world.get_resource::<FlameOverlays>().cloned(),
    });
    diag::info!(
        Sim,
        "save: `{name}` (id {id}) at tick {}: {} threads, {} movers{}",
        tick.0,
        save.threads.len(),
        save.movers.len(),
        if commit { "" } else { ", not committed" }
    );
    {
        let mut saves = saves();
        if commit {
            saves.current = Some(save);
        } else {
            saves.uncommitted = Some(save);
        }
    }
    if let Some(mut request) = world.get_resource_mut::<SaveRequest>() {
        request.written = true;
    }
}

/// Resources of the match that a save holds, or that are not the match's
/// own (the level's content, the engine's plumbing, the client's scripts).
const ACCOUNTED: &[&str] = &[
    "RoundScript",
    "MatchScript",
    "Turrets",
    "Sentients",
    "Mechanics",
    "LinkEdits",
    "StepRequest",
    "NativeRegistry",
    "Schedules",
    "SimStateEntity",
    "ReportedGameGaps",
    "TestClients",
    "PersistentDataStore",
    "LocalPlayerProfile",
    "FlameOverlays",
    "GametypeSettings",
    "ClientScripts",
    "ClientPresentation",
    "ThreadIndex",
    "SaveRequest",
    "SaveLoaded",
    "DefaultQueryFilters",
];

/// Logs each resource of the match a save does not hold.
fn note_unsaved(world: &World) {
    for (_, info, _) in world.iter_resources() {
        let name = format!("{}", info.name());
        let short = name.rsplit("::").next().unwrap_or(&name);
        if !ACCOUNTED.contains(&short) {
            diag::warn!(Sim, "save: `{name}` is not kept in the save");
        }
    }
}

/// `loadgame_continue`: the current save is loaded with the level, when
/// there is one.
pub(crate) fn request_load() -> bool {
    let mut saves = saves();
    saves.load = saves.current.is_some();
    saves.load
}

/// Puts the save `loadgame_continue` asked for in place of this freshly
/// loaded level, when it is this level's: answers the tick it was made at.
pub(crate) fn load_requested(world: &mut World) -> Option<Tick> {
    let save = {
        let mut saves = saves();
        if !std::mem::take(&mut saves.load) {
            return None;
        }
        saves.current.clone()?
    };
    let map = map_name(world);
    if save.map != map {
        diag::warn!(
            Sim,
            "save: `{}` is not this level's (`{map}`); the level starts afresh",
            save.map
        );
        return None;
    }
    let runtime = world.resource::<RoundScript>();
    if runtime.program_fingerprint() != save.round.program_fingerprint() {
        diag::warn!(
            Sim,
            "save: the level's scripts are not the ones the save ran; the level starts afresh"
        );
        return None;
    }
    restore(world, &save);
    Some(save.tick)
}

fn restore(world: &mut World, save: &SavedGame) {
    let state = state_entity(world);
    world
        .get_mut::<SimState>(state)
        .expect("the match has its state")
        .restore_saved(save.sim.clone());
    clear_payload_rows(world);
    spawn_payload_rows(
        world,
        save.players.clone(),
        save.projectiles.clone(),
        save.movers.clone(),
        save.items.clone(),
    );
    let threads: Vec<Entity> = world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    for entity in threads {
        world.despawn(entity);
    }
    for thread in &save.threads {
        world.spawn(thread.clone());
    }
    crate::script::forget_thread_index(world);
    world
        .resource_mut::<RoundScript>()
        .restore_saved(save.round.clone());
    world.insert_resource(save.match_script.clone());
    restore_resource(world, save.turrets.clone());
    restore_resource(world, save.sentients.clone());
    restore_resource(world, save.mechanics.clone());
    restore_resource(world, save.link_edits.clone());
    restore_resource(world, save.flame_overlays.clone());
    world.insert_resource(SaveLoaded {
        at: std::time::Instant::now(),
    });
    world.remove_resource::<SaveRequest>();
    if let Err(fault) = crate::script::start(
        world,
        SAVE_RESTORED,
        crate::script::Value::level(),
        Vec::new(),
    ) {
        diag::warn!(Sim, "save: {SAVE_RESTORED} did not start: {fault}");
    }
    diag::info!(
        Sim,
        "save: `{}` (id {}) loaded at tick {}: {} threads, {} movers",
        save.map,
        save.id,
        save.tick.0,
        save.threads.len(),
        save.movers.len()
    );
}

fn restore_resource<R: Resource>(world: &mut World, saved: Option<R>) {
    match saved {
        Some(resource) => world.insert_resource(resource),
        None => {
            world.remove_resource::<R>();
        }
    }
}
