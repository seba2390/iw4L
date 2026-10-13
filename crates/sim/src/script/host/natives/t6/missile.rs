//! Black Ops 2's engine notifies for a player's missiles: the fire
//! dispatch sends grenades and mines (and grenade-class projectiles) as a
//! `grenade` with `grenade_fire`, or `grenade_launcher_fire` when the
//! weapon arms after a distance, other projectiles as a `rocket` with
//! `missile_fire`; a grenade at rest says `stationary`; a missile gone off
//! says `explode` then `death` at once and is freed.

use crate::script::host::registry::{MissileLaunch, MissileRule};
use crate::script::runtime::raise;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeSet;

const WEAPTYPE_GRENADE: i32 = 1;
const WEAPTYPE_PROJECTILE: i32 = 2;
const WEAPTYPE_MINE: i32 = 6;
const WEAPCLASS_GRENADE: i32 = 5;

/// The missiles that have said they are at rest.
#[derive(Resource, Default)]
struct Resting(BTreeSet<u64>);

fn launched(world: &mut World, projectile: &crate::ProjectileState) -> Option<MissileLaunch> {
    let name = crate::script_player::weapon_name(
        &crate::frame::FrameWorld::from_world(world),
        projectile.weapon,
    );
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

fn flying(world: &mut World, object: u64, projectile: &crate::ProjectileState, stuck: bool) {
    let mut resting = std::mem::take(&mut world.get_resource_or_insert_with(Resting::default).0);
    {
        let runtime = world.resource::<RoundScript>();
        resting.retain(|id| runtime.entities.contains_key(id));
    }
    if stuck && resting.insert(object) {
        diag::warn!(
            Sim,
            "t6.grenades.stuck: a stuck grenade's stationary and grenade_stuck are not sent yet"
        );
    } else if projectile.resting() && resting.insert(object) {
        raise(world, Value::Object(object), "stationary", Vec::new());
    }
    world.resource_mut::<Resting>().0 = resting;
}

const RULE: MissileRule = MissileRule {
    launched,
    flying,
    exploded: |_, _, origin| vec![Value::Vector(origin)],
    linger_ms: None,
};

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.on_missiles(RULE);
}
