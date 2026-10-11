//! The runtime's player and command as another game's own movement sees
//! them (`game_api::movement`), and the player written back.

use crate::third_person::{
    PM_TYPE_DEAD, PM_TYPE_DEAD_LINKED, PM_TYPE_INTERMISSION, PM_TYPE_LAST_STAND,
    PM_TYPE_NORMAL_LINKED, PM_TYPE_SPECTATOR,
};
use crate::{ENTITYNUM_NONE, PlayerState, UserCmd, buttons, eflags, pm_flags};
use game_api::movement::{
    ENTITY_NONE, ENTITY_WORLD, HELD_WEAPONS, HeldWeapon, MoveCommand, MovePlayer, MoveType, Stance,
    buttons as intent,
};

const ENTITYNUM_WORLD: i32 = 0x7fe;

fn move_type(pm_type: i32) -> MoveType {
    match pm_type {
        PM_TYPE_NORMAL_LINKED => MoveType::NormalLinked,
        2 => MoveType::Noclip,
        3 => MoveType::Ufo,
        4 | PM_TYPE_SPECTATOR => MoveType::Spectator,
        PM_TYPE_INTERMISSION => MoveType::Intermission,
        PM_TYPE_LAST_STAND => MoveType::LastStand,
        PM_TYPE_DEAD => MoveType::Dead,
        PM_TYPE_DEAD_LINKED => MoveType::DeadLinked,
        _ => MoveType::Normal,
    }
}

fn shared_entity(entity: i32) -> i32 {
    match entity {
        ENTITYNUM_NONE => ENTITY_NONE,
        ENTITYNUM_WORLD => ENTITY_WORLD,
        n => n,
    }
}

fn own_entity(entity: i32) -> i32 {
    match entity {
        ENTITY_NONE => ENTITYNUM_NONE,
        ENTITY_WORLD => ENTITYNUM_WORLD,
        n => n,
    }
}

impl PlayerState {
    #[must_use]
    pub fn move_player(&self, client_num: i32, frozen: bool) -> MovePlayer {
        MovePlayer {
            client_num,
            command_time: self.command_time,
            move_type: move_type(self.pm_type),
            origin: self.origin,
            velocity: self.velocity,
            viewangles: self.viewangles,
            delta_angles: self.delta_angles,
            gravity: self.gravity,
            speed: self.speed,
            ground_entity: shared_entity(self.ground_entity_num),
            weapon: self.weapon,
            move_speed_scale: self.move_speed_scale_multiplier,
            frozen,
            view_height: self.view_height_current,
            view_height_target: self.view_height_target,
            bob_cycle: self.bob_cycle,
            leanf: self.leanf,
            movement_dir: self.movement_dir,
            stance: if self.pm_flags & pm_flags::PRONE != 0 {
                Stance::Prone
            } else if self.pm_flags & pm_flags::CROUCH != 0 {
                Stance::Crouch
            } else {
                Stance::Stand
            },
            sprinting: self.pm_flags & pm_flags::SPRINTING != 0,
            held: [HeldWeapon::default(); HELD_WEAPONS],
            weapon_pos_frac: self.f_weapon_pos_frac,
            aim_spread_scale: self.aim_spread_scale,
            weapon_state: 0,
            weap_anim: 0,
            offhand: u32::try_from(self.off_hand_index).unwrap_or(0),
            grenade_time_left: self.grenade_time_left,
            game: self.game_move,
        }
    }

    /// Writes the player back; its stance and sprint go to these flags, which
    /// the rest of the runtime reads.
    pub fn store_move_player(&mut self, player: &MovePlayer) {
        self.command_time = player.command_time;
        self.origin = player.origin;
        self.velocity = player.velocity;
        self.viewangles = player.viewangles;
        self.delta_angles = player.delta_angles;
        self.gravity = player.gravity;
        self.speed = player.speed;
        self.ground_entity_num = own_entity(player.ground_entity);
        self.view_height_current = player.view_height;
        self.view_height_target = player.view_height_target;
        self.bob_cycle = player.bob_cycle;
        self.leanf = player.leanf;
        self.movement_dir = player.movement_dir;
        let (flags, e_flags) = match player.stance {
            Stance::Stand => (0, 0),
            Stance::Crouch => (pm_flags::CROUCH, eflags::DUCK),
            Stance::Prone => (pm_flags::PRONE, eflags::PRONE),
        };
        let sprint = if player.sprinting {
            pm_flags::SPRINTING
        } else {
            0
        };
        self.pm_flags = (self.pm_flags
            & !(pm_flags::PRONE | pm_flags::CROUCH | pm_flags::SPRINTING))
            | flags
            | sprint;
        self.e_flags = (self.e_flags & !(eflags::DUCK | eflags::PRONE)) | e_flags;
        self.weapon = player.weapon;
        self.f_weapon_pos_frac = player.weapon_pos_frac;
        self.aim_spread_scale = player.aim_spread_scale;
        self.off_hand_index = player.offhand as i32;
        self.grenade_time_left = player.grenade_time_left;
        self.game_move = player.game;
    }
}

impl UserCmd {
    /// The command by what the player does; the runtime has no lean keys.
    #[must_use]
    pub fn move_command(&self) -> MoveCommand {
        const MAP: [(u32, u32); 15] = [
            (buttons::ATTACK, intent::ATTACK),
            (buttons::SPRINT, intent::SPRINT),
            (buttons::MELEE_CHARGE, intent::MELEE),
            (buttons::USE, intent::USE),
            (buttons::RELOAD, intent::RELOAD),
            (buttons::USE_RELOAD, intent::USE_RELOAD),
            (buttons::PRONE, intent::PRONE),
            (buttons::CROUCH, intent::CROUCH),
            (buttons::JUMP, intent::JUMP),
            (buttons::ADS, intent::ADS),
            (buttons::STANCE_HELD, intent::STANCE_HELD),
            (buttons::BREATH, intent::HOLD_BREATH),
            (buttons::FRAG, intent::FRAG),
            (buttons::SMOKE, intent::SMOKE),
            (buttons::THROW, intent::THROW),
        ];
        MoveCommand {
            server_time: self.server_time,
            buttons: MAP
                .iter()
                .filter(|(from, _)| self.buttons & from != 0)
                .fold(0, |bits, (_, to)| bits | to),
            angles: self.angles,
            forwardmove: self.forwardmove,
            rightmove: self.rightmove,
            weapon: u32::from(self.weapon),
            offhand: u32::from(self.off_hand_index),
            alt_weapon: u32::from(self.weapon_mapped),
            melee_charge_yaw: self.melee_charge_yaw,
            melee_charge_dist: self.melee_charge_dist,
        }
    }
}
