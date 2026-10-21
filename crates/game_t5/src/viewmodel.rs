//! Which of a weapon's animations Black Ops' client plays on the viewmodel
//! when a player's weapon animation number changes, and at what rate, read
//! from its executable (the zombie mode's rules).

use fastfile_t5::size::weap_anim as a;
use game_api::movement::{GAME_MOVE_BYTES, MovePlayer};
use game_api::{
    CameraAnim, Unknown, ViewmodelAnim, ViewmodelCamera, ViewmodelPick, ViewmodelRules,
    ViewmodelStart,
};
use movement_t5::weapon::state as ws;
use movement_t5::weapon::weap_anim as w;
use movement_t5::{perks, tuning};

/// Right-hand numbers Black Ops' client plays as the left gun's fire.
const FIRE_LEFT: u32 = 3;
const LAST_SHOT_LEFT: u32 = 5;

pub(crate) const VIEWMODEL: ViewmodelRules = ViewmodelRules {
    anim,
    rate: Some(rate),
    left: Some(left),
    hidden: Some(hidden),
    camera: Some(ViewmodelCamera {
        tag: "tag_camera",
        anim: camera,
    }),
    playback: None,
    unslotted: None,
    pick: Some(pick),
    blend: None,
    left_empty_idle: false,
};

/// Where an animation's time comes from: a field of the weapon's definition
/// or of its variant, in msec; the rate is the animation's length over it.
#[derive(Clone, Copy)]
enum Time {
    Def(usize),
    Variant(usize),
    /// The animation plays at its own rate.
    Own,
}

/// Black Ops' client's time field for each animation, in its own order.
const TIMES: [Time; 0x40] = {
    use Time::{Def, Own, Variant};
    let mut t = [Own; 0x40];
    t[a::HOLD_FIRE] = Def(0x3bc);
    t[a::RECHAMBER] = Def(0x3b4);
    t[a::MELEE] = Def(0x3c4);
    t[a::MELEE_CHARGE] = Def(0x3c8);
    t[a::RELOAD] = Variant(0x24);
    t[a::RELOAD_RIGHT] = Def(0x3cc);
    t[a::RELOAD_EMPTY] = Variant(0x28);
    t[a::RELOAD_START] = Def(0x3ec);
    t[a::RELOAD_END] = Def(0x3f4);
    t[a::RELOAD_QUICK] = Variant(0x2c);
    t[a::RELOAD_QUICK_EMPTY] = Variant(0x30);
    t[a::RAISE] = Def(0x3fc);
    t[a::FIRST_RAISE] = Def(0x40c);
    t[a::DROP] = Def(0x3f8);
    t[a::ALT_RAISE] = Variant(0x3c);
    t[a::ALT_DROP] = Def(0x400);
    t[a::QUICK_RAISE] = Def(0x408);
    t[a::QUICK_DROP] = Def(0x404);
    t[a::EMPTY_RAISE] = Def(0x410);
    t[a::EMPTY_DROP] = Def(0x414);
    t[a::SPRINT_IN] = Def(0x418);
    t[a::SPRINT_LOOP] = Def(0x41c);
    t[a::SPRINT_OUT] = Def(0x420);
    t[a::SPRINT_EMPTY_IN] = Def(0x418);
    t[a::SPRINT_EMPTY_LOOP] = Def(0x41c);
    t[a::SPRINT_EMPTY_OUT] = Def(0x420);
    t[a::LOWREADY_IN] = Def(0x424);
    t[a::LOWREADY_LOOP] = Def(0x428);
    t[a::LOWREADY_OUT] = Def(0x42c);
    t[a::CONT_FIRE_IN] = Def(0x430);
    t[a::CONT_FIRE_LOOP] = Def(0x434);
    t[a::CONT_FIRE_OUT] = Def(0x438);
    t[a::DEPLOY] = Def(0x44c);
    t[a::BREAKDOWN] = Def(0x450);
    t[a::NIGHTVISION_WEAR] = Def(0x454);
    t[a::NIGHTVISION_REMOVE] = Def(0x460);
    t[a::DTP_IN] = Def(0x43c);
    t[a::DTP_LOOP] = Def(0x440);
    t[a::DTP_OUT] = Def(0x444);
    t[a::DTP_EMPTY_IN] = Def(0x43c);
    t[a::DTP_EMPTY_LOOP] = Def(0x440);
    t[a::DTP_EMPTY_OUT] = Def(0x444);
    t[a::SLIDE_IN] = Def(0x448);
    t[a::SPRINT_CAMERA] = Def(0x41c);
    t[a::DTP_IN_CAMERA] = Def(0x43c);
    t[a::DTP_LOOP_CAMERA] = Def(0x440);
    t[a::DTP_OUT_CAMERA] = Def(0x444);
    t[a::RELOAD_EMPTY_LEFT] = Variant(0x28);
    t[a::RELOAD_LEFT] = Variant(0x24);
    t
};

/// What a zero perk multiplier makes the rate.
const INSTANT_RATE: f32 = 1000.0;
/// The share of the fast reload multiplier each perk level takes.
const RELOAD_LEVEL_SHARE: f32 = 0.333_333_34;

fn i32_at(bytes: &[u8], off: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

/// Black Ops' client's rate for an animation it starts: the animation's
/// length over the weapon's time for it (0 for a time of 0, 1 with none),
/// sped by the fast reload perk while either hand reloads (by its level
/// share of the reload multiplier) or by the rate of fire perk while the
/// right hand rechambers. While the weapon mantles its mantle animations
/// run over the mantle's own length.
fn rate(start: &ViewmodelStart<'_>) -> f32 {
    let ps = player(start.game);
    let time = match TIMES.get(start.anim).copied().unwrap_or(Time::Own) {
        _ if matches!(start.anim, a::MANTLE | a::MANTLE_CAMERA)
            && movement_t5::weapon::mantling_anim(&ps) =>
        {
            Some(ps.mantle_length)
        }
        Time::Def(off) => i32_at(start.def, off),
        Time::Variant(off) => i32_at(start.variant, off),
        Time::Own => None,
    };
    let rate = match time {
        None => 1.0,
        Some(0) => 0.0,
        Some(time) => start.length_ms as f32 / time as f32,
    };
    let reloading = |s: i32| (ws::RELOADING..=ws::RELOADING_LEFT_INTERRUPT).contains(&s);
    if (reloading(ps.weaponstate) || reloading(ps.weaponstate_left))
        && ps.perks & perks::FAST_RELOAD != 0
    {
        let mult = tuning::PERK_WEAP_RELOAD_MULTIPLIER;
        if mult == 0.0 {
            return INSTANT_RATE;
        }
        // Black Ops runs this in the x87's extended precision.
        let share = f64::from((ps.perks >> 6) & 3) * f64::from(RELOAD_LEVEL_SHARE);
        let rate = f64::from(rate);
        return (rate * (1.0 - share) + rate / f64::from(mult) * share) as f32;
    }
    if matches!(ps.weaponstate, ws::RECHAMBERING | ws::RECHAMBER_END)
        && ps.perks & perks::RATE_OF_FIRE != 0
    {
        let mult = tuning::PERK_WEAP_RATE_MULTIPLIER;
        return if mult == 0.0 {
            INSTANT_RATE
        } else {
            rate / mult
        };
    }
    rate
}

/// Whether a script hid the viewmodel (`hideviewmodel`).
fn hidden(game: &[u8; GAME_MOVE_BYTES]) -> bool {
    crate::movement::viewmodel_hidden(game)
}

/// Black Ops' client on a change of a dual-wield weapon's left-hand number:
/// the left idle (never its empty idle) once the left animations have
/// finished, at once on a quick idle; the left fire, last shot and reloads;
/// any other number leaves what plays.
fn left(
    weap_anim: u32,
    _named: &dyn Fn(usize) -> bool,
) -> (ViewmodelAnim, Option<&'static Unknown>) {
    use ViewmodelAnim::{Idle, IdleNow, Keep, Play};
    (
        match weap_anim & !w::TOGGLE {
            w::IDLE => Idle,
            w::QUICK_IDLE => IdleNow,
            w::FIRE => Play(a::FIRE_LEFT),
            w::LAST_SHOT => Play(a::LASTSHOT_LEFT),
            w::RELOAD => Play(a::RELOAD_LEFT),
            w::RELOAD_EMPTY => Play(a::RELOAD_EMPTY_LEFT),
            _ => Keep,
        },
        None,
    )
}

fn player(game: &[u8; GAME_MOVE_BYTES]) -> movement_t5::PlayerState {
    let mut player = MovePlayer {
        game: *game,
        ..MovePlayer::default()
    };
    crate::movement::unpack(&mut player).0
}

/// Black Ops' client on the mantle number: the weapon's mantle animation
/// for a mantle higher than `mantle_weapon_anim_height`, else idle at once;
/// every other number as [`anim`].
fn pick(weap_anim: u32, pick: &ViewmodelPick<'_>) -> (ViewmodelAnim, Option<&'static Unknown>) {
    if weap_anim & !w::TOGGLE != w::MANTLE {
        return anim(weap_anim, pick.named);
    }
    let tall = movement_t5::mantle::tall_enough_for_weapon_anim(&player(pick.game));
    if (pick.named)(a::MANTLE) && tall {
        (ViewmodelAnim::Play(a::MANTLE), None)
    } else {
        (ViewmodelAnim::IdleNow, None)
    }
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
    let play = |anim: usize| (Play(anim), None);
    match weap_anim & !w::TOGGLE {
        w::IDLE => (Idle, None),
        w::QUICK_IDLE => (IdleNow, None),
        w::FIRE => play(a::FIRE),
        // A dual-wield weapon's right number may play the left gun's fire.
        FIRE_LEFT => play(a::FIRE_LEFT),
        LAST_SHOT_LEFT => play(a::LASTSHOT_LEFT),
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
        w::DTP_IN => (first(&[a::DTP_IN], IdleNow), None),
        w::DTP_LOOP => (first(&[a::DTP_LOOP], IdleNow), None),
        w::DTP_OUT => (first(&[a::DTP_OUT], IdleNow), None),
        w::DTP_EMPTY_IN => (first(&[a::DTP_EMPTY_IN, a::DTP_IN], IdleNow), None),
        w::DTP_EMPTY_LOOP => (first(&[a::DTP_EMPTY_LOOP, a::DTP_LOOP], IdleNow), None),
        w::DTP_EMPTY_OUT => (first(&[a::DTP_EMPTY_OUT, a::DTP_OUT], IdleNow), None),
        w::SLIDE_IN => (first(&[a::SLIDE_IN], IdleNow), None),
        _ => (IdleNow, None),
    }
}

/// Black Ops' client's camera animations: the sprint loop and the dive to
/// prone's start their own when the weapon names it, the sprint out stops
/// them; the mantle starts its own for a mantle higher than
/// `mantle_weapon_anim_height`.
fn camera(
    weap_anim: u32,
    named: &dyn Fn(usize) -> bool,
    game: &[u8; GAME_MOVE_BYTES],
) -> Option<CameraAnim> {
    let start = |anim: usize| named(anim).then_some(CameraAnim::Start(anim));
    match weap_anim & !w::TOGGLE {
        w::SPRINT_LOOP | w::SPRINT_EMPTY_LOOP => start(a::SPRINT_CAMERA),
        w::SPRINT_OUT | w::SPRINT_EMPTY_OUT => named(a::SPRINT_CAMERA).then_some(CameraAnim::Stop),
        w::DTP_IN | w::DTP_EMPTY_IN => start(a::DTP_IN_CAMERA),
        w::DTP_LOOP | w::DTP_EMPTY_LOOP => start(a::DTP_LOOP_CAMERA),
        w::DTP_OUT | w::DTP_EMPTY_OUT => start(a::DTP_OUT_CAMERA),
        w::MANTLE if movement_t5::mantle::tall_enough_for_weapon_anim(&player(game)) => {
            start(a::MANTLE_CAMERA)
        }
        _ => None,
    }
}
