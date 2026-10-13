//! Black Ops 2's zombies barriers: a `zbarrier_<type>` map entity whose
//! pieces open and close by animation, as its barrier handlers and think do.

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
            .insert(id, Barrier { kind, pieces });
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
