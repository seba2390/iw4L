//! What Black Ops 2's client does when a player fires: which of the weapon's
//! fire sounds the shot plays, its flash, shell and tracer effects, automatic
//! fire sound simulation (`snd_autoSim`), and view kick.

use fastfile_t6::weapon::{WeaponDefView, def, weap_class};
use game_api::Rule;
use game_api::fire::{AutoFire, ClientFireRules, FireAutoSimRules, FireSound, FireSoundShot};
use movement_t6::weapon::fire_type;

/// `loopFireSound` and `loopFireSoundPlayer` in `WeapDef`.
const LOOP_FIRE_SOUND: u32 = 184;
const LOOP_FIRE_SOUND_PLAYER: u32 = 188;

pub(crate) const CLIENT_FIRE: ClientFireRules = ClientFireRules {
    sound,
    effects: Rule::Known(()),
    left_from_own_weapon: false,
    autosim: Some(FireAutoSimRules {
        simulated,
        new: crate::fire_autosim::new,
    }),
    view_kick: Some(crate::view_kick::RULES),
    beams: None,
    impact_sound: None,
};

/// A shot by a player this client does not view, of a turret, or in a
/// killcam goes to the client's automatic fire simulation (`snd_autoSim`, on
/// by default) unless the weapon fires single or charged shots, is a burst
/// weapon naming a world fire start sound, or has a loop fire sound.
fn simulated(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Option<AutoFire> {
    let def = WeaponDefView::new(def, variant)?;
    let fire = def.fire_type();
    if fire == fire_type::SINGLE_SHOT || fire == fire_type::CHARGE_SHOT {
        return None;
    }
    let burst = matches!(fire, fire_type::BURST_2..=fire_type::BURST_5);
    if !shot.player_view && burst && def.def_i32(def::FIRE_START_SOUND) != 0 {
        return None;
    }
    let loop_field = if shot.player_view {
        LOOP_FIRE_SOUND_PLAYER
    } else {
        LOOP_FIRE_SOUND
    };
    if matches!(fire, fire_type::FULL_AUTO | fire_type::MINIGUN) && def.def_i32(loop_field) != 0 {
        return None;
    }
    let simulated = def.weapon_class() == weap_class::TURRET || !shot.player_view || shot.killcam;
    simulated.then(|| AutoFire {
        fire_time_ms: def.fire_time(),
        burst,
    })
}

/// A minigun with a loop fire sound plays no per-shot fire sound here. Any
/// other shot plays (at once, or through `snd_autoSim`) the player's sound in
/// first person, the world's otherwise, the killcam sound when in a killcam
/// and the weapon names one, the last shot sound of that view for either
/// hand's last round when the weapon names one, and the fire start sound on
/// the first round of a burst (`parm == 1`).
fn sound(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>> {
    let Some(def) = WeaponDefView::new(def, variant) else {
        return Rule::Known(None);
    };
    let fire = def.fire_type();
    let names = |field: u32| def.def_i32(field) != 0;
    let loop_field = if shot.player_view {
        LOOP_FIRE_SOUND_PLAYER
    } else {
        LOOP_FIRE_SOUND
    };
    if fire == fire_type::MINIGUN && names(loop_field) {
        return Rule::Known(None);
    }
    let burst = matches!(fire, fire_type::BURST_2..=fire_type::BURST_5);
    let (start, killcam) = match shot.player_view {
        true => (def::FIRE_START_SOUND_PLAYER, def::FIRE_KILLCAM_SOUND_PLAYER),
        false => (def::FIRE_START_SOUND, def::FIRE_KILLCAM_SOUND),
    };
    let last = shot.last_shot && shot.last_shot_sound;
    if last {
        return Rule::Known(Some(if shot.player_view {
            FireSound::LastShotPlayer
        } else {
            FireSound::LastShot
        }));
    }
    if shot.killcam && names(killcam) {
        return Rule::Known(Some(if shot.player_view {
            FireSound::KillcamPlayer
        } else {
            FireSound::Killcam
        }));
    }
    let with_start = burst && shot.parm == 1 && names(start);
    Rule::Known(Some(match (shot.player_view, with_start) {
        (false, false) => FireSound::Fire,
        (false, true) => FireSound::FireWithStart,
        (true, false) => FireSound::FirePlayer,
        (true, true) => FireSound::FirePlayerWithStart,
    }))
}
