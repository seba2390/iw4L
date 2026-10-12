//! Black Ops 2's player builtins over IW4L's player state, which the owner
//! lends Black Ops 2 with its movement (`t6.movement.player`).

use super::super::player::player;
use super::{float, int, vector};
use crate::ClientId;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use crate::script_player::{CmdButton, holds_now_or_last};
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
    registry.register(Method, "issprinting", |world, receiver, _| {
        let id = ClientId(
            player(world, receiver)
                .map_err(|_| "IsSprinting can only be called on a player.".to_owned())?,
        );
        Ok(Value::Int(
            FrameWorld::from_world(world)
                .player(id)
                .is_some_and(|ps| {
                    ps.last_sprint_start != 0 && ps.last_sprint_end < ps.last_sprint_start
                })
                .into(),
        ))
    });
    registry.register(Method, "isonladder", |world, receiver, _| {
        pm_flag(world, receiver, LADDER)
    });
    registry.register(Method, "ismantling", |world, receiver, _| {
        pm_flag(world, receiver, MANTLING)
    });
    // Sprinting and aiming down the sights: the shared allowances, through
    // the game's own movement when it has one. Melee: the handler sets its
    // no-melee pm flag; IW4L takes the melee button from the lent
    // movement's commands instead (`t6.player.allowances`). Each answers the
    // previous allowance.
    registry.register(Method, "allowsprint", |world, receiver, args| {
        allowance(
            world,
            receiver,
            args,
            |a| &mut a.sprint,
            super::super::player::allow_sprint,
        )
    });
    registry.register(Method, "allowads", |world, receiver, args| {
        allowance(
            world,
            receiver,
            args,
            |a| &mut a.ads,
            super::super::player::allow_ads,
        )
    });
    registry.register(Method, "allowmelee", |world, receiver, args| {
        allowance(
            world,
            receiver,
            args,
            |a| &mut a.melee,
            |world, receiver, args| {
                let id = ClientId(player(world, receiver)?);
                let allowed = int(args, 0)? != 0;
                FrameWorld::from_world(world)
                    .client_meta_mut(id)
                    .controls
                    .melee_disabled = !allowed;
                Ok(Value::Undefined)
            },
        )
    });
    // Buttons held this frame or the last.
    macro_rules! button {
        ($($name:literal => $button:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let id = ClientId(player(world, receiver)?);
                Ok(Value::Int(
                    holds_now_or_last(&mut FrameWorld::from_world(world), id, CmdButton::$button)
                        .into(),
                ))
            });
        )*};
    }
    button!(
        "attackbuttonpressed" => Attack,
        "adsbuttonpressed" => Ads,
        "meleebuttonpressed" => Melee,
        "fragbuttonpressed" => Frag,
        "secondaryoffhandbuttonpressed" => SecondaryOffhand,
        "jumpbuttonpressed" => Jump,
        "sprintbuttonpressed" => Sprint,
        "stancebuttonpressed" => Stance,
        "throwbuttonpressed" => Throw,
    );
    registry.register(Method, "getvelocity", |world, receiver, _| {
        let id = ClientId(player(world, receiver).map_err(|_| {
            "t6.entities.velocity: the velocity of an entity that is not a player is not kept"
                .to_owned()
        })?);
        Ok(Value::Vector(
            FrameWorld::from_world(world)
                .player(id)
                .map_or([0.0; 3], |ps| ps.velocity),
        ))
    });
    registry.register(Method, "setvelocity", |world, receiver, args| {
        let id = ClientId(player(world, receiver)?);
        let velocity = vector(args, 0)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.velocity = velocity;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getplayerviewheight", |world, receiver, _| {
        let id = ClientId(player(world, receiver)?);
        Ok(Value::Float(
            FrameWorld::from_world(world)
                .player(id)
                .map_or(0.0, |ps| ps.view_height_current),
        ))
    });
    // The current offhand weapon's name, "none" without one.
    registry.register(Method, "getcurrentoffhand", |world, receiver, _| {
        let id = ClientId(player(world, receiver)?);
        let frame = FrameWorld::from_world(world);
        let offhand = frame.player(id).map_or(0, |ps| ps.off_hand_index & 0xff);
        Ok(Value::string(if offhand == 0 || offhand == 0xff {
            "none"
        } else {
            frame.weapon_script_name(offhand as u32)
        }))
    });
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

/// What a script has taken from a player's body, kept so each allowance
/// can answer the one before it.
#[derive(Clone, Copy, Debug, Default)]
struct Taken {
    sprint: bool,
    ads: bool,
    melee: bool,
}

#[derive(Resource, Default)]
struct Allowances(BTreeMap<u32, Taken>);

/// `allowsprint`/`allowads`/`allowmelee`: applies the allowance and answers
/// the previous one.
fn allowance(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    slot: fn(&mut Taken) -> &mut bool,
    apply: crate::script::Native,
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let allowed = int(args, 0)? != 0;
    apply(world, receiver, args)?;
    let taken = slot(
        world
            .get_resource_or_insert_with(Allowances::default)
            .into_inner()
            .0
            .entry(client)
            .or_default(),
    );
    let was = !*taken;
    *taken = !allowed;
    Ok(Value::Int(was.into()))
}

/// The player-state flag bits the engine reads for these queries.
const MANTLING: u32 = 2;
const LADDER: u32 = 3;

fn pm_flag(world: &mut World, receiver: &Value, bit: u32) -> Result<Value, String> {
    let id = ClientId(
        player(world, receiver)
            .map_err(|_| "setstat: entity must be a player entity".to_owned())?,
    );
    Ok(Value::Int(
        FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| (ps.pm_flags >> bit & 1) as i32),
    ))
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
