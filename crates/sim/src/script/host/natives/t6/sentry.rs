//! Black Ops 2's turret think, every 50 ms: a sentry aiming for itself
//! (`auto_nonai`) picks a target among the sentients, turns onto it (wide
//! of it for its yaw convergence time), says when it starts and stops
//! firing, and scans its arcs when it has nothing to aim at. A turret not
//! aiming for itself drops its gun. `shootturret` fires the turret's weapon
//! from its `tag_flash` as the engine's bullet fire does, the turret being
//! the attacker when nobody uses it.

use super::super::super::presence;
use super::super::engine::{collider_entity, entity_trace, trace, trace_ignore};
use super::damage;
use super::movement::{field, origin_of};
use super::riotshield::{dvar_f32, dvar_i32, vector_angles};
use super::turret::{MODE_AI, MODE_AUTO, TYPE_SENTRY, TYPE_TOW, Turret, Turrets};
use crate::bullet_collision::{ColliderId, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{RoundScript, Value};
use asset_core::ZoneWeapon;
use bevy_ecs::prelude::{Resource, World};

/// A think's share of a second, and the think's period.
const STEP: f32 = 0.05;
const THINK_MS: f32 = 50.0;
/// How fast a turret not aiming for itself turns, degrees a second.
const IDLE_TURN_RATE: f32 = 200.0;
const SENTRY_TARGET_TIME: (&str, i32) = ("turret_sentrytargettime", 1000);
const KILLSTREAK_TARGET_TIME: (&str, i32) = ("turret_killstreaktargettime", 4000);
const SENTIENT_SLOTS: usize = 72;
/// The teams each team's turret aims at: every team but its own (none for
/// the free team).
const ENEMY_TEAMS: [u32; 10] = [0, 508, 506, 502, 494, 478, 446, 382, 254, 0];
/// A player's eye is aimed at this much lower standing (view height 60) and
/// crouched (40); feet are aimed at this far above the origin.
const PLAYER_EYE_DROP: [(i32, f32); 2] = [(60, 10.0), (40, 5.0)];
const FEET_RAISE: f32 = 2.0;
/// The sight line from the muzzle to a target's eye; one shorter than this
/// counts as blocked.
const SIGHT_MASK: u32 = 0x0080_1c03;
const SIGHT_MIN: f32 = 0.2;
/// It opens fire within this many degrees of its aim, or this long after
/// taking the target.
const SETTLE_DEGREES: f32 = 5.0;
const SETTLE_MS: i64 = 250;
/// Firing wide: first this far beside the target, then this far plus
/// another share of the convergence time left.
const MISS_FIRST: f32 = 63.0;
const MISS_NEAR: f32 = 33.0;
const MISS_FAR: f32 = 30.0;
/// A turret aimed at another turret aims this far above its origin.
const TURRET_TARGET_RAISE: f32 = 50.0;
/// The scan stays this far inside the yaw arcs, and within a quarter turn.
const SCAN_MARGIN: f32 = 4.0;
const SCAN_LIMIT: f32 = 90.0;
const BULLET_RANGE: (&str, f32) = ("bulletrange", 8192.0);
const BULLET_BOUNCE: (&str, i32) = ("tu13_bg_enablebulletweaponbounce", 1);
/// A bullet goes through at most this many surfaces and bodies; a surface
/// it passes leaves it this far beyond (eight times, and spent, when it
/// grazes the surface).
const BULLET_HITS: usize = 12;
const PASS_OFFSET: f32 = 0.135;
const GRAZE: f32 = 0.125;
const SURF_PASS: u32 = 0x10;
/// Weapon classes firing several pellets.
const SPREAD_CLASSES: [i32; 2] = [3, 13];
const MOD_PISTOL_BULLET: usize = 1;
const MOD_RIFLE_BULLET: usize = 2;
const DFLAG_ARMOR_PIERCING: i32 = 2;
const DFLAG_PENETRATION: i32 = 8;
/// The zone's depths a bullet passes, by penetration type and surface.
const PENETRATION_TABLE: &str = "info/bullet_penetration_mp";
const PENETRATION_TABLE_HEADER: &str = "BULLET_PEN_TABLE";
const PENETRATION_TYPES: [&str; 3] = ["small", "medium", "large"];
const PENETRATION_COUNT: (&str, i32) = ("penetrationcount", 5);
/// A charge-shot weapon's bullet always passes this many surfaces.
const FIRE_CHARGE_SHOT: i32 = 8;
const CHARGE_SHOT_PENETRATIONS: i32 = 5;
/// The exit is looked for back from this far before the next hit.
const EXIT_BACK: f32 = 0.01;
/// Surfaces any bullet passes: flagged so, or water.
const SURF_FULL_DEPTH: u32 = 0x4;
const WATER: u32 = 20;
const FULL_DEPTH: f32 = 100.0;
const SURF_NO_PENETRATE: u32 = 0x100;
/// The surfaces a body and a riot shield are struck as.
const FLESH_FLAGS: u32 = 7 << 20;
const RIOTSHIELD_FLAGS: u32 = 31 << 20;

/// The engine's state for a turret's aim (`t6.turret.state`).
#[derive(Clone, Debug, Default)]
pub(super) struct Aim {
    /// The gun's pitch and yaw on the turret.
    gun: [f32; 2],
    target: Option<u64>,
    target_time: i64,
    has_target: bool,
    miss_chosen: bool,
    miss: [f32; 3],
    last_slot: Option<usize>,
    manual_offset: [f32; 3],
    scanning: bool,
    scan_back: bool,
    scan_speed: f32,
    scan_brake: f32,
    scan_until: i64,
    /// Least pitch, least yaw, most pitch, most yaw.
    arcs: [f32; 4],
    max_range_sq: f32,
    /// For an AI's shots and a player's.
    spread: [f32; 2],
}

impl Aim {
    /// As the engine sets a turret up from its weapon.
    pub(super) fn new(def: &ZoneWeapon) -> Self {
        let [left, right, top, bottom] = def.turret_arcs;
        let range = def.turret_max_range;
        Self {
            arcs: [
                (-top).min(0.0),
                (-right).min(0.0),
                bottom.max(0.0),
                left.max(0.0),
            ],
            max_range_sq: if range <= 0.0 {
                f32::MAX
            } else {
                range * range
            },
            spread: def.turret_spread.map(|s| s.max(0.0)),
            ..Self::default()
        }
    }
}

/// The sentients by the engine's slot, taken when one spawns and given up
/// when it dies or leaves (`t6.turret.sentient_slots`).
#[derive(Resource)]
struct Sentients([Option<u64>; SENTIENT_SLOTS]);

impl Default for Sentients {
    fn default() -> Self {
        Self([None; SENTIENT_SLOTS])
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    v.map(|c| c * s)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    let length = if -length >= 0.0 { 1.0 } else { length };
    scale(v, 1.0 / length)
}

/// The engine's `AngleNormalize180`.
fn norm180(angle: f32) -> f32 {
    let turns = angle * (1.0 / 360.0);
    (turns - (turns + 0.5).floor()) * 360.0
}

/// Forward, left and up of angles.
fn axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match field(world, id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn int_field(world: &mut World, id: u64, name: &str) -> i32 {
    match field(world, id, name) {
        Value::Int(n) => n,
        Value::Float(f) => f as i32,
        _ => 0,
    }
}

fn now_ms(world: &World) -> i64 {
    super::super::super::players::now_ms(world)
}

fn live(world: &World, id: u64) -> bool {
    world.resource::<RoundScript>().entities.contains_key(&id)
}

/// A player in play, or a live actor: what holds a sentient.
fn holds_sentient(world: &mut World, id: u64) -> bool {
    let Some(entity) = world.resource::<RoundScript>().entities.get(&id) else {
        return false;
    };
    if world.resource::<RoundScript>().player_client(id).is_some() {
        return matches!(field(world, id, "sessionstate"), Value::String(s) if s.as_ref() == "playing");
    }
    entity.kind == super::EntityKind::Actor && int_field(world, id, "health") > 0
}

/// Gives up the slots of sentients gone, and gives the new ones the first
/// free slots in the order they were made.
fn sync_sentients(world: &mut World) {
    let mut slots = world.get_resource_or_insert_with(Sentients::default).0;
    for slot in &mut slots {
        if slot.is_some_and(|id| !holds_sentient(world, id)) {
            *slot = None;
        }
    }
    let candidates: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(id, entity)| {
                entity.kind == super::EntityKind::Actor || runtime.player_client(**id).is_some()
            })
            .map(|(id, _)| *id)
            .collect()
    };
    for id in candidates {
        if slots.contains(&Some(id)) || !holds_sentient(world, id) {
            continue;
        }
        if let Some(free) = slots.iter_mut().find(|slot| slot.is_none()) {
            *free = Some(id);
        }
    }
    world.resource_mut::<Sentients>().0 = slots;
}

fn is_sentient(world: &World, id: u64) -> bool {
    world
        .get_resource::<Sentients>()
        .is_some_and(|s| s.0.contains(&Some(id)))
}

fn sentient_team(world: &mut World, id: u64) -> usize {
    match field(world, id, "team") {
        Value::String(team) => super::team_index(&team).unwrap_or(0),
        _ => 0,
    }
}

/// The bone control the server sets from the gun's angles: the pitch on
/// `tag_aim_pitch` where the model has one, the yaw (and else the pitch) on
/// `tag_aim` (or `tag_turret`) and `tag_aim_animated`, nothing on
/// `tag_flash`; a model with neither turns only `tag_barrel`'s pitch.
fn controls(world: &mut World, id: u64, gun: [f32; 2]) -> Vec<(&'static str, [f32; 3])> {
    let [pitch, yaw] = gun;
    let Some([aim_pitch, aim, turret]) =
        presence::tags_present(world, id, ["tag_aim_pitch", "tag_aim", "tag_turret"])
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut rest = pitch;
    if aim_pitch {
        out.push(("tag_aim_pitch", [pitch, 0.0, 0.0]));
        rest = 0.0;
    }
    let base = if aim {
        "tag_aim"
    } else if turret {
        "tag_turret"
    } else {
        out.push(("tag_barrel", [rest, 0.0, 0.0]));
        return out;
    };
    out.push((base, [rest, yaw, 0.0]));
    out.push(("tag_aim_animated", [rest, yaw, 0.0]));
    out.push(("tag_flash", [0.0; 3]));
    out
}

/// A tag's world position and axis with the gun's bone control.
fn gun_tag(world: &mut World, id: u64, gun: [f32; 2], tag: &str) -> Option<presence::TagPose> {
    let controls = controls(world, id, gun);
    presence::tag_pose_controlled(world, id, tag, &controls).flatten()
}

/// Pitch and yaw from `from` to `to` in the turret's own frame.
fn local_angles(turret_axis: [[f32; 3]; 3], from: [f32; 3], to: [f32; 3]) -> [f32; 2] {
    let dir = sub(to, from);
    let local = [
        dot(dir, turret_axis[0]),
        dot(dir, turret_axis[1]),
        dot(dir, turret_axis[2]),
    ];
    vector_angles(local).map(norm180)
}

/// The muzzle and the angles to a point inside the turret's arcs.
fn point_angles(
    world: &mut World,
    id: u64,
    t: &Turret,
    point: [f32; 3],
) -> Option<([f32; 3], [f32; 2])> {
    let flash = gun_tag(world, id, t.aim.gun, "tag_flash")?.0;
    let turret_axis = axis(vector_field(world, id, "angles"));
    let angles = local_angles(turret_axis, flash, point);
    let [pitch_min, yaw_min, pitch_max, yaw_max] = t.aim.arcs;
    ((pitch_min..=pitch_max).contains(&angles[0]) && (yaw_min..=yaw_max).contains(&angles[1]))
        .then_some((flash, angles))
}

/// A sentient's eye; a player's is aimed at a little lower.
fn sentient_eye(world: &mut World, id: u64, aiming: bool) -> [f32; 3] {
    let mut eye =
        super::combat::eye(world, &Value::Object(id)).unwrap_or_else(|_| origin_of(world, id));
    if aiming && let Some(client) = world.resource::<RoundScript>().player_client(id) {
        let height = FrameWorld::from_world(world)
            .player(crate::ClientId(client))
            .map_or(0, |ps| ps.view_height_target);
        if let Some((_, drop)) = PLAYER_EYE_DROP.iter().find(|(h, _)| *h == height) {
            eye[2] -= drop;
        }
    }
    eye
}

/// Where the turret aims at a sentient: its eye, else its feet (feet first
/// while firing wide), else level with the muzzle when the eye and feet lie
/// either side of the pitch arcs; nothing outside the yaw arcs. With the
/// muzzle and the angles.
fn aim_point(
    world: &mut World,
    id: u64,
    t: &Turret,
    target: u64,
) -> Option<([f32; 3], [f32; 3], [f32; 2])> {
    let origin = origin_of(world, target);
    let eye = sentient_eye(world, target, true);
    let flash = gun_tag(world, id, t.aim.gun, "tag_flash")?.0;
    let turret_axis = axis(vector_field(world, id, "angles"));
    let [eye_pitch, yaw] = local_angles(turret_axis, flash, eye);
    let feet = [origin[0], origin[1], origin[2] + FEET_RAISE];
    let feet_pitch = local_angles(turret_axis, flash, feet)[0];
    let [pitch_min, yaw_min, pitch_max, yaw_max] = t.aim.arcs;
    if yaw > yaw_max || yaw < yaw_min {
        return None;
    }
    let inside = |pitch: f32| pitch <= pitch_max && pitch_min <= pitch;
    let (first, second) = if t.state == 2 {
        ((feet_pitch, feet), (eye_pitch, eye))
    } else {
        ((eye_pitch, eye), (feet_pitch, feet))
    };
    for (pitch, point) in [first, second] {
        if inside(pitch) {
            return Some((point, flash, [pitch, yaw]));
        }
    }
    ((first.0 > pitch_max) != (second.0 > pitch_max)).then_some((
        [eye[0], eye[1], flash[2]],
        flash,
        [0.0, yaw],
    ))
}

/// Nothing of the world between the muzzle and the point.
fn sight_clear(world: &mut World, from: [f32; 3], to: [f32; 3]) -> bool {
    let delta = sub(to, from);
    if dot(delta, delta).sqrt() < SIGHT_MIN {
        return false;
    }
    let line = trace(world, from, to, [0.0; 3], [0.0; 3], SIGHT_MASK);
    line.fraction >= 1.0 && line.startsolid == 0 && line.allsolid == 0
}

/// Within range and arcs, and seen from the muzzle: its distance squared.
fn can_target(world: &mut World, id: u64, t: &Turret, target: u64) -> Option<f32> {
    let origin = origin_of(world, target);
    let delta = sub(origin, origin_of(world, id));
    let distance_sq = dot(delta, delta);
    if t.flags & TYPE_TOW == 0 && distance_sq >= t.aim.max_range_sq {
        return None;
    }
    let (from, to) = if is_sentient(world, target) {
        let (_, flash, _) = aim_point(world, id, t, target)?;
        (flash, sentient_eye(world, target, false))
    } else {
        (point_angles(world, id, t, origin)?.0, origin)
    };
    sight_clear(world, from, to).then_some(distance_sq)
}

/// Players out of play are not aimed at.
fn player_in_play(world: &mut World, id: u64) -> bool {
    world.resource::<RoundScript>().player_client(id).is_none()
        || int_field(world, id, "health") > 0
}

/// `turret_SentryTargetTime` it keeps its target outright unless it is of
/// its own team; then while it can aim at it. Each think it looks at one
/// more sentient of an enemy team in turn and takes it if it can aim at it
/// and it is nearer.
fn find_best(world: &mut World, id: u64, t: &mut Turret, now: i64) -> Option<u64> {
    let mut keep = None;
    if t.aim.has_target
        && let Some(current) = t.aim.target
        && is_sentient(world, current)
        && player_in_play(world, current)
    {
        let hold = i64::from(dvar_i32(world, SENTRY_TARGET_TIME.0, SENTRY_TARGET_TIME.1));
        if hold + t.aim.target_time < now {
            if int_field(world, current, "ignoreme") == 0
                && can_target(world, id, t, current).is_some()
            {
                keep = Some(current);
            }
        } else {
            let team = sentient_team(world, current);
            if t.team != team || team == 0 {
                return Some(current);
            }
        }
    }
    let mask = ENEMY_TEAMS.get(t.team).copied().unwrap_or(0);
    let slots = world.resource::<Sentients>().0;
    let start = t.aim.last_slot.map_or(0, |slot| slot + 1);
    let mut next = None;
    for (slot, holder) in slots.iter().enumerate().skip(start) {
        if let Some(holder) = *holder {
            let team = sentient_team(world, holder);
            if mask & (1 << (team & 31)) != 0 || team == 0 {
                next = Some((slot, holder));
                break;
            }
        }
    }
    t.aim.last_slot = next.map(|(slot, _)| slot);
    let Some((_, candidate)) = next else {
        return keep;
    };
    if int_field(world, candidate, "ignoreme") != 0
        || Some(candidate) == t.owner
        || !player_in_play(world, candidate)
    {
        return keep;
    }
    if let Value::Object(owner) = field(world, candidate, "script_owner")
        && Some(owner) == t.owner
    {
        return keep;
    }
    let Some(distance_sq) = can_target(world, id, t, candidate) else {
        return keep;
    };
    let nearer = match keep {
        None => true,
        Some(kept) => {
            let delta = sub(origin_of(world, id), origin_of(world, kept));
            distance_sq < dot(delta, delta)
        }
    };
    if nearer { Some(candidate) } else { keep }
}

/// A new target restarts the target time and says so; the same one held
/// past `turret_KillstreakTargetTime` restarts it too.
fn set_target(
    world: &World,
    t: &mut Turret,
    target: u64,
    now: i64,
    events: &mut Vec<&'static str>,
) {
    if t.aim.target != Some(target) {
        t.aim.target = Some(target);
        t.aim.miss_chosen = false;
        t.aim.target_time = now;
        events.push("turret_target_aquired");
        return;
    }
    let hold = i64::from(dvar_i32(
        world,
        KILLSTREAK_TARGET_TIME.0,
        KILLSTREAK_TARGET_TIME.1,
    ));
    if hold + t.aim.target_time < now {
        t.aim.target_time = now;
    }
}

fn convergence_left(t: &Turret, axis: usize, now: i64) -> i64 {
    i64::from(t.convergence_ms[axis]) + t.aim.target_time - now
}

/// Idle to aiming once within 5 degrees or 250 ms after taking the target,
/// aiming to firing once the yaw convergence time is up; the point aimed at
/// is kept.
fn update_state(t: &mut Turret, angles: [f32; 2], now: i64, events: &mut Vec<&'static str>) {
    if t.state == 0 {
        let settling = now < t.aim.target_time + SETTLE_MS || t.flags & TYPE_TOW != 0;
        let off = norm180(t.aim.gun[0] - angles[0]).abs() >= SETTLE_DEGREES
            || norm180(t.aim.gun[1] - angles[1]).abs() >= SETTLE_DEGREES;
        if settling && off {
            t.aim.has_target = true;
            return;
        }
        events.push("turretstatechange");
        t.state = 2;
    }
    if t.state == 2 && convergence_left(t, 1, now) < 0 {
        t.state = 1;
    }
    t.aim.has_target = true;
}

/// The angles to a point inside the arcs, the state updated.
fn aim_at_point(
    world: &mut World,
    id: u64,
    t: &mut Turret,
    point: [f32; 3],
    now: i64,
    events: &mut Vec<&'static str>,
) -> Option<[f32; 2]> {
    match point_angles(world, id, t, point) {
        Some((_, angles)) => {
            update_state(t, angles, now, events);
            Some(angles)
        }
        None => {
            t.aim.has_target = false;
            None
        }
    }
}

/// Aims at a sentient: for the yaw convergence time wide of it, first
/// 63 beside it at its feet (or its aim height) on the side the gun points,
/// then 33 plus 30 times the share of the time left from the aim point.
fn aim_sentient(
    world: &mut World,
    id: u64,
    t: &mut Turret,
    target: u64,
    now: i64,
    events: &mut Vec<&'static str>,
) -> Option<[f32; 2]> {
    set_target(world, t, target, now, events);
    let (aim, flash, angles) = aim_point(world, id, t, target)?;
    let convergence = t.convergence_ms[1];
    if convergence_left(t, 1, now) > 0 {
        if !t.aim.miss_chosen {
            t.aim.miss_chosen = true;
            let gun = gun_tag(world, id, t.aim.gun, "tag_aim")
                .or_else(|| gun_tag(world, id, t.aim.gun, "tag_barrel"))?;
            let (dx, dy) = (aim[0] - flash[0], aim[1] - flash[1]);
            let length = (dx * dx + dy * dy).sqrt();
            let length = if -length >= 0.0 { 1.0 } else { length };
            let side = [dy / length * MISS_FIRST, -dx / length * MISS_FIRST];
            let toward = gun.1[0][0] * side[0] + gun.1[0][1] * side[1] >= 0.0;
            let sign = if toward { 1.0 } else { -1.0 };
            let feet = origin_of(world, target)[2];
            for z in [feet, aim[2]] {
                let miss = [aim[0] + side[0] * sign, aim[1] + side[1] * sign, z];
                if let Some(angles) = aim_at_point(world, id, t, miss, now, events) {
                    t.aim.miss = normalize(sub(miss, aim));
                    return Some(angles);
                }
            }
            t.aim.miss = [0.0; 3];
        } else {
            let share = if convergence < 1 {
                0.0
            } else {
                (t.aim.target_time - now + i64::from(convergence)) as f32 / convergence as f32
            };
            let point = add(
                add(aim, scale(t.aim.miss, MISS_NEAR)),
                scale(t.aim.miss, share * MISS_FAR),
            );
            if let Some(angles) = aim_at_point(world, id, t, point, now, events) {
                return Some(angles);
            }
        }
    }
    update_state(t, angles, now, events);
    Some(angles)
}

fn clear_target(t: &mut Turret, events: &mut Vec<&'static str>) {
    if t.aim.target.take().is_some() {
        events.push("turret_target_lost");
    }
    t.aim.has_target = false;
    t.aim.miss_chosen = false;
    if t.state != 0 {
        events.push("turretstatechange");
    }
    t.state = 0;
}

fn stop_scan(t: &mut Turret, events: &mut Vec<&'static str>) {
    if t.aim.scanning {
        events.push("turret_scan_stop");
    }
    t.aim.scanning = false;
    t.aim.scan_back = !t.aim.scan_back;
    t.aim.scan_speed = 0.0;
    t.aim.scan_brake = 0.0;
    t.aim.scan_until = 0;
}

/// Each axis turns at the speed that lands on the angles as its convergence
/// time runs out, within the weapon's turn rates (200 a second when not
/// aiming for itself). The engine says each think whether it is on target;
/// no on-target angle is ever set, so it never is.
fn turn(t: &mut Turret, angles: [f32; 2], aiming: bool, now: i64, events: &mut Vec<&'static str>) {
    stop_scan(t, events);
    let mut rates = [0, 1].map(|i| {
        let rate = norm180(t.aim.gun[i] - angles[i]).abs() * 1000.0 / THINK_MS;
        let left = convergence_left(t, i, now);
        if left > 0 && THINK_MS < left as f32 {
            THINK_MS / left as f32 * rate
        } else {
            rate
        }
    });
    if aiming {
        let [least_pitch, least_yaw, most_pitch, most_yaw] = t.def.turret_turn_rates;
        for (rate, (least, most)) in rates
            .iter_mut()
            .zip([(least_pitch, most_pitch), (least_yaw, most_yaw)])
        {
            if most - *rate < 0.0 {
                *rate = most;
            }
            if *rate - least < 0.0 {
                *rate = least;
            }
        }
    } else {
        rates = [IDLE_TURN_RATE; 2];
    }
    for i in 0..2 {
        let step = rates[i] * STEP;
        let delta = norm180(angles[i] - t.aim.gun[i]).clamp(-step, step);
        t.aim.gun[i] += delta;
    }
    events.push("turret_not_on_target");
}

/// The engine's angle approach: toward `target` by at most `speed * step`.
fn approach(target: f32, current: f32, speed: f32, step: f32) -> f32 {
    let mut target = target;
    while target - current > 180.0 {
        target -= 360.0;
    }
    while target - current < -180.0 {
        target += 360.0;
    }
    let mut move_by = speed * step;
    if target - current <= 0.0 {
        move_by = -move_by;
    }
    let gap = (target - current).abs();
    if gap > 0.001 && move_by.abs() <= gap {
        target = move_by + current;
    }
    norm180(target)
}

/// With nothing to aim at a sentry levels its gun, then sweeps its yaw
/// arcs: from a pause at one end it speeds up to the weapon's scan speed and
/// brakes to stop at the other.
fn scan(t: &mut Turret, now: i64, events: &mut Vec<&'static str>) {
    let (speed, accel, pause) = t.def.turret_scan;
    let pitch = approach(0.0, t.aim.gun[0], speed, STEP);
    t.aim.gun[0] = pitch;
    if pitch != 0.0 {
        return;
    }
    let low = (t.aim.arcs[1] + SCAN_MARGIN).max(-SCAN_LIMIT);
    let high = (t.aim.arcs[3] - SCAN_MARGIN).min(SCAN_LIMIT);
    let (left, sign) = if t.aim.scan_back {
        (t.aim.gun[1] - low, -1.0)
    } else {
        (high - t.aim.gun[1], 1.0)
    };
    if !t.aim.scanning {
        t.aim.scanning = true;
        t.aim.scan_speed = 0.0;
        t.aim.scan_until = i64::from(pause) + now;
        t.aim.scan_brake = (speed * speed / (accel * 2.0)).min(left * 0.5);
        events.push("turret_scan_start");
    }
    if now < t.aim.scan_until {
        return;
    }
    if t.aim.scan_brake <= left {
        t.aim.scan_speed = (accel * STEP + t.aim.scan_speed).min(speed);
    } else {
        t.aim.scan_speed -= accel * STEP;
        if t.aim.scan_speed <= 0.0 {
            stop_scan(t, events);
            return;
        }
    }
    t.aim.gun[1] += t.aim.scan_speed * STEP * sign;
}

fn sentry(world: &mut World, id: u64, t: &mut Turret, now: i64, events: &mut Vec<&'static str>) {
    let best = find_best(world, id, t, now);
    if let Some(best) = best
        && let Some(angles) = aim_sentient(world, id, t, best, now, events)
    {
        turn(t, angles, true, now, events);
        return;
    }
    if let Some(manual) = t.manual_target.filter(|m| live(world, *m))
        && best != Some(manual)
    {
        let angles = if is_sentient(world, manual) {
            can_target(world, id, t, manual)
                .and_then(|_| aim_sentient(world, id, t, manual, now, events))
        } else {
            if world.resource::<Turrets>().0.contains_key(&manual) {
                t.aim.manual_offset = [0.0, 0.0, TURRET_TARGET_RAISE];
            }
            set_target(world, t, manual, now, events);
            let point = add(origin_of(world, manual), t.aim.manual_offset);
            aim_at_point(world, id, t, point, now, events)
        };
        if let Some(angles) = angles {
            turn(t, angles, true, now, events);
            return;
        }
    }
    clear_target(t, events);
    scan(t, now, events);
}

fn think_one(
    world: &mut World,
    id: u64,
    t: &mut Turret,
    now: i64,
    events: &mut Vec<&'static str>,
) -> Result<(), String> {
    if t.flags & MODE_AUTO != 0 && t.flags & MODE_AI == 0 {
        if t.flags & TYPE_SENTRY != 0 {
            sentry(world, id, t, now, events);
        }
        if t.flags & TYPE_TOW != 0 {
            return Err("t6.turret.tow: a TOW turret's think is not implemented yet".into());
        }
        return Ok(());
    }
    if t.state != 0 {
        events.push("turretstatechange");
    }
    t.state = 0;
    turn(t, [t.drop_pitch, 0.0], false, now, events);
    Ok(())
}

/// Every turret's think, once a server frame (the engine's 50 ms).
pub(super) fn think(world: &mut World) {
    let ids: Vec<u64> = match world.get_resource::<Turrets>() {
        Some(turrets) if !turrets.0.is_empty() => turrets.0.keys().copied().collect(),
        _ => return,
    };
    sync_sentients(world);
    let now = now_ms(world);
    for id in ids {
        if !live(world, id) {
            continue;
        }
        let Some(mut turret) = world.resource::<Turrets>().0.get(&id).cloned() else {
            continue;
        };
        let mut events = Vec::new();
        let result = think_one(world, id, &mut turret, now, &mut events);
        world.resource_mut::<Turrets>().0.insert(id, turret);
        for name in events {
            raise(world, Value::Object(id), name, Vec::new());
        }
        if let Err(fault) = result {
            diag::warn!(Sim, "turret {id}: {fault}");
        }
    }
}

/// The engine's random numbers for a shot: seeded from the time.
fn scramble(time: u32) -> u32 {
    let low = time & 0xff;
    let seed = time ^ (low << 8);
    (low << 24) ^ seed ^ (low << 16)
}

fn random(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    (*seed >> 17) as f32 * (1.0 / 32768.0)
}

fn skip_randoms(seed: &mut u32, count: usize) {
    for _ in 0..count {
        *seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    }
}

/// Damage at a distance: the first step's within its range, the last's
/// beyond the last range, else between the two steps around it.
fn damage_at(def: &ZoneWeapon, from: [f32; 3], to: [f32; 3]) -> i32 {
    let delta = sub(to, from);
    let distance_sq = dot(delta, delta);
    let curve = def.damage_curve;
    if distance_sq <= curve[0].1 * curve[0].1 {
        return curve[0].0;
    }
    if distance_sq > curve[5].1 * curve[5].1 {
        return curve[5].0;
    }
    let mut low = 0;
    let mut high = 1;
    loop {
        let range = curve[high].1;
        if range >= 0.001 && range * range > distance_sq {
            break;
        }
        if range >= 0.001 {
            low = high;
        }
        high += 1;
        if high > 5 {
            return curve[0].0;
        }
    }
    let share = (distance_sq.sqrt() - curve[low].1) / (curve[high].1 - curve[low].1);
    (curve[low].0 as f32 * (1.0 - share) + curve[high].0 as f32 * share) as i32
}

/// The hit location a trace struck, by Black Ops 2's numbers.
fn hit_location(collider: ColliderId) -> usize {
    match collider {
        ColliderId::EntityDObjBone {
            part_classification,
            ..
        } => usize::from(part_classification),
        // A player's part is its body model's own part classification.
        ColliderId::Player { hitloc, .. } => Some(usize::from(hitloc))
            .filter(|&part| part < damage::HIT_LOCATIONS.len())
            .unwrap_or(0),
        _ => 0,
    }
}

fn surface_flags(collider: ColliderId) -> u32 {
    match collider {
        ColliderId::World { surface_flags, .. }
        | ColliderId::EntityDObjBone { surface_flags, .. }
        | ColliderId::EntityLinkedBrush { surface_flags, .. } => surface_flags,
        ColliderId::Player { .. } => 0,
    }
}

fn takes_damage(world: &mut World, id: u64) -> bool {
    let sentient = world.resource::<RoundScript>().player_client(id).is_some()
        || damage::is_actor(world, &Value::Object(id));
    if sentient {
        return !matches!(field(world, id, "takedamage"), Value::Int(0));
    }
    world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| entity.can_damage)
}

/// The end of a bullet's spent course: a bouncing weapon's ricochet is not
/// known.
fn bullet_end(world: &World, def: &ZoneWeapon) -> Result<(), String> {
    if def.bullet_bounce && dvar_i32(world, BULLET_BOUNCE.0, BULLET_BOUNCE.1) != 0 {
        return Err(format!(
            "t6.turret.bullet_bounce: '{}' bounces its bullets, which is not implemented yet",
            def.name
        ));
    }
    Ok(())
}

/// What a bullet's trace struck.
#[derive(Clone)]
struct Struck {
    point: [f32; 3],
    normal: [f32; 3],
    collider: ColliderId,
    entity: Value,
    flags: u32,
    /// The surface whose depth the bullet passes.
    depth_type: u32,
    start_solid: bool,
    all_solid: bool,
}

/// A players' or an actor's body.
fn is_body(world: &World, id: u64) -> bool {
    let runtime = world.resource::<RoundScript>();
    runtime.player_client(id).is_some()
        || runtime
            .entities
            .get(&id)
            .is_some_and(|entity| entity.kind == super::EntityKind::Actor)
}

/// The engine's bullet trace: a body is struck as flesh and a riot shield
/// as its own surface; a surface of no type keeps the last one's depth,
/// one flagged against penetration has none.
fn bullet_trace(
    world: &mut World,
    start: [f32; 3],
    end: [f32; 3],
    dir: [f32; 3],
    ignore: &Value,
    last_type: u32,
) -> Option<Struck> {
    let mask = FrameWorld::from_world(world).bullet_mask();
    let ignored = trace_ignore(world, Some(ignore));
    let (point, normal, collider, start_solid, all_solid) =
        match entity_trace(world, start, end, mask, ignored) {
            TraceOutcome::Hit {
                end,
                normal,
                collider,
                ..
            } => (end, normal, collider, false, false),
            TraceOutcome::StartSolid {
                collider: Some(collider),
                end,
                all_solid,
            } => (end, dir.map(|v| -v), collider, true, all_solid),
            _ => return None,
        };
    let entity = collider_entity(world, collider);
    let mut flags = surface_flags(collider);
    if let Value::Object(id) = entity {
        if hit_location(collider) == damage::RIOTSHIELD {
            flags = RIOTSHIELD_FLAGS;
        } else if flags == 0 && is_body(world, id) {
            flags = FLESH_FLAGS;
        }
    }
    let surface = (flags >> 20) & 0x3f;
    let depth_type = if flags & SURF_NO_PENETRATE != 0 {
        0
    } else if surface == 0 {
        last_type
    } else {
        surface
    };
    Some(Struck {
        point,
        normal,
        collider,
        entity,
        flags,
        depth_type,
        start_solid,
        all_solid,
    })
}

/// The engine's bullet hit: the thing struck takes the weapon's damage for
/// the distance from the muzzle, scaled.
#[allow(clippy::too_many_arguments)]
fn strike(
    world: &mut World,
    id: u64,
    def: &ZoneWeapon,
    muzzle: [f32; 3],
    dir: [f32; 3],
    struck: &Struck,
    damage_scale: f32,
    dflags: i32,
) -> Result<(), String> {
    let Value::Object(target) = struck.entity else {
        return Ok(());
    };
    let location = hit_location(struck.collider);
    if location == damage::RIOTSHIELD {
        return Err("t6.turret.riotshield_hit: a turret's bullet striking a riot shield is not implemented yet".into());
    }
    if !takes_damage(world, target) {
        return Ok(());
    }
    let turret = Value::Object(id);
    let amount = (damage_at(def, muzzle, struck.point) as f32 * damage_scale) as i32;
    damage::damage(
        world,
        &struck.entity,
        damage::Hit {
            inflictor: turret.clone(),
            attacker: turret,
            dir: Some(dir),
            point: Some(struck.point),
            damage: amount,
            dflags: dflags
                | if def.armor_piercing {
                    DFLAG_ARMOR_PIERCING
                } else {
                    0
                },
            means: if def.rifle_bullet {
                MOD_RIFLE_BULLET
            } else {
                MOD_PISTOL_BULLET
            },
            weapon: Some(def.name.clone()),
            hit_location: location,
            located: false,
            time_offset: 0,
        },
    )
}

/// The engine's advance past a hit: past an entity from the hit, leaving
/// it out; past the world `distance` on over the facing (eight times, and
/// spent, when the bullet grazes it).
fn advance(struck: &Struck, dir: [f32; 3], distance: f32) -> (bool, [f32; 3], Value) {
    if matches!(struck.entity, Value::Object(_)) || distance <= 0.0 {
        return (true, struck.point, struck.entity.clone());
    }
    let facing = -dot(struck.normal, dir);
    if facing < GRAZE {
        return (
            false,
            add(struck.point, scale(dir, distance * 8.0)),
            Value::Undefined,
        );
    }
    (
        true,
        add(struck.point, scale(dir, distance / facing)),
        Value::Undefined,
    )
}

/// One bullet from the muzzle to the end: each thing struck takes the
/// weapon's damage for the distance from the muzzle; it goes on through
/// surfaces that let it pass and, at half damage, through bodies a rifle
/// bullet strikes.
fn bullet(
    world: &mut World,
    id: u64,
    def: &ZoneWeapon,
    muzzle: [f32; 3],
    end: [f32; 3],
    dir: [f32; 3],
) -> Result<(), String> {
    let mut start = muzzle;
    let mut ignore = Value::Object(id);
    let mut damage_scale = 1.0;
    for _ in 0..BULLET_HITS {
        let Some(struck) = bullet_trace(world, start, end, dir, &ignore, 0) else {
            return Ok(());
        };
        strike(world, id, def, muzzle, dir, &struck, damage_scale, 0)?;
        if struck.flags & SURF_PASS != 0 {
            let (advanced, from, left_out) = advance(&struck, dir, PASS_OFFSET);
            if !advanced {
                return Ok(());
            }
            (start, ignore) = (from, left_out);
            continue;
        }
        let Value::Object(target) = struck.entity else {
            return bullet_end(world, def);
        };
        if !takes_damage(world, target) {
            return bullet_end(world, def);
        }
        if !struck.start_solid {
            if !def.rifle_bullet || !is_sentient(world, target) {
                return bullet_end(world, def);
            }
            damage_scale *= 0.5;
        }
        (start, ignore) = (struck.point, struck.entity);
    }
    Ok(())
}

/// The zone's penetration depths by penetration type (none, small, medium,
/// large) and surface type.
fn depth_table(world: &World) -> Result<[[f32; 32]; 4], String> {
    let text = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.raw_files.get(PENETRATION_TABLE).cloned())
        .ok_or_else(|| format!("{PENETRATION_TABLE} is not in the zones"))?;
    let mut words = text.trim().split('\\');
    if words.next() != Some(PENETRATION_TABLE_HEADER) {
        return Err(format!(
            "{PENETRATION_TABLE} is not a bullet penetration table"
        ));
    }
    let mut depths = [[0.0; 32]; 4];
    while let (Some(key), Some(value)) = (words.next(), words.next()) {
        let Some((size, surface)) = key.split_once('_') else {
            continue;
        };
        if let Some(row) = PENETRATION_TYPES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(size))
            && let Some(column) = super::entity::SURFACES
                .iter()
                .position(|name| name.eq_ignore_ascii_case(surface))
        {
            depths[row + 1][column] = super::atof(value);
        }
    }
    Ok(depths)
}

/// The depth of a surface for the weapon's penetration type (none for a
/// surface of no type).
fn depth(table: &[[f32; 32]; 4], def: &ZoneWeapon, surface: u32) -> f32 {
    if surface == 0 {
        return 0.0;
    }
    usize::try_from(def.penetrate_type)
        .ok()
        .and_then(|row| table.get(row))
        .and_then(|row| row.get(surface as usize))
        .copied()
        .unwrap_or(0.0)
}

/// The engine's penetrating bullet: after the first hit it passes up to
/// `penetrationCount` surfaces (five for a charge shot), each costing the
/// damage scale its thickness over the lesser depth of its entry and exit,
/// and the next thing struck takes the scaled damage. The attacker's perks
/// and charge are a player's; the turret has none.
fn penetrate(
    world: &mut World,
    id: u64,
    def: &ZoneWeapon,
    muzzle: [f32; 3],
    end: [f32; 3],
    dir: [f32; 3],
) -> Result<(), String> {
    let Some(mut struck) = bullet_trace(world, muzzle, end, dir, &Value::Object(id), 0) else {
        return Ok(());
    };
    strike(world, id, def, muzzle, dir, &struck, 1.0, 0)?;
    let table = depth_table(world)?;
    let count = if def.fire_type == FIRE_CHARGE_SHOT {
        CHARGE_SHOT_PENETRATIONS
    } else {
        dvar_i32(world, PENETRATION_COUNT.0, PENETRATION_COUNT.1)
    };
    let back = scale(dir, -1.0);
    let mut damage_scale = 1.0;
    for _ in 0..count {
        let mut passes = depth(&table, def, struck.depth_type);
        if struck.flags & SURF_FULL_DEPTH != 0 || (struck.flags >> 20) & 0x3f == WATER {
            passes = FULL_DEPTH;
        }
        if passes <= 0.0 {
            return Ok(());
        }
        let entry = struck.point;
        let (advanced, from, ignore) = advance(&struck, dir, PASS_OFFSET);
        if !advanced {
            return Ok(());
        }
        let next = bullet_trace(world, from, end, dir, &ignore, struck.depth_type);
        let exit_end = add(entry, scale(back, EXIT_BACK));
        let (mut exit_start, mut exit_ignore) = (end, ignore);
        if let Some(next) = &next {
            let mirrored = Struck {
                normal: scale(next.normal, -1.0),
                ..next.clone()
            };
            (_, exit_start, exit_ignore) = advance(&mirrored, back, EXIT_BACK);
        }
        let last_type = next.as_ref().map_or(0, |next| next.depth_type);
        let exit = bullet_trace(world, exit_start, exit_end, back, &exit_ignore, last_type);
        let all_solid = exit.as_ref().is_some_and(|exit| exit.all_solid)
            || (next.as_ref().is_some_and(|next| next.start_solid)
                && exit.as_ref().is_some_and(|exit| exit.start_solid));
        if exit.is_some() || all_solid {
            let through = if all_solid {
                sub(exit_end, exit_start)
            } else {
                sub(entry, exit.as_ref().map_or(entry, |exit| exit.point))
            };
            let thickness = dot(through, through).sqrt().max(1.0);
            if let Some(exit) = &exit {
                passes = passes.min(depth(&table, def, exit.depth_type));
                if exit.flags & SURF_FULL_DEPTH != 0 {
                    passes = FULL_DEPTH;
                } else if passes <= 0.0 {
                    return Ok(());
                }
            }
            damage_scale -= thickness / passes;
            if damage_scale <= 0.0 {
                return Ok(());
            }
        }
        let Some(next) = next else {
            return Ok(());
        };
        if !all_solid {
            strike(
                world,
                id,
                def,
                muzzle,
                dir,
                &next,
                damage_scale,
                DFLAG_PENETRATION,
            )?;
        }
        struck = next;
    }
    Ok(())
}

/// The engine's bullet fire for a shot from the muzzle: one bullet out to
/// `bulletrange`, within the spread cone (a flat-spread weapon's single
/// shot goes straight).
fn bullet_fire(
    world: &mut World,
    id: u64,
    def: &ZoneWeapon,
    spread: f32,
    muzzle: presence::TagPose,
    now: i64,
) -> Result<(), String> {
    if SPREAD_CLASSES.contains(&def.weap_class) {
        return Err(format!(
            "t6.turret.pellets: '{}' fires pellets from a turret, which is not implemented yet",
            def.name
        ));
    }
    if def.bullet_impact_explode {
        return Err(format!(
            "t6.turret.impact_explode: '{}' explodes on impact, which is not implemented yet",
            def.name
        ));
    }
    let range = dvar_f32(world, BULLET_RANGE);
    let (origin, [forward, left, up]) = muzzle;
    let mut seed = scramble(now as u32);
    let cone = spread.to_radians().tan() * range;
    let turn = (random(&mut seed) * 360.0).to_radians();
    skip_randoms(&mut seed, 4);
    let radius = random(&mut seed);
    let (mut across, mut high) = (turn.cos() * radius, turn.sin() * radius);
    if def.flat_spread {
        across = 0.0;
        high = 0.0;
    }
    let end: [f32; 3] = std::array::from_fn(|i| {
        origin[i] + forward[i] * range + left[i] * across * cone + up[i] * high * cone
    });
    let dir = normalize(sub(end, origin));
    if def.penetrate_type != 0 {
        return penetrate(world, id, def, origin, end, dir);
    }
    bullet(world, id, def, origin, end, dir)
}

/// `shootturret`: each of the fire's shots from `tag_flash` along the gun,
/// with the turret's AI spread (nobody uses it, so it is its own attacker).
pub(super) fn shoot(world: &mut World, id: u64) -> Result<(), String> {
    let turret = world.resource::<Turrets>().0[&id].clone();
    let def = turret.def.clone();
    if def.weap_type != 0 {
        return Err(format!(
            "t6.turret.projectile: '{}' fires no bullets, which is not implemented yet",
            def.name
        ));
    }
    let shots = match def.fire_barrels {
        1 | 5 => 2,
        3 => 4,
        _ => 1,
    };
    let now = now_ms(world);
    for _ in 0..shots {
        let muzzle = gun_tag(world, id, turret.aim.gun, "tag_flash")
            .ok_or_else(|| format!("turret '{}' has no tag_flash", turret.weapon))?;
        bullet_fire(world, id, &def, turret.aim.spread[0], muzzle, now)?;
    }
    Ok(())
}
