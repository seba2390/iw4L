//! The sights, a minigun's and a jet gun's spin, held breath, the weapon's
//! idle sway time, the anti-quickscope penalty and the airburst sight flag.

use super::{Gap, aimed_weapon, def, fire_type, view_weapon, weapon_class, weapon_grants};
use crate::state::{buttons, e_flags, e_flags2, pm_flags, weap_flags, weapon_state as ws};
use crate::{MoveWorld, Pm, Pml, perks, tuning};

/// The sights come up or go down at the variant's rates.
pub(crate) fn ads_lerp<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = aimed_weapon(pm);
    let d = def(pm, weapon).def;
    let ps = &mut *pm.ps;
    if ps.e_flags2 & e_flags2::ADS_RESET != 0 && ps.weapon_pos_frac > 0.0 {
        pm.reset_ads();
        pm.ps.weapon_pos_frac = 0.0;
        pm.ps.ads_delay_time = 0;
        return;
    }
    if ps.link_flags & 4 != 0 {
        ps.weapon_pos_frac = if ps.pm_flags & pm_flags::ADS_INTENT != 0 {
            1.0
        } else {
            0.0
        };
        ps.ads_delay_time = 0;
        return;
    }
    if tuning::SCOPE_EXIT_ON_DAMAGE && ps.damage_count != 0 && d.ads_overlay_reticle() != 0 {
        pm.reset_ads();
        pm.ps.weapon_pos_frac = 0.0;
        pm.ps.ads_delay_time = 0;
        return;
    }
    if !d.aim_down_sight() {
        ps.weapon_pos_frac = 0.0;
        ps.ads_delay_time = 0;
        return;
    }
    let segmented = d.segmented_reload() || super::offscreen_reload(pm);
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let trans = d.ads_reload_trans_time();
    let past_trans = ps.weapon_time != trans && ps.weapon_time - trans >= 0;
    let reload_blocks = !d.reload_while_ads()
        && ((!segmented && s == ws::RELOADING && past_trans)
            || (segmented
                && ((ws::RELOADING..=ws::RELOAD_START_INTERRUPT).contains(&s)
                    || (s == ws::RELOAD_END && past_trans))));
    let rechamber_blocks =
        !d.rechamber_while_ads() && matches!(s, ws::RECHAMBERING | ws::RECHAMBER_END);
    let mut up = false;
    if !reload_blocks && !rechamber_blocks {
        if ps.e_flags & e_flags::VEHICLE_VIEW != 0 && (1..=4).contains(&ps.vehicle_pos) {
            pm.gap(Gap::MountedView);
        } else {
            up = ps.pm_flags & pm_flags::ADS_INTENT != 0;
        }
    }
    if d.ads_fire()
        && (ps.weapon_delay != 0
            || (d.fire_type() == fire_type::STACKED && ps.weap_flags & weap_flags::FIRE_LATCH != 0))
        && matches!(s, ws::FIRING | ws::CONT_FIRE_LOOP)
    {
        up = true;
    }
    let ps = &mut *pm.ps;
    let frac = ps.weapon_pos_frac;
    if frac != 1.0 || up || tuning::ADS_EXIT_DELAY < 1 {
        ps.ads_delay_time = 0;
        if !up {
            if frac == 0.0 {
                return;
            }
        } else if frac == 1.0 {
            return;
        }
    } else {
        if ps.ads_delay_time == 0 {
            ps.ads_delay_time = pm.cmd.server_time + tuning::ADS_EXIT_DELAY;
        }
        if ps.ads_delay_time <= pm.cmd.server_time {
            ps.ads_delay_time = 0;
            if frac == 0.0 {
                return;
            }
        } else {
            up = true;
            if frac == 1.0 {
                return;
            }
        }
    }
    let mut rate = if up {
        d.ads_in_rate()
    } else {
        -d.ads_out_rate()
    };
    if perks::has(ps.perks, perks::FAST_ADS) && !matches!(d.player_anim_type(), 3 | 17) {
        rate /= tuning::PERK_WEAP_ADS_MULTIPLIER;
    }
    if pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 {
        pm.gap(Gap::MountedView);
    }
    let ps = &mut *pm.ps;
    let next = pml.msec as f32 * rate + ps.weapon_pos_frac;
    let capped = if next - 1.0 < 0.0 { next } else { 1.0 };
    ps.weapon_pos_frac = if -next < 0.0 { capped } else { 0.0 };
}

/// A minigun spins up while fire or the sights are held; a jet gun spins
/// (toward -1) while fire is held and it has not overheated.
pub(super) fn spin<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = aimed_weapon(pm);
    let d = def(pm, weapon).def;
    let msec = pml.msec as f32;
    let attack = pm.cmd.buttons.held(buttons::ATTACK);
    match d.fire_type() {
        fire_type::MINIGUN => {
            let ps = &mut *pm.ps;
            let (time, dir) = if pm.cmd.buttons.held(buttons::ADS) || attack {
                ps.e_flags2 |= e_flags2::SPINNING;
                (d.spin_up_time(), 1.0)
            } else {
                ps.e_flags2 &= !e_flags2::SPINNING;
                (d.spin_down_time(), -1.0)
            };
            let next = msec * (dir / time as f32) + ps.weapon_spin_lerp;
            let capped = if next - 1.0 < 0.0 { next } else { 1.0 };
            ps.weapon_spin_lerp = if -next < 0.0 { capped } else { 0.0 };
        }
        fire_type::JETGUN => {
            let held = pm.ps.weapon;
            let spinning = attack && !super::overheated(pm, held);
            let ps = &mut *pm.ps;
            let rate = if ws::is_raising_or_dropping(ps.weaponstate) {
                ps.e_flags2 &= !e_flags2::SPINNING;
                0.0
            } else if 0.0 < d.spin_up_time() as f32 {
                if spinning {
                    ps.e_flags2 |= e_flags2::SPINNING;
                    1.0 / d.spin_up_time() as f32
                } else {
                    ps.e_flags2 &= !e_flags2::SPINNING;
                    -1.0 / d.spin_down_time() as f32
                }
            } else if spinning {
                ps.e_flags2 |= e_flags2::SPINNING;
                0.5
            } else {
                ps.e_flags2 &= !e_flags2::SPINNING;
                -0.5
            };
            let next = -msec * rate + ps.weapon_spin_lerp;
            ps.weapon_spin_lerp = if -1.0 - next < 0.0 {
                next.min(0.0)
            } else {
                -1.0
            };
        }
        _ => pm.ps.weapon_spin_lerp = 0.0,
    }
}

/// Steps the zoom level on a weapon with a variable-zoom attachment (`vzoom`).
pub(super) fn scope_zoom<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let w = def(pm, pm.ps.weapon);
    if !w
        .name
        .split('+')
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("vzoom"))
    {
        return;
    }
    let change_zoom = pm.cmd.buttons.held(buttons::CHANGE_ZOOM);
    let ps = &mut *pm.ps;
    if ps.ads_zoom_latched
        || pm.cmd.server_time - ps.ads_zoom_time < 41
        || !change_zoom
        || ps.weapon_pos_frac < 1.0
    {
        if pm.cmd.server_time - ps.ads_zoom_time > 40 && !change_zoom && ps.weapon_pos_frac >= 1.0 {
            ps.ads_zoom_latched = false;
            ps.ads_zoom_time = pm.cmd.server_time;
        }
    } else {
        ps.ads_prev_zoom_select = crate::inventory::held(ps, ps.weapon).map_or(0, |h| h.zoom);
        ps.ads_prev_zoom_select_time = pm.cmd.server_time;
        let weapon = ps.weapon;
        if let Some(held) = crate::inventory::held_mut(ps, weapon) {
            held.zoom = (held.zoom + 1) & 1;
            ps.ads_zoom_latched = true;
            ps.ads_zoom_time = pm.cmd.server_time;
            pm.event(crate::events::event::SCOPE_ZOOM, 0);
        }
    }
}

/// Moves `from` towards `to` at `rate` per second; snaps when close.
pub(super) fn lerp_towards(to: f32, from: f32, rate: f32, frametime: f32) -> f32 {
    let step = (to - from) * rate * frametime;
    let gap = (to - from).abs();
    if 0.001 < gap && step.abs() <= gap {
        step + from
    } else {
        to
    }
}

fn breath_times<W: MoveWorld>(pm: &Pm<'_, W>) -> (i32, i32) {
    let mut hold = (tuning::BREATH_HOLD_TIME * 1000.0) as i32;
    let gasp = (tuning::BREATH_GASP_TIME * 1000.0) as i32;
    if perks::has(pm.ps.perks, perks::HOLD_BREATH)
        || weapon_grants(pm, view_weapon(pm), perks::HOLD_BREATH)
    {
        hold += (tuning::PERK_EXTRA_BREATH * 1000.0) as i32;
    }
    (hold, gasp)
}

/// Holding breath steadies a scope until the breath runs out, then the
/// player gasps.
pub(super) fn hold_breath<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let d = def(pm, view_weapon(pm)).def;
    let (hold, gasp) = breath_times(pm);
    let ps = &mut *pm.ps;
    if hold < 1 || d.airburst_weapon() {
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
        ps.hold_breath_scale = 1.0;
        ps.hold_breath_timer = 0;
        return;
    }
    if ps.weapon_pos_frac == 1.0
        && d.weapon_class() != weapon_class::ITEM
        && d.hold_breath_to_steady()
        && pm.cmd.buttons.held(buttons::HOLD_BREATH)
        && !(0 < ps.quick_scope_timer && tuning::ANTI_QUICK_SCOPE)
        && !perks::has(ps.perks, perks::DEADSHOT)
    {
        if ps.hold_breath_timer == 0 {
            ps.weap_flags |= weap_flags::HOLD_BREATH;
        }
    } else {
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
    }
    let holding = ps.weap_flags & weap_flags::HOLD_BREATH != 0;
    if holding {
        ps.hold_breath_timer += pml.msec;
    } else {
        ps.hold_breath_timer -= pml.msec;
    }
    if ps.hold_breath_timer < 0 {
        ps.hold_breath_timer = 0;
    }
    if holding && hold < ps.hold_breath_timer {
        ps.hold_breath_timer = gasp + hold;
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
    }
    let (target, lerp) = if ps.weap_flags & weap_flags::HOLD_BREATH == 0 {
        (
            (tuning::BREATH_GASP_SCALE - 1.0)
                * (ps.hold_breath_timer as f32 / (gasp + hold) as f32)
                + 1.0,
            tuning::BREATH_GASP_LERP,
        )
    } else {
        (0.0, tuning::BREATH_HOLD_LERP)
    };
    ps.hold_breath_scale = lerp_towards(
        (target - 1.0) * ps.weapon_pos_frac + 1.0,
        ps.hold_breath_scale,
        lerp,
        pml.frametime,
    );
}

/// Firing a scope while holding breath spends it.
pub(super) fn hold_breath_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, view_weapon(pm)).def;
    let ps = &mut *pm.ps;
    if ps.weapon_pos_frac != 1.0
        || d.ads_overlay_reticle() == 0
        || d.weapon_class() == weapon_class::ITEM
    {
        return;
    }
    let hold = (tuning::BREATH_HOLD_TIME * 1000.0) as i32;
    let fire_delay = (tuning::BREATH_FIRE_DELAY * 1000.0) as i32;
    if ps.hold_breath_timer < hold {
        ps.hold_breath_timer += fire_delay;
        if hold < ps.hold_breath_timer {
            ps.hold_breath_timer = hold;
        }
    }
    ps.weap_flags &= !weap_flags::HOLD_BREATH;
}

/// The sway the sights' idle time drives: a scope's at the held-breath scale
/// and the weapon's own, half again past 1.
pub(super) fn idle_time<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if pm.ps.weapon_pos_frac == 0.0 {
        return;
    }
    let weapon = view_weapon(pm);
    if !super::some(weapon) {
        return;
    }
    let d = def(pm, weapon).def;
    let ps = &mut *pm.ps;
    let speed = if d.aim_down_sight() {
        (d.ads_idle_speed() - d.hip_idle_speed()) * ps.weapon_pos_frac + d.hip_idle_speed()
    } else {
        d.hip_idle_speed()
    };
    let ft = pml.frametime;
    if d.ads_overlay_reticle() != 0 {
        ps.weapon_idle_time += (ps.hold_breath_scale * ft * 1000.0 * speed) as i32;
    }
    let mut scale = ps.hold_breath_scale;
    if 1.0 < scale {
        scale = (scale - 1.0) * 0.5 + 1.0;
    }
    ps.weapon_idle_time += (scale * ft * 1000.0 * speed) as i32;
}

/// A sniper raised and fired at once sways more for a while.
pub(super) fn anti_quick_scope<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, old_frac: f32) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &mut *pm.ps;
    let time = (d.anti_quick_scope_time() * 1000.0) as i32;
    if time < 1 || !d.anti_quick_scope() {
        ps.quick_scope_timer = 0;
        ps.quick_scope_scale = 1.0;
        return;
    }
    let frac = ps.weapon_pos_frac;
    if old_frac <= frac && 0.001 <= frac {
        if 1.0 <= frac {
            ps.quick_scope_timer -= pml.msec;
        } else {
            ps.quick_scope_timer = time;
        }
    } else {
        ps.quick_scope_timer = 0;
    }
    if ps.quick_scope_timer < 0 {
        ps.quick_scope_timer = 0;
    }
    let target = ((ps.quick_scope_timer as f32 / time as f32) * (d.anti_quick_scope_scale() - 1.0)
        + 1.0
        - 1.0)
        * frac
        + 1.0;
    let scale = lerp_towards(
        target,
        ps.quick_scope_scale,
        tuning::BREATH_GASP_LERP,
        pml.frametime,
    );
    ps.quick_scope_scale = if 1.0 <= scale { scale } else { 1.0 };
}

/// The spread a quickscope adds while its penalty holds at full.
pub(super) fn quick_scope_spread<W: MoveWorld>(pm: &Pm<'_, W>) -> f32 {
    let ps = &*pm.ps;
    if !tuning::ANTI_QUICK_SCOPE || ps.quick_scope_scale <= 1.0 {
        return 0.0;
    }
    let d = def(pm, ps.weapon).def;
    if d.anti_quick_scope_spread_max() < ps.aim_spread_scale
        || ps.quick_scope_timer != (d.anti_quick_scope_time() * 1000.0) as i32
    {
        return 0.0;
    }
    (ps.quick_scope_scale - 1.0) * d.anti_quick_scope_spread_multiplier()
}

/// An airburst weapon aimed (or seated) with its sight button shows its
/// range finder.
pub(super) fn airburst_ads<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let airburst = def(pm, pm.ps.weapon).def.airburst_weapon();
    let ps = &mut *pm.ps;
    let on = if ps.link_flags & 4 != 0 {
        airburst
    } else {
        ps.weapon_pos_frac == 1.0 && airburst && pm.cmd.buttons.held(buttons::AIRBURST_ADS)
    };
    if on {
        ps.e_flags2 |= e_flags2::AIRBURST_ADS;
    } else {
        ps.e_flags2 &= !e_flags2::AIRBURST_ADS;
    }
}
