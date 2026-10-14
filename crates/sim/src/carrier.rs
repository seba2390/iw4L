use core::fmt;
use core::ops::{Deref, DerefMut};

use bevy_ecs::prelude::{Entity, World};
use playerstate_iw4::PlayerState;

use crate::frame::{
    FrameWorld, PayloadIndex, collect_dropped_items, collect_players, collect_projectiles,
    collect_script_movers, install_state_entity, player_ref, spawn_payload_rows,
};
use crate::gentity::ScriptMoverGentity;
use crate::identities::ScriptModelId;
use crate::input::TickInput;
use crate::snapshot::Snapshot;
use crate::world::{ClientId, SimState, Tick};

pub struct SimWorld {
    ecs: World,
    state_entity: Entity,
    schedule: bevy_ecs::schedule::Schedule,
}

impl Default for SimWorld {
    fn default() -> Self {
        let mut ecs = World::new();
        let state_entity = ecs.spawn(SimState::default()).id();
        ecs.entity_mut(state_entity).insert(PayloadIndex::default());
        install_state_entity(&mut ecs, state_entity);
        ecs.insert_resource(crate::LocalPlayerProfile::default());
        ecs.insert_resource(crate::PersistentDataStore::default());
        ecs.init_resource::<crate::script::MatchScript>();
        ecs.insert_resource(crate::script::RoundScript::new(
            ecs.resource::<crate::script::MatchScript>(),
        ));
        ecs.insert_resource(crate::script::Mechanics::default());
        ecs.insert_resource(crate::script::NativeRegistry::default());
        Self {
            ecs,
            state_entity,
            schedule: crate::step::schedule(),
        }
    }
}

impl Clone for SimWorld {
    fn clone(&self) -> Self {
        let mut ecs = World::new();
        let state_entity = ecs.spawn((**self).clone()).id();
        ecs.entity_mut(state_entity).insert(PayloadIndex::default());
        install_state_entity(&mut ecs, state_entity);
        spawn_payload_rows(
            &mut ecs,
            collect_players(&self.ecs),
            collect_projectiles(&self.ecs),
            collect_script_movers(&self.ecs),
            collect_dropped_items(&self.ecs),
        );
        ecs.insert_resource(*self.ecs.resource::<crate::LocalPlayerProfile>());
        ecs.insert_resource(self.ecs.resource::<crate::PersistentDataStore>().clone());
        crate::script::copy_state(&self.ecs, &mut ecs);
        Self {
            ecs,
            state_entity,
            schedule: crate::step::schedule(),
        }
    }
}

impl SimWorld {
    /// The script world's living actors, for a test player.
    pub fn living_actors(&self) -> Vec<crate::script::ActorSighting> {
        crate::script::host::sightings::living_actors(&self.ecs)
    }

    /// The centre of the body box nearest `near` on a script model's posed
    /// collision, for a test player's aim.
    pub fn body_point(&self, body: ScriptModelId, near: [f32; 3]) -> Option<[f32; 3]> {
        let owner = crate::AuthorityModelOwner::ScriptModel(body);
        let row = self
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner == owner)?;
        let bones = &row.dobj.as_ref()?.current_collision.as_ref()?.bones;
        let distance = |at: [f32; 3]| (0..3).map(|i| (at[i] - near[i]).powi(2)).sum::<f32>();
        bones
            .iter()
            .map(|bone| bone.center)
            .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
    }

    /// The doors, debris, wall weapons, box and power switch the scripts
    /// offer, for a test player's use scenarios.
    pub fn use_sites(&self) -> Vec<crate::script::UseSite> {
        crate::script::host::sightings::use_sites(&self.ecs)
    }

    /// The middle of a use site: its brush model's middle for a trigger
    /// entity, half a radius trigger's height above its origin, else its
    /// origin (a unitrigger stub's box is centred there).
    pub fn use_site_center(&self, site: &crate::script::UseSite) -> [f32; 3] {
        if let Some(height) = site.height {
            return [
                site.origin[0],
                site.origin[1],
                site.origin[2] + height * 0.5,
            ];
        }
        site.brush
            .and_then(|n| self.clip_cmodels().models.get(n as usize))
            .map_or(site.origin, |model| {
                std::array::from_fn(|i| site.origin[i] + (model.mins[i] + model.maxs[i]) * 0.5)
            })
    }

    /// The use triggers near a point (what a unitrigger stub made), for a
    /// test player.
    pub fn use_triggers_near(&self, point: [f32; 3], reach: f32) -> Vec<String> {
        crate::script::host::sightings::use_triggers_near(&self.ecs, point, reach)
    }

    /// Where the map's path nodes stand: places a player may stand, for a
    /// test player.
    pub fn path_node_origins(&self) -> Vec<[f32; 3]> {
        self.actor_paths()
            .map_or_else(Vec::new, |paths| paths.origins().collect())
    }

    /// Whether a player standing at `origin` touches a volume of a zone the
    /// scripts have enabled (Black Ops 2's `in_enabled_playable_area`), for
    /// a test player.
    pub fn in_enabled_player_volume(&self, origin: [f32; 3]) -> bool {
        let mins = std::array::from_fn(|i| origin[i] + crate::PLAYER_MINS[i]);
        let maxs = std::array::from_fn(|i| origin[i] + crate::PLAYER_MAXS[i]);
        crate::script::host::sightings::enabled_player_volumes(&self.ecs)
            .into_iter()
            .any(|(at, brush, trigger_model)| {
                crate::script::host::triggers::map_volume_touches(
                    self,
                    at,
                    brush,
                    trigger_model,
                    mins,
                    maxs,
                )
            })
    }

    /// Whether a player standing at `origin` touches a volume of the named
    /// zone, for a test player.
    pub fn in_zone_player_volume(&self, origin: [f32; 3], zone: &str) -> bool {
        let mins = std::array::from_fn(|i| origin[i] + crate::PLAYER_MINS[i]);
        let maxs = std::array::from_fn(|i| origin[i] + crate::PLAYER_MAXS[i]);
        crate::script::host::sightings::zone_player_volumes(&self.ecs, zone)
            .into_iter()
            .any(|(at, brush, trigger_model)| {
                crate::script::host::triggers::map_volume_touches(
                    self,
                    at,
                    brush,
                    trigger_model,
                    mins,
                    maxs,
                )
            })
    }

    /// How many volumes the enabled zones have, for a test player's log.
    pub fn enabled_player_volume_count(&self) -> usize {
        crate::script::host::sightings::enabled_player_volumes(&self.ecs).len()
    }

    /// An entry of a level array field, for a test player.
    pub fn level_array_entry(&self, array: &str, key: &str) -> Option<String> {
        crate::script::host::sightings::level_array_entry(&self.ecs, array, key)
    }

    /// A script object's field as text, for a test player.
    pub fn object_field_text(&self, id: u64, name: &str) -> String {
        crate::script::host::sightings::object_field_text(&self.ecs, id, name)
    }

    /// The receivers of the waits pending on a notify name, for a test
    /// player.
    pub fn waits_on(&self, name: &str) -> Vec<String> {
        crate::script::host::sightings::waits_on(&self.ecs, name)
    }

    /// The origin, yaw and entity number of the entity a level field holds,
    /// for a test player.
    pub fn level_entity_pose(&self, name: &str) -> Option<([f32; 3], f32, i32)> {
        crate::script::host::sightings::level_entity_pose(&self.ecs, name)
    }

    /// The entity number a player stands on, for a test player.
    pub fn player_ground_entity(&self, id: crate::ClientId) -> Option<i32> {
        self.player(id).map(|ps| ps.ground_entity_num)
    }

    /// A player's script field as text, for a test player.
    pub fn player_field_text(&self, client: crate::ClientId, name: &str) -> String {
        crate::script::host::sightings::player_field_text(&self.ecs, client, name)
    }

    /// Whether a level flag is set, for a test player.
    pub fn level_flag(&self, name: &str) -> bool {
        crate::script::host::sightings::level_flag(&self.ecs, name)
    }

    /// Whether the scripts mark a zone enabled, for a test player.
    pub fn zone_is_enabled(&self, name: &str) -> bool {
        crate::script::host::sightings::zone_is_enabled(&self.ecs, name)
    }

    /// Whether the scripts mark a zone active, for a test player.
    pub fn zone_is_active(&self, name: &str) -> bool {
        crate::script::host::sightings::zone_is_active(&self.ecs, name)
    }

    /// What `_zm_unitrigger` holds for a unitrigger stub, for a test
    /// player's log.
    pub fn unitrigger_state(&self, stub: u64) -> String {
        crate::script::host::sightings::unitrigger_state(&self.ecs, stub)
    }

    /// Whether a script object still lives, for a test player.
    pub fn script_object_lives(&self, id: u64) -> bool {
        crate::script::host::sightings::object_lives(&self.ecs, id)
    }

    /// An integer script field of the level or a player, for a test player.
    pub fn script_int(&self, owner: crate::script::ScriptOwner, name: &str) -> Option<i32> {
        crate::script::host::sightings::script_int(&self.ecs, owner, name)
    }

    /// Starts a script thread on the host, for a test player: `receiver` is
    /// `level`, `playerN` or `targetname=<name>` (the first entity of that
    /// targetname); each argument reads as a number, an `(x,y,z)`
    /// vector, or else a string.
    pub fn start_script(
        &mut self,
        name: &str,
        receiver: &str,
        args: &[String],
    ) -> Result<(), String> {
        use crate::script::Value;
        let receiver = match receiver.strip_prefix("player") {
            Some(n) => {
                let client = n.parse().map_err(|_| format!("bad receiver {receiver}"))?;
                crate::script::host::players::player_object(&self.ecs, client)
            }
            None if receiver == "level" => Value::level(),
            None => match receiver.strip_prefix("targetname=") {
                Some(name) => {
                    let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
                    let ids: Vec<u64> = runtime.entities.keys().copied().collect();
                    let wanted = Value::String(name.into());
                    let id = ids
                        .into_iter()
                        .find(|&id| runtime.object_field(id, "targetname") == wanted)
                        .ok_or_else(|| format!("no entity named {name}"))?;
                    Value::Object(id)
                }
                None => return Err(format!("bad receiver {receiver}")),
            },
        };
        let args = args
            .iter()
            .map(|arg| {
                if let Ok(n) = arg.parse::<i32>() {
                    return Value::Int(n);
                }
                if let Ok(f) = arg.parse::<f32>() {
                    return Value::Float(f);
                }
                let parts: Vec<f32> = arg
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split(',')
                    .filter_map(|part| part.trim().parse().ok())
                    .collect();
                match parts.as_slice() {
                    [x, y, z] => Value::Vector([*x, *y, *z]),
                    _ => Value::String(arg.as_str().into()),
                }
            })
            .collect();
        crate::script::start(&mut self.ecs, name, receiver, args)
            .map(|_| ())
            .map_err(|fault| fault.to_string())
    }
}

impl fmt::Debug for SimWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimWorld").field("state", &**self).finish()
    }
}

impl Deref for SimWorld {
    type Target = SimState;

    fn deref(&self) -> &Self::Target {
        self.ecs
            .get::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
    }
}

impl DerefMut for SimWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ecs
            .get_mut::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
            .into_inner()
    }
}

impl SimWorld {
    pub fn validate_entity_lifetimes(&self) -> Result<(), String> {
        crate::frame::validate_entity_payloads(&self.ecs)
    }

    pub fn entity_lifetime_summary(&self) -> String {
        let snapshot = self.entity_kernel().to_snapshot();
        let occupied = snapshot
            .slots
            .iter()
            .filter(|slot| slot.occupied.is_some())
            .count();
        let held = snapshot
            .slots
            .iter()
            .filter(|slot| {
                slot.occupied
                    .is_some_and(|slot| slot.transient_event_time_ms.is_some())
            })
            .count();
        let runtime = self.ecs.resource::<crate::script::RoundScript>();
        format!(
            "occupied_slots = {occupied}\nevent_holds = {held}\nprojectile_payloads = {}\nitem_payloads = {}\nmover_payloads = {}\nscript_objects = {}\nscript_defined_entities = {}\nentity_consistency = {:?}\nmap_glass = {:?}\n",
            collect_projectiles(&self.ecs).len(),
            collect_dropped_items(&self.ecs).len(),
            collect_script_movers(&self.ecs).len(),
            runtime.objects.len(),
            runtime
                .entities
                .keys()
                .filter(|id| runtime.script_is_defined(id))
                .count(),
            self.validate_entity_lifetimes(),
            self.content().map().glass
        )
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn persistent_data(&self) -> &crate::PersistentDataStore {
        self.ecs.resource::<crate::PersistentDataStore>()
    }

    pub fn persistent_data_mut(&mut self) -> &mut crate::PersistentDataStore {
        self.ecs
            .resource_mut::<crate::PersistentDataStore>()
            .into_inner()
    }

    pub fn set_persistent_data(&mut self, store: crate::PersistentDataStore) {
        self.ecs.insert_resource(store);
    }

    pub fn local_player_profile(&self) -> crate::LocalPlayerProfile {
        *self.ecs.resource::<crate::LocalPlayerProfile>()
    }

    pub fn set_local_player_profile(&mut self, profile: crate::LocalPlayerProfile) {
        self.ecs.insert_resource(profile);
    }

    pub(crate) fn frame(&mut self) -> FrameWorld<'_> {
        FrameWorld::new(&mut self.ecs, self.state_entity)
    }

    fn run(
        &mut self,
        tick: Tick,
        input: &TickInput,
        msec: i32,
        reason: StepReason,
    ) -> Result<crate::TickResult, crate::script::Fault> {
        crate::step::run_schedule(&mut self.ecs, &mut self.schedule, tick, input, msec, reason)
    }

    pub fn visit_projectiles(&self, visit: impl FnMut(&crate::ProjectileState)) {
        crate::frame::visit_projectiles(&self.ecs, visit);
    }

    pub fn script_mover_count(&self) -> usize {
        crate::frame::script_mover_row_count(&self.ecs)
    }

    pub fn visit_script_movers(&self, visit: impl FnMut(&ScriptMoverGentity)) {
        crate::frame::visit_script_movers(&self.ecs, visit);
    }

    pub fn script_mover_by_number(&self, number: i32) -> Option<ScriptMoverGentity> {
        crate::frame::script_mover_by_number(&self.ecs, number)
    }

    pub fn script_mover_by_id(&self, id: ScriptModelId) -> Option<ScriptMoverGentity> {
        crate::frame::script_mover_by_id(&self.ecs, id)
    }

    pub fn install_gsc_program(
        &mut self,
        program: crate::script::Program,
        natives: crate::script::NativeRegistry,
        level: crate::script::LevelData,
    ) -> Result<(), crate::script::Fault> {
        crate::script::install(&mut self.ecs, program, natives, level)
    }

    pub fn set_expected_players(&mut self, count: usize) {
        if let Some(mut runtime) = self.ecs.get_resource_mut::<crate::script::RoundScript>() {
            runtime.expected_players = count;
        }
    }

    pub fn start_gsc(
        &mut self,
        name: &str,
        receiver: crate::script::Value,
        arguments: Vec<crate::script::Value>,
    ) -> Result<u64, crate::script::Fault> {
        crate::script::start(&mut self.ecs, name, receiver, arguments)
    }

    /// Registers the local FoF floats before installing scripts on listen authority.
    pub fn register_local_presentation_dvars(&mut self, local: Option<ClientId>) {
        crate::script::host::natives::iw4::register_local_presentation_dvars(&mut self.ecs, local);
    }

    /// The gametype's settings the scripts read (`GetGametypeSetting`).
    pub fn set_gametype_settings(&mut self, settings: Vec<(String, String)>) {
        self.ecs.insert_resource(crate::script::GametypeSettings(
            settings.into_iter().collect(),
        ));
    }

    pub fn set_gsc_dvar(&mut self, name: &str, value: &str) {
        crate::script::set_dvar(&mut self.ecs, name, value);
    }

    pub fn gsc_realm(&self) -> Option<crate::script::Realm> {
        self.ecs
            .resource::<crate::script::RoundScript>()
            .program
            .as_ref()
            .map(|program| program.rules())
    }

    pub fn gsc_program_fingerprint(&self) -> Option<[u8; 32]> {
        self.ecs
            .resource::<crate::script::RoundScript>()
            .program_fingerprint()
    }

    pub fn set_script_archive_time(
        &mut self,
        client: ClientId,
        requested_ms: i32,
        attained_ms: i32,
    ) {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if let Some(slot) = runtime.players.get_mut(&client.0)
            && slot.seat.archive_ms == requested_ms
        {
            slot.seat.archive_ms = attained_ms;
        }
    }

    pub fn script_seats(&self) -> Vec<(ClientId, crate::ScriptSeat)> {
        crate::script::script_seats(&self.ecs)
    }

    pub fn take_script_fault(&mut self) -> Option<crate::script::Fault> {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if runtime.fault_reported {
            return None;
        }
        runtime.fault_reported = runtime.fault.is_some();
        runtime.fault.clone()
    }

    pub fn spawn_script_mover(
        &mut self,
        id: ScriptModelId,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        self.frame().spawn_script_mover(id, origin, angles)
    }

    pub fn spawn_brush_mover(
        &mut self,
        id: ScriptModelId,
        cmodel: u32,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        let mut frame = self.frame();
        let number = frame.spawn_script_mover(id, origin, angles)?;
        if let Some(mover) = frame.script_mover_mut_by_number(number) {
            mover.state.index = cmodel as i32;
            mover.state.solid = entity_iw4::SCRIPT_MOVER_BMODEL_SOLID;
        }
        Ok(number)
    }

    pub fn gentity_number(&self, id: ScriptModelId) -> Option<i32> {
        self.script_mover_by_id(id).map(|mover| mover.state.number)
    }

    pub fn begin_script_mover_rotate_velocity(
        &mut self,
        id: ScriptModelId,
        speed: [f32; 3],
        total_time_seconds: f32,
        level_time_ms: i32,
    ) -> bool {
        self.frame().begin_script_mover_rotate_velocity(
            id,
            speed,
            total_time_seconds,
            level_time_ms,
        )
    }

    pub fn begin_script_movers_rotate_velocity_supplied(
        &mut self,
        speed: f32,
        level_time_ms: i32,
    ) -> usize {
        self.frame()
            .begin_script_movers_rotate_velocity_supplied(speed, level_time_ms)
    }

    pub fn set_script_mover_r_box(
        &mut self,
        number: i32,
        box_mid: [f32; 3],
        box_half: [f32; 3],
    ) -> bool {
        self.frame()
            .set_script_mover_r_box(number, box_mid, box_half)
    }

    pub fn set_script_mover_origin(&mut self, number: i32, origin: [f32; 3]) -> bool {
        self.frame().set_script_mover_origin(number, origin)
    }

    pub fn player(&self, id: ClientId) -> Option<&PlayerState> {
        player_ref(&self.ecs, id)
    }

    pub fn visit_players(&self, visit: impl FnMut(ClientId, &PlayerState)) {
        crate::frame::visit_players(&self.ecs, visit);
    }

    pub fn player_count(&self) -> usize {
        crate::frame::player_row_count(&self.ecs)
    }

    pub fn retire_client(&mut self, id: ClientId) {
        let mut runtime = self.ecs.resource_mut::<crate::script::RoundScript>();
        if runtime.players.contains_key(&id.0) {
            runtime.disconnects.insert(id.0);
        } else {
            self.frame().retire_client(id);
            self.ecs
                .resource_mut::<crate::PersistentDataStore>()
                .unbind(id);
        }
    }

    pub fn set_origin(&mut self, id: ClientId, origin: [f32; 3]) -> bool {
        self.frame().set_origin(id, origin)
    }

    pub fn set_legs_anim(&mut self, id: ClientId, legs_anim: i32) -> bool {
        self.frame().set_legs_anim(id, legs_anim)
    }

    pub fn set_viewangles(&mut self, id: ClientId, viewangles: [f32; 3]) -> bool {
        self.frame().set_viewangles(id, viewangles)
    }

    pub fn set_e_flags(&mut self, id: ClientId, e_flags: u32) -> bool {
        self.frame().set_e_flags(id, e_flags)
    }

    pub fn set_leanf(&mut self, id: ClientId, leanf: f32) -> bool {
        self.frame().set_leanf(id, leanf)
    }

    pub fn set_view_height_target(&mut self, id: ClientId, view_height_target: i32) -> bool {
        self.frame().set_view_height_target(id, view_height_target)
    }

    pub fn gate_nudge_origin(&mut self, id: ClientId, delta: [f32; 3]) {
        self.frame().gate_nudge_origin(id, delta);
    }

    pub fn snapshot(&self, tick: Tick) -> Snapshot {
        let state = self
            .ecs
            .get::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component");
        state.snapshot_with_dynamic_rows(
            tick,
            collect_players(&self.ecs),
            collect_projectiles(&self.ecs),
            collect_script_movers(&self.ecs),
            collect_dropped_items(&self.ecs),
        )
    }

    pub fn slow_motion(&self) -> Option<crate::ScriptSlowMotion> {
        self.ecs
            .resource::<crate::script::RoundScript>()
            .engine
            .slow_motion
    }

    pub fn adopt_snapshot(&mut self, snapshot: &Snapshot) -> crate::AdoptReport {
        assert!(
            self.gsc_program_fingerprint().is_none(),
            "snapshot lacks GSC state; restore a full SimWorld checkpoint"
        );
        self.frame().adopt_snapshot(snapshot)
    }

    pub fn adopt_prediction_snapshot(
        &mut self,
        snapshot: &Snapshot,
        local: ClientId,
    ) -> crate::AdoptReport {
        assert!(
            self.gsc_program_fingerprint().is_none(),
            "prediction snapshots cannot restore authority GSC state"
        );
        self.frame().adopt_prediction_snapshot(snapshot, local)
    }

    pub fn request_round_restart(&mut self) -> Result<(), String> {
        if !self.is_running() || !self.cheats_enabled() {
            return Err("round restart needs an active match with cheats".into());
        }
        self.ecs
            .resource_mut::<crate::script::RoundScript>()
            .request_round_restart(true)
    }

    pub fn shutdown_game(&mut self) {
        self.frame().shutdown_game();
        crate::script::reset(&mut self.ecs);
        self.ecs
            .resource_mut::<crate::PersistentDataStore>()
            .clear_bindings();
    }

    pub fn hitvol_dump(&self) -> Vec<crate::world::HitvolDumpRow> {
        SimState::hitvol_dump(self, |id| player_ref(&self.ecs, id).copied())
    }

    pub fn lagcomp_query_for(
        &self,
        attacker: ClientId,
        current: Tick,
    ) -> crate::bullet_collision::LagcompQuery {
        SimState::lagcomp_query_for(self, attacker, current, |id| {
            player_ref(&self.ecs, id).copied()
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepReason {
    #[default]
    AuthorityFrame,

    PredictNew,

    Replay,
}

impl StepReason {
    pub const fn advances_authority_world(self) -> bool {
        matches!(self, Self::AuthorityFrame)
    }

    pub const fn is_replay(self) -> bool {
        matches!(self, Self::Replay)
    }
}

pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: StepReason,
) -> crate::TickResult {
    try_step(world, tick, input, msec, reason)
        .unwrap_or_else(|fault| panic!("GSC execution failed: {fault}"))
}

pub fn try_step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: StepReason,
) -> Result<crate::TickResult, crate::script::Fault> {
    world.run(tick, input, msec, reason)
}
