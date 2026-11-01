use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

use crate::script::host::natives;
use crate::script::{Namespace, RoundScript, Value};

pub type Native = fn(&mut World, &Value, &[Value]) -> Result<Value, String>;
/// Engine work a game's natives do every authority tick, before scripts run.
pub type Tick = fn(&mut World);
/// A game's own damage for a script entity a shot or blast struck; true
/// when the game took it.
pub(crate) type EntityHitHook = fn(&mut World, u64, &super::entity_damage::EntityHit) -> bool;
/// A game's own use rule: for each player who pressed use (client, player
/// object), the entity their use reaches, as (entity, player object).
pub(crate) type UseRule = fn(&mut World, &[(u32, u64)]) -> Vec<(u64, u64)>;
/// A game's engine fields a player's entity has when it connects, set on
/// the player object before the connect callback runs.
pub(crate) type ConnectHook = fn(&mut World, u64);
/// A game's client game: what its client scripts see of the server's world
/// each tick, before they run (the client's world).
pub(crate) type ClientSync = fn(&mut World);

/// A game's own engine notifies for its players' missiles.
#[derive(Clone, Copy)]
pub(crate) struct MissileRule {
    /// A launched projectile's entity class, the notify its owner hears and
    /// the arguments after the entity and weapon name; `None` for none.
    pub launched: fn(&mut World, &crate::ProjectileState) -> Option<MissileLaunch>,
    /// A missile still flying (or resting) this tick, and whether it stuck.
    pub flying: fn(&mut World, u64, &crate::ProjectileState, bool),
    /// The arguments `explode` carries for a missile gone off there.
    pub exploded: fn(&mut World, u64, [f32; 3]) -> Vec<Value>,
    /// How long a grenade gone off stays before `death`; `None` for at once.
    pub linger_ms: Option<i64>,
}

pub(crate) struct MissileLaunch {
    pub classname: &'static str,
    pub notify: &'static str,
    pub extra: Vec<Value>,
}

#[derive(Resource, Clone)]
pub struct NativeRegistry(
    BTreeMap<(Namespace, String), Native>,
    Vec<Tick>,
    Vec<EntityHitHook>,
    Option<UseRule>,
    Vec<ConnectHook>,
    Option<MissileRule>,
    Option<ClientSync>,
    Option<ClientSync>,
);
impl Default for NativeRegistry {
    /// The engine services alone: no game's own natives.
    fn default() -> Self {
        Self::engine_services()
    }
}
impl NativeRegistry {
    /// No builtins at all.
    pub(crate) fn empty() -> Self {
        Self(
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            None,
        )
    }

    /// Black Ops' client VM natives (its client scripts, CSC).
    pub fn black_ops_client(server: &NativeRegistry) -> Self {
        Self::client(server, |registry| {
            natives::t5_client::register(registry);
            natives::t5_viewmodel::register_client(registry);
            natives::t5_perks::register_client(registry);
            natives::t5_scrambler::register_client(registry);
        })
    }

    /// Builtins every game's scripts are served the same way: the language
    /// helpers, dvars, math, entities, players, sound, physics, triggers.
    pub fn engine_services() -> Self {
        let mut registry = Self(
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        registry.register(Namespace::Function, "isdefined", |world, _, args| {
            if args.len() != 1 {
                return Err("isdefined expects one argument".into());
            }
            let defined = match &args[0] {
                Value::Undefined => false,
                Value::Object(id) => {
                    let runtime = world.resource::<RoundScript>();
                    runtime.script_is_defined(id)
                }
                _ => true,
            };
            Ok(Value::Int(i32::from(defined)))
        });
        registry.register(Namespace::Function, "gettime", |world, _, args| {
            if !args.is_empty() {
                return Err("gettime expects no arguments".into());
            }
            let tick = world.resource::<crate::step::StepRequest>().tick;
            let ms = u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS);
            i32::try_from(ms)
                .map(Value::Int)
                .map_err(|_| "gettime overflow".into())
        });
        registry.register(Namespace::Function, "spawnstruct", |world, _, args| {
            if !args.is_empty() {
                return Err("spawnstruct expects no arguments".into());
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            let id = runtime.next_object;
            runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
            runtime.objects.insert(id, BTreeMap::new());
            Ok(Value::Object(id))
        });
        natives::iw4::register(&mut registry);
        super::audio::register(&mut registry);
        natives::math::register(&mut registry);
        natives::shared::register(&mut registry);
        natives::engine::register(&mut registry);
        natives::player::register(&mut registry);
        super::weapons::register(&mut registry);
        super::physics::register(&mut registry);
        super::controls::register(&mut registry);
        super::client_effects::register(&mut registry);
        super::triggers::register(&mut registry);
        super::spectators::register(&mut registry);
        registry
    }

    /// Modern Warfare 2's game systems: ranks, the local profile,
    /// objectives, vehicles, missile guidance, turrets.
    pub fn with_mw2_systems(mut self) -> Self {
        natives::skill::register(&mut self);
        natives::local_profile::register(&mut self);
        super::objectives::register(&mut self);
        super::vehicles::register(&mut self);
        super::guidance::register(&mut self);
        super::turrets::register(&mut self);
        self
    }

    /// Black Ops' natives: its builtins, zombies, and AI actors.
    pub fn with_black_ops(mut self) -> Self {
        natives::t5::register(&mut self);
        natives::t5_zombie::register(&mut self);
        natives::t5_turret::register(&mut self);
        natives::t5_use::register(&mut self);
        natives::t5_viewmodel::register(&mut self);
        super::objectives::register(&mut self);
        super::actors::register(&mut self);
        super::actors::register_damage(&mut self);
        super::actor_anims::register(&mut self);
        super::actor_nav::register(&mut self);
        self
    }

    /// Black Ops multiplayer's natives where its server's differ from the
    /// zombies server's.
    pub fn with_black_ops_multiplayer(mut self) -> Self {
        natives::t5_actionslot::register(&mut self);
        natives::t5_classes::register(&mut self);
        natives::t5_location::register(&mut self);
        natives::t5_mp_turret::register(&mut self);
        natives::t5_napalm::register(&mut self);
        natives::t5_objectives::register(&mut self);
        natives::t5_obituary::register(&mut self);
        natives::t5_perks::register(&mut self);
        natives::t5_radar::register(&mut self);
        natives::t5_stats::register(&mut self);
        natives::t5_sprint::register(&mut self);
        natives::t5_targets::register(&mut self);
        self
    }
}
impl NativeRegistry {
    /// Black Ops 3's builtins alone: none of the engine services above, which
    /// serve the other games' rules.
    pub fn black_ops_2() -> Self {
        let mut registry = Self(
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        natives::t6::register(&mut registry);
        registry
    }

    /// Black Ops 2's client builtins, for its client scripts.
    pub fn black_ops_2_client() -> Self {
        let mut registry = Self(
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        natives::t6::client::register(&mut registry);
        registry
    }

    pub fn black_ops_3() -> Self {
        let mut registry = Self(
            BTreeMap::new(),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        natives::t7::register(&mut registry);
        registry
    }

    pub fn on_tick(&mut self, tick: Tick) {
        self.1.push(tick);
    }

    pub(crate) fn ticks(&self) -> Vec<Tick> {
        self.1.clone()
    }

    pub(crate) fn on_entity_hit(&mut self, hook: EntityHitHook) {
        self.2.push(hook);
    }

    pub(crate) fn entity_hit_hooks(&self) -> Vec<EntityHitHook> {
        self.2.clone()
    }

    /// The game's use rule replaces IW4L's for use triggers and usable
    /// entities.
    pub(crate) fn on_use(&mut self, rule: UseRule) {
        self.3 = Some(rule);
    }

    pub(crate) fn use_rule(&self) -> Option<UseRule> {
        self.3
    }

    pub(crate) fn on_connect(&mut self, hook: ConnectHook) {
        self.4.push(hook);
    }

    pub(crate) fn connect_hooks(&self) -> Vec<ConnectHook> {
        self.4.clone()
    }

    /// The game's missile notifies replace IW4L's.
    pub(crate) fn on_missiles(&mut self, rule: MissileRule) {
        self.5 = Some(rule);
    }

    pub(crate) fn missile_rule(&self) -> Option<MissileRule> {
        self.5
    }

    /// The client game a client registry's scripts run in.
    pub(crate) fn on_client_sync(&mut self, sync: ClientSync) {
        self.6 = Some(sync);
    }

    pub(crate) fn client_sync(&self) -> Option<ClientSync> {
        self.6
    }

    /// What a client registry's game shows of its own after its scripts ran.
    pub(crate) fn on_client_present(&mut self, present: ClientSync) {
        self.7 = Some(present);
    }

    pub(crate) fn client_present(&self) -> Option<ClientSync> {
        self.7
    }

    pub fn register(&mut self, namespace: Namespace, name: &str, native: Native) {
        self.0
            .insert((namespace, name.to_ascii_lowercase()), native);
    }
    /// Binds every builtin the program names that the host does not implement
    /// to a native that fails, so a partial host still runs the scripts and each
    /// missing call site is reported once. Returns the names it bound.
    pub fn bind_gaps(&mut self, program: &crate::script::Program) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        for builtin in &program.natives {
            if self.get(builtin.namespace, builtin.name).is_none() {
                self.register(builtin.namespace, builtin.name, |_, _, _| {
                    Err("not implemented yet".into())
                });
                gaps.push(builtin.name);
            }
        }
        gaps.sort_unstable();
        gaps.dedup();
        gaps
    }
    /// How many of the builtins the program names the host does not
    /// implement, and how many it names.
    pub fn coverage(&self, program: &crate::script::Program) -> (usize, usize) {
        let named: std::collections::BTreeSet<_> = program
            .natives
            .iter()
            .map(|builtin| (builtin.namespace, builtin.name))
            .collect();
        let missing = named
            .iter()
            .filter(|(namespace, name)| self.get(*namespace, name).is_none())
            .count();
        (missing, named.len())
    }
    pub(crate) fn get(&self, namespace: Namespace, name: &str) -> Option<Native> {
        self.0.get(&(namespace, name.to_owned())).copied()
    }
}

pub(crate) fn unavailable(
    world: &mut World,
    name: &'static str,
    reason: &str,
) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let calls = runtime.unsupported.entry(name).or_default();
    *calls = calls.saturating_add(1);
    Err(format!("unavailable: {reason}"))
}
