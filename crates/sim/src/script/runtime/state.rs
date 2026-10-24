use bevy_ecs::prelude::Resource;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::script::host;
use crate::script::{ArrayKey, Fault, Native, Program, StringTable, Value};

#[derive(Resource, Clone, Debug)]
pub(crate) struct RoundScript {
    pub(crate) program: Option<Arc<Program>>,
    pub(crate) natives: Vec<Native>,
    pub(crate) next_serial: u64,
    pub fault: Option<Fault>,
    pub(crate) fault_reported: bool,
    pub(crate) errors: BTreeMap<(String, String), u64>,
    /// Deleted entities scripts may still hold. They keep their fields until
    /// the end of the frame that deleted them, then read as undefined.
    pub(crate) dead: std::collections::BTreeSet<u64>,
    pub(crate) dying: Vec<u64>,
    pub(crate) last_tick: Option<crate::Tick>,
    pub(crate) started: bool,
    pub(crate) t6_match_started: Option<crate::Tick>,
    pub(crate) t6_match_ended: Option<crate::Tick>,
    pub(crate) zombies: host::t6_zombies::Survival,
    pub(crate) objects: BTreeMap<u64, BTreeMap<u32, Value>>,
    pub(crate) next_object: u64,
    /// Black Ops 3's `world` and `classes` objects, made the first time a
    /// script names them so other games' object numbers stay as they are.
    pub(crate) world_object: Option<u64>,
    pub(crate) classes_object: Option<u64>,
    pub(crate) arrays: BTreeMap<u64, BTreeMap<ArrayKey, Value>>,
    pub(crate) array_aliases: super::aliases::ArrayAliases,
    pub(crate) dynamic_symbols: BTreeMap<Arc<str>, u32>,
    /// `symbol`'s answers by name, hashed: the program's table is ordered
    /// by string, which costs a run of string compares per lookup.
    symbol_cache: SymbolCache,
    pub(crate) buckets: BTreeMap<i64, VecDeque<u64>>,
    pub(crate) spawned: Vec<u64>,
    pub(crate) waiters: Vec<crate::script::Waiter>,
    pub(crate) loading: bool,
    pub(crate) presented: BTreeMap<&'static str, Vec<Value>>,
    pub(crate) unsupported: BTreeMap<&'static str, u64>,
    pub(crate) budget: usize,
    pub(crate) suspended: Vec<u64>,
    pub(crate) suspended_frames: usize,
    /// Endons that fired on a suspended thread; applied when its child yields.
    pub(crate) pending_unwinds: Vec<(u64, usize)>,
    pub(crate) entities: BTreeMap<u64, host::entities::ScriptEntity>,
    pub(crate) actor_anims: BTreeMap<u64, host::actor_anims::ActorAnim>,
    pub(crate) actor_brains: BTreeMap<u64, host::actor_brain::ActorBrain>,
    pub(crate) actor_moves: BTreeMap<u64, host::actor_nav::ActorMove>,
    /// Each enemy's hellhound attack spots, by the dogs holding them.
    pub(crate) dog_spots: BTreeMap<u64, host::actor_dog::AttackSpots>,
    /// When each actor spawner last spawned (ms); one spawn a frame.
    pub(crate) spawner_times: BTreeMap<u64, i64>,
    /// `setailimit`: no spawn while this many actors are in use (0: none).
    pub(crate) ai_limit: i32,
    /// Script objects standing for path nodes, by node index.
    pub(crate) path_node_objects: BTreeMap<u16, u64>,
    pub(crate) hud_slots: BTreeMap<u64, usize>,
    pub(crate) next_entity_number: i32,
    /// The numbers the entity pool held when last seen (a superset of what
    /// it holds now), which `create_entity` passes over.
    pub(crate) pool_numbers: std::collections::BTreeSet<i32>,
    pub(crate) tables: Arc<BTreeMap<String, StringTable>>,
    pub(crate) rng: u32,
    pub(crate) pending_notifies: Vec<(Value, Arc<str>, Vec<Value>)>,
    pub(crate) signals: Vec<Arc<str>>,
    pub(crate) engine: host::entities::EngineState,
    pub(crate) players: BTreeMap<u32, host::players::PlayerSlot>,
    pub(crate) menu_answers: BTreeMap<u32, VecDeque<host::players::MenuAnswer>>,
    pub(crate) team_ranks: BTreeMap<u32, (i32, i32)>,
    pub(crate) ranks_sent: bool,
    pub(crate) selected_classes: BTreeMap<u32, u32>,
    pub(crate) thrown_insertions: BTreeMap<u32, (Arc<str>, u64)>,
    pub(crate) insertion_spots: Vec<([f32; 3], Arc<str>)>,
    pub(crate) disconnects: std::collections::BTreeSet<u32>,
    pub(crate) kicks: BTreeMap<u32, String>,
    pub(crate) joined: std::collections::BTreeSet<u32>,
    pub(crate) current_hit: Option<crate::script_player::Hit>,
    pub(crate) deaths: VecDeque<(u32, &'static str, Vec<Value>)>,
    pub(crate) exit_level: bool,
    /// A failed level is on its way to loading again.
    pub(crate) reloading: bool,
    /// The match time after which a failed level loads again.
    pub(crate) reload_after_ms: Option<i64>,
    /// `ChangeLevel`: the level to load and the match time it loads at.
    pub(crate) change_level: Option<(String, i64)>,
    pub(crate) shown: BTreeMap<u64, host::presence::Shown>,
    pub(crate) retired_presence: Vec<(crate::ScriptModelId, bool)>,
    /// Map models the server leaves to the client (Black Ops' client-only
    /// map entities): never drawn or solid in the server's world.
    pub(crate) withheld_presence: Vec<crate::ScriptModelId>,
    pub(crate) blasts: Vec<host::entity_damage::ScriptBlast>,
    pub(crate) hits: Vec<host::entity_damage::ScriptHit>,
    pub(crate) use_held: std::collections::BTreeSet<u32>,
    pub(crate) fired_once: std::collections::BTreeSet<u64>,
    pub(crate) require_look_at: std::collections::BTreeSet<u64>,
    pub(crate) server_info: std::collections::BTreeSet<String>,
    pub(crate) server_info_defaults: BTreeMap<String, String>,
    pub(crate) missiles: BTreeMap<crate::ProjectileId, u64>,
    pub(crate) grenade_touches: Vec<host::triggers::GrenadeTouch>,
    pub(crate) lingering: Vec<(i64, u64)>,
    /// Lobby members the match waits for before play starts.
    pub(crate) expected_players: usize,
    /// A client VM's runtime (Black Ops' client scripts), not the server's.
    pub(crate) client: bool,
    /// Notifies the engine raises later (a sound or animation done): when,
    /// on what, which notify with which arguments.
    pub(crate) timed_notifies: Vec<(i64, Value, std::sync::Arc<str>, Vec<Value>)>,
    /// Black Ops' `playSound(alias, notify)` notifies, by entity number.
    pub(crate) sound_notifies: BTreeMap<i32, SoundNotify>,
    /// Clients' reports of those sounds' lengths (entity number, msec), for
    /// the next step.
    pub(crate) sound_lengths: Vec<(i32, i32)>,
    retired_items: Vec<u64>,
    finishing: std::collections::BTreeSet<u64>,
    finishing_order: Vec<u64>,
    pub(crate) vehicles: BTreeMap<u64, host::vehicles::Heli>,
    pub(crate) planes: BTreeMap<u64, host::vehicles::Plane>,
    pub(crate) use_selected: BTreeMap<u32, u64>,
    pub(crate) t5: host::natives::t5::T5State,
    /// What the server's scripts send its clients' scripts this frame.
    pub(crate) client_messages: Vec<crate::script::client::ClientMessage>,
    pub(crate) restart: Option<Arc<host::restart::RestartPlan>>,
    pub(crate) finished: bool,
    pub(crate) pending_restart: Option<bool>,
    pub(crate) restored_pers: BTreeMap<u32, host::restart::Detached>,
}

impl RoundScript {
    pub(crate) fn new(_: &MatchScript) -> Self {
        Self {
            program: Default::default(),
            natives: Default::default(),
            next_serial: Default::default(),
            fault: Default::default(),
            fault_reported: Default::default(),
            errors: Default::default(),
            dead: Default::default(),
            dying: Default::default(),
            last_tick: Default::default(),
            started: Default::default(),
            t6_match_started: Default::default(),
            t6_match_ended: Default::default(),
            zombies: Default::default(),
            objects: Default::default(),
            next_object: Default::default(),
            world_object: Default::default(),
            classes_object: Default::default(),
            arrays: Default::default(),
            array_aliases: Default::default(),
            dynamic_symbols: Default::default(),
            symbol_cache: Default::default(),
            buckets: Default::default(),
            spawned: Default::default(),
            waiters: Default::default(),
            loading: Default::default(),
            presented: Default::default(),
            unsupported: Default::default(),
            budget: Default::default(),
            suspended: Default::default(),
            suspended_frames: Default::default(),
            pending_unwinds: Default::default(),
            entities: Default::default(),
            actor_anims: Default::default(),
            actor_brains: Default::default(),
            actor_moves: Default::default(),
            dog_spots: Default::default(),
            spawner_times: Default::default(),
            ai_limit: Default::default(),
            path_node_objects: Default::default(),
            hud_slots: Default::default(),
            pool_numbers: Default::default(),
            next_entity_number: Default::default(),
            tables: Default::default(),
            rng: Default::default(),
            pending_notifies: Default::default(),
            signals: Default::default(),
            engine: Default::default(),
            players: Default::default(),
            menu_answers: Default::default(),
            team_ranks: Default::default(),
            ranks_sent: Default::default(),
            selected_classes: Default::default(),
            thrown_insertions: Default::default(),
            insertion_spots: Default::default(),
            disconnects: Default::default(),
            kicks: Default::default(),
            joined: Default::default(),
            current_hit: Default::default(),
            deaths: Default::default(),
            exit_level: Default::default(),
            reloading: Default::default(),
            reload_after_ms: Default::default(),
            change_level: None,
            shown: Default::default(),
            retired_presence: Default::default(),
            withheld_presence: Default::default(),
            blasts: Default::default(),
            hits: Default::default(),
            use_held: Default::default(),
            fired_once: Default::default(),
            require_look_at: Default::default(),
            server_info: Default::default(),
            server_info_defaults: Default::default(),
            missiles: Default::default(),
            grenade_touches: Default::default(),
            lingering: Default::default(),
            expected_players: Default::default(),
            client: Default::default(),
            timed_notifies: Default::default(),
            sound_notifies: Default::default(),
            sound_lengths: Default::default(),
            retired_items: Default::default(),
            finishing: Default::default(),
            finishing_order: Default::default(),
            vehicles: Default::default(),
            planes: Default::default(),
            use_selected: Default::default(),
            t5: Default::default(),
            client_messages: Default::default(),
            restart: Default::default(),
            finished: Default::default(),
            pending_restart: Default::default(),
            restored_pers: Default::default(),
        }
    }

    pub(crate) fn program_fingerprint(&self) -> Option<[u8; 32]> {
        self.program.as_ref().map(|program| program.fingerprint())
    }

    pub(crate) fn live(&self, id: &u64) -> bool {
        self.objects.contains_key(id) && !self.dead.contains(id)
    }

    pub(crate) fn script_is_defined(&self, id: &u64) -> bool {
        self.live(id) && !self.finishing.contains(id)
    }

    pub(crate) fn can_receive_call(&self, id: &u64) -> bool {
        self.script_is_defined(id) && !self.retired_items.contains(id)
    }

    pub(crate) fn request_delete(&mut self, id: u64) -> bool {
        if !self.live(&id) || !self.finishing.insert(id) {
            return false;
        }
        self.finishing_order.push(id);
        true
    }

    pub(crate) fn retire_item_payload(&mut self, number: i32) {
        self.retired_items
            .extend(self.entities.iter().filter_map(|(id, entity)| {
                matches!(entity.kind, host::entities::EntityKind::Item(at) if at == number)
                    .then_some(*id)
            }));
    }

    pub(crate) fn take_retired_items(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.retired_items)
    }

    pub(crate) fn take_finishing(&mut self) -> Vec<u64> {
        self.finishing.clear();
        std::mem::take(&mut self.finishing_order)
    }

    pub(crate) fn request_round_restart(&mut self, persist: bool) -> Result<(), String> {
        if self.program.is_none() || !self.started || self.fault.is_some() {
            return Err("no healthy started match scripts".into());
        }
        if self.finished {
            return Err("match end already requested".into());
        }
        self.finished = true;
        self.pending_restart = Some(persist);
        self.signals.push(crate::script::MAP_RESTART.into());
        Ok(())
    }

    pub(crate) fn symbol(&mut self, name: &str) -> u32 {
        let program = self.program.as_ref().unwrap();
        let cache = &mut self.symbol_cache;
        let key = Arc::as_ptr(program) as usize;
        if cache.program != key || cache.dynamic > self.dynamic_symbols.len() {
            cache.ids.clear();
            cache.program = key;
        }
        if let Some(&id) = cache.ids.get(name) {
            return id;
        }
        let id = match program.symbol_ids.get(name) {
            Some(&id) => id,
            None => {
                let next = (program.symbols.len() + self.dynamic_symbols.len()) as u32;
                *self.dynamic_symbols.entry(name.into()).or_insert(next)
            }
        };
        cache.dynamic = self.dynamic_symbols.len();
        cache.ids.insert(name.into(), id);
        id
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct MatchScript {
    pub(crate) precached: BTreeMap<(&'static str, String), i32>,
    pub(crate) dvars: BTreeMap<String, String>,
    pub(crate) local_presentation_dvars: bool,
    pub(crate) local_presentation_client: Option<crate::ClientId>,
    pub(crate) pending_local_dvars: Vec<(crate::TargetBoxDvar, String)>,
    pub(crate) personal_classes: BTreeMap<(u32, u32), crate::ClassDef>,
    pub(crate) weapon_bridge: BTreeMap<u32, Vec<(u32, u32)>>,
    pub(crate) next_spawned_presence: u32,
    pub(crate) match_data: host::match_data::MatchData,
}

/// FNV-1a: short field names hash in a few cycles.
#[derive(Clone, Copy)]
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SymbolCache {
    /// The program the ids belong to (its address).
    program: usize,
    /// How many dynamic symbols there were when it last learned one.
    dynamic: usize,
    ids: std::collections::HashMap<Box<str>, u32, std::hash::BuildHasherDefault<Fnv>>,
}

/// A sound notify an entity waits on: raised once its sound has played
/// (`start_ms` plus the length the client reported; 5000 msec while none
/// has been reported).
#[derive(Clone, Debug)]
pub(crate) struct SoundNotify {
    pub(crate) receiver: Value,
    pub(crate) notify: std::sync::Arc<str>,
    pub(crate) start_ms: i64,
    pub(crate) duration_ms: i32,
}
