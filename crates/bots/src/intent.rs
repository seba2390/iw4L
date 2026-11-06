#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOutcome {
    None,
    Clear,
    Blocked,
    Unreachable,
    BudgetExhausted,
    IncompleteGraph,
    ProgressLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    Hold,
    Walk,
    Drop,
    BreakGlass,
    Mantle,
    Ladder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotIntent {
    pub look_at: Option<[f32; 3]>,
    pub move_goal: Option<[f32; 3]>,
    pub desired_range: Option<f32>,
    pub move_mode: MoveMode,
    pub fire: bool,
    pub use_button: bool,
    pub reload: bool,
    pub crouch: bool,
    pub sprint: bool,
    /// The weapon the command asks for. `None` keeps the observed weapon; a
    /// selection is held through every phase of the change, not pulsed once.
    pub weapon: Option<u16>,
    pub path: PathOutcome,
    /// How the view turns toward `look_at`, when the game's own bots say
    /// (Black Ops' `sv_botYawSpeed`, `sv_botPitchSpeed`); the host motor's
    /// slew otherwise.
    pub turn: Option<TurnRates>,
    /// Aim down the sights.
    pub ads: bool,
    /// Black Ops' strafe: the command's forward and right moves, against the
    /// view, in place of walking to `move_goal`.
    pub strafe: Option<(i8, i8)>,
    pub melee: bool,
    /// Hold an offhand's button: 0 the lethal slot, 1 the tactical.
    pub offhand: Option<usize>,
    /// The view pitch to turn to in place of `look_at`'s (Black Ops' throw).
    pub pitch: Option<f32>,
}

/// Black Ops' bot turn: each 50 ms the view covers the angle left times the
/// speed times 0.05, and lands on the target when that would pass it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnRates {
    pub yaw_speed: f32,
    pub pitch_speed: f32,
}

impl Default for BotIntent {
    fn default() -> Self {
        Self {
            look_at: None,
            move_goal: None,
            desired_range: None,
            move_mode: MoveMode::Hold,
            fire: false,
            use_button: false,
            reload: false,
            crouch: false,
            sprint: false,
            weapon: None,
            path: PathOutcome::None,
            turn: None,
            ads: false,
            strafe: None,
            melee: false,
            offhand: None,
            pitch: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotorReport {
    #[default]
    Executing,
    Arrived,
    Blocked,
    Unsupported,
}
