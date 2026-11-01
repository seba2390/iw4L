use fastfile_t6::{AssetType, ZoneLoad, ddl::ddl as parse_ddl};
mod camouflage;
mod common_compile;
mod destructible;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use bevy::prelude::Image;

use super::{CommonCensus, LoadedWorld, MaterialPopulation, ZoneLane};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use asset_transport::ZoneImage;
use asset_transport::progress::{LoadProgress, StageId};
use asset_world::WorldDrawPolicy;

struct T6ModelCapture {
    pub skel: asset_model::ModelSkel,
    pub surface_materials: Vec<Option<String>>,
    pub view: bool,
    pub world: bool,
}

struct T6MaterialCapture {
    pub native: Option<T6NativeMaterial>,
}

#[derive(Clone)]
struct T6NativeMaterial {
    pub header: Vec<u8>,
    pub technique_set: String,
    pub textures: Vec<asset_material::t6_techset::T6Texture>,
    pub constants: Vec<asset_material::MaterialConstant>,
    pub state: asset_material::t6_techset::T6MaterialState,
}

#[derive(Default)]
struct T6Content {
    pub path: std::path::PathBuf,
    pub models: Vec<T6ModelCapture>,
    pub materials: BTreeMap<String, T6MaterialCapture>,
    pub sound_names: std::collections::BTreeSet<String>,
    pub xanims: asset_anim::XAnimBuild,
    pub note_sounds: std::collections::BTreeSet<String>,
    pub hands: Option<String>,
    pub techsets: BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    pub melee: Option<asset_game::T6Melee>,
    pub fx: Vec<asset_game::T6FxCapture>,
    pub fx_materials: BTreeMap<String, T6MaterialCapture>,
    pub camouflages: BTreeMap<String, Vec<asset_game::WeaponCamouflage>>,
    pub report: Vec<String>,
}

fn t6_script_model_instance(
    placement: &asset_world::ScriptModelPlacement,
    scene_assets: &mut asset_world::MapXModelSceneCatalog,
    definitions: &BTreeMap<String, Arc<xmodel_runtime::T5DestructibleDef>>,
) -> asset_world::ScriptModelSceneInstance {
    use bevy::math::{EulerRot, Quat, Vec3};
    let current_model = asset_world::MapXModelAssetKey(placement.model.clone());
    if scene_assets.get(&current_model).is_none() {
        scene_assets.insert(
            current_model.clone(),
            asset_world::MapXModelSceneAsset::Unavailable {
                reason: "T6 script model XModel is not in the map zone",
            },
        );
    }
    let definition = definitions.get(&placement.destructible_def).cloned();
    let mut dobj_state =
        xmodel_runtime::DObjSemanticState::bind_pose(placement.model.clone(), 1, 1);
    if let Some(def) = &definition
        && placement.model == def.model
        && let Some(asset_world::MapXModelSceneAsset::T6(skel)) = scene_assets.get(&current_model)
    {
        let health: Vec<_> = def.pieces.iter().map(|piece| piece.health as i16).collect();
        dobj_state.hide_part_bits = def.hide_parts(&health, &skel.bone_names);
    }
    let [pitch, yaw, roll] = placement.angles.map(f32::to_radians);
    asset_world::ScriptModelSceneInstance {
        id: placement.id,
        dobj_state,
        current_model,
        transform: bevy::prelude::Transform {
            translation: Vec3::from_array(placement.origin),
            rotation: Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll),
            scale: Vec3::ONE,
        },
        lighting_origin: placement.lighting_origin.unwrap_or(placement.origin),
        metadata: asset_world::ScriptModelMetadata {
            gameobject: placement.gameobject.clone(),
            targetname: placement.targetname.clone(),
            script_noteworthy: placement.script_noteworthy.clone(),
            destructible_type: placement.destructible_type.clone(),
            destructible_def: placement.destructible_def.clone(),
            t5_destructible: definition,
            target: placement.target.clone(),
            script_exploder: placement.script_exploder.clone(),
            brush_link: placement.brush_link.clone(),
            script_accumulate: placement.script_accumulate,
            script_threshold: placement.script_threshold,
            script_destructable_area: placement.script_destructable_area.clone(),
            script_fxid: placement.script_fxid.clone(),
        },
    }
}

fn melee_weapon(load: &fastfile_t6::ZoneLoad) -> Option<asset_game::T6Melee> {
    use fastfile_t6::weapon::{WeaponView, def, weap_anim};
    let weapon = load
        .assets
        .iter()
        .filter_map(|asset| WeaponView::new(load, asset))
        .find(|weapon| {
            matches!(
                weapon.name(),
                Some(asset_game::T6_MELEE_WEAPON | "knife_zm")
            )
        })?;
    let clip = |slot: usize| {
        weapon
            .xanim(slot as u32)
            .filter(|name| !name.is_empty())
            .map(|name| {
                format!(
                    "{}{}",
                    asset_game::T6_XANIM_PREFIX,
                    name.to_ascii_lowercase()
                )
            })
    };
    Some(asset_game::T6Melee {
        knife: asset_game::t6_model_name(weapon.def_asset_array_name(def::GUN_XMODEL, 0)?),
        melee: clip(weap_anim::MELEE)?,
        charge: clip(weap_anim::MELEE_CHARGE),
    })
}

const COLOR_MAP_SAMPLER_HASH: u32 = 0xa0ab_1041;
const HANDS_ZONE: &str = "faction_seals_mp.ff";

/// The post materials Black Ops 2's renderer registers for its screen
/// overlays: its burn, electric shock, poison and revive passes, the blur
/// pass's blend and the blur's eight filters.
const T6_OVERLAY_MATERIALS: [&str; 13] = [
    "flamethrowerfx_color_distort_overlay_bloom",
    "zombie_electric_shock_overlay",
    "poison",
    "revivefx",
    "feedbackblend",
    "filter_symmetric_1",
    "filter_symmetric_2",
    "filter_symmetric_3",
    "filter_symmetric_4",
    "filter_symmetric_5",
    "filter_symmetric_6",
    "filter_symmetric_7",
    "filter_symmetric_8",
];
/// The filter materials the installed maps' client scripts map and enable
/// (`game_t6::overlay`), captured when the map's zones carry them.
const T6_FILTER_MATERIALS: [&str; 1] = ["generic_filter_zombie_perk_vulture"];
const HANDS_MODEL: &str = "c_usa_mp_seal6_longsleeve_viewhands";

fn hands_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let path = common.with_file_name(HANDS_ZONE);
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 hands: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 hands: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

fn xanim_notes<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8]) -> Vec<&'z str> {
    let count = u32::from(header.get(34).copied().unwrap_or(0));
    let Some(raw) = header_u32(header, 96).filter(|&raw| raw != 0 && raw < 0xFFFF_FFFE) else {
        return Vec::new();
    };
    let e = raw - 1;
    let notify = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    (0..count)
        .filter_map(|i| {
            let id = load.blocks.bytes(notify.at(8 * i), 2).ok()?;
            load.script_string(u16::from_le_bytes([id[0], id[1]]))
        })
        .collect()
}

fn capture_player_anim_sources(
    zones: &[&ZoneLoad],
    family: asset_core::FamilyId,
) -> asset_anim::PlayerAnimSources {
    let mut sources = asset_anim::PlayerAnimSources::default();
    for load in zones {
        for asset in &load.assets {
            if asset.ty != AssetType::RawFile || asset.header.len() < 12 {
                continue;
            }
            let Some(name) = header_str(load, &asset.header, 0) else {
                continue;
            };
            let Some(len) = header_u32(&asset.header, 4).map(|n| n as usize) else {
                continue;
            };
            let Some(ptr) = header_u32(&asset.header, 8).and_then(decode_ptr) else {
                continue;
            };
            let Ok(bytes) = load.blocks.bytes(ptr, len) else {
                continue;
            };
            sources.capture(family, name, bytes, false);
        }
    }
    sources.compile();
    sources
}

fn capture_xanims(
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    player_anim_sources: &asset_anim::PlayerAnimSources,
    content: &mut T6Content,
) {
    use fastfile_t6::weapon::WeaponView;
    let mut wanted = std::collections::BTreeSet::new();
    for (source, asset) in std::iter::once(load)
        .chain(others)
        .flat_map(|source| source.assets.iter().map(move |asset| (source, asset)))
    {
        let Some(weapon) = WeaponView::new(source, asset) else {
            continue;
        };
        if weapon.name().is_some() {
            wanted.extend(asset_game::t6_weapon_xanim_names(weapon));
            wanted.extend(asset_game::t6_attachment_xanim_names(weapon));
        }
    }
    let ns = asset_core::AssetNamespace::T6;
    let (total, mut captured) = (wanted.len(), 0usize);
    let mut default_parts = None;
    let assets = std::iter::once(load)
        .chain(others)
        .flat_map(|load| load.assets.iter().map(move |asset| (load, asset)));
    for (load, asset) in assets {
        if asset.ty != AssetType::XAnimParts {
            continue;
        }
        if default_parts.is_none()
            && header_str(load, &asset.header, 0).is_some_and(|name| name == "void")
        {
            default_parts = Some((load, asset));
        }
        if !header_str(load, &asset.header, 0)
            .is_some_and(|name| wanted.remove(&name.to_ascii_lowercase()))
        {
            continue;
        }
        if content
            .xanims
            .capture_xanim_t6(ns, asset_game::T6_XANIM_PREFIX, load, asset)
        {
            captured += 1;
            content.note_sounds.extend(
                xanim_notes(load, &asset.header)
                    .into_iter()
                    .filter_map(|note| {
                        asset_game::t5_inline_note_alias(note, asset_game::T5_NOTE_SOUND_PREFIX)
                    })
                    .map(str::to_owned),
            );
        }
    }
    // An external XAnimParts entry (`,name`) whose target is in no loaded
    // zone receives a copy of the startup zone's `void` clip.
    if !wanted.is_empty()
        && let Some((default_load, default_asset)) = default_parts
    {
        let default_key = format!("{}void", asset_game::T6_XANIM_PREFIX);
        if content.xanims.capture_xanim_t6(
            ns,
            asset_game::T6_XANIM_PREFIX,
            default_load,
            default_asset,
        ) && let Some(void) = content
            .xanims
            .get(ns, &default_key)
            .map(|entry| (*entry.parts).clone())
        {
            for (source, asset) in std::iter::once(load)
                .chain(others)
                .flat_map(|source| source.assets.iter().map(move |asset| (source, asset)))
            {
                if asset.ty == AssetType::XAnimParts
                    && let Some(name) = header_str(source, &asset.header, 0)
                        .and_then(|name| name.strip_prefix(','))
                        .map(str::to_ascii_lowercase)
                    && wanted.remove(&name)
                {
                    let mut parts = void.clone();
                    parts.name = format!("{}{name}", asset_game::T6_XANIM_PREFIX);
                    content.xanims.insert_in(
                        ns,
                        asset_anim::CapturedXAnim {
                            namespace: ns,
                            parts: Arc::new(parts),
                        },
                    );
                    captured += 1;
                }
            }
        }
    }
    content.report.push(format!(
        "t6 xanims: {captured} of {total} weapon animations captured; {} notetrack sounds",
        content.note_sounds.len()
    ));
    let mut body_names: std::collections::BTreeSet<String> = player_anim_sources
        .leaf_names()
        .map(str::to_ascii_lowercase)
        .collect();
    let total = body_names.len();
    let mut captured = 0;
    for (source, asset) in std::iter::once(load)
        .chain(others)
        .flat_map(|source| source.assets.iter().map(move |asset| (source, asset)))
    {
        if asset.ty == AssetType::XAnimParts
            && header_str(source, &asset.header, 0)
                .is_some_and(|name| body_names.remove(&name.to_ascii_lowercase()))
            && content
                .xanims
                .capture_xanim_t6(asset_core::AssetNamespace::T6, "", source, asset)
        {
            captured += 1;
        }
    }
    content.report.push(format!(
        "t6 player clips: {captured}/{total} native clips captured"
    ));
    if asset_transport::t6_content::T6ContentMode::for_path(&content.path)
        == asset_transport::t6_content::T6ContentMode::Zombies
    {
        let mut captured = 0;
        for (source, asset) in std::iter::once(load)
            .chain(others)
            .flat_map(|source| source.assets.iter().map(move |asset| (source, asset)))
        {
            if asset.ty == AssetType::XAnimParts
                && header_str(source, &asset.header, 0).is_some_and(|name| {
                    name.starts_with("a_zombie")
                        || name.starts_with("ai_zombie")
                        || name.starts_with("zm_walk")
                        || name.starts_with("zombie_")
                        || name.starts_with("o_zombie_board_")
                })
                && content.xanims.capture_xanim_t6(
                    asset_core::AssetNamespace::T6,
                    "",
                    source,
                    asset,
                )
            {
                captured += 1;
            }
        }
        content
            .report
            .push(format!("t6 zombie clips: {captured} native clips captured"));
    }
}

fn capture_sounds(
    path: &Path,
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    claim: Option<asset_audio::ZoneSoundCapture>,
    content: &mut T6Content,
) {
    let mut names = std::collections::BTreeSet::new();
    for zone in std::iter::once(load).chain(others) {
        for asset in &zone.assets {
            let Some(weapon) = fastfile_t6::weapon::WeaponView::new(zone, asset) else {
                continue;
            };
            if weapon.name().is_some() {
                names.extend(asset_game::t6_weapon_sound_names(weapon));
                names.extend(asset_game::t6_attachment_sound_names(weapon));
            }
        }
    }
    names.extend(content.note_sounds.iter().cloned());
    names.extend(
        asset_game::T6_EQUIPMENT_SOUNDS
            .iter()
            .map(|&name| name.to_owned()),
    );
    let (banks, mut report) = asset_audio::t6_sound_banks(path);
    let foley = foley_zone(path, &mut report);
    let mode = asset_transport::t6_content::T6ContentMode::for_path(path);
    let code = sound_zone(
        &path.with_file_name(format!(
            "{}.ff",
            if mode == asset_transport::t6_content::T6ContentMode::Zombies {
                "code_post_gfx_zm"
            } else {
                "code_post_gfx_mp"
            }
        )),
        &mut report,
    );
    let loads: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load)
        .chain(others)
        .chain(&foley)
        .chain(&code)
        .collect();
    names.extend(
        asset_audio::t6_sound_names(&loads)
            .into_iter()
            .filter(|name| name.starts_with("mus_") || name.starts_with("uin_")),
    );
    let (catalog, filled, gaps) =
        asset_audio::capture_t6_sounds(path, &loads, &banks, names.iter().map(String::as_str));
    report.push(format!(
        "t6 sounds: {} of {} weapon aliases read (+{} secondary layers, {} audio assets) from {} sound banks; {} gaps",
        filled.iter().filter(|name| names.contains(*name)).count(),
        names.len(),
        filled.iter().filter(|name| !names.contains(*name)).count(),
        catalog.loaded_sounds().len(),
        banks.len(),
        gaps.len()
    ));
    report.extend(gaps.into_iter().take(32));
    if let Some(mut capture) = claim {
        capture.set_t6(catalog);
        capture.deposit(Ok(()));
    } else {
        report.push("t6 sounds: common_mp sound source already claimed".to_owned());
    }
    content.sound_names = filled.into_iter().collect();
    content.report.extend(report);
}

fn patch_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let mode = asset_transport::t6_content::T6ContentMode::for_path(common);
    let path = common.with_file_name(format!("{}.ff", mode.patch()));
    content_zone(&path, report)
}

fn content_zone(path: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let image = match asset_transport::open_t6_zone(path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 content: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 content: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

/// The map's gumps: the zones `<map>_gump_<name>` beside it, each by the
/// gump name the client scripts load it by.
fn map_gumps(path: &Path, report: &mut Vec<String>) -> Vec<(String, ZoneLoad)> {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let prefix = format!("{stem}_gump_");
    let mut names: Vec<String> = path
        .parent()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(str::to_owned))
        .filter_map(|name| {
            let gump = name.strip_suffix(".ff")?;
            gump.starts_with(&prefix).then(|| gump.to_owned())
        })
        .collect();
    names.sort_unstable();
    names
        .into_iter()
        .filter_map(|gump| {
            let load = content_zone(&path.with_file_name(format!("{gump}.ff")), report)?;
            Some((gump, load))
        })
        .collect()
}

/// The models the gumps carry, by name, each with the gumps that carry it
/// (the first gump's copy drawn for all). A model whose materials the gump
/// and the map's zones do not define is left out.
fn gump_models<'z>(
    gumps: &'z [(String, ZoneLoad)],
    zones: &[&'z ZoneLoad],
) -> BTreeMap<&'z str, (asset_model::T6Model<'z>, Vec<String>)> {
    let mut models: BTreeMap<&'z str, (asset_model::T6Model<'z>, Vec<String>)> = BTreeMap::new();
    for (gump, load) in gumps {
        for asset in &load.assets {
            let Some(model) = asset_model::T6Model::new(load, asset) else {
                continue;
            };
            let Some(name) = model.name().filter(|name| !name.starts_with(',')) else {
                continue;
            };
            let defined = (0..model.surface_count()).all(|surface| {
                model
                    .material_slot(surface)
                    .and_then(|slot| load.asset_at(slot))
                    .is_some_and(|material| resolve_material(load, zones, material).is_ok())
            });
            if !defined {
                continue;
            }
            models
                .entry(name)
                .or_insert_with(|| (model, Vec::new()))
                .1
                .push(gump.clone());
        }
    }
    models
}

fn sound_zone(path: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let image = asset_transport::open_t6_zone(path)
        .map_err(|error| report.push(format!("t6 sound source {}: {error:?}", path.display())))
        .ok()?;
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 sound source {}: {error:?}", path.display()));
    }
    Some(load)
}

fn foley_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    if asset_transport::t6_content::T6ContentMode::for_path(common)
        == asset_transport::t6_content::T6ContentMode::Zombies
    {
        return None;
    }
    let dir = common.parent()?;
    let smallest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("mp_") && name.ends_with(".ff")
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.len(), entry.path())))
        .min()
        .map(|(_, path)| path);
    let Some(path) = smallest else {
        report.push(format!(
            "t6 sounds: no map zone in {} for weapon foley",
            dir.display()
        ));
        return None;
    };
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 sounds: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 sounds: {} walk: {error:?}", path.display()));
    }
    report.push(format!("t6 sounds: weapon foley from {}", path.display()));
    Some(load)
}

pub struct T6Lane;

impl T6Lane {
    pub const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::SupportedPopulated),
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
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::BodySkeleton,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];
}

fn schema() -> Result<&'static fastfile_t6::schema::Schema, String> {
    static SCHEMA: OnceLock<Result<fastfile_t6::schema::Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| fastfile_t6::schema::parse().map_err(|e| format!("T6 load plan: {e:?}")))
        .as_ref()
        .map_err(Clone::clone)
}

const FONT_MAP_HASH: u32 = 0xd2ee_eeb8;
const COLOR_MAP_HASH: u32 = 0xa0ab_1041;
const MATERIAL_TEXTURE_COUNT: usize = 84;
const MATERIAL_TEXTURE_TABLE: usize = 96;
const MATERIAL_TEXTURE_DEF: u32 = 16;
const IMAGE_STREAMED_PART_HASH: usize = 40;
const IMAGE_STREAMED_PART_COUNT: usize = 60;
const IMAGE_NAME: usize = 72;
const IMAGE_HASH: usize = 76;

fn header_u32(header: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(header.get(at..at + 4)?.try_into().ok()?))
}

fn header_str<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8], at: usize) -> Option<&'z str> {
    let raw = header_u32(header, at)?;
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    let p = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    core::str::from_utf8(load.blocks.cstr(p).ok()?).ok()
}

fn open_ipaks(zone: &Path, report: &mut Vec<String>) -> Vec<asset_transport::IPak> {
    let Some(dir) = zone.parent() else {
        return Vec::new();
    };
    let mut directories = vec![dir.to_owned()];
    if let Ok(zones) = asset_transport::discover::find_t6_localized_zones(zone, None) {
        directories.extend(
            zones
                .into_iter()
                .filter_map(|zone| zone.path.parent().map(Path::to_owned)),
        );
    }
    directories.sort();
    directories.dedup();
    let mut paths = Vec::new();
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            report.push(format!("t6 content: cannot read {}", directory.display()));
            continue;
        };
        paths.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ipak"))
        }));
    }
    paths.sort();
    paths
        .iter()
        .filter_map(|path| match asset_transport::IPak::open(path) {
            Ok(ipak) => Some(ipak),
            Err(error) => {
                report.push(format!("t6 content: {error}"));
                None
            }
        })
        .collect()
}

fn t6_sampler_state(state: u8) -> u8 {
    const FILTER_LINEAR: u8 = 2;
    let filter = state & 0b111;
    (state & !0b111) | filter.min(FILTER_LINEAR)
}

type DecodedTextures = BTreeMap<String, Arc<Image>>;

fn decode_ptr(raw: u32) -> Option<fastfile_t6::Ptr> {
    (raw != 0 && raw < 0xFFFF_FFFE).then(|| fastfile_t6::Ptr {
        block: ((raw - 1) >> 29) as u8,
        offset: (raw - 1) & 0x1FFF_FFFF,
    })
}

const MATERIAL_CONSTANT_COUNT: usize = 85;
const MATERIAL_TECHNIQUE_SET: u32 = 92;
const MATERIAL_CONSTANT_TABLE: usize = 100;
const MATERIAL_STATE_BITS_ENTRY: usize = 48;
const MATERIAL_STATE_BITS_TABLE: usize = 104;
const MATERIAL_STATE_BITS: u32 = 20;
const MATERIAL_CONSTANT_DEF: u32 = 32;
const TECHNIQUE_SET_WORLD_VERT_FORMAT: usize = 4;
const TECHNIQUE_SET_TECHNIQUES: usize = 8;
const TECHNIQUE_HEADER: u32 = 8;
const MATERIAL_PASS: u32 = 24;
const VERTEX_DECL_HEADER: usize = 36;
const SHADER_ARGUMENT: u32 = 12;

fn read_technique_set(
    load: &fastfile_t6::ZoneLoad,
    header: &[u8],
) -> Option<asset_material::t6_techset::T6TechniqueSet> {
    use asset_material::t6_techset::{
        T6Argument, T6Pass, T6Technique, T6TechniqueSet, argument_type,
    };
    let blocks = &load.blocks;
    let u16_le = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
    let shader = |pointer: Option<fastfile_t6::Ptr>| -> Option<(String, Vec<u8>)> {
        let header = blocks.bytes(pointer?, 16).ok()?;
        let program = decode_ptr(header_u32(header, 8)?)?;
        let size = header_u32(header, 12)? as usize;
        Some((
            header_str(load, header, 0).unwrap_or("").to_owned(),
            blocks.bytes(program, size).ok()?.to_vec(),
        ))
    };
    let techniques = (0..T6_TECHNIQUE_COUNT)
        .map(|index| {
            let technique = decode_ptr(header_u32(header, TECHNIQUE_SET_TECHNIQUES + 4 * index)?)?;
            let head = blocks.bytes(technique, TECHNIQUE_HEADER as usize).ok()?;
            let (flags, pass_count) = (u16_le(head, 4), u16_le(head, 6));
            let passes = (0..u32::from(pass_count))
                .map(|p| {
                    let pass = blocks
                        .bytes(
                            technique.at(TECHNIQUE_HEADER + MATERIAL_PASS * p),
                            MATERIAL_PASS as usize,
                        )
                        .ok()?;
                    let (vertex_name, vertex) = shader(decode_ptr(header_u32(pass, 4)?))?;
                    let (pixel_name, pixel) = shader(decode_ptr(header_u32(pass, 8)?))?;
                    let count = u32::from(pass[12]) + u32::from(pass[13]) + u32::from(pass[14]);
                    let table = decode_ptr(header_u32(pass, 20)?);
                    let arguments = (0..count)
                        .filter_map(|a| {
                            let raw = blocks.bytes(table?.at(SHADER_ARGUMENT * a), 12).ok()?;
                            let kind = u16_le(raw, 0);
                            let def = header_u32(raw, 8)?;
                            let literal = matches!(
                                kind,
                                argument_type::LITERAL_VERTEX_CONST
                                    | argument_type::LITERAL_PIXEL_CONST
                            )
                            .then(|| {
                                let words = blocks.bytes(decode_ptr(def)?, 16).ok()?;
                                Some(core::array::from_fn(|i| {
                                    header_u32(words, 4 * i).unwrap_or(0)
                                }))
                            })
                            .flatten();
                            Some(T6Argument {
                                kind,
                                offset: u16_le(raw, 2),
                                size: raw[4],
                                buffer: u16_le(raw, 6),
                                def,
                                literal,
                            })
                        })
                        .collect();
                    let layer_routing = match decode_ptr(header_u32(pass, 0)?) {
                        Some(decl) => {
                            let decl = blocks.bytes(decl, VERTEX_DECL_HEADER).ok()?;
                            decl[4..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .take(usize::from(decl[0]))
                                .copied()
                                .filter(|[source, _]| (6..=9).contains(source))
                                .collect()
                        }
                        None => Vec::new(),
                    };
                    Some(T6Pass {
                        vertex_name,
                        vertex,
                        pixel_name,
                        pixel,
                        custom_sampler_flags: pass[15],
                        arguments,
                        layer_routing,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(T6Technique { flags, passes })
        })
        .collect();
    Some(T6TechniqueSet {
        name: header_str(load, header, 0)?
            .trim_start_matches(',')
            .to_owned(),
        world_vert_format: header[TECHNIQUE_SET_WORLD_VERT_FORMAT],
        techniques,
    })
}

const T6_TECHNIQUE_COUNT: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColourMapAlpha {
    Mask,
    Gloss,
}

fn resolve_material<'a>(
    load: &'a fastfile_t6::ZoneLoad,
    zones: &[&'a fastfile_t6::ZoneLoad],
    material: &'a fastfile_t6::LoadedAsset,
) -> Result<(&'a fastfile_t6::ZoneLoad, &'a fastfile_t6::LoadedAsset), String> {
    let raw_name = header_str(load, &material.header, 0).ok_or("T6 material name missing")?;
    let Some(name) = raw_name.strip_prefix(',') else {
        return Ok((load, material));
    };
    zones
        .iter()
        .find_map(|zone| {
            zone.assets
                .iter()
                .find(|asset| {
                    asset.ty == AssetType::Material
                        && header_str(zone, &asset.header, 0) == Some(name)
                })
                .map(|asset| (*zone, asset))
        })
        .ok_or_else(|| format!("T6 material {name}: definition missing"))
}

#[allow(clippy::too_many_arguments)]
fn capture_native(
    load: &fastfile_t6::ZoneLoad,
    zones: &[&fastfile_t6::ZoneLoad],
    address: Option<fastfile_t6::Ptr>,
    material: &fastfile_t6::LoadedAsset,
    colour_alpha: ColourMapAlpha,
    ipaks: &[asset_transport::IPak],
    decoded: &mut DecodedTextures,
    techsets: &mut BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    report: &mut Vec<String>,
) -> Option<T6NativeMaterial> {
    let techset = material
        .field(MATERIAL_TECHNIQUE_SET)
        .map(|index| &load.assets[index])
        .or_else(|| load.asset_at(address?.at(MATERIAL_TECHNIQUE_SET)))?;
    let technique_set = header_str(load, &techset.header, 0)?
        .trim_start_matches(',')
        .to_owned();
    if !techsets.contains_key(&technique_set) {
        let defined = |set: &asset_material::t6_techset::T6TechniqueSet| {
            set.techniques.iter().any(Option::is_some)
        };
        let read = read_technique_set(load, &techset.header)
            .filter(|set| defined(set))
            .or_else(|| {
                zones.iter().find_map(|zone| {
                    zone.assets
                        .iter()
                        .filter(|asset| asset.ty == AssetType::TechniqueSet)
                        .filter(|asset| {
                            header_str(zone, &asset.header, 0)
                                .is_some_and(|name| name.trim_start_matches(',') == technique_set)
                        })
                        .filter_map(|asset| read_technique_set(zone, &asset.header))
                        .find(|set| defined(set))
                })
            });
        match read {
            Some(set) => {
                techsets.insert(technique_set.clone(), set);
            }
            None => {
                report.push(format!(
                    "t6 native: {technique_set}: technique set unreadable"
                ));
                return None;
            }
        }
    }
    let header = &material.header;
    let entries = header
        .get(MATERIAL_STATE_BITS_ENTRY..MATERIAL_STATE_BITS_ENTRY + 36)?
        .try_into()
        .ok()?;
    let count = usize::from(*header.get(86)?);
    let mut rows = Vec::with_capacity(count);
    if count > 0 {
        let table = decode_ptr(header_u32(header, MATERIAL_STATE_BITS_TABLE)?)?;
        for row in 0..count {
            let bytes = load
                .blocks
                .bytes(table.at(u32::try_from(row).ok()? * MATERIAL_STATE_BITS), 8)
                .ok()?;
            rows.push([header_u32(bytes, 0)?, header_u32(bytes, 4)?]);
        }
    }
    let state = asset_material::t6_techset::T6MaterialState { entries, rows };
    let lit_state = state
        .first_bits(asset_material::t6_techset::T6_TECHNIQUE_LIT)
        .map(|bits| bits[0]);
    let opaque = colour_alpha == ColourMapAlpha::Mask
        && lit_state.is_some_and(|bits| {
            asset_material::MaterialDrawMode::from_state_bits([bits, 0])
                == asset_material::MaterialDrawMode::Opaque
        });
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT)?);
    let table = decode_ptr(header_u32(header, MATERIAL_TEXTURE_TABLE)?);
    let mut textures = Vec::new();
    for index in 0..count {
        let def = table?.at(index * MATERIAL_TEXTURE_DEF);
        let Ok(bytes) = load.blocks.bytes(def, MATERIAL_TEXTURE_DEF as usize) else {
            continue;
        };
        let (name_hash, sampler_state, semantic) = (header_u32(bytes, 0)?, bytes[6], bytes[7]);
        let Some(image) = load.asset_in(material, def.at(12)) else {
            continue;
        };
        let wanted = header_str(load, &image.header, IMAGE_NAME)
            .unwrap_or("")
            .trim_start_matches(',');
        let (image_load, image) = if image.header[4] != 0 {
            (load, image)
        } else {
            zones
                .iter()
                .find_map(|zone| {
                    zone.assets
                        .iter()
                        .find(|candidate| {
                            candidate.ty == AssetType::Image
                                && candidate.header[4] != 0
                                && header_str(zone, &candidate.header, IMAGE_NAME)
                                    .is_some_and(|name| name.trim_start_matches(',') == wanted)
                        })
                        .map(|image| (*zone, image))
                })
                .unwrap_or((load, image))
        };
        let image_name = header_str(image_load, &image.header, IMAGE_NAME)
            .unwrap_or("")
            .to_owned();
        let texels = match decoded.get(&image_name) {
            Some(texels) => texels.clone(),
            None => {
                let texels = decode_map_image(image_load, image, ipaks);
                match texels {
                    Ok(texels) => {
                        let texels = Arc::new(texels);
                        decoded.insert(image_name.clone(), texels.clone());
                        texels
                    }
                    Err(error) => {
                        report.push(format!("T6 material image {image_name}: {error}"));
                        continue;
                    }
                }
            }
        };
        let (image_name, texels) = if opaque && name_hash == COLOR_MAP_SAMPLER_HASH {
            let name = format!("{image_name}$opaque");
            let texels = decoded
                .entry(name.clone())
                .or_insert_with(|| Arc::new(asset_material::with_opaque_alpha(&texels)))
                .clone();
            (name, texels)
        } else {
            (image_name, texels)
        };
        textures.push(asset_material::t6_techset::T6Texture {
            name_hash,
            sampler_state: t6_sampler_state(sampler_state),
            semantic,
            image: image_name,
            texels,
        });
    }
    let constant_count = u32::from(*header.get(MATERIAL_CONSTANT_COUNT)?);
    let constant_table = decode_ptr(header_u32(header, MATERIAL_CONSTANT_TABLE)?);
    let constants = (0..constant_count)
        .filter_map(|index| {
            let bytes = load
                .blocks
                .bytes(
                    constant_table?.at(index * MATERIAL_CONSTANT_DEF),
                    MATERIAL_CONSTANT_DEF as usize,
                )
                .ok()?;
            Some(asset_material::MaterialConstant {
                name_hash: header_u32(bytes, 0)?,
                name: bytes[4..16].try_into().ok()?,
                literal: core::array::from_fn(|i| {
                    f32::from_bits(header_u32(bytes, 16 + 4 * i).unwrap_or(0))
                }),
            })
        })
        .collect();
    let constants: Vec<asset_material::MaterialConstant> = constants;
    Some(T6NativeMaterial {
        header: material.header.clone(),
        technique_set,
        textures,
        constants,
        state,
    })
}

fn read_streamed_image(
    load: &fastfile_t6::ZoneLoad,
    image: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
) -> Result<(String, Arc<[u8]>), String> {
    let name = header_str(load, &image.header, IMAGE_NAME)
        .ok_or("image without a name")?
        .to_owned();
    if image
        .header
        .get(IMAGE_STREAMED_PART_COUNT)
        .copied()
        .unwrap_or(0)
        == 0
    {
        return Err(format!("{name}: not streamed"));
    }
    let hash = header_u32(&image.header, IMAGE_HASH).ok_or("short image")?;
    let part = header_u32(&image.header, IMAGE_STREAMED_PART_HASH).ok_or("short image")?;
    let bytes = ipaks
        .iter()
        .find_map(|ipak| ipak.read_shared(hash, part))
        .ok_or_else(|| format!("{name}: in no image package"))??;
    Ok((name, bytes))
}

fn decode_map_image(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
) -> Result<Image, String> {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    if asset.header[IMAGE_STREAMED_PART_COUNT] != 0 {
        let (_, bytes) = read_streamed_image(load, asset, ipaks)?;
        if asset.header[4] == 5 {
            return asset_material::decode_iwi_cubemap_native(&bytes, false);
        }
        return asset_material::decode_iwi_texture_native(&bytes, 2, false);
    }
    let name = header_str(load, &asset.header, IMAGE_NAME).unwrap_or("");
    let source = asset
        .image_data
        .as_ref()
        .ok_or_else(|| format!("T6 image {name} has no pixel definition"))?;
    let half = |at| u16::from_le_bytes(asset.header[at..at + 2].try_into().unwrap());
    let (width, height, depth) = (half(20), half(22), half(24));
    if asset.header[4] == 5 {
        let format = match source.format {
            71 => u32::from_le_bytes(*b"DXT1"),
            77 => u32::from_le_bytes(*b"DXT5"),
            other => return Err(format!("T6 cubemap {name}: unsupported format {other}")),
        };
        return asset_material::decode_reflection_probe_cubemap(&asset_material::AuthoredImage {
            namespace: asset_core::AssetNamespace::T6,
            name: asset_core::AssetRef::Real(name.to_owned()),
            map_type: 5,
            semantic: asset.header[5],
            category: asset.header[6],
            use_srgb_reads: false,
            width,
            height,
            depth,
            level_count: source.level_count,
            format,
            payload: Arc::new(source.payload.clone()),
            decoded: None,
            common_owned: false,
            decoded_variant: None,
            decoded_by: None,
            pending_decode: None,
        });
    }
    if source.level_count != 1 || depth != 1 || width == 0 || height == 0 {
        return Err(format!("T6 image {name}: unsupported dimensions/mips"));
    }
    let (data, stride) = match source.format {
        28 => (source.payload.clone(), 4),
        65 => (
            source
                .payload
                .iter()
                .flat_map(|&alpha| [0, 0, 0, alpha])
                .collect(),
            4,
        ),
        other => return Err(format!("T6 image {name}: unsupported format {other}")),
    };
    if data.len() != usize::from(width) * usize::from(height) * stride {
        return Err(format!("T6 image {name}: pixel count mismatch"));
    }
    let mut image = Image::new(
        Extent3d {
            width: u32::from(width),
            height: u32::from(height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    Ok(image)
}

fn decode_world_image(
    load: &fastfile_t6::ZoneLoad,
    image: &fastfile_t6::LoadedAsset,
    sources: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> Result<Image, String> {
    let name = header_str(load, &image.header, IMAGE_NAME).ok_or("T6 world image has no name")?;
    if name.starts_with(',') {
        let name = name.trim_start_matches(',');
        for source in sources {
            if let Some(image) = source.assets.iter().find(|asset| {
                asset.ty == AssetType::Image
                    && header_str(source, &asset.header, IMAGE_NAME) == Some(name)
            }) {
                return decode_map_image(source, image, ipaks);
            }
        }
        return Err(format!(
            "T6 world image {name}: definition missing from map dependencies"
        ));
    }
    decode_map_image(load, image, ipaks)
}

pub struct T6UiArt {
    images: Vec<(String, asset_material::ZoneUiImage)>,
    fonts: Vec<asset_material::ui_font::UiBitmapFont>,
    modules: Vec<(String, Arc<[u8]>)>,
    tables: Vec<asset_core::ZoneTable>,
    layouts: Vec<asset_core::BitLayout>,
    localized: Vec<(String, String)>,
    installed_maps: Vec<String>,
}

impl T6UiArt {
    pub fn publish(self) -> asset_material::UiImagePublication {
        let (publication, fonts) = self.into_parts();
        store_t6_ui_fonts(fonts);
        publication
    }

    /// The published images, tables and modules, and the fonts left for the
    /// caller to store (`store_t6_ui_fonts`) when it shows this art.
    pub fn into_parts(
        self,
    ) -> (
        asset_material::UiImagePublication,
        Vec<asset_material::ui_font::UiBitmapFont>,
    ) {
        let mut images = asset_material::UiImageBuild::default();
        images.script_tables(T6_UI, self.tables);
        images.script_modules(T6_UI, self.modules);
        images.script_layouts(T6_UI, self.layouts);
        images.script_localized(T6_UI, self.localized);
        images.script_installed_maps(T6_UI, self.installed_maps);
        images.zone_images(T6_UI, self.images);
        (images.publish(), self.fonts)
    }
}

const T6_UI: asset_core::AssetNamespace = asset_core::AssetNamespace::T6;

/// Makes `fonts` the Black Ops 2 bitmap fonts the UI draws with.
pub fn store_t6_ui_fonts(fonts: Vec<asset_material::ui_font::UiBitmapFont>) {
    asset_material::ui_font::store_ui_fonts(T6_UI, fonts);
}

pub fn load_t6_ui_art(installation: &Path, zombies: bool) -> Result<T6UiArt, String> {
    let mode = if zombies {
        asset_transport::t6_content::T6ContentMode::Zombies
    } else {
        asset_transport::t6_content::T6ContentMode::Multiplayer
    };
    let common = installation
        .join("zone")
        .join("all")
        .join(format!("{}.ff", mode.common()));
    if !common.is_file() {
        return Err("T6 common zone missing".into());
    }
    let mut report = Vec::new();
    let ipaks = open_ipaks(&common, &mut report);
    let art = capture_weapon_icons(&common, &[], &ipaks, &mut report);
    for line in report {
        diag::info!(Zone, "{line}");
    }
    Ok(art)
}

fn capture_weapon_icons(
    common: &Path,
    loads: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    report: &mut Vec<String>,
) -> T6UiArt {
    let mut wanted = std::collections::BTreeSet::new();
    for load in loads {
        for asset in &load.assets {
            if let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) {
                let overlays = weapon
                    .attachment_uniques()
                    .filter_map(|unique| {
                        unique.asset_field_name(fastfile_t6::weapon::unique::OVERLAY_MATERIAL)
                    })
                    .chain(
                        weapon.variant_asset_name(fastfile_t6::weapon::variant::OVERLAY_MATERIAL),
                    );
                wanted.extend(overlays.map(str::to_ascii_lowercase));
                for slot in [
                    fastfile_t6::weapon::def::RETICLE_CENTER,
                    fastfile_t6::weapon::def::RETICLE_SIDE,
                ] {
                    if let Some(reticle) = weapon.def_loaded_asset_name(slot) {
                        wanted.insert(asset_game::t6_model_name(reticle).to_ascii_lowercase());
                    }
                }
                if let Some(icon) = weapon.def_loaded_asset_name(fastfile_t6::weapon::def::HUD_ICON)
                {
                    wanted.insert(asset_game::t6_model_name(icon).to_ascii_lowercase());
                }
            }
            let Some(table) = asset_game::capture_t6_string_table(load, asset) else {
                continue;
            };
            let attachments = table.name.eq_ignore_ascii_case("mp/attachmentTable.csv");
            if !attachments && !asset_game::is_stats_table_name(&table.name) {
                continue;
            }
            for row in 0..table.rows {
                let cell = |column: usize| {
                    table
                        .cells
                        .get(row * table.columns + column)
                        .map_or("", String::as_str)
                };
                if (attachments || cell(2).starts_with("weapon_")) && !cell(6).is_empty() {
                    wanted.insert(cell(6).to_ascii_lowercase());
                }
            }
        }
    }
    let mode = asset_transport::t6_content::T6ContentMode::for_path(common);
    let mut ui_zones: Vec<_> = mode
        .icons()
        .iter()
        .map(|name| common.with_file_name(name))
        .collect();
    ui_zones.extend(mode.load_zones(common));
    match asset_transport::discover::find_t6_localized_zones(common, None) {
        Ok(zones) => ui_zones.extend(zones.into_iter().map(|zone| zone.path)),
        Err(error) => report.push(format!("t6 localized UI: {error}")),
    }
    let extra: Vec<_> = ui_zones
        .iter()
        .filter_map(|path| {
            let image = match asset_transport::open_t6_zone(path) {
                Ok(image) => image,
                Err(error) => {
                    report.push(format!("t6 icons: {}: {error:?}", path.display()));
                    return None;
                }
            };
            let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
            if let Err(error) = walked {
                report.push(format!("t6 icons: {} walk: {error:?}", path.display()));
            }
            Some(load)
        })
        .collect();
    let mut icons = Vec::new();
    let mut failed = Vec::new();
    let all: Vec<&fastfile_t6::ZoneLoad> = extra.iter().chain(loads.iter().copied()).collect();
    let mut seen_fonts = std::collections::BTreeSet::new();
    let fonts: Vec<_> = all
        .iter()
        .flat_map(|load| {
            load.assets
                .iter()
                .filter_map(|asset| capture_ui_font(load, asset))
        })
        .filter(|font| seen_fonts.insert(font.name.to_ascii_lowercase()))
        .collect();
    let mut ui_strings = std::collections::BTreeSet::new();
    let mut material_names = std::collections::BTreeSet::new();
    let mut ui_modules = 0usize;
    let mut modules = BTreeMap::new();
    let mut tables = BTreeMap::new();
    let mut layouts: Vec<asset_core::BitLayout> = Vec::new();
    let mut localized = BTreeMap::new();
    for load in &all {
        for asset in &load.assets {
            if asset.ty == AssetType::LocalizeEntry
                && let (Some(value), Some(name)) = (
                    header_str(load, &asset.header, 0),
                    header_str(load, &asset.header, 4),
                )
            {
                localized
                    .entry(name.to_owned())
                    .or_insert_with(|| value.to_owned());
            }
            if let Some(ddl) = parse_ddl(load, asset)
                && let Some(def) = ddl.versions.into_iter().next()
                && !layouts
                    .iter()
                    .any(|l| l.name.eq_ignore_ascii_case(&ddl.name))
            {
                layouts.push(asset_core::BitLayout {
                    name: ddl.name,
                    version: def.version,
                    bits: def.structs.first().map_or(0, |r| r.size),
                    structs: def
                        .structs
                        .into_iter()
                        .map(|s| asset_core::BitStruct {
                            name: s.name,
                            members: s
                                .members
                                .into_iter()
                                .map(|m| asset_core::BitMember {
                                    name: m.name,
                                    bits: m.bits,
                                    offset: m.offset,
                                    kind: m.kind,
                                    external: m.external,
                                    range: m.range,
                                    array_size: m.array_size,
                                    enum_index: m.enum_index,
                                    min: 0,
                                    max: 0,
                                    permission: 0,
                                })
                                .collect(),
                        })
                        .collect(),
                    enums: def.enums.into_iter().map(|e| e.members).collect(),
                });
            }
            if let Some(table) = fastfile_t6::string_table(load, asset) {
                if table.name.eq_ignore_ascii_case("mp/rankicontable.csv") && table.columns > 1 {
                    for row in table.cells.chunks_exact(table.columns) {
                        wanted.extend(
                            row.iter()
                                .skip(1)
                                .filter(|cell| !cell.is_empty())
                                .map(|cell| cell.to_ascii_lowercase()),
                        );
                    }
                }
                tables
                    .entry(table.name.to_ascii_lowercase())
                    .or_insert_with(|| asset_core::ZoneTable {
                        name: table.name.to_owned(),
                        columns: table.columns,
                        rows: table.rows,
                        cells: table.cells.into_iter().map(str::to_owned).collect(),
                    });
            }
            if let Ok(Some(raw)) = fastfile_t6::read_raw_file(load, asset)
                && (((raw.name.starts_with("ui/") || raw.name.starts_with("ui_mp/"))
                    && raw.name.ends_with(".lua"))
                    || raw.name.ends_with(".cfg"))
            {
                modules
                    .entry(raw.name.to_ascii_lowercase())
                    .or_insert_with(|| Arc::from(raw.bytes));
            }
        }
        let literals = load.ui_literals();
        ui_modules += literals.modules;
        ui_strings.extend(literals.strings);
        material_names.extend(literals.materials);
        report.extend(
            literals
                .errors
                .into_iter()
                .map(|error| format!("t6 UI literal capture refused: {error:?}")),
        );
    }
    let ui_materials: std::collections::BTreeSet<_> =
        material_names.intersection(&ui_strings).cloned().collect();
    report.push(format!(
        "t6 UI: {ui_modules} bytecode modules decoded; {} material names matched",
        ui_materials.len()
    ));
    wanted.extend(ui_materials);
    wanted.extend(fonts.iter().map(|font| font.material.clone()));
    report.push(format!(
        "t6 UI fonts: {} native bitmap fonts captured",
        fonts.len()
    ));
    for load in loads.iter().copied().chain(&extra) {
        for asset in &load.assets {
            if asset.ty != AssetType::Material {
                continue;
            }
            // `,name` is a header-only reference; the zone defining it is read too.
            let Some(name) = header_str(load, &asset.header, 0)
                .filter(|name| !name.starts_with(','))
                .map(str::to_ascii_lowercase)
            else {
                continue;
            };
            let presentation = name.starts_with("loadscreen_mp_")
                || name.starts_with("loadscreen_zm_")
                || name.starts_with("faction_")
                || name.starts_with("hud_chalk_")
                || name.starts_with("menu_")
                || name.starts_with("ui_")
                || name.starts_with("lui_")
                || name.starts_with("controller_")
                || (name.starts_with("specialty_") && name.contains("_zombie"))
                || matches!(
                    name.as_str(),
                    "scorebar_zom_1"
                        | "zom_hud_craftable_tank_shovel"
                        | "zom_hud_shovel_gold"
                        | "scorebar_fadein"
                        | "overlay_low_health"
                        | "waypoint_revive_zm"
                        | "progress_bar_bg"
                        | "progress_bar_fg"
                        | "progress_bar_fill"
                        | "hud_mp_vis_left_lower_back"
                        | "hud_faction_backing"
                        | "hud_faction_back_light"
                        | "compassping_player"
                );
            if (!wanted.contains(&name) && !presentation)
                || icons.iter().any(|(seen, _)| seen == &name)
            {
                continue;
            }
            match capture_icon(load, asset, &all, ipaks) {
                Ok(iwi) => icons.push((
                    name,
                    asset_material::ZoneUiImage {
                        iwi,
                        state: capture_ui_state(load, asset),
                        rgba: None,
                        sampler: None,
                        scanline: None,
                    },
                )),
                Err(error) => failed.push(format!("{name}: {error}")),
            }
        }

        if wanted.is_empty() {
            continue;
        }
        for asset in &load.assets {
            if asset.ty != AssetType::Image {
                continue;
            }
            let Some(name) = header_str(load, &asset.header, IMAGE_NAME)
                .map(str::to_ascii_lowercase)
                .filter(|name| wanted.contains(name))
            else {
                continue;
            };
            let decoded =
                read_streamed_image(load, asset, ipaks).and_then(|(_, bytes)| checked_iwi(bytes));
            match decoded {
                Ok(iwi) => {
                    wanted.remove(&name);
                    icons.push((
                        name,
                        asset_material::ZoneUiImage {
                            iwi,
                            state: None,
                            rgba: None,
                            sampler: None,
                            scanline: None,
                        },
                    ));
                }
                Err(error) => failed.push(format!("{name}: {error}")),
            }
        }
    }
    report.push(format!(
        "t6 icons: {} UI images decoded; {} failed {failed:?}; {} in no zone read {wanted:?}",
        icons.len(),
        failed.len(),
        wanted.len()
    ));
    let installed_maps = common
        .parent()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| {
            n.starts_with("zm_")
                && n.ends_with(".ff")
                && !n.contains("patch")
                && !n.contains("gump")
        })
        .map(|n| n.trim_end_matches(".ff").to_owned())
        .collect();
    T6UiArt {
        images: icons,
        fonts,
        modules: modules.into_iter().collect(),
        tables: tables.into_values().collect(),
        layouts,
        localized: localized.into_iter().collect(),
        installed_maps,
    }
}

fn capture_ui_font(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
) -> Option<asset_material::ui_font::UiBitmapFont> {
    if asset.ty != AssetType::Font {
        return None;
    }
    let name = header_str(load, &asset.header, 0)?.to_owned();
    let pixel_height = header_u32(&asset.header, 4)?.clamp(1, 256);
    let count = header_u32(&asset.header, 12)? as usize;
    if count == 0 || count > 65536 {
        return None;
    }
    let material = load.assets.get(asset.field(20)?)?;
    let material = header_str(load, &material.header, 0)?
        .trim_start_matches(',')
        .to_ascii_lowercase();
    let bytes = load
        .blocks
        .bytes(
            decode_ptr(header_u32(&asset.header, 28)?)?,
            count.checked_mul(24)?,
        )
        .ok()?;
    let glyphs = bytes
        .as_chunks::<24>()
        .0
        .iter()
        .filter_map(|row| {
            let glyph = fastfile_iw4::GlyphCapture::from_row(row);
            [glyph.s0, glyph.t0, glyph.s1, glyph.t1]
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .then_some((glyph.letter, glyph))
        })
        .collect();
    Some(asset_material::ui_font::UiBitmapFont {
        name,
        material,
        pixel_height,
        glyphs,
    })
}

fn capture_ui_state(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
) -> Option<render_material::CompiledPassState> {
    let header = &material.header;
    let row = *header.get(MATERIAL_STATE_BITS_ENTRY + 2)?;
    if row == u8::MAX || row >= *header.get(86)? {
        return None;
    }
    let table = decode_ptr(header_u32(header, MATERIAL_STATE_BITS_TABLE)?)?;
    let bytes = load
        .blocks
        .bytes(table.at(u32::from(row) * MATERIAL_STATE_BITS), 8)
        .ok()?;
    let state = asset_material::compile_material_state(
        asset_core::FamilyId::T6,
        [header_u32(bytes, 0)?, header_u32(bytes, 4)?],
    )?;
    state.unsupported_host_fields().is_none().then_some(state)
}

fn capture_icon(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> Result<Arc<[u8]>, String> {
    let header = &material.header;
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT).ok_or("short material")?);
    let raw = header_u32(header, MATERIAL_TEXTURE_TABLE).ok_or("short material")?;
    if raw == 0 {
        return Err("no textures".to_owned());
    }
    let e = raw - 1;
    let table = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    for index in 0..count {
        let def = table.at(index * MATERIAL_TEXTURE_DEF);
        if !(matches!(
            load.blocks.u32_at(def).ok(),
            Some(COLOR_MAP_HASH | FONT_MAP_HASH)
        ) || (header_str(load, header, 0) == Some("ui_globe_zm") && index == 0))
        {
            continue;
        }
        let image = load
            .asset_in(material, def.at(12))
            .ok_or("colour map without an image")?;
        let (name, bytes) = match read_streamed_image(load, image, ipaks) {
            Ok(read) => read,
            Err(error) => header_str(load, &image.header, IMAGE_NAME)
                .and_then(|name| name.strip_prefix(','))
                .and_then(|name| {
                    zones.iter().find_map(|zone| {
                        zone.assets
                            .iter()
                            .filter(|asset| asset.ty == AssetType::Image)
                            .filter(|asset| {
                                header_str(zone, &asset.header, IMAGE_NAME) == Some(name)
                            })
                            .find_map(|asset| read_streamed_image(zone, asset, ipaks).ok())
                    })
                })
                .ok_or(error)?,
        };
        return checked_iwi(bytes).map_err(|e| format!("{name}: {e}"));
    }
    Err("no colour map".to_owned())
}

fn checked_iwi(bytes: Arc<[u8]>) -> Result<Arc<[u8]>, String> {
    asset_material::decode_iwi_rgba(&bytes)?;
    Ok(bytes)
}

fn attachment_rest(offset: [f32; 3], angles: [f32; 3]) -> (bevy::math::Quat, bevy::math::Vec3) {
    let [pitch, yaw, roll] = angles.map(f32::to_radians);
    (
        bevy::math::Quat::from_euler(bevy::math::EulerRot::ZYX, yaw, pitch, roll),
        bevy::math::Vec3::from_array(offset),
    )
}

fn capture_content(
    path: &Path,
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> T6Content {
    use fastfile_t6::weapon::{WeaponView, def};
    let mut content = T6Content {
        path: path.to_owned(),
        ..Default::default()
    };
    let zones: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load).chain(others).collect();
    let mut models: BTreeMap<&str, (&fastfile_t6::ZoneLoad, &fastfile_t6::LoadedAsset)> =
        BTreeMap::new();
    for load in std::iter::once(load).chain(others) {
        for asset in &load.assets {
            if let Some(name) = asset_model::T6Model::new(load, asset).and_then(|m| m.name())
                && !name.starts_with(',')
            {
                models.entry(name).or_insert((load, asset));
            }
        }
    }
    capture_effects(&zones, ipaks, &mut content);
    let mut wanted: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    // One weapon's view model is another's world model (`tag_origin`).
    let mut both: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut placements: BTreeMap<String, ([f32; 3], [f32; 3])> = BTreeMap::new();
    let mut copies: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    // The weapon catalog reads every zone's weapons (Classic's equipment is
    // in its own zone), so their models are wanted from every zone too.
    for (zone, asset) in zones
        .iter()
        .flat_map(|zone| zone.assets.iter().map(move |asset| (*zone, asset)))
    {
        let Some(weapon) = WeaponView::new(zone, asset) else {
            continue;
        };
        if weapon.name().is_none() {
            continue;
        }

        for (name, view) in [
            (weapon.def_asset_array_name(def::GUN_XMODEL, 0), true),
            (weapon.def_asset_array_name(def::WORLD_MODEL, 0), false),
            (weapon.def_asset_name(def::PROJECTILE_MODEL), false),
            (weapon.name().and_then(asset_game::t6_planted_model), false),
        ] {
            if let Some(name) = name {
                let name = asset_game::t6_model_name(name);
                match wanted.get(&name) {
                    Some(&(seen, _)) if seen != view => {
                        both.insert(name);
                    }
                    Some(_) => {}
                    None => {
                        wanted.insert(name, (view, false));
                    }
                }
            }
        }
        for unique in weapon.attachment_uniques() {
            let placed = [true, false]
                .into_iter()
                .flat_map(|view| {
                    asset_game::t6_attachment_models(unique, view)
                        .into_iter()
                        .map(move |placed| (placed, view))
                })
                .chain(asset_game::t6_attachment_ads_model(unique).map(|placed| (placed, true)));
            for (placed, view) in placed {
                placements.insert(placed.copy.clone(), (placed.offset, placed.angles));
                wanted.entry(placed.copy.clone()).or_insert((view, false));
                copies.insert(placed.copy, (placed.model, placed.tag));
            }
        }
        for view in [true, false] {
            for slot in 0..fastfile_t6::weapon::variant::ATTACH_MODEL_COUNT {
                if let Some((name, offset, angles)) = weapon.attached_model(slot, view) {
                    let name = asset_game::t6_model_name(name);
                    placements.entry(name.clone()).or_insert((offset, angles));
                    wanted.entry(name).or_insert((view, false));
                }
            }
        }
    }
    for effect in &content.fx {
        for elem in &effect.elems {
            if elem.raw.get(FX_ELEM_TYPE) == Some(&FX_ELEM_MODEL) {
                for name in &elem.visuals {
                    if !name.is_empty() {
                        wanted.entry(name.clone()).or_insert((false, false));
                    }
                }
            }
        }
    }
    for allies in ["seals", "fbi", "isa"] {
        for axis in ["pla", "pmc", "cd"] {
            for name in asset_game::ObjectiveVisuals::t6(Some(allies), Some(axis)).model_names() {
                if models.contains_key(name) {
                    wanted.entry(name.to_owned()).or_insert((false, false));
                }
            }
        }
    }
    if !wanted.is_empty() {
        let hands = if asset_transport::t6_content::T6ContentMode::for_path(path)
            == asset_transport::t6_content::T6ContentMode::Zombies
        {
            models
                .keys()
                .copied()
                .find(|name| name.starts_with("c_zom_") && name.contains("viewhands"))
        } else {
            Some(HANDS_MODEL)
        };
        if let Some(hands) = hands {
            wanted.insert(hands.to_owned(), (true, true));
        }
        if let Some(knife) = melee_weapon(load).map(|melee| melee.knife) {
            wanted.entry(knife).or_insert((true, false));
        }
    }
    content.melee = melee_weapon(load);
    let (mut failed, mut decoded, mut missing) = (0usize, 0usize, 0usize);
    let mut native_textures = DecodedTextures::new();
    let sources: Vec<String> = wanted
        .keys()
        .map(|name| {
            copies
                .get(name)
                .map_or(name.as_str(), |(model, _)| model.as_str())
                .to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let jobs: Vec<_> = sources
        .iter()
        .map(|source| {
            let &(load, asset) = models.get(source.as_str())?;
            asset_model::T6Model::new(load, asset)
        })
        .collect();
    let pool = crate::session_load::load_pool();
    let chunk_size = jobs.len().div_ceil(pool.thread_num().max(1) * 16).max(1);
    let mut captured = pool.scope(|scope| {
        for (index, chunk) in jobs.chunks(chunk_size).enumerate() {
            scope.spawn(async move {
                let skels: Vec<_> = chunk
                    .iter()
                    .map(|model| {
                        model.and_then(|model| asset_model::capture_model_skel_t6(model, |_| None))
                    })
                    .collect();
                (index, skels)
            });
        }
    });
    // Complete pure captures in parallel, then retain the input mutation
    // and refusal order regardless of which task finished first.
    captured.sort_unstable_by_key(|(index, _)| *index);
    let geometry: BTreeMap<String, _> = sources
        .into_iter()
        .zip(captured.into_iter().flat_map(|(_, skels)| skels))
        .collect();
    for (name, (view, hands)) in wanted {
        let copy = copies.get(&name);
        let source = copy.map_or(name.as_str(), |(model, _)| model.as_str());
        let Some(&(load, asset)) = models.get(source) else {
            content
                .report
                .push(format!("t6 content: {name}: in no zone read"));
            continue;
        };
        let Some(model) = asset_model::T6Model::new(load, asset) else {
            continue;
        };
        let Some(mut skel) = geometry.get(source).cloned().flatten() else {
            failed += 1;
            content
                .report
                .push(format!("t6 content: {name}: model capture failed"));
            continue;
        };
        if let Some(&(offset, angles)) = placements.get(&name)
            && let Some(pose) = skel.pose.as_mut()
        {
            pose.root_rest = Some(attachment_rest(offset, angles));
        }
        if let Some((_, tag)) = copy {
            skel.name = name.clone();
            skel.mount_tag = tag.clone();
        }
        let mut surface_materials = Vec::with_capacity(model.surface_count());
        for surface in 0..model.surface_count() {
            let material = model
                .material_slot(surface)
                .and_then(|slot| load.asset_in(asset, slot));
            let material_name = material
                .and_then(|m| header_str(load, &m.header, 0))
                .map(str::to_owned);
            if let (Some(material), Some(material_name)) = (material, &material_name)
                && !content.materials.contains_key(material_name)
            {
                let reported = content.report.len();
                let native = model
                    .material_slot(surface)
                    .and_then(|slot| load.blocks.ptr_at(slot).ok().flatten())
                    .and_then(|address| {
                        capture_native(
                            load,
                            &zones,
                            Some(address),
                            material,
                            if hands {
                                ColourMapAlpha::Mask
                            } else {
                                ColourMapAlpha::Gloss
                            },
                            ipaks,
                            &mut native_textures,
                            &mut content.techsets,
                            &mut content.report,
                        )
                    });
                missing += content.report.len() - reported;
                decoded += native.as_ref().map_or(0, |capture| capture.textures.len());
                content
                    .materials
                    .insert(material_name.clone(), T6MaterialCapture { native });
            }
            surface_materials.push(material_name);
        }
        if hands {
            content.hands = Some(name.clone());
        }
        content.models.push(T6ModelCapture {
            skel,
            surface_materials,
            view: view || both.contains(&name),
            world: !view || both.contains(&name),
        });
    }
    camouflage::capture(load, &zones, ipaks, &mut native_textures, &mut content);
    content.report.push(format!(
        "t6 content: {} models ({failed} failed), {} materials, {decoded} native textures decoded, {missing} missing; {} image packages",
        content.models.len(),
        content.materials.len(),
        ipaks.len()
    ));
    content
}

const FX_ELEM_DEF: u32 = 292;
const FX_ELEM_TYPE: usize = 184;
const FX_ELEM_VEL_SAMPLES: usize = 188;
const FX_ELEM_VIS_SAMPLES: usize = 192;
const FX_ELEM_VISUALS: u32 = 196;
const FX_ELEM_CHILDREN: [usize; 3] = [224, 228, 232];
const FX_ELEM_VEL_SAMPLE: usize = 96;
const FX_ELEM_VIS_SAMPLE: usize = 48;
const FX_EFFECT_DEF_ELEMS: usize = 28;
const FX_ELEM_LAST_SPRITE: u8 = 6;
const FX_ELEM_MODEL: u8 = 7;
const FX_ELEM_SOUND: u8 = 10;
const FX_ELEM_DECAL: u8 = 11;
const FX_ELEM_RUNNER: u8 = 12;

fn effect_name_at(load: &fastfile_t6::ZoneLoad, slot: fastfile_t6::Ptr) -> Option<&str> {
    let ptr = load.blocks.ptr_at(slot).ok()??;
    std::str::from_utf8(load.blocks.cstr(ptr).ok()?)
        .ok()
        .map(|name| name.strip_prefix(',').unwrap_or(name))
        .filter(|name| !name.is_empty())
}

fn capture_tracers(
    zones: &[&ZoneLoad],
    zone_path: &Path,
    namespace: asset_core::AssetNamespace,
) -> asset_game::TracerCatalog {
    let mut catalog = asset_game::TracerCatalog::default();
    catalog.set_capture_zone(asset_core::ZoneOwner::from_zone_path(zone_path));
    catalog.set_capture_ns(namespace);
    for load in zones {
        for asset in &load.assets {
            if asset.ty != AssetType::Tracer || asset.header.len() < 128 {
                continue;
            }
            let Some(raw_name) = header_str(load, &asset.header, 0) else {
                continue;
            };
            if raw_name.starts_with(',') || raw_name.is_empty() {
                continue;
            }
            let name = asset_game::t6_model_name(raw_name);
            let Some(slot) = decode_ptr(0xA000_0009) else {
                continue;
            };
            let material_hint = load
                .asset_in(asset, slot)
                .filter(|m| m.ty == AssetType::Material)
                .and_then(|m| header_str(load, &m.header, 0))
                .map(asset_game::t6_model_name);
            let f = |at: usize| f32::from_bits(header_u32(&asset.header, at).unwrap_or(0));
            let colors =
                std::array::from_fn(|row| std::array::from_fn(|col| f(48 + row * 16 + col * 4)));
            catalog.capture_t6(asset_game::OwnedTracerDef {
                namespace,
                name,
                material: if material_hint.is_some() {
                    asset_core::AssetEdge::Unresolved(asset_core::AssetEdgeReason::CatalogMiss)
                } else {
                    asset_core::AssetEdge::Absent
                },
                material_authored: asset_game::AuthoredRef {
                    slot: material_hint.is_some(),
                    alias: false,
                },
                material_hint,
                material_namespace: namespace,
                draw_interval: header_u32(&asset.header, 12).unwrap_or(1),
                speed: f(16),
                beam_length: f(20),
                beam_width: f(24),
                screw_radius: f(28),
                screw_dist: f(32),
                colors,
            });
        }
    }
    catalog
}

fn capture_impact_table(
    zones: &[&fastfile_t6::ZoneLoad],
) -> Option<asset_game::OwnedFxImpactTable> {
    for load in zones {
        for asset in &load.assets {
            if asset.ty != AssetType::ImpactFx {
                continue;
            }
            let table = decode_ptr(header_u32(&asset.header, 4)?)?;
            let mut entries = Vec::new();
            for row in 0..21 {
                let at = table.at(row * 144);
                let mut entry = asset_game::OwnedFxImpactEntry::default();
                for (i, name) in entry.nonflesh.iter_mut().enumerate() {
                    *name = load
                        .asset_in(asset, at.at(i as u32 * 4))
                        .filter(|fx| fx.ty == AssetType::Fx)
                        .and_then(|fx| header_str(load, &fx.header, 0))
                        .map(asset_game::t6_model_name)
                        .unwrap_or_else(String::new);
                }
                for (i, name) in entry.flesh.iter_mut().enumerate() {
                    *name = load
                        .asset_in(asset, at.at(128 + i as u32 * 4))
                        .filter(|fx| fx.ty == AssetType::Fx)
                        .and_then(|fx| header_str(load, &fx.header, 0))
                        .map(asset_game::t6_model_name)
                        .unwrap_or_else(String::new);
                }
                entries.push(entry);
            }
            return Some(asset_game::OwnedFxImpactTable {
                family: Some(asset_core::FamilyId::T6),
                name: header_str(load, &asset.header, 0)?.to_owned(),
                entries,
                capture_gaps: 0,
            });
        }
    }
    None
}

fn capture_effects(
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    content: &mut T6Content,
) {
    let mut wanted: std::collections::BTreeSet<String> = asset_game::T6_EFFECTS
        .iter()
        .map(|&name| name.to_owned())
        .collect();
    use fastfile_t6::weapon::def;
    for load in zones {
        for asset in &load.assets {
            if let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) {
                for field in [
                    def::VIEW_FLASH_EFFECT,
                    def::WORLD_FLASH_EFFECT,
                    def::VIEW_SHELL_EJECT_EFFECT,
                    def::WORLD_SHELL_EJECT_EFFECT,
                    def::VIEW_LAST_SHOT_EJECT_EFFECT,
                    def::WORLD_LAST_SHOT_EJECT_EFFECT,
                    def::PROJ_EXPLOSION_EFFECT,
                ] {
                    wanted.extend(
                        weapon
                            .def_loaded_asset_name(field)
                            .map(asset_game::t6_model_name),
                    );
                }
            }
        }
    }
    if let Some(table) = capture_impact_table(zones) {
        for entry in table.entries {
            wanted.extend(
                entry
                    .nonflesh
                    .into_iter()
                    .chain(entry.flesh)
                    .filter(|name| !name.is_empty()),
            );
        }
    }
    let mut textures = DecodedTextures::new();
    for &load in zones {
        for asset in &load.assets {
            if asset.ty != AssetType::Tracer {
                continue;
            }
            let Some(slot) = decode_ptr(0xA000_0009) else {
                continue;
            };
            let material = load.asset_in(asset, slot);
            let name = material
                .and_then(|m| header_str(load, &m.header, 0))
                .map(asset_game::t6_model_name);
            if let (Some(material), Some(name)) = (material, name)
                && !content.fx_materials.contains_key(&name)
            {
                let native = load.blocks.ptr_at(slot).ok().flatten().and_then(|address| {
                    capture_native(
                        load,
                        zones,
                        Some(address),
                        material,
                        ColourMapAlpha::Mask,
                        ipaks,
                        &mut textures,
                        &mut content.techsets,
                        &mut content.report,
                    )
                });
                content
                    .fx_materials
                    .insert(name, T6MaterialCapture { native });
            }
        }
    }
    wanted.insert("maps/mp_maps/fx_mp_exp_bomb".to_owned());
    for load in zones {
        match destructible::effect_names(load) {
            Ok(names) => wanted.extend(names),
            Err(error) => content.report.push(error),
        }
    }
    loop {
        let before = content.fx.len();
        for &load in zones {
            for asset in &load.assets {
                if asset.ty != AssetType::Fx {
                    continue;
                }
                let Some(name) = header_str(load, &asset.header, 0) else {
                    continue;
                };
                if name.starts_with(',')
                    || !wanted.contains(name)
                    || content.fx.iter().any(|fx| fx.name == name)
                {
                    continue;
                }
                match capture_effect(load, zones, asset, name, ipaks, &mut textures, content) {
                    Some(fx) => content.fx.push(fx),
                    None => content
                        .report
                        .push(format!("t6 effects: {name}: unreadable")),
                }
            }
        }
        for fx in &content.fx[before..] {
            for elem in &fx.elems {
                if elem.raw[FX_ELEM_TYPE] == FX_ELEM_RUNNER {
                    wanted.extend(elem.visuals.iter().filter(|v| !v.is_empty()).cloned());
                }
                for child in [
                    &elem.effect_on_impact,
                    &elem.effect_on_death,
                    &elem.effect_emitted,
                ] {
                    if !child.is_empty() {
                        wanted.insert(child.clone());
                    }
                }
            }
        }
        if content.fx.len() == before {
            break;
        }
    }
    let missing: Vec<_> = wanted
        .iter()
        .filter(|name| !content.fx.iter().any(|fx| fx.name == **name))
        .collect();
    content.report.push(format!(
        "t6 effects: {} captured, {} materials; in no zone read: {missing:?}",
        content.fx.len(),
        content.fx_materials.len()
    ));
}

fn capture_effect(
    load: &fastfile_t6::ZoneLoad,
    zones: &[&fastfile_t6::ZoneLoad],
    asset: &fastfile_t6::LoadedAsset,
    name: &str,
    ipaks: &[asset_transport::IPak],
    textures: &mut DecodedTextures,
    content: &mut T6Content,
) -> Option<asset_game::T6FxCapture> {
    let header = &asset.header;
    let count = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]) as u32;
    let elem_count = count(8) + count(10) + count(12);
    let mut fx = asset_game::T6FxCapture {
        name: name.to_owned(),
        header: header.clone(),
        elems: Vec::new(),
    };
    if elem_count == 0 {
        return Some(fx);
    }
    let elems = decode_ptr(header_u32(header, FX_EFFECT_DEF_ELEMS)?)?;
    let blocks = &load.blocks;
    for i in 0..elem_count {
        let at = elems.at(i * FX_ELEM_DEF);
        let raw = blocks.bytes(at, FX_ELEM_DEF as usize).ok()?.to_vec();
        let elem_type = raw[FX_ELEM_TYPE];
        let visual_count = usize::from(raw[FX_ELEM_TYPE + 1]);
        let samples = |field: usize, n: usize, stride: usize| -> Vec<u8> {
            decode_ptr(header_u32(&raw, field).unwrap_or(0))
                .and_then(|p| blocks.bytes(p, n * stride).ok())
                .map_or_else(Vec::new, <[u8]>::to_vec)
        };
        let vel_samples = samples(
            FX_ELEM_VEL_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 2]) + 1,
            FX_ELEM_VEL_SAMPLE,
        );
        let vis_samples = samples(
            FX_ELEM_VIS_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 3]) + 1,
            FX_ELEM_VIS_SAMPLE,
        );
        let slots: Vec<fastfile_t6::Ptr> = if elem_type == FX_ELEM_DECAL {
            decode_ptr(header_u32(&raw, FX_ELEM_VISUALS as usize).unwrap_or(0)).map_or_else(
                Vec::new,
                |arr| {
                    (0..visual_count as u32 * 2)
                        .map(|v| arr.at(v * 4))
                        .collect()
                },
            )
        } else if visual_count > 1 {
            decode_ptr(header_u32(&raw, FX_ELEM_VISUALS as usize).unwrap_or(0))
                .map_or_else(Vec::new, |arr| {
                    (0..visual_count as u32).map(|v| arr.at(v * 4)).collect()
                })
        } else if visual_count == 1 {
            vec![at.at(FX_ELEM_VISUALS)]
        } else {
            Vec::new()
        };
        let mut visuals = Vec::new();
        for slot in slots {
            let visual = match elem_type {
                0..=FX_ELEM_LAST_SPRITE | FX_ELEM_DECAL => {
                    let material = load.asset_in(asset, slot);
                    let name = material.and_then(|m| header_str(load, &m.header, 0));
                    if let (Some(material), Some(name)) = (material, name)
                        && !content.fx_materials.contains_key(name)
                    {
                        let native = load.blocks.ptr_at(slot).ok().flatten().and_then(|address| {
                            capture_native(
                                load,
                                zones,
                                Some(address),
                                material,
                                ColourMapAlpha::Mask,
                                ipaks,
                                textures,
                                &mut content.techsets,
                                &mut content.report,
                            )
                        });
                        content
                            .fx_materials
                            .insert(name.to_owned(), T6MaterialCapture { native });
                    }
                    name.map(str::to_owned)
                }
                FX_ELEM_MODEL => load
                    .asset_in(asset, slot)
                    .and_then(|m| header_str(load, &m.header, 0))
                    .map(asset_game::t6_model_name),
                FX_ELEM_SOUND => blocks
                    .bytes(slot, 4)
                    .ok()
                    .and_then(|b| header_str(load, b, 0))
                    .map(str::to_owned),
                FX_ELEM_RUNNER => effect_name_at(load, slot).map(str::to_owned),
                _ => None,
            };
            visuals.push(visual.unwrap_or_else(String::new));
        }
        let child = |field: usize| {
            effect_name_at(load, at.at(field as u32)).map_or_else(String::new, str::to_owned)
        };
        fx.elems.push(asset_game::T6FxElemCapture {
            vel_samples,
            vis_samples,
            visuals,
            effect_on_impact: child(FX_ELEM_CHILDREN[0]),
            effect_on_death: child(FX_ELEM_CHILDREN[1]),
            effect_emitted: child(FX_ELEM_CHILDREN[2]),
            raw,
        });
    }
    Some(fx)
}

fn native_material_seed(
    path: &Path,
    name: &str,
    material: &fastfile_t6::LoadedAsset,
    technique_set: &str,
    is_sky: bool,
    materials: &mut asset_material::MaterialCatalog,
) -> Result<usize, String> {
    let seed = materials.link_material(native_material_definition(
        path,
        name,
        &material.header,
        technique_set,
        is_sky,
    )?);
    Ok(seed)
}

fn native_material_definition(
    path: &Path,
    name: &str,
    h: &[u8],
    technique_set: &str,
    is_sky: bool,
) -> Result<asset_material::AuthoredMaterial, String> {
    use asset_world::world_t6::Reader;
    if h.len() < 89 {
        return Err(format!("T6 material {name}: truncated header"));
    }
    Ok(asset_material::AuthoredMaterial {
        name: asset_core::AssetRef::Real(name.to_owned()),
        namespace: asset_core::AssetNamespace::T6,
        technique_set: asset_core::AssetRef::Real(
            asset_material::t6_techset::T6Draw::Lit.technique_set_name(technique_set),
        ),
        technique_set_edge: Default::default(),
        draw_surf: u64::from_le_bytes(h[16..24].try_into().unwrap()),
        sort_key: if is_sky {
            asset_iw4::SORT_KEY_SKYBOX
        } else {
            h[9]
        },
        info_game_flags: h[4],
        texture_atlas: Some([h[10], h[11]]),
        surface_type_bits: Some(Reader::word(h, 24)?),
        t5_layered_surface_types: Some(Reader::word(h, 28)?),
        state_flags: h[87],
        camera_region: if is_sky {
            asset_iw4::CAMERA_REGION_LIT_OPAQUE
        } else {
            camera_region(h[88])
        },
        state_bits: Vec::new(),
        state_bits_entry: None,
        t5_state_bits_entry: None,
        iw5_state_bits_entry: None,
        technique_table: None,
        route: None,
        textures: Vec::new(),
        constants: Vec::new(),
        zone: asset_core::ZoneOwner::from_zone_path(path),
    })
}

fn camera_region(t6: u8) -> u8 {
    match t6 {
        0 | 2 | 6 => asset_iw4::CAMERA_REGION_LIT_OPAQUE,
        1 => asset_iw4::CAMERA_REGION_LIT_TRANS,
        3..=5 => asset_iw4::CAMERA_REGION_EMISSIVE,
        7 => asset_iw4::CAMERA_REGION_DEPTH_HACK,
        _ => asset_iw4::CAMERA_REGION_NONE,
    }
}

fn capture_soldiers(
    path: &Path,
    map: &ZoneLoad,
    factions: &[fastfile_t6::ZoneLoad],
    shared: &[fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    kits: asset_model::SoldierKits,
    materials: &mut asset_material::MaterialCatalog,
    report: &mut Vec<String>,
) -> Result<(asset_model::BodyMeshBuild, asset_model::FpvMeshBuild), String> {
    use asset_core::{AssetNamespace, WalkLocalMaterialIndex};
    use asset_material::t6_techset::T6Draw;
    let mut bodies = asset_model::BodyMeshBuild::default();
    let mut fpv = asset_model::FpvMeshBuild::default();
    let zones: Vec<_> = std::iter::once(map).chain(factions).chain(shared).collect();
    let mut decoded = DecodedTextures::new();
    let mut techsets = BTreeMap::new();
    let mut bound = BTreeMap::new();
    for kit in kits.all() {
        for (name, hands) in
            std::iter::once((&kit.body, false)).chain(kit.arms.as_ref().map(|name| (name, true)))
        {
            let captured = if hands {
                fpv.contains(AssetNamespace::T6, name)
            } else {
                bodies.get(name).is_some()
            };
            if captured {
                continue;
            }
            let (load, asset, model) = zones
                .iter()
                .copied()
                .find_map(|load| {
                    load.assets.iter().find_map(|asset| {
                        let model = asset_model::T6Model::new(load, asset)?;
                        (model.name() == Some(name.as_str())).then_some((load, asset, model))
                    })
                })
                .ok_or_else(|| format!("T6 soldier {name}: model missing"))?;
            let mut rows = Vec::with_capacity(model.surface_count());
            for surface in 0..model.surface_count() {
                let slot = model
                    .material_slot(surface)
                    .ok_or("T6 soldier material slot missing")?;
                let material = load
                    .asset_in(asset, slot)
                    .ok_or("T6 soldier material missing")?;
                let raw_name = header_str(load, &material.header, 0)
                    .ok_or("T6 soldier material name missing")?;
                let material_name = asset_core::AssetRef::bare_name(raw_name);
                if let Some(&row) = bound.get(material_name) {
                    rows.push(Some(row));
                    continue;
                }
                let address = load
                    .blocks
                    .ptr_at(slot)
                    .map_err(|e| format!("T6 soldier material pointer: {e:?}"))?
                    .ok_or("T6 soldier material null")?;
                let (material_load, material) = resolve_material(load, &zones, material)?;
                let address = std::ptr::eq(material_load, load).then_some(address);
                let native = capture_native(
                    material_load,
                    &zones,
                    address,
                    material,
                    ColourMapAlpha::Mask,
                    ipaks,
                    &mut decoded,
                    &mut techsets,
                    report,
                )
                .ok_or_else(|| {
                    format!("T6 soldier material {material_name}: native capture failed")
                })?;
                let set = &techsets[&native.technique_set];
                materials.link_t6_technique_set(set, T6Draw::Lit, report);
                let seed = native_material_seed(
                    path,
                    material_name,
                    material,
                    &set.name,
                    false,
                    materials,
                )?;
                let row = materials
                    .t6_material(
                        seed,
                        material_name,
                        set,
                        &native.textures,
                        native.constants,
                        &native.state,
                        T6Draw::Lit,
                        report,
                    )
                    .map_err(|e| format!("T6 soldier material link failed: {e:?}"))?;
                let row = WalkLocalMaterialIndex::from_walk(row);
                bound.insert(material_name.to_owned(), row);
                rows.push(Some(row));
            }
            let skel = asset_model::capture_model_skel_t6(model, |surface| rows[surface])
                .ok_or_else(|| format!("T6 soldier {name}: skeleton capture failed"))?;
            report.push(format!(
                "T6 soldier {name}: {} bones, {} vertices, {} surfaces",
                skel.bones.len(),
                skel.positions.len(),
                skel.surface_materials.len()
            ));
            if hands {
                fpv.insert_in(AssetNamespace::T6, skel, Some(materials));
            } else {
                bodies.insert_in(AssetNamespace::T6, skel, Some(materials));
            }
        }
    }
    bodies.set_kits(kits);
    materials.resolve_technique_set_edges();
    Ok((bodies, fpv))
}

/// The map's entities, then those its gametype's zone adds to them, as the
/// engine reads on from the map's into the addon's.
fn map_entities(path: &Path, load: &ZoneLoad, report: &mut Vec<String>) -> Result<String, String> {
    let mut entities = asset_world::world_t6::entity_string(load)?.to_owned();
    if asset_transport::t6_content::T6ContentMode::for_path(path)
        == asset_transport::t6_content::T6ContentMode::Zombies
        && let Some(zone) = asset_transport::mode_zone(path)
        && let source = path.with_file_name(format!("{zone}.ff"))
        && source.is_file()
        && let Some(mode) = content_zone(&source, report)
        && let Some(addon) = asset_world::world_t6::addon_entity_string(&mode, &zone)
    {
        entities.push_str(addon);
    }
    Ok(entities)
}

/// A compiled script the zone carries, by name.
fn script_bytes(load: &ZoneLoad, name: &str) -> Option<Vec<u8>> {
    let script = load
        .assets
        .iter()
        .find(|a| a.ty == AssetType::Script && header_str(load, &a.header, 0) == Some(name))?;
    let at = decode_ptr(header_u32(&script.header, 8)?)?;
    let length = header_u32(&script.header, 4)? as usize;
    load.blocks.bytes(at, length).ok().map(<[u8]>::to_vec)
}

fn map_teams(
    path: &Path,
    map: &fastfile_t6::ZoneLoad,
    report: &mut Vec<String>,
) -> Result<
    (
        asset_game::MapTeamSettings,
        Vec<fastfile_t6::ZoneLoad>,
        asset_model::SoldierKits,
    ),
    String,
> {
    use asset_core::{AssetKey, AssetKind, AssetNamespace};
    if asset_transport::t6_content::T6ContentMode::for_path(path)
        == asset_transport::t6_content::T6ContentMode::Zombies
    {
        // The map's script dresses its players: each character the zones
        // carry is a kit, in the script's order; the first two also stand
        // for the sides.
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // The gametype's own zone, which the engine loads with the map,
        // carries the survivors of maps that have more than one mode.
        let gametype_zone: Vec<ZoneLoad> = asset_transport::mode_zone(path)
            .map(|zone| path.with_file_name(format!("{zone}.ff")))
            .filter(|source| source.is_file())
            .and_then(|source| content_zone(&source, report))
            .into_iter()
            .collect();
        let carried = |name: &str| {
            std::iter::once(map).chain(&gametype_zone).any(|load| {
                load.assets.iter().any(|asset| {
                    asset_model::T6Model::new(load, asset)
                        .is_some_and(|model| model.name() == Some(name))
                })
            })
        };
        let script = |name: &str| {
            std::iter::once(map)
                .chain(&gametype_zone)
                .find_map(|load| script_bytes(load, name))
                .or_else(|| {
                    let patch = content_zone(
                        &path.with_file_name(format!("{stem}_patch.ff")),
                        &mut Vec::new(),
                    )?;
                    script_bytes(&patch, name)
                })
        };
        let characters: Vec<(String, String)> = script(&format!("maps/mp/{stem}.gsc"))
            .and_then(|bytes| asset_game::t6_player_characters(&bytes))
            .into_iter()
            .flatten()
            .filter_map(|(body, hands)| {
                let body = match body {
                    asset_game::T6Body::Model(model) => model,
                    asset_game::T6Body::Character(module) => {
                        asset_game::t6_character_model(&script(&format!("{module}.gsc"))?)?
                    }
                };
                Some((body, hands))
            })
            // A character the zones have no models for is another mode's.
            .filter(|(body, hands)| carried(body) && carried(hands))
            .collect();
        report.push(format!("T6 zombies characters: {characters:?}"));
        let kit = |index: usize| {
            characters
                .get(index)
                .map(|(body, hands)| asset_model::SoldierKit {
                    body: body.clone(),
                    head: None,
                    arms: Some(hands.clone()),
                })
        };
        return Ok((
            asset_game::MapTeamSettings::default(),
            gametype_zone,
            asset_model::SoldierKits {
                allies: kit(0),
                axis: kit(1).or_else(|| kit(0)),
                extra: (2..characters.len()).filter_map(kit).collect(),
            },
        ));
    }
    let patch = patch_zone(path, report).ok_or("T6 map table zone missing")?;
    let table = patch
        .assets
        .iter()
        .filter_map(|a| asset_game::capture_t6_string_table(&patch, a))
        .find(|t| t.name == "mp/mapstable.csv");
    let table = table.ok_or("T6 map faction table missing")?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("T6 map name missing")?;
    let row = (0..table.rows)
        .find(|&r| table.cell(r as i32, 0) == stem)
        .ok_or("T6 map absent from faction table")?;
    let mut settings = asset_game::MapTeamSettings::default();
    let mut kits = asset_model::SoldierKits::default();
    let mut loads = Vec::new();
    for (index, column) in [1, 2].into_iter().enumerate() {
        let faction = table.cell(row as i32, column);
        let faction_path = path.with_file_name(format!("faction_{faction}_mp.ff"));
        let image = asset_transport::open_t6_zone(&faction_path)
            .map_err(|e| format!("T6 faction {faction}: {e:?}"))?;
        let (load, walk) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
        walk.map_err(|e| format!("T6 faction {faction}: {e:?}"))?;
        let script_name = format!("maps/mp/teams/_teamset_{faction}.gsc");
        let script = load
            .assets
            .iter()
            .find(|a| {
                a.ty == AssetType::Script
                    && header_str(&load, &a.header, 0) == Some(script_name.as_str())
            })
            .ok_or("T6 faction script missing")?;
        let data = load
            .blocks
            .bytes(
                decode_ptr(header_u32(&script.header, 8).ok_or("T6 faction script pointer")?)
                    .ok_or("T6 faction script null")?,
                header_u32(&script.header, 4).ok_or("T6 faction script length")? as usize,
            )
            .map_err(|e| format!("T6 faction script: {e:?}"))?;
        let properties =
            asset_game::t6_team_properties(data).ok_or("T6 faction properties unreadable")?;
        let value = |key: &str| properties.get(key).cloned();
        let key = |name, kind| {
            value(name).and_then(|name| AssetKey::new(AssetNamespace::T6, kind, name).ok())
        };
        let color = value("g_TeamColor_").and_then(|s| {
            let values: Vec<f32> = s
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            values.try_into().ok()
        });
        let icon = key("icons", AssetKind::Material);
        let name = key("g_TeamName_", AssetKind::Localize);
        let strings = asset_game::FactionStrings {
            name: value("strings/_name"),
            eliminated: value("strings/_eliminated"),
            forfeited: value("strings/_forfeited"),
        };
        let music = asset_game::TeamMusic {
            spawn: value("music/spawn_").map(|s| format!("mus_{}", s.to_ascii_lowercase())),
            victory: value("music/victory_").map(|s| format!("mus_{}", s.to_ascii_lowercase())),
            defeat: Some("mus_loss".into()),
            winning: Some("mus_time_running_out".into()),
            losing: Some("mus_time_running_out".into()),
        };
        if index == 0 {
            settings.allies_charset = Some(faction.to_owned());
            settings.allies = icon;
            settings.allies_name = name;
            settings.allies_color = color;
            settings.allies_strings = strings;
            settings.allies_voice = value("voice");
            settings.allies_music = music;
        } else {
            settings.axis_charset = Some(faction.to_owned());
            settings.axis = icon;
            settings.axis_name = name;
            settings.axis_color = color;
            settings.axis_strings = strings;
            settings.axis_voice = value("voice");
            settings.axis_music = music;
        }
        let team = if index == 0 { "allies" } else { "axis" };
        if properties.contains_key("attackers") {
            settings.attackers = Some(team.into());
        }
        if properties.contains_key("defenders") {
            settings.defenders = Some(team.into());
        }
        report.push(format!(
            "T6 faction {team}: {faction}, voice={:?}, icon={:?}",
            value("voice"),
            value("icons")
        ));
        let names: Vec<String> = load
            .assets
            .iter()
            .filter_map(|asset| {
                asset_model::T6Model::new(&load, asset)?
                    .name()
                    .map(str::to_owned)
            })
            .collect();
        let mut faction_kits = asset_model::soldier_kits(&names);
        let kit = faction_kits
            .allies
            .take()
            .or(faction_kits.axis.take())
            .ok_or_else(|| format!("T6 faction {faction}: soldier model missing"))?;
        if index == 0 {
            kits.allies = Some(kit);
        } else {
            kits.axis = Some(kit);
        }
        loads.push(load);
        if let Some(zone) = path.parent().and_then(Path::parent) {
            let localized = zone
                .join("english")
                .join(format!("en_faction_{faction}_mp.ff"));
            if localized.exists() {
                let image = asset_transport::open_t6_zone(&localized)
                    .map_err(|e| format!("T6 localized faction: {e:?}"))?;
                let (load, walk) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
                walk.map_err(|e| format!("T6 localized faction: {e:?}"))?;
                loads.push(load);
            }
        }
    }
    Ok((settings, loads, kits))
}

impl ZoneLane for T6Lane {
    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        material_seed: asset_material::MaterialCatalog,
        common_film_visions: &super::FilmVisionCatalog,
    ) -> LoadedWorld {
        let stage = progress.begin_scoped(StageId::MapAssets, "t6_world", None);
        let result = (|| -> Result<LoadedWorld, String> {
            use asset_core::AssetNamespace;
            use asset_material::t6_techset::T6Draw;
            use asset_world::world_t6::{Reader, build_clip_collision, build_world_draw};
            let (load, walked) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
            walked.map_err(|e| format!("T6 map walk: {e:?}"))?;
            let mut report = vec![format!("T6 map: {} assets", load.assets.len())];
            let world_asset = load
                .assets
                .iter()
                .find(|a| a.ty == AssetType::GfxWorld)
                .ok_or("T6 GfxWorld missing")?;
            let view = fastfile_t6::world::WorldView::new(&load, world_asset)
                .map_err(|e| format!("T6 GfxWorld: {e:?}"))?;
            let ipaks = open_ipaks(path, &mut report);
            let sources_stage =
                progress.begin_scoped(StageId::MapAssets, "t6_shared_sources", None);
            let mode = asset_transport::t6_content::T6ContentMode::for_path(path);
            let mut shared_zones = vec![mode.startup()[0].to_owned(), mode.common().to_owned()];
            if mode == asset_transport::t6_content::T6ContentMode::Zombies {
                shared_zones.extend(
                    mode.supplements(path)
                        .into_iter()
                        .filter(|name| path.with_file_name(format!("{name}.ff")).is_file()),
                );
            }
            let shared_loads = shared_zones
                .into_iter()
                .map(|name| {
                    let image =
                        asset_transport::open_t6_zone(path.with_file_name(format!("{name}.ff")))
                            .map_err(|error| format!("T6 shared image zone {name}: {error:?}"))?;
                    let (load, result) =
                        fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
                    result.map_err(|error| format!("T6 shared image zone {name}: {error:?}"))?;
                    Ok(load)
                })
                .collect::<Result<Vec<_>, String>>()?;
            sources_stage.done();
            let mut code_hud_menus = asset_game::MenuCatalog::default();
            for zone in shared_loads.iter().chain(std::iter::once(&load)) {
                code_hud_menus.absorb(asset_game::t6_menu_catalog(zone));
            }
            report.push(format!(
                "T6 menus: {} menus in {} lists",
                code_hud_menus.menus.len(),
                code_hud_menus.lists.len()
            ));
            let image_loads: Vec<_> = std::iter::once(&load).chain(&shared_loads).collect();
            let mut materials = material_seed;
            let mut decoded = DecodedTextures::new();
            let mut techsets = BTreeMap::new();
            let mut material_rows = BTreeMap::new();
            let mut layer_formats = BTreeMap::new();
            let sky_name = header_str(&load, &world_asset.header, 36);
            let sky_model = sky_name
                .and_then(|name| {
                    load.assets.iter().find(|asset| {
                        asset.ty == AssetType::XModel
                            && header_str(&load, &asset.header, 0) == Some(name)
                    })
                })
                .and_then(|asset| asset_model::T6Model::new(&load, asset));
            let world_materials = view
                .surfaces()
                .map(|surface| surface.material().ok_or("T6 surface material missing"))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sky_materials = Vec::new();
            if let Some(model) = sky_model {
                for surface in 0..model.surface_count() {
                    let material = model
                        .material_slot(surface)
                        .and_then(|slot| load.asset_at(slot))
                        .ok_or("T6 sky material missing")?;
                    sky_materials.push(material);
                }
            }
            let smodel_slots = match Reader::word(&world_asset.header, 784)? {
                0 => Vec::new(),
                count => {
                    let rows = Reader::ptr(&world_asset.header, 876)?;
                    (0..count).map(|i| rows.at(i * 152 + 56)).collect()
                }
            };
            let gump_loads = map_gumps(path, &mut report);
            let gump_models = gump_models(&gump_loads, &image_loads);
            // Each static model's model, and the gumps that carry it: a model
            // the map's zone only names (`,name`) is the engine's `void` until
            // a gump that carries it loads.
            let mut smodels: Vec<(&fastfile_t6::LoadedAsset, asset_model::T6Model, Vec<String>)> =
                Vec::new();
            for &slot in &smodel_slots {
                let Some(asset) = load.asset_in(world_asset, slot) else {
                    continue;
                };
                if smodels
                    .iter()
                    .any(|(seen, _, _)| std::ptr::eq(*seen, asset))
                {
                    continue;
                }
                let Some(model) = asset_model::T6Model::new(&load, asset) else {
                    continue;
                };
                match model
                    .name()
                    .and_then(|name| name.strip_prefix(','))
                    .and_then(|name| gump_models.get(name))
                {
                    Some((carried, gumps)) => smodels.push((asset, *carried, gumps.clone())),
                    None => smodels.push((asset, model, Vec::new())),
                }
            }
            report.push(format!(
                "T6 gumps: {} zones carry {} models; {} static models are left to them",
                gump_loads.len(),
                gump_models.len(),
                smodels
                    .iter()
                    .filter(|(_, _, gumps)| !gumps.is_empty())
                    .count()
            ));
            let script_placements = asset_world::parse_script_model_placements(&map_entities(
                path,
                &load,
                &mut report,
            )?);
            let destructibles = destructible::capture(&load)?;
            let destructible_models = destructible::model_names(&destructibles);
            let mut script_xmodels: Vec<asset_model::T6Model> = Vec::new();
            for source in &image_loads {
                for asset in &source.assets {
                    let Some(model) = asset_model::T6Model::new(source, asset) else {
                        continue;
                    };
                    let Some(name) = model.name() else {
                        continue;
                    };
                    // A zombies map's scripts may set any model its zones carry.
                    let zombies = asset_transport::t6_content::T6ContentMode::for_path(path)
                        == asset_transport::t6_content::T6ContentMode::Zombies;
                    if (zombies
                        || script_placements.iter().any(|p| p.model == name)
                        || destructible_models.contains(name))
                        && !script_xmodels.iter().any(|seen| seen.name() == Some(name))
                    {
                        script_xmodels.push(model);
                    }
                }
            }
            // The models only gumps carry, for the entities that set them;
            // each draws while a gump that carries it is held.
            let mut script_gumps = Vec::new();
            for (name, (model, gumps)) in &gump_models {
                if !script_xmodels.iter().any(|seen| seen.name() == Some(*name)) {
                    script_xmodels.push(*model);
                    script_gumps.push(((*name).to_owned(), gumps.clone()));
                }
            }
            let smodel_materials: Vec<_> = smodels
                .iter()
                .map(|(_, model, _)| model)
                .chain(&script_xmodels)
                .flat_map(|model| {
                    (0..model.surface_count()).filter_map(|surface| {
                        Some((
                            model.source(),
                            model.source().asset_at(model.material_slot(surface)?)?,
                        ))
                    })
                })
                .collect();
            let mut surface_materials = Vec::with_capacity(world_materials.len());
            let mut surface_layer_formats = Vec::with_capacity(world_materials.len());
            let materials_stage = progress.begin_scoped(StageId::MapAssets, "t6_materials", None);
            for (source, material) in world_materials
                .iter()
                .chain(&sky_materials)
                .map(|material| (&load, *material))
                .chain(smodel_materials.iter().copied())
            {
                let raw_name =
                    header_str(source, &material.header, 0).ok_or("T6 material name missing")?;
                let name = asset_core::AssetRef::bare_name(raw_name).to_owned();
                let row = if let Some(row) = material_rows.get(&name) {
                    *row
                } else {
                    let is_sky = sky_materials.iter().any(|sky| std::ptr::eq(*sky, material));
                    let (material_load, material) =
                        resolve_material(source, &image_loads, material)?;
                    let native = capture_native(
                        material_load,
                        &image_loads,
                        None,
                        material,
                        ColourMapAlpha::Gloss,
                        &ipaks,
                        &mut decoded,
                        &mut techsets,
                        &mut report,
                    )
                    .ok_or_else(|| format!("T6 material {name}: native capture failed"))?;
                    let set = &techsets[&native.technique_set];
                    materials.link_t6_technique_set(set, T6Draw::Lit, &mut report);
                    let seed = native_material_seed(
                        path,
                        &name,
                        material,
                        &set.name,
                        is_sky,
                        &mut materials,
                    )?;
                    let row = materials
                        .t6_material(
                            seed,
                            &name,
                            set,
                            &native.textures,
                            native.constants,
                            &native.state,
                            T6Draw::Lit,
                            &mut report,
                        )
                        .map_err(|e| format!("T6 material link failed: {e:?}"))?;
                    layer_formats.insert(row, set.world_vert_format);
                    material_rows.insert(name, row);
                    row
                };
                surface_materials.push(Some(row));
                surface_layer_formats.push(layer_formats.get(&row).copied().unwrap_or(0));
            }
            // The renderer's own post materials its screen overlays draw,
            // from the startup zone that carries them, and the client
            // scripts' filter materials the map's zones carry.
            let filters = T6_FILTER_MATERIALS.iter().map(|name| (*name, true));
            for (name, filter) in T6_OVERLAY_MATERIALS
                .iter()
                .map(|name| (*name, false))
                .chain(filters)
            {
                if material_rows.contains_key(name) {
                    continue;
                }
                let Some((source, material)) = image_loads.iter().find_map(|zone| {
                    zone.assets
                        .iter()
                        .find(|a| {
                            a.ty == AssetType::Material
                                && header_str(zone, &a.header, 0) == Some(name)
                        })
                        .map(|a| (*zone, a))
                }) else {
                    if !filter {
                        report.push(format!("T6 overlay material {name}: no zone carries it"));
                    }
                    continue;
                };
                let Some(native) = capture_native(
                    source,
                    &image_loads,
                    None,
                    material,
                    ColourMapAlpha::Gloss,
                    &ipaks,
                    &mut decoded,
                    &mut techsets,
                    &mut report,
                ) else {
                    report.push(format!("T6 overlay material {name}: native capture failed"));
                    continue;
                };
                let set = &techsets[&native.technique_set];
                materials.link_t6_technique_set(set, T6Draw::Lit, &mut report);
                let seed =
                    native_material_seed(path, name, material, &set.name, false, &mut materials)?;
                match materials.t6_material(
                    seed,
                    name,
                    set,
                    &native.textures,
                    native.constants,
                    &native.state,
                    T6Draw::Lit,
                    &mut report,
                ) {
                    Ok(row) => {
                        material_rows.insert(name.to_owned(), row);
                    }
                    Err(error) => report.push(format!("T6 overlay material {name}: {error:?}")),
                }
            }
            materials_stage.done();
            report.push(format!(
                "T6 map materials: {} unique technique sets linked once",
                techsets.len()
            ));
            surface_materials.truncate(world_materials.len());
            surface_layer_formats.truncate(world_materials.len());
            let mut draw = build_world_draw(
                &load,
                world_asset,
                &materials,
                surface_materials,
                &surface_layer_formats,
            )?;
            if let Some(model) = sky_model {
                let skel = asset_model::capture_model_skel_t6(model, |surface| {
                    let material = load.asset_at(model.material_slot(surface)?)?;
                    let name = header_str(&load, &material.header, 0)?;
                    material_rows
                        .get(name)
                        .copied()
                        .map(asset_core::WalkLocalMaterialIndex::from_walk)
                })
                .ok_or("T6 sky model geometry missing")?;
                draw.sky_model = Some(asset_world::world_t6::model_mesh(&load, model, skel)?);
            }
            report.push(format!(
                "T6 skybox: name={sky_name:?} captured={}",
                draw.sky_model.is_some()
            ));
            let model_material = |model: asset_model::T6Model, surface| {
                let material = model.source().asset_at(model.material_slot(surface)?)?;
                let name = header_str(model.source(), &material.header, 0)?;
                material_rows
                    .get(asset_core::AssetRef::bare_name(name))
                    .copied()
                    .map(asset_core::WalkLocalMaterialIndex::from_walk)
            };
            let mut static_model_meshes = Vec::new();
            let mut static_model_gumps = Vec::new();
            let mut smodel_mesh = Vec::with_capacity(smodels.len());
            for (asset, model, gumps) in &smodels {
                let mesh = asset_model::capture_model_skel_t6(*model, |surface| {
                    model_material(*model, surface)
                })
                .map(|skel| asset_world::world_t6::model_mesh(model.source(), *model, skel));
                match mesh {
                    Some(Ok(mesh)) => {
                        smodel_mesh.push((*asset, static_model_meshes.len()));
                        static_model_meshes.push(mesh);
                        static_model_gumps.push(gumps.clone());
                    }
                    Some(Err(error)) => {
                        report.push(format!("T6 static model {:?}: {error}", model.name()))
                    }
                    None => report.push(format!(
                        "T6 static model {:?}: geometry missing",
                        model.name()
                    )),
                }
            }
            let static_model_instances =
                asset_world::world_t6::static_model_placements(&load, world_asset, |slot| {
                    let asset = load.asset_in(world_asset, slot)?;
                    smodel_mesh
                        .iter()
                        .find(|(seen, _)| std::ptr::eq(*seen, asset))
                        .map(|&(_, mesh)| mesh)
                })?;
            let mut map_xmodel_scene_assets = asset_world::MapXModelSceneCatalog::default();
            for model in &script_xmodels {
                let (Some(name), Some(skel)) = (
                    model.name(),
                    asset_model::capture_model_skel_t6(*model, |surface| {
                        model_material(*model, surface)
                    }),
                ) else {
                    continue;
                };
                map_xmodel_scene_assets.insert(
                    asset_world::MapXModelAssetKey(name.to_owned()),
                    asset_world::MapXModelSceneAsset::T6(Arc::new(skel)),
                );
            }
            for (name, gumps) in script_gumps {
                map_xmodel_scene_assets.set_gumps(name, gumps);
            }
            let script_model_instances = script_placements
                .iter()
                .map(|placement| {
                    t6_script_model_instance(
                        placement,
                        &mut map_xmodel_scene_assets,
                        &destructibles,
                    )
                })
                .collect::<Vec<_>>();
            report.push(format!(
                "T6 script models: placements={} models={}",
                script_model_instances.len(),
                script_xmodels.len()
            ));
            report.push(format!(
                "T6 static models: slots={} meshes={} placed={}",
                static_model_instances.len(),
                static_model_meshes.len(),
                static_model_instances.iter().flatten().count()
            ));
            for zone in shared_loads.iter().chain(std::iter::once(&load)) {
                for def in zone
                    .assets
                    .iter()
                    .filter(|asset| asset.ty == AssetType::LightDef)
                {
                    let Some(name) = header_str(zone, &def.header, 0) else {
                        continue;
                    };
                    let Some(image) = def.field(4).and_then(|index| zone.assets.get(index)) else {
                        continue;
                    };
                    let image_name = header_str(zone, &image.header, IMAGE_NAME)
                        .ok_or("T6 attenuation image name missing")?;
                    let decoded = match decode_world_image(zone, image, &image_loads, &ipaks) {
                        Ok(image) => Arc::new(image),
                        Err(error)
                            if !draw.primary_lights.iter().any(|light| {
                                light.def_name.as_deref().is_some_and(|wanted| {
                                    wanted.trim_start_matches(',') == name.trim_start_matches(',')
                                })
                            }) =>
                        {
                            report.push(format!("T6 unused light definition {name}: {error}"));
                            continue;
                        }
                        Err(error) => return Err(format!("T6 light definition {name}: {error}")),
                    };
                    let width = decoded.width() as u16;
                    materials.link_image(asset_material::AuthoredImage {
                        namespace: AssetNamespace::T6,
                        name: asset_core::AssetRef::Real(image_name.to_owned()),
                        map_type: image.header[4],
                        semantic: image.header[5],
                        category: image.header[6],
                        use_srgb_reads: false,
                        width,
                        height: decoded.height() as u16,
                        depth: 1,
                        level_count: decoded.texture_descriptor.mip_level_count as u8,
                        format: 0,
                        payload: Arc::new(Vec::new()),
                        decoded: Some(decoded),
                        common_owned: false,
                        decoded_variant: None,
                        decoded_by: None,
                        pending_decode: None,
                    });
                    draw.light_defs
                        .push(asset_world::world_draw::CapturedLightDef {
                            namespace: AssetNamespace::T6,
                            name: asset_core::AssetRef::decode(name),
                            attenuation_image_name: Some(image_name.to_owned()),
                            attenuation_width: Some(width),
                            attenuation_sampler: t6_sampler_state(def.header[8]),
                            lmap_lookup_start: Reader::word(&def.header, 12)? as i32,
                        });
                }
            }
            report.push(format!("T6 light definitions: {}", draw.light_defs.len()));
            if let Some(image) = world_asset
                .field(740)
                .and_then(|index| load.assets.get(index))
            {
                let decoded = Arc::new(decode_world_image(&load, image, &image_loads, &ipaks)?);
                let name = header_str(&load, &image.header, IMAGE_NAME)
                    .ok_or("T6 outdoor image name missing")?
                    .trim_start_matches(',');
                materials.link_image(asset_material::AuthoredImage {
                    namespace: AssetNamespace::T6,
                    name: asset_core::AssetRef::Real(name.to_owned()),
                    map_type: image.header[4],
                    semantic: image.header[5],
                    category: image.header[6],
                    use_srgb_reads: false,
                    width: decoded.width() as u16,
                    height: decoded.height() as u16,
                    depth: 1,
                    level_count: decoded.texture_descriptor.mip_level_count as u8,
                    format: image.image_data.as_ref().map_or(0, |source| source.format),
                    payload: Arc::new(Vec::new()),
                    decoded: Some(decoded),
                    common_owned: false,
                    decoded_variant: None,
                    decoded_by: None,
                    pending_decode: None,
                });
            }
            let reader = Reader(&load);
            let probe_count = Reader::word(&world_asset.header, 396)?;
            if probe_count != 0 {
                let probes = Reader::ptr(&world_asset.header, 400)?;
                for i in 0..probe_count {
                    let row = probes.at(i * 76);
                    let image = load
                        .asset_in(world_asset, row.at(60))
                        .ok_or("T6 reflection image missing")?;
                    let decoded = Arc::new(decode_world_image(&load, image, &image_loads, &ipaks)?);
                    let name = header_str(&load, &image.header, IMAGE_NAME)
                        .ok_or("T6 probe name missing")?;
                    let index = materials.link_image(asset_material::AuthoredImage {
                        namespace: AssetNamespace::T6,
                        name: asset_core::AssetRef::Real(name.to_owned()),
                        map_type: 5,
                        semantic: image.header[5],
                        category: image.header[6],
                        use_srgb_reads: false,
                        width: decoded.width() as u16,
                        height: decoded.height() as u16,
                        depth: 1,
                        level_count: decoded.texture_descriptor.mip_level_count as u8,
                        format: u32::from_le_bytes(*b"DXT5"),
                        payload: Arc::new(Vec::new()),
                        decoded: Some(decoded),
                        common_owned: false,
                        decoded_variant: None,
                        decoded_by: None,
                        pending_decode: None,
                    });
                    draw.reflection_probes
                        .push(asset_world::WorldReflectionProbe {
                            image: Some(index),
                            origin: reader.xyz(row)?,
                            lighting_sh: Some([
                                reader.xyzw(row.at(12))?,
                                reader.xyzw(row.at(28))?,
                                reader.xyzw(row.at(44))?,
                            ]),
                        });
                }
            }
            let page_count = Reader::word(&world_asset.header, 408)?;
            if page_count != 0 {
                let pages = Reader::ptr(&world_asset.header, 412)?;
                let mut decoded_pages = Vec::new();
                for i in 0..page_count {
                    let row = pages.at(i * 8);
                    let primary = load.asset_in(world_asset, row);
                    let secondary = load
                        .asset_in(world_asset, row.at(4))
                        .ok_or("T6 secondary lightmap missing")?;
                    let primary_image = primary
                        .map(|asset| decode_world_image(&load, asset, &image_loads, &ipaks))
                        .transpose()?;
                    let secondary_image =
                        decode_world_image(&load, secondary, &image_loads, &ipaks)?;
                    let secondary_size =
                        bevy::math::UVec2::new(secondary_image.width(), secondary_image.height());
                    let primary_size = primary_image
                        .as_ref()
                        .map(|image| bevy::math::UVec2::new(image.width(), image.height()))
                        .unwrap_or(secondary_size);
                    decoded_pages.push(Some(asset_world::WorldLightmap {
                        ambient_image: secondary_image.clone(),
                        directional_image: secondary_image.clone(),
                        sun_mask_image: primary_image.as_ref().unwrap_or(&secondary_image).clone(),
                        primary_image,
                        secondary_image: Some(secondary_image),
                        secondary_b_image: None,
                        ambient_source_name: header_str(&load, &secondary.header, IMAGE_NAME)
                            .unwrap_or("")
                            .to_owned(),
                        sun_mask_source_name: primary
                            .and_then(|asset| header_str(&load, &asset.header, IMAGE_NAME))
                            .unwrap_or("")
                            .to_owned(),
                        ambient_size: secondary_size,
                        sun_mask_size: primary_size,
                    }));
                }
                draw.lightmap = Ok(decoded_pages);
            }
            report.push(format!(
                "T6 lighting images: {probe_count} reflection probes, {page_count} lightmap pages"
            ));
            report.push(format!(
                "T6 world: {} vertices, {} triangles, {} surfaces, {} materials",
                draw.stats.vertices,
                draw.stats.triangles,
                draw.stats.surfaces,
                material_rows.len()
            ));
            let (team_settings, faction_loads, kits) = map_teams(path, &load, &mut report)?;
            let (bodies, fpv_meshes) = capture_soldiers(
                path,
                &load,
                &faction_loads,
                &shared_loads,
                &ipaks,
                kits,
                &mut materials,
                &mut report,
            )?;
            let mut sound_loads: Vec<_> = std::iter::once(&load)
                .chain(&faction_loads)
                .chain(&shared_loads)
                .collect();
            let mut sound_names = asset_audio::t6_sound_names(&sound_loads);
            sound_names.extend(
                [&team_settings.allies_music, &team_settings.axis_music]
                    .into_iter()
                    .flat_map(|music| {
                        [
                            &music.spawn,
                            &music.victory,
                            &music.defeat,
                            &music.winning,
                            &music.losing,
                        ]
                        .into_iter()
                        .filter_map(|alias| alias.clone())
                    }),
            );
            sound_names.sort();
            sound_names.dedup();
            sound_loads.extend(&shared_loads);
            let mode = asset_transport::t6_content::T6ContentMode::for_path(path);
            let code = sound_zone(
                &path.with_file_name(format!(
                    "{}.ff",
                    if mode == asset_transport::t6_content::T6ContentMode::Zombies {
                        "code_post_gfx_zm"
                    } else {
                        "code_post_gfx_mp"
                    }
                )),
                &mut report,
            );
            sound_loads.extend(&code);
            let (banks, bank_report) = asset_audio::t6_sound_banks(path);
            report.extend(bank_report);
            let (sound_catalog, filled, sound_gaps) = asset_audio::capture_t6_sounds(
                path,
                &sound_loads,
                &banks,
                sound_names.iter().map(String::as_str),
            );
            report.push(format!(
                "T6 map/faction sounds: {} names, {} captured, {} gaps",
                sound_names.len(),
                filled.len(),
                sound_gaps.len()
            ));
            report.extend(sound_gaps);
            let mut sound_capture = asset_audio::ZoneSoundCapture::for_map(
                path,
                asset_core::FamilyId::T6,
                "T6 map/factions",
            );
            sound_capture.set_t6(sound_catalog);
            let sound = Some(sound_capture.finish(Ok(())));
            let collision = build_clip_collision(&load)?;
            report.push(format!(
                "T6 collision: {} brushes, {} BSP leaves, {} triangles, {} static models",
                collision.brushes.len(),
                collision.leaves.len(),
                collision.mesh.tri_indices.len() / 3,
                collision.static_models.len(),
            ));
            let entities = map_entities(path, &load, &mut report)?;
            let minimap_corners = asset_world::parse_minimap_corners(&entities);
            let north_yaw = asset_world::parse_worldspawn_north_yaw(&entities);
            let ui_modules = fastfile_t6::scripts(&load)
                .modules
                .into_iter()
                .map(|module| (module.name.to_owned(), Arc::from(module.bytes)))
                .collect();
            let mut ui_images = Vec::new();
            for asset in &load.assets {
                if asset.ty == AssetType::Material
                    && let Some(name) = header_str(&load, &asset.header, 0)
                        .filter(|name| name.starts_with("compass_map_"))
                {
                    match capture_icon(&load, asset, &[&load], &ipaks) {
                        Ok(iwi) => ui_images.push((
                            name.to_owned(),
                            asset_material::ZoneUiImage {
                                iwi,
                                state: capture_ui_state(&load, asset),
                                rgba: None,
                                sampler: None,
                                scanline: None,
                            },
                        )),
                        Err(error) => report.push(format!("T6 compass material {name}: {error}")),
                    }
                }
            }
            let entities = asset_world::t6_entities_for_iw4_rules(&entities);
            let spawns = match mode {
                asset_transport::t6_content::T6ContentMode::Multiplayer => {
                    asset_world::dm_spawn_points_treyarch(&entities)
                }
                asset_transport::t6_content::T6ContentMode::Zombies => {
                    asset_world::zombies_spawn_points(&entities)
                }
            };
            let mut scripts = crate::ScriptSources::default();
            scripts.set_entities(entities.clone());
            scripts.capture_config(
                "dlc/mounted.txt",
                asset_transport::t6_content::T6ContentMode::dlc_packages(path).join("\n"),
            );
            // The factions' scripts are in the factions' own zones.
            let faction_zones = [&team_settings.allies_charset, &team_settings.axis_charset]
                .into_iter()
                .flatten()
                .map(|faction| path.with_file_name(format!("faction_{faction}_mp.ff")));
            super::capture_script_zones(
                path,
                image,
                faction_zones.chain(mode.captured_patches()).collect(),
                &mut scripts,
                &mut report,
            );
            for (module, bytes) in mode.captured_modules() {
                report.push(format!("scripts: captured module {module}"));
                scripts.capture_module(&module, bytes);
            }
            if let Ok(ropes) = capture_t6_ropes(&load)
                && !ropes.is_empty()
            {
                scripts.capture_config("physconstraints/ropes.txt", ropes);
            }
            if let Ok(volumes) = capture_t6_fog_volumes(&load, &world_asset.header)
                && !volumes.is_empty()
            {
                scripts.capture_config("gfxworld/fog_volumes.txt", volumes);
            }
            let light_grid = asset_world::world_t6::light_grid(&load, world_asset)
                .map_err(|e| report.push(format!("T6 light grid: {e}")))
                .ok();
            let mut smodel_lighting_samples = match &light_grid {
                Some(grid) => {
                    let palette =
                        asset_world::world_t6::static_model_lighting_palette(&load, world_asset)?;
                    let origins =
                        asset_world::world_t6::static_model_lighting_origins(&load, world_asset)?;
                    let mut authored = Vec::new();
                    let mut unresolved = Vec::new();
                    for (slot, origin) in origins {
                        if static_model_instances.get(slot).is_none_or(Option::is_none) {
                            continue;
                        }
                        match asset_model::model_lighting::t6_smodel_palette_lighting(
                            &grid.view(),
                            palette[slot],
                            slot,
                            origin,
                        ) {
                            Some(sample) => authored.push(sample),
                            None => unresolved.push((slot, origin)),
                        }
                    }
                    report.push(format!(
                        "T6 authored static model lighting: {} placements, {} spatial samples",
                        authored.len(),
                        unresolved.len()
                    ));
                    authored.extend(super::helpers::smodel_lighting_samples(
                        &mut report,
                        grid,
                        unresolved,
                        &static_model_instances,
                        Some(&collision),
                    ));
                    authored.sort_unstable_by_key(|sample| sample.authored_slot);
                    authored
                }
                None => Vec::new(),
            };
            let mut vertex_lighting =
                asset_world::world_t6::static_model_vertex_lighting(&load, world_asset)?;
            for sample in &mut smodel_lighting_samples {
                sample.vertex_lighting = vertex_lighting
                    .get_mut(sample.authored_slot)
                    .and_then(Option::take)
                    .map(std::sync::Arc::new);
            }
            let vertex_lit_slots = vertex_lighting.iter().flatten().count();
            if vertex_lit_slots > 0 {
                report.push(format!(
                    "T6 static model vertex lighting: {vertex_lit_slots} slots without a lighting sample"
                ));
            }
            let reflection_probe_images =
                super::helpers::decode_reflection_probes(&mut report, &draw, &materials);
            let exp_fog = load
                .assets
                .iter()
                .filter(|a| a.ty == AssetType::Script)
                .filter(|a| {
                    header_str(&load, &a.header, 0).is_some_and(|n| {
                        n.starts_with("maps/mp/createart/") && n.ends_with("_art.gsc")
                    })
                })
                .find_map(|a| {
                    let data = load
                        .blocks
                        .bytes(
                            decode_ptr(header_u32(&a.header, 8)?)?,
                            header_u32(&a.header, 4)? as usize,
                        )
                        .ok()?;
                    asset_world::t6_createart_fog(data)
                });
            report.push(format!(
                "T6 createart fog: {}",
                if exp_fog.is_some() { "ready" } else { "none" }
            ));
            let vision_name = path.file_stem().map(|stem| {
                format!(
                    "vision/{}.vision",
                    stem.to_string_lossy().to_ascii_lowercase()
                )
            });
            let t6_visions: std::collections::BTreeMap<_, _> = shared_loads
                .iter()
                .chain(std::iter::once(&load))
                .flat_map(|zone| zone.assets.iter().map(move |asset| (zone, asset)))
                .filter(|(_, a)| a.ty == AssetType::RawFile)
                .filter_map(|(zone, a)| {
                    let name = header_str(zone, &a.header, 0)?
                        .replace('\\', "/")
                        .to_ascii_lowercase();
                    if !name.starts_with("vision/") || !name.ends_with(".vision") {
                        return None;
                    }
                    let grade = (|| {
                        let length = header_u32(&a.header, 4)
                            .ok_or(asset_world::T6FilmGradeParseError::InvalidSpan)?
                            as usize;
                        let pointer = header_u32(&a.header, 8)
                            .and_then(decode_ptr)
                            .ok_or(asset_world::T6FilmGradeParseError::InvalidSpan)?;
                        let data = zone
                            .blocks
                            .bytes(pointer, length)
                            .map_err(|_| asset_world::T6FilmGradeParseError::InvalidSpan)?;
                        let text = std::str::from_utf8(data)
                            .map_err(|_| asset_world::T6FilmGradeParseError::InvalidText)?;
                        asset_world::parse_t6_vision(text)
                    })();
                    Some((name, grade))
                })
                .collect();
            let t6_vision = match vision_name.as_ref().and_then(|name| t6_visions.get(name)) {
                Some(Ok(vision)) => {
                    report.push(format!(
                        "T6 vision: ready film={} bloom={}",
                        vision.film.is_some(),
                        vision.bloom.is_some()
                    ));
                    Some(*vision)
                }
                Some(Err(error)) => {
                    report.push(format!("T6 vision: refused {error:?}"));
                    None
                }
                None => {
                    report.push("T6 vision: missing map preset".into());
                    None
                }
            };
            let mut effect_content = T6Content {
                path: path.to_owned(),
                ..Default::default()
            };
            capture_effects(&image_loads, &ipaks, &mut effect_content);
            let mut fx = asset_game::FxCatalog::default();
            let mut effect_refusals = Vec::new();
            report.push(common_compile::bind_t6_fx(
                &mut effect_content,
                &mut materials,
                &mut fx,
                &mut effect_refusals,
            ));
            report.extend(effect_content.report);
            report.extend(
                effect_refusals
                    .into_iter()
                    .map(|refusal| format!("T6 map effects: {refusal:?}")),
            );
            let world = crate::session_load::PreparedWorld {
                min: draw.stats.min,
                max: draw.stats.max,
                world_bounds: draw.stats.bounds,
                draw: Some(draw),
                light_grid,
                reflection_probe_images,
                static_model_meshes,
                static_model_gumps,
                static_model_instances,
                map_xmodel_scene_assets,
                script_model_instances,
                fx,
                smodel_lighting_samples,
                policy: WorldDrawPolicy::t6(),
                intermission_view: asset_world::parse_intermission_view(&entities),
                exp_fog,
                t6_vision,
                t6_visions,
                film_visions: common_film_visions.clone(),
                ..crate::PreparedWorld::empty(WorldDrawPolicy::t6())
            };
            let mut xanims = asset_anim::XAnimBuild::default();
            if mode == asset_transport::t6_content::T6ContentMode::Zombies {
                let namespace = asset_core::AssetNamespace::T6;
                for source in &image_loads {
                    for asset in &source.assets {
                        xanims.capture_anim_states_t6(namespace, source, asset);
                    }
                }
                // Every clip the loaded zones carry, as the engine has them
                // all for its scripts' `%` animations and state tables.
                for source in image_loads.iter().copied() {
                    for asset in &source.assets {
                        if asset.ty == AssetType::XAnimParts {
                            xanims.capture_xanim_t6(namespace, "", source, asset);
                        }
                    }
                }
                report.push(format!("T6 map animations: {}", xanims.len()));
            }
            Ok(LoadedWorld {
                ui_images,
                ui_modules,
                scripts,
                world,
                xanims,
                materials,
                sound,
                collision: Some(collision),
                bodies,
                fpv_meshes,
                spawns,
                facts: crate::MapFacts {
                    minimap_corners,
                    north_yaw,
                    script_sound: asset_audio::MapScriptSoundFacts {
                        attackers: team_settings.attackers.clone(),
                        defenders: team_settings.defenders.clone(),
                        ..Default::default()
                    },
                    team_settings,
                    path_nodes: load
                        .path_nodes()
                        .into_iter()
                        .map(|node| asset_world::PathNode {
                            kind: match node.kind {
                                1 => asset_world::PathNodeKind::Path,
                                17 => asset_world::PathNodeKind::NegotiationBegin,
                                18 => asset_world::PathNodeKind::NegotiationEnd,
                                other => asset_world::PathNodeKind::Other(other as u16),
                            },
                            origin: node.origin,
                            yaw: node.angle,
                            fields: Some(asset_world::PathNodeFields {
                                type_name: node.type_name().to_owned(),
                                spawnflags: node.spawnflags,
                                script_linkname: node.script_linkname,
                                script_noteworthy: node.script_noteworthy,
                                radius: node.radius,
                                min_use_dist_sq: node.min_use_dist_sq,
                            }),
                            targetname: node.targetname,
                            target: node.target,
                            animscript: node.animscript,
                            links: node
                                .links
                                .iter()
                                .map(|link| asset_world::PathLink {
                                    node: link.node,
                                    distance: link.distance,
                                    negotiation: link.negotiation,
                                })
                                .collect(),
                        })
                        .collect(),
                    code_hud_menus: asset_game::SessionCodeHudMenus(Some(std::sync::Arc::new(
                        code_hud_menus,
                    ))),
                    ..Default::default()
                },
                report,
                ..LoadedWorld::empty(WorldDrawPolicy::t6())
            })
        })();
        match result {
            Ok(loaded) => {
                stage.done();
                loaded
            }
            Err(error) => {
                stage.fail();
                LoadedWorld::with_gap(
                    WorldDrawPolicy::t6(),
                    PreparedCapability::PreparedWorld,
                    error,
                    None,
                )
            }
        }
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus {
        let mut report = vec![format!("common_mp (T6): {}", path.display())];
        // Claimed before the walk: the sound bank may ask for this zone while
        // the content is still decoding, and must wait for these aliases
        // rather than read the zone itself.
        let sound_claim = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T6,
            "t6 common",
        );
        let schema = match schema() {
            Ok(schema) => schema,
            Err(error) => {
                report.push(error);
                return CommonCensus {
                    material_population: material_seed,
                    report,
                    ..Default::default()
                };
            }
        };
        let stage = progress.begin_scoped(StageId::CommonAssets, "t6_walk", None);
        let (load, walked) = fastfile_t6::load_zone(schema, &image.bytes, |_, _| true);
        stage.done();
        if let Err(error) = walked {
            report.push(format!(
                "common_mp T6 walk: stopped after {} assets — {error:?}",
                load.assets.len()
            ));
        }
        let mut catalog = asset_game::WeaponCatalog::default();
        catalog.set_capture_ns(asset_core::AssetNamespace::T6);
        let content_stage = progress.begin_scoped(StageId::CommonAssets, "t6_content", None);
        let sources_stage = progress.begin_scoped(StageId::CommonAssets, "t6_sources", None);
        let mode = asset_transport::t6_content::T6ContentMode::for_path(path);
        let patch = patch_zone(path, &mut report);
        let ipaks = open_ipaks(path, &mut report);
        let mut others: Vec<_> = patch.into_iter().collect();
        let mut before_map = others.len();
        match mode {
            asset_transport::t6_content::T6ContentMode::Multiplayer => {
                others.extend(hands_zone(path, &mut report))
            }
            asset_transport::t6_content::T6ContentMode::Zombies => {
                if path.file_stem().and_then(|s| s.to_str()) != Some(mode.common()) {
                    // The engine's default models (`defaultweapon`,
                    // `viewmodel_default`) are in its first startup zone.
                    if let Some(startup) = content_zone(
                        &path.with_file_name(format!("{}.ff", mode.startup()[0])),
                        &mut report,
                    ) {
                        others.insert(0, startup);
                    }
                    others.extend(content_zone(
                        &path.with_file_name("common_zm.ff"),
                        &mut report,
                    ));
                    before_map = others.len();
                    for name in mode.supplements(path).into_iter().skip(1) {
                        let source = path.with_file_name(format!("{name}.ff"));
                        if source.is_file() {
                            others.extend(content_zone(&source, &mut report));
                        }
                    }
                }
            }
        }
        // Zombies weapons in the engine's zone order; a later zone's weapon
        // replaces one of the same name.
        let (early, late) = match mode {
            asset_transport::t6_content::T6ContentMode::Zombies => others.split_at(before_map),
            asset_transport::t6_content::T6ContentMode::Multiplayer => (&others[..0], &others[..0]),
        };
        let mut seen = 0;
        for zone in early.iter().chain(std::iter::once(&load)).chain(late) {
            for weapon in zone.weapons() {
                seen += 1;
                if let Some(view) = weapon {
                    catalog.capture_t6(view);
                }
            }
        }
        let captured = catalog.len();
        sources_stage.done();
        let models_stage = progress.begin_scoped(StageId::CommonAssets, "t6_models", None);
        let mut content = capture_content(path, &load, &others, &ipaks);
        models_stage.done();
        let anims_stage = progress.begin_scoped(StageId::CommonAssets, "t6_anims", None);
        let ordered_loads: Vec<_> = early
            .iter()
            .chain(std::iter::once(&load))
            .chain(late)
            .collect();
        let player_anim_sources = capture_player_anim_sources(&ordered_loads, image.game);
        capture_xanims(&load, &others, &player_anim_sources, &mut content);
        anims_stage.done();
        let icons_stage = progress.begin_scoped(StageId::CommonAssets, "t6_icons", None);
        let icon_loads: Vec<_> = others.iter().chain(std::iter::once(&load)).collect();
        let icons = capture_weapon_icons(path, &icon_loads, &ipaks, &mut report);
        asset_material::ui_font::store_ui_fonts(asset_core::AssetNamespace::T6, icons.fonts);
        icons_stage.done();
        let sounds_stage = progress.begin_scoped(StageId::CommonAssets, "t6_sounds", None);
        capture_sounds(path, &load, &others, sound_claim, &mut content);
        sounds_stage.done();
        let (hits, misses, bytes) = ipaks
            .iter()
            .map(asset_transport::IPak::read_cache_stats)
            .fold((0, 0, 0), |(h, m, b), (hits, misses, bytes)| {
                (h + hits, m + misses, b + bytes)
            });
        report.push(format!(
            "T6 common IPAK reuse: hits={hits} inflates={misses} retained_bytes={bytes}"
        ));
        content_stage.done();
        report.extend(content.report.iter().cloned());
        let impact_loads: Vec<_> = std::iter::once(&load).chain(others.iter()).collect();
        let impact_fx = capture_impact_table(&impact_loads);
        let tracers = capture_tracers(&impact_loads, path, image.game);
        let weapons = catalog.into_build();
        report.push(format!(
            "T6 content: walked {} assets; weapons {seen} seen, {captured} captured",
            load.assets.len()
        ));
        CommonCensus {
            ui_images: icons.images,
            ui_modules: icons.modules,
            ui_tables: icons.tables,
            weapons,
            impact_fx,
            tracers,
            player_anim_sources,
            material_population: material_seed,
            report,
            preparation: Some(Box::new(common_compile::T6CommonCompiler {
                content,
                captured_weapons: captured,
            })),
            ..Default::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        _image: &ZoneImage,
        _progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        MaterialPopulation {
            materials: material_seed,
            report: vec![format!(
                "startup materials: T6 materials are not read ({})",
                path.display()
            )],
            ..Default::default()
        }
    }
}

/// The map's ropes, one per line: the `ClipMap` constraints of type
/// `CONSTRAINT_ROPE` (8), in map order, which is how `getrope` numbers them.
/// Each line is the targetname, then tab-separated: the start's and end's
/// attachment types and entities, the start and end points, the length,
/// the wind scale and the flags.
fn capture_t6_ropes(load: &ZoneLoad) -> Result<String, String> {
    use asset_world::world_t6::Reader;
    let Some(asset) = load
        .assets
        .iter()
        .find(|a| matches!(a.ty, AssetType::ClipMap | AssetType::ClipMapPvs))
    else {
        return Ok(String::new());
    };
    let r = Reader(load);
    let count = Reader::word(&asset.header, 312)?;
    if count == 0 {
        return Ok(String::new());
    }
    let table = Reader::ptr(&asset.header, 316)?;
    let mut out = String::new();
    for i in 0..count {
        let b = r.bytes(table.at(i * 168), 168)?;
        if Reader::word(b, 4)? != 8 {
            continue;
        }
        let name_id = u16::from_le_bytes([b[0], b[1]]);
        let word = |off: usize| Reader::word(b, off);
        let float = |off: usize| Reader::word(b, off).map(f32::from_bits);
        out.push_str(load.script_string(name_id).unwrap_or(""));
        out.push_str(&format!(
            "\t{} {}\t{} {}\t{} {} {}\t{} {} {}\t{}\t{}\t{}\n",
            word(8)? as i32,
            word(12)? as i32,
            word(24)? as i32,
            word(28)? as i32,
            float(52)?,
            float(56)?,
            float(60)?,
            float(64)?,
            float(68)?,
            float(72)?,
            float(104)?,
            float(116)?,
            word(88)?,
        ));
    }
    Ok(out)
}

/// The map's world fog volumes as `getworldfogscriptid` reads them, one per
/// line: the `GfxWorld` volume's packed word (bank bits, plane count - 1 in
/// bits 8..16, first plane from bit 16), its script id, and its planes.
fn capture_t6_fog_volumes(load: &ZoneLoad, h: &[u8]) -> Result<String, String> {
    use asset_world::world_t6::Reader;
    let r = Reader(load);
    let count = Reader::word(h, 308)? as u16;
    if count == 0 {
        return Ok(String::new());
    }
    let volumes = Reader::ptr(h, 312)?;
    let planes = Reader::ptr(h, 320)?;
    let mut out = String::new();
    for i in 0..u32::from(count) {
        let b = r.bytes(volumes.at(i * 100), 100)?;
        let packed = Reader::word(b, 12)?;
        let script_id = u16::from_le_bytes([b[32], b[33]]);
        let first = packed >> 16;
        let plane_count = ((packed >> 8) & 0xff) + 1;
        let mut text = Vec::with_capacity(plane_count as usize);
        for j in 0..plane_count {
            let c = r.xyzw(planes.at((first + j) * 16))?;
            text.push(format!("{} {} {} {}", c[0], c[1], c[2], c[3]));
        }
        out.push_str(&format!("{packed}\t{script_id}\t{}\n", text.join(";")));
    }
    Ok(out)
}
