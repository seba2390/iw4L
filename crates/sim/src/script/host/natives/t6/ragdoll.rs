//! Black Ops 2's ragdolls. `startragdoll` turns an entity's trajectory into
//! a ragdoll one (`isragdoll` reads it back, a corpse keeps it); the client
//! game then builds a ragdoll on the body from the game's `ragdoll.cfg`
//! (always its definition 1), moving off as the animation moved it, and
//! steps it until it settles. `launchragdoll` (server and client) pushes it,
//! and grenade and rocket explosions and the physics explosion builtins push
//! the four nearest. IW4L steps the ragdoll where it poses actors (the
//! authority's presentation) on the neutral stepper in `xmodel_runtime`,
//! whose bones replace the animation's in the body's pose.

use crate::frame::FrameWorld;
use crate::script::RoundScript;
use bevy_ecs::prelude::{Resource, World};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use std::sync::Arc;
use xmodel_runtime::{
    Ragdoll, RagdollBoneDef, RagdollDef, RagdollJointDef, RagdollLerpBoneDef, RagdollLimitDef,
    RagdollStartRules, RagdollState, RagdollSweep, RagdollTuning, RagdollWorld,
};

/// The config the client game executes as it starts.
const CONFIG: &str = "ragdoll.cfg";
/// The definition the engine builds every ragdoll with.
const DEFINITION: &str = "1";
/// `startragdoll`'s trajectory types: from an interpolated one, from a
/// gravity one, from any other.
const TR_INTERPOLATE: u8 = 1;
const TR_GRAVITY: u8 = 6;
const TR_RAGDOLL: u8 = 12;
const TR_RAGDOLL_GRAVITY: u8 = 13;
const TR_RAGDOLL_INTERPOLATE: u8 = 14;
/// A ragdoll whose bodies average below this height has left the level.
const FELL_OUT_HEIGHT: f32 = -2500.0;
/// The start velocity: per second from the two poses, at most this fast,
/// and at most this fast upwards.
const START_SPEED_MAX: f32 = 600.0;
const START_UP_SPEED_MAX: f32 = 50.0;
/// The most ragdolls one explosion pushes, nearest first.
const EXPLOSION_RAGDOLLS: usize = 4;
/// The level surfaces ragdolls rest on (t6.actors.ragdoll_solver: the physics
/// world's own collision filter is not read).
const CONTENTS_SOLID: u32 = 1;

/// Each joint's friction grows by up to this much of itself over
/// `ragdoll_jointlerp_time`.
const JOINT_FRICTION_RAMP: f32 = 1.0;

/// The dvars the ragdoll reads, with their registered defaults.
const ENABLE: (&str, f32) = ("ragdoll_enable", 1.0);
const MAX_LIFE: (&str, f32) = ("ragdoll_max_life", 9000.0);
const EXPLODE_FORCE: (&str, f32) = ("ragdoll_explode_force", 18.0);
const EXPLODE_UPBIAS: (&str, f32) = ("ragdoll_explode_upbias", 0.8);
const BULLET_FORCE: (&str, f32) = ("ragdoll_bullet_force", 0.5);
const BASELERP_TIME: (&str, f32) = ("ragdoll_baselerp_time", 1000.0);
const JOINTLERP_TIME: (&str, f32) = ("ragdoll_jointlerp_time", 3000.0);
const ROTVEL_SCALE: (&str, f32) = ("ragdoll_rotvel_scale", 1.0);
const JITTER_SCALE: (&str, f32) = ("ragdoll_jitter_scale", 1.0);
const SELF_COLLISION_SCALE: (&str, f32) = ("ragdoll_self_collision_scale", 1.2);
const REACTIVATION_CUTOFF: (&str, f32) = ("ragdoll_reactivation_cutoff", 1200.0);
const GRAVITY: (&str, f32) = ("phys_gravity", -800.0);

fn dvar(world: &World, (name, default): (&str, f32)) -> f32 {
    let dvars = &world.resource::<crate::script::MatchScript>().dvars;
    dvars
        .get(name)
        .or_else(|| dvars.get(&name.to_ascii_lowercase()))
        .map_or(default, |value| super::atof(value))
}

/// A ragdoll body on an entity: the trajectory type `startragdoll` gave it,
/// the pose it had a step before its ragdoll starts, and the ragdoll.
#[derive(Default)]
struct Body {
    tr_type: u8,
    previous: Option<Vec<Mat4>>,
    ragdoll: Option<Ragdoll>,
    /// Pushes that came before the ragdoll did.
    launches: Vec<([f32; 3], Option<String>)>,
}

#[derive(Resource, Default)]
struct Ragdolls {
    definition: Option<Option<Arc<RagdollDef>>>,
    bodies: BTreeMap<u64, Body>,
    started: u32,
}

fn ragdolls(world: &mut World) -> &mut Ragdolls {
    if !world.contains_resource::<Ragdolls>() {
        world.insert_resource(Ragdolls::default());
    }
    world.resource_mut::<Ragdolls>().into_inner()
}

/// Definition 1 of the zones' `ragdoll.cfg`, read once.
fn definition(world: &mut World) -> Option<Arc<RagdollDef>> {
    if let Some(found) = world
        .get_resource::<Ragdolls>()
        .and_then(|r| r.definition.clone())
    {
        return found;
    }
    let text = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.raw_files.get(CONFIG).cloned());
    let baselerp = dvar(world, BASELERP_TIME) as u32;
    let parsed = text.and_then(|text| parse(&text, baselerp)).map(Arc::new);
    if parsed.is_none() {
        diag::warn!(
            Sim,
            "t6.actors.ragdoll: no ragdoll definition {DEFINITION} in {CONFIG}"
        );
    }
    ragdolls(world).definition = Some(parsed.clone());
    parsed
}

/// Definition 1's commands in the config, as the engine's `ragdoll_bone`,
/// `ragdoll_baselerp_bone`, `ragdoll_joint`, `ragdoll_limit` and
/// `ragdoll_selfpair` read them. A bone's capsule reaches its child bone;
/// the fourth body's (the neck's, up to the head) 2 further.
fn parse(text: &str, baselerp: u32) -> Option<RagdollDef> {
    const NECK_BODY: usize = 3;
    const NECK_EXTRA: f32 = 2.0;
    let mut def = RagdollDef::default();
    let mut limits: Vec<[Option<RagdollLimitDef>; 3]> = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("");
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.get(1) != Some(&DEFINITION) {
            continue;
        }
        let num = |index: usize| words.get(index).map_or(0.0, |w| super::atof(w));
        match words[0].to_ascii_lowercase().as_str() {
            "ragdoll_bone" if words.len() >= 10 => {
                let parent = super::atoi(words[8]);
                def.bones.push(RagdollBoneDef {
                    p0: words[2].to_owned(),
                    p1: words[3].to_owned(),
                    radius: num(4),
                    cog_lerp: num(5),
                    mass: num(6),
                    friction: num(7),
                    parent: usize::try_from(parent).ok(),
                    extra_half_length: if def.bones.len() == NECK_BODY {
                        NECK_EXTRA
                    } else {
                        0.0
                    },
                });
            }
            "ragdoll_baselerp_bone" if words.len() >= 3 => {
                let millis = words
                    .get(3)
                    .map_or(baselerp, |w| super::atoi(w).clamp(100, 6000) as u32);
                def.lerp_bones.push(RagdollLerpBoneDef {
                    tag: words[2].to_owned(),
                    millis,
                });
            }
            "ragdoll_joint" if words.len() >= 4 => {
                def.joints.push(RagdollJointDef {
                    bone: usize::try_from(super::atoi(words[2])).ok()?,
                    hinge: words[3].eq_ignore_ascii_case("hinge"),
                    limits: [None; 3],
                });
                limits.push([None; 3]);
            }
            "ragdoll_limit" if words.len() >= 7 => {
                let joint = usize::try_from(super::atoi(words[2])).ok()?;
                let axis = match words[3].to_ascii_lowercase().as_str() {
                    "x" => 0,
                    "y" => 1,
                    "z" => 2,
                    _ => continue,
                };
                if let Some(slot) = limits.get_mut(joint) {
                    slot[axis] = Some(RagdollLimitDef {
                        friction: num(4),
                        min: num(5).to_radians(),
                        max: num(6).to_radians(),
                    });
                }
            }
            "ragdoll_selfpair" if words.len() >= 4 => {
                let a = usize::try_from(super::atoi(words[2])).ok()?;
                let b = usize::try_from(super::atoi(words[3])).ok()?;
                def.self_pairs.push((a, b));
            }
            _ => {}
        }
    }
    for (joint, limits) in def.joints.iter_mut().zip(limits) {
        joint.limits = limits;
    }
    (!def.bones.is_empty()).then_some(def)
}

/// `startragdoll`: the entity's trajectory becomes a ragdoll one.
pub(super) fn start(world: &mut World, id: u64) {
    let tr_type = match world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .map(|e| &e.kind)
    {
        // Actors and players move by interpolation; missiles fall.
        Some(
            super::super::super::entities::EntityKind::Actor
            | super::super::super::entities::EntityKind::Player
            | super::super::super::entities::EntityKind::ActorCorpse,
        ) => TR_INTERPOLATE,
        Some(super::super::super::entities::EntityKind::Missile { .. }) => TR_GRAVITY,
        _ => 0,
    };
    let tr_type = match tr_type {
        TR_INTERPOLATE => TR_RAGDOLL_INTERPOLATE,
        TR_GRAVITY => TR_RAGDOLL_GRAVITY,
        _ => TR_RAGDOLL,
    };
    ragdolls(world).bodies.entry(id).or_default().tr_type = tr_type;
    diag::debug!(Sim, "t6zm: startragdoll entity={id} trType={tr_type}");
}

/// `isragdoll`: whether the entity's trajectory is a ragdoll one.
pub(super) fn is_ragdoll(world: &World, id: u64) -> bool {
    world
        .get_resource::<Ragdolls>()
        .and_then(|r| r.bodies.get(&id))
        .is_some_and(|body| (TR_RAGDOLL..=TR_RAGDOLL_INTERPOLATE).contains(&body.tr_type))
}

/// `launchragdoll`: pushes a ragdolling entity along `direction` (on one
/// body when `bone` names one of its bones); nothing for another entity.
pub(super) fn launch(world: &mut World, id: u64, direction: [f32; 3], bone: Option<String>) {
    if !is_ragdoll(world, id) {
        return;
    }
    let Some(def) = definition(world) else {
        return;
    };
    let force = dvar(world, BULLET_FORCE);
    let share = if bone.is_some() {
        1.0
    } else {
        def.bones.len() as f32
    };
    let impulse = direction.map(|v| v * (force / share));
    diag::debug!(Sim, "t6zm: launchragdoll entity={id} dir={direction:?}");
    let body = ragdolls(world).bodies.entry(id).or_default();
    match body.ragdoll.as_mut() {
        Some(ragdoll) => ragdoll.launch(&def, impulse, bone.as_deref()),
        None if body.launches.len() < 2 => body.launches.push((impulse, bone)),
        None => {}
    }
}

/// A corpse takes over the actor's ragdoll.
pub(super) fn carry(world: &mut World, from: u64, to: u64) {
    let state = ragdolls(world);
    if let Some(body) = state.bodies.remove(&from) {
        state.bodies.insert(to, body);
    }
}

/// The view a player watches ragdolls from: the first player's eye.
fn view(world: &mut World) -> Option<[f32; 3]> {
    let first = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(_, e)| e.kind == super::super::super::entities::EntityKind::Player)
            .min_by_key(|(_, e)| e.number)
            .and_then(|(id, _)| runtime.player_client(*id))
    }?;
    crate::script_player::eye(&FrameWorld::from_world(world), crate::ClientId(first))
}

/// An explosion's push on the four ragdolls nearest it, each still within
/// `ragdoll_reactivation_cutoff` of the view and whose root lies inside the
/// outer radius (on the level plane for a cylinder). Each body, at a point
/// jittered by up to 1 on each axis, takes the explosion's magnitude inside
/// the inner radius and less out to the outer, away from the explosion (or
/// along `direction`), lifted by `ragdoll_explode_upbias`, as an impulse of
/// `ragdoll_explode_force` shared among the bodies. A pushed ragdoll wakes
/// and its life restarts.
pub(super) fn explosion(
    world: &mut World,
    cylinder: bool,
    origin: [f32; 3],
    inner: f32,
    outer: f32,
    direction: Option<[f32; 3]>,
    magnitude: f32,
) {
    if outer == 0.0 {
        return;
    }
    let Some(view) = view(world) else {
        return;
    };
    let cutoff = dvar(world, REACTIVATION_CUTOFF);
    let force = dvar(world, EXPLODE_FORCE);
    let upbias = dvar(world, EXPLODE_UPBIAS);
    let jitter = dvar(world, JITTER_SCALE);
    let origin = Vec3::from_array(origin);
    let (outer_sq, inner_sq) = (outer * outer, inner * inner);
    let falloff = if inner_sq < outer_sq {
        1.0 / (outer_sq - inner_sq)
    } else {
        0.0
    };
    let state = ragdolls(world);
    let mut nearest: Vec<(f32, u64)> = state
        .bodies
        .iter()
        .filter_map(|(id, body)| {
            let root = Vec3::from_array(body.ragdoll.as_ref()?.root());
            Some((root.distance_squared(origin), *id))
        })
        .collect();
    nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut pushed = 0;
    for (_, id) in nearest.into_iter().take(EXPLOSION_RAGDOLLS) {
        let Some(ragdoll) = state.bodies.get_mut(&id).and_then(|b| b.ragdoll.as_mut()) else {
            continue;
        };
        let root = Vec3::from_array(ragdoll.root());
        if root.distance_squared(Vec3::from_array(view)) > cutoff * cutoff {
            continue;
        }
        let mut offset = origin - root;
        if cylinder {
            offset.z = 0.0;
        }
        if offset.length_squared() >= outer_sq {
            continue;
        }
        let share = 1.0 / ragdoll.body_count() as f32;
        ragdoll.life_ms = 0;
        let centres: Vec<[f32; 3]> = ragdoll.body_centers().collect();
        for (index, centre) in centres.into_iter().enumerate() {
            let spot = Vec3::from_array(centre)
                + Vec3::new(
                    jitter_unit(id, index, 0) * jitter,
                    jitter_unit(id, index, 1) * jitter,
                    jitter_unit(id, index, 2) * jitter,
                );
            let mut away = spot - origin;
            let distance_sq = away.length_squared();
            if distance_sq >= outer_sq {
                continue;
            }
            let scale = if distance_sq <= inner_sq {
                magnitude
            } else {
                (outer_sq - distance_sq) * falloff * magnitude
            };
            let mut push = match direction {
                Some(direction) => Vec3::from_array(direction),
                None => {
                    if cylinder {
                        away.z = 0.0;
                    }
                    away.normalize_or(Vec3::Z)
                }
            };
            push.z += upbias;
            let push = push.normalize_or(Vec3::Z) * (force * scale * share);
            ragdoll.push_body(
                index,
                push.to_array(),
                (spot - Vec3::from_array(centre)).to_array(),
            );
        }
        ragdoll.wake();
        pushed += 1;
    }
    if pushed > 0 {
        diag::debug!(
            Sim,
            "t6zm: ragdoll explosion cylinder={cylinder} at={:?} outer={outer} pushed={pushed}",
            origin.to_array()
        );
    }
}

/// A fixed spread in [-1, 1] for a body's push point.
fn jitter_unit(id: u64, body: usize, axis: u64) -> f32 {
    let mut x = id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((body as u64) << 8 | axis);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// Sweeps a ragdoll's spheres through the level's solid clip.
struct Level<'a>(&'a FrameWorld<'a>);

impl RagdollWorld for Level<'_> {
    fn sweep_sphere(&mut self, start: [f32; 3], end: [f32; 3], radius: f32) -> RagdollSweep {
        let half = [radius; 3];
        let trace = self
            .0
            .trace_clip(start, end, half.map(|h| -h), half, CONTENTS_SOLID);
        if trace.startsolid != 0 || trace.allsolid != 0 {
            RagdollSweep::StartSolid
        } else if trace.fraction < 1.0 {
            RagdollSweep::Hit {
                fraction: trace.fraction,
                normal: trace.normal,
            }
        } else {
            RagdollSweep::Clear
        }
    }
}

/// Steps every ragdoll a tick and poses its body: an entity whose trajectory
/// became a ragdoll one keeps its pose for a tick, then starts its ragdoll
/// moving as the animation moved it between them.
pub(crate) fn present(world: &mut World) {
    if world
        .get_resource::<Ragdolls>()
        .is_none_or(|r| r.bodies.is_empty())
    {
        return;
    }
    let ids: Vec<u64> = ragdolls(world).bodies.keys().copied().collect();
    let gone: Vec<u64> = {
        let runtime = world.resource::<RoundScript>();
        ids.iter()
            .copied()
            .filter(|id| !runtime.entities.contains_key(id))
            .collect()
    };
    for id in gone {
        ragdolls(world).bodies.remove(&id);
    }
    let Some(def) = definition(world) else {
        return;
    };
    let enabled = dvar(world, ENABLE) != 0.0;
    let max_life = dvar(world, MAX_LIFE) as u32;
    let rules = RagdollStartRules {
        dt_ms: crate::MATCH_TICK_MS as f32,
        max_speed: START_SPEED_MAX,
        max_up_speed: START_UP_SPEED_MAX,
        angular_scale: dvar(world, ROTVEL_SCALE),
    };
    let gravity = dvar(world, GRAVITY);
    let jointlerp = dvar(world, JOINTLERP_TIME).max(1.0);
    let self_collision = dvar(world, SELF_COLLISION_SCALE);
    let jitter = dvar(world, JITTER_SCALE);
    for (origin, radius) in explosions(world) {
        explosion(world, false, origin, 0.0, radius, None, 1.0);
    }
    let ids: Vec<u64> = ragdolls(world).bodies.keys().copied().collect();
    for id in ids {
        let Some(presence) = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .and_then(|e| e.presence)
        else {
            continue;
        };
        let mut body = std::mem::take(ragdolls(world).bodies.get_mut(&id).unwrap());
        if (TR_RAGDOLL..=TR_RAGDOLL_INTERPOLATE).contains(&body.tr_type) && enabled {
            step_body(
                world,
                id,
                presence,
                &def,
                &mut body,
                rules,
                gravity,
                jointlerp,
                self_collision,
                jitter,
                max_life,
            );
        }
        if let Some(slot) = ragdolls(world).bodies.get_mut(&id) {
            *slot = body;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn step_body(
    world: &mut World,
    id: u64,
    presence: crate::ScriptModelId,
    def: &RagdollDef,
    body: &mut Body,
    rules: RagdollStartRules,
    gravity: f32,
    jointlerp: f32,
    self_collision: f32,
    jitter: f32,
    max_life: u32,
) {
    if body
        .ragdoll
        .as_ref()
        .is_some_and(|r| r.state == RagdollState::Settled)
    {
        return;
    }
    let Some((world_from_model, (skeleton, animated))) = FrameWorld::from_world(world)
        .collision_owner_mut(presence)
        .and_then(|row| row.dobj.as_ref())
        .and_then(|dobj| Some((dobj.world_from_model, dobj.animated_bones()?)))
    else {
        return;
    };
    let find = |name: &str| skeleton.find(name);
    if body.ragdoll.is_none() {
        let Some(previous) = body.previous.take() else {
            body.previous = Some(animated);
            return;
        };
        let seed = world.get_resource::<Ragdolls>().map_or(0, |r| r.started) ^ (id as u32);
        let Some(mut ragdoll) = Ragdoll::start(def, find, &animated, &previous, rules, seed) else {
            diag::warn!(
                Sim,
                "t6.actors.ragdoll: entity {id}'s body lacks ragdoll bones"
            );
            body.tr_type = 0;
            return;
        };
        for (impulse, bone) in body.launches.drain(..) {
            ragdoll.launch(def, impulse, bone.as_deref());
        }
        ragdolls(world).started += 1;
        diag::debug!(
            Sim,
            "t6zm: ragdoll started entity={id} root={:?}",
            ragdoll.root()
        );
        body.ragdoll = Some(ragdoll);
    }
    let Some(ragdoll) = body.ragdoll.as_mut() else {
        return;
    };
    let ramp = 1.0 + JOINT_FRICTION_RAMP * (ragdoll.life_ms as f32 / jointlerp).clamp(0.0, 1.0);
    let tuning = RagdollTuning {
        gravity: [0.0, 0.0, gravity],
        self_collision_scale: self_collision,
        joint_friction_scale: ramp,
    };
    {
        let frame = FrameWorld::from_world(world);
        ragdoll.step(
            def,
            tuning,
            crate::MATCH_TICK_MS as u32,
            jitter,
            &mut Level(&frame),
        );
    }
    if ragdoll.mean_height() < FELL_OUT_HEIGHT || ragdoll.life_ms > max_life {
        ragdoll.state = RagdollState::Settled;
        diag::debug!(
            Sim,
            "t6zm: ragdoll settled entity={id} root={:?}",
            ragdoll.root()
        );
    }
    let mut overrides = Vec::with_capacity(def.bones.len() + def.lerp_bones.len());
    ragdoll.pose(def, find, &animated, world_from_model, &mut overrides);
    if let Some(dobj) = FrameWorld::from_world(world)
        .collision_owner_mut(presence)
        .and_then(|row| row.dobj.as_mut())
    {
        dobj.set_bone_overrides(overrides);
        dobj.materialize();
    }
}

/// This tick's grenade and rocket explosions: where, and their weapon's
/// explosion radius.
fn explosions(world: &mut World) -> Vec<([f32; 3], f32)> {
    let frame = FrameWorld::from_world(world);
    frame
        .entity_events()
        .iter()
        .filter(|record| {
            matches!(
                record.event.taxonomy_name(),
                Some("EV_GRENADE_EXPLODE" | "EV_ROCKET_EXPLODE")
            )
        })
        .filter_map(|record| {
            let facts = frame.equipment_facts_for(record.payload.weapon)?;
            Some((record.payload.origin, facts.explosion_radius as f32))
        })
        .collect()
}
