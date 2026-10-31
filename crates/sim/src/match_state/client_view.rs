#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptSeat {
    pub spectator_client: i32,
    pub kill_cam_entity: i32,
    pub look_at_entity: i32,
    pub archive_ms: i32,
    pub ps_offset_ms: i32,
    pub length_ms: i32,
    pub final_kill: bool,
}

impl Default for ScriptSeat {
    fn default() -> Self {
        Self {
            spectator_client: -1,
            kill_cam_entity: -1,
            look_at_entity: -1,
            archive_ms: 0,
            ps_offset_ms: 0,
            length_ms: 0,
            final_kill: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamHud {
    pub final_kill: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocationSelection {
    pub material: String,
    pub choose_direction: bool,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuCommand {
    pub serial: u32,
    pub kind: MenuCommandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuCommandKind {
    Open(String),
    ClosePopup,
    CloseInGame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisionChange {
    pub name: String,
    pub duration_ms: i32,
    pub set_ms: i32,
    /// Set by Black Ops 2's `visionsetnakedlerp`: the vision shown blends
    /// from `name` toward another by a fraction, at once.
    pub lerp: Option<VisionLerp>,
}

/// The other side of a vision blend: `fraction` 0 shows the change's `name`,
/// 1 shows `from`.
#[derive(Clone, Debug, PartialEq)]
pub struct VisionLerp {
    pub from: String,
    pub fraction: f32,
}

/// A client script's override of one of the map's primary lights (`makelight`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientLightOverride {
    pub index: u16,
    pub color: [u8; 4],
    pub intensity: f32,
    pub radius: f32,
    pub cos_half_fov_outer: f32,
    pub cos_half_fov_inner: f32,
    /// Where it points and stands, when its entity moves it.
    pub direction: Option<[f32; 3]>,
    pub origin: Option<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnedDynEntLaunch {
    pub model: String,
    pub pos: [f32; 3],
    pub angles: [f32; 3],
    pub hit_pos: [f32; 3],
    pub force: [f32; 3],
    pub fx: Option<String>,
}

pub const SPAWNED_DYN_ENT_TAIL: usize = 32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewEffects {
    pub naked_vision: Option<VisionChange>,
    pub thermal_vision: Option<VisionChange>,
    pub missile_vision: Option<VisionChange>,
    pub night_vision: Option<crate::VisionChange>,
    pub pain_vision: Option<crate::VisionChange>,
    pub last_stand_vision: Option<crate::VisionChange>,
    pub depth_of_field: ScriptDepthOfField,
    pub blur: Option<ScriptBlur>,
    /// The client scripts' latest vision calls, by serial.
    pub client_visions: Vec<(u32, crate::VisionCall)>,
    /// What Black Ops 2's client scripts set on the screen.
    pub client_screen: ClientScreen,
    /// Fog set for this client alone (by its player method or client script).
    pub fog: Option<crate::ScriptFog>,
    /// Water fog set for this client (`setWaterFog`).
    pub water_fog: Option<crate::ScriptFogParams>,
    /// Primary light overrides from the client's light entities (`makelight`).
    pub client_lights: Vec<ClientLightOverride>,
    /// Dynamic entities spawned by script (`CreateDynEntAndLaunch`), by serial.
    pub spawned_dyn_ents: Vec<(u32, SpawnedDynEntLaunch)>,
    /// Incremented when `CleanupSpawnedDynEnts` clears spawned dynamic entities.
    pub dyn_ent_cleanup_serial: u32,
}

/// The screen state Black Ops 2's client scripts set through its client
/// game, as the local client's draw reads it each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientScreen {
    /// The last `sethidegumpalpha`.
    pub hide_fade: Option<HideFade>,
    /// The last `zacklodscalerigid`.
    pub lod_scale_rigid: Option<LodScaleRigid>,
    /// The last switch of the client's script fog.
    pub fog: Option<game_api::fog::ScriptFogSwitch>,
    /// The active world fog bank mask, once the client's scripts or server
    /// set one.
    pub fog_bank: Option<u16>,
    /// The gump each of the local client's gump slots holds (`loadgump`):
    /// a model the map's zones leave to gumps draws while a slot holds one
    /// of the gumps that carry it.
    pub gumps: [Option<String>; GUMP_SLOTS],
    /// The burning and electric shock overlays and the flame render dvars.
    pub overlays: game_api::post::ScreenOverlays,
    /// The script vector rows the client's scripts map on its entities.
    pub script_vectors: Vec<EntityScriptVectors>,
    /// The entities the client does not draw this frame.
    pub undrawn_entities: Vec<u16>,
    /// Black Ops' extra camera while a client script has one on: its
    /// entity's place and the view's field of view.
    pub extra_camera: Option<game_api::ExtraCamera>,
    /// The 3D UI windows' places in their texture, once a client script
    /// sets one (`ui3dsetwindow`); every window is empty until then.
    pub ui3d_windows: Option<game_api::Ui3dWindows>,
    /// Black Ops' client scripts asked for the alternate aim parameters
    /// (`useAlternateAimParams`, Deadshot): the scope sways no more, and
    /// the hold-breath hints do not show.
    pub alternate_aim: bool,
    /// Black Ops multiplayer's client scrambler state: how much the
    /// nearest enemy Jammer jams, whether an enemy counter-spy plane is up,
    /// and where the local player's own Jammers are.
    pub enemy_scrambler: f32,
    pub global_scrambler: bool,
    pub friendly_scramblers: Vec<[f32; 2]>,
}

/// The script vector rows an entity's draws take (`mapshaderconstant`,
/// `setshaderconstant`): each of `scriptVector0` to `scriptVector3` the
/// entity maps, with the vector its last slot for that row holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityScriptVectors {
    pub number: u16,
    pub rows: [Option<[f32; 4]>; 4],
}

/// The gump slots Black Ops 2's client keeps.
pub const GUMP_SLOTS: usize = 4;

/// A `zacklodscalerigid` call: `r_lodScaleRigid`'s domain floor moves to
/// `min` and its value is set to `value` (a value over the domain's top is
/// refused).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodScaleRigid {
    pub set_ms: i32,
    pub value: f32,
    pub min: f32,
}

/// A `sethidegumpalpha` call: the screen hides behind a full-screen fade
/// from `set_ms`, in `color` (the last colour a call gave, black at first).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HideFade {
    pub set_ms: i32,
    pub color: [f32; 3],
}

/// How many client-script vision calls a snapshot carries.
pub const CLIENT_VISION_TAIL: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptDepthOfField {
    pub near_start: f32,
    pub near_end: f32,
    pub far_start: f32,
    pub far_end: f32,
    pub near_blur: f32,
    pub far_blur: f32,
}

impl ScriptDepthOfField {
    pub fn overrides_scene(&self) -> bool {
        self.near_start != 0.0
            || self.near_end != 0.0
            || self.far_start != 0.0
            || self.far_end != 0.0
    }
}

pub const MENU_COMMAND_TAIL: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadarMode {
    #[default]
    Off,
    Normal,
    Fast,
    Constant,
}

impl RadarMode {
    pub fn wire_tag(self) -> u8 {
        self as u8
    }

    pub fn from_wire_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Off,
            1 => Self::Normal,
            2 => Self::Fast,
            3 => Self::Constant,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkedWeaponView {
    pub entity_num: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RemoteMissile {
    pub projectile: crate::ProjectileId,
    pub entnum: i32,
    pub angles: [f32; 3],
    pub armed: bool,
    pub boosted: bool,
    pub attack: bool,
    pub unlink_at_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptBlur {
    pub from: f32,
    pub to: f32,
    pub set_ms: i32,
    pub duration_ms: i32,
}

impl ScriptBlur {
    pub fn sample(self, now_ms: i32) -> f32 {
        let fraction = if self.duration_ms <= 0 {
            1.0
        } else {
            (now_ms.wrapping_sub(self.set_ms) as f32 / self.duration_ms as f32).clamp(0.0, 1.0)
        };
        self.from + (self.to - self.from) * fraction
    }
}

/// Rendering dvars authored by GSC are delivered to clients without console gates.
pub fn is_postfx_dvar(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("r_film")
        || name.starts_with("r_glow")
        || name.starts_with("r_dof")
        || matches!(
            name.as_str(),
            "r_blur"
                | "r_hue"
                | "r_brightness"
                | "r_contrast"
                | "r_saturation"
                | "r_gamma"
                | "r_exposure"
                | "nightvision"
                | "cg_drawshellshock"
                | "thermalblurfactorscope"
                | "thermalblurfactornoscope"
        )
}
