/// The material a menu draws a game's extra camera's view with.
pub const EXTRA_CAMERA_MATERIAL: &str = "extracam2d";
/// That view as a picture: its renderer binds the camera's target to this.
pub const EXTRA_CAMERA_IMAGE: bevy::asset::Handle<bevy::image::Image> =
    bevy::asset::uuid_handle!("2d7cc77e-016c-423e-8b94-557801e36676");

pub mod scope;
pub use scope::{
    InMatch, InRound, MatchScope, RoundPhase, ScopeApp, ScopeEpoch, ScopePlugin, ScopeRegistry,
    ScopeSet, Staging, Timeline,
};
mod screen_effects;
pub use screen_effects::{
    MenuBlur, ScreenBlurRequest, ScreenEffectsDvars, ScreenEffectsPublished, ScreenEffectsView,
};
pub mod class_presets;
pub mod pad;
pub mod retire;
pub mod schedule;
pub mod script_entity_notify;
pub mod script_notify;
pub mod session;
pub mod settings;
pub mod ui;

pub use class_presets::{ClassPreset, pick_showcase, showcase_classes};
pub use pad::{ActivePad, ControllerIcon, InputDevices, TestControllerRumble};
pub use retire::Retiring;
pub use schedule::{
    AUTHORITY_TOC, AuthorityBookkeeping, AuthorityEdge, AuthoritySet, CLIENT_TOC,
    ClassEquipResolved, ClientEdge, ClientSet, FxSoundPublished, LifeFrontPublished,
    ModelLightingSeated, OwnerEventsPublished, PresentedPublished, RenderSet, SessionSwapApplied,
    WORKER_CMD_AFTER, WORKER_CMD_END_FENCE, WORKER_CMD_NOT_RENDER_THREAD, WORKER_CMD_RETAIL_NAMES,
    WORKER_CMD_TOC, WorkerCmdSet, authority_set_name, client_set_name, configure_authority_sets,
    configure_client_sets, configure_render_sets, configure_worker_cmd_sets, worker_cmd_name,
};
pub use script_entity_notify::{AbortKillcam, BeginKillcam, KillcamEnded, SpawnedPlayer};
pub use script_notify::{
    ExitLevelCalled, LevelChangeCalled, LevelReloadCalled, register_script_notify,
};
pub use session::{
    AdmissionKey, AppScreen, BotNavigationReady, CacWeaponOffer, CinematicInput,
    ClassSelectHandoff, DisplayEncodedCamera, Headless, HostClassLoadouts, HostClassSlot, HudDraw,
    HudInputView, InstalledMatch, LaunchIdentity, LaunchReport, LifeEndCause, LifeEnded,
    LifeStartReason, LifeStarted, LocalLoadKey, LocalSpawnArmed, MapLoadApproved, MapLoadFailed,
    MatchKey, ReadinessState, RuntimeRole, TeardownReason, UiCamera, UiDraw, UiOverlayInput,
    UiPointerFree, ViewSubject, WorldGeneration, WorldProducts, WorldReadiness, WorldStamp,
};
pub use settings::{
    DisplayGamma, DisplayResolution, GameSettings, MasterPreset, MasterVolumeGain, OtherGame,
    SoundCategoryVolumes,
};
pub use ui::{
    AssetFamilies, FrontendContentLoading, HostMatchRules, MatchPause, NativeGameMenu,
    UiBindRequest, UiBindingCapture, UiExecCommand, UiMenuDvars, UiMenuKey, UiMenuRequest,
    UiPartyState, UiPlayCinematic, UiPlayMusic, UiPlaySound, UiStopCinematic, UiStopMusic,
    UnifiedFrontend, register_ui_contracts,
};
