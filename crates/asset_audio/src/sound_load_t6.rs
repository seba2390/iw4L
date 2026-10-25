use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::sound_catalog::{CapturedAlias, CapturedSound, LoadedSoundPcm, SoundCatalog};
use crate::{AssetEdge, AssetEdgeReason, ZoneGame, ZoneOwner};
use asset_transport::{SoundAssetBank, snd_hash_name};
use fastfile_t6::{AssetType, Ptr, ZoneLoad};

const SND_BANK_ALIAS_COUNT: usize = 4;
const SND_BANK_ALIAS: usize = 8;
const SND_ALIAS_LIST: u32 = 20;
const SND_ALIAS_LIST_ID: usize = 4;
const SND_ALIAS_LIST_HEAD: usize = 8;
const SND_ALIAS_LIST_COUNT: usize = 12;
const SND_ALIAS: u32 = 96;
const SND_ALIAS_SECONDARY: usize = 12;
const SND_ALIAS_FLAGS0: usize = 24;

/// The namespace Black Ops 2's sounds are captured under.
const NAMESPACE: crate::AssetNamespace = crate::AssetNamespace::T6;

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn decode_ptr(raw: u32) -> Option<Ptr> {
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    Some(Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    })
}

pub fn t6_sound_banks(zone: &Path) -> (Vec<SoundAssetBank>, Vec<String>) {
    match zone.parent().and_then(Path::parent).and_then(Path::parent) {
        Some(root) => asset_transport::open_sound_asset_banks(&root.join("sound")),
        None => (
            Vec::new(),
            vec![format!("{}: no install root", zone.display())],
        ),
    }
}

#[derive(Default)]
struct SoundGlobals {
    curves: Vec<crate::CapturedSndCurve>,
    pans: Vec<(String, [f32; 6])>,
    groups: Vec<crate::MixerGroup>,
    mix: Option<crate::SoundMixNames>,
    /// How many duck groups the driver declares (each duck's attenuation
    /// has one volume per group).
    duck_groups: u32,
}

/// Each name hash at `at` in the rows of a count and pointer.
fn row_hashes(load: &ZoneLoad, header: &[u8], count_at: usize, size: u32, at: usize) -> Vec<u32> {
    let Some(base) = header
        .get(count_at + 4..count_at + 8)
        .and_then(|b| decode_ptr(le32(b, 0)))
    else {
        return Vec::new();
    };
    (0..le32(header, count_at))
        .map_while(|i| load.blocks.bytes(base.at(i * size), size as usize).ok())
        .map(|row| le32(row, at))
        .collect()
}

fn capture_globals(loads: &[&ZoneLoad]) -> SoundGlobals {
    let mut globals = SoundGlobals::default();
    for load in loads {
        for asset in load
            .assets
            .iter()
            .filter(|a| a.ty == AssetType::SndDriverGlobals)
        {
            if asset.header.len() < 28 {
                continue;
            }
            let rows = |count_at, pointer_at, size: u32| -> Vec<&[u8]> {
                let Some(base) = decode_ptr(le32(&asset.header, pointer_at)) else {
                    return Vec::new();
                };
                (0..le32(&asset.header, count_at))
                    .map_while(|i| load.blocks.bytes(base.at(i * size), size as usize).ok())
                    .collect()
            };
            let name = |b: &[u8]| {
                String::from_utf8_lossy(&b[..b[..32].iter().position(|&b| b == 0).unwrap_or(32)])
                    .into_owned()
            };
            let float = |b: &[u8], at| f32::from_bits(le32(b, at));
            if globals.groups.is_empty() {
                globals.groups = rows(4, 8, 80)
                    .iter()
                    .map(|b| crate::MixerGroup {
                        parent: le32(b, 68) as i32,
                        attenuation: f32::from(le16(b, 78)) / 65535.0,
                        category: None,
                    })
                    .collect();
            }
            if globals.curves.is_empty() {
                globals.curves = rows(12, 16, 100)
                    .iter()
                    .map(|b| crate::CapturedSndCurve {
                        name: name(b),
                        knots: (0..8)
                            .map(|i| (float(b, 36 + i * 8), float(b, 40 + i * 8)))
                            .collect(),
                    })
                    .collect();
            }
            // SndDriverGlobals: contexts (36 bytes: the type, then its eight
            // values) at 0x24, side-chain ducks (60 bytes) at 0x34 and futz
            // patches (100 bytes) at 0x3c, each named by the hash after its
            // 32-byte name.
            if globals.mix.is_none() && asset.header.len() >= 0x44 {
                globals.mix = Some(crate::SoundMixNames {
                    contexts: rows(0x24, 0x28, 36)
                        .iter()
                        .map(|b| (le32(b, 0), std::array::from_fn(|i| le32(b, 4 + i * 4))))
                        .collect(),
                    ducks: row_hashes(load, &asset.header, 0x34, 60, 0x20),
                    futzes: row_hashes(load, &asset.header, 0x3c, 100, 0x20),
                    snapshots: Vec::new(),
                });
            }
            if globals.duck_groups == 0 {
                globals.duck_groups = le32(&asset.header, 0x1c);
            }
            if globals.pans.is_empty() {
                globals.pans = rows(20, 24, 60)
                    .iter()
                    .map(|b| (name(b), std::array::from_fn(|i| float(b, 36 + i * 4))))
                    .collect();
            }
        }
    }
    globals
}

/// A bank's ducks (76 bytes each, count and pointer at 0x18): the name's
/// hash at 0x20, fade-in, fade-out, start delay, distance and length at
/// 0x24 to 0x34, and a pointer to one volume per duck group at 0x40.
fn bank_ducks(load: &ZoneLoad, header: &[u8], groups: u32) -> Vec<crate::SoundDuck> {
    const SIZE: u32 = 76;
    let Some(base) = header.get(0x1c..0x20).and_then(|b| decode_ptr(le32(b, 0))) else {
        return Vec::new();
    };
    let float = |b: &[u8], at| f32::from_bits(le32(b, at));
    (0..le32(header, 0x18))
        .map_while(|i| load.blocks.bytes(base.at(i * SIZE), SIZE as usize).ok())
        .map(|row| crate::SoundDuck {
            hash: le32(row, 0x20),
            fade_in: float(row, 0x24),
            fade_out: float(row, 0x28),
            start_delay: float(row, 0x2c),
            distance: float(row, 0x30),
            length: float(row, 0x34),
            attenuation: decode_ptr(le32(row, 0x40))
                .and_then(|at| load.blocks.bytes(at, groups as usize * 4).ok())
                .map(|bytes| {
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_le_bytes(*b))
                        .collect()
                })
                .unwrap_or_else(Vec::new),
        })
        .collect()
}

fn sound_rawfile(name: &str) -> bool {
    name.starts_with("rumble/")
        || name == "soundaliases/channels.def"
        || name.ends_with("/soundaliases/channels.def")
        || ((name.starts_with("maps/createfx/") || name.starts_with("maps/mp/"))
            && name.ends_with("_fx.gsc"))
}

pub fn capture_t6_sounds<'n>(
    zone: &Path,
    loads: &[&ZoneLoad],
    banks: &[SoundAssetBank],
    names: impl IntoIterator<Item = &'n str>,
) -> (SoundCatalog, Vec<String>, Vec<String>) {
    let game = ZoneGame::T6;
    let globals = capture_globals(loads);
    let mut report = Vec::new();
    let mut lists: HashMap<u32, (&ZoneLoad, Ptr, u32)> = HashMap::new();
    for &load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let (Some(count), Some(array)) = (
                bank.header
                    .get(SND_BANK_ALIAS_COUNT..SND_BANK_ALIAS_COUNT + 4),
                bank.header
                    .get(SND_BANK_ALIAS..SND_BANK_ALIAS + 4)
                    .and_then(|b| decode_ptr(le32(b, 0))),
            ) else {
                continue;
            };
            for i in 0..le32(count, 0) {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                if let Some(head) = decode_ptr(le32(list, SND_ALIAS_LIST_HEAD)) {
                    lists.entry(le32(list, SND_ALIAS_LIST_ID)).or_insert((
                        load,
                        head,
                        le32(list, SND_ALIAS_LIST_COUNT),
                    ));
                }
            }
        }
    }

    let mut catalog = SoundCatalog::default();
    catalog.set_capture_zone(ZoneOwner::from_zone_path(zone));
    catalog.set_capture_game(game);
    let mut ignored_rawfiles = 0usize;
    for &load in loads.iter().rev() {
        for asset in load.assets.iter().filter(|a| a.ty == AssetType::RawFile) {
            let rawfile = || -> Result<Option<_>, &str> {
                let header = asset.header.get(..12).ok_or("truncated header")?;
                let name = decode_ptr(le32(header, 0))
                    .and_then(|p| load.blocks.cstr(p).ok())
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .ok_or("invalid name")?
                    .replace('\\', "/")
                    .to_ascii_lowercase();
                if !sound_rawfile(&name) {
                    return Ok(None);
                }
                let len = le32(header, 4) as usize;
                let data = if len == 0 {
                    &[]
                } else {
                    decode_ptr(le32(header, 8))
                        .and_then(|p| load.blocks.bytes(p, len).ok())
                        .ok_or("invalid data span")?
                };
                Ok(Some((name, data)))
            };
            match rawfile() {
                Ok(Some((name, data))) => catalog.ingest_rawfile(&name, data, false),
                Ok(None) => ignored_rawfiles += 1,
                Err(error) => report.push(format!("t6 sound rawfile: {error}")),
            }
        }
    }
    let (rawfile_count, rawfile_bytes) = catalog
        .rawfiles_in(NAMESPACE)
        .fold((0usize, 0usize), |(count, bytes), (_, data)| {
            (count + 1, bytes + data.len())
        });
    report.push(format!(
        "t6 audio rawfiles: n={rawfile_count} bytes={rawfile_bytes} ignored={ignored_rawfiles}"
    ));
    if !globals.groups.is_empty() {
        catalog.ingest_mixer_groups(NAMESPACE, globals.groups.clone());
    }
    if let Some(mix) = &globals.mix {
        // The driver's contexts, the `mature` one starting `explicit` (the
        // game setting the client hands its sound thread, on by default).
        catalog.ingest_sound_contexts(
            NAMESPACE,
            crate::SoundContexts {
                defs: mix
                    .contexts
                    .iter()
                    .map(|&(ty, values)| crate::SoundContextDef { ty, values })
                    .collect(),
                initial: vec![(snd_hash_name("mature"), snd_hash_name("explicit"))],
            },
        );
    }
    if let Some(mut mix) = globals.mix.clone() {
        // Each bank's mixer snapshots (76 bytes, at 0x18), every bank
        // loaded.
        for &load in loads {
            for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
                if bank.header.len() >= 0x20 {
                    mix.snapshots
                        .extend(row_hashes(load, &bank.header, 0x18, 76, 0x20));
                    catalog.ingest_ducks(
                        NAMESPACE,
                        bank_ducks(load, &bank.header, globals.duck_groups),
                    );
                }
            }
        }
        catalog.set_sound_mix_names(mix);
    }
    let mut loaded: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut filled = Vec::new();
    let mut queue: Vec<String> = names.into_iter().map(str::to_owned).collect();
    queue.reverse();
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        let name = name.as_str();
        let Some(&(load, head, count)) = lists.get(&snd_hash_name(name)) else {
            report.push(format!("t6 sound {name}: no alias"));
            continue;
        };
        catalog.set_playback_ms(NAMESPACE, name, playback_ms(load, head, count, banks));
        let mut aliases = Vec::with_capacity(count as usize);
        for k in 0..count {
            let Ok(row) = load
                .blocks
                .bytes(head.at(k * SND_ALIAS), SND_ALIAS as usize)
            else {
                continue;
            };
            let asset = le32(row, 16);
            loaded
                .entry(asset)
                .or_insert_with(|| load_asset(&mut catalog, banks, asset, name, &mut report));
            let loaded_name = format!("t6/{asset:08x}");
            let secondary = decode_ptr(le32(row, SND_ALIAS_SECONDARY))
                .and_then(|p| load.blocks.cstr(p).ok())
                .and_then(|b| std::str::from_utf8(b).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if let Some(secondary) = &secondary
                && seen.insert(secondary.clone())
            {
                queue.push(secondary.clone());
            }
            let flags = le32(row, SND_ALIAS_FLAGS0);
            let curves = le32(row, SND_ALIAS_FLAGS0 + 4);
            let pan = globals.pans.get(usize::from(row[92]));
            aliases.push(CapturedAlias {
                alias_name: name.to_owned(),
                secondary,
                loaded_name: Some(loaded_name.clone()),
                loaded: AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                file_type: Some(1),
                file_name: Some(loaded_name),
                vol_min: f32::from(le16(row, 60)) / 65535.0,
                vol_max: f32::from(le16(row, 62)) / 65535.0,
                pitch_min: f32::from(le16(row, 64)) / 32767.0,
                pitch_max: f32::from(le16(row, 66)) / 32767.0,
                dist_min: f32::from(le16(row, 68)),
                dist_max: f32::from(le16(row, 70)),
                start_delay: i32::from(le16(row, 54)),
                flags: Some(flags),
                looping: Some(flags & 1 != 0),
                volume_falloff: globals.curves.get(((curves >> 2) & 0x3f) as usize).cloned(),
                near_falloff: globals
                    .curves
                    .get(((curves >> 14) & 0x3f) as usize)
                    .cloned(),
                speaker_map: Some(
                    pan.map_or_else(|| format!("t6/pan/{}", row[92]), |(name, _)| name.clone()),
                ),
                t6_speaker_pan: pan.map(|(_, gains)| *gains),
                limit_count: Some(row[93]),
                entity_limit_count: Some(row[94]),
                // The duck group whose volume the voice plays at.
                snapshot_group: Some(row[95]),
                voice_snapshot: None,
                // The context type and the value the variant plays in.
                context: Some([le32(row, 36), le32(row, 40)]),
                voice_priority: Some(crate::VoicePriority {
                    thresholds: [row[86], row[87]],
                    values: [row[90], row[91]],
                    distance_max: f32::from(le16(row, 70)),
                }),
                probability: f32::from(row[88]) / 255.0,
                ..Default::default()
            });
        }
        if aliases.is_empty() {
            continue;
        }
        catalog.ingest_sound(CapturedSound {
            name: name.to_owned(),
            aliases,
            game: ZoneGame::T6,
            zone: catalog.capture_zone_for_ingest(),
        });
        filled.push(name.to_owned());
    }
    catalog.resolve_loaded_edges();
    catalog.publish();
    (catalog, filled, report)
}

/// The engine's playback time of an alias (`soundgetplaybacktime`): the
/// longest of its first 64 variants, each its asset's length in msec and
/// 500 more from a loaded bank, 250 more from a streamed one; a variant with
/// no asset makes it 0, one whose asset is in no bank is passed over, and
/// -1 when none counts.
fn playback_ms(load: &ZoneLoad, head: Ptr, count: u32, banks: &[SoundAssetBank]) -> i32 {
    const VARIANTS: u32 = 64;
    const LOADED_EXTRA_MS: u32 = 500;
    const STREAMED_EXTRA_MS: u32 = 250;
    let mut longest = -1;
    for k in 0..count.min(VARIANTS) {
        let Ok(row) = load
            .blocks
            .bytes(head.at(k * SND_ALIAS), SND_ALIAS as usize)
        else {
            continue;
        };
        let asset = le32(row, 16);
        if asset == 0 {
            return 0;
        }
        let found = |loaded: bool| {
            banks.iter().find_map(|bank| {
                let streamed = bank
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sabs"));
                (streamed != loaded).then(|| bank.entry(asset)).flatten()
            })
        };
        let Some((entry, extra)) = found(true)
            .map(|entry| (entry, LOADED_EXTRA_MS))
            .or_else(|| found(false).map(|entry| (entry, STREAMED_EXTRA_MS)))
        else {
            continue;
        };
        let Some(rate) = entry.frame_rate() else {
            continue;
        };
        let ms = (entry.frame_count.wrapping_mul(1000) / rate + extra) as i32;
        longest = longest.max(ms);
    }
    longest
}

fn load_asset(
    catalog: &mut SoundCatalog,
    banks: &[SoundAssetBank],
    id: u32,
    alias: &str,
    report: &mut Vec<String>,
) -> Option<String> {
    let Some((bank, entry)) = banks
        .iter()
        .find_map(|bank| bank.entry(id).map(|entry| (bank, entry)))
    else {
        report.push(format!("t6 sound {alias}: asset {id:08x} in no bank"));
        return None;
    };
    let source = crate::SabMediaSource {
        bank: bank.path().to_owned(),
        entry,
    };
    if let crate::SabCodec::Unsupported(format) = source.codec() {
        report.push(format!(
            "t6 sound {alias}: asset {id:08x} has unsupported SAB codec {format}"
        ));
    }
    let sound = LoadedSoundPcm::from_sab(source, ZoneGame::T6, catalog.capture_zone_for_ingest());
    let name = sound.name.clone();
    catalog.ingest_loaded(sound);
    Some(name)
}

pub fn t6_sound_names(loads: &[&ZoneLoad]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let Some(count) = bank.header.get(4..8).map(|b| le32(b, 0)) else {
                continue;
            };
            let Some(array) = bank.header.get(8..12).and_then(|b| decode_ptr(le32(b, 0))) else {
                continue;
            };
            for i in 0..count {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                let Some(name) = decode_ptr(le32(list, 0))
                    .and_then(|p| load.blocks.cstr(p).ok())
                    .and_then(|b| std::str::from_utf8(b).ok())
                else {
                    continue;
                };
                if snd_hash_name(name) == le32(list, 4) {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    names.into_iter().collect()
}
