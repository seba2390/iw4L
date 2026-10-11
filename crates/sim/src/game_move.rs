//! A game's own player movement on the runtime's player: the shared fields
//! come from the player state and go back to it, what the game keeps for
//! itself rides in the player state's game bytes.

use crate::frame::FrameWorld;
use crate::world::ClientId;
use game_api::movement::{
    GameWeapon, MoveCommand, MoveContext, MoveOutcome, MovePlayer, MoveRestriction, MoveTrace,
    MoveWorld, PlayerMovement, Stance, WaterSurface,
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

fn frozen(world: &FrameWorld, id: ClientId) -> bool {
    world.client_meta(id).is_some_and(|m| m.controls.frozen)
}

/// The match around the moving player: the runtime's collision, its players
/// and the game's weapon fields.
struct MatchMoveWorld<'a> {
    world: &'a FrameWorld<'a>,
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

    /// The runtime has no water query yet.
    fn water_surface(&self, _origin: [f32; 3], _up: f32, _down: f32) -> WaterSurface {
        WaterSurface::Unknown
    }
}

/// Runs one command with the game's movement and writes the player back.
pub(crate) fn run(
    world: &mut FrameWorld,
    movement: &'static dyn PlayerMovement,
    id: ClientId,
    cmd: MoveCommand,
    trace: &MoveTraceFn<'_>,
) -> MoveOutcome {
    let Some(ps) = world.player(id).copied() else {
        return MoveOutcome::default();
    };
    let mut player = ps.move_player(id.0 as i32, frozen(world, id));
    if world.publishes_snapshot() {
        let runtime = world.ecs().get_resource::<crate::script::MatchScript>();
        movement.think(&mut player, &|name| {
            runtime.and_then(|runtime| runtime.dvars.get(name).cloned())
        });
    }
    let context = MoveContext {
        predicting: !world.publishes_snapshot(),
    };
    let outcome = {
        let around = MatchMoveWorld {
            world: &*world,
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
    outcome
}

/// Changes a spawned player through the game's own movement.
fn update<T>(
    world: &mut FrameWorld,
    id: ClientId,
    change: impl FnOnce(&mut MovePlayer) -> T,
) -> Option<T> {
    let frozen = frozen(world, id);
    let ps = world.player_mut(id)?;
    let mut player = ps.move_player(id.0 as i32, frozen);
    let out = change(&mut player);
    ps.store_move_player(&player);
    Some(out)
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
