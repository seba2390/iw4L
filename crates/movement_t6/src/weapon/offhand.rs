//! Offhands (grenades and equipment thrown with the weapon away or none in
//! hand) and the riot shield's planting.

use super::{
    def, finish, fire::grenade_fire_held, fire::plant_check, hold_prone, offhand_class,
    offhand_slot, some, start_anim, start_anim_both, start_anim_left, weap_anim, weapon_type,
};
use crate::events::{anim, event};
use crate::inventory as inv;
use crate::math;
use crate::state::{
    ENTITYNUM_NONE, ENTITYNUM_WORLD, buttons, e_flags, e_flags2, pm_flags, weap_flags,
    weapon_state as ws,
};
use crate::{MoveWorld, Pm, Pml, tuning};

/// What a riot shield is set down on and must have room in.
const RIOTSHIELD_SPOT_MASK: u32 = 0x0281_8091;

/// The first held weapon for an offhand slot.
fn offhand_in_slot<W: MoveWorld>(pm: &Pm<'_, W>, slot: i32) -> u32 {
    pm.ps
        .held_weapons
        .iter()
        .map(|h| h.weapon)
        .find(|&w| some(w) && def(pm, w).def.offhand_slot() == slot)
        .unwrap_or(0)
}

/// The offhand to throw from a slot: its first held weapon, when it has
/// rounds, a detonator, or (lethal) a grenade to throw back.
fn find_offhand<W: MoveWorld>(pm: &Pm<'_, W>, slot: i32) -> u32 {
    if !(1..=3).contains(&slot) {
        return 0;
    }
    let w = offhand_in_slot(pm, slot);
    if !some(w) {
        return 0;
    }
    if inv::total(pm.ps, pm.world, w) < 1
        && !def(pm, w).def.has_detonator()
        && (slot != offhand_slot::LETHAL || pm.ps.throw_back_grenade_time_left < 1)
    {
        return 0;
    }
    w
}

/// No offhand to throw in that slot: the empty click, and a hint when the
/// player holds one without rounds.
fn offhand_empty<W: MoveWorld>(pm: &mut Pm<'_, W>, slot: i32) {
    pm.event(event::EMPTY_OFFHAND, 0);
    if some(offhand_in_slot(pm, slot)) {
        if slot == offhand_slot::LETHAL {
            pm.event(event::NO_FRAG_GRENADE_HINT, 0);
        } else if slot == offhand_slot::TACTICAL {
            pm.event(event::NO_SPECIAL_GRENADE_HINT, 0);
        }
    }
}

/// The throw starts: the weapon goes down first when one is held.
fn offhand_init<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.water_level >= 4 {
        return;
    }
    let offhand = pm.ps.offhand_index as i32;
    pm.event(event::WEAPON_SWITCH_STARTED, offhand);
    let ps = &mut *pm.ps;
    ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND_INIT;
    ps.weapon_delay = 0;
    if some(ps.weapon) {
        ps.weap_flags |= weap_flags::OFFHAND_SWITCH;
    }
    ps.throw_back_grenade_owner = ENTITYNUM_NONE;
    pm.reset_ads();
}

/// The offhand comes up: pulled back (or a detonator out of rounds set off)
/// unless it is a grenade thrown back.
pub(super) fn offhand_prepare<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let _ = pml;
    if pm.ps.water_level >= 4 || some(pm.ps.weapon) {
        return;
    }
    let offhand = pm.ps.offhand_index;
    let d = def(pm, offhand).def;
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::OFFHAND_PREPARE;
    ps.weap_flags = (ps.weap_flags | weap_flags::OFFHAND_ACTIVE) & !weap_flags::OFFHAND_SWITCH;
    ps.weapon_time = d.hold_fire_time();
    ps.weapon_delay = 0;
    if ps.throw_back_grenade_owner != ENTITYNUM_NONE {
        start_anim(ps, weap_anim::ALT_RAISE);
        hold_prone(ps);
        return;
    }
    if d.has_detonator() && inv::total(ps, pm.world, offhand) == 0 {
        ps.weaponstate = ws::OFFHAND_DETONATE;
        ps.weapon_time = d.detonate_time();
        ps.weapon_delay = d.detonate_delay();
        start_anim(ps, weap_anim::DETONATE);
        hold_prone(ps);
        return;
    }
    pm.event(event::PREP_OFFHAND, offhand as i32);
    start_anim_both(pm.ps, weap_anim::HOLD_FIRE);
    if !some(pm.ps.weapon) {
        let event = if d.offhand_class() == offhand_class::GEAR {
            anim::RAISE_WEAPON
        } else {
            anim::PRIME_GRENADE
        };
        pm.anim_event(event, true, true);
    }
    hold_prone(pm.ps);
}

/// The offhand is held: its fuse starts burning.
pub(super) fn offhand_hold<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let fuse = def(pm, pm.ps.offhand_index).def.fuse_time();
    let ps = &mut *pm.ps;
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weapon_time = 0;
    ps.weapon_delay = 0;
    ps.weaponstate = ws::OFFHAND_HOLD;
    if ps.throw_back_grenade_owner == ENTITYNUM_NONE {
        ps.grenade_time_left = fuse;
    }
}

/// Throws when the key is let go, the way the throw keys ask; a hold-use
/// grenade is put away instead.
pub(super) fn offhand_start<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let offhand = pm.ps.offhand_index;
    let d = def(pm, offhand).def;
    if d.weapon_type() == weapon_type::GRENADE && !plant_check(pm, offhand, false) {
        pm.event(event::CANNOTPLANT, offhand as i32);
        offhand_end(pm);
        return;
    }
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    let throw_key = |b: crate::Buttons| b.held(buttons::FRAG) || b.held(buttons::SMOKE);
    let side_keys = cmd.held(buttons::THROW_A)
        || cmd.held(buttons::THROW_B)
        || cmd.held(buttons::THROW_C)
        || cmd.held(buttons::THROW_D);
    if !d.hold_button_to_throw() && throw_key(old) && throw_key(cmd) {
        if d.offhand_hold_is_cancelable() && cmd.held(buttons::CANCEL) {
            offhand_end(pm);
            return;
        }
        if d.offhand_class() != offhand_class::SMOKE || !side_keys {
            pm.ps.weapon_delay = 1;
            return;
        }
    }
    if d.is_hold_use_grenade() {
        offhand_force_putaway(pm, pml);
        return;
    }
    let ps = &mut *pm.ps;
    ps.offhand_throw = 0x37;
    if cmd.held(buttons::THROW_A) {
        ps.offhand_throw = 0x31;
    }
    if cmd.held(buttons::THROW_B) {
        ps.offhand_throw = 0x32;
    }
    if cmd.held(buttons::THROW_C) {
        ps.offhand_throw = 0x34;
    }
    if cmd.held(buttons::THROW_D) {
        ps.offhand_throw = 0x33;
    }
    ps.weaponstate = ws::OFFHAND_START;
    ps.weapon_time = d.fire_time();
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weapon_delay = d.fire_delay();
    start_anim(ps, weap_anim::FIRE);
    pm.anim_event(anim::FIRE_WEAPON, false, true);
}

/// The offhand leaves the hand: thrown, or dropped in deep water.
pub(super) fn offhand_throw<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let offhand = pm.ps.offhand_index;
    if pm.ps.water_level > 3 {
        pm.event(event::GRENADE_DROP, offhand as i32);
        let ps = &mut *pm.ps;
        ps.weap_flags = (ps.weap_flags & !weap_flags::OFFHAND_ACTIVE) | weap_flags::SWIM_PUTAWAY;
        ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
        ps.weaponstate = ws::SWIM_IN;
        return;
    }
    if def(pm, offhand).def.weapon_type() == weapon_type::GRENADE
        && !plant_check(pm, offhand, false)
    {
        pm.event(event::CANNOTPLANT, offhand as i32);
        offhand_end(pm);
        return;
    }
    pm.event(event::USE_OFFHAND, offhand as i32);
    if pm.ps.throw_back_grenade_owner == ENTITYNUM_NONE {
        if inv::total(pm.ps, pm.world, offhand) == 0 {
            pm.event(event::EMPTY_OFFHAND, 0);
        } else {
            inv::use_clip(pm.ps, pm.world, offhand, 1);
        }
    }
    let ps = &mut *pm.ps;
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND;
}

/// The throw is over: the weapon comes back quickly.
pub(super) fn offhand_end<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = pm.ps.weapon;
    if !some(weapon) {
        let ps = &mut *pm.ps;
        ps.weap_flags |= weap_flags::OFFHAND_EMPTY_HANDED;
        ps.weapon_time = 0;
        ps.weapon_delay = 1;
    } else {
        let d = def(pm, weapon).def;
        let empty = inv::hand_clip_empty(pm.ps, pm.world);
        let dual = super::dual_wield(pm, weapon);
        let ps = &mut *pm.ps;
        ps.weapon_time = d.quick_raise_time();
        ps.weapon_delay = 0;
        start_anim(
            ps,
            if empty {
                weap_anim::EMPTY_RAISE
            } else {
                weap_anim::QUICK_RAISE
            },
        );
        if dual {
            start_anim_left(ps, weap_anim::QUICK_RAISE);
        }
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !(weap_flags::OFFHAND_ACTIVE | weap_flags::OFFHAND_SWITCH);
    ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    ps.throw_back_grenade_time_left = 0;
    ps.throw_back_grenade_owner = ENTITYNUM_NONE;
    ps.weaponstate = ws::OFFHAND_END;
}

/// A hold-use grenade (or a forced end) puts the offhand away at once.
pub(super) fn offhand_force_putaway<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let _ = pml;
    let t = def(pm, pm.ps.offhand_index).def.quick_drop_time();
    let ps = &mut *pm.ps;
    ps.weapon_time = t;
    ps.weapon_delay = 0;
    start_anim(ps, weap_anim::DROP);
    pm.anim_event(anim::DROP_WEAPON, false, true);
    let ps = &mut *pm.ps;
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND;
}

/// A cooking grenade burns down; held too long it goes off in hand. True
/// when it did.
pub(super) fn grenade_cook<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    let active = ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0;
    let weapon = if active { ps.offhand_index } else { ps.weapon };
    if !active && !some(weapon) {
        return false;
    }
    let d = def(pm, weapon).def;
    if !matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        || pm.ps.grenade_time_left <= 0
    {
        return false;
    }
    if d.cook_off_hold() {
        pm.ps.grenade_time_left -= pml.msec;
    }
    let ps = &*pm.ps;
    if ps.water_level >= 4 && active && ps.weaponstate == ws::OFFHAND_HOLD {
        pm.ps.weaponstate = ws::OFFHAND_START;
        inv::use_clip(pm.ps, pm.world, weapon, 1);
        return false;
    }
    if ps.grenade_time_left >= 1 {
        return false;
    }
    pm.ps.grenade_time_left = -1;
    let offhand = pm.ps.offhand_index as i32;
    pm.event(event::GRENADE_SUICIDE, offhand);
    let ps = &mut *pm.ps;
    finish(ps);
    start_anim(ps, weap_anim::QUICK_IDLE);
    ps.throw_back_grenade_time_left = 0;
    ps.throw_back_grenade_owner = ENTITYNUM_NONE;
    if inv::held(ps, weapon).is_some() {
        inv::use_clip(ps, pm.world, weapon, 1);
    }
    true
}

/// A held throw is let go of or cancelled before it leaves the hand.
pub(super) fn check_offhand_cancel<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let cmd = pm.cmd.buttons;
    let s = pm.ps.weaponstate;
    if s == ws::OFFHAND_PREPARE {
        let d = def(pm, pm.ps.offhand_index).def;
        if d.hold_button_to_throw() {
            if cmd.held(buttons::FRAG) || cmd.held(buttons::SMOKE) {
                return;
            }
        } else if !d.offhand_hold_is_cancelable() || !cmd.held(buttons::CANCEL) {
            return;
        }
        offhand_end(pm);
    } else if s == ws::OFFHAND_INIT
        && def(pm, pm.ps.offhand_index)
            .def
            .offhand_hold_is_cancelable()
    {
        if cmd.held(buttons::CANCEL) {
            offhand_end(pm);
        }
        return;
    }
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let weapon = if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s) {
        ps.offhand_index
    } else {
        ps.weapon
    };
    let d = def(pm, weapon).def;
    let cancelable = d.offhand_hold_is_cancelable();
    let busy = ws::is_firing(s) || ws::is_melee(s);
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        && (cancelable || (some(ps.weapon) && busy && d.hold_button_to_throw()))
        && !cmd.held(buttons::ATTACK)
        && (!cancelable || cmd.held(buttons::CANCEL))
        && d.offhand_slot() != offhand_slot::EQUIPMENT
    {
        let ps = &mut *pm.ps;
        finish(ps);
        start_anim(ps, weap_anim::QUICK_IDLE);
    }
}

/// The frag or smoke key throws that slot's offhand.
pub(super) fn check_offhand<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let _ = pml;
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.water_level >= 4
        || ps.e_flags & e_flags::TURRET != 0
        || ps.weap_flags & (weap_flags::WEAPONS_DISABLED | weap_flags::OFFHANDS_DISABLED) != 0
        || super::in_vehicle(ps)
        || ps.pm_flags & pm_flags::SPRINTING != 0
        || grenade_fire_held(pm)
        || matches!(s, ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT)
        || ((ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s) && s != ws::OFFHAND_END)
        || ws::is_dtp(s)
        || ws::is_lowready(s)
        || ws::is_riotshield(s)
        || ps.e_flags2 & e_flags2::REMOTE_CONTROL != 0
        || ps.link_flags & 4 != 0
        || ps.weap_flags & weap_flags::FIRING_GUIDED != 0
    {
        return;
    }
    let asked = pm.cmd.offhand_index;
    if inv::held(pm.ps, asked).is_some() {
        pm.ps.offhand_index = asked;
    }
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    let slot = if cmd.held(buttons::FRAG) && !old.held(buttons::FRAG) {
        offhand_slot::LETHAL
    } else if cmd.held(buttons::SMOKE) && !old.held(buttons::SMOKE) {
        offhand_slot::TACTICAL
    } else {
        return;
    };
    let w = find_offhand(pm, slot);
    if !some(w) {
        offhand_empty(pm, slot);
        return;
    }
    let d = def(pm, w).def;
    if d.is_tactical_insertion() {
        pm.ps.weap_flags &= !weap_flags::PLACEMENT_BLOCKED;
    }
    if d.weapon_type() == weapon_type::GRENADE && !plant_check(pm, w, true) {
        pm.event(event::CANNOTPLANT, w as i32);
        return;
    }
    pm.event(event::SWITCH_OFFHAND, w as i32);
    pm.ps.offhand_index = w;
    let ps = &*pm.ps;
    if ps.cursor_hint_ent_index == ENTITYNUM_NONE
        && (!some(ps.weapon) || ps.weaponstate == ws::OFFHAND_END)
    {
        offhand_prepare(pm, pml);
        return;
    }
    offhand_init(pm);
}

/// Attack with a riot shield in hand starts planting it.
pub(super) fn check_riotshield<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if d.weapon_type() != weapon_type::RIOTSHIELD
        || ps.water_level >= 4
        || ps.e_flags & e_flags::TURRET != 0
        || ps.weap_flags & (weap_flags::WEAPONS_DISABLED | weap_flags::OFFHANDS_DISABLED) != 0
        || super::in_vehicle(ps)
        || ps.e_flags & e_flags::VEHICLE_VIEW != 0
        || ps.pm_flags & pm_flags::SPRINTING != 0
        || grenade_fire_held(pm)
        || matches!(s, ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT)
        || (ws::is_riotshield(s) && s != ws::RIOTSHIELD_END)
        || ws::is_dtp(s)
        || ws::is_lowready(s)
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ps.e_flags2 & e_flags2::REMOTE_CONTROL != 0
        || ps.weap_flags & weap_flags::FIRING_GUIDED != 0
        || !pm.cmd.buttons.held(buttons::ATTACK)
        || pm.oldcmd.buttons.held(buttons::ATTACK)
    {
        return;
    }
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::RIOTSHIELD_PREPARE;
    ps.weapon_time = d.hold_fire_time();
    ps.weapon_delay = 0;
    pm.event(event::PREP_RIOTSHIELD, weapon as i32);
    start_anim_both(pm.ps, weap_anim::HOLD_FIRE);
    hold_prone(pm.ps);
}

/// The planted shield's spot ahead and below the eye: on the world, level
/// enough and near the player's height.
fn riotshield_spot<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    let yaw = ps.viewangles[1];
    let mut eye = [
        ps.origin[0],
        ps.origin[1],
        ps.view_height_current + ps.origin[2],
    ];
    if ps.e_flags & e_flags::VEHICLE_VIEW == 0 && ps.leanf != 0.0 {
        let lean = (2.0 - ps.leanf.abs()) * ps.leanf;
        let (_, right, _) = math::angle_vectors([0.0, yaw, lean * 16.0]);
        let along = lean * 20.0;
        for (e, r) in eye.iter_mut().zip(right) {
            *e += r * along;
        }
    }
    eye[2] += tuning::STANDING_VIEW_HEIGHT as f32 - ps.view_height_current;
    let (forward, _, _) = math::angle_vectors([20.0, yaw, 0.0]);
    let start = [
        forward[0] * 20.0 + eye[0],
        forward[1] * 20.0 + eye[1],
        forward[2] * 20.0 + eye[2],
    ];
    let end = [start[0], start[1], start[2] - (eye[2] - ps.origin[2])];
    let t = pm.trace_plain(start, end, [0.0; 3], [0.0; 3], RIOTSHIELD_SPOT_MASK);
    if 1.0 <= t.fraction {
        return false;
    }
    let t = pm.trace_plain(start, end, [-50.0; 3], [50.0; 3], RIOTSHIELD_SPOT_MASK);
    let z = (end[2] - start[2]) * t.fraction + start[2] - 50.0;
    if t.allsolid || t.startsolid || t.fraction < 0.0001 || t.normal[2] < 0.7 {
        return false;
    }
    if t.entity != ENTITYNUM_WORLD {
        return false;
    }
    let n = t.normal;
    let f = forward;
    let right = [
        n[1] * f[2] - n[2] * f[1],
        n[2] * f[0] - n[0] * f[2],
        n[0] * f[1] - n[1] * f[0],
    ];
    let ahead = [
        right[1] * n[2] - right[2] * n[1],
        right[2] * n[0] - right[0] * n[2],
        right[0] * n[1] - right[1] * n[0],
    ];
    let (pitch, roll) = axis_pitch_roll(ahead, right);
    pitch.abs() <= tuning::RIOTSHIELD_DEPLOY_PITCH_MAX
        && roll.abs() <= tuning::RIOTSHIELD_DEPLOY_ROLL_MAX
        && (pm.ps.origin[2] - z).abs() <= tuning::RIOTSHIELD_DEPLOY_ZDIFF_MAX
}

/// The pitch and roll (each within -180..180) of an axis from its forward
/// and right vectors, the way the game turns an axis into angles.
fn axis_pitch_roll(forward: [f32; 3], right: [f32; 3]) -> (f32, f32) {
    let (pitch, yaw) = if forward[1] == 0.0 && forward[0] == 0.0 {
        (if -forward[2] < 0.0 { 270.0 } else { 90.0 }, 0.0)
    } else {
        let yaw =
            libm::atan2(f64::from(forward[1]), f64::from(forward[0])) as f32 * 180.0 * 0.31830987;
        let xy = libm::sqrtf(forward[0] * forward[0] + forward[1] * forward[1]);
        let pitch = libm::atan2(f64::from(forward[2]), f64::from(xy)) as f32 * -180.0 * 0.31830987;
        (
            pitch + if pitch < 0.0 { 360.0 } else { 0.0 },
            yaw + if yaw < 0.0 { 360.0 } else { 0.0 },
        )
    };
    let (sy, cy) = sincos(-yaw);
    let side = right[0] * sy + right[1] * cy;
    let ahead = right[0] * cy - right[1] * sy;
    let (sp, cp) = sincos(-pitch);
    let v = [right[2] * sp + ahead * cp, side, right[2] * cp - ahead * sp];
    let tilt = if v[1] == 0.0 && v[0] == 0.0 {
        if 0.0 <= -v[2] { 90.0 } else { -90.0 }
    } else {
        let xy = libm::sqrtf(v[0] * v[0] + v[1] * v[1]);
        libm::atan2(f64::from(v[2]), f64::from(xy)) as f32 * -180.0 * 0.31830987
    };
    let roll = if 0.0 <= side {
        -tilt
    } else if tilt < 0.0 {
        tilt + 180.0
    } else {
        tilt - 180.0
    };
    (
        math::angle_normalize_180(pitch),
        math::angle_normalize_180(roll),
    )
}

fn sincos(degrees: f32) -> (f32, f32) {
    let r = f64::from(degrees * 0.017453292);
    (libm::sin(r) as f32, libm::cos(r) as f32)
}

/// While attack is held the shield's spot is tested each frame; let go, it
/// is planted on a good spot or lowered.
pub(super) fn riotshield_place<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    if d.weapon_type() != weapon_type::RIOTSHIELD || !some(weapon) {
        lower_riotshield(pm, d.reload_time());
        return;
    }
    let good = riotshield_spot(pm);
    if let Some(h) = inv::held_mut(pm.ps, weapon)
        && !good
    {
        h.riotshield = true;
    }
    if good {
        pm.ps.weap_flags &= !weap_flags::PLACEMENT_BLOCKED;
    } else {
        pm.ps.weap_flags |= weap_flags::PLACEMENT_BLOCKED;
    }
    if pm.cmd.buttons.held(buttons::ATTACK) && pm.oldcmd.buttons.held(buttons::ATTACK) {
        pm.ps.weapon_delay = 1;
        return;
    }
    let _ = pml;
    if !good {
        lower_riotshield(pm, d.reload_time());
        return;
    }
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::RIOTSHIELD_DEPLOY;
    ps.weapon_time = d.fire_time();
    ps.weapon_delay = d.fire_delay();
    start_anim(ps, weap_anim::FIRE);
    pm.anim_event(anim::FIRE_WEAPON, false, true);
}

fn lower_riotshield<W: MoveWorld>(pm: &mut Pm<'_, W>, time: i32) {
    let weapon = pm.ps.weapon;
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::RIOTSHIELD_END;
    ps.weapon_time = time;
    ps.weapon_delay = 0;
    start_anim(ps, weap_anim::RELOAD);
    pm.event(event::LOWER_RIOTSHIELD, weapon as i32);
}
