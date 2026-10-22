use bevy::prelude::*;
use frame::{LifeStarted, UiDraw, ViewSubject};
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};

use crate::draw2d::{Draw2dProvenance, Draw2dQuad};
use crate::gpu_list::{HudTessPass, TessJob};

#[derive(Component)]
pub(crate) struct DamageDirectionRaster;

#[derive(Clone, Copy, Default)]
struct Hit {
    time: i32,
    duration: i32,
    yaw: f32,
    stun: bool,
}

/// Modern Warfare 2's damage icons; a game with its own code HUD names its own.
const ENGINE_ICONS: game_api::hud::DamageIcons = game_api::hud::DamageIcons {
    slots: 12,
    time_ms: 6000,
    hidden_by_flash_and_mantle: true,
    stun_material: Some("hit_direction_stun"),
};

#[derive(Resource, Default)]
pub(crate) struct DamageDirections {
    hits: [Hit; ENGINE_ICONS.slots],
    event: Option<u32>,
    subject: Option<ViewSubject>,
    time: Option<i32>,
    weapon_frac: f32,
    ads_in: bool,
    random: u32,
    flash: Flash,
}

/// The red flash's view kick and when it ends, and the client's random
/// state its kick draws from.
#[derive(Clone, Copy, Default)]
struct Flash {
    pitch: f32,
    end: i32,
    random: u32,
}

impl Flash {
    /// A random float from `min` to `max`.
    fn flrand(&mut self, min: f32, max: f32) -> f32 {
        self.random = self.random.wrapping_mul(214013).wrapping_add(2531011);
        (self.random >> 17) as f32 * (max - min) * (1.0 / 32768.0) + min
    }

    /// A hit kicks the view: straight up when it came from no direction,
    /// else by how far ahead it came from and a random share of how far to
    /// the side; a hit from yaw and pitch 0 keeps the last kick.
    fn hit(&mut self, rules: &game_api::hud::DamageFlash, hit: FlashHit, now: i32) {
        let kick = hit.damage as f32 * rules.kick_scale;
        let kick = if kick < rules.kick_min || rules.kick_max < kick {
            if kick < rules.kick_min {
                rules.kick_min
            } else {
                rules.kick_max
            }
        } else {
            kick
        };
        let (yaw, pitch) = (hit.yaw, hit.pitch);
        if yaw == 255 && pitch == 255 {
            self.pitch = -kick;
        } else if yaw != 0 || pitch != 0 {
            let turn = 360.0 / 255.0;
            let dir = forward([pitch as f32 * turn, yaw as f32 * turn]);
            let [view_forward, view_left] = view_axis(hit.viewangles);
            let dot = |a: [f32; 3], b: [f32; 3]| a[1] * b[1] + a[2] * b[2] + a[0] * b[0];
            let side = dot(view_left, dir);
            let ahead = dot(view_forward, dir);
            self.pitch = ahead * kick;
            // The first draw goes to the roll, which the flash does not read.
            self.flrand(-rules.kick_random, rules.kick_random);
            self.pitch += self.flrand(-rules.kick_random, rules.kick_random) * side * kick;
        }
        self.end = now + rules.time_ms;
    }

    /// The flash this frame, if it plays.
    fn quad(
        &self,
        rules: &game_api::hud::DamageFlash,
        now: i32,
        size: [f32; 2],
    ) -> Option<Draw2dQuad> {
        if self.end <= now {
            return None;
        }
        let strength = ((self.end - now) as f32 * self.pitch * rules.alpha_per_ms).abs();
        let strength = if rules.alpha_max < strength {
            rules.alpha_max
        } else {
            strength
        };
        let mut color = rules.color;
        color[3] = strength * rules.color[3] * rules.alpha_scale;
        let [x0, y0] = [rules.corner; 2];
        let [x1, y1] = [x0 + size[0] + rules.grow, y0 + size[1] + rules.grow];
        Some(Draw2dQuad {
            xy: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color,
            material: "white".into(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            provenance: Draw2dProvenance::CgDraw {
                site: "damage_flash",
            },
            layer: 0,
            clip: None,
        })
    }
}

/// What a hit's flash reads: its damage, the direction it came from (as
/// yaw and pitch bytes) and the view's angles.
struct FlashHit {
    damage: i32,
    yaw: i32,
    pitch: i32,
    viewangles: [f32; 3],
}

/// The forward vector of a pitch and yaw in degrees.
fn forward([pitch, yaw]: [f32; 2]) -> [f32; 3] {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    [cp * cy, cp * sy, -sp]
}

/// The view's forward and left axes.
fn view_axis([pitch, yaw, roll]: [f32; 3]) -> [[f32; 3]; 2] {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sr, cr) = roll.to_radians().sin_cos();
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    [[cp * cy, cp * sy, -sp], right.map(|v| -v)]
}

pub(crate) fn spawn(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, DamageDirectionRaster);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Res<ViewSubject>,
    clock: Res<FrameClock>,
    ui_draw: Res<UiDraw>,
    aim: Res<net::ViewweaponAim>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    mode: Option<Res<game_api::ModeRules>>,
    mut started: MessageReader<LifeStarted>,
    mut directions: ResMut<DamageDirections>,
    mut pass: ResMut<HudTessPass>,
) {
    pass.damage_direction = TessJob::Hide;
    let game_hud = match mode.as_ref().map(|mode| mode.hud.code_hud) {
        Some(game_api::Rule::Known(game_api::hud::CodeHud::Game(hud))) => Some(hud),
        _ => None,
    };
    let icons = game_hud.map_or(ENGINE_ICONS, |hud| hud.damage_icons);
    let flash_rules = game_hud.and_then(|hud| hud.damage_flash);
    let slots = icons.slots.clamp(1, ENGINE_ICONS.slots);
    let now = clock.time();
    let reset = started.read().filter(|ev| ev.client == local.0.0).count() != 0
        || directions.subject != Some(*view)
        || directions.time.is_some_and(|time| now < time);
    if reset {
        *directions = DamageDirections::default();
    }
    directions.subject = Some(*view);
    directions.time = Some(now);
    let Some(ps) = presented.player(local.0) else {
        *directions = DamageDirections::default();
        return;
    };
    let dvars = presented
        .snapshot()
        .map(|snapshot| snapshot.meta.script_dvars(local.0));
    let hit = directions
        .event
        .is_some_and(|event| event != ps.damage_event)
        && ps.damage_count > 0;
    if let Some(rules) = flash_rules.as_ref()
        && hit
    {
        let hit = FlashHit {
            damage: ps.damage_count,
            yaw: ps.damage_yaw as i32,
            pitch: ps.damage_pitch as i32,
            viewangles: ps.viewangles,
        };
        directions.flash.hit(rules, hit, now);
    }
    if hit && !(ps.damage_yaw == 255 && ps.damage_pitch == 255) {
        let duration = dvars
            .and_then(|d| d.int("cg_hudDamageIconTime"))
            .unwrap_or(icons.time_ms)
            .max(0);
        let slot = directions.hits[..slots]
            .iter()
            .enumerate()
            .min_by_key(|(_, hit)| hit.time)
            .map(|(index, _)| index)
            .unwrap_or(0);
        directions.random = directions.random.wrapping_mul(214013).wrapping_add(2531011);
        let jitter = (((directions.random >> 16) & 0x7fff) as f32 / 32768.0 - 0.5) * 20.0;
        directions.hits[slot] = Hit {
            time: now,
            duration,
            yaw: ps.damage_yaw as f32 * (360.0 / 256.0) + jitter,
            stun: ps.damage_flags & 1 != 0,
        };
    }
    directions.event = Some(ps.damage_event);
    if ps.f_weapon_pos_frac != directions.weapon_frac {
        directions.ads_in = ps.f_weapon_pos_frac > directions.weapon_frac;
    }
    directions.weapon_frac = ps.f_weapon_pos_frac;
    let scoped = weapons.as_ref().is_some_and(|weapons| {
        let index = playerstate_iw4::get_viewmodel_weapon_index(ps);
        weapons
            .snapshot_weapon(presented.weapon_epoch(), index)
            .ok()
            .and_then(|weapon| weapon.hud_facts())
            .is_some_and(|facts| {
                hud_iw4::get_weap_reticle_zoom(
                    ps.f_weapon_pos_frac,
                    directions.ads_in,
                    &hud_iw4::WeaponAdsOverlayFacts {
                        ads_zoom_in_frac: facts.ads_zoom_in_frac,
                        ads_zoom_out_frac: facts.ads_zoom_out_frac,
                        overlay_material: u32::from(
                            weapons.registry().overlay_material_of(index).is_some(),
                        ),
                        overlay_reticle: facts.overlay_reticle,
                        ..default()
                    },
                )
                .active
            })
    });
    let flashed = hud_iw4::is_flashbanged(
        now,
        ps.shellshock_time,
        ps.shellshock_duration,
        presented
            .shellshock(local.0)
            .map_or(hud_iw4::SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
    ) != 0;
    let flash_or_mantle = flashed || ps.pm_flags & playerstate_iw4::pm_flags::MANTLE != 0;
    if !surface.is_ready() || !ui_draw.0 {
        return;
    }
    let mut quads: Vec<Draw2dQuad> = flash_rules
        .as_ref()
        .and_then(|rules| {
            directions
                .flash
                .quad(rules, now, [surface.width(), surface.height()])
        })
        .into_iter()
        .collect();
    if (icons.hidden_by_flash_and_mantle && flash_or_mantle)
        || (scoped
            && dvars
                .and_then(|d| d.int("cg_hudDamageIconInScope"))
                .unwrap_or(0)
                == 0)
        || dvars
            .and_then(|d| d.int("cg_drawDamageDirection"))
            .unwrap_or(1)
            == 0
        || (view.in_killcam() && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE)
    {
        if !quads.is_empty() {
            pass.damage_direction = TessJob::Quads(quads);
        }
        return;
    }
    let value = |name, default| {
        dvars
            .and_then(|d| d.float(name))
            .filter(|value| value.is_finite())
            .unwrap_or(default)
            .clamp(0.0, 512.0)
    };
    let scale = surface.scale_virtual_to_real();
    let half_width = value("cg_hudDamageIconWidth", 128.0) * scale[0] * 0.5;
    let offset = value("cg_hudDamageIconOffset", 128.0) * scale[1];
    let outer = offset + value("cg_hudDamageIconHeight", 64.0) * scale[1];
    let mut center = [surface.width() * 0.5, surface.height() * 0.5];
    if scoped && aim.live {
        let projection = crate::presentation_scale::PresentationScale::from_window(
            surface.width(),
            surface.height(),
        );
        let factor = projection.factor(crate::presentation_scale::ScaleClass::ProjectionBound);
        center[0] += aim.xhair_x * factor;
        center[1] += aim.xhair_y * factor;
    }
    for hit in &directions.hits[..slots] {
        let age = now.saturating_sub(hit.time);
        if age <= 0 || age >= hit.duration {
            continue;
        }
        let (sin, cos) = (ps.viewangles[1] - hit.yaw).to_radians().sin_cos();
        let xy = [
            [half_width, outer],
            [-half_width, outer],
            [-half_width, offset],
            [half_width, offset],
        ]
        .map(|[x, y]| [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]);
        quads.push(Draw2dQuad {
            xy,
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: [
                1.0,
                1.0,
                1.0,
                (2.0 - 2.0 * age as f32 / hit.duration as f32).min(1.0),
            ],
            material: match icons.stun_material {
                Some(stun) if hit.stun => stun,
                _ => "hit_direction",
            }
            .into(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            provenance: Draw2dProvenance::CgDraw {
                site: "damage_direction",
            },
            layer: 0,
            clip: None,
        });
    }
    pass.damage_direction = TessJob::Quads(quads);
}
