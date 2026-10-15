//! What Black Ops' client does when a player fires: which of the weapon's
//! fire sounds the shot plays, and its flash and shell.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{ClientFireRules, FireSound, FireSoundShot};
use game_api::{Rule, Unknown, unknown};

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
    // Tracers and the view kick are `t5.weapons.tracers` and
    // `t5.weapons.view_kick`.
    effects: Rule::Known(()),
    left_from_own_weapon: true,
};

fn autosim_unknown() -> &'static Unknown {
    unknown!(
        "t5.audio.fire_autosim",
        "the fire sound of an automatic weapon fired by a player this client does not view in first person (and of a turret, and in a killcam) is not played",
        "the client's automatic fire sound simulation (snd_autoSim), which replays such a shot's sound every fire time while the shooter's entity is firing, and the firing flag the movement sets carried to clients on the shooter's entity"
    )
}

/// A minigun plays no fire sound here. A turret's shot, a shot this client
/// does not view in first person, and any shot in a killcam go to the
/// client's automatic fire simulation (`snd_autoSim`, on by default) unless
/// the weapon fires single shots. Any other shot plays at once: the
/// player's sound in first person, the world's otherwise, and the last shot
/// sound of that view for the right hand's last round when the weapon names
/// one.
fn sound(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>> {
    let Some(def) = WeaponDefView::new(def, variant) else {
        return Rule::Unknown(crate::movement::weapon_unknown());
    };
    if def.fire_type() == FIRE_MINIGUN {
        return Rule::Known(None);
    }
    let simulated = def.weapon_class() == CLASS_TURRET || !shot.player_view || shot.killcam;
    if simulated && def.fire_type() != FIRE_SINGLE_SHOT {
        return Rule::Unknown(autosim_unknown());
    }
    let last = shot.last_shot && !shot.left && shot.last_shot_sound;
    Rule::Known(Some(match (shot.player_view, last) {
        (false, false) => FireSound::Fire,
        (false, true) => FireSound::LastShot,
        (true, false) => FireSound::FirePlayer,
        (true, true) => FireSound::LastShotPlayer,
    }))
}
