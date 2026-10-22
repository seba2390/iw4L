//! Black Ops' own HUD in a Black Ops session: the menus of its HUD menu lists
//! (`weaponinfo_zombie`, `dpad_zombie`, `competitivemodescores`, ...) drawn
//! through the menu engine, each shown by its own visibility expression.
//!
//! Owner draws paint only what their Black Ops name states (the clip count of
//! `CG_PLAYER_WEAPON_AMMO_CLIP` in the item's font, ...); the ones whose draw
//! rule lives only in the executable report a gap and draw nothing
//! (docs/fidelity/t5.md, "Waiting on the maintainer").

use asset_game::{LocalizeCatalog, MenuCatalog, T5_HUD_MENU_LISTS};
use assets::BoundWeapons;
use hud_iw4::{ExprError, ExprHost, Operand};
use playerstate_iw4::{PM_TYPE_DEAD, PlayerState};

use crate::ammo::{WeaponbarAmmo, offhand_weapon_index};
use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeGapKind, ChromeMenuAnim, MenuVisOnError, OwnerDrawArgs,
    OwnerDrawPaint, execute_chrome_menu_ex, push_owner_text,
};
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};

const CG_PLAYER_HEAT_VALUE: i32 = 7;
const CG_CURSORHINT: i32 = 72;
const CG_PLAYER_WEAPON_NAME: i32 = 81;
const CG_DEADQUOTE: i32 = 97;
const CG_OFFHAND_WEAPON_ICON_FRAG: i32 = 103;
const CG_OFFHAND_WEAPON_ICON_SMOKEFLASH: i32 = 104;
const CG_PLAYER_WEAPON_BACKGROUND: i32 = 116;
const CG_PLAYER_WEAPON_AMMO_CLIP: i32 = 120;
const CG_PLAYER_WEAPON_AMMO_CLIP_DUAL_WIELD: i32 = 121;
const CG_PLAYER_WEAPON_AMMO_STOCK: i32 = 122;
const CG_PLAYER_WEAPON_LOW_AMMO_WARNING: i32 = 123;
const CG_PLAYER_ACTIONSLOT_1: i32 = 166;
const CG_PLAYER_ACTIONSLOT_4: i32 = 169;
/// The second `vehiclehud` owner draw and `amitalking`: neither of Black
/// Ops' owner draw dispatches (text and picture) has a case for them in its
/// single player, so they draw nothing.
const CG_VEHICLE_HUD_2: i32 = 193;
const CG_AMITALKING: i32 = 266;
const CG_COMPETITIVE_MODE_SCORES: i32 = 288;
const CG_PLAYER_ACTIONSLOT_ARROW_1: i32 = 294;
const CG_PLAYER_ACTIONSLOT_ARROW_4: i32 = 297;

/// What the local client knows that Black Ops' HUD expressions ask for.
pub(crate) struct T5HudState<'a> {
    pub ms: i32,
    pub dvars: sim::ScriptDvars<'a>,
    pub ui_dvars: Option<&'a frame::UiMenuDvars>,
    pub zombies: bool,
    pub input: Option<&'a frame::HudInputView>,
    pub vis: crate::weaponbar::HudPlayerVis,
    pub in_killcam: bool,
    pub ps: &'a PlayerState,
    pub weapons: Option<&'a BoundWeapons<'a>>,
    pub ammo: Option<WeaponbarAmmo>,
    pub weapon_name: Option<String>,
    pub hide_ammo: bool,
    pub ammo_widths: [i32; 2],
    pub hint: Option<T5Hint>,
    pub local_client: usize,
    pub scores: Vec<(usize, i32)>,
    pub deadquote_start_ms: Option<i32>,
    /// What the game's hint and message owner draws show, by number.
    pub owner_hints: Vec<(i32, OwnerHintText)>,
    /// The game's weapon info rules, what they read and the client time
    /// the weapon info was last shown.
    pub weapon_info: Option<WeaponInfoFrame<'a>>,
}

/// The game's weapon info for a frame.
#[derive(Clone, Copy)]
pub(crate) struct WeaponInfoFrame<'a> {
    pub rules: &'static game_api::hud::WeaponInfoRules,
    pub view: &'a game_api::hud::WeaponInfoView<'a>,
    pub shown_at: i32,
}

/// A hint or message owner draw, resolved for the frame.
#[derive(Clone, Debug)]
pub(crate) enum OwnerHintText {
    Nothing,
    /// Text centred on the item, in the colour the game draws it.
    Text {
        text: String,
        white: bool,
    },
}

/// The owner draws the game's hint rule answers for.
const OWNER_HINT_DRAWS: [i32; 14] = [
    71, 80, 90, 113, 132, 192, 195, 196, 197, 199, 200, 311, 316, 322,
];

/// Resolves the game's hint and message owner draws for the local player;
/// a part of the game's rule the runtime does not run is logged once by its
/// id and draws nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_owner_hints(
    hud: &game_api::hud::GameCodeHud,
    ps: &PlayerState,
    local: i32,
    weapons: Option<&BoundWeapons<'_>>,
    scoreboard: bool,
    paused: bool,
    dvars: sim::ScriptDvars<'_>,
    strings: Option<&assets::PreparedLocalizedStrings>,
    namespace: asset_core::AssetNamespace,
    input: Option<&frame::HudInputView>,
    reported: &mut std::collections::HashSet<&'static str>,
) -> Vec<(i32, OwnerHintText)> {
    let player = ps.move_player(local, false);
    let weapon = |index: u32| {
        let facts = weapons?.row(index)?.hud_facts()?;
        let [a, b, c] = facts.ads_zoom_fovs;
        Some(game_api::hud::HintWeapon {
            ads_overlay: facts.overlay_reticle != 0
                || weapons?.registry().overlay_material_of(index).is_some(),
            weap_class: facts.weap_class,
            zoom_varies: a != b || b != c,
        })
    };
    let dvar = |name: &str| dvars.float(name);
    let view = game_api::hud::OwnerHintView {
        player: &player,
        weapon: &weapon,
        scoreboard,
        paused,
        dvar: &dvar,
    };
    let mut out = Vec::new();
    for owner_draw in OWNER_HINT_DRAWS {
        let shown = match (hud.owner_hint)(owner_draw, &view) {
            game_api::hud::OwnerHint::NotHint => continue,
            game_api::hud::OwnerHint::Nothing => OwnerHintText::Nothing,
            game_api::hud::OwnerHint::Unrun(gap) => {
                if reported.insert(gap.id) {
                    diag::info!(Ui, "game gap {}: {}", gap.id, gap.what);
                }
                OwnerHintText::Nothing
            }
            game_api::hud::OwnerHint::Text { key, binds } => {
                let Some(template) = strings.and_then(|s| s.0.text_in(namespace, key)) else {
                    continue;
                };
                let bind = binds
                    .iter()
                    .find_map(|cmd| input.and_then(|i| i.binding_keys.get(*cmd)))
                    .map_or("", String::as_str);
                OwnerHintText::Text {
                    text: template.replace("&&1", bind),
                    white: owner_draw != 80,
                }
            }
        };
        out.push((owner_draw, shown));
    }
    out
}

/// Paints a resolved hint: Black Ops centres it on the item by half its
/// width, rounded to the nearest whole unit.
fn paint_owner_hint(
    args: &OwnerDrawArgs<'_>,
    hint: &OwnerHintText,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let OwnerHintText::Text { text, white } = hint else {
        return OwnerDrawPaint::Painted;
    };
    let color = if *white { [1.0; 4] } else { args.color };
    match crate::chrome::push_owner_text_at(
        args,
        text,
        color,
        |width, _| {
            let half = (f64::from(width) * 0.5 + 9.313_225_746_154_785e-10).round_ties_even();
            (args.rect.x - half as f32, args.rect.y)
        },
        frame,
    ) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

/// Latches the client time when `ui_deadquote` is set by `MissionFailed` (`'Y'`),
/// driving `deadscreen` owner draw 97 (`CG_DEADQUOTE`) and the post-`g_deathDelay`
/// black screen fade (`cg_deathScreenFadeOutTime`).
#[derive(Default)]
pub(crate) struct T5DeadscreenLatch {
    start_ms: Option<i32>,
}

impl T5DeadscreenLatch {
    pub(crate) fn update(
        &mut self,
        dvars: sim::ScriptDvars<'_>,
        ui_dvars: Option<&frame::UiMenuDvars>,
        now_ms: i32,
    ) -> Option<i32> {
        let quote = dvars
            .string("ui_deadquote")
            .or_else(|| ui_dvars.and_then(|d| d.get("ui_deadquote")))
            .unwrap_or("");
        if quote.trim().is_empty() {
            self.start_ms = None;
        } else if self.start_ms.is_none() {
            self.start_ms = Some(now_ms);
        }
        self.start_ms
    }
}

/// Per-client displayed score stepped toward the snapshot score each frame by
/// Black Ops' competitive score helper (`±1` within 15 points, else
/// `trunc(diff * 0.2 + 1.0)`).
#[derive(Default)]
pub(crate) struct T5ScoreLatch {
    displayed: [i32; 4],
}

impl T5ScoreLatch {
    pub(crate) fn step(&mut self, client: usize, target: i32) -> i32 {
        let Some(current) = self.displayed.get_mut(client) else {
            return target;
        };
        let diff = target - *current;
        if diff != 0 {
            let step = if (1..=15).contains(&diff) {
                1
            } else if (-15..=-1).contains(&diff) {
                -1
            } else {
                ((diff as f64) * 0.2 + 1.0) as i32
            };
            *current += step;
            if (*current > target && diff > 0) || (*current < target && diff < 0) {
                *current = target;
            }
        }
        *current
    }
}

/// The cursor hint the owner draw shows, latched as Black Ops' client game
/// latches it: the last hint the player state carried, faded out over
/// `cg_hintFadeTime` (100 ms) once it is gone.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct T5Hint {
    pub kind: i32,
    pub text: Option<String>,
    /// A weapon hint's display name (`cg_weaponHintsCoD1Style` 1), drawn
    /// beside the prompt and above the icon.
    pub weapon_name: Option<String>,
    /// `(namespace, material, scale_x, scale_y, offset_x_frac, dual_wield)`.
    pub icon: Option<(asset_core::AssetNamespace, String, f32, f32, f32, bool)>,
    pub alpha: f32,
}

/// Black Ops' `cg_hintFadeTime`.
const HINT_FADE_MS: i32 = 100;

/// The hint latched from the player state: its type, string and the client
/// time it was last seen.
#[derive(Default)]
pub(crate) struct T5HintLatch {
    kind: i32,
    string: i32,
    seen_ms: i32,
}

impl T5HintLatch {
    /// Latches the player state's hint and gives the fade alpha: whole until
    /// the hint is gone, then down to nothing over the fade time's last 100 ms.
    pub(crate) fn update(&mut self, ps: &PlayerState, now_ms: i32) -> Option<(i32, i32, f32)> {
        if ps.cursor_hint != 0 {
            *self = Self {
                kind: ps.cursor_hint,
                string: ps.cursor_hint_string,
                seen_ms: now_ms,
            };
        }
        if self.seen_ms == 0 {
            return None;
        }
        let elapsed = now_ms.wrapping_sub(self.seen_ms);
        if elapsed >= HINT_FADE_MS {
            self.kind = 0;
            return None;
        }
        let left = HINT_FADE_MS - elapsed;
        let alpha = if left >= 100 {
            1.0
        } else {
            left as f32 / 100.0
        };
        Some((self.kind, self.string, alpha))
    }
}

impl T5HudState<'_> {
    fn dvar(&self, name: &str) -> Option<String> {
        if let Some(value) = self.dvars.string(name) {
            return Some(value.to_owned());
        }
        if let Some(value) = self.ui_dvars.and_then(|d| d.get(name)) {
            return Some(value.to_owned());
        }
        let flag = |on: bool| String::from(if on { "1" } else { "0" });
        match name.to_ascii_lowercase().as_str() {
            "zombiemode" => Some(flag(self.zombies)),
            "zombietron" => Some(flag(
                self.dvars
                    .string("mapname")
                    .is_some_and(|map| map.eq_ignore_ascii_case("zombietron")),
            )),
            // One local player per process, and the runtime never pauses.
            "splitscreen" | "cl_paused" => Some(flag(false)),
            "ui_ammo_stock_width" => Some(self.ammo_widths[0].to_string()),
            "ui_right_ammo_width" => Some(self.ammo_widths[1].to_string()),
            _ => None,
        }
    }
}

/// A dvar nothing in the session registered reads as unset (`0`, `""`), as
/// the menu engine reads a missing dvar; each name is logged once so the
/// ledger can list them against Black Ops' defaults.
fn unset_dvar(name: &str) -> String {
    static SEEN: std::sync::Mutex<std::collections::BTreeSet<String>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut seen = SEEN.lock().unwrap_or_else(|poison| poison.into_inner());
    if seen.insert(name.to_ascii_lowercase()) {
        diag::info!(
            Ui,
            "t5 hud: dvar `{name}` is not registered; reads as unset"
        );
    }
    String::new()
}

struct T5HudHost<'a, 'b> {
    state: &'a T5HudState<'b>,
}

impl ExprHost for T5HudHost<'_, '_> {
    fn binding_label(&self, command: &str) -> Option<&str> {
        self.state
            .input?
            .binding_keys
            .get(command)
            .map(String::as_str)
    }
    fn milliseconds(&self) -> i32 {
        self.state.ms
    }
    fn static_dvar_int(&self, _index: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("static dvar"))
    }
    fn team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("team field"))
    }
    fn player_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("player field"))
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    fn local_var_string(&self, _name: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("local var"))
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("timeleft"))
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("score"))
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("gametype"))
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        Ok(hud_iw4::WeaponLockView {
            ads_javelin: self.state.vis.ads_javelin,
            ..hud_iw4::WeaponLockView::default()
        })
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        let value = self.state.dvar(name).unwrap_or_else(|| unset_dvar(name));
        let value = value.trim();
        Ok(value
            .parse::<i32>()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|v| v as i32))
            .unwrap_or(0))
    }
    fn dvar_string(&self, name: &str) -> Result<String, ExprError> {
        Ok(self.state.dvar(name).unwrap_or_else(|| unset_dvar(name)))
    }
    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.vis.ui_active))
    }
    fn flashbanged(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.vis.flashbanged))
    }
    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.in_killcam))
    }
    fn key_binding(&self, command: &str) -> Result<Operand, ExprError> {
        let slot = command
            .strip_prefix("+actionslot ")
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| i.checked_sub(1))
            .ok_or(ExprError::Host("keybinding"))?;
        let key = self
            .state
            .input
            .and_then(|input| input.action_slot_keys.get(slot))
            .and_then(|key| key.as_deref())
            .unwrap_or("");
        Ok(Operand::Str(key.to_owned()))
    }
    fn is_dual_wield(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.state.ammo.as_ref().is_some_and(|a| a.dual)))
    }
    fn is_fuel_weapon(&self) -> Result<i32, ExprError> {
        let weapon = weapon_iw4::get_viewmodel_weapon_index(self.state.ps);
        Ok(i32::from(
            self.state
                .weapons
                .and_then(|w| w.row(weapon))
                .and_then(|w| w.hud_facts())
                .is_some_and(|facts| facts.fuel_tank),
        ))
    }
}

/// The left-to-right widths `weaponinfo_zombie` positions the clip with, in
/// the fonts of its `ammoStock` and `clip` items.
pub(crate) fn ammo_widths(
    catalog: &MenuCatalog,
    stock: Option<i32>,
    clip: Option<i32>,
) -> [i32; 2] {
    let width = |item_name: &str, value: Option<i32>| {
        let Some(value) = value else {
            return 0;
        };
        catalog
            .get("weaponinfo_zombie")
            .and_then(|menu| menu.items.iter().find(|item| item.name == item_name))
            .and_then(|item| {
                let assets = crate::chrome::ChromeAssets {
                    catalog: Some(catalog),
                    localize: None,
                };
                let font = assets.font_name(item.font_enum, 1.0, item.text_scale)?;
                Some(crate::chrome::layout_text_width(
                    assets.layout()?,
                    catalog.font(font)?,
                    &value.to_string(),
                    item.text_scale,
                ) as i32)
            })
            .unwrap_or(0)
    };
    [width("ammoStock", stock), width("clip", clip)]
}

fn background_key(raw: &str) -> Option<(asset_core::AssetNamespace, String)> {
    let key = asset_core::AssetKey::parse(raw).ok()?;
    (key.kind == asset_core::AssetKind::Material).then_some((key.namespace, key.name))
}

/// A material stretched over `(x, y, w, h)` in the owner draw's alignment;
/// negative sizes mirror it.
#[allow(clippy::too_many_arguments)]
fn push_pic_at(
    args: &OwnerDrawArgs<'_>,
    namespace: asset_core::AssetNamespace,
    material: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
    frame: &mut ChromeFrame,
) {
    if w.abs() <= f32::EPSILON || h.abs() <= f32::EPSILON {
        return;
    }
    let applied = args.surface.apply_rect(
        x,
        y,
        w.abs(),
        h.abs(),
        args.rect.horz_align as i32,
        args.rect.vert_align as i32,
    );
    let (s0, s1) = if w < 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let (t0, t1) = if h < 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    frame.list.cmds.push(Draw2dCmd {
        material_namespace: namespace,
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0,
        t0,
        s1,
        t1,
        color,
        material,
        op: Draw2dOp::StretchPic,
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
    });
}

fn push_pic(
    args: &OwnerDrawArgs<'_>,
    namespace: asset_core::AssetNamespace,
    material: String,
    frame: &mut ChromeFrame,
) {
    let rect = args.rect;
    push_pic_at(
        args, namespace, material, rect.x, rect.y, rect.w, rect.h, args.color, frame,
    );
}

fn text(args: &OwnerDrawArgs<'_>, value: &str, frame: &mut ChromeFrame) -> OwnerDrawPaint {
    match push_owner_text(args, value, args.color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

/// Resolves a latched cursor hint (`kind`, `string`, `alpha`) into what
/// `CG_DrawCursorhint` draws.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_hint(
    kind: i32,
    string: i32,
    alpha: f32,
    ps: &PlayerState,
    meta: &sim::SnapshotMeta,
    strings: Option<&assets::PreparedLocalizedStrings>,
    input: Option<&frame::HudInputView>,
    weapons: Option<&BoundWeapons<'_>>,
    namespace: asset_core::AssetNamespace,
    gaps: &mut crate::gaps::HudPresentationGaps,
) -> Option<T5Hint> {
    if kind <= 0 || kind == 6 {
        return None;
    }
    let bind = input
        .and_then(|i| i.use_key.as_deref())
        .or_else(|| strings.and_then(|s| s.0.text("KEY_USE")))
        .unwrap_or("KEY_USE");
    let mut hint_text = |idx: i32| -> Option<String> {
        if idx < 0 {
            return None;
        }
        let strings = strings?;
        match crate::use_hint::hint_string_text(meta, strings, bind, idx) {
            Ok((text, _)) => Some(text),
            Err(reason) => {
                gaps.raise(crate::gaps::GapCause::CursorHintMissing { reason });
                None
            }
        }
    };
    match kind {
        1 | 2 => Some(T5Hint {
            kind,
            text: Some(hint_text(string)?),
            weapon_name: None,
            icon: None,
            alpha,
        }),
        3..=5 | 7 => {
            let material = match kind {
                3 => "hint_usable",
                4 => "hint_health",
                5 => "hint_friendly",
                _ => "zom_icon_community_pot",
            };
            let text = if string >= 0 {
                hint_text(string)
            } else if kind == 4 {
                strings
                    .and_then(|s| s.0.text("PLATFORM_PICKUPHEALTH"))
                    .map(|t| t.replace("&&1", bind))
            } else {
                None
            };
            let scale = if kind == 7 { 0.6 } else { 1.0 };
            Some(T5Hint {
                kind,
                text,
                weapon_name: None,
                icon: Some((namespace, material.to_owned(), scale, scale, 0.0, false)),
                alpha,
            })
        }
        8..=135 => {
            let weapon = (kind - 7) as u32;
            let weapons = weapons?;
            let registry = weapons.registry();
            let (icon_ns, image, ratio) = match (
                registry.pickup_icon_of(weapon),
                registry.hud_icon_namespace_of(weapon),
            ) {
                (Some((image, ratio)), Some(ns)) => (ns, image.to_owned(), ratio),
                _ => (namespace, String::from("hint_usable"), 0),
            };
            let (scale_x, scale_y, offset_x) = if ratio != 0 {
                (2.0, if ratio != 1 { 0.5 } else { 1.0 }, -0.5)
            } else {
                (1.0, 1.0, 0.0)
            };
            let weapon_name = registry
                .display_name_key_of(weapon)
                .and_then(|key| {
                    let strings = strings?;
                    registry
                        .identity_namespace_of(weapon)
                        .map_or_else(|| strings.0.text(key), |ns| strings.0.text_in(ns, key))
                })
                .map(str::to_owned);
            let text = if string >= 0 {
                hint_text(string)
            } else {
                let primary_count = ps
                    .weapons
                    .iter()
                    .filter(|&&w| {
                        w > 0
                            && weapons
                                .row(w as u32)
                                .and_then(|w| w.hud_facts())
                                .is_some_and(|f| f.is_primary())
                    })
                    .count();
                let offhand = weapons
                    .row(weapon)
                    .and_then(|w| w.hud_facts())
                    .map_or(0, |f| f.offhand_class);
                let (key, key_bind) = if offhand == 1 {
                    (
                        "PLATFORM_THROWBACKGRENADE",
                        input
                            .and_then(|i| i.binding_keys.get("+frag").map(String::as_str))
                            .unwrap_or(bind),
                    )
                } else if offhand != 0 || primary_count < 2 {
                    ("PLATFORM_PICKUPNEWWEAPON", bind)
                } else {
                    ("PLATFORM_SWAPWEAPONS", bind)
                };
                strings
                    .and_then(|s| s.0.text(key))
                    .map(|t| t.replace("&&1", key_bind))
            };
            Some(T5Hint {
                kind,
                text,
                weapon_name,
                icon: Some((
                    icon_ns,
                    image,
                    scale_x,
                    scale_y,
                    offset_x,
                    ps.cursor_hint_dual_wield != 0,
                )),
                alpha,
            })
        }
        _ => None,
    }
}

/// Paints owner draw 72 (`CG_CURSORHINT`) by Black Ops' `CG_DrawCursorhint`
/// rules (`cg_cursorHints` 4, `cg_weaponHintsCoD1Style` 1).
fn paint_cursor_hint(
    args: &OwnerDrawArgs<'_>,
    hint: &T5Hint,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let color = [1.0, 1.0, 1.0, hint.alpha];
    if matches!(hint.kind, 1 | 2) {
        let Some(text) = hint.text.as_deref().filter(|t| !t.is_empty()) else {
            return OwnerDrawPaint::Painted;
        };
        return match crate::chrome::push_owner_text_at(
            args,
            text,
            color,
            |width, height| (width * -0.5, height * 0.5 + args.rect.y),
            frame,
        ) {
            Ok(()) => OwnerDrawPaint::Painted,
            Err(kind) => OwnerDrawPaint::Gap(kind),
        };
    }
    let Some((namespace, material, scale_x, scale_y, offset_x, dual_wield)) = &hint.icon else {
        return OwnerDrawPaint::Painted;
    };
    if hint.kind == 7
        && let Some(text) = hint.text.as_deref().filter(|t| !t.is_empty())
    {
        let split = text.find(':').map_or(0, |i| i + 1);
        let prefix = if split > 0 { &text[..split] } else { text };
        let suffix = text.get(split + 1..).unwrap_or("");
        let (font_name, font, w1, h) = match crate::chrome::measure_owner_text(args, prefix) {
            Ok(m) => m,
            Err(kind) => return OwnerDrawPaint::Gap(kind),
        };
        let w2 = match crate::chrome::measure_owner_text(args, suffix) {
            Ok((_, _, w2, _)) => w2,
            Err(kind) => return OwnerDrawPaint::Gap(kind),
        };
        let icon_w = args.rect.w * 0.6;
        let icon_h = args.rect.h * 0.6;
        let x1 = (icon_w + w1 + w2) * -0.5;
        let icon_y = args.rect.y - args.rect.h * 0.3;
        let text_y = args.rect.y + h * 0.5;
        crate::chrome::push_owner_text_run(args, font_name, font, prefix, color, x1, text_y, frame);
        crate::chrome::push_owner_text_run(
            args,
            font_name,
            font,
            &format!(" {suffix}"),
            color,
            x1 + w1 + icon_w,
            text_y,
            frame,
        );
        push_pic_at(
            args,
            *namespace,
            material.clone(),
            x1 + w1,
            icon_y,
            icon_w,
            icon_h,
            color,
            frame,
        );
        return OwnerDrawPaint::Painted;
    }
    let icon_w = args.rect.w * scale_x;
    let icon_h = args.rect.h * scale_y;
    let Some(text) = hint.text.as_deref().filter(|t| !t.is_empty()) else {
        let x = args.rect.x - (args.rect.w + args.rect.w * offset_x) * 0.5;
        push_pic_at(
            args,
            *namespace,
            material.clone(),
            x,
            args.rect.y,
            icon_w,
            icon_h,
            color,
            frame,
        );
        return OwnerDrawPaint::Painted;
    };
    let (font_name, font, w, h) = match crate::chrome::measure_owner_text(args, text) {
        Ok(m) => m,
        Err(kind) => return OwnerDrawPaint::Gap(kind),
    };
    let icon_y = args.rect.y - icon_h * 0.5;
    let text_y = args.rect.y + h * 0.5;
    if let Some(wname) = hint.weapon_name.as_deref().filter(|s| !s.is_empty()) {
        let w_name = match crate::chrome::measure_owner_text(args, wname) {
            Ok((_, _, wn, _)) => wn,
            Err(kind) => return OwnerDrawPaint::Gap(kind),
        };
        let text_x = (w + w_name) * -0.5;
        crate::chrome::push_owner_text_run(
            args, font_name, font, text, color, text_x, text_y, frame,
        );
        crate::chrome::push_owner_text_run(
            args,
            font_name,
            font,
            &format!(" {wname}"),
            color,
            text_x + w,
            text_y,
            frame,
        );
        let pic_x = icon_w * -0.5;
        let pic_y = icon_y + h * 2.0;
        if !dual_wield {
            push_pic_at(
                args,
                *namespace,
                material.clone(),
                pic_x,
                pic_y,
                icon_w,
                icon_h,
                color,
                frame,
            );
        } else {
            let half = icon_w * 0.5;
            push_pic_at(
                args,
                *namespace,
                material.clone(),
                pic_x - half,
                pic_y,
                icon_w,
                icon_h,
                color,
                frame,
            );
            push_pic_at(
                args,
                *namespace,
                material.clone(),
                pic_x + half,
                pic_y,
                -icon_w,
                icon_h,
                color,
                frame,
            );
        }
        return OwnerDrawPaint::Painted;
    }
    let text_x = (icon_w + w) * -0.5;
    crate::chrome::push_owner_text_run(args, font_name, font, text, color, text_x, text_y, frame);
    push_pic_at(
        args,
        *namespace,
        material.clone(),
        text_x + w,
        icon_y,
        icon_w,
        icon_h,
        color,
        frame,
    );
    OwnerDrawPaint::Painted
}

/// Paints owner draw 288 (`CG_COMPETITIVE_MODE_SCORES`) by Black Ops'
/// `CG_DrawCompetitiveModeScores` / `CG_DrawCompetitiveModeScoreRow` rules.
fn paint_competitive_scores(
    state: &T5HudState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(layout) = args.assets.layout() else {
        return OwnerDrawPaint::Gap(ChromeGapKind::AssetFont);
    };
    let Some(font_name) = args
        .assets
        .font_name(3, args.surface.scale_virtual_to_real()[1], 0.39)
    else {
        return OwnerDrawPaint::Gap(ChromeGapKind::AssetFont);
    };
    let Some(font) = args.assets.catalog.and_then(|c| c.font(font_name)) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::AssetFont);
    };
    let namespace = args.assets.namespace();
    const GAMERTAG_COLORS: [[f32; 3]; 4] = [
        [1.0, 1.0, 1.0],
        [0.486, 0.812, 0.933],
        [0.965, 0.792, 0.314],
        [0.514, 0.925, 0.533],
    ];
    let mut y = args.rect.y;
    for &(client, score) in state.scores.iter().rev() {
        let is_local = client == state.local_client;
        let extra = if is_local { 3.0 } else { 0.0 };
        let row_y = y - (9.0 + extra);
        let bar_h = 18.0 - if is_local { 0.0 } else { 3.0 } + 9.0;
        let bar_w = args.rect.w - if is_local { 0.0 } else { 20.0 };
        let alpha = (0.8_f32 * if is_local { 2.0 } else { 0.8 }).min(1.0);
        let bar_color = [0.424 + if is_local { 0.05 } else { 0.0 }, 0.004, 0.0, alpha];
        push_pic_at(
            args,
            namespace,
            format!("scorebar_zom_{}", (client % 4) + 1),
            args.rect.x,
            row_y,
            bar_w,
            bar_h,
            bar_color,
            frame,
        );
        let value = score.to_string();
        let mut text_scale = 0.35 + if is_local { 0.04 } else { -0.05 };
        let col_w = args.rect.w;
        while text_scale > 0.05
            && crate::chrome::layout_text_width(layout, font, &value, text_scale) > col_w
        {
            text_scale -= 0.025;
        }
        let text_h = layout.text_height(font.pixel_height, text_scale);
        let text_x = args.rect.x + 5.0;
        let text_y = row_y + (bar_h + text_h) * 0.62;
        let draw_scale = layout.text_scale(
            font.pixel_height,
            layout.text_paint_scale(text_scale, args.anim.scale),
        );
        let applied = args.surface.apply_rect(
            text_x,
            text_y,
            draw_scale,
            draw_scale,
            args.rect.horz_align as i32,
            args.rect.vert_align as i32,
        );
        let [r, g, b] = GAMERTAG_COLORS[client % 4];
        let text_color = [r, g, b, alpha];
        frame.list.cmds.push(Draw2dCmd {
            material_namespace: namespace,
            x: (applied.x + 0.5).floor(),
            y: (applied.y + 0.5).floor(),
            w: applied.w,
            h: applied.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: text_color,
            material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: draw_scale,
                text: value,
                loc_key: String::new(),
                style: if text_scale < 0.2 { 0 } else { 3 },
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
            layer: 1,
        });
        y = (row_y - bar_h + (9.0 + extra)) - (1.0 - 4.0);
    }
    OwnerDrawPaint::Painted
}

fn paint_owner(
    state: &T5HudState<'_>,
    args: OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let ammo = state.ammo.as_ref().filter(|_| !state.hide_ammo);
    match args.item.owner_draw {
        CG_PLAYER_WEAPON_BACKGROUND => {
            if let Some((namespace, material)) = background_key(&args.item.background) {
                push_pic(&args, namespace, material, frame);
            }
            OwnerDrawPaint::Painted
        }
        CG_PLAYER_WEAPON_AMMO_CLIP => match ammo.and_then(|a| a.clip) {
            Some(clip) => text(&args, &clip.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_AMMO_CLIP_DUAL_WIELD => match ammo.and_then(|a| a.clip_alt) {
            Some(clip) => text(&args, &clip.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_AMMO_STOCK => match ammo.and_then(|a| a.stock) {
            Some(stock) => text(&args, &stock.to_string(), frame),
            None => OwnerDrawPaint::Painted,
        },
        CG_PLAYER_WEAPON_NAME => match state.weapon_name.as_deref() {
            Some(name) if !state.hide_ammo => text(&args, name, frame),
            _ => OwnerDrawPaint::Painted,
        },
        CG_OFFHAND_WEAPON_ICON_FRAG | CG_OFFHAND_WEAPON_ICON_SMOKEFLASH => {
            if state.ps.pm_type >= PM_TYPE_DEAD || state.hide_ammo {
                return OwnerDrawPaint::Painted;
            }
            let Some(weapons) = state.weapons else {
                return OwnerDrawPaint::Painted;
            };
            let class = if args.item.owner_draw == CG_OFFHAND_WEAPON_ICON_FRAG {
                state.ps.offhand_primary
            } else {
                state.ps.offhand_secondary
            };
            let Some(index) = offhand_weapon_index(state.ps, weapons, class) else {
                return OwnerDrawPaint::Painted;
            };
            let registry = weapons.registry();
            match (
                registry.hud_icon_image_of(index),
                registry.hud_icon_namespace_of(index),
            ) {
                (Some(image), Some(namespace)) => {
                    push_pic(&args, namespace, image.to_owned(), frame);
                    OwnerDrawPaint::Painted
                }
                _ => OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp),
            }
        }
        slot @ CG_PLAYER_ACTIONSLOT_1..=CG_PLAYER_ACTIONSLOT_4 => {
            let slot = slot - CG_PLAYER_ACTIONSLOT_1;
            let Some(weapon) = crate::weaponbar::action_slot_weapon(state.ps, slot, state.weapons)
            else {
                return OwnerDrawPaint::Painted;
            };
            let Some(weapons) = state.weapons else {
                return OwnerDrawPaint::Painted;
            };
            let registry = weapons.registry();
            let Some((material, _)) = registry.dpad_icon_of(weapon) else {
                return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
            };
            let Some(namespace) =
                registry.component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            else {
                return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
            };
            push_pic(&args, namespace, material.to_owned(), frame);
            OwnerDrawPaint::Painted
        }
        CG_CURSORHINT => match state.hint.as_ref() {
            None => OwnerDrawPaint::Painted,
            Some(hint) => paint_cursor_hint(&args, hint, frame),
        },
        CG_DEADQUOTE => paint_deadquote(state, &args, frame),
        CG_COMPETITIVE_MODE_SCORES => paint_competitive_scores(state, &args, frame),
        slot @ CG_PLAYER_ACTIONSLOT_ARROW_1..=CG_PLAYER_ACTIONSLOT_ARROW_4 => {
            paint_action_slot_arrow(
                state,
                &args,
                (slot - CG_PLAYER_ACTIONSLOT_ARROW_1) as usize,
                frame,
            )
        }
        CG_PLAYER_WEAPON_LOW_AMMO_WARNING => paint_low_ammo_warning(state, &args, frame),
        CG_PLAYER_HEAT_VALUE => OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw),
        CG_VEHICLE_HUD_2 | CG_AMITALKING => OwnerDrawPaint::Painted,
        n => match state.owner_hints.iter().find(|(draw, _)| *draw == n) {
            Some((_, hint)) => paint_owner_hint(&args, hint, frame),
            None => OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw),
        },
    }
}

/// An action slot's arrow: the item's picture in its colour, with the alpha
/// the game's weapon info rule gives it.
fn paint_action_slot_arrow(
    state: &T5HudState<'_>,
    args: &OwnerDrawArgs<'_>,
    slot: usize,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(info) = state.weapon_info else {
        return OwnerDrawPaint::Painted;
    };
    let alpha = (info.rules.arrow)(info.view, slot, args.color[3], info.shown_at);
    if alpha == 0.0 {
        return OwnerDrawPaint::Painted;
    }
    let Some((namespace, material)) = background_key(&args.item.background) else {
        return OwnerDrawPaint::Painted;
    };
    let [r, g, b, _] = args.color;
    let rect = args.rect;
    push_pic_at(
        args,
        namespace,
        material,
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        [r, g, b, alpha],
        frame,
    );
    OwnerDrawPaint::Painted
}

/// The low ammo warning: the item's picture, when it has one, and the
/// warning's text with the reload key, both in the warning's colour.
fn paint_low_ammo_warning(
    state: &T5HudState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(info) = state.weapon_info else {
        return OwnerDrawPaint::Painted;
    };
    let Some(warning) = (info.rules.low_ammo)(info.view, info.shown_at) else {
        return OwnerDrawPaint::Painted;
    };
    if let Some((namespace, material)) = background_key(&args.item.background) {
        let rect = args.rect;
        push_pic_at(
            args,
            namespace,
            material,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            warning.color,
            frame,
        );
    }
    let localize = |key: &str| {
        args.assets
            .localize
            .and_then(|l| l.text(key))
            .unwrap_or(key)
            .to_owned()
    };
    let bind = state
        .input
        .and_then(|input| input.binding_keys.get(warning.bind))
        .cloned()
        .unwrap_or_else(|| localize("KEY_UNBOUND"));
    let text = localize(warning.key).replace("&&1", &bind);
    match push_owner_text(args, &text, warning.color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

/// Paints owner draw 97 (`CG_DEADQUOTE` on `deadscreen`): when `ui_deadquote`
/// is set and `deadquoteStartTime < cg.time`, fades in the localized quote
/// (stripping a leading `'@'`) over `hud_deathQuoteFadeTime` (1000 ms) using
/// wrapped text (`UI_DrawWrappedText`).
fn paint_deadquote(
    state: &T5HudState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(start_ms) = state.deadquote_start_ms.filter(|&start| start < state.ms) else {
        return OwnerDrawPaint::Painted;
    };
    let Some(raw) = state.dvar("ui_deadquote").filter(|s| !s.trim().is_empty()) else {
        return OwnerDrawPaint::Painted;
    };
    let fade_ms = state
        .dvar("hud_deathQuoteFadeTime")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(1000);
    let elapsed = state.ms - start_ms;
    let alpha = if elapsed <= fade_ms {
        elapsed as f32 / fade_ms.max(1) as f32
    } else {
        1.0
    };
    let key = raw.strip_prefix('@').unwrap_or(&raw);
    let quote = args
        .assets
        .localize
        .and_then(|l| l.text(key))
        .unwrap_or(key);
    let color = [args.color[0], args.color[1], args.color[2], alpha];
    let res = crate::chrome::push_owner_wrapped_text(args, quote, color, frame);
    match res {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

/// Black Ops' `Menus_Add` hash (`h = 0x1505; h = tolower(c) + h * 33`; `0`
/// for an empty name), which orders loaded menus in the UI context.
fn menu_hash(name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    name.bytes().fold(0x1505_u32, |hash, byte| {
        let lower = i32::from(byte.to_ascii_lowercase() as i8) as u32;
        lower.wrapping_add(hash.wrapping_mul(33))
    })
}

fn dvar_truthy(state: &T5HudState<'_>, name: &str) -> bool {
    state
        .dvar(name)
        .is_some_and(|v| !matches!(v.trim(), "" | "0"))
}

/// Every menu of Black Ops' HUD lists (`CG_LoadHudMenus`), kept sorted by
/// `Menus_Add`'s name hash and painted in that order (`Menus_PaintAll`).
/// Returns the draw list and the first failure per menu.
pub(crate) fn paint(
    catalog: &MenuCatalog,
    localize: Option<&LocalizeCatalog>,
    state: &T5HudState<'_>,
    surface: &crate::surface::Hud2dSurface,
    exprs: &mut crate::expr_cache::MenuExprCache,
) -> (Draw2dList, Vec<String>) {
    let host = T5HudHost { state };
    let mut list = Draw2dList::default();
    let mut errors = Vec::new();
    let mut lists: Vec<String> = T5_HUD_MENU_LISTS
        .iter()
        .map(|&name| name.to_owned())
        .collect();
    if dvar_truthy(state, "onlinegame") || dvar_truthy(state, "systemlink") {
        lists.push(String::from("ui/hud_coop.txt"));
    }
    if let Some(map) = state.dvars.string("mapname").filter(|m| !m.is_empty()) {
        lists.push(format!("ui/hud_{map}.txt"));
    }
    if state
        .dvar("ui_gametype")
        .is_some_and(|g| g.trim().eq_ignore_ascii_case("vs"))
    {
        lists.push(String::from("ui/hud_vs.txt"));
    }
    let mut sorted: Vec<(u32, &asset_game::MenuDef)> = Vec::new();
    for name in lists
        .iter()
        .filter_map(|name| catalog.list_menus.get(name.as_str()))
        .flatten()
    {
        let Some(menu) = catalog.get(name) else {
            continue;
        };
        if sorted
            .iter()
            .any(|(_, existing)| std::ptr::eq(*existing, menu))
        {
            continue;
        }
        let hash = menu_hash(&menu.name);
        let Err(idx) = sorted.binary_search_by_key(&hash, |(h, _)| *h) else {
            continue;
        };
        sorted.insert(idx, (hash, menu));
    }
    for (_, menu) in sorted {
        let mut hook =
            |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| paint_owner(state, args, frame);
        let frame = execute_chrome_menu_ex(
            menu,
            &host,
            surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize,
            },
            ChromeMenuAnim::IDENTITY,
            exprs,
            MenuVisOnError::HideAll,
            Some(&mut hook),
        );
        if let Some((item, error)) = frame.vis_errors.first() {
            errors.push(format!("{} item {item}: {error}", menu.name));
        } else if let Some((item, kind)) = frame.coverage.gap_ids.first() {
            errors.push(format!("{} item {item}: {kind:?}", menu.name));
        }
        list.cmds.extend(frame.list.cmds);
    }
    if let Some(start_ms) = state.deadquote_start_ms
        && state.ms - start_ms >= 4000
    {
        let fade_out_sec = state
            .dvar("cg_deathScreenFadeOutTime")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.5);
        let fade_ms = (fade_out_sec * 1000.0) as i32;
        let alpha = ((state.ms - start_ms - 4000) as f32 / fade_ms.max(1) as f32).clamp(0.0, 1.0);
        let applied = surface.apply_rect(0.0, 0.0, 640.0, 480.0, 4, 4);
        list.cmds.push(Draw2dCmd {
            material_namespace: catalog
                .namespace
                .unwrap_or(crate::images::HUD_CHROME_NAMESPACE),
            x: applied.x,
            y: applied.y,
            w: applied.w,
            h: applied.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: [0.0, 0.0, 0.0, alpha],
            material: String::from("black"),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::OwnerDraw(CG_DEADQUOTE),
            layer: 1,
        });
    }
    (list, errors)
}
