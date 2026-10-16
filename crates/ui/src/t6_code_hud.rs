//! Black Ops 2's code-drawn HUD pieces, drawn as its client draws them: from
//! the legacy HUD menus its zombies client loads (`ui_mp/hud_zombies.txt`,
//! `SessionCodeHudMenus`), each owner draw by its own rule, placed by Black
//! Ops 2's screen placement (`game_t6::scrplace`) in its own UI fonts.
//!
//! The cursor hint (`cursorhints`, owner draw 72): each frame the player
//! state carries a hint, its start is now, its fade 100 msec (2000 for hint
//! 2), its kind and string the player state's; the hint's alpha is its fade
//! left over 100 msec once under 100. Hints 1 and 2 draw the hint string,
//! localized with the use key, centred on the screen's x and half its height
//! below the item's y.

use asset_core::AssetNamespace;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use game_api::ui::{Content, Draw};
use game_t6 as t6;
use hud_iw4 as hud;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};
use playerstate_iw4 as ps;
use std::collections::BTreeSet;
use weapon_iw4 as w_iw4;

/// The cursor hint owner draw (`CG_CURSORHINT`).
const OWNERDRAW_CURSORHINT: i32 = 72;
/// Draw ids of the code HUD, above the LUI scene's.
pub(crate) const CODE_HUD_DRAW_ID: u32 = 0x8000_0000;
const CODE_HUD_CROSSHAIR_ID: u32 = 0x8000_8000;
const CODE_HUD_HIDE_FADE_ID: u32 = 0x8000_7fff;
const CODE_HUD_MENU_DRAW_ID: u32 = 0x8001_0000;
const CODE_HUD_FRONT_ELEM_ID: u32 = 0x8002_0000;

/// Black Ops 2's text colours `^0`..`^7`.
const COLOR_CODES: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.2, 0.2],
    [0.0, 1.0, 0.0],
    [1.0, 1.0, 0.5],
    [0.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [1.0, 0.36, 1.0],
    [1.0, 1.0, 1.0],
];

/// The code HUD's draws this frame, in the LUI scene's 720-high space:
/// background server `HudElem`s (`foreground == 0`) behind the LUI HUD, and
/// legacy HUD menus plus foreground server `HudElem`s in front.
#[derive(Resource, Default)]
pub(crate) struct CodeHudDraws {
    pub(crate) back: Vec<Draw>,
    pub(crate) front: Vec<Draw>,
}

/// The gump hide fade and the last `sethidegumpalpha` it started from.
#[derive(Default)]
struct HideFadeView {
    fade: t6::client_screen::HideFade,
    seen: Option<i32>,
}

/// The cursor hint the client game keeps between frames.
#[derive(Default)]
struct CursorHint {
    kind: i32,
    string: i32,
    start_ms: i32,
    fade_ms: i32,
    last_ads_frac: f32,
    position_to_ads: bool,
    logged: Option<(i32, i32)>,
    logged_elems: BTreeSet<String>,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<CodeHudDraws>().add_systems(
        Update,
        update
            .in_set(frame::ClientSet::Ui)
            .before(super::script_ui::ScriptUiPaint),
    );
}

/// A run of text in one colour, and its width in font pixels.
struct Run {
    text: String,
    color: [f32; 3],
    width: f32,
}

/// The text's runs by colour code, each measured as `R_TextWidth` does:
/// `^` and a digit or `F` change colour (or are skipped), `^C<digit>` and
/// `^L` are skipped.
// t6.hud.code_hud: `^8`, `^9` and `^F` colours, `^B` button icons and the
// text styles' shadows are drawn as IW4L's T6 text renderer draws them.
fn runs(text: &str, font: &asset_material::ui_font::UiBitmapFont, base: [f32; 3]) -> Vec<Run> {
    let mut runs = vec![Run {
        text: String::new(),
        color: base,
        width: 0.0,
    }];
    let mut chars = text.chars().peekable();
    while let Some(letter) = chars.next() {
        if letter == '^' {
            match chars.peek().copied() {
                Some(code @ '0'..='7') => {
                    chars.next();
                    runs.push(Run {
                        text: String::new(),
                        color: COLOR_CODES[(code as u8 - b'0') as usize],
                        width: 0.0,
                    });
                    continue;
                }
                Some('8' | '9' | 'F') => {
                    chars.next();
                    continue;
                }
                Some('C') => {
                    chars.next();
                    if chars.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                    }
                    continue;
                }
                Some('L') => {
                    chars.next();
                    continue;
                }
                _ => {}
            }
        }
        let run = runs.last_mut().unwrap();
        run.width += font
            .glyphs
            .get(&(letter as u16))
            .map_or(0.0, |glyph| f32::from(glyph.dx));
        run.text.push(letter);
    }
    runs.retain(|run| !run.text.is_empty());
    runs
}

/// The hint string the player state names, localized with the use key
/// (`CG_GetHintString`).
// t6.hud.code_hud: the key binding text is the one IW4L's Black Ops 2 HUD
// resolves (`[{+activate}]` and `&&1` take the use key's name).
fn hint_string(
    namespace: AssetNamespace,
    snapshot: &sim::Snapshot,
    strings: &PreparedLocalizedStrings,
    use_key: &str,
    index: i32,
) -> Option<String> {
    let raw = sim::hud_string_in_occupied(&snapshot.meta.hud_strings, index)?;
    let text = if let Some(plain) = raw.strip_prefix(sim::HUD_STRING_PLAIN) {
        plain.to_owned()
    } else {
        let mut parts = raw.split(sim::HUD_PRINT_ARG_SEPARATOR);
        let mut text = strings.0.text_in(namespace, parts.next()?)?.to_owned();
        for (index, value) in parts.enumerate() {
            text = text.replace(&format!("&&{}", index + 1), value);
        }
        text
    };
    Some(
        hud::replace_directive(&text, |command| {
            if matches!(command, "+activate" | "+usereload") {
                use_key.to_owned()
            } else {
                String::new()
            }
        })
        .replace("&&1", use_key),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_hud_elems(
    namespace: AssetNamespace,
    snapshot: &sim::Snapshot,
    local: sim::ClientId,
    ps: &ps::PlayerState,
    strings: &PreparedLocalizedStrings,
    use_key: &str,
    menu_open: bool,
    now: i32,
    placement: &t6::scrplace::Placement,
    ui_small_font: f32,
    ui_big_font: f32,
    to_canvas: f32,
    foreground: bool,
    base_id: u32,
    out: &mut Vec<Draw>,
    logged: &mut BTreeSet<String>,
) {
    let Some(meta) = snapshot.meta.for_client(local) else {
        return;
    };
    let alive = meta.lifecycle == sim::ClientLifecycle::Alive && ps.health > 0;
    let in_scope = ps.pm_type == 5;
    let zoomed = ps.f_weapon_pos_frac >= 1.0;
    let mut elems: Vec<&hud::HudElem> = hud::copy_in_use_prefix(&meta.hud_current)
        .iter()
        .chain(hud::copy_in_use_prefix(&meta.hud_archival).iter())
        .collect();
    elems.sort_by(|a, b| a.sort.total_cmp(&b.sort));
    for elem in elems {
        // `CG_Draw2dHudElemList`: background vs foreground pass and hide flags.
        if ((elem.flags & 1 != 0) != foreground)
            || (elem.flags & 2 != 0 && !alive)
            || (elem.flags & 4 != 0 && menu_open)
            || (elem.flags & 0x40 != 0 && in_scope)
            || (elem.flags & 0x2000 != 0 && zoomed)
        {
            continue;
        }
        let rgba = hud::lerp_hud_colors(elem, now);
        if rgba[3] == 0 {
            continue;
        }
        let color = rgba.map(|c| f32::from(c) / 255.0);
        let (font_enum, scale_mult) = t6::scrplace::hud_elem_font(elem.font);
        let scale = hud::hud_elem_lerp_font_scale(elem, now) * scale_mult;
        let font_name =
            t6::scrplace::menu_font(font_enum, scale, placement, ui_small_font, ui_big_font);
        let Some(font) = asset_material::ui_font::ui_font(namespace, font_name) else {
            continue;
        };
        let pixel_height = font.pixel_height.max(1) as f32;
        let normalized = scale * 48.0 / pixel_height;
        let font_height = (pixel_height * normalized + 0.5).floor();

        if elem.elem_type == hud::HE_TYPE_MATERIAL {
            let Some(raw_name) = u8::try_from(elem.material_index)
                .ok()
                .and_then(|i| sim::name_in_occupied(&snapshot.meta.hud_materials, i))
            else {
                continue;
            };
            let name = asset_core::AssetRef::bare_name(raw_name);
            let w = placement.hud_elem_width(
                elem.width,
                elem.align_screen,
                elem.from_width,
                elem.from_align_screen,
                elem.scale_start_time,
                elem.scale_time,
                now,
                font_height,
            );
            let h = placement.hud_elem_height(
                elem.height,
                elem.align_screen,
                elem.from_height,
                elem.from_align_screen,
                elem.scale_start_time,
                elem.scale_time,
                now,
                font_height,
            );
            let [x, y] = placement.hud_elem_pos(
                elem.x,
                elem.y,
                elem.align_org,
                elem.align_screen,
                elem.from_x,
                elem.from_y,
                elem.from_align_org,
                elem.from_align_screen,
                elem.move_start_time,
                elem.move_time,
                elem.font_scale_start_time,
                elem.font_scale_time,
                now,
                w,
                h,
            );
            if logged.insert(format!("mat:{foreground}:{name}")) {
                diag::info!(
                    Ui,
                    "t6 code HUD: hudelem material fg={} name={} at=({:.0},{:.0}) size=({:.0},{:.0}) alpha={:.2}",
                    foreground,
                    name,
                    x,
                    y,
                    w,
                    h,
                    color[3]
                );
            }
            let id = base_id + out.len() as u32;
            out.push(Draw {
                id,
                rectangle: [
                    x * to_canvas,
                    y * to_canvas,
                    (x + w) * to_canvas,
                    (y + h) * to_canvas,
                ],
                color,
                rotation: 0.0,
                alignment: 0,
                clip: None,
                content: Content::Image(name.to_owned()),
                text_shadow: false,
            });
            continue;
        }

        let body = match elem.elem_type {
            hud::HE_TYPE_TEXT if elem.text > 0 => {
                hint_string(namespace, snapshot, strings, use_key, elem.text)
            }
            hud::HE_TYPE_VALUE => Some(format!("{}", elem.value)),
            hud::HE_TYPE_TIMER_DOWN | hud::HE_TYPE_TIMER_UP | hud::HE_TYPE_TIMER_STATIC => {
                let ms = match elem.elem_type {
                    hud::HE_TYPE_TIMER_DOWN => (elem.time - now + 999).max(0),
                    hud::HE_TYPE_TIMER_UP => (now - elem.time).max(0),
                    _ => elem.time.max(0),
                };
                let secs = ms / 1000;
                Some(format!("{}:{:02}", secs / 60, secs % 60))
            }
            hud::HE_TYPE_TENTHS_TIMER_DOWN
            | hud::HE_TYPE_TENTHS_TIMER_UP
            | hud::HE_TYPE_TENTHS_TIMER_STATIC => {
                let ms = match elem.elem_type {
                    hud::HE_TYPE_TENTHS_TIMER_DOWN => (elem.time - now + 99).max(0),
                    hud::HE_TYPE_TENTHS_TIMER_UP => (now - elem.time).max(0),
                    _ => elem.time.max(0),
                };
                let tenths = ms / 100;
                Some(format!(
                    "{}:{:02}.{}",
                    tenths / 600,
                    (tenths / 10) % 60,
                    tenths % 10
                ))
            }
            _ => None,
        };
        let label = (elem.label > 0)
            .then(|| hint_string(namespace, snapshot, strings, use_key, elem.label))
            .flatten();
        let text = match (label, body) {
            (Some(l), Some(b)) => {
                if l.contains("&&1") {
                    l.replacen("&&1", &b, 1)
                } else {
                    format!("{l}{b}")
                }
            }
            (Some(l), None) => l,
            (None, Some(b)) => b,
            (None, None) => continue,
        };
        if text.is_empty() {
            continue;
        }
        let text_runs = runs(&text, &font, [color[0], color[1], color[2]]);
        let [sx, sy] = placement.scale_virtual_to_real;
        let glyph_w = normalized * sx;
        let glyph_h = pixel_height * normalized * sy;
        let width: f32 = text_runs.iter().map(|r| r.width * glyph_w).sum();
        let [mut x, y] = placement.hud_elem_pos(
            elem.x,
            elem.y,
            elem.align_org,
            elem.align_screen,
            elem.from_x,
            elem.from_y,
            elem.from_align_org,
            elem.from_align_screen,
            elem.move_start_time,
            elem.move_time,
            elem.font_scale_start_time,
            elem.font_scale_time,
            now,
            width,
            font_height,
        );
        let baseline_y = (y + font_height + 0.5).floor();
        if logged.insert(format!("txt:{foreground}:{text}")) {
            diag::info!(
                Ui,
                "t6 code HUD: hudelem text fg={} font={} at=({:.0},{:.0}) text={text:?}",
                foreground,
                font_name,
                x,
                baseline_y
            );
        }
        for run in text_runs {
            let run_width = run.width * glyph_w;
            let id = base_id + out.len() as u32;
            out.push(Draw {
                id,
                rectangle: [
                    x * to_canvas,
                    (baseline_y - glyph_h) * to_canvas,
                    (x + run_width) * to_canvas,
                    baseline_y * to_canvas,
                ],
                color: [run.color[0], run.color[1], run.color[2], color[3]],
                rotation: 0.0,
                alignment: 0,
                clip: None,
                content: Content::Text {
                    value: run.text,
                    font: font_name.to_owned(),
                },
                text_shadow: true,
            });
            x += run_width;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_crosshair(
    ps: &ps::PlayerState,
    weapons: Option<&PreparedWeapons>,
    weapon_epoch: Option<u32>,
    placement: &t6::scrplace::Placement,
    tan_half: f32,
    draw_crosshair_dvar: bool,
    crosshair_alpha: f32,
    crosshair_alpha_min: f32,
    position_to_ads: bool,
    to_canvas: f32,
    out: &mut Vec<Draw>,
    logged: &mut BTreeSet<String>,
) {
    if !draw_crosshair_dvar
        || ps.f_weapon_pos_frac >= 1.0
        || matches!(ps.weaponstate_primary, 19..=21)
        || !hud::should_draw_crosshair(
            true,
            ps.weaponstate_primary,
            ps.weaponstate_secondary,
            ps.last_weapon_hand,
        )
    {
        return;
    }
    let Some(bound) = weapons.and_then(|w| w.for_snapshot(weapon_epoch).ok()) else {
        return;
    };
    let weapon = w_iw4::get_viewmodel_weapon_index(ps);
    let Some(facts) = bound.row(weapon).and_then(|r| r.hud_facts()) else {
        return;
    };
    let Some(reticle) = bound.registry().reticle_of(weapon) else {
        return;
    };
    let reticle_facts = hud::WeaponReticleFacts {
        i_reticle_min_ofs: facts.i_reticle_min_ofs,
        hip_reticle_side_pos: facts.hip_reticle_side_pos,
        i_reticle_side_size: facts.i_reticle_side_size,
    };
    let ads_xf = hud::WeaponAdsCrosshairFacts {
        ads_aim_pitch: facts.ads_aim_pitch,
        ads_crosshair_in_frac: facts.ads_crosshair_in_frac,
        ads_crosshair_out_frac: facts.ads_crosshair_out_frac,
    };
    let trans_scale =
        hud::hip_crosshair_trans_scale(ps.f_weapon_pos_frac, position_to_ads, &ads_xf, tan_half);
    let alpha = hud::calc_reticle_alpha(
        trans_scale,
        crosshair_alpha,
        crosshair_alpha_min,
        ps.aim_spread_scale,
    );
    if alpha < hud::RETICLE_SIDES_ALPHA_DRAW_MIN {
        return;
    }
    let color = [1.0, 1.0, 1.0, alpha];
    if reticle.side_authored
        && reticle.side_size > 0
        && let Some(side_img) = reticle.side_image.as_deref()
    {
        let side_name = asset_core::AssetRef::bare_name(side_img);
        let side_size_v = hud::reticle_draw_size(&reticle_facts, trans_scale);
        let spread_facts = w_iw4::WeaponSpreadFacts {
            stand_min: facts.hip_spread_stand_min,
            ducked_min: facts.hip_spread_ducked_min,
            prone_min: facts.hip_spread_prone_min,
            stand_max: facts.hip_spread_stand_max,
            ducked_max: facts.hip_spread_ducked_max,
            prone_max: facts.hip_spread_prone_max,
        };
        let cone = w_iw4::get_spread_for_weapon(
            ps.view_height_current,
            ps.spread_override,
            w_iw4::SpreadOverrideState::from_i32(ps.spread_override_state),
            &spread_facts,
            w_iw4::perk_weap_spread_multiplier(ps.perks[0]),
        );
        let gap_v = hud::calc_reticle_spread(
            cone.min,
            cone.max,
            ps.aim_spread_scale,
            trans_scale,
            tan_half,
            &reticle_facts,
            side_size_v,
        );
        let [w, h] = side_size_v;
        if logged.insert(format!("reticle:side:{side_name}")) {
            diag::info!(
                Ui,
                "t6 code HUD: reticle side={} size=({:.0},{:.0}) gap=({:.1},{:.1})",
                side_name,
                w,
                h,
                gap_v[0],
                gap_v[1]
            );
        }
        for (idx, (rect, rot)) in [
            ([-w * 0.5, -h - gap_v[1], w, h], 0.0),
            ([gap_v[0], -h * 0.5, w, h], 90.0),
            ([-w * 0.5, gap_v[1], w, h], 180.0),
            ([-w - gap_v[0], -h * 0.5, w, h], 270.0),
        ]
        .into_iter()
        .enumerate()
        {
            let [x, y, rw, rh] = placement.apply_rect(rect, 2, 2);
            out.push(Draw {
                id: CODE_HUD_CROSSHAIR_ID + idx as u32,
                rectangle: [
                    x * to_canvas,
                    y * to_canvas,
                    (x + rw) * to_canvas,
                    (y + rh) * to_canvas,
                ],
                color,
                rotation: rot,
                alignment: 0,
                clip: None,
                content: Content::Image(side_name.to_owned()),
                text_shadow: false,
            });
        }
    }
    if reticle.center_authored
        && reticle.center_size > 0
        && let Some(center_img) = reticle.center_image.as_deref()
    {
        let center_name = asset_core::AssetRef::bare_name(center_img);
        let cook = if facts.cooks_grenade && ps.grenade_time_left > 0 {
            (ps.grenade_time_left % 1000) as f32 * 0.01
        } else {
            0.0
        };
        let size = (reticle.center_size as f32 + cook) * trans_scale;
        if logged.insert(format!("reticle:center:{center_name}")) {
            diag::info!(
                Ui,
                "t6 code HUD: reticle center={} size={:.0}",
                center_name,
                size
            );
        }
        let [x, y, rw, rh] = placement.apply_rect([-size * 0.5, -size * 0.5, size, size], 2, 2);
        out.push(Draw {
            id: CODE_HUD_CROSSHAIR_ID + 4,
            rectangle: [
                x * to_canvas,
                y * to_canvas,
                (x + rw) * to_canvas,
                (y + rh) * to_canvas,
            ],
            color,
            rotation: 0.0,
            alignment: 0,
            clip: None,
            content: Content::Image(center_name.to_owned()),
            text_shadow: false,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn update(
    menus: Option<Res<asset_game::SessionCodeHudMenus>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    clock: Option<Res<FrameClock>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    weapons: Option<Res<PreparedWeapons>>,
    input: Res<frame::HudInputView>,
    menu_open: Res<frame::NativeGameMenu>,
    cameras: Query<&Projection, With<Camera3d>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut draws: ResMut<CodeHudDraws>,
    mut hint: Local<CursorHint>,
    mut hide: Local<HideFadeView>,
) {
    draws.back.clear();
    draws.front.clear();
    let (Some(catalog), Some(snapshot), Some(clock), Some(strings), Ok(window)) = (
        menus.as_ref().and_then(|m| m.0.as_deref()),
        presented.snapshot(),
        clock,
        strings,
        windows.single(),
    ) else {
        return;
    };
    let Some(namespace) = catalog.namespace else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        *hint = CursorHint::default();
        return;
    };
    let Some(meta) = snapshot.meta.for_client(local.0) else {
        *hint = CursorHint::default();
        return;
    };
    let alive = meta.lifecycle == sim::ClientLifecycle::Alive && ps.health > 0;
    let now = clock.time();
    if !alive {
        hint.kind = 0;
    } else if ps.cursor_hint != 0 {
        // `CG_UpdateCursorHints`: a hint in the player state starts now.
        hint.start_ms = now;
        hint.fade_ms = if ps.cursor_hint == 2 { 2000 } else { 100 };
        hint.kind = ps.cursor_hint;
        hint.string = ps.cursor_hint_string;
    }
    let dvars = snapshot.meta.script_dvars(local.0);
    let float_dvar = |name: &str, default: f32| {
        dvars
            .string(name)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(default)
    };
    let ui_small_font = float_dvar("ui_smallFont", 0.563);
    let ui_big_font = float_dvar("ui_bigFont", 0.688);
    let placement = t6::scrplace::Placement::setup(
        window.width(),
        window.height(),
        float_dvar("safeArea_horizontal", 0.85),
        float_dvar("safeArea_vertical", 0.85),
    );
    // The LUI scene's space is 720 high.
    let to_canvas = 720.0 / window.height().max(1.0);
    let use_key = input.use_key.as_deref().unwrap_or("");

    // 0. The gump hide fade, over the whole viewport before the HUD.
    if let Some(call) = meta.view_effects.client_screen.hide_fade
        && hide.seen != Some(call.set_ms)
    {
        hide.seen = Some(call.set_ms);
        hide.fade.start(call.set_ms, call.color);
        diag::info!(
            Ui,
            "t6 code HUD: gump hide fade starts at {} color=({:.2},{:.2},{:.2})",
            call.set_ms,
            call.color[0],
            call.color[1],
            call.color[2]
        );
    }
    let was_active = hide.fade.active();
    // t6.csc.gump.stream: IW4L loads the whole map at start, so no gump zone
    // is ever loading.
    if let Some(color) = hide.fade.frame(now, false) {
        let [x, y, w, h] = placement.apply_rect([0.0, 0.0, 640.0, 480.0], 4, 4);
        draws.back.push(Draw {
            id: CODE_HUD_HIDE_FADE_ID,
            rectangle: [
                x * to_canvas,
                y * to_canvas,
                (x + w) * to_canvas,
                (y + h) * to_canvas,
            ],
            color,
            rotation: 0.0,
            alignment: 0,
            clip: None,
            content: Content::Image("white".to_owned()),
            text_shadow: false,
        });
    } else if was_active {
        diag::info!(Ui, "t6 code HUD: gump hide fade ends at {now}");
    }

    // 1. Background server `HudElem`s (`CG_Draw2dHudElems(localClientNum, 0)`).
    draw_hud_elems(
        namespace,
        snapshot,
        local.0,
        ps,
        &strings,
        use_key,
        menu_open.0,
        now,
        &placement,
        ui_small_font,
        ui_big_font,
        to_canvas,
        false,
        CODE_HUD_DRAW_ID,
        &mut draws.back,
        &mut hint.logged_elems,
    );

    // 2. Legacy HUD menus (`Menu_PaintAll`).
    let lists = asset_game::t6_hud_menu_lists(
        dvars
            .string("g_gametype")
            .unwrap_or(snapshot.meta.kind.token()),
    );
    let bound_weapons = weapons
        .as_ref()
        .and_then(|w| w.for_snapshot(presented.weapon_epoch()).ok());
    if ps.f_weapon_pos_frac > hint.last_ads_frac + 1e-4 {
        hint.position_to_ads = true;
    } else if ps.f_weapon_pos_frac + 1e-4 < hint.last_ads_frac {
        hint.position_to_ads = false;
    }
    hint.last_ads_frac = ps.f_weapon_pos_frac;
    if alive && !menu_open.0 {
        let tan_half = cameras
            .iter()
            .find_map(|p| match p {
                Projection::Perspective(persp) => Some((persp.fov * 0.5).tan()),
                _ => None,
            })
            .unwrap_or(0.47);
        draw_crosshair(
            ps,
            weapons.as_deref(),
            presented.weapon_epoch(),
            &placement,
            tan_half,
            float_dvar("cg_drawCrosshair", 1.0) != 0.0,
            float_dvar("cg_crosshairAlpha", 1.0),
            float_dvar("cg_crosshairAlphaMin", 0.5),
            hint.position_to_ads,
            to_canvas,
            &mut draws.front,
            &mut hint.logged_elems,
        );
    }
    for list in &lists {
        for name in catalog.list_menus.get(list.as_str()).into_iter().flatten() {
            let Some(menu) = catalog.get(name) else {
                continue;
            };
            if menu.items.is_empty() {
                continue;
            }
            for item in &menu.items {
                if item.owner_draw != OWNERDRAW_CURSORHINT {
                    continue;
                }
                let alpha = {
                    let left = hint.fade_ms - (now - hint.start_ms);
                    if left < 100 { left as f32 / 100.0 } else { 1.0 }
                };
                if alpha <= 0.0 || hint.kind == 0 {
                    hint.kind = 0;
                    continue;
                }
                let mut weapon_icon: Option<(String, f32, f32)> = None;
                let text = match hint.kind {
                    1 | 2 if hint.string >= 0 => {
                        hint_string(namespace, snapshot, &strings, use_key, hint.string)
                    }
                    3 => {
                        let text = (hint.string >= 0)
                            .then(|| {
                                hint_string(namespace, snapshot, &strings, use_key, hint.string)
                            })
                            .flatten();
                        Some(
                            text.filter(|t| !t.is_empty() && !t.starts_with('0'))
                                .unwrap_or_else(|| "MISSING_ACTIVATE_TRIGGER_TEXT".to_owned()),
                        )
                    }
                    4 => {
                        if hint.string >= 0 {
                            hint_string(namespace, snapshot, &strings, use_key, hint.string)
                        } else {
                            strings
                                .0
                                .text_in(namespace, "PLATFORM_PICKUPHEALTH")
                                .map(|t| t.replace("&&1", use_key))
                        }
                    }
                    kind if kind >= 7 => {
                        let weapon_idx = (kind - 7) as u32;
                        if let Some(bound) = bound_weapons.as_ref() {
                            let registry = bound.registry();
                            if let Some((img, ratio)) = registry.pickup_icon_of(weapon_idx) {
                                let (sx, sy) = match ratio {
                                    1 => (2.0, 1.0),
                                    2 => (2.0, 0.5),
                                    _ => (1.0, 1.0),
                                };
                                weapon_icon =
                                    Some((asset_core::AssetRef::bare_name(img).to_owned(), sx, sy));
                            }
                            if hint.string >= 0 {
                                hint_string(namespace, snapshot, &strings, use_key, hint.string)
                            } else {
                                let wname = registry
                                    .display_name_key_of(weapon_idx)
                                    .and_then(|k| strings.0.text_in(namespace, k))
                                    .unwrap_or_else(|| registry.name_of(weapon_idx));
                                let primary_count = ps
                                    .weapons
                                    .iter()
                                    .filter(|&&w| {
                                        w > 0
                                            && bound
                                                .row(w as u32)
                                                .and_then(|r| r.hud_facts())
                                                .is_some_and(|f| f.is_primary())
                                    })
                                    .count();
                                let offhand = bound
                                    .row(weapon_idx)
                                    .and_then(|r| r.hud_facts())
                                    .map_or(0, |f| f.offhand_class);
                                let key = if offhand == 1 {
                                    "PLATFORM_THROWBACKGRENADE"
                                } else if offhand != 0 || primary_count < 2 {
                                    "PLATFORM_PICKUPNEWWEAPON"
                                } else {
                                    "PLATFORM_SWAPWEAPONS"
                                };
                                strings.0.text_in(namespace, key).map(|tmpl| {
                                    let tmpl = hud::replace_directive(tmpl, |cmd| {
                                        if matches!(cmd, "+activate" | "+usereload" | "+frag") {
                                            use_key.to_owned()
                                        } else {
                                            String::new()
                                        }
                                    })
                                    .replace("&&1", use_key);
                                    if tmpl.contains("&&2") {
                                        tmpl.replace("&&2", wname)
                                    } else {
                                        format!("{tmpl} {wname}")
                                    }
                                })
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let (horz, vert) = if item.rect.horz_align == 0 && item.rect.vert_align == 0 {
                    (menu.rect.horz_align, menu.rect.vert_align)
                } else {
                    (item.rect.horz_align, item.rect.vert_align)
                };
                let rect_x = menu.rect.x + item.rect.x;
                let rect_y = menu.rect.y + item.rect.y;
                let rect_w = if item.rect.w != 0.0 {
                    item.rect.w
                } else {
                    menu.rect.w
                };
                let rect_h = if item.rect.h != 0.0 {
                    item.rect.h
                } else {
                    menu.rect.h
                };
                let font_name = t6::scrplace::menu_font(
                    item.font_enum,
                    item.text_scale,
                    &placement,
                    ui_small_font,
                    ui_big_font,
                );
                let Some(font) = asset_material::ui_font::ui_font(namespace, font_name) else {
                    continue;
                };
                let pixel_height = font.pixel_height.max(1) as f32;
                let normalized = item.text_scale * 48.0 / pixel_height;
                let color = [
                    item.fore_color[0],
                    item.fore_color[1],
                    item.fore_color[2],
                    item.fore_color[3] * alpha,
                ];
                let height = (pixel_height * normalized + 0.5).floor();
                if let Some(text) = text.filter(|t| !t.is_empty()) {
                    let runs = runs(&text, &font, [color[0], color[1], color[2]]);
                    let width =
                        (runs.iter().map(|run| run.width).sum::<f32>() * normalized + 0.5).floor();
                    let x = if hint.kind == 3 {
                        (rect_w + width) * -0.5 - rect_x
                    } else {
                        -width * 0.5
                    };
                    let [x, y, w, h] = placement.apply_rect(
                        [x, height * 0.5 + rect_y, normalized, normalized],
                        i32::from(horz),
                        i32::from(vert),
                    );
                    // `UI_DrawText` snaps the baseline to whole pixels.
                    let (mut x, y) = ((x + 0.5).floor(), (y + 0.5).floor());
                    let glyph_height = pixel_height * h;
                    if hint.logged != Some((hint.kind, hint.string)) {
                        hint.logged = Some((hint.kind, hint.string));
                        diag::info!(
                            Ui,
                            "t6 code HUD: cursorhint kind={} font={} at=({:.0},{:.0}) size=({:.0},{:.0}) text={text:?}",
                            hint.kind,
                            font_name,
                            x,
                            y,
                            width,
                            height
                        );
                    }
                    for run in runs {
                        let run_width = run.width * w;
                        let id = CODE_HUD_MENU_DRAW_ID + draws.front.len() as u32;
                        draws.front.push(Draw {
                            id,
                            rectangle: [
                                x * to_canvas,
                                (y - glyph_height) * to_canvas,
                                (x + run_width) * to_canvas,
                                y * to_canvas,
                            ],
                            color: [run.color[0], run.color[1], run.color[2], color[3]],
                            rotation: 0.0,
                            alignment: 0,
                            clip: None,
                            content: Content::Text {
                                value: run.text,
                                font: font_name.to_owned(),
                            },
                            text_shadow: item.text_style != 0,
                        });
                        x += run_width;
                    }
                }
                if let Some((icon_name, scale_x, scale_y)) = weapon_icon {
                    let icon_w = rect_w * scale_x;
                    let icon_h = rect_h * scale_y;
                    let icon_x = -icon_w * 0.5;
                    let icon_y = (rect_y - icon_h * 0.5) + height * 2.0;
                    let [x, y, w, h] = placement.apply_rect(
                        [icon_x, icon_y, icon_w, icon_h],
                        i32::from(horz),
                        i32::from(vert),
                    );
                    let id = CODE_HUD_MENU_DRAW_ID + draws.front.len() as u32;
                    draws.front.push(Draw {
                        id,
                        rectangle: [
                            x * to_canvas,
                            y * to_canvas,
                            (x + w) * to_canvas,
                            (y + h) * to_canvas,
                        ],
                        color,
                        rotation: 0.0,
                        alignment: 0,
                        clip: None,
                        content: Content::Image(icon_name),
                        text_shadow: false,
                    });
                }
            }
        }
    }

    // 3. Foreground server `HudElem`s (`CG_Draw2dHudElems(localClientNum, 1)`).
    draw_hud_elems(
        namespace,
        snapshot,
        local.0,
        ps,
        &strings,
        use_key,
        menu_open.0,
        now,
        &placement,
        ui_small_font,
        ui_big_font,
        to_canvas,
        true,
        CODE_HUD_FRONT_ELEM_ID,
        &mut draws.front,
        &mut hint.logged_elems,
    );
}
