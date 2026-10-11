//! Black Ops' player movement (`movement_t5`) behind `game_api`'s
//! `PlayerMovement`: the shared player fields come from the match, the rest
//! of Black Ops' player state lives in the player's game bytes.

use fastfile_t5::size::{WEAPON_DEF, WEAPON_VARIANT_DEF};
use fastfile_t5::weapon_def::WeaponDefView;
use game_api::movement::{
    ENTITY_NONE, ENTITY_WORLD, GAME_MOVE_BYTES, HELD_WEAPONS, HeldWeapon, MoveCommand, MoveContext,
    MoveOutcome, MovePlayer, MoveRestriction, MoveSignal, MoveTrace, MoveType, MoveWorld,
    PlayerMovement, Stance, WaterSurface, buttons as intent,
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
        Gap::Ladder => unknown!(
            "t5.movement.ladder",
            "Black Ops' ladder climbing: a player on a ladder does not move",
            "Black Ops' ladder move from its executable"
        ),
        Gap::Mantle => unknown!(
            "t5.movement.mantle",
            "Black Ops' mantling over ledges: never started",
            "Black Ops' mantle check and mantle move from its executable"
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
        Gap::Shellshock => unknown!(
            "t5.movement.shellshock",
            "a shellshocked Black Ops player moves at full speed",
            "the movement scale in Black Ops' shellshock files"
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
        Gap::SpecialMove => unknown!(
            "t5.movement.special",
            "Black Ops' linked, noclip, ufo, spectator and intermission moves",
            "those player moves from its executable"
        ),
    }
}

fn events_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.events",
        "Black Ops' movement events and player animations are not played: footsteps, landings, fall damage, stance and jump sounds, the legs and torso animations others see",
        "Black Ops' client event handling and player animation script from its executable"
    )
}

fn fire_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.fire",
        "what a Black Ops shot does is not run: bullets, projectiles, thrown grenades, melee hits",
        "Black Ops' server fire rules (bullets and spread, projectiles, grenades, melee) from its executable"
    )
}

/// The weapon events whose effect the server makes.
fn fires(event: i32) -> bool {
    use movement_t5::events::event as ev;
    matches!(
        event,
        ev::FIRE_WEAPON
            | ev::FIRE_WEAPON_LASTSHOT
            | ev::FIRE_WEAPON_LEFT
            | ev::FIRE_WEAPON_LASTSHOT_LEFT
            | ev::FIRE_MELEE
            | ev::USE_OFFHAND
            | ev::DETONATE
            | ev::GRENADE_SUICIDE
    )
}

fn water_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.water",
        "the water level under a Black Ops player: the match cannot tell, so players are taken as dry",
        "a water surface query over the map's water for the runtime"
    )
}

fn weapon_unknown() -> &'static Unknown {
    unknown!(
        "t5.movement.weapon",
        "a weapon without Black Ops' definition is read with every field zero",
        "the weapon's Black Ops weapon definition"
    )
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

/// Black Ops' client asks for a dive when the stance key goes down while
/// the player sprints standing.
fn dive_asked(ps: &PlayerState, cmd: &MoveCommand) -> bool {
    cmd.buttons & intent::PRONE != 0
        && ps.pm_flags & pm_flags::SPRINTING != 0
        && ps.view_height_current >= 60.0
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
}

fn unpack(player: &mut MovePlayer) -> (PlayerState, UserCmd) {
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

fn pack(player: &mut MovePlayer, ps: &mut PlayerState, last: &mut UserCmd) {
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
    player.offhand = ps.offhand_index;
    player.grenade_time_left = ps.grenade_time_left;
    game_state(ps, last, &mut player.game, true);
}

struct World<'a> {
    world: &'a dyn MoveWorld,
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
        }
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
        if player.frozen {
            ps.pm_flags |= pm_flags::FROZEN;
        } else {
            ps.pm_flags &= !pm_flags::FROZEN;
        }
        let mut buttons = buttons_of(cmd);
        if dive_asked(&ps, cmd) {
            buttons.press(buttons::DIVE);
        }
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
        };
        let mask = if ps.pm_type >= pm_type::DEAD {
            DEAD_CLIP_MASK
        } else {
            PLAYER_CLIP_MASK
        };
        let adapter = World {
            world,
            weapon_unknown: core::cell::Cell::new(false),
            water_unknown: core::cell::Cell::new(false),
        };
        let held: [movement_t5::HeldRounds; HELD_WEAPONS] = core::array::from_fn(|i| {
            (
                player.held[i].weapon,
                player.held[i].clip,
                player.held[i].stock,
            )
        });
        movement_t5::load_held(&mut ps, &adapter, &held);
        let mut outcome = MoveOutcome::default();
        {
            let mut pm = Pm::new(&mut ps, t5cmd, last, mask, &adapter);
            pm.zombiemode = self.zombies;
            pm.client_side = context.predicting;
            let out = movement_t5::pmove(&mut pm);
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
            if outcome
                .signals
                .iter()
                .any(|s| matches!(s, MoveSignal::Event { event, .. } if fires(*event)))
            {
                outcome.gaps.push(fire_unknown());
            }
            last = pm.cmd;
        }
        if adapter.water_unknown.get() {
            outcome.gaps.push(water_unknown());
        }
        if adapter.weapon_unknown.get() {
            outcome.gaps.push(weapon_unknown());
        }
        for (slot, (weapon, clip, stock)) in movement_t5::held_rounds(&ps, &adapter)
            .into_iter()
            .enumerate()
        {
            player.held[slot] = HeldWeapon {
                weapon,
                clip,
                stock,
            };
        }
        pack(player, &mut ps, &mut last);
        outcome
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
        player.game = [0; GAME_MOVE_BYTES];
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

    fn clear_perks(&self, player: &mut MovePlayer) {
        let (mut ps, mut last) = unpack(player);
        ps.perks = 0;
        pack(player, &mut ps, &mut last);
    }
}
