//! The weapons a Black Ops player holds and their ammo: per held weapon a
//! few flags, and ammo kept by the shared ammo and clip numbers.

use crate::state::{AmmoSlot, HeldWeapon, PlayerState};
use crate::{MoveWorld, Weapon, tuning};

pub(crate) fn held(ps: &PlayerState, weapon: u32) -> Option<&HeldWeapon> {
    if weapon == 0 {
        return None;
    }
    ps.held_weapons.iter().find(|h| h.weapon == weapon)
}

fn held_mut(ps: &mut PlayerState, weapon: u32) -> Option<&mut HeldWeapon> {
    if weapon == 0 {
        return None;
    }
    ps.held_weapons.iter_mut().find(|h| h.weapon == weapon)
}

pub(crate) fn has_weapon(ps: &PlayerState, weapon: u32) -> bool {
    held(ps, weapon).is_some()
}

pub(crate) fn needs_rechamber(ps: &PlayerState, weapon: u32) -> bool {
    held(ps, weapon).is_some_and(|h| h.needs_rechamber)
}

pub(crate) fn set_needs_rechamber(ps: &mut PlayerState, weapon: u32, on: bool) {
    if let Some(h) = held_mut(ps, weapon) {
        h.needs_rechamber = on;
    }
}

pub(crate) fn used_before(ps: &PlayerState, weapon: u32) -> bool {
    held(ps, weapon).is_some_and(|h| h.used_before)
}

pub(crate) fn set_used_before(ps: &mut PlayerState, weapon: u32, on: bool) {
    if let Some(h) = held_mut(ps, weapon) {
        h.used_before = on;
    }
}

/// The second magazine of a dual-magazine weapon is in.
pub(crate) fn dual_mag_in(ps: &PlayerState, w: &Weapon<'_>, weapon: u32) -> bool {
    w.def.dual_mag() && held(ps, weapon).is_some_and(|h| h.dual_mag)
}

pub(crate) fn set_dual_mag(ps: &mut PlayerState, w: &Weapon<'_>, weapon: u32, swap: bool) {
    let dual = w.def.dual_mag();
    if let Some(h) = held_mut(ps, weapon) {
        h.dual_mag = dual && swap;
    }
}

pub(crate) fn fuel(ps: &PlayerState, weapon: u32) -> i32 {
    held(ps, weapon).map_or(0, |h| h.fuel)
}

pub(crate) fn set_fuel(ps: &mut PlayerState, weapon: u32, fuel: i32) {
    if let Some(h) = held_mut(ps, weapon) {
        h.fuel = fuel;
    }
}

pub(crate) fn add_fuel(ps: &mut PlayerState, weapon: u32, fuel: i32) {
    if let Some(h) = held_mut(ps, weapon) {
        h.fuel += fuel;
    }
}

/// Rounds in the clip a weapon loads.
pub(crate) fn clip(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    ps.ammo_in_clip
        .iter()
        .find(|slot| slot.index == w.clip_index)
        .map_or(0, |slot| slot.count)
}

/// Rounds beside the clip, for a weapon's ammo.
pub(crate) fn stock(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    ps.ammo_not_in_clip
        .iter()
        .find(|slot| slot.index == w.ammo_index)
        .map_or(0, |slot| slot.count)
}

/// Every round a weapon has.
pub(crate) fn total(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    stock(ps, w) + clip(ps, w)
}

/// The clip's slot, taking a free one for a clip not yet kept.
fn clip_slot(ps: &mut PlayerState, clip_index: i32) -> Option<&mut AmmoSlot> {
    let at = ps
        .ammo_in_clip
        .iter()
        .position(|slot| slot.index == clip_index)
        .or_else(|| {
            let free = ps.ammo_in_clip.iter().position(|slot| slot.index == 0)?;
            ps.ammo_in_clip[free] = AmmoSlot {
                index: clip_index,
                count: 0,
            };
            Some(free)
        })?;
    Some(&mut ps.ammo_in_clip[at])
}

pub(crate) fn set_clip(ps: &mut PlayerState, w: &Weapon<'_>, count: i32) {
    if let Some(slot) = clip_slot(ps, w.clip_index) {
        slot.count = count.max(0);
    }
}

/// Takes `count` rounds from the clip, as many as it has.
pub(crate) fn use_clip(ps: &mut PlayerState, w: &Weapon<'_>, count: i32) {
    let count = count.min(clip(ps, w));
    if let Some(slot) = clip_slot(ps, w.clip_index) {
        slot.count = (slot.count - count).max(0);
    }
}

/// Adds to the rounds beside the clip; a full table gives up its first slot.
pub(crate) fn add_stock(ps: &mut PlayerState, w: &Weapon<'_>, count: i32) {
    let at = match ps
        .ammo_not_in_clip
        .iter()
        .position(|slot| slot.index == w.ammo_index)
    {
        Some(at) => at,
        None => {
            let at = ps
                .ammo_not_in_clip
                .iter()
                .position(|slot| slot.index == 0)
                .unwrap_or(0);
            ps.ammo_not_in_clip[at] = AmmoSlot {
                index: w.ammo_index,
                count: 0,
            };
            at
        }
    };
    let slot = &mut ps.ammo_not_in_clip[at];
    slot.count += count;
    if slot.count < 0 {
        slot.count = 0;
    }
}

/// A clip's size: scaled by `player_clipSizeMultiplier` when over one round.
pub(crate) fn clip_size(w: &Weapon<'_>) -> i32 {
    let size = w.def.clip_size();
    if size > 1 {
        crate::math::round_nudged(size as f32 * tuning::CLIP_SIZE_MULTIPLIER).max(1)
    } else {
        size
    }
}

/// The weapon the running hand holds.
pub(crate) fn hand_weapon<W: MoveWorld>(ps: &PlayerState, world: &W) -> u32 {
    if ps.left_hand {
        world.weapon(ps.weapon).dual_wield_weapon
    } else {
        ps.weapon
    }
}

/// Rounds in the running hand's clip.
pub(crate) fn hand_clip<W: MoveWorld>(ps: &PlayerState, world: &W) -> i32 {
    clip(ps, &world.weapon(hand_weapon(ps, world)))
}

/// The running hand's clip is empty; never for unlimited ammo.
pub(crate) fn hand_clip_empty<W: MoveWorld>(ps: &PlayerState, world: &W) -> bool {
    if world.weapon(ps.weapon).def.unlimited_ammo() {
        return false;
    }
    hand_clip(ps, world) == 0
}

/// A held weapon and its rounds, as the match keeps them: `(weapon, clip,
/// stock)`.
pub type HeldRounds = (u32, i32, i32);

/// Takes the match's held weapons and rounds into the player state: rounds
/// fill the shared clip and ammo slots, and a weapon new to its slot is given
/// as Black Ops gives one; a weapon gone from every slot is taken.
pub fn load_held<W: MoveWorld>(ps: &mut PlayerState, world: &W, held: &[HeldRounds]) {
    ps.ammo_in_clip = [AmmoSlot::default(); crate::state::MAX_HELD_WEAPONS];
    ps.ammo_not_in_clip = [AmmoSlot::default(); crate::state::MAX_HELD_WEAPONS];
    for &(weapon, clip, stock) in held {
        if weapon == 0 {
            continue;
        }
        let w = world.weapon(weapon);
        if !ps.ammo_in_clip.iter().any(|s| s.index == w.clip_index)
            && let Some(slot) = ps.ammo_in_clip.iter_mut().find(|s| s.index == 0)
        {
            *slot = AmmoSlot {
                index: w.clip_index,
                count: clip,
            };
        }
        if !ps.ammo_not_in_clip.iter().any(|s| s.index == w.ammo_index)
            && let Some(slot) = ps.ammo_not_in_clip.iter_mut().find(|s| s.index == 0)
        {
            *slot = AmmoSlot {
                index: w.ammo_index,
                count: stock,
            };
        }
    }
    let mut given = [0u32; crate::state::MAX_HELD_WEAPONS];
    for (i, &(weapon, _, _)) in held.iter().enumerate().take(ps.held_weapons.len()) {
        let slot = &mut ps.held_weapons[i];
        if slot.weapon == weapon {
            continue;
        }
        let gone = slot.weapon;
        *slot = HeldWeapon {
            weapon,
            ..HeldWeapon::default()
        };
        if gone != 0 && !held.iter().any(|h| h.0 == gone) {
            if ps.weapon == gone {
                ps.weapon = 0;
            }
            if ps.melee_weapon == gone {
                ps.melee_weapon = 0;
            }
        }
        given[i] = weapon;
    }
    for weapon in given {
        if weapon != 0 {
            gave(ps, world, weapon);
        }
    }
}

/// What Black Ops does with a weapon it gives: a melee weapon becomes the
/// melee weapon, an offhand the offhand when none with ammo is ready.
fn gave<W: MoveWorld>(ps: &mut PlayerState, world: &W, weapon: u32) {
    let d = world.weapon(weapon).def;
    if d.weapon_class() == crate::weapon::weapon_class::ITEM {
        return;
    }
    if d.weapon_class() == crate::weapon::weapon_class::MELEE {
        ps.melee_weapon = weapon;
    }
    if d.offhand_class() == 0 {
        return;
    }
    if ps.offhand_index == 0 {
        ps.offhand_index = weapon;
        return;
    }
    let current = world.weapon(ps.offhand_index);
    if total(ps, &current) < 1 {
        let slot = current.def.offhand_slot();
        let other = ps.held_weapons.iter().map(|h| h.weapon).find(|&w| {
            w != 0 && world.weapon(w).def.offhand_slot() == slot && total(ps, &world.weapon(w)) > 0
        });
        ps.offhand_index = other.unwrap_or(weapon);
    }
}

/// The held weapons and their rounds after the step, by slot.
pub fn held_rounds<W: MoveWorld>(
    ps: &PlayerState,
    world: &W,
) -> [HeldRounds; crate::state::MAX_HELD_WEAPONS] {
    core::array::from_fn(|i| {
        let weapon = ps.held_weapons[i].weapon;
        if weapon == 0 {
            return (0, 0, 0);
        }
        let w = world.weapon(weapon);
        (weapon, clip(ps, &w), stock(ps, &w))
    })
}
