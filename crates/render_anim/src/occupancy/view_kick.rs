use asset_game::{WeaponFpvFacts, WeaponKickFacts};
use assets::PreparedWeapons;
use bevy::prelude::*;
use frame::{LifeStarted, ViewSubject};
use hud_iw4::{
    CG_FOV_MIN_DEFAULT, CG_FOV_SCALE_DEFAULT, FovInputs, WeaponAdsOverlayFacts, calc_fov_from_ads,
    horizontal_to_vertical_fov_deg, shellshock_perturb_axis, shellshock_view_kick,
    zoom_sensitivity,
};
use math_iw4::{add_lean_to_position, angle_vectors};
use net::{
    AppliedEntityEventWalk, ClientActionInput, FrameClock, LocalPresentClient, PresentedSnapshot,
};
use playerstate_iw4::ENTITYNUM_NONE;
use weapon_iw4::{
    VIEW_DAMAGE_UNDIRECTED, VIEW_ORG_BOB_Z_MIN_OFS, ViewAngleBobInputs, ViewOrgBobInputs,
    crash_land_fall_height, crash_land_view_dip, damage_feedback_kick, get_viewmodel_weapon_index,
    land_origin_z, view_angle_bob, view_damage_angles, view_org_bob, viewweapon_land_origin_z,
};

use crate::anim::view_kick_state::{KickParams, ViewKickState, add_kick_to_viewangles};
use crate::anim::view_sway::ViewSwayState;
use crate::occupancy::remote_body::RemotePlayer;
use crate::occupancy::third_person::{
    RemoteMissileView, linked_weapon_camera, presented_is_third_person, third_person_camera,
};
use render_scene::{FlyCamera, FpvLens, SimCamera, transform_from_iw_view};
use render_scene::{WorldCameraPose, WorldScriptModelInstance};

const MISSILE_CAM_FOV: f32 = 15.0;

fn kick_params(k: &WeaponKickFacts) -> KickParams {
    *k
}

#[derive(Resource, Default)]
pub struct PendingViewHurt(pub u32);

#[derive(Resource, Default)]
pub struct SessionViewKick {
    pub state: ViewKickState,
    pub sway: ViewSwayState,

    pub placement_move_origin: [f32; 3],

    pub placement_move_angles: [f32; 3],

    pub weap_idle_time: i32,

    pub last_idle_factor: f32,

    pub view_last_idle_factor: f32,

    pub land_change: f32,

    pub land_time: i32,

    pub land_view_dip: i32,

    pub viewweapon_land_z: f32,

    pub viewweapon_land_view: [f32; 3],

    pub refdef_view_angles: [f32; 3],

    pub refdef_vieworg: [f32; 3],

    pub horiz_fov_deg: f32,

    pub killcam_focus_distance: Option<f32>,

    pub damage_time: i32,

    pub v_dmg_pitch: f32,

    pub v_dmg_roll: f32,
    /// The game's own view kick for a hit, when its code HUD has one, and
    /// the client random state that kick draws from.
    damage_rules: Option<game_api::hud::DamageFlash>,
    damage_random: u32,
    last_weapon_id: u32,
    last_origin: [f32; 3],
    last_velocity: [f32; 3],
    last_ground_entity: i32,
    have_land_prev: bool,
    last_damage_event: u32,
    have_damage_prev: bool,

    pub seeded_this_frame: u32,

    pub(super) last_weapon_pos_frac: f32,

    pub b_position_to_ads: bool,
}

impl SessionViewKick {
    pub fn clear_for_new_life(&mut self) {
        self.state.reset();
        self.sway.reset();
        self.placement_move_origin = [0.0; 3];
        self.placement_move_angles = [0.0; 3];
        self.weap_idle_time = 0;
        self.last_idle_factor = 0.0;
        self.view_last_idle_factor = 0.0;
        self.land_change = 0.0;
        self.land_time = 0;
        self.land_view_dip = 0;
        self.viewweapon_land_z = 0.0;
        self.viewweapon_land_view = [0.0; 3];
        self.damage_time = 0;
        self.v_dmg_pitch = 0.0;
        self.v_dmg_roll = 0.0;
        self.last_origin = [0.0; 3];
        self.last_velocity = [0.0; 3];
        self.last_ground_entity = 0;
        self.have_land_prev = false;
        self.last_damage_event = 0;
        self.have_damage_prev = false;
        self.seeded_this_frame = 0;
        self.last_weapon_pos_frac = 0.0;
        self.b_position_to_ads = true;
    }
}

pub fn reset_view_kick_on_life_started(
    local: Res<LocalPresentClient>,
    mut started: MessageReader<LifeStarted>,
    mut kick: ResMut<SessionViewKick>,
    mut hurt: ResMut<PendingViewHurt>,
) {
    for ev in started.read() {
        if ev.client == local.0.0 {
            kick.clear_for_new_life();
            hurt.0 = 0;
        }
    }
}

pub fn tick_session_view_kick(
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    walk: Option<Res<AppliedEntityEventWalk>>,
    mode: Option<Res<game_api::ModeRules>>,
    mut kick: ResMut<SessionViewKick>,
) {
    kick.damage_rules = match mode.as_deref().map(|mode| mode.hud.code_hud) {
        Some(game_api::Rule::Known(game_api::hud::CodeHud::Game(hud))) => hud.damage_flash,
        _ => None,
    };
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let viewmodel = get_viewmodel_weapon_index(ps);
    if viewmodel != kick.last_weapon_id {
        kick.state.reset();
        kick.sway.reset();
        kick.placement_move_origin = [0.0; 3];
        kick.placement_move_angles = [0.0; 3];
        kick.weap_idle_time = 0;
        kick.last_idle_factor = 0.0;
        kick.view_last_idle_factor = 0.0;
        kick.seeded_this_frame = 0;
        kick.last_weapon_id = viewmodel;
        kick.last_weapon_pos_frac = 0.0;
        kick.b_position_to_ads = true;
    }

    let Some(reg) = weapons.as_ref() else {
        return;
    };
    let Some(facts) = reg
        .snapshot_weapon(presented.weapon_epoch(), viewmodel)
        .ok()
        .and_then(|weapon| weapon.fpv_facts())
    else {
        return;
    };
    if !facts.body_resolved {
        return;
    }

    let dt = clock.frametime_secs();
    kick.seeded_this_frame = 0;
    let game_kick = mode
        .as_deref()
        .and_then(|mode| mode.client_fire)
        .and_then(|rules| rules.view_kick)
        .is_some();
    if game_kick {
        kick.state.reset();
    } else {
        let n = walk.map(|w| w.local_fire).unwrap_or(0);
        if n > 0 {
            let reduce_window_active = ps.weapon_restrict_kick_time > 0;
            let params = kick_params(&facts.kick);
            for _ in 0..n {
                kick.state.seed_fire(
                    &params,
                    ps.f_weapon_pos_frac,
                    ps.weap_flags,
                    ps.recoil_scale,
                    reduce_window_active,
                );
            }
            kick.seeded_this_frame = n;
        }

        let params = kick_params(&facts.kick);
        kick.state.advance(&params, ps.f_weapon_pos_frac, dt);
    }

    let overlay_active = facts.overlay_reticle != 0;
    kick.sway.advance(
        facts.sway.hip_params(),
        facts.sway.ads_params(),
        ps.viewangles,
        ps.f_weapon_pos_frac,
        facts.aim_down_sight,
        overlay_active,
        1.0,
        dt,
    );
}

pub fn sync_camera_from_presented(
    mut killcam: Local<super::killcam::KillcamCamera>,
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    sim_cam: Res<SimCamera>,
    settings: Res<frame::GameSettings>,
    mut kick: ResMut<SessionViewKick>,
    mut hurt: ResMut<PendingViewHurt>,
    weapons: Option<Res<PreparedWeapons>>,
    mut actions: Option<ResMut<ClientActionInput>>,
    mut q: Query<
        &mut Transform,
        (
            With<FlyCamera>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
    mut lenses: Query<&mut Projection, With<FpvLens>>,
    view: Res<ViewSubject>,
    death_cam_clip: Res<crate::occupancy::dyn_ent::DynEntPhysClip>,
    mut missile_view: Local<RemoteMissileView>,
    mut shock_kicking: Local<bool>,
) {
    kick.killcam_focus_distance = None;
    let base_fov = script_fov(&presented, local.0).unwrap_or(settings.fov);
    let shock_kick = shock_view_kick(&presented, local.0, clock.time());
    if (shock_kick.0 != [0.0; 2]) != *shock_kicking {
        *shock_kicking = !*shock_kicking;
        diag::info!(
            World,
            "shellshock view kick: {} {:?} ordered_sums={}",
            if *shock_kicking { "on" } else { "off" },
            shock_kick.0,
            shock_kick.1
        );
    }
    if let Some(view) = sim_cam.view_override {
        for mut transform in &mut q {
            *transform = shock_eye(view.transform, shock_kick);
        }
        for mut projection in &mut lenses {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = horizontal_to_vertical_fov_deg(view.horizontal_fov).to_radians();
            }
        }
        kick.horiz_fov_deg = view.horizontal_fov;
        kick.refdef_vieworg = view.transform.translation.to_array();
        let forward = *view.transform.forward();
        let right = *view.transform.right();
        let up = *view.transform.up();
        kick.refdef_view_angles = [
            -forward.z.clamp(-1.0, 1.0).asin().to_degrees(),
            forward.y.atan2(forward.x).to_degrees(),
            (-right.z).atan2(up.z).to_degrees(),
        ];
        return;
    }
    if !sim_cam.enabled {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let viewmodel = get_viewmodel_weapon_index(ps);
    if let Some((pose, fov, focus_distance)) = killcam.update(
        &presented,
        local.0,
        clock.time(),
        view.in_killcam(),
        weapons.as_deref(),
        death_cam_clip.0.as_deref(),
    ) {
        let pose = earthquake_pose(pose, &presented, clock.time());
        let eye = shock_eye(transform_from_iw_view(pose), shock_kick);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        for mut projection in &mut lenses {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = horizontal_to_vertical_fov_deg(fov).to_radians();
            }
        }
        kick.horiz_fov_deg = fov;
        kick.refdef_vieworg = pose.origin;
        kick.refdef_view_angles = pose.angles;
        kick.killcam_focus_distance = focus_distance;
        return;
    }
    if let Some(pose) = missile_view.camera(
        &presented,
        local.0,
        clock.time(),
        death_cam_clip.0.as_deref(),
    ) {
        let pose = earthquake_pose(pose, &presented, clock.time());
        let eye = shock_eye(transform_from_iw_view(pose), shock_kick);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        let horiz = MISSILE_CAM_FOV;
        if let Some(actions) = actions.as_deref_mut() {
            actions.fov_scale = zoom_sensitivity(horiz) * actions.shellshock_look_scale;
        }
        let vertical = horizontal_to_vertical_fov_deg(horiz).to_radians();
        for mut projection in lenses.iter_mut() {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = vertical;
            }
        }
        kick.horiz_fov_deg = horiz;
        return;
    }
    if presented_is_third_person(
        &presented,
        local.0,
        view.in_killcam(),
        settings.third_person,
    ) {
        let Some(pose) = linked_weapon_camera(&presented, local.0)
            .or_else(|| third_person_camera(&presented, local.0, death_cam_clip.0.as_deref()))
        else {
            return;
        };
        let pose = earthquake_pose(pose, &presented, clock.time());
        let eye = shock_eye(transform_from_iw_view(pose), shock_kick);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        kick.refdef_vieworg = pose.origin;
        kick.refdef_view_angles = pose.angles;
        kick.horiz_fov_deg = apply_fpv_lens_fov(
            &mut lenses,
            base_fov,
            ps.pm_type,
            ps.link_flags,
            ps.e_flags,
            0.0,
            ps.scope_zoom_level,
            viewmodel,
            weapons.as_ref().and_then(|w| {
                w.snapshot_weapon(presented.weapon_epoch(), viewmodel)
                    .ok()
                    .and_then(|weapon| weapon.fpv_facts())
            }),
            false,
            actions.as_deref_mut(),
        )
        .unwrap_or(base_fov);
        return;
    }
    let offset = presented.view_offset();
    let xyspeed = {
        let vx = ps.velocity[0];
        let vy = ps.velocity[1];
        math_iw4::vec3_length([vx, vy, 0.0])
    };
    let org = ViewOrgBobInputs {
        bob_cycle: (ps.bob_cycle as u32 & 0xff) as u8,
        xyspeed,
        view_height_target: ps.view_height_target,
        pm_flags: ps.pm_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
        perks0: ps.perks[0],
    };
    stamp_damage_feedback(
        &mut kick,
        &mut hurt,
        ps.damage_event,
        ps.damage_yaw,
        ps.damage_pitch,
        ps.damage_count,
        ps.viewangles,
        clock.time(),
    );
    let bob_angles = match weapons.as_ref().and_then(|w| {
        w.snapshot_weapon(presented.weapon_epoch(), viewmodel)
            .ok()
            .and_then(|weapon| weapon.fpv_facts())
    }) {
        Some(facts) if facts.body_resolved => view_angle_bob(ViewAngleBobInputs {
            org,
            e_flags: ps.e_flags,
            overlay_reticle: facts.overlay_reticle,
            ads_bob_factor: facts.ads_bob_factor,
            ads_view_bob_mult: facts.ads_view_bob_mult,
            time: clock.time(),
            damage_time: kick.damage_time,
            v_dmg_pitch: kick.v_dmg_pitch,
            v_dmg_roll: kick.v_dmg_roll,
            aim_down_sight: facts.aim_down_sight,
            idle: facts.idle,
            frametime: clock.frametime_secs(),
            hold_breath_scale: ps.hold_breath_scale,
            weap_idle_time: kick.weap_idle_time,
            view_last_idle_factor: kick.view_last_idle_factor,
        }),
        _ => view_damage_angles(ViewAngleBobInputs {
            org,
            time: clock.time(),
            damage_time: kick.damage_time,
            v_dmg_pitch: kick.v_dmg_pitch,
            v_dmg_roll: kick.v_dmg_roll,
            ..ViewAngleBobInputs::default()
        }),
    };
    kick.weap_idle_time = bob_angles.weap_idle_time;
    kick.view_last_idle_factor = bob_angles.view_last_idle_factor;
    let kick_angles = add_kick_to_viewangles(ps.viewangles, kick.state.kick_angles);
    let angles = [
        kick_angles[0] + bob_angles.pitch,
        kick_angles[1] + bob_angles.yaw,
        kick_angles[2] + bob_angles.roll,
    ];
    kick.refdef_view_angles = angles;
    let mut origin = [
        ps.origin[0] + offset[0],
        ps.origin[1] + offset[1],
        ps.origin[2] + offset[2] + ps.view_height_current,
    ];
    let bob = view_org_bob(org);
    origin[2] += bob.vertical;

    let (fwd, right, up) = angle_vectors(angles);
    origin[0] += bob.horizontal * right[0];
    origin[1] += bob.horizontal * right[1];
    origin[2] += bob.horizontal * right[2];
    let land_ofs = stamp_and_land_origin_z(
        &mut kick,
        ps.gravity,
        ps.origin,
        ps.velocity,
        ps.ground_entity_num,
        clock.time(),
    );
    origin[2] += land_ofs;
    let delta_ms = clock.time().wrapping_sub(kick.land_time);
    kick.viewweapon_land_z = viewweapon_land_origin_z(delta_ms, kick.land_change);
    kick.viewweapon_land_view = [
        kick.viewweapon_land_z * fwd[2],
        kick.viewweapon_land_z * right[2],
        kick.viewweapon_land_z * up[2],
    ];
    origin = add_lean_to_position(origin, ps.viewangles[1], ps.leanf, 16.0, 20.0);
    let min_z = ps.origin[2] + offset[2] + VIEW_ORG_BOB_Z_MIN_OFS;
    if origin[2] < min_z {
        origin[2] = min_z;
    }
    let pose = earthquake_pose(WorldCameraPose { origin, angles }, &presented, clock.time());
    kick.refdef_vieworg = pose.origin;
    kick.refdef_view_angles = pose.angles;
    let eye = shock_eye(transform_from_iw_view(pose), shock_kick);
    for mut transform in &mut q {
        transform.translation = eye.translation;
        transform.rotation = eye.rotation;
    }

    if ps.f_weapon_pos_frac > kick.last_weapon_pos_frac {
        kick.b_position_to_ads = true;
    } else if ps.f_weapon_pos_frac < kick.last_weapon_pos_frac {
        kick.b_position_to_ads = false;
    }
    kick.last_weapon_pos_frac = ps.f_weapon_pos_frac;
    if let Some(horiz) = apply_fpv_lens_fov(
        &mut lenses,
        base_fov,
        ps.pm_type,
        ps.link_flags,
        ps.e_flags,
        ps.f_weapon_pos_frac,
        ps.scope_zoom_level,
        viewmodel,
        weapons.as_ref().and_then(|w| {
            w.snapshot_weapon(presented.weapon_epoch(), viewmodel)
                .ok()
                .and_then(|weapon| weapon.fpv_facts())
        }),
        kick.b_position_to_ads,
        actions.as_deref_mut(),
    ) {
        kick.horiz_fov_deg = horiz;
    }
}

/// The `cg_fov` a script set on the local client (`setClientDvar`), within
/// the dvar's domain, which stands for the player's own field of view.
fn script_fov(presented: &PresentedSnapshot, local: sim::ClientId) -> Option<f32> {
    let meta = presented.snapshot()?.meta.for_client(local)?;
    let (_, value) = meta
        .client_dvars
        .iter()
        .rev()
        .find(|(name, _)| name.eq_ignore_ascii_case("cg_fov"))?;
    let fov = value.trim().parse::<f32>().ok()?;
    fov.is_finite()
        .then(|| fov.clamp(CG_FOV_DOMAIN.0, CG_FOV_DOMAIN.1))
}

/// `cg_fov`'s domain.
const CG_FOV_DOMAIN: (f32, f32) = (1.0, 160.0);

fn apply_fpv_lens_fov(
    lenses: &mut Query<&mut Projection, With<FpvLens>>,
    base_fov: f32,
    pm_type: i32,
    link_flags: u32,
    e_flags: u32,
    f_weapon_pos_frac: f32,
    scope_zoom_level: u32,
    viewmodel: u32,
    facts: Option<WeaponFpvFacts>,
    b_position_to_ads: bool,
    actions: Option<&mut ClientActionInput>,
) -> Option<f32> {
    let facts = facts
        .filter(|f| f.body_resolved)
        .unwrap_or_else(WeaponFpvFacts::default);
    let overlay = WeaponAdsOverlayFacts {
        ads_zoom_in_frac: facts.ads_zoom_in_frac,
        ads_zoom_out_frac: facts.ads_zoom_out_frac,
        overlay_reticle: facts.overlay_reticle,
        ..WeaponAdsOverlayFacts::default()
    };
    let ads_target = facts
        .scope_zoom
        .fov(scope_zoom_level)
        .unwrap_or(facts.ads_zoom_fov);
    let ads_target = if ads_target.is_finite() && ads_target > 0.0 {
        ads_target
    } else {
        base_fov
    };
    let inputs = FovInputs {
        cg_fov: base_fov,
        pm_type,
        link_flags,
        e_flags,
        weapon_index_nonzero: viewmodel != 0,
        aim_down_sight: facts.aim_down_sight && facts.ads_zoom_fov > 0.0,
        ads_zoom_fov: ads_target,
        overlay_zoom: 0.0,
        fov_scale: CG_FOV_SCALE_DEFAULT,
        fov_min: CG_FOV_MIN_DEFAULT.min(ads_target),
    };
    let (horiz, _) = calc_fov_from_ads(&inputs, f_weapon_pos_frac, b_position_to_ads, &overlay);
    let zoom_sensitivity = zoom_sensitivity(horiz);
    if let Some(actions) = actions {
        actions.fov_scale = zoom_sensitivity * actions.shellshock_look_scale;
    }
    let vertical = horizontal_to_vertical_fov_deg(horiz).to_radians();
    for mut projection in lenses.iter_mut() {
        if let Projection::Perspective(perspective) = &mut *projection {
            perspective.fov = vertical;
        }
    }
    Some(horiz)
}

fn stamp_and_land_origin_z(
    kick: &mut SessionViewKick,
    gravity: i32,
    origin: [f32; 3],
    velocity: [f32; 3],
    ground_entity: i32,
    cg_time: i32,
) -> f32 {
    if kick.have_land_prev
        && kick.last_ground_entity == ENTITYNUM_NONE
        && ground_entity != ENTITYNUM_NONE
        && let Some(fall) = crash_land_fall_height(
            gravity,
            kick.last_origin[2],
            origin[2],
            kick.last_velocity[2],
        )
    {
        let dip = crash_land_view_dip(fall);
        if dip > 0 {
            kick.land_change = -(dip as f32);
            kick.land_time = cg_time;
            kick.land_view_dip = dip;
        }
    }
    kick.last_origin = origin;
    kick.last_velocity = velocity;
    kick.last_ground_entity = ground_entity;
    kick.have_land_prev = true;
    let delta = cg_time.wrapping_sub(kick.land_time) as f32;
    land_origin_z(delta, kick.land_change)
}

fn stamp_damage_feedback(
    kick: &mut SessionViewKick,
    hurt: &mut PendingViewHurt,
    damage_event: u32,
    damage_yaw: u32,
    damage_pitch: u32,
    damage_count: i32,
    viewangles: [f32; 3],
    cg_time: i32,
) {
    let mut stamped = false;
    if kick.have_damage_prev && damage_event != kick.last_damage_event && damage_count != 0 {
        stamp_kick(kick, damage_yaw, damage_pitch, damage_count, viewangles);
        kick.damage_time = cg_time.max(1);
        stamped = true;
    }
    if !stamped && hurt.0 > 0 {
        hurt.0 -= 1;
        stamp_kick(
            kick,
            VIEW_DAMAGE_UNDIRECTED,
            VIEW_DAMAGE_UNDIRECTED,
            1,
            viewangles,
        );
        kick.damage_time = cg_time.max(1);
    }
    kick.last_damage_event = damage_event;
    kick.have_damage_prev = true;
}

/// A hit kicks the view by the game's own rule when it has one, else by
/// Modern Warfare 2's.
fn stamp_kick(kick: &mut SessionViewKick, yaw: u32, pitch: u32, damage: i32, viewangles: [f32; 3]) {
    if let Some(rules) = kick.damage_rules {
        game_damage_kick(kick, &rules, [yaw, pitch], damage, viewangles);
        return;
    }
    let punch = damage_feedback_kick(yaw, pitch, damage, viewangles);
    kick.v_dmg_pitch = punch.v_dmg_pitch;
    kick.v_dmg_roll = punch.v_dmg_roll;
}

/// A hit's view kick by the game's rule: its damage times `kick_scale`, held
/// to `kick_min..=kick_max`; straight up when it came from no direction;
/// else the pitch by how far ahead it came from and the roll by how far to
/// the left, each plus a random share up to `kick_random` of the other; a
/// hit from yaw and pitch 0 keeps the last kick.
fn game_damage_kick(
    kick: &mut SessionViewKick,
    rules: &game_api::hud::DamageFlash,
    [yaw, pitch]: [u32; 2],
    damage: i32,
    viewangles: [f32; 3],
) {
    let amount = damage as f32 * rules.kick_scale;
    let amount = if amount < rules.kick_min || rules.kick_max < amount {
        if amount < rules.kick_min {
            rules.kick_min
        } else {
            rules.kick_max
        }
    } else {
        amount
    };
    if yaw == VIEW_DAMAGE_UNDIRECTED && pitch == VIEW_DAMAGE_UNDIRECTED {
        kick.v_dmg_roll = 0.0;
        kick.v_dmg_pitch = -amount;
        return;
    }
    if yaw == 0 && pitch == 0 {
        return;
    }
    let turn = 360.0f32 / 255.0;
    let (dir, _, _) = angle_vectors([pitch as f32 * turn, yaw as f32 * turn, 0.0]);
    let (forward, right, _) = angle_vectors(viewangles);
    let dot = |a: [f32; 3], b: [f32; 3]| a[1] * b[1] + a[2] * b[2] + a[0] * b[0];
    let left = dot([-right[0], -right[1], -right[2]], dir);
    let ahead = dot(forward, dir);
    kick.v_dmg_roll = -(left * amount);
    kick.v_dmg_pitch = ahead * amount;
    // The random shares are worked in the x87 unit's precision.
    let share = flrand(
        &mut kick.damage_random,
        -rules.kick_random,
        rules.kick_random,
    );
    kick.v_dmg_roll =
        (share * f64::from(ahead) * f64::from(amount) + f64::from(kick.v_dmg_roll)) as f32;
    let share = flrand(
        &mut kick.damage_random,
        -rules.kick_random,
        rules.kick_random,
    );
    kick.v_dmg_pitch =
        (share * f64::from(left) * f64::from(amount) + f64::from(kick.v_dmg_pitch)) as f32;
}

/// The client's random float from `min` to `max`.
fn flrand(random: &mut u32, min: f32, max: f32) -> f64 {
    *random = random.wrapping_mul(214013).wrapping_add(2531011);
    f64::from(*random >> 17) * (f64::from(max) - f64::from(min)) * (1.0 / 32768.0) + f64::from(min)
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct GunOffset {
    pub x: f32,

    pub y: f32,

    pub z: f32,
}

impl GunOffset {
    pub fn xyz(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

pub(crate) fn apply_cg_gun_offset_view(origin: [f32; 3], gun: [f32; 3]) -> [f32; 3] {
    [origin[0] + gun[0], origin[1] + gun[1], origin[2] + gun[2]]
}

pub(crate) fn apply_viewweapon_land_view(origin: [f32; 3], land_view: [f32; 3]) -> [f32; 3] {
    [
        origin[0] + land_view[0],
        origin[1] + land_view[1],
        origin[2] + land_view[2],
    ]
}

pub(crate) fn iw_view_placement_to_bevy_camera_local(
    origin: [f32; 3],
    angles_deg: [f32; 3],
) -> Transform {
    let [fwd, right, up] = origin;
    let translation = Vec3::new(right, up, -fwd);
    Transform {
        translation,
        rotation: crate::anim::fpv_pose::placement_angles_to_bevy_camera_quat(angles_deg),
        ..Default::default()
    }
}

/// The local player's shellshock view kick this frame: none without a shock
/// file that kicks, before the shock starts or once it is over.
fn shock_view_kick(
    presented: &PresentedSnapshot,
    local: sim::ClientId,
    now_ms: i32,
) -> ([f32; 2], bool) {
    let (Some(ps), Some(kick)) = (
        presented.player(local),
        presented.shellshock(local).and_then(|shock| shock.kick),
    ) else {
        return ([0.0; 2], false);
    };
    let elapsed = now_ms.wrapping_sub(ps.shellshock_time);
    if ps.shellshock_time == 0 || elapsed < 0 {
        return ([0.0; 2], kick.ordered_sums);
    }
    (
        shellshock_view_kick(kick, elapsed, ps.shellshock_duration),
        kick.ordered_sums,
    )
}

/// The eye turned by a shellshock's view kick, after every other part of
/// the view (the game turns its finished view axis).
fn shock_eye(mut eye: Transform, (kick, ordered_sums): ([f32; 2], bool)) -> Transform {
    if kick == [0.0; 2] {
        return eye;
    }
    let axis = [
        eye.forward().to_array(),
        (-*eye.right()).to_array(),
        eye.up().to_array(),
    ];
    let [forward, left, up] =
        shellshock_perturb_axis(kick, axis, ordered_sums).map(Vec3::from_array);
    eye.rotation = Quat::from_mat3(&Mat3::from_cols(-left, up, -forward));
    eye
}

fn earthquake_pose(
    mut pose: WorldCameraPose,
    presented: &PresentedSnapshot,
    now_ms: i32,
) -> WorldCameraPose {
    if let Some(snapshot) = presented.snapshot() {
        let eye = pose.origin;
        for quake in &snapshot.meta.objectives.earthquakes {
            let offset = quake.angle_offset(eye, now_ms);
            for (angle, delta) in pose.angles.iter_mut().zip(offset) {
                *angle += delta;
            }
        }
    }
    pose
}
