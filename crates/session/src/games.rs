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
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
    }
}

/// The natives the scripts of the game the map belongs to bind.
pub(crate) fn natives(family: FamilyId) -> sim::script::NativeRegistry {
    let services = sim::script::NativeRegistry::engine_services();
    match family {
        FamilyId::Iw4 => services.with_mw2_systems(),
        FamilyId::T5 => services.with_black_ops(),
        FamilyId::Iw5 | FamilyId::T6 => services,
    }
}

/// How the screen of the game the map belongs to reacts to its match.
pub(crate) fn vision(family: FamilyId) -> &'static dyn game_api::GameVision {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
    }
}
