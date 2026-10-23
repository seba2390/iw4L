//! What Black Ops' client does when a player fires: which of the weapon's
//! fire sounds the shot plays, its flash and shell, and its bullets' beams.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::Rule;
use game_api::fire::{
    AutoFire, ClientFireRules, FireAutoSimRules, FireSound, FireSoundShot, ImpactSoundWeapon,
    WhizbySound,
};

/// `weaponClass` turret.
const CLASS_TURRET: i32 = 7;
/// `fireType` single shot and minigun.
const FIRE_SINGLE_SHOT: i32 = 1;
const FIRE_MINIGUN: i32 = 6;

pub(crate) const CLIENT_FIRE: ClientFireRules = ClientFireRules {
    sound,
    // Black Ops' client fire event marks the shooter's entity fired (right
    // or left); its next weapon draw bolts the view flash to the viewmodel's
    // flash tag in first person, else the world flash to the entity's, and
    // the event ejects the view or world shell (the last-shot pair for a
    // last shot when both are authored) from the brass tag of the firing
    // hand, unless the weapon is bolt action, whose rechamber ejects it.
    effects: Rule::Known(()),
    left_from_own_weapon: true,
    autosim: Some(FireAutoSimRules {
        simulated,
        new: crate::fire_autosim::new,
    }),
    view_kick: Some(crate::view_kick::RULES),
    beams: Some(crate::tracer::RULES),
    impact_sound: Some(impact_sound),
    whizby: Some(whizby),
};

/// Black Ops' impact sound: its client picks a table by the weapon's
/// `impactType` (the exit side's own for most bullets) and plays the
/// table's alias for the surface struck, `<prefix>_<surface>`, where the
/// bullet hit or the explosion went off (beside the weapon's own explosion
/// sound). Type 6 reads the weapon's bounce sound table, and types 11 to 13
/// read the weapon's own explosion sound fields (falling back to
/// `wpn_mortar_shell_<surface>` and `wpn_tank_shell_<surface>` for 12 and
/// 13).
fn impact_sound(weapon: &ImpactSoundWeapon<'_>, surface: u8, exit: bool) -> Option<String> {
    let def = WeaponDefView::new(weapon.def, weapon.variant)?;
    let prefix = match (def.impact_type(), exit) {
        (1, false) => "prj_bullet_impact_small",
        (1, true) => "prj_bullet_impact_small_exit",
        (2, false) => "prj_bullet_impact_large",
        (2, true) => "prj_bullet_impact_large_exit",
        (3, false) => "prj_bullet_impact_ap",
        (3, true) => "prj_bullet_impact_ap_exit",
        (4, false) => "prj_bullet_impact_xtreme",
        (4, true) => "prj_bullet_impact_xtreme_exit",
        (5, false) => "prj_bulletspray_impact_small",
        (5, true) => "prj_bulletspray_small_exit",
        (6, _) => {
            return weapon
                .bounce_sounds
                .and_then(|table| table.get(usize::from(surface)))
                .and_then(Clone::clone);
        }
        (11, _) => return weapon.explosion_sounds[0].clone(),
        (12, _) => {
            if let Some(alias) = &weapon.explosion_sounds[1] {
                return Some(alias.clone());
            }
            "wpn_mortar_shell"
        }
        (13, _) => {
            if let Some(alias) = &weapon.explosion_sounds[2] {
                return Some(alias.clone());
            }
            "wpn_tank_shell"
        }
        (14, false) => "prj_bolt_impact",
        (14, true) => "prj_bolt_impact_exit",
        (15, _) => "prj_blade_impact",
        (7, _) => "wpn_grenade_explode",
        (8, _) => "wpn_rifle_grenade",
        (9 | 10, _) => "wpn_rocket_explode",
        _ => return None,
    };
    Some(format!("{prefix}_{}", crate::surface::name(surface)))
}

/// Black Ops' bullet whizby test: a bullet coming from in front of the view
/// origin (`dot(dir, normalize(start - view)) < 0`) that intersects the
/// 250-unit sphere around `view_origin` plays the weapon's own `whizbySound`
/// (else `prj_whizby`) and, when named, its `crackSound`.
fn whizby(
    weapon: &ImpactSoundWeapon<'_>,
    start: [f32; 3],
    end: [f32; 3],
    view_origin: [f32; 3],
) -> Option<WhizbySound> {
    const RADIUS: f32 = 250.0;
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let dz = end[2] - start[2];
    let d_len = (dz * dz + dx * dx + dy * dy).sqrt();
    let d_inv = if -d_len >= 0.0 { 1.0 } else { 1.0 / d_len };
    let dir = [dx * d_inv, dy * d_inv, dz * d_inv];

    let sx = start[0] - view_origin[0];
    let sy = start[1] - view_origin[1];
    let sz = start[2] - view_origin[2];
    let s_len = (sz * sz + sx * sx + sy * sy).sqrt();
    let s_inv = if -s_len >= 0.0 { 1.0 } else { 1.0 / s_len };
    let dot = dir[2] * sz * s_inv + dir[1] * sy * s_inv + dir[0] * sx * s_inv;
    if dot >= 0.0 {
        return None;
    }

    let a = dx * dx + dz * dz + dy * dy;
    let b = (sz * dz + sy * dy + sx * dx) * 2.0;
    let c = (view_origin[2] * view_origin[2]
        + view_origin[1] * view_origin[1]
        + view_origin[0] * view_origin[0]
        + start[2] * start[2]
        + start[1] * start[1]
        + start[0] * start[0]
        - (view_origin[2] * start[2] + view_origin[1] * start[1] + view_origin[0] * start[0])
            * 2.0)
        - RADIUS * RADIUS;
    let disc = b * b - c * a * 4.0;
    if disc <= 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let denom = a * 2.0;
    let mut t0 = (root - b) / denom;
    if t0 >= 1.0 {
        t0 = 1.0;
    }
    if -t0 >= 0.0 {
        t0 = 0.0;
    }
    let mut t1 = (-b - root) / denom;
    if t1 >= 1.0 {
        t1 = 1.0;
    }
    if -t1 >= 0.0 {
        t1 = 0.0;
    }
    if t0 == t1 {
        return None;
    }
    let ta = if t1 <= t0 { t1 } else { t0 };
    let origin = [dx * ta + start[0], dy * ta + start[1], dz * ta + start[2]];
    let alias = weapon
        .whizby_sound
        .filter(|s| !s.is_empty())
        .unwrap_or("prj_whizby")
        .to_owned();
    let crack_alias = weapon
        .crack_sound
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Some(WhizbySound {
        origin,
        alias,
        crack_alias,
    })
}

/// `fireType` burst.
const FIRE_BURST: i32 = 3;

/// A turret's shot, a shot this client does not view in first person, and
/// any shot in a killcam go to the client's automatic fire simulation
/// (`snd_autoSim`, on by default) unless the weapon fires single shots.
fn simulated(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Option<AutoFire> {
    let def = WeaponDefView::new(def, variant)?;
    let simulated = def.weapon_class() == CLASS_TURRET || !shot.player_view || shot.killcam;
    (simulated && def.fire_type() != FIRE_SINGLE_SHOT).then(|| AutoFire {
        fire_time_ms: def.fire_time(),
        burst: def.fire_type() == FIRE_BURST,
    })
}

/// A minigun plays no fire sound here. Any other shot plays (at once, or
/// through the automatic fire simulation) the player's sound in first
/// person, the world's otherwise, and the last shot sound of that view for
/// the right hand's last round when the weapon names one.
fn sound(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>> {
    let Some(def) = WeaponDefView::new(def, variant) else {
        return Rule::Unknown(crate::movement::weapon_unknown());
    };
    if def.fire_type() == FIRE_MINIGUN {
        return Rule::Known(None);
    }
    let last = shot.last_shot && !shot.left && shot.last_shot_sound;
    Rule::Known(Some(match (shot.player_view, last) {
        (false, false) => FireSound::Fire,
        (false, true) => FireSound::LastShot,
        (true, false) => FireSound::FirePlayer,
        (true, true) => FireSound::LastShotPlayer,
    }))
}
