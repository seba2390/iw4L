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
    /// Its own player state, as its snapshot carries it.
    pub(crate) player: LocalPlayerView,
}

/// A local client's own player state, which its client scripts read
/// (`getLocalClientHealth`, `getCurrentWeapon`, `isADS`, …).
#[derive(Clone, Debug, Default)]
pub(crate) struct LocalPlayerView {
    pub(crate) health: i32,
    pub(crate) max_health: i32,
    /// The current weapon's name, `none` without one.
    pub(crate) weapon: String,
    /// Whether the weapon is raised at all toward aiming down its sights.
    pub(crate) ads: bool,
    pub(crate) meleeing: bool,
    pub(crate) throwing_grenade: bool,
    pub(crate) origin: [f32; 3],
    pub(crate) velocity: [f32; 3],
    pub(crate) angles: [f32; 3],
    /// Its eye: origin raised by its view height, leaned.
    pub(crate) eye: [f32; 3],
    /// Its weapons' clip ammo, by weapon name.
    pub(crate) clips: std::collections::BTreeMap<String, i32>,
    /// The naked vision the server last set for it, `default` before any.
    pub(crate) naked_vision: String,
}

/// The local client's player state from the server's world.
fn local_player_view(server: &mut World) -> LocalPlayerView {
    let id = crate::ClientId(0);
    let (ps, max_health) = {
        let frame = crate::frame::FrameWorld::from_world(server);
        let Some(ps) = frame.player(id) else {
            return LocalPlayerView::default();
        };
        let max_health = frame
            .client_meta(id)
            .map(|meta| meta.max_health)
            .filter(|&n| n > 0)
            .unwrap_or(ps.max_health);
        (*ps, max_health)
    };
    let own_vision = crate::frame::FrameWorld::from_world(server)
        .client_meta(id)
        .and_then(|meta| meta.view_effects.naked_vision.as_ref())
        .map(|vision| vision.name.clone());
    let naked_vision = own_vision
        .or_else(|| {
            let runtime = server.resource::<RoundScript>();
            runtime.engine.naked_vision.as_ref().map(|v| v.name.clone())
        })
        .unwrap_or_else(|| "default".to_owned());
    let weapon = super::host::players::script_weapon(server, 0, ps.weapon);
    let frame = crate::frame::FrameWorld::from_world(server);
    let (meleeing, throwing_grenade) = crate::game_move::weapon_activity(&frame, id)
        .map_or((false, false), |a| (a.meleeing, a.throwing_grenade));
    let eye = math_iw4::add_lean_to_position(
        [
            ps.origin[0],
            ps.origin[1],
            ps.view_height_current + ps.origin[2],
        ],
        ps.viewangles[1],
        ps.leanf,
        16.0,
        20.0,
    );
    let clips = ps
        .weapons
        .iter()
        .filter_map(|&held| u32::try_from(held).ok().filter(|&w| w != 0))
        .map(|held| {
            (
                crate::script_player::weapon_name(&frame, held),
                crate::script_player::ammo_clip(&frame, id, held),
            )
        })
        .collect();
    LocalPlayerView {
        health: ps.health,
        max_health,
        weapon: crate::script_player::weapon_name(&frame, weapon),
        ads: ps.move_player(0, false).weapon_pos_frac > 0.0,
        meleeing,
        throwing_grenade,
        origin: ps.origin,
        velocity: ps.velocity,
        angles: ps.viewangles,
        eye,
        clips,
        naked_vision,
    }
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
    /// The local player jumped or landed.
    JumpLand(game_api::movement::JumpLand),
}

/// A local player's jump or landing, for its client scripts.
pub(crate) fn jump_land(
    server: &mut World,
    client: crate::ClientId,
    event: game_api::movement::JumpLand,
) {
    if client.0 != 0
        || (!server.contains_resource::<ClientScripts>()
            && !server.contains_resource::<super::host::client_scripts::ClientScripts>())
    {
        return;
    }
    if let Some(mut runtime) = server.get_resource_mut::<RoundScript>() {
        runtime.client_messages.push(ClientMessage::JumpLand(event));
    }
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

/// What the local client's sound system last said of the sounds the client
/// played, by playback id; nothing until a sound system runs.
#[derive(Resource, Default)]
pub(crate) struct ClientSoundReports {
    /// Whether a sound system has reported at all.
    pub(crate) heard: bool,
    pub(crate) statuses: std::collections::BTreeMap<i32, crate::ClientSoundStatus>,
}

/// The local client's sound system reports on the client's sounds: the
/// sounds it names are no longer waiting to start.
pub(crate) fn report_sounds(server: &mut World, statuses: Vec<crate::ClientSoundStatus>) {
    let Some(mut client) = server.get_resource_mut::<ClientScripts>() else {
        crate::script::host::client_scripts::report_sounds(server, statuses);
        return;
    };
    apply_sound_reports(&mut client.world, statuses);
}

/// A client VM's world takes its sound system's reports.
pub(crate) fn apply_sound_reports(world: &mut World, statuses: Vec<crate::ClientSoundStatus>) {
    {
        let mut output = world.resource_mut::<ClientOutput>();
        for status in &statuses {
            output.issued.remove(&status.id);
        }
    }
    let mut reports = world.resource_mut::<ClientSoundReports>();
    reports.heard = true;
    if crate::sound_trace::enabled() {
        for status in &statuses {
            let was = reports.statuses.get(&status.id).map(|old| old.playing);
            if was != Some(status.playing) {
                crate::sound_trace::client_report(status);
            }
        }
    }
    reports.statuses = statuses.into_iter().map(|s| (s.id, s)).collect();
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
    /// A line emitter (`soundlineemitter(alias, start, end)`), by its alias
    /// and its ends' bits: it plays at the point of its segment nearest the
    /// listener.
    Line(String, [u32; 3], [u32; 3]),
}

/// The point of a segment nearest a point, as the client's sound system
/// finds it: a segment shorter than a unit is its start.
pub(crate) fn nearest_on_segment(point: [f32; 3], start: [f32; 3], end: [f32; 3]) -> [f32; 3] {
    let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let length_sq = d[0] * d[0] + d[2] * d[2] + d[1] * d[1];
    if length_sq < 1.0 {
        return start;
    }
    let t = ((point[0] - start[0]) * d[0]
        + (point[2] - start[2]) * d[2]
        + (point[1] - start[1]) * d[1])
        / length_sq;
    if t < 0.0 {
        return start;
    }
    if 1.0 < t {
        return end;
    }
    [
        d[0] * t + start[0],
        d[1] * t + start[1],
        d[2] * t + start[2],
    ]
}

/// A loop sound the client plays until it stops it.
#[derive(Clone, Debug)]
pub(crate) struct ClientLoop {
    pub(crate) alias: String,
    /// The loop's own number, which names it to the audio while it plays.
    pub(crate) serial: u32,
    /// The playback id its script was answered.
    pub(crate) id: i32,
}

/// An effect the client plays at a point (`playFx`, or a triggered
/// `spawnFx` entity): it begins at `begin_ms` (level time) and is alive
/// through `end_ms`.
#[derive(Clone, Debug)]
pub(crate) struct ClientEffect {
    pub(crate) name: String,
    pub(crate) origin: [f32; 3],
    pub(crate) forward: [f32; 3],
    pub(crate) up: [f32; 3],
    /// When it begins; an untriggered `spawnFx` entity has none yet.
    pub(crate) begin_ms: Option<i32>,
    pub(crate) end_ms: i32,
}

/// An effect the client plays on an entity's tag (`playFxOnTag`).
#[derive(Clone, Debug)]
pub(crate) struct ClientTagEffect {
    pub(crate) name: String,
    pub(crate) entity: u64,
    pub(crate) tag: String,
    pub(crate) end_ms: i32,
}

/// A tag effect to start or stop on the client's view of an entity.
#[derive(Clone, Debug)]
pub(crate) struct ClientTagEvent {
    pub(crate) effect: ClientTagEffect,
    pub(crate) stop: bool,
}

/// What the client VM's frame asks of the client: sounds to play, the
/// loops it plays, and its effects.
#[derive(Resource, Default)]
pub(crate) struct ClientOutput {
    /// The sound commands for the local client's sound system.
    pub(crate) sound_commands: Vec<crate::ScriptAudioCommand>,
    /// Sounds played that the sound system has not reported on yet.
    pub(crate) issued: std::collections::BTreeSet<i32>,
    /// The next sound's playback id, as `playsound` answers it.
    pub(crate) next_sound: i32,
    pub(crate) loops: std::collections::BTreeMap<LoopSource, ClientLoop>,
    next_loop: u32,
    /// Effects played at a point, by handle.
    pub(crate) effects: std::collections::BTreeMap<i32, ClientEffect>,
    /// Effects played on a tag, by handle.
    pub(crate) tag_effects: std::collections::BTreeMap<i32, ClientTagEffect>,
    /// `spawnFx` entities' effects, by entity.
    pub(crate) fx_entities: std::collections::BTreeMap<u64, ClientEffect>,
    /// Tag effects to start or stop this frame.
    pub(crate) tag_events: Vec<ClientTagEvent>,
    next_effect: i32,
    /// The latest vision calls, by serial.
    pub(crate) vision_calls: std::collections::VecDeque<(u32, crate::VisionCall)>,
    next_vision_call: u32,
}

impl ClientOutput {
    /// Records a vision call, keeping the latest few.
    pub(crate) fn vision_call(&mut self, call: crate::VisionCall) {
        self.next_vision_call = self.next_vision_call.wrapping_add(1).max(1);
        self.vision_calls.push_back((self.next_vision_call, call));
        while self.vision_calls.len() > crate::CLIENT_VISION_TAIL {
            self.vision_calls.pop_front();
        }
    }

    /// A new effect handle.
    pub(crate) fn effect_handle(&mut self) -> i32 {
        self.next_effect = self.next_effect.wrapping_add(1).max(1);
        self.next_effect
    }

    /// Whether the effect a handle names is still alive at `now`; a dead
    /// one's handle is forgotten.
    pub(crate) fn effect_alive(&mut self, handle: i32, now: i32) -> bool {
        if let Some(effect) = self.effects.get(&handle) {
            if now <= effect.end_ms {
                return true;
            }
            self.effects.remove(&handle);
        }
        if let Some(effect) = self.tag_effects.get(&handle) {
            if now <= effect.end_ms {
                return true;
            }
            self.tag_effects.remove(&handle);
        }
        false
    }

    /// Kills the effect a handle names.
    pub(crate) fn kill_effect(&mut self, handle: i32) {
        self.effects.remove(&handle);
        if let Some(effect) = self.tag_effects.remove(&handle) {
            self.tag_events.push(ClientTagEvent { effect, stop: true });
        }
    }

    /// A new playback id.
    pub(crate) fn sound_id(&mut self) -> i32 {
        let id = self.next_sound;
        self.next_sound = self.next_sound.wrapping_add(1);
        id
    }

    /// Starts a loop in place of the one playing there, under a new playback
    /// id, which it answers.
    pub(crate) fn restart_loop(&mut self, source: LoopSource, alias: String) -> i32 {
        let serial = self.next_loop & 0x0fff_ffff;
        self.next_loop = self.next_loop.wrapping_add(1);
        let id = self.sound_id();
        self.loops.insert(source, ClientLoop { alias, serial, id });
        id
    }

    /// Starts a loop (keeping the one already playing the same alias there),
    /// answering its playback id.
    pub(crate) fn start_loop(&mut self, source: LoopSource, alias: String) -> i32 {
        if let Some(playing) = self.loops.get(&source).filter(|l| l.alias == alias) {
            return playing.id;
        }
        let serial = self.next_loop & 0x0fff_ffff;
        self.next_loop = self.next_loop.wrapping_add(1);
        let id = self.sound_id();
        self.loops.insert(source, ClientLoop { alias, serial, id });
        id
    }
}

/// The effects the server registered (`loadFx`), which the client's
/// `loadFx` looks up, and how long each lives once it begins.
#[derive(Resource, Default)]
pub(crate) struct ClientEffectTable {
    /// Effect names by index.
    pub(crate) names: std::collections::BTreeMap<i32, String>,
    /// Indices by lowercase effect name.
    pub(crate) indices: std::collections::BTreeMap<String, i32>,
    /// The longest each effect lives (msec), by lowercase name.
    pub(crate) lives: std::collections::BTreeMap<String, i32>,
}

impl ClientEffectTable {
    /// How long an effect lives once it begins: zero when its definition
    /// is not loaded.
    pub(crate) fn life(&self, name: &str) -> i32 {
        self.lives
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }
}

/// A client entity turned into a primary light (`makelight`).
#[derive(Clone, Debug)]
pub(crate) struct ClientLight {
    /// The map's primary light it was made from.
    pub(crate) index: usize,
    /// Its color as bytes, with its exponent in the fourth.
    pub(crate) color: [u8; 4],
    pub(crate) intensity: f32,
    pub(crate) radius: f32,
    pub(crate) cos_half_fov_outer: f32,
    pub(crate) cos_half_fov_inner: f32,
}

/// The map's primary lights, and the client entities made lights.
#[derive(Resource, Default)]
pub(crate) struct ClientLights {
    pub(crate) map: Vec<super::MapPrimaryLight>,
    pub(crate) lights: std::collections::BTreeMap<u64, ClientLight>,
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
pub(crate) const CLIENT_LOOP_OWNER: u32 = 0x4000_0000;

/// Whether the local client's sound system still plays a playback: a loop
/// the client plays, or a sound it reported playing; a sound played since
/// its last report plays until it says otherwise, and with no sound system
/// nothing plays.
pub(crate) fn sound_playing(world: &World, id: i32) -> bool {
    let output = world.resource::<ClientOutput>();
    if output.loops.values().any(|playing| playing.id == id) {
        return true;
    }
    let reports = world.resource::<ClientSoundReports>();
    if !reports.heard {
        return false;
    }
    match reports.statuses.get(&id) {
        Some(status) => status.playing,
        None => output.issued.contains(&id),
    }
}

/// The loops the local client plays, where they play from now.
pub(crate) fn loop_rows(server: &mut World) -> Vec<(crate::ScriptModelId, String, [f32; 3])> {
    let Some(mut client) = server.get_resource_mut::<ClientScripts>() else {
        return crate::script::host::client_scripts::loop_rows(server);
    };
    let world = &mut client.world;
    let loops: Vec<(LoopSource, ClientLoop)> = world
        .resource::<ClientOutput>()
        .loops
        .iter()
        .map(|(source, sound)| (source.clone(), sound.clone()))
        .collect();
    let listener = world.resource::<ClientView>().locals[0].player.eye;
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
                LoopSource::Line(_, start, end) => {
                    nearest_on_segment(listener, start.map(f32::from_bits), end.map(f32::from_bits))
                }
            };
            Some((
                crate::ScriptModelId::from_wire(CLIENT_LOOP_OWNER | sound.serial),
                sound.alias.clone(),
                origin,
            ))
        })
        .collect()
}

/// A server entity mirrored into the client's snapshot (`0..0x3ff`).
#[derive(Clone, Debug)]
struct MirroredEntity {
    client_object: u64,
    flags: u16,
    has_eyes: bool,
    spawned_callback: bool,
    /// Its origin in the last snapshot.
    origin: [f32; 3],
}

/// The speed of each mirrored entity between its last two snapshots, in
/// units per second, by client entity.
#[derive(Resource, Default)]
pub(crate) struct SnapshotSpeeds(pub(crate) std::collections::BTreeMap<u64, f32>);

/// What the client sees of a server entity in its snapshot.
#[derive(Clone, Debug)]
struct ServerEntitySnapshot {
    server_id: u64,
    number: i32,
    kind: EntityKind,
    classname: Arc<str>,
    ent_type: &'static str,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Value,
    species: Option<&'static str>,
    is_dog: Option<bool>,
    weapon: Option<String>,
    targetname: Option<Value>,
    client_flags: u16,
    has_eyes: bool,
    hidden: bool,
}

/// The client VM: its world, and the entries it starts with once the
/// client has a snapshot.
#[derive(Resource)]
pub(crate) struct ClientScripts {
    world: World,
    entries: Vec<String>,
    callbacks: Option<game_api::ClientCallbacks>,
    mirrored: std::collections::BTreeMap<u64, MirroredEntity>,
    /// The server entities the client's own models draw through, by client
    /// entity.
    twins: std::collections::BTreeMap<u64, Twin>,
    /// Tag effects waiting for a twin made this frame to have its model.
    pending_tag_events: Vec<ClientTagEvent>,
    started: bool,
    stopped: bool,
}

/// A server entity that draws a client entity's model.
#[derive(Clone, Copy, Debug)]
struct Twin {
    server: u64,
    /// Whether the client made it (a model it spawned), rather than a
    /// client-only map model's own server entity.
    made: bool,
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
        level.entity_fields.as_ref(),
    )?;
    let local = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let object = runtime
            .create_player(0)
            .map_err(|m| client_fault("install", m))?;
        runtime.set_object_field(object, "origin", Value::Vector([0.0; 3]));
        runtime.set_object_field(object, "angles", Value::Vector([0.0; 3]));
        runtime.set_object_field(object, "type", Value::string("player"));
        runtime.set_object_field(object, "species", Value::string("human"));
        runtime.set_object_field(object, "model", Value::string(""));
        runtime.set_object_field(object, "serverflags", Value::Int(0));
        LocalClient {
            object,
            has_snapshot: false,
            player: LocalPlayerView::default(),
        }
    };
    world.insert_resource(ClientView {
        locals: vec![local],
        host: true,
    });
    world.insert_resource(ClientOutput::default());
    world.insert_resource(ClientSoundReports::default());
    world.insert_resource(ClientEffectTable {
        lives: level.effect_lives.clone(),
        ..Default::default()
    });
    world.insert_resource(ClientLights {
        map: level.primary_lights.clone(),
        ..Default::default()
    });
    server.insert_resource(ClientScripts {
        world,
        entries,
        callbacks,
        mirrored: Default::default(),
        twins: Default::default(),
        pending_tag_events: Vec::new(),
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
            for (name, value) in &server.resource::<MatchScript>().dvars {
                client
                    .world
                    .resource_mut::<MatchScript>()
                    .dvars
                    .insert(name.clone(), value.clone());
            }
            sync_effect_table(server, &mut client.world);
            let player = local_player_view(server);
            client.world.resource_mut::<ClientView>().locals[0].player = player;
            let snapshots = collect_server_entities(server);
            run_frame(&mut client, tick, reason, messages, snapshots);
            let commands =
                std::mem::take(&mut client.world.resource_mut::<ClientOutput>().sound_commands);
            let mut frame = crate::frame::FrameWorld::from_world(server);
            for command in commands {
                frame.push_script_audio(command);
            }
            let made = sync_client_models(server, &mut client);
            deliver_tag_effects(server, &mut client, &made);
        }
    }
    server.insert_resource(client);
}

/// The client sees the effects the server registered.
fn sync_effect_table(server: &World, client: &mut World) {
    let precached = &server.resource::<MatchScript>().precached;
    let count = precached.keys().filter(|(kind, _)| *kind == "fx").count();
    let mut table = client.resource_mut::<ClientEffectTable>();
    if table.names.len() == count {
        return;
    }
    table.names.clear();
    table.indices.clear();
    for ((kind, name), &index) in precached {
        if *kind == "fx" {
            table.names.insert(index, name.clone());
            table
                .indices
                .entry(name.to_ascii_lowercase())
                .or_insert(index);
        }
    }
}

/// Plays and stops the client's tag effects on the server's entities the
/// client sees, for the local client only; an effect on a client-only
/// entity plays at its origin.
fn deliver_tag_effects(
    server: &mut World,
    client: &mut ClientScripts,
    made: &std::collections::BTreeSet<u64>,
) {
    let mut events = std::mem::take(&mut client.pending_tag_events);
    events.extend(std::mem::take(
        &mut client.world.resource_mut::<ClientOutput>().tag_events,
    ));
    for event in events {
        let effect = &event.effect;
        // A twin made this frame has its model from the server's next
        // frame on.
        if made.contains(&effect.entity) {
            client.pending_tag_events.push(event);
            continue;
        }
        let mirrored = client
            .mirrored
            .iter()
            .find(|(_, entry)| entry.client_object == effect.entity)
            .map(|(server_id, _)| *server_id)
            .or_else(|| client.twins.get(&effect.entity).map(|twin| twin.server));
        let audience = crate::EventAudience::Client(crate::ClientId(0));
        let result = match mirrored {
            Some(server_id) => super::host::natives::engine::effect_on_tag(
                server,
                &effect.name,
                &Value::Object(server_id),
                &effect.tag,
                event.stop,
                audience,
            ),
            None if event.stop => Ok(()),
            None => {
                let origin = match client
                    .world
                    .resource_mut::<RoundScript>()
                    .object_field(effect.entity, "origin")
                {
                    Value::Vector(origin) => origin,
                    _ => continue,
                };
                let index =
                    crate::frame::FrameWorld::from_world(server).effect_name_index(&effect.name);
                super::host::natives::engine::play_fx_to(
                    server,
                    audience,
                    index,
                    origin,
                    [0.0, 0.0, 1.0],
                    [0.0; 3],
                );
                Ok(())
            }
        };
        if let Err(fault) = result {
            diag::warn!(Sim, "csc: tag effect {} not played: {fault}", effect.name);
        }
    }
}

/// The client's own models draw through twins in the server's world: a
/// client-only map model's own server entity, or one made for a model the
/// client spawned (no-solid, client-only, never mirrored back). Each frame
/// a twin takes its client entity's origin, angles, model and visibility.
/// Answers the client entities whose twins were made this frame.
fn sync_client_models(
    server: &mut World,
    client: &mut ClientScripts,
) -> std::collections::BTreeSet<u64> {
    struct Row {
        object: u64,
        presence: Option<crate::ScriptModelId>,
        origin: [f32; 3],
        angles: [f32; 3],
        model: Value,
        hidden: bool,
    }
    let rows: Vec<Row> = {
        let mut runtime = client.world.resource_mut::<RoundScript>();
        let own: Vec<(u64, Option<crate::ScriptModelId>, bool)> = runtime
            .entities
            .iter()
            .filter(|(_, e)| e.kind != EntityKind::HudElem && e.number >= CLIENT_ENTITY_BASE)
            .map(|(id, e)| (*id, e.presence, e.hidden))
            .collect();
        own.into_iter()
            .map(|(object, presence, hidden)| {
                let vector =
                    |runtime: &mut RoundScript, field| match runtime.object_field(object, field) {
                        Value::Vector(v) => v,
                        _ => [0.0; 3],
                    };
                Row {
                    object,
                    presence,
                    origin: vector(&mut runtime, "origin"),
                    angles: vector(&mut runtime, "angles"),
                    model: runtime.object_field(object, "model"),
                    hidden,
                }
            })
            .collect()
    };
    let alive: std::collections::BTreeSet<u64> = rows.iter().map(|row| row.object).collect();
    let gone: Vec<(u64, Twin)> = client
        .twins
        .iter()
        .filter(|(object, _)| !alive.contains(object))
        .map(|(object, twin)| (*object, *twin))
        .collect();
    for (object, twin) in gone {
        client.twins.remove(&object);
        let mut runtime = server.resource_mut::<RoundScript>();
        if twin.made {
            runtime.delete_entity(twin.server);
        } else if let Some(entity) = runtime.entities.get_mut(&twin.server) {
            entity.hidden = true;
        }
    }
    let mut made = std::collections::BTreeSet::new();
    for row in rows {
        let twin = match client.twins.get(&row.object) {
            Some(twin) => *twin,
            None => {
                let twin = if let Some(presence) = row.presence {
                    let Some(server_id) = server.resource::<RoundScript>().presented_by(presence)
                    else {
                        continue;
                    };
                    Twin {
                        server: server_id,
                        made: false,
                    }
                } else if matches!(&row.model, Value::String(m) if !m.is_empty() && !m.starts_with('*'))
                {
                    match make_twin(server, row.origin) {
                        Ok(server_id) => {
                            made.insert(row.object);
                            Twin {
                                server: server_id,
                                made: true,
                            }
                        }
                        Err(fault) => {
                            diag::warn!(Sim, "csc: client model not drawn: {fault}");
                            continue;
                        }
                    }
                } else {
                    continue;
                };
                client.twins.insert(row.object, twin);
                twin
            }
        };
        let mut runtime = server.resource_mut::<RoundScript>();
        runtime.set_object_field(twin.server, "origin", Value::Vector(row.origin));
        runtime.set_object_field(twin.server, "angles", Value::Vector(row.angles));
        runtime.set_object_field(twin.server, "model", row.model);
        if let Some(entity) = runtime.entities.get_mut(&twin.server) {
            entity.hidden = row.hidden;
        }
    }
    made
}

/// The first client-only entity number.
const CLIENT_ENTITY_BASE: i32 = 0x400;

/// A server script model for a model the client spawned.
fn make_twin(server: &mut World, origin: [f32; 3]) -> Result<u64, String> {
    let presence = super::host::presence::spawn_presence(server, origin)?;
    let number = crate::frame::FrameWorld::from_world(server).gentity_number(presence);
    let mut runtime = server.resource_mut::<RoundScript>();
    let id = runtime.create_entity(EntityKind::Spawned, "")?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
    if let Some(entity) = runtime.entities.get_mut(&id) {
        entity.presence = Some(presence);
        entity.client_only = true;
        entity.solid = false;
        if let Some(number) = number {
            entity.number = number;
        }
    }
    Ok(id)
}

/// Row ids of the client's point effects and effect entities, apart from
/// the server's effect entities.
const CLIENT_EFFECT_ROW: u32 = 0x8000_0000;
const CLIENT_EFFECT_ENTITY_ROW: u32 = 0xc000_0000;

/// The local client's latest vision calls, by serial.
pub(crate) fn vision_calls(server: &World) -> Vec<(u32, crate::VisionCall)> {
    server
        .get_resource::<ClientScripts>()
        .map_or_else(Vec::new, |client| {
            let output = client.world.resource::<ClientOutput>();
            output.vision_calls.iter().cloned().collect()
        })
}

/// The effects the local client plays at points now, begun and alive, by
/// their row ids.
pub(crate) fn effect_rows(server: &World, now: i32) -> Vec<(u32, ClientEffect)> {
    let Some(client) = server.get_resource::<ClientScripts>() else {
        return Vec::new();
    };
    let output = client.world.resource::<ClientOutput>();
    let playing = |effect: &&ClientEffect| effect.begin_ms.is_some() && now <= effect.end_ms;
    let points = output
        .effects
        .iter()
        .map(|(handle, effect)| (CLIENT_EFFECT_ROW | (*handle as u32 & 0x3fff_ffff), effect));
    let entities = output.fx_entities.iter().map(|(id, effect)| {
        (
            CLIENT_EFFECT_ENTITY_ROW | (*id as u32 & 0x3fff_ffff),
            effect,
        )
    });
    points
        .chain(entities)
        .filter(|(_, effect)| playing(effect))
        .map(|(id, effect)| (id, effect.clone()))
        .collect()
}

fn collect_server_entities(server: &mut World) -> Vec<ServerEntitySnapshot> {
    let mut snapshots = Vec::new();
    let ids: Vec<(u64, super::host::entities::ScriptEntity)> = server
        .resource::<RoundScript>()
        .entities
        .iter()
        .map(|(id, e)| (*id, e.clone()))
        .collect();
    for (id, entity) in ids {
        let (is_world, turret, plane, flags, has_eyes, transmit_tn) = {
            let runtime = server.resource::<RoundScript>();
            (
                runtime.engine.world == Some(id),
                runtime.engine.turrets.contains_key(&id),
                runtime.planes.contains_key(&id),
                runtime.t5.client_flags.get(&id).copied().unwrap_or(0),
                runtime.t5.has_eyes.contains(&id),
                runtime.t5.transmit_targetname.contains(&id),
            )
        };
        if is_world || entity.client_only {
            continue;
        }
        let ent_type = match entity.kind {
            EntityKind::Player => "player",
            EntityKind::Actor => "actor",
            EntityKind::Corpse { .. } | EntityKind::ActorCorpse => "NA",
            EntityKind::Missile(_) => "missile",
            EntityKind::Vehicle => {
                if plane {
                    "plane"
                } else {
                    "vehicle"
                }
            }
            EntityKind::Map | EntityKind::Spawned => {
                if turret {
                    "mg42"
                } else if matches!(&*entity.classname, "script_model" | "script_brushmodel")
                    || (&*entity.classname == "script_origin"
                        && (entity.kind == EntityKind::Spawned
                            || transmit_tn
                            || flags != 0
                            || entity.linked_to.is_some()
                            || entity.loop_sound.is_some()))
                {
                    "scriptmover"
                } else {
                    continue;
                }
            }
            EntityKind::HudElem | EntityKind::Item(_) => continue,
        };
        let (mut origin, mut angles, model, species, is_dog, targetname) = {
            let mut runtime = server.resource_mut::<RoundScript>();
            let origin = match runtime.object_field(id, "origin") {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            let angles = match runtime.object_field(id, "angles") {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            let model = match runtime.object_field(id, "model") {
                Value::String(s) if !s.starts_with('*') => Value::String(s),
                _ => Value::string(""),
            };
            let (species, is_dog) = match entity.kind {
                EntityKind::Player => (Some("human"), None),
                EntityKind::Actor | EntityKind::Corpse { .. } => {
                    let sp = match runtime.object_field(id, "type") {
                        Value::String(s) => match &*s {
                            "human" => "human",
                            "dog" => "dog",
                            "zombie" => "zombie",
                            "zombie_dog" => "zombie_dog",
                            _ => "unknown",
                        },
                        _ => "unknown",
                    };
                    (Some(sp), Some(matches!(sp, "dog" | "zombie_dog")))
                }
                _ => (None, None),
            };
            let targetname = if transmit_tn {
                match runtime.object_field(id, "targetname") {
                    Value::String(s) if !s.is_empty() => Some(Value::String(s)),
                    _ => None,
                }
            } else {
                None
            };
            (origin, angles, model, species, is_dog, targetname)
        };
        if entity.kind == EntityKind::Player
            && let Some(ps) =
                crate::frame::player_ref(server, crate::ClientId(entity.number as u32))
        {
            origin = ps.origin;
            angles = ps.viewangles;
        }
        let weapon = if matches!(entity.kind, EntityKind::Missile(_)) {
            let frame = crate::frame::FrameWorld::from_world(server);
            frame
                .projectile_by_number(entity.number)
                .map(|p| crate::script_player::weapon_name(&frame, p.weapon).to_owned())
        } else {
            None
        };
        snapshots.push(ServerEntitySnapshot {
            server_id: id,
            number: entity.number,
            kind: entity.kind,
            classname: entity.classname,
            ent_type,
            origin,
            angles,
            model,
            species,
            is_dog,
            weapon,
            targetname,
            client_flags: flags,
            has_eyes,
            hidden: entity.hidden,
        });
    }
    snapshots
}

fn run_frame(
    client: &mut ClientScripts,
    tick: crate::Tick,
    reason: crate::StepReason,
    messages: Vec<ClientMessage>,
    snapshots: Vec<ServerEntitySnapshot>,
) {
    for local in &mut client.world.resource_mut::<ClientView>().locals {
        local.has_snapshot = true;
    }
    client
        .world
        .insert_resource(crate::step::StepRequest::frame(tick, reason));
    if !client.started {
        client.started = true;
        for entry in &client.entries {
            if let Err(fault) =
                super::runtime::start(&mut client.world, entry, Value::level(), Vec::new())
            {
                diag::warn!(Sim, "csc: client script {entry} not started: {fault}");
            }
        }
        // The client game connects its local client right after the level's
        // main has run to its first wait.
        if let Some(callbacks) = client.callbacks {
            start_callback(
                &mut client.world,
                callbacks.local_client_connect,
                Value::level(),
                vec![Value::Int(0)],
            );
        }
    }
    sync_snapshot_entities(client, snapshots, tick);
    let world = &mut client.world;
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
            // Black Ops calls these for the first person, never quiet.
            ClientMessage::JumpLand(event) => {
                let player = Value::Object(world.resource::<ClientView>().locals[0].object);
                match event {
                    game_api::movement::JumpLand::Jump(surface) => (
                        callbacks.player_jump,
                        vec![
                            Value::Int(0),
                            player,
                            Value::string(surface),
                            Value::Int(1),
                            Value::Int(0),
                        ],
                    ),
                    game_api::movement::JumpLand::Land(surface, hurt) => (
                        callbacks.player_land,
                        vec![
                            Value::Int(0),
                            player,
                            Value::string(surface),
                            Value::Int(1),
                            Value::Int(0),
                            Value::Int(hurt.into()),
                        ],
                    ),
                    game_api::movement::JumpLand::Foliage => continue,
                }
            }
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
    let now = crate::level_time_ms(tick);
    super::host::mechanics::advance_client_mechanics(world, i64::from(now));
    let mut output = world.resource_mut::<ClientOutput>();
    output.effects.retain(|_, effect| now <= effect.end_ms);
    output.tag_effects.retain(|_, effect| now <= effect.end_ms);
    world.remove_resource::<crate::step::StepRequest>();
    if let Some(fault) = world.resource::<RoundScript>().fault.clone() {
        diag::warn!(Sim, "csc: client scripts stopped: {fault}");
        client.stopped = true;
    }
}

fn sync_snapshot_entities(
    client: &mut ClientScripts,
    snapshots: Vec<ServerEntitySnapshot>,
    tick: crate::Tick,
) {
    let now = (u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS)) as i64;
    let local_player_0 = client.world.resource::<ClientView>().locals[0].object;
    let present: std::collections::BTreeSet<u64> = snapshots.iter().map(|s| s.server_id).collect();
    let stale_ids: Vec<u64> = client
        .mirrored
        .keys()
        .copied()
        .filter(|id| !present.contains(id))
        .collect();
    let mut removed = Vec::with_capacity(stale_ids.len());
    for id in stale_ids {
        if let Some(entry) = client.mirrored.remove(&id) {
            if entry.client_object == local_player_0 {
                shutdown_mirrored(client, entry, local_player_0, now);
            } else {
                removed.push(entry);
            }
        }
    }

    for snap in &snapshots {
        let (client_object, new_ent, prev_flags) =
            if let Some(entry) = client.mirrored.get_mut(&snap.server_id) {
                let prev = entry.flags;
                entry.flags = snap.client_flags;
                let d = std::array::from_fn::<f32, 3, _>(|i| snap.origin[i] - entry.origin[i]);
                let speed = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                    / crate::MATCH_TICK_MS as f32
                    * 1000.0;
                entry.origin = snap.origin;
                let object = entry.client_object;
                client
                    .world
                    .get_resource_or_insert_with(SnapshotSpeeds::default)
                    .0
                    .insert(object, speed);
                (object, false, prev)
            } else {
                let object = if snap.kind == EntityKind::Player && snap.number == 0 {
                    local_player_0
                } else {
                    let mut runtime = client.world.resource_mut::<RoundScript>();
                    let saved_next = runtime.next_entity_number;
                    let Ok(id) = runtime.create_entity(snap.kind.clone(), &snap.classname) else {
                        continue;
                    };
                    runtime.next_entity_number = saved_next;
                    if let Some(entity) = runtime.entities.get_mut(&id) {
                        entity.number = snap.number;
                    }
                    id
                };
                client.mirrored.insert(
                    snap.server_id,
                    MirroredEntity {
                        client_object: object,
                        flags: snap.client_flags,
                        has_eyes: false,
                        spawned_callback: false,
                        origin: snap.origin,
                    },
                );
                (object, true, 0)
            };

        {
            let mut runtime = client.world.resource_mut::<RoundScript>();
            runtime.set_object_field(client_object, "origin", Value::Vector(snap.origin));
            runtime.set_object_field(client_object, "angles", Value::Vector(snap.angles));
            runtime.set_object_field(client_object, "type", Value::string(snap.ent_type));
            runtime.set_object_field(client_object, "model", snap.model.clone());
            runtime.set_object_field(
                client_object,
                "serverflags",
                Value::Int(i32::from(snap.client_flags)),
            );
            runtime.set_object_field(
                client_object,
                "haseyes",
                Value::Int(i32::from(snap.has_eyes)),
            );
            if let Some(species) = snap.species {
                runtime.set_object_field(client_object, "species", Value::string(species));
            }
            if let Some(is_dog) = snap.is_dog {
                runtime.set_object_field(client_object, "isdog", Value::Int(i32::from(is_dog)));
            }
            if let Some(weapon) = &snap.weapon {
                runtime.set_object_field(client_object, "weapon", Value::string(weapon));
            }
            if let Some(targetname) = &snap.targetname {
                runtime.set_object_field(client_object, "targetname", targetname.clone());
            }
        }

        if let Some(callbacks) = client.callbacks {
            if new_ent {
                for bit in 0..16 {
                    if (snap.client_flags & (1 << bit)) != 0 {
                        start_callback(
                            &mut client.world,
                            callbacks.client_flag,
                            Value::Object(client_object),
                            vec![Value::Int(0), Value::Int(bit), Value::Int(1), Value::Int(1)],
                        );
                    }
                }
                if snap.client_flags != 0 {
                    start_callback(
                        &mut client.world,
                        callbacks.client_flag_as_val,
                        Value::Object(client_object),
                        vec![Value::Int(0), Value::Int(i32::from(snap.client_flags))],
                    );
                }
            } else if snap.client_flags != prev_flags {
                let diff = snap.client_flags ^ prev_flags;
                for bit in 0..16 {
                    if (diff & (1 << bit)) != 0 {
                        let set = i32::from((snap.client_flags & (1 << bit)) != 0);
                        start_callback(
                            &mut client.world,
                            callbacks.client_flag,
                            Value::Object(client_object),
                            vec![
                                Value::Int(0),
                                Value::Int(bit),
                                Value::Int(set),
                                Value::Undefined,
                            ],
                        );
                    }
                }
                start_callback(
                    &mut client.world,
                    callbacks.client_flag_as_val,
                    Value::Object(client_object),
                    vec![Value::Int(0), Value::Int(i32::from(snap.client_flags))],
                );
            }
        }
    }

    for entry in removed {
        shutdown_mirrored(client, entry, local_player_0, now);
    }

    for snap in &snapshots {
        if snap.hidden {
            continue;
        }
        let Some(entry) = client.mirrored.get_mut(&snap.server_id) else {
            continue;
        };
        let client_object = entry.client_object;
        let spawn_cb = if !entry.spawned_callback {
            match snap.ent_type {
                "actor" | "missile" | "vehicle" | "plane" => {
                    entry.spawned_callback = true;
                    client.callbacks.map(|cb| cb.entity_spawned)
                }
                "player" => {
                    entry.spawned_callback = true;
                    client.callbacks.map(|cb| cb.player_spawned)
                }
                "scriptmover" => {
                    entry.spawned_callback = true;
                    None
                }
                _ => None,
            }
        } else {
            None
        };
        let eye_cb = if snap.ent_type == "actor"
            && snap.is_dog == Some(false)
            && entry.has_eyes != snap.has_eyes
        {
            entry.has_eyes = snap.has_eyes;
            client.callbacks.map(|cb| (cb.zombie_eye, snap.has_eyes))
        } else {
            None
        };
        if let Some(function) = spawn_cb {
            start_callback(
                &mut client.world,
                function,
                Value::Object(client_object),
                vec![Value::Int(0)],
            );
        }
        if let Some((function, has_eyes)) = eye_cb {
            start_callback(
                &mut client.world,
                function,
                Value::Object(client_object),
                vec![Value::Int(0), Value::Int(i32::from(has_eyes))],
            );
        }
    }
}

fn shutdown_mirrored(
    client: &mut ClientScripts,
    entry: MirroredEntity,
    local_player_0: u64,
    now: i64,
) {
    client
        .world
        .resource_mut::<ClientOutput>()
        .loops
        .remove(&LoopSource::Entity(entry.client_object));
    if entry.spawned_callback {
        super::runtime::notify_now(
            &mut client.world,
            Value::Object(entry.client_object),
            "entityshutdown",
            Vec::new(),
            now,
        );
        if let Some(callbacks) = client.callbacks {
            start_callback(
                &mut client.world,
                callbacks.entity_shutdown,
                Value::level(),
                vec![Value::Int(0), Value::Object(entry.client_object)],
            );
        }
    }
    if let Some(mut speeds) = client.world.get_resource_mut::<SnapshotSpeeds>() {
        speeds.0.remove(&entry.client_object);
    }
    if entry.client_object != local_player_0 {
        client
            .world
            .resource_mut::<RoundScript>()
            .delete_entity(entry.client_object);
    }
}

fn start_callback(world: &mut World, function: &str, receiver: Value, args: Vec<Value>) {
    if let Err(fault) = super::runtime::start(world, function, receiver, args) {
        diag::warn!(Sim, "csc: {function} not started: {fault}");
    }
}
