//! What Black Ops 2's client does when a player fires: which of the weapon's
//! fire sounds the shot plays.

use fastfile_t6::weapon::{WeaponDefView, def, weap_class};
use game_api::fire::{ClientFireRules, FireSound, FireSoundShot};
use game_api::{Rule, Unknown, unknown};
use movement_t6::weapon::fire_type;

pub(crate) const CLIENT_FIRE: ClientFireRules = ClientFireRules {
    sound,
    effects: Rule::Unknown(unknown!(
        "t6.weapons.fire_effects",
        "a Black Ops 2 shot shows no muzzle flash, shell or tracer, plays no shell casing sound, and moves no view kick or rumble",
        "the rest of Black Ops 2's client fire event from its executable: the flash effect, shell ejection and its casing sound, tracers, the view kick and rumble"
    )),
    left_from_own_weapon: true,
};

fn autosim_unknown() -> &'static Unknown {
    unknown!(
        "t6.audio.fire_autosim",
        "the fire sound of an automatic or burst weapon fired by a player this client does not view in first person (and of a turret, and in a killcam) is not played",
        "the client's automatic fire sound simulation (snd_autoSim), which replays such a shot's sound every fire time while the shooter fires, and the firing state carried to clients on the shooter's entity"
    )
}

fn start_unknown() -> &'static Unknown {
    unknown!(
        "t6.audio.fire_start",
        "a burst or charge-shot weapon that names a fire start sound plays it with the first shot of a burst; the shot's place in its burst is not known here, so the weapon's shots play nothing",
        "the fire event's shot count in the client's fire sound rules, and a second sound with the shot"
    )
}

fn charge_level_unknown() -> &'static Unknown {
    unknown!(
        "t6.audio.fire_charge_level",
        "a charge-shot weapon's shot sounds the alternate weapon its charge level steps to; the level is not known here, so the shot plays nothing",
        "the fire event's charge level in the client's fire sound rules"
    )
}

fn killcam_sound_unknown() -> &'static Unknown {
    unknown!(
        "t6.audio.fire_killcam_sound",
        "a weapon naming a killcam fire sound plays it instead of its fire sound while the client's snapshot flags 1 or 2 are set; the flags do not reach the fire sound rules, so its shots play nothing",
        "the client's snapshot flags in the fire sound rules"
    )
}

fn left_weapon_unknown() -> &'static Unknown {
    unknown!(
        "t6.audio.fire_left_weapon",
        "a left-hand shot by a player this client does not view sounds the dual-wield weapon of the shooter's weapon; the runtime offers the client's own, so the shot plays nothing",
        "the shooter's dual-wield weapon for a left-hand shot in the client's fire sound rules"
    )
}

/// A shot by a player this client does not view, of a turret, or in a
/// killcam goes to the client's automatic fire simulation (`snd_autoSim`, on
/// by default) unless the weapon fires single or charged shots. Any other
/// shot plays at once: the player's sound in first person, the world's
/// otherwise, and the last shot sound of that view for either hand's last
/// round when the weapon names one.
fn sound(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>> {
    let Some(def) = WeaponDefView::new(def, variant) else {
        return Rule::Unknown(crate::movement::weapon_unknown());
    };
    if shot.left && !shot.player_view {
        return Rule::Unknown(left_weapon_unknown());
    }
    let fire = def.fire_type();
    if fire == fire_type::CHARGE_SHOT && !shot.left && !shot.last_shot {
        return Rule::Unknown(charge_level_unknown());
    }
    let burst = matches!(
        fire,
        fire_type::BURST_2..=fire_type::BURST_5 | fire_type::CHARGE_SHOT
    );
    let names = |field: u32| def.def_i32(field) != 0;
    // A shot this client does not view of a burst weapon naming a start sound plays at once.
    if !shot.player_view && burst && names(def::FIRE_START_SOUND) {
        return Rule::Unknown(start_unknown());
    }
    let automatic = fire != fire_type::SINGLE_SHOT && fire != fire_type::CHARGE_SHOT;
    let simulated = def.weapon_class() == weap_class::TURRET || !shot.player_view || shot.killcam;
    if automatic && simulated {
        return Rule::Unknown(autosim_unknown());
    }
    let (start, killcam) = match shot.player_view {
        true => (def::FIRE_START_SOUND_PLAYER, def::FIRE_KILLCAM_SOUND_PLAYER),
        false => (def::FIRE_START_SOUND, def::FIRE_KILLCAM_SOUND),
    };
    if burst && names(start) {
        return Rule::Unknown(start_unknown());
    }
    let last = shot.last_shot && shot.last_shot_sound;
    if !last && names(killcam) {
        return Rule::Unknown(killcam_sound_unknown());
    }
    Rule::Known(Some(match (shot.player_view, last) {
        (false, false) => FireSound::Fire,
        (false, true) => FireSound::LastShot,
        (true, false) => FireSound::FirePlayer,
        (true, true) => FireSound::LastShotPlayer,
    }))
}
