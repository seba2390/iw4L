//! Black Ops' ammo and clip numbers: which of a player's weapons share
//! rounds beside the clip and in it.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::movement::AmmoWeapon;

/// The clip number of a weapon with unlimited ammo.
const UNLIMITED_CLIP: i32 = -1;

/// Each weapon in load order takes an ammo number and a clip number. A
/// `sharedAmmo` weapon takes the first earlier one's whose ammo (clip) name
/// is the same; otherwise a dual-wield weapon whose other hand was loaded
/// first takes that weapon's ammo number; otherwise a weapon takes a new
/// number. A weapon with unlimited ammo has no clip of its own. Black Ops
/// counts from 0; the runtime's numbers count from 1.
pub(crate) fn numbers(weapons: &[AmmoWeapon<'_>]) -> Vec<(i32, i32)> {
    let mut ammo_owners: Vec<usize> = Vec::new();
    let mut clip_owners: Vec<usize> = Vec::new();
    let mut out: Vec<Option<(i32, i32)>> = vec![None; weapons.len()];
    for (at, weapon) in weapons.iter().enumerate() {
        let Some(def) = WeaponDefView::new(weapon.def, weapon.variant) else {
            out[at] = Some((
                new_slot(&mut ammo_owners, at),
                new_slot(&mut clip_owners, at),
            ));
            continue;
        };
        let shared = |owners: &[usize], clip: bool| {
            def.shared_ammo()
                .then(|| {
                    owners
                        .iter()
                        .position(|&owner| name(&weapons[owner], clip) == name(weapon, clip))
                })
                .flatten()
                .map(|slot| slot as i32 + 1)
        };
        let ammo = shared(&ammo_owners, false)
            .or_else(|| {
                weapon
                    .dual_wield
                    .filter(|_| def.dual_wield())
                    .and_then(|other| out[other])
                    .map(|(ammo, _)| ammo)
            })
            .unwrap_or_else(|| new_slot(&mut ammo_owners, at));
        let clip = if def.unlimited_ammo() {
            UNLIMITED_CLIP
        } else {
            shared(&clip_owners, true).unwrap_or_else(|| new_slot(&mut clip_owners, at))
        };
        out[at] = Some((ammo, clip));
    }
    out.into_iter()
        .map(|numbers| numbers.unwrap_or((0, 0)))
        .collect()
}

/// A weapon's clip name, or its ammo name.
fn name<'a>(weapon: &AmmoWeapon<'a>, clip: bool) -> &'a str {
    if clip {
        weapon.clip_name
    } else {
        weapon.ammo_name
    }
}

fn new_slot(owners: &mut Vec<usize>, at: usize) -> i32 {
    owners.push(at);
    owners.len() as i32
}
