//! Which of a weapon's animations Black Ops 2's client plays on the viewmodel
//! when a player's weapon animation number changes, read from its
//! executable.

use fastfile_t6::weapon::weap_anim as a;
use game_api::{Unknown, ViewmodelAnim, ViewmodelRules, unknown};
use movement_t6::weapon::weap_anim as w;

/// The right-hand number Black Ops 2's client plays as the left gun's last
/// shot.
const LAST_SHOT_LEFT: u32 = 5;

pub(crate) const VIEWMODEL: ViewmodelRules = ViewmodelRules {
    anim,
    rate: None,
    left: None,
    hidden: None,
    camera: None,
    playback: Some(unknown!(
        "t6.viewmodel.playback",
        "a Black Ops 2 weapon animation starts with the runtime viewmodel's blend times and rates, not Black Ops 2's: it cuts to most animations at once, blends the crawl moves and a quick idle over half a second, and scales the rate by the player's perks for the weapon state (the reload, rate, switch, melee, sprint recovery, toss and equipment use multipliers)",
        "Black Ops 2's client animation start (its blend times and perk rates) in the runtime's viewmodel"
    )),
    unslotted: Some(unknown!(
        "t6.viewmodel.unslotted",
        "a Black Ops 2 weapon animation with no slot in the runtime's viewmodel (fire intro, empty melee and melee charge, empty sprint, low ready, continuous fire, crawl, deploy, breakdown, ADS fire intro, dive to prone, slide, mantle) is not played",
        "slots for Black Ops 2's own weapon animations in the runtime's viewmodel"
    )),
    pick: None,
    blend: None,
    left_empty_idle: false,
};

fn camera_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.camera",
        "the camera animation Black Ops 2 starts with a sprint loop, dive to prone or mantle animation, and stops with a sprint out, is not played",
        "camera animations on the runtime's first-person view"
    )
}

fn mantle_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.mantle",
        "the mantle animation plays only for a mantle higher than mantle_weapon_anim_height (32); the player's mantle is not known, so it does not change what plays",
        "the player's mantle transition carried to the client, and Black Ops 2's mantle heights"
    )
}

fn dual_wield_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.dual_wield",
        "a left gun's animation (Black Ops 2 plays them on the one viewmodel, from animation 78, by the left hand's own animation number) is not played",
        "the left hand's animation number carried to the client, and Black Ops 2's left-hand animation rules"
    )
}

fn fire_intro_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.fire_intro",
        "a shot plays the fire or ADS fire animation; Black Ops 2 plays the fire intro or ADS fire intro instead while the player's intro shots are fewer than the weapon's intro fire length",
        "the weapon's intro fire length and the player's intro shots fired in the client's viewmodel rules"
    )
}

fn follow_fire_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.follow_fire",
        "a last shot or ADS last shot always plays; while following another player or in a killcam, Black Ops 2 (cg_weaponSimulateFireAnims, on) plays a shot's animation from the number only when its client has marked that hand's shot",
        "whether the client follows a player or watches a killcam, and the client's shot marks, in the viewmodel rules"
    )
}

fn melee_pick_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.melee_pick",
        "a melee of a weapon naming more than one melee animation plays none; Black Ops 2 picks one of them at random",
        "a random pick among the weapon's melee animations in the client's viewmodel rules"
    )
}

/// Black Ops 2's client on a change of the weapon animation number. A
/// sprint, dive, low-ready, crawl or melee number the weapon has no
/// animation for falls back as Black Ops 2's does: to its full counterpart,
/// to idle at once, or to what plays. Numbers it does not know idle at once.
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
    // The first melee animation, unless the weapon names the next ones too.
    let melee = || match named(a::MELEE1) {
        false => play(a::MELEE),
        true => (Keep, Some(melee_pick_unknown())),
    };
    match weap_anim & !w::TOGGLE {
        w::IDLE => (Idle, None),
        w::QUICK_IDLE => (IdleNow, None),
        w::FIRE => (Play(a::FIRE), Some(fire_intro_unknown())),
        w::LAST_SHOT => (Play(a::LASTSHOT), Some(follow_fire_unknown())),
        LAST_SHOT_LEFT => (Keep, Some(dual_wield_unknown())),
        w::RECHAMBER => play(a::RECHAMBER),
        w::ADS_FIRE => (Play(a::ADS_FIRE), Some(fire_intro_unknown())),
        w::ADS_LAST_SHOT => (Play(a::ADS_LASTSHOT), Some(follow_fire_unknown())),
        w::ADS_RECHAMBER => play(a::ADS_RECHAMBER),
        w::MELEE => melee(),
        w::MELEE_EMPTY if named(a::MELEE_EMPTY) => play(a::MELEE_EMPTY),
        w::MELEE_EMPTY => melee(),
        w::MELEE_CHARGE => play(a::MELEE_CHARGE),
        w::MELEE_CHARGE_EMPTY => (first(&[a::MELEE_CHARGE_EMPTY], Play(a::MELEE_CHARGE)), None),
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
        w::SPRINT_OUT => (first(&[a::SPRINT_OUT], Keep), camera(a::SPRINT_CAMERA)),
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
        w::SPRINT_EMPTY_OUT => (
            first(&[a::SPRINT_EMPTY_OUT, a::SPRINT_OUT], Keep),
            camera(a::SPRINT_CAMERA),
        ),
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
        w::MANTLE if named(a::MANTLE) => (Keep, Some(mantle_unknown())),
        w::MANTLE if named(a::MANTLE_CAMERA) => (IdleNow, Some(mantle_unknown())),
        w::CRAWL_IN => (first(&[a::CRAWL_IN], Keep), None),
        w::CRAWL_FORWARD => (first(&[a::CRAWL_FORWARD], Keep), None),
        w::CRAWL_BACK => (first(&[a::CRAWL_BACK], Keep), None),
        w::CRAWL_RIGHT => (first(&[a::CRAWL_RIGHT], Keep), None),
        w::CRAWL_LEFT => (first(&[a::CRAWL_LEFT], Keep), None),
        w::CRAWL_OUT => (first(&[a::CRAWL_OUT], Keep), None),
        w::CRAWL_EMPTY_IN => (first(&[a::CRAWL_EMPTY_IN, a::CRAWL_IN], Keep), None),
        w::CRAWL_EMPTY_FORWARD => (
            first(&[a::CRAWL_EMPTY_FORWARD, a::CRAWL_FORWARD], Keep),
            None,
        ),
        w::CRAWL_EMPTY_BACK => (first(&[a::CRAWL_EMPTY_BACK, a::CRAWL_BACK], Keep), None),
        w::CRAWL_EMPTY_RIGHT => (first(&[a::CRAWL_EMPTY_RIGHT, a::CRAWL_RIGHT], Keep), None),
        w::CRAWL_EMPTY_LEFT => (first(&[a::CRAWL_EMPTY_LEFT, a::CRAWL_LEFT], Keep), None),
        w::CRAWL_EMPTY_OUT => (first(&[a::CRAWL_EMPTY_OUT, a::CRAWL_OUT], Keep), None),
        _ => (IdleNow, None),
    }
}
