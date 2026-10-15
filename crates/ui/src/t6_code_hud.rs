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
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use game_api::ui::{Content, Draw};
use game_t6 as t6;
use hud_iw4 as hud;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};

/// The cursor hint owner draw (`CG_CURSORHINT`).
const OWNERDRAW_CURSORHINT: i32 = 72;
/// Draw ids of the code HUD, above the LUI scene's.
pub(crate) const CODE_HUD_DRAW_ID: u32 = 0x8000_0000;

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

/// The code HUD's draws this frame, in the LUI scene's 720-high space.
#[derive(Resource, Default)]
pub(crate) struct CodeHudDraws(pub(crate) Vec<Draw>);

/// The cursor hint the client game keeps between frames.
#[derive(Default)]
struct CursorHint {
    kind: i32,
    string: i32,
    start_ms: i32,
    fade_ms: i32,
    logged: Option<(i32, i32)>,
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
fn update(
    menus: Option<Res<asset_game::SessionCodeHudMenus>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    clock: Option<Res<FrameClock>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    input: Res<frame::HudInputView>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut draws: ResMut<CodeHudDraws>,
    mut hint: Local<CursorHint>,
) {
    draws.0.clear();
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
    let Some(ps) = presented.alive_player(local.0) else {
        *hint = CursorHint::default();
        return;
    };
    let now = clock.time();
    // `CG_UpdateCursorHints`: a hint in the player state starts now.
    if ps.cursor_hint != 0 {
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
    let placement = t6::scrplace::Placement::setup(
        window.width(),
        window.height(),
        float_dvar("safeArea_horizontal", 0.85),
        float_dvar("safeArea_vertical", 0.85),
    );
    // The LUI scene's space is 720 high.
    let to_canvas = 720.0 / window.height().max(1.0);
    let lists = asset_game::t6_hud_menu_lists(
        dvars
            .string("g_gametype")
            .unwrap_or(snapshot.meta.kind.token()),
    );
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
                let use_key = input.use_key.as_deref().unwrap_or("");
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
                    _ => None,
                };
                let Some(text) = text.filter(|t| !t.is_empty()) else {
                    continue;
                };
                // The item's rect in the menu; an unaligned item takes the
                // menu's alignment.
                let (horz, vert) = if item.rect.horz_align == 0 && item.rect.vert_align == 0 {
                    (menu.rect.horz_align, menu.rect.vert_align)
                } else {
                    (item.rect.horz_align, item.rect.vert_align)
                };
                let rect_x = menu.rect.x + item.rect.x;
                let rect_y = menu.rect.y + item.rect.y;
                let font_name = t6::scrplace::menu_font(
                    item.font_enum,
                    item.text_scale,
                    &placement,
                    float_dvar("ui_smallFont", 0.563),
                    float_dvar("ui_bigFont", 0.688),
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
                let runs = runs(&text, &font, [color[0], color[1], color[2]]);
                let width =
                    (runs.iter().map(|run| run.width).sum::<f32>() * normalized + 0.5).floor();
                let height = (pixel_height * normalized + 0.5).floor();
                let x = if hint.kind == 3 {
                    (item.rect.w + width) * -0.5 - rect_x
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
                    let id = CODE_HUD_DRAW_ID + draws.0.len() as u32;
                    draws.0.push(Draw {
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
        }
    }
}
