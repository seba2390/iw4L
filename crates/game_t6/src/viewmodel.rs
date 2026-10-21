//! Which of a weapon's animations Black Ops 2's client plays on the viewmodel
//! when a player's weapon animation number changes, at what rate and over
//! what blend, read from its executable.

use fastfile_t6::weapon::weap_anim as a;
use game_api::movement::{GAME_MOVE_BYTES, MovePlayer};
use game_api::{
    CameraAnim, Unknown, ViewmodelAnim, ViewmodelCamera, ViewmodelPick, ViewmodelRules,
    ViewmodelStart, unknown,
};
use movement_t6::state::weapon_state as ws;
use movement_t6::weapon::weap_anim as w;
use movement_t6::{perks, tuning};

/// The right-hand number Black Ops 2's client plays as the left gun's last
/// shot.
const LAST_SHOT_LEFT: u32 = 5;

pub(crate) const VIEWMODEL: ViewmodelRules = ViewmodelRules {
    anim,
    rate: Some(rate),
    left: Some(left),
    hidden: None,
    camera: Some(ViewmodelCamera {
        tag: "tag_camera",
        anim: camera,
    }),
    playback: None,
    unslotted: None,
    pick: Some(pick),
    blend: Some(blend),
    left_empty_idle: true,
};

fn mantle_unknown() -> &'static Unknown {
    unknown!(
        "t6.viewmodel.mantle",
        "the mantle animation plays only for a mantle higher than mantle_weapon_anim_height (32), at the rate of the mantle's own time; the player's mantle is not known to the viewmodel, so it does not change what plays",
        "the player's mantle transition carried to the client, and Black Ops 2's mantle heights"
    )
}

/// Where an animation's time comes from: a field of the weapon's definition
/// or of its variant, in msec; the rate is the animation's length over it.
#[derive(Clone, Copy)]
enum Time {
    Def(usize),
    Variant(usize),
    /// The animation plays at its own rate.
    Own,
}

/// Black Ops 2's client's time field for each animation, in its own order.
/// (The reloads' times are the variant's, scaled by the attachments as the
/// weapon's bytes are built; the first raise's and the alternate raise's and
/// drop's an attachment may set instead are not read.)
const TIMES: [Time; 0x58] = {
    use Time::{Def, Own, Variant};
    let mut t = [Own; 0x58];
    t[a::HOLD_FIRE] = Def(0x478);
    t[a::FLOURISH] = Def(0x530);
    t[a::RECHAMBER] = Def(0x470);
    t[a::MELEE] = Def(0x480);
    t[a::MELEE1] = Def(0x480);
    t[a::MELEE2] = Def(0x480);
    t[a::MELEE3] = Def(0x480);
    t[a::MELEE_EMPTY] = Def(0x480);
    t[a::MELEE_CHARGE] = Def(0x488);
    t[a::MELEE_CHARGE_EMPTY] = Def(0x488);
    t[a::RELOAD] = Variant(0x1e0);
    t[a::RELOAD_RIGHT] = Def(0x48c);
    t[a::RELOAD_EMPTY] = Variant(0x1e4);
    t[a::RELOAD_START] = Def(0x4ac);
    t[a::RELOAD_END] = Def(0x4b4);
    t[a::RELOAD_QUICK] = Variant(0x1e8);
    t[a::RELOAD_QUICK_EMPTY] = Variant(0x1ec);
    t[a::RAISE] = Def(0x4bc);
    t[a::FIRST_RAISE] = Def(0x4cc);
    t[a::DROP] = Def(0x4b8);
    t[a::ALT_RAISE] = Variant(0x1f8);
    t[a::ALT_DROP] = Def(0x4c0);
    t[a::QUICK_RAISE] = Def(0x4c8);
    t[a::QUICK_DROP] = Def(0x4c4);
    t[a::EMPTY_RAISE] = Def(0x4d0);
    t[a::EMPTY_DROP] = Def(0x4d4);
    t[a::SPRINT_IN] = Def(0x4d8);
    t[a::SPRINT_LOOP] = Def(0x4dc);
    t[a::SPRINT_OUT] = Def(0x4e0);
    t[a::SPRINT_EMPTY_IN] = Def(0x4d8);
    t[a::SPRINT_EMPTY_LOOP] = Def(0x4dc);
    t[a::SPRINT_EMPTY_OUT] = Def(0x4e0);
    t[a::LOWREADY_IN] = Def(0x4e4);
    t[a::LOWREADY_LOOP] = Def(0x4e8);
    t[a::LOWREADY_OUT] = Def(0x4ec);
    t[a::CONT_FIRE_IN] = Def(0x4f0);
    t[a::CONT_FIRE_LOOP] = Def(0x4f4);
    t[a::CONT_FIRE_OUT] = Def(0x4f8);
    t[a::CRAWL_IN] = Def(0x508);
    t[a::CRAWL_FORWARD] = Def(0x50c);
    t[a::CRAWL_BACK] = Def(0x510);
    t[a::CRAWL_RIGHT] = Def(0x514);
    t[a::CRAWL_LEFT] = Def(0x518);
    t[a::CRAWL_OUT] = Def(0x520);
    t[a::CRAWL_EMPTY_IN] = Def(0x508);
    t[a::CRAWL_EMPTY_FORWARD] = Def(0x50c);
    t[a::CRAWL_EMPTY_BACK] = Def(0x510);
    t[a::CRAWL_EMPTY_RIGHT] = Def(0x514);
    t[a::CRAWL_EMPTY_LEFT] = Def(0x518);
    t[a::CRAWL_EMPTY_OUT] = Def(0x520);
    t[a::DEPLOY] = Def(0x528);
    t[a::BREAKDOWN] = Def(0x52c);
    t[a::NIGHTVISION_WEAR] = Def(0x534);
    t[a::NIGHTVISION_REMOVE] = Def(0x540);
    t[a::DTP_IN] = Def(0x4fc);
    t[a::DTP_LOOP] = Def(0x500);
    t[a::DTP_OUT] = Def(0x504);
    t[a::DTP_EMPTY_IN] = Def(0x4fc);
    t[a::DTP_EMPTY_LOOP] = Def(0x500);
    t[a::DTP_EMPTY_OUT] = Def(0x504);
    t[a::SLIDE_IN] = Def(0x524);
    t[a::SPRINT_CAMERA] = Def(0x4dc);
    t[a::DTP_IN_CAMERA] = Def(0x4fc);
    t[a::DTP_LOOP_CAMERA] = Def(0x500);
    t[a::DTP_OUT_CAMERA] = Def(0x504);
    t[a::DW_LEFT_FLOURISH] = Def(0x530);
    t[a::DW_LEFT_RELOAD_EMPTY] = Variant(0x1e4);
    t[a::DW_LEFT_RELOAD] = Variant(0x1e0);
    t
};

/// The definition's sprint scale and move speed scale, which the sprint
/// loop's time is scaled by.
const SPRINT_SCALE: usize = 0x29c;
const MOVE_SPEED_SCALE: usize = 0x588;
const SPRINT_LOOP_SCALE: f32 = 0.5;
/// The definition's intro fire length (shots).
const INTRO_FIRE_LENGTH: usize = 0x960;
/// The definition's offhand slot, and its equipment and specific-use slots.
const OFFHAND_SLOT: usize = 100;
const EQUIPMENT_SLOT: i32 = 3;
const SPECIFIC_USE_SLOT: i32 = 4;

/// What a zero perk multiplier makes the rate.
const INSTANT_RATE: f32 = 1000.0;
/// The perk multipliers this rule reads that `movement_t6` has no use for.
const PERK_WEAP_MELEE_MULTIPLIER: f32 = 0.5;
const PERK_SPRINT_RECOVERY_MULTIPLIER: f32 = 0.3;
/// A quick idle blends in over this when something still plays.
const QUICK_IDLE_BLEND: f32 = 0.5;
/// The crawl animations blend in over this.
const CRAWL_BLEND: f32 = 0.5;

fn i32_at(bytes: &[u8], off: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

fn f32_at(bytes: &[u8], off: usize) -> Option<f32> {
    Some(f32::from_le_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

fn player(game: &[u8; GAME_MOVE_BYTES]) -> movement_t6::state::PlayerState {
    let mut player = MovePlayer {
        game: *game,
        ..MovePlayer::default()
    };
    crate::movement::unpack(&mut player).0
}

/// Black Ops 2's client's rate for an animation it starts: the animation's
/// length over the weapon's time for it (0 for a time of 0, 1 with none;
/// the sprint loop's time scaled by the weapon's sprint and move speed
/// scales over `player_sprintCameraBob`), then over the perk multiplier for
/// what the player's weapon is doing: a reload of either hand with fast
/// reload, a rechamber with rate of fire, a raise or drop with fast weapon
/// switch (not of a specific-use offhand), the melee's end with fast melee
/// recovery, the sprint's drop with sprint recovery, an offhand's states
/// with fast toss (fast equipment use for equipment).
fn rate(start: &ViewmodelStart<'_>) -> f32 {
    let time = match TIMES.get(start.anim).copied().unwrap_or(Time::Own) {
        Time::Def(off) => i32_at(start.def, off),
        Time::Variant(off) => i32_at(start.variant, off),
        Time::Own => None,
    };
    let mut rate = match time {
        None => 1.0,
        Some(0) => 0.0,
        Some(time) => {
            let mut time = time as f32;
            if start.anim == a::SPRINT_LOOP {
                let sprint = f32_at(start.def, SPRINT_SCALE).unwrap_or(1.0);
                let speed = f32_at(start.def, MOVE_SPEED_SCALE).unwrap_or(1.0);
                time *= sprint * SPRINT_LOOP_SCALE * speed / tuning::SPRINT_CAMERA_BOB;
            }
            start.length_ms as f32 / time
        }
    };
    let ps = player(start.game);
    let has = |perk| perks::has(ps.perks, perk);
    let reloading = |s: i32| (ws::RELOADING..=ws::RELOADING_LEFT_INTERRUPT).contains(&s);
    let over = |rate: f32, mult: f32| {
        if mult == 0.0 {
            INSTANT_RATE
        } else {
            rate / mult
        }
    };
    let slot = i32_at(start.def, OFFHAND_SLOT).unwrap_or(0);
    if (reloading(ps.weaponstate) || reloading(ps.weaponstate_left)) && has(perks::FAST_RELOAD) {
        return over(rate, tuning::PERK_WEAP_RELOAD_MULTIPLIER);
    }
    if matches!(ps.weaponstate, ws::RECHAMBERING | ws::RECHAMBER_END) && has(perks::RATE_OF_FIRE) {
        return over(rate, tuning::PERK_WEAP_RATE_MULTIPLIER);
    }
    let switching = (ws::RAISING..=ws::DROPPING_ALT).contains(&ps.weaponstate);
    if has(perks::FAST_WEAPON_SWITCH) && switching && slot != SPECIFIC_USE_SLOT {
        rate /= tuning::PERK_WEAP_SWITCH_MULTIPLIER;
    } else if has(perks::FAST_MELEE_RECOVERY) && ps.weaponstate == ws::MELEE_END {
        rate /= PERK_WEAP_MELEE_MULTIPLIER;
    } else if ps.weaponstate == ws::SPRINT_DROP && has(perks::SPRINT_RECOVERY) {
        return over(rate, PERK_SPRINT_RECOVERY_MULTIPLIER);
    } else if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&ps.weaponstate) {
        if has(perks::FAST_TOSS) && slot != EQUIPMENT_SLOT {
            rate /= tuning::PERK_WEAP_TOSS_MULTIPLIER;
        } else if has(perks::FAST_EQUIPMENT_USE) && slot == EQUIPMENT_SLOT {
            rate /= tuning::PERK_WEAP_EQUIPMENT_USE_MULTIPLIER;
        }
    }
    rate
}

/// Black Ops 2's client cuts to every animation it starts but the crawl
/// moves, which blend in over half a second.
fn blend(anim: usize) -> f32 {
    if (a::CRAWL_IN..=a::CRAWL_EMPTY_OUT).contains(&anim) {
        CRAWL_BLEND
    } else {
        0.0
    }
}

/// Black Ops 2's client on a change of a dual-wield weapon's left-hand
/// number: the left idle (its empty idle while the left clip is empty) once
/// the left animations have finished, at once on a quick idle; the left
/// fire, last shot and reloads; any other number leaves what plays.
fn left(
    weap_anim: u32,
    _named: &dyn Fn(usize) -> bool,
) -> (ViewmodelAnim, Option<&'static Unknown>) {
    use ViewmodelAnim::{Idle, IdleNow, Keep, Play};
    (
        match weap_anim & !w::TOGGLE {
            w::IDLE => Idle,
            w::QUICK_IDLE => IdleNow,
            w::FIRE => Play(a::DW_LEFT_FIRE),
            w::LAST_SHOT => Play(a::DW_LEFT_LASTSHOT),
            w::RELOAD => Play(a::DW_LEFT_RELOAD),
            w::RELOAD_EMPTY => Play(a::DW_LEFT_RELOAD_EMPTY),
            _ => Keep,
        },
        None,
    )
}

/// Black Ops 2's client with the player's own fields: a shot plays the fire
/// intro (its ADS one aiming) while the player's intro shots are fewer than
/// the weapon's intro fire length; a melee plays one of the weapon's melee
/// animations at random (the first, and each next one it names in turn); a
/// quick idle blends in over half a second (shortened by fast weapon switch).
/// The rest is [`anim`]'s. The local player's own shots always play (the
/// client's follow and killcam marks are for watching another player).
fn pick(weap_anim: u32, pick: &ViewmodelPick<'_>) -> (ViewmodelAnim, Option<&'static Unknown>) {
    use ViewmodelAnim::{IdleOver, Play};
    let named = pick.named;
    let intro = || {
        let ps = player(pick.game);
        let length = i32_at(pick.def, INTRO_FIRE_LENGTH).unwrap_or(0);
        length != 0 && ps.intro_shots_fired < length
    };
    let melee = || {
        let count = 1 + [a::MELEE1, a::MELEE2, a::MELEE3]
            .iter()
            .take_while(|&&anim| named(anim))
            .count();
        Play(a::MELEE + pick.random as usize % count)
    };
    match weap_anim & !w::TOGGLE {
        w::QUICK_IDLE => {
            let ps = player(pick.game);
            let blend = if perks::has(ps.perks, perks::FAST_WEAPON_SWITCH) {
                QUICK_IDLE_BLEND * tuning::PERK_WEAP_SWITCH_MULTIPLIER
            } else {
                QUICK_IDLE_BLEND
            };
            (IdleOver(blend), None)
        }
        w::FIRE => (Play(if intro() { a::FIRE_INTRO } else { a::FIRE }), None),
        w::ADS_FIRE => (
            Play(if intro() {
                a::ADS_FIRE_INTRO
            } else {
                a::ADS_FIRE
            }),
            None,
        ),
        w::MELEE => (melee(), None),
        w::MELEE_EMPTY if named(a::MELEE_EMPTY) => (Play(a::MELEE_EMPTY), None),
        w::MELEE_EMPTY => (melee(), None),
        _ => anim(weap_anim, named),
    }
}

/// Black Ops 2's client's camera animations: the sprint loop and the dive to
/// prone's start their own when the weapon names it, the sprint out stops
/// them. The mantle's is not run (`t6.viewmodel.mantle`).
fn camera(weap_anim: u32, named: &dyn Fn(usize) -> bool) -> Option<CameraAnim> {
    let start = |anim: usize| named(anim).then_some(CameraAnim::Start(anim));
    match weap_anim & !w::TOGGLE {
        w::SPRINT_LOOP | w::SPRINT_EMPTY_LOOP => start(a::SPRINT_CAMERA),
        w::SPRINT_OUT | w::SPRINT_EMPTY_OUT => named(a::SPRINT_CAMERA).then_some(CameraAnim::Stop),
        w::DTP_IN | w::DTP_EMPTY_IN => start(a::DTP_IN_CAMERA),
        w::DTP_LOOP | w::DTP_EMPTY_LOOP => start(a::DTP_LOOP_CAMERA),
        w::DTP_OUT | w::DTP_EMPTY_OUT => start(a::DTP_OUT_CAMERA),
        _ => None,
    }
}

/// Black Ops 2's client on a change of the weapon animation number, without
/// the weapon's or player's fields. A sprint, dive, low-ready, crawl or melee
/// number the weapon has no animation for falls back as Black Ops 2's does:
/// to its full counterpart, to idle at once, or to what plays. Numbers it
/// does not know idle at once.
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
    let play = |anim: usize| (Play(anim), None);
    match weap_anim & !w::TOGGLE {
        w::IDLE => (Idle, None),
        w::QUICK_IDLE => (IdleNow, None),
        w::FIRE => play(a::FIRE),
        w::LAST_SHOT => play(a::LASTSHOT),
        LAST_SHOT_LEFT => play(a::DW_LEFT_LASTSHOT),
        w::RECHAMBER => play(a::RECHAMBER),
        w::ADS_FIRE => play(a::ADS_FIRE),
        w::ADS_LAST_SHOT => play(a::ADS_LASTSHOT),
        w::ADS_RECHAMBER => play(a::ADS_RECHAMBER),
        w::MELEE => play(a::MELEE),
        w::MELEE_EMPTY => (first(&[a::MELEE_EMPTY], Play(a::MELEE)), None),
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
            None,
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
            None,
        ),
        w::SPRINT_EMPTY_OUT => (first(&[a::SPRINT_EMPTY_OUT, a::SPRINT_OUT], Keep), None),
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
        w::DTP_IN => (first(&[a::DTP_IN], IdleNow), None),
        w::DTP_LOOP => (first(&[a::DTP_LOOP], IdleNow), None),
        w::DTP_OUT => (first(&[a::DTP_OUT], IdleNow), None),
        w::DTP_EMPTY_IN => (first(&[a::DTP_EMPTY_IN, a::DTP_IN], IdleNow), None),
        w::DTP_EMPTY_LOOP => (first(&[a::DTP_EMPTY_LOOP, a::DTP_LOOP], IdleNow), None),
        w::DTP_EMPTY_OUT => (first(&[a::DTP_EMPTY_OUT, a::DTP_OUT], IdleNow), None),
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
