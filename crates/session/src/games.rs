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
        FamilyId::T6 => &game_t6::GAME,
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

/// Where games whose zones `asset_transport` opens keep the compiled scripts
/// their maps run as they are.
pub fn script_zone_formats() -> Vec<asset_core::ScriptZoneFormat> {
    vec![game_t6::SCRIPT_ZONE_FORMAT]
}

/// The menu expression parser of each game whose menu catalogs tag their
/// programs with the game (`asset_game`'s Black Ops catalog writes `t5`,
/// its Black Ops 2 catalog `t6`).
pub fn menu_parsers() -> menu_expr::MenuParsers {
    menu_expr::MenuParsers(vec![
        ("t5", game_t5::parse_menu_expression),
        ("t6", game_t6::parse_menu_expression),
    ])
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
pub(crate) fn natives(family: FamilyId, mode: &game_api::ModeRules) -> sim::script::NativeRegistry {
    let services = sim::script::NativeRegistry::engine_services;
    match family {
        FamilyId::Iw4 => services().with_mw2_systems(),
        FamilyId::T5 if mode.hud.zombies => services().with_black_ops(),
        FamilyId::T5 => services().with_black_ops().with_black_ops_multiplayer(),
        FamilyId::T6 if !mode.host_rules => sim::script::NativeRegistry::black_ops_2(),
        FamilyId::Iw5 | FamilyId::T6 => services(),
        FamilyId::T7 => sim::script::NativeRegistry::black_ops_3(),
    }
}

/// The natives of the game's client VM, when its client runs scripts of its
/// own.
pub(crate) fn client_natives(
    family: FamilyId,
    server: &sim::script::NativeRegistry,
) -> Option<sim::script::NativeRegistry> {
    match family {
        FamilyId::T5 => Some(sim::script::NativeRegistry::black_ops_client(server)),
        FamilyId::T6 => Some(sim::script::NativeRegistry::black_ops_2_client()),
        FamilyId::Iw4 | FamilyId::Iw5 | FamilyId::T7 => None,
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
