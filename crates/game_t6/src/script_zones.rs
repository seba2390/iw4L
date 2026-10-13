//! Where Black Ops 2 keeps the compiled scripts a zombies map runs.

use std::sync::OnceLock;

pub static SCRIPT_ZONE_FORMAT: asset_core::ScriptZoneFormat = asset_core::ScriptZoneFormat {
    game: asset_core::FamilyId::T6,
    script_zones,
    scripts,
};

/// The engine loads the common zone, its patch, the gametype's zone, the map
/// and the map's own patch, in that order.
fn script_zones(map: &str) -> Vec<String> {
    if !map.starts_with("zm_") {
        return Vec::new();
    }
    vec![
        "common_zm".to_owned(),
        "patch_zm".to_owned(),
        format!("so_zclassic_{map}"),
        map.to_owned(),
        format!("{map}_patch"),
    ]
}

fn schema() -> Result<&'static fastfile_t6::schema::Schema, String> {
    static SCHEMA: OnceLock<Result<fastfile_t6::schema::Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| fastfile_t6::schema::parse().map_err(|error| format!("{error:?}")))
        .as_ref()
        .map_err(Clone::clone)
}

fn scripts(content: &[u8]) -> asset_core::ZoneScripts {
    let mut out = asset_core::ZoneScripts::default();
    let schema = match schema() {
        Ok(schema) => schema,
        Err(error) => {
            out.report.push(format!("t6 zone schema: {error}"));
            return out;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema, content, |_, _| true);
    if let Err(error) = walked {
        out.report.push(format!("t6 zone walk: {error:?}"));
        return out;
    }
    for module in fastfile_t6::scripts(&load).modules {
        out.modules
            .push((module.name.to_owned(), module.bytes.to_vec()));
    }
    for asset in &load.assets {
        if let Some(file) = fastfile_t6::raw_file(&load, asset)
            && (file.name.ends_with(".cfg")
                || file.name == "radiant/keys.txt"
                || file.name.starts_with("info/"))
            && let Ok(text) = std::str::from_utf8(file.bytes)
        {
            out.configs.push((file.name.to_owned(), text.to_owned()));
        }
        if let Some(table) = fastfile_t6::string_table(&load, asset) {
            out.tables.push(asset_core::ZoneTable {
                name: table.name.to_owned(),
                columns: table.columns,
                rows: table.rows,
                cells: table.cells.into_iter().map(str::to_owned).collect(),
            });
        }
        // The data definition's newest version.
        if let Some(ddl) = fastfile_t6::ddl::ddl(&load, asset)
            && let Some(def) = ddl.versions.into_iter().next()
        {
            out.layouts.push(asset_core::BitLayout {
                name: ddl.name,
                bits: def.structs.first().map_or(0, |root| root.size),
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
                            })
                            .collect(),
                    })
                    .collect(),
                enums: def.enums.into_iter().map(|e| e.members).collect(),
            });
        }
    }
    let anim = |anim: Option<fastfile_t6::zbarrier::BarrierAnim>| {
        anim.map(|anim| asset_core::BarrierAnim {
            name: anim.name,
            timing: anim.timing,
        })
    };
    for weapon in load.weapons().flatten() {
        if let Some(weapon) = zone_weapon(&weapon) {
            out.weapons.push(weapon);
        }
    }
    for barrier in load.barriers() {
        out.barriers.push(asset_core::BarrierType {
            name: barrier.name,
            piece_count: barrier.piece_count,
            auto_hide_open_pieces: barrier.auto_hide_open_pieces,
            taunts: barrier.taunts,
            reach_through_attacks: barrier.reach_through_attacks,
            taunt_anim_state: barrier.taunt_anim_state,
            reach_through_anim_state: barrier.reach_through_anim_state,
            attack_slots: barrier.attack_slots,
            attack_slot_horz_offset: barrier.attack_slot_horz_offset,
            pieces: barrier
                .pieces
                .into_iter()
                .map(|piece| asset_core::BarrierPiece {
                    models: piece.models,
                    open_anim: anim(piece.tear_anim),
                    close_anim: anim(piece.board_anim),
                    anim_state: piece.anim_state,
                    anim_substate: piece.anim_substate,
                    upgraded_lives: piece.upgraded_lives,
                })
                .collect(),
        });
    }
    out
}

/// A weapon and the weapons the engine registers with it: its attachments'
/// alternates (unless it is itself an alternate mode), its alternate, its
/// attachments' dual-wield weapons, its dual-wield weapon, then its stand,
/// crouch and prone mounted weapons.
fn zone_weapon(weapon: &fastfile_t6::weapon::WeaponView<'_>) -> Option<asset_core::ZoneWeapon> {
    use fastfile_t6::weapon::{def, inventory_type, variant};
    let name = weapon.name()?;
    if !weapon.has_def() {
        return None;
    }
    let mut companions = Vec::new();
    let mut add = |name: Option<&str>| {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            companions.push(name.to_owned());
        }
    };
    if weapon.def_i32(def::INVENTORY_TYPE) != inventory_type::ALTMODE {
        for unique in weapon.attachment_uniques() {
            add(unique.alt_weapon());
        }
    }
    add(weapon.variant_str(variant::ALT_WEAPON_NAME));
    for unique in weapon.attachment_uniques() {
        add(unique.dual_wield_weapon());
    }
    if weapon.def_bool(def::DUAL_WIELD) {
        add(weapon.def_str(def::DUAL_WIELD_WEAPON));
    }
    for mounted in def::MOUNTED_WEAPONS {
        add(weapon.def_str(mounted));
    }
    let text = |off| weapon.def_str(off).unwrap_or("").to_owned();
    Some(asset_core::ZoneWeapon {
        name: name.to_owned(),
        companions,
        fire_sound: text(def::FIRE_SOUND),
        fire_sound_player: text(def::FIRE_SOUND_PLAYER),
        pickup_sound: text(def::PICKUP_SOUND),
        pickup_sound_player: text(def::PICKUP_SOUND_PLAYER),
        retrievable: weapon.def_bool(def::RETRIEVABLE),
        watcher: weapon.def_bool(def::WATCHER),
        clip_size: weapon.variant_i32(variant::CLIP_SIZE),
        start_ammo: weapon.def_i32(def::START_AMMO),
        max_ammo: weapon.def_i32(def::MAX_AMMO),
        ammo_count_clip_relative: weapon.def_bool(def::AMMO_COUNT_CLIP_RELATIVE),
        clip_only: weapon.def_bool(def::CLIP_ONLY),
        tank_life_time: weapon.def_i32(def::TANK_LIFE_TIME),
        melee_damage: weapon.def_i32(def::MELEE_DAMAGE),
        weap_type: weapon.def_i32(def::WEAP_TYPE),
        weap_class: weapon.def_i32(def::WEAP_CLASS),
        inventory_type: weapon.def_i32(def::INVENTORY_TYPE),
        fire_type: weapon.def_i32(def::FIRE_TYPE),
        offhand_slot: weapon.def_i32(def::OFFHAND_SLOT),
        alt_weapon: weapon
            .variant_str(variant::ALT_WEAPON_NAME)
            .unwrap_or("")
            .to_owned(),
        dual_wield: weapon.def_bool(def::DUAL_WIELD),
        dual_wield_weapon: text(def::DUAL_WIELD_WEAPON),
        location_damage: weapon
            .def_f32_array::<{ def::HIT_LOCATIONS }>(def::LOCATION_DAMAGE_MULTIPLIERS)
            .map_or_else(Vec::new, |scales| scales.to_vec()),
        world_models: (0..def::WORLD_MODEL_COUNT)
            .map(|index| {
                weapon
                    .def_asset_array_name(def::WORLD_MODEL, index)
                    .unwrap_or("")
                    .to_owned()
            })
            .collect(),
        view_models: (0..def::WORLD_MODEL_COUNT)
            .map(|index| {
                weapon
                    .def_asset_array_name(def::GUN_XMODEL, index)
                    .unwrap_or("")
                    .to_owned()
            })
            .collect(),
        turret_convergence: def::TURRET_CONVERGENCE.map(|off| weapon.def_f32(off)),
        turret_arcs: def::TURRET_ARCS.map(|off| weapon.def_f32(off)),
        turret_spread: def::TURRET_SPREAD.map(|off| weapon.def_f32(off)),
        turret_turn_rates: def::TURRET_TURN_RATES.map(|off| weapon.def_f32(off)),
        turret_max_range: weapon.def_f32(def::TURRET_MAX_RANGE),
        turret_scan: (
            weapon.def_f32(def::TURRET_SCAN_SPEED),
            weapon.def_f32(def::TURRET_SCAN_ACCEL),
            weapon.def_i32(def::TURRET_SCAN_PAUSE),
        ),
        damage_curve: weapon.damage_curve(),
        fire_barrels: weapon.def_i32(def::FIRE_BARRELS),
        penetrate_type: weapon.def_i32(def::PENETRATE_TYPE),
        rifle_bullet: weapon.def_bool(def::RIFLE_BULLET),
        armor_piercing: weapon.def_bool(def::ARMOR_PIERCING),
        bullet_bounce: weapon.def_bool(def::BULLET_BOUNCE),
        bullet_impact_explode: weapon.def_bool(def::BULLET_IMPACT_EXPLODE),
        flat_spread: weapon.variant_i32(variant::SPREAD_FLAGS) & (1 << variant::SPREAD_FLAT_BIT)
            != 0,
        projectile_activate_dist: weapon.def_i32(def::PROJECTILE_ACTIVATE_DIST),
    })
}
