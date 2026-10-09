//! The one place that names games: which game's rules run a match.

/// The scripts a match runs. The zombies mode runs Black Ops' own; every other
/// match runs Modern Warfare 2's gametype scripts, whatever the map's game.
pub(crate) fn scripts(zombies: bool) -> &'static dyn game_api::GameScripts {
    if zombies {
        &game_t5::GAME
    } else {
        &game_iw4::GAME
    }
}
