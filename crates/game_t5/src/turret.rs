//! Black Ops' map turrets (`misc_turret`): its turret spawn reads them from
//! the spawn keys a map sets, else the turret weapon's own definition; its
//! turret think (every 50 ms) cools the gun, searches the sentients for a
//! target, aims at it with convergence and keeps the firing state; its
//! `shootturret` fires one shot of the turret weapon.

use crate::fire::{Bullets, WeaponParms};
use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::FireTeam;
use game_api::movement::GameWeapon;
use game_api::turret::{
    GameTurret, Sentient, TurretArcs, TurretRules, TurretSpawn, TurretTag, TurretWorld,
};
use game_api::{Unknown, unknown};

/// The weapon class a turret's `weaponinfo` must have.
const WEAPCLASS_TURRET: i32 = 7;
/// `weapType` bullet.
const WEAPTYPE_BULLET: i32 = 0;
/// Spawned in `auto_ai` mode (aims for the AI and by itself) and usable.
const SPAWN_FLAGS: u32 = 0x1003;
/// Neither `axis` (1) nor `allies` (2).
const SPAWN_TEAM: i32 = 3;
const DROP_PITCH: f32 = -90.0;
const DEFAULT_HEALTH: i32 = 100;

/// The think's interval, ms.
const THINK_MS: i32 = 50;
/// Black Ops' sentient pool.
const SENTIENTS: usize = 36;

/// Turret flags: the AI aims it, it aims by itself (`setmode`).
const FLAG_AI: u32 = 0x1;
const FLAG_AUTO: u32 = 0x2;
/// Its converging aim has taken its starting miss.
const FLAG_CONVERGING: u32 = 0x8;
/// It has a target (`getturrettarget`).
const FLAG_HAS_TARGET: u32 = 0x40;
/// Its last aimed shot left the gun's cone.
const FLAG_OFF_CONE: u32 = 0x80;
/// `turretfiredisable`.
const FLAG_FIRE_DISABLED: u32 = 0x4000;
/// Its model has a second barrel (`tag_flash_2`).
const FLAG_TWO_BARRELS: u32 = 0x8000;
/// `fireType` single shot and minigun.
const FIRE_MINIGUN: i32 = 6;
/// The camera shake a turret's shot starts on clients near it: scale,
/// duration (ms) and radius.
const SHOT_SHAKE: (f32, i32, f32) = (0.05, 100, 100.0);

/// Firing states: off, firing, about to fire.
const FIRING_ON: i32 = 1;
const FIRING_START: i32 = 2;

/// A player's movement types from this one up are not targeted.
const PM_DEAD_TYPES: i32 = 9;
/// What the search's sight trace stops at.
const SIGHT_MASK: u32 = 0x0080_1803;
/// The least smoke visibility a target is seen through.
const MIN_VISIBILITY: f32 = 0.2;

const INV_360: f32 = 0.00277777785;
const DEG2RAD: f32 = 0.0174532924;
const RAD2DEG: f32 = 57.2957764;

pub(crate) const TURRETS: TurretRules = TurretRules {
    classname: "misc_turret",
    spawn,
    max: 96,
    think_ms: THINK_MS,
    sentients: SENTIENTS,
    think,
    shoot,
    bone_controls,
    restore_drop_pitch: unknown!(
        "t5.turret.drop_pitch",
        "`restoredefaultdroppitch` does not trace for the turret's drop pitch: the turret keeps the one it has",
        "Black Ops' drop-pitch trace (from `tag_aim` toward `tag_butt` in the model's base pose, stepping the pitch down 3 degrees up to 30 times until the trace hits), ported from its executable with the model's base-pose tags"
    ),
    player_use: unknown!(
        "t5.turret.use",
        "a player cannot mount a usable Black Ops turret, so `getturretowner` answers undefined",
        "Black Ops' turret use (activate, the player's view and fire on the turret, dismount)"
    ),
};

fn manned_unknown() -> &'static Unknown {
    unknown!(
        "t5.turret.manned",
        "a turret in `manual` mode, or with a manual target (`settargetentity`) its search did not pick, holds still: Black Ops' manual-target aim and its manned (AI-used) think are not run",
        "Black Ops' manual-target aims (an entity, a sentient) and its manned-turret think, ported from its executable, and actors that use turrets"
    )
}

fn aim_refine_unknown() -> &'static Unknown {
    unknown!(
        "t5.turret.aim_refine",
        "a turret's shot at a target goes along the gun as it points: Black Ops bends a shot toward a better point on the target when that point lies within about 18 degrees of the gun",
        "Black Ops' turret shot refinement (its trace along the gun to the target's distance, and the aim point on the target it turns the shot to), ported from its executable"
    )
}

fn weapon_type_unknown() -> &'static Unknown {
    unknown!(
        "t5.turret.weapon_type",
        "a turret whose weapon does not fire bullets (a projectile or the fourth weapon type) does not shoot",
        "Black Ops' turret shot for its projectile and fourth-type weapons, ported from its executable"
    )
}

fn visibility_unknown() -> &'static Unknown {
    unknown!(
        "t5.fx.visibility",
        "sight through smoke is not dimmed: a turret sees and refines its shot through any smoke a sight trace passes",
        "Black Ops' effect visibility between two points (the smoke and fog volumes its effects keep), which a sight stops at below 0.2"
    )
}

/// The turret a `misc_turret` spawns as. Keys the map sets win over the
/// weapon's own definition; arcs are kept as limits on either side of the
/// spawn angles, times in whole ms rounded to nearest.
fn spawn(
    weapon: Option<&GameWeapon>,
    weapon_name: &str,
    key: &dyn Fn(&str) -> Option<f32>,
    has_tag: &dyn Fn(&str) -> bool,
) -> Result<TurretSpawn, String> {
    let weapon =
        weapon.ok_or_else(|| format!("bad weaponinfo '{weapon_name}' specified for turret"))?;
    let def = WeaponDefView::new(&weapon.def, &weapon.variant)
        .ok_or_else(|| format!("bad weaponinfo '{weapon_name}' specified for turret"))?;
    if def.weapon_class() != WEAPCLASS_TURRET {
        return Err(format!("Could not load turret '{weapon_name}'"));
    }
    let float = |name: &str, default: f32| key(name).unwrap_or(default);
    let right = (-float("rightarc", def.right_arc())).min(0.0);
    let left = float("leftarc", def.left_arc()).max(0.0);
    let top = (-float("toparc", def.top_arc())).min(0.0);
    let bottom = float("bottomarc", def.bottom_arc()).max(0.0);
    // The cone a shot may leave the gun by: the yaw span, at least 90 and
    // at most 180 degrees.
    let span = left - right;
    let widest = if 90.0 - span >= 0.0 {
        90.0
    } else if span - 180.0 < 0.0 {
        span
    } else {
        180.0
    };
    let aim_cone_cos = f64::from(widest * DEG2RAD).cos() as f32;
    let yaw_convergence = key("yawconvergencetime")
        .or_else(|| key("convergencetime"))
        .unwrap_or(def.yaw_convergence_time())
        .max(0.0);
    let pitch_convergence = float("pitchconvergencetime", def.pitch_convergence_time()).max(0.0);
    let suppression = float("suppressionTime", def.suppression_time()).max(0.0);
    let max_range = float("maxrange", def.max_range());
    let accuracy = float("accuracy", def.accuracy()).clamp(0.0, 1.0);
    let mut flags = SPAWN_FLAGS;
    if has_tag("tag_flash_2") {
        flags |= FLAG_TWO_BARRELS;
    }
    Ok(TurretSpawn {
        flags,
        team: SPAWN_TEAM,
        arcs: TurretArcs {
            top,
            right,
            bottom,
            left,
        },
        spawned_yaw: [right, left],
        aim_cone_cos,
        drop_pitch: DROP_PITCH,
        convergence_ms: [ms(pitch_convergence), ms(yaw_convergence)],
        suppression_ms: ms(suppression),
        max_range_squared: if max_range > 0.0 {
            max_range * max_range
        } else {
            f32::MAX
        },
        accuracy,
        ai_spread: float("aiSpread", def.ai_spread()).max(0.0),
        player_spread: float("playerSpread", def.player_spread()).max(0.0),
        default_health: DEFAULT_HEALTH,
    })
}

/// Seconds to whole ms as the spawn rounds them: `(int)(s * 1000 + 0.5)`.
fn ms(seconds: f32) -> i32 {
    (seconds * 1000.0 + 0.5) as i32
}

/// Black Ops' turret controller: the gun's pitch less its pitch offset on
/// `tag_aim_pitch` (on `tag_aim` with the yaw when the model has no pitch
/// bone), the yaw on `tag_aim` and `tag_aim_animated`, the pitch offset on
/// `tag_flash`.
fn bone_controls(gun: [f32; 3], has_tag: &dyn Fn(&str) -> bool) -> Vec<(&'static str, [f32; 3])> {
    let pitch = gun[0] - gun[2];
    let mut controls = Vec::with_capacity(4);
    let aim_pitch = if has_tag("tag_aim_pitch") {
        controls.push(("tag_aim_pitch", [pitch, 0.0, 0.0]));
        0.0
    } else {
        pitch
    };
    controls.push(("tag_aim", [aim_pitch, gun[1], 0.0]));
    controls.push(("tag_aim_animated", [aim_pitch, gun[1], 0.0]));
    controls.push(("tag_flash", [gun[2], 0.0, 0.0]));
    controls
}

/// The game's team numbers.
fn team_number(team: Option<FireTeam>) -> i32 {
    match team {
        None => 0,
        Some(FireTeam::Axis) => 1,
        Some(FireTeam::Allies) => 2,
        Some(FireTeam::Neutral) => 3,
    }
}

/// The sentients a turret of this team searches: axis for allies, allies
/// for axis, the teamless for any other.
fn enemy_mask(team: i32) -> u32 {
    let enemy = match team {
        1 => 2,
        2 => 1,
        _ => 0,
    };
    1 << enemy
}

/// `angle` wrapped into [-180, 180), in single precision.
fn angle_normalize_180(angle: f32) -> f32 {
    let turns = angle * INV_360;
    let whole = (turns + 0.5).floor();
    (turns - whole) * 360.0
}

/// The pitch and yaw a direction points at, within ±90 and ±180.
fn vectoangles(v: [f32; 3]) -> [f32; 3] {
    if v[1] == 0.0 && v[0] == 0.0 {
        let pitch = if -v[2] >= 0.0 { 90.0 } else { -90.0 };
        return [pitch, 0.0, 0.0];
    }
    let yaw = f64::from(v[1]).atan2(f64::from(v[0])) as f32 * RAD2DEG;
    [vectopitch(v), yaw, 0.0]
}

/// The pitch a direction points at, within ±90.
fn vectopitch(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return if -v[2] >= 0.0 { 90.0 } else { -90.0 };
    }
    let forward = (v[1] * v[1] + v[0] * v[0]).sqrt();
    f64::from(v[2]).atan2(f64::from(forward)) as f32 * -RAD2DEG
}

/// `1 / length`, or 1 for a vector of no length.
fn inverse_length(length: f32) -> f32 {
    1.0 / if -length >= 0.0 { 1.0 } else { length }
}

fn normalize2(v: &mut [f32; 2]) {
    let inverse = inverse_length((v[0] * v[0] + v[1] * v[1]).sqrt());
    v[0] *= inverse;
    v[1] *= inverse;
}

fn normalize3(v: &mut [f32; 3]) {
    let inverse = inverse_length((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt());
    for x in v {
        *x *= inverse;
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The turret weapon's definition, copied out of the match.
fn weapon_def(world: &dyn TurretWorld, weapon: u32) -> Option<(Vec<u8>, Vec<u8>)> {
    world
        .weapon(weapon)
        .map(|w| (w.def.to_vec(), w.variant.to_vec()))
}

/// The turret's think: its heat cools, its fire loop sound counts down, then
/// a turret that aims by itself (not for the AI) searches and aims; one the
/// AI would aim with no AI on it drops its gun.
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
    if flags & FLAG_AUTO != 0 && flags & FLAG_AI == 0 {
        think.auto();
    } else if flags & FLAG_AI != 0 {
        // No actor uses a turret here, so one the AI aims has no gunner.
        think.stop(false);
    } else {
        think.gaps.push(manned_unknown());
    }
    gaps
}

/// One shot of the turret's weapon (`shootturret`): none while its fire is
/// disabled. Its fire loop sound runs for three fire times.
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
    // A player on the turret fires it from their view; none can be here.
    let aiming = turret.spawn.flags & FLAG_HAS_TARGET != 0;
    let mut think = Think {
        turret,
        world,
        gaps: &mut gaps,
    };
    think.fire(&def, aiming);
    gaps
}

struct Think<'a> {
    turret: &'a mut GameTurret,
    world: &'a mut dyn TurretWorld,
    gaps: &'a mut Vec<&'static Unknown>,
}

impl Think<'_> {
    fn time(&self) -> i32 {
        self.world.level_time()
    }

    fn flags(&mut self) -> &mut u32 {
        &mut self.turret.spawn.flags
    }

    fn tag(&mut self, tag: &str) -> Option<TurretTag> {
        let gun = self.turret.state.gun_angles;
        self.world.turret_tag(self.turret.entity, tag, gun)
    }

    /// An overheating weapon cools by its cooldown rate over the frame; it
    /// stops overheating once down to its end value.
    fn cool(&mut self) {
        let Some((def_bytes, variant_bytes)) = weapon_def(&*self.world, self.turret.weapon) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        // Only a player on the turret could keep it from cooling.
        if def.overheat_weapon() == 0 {
            return;
        }
        let elapsed = (self.time() - self.world.previous_level_time()) as f32;
        let state = &mut self.turret.state;
        state.heat -= elapsed * def.cooldown_rate() * 0.001;
        if state.heat < 0.0 {
            state.heat = 0.0;
        }
        if state.heat <= def.overheat_end_val() {
            state.overheating = false;
        }
    }

    /// The fire loop sound: each think clears the entity's loop sound and,
    /// for three fire times after a shot, sets the weapon's fire loop (a
    /// player's variant on a turret a player mans, which none does here);
    /// when the countdown runs out on a weapon with a stop sound, the loop
    /// stops and the stop sound plays.
    fn fire_sound(&mut self) {
        let entity = self.turret.entity;
        self.world.turret_loop_sound(entity, None);
        if self.turret.state.fire_sound_ms <= 0 {
            return;
        }
        let Some(weapon) = self.world.weapon(self.turret.weapon) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let fire_loop = weapon.fire_loop_sounds[0].clone();
        let fire_stop = weapon.fire_stop_sounds[0].clone();
        self.world.turret_loop_sound(entity, fire_loop.as_deref());
        self.turret.state.fire_sound_ms -= THINK_MS;
        if self.turret.state.fire_sound_ms <= 0
            && let Some(stop) = fire_stop
        {
            self.world.turret_loop_sound(entity, None);
            self.world.turret_sound(entity, &stop);
        }
    }

    /// What the clients do for a shot (Black Ops' MG42 fire event, one per
    /// barrel): the camera shake around it and, as their fire handler does,
    /// the weapon's world flash on the barrel's tag and the fire sound from
    /// the barrel (which their fire rules time). A minigun shows nothing.
    fn fire_event(&mut self, def: &WeaponDefView<'_>) {
        let entity = self.turret.entity;
        let (origin, _) = self.world.turret_pose(entity);
        let (scale, duration_ms, radius) = SHOT_SHAKE;
        self.world.shake_camera(origin, scale, duration_ms, radius);
        if def.fire_type() == FIRE_MINIGUN {
            return;
        }
        let tag = if self.turret.state.barrel == 0 {
            "tag_flash"
        } else {
            "tag_flash_2"
        };
        let flash = self
            .world
            .weapon(self.turret.weapon)
            .and_then(|weapon| weapon.world_flash.clone());
        if let Some(flash) = flash {
            self.world.turret_tag_effect(entity, &flash, tag);
        }
        if let Some(muzzle) = self.tag(tag) {
            let barrel = self.turret.state.barrel;
            self.world
                .turret_fire_sound(entity, self.turret.weapon, barrel, muzzle.origin);
        }
    }

    /// A turret aiming by itself: the target its search finds, aimed at and
    /// turned to; else a manual target; else it lets go of its target and
    /// turns back to its rest.
    fn auto(&mut self) {
        let found = self.search();
        let manual = self.turret.state.manual_target;
        if let Some(target) = found {
            let mut angles = [0.0; 3];
            let conv = self.turret.spawn.convergence_ms[1];
            if self.aim(&target, true, conv, &mut angles) {
                self.turn(angles, true);
                return;
            }
            if manual == Some(target.entity) {
                self.clear_target();
                self.stop(true);
                return;
            }
        }
        if manual.is_some() {
            self.gaps.push(manned_unknown());
            return;
        }
        self.clear_target();
        self.stop(true);
    }

    /// The search: the current target while it is newer than
    /// `turret_SentryTargetTime` (an enemy kept without a look, a friend
    /// dropped), else it again when it still checks; then the next sentient
    /// of the enemy team after the last one looked at, taken when it checks
    /// and is nearer than the current target.
    fn search(&mut self) -> Option<Sentient> {
        let mut current = None;
        if *self.flags() & FLAG_HAS_TARGET != 0
            && let Some(target) = self.turret.state.target
            && let Some(sentient) = self.world.sentient_of(target)
            && sentient
                .pm_type
                .is_none_or(|pm_type| pm_type < PM_DEAD_TYPES)
        {
            let hold = crate::fire::dvar(&*self.world, "turret_SentryTargetTime", 4000i32);
            if self.time() > hold + self.turret.state.target_time {
                if !sentient.ignore_me && self.check(&sentient).is_some() {
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
        let next = (from..SENTIENTS).find_map(|slot| {
            let sentient = self.world.sentient(slot)?;
            (mask & (1 << team_number(sentient.team)) != 0).then_some((slot, sentient))
        });
        let Some((slot, candidate)) = next else {
            self.turret.state.cursor = -1;
            return current;
        };
        self.turret.state.cursor = slot as i32;
        if !candidate.ignore_me
            && candidate.entity != self.turret.state.user
            && let Some(distance_sq) = self.check(&candidate)
        {
            let nearer = match &current {
                None => true,
                Some(current) => {
                    let (origin, _) = self.world.turret_pose(self.turret.entity);
                    let d = [
                        f64::from(current.origin[0]) - f64::from(origin[0]),
                        f64::from(current.origin[1]) - f64::from(origin[1]),
                        f64::from(current.origin[2]) - f64::from(origin[2]),
                    ];
                    f64::from(distance_sq) < d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
                }
            };
            if nearer {
                return Some(candidate);
            }
        }
        current
    }

    /// A target the turret can take: in range, within its arcs, no friendly
    /// player in the line, and in sight from the barrel to its eye. Returns
    /// its squared distance.
    fn check(&mut self, target: &Sentient) -> Option<f32> {
        // A player on a turret or vehicle is passed by; no player is on
        // one here (`t5.turret.use`).
        let (origin, _) = self.world.turret_pose(self.turret.entity);
        let d = sub(target.origin, origin);
        let distance_sq = d[0] * d[0] + d[2] * d[2] + d[1] * d[1];
        if distance_sq >= self.turret.spawn.max_range_squared {
            return None;
        }
        let mut angles = [0.0; 3];
        let (_, flash) = self.aim_point(target, &mut angles)?;
        if self.friendly_in_line(flash, target) {
            return None;
        }
        if !self.sight(flash, target.eye, target.entity) {
            return None;
        }
        Some(distance_sq)
    }

    /// Where on a target the turret aims, within its arcs: its yaw from the
    /// barrel to its eye, then its pitch to its feet (2 units up) or its eye,
    /// the eye first while firing. A target the pitch arcs fall between is
    /// aimed at level with the barrel. Returns the point and the barrel's
    /// tag; `angles` takes the aim relative to the turret's base.
    fn aim_point(
        &mut self,
        target: &Sentient,
        angles: &mut [f32; 3],
    ) -> Option<([f32; 3], [f32; 3])> {
        let eye = target.eye;
        let flash = self.tag("tag_flash")?.origin;
        let (_, base) = self.world.turret_pose(self.turret.entity);
        let arcs = self.turret.spawn.arcs;
        let to_eye = vectoangles(sub(eye, flash));
        let yaw = angle_normalize_180(to_eye[1] - base[1]);
        angles[1] = yaw;
        if yaw > arcs.left || arcs.right > yaw {
            return None;
        }
        let feet = [target.origin[0], target.origin[1], target.origin[2] + 2.0];
        let eye_pitch = movement_t5::math::angle_delta(to_eye[0], base[0]);
        let feet_pitch = movement_t5::math::angle_delta(vectopitch(sub(feet, flash)), base[0]);
        let order = if self.turret.state.firing == FIRING_ON {
            [(eye_pitch, eye), (feet_pitch, feet)]
        } else {
            [(feet_pitch, feet), (eye_pitch, eye)]
        };
        // Which way each point leaves the pitch arcs: below or above.
        let mut below = [false; 2];
        for (i, (pitch, point)) in order.into_iter().enumerate() {
            angles[0] = pitch;
            if pitch > arcs.bottom {
                below[i] = true;
            } else if arcs.top <= pitch {
                return Some((point, flash));
            }
        }
        if below[0] == below[1] {
            return None;
        }
        angles[0] = 0.0;
        Some(([eye[0], eye[1], flash[2]], flash))
    }

    /// Whether the first player, on the turret's team, stands in the line to
    /// the target: within the angle their heights subtend at the barrel
    /// (scaled by `turretPlayerAvoidScale`) of the target, or right at the
    /// barrel.
    fn friendly_in_line(&mut self, flash: [f32; 3], target: &Sentient) -> bool {
        let Some(player) = self.world.sentient_of(0) else {
            return false;
        };
        let team = self.turret.spawn.team;
        if team_number(player.team) != team || team == 0 {
            return false;
        }
        let center = |s: &Sentient| -> [f32; 3] {
            std::array::from_fn(|i| s.origin[i] + (s.maxs[i] + s.mins[i]) * 0.5)
        };
        let to_player = sub(center(&player), flash);
        let player_distance = (to_player[2] * to_player[2]
            + to_player[1] * to_player[1]
            + to_player[0] * to_player[0])
            .sqrt();
        let inverse = inverse_length(player_distance);
        let player_dir = to_player.map(|x| x * inverse);
        if 1e-5 > player_distance {
            return true;
        }
        let to_target = sub(center(target), flash);
        let target_distance = (to_target[2] * to_target[2]
            + to_target[1] * to_target[1]
            + to_target[0] * to_target[0])
            .sqrt();
        let inverse = inverse_length(target_distance);
        let dot = to_target[2] * inverse * player_dir[2]
            + to_target[1] * inverse * player_dir[1]
            + inverse * to_target[0] * player_dir[0];
        let apart = f64::from(dot).acos() as f32;
        let scale = crate::fire::dvar(&*self.world, "turretPlayerAvoidScale", 1.7f32);
        let heights = ((player.maxs[2] - player.mins[2]) + (target.maxs[2] - target.mins[2])) * 0.5;
        let near = f64::from(heights / player_distance * scale).atan() as f32;
        near >= apart
    }

    /// A sight trace from the barrel to the target, through the turret and
    /// the target, and not through thick smoke.
    fn sight(&mut self, flash: [f32; 3], eye: [f32; 3], target: i32) -> bool {
        if !self
            .world
            .sight_clear(flash, eye, [self.turret.entity, target], SIGHT_MASK)
        {
            return false;
        }
        self.visibility() >= MIN_VISIBILITY
    }

    /// The smoke visibility between two points.
    fn visibility(&mut self) -> f32 {
        self.gaps.push(visibility_unknown());
        1.0
    }

    /// Aims at a target: a new one starts its convergence. While the yaw
    /// convergence time runs the turret aims off the target (63 units to the
    /// side its gun already points, at the target's feet height or its aim
    /// point's), closing to 33 units at the end; then at the target itself.
    fn aim(&mut self, target: &Sentient, on: bool, conv: i32, angles: &mut [f32; 3]) -> bool {
        let time = self.time();
        if self.turret.state.target != Some(target.entity) {
            self.turret.state.target = Some(target.entity);
            *self.flags() &= !FLAG_CONVERGING;
            self.turret.state.target_time = time;
        }
        let Some((point, _)) = self.aim_point(target, angles) else {
            return false;
        };
        let state = self.turret.state;
        let left = self.turret.spawn.convergence_ms[1] - time + state.target_time;
        if left <= 0 {
            self.fire_state(point, on, angles);
            return true;
        }
        if *self.flags() & FLAG_CONVERGING == 0 {
            *self.flags() |= FLAG_CONVERGING;
            let Some(aim) = self.tag("tag_aim") else {
                return false;
            };
            let mut side = [point[1] - aim.origin[1], aim.origin[0] - point[0]];
            normalize2(&mut side);
            let side = [side[0] * 63.0, side[1] * 63.0];
            let facing = aim.axis[0][1] * side[1] + aim.axis[0][0] * side[0];
            let mut miss = if facing < 0.0 {
                [point[0] - side[0], point[1] - side[1], target.origin[2]]
            } else {
                [point[0] + side[0], point[1] + side[1], target.origin[2]]
            };
            let aimed = self.aim_at(miss, on, angles) || {
                miss[2] = point[2];
                self.aim_at(miss, on, angles)
            };
            if aimed {
                let mut dir = sub(miss, point);
                normalize3(&mut dir);
                self.turret.state.miss = dir;
                return true;
            }
            self.turret.state.miss = [0.0; 3];
            self.fire_state(point, on, angles);
            return true;
        }
        let frac = if conv > 0 {
            (state.target_time - time + conv) as f32 / conv as f32
        } else {
            0.0
        } * 30.0;
        let miss = state.miss;
        let off = std::array::from_fn(|i| miss[i] * 33.0 + miss[i] * frac + point[i]);
        if self.aim_at(off, on, angles) {
            return true;
        }
        self.fire_state(point, on, angles);
        true
    }

    /// Aims at a point within the turret's arcs; out of them the turret
    /// loses its target.
    fn aim_at(&mut self, point: [f32; 3], on: bool, angles: &mut [f32; 3]) -> bool {
        if !self.within_arcs(point, angles) {
            *self.flags() &= !FLAG_HAS_TARGET;
            return false;
        }
        self.fire_state(point, on, angles);
        true
    }

    /// The angles from the barrel to a point, relative to the turret's base,
    /// and whether they are within its arcs.
    fn within_arcs(&mut self, point: [f32; 3], angles: &mut [f32; 3]) -> bool {
        let Some(flash) = self.tag("tag_flash") else {
            return false;
        };
        let (_, base) = self.world.turret_pose(self.turret.entity);
        let to = vectoangles(sub(point, flash.origin));
        let arcs = self.turret.spawn.arcs;
        let limits = [(arcs.top, arcs.bottom), (arcs.right, arcs.left)];
        for i in 0..2 {
            let relative = angle_normalize_180(to[i] - base[i]);
            angles[i] = relative;
            let (low, high) = limits[i];
            if relative > high || low > relative {
                return false;
            }
        }
        true
    }

    /// The firing state while aiming: it starts firing 250 ms after taking
    /// its target or once within 5 degrees of its aim, and fires in earnest
    /// once the yaw convergence time is over. Not aiming stops it. The
    /// point becomes the turret's aim point.
    fn fire_state(&mut self, point: [f32; 3], on: bool, angles: &[f32; 3]) {
        if on {
            let time = self.time();
            let state = self.turret.state;
            let left = self.turret.spawn.convergence_ms[1] - time + state.target_time;
            let gun = state.gun_angles;
            let mut firing = state.firing;
            if firing == 0
                && (time >= state.target_time + 250
                    || (angle_normalize_180(gun[0] - angles[0]).abs() < 5.0
                        && movement_t5::math::angle_delta(gun[1], angles[1]).abs() < 5.0))
            {
                self.set_firing(FIRING_START);
                firing = FIRING_START;
            }
            if firing == FIRING_START && left < 0 {
                self.set_firing(FIRING_ON);
            }
        } else {
            self.set_firing(0);
        }
        *self.flags() |= FLAG_HAS_TARGET;
        self.turret.state.aim_point = point;
    }

    /// Sets the firing state; turning on or off raises `turretstatechange`.
    fn set_firing(&mut self, firing: i32) {
        if (self.turret.state.firing != 0) != (firing != 0) {
            self.world
                .notify(self.turret.entity, "turretstatechange", &[]);
        }
        self.turret.state.firing = firing;
    }

    /// Lets go of the target: no target, no convergence, not firing.
    fn clear_target(&mut self) {
        self.turret.state.target = None;
        *self.flags() &= !(FLAG_HAS_TARGET | FLAG_CONVERGING);
        self.set_firing(0);
    }

    /// Stops firing and turns to its rest: straight ahead when aiming by
    /// itself, its drop pitch otherwise.
    fn stop(&mut self, on: bool) {
        self.set_firing(0);
        let pitch = if on {
            0.0
        } else {
            self.turret.spawn.drop_pitch
        };
        self.turn([pitch, 0.0, 0.0], on);
    }

    /// The turn rates to reach the angles by the next think, slowed so the
    /// convergence times are spent turning.
    fn rates(&self, angles: [f32; 3]) -> [f32; 2] {
        let time = self.time();
        let state = &self.turret.state;
        let gun = state.gun_angles;
        let pitch_left = self.turret.spawn.convergence_ms[0] - time + state.target_time;
        let yaw_left = self.turret.spawn.convergence_ms[1] - time + state.target_time;
        let interval = THINK_MS as f32;
        let mut pitch = angle_normalize_180(gun[0] - angles[0]).abs() * 1000.0 / interval;
        let mut yaw = angle_normalize_180(gun[1] - angles[1]).abs() * 1000.0 / interval;
        // The game tests a time left above zero, then above the interval.
        if THINK_MS < yaw_left {
            yaw *= interval / yaw_left as f32;
        }
        if THINK_MS < pitch_left {
            pitch *= interval / pitch_left as f32;
        }
        [pitch, yaw]
    }

    /// Turns the gun toward the angles at the rates, clamped to the
    /// weapon's turn speeds while aiming (200 degrees a second otherwise),
    /// one think's worth. Returns whether it got there.
    fn turn(&mut self, angles: [f32; 3], on: bool) -> bool {
        let mut rates = self.rates(angles);
        if on {
            let Some((def_bytes, variant_bytes)) = weapon_def(&*self.world, self.turret.weapon)
            else {
                self.gaps.push(crate::movement::weapon_unknown());
                return false;
            };
            let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
                self.gaps.push(crate::movement::weapon_unknown());
                return false;
            };
            if def.max_vert_turn_speed() - rates[0] < 0.0 {
                rates[0] = def.max_vert_turn_speed();
            }
            if rates[0] - def.min_vert_turn_speed() < 0.0 {
                rates[0] = def.min_vert_turn_speed();
            }
            if def.max_hor_turn_speed() - rates[1] < 0.0 {
                rates[1] = def.max_hor_turn_speed();
            }
            if rates[1] - def.min_hor_turn_speed() < 0.0 {
                rates[1] = def.min_hor_turn_speed();
            }
        } else {
            rates = [200.0, 200.0];
        }
        // The game also holds the pitch at a stored value under flags its
        // manned paths set; nothing here sets them.
        let mut arrived = true;
        let gun = &mut self.turret.state.gun_angles;
        for i in 0..2 {
            let step = rates[i] * 0.05;
            let mut delta = angle_normalize_180(angles[i] - gun[i]);
            if delta > step {
                delta = step;
                arrived = false;
            } else if -step > delta {
                delta = -step;
                arrived = false;
            }
            gun[i] += delta;
        }
        arrived
    }

    /// One shot: from the barrel along the gun (its base angles turned by
    /// the gun's), noting whether the aim point is outside the gun's cone,
    /// with the weapon's AI spread. A two-barrelled turret alternates.
    fn fire(&mut self, def: &WeaponDefView<'_>, aiming: bool) {
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
        if aiming {
            let mut to = sub(self.turret.state.aim_point, flash.origin);
            normalize3(&mut to);
            let dot = forward[2] * to[2] + forward[1] * to[1] + forward[0] * to[0];
            if self.turret.spawn.aim_cone_cos >= dot {
                *self.flags() |= FLAG_OFF_CONE;
            } else {
                *self.flags() &= !FLAG_OFF_CONE;
            }
            self.visibility();
            if self.turret.state.target.is_some() {
                self.gaps.push(aim_refine_unknown());
            }
        }
        // The turret fires for itself: no owner here is a player or an
        // entity of its own.
        let spread = self.turret.spawn.ai_spread;
        if def.weapon_type() == WEAPTYPE_BULLET {
            let time = self.time();
            let entity = self.turret.entity;
            Bullets {
                attacker: entity,
                weapon_entity: entity,
                zombies: true,
                gaps: self.gaps,
            }
            .fire(
                def,
                self.turret.weapon,
                spread,
                &parms,
                time,
                &mut *self.world,
            );
        } else {
            self.gaps.push(weapon_type_unknown());
        }
        self.fire_event(def);
        if *self.flags() & FLAG_TWO_BARRELS != 0 {
            self.turret.state.barrel = (self.turret.state.barrel + 1) % 2;
        }
    }
}
