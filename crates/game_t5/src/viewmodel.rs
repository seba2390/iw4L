//! Which of a weapon's animations Black Ops' client plays on the viewmodel
//! when a player's weapon animation number changes, read from its
//! executable (the zombie mode's rules).

use fastfile_t5::size::weap_anim as a;
use game_api::{Unknown, ViewmodelAnim, ViewmodelRules, unknown};
use movement_t5::weapon::weap_anim as w;

/// Right-hand numbers Black Ops' client plays as the left gun's fire.
const FIRE_LEFT: u32 = 3;
const LAST_SHOT_LEFT: u32 = 5;

pub(crate) const VIEWMODEL: ViewmodelRules = ViewmodelRules {
    anim,
    playback: unknown!(
        "t5.viewmodel.playback",
        "a Black Ops weapon animation starts with the runtime viewmodel's blend times and rates (from the weapon's timers), not Black Ops' own: it cuts to most animations at once, blends only a quick idle, and takes each animation's rate from its own time field and the fast reload and rechamber perks",
        "Black Ops' client animation start (its per-animation time table and perk rates) in the runtime's viewmodel"
    ),
    unslotted: unknown!(
        "t5.viewmodel.unslotted",
        "a Black Ops weapon animation with no slot in the runtime's viewmodel (low ready, continuous fire, deploy, breakdown, dive to prone, slide) is not played",
        "slots for Black Ops' own weapon animations in the runtime's viewmodel"
    ),
};

fn camera_unknown() -> &'static Unknown {
    unknown!(
        "t5.viewmodel.camera",
        "the camera animation Black Ops plays with a sprint, dive or mantle animation is not played",
        "camera animations on the runtime's first-person view"
    )
}

fn mantle_unknown() -> &'static Unknown {
    unknown!(
        "t5.viewmodel.mantle",
        "the mantle animation plays only for a mantle higher than mantle_weapon_anim_height (32); the player's mantle is not known, so it does not change what plays",
        "Black Ops' mantling in movement_t5 and its mantle animation heights"
    )
}

fn dual_wield_unknown() -> &'static Unknown {
    unknown!(
        "t5.viewmodel.dual_wield",
        "a left gun's animation (Black Ops plays them on the one viewmodel, from its 58th animation) is not played",
        "the left hand's animation number carried to the client, and Black Ops' left-hand animation rules"
    )
}

/// Black Ops' client on a change of the weapon animation number. A sprint,
/// dive or low-ready number the weapon has no animation for falls back as
/// Black Ops' does: to its full or empty counterpart, or to idle at once;
/// a sprint in or out the weapon does not name leaves what plays. Numbers
/// it does not know idle at once.
fn anim(
    weap_anim: u32,
    named: &dyn Fn(usize) -> bool,
) -> (ViewmodelAnim, Option<&'static Unknown>) {
    use ViewmodelAnim::{Idle, IdleNow, Keep, Play};
    let first = |anims: &[usize], or: ViewmodelAnim| {
        anims
            .iter()
            .find(|&&anim| named(anim))
            .map_or(or, |&anim| Play(anim))
    };
    let camera = |anim: usize| named(anim).then(camera_unknown);
    let play = |anim: usize| (Play(anim), None);
    match weap_anim & !w::TOGGLE {
        w::IDLE => (Idle, None),
        w::QUICK_IDLE => (IdleNow, None),
        w::FIRE => play(a::FIRE),
        FIRE_LEFT | LAST_SHOT_LEFT => (Keep, Some(dual_wield_unknown())),
        w::LAST_SHOT => play(a::LASTSHOT),
        w::RECHAMBER => play(a::RECHAMBER),
        w::ADS_FIRE => play(a::ADS_FIRE),
        w::ADS_LAST_SHOT => play(a::ADS_LASTSHOT),
        w::ADS_RECHAMBER => play(a::ADS_RECHAMBER),
        w::MELEE => play(a::MELEE),
        w::MELEE_CHARGE => play(a::MELEE_CHARGE),
        w::DROP => play(a::DROP),
        w::RAISE => play(a::RAISE),
        w::FIRST_RAISE => play(a::FIRST_RAISE),
        w::RELOAD => play(a::RELOAD),
        w::RELOAD_EMPTY => play(a::RELOAD_EMPTY),
        w::RELOAD_START => play(a::RELOAD_START),
        w::RELOAD_END => play(a::RELOAD_END),
        w::RELOAD_QUICK => play(a::RELOAD_QUICK),
        w::RELOAD_QUICK_EMPTY => play(a::RELOAD_QUICK_EMPTY),
        w::ALT_DROP => play(a::ALT_DROP),
        w::ALT_RAISE => play(a::ALT_RAISE),
        w::QUICK_DROP => play(a::QUICK_DROP),
        w::QUICK_RAISE => play(a::QUICK_RAISE),
        w::EMPTY_DROP => play(a::EMPTY_DROP),
        w::EMPTY_RAISE => play(a::EMPTY_RAISE),
        w::SPRINT_IN => (first(&[a::SPRINT_IN], Keep), None),
        // The loop plays when the weapon names the sprint in, not the loop.
        w::SPRINT_LOOP => (
            if named(a::SPRINT_IN) {
                Play(a::SPRINT_LOOP)
            } else {
                IdleNow
            },
            camera(a::SPRINT_CAMERA),
        ),
        w::SPRINT_OUT => (first(&[a::SPRINT_OUT], Keep), None),
        w::SPRINT_EMPTY_IN => (first(&[a::SPRINT_EMPTY_IN, a::SPRINT_IN], Keep), None),
        w::SPRINT_EMPTY_LOOP => (
            if named(a::SPRINT_EMPTY_IN) {
                Play(a::SPRINT_EMPTY_LOOP)
            } else if named(a::SPRINT_IN) {
                Play(a::SPRINT_LOOP)
            } else {
                IdleNow
            },
            camera(a::SPRINT_CAMERA),
        ),
        w::SPRINT_EMPTY_OUT => (first(&[a::SPRINT_EMPTY_OUT, a::SPRINT_OUT], Keep), None),
        // `zombiemode` is set: the low-ready animations play.
        w::LOWREADY_IN => (first(&[a::LOWREADY_IN], IdleNow), None),
        w::LOWREADY_LOOP => (first(&[a::LOWREADY_LOOP], IdleNow), None),
        w::LOWREADY_OUT => (first(&[a::LOWREADY_OUT], IdleNow), None),
        w::CONT_FIRE_IN => play(a::CONT_FIRE_IN),
        w::CONT_FIRE_LOOP => play(a::CONT_FIRE_LOOP),
        w::CONT_FIRE_OUT => play(a::CONT_FIRE_OUT),
        w::HOLD_FIRE => play(a::HOLD_FIRE),
        w::DETONATE => play(a::DETONATE),
        w::NIGHTVISION_WEAR => play(a::NIGHTVISION_WEAR),
        w::NIGHTVISION_REMOVE => play(a::NIGHTVISION_REMOVE),
        w::DEPLOY => play(a::DEPLOY),
        w::BREAKDOWN => play(a::BREAKDOWN),
        w::DTP_IN => (first(&[a::DTP_IN], IdleNow), camera(a::DTP_IN_CAMERA)),
        w::DTP_LOOP => (first(&[a::DTP_LOOP], IdleNow), camera(a::DTP_LOOP_CAMERA)),
        w::DTP_OUT => (first(&[a::DTP_OUT], IdleNow), camera(a::DTP_OUT_CAMERA)),
        w::DTP_EMPTY_IN => (
            first(&[a::DTP_EMPTY_IN, a::DTP_IN], IdleNow),
            camera(a::DTP_IN_CAMERA),
        ),
        w::DTP_EMPTY_LOOP => (
            first(&[a::DTP_EMPTY_LOOP, a::DTP_LOOP], IdleNow),
            camera(a::DTP_LOOP_CAMERA),
        ),
        w::DTP_EMPTY_OUT => (
            first(&[a::DTP_EMPTY_OUT, a::DTP_OUT], IdleNow),
            camera(a::DTP_OUT_CAMERA),
        ),
        w::SLIDE_IN => (first(&[a::SLIDE_IN], IdleNow), None),
        w::MANTLE if named(a::MANTLE) || named(a::MANTLE_CAMERA) => (Keep, Some(mantle_unknown())),
        _ => (IdleNow, None),
    }
}
