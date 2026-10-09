//! The one place that names games: which game's rules run a match.

use asset_core::FamilyId;

/// The scripts of the game the map belongs to.
pub(crate) fn scripts(family: FamilyId) -> &'static dyn game_api::GameScripts {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
    }
}

/// The modes of the game the map belongs to.
pub fn modes(family: FamilyId) -> &'static dyn game_api::GameModes {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
    }
}

/// The menu expression parser of each game whose menu catalogs tag their
/// programs with the game (`asset_game`'s Black Ops catalog writes `t5`).
pub fn menu_parsers() -> menu_expr::MenuParsers {
    menu_expr::MenuParsers(vec![("t5", game_t5::parse_menu_expression)])
}

/// How the menus of the game a catalog belongs to are drawn.
pub fn menus(family: FamilyId) -> &'static dyn game_api::GameMenus {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &T5_ON_IW4_MENU_LAYOUT,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
    }
}

/// Black Ops' menus drawn with Modern Warfare 2's menu engine layout.
struct T5OnIw4MenuLayout;

static T5_ON_IW4_MENU_LAYOUT: T5OnIw4MenuLayout = T5OnIw4MenuLayout;

impl game_api::GameMenus for T5OnIw4MenuLayout {
    fn font(
        &self,
        font_enum: i32,
        placement_scale: f32,
        text_scale: f32,
    ) -> game_api::Rule<&'static str> {
        game_api::GameMenus::font(&game_t5::GAME, font_enum, placement_scale, text_scale)
    }

    fn layout(&self) -> game_api::Rule<&'static dyn game_api::MenuLayout> {
        game_api::Rule::Known(&game_iw4::MenuLayout)
    }
}
