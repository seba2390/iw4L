//! Black Ops' map turrets (`misc_turret`), as its turret spawn reads them:
//! the spawn keys a map sets, else the turret weapon's own definition.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::movement::GameWeapon;
use game_api::turret::{TurretArcs, TurretRules, TurretSpawn};
use game_api::unknown;

/// The weapon class a turret's `weaponinfo` must have.
const WEAPCLASS_TURRET: i32 = 7;
/// Spawned in `auto_ai` mode (aims for the AI and by itself) and usable.
const SPAWN_FLAGS: u32 = 0x1003;
/// Neither `axis` (1) nor `allies` (2).
const SPAWN_TEAM: i32 = 3;
const DROP_PITCH: f32 = -90.0;
const DEFAULT_HEALTH: i32 = 100;

pub(crate) const TURRETS: TurretRules = TurretRules {
    classname: "misc_turret",
    spawn,
    max: 96,
    think: unknown!(
        "t5.turret.think",
        "a Black Ops turret does not think: it never picks a target, aims, fires (`shootturret`), overheats or raises turretstatechange, and `restoredefaultdroppitch` does not trace for its drop pitch",
        "Black Ops' turret think (every 50 ms: cooldown, target search by mode and team, aim with convergence, the weapon's fire) and its drop-pitch trace, ported from its executable"
    ),
    player_use: unknown!(
        "t5.turret.use",
        "a player cannot mount a usable Black Ops turret, so `getturretowner` answers undefined",
        "Black Ops' turret use (activate, the player's view and fire on the turret, dismount)"
    ),
};

/// The turret a `misc_turret` spawns as. Keys the map sets win over the
/// weapon's own definition; arcs are kept as limits on either side of the
/// spawn angles, times in whole ms rounded to nearest.
fn spawn(
    weapon: Option<&GameWeapon>,
    weapon_name: &str,
    key: &dyn Fn(&str) -> Option<f32>,
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
    let yaw_convergence = key("yawconvergencetime")
        .or_else(|| key("convergencetime"))
        .unwrap_or(def.yaw_convergence_time())
        .max(0.0);
    let pitch_convergence = float("pitchconvergencetime", def.pitch_convergence_time()).max(0.0);
    let suppression = float("suppressionTime", def.suppression_time()).max(0.0);
    let max_range = float("maxrange", def.max_range());
    let accuracy = float("accuracy", def.accuracy()).clamp(0.0, 1.0);
    Ok(TurretSpawn {
        flags: SPAWN_FLAGS,
        team: SPAWN_TEAM,
        arcs: TurretArcs {
            top,
            right,
            bottom,
            left,
        },
        spawned_yaw: [right, left],
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
