use std::sync::Arc;

pub type Module = (String, Arc<[u8]>);

#[derive(Clone, Debug, Default)]
pub struct Font {
    pub name: String,
    pub height: f32,
    pub advances: Vec<(u16, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Table {
    pub name: String,
    pub columns: usize,
    pub cells: Vec<String>,
}

/// An objective slot of the local client's snapshot (`objective_add`), as a
/// game's UI reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Objective {
    pub index: u32,
    /// The slot's state; 0 is an empty slot.
    pub state: i32,
    pub name: String,
    pub origin: [f32; 3],
    /// The entity the objective follows, if any.
    pub entity: Option<u32>,
    pub team: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    pub id: u32,
    pub name: String,
    pub team: i32,
    pub score: i32,
    pub kills: i32,
    pub deaths: i32,
    pub rank: i32,
    pub prestige: i32,
    pub zombie_stats: [i32; 3],
    pub ping: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reward {
    pub cost: i32,
    pub material: String,
    pub label: String,
    pub quantity: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientFieldEvent {
    pub name: String,
    pub old: f32,
    pub new: f32,
    pub initial: bool,
    pub new_entity: bool,
    pub entity: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LuiNotify {
    pub name: String,
    pub data: Vec<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OffhandSlot {
    pub ammo: i32,
    pub material: String,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub viewport: [f32; 2],
    pub safe_area: [f32; 2],
    pub time: i32,
    pub remaining: Option<i32>,
    pub zombies: bool,
    pub team_game: bool,
    pub scoreboard: bool,
    pub menu: bool,
    pub local_client: u32,
    pub map: String,
    pub gametype: String,
    /// The zombies start location the match runs, empty for the map's
    /// default (`ui_zm_mapstartlocation`).
    pub location: String,
    pub localized: Arc<[(String, String)]>,
    pub tables: Arc<[Table]>,
    pub fonts: Arc<[Font]>,
    pub players: Vec<Player>,
    /// The local client's objective slots that are in use.
    pub objectives: Vec<Objective>,
    pub rewards: Vec<Reward>,
    pub rounds_played: i32,
    pub weapon_name: String,
    pub weapon_display_name: String,
    pub weapon_inventory_type: i32,
    pub ammo_clip: i32,
    pub ammo_stock: i32,
    pub low_clip: bool,
    pub offhand: [OffhandSlot; 2],
    pub client_fields: Vec<ClientFieldEvent>,
    pub lui_notifies: Vec<LuiNotify>,
    /// Input and engine events for the scene's menus this frame, in order.
    pub events: Vec<UiEvent>,
    /// The data definitions the zones carry (a player's stats layout among
    /// them).
    pub layouts: Arc<[asset_core::BitLayout]>,
    /// The maps whose zones are installed, by zone name.
    pub installed_maps: Arc<[String]>,
    pub unavailable_maps: Arc<[(String, String)]>,
}

/// An event the engine sends the scene's roots.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    /// A key or pad button the engine names for menus (`gamepad_button`):
    /// `primary`, `secondary`, `up`, `down`, ...; `qualifier` is `dpad` or
    /// `mwheel` when the engine sends one.
    Button {
        button: &'static str,
        qualifier: Option<&'static str>,
        down: bool,
    },
    /// The mouse moved to `x`, `y` in the root's units.
    MouseMove { x: f32, y: f32 },
    /// A mouse button (`left`, `right`) went down or up at `x`, `y`.
    Mouse {
        button: &'static str,
        down: bool,
        x: f32,
        y: f32,
    },
    /// The in-game menu key opened the menu the server named.
    InGameMenu { menu: String },
}

/// What the scene's scripts ask the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum UiRequest {
    /// Console text the scripts executed (`Engine.Exec`).
    Exec(String),
    /// The menu music the scripts started (`Engine.PlayMenuMusic`), or
    /// `None` to stop it.
    MenuMusic(Option<String>),
    /// A sound the scripts played for the local client (`Engine.PlaySound`).
    Sound(String),
    /// The gamepad's motor speeds the scripts set (`Engine.PlayRumble`).
    Rumble { left: f32, right: f32 },
    /// The zombies gametype and start location the menus picked
    /// (`ui_gametype`, `ui_zm_mapstartlocation`), for the map the next
    /// `Exec` starts.
    ZombiesStart { gametype: String, location: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Image(String),
    Globe {
        material: String,
        rotation: [f32; 4],
    },
    GameTimer,
    PlayerEmblem(u32),
    Voice(u32),
    Text {
        value: String,
        font: String,
    },
    Compass {
        layer: u8,
        full: bool,
        material: String,
        range: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub id: u32,
    pub rectangle: [f32; 4],
    pub color: [f32; 4],
    pub rotation: f32,
    pub alignment: u8,
    pub clip: Option<[f32; 4]>,
    pub content: Content,
    /// Text draws with the renderer's drop shadow.
    pub text_shadow: bool,
}

pub trait ScriptUi: Send + Sync {
    fn update(&mut self, input: &Input) -> Result<Vec<Draw>, String>;

    /// What the scripts asked of the engine since the last call.
    fn take_requests(&mut self) -> Vec<UiRequest> {
        Vec::new()
    }

    /// Whether a menu that takes the keys and mouse is open.
    fn menu_open(&self) -> bool {
        false
    }
}
