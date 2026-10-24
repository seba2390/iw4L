use super::*;
use fastfile_t5::{WeaponView, size as sz};

impl WeaponCatalog {
    pub fn capture_t5(&mut self, w: WeaponView<'_, '_>) {
        let geometry = *w.geometry();
        let Some(name) = w.cstr(geometry.name) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = bare_owned(w.cstr(geometry.gun_xmodel_name));
        let hand_xmodel = bare_owned(w.cstr(geometry.hand_xmodel_name));
        let sz_xanims = read_sz_xanims_t5(&w, false);
        let (facts, game_bytes) = capture_t5_body_facts(&w);
        self.entries.push(CatalogWeapon {
            namespace: self
                .capture_ns
                .expect("asset capture requires an explicit family"),
            impact_payload: w
                .def_str(sz::WEAPON_DEF_IMPACT_PAYLOAD_OFF)
                .map(str::to_owned),
            alternate_weapon: w
                .cstr(geometry.alternate_weapon_name)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle_center_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_RETICLE_CENTER_OFF)),
            reticle_side_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_RETICLE_SIDE_OFF)),
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(|p| (p.block, p.offset)),
            display_name_key: w
                .cstr(geometry.display_name)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: {
                let mut reticle = leftover_t5_reticle(&w);
                reticle.center_material =
                    bare_owned(w.cstr(geometry.reticle_center_name)).or(reticle.center_material);
                reticle.side_material =
                    bare_owned(w.cstr(geometry.reticle_side_name)).or(reticle.side_material);
                reticle
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: leftover_t5_overlay_pick(&w).and_then(|(name, _)| name),
            overlay_image: None,
            overlay_material_slot: leftover_t5_overlay_pick(&w).and_then(|(_, slot)| slot),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            attached_models: Default::default(),
            t6_clip_models: Default::default(),
            t6_attachments: Vec::new(),
            t6_attachment_stats: Vec::new(),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            hud_icon: bare_owned(w.def_asset_name(sz::WEAPON_DEF_HUD_ICON_OFF)),
            hud_icon_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_HUD_ICON_OFF)),

            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: w.def_i32(0x324),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: bare_owned(w.cstr(geometry.kill_icon_name)),
            kill_icon_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_KILL_ICON_OFF)),
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_PROJ_TRAIL_EFFECT_OFF)),
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: t5_to_iw4_ptr(w.def_slot(sz::WEAPON_DEF_PROJ_IGNITION_EFFECT_OFF)),
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,

            camo_models: WeaponCamoModels::default(),
            skin_parent: w
                .def_str(sz::WEAPON_DEF_PARENT_WEAPON_NAME_OFF)
                .map(str::to_owned),
            world_model: bare_owned(w.cstr(geometry.world_model_name)),
            projectile_model: bare_owned(w.cstr(geometry.projectile_model_name)),
            knife_xmodel: None,
            rocket_model: bare_owned(w.cstr(geometry.rocket_model_name)),
            sz_xanims,
            dual_wield_weapon: w
                .def_str(sz::WEAPON_DEF_DUAL_WIELD_WEAPON_NAME_OFF)
                .map(str::to_owned),
            grenade_weapon: w
                .def_str(sz::WEAPON_DEF_GRENADE_WEAPON_OFF)
                .map(str::to_owned),
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: read_sz_xanims_t5(&w, true),
            hide_tags: w.hide_tags().into_iter().map(str::to_owned).collect(),
            sounds: leftover_t5_sounds(&w),
            combat_fx: WeaponCombatFx::empty(crate::AssetNamespace::T5),
            combat_slots: CombatFxSlots::default(),
            facts,
            game_bytes,
        });
        let last = self.entries.last_mut().expect("just pushed");
        let (fx, slots) = leftover_t5_combat_fx(&w);
        last.combat_fx = fx;
        last.combat_slots = slots;
        if normalize_weapon_name(&last.name) == "hatchet" {
            last.facts.weap_class = 9;
            last.facts.stick_to_players = true;
        }
    }
}

pub fn t5_inline_note_alias<'a>(note: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, tail) = note.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(tail)
        .filter(|alias| !alias.is_empty())
}

pub(super) fn remap_t5_weap_class(raw: i32) -> i32 {
    match raw {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        5 => 6,
        6 => 7,
        7 => 8,
        8 => 10,
        10 => 11,
        other => other,
    }
}

pub(super) fn read_sz_xanims_t5(
    w: &WeaponView<'_, '_>,
    left: bool,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let t5 = w.xanim_names().map(|name| name.map(str::to_owned));
    let mut out = remap_t5_sz_xanims(&t5);
    if left {
        for (src, dst) in LEFT_ANIM_SLOTS.iter().enumerate() {
            if let Some(dst) = *dst {
                out[dst] = t5[src].clone();
            }
        }
    }
    out
}

/// The left viewmodel's slot of each of the weapon's own (left gun's)
/// animation slots.
static LEFT_ANIM_SLOTS: &[Option<usize>] = &{
    use sz::{WEAPON_XANIM_COUNT, weap_anim as a};
    let mut slots = [None; WEAPON_XANIM_COUNT];
    slots[a::IDLE_LEFT] = Some(weap_anim::IDLE);
    slots[a::EMPTY_IDLE_LEFT] = Some(weap_anim::EMPTY_IDLE);
    slots[a::FIRE_LEFT] = Some(weap_anim::FIRE);
    slots[a::LASTSHOT_LEFT] = Some(weap_anim::LASTSHOT);
    slots[a::RELOAD_LEFT] = Some(weap_anim::RELOAD);
    slots[a::RELOAD_EMPTY_LEFT] = Some(weap_anim::RELOAD_EMPTY);
    slots
};

pub(super) fn remap_t5_sz_xanims(t5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (src, dst) in RUNTIME_ANIM_SLOTS.iter().enumerate() {
        if let Some(dst) = *dst
            && src < t5.len()
        {
            out[dst] = t5[src].clone();
        }
    }
    out
}

/// The runtime slot of each of the weapon's own animation slots.
static RUNTIME_ANIM_SLOTS: &[Option<usize>] = &{
    use sz::{WEAPON_XANIM_COUNT, weap_anim as t5_anim};
    const PAIRS: [(usize, usize); 59] = [
        (t5_anim::IDLE, weap_anim::IDLE),
        (t5_anim::EMPTY_IDLE, weap_anim::EMPTY_IDLE),
        (t5_anim::FIRE, weap_anim::FIRE),
        (t5_anim::HOLD_FIRE, weap_anim::HOLD_FIRE),
        (t5_anim::LASTSHOT, weap_anim::LASTSHOT),
        (t5_anim::RECHAMBER, weap_anim::RECHAMBER),
        (t5_anim::MELEE, weap_anim::MELEE),
        (t5_anim::MELEE_CHARGE, weap_anim::MELEE_CHARGE),
        (t5_anim::RELOAD, weap_anim::RELOAD),
        (t5_anim::RELOAD_EMPTY, weap_anim::RELOAD_EMPTY),
        (t5_anim::RELOAD_START, weap_anim::RELOAD_START),
        (t5_anim::RELOAD_END, weap_anim::RELOAD_END),
        (t5_anim::RELOAD_QUICK, weap_anim_extra::RELOAD_QUICK),
        (
            t5_anim::RELOAD_QUICK_EMPTY,
            weap_anim_extra::RELOAD_QUICK_EMPTY,
        ),
        (t5_anim::RAISE, weap_anim::RAISE),
        (t5_anim::FIRST_RAISE, weap_anim::FIRST_RAISE),
        (t5_anim::DROP, weap_anim::DROP),
        (t5_anim::ALT_RAISE, weap_anim::ALT_RAISE),
        (t5_anim::ALT_DROP, weap_anim::ALT_DROP),
        (t5_anim::QUICK_RAISE, weap_anim::QUICK_RAISE),
        (t5_anim::QUICK_DROP, weap_anim::QUICK_DROP),
        (t5_anim::EMPTY_RAISE, weap_anim::EMPTY_RAISE),
        (t5_anim::EMPTY_DROP, weap_anim::EMPTY_DROP),
        (t5_anim::SPRINT_IN, weap_anim::SPRINT_IN),
        (t5_anim::SPRINT_LOOP, weap_anim::SPRINT_LOOP),
        (t5_anim::SPRINT_OUT, weap_anim::SPRINT_OUT),
        (t5_anim::DETONATE, weap_anim::DETONATE),
        (t5_anim::NIGHTVISION_WEAR, weap_anim::NIGHTVISION_WEAR),
        (t5_anim::NIGHTVISION_REMOVE, weap_anim::NIGHTVISION_REMOVE),
        (t5_anim::ADS_FIRE, weap_anim::ADS_FIRE),
        (t5_anim::ADS_LASTSHOT, weap_anim::ADS_LASTSHOT),
        (t5_anim::ADS_RECHAMBER, weap_anim::ADS_RECHAMBER),
        (t5_anim::ADS_UP, weap_anim::ADS_UP),
        (t5_anim::ADS_DOWN, weap_anim::ADS_DOWN),
        (t5_anim::RELOAD_RIGHT, weap_anim_extra::RELOAD_RIGHT),
        (t5_anim::SPRINT_EMPTY_IN, weap_anim_extra::SPRINT_EMPTY_IN),
        (
            t5_anim::SPRINT_EMPTY_LOOP,
            weap_anim_extra::SPRINT_EMPTY_LOOP,
        ),
        (t5_anim::SPRINT_EMPTY_OUT, weap_anim_extra::SPRINT_EMPTY_OUT),
        (t5_anim::LOWREADY_IN, weap_anim_extra::LOWREADY_IN),
        (t5_anim::LOWREADY_LOOP, weap_anim_extra::LOWREADY_LOOP),
        (t5_anim::LOWREADY_OUT, weap_anim_extra::LOWREADY_OUT),
        (t5_anim::CONT_FIRE_IN, weap_anim_extra::CONT_FIRE_IN),
        (t5_anim::CONT_FIRE_LOOP, weap_anim_extra::CONT_FIRE_LOOP),
        (t5_anim::CONT_FIRE_OUT, weap_anim_extra::CONT_FIRE_OUT),
        (t5_anim::DEPLOY, weap_anim_extra::DEPLOY),
        (t5_anim::BREAKDOWN, weap_anim_extra::BREAKDOWN),
        (t5_anim::DTP_IN, weap_anim_extra::DTP_IN),
        (t5_anim::DTP_LOOP, weap_anim_extra::DTP_LOOP),
        (t5_anim::DTP_OUT, weap_anim_extra::DTP_OUT),
        (t5_anim::DTP_EMPTY_IN, weap_anim_extra::DTP_EMPTY_IN),
        (t5_anim::DTP_EMPTY_LOOP, weap_anim_extra::DTP_EMPTY_LOOP),
        (t5_anim::DTP_EMPTY_OUT, weap_anim_extra::DTP_EMPTY_OUT),
        (t5_anim::SLIDE_IN, weap_anim_extra::SLIDE_IN),
        (t5_anim::MANTLE, weap_anim_extra::MANTLE),
        (t5_anim::SPRINT_CAMERA, weap_anim_extra::SPRINT_CAMERA),
        (t5_anim::DTP_IN_CAMERA, weap_anim_extra::DTP_IN_CAMERA),
        (t5_anim::DTP_LOOP_CAMERA, weap_anim_extra::DTP_LOOP_CAMERA),
        (t5_anim::DTP_OUT_CAMERA, weap_anim_extra::DTP_OUT_CAMERA),
        (t5_anim::MANTLE_CAMERA, weap_anim_extra::MANTLE_CAMERA),
    ];
    let mut slots = [None; WEAPON_XANIM_COUNT];
    let mut i = 0;
    while i < PAIRS.len() {
        slots[PAIRS[i].0] = Some(PAIRS[i].1);
        i += 1;
    }
    slots
};

pub(super) fn t5_to_iw4_ptr(p: Option<fastfile_t5::Ptr>) -> Option<Ptr> {
    p.map(|p| Ptr {
        block: p.block,
        offset: p.offset,
    })
}

/// A material or model name as the catalog keys it.
fn bare_owned(name: Option<&str>) -> Option<String> {
    name.map(asset_core::AssetRef::bare_name)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(super) fn leftover_t5_offhand_class(raw: i32) -> i32 {
    match raw {
        4 => 5,
        other => other,
    }
}

pub(super) fn leftover_t5_ads_rate(trans_ms: i32, stored: f32) -> f32 {
    if stored > 0.0 {
        stored
    } else if trans_ms > 0 {
        1.0 / trans_ms as f32
    } else {
        0.0
    }
}

pub(super) fn leftover_t5_reticle(w: &WeaponView<'_, '_>) -> WeaponReticleAssets {
    WeaponReticleAssets {
        center_material: bare_owned(w.def_asset_name(sz::WEAPON_DEF_RETICLE_CENTER_OFF)),
        side_material: bare_owned(w.def_asset_name(sz::WEAPON_DEF_RETICLE_SIDE_OFF)),
        center_authored: w.def_slot(sz::WEAPON_DEF_RETICLE_CENTER_OFF).is_some(),
        side_authored: w.def_slot(sz::WEAPON_DEF_RETICLE_SIDE_OFF).is_some(),
        center_size: w.def_i32(sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF),
        side_size: w.def_i32(sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF),
        ..WeaponReticleAssets::default()
    }
}

pub(super) fn leftover_t5_overlay_pick(
    w: &WeaponView<'_, '_>,
) -> Option<(Option<String>, Option<Ptr>)> {
    let geometry = w.geometry();
    geometry.variant?;
    let hi = bare_owned(w.cstr(geometry.overlay_material_name))
        .or_else(|| bare_owned(w.variant_asset_name(sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF)));
    let lo = bare_owned(w.cstr(geometry.overlay_material_lowres_name))
        .or_else(|| bare_owned(w.variant_asset_name(sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF)));
    let hi_slot = t5_to_iw4_ptr(w.variant_slot(sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF));
    let lo_slot = t5_to_iw4_ptr(w.variant_slot(sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF));
    if hi.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((hi, hi_slot));
    }
    if lo.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((lo, lo_slot));
    }
    Some((hi.or(lo), hi_slot.or(lo_slot)))
}

pub(super) fn leftover_t5_zoom_fov(w: &WeaponView<'_, '_>) -> f32 {
    leftover_t5_first_positive_fov(
        w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_FOV3_OFF),
        w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF),
        w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF),
    )
}

pub(super) fn leftover_t5_first_positive_fov(fov1: f32, fov2: f32, fov3: f32) -> f32 {
    for fov in [fov1, fov2, fov3] {
        if fov.is_finite() && fov > 0.0 && fov < 180.0 {
            return fov;
        }
    }
    0.0
}

pub(super) fn leftover_t5_combat_fx(w: &WeaponView<'_, '_>) -> (WeaponCombatFx, CombatFxSlots) {
    let slot = |off| t5_to_iw4_ptr(w.def_slot(off));
    let hint = |off| bare_owned(w.def_asset_name(off));
    let slots = CombatFxSlots {
        view_flash: slot(sz::WEAPON_DEF_VIEW_FLASH_OFF),
        world_flash: slot(sz::WEAPON_DEF_WORLD_FLASH_OFF),
        view_shell_eject: slot(sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF),
        world_shell_eject: slot(sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF),
        view_last_shot_eject: slot(sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF),
        world_last_shot_eject: slot(sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF),
        explosion: slot(sz::WEAPON_DEF_PROJ_EXPLOSION_EFFECT_OFF),
        tracer: None,
    };
    let fx = WeaponCombatFx {
        view_flash_hint: hint(sz::WEAPON_DEF_VIEW_FLASH_OFF),
        world_flash_hint: hint(sz::WEAPON_DEF_WORLD_FLASH_OFF),
        view_shell_eject_hint: hint(sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF),
        world_shell_eject_hint: hint(sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF),
        view_last_shot_eject_hint: hint(sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF),
        world_last_shot_eject_hint: hint(sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF),
        explosion_hint: hint(sz::WEAPON_DEF_PROJ_EXPLOSION_EFFECT_OFF),
        last_shot_eject_pair_authored: slots.last_shot_pair_authored(),
        ..WeaponCombatFx::empty(crate::AssetNamespace::T5)
    };
    (fx, slots)
}

pub(super) fn leftover_t5_sounds(w: &WeaponView<'_, '_>) -> WeaponSoundAliases {
    if w.geometry().weap_def.is_none() {
        return WeaponSoundAliases::default();
    }
    let s = |off| w.def_str(off).map(str::to_owned);
    WeaponSoundAliases {
        projectile: s(sz::WEAPON_DEF_PROJECTILE_SOUND_OFF),
        proj_explosion: s(sz::WEAPON_DEF_PROJ_EXPLOSION_SOUND_OFF),
        proj_ignition_sound: s(sz::WEAPON_DEF_PROJ_IGNITION_SOUND_OFF),
        notetrack_convention: NotetrackConvention::InlinePrefix,
        fire: s(sz::WEAPON_DEF_SND_FIRE_OFF),
        fire_player: s(sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF),
        fire_loop: s(sz::WEAPON_DEF_SND_LOOP_FIRE_OFF),
        fire_loop_player: s(sz::WEAPON_DEF_SND_LOOP_FIRE_PLAYER_OFF),
        fire_stop: s(sz::WEAPON_DEF_SND_STOP_FIRE_OFF),
        fire_stop_player: s(sz::WEAPON_DEF_SND_STOP_FIRE_PLAYER_OFF),
        fire_last: s(sz::WEAPON_DEF_SND_LAST_SHOT_OFF),
        fire_last_player: s(sz::WEAPON_DEF_SND_LAST_SHOT_PLAYER_OFF),
        pullback: s(sz::WEAPON_DEF_SND_PULLBACK_OFF),
        pullback_player: s(sz::WEAPON_DEF_SND_PULLBACK_PLAYER_OFF),
        alt_switch: s(sz::WEAPON_DEF_SND_ALT_SWITCH_OFF),
        alt_switch_player: s(sz::WEAPON_DEF_SND_ALT_SWITCH_PLAYER_OFF),
        raise: s(sz::WEAPON_DEF_SND_RAISE_OFF),
        first_raise: s(sz::WEAPON_DEF_SND_FIRST_RAISE_OFF),
        first_raise_player: s(sz::WEAPON_DEF_SND_FIRST_RAISE_PLAYER_OFF),
        putaway: s(sz::WEAPON_DEF_SND_PUTAWAY_OFF),
        empty_fire: s(sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF),
        empty_fire_player: s(sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF),
        rechamber: s(sz::WEAPON_DEF_SND_RECHAMBER_OFF),
        rechamber_player: s(sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF),
        reload: s(sz::WEAPON_DEF_SND_RELOAD_OFF),
        reload_player: s(sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF),
        reload_empty: s(sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF),
        reload_empty_player: s(sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF),
        reload_start: s(sz::WEAPON_DEF_SND_RELOAD_START_OFF),
        reload_start_player: s(sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF),
        reload_end: s(sz::WEAPON_DEF_SND_RELOAD_END_OFF),
        reload_end_player: s(sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF),
        raise_player: s(sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF),
        putaway_player: s(sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF),
        melee_swipe: s(sz::WEAPON_DEF_SND_MELEE_SWIPE_OFF),
        melee_swipe_player: s(sz::WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF),
        melee_hit: s(sz::WEAPON_DEF_SND_MELEE_HIT_OFF),
        melee_miss: s(sz::WEAPON_DEF_SND_MELEE_MISS_OFF),
        pickup: s(0x07c),
        pickup_player: s(0x080),
        ammo_pickup: s(0x084),
        ammo_pickup_player: s(0x088),
        detonate: s(0x130),
        detonate_player: s(0x134),
        bounce: w
            .def_str_array::<31>(0x174)
            .map(|arr| arr.map(|s| s.map(str::to_owned)))
            .unwrap_or([const { None }; 31]),
        notetrack_sound_map: w
            .def_note_map(
                sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF,
                sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF,
            )
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
        ..WeaponSoundAliases::default()
    }
}

/// The body facts, and the definition and variant as the zone holds them for
/// Black Ops' own weapon rules.
pub(super) fn capture_t5_body_facts(
    w: &WeaponView<'_, '_>,
) -> (WeaponBodyFacts, Option<Arc<GameWeaponBytes>>) {
    let geometry = w.geometry();
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: geometry.weap_type,
        weap_class: remap_t5_weap_class(geometry.weap_class),
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        rechamber_time_ms: geometry.rechamber_time_ms,
        drop_time_ms: geometry.drop_time_ms,
        alternate_raise_time_ms: geometry.alternate_raise_time_ms,
        alternate_drop_time_ms: geometry.alternate_drop_time_ms,
        raise_time_ms: geometry.raise_time_ms,
        bolt_action: geometry.bolt_action,
        select_requires_ammo: Some(leftover_t5_select_requires_ammo()),
        ..WeaponBodyFacts::default()
    };
    let def_i32 = |off| w.def_i32(off);
    let def_f32 = |off| w.def_f32(off);
    let def_flag = |off| w.def_u8(off) != 0;
    if geometry.weap_def.is_some() {
        facts.kill_icon_ratio = def_i32(sz::WEAPON_DEF_KILL_ICON_RATIO_OFF);
        facts.flip_kill_icon = def_flag(sz::WEAPON_DEF_FLIP_KILL_ICON_OFF);
    }
    if geometry.variant.is_some() {
        facts.silenced = w.variant_u8(sz::WEAPON_VARIANT_SILENCED_OFF) != 0;
        if w.variant_u8(sz::WEAPON_VARIANT_RAPID_FIRE_OFF) != 0 {
            facts.fire_time_ms = (facts.fire_time_ms as f32 * 0.75) as i32;
        }
        facts.reload_time_ms = w.variant_i32(sz::WEAPON_VARIANT_RELOAD_TIME_OFF);
        facts.reload_empty_time_ms = w.variant_i32(sz::WEAPON_VARIANT_RELOAD_EMPTY_TIME_OFF);
        let ads_in_ms = w.variant_i32(sz::WEAPON_VARIANT_ADS_TRANS_IN_OFF);
        let ads_out_ms = w.variant_i32(sz::WEAPON_VARIANT_ADS_TRANS_OUT_OFF);
        let stored_in = w.variant_f32(sz::WEAPON_VARIANT_ADS_IN_RATE_OFF);
        let stored_out = w.variant_f32(sz::WEAPON_VARIANT_ADS_OUT_RATE_OFF);
        facts.ads_in_rate = leftover_t5_ads_rate(ads_in_ms, stored_in);
        facts.ads_out_rate = leftover_t5_ads_rate(ads_out_ms, stored_out);
        facts.ads_zoom_fov = leftover_t5_zoom_fov(w);
        if w.cstr(geometry.name)
            .is_some_and(|name| name.split('_').any(|part| part == "vzoom"))
        {
            facts.scope_zoom = weapon_iw4::ScopeZoom::from_fovs([
                facts.ads_zoom_fov,
                w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF),
                w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF),
            ]);
        }
        facts.ads_zoom_fovs = [
            sz::WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF,
            sz::WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF,
            sz::WEAPON_VARIANT_ADS_ZOOM_FOV3_OFF,
        ]
        .map(|off| w.variant_f32(off));
        facts.ads_zoom_in_frac = w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_IN_FRAC_OFF);
        facts.ads_zoom_out_frac = w.variant_f32(sz::WEAPON_VARIANT_ADS_ZOOM_OUT_FRAC_OFF);
    }
    if geometry.weap_def.is_none() {
        return (facts, None);
    }
    facts.inventory_type = def_i32(sz::WEAPON_INVENTORY_TYPE_OFF);
    facts.dual_wield = def_flag(sz::WEAPON_DEF_DUAL_WIELD_OFF);
    facts.fuel_tank = def_flag(sz::WEAPON_DEF_FUEL_TANK_OFF);
    facts.impact_type = def_i32(sz::WEAPON_DEF_IMPACT_TYPE_OFF);
    facts.ammo_counter_clip = def_i32(sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF);
    facts.start_ammo = def_i32(sz::WEAPON_DEF_START_AMMO_OFF);
    facts.max_ammo = def_i32(sz::WEAPON_DEF_MAX_AMMO_OFF);
    facts.ammo_count_clip_relative = def_flag(sz::WEAPON_DEF_AMMO_COUNT_CLIP_RELATIVE_OFF);
    facts.shots_per_fire = def_i32(sz::WEAPON_DEF_SHOT_COUNT_OFF);
    apply_leftover_hip_spread(
        &mut facts,
        leftover_hip_spread_block(def_f32, sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF),
    );
    facts.damage = def_i32(sz::WEAPON_DEF_DAMAGE_OFF);
    facts.min_damage = def_i32(sz::WEAPON_DEF_MIN_DAMAGE_OFF);
    facts.max_damage_range = def_f32(sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF);
    facts.min_damage_range = def_f32(sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF);
    facts.explosion_radius = def_i32(sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF);
    facts.explosion_radius_min = def_i32(sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF);
    facts.explosion_inner_damage = def_i32(sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF);
    facts.explosion_outer_damage = def_i32(sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF);
    facts.projectile_speed = def_i32(sz::WEAPON_DEF_PROJECTILE_SPEED_OFF);
    facts.projectile_speed_up = def_i32(sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF);
    facts.projectile_activate_dist = def_i32(sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF);
    facts.projectile_explosion_type = def_i32(sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF);
    facts.missile_guidance = def_i32(sz::WEAPON_DEF_MISSILE_GUIDANCE_OFF);
    facts.ignition_delay_ms = def_i32(sz::WEAPON_DEF_IGNITION_DELAY_OFF);
    facts.proj_impact_explode = def_flag(sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF);
    facts.offhand_class = leftover_t5_offhand_class(def_i32(sz::WEAPON_DEF_OFFHAND_CLASS_OFF));
    facts.hold_fire_time_ms = def_i32(sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF);
    facts.fuse_time_ms = def_i32(sz::WEAPON_DEF_FUSE_TIME_OFF);
    facts.require_lock_to_fire = def_flag(sz::WEAPON_DEF_REQUIRE_LOCK_TO_FIRE_OFF);
    facts.stickiness = def_i32(sz::WEAPON_DEF_STICKINESS_OFF);
    facts.stick_to_players = matches!(facts.stickiness, 1 | 5);
    facts.has_detonator = def_flag(sz::WEAPON_DEF_HAS_DETONATOR_OFF);
    facts.timed_detonation = def_flag(sz::WEAPON_DEF_TIMED_DETONATION_OFF);
    facts.projectile_rotates = def_flag(sz::WEAPON_DEF_ROTATE_OFF);
    facts.cook_off_hold = def_flag(sz::WEAPON_DEF_COOK_OFF_HOLD_OFF);
    facts.keep_crosshair_when_ads = def_flag(sz::WEAPON_DEF_KEEP_CROSSHAIR_WHEN_ADS_OFF);
    facts.offhand_hold_is_cancelable =
        Some(def_flag(sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF));
    facts.parallel_bounce = w.def_f32_array(sz::WEAPON_DEF_PARALLEL_BOUNCE_OFF);
    facts.perpendicular_bounce = w.def_f32_array(sz::WEAPON_DEF_PERPENDICULAR_BOUNCE_OFF);
    facts.fire_delay_ms = def_i32(sz::WEAPON_DEF_FIRE_DELAY_OFF);
    facts.quick_drop_time_ms = def_i32(sz::WEAPON_QUICK_DROP_TIME_OFF);
    facts.quick_raise_time_ms = def_i32(sz::WEAPON_QUICK_RAISE_TIME_OFF);
    facts.melee_damage = def_i32(sz::WEAPON_DEF_MELEE_DAMAGE_OFF);
    facts.melee_time_ms = def_i32(sz::WEAPON_DEF_MELEE_TIME_OFF);
    facts.melee_delay_ms = def_i32(sz::WEAPON_DEF_MELEE_DELAY_OFF);
    facts.melee_charge_time_ms = def_i32(sz::WEAPON_DEF_MELEE_CHARGE_TIME_OFF);
    facts.melee_charge_delay_ms = def_i32(sz::WEAPON_DEF_MELEE_CHARGE_DELAY_OFF);
    facts.use_as_melee = def_flag(sz::WEAPON_DEF_USE_AS_MELEE_OFF);
    facts.rechamber_bolt_delay_ms = def_i32(sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF);

    facts.reload_show_rocket_time_ms = def_i32(0x3d4);
    facts.reload_add_time_ms = def_i32(sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF);
    facts.reload_empty_add_time_ms = def_i32(sz::WEAPON_DEF_RELOAD_EMPTY_ADD_TIME_OFF);
    facts.reload_start_time_ms = def_i32(sz::WEAPON_DEF_RELOAD_START_TIME_OFF);
    facts.reload_start_add_time_ms = def_i32(sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF);
    facts.reload_end_time_ms = def_i32(sz::WEAPON_DEF_RELOAD_END_TIME_OFF);
    if w.variant_u8(sz::WEAPON_VARIANT_DUAL_MAG_OFF) != 0 {
        facts.dual_mag = Some(weapon_iw4::DualMagTimes {
            reload_ms: w.variant_i32(sz::WEAPON_VARIANT_RELOAD_QUICK_TIME_OFF),
            reload_empty_ms: w.variant_i32(sz::WEAPON_VARIANT_RELOAD_QUICK_EMPTY_TIME_OFF),
            add_ms: def_i32(sz::WEAPON_DEF_RELOAD_QUICK_ADD_TIME_OFF),
            empty_add_ms: def_i32(sz::WEAPON_DEF_RELOAD_QUICK_EMPTY_ADD_TIME_OFF),
        });
    }
    facts.reload_ammo_add = def_i32(sz::WEAPON_DEF_RELOAD_AMMO_ADD_OFF);
    facts.reload_start_add = def_i32(sz::WEAPON_DEF_RELOAD_START_ADD_OFF);
    facts.overlay_reticle = def_i32(sz::WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF);
    facts.can_hold_breath = facts.overlay_reticle != 0 && facts.weap_class != 11;
    facts.overlay_interface = def_i32(sz::WEAPON_DEF_ADS_OVERLAY_INTERFACE_OFF);
    facts.ads_overlay_width = def_f32(sz::WEAPON_DEF_ADS_OVERLAY_WIDTH_OFF);
    facts.ads_overlay_height = def_f32(sz::WEAPON_DEF_ADS_OVERLAY_HEIGHT_OFF);
    facts.i_reticle_side_size = def_i32(sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF);
    facts.i_reticle_min_ofs = def_i32(sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF);
    facts.hip_reticle_side_pos = def_f32(sz::WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF);
    facts.no_ads_when_mag_empty = def_flag(sz::WEAPON_DEF_NO_ADS_WHEN_MAG_EMPTY_OFF);
    facts.aim_down_sight = def_flag(sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF);
    facts.rechamber_while_ads = def_flag(sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF);
    facts.ads_fire_only = def_flag(sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF);
    facts.no_partial_reload = def_flag(sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF);
    facts.segmented_reload = def_flag(sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF);
    facts.idle = WeaponIdleInputs {
        ads_idle_amount: def_f32(sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF),
        hip_idle_amount: def_f32(sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF),
        ads_idle_speed: def_f32(sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF),
        hip_idle_speed: def_f32(sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF),
        idle_crouch_factor: def_f32(sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF),
        idle_prone_factor: def_f32(sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF),
    };
    facts.inherits_perks = leftover_t5_inherits_host_perks();
    facts.kick = leftover_t5_kick(w);
    // The ammo and clip names weapons share rounds by; an unnamed one is
    // "".
    let [ammo_name, clip_name] = [geometry.ammo_name, geometry.clip_name]
        .map(|ptr| w.cstr(ptr).map_or_else(String::new, str::to_owned));
    let game_bytes = w.def_bytes().map(|(def, var)| {
        Arc::new(GameWeaponBytes {
            def: def.into(),
            variant: var.into(),
            ammo_name: ammo_name.clone(),
            clip_name: clip_name.clone(),
            named_anims: w
                .xanim_names()
                .iter()
                .enumerate()
                .filter(|(_, name)| name.is_some())
                .fold(0u128, |bits, (slot, _)| bits | 1 << slot),
            anim_slots: RUNTIME_ANIM_SLOTS,
            left_anim_slots: LEFT_ANIM_SLOTS,
            location_damage: w
                .def_f32_array::<{ sz::WEAPON_LOCATION_DAMAGE_COUNT }>(
                    sz::WEAPON_DEF_LOCATION_DAMAGE_OFF,
                )
                .map(Box::from),
            parallel_bounce: facts.parallel_bounce.map(Box::from),
            perpendicular_bounce: facts.perpendicular_bounce.map(Box::from),
            world_clip_model: bare_owned(w.cstr(geometry.world_clip_model_name)),
            has_stowed_model: geometry.has_stowed_model,
            explosion_sounds: [0x620, 0x624, 0x628].map(|off| w.def_str(off).map(str::to_owned)),
            bounce_sounds: w
                .def_str_array::<31>(0x174)
                .map(|arr| Box::new(arr.map(|s| s.map(str::to_owned)))),
            whizby_sound: w.def_str(0x0cc).map(str::to_owned),
            crack_sound: w.def_str(0x0c8).map(str::to_owned),
        })
    });
    (facts, game_bytes)
}

pub(super) fn leftover_t5_inherits_host_perks() -> bool {
    true
}

pub(super) fn leftover_t5_select_requires_ammo() -> bool {
    false
}

pub(super) fn leftover_t5_kick(w: &WeaponView<'_, '_>) -> WeaponKickFacts {
    let mut k = WeaponKickFacts::default();
    if w.geometry().variant.is_some() {
        k.f_ads_view_kick_center_speed =
            w.variant_f32(sz::WEAPON_VARIANT_ADS_VIEW_KICK_CENTER_SPEED_OFF);
        k.f_hip_view_kick_center_speed =
            w.variant_f32(sz::WEAPON_VARIANT_HIP_VIEW_KICK_CENTER_SPEED_OFF);
    }
    if w.geometry().weap_def.is_none() {
        return k;
    }
    let f = |off| w.def_f32(off);
    k.gun_max_pitch = f(sz::WEAPON_DEF_GUN_MAX_PITCH_OFF);
    k.gun_max_yaw = f(sz::WEAPON_DEF_GUN_MAX_YAW_OFF);
    k.ads_gun_kick_reduced_kick_bullets =
        w.def_i32(sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_BULLETS_OFF);
    k.ads_gun_kick_reduced_kick_percent = f(sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_PERCENT_OFF);
    k.ads_gun_kick_pitch_min = f(sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF);
    k.ads_gun_kick_pitch_max = f(sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF);
    k.ads_gun_kick_yaw_min = f(sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF);
    k.ads_gun_kick_yaw_max = f(sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF);
    k.ads_gun_kick_accel = f(sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF);
    k.ads_gun_kick_speed_max = f(sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF);
    k.ads_gun_kick_speed_decay = f(sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF);
    k.ads_gun_kick_static_decay = f(sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF);
    k.ads_view_kick_pitch_min = f(sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF);
    k.ads_view_kick_pitch_max = f(sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF);
    k.ads_view_kick_yaw_min = f(sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF);
    k.ads_view_kick_yaw_max = f(sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_reduced_kick_bullets =
        w.def_i32(sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_BULLETS_OFF);
    k.hip_gun_kick_reduced_kick_percent = f(sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_PERCENT_OFF);
    k.hip_gun_kick_pitch_min = f(sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF);
    k.hip_gun_kick_pitch_max = f(sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF);
    k.hip_gun_kick_yaw_min = f(sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF);
    k.hip_gun_kick_yaw_max = f(sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_accel = f(sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF);
    k.hip_gun_kick_speed_max = f(sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF);
    k.hip_gun_kick_speed_decay = f(sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF);
    k.hip_gun_kick_static_decay = f(sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF);
    k.hip_view_kick_pitch_min = f(sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF);
    k.hip_view_kick_pitch_max = f(sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF);
    k.hip_view_kick_yaw_min = f(sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF);
    k.hip_view_kick_yaw_max = f(sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF);
    k
}
