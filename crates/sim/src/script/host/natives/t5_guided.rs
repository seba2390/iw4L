//! Black Ops multiplayer's guided missile camera as BlackOpsMP.exe's handlers
//! keep it (the Valkyrie Rockets): a remote-guided missile names itself to its
//! owner as it leaves, `linkguidedmissilecamera` puts the owner's view on it
//! and `unlinkguidedmissilecamera` takes it back. `detonate` explodes a
//! grenade or a projectile where it is, and `ismissileinsideheightlock` tests
//! a missile against the map's `heli_height_lock` volumes.

use super::super::entities::EntityKind;
use super::engine::entity_id;
use crate::frame::FrameWorld;
use crate::script::Namespace::Method;
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};
use game_api::movement::ScriptFlag;
use std::collections::BTreeMap;

/// The classname of the volumes that bound a guided missile.
const HEIGHT_LOCK_CLASS: &str = "heli_height_lock";

/// The missile each client's camera would follow.
#[derive(Resource, Default)]
struct GuidedMissiles(BTreeMap<u32, i32>);

pub(crate) fn set_guided_missile(world: &mut World, client: ClientId, missile: Option<i32>) {
    let mut guided = world.get_resource_or_insert_with(GuidedMissiles::default);
    match missile {
        Some(missile) => {
            guided.0.insert(client.0, missile);
        }
        None => {
            guided.0.remove(&client.0);
        }
    }
}

/// What the client's latest command says to the missile it steers.
pub(crate) fn guided_input(world: &mut FrameWorld, id: ClientId) -> game_api::fire::GuidedInput {
    let held = crate::script_player::buttons(world, id);
    let axes = world
        .ecs()
        .resource::<crate::step::StepRequest>()
        .input
        .cmds
        .iter()
        .rev()
        .find(|command| command.client == id)
        .map_or([0; 2], |command| command.command.remote_control);
    game_api::fire::GuidedInput {
        pitch: axes[0] as i8,
        yaw: axes[1] as i8,
        attack: held & crate::script::command_buttons("+attack") != 0,
        boost: held & crate::script::command_buttons("+smoke") != 0,
    }
}

fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    let id = entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })
}

fn set_camera_flag(world: &mut World, client: u32, on: bool) {
    let mut frame = FrameWorld::from_world(world);
    crate::game_move::set_script_flag(&mut frame, ClientId(client), ScriptFlag::GuidedCamera, on);
}

fn link_camera(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver)?;
    set_camera_flag(world, client, true);
    let missile = world
        .get_resource::<GuidedMissiles>()
        .and_then(|guided| guided.0.get(&client).copied());
    let mut frame = FrameWorld::from_world(world);
    if let Some(number) = missile
        && let Some(projectile) = frame.projectile_by_number(number)
        && frame.client_meta(ClientId(client)).is_some()
    {
        frame.client_meta_mut(ClientId(client)).remote_missile = Some(crate::RemoteMissile {
            projectile: projectile.id,
            entnum: number,
            angles: projectile.apos.tr_base,
            guided: true,
            ..Default::default()
        });
    }
    Ok(Value::Undefined)
}

fn unlink_camera(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver)?;
    set_camera_flag(world, client, false);
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_some() {
        frame.client_meta_mut(ClientId(client)).remote_missile = None;
    }
    Ok(Value::Undefined)
}

/// Inside any height lock volume across the ground; its height is not tested.
fn inside_height_lock(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    let id = entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    let at = runtime
        .entities
        .get(&id)
        .and_then(|entity| match entity.kind {
            EntityKind::Missile(_) => Some(entity.number),
            _ => None,
        });
    let origin = match at {
        Some(number) => FrameWorld::from_world(world)
            .projectile_by_number(number)
            .map(|projectile| projectile.origin),
        None => Some(super::engine::vector_field(world, id, "origin")),
    }
    .unwrap_or([0.0; 3]);
    let volumes: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| &*entity.classname == HEIGHT_LOCK_CLASS)
        .map(|(volume, _)| *volume)
        .collect();
    for volume in volumes {
        let (mins, maxs) = super::super::triggers::entity_bounds(world, volume);
        let place = super::engine::vector_field(world, volume, "origin");
        if mins[0] + place[0] <= origin[0]
            && origin[0] <= maxs[0] + place[0]
            && mins[1] + place[1] <= origin[1]
            && origin[1] <= maxs[1] + place[1]
        {
            return Ok(Value::Int(1));
        }
    }
    Ok(Value::Int(0))
}

/// `detonate([attacker])`: the missile explodes now, its blast credited to
/// the attacker when one is named.
fn detonate(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let number = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .and_then(|entity| match entity.kind {
            EntityKind::Missile(_) => Some(entity.number),
            _ => None,
        })
        .ok_or("entity is not a grenade or projectile")?;
    if let Some(attacker) = args
        .first()
        .filter(|value| !matches!(value, Value::Undefined))
    {
        let attacker = world
            .resource::<RoundScript>()
            .player_client_of(attacker)
            .ok_or("Entity is not a player")?;
        crate::game_fire::set_game_missile_owner(
            &mut FrameWorld::from_world(world),
            number,
            attacker as i32,
        );
    }
    let tick = world.resource::<crate::step::StepRequest>().tick;
    crate::game_fire::detonate_moved_missile(&mut FrameWorld::from_world(world), tick, number);
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "linkguidedmissilecamera", link_camera);
    registry.register(Method, "unlinkguidedmissilecamera", unlink_camera);
    registry.register(Method, "ismissileinsideheightlock", inside_height_lock);
    registry.register(Method, "detonate", detonate);
}
