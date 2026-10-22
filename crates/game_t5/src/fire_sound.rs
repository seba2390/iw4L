//! What Black Ops' client does when a player fires: which of the weapon's
//! fire sounds the shot plays, its flash and shell, and its bullets' beams.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::Rule;
use game_api::fire::{AutoFire, ClientFireRules, FireAutoSimRules, FireSound, FireSoundShot};

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
};

/// Black Ops' impact sound: its client picks a table by the weapon's
/// `impactType` (the exit side's own for most bullets) and plays the
/// table's alias for the surface struck, `<prefix>_<surface>`, where the
/// bullet hit or the explosion went off (beside the weapon's own explosion
/// sound). The bouncing and weapon-specific types (6, 11 to 13) sound
/// through their weapon's own aliases.
fn impact_sound(def: &[u8], variant: &[u8], surface: u8, exit: bool) -> Option<String> {
    let def = WeaponDefView::new(def, variant)?;
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
