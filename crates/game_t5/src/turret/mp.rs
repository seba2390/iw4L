//! Black Ops multiplayer's turrets, as BlackOpsMP.exe runs them: the
//! killstreak Sentry Gun (`setturrettype("sentry")`) searches the sentient
//! pool for an enemy, the SAM Turret (`"tow"`) searches the lock-on target
//! list for an aircraft; with neither in sight each sweeps its arcs. Any
//! turret not aiming by itself drops its gun. Its shot carries no aim
//! refinement and is the turret's own; a projectile shot goes through the
//! multiplayer missile spawn.

use super::*;
use game_api::turret::TurretTarget;

/// Turret flags `setturrettype` sets.
pub(crate) const FLAG_SENTRY: u32 = 0x1_0000;
pub(crate) const FLAG_TOW: u32 = 0x2_0000;
/// A scanning sweep is under way, and which way it turns (toward the right
/// arc when set).
const FLAG_SCANNING: u32 = 0x4_0000;
const FLAG_SCAN_BACK: u32 = 0x8_0000;
/// The turret holds still instead of scanning.
const FLAG_NO_SCAN: u32 = 0x10_0000;

/// The entity flag a turret put down carries until its gun levels.
pub(crate) const EF_PLACED: u32 = 0x20;

/// A sentient player who is spectating: `pm_type` 4.
const PM_SPECTATOR: i32 = 4;
/// `specialty_nottargetedbyai`, in the game's perk order.
const PERK_NOT_TARGETED_BY_AI: u32 = 35;
/// The offhand slot whose weapon lowers its holder's eye by 8 units.
const OFFHAND_SLOT_LOW_EYE: i32 = 3;

/// The sight traces' content masks: a SAM's, any other turret's.
const SIGHT_MASK_TOW: u32 = 0x0280_e893;
const SIGHT_MASK: u32 = 0x0081_0011;

pub(crate) const TURRETS: TurretRules = TurretRules {
    spawn,
    max: 32,
    sentients: 48,
    think,
    shoot,
    ..super::TURRETS
};

fn fourth_type_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.turret_weapon_type",
        "a multiplayer turret whose weapon is of the fourth weapon type does not shoot",
        "Black Ops' multiplayer turret shot for that weapon type (BlackOpsMP.exe)"
    )
}

/// A multiplayer turret spawns as a map turret does, but with no team.
fn spawn(
    weapon: Option<&GameWeapon>,
    weapon_name: &str,
    key: &dyn Fn(&str) -> Option<f32>,
    has_tag: &dyn Fn(&str) -> bool,
) -> Result<TurretSpawn, String> {
    let mut spawn = super::spawn(weapon, weapon_name, key, has_tag)?;
    spawn.team = 0;
    Ok(spawn)
}

/// The think: heat cools and the fire loop counts down; a turret that does
/// not aim by itself (or aims for the AI) drops its gun; a Sentry Gun runs
/// its search, then a SAM Turret its own.
fn think(turret: &mut GameTurret, world: &mut dyn TurretWorld) -> Vec<&'static Unknown> {
    let mut gaps = Vec::new();
    let mut think = Think {
        turret,
        world,
        gaps: &mut gaps,
    };
    think.turret.state.gun_angles[2] = 0.0;
    think.cool();
    think.fire_sound();
    let flags = think.turret.spawn.flags;
    if flags & FLAG_AUTO == 0 || flags & FLAG_AI != 0 {
        think.stop_mp(false);
        return gaps;
    }
    if flags & FLAG_SENTRY != 0 {
        think.sentry();
    }
    if think.turret.spawn.flags & FLAG_TOW != 0 {
        think.tow();
    }
    gaps
}

/// One shot (`shootturret`): none while fire is disabled. The fire loop runs
/// for three fire times; the turret fires as itself, with its AI spread.
fn shoot(turret: &mut GameTurret, world: &mut dyn TurretWorld) -> Vec<&'static Unknown> {
    let mut gaps = Vec::new();
    if turret.spawn.flags & FLAG_FIRE_DISABLED != 0 {
        return gaps;
    }
    let Some((def_bytes, variant_bytes)) = weapon_def(&*world, turret.weapon) else {
        gaps.push(crate::movement::weapon_unknown());
        return gaps;
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        gaps.push(crate::movement::weapon_unknown());
        return gaps;
    };
    turret.state.fire_sound_ms = def.fire_time() * 3;
    // Black Ops credits the shot to the entity that owns the turret's
    // linked use, which only a player on it sets; none is here, so the
    // turret fires as itself and its just-put-down flag clears.
    turret.state.entity_flags &= !EF_PLACED;
    let mut think = Think {
        turret,
        world,
        gaps: &mut gaps,
    };
    think.fire_mp(&def);
    gaps
}

impl Think<'_> {
    /// The Sentry Gun: the enemy its search finds, aimed at and turned to;
    /// else its manual target; else it lets go and sweeps.
    fn sentry(&mut self) {
        let found = self.sentry_search();
        let manual = self.turret.state.manual_target;
        let conv = self.turret.spawn.convergence_ms[1];
        if let Some(target) = found {
            let mut angles = [0.0; 3];
            if self.aim_mp(&target, conv, &mut angles) {
                self.turn_mp(angles, true);
                return;
            }
            if manual == Some(target.entity) {
                self.idle();
                return;
            }
        }
        if let Some(manual) = manual {
            let aimed = match self.world.sentient_of(manual) {
                Some(sentient) => {
                    self.check_sentient(&sentient).is_some() && {
                        let mut angles = [0.0; 3];
                        let aimed = self.aim_mp(&sentient, conv, &mut angles);
                        if aimed {
                            self.turn_mp(angles, true);
                        }
                        aimed
                    }
                }
                None => self.aim_entity(manual),
            };
            if aimed {
                return;
            }
        }
        self.idle();
    }

    /// The SAM Turret: the aircraft its search finds (or its manual
    /// target), aimed at and turned to; else it lets go and sweeps. Every
    /// think without a target raises `turretstatechange`.
    fn tow(&mut self) {
        let found = self.tow_search();
        let target = self.turret.state.manual_target.or(found);
        if let Some(target) = target
            && self.aim_entity(target)
        {
            return;
        }
        self.idle();
        self.world
            .notify(self.turret.entity, "turretstatechange", &[]);
    }

    /// No target: it lets go of the one it had, then sweeps its arcs, or
    /// turns straight ahead when it may not scan.
    fn idle(&mut self) {
        self.clear_target();
        let flags = self.turret.spawn.flags;
        if flags & (FLAG_SENTRY | FLAG_TOW) != 0 && flags & FLAG_NO_SCAN == 0 {
            self.scan();
        } else {
            self.stop_mp(true);
        }
    }

    /// The Sentry Gun's search: the current target while it is newer than
    /// `turret_SentryTargetTime` (an enemy kept without a look), else it
    /// again when it still checks; then the next sentient of the enemy team
    /// (or of none) after the last one looked at, taken when it may be
    /// targeted, checks and is nearer than the current one.
    fn sentry_search(&mut self) -> Option<Sentient> {
        let mut current = None;
        if *self.flags() & FLAG_HAS_TARGET != 0
            && let Some(target) = self.turret.state.target
            && let Some(sentient) = self.world.sentient_of(target)
            && sentient
                .pm_type
                .is_none_or(|pm_type| pm_type < PM_DEAD_TYPES)
        {
            let hold = crate::fire::dvar(&*self.world, "turret_SentryTargetTime", 1000i32);
            if hold + self.turret.state.target_time < self.time() {
                if !sentient.ignore_me && self.check_sentient(&sentient).is_some() {
                    current = Some(sentient);
                }
            } else {
                let team = team_number(sentient.team);
                if self.turret.spawn.team != team || team == 0 {
                    return Some(sentient);
                }
            }
        }
        let mask = enemy_mask(self.turret.spawn.team);
        let from = usize::try_from(self.turret.state.cursor + 1).unwrap_or(0);
        let next = (from..TURRETS.sentients).find_map(|slot| {
            let sentient = self.world.sentient(slot)?;
            let team = team_number(sentient.team);
            (mask & (1 << team) != 0 || team == 0).then_some((slot, sentient))
        });
        let Some((slot, candidate)) = next else {
            self.turret.state.cursor = -1;
            return current;
        };
        self.turret.state.cursor = slot as i32;
        if candidate.ignore_me || Some(candidate.entity) == self.turret.state.owner {
            return current;
        }
        if candidate.pm_type.is_some_and(|pm_type| {
            pm_type == PM_SPECTATOR || candidate.spectating || pm_type >= PM_DEAD_TYPES
        }) {
            return current;
        }
        let Some(distance_sq) = self.check_sentient(&candidate) else {
            return current;
        };
        if candidate.pm_type.is_some() && candidate.perks & (1 << PERK_NOT_TARGETED_BY_AI) != 0 {
            return current;
        }
        let nearer = match &current {
            None => true,
            Some(current) => {
                let (origin, _) = self.world.turret_pose(self.turret.entity);
                let d: [f64; 3] =
                    std::array::from_fn(|i| f64::from(current.origin[i]) - f64::from(origin[i]));
                f64::from(distance_sq) < d[1] * d[1] + d[2] * d[2] + d[0] * d[0]
            }
        };
        if nearer { Some(candidate) } else { current }
    }

    /// The SAM Turret's search: the current target while it is newer than
    /// `turret_KillstreakTargetTime`; else the first target on the list of
    /// another team (or of a turret with none), not its owner's, that
    /// turrets may acquire and that checks, else the current one while it
    /// still checks.
    fn tow_search(&mut self) -> Option<i32> {
        let targets = self.world.turret_targets();
        let mut held = None;
        if *self.flags() & FLAG_HAS_TARGET != 0
            && let Some(target) = self.turret.state.target
            && let Some(listed) = targets.iter().flatten().find(|t| t.entity == target)
        {
            let hold = crate::fire::dvar(&*self.world, "turret_KillstreakTargetTime", 4000i32);
            if self.time() <= hold + self.turret.state.target_time {
                return Some(listed.entity);
            }
            held = Some(*listed);
        }
        let owner = self.turret.state.owner.unwrap_or(ENTITY_NONE);
        for candidate in targets.iter().flatten() {
            if let Some(target) = held
                && self.check_target(&target).is_none()
            {
                held = None;
            }
            let team = self.turret.spawn.team;
            if (team != candidate.team || team == 0)
                && owner != candidate.owner
                && !candidate.no_turret_acquire
                && self.check_target(candidate).is_some()
            {
                self.take_target(candidate.entity);
                return Some(candidate.entity);
            }
        }
        held.map(|t| t.entity)
    }

    /// A list target the turret can take: a sentient as the Sentry Gun's
    /// check has it, else any entity within its arcs and in sight of the
    /// barrel.
    fn check_target(&mut self, target: &TurretTarget) -> Option<f32> {
        if let Some(sentient) = self.world.sentient_of(target.entity) {
            return self.check_sentient(&sentient);
        }
        let distance_sq = self.distance_sq(target.origin);
        if !self.in_range(distance_sq) {
            return None;
        }
        let mut angles = [0.0; 3];
        if !self.within_arcs(target.origin, &mut angles) {
            return None;
        }
        let flash = self.tag("tag_flash")?.origin;
        self.sight_mp(flash, target.origin, target.entity)
            .then_some(distance_sq)
    }

    /// A sentient the turret can take: in range (any range for a SAM),
    /// within its arcs at its led aim point, and its eye in sight of the
    /// barrel. A player on a turret or vehicle is passed by; none is on
    /// one here. Returns its squared distance.
    fn check_sentient(&mut self, target: &Sentient) -> Option<f32> {
        let distance_sq = self.distance_sq(target.origin);
        if !self.in_range(distance_sq) {
            return None;
        }
        let led = self.led(target);
        let mut angles = [0.0; 3];
        let (_, flash) = self.aim_point(&led, &mut angles)?;
        self.sight_mp(flash, target.eye, target.entity)
            .then_some(distance_sq)
    }

    fn distance_sq(&mut self, point: [f32; 3]) -> f32 {
        let (origin, _) = self.world.turret_pose(self.turret.entity);
        let d = sub(point, origin);
        d[2] * d[2] + d[0] * d[0] + d[1] * d[1]
    }

    fn in_range(&self, distance_sq: f32) -> bool {
        self.turret.spawn.flags & FLAG_TOW != 0 || distance_sq < self.turret.spawn.max_range_squared
    }

    fn sight_mp(&mut self, from: [f32; 3], to: [f32; 3], target: i32) -> bool {
        let mask = if self.turret.spawn.flags & FLAG_TOW != 0 {
            SIGHT_MASK_TOW
        } else {
            SIGHT_MASK
        };
        self.world
            .sight_clear(from, to, [self.turret.entity, target], mask)
    }

    /// A player as the turret aims at them: their eye 8 units lower while
    /// they hold a weapon of the low-eye offhand slot, led by their
    /// velocity over `turret_TargetLeadBias` think intervals.
    fn led(&mut self, target: &Sentient) -> Sentient {
        let Some(player) = self.world.player(target.entity) else {
            return *target;
        };
        let (ps, _) = crate::movement::unpack(&mut player.clone());
        let mut eye = crate::fire::view_origin(&ps);
        let low = self
            .world
            .weapon(ps.weapon)
            .and_then(|w| WeaponDefView::new(&w.def, &w.variant).map(|d| d.offhand_slot()))
            == Some(OFFHAND_SLOT_LOW_EYE);
        if low {
            eye[2] -= 8.0;
        }
        let lead = crate::fire::dvar(&*self.world, "turret_TargetLeadBias", 1i32);
        let seconds = (lead * THINK_MS) as f32 * 0.001;
        Sentient {
            eye: std::array::from_fn(|i| ps.velocity[i] * seconds + eye[i]),
            ..*target
        }
    }

    /// The turret takes a target: a new one restarts its convergence; one
    /// held past `turret_KillstreakTargetTime` restarts its time.
    fn take_target(&mut self, entity: i32) {
        let time = self.time();
        if self.turret.state.target != Some(entity) {
            self.turret.state.target = Some(entity);
            *self.flags() &= !FLAG_CONVERGING;
            self.turret.state.target_time = time;
            return;
        }
        let hold = crate::fire::dvar(&*self.world, "turret_KillstreakTargetTime", 4000i32);
        if hold + self.turret.state.target_time < time {
            self.turret.state.target_time = time;
        }
    }

    /// Aims at a sentient at its led eye, with convergence.
    fn aim_mp(&mut self, target: &Sentient, conv: i32, angles: &mut [f32; 3]) -> bool {
        self.take_target(target.entity);
        let led = self.led(target);
        self.aim(&led, true, conv, angles)
    }

    /// Aims at an entity's origin, within the turret's arcs, and turns to
    /// it.
    fn aim_entity(&mut self, entity: i32) -> bool {
        self.take_target(entity);
        let (origin, _) = self.world.turret_pose(entity);
        let mut angles = [0.0; 3];
        if !self.within_arcs(origin, &mut angles) {
            *self.flags() &= !FLAG_HAS_TARGET;
            return false;
        }
        self.fire_state(origin, true, &angles);
        self.turn_mp(angles, true);
        true
    }

    /// Stops firing and turns to its rest: straight ahead when aiming by
    /// itself, its drop pitch otherwise.
    fn stop_mp(&mut self, on: bool) {
        self.set_firing(0);
        let pitch = if on {
            0.0
        } else {
            self.turret.spawn.drop_pitch
        };
        self.turn_mp([pitch, 0.0, 0.0], on);
    }

    /// Any turn ends a sweep under way and flips the way the next one goes.
    fn turn_mp(&mut self, angles: [f32; 3], on: bool) -> bool {
        self.end_sweep();
        self.turn(angles, on)
    }

    fn end_sweep(&mut self) {
        if *self.flags() & FLAG_SCANNING != 0 {
            self.world
                .notify(self.turret.entity, "turret_scan_stop", &[]);
        }
        let flags = self.flags();
        *flags = (*flags & !FLAG_SCANNING) ^ FLAG_SCAN_BACK;
        let state = &mut self.turret.state;
        state.scan_speed = 0.0;
        state.scan_slow_at = 0.0;
        state.scan_pause_until = 0;
    }

    /// The sweep: the gun levels to its scanning pitch at the weapon's scan
    /// speed, then, after the weapon's scan pause, turns to 4 degrees inside
    /// one yaw arc (at most 90 degrees off), speeding up and slowing down
    /// by its scan acceleration, and back.
    fn scan(&mut self) {
        let Some((def_bytes, variant_bytes)) = weapon_def(&*self.world, self.turret.weapon) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let target_pitch = self.turret.state.scanning_pitch;
        let pitch = angle_approach(
            target_pitch,
            self.turret.state.gun_angles[0],
            def.scan_speed(),
            0.05,
        );
        self.turret.state.gun_angles[0] = pitch;
        if target_pitch != pitch {
            return;
        }
        self.turret.state.entity_flags &= !EF_PLACED;
        let arcs = self.turret.spawn.arcs;
        let low = (arcs.right + 4.0).max(-90.0);
        let high = (arcs.left - 4.0).min(90.0);
        let yaw = self.turret.state.gun_angles[1];
        let (left, sign) = if *self.flags() & FLAG_SCAN_BACK == 0 {
            (high - yaw, 1.0)
        } else {
            (yaw - low, -1.0)
        };
        if *self.flags() & FLAG_SCANNING == 0 {
            self.start_sweep(&def, left);
        }
        let time = self.time();
        let state = &mut self.turret.state;
        if state.scan_pause_until > time {
            return;
        }
        if left < state.scan_slow_at {
            state.scan_speed -= def.scan_accel() * 0.05;
            if state.scan_speed <= 0.0 {
                self.end_sweep();
                return;
            }
        } else {
            state.scan_speed += def.scan_accel() * 0.05;
            if def.scan_speed() <= state.scan_speed {
                state.scan_speed = def.scan_speed();
            }
        }
        state.gun_angles[1] += state.scan_speed * sign * 0.05;
    }

    /// A sweep starts from rest: it pauses for the weapon's scan pause time
    /// and slows over the distance its speed takes to shed, or over the
    /// second half of the turn when that is shorter.
    fn start_sweep(&mut self, def: &WeaponDefView<'_>, left: f32) {
        *self.flags() |= FLAG_SCANNING;
        let time = self.time();
        let state = &mut self.turret.state;
        state.scan_speed = 0.0;
        state.scan_pause_until = def.scan_pause_time() + time;
        state.scan_slow_at = def.scan_speed() * def.scan_speed() / (def.scan_accel() * 2.0);
        let half = left * 0.5;
        if half < state.scan_slow_at {
            state.scan_slow_at = half;
        }
        self.world
            .notify(self.turret.entity, "turret_scan_start", &[]);
    }

    /// The shot: from the barrel along the gun; bullets with the turret's
    /// AI spread, a projectile weapon's missile.
    fn fire_mp(&mut self, def: &WeaponDefView<'_>) {
        let Some(flash) = self.tag("tag_flash") else {
            return;
        };
        let (_, base) = self.world.turret_pose(self.turret.entity);
        let gun = self.turret.state.gun_angles;
        let (forward, right, up) =
            movement_t5::math::angle_vectors([base[0] + gun[0], base[1] + gun[1], base[2]]);
        let parms = WeaponParms {
            forward,
            right,
            up,
            muzzle: flash.origin,
        };
        match def.weapon_type() {
            WEAPTYPE_BULLET => {
                let time = self.time();
                let entity = self.turret.entity;
                Bullets {
                    attacker: entity,
                    weapon_entity: entity,
                    hand: 0,
                    zombies: false,
                    deep_penetration: false,
                    gaps: self.gaps,
                }
                .fire(
                    def,
                    self.turret.weapon,
                    self.turret.spawn.ai_spread,
                    &parms,
                    time,
                    &mut *self.world,
                );
            }
            WEAPTYPE_FOURTH => self.gaps.push(fourth_type_unknown()),
            _ => self.fire_missile(def, forward, right, up, flash.origin, base),
        }
        self.fire_event(def);
        if *self.flags() & FLAG_TWO_BARRELS != 0 {
            self.turret.state.barrel = (self.turret.state.barrel + 1) % 2;
        }
    }
}

impl Think<'_> {
    /// A projectile shot: no spread (the random draw is still made), from
    /// the barrel; a SAM Turret's missile is its owner's and is told the
    /// target it aims at when that is on the lock-on list. The turret hears
    /// `missile_fire` (the missile, the weapon, the target).
    #[allow(clippy::too_many_arguments)]
    fn fire_missile(
        &mut self,
        def: &WeaponDefView<'_>,
        forward: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        muzzle: [f32; 3],
        base: [f32; 3],
    ) {
        let entity = self.turret.entity;
        let tow = *self.flags() & FLAG_TOW != 0;
        let target = self.turret.state.target.filter(|&target| {
            tow && self
                .world
                .turret_targets()
                .iter()
                .flatten()
                .any(|t| t.entity == target)
        });
        let owner = if tow {
            self.turret.state.owner.unwrap_or(entity)
        } else {
            entity
        };
        let dir = crate::missile::spread_dir(0.0, forward, right, up, &mut *self.world);
        let Some(missile) = crate::missile::spawn_rocket_mp(
            entity,
            owner,
            self.turret.weapon,
            def,
            muzzle,
            dir,
            Some(base),
            target,
            &mut *self.world,
            self.gaps,
        ) else {
            return;
        };
        let mut args = vec![
            game_api::fire::NotifyArg::Entity(missile),
            game_api::fire::NotifyArg::Weapon(self.turret.weapon),
        ];
        if let Some(target) = target {
            args.push(game_api::fire::NotifyArg::Entity(target));
        }
        self.world.notify(entity, "missile_fire", &args);
    }
}

/// The entity number of none.
const ENTITY_NONE: i32 = 1023;
/// The weapon type a turret shot handles apart from bullets and projectiles.
const WEAPTYPE_FOURTH: i32 = 4;

/// `current` turned toward `target` by `speed * dt` degrees, or onto it
/// when that would pass it, wrapped into [-180, 180).
fn angle_approach(target: f32, current: f32, speed: f32, dt: f32) -> f32 {
    let mut target = target;
    while target - current > 180.0 {
        target -= 360.0;
    }
    while target - current < -180.0 {
        target += 360.0;
    }
    let mut step = speed * dt;
    if target - current <= 0.0 {
        step = -step;
    }
    let delta = (target - current).abs();
    if 0.001 < delta && step.abs() <= delta {
        target = step + current;
    }
    angle_normalize_180(target)
}
