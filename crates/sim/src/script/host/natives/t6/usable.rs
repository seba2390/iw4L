//! Black Ops 2's use rule: which entity a player's use reaches, as the
//! engine decides it each frame (its search of the usable entities near the
//! player, then its pick of the one whose hint shows) and fires it when the
//! player presses use. What IW4L does not do of it is listed under
//! `t6.use.rule` in `docs/fidelity/t6.md`.

use super::super::super::entities::EntityKind;
use super::{EntityBounds, EntityFlags};
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;

/// The player's box for touching a trigger: 15 each way, 70 high, whatever
/// the stance.
const PLAYER_HALF: f32 = 15.0;
const PLAYER_HEIGHT: f32 = 70.0;
/// The search reaches at least this far each way flat, and this far up and
/// down, from the eye.
const SEARCH_MIN: f32 = 384.0;
const SEARCH_HEIGHT: f32 = 96.0;
/// `player_throwbackOuterRadius_zm`'s default: the search reaches this far
/// when it is more than `SEARCH_MIN`.
const THROWBACK_OUTER_RADIUS_ZM: f32 = 192.0;
/// `player_useRadius_zm`'s default, which the scripts raise.
const USE_RADIUS_ZM: f32 = 72.0;
const USE_RADIUS_DVAR: &str = "player_useradius_zm";
/// A trigger that asks to be looked at wants its middle within this dot of
/// the view.
const LOOK_DOT: f32 = 0.76;
/// A trigger the player stands in comes before every other.
const TOUCH_SCORE: f32 = -144.0;
/// How the facing adds to a candidate's distance: 0 looking at it, this
/// much looking away.
const FACING_SCORE: f32 = 512.0;
/// The bonuses a use trigger and a radius, box or radius use trigger with a
/// cursor hint take.
const USE_BONUS: f32 = 512.0;
const CURSOR_BONUS: f32 = 768.0;
/// A script mover's bonus.
const MOVER_BONUS: f32 = 384.0;
/// The sight line from the eye to a candidate's middle: solid and glass.
const SIGHT_MASK: u32 = 0x11;
/// What a candidate the eye cannot see adds to its score: it goes after
/// every one it can, and is still picked when nothing else is.
const UNSEEN_SCORE: f32 = 10_000.0;
/// The cursor hint a map's `trigger_use` and `trigger_use_touch` start
/// with; a spawned trigger starts with none.
const MAP_USE_CURSOR: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Use,
    UseTouch,
    Radius,
    Box,
    RadiusUse,
    Mover,
}

impl Class {
    fn of(classname: &str) -> Option<Self> {
        Some(match classname {
            "trigger_use" => Self::Use,
            "trigger_use_touch" => Self::UseTouch,
            "trigger_radius" => Self::Radius,
            "trigger_box" => Self::Box,
            "trigger_radius_use" => Self::RadiusUse,
            "script_model" | "script_brushmodel" | "script_origin" => Self::Mover,
            _ => return None,
        })
    }

    /// Stood in, not reached from the eye.
    fn touched(self) -> bool {
        matches!(self, Self::UseTouch | Self::Radius | Self::Box)
    }
}

/// The volume a candidate is tested against.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Standing on its origin.
    Cylinder { radius: f32, height: f32 },
    /// Centred on its origin, turned by its angles.
    Box { half: [f32; 3] },
    /// A brush or trigger model at its origin.
    Model {
        brush: Option<u32>,
        trigger: Option<u32>,
    },
    /// A point (a script mover without a volume).
    Point,
}

#[derive(Clone, Debug)]
struct Candidate {
    id: u64,
    class: Class,
    origin: [f32; 3],
    angles: [f32; 3],
    shape: Shape,
    cursor: i32,
    look_at: bool,
    abs: ([f32; 3], [f32; 3]),
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

/// The axes of `angles` (pitch, yaw, roll in degrees): forward, left, up.
fn axes(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let [p, y, r] = angles.map(f32::to_radians);
    let (sp, cp) = p.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sr, cr) = r.sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    [forward, right.map(|c| -c), up]
}

fn overlaps(a: ([f32; 3], [f32; 3]), b: ([f32; 3], [f32; 3])) -> bool {
    (0..3).all(|i| a.0[i] <= b.1[i] && b.0[i] <= a.1[i])
}

fn object_vector(runtime: &mut RoundScript, id: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// Every entity a use may reach: the use triggers, the touch triggers a use
/// works on, and script movers a script made usable.
fn candidates(world: &mut World) -> Vec<Candidate> {
    let bounds = world.get_resource::<EntityBounds>().map(|b| b.0.clone());
    let flags_look = world
        .get_resource::<EntityFlags>()
        .map_or_else(std::collections::BTreeSet::new, |f| {
            f.require_look_at.clone()
        });
    let frame = FrameWorld::from_world(world);
    let models: Vec<([f32; 3], [f32; 3])> = frame
        .clip_cmodels()
        .models
        .iter()
        .map(|m| (m.mins, m.maxs))
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    let ids: Vec<u64> = runtime
        .entities
        .keys()
        .copied()
        .filter(|id| runtime.live(id))
        .collect();
    let mut out = Vec::new();
    for id in ids {
        let entity = &runtime.entities[&id];
        let Some(class) = Class::of(&entity.classname) else {
            continue;
        };
        let usable = entity.usable.clone();
        if class == Class::Mover && !usable.as_ref().is_some_and(|u| u.enabled) {
            continue;
        }
        if usable.as_ref().is_some_and(|u| !u.enabled) {
            continue;
        }
        let (kind, cylinder, brush, trigger) = (
            entity.kind.clone(),
            entity.cylinder,
            entity.brush,
            entity.trigger_model,
        );
        let cursor = usable.map_or(
            if kind == EntityKind::Map && matches!(class, Class::Use | Class::UseTouch) {
                MAP_USE_CURSOR
            } else {
                0
            },
            |u| u.cursor,
        );
        let origin = object_vector(&mut runtime, id, "origin");
        let angles = object_vector(&mut runtime, id, "angles");
        let shape = if let Some((radius, height)) = cylinder {
            Shape::Cylinder { radius, height }
        } else if brush.is_some() || trigger.is_some() {
            Shape::Model { brush, trigger }
        } else if let Some((_, maxs)) = bounds.as_ref().and_then(|b| b.get(&id)) {
            Shape::Box { half: *maxs }
        } else {
            Shape::Point
        };
        let abs = match shape {
            Shape::Cylinder { radius, height } => (
                [origin[0] - radius, origin[1] - radius, origin[2]],
                [origin[0] + radius, origin[1] + radius, origin[2] + height],
            ),
            Shape::Box { half } => {
                let axis = axes(angles);
                let reach: [f32; 3] =
                    std::array::from_fn(|i| (0..3).map(|k| (axis[k][i] * half[k]).abs()).sum());
                (
                    sub(origin, reach),
                    std::array::from_fn(|i| origin[i] + reach[i]),
                )
            }
            Shape::Model { brush, .. } => brush.and_then(|n| models.get(n as usize)).map_or(
                (origin, origin),
                |(mins, maxs)| {
                    (
                        std::array::from_fn(|i| origin[i] + mins[i]),
                        std::array::from_fn(|i| origin[i] + maxs[i]),
                    )
                },
            ),
            Shape::Point => (origin, origin),
        };
        out.push(Candidate {
            id,
            class,
            origin,
            angles,
            shape,
            cursor,
            look_at: flags_look.contains(&id) || runtime.require_look_at.contains(&id),
            abs,
        });
    }
    out
}

/// The engine's contact test of the player's box with a candidate: a
/// cylinder by height and flat reach; a turned box by the line through the
/// middle of the player's box, its half width in from each end, against
/// the box grown by that half width; a model by its own volume.
fn touches(world: &mut World, candidate: &Candidate, mins: [f32; 3], maxs: [f32; 3]) -> bool {
    let mid: [f32; 3] = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5);
    match candidate.shape {
        Shape::Cylinder { radius, height } => {
            let o = candidate.origin;
            let reach = (maxs[0] - mid[0]) + radius;
            o[2] < maxs[2]
                && mins[2] < o[2] + height
                && (mid[0] - o[0]).powi(2) + (mid[1] - o[1]).powi(2) < reach * reach
        }
        Shape::Box { half } => {
            let grow = (maxs[0] - mins[0]) * 0.5;
            let low: [f32; 3] = std::array::from_fn(|i| mins[i] + grow);
            let high: [f32; 3] = std::array::from_fn(|i| maxs[i] - grow);
            let axis = axes(candidate.angles);
            let local = |p: [f32; 3]| {
                let d = sub(p, candidate.origin);
                [dot(d, axis[0]), dot(d, axis[1]), dot(d, axis[2])]
            };
            let (a, b) = (local(low), local(high));
            let box_lo = half.map(|h| -h - grow);
            let box_hi = half.map(|h| h + grow);
            let seg_lo: [f32; 3] = std::array::from_fn(|i| a[i].min(b[i]));
            let seg_hi: [f32; 3] = std::array::from_fn(|i| a[i].max(b[i]));
            if !(0..3).all(|i| seg_lo[i] <= box_hi[i] && box_lo[i] <= seg_hi[i]) {
                return false;
            }
            let d = sub(b, a);
            let extent: [f32; 3] = std::array::from_fn(|i| (box_hi[i] - box_lo[i]) * 0.5);
            let c: [f32; 3] = std::array::from_fn(|i| a[i] - (box_lo[i] + box_hi[i]) * 0.5);
            [(0, 1), (1, 2), (2, 0)].into_iter().all(|(i, j)| {
                d[j].abs() * extent[i] + d[i].abs() * extent[j] - (c[i] * d[j] - c[j] * d[i]).abs()
                    >= 0.0
            })
        }
        Shape::Model { brush, trigger } => super::super::super::triggers::map_volume_touches(
            &FrameWorld::from_world(world),
            candidate.origin,
            brush,
            trigger,
            mins,
            maxs,
        ),
        Shape::Point => false,
    }
}

/// What the search and pick read of a player.
struct Viewer {
    client: u32,
    eye: [f32; 3],
    forward: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
}

fn viewer(world: &mut World, client: u32) -> Option<Viewer> {
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(crate::ClientId(client))?;
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let forward = axes(ps.viewangles)[0];
    Some(Viewer {
        client,
        eye,
        forward,
        mins: [
            ps.origin[0] - PLAYER_HALF,
            ps.origin[1] - PLAYER_HALF,
            ps.origin[2],
        ],
        maxs: [
            ps.origin[0] + PLAYER_HALF,
            ps.origin[1] + PLAYER_HALF,
            ps.origin[2] + PLAYER_HEIGHT,
        ],
    })
}

fn use_radius(world: &World) -> f32 {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(USE_RADIUS_DVAR)
        .map_or(USE_RADIUS_ZM, |value| super::atof(value))
}

/// The search: each candidate near the player that the use reaches, with
/// its score, lowest first; those the eye cannot see come last.
fn search(world: &mut World, viewer: &Viewer, candidates: &[Candidate]) -> Vec<(f32, usize)> {
    let reach = THROWBACK_OUTER_RADIUS_ZM.max(SEARCH_MIN);
    let area = (
        [
            viewer.eye[0] - reach,
            viewer.eye[1] - reach,
            viewer.eye[2] - SEARCH_HEIGHT,
        ],
        [
            viewer.eye[0] + reach,
            viewer.eye[1] + reach,
            viewer.eye[2] + SEARCH_HEIGHT,
        ],
    );
    let radius = use_radius(world);
    let player = (viewer.mins, viewer.maxs);
    let mut found = Vec::new();
    for (index, c) in candidates.iter().enumerate() {
        if !overlaps(c.abs, area) {
            continue;
        }
        let center: [f32; 3] = std::array::from_fn(|i| (c.abs.0[i] + c.abs.1[i]) * 0.5);
        let to = sub(center, viewer.eye);
        let distance = length(to);
        let facing = if distance > 0.0 {
            dot(viewer.forward, to) / distance
        } else {
            dot(viewer.forward, to)
        };
        if c.class.touched() {
            if !overlaps(player, c.abs) {
                continue;
            }
            if matches!(c.class, Class::Radius | Class::Box)
                && (c.cursor < 1 || c.look_at && facing < LOOK_DOT)
            {
                continue;
            }
            if !touches(world, c, viewer.mins, viewer.maxs) {
                continue;
            }
            found.push((TOUCH_SCORE, index));
            continue;
        }
        if distance > radius {
            continue;
        }
        if matches!(c.class, Class::Use | Class::RadiusUse) && c.look_at && facing < LOOK_DOT {
            continue;
        }
        if c.class == Class::RadiusUse && !touches(world, c, viewer.mins, viewer.maxs) {
            continue;
        }
        let mut score = (1.0 - (facing + 1.0) * 0.5) * FACING_SCORE;
        score -= match c.class {
            Class::RadiusUse if c.cursor > 0 => CURSOR_BONUS,
            Class::Use => USE_BONUS,
            Class::Mover => MOVER_BONUS,
            _ => 0.0,
        };
        found.push((distance + score, index));
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let frame = FrameWorld::from_world(world);
    for (score, index) in &mut found {
        let c = &candidates[*index];
        if c.class.touched() {
            continue;
        }
        let center: [f32; 3] = std::array::from_fn(|i| (c.abs.0[i] + c.abs.1[i]) * 0.5);
        // A model's hit boxes stop bullets, not this sight line.
        let seen = matches!(
            frame.sensor_trace(crate::BulletTraceQuery {
                start: viewer.eye,
                end: center,
                mask: SIGHT_MASK,
                ignore: Some(crate::ClientId(viewer.client)),
                ignore_hit: None,
                ignore_model: None,
            }),
            crate::TraceOutcome::Miss { .. }
                | crate::TraceOutcome::Hit {
                    collider: crate::ColliderId::EntityDObjBone { .. },
                    ..
                }
        );
        if !seen {
            *score += UNSEEN_SCORE;
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}

/// The pick: the first candidate not hidden from the player, claimed by
/// another or kept to another team; it is used only with a cursor hint.
fn pick(
    world: &mut World,
    viewer: &Viewer,
    candidates: &[Candidate],
    found: &[(f32, usize)],
) -> Option<u64> {
    let player_team = super::super::super::players::player_object(world, viewer.client);
    let player_team = match player_team {
        Value::Object(object) => match world
            .resource_mut::<RoundScript>()
            .object_field(object, "team")
        {
            Value::String(team) => super::team_index(&team),
            _ => None,
        },
        _ => None,
    };
    let teams = world
        .get_resource::<EntityFlags>()
        .map_or_else(std::collections::BTreeMap::new, |f| f.trigger_team.clone());
    let runtime = world.resource::<RoundScript>();
    let bit = 1u64.checked_shl(viewer.client).unwrap_or(0);
    for &(_, index) in found {
        let c = &candidates[index];
        let entity = &runtime.entities[&c.id];
        if entity.hidden && entity.shown_to & bit == 0 {
            continue;
        }
        if entity
            .trigger_policy
            .claimed_by
            .is_some_and(|owner| owner != viewer.client)
        {
            continue;
        }
        if let Some(&team) = teams.get(&c.id)
            && team != 0
            && player_team != Some(team)
        {
            continue;
        }
        return (c.cursor != 0).then_some(c.id);
    }
    None
}

/// The entity a player's use reaches this frame, if any.
pub(super) fn usable_entity(world: &mut World, client: u32) -> Option<u64> {
    if super::laststand::is_down(world, client) {
        return None;
    }
    let viewer = viewer(world, client)?;
    let candidates = candidates(world);
    let found = search(world, &viewer, &candidates);
    pick(world, &viewer, &candidates, &found)
}

/// The use rule IW4L's trigger pass hands its use presses to: each
/// pressing player uses the entity their use reaches.
pub(super) fn use_presses(world: &mut World, pressed: &[(u32, u64)]) -> Vec<(u64, u64)> {
    pressed
        .iter()
        .filter_map(|&(client, player)| Some((usable_entity(world, client)?, player)))
        .collect()
}
