//! Black Ops 2's engine notifies for a player's missiles: the fire
//! dispatch sends grenades and mines (and grenade-class projectiles) as a
//! `grenade` with `grenade_fire`, or `grenade_launcher_fire` when the
//! weapon arms after a distance, other projectiles as a `rocket` with
//! `missile_fire`; a grenade at rest says `stationary`; a missile gone off
//! says `explode` then `death` at once and is freed.

use super::super::super::entities::EntityKind;
use super::{float, string, vector};
use crate::frame::FrameWorld;
use crate::script::host::registry::{MissileLaunch, MissileRule};
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

const WEAPTYPE_GRENADE: i32 = 1;
const WEAPTYPE_PROJECTILE: i32 = 2;
const WEAPTYPE_MINE: i32 = 6;
const WEAPCLASS_GRENADE: i32 = 5;
const DEFAULT_MAGIC_FUSE_MS: i32 = 5000;

fn launched(world: &mut World, projectile: &crate::ProjectileState) -> Option<MissileLaunch> {
    let name = crate::script_player::weapon_name(&FrameWorld::from_world(world), projectile.weapon);
    let Some(def) = super::weapon::zone_weapon(world, &name) else {
        diag::warn!(
            Sim,
            "missile of '{name}': the weapon is not in the zones; no fire notify"
        );
        return None;
    };
    let grenade = matches!(def.weap_type, WEAPTYPE_GRENADE | WEAPTYPE_MINE)
        || (def.weap_type == WEAPTYPE_PROJECTILE && def.weap_class == WEAPCLASS_GRENADE);
    if grenade {
        return Some(MissileLaunch {
            classname: "grenade",
            notify: if def.projectile_activate_dist == 0 {
                "grenade_fire"
            } else {
                "grenade_launcher_fire"
            },
            extra: Vec::new(),
        });
    }
    if def.weap_type != WEAPTYPE_PROJECTILE {
        return None;
    }
    if projectile.guide.target.is_some() {
        diag::warn!(
            Sim,
            "t6.grenades.missile_target: '{name}' locked a target, which missile_fire does not pass yet"
        );
    }
    Some(MissileLaunch {
        classname: "rocket",
        notify: "missile_fire",
        extra: vec![Value::Undefined],
    })
}

const RULE: MissileRule = MissileRule {
    launched,
    flying: |_, _, _, _| {},
    exploded: |_, _, origin| vec![Value::Vector(origin)],
    linger_ms: None,
};

fn missile_entity(world: &World, receiver: &Value, err: &str) -> Result<(u64, i32), String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    let entity = &world.resource::<RoundScript>().entities[&id];
    match entity.kind {
        EntityKind::Missile(_) => Ok((id, entity.number)),
        _ => Err(err.into()),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.on_missiles(RULE);

    // gscm_detonate([attacker]): detonates a grenade or projectile missile.
    registry.register(Method, "detonate", |world, receiver, args| {
        let (id, number) =
            missile_entity(world, receiver, "entity is not a grenade or projectile")?;
        let (attacker, owner_obj) = match args.first() {
            None => (None, None),
            Some(Value::Undefined) => (Some(game_api::movement::ENTITY_WORLD), Some(None)),
            Some(value) => {
                let owner_id = super::super::engine::entity_id(world, value)?;
                let Some(client) = world.resource::<RoundScript>().player_client(owner_id) else {
                    return Err("Entity is not a player".into());
                };
                (Some(client as i32), Some(Some(owner_id)))
            }
        };
        if let Some(owner_obj) = owner_obj
            && let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id)
        {
            entity.missile_owner = owner_obj;
        }
        let tick = world.resource::<crate::step::StepRequest>().tick;
        crate::game_fire::detonate_game_missile(
            &mut FrameWorld::from_world(world),
            tick,
            number,
            attacker,
        );
        Ok(Value::Undefined)
    });

    // gscm_makegrenadedud: marks a grenade a dud so its fuse will not explode it.
    registry.register(Method, "makegrenadedud", |world, receiver, _| {
        let (_, number) = missile_entity(
            world,
            receiver,
            "MakeGrenadeDud: entity must be a grenade entity",
        )?;
        crate::game_fire::make_game_missile_dud(&mut FrameWorld::from_world(world), number);
        Ok(Value::Undefined)
    });

    // gscm_resetmissiledetonationtime([seconds]): resets a missile's fuse.
    registry.register(
        Method,
        "resetmissiledetonationtime",
        |world, receiver, args| {
            let id = super::super::engine::entity_id(world, receiver)?;
            let number = match world.resource::<RoundScript>().entities.get(&id) {
                Some(entity) if matches!(entity.kind, EntityKind::Missile(_)) => entity.number,
                _ => return Ok(Value::Undefined),
            };
            let after_ms = if !args.is_empty() {
                Some((float(args, 0)? * 1000.0) as i32)
            } else {
                None
            };
            let tick = world.resource::<crate::step::StepRequest>().tick;
            crate::game_fire::reset_game_missile_detonation_time(
                &mut FrameWorld::from_world(world),
                tick,
                number,
                after_ms,
            );
            Ok(Value::Undefined)
        },
    );

    // gscm_setmissileowner(owner): sets the missile's owner entity.
    registry.register(Method, "setmissileowner", |world, receiver, args| {
        let (id, number) =
            missile_entity(world, receiver, "SetMissileOwner called on non-missile ent")?;
        let owner_id = super::super::engine::entity_id(
            world,
            args.first().ok_or("SetMissileOwner expects an entity")?,
        )?;
        let client = world.resource::<RoundScript>().player_client(owner_id);
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.missile_owner = Some(owner_id);
        }
        let owner_num = client.map_or(game_api::movement::ENTITY_WORLD, |c| c as i32);
        crate::game_fire::set_game_missile_owner(
            &mut FrameWorld::from_world(world),
            number,
            owner_num,
        );
        Ok(Value::Undefined)
    });

    // gscm_forcegrenadethrow: forces a player holding a primed offhand to throw it.
    registry.register(Method, "forcegrenadethrow", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            let number = world.resource::<RoundScript>().entities[&id].number;
            return Err(format!("entity {number} is not a player"));
        };
        crate::script_player::force_grenade_throw(
            &mut FrameWorld::from_world(world),
            crate::ClientId(client),
        );
        Ok(Value::Undefined)
    });

    // gscm_magicgrenadetype(weapon, origin, velocity, [fuse_seconds]).
    registry.register(Method, "magicgrenadetype", |world, receiver, args| {
        let spawner_id = super::super::engine::entity_id(world, receiver)?;
        let client = world.resource::<RoundScript>().player_client(spawner_id);
        let name = string(args, 0)?;
        let Some(def) = super::weapon::zone_weapon(world, &name) else {
            return Err("MagicGrenadeManual invalid weapon type specified.\n".into());
        };
        if !matches!(def.weap_type, WEAPTYPE_GRENADE | WEAPTYPE_MINE) {
            return Err("MagicGrenadeManual invalid weapon type specified.\n".into());
        }
        let weapon = crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)
            .map_err(|_| String::from("MagicGrenadeManual invalid weapon type specified.\n"))?;
        let origin = vector(args, 1)?;
        let velocity = vector(args, 2)?;
        let fuse_ms = if args.len() == 4 {
            (float(args, 3)? * 1000.0) as i32
        } else {
            DEFAULT_MAGIC_FUSE_MS
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let owner_num = client.map_or(game_api::movement::ENTITY_WORLD, |c| c as i32);
        let projectile = crate::game_fire::spawn_game_grenade(
            &mut FrameWorld::from_world(world),
            owner_num,
            weapon,
            tick,
            origin,
            velocity,
            fuse_ms,
        )
        .ok_or("MagicGrenadeManual could not spawn grenade")?;
        let object = super::super::super::weapons::adopt(world, &projectile, "grenade")?;
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            if let Some(entity) = runtime.entities.get_mut(&object) {
                entity.missile_owner = Some(spawner_id);
            }
            let model = format!("{}{}", crate::WEAPON_MODEL_PREFIX, projectile.weapon);
            runtime.set_object_field(object, "nativename", Value::string(&def.name));
            runtime.set_object_field(object, "weaponname", Value::string(&def.name));
            runtime.set_object_field(object, "weaponmodel", Value::string(&model));
        }
        let notify = if def.projectile_activate_dist == 0 {
            "grenade_fire"
        } else {
            "grenade_launcher_fire"
        };
        raise(
            world,
            receiver.clone(),
            notify,
            vec![Value::Object(object), Value::string(&def.name)],
        );
        Ok(Value::Object(object))
    });
}
