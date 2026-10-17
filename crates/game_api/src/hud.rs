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
    pub damage_icons: DamageIcons,
    pub crosshair: CrosshairFn,
    pub scoreboard: Option<&'static CodeScoreboard>,
    /// The game's code-drawn pieces the runtime does not draw, reported
    /// while the HUD shows.
    pub undrawn: &'static [&'static Unknown],
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
}

#[derive(Debug, Default)]
pub struct Crosshair {
    pub pictures: Vec<CrosshairPicture>,
    /// The parts of the game's rule the runtime does not run.
    pub unrun: Vec<&'static Unknown>,
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

/// The fields of a script hud elem a game's network may send with less
/// precision than the script gave them. Colours are red, green, blue and
/// alpha bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SentHudElem {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub font_scale: f32,
    pub from_font_scale: f32,
    pub width: i32,
    pub height: i32,
    pub from_width: i32,
    pub from_height: i32,
    pub color: [u8; 4],
    pub from_color: [u8; 4],
    pub glow_color: [u8; 4],
    /// Transition lengths, in milliseconds.
    pub fade_time: i32,
    pub scale_time: i32,
    pub move_time: i32,
    pub font_scale_time: i32,
}
