//! Black Ops 2's builtins only its multiplayer scripts reach.

use super::{float, int, string, vector};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The module only a multiplayer program links: the scripts run a
/// multiplayer match, not a zombies one.
const GLOBAL_LOGIC_INIT: &str = "maps/mp/gametypes/_globallogic::init";

/// Whether the server's scripts are Black Ops 2's multiplayer ones.
pub(super) fn is_multiplayer(world: &World) -> bool {
    world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_some_and(|program| program.names.contains_key(GLOBAL_LOGIC_INIT))
}

/// The session of a multiplayer match here: an online private match
/// (`docs/fidelity/t6-mp.md`, `t6.session.private`).
pub(super) const SESSION_FLAGS: u32 = 0xc;
/// `level.gamemode_private_match`, the only game mode a match here has.
pub(super) const GAME_MODE_FLAGS: u32 = 1 << 1;

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // The table's own name stands for its id: the lookups take either.
    registry.register(Function, "tablelookupfindcoreasset", |_, _, args| {
        Ok(Value::string(&string(args, 0)?))
    });
    registry.register(Function, "tablelookupistring", |world, _, args| {
        Ok(Value::LocalizedString(
            super::table_lookup(world, args)?.into(),
        ))
    });
    registry.register(Function, "istring", |_, _, args| {
        Ok(Value::LocalizedString(string(args, 0)?.into()))
    });
    // Milliseconds as seconds, nothing for a weapon the zones lack.
    registry.register(Function, "weaponfiretime", |world, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Float(
            world
                .resource::<RoundScript>()
                .restart
                .as_ref()
                .and_then(|plan| plan.weapons.get(&name.to_ascii_lowercase()))
                .map_or(0.0, |weapon| weapon.fire_time as f32 * 0.001),
        ))
    });
    // The vehicle definitions' tread effects are not read
    // (t6.vehicles.tread_fx): there are none to load.
    registry.register(Function, "getvehicletreadfxarray", |world, _, args| {
        string(args, 0)?;
        crate::script::host::arrays::new_array(world, Vec::new())
    });
    // The rank and prestige a player's state carries, which no client reads
    // here yet (t6.mp.rank_state).
    registry.register(Method, "setrank", |_, _, args| {
        for index in 0..args.len().min(2) {
            let value = int(args, index)?;
            if !(0..0x100).contains(&value) {
                return Err(format!("setrank: {value} is out of range"));
            }
        }
        Ok(Value::Undefined)
    });
    // Whether the map's GPS jammers and spawn protection hide a player on
    // the enemy's radar, which nothing draws yet (t6.mp.radar_flags).
    for name in [
        "gpsjammeractive",
        "gpsjammerinactive",
        "spawnprotectionactive",
        "spawnprotectioninactive",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    // The looping sound `playlocalsound` started, for the local client.
    registry.register(Method, "stoplocalsound", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setplayerrenderoptions", |_, _, args| {
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    // The first raise of a spawn's weapon, which IW4L's weapon state does not
    // tell apart (t6.mp.first_raise).
    registry.register(Method, "initialweaponraise", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    // Whether a weapon replaces the spawn weapon: the alternate of the other,
    // which no class weapon is without its attachments (t6.weapons.attachments).
    registry.register(Function, "doesweaponreplacespawnweapon", |_, _, args| {
        string(args, 0)?;
        string(args, 1)?;
        Ok(Value::Int(0))
    });
    // The stowed weapon and the current one's alternate, by the same gap.
    registry.register(Method, "getstowedweapon", |_, _, _| {
        Ok(Value::string("none"))
    });
    registry.register(Method, "getcurrentweaponaltweapon", |_, _, _| {
        Ok(Value::string("none"))
    });
    // Telemetry and progress a private match does not keep.
    for name in [
        "seteverhadweaponall",
        "recordloadoutperksandkillstreaks",
        "bbclasschoice",
        "refreshshieldattachment",
        "addrankxp",
        "addrankxpvalue",
        "recordmultikill",
        "recordkillstreakevent",
        "recordkillstreakendevent",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    registry.register(Function, "bbclasschoice", |_, _, _| Ok(Value::Undefined));
    // The missile lock's state on the player, which wait for lock-on weapons
    // (t6.weapons.lock_on).
    for name in [
        "weaponlockfree",
        "weaponlocknoclearance",
        "weaponlocktargettooclose",
        "weaponlockfinalize",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    registry.register(Method, "setweaponheatpercent", |_, _, args| {
        string(args, 0)?;
        float(args, 1)?;
        Ok(Value::Undefined)
    });
    // A shot for the match's accuracy stats, which a private match keeps none of.
    registry.register(Method, "trackweaponfirenative", |_, _, _| {
        Ok(Value::Undefined)
    });
    // The entities that take damage, within a radius in the plane of a point
    // when one is given.
    registry.register(Function, "getdamageableentarray", |world, _, args| {
        let center = if args.is_empty() {
            None
        } else {
            Some((vector(args, 0)?, float(args, 1)?))
        };
        let candidates: Vec<u64> = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let ids: Vec<u64> = runtime.entities.keys().copied().collect();
            ids.into_iter()
                .filter(|id| {
                    runtime.player_client(*id).is_some()
                        || matches!(runtime.object_field(*id, "takedamage"), Value::Int(1..))
                })
                .collect()
        };
        let mut found = Vec::new();
        for id in candidates {
            let inside = center.is_none_or(|(point, radius)| {
                let origin = super::movement::origin_of(world, id);
                let (x, y) = (point[0] - origin[0], point[1] - origin[1]);
                x * x + y * y <= radius * radius
            });
            if inside {
                found.push(Value::Object(id));
            }
        }
        crate::script::host::arrays::new_array(world, found)
    });
    // The after-action report's rival, league team and film, which a private
    // match keeps none of.
    registry.register(Method, "setnemesisxuid", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "getleagueteamid", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getdemofileid", |_, _, _| Ok(Value::Int(0)));
    // The team's overhead icon, which nothing draws yet (#697).
    registry.register(Function, "precacheheadicon", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    // The entities in a box around a point: players by their bounds, others
    // by their origin (t6.mp.touching_volume).
    registry.register(Function, "gettouchingvolume", |world, _, args| {
        let origin = vector(args, 0)?;
        let mins = vector(args, 1)?;
        let maxs = vector(args, 2)?;
        let lo: [f32; 3] = std::array::from_fn(|i| origin[i] + mins[i]);
        let hi: [f32; 3] = std::array::from_fn(|i| origin[i] + maxs[i]);
        let ids: Vec<u64> = world
            .resource::<RoundScript>()
            .entities
            .keys()
            .copied()
            .collect();
        let mut found = Vec::new();
        for id in ids {
            let client = world.resource::<RoundScript>().player_client(id);
            let at = super::movement::origin_of(world, id);
            let (low, high) = match client {
                Some(_) => (
                    [at[0] - 15.0, at[1] - 15.0, at[2]],
                    [at[0] + 15.0, at[1] + 15.0, at[2] + 70.0],
                ),
                None => (at, at),
            };
            if (0..3).all(|i| low[i] <= hi[i] && high[i] >= lo[i]) {
                found.push(Value::Object(id));
            }
        }
        crate::script::host::arrays::new_array(world, found)
    });
    // An entity's flags for what bots may target and for skipping costly
    // updates, which nothing reads yet (t6.mp.entity_flags).
    registry.register(Method, "allowbottargetting", |_, _, args| {
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setcheapflag", |_, _, args| {
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    // An angle into 0 to 360.
    registry.register(Function, "absangleclamp360", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.rem_euclid(360.0)))
    });
    // The location selector's material, for a selection nothing shows yet
    // (t6.mp.location_selection).
    registry.register(Function, "precachelocationselector", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    // A killcam's field of view (t6.mp.killcam_fov).
    registry.register(Method, "setfovforkillcam", |_, _, args| {
        float(args, 0)?;
        Ok(Value::Undefined)
    });
    // The rank table's experience, which nothing reads in a private match.
    registry.register(Function, "registerxp", |_, _, _| Ok(Value::Undefined));
    // A league team's custom name; a private match has none.
    registry.register(Function, "getcustomteamname", |_, _, args| {
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    // Whether a public lobby's host advertises the game, which a private
    // match here never does.
    registry.register(Function, "changeadvertisedstatus", |_, _, args| {
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    // The pregame lobby's match, which private matches do not run.
    registry.register(Function, "ispregamegamestarted", |_, _, _| {
        Ok(Value::Int(0))
    });
    // A solid script model carrying the collision model; it answers nothing.
    registry.register(Function, "spawncollision", |world, _, args| {
        let model = string(args, 0)?;
        let targetname = string(args, 1)?;
        let origin = vector(args, 2)?;
        let angles = vector(args, 3)?;
        let entity = super::super::engine::spawn_entity(
            world,
            &[Value::string("script_model"), Value::Vector(origin)],
        )?;
        let id = super::super::engine::entity_id(world, &entity)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "targetname", Value::string(&targetname));
        runtime.set_object_field(id, "model", Value::string(&model));
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.solid = true;
        }
        Ok(Value::Undefined)
    });
    // The map's helicopter height patches, which wait for the helicopter
    // runtime (t6.heli.height_patches).
    registry.register(Function, "setheliheightpatchenabled", |_, _, args| {
        string(args, 0)?;
        int(args, 1)?;
        Ok(Value::Undefined)
    });
    // Only `addtestclient` adds a test client, which waits for the bot
    // builtins (t6.bots.test_clients).
    registry.register(Method, "istestclient", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "istestclient", |_, _, _| Ok(Value::Int(0)));
}
