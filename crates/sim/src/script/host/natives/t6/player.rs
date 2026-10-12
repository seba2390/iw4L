//! Black Ops 2's player builtins over IW4L's player state, which the owner
//! lends Black Ops 2 with its movement (`t6.movement.player`).

use super::super::player::player;
use super::{float, int};
use crate::ClientId;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The stances a script has refused a player, kept for the movement that
/// will honour them.
#[derive(Clone, Copy, Debug, Default)]
struct Refused {
    stand: bool,
    crouch: bool,
    prone: bool,
}

#[derive(Resource, Default)]
struct Stances(BTreeMap<u32, Refused>);

fn refused(world: &mut World, client: u32) -> &mut Refused {
    world
        .get_resource_or_insert_with(Stances::default)
        .into_inner()
        .0
        .entry(client)
        .or_default()
}

/// `allowstand`/`allowcrouch`/`allowprone`: the previous allowance.
fn allow_stance(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    stance: fn(&mut Refused) -> &mut bool,
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let allowed = int(args, 0)? != 0;
    let slot = stance(refused(world, client));
    let was = !*slot;
    *slot = !allowed;
    Ok(Value::Int(was.into()))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    // Health is the maximum's share, rounded; a share that leaves none is
    // refused, as the engine prints and does.
    registry.register(Method, "setnormalhealth", |world, receiver, args| {
        let id = ClientId(player(world, receiver)?);
        let share = float(args, 0)?.min(1.0);
        let mut frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player_mut(id) else {
            return Ok(Value::Undefined);
        };
        let health = (ps.max_health as f32 * share + 9.313_226e-10).round_ties_even() as i32;
        if health > 0 {
            ps.health = health;
        } else {
            diag::warn!(Sim, "ERROR: Cannot setnormalhealth to 0 or below.");
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getstance", |world, receiver, _| {
        let id = ClientId(
            player(world, receiver)
                .map_err(|_| "GetStance is only defined for players.".to_owned())?,
        );
        Ok(Value::string(crate::script_player::stance(
            &FrameWorld::from_world(world),
            id,
        )))
    });
    registry.register(Method, "allowjump", |world, receiver, args| {
        let id = ClientId(player(world, receiver)?);
        let allowed = int(args, 0)? != 0;
        let mut frame = FrameWorld::from_world(world);
        let controls = &mut frame.client_meta_mut(id).controls;
        let was = !controls.jump_disabled;
        controls.jump_disabled = !allowed;
        Ok(Value::Int(was.into()))
    });
    registry.register(Method, "allowstand", |world, receiver, args| {
        allow_stance(world, receiver, args, |r| &mut r.stand)
    });
    registry.register(Method, "allowcrouch", |world, receiver, args| {
        allow_stance(world, receiver, args, |r| &mut r.crouch)
    });
    registry.register(Method, "allowprone", |world, receiver, args| {
        allow_stance(world, receiver, args, |r| &mut r.prone)
    });
    for (name, on) in [
        ("enableinvulnerability", true),
        ("disableinvulnerability", false),
    ] {
        registry.register(Method, name, if on { invulnerable } else { vulnerable });
    }
    registry.register(Method, "startfadingblur", |world, receiver, args| {
        let blur = float(args, 0)?;
        let seconds = float(args, 1)?;
        if seconds < 0.0 {
            return Err("Time must be positive".into());
        }
        if blur < 0.0 {
            return Err("Blur value must be greater than 0".into());
        }
        player(world, receiver)
            .map_err(|_| "setblur() called on an invalid client entity.\n".to_owned())?;
        Err(format!(
            "t6.player.blur: the client's blur to {blur} over {seconds}s is not drawn"
        ))
    });
}

fn invulnerable(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    set_invulnerable(world, receiver, true)
}

fn vulnerable(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    set_invulnerable(world, receiver, false)
}

/// The client's god flag, which the engine's player damage honours.
fn set_invulnerable(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let client = player(world, receiver)?;
    if let Some(slot) = world.resource_mut::<RoundScript>().players.get_mut(&client) {
        slot.invulnerable = on;
    }
    Ok(Value::Undefined)
}

/// Whether a script made the player invulnerable.
pub(super) fn is_invulnerable(world: &World, client: u32) -> bool {
    world
        .resource::<RoundScript>()
        .players
        .get(&client)
        .is_some_and(|slot| slot.invulnerable)
}
