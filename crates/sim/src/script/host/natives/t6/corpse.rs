//! Black Ops 2's actor corpses. Once a dead actor's death animscript has
//! ended, the engine's actor think turns it into a corpse: a corpse entity
//! takes the actor's model, pose, origin and angles, the actor is told
//! `death` and freed, and its actor slot is free again. Corpses fill
//! `ai_corpseCount` slots (read as the level starts); a new corpse takes
//! the first free slot, or, with none free, frees the corpse the first
//! player is least likely to see: the farthest behind him, else the
//! farthest at least 45° off his view, else the farthest (distances on the
//! level plane from his eye).

use super::super::super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{MatchScript, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// `ai_corpseCount` as the engine registers it, read as the level starts.
const CORPSE_COUNT_DVAR: &str = "ai_corpseCount";
const CORPSE_COUNT_DEFAULT: usize = 8;
const CORPSE_COUNT_MAX: usize = 8;
/// The contents the engine gives a corpse.
const CORPSE_CONTENTS: i32 = 0x0400_4000;
/// A corpse's classname: what the engine's entity init leaves.
const CORPSE_CLASSNAME: &str = "noclass";

/// The corpse in each slot.
#[derive(Resource)]
struct CorpseSlots(Vec<Option<u64>>);

fn slots(world: &mut World) -> &mut Vec<Option<u64>> {
    if !world.contains_resource::<CorpseSlots>() {
        let count = world
            .resource::<MatchScript>()
            .dvars
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(CORPSE_COUNT_DVAR))
            .map_or(CORPSE_COUNT_DEFAULT, |(_, value)| {
                usize::try_from(super::atoi(value)).unwrap_or(0)
            })
            .min(CORPSE_COUNT_MAX);
        world.insert_resource(CorpseSlots(vec![None; count]));
    }
    &mut world.resource_mut::<CorpseSlots>().into_inner().0
}

/// The first player's eye and his view's direction on the level plane
/// (the whole direction when he looks straight up or down); without a
/// player, the origin and +y.
fn viewer(world: &mut World) -> ([f32; 3], [f32; 3], bool) {
    let first = {
        let runtime = world.resource::<RoundScript>();
        runtime
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::Player)
            .min_by_key(|(_, e)| e.number)
            .and_then(|(id, _)| runtime.player_client(*id))
    };
    let frame = FrameWorld::from_world(world);
    let Some((eye, angles)) = first.and_then(|client| {
        let id = crate::ClientId(client);
        Some((
            crate::script_player::eye(&frame, id)?,
            frame.player(id)?.viewangles,
        ))
    }) else {
        return ([0.0; 3], [0.0, 1.0, 0.0], true);
    };
    let (forward, _, _) = math_iw4::angle_vectors(angles);
    let flat = (forward[0] * forward[0] + forward[1] * forward[1]).sqrt();
    if flat == 0.0 {
        return (eye, forward, false);
    }
    let scale = 1.0 / flat;
    (eye, [forward[0] * scale, forward[1] * scale, 0.0], true)
}

/// The slot a new corpse takes: the first free one, else the slot of the
/// corpse freed for it; none when there are no slots.
fn take_slot(world: &mut World) -> Option<usize> {
    if let Some(free) = slots(world).iter().position(Option::is_none) {
        return Some(free);
    }
    let (eye, forward, level) = viewer(world);
    let held: Vec<(usize, u64)> = slots(world)
        .iter()
        .enumerate()
        .filter_map(|(slot, corpse)| Some((slot, (*corpse)?)))
        .collect();
    let (mut behind, mut aside, mut far) = ((-1.0f32, 0), (-1.0f32, 0), (-1.0f32, 0));
    for &(slot, corpse) in &held {
        let at = match world
            .resource_mut::<RoundScript>()
            .object_field(corpse, "origin")
        {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let d = [
            at[0] - eye[0],
            at[1] - eye[1],
            if level { 0.0 } else { at[2] - eye[2] },
        ];
        let along = d[1] * forward[1] + d[0] * forward[0] + d[2] * forward[2];
        let dist_sq = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
        if along < 0.0 && behind.0 < dist_sq {
            behind = (dist_sq, slot);
        }
        if along * along <= dist_sq * 0.5 && aside.0 < dist_sq {
            aside = (dist_sq, slot);
        }
        if far.0 < dist_sq {
            far = (dist_sq, slot);
        }
    }
    if held.is_empty() {
        return None;
    }
    let slot = if behind.0 != -1.0 {
        behind.1
    } else if aside.0 != -1.0 {
        aside.1
    } else {
        far.1
    };
    if let Some(corpse) = slots(world)[slot].take() {
        world.resource_mut::<RoundScript>().delete_entity(corpse);
    }
    Some(slot)
}

/// Turns a dead actor whose death animscript has ended into a corpse and
/// frees the actor.
pub(super) fn finish_death(world: &mut World, actor: u64) -> Result<(), String> {
    let slot = take_slot(world);
    if let Some(slot) = slot {
        let corpse = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let fields: Vec<(&str, Value)> = ["origin", "angles", "model"]
                .into_iter()
                .map(|name| (name, runtime.object_field(actor, name)))
                .collect();
            let corpse = runtime.create_entity(EntityKind::ActorCorpse, CORPSE_CLASSNAME)?;
            for (name, value) in fields {
                runtime.set_object_field(corpse, name, value);
            }
            // The corpse takes the actor's body as drawn: its model and
            // pose (t6.actors.corpse_entity: the engine spawns it its own
            // entity number from eight it keeps for corpses).
            let (presence, number) = runtime
                .entities
                .get_mut(&actor)
                .map(|e| (e.presence.take(), e.number))
                .ok_or("actor is gone")?;
            if let Some(anim) = runtime.actor_anims.remove(&actor) {
                runtime.actor_anims.insert(corpse, anim);
            }
            let entity = runtime.entities.get_mut(&corpse).ok_or("corpse is gone")?;
            entity.presence = presence;
            entity.number = number;
            entity.contents = CORPSE_CONTENTS;
            corpse
        };
        slots(world)[slot] = Some(corpse);
    }
    raise(world, Value::Object(actor), "death", Vec::new());
    world.resource_mut::<RoundScript>().delete_entity(actor);
    Ok(())
}

/// A corpse freed (by script, or for a new one) leaves its slot, as the
/// engine's entity free does.
pub(super) fn forget_deleted(world: &mut World) {
    let Some(held) = world.get_resource::<CorpseSlots>().map(|s| s.0.clone()) else {
        return;
    };
    let gone: Vec<usize> = {
        let runtime = world.resource::<RoundScript>();
        held.iter()
            .enumerate()
            .filter(|(_, corpse)| corpse.is_some_and(|id| !runtime.entities.contains_key(&id)))
            .map(|(slot, _)| slot)
            .collect()
    };
    let mut slots = world.resource_mut::<CorpseSlots>();
    for slot in gone {
        slots.0[slot] = None;
    }
}
