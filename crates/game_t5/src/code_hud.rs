//! Black Ops zombies' code-drawn HUD, read from its executable: the
//! crosshair, the damage icons and the TAB scoreboard. Black Ops draws no
//! blood overlay of its own; zombies' damage overlay is scripted
//! (`_gameskill`).

use game_api::hud::{
    CodeScoreboard, Crosshair, CrosshairPicture, CrosshairView, DamageIcons, GameCodeHud,
    ScoreAlign, ScoreColumn, ScoreValue,
};
use game_api::{Unknown, unknown};
use movement_t5::weapon::state as ws;

pub(crate) static CODE_HUD: GameCodeHud = GameCodeHud {
    blood: false,
    damage_icons: DamageIcons {
        slots: 8,
        time_ms: 2000,
        hidden_by_flash_and_mantle: false,
        stun_material: None,
    },
    crosshair,
    scoreboard: Some(&SCOREBOARD),
    undrawn: &[unknown!(
        "t5.hud.code_hud",
        "Black Ops' other code-drawn HUD pieces are not drawn: grenade indicators, friendly names, the players-in-view scan, chat lines, the red damage flash, the burning, electrified and teleport overlays and the night vision overlay",
        "those pieces of its 2D draw (executable)"
    )],
};

/// The weapon types whose cooked time grows the centre picture: grenades
/// and mines.
const WEAPTYPE_GRENADE: i32 = 1;
const WEAPTYPE_MINE: i32 = 6;

/// Entity flags: on a vehicle or a turret (its own crosshair), and the
/// other crosshair the flag `0x4000` draws.
const EF_VEHICLE_TURRET: u32 = 0x300;
const EF_OTHER_CROSSHAIR: u32 = 0x4000;

/// `weapFlags`: the offhand is out.
const WEAPFLAG_OFFHAND: u32 = 0x2;

const DEG2RAD: f32 = 0.017_453_292;

/// The C library's tangent, taken in double precision as Black Ops does.
fn tan(x: f32) -> f32 {
    f64::from(x).tan() as f32
}

fn crosshair_unrun() -> &'static Unknown {
    unknown!(
        "t5.hud.crosshair",
        "Black Ops' crosshair draws white: its colour over a friend or an enemy needs the client's crosshair entity scan, which is not run; a weapon's sight overlay and the crosshair fade it brings, the vehicle and turret crosshairs, the vision-set lerp and timed screen effects that hide it, and the stereo 3D depth are not run either",
        "the client crosshair entity scan and those pieces of the crosshair draw (executable)"
    )
}

/// Black Ops' crosshair: the weapon's centre picture, and its side picture
/// four times turned, pushed out by the gun's hip spread.
fn crosshair(view: &CrosshairView<'_>) -> Crosshair {
    let mut out = Crosshair::default();
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    if view.third_person || (ws::LOWREADY_RAISE..=ws::LOWREADY_DROP).contains(&ps.weaponstate) {
        return out;
    }
    // A dead player draws one only when his flags ask for it (the other
    // crosshair's own test of the vehicle type is not run: that crosshair
    // is not drawn).
    if ps.pm_type > movement_t5::state::pm_type::LAST_STAND_GETTING_UP
        && ps.e_flags & EF_OTHER_CROSSHAIR == 0
        && (ps.e_flags2 >> 28) & 1 == 0
    {
        return out;
    }
    let dvar = |name: &str, default: f32| (view.dvar)(name).unwrap_or(default);
    if ps.e_flags & (EF_VEHICLE_TURRET | EF_OTHER_CROSSHAIR) != 0 {
        out.unrun.push(crosshair_unrun());
        return out;
    }
    if dvar("show_reticle_during_swimming", 0.0) == 0.0 && ps.water_level >= 3 {
        return out;
    }
    let weapon = if ps.weap_flags & WEAPFLAG_OFFHAND != 0
        && ps.weaponstate > ws::MELEE_END
        && ps.weaponstate < ws::DETONATING
    {
        ps.offhand_index
    } else {
        ps.weapon
    };
    if weapon == 0 {
        return out;
    }
    let Some(def) = (view.weapon)(weapon) else {
        return out;
    };
    let held = (view.weapon)(ps.weapon).unwrap_or(def);
    out.unrun.push(crosshair_unrun());
    let frac = ps.weapon_pos_frac;
    if def.ads_overlay && frac != 0.0 {
        return out;
    }
    let alpha = dvar("cg_crosshairAlpha", 1.0);
    if !(0.01 <= alpha) {
        return out;
    }
    let keep = dvar("cg_forceSniperBobHack", 0.0) != 0.0 || def.keep_crosshair_when_ads;
    if frac == 1.0 && !keep {
        return out;
    }
    if (view.paused && dvar("cg_drawpaused", 1.0) != 0.0)
        || dvar("cg_drawCrosshair", 1.0) == 0.0
        || dvar("hud_missionFailed", 0.0) != 0.0
        || (ws::MELEE_INIT..=ws::MELEE_END).contains(&ps.weaponstate)
    {
        return out;
    }
    let reloading = if held.dual_wield {
        ps.weaponstate == ws::RELOADING && ps.weaponstate_left == ws::RELOADING
    } else {
        ps.weaponstate == ws::RELOADING
    };
    if reloading || (ws::RAISING..=ws::DROPPING_ALT).contains(&ps.weaponstate) {
        return out;
    }
    let tan_half = view.tan_half_fov_y;
    // Raising the sights shrinks the crosshair and drops it to the sights'
    // aim pitch.
    let (mut scale, mut y) = (1.0f32, 0.0f32);
    if frac != 0.0 && !def.keep_crosshair_when_ads {
        let window = if view.to_ads {
            def.ads_crosshair_in_frac
        } else {
            def.ads_crosshair_out_frac
        };
        let t = frac - (1.0 - window);
        if 0.0 < t {
            let t = t / window;
            scale = 1.0 - t * 0.5;
            y = tan(def.ads_aim_pitch * DEG2RAD) / tan_half * t * 240.0;
        }
    }
    let x = 0.0;
    let white = [1.0, 1.0, 1.0, alpha];
    if def.has_center {
        let mut size = def.center_size as f32;
        if matches!(def.weap_type, WEAPTYPE_GRENADE | WEAPTYPE_MINE)
            && def.cook_off_hold
            && ps.grenade_time_left != 0
        {
            size += (ps.grenade_time_left % 1000) as f32 * 0.01;
        }
        size *= scale;
        out.pictures.push(CrosshairPicture {
            side: false,
            x: x - size * 0.5,
            y: y - size * 0.5,
            w: size,
            h: size,
            rotation: 0.0,
            color: white,
        });
    }
    if !def.has_side {
        return out;
    }
    let aim = ps.aim_spread_scale * (1.0 / 255.0);
    let mut side_alpha = (1.0 - aim) * alpha;
    let alpha_min = dvar("cg_crosshairAlphaMin", 0.5);
    if side_alpha < alpha_min {
        side_alpha = alpha_min;
    }
    if !(0.01 <= side_alpha) {
        return out;
    }
    let s = def.side_size as f32 * scale;
    let (min, max) = crate::fire::hip_spread(
        &ps,
        def.hip_spread_min,
        def.hip_spread_max,
        Some(dvar("perk_weapSpreadMultiplier", 0.65)),
    );
    let mut v = tan((aim * (max - min) + min) * scale * DEG2RAD) / tan_half * 240.0;
    let min_ofs = def.min_ofs as f32;
    if v < min_ofs {
        v = min_ofs;
    }
    let gx = v - s * def.side_pos;
    let gy = v - s * def.side_pos;
    let e = s * 0.125;
    let h = s * 0.5;
    let color = [1.0, 1.0, 1.0, side_alpha];
    for (px, py, rotation) in [
        (x - h, y - s - gy - e, 0.0),
        (x + gx, y - h, 90.0),
        (x - h - e, y + gy, 180.0),
        (x - s - gx - e, y - h - e, 270.0),
    ] {
        out.pictures.push(CrosshairPicture {
            side: true,
            x: px,
            y: py,
            w: s,
            h: s,
            rotation,
            color,
        });
    }
    out
}

/// The solo zombies scoreboard: name, points, kills and headshots. Online
/// and system-link games add the voice, downs, revives and ping columns;
/// they are not drawn here. The colours are the ones `code_pre_gfx`'s
/// `default.cfg` sets over the executable's defaults (red, green and blue
/// only: the alpha reads as 1, as the multiplayer board's rows, drawn with
/// the same alpha rule, show).
static SCOREBOARD: CodeScoreboard = CodeScoreboard {
    width: 500.0,
    height: 330.0,
    item_height: 18.0,
    top_offset: 10.0,
    header_offset: 42.0,
    header_height: 35.0,
    inset: 9.0,
    row_gap: 2.0,
    bottom_room: 3.0 + 2.0 + 14.0 + 1.0,
    font: 3,
    header_scale: 0.315,
    row_scale: 0.35,
    scale_step: 0.025,
    plain_below: 0.2,
    text_offset: 0.62,
    columns: &[
        ScoreColumn {
            value: ScoreValue::Name,
            width: 0.625,
            title: "",
            align: ScoreAlign::Center,
        },
        ScoreColumn {
            value: ScoreValue::Score,
            width: 0.125,
            title: "CGAME_SB_POINTS",
            align: ScoreAlign::Center,
        },
        ScoreColumn {
            value: ScoreValue::Kills,
            width: 0.125,
            title: "CGAME_SB_KILLS",
            align: ScoreAlign::Center,
        },
        ScoreColumn {
            value: ScoreValue::Headshots,
            width: 0.125,
            title: "CGAME_SB_HEADSHOTS",
            align: ScoreAlign::Center,
        },
    ],
    bar_materials: &[
        "scorebar_zom_long_1",
        "scorebar_zom_long_2",
        "scorebar_zom_long_3",
        "scorebar_zom_long_4",
    ],
    bar_color: [0.424, 0.004, 0.0],
    bar_alpha: 0.35,
    name_colors: &[
        [1.0, 1.0, 1.0, 1.0],
        [0.486, 0.812, 0.933, 1.0],
        [0.965, 0.792, 0.314, 1.0],
        [0.514, 0.925, 0.533, 1.0],
    ],
    fade_ms: 100,
    footer_height: 14.0,
    footer_bottom: 5.0,
    footer_scale: 0.2,
    footer_step: 0.01,
    footer_floor: 0.075,
    hostname: "BlackOpsZombie",
};
