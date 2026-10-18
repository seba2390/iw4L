//! What Black Ops 2's `finishplayerdamage` does to the player state, read
//! from the server's handler: the splash scale of a vehicle gunner, the
//! damage timer, the hit's direction and kind for the player animation
//! script, and what going down lets go of.

use fastfile_t6::weapon::{WeaponDefView, weap_class, weap_type};
use game_api::movement::{PlayerHurt, WeaponTable};
use movement_t6::PlayerState;
use movement_t6::state::{
    ENTITYNUM_NONE, e_flags, e_flags2, pm_flags, pm_type, weap_flags, weapon_state as ws,
};
use movement_t6::tuning::{DMGTIMER_MAX_TIME, DMGTIMER_TIME_PER_POINT};

/// `vehGunnerSplashDamage`: what a gunner's seat lets through of a blast.
const VEH_GUNNER_SPLASH_DAMAGE: f32 = 0.1;
/// The other flag that keeps the server's player death from running.
const OTHER_FLAG_REFUSES_DEATH: u32 = 0x2;
/// The gunner seats of a vehicle.
const GUNNER_SEATS: std::ops::RangeInclusive<i32> = 1..=4;

/// The means of death, by the game's numbers.
mod means {
    pub const GRENADE: usize = 3;
    pub const GRENADE_SPLASH: usize = 4;
    pub const PROJECTILE: usize = 5;
    pub const PROJECTILE_SPLASH: usize = 6;
    pub const MELEE: usize = 7;
    pub const BAYONET: usize = 8;
    pub const HEAD_SHOT: usize = 9;
    pub const FALLING: usize = 12;
    pub const SUICIDE: usize = 13;
    pub const EXPLOSIVE: usize = 15;
    pub const BURNED: usize = 17;
}

/// The player animation script's `DMGDIRECTION` values.
mod direction {
    pub const FRONT: i32 = 1;
    pub const BACK: i32 = 2;
    pub const LEFT: i32 = 3;
    pub const RIGHT: i32 = 4;
}

/// The player animation script's `DMGTYPE` values.
mod kind {
    pub const BULLET: i32 = 0;
    pub const SPREAD: i32 = 1;
    pub const BURNED: i32 = 2;
    pub const MELEE: i32 = 3;
    pub const HEAD_SHOT: i32 = 4;
    pub const EXPLOSIVE: i32 = 5;
    pub const SUICIDE: i32 = 6;
    pub const RIOTSHIELD: i32 = 7;
    pub const FALLING: i32 = 8;
}

/// The definition of a filled weapon slot.
fn weapon_def<'a>(weapons: &'a dyn WeaponTable, weapon: u32) -> Option<WeaponDefView<'a>> {
    if weapon & 0xff == 0 {
        return None;
    }
    let w = weapons.weapon(weapon)?;
    WeaponDefView::new(&w.def, &w.variant)
}

/// 1/π as the helper multiplies by it.
const INV_PI: f32 = f32::from_bits(0x3ea2_f983);

/// The yaw of a direction in degrees, 0 to 360, as the handler's own helper
/// works it out: two single-precision products after the double arctangent.
fn vectoyaw(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return 0.0;
    }
    let yaw = libm::atan2(f64::from(v[1]), f64::from(v[0])) as f32 * 180.0 * INV_PI;
    if yaw >= 0.0 { yaw } else { 360.0 + yaw }
}

/// Where the hit travels against the view's whole degrees of yaw.
fn hit_direction(dir: Option<[f32; 3]>, view_yaw: f32) -> i32 {
    let Some(dir) = dir else {
        return direction::BACK;
    };
    let yaw = vectoyaw(dir);
    let view = if view_yaw < 0.0 {
        view_yaw + 360.0
    } else {
        view_yaw
    };
    let mut delta = yaw - (view as i32) as f32;
    if delta < 0.0 {
        delta += 360.0;
    }
    if !(45.0..315.0).contains(&delta) {
        direction::BACK
    } else if (135.0..225.0).contains(&delta) {
        direction::FRONT
    } else if (45.0..135.0).contains(&delta) {
        direction::RIGHT
    } else {
        direction::LEFT
    }
}

/// The kind of hit, by its means and its weapon.
fn hit_kind(means: usize, def: Option<&WeaponDefView<'_>>) -> i32 {
    match means {
        means::BURNED => kind::BURNED,
        means::GRENADE
        | means::GRENADE_SPLASH
        | means::PROJECTILE
        | means::PROJECTILE_SPLASH
        | means::EXPLOSIVE => kind::EXPLOSIVE,
        means::MELEE | means::BAYONET => {
            if def.is_some_and(|d| d.weapon_type() == weap_type::RIOTSHIELD) {
                kind::RIOTSHIELD
            } else {
                kind::MELEE
            }
        }
        means::HEAD_SHOT => kind::HEAD_SHOT,
        means::SUICIDE => kind::SUICIDE,
        means::FALLING => kind::FALLING,
        _ => {
            if def.is_some_and(|d| {
                matches!(
                    d.weapon_class(),
                    weap_class::SPREAD | weap_class::PISTOL_SPREAD
                )
            }) {
                kind::SPREAD
            } else {
                kind::BULLET
            }
        }
    }
}

/// The handler's work on the player state before it takes the health;
/// answers the damage it takes.
pub(crate) fn hurt(ps: &mut PlayerState, hurt: &PlayerHurt<'_>) -> i32 {
    let mut damage = hurt.damage;
    // A gunner's seat keeps out most of a blast (the x87 product, rounded to
    // a double and truncated).
    if ps.e_flags & e_flags::VEHICLE_VIEW != 0
        && ps.e_flags2 & e_flags2::REMOTE_CONTROL == 0
        && GUNNER_SEATS.contains(&ps.vehicle_pos)
        && matches!(hurt.means, means::PROJECTILE_SPLASH | means::GRENADE_SPLASH)
    {
        damage = (f64::from(VEH_GUNNER_SPLASH_DAMAGE) * f64::from(damage)) as i32;
    }
    ps.damage_timer += (damage as f32 * DMGTIMER_TIME_PER_POINT) as i32;
    ps.dmg_direction = hit_direction(hurt.dir, ps.viewangles[1]);
    let def = weapon_def(hurt.weapons, hurt.weapon);
    ps.dmg_type = hit_kind(hurt.means, def.as_ref());
    if ps.damage_timer as f32 > DMGTIMER_MAX_TIME {
        ps.damage_timer = DMGTIMER_MAX_TIME as i32;
    }
    ps.damage_duration = ps.damage_timer;
    damage
}

/// What a player going down lets go of: a deployed weapon goes back to
/// ready, and an offhand being readied or held is thrown at once, as if its
/// button had been let go.
pub(crate) fn going_down(ps: &mut PlayerState, weapons: &dyn WeaponTable) -> bool {
    let mut on_turret = false;
    if ps.e_flags & e_flags::TURRET != 0
        || matches!(
            ps.weaponstate,
            ws::DEPLOYING | ws::DEPLOYED | ws::BREAKING_DOWN
        )
    {
        on_turret = ps.e_flags & e_flags::TURRET != 0;
        ps.weaponstate = ws::READY;
    }
    if matches!(
        ps.weaponstate,
        ws::OFFHAND_INIT | ws::OFFHAND_PREPARE | ws::OFFHAND_HOLD
    ) {
        ps.weaponstate = ws::OFFHAND_START;
        // An offhand carries no attachment to scale its fire time by.
        let (fire_time, fire_delay) = weapon_def(weapons, ps.offhand_index)
            .map_or((0, 0), |d| (d.fire_time(), d.fire_delay()));
        ps.weapon_time = fire_time;
        ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
        ps.weapon_delay = fire_delay;
        movement_t6::weapon::start_anim(ps, movement_t6::weapon::weap_anim::FIRE);
    }
    on_turret
}

/// The kind of a killing hit: as a hurt's, but a suicide by grenade counts
/// as explosive and a fall only while the player is in the air.
fn death_kind(means: usize, def: Option<&WeaponDefView<'_>>, ground_entity: i32) -> i32 {
    match means {
        means::SUICIDE => {
            if def.is_some_and(|d| d.weapon_type() == weap_type::GRENADE) {
                kind::EXPLOSIVE
            } else {
                kind::SUICIDE
            }
        }
        means::FALLING if ground_entity != ENTITYNUM_NONE => hit_kind(0, def),
        _ => hit_kind(means, def),
    }
}

/// What the server's player death does to the player state before the
/// scripts hear of it; `Err` with the move type when it refuses to run.
pub(crate) fn die(ps: &mut PlayerState, death: &PlayerHurt<'_>) -> Result<(), i32> {
    if ps.pm_flags & pm_flags::LAUNCHED != 0 {
        ps.velocity[0] = 0.0;
        ps.velocity[1] = 0.0;
    }
    let alive = matches!(
        ps.pm_type,
        pm_type::NORMAL
            | pm_type::NORMAL_LINKED
            | pm_type::LAST_STAND
            | pm_type::LAST_STAND_REVIVED
            | pm_type::LAST_STAND_GETTING_UP
    );
    if !alive || ps.other_flags & OTHER_FLAG_REFUSES_DEATH != 0 {
        return Err(ps.pm_type);
    }
    let def = weapon_def(death.weapons, death.weapon);
    ps.dmg_type = death_kind(death.means, def.as_ref(), ps.ground_entity_num);
    Ok(())
}
