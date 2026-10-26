//! Black Ops' player movement (`movement_t5`) behind `game_api`'s
//! `PlayerMovement`: the shared player fields come from the match, the rest
//! of Black Ops' player state lives in the player's game bytes.

use fastfile_t5::size::{WEAPON_DEF, WEAPON_VARIANT_DEF};
use fastfile_t5::weapon_def::WeaponDefView;
use game_api::movement::{
    ClientInputFrame, ClientInputState, ClientStanceCmd, ENTITY_NONE, ENTITY_WORLD,
    GAME_MOVE_BYTES, HELD_WEAPONS, HeldWeapon, MoveCommand, MoveContext, MoveOutcome, MovePlayer,
    MoveRestriction, MoveSignal, MoveStatus, MoveTrace, MoveType, MoveWorld, PlayerMovement,
    ScriptFlag, ShellshockStart, Stance, WaterSurface, WeaponActivity, WeaponNotify,
    WeaponNotifyMemory, buttons as intent,
};
use game_api::{Unknown, unknown};
use movement_t5::events::event;
use movement_t5::state::{
    ENTITYNUM_NONE, ENTITYNUM_WORLD, buttons, pm_flags, pm_type, view_height,
};
use movement_t5::{Buttons, Gap, MoveEvent, PlayerState, Pm, UserCmd};

pub struct T5Movement {
    /// The mode sets `zombiemode`.
    zombies: bool,
}

/// Black Ops' movement in its zombies mode.
pub static ZOMBIES: T5Movement = T5Movement { zombies: true };

const PLAYER_CLIP_MASK: u32 = 0x0281_c011;
const DEAD_CLIP_MASK: u32 = 0x0081_0011;

fn gap_unknown(gap: Gap) -> &'static Unknown {
    match gap {
        Gap::Mantle => unknown!(
            "t5.movement.mantle_anims",
            "a mantle whose PLAYER_MANTLE animations the match has not loaded is not started or stops where it is",
            "the player_mantle_* animations from the zone"
        ),
        Gap::Swim => unknown!(
            "t5.movement.swim",
            "Black Ops' swimming: a player in deep water does not move",
            "Black Ops' water move from its executable"
        ),
        Gap::MountedView => unknown!(
            "t5.movement.mounted",
            "Black Ops' movement on turrets and vehicles",
            "Black Ops' turret and vehicle player moves from its executable"
        ),
        Gap::ShellshockTorso => unknown!(
            "t5.movement.shellshock_torso",
            "a Black Ops shellshock that plays the torso's shellshock animation does not set the torso idle when it ends",
            "the player's torso animation kept by the game's movement"
        ),
        Gap::Overheat => unknown!(
            "t5.weapons.overheat",
            "Black Ops' weapon heat: a weapon that overheats never does",
            "Black Ops' weapon heat rules from its executable"
        ),
        Gap::Launched => unknown!(
            "t5.movement.launch",
            "Black Ops' bayonet launch",
            "Black Ops' launch move from its executable"
        ),
        Gap::AnimLock => unknown!(
            "t5.movement.anim_lock",
            "Black Ops' scripted animation lock on a player",
            "the player animation script durations that end the lock"
        ),
        Gap::PhysicsWorld => unknown!(
            "t5.movement.physics_world",
            "the match cannot list Black Ops' physics world, so the player sweeps the clip map (Black Ops' collision mode 0)",
            "the map's brush corner points and partition vertices in the match's collision"
        ),
    }
}

fn events_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.events",
        "Black Ops' movement events and player animations are not played (fall damage is): landing sounds, stance and jump sounds, other players' footsteps, the legs and torso animations others see",
        "Black Ops' client event handling and player animation script from its executable"
    )
}

fn water_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.water",
        "the water level under a Black Ops player: the match cannot tell, so players are taken as dry",
        "a water surface query over the map's water for the runtime"
    )
}

pub(crate) fn weapon_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.weapon",
        "a weapon without Black Ops' definition is read with every field zero",
        "the weapon's Black Ops weapon definition"
    )
}

fn physics_hit(hit: crate::physics::Hit) -> movement_t5::Trace {
    movement_t5::Trace {
        normal: hit.normal,
        fraction: hit.fraction,
        surface_flags: hit.surface_flags,
        contents: hit.contents,
        entity: hit.entity.map_or(ENTITYNUM_NONE, t5_entity),
        allsolid: false,
        startsolid: hit.startsolid,
        walkable: hit.walkable,
        glass: false,
    }
}

/// Black Ops numbers the world and no entity at the top of its entities.
fn t5_entity(entity: i32) -> i32 {
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
    const MAP: [(u32, u32); 21] = [
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
        (intent::NIGHTVISION, buttons::NIGHTVISION),
        (intent::OFFHAND_HOLD_CANCEL, buttons::OFFHAND_CANCEL),
        (intent::DIVE, buttons::DIVE),
        (intent::SPECIAL_FIRE, buttons::SPECIAL_FIRE),
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

/// The player state Black Ops keeps beyond the shared fields, in the game bytes.
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
}

/// Black Ops' own player fields and the previous command's moves.
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
    b.i(&mut ps.launch_time, write);
    b.i(&mut ps.anim_lock_end, write);
    b.word(&mut ps.perks, write);
    b.f(&mut ps.aim_spread_scale, write);
    b.i(&mut ps.move_disabled, write);
    b.i(&mut ps.shellshock_index, write);
    b.i(&mut ps.shellshock_time, write);
    b.i(&mut ps.shellshock_duration, write);
    b.word(&mut ps.mantle_flags, write);
    b.f(&mut ps.mantle_yaw, write);
    b.i(&mut ps.mantle_timer, write);
    b.i(&mut ps.mantle_trans_index, write);
    b.i(&mut ps.mantle_length, write);
    let mut moves = (last.forwardmove as u8 as u32) | ((last.rightmove as u8 as u32) << 8);
    b.word(&mut moves, write);
    last.forwardmove = moves as u8 as i8;
    last.rightmove = (moves >> 8) as u8 as i8;
    b.word(&mut last.buttons.0[0], write);
    b.word(&mut last.buttons.0[1], write);
    b.f(&mut last.gun_angles[0], write);
    b.f(&mut last.gun_angles[1], write);

    b.i(&mut ps.weapon_time, write);
    b.i(&mut ps.weapon_delay, write);
    b.i(&mut ps.weapon_time_left, write);
    b.i(&mut ps.weapon_delay_left, write);
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
    b.i(&mut ps.stack_fire_count, write);
    b.f(&mut ps.hold_breath_scale, write);
    b.i(&mut ps.hold_breath_timer, write);
    b.f(&mut ps.melee_charge_yaw, write);
    b.i(&mut ps.melee_charge_dist, write);
    b.i(&mut ps.melee_charge_time, write);
    b.word(&mut ps.weap_lock_flags, write);
    b.word(&mut ps.forced_anim_weapon, write);
    b.i(&mut ps.forced_anim_state, write);
    b.word(&mut ps.forced_anim_prev_weapon, write);
    b.word(&mut ps.weap_anim, write);
    b.word(&mut ps.weap_anim_left, write);
    b.i(&mut ps.offhand_throw, write);
    b.i(&mut ps.ads_zoom_select, write);
    b.i(&mut ps.ads_zoom_time, write);
    b.flag(&mut ps.ads_zoom_latched, write);
    b.word(&mut ps.other_flags, write);
    for held in &mut ps.held_weapons {
        b.word(&mut held.weapon, write);
        b.i(&mut held.fuel, write);
        b.flag(&mut held.needs_rechamber, write);
        b.flag(&mut held.used_before, write);
        b.flag(&mut held.dual_mag, write);
    }
    let mut being_revived = ps.pm_type == pm_type::LAST_STAND_REVIVED;
    b.flag(&mut being_revived, write);
    if !write && ps.pm_type == pm_type::LAST_STAND && being_revived {
        ps.pm_type = pm_type::LAST_STAND_REVIVED;
    }
}

/// Whether a script hid the viewmodel: Black Ops' client keeps it beside
/// its player state (the server's `hideviewmodel` command sets it); the
/// runtime carries it in the player's last game byte, past the fields above.
const VIEWMODEL_HIDDEN: usize = GAME_MOVE_BYTES - 1;

pub(crate) fn viewmodel_hidden(game: &[u8; GAME_MOVE_BYTES]) -> bool {
    game[VIEWMODEL_HIDDEN] != 0
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
        ground_entity_num: t5_entity(player.ground_entity),
        weapon: player.weapon,
        move_speed_scale_multiplier: player.move_speed_scale,
        view_height_current: player.view_height,
        bob_cycle: player.bob_cycle,
        leanf: player.leanf,
        movement_dir: player.movement_dir,
        ..PlayerState::default()
    };
    let mut last = UserCmd::default();
    game_state(&mut ps, &mut last, &mut player.game, false);
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
    player.stance = match movement_t5::effective_stance(ps) {
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
    physics: Option<crate::physics::PhysicsWorld<'a>>,
    weapon_unknown: core::cell::Cell<bool>,
    water_unknown: core::cell::Cell<bool>,
}

impl movement_t5::MoveWorld for World<'_> {
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> movement_t5::Trace {
        let t: MoveTrace = self.world.trace(
            start,
            end,
            mins,
            maxs,
            shared_entity(pass_entity),
            contentmask,
        );
        movement_t5::Trace {
            normal: t.normal,
            fraction: t.fraction,
            surface_flags: t.surface_flags,
            contents: t.contents,
            entity: t5_entity(t.entity),
            allsolid: t.allsolid,
            startsolid: t.startsolid,
            walkable: t.walkable,
            glass: t.glass,
        }
    }

    fn physics_collision(&self) -> bool {
        self.physics.is_some()
    }

    fn physics_trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> movement_t5::Trace {
        let pass = shared_entity(pass_entity);
        let hit = self
            .physics
            .as_ref()
            .and_then(|physics| physics.trace(start, end, mins, maxs, pass, contentmask));
        match hit {
            Some(hit) => physics_hit(hit),
            None => movement_t5::MoveWorld::trace(
                self,
                start,
                end,
                mins,
                maxs,
                pass_entity,
                contentmask,
            ),
        }
    }

    fn physics_ground_trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> movement_t5::Trace {
        let pass = shared_entity(pass_entity);
        let hit = self
            .physics
            .as_ref()
            .and_then(|physics| physics.ground_trace(start, end, mins, maxs, pass, contentmask));
        match hit {
            Some(hit) => physics_hit(hit),
            None => movement_t5::MoveWorld::trace(
                self,
                start,
                end,
                mins,
                maxs,
                pass_entity,
                contentmask,
            ),
        }
    }

    fn physics_slide(
        &self,
        slide: &movement_t5::PhysicsSlide,
        pass_entity: i32,
    ) -> Option<movement_t5::PhysicsSlid> {
        let mut slid = self
            .physics
            .as_ref()?
            .slide(slide, shared_entity(pass_entity))?;
        for entity in &mut slid.touched[..slid.touched_count] {
            *entity = t5_entity(*entity);
        }
        Some(slid)
    }

    fn can_stand_on(&self, entity: i32) -> bool {
        !self.world.is_player(shared_entity(entity))
    }

    fn weapon(&self, weapon: u32) -> movement_t5::Weapon<'_> {
        static ZERO_DEF: [u8; WEAPON_DEF] = [0; WEAPON_DEF];
        static ZERO_VARIANT: [u8; WEAPON_VARIANT_DEF] = [0; WEAPON_VARIANT_DEF];
        let found = self.world.weapon(weapon).and_then(|w| {
            Some(movement_t5::Weapon {
                def: WeaponDefView::new(&w.def, &w.variant)?,
                ammo_index: w.ammo_index,
                clip_index: w.clip_index,
                alt_weapon: w.alt_weapon,
                dual_wield_weapon: w.dual_wield_weapon,
                named_anims: w.named_anims,
                name: &w.name,
            })
        });
        found.unwrap_or_else(|| {
            self.weapon_unknown.set(true);
            movement_t5::Weapon {
                def: WeaponDefView::new(&ZERO_DEF, &ZERO_VARIANT).expect("full-size blocks"),
                ammo_index: 0,
                clip_index: 0,
                alt_weapon: 0,
                dual_wield_weapon: 0,
                named_anims: 0,
                name: "",
            }
        })
    }

    fn weapon_exists(&self, weapon: u32) -> bool {
        self.world.weapon(weapon).is_some()
    }

    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32> {
        match self.world.water_surface(origin, up, down) {
            WaterSurface::At(z) => Some(z),
            WaterSurface::Dry => None,
            WaterSurface::Unknown => {
                self.water_unknown.set(true);
                None
            }
        }
    }

    fn anim_length_ms(&self, name: &str) -> Option<i32> {
        self.world.anim_length_ms(name)
    }

    fn anim_root_delta(&self, name: &str, frac: f32) -> Option<[f32; 3]> {
        self.world.anim_root_delta(name, frac)
    }

    /// A shock the match does not know leaves movement as it is.
    fn shellshock(&self, index: i32) -> movement_t5::Shellshock {
        self.world.shellshock(index).map_or(
            movement_t5::Shellshock {
                movement: 1.0,
                animation: false,
            },
            |shock| movement_t5::Shellshock {
                movement: shock.movement_scale,
                animation: shock.animation,
            },
        )
    }
}

impl PlayerMovement for T5Movement {
    fn pmove(
        &self,
        player: &mut MovePlayer,
        cmd: &MoveCommand,
        _oldcmd: &MoveCommand,
        world: &dyn MoveWorld,
        context: MoveContext,
    ) -> MoveOutcome {
        let (mut ps, mut last) = unpack(player);
        // The server's client think ends a shellshock whose time ran out.
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
        let buttons = buttons_of(cmd);
        let t5cmd = UserCmd {
            server_time: cmd.server_time,
            buttons,
            angles: cmd.angles,
            weapon: cmd.weapon,
            offhand_index: cmd.offhand,
            alt_mode_weapon: cmd.alt_weapon,
            forwardmove: cmd.forwardmove,
            rightmove: cmd.rightmove,
            melee_charge_yaw: cmd.melee_charge_yaw,
            melee_charge_dist: cmd.melee_charge_dist,
            gun_angles: [0, 1].map(|axis| {
                const ANGLE2SHORT: f32 = 65536.0 / 360.0;
                const SHORT2ANGLE: f32 = 360.0 / 65536.0;
                let deg = movement_t5::math::angle_normalize_180(
                    ps.viewangles[axis] + cmd.gun_angle_offset[axis],
                );
                f32::from(((deg * ANGLE2SHORT) as i32) as i16) * SHORT2ANGLE
            }),
        };
        let mask = if ps.pm_type >= pm_type::DEAD {
            DEAD_CLIP_MASK
        } else {
            PLAYER_CLIP_MASK
        };
        let adapter = World {
            world,
            physics: world
                .lists_solid_pieces()
                .then(|| crate::physics::PhysicsWorld::new(world)),
            weapon_unknown: core::cell::Cell::new(false),
            water_unknown: core::cell::Cell::new(false),
        };
        let held: [movement_t5::HeldRounds; HELD_WEAPONS] = core::array::from_fn(|i| {
            (
                player.held[i].weapon,
                player.held[i].clip,
                player.held[i].stock,
                player.held[i].left_clip,
            )
        });
        movement_t5::load_held(&mut ps, &adapter, &held);
        let mut outcome = MoveOutcome::default();
        let view_weapon;
        {
            let mut pm = Pm::new(&mut ps, t5cmd, last, mask, &adapter);
            pm.zombiemode = self.zombies;
            pm.client_side = context.predicting;
            pm.move_yaw = context.move_yaw;
            let out = movement_t5::pmove(&mut pm);
            view_weapon = movement_t5::weapon::view_weapon(&pm);
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
            outcome.gaps = pm.gaps().map(gap_unknown).collect();
            if !outcome.signals.is_empty() {
                outcome.gaps.push(events_unknown());
            }
            last = pm.cmd;
        }
        if adapter.water_unknown.get() {
            outcome.gaps.push(water_unknown());
        }
        if adapter.weapon_unknown.get() {
            outcome.gaps.push(weapon_unknown());
        }
        for (slot, (weapon, clip, stock, left_clip)) in movement_t5::held_rounds(&ps, &adapter)
            .into_iter()
            .enumerate()
        {
            player.held[slot] = HeldWeapon {
                weapon,
                clip,
                stock,
                left_clip,
            };
        }
        pack(player, &mut ps, &mut last);
        // The viewmodel shows Black Ops' view weapon: the knife while
        // knifing, the offhand while one is thrown.
        player.view_weapon = (view_weapon != player.weapon).then_some(view_weapon);
        outcome
    }

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

    /// Black Ops' client think: run speed from `g_speed`, gravity from
    /// `bg_gravity`, each at its registered default when unset.
    fn think(&self, player: &mut MovePlayer, dvar: &dyn Fn(&str) -> Option<String>) {
        player.speed = dvar("g_speed")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(190);
        player.gravity = dvar("bg_gravity")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(800.0) as i32;
    }

    fn spawn(&self, player: &mut MovePlayer) {
        let mut ps = PlayerState {
            view_height_target: view_height::STAND,
            view_height_current: view_height::STAND as f32,
            ..PlayerState::default()
        };
        let mut last = UserCmd::default();
        let hidden = player.game[VIEWMODEL_HIDDEN];
        player.game = [0; GAME_MOVE_BYTES];
        player.game[VIEWMODEL_HIDDEN] = hidden;
        player.view_height = ps.view_height_current;
        player.view_height_target = ps.view_height_target;
        player.stance = Stance::Stand;
        player.sprinting = false;
        game_state(&mut ps, &mut last, &mut player.game, true);
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
            MoveRestriction::Ads => (movement_t5::state::weap_flags::NO_ADS, true),
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

    /// Black Ops' `setstance`: the stance flags and eye target, the prone
    /// direction when going prone, and the forced-stance event.
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
            gaps: vec![events_unknown()],
            ..MoveOutcome::default()
        }
    }

    /// Black Ops' last stand entry: raises the stance-to-last-stand animation
    /// event (`CROUCH_TO_LAST_STAND` when ducked, `PRONE_TO_LAST_STAND` when
    /// prone, else `STAND_TO_LAST_STAND`) and detaches a turret or vehicle.
    fn enter_last_stand(
        &self,
        player: &mut MovePlayer,
        _weapons: &dyn game_api::movement::WeaponTable,
    ) -> MoveOutcome {
        use movement_t5::events::anim;
        use movement_t5::state::e_flags;
        let (mut ps, mut last) = unpack(player);
        ps.pm_type = pm_type::LAST_STAND;
        let event = if ps.pm_flags & pm_flags::DUCKED != 0 {
            anim::CROUCH_TO_LAST_STAND
        } else if ps.pm_flags & pm_flags::PRONE != 0 {
            anim::PRONE_TO_LAST_STAND
        } else {
            anim::STAND_TO_LAST_STAND
        };
        let mut gaps = vec![events_unknown()];
        if ps.e_flags & (e_flags::TURRET | e_flags::VEHICLE_VIEW) != 0 {
            ps.e_flags &= !(e_flags::TURRET | e_flags::VEHICLE_VIEW);
            gaps.push(gap_unknown(Gap::MountedView));
        }
        pack(player, &mut ps, &mut last);
        MoveOutcome {
            signals: vec![MoveSignal::Anim { event }],
            gaps,
            ..MoveOutcome::default()
        }
    }

    /// Black Ops' `reviveplayer`: clears `beingRevived` and raises the
    /// last-stand-to-stance animation event from the player's last command
    /// buttons (`LAST_STAND_TO_CROUCH` when crouch is held,
    /// `LAST_STAND_TO_PRONE` when prone is held, else `LAST_STAND_TO_STAND`).
    fn revive(&self, player: &mut MovePlayer) -> MoveOutcome {
        use movement_t5::events::anim;
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
            gaps: vec![events_unknown()],
            ..MoveOutcome::default()
        }
    }

    fn undo_last_stand(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.pm_type = pm_type::NORMAL;
        pack(player, &mut ps, &mut last);
    }

    /// Black Ops' `startrevive` (`true`) and `stoprevive` (`false`): switches
    /// the downed player between `LAST_STAND_REVIVED` (which freezes crawling
    /// in `pmove`) and `LAST_STAND`.
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

    /// The ladder, mantle and no-melee pm flags, the damage timer, the
    /// weapon flag that keeps an empty weapon in hand and the weapon state,
    /// for a test player's log.
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
            no_weapon_cycling: ps.weap_flags & movement_t5::state::weap_flags::CYCLING_DISABLED
                != 0,
            being_revived: ps.pm_type == pm_type::LAST_STAND_REVIVED,
            weapon_state: ps.weaponstate,
        })
    }

    /// `allowmelee` sets the no-melee pm flag (`0x0800_0000`);
    /// `disableweaponcycling` and `enableweaponcycling` set and clear the
    /// weapon flag (`0x200`) that keeps an empty weapon in hand;
    /// `disableweapons` and `disableoffhandweapons` (and their enables) the
    /// weapon flags `0x80` and `0x100`. `disableweapons` also ends a dive at
    /// once.
    fn set_script_flag(&self, player: &mut MovePlayer, flag: ScriptFlag, on: bool) -> Option<bool> {
        use movement_t5::state::weap_flags;
        let (mut ps, mut last) = unpack(player);
        if flag == ScriptFlag::WeaponsDisabled && on {
            movement_t5::end_dive_for_disabled_weapons(&mut ps);
        }
        let (word, bit) = match flag {
            ScriptFlag::NoMelee => (&mut ps.pm_flags, pm_flags::NO_MELEE),
            ScriptFlag::NoWeaponCycling => (&mut ps.weap_flags, weap_flags::CYCLING_DISABLED),
            ScriptFlag::WeaponsDisabled => (&mut ps.weap_flags, weap_flags::WEAPONS_DISABLED),
            ScriptFlag::OffhandsDisabled => (&mut ps.weap_flags, weap_flags::OFFHANDS_DISABLED),
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

    fn hide_viewmodel(&self, player: &mut MovePlayer, hide: bool) -> Option<MoveOutcome> {
        let mut outcome = MoveOutcome::default();
        if hide {
            let (mut ps, mut last) = unpack(player);
            if let Some((event, parm)) = movement_t5::weapon::hide_viewmodel(&mut ps) {
                outcome.signals.push(MoveSignal::Event { event, parm });
            }
            pack(player, &mut ps, &mut last);
        }
        player.game[VIEWMODEL_HIDDEN] = u8::from(hide);
        Some(outcome)
    }

    fn crosshair_scan(
        &self,
        player: &mut MovePlayer,
        scan: &game_api::movement::CrosshairScan<'_>,
    ) {
        let (mut ps, mut last) = unpack(player);
        crate::crosshair_scan::scan(&mut ps, scan);
        pack(player, &mut ps, &mut last);
    }

    fn cursor_hints(&self) -> &'static [&'static str] {
        &[
            "HINT_NONE",
            "HINT_NOICON",
            "HINT_SEAT",
            "HINT_ACTIVATE",
            "HINT_HEALTH",
            "HINT_FRIENDLY",
            "HINT_SPECTATOR",
            "HINT_TEAMPOT",
        ]
    }

    /// Black Ops' `trigger_use` spawn starts a map trigger at
    /// `HINT_ACTIVATE`.
    fn map_use_cursor(&self) -> Option<i32> {
        Some(3)
    }

    fn set_perk(&self, player: &mut MovePlayer, perk: &str, on: bool) -> bool {
        let Some(bits) = movement_t5::perks::bits(perk) else {
            return false;
        };
        let (mut ps, mut last) = unpack(player);
        if on {
            ps.perks |= bits;
        } else {
            ps.perks &= !bits;
        }
        pack(player, &mut ps, &mut last);
        true
    }

    fn has_perk(&self, player: &MovePlayer, perk: &str) -> bool {
        let Some(bits) = movement_t5::perks::bits(perk) else {
            return false;
        };
        let mut player = *player;
        let (ps, _) = unpack(&mut player);
        ps.perks & bits != 0
    }

    /// `isFiring`, `isMeleeing`, `isThrowingGrenade` and `isSwitchingWeapons`
    /// test the weapon state alone.
    fn weapon_activity(&self, player: &MovePlayer) -> WeaponActivity {
        use movement_t5::weapon::state as ws;
        let mut player = *player;
        let (ps, _) = unpack(&mut player);
        let state = ps.weaponstate;
        let meleeing = ws::is_melee(state);
        WeaponActivity {
            firing: meleeing
                || matches!(
                    state,
                    ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END | ws::CONT_FIRE_LOOP
                ),
            meleeing,
            throwing_grenade: ws::is_offhand(state),
            switching: ws::is_raising_or_dropping(state),
        }
    }

    /// Black Ops' end of a client's server frame: a changed weapon; the end
    /// of a raise (pending from a frame raising); the firing states while
    /// alive; night vision.
    fn weapon_notifies(
        &self,
        player: &MovePlayer,
        memory: &mut WeaponNotifyMemory,
    ) -> Vec<WeaponNotify> {
        use movement_t5::weapon::state as ws;
        let weapon = player.weapon;
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
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
            memory.raise_pending = true;
        }
        let firing =
            matches!(ps.weaponstate, ws::FIRING | ws::CONT_FIRE_LOOP) && ps.pm_type < pm_type::DEAD;
        if firing != memory.firing {
            out.push(if firing {
                WeaponNotify::BeginFiring
            } else {
                WeaponNotify::EndFiring
            });
            memory.firing = firing;
        }
        let night_vision = ps.weap_flags & movement_t5::state::weap_flags::NIGHTVISION != 0;
        if night_vision != memory.night_vision {
            out.push(if night_vision {
                WeaponNotify::NightVisionOn
            } else {
                WeaponNotify::NightVisionOff
            });
            memory.night_vision = night_vision;
        }
        out
    }

    fn server_events(
        &self,
        before: &MovePlayer,
        player: &MovePlayer,
        signals: &[MoveSignal],
        world: &mut dyn game_api::fire::FireWorld,
    ) -> Vec<&'static Unknown> {
        let (ps, _) = unpack(&mut player.clone());
        let (was, was_cmd) = unpack(&mut before.clone());
        // The server updates the client's gun angles after the command's
        // events, so a shot uses the previous command's gun angles (zeroed
        // on spawn).
        let gun_angles = if was.command_time != 0 {
            was_cmd.gun_angles
        } else {
            [ps.viewangles[0], ps.viewangles[1]]
        };
        crate::fire::server_events(
            &ps,
            was.aim_spread_scale,
            gun_angles,
            signals,
            self.zombies,
            world,
        )
    }

    fn radius_damage(
        &self,
        blast: &game_api::fire::RadiusDamage,
        world: &mut dyn game_api::fire::FireWorld,
    ) -> Option<Vec<&'static Unknown>> {
        Some(crate::radius::radius_damage(blast, world))
    }

    fn actor_melee(
        &self,
        melee: &game_api::fire::ActorMelee,
        world: &mut dyn game_api::fire::FireWorld,
    ) -> Option<(i32, Vec<&'static Unknown>)> {
        Some(crate::fire::actor_melee(melee, world))
    }

    fn run_missile(
        &self,
        missile: &mut game_api::fire::GameMissile,
        world: &mut dyn game_api::fire::FireWorld,
    ) -> (game_api::fire::MissileRun, Vec<&'static Unknown>) {
        crate::missile::run_missile(missile, world)
    }

    fn reset_missile_detonation_time(
        &self,
        missile: &mut game_api::fire::GameMissile,
        after_ms: Option<i32>,
        world: &dyn game_api::fire::FireWorld,
    ) {
        crate::missile::reset_detonation_time(missile, after_ms, world);
    }

    /// Black Ops' client turns these events of its own player into its
    /// invalid command hint: no frag grenade (2), no special grenade (3),
    /// stand blocked (6), crouch blocked (7), target too close (8), lock-on
    /// required (9), not enough clearance (10).
    fn command_hint(&self, signal: &MoveSignal) -> Option<i32> {
        use movement_t5::events::event;
        let MoveSignal::Event { event, parm } = *signal else {
            return None;
        };
        match event {
            event::STANCE_FORCE_CROUCH | event::STANCE_FORCE_PRONE if parm == 1 => {
                Some(crate::code_hud::HINT_STAND_BLOCKED)
            }
            event::STANCE_FORCE_CROUCH | event::STANCE_FORCE_PRONE if parm == 2 => {
                Some(crate::code_hud::HINT_CROUCH_BLOCKED)
            }
            event::NO_FRAG_GRENADE_HINT => Some(crate::code_hud::HINT_NO_FRAG_GRENADE),
            event::NO_SPECIAL_GRENADE_HINT => Some(crate::code_hud::HINT_NO_SPECIAL_GRENADE),
            event::TARGET_TOO_CLOSE_HINT => Some(crate::code_hud::HINT_TARGET_TOO_CLOSE),
            event::LOCKON_REQUIRED_HINT => Some(crate::code_hud::HINT_LOCKON_REQUIRED),
            event::TARGET_NOT_ENOUGH_CLEARANCE => {
                Some(crate::code_hud::HINT_TARGET_NOT_ENOUGH_CLEARANCE)
            }
            _ => None,
        }
    }

    fn client_input(
        &self,
        state: &mut ClientInputState,
        frame: &ClientInputFrame<'_>,
    ) -> Option<u32> {
        const STANCE_HOLD_MS: i32 = 300;
        const DTP_HOLD_MS: i32 = 200;

        let key_override = frame.prone_key_active || frame.movedown_key_active;
        if !key_override {
            for &forced in frame.forced_stances {
                state.stance = match forced {
                    Stance::Stand => 0,
                    Stance::Crouch => 1,
                    Stance::Prone => 2,
                };
            }
        }

        for &cmd in frame.stance_cmds {
            match cmd {
                ClientStanceCmd::StanceDown(at) if !key_override => {
                    state.stance_held = true;
                    state.stance_press_time = at;
                    if !frame.sprint_standing {
                        state.prev_stance = state.stance;
                        if state.stance != 1 {
                            state.stance = 1;
                        }
                    }
                }
                ClientStanceCmd::StanceUp if !key_override => {
                    if frame.sprint_standing {
                        state.stance = 1;
                    } else if state.stance_held && state.prev_stance == 1 {
                        state.stance = 0;
                    }
                    state.stance_held = false;
                }
                ClientStanceCmd::LowerStance if !key_override => {
                    if state.stance == 0 {
                        state.stance = 1;
                    } else if state.stance == 1 {
                        state.stance = 2;
                    }
                }
                ClientStanceCmd::RaiseStance if !key_override => {
                    if state.stance == 1 {
                        state.stance = 0;
                    } else if state.stance == 2 {
                        state.stance = 1;
                    }
                }
                ClientStanceCmd::ToggleCrouch if !key_override => {
                    state.stance = u8::from(state.stance != 1);
                }
                ClientStanceCmd::ToggleProne if !key_override => {
                    let target = if frame.sprint_standing { 3 } else { 2 };
                    state.stance = if state.stance != target { target } else { 0 };
                }
                ClientStanceCmd::GoProne if !key_override => {
                    state.stance = 2;
                }
                ClientStanceCmd::GoCrouch if !key_override => {
                    state.stance = 1;
                }
                ClientStanceCmd::GoStandDown => {
                    if state.stance == 0 {
                        if !state.up_held_by_gostand && !state.up_held_by_moveup {
                            state.up_was_pressed = true;
                        }
                        state.up_held_by_gostand = true;
                    } else if !key_override {
                        state.stance = 0;
                    }
                }
                ClientStanceCmd::GoStandUp => {
                    state.up_held_by_gostand = false;
                }
                ClientStanceCmd::MoveUpDown if !key_override => {
                    if state.stance == 1 {
                        state.stance = 0;
                    } else if state.stance == 2 {
                        state.stance = 1;
                    } else {
                        if !state.up_held_by_gostand && !state.up_held_by_moveup {
                            state.up_was_pressed = true;
                        }
                        state.up_held_by_moveup = true;
                    }
                }
                ClientStanceCmd::MoveUpUp => {
                    state.up_held_by_moveup = false;
                }
                _ => {}
            }
        }

        let jam_time = frame
            .weapon
            .and_then(|w| WeaponDefView::new(&w.def, &w.variant))
            .filter(|def| def.fire_type() == 1)
            .map_or(0, |def| def.jam_fire_time());
        for &at in frame.attack_presses {
            let mut jammed = false;
            if !state.attack_latched && jam_time > 0 {
                let dt = at.wrapping_sub(state.last_attack_down_msec);
                if dt > 0 && dt < jam_time {
                    let chance = ((60000.0 / dt as f32) - (60000.0 / jam_time as f32)) as i32;
                    let chance = chance.clamp(0, 100);
                    if at.rem_euclid(100) < chance {
                        state.special_fire_active = true;
                        state.special_fire_was_pressed = true;
                        jammed = true;
                    }
                }
            }
            if !jammed && !state.attack_latched {
                state.attack_latched = true;
                state.last_attack_down_msec = at;
            }
        }
        if !frame.attack_key_active {
            state.attack_latched = false;
            state.special_fire_active = false;
        }

        let mut out = 0u32;
        if frame.prone_key_active {
            out |= intent::PRONE | intent::STANCE_HELD;
        } else if frame.movedown_key_active {
            out |= intent::CROUCH | intent::STANCE_HELD;
        } else {
            if state.stance_held {
                let (hold_ms, is_dtp) = if frame.sprint_standing {
                    (DTP_HOLD_MS, true)
                } else {
                    (STANCE_HOLD_MS, false)
                };
                if frame.now_msec.wrapping_sub(state.stance_press_time) >= hold_ms {
                    state.stance = if is_dtp {
                        3
                    } else if state.prev_stance == 2 {
                        0
                    } else {
                        2
                    };
                    state.stance_held = false;
                }
            }
            match state.stance {
                1 => out |= intent::CROUCH,
                2 => out |= intent::PRONE,
                3 => out |= intent::PRONE | intent::DIVE,
                _ => {}
            }
        }
        if state.up_held_by_gostand || state.up_held_by_moveup || state.up_was_pressed {
            out |= intent::JUMP;
            state.up_was_pressed = false;
        }
        if state.special_fire_active || state.special_fire_was_pressed {
            out |= intent::SPECIAL_FIRE;
            state.special_fire_was_pressed = false;
        }
        Some(out)
    }

    fn forced_stance(&self, signal: &MoveSignal) -> Option<Stance> {
        let MoveSignal::Event { event, .. } = *signal else {
            return None;
        };
        match event {
            event::STANCE_FORCE_STAND => Some(Stance::Stand),
            event::STANCE_FORCE_CROUCH => Some(Stance::Crouch),
            event::STANCE_FORCE_PRONE => Some(Stance::Prone),
            _ => None,
        }
    }

    fn resets_ads(&self, signal: &MoveSignal) -> bool {
        matches!(
            *signal,
            MoveSignal::Event {
                event: event::RESET_ADS,
                ..
            }
        )
    }

    fn offhand_cancel_state(&self, player: &MovePlayer) -> Option<u32> {
        use movement_t5::weapon::state as ws;
        let mut copy = *player;
        let (ps, _) = unpack(&mut copy);
        (ws::OFFHAND_INIT..=ws::OFFHAND)
            .contains(&ps.weaponstate)
            .then_some(ps.offhand_index)
    }

    /// Black Ops' client names a footstep event's movement type and its
    /// parm's surface for `CodeCallback_PlayerFootstep`.
    fn footstep(&self, signal: &MoveSignal) -> Option<(&'static str, &'static str)> {
        use movement_t5::events::event;
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
            _ => return None,
        };
        let surface = crate::grenade::SURFACE_NAMES
            .get(usize::try_from(parm).ok()?)
            .copied()?;
        Some((kind, surface))
    }

    /// Black Ops' client game tells its scripts of a jump (event 108, the
    /// surface in its parameter), a landing (109 plus the surface, or the
    /// fall-damage events 140 plus the surface, which hurt) and foliage
    /// (event 1), and sounds its mantle, dive and slide code sounds.
    fn jump_land(&self, signal: &MoveSignal) -> Option<game_api::movement::JumpLand> {
        use game_api::movement::JumpLand;
        use movement_t5::events::event;
        const SURFACE_CLASSES: [&str; 9] = [
            "default", "metal", "concrete", "wood", "snow", "dirt", "gravel", "grass", "mud",
        ];
        let MoveSignal::Event { event, parm } = *signal else {
            return None;
        };
        let surface = |index: i32| {
            crate::grenade::SURFACE_NAMES
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
            e if (event::FALL_DAMAGE..event::FALL_DAMAGE + 31).contains(&e) => {
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

    fn clear_perks(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.perks = 0;
        pack(player, &mut ps, &mut last);
    }
}
