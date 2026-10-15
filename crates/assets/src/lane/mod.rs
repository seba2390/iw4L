mod helpers;
mod iw4;
mod iw5;
mod sink;
mod t5;
mod t6;
pub use t6::{T6UiArt, load_t6_ui_art};

use std::path::Path;

pub(crate) use helpers::{
    build_iw5_static_model_draw, build_static_model_draw, build_t5_static_model_draw,
};
pub(crate) use sink::{CommonWalkSink, MaterialPopulationSink, ZoneWalkSink};
pub(crate) use t5::T5Lane;

use crate::{
    lane_capability::{LaneStatus, PreparedCapability},
    session_load::{PreparedWorld, WorldDrawPolicy},
};
use asset_anim::XAnimBuild;
use asset_core::ZoneGame;
use asset_game::WeaponBuild;
use asset_model::{BodyMeshBuild, FpvMeshBuild, WorldWeaponBuild};
use asset_transport::{LoadProgress, ZoneImage};

pub type FilmVisionCatalog = std::collections::BTreeMap<
    String,
    Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
>;

#[derive(Clone, Debug)]
pub struct LaneGap {
    pub capability: PreparedCapability,
    pub reason: String,
    pub addr: Option<&'static str>,
}

pub struct LoadedWorld {
    pub ui_images: Vec<(String, asset_material::ZoneUiImage)>,
    pub ui_modules: Vec<(String, std::sync::Arc<[u8]>)>,
    pub scripts: crate::ScriptSources,
    pub world: PreparedWorld,
    /// What the map zone itself captured. The local material indices in
    /// `world` are indices into this pool until the match finalizes one.
    pub materials: asset_material::MaterialCatalog,
    pub collision: Option<asset_world::ClipCollision>,
    pub spawns: Vec<asset_world::SpawnPoint>,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub facts: crate::MapFacts,
    /// Bytes the zone arenas held while the walk read them. The arenas
    /// themselves die with the walk; only their size travels.
    pub arena_bytes: usize,
    pub sound: Option<Result<asset_audio::SoundCatalog, String>>,
    pub report: Vec<String>,
    pub gaps: Vec<LaneGap>,
    /// Map static models whose model another zone holds.
    pub(crate) foreign_models: ForeignModels,
}

/// Static models a map places but another zone holds (`,name` stubs),
/// linked once the zones loaded alongside the map have been walked.
#[derive(Default)]
pub(crate) struct ForeignModels {
    pub static_models: Vec<asset_world::ForeignStaticModel>,
    pub clip_models: Vec<(String, helpers::T5ClipModelRow)>,
}

impl ForeignModels {
    pub(crate) fn names(&self) -> std::collections::BTreeSet<String> {
        self.static_models
            .iter()
            .map(|model| model.name.clone())
            .chain(self.clip_models.iter().map(|(name, _)| name.clone()))
            .collect()
    }
}

impl LoadedWorld {
    pub fn with_gap(
        policy: WorldDrawPolicy,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) -> Self {
        let reason = reason.into();
        Self {
            world: PreparedWorld {
                policy,
                ..PreparedWorld::empty(policy)
            },
            report: vec![reason.clone()],
            gaps: vec![LaneGap {
                capability,
                reason,
                addr,
            }],
            ..LoadedWorld::empty(policy)
        }
    }

    pub fn push_gap(
        &mut self,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) {
        self.gaps.push(LaneGap {
            capability,
            reason: reason.into(),
            addr,
        });
    }
}

/// A game's bullet penetration depths, by penetration and surface type.
pub type PenTable = weapon_iw4::PenetrationDepthTable;
/// A game's damage multipliers, by hit location.
pub type LochitTable = [f32; weapon_iw4::HITLOC_COUNT];

#[derive(Default)]
pub struct CommonCensus {
    /// Draw meshes of the map static models this zone holds.
    pub static_meshes: Vec<asset_world::ForeignStaticMesh>,
    pub ui_tables: Vec<asset_core::ZoneTable>,
    pub ui_modules: Vec<(String, std::sync::Arc<[u8]>)>,
    pub ui_images: Vec<(String, asset_material::material_images::ZoneUiImage)>,
    pub scripts: crate::ScriptSources,
    pub scene_models: asset_world::MapXModelSceneCatalog,
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    pub weapons: WeaponBuild,

    pub cac_tables: Vec<asset_game::CapturedStringTable>,
    pub fpv: FpvMeshBuild,
    pub world_weapons: WorldWeaponBuild,

    pub projectile_meshes: asset_model::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub player_anim_sources: asset_anim::PlayerAnimSources,
    pub fx: asset_game::FxCatalog,
    pub fx_models: asset_game::FxModelCatalog,

    pub tracers: asset_game::TracerCatalog,
    pub impact_fx: Option<asset_game::OwnedFxImpactTable>,

    pub material_population: asset_material::MaterialCatalog,

    pub light_defs: Vec<asset_world::CapturedLightDef>,
    pub report: Vec<String>,

    pub pen_table: PenTable,
    pub pen_table_loaded: bool,
    pub lochit_table: Option<LochitTable>,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    /// Bytes the common_mp arenas held while the walk read them; the arenas
    /// themselves do not outlive it.
    pub s1_common_bytes: usize,

    pub teamsets: std::collections::HashMap<String, asset_game::MapTeamSettings>,
    pub film_visions: FilmVisionCatalog,

    pub pending_images: Option<asset_material::material_images::ImageDemandPlan>,

    pub(crate) preparation: Option<Box<dyn CommonFamilyCompiler>>,
}

pub(crate) struct CommonPreparationProducts {
    pub weapons: WeaponBuild,
    pub materials: asset_material::MaterialCatalog,
    pub fpv: FpvMeshBuild,
    pub world: WorldWeaponBuild,
    pub projectiles: asset_model::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub fx: asset_game::FxCatalog,
    pub report: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CommonDependencyRefusal {
    NativeMaterial {
        family: asset_core::FamilyId,
        name: String,
        reason: String,
    },
    WeaponPreparation(asset_game::WeaponPreparationRefusal),
}

pub(crate) struct CommonPreparationResult {
    pub products: CommonPreparationProducts,
    pub refusals: Vec<CommonDependencyRefusal>,
}

pub(crate) trait CommonFamilyCompiler: Send {
    fn compile(self: Box<Self>, products: CommonPreparationProducts) -> CommonPreparationResult;
}

pub struct MaterialPopulation {
    pub materials: asset_material::MaterialCatalog,
    pub light_defs: Vec<asset_world::CapturedLightDef>,
    pub walked: usize,
    pub report: Vec<String>,
    pub cac_tables: Vec<asset_game::CapturedStringTable>,
    pub scripts: crate::ScriptSources,
}

impl Default for MaterialPopulation {
    fn default() -> Self {
        Self {
            materials: asset_material::MaterialCatalog::default(),
            light_defs: Vec::new(),
            walked: 0,
            report: Vec::new(),
            cac_tables: Vec::new(),
            scripts: crate::ScriptSources::default(),
        }
    }
}

pub trait ZoneLane: Send + Sync {
    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)];

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,

        shared_surfaces: asset_model::SharedXModelSurfaces,

        material_seed: asset_material::MaterialCatalog,
        common_film_visions: &FilmVisionCatalog,
    ) -> LoadedWorld;

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus;

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation;
}

pub fn lane(game: ZoneGame) -> &'static dyn ZoneLane {
    match game {
        ZoneGame::Iw4 => &iw4::Iw4Lane,
        ZoneGame::T5 => &t5::T5Lane,
        ZoneGame::Iw5 => &iw5::Iw5Lane,
        ZoneGame::T6 => &t6::T6Lane,
        _ => &UnreadLane,
    }
}

/// The lane of a game whose zones IW4L does not decode: every capability is
/// missing, and a map of it loads as an empty world with that gap.
struct UnreadLane;

impl UnreadLane {
    const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::MissingDecoder),
        (
            PreparedCapability::PreparedWorld,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::CollisionSpawns,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::WeaponCatalog,
            LaneStatus::MissingDecoder,
        ),
        (PreparedCapability::BodySkeleton, LaneStatus::MissingDecoder),
        (PreparedCapability::PlayableFfa, LaneStatus::MissingDecoder),
    ];
}

impl ZoneLane for UnreadLane {
    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        _progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        _material_seed: asset_material::MaterialCatalog,
        _common_film_visions: &FilmVisionCatalog,
    ) -> LoadedWorld {
        let mut world = LoadedWorld::with_gap(
            WorldDrawPolicy::unread(image.game),
            PreparedCapability::PreparedWorld,
            format!(
                "{}: IW4L does not decode {} zones",
                path.display(),
                image.game.prefix()
            ),
            Some("assets::lane::unread::load_world/no_decoder"),
        );
        let Some(format) = asset_transport::registered_zone_format(image.game) else {
            return world;
        };
        let map = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        for zone in (format.script_zones)(map) {
            let scripts = if zone == map {
                (format.scripts)(&image.bytes)
            } else {
                match asset_transport::open_zone(path.with_file_name(format!("{zone}.ff"))) {
                    Ok(companion) => (format.scripts)(&companion.bytes),
                    Err(error) => {
                        world.report.push(format!("{zone}: {error}"));
                        continue;
                    }
                }
            };
            let count = scripts.modules.len();
            for (name, bytes) in scripts.modules {
                world.scripts.capture_compiled(&name, bytes);
            }
            for (name, text) in scripts.configs {
                world.scripts.capture_config(&name, text);
            }
            world.report.push(format!("{zone}: {count} script modules"));
            world.report.extend(scripts.report);
        }
        world.report.push(format!(
            "{map}: {} script modules after its zones",
            world.scripts.len()
        ));
        let entities = (format.entities)(&image.bytes);
        world.report.extend(entities.report);
        if let Some(text) = entities.text {
            world.scripts.set_entities(text);
        }
        let drawn = (format.world)(&image.bytes);
        world.report.extend(drawn.report);
        if let Some(geometry) = drawn.geometry {
            world.world.draw = Some(asset_world::world_t7::build_world_draw(&geometry));
        }
        if let Some(collision) = drawn.collision {
            match asset_world::world_t7::build_clip_collision(&collision) {
                Ok(clip) => world.collision = Some(clip),
                Err(error) => world.report.push(format!("t7 collision: {error}")),
            }
        }
        world
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        _progress: &LoadProgress,
        _decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus {
        CommonCensus {
            material_population: material_seed,
            report: vec![format!(
                "{}: IW4L does not decode {} zones",
                path.display(),
                image.game.prefix()
            )],
            ..CommonCensus::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        _progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        MaterialPopulation {
            materials: material_seed,
            report: vec![format!(
                "{}: IW4L does not decode {} zones",
                path.display(),
                image.game.prefix()
            )],
            ..MaterialPopulation::default()
        }
    }
}

pub const LANE_GAPS: &[&str] = &[
    "assets::lane::iw4::load_world/zone_header",
    "assets::lane::iw4::load_world/zone_arenas",
    "assets::lane::iw4::load_world/no_gfx_world",
    "assets::lane::iw4::load_world/world_mesh",
    "assets::lane::t5::load_world/zone_header",
    "assets::lane::t5::load_world/zone_arenas",
    "assets::lane::t5::load_world/no_gfx_world",
    "assets::lane::t5::load_world/world_mesh",
    "assets::lane::iw5::load_world/zone_header",
    "assets::lane::iw5::load_world/zone_arenas",
    "assets::lane::iw5::load_world/no_gfx_world",
    "assets::lane::iw5::load_world/world_mesh",
    "assets::lane::t6::load_world/no_decoder",
    "assets::lane::unread::load_world/no_decoder",
    "assets::session_load::load_prepared_match/zone_open",
];

impl LoadedWorld {
    pub fn empty(policy: WorldDrawPolicy) -> Self {
        Self {
            ui_images: Vec::new(),
            ui_modules: Vec::new(),
            scripts: Default::default(),
            world: PreparedWorld::empty(policy),
            materials: Default::default(),
            collision: Default::default(),
            spawns: Default::default(),
            bodies: Default::default(),
            fpv_meshes: Default::default(),
            xanims: Default::default(),
            facts: Default::default(),
            arena_bytes: Default::default(),
            sound: Default::default(),
            report: Default::default(),
            gaps: Default::default(),
            foreign_models: Default::default(),
        }
    }
}

/// The compiled scripts a map runs as they are: the zones its game's script
/// format names, in load order, then the downloaded patches the owner
/// captured, a later zone's module replacing an earlier one's.
pub(crate) fn capture_script_zones(
    path: &Path,
    image: &ZoneImage,
    captured: Vec<std::path::PathBuf>,
    scripts: &mut crate::ScriptSources,
    report: &mut Vec<String>,
) {
    let Some(format) = asset_transport::registered_script_zone_format(image.game) else {
        return;
    };
    let map = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let zones = (format.script_zones)(map);
    if zones.is_empty() {
        return;
    }
    let files = zones
        .iter()
        .map(|zone| (zone.as_str() != map).then(|| path.with_file_name(format!("{zone}.ff"))))
        .chain(captured.into_iter().map(Some));
    for file in files {
        let opened;
        let (name, bytes) = match &file {
            None => (map.to_owned(), &image.bytes),
            Some(file) if !file.is_file() => continue,
            Some(file) => match asset_transport::open_zone(file) {
                Ok(zone) => {
                    opened = zone;
                    (file.display().to_string(), &opened.bytes)
                }
                Err(error) => {
                    report.push(format!("scripts: {}: {error}", file.display()));
                    continue;
                }
            },
        };
        let found = (format.scripts)(bytes);
        report.push(format!("scripts: {name}: {} modules", found.modules.len()));
        report.extend(found.report);
        for (module, bytes) in found.modules {
            scripts.capture_module(&module, bytes);
        }
        for (file, text) in found.configs {
            scripts.capture_config(&file, text);
        }
        for table in found.tables {
            scripts.capture_zone_table(table);
        }
        for layout in found.layouts {
            scripts.capture_layout(layout);
        }
        for barrier in found.barriers {
            scripts.capture_barrier(barrier);
        }
        for weapon in found.weapons {
            scripts.capture_weapon(weapon);
        }
    }
}
