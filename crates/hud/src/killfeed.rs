use std::collections::{HashMap, HashSet, VecDeque};

use asset_game::MenuCatalog;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use hud_iw4::{
    GAME_MSG_WIN0_FADE_IN_TIME_MS, GAME_MSG_WIN0_FADE_OUT_TIME_MS, GAME_MSG_WIN0_LINE_COUNT,
    GAME_MSG_WIN0_MSG_TIME_MS, GAME_MSG_WIN0_SCROLL_TIME_MS, ITEM_TYPE_GAME_MESSAGE_WINDOW,
    KILLICON_DIED, embed_hud_icon_size_byte, gamenotify_line, killicon_stretch_uv,
    killicon_virtual_size, normalized_text_scale, obituary_mod, obituary_mod_killicon,
};
use net::{FrameClock, LocalPresentClient};

use crate::chrome::text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

#[derive(Clone, Debug)]
enum KillfeedLine {
    Obituary {
        start_ms: i32,
        end_ms: i32,
        icon: String,
        icon_namespace: asset_core::AssetNamespace,
        attacker: String,
        has_attacker: bool,
        victim: String,
        attacker_team: i32,
        victim_team: i32,

        kill_icon_ratio: i32,

        flip_kill_icon: bool,
    },
    Notify {
        start_ms: i32,
        end_ms: i32,
        text: String,
        name_empty: bool,
    },
}

impl KillfeedLine {
    fn start_ms(&self) -> i32 {
        match self {
            Self::Obituary { start_ms, .. } | Self::Notify { start_ms, .. } => *start_ms,
        }
    }

    fn end_ms(&self) -> i32 {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => *end_ms,
        }
    }

    fn fade_out(&mut self, now: i32) {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => {
                *end_ms = (*end_ms).min(now.saturating_add(GAME_MSG_WIN0_FADE_OUT_TIME_MS));
            }
        }
    }
}

#[derive(Clone, Debug)]
struct KillIconPick {
    stem: String,

    namespace: asset_core::AssetNamespace,
    ratio: i32,
    flip: bool,
}

#[derive(Resource, Default)]
pub(crate) struct KillfeedWindow {
    lines: VecDeque<KillfeedLine>,
    bold: VecDeque<(i32, String)>,
    seen: HashSet<u32>,
    game: VecDeque<GameLine>,
    game_bold: VecDeque<GameLine>,
}

/// A line of a game's own message windows.
#[derive(Clone, Debug)]
struct GameLine {
    start_ms: i32,
    end_ms: i32,
    content: GameLineContent,
}

#[derive(Clone, Debug)]
enum GameLineContent {
    Obituary {
        icon: KillIconPick,
        attacker: Option<(String, i32)>,
        victim: (String, i32),
    },
    /// A script print's line.
    Text(String),
}

/// A line into a game message window; the line past the window's count
/// fades out.
fn push_game_line(
    lines: &mut VecDeque<GameLine>,
    timing: game_api::hud::MessageWindow,
    content: GameLineContent,
    now: i32,
) {
    lines.push_back(GameLine {
        start_ms: now,
        end_ms: now.saturating_add(timing.msg_time_ms),
        content,
    });
    if lines.len() > timing.line_count {
        let fading = lines.len() - timing.line_count - 1;
        let line = &mut lines[fading];
        line.end_ms = line.end_ms.min(now.saturating_add(timing.fade_out_ms));
    }
}

impl KillfeedWindow {
    fn push(&mut self, line: KillfeedLine, now: i32) {
        self.lines.retain(|line| now < line.end_ms());
        self.lines.push_back(line);
        while self.lines.len() > GAME_MSG_WIN0_LINE_COUNT + 3 {
            self.lines.pop_front();
        }
        if self.lines.len() > GAME_MSG_WIN0_LINE_COUNT {
            let fading = self.lines.len() - GAME_MSG_WIN0_LINE_COUNT - 1;
            self.lines[fading].fade_out(now);
        }
    }
}

const BOLD_LINE_COUNT: usize = 3;
const BOLD_MSG_TIME_MS: i32 = 3000;
const BOLD_TEXT_SCALE: f32 = 0.5;
const BOLD_Y: f32 = 100.0;

#[derive(Component)]
pub(crate) struct KillfeedRaster;

pub(crate) fn spawn_killfeed(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, (KillfeedRaster, crate::plugin::CodeHudPiece));
}

fn hide(pass: &mut HudTessPass) {
    pass.killfeed = TessJob::Hide;
}

fn pick_kill_icon(
    payload: &sim::EntityEventPayload,
    weapons: Option<&assets::BoundWeapons<'_>>,
) -> KillIconPick {
    let chrome = crate::images::HUD_CHROME_NAMESPACE;

    if let Some(mod_) = obituary_mod(payload.event_parm) {
        let Some(stem) = obituary_mod_killicon(mod_) else {
            return KillIconPick {
                stem: String::new(),
                namespace: chrome,
                ratio: 0,
                flip: false,
            };
        };
        return KillIconPick {
            stem: stem.to_owned(),
            namespace: chrome,
            ratio: 0,
            flip: false,
        };
    }
    let weapon = payload.event_parm as u32;
    let fallback = KillIconPick {
        stem: KILLICON_DIED.to_owned(),
        namespace: chrome,
        ratio: 0,
        flip: false,
    };
    let Some(reg) = weapons else {
        return fallback;
    };
    let Some(stem) = reg
        .registry()
        .kill_icon_image_of(weapon)
        .or_else(|| reg.registry().kill_icon_of(weapon))
    else {
        return fallback;
    };
    let (ratio, flip) = match reg.row(weapon).and_then(|weapon| weapon.hud_facts()) {
        Some(facts) => (facts.kill_icon_ratio, facts.flip_kill_icon),
        None => (0, false),
    };
    KillIconPick {
        stem: stem.to_owned(),
        namespace: reg
            .registry()
            .component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            .unwrap_or(chrome),
        ratio,
        flip,
    }
}

fn snapshot_client_team(presented: &net::PresentedSnapshot, client: i32) -> i32 {
    if client < 0 {
        return 0;
    }
    presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(sim::ClientId(client as u32)))
        .map_or(0, |meta| meta.client_state_team)
}

pub fn team_name_color(local_team: i32, team: i32, dvars: sim::ScriptDvars<'_>) -> [f32; 4] {
    if !matches!(local_team, 1 | 2) || !matches!(team, 1 | 2) {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let (name, default) = if dvars
        .int("useRelativeTeamColors")
        .is_some_and(|value| value != 0)
    {
        if team == local_team {
            ("g_TeamColor_MyTeam", gamemode_iw4::TEAM_COLOR_MY_TEAM)
        } else {
            ("g_TeamColor_EnemyTeam", gamemode_iw4::TEAM_COLOR_ENEMY_TEAM)
        }
    } else if team == 1 {
        ("g_TeamColor_Axis", gamemode_iw4::TEAM_COLOR_AXIS)
    } else {
        ("g_TeamColor_Allies", gamemode_iw4::TEAM_COLOR_ALLIES)
    };
    let parse = |rgb: &str| {
        let mut values = rgb.split_whitespace().map(str::parse::<f32>);
        let rgb = [
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
        ];
        (values.next().is_none() && rgb.iter().all(|value| value.is_finite())).then(|| {
            [
                rgb[0].clamp(0.0, 1.0),
                rgb[1].clamp(0.0, 1.0),
                rgb[2].clamp(0.0, 1.0),
                1.0,
            ]
        })
    };
    dvars
        .string(name)
        .and_then(parse)
        .or_else(|| parse(default))
        .unwrap_or([1.0; 4])
}

fn snapshot_client_name(presented: &net::PresentedSnapshot, client: i32) -> String {
    if client < 0 {
        return String::new();
    }
    let Some(snap) = presented.snapshot() else {
        return String::new();
    };
    let Some(meta) = snap.meta.for_client(sim::ClientId(client as u32)) else {
        return String::new();
    };
    match entity_iw4::client_state_name(&meta.name) {
        Some(s) => s.to_owned(),
        None => String::new(),
    }
}

pub(crate) fn obituary(
    obituary: On<net::EntityObituary>,
    mut window: ResMut<KillfeedWindow>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<net::PresentedSnapshot>,
    clock: Res<FrameClock>,
    local: Res<LocalPresentClient>,
    mode: Option<Res<game_api::ModeRules>>,
    hud_menus: Option<Res<asset_game::SessionHudMenus>>,
) {
    if obituary.in_killcam {
        return;
    }
    let payload = obituary.event.payload;
    let bound = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_event(obituary.event.world).ok());
    if let Some(hud) = crate::game_code_hud::game_hud(mode.as_deref()) {
        if let Some(rules) = hud.killfeed {
            let namespace = hud_menus
                .as_ref()
                .and_then(|menus| menus.0.as_ref()?.namespace)
                .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
            game_obituary(
                rules,
                namespace,
                &payload,
                bound.as_ref(),
                &presented,
                local.0,
                clock.time(),
                &mut window.game,
            );
        }
        return;
    }
    let pick = pick_kill_icon(&payload, bound.as_ref());
    let attacker = snapshot_client_name(&presented, payload.attacker_entity_num);
    let victim = snapshot_client_name(&presented, payload.other_entity_num);
    let attacker_team = snapshot_client_team(&presented, payload.attacker_entity_num);
    let victim_team = snapshot_client_team(&presented, payload.other_entity_num);
    let now = clock.time();
    window.push(
        KillfeedLine::Obituary {
            start_ms: now,
            end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
            icon: pick.stem,
            icon_namespace: pick.namespace,
            attacker,
            has_attacker: (0..18).contains(&payload.attacker_entity_num)
                && payload.attacker_entity_num != payload.other_entity_num,
            victim,
            attacker_team,
            victim_team,
            kill_icon_ratio: pick.ratio,
            flip_kill_icon: pick.flip,
        },
        now,
    );
}

/// A client's name and team, when the client keeps its player.
fn game_client(presented: &net::PresentedSnapshot, client: i32) -> Option<(String, i32)> {
    let meta = presented
        .snapshot()?
        .meta
        .for_client(sim::ClientId(u32::try_from(client).ok()?))?;
    Some((
        snapshot_client_name(presented, client),
        meta.client_state_team,
    ))
}

fn game_obituary(
    rules: &game_api::hud::KillfeedRules,
    namespace: asset_core::AssetNamespace,
    payload: &sim::EntityEventPayload,
    weapons: Option<&assets::BoundWeapons<'_>>,
    presented: &net::PresentedSnapshot,
    local: sim::ClientId,
    now: i32,
    lines: &mut VecDeque<GameLine>,
) {
    let Some(victim) = game_client(presented, payload.other_entity_num) else {
        if payload.other_entity_num < 0 {
            diag::warn!(Ui, "{}", rules.out_of_range);
        }
        return;
    };
    let attacker = match payload.attacker_entity_num {
        number if number < 0 || number == payload.other_entity_num => None,
        number => match game_client(presented, number) {
            Some(attacker) => Some(attacker),
            None => return,
        },
    };
    if game_client(presented, local.0 as i32).is_none() {
        return;
    }
    let material = |name: &str| KillIconPick {
        stem: name.to_owned(),
        namespace,
        ratio: 0,
        flip: false,
    };
    let icon = match (rules.kill_icon)(payload.event_parm) {
        game_api::hud::KillIcon::Material(name) => material(name),
        game_api::hud::KillIcon::Weapon { weapon, none } => weapons
            .and_then(|reg| {
                let stem = reg.registry().kill_icon_of(weapon)?;
                let facts = reg.row(weapon).and_then(|row| row.hud_facts());
                Some(KillIconPick {
                    stem: stem.to_owned(),
                    namespace: reg
                        .registry()
                        .component_namespace_of(weapon, asset_game::WeaponComponent::Material)
                        .unwrap_or(namespace),
                    ratio: facts.map_or(0, |facts| facts.kill_icon_ratio),
                    flip: facts.is_some_and(|facts| facts.flip_kill_icon),
                })
            })
            .unwrap_or_else(|| material(none)),
    };
    push_game_line(
        lines,
        rules.timing,
        GameLineContent::Obituary {
            icon,
            attacker,
            victim,
        },
        now,
    );
}

fn game_name_color(color: game_api::hud::NameColor, dvars: sim::ScriptDvars<'_>) -> [f32; 3] {
    match color {
        game_api::hud::NameColor::White => [1.0; 3],
        game_api::hud::NameColor::Dvar { name, default } => dvars
            .string(name)
            .and_then(|rgb| {
                let mut values = rgb.split_whitespace().map(str::parse::<f32>);
                let rgb = [
                    values.next()?.ok()?,
                    values.next()?.ok()?,
                    values.next()?.ok()?,
                ];
                Some(rgb.map(|value| value.clamp(0.0, 1.0)))
            })
            .unwrap_or(default),
    }
}

/// The alpha of a game message window's line: faded in after it scrolls
/// in, faded out before it ends.
fn game_line_alpha(timing: game_api::hud::MessageWindow, line: &GameLine, now: i32) -> f32 {
    let mut alpha = 1.0f32;
    let left = line.end_ms - now;
    if left < timing.fade_out_ms {
        alpha = left as f32 / timing.fade_out_ms as f32;
    }
    let age = now - line.start_ms;
    let (fade_in, scroll) = (timing.fade_in_ms, timing.scroll_time_ms);
    if fade_in < scroll {
        if age < scroll {
            if scroll - fade_in < age {
                alpha *= (age - (scroll - fade_in)) as f32 / fade_in as f32;
            } else {
                alpha = 0.0;
            }
        }
    } else if fade_in != 0 && age < fade_in {
        alpha *= age as f32 / fade_in as f32;
    }
    alpha.max(0.0)
}

/// A script print's text: its localized template with its arguments in
/// place of `&&1`, `&&2`, ….
fn game_print_text(
    strings: Option<&PreparedLocalizedStrings>,
    cmd: &net::SvcGameNotify,
) -> Option<String> {
    let strings = strings?;
    let template = crate::hudelem::resolve_hud_text(strings, &cmd.key)?;
    if cmd.name.is_empty() {
        return Some(template);
    }
    let args = cmd.name.split(sim::HUD_PRINT_ARG_SEPARATOR).map(|arg| {
        crate::hudelem::resolve_hud_text(strings, arg).unwrap_or_else(|| arg.to_owned())
    });
    Some(args.enumerate().fold(template, |line, (i, arg)| {
        line.replace(&format!("&&{}", i + 1), &arg)
    }))
}

/// A game message window's font and its text scale.
fn game_window_font<'a>(
    catalog: Option<&'a MenuCatalog>,
    strings: Option<&PreparedLocalizedStrings>,
    surface: &crate::surface::Hud2dSurface,
    menu: &str,
    window: i32,
) -> Option<(&'a asset_game::FontDef, f32)> {
    let catalog = catalog?;
    let item = catalog.get(menu)?.items.iter().find(|item| {
        item.item_type == ITEM_TYPE_GAME_MESSAGE_WINDOW && item.game_msg_window_index == window
    })?;
    let assets = crate::chrome::ChromeAssets {
        catalog: Some(catalog),
        localize: strings.map(|s| &s.0),
    };
    let font_name = assets.font_name(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    )?;
    let font = catalog.font(font_name)?;
    let scale = assets
        .layout()?
        .text_scale(font.pixel_height, item.text_scale);
    Some((font, scale))
}

/// A print cut into a window's lines as the game's console print wraps
/// it: past `width` a line ends after its last blank (or before the letter
/// that overflows when it has none), at a newline, and the next line
/// starts past its blanks in the last colour set (`^<n>`, unless white).
fn wrap_print(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut color = '7';
    let mut rest = text;
    let mut first = true;
    loop {
        if !first {
            rest = rest.trim_start_matches(' ');
        }
        let prefix = if color == '7' {
            String::new()
        } else {
            format!("^{color}")
        };
        let mut cut = rest.len();
        let mut next = rest.len();
        let mut after_blank = None;
        let mut chars = rest.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if c == '\n' {
                (cut, next) = (at, at + 1);
                first = true;
                break;
            }
            if c == '^'
                && let Some(&(_, code)) = chars.peek()
                && (('0'..='@').contains(&code) || code == 'F')
            {
                color = code;
                chars.next();
                continue;
            }
            let end = at + c.len_utf8();
            if at != 0 && c.is_whitespace() {
                after_blank = Some((end, color));
            }
            if measure(&rest[..end]) > width {
                let at = if at == 0 { end } else { at };
                let (end, carried) = after_blank.unwrap_or((at, color));
                (cut, next, color) = (end, end, carried);
                first = false;
                break;
            }
        }
        out.push(format!("{prefix}{}", &rest[..cut]));
        if next >= rest.len() {
            break;
        }
        rest = &rest[next..];
    }
    out
}

/// Which of a game's message windows to draw: its menu, index and timing.
#[derive(Clone, Copy)]
struct GameWindow {
    menu: &'static str,
    window: i32,
    timing: game_api::hud::MessageWindow,
}

#[allow(clippy::too_many_arguments)]
fn draw_game_killfeed(
    rules: &game_api::hud::KillfeedRules,
    window: GameWindow,
    lines: &mut VecDeque<GameLine>,
    now: i32,
    surface: &crate::surface::Hud2dSurface,
    catalog: Option<&MenuCatalog>,
    strings: Option<&PreparedLocalizedStrings>,
    presented: &net::PresentedSnapshot,
    local: sim::ClientId,
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
    gaps: &mut HudPresentationGaps,
) -> Option<Vec<crate::draw2d::Draw2dQuad>> {
    while lines.front().is_some_and(|line| now - line.end_ms >= 0) {
        lines.pop_front();
    }
    if lines.is_empty() {
        return None;
    }
    let catalog = catalog?;
    let item = catalog.get(window.menu)?.items.iter().find(|item| {
        item.item_type == ITEM_TYPE_GAME_MESSAGE_WINDOW
            && item.game_msg_window_index == window.window
    })?;
    let assets = crate::chrome::ChromeAssets {
        catalog: Some(catalog),
        localize: strings.map(|s| &s.0),
    };
    let font_name = assets.font_name(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    )?;
    let font = catalog.font(font_name)?;
    let layout = assets.layout()?;
    let namespace = catalog
        .namespace
        .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
    let font_material = asset_core::AssetRef::bare_name(&font.material).to_owned();
    hud_images.get(namespace, &font_material, images)?;
    let snapshot = presented.snapshot()?;
    let dvars = snapshot.meta.script_dvars(local);
    let local_team = game_client(presented, local.0 as i32).map_or(0, |(_, team)| team);
    let suicide = strings
        .and_then(|s| crate::hudelem::resolve_hud_text(s, rules.suicide))
        .unwrap_or_else(|| rules.suicide.to_owned());

    let timing = window.timing;
    let scale = layout.text_scale(font.pixel_height, item.text_scale);
    let line_height = (f64::from(item.text_scale * 48.0) + 0.5).round_ties_even() as i32;
    let space = text_width(font, " ") as f32 * scale;
    let width = |text: &str| text_width(font, text) as f32 * scale;
    let glyph_height = font.pixel_height as f32 * scale;
    let icon_px = |em: f32| {
        let stored = embed_hud_icon_size_byte(em);
        ((i32::from(stored) - 16) * font.pixel_height + 16) / 32
    };
    let (horz, vert) = (
        i32::from(item.rect.horz_align),
        i32::from(item.rect.vert_align),
    );

    let mut y = item.rect.y as i32;
    for line in lines.iter() {
        let age = now - line.start_ms;
        if age < timing.scroll_time_ms {
            let left = (1.0 - age as f32 / timing.scroll_time_ms as f32).clamp(0.0, 1.0);
            y += (line_height as f32 * left + 0.5).round_ties_even() as i32;
        }
    }
    let miss = hud_images.miss_reason();
    let mut cmds = Vec::new();
    for line in lines.iter().rev() {
        y -= line_height;
        if now - line.end_ms >= 0 {
            continue;
        }
        let alpha = game_line_alpha(timing, line, now) * item.fore_color[3];
        if alpha <= 0.0 {
            continue;
        }
        let text = |cmds: &mut Vec<Draw2dCmd>, x: f32, text: String, rgb: [f32; 3]| {
            let applied = surface.apply_rect(x, y as f32, scale, scale, horz, vert);
            cmds.push(Draw2dCmd {
                material_namespace: namespace,
                ..text_cmd(
                    font_name,
                    item.text_style,
                    applied.x,
                    applied.y,
                    applied.w,
                    applied.h,
                    font_material.clone(),
                    text,
                    [rgb[0], rgb[1], rgb[2], alpha],
                    "killfeed_obituary",
                )
            });
        };
        let mut x = item.rect.x;
        let (icon, attacker, victim) = match &line.content {
            GameLineContent::Text(line) => {
                let rgb = [item.fore_color[0], item.fore_color[1], item.fore_color[2]];
                text(&mut cmds, x, line.clone(), rgb);
                continue;
            }
            GameLineContent::Obituary {
                icon,
                attacker,
                victim,
            } => (icon, attacker, victim),
        };
        if let Some((name, team)) = attacker {
            let rgb = game_name_color((rules.name_color)(local_team, *team), dvars);
            text(&mut cmds, x, name.clone(), rgb);
            x += width(name) + space;
        }
        let (em_w, em_h) = (rules.icon_size)(icon.ratio);
        let (icon_w, icon_h) = (icon_px(em_w) as f32 * scale, icon_px(em_h) as f32 * scale);
        if hud_images.get(icon.namespace, &icon.stem, images).is_some() {
            let placed = surface.apply_rect(
                x,
                y as f32 - (glyph_height + icon_h) * 0.5,
                icon_w,
                icon_h,
                horz,
                vert,
            );
            let (s0, s1) = if icon.flip { (1.0, 0.0) } else { (0.0, 1.0) };
            cmds.push(Draw2dCmd {
                material_namespace: icon.namespace,
                x: placed.x,
                y: placed.y,
                w: placed.w,
                h: placed.h,
                s0,
                t0: 0.0,
                s1,
                t1: 1.0,
                color: [1.0, 1.0, 1.0, alpha],
                material: icon.stem.clone(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw {
                    site: "killfeed_obituary",
                },
                layer: 1,
            });
        } else {
            gaps.raise(GapCause::ObituaryKillIconMissing {
                name: icon.stem.clone(),
                miss,
            });
        }
        x += icon_w + space;
        let (name, team) = victim;
        let victim = match attacker {
            Some(_) => name.clone(),
            None => format!("{name} {suicide}"),
        };
        let rgb = game_name_color((rules.name_color)(local_team, *team), dvars);
        text(&mut cmds, x, victim, rgb);
    }
    let list = Draw2dList { cmds };
    let fonts = HashMap::from([(font_name.to_owned(), font)]);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    (!quads.is_empty()).then_some(quads)
}

#[allow(clippy::too_many_arguments)]
fn text_cmd(
    font_name: &str,
    text_style: i32,
    x: f32,
    y: f32,
    cmd_w: f32,
    cmd_h: f32,
    material: String,
    text: String,
    color: [f32; 4],
    site: &'static str,
) -> Draw2dCmd {
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (x + 0.5).floor(),
        y: (y + 0.5).floor(),
        w: cmd_w,
        h: cmd_h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material,
        op: Draw2dOp::TextRun {
            font: font_name.to_owned(),
            scale: cmd_w,
            text,
            loc_key: String::new(),

            style: text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site },
        layer: 1,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_killfeed(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<net::PresentedSnapshot>,
    clock: Res<FrameClock>,
    local: Res<LocalPresentClient>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut window: ResMut<KillfeedWindow>,
    mut pass: ResMut<HudTessPass>,
    mut notifies: MessageReader<net::SvcGameNotify>,
    view: Option<Res<frame::ViewSubject>>,
    mode: Option<Res<game_api::ModeRules>>,
    hud_menus: Option<Res<asset_game::SessionHudMenus>>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    if presented.player(local.0).is_none() {
        window.lines.clear();
        window.bold.clear();
        window.seen.clear();
        window.game.clear();
        window.game_bold.clear();
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }

    let now = clock.time();
    if let Some(hud) = crate::game_code_hud::game_hud(mode.as_deref()) {
        hide(&mut pass);
        let Some(rules) = hud.killfeed else {
            return;
        };
        let catalog = hud_menus.as_ref().and_then(|m| m.0.as_deref());
        let window = &mut *window;
        for cmd in notifies.read() {
            let bold = match cmd.tag {
                net::SVC_PRINT => false,
                net::SVC_PRINT_BOLD => true,
                _ => continue,
            };
            if !window.seen.insert(cmd.id) {
                continue;
            }
            let Some(text) = game_print_text(strings.as_deref(), cmd) else {
                gaps.raise(GapCause::LocalizedRowMissing {
                    key: cmd.key.clone(),
                });
                continue;
            };
            let (menu, index, timing, width, lines) = if bold {
                (
                    rules.bold_menu,
                    rules.bold_window,
                    rules.bold_timing,
                    rules.bold_width,
                    &mut window.game_bold,
                )
            } else {
                (
                    rules.menu,
                    rules.window,
                    rules.timing,
                    rules.print_width,
                    &mut window.game,
                )
            };
            let pieces = match game_window_font(catalog, strings.as_deref(), &surface, menu, index)
            {
                Some((font, scale)) => {
                    wrap_print(&text, width, |part| text_width(font, part) as f32 * scale)
                }
                None => vec![text],
            };
            for piece in pieces {
                push_game_line(lines, timing, GameLineContent::Text(piece), now);
            }
        }
        if view.is_some_and(|v| v.in_killcam()) {
            return;
        }
        let windows = [
            (
                GameWindow {
                    menu: rules.menu,
                    window: rules.window,
                    timing: rules.timing,
                },
                &mut window.game,
            ),
            (
                GameWindow {
                    menu: rules.bold_menu,
                    window: rules.bold_window,
                    timing: rules.bold_timing,
                },
                &mut window.game_bold,
            ),
        ];
        let mut all = Vec::new();
        for (game_window, lines) in windows {
            if let Some(quads) = draw_game_killfeed(
                rules,
                game_window,
                lines,
                now,
                &surface,
                catalog,
                strings.as_deref(),
                &presented,
                local.0,
                &mut hud_images,
                &mut images,
                &mut gaps,
            ) {
                all.extend(quads);
            }
        }
        if !all.is_empty() {
            pass.killfeed = TessJob::Quads(all);
        }
        return;
    }
    for cmd in notifies.read() {
        if cmd.tag == net::SVC_CHAT {
            continue;
        }
        if !window.seen.insert(cmd.id) {
            continue;
        }
        let Some(template) = strings
            .as_ref()
            .and_then(|s| crate::hudelem::resolve_hud_text(s, &cmd.key))
        else {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: cmd.key.clone(),
            });
            continue;
        };
        let args: Vec<String> = cmd
            .name
            .split(sim::HUD_PRINT_ARG_SEPARATOR)
            .map(|arg| {
                strings
                    .as_ref()
                    .and_then(|s| crate::hudelem::resolve_hud_text(s, arg))
                    .unwrap_or_else(|| arg.to_owned())
            })
            .collect();
        let text = if template.contains("&&2") {
            args.iter().enumerate().fold(template, |line, (i, arg)| {
                line.replace(&format!("&&{}", i + 1), arg)
            })
        } else {
            gamenotify_line(&template, &args[0])
        };
        if cmd.tag == net::SVC_PRINT_BOLD {
            window.bold.push_back((now, text));
            while window.bold.len() > BOLD_LINE_COUNT {
                window.bold.pop_front();
            }
            continue;
        }
        window.push(
            KillfeedLine::Notify {
                start_ms: now,
                end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
                text,
                name_empty: cmd.tag == net::SVC_DISCONNECT_NOTIFY && cmd.name.is_empty(),
            },
            now,
        );
    }
    window.lines.retain(|line| now < line.end_ms());
    window
        .bold
        .retain(|(start, _)| now.saturating_sub(*start) < BOLD_MSG_TIME_MS);
    if window.lines.is_empty() && window.bold.is_empty() {
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }
    if view.is_some_and(|v| v.in_killcam()) {
        hide(&mut pass);
        return;
    }

    let names_ok = match window.lines.back() {
        None => true,
        Some(KillfeedLine::Obituary {
            has_attacker,
            attacker,
            victim,
            ..
        }) => (!*has_attacker || !attacker.is_empty()) && !victim.is_empty(),
        Some(KillfeedLine::Notify { text, .. }) => !text.is_empty(),
    };
    match window.lines.back() {
        None => {}
        Some(KillfeedLine::Obituary { .. }) => {
            if !names_ok {
                gaps.raise(GapCause::ObituaryNoClientInfo);
            } else {
                gaps.clear(HudGap::Obituary);
            }
        }
        Some(KillfeedLine::Notify { name_empty, .. }) => {
            if *name_empty {
                gaps.raise(GapCause::GameNotifyNoClientInfo);
            } else if names_ok {
                gaps.clear(HudGap::Obituary);
            }
        }
    }

    let local_team = snapshot_client_team(&presented, local.0.0 as i32);
    let Some(snapshot) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let dvars = snapshot.meta.script_dvars(local.0);
    let Some(item) = catalog
        .as_ref()
        .and_then(|c| c.get("hud_fullscreen"))
        .and_then(|menu| {
            menu.items.iter().find(|item| {
                item.item_type == hud_iw4::ITEM_TYPE_GAME_MESSAGE_WINDOW
                    && item.game_msg_window_index == 0
            })
        })
    else {
        hide(&mut pass);
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let font = catalog.as_ref().and_then(|c| c.font(font_name));
    let (nscale, font_material) = match font {
        Some(def) => (
            normalized_text_scale(def.pixel_height, item.text_scale),
            asset_core::AssetRef::bare_name(&def.material).to_owned(),
        ),
        None => (0.0, String::new()),
    };
    if names_ok && font.is_none() {
        gaps.raise(GapCause::ObituaryNoClientInfo);
    }

    let miss = hud_images.miss_reason();
    let mut cmds = Vec::new();
    let mut fonts = HashMap::new();
    let font_tex_ok = if let Some(def) = font {
        fonts.insert(font_name.to_owned(), def);
        hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &font_material,
                &mut images,
            )
            .is_some()
    } else {
        false
    };
    if names_ok && font.is_some() && !font_tex_ok {
        let image = hud_images
            .zone_image_name(&font_material)
            .map(str::to_owned);
        gaps.raise(GapCause::FontAtlasMissing {
            material: font_material.clone(),
            image,
        });
    }

    let line_height = (item.text_scale * hud_iw4::GAME_MSG_CHAR_EM + 0.5).floor();
    let icon_spacing = font.map_or(0.0, |def| text_width(def, " ") as f32 * nscale);
    let scroll_offset: f32 = window
        .lines
        .iter()
        .map(|line| {
            let age = now.saturating_sub(line.start_ms());
            let fraction = (1.0 - age as f32 / GAME_MSG_WIN0_SCROLL_TIME_MS as f32).clamp(0.0, 1.0);
            (line_height * fraction + 0.5).floor()
        })
        .sum();
    for (i, line) in window.lines.iter().rev().enumerate() {
        let first_cmd = cmds.len();
        let age = now.saturating_sub(line.start_ms());

        let alpha = (age as f32 / GAME_MSG_WIN0_FADE_IN_TIME_MS as f32).clamp(0.0, 1.0)
            * ((line.end_ms() - now) as f32 / GAME_MSG_WIN0_FADE_OUT_TIME_MS as f32)
                .clamp(0.0, 1.0)
            * item.fore_color[3];
        let y_virtual = item.rect.y + scroll_offset - line_height * (i as f32 + 1.0);
        match line {
            KillfeedLine::Notify { text, .. } => {
                if font.is_some() && font_tex_ok {
                    let applied = surface.apply_rect(
                        item.rect.x,
                        y_virtual,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    cmds.push(text_cmd(
                        font_name,
                        item.text_style,
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        text.clone(),
                        [
                            item.fore_color[0],
                            item.fore_color[1],
                            item.fore_color[2],
                            1.0,
                        ],
                        "killfeed_game_msg",
                    ));
                }
            }
            KillfeedLine::Obituary {
                icon,
                icon_namespace,
                attacker,
                has_attacker,
                victim,
                attacker_team,
                victim_team,
                kill_icon_ratio,
                flip_kill_icon,
                ..
            } => {
                let mut x_virtual = item.rect.x;
                let line_names = (!*has_attacker || !attacker.is_empty())
                    && !victim.is_empty()
                    && font.is_some()
                    && font_tex_ok;
                if line_names
                    && *has_attacker
                    && let Some(def) = font
                {
                    let attacker_w = text_width(def, attacker) as f32 * nscale;
                    let applied = surface.apply_rect(
                        x_virtual,
                        y_virtual,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    cmds.push(text_cmd(
                        font_name,
                        item.text_style,
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        attacker.clone(),
                        team_name_color(local_team, *attacker_team, dvars),
                        "killfeed_obituary",
                    ));
                    x_virtual += attacker_w + icon_spacing;
                }

                let (icon_vw, icon_vh) = killicon_virtual_size(*kill_icon_ratio);
                let icon_scale = item.text_scale / hud_iw4::GAME_MSG_WIN0_TEXT_SCALE;
                let (icon_vw, icon_vh) = (icon_vw * icon_scale, icon_vh * icon_scale);
                let (s0, s1) = killicon_stretch_uv(*flip_kill_icon);
                let placed = surface.apply_rect(
                    x_virtual,
                    y_virtual - icon_vh,
                    icon_vw,
                    icon_vh,
                    i32::from(item.rect.horz_align),
                    i32::from(item.rect.vert_align),
                );
                let icon_ok = hud_images.get(*icon_namespace, icon, &mut images).is_some();
                if !icon_ok {
                    gaps.raise(GapCause::ObituaryKillIconMissing {
                        name: icon.clone(),
                        miss,
                    });
                } else {
                    cmds.push(Draw2dCmd {
                        material_namespace: *icon_namespace,
                        x: (placed.x + 0.5).floor(),
                        y: (placed.y + 0.5).floor(),
                        w: placed.w,
                        h: placed.h,
                        s0,
                        t0: 0.0,
                        s1,
                        t1: 1.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        material: icon.clone(),
                        op: Draw2dOp::StretchPic,
                        provenance: Draw2dProvenance::CgDraw {
                            site: "killfeed_obituary",
                        },
                        layer: 1,
                    });
                }
                if line_names {
                    x_virtual += icon_vw + icon_spacing;
                    let applied = surface.apply_rect(
                        x_virtual,
                        y_virtual,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    cmds.push(text_cmd(
                        font_name,
                        item.text_style,
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        victim.clone(),
                        team_name_color(local_team, *victim_team, dvars),
                        "killfeed_obituary",
                    ));
                }
            }
        }
        for cmd in &mut cmds[first_cmd..] {
            cmd.color[3] *= alpha;
        }
    }
    if let Some(def) = catalog
        .as_ref()
        .and_then(|c| c.font(HUD_SMALL_FONT))
        .filter(|_| !window.bold.is_empty())
    {
        let font_material = asset_core::AssetRef::bare_name(&def.material).to_owned();
        fonts.insert(HUD_SMALL_FONT.to_owned(), def);
        let bold_font_tex_ok = hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &font_material,
                &mut images,
            )
            .is_some();
        let scale = normalized_text_scale(def.pixel_height, BOLD_TEXT_SCALE);
        for (i, (start, text)) in window.bold.iter().enumerate().filter(|_| bold_font_tex_ok) {
            let age = now.saturating_sub(*start);
            let alpha = ((BOLD_MSG_TIME_MS - age) as f32 / 500.0).clamp(0.0, 1.0);
            let width = text_width(def, text) as f32 * scale;
            let line_h = def.pixel_height as f32 * scale;
            let applied = surface.apply_rect(
                -width / 2.0,
                BOLD_Y + line_h * i as f32,
                scale,
                scale,
                hud_iw4::ALIGN_CENTER,
                hud_iw4::ALIGN_VIEWABLE,
            );
            cmds.push(text_cmd(
                HUD_SMALL_FONT,
                hud_iw4::GAME_MSG_WIN0_TEXT_STYLE,
                applied.x,
                applied.y,
                applied.w,
                applied.h,
                font_material.clone(),
                text.clone(),
                [1.0, 1.0, 1.0, alpha],
                "killfeed_bold_msg",
            ));
        }
    }

    let list = Draw2dList { cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.killfeed = TessJob::Quads(quads);
}
