use crate::identities::{LifeSequence, MatchPhase};
use crate::world::ClientId;
use crate::world_objects::WorldObjectSnapshot;

use super::ClientLifecycle;
use super::client_view::{
    KillcamHud, LinkedWeaponView, LocationSelection, MenuCommand, RadarMode, RemoteMissile,
    ViewEffects,
};
use super::events::{EntityEventRecord, EventRecord, PelletFxRecord, SimEvent};
use super::loadout::LoadoutSpec;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientSnapshotMeta {
    pub shield: Option<crate::ShieldAttachment>,
    pub shield_collision: Option<crate::ShieldCarrierCollision>,
    pub controls: super::ScriptControls,
    pub weapon_lock: crate::WeaponLock,
    pub killcam_hud: Option<KillcamHud>,
    pub lifecycle: ClientLifecycle,
    pub god_mode: bool,
    pub loadout: Option<LoadoutSpec>,
    pub life_sequence: LifeSequence,

    pub item_use_spawn_ms: i32,
    pub item_use_entity: Option<crate::EntityRef>,
    pub item_use_press_ms: i32,

    pub ammo_clip: i32,

    pub ammo_stock: i32,

    /// The count of invalid command hints the game's movement raised for
    /// the player, and the last one (the game's number for it).
    pub command_hint: (u8, i32),
    /// Counts the times the weapon in hand ran out of everything.
    pub weapon_emptied: u8,

    pub score: i32,
    pub kills: i32,
    pub deaths: i32,
    pub kill_streak: i32,
    /// Zombies scoreboard: downs, revives, headshots.
    pub zombie_stats: [i32; 3],
    /// Black Ops multiplayer's scoreboard: the values of the four columns
    /// `setscoreboardcolumns` named, written when the scripts write the
    /// player's field, and the scripts' `statusicon` material.
    pub score_columns: [i32; 4],
    pub status_icon: String,
    pub radar: RadarMode,
    pub radar_blocked: bool,
    /// Black Ops' player state radar levels: spy plane, satellite.
    pub radar_levels: [i32; 2],
    /// The HUD visibility bits Black Ops' scripts set: the player's UI flags
    /// (bits 0–3, `setClientUIVisibilityFlag`) and the match flags (4–16,
    /// `setMatchFlag`).
    pub ui_flags: u32,
    pub remote_missile: Option<RemoteMissile>,
    pub linked_weapon_view: Option<LinkedWeaponView>,

    pub ammo_by_weapon: Vec<(u32, i32, i32)>,

    pub taped_mag_spent: Vec<u32>,

    pub weapon_shot_count: u8,
    pub burst_latch: bool,
    pub burst_latch_secondary: bool,
    pub rechamber_pending: bool,
    pub rechamber_pending_secondary: bool,
    pub pending_brass: [Option<crate::PendingBrass>; 2],

    pub dead_since_tick: Option<u32>,

    pub look_at_killer_yaw: i32,

    pub name: [u8; 16],

    pub hud_archival: Vec<hud_iw4::HudElem>,

    pub hud_current: Vec<hud_iw4::HudElem>,

    pub ffa_team: Option<u8>,

    /// The soldier kit the game dressed the player in, when it did.
    pub kit: Option<u8>,

    pub client_state_team: i32,

    pub rank: i32,

    pub prestige: i32,

    pub player_card_icon: u32,

    pub player_card_title: u32,

    pub player_card_nameplate: u32,

    pub client_dvars: Vec<(String, String)>,

    pub shellshock: Option<hud_iw4::ShockParams>,

    pub view_effects: ViewEffects,

    pub menu_commands: Vec<MenuCommand>,

    pub lui_client_fields: Vec<(u32, game_api::ui::ClientFieldEvent)>,

    pub lui_notifies: Vec<(u32, game_api::ui::LuiNotify)>,

    pub location_selection: Option<LocationSelection>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapshotMeta {
    pub objectives: crate::ObjectiveMatch,
    pub phase: MatchPhase,
    pub round_serial: u32,
    pub rounds_played: i32,

    pub match_elapsed_ms: u32,
    pub prematch: gamemode_iw4::PrematchStep,

    pub score_limit: i32,

    pub time_limit_ms: u32,

    pub kind: gamemode_iw4::GameModeKind,
    pub clients: Vec<(ClientId, ClientSnapshotMeta)>,

    pub journal: Vec<EventRecord>,

    pub entity_events: Vec<EntityEventRecord>,

    pub pellet_fx: Vec<PelletFxRecord>,

    pub sound_aliases: crate::SoundAliasCsOccupied,

    pub effect_names: crate::EffectNameCsOccupied,

    pub hud_materials: crate::HudMaterialCsOccupied,

    pub hud_strings: crate::HudStringCsOccupied,

    pub rng: RngDebugMeta,

    pub world_objects: WorldObjectSnapshot,

    pub area_entities: Option<crate::AreaEntityWorldSnapshot>,

    /// What the scripts tell their bots to do; the host's own, not sent.
    pub bot_orders: Vec<BotOrder>,
    /// How the game's own bots react, fire and turn, when its scripts set it;
    /// the host's own, not sent.
    pub bot_tuning: Option<BotTuning>,
    /// The game's scripted turrets, as a client's draw reads them.
    pub turret_models: Vec<TurretModelRow>,

    pub entity_dobjs: Vec<(
        crate::AuthorityModelOwner,
        xmodel_runtime::DObjSemanticState,
    )>,

    pub entities: Vec<entity_iw4::EntityState>,

    pub script_movers: Vec<crate::ScriptMoverGentity>,

    pub entity_kernel: crate::EntityKernelSnapshot,

    pub corpses: crate::PlayerCorpsePool,

    pub item_ammo: Vec<DroppedItemAmmo>,

    pub item_pickups: Vec<ItemPickupRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DroppedItemAmmo {
    pub entnum: i32,
    pub clip_r: i32,
    pub clip_l: i32,
    pub stock: i32,
    pub scavenger: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemPickupRecord {
    pub picker: i32,
    pub weapon: u32,
    pub from_entnum: i32,
    pub clip_r: i32,
    pub clip_l: i32,
    pub stock: i32,
    pub swapped_entnum: i32,
    pub picker_pm_type: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RngDebugMeta {
    pub root_seed: u64,
    pub scheme: u32,
    pub spawn_draws: u64,
    pub combat_draws: u64,
    pub bot_draws: u64,
}

impl SnapshotMeta {
    pub fn for_client(&self, id: ClientId) -> Option<&ClientSnapshotMeta> {
        self.clients.iter().find(|(c, _)| *c == id).map(|(_, m)| m)
    }

    pub fn events_for(&self, id: ClientId) -> impl Iterator<Item = &SimEvent> {
        self.journal
            .iter()
            .filter(move |record| record.audience.projects_to(id))
            .map(|record| &record.event)
    }

    pub fn script_dvars(&self, id: ClientId) -> ScriptDvars<'_> {
        ScriptDvars {
            client: self
                .for_client(id)
                .map_or(&[], |m| m.client_dvars.as_slice()),
            server: &self.objectives.server_info,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetBoxDvar {
    Scale,
    MinSize,
    SpawnDelay,
    SpawnFade,
}

impl TargetBoxDvar {
    pub fn named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "foficonscale" => Some(Self::Scale),
            "foficonminsize" => Some(Self::MinSize),
            "foficonspawntimedelay" => Some(Self::SpawnDelay),
            "foficonspawntimefade" => Some(Self::SpawnFade),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Scale => "FoFIconScale",
            Self::MinSize => "FoFIconMinSize",
            Self::SpawnDelay => "FoFIconSpawnTimeDelay",
            Self::SpawnFade => "FoFIconSpawnTimeFade",
        }
    }

    pub fn default_value(self) -> f32 {
        match self {
            Self::Scale => 1.3,
            Self::MinSize => 30.0,
            Self::SpawnDelay => 2.0,
            Self::SpawnFade => 5.0,
        }
    }

    pub fn accepts(self, value: f32) -> bool {
        let minimum = if self == Self::Scale { 0.1 } else { 0.0 };
        minimum <= value && value <= f32::MAX
    }

    pub fn parse(self, value: &str) -> Option<f32> {
        let value = parse_float_dvar(value);
        self.accepts(value).then_some(value)
    }
}

fn parse_float_dvar(value: &str) -> f32 {
    let mut end = value.len().min(1023);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    crate::script::host::natives::iw4::atof(&value[..end]) as f32
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptDvars<'a> {
    client: &'a [(String, String)],
    server: &'a [(String, String)],
}

impl<'a> ScriptDvars<'a> {
    pub fn string(&self, name: &str) -> Option<&'a str> {
        self.client
            .iter()
            .chain(self.server)
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        self.string(name).map(parse_float_dvar)
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        let value = self.string(name)?.trim();
        value
            .parse::<i32>()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|v| v as i32))
    }

    pub fn text(&self, name: &str, localize: impl Fn(&str) -> Option<String>) -> Option<String> {
        let mut parts = self.string(name)?.split(crate::HUD_PRINT_ARG_SEPARATOR);
        let head = parts.next().unwrap_or("");
        let Some(key) = head.strip_prefix('@') else {
            return Some(head.to_owned());
        };
        let template = localize(key)?;
        Some(parts.enumerate().fold(template, |line, (i, arg)| {
            line.replace(&format!("&&{}", i + 1), arg)
        }))
    }
}

/// A bot's orders from the scripts (Black Ops' `SetScriptGoal`, `PressUseButton`
/// and `PressAttackButton`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotOrder {
    pub client: u32,
    /// Where to go, and how near counts.
    pub goal: Option<([f32; 3], f32)>,
    /// Hold use until this level time.
    pub use_until_ms: i32,
    /// Attack until this level time.
    pub attack_until_ms: i32,
    /// The player `SetAttacker` named last, and the level time it did.
    pub attacker: Option<(u32, i32)>,
}

/// Black Ops' bot dvars (`sv_bot*`), which `_bot.gsc` sets by difficulty.
/// Times are milliseconds; turn speeds are the share of the remaining angle
/// a bot turns a second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotTuning {
    pub reaction_ms: (i32, i32),
    pub fire_ms: (i32, i32),
    pub pitch_hold_ms: (i32, i32),
    pub yaw_speed: f32,
    pub yaw_speed_ads: f32,
    pub pitch_speed: f32,
    pub pitch_speed_ads: f32,
    /// The whole sight cone, degrees.
    pub fov: f32,
    /// How many server frames ahead it aims at a target.
    pub lead_frames: f32,
    /// Within this a bot fires with no reaction time.
    pub close_distance: f32,
    /// How long it keeps a target that died before looking for another.
    pub death_ms: (i32, i32),
    /// Once it aims down the sights, it holds them at least this long.
    pub ads_ms: (i32, i32),
    /// It sprints only toward a goal at least this far away.
    pub sprint_distance: f32,
    /// Nearer its path's end than this it may crouch, for this long.
    pub crouch_distance: f32,
    pub crouch_ms: (i32, i32),
    /// The chance a frame it starts to strafe, and for how long.
    pub strafe_chance: f32,
    pub strafe_ms: (i32, i32),
    /// It knifes a target this near.
    pub melee_dist: f32,
    /// Whether it throws grenades, and the hold of a throw.
    pub allow_grenades: bool,
    pub grenade_ms: (i32, i32),
}

/// A scripted turret as a client's draw reads it: Black Ops' client draws an
/// enemy's in its weapon's second world model for a player with Hacker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurretModelRow {
    pub entity: i32,
    pub weapon: u32,
    /// `0` free, `1` axis, `2` allies.
    pub team: u8,
    /// The client who owns it; `-1` for none.
    pub owner_client: i32,
    pub carried: bool,
}
