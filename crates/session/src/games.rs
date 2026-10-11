//! The one place that names games: which game's rules run a match.

use asset_core::FamilyId;

/// The scripts of the game the map belongs to.
pub(crate) fn scripts(family: FamilyId) -> &'static dyn game_api::GameScripts {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
        FamilyId::T7 => &game_t7::GAME,
    }
}

/// The modes of the game the map belongs to.
pub fn modes(family: FamilyId) -> &'static dyn game_api::GameModes {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &T6_ON_IW4_MOVEMENT,
        FamilyId::T7 => &game_t7::GAME,
    }
}

/// The game whose zones an installation of `game` holds; `None` for Modern
/// Warfare, which IW4L recognises by its own files.
pub fn installation_family(game: frame::OtherGame) -> Option<FamilyId> {
    match game {
        frame::OtherGame::ModernWarfare2 => Some(FamilyId::Iw4),
        frame::OtherGame::ModernWarfare3 => Some(FamilyId::Iw5),
        frame::OtherGame::BlackOps => Some(FamilyId::T5),
        frame::OtherGame::BlackOps2 => Some(FamilyId::T6),
        frame::OtherGame::BlackOps3 => Some(FamilyId::T7),
        frame::OtherGame::ModernWarfare => None,
    }
}

/// The zone formats of games whose zones `asset_transport` does not open itself.
pub fn zone_formats() -> Vec<asset_core::ZoneFormat> {
    vec![fastfile_t7::ZONE_FORMAT]
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
        FamilyId::T7 => &game_t7::GAME,
    }
}

/// The natives the scripts of the game the map belongs to bind.
pub(crate) fn natives(family: FamilyId) -> sim::script::NativeRegistry {
    let services = sim::script::NativeRegistry::engine_services();
    match family {
        FamilyId::Iw4 => services.with_mw2_systems(),
        FamilyId::T5 => services.with_black_ops(),
        FamilyId::Iw5 | FamilyId::T6 | FamilyId::T7 => services,
    }
}

/// How the screen of the game the map belongs to reacts to its match.
pub(crate) fn vision(family: FamilyId) -> &'static dyn game_api::GameVision {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &game_t6::GAME,
        FamilyId::T7 => &game_t7::GAME,
    }
}

/// Black Ops 2 moves and fires by Modern Warfare 2's rules: the owner kept
/// Black Ops 2 playable on them until its own exist (docs/fidelity/t6.md).
struct T6OnIw4Movement;

static T6_ON_IW4_MOVEMENT: T6OnIw4Movement = T6OnIw4Movement;

impl game_api::GameModes for T6OnIw4Movement {
    fn mode(&self, gametype: &str) -> game_api::Rule<game_api::ModeRules> {
        match game_api::GameModes::mode(&game_t6::GAME, gametype) {
            game_api::Rule::Known(mut mode) => {
                mode.movement = game_api::Rule::Known(game_api::MovementRules::Simulation);
                mode.weapons = game_api::Rule::Known(game_api::WeaponRules::Simulation);
                game_api::Rule::Known(mode)
            }
            unknown => unknown,
        }
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        game_api::GameModes::zombies(&game_t6::GAME)
    }
}
