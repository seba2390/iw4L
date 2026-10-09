//! The Black Ops zombies HUD the client draws itself: the points column and
//! the weapon info in the bottom right. Round chalk, perks and power-ups are
//! script hud elems.

use std::collections::HashMap;

use asset_game::{FontDef, MenuCatalog};
use assets::BoundWeapons;
use bevy::prelude::*;
use playerstate_iw4::{PM_TYPE_DEAD, PM_TYPE_INTERMISSION, PM_TYPE_SPECTATOR, PlayerState};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gpu_list::TessJob;
use crate::images::HudImages;
use crate::surface::Hud2dSurface;

/// Row colours of the points column, one per player slot.
pub(crate) const POINT_COLORS: [[f32; 4]; 4] = [
    [1.0, 1.0, 1.0, 1.0],
    [0.49, 0.81, 0.93, 1.0],
    [0.96, 0.79, 0.31, 1.0],
    [0.51, 0.93, 0.53, 1.0],
];

/// Where the zombie scripts expect the local score (`score_highlight`): its
/// popups end at x -103 and centre on y -100, rows 20 apart upwards.
const SCORE_X: f32 = -96.0;
const SCORE_Y: f32 = -100.0;
const SCORE_ROW: f32 = 20.0;
const SCORE_SCALE: f32 = 0.4;

const AMMO_RIGHT: f32 = -28.0;
const AMMO_BASELINE: f32 = -26.0;
const CLIP_SCALE: f32 = 0.7;
const STOCK_SCALE: f32 = 0.42;
const NAME_BASELINE: f32 = -64.0;
const NAME_SCALE: f32 = 0.3;
const GRENADE_SIZE: f32 = 18.0;
const GRENADE_STEP: f32 = 10.0;
const MAX_GRENADE_ICONS: i32 = 4;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const EMPTY_CLIP: [f32; 4] = [0.9, 0.12, 0.1, 1.0];

struct HudFont<'a> {
    name: &'static str,
    def: &'a FontDef,
    material: String,
}

fn hud_font<'a>(catalog: &'a MenuCatalog, names: &[&'static str]) -> Option<HudFont<'a>> {
    names.iter().find_map(|&name| {
        let def = catalog.font(name)?;
        Some(HudFont {
            name,
            def,
            material: asset_core::AssetRef::bare_name(&def.material).to_owned(),
        })
    })
}

/// A text run whose left edge (or right edge, with `right`) sits at `x`, on
/// baseline `y`, both in virtual units from the bottom right.
#[allow(clippy::too_many_arguments)]
fn text(
    surface: &Hud2dSurface,
    font: &HudFont<'_>,
    scale: f32,
    text: String,
    x: f32,
    y: f32,
    right: bool,
    color: [f32; 4],
    site: &'static str,
) -> Draw2dCmd {
    let nscale = hud_iw4::normalized_text_scale(font.def.pixel_height, scale);
    let width = if right {
        crate::chrome::ui_text_width(font.def, &text, scale)
    } else {
        0.0
    };
    let rect = surface.apply_rect(
        x - width,
        y,
        nscale,
        nscale,
        hud_iw4::ALIGN_USER_MAX,
        hud_iw4::ALIGN_USER_MAX,
    );
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material: font.material.clone(),
        op: Draw2dOp::TextRun {
            font: font.name.to_owned(),
            scale: nscale,
            text,
            loc_key: String::new(),
            style: 3,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site },
        layer: 1,
    }
}

fn finish(
    mut cmds: Vec<Draw2dCmd>,
    fonts: &[&HudFont<'_>],
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
) -> TessJob {
    cmds.retain(|cmd| {
        hud_images
            .get(cmd.material_namespace, &cmd.material, images)
            .is_some()
    });
    let fonts: HashMap<String, &FontDef> = fonts
        .iter()
        .map(|font| (font.name.to_owned(), font.def))
        .collect();
    let (quads, _) = tessellate_fonts(&Draw2dList { cmds }, &fonts);
    if quads.is_empty() {
        TessJob::Hide
    } else {
        TessJob::Quads(quads)
    }
}

/// The points column: the local player at the bottom, the others above.
pub(crate) fn points(
    surface: &Hud2dSurface,
    catalog: &MenuCatalog,
    scores: &[(bool, i32)],
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
) -> TessJob {
    let Some(font) = hud_font(catalog, &["fonts/hudbigfont", "fonts/bigfont"]) else {
        return TessJob::Hide;
    };
    let cap = font.def.pixel_height as f32
        * hud_iw4::normalized_text_scale(font.def.pixel_height, SCORE_SCALE)
        * 0.35;
    let cmds = scores
        .iter()
        .enumerate()
        .map(|(row, (_, score))| {
            text(
                surface,
                &font,
                SCORE_SCALE,
                score.to_string(),
                SCORE_X,
                SCORE_Y - row as f32 * SCORE_ROW + cap,
                false,
                POINT_COLORS[row % POINT_COLORS.len()],
                "zombie_points",
            )
        })
        .collect();
    finish(cmds, &[&font], hud_images, images)
}

/// Clip, stock, weapon name and grenades of the held weapon.
pub(crate) struct WeaponInfo<'a> {
    pub ps: &'a PlayerState,
    pub weapons: Option<&'a BoundWeapons<'a>>,
    pub clip: Option<i32>,
    pub stock: Option<i32>,
    pub name: Option<String>,
    pub frags: i32,
}

pub(crate) fn weapon_info(
    surface: &Hud2dSurface,
    catalog: &MenuCatalog,
    info: WeaponInfo<'_>,
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
) -> TessJob {
    if matches!(
        info.ps.pm_type,
        PM_TYPE_SPECTATOR | PM_TYPE_INTERMISSION | PM_TYPE_DEAD..
    ) {
        return TessJob::Hide;
    }
    let (Some(big), Some(small)) = (
        hud_font(catalog, &["fonts/hudbigfont", "fonts/bigfont"]),
        hud_font(catalog, &["fonts/hudsmallfont", "fonts/smallfont"]),
    ) else {
        return TessJob::Hide;
    };
    let mut cmds = Vec::new();
    let mut left = AMMO_RIGHT;
    if let Some(stock) = info.stock {
        let stock = format!("/{stock}");
        left -= crate::chrome::ui_text_width(big.def, &stock, STOCK_SCALE);
        cmds.push(text(
            surface,
            &big,
            STOCK_SCALE,
            stock,
            AMMO_RIGHT,
            AMMO_BASELINE,
            true,
            WHITE,
            "zombie_stock",
        ));
    }
    if let Some(clip) = info.clip {
        let clip = clip.to_string();
        let color = if clip == "0" { EMPTY_CLIP } else { WHITE };
        left -= crate::chrome::ui_text_width(big.def, &clip, CLIP_SCALE) + 2.0;
        cmds.push(text(
            surface,
            &big,
            CLIP_SCALE,
            clip,
            left,
            AMMO_BASELINE,
            false,
            color,
            "zombie_clip",
        ));
    }
    if let Some(name) = info.name {
        cmds.push(text(
            surface,
            &small,
            NAME_SCALE,
            name,
            AMMO_RIGHT,
            NAME_BASELINE,
            true,
            WHITE,
            "zombie_weapon_name",
        ));
    }
    if let Some((material, namespace)) = info.weapons.and_then(|weapons| {
        let index = crate::ammo::offhand_weapon_index(info.ps, weapons, info.ps.offhand_primary)?;
        Some((
            weapons.registry().hud_icon_image_of(index)?,
            weapons.registry().hud_icon_namespace_of(index)?,
        ))
    }) {
        for i in 0..info.frags.min(MAX_GRENADE_ICONS) {
            let rect = surface.apply_rect(
                left - 12.0 - GRENADE_SIZE - i as f32 * GRENADE_STEP,
                AMMO_BASELINE - GRENADE_SIZE,
                GRENADE_SIZE,
                GRENADE_SIZE,
                hud_iw4::ALIGN_USER_MAX,
                hud_iw4::ALIGN_USER_MAX,
            );
            cmds.push(Draw2dCmd {
                material_namespace: namespace,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: WHITE,
                material: material.to_owned(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw {
                    site: "zombie_grenades",
                },
                layer: 1,
            });
        }
    }
    finish(cmds, &[&big, &small], hud_images, images)
}
