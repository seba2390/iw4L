use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitLevelCalled;

/// The match's level loads again from the start.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelReloadCalled;

/// The match's scripts change to another level of the same game
/// (`ChangeLevel`): its map name, and the gametype it loads under when the
/// change also switches the game's mode (`None`: the match's own).
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct LevelChangeCalled(pub String, pub Option<String>);

pub fn register_script_notify(app: &mut App) {
    app.add_message::<ExitLevelCalled>();
    app.add_message::<LevelReloadCalled>();
    app.add_message::<LevelChangeCalled>();
}
