use gamemode_iw4::Team;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveMatch {
    pub scores: [i32; 3],
    pub compass: Vec<CompassObjective>,
    pub vehicles: Vec<CompassVehicle>,
    pub vehicle_targets: Vec<VehicleHudTarget>,
    pub server_info: Vec<(String, String)>,
    pub game_end_time: i32,
    pub slow_motion: Option<crate::ScriptSlowMotion>,
    pub ambient: Option<crate::ScriptAmbient>,
    pub ac130_ambient: Option<crate::ScriptAmbient>,
    pub rumble_aliases: Vec<(i32, String)>,
    pub scripted_effects: bool,
    pub effects: Vec<ScriptEffect>,
    pub fog: Option<crate::ScriptFog>,
    pub earthquakes: Vec<crate::ScriptEarthquake>,
    pub naked_vision: Option<crate::VisionChange>,
    pub thermal_vision: Option<crate::VisionChange>,
    pub thermal_body_material: String,
    pub missile_vision: Option<crate::VisionChange>,
    pub night_vision: Option<crate::VisionChange>,
    pub pain_vision: Option<crate::VisionChange>,
    pub last_stand_vision: Option<crate::VisionChange>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptEffect {
    pub id: u32,
    pub effect: u8,
    pub origin: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub start_ms: Option<i32>,
    pub repeat_ms: i32,
    pub cull_distance: f32,
    pub viewers: Option<u64>,
    /// How a client script ended it, while the effect system still holds it.
    pub end: EffectEnd,
    /// The entity it is bolted to, when it plays on one.
    pub bolt: Option<EffectBolt>,
}

/// What an effect is bolted to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectBolt {
    /// An entity: its number and the bone of its model it follows (`None`:
    /// the entity itself).
    Entity { number: u16, bone: Option<u16> },
    /// An entity, at the bone the tag names among the bones of its drawn
    /// models (attached ones too), else the entity itself.
    EntityTag { number: u16, tag: String },
    /// The local client's viewmodel, at the bone a tag names (the viewmodel
    /// itself when it has no such tag).
    Viewmodel { tag: String },
}

/// How a client script ended an effect it plays (Black Ops 2's `stopfx` and
/// `deletefx`): the effect stays in the effect system until it is freed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectEnd {
    #[default]
    Playing,
    /// It spawns nothing more; its elements live out their lives.
    Stopped,
    /// Its elements are killed with it.
    Killed,
}

impl ScriptEffect {
    pub fn plays_for(&self, client: u32) -> bool {
        self.viewers
            .is_none_or(|mask| client < 64 && mask & (1 << client) != 0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ObjectiveState {
    #[default]
    Empty = 0,
    Active = 1,
    Invisible = 2,
    Done = 3,
    Current = 4,
    Failed = 5,
}

impl ObjectiveState {
    pub fn from_script(name: &str) -> Option<Self> {
        Some(match name {
            "empty" => Self::Empty,
            "active" => Self::Active,
            "invisible" => Self::Invisible,
            "done" => Self::Done,
            "current" => Self::Current,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub fn from_u8(raw: u8) -> Option<Self> {
        Some(match raw {
            0 => Self::Empty,
            1 => Self::Active,
            2 => Self::Invisible,
            3 => Self::Done,
            4 => Self::Current,
            5 => Self::Failed,
            _ => return None,
        })
    }

    pub fn drawn(self) -> bool {
        matches!(self, Self::Active | Self::Current)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompassObjective {
    pub index: u8,
    pub state: ObjectiveState,
    pub origin: [f32; 3],
    pub team: Team,
    pub icon: String,
    pub viewer: Option<u32>,
}

impl ObjectiveMatch {
    pub fn server_info(&self, name: &str) -> Option<&str> {
        self.server_info
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn time_left_ms(&self, now_ms: i32) -> i32 {
        self.game_end_time.saturating_sub(now_ms)
    }
}

impl CompassObjective {
    /// `shows_to` by the client state's team number.
    pub fn shows_to_team_number(&self, team: i32, client: u32) -> bool {
        let team = u8::try_from(team)
            .ok()
            .and_then(Team::from_packed_u8)
            .unwrap_or(Team::Free);
        self.shows_to(team, client)
    }

    pub fn shows_to(&self, team: Team, client: u32) -> bool {
        self.state.drawn()
            && (self.team == Team::Free || self.team == team)
            && self.viewer.is_none_or(|viewer| viewer == client)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompassVehicle {
    pub origin: [f32; 3],
    pub yaw: f32,
    pub owner: u32,
    pub team: i32,
    pub icons: [String; 2],
    pub size: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleHudTarget {
    pub slot: u8,
    pub entity: u16,
    pub model: crate::ScriptModelId,
    pub owner: crate::ClientId,
}
