/// The names a game's client sound commands look up in its sound driver
/// and banks, each by its sound name hash: every sound context type with
/// the values it allows, the futz patches, the side-chain ducks and the
/// banks' mixer snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundMixNames {
    /// Each context type and its values (0 is no value), in driver order.
    pub contexts: Vec<(u32, [u32; 8])>,
    pub futzes: Vec<u32>,
    pub ducks: Vec<u32>,
    pub snapshots: Vec<u32>,
}
