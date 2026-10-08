use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use bevy_ecs::prelude::World;
use glam::Vec3;

use super::entities::{EntityKind, HudAudience};
use crate::frame::FrameWorld;
use crate::script::{Runtime, Value};
use crate::{ClientId, ClientLifecycle, MatchPhase, Tick};

const MAX_ALIVE: usize = 24;
const fn ticks(ms: u32) -> u32 {
    ms.div_ceil(crate::MATCH_TICK_MS)
}
const HULL_MIN: [f32; 3] = [-14.0, -14.0, 0.0];
const HULL_MAX: [f32; 3] = [14.0, 14.0, 64.0];
const MASK: u32 = crate::bullet_collision::MASK_PLAYER_SOLID;

#[derive(Clone, Debug, Default)]
pub(crate) struct Survival {
    pub authored: Arc<Vec<Vec<(String, String)>>>,
    initialized: bool,
    round: u32,
    started: Option<u32>,
    remaining: u32,
    next_spawn: u32,
    next_round: Option<u32>,
    ended: Option<u32>,
    actors: BTreeMap<u64, Actor>,
    survivors: BTreeMap<u32, Survivor>,
    nodes: Arc<Vec<[f32; 3]>>,
    edges: Arc<Vec<Vec<usize>>>,
    purchases: Vec<Purchase>,
    box_claims: BTreeMap<usize, BoxClaim>,
    model: Option<String>,
    walk: Option<String>,
    head: Option<(String, String)>,
    revives: BTreeMap<u32, (ClientId, u32)>,
}

#[derive(Clone, Debug)]
struct Actor {
    origin: [f32; 3],
    path: VecDeque<usize>,
    repath: u32,
    attack_due: u32,
    velocity: [f32; 3],
}

#[derive(Clone, Debug, Default)]
struct Survivor {
    guns: Vec<u32>,
    perks: BTreeSet<String>,
    use_held: bool,
    hud: Option<u64>,
    prompt: Option<u64>,
    last_prompt: String,
    round_label: Option<u64>,
    last_round: u32,
    downed_since: Option<u32>,
    solo_revives_bought: u8,
    perk_label: Option<u64>,
    last_perks: String,
}

#[derive(Clone, Debug)]
struct Purchase {
    origin: [f32; 3],
    kind: PurchaseKind,
    price: i32,
}

#[derive(Clone, Debug)]
enum PurchaseKind {
    Weapon(String),
    Box,
    Upgrade,
    Perk(String),
}

#[derive(Clone, Debug)]
struct BoxClaim {
    client: ClientId,
    weapon: u32,
    ready: u32,
    expires: u32,
}

pub(crate) fn active(world: &mut World) -> bool {
    FrameWorld::from_world(world).bootstrap_ref().kind == gamemode_iw4::GameModeKind::Zombies
}

fn field<'a>(pairs: &'a [(String, String)], name: &str) -> &'a str {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map_or("", |(_, value)| value)
}

fn point(text: &str) -> Option<[f32; 3]> {
    let values: Vec<f32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let result: [f32; 3] = values.try_into().ok()?;
    result.iter().all(|n| n.is_finite()).then_some(result)
}

fn ground(frame: &FrameWorld, at: [f32; 3]) -> Option<[f32; 3]> {
    let mut start = at;
    start[2] += 18.0;
    let mut end = at;
    end[2] -= 96.0;
    let trace = frame.trace_world(start, end, HULL_MIN, HULL_MAX, MASK);
    (trace.fraction < 1.0 && trace.startsolid == 0 && trace.allsolid == 0 && trace.normal[2] >= 0.7)
        .then(|| [trace.endpos[0], trace.endpos[1], trace.endpos[2] + 0.1])
}

fn clear(frame: &FrameWorld, a: [f32; 3], b: [f32; 3]) -> bool {
    let mut start = a;
    let mut end = b;
    start[2] += 18.0;
    end[2] += 18.0;
    let trace = frame.trace_world(start, end, HULL_MIN, HULL_MAX, MASK);
    trace.fraction >= 1.0 && trace.startsolid == 0 && trace.allsolid == 0
}

struct ActorCollision<'a, 'w>(&'a FrameWorld<'w>);
impl movement_iw4::CollisionBackend for ActorCollision<'_, '_> {
    fn trace(&self, input: movement_iw4::GroundTraceInput) -> trace_iw4::Trace {
        self.0.trace_world(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
        )
    }
}

fn traverse(frame: &FrameWorld, actor: &mut Actor, direction: Vec3, speed: f32) {
    let mut motion = playerstate_iw4::PlayerState {
        origin: actor.origin,
        velocity: [direction.x * speed, direction.y * speed, actor.velocity[2]],
        ground_entity_num: playerstate_iw4::ENTITYNUM_NONE,
        ..playerstate_iw4::PlayerState::ZERO
    };
    let mut step = movement_iw4::Pml {
        forward: direction.to_array(),
        right: [0.0; 3],
        up: [0.0, 0.0, 1.0],
        frametime: crate::MATCH_TICK_MS as f32 / 1000.0,
        msec: crate::MATCH_TICK_MS as i32,
        walking: 0,
        ground_plane: 0,
        almost_ground_plane: 0,
        ground_trace: [0; 11],
        previous_origin: actor.origin,
        previous_velocity: actor.velocity,
        holdrand: 0,
        jump_animations: [None; 4],
        mantle_movetype: None,
        landing_animation: false,
        fall_damage: 0,
    };
    let bounds = movement_iw4::MoveBounds {
        mins: HULL_MIN,
        maxs: HULL_MAX,
        tracemask: MASK,
    };
    let collision = ActorCollision(frame);
    movement_iw4::complete_ground_trace(&mut motion, &mut step, bounds, 127, &collision);
    movement_iw4::step_slide_move(
        &mut motion,
        &step,
        &collision,
        HULL_MIN,
        HULL_MAX,
        MASK,
        Some(800.0),
    );
    if motion
        .origin
        .iter()
        .chain(&motion.velocity)
        .all(|value| value.is_finite())
    {
        actor.origin = motion.origin;
        actor.velocity = motion.velocity;
    }
}

fn reachable_node(frame: &FrameWorld, nodes: &[[f32; 3]], at: [f32; 3]) -> Option<usize> {
    let mut candidates: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            (
                i,
                Vec3::from_array(*node).distance_squared(Vec3::from_array(at)),
            )
        })
        .collect();
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    candidates
        .into_iter()
        .take(12)
        .find(|&(i, distance)| distance <= 256.0 * 256.0 && clear(frame, at, nodes[i]))
        .map(|(i, _)| i)
}

fn nearest(nodes: &[[f32; 3]], at: [f32; 3]) -> Option<usize> {
    nodes
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            Vec3::from_array(**a)
                .distance_squared(Vec3::from_array(at))
                .total_cmp(&Vec3::from_array(**b).distance_squared(Vec3::from_array(at)))
        })
        .map(|(i, _)| i)
}

fn route(state: &Survival, start: usize, goal: usize) -> VecDeque<usize> {
    let mut parents = vec![usize::MAX; state.nodes.len()];
    let mut queue = VecDeque::from([start]);
    parents[start] = start;
    while let Some(node) = queue.pop_front() {
        if node == goal {
            break;
        }
        for &next in &state.edges[node] {
            if parents[next] == usize::MAX {
                parents[next] = node;
                queue.push_back(next);
            }
        }
    }
    if parents[goal] == usize::MAX {
        return VecDeque::new();
    }
    let mut path = VecDeque::new();
    let mut node = goal;
    while node != start {
        path.push_front(node);
        node = parents[node];
    }
    path
}

fn initialize(world: &mut World, state: &mut Survival) {
    let mut frame = FrameWorld::from_world(world);
    for client in frame.client_ids_sorted() {
        let meta = frame.client_meta_mut(client);
        meta.score = 0;
        meta.kills = 0;
        meta.deaths = 0;
    }
    state.model = frame.zombie_body_model();
    state.walk = frame.zombie_walk_anim();
    state.head = state
        .model
        .as_deref()
        .and_then(|model| frame.zombie_head_attachment(model));
    let mut nodes = Vec::new();
    for pairs in state.authored.iter() {
        let target = field(pairs, "targetname");
        let weapon = field(pairs, "zombie_weapon_upgrade");
        let Some(origin) = point(field(pairs, "origin")) else {
            continue;
        };
        if field(pairs, "classname") == "node_pathnode" {
            if let Some(origin) = ground(&frame, origin)
                && !nodes.iter().any(|&other| {
                    Vec3::from_array(origin).distance_squared(Vec3::from_array(other)) < 64.0
                })
            {
                nodes.push(origin);
            }
        }
        let note = field(pairs, "script_noteworthy");
        let filters = field(pairs, "script_string");
        if target == "zm_perk_machine"
            && !filters.is_empty()
            && !filters
                .split_whitespace()
                .any(|s| s.starts_with("zclassic"))
        {
            continue;
        }
        let kind = if target == "zm_perk_machine" {
            match note {
                "specialty_weapupgrade" => Some(PurchaseKind::Upgrade),
                "specialty_armorvest" => Some(PurchaseKind::Perk("juggernog".into())),
                "specialty_fastreload" => Some(PurchaseKind::Perk("sleight".into())),
                "specialty_quickrevive" => Some(PurchaseKind::Perk("revive".into())),
                _ => None,
            }
        } else if target == "perksacola" && field(pairs, "script_sound") == "mx_packa_jingle" {
            Some(PurchaseKind::Upgrade)
        } else if !weapon.is_empty() {
            Some(PurchaseKind::Weapon(weapon.to_owned()))
        } else if target == "treasure_chest_use" {
            Some(PurchaseKind::Box)
        } else if target == "zombie_vending_upgrade" {
            Some(PurchaseKind::Upgrade)
        } else if target.starts_with("zombie_vending_") {
            Some(PurchaseKind::Perk(
                target.trim_start_matches("zombie_vending_").to_owned(),
            ))
        } else {
            None
        };
        if let Some(kind) = kind {
            let fallback = match &kind {
                PurchaseKind::Weapon(name) => match name.as_str() {
                    "m14_zm" | "rottweil72_zm" => 500,
                    "mp5k_zm" | "870mcs_zm" => 1200,
                    "m16_zm" => 1200,
                    _ => 1000,
                },
                PurchaseKind::Box => 950,
                PurchaseKind::Upgrade => 5000,
                PurchaseKind::Perk(name) => match name.as_str() {
                    "juggernog" => 2500,
                    "sleight" => 3000,
                    "doubletap" | "staminup" => 2000,
                    _ => 1500,
                },
            };
            let price = field(pairs, "zombie_cost")
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(fallback);
            state.purchases.push(Purchase {
                origin,
                kind,
                price,
            });
        }
    }
    let mut edges = vec![Vec::new(); nodes.len()];
    for (i, &a) in nodes.iter().enumerate() {
        let mut neighbors: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter_map(|(j, &b)| {
                let distance = Vec3::from_array(a).distance_squared(Vec3::from_array(b));
                (i != j && distance < 256.0 * 256.0 && (a[2] - b[2]).abs() <= 48.0)
                    .then_some((j, distance))
            })
            .collect();
        neighbors.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(j, _) in neighbors.iter().take(12) {
            if clear(&frame, a, nodes[j]) {
                edges[i].push(j);
            }
        }
    }
    state.nodes = Arc::new(nodes);
    state.edges = Arc::new(edges);
    state.initialized = true;
    state.next_round = Some(0);
    diag::info!(
        Sim,
        "zombies initialized nodes={} purchases={} body={:?} walk={:?}",
        state.nodes.len(),
        state.purchases.len(),
        state.model,
        state.walk
    );
}

fn weapon_id(frame: &FrameWorld, name: &str) -> Option<u32> {
    frame
        .weapon_script_names()
        .iter()
        .position(|n| n == name)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| frame.weapon_runnable(n))
}

fn score(world: &mut World, client: ClientId, amount: i32) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = FrameWorld::from_world(world);
    let Some(meta) = frame.client_meta(client) else {
        return;
    };
    let (points, kills, deaths) = (meta.score.saturating_add(amount), meta.kills, meta.deaths);
    frame.client_meta_mut(client).score = points;
    frame.push_event(
        tick,
        crate::EventAudience::All,
        crate::SimEvent::ScoreChanged {
            client,
            score: points,
            kills,
            deaths,
        },
    );
}

fn spawn_player(world: &mut World, state: &mut Survival, client: ClientId, tick: Tick) {
    let mut frame = FrameWorld::from_world(world);
    let avoid: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| *id != client)
        .filter_map(|id| frame.player(id).map(|ps| ps.origin))
        .collect();
    let report = crate::spawn::decide_forced_spawn(
        &frame,
        crate::SpawnPick::Seeded(u64::from(client.0) + u64::from(tick.0)),
        &avoid,
        entity_iw4::TEAM_FREE,
    );
    let Some(spawn) = report.accepted else {
        return;
    };
    let Some(pistol) = weapon_id(&frame, "m1911_zm") else {
        return;
    };
    frame.client_meta_mut(client).client_state_team = entity_iw4::TEAM_FREE;
    frame.client_meta_mut(client).max_health = 100;
    crate::script_player::spawn(
        &mut frame,
        tick,
        client,
        spawn.traced_origin,
        spawn.raw_angles,
        "playing",
    );
    if crate::script_player::give_weapon(&mut frame, client, pistol, false).is_ok() {
        let _ = crate::script_player::set_spawn_weapon(&mut frame, client, pistol);
    }
    if let Some(slot) = frame
        .ecs()
        .resource_mut::<Runtime>()
        .players
        .get_mut(&client.0)
    {
        slot.sessionstate = "playing".into();
        slot.begun = true;
    }
    let survivor = state.survivors.entry(client.0).or_default();
    survivor.guns = vec![pistol];
    survivor.perks.clear();
    survivor.downed_since = None;
    survivor.use_held = false;
    state.revives.remove(&client.0);
    let points = frame.client_meta(client).map_or(0, |meta| meta.score);
    drop(frame);
    if points < 500 {
        score(world, client, 500 - points);
    }
    diag::info!(
        Sim,
        "zombies survivor spawned client={} pistol=m1911_zm",
        client.0
    );
}

fn spawn_actor(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) -> bool {
    let Some(model) = &state.model else {
        return false;
    };
    let offset = tick.0 as usize % state.nodes.len().max(1);
    let origin = (0..state.nodes.len())
        .map(|i| state.nodes[(i + offset) % state.nodes.len()])
        .find(|at| {
            players.iter().all(|(_, player)| {
                let distance = Vec3::from_array(*at).distance(Vec3::from_array(*player));
                (320.0..1400.0).contains(&distance)
            }) && players.iter().any(|(_, player)| {
                nearest(&state.nodes, *player)
                    .zip(nearest(&state.nodes, *at))
                    .is_some_and(|(goal, start)| !route(state, start, goal).is_empty())
            }) && state.actors.values().all(|actor| {
                Vec3::from_array(actor.origin).distance_squared(Vec3::from_array(*at)) > 1600.0
            })
        });
    let Some(origin) = origin else {
        return false;
    };
    let Ok(presence) = super::presence::spawn_presence(world, origin) else {
        return false;
    };
    let number = FrameWorld::from_world(world).gentity_number(presence);
    let mut runtime = world.resource_mut::<Runtime>();
    let Ok(object) = runtime.create_entity(EntityKind::Spawned, "actor_zombie") else {
        return false;
    };
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "model", Value::string(model));
    let health = if state.round <= 9 {
        100 + state.round as i32 * 50
    } else {
        (550.0_f64 * 1.1_f64.powi((state.round - 9).min(150) as i32)).min(i32::MAX as f64) as i32
    };
    runtime.set_object_field(object, "health", Value::Int(health));
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    if let Some((head, tag)) = &state.head {
        entity
            .attachments
            .push((head.as_str().into(), tag.as_str().into()));
    }
    entity.contents = crate::bullet_collision::CONTENTS_BODY as i32;
    entity.can_damage = true;
    entity.can_radius_damage = true;
    if let Some(number) = number {
        entity.number = number;
    }
    entity.anim_op = state.walk.as_ref().map(|clip| Some(clip.as_str().into()));
    state.actors.insert(
        object,
        Actor {
            origin,
            path: VecDeque::new(),
            repath: 0,
            attack_due: tick.0 + ticks(1000),
            velocity: [0.0; 3],
        },
    );
    diag::info!(
        Sim,
        "zombie spawned object={object} round={} health={health} origin={origin:?}",
        state.round
    );
    true
}

fn move_actors(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) -> Vec<crate::script_player::Hit> {
    let mut hits = Vec::new();
    let actors = std::mem::take(&mut state.actors);
    for (object, mut actor) in actors {
        let Some(&(victim, target)) = players.iter().min_by(|(_, a), (_, b)| {
            Vec3::from_array(*a)
                .distance_squared(Vec3::from_array(actor.origin))
                .total_cmp(&Vec3::from_array(*b).distance_squared(Vec3::from_array(actor.origin)))
        }) else {
            state.actors.insert(object, actor);
            continue;
        };
        let mut frame = FrameWorld::from_world(world);
        if tick.0 >= actor.repath {
            actor.path = reachable_node(&frame, &state.nodes, actor.origin)
                .zip(reachable_node(&frame, &state.nodes, target))
                .map(|(start, goal)| route(state, start, goal))
                .unwrap_or_default();
            actor.repath = tick.0 + ticks(1000);
        }
        let direct = clear(&frame, actor.origin, target);
        while actor.path.front().is_some_and(|&node| {
            Vec3::from_array(state.nodes[node]).distance(Vec3::from_array(actor.origin)) < 24.0
        }) {
            actor.path.pop_front();
        }
        let goal = if direct {
            Some(target)
        } else {
            actor.path.front().map(|&node| state.nodes[node])
        };
        let delta = Vec3::from_array(target) - Vec3::from_array(actor.origin);
        if delta.length() < 60.0 && direct {
            if tick.0 < actor.attack_due {
                state.actors.insert(object, actor);
                continue;
            }
            actor.attack_due = tick.0 + ticks(1000);
            hits.push(crate::script_player::Hit {
                victim,
                attacker: None,
                amount: 50,
                flags: 0,
                means: "MOD_MELEE",
                weapon: 0,
                point: target,
                dir: delta.normalize_or_zero().to_array(),
                hitloc: 0,
                inflictor: None,
                commit: None,
            });
        } else if let Some(goal) = goal {
            let direction = Vec3::new(goal[0] - actor.origin[0], goal[1] - actor.origin[1], 0.0)
                .normalize_or_zero();
            let speed = (35.0 + state.round as f32 * 7.0).min(170.0);
            traverse(&frame, &mut actor, direction, speed);
            let yaw = direction.y.atan2(direction.x).to_degrees();
            let runtime = frame.ecs().resource_mut::<Runtime>().into_inner();
            runtime.set_object_field(object, "origin", Value::Vector(actor.origin));
            runtime.set_object_field(object, "angles", Value::Vector([0.0, yaw, 0.0]));
        }
        state.actors.insert(object, actor);
    }
    hits
}

fn give_gun(
    world: &mut World,
    survivor: &mut Survivor,
    client: ClientId,
    weapon: u32,
    replace: Option<u32>,
) -> bool {
    let mut frame = FrameWorld::from_world(world);
    if frame.weapon_is_melee_only(weapon) || frame.equipment_facts_for(weapon).is_some() {
        return false;
    }
    if survivor.guns.contains(&weapon) {
        crate::script_player::give_max_ammo(&mut frame, client, weapon);
        return true;
    }
    if crate::script_player::give_weapon(&mut frame, client, weapon, false).is_err() {
        return false;
    }
    let remove = replace
        .or_else(|| (survivor.guns.len() >= 2).then(|| frame.player(client).unwrap().weapon));
    if let Some(remove) = remove {
        crate::script_player::take_weapon(&mut frame, client, remove);
        survivor.guns.retain(|&gun| gun != remove);
    }
    survivor.guns.push(weapon);
    let _ = crate::script_player::set_spawn_weapon(&mut frame, client, weapon);
    true
}

fn purchase(
    world: &mut World,
    state: &mut Survival,
    survivor: &mut Survivor,
    client: ClientId,
    index: usize,
    tick: Tick,
) {
    let row = state.purchases[index].clone();
    let frame = FrameWorld::from_world(world);
    let cash = frame.client_meta(client).map_or(0, |meta| meta.score);
    let held = frame.player(client).map_or(0, |ps| ps.weapon);
    let resolve = |name: &str| weapon_id(&frame, name);
    let solo = frame.client_ids_sorted().len() == 1;
    let mut price = if matches!(&row.kind, PurchaseKind::Perk(name) if name == "revive") && solo {
        500
    } else {
        row.price
    };
    let gun = match &row.kind {
        PurchaseKind::Weapon(name) => {
            let gun = resolve(name);
            if gun.is_some_and(|gun| survivor.guns.contains(&gun)) {
                price /= 2;
            }
            gun
        }
        PurchaseKind::Upgrade => {
            let name = frame.weapon_script_name(held);
            if name.contains("_upgraded") {
                return;
            }
            name.strip_suffix("_zm")
                .and_then(|base| resolve(&format!("{base}_upgraded_zm")))
        }
        PurchaseKind::Box => {
            if let Some(claim) = state.box_claims.get(&index) {
                if claim.client != client || tick.0 < claim.ready {
                    return;
                }
                let weapon = claim.weapon;
                drop(frame);
                if give_gun(world, survivor, client, weapon, None) {
                    state.box_claims.remove(&index);
                }
                return;
            }
            let pool: Vec<_> = frame
                .weapon_script_names()
                .iter()
                .filter(|name| name.ends_with("_zm") && !name.contains("upgraded"))
                .filter_map(|name| resolve(name))
                .filter(|&gun| {
                    !survivor.guns.contains(&gun)
                        && frame
                            .combat_facts_for(gun)
                            .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_BULLET)
                        && frame.equipment_facts_for(gun).is_none()
                        && !frame.weapon_is_melee_only(gun)
                })
                .collect();
            if pool.is_empty() || cash < price {
                return;
            }
            let choice = ((u64::from(tick.0)
                .wrapping_mul(6364136223846793005u64)
                .wrapping_add(u64::from(client.0)))
                % pool.len() as u64) as usize;
            state.box_claims.insert(
                index,
                BoxClaim {
                    client,
                    weapon: pool[choice],
                    ready: tick.0 + ticks(3000),
                    expires: tick.0 + ticks(15000),
                },
            );
            drop(frame);
            score(world, client, -price);
            diag::info!(Sim, "zombies box spin client={} cost={price}", client.0);
            return;
        }
        PurchaseKind::Perk(name) => {
            if !matches!(name.as_str(), "juggernog" | "sleight" | "revive")
                || survivor.perks.contains(name)
                || survivor.perks.len() >= 4
                || (name == "revive" && solo && survivor.solo_revives_bought >= 3)
                || cash < price
            {
                return;
            }
            drop(frame);
            let mut frame = FrameWorld::from_world(world);
            if name == "juggernog" {
                frame.client_meta_mut(client).max_health = 250;
                if let Some(ps) = frame.player_mut(client) {
                    ps.max_health = 250;
                    ps.health = 250;
                }
            } else if name == "sleight" {
                crate::script_player::set_perk(&mut frame, client, "specialty_fastreload", true);
            }
            survivor.perks.insert(name.clone());
            if name == "revive" && solo {
                survivor.solo_revives_bought += 1;
            }
            drop(frame);
            score(world, client, -price);
            return;
        }
    };
    let Some(gun) = gun else {
        return;
    };
    if cash < price {
        return;
    }
    let replace = matches!(row.kind, PurchaseKind::Upgrade).then_some(held);
    drop(frame);
    if give_gun(world, survivor, client, gun, replace) {
        score(world, client, -price);
        diag::info!(
            Sim,
            "zombies purchase client={} weapon={gun} cost={price}",
            client.0
        );
    }
}

fn make_hud(world: &mut World, client: ClientId, y: f32, scale: f32) -> Option<u64> {
    let Value::Object(object) =
        super::hud::new_hud_elem(world, HudAudience::Client(client.0)).ok()?
    else {
        return None;
    };
    for (name, value) in [
        ("x", Value::Float(24.0)),
        ("y", Value::Float(y)),
        ("fontscale", Value::Float(scale)),
        ("foreground", Value::Int(1)),
    ] {
        let _ = super::hud::store_field(world, object, name, &value);
    }
    Some(object)
}

fn hud_text(world: &mut World, object: u64, text: &str) {
    let Ok(index) = super::hud::string_index(world, &Value::string(text)) else {
        return;
    };
    let Some(&slot) = world.resource::<Runtime>().hud_slots.get(&object) else {
        return;
    };
    if let Some(slot) = FrameWorld::from_world(world)
        .hud_elem_slots_mut()
        .get_mut(slot)
    {
        slot.elem.elem_type = hud_iw4::HE_TYPE_TEXT;
        slot.elem.text = index;
    }
}

fn restore_survivor(world: &mut World, state: &mut Survival, victim: ClientId) {
    if let Some(survivor) = state.survivors.get_mut(&victim.0) {
        survivor.downed_since = None;
        survivor.perks.clear();
    }
    let mut frame = FrameWorld::from_world(world);
    crate::script_player::revive(&mut frame, victim);
    crate::script_player::clear_perks(&mut frame, victim);
    frame.client_meta_mut(victim).max_health = 100;
    if let Some(ps) = frame.player_mut(victim) {
        ps.health = 100;
        ps.max_health = 100;
    }
    state.revives.remove(&victim.0);
    diag::info!(Sim, "zombies survivor revived client={}", victim.0);
}

fn advance_revives(world: &mut World, state: &mut Survival, tick: Tick) {
    let downed: Vec<_> = state
        .survivors
        .iter()
        .filter_map(|(&client, survivor)| {
            Some((
                ClientId(client),
                survivor.downed_since?,
                survivor.perks.contains("revive"),
            ))
        })
        .collect();
    for (victim, since, quick) in downed {
        let mut frame = FrameWorld::from_world(world);
        if !frame
            .client_meta(victim)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        {
            state.survivors.get_mut(&victim.0).unwrap().downed_since = None;
            state.revives.remove(&victim.0);
            continue;
        }
        let Some(origin) = frame.player(victim).map(|ps| ps.origin) else {
            continue;
        };
        let living: Vec<_> = frame
            .client_ids_sorted()
            .into_iter()
            .filter(|&client| client != victim)
            .filter(|client| {
                state
                    .survivors
                    .get(&client.0)
                    .is_some_and(|survivor| survivor.downed_since.is_none())
            })
            .filter(|&client| {
                frame
                    .client_meta(client)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
            })
            .collect();
        if frame.client_ids_sorted().len() == 1
            && living.is_empty()
            && quick
            && tick.0.saturating_sub(since) >= ticks(7000)
        {
            drop(frame);
            restore_survivor(world, state, victim);
            continue;
        }
        let helper = living.into_iter().find(|&client| {
            let Some(ps) = frame.player(client) else {
                return false;
            };
            let at = ps.origin;
            let nearby =
                Vec3::from_array(at).distance_squared(Vec3::from_array(origin)) <= 80.0 * 80.0;
            let sight = frame
                .trace_world(
                    [at[0], at[1], at[2] + 40.0],
                    [origin[0], origin[1], origin[2] + 20.0],
                    [0.0; 3],
                    [0.0; 3],
                    0x11,
                )
                .fraction
                >= 1.0;
            nearby
                && sight
                && crate::script_player::buttons(&mut frame, client)
                    & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
                    != 0
        });
        if let Some(helper) = helper {
            let progress = state.revives.entry(victim.0).or_insert((helper, tick.0));
            if progress.0 != helper {
                *progress = (helper, tick.0);
            }
            let duration = if state
                .survivors
                .get(&helper.0)
                .is_some_and(|survivor| survivor.perks.contains("revive"))
            {
                ticks(1500)
            } else {
                ticks(3000)
            };
            if tick.0.saturating_sub(progress.1) >= duration {
                drop(frame);
                restore_survivor(world, state, victim);
                score(world, helper, 50);
                continue;
            }
        } else {
            state.revives.remove(&victim.0);
        }
        if tick.0.saturating_sub(since) >= ticks(45000) {
            crate::script_player::kill(&mut frame, tick, victim, None, None);
            frame.client_meta_mut(victim).deaths += 1;
            state.survivors.get_mut(&victim.0).unwrap().downed_since = None;
            if let Some(slot) = frame
                .ecs()
                .resource_mut::<Runtime>()
                .players
                .get_mut(&victim.0)
            {
                slot.sessionstate = "dead".into();
            }
            diag::info!(Sim, "zombies survivor bled out client={}", victim.0);
        }
    }
}

fn interactions(
    world: &mut World,
    state: &mut Survival,
    tick: Tick,
    players: &[(ClientId, [f32; 3])],
) {
    state.box_claims.retain(|_, claim| tick.0 < claim.expires);
    for &(client, origin) in players {
        let mut survivor = state.survivors.remove(&client.0).unwrap_or_default();
        let mut frame = FrameWorld::from_world(world);
        let held = crate::script_player::buttons(&mut frame, client)
            & (playerstate_iw4::buttons::USE | playerstate_iw4::buttons::USE_RELOAD)
            != 0;
        let selected = state
            .purchases
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                Vec3::from_array(row.origin).distance_squared(Vec3::from_array(origin))
                    <= 96.0 * 96.0
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            row.origin,
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 0.95
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(a.origin)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(
                        &Vec3::from_array(b.origin).distance_squared(Vec3::from_array(origin)),
                    )
            })
            .map(|(i, _)| i);
        let revival = state
            .survivors
            .iter()
            .filter(|(_, survivor)| survivor.downed_since.is_some())
            .find_map(|(&id, _)| {
                let at = frame.player(ClientId(id))?.origin;
                (Vec3::from_array(at).distance_squared(Vec3::from_array(origin)) <= 80.0 * 80.0
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 40.0],
                            [at[0], at[1], at[2] + 20.0],
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 1.0)
                    .then_some(id)
            });
        let solo = frame.client_ids_sorted().len() == 1;
        let cash = frame.client_meta(client).map_or(0, |meta| meta.score);
        drop(frame);
        if held
            && revival.is_none()
            && !survivor.use_held
            && let Some(index) = selected
        {
            purchase(world, state, &mut survivor, client, index, tick);
        }
        survivor.use_held = held;
        if survivor.perk_label.is_none() {
            survivor.perk_label = make_hud(world, client, 320.0, 1.0);
        }
        let perks = format!(
            "PERKS: {}",
            survivor
                .perks
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        );
        if survivor.last_perks != perks {
            if let Some(object) = survivor.perk_label {
                hud_text(world, object, &perks);
            }
            survivor.last_perks = perks;
        }
        if survivor.hud.is_none() {
            survivor.hud = make_hud(world, client, 400.0, 1.8);
        }
        if survivor.prompt.is_none() {
            survivor.prompt = make_hud(world, client, 260.0, 1.0);
        }
        if survivor.round_label.is_none() {
            survivor.round_label = make_hud(world, client, 360.0, 1.4);
        }
        if let Some(object) = survivor.hud {
            let slot = world.resource::<Runtime>().hud_slots.get(&object).copied();
            let mut frame = FrameWorld::from_world(world);
            if let Some(slot) = slot.and_then(|slot| frame.hud_elem_slots_mut().get_mut(slot)) {
                slot.elem.elem_type = hud_iw4::HE_TYPE_VALUE;
                slot.elem.value = cash as f32;
            }
        }
        let prompt = selected
            .filter(|_| revival.is_none())
            .map(|index| {
                let row = &state.purchases[index];
                match &row.kind {
                    PurchaseKind::Weapon(name) => {
                        let frame = FrameWorld::from_world(world);
                        let owned =
                            weapon_id(&frame, name).is_some_and(|gun| survivor.guns.contains(&gun));
                        if owned {
                            format!("USE: {name} ammo  [{} points]", row.price / 2)
                        } else {
                            format!("USE: {name}  [{} points]", row.price)
                        }
                    }
                    PurchaseKind::Box => match state.box_claims.get(&index) {
                        Some(claim) if tick.0 < claim.ready => "Mystery Box spinning...".to_owned(),
                        Some(claim) if claim.client == client => {
                            "USE: take Mystery Box weapon".to_owned()
                        }
                        Some(_) => "Mystery Box in use".to_owned(),
                        None => format!("USE: Mystery Box  [{} points]", row.price),
                    },
                    PurchaseKind::Upgrade => "USE: Pack-a-Punch  [5000 points]".to_owned(),
                    PurchaseKind::Perk(name)
                        if matches!(name.as_str(), "juggernog" | "sleight" | "revive") =>
                    {
                        let label = match name.as_str() {
                            "juggernog" => "Jugger-Nog",
                            "sleight" => "Speed Cola",
                            _ => "Quick Revive",
                        };
                        let price = if name == "revive" && solo {
                            500
                        } else {
                            row.price
                        };
                        format!("USE: {label}  [{price} points]")
                    }
                    PurchaseKind::Perk(_) => "Perk: Work in Progress".to_owned(),
                }
            })
            .unwrap_or_else(|| {
                let Some(victim) = revival else {
                    return String::new();
                };
                let duration = if survivor.perks.contains("revive") {
                    ticks(1500)
                } else {
                    ticks(3000)
                };
                match state
                    .revives
                    .get(&victim)
                    .filter(|(helper, _)| *helper == client)
                {
                    Some((_, start)) => format!(
                        "USE: reviving teammate {}%",
                        tick.0
                            .saturating_sub(*start)
                            .saturating_mul(100)
                            .checked_div(duration)
                            .unwrap_or(0)
                            .min(100)
                    ),
                    None => "USE: revive teammate (hold)".into(),
                }
            });
        if survivor.last_prompt != prompt {
            if let Some(object) = survivor.prompt {
                hud_text(world, object, &prompt);
            }
            survivor.last_prompt = prompt;
        }
        if survivor.last_round != state.round {
            if let Some(object) = survivor.round_label {
                hud_text(world, object, &format!("ROUND {}", state.round));
            }
            survivor.last_round = state.round;
        }
        state.survivors.insert(client.0, survivor);
    }
}

pub(crate) fn advance(world: &mut World) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut state = std::mem::take(&mut world.resource_mut::<Runtime>().zombies);
    if !state.initialized {
        initialize(world, &mut state);
    }
    let connected: BTreeSet<_> = FrameWorld::from_world(world)
        .client_ids_sorted()
        .into_iter()
        .map(|id| id.0)
        .collect();
    let departed: Vec<_> = state
        .survivors
        .keys()
        .copied()
        .filter(|id| !connected.contains(id))
        .collect();
    for id in departed {
        if let Some(survivor) = state.survivors.remove(&id) {
            for object in [
                survivor.hud,
                survivor.prompt,
                survivor.round_label,
                survivor.perk_label,
            ]
            .into_iter()
            .flatten()
            {
                super::hud::destroy(world, object);
            }
        }
        state.revives.remove(&id);
        state.revives.retain(|_, (helper, _)| helper.0 != id);
        state.box_claims.retain(|_, claim| claim.client.0 != id);
    }
    if let Some(ended) = state.ended {
        world.resource_mut::<Runtime>().zombies = state;
        if tick.0.saturating_sub(ended) >= ticks(5000) {
            world.resource_mut::<Runtime>().pending_restart = Some(false);
            super::restart::restart_level(world, tick);
        }
        return;
    }
    let mut hits = Vec::new();
    let mut frame = FrameWorld::from_world(world);
    if let Some(started) = state.started {
        frame.set_match_elapsed_ms(
            tick.0
                .saturating_sub(started)
                .saturating_mul(crate::MATCH_TICK_MS),
        );
    }
    if frame.phase() == MatchPhase::Warmup {
        frame.set_phase(MatchPhase::Playing);
    }
    let spawn: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| {
            frame.client_meta(id).is_some_and(|meta| {
                matches!(
                    meta.lifecycle,
                    ClientLifecycle::Connecting | ClientLifecycle::ChoosingClass
                )
            })
        })
        .collect();
    drop(frame);
    for client in spawn {
        spawn_player(world, &mut state, client, tick);
    }
    advance_revives(world, &mut state, tick);
    if state.next_round.is_some_and(|due| tick.0 >= due) && state.round > 0 {
        let frame = FrameWorld::from_world(world);
        let returning: Vec<_> = frame
            .client_ids_sorted()
            .into_iter()
            .filter(|&id| {
                frame
                    .client_meta(id)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Dead)
            })
            .collect();
        drop(frame);
        for client in returning {
            spawn_player(world, &mut state, client, tick);
        }
    }
    let frame = FrameWorld::from_world(world);
    let players: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| {
            frame
                .client_meta(id)
                .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        })
        .filter(|id| {
            state
                .survivors
                .get(&id.0)
                .is_none_or(|survivor| survivor.downed_since.is_none())
        })
        .filter_map(|id| frame.player(id).map(|ps| (id, ps.origin)))
        .collect();
    drop(frame);
    if players.is_empty() {
        let auto_revive_pending = connected.len() == 1
            && state.survivors.values().any(|survivor| {
                survivor.downed_since.is_some() && survivor.perks.contains("revive")
            });
        if !state.survivors.is_empty() && !auto_revive_pending {
            state.ended = Some(tick.0);
            let mut frame = FrameWorld::from_world(world);
            frame.set_phase(MatchPhase::Intermission);
            for client in frame.client_ids_sorted() {
                frame.client_meta_mut(client).lifecycle = ClientLifecycle::Intermission;
            }
            diag::info!(Sim, "zombies game over round={}", state.round);
        }
    } else {
        if state.next_round.is_some_and(|due| tick.0 >= due) {
            state.started.get_or_insert(tick.0);
            state.round += 1;
            state.remaining =
                6 + (state.round - 1) * 2 + players.len().saturating_sub(1) as u32 * 4;
            state.next_round = None;
            state.next_spawn = tick.0 + ticks(2000);
            diag::info!(
                Sim,
                "zombies round={} total={}",
                state.round,
                state.remaining
            );
        }
        if state.remaining > 0 && state.actors.len() < MAX_ALIVE && tick.0 >= state.next_spawn {
            if spawn_actor(world, &mut state, tick, &players) {
                state.remaining -= 1;
            }
            state.next_spawn =
                tick.0 + ticks((2000u32.saturating_sub(state.round.saturating_mul(100))).max(750));
        }
        hits = move_actors(world, &mut state, tick, &players);
        interactions(world, &mut state, tick, &players);
        if state.remaining == 0 && state.actors.is_empty() && state.next_round.is_none() {
            state.next_round = Some(tick.0 + ticks(8000));
        }
    }
    world.resource_mut::<Runtime>().zombies = state;
    for hit in hits {
        player_damage(world, tick, &hit);
    }
}

pub(crate) fn entity_damage_amount(
    world: &World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
) -> i32 {
    if hit.amount > 0
        && hit.means == "MOD_MELEE"
        && world
            .resource::<Runtime>()
            .zombies
            .actors
            .contains_key(&object)
    {
        hit.amount.max(150)
    } else {
        hit.amount
    }
}

pub(crate) fn entity_damage(
    world: &mut World,
    object: u64,
    hit: &super::entity_damage::EntityHit,
    before: i32,
    after: i32,
    headshot: bool,
) {
    if !world
        .resource::<Runtime>()
        .zombies
        .actors
        .contains_key(&object)
        || before <= 0
        || hit.amount <= 0
    {
        return;
    }
    let killed = after <= 0;
    if killed {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.zombies.actors.remove(&object);
        runtime.pending_deletes.push(object);
    }
    if let Some(client) = hit.attacker {
        if killed {
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(client).is_some() {
                frame.client_meta_mut(client).kills += 1;
            }
        }
        let reward = if !killed {
            10
        } else if hit.means == "MOD_MELEE" {
            130
        } else if headshot {
            100
        } else {
            60
        };
        score(world, client, reward);
        diag::info!(
            Sim,
            "zombies hit object={object} before={before} after={after} points={reward} client={}",
            client.0
        );
    }
}

pub(crate) fn player_damage(world: &mut World, tick: Tick, hit: &crate::script_player::Hit) {
    if hit.attacker.is_some_and(|client| client != hit.victim) {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    if frame.phase() != MatchPhase::Playing
        || !frame
            .client_meta(hit.victim)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    if frame
        .player(hit.victim)
        .is_some_and(|ps| ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND != 0)
    {
        return;
    }
    if crate::script_player::finish_damage(&mut frame, hit.victim, hit.amount, Some(hit.dir))
        == crate::script_player::Finish::Killed
    {
        let teammate_alive = frame.client_ids_sorted().into_iter().any(|client| {
            client != hit.victim
                && frame
                    .client_meta(client)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
                && frame
                    .player(client)
                    .is_some_and(|ps| ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND == 0)
        });
        let quick = frame.client_ids_sorted().len() == 1
            && frame
                .ecs()
                .resource::<Runtime>()
                .zombies
                .survivors
                .get(&hit.victim.0)
                .is_some_and(|survivor| survivor.perks.contains("revive"));
        if teammate_alive || quick {
            crate::script_player::clear_perks(&mut frame, hit.victim);
            frame.client_meta_mut(hit.victim).max_health = 100;
            if let Some(ps) = frame.player_mut(hit.victim) {
                ps.health = 1;
                ps.max_health = 100;
                ps.pm_type = playerstate_iw4::PM_TYPE_LAST_STAND;
                ps.pm_flags |= playerstate_iw4::pm_flags::LAST_STAND;
                ps.view_height_target = movement_iw4::view_height::LAST_STAND;
            }
            if let Some(survivor) = frame
                .ecs()
                .resource_mut::<Runtime>()
                .zombies
                .survivors
                .get_mut(&hit.victim.0)
            {
                survivor.downed_since = Some(tick.0);
                survivor.perks.retain(|name| name == "revive");
            }
            diag::info!(Sim, "zombies survivor downed client={}", hit.victim.0);
            return;
        }
        crate::script_player::kill(&mut frame, tick, hit.victim, hit.attacker, hit.commit);
        frame.client_meta_mut(hit.victim).deaths += 1;
        if let Some(slot) = frame
            .ecs()
            .resource_mut::<Runtime>()
            .players
            .get_mut(&hit.victim.0)
        {
            slot.sessionstate = "dead".into();
        }
        diag::info!(Sim, "zombies survivor died client={}", hit.victim.0);
    }
}
