//! The weapons a Black Ops 2 player holds and their ammo: per held weapon a
//! few flags, and the clip and the rounds beside it kept by the held slot of
//! the weapon that owns them.

// TODO(weapon port): remove once the weapon state machine uses these.
#![allow(dead_code)]

use crate::state::{HeldWeapon, MAX_HELD_WEAPONS, PlayerState};
use crate::weapon::{fire_type, inventory_type, weapon_class};
use crate::{MoveWorld, tuning};

/// The held weapon `weapon` is; none for no weapon.
pub(crate) fn held(ps: &PlayerState, weapon: u32) -> Option<&HeldWeapon> {
    if weapon & 0xff == 0 {
        return None;
    }
    ps.held_weapons.iter().find(|h| h.weapon == weapon)
}

pub(crate) fn held_mut(ps: &mut PlayerState, weapon: u32) -> Option<&mut HeldWeapon> {
    if weapon & 0xff == 0 {
        return None;
    }
    ps.held_weapons.iter_mut().find(|h| h.weapon == weapon)
}

/// The held slot of an ammo owner; unlike `held`, weapon 0 finds an empty
/// slot.
fn slot(ps: &PlayerState, owner: u32) -> Option<usize> {
    ps.held_weapons.iter().position(|h| h.weapon == owner)
}

fn dual_wield<W: MoveWorld>(world: &W, weapon: u32) -> bool {
    weapon & 0xff != 0 && world.weapon(weapon).def.dual_wield()
}

/// A dual-wield weapon's ammo lives on its left-hand weapon.
fn ammo_on_left<W: MoveWorld>(world: &W, weapon: u32) -> bool {
    let d = world.weapon(weapon).def;
    d.inventory_type() == inventory_type::DW_LEFT_HAND
        || (dual_wield(world, weapon) && d.player_anim_type() == 1)
}

/// A charge-shot weapon's rounds belong to the weapon its variant loads
/// under.
fn charge_shot_owner<W: MoveWorld>(world: &W, weapon: u32, owner: u32) -> u32 {
    if world.weapon(weapon).def.fire_type() != fire_type::CHARGE_SHOT {
        return owner;
    }
    let mut owner = owner;
    let mut from = weapon;
    while let Some(parent) = world.variant_parent(from) {
        owner = parent;
        from = parent;
    }
    owner
}

/// The weapon whose held slot keeps `weapon`'s clip.
pub(crate) fn clip_owner<W: MoveWorld>(world: &W, weapon: u32) -> u32 {
    let w = world.weapon(weapon);
    let mut owner = weapon;
    if w.def.inventory_type() == inventory_type::ALT_MODE
        && !w.def.ignore_attachments()
        && !w.def.dual_wield()
    {
        owner = w.alt_weapon;
    }
    charge_shot_owner(world, weapon, owner)
}

/// The weapon whose held slot keeps `weapon`'s rounds beside the clip.
pub(crate) fn ammo_owner<W: MoveWorld>(world: &W, weapon: u32) -> u32 {
    let w = world.weapon(weapon);
    let mut owner = weapon;
    if w.def.inventory_type() == inventory_type::ALT_MODE && !w.def.ignore_attachments() {
        owner = w.alt_weapon;
    }
    if dual_wield(world, weapon) && ammo_on_left(world, weapon) {
        owner = w.dual_wield_weapon;
    }
    charge_shot_owner(world, weapon, owner)
}

/// Rounds in the clip `weapon` loads.
pub(crate) fn clip<W: MoveWorld>(ps: &PlayerState, world: &W, weapon: u32) -> i32 {
    slot(ps, clip_owner(world, weapon)).map_or(0, |i| ps.ammo_in_clip[i])
}

/// Rounds beside the clip for `weapon`.
pub(crate) fn stock<W: MoveWorld>(ps: &PlayerState, world: &W, weapon: u32) -> i32 {
    slot(ps, ammo_owner(world, weapon)).map_or(0, |i| ps.ammo_not_in_clip[i])
}

/// Every round `weapon` has.
pub(crate) fn total<W: MoveWorld>(ps: &PlayerState, world: &W, weapon: u32) -> i32 {
    stock(ps, world, weapon) + clip(ps, world, weapon)
}

/// Adds to the clip, never below empty; the clip after.
pub(crate) fn add_clip<W: MoveWorld>(
    ps: &mut PlayerState,
    world: &W,
    weapon: u32,
    count: i32,
) -> i32 {
    let Some(i) = slot(ps, clip_owner(world, weapon)) else {
        return 0;
    };
    ps.ammo_in_clip[i] = (ps.ammo_in_clip[i] + count).max(0);
    ps.ammo_in_clip[i]
}

pub(crate) fn set_clip<W: MoveWorld>(ps: &mut PlayerState, world: &W, weapon: u32, count: i32) {
    if let Some(i) = slot(ps, clip_owner(world, weapon)) {
        ps.ammo_in_clip[i] = count.max(0);
    }
}

/// Takes `count` rounds from the clip, as many as it has; the clip after.
pub(crate) fn use_clip<W: MoveWorld>(
    ps: &mut PlayerState,
    world: &W,
    weapon: u32,
    count: i32,
) -> i32 {
    let count = count.min(clip(ps, world, weapon));
    let Some(i) = slot(ps, clip_owner(world, weapon)) else {
        return 0;
    };
    ps.ammo_in_clip[i] = (ps.ammo_in_clip[i] - count).max(0);
    ps.ammo_in_clip[i]
}

/// Adds to the rounds beside the clip, never below none.
pub(crate) fn add_stock<W: MoveWorld>(ps: &mut PlayerState, world: &W, weapon: u32, count: i32) {
    if let Some(i) = slot(ps, ammo_owner(world, weapon)) {
        ps.ammo_not_in_clip[i] = (ps.ammo_not_in_clip[i] + count).max(0);
    }
}

/// Empties the clip and the rounds beside it that `weapon` uses.
pub(crate) fn clear_rounds<W: MoveWorld>(ps: &mut PlayerState, world: &W, weapon: u32) {
    if let Some(i) = slot(ps, ammo_owner(world, weapon)) {
        ps.ammo_not_in_clip[i] = 0;
    }
    if let Some(i) = slot(ps, clip_owner(world, weapon)) {
        ps.ammo_in_clip[i] = 0;
    }
}

/// A clip's size: scaled by `player_clipSizeMultiplier` when over one round.
pub(crate) fn clip_size<W: MoveWorld>(world: &W, weapon: u32) -> i32 {
    let size = world.weapon(weapon).def.clip_size();
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
    clip(ps, world, hand_weapon(ps, world))
}

/// The running hand's clip is empty; never for unlimited ammo.
pub(crate) fn hand_clip_empty<W: MoveWorld>(ps: &PlayerState, world: &W) -> bool {
    if world.weapon(ps.weapon).def.unlimited_ammo() {
        return false;
    }
    slot(ps, clip_owner(world, hand_weapon(ps, world))).is_none_or(|i| ps.ammo_in_clip[i] == 0)
}

pub(crate) fn needs_rechamber(ps: &PlayerState, weapon: u32) -> bool {
    held(ps, weapon).is_some_and(|h| h.needs_rechamber)
}

/// The second magazine of a dual-magazine weapon is in.
pub(crate) fn dual_mag_in<W: MoveWorld>(ps: &PlayerState, world: &W, weapon: u32) -> bool {
    held(ps, weapon).is_some_and(|h| h.dual_mag) && world.weapon(weapon).def.dual_mag()
}

pub(crate) fn fuel(ps: &PlayerState, weapon: u32) -> i32 {
    held(ps, weapon).map_or(0, |h| h.fuel)
}

pub(crate) fn set_fuel(ps: &mut PlayerState, weapon: u32, fuel: i32) {
    if let Some(h) = held_mut(ps, weapon) {
        h.fuel = fuel;
    }
}

/// A held weapon and its rounds, as the match keeps them: `(weapon, clip,
/// stock)`.
pub type HeldRounds = (u32, i32, i32);

/// Takes the match's held weapons and rounds into the player state: each
/// slot keeps its own weapon's rounds, and a weapon new to its slot is
/// given; a weapon gone from every slot is taken.
pub fn load_held<W: MoveWorld>(ps: &mut PlayerState, world: &W, held: &[HeldRounds]) {
    ps.ammo_in_clip = [0; MAX_HELD_WEAPONS];
    ps.ammo_not_in_clip = [0; MAX_HELD_WEAPONS];
    let mut given = [0u32; MAX_HELD_WEAPONS];
    for (i, &(weapon, clip, stock)) in held.iter().enumerate().take(MAX_HELD_WEAPONS) {
        ps.ammo_in_clip[i] = clip;
        ps.ammo_not_in_clip[i] = stock;
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

fn gave<W: MoveWorld>(ps: &mut PlayerState, world: &W, weapon: u32) {
    let d = world.weapon(weapon).def;
    if d.weapon_class() == weapon_class::ITEM {
        return;
    }
    if d.weapon_class() == weapon_class::MELEE {
        ps.melee_weapon = weapon;
    }
    if d.offhand_class() == 0 {
        return;
    }
    if ps.offhand_index == 0 {
        ps.offhand_index = weapon;
        return;
    }
    let current = ps.offhand_index;
    if total(ps, world, current) < 1 {
        let slot = world.weapon(current).def.offhand_slot();
        let other = ps.held_weapons.iter().map(|h| h.weapon).find(|&w| {
            w != 0 && world.weapon(w).def.offhand_slot() == slot && total(ps, world, w) > 0
        });
        ps.offhand_index = other.unwrap_or(weapon);
    }
}

/// The held weapons and their rounds after the step, by slot.
pub fn held_rounds<W: MoveWorld>(ps: &PlayerState, world: &W) -> [HeldRounds; MAX_HELD_WEAPONS] {
    core::array::from_fn(|i| {
        let weapon = ps.held_weapons[i].weapon;
        if weapon == 0 {
            return (0, 0, 0);
        }
        (weapon, clip(ps, world, weapon), stock(ps, world, weapon))
    })
}
