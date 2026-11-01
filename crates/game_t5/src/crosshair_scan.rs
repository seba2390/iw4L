//! Black Ops' crosshair scan: at the end of each server frame a line from
//! the player's eye along the view marks in the player's weapon flags what
//! the crosshair is on, which colours the client's crosshair (and holds
//! fire on a friend).

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::movement::{CrosshairHitKind, CrosshairScan};
use movement_t5::state::{PlayerState, weap_flags};

/// The scan's mask (Black Ops' shot contents).
const MASK: u32 = 0x2280_e803;
/// How far the scan looks.
const RANGE: f32 = 15000.0;
/// A hit closer than this is the eye's own start in solid: nothing.
const NEAREST: f32 = 0.0001;

const CLEARED: u32 =
    weap_flags::CANCEL_GRENADE | weap_flags::ENEMY_IN_CROSSHAIR | weap_flags::ZOMBIE_GUN_TARGET;

fn flag(scan: &CrosshairScan<'_>, name: &str) -> bool {
    (scan.dvar)(name).is_some_and(|value| value.trim().parse::<f32>().is_ok_and(|v| v != 0.0))
}

fn float(scan: &CrosshairScan<'_>, name: &str, default: f32) -> f32 {
    (scan.dvar)(name)
        .and_then(|value| value.trim().parse::<f32>().ok())
        .unwrap_or(default)
}

/// The eye moved aside by the player's lean (16 degrees of roll, 20 units).
fn lean(eye: [f32; 3], yaw: f32, leanf: f32) -> [f32; 3] {
    if leanf == 0.0 {
        return eye;
    }
    let fraction = (2.0 - leanf.abs()) * leanf;
    let (_, right, _) = movement_t5::math::angle_vectors([0.0, yaw, 16.0 * fraction]);
    let scale = fraction * 20.0;
    std::array::from_fn(|i| eye[i] + scale * right[i])
}

pub(crate) fn scan(ps: &mut PlayerState, scan: &CrosshairScan<'_>) {
    // A player who has run no command yet, or the level's first half
    // second: no scan.
    if ps.command_time == 0 || scan.level_time <= 500 {
        return;
    }
    ps.weap_flags &= !CLEARED;
    if flag(scan, "player_forceRedCrosshair") {
        ps.weap_flags |= weap_flags::ENEMY_IN_CROSSHAIR;
    }
    // The eye (a vehicle seat's gun would look from its tag; no Kino or Five
    // vehicle has one).
    let eye = lean(
        [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ],
        ps.viewangles[1],
        ps.leanf,
    );
    let (forward, _, _) = movement_t5::math::angle_vectors(ps.viewangles);
    let offset = float(scan, "bg_gunXOffset", 0.0);
    let start = std::array::from_fn(|i| offset * forward[i] + eye[i]);
    let end = std::array::from_fn(|i| forward[i] * RANGE + start[i]);
    let Some((hit, fraction)) = (scan.trace)(start, end, MASK) else {
        return;
    };
    if fraction * RANGE < NEAREST {
        return;
    }
    let Some(weapon) = (scan.weapon)(ps.weapon) else {
        return;
    };
    // Multiplayer reads 600 units for a player carrying a turret.
    let range = if !flag(scan, "zombiemode") && ps.weap_flags & weap_flags::CARRYING_TURRET != 0 {
        600.0
    } else {
        WeaponDefView::new(&weapon.def, &weapon.variant)
            .map_or(0.0, |def| def.enemy_crosshair_range())
    };
    let d = std::array::from_fn::<f32, 3, _>(|i| hit.origin[i] - start[i]);
    let dist_sq = d[2] * d[2] + d[1] * d[1] + d[0] * d[0];
    let (allied, actor) = match hit.kind {
        CrosshairHitKind::Actor { allied } => (allied, true),
        CrosshairHitKind::Player { allied } => (allied, false),
        // Turrets and the other entities a player reads names from are
        // not marked (none on Kino or Five).
        CrosshairHitKind::Other => return,
    };
    if flag(scan, "zombietron") {
        return;
    }
    if flag(scan, "zombiemode") && actor {
        // The upgraded ballistic knives mark some actors the same way, by a
        // field of the actor not identified (`t5.hud.crosshair`).
        if weapon.name.starts_with("humangun_") {
            if dist_sq < range * range {
                ps.weap_flags |= weap_flags::ZOMBIE_GUN_TARGET;
            }
            return;
        }
    }
    if allied {
        let friendly = float(scan, "g_friendlyfireDist", 175.0);
        if dist_sq < friendly * friendly {
            ps.weap_flags |= weap_flags::CANCEL_GRENADE;
        }
    } else if dist_sq < range * range {
        ps.weap_flags |= weap_flags::ENEMY_IN_CROSSHAIR;
    }
}
