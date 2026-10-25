//! The code-drawn HUD: the pieces a game's executable draws itself, not its
//! menus or its scripts' hud elems.

use crate::Unknown;
use crate::movement::MovePlayer;

/// Which code-drawn HUD a mode shows.
#[derive(Clone, Copy, Debug)]
pub enum CodeHud {
    /// Modern Warfare 2's pieces (reticle, blood, killfeed, names, hints,
    /// splash, playercard, compass, score bar, scoreboard).
    ModernWarfare2,
    /// The game's own pieces; none of Modern Warfare 2's draw.
    Game(&'static GameCodeHud),
}

/// A game's own code-drawn HUD.
#[derive(Debug)]
pub struct GameCodeHud {
    /// Modern Warfare 2's blood overlay is the game's too.
    pub blood: bool,
    /// Each hit flashes the screen red, by how hard it kicks the view.
    pub damage_flash: Option<DamageFlash>,
    pub damage_icons: DamageIcons,
    pub crosshair: CrosshairFn,
    pub scoreboard: Option<&'static CodeScoreboard>,
    /// What the game's hint and message owner draws show.
    pub owner_hint: OwnerHintFn,
    /// The game's weapon info: its action slot arrows and low ammo
    /// warning.
    pub weapon_info: Option<&'static WeaponInfoRules>,
    /// The invalid command hint the client keeps, given what it kept, the
    /// hint an event raised this frame (by the game's number) and the
    /// client time.
    pub command_hint: Option<CommandHintFn>,
    /// The game's grenade indicators.
    pub grenade_indicators: Option<GrenadeIndicatorFn>,
    /// The game's code-drawn pieces the runtime does not draw, reported
    /// while the HUD shows.
    pub undrawn: &'static [&'static Unknown],
}

/// The invalid command hint a client keeps: the game's number for it and
/// the client time it was raised; number 0 is none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandHint {
    pub kind: i32,
    pub time: i32,
}

pub type CommandHintFn = fn(CommandHint, Option<i32>, i32) -> CommandHint;

/// What a game's hint or message owner draw (its number) shows this frame.
pub type OwnerHintFn = fn(i32, &OwnerHintView<'_>) -> OwnerHint;

/// What a game's hint and message owner draws read.
pub struct OwnerHintView<'a> {
    /// The local player as the game's movement keeps him.
    pub player: &'a MovePlayer,
    /// The weapon's hint fields; `None` when the client does not know it.
    pub weapon: &'a dyn Fn(u32) -> Option<HintWeapon>,
    /// The scoreboard is up.
    pub scoreboard: bool,
    /// The offline game is paused.
    pub paused: bool,
    /// A client dvar's value, when the client keeps it.
    pub dvar: &'a dyn Fn(&str) -> Option<f32>,
    /// The client time and the invalid command hint the client keeps.
    pub now: i32,
    pub command_hint: CommandHint,
    /// The client's scripts asked for the alternate aim parameters.
    pub alternate_aim: bool,
}

/// A weapon definition's fields a hint reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HintWeapon {
    /// The weapon draws a sight overlay when aimed.
    pub ads_overlay: bool,
    /// The weapon class, by the game's numbers.
    pub weap_class: i32,
    /// The scope has more than one zoom level.
    pub zoom_varies: bool,
}

/// What a hint or message owner draw shows.
#[derive(Clone, Copy, Debug)]
pub enum OwnerHint {
    /// Not one of the game's hint or message owner draws.
    NotHint,
    /// Nothing this frame.
    Nothing,
    /// The localized string `key`, its `&&1` the key bound to the first of
    /// `binds` that has one, centred on the item.
    Text {
        key: &'static str,
        binds: &'static [&'static str],
    },
    /// The localized string `key` (its `&&1` the key bound to the first of
    /// `binds` that has one) and after it `material` at the item's size,
    /// white, the pair centred on the item's left edge and raised by
    /// `raise`.
    TextPicture {
        key: &'static str,
        binds: &'static [&'static str],
        material: &'static str,
        raise: f32,
    },
    /// The localized string `key` centred on the item in the item's colour
    /// with this alpha.
    Faded { key: &'static str, alpha: f32 },
    /// The game draws something the runtime does not.
    Unrun(&'static Unknown),
}

/// A game's weapon info: the pieces of its HUD shown for a while after
/// the player touches his weapons, then faded out.
#[derive(Debug)]
pub struct WeaponInfoRules {
    /// The client time the weapon info was last shown, given the time it
    /// was shown before (0: not shown): the player's input and weapons this
    /// frame show it again, and it is let go once it has faded.
    pub update: fn(&WeaponInfoView<'_>, i32) -> i32,
    /// The alpha of an action slot's arrow (by slot, from 0), given the
    /// item's own alpha and the time the weapon info was shown.
    pub arrow: fn(&WeaponInfoView<'_>, usize, f32, i32) -> f32,
    /// The colour of an action slot's key label (by slot, from 0), given
    /// the item's own colour and the time the weapon info was shown; alpha
    /// 0 draws nothing.
    pub bind_label: fn(&WeaponInfoView<'_>, usize, [f32; 4], i32) -> [f32; 4],
    /// The low ammo warning, given the time the weapon info was shown.
    pub low_ammo: fn(&WeaponInfoView<'_>, i32) -> Option<LowAmmoWarning>,
    /// The invalid command hint (the game's number) pressing the action
    /// slot `slot_pressed` raises.
    pub slot_hint: fn(&WeaponInfoView<'_>) -> Option<i32>,
    /// The weapon info's alpha (its background picture) given the time it
    /// was shown.
    pub fade: fn(&WeaponInfoView<'_>, i32) -> f32,
    /// The alpha of the weapon's round counts given the time the weapon
    /// info was shown.
    pub ammo_fade: fn(&WeaponInfoView<'_>, i32) -> f32,
    /// What the clip and the rounds beside it show, given their counts.
    pub ammo_text: fn(&WeaponInfoView<'_>, i32, i32) -> WeaponInfoAmmo,
    /// The weapon name's alpha, by the time the selection last changed.
    pub name_fade: fn(&WeaponInfoView<'_>) -> f32,
    /// The gap the weapon name keeps from its item's right edge.
    pub name_right_gap: f32,
    /// The icons of an offhand class (the game's number), given the time
    /// the weapon info was shown.
    pub offhand: fn(&WeaponInfoView<'_>, i32, i32) -> Option<OffhandIcons>,
}

/// An offhand class's icons: the weapon's HUD icon `count` times, the
/// first at the item, each next a `step` of the item's width further right
/// and its colour times `falloff` once more; all at `alpha` times the
/// item's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffhandIcons {
    pub weapon: u32,
    pub count: i32,
    pub alpha: f32,
    pub step: f32,
    pub falloff: f32,
}

/// The weapon info's round counts as the game prints them.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponInfoAmmo {
    pub clip: String,
    pub stock: String,
    /// The stock's colour in place of its item's, when the game recolours it.
    pub stock_rgb: Option<[f32; 3]>,
}

/// What a game's weapon info reads of the local player.
pub struct WeaponInfoView<'a> {
    /// The local player as the game's movement keeps him.
    pub player: &'a MovePlayer,
    /// The client time.
    pub now: i32,
    /// The weapon the client has selected, and the client time it last
    /// changed its selection.
    pub selected: u32,
    pub selected_at: i32,
    /// The client time an action slot key was last pressed, and its slot
    /// (from 0).
    pub slot_pressed_at: i32,
    pub slot_pressed: usize,
    /// The weapon selected when that key was pressed.
    pub slot_pressed_from: u32,
    /// An action slot's kind and weapon, by slot.
    pub action_slot: &'a dyn Fn(usize) -> (i32, u32),
    /// A weapon's definition and variant in the game's layout.
    pub weapon: &'a dyn Fn(u32) -> Option<(&'a [u8], &'a [u8])>,
    /// A weapon's alternate weapon and its left-hand weapon; 0 when none.
    pub alternate: &'a dyn Fn(u32) -> u32,
    pub left_hand: &'a dyn Fn(u32) -> u32,
    /// The rounds in a weapon's clip and beside it, and its ammo number
    /// (weapons with one number share the rounds beside the clip).
    pub clip: &'a dyn Fn(u32) -> i32,
    pub stock: &'a dyn Fn(u32) -> i32,
    pub ammo_number: &'a dyn Fn(u32) -> i32,
    /// A client dvar's value, when the client keeps it.
    pub dvar: &'a dyn Fn(&str) -> Option<f32>,
}

/// The low ammo warning: a localized string whose `&&1` is the key bound
/// to `bind`, drawn in `color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowAmmoWarning {
    pub key: &'static str,
    pub bind: &'static str,
    pub color: [f32; 4],
}

/// The red flash a hit draws over the whole screen: the hit's view kick
/// (its damage times `kick_scale`, held to `kick_min..=kick_max`; a hit from
/// a direction kicks by how far ahead it came from, plus a random share up
/// to `kick_random` of how far to the side) fades over `time_ms`, the
/// flash's alpha the kick times the msec left times `alpha_per_ms`, at most
/// `alpha_max`, times the colour's alpha and `alpha_scale`.
#[derive(Clone, Copy, Debug)]
pub struct DamageFlash {
    pub kick_scale: f32,
    pub kick_min: f32,
    pub kick_max: f32,
    pub kick_random: f32,
    pub time_ms: i32,
    pub alpha_per_ms: f32,
    pub alpha_max: f32,
    pub color: [f32; 4],
    pub alpha_scale: f32,
    /// The flash's top left corner, in pixels on both axes, and how much it
    /// is wider and taller than the screen.
    pub corner: f32,
    pub grow: f32,
}

/// The icons pointing at where damage came from.
#[derive(Clone, Copy, Debug)]
pub struct DamageIcons {
    /// How many hits show at once; a new hit takes the oldest slot.
    pub slots: usize,
    /// How long an icon shows, in milliseconds, while the client keeps no
    /// `cg_hudDamageIconTime`.
    pub time_ms: i32,
    /// A flash or a mantle hides the icons.
    pub hidden_by_flash_and_mantle: bool,
    /// The picture of a stunning hit; `None` draws every hit with the
    /// plain one.
    pub stun_material: Option<&'static str>,
}

/// The crosshair a game draws for its player: the pictures, and what of the
/// game's rule the runtime cannot run.
pub type CrosshairFn = fn(&CrosshairView<'_>) -> Crosshair;

/// What a game's crosshair rule reads.
pub struct CrosshairView<'a> {
    /// The local player as the game's movement keeps him.
    pub player: &'a MovePlayer,
    /// The weapon the crosshair is drawn for, as its definition gives it;
    /// `None` when the weapon is not known to the client.
    pub weapon: &'a dyn Fn(u32) -> Option<CrosshairWeapon>,
    /// The view is not the player's own eyes.
    pub third_person: bool,
    /// The sights are moving up (the last change of the weapon position
    /// raised it).
    pub to_ads: bool,
    /// The tangent of half the vertical field of view.
    pub tan_half_fov_y: f32,
    /// Where the gun aims on the virtual screen, from its centre.
    pub gun_offset: [f32; 2],
    /// The offline game is paused.
    pub paused: bool,
    /// A client dvar's value, when the client keeps it.
    pub dvar: &'a dyn Fn(&str) -> Option<f32>,
}

/// A weapon definition's crosshair fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrosshairWeapon {
    pub weap_type: i32,
    pub has_center: bool,
    pub has_side: bool,
    pub center_size: i32,
    pub side_size: i32,
    pub min_ofs: i32,
    pub side_pos: f32,
    pub ads_aim_pitch: f32,
    pub ads_crosshair_in_frac: f32,
    pub ads_crosshair_out_frac: f32,
    /// Stand, ducked and prone hip spread, minimum then maximum.
    pub hip_spread_min: [f32; 3],
    pub hip_spread_max: [f32; 3],
    pub cook_off_hold: bool,
    pub keep_crosshair_when_ads: bool,
    pub dual_wield: bool,
    /// The weapon draws a sight overlay when aimed.
    pub ads_overlay: bool,
    /// How far into raising (lowering) the sights the overlay comes in
    /// (goes), its size and what its alpha is divided by.
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
    pub ads_overlay_width: f32,
    pub ads_overlay_height: f32,
    pub ads_overlay_alpha_scale: f32,
    /// The crosshair takes the colour of what it is on.
    pub color_change: bool,
    /// The weapon is Black Ops' human gun (its name starts `humangun_`).
    pub human_gun: bool,
}

#[derive(Debug, Default)]
pub struct Crosshair {
    pub pictures: Vec<CrosshairPicture>,
    /// The weapon's sight overlay, drawn before the crosshair.
    pub sight_overlay: Option<SightOverlay>,
    /// The parts of the game's rule the runtime does not run.
    pub unrun: Vec<&'static Unknown>,
}

/// A weapon's sight overlay in the virtual 640x480 screen with its origin
/// at the screen's centre: pictures of the weapon's overlay material, the
/// rectangle they cover (the screen around it is filled from the picture's
/// outer edge) and its alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct SightOverlay {
    pub pictures: Vec<OverlayPicture>,
    /// `[x, y, w, h]`.
    pub frame: [f32; 4],
    pub alpha: f32,
}

/// One picture of a sight overlay and its texture coordinates
/// (`[s0, t0, s1, t1]`, which mirror it when reversed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayPicture {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub st: [f32; 4],
}

/// One picture of the crosshair, placed in the virtual 640x480 screen with
/// its origin at the screen's centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosshairPicture {
    /// The weapon's side picture; else its centre picture.
    pub side: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Turned about its centre, in degrees.
    pub rotation: f32,
    pub color: [f32; 4],
}

/// What a game's grenade indicators draw this frame.
pub type GrenadeIndicatorFn = fn(&GrenadeIndicatorView<'_>) -> GrenadeIndicators;

/// What a game's grenade indicators read.
pub struct GrenadeIndicatorView<'a> {
    /// The local player as the game's movement keeps him.
    pub player: &'a MovePlayer,
    /// The match's missiles, as the client presents them.
    pub missiles: &'a [IndicatorMissile],
    /// A weapon's indicator fields; `None` when the client does not know
    /// it.
    pub weapon: &'a dyn Fn(u32) -> Option<IndicatorWeapon>,
    /// The view is not the player's own eyes.
    pub third_person: bool,
    /// The client's time, in milliseconds.
    pub time: i32,
    /// A client dvar's value, when the client keeps it.
    pub dvar: &'a dyn Fn(&str) -> Option<f32>,
}

/// A missile as the grenade indicators read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorMissile {
    pub weapon: u32,
    pub origin: [f32; 3],
    /// Its trajectory's velocity.
    pub delta: [f32; 3],
    /// The time it is drawn from.
    pub time: i32,
    /// Its owner is the local player, or on the local player's team.
    pub owner_local: bool,
    pub owner_teammate: bool,
}

/// A weapon definition's grenade indicator fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IndicatorWeapon {
    pub weap_type: i32,
    pub proj_explosion_type: i32,
    pub show_indicator: bool,
    pub timed_detonation: bool,
    pub explosion_radius: i32,
    pub indicator_radius: i32,
    pub stickiness: i32,
    pub fuse_time_ms: i32,
    /// The weapon names its own indicator icon.
    pub has_icon: bool,
    /// The weapon's sights draw an overlay.
    pub ads_overlay: bool,
}

/// What the grenade indicators draw.
#[derive(Debug, Default)]
pub struct GrenadeIndicators {
    pub pictures: Vec<IndicatorPicture>,
    /// The parts of the game's rule the runtime does not run.
    pub unrun: Vec<&'static Unknown>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IndicatorMaterial {
    Named(&'static str),
    /// The icon the weapon names.
    WeaponIcon(u32),
}

/// One picture of the grenade indicators, placed in the virtual 640x480
/// screen with its origin at the screen's centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorPicture {
    pub material: IndicatorMaterial,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Turned about its centre, in degrees.
    pub rotation: f32,
    pub color: [f32; 4],
}

/// A scoreboard the game's executable draws, of rows under a header of
/// column titles and a footer line, on the virtual 640x480 screen.
#[derive(Debug)]
pub struct CodeScoreboard {
    /// `cg_scoreboardWidth`, `cg_scoreboardHeight`, `cg_scoreboardItemHeight`.
    pub width: f32,
    pub height: f32,
    pub item_height: f32,
    /// The board's top is centred, then lowered by this.
    pub top_offset: f32,
    /// The header row starts this far below the top.
    pub header_offset: f32,
    /// The header's text height (the unnamed integer dvar beside the
    /// scoreboard's).
    pub header_height: f32,
    /// The columns' and the bars' inset from the board's sides.
    pub inset: f32,
    /// The space between the rows and the room kept below them.
    pub row_gap: f32,
    pub bottom_room: f32,
    /// `cg_scoreboardFont`; `cg_scoreboardHeaderFontScale`; the rows' scale.
    pub font: i32,
    pub header_scale: f32,
    pub row_scale: f32,
    /// A cell's scale steps down by this while its text is wider than its
    /// column; below `plain_below` it draws without its shadow.
    pub scale_step: f32,
    pub plain_below: f32,
    /// `cg_scoreboardTextOffset`.
    pub text_offset: f32,
    pub columns: &'static [ScoreColumn],
    /// The row bar's picture by client number.
    pub bar_materials: &'static [&'static str],
    /// The bar's colour (`cg_ScoresColor_Zombie`) and its alpha factor
    /// (`cg_ScoresColor_Transparency`).
    pub bar_color: [f32; 3],
    pub bar_alpha: f32,
    /// The rows' text colour by client number (`cg_ScoresColor_Gamertag_<n>`).
    pub name_colors: &'static [[f32; 4]],
    /// The board fades out over this long once it is let go, in milliseconds.
    pub fade_ms: i32,
    /// The footer: the host's name (the server's `sv_hostname`) at the left,
    /// in a band this high above the board's bottom, its scale stepped down
    /// from `footer_scale` by `footer_step` to no less than `footer_floor`
    /// until it fits.
    pub footer_height: f32,
    pub footer_bottom: f32,
    pub footer_scale: f32,
    pub footer_step: f32,
    pub footer_floor: f32,
    pub hostname: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ScoreColumn {
    pub value: ScoreValue,
    /// The column's share of the board's inner width.
    pub width: f32,
    /// The localized title.
    pub title: &'static str,
    pub align: ScoreAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreValue {
    Name,
    Score,
    Kills,
    Headshots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreAlign {
    Left,
    Center,
    Right,
}
