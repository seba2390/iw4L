//! A game's own player movement on the runtime's player: the shared fields
//! come from the player state and go back to it, what the game keeps for
//! itself rides in the player state's game bytes.

use crate::frame::FrameWorld;
use crate::world::ClientId;
use game_api::movement::{
    GameWeapon, HELD_WEAPONS, HeldWeapon, MoveCommand, MoveContext, MoveOutcome, MovePlayer,
    MoveRestriction, MoveSignal, MoveStatus, MoveTrace, MoveWorld, PlayerMovement, ScriptFlag,
    ShellshockStart, ShockMove, Stance, VolumeColumn, WaterSurface,
};

/// Sweeps `mins`..`maxs` from `start` to `end` against what `mask` names,
/// through the moving player.
pub(crate) type MoveTraceFn<'a> =
    dyn Fn([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> MoveTrace + 'a;

/// The game's own movement, when the match runs one.
pub(crate) fn rules(world: &FrameWorld) -> Option<&'static dyn PlayerMovement> {
    match world.bootstrap_ref().mode?.movement {
        game_api::Rule::Known(game_api::MovementRules::Game(movement)) => Some(movement),
        _ => None,
    }
}

/// The match runs the game's own weapon rules.
pub(crate) fn game_weapons(world: &FrameWorld) -> bool {
    world.bootstrap_ref().mode.map(|mode| mode.weapons)
        == Some(game_api::Rule::Known(game_api::WeaponRules::Game))
}

fn frozen(world: &FrameWorld, id: ClientId) -> bool {
    world.client_meta(id).is_some_and(|m| m.controls.frozen)
}

/// The match around the moving player: the runtime's collision, its players
/// and the game's weapon fields.
struct MatchMoveWorld<'a> {
    world: &'a FrameWorld<'a>,
    id: ClientId,
    content: std::sync::Arc<crate::SimContent>,
    trace: &'a MoveTraceFn<'a>,
}

impl MoveWorld for MatchMoveWorld<'_> {
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        _pass_entity: i32,
        mask: u32,
    ) -> MoveTrace {
        (self.trace)(start, end, mins, maxs, mask)
    }

    fn is_player(&self, entity: i32) -> bool {
        u32::try_from(entity).is_ok_and(|n| self.world.player(ClientId(n)).is_some())
    }

    fn weapon(&self, weapon: u32) -> Option<&GameWeapon> {
        self.content.game_weapon(weapon)
    }

    /// The moving player's shock: the one the match last started on them,
    /// which is the one their state names.
    fn shellshock(&self, _index: i32) -> Option<ShockMove> {
        let shock = self.world.client_meta(self.id)?.shellshock.as_ref()?;
        Some(ShockMove {
            movement_scale: shock.movement_scale,
            animation: shock.animation,
        })
    }

    /// A player's origin, or a script entity's while it is in use.
    fn entity_origin(&self, entity: i32) -> Option<[f32; 3]> {
        if let Ok(client) = u32::try_from(entity)
            && let Some(ps) = self.world.player(ClientId(client))
        {
            return Some(ps.origin);
        }
        let ecs = self.world.ecs_ref();
        let object = crate::script::object_by_number(ecs, entity)?;
        match ecs
            .get_resource::<crate::script::RoundScript>()?
            .peek_object_field(object, "origin")?
        {
            crate::script::Value::Vector(origin) => Some(origin),
            _ => None,
        }
    }

    /// A player's velocity, which is what the game's player state puts in
    /// its entity's trajectory; other entities' are not known.
    fn entity_velocity(&self, entity: i32) -> Option<[f32; 3]> {
        let client = u32::try_from(entity).ok()?;
        Some(self.world.player(ClientId(client))?.velocity)
    }

    /// The runtime has no water query yet.
    fn water_surface(&self, _origin: [f32; 3], _up: f32, _down: f32) -> WaterSurface {
        WaterSurface::Unknown
    }

    fn anim_length_ms(&self, name: &str) -> Option<i32> {
        Some(self.world.anim_clip_named(name)?.length_msec())
    }

    fn anim_root_delta(&self, name: &str, frac: f32) -> Option<[f32; 3]> {
        Some(self.world.anim_clip_named(name)?.abs_delta_trans(frac))
    }

    fn world_line(&self, start: [f32; 3], end: [f32; 3], mask: u32) -> Option<MoveTrace> {
        let content = &self.content;
        let trace = crate::world::clip_trace(
            content.clip_brushes(),
            content.clip_bsp(),
            content.clip_mesh(),
            start,
            end,
            [0.0; 3],
            [0.0; 3],
            mask,
            &|_| true,
        );
        Some(trace.move_trace())
    }

    fn volume_column(
        &self,
        classname: &str,
        limit: usize,
        point: [f32; 3],
        reach: f32,
    ) -> VolumeColumn {
        let Some(round) = self
            .world
            .ecs_ref()
            .get_resource::<crate::script::RoundScript>()
        else {
            return VolumeColumn::Unknown;
        };
        let mut volumes: Vec<(u32, [f32; 3])> = Vec::new();
        for pairs in round.zombies.authored.iter() {
            if volumes.len() >= limit {
                break;
            }
            let field = |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v.as_str())
            };
            if field("classname") != Some(classname) {
                continue;
            }
            let model = field("model")
                .and_then(|model| model.get(1..))
                .map_or(0, |n| n.trim().parse::<u32>().unwrap_or(0) & 0xffff);
            if volumes.iter().any(|(known, _)| *known == model) {
                continue;
            }
            let mut origin = [0.0_f32; 3];
            for (axis, value) in field("origin")
                .unwrap_or("")
                .split_whitespace()
                .take(3)
                .enumerate()
            {
                origin[axis] = value.parse().unwrap_or(0.0);
            }
            volumes.push((model, origin));
        }
        if volumes.is_empty() {
            return VolumeColumn::NoVolumes;
        }
        let start = [point[0], point[1], point[2] + reach];
        let end = [point[0], point[1], point[2] - reach];
        for (model, origin) in volumes {
            match crate::world::brush_model_line_hits(&self.content, model, origin, start, end) {
                None => return VolumeColumn::Unknown,
                Some(true) => return VolumeColumn::Inside,
                Some(false) => {}
            }
        }
        VolumeColumn::Outside
    }
}

/// Runs one command with the game's movement and writes the player back;
/// on the server, the shots its events fire follow.
pub(crate) fn run(
    world: &mut FrameWorld,
    movement: &'static dyn PlayerMovement,
    id: ClientId,
    cmd: MoveCommand,
    trace: &MoveTraceFn<'_>,
    tick: crate::world::Tick,
) -> MoveOutcome {
    let Some(ps) = world.player(id).copied() else {
        return MoveOutcome::default();
    };
    let mut player = ps.move_player(id.0 as i32, frozen(world, id));
    player.free_look = crate::script::host::spectators::free_look(world.ecs(), id);
    let held = held_rounds(world, id, ps.weapons);
    player.held = held;
    if world.publishes_snapshot() {
        let runtime = world.ecs().get_resource::<crate::script::MatchScript>();
        movement.think(&mut player, &|name| {
            runtime.and_then(|runtime| runtime.dvars.get(name).cloned())
        });
    }
    let context = MoveContext {
        predicting: !world.publishes_snapshot(),
        level_time: crate::level_time_ms(tick),
        move_yaw: world.top_down_move_yaw(id),
    };
    let before = player;
    let mut outcome = {
        let around = MatchMoveWorld {
            world: &*world,
            id,
            content: world.content(),
            trace,
        };
        movement.pmove(&mut player, &cmd, &MoveCommand::default(), &around, context)
    };
    // A command the game's movement could not run is still used up.
    player.command_time = player.command_time.max(cmd.server_time);
    if let Some(ps) = world.player_mut(id) {
        ps.store_move_player(&player);
    }
    apply_held(world, id, &held, &player.held);
    world.client_meta_mut(id).mirror_held_ammo(player.weapon);
    if world.publishes_snapshot()
        && outcome
            .signals
            .iter()
            .any(|signal| matches!(signal, MoveSignal::Event { .. }))
    {
        let mut around = crate::game_fire::MatchFireWorld::new(world, tick, Some(id));
        let gaps = movement.server_events(&before, &player, &outcome.signals, &mut around);
        outcome.gaps.extend(gaps);
    }
    if world.publishes_snapshot() {
        for signal in &outcome.signals {
            if let Some((kind, surface)) = movement.footstep(signal) {
                crate::script::client::footstep(world.ecs(), id, kind, surface);
            }
            if let Some(jump_land) = movement.jump_land(signal) {
                crate::script::client::jump_land(world.ecs(), id, jump_land);
            }
            // The server tells the player's scripts when the mantle ends.
            if let MoveSignal::MantleStart { duration_ms, .. } = *signal {
                use crate::script::host::entity_damage::{
                    NotifyValue, notify_entity, player_object_id,
                };
                if let Some(object) = player_object_id(world.ecs(), id.0) {
                    let end_time = crate::level_time_ms(tick) + duration_ms;
                    notify_entity(
                        world.ecs(),
                        object,
                        "mantle_start",
                        &[NotifyValue::Int(end_time)],
                    );
                }
            }
        }
    }
    outcome
}

/// The weapons in the player's slots and their rounds.
fn held_rounds(
    world: &FrameWorld,
    id: ClientId,
    slots: [i32; HELD_WEAPONS],
) -> [HeldWeapon; HELD_WEAPONS] {
    core::array::from_fn(|slot| {
        let weapon = u32::try_from(slots[slot]).unwrap_or(0);
        if weapon == 0 {
            return HeldWeapon::default();
        }
        HeldWeapon {
            weapon,
            clip: crate::script_player::ammo_clip(world, id, weapon),
            stock: crate::script_player::ammo_stock(world, id, weapon),
        }
    })
}

/// What the game's weapon rules did to the slots: rounds spent or loaded,
/// weapons dropped.
fn apply_held(
    world: &mut FrameWorld,
    id: ClientId,
    before: &[HeldWeapon; HELD_WEAPONS],
    after: &[HeldWeapon; HELD_WEAPONS],
) {
    for (was, now) in before.iter().zip(after) {
        if was == now || was.weapon == 0 {
            continue;
        }
        if now.weapon != was.weapon {
            crate::script_player::take_weapon(world, id, was.weapon);
            continue;
        }
        if now.clip != was.clip {
            crate::script_player::set_ammo_clip(world, id, now.weapon, now.clip);
        }
        if now.stock != was.stock {
            crate::script_player::set_ammo_stock(world, id, now.weapon, now.stock);
        }
    }
}

/// Changes a spawned player through the game's own movement.
fn update<T>(
    world: &mut FrameWorld,
    id: ClientId,
    change: impl FnOnce(&mut MovePlayer) -> T,
) -> Option<T> {
    let frozen = frozen(world, id);
    let ps = world.player(id).copied()?;
    let mut player = ps.move_player(id.0 as i32, frozen);
    player.held = held_rounds(world, id, ps.weapons);
    let out = change(&mut player);
    world.player_mut(id)?.store_move_player(&player);
    Some(out)
}

/// Starts (`Some`) or clears (`None`) the player's shellshock in the game's
/// own movement; false when the match runs the simulation's movement.
pub(crate) fn shellshock(
    world: &mut FrameWorld,
    id: ClientId,
    shock: Option<ShellshockStart>,
) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.shellshock(player, shock)).unwrap_or(false)
}

/// A script's restriction, kept by the game's own movement; false when the
/// match runs the simulation's.
pub(crate) fn restrict(
    world: &mut FrameWorld,
    id: ClientId,
    what: MoveRestriction,
    allowed: bool,
) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.restrict(player, what, allowed));
    true
}

/// A script's `setstance` through the game's own movement; false when the
/// match runs the simulation's.
pub(crate) fn set_stance(world: &mut FrameWorld, id: ClientId, stance: &str) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    let stance = match stance {
        "stand" => Stance::Stand,
        "crouch" => Stance::Crouch,
        "prone" => Stance::Prone,
        _ => return true,
    };
    if let Some(outcome) = update(world, id, |player| movement.set_stance(player, stance)) {
        for gap in outcome.gaps {
            world.report_game_gap(gap);
        }
    }
    true
}

/// Enters last stand in the game's own movement.
pub(crate) fn enter_last_stand(world: &mut FrameWorld, id: ClientId) {
    let Some(movement) = rules(world) else {
        return;
    };
    let content = world.content();
    if let Some(outcome) = update(world, id, |player| {
        movement.enter_last_stand(player, &*content)
    }) {
        for gap in outcome.gaps {
            world.report_game_gap(gap);
        }
    }
}

/// Revives a downed player in the game's own movement.
pub(crate) fn revive(world: &mut FrameWorld, id: ClientId) {
    let Some(movement) = rules(world) else {
        return;
    };
    if let Some(outcome) = update(world, id, |player| movement.revive(player)) {
        for gap in outcome.gaps {
            world.report_game_gap(gap);
        }
    }
}

/// Clears last stand in the game's own movement without raising an animation
/// event.
pub(crate) fn undo_last_stand(world: &mut FrameWorld, id: ClientId) {
    let Some(movement) = rules(world) else {
        return;
    };
    update(world, id, |player| movement.undo_last_stand(player));
}

/// Sets or clears `being_revived` on a downed player in the game's own
/// movement.
pub(crate) fn set_being_revived(world: &mut FrameWorld, id: ClientId, being_revived: bool) {
    let Some(movement) = rules(world) else {
        return;
    };
    update(world, id, |player| {
        movement.set_being_revived(player, being_revived);
    });
}

/// Whether the player is on a ladder or mantling by the game's own movement;
/// `None` when the match runs the simulation's.
pub(crate) fn status(world: &FrameWorld, id: ClientId) -> Option<MoveStatus> {
    let movement = rules(world)?;
    let ps = world.player(id)?;
    movement.status(&ps.move_player(id.0 as i32, frozen(world, id)))
}

/// Sets or clears a script's player flag in the game's own movement; whether
/// it was set before, `None` when the movement has no such flag.
pub(crate) fn set_script_flag(
    world: &mut FrameWorld,
    id: ClientId,
    flag: ScriptFlag,
    on: bool,
) -> Option<bool> {
    let movement = rules(world)?;
    update(world, id, |player| {
        movement.set_script_flag(player, flag, on)
    })
    .flatten()
}

/// What the game's own handler does to a hurt player's state before it takes
/// the health: `means` by the game's numbers, `dir` of length 1. Answers the
/// damage the game takes, `None` when the match runs the simulation's
/// movement.
pub(crate) fn hurt(
    world: &mut FrameWorld,
    id: ClientId,
    damage: i32,
    means: usize,
    weapon: u32,
    dir: Option<[f32; 3]>,
) -> Option<i32> {
    let movement = rules(world)?;
    let content = world.content();
    update(world, id, |player| {
        movement.hurt(
            player,
            &game_api::movement::PlayerHurt {
                damage,
                means,
                weapon,
                dir,
                weapons: &*content,
            },
        )
    })
}

/// What the game's own player death does to the player's state; `None`
/// when the match runs the simulation's movement, `Some(Err)` with the move
/// type when the death refuses to run.
pub(crate) fn die(
    world: &mut FrameWorld,
    id: ClientId,
    means: usize,
    weapon: u32,
) -> Option<Result<(), i32>> {
    let movement = rules(world)?;
    let content = world.content();
    update(world, id, |player| {
        movement.die(
            player,
            &game_api::movement::PlayerHurt {
                damage: 0,
                means,
                weapon,
                dir: None,
                weapons: &*content,
            },
        )
    })
}

/// Clears the damage timer of the game's own movement; false when the match
/// runs the simulation's.
pub(crate) fn clear_damage_timer(world: &mut FrameWorld, id: ClientId) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.clear_damage_timer(player));
    true
}

/// Sets or clears a perk the game's own movement reads; false when the match
/// runs the simulation's.
pub(crate) fn set_perk(world: &mut FrameWorld, id: ClientId, perk: &str, on: bool) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.set_perk(player, perk, on));
    true
}

pub(crate) fn clear_perks(world: &mut FrameWorld, id: ClientId) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.clear_perks(player));
    true
}

/// What the player's weapon is doing by the game's own weapon rules; `None`
/// when the match runs the simulation's.
pub(crate) fn weapon_activity(
    world: &FrameWorld,
    id: ClientId,
) -> Option<game_api::movement::WeaponActivity> {
    if !game_weapons(world) {
        return None;
    }
    let movement = rules(world)?;
    let Some(ps) = world.player(id) else {
        return Some(game_api::movement::WeaponActivity::default());
    };
    Some(movement.weapon_activity(&ps.move_player(id.0 as i32, false)))
}

pub(crate) fn weapon_spin(world: &FrameWorld, id: ClientId) -> Option<f32> {
    if !game_weapons(world) {
        return None;
    }
    let movement = rules(world)?;
    let ps = world.player(id)?;
    Some(movement.weapon_spin(&ps.move_player(id.0 as i32, false)))
}

pub(crate) fn set_weapon_spin(world: &mut FrameWorld, id: ClientId, spin: f32) -> bool {
    if !game_weapons(world) {
        return false;
    }
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.set_weapon_spin(player, spin));
    true
}

pub(crate) fn weapon_heat(world: &FrameWorld, id: ClientId, weapon: u32) -> Option<(f32, bool)> {
    if !game_weapons(world) {
        return None;
    }
    let movement = rules(world)?;
    let ps = world.player(id)?;
    Some(movement.weapon_heat(&ps.move_player(id.0 as i32, false), weapon))
}

pub(crate) fn set_weapon_heat(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    overheating: bool,
    heat: f32,
    end_val: f32,
    overheat_weapon: bool,
) -> bool {
    if !game_weapons(world) {
        return false;
    }
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| {
        movement.set_weapon_heat(player, weapon, overheating, heat, end_val, overheat_weapon)
    });
    true
}
