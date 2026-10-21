//! Black Ops 2's zombies barriers: a `zbarrier_<type>` map entity whose
//! pieces open and close by animation, as its barrier handlers and think do.
//!
//! Clients draw each piece as its own entity at the barrier's place: the
//! model its flags pick (upgraded, else alternate, else default), hidden by
//! its flag, posed in its opening animation (its closing one under the close
//! flag) as far as the piece has got. A piece using the box's rise logic
//! carries a weapon model that, each time its opening animation passes a
//! `switch` note, turns into one of the client's box weapons at random (the
//! fourth piece shows that weapon's left-hand twin, or nothing); it goes
//! once the animation has played. IW4L draws the pieces as models the
//! authority places (`t6.zbarrier.client`).

use super::super::super::arrays::new_array;
use super::{float, int, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use asset_core::BarrierType;
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

const CLOSED: u8 = 0;
const OPENING: u8 = 1;
const OPEN: u8 = 2;
const CLOSING: u8 = 3;
const TARGETTED: u8 = 4;
/// Piece states by number, as script names them.
const STATES: [&str; 5] = [
    "closed",
    "opening",
    "open",
    "closing",
    "targetted_by_zombie",
];

/// Piece flags the engine sends to clients.
const ALTERNATE_MODEL: u8 = 1;
const UPGRADED_MODEL: u8 = 2;
/// The piece plays its closing animation (else its opening one).
const CLOSE_ANIM: u8 = 4;
const HIDDEN: u8 = 8;
const BOX_RISE: u8 = 0x10;

#[derive(Clone, Copy)]
struct Piece {
    state: u8,
    /// The animation rate, packed in a byte as the engine keeps it.
    rate: u8,
    /// The state an opening or closing piece ends in.
    target: u8,
    start: i32,
    flags: u8,
}

struct Barrier {
    kind: Arc<BarrierType>,
    pieces: [Piece; 6],
    /// How each piece is drawn.
    drawn: [Option<Drawn>; 6],
}

/// A piece's drawn model and the pose last given it, and the weapon models a
/// rising box piece carries.
struct Drawn {
    object: u64,
    shown: Option<(String, bool, Option<String>, u16)>,
    rise: Option<Rise>,
}

struct Rise {
    weapon: u64,
    /// The animation's progress last frame, 0 to 1.
    last: f32,
}

#[derive(Resource, Default)]
struct Barriers(BTreeMap<u64, Barrier>);

/// `(scale - 0.1) / 1.9` of 255, rounded as the x87 store does; only the low
/// byte is kept.
fn rate_byte(scale: f32) -> u8 {
    let x = ((scale - 0.1) * 0.526_315_8) * 255.0;
    let rounded = (f64::from(x) + 9.313_225_746_154_785e-10).round_ties_even();
    if rounded.is_finite() && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&rounded) {
        rounded as i32 as u8
    } else {
        0
    }
}

fn rate_of(byte: u8) -> f32 {
    f32::from(byte) * (1.0 / 255.0) * 1.9 + 0.1
}

fn now_ms(world: &World) -> i32 {
    super::super::super::players::now_ms(world) as i32
}

fn set_state(kind: &BarrierType, piece: &mut Piece, state: u8, scale: f32, now: i32) {
    piece.state = state;
    piece.rate = rate_byte(scale);
    piece.start = now;
    let auto_hide = kind.auto_hide_open_pieces != 0;
    match state {
        CLOSED | TARGETTED => piece.flags |= CLOSE_ANIM,
        OPENING => {
            piece.target = OPEN;
            piece.flags &= !CLOSE_ANIM;
        }
        OPEN => {
            piece.flags &= !CLOSE_ANIM;
            if auto_hide {
                piece.flags |= HIDDEN;
            }
        }
        CLOSING => {
            piece.target = CLOSED;
            piece.flags |= CLOSE_ANIM;
            if auto_hide {
                piece.flags &= !HIDDEN;
            }
        }
        _ => {}
    }
}

/// The animation length in ms, as the engine's think reads it.
fn anim_ms(kind: &BarrierType, index: usize, close: bool) -> Result<i32, String> {
    let piece = &kind.pieces[index];
    let anim = if close {
        &piece.close_anim
    } else {
        &piece.open_anim
    };
    let (frames, framerate) = anim.as_ref().and_then(|anim| anim.timing).ok_or_else(|| {
        format!(
            "piece {index} of {} has no animation length in its zone (t6.zbarrier.anim)",
            kind.name
        )
    })?;
    Ok(((f32::from(frames) / framerate) * 1000.0) as i32)
}

/// Runs the barrier's think (every server frame, after scripts) up to `now`:
/// an opening or closing piece ends once its animation has played.
fn advance(barrier: &mut Barrier, now: i32) -> Result<(), String> {
    let tick = crate::MATCH_TICK_MS as i32;
    for index in 0..barrier
        .pieces
        .len()
        .min(barrier.kind.piece_count.max(0) as usize)
    {
        let piece = barrier.pieces[index];
        if piece.state != OPENING && piece.state != CLOSING {
            continue;
        }
        let length = anim_ms(&barrier.kind, index, piece.flags & CLOSE_ANIM != 0)?;
        let duration = (length as f32 * rate_of(piece.rate)) as i32;
        let done = match duration {
            ..0 => continue,
            0 => piece.start + tick,
            _ => piece.start + (duration + tick - 1) / tick * tick,
        };
        if now > done {
            set_state(
                &barrier.kind,
                &mut barrier.pieces[index],
                piece.target,
                1.0,
                done,
            );
        }
    }
    Ok(())
}

/// The barrier an entity is, its think caught up.
fn barrier<'w>(world: &'w mut World, receiver: &Value) -> Result<&'w mut Barrier, String> {
    let id = super::super::engine::entity_id(world, receiver)
        .map_err(|_| String::from("not an entity"))?;
    let now = now_ms(world);
    if !world
        .get_resource::<Barriers>()
        .is_some_and(|known| known.0.contains_key(&id))
    {
        let runtime = world.resource::<RoundScript>();
        let kind = runtime
            .entities
            .get(&id)
            .and_then(|entity| entity.classname.strip_prefix("zbarrier_"))
            .and_then(|name| {
                runtime
                    .restart
                    .as_ref()?
                    .barriers
                    .get(&name.to_ascii_lowercase())
                    .cloned()
            })
            .ok_or_else(|| format!("entity {id} is not a zbarrier"))?;
        let kind = with_loaded_timings(world, kind);
        // At level start every piece is closed, at full rate.
        let pieces = [Piece {
            state: CLOSED,
            rate: rate_byte(1.0),
            target: CLOSED,
            start: now,
            flags: CLOSE_ANIM,
        }; 6];
        world
            .get_resource_or_insert_with(Barriers::default)
            .0
            .insert(
                id,
                Barrier {
                    kind,
                    pieces,
                    drawn: Default::default(),
                },
            );
    }
    let barrier = world
        .resource_mut::<Barriers>()
        .into_inner()
        .0
        .get_mut(&id)
        .expect("barrier just made");
    advance(barrier, now)?;
    Ok(barrier)
}

/// A type whose piece animations its own zone lacks takes their lengths from
/// the zones that carry them, as the engine resolves animations across the
/// zones loaded.
fn with_loaded_timings(world: &mut World, kind: Arc<BarrierType>) -> Arc<BarrierType> {
    let missing = kind.pieces.iter().any(|piece| {
        [&piece.open_anim, &piece.close_anim]
            .into_iter()
            .flatten()
            .any(|anim| anim.timing.is_none())
    });
    if !missing {
        return kind;
    }
    let frame = crate::frame::FrameWorld::from_world(world);
    let mut owned = (*kind).clone();
    for piece in &mut owned.pieces {
        for anim in [&mut piece.open_anim, &mut piece.close_anim]
            .into_iter()
            .flatten()
        {
            if anim.timing.is_none()
                && let Some(clip) = frame.anim_clip_named(&anim.name)
            {
                anim.timing = Some((clip.numframes, clip.framerate));
            }
        }
    }
    Arc::new(owned)
}

/// The model a piece draws as: the upgraded one when its flag is set and it
/// has one, else the alternate one likewise, else its default.
fn piece_model(piece: &asset_core::BarrierPiece, flags: u8) -> Option<&str> {
    let [default, alternate, upgraded] = &piece.models;
    if flags & UPGRADED_MODEL != 0
        && let Some(model) = upgraded
    {
        return Some(model);
    }
    if flags & ALTERNATE_MODEL != 0
        && let Some(model) = alternate
    {
        return Some(model);
    }
    default.as_deref()
}

/// How far through its animation a piece is (0 to 1): an opening or closing
/// piece by its time and rate, any other at the end of its last one.
fn progress(kind: &BarrierType, index: usize, piece: &Piece, now: i32) -> f32 {
    if piece.state != OPENING && piece.state != CLOSING {
        return 1.0;
    }
    let Ok(length) = anim_ms(kind, index, piece.flags & CLOSE_ANIM != 0) else {
        return 1.0;
    };
    let duration = length as f32 * rate_of(piece.rate);
    if duration <= 0.0 {
        return 1.0;
    }
    ((now - piece.start) as f32 / duration).clamp(0.0, 1.0)
}

/// The class drawn pieces take, so scripts looking for their own script
/// models do not meet them.
const PIECE_CLASSNAME: &str = "zbarrier_piece";
/// The model a rising box weapon starts as, and the tag of the rising piece
/// it sits on.
const RISE_MODEL: &str = "tag_origin";
const RISE_TAG: &str = "tag_origin";
/// The rising piece that cycles the weapon; the one after it holds the
/// weapon's left-hand twin.
const RISE_WEAPON_PIECE: usize = 3;
/// The note at which the rising weapon changes.
const RISE_SWITCH_NOTE: &str = "switch";

fn spawn_piece(world: &mut World, origin: [f32; 3]) -> Option<u64> {
    let spawned = super::super::engine::spawn_entity(
        world,
        &[Value::string("script_model"), Value::Vector(origin)],
    );
    let Ok(Value::Object(object)) = spawned else {
        return None;
    };
    if let Some(entity) = world
        .resource_mut::<RoundScript>()
        .entities
        .get_mut(&object)
    {
        entity.classname = PIECE_CLASSNAME.into();
    }
    Some(object)
}

/// Poses a drawn piece in one clip at `time` (0 to 1).
fn pose_clip(world: &mut World, object: u64, clip: Arc<xmodel_runtime::AnimClip>, time: f32) {
    use xmodel_runtime::{
        XAnimNodeState, XAnimSemanticNode, XAnimSemanticNodeKind, XAnimTreeSnapshot,
    };
    let Some(presence) = world
        .resource::<RoundScript>()
        .entities
        .get(&object)
        .and_then(|entity| entity.presence)
    else {
        return;
    };
    let tree = XAnimTreeSnapshot {
        definition_revision: 1,
        state_revision: 0,
        nodes: vec![XAnimSemanticNode {
            parent: None,
            kind: XAnimSemanticNodeKind::Leaf,
            clip: Some(clip.name.clone()),
            parts: None,
            state: XAnimNodeState {
                time,
                old_time: time,
                goal_weight: 1.0,
                weight: 1.0,
                rate: 1.0,
                ..XAnimNodeState::default()
            },
        }],
    };
    let resolved = tree
        .resolve(|name| (name == clip.name).then(|| Arc::clone(&clip)))
        .ok();
    if let Some(dobj) = crate::frame::FrameWorld::from_world(world)
        .collision_owner_mut(presence)
        .and_then(|row| row.dobj.as_mut())
    {
        dobj.set_tree_pose(tree, resolved);
        dobj.materialize();
    }
}

fn set_model(world: &mut World, object: u64, model: &str, hidden: bool) {
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(object, "model", Value::string(model));
    if let Some(entity) = runtime.entities.get_mut(&object) {
        entity.hidden = hidden;
    }
}

fn place(world: &mut World, object: u64, origin: [f32; 3], angles: [f32; 3]) {
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "angles", Value::Vector(angles));
}

/// Shows a weapon on a drawn model: its model and hide tags.
fn show_weapon(world: &mut World, object: u64, weapon: &str, model: &str) {
    set_model(world, object, model, false);
    super::weapon::hide_weapon_tags(world, object, weapon);
}

fn delete(world: &mut World, object: u64) {
    world.resource_mut::<RoundScript>().delete_entity(object);
}

/// Draws every barrier's pieces as its clients would.
pub(crate) fn draw(world: &mut World) {
    let candidates: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("zbarrier_"))
        .filter(|(_, entity)| &*entity.classname != PIECE_CLASSNAME)
        .map(|(id, _)| *id)
        .collect();
    if candidates.is_empty() {
        return;
    }
    let now = now_ms(world);
    let box_weapons = super::client::entity::zombie_box_weapons(world);
    for id in candidates {
        if barrier(world, &Value::Object(id)).is_err() {
            continue;
        }
        let (origin, angles) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let vector = |value: Value| match value {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            (
                vector(runtime.object_field(id, "origin")),
                vector(runtime.object_field(id, "angles")),
            )
        };
        let (kind, pieces) = {
            let barrier = &world.resource::<Barriers>().0[&id];
            (Arc::clone(&barrier.kind), barrier.pieces)
        };
        let count = (kind.piece_count.clamp(0, 6) as usize).min(kind.pieces.len());
        for index in 0..count {
            draw_piece(
                world,
                id,
                &kind,
                index,
                pieces[index],
                origin,
                angles,
                now,
                &box_weapons,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_piece(
    world: &mut World,
    id: u64,
    kind: &BarrierType,
    index: usize,
    piece: Piece,
    origin: [f32; 3],
    angles: [f32; 3],
    now: i32,
    box_weapons: &[(String, String, bool)],
) {
    let def = &kind.pieces[index];
    let Some(model) = piece_model(def, piece.flags) else {
        return;
    };
    let mut drawn = world
        .resource_mut::<Barriers>()
        .0
        .get_mut(&id)
        .and_then(|barrier| barrier.drawn[index].take());
    if drawn.as_ref().is_some_and(|d| {
        !world
            .resource::<RoundScript>()
            .entities
            .contains_key(&d.object)
    }) {
        drawn = None;
    }
    let mut drawn = match drawn {
        Some(drawn) => drawn,
        None => {
            let Some(object) = spawn_piece(world, origin) else {
                return;
            };
            diag::debug!(
                Sim,
                "t6zm: zbarrier {id} ({}) piece {index} drawn as {model}",
                kind.name
            );
            Drawn {
                object,
                shown: None,
                rise: None,
            }
        }
    };
    let close = piece.flags & CLOSE_ANIM != 0;
    let anim = if close {
        &def.close_anim
    } else {
        &def.open_anim
    };
    let progress = progress(kind, index, &piece, now);
    let time_key = (progress * 1000.0) as u16;
    let hidden = piece.flags & HIDDEN != 0;
    let wanted = (
        model.to_owned(),
        hidden,
        anim.as_ref().map(|anim| anim.name.clone()),
        time_key,
    );
    if drawn.shown.as_ref() != Some(&wanted) {
        if drawn.shown.as_ref().map(|shown| (&shown.1, &shown.2)) != Some((&wanted.1, &wanted.2)) {
            diag::debug!(
                Sim,
                "t6zm: zbarrier {id} piece {index} state={} hidden={hidden} anim={:?} at {progress:.2}",
                piece.state,
                wanted.2
            );
        }
        place(world, drawn.object, origin, angles);
        set_model(world, drawn.object, model, hidden);
        let clip = anim.as_ref().and_then(|anim| {
            crate::frame::FrameWorld::from_world(world).anim_clip_named(&anim.name)
        });
        if let Some(clip) = clip {
            pose_clip(world, drawn.object, clip, progress);
        }
        drawn.shown = Some(wanted);
    }
    // The rising box weapon: while the piece opens under the rise logic.
    let rising = piece.flags & BOX_RISE != 0 && !close && piece.state == OPENING;
    if rising && index >= RISE_WEAPON_PIECE {
        let clip = def.open_anim.as_ref().and_then(|anim| {
            crate::frame::FrameWorld::from_world(world).anim_clip_named(&anim.name)
        });
        let rise = match drawn.rise.take() {
            Some(rise) => Some(rise),
            None => spawn_piece(world, origin).map(|weapon| {
                set_model(world, weapon, RISE_MODEL, false);
                Rise { weapon, last: 0.0 }
            }),
        };
        if let Some(mut rise) = rise {
            // The weapon sits where the piece's animation carries its origin.
            let (at, turned) =
                super::super::super::presence::tag_world(world, drawn.object, RISE_TAG)
                    .map_or((origin, angles), |(at, axis)| {
                        (at, math_iw4::axis_to_angles(axis))
                    });
            place(world, rise.weapon, at, turned);
            let switched = clip.as_ref().is_some_and(|clip| {
                clip.notifies.iter().any(|note| {
                    note.name.eq_ignore_ascii_case(RISE_SWITCH_NOTE)
                        && note.time > rise.last
                        && note.time <= progress
                })
            });
            if switched && index == RISE_WEAPON_PIECE && !box_weapons.is_empty() {
                let pick = rise_pick(world, id) % box_weapons.len();
                let (weapon, model, dual_wield) = &box_weapons[pick];
                show_weapon(world, rise.weapon, weapon, model);
                diag::debug!(Sim, "t6zm: zbarrier {id} box weapon {weapon}");
                let twin = world
                    .resource_mut::<Barriers>()
                    .0
                    .get_mut(&id)
                    .and_then(|barrier| barrier.drawn[index + 1].as_ref())
                    .and_then(|d| d.rise.as_ref().map(|r| r.weapon));
                if let Some(twin) = twin {
                    let left = dual_wield
                        .then(|| super::weapon::zone_weapon(world, weapon))
                        .flatten()
                        .and_then(|facts| {
                            super::weapon::zone_weapon(world, &facts.dual_wield_weapon)
                        })
                        .and_then(|facts| facts.world_models.first().cloned())
                        .filter(|model| !model.is_empty());
                    match left {
                        Some(left) => show_weapon(world, twin, weapon, &left),
                        None => set_model(world, twin, RISE_MODEL, false),
                    }
                }
            }
            rise.last = progress;
            drawn.rise = Some(rise);
        }
    } else if let Some(rise) = drawn.rise.take() {
        delete(world, rise.weapon);
    }
    if let Some(barrier) = world.resource_mut::<Barriers>().0.get_mut(&id) {
        barrier.drawn[index] = Some(drawn);
    }
}

/// The client's random pick of a box weapon (`rand() % 32`, then by the
/// list's size).
fn rise_pick(world: &mut World, id: u64) -> usize {
    let seed = world
        .get_resource_or_insert_with(RiseSeed::default)
        .into_inner();
    seed.0 = seed.0.wrapping_mul(1_103_515_245).wrapping_add(12_345) ^ id as u32;
    ((seed.0 >> 16) % 32) as usize
}

#[derive(Resource, Default)]
struct RiseSeed(u32);

fn piece_index(barrier: &Barrier, args: &[Value]) -> Result<usize, String> {
    let index = int(args, 0)?;
    let count = barrier.kind.piece_count;
    if index < 0 || index >= count {
        return Err(format!(
            "This ZBarrier type has pieces in the range 0 to {} - {index} is out of range.\n",
            count - 1
        ));
    }
    Ok(index as usize)
}

fn state_named(args: &[Value], index: usize) -> Result<u8, String> {
    let name = string(args, index)?.to_ascii_lowercase();
    STATES
        .iter()
        .position(|state| *state == name)
        .map(|state| state as u8)
        .ok_or_else(|| "Unknown state for ZBarrier piece.".into())
}

fn change_flags(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    change: fn(u8) -> u8,
) -> Result<Value, String> {
    let barrier = barrier(world, receiver)?;
    let index = piece_index(barrier, args)?;
    let piece = &mut barrier.pieces[index];
    piece.flags = change(piece.flags);
    Ok(Value::Undefined)
}

/// Whether every piece is in one of `states`.
fn all_in(world: &mut World, receiver: &Value, states: &[u8]) -> Result<Value, String> {
    let barrier = barrier(world, receiver)?;
    let count = barrier.kind.piece_count.max(0) as usize;
    let all = barrier.pieces[..count.min(6)]
        .iter()
        .all(|piece| states.contains(&piece.state));
    Ok(Value::Int(all.into()))
}

/// The collision model each zbarrier was given.
#[derive(bevy_ecs::prelude::Resource, Default)]
struct CollisionModels(std::collections::BTreeMap<u64, String>);

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getzbarrierarray", |world, _, _| {
        let candidates: Vec<u64> = world
            .resource::<RoundScript>()
            .entities
            .iter()
            .filter(|(_, entity)| entity.classname.starts_with("zbarrier_"))
            .map(|(id, _)| *id)
            .collect();
        let mut listed = Vec::new();
        for id in candidates {
            if barrier(world, &Value::Object(id)).is_ok() {
                listed.push(Value::Object(id));
            }
        }
        new_array(world, listed)
    });
    // gscm_setzbarriercolmodel: the zbarrier's own model, its collision
    // (`t6.barriers.colmodel`: kept, not collided against).
    registry.register(Method, "setzbarriercolmodel", |world, receiver, args| {
        barrier(world, receiver)?;
        let Some(Value::String(model)) = args.first() else {
            return Err("setzbarriercolmodel expects a model".into());
        };
        let id = super::super::engine::entity_id(world, receiver)?;
        world
            .get_resource_or_insert_with(CollisionModels::default)
            .0
            .insert(id, model.to_string());
        Ok(Value::Undefined)
    });
    registry.register(Method, "iszbarrier", |world, receiver, _| {
        Ok(Value::Int(barrier(world, receiver).is_ok().into()))
    });
    registry.register(Method, "getnumzbarrierpieces", |world, receiver, _| {
        Ok(Value::Int(barrier(world, receiver)?.kind.piece_count))
    });
    registry.register(Method, "setzbarrierpiecestate", |world, receiver, args| {
        let now = now_ms(world);
        let barrier = barrier(world, receiver)?;
        let index = piece_index(barrier, args)?;
        let state = state_named(args, 1)?;
        let scale = if args.len() == 3 {
            float(args, 2)?
        } else {
            1.0
        };
        let kind = barrier.kind.clone();
        set_state(&kind, &mut barrier.pieces[index], state, scale, now);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getzbarrierpiecestate", |world, receiver, args| {
        let barrier = barrier(world, receiver)?;
        let index = piece_index(barrier, args)?;
        let state = barrier.pieces[index].state;
        Ok(Value::String(
            STATES.get(usize::from(state)).copied().unwrap_or("").into(),
        ))
    });
    registry.register(
        Method,
        "getzbarrierpieceindicesinstate",
        |world, receiver, args| {
            let barrier = barrier(world, receiver)?;
            let state = state_named(args, 0)?;
            let count = barrier.kind.piece_count.max(0) as usize;
            let indices = barrier.pieces[..count.min(6)]
                .iter()
                .enumerate()
                .filter(|(_, piece)| piece.state == state)
                .map(|(index, _)| Value::Int(index as i32))
                .collect();
            new_array(world, indices)
        },
    );
    registry.register(Method, "iszbarrieropen", |world, receiver, _| {
        all_in(world, receiver, &[OPEN])
    });
    registry.register(Method, "iszbarrierclosed", |world, receiver, _| {
        all_in(world, receiver, &[CLOSED, TARGETTED])
    });
    registry.register(Method, "hidezbarrierpiece", |world, receiver, args| {
        change_flags(world, receiver, args, |flags| flags | HIDDEN)
    });
    registry.register(Method, "showzbarrierpiece", |world, receiver, args| {
        change_flags(world, receiver, args, |flags| flags & !HIDDEN)
    });
    registry.register(
        Method,
        "zbarrierpieceuseboxriselogic",
        |world, receiver, args| change_flags(world, receiver, args, |flags| flags | BOX_RISE),
    );
    registry.register(
        Method,
        "zbarrierpieceusedefaultmodel",
        |world, receiver, args| {
            change_flags(world, receiver, args, |flags| {
                flags & !(ALTERNATE_MODEL | UPGRADED_MODEL)
            })
        },
    );
    registry.register(
        Method,
        "zbarrierpieceusealternatemodel",
        |world, receiver, args| {
            change_flags(world, receiver, args, |flags| {
                flags & !UPGRADED_MODEL | ALTERNATE_MODEL
            })
        },
    );
    registry.register(
        Method,
        "zbarrierpieceuseupgradedmodel",
        |world, receiver, args| {
            change_flags(world, receiver, args, |flags| {
                flags & !ALTERNATE_MODEL | UPGRADED_MODEL
            })
        },
    );
    // Open and opening play the opening animation, closed and closing the
    // closing one; the length is scaled by the rate, held to 0.1 – 2.
    registry.register(
        Method,
        "getzbarrierpieceanimlengthforstate",
        |world, receiver, args| {
            let barrier = barrier(world, receiver)?;
            let index = piece_index(barrier, args)?;
            let close = match state_named(args, 1)? {
                OPEN | OPENING => false,
                CLOSED | CLOSING => true,
                _ => return Err("Unknown state for ZBarrier piece.".into()),
            };
            let scale = if args.len() == 3 {
                let scale = float(args, 2)?;
                if scale - 2.0 >= 0.0 {
                    2.0
                } else if 0.1 - scale >= 0.0 {
                    0.1
                } else {
                    scale
                }
            } else {
                1.0
            };
            let piece = &barrier.kind.pieces[index];
            let anim = if close {
                &piece.close_anim
            } else {
                &piece.open_anim
            };
            let (frames, framerate) = anim.as_ref().and_then(|anim| anim.timing).ok_or_else(|| {
            format!(
                "piece {index} of {} has no animation length in its zone (t6.zbarrier.anim)",
                barrier.kind.name
            )
        })?;
            Ok(Value::Float(f32::from(frames) / framerate * scale))
        },
    );
    registry.register(
        Method,
        "getzbarrierpieceanimstate",
        |world, receiver, args| {
            let barrier = barrier(world, receiver)?;
            let index = piece_index(barrier, args)?;
            Ok(Value::String(
                barrier.kind.pieces[index].anim_state.as_str().into(),
            ))
        },
    );
    registry.register(
        Method,
        "getzbarrierpieceanimsubstate",
        |world, receiver, args| {
            let barrier = barrier(world, receiver)?;
            let index = piece_index(barrier, args)?;
            Ok(Value::String(
                barrier.kind.pieces[index].anim_substate.as_str().into(),
            ))
        },
    );
    registry.register(
        Method,
        "getupgradedpiecenumlives",
        |world, receiver, args| {
            let barrier = barrier(world, receiver)?;
            let index = piece_index(barrier, args)?;
            Ok(Value::Int(barrier.kind.pieces[index].upgraded_lives as i32))
        },
    );
    registry.register(Method, "getzbarriernumattackslots", |world, receiver, _| {
        Ok(Value::Int(barrier(world, receiver)?.kind.attack_slots))
    });
    registry.register(
        Method,
        "getzbarrierattackslothorzoffset",
        |world, receiver, _| {
            Ok(Value::Float(
                barrier(world, receiver)?.kind.attack_slot_horz_offset,
            ))
        },
    );
    registry.register(
        Method,
        "zbarriersupportszombietaunts",
        |world, receiver, _| {
            Ok(Value::Int(
                (barrier(world, receiver)?.kind.taunts == 1).into(),
            ))
        },
    );
    registry.register(
        Method,
        "zbarriersupportszombiereachthroughattacks",
        |world, receiver, _| {
            Ok(Value::Int(
                (barrier(world, receiver)?.kind.reach_through_attacks == 1).into(),
            ))
        },
    );
    registry.register(Method, "getzbarriertauntanimstate", |world, receiver, _| {
        Ok(Value::String(
            barrier(world, receiver)?
                .kind
                .taunt_anim_state
                .as_str()
                .into(),
        ))
    });
    registry.register(
        Method,
        "getzbarrierreachthroughattackanimstate",
        |world, receiver, _| {
            let barrier = barrier(world, receiver)?;
            Ok(Value::String(
                barrier.kind.reach_through_anim_state.as_str().into(),
            ))
        },
    );
}
