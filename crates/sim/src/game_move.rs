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

/// The match keeps rounds by the game's own ammo and clip numbers.
pub(crate) fn game_ammo_numbers(world: &FrameWorld) -> bool {
    world
        .bootstrap_ref()
        .mode
        .is_some_and(|mode| mode.ammo_numbers.is_some())
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
    solids: Option<&'a crate::world::solid_pieces::SolidScene<'a>>,
}

impl game_api::collision::SolidSource for MatchMoveWorld<'_> {
    fn lists_solid_pieces(&self) -> bool {
        self.solids.is_some()
    }

    fn solid_pieces<'s>(
        &'s self,
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
        pass_entity: i32,
        out: &mut Vec<game_api::collision::SolidPiece<'s>>,
    ) -> bool {
        let Some(scene) = self.solids else {
            return false;
        };
        crate::world::solid_pieces::gather(scene, mins, maxs, mask, pass_entity, out);
        true
    }

    fn clip_material_surface_flags(&self, name: &str) -> Option<u32> {
        crate::world::solid_pieces::clip_material_surface_flags(&self.content, name)
    }
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
    solids: Option<&crate::world::solid_pieces::SolidScene<'_>>,
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
            solids,
        };
        movement.pmove(&mut player, &cmd, &MoveCommand::default(), &around, context)
    };
    // A command the game's movement could not run is still used up.
    player.command_time = player.command_time.max(cmd.server_time);
    let dt_ms = (player.command_time - before.command_time).clamp(0, 200);
    if let Some(ps) = world.player_mut(id) {
        ps.legs_timer = (ps.legs_timer - dt_ms).max(0);
        ps.torso_timer = (ps.torso_timer - dt_ms).max(0);
        ps.store_move_player(&player);
        for signal in &outcome.signals {
            if let Some(stance) = movement.forced_stance(signal) {
                ps.push_stance_event(stance);
            }
            if movement.resets_ads(signal) {
                ps.push_reset_ads_event();
            }
        }
    }
    apply_held(world, id, &held, &player.held);
    world.client_meta_mut(id).mirror_held_ammo(player.weapon);
    apply_move_anim_signals(world, id, &player, &cmd, &outcome.signals);
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
            if let Some((kind, surface)) = movement.footstep(signal, &player) {
                crate::script::client::footstep(world.ecs(), id, kind, surface, cmd.server_time);
            }
            if let Some(kind) = movement.command_hint(signal) {
                let meta = world.client_meta_mut(id);
                meta.command_hint = (meta.command_hint.0.wrapping_add(1), kind);
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

fn map_t6_anim_event(event: i32) -> Option<u8> {
    Some(match event {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 5,
        5 => 6,
        6 => 7,
        7 => 22,
        8 => 10,
        9 => 11,
        10 => 12,
        11 => 13,
        12 => 14,
        13 => 16,
        14 => 23,
        15 => 24,
        16 => 17,
        17 => 25,
        18 => 18,
        19 => 19,
        20 => 26,
        21 => 20,
        26 => 27,
        27 => 28,
        28 => 29,
        29 => 30,
        30 => 31,
        31 => 32,
        32 => 33,
        33 => 34,
        34 => 35,
        35 => 36,
        36 => 37,
        _ => return None,
    })
}

fn map_t6_legs_movetype(player: &MovePlayer, cmd: &MoveCommand, movetype: i32) -> Option<u8> {
    use game_api::movement::MoveType;
    let backward = cmd.forwardmove < 0;
    let last_stand = player.move_type == MoveType::LastStand;
    Some(match movetype {
        1 => {
            if last_stand {
                51
            } else {
                match player.stance {
                    Stance::Stand => 1,
                    Stance::Crouch => 2,
                    Stance::Prone => 3,
                }
            }
        }
        2 => 54,
        3 => {
            if last_stand {
                if backward { 53 } else { 52 }
            } else {
                match (player.stance, backward) {
                    (Stance::Stand, false) => 4,
                    (Stance::Stand, true) => 5,
                    (Stance::Crouch, false) => 6,
                    (Stance::Crouch, true) => 7,
                    (Stance::Prone, false) => 8,
                    (Stance::Prone, true) => 9,
                }
            }
        }
        4 => match (player.stance, backward) {
            (Stance::Crouch, false) => 12,
            (Stance::Crouch, true) => 13,
            (_, false) => 10,
            (_, true) => 11,
        },
        5 => 18,
        6 => 19,
        7 => 20,
        8..=18 => (movetype + 13) as u8,
        19 => {
            if player.stance == Stance::Crouch {
                16
            } else {
                14
            }
        }
        20 => {
            if player.stance == Stance::Crouch {
                17
            } else {
                15
            }
        }
        21 => 55,
        22 => 56,
        23 => 57,
        24 => 58,
        25 => 59,
        26 => 60,
        _ => return None,
    })
}

fn apply_move_anim_signals(
    world: &mut FrameWorld,
    id: ClientId,
    player: &MovePlayer,
    cmd: &MoveCommand,
    signals: &[MoveSignal],
) {
    let Some(script) = world.player_anim_script() else {
        return;
    };
    let Some(ps) = world.player(id).copied() else {
        return;
    };
    let (view_w, primary) = crate::pmove_anim_weapon_ids(&ps);
    let view_facts = world.combat_facts_for(view_w);
    let primary_facts = world.combat_facts_for(primary);
    let strafing = crate::player_anim_script::anim_strafing(&ps, cmd.forwardmove, cmd.rightmove);
    let mut current_movetype =
        crate::player_anim_script::event_anim_movetype(&ps, world.last_anim_movetype(id));
    let mut force_legs = false;
    for signal in signals {
        match *signal {
            MoveSignal::Anim { event: -1 } => {
                if let Some(ps) = world.player_mut(id) {
                    crate::player_anim_script::reset_stance_torso(ps);
                }
            }
            MoveSignal::Anim { event } => {
                let Some(et) = map_t6_anim_event(event) else {
                    continue;
                };
                let mut seed = world.anim_event_seed();
                let Some(ps_now) = world.player(id).copied() else {
                    continue;
                };
                let conds = crate::anim_conditions_from_pmove(
                    &ps_now,
                    view_facts,
                    primary_facts,
                    Some(current_movetype),
                    strafing,
                    cmd.buttons,
                );
                if let Some(ps_mut) = world.player_mut(id) {
                    let force = matches!(et, 1 | 3 | 5 | 11..=16 | 26..=37);
                    let _ = script.apply_event(ps_mut, et, &conds, &mut seed, force);
                }
            }
            MoveSignal::LegsMove { movetype } => {
                if (8..=18).contains(&movetype) {
                    force_legs = true;
                }
                let Some(mt) = map_t6_legs_movetype(player, cmd, movetype) else {
                    continue;
                };
                current_movetype = mt;
                let Some(ps_now) = world.player(id).copied() else {
                    continue;
                };
                let conds = crate::anim_conditions_from_pmove(
                    &ps_now,
                    view_facts,
                    primary_facts,
                    Some(mt),
                    strafing,
                    cmd.buttons,
                );
                if let Some(ps_mut) = world.player_mut(id)
                    && let Some(selected) = script.apply(ps_mut, mt, id.0, &conds, force_legs)
                {
                    world.set_anim_movement(id, selected, strafing);
                }
            }
            _ => {}
        }
    }
}

/// Runs a player animation script event (`event` in `ANIM_ET_NAMES`) and
/// returns its duration in milliseconds (`duration_ms + 50`).
pub(crate) fn play_anim_event(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    force: bool,
) -> Option<i32> {
    let script = world.player_anim_script()?;
    let mut seed = world.anim_event_seed();
    let ps = world.player(id).copied()?;
    let (view_w, primary) = crate::pmove_anim_weapon_ids(&ps);
    let view_facts = world.combat_facts_for(view_w);
    let primary_facts = world.combat_facts_for(primary);
    let movetype =
        crate::player_anim_script::event_anim_movetype(&ps, world.last_anim_movetype(id));
    let strafing = world.last_anim_strafing(id);
    let conds = crate::anim_conditions_from_pmove(
        &ps,
        view_facts,
        primary_facts,
        Some(movetype),
        strafing,
        world.anim_command_buttons(id),
    );
    let ps_mut = world.player_mut(id)?;
    script.apply_event_duration(ps_mut, event, &conds, &mut seed, force)
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
        let left = left_weapon(world, weapon);
        HeldWeapon {
            weapon,
            clip: crate::script_player::ammo_clip(world, id, weapon),
            stock: crate::script_player::ammo_stock(world, id, weapon),
            left_clip: if left == 0 {
                0
            } else {
                crate::script_player::ammo_clip(world, id, left)
            },
        }
    })
}

/// A held weapon's dual-wield left-hand weapon, where the game keeps the
/// left hand's rounds by a weapon of its own; 0 for none.
pub(crate) fn left_weapon(world: &FrameWorld, weapon: u32) -> u32 {
    if !game_ammo_numbers(world) {
        return 0;
    }
    world
        .content()
        .game_weapon(weapon)
        .filter(|_| world.combat_facts_for(weapon).is_some_and(|f| f.dual_wield))
        .map_or(0, |w| w.dual_wield_weapon)
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
        let left = left_weapon(world, now.weapon);
        if left != 0 && now.left_clip != was.left_clip {
            crate::script_player::set_ammo_clip(world, id, left, now.left_clip);
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
        if let Some(ps) = world.player_mut(id) {
            for signal in &outcome.signals {
                if let Some(forced) = movement.forced_stance(signal) {
                    ps.push_stance_event(forced);
                }
            }
        }
        for gap in outcome.gaps {
            world.report_game_gap(gap);
        }
    }
    true
}

/// A script hides (`hideviewmodel`) or shows the player's viewmodel by the
/// game's own rules, sending the events they raise; false when the game has
/// no such rule.
pub(crate) fn hide_viewmodel(world: &mut FrameWorld, id: ClientId, hide: bool) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    let Some(before) = world
        .player(id)
        .map(|ps| ps.move_player(id.0 as i32, frozen(world, id)))
    else {
        return false;
    };
    let Some(Some((outcome, player))) = update(world, id, |player| {
        movement
            .hide_viewmodel(player, hide)
            .map(|outcome| (outcome, *player))
    }) else {
        return false;
    };
    let tick = world
        .ecs()
        .get_resource::<crate::step::StepRequest>()
        .map(|request| request.tick);
    if let Some(tick) = tick
        && world.publishes_snapshot()
        && !outcome.signals.is_empty()
    {
        let mut around = crate::game_fire::MatchFireWorld::new(world, tick, Some(id));
        for gap in movement.server_events(&before, &player, &outcome.signals, &mut around) {
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
    if let Some((outcome, player)) = update(world, id, |player| {
        let outcome = movement.enter_last_stand(player, &*content);
        (outcome, *player)
    }) {
        apply_move_anim_signals(
            world,
            id,
            &player,
            &MoveCommand::default(),
            &outcome.signals,
        );
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
    if let Some((outcome, player)) = update(world, id, |player| {
        let outcome = movement.revive(player);
        (outcome, *player)
    }) {
        apply_move_anim_signals(
            world,
            id,
            &player,
            &MoveCommand::default(),
            &outcome.signals,
        );
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

/// Puts the player on a turret (its view clamp and stance) or takes them off
/// it through the game's own movement; false when it has no turrets.
pub(crate) fn mount_turret(
    world: &mut FrameWorld,
    id: ClientId,
    mount: Option<&game_api::turret::TurretMount>,
) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.mount_turret(player, mount)).unwrap_or(false)
}

/// The player as a turret's use and think read them: their entity number,
/// view, stance, frozen controls, weapon state and whether they hold
/// attack.
pub(crate) fn turret_user(
    world: &mut FrameWorld,
    id: ClientId,
    entity: i32,
) -> Option<game_api::turret::TurretUser> {
    let frozen = frozen(world, id);
    let player = world.player(id)?.move_player(id.0 as i32, frozen);
    let attack =
        crate::script_player::buttons(world, id) & crate::script::command_buttons("+attack") != 0;
    Some(game_api::turret::TurretUser {
        entity,
        view: player.viewangles,
        stance: player.stance,
        frozen,
        weapon_state: player.weapon_state,
        attack,
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

/// The player holds the perk at `bit` of the game movement's perk mask.
pub(crate) fn has_perk_bit(world: &mut FrameWorld, id: ClientId, bit: u32) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.has_perk_bit(player, bit)).unwrap_or(false)
}

/// The player is in water by the game movement's rules.
pub(crate) fn in_water(world: &mut FrameWorld, id: ClientId) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.in_water(player)).unwrap_or(false)
}

/// Sets every perk the game's own movement reads from the scripts' perk
/// bits; false when the match runs the simulation's.
pub(crate) fn set_perk_mask(world: &mut FrameWorld, id: ClientId, mask: u64) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| movement.set_perk_mask(player, mask)).unwrap_or(false)
}

/// The sprint length and recharge pause the scripts set; false when the
/// match runs the simulation's movement.
pub(crate) fn set_sprint_times(
    world: &mut FrameWorld,
    id: ClientId,
    duration: Option<i32>,
    cooldown: Option<i32>,
) -> bool {
    let Some(movement) = rules(world) else {
        return false;
    };
    update(world, id, |player| {
        movement.set_sprint_times(player, duration, cooldown)
    })
    .unwrap_or(false)
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
