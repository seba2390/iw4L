//! Weapon poses: sprint, dive to prone, slide, crawl, deploy, continuous
//! fire, the mantle and swim put-aways, night vision and detonators.

use super::{
    def, finish, fire, start_anim, start_anim_both, start_anim_unless_playing, view_weapon,
    weap_anim, weapon_grants, weapon_type,
};
use crate::events::event;
use crate::inventory as inv;
use crate::state::{
    ENTITYNUM_NONE, PlayerState, buttons, pm_flags, weap_flags, weapon_state as ws,
};
use crate::{MoveWorld, Pm, perks, tuning};

/// Mantling with the mantle viewmodel animation up.
pub(super) fn mantling_anim(ps: &PlayerState) -> bool {
    ps.pm_flags & pm_flags::MANTLE != 0
        && (!tuning::MANTLE_ENABLE || ps.mantle_flags & mantle_flags::NO_WEAPON != 0)
}

/// Mantling with the weapon kept: it is put away.
pub(super) fn mantle_puts_away(ps: &PlayerState) -> bool {
    tuning::MANTLE_ENABLE
        && ps.pm_flags & pm_flags::MANTLE != 0
        && ps.mantle_flags & mantle_flags::NO_WEAPON == 0
}

/// Black Ops 2's mantle state flags.
pub mod mantle_flags {
    /// The weapon is down while mantling.
    pub const NO_WEAPON: u32 = 0x40;
}

/// The hand states that fire or swing.
fn firing_or_melee(s: i32) -> bool {
    ws::is_firing(s) || ws::is_melee(s)
}

/// Sets a pose state with its time and no delay.
fn set_pose(ps: &mut PlayerState, state: i32, time: i32) {
    ps.weaponstate = state;
    ps.weapon_time = time;
    ps.weapon_delay = 0;
}

/// A pose's animation on both hands, its empty-clip version when the clip
/// is empty.
fn pose_anim<W: MoveWorld>(pm: &mut Pm<'_, W>, anim: u32, empty_anim: u32) {
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    start_anim_both(pm.ps, if empty { empty_anim } else { anim });
}

/// Sprinting raises the sprint pose; stopping drops it.
pub(super) fn check_sprint<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.water_level >= 4
        || !super::some(pm.cmd.weapon)
        || firing_or_melee(s)
        || firing_or_melee(ps.weaponstate_left)
        || ws::is_raising_or_dropping(s)
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_dtp(s)
        || ws::is_riotshield(s)
        || s == ws::DETONATING
        || ps.pm_flags & pm_flags::DIVING != 0
    {
        return;
    }
    let sprinting = ps.pm_flags & pm_flags::SPRINTING != 0;
    if sprinting && !ws::is_sprint(s) {
        let t = def(pm, pm.ps.weapon).def.sprint_in_time();
        set_pose(pm.ps, ws::SPRINT_RAISE, t);
        pose_anim(pm, weap_anim::SPRINT_IN, weap_anim::SPRINT_EMPTY_IN);
    } else if !sprinting && matches!(s, ws::SPRINT_RAISE | ws::SPRINT_LOOP) {
        sprint_drop(pm);
    }
}

pub(super) fn sprint_loop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    set_pose(pm.ps, ws::SPRINT_LOOP, 0);
    pose_anim(pm, weap_anim::SPRINT_LOOP, weap_anim::SPRINT_EMPTY_LOOP);
}

/// The sprint pose drops, faster with sprint recovery.
fn sprint_drop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let mut t = def(pm, pm.ps.weapon).def.sprint_out_time();
    if perks::has(pm.ps.perks, perks::SPRINT_RECOVERY)
        || weapon_grants(pm, view_weapon(pm), perks::SPRINT_RECOVERY)
    {
        t = (tuning::PERK_SPRINT_RECOVERY_MULTIPLIER * t as f32) as i32;
    }
    set_pose(pm.ps, ws::SPRINT_DROP, t);
    pose_anim(pm, weap_anim::SPRINT_OUT, weap_anim::SPRINT_EMPTY_OUT);
}

/// Diving to prone and not yet sliding along the ground.
pub(super) fn diving(ps: &PlayerState) -> bool {
    tuning::DTP && ps.pm_flags & pm_flags::DIVING != 0 && ps.jump_time < 0
}

/// A dive-to-prone pose on both hands, unless that animation already plays.
fn dtp_pose<W: MoveWorld>(pm: &mut Pm<'_, W>, state: i32, time: i32, anim: u32, empty_anim: u32) {
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let ps = &mut *pm.ps;
    ps.weaponstate = state;
    ps.weaponstate_left = state;
    ps.weapon_time = time;
    ps.weapon_delay = 0;
    let a = if empty { empty_anim } else { anim };
    start_anim_unless_playing(ps, a, false);
    start_anim_unless_playing(ps, a, true);
}

pub(super) fn check_dtp<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.water_level < 4
        && super::some(pm.cmd.weapon)
        && !firing_or_melee(s)
        && !ws::is_raising_or_dropping(s)
        && !(ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        && !ws::is_riotshield(s)
        && !ws::is_dtp(s)
        && ps.pm_flags & pm_flags::DIVING != 0
        && !diving(ps)
    {
        let t = def(pm, pm.ps.weapon).def.dtp_in_time();
        dtp_pose(
            pm,
            ws::DTP_IN,
            t,
            weap_anim::DTP_IN,
            weap_anim::DTP_EMPTY_IN,
        );
    }
}

pub(super) fn dtp_loop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    dtp_pose(
        pm,
        ws::DTP_LOOP,
        0,
        weap_anim::DTP_LOOP,
        weap_anim::DTP_EMPTY_LOOP,
    );
}

pub(super) fn dtp_out<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.dtp_out_time();
    dtp_pose(
        pm,
        ws::DTP_OUT,
        t,
        weap_anim::DTP_OUT,
        weap_anim::DTP_EMPTY_OUT,
    );
}

/// A slide that starts this frame plays the slide pose.
pub(super) fn check_slide<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.water_level < 4
        && super::some(pm.cmd.weapon)
        && !firing_or_melee(s)
        && !ws::is_raising_or_dropping(s)
        && !(ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        && !ws::is_riotshield(s)
        && !ws::is_dtp(s)
        && s != ws::SLIDE_IN
        && ps.pm_flags & pm_flags::SLIDING != 0
        && ps.slick_start_time == pm.cmd.server_time
    {
        let t = def(pm, pm.ps.weapon).def.slide_in_time();
        let ps = &mut *pm.ps;
        set_pose(ps, ws::SLIDE_IN, t);
        start_anim_unless_playing(ps, weap_anim::SLIDE_IN, false);
    }
}

/// A mountable weapon deploys on the ground with rounds, or breaks down.
pub(super) fn check_deploy<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    let s = pm.ps.weaponstate;
    if !d.mountable_weapon()
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_raising_or_dropping(s)
        || ws::is_dtp(s)
        || ws::is_riotshield(s)
    {
        return;
    }
    let clip = inv::clip(pm.ps, pm.world, weapon);
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::DEPLOY == 0 || ws::is_reloading(s) || clip < 1 {
        if ps.weap_flags & weap_flags::BREAKDOWN != 0 {
            ps.weap_flags &= !weap_flags::BREAKDOWN;
            set_pose(ps, ws::BREAKING_DOWN, d.breakdown_time());
            start_anim(ps, weap_anim::BREAKDOWN);
        }
        return;
    }
    ps.weap_flags &= !weap_flags::DEPLOY;
    if ps.ground_entity_num != ENTITYNUM_NONE && 0.0 <= ps.velocity[2] {
        set_pose(ps, ws::DEPLOYING, d.deploy_time());
        start_anim(ps, weap_anim::DEPLOY);
    }
}

/// While mantling the weapon goes down; after, it comes back.
pub(super) fn check_mantle_anim<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    if !mantling_anim(ps) {
        if ps.pm_flags & pm_flags::MANTLE == 0
            && ps.weap_anim & !weap_anim::TOGGLE == weap_anim::MANTLE
        {
            finish(ps);
        }
        return;
    }
    if ps.weaponstate == ws::SPRINT_DROP {
        ps.weapon_time = 0;
    }
    start_anim_unless_playing(ps, weap_anim::MANTLE, false);
}

/// A continuous-fire weapon winds up while fire is held and down after.
pub(super) fn check_cont_fire<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if !super::some(pm.cmd.weapon)
        || !d.continuous_fire()
        || ps.water_level >= 4
        || matches!(s, ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END)
        || ws::is_melee(s)
        || ws::is_raising_or_dropping(s)
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_riotshield(s)
        || ws::is_dtp(s)
        || s == ws::DETONATING
    {
        return;
    }
    let left = pm.ps.left_hand;
    let held = fire::fire_held(pm, delay_done, true, left);
    if !super::overheated(pm, weapon) && held {
        let ps = &mut *pm.ps;
        if !ws::is_cont_fire(ps.weaponstate) {
            set_pose(ps, ws::CONT_FIRE_IN, d.cont_fire_in_time());
            start_anim(ps, weap_anim::CONT_FIRE_IN);
        }
    } else if matches!(pm.ps.weaponstate, ws::CONT_FIRE_IN | ws::CONT_FIRE_LOOP) {
        let ps = &mut *pm.ps;
        set_pose(ps, ws::CONT_FIRE_OUT, d.cont_fire_out_time());
        start_anim(ps, weap_anim::CONT_FIRE_OUT);
    }
}

pub(super) fn cont_fire_loop(ps: &mut PlayerState) {
    set_pose(ps, ws::CONT_FIRE_LOOP, 0);
    start_anim(ps, weap_anim::CONT_FIRE_LOOP);
}

/// Crawling prone with the weapon down; firing or standing up raises it.
fn crawl_in<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.crawl_in_time();
    set_pose(pm.ps, ws::CRAWL_IN, t);
    pose_anim(pm, weap_anim::CRAWL_IN, weap_anim::CRAWL_EMPTY_IN);
}

fn crawl_out<W: MoveWorld>(pm: &mut Pm<'_, W>, to_fire: bool) {
    let d = def(pm, pm.ps.weapon).def;
    let t = if to_fire {
        d.crawl_out_fire_time()
    } else {
        d.crawl_out_time()
    };
    set_pose(pm.ps, ws::CRAWL_OUT, t);
    pose_anim(pm, weap_anim::CRAWL_OUT, weap_anim::CRAWL_EMPTY_OUT);
}

/// Fire held with a shot to fire: the crawl ends for it.
fn crawl_fires<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) -> bool {
    let left = pm.ps.left_hand;
    if fire::fire_held(pm, delay_done, true, left) && !fire::burst_complete(pm, pm.ps.weapon) {
        crawl_out(pm, true);
        return true;
    }
    false
}

/// The crawl animation follows the stick: forward, back, or a side when it
/// leans more than twice as far that way.
pub(super) fn crawl_direction<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    if crawl_fires(pm, delay_done) {
        return;
    }
    let fwd = i32::from(pm.cmd.forwardmove);
    let right = i32::from(pm.cmd.rightmove);
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let (state, anim) = if fwd * fwd * 4 <= right * right {
        if right < 1 {
            (ws::CRAWL_LEFT, weap_anim::CRAWL_LEFT)
        } else {
            (ws::CRAWL_RIGHT, weap_anim::CRAWL_RIGHT)
        }
    } else if fwd < 1 {
        (ws::CRAWL_BACK, weap_anim::CRAWL_BACK)
    } else {
        (ws::CRAWL_FORWARD, weap_anim::CRAWL_FORWARD)
    };
    let anim = if empty {
        anim + (weap_anim::CRAWL_EMPTY_IN - weap_anim::CRAWL_IN)
    } else {
        anim
    };
    let ps = &mut *pm.ps;
    if ps.weaponstate != state {
        set_pose(ps, state, 0);
        start_anim_both(ps, anim);
    }
}

pub(super) fn crawl_loop<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    if crawl_fires(pm, delay_done) {
        return;
    }
    crawl_direction(pm, delay_done);
}

/// Dive to prone's post-move pause still holds the weapon down.
fn dtp_pause_over<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    !(tuning::DTP
        && ps.pm_flags & pm_flags::DIVING == 0
        && ps.dive_end_time != 0
        && ((pm.cmd.server_time - ps.dive_end_time) as f32) < tuning::DTP_POST_MOVE_PAUSE)
}

/// Prone and moving with nothing in hand busy: the crawl pose; a fire press
/// or standing ends it.
pub(super) fn check_prone_crawl<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    if ps.left_hand {
        return;
    }
    let moving = pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0;
    let s = ps.weaponstate;
    if ps.water_level > 3
        || !super::some(pm.cmd.weapon)
        || firing_or_melee(s)
        || firing_or_melee(ps.weaponstate_left)
        || ws::is_reloading(s)
        || ws::is_raising_or_dropping(s)
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_riotshield(s)
        || ws::is_dtp(s)
        || ws::is_sprint(s)
        || s == ws::DETONATING
        || ps.pm_flags & pm_flags::DIVING != 0
    {
        return;
    }
    let weapon = ps.weapon;
    let right_done = fire::burst_complete(pm, weapon);
    let right_fire = fire::fire_held(pm, false, true, false);
    let (mut left_fire, mut left_done) = (false, false);
    let dual = super::dual_wield(pm, weapon);
    if dual {
        pm.ps.left_hand = true;
        left_fire = fire::fire_held(pm, false, true, true);
        left_done = fire::burst_complete(pm, weapon);
        pm.ps.left_hand = false;
    }
    let s = pm.ps.weaponstate;
    if ((right_fire && !right_done) || (left_fire && !left_done))
        && (ws::CRAWL_IN..=ws::CRAWL_LEFT).contains(&s)
    {
        crawl_out(pm, true);
        return;
    }
    let prone = pm.ps.pm_flags & pm_flags::PRONE != 0;
    if !prone && ws::is_crawl_loop(s) {
        crawl_out(pm, false);
        return;
    }
    if right_fire && (!right_done || ws::is_crawl(s)) {
        return;
    }
    if dual && left_fire && (!left_done || ws::is_crawl(s)) {
        return;
    }
    if prone {
        if moving {
            if !ws::is_crawl(s) {
                crawl_in(pm);
            }
            return;
        }
        if !dtp_pause_over(pm) {
            return;
        }
    }
    if (ws::CRAWL_IN..=ws::CRAWL_LEFT).contains(&pm.ps.weaponstate) {
        crawl_out(pm, false);
    }
}

/// The night vision key toggles the goggles; the weapon stays up.
pub(super) fn check_nightvision<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.oldcmd.buttons.held(buttons::NIGHTVISION) || !pm.cmd.buttons.held(buttons::NIGHTVISION) {
        return;
    }
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::NIGHTVISION != 0 {
        ps.weap_flags &= !weap_flags::NIGHTVISION;
        pm.event(event::NIGHTVISION_REMOVE, 0);
    } else {
        ps.weap_flags |= weap_flags::NIGHTVISION;
        pm.event(event::NIGHTVISION_WEAR, 0);
    }
}

/// A detonator's fire key sets off what it placed.
pub(super) fn check_detonate<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = pm.ps.weapon;
    if !super::some(weapon) {
        return;
    }
    let d = def(pm, weapon).def;
    let s = pm.ps.weaponstate;
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        && d.has_detonator()
        && s != ws::DETONATING
        && !ws::is_reloading(s)
        && !firing_or_melee(s)
        && !ws::is_raising_or_dropping(s)
        && !(ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        && !ws::is_riotshield(s)
        && !ws::is_dtp(s)
        && pm.cmd.buttons.held(buttons::ATTACK)
    {
        let ps = &mut *pm.ps;
        ps.weaponstate = ws::DETONATING;
        ps.weapon_time = d.detonate_time();
        ps.weapon_delay = d.detonate_delay();
        start_anim(ps, weap_anim::DETONATE);
    }
}

/// A detonation's delay ran out: what was placed goes off.
pub(super) fn detonating<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    if !delay_done {
        finish(pm.ps);
        return;
    }
    let ps = &*pm.ps;
    let weapon = if ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0 {
        ps.offhand_index
    } else {
        ps.weapon
    };
    if super::some(weapon) {
        pm.event(event::DETONATE, 0);
    }
}

/// Swimming: the weapon goes away.
pub(super) fn swim_putaway<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.event(event::PUTAWAY_WEAPON, 0);
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::SWIM_PUTAWAY == 0 {
        start_anim_unless_playing(ps, weap_anim::DROP, false);
    } else {
        start_anim(ps, weap_anim::DROP);
    }
}

/// Out of deep water: the weapon comes back.
pub(super) fn swim_raise<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.ps.weaponstate = ws::RAISING;
    pm.event(event::RAISE_WEAPON, 0);
    start_anim(pm.ps, weap_anim::RAISE);
}
