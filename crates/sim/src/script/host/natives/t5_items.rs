//! Black Ops multiplayer's weapons on the ground, as BlackOpsMP.exe keeps
//! them: `dropitem` and `dropscavengeritem`, the two ammo slots an item
//! carries, its fall and rest, the farthest-from-everyone cleanup, and picking
//! it up (ammo by touch, the weapon itself by use, swapping for the one in
//! hand).

use super::super::args::{int, string};
use crate::ClientId;
use crate::frame::FrameWorld;
use crate::item::{
    TR_GRAVITY, TR_LINEAR_STOP, TR_STATIONARY, Trace, Trajectory, evaluate_trajectory,
};
use crate::script::Namespace::Method;
use crate::script::host::registry::ItemRule;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use math_iw4::angle_vectors;
use std::collections::BTreeMap;

/// `g_maxDroppedWeapons`' default.
const MAX_DROPPED_WEAPONS: usize = 16;
/// An item's box.
const ITEM_MINS: [f32; 3] = [-1.0; 3];
const ITEM_MAXS: [f32; 3] = [1.0; 3];
/// What an item falls onto when it has no clip mask of its own.
const ITEM_MASK: u32 = 0x491;
/// An item keeps its owner this long; until then the owner cannot take it.
const OWNER_MS: i32 = 1000;
/// A frame of an item's fall.
const FRAME_MS: i32 = 50;
/// A step this short (squared) drops a unit, to find the ground.
const STILL_DISTANCE_SQ: f32 = 0.1;
/// A blocked move this early is a landing.
const LANDED_FRACTION: f32 = 0.01;
/// A surface this steep is slid along.
const FLOOR_NORMAL_Z: f32 = 0.5;
/// A swapped weapon is laid down from this far over the taken one.
const SWAP_LIFT: f32 = 2.0;
/// The contents an item may not rest in (`nodrop`).
const CONTENTS_NO_DROP: u32 = 0x8000_0000;
/// The reach of an item's touch from the player's origin.
const TOUCH_XY: f32 = 36.0;
const TOUCH_BELOW: f32 = -88.0;
const TOUCH_ABOVE: f32 = 18.0;
/// A player's touch search reaches this far beyond their box.
const TOUCH_MARGIN: f32 = 20.0;
const SCAVENGER_PERK: &str = "specialty_scavenger";

/// The fields of Black Ops' weapon definition this file reads.
const DEF_INVENTORY_TYPE: usize = 0x2c;
const DEF_OFFHAND_CLASS: usize = 0x68;
const DEF_MAX_AMMO: usize = 0x344;
const DEF_SHARED_AMMO_CAP: usize = 0x354;
const DEF_CLIP_RELATIVE: usize = 0x359;
const DEF_AVOID_DROP_CLEANUP: usize = 0x47e;
const DEF_CARRIED_KILLSTREAK: usize = 0x552;
const DEF_CLIP_ONLY: usize = 0x561;
const VARIANT_CLIP_SIZE: usize = 0x20;
const INVENTORY_PRIMARY: i32 = 0;
const INVENTORY_ALT: i32 = 3;
const OFFHAND_FRAG: i32 = 1;

static SHARED_AMMO_CAP: game_api::Unknown = game_api::Unknown {
    id: "t5.mp.shared_ammo_cap",
    what: "a weapon with a shared ammo cap on the ground: its cap is not applied",
    needs: "BlackOpsMP.exe's shared ammo cap table",
};

/// One of an item's two ammo slots: its weapon and its alternate, or its
/// left hand's weapon.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Slot {
    weapon: u32,
    stock: i32,
    clip: i32,
}

/// What an item keeps beyond the row every game's items have.
#[derive(Clone, Copy, Debug, Default)]
struct GroundWeapon {
    slots: [Slot; 2],
    owner: Option<u32>,
    owner_until_ms: i32,
    /// Resting on something.
    grounded: bool,
    /// The flag `dropscavengeritem` sets.
    scavenger: bool,
}

#[derive(Resource, Default)]
struct GroundWeapons(BTreeMap<i32, GroundWeapon>);

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "dropitem", drop_item);
    registry.register(Method, "dropscavengeritem", drop_scavenger_item);
    registry.register(Method, "itemweaponsetammo", item_weapon_set_ammo);
    registry.on_items(ItemRule {
        run: run_item,
        touch,
        select,
        grab,
    });
}

fn ground_weapon(world: &World, number: i32) -> Option<GroundWeapon> {
    world
        .get_resource::<GroundWeapons>()?
        .0
        .get(&number)
        .copied()
}

fn def_i32(world: &FrameWorld, weapon: u32, at: usize) -> i32 {
    world
        .content()
        .game_weapon(weapon)
        .and_then(|game| game.def.get(at..at + 4))
        .map_or(0, |bytes| {
            i32::from_le_bytes(bytes.try_into().expect("four bytes"))
        })
}

fn def_flag(world: &FrameWorld, weapon: u32, at: usize) -> bool {
    world
        .content()
        .game_weapon(weapon)
        .and_then(|game| game.def.get(at))
        .is_some_and(|byte| *byte != 0)
}

fn clip_size(world: &FrameWorld, weapon: u32) -> i32 {
    world
        .content()
        .game_weapon(weapon)
        .and_then(|game| game.variant.get(VARIANT_CLIP_SIZE..VARIANT_CLIP_SIZE + 4))
        .map_or(0, |bytes| {
            i32::from_le_bytes(bytes.try_into().expect("four bytes"))
        })
}

fn alt_of(world: &FrameWorld, weapon: u32) -> u32 {
    world
        .content()
        .game_weapon(weapon)
        .map_or(0, |game| game.alt_weapon)
}

fn ammo_index(world: &FrameWorld, weapon: u32) -> i32 {
    world
        .content()
        .game_weapon(weapon)
        .map_or(0, |game| game.ammo_index)
}

fn held(world: &FrameWorld, id: ClientId) -> Vec<u32> {
    world.player(id).map_or_else(Vec::new, |ps| {
        ps.weapons
            .iter()
            .filter(|&&weapon| weapon > 0)
            .map(|&weapon| weapon as u32)
            .collect()
    })
}

/// A weapon's most rounds in reserve, with its clip when it counts in clips.
fn weapon_max_ammo(world: &FrameWorld, weapon: u32) -> i32 {
    let max = def_i32(world, weapon, DEF_MAX_AMMO);
    if def_flag(world, weapon, DEF_CLIP_RELATIVE) {
        max * clip_size(world, weapon).max(1)
    } else {
        max
    }
}

/// The most a player holds of a weapon's ammo: a clip-only weapon's clip, else
/// the sum for every held weapon on that ammo but `except`.
fn player_max_ammo(world: &mut FrameWorld, id: ClientId, weapon: u32, except: u32) -> i32 {
    if def_i32(world, weapon, DEF_SHARED_AMMO_CAP) > 0 {
        world.report_game_gap(&SHARED_AMMO_CAP);
    }
    if def_flag(world, weapon, DEF_CLIP_ONLY) {
        return clip_size(world, weapon);
    }
    let ammo = ammo_index(world, weapon);
    held(world, id)
        .into_iter()
        .filter(|&other| other != except && ammo_index(world, other) == ammo)
        .filter(|&other| {
            def_i32(world, other, DEF_INVENTORY_TYPE) != INVENTORY_ALT
                || alt_of(world, other) != other
        })
        .map(|other| weapon_max_ammo(world, other))
        .sum()
}

/// What a player can still take of a weapon's ammo.
fn ammo_room(world: &mut FrameWorld, id: ClientId, weapon: u32) -> i32 {
    if def_flag(world, weapon, DEF_CLIP_ONLY) {
        return clip_size(world, weapon) - crate::script_player::ammo_clip(world, id, weapon);
    }
    player_max_ammo(world, id, weapon, 0) - crate::script_player::ammo_stock(world, id, weapon)
}

fn has_room_for(world: &mut FrameWorld, id: ClientId, weapon: u32) -> bool {
    let ammo = ammo_index(world, weapon);
    for other in held(world, id) {
        if ammo_index(world, other) == ammo && ammo_room(world, id, other) > 0 {
            return true;
        }
    }
    false
}

fn holds_same_ammo(world: &FrameWorld, id: ClientId, weapon: u32) -> bool {
    let ammo = ammo_index(world, weapon);
    held(world, id)
        .into_iter()
        .any(|other| ammo_index(world, other) == ammo)
}

/// The rounds of a weapon's reserve its drop takes: what the other weapons on
/// its ammo could not hold.
fn stock_to_drop(world: &mut FrameWorld, id: ClientId, weapon: u32) -> i32 {
    let others = player_max_ammo(world, id, weapon, weapon);
    (crate::script_player::ammo_stock(world, id, weapon) - others).max(0)
}

/// The next weapon of an item's ammo slots: the left hand's, else the
/// alternate.
fn next_slot_weapon(world: &FrameWorld, weapon: u32) -> u32 {
    match crate::game_move::left_weapon(world, weapon) {
        0 => alt_of(world, weapon),
        left => left,
    }
}

fn take_with_alts(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    let left = crate::game_move::left_weapon(world, weapon);
    if left != 0 {
        crate::script_player::set_ammo_clip(world, id, left, 0);
    }
    crate::script_player::take_weapon(world, id, weapon);
    let mut alt = alt_of(world, weapon);
    while alt != 0 && crate::script_player::has_weapon(world, id, alt) {
        crate::script_player::take_weapon(world, id, alt);
        alt = alt_of(world, alt);
    }
}

fn item_object(world: &World, number: i32) -> Option<u64> {
    world
        .resource::<RoundScript>()
        .entities
        .iter()
        .find_map(|(id, entity)| {
            matches!(entity.kind, super::super::entities::EntityKind::Item(at) if at == number)
                .then_some(*id)
        })
}

/// The player's box: its height by stance.
fn player_box(world: &FrameWorld, id: ClientId) -> f32 {
    crate::game_move::status(world, id).map_or(70.0, |status| status.hull_height as f32)
}

fn trace_item(world: &FrameWorld, start: [f32; 3], end: [f32; 3]) -> Trace {
    world.trace_world_hull_except(start, end, ITEM_MINS, ITEM_MAXS, ITEM_MASK, &|_| false)
}

fn lerp(from: [f32; 3], to: [f32; 3], fraction: f32) -> [f32; 3] {
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * fraction)
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// The cue's rule when it is full: frees the weapon farthest from every
/// player alive, leaving those that avoid the cleanup and the one in `keep`.
fn free_a_slot(world: &mut FrameWorld, keep: Option<i32>) {
    let numbers = world.dropped_item_numbers_sorted();
    if numbers.len() < MAX_DROPPED_WEAPONS {
        return;
    }
    let players: Vec<[f32; 3]> = world
        .client_ids_sorted()
        .into_iter()
        .filter_map(|id| {
            world
                .player(id)
                .filter(|ps| ps.health > 0)
                .map(|ps| ps.origin)
        })
        .collect();
    let mut best: Option<(f32, i32)> = None;
    for &number in &numbers {
        let Some(item) = world.dropped_item_by_number(number) else {
            continue;
        };
        if Some(number) == keep || def_flag(world, item.state.index as u32, DEF_AVOID_DROP_CLEANUP)
        {
            continue;
        }
        let nearest = players
            .iter()
            .map(|at| distance_sq(item.origin, *at))
            .fold(f32::MAX, f32::min);
        if best.is_none_or(|(far, _)| far < nearest) {
            best = Some((nearest, number));
        }
    }
    if best.is_none() {
        diag::warn!(
            Sim,
            "Could not find a suitable weapon entity to free out of {} possible.  Using index zero.",
            numbers.len()
        );
    }
    if let Some(number) = best.map(|(_, number)| number).or(numbers.first().copied()) {
        world.despawn_dropped_item(number);
    }
}

/// Makes an item flying from a player: thrown up from the middle of their
/// height, off their yaw, by the four drop dvars.
fn launch(world: &mut FrameWorld, id: ClientId, weapon: u32, keep: Option<i32>) -> Option<i32> {
    let ps = *world.player(id)?;
    let height = player_box(world, id);
    let origin = [ps.origin[0], ps.origin[1], ps.origin[2] + height * 0.5];
    let mut seed = world.anim_event_seed();
    let velocity = crate::item::drop_item_velocity(ps.viewangles[1], &mut seed);
    world.set_anim_event_seed(seed);
    let now = world.entity_kernel().level_time_ms();
    free_a_slot(world, keep);
    let pos = Trajectory {
        tr_type: TR_GRAVITY,
        tr_time: now,
        tr_duration: 0,
        tr_delta: velocity,
        tr_base: origin,
    };
    let apos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: 0,
        tr_duration: 0,
        tr_delta: [0.0; 3],
        tr_base: [0.0, ps.viewangles[1], 0.0],
    };
    crate::item::spawn_flying_item(world, weapon, origin, pos, apos, id.0 as i32)
}

/// Black Ops' weapon drop: an alternate mode drops its base weapon; a weapon
/// not held or with no ammo is taken with nothing dropped; the item takes the
/// rounds and goes from the player's middle to the tag.
fn drop_weapon(
    world: &mut World,
    id: ClientId,
    weapon: u32,
    tag: Option<&str>,
    keep: Option<i32>,
) -> Option<i32> {
    let mut frame = FrameWorld::from_world(world);
    let weapon = if def_i32(&frame, weapon, DEF_INVENTORY_TYPE) == INVENTORY_ALT {
        let base = alt_of(&frame, weapon);
        if base == 0 {
            diag::warn!(
                Sim,
                "Drop_Weapon(): Trying to drop alt-type weapon, \"{}\", but it has no corresponding altWeapon set.",
                frame.weapon_script_name(weapon)
            );
            return None;
        }
        base
    } else {
        weapon
    };
    if !crate::script_player::has_weapon(&frame, id, weapon) {
        return None;
    }
    let clip = crate::script_player::ammo_clip(&frame, id, weapon);
    let stock = stock_to_drop(&mut frame, id, weapon);
    if (clip <= 0 && stock <= 0) || (def_flag(&frame, weapon, DEF_CLIP_ONLY) && clip == 0) {
        take_with_alts(&mut frame, id, weapon);
        return None;
    }
    let number = launch(&mut frame, id, weapon, keep)?;
    let mut slots = [Slot::default(); 2];
    let mut slot_weapon = weapon;
    for slot in &mut slots {
        if slot_weapon == 0 {
            break;
        }
        *slot = Slot {
            weapon: slot_weapon,
            clip: crate::script_player::ammo_clip(&frame, id, slot_weapon),
            stock: stock_to_drop(&mut frame, id, slot_weapon),
        };
        slot_weapon = next_slot_weapon(&frame, slot_weapon);
    }
    take_with_alts(&mut frame, id, weapon);
    let now = frame.entity_kernel().level_time_ms();
    if let Some(row) = frame.dropped_item_mut_by_number(number) {
        row.clip_r = slots[0].clip;
        row.clip_l = slots[1].clip;
        row.stock = slots[0].stock;
    }
    let ps = *frame.player(id)?;
    let height = player_box(&frame, id);
    world.resource_mut::<GroundWeapons>().0.insert(
        number,
        GroundWeapon {
            slots,
            owner: Some(id.0),
            owner_until_ms: now + OWNER_MS,
            grounded: false,
            scavenger: false,
        },
    );
    let Some(tag) = tag else {
        return Some(number);
    };
    let object = super::super::players::player_object(world, id.0);
    let offset = match object {
        Value::Object(object) => super::super::presence::tag_offset(world, object, tag),
        _ => None,
    };
    // The player's model as it stands unanimated: the server keeps no
    // animated pose of it to read the tag from.
    let axis = math_iw4::angles_to_axis([0.0, ps.viewangles[1], 0.0]);
    let tag_at = offset.map(|offset| -> [f32; 3] {
        std::array::from_fn(|i| {
            ps.origin[i] + axis[0][i] * offset[0] + axis[1][i] * offset[1] + axis[2][i] * offset[2]
        })
    });
    let mut frame = FrameWorld::from_world(world);
    if let Some(end) = tag_at {
        let start = [ps.origin[0], ps.origin[1], ps.origin[2] + height * 0.5];
        let hit = trace_item(&frame, start, end);
        let at = lerp(start, end, hit.fraction);
        if let Some(row) = frame.dropped_item_mut_by_number(number) {
            row.origin = at;
            row.state.tr_base = at;
            row.state.tr_time = now;
        }
    }
    if let Some(row) = frame.dropped_item_mut_by_number(number) {
        row.state.apos_tr_base = [0.0, ps.viewangles[1], 0.0];
    }
    Some(number)
}

/// `player dropitem(weapon [, tag])`; the weapon's tag defaults to
/// `tag_weapon_right`.
fn drop_item(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = ClientId(
        world
            .resource::<RoundScript>()
            .player_client_of(receiver)
            .ok_or("not an entity")?,
    );
    let Ok(weapon) = super::player::player_weapon(world, id, args, 0) else {
        return Ok(Value::Undefined);
    };
    let tag = match args.get(1) {
        Some(_) => string(args, 1)?,
        None => "tag_weapon_right".into(),
    };
    world.get_resource_or_insert_with(GroundWeapons::default);
    let Some(number) = drop_weapon(world, id, weapon, Some(&tag), None) else {
        return Ok(Value::Undefined);
    };
    let class = item_class(world, number);
    super::player::new_item_entity(world, number, &class)
}

/// `player dropscavengeritem(weapon)`: an item flying from the player with the
/// scavenger flag and no ammo slots.
fn drop_scavenger_item(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = ClientId(
        world
            .resource::<RoundScript>()
            .player_client_of(receiver)
            .ok_or("not an entity")?,
    );
    let Ok(weapon) = super::player::player_weapon(world, id, args, 0) else {
        return Ok(Value::Undefined);
    };
    let mut frame = FrameWorld::from_world(world);
    let Some(number) = launch(&mut frame, id, weapon, None) else {
        return Ok(Value::Undefined);
    };
    let now = frame.entity_kernel().level_time_ms();
    world
        .get_resource_or_insert_with(GroundWeapons::default)
        .0
        .insert(
            number,
            GroundWeapon {
                slots: [Slot::default(); 2],
                owner: Some(id.0),
                owner_until_ms: now + OWNER_MS,
                grounded: false,
                scavenger: true,
            },
        );
    super::player::new_item_entity(world, number, super::player::SCAVENGER_ITEM_CLASS)
}

/// The classname of an item: `weapon_<name>`, which the scripts read back.
pub(crate) fn item_class(world: &mut World, number: i32) -> String {
    let frame = FrameWorld::from_world(world);
    let weapon = frame
        .dropped_item_by_number(number)
        .map_or(0, |item| item.state.index as u32);
    format!("weapon_{}", frame.weapon_script_name(weapon))
}

/// `item itemweaponsetammo(clip, stock [, slot])`: slot 0 or 1, the clip cut to
/// the slot weapon's clip size.
fn item_weapon_set_ammo(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let Value::Object(object) = receiver else {
        return Err("not an entity".into());
    };
    let number = match world
        .resource::<RoundScript>()
        .entities
        .get(object)
        .map(|entity| &entity.kind)
    {
        Some(super::super::entities::EntityKind::Item(number)) => *number,
        _ => return Err("Entity is not an item.".into()),
    };
    if ground_weapon(world, number).is_none_or(|item| item.scavenger) {
        return Err("Item entity is not a weapon.".into());
    }
    let clip = int(args, 0)?;
    if clip < 0 {
        return Err("Ammo count must not be negative".into());
    }
    let stock = int(args, 1)?;
    if stock < 0 {
        return Err("Ammo count must not be negative".into());
    }
    let hand = match args.get(2) {
        Some(_) => {
            let hand = int(args, 2)?;
            if !(0..=1).contains(&hand) {
                return Err("Value out of range.  Allowed values: 0 to 2".into());
            }
            hand as usize
        }
        None => 0,
    };
    let weapon = ground_weapon(world, number).map_or(0, |item| item.slots[hand].weapon);
    if weapon == 0 {
        return Ok(Value::Undefined);
    }
    let size = clip_size(&FrameWorld::from_world(world), weapon);
    if let Some(item) = world.resource_mut::<GroundWeapons>().0.get_mut(&number) {
        item.slots[hand].stock = stock;
        item.slots[hand].clip = clip.min(size);
    }
    let slots = ground_weapon(world, number).map(|item| item.slots);
    if let (Some(slots), Some(row)) = (
        slots,
        FrameWorld::from_world(world).dropped_item_mut_by_number(number),
    ) {
        row.clip_r = slots[0].clip;
        row.clip_l = slots[1].clip;
        row.stock = slots[0].stock;
    }
    Ok(Value::Undefined)
}

/// An item's frame: falls by its trajectory, a blocked move stopping at what
/// it met over the next 50 ms; lands on a surface that faces up unless the
/// surface's contents refuse it (a wall or ceiling it stays against frees it).
fn run_item(world: &mut World, time_ms: i32, number: i32) {
    let Some(mut ground) = ground_weapon(world, number) else {
        return;
    };
    if ground.owner.is_some() && ground.owner_until_ms <= time_ms {
        ground.owner = None;
    }
    let mut frame = FrameWorld::from_world(world);
    let Some(item) = frame.dropped_item_by_number(number) else {
        return;
    };
    let mut state = item.state;
    let mut origin = item.origin;
    if !ground.grounded && state.tr_type == TR_STATIONARY {
        state.tr_type = TR_GRAVITY;
        state.tr_time = time_ms;
        state.tr_base = origin;
        state.tr_delta = [0.0; 3];
    }
    if state.tr_type == TR_STATIONARY {
        world
            .resource_mut::<GroundWeapons>()
            .0
            .insert(number, ground);
        return;
    }
    let trajectory = Trajectory {
        tr_type: state.tr_type,
        tr_time: state.tr_time,
        tr_duration: state.tr_duration,
        tr_delta: state.tr_delta,
        tr_base: state.tr_base,
    };
    let mut target = evaluate_trajectory(&trajectory, time_ms + FRAME_MS);
    if distance_sq(target, origin) < STILL_DISTANCE_SQ {
        target[2] -= 1.0;
    }
    let mut hit = trace_item(&frame, origin, target);
    let mut stopped = origin;
    if hit.fraction >= 1.0 {
        origin = target;
    } else {
        stopped = lerp(origin, target, hit.fraction);
        if hit.startsolid == 0 && hit.fraction < LANDED_FRACTION && hit.normal[2] < FLOOR_NORMAL_Z {
            let into: f32 = (0..3)
                .map(|i| (target[i] - origin[i]) * hit.normal[i])
                .sum();
            let push = 1.0 - into;
            let off: [f32; 3] = std::array::from_fn(|i| target[i] + hit.normal[i] * push);
            hit = trace_item(&frame, stopped, off);
            stopped = lerp(stopped, off, hit.fraction);
        }
        state.tr_type = TR_LINEAR_STOP;
        state.tr_time = time_ms;
        state.tr_duration = FRAME_MS;
        state.tr_base = origin;
        state.tr_delta = std::array::from_fn(|i| (stopped[i] - origin[i]) * 20.0);
        origin = stopped;
    }
    let mut freed = false;
    if hit.fraction < LANDED_FRACTION {
        let nodrop = crate::game_fire::point_contents(&frame, origin, CONTENTS_NO_DROP) != 0;
        if hit.normal[2] > 0.0 && !nodrop {
            state.tr_type = TR_STATIONARY;
            state.tr_base = stopped;
            state.tr_time = 0;
            state.tr_duration = 0;
            state.tr_delta = [0.0; 3];
            state.apos_tr_base = rest_angles(state.apos_tr_base, hit.normal);
            origin = stopped;
            ground.grounded = true;
        } else {
            freed = true;
        }
    }
    if freed {
        frame.despawn_dropped_item(number);
        world.resource_mut::<GroundWeapons>().0.remove(&number);
        return;
    }
    if let Some(row) = frame.dropped_item_mut_by_number(number) {
        row.state = state;
        row.origin = origin;
        row.falling = state.tr_type == TR_GRAVITY;
    }
    world
        .resource_mut::<GroundWeapons>()
        .0
        .insert(number, ground);
}

/// A landed weapon's angles: its facing laid into the surface, on its side.
fn rest_angles(angles: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let (forward, _, _) = angle_vectors(angles);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let flat = cross(cross(normal, forward), normal);
    let yaw = flat[1].atan2(flat[0]).to_degrees();
    let pitch = (-flat[2])
        .atan2((flat[0] * flat[0] + flat[1] * flat[1]).sqrt())
        .to_degrees();
    [pitch, yaw, 90.0]
}

/// May this player take this item: not with weapons disabled, not within its
/// owner's second, by its slot weapon or its alternate.
fn may_grab(world: &mut FrameWorld, id: ClientId, item: &GroundWeapon, touched: bool) -> bool {
    if crate::game_move::status(world, id).is_some_and(|status| status.weapons_disabled) {
        return false;
    }
    if item.owner == Some(id.0) {
        return false;
    }
    let weapon = item.slots[0].weapon;
    if grabbable(world, id, weapon, touched, item.scavenger) {
        return true;
    }
    let alt = alt_of(world, weapon);
    alt != 0 && grabbable(world, id, alt, touched, item.scavenger)
}

fn grabbable(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    touched: bool,
    scavenger: bool,
) -> bool {
    if !touched {
        return !crate::script_player::has_weapon(world, id, weapon);
    }
    let room = weapon != 0 && has_room_for(world, id, weapon);
    if def_i32(world, weapon, DEF_OFFHAND_CLASS) == OFFHAND_FRAG && room {
        return true;
    }
    if (crate::script_player::has_weapon(world, id, weapon) || holds_same_ammo(world, id, weapon))
        && room
    {
        return true;
    }
    scavenger
}

/// Add_Ammo: rounds into the reserve, cut to what the player may hold; a
/// clip-only weapon takes them in its clip, and is given if missing. Answers
/// the rounds gained.
fn add_ammo(world: &mut FrameWorld, id: ClientId, weapon: u32, count: i32, fill_clip: bool) -> i32 {
    if !crate::script_player::has_weapon(world, id, weapon)
        && def_i32(world, weapon, DEF_OFFHAND_CLASS) != OFFHAND_FRAG
    {
        return 0;
    }
    let clip_before = crate::script_player::ammo_clip(world, id, weapon);
    let stock_before = crate::script_player::ammo_stock(world, id, weapon);
    let max = player_max_ammo(world, id, weapon, 0);
    let clip_only = def_flag(world, weapon, DEF_CLIP_ONLY);
    let mut stock = (stock_before + count).max(0);
    let mut clip = clip_before;
    if clip_only && !crate::script_player::has_weapon(world, id, weapon) {
        let _ = crate::script_player::give_weapon(world, id, weapon, false);
        clip = 0;
    }
    if clip_only || fill_clip {
        let size = clip_size(world, weapon);
        let moved = (size - clip).min(stock).max(0);
        stock -= moved;
        clip += moved;
    }
    if clip_only {
        stock = 0;
    } else if stock > max {
        stock = max;
    }
    clip = clip.min(clip_size(world, weapon));
    crate::script_player::set_ammo_clip(world, id, weapon, clip);
    crate::script_player::set_ammo_stock(world, id, weapon, stock);
    clip + stock - clip_before - stock_before
}

/// A player's touch of the items near them, as Black Ops' trigger search
/// walks them.
fn touch(world: &mut World, client: u32) {
    let id = ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    let Some(ps) = frame.player(id).copied() else {
        return;
    };
    if ps.health < 1 {
        return;
    }
    let height = player_box(&frame, id);
    let reach = (height + TOUCH_MARGIN + 1.0).min(-TOUCH_BELOW);
    for number in frame.dropped_item_numbers_sorted() {
        let Some(item) = frame.dropped_item_by_number(number) else {
            continue;
        };
        let delta: [f32; 3] = std::array::from_fn(|i| ps.origin[i] - item.origin[i]);
        if delta[0].abs() > TOUCH_XY
            || delta[1].abs() > TOUCH_XY
            || delta[2] > TOUCH_ABOVE
            || delta[2] < -reach
        {
            continue;
        }
        touched(world, id, number);
        frame = FrameWorld::from_world(world);
    }
}

/// The player's touch of an item: a scavenger pack, or the ammo it has room
/// for. Taking any leaves nothing of the item.
fn touched(world: &mut World, id: ClientId, number: i32) {
    let Some(item) = ground_weapon(world, number) else {
        return;
    };
    let mut frame = FrameWorld::from_world(world);
    if !may_grab(&mut frame, id, &item, true) || !may_touch(&frame, id, &item) {
        return;
    }
    let weapon = item.slots[0].weapon;
    if item.scavenger {
        if !super::t5_perks::has_perk(world, id.0, SCAVENGER_PERK) {
            return;
        }
        let mut frame = FrameWorld::from_world(world);
        record_pickup(&mut frame, id, weapon, number, crate::item::ENTITY_NONE);
        frame.despawn_dropped_item(number);
        world.resource_mut::<GroundWeapons>().0.remove(&number);
        return;
    }
    if !crate::script_player::has_weapon(&frame, id, weapon)
        && !holds_same_ammo(&frame, id, weapon)
        && def_i32(&frame, weapon, DEF_OFFHAND_CLASS) != OFFHAND_FRAG
    {
        return;
    }
    let mut took = false;
    for slot in item.slots.iter().filter(|slot| slot.weapon != 0) {
        took |= add_ammo(&mut frame, id, slot.weapon, slot.stock + slot.clip, false) != 0;
    }
    if !took {
        return;
    }
    record_pickup(&mut frame, id, weapon, number, crate::item::ENTITY_NONE);
    crate::item::push_pickup_event(&mut frame, id, weapon, true);
    frame.despawn_dropped_item(number);
    world.resource_mut::<GroundWeapons>().0.remove(&number);
}

/// Black Ops' gate before any pickup: not in last stand, not holding a carried
/// killstreak weapon.
fn may_touch(frame: &FrameWorld, id: ClientId, _item: &GroundWeapon) -> bool {
    let Some(ps) = frame.player(id) else {
        return false;
    };
    let downed =
        ps.move_player(id.0 as i32, false).move_type == game_api::movement::MoveType::LastStand;
    !downed && !def_flag(frame, ps.weapon, DEF_CARRIED_KILLSTREAK)
}

fn record_pickup(frame: &mut FrameWorld, id: ClientId, weapon: u32, from: i32, swapped: i32) {
    let picker_pm_type = frame.player(id).map_or(0, |ps| ps.pm_type);
    frame.item_pickups_mut().push(crate::ItemPickupRecord {
        picker: id.0 as i32,
        weapon,
        from_entnum: from,
        clip_r: 0,
        clip_l: 0,
        stock: 0,
        swapped_entnum: swapped,
        picker_pm_type,
    });
}

/// The item a player's use reaches and whose hint shows.
fn select(world: &mut World, client: u32) -> Option<(i32, u32)> {
    let number = super::t5_use::usable_item(world, client)?;
    let item = ground_weapon(world, number)?;
    Some((number, item.slots[0].weapon))
}

/// Answers whether a player's use search takes this item, and whether its
/// hint shows: it is not a scavenger pack, they could take it, and a player
/// holding a non-primary with two primaries already gets nothing.
pub(crate) fn usable_by(world: &mut World, client: u32, number: i32) -> bool {
    let Some(item) = ground_weapon(world, number) else {
        return false;
    };
    if item.scavenger {
        return false;
    }
    let id = ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    if !may_grab(&mut frame, id, &item, false) {
        return false;
    }
    let weapon = item.slots[0].weapon;
    let Some(ps) = frame.player(id).copied() else {
        return false;
    };
    let in_hand = def_i32(&frame, ps.weapon, DEF_INVENTORY_TYPE);
    let on_ground = def_i32(&frame, weapon, DEF_INVENTORY_TYPE);
    let hand_is_primary = in_hand == INVENTORY_PRIMARY || in_hand == INVENTORY_ALT;
    let ground_is_primary = on_ground == INVENTORY_PRIMARY || on_ground == INVENTORY_ALT;
    !(!hand_is_primary && ground_is_primary && primary_count(&frame, id) > 1)
}

fn primary_count(frame: &FrameWorld, id: ClientId) -> usize {
    held(frame, id)
        .into_iter()
        .filter(|&weapon| def_i32(frame, weapon, DEF_INVENTORY_TYPE) == INVENTORY_PRIMARY)
        .count()
}

/// The weapon a swap lays down: the one in hand, the base of an alternate
/// mode; none unless it is a primary.
fn weapon_to_swap(frame: &FrameWorld, id: ClientId) -> u32 {
    let Some(ps) = frame.player(id) else {
        return 0;
    };
    if ps.weapon == 0 {
        return 0;
    }
    let (weapon, kind) = if def_i32(frame, ps.weapon, DEF_INVENTORY_TYPE) == INVENTORY_ALT {
        (
            ps.weapon_primary,
            def_i32(frame, ps.weapon_primary, DEF_INVENTORY_TYPE),
        )
    } else {
        (ps.weapon, def_i32(frame, ps.weapon, DEF_INVENTORY_TYPE))
    };
    if !crate::script_player::has_weapon(frame, id, weapon) || kind != INVENTORY_PRIMARY {
        return 0;
    }
    weapon
}

/// A player's use taking an item: the item hears `touch`, the weapon in hand
/// goes down where the item lay when the player has two primaries, the item's
/// rounds come over, the item hears `trigger` with the weapon put down, and the
/// player raises the new weapon.
fn grab(world: &mut World, client: u32, number: i32) {
    let id = ClientId(client);
    let Some(item) = ground_weapon(world, number) else {
        return;
    };
    if let Some(object) = item_object(world, number) {
        let player = super::super::players::player_object(world, client);
        crate::script::runtime::raise(world, Value::Object(object), "touch", vec![player]);
    }
    let mut frame = FrameWorld::from_world(world);
    if !may_grab(&mut frame, id, &item, false) || !may_touch(&frame, id, &item) {
        return;
    }
    let weapon = item.slots[0].weapon;
    let Some(row) = frame.dropped_item_by_number(number) else {
        return;
    };
    let mut swapped = crate::item::ENTITY_NONE;
    let in_hand = frame.player(id).map_or(0, |ps| ps.weapon);
    if def_i32(&frame, weapon, DEF_INVENTORY_TYPE) == INVENTORY_PRIMARY {
        if in_hand != 0 && !crate::script_player::has_weapon(&frame, id, in_hand) {
            return;
        }
        if primary_count(&frame, id) > 1 {
            let lay_down = weapon_to_swap(&frame, id);
            if lay_down == 0 {
                return;
            }
            if let Some(down) = drop_weapon(world, id, lay_down, None, Some(number)) {
                let class = item_class(world, down);
                if super::player::new_item_entity(world, down, &class).is_ok() {
                    swapped = down;
                }
                let frame = FrameWorld::from_world(world);
                let start = row.origin;
                let lift = [start[0], start[1], start[2] + SWAP_LIFT];
                let up = trace_item(&frame, start, lift);
                let at = if up.startsolid != 0 {
                    start
                } else {
                    let top = lerp(start, lift, up.fraction);
                    let back = trace_item(&frame, top, start);
                    lerp(top, start, back.fraction)
                };
                let mut frame = FrameWorld::from_world(world);
                if let Some(down_row) = frame.dropped_item_mut_by_number(down) {
                    down_row.origin = at;
                    down_row.falling = false;
                    down_row.state.tr_type = TR_STATIONARY;
                    down_row.state.tr_base = at;
                    down_row.state.tr_time = 0;
                    down_row.state.tr_duration = 0;
                    down_row.state.tr_delta = [0.0; 3];
                    down_row.state.apos_tr_base = row.state.apos_tr_base;
                }
                if let Some(down_item) = world.resource_mut::<GroundWeapons>().0.get_mut(&down) {
                    down_item.grounded = item.grounded;
                }
            }
        }
    }
    let mut frame = FrameWorld::from_world(world);
    let akimbo = crate::game_move::left_weapon(&frame, weapon) != 0;
    if let Err(message) = crate::script_player::give_weapon(&mut frame, id, weapon, akimbo) {
        diag::warn!(Sim, "gsc: {message}; the weapon on the ground stays");
        return;
    }
    for slot in item.slots.iter().filter(|slot| slot.weapon != 0) {
        crate::script_player::set_ammo_clip(&mut frame, id, slot.weapon, slot.clip);
        crate::script_player::set_ammo_stock(&mut frame, id, slot.weapon, 0);
        add_ammo(&mut frame, id, slot.weapon, slot.stock, false);
    }
    record_pickup(&mut frame, id, weapon, number, swapped);
    crate::item::push_pickup_event(&mut frame, id, weapon, false);
    if def_i32(&frame, weapon, DEF_OFFHAND_CLASS) == 0 {
        crate::script_player::switch_to_weapon(&mut frame, id, weapon);
    }
    frame.despawn_dropped_item(number);
    world.resource_mut::<GroundWeapons>().0.remove(&number);
}
