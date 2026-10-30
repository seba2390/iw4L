use super::super::args::{arg, float, int, kind, optional, string, vector};
use super::super::arrays::{array_values, new_array};
use super::super::entities::EntityKind;
use super::iw4::atof;
use super::shared::{sight, weapon_facts};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{Arc, BTreeMap, NativeRegistry, RoundScript, StringTable, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

/// Which blood effect a bullet impact on a flesh entity plays (`bloodimpact`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BloodImpact {
    #[default]
    Normal,
    None,
    Hero,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct T5Ragdoll {
    pub(crate) launched: bool,
    pub(crate) velocity: [f32; 3],
    pub(crate) angular_vel: [f32; 3],
    pub(crate) pitch_fold: f32,
    pub(crate) roll_fold: f32,
    pub(crate) hitloc: String,
    pub(crate) grounded: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct T5State {
    influencers: BTreeMap<i32, Influencer>,
    next_influencer: i32,
    spawn_points: BTreeMap<Arc<str>, Vec<Value>>,
    randomness: f32,
    base_weights: Vec<(i32, [f32; 3], f32, f32)>,
    pub(crate) client_flags: BTreeMap<u64, u16>,
    pub(crate) has_eyes: std::collections::BTreeSet<u64>,
    pub(crate) transmit_targetname: std::collections::BTreeSet<u64>,
    pub(crate) blood_impact: BTreeMap<u64, BloodImpact>,
    pub(crate) ai_event_listeners: Vec<(u64, u32)>,
    pub(crate) ragdolls: BTreeMap<u64, T5Ragdoll>,
    pub(crate) corpse_slots: Vec<u64>,
    pub(crate) death_contents: BTreeMap<u64, i32>,
    pub(crate) spawned_dyn_ents: std::collections::VecDeque<(u32, crate::SpawnedDynEntLaunch)>,
    pub(crate) next_spawned_dyn_ent_serial: u32,
    pub(crate) dyn_ent_cleanup_serial: u32,
    dstats: BTreeMap<(u32, String), Value>,
    /// Each player's score multiplier (`setscoremultiplier`), by client.
    score_multipliers: BTreeMap<u32, i32>,
    /// Entities taken out of player collision (`setplayercollision`).
    pub(crate) no_player_collision: std::collections::BTreeSet<u64>,
    statuses: std::collections::BTreeSet<(u32, &'static str)>,
    client_systems: Vec<Arc<str>>,
    spawn_ids: BTreeMap<u32, i32>,
}

impl T5State {
    pub(crate) fn roots(&self, pending: &mut Vec<Value>) {
        pending.extend(self.spawn_points.values().flatten().cloned());
        pending.extend(self.dstats.values().cloned());
    }
}

#[derive(Clone, Debug)]
struct Influencer {
    shape: Shape,
    origin: [f32; 3],
    score: f32,
    team_mask: i32,
    curve: i32,
    expires_ms: Option<i64>,
    owner: Option<u64>,
    enabled: bool,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Sphere {
        radius: f32,
    },
    Cylinder {
        forward: [f32; 3],
        radius: f32,
        length: f32,
    },
}

const CURVE_LINEAR: i32 = 1;
const CURVE_STEEP: i32 = 2;
const CURVE_INVERSE_LINEAR: i32 = 3;
const CURVE_NEGATIVE_TO_POSITIVE: i32 = 4;

const TEAM_MASK_FREE: i32 = 1;
const TEAM_MASK_AXIS: i32 = 2;
const TEAM_MASK_ALLIES: i32 = 4;

const STATS_TABLE: &str = "mp/statstable.csv";
const STATS_REFERENCE: usize = 4;
const STATS_CLASSES: usize = 11;
const STATS_SLOT: usize = 13;

fn t5(world: &mut World) -> &mut T5State {
    &mut world.resource_mut::<RoundScript>().into_inner().t5
}

fn now_ms(world: &World) -> i64 {
    super::super::players::now_ms(world)
}

fn team_mask(team: &str) -> i32 {
    match team {
        "axis" => TEAM_MASK_AXIS,
        "allies" => TEAM_MASK_ALLIES,
        _ => TEAM_MASK_FREE,
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn axes(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

fn curve_weight(curve: i32, fraction: f32) -> f32 {
    let fraction = fraction.clamp(0.0, 1.0);
    match curve {
        CURVE_LINEAR => 1.0 - fraction,
        CURVE_STEEP => (1.0 - fraction) * (1.0 - fraction),
        CURVE_INVERSE_LINEAR => fraction,
        CURVE_NEGATIVE_TO_POSITIVE => 2.0 * fraction - 1.0,
        _ => 1.0,
    }
}

impl Influencer {
    fn score_at(&self, point: [f32; 3]) -> Option<f32> {
        let offset = sub(point, self.origin);
        let fraction = match self.shape {
            Shape::Sphere { radius } => {
                let distance = length(offset);
                (distance <= radius).then(|| distance / radius.max(1.0))?
            }
            Shape::Cylinder {
                forward,
                radius,
                length: reach,
            } => {
                let along = dot(offset, forward);
                if !(0.0..=reach).contains(&along) {
                    return None;
                }
                let across = length(sub(offset, forward.map(|f| f * along)));
                (across <= radius).then(|| along / reach.max(1.0))?
            }
        };
        Some(self.score * curve_weight(self.curve, fraction))
    }
}

fn origin_of(world: &mut World, value: &Value) -> [f32; 3] {
    vector_field(world, value, "origin")
}

fn vector_field(world: &mut World, value: &Value, name: &str) -> [f32; 3] {
    match super::super::players::entity_field(world, object_of(value).unwrap_or(0), name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn object_of(value: &Value) -> Option<u64> {
    match value {
        Value::Object(id) => Some(*id),
        _ => None,
    }
}

fn add_influencer(world: &mut World, influencer: Influencer) -> Value {
    let state = t5(world);
    let id = state.next_influencer;
    state.next_influencer += 1;
    state.influencers.insert(id, influencer);
    Value::Int(id)
}

fn timeout(world: &World, args: &[Value], index: usize) -> Result<Option<i64>, String> {
    let seconds = optional(args, index, float)?.unwrap_or(0.0);
    Ok((seconds > 0.0).then(|| now_ms(world) + (seconds * 1000.0) as i64))
}

fn settle_influencers(world: &mut World) {
    let now = now_ms(world);
    let owners: Vec<(i32, u64)> = world
        .resource::<RoundScript>()
        .t5
        .influencers
        .iter()
        .filter_map(|(id, i)| i.owner.map(|o| (*id, o)))
        .collect();
    for (id, owner) in owners {
        let live = world.resource::<RoundScript>().live(&owner);
        let origin = live.then(|| origin_of(world, &Value::Object(owner)));
        let state = t5(world);
        match origin {
            Some(origin) => state.influencers.get_mut(&id).unwrap().origin = origin,
            None => {
                state.influencers.remove(&id);
            }
        }
    }
    t5(world)
        .influencers
        .retain(|_, i| i.expires_ms.is_none_or(|at| at > now));
}

fn random_unit(world: &mut World) -> f32 {
    (super::math::random(world) % 10_000) as f32 / 10_000.0
}

fn sorted_spawn_points(
    world: &mut World,
    point_team: &str,
    influencer_team: &str,
    player: Option<u64>,
) -> Result<Value, String> {
    settle_influencers(world);
    let points = t5(world)
        .spawn_points
        .get(point_team)
        .cloned()
        .unwrap_or_else(Vec::new);
    let mask = team_mask(influencer_team);
    let mut scored = Vec::with_capacity(points.len());
    for point in points {
        let origin = origin_of(world, &point);
        let yaw = vector_field(world, &point, "angles")[1].to_radians();
        let forward = [yaw.cos(), yaw.sin(), 0.0];
        let state = &world.resource::<RoundScript>().t5;
        let mut score: f32 = state
            .influencers
            .values()
            .filter(|i| {
                i.enabled && i.team_mask & mask != 0 && (player.is_none() || i.owner != player)
            })
            .filter_map(|i| i.score_at(origin))
            .sum();
        for (weight_mask, at, facing, bonus) in &state.base_weights {
            let to = sub(*at, origin);
            let flat = length([to[0], to[1], 0.0]);
            let cos = if flat > 0.0 {
                dot(to, forward) / flat
            } else {
                1.0
            };
            if weight_mask & mask != 0 && cos >= facing.to_radians().cos() {
                score += bonus;
            }
        }
        let randomness = state.randomness;
        score += randomness * random_unit(world);
        scored.push((score, point));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    new_array(world, scored.into_iter().map(|(_, p)| p).collect())
}

fn players_on(world: &mut World, team: &str, except: Option<u32>) -> Vec<u32> {
    let clients: Vec<(u32, u64)> = world
        .resource::<RoundScript>()
        .players
        .iter()
        .filter(|(c, slot)| &*slot.sessionstate == "playing" && Some(**c) != except)
        .map(|(c, slot)| (*c, slot.object))
        .collect();
    clients
        .into_iter()
        .filter(|(_, object)| {
            team == "free"
                || super::super::players::entity_field(world, *object, "team")
                    == Value::string(team)
        })
        .map(|(c, _)| c)
        .collect()
}

fn eye(world: &mut World, client: u32) -> Option<[f32; 3]> {
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(ClientId(client))?;
    (ps.health > 0).then(|| {
        [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ]
    })
}

fn do_damage(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let amount = float(args, 0)? as i32;
    let origin = vector(args, 1)?;
    let runtime = world.resource::<RoundScript>();
    let target = match runtime.player_client_of(receiver) {
        Some(client) => crate::script::HitTarget::Player(ClientId(client)),
        None => match runtime.presence_of(receiver) {
            Some(id) => crate::script::HitTarget::Entity(id),
            None => {
                super::engine::entity_id(world, receiver)?;
                return Ok(Value::Undefined);
            }
        },
    };
    let attacker = args
        .get(2)
        .and_then(|value| runtime.player_client_of(value))
        .map(ClientId);
    let attacker_object = match args.get(2) {
        Some(Value::Object(id)) if attacker.is_none() && runtime.entities.contains_key(id) => {
            Some(*id)
        }
        _ => None,
    };
    let inflictor = args.get(3).and_then(|value| runtime.presence_of(value));
    // Singleplayer and zombie scripts pass (…, means, hitLocation) where
    // multiplayer passes (…, onHead, means, flags, weapon).
    if let Some(Value::String(means)) = args.get(4) {
        let means = super::super::entity_damage::means_named(means).unwrap_or("MOD_UNKNOWN");
        let hitloc = match args.get(5) {
            Some(Value::String(name)) => crate::script_player::hit_location_index(name),
            _ => 0,
        };
        let weapon = attacker
            .and_then(|c| FrameWorld::from_world(world).player(c).map(|ps| ps.weapon))
            .unwrap_or(0);
        apply_do_damage(
            world,
            crate::script::ScriptHit {
                piece: None,
                target,
                amount,
                origin,
                attacker,
                attacker_object,
                inflictor,
                means,
                weapon,
                flags: 0,
                hitloc,
            },
        );
        return Ok(Value::Undefined);
    }
    let on_head = optional(args, 4, int)?.unwrap_or(0) != 0;
    let means = optional(args, 5, string)?
        .map(|name| super::super::entity_damage::means_named(&name))
        .transpose()?
        .unwrap_or("MOD_UNKNOWN");
    let flags = optional(args, 6, int)?.unwrap_or(0);
    let weapon = match optional(args, 7, string)? {
        Some(name) if name != "none" => {
            crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)?
        }
        _ => attacker
            .and_then(|c| FrameWorld::from_world(world).player(c).map(|ps| ps.weapon))
            .unwrap_or(0),
    };
    let hitloc = if on_head {
        crate::script_player::hit_location_index("head")
    } else {
        0
    };
    apply_do_damage(
        world,
        crate::script::ScriptHit {
            piece: None,
            target,
            amount,
            origin,
            attacker,
            attacker_object,
            inflictor,
            means,
            weapon,
            flags,
            hitloc,
        },
    );
    Ok(Value::Undefined)
}

fn apply_do_damage(world: &mut World, hit: crate::script::ScriptHit) {
    if let crate::script::HitTarget::Entity(target) = hit.target
        && hit.piece.is_none()
    {
        let (inflictor, at) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let inflictor = hit.inflictor.and_then(|id| runtime.presented_by(id));
            let at = match runtime
                .presented_by(target)
                .map(|o| runtime.object_field(o, "origin"))
            {
                Some(Value::Vector(at)) => at,
                _ => hit.origin,
            };
            (inflictor, at)
        };
        let hit_location = crate::script_player::hit_location_name(hit.hitloc);
        super::super::entity_damage::damage_entity(
            world,
            &super::super::entity_damage::EntityHit {
                target,
                amount: hit.amount,
                attacker: hit.attacker,
                attacker_object: hit.attacker_object,
                means: hit.means,
                weapon: hit.weapon,
                point: hit.origin,
                dir: std::array::from_fn(|i| at[i] - hit.origin[i]),
                bone: None,
                flags: hit.flags,
                hit_location: Some(hit_location),
                part: Some(hit.hitloc),
                inflictor,
                time_offset: 0,
            },
        );
        return;
    }
    world.resource_mut::<RoundScript>().hits.push(hit);
}

pub(crate) fn player_id(world: &World, receiver: &Value) -> Result<u32, String> {
    super::player::player(world, receiver)
}

/// The player's weapon activity by the game's own weapon rules.
fn weapon_activity(
    world: &mut World,
    receiver: &Value,
) -> Result<game_api::movement::WeaponActivity, String> {
    let id = ClientId(player_id(world, receiver)?);
    crate::game_move::weapon_activity(&FrameWorld::from_world(world), id)
        .ok_or_else(|| "the match runs no game weapon rules".to_owned())
}

fn stats_row(
    world: &World,
    reference: &str,
) -> Option<(Arc<BTreeMap<String, StringTable>>, usize)> {
    let tables = world.resource::<RoundScript>().tables.clone();
    let table = tables.get(&super::super::entities::table_key(STATS_TABLE))?;
    let row = (0..table.rows).find(|&row| {
        table
            .cell(row, STATS_REFERENCE)
            .is_some_and(|cell| cell.eq_ignore_ascii_case(reference))
    })?;
    Some((tables, row))
}

fn stats_cell(world: &World, reference: &str, column: usize) -> Option<String> {
    let (tables, row) = stats_row(world, reference)?;
    tables[&super::super::entities::table_key(STATS_TABLE)]
        .cell(row, column)
        .map(str::to_owned)
}

fn weapon_reference(name: &str) -> &str {
    name.strip_suffix("_mp").unwrap_or(name)
}

fn dstat_path(args: &[Value]) -> Result<String, String> {
    let mut path = String::new();
    for (i, key) in args.iter().enumerate() {
        if i > 0 {
            path.push('.');
        }
        match key {
            Value::String(s) => path.push_str(&s.to_ascii_lowercase()),
            Value::Int(n) => path.push_str(&n.to_string()),
            other => {
                return Err(format!(
                    "stat key must be a string or int, not {}",
                    kind(other)
                ));
            }
        }
    }
    if path.is_empty() {
        return Err("a stat needs a key".into());
    }
    Ok(path)
}

pub(crate) fn client_bits(world: &World, clients: impl IntoIterator<Item = u32>) -> u64 {
    let _ = world;
    clients
        .into_iter()
        .filter(|c| *c < 64)
        .fold(0, |bits, c| bits | 1 << c)
}

pub(crate) fn visibility(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut super::super::entities::ScriptEntity, u64),
) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    let everyone = client_bits(world, clients);
    let runtime = world.resource_mut::<RoundScript>().into_inner();
    change(runtime.entities.get_mut(&id).unwrap(), everyone);
    Ok(Value::Undefined)
}

/// A spawned `trigger_radius_use`'s size (Black Ops' spawn function).
const RADIUS_USE_RADIUS: f32 = 72.0;
const RADIUS_USE_HEIGHT: f32 = 96.0;

pub(crate) fn register(registry: &mut NativeRegistry) {
    register_script(registry);
    register_spawning(registry);
    register_player(registry);
    register_entity(registry);
    register_platform(registry);
    register_refused(registry);
    register_bots(registry);
    register_swept(registry);
    register_shellshock(registry);
    register_earthquake(registry);
}

/// `earthquake(scale, duration, origin, radius, [player])`: every client
/// receives it; a scale, msec or radius not above 0 is an error.
fn register_earthquake(registry: &mut NativeRegistry) {
    registry.register(Function, "earthquake", |world, _, args| {
        let scale = float(args, 0)?;
        let duration_ms = earthquake_ms(float(args, 1)?);
        let origin = vector(args, 2)?;
        let radius = float(args, 3)?;
        if scale <= 0.0 {
            return Err("Scale must be greater than 0".into());
        }
        if duration_ms < 1 {
            return Err("duration must be greater than 0".into());
        }
        if radius <= 0.0 {
            return Err("Radius must be greater than 0".into());
        }
        super::scene_effects::shake_camera(world, scale, duration_ms, origin, radius);
        Ok(Value::Undefined)
    });
}

/// An earthquake's seconds in msec: times 1000 as a float, to the nearest
/// (ties to even).
pub(super) fn earthquake_ms(seconds: f32) -> i32 {
    (seconds * 1000.0).round_ties_even() as i32
}

/// Black Ops' shellshocks: the shock file is read at precache, a name
/// without a file of its own playing the game's default shock.
fn register_shellshock(registry: &mut NativeRegistry) {
    registry.register(Function, "precacheshellshock", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err(
                "precacheShellShock must be called before any wait statements in the level script"
                    .into(),
            );
        }
        let name = string(args, 0)?;
        super::iw4::precache(world, "shellshock", name.clone())?;
        let frame = FrameWorld::from_world(world);
        match frame.shock_file(&name) {
            Some(file) if !file.eq_ignore_ascii_case(&name) => {
                diag::info!(
                    Sim,
                    "shellshock: couldn't open 'shock/{name}.shock', playing shock/{file}.shock"
                );
                Ok(Value::Undefined)
            }
            Some(_) => Ok(Value::Undefined),
            None => Err(format!("couldn't find shell shock '{name}' -- see console")),
        }
    });
    registry.register(Method, "shellshock", |world, receiver, args| {
        let id = ClientId(player_id(world, receiver)?);
        let name = string(args, 0)?;
        let duration = shock_milliseconds(float(args, 1)?);
        if !(0..=60_000).contains(&duration) {
            return Err(format!(
                "duration {} should be >= 0 and <= 60",
                duration as f32 * 0.001
            ));
        }
        let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        let mut frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player(id) else {
            return Ok(Value::Undefined);
        };
        // A shock replaces the running one when it is at least as long or
        // the running one is over.
        if ps.shellshock_duration > duration
            && ps.shellshock_duration > now.wrapping_sub(ps.shellshock_time)
        {
            return Ok(Value::Undefined);
        }
        let index = frame
            .ecs_ref()
            .resource::<crate::script::MatchScript>()
            .precached
            .iter()
            .find(|((kind, item), _)| *kind == "shellshock" && item.eq_ignore_ascii_case(&name))
            .map(|(_, &index)| index)
            .ok_or_else(|| format!("shellshock '{name}' was not precached"))?;
        let file = frame
            .shock_file(&name)
            .ok_or_else(|| format!("shellshock '{name}' has no shock file"))?;
        let shock = frame.shock(&file).cloned();
        for gap in frame.shock_gaps(&file).to_vec() {
            frame.report_game_gap(gap);
        }
        let start = game_api::movement::ShellshockStart {
            index,
            duration_ms: duration,
            level_time: now,
        };
        crate::game_move::shellshock(&mut frame, id, Some(start));
        if let Some(ps) = frame.player_mut(id) {
            ps.shellshock_index = index;
            ps.shellshock_time = now;
            ps.shellshock_duration = duration;
        }
        frame.client_meta_mut(id).shellshock = shock;
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopshellshock", |world, receiver, args| {
        if !args.is_empty() {
            return Err("USAGE: <player> stopshellshock()".into());
        }
        let id = ClientId(player_id(world, receiver)?);
        let mut frame = FrameWorld::from_world(world);
        crate::game_move::shellshock(&mut frame, id, None);
        if let Some(ps) = frame.player_mut(id) {
            ps.shellshock_index = 0;
            ps.shellshock_time = 0;
            ps.shellshock_duration = 0;
        }
        Ok(Value::Undefined)
    });
}

/// A script's seconds as the game's shellshock takes them: `seconds * 1000`
/// in single precision, plus 2^-30, rounded to the nearest integer.
fn shock_milliseconds(seconds: f32) -> i32 {
    let bias = f64::from(2.0f32.powi(-30));
    let rounded = (f64::from(seconds * 1000.0) + bias).round_ties_even();
    if rounded.is_finite() && rounded.abs() < 2_147_483_648.0 {
        rounded as i32
    } else {
        i32::MIN
    }
}

fn register_script(registry: &mut NativeRegistry) {
    registry.register(Function, "float", |_, _, args| {
        Ok(Value::Float(match arg(args, 0)? {
            Value::String(s) => atof(s) as f32,
            _ => float(args, 0)?,
        }))
    });
    registry.register(Function, "isarray", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::Array(_)).into()))
    });
    registry.register(Function, "vectorcross", |_, _, args| {
        Ok(Value::Vector(cross(vector(args, 0)?, vector(args, 1)?)))
    });
    registry.register(Function, "rotatepoint", |_, _, args| {
        let point = vector(args, 0)?;
        let [forward, right, up] = axes(vector(args, 1)?);
        Ok(Value::Vector(std::array::from_fn(|i| {
            forward[i] * point[0] - right[i] * point[1] + up[i] * point[2]
        })))
    });
    registry.register(Function, "isai", |world, _, args| {
        let actor = world
            .resource::<RoundScript>()
            .entity(arg(args, 0)?)
            .is_some_and(|(_, e)| e.kind == EntityKind::Actor);
        Ok(Value::Int(actor.into()))
    });
    registry.register(Function, "isvehicle", |world, _, args| {
        let vehicle = world
            .resource::<RoundScript>()
            .entity(arg(args, 0)?)
            .is_some_and(|(_, e)| e.kind == EntityKind::Vehicle);
        Ok(Value::Int(vehicle.into()))
    });
    registry.register(Function, "tablelookupcolumnforrow", |world, _, args| {
        let name = string(args, 0)?;
        let (row, column) = (int(args, 1)?, int(args, 2)?);
        let tables = world.resource::<RoundScript>().tables.clone();
        let cell = tables
            .get(&super::super::entities::table_key(&name))
            .filter(|_| row >= 0 && column >= 0)
            .and_then(|t| t.cell(row as usize, column as usize))
            .unwrap_or("");
        Ok(Value::string(cell))
    });
    registry.register(Function, "getdefaultclassslot", |world, _, args| {
        let class = string(args, 0)?.to_ascii_lowercase();
        let slot = string(args, 1)?;
        let tables = world.resource::<RoundScript>().tables.clone();
        let Some(table) = tables.get(&super::super::entities::table_key(STATS_TABLE)) else {
            return Err(format!("{STATS_TABLE} is not loaded"));
        };
        let found = (0..table.rows).find(|&row| {
            table
                .cell(row, STATS_SLOT)
                .is_some_and(|s| s.eq_ignore_ascii_case(&slot))
                && table.cell(row, STATS_CLASSES).is_some_and(|classes| {
                    classes
                        .split_whitespace()
                        .any(|c| c.eq_ignore_ascii_case(&class))
                })
        });
        Ok(Value::string(
            found
                .and_then(|row| table.cell(row, STATS_REFERENCE))
                .unwrap_or("weapon_null"),
        ))
    });
    registry.register(Function, "getbaseweaponitemindex", |world, _, args| {
        let name = string(args, 0)?;
        let base = weapon_reference(&name)
            .split('_')
            .next()
            .unwrap_or("")
            .to_owned();
        let index = stats_cell(world, &base, 0).map_or(0, |n| super::iw4::atoi(&n));
        Ok(Value::Int(index))
    });
    registry.register(Function, "getreffromitemindex", |world, _, args| {
        let index = int(args, 0)?.to_string();
        let tables = world.resource::<RoundScript>().tables.clone();
        let reference = tables
            .get(&super::super::entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(index.as_str()))
                    .and_then(|row| t.cell(row, STATS_REFERENCE))
            })
            .unwrap_or("");
        Ok(Value::string(reference))
    });
    registry.register(Function, "getitemgroupfromitemindex", |world, _, args| {
        let index = int(args, 0)?.to_string();
        let tables = world.resource::<RoundScript>().tables.clone();
        let group = tables
            .get(&super::super::entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(index.as_str()))
                    .and_then(|row| t.cell(row, 2))
            })
            .unwrap_or("");
        Ok(Value::string(group))
    });
    registry.register(Function, "getattachmentindex", |world, _, args| {
        let name = string(args, 0)?;
        let tables = world.resource::<RoundScript>().tables.clone();
        let index = tables
            .get(&super::super::entities::table_key("mp/attachmenttable.csv"))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| {
                        t.cell(row, 4)
                            .is_some_and(|c| c.eq_ignore_ascii_case(&name))
                    })
                    .and_then(|row| t.cell(row, 9))
            })
            .map_or(0, super::iw4::atoi);
        Ok(Value::Int(index))
    });
    registry.register(Function, "getitemattachment", |world, _, args| {
        let (item, slot) = (int(args, 0)?.to_string(), int(args, 1)?);
        if slot <= 0 {
            return Ok(Value::string("none"));
        }
        let tables = world.resource::<RoundScript>().tables.clone();
        let attachment = tables
            .get(&super::super::entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(item.as_str()))
                    .and_then(|row| t.cell(row, 8))
            })
            .and_then(|list| {
                list.split_whitespace()
                    .nth((slot - 1) as usize)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "none".into());
        Ok(Value::String(attachment.into()))
    });
    registry.register(Function, "getweaponindexfromname", |world, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(
            FrameWorld::from_world(world)
                .weapon_index_by_script_name(&name)
                .map_or(0, |i| i as i32),
        ))
    });
    registry.register(Function, "isweaponprimary", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int(
            (facts.inventory_type == 0 && facts.weap_class != 6).into(),
        ))
    });
    registry.register(Function, "isweaponequipment", |world, _, args| {
        let name = string(args, 0)?;
        // A weapon without a stats row has no slot and no group.
        let slot =
            stats_cell(world, weapon_reference(&name), STATS_SLOT).unwrap_or_else(String::new);
        Ok(Value::Int(slot.eq_ignore_ascii_case("equipment").into()))
    });
    registry.register(Function, "isweaponspecificuse", |world, _, args| {
        let name = string(args, 0)?;
        let group = stats_cell(world, weapon_reference(&name), 2).unwrap_or_else(String::new);
        Ok(Value::Int(group.eq_ignore_ascii_case("killstreak").into()))
    });
    registry.register(Function, "weaponreloadtime", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Float(facts.reload_time_ms as f32 / 1000.0))
    });
    registry.register(Function, "getweaponfusetime", |world, _, args| {
        let name = string(args, 0)?;
        let frame = FrameWorld::from_world(world);
        let index = frame
            .weapon_index_by_script_name(&name)
            .ok_or_else(|| format!("unknown weapon '{name}'"))?;
        Ok(Value::Int(
            frame
                .equipment_facts_for(index)
                .map_or(0, |e| e.fuse_time_ms),
        ))
    });
    registry.register(Function, "getallvehiclenodes", |world, _, _| {
        super::super::vehicles::all_nodes(world)
    });
    registry.register(Function, "getweaponstowedmodel", |world, _, args| {
        let name = string(args, 0)?;
        let frame = FrameWorld::from_world(world);
        let Some(index) = frame
            .weapon_index_by_script_name(&name)
            .filter(|&index| index != 0 && !name.eq_ignore_ascii_case("none"))
        else {
            return Ok(Value::Int(0));
        };
        let stowed = frame
            .content()
            .game_weapon(index)
            .is_some_and(|w| w.has_stowed_model);
        Ok(Value::Int(stowed.into()))
    });
    registry.register(Function, "getaitriggerflags", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getvehicletriggerflags", |_, _, _| {
        Ok(Value::Int(0))
    });
    for name in [
        "getwatcherweapons",
        "getretrievableweapons",
        "getvehicletreadfxarray",
    ] {
        registry.register(Function, name, |world, _, _| new_array(world, Vec::new()));
    }
    macro_rules! weapon_sound {
        ($($name:literal => $field:ident),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                let weapon = match arg(args, 0)? {
                    Value::String(name) => FrameWorld::from_world(world)
                        .weapon_index_by_script_name(name)
                        .ok_or_else(|| format!("unknown weapon '{name}'"))?,
                    _ => int(args, 0)? as u32,
                };
                let alias = FrameWorld::from_world(world)
                    .weapon_script_sounds(weapon)
                    .and_then(|sounds| sounds.$field.clone());
                Ok(alias.map_or(Value::Undefined, |alias| Value::String(alias.into())))
            });
        )*};
    }
    weapon_sound!(
        "getweaponfiresound" => fire,
        "getweaponfiresoundplayer" => fire_player,
        "getweaponpickupsound" => pickup,
        "getweaponpickupsoundplayer" => pickup_player,
        "getweaponprojexplosionsound" => proj_explosion,
    );
    registry.register(Method, "dodamage", do_damage);
    registry.register(Method, "setenemymodel", |world, receiver, args| {
        string(args, 0)?;
        super::engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getmaxactivecontracts", |_, _, _| {
        Ok(Value::Int(0))
    });
    registry.register(Function, "getnumgvrules", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "ispregameenabled", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "ispregamegamestarted", |_, _, _| {
        Ok(Value::Int(0))
    });
    registry.register(Function, "isdemoenabled", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isdemorecording", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isglobalstatsserver", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getgametypeenumfromname", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getplayerspawnid", |world, _, args| {
        let client = player_id(world, arg(args, 0)?)?;
        let state = t5(world);
        let next = state.spawn_ids.len() as i32;
        Ok(Value::Int(*state.spawn_ids.entry(client).or_insert(next)))
    });
    registry.register(Function, "clientsysregister", |world, _, args| {
        let name: Arc<str> = string(args, 0)?.into();
        let systems = &mut t5(world).client_systems;
        let id = systems.iter().position(|s| *s == name).unwrap_or_else(|| {
            systems.push(name);
            systems.len() - 1
        });
        Ok(Value::Int(id as i32))
    });
    // `ActivateClientExploder(id)` / `DeactivateClientExploder(id)`: sent to
    // every client, a stop as the negated id.
    registry.register(Function, "activateclientexploder", |world, _, args| {
        let id = int(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .client_messages
            .push(crate::script::client::ClientMessage::Exploder(id));
        Ok(Value::Undefined)
    });
    registry.register(Function, "deactivateclientexploder", |world, _, args| {
        let id = int(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .client_messages
            .push(crate::script::client::ClientMessage::Exploder(-id));
        Ok(Value::Undefined)
    });
    // `ClientSysSetState(index, state)`: a registered client system's new
    // state, sent to every client (the method: to that player).
    registry.register(Function, "clientsyssetstate", |world, _, args| {
        client_sys_set_state(world, None, args)
    });
    registry.register(Method, "clientsyssetstate", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        client_sys_set_state(world, Some(client), args)
    });
    registry.register(Function, "playsoundatposition", |world, _, args| {
        let alias = string(args, 0)?;
        let origin = vector(args, 1)?;
        super::engine::play_sound_at(world, origin, &alias)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getdroppedweapons", |world, _, _| {
        let items: Vec<Value> = world
            .resource::<RoundScript>()
            .entities
            .iter()
            .filter(|(_, e)| matches!(e.kind, EntityKind::Item(_)))
            .map(|(id, _)| Value::Object(*id))
            .collect();
        new_array(world, items)
    });
}

fn key_number(args: &[Value], index: usize) -> Result<f32, String> {
    match args.get(index) {
        Some(Value::String(text)) => Ok(atof(text) as f32),
        _ => float(args, index),
    }
}

fn register_spawning(registry: &mut NativeRegistry) {
    registry.register(Function, "addsphereinfluencer", |world, _, args| {
        let influencer = Influencer {
            origin: vector(args, 1)?,
            shape: Shape::Sphere {
                radius: key_number(args, 2)?,
            },
            score: key_number(args, 3)?,
            team_mask: int(args, 4)?,
            curve: optional(args, 6, int)?.unwrap_or(0),
            expires_ms: timeout(world, args, 7)?,
            owner: args.get(8).and_then(object_of),
            enabled: true,
        };
        Ok(add_influencer(world, influencer))
    });
    registry.register(Function, "addcylinderinfluencer", |world, _, args| {
        let forward = vector(args, 2)?;
        let norm = length(forward).max(f32::EPSILON);
        let influencer = Influencer {
            origin: vector(args, 1)?,
            shape: Shape::Cylinder {
                forward: forward.map(|f| f / norm),
                radius: key_number(args, 4)?,
                length: key_number(args, 5)?,
            },
            score: key_number(args, 6)?,
            team_mask: int(args, 7)?,
            curve: optional(args, 9, int)?.unwrap_or(0),
            expires_ms: timeout(world, args, 10)?,
            owner: args.get(11).and_then(object_of),
            enabled: true,
        };
        Ok(add_influencer(world, influencer))
    });
    registry.register(Function, "removeinfluencer", |world, _, args| {
        let id = int(args, 0)?;
        t5(world).influencers.remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Function, "enableinfluencer", |world, _, args| {
        let (id, on) = (int(args, 0)?, int(args, 1)? != 0);
        if let Some(i) = t5(world).influencers.get_mut(&id) {
            i.enabled = on;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setinfluencerteammask", |world, _, args| {
        let (id, mask) = (int(args, 0)?, int(args, 1)?);
        if let Some(i) = t5(world).influencers.get_mut(&id) {
            i.team_mask = mask;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "clearspawnpoints", |world, _, _| {
        t5(world).spawn_points.clear();
        Ok(Value::Undefined)
    });
    registry.register(Function, "addspawnpoints", |world, _, args| {
        let team: Arc<str> = string(args, 0)?.into();
        let points = array_values(world, arg(args, 1)?)?;
        t5(world)
            .spawn_points
            .entry(team)
            .or_default()
            .extend(points);
        Ok(Value::Undefined)
    });
    registry.register(
        Function,
        "setspawnpointrandomvariation",
        |world, _, args| {
            t5(world).randomness = float(args, 0)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Function, "setspawnpointsbaseweight", |world, _, args| {
        let mask = int(args, 0)?;
        let origin = vector(args, 1)?;
        let facing = float(args, 2)?;
        let bonus = float(args, 3)?;
        t5(world).base_weights.push((mask, origin, facing, bonus));
        Ok(Value::Undefined)
    });
    registry.register(Function, "getsortedspawnpoints", |world, _, args| {
        let point_team = string(args, 0)?;
        let influencer_team = string(args, 1)?;
        let player = args.get(2).and_then(object_of);
        sorted_spawn_points(world, &point_team, &influencer_team, player)
    });
    registry.register(Function, "isspawnpointvisible", |world, _, args| {
        let origin = vector(args, 0)?;
        let team = string(args, 2)?;
        let except = args
            .get(3)
            .and_then(|p| world.resource::<RoundScript>().player_client_of(p));
        let target = [origin[0], origin[1], origin[2] + 60.0];
        for client in players_on(world, &team, except) {
            if let Some(eye) = eye(world, client)
                && sight(world, eye, target)
            {
                return Ok(Value::Int(1));
            }
        }
        Ok(Value::Int(0))
    });
    registry.register(Function, "recordusedspawnpoint", |world, _, args| {
        player_id(world, arg(args, 0)?)?;
        Ok(Value::Undefined)
    });
}

fn register_player(registry: &mut NativeRegistry) {
    macro_rules! answers {
        ($value:expr => $($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                player_id(world, receiver)?;
                Ok($value)
            });
        )*};
    }
    answers!(Value::Int(0) => "isdemoclient", "issplitscreen", "istestclient", "isinvehicle",
        "isremotecontrolling", "isflared", "deathstreakactive", "isitemlocked",
        "depthofplayerinwater", "needsrevive");
    answers!(Value::Int(1) => "isitempurchased");
    answers!(Value::Undefined => "getvehicleoccupied");
    // `OpenMainMenu(name)` tells the player's client to open a script menu:
    // `main` makes the client's main menu active, which opens the menu of
    // that name. Answers that the client was told.
    registry.register(Method, "openmainmenu", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let menu = string(args, 0)?.to_ascii_lowercase();
        super::player::send_menu_command(world, client, crate::MenuCommandKind::Open(menu));
        Ok(Value::Int(1))
    });
    // `CloseMainMenu()` closes every menu on the client and leaves no menu
    // active; Black Ops' named form closes one menu, which no script uses.
    registry.register(Method, "closemainmenu", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        if !args.is_empty() {
            return Err("closemainmenu: closing one named menu is not supported".into());
        }
        super::player::send_menu_command(world, client, crate::MenuCommandKind::CloseInGame);
        Ok(Value::Int(1))
    });
    // Black Ops' `SetDepthOfField` checks only its distances: each is at
    // least 0, a near range that does not open turns near blur off, a far
    // range that does not open or a far blur of 0 turns far blur off, and
    // an open far range must start at or past the near end. Its blurs are
    // taken as given.
    registry.register(Method, "setdepthoffield", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        if args.len() != 6 {
            return Err("Incorrect number of parameters".into());
        }
        let mut values = [0.0; 6];
        for (i, value) in values.iter_mut().enumerate() {
            *value = super::super::args::float(args, i)?;
        }
        let [near_start, near_end, far_start, far_end, near_blur, far_blur] = values;
        for (value, what) in [
            (near_start, "near start"),
            (near_end, "near end"),
            (far_start, "far start"),
            (far_end, "far end"),
        ] {
            if value < 0.0 {
                return Err(format!("{what} must be >= 0"));
            }
        }
        let (near_start, near_end) = if near_start >= near_end {
            (0.0, 0.0)
        } else {
            (near_start, near_end)
        };
        let (far_start, far_end) = if far_start >= far_end || far_blur == 0.0 {
            (0.0, 0.0)
        } else {
            if near_end > far_start {
                return Err("far start must be >= near end, or far depth of field should be disabled with far start >= far end or far blur == 0".into());
            }
            (far_start, far_end)
        };
        let dof = crate::ScriptDepthOfField {
            near_start,
            near_end,
            far_start,
            far_end,
            near_blur,
            far_blur,
        };
        super::super::client_effects::edit_view(world, client, |view| {
            view.depth_of_field = dof
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "islocaltohost", |world, receiver, _| {
        Ok(Value::Int((player_id(world, receiver)? == 0).into()))
    });
    registry.register(Method, "isplayeronsamemachine", |world, receiver, args| {
        let (a, b) = (
            player_id(world, receiver)?,
            player_id(world, arg(args, 0)?)?,
        );
        Ok(Value::Int((a == b).into()))
    });
    registry.register(Method, "isfiring", |world, receiver, _| {
        Ok(Value::Int(weapon_activity(world, receiver)?.firing.into()))
    });
    registry.register(Method, "ismeleeing", |world, receiver, _| {
        Ok(Value::Int(
            weapon_activity(world, receiver)?.meleeing.into(),
        ))
    });
    registry.register(Method, "isthrowinggrenade", |world, receiver, _| {
        Ok(Value::Int(
            weapon_activity(world, receiver)?.throwing_grenade.into(),
        ))
    });
    registry.register(Method, "isswitchingweapons", |world, receiver, _| {
        Ok(Value::Int(
            weapon_activity(world, receiver)?.switching.into(),
        ))
    });
    registry.register(Method, "getmovespeedscale", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        Ok(Value::Float(
            FrameWorld::from_world(world)
                .player(id)
                .map_or(1.0, |ps| ps.move_speed_scale_multiplier),
        ))
    });
    registry.register(Method, "getperks", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        let perks: Vec<Value> = world
            .resource::<RoundScript>()
            .players
            .get(&client)
            .map(|slot| {
                slot.perks
                    .iter()
                    .map(|p| Value::String(p.clone().into()))
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        new_array(world, perks)
    });
    registry.register(Method, "getdstat", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let path = dstat_path(args)?;
        Ok(t5(world)
            .dstats
            .get(&(client, path))
            .cloned()
            .unwrap_or(Value::Int(0)))
    });
    // Black Ops sets the health along with the max health.
    registry.register(Method, "setmaxhealth", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let health = Value::Int(int(args, 0)?);
        super::super::players::store_field(world, client, "maxhealth", &health)?;
        super::super::players::store_field(world, client, "health", &health)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setdstat", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let (value, keys) = args
            .split_last()
            .ok_or("setdstat needs a key and a value")?;
        let path = dstat_path(keys)?;
        t5(world).dstats.insert((client, path), value.clone());
        Ok(Value::Undefined)
    });
    macro_rules! status {
        ($($name:literal => $status:literal = $on:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let client = player_id(world, receiver)?;
                let statuses = &mut t5(world).statuses;
                if $on {
                    statuses.insert((client, $status));
                } else {
                    statuses.remove(&(client, $status));
                }
                Ok(Value::Undefined)
            });
        )*};
    }
    status!(
        "stopburning" => "burning" = false,
        "startpoisoning" => "poisoned" = true,
        "stoppoisoning" => "poisoned" = false,
    );
    registry.register(Method, "ispoisoned", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        Ok(Value::Int(
            t5(world).statuses.contains(&(client, "poisoned")).into(),
        ))
    });
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                super::player::present(world, receiver, $name, args)
            });
        )*};
    }
    presented!(
        "setclientuivisibilityflag",
        "displaychallengecomplete",
        "displaycontract",
        "displayendgame",
        "displayendgamemilestone",
        "displaygamemodemessage",
        "displaykillstreak",
        "displaymedal",
        "displayrankup",
        "displayteammessage",
        "displaywagerpopup",
        "clearcenterpopups",
        "clearpopups",
        "clearendgame",
        "directionalhitindicator",
        "visionsetlerpratio",
        "setplayerrenderoptions",
        "starttanning",
        "setsprintduration",
        "setsprintcooldown",
        "setblockweaponpickup",
        "cameraactivate",
        "camerasetposition",
        "camerasetlookat",
        "setcameraspikeactive",
        "setspawnclientflag",
        "setnemesisxuid",
        "giveachievement",
        "disabledeathstreak",
        "enabledeathstreak",
        "setpregameclass",
        "setpregameteam",
        "beginlocationairstrikeselection",
        "beginlocationartilleryselection",
        "beginlocationcomlinkselection",
        "beginlocationmortarselection",
        "beginlocationnapalmselection",
    );
    registry.register(Method, "setlaststandprevweap", |world, receiver, args| {
        let _ = player_id(world, receiver)?;
        let _ = string(args, 0)?;
        Ok(Value::Undefined)
    });
    // Black Ops' handlers set and clear the player's weapon flags its
    // movement reads; nothing masks the command.
    macro_rules! weapon_flag {
        ($($name:literal => $flag:ident = $on:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let client = player_id(world, receiver)?;
                crate::game_move::set_script_flag(
                    &mut crate::frame::FrameWorld::from_world(world),
                    crate::ClientId(client),
                    game_api::movement::ScriptFlag::$flag,
                    $on,
                );
                Ok(Value::Undefined)
            });
        )*};
    }
    weapon_flag!(
        "disableweapons" => WeaponsDisabled = true,
        "enableweapons" => WeaponsDisabled = false,
        "disableoffhandweapons" => OffhandsDisabled = true,
        "enableoffhandweapons" => OffhandsDisabled = false,
    );
    // Black Ops' `Spawn` makes a `trigger_radius_use` by its spawn function:
    // 72 wide and 96 high whatever the arguments, with no cursor hint yet.
    registry.register(Function, "spawn", |world, _, args| {
        if super::super::args::string(args, 0)? != "trigger_radius_use" {
            return super::engine::spawn_entity(world, args);
        }
        let origin = super::super::args::vector(args, 1)?;
        let flags = super::super::args::optional(args, 2, super::super::args::int)?.unwrap_or(0);
        let mut runtime = world.resource_mut::<crate::script::RoundScript>();
        let id = runtime.create_entity(
            super::super::entities::EntityKind::Spawned,
            "trigger_radius_use",
        )?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        runtime.set_object_field(id, "spawnflags", Value::Int(flags));
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.cylinder = Some((RADIUS_USE_RADIUS, RADIUS_USE_HEIGHT));
            entity.usable = Some(super::super::entities::Usable {
                cursor: 0,
                hint: -1,
                enabled: true,
                barred: Default::default(),
            });
        }
        Ok(Value::Object(id))
    });
    // `SpawnCollision(model, targetname, origin, angles)`: the multiplayer
    // maps' patch scripts place collision-only models (`collision_wall_*`,
    // `collision_geo_*`) as solid script models.
    registry.register(Function, "spawncollision", |world, _, args| {
        let model = super::super::args::string(args, 0)?;
        let targetname = super::super::args::string(args, 1)?;
        let origin = super::super::args::vector(args, 2)?;
        let angles = super::super::args::vector(args, 3)?;
        let entity = super::engine::spawn_entity(
            world,
            &[Value::string("script_model"), Value::Vector(origin)],
        )?;
        let id = super::engine::entity_id(world, &entity)?;
        let mut runtime = world.resource_mut::<crate::script::RoundScript>();
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "targetname", Value::string(&targetname));
        runtime.set_object_field(id, "model", Value::string(&model));
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.solid = true;
        }
        Ok(entity)
    });
    registry.register(Method, "disableweaponcycling", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        crate::game_move::set_script_flag(
            &mut crate::frame::FrameWorld::from_world(world),
            crate::ClientId(client),
            game_api::movement::ScriptFlag::NoWeaponCycling,
            true,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableweaponcycling", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        crate::game_move::set_script_flag(
            &mut crate::frame::FrameWorld::from_world(world),
            crate::ClientId(client),
            game_api::movement::ScriptFlag::NoWeaponCycling,
            false,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "calcplayeroptions", |world, receiver, args| {
        player_id(world, receiver)?;
        let pattern = if args.len() == 1 { 0 } else { int(args, 0)? };
        Ok(Value::Int(pattern << 26))
    });
    // `calcweaponoptions`: in multiplayer, `(class, slot)` reads the custom
    // class's camo; the direct form `(camo, [lens, [reticle, [reticle_color]]])`
    // packs `(client + 1) << 21` and the 6-bit camo, 4-bit lens, 6-bit reticle
    // and 3-bit reticle colour.
    registry.register(Method, "calcweaponoptions", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        if args.len() == 2 {
            let class = crate::ClassId(int(args, 0)? as u32);
            let slot = usize::from(int(args, 1)? != 0);
            let camo = super::super::players::personal_class(world, client, class)
                .map_or(0, |class| i32::from(class.camos[slot]));
            return Ok(Value::Int(camo & 63));
        }
        let mut bits = (((client + 1) & 0x1f) << 21) | ((int(args, 0)? as u32) & 0x3f);
        if args.len() > 1 {
            bits ^= (bits ^ (((int(args, 1)? as u32) & 0xf) << 6)) & 0x3c0;
            if args.len() > 2 {
                bits ^= (bits ^ (((int(args, 2)? as u32) & 0x3f) << 10)) & 0xfc00;
                if args.len() > 3 {
                    bits ^= (bits ^ (((int(args, 3)? as u32) & 0x7) << 16)) & 0x7_0000;
                }
            }
        }
        Ok(Value::Int(bits as i32))
    });
}

/// Black Ops' height with no water under the map's water grid.
const NO_WATER_HEIGHT: f32 = -131_072.0;
/// How far above and below the point `getWaterHeight` looks for water.
const WATER_REACH: f32 = 200.0;
const CONTENTS_WATER: u32 = 0x20;

fn register_entity(registry: &mut NativeRegistry) {
    // `getWaterHeight(point)`: the top of the water found from 200 above the
    // point to 200 below it, else the map's water grid height there, which
    // is -131072 where the grid holds none (the grid is not read).
    registry.register(Function, "getwaterheight", |world, _, args| {
        let at = vector(args, 0)?;
        let start = [at[0], at[1], at[2] + WATER_REACH];
        let end = [at[0], at[1], at[2] - WATER_REACH];
        let hit = super::engine::trace(world, start, end, [0.0; 3], [0.0; 3], CONTENTS_WATER);
        let surface = (hit.fraction < 1.0).then(|| start[2] + (end[2] - start[2]) * hit.fraction);
        Ok(Value::Float(
            surface.map_or(NO_WATER_HEIGHT, |z| z.max(NO_WATER_HEIGHT)),
        ))
    });
    // `getLinkedEnt()`: the entity a player or entity is linked to.
    registry.register(Method, "getlinkedent", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let parent = match runtime.player_client(id) {
            Some(client) => runtime
                .players
                .get(&client)
                .and_then(|slot| slot.link.as_ref())
                .map(|link| link.parent),
            None => runtime
                .entities
                .get(&id)
                .and_then(|entity| entity.linked_to.as_ref())
                .map(|link| link.parent),
        };
        Ok(parent.map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "setclientflag", |world, receiver, args| {
        let (id, bit) = (super::engine::entity_id(world, receiver)?, int(args, 0)?);
        if !(0..16).contains(&bit) {
            return Err(format!(
                "SetClientFlag: Index {bit} out of range (0 - 15)\n"
            ));
        }
        *t5(world).client_flags.entry(id).or_default() |= 1 << bit;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearclientflag", |world, receiver, args| {
        let (id, bit) = (super::engine::entity_id(world, receiver)?, int(args, 0)?);
        if !(0..16).contains(&bit) {
            return Err(format!(
                "ClearClientFlag: Index {bit} out of range (0 - 15)\n"
            ));
        }
        *t5(world).client_flags.entry(id).or_default() &= !(1 << bit);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setclientflagasval", |world, receiver, args| {
        let (id, val) = (super::engine::entity_id(world, receiver)?, int(args, 0)?);
        if !(0..0x10000).contains(&val) {
            return Err(format!(
                "SetClientFlagAsVal: Index {val} out of range (0 - 15)\n"
            ));
        }
        t5(world).client_flags.insert(id, val as u16);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getclientflag", |world, receiver, args| {
        let (id, bit) = (super::engine::entity_id(world, receiver)?, int(args, 0)?);
        let flags = t5(world).client_flags.get(&id).copied().unwrap_or(0);
        Ok(Value::Int(i32::from((flags >> bit.clamp(0, 15)) & 1)))
    });
    registry.register(Method, "transmittargetname", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        t5(world).transmit_targetname.insert(id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setinvisibletoall", |world, receiver, _| {
        visibility(world, receiver, |e, _| {
            e.hidden = true;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "setvisibletoall", |world, receiver, _| {
        visibility(world, receiver, |e, _| {
            e.hidden = false;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "setvisibletoplayer", |world, receiver, args| {
        let bit = client_bits(world, [player_id(world, arg(args, 0)?)?]);
        visibility(world, receiver, |e, _| {
            if e.hidden {
                e.shown_to |= bit;
            }
        })
    });
    registry.register(Method, "setinvisibletoplayer", |world, receiver, args| {
        let bit = client_bits(world, [player_id(world, arg(args, 0)?)?]);
        visibility(world, receiver, |e, everyone| {
            if !e.hidden {
                e.hidden = true;
                e.shown_to = everyone;
            }
            e.shown_to &= !bit;
        })
    });
    registry.register(Method, "setvisibletoteam", |world, receiver, args| {
        let team = string(args, 0)?;
        let clients = players_on(world, &team, None);
        let bits = client_bits(world, clients);
        visibility(world, receiver, |e, _| {
            e.hidden = true;
            e.shown_to = bits;
        })
    });
    registry.register(Method, "islinkedto", |world, receiver, args| {
        let runtime = world.resource::<RoundScript>();
        let parent = object_of(arg(args, 0)?);
        let linked = runtime
            .entity(receiver)
            .and_then(|(_, e)| e.linked_to.as_ref())
            .is_some_and(|link| Some(link.parent) == parent);
        Ok(Value::Int(linked.into()))
    });
    registry.register(Method, "getgroundent", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        let number = FrameWorld::from_world(world)
            .player(id)
            .map_or(-1, |ps| ps.ground_entity_num);
        let runtime = world.resource::<RoundScript>();
        Ok(runtime
            .entities
            .iter()
            .find(|(_, e)| e.number == number && e.kind != EntityKind::HudElem)
            .map_or(Value::Undefined, |(id, _)| Value::Object(*id)))
    });
    macro_rules! entity_fields {
        ($($name:literal => $field:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let id = super::engine::entity_id(world, receiver)?;
                let value = arg(args, 0)?.clone();
                world.resource_mut::<RoundScript>().set_object_field(id, $field, value);
                Ok(Value::Undefined)
            });
        )*};
    }
    entity_fields!(
        "setowner" => "owner",
        "setentityowner" => "owner",
        "setteam" => "team",
        "setattacker" => "attacker",
    );
    registry.register(Method, "clearentityowner", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "owner", Value::Undefined);
        Ok(Value::Undefined)
    });
    // `ent playSound(alias, [notify])`: a sound event on the entity; with a
    // notify, the entity raises it once the sound has played (a pending
    // one is raised at once).
    registry.register(Method, "playsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let alias = match args.first() {
            Some(_) => string(args, 0)?.to_string(),
            None => return Ok(Value::Undefined),
        };
        let notify = match args.get(1) {
            Some(_) if args.len() > 1 => Some(Arc::<str>::from(string(args, 1)?.as_ref())),
            _ => None,
        };
        let (number, origin) = entity_number_origin(world, receiver, id)?;
        if alias.is_empty() {
            return Ok(Value::Undefined);
        }
        if FrameWorld::from_world(world).script_sound_exists(&alias) == Some(true) {
            super::engine::sound_on_entity(world, number, origin, &alias, notify.is_some());
        }
        if let Some(notify) = notify {
            let now = super::super::players::now_ms(world);
            let old = world.resource_mut::<RoundScript>().sound_notifies.insert(
                number,
                crate::script::runtime::SoundNotify {
                    receiver: receiver.clone(),
                    notify,
                    start_ms: now,
                    duration_ms: -1,
                },
            );
            if let Some(old) = old {
                crate::script::runtime::raise(world, old.receiver, &old.notify, Vec::new());
            }
        }
        Ok(Value::Undefined)
    });
    // `ent stopSound(alias)`: stops the alias's sounds on the entity.
    registry.register(Method, "stopsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let alias = string(args, 0)?.to_string();
        let (number, origin) = entity_number_origin(world, receiver, id)?;
        if !alias.is_empty() {
            super::engine::stop_sound_on_entity(world, number, origin, &alias);
        }
        Ok(Value::Undefined)
    });
    // Black Ops' single player handles these with an empty function: no
    // receiver or argument check, nothing done.
    for name in ["setsoundblend", "setreverb", "deactivatereverb"] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    macro_rules! entity_accepts {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                super::engine::entity_id(world, receiver)?;
                Ok(Value::Undefined)
            });
        )*};
    }
    entity_accepts!(
        "sethintlowpriority",
        "sethintstringforperk",
        "setperkfortrigger",
        "setignoreentfortrigger",
        "enablelinkto",
        "setforcenocull",
        "setrevivehintstring",
        "sendfaceevent",
    );
}

fn register_platform(registry: &mut NativeRegistry) {
    macro_rules! platform {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, _| Ok(Value::Undefined));
        )*};
    }
    platform!(
        "bbprint",
        "incrementcounter",
        "pixbeginevent",
        "pixendevent",
        "pixmarker",
        "recordmatchbegin",
        "recordplayermatchend",
        "recordplayerstats",
        "uploadstats",
        "reportfilm",
        "changeadvertisedstatus",
        "pcserverupdateplaylist",
        "incrementescrow",
        "adddemobookmark",
        "setdemointermissionpoint",
        "startdemorecording",
        "stopdemorecording",
        "resettimeout",
        "setarchive",
        "setmatchtalkflag",
        "setqosgamedatapayload",
        "resetpregamedata",
        "debugstar",
        "sphere",
    );
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                world.resource_mut::<RoundScript>().presented.insert($name, args.to_vec());
                Ok(Value::Undefined)
            });
        )*};
    }
    presented!(
        "settimescale",
        "announcement",
        "clientannouncement",
        "setmatchflag",
        "setscoreboardcolumns",
        "setbombtimer",
        "reviveobituary",
        "artilleryiconlocation",
        "setteamsatellite",
        "setteamspyplane",
        "objective_setinvisibletoall",
        "objective_setinvisibletoplayer",
        "objective_setvisibletoplayer",
    );
    registry.register(Function, "getteamsatellite", |world, _, args| {
        let team = string(args, 0)?;
        Ok(presented_team_flag(world, "setteamsatellite", &team))
    });
    registry.register(Function, "getteamspyplane", |world, _, args| {
        let team = string(args, 0)?;
        Ok(presented_team_flag(world, "setteamspyplane", &team))
    });
}

fn presented_team_flag(world: &World, key: &str, team: &str) -> Value {
    match world
        .resource::<RoundScript>()
        .presented
        .get(key)
        .map(Vec::as_slice)
    {
        Some([Value::String(t), value, ..]) if **t == *team => value.clone(),
        _ => Value::Int(0),
    }
}

fn register_refused(registry: &mut NativeRegistry) {
    macro_rules! refused {
        ($namespace:ident: $($name:literal),* => $message:literal) => {$(
            registry.register($namespace, $name, |_, _, _| Err($message.into()));
        )*};
    }
    refused!(Function: "ban", "killserver" => "the host decides who stays and when the server stops");
    refused!(Function: "sethostmigrationstatus" => "host migration is not simulated");
    refused!(Function: "getcontractname", "getcontractrequiredcount", "getcontractrequirements",
        "getcontractresetconditions", "getcontractrewardcp", "getcontractrewardxp",
        "getcontractstatname", "getcontractstattype"
        => "contracts are online-only; none are active");
    refused!(Method: "getindexforactivecontract", "getactivecontractprogress",
        "getactivecontracttimepassed", "hasactivecontractexpired",
        "incrementactivecontractprogress", "incrementactivecontracttime",
        "isactivecontractcomplete", "resetactivecontractprogress"
        => "contracts are online-only; none are active");
    refused!(Function: "getgvrule", "getwagergametypelist", "pregamestartgame"
        => "game variants, wagers and the pregame lobby are not simulated");

    refused!(Function: "getmaxvehicles"
        => "T5 vehicle limits are not configured");
    refused!(Method: "getoccupantseat", "getseatoccupant", "getvehoccupants", "usevehicle",
        "launchvehicle", "makevehicleunusable", "setvehicleteam", "vehgetmodel",
        "gettreadhealth", "getspeed", "getspeedmph", "setspeed", "isvehicleimmunetodamage",
        "finishvehicledamage", "finishvehicleradiusdamage", "changeseatbuttonpressed",
        "setheliheightlock", "isinsideheliheightlock", "setviewclamp", "resetviewclamp",
        "returnplayercontrol", "setjitterparams", "heliturretdogtrace",
        "heliturretsighttrace", "setanim", "useanimtree"
        => "receiver is not a vehicle");

    refused!(Method: "spawnactor", "setgoalnode", "setgoalpos",
        "forceteleport", "orientmode", "traversemode", "setanimstate", "setaimanimweights",
        "getnegotiationstartnode", "finishactordamage",
        "clearentitytarget", "playbattlechattertoteam"
        => "actors are not simulated");
    refused!(Method: "canplayerplaceturret", "carryturret", "stopcarryturret", "relinktoturret",
        "setturretcarried", "setturrethint", "setturrettype", "setscanningpitch",
        "actionslotfourbuttonpressed"
        => "receiver is not a turret");
    refused!(Function: "target_getarray", "target_isincircle", "target_istarget",
        "target_remove", "target_set", "target_setturretaquire"
        => "missile lock targets are not simulated");
    refused!(Method: "missile_settarget", "ismissileinsideheightlock", "getlockonradius",
        "getlockonspeed", "linkguidedmissilecamera", "unlinkguidedmissilecamera",
        "makegrenadedud", "launchbomb"
        => "receiver is not a guided missile");

    // `player SetScoreMultiplier(n)` / `GetScoreMultiplier()`: a number the
    // player's state carries (Dead Ops Arcade packs its multiplier and
    // progress into it).
    registry.register(Method, "setscoremultiplier", |world, receiver, args| {
        let client = super::player::player(world, receiver)?;
        let value = int(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .t5
            .score_multipliers
            .insert(client, value);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getscoremultiplier", |world, receiver, _| {
        let client = super::player::player(world, receiver)?;
        let value = world
            .resource::<RoundScript>()
            .t5
            .score_multipliers
            .get(&client)
            .copied()
            .unwrap_or(0);
        Ok(Value::Int(value))
    });
    // `ent SetPlayerCollision(on)`: whether players collide with the
    // entity. Players here collide with no actor or entity body yet.
    registry.register(Method, "setplayercollision", |world, receiver, args| {
        if args.len() != 1 {
            return Err("Bad number of paramaters\n".into());
        }
        let id = super::engine::entity_id(world, receiver)?;
        let on = int(args, 0)? != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        if on {
            runtime.t5.no_player_collision.remove(&id);
        } else {
            runtime.t5.no_player_collision.insert(id);
        }
        Ok(Value::Undefined)
    });
    // `ent StartRagdoll([from_impact])`, `ent IsRagdoll()`, and
    // `ent LaunchRagdoll(force, [hitloc])`: transitions an actor
    // or corpse into ragdoll state and applies whole-body or hit-location
    // impulse scaled by `ragdoll_bullet_force / total_mass` (`500.0 / 138.6`).
    registry.register(Method, "startragdoll", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if !args.is_empty() {
            let _ = int(args, 0)?;
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(movement) = runtime.actor_moves.get_mut(&id) {
            movement.velocity = [0.0; 3];
            movement.drop_path();
        }
        runtime.t5.ragdolls.entry(id).or_default();
        diag::info!(Sim, "t5 startragdoll entity={id}");
        Ok(Value::Undefined)
    });
    registry.register(Method, "isragdoll", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let active = world
            .resource::<RoundScript>()
            .t5
            .ragdolls
            .contains_key(&id);
        Ok(Value::Int(i32::from(active)))
    });
    registry.register(Method, "launchragdoll", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if args.is_empty() {
            return Err("LaunchRagdoll requires a force vector".into());
        }
        let force = vector(args, 0)?;
        let hitloc = if args.len() > 1 {
            string(args, 1)?
        } else {
            String::new()
        };
        let scale = 500.0 / 138.6_f32;
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(movement) = runtime.actor_moves.get_mut(&id) {
            movement.velocity = [0.0; 3];
            movement.drop_path();
        }
        let ragdoll = runtime.t5.ragdolls.entry(id).or_default();
        ragdoll.launched = true;
        ragdoll.velocity[0] += force[0] * scale;
        ragdoll.velocity[1] += force[1] * scale;
        ragdoll.velocity[2] += (force[2] * scale).max(60.0);
        ragdoll.angular_vel = [
            force[2].clamp(-180.0, 180.0) + 120.0,
            force[0].clamp(-180.0, 180.0),
            force[1].clamp(-180.0, 180.0),
        ];
        ragdoll.hitloc = hitloc.clone();
        ragdoll.grounded = false;
        diag::info!(
            Sim,
            "t5 launchragdoll entity={id} force=({:.1},{:.1},{:.1}) hitloc={hitloc}",
            force[0],
            force[1],
            force[2]
        );
        Ok(Value::Undefined)
    });
    // Server `CreateDynEntAndLaunch(model, pos, angles, hitpos, force, [fx])`.
    registry.register(Function, "createdynentandlaunch", |world, _, args| {
        if args.len() < 5 {
            return Err(
                "CreateDynEntAndLaunch called with invalid params. CreateDynEntAndLaunch( <model>, <pos>, <angles>, <hitpos>, <force>, <fx> )"
                    .into(),
            );
        }
        let model = string(args, 0)?;
        let pos = vector(args, 1)?;
        let angles = vector(args, 2)?;
        let hit_pos = vector(args, 3)?;
        let force = vector(args, 4)?;
        let fx = if args.len() > 5 && !matches!(args[5], Value::Undefined) {
            let id = int(args, 5)?;
            world
                .resource::<crate::script::MatchScript>()
                .precached
                .iter()
                .find_map(|((kind, name), &idx)| {
                    (*kind == "fx" && idx == id).then(|| name.clone())
                })
        } else {
            None
        };
        let launch = crate::SpawnedDynEntLaunch {
            model,
            pos,
            angles,
            hit_pos,
            force,
            fx,
        };
        let state = t5(world);
        state.next_spawned_dyn_ent_serial = state
            .next_spawned_dyn_ent_serial
            .max(1_000_000)
            .wrapping_add(1);
        let serial = state.next_spawned_dyn_ent_serial;
        state.spawned_dyn_ents.push_back((serial, launch));
        while state.spawned_dyn_ents.len() > crate::SPAWNED_DYN_ENT_TAIL {
            state.spawned_dyn_ents.pop_front();
        }
        Ok(Value::Undefined)
    });
    // `PhysicsExplosionSphere(origin, outer, inner, magnitude, [inner_dmg], [outer_dmg])`.
    registry.register(Function, "physicsexplosionsphere", |world, _, args| {
        if !(4..=6).contains(&args.len()) {
            return Err("Incorrect number of parameters".into());
        }
        let origin = vector(args, 0)?;
        let outer = int(args, 1)? as i16 as f32;
        let inner = float(args, 2)?;
        if inner < 0.0 {
            return Err("Radius is negative".into());
        }
        if outer < inner {
            return Err("Inner radius is outside the outer radius".into());
        }
        let magnitude = float(args, 3)?;
        let outer_damage = if args.len() >= 5 { int(args, 4)? } else { 0 };
        let inner_damage = if args.len() == 6 {
            int(args, 5)?
        } else {
            outer_damage
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        crate::gentity::push_physics_explosion(
            &mut FrameWorld::from_world(world),
            tick,
            origin,
            outer,
            inner,
            magnitude,
            inner_damage,
            outer_damage,
            false,
            [0.0; 3],
        );
        Ok(Value::Undefined)
    });
    // `PhysicsJolt(origin, outer, inner, force_vec)`.
    registry.register(Function, "physicsjolt", |world, _, args| {
        if args.len() != 4 {
            return Err("Incorrect number of parameters".into());
        }
        let origin = vector(args, 0)?;
        let outer = int(args, 1)? as i16 as f32;
        let inner = int(args, 2)? as i16 as f32;
        let force_vec = vector(args, 3)?;
        let tick = world.resource::<crate::step::StepRequest>().tick;
        crate::gentity::push_physics_explosion(
            &mut FrameWorld::from_world(world),
            tick,
            origin,
            outer,
            inner.max(0.0),
            1.0,
            0,
            0,
            false,
            force_vec,
        );
        Ok(Value::Undefined)
    });
    // `player StartCameraTween(seconds)`: Black Ops sends the player an
    // event that eases its camera to the new view over that time; the
    // runtime's client has no camera tween, so the view cuts.
    registry.register(Method, "startcameratween", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        }
        float(args, 0)?;
        Ok(Value::Undefined)
    });
    // `ent PhysicsLaunch([point, force])`: Black Ops hands the entity to
    // a physics body until it comes to rest (`t5_physics`).
    registry.register(Method, "physicslaunch", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let (number, classname) = {
            let entity = &world.resource::<RoundScript>().entities[&id];
            (entity.number, entity.classname.clone())
        };
        if !matches!(
            &*classname,
            "script_brushmodel" | "script_model" | "script_origin" | "light"
        ) {
            return Err(format!(
                "entity {number} is not a script_brushmodel, script_model, script_origin, or light"
            ));
        }
        super::t5_physics::launch(world, id, args)
    });
    refused!(Function: "spawntimedfx", "boundswouldtelefrag", "testspawnpoint"
        => "not simulated");
    refused!(Method: "launch", "fakefire",
        "spawnnapalmgroundflame", "revive", "docowardswayanims",
        "ghost", "teleport", "istouchingvolume", "playersighttrace",
        "throwbuttonpressed", "actionslotonebuttonpressed", "depthinwater",
        "devaddpitch", "devaddroll", "devaddyaw"
        => "not simulated");
}

fn client_sys_set_state(
    world: &mut World,
    client: Option<u32>,
    args: &[Value],
) -> Result<Value, String> {
    let index = int(args, 0)?;
    if !(0..=8).contains(&index) {
        return Err(format!(
            "ClientSysSetState - state index ({index}) out of bounds (0 - 8)"
        ));
    }
    let Some(system) = t5(world).client_systems.get(index as usize).cloned() else {
        return Err(format!(
            "ClientSysSetState - state index ({index}) unregistered.  Use ClientSysRegister first."
        ));
    };
    let state: Arc<str> = match args.get(1) {
        None | Some(Value::Undefined) => "".into(),
        Some(_) => string(args, 1)?.into(),
    };
    world.resource_mut::<RoundScript>().client_messages.push(
        crate::script::client::ClientMessage::SystemState {
            client,
            system,
            state,
        },
    );
    Ok(Value::Undefined)
}

/// An entity's number and origin, for its sound events.
fn entity_number_origin(
    world: &mut World,
    receiver: &Value,
    id: u64,
) -> Result<(i32, [f32; 3]), String> {
    let number = world.resource::<RoundScript>().entities[&id].number;
    let origin = super::engine::origin_of(world, receiver)?;
    Ok((number, origin))
}

/// What the scripts asked of each test client: its goal, its scripted
/// enemy and the buttons it should press. The host's bots move and fight
/// on their own (`t5.bots.host_ai`); these are kept for the scripts.
#[derive(bevy_ecs::prelude::Resource, Default)]
struct BotScriptState(BTreeMap<u32, BotScript>);

#[derive(Default)]
struct BotScript {
    goal: Option<([f32; 3], f32)>,
    enemy: Option<Value>,
}

fn bot_script<'a>(world: &'a mut World, receiver: &Value) -> Result<&'a mut BotScript, String> {
    let client = player_id(world, receiver)?;
    Ok(world
        .get_resource_or_insert_with(BotScriptState::default)
        .into_inner()
        .0
        .entry(client)
        .or_default())
}

/// `ent IsTouchingSwept(other, [min z], [max z])`: the box of whichever of
/// the two is not a trigger volume (the receiver when neither is), stretched
/// down to `min z` and up to `max z`, then grown in height to at least its
/// width, against the other.
fn register_swept(registry: &mut NativeRegistry) {
    registry.register(Method, "istouchingswept", |world, receiver, args| {
        let own = super::engine::entity_id(world, receiver)?;
        let other = super::engine::entity_id(world, arg(args, 0)?)?;
        let (swept, target) = if !super::super::triggers::has_volume(world, own) {
            (own, other)
        } else if super::super::triggers::has_volume(world, other) {
            return Err("istouchingswept cannot be called on 2 brush/cylinder entities".into());
        } else {
            (other, own)
        };
        let (mut mins, mut maxs) = super::super::triggers::toucher_box(world, swept);
        if args.len() > 1 {
            let z = float(args, 1)?;
            if mins[2] - z >= 0.0 {
                mins[2] = z;
            }
        }
        if args.len() > 2 {
            let z = float(args, 2)?;
            if z - maxs[2] >= 0.0 {
                maxs[2] = z;
            }
        }
        let height = maxs[2] - mins[2];
        let x = maxs[0] - mins[0];
        let y = maxs[1] - mins[1];
        let width = if x - y < 0.0 { y } else { x };
        if height < width {
            let grow = (width - height) * 0.5;
            mins[2] -= grow;
            maxs[2] += grow;
        }
        let hit = super::super::triggers::box_touches(world, target, mins, maxs);
        Ok(Value::Int(hit.into()))
    });
}

/// Black Ops' test clients and the bot natives its `_bot.gsc` calls.
fn register_bots(registry: &mut NativeRegistry) {
    registry.register(Function, "addtestclient", |world, _, _| {
        Ok(super::super::players::add_test_client(world).map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "setscriptgoal", |world, receiver, args| {
        let goal = vector(args, 0)?;
        let radius = optional(args, 1, float)?.unwrap_or(0.0);
        bot_script(world, receiver)?.goal = Some((goal, radius));
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearscriptgoal", |world, receiver, _| {
        bot_script(world, receiver)?.goal = None;
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasscriptgoal", |world, receiver, _| {
        Ok(Value::Int(
            bot_script(world, receiver)?.goal.is_some().into(),
        ))
    });
    registry.register(Method, "setscriptenemy", |world, receiver, args| {
        let enemy = arg(args, 0)?.clone();
        bot_script(world, receiver)?.enemy = Some(enemy);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearscriptenemy", |world, receiver, _| {
        bot_script(world, receiver)?.enemy = None;
        Ok(Value::Undefined)
    });
    // The threat the scripts set; the host bots' own target is not shared.
    registry.register(Method, "getthreat", |world, receiver, _| {
        Ok(bot_script(world, receiver)?
            .enemy
            .clone()
            .unwrap_or(Value::Undefined))
    });
    // The straight line to the scripts' goal stands in for the path's
    // lookahead (`t5.bots.host_ai`).
    registry.register(Method, "getlookaheaddist", |world, receiver, _| {
        let origin = origin_of(world, receiver);
        let goal = bot_script(world, receiver)?.goal;
        Ok(Value::Float(goal.map_or(0.0, |(goal, _)| {
            let d = sub(goal, origin);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })))
    });
    registry.register(Method, "getlookaheaddir", |world, receiver, _| {
        let origin = origin_of(world, receiver);
        let goal = bot_script(world, receiver)?.goal;
        Ok(Value::Vector(goal.map_or([0.0; 3], |(goal, _)| {
            let d = sub(goal, origin);
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len > 0.0 {
                [d[0] / len, d[1] / len, d[2] / len]
            } else {
                [0.0; 3]
            }
        })))
    });
    // The host bots press their own buttons (`t5.bots.host_ai`).
    for name in ["pressattackbutton", "pressusebutton"] {
        registry.register(Method, name, |world, receiver, _| {
            player_id(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
}
