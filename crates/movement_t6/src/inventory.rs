//! The weapons a Black Ops 2 player holds and their ammo: per held weapon a
//! few flags, and ammo kept by the shared ammo and clip numbers.

use crate::state::{AmmoSlot, HeldWeapon, PlayerState};
use crate::{MoveWorld, Weapon};

pub(crate) fn clip(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    ps.ammo_in_clip
        .iter()
        .find(|slot| slot.index == w.clip_index)
        .map_or(0, |slot| slot.count)
}

pub(crate) fn stock(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    ps.ammo_not_in_clip
        .iter()
        .find(|slot| slot.index == w.ammo_index)
        .map_or(0, |slot| slot.count)
}

pub(crate) fn total(ps: &PlayerState, w: &Weapon<'_>) -> i32 {
    stock(ps, w) + clip(ps, w)
}

pub type HeldRounds = (u32, i32, i32);

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
