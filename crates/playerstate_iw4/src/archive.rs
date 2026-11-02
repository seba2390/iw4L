use crate::PlayerState;

impl PlayerState {
    pub const fn is_live_frame(&self) -> bool {
        self.delta_time == 0
    }

    /// Whether this is another player's live state, shown to a spectator
    /// following it (the seat leaves no killcam entity, which no player's
    /// own state does).
    pub const fn is_live_follow(&self) -> bool {
        self.kill_cam_entity == crate::ENTITYNUM_NONE
    }
}
