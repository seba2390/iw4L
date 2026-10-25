//! Black Ops' grenade indicators, read from its executable: each drawn
//! missile whose weapon asks for one and that is close enough, slow enough
//! and low enough gets a pointer turned toward it and an icon, around the
//! screen's centre.

use game_api::hud::{
    GrenadeIndicatorView, GrenadeIndicators, IndicatorMaterial, IndicatorPicture, IndicatorWeapon,
};
use game_api::unknown;
use movement_t5::math::{angle_normalize_180, vectoyaw};

/// The most missiles the indicators keep in a frame.
const MAX_INDICATORS: usize = 32;
/// Weapon types an indicator shows: grenades, mines and projectiles.
const WEAPTYPE_GRENADE: i32 = 1;
const WEAPTYPE_PROJECTILE: i32 = 2;
const WEAPTYPE_MINE: i32 = 6;
/// Explosion types that are grenades (frag, heavy explosive and fire);
/// a flashbang is shown too.
const EXPLOSION_GRENADE: i32 = 0;
const EXPLOSION_FLASHBANG: i32 = 2;
const EXPLOSION_HEAVY: i32 = 6;
const EXPLOSION_FIRE: i32 = 7;
const PM_SPECTATOR: i32 = 4;
/// `otherFlags`: the player's view hides the indicators.
const OTHER_FLAG_HIDE_INDICATORS: u32 = 2;
/// Stickiness kinds whose grenades are shown however high they are.
const STICK_ALL: i32 = 1;
const STICK_ALL_ORIENT: i32 = 2;

/// `cg_hudGrenadeIndicatorStartColor` and `…TargetColor`: vector dvars
/// the client keeps no values of; no zombies script sets them.
const START_COLOR: [f32; 3] = [1.0; 3];
const TARGET_COLOR: [f32; 3] = [1.0; 3];
/// `cg_hudGrenadePointerPivot`.
const POINTER_PIVOT: [f32; 2] = [12.0, 27.0];

fn grenade_like(explosion: i32) -> bool {
    matches!(
        explosion,
        EXPLOSION_GRENADE | EXPLOSION_HEAVY | EXPLOSION_FIRE
    )
}

pub(crate) fn draw(view: &GrenadeIndicatorView<'_>) -> GrenadeIndicators {
    let mut out = GrenadeIndicators::default();
    out.unrun.push(unknown!(
        "t5.hud.grenade_indicators",
        "Black Ops' grenade indicators are drawn, but the height test does not take the clearance above the player (a trace up from his origin every 50 ms, plus 70) as a limit; the throwback icon (the player's cursor hint on the grenade) is not drawn, the cursor hint not being kept; while a sight overlay shows they are not drawn (Black Ops centres them on the gun's projected position); and the screen scale Black Ops multiplies in is taken as 1",
        "the client trace up, the cursor hint in the player state, the gun position's projection and that scale (executable)"
    ));
    let dvar = |name: &str, default: f32| (view.dvar)(name).unwrap_or(default);
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    if ps.pm_type == PM_SPECTATOR
        || ps.other_flags & OTHER_FLAG_HIDE_INDICATORS != 0
        || view.third_person
    {
        return out;
    }
    if (view.weapon)(ps.weapon).is_some_and(|held| held.ads_overlay) && ps.weapon_pos_frac != 0.0 {
        return out;
    }
    let max_speed = dvar("bg_maxGrenadeIndicatorSpeed", 100.0);
    let friendly_fire = dvar("scr_friendlyfire", 0.0) != 0.0;
    let mut entries = Vec::new();
    for missile in view.missiles {
        if missile.time > view.time || entries.len() == MAX_INDICATORS {
            continue;
        }
        let Some(def) = (view.weapon)(missile.weapon) else {
            continue;
        };
        if !matches!(
            def.weap_type,
            WEAPTYPE_GRENADE | WEAPTYPE_MINE | WEAPTYPE_PROJECTILE
        ) || !(grenade_like(def.proj_explosion_type)
            || def.proj_explosion_type == EXPLOSION_FLASHBANG
            || def.show_indicator)
            || !def.timed_detonation
        {
            continue;
        }
        let danger = danger(view, &ps, missile.origin, &def);
        if danger == 0.0 || !(missile.owner_local || !missile.owner_teammate || !friendly_fire) {
            continue;
        }
        let speed_limit = if grenade_like(def.proj_explosion_type) {
            max_speed * max_speed
        } else {
            1.0
        };
        let [dx, dy, dz] = missile.delta;
        if dx * dx + dy * dy + dz * dz > speed_limit {
            continue;
        }
        let material = if grenade_like(def.proj_explosion_type) {
            if def.has_icon {
                IndicatorMaterial::WeaponIcon(missile.weapon)
            } else {
                IndicatorMaterial::Named("hud_grenadeicon")
            }
        } else if dvar("cg_hudGrenadeIconEnabledFlash", 0.0) != 0.0 {
            IndicatorMaterial::Named("hud_flashbangicon")
        } else {
            continue;
        };
        let fuse = if def.fuse_time_ms != 0 {
            ((view.time - missile.time) as f32 / def.fuse_time_ms as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        entries.push((missile.origin, 1.0 - danger, fuse, material));
    }
    let offset = dvar("cg_hudGrenadeIconOffset", 50.0);
    let fade_up = dvar("cg_hudGrenadeIndicatorFadeUp", 1.0) != 0.0;
    for (origin, near, fuse, material) in entries {
        let delta = std::array::from_fn::<_, 3, _>(|i| origin[i] - ps.origin[i]);
        let (rgb, alpha) = if fade_up {
            (
                std::array::from_fn::<_, 3, _>(|i| {
                    (TARGET_COLOR[i] - START_COLOR[i]) * fuse + START_COLOR[i]
                }),
                near.clamp(0.1, 1.0),
            )
        } else {
            let low = dvar("cg_hudGrenadePointerPulseMin", 0.3);
            let half = (dvar("cg_hudGrenadePointerPulseMax", 1.85) - low) * 0.5;
            let phase =
                view.time as f32 * dvar("cg_hudGrenadePointerPulseFreq", 1.7) * 0.006_283_185;
            ([1.0; 3], phase.sin() * half + low + half)
        };
        let alpha = if alpha <= 0.0 { 0.0 } else { alpha.min(1.0) };
        let color = [rgb[0], rgb[1], rgb[2], alpha];
        let yaw = angle_normalize_180(vectoyaw(delta) - ps.viewangles[1]);
        let (sin, cos) = yaw.to_radians().sin_cos();
        let at = [-sin * offset, -cos * offset];
        // The pointer turns by the negated yaw about its pivot, placed at
        // the icon's offset.
        let (w, h) = (
            dvar("cg_hudGrenadePointerWidth", 25.0),
            dvar("cg_hudGrenadePointerHeight", 12.0),
        );
        let turn = -yaw;
        let (ts, tc) = turn.to_radians().sin_cos();
        let (ox, oy) = (w * 0.5 - POINTER_PIVOT[0], h * 0.5 - POINTER_PIVOT[1]);
        out.pictures.push(IndicatorPicture {
            material: IndicatorMaterial::Named("hud_grenadepointer"),
            x: at[0] + ox * tc - oy * ts - w * 0.5,
            y: at[1] + ox * ts + oy * tc - h * 0.5,
            w,
            h,
            rotation: turn,
            color,
        });
        let (w, h) = (
            dvar("cg_hudGrenadeIconWidth", 25.0),
            dvar("cg_hudGrenadeIconHeight", 25.0),
        );
        out.pictures.push(IndicatorPicture {
            material,
            x: at[0] - w * 0.5,
            y: at[1] - h * 0.5,
            w,
            h,
            rotation: 0.0,
            color,
        });
    }
    out
}

/// How near the missile is within its indicator range (0 when outside it
/// or too far above or below the player), as a fraction of the range
/// squared, at least 0.001.
fn danger(
    view: &GrenadeIndicatorView<'_>,
    ps: &movement_t5::state::PlayerState,
    origin: [f32; 3],
    def: &IndicatorWeapon,
) -> f32 {
    let dvar = |name: &str, default: f32| (view.dvar)(name).unwrap_or(default);
    let delta = std::array::from_fn::<_, 3, _>(|i| origin[i] - ps.origin[i]);
    let range = if grenade_like(def.proj_explosion_type) || def.show_indicator {
        if def.indicator_radius != 0 {
            def.indicator_radius as f32
        } else {
            let frag = dvar("cg_hudGrenadeIconMaxRangeFrag", 250.0);
            if frag == 0.0 {
                0.0
            } else {
                frag.max(def.explosion_radius as f32)
            }
        }
    } else {
        dvar("cg_hudGrenadeIconMaxRangeFlash", 500.0)
    };
    let dist2 = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
    if dist2 > range * range {
        return 0.0;
    }
    let max_height = dvar("cg_hudGrenadeIconMaxHeight", 104.0);
    if delta[2] >= 0.0 {
        if !matches!(def.stickiness, STICK_ALL | STICK_ALL_ORIENT)
            && delta[2] > ps.view_height_current + max_height
        {
            return 0.0;
        }
    } else if delta[2] < -max_height {
        return 0.0;
    }
    let frac = dist2 / (range * range);
    if frac <= 0.001 { 0.001 } else { frac.min(1.0) }
}
