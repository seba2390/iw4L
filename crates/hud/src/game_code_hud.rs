//! The code-drawn HUD pieces of a game with its own rules
//! ([`game_api::hud::CodeHud::Game`]): its crosshair and its scoreboard,
//! drawn from the pictures and the layout the game's rules give.

use std::collections::{HashMap, HashSet};

use asset_game::SessionHudMenus;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use frame::ViewSubject;
use game_api::hud::{
    CodeHud, CodeScoreboard, CrosshairWeapon, GameCodeHud, ScoreAlign, ScoreValue,
};
use net::{ClientActionInput, LocalPresentClient, PresentedSnapshot};

use crate::chrome::{ChromeAssets, layout_text_width};
use crate::draw2d::{
    Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, Draw2dQuad, tessellate_fonts,
};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::scoreboard::{ALIGN_CENTER, ALIGN_SUB, ALIGN_VIEWABLE, client_state_name};
use crate::surface::Hud2dSurface;

#[derive(Component)]
pub(crate) struct GameCrosshairRaster;

#[derive(Component)]
pub(crate) struct GameScoreboardRaster;

pub(crate) fn spawn_crosshair(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, GameCrosshairRaster);
}

pub(crate) fn spawn_scoreboard(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, GameScoreboardRaster);
}

/// The game's own code HUD, when the mode has one.
pub(crate) fn game_hud(mode: Option<&game_api::ModeRules>) -> Option<&'static GameCodeHud> {
    match mode?.hud.code_hud {
        game_api::Rule::Known(CodeHud::Game(hud)) => Some(hud),
        _ => None,
    }
}

/// Which way the sights last moved.
#[derive(Default)]
pub(crate) struct AdsDirection {
    last_frac: f32,
    to_ads: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_crosshair(
    surface: Res<Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    cameras: Query<&Projection, With<Camera3d>>,
    view: Res<ViewSubject>,
    settings: Res<frame::GameSettings>,
    pause: Option<Res<frame::MatchPause>>,
    mode: Option<Res<game_api::ModeRules>>,
    aim: Res<net::ViewweaponAim>,
    mut pass: ResMut<HudTessPass>,
    mut ads: Local<AdsDirection>,
    mut reported: Local<HashSet<&'static str>>,
) {
    pass.game_crosshair = TessJob::Hide;
    let Some(hud) = game_hud(mode.as_deref()) else {
        return;
    };
    if !surface.is_ready() {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    if ps.f_weapon_pos_frac != ads.last_frac {
        ads.to_ads = ps.f_weapon_pos_frac > ads.last_frac;
    }
    ads.last_frac = ps.f_weapon_pos_frac;
    let Some(weapons) = weapons.as_deref() else {
        return;
    };
    let Some(tan_half) = cameras.iter().find_map(|p| match p {
        Projection::Perspective(persp) => Some((persp.fov * 0.5).tan()),
        _ => None,
    }) else {
        return;
    };
    let epoch = presented.weapon_epoch();
    let weapon = |index: u32| {
        let facts = weapons
            .snapshot_weapon(epoch, index)
            .ok()
            .and_then(|weapon| weapon.hud_facts())?;
        let reticle = weapons.registry().reticle_of(index);
        Some(CrosshairWeapon {
            weap_type: facts.weap_type,
            has_center: reticle.is_some_and(|r| r.center_material.is_some()),
            has_side: reticle.is_some_and(|r| r.side_material.is_some()),
            center_size: reticle.map_or(0, |r| r.center_size),
            side_size: facts.i_reticle_side_size,
            min_ofs: facts.i_reticle_min_ofs,
            side_pos: facts.hip_reticle_side_pos,
            ads_aim_pitch: facts.ads_aim_pitch,
            ads_crosshair_in_frac: facts.ads_crosshair_in_frac,
            ads_crosshair_out_frac: facts.ads_crosshair_out_frac,
            hip_spread_min: [
                facts.hip_spread_stand_min,
                facts.hip_spread_ducked_min,
                facts.hip_spread_prone_min,
            ],
            hip_spread_max: [
                facts.hip_spread_stand_max,
                facts.hip_spread_ducked_max,
                facts.hip_spread_prone_max,
            ],
            cook_off_hold: facts.cook_off_hold,
            keep_crosshair_when_ads: facts.keep_crosshair_when_ads,
            dual_wield: facts.dual_wield,
            ads_overlay: facts.overlay_reticle != 0
                || weapons.registry().overlay_material_of(index).is_some(),
            ads_zoom_in_frac: facts.ads_zoom_in_frac,
            ads_zoom_out_frac: facts.ads_zoom_out_frac,
            ads_overlay_width: facts.ads_overlay_width,
            ads_overlay_height: facts.ads_overlay_height,
            ads_overlay_alpha_scale: facts.ads_overlay_alpha_scale,
            color_change: facts.crosshair_color_change,
            human_gun: weapons.registry().name_of(index).starts_with("humangun_"),
        })
    };
    let dvars = presented
        .snapshot()
        .map(|snapshot| snapshot.meta.script_dvars(local.0));
    let dvar = |name: &str| dvars.and_then(|d| d.float(name));
    let third_person =
        crate::reticle::rendering_third_person(ps, &presented, local.0, &view, &settings);
    let player = ps.move_player(local.0.0 as i32, false);
    let crosshair = (hud.crosshair)(&game_api::hud::CrosshairView {
        player: &player,
        weapon: &weapon,
        third_person,
        to_ads: ads.to_ads,
        tan_half_fov_y: tan_half,
        gun_offset: if aim.live {
            [aim.xhair_x, aim.xhair_y]
        } else {
            [0.0; 2]
        },
        paused: pause.is_some_and(|pause| pause.0),
        dvar: &dvar,
    });
    for gap in crosshair.unrun {
        if reported.insert(gap.id) {
            diag::info!(Ui, "game gap {}: {}", gap.id, gap.what);
        }
    }
    let index = crate::reticle::viewmodel_weapon(ps);
    let mut quads = Vec::new();
    if let Some(overlay) = &crosshair.sight_overlay
        && let Some(material) = weapons.registry().overlay_material_of(index)
    {
        let namespace = weapons
            .registry()
            .component_namespace_of(index, asset_game::WeaponComponent::OverlayImage)
            .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
        sight_overlay_quads(&surface, overlay, material, namespace, &mut quads);
    }
    let Some(reticle) = weapons.registry().reticle_of(index) else {
        pass.game_crosshair = TessJob::Quads(quads);
        return;
    };
    let namespace = weapons
        .registry()
        .component_namespace_of(index, asset_game::WeaponComponent::Material)
        .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
    for picture in crosshair.pictures {
        let name = if picture.side {
            reticle
                .side_image
                .as_ref()
                .or(reticle.side_material.as_ref())
        } else {
            reticle
                .center_image
                .as_ref()
                .or(reticle.center_material.as_ref())
        };
        let Some(name) = name else {
            continue;
        };
        let r = surface.apply_rect(
            picture.x,
            picture.y,
            picture.w,
            picture.h,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        let center = [r.x + r.w * 0.5, r.y + r.h * 0.5];
        let (sin, cos) = picture.rotation.to_radians().sin_cos();
        let (hw, hh) = (r.w * 0.5, r.h * 0.5);
        let xy = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
            .map(|[x, y]| [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]);
        quads.push(Draw2dQuad {
            xy,
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: picture.color,
            material: name.clone(),
            material_namespace: namespace,
            provenance: Draw2dProvenance::CgDraw { site: "crosshair" },
            layer: 0,
            clip: None,
        });
    }
    pass.game_crosshair = TessJob::Quads(quads);
}

/// A sight overlay's pictures, and the screen around them filled from the
/// picture's outer edge: its first column beside them, its first row above
/// and below.
fn sight_overlay_quads(
    surface: &Hud2dSurface,
    overlay: &game_api::hud::SightOverlay,
    material: &str,
    namespace: asset_core::AssetNamespace,
    quads: &mut Vec<Draw2dQuad>,
) {
    let color = [1.0, 1.0, 1.0, overlay.alpha];
    let mut push = |x: f32, y: f32, w: f32, h: f32, [s0, t0, s1, t1]: [f32; 4]| {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        quads.push(Draw2dQuad {
            xy: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            st: [[s0, t0], [s1, t0], [s1, t1], [s0, t1]],
            color,
            material: material.to_owned(),
            material_namespace: namespace,
            provenance: Draw2dProvenance::CgDraw {
                site: "sight_overlay",
            },
            layer: 0,
            clip: None,
        });
    };
    for picture in &overlay.pictures {
        let r = surface.apply_rect(
            picture.x,
            picture.y,
            picture.w,
            picture.h,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        push(r.x, r.y, r.w, r.h, picture.st);
    }
    let [x, y, w, h] = overlay.frame;
    let r = surface.apply_rect(x, y, w, h, ALIGN_CENTER, ALIGN_CENTER);
    let (screen_w, screen_h) = (surface.width(), surface.height());
    let (left, right, top, bottom) = (r.x, r.x + r.w, r.y, r.y + r.h);
    push(0.0, 0.0, left, screen_h, [0.0, 0.0, 0.0, 1.0]);
    push(right, 0.0, screen_w - right, screen_h, [0.0, 0.0, 0.0, 1.0]);
    push(left, 0.0, right - left, top, [0.0, 0.0, 1.0, 0.0]);
    push(
        left,
        bottom,
        right - left,
        screen_h - bottom,
        [0.0, 0.0, 1.0, 0.0],
    );
}

/// The game's grenade indicators, drawn after its crosshair.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_grenade_indicators(
    surface: Res<Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    view: Res<ViewSubject>,
    settings: Res<frame::GameSettings>,
    clock: Res<net::FrameClock>,
    mode: Option<Res<game_api::ModeRules>>,
    hud_images: Res<crate::images::HudImages>,
    aim: Res<net::ViewweaponAim>,
    mut pass: ResMut<HudTessPass>,
    mut reported: Local<HashSet<&'static str>>,
) {
    let Some(draw) = game_hud(mode.as_deref()).and_then(|hud| hud.grenade_indicators) else {
        return;
    };
    let (Some(ps), Some(snap), Some(weapons)) = (
        presented.player(local.0),
        presented.snapshot(),
        weapons.as_deref(),
    ) else {
        return;
    };
    if !surface.is_ready() {
        return;
    }
    let time = clock.time();
    let team = |client: sim::ClientId| {
        snap.meta
            .clients
            .iter()
            .find(|(id, _)| *id == client)
            .map_or(0, |(_, meta)| meta.client_state_team)
    };
    let local_team = team(local.0);
    let missiles: Vec<_> = presented
        .presented_projectiles()
        .iter()
        .map(|row| game_api::hud::IndicatorMissile {
            weapon: row.weapon(),
            origin: row.origin_at(time),
            delta: row.pos().tr_delta,
            time: row.launch_time(),
            owner_local: row.owner() == local.0,
            owner_teammate: local_team != 0 && team(row.owner()) == local_team,
        })
        .collect();
    let epoch = presented.weapon_epoch();
    let weapon = |index: u32| {
        let facts = weapons
            .snapshot_weapon(epoch, index)
            .ok()
            .and_then(|weapon| weapon.hud_facts())?;
        Some(game_api::hud::IndicatorWeapon {
            weap_type: facts.weap_type,
            proj_explosion_type: facts.proj_explosion_type,
            show_indicator: facts.show_indicator,
            timed_detonation: facts.timed_detonation,
            explosion_radius: facts.explosion_radius,
            indicator_radius: facts.indicator_radius,
            stickiness: facts.stickiness,
            fuse_time_ms: facts.fuse_time_ms,
            has_icon: indicator_icon(weapons, index).is_some(),
            ads_overlay: facts.overlay_reticle != 0
                || weapons.registry().overlay_material_of(index).is_some(),
        })
    };
    let dvars = snap.meta.script_dvars(local.0);
    let dvar = |name: &str| dvars.float(name);
    let player = ps.move_player(local.0.0 as i32, false);
    let indicators = draw(&game_api::hud::GrenadeIndicatorView {
        player: &player,
        missiles: &missiles,
        weapon: &weapon,
        third_person: crate::reticle::rendering_third_person(
            ps, &presented, local.0, &view, &settings,
        ),
        gun_offset: if aim.live {
            [aim.xhair_x, aim.xhair_y]
        } else {
            [0.0; 2]
        },
        time,
        dvar: &dvar,
    });
    for gap in indicators.unrun {
        if reported.insert(gap.id) {
            diag::info!(Ui, "game gap {}: {}", gap.id, gap.what);
        }
    }
    let mut quads = Vec::new();
    for picture in indicators.pictures {
        let material = match picture.material {
            game_api::hud::IndicatorMaterial::Named(name) => name,
            game_api::hud::IndicatorMaterial::WeaponIcon(index) => {
                let Some(name) = indicator_icon(weapons, index) else {
                    continue;
                };
                name
            }
        };
        // The game's own materials are in its zones' images when the match
        // loaded them there, else in the code zone the menus come from.
        let namespace = hud_images
            .map_namespace()
            .filter(|ns| hud_images.has_zone_image(*ns, material))
            .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
        let r = surface.apply_rect(
            picture.x,
            picture.y,
            picture.w,
            picture.h,
            ALIGN_CENTER,
            ALIGN_CENTER,
        );
        let center = [r.x + r.w * 0.5, r.y + r.h * 0.5];
        let (sin, cos) = picture.rotation.to_radians().sin_cos();
        let (hw, hh) = (r.w * 0.5, r.h * 0.5);
        let xy = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
            .map(|[x, y]| [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]);
        quads.push(Draw2dQuad {
            xy,
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: picture.color,
            material: material.into(),
            material_namespace: namespace,
            provenance: Draw2dProvenance::CgDraw {
                site: "grenade_indicators",
            },
            layer: 0,
            clip: None,
        });
    }
    if quads.is_empty() {
        return;
    }
    match &mut pass.game_crosshair {
        TessJob::Quads(drawn) => drawn.extend(quads),
        job => *job = TessJob::Quads(quads),
    }
}

fn indicator_icon(weapons: &PreparedWeapons, index: u32) -> Option<&str> {
    weapons
        .registry()
        .reticle_of(index)?
        .indicator_icon
        .as_deref()
}

/// When the board was last held, for its fade.
#[derive(Default)]
pub(crate) struct ScoreboardFade {
    released_at: Option<i32>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_scoreboard(
    surface: Res<Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    actions: Option<Res<ClientActionInput>>,
    menus: Option<Res<SessionHudMenus>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    pause: Option<Res<frame::MatchPause>>,
    clock: Res<net::FrameClock>,
    mode: Option<Res<game_api::ModeRules>>,
    mut pass: ResMut<HudTessPass>,
    mut fade: Local<ScoreboardFade>,
) {
    pass.game_scoreboard = TessJob::Hide;
    let Some(board) = game_hud(mode.as_deref()).and_then(|hud| hud.scoreboard) else {
        return;
    };
    let Some(snap) = presented.snapshot() else {
        return;
    };
    if !surface.is_ready() || pause.is_some_and(|pause| pause.0) {
        return;
    }
    let now = clock.time();
    let held = actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
    let alpha = if held {
        fade.released_at = None;
        1.0
    } else {
        let released = *fade.released_at.get_or_insert(now);
        let age = now - released;
        if age < 0 || age >= board.fade_ms || board.fade_ms <= 0 {
            return;
        }
        (board.fade_ms - age) as f32 / board.fade_ms as f32
    };
    if !held && fade.released_at == Some(now) && alpha >= 1.0 {
        return;
    }
    let Some(catalog) = menus.as_ref().and_then(|m| m.0.as_deref()) else {
        return;
    };
    let assets = ChromeAssets {
        catalog: Some(catalog),
        localize: strings.as_ref().map(|s| &s.0),
    };
    let mut rows: Vec<_> = snap.meta.clients.iter().collect();
    rows.sort_by_key(|(id, _)| id.0);
    let list = draw_board(board, &assets, &surface, &rows, alpha);
    let Some(list) = list else {
        return;
    };
    let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
    for cmd in &list.cmds {
        if let Draw2dOp::TextRun { font, .. } = &cmd.op
            && let Some(def) = catalog.font(font)
        {
            fonts.insert(font.clone(), def);
        }
    }
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.game_scoreboard = TessJob::Quads(quads);
    }
}

struct BoardText<'a> {
    font_name: &'static str,
    font: &'a asset_game::FontDef,
    layout: &'static dyn game_api::MenuLayout,
}

impl BoardText<'_> {
    fn width(&self, text: &str, scale: f32) -> f32 {
        layout_text_width(self.layout, self.font, text, scale).trunc()
    }
}

fn draw_board(
    board: &CodeScoreboard,
    assets: &ChromeAssets<'_>,
    surface: &Hud2dSurface,
    rows: &[&(sim::ClientId, sim::ClientSnapshotMeta)],
    alpha: f32,
) -> Option<Draw2dList> {
    let layout = assets.layout()?;
    let text_font = |scale: f32| {
        let name = assets.font_name(board.font, surface.scale_virtual_to_real()[1], scale)?;
        Some(BoardText {
            font_name: name,
            font: assets.catalog?.font(name)?,
            layout,
        })
    };
    let namespace = assets.namespace();
    let mut cmds = Vec::new();
    let place = surface.placement();
    let viewable = place.virtual_viewable_max[0] - place.virtual_viewable_min[0];
    let x0 = ((viewable - board.width) * 0.5).max(0.0);
    let top = ((480.0 - board.height) * 0.5 + board.top_offset).max(0.0);
    let inner = board.width - board.inset * 2.0;
    let mut text =
        |font: &BoardText<'_>, x: f32, y: f32, scale: f32, style: i32, s: &str, color| {
            let draw_scale = layout.text_scale(font.font.pixel_height, scale);
            let r = surface.apply_rect(x, y, draw_scale, draw_scale, ALIGN_VIEWABLE, ALIGN_SUB);
            cmds.push(Draw2dCmd {
                x: (r.x + 0.5).floor(),
                y: (r.y + 0.5).floor(),
                w: r.w,
                h: r.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color,
                material: asset_core::AssetRef::bare_name(&font.font.material).to_owned(),
                material_namespace: namespace,
                op: Draw2dOp::TextRun {
                    font: font.font_name.to_owned(),
                    scale: draw_scale,
                    text: s.to_owned(),
                    loc_key: String::new(),
                    style,
                    fx: None,
                    glow: None,
                },
                provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
                layer: 2,
            });
        };

    // The column titles, centred in their columns.
    let header_y = top + board.header_offset + board.item_height + 4.0;
    let header_font = text_font(board.header_scale)?;
    let mut x = x0 + board.inset;
    for column in board.columns {
        if !column.title.is_empty() {
            let title = assets
                .localize
                .and_then(|l| l.text(column.title))
                .unwrap_or(column.title);
            let w = header_font.width(title, board.header_scale);
            text(
                &header_font,
                (column.width * inner - w) * 0.5 + x,
                header_y + board.header_height,
                board.header_scale,
                3,
                title,
                [1.0, 1.0, 1.0, alpha],
            );
        }
        x += column.width * inner;
    }

    // One bar per player while it fits above the footer.
    let mut bars = Vec::new();
    let mut y = header_y + board.header_height + board.row_gap;
    let limit = top + board.height - board.bottom_room;
    for (id, meta) in rows.iter().copied() {
        if y + board.item_height > limit {
            break;
        }
        let n = id.0 as usize;
        if let Some(material) = board
            .bar_materials
            .get(n.min(board.bar_materials.len() - 1))
        {
            let r = surface.apply_rect(
                x0 + board.inset,
                y,
                inner,
                board.item_height,
                ALIGN_VIEWABLE,
                ALIGN_SUB,
            );
            bars.push(Draw2dCmd {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: [
                    board.bar_color[0],
                    board.bar_color[1],
                    board.bar_color[2],
                    alpha * board.bar_alpha,
                ],
                material: (*material).to_owned(),
                material_namespace: namespace,
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
                layer: 1,
            });
        }
        let color = board
            .name_colors
            .get(n)
            .copied()
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let color = [color[0], color[1], color[2], alpha];
        let font = text_font(board.row_scale)?;
        let mut x = x0 + board.inset;
        for column in board.columns {
            let col = column.width * inner;
            let value = match column.value {
                ScoreValue::Name => client_state_name(&meta.name).unwrap_or("").to_owned(),
                ScoreValue::Score => meta.score.to_string(),
                ScoreValue::Kills => meta.kills.to_string(),
                ScoreValue::Headshots => meta.zombie_stats[2].to_string(),
            };
            let mut scale = board.row_scale;
            while font.width(&value, scale) > col {
                scale -= board.scale_step;
            }
            let style = if scale < board.plain_below { 0 } else { 3 };
            let w = font.width(&value, scale);
            let dx = match column.align {
                ScoreAlign::Center => (col - w) * 0.5,
                ScoreAlign::Right => col - w - 4.0,
                ScoreAlign::Left => 0.0,
            };
            let text_h = layout.text_height(font.font.pixel_height, scale);
            text(
                &font,
                x + dx,
                y + (text_h + board.item_height) * board.text_offset,
                scale,
                style,
                &value,
                color,
            );
            x += col;
        }
        y += board.item_height + board.row_gap;
    }

    // The footer: the host's name at the left, its scale stepped down until
    // it fits.
    let mut scale = board.footer_scale;
    let font = text_font(scale)?;
    while font.width(board.hostname, scale) + 4.0 > board.width - board.footer_height
        && scale > board.footer_floor
    {
        scale -= board.footer_step;
    }
    let text_h = layout.text_height(font.font.pixel_height, scale);
    text(
        &font,
        x0 + board.inset,
        top + board.height - board.footer_bottom - (board.footer_height - text_h) * 0.5,
        scale,
        3,
        board.hostname,
        [1.0, 1.0, 1.0, alpha],
    );
    bars.extend(cmds);
    Some(Draw2dList { cmds: bars })
}
