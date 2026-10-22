//! Black Ops' use rule: which entity a player's use reaches, as its engine
//! decides it each frame (the search of the use triggers near the player,
//! ranked by distance and facing, then the pick of the first one the
//! player may use, whose hint shows), and fires it when the player presses
//! use. Its other usables (weapons on the ground, missiles, turrets and
//! actors) are not run; the zombie maps' scripts make none.

use crate::frame::FrameWorld;
use crate::script::{NativeRegistry, RoundScript};
use bevy_ecs::prelude::World;

/// The player's box: 15 each way, 70 high, whatever the stance.
const PLAYER_HALF: f32 = 15.0;
const PLAYER_HEIGHT: f32 = 70.0;
/// The search reaches at least this far each way flat (or
/// `player_throwbackOuterRadius` when larger), and this far up and down,
/// from the eye.
const SEARCH_MIN: f32 = 384.0;
const SEARCH_HEIGHT: f32 = 96.0;
/// `player_throwbackOuterRadius`'s default.
const THROWBACK_OUTER_RADIUS: f32 = 192.0;
/// A use trigger answers from this far from the eye to its middle.
const USE_REACH: f32 = 72.0;
/// A use trigger that asks to be looked at wants its middle within this
/// dot of the view.
const LOOK_DOT: f32 = 0.76;
/// A trigger the player stands in comes before every other.
const TOUCH_SCORE: f32 = -144.0;
/// How the facing adds to a candidate's distance: 0 looking at it, this
/// much looking away; a `trigger_use` takes it off again.
const FACING_SCORE: f32 = 512.0;
/// The sight line from the eye to a candidate's middle: solid and glass.
const SIGHT_MASK: u32 = 0x11;
/// How far past a world surface the entity sight line goes on, and how
/// many surfaces it steps through.
const SIGHT_STEP_PAST: f32 = 0.125;
const MAX_SIGHT_STEPS: usize = 32;
/// Weapon states the pick waits out (the offhand throw) and the deploy.
const OFFHAND_STATES: std::ops::RangeInclusive<i32> = 0x15..=0x19;
const DEPLOYING: i32 = 0x24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Use,
    UseTouch,
    Radius,
    RadiusUse,
}

impl Class {
    fn of(classname: &str) -> Option<Self> {
        Some(match classname {
            "trigger_use" => Self::Use,
            "trigger_use_touch" => Self::UseTouch,
            "trigger_radius" => Self::Radius,
            "trigger_radius_use" => Self::RadiusUse,
            _ => return None,
        })
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.on_use(use_presses);
}

fn use_presses(world: &mut World, pressed: &[(u32, u64)]) -> Vec<(u64, u64)> {
    pressed
        .iter()
        .filter_map(|&(client, player)| Some((usable_entity(world, client)?, player)))
        .collect()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn overlaps(a: ([f32; 3], [f32; 3]), b: ([f32; 3], [f32; 3])) -> bool {
    (0..3).all(|i| a.0[i] <= b.1[i] && b.0[i] <= a.1[i])
}

fn forward(angles: [f32; 3]) -> [f32; 3] {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    [cp * cy, cp * sy, -sp]
}

/// The entity a player's use reaches this frame, if any.
fn usable_entity(world: &mut World, client: u32) -> Option<u64> {
    let frame = FrameWorld::from_world(world);
    let id = crate::ClientId(client);
    let ps = frame.player(id)?;
    // Downed, a player uses no trigger.
    if ps.move_player(client as i32, false).move_type == game_api::movement::MoveType::LastStand {
        return None;
    }
    if let Some(status) = crate::game_move::status(&frame, id)
        && (status.mantling
            || status.sprinting
            || status.diving
            || OFFHAND_STATES.contains(&status.weapon_state)
            || status.weapon_state == DEPLOYING)
    {
        return None;
    }
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let view = forward(ps.viewangles);
    let player = (
        [
            ps.origin[0] - PLAYER_HALF,
            ps.origin[1] - PLAYER_HALF,
            ps.origin[2],
        ],
        [
            ps.origin[0] + PLAYER_HALF,
            ps.origin[1] + PLAYER_HALF,
            ps.origin[2] + PLAYER_HEIGHT,
        ],
    );
    let found = search(world, client, eye, view, player);
    pick(world, client, &found)
}

/// The search: each trigger near the player that the use reaches, by its
/// score, lowest first; one the eye cannot see is left out.
fn search(
    world: &mut World,
    client: u32,
    eye: [f32; 3],
    view: [f32; 3],
    player: ([f32; 3], [f32; 3]),
) -> Vec<(f32, u64, [f32; 3], Class)> {
    let reach = THROWBACK_OUTER_RADIUS.max(SEARCH_MIN);
    let area = (
        [eye[0] - reach, eye[1] - reach, eye[2] - SEARCH_HEIGHT],
        [eye[0] + reach, eye[1] + reach, eye[2] + SEARCH_HEIGHT],
    );
    let triggers: Vec<(u64, Class, i32, bool, bool)> = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(id, _)| runtime.live(id))
            .filter_map(|(&id, entity)| {
                let class = Class::of(&entity.classname)?;
                let usable = entity.usable.as_ref();
                if usable.is_some_and(|u| !u.enabled || u.barred.contains(&client)) {
                    return None;
                }
                let cursor = usable.map(|u| u.cursor);
                Some((
                    id,
                    class,
                    cursor.unwrap_or(0),
                    cursor.is_none(),
                    runtime.require_look_at.contains(&id),
                ))
            })
            .collect()
    };
    let mut found = Vec::new();
    for (id, class, cursor, default_cursor, look_at) in triggers {
        let cursor = if default_cursor {
            super::super::physics::map_use_cursor(world, id).unwrap_or(cursor)
        } else {
            cursor
        };
        let Some((abs, touched)) =
            super::super::triggers::trigger_reach(world, id, player.0, player.1)
        else {
            continue;
        };
        if !overlaps(abs, area) {
            continue;
        }
        let center: [f32; 3] = std::array::from_fn(|i| (abs.0[i] + abs.1[i]) * 0.5);
        match class {
            Class::UseTouch | Class::Radius => {
                if class == Class::Radius && cursor <= 0 {
                    continue;
                }
                if overlaps(player, abs) && touched {
                    found.push((TOUCH_SCORE, id, center, class));
                }
            }
            Class::Use | Class::RadiusUse => {
                let to = sub(center, eye);
                let distance = dot(to, to).sqrt();
                if distance > USE_REACH {
                    continue;
                }
                let facing = if distance > 0.0 {
                    dot(view, to) / distance
                } else {
                    dot(view, to)
                };
                if look_at && facing < LOOK_DOT {
                    continue;
                }
                let mut score = (1.0 - (facing + 1.0) * 0.5) * FACING_SCORE;
                if class == Class::Use {
                    score -= FACING_SCORE;
                }
                found.push((score + distance, id, center, class));
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let frame = FrameWorld::from_world(world);
    found.retain(|&(_, _, center, class)| {
        matches!(class, Class::UseTouch | Class::Radius)
            || entities_clear(&frame, client, eye, center)
    });
    found
}

/// Black Ops' sight line from the eye to a candidate's middle: only the
/// entities in it count (the player's own and the candidate's left out),
/// not the world, so a switch set into a wall is seen. A world hit is
/// stepped past; a brush model blocks.
fn entities_clear(frame: &FrameWorld, client: u32, eye: [f32; 3], center: [f32; 3]) -> bool {
    let dir = sub(center, eye);
    let length = dot(dir, dir).sqrt();
    if length <= 0.0 {
        return true;
    }
    let step = dir.map(|c| c / length);
    let mut start = eye;
    for _ in 0..MAX_SIGHT_STEPS {
        match frame.sensor_trace(crate::BulletTraceQuery {
            start,
            end: center,
            mask: SIGHT_MASK,
            ignore: Some(crate::ClientId(client)),
            ignore_hit: None,
            ignore_model: None,
        }) {
            crate::TraceOutcome::Miss { .. } => return true,
            crate::TraceOutcome::Hit {
                collider: crate::ColliderId::EntityLinkedBrush { .. },
                ..
            } => return false,
            crate::TraceOutcome::StartSolid {
                collider: Some(crate::ColliderId::EntityLinkedBrush { .. }),
                ..
            } => return false,
            // Inside the world to the end: no entity is in the way.
            crate::TraceOutcome::StartSolid {
                all_solid: true, ..
            }
            | crate::TraceOutcome::Invalid { .. } => return true,
            crate::TraceOutcome::Hit { end, .. } | crate::TraceOutcome::StartSolid { end, .. } => {
                let past: [f32; 3] = std::array::from_fn(|i| end[i] + step[i] * SIGHT_STEP_PAST);
                start = if dot(sub(past, start), step) > 0.0 {
                    past
                } else {
                    std::array::from_fn(|i| start[i] + step[i] * SIGHT_STEP_PAST)
                };
                if dot(sub(center, start), step) <= 0.0 {
                    return true;
                }
            }
        }
    }
    true
}

/// The pick: the first trigger the player may use (its team, its claim);
/// it is used only with a cursor hint.
fn pick(world: &mut World, client: u32, found: &[(f32, u64, [f32; 3], Class)]) -> Option<u64> {
    for &(_, id, _, _) in found {
        if !super::super::triggers::use_eligible(world, id, client) {
            continue;
        }
        let cursor = {
            let runtime = world.resource::<RoundScript>();
            runtime.entities.get(&id)?.usable.as_ref().map(|u| u.cursor)
        };
        let cursor = cursor.or_else(|| super::super::physics::map_use_cursor(world, id))?;
        return (cursor != 0).then_some(id);
    }
    None
}
