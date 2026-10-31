use bevy_ecs::prelude::Resource;

use crate::ClientId;

/// The local profile's stats, for a game that keeps a stats buffer in the
/// player's profile (Black Ops multiplayer): the match hands them to the
/// local client when it connects, and rebuilds them when the profile has
/// none or keeps an older stats version.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileStats {
    /// The client the profile plays as on this machine.
    pub client: Option<ClientId>,
    pub bytes: Option<Vec<u8>>,
    /// Counts the match's rewrites of `bytes`, for the profile's save.
    pub revision: u64,
}
