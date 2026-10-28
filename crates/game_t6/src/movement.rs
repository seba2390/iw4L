//! Black Ops 2's player movement (`movement_t6`) behind `game_api`'s
//! `PlayerMovement`: the shared player fields come from the match, the rest
//! of Black Ops 2's player state lives in the player's game bytes.

use fastfile_t6::weapon::{WeaponDefView, def, variant};
use game_api::movement::{
    ENTITY_NONE, ENTITY_WORLD, GAME_MOVE_BYTES, HELD_WEAPONS, HeldWeapon, MeleeTarget, MoveCommand,
    MoveContext, MoveOutcome, MovePlayer, MoveRestriction, MoveSignal, MoveStatus, MoveTrace,
    MoveType, MoveWorld, PlayerMovement, ScriptFlag, ShellshockStart, Stance, VolumeColumn,
    WeaponActivity, WeaponNotify, WeaponNotifyMemory, buttons as intent,
};
use game_api::{Unknown, unknown};
use movement_t6::events::event;
use movement_t6::state::{
    ENTITYNUM_NONE, ENTITYNUM_WORLD, buttons, other_flags, pm_flags, pm_type, view_height,
    weapon_state as ws,
};
use movement_t6::{Buttons, Gap, MoveEvent, PlayerState, Pm, UserCmd};

pub struct T6Movement {
    zombies: bool,
}

pub static ZOMBIES: T6Movement = T6Movement { zombies: true };
pub static MULTIPLAYER: T6Movement = T6Movement { zombies: false };

const PLAYER_CLIP_MASK: u32 = 0x0281_c011;
const DEAD_CLIP_MASK: u32 = 0x0081_0011;
/// The spectator's height-lock volumes: the map's first 32 of these, one per
/// brush model.
const WATER_CONTENTS: u32 = 0x20;
const SURFACE_TYPE_MASK: u32 = 0x03f0_0000;
const SPECTATOR_CLIP_MASK: u32 = 0x0080_0811;
const SPECTATOR_SPEED: i32 = 400;
const HEIGHT_LOCK_CLASS: &str = "heli_height_lock";
const HEIGHT_LOCK_LIMIT: usize = 32;

pub(crate) fn gap_unknown(gap: Gap) -> &'static Unknown {
    match gap {
        Gap::Mantle => unknown!(
            "t6.movement.mantle_anims",
            "a Black Ops 2 mantle whose mantle animations (mp_mantle_up_*, the over animations) are not loaded: it does not start or move",
            "the mantle animations from the zones"
        ),
        Gap::Mount => unknown!(
            "t6.movement.mount",
            "a mountable Black Ops 2 weapon set down on a ledge or while prone",
            "Black Ops 2's weapon mount from its executable"
        ),
        Gap::MountedView => unknown!(
            "t6.movement.mounted",
            "Black Ops 2's movement on turrets and vehicles",
            "Black Ops 2's turret and vehicle player moves from its executable"
        ),
        Gap::Shellshock => unknown!(
            "t6.movement.shellshock",
            "a shellshocked Black Ops 2 player whose shock file the match does not know moves at full speed",
            "the shock's file in the match"
        ),
        Gap::AnimLock => unknown!(
            "t6.movement.anim_lock",
            "Black Ops 2's scripted animation lock on a player",
            "the player animation script durations that end the lock"
        ),
        Gap::HeightLock => unknown!(
            "t6.movement.height_lock",
            "whether a Black Ops 2 spectator stays inside the map's height-lock volumes",
            "the runtime's line test against the map's heli_height_lock brush models"
        ),
    }
}

fn events_unknown() -> &'static Unknown {
    unknown!(
        "t6.movement.events",
        "the temporary event entity Black Ops 2's server spawns at a mantle's start for other clients is not spawned",
        "the mantle start temporary event entity"
    )
}

fn water_unknown() -> &'static Unknown {
    unknown!(
        "t6.movement.water",
        "the water level under a Black Ops 2 player: the match cannot trace a line against the world's water, so players are taken as dry",
        "the runtime's world line trace"
    )
}

fn target_velocity_unknown() -> &'static Unknown {
    unknown!(
        "t6.movement.launch_target_velocity",
        "a melee lunge at an actor is not steered onto where the actor is heading: Black Ops 2 reads the target entity's trajectory delta, and what an actor's carries is not read",
        "what Black Ops 2's server puts in an actor's trajectory delta"
    )
}

fn t6_entity(entity: i32) -> i32 {
    match entity {
        ENTITY_NONE => ENTITYNUM_NONE,
        ENTITY_WORLD => ENTITYNUM_WORLD,
        n => n,
    }
}

fn shared_entity(entity: i32) -> i32 {
    match entity {
        ENTITYNUM_NONE => ENTITY_NONE,
        ENTITYNUM_WORLD => ENTITY_WORLD,
        n => n,
    }
}

fn pm_type_of(move_type: MoveType) -> i32 {
    match move_type {
        MoveType::Normal => pm_type::NORMAL,
        MoveType::NormalLinked => pm_type::NORMAL_LINKED,
        MoveType::Noclip => pm_type::NOCLIP,
        MoveType::Ufo => pm_type::UFO,
        MoveType::Spectator => pm_type::SPECTATOR,
        MoveType::Intermission => pm_type::INTERMISSION,
        MoveType::LastStand => pm_type::LAST_STAND,
        MoveType::Dead => pm_type::DEAD,
        MoveType::DeadLinked => pm_type::DEAD_LINKED,
    }
}

fn buttons_of(cmd: &MoveCommand) -> Buttons {
    const MAP: [(u32, u32); 17] = [
        (intent::ATTACK, buttons::ATTACK),
        (intent::SPRINT, buttons::SPRINT),
        (intent::MELEE, buttons::MELEE),
        (intent::USE, buttons::USE),
        (intent::RELOAD, buttons::RELOAD),
        (intent::USE_RELOAD, buttons::USE_RELOAD),
        (intent::LEAN_LEFT, buttons::LEAN_LEFT),
        (intent::LEAN_RIGHT, buttons::LEAN_RIGHT),
        (intent::PRONE, buttons::PRONE),
        (intent::CROUCH, buttons::CROUCH),
        (intent::JUMP, buttons::JUMP),
        (intent::ADS, buttons::ADS),
        (intent::STANCE_HELD, buttons::STANCE),
        (intent::HOLD_BREATH, buttons::HOLD_BREATH),
        (intent::FRAG, buttons::FRAG),
        (intent::SMOKE, buttons::SMOKE),
        (intent::THROW, buttons::THROW),
    ];
    let mut out = Buttons::default();
    for (from, to) in MAP {
        if cmd.buttons & from != 0 {
            out.press(to);
        }
    }
    if cmd.buttons & intent::TALKING != 0 {
        out.0[0] |= 0x4;
    }
    out
}

fn dive_asked(ps: &PlayerState, cmd: &MoveCommand) -> bool {
    cmd.buttons & intent::PRONE != 0
        && ps.pm_flags & pm_flags::SPRINTING != 0
        && ps.view_height_current >= 60.0
}

struct Bytes<'a> {
    buf: &'a mut [u8; GAME_MOVE_BYTES],
    at: usize,
}

impl Bytes<'_> {
    fn word(&mut self, value: &mut u32, write: bool) {
        let slot = &mut self.buf[self.at..self.at + 4];
        if write {
            slot.copy_from_slice(&value.to_le_bytes());
        } else {
            *value = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
        }
        self.at += 4;
    }

    fn i(&mut self, value: &mut i32, write: bool) {
        let mut w = *value as u32;
        self.word(&mut w, write);
        *value = w as i32;
    }

    fn f(&mut self, value: &mut f32, write: bool) {
        let mut w = value.to_bits();
        self.word(&mut w, write);
        *value = f32::from_bits(w);
    }

    fn flag(&mut self, value: &mut bool, write: bool) {
        if write {
            self.buf[self.at] = u8::from(*value);
        } else {
            *value = self.buf[self.at] != 0;
        }
        self.at += 1;
    }

    fn byte(&mut self, value: &mut u8, write: bool) {
        if write {
            self.buf[self.at] = *value;
        } else {
            *value = self.buf[self.at];
        }
        self.at += 1;
    }
}

/// A held weapon's flags in one byte.
fn held_flags(held: &mut movement_t6::state::HeldWeapon, b: &mut Bytes<'_>, write: bool) {
    let mut bits = u8::from(held.needs_rechamber)
        | u8::from(held.used_before) << 1
        | u8::from(held.dual_mag) << 2
        | u8::from(held.riotshield) << 3
        | u8::from(held.zoom != 0) << 4
        | u8::from(held.overheating) << 5;
    b.byte(&mut bits, write);
    held.needs_rechamber = bits & 1 != 0;
    held.used_before = bits & 2 != 0;
    held.dual_mag = bits & 4 != 0;
    held.riotshield = bits & 8 != 0;
    held.zoom = i32::from(bits >> 4 & 1);
    held.overheating = bits & 0x20 != 0;
}

fn game_state(
    ps: &mut PlayerState,
    last: &mut UserCmd,
    buf: &mut [u8; GAME_MOVE_BYTES],
    write: bool,
) {
    let mut b = Bytes { buf, at: 0 };
    b.word(&mut ps.pm_flags, write);
    b.word(&mut ps.weap_flags, write);
    b.i(&mut ps.pm_time, write);
    b.i(&mut ps.foliage_sound_time, write);
    b.i(&mut ps.ground_surface_type, write);
    for c in &mut ps.ladder_vec {
        b.f(c, write);
    }
    b.i(&mut ps.jump_time, write);
    b.f(&mut ps.jump_origin_z, write);
    b.i(&mut ps.slick_start_time, write);
    b.i(&mut ps.legs_timer, write);
    b.i(&mut ps.torso_timer, write);
    b.i(&mut ps.damage_timer, write);
    b.i(&mut ps.damage_duration, write);
    b.word(&mut ps.e_flags, write);
    b.word(&mut ps.e_flags2, write);
    b.i(&mut ps.weaponstate, write);
    b.i(&mut ps.weaponstate_left, write);
    b.f(&mut ps.weapon_pos_frac, write);
    b.i(&mut ps.view_height_target, write);
    b.i(&mut ps.view_height_lerp_time, write);
    b.i(&mut ps.view_height_lerp_target, write);
    b.i(&mut ps.view_height_lerp_down, write);
    b.f(&mut ps.prone_direction, write);
    b.f(&mut ps.prone_direction_pitch, write);
    b.f(&mut ps.prone_torso_pitch, write);
    b.i(&mut ps.sprint_button_up_required, write);
    b.i(&mut ps.sprint_exhausted, write);
    b.i(&mut ps.last_sprint_start, write);
    b.i(&mut ps.last_sprint_end, write);
    b.i(&mut ps.sprint_start_max_length, write);
    b.i(&mut ps.dive_end_time, write);
    b.f(&mut ps.prone_check_torso_pitch, write);
    b.f(&mut ps.prone_check_waist_pitch, write);
    b.i(&mut ps.anim_lock_end, write);
    b.word(&mut ps.perks[0], write);
    b.word(&mut ps.perks[1], write);
    b.f(&mut ps.aim_spread_scale, write);
    b.i(&mut ps.move_disabled, write);
    let mut moves = (last.forwardmove as u8 as u32) | ((last.rightmove as u8 as u32) << 8);
    b.word(&mut moves, write);
    last.forwardmove = moves as u8 as i8;
    last.rightmove = (moves >> 8) as u8 as i8;
    b.word(&mut last.buttons.0[0], write);
    b.word(&mut last.buttons.0[1], write);

    b.i(&mut ps.weapon_time, write);
    b.i(&mut ps.weapon_delay, write);
    b.i(&mut ps.weapon_time_left, write);
    b.i(&mut ps.weapon_delay_left, write);
    b.i(&mut ps.weapon_idle_time, write);
    b.i(&mut ps.grenade_time_left, write);
    b.i(&mut ps.throw_back_grenade_owner, write);
    b.i(&mut ps.throw_back_grenade_time_left, write);
    b.i(&mut ps.weapon_restrict_kick_time, write);
    b.word(&mut ps.offhand_index, write);
    b.word(&mut ps.last_weapon_alt_mode_switch, write);
    b.word(&mut ps.melee_weapon, write);
    b.i(&mut ps.weapon_shot_count, write);
    b.i(&mut ps.weapon_shot_count_left, write);
    b.i(&mut ps.ads_delay_time, write);
    b.i(&mut ps.spread_override, write);
    b.i(&mut ps.spread_override_state, write);
    b.f(&mut ps.weapon_spin_lerp, write);
    b.f(&mut ps.hold_breath_scale, write);
    b.i(&mut ps.hold_breath_timer, write);
    b.i(&mut ps.charge_shot_timer, write);
    b.i(&mut ps.charge_shot_level, write);
    b.i(&mut ps.shots_fired_from_chamber, write);
    b.f(&mut ps.quick_scope_scale, write);
    b.i(&mut ps.quick_scope_timer, write);
    b.i(&mut ps.melee_charge_ent, write);
    b.i(&mut ps.melee_charge_dist, write);
    b.i(&mut ps.melee_charge_time, write);
    b.word(&mut ps.weap_lock_flags, write);
    b.i(&mut ps.airburst_distance, write);
    b.word(&mut ps.inventory_weapon, write);
    b.word(&mut ps.weap_anim, write);
    b.word(&mut ps.weap_anim_left, write);
    b.i(&mut ps.offhand_throw, write);
    b.i(&mut ps.ads_zoom_select, write);
    b.i(&mut ps.ads_zoom_time, write);
    b.flag(&mut ps.ads_zoom_latched, write);
    b.i(&mut ps.ads_prev_zoom_select, write);
    b.i(&mut ps.ads_prev_zoom_select_time, write);
    b.i(&mut ps.intro_shots_fired, write);
    b.word(&mut ps.other_flags, write);
    for held in &mut ps.held_weapons {
        b.word(&mut held.weapon, write);
        b.f(&mut held.heat, write);
        b.i(&mut held.fuel, write);
        held_flags(held, &mut b, write);
    }
    let mut being_revived = ps.pm_type == pm_type::LAST_STAND_REVIVED;
    b.flag(&mut being_revived, write);
    if !write && ps.pm_type == pm_type::LAST_STAND && being_revived {
        ps.pm_type = pm_type::LAST_STAND_REVIVED;
    }
    b.i(&mut ps.water_level, write);
    b.f(&mut ps.mantle_yaw, write);
    b.i(&mut ps.mantle_timer, write);
    b.i(&mut ps.mantle_trans_index, write);
    b.word(&mut ps.mantle_flags, write);
    b.i(&mut ps.mover_entity_num, write);
    b.i(&mut ps.mover_timestamp, write);
    let mut direction = ps.dmg_direction as u8;
    b.byte(&mut direction, write);
    ps.dmg_direction = i32::from(direction);
    let mut kind = ps.dmg_type as u8;
    b.byte(&mut kind, write);
    ps.dmg_type = i32::from(kind);
    b.i(&mut ps.shellshock_index, write);
    b.i(&mut ps.shellshock_time, write);
    b.i(&mut ps.shellshock_duration, write);
}

pub(crate) fn unpack(player: &mut MovePlayer) -> (PlayerState, UserCmd) {
    let mut ps = PlayerState {
        client_num: player.client_num,
        command_time: player.command_time,
        pm_type: pm_type_of(player.move_type),
        origin: player.origin,
        velocity: player.velocity,
        viewangles: player.viewangles,
        delta_angles: player.delta_angles,
        gravity: player.gravity,
        speed: player.speed,
        ground_entity_num: t6_entity(player.ground_entity),
        weapon: player.weapon,
        move_speed_scale_multiplier: player.move_speed_scale,
        view_height_current: player.view_height,
        bob_cycle: player.bob_cycle,
        leanf: player.leanf,
        movement_dir: player.movement_dir,
        // IW4L seats no Black Ops 2 player in a vehicle.
        vehicle_entity: ENTITYNUM_NONE,
        ..PlayerState::default()
    };
    let mut last = UserCmd::default();
    game_state(&mut ps, &mut last, &mut player.game, false);
    ps.weapon_pos_frac = player.weapon_pos_frac;
    ps.aim_spread_scale = player.aim_spread_scale;
    (ps, last)
}

pub(crate) fn pack(player: &mut MovePlayer, ps: &mut PlayerState, last: &mut UserCmd) {
    player.command_time = ps.command_time;
    player.origin = ps.origin;
    player.velocity = ps.velocity;
    player.viewangles = ps.viewangles;
    player.delta_angles = ps.delta_angles;
    player.ground_entity = shared_entity(ps.ground_entity_num);
    player.view_height = ps.view_height_current;
    player.view_height_target = ps.view_height_target;
    player.bob_cycle = ps.bob_cycle;
    player.leanf = ps.leanf;
    player.movement_dir = ps.movement_dir;
    player.stance = match movement_t6::effective_stance(ps) {
        1 => Stance::Prone,
        2 => Stance::Crouch,
        _ => Stance::Stand,
    };
    player.sprinting = ps.pm_flags & pm_flags::SPRINTING != 0;
    player.weapon = ps.weapon;
    player.weapon_pos_frac = ps.weapon_pos_frac;
    player.aim_spread_scale = ps.aim_spread_scale;
    player.weapon_state = ps.weaponstate;
    player.weap_anim = ps.weap_anim;
    player.weap_anim_left = Some(ps.weap_anim_left);
    player.offhand = ps.offhand_index;
    player.grenade_time_left = ps.grenade_time_left;
    game_state(ps, last, &mut player.game, true);
}

struct World<'a> {
    world: &'a dyn MoveWorld,
    water_unknown: core::cell::Cell<bool>,
    height_lock_unknown: core::cell::Cell<bool>,
    target_velocity_unknown: core::cell::Cell<bool>,
}

impl movement_t6::MoveWorld for World<'_> {
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> movement_t6::Trace {
        let t: MoveTrace = self.world.trace(
            start,
            end,
            mins,
            maxs,
            shared_entity(pass_entity),
            contentmask,
        );
        movement_t6::Trace {
            normal: t.normal,
            fraction: t.fraction,
            surface_flags: t.surface_flags,
            contents: t.contents,
            entity: t6_entity(t.entity),
            allsolid: t.allsolid,
            startsolid: t.startsolid,
            walkable: t.walkable,
            glass: t.glass,
        }
    }

    fn can_stand_on(&self, entity: i32) -> bool {
        !self.world.is_player(shared_entity(entity))
    }

    fn weapon(&self, weapon: u32) -> movement_t6::Weapon<'_> {
        static ZERO_DEF: [u8; def::SIZE] = [0; def::SIZE];
        static ZERO_VARIANT: [u8; variant::SIZE] = [0; variant::SIZE];
        let found = self.world.weapon(weapon).and_then(|w| {
            Some(movement_t6::Weapon {
                def: WeaponDefView::new(&w.def, &w.variant)?,
                ammo_index: w.ammo_index,
                clip_index: w.clip_index,
                alt_weapon: w.alt_weapon,
                dual_wield_weapon: w.dual_wield_weapon,
                named_anims: w.named_anims,
                name: &w.name,
            })
        });
        found.unwrap_or_else(|| movement_t6::Weapon {
            def: WeaponDefView::new(&ZERO_DEF, &ZERO_VARIANT).expect("full-size blocks"),
            ammo_index: 0,
            clip_index: 0,
            alt_weapon: 0,
            dual_wield_weapon: 0,
            named_anims: 0,
            name: "",
        })
    }

    fn weapon_exists(&self, weapon: u32) -> bool {
        self.world.weapon(weapon).is_some()
    }

    fn variant_parent(&self, weapon: u32) -> Option<u32> {
        self.world.weapon(weapon)?.variant_parent
    }

    fn anim_length_ms(&self, name: &str) -> Option<i32> {
        self.world.anim_length_ms(name)
    }

    fn anim_root_delta(&self, name: &str, frac: f32) -> Option<[f32; 3]> {
        self.world.anim_root_delta(name, frac)
    }

    fn height_lock(&self, point: [f32; 3], reach: f32) -> Option<bool> {
        match self
            .world
            .volume_column(HEIGHT_LOCK_CLASS, HEIGHT_LOCK_LIMIT, point, reach)
        {
            VolumeColumn::NoVolumes => None,
            VolumeColumn::Inside => Some(true),
            VolumeColumn::Outside => Some(false),
            VolumeColumn::Unknown => {
                self.height_lock_unknown.set(true);
                None
            }
        }
    }

    fn entity_origin(&self, entity: i32) -> Option<[f32; 3]> {
        self.world.entity_origin(shared_entity(entity))
    }

    fn shock_movement(&self, index: i32) -> Option<f32> {
        self.world
            .shellshock(index)
            .map(|shock| shock.movement_scale)
    }

    fn entity_velocity(&self, entity: i32) -> Option<[f32; 3]> {
        let entity = shared_entity(entity);
        let velocity = self.world.entity_velocity(entity);
        if velocity.is_none() && !self.world.is_player(entity) {
            self.target_velocity_unknown.set(true);
        }
        velocity
    }

    /// The engine's water height: a line from `up` above to `down` below
    /// against the world's water, which counts where it strikes a surface
    /// with a surface type, or at its start when that is in water.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32> {
        let start = [origin[0], origin[1], origin[2] + up];
        let end = [origin[0], origin[1], origin[2] + down];
        let Some(t) = self.world.world_line(start, end, WATER_CONTENTS) else {
            self.water_unknown.set(true);
            return None;
        };
        if t.fraction != 1.0 && t.surface_flags & SURFACE_TYPE_MASK != 0 {
            return Some((end[2] - start[2]) * t.fraction + start[2]);
        }
        t.startsolid.then_some(start[2])
    }
}

impl PlayerMovement for T6Movement {
    fn pmove(
        &self,
        player: &mut MovePlayer,
        cmd: &MoveCommand,
        _oldcmd: &MoveCommand,
        world: &dyn MoveWorld,
        context: MoveContext,
    ) -> MoveOutcome {
        let (mut ps, mut last) = unpack(player);
        // The server's end of frame ends a shellshock whose time ran out.
        if !context.predicting
            && ps.shellshock_time.wrapping_add(ps.shellshock_duration) < context.level_time
        {
            ps.pm_flags &= !pm_flags::SHELLSHOCKED;
        }
        if player.frozen {
            ps.pm_flags |= pm_flags::FROZEN;
        } else {
            ps.pm_flags &= !pm_flags::FROZEN;
        }
        // The server's end of frame marks a playing client's state.
        if !matches!(
            player.move_type,
            MoveType::Spectator | MoveType::Intermission
        ) {
            ps.other_flags = (ps.other_flags & !(other_flags::PLAYER | other_flags::SPECTATOR))
                | other_flags::PLAYER;
        }
        let mut buttons = buttons_of(cmd);
        if dive_asked(&ps, cmd) {
            buttons.press(buttons::DIVE);
        }
        let t6cmd = UserCmd {
            server_time: cmd.server_time,
            buttons,
            angles: cmd.angles,
            weapon: cmd.weapon,
            offhand_index: cmd.offhand,
            alt_mode_weapon: cmd.alt_weapon,
            forwardmove: cmd.forwardmove,
            rightmove: cmd.rightmove,
            melee_charge_ent: cmd.melee_charge_ent.unwrap_or(ENTITYNUM_NONE as u16),
            melee_charge_dist: cmd.melee_charge_dist,
            fire_buttons_swapped: false,
            airburst_distance: 0,
        };
        let mask = if ps.pm_type == pm_type::SPECTATOR {
            SPECTATOR_CLIP_MASK
        } else if ps.pm_type >= pm_type::DEAD {
            DEAD_CLIP_MASK
        } else {
            PLAYER_CLIP_MASK
        };
        let adapter = World {
            world,
            water_unknown: core::cell::Cell::new(false),
            height_lock_unknown: core::cell::Cell::new(false),
            target_velocity_unknown: core::cell::Cell::new(false),
        };
        let held: [movement_t6::HeldRounds; HELD_WEAPONS] = core::array::from_fn(|i| {
            (
                player.held[i].weapon,
                player.held[i].clip,
                player.held[i].stock,
            )
        });
        movement_t6::load_held(&mut ps, &adapter, &held);
        let mut outcome = MoveOutcome::default();
        let view_weapon;
        {
            let mut pm = Pm::new(&mut ps, t6cmd, last, mask, &adapter);
            pm.zombiemode = self.zombies;
            pm.client_side = !context.predicting;
            let out = movement_t6::pmove(&mut pm);
            view_weapon = movement_t6::weapon::view_weapon(&pm);
            outcome.walking = out.walking;
            outcome.bounds = Some((out.mins, out.maxs));
            outcome.touched = pm.touched().iter().map(|&e| shared_entity(e)).collect();
            for event in pm.events() {
                outcome.signals.push(match event {
                    MoveEvent::Entity(event, parm) => MoveSignal::Event { event, parm },
                    MoveEvent::Anim(event, _, _) => MoveSignal::Anim { event },
                    MoveEvent::ProneAnim => MoveSignal::Anim { event: -1 },
                    MoveEvent::LegsMove(movetype, _) => MoveSignal::LegsMove { movetype },
                });
            }
            if let Some((end, duration_ms)) = pm.mantle_started {
                outcome
                    .signals
                    .push(MoveSignal::MantleStart { end, duration_ms });
            }
            outcome.gaps.extend(pm.gaps().map(gap_unknown));
            if outcome
                .signals
                .iter()
                .any(|s| matches!(s, MoveSignal::MantleStart { .. }))
            {
                outcome.gaps.push(events_unknown());
            }
            last = pm.cmd;
        }
        if adapter.height_lock_unknown.get() {
            outcome.gaps.push(gap_unknown(Gap::HeightLock));
        }
        if adapter.target_velocity_unknown.get() {
            outcome.gaps.push(target_velocity_unknown());
        }
        if adapter.water_unknown.get() {
            outcome.gaps.push(water_unknown());
        }
        for (slot, (weapon, clip, stock)) in movement_t6::held_rounds(&ps, &adapter)
            .into_iter()
            .enumerate()
        {
            player.held[slot] = HeldWeapon {
                weapon,
                clip,
                stock,
                left_clip: player.held[slot].left_clip,
            };
        }
        pack(player, &mut ps, &mut last);
        player.view_weapon = (view_weapon != player.weapon).then_some(view_weapon);
        outcome
    }

    fn think(&self, player: &mut MovePlayer, dvar: &dyn Fn(&str) -> Option<String>) {
        // The server's spectator think sets its own speed and runs no other.
        if player.move_type == MoveType::Spectator {
            player.speed = if player.free_look { SPECTATOR_SPEED } else { 0 };
            return;
        }
        let (ps, _) = unpack(player);
        let mut speed = dvar("g_speed")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(190.0);
        if movement_t6::perks::has(ps.perks, movement_t6::perks::MOVE_FASTER) {
            let mult = dvar("perk_speedMultiplier")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(movement_t6::tuning::MOVE_FASTER_SPEED_MULTIPLIER);
            speed *= mult;
        }
        player.speed = speed as i32;
        player.gravity = dvar("bg_gravity")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(800.0) as i32;
    }

    fn moves_spectators(&self) -> bool {
        true
    }

    fn spawn(&self, player: &mut MovePlayer) {
        let mut ps = PlayerState {
            view_height_target: view_height::STAND,
            view_height_current: view_height::STAND as f32,
            ..PlayerState::default()
        };
        let mut last = UserCmd::default();
        player.game = [0; GAME_MOVE_BYTES];
        player.view_height = ps.view_height_current;
        player.view_height_target = ps.view_height_target;
        player.stance = Stance::Stand;
        player.sprinting = false;
        game_state(&mut ps, &mut last, &mut player.game, true);
    }

    /// `shellshock` keeps the shock's number, start and duration and marks
    /// the player shellshocked (the builtin hands only a living player's
    /// shock over); `stopshellshock` clears all four.
    fn shellshock(&self, player: &mut MovePlayer, shock: Option<ShellshockStart>) -> bool {
        let (mut ps, mut last) = unpack(player);
        match shock {
            Some(shock) => {
                ps.shellshock_index = shock.index;
                ps.shellshock_time = shock.level_time;
                ps.shellshock_duration = shock.duration_ms;
                ps.pm_flags |= pm_flags::SHELLSHOCKED;
            }
            None => {
                ps.shellshock_index = 0;
                ps.shellshock_time = 0;
                ps.shellshock_duration = 0;
                ps.pm_flags &= !pm_flags::SHELLSHOCKED;
            }
        }
        pack(player, &mut ps, &mut last);
        true
    }

    fn restrict(&self, player: &mut MovePlayer, what: MoveRestriction, allowed: bool) {
        let (mut ps, mut last) = unpack(player);
        let (bit, in_other) = match what {
            MoveRestriction::Jump => (pm_flags::NO_JUMP, false),
            MoveRestriction::Sprint => (pm_flags::SPRINT_DISABLED, false),
            MoveRestriction::Stand => (pm_flags::NO_STAND, false),
            MoveRestriction::Crouch => (pm_flags::NO_CROUCH, false),
            MoveRestriction::Prone => (pm_flags::NO_PRONE, false),
            MoveRestriction::Lean => (pm_flags::NO_LEAN, false),
            MoveRestriction::Ads => (movement_t6::state::weap_flags::NO_ADS, true),
        };
        let word = if in_other {
            &mut ps.weap_flags
        } else {
            &mut ps.pm_flags
        };
        if allowed {
            *word &= !bit;
        } else {
            *word |= bit;
        }
        pack(player, &mut ps, &mut last);
    }

    fn set_stance(&self, player: &mut MovePlayer, stance: Stance) -> MoveOutcome {
        let (mut ps, mut last) = unpack(player);
        let (target, flags, event) = match stance {
            Stance::Stand => (view_height::STAND, 0, event::STANCE_FORCE_STAND),
            Stance::Crouch => (
                view_height::CROUCH,
                pm_flags::DUCKED,
                event::STANCE_FORCE_CROUCH,
            ),
            Stance::Prone => {
                if ps.pm_flags & pm_flags::PRONE == 0 {
                    ps.prone_direction = ps.viewangles[1];
                }
                (
                    view_height::PRONE,
                    pm_flags::PRONE,
                    event::STANCE_FORCE_PRONE,
                )
            }
        };
        ps.view_height_target = target;
        ps.pm_flags = (ps.pm_flags & !(pm_flags::PRONE | pm_flags::DUCKED)) | flags;
        pack(player, &mut ps, &mut last);
        MoveOutcome {
            signals: vec![MoveSignal::Event { event, parm: 0 }],
            ..MoveOutcome::default()
        }
    }

    /// What `finishplayerdamage` does to a player going down: its down
    /// animation event by stance, then what the player lets go of
    /// (`player_damage::going_down`).
    fn enter_last_stand(
        &self,
        player: &mut MovePlayer,
        weapons: &dyn game_api::movement::WeaponTable,
    ) -> MoveOutcome {
        use movement_t6::events::anim;
        let (mut ps, mut last) = unpack(player);
        ps.pm_type = pm_type::LAST_STAND;
        let event = if ps.pm_flags & pm_flags::DUCKED != 0 {
            anim::CROUCH_TO_LAST_STAND
        } else if ps.pm_flags & pm_flags::PRONE != 0 {
            anim::PRONE_TO_LAST_STAND
        } else {
            anim::STAND_TO_LAST_STAND
        };
        let mut gaps = Vec::new();
        if crate::player_damage::going_down(&mut ps, weapons) {
            gaps.push(gap_unknown(Gap::MountedView));
        }
        pack(player, &mut ps, &mut last);
        MoveOutcome {
            signals: vec![MoveSignal::Anim { event }],
            gaps,
            ..MoveOutcome::default()
        }
    }

    fn revive(&self, player: &mut MovePlayer) -> MoveOutcome {
        use movement_t6::events::anim;
        let (mut ps, mut last) = unpack(player);
        ps.pm_type = pm_type::NORMAL;
        let event = if last.buttons.held(buttons::CROUCH) {
            anim::LAST_STAND_TO_CROUCH
        } else if last.buttons.held(buttons::PRONE) {
            anim::LAST_STAND_TO_PRONE
        } else {
            anim::LAST_STAND_TO_STAND
        };
        pack(player, &mut ps, &mut last);
        MoveOutcome {
            signals: vec![MoveSignal::Anim { event }],
            ..MoveOutcome::default()
        }
    }

    fn undo_last_stand(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.pm_type = pm_type::NORMAL;
        pack(player, &mut ps, &mut last);
    }

    fn set_being_revived(&self, player: &mut MovePlayer, being_revived: bool) {
        let (mut ps, mut last) = unpack(player);
        if matches!(
            ps.pm_type,
            pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED
        ) {
            ps.pm_type = if being_revived {
                pm_type::LAST_STAND_REVIVED
            } else {
                pm_type::LAST_STAND
            };
        }
        pack(player, &mut ps, &mut last);
    }

    /// `isonladder` and `ismantling` read the ladder and mantle pm flags;
    /// the rest is for a test player's log.
    fn status(&self, player: &MovePlayer) -> Option<MoveStatus> {
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
        Some(MoveStatus {
            on_ladder: ps.pm_flags & pm_flags::LADDER != 0,
            mantling: ps.pm_flags & pm_flags::MANTLE != 0,
            sprinting: ps.pm_flags & pm_flags::SPRINTING != 0,
            diving: ps.pm_flags & pm_flags::DIVING != 0,
            damage_timer: ps.damage_timer,
            no_melee: ps.pm_flags & pm_flags::NO_MELEE != 0,
            no_weapon_cycling: ps.weap_flags & movement_t6::state::weap_flags::CYCLING_DISABLED
                != 0,
            being_revived: ps.pm_type == pm_type::LAST_STAND_REVIVED,
            weapon_state: ps.weaponstate,
        })
    }

    /// `allowmelee` sets the no-melee pm flag; `disableweaponcycling` and
    /// `enableweaponcycling` the weapon flag that keeps an empty weapon in
    /// hand.
    fn set_script_flag(&self, player: &mut MovePlayer, flag: ScriptFlag, on: bool) -> Option<bool> {
        let (mut ps, mut last) = unpack(player);
        let (word, bit) = match flag {
            ScriptFlag::NoMelee => (&mut ps.pm_flags, pm_flags::NO_MELEE),
            ScriptFlag::NoWeaponCycling => (
                &mut ps.weap_flags,
                movement_t6::state::weap_flags::CYCLING_DISABLED,
            ),
            // The runtime's command mask still runs these for Black Ops 2.
            ScriptFlag::WeaponsDisabled | ScriptFlag::OffhandsDisabled => return None,
        };
        let was = *word & bit != 0;
        if on {
            *word |= bit;
        } else {
            *word &= !bit;
        }
        pack(player, &mut ps, &mut last);
        Some(was)
    }

    /// `reviveplayer` clears the timer and where it started.
    fn clear_damage_timer(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.damage_timer = 0;
        ps.damage_duration = 0;
        pack(player, &mut ps, &mut last);
    }

    fn hurt(&self, player: &mut MovePlayer, hurt: &game_api::movement::PlayerHurt<'_>) -> i32 {
        let (mut ps, mut last) = unpack(player);
        let damage = crate::player_damage::hurt(&mut ps, hurt);
        pack(player, &mut ps, &mut last);
        damage
    }

    fn die(
        &self,
        player: &mut MovePlayer,
        death: &game_api::movement::PlayerHurt<'_>,
    ) -> Result<(), i32> {
        let (mut ps, mut last) = unpack(player);
        let out = crate::player_damage::die(&mut ps, death);
        pack(player, &mut ps, &mut last);
        out
    }

    fn cursor_hints(&self) -> &'static [&'static str] {
        &[
            "HINT_NONE",
            "HINT_NOICON",
            "HINT_SEAT",
            "HINT_ACTIVATE",
            "HINT_HEALTH",
            "HINT_SPECTATOR",
            "HINT_TEAMPOT",
            "HINT_WEAPON",
        ]
    }

    fn set_perk(&self, player: &mut MovePlayer, perk: &str, on: bool) -> bool {
        let Some(idx) = movement_t6::perks::index(perk) else {
            return false;
        };
        let (mut ps, mut last) = unpack(player);
        movement_t6::perks::set(&mut ps.perks, idx, on);
        pack(player, &mut ps, &mut last);
        true
    }

    fn has_perk(&self, player: &MovePlayer, perk: &str) -> bool {
        let Some(idx) = movement_t6::perks::index(perk) else {
            return false;
        };
        let mut player = *player;
        let (ps, _) = unpack(&mut player);
        movement_t6::perks::has(ps.perks, idx)
    }

    /// `isfiring`, `ismeleeing`, `isthrowinggrenade` and
    /// `isswitchingweapons` test the weapon state alone.
    fn weapon_activity(&self, player: &MovePlayer) -> WeaponActivity {
        let mut player = *player;
        let (ps, _) = unpack(&mut player);
        let state = ps.weaponstate;
        let meleeing = ws::is_melee(state);
        WeaponActivity {
            firing: meleeing || ws::is_firing(state),
            meleeing,
            throwing_grenade: ws::is_offhand(state),
            switching: ws::is_raising_or_dropping(state),
        }
    }

    /// Black Ops 2's end of a client's server frame: a changed weapon, a
    /// raise starting and ending, the firing states while alive, night
    /// vision, dive to prone and sprint.
    fn weapon_notifies(
        &self,
        player: &MovePlayer,
        memory: &mut WeaponNotifyMemory,
    ) -> Vec<WeaponNotify> {
        let weapon = player.weapon;
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
        let alive = ps.pm_type < pm_type::DEAD;
        let mut out = Vec::new();
        if weapon != memory.weapon {
            out.push(WeaponNotify::Change(weapon, memory.weapon));
            memory.weapon = weapon;
        }
        let raising = matches!(ps.weaponstate, ws::RAISING | ws::RAISING_ALT);
        if memory.raise_pending && !raising {
            out.push(WeaponNotify::ChangeComplete(weapon));
            memory.raise_pending = false;
        } else if raising {
            if !memory.raise_pending {
                out.push(WeaponNotify::Raising(weapon));
            }
            memory.raise_pending = true;
        }
        let firing = matches!(ps.weaponstate, ws::FIRING | ws::CONT_FIRE_LOOP) && alive;
        if firing != memory.firing {
            out.push(if firing {
                WeaponNotify::BeginFiring
            } else {
                WeaponNotify::EndFiring
            });
            memory.firing = firing;
        }
        let night_vision = ps.weap_flags & movement_t6::state::weap_flags::NIGHTVISION != 0;
        if night_vision != memory.night_vision {
            out.push(if night_vision {
                WeaponNotify::NightVisionOn
            } else {
                WeaponNotify::NightVisionOff
            });
            memory.night_vision = night_vision;
        }
        let dive = ws::is_dtp(ps.weaponstate) && alive;
        if dive != memory.dive {
            out.push(if dive {
                WeaponNotify::DiveStart
            } else {
                WeaponNotify::DiveEnd
            });
            memory.dive = dive;
        }
        let sprint = ps.last_sprint_start != 0 && ps.last_sprint_end < ps.last_sprint_start;
        if sprint != memory.sprint {
            out.push(if sprint {
                WeaponNotify::SprintBegin
            } else {
                WeaponNotify::SprintEnd
            });
            memory.sprint = sprint;
        }
        out
    }

    fn melee_target(&self, aim: &game_api::movement::MeleeAim<'_>) -> Option<MeleeTarget> {
        crate::automelee::melee_target(aim)
    }

    fn server_events(
        &self,
        before: &MovePlayer,
        player: &MovePlayer,
        signals: &[MoveSignal],
        world: &mut dyn game_api::fire::FireWorld,
    ) -> Vec<&'static Unknown> {
        let (ps, _) = unpack(&mut player.clone());
        let (was, _) = unpack(&mut before.clone());
        crate::fire::server_events(&ps, &was, signals, self.zombies, world)
    }

    fn clear_perks(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.perks = [0; 2];
        pack(player, &mut ps, &mut last);
    }

    fn footstep(
        &self,
        signal: &MoveSignal,
        _player: &MovePlayer,
    ) -> Option<(&'static str, &'static str)> {
        let MoveSignal::Event { event, parm } = *signal else {
            return None;
        };
        let kind = match event {
            event::FOOTSTEP_SPRINT => "sprint",
            event::FOOTSTEP_RUN => "run",
            event::FOOTSTEP_WALK => "walk",
            event::FOOTSTEP_CROUCH_RUN => "crouch_run",
            event::FOOTSTEP_CROUCH_WALK => "crouch_walk",
            event::FOOTSTEP_PRONE => "prone",
            event::LADDER_HAND_GRAB => "ladder_hand",
            _ => return None,
        };
        let surface = crate::missile::SURFACES
            .get(usize::try_from(parm).ok()?)
            .copied()?;
        Some((kind, surface))
    }

    fn jump_land(&self, signal: &MoveSignal) -> Option<game_api::movement::JumpLand> {
        use game_api::movement::JumpLand;
        const SURFACE_CLASSES: [&str; 9] = [
            "default", "metal", "concrete", "wood", "snow", "dirt", "gravel", "grass", "mud",
        ];
        let MoveSignal::Event { event, parm } = *signal else {
            return None;
        };
        let surface = |index: i32| {
            crate::missile::SURFACES
                .get(usize::try_from(index).ok()?)
                .copied()
        };
        let surface_class = |index: i32| {
            SURFACE_CLASSES
                .get(usize::try_from(index).ok()?)
                .copied()
                .or(Some("default"))
        };
        match event {
            event::FOLIAGE_SOUND => Some(JumpLand::Foliage),
            event::JUMP => Some(JumpLand::Jump(surface(parm)?)),
            e if (event::LAND..event::FALL_DAMAGE).contains(&e) => {
                Some(JumpLand::Land(surface(e - event::LAND)?, false))
            }
            e if (event::FALL_DAMAGE..event::FALL_DAMAGE + 32).contains(&e) => {
                Some(JumpLand::Land(surface(e - event::FALL_DAMAGE)?, true))
            }
            event::MANTLE => Some(JumpLand::Mantle),
            event::DIVE_START => Some(JumpLand::DiveStart),
            event::DIVE_LAND => Some(JumpLand::DiveLand(surface_class(parm)?)),
            event::SLIDE_LOOP_START => Some(JumpLand::SlideStart(surface_class(parm)?)),
            event::SLIDE_LOOP_END => Some(JumpLand::SlideStop(surface_class(parm)?)),
            event::NIGHTVISION_WEAR => Some(JumpLand::NightVision(true)),
            event::NIGHTVISION_REMOVE => Some(JumpLand::NightVision(false)),
            _ => None,
        }
    }

    fn weapon_spin(&self, player: &MovePlayer) -> f32 {
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
        ps.weapon_spin_lerp
    }

    fn set_weapon_spin(&self, player: &mut MovePlayer, spin: f32) {
        let (mut ps, mut last) = unpack(player);
        ps.weapon_spin_lerp = spin;
        pack(player, &mut ps, &mut last);
    }

    fn weapon_heat(&self, player: &MovePlayer, weapon: u32) -> (f32, bool) {
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
        movement_t6::weapon::heat(&ps, weapon)
    }

    fn set_weapon_heat(
        &self,
        player: &mut MovePlayer,
        weapon: u32,
        overheating: bool,
        heat: f32,
        end_val: f32,
        overheat_weapon: bool,
    ) {
        let (mut ps, mut last) = unpack(player);
        for (i, h) in player.held.iter().enumerate() {
            if h.weapon == weapon && ps.held_weapons[i].weapon != weapon {
                ps.held_weapons[i] = movement_t6::state::HeldWeapon {
                    weapon,
                    ..movement_t6::state::HeldWeapon::default()
                };
            }
        }
        if overheating {
            movement_t6::weapon::add_heat_raw(&mut ps, weapon, 100.0, end_val, overheat_weapon);
            movement_t6::weapon::add_heat_raw(
                &mut ps,
                weapon,
                heat - 100.0,
                end_val,
                overheat_weapon,
            );
        } else {
            movement_t6::weapon::add_heat_raw(&mut ps, weapon, -100.0, end_val, overheat_weapon);
            movement_t6::weapon::add_heat_raw(&mut ps, weapon, heat, end_val, overheat_weapon);
        }
        pack(player, &mut ps, &mut last);
    }
}
