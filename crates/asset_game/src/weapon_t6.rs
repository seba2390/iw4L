const PLANTED_MODELS: &[(&str, &str)] = &[("tactical_insertion", "t6_wpn_tac_insert_world")];

pub fn planted_model(name: &str) -> Option<&'static str> {
    let base = base_name(name);
    PLANTED_MODELS
        .iter()
        .find(|(t6, _)| *t6 == base)
        .map(|(_, model)| *model)
}

pub const T6_EFFECTS: &[&str] = &[
    "maps/zombie_tomb/fx_tomb_player_weather_rain",
    "maps/zombie_tomb/fx_tomb_player_weather_snow",
    "misc/fx_zombie_powerup_off",
    "misc/fx_zombie_powerup_grab",
    "misc/fx_zombie_powerup_wave",
    "misc/fx_zombie_mini_nuke_hotness",
    "misc/fx_equip_tac_insert_light_grn",
    "misc/fx_equip_tac_insert_light_red",
    "misc/fx_equip_tac_insert_exp",
    "weapon/bouncing_betty/fx_betty_explosion",
    "weapon/bouncing_betty/fx_betty_destroyed",
    "weapon/bouncing_betty/fx_betty_launch_dust",
    "weapon/bouncing_betty/fx_betty_light_green",
    "weapon/bouncing_betty/fx_betty_light_red",
    "weapon/trophy_system/fx_trophy_flash_lng",
    "weapon/trophy_system/fx_trophy_radius_detonation",
    "weapon/trophy_system/fx_trophy_light_friendly",
    "weapon/trophy_system/fx_trophy_light_enemy",
    "weapon/trophy_system/fx_trophy_deploy_impact",
    "weapon/grenade/fx_prox_grenade_scan_grn",
    "weapon/grenade/fx_prox_grenade_scan_red",
    "weapon/grenade/fx_prox_grenade_wrn_grn",
    "weapon/grenade/fx_prox_grenade_wrn_red",
    "weapon/grenade/fx_prox_grenade_impact_player_spwner",
    "weapon/grenade/fx_spark_disabled_weapon",
    "weapon/c4/fx_c4_light_green",
    "weapon/c4/fx_c4_light_red",
    "weapon/emp/fx_emp_explosion_equip",
    "explosions/fx_exp_equipment",
    "explosions/fx_exp_equipment_lg",
    "explosions/fx_flashbang",
    "weapon/grenade/fx_prox_grenade_exp",
    "weapon/satchel/fx_explosion_satchel_generic",
    "weapon/sensor_grenade/fx_sensor_exp_scan_friendly",
    "weapon/sensor_grenade/fx_sensor_exp_scan_enemy",
    "impacts/fx_flesh_hit_knife",
    "impacts/fx_flesh_hit_knife_mp",
];

pub const T6_EQUIPMENT_SOUNDS: &[&str] = &[
    "wpn_claymore_alert",
    "fly_betty_jump",
    "fly_betty_explo",
    "dst_equipment_destroy",
    "dst_disable_spark",
    "dst_tac_insert_break",
    "wpn_taser_mine_zap",
    "wpn_taser_mine_tacmask",
    "wpn_trophy_alert",
    "wpn_trophy_spin",
    "fly_sensor_nade_lp",
    "prj_bullet_impact_knife",
    "melee_hit",
];

pub const MELEE_WEAPON: &str = "knife_mp";

pub fn base_name(name: &str) -> &str {
    let mut base = name.strip_suffix("_mp").unwrap_or(name);
    for prefix in ["sf_", "dualoptic_", "gl_"] {
        if let Some(rest) = base.strip_prefix(prefix) {
            base = rest;
        }
    }
    for suffix in ["_dw", "_lh"] {
        if let Some(rest) = base.strip_suffix(suffix) {
            base = rest;
        }
    }
    base
}

pub fn capture_t6_string_table(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
) -> Option<crate::CapturedStringTable> {
    let table = fastfile_t6::string_table(load, asset)?;
    Some(crate::CapturedStringTable {
        name: table.name.to_owned(),
        columns: table.columns,
        rows: table.rows,
        cells: table.cells.into_iter().map(str::to_owned).collect(),
    })
}
