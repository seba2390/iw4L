//! Black Ops zombies' code-drawn HUD, read from its executable: the
//! crosshair, the damage icons and the TAB scoreboard. Black Ops draws no
//! blood overlay of its own; zombies' damage overlay is scripted
//! (`_gameskill`).

use game_api::hud::{
    CodeScoreboard, CommandHint, Crosshair, CrosshairPicture, CrosshairView, DamageFlash,
    DamageIcons, GameCodeHud, OwnerHint, OwnerHintView, ScoreAlign, ScoreColumn, ScoreValue,
};
use game_api::{Unknown, unknown};
use movement_t5::weapon::state as ws;

pub(crate) static CODE_HUD: GameCodeHud = GameCodeHud {
    blood: false,
    damage_flash: Some(DamageFlash {
        kick_scale: 0.2,
        kick_min: 0.5,
        kick_max: 90.0,
        kick_random: 0.4,
        time_ms: 500,
        alpha_per_ms: 0.002,
        alpha_max: 5.0,
        color: [0.2, 0.0, 0.0, 0.2],
        alpha_scale: 0.7,
        corner: -10.0,
        grow: 10.0,
    }),
    damage_icons: DamageIcons {
        slots: 8,
        time_ms: 2000,
        hidden_by_flash_and_mantle: false,
        stun_material: None,
    },
    crosshair,
    scoreboard: Some(&SCOREBOARD),
    owner_hint,
    weapon_info: Some(&crate::weapon_info::WEAPON_INFO),
    command_hint: Some(command_hint),
    undrawn: &[unknown!(
        "t5.hud.code_hud",
        "Black Ops' other code-drawn HUD pieces are not drawn: grenade indicators, friendly names, the players-in-view scan, chat lines and the night vision overlay",
        "those pieces of its 2D draw (executable)"
    )],
};

/// Black Ops' invalid command hints, by its number for them.
pub(crate) const HINT_NO_AMMO: i32 = 1;
pub(crate) const HINT_NO_FRAG_GRENADE: i32 = 2;
pub(crate) const HINT_NO_SPECIAL_GRENADE: i32 = 3;
pub(crate) const HINT_NO_EQUIPMENT: i32 = 5;
pub(crate) const HINT_STAND_BLOCKED: i32 = 6;
pub(crate) const HINT_CROUCH_BLOCKED: i32 = 7;
pub(crate) const HINT_TARGET_TOO_CLOSE: i32 = 8;
pub(crate) const HINT_LOCKON_REQUIRED: i32 = 9;
pub(crate) const HINT_TARGET_NOT_ENOUGH_CLEARANCE: i32 = 10;
/// `cg_invalidCmdHintDuration`, `cg_invalidCmdHintBlinkInterval`.
const INVALID_CMD_HINT_DURATION: f32 = 1800.0;
const INVALID_CMD_HINT_BLINK_INTERVAL: f32 = 600.0;
/// The last movement type that shows the invalid command hint.
const PM_TYPE_LAST_HINT: i32 = 8;

/// Black Ops' client keeps one invalid command hint: a new one replaces a
/// different one and starts its time, the same one keeps its time; it is
/// let go `cg_invalidCmdHintDuration` after its time.
fn command_hint(kept: CommandHint, event: Option<i32>, now: i32) -> CommandHint {
    let mut kept = kept;
    if kept.kind != 0 && INVALID_CMD_HINT_DURATION as i32 + kept.time < now {
        kept.kind = 0;
    }
    match event {
        Some(kind) if kind != kept.kind => CommandHint { kind, time: now },
        _ => kept,
    }
}

/// The invalid command hint's text, by its number; the others draw
/// nothing.
fn command_hint_key(kind: i32) -> Option<&'static str> {
    Some(match kind {
        HINT_NO_AMMO => "WEAPON_NO_AMMO",
        HINT_NO_FRAG_GRENADE => "WEAPON_NO_FRAG_GRENADE",
        HINT_NO_SPECIAL_GRENADE => "WEAPON_NO_SPECIAL_GRENADE",
        HINT_NO_EQUIPMENT => "WEAPON_NO_EQUIPMENT",
        HINT_STAND_BLOCKED => "GAME_STAND_BLOCKED",
        HINT_CROUCH_BLOCKED => "GAME_CROUCH_BLOCKED",
        HINT_TARGET_TOO_CLOSE => "WEAPON_TARGET_TOO_CLOSE",
        HINT_LOCKON_REQUIRED => "WEAPON_LOCKON_REQUIRED",
        HINT_TARGET_NOT_ENOUGH_CLEARANCE => "WEAPON_TARGET_NOT_ENOUGH_CLEARANCE",
        _ => return None,
    })
}

/// Black Ops' hint and message owner draws.
const CG_HOLD_BREATH: i32 = 71;
const CG_MANTLE_HINT: i32 = 80;
const CG_CENTER_OBITUARY: i32 = 90;
const CG_INVALID_CMD_HINT: i32 = 113;
const CG_SUCCESSFUL_CMD_HINT: i32 = 132;
const CG_VEHICLE_HUD: i32 = 192;
const CG_TALKER_1: i32 = 195;
const CG_TALKER_3: i32 = 197;
const CG_FRIENDLY_ARROWS: i32 = 199;
const CG_FRIENDLY_NAMES: i32 = 200;
const CG_HOLD_BREATH_ZOOM: i32 = 311;
const CG_WRISTWATCH: i32 = 316;
const CG_OBJECTIVE_INDICATORS: i32 = 322;

/// Entity flags that hide the breath hints: on a vehicle or a turret, or
/// the other crosshair.
const EF_NO_BREATH_HINT: u32 = EF_VEHICLE_TURRET | EF_OTHER_CROSSHAIR;
/// `mantleState.flags`: a mantle is offered; a mount is offered.
const MANTLE_OFFERED: u32 = 0x10;
const MOUNT_OFFERED: u32 = 0x80;
/// How far the mantle hint rises when a mount is offered too.
const MANTLE_RAISE_WITH_MOUNT: f32 = -35.0;
/// The weapon state in which neither offer shows.
const WEAPON_DEPLOYING: i32 = 0x24;
/// The runtime's number for Black Ops' item weapon class (10).
const WEAPCLASS_ITEM: i32 = 11;

/// Black Ops' hint and message owner draws, read from its executable's text
/// and picture owner draw dispatches. The zombies-only gate of the breath
/// hint (the client's view linked to a camera) never holds: the client's
/// `linktocamera` is not implemented.
fn owner_hint(owner_draw: i32, view: &OwnerHintView<'_>) -> OwnerHint {
    let dvar = |name: &str, default: f32| (view.dvar)(name).unwrap_or(default) != 0.0;
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    match owner_draw {
        CG_HOLD_BREATH | CG_HOLD_BREATH_ZOOM => {
            if !dvar("cg_drawBreathHint", 1.0)
                || view.alternate_aim
                || ps.weap_flags & movement_t5::state::weap_flags::HOLD_BREATH != 0
                || ps.e_flags & EF_NO_BREATH_HINT != 0
            {
                return OwnerHint::Nothing;
            }
            if owner_draw == CG_HOLD_BREATH
                && (!dvar("cg_drawpaused", 1.0) || view.scoreboard || view.paused)
            {
                return OwnerHint::Nothing;
            }
            let Some(weapon) = (view.weapon)(ps.weapon) else {
                return OwnerHint::Nothing;
            };
            if !weapon.ads_overlay
                || weapon.weap_class == WEAPCLASS_ITEM
                || ps.weapon_pos_frac != 1.0
            {
                return OwnerHint::Nothing;
            }
            if owner_draw == CG_HOLD_BREATH {
                OwnerHint::Text {
                    key: "PLATFORM_HOLD_BREATH",
                    binds: &["+holdbreath", "+melee_breath", "+breath_sprint"],
                }
            } else if weapon.zoom_varies {
                OwnerHint::Text {
                    key: "PLATFORM_HOLD_BREATH_ZOOM",
                    binds: &["+melee"],
                }
            } else {
                OwnerHint::Nothing
            }
        }
        CG_MANTLE_HINT => {
            if !dvar("cg_drawMantleHint", 1.0)
                || !dvar("cg_drawpaused", 1.0)
                || view.scoreboard
                || view.paused
                || ps.weaponstate == WEAPON_DEPLOYING
            {
                return OwnerHint::Nothing;
            }
            // With a mount offered too, the mantle hint is raised above it;
            // the runtime draws one hint per owner draw, the mantle's.
            if ps.mantle_flags & MANTLE_OFFERED != 0 {
                OwnerHint::TextPicture {
                    key: "PLATFORM_MANTLE",
                    binds: &["+gostand", "+moveup"],
                    material: "hint_mantle",
                    raise: if ps.mantle_flags & MOUNT_OFFERED != 0 {
                        MANTLE_RAISE_WITH_MOUNT
                    } else {
                        0.0
                    },
                }
            } else if ps.mantle_flags & MOUNT_OFFERED != 0 {
                OwnerHint::Text {
                    key: "PLATFORM_MOUNT",
                    binds: &["+usereload", "+activate"],
                }
            } else {
                OwnerHint::Nothing
            }
        }
        CG_INVALID_CMD_HINT => {
            let duration = (view.dvar)("cg_invalidCmdHintDuration")
                .unwrap_or(INVALID_CMD_HINT_DURATION) as i32;
            let hint = view.command_hint;
            if hint.kind == 0 || duration + hint.time < view.now || ps.pm_type > PM_TYPE_LAST_HINT {
                return OwnerHint::Nothing;
            }
            let Some(key) = command_hint_key(hint.kind) else {
                return OwnerHint::Nothing;
            };
            // It blinks in: its alpha climbs from 0 to 1 over each interval.
            let interval = (view.dvar)("cg_invalidCmdHintBlinkInterval")
                .unwrap_or(INVALID_CMD_HINT_BLINK_INTERVAL) as i32;
            if interval <= 0 {
                return OwnerHint::Nothing;
            }
            let alpha = ((view.now - hint.time) % interval) as f32 / interval as f32;
            OwnerHint::Faded { key, alpha }
        }
        // Black Ops' single player sets the centre obituary only from the
        // `obituary` builtin, which no zombies script calls, and never sets
        // the cheat message; both stay empty.
        CG_CENTER_OBITUARY | CG_SUCCESSFUL_CMD_HINT => OwnerHint::Nothing,
        // No client talks: the runtime carries no voice chat.
        CG_TALKER_1..=CG_TALKER_3 => OwnerHint::Nothing,
        CG_VEHICLE_HUD => {
            if ps.e_flags & EF_OTHER_CROSSHAIR == 0 {
                OwnerHint::Nothing
            } else {
                OwnerHint::Unrun(unknown!(
                    "t5.hud.vehicle_hud",
                    "Black Ops' vehicle HUD is not drawn",
                    "its two vehicle HUD draws (executable); zombies has no vehicles"
                ))
            }
        }
        CG_FRIENDLY_ARROWS | CG_FRIENDLY_NAMES | CG_WRISTWATCH | CG_OBJECTIVE_INDICATORS => {
            OwnerHint::Unrun(unknown!(
                "t5.hud.markers",
                "Black Ops' friendly arrows and names, wristwatch and objective indicators are not drawn",
                "those picture owner draws (executable)"
            ))
        }
        _ => OwnerHint::NotHint,
    }
}

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
        "Black Ops' crosshair: a weapon's sight overlay and the crosshair fade it brings, the vehicle and turret crosshairs, the vision-set lerp and timed screen effects that hide it, and the stereo 3D depth are not run",
        "those pieces of the crosshair draw (executable)"
    )
}

/// The crosshair's colour: white, unless the weapon changes it (or the
/// player is on a vehicle) and the server's crosshair scan marked what it
/// is on: a friend in its team's colour, an enemy in the enemy team's
/// (while `cg_crosshairEnemyColor`), the human gun's target in its own
/// (`team_indicator` 3 takes the alternate team colours).
fn crosshair_rgb(
    view: &CrosshairView<'_>,
    ps: &movement_t5::state::PlayerState,
    def: game_api::hud::CrosshairWeapon,
) -> [f32; 3] {
    use movement_t5::state::weap_flags::{CANCEL_GRENADE, ENEMY_IN_CROSSHAIR, ZOMBIE_GUN_TARGET};
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
    if !def.color_change && ps.e_flags & EF_OTHER_CROSSHAIR == 0 {
        return WHITE;
    }
    let dvar = |name: &str, default: f32| (view.dvar)(name).unwrap_or(default);
    let alt = dvar("team_indicator", 0.0) == 3.0;
    let flags = ps.weap_flags;
    if flags & CANCEL_GRENADE != 0 {
        // `g_TeamColor_MyTeam` / `_MyTeamAlt`.
        return if alt {
            [0.35, 1.0, 1.0]
        } else {
            [0.4, 0.7, 0.4]
        };
    }
    if flags & ENEMY_IN_CROSSHAIR != 0 && dvar("cg_crosshairEnemyColor", 1.0) != 0.0 {
        // `g_TeamColor_EnemyTeam` / `_EnemyTeamAlt`.
        return if alt {
            [1.0, 0.5, 0.0]
        } else {
            [1.0, 0.315, 0.35]
        };
    }
    if dvar("zombiemode", 0.0) == 0.0 || flags & ZOMBIE_GUN_TARGET == 0 {
        return WHITE;
    }
    if def.human_gun {
        [1.0, 1.0, 0.0]
    } else {
        [0.0, 1.0, 1.0]
    }
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
    let rgb = crosshair_rgb(view, &ps, def);
    let white = [rgb[0], rgb[1], rgb[2], alpha];
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
    let color = [rgb[0], rgb[1], rgb[2], side_alpha];
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
