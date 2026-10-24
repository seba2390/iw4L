use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    super::t5_vision::register(registry);
    registry.register(Function, "getplayers", |world, _, args| {
        let team = optional_team(args)?;
        let players = players(world)
            .into_iter()
            .filter(|&id| team.as_deref().is_none_or(|team| on_team(world, id, team)))
            .map(Value::Object)
            .collect();
        new_array(world, players)
    });
    // `GetAIArray([team, ...])`: living actors on any of the teams (all when
    // none is named), dogs and actors whose death is held off left out.
    registry.register(Function, "getaiarray", |world, _, args| {
        let mut mask = 0;
        for index in 0..args.len() {
            mask |= team_mask(&super::super::args::string(args, index)?)?;
        }
        if mask == 0 {
            mask = ALL_TEAMS;
        }
        let actors = ai_array(world, mask, |species| {
            !matches!(species, "dog" | "zombie_dog")
        })
        .into_iter()
        .map(Value::Object)
        .collect();
        new_array(world, actors)
    });
    // `GetAISpeciesArray([team], [species])`: living actors on the team (or
    // all) of the species (or all), actors whose death is held off left out.
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let mask = match args.first() {
            Some(_) => team_mask(&super::super::args::string(args, 0)?)?,
            None => ALL_TEAMS,
        };
        let species = match args.get(1) {
            Some(_) => species_named(&super::super::args::string(args, 1)?)?,
            None => None,
        };
        let actors = ai_array(world, mask, |own| {
            species.is_none_or(|wanted| wanted == own)
        })
        .into_iter()
        .map(Value::Object)
        .collect();
        new_array(world, actors)
    });
    registry.register(Function, "issentient", |world, _, args| {
        let sentient = match world
            .resource::<RoundScript>()
            .entity(super::super::args::arg(args, 0)?)
        {
            Some((_, entity)) => matches!(entity.kind, EntityKind::Player | EntityKind::Actor),
            None => false,
        };
        Ok(Value::Int(sentient.into()))
    });
    registry.register(Function, "getspawnerarray", |world, _, args| {
        no_args(args)?;
        let spawners = spawners(world).into_iter().map(Value::Object).collect();
        new_array(world, spawners)
    });
    registry.register(Function, "codespawn", |world, _, args| {
        super::engine::spawn_entity(world, args)
    });
    registry.register(Method, "iswaitingonsound", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let waiting =
            world
                .resource::<RoundScript>()
                .timed_notifies
                .iter()
                .any(|(_, target, _, args)| {
                    matches!(target, Value::Object(o) if *o == id) && args.is_empty()
                });
        Ok(Value::Int(waiting.into()))
    });
    // Script pacing on snapshot acknowledgement is left to the network layer:
    // scripts see no remote clients and wait a tenth of a second instead.
    // Offline (neither an online nor a system link game) there are none;
    // otherwise the party's other members, which the session counts as its
    // other players.
    registry.register(Function, "numremoteclients", |world, _, args| {
        no_args(args)?;
        let online = dvar_set(world, "onlinegame") || dvar_set(world, "systemlink");
        let remote = if online {
            (players(world).len() as i32).max(1) - 1
        } else {
            0
        };
        Ok(Value::Int(remote))
    });
    // `g_gameskill` names the difficulty (registered 1, "medium").
    registry.register(Function, "getdifficulty", |world, _, args| {
        no_args(args)?;
        let skill = world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get("g_gameskill")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(1);
        Ok(Value::string(
            ["easy", "medium", "hard", "fu"]
                .get(skill)
                .copied()
                .unwrap_or("medium"),
        ))
    });
    registry.register(Function, "issaverecentlyloaded", |world, _, args| {
        no_args(args)?;
        Ok(Value::Int(i32::from(crate::save_game::recently_loaded(
            world,
        ))))
    });
    // The host keeps no AI budget, water simulation or exploder ids of its own.
    // `SetAILimit(n)`: actor spawns fail while n actors are in use.
    registry.register(Function, "setailimit", |world, _, args| {
        let limit = super::super::args::int(args, 0)?;
        if !(0..=32).contains(&limit) {
            return Err("SetAILimit must take a value between 0 and 32 inclusive.".into());
        }
        world.resource_mut::<RoundScript>().ai_limit = limit;
        Ok(Value::Undefined)
    });
    registry.register(Function, "resetailimit", |world, _, _| {
        world.resource_mut::<RoundScript>().ai_limit = 0;
        Ok(Value::Undefined)
    });
    registry.register(Function, "watersimenable", |world, _, args| {
        if args.len() != 1 {
            return Err("watersimenable() called with wrong params.\n".into());
        }
        let enabled = super::super::args::int(args, 0)? != 0;
        world
            .resource_mut::<crate::script::MatchScript>()
            .dvars
            .insert(
                "r_watersim_enabled".into(),
                if enabled { "1" } else { "0" }.into(),
            );
        Ok(Value::Undefined)
    });
    // `CleanupSpawnedDynEnts()`: removes dynamic entities spawned
    // via `CreateDynEntAndLaunch` on every client.
    registry.register(Function, "cleanupspawneddynents", |world, _, _| {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.t5.dyn_ent_cleanup_serial = runtime.t5.dyn_ent_cleanup_serial.wrapping_add(1);
        runtime.t5.spawned_dyn_ents.clear();
        Ok(Value::Undefined)
    });
    // `GetCorpseArray()`: the active actor corpses in slot order.
    registry.register(Function, "getcorpsearray", |world, _, args| {
        if !args.is_empty() {
            return Err("cannot call getcorpsearray with parameters".into());
        }
        let corpses: Vec<Value> = {
            let runtime = world.resource::<RoundScript>();
            runtime
                .t5
                .corpse_slots
                .iter()
                .copied()
                .filter(|id| {
                    runtime
                        .entities
                        .get(id)
                        .is_some_and(|e| e.kind == EntityKind::ActorCorpse)
                })
                .map(Value::Object)
                .collect()
        };
        new_array(world, corpses)
    });
    // `RefreshHudAmmoCounter()`: Black Ops tells every client to redraw
    // its ammo counter; the runtime's HUD reads the ammo each frame.
    registry.register(Function, "refreshhudammocounter", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Method, "setexploderid", |world, receiver, args| {
        let _ = super::engine::entity_id(world, receiver)?;
        let _ = super::super::args::int(args, 0)?;
        Ok(Value::Undefined)
    });
    // Script-state tracing is a developer aid; `haseyes` sets the entity's
    // eye-glow flag in the snapshot.
    registry.register(Method, "trackscriptstate", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "haseyes", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if args.len() != 1 {
            return Err("HasEyes() called with wrong params.\n".into());
        }
        if super::super::args::int(args, 0)? != 0 {
            world.resource_mut::<RoundScript>().t5.has_eyes.insert(id);
        } else {
            world.resource_mut::<RoundScript>().t5.has_eyes.remove(&id);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "isnotarget", |world, receiver, _| {
        let id = match receiver {
            Value::Object(id) => *id,
            _ => return Err("receiver is not an entity".into()),
        };
        let notarget = world
            .resource_mut::<RoundScript>()
            .object_field(id, "notarget");
        Ok(Value::Int(i32::from(
            matches!(notarget, Value::Int(n) if n != 0),
        )))
    });
    registry.register(Function, "getnumconnectedplayers", |world, _, args| {
        no_args(args)?;
        Ok(Value::Int(players(world).len() as i32))
    });
    // Scripts wait while fewer than the expected players are connected: the
    // host's lobby members, or whoever is connected outside a lobby.
    registry.register(Function, "getnumexpectedplayers", |world, _, args| {
        no_args(args)?;
        let expected = world.resource::<RoundScript>().expected_players;
        Ok(Value::Int(players(world).len().max(expected) as i32))
    });
    // Hints to engine systems the host does not model: AI perception events,
    // threat bias between groups (zombies always hunt the closest player),
    // engagement ranges, player shove and knockback, the health shield, rope
    // physics, weapon hide-tags on display models, the transporter screen
    // effect, save games and network stat reports.
    register_actor(registry);
    registry.register(Method, "addaieventlistener", |world, receiver, args| {
        let entity = super::engine::entity_id(world, receiver)?;
        let name = super::super::args::string(args, 0)?;
        super::super::actors::add_ai_event_listener(world, entity, &name)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "removeaieventlistener", |world, receiver, args| {
        let entity = super::engine::entity_id(world, receiver)?;
        let name = if args.is_empty() {
            None
        } else {
            Some(super::super::args::string(args, 0)?)
        };
        super::super::actors::remove_ai_event_listener(world, entity, name.as_deref())?;
        Ok(Value::Undefined)
    });
    // `pushplayer(true)` leaves player bodies out of the actor's clipmask,
    // `false` puts them back.
    registry.register(Method, "pushplayer", |world, receiver, args| {
        let actor = super::engine::entity_id(world, receiver)?;
        let push = super::super::args::int(args, 0)? != 0;
        if let Some(movement) = world
            .resource_mut::<crate::script::RoundScript>()
            .actor_moves
            .get_mut(&actor)
        {
            movement.push_player = push;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setengagementmindist", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        let dist = super::super::args::float(args, 0)?;
        let falloff = super::super::args::float(args, 1)?;
        if dist < falloff {
            return Err(format!(
                "Min dist falloff must be <= min dist. [{dist:.6} < {falloff:.6}]"
            ));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setengagementmaxdist", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        let dist = super::super::args::float(args, 0)?;
        let falloff = super::super::args::float(args, 1)?;
        if falloff < dist {
            return Err(format!(
                "Max dist falloff must be >= max dist. [{falloff:.6} > {dist:.6}]"
            ));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "useweaponhidetags", |world, receiver, args| {
        super::t5_client::use_weapon_hide_tags(world, receiver, args, |world, name| {
            super::t5_client::weapon_model(world, name).map(|(_, tags)| tags)
        })
    });
    registry.register(Method, "getthreatbiasgroup", |_, _, _| {
        Ok(Value::string(""))
    });
    for name in [
        "setthreatbiasgroup",
        "setteamforentity",
        "playerknockback",
        "enablehealthshield",
        // A perk machine shaking as the power comes on.
        "vibrate",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    // `ent Gib(type, tags_array)`: encodes the gib type (`"freeze"` ->
    // `0x100`, `"up"` -> `0x200`, `"normal"` -> `0`) and location bitmask
    // (`0..=7`) into event `0xca` for the client's `CodeCallback_GibEvent`.
    registry.register(Method, "gib", |world, receiver, args| {
        let entity = super::engine::entity_id(world, receiver)?;
        let gib_type = super::super::args::string(args, 0)?;
        let type_bits = match gib_type.as_str() {
            "freeze" => 0x100u32,
            "up" => 0x200u32,
            _ => 0u32,
        };
        let tags = super::super::arrays::array_values(world, super::super::args::arg(args, 1)?)?;
        let mut mask = type_bits;
        for val in tags {
            let Value::Int(bit) = val else {
                return Err("gib takes an array of integer locations".into());
            };
            if !(0..=7).contains(&bit) {
                return Err("gib location out of range 0..7".into());
            }
            mask |= 1u32 << (bit as u32);
        }
        diag::info!(
            Sim,
            "t5 server gib entity={entity} type={gib_type} mask=0x{mask:x}"
        );
        crate::script::client::gib_event(world, entity, mask);
        Ok(Value::Undefined)
    });
    // `player IsLookingAt(ent)`: whether the player's `lookatent` (a field
    // scripts set) is that entity.
    registry.register(Method, "islookingat", |world, receiver, args| {
        let player = super::engine::entity_id(world, receiver)?;
        if world
            .resource::<RoundScript>()
            .player_client(player)
            .is_none()
        {
            let number = world.resource::<RoundScript>().entities[&player].number;
            return Err(format!("entity {number} is not a player"));
        }
        let target = super::engine::entity_id(world, super::super::args::arg(args, 0)?)?;
        let looking = world
            .resource_mut::<RoundScript>()
            .object_field(player, "lookatent")
            == Value::Object(target);
        Ok(Value::Int(looking.into()))
    });
    // `ent PlayWeaponDeathEffects(weapon, [int])`: sends the entity's weapon
    // death effect event (0xc1) to the client's `CodeCallback_PlayWeaponDeathEffects`.
    registry.register(Method, "playweapondeatheffects", |world, receiver, args| {
        if args.is_empty() {
            return Err("PlayWeaponDeathEffects <weaponName>.\n".into());
        }
        let name = super::super::args::string(args, 0)?;
        let known = crate::frame::FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .is_some_and(|index| index != 0);
        if !known {
            return Err(format!(
                "PlayWeaponDeathEffects called with unknown weapon name {name}\n"
            ));
        }
        let entity = super::engine::entity_id(world, receiver)?;
        let userdata = if args.len() == 2 {
            i32::from(super::super::args::int(args, 1)? as i16)
        } else {
            0
        };
        crate::script::client::weapon_death_effects(world, entity, name, userdata);
        Ok(Value::Undefined)
    });
    // `ent SetSpawnerTeam(team)`: an AI spawner's team.
    registry.register(Method, "setspawnerteam", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let spawner = world.resource::<RoundScript>().entities[&id]
            .classname
            .starts_with("actor_");
        if !spawner {
            return Err("setspawnerteam can only be applied to AI spawners".into());
        }
        let team = super::super::args::string(args, 0)?;
        if !matches!(team.as_str(), "axis" | "allies" | "neutral") {
            return Err(format!(
                "unknown team '{team}', should be axis, allies, or neutral"
            ));
        }
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "team", Value::string(&team));
        Ok(Value::Undefined)
    });
    registry.register(Method, "setblur", |world, receiver, args| {
        player_blur(world, receiver, args, false)
    });
    registry.register(Method, "startfadingblur", |world, receiver, args| {
        player_blur(world, receiver, args, true)
    });
    registry.register(Method, "startrevive", |world, receiver, _| {
        let id = crate::ClientId(super::player::player(world, receiver)?);
        crate::game_move::set_being_revived(
            &mut crate::frame::FrameWorld::from_world(world),
            id,
            true,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "stoprevive", |world, receiver, _| {
        let id = crate::ClientId(super::player::player(world, receiver)?);
        crate::game_move::set_being_revived(
            &mut crate::frame::FrameWorld::from_world(world),
            id,
            false,
        );
        Ok(Value::Undefined)
    });
    // Stance and lean locks (while a perk bottle is drunk), kept by Black
    // Ops' movement.
    macro_rules! restriction {
        ($($name:literal => $what:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let allowed = super::player::flag(args, 0)?;
                let id = crate::ClientId(super::player::player(world, receiver)?);
                crate::game_move::restrict(
                    &mut crate::frame::FrameWorld::from_world(world),
                    id,
                    game_api::movement::MoveRestriction::$what,
                    allowed,
                );
                Ok(Value::Undefined)
            });
        )*};
    }
    restriction! {
        "allowstand" => Stand,
        "allowcrouch" => Crouch,
        "allowprone" => Prone,
        "allowlean" => Lean,
    }
    // `allowmelee(0)` sets the no-melee pm flag (`0x0800_0000`); `allowmelee(1)`
    // clears it.
    registry.register(Method, "allowmelee", |world, receiver, args| {
        let allowed = super::player::flag(args, 0)?;
        let id = crate::ClientId(super::player::player(world, receiver)?);
        crate::game_move::set_script_flag(
            &mut crate::frame::FrameWorld::from_world(world),
            id,
            game_api::movement::ScriptFlag::NoMelee,
            !allowed,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableinvulnerability", |world, receiver, _| {
        set_invulnerable(world, receiver, true)
    });
    registry.register(Method, "disableinvulnerability", |world, receiver, _| {
        set_invulnerable(world, receiver, false)
    });
    // No search for a spot that sees a lost enemy: the reacquire steps fail
    // and the animscripts fall back to running at the enemy along the paths.
    registry.register(Method, "reacquirestep", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        let _ = super::super::args::float(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "usereacquirenode", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        Ok(Value::Int(0))
    });
    registry.register(Method, "reacquiremove", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        Ok(Value::Int(0))
    });
    for name in [
        "findreacquirenode",
        "getreacquirenode",
        "findreacquiredirectpath",
        "findreacquireproximatepath",
        "flagenemyunattackable",
    ] {
        registry.register(Method, name, |world, receiver, _| {
            let id = super::engine::entity_id(world, receiver)?;
            if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
                return Err("not an actor".into());
            }
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "magicgrenade", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = {
            let runtime = world.resource::<RoundScript>();
            if runtime.entities[&id].kind != EntityKind::Actor {
                return Err("MagicGrenade only supports actors.\n".into());
            }
            runtime.entities[&id].classname.clone()
        };
        if args.len() != 2 && args.len() != 3 {
            return Err(
                "<actor> MagicGrenade <origin> <target position> [time to blow (seconds)].\n"
                    .into(),
            );
        }
        let _ = super::super::args::vector(args, 0)?;
        let _ = super::super::args::vector(args, 1)?;
        if args.len() == 3 {
            let _ = super::super::args::float(args, 2)?;
        }
        Err(format!(
            "Actor [{classname}] doesn't have a grenade weapon set."
        ))
    });
    registry.register(Method, "magicgrenademanual", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = {
            let runtime = world.resource::<RoundScript>();
            if runtime.entities[&id].kind != EntityKind::Actor {
                return Err("MagicGrenadeManual only supports actors.\n".into());
            }
            runtime.entities[&id].classname.clone()
        };
        if args.len() != 2 && args.len() != 3 {
            return Err(
                "<actor> MagicGrenadeManual <origin> <velocity> [time To Blow (seconds)].\n".into(),
            );
        }
        let _ = super::super::args::vector(args, 0)?;
        let _ = super::super::args::vector(args, 1)?;
        if args.len() == 3 {
            let _ = super::super::args::float(args, 2)?;
        }
        Err(format!(
            "Actor [{classname}] doesn't have a grenade weapon set."
        ))
    });
    registry.register(Method, "dropweapon", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        let name = super::super::args::string(args, 0)?;
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return Ok(Value::Undefined);
        }
        let _ = super::super::args::string(args, 1)?;
        let _ = super::super::args::float(args, 2)?;
        let known = crate::frame::FrameWorld::from_world(world)
            .weapon_index_by_script_name(&name)
            .is_some_and(|index| index != 0);
        if !known {
            return Err(format!("unknown weapon '{name}' in dropWeapon"));
        }
        Ok(Value::Undefined)
    });
    // Black Ops: an unknown name is weapon 0, which is no gas weapon
    // (`weaponType` 4).
    registry.register(Function, "weaponisgasweapon", |world, _, args| {
        let name = super::super::args::string(args, 0)?;
        let frame = crate::frame::FrameWorld::from_world(world);
        let gas = frame
            .weapon_index_by_script_name(&name)
            .filter(|&index| index != 0)
            .and_then(|index| frame.combat_facts_for(index))
            .is_some_and(|facts| facts.weap_type == WEAPTYPE_GAS);
        Ok(Value::Int(gas.into()))
    });
    // The weapon a dual-wield weapon holds in its left hand, else "none".
    registry.register(Function, "weapondualwieldweaponname", |world, _, args| {
        let name = super::super::args::string(args, 0)?;
        let frame = crate::frame::FrameWorld::from_world(world);
        let weapon = named_weapon(&frame, &name)?;
        let left = frame
            .combat_facts_for(weapon)
            .filter(|facts| facts.dual_wield)
            .and(frame.content().game_weapon(weapon))
            .map_or(0, |game| game.dual_wield_weapon);
        if left == 0 {
            return Ok(Value::string("none"));
        }
        Ok(Value::string(frame.weapon_script_name(left)))
    });
    // No water volumes are simulated.
    registry.register(Method, "depthinwater", |_, _, _| Ok(Value::Float(0.0)));
    registry.register(Function, "absangleclamp180", |_, _, args| {
        Ok(Value::Float(abs_angle_clamp180(super::super::args::float(
            args, 0,
        )?)))
    });
    // A single-player public demo build answers true; this is not one.
    registry.register(Function, "iscoopepd", |_, _, args| {
        if !args.is_empty() {
            return Err("USAGE: bool = IsCoopEPD()\n".into());
        }
        Ok(Value::Int(0))
    });
    // Black Ops' handler does nothing.
    registry.register(Method, "givegamerpicture", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "weaponisboltaction", |world, _, args| {
        let name = super::super::args::string(args, 0)?;
        let frame = crate::frame::FrameWorld::from_world(world);
        let Some(index) = frame
            .weapon_index_by_script_name(&name)
            .filter(|_| !name.eq_ignore_ascii_case("none"))
        else {
            return Err(if name.eq_ignore_ascii_case("none") {
                format!("Weapon name \"{name}\" is not valid.\n")
            } else {
                format!(
                    "Unknown weapon name \"{name}\": script may need to call PreCacheItem(\"{name}\") during level init.\n"
                )
            });
        };
        Ok(Value::Int(
            frame.weapon_combat_row_or_none(index).bolt_action.into(),
        ))
    });
    registry.register(Function, "getthreatbias", |_, _, _| Ok(Value::Int(0)));
    // The server's `UpdateGamerProfile()` does nothing in Black Ops.
    registry.register(Function, "updategamerprofile", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Method, "uploadscore", |world, receiver, args| {
        upload_score(world, receiver, args)
    });
    registry.register(Function, "missionfailed", |world, _, _| {
        mission_failed(world)?;
        Ok(Value::Undefined)
    });
    // `ChangeLevel(map, persistent, exitTime)`: the level `map` loads once
    // `exitTime` seconds have passed. Without the third argument Black Ops
    // waits `g_changelevel_time` (registered 0); the runtime keeps no
    // weapons across levels, so `persistent` is read and dropped.
    registry.register(Function, "changelevel", |world, _, args| {
        let map = super::super::args::string(args, 0)?;
        if args.len() > 1 {
            super::super::args::int(args, 1)?;
        }
        let exit_ms = if args.len() > 2 {
            let ms = (super::super::args::float(args, 2)? * 1000.0).round() as i64;
            if ms < 0 {
                return Err("exitTime cannot be negative".into());
            }
            ms
        } else {
            let time = world
                .resource::<crate::script::MatchScript>()
                .dvars
                .get("g_changelevel_time")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0);
            (time.max(0.0) * 1000.0).round() as i64
        };
        let now = i64::from(crate::level_time_ms(
            world.resource::<crate::step::StepRequest>().tick,
        ));
        world.resource_mut::<RoundScript>().change_level = Some((map, now + exit_ms));
        Ok(Value::Undefined)
    });
    // `SaveGame(name, description, screenshot, flag)`: Black Ops refuses a
    // save while the first player is dead, and saves the whole game at the
    // end of the frame under `<mapname>` ("levelstart") or
    // `<mapname>-<name>`; it answers the save's id.
    registry.register(Function, "savegame", |world, _, args| {
        let dead = crate::frame::FrameWorld::from_world(world)
            .player(crate::ClientId(0))
            .is_some_and(|player| player.health < 1);
        if dead {
            return Err("Attempting to save while dead\n".into());
        }
        save(world, args, true)
    });
    // `SaveGameNoCommit`: refuses while any player is dead, and keeps the
    // save aside until `CommitSave` takes it.
    registry.register(Function, "savegamenocommit", |world, _, args| {
        let dead = crate::frame::FrameWorld::from_world(world)
            .client_ids_sorted()
            .into_iter()
            .any(|id| {
                crate::frame::FrameWorld::from_world(world)
                    .player(id)
                    .is_some_and(|player| player.health < 1)
            });
        if dead {
            return Err("Attempting to save while dead\n".into());
        }
        save(world, args, false)
    });
    registry.register(Function, "commitsave", |_, _, args| {
        let id = super::super::args::int(args, 0)?;
        crate::save_game::commit(id)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "issavesuccessful", |world, _, _| {
        Ok(Value::Int(i32::from(crate::save_game::successful(world))))
    });
    for name in [
        "createthreatbiasgroup",
        "setthreatbias",
        "ropesetflag",
        "reportmtu",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Undefined));
    }
    // A player who holds a cooked grenade too long drops it at their feet
    // (and still takes the blast) instead of dying at once.
    registry.register(Function, "disablegrenadesuicide", |world, _, _| {
        world
            .resource_mut::<RoundScript>()
            .engine
            .grenade_suicide_disabled = true;
        Ok(Value::Undefined)
    });
    registry.register(Function, "enablegrenadesuicide", |world, _, _| {
        world
            .resource_mut::<RoundScript>()
            .engine
            .grenade_suicide_disabled = false;
        Ok(Value::Undefined)
    });
}

/// A save's arguments: its name (`""` is "auto"); the description, the
/// screenshot and the flag are not kept.
fn save(world: &mut World, args: &[Value], commit: bool) -> Result<Value, String> {
    let name = super::super::args::string(args, 0)?;
    crate::save_game::request(world, &name, commit).map(Value::Int)
}

/// A boolean engine dvar is set.
fn dvar_set(world: &World, name: &str) -> bool {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .is_some_and(|value| value.trim().parse::<f64>().is_ok_and(|v| v != 0.0))
}

/// Whether a failed level slows time: not while a listed dvar is set, nor
/// in the excluded game type.
fn slows_time(world: &World, slow: &game_api::FailedSlowMotion) -> bool {
    let dvars = &world.resource::<crate::script::MatchScript>().dvars;
    let gametype = dvars.get("ui_gametype").map_or("", String::as_str);
    !slow.unless_dvars.iter().any(|name| dvar_set(world, name))
        && !gametype.eq_ignore_ascii_case(slow.unless_gametype)
}

/// An angle's distance from 0 in degrees, 0 to 180: the angle in turns
/// less its floor (rounded through the float's 2^23 trick), back in
/// degrees, folded at 180.
fn abs_angle_clamp180(angle: f32) -> f32 {
    let turns = angle * (1.0 / 360.0);
    let sign = f32::from_bits(turns.to_bits() & 0x8000_0000);
    let magic = f32::from_bits(0x4b00_0000 | sign.to_bits());
    let mut floor = turns + magic - magic;
    // `cmpnless`: one less when rounding went up (or the difference is NaN).
    if !(floor - turns <= sign) {
        floor -= 1.0;
    }
    let degrees = (turns - floor) * 360.0;
    if degrees > 180.0 {
        360.0 - degrees
    } else {
        degrees
    }
}

/// `uploadScore(leaderboard, value, …)`: nothing in a system link game;
/// else the receiver must be a player and take 2 to 10 integers, which
/// Black Ops sends that player's client as its leaderboard write command.
/// No client here keeps a leaderboard, so nothing is sent.
fn upload_score(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    // `systemlink` defaults to 0.
    let systemlink = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("systemlink")
        .is_some_and(|value| value.trim() == "1");
    if systemlink {
        return Ok(Value::Undefined);
    }
    super::player::player(world, receiver)?;
    if !(2..=10).contains(&args.len()) {
        return Err("Incorrect number of parameters\n".into());
    }
    for index in 0..args.len() {
        super::super::args::int(args, index)?;
    }
    Ok(Value::Undefined)
}

/// Every client is sent the failure's client dvar; with a player in the level
/// and no reload under way, the client blur ramps to `blur_target` over the
/// death delay, any `ui_deadquote` is sent with its `death` sound, and the
/// level loads again once the delay has passed.
fn mission_failed(world: &mut World) -> Result<(), String> {
    let rules = crate::frame::FrameWorld::from_world(world)
        .bootstrap_ref()
        .mode
        .and_then(|mode| mode.level_failed)
        .ok_or("missionfailed: this mode cannot fail its level")?;
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    let (name, value) = rules.client_dvar;
    for &client in &clients {
        super::player::send_client_dvar(world, client, name, value);
    }
    if players(world).is_empty() {
        return Ok(());
    }
    let now_ms = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let now = i64::from(now_ms);
    let deadquote = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(&rules.deadquote_dvar.to_ascii_lowercase())
        .cloned()
        .filter(|s| !s.is_empty());
    let already_reloading = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let was = runtime.reloading;
        if !was {
            runtime.reloading = true;
            let delay = rules.reload_delay_ms + rules.death_delay_ms;
            runtime.reload_after_ms = Some(now + delay);
            if let Some(gap) = rules.unrun {
                diag::info!(Sim, "game gap {}: {}", gap.id, gap.what);
            }
        }
        was
    };
    if !already_reloading {
        if let Some(slow) = rules.slow_motion
            && slows_time(world, &slow)
        {
            let ramp = rules.death_delay_ms;
            let real = rules.reload_delay_ms + rules.death_delay_ms;
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.engine.slow_motion = Some(crate::ScriptSlowMotion {
                from: 1.0,
                to: slow.to,
                start_ms: now_ms,
                duration_ms: ramp as i32,
            });
            // The reload waits real time; under the ramp (linear in real
            // time, then held) that much real time is this much match time.
            let to = f64::from(slow.to);
            let match_ms = ramp as f64 * (1.0 + to) * 0.5 + (real - ramp).max(0) as f64 * to;
            runtime.reload_after_ms = Some(now + match_ms.round() as i64);
        }
        if rules.fade_out_sound {
            crate::frame::FrameWorld::from_world(world)
                .push_script_audio(crate::ScriptAudioCommand::SoundFadeOut);
        }
        let mut frame = crate::frame::FrameWorld::from_world(world);
        for &client in &clients {
            if frame.client_meta(crate::ClientId(client)).is_some() {
                frame
                    .client_meta_mut(crate::ClientId(client))
                    .view_effects
                    .blur = Some(crate::ScriptBlur {
                    from: 0.0,
                    to: rules.blur_target,
                    set_ms: now_ms,
                    duration_ms: rules.death_delay_ms as i32,
                });
            }
        }
        if let Some(quote) = deadquote {
            for &client in &clients {
                super::player::send_client_dvar(world, client, rules.deadquote_dvar, &quote);
            }
            let mut frame = crate::frame::FrameWorld::from_world(world);
            let alias_index = frame.sound_alias_index(rules.death_sound);
            for &client in &clients {
                frame.push_local_sound(crate::PendingLocalSound {
                    recipient: crate::ClientId(client),
                    stop: false,
                    alias_index,
                });
            }
        }
    }
    Ok(())
}

/// Black Ops' `setblur` and `startfadingblur` player methods: validate `time`
/// and `blur`, then ramp the player's screen blur (`setblur` from current to
/// `blur` over `time * 1000` ms; `startfadingblur` from `blur` to `0` over
/// `time * 1000 + 50` ms) unless a priority-2 `MissionFailed` blur is active.
fn player_blur(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    fading: bool,
) -> Result<Value, String> {
    let blur = super::super::args::float(args, 0)?;
    let seconds = super::super::args::float(args, 1)?;
    if seconds < 0.0 {
        return Err("Time must be positive".into());
    }
    let duration_ms = (seconds * 1000.0) as i32;
    if blur < 0.0 {
        return Err("Blur value must be greater than 0".into());
    }
    let client = super::player::player(world, receiver)
        .map_err(|_| "setblur() called on an invalid client entity.\n".to_owned())?;
    if world.resource::<RoundScript>().reloading {
        return Ok(Value::Undefined);
    }
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let mut frame = crate::frame::FrameWorld::from_world(world);
    if frame.client_meta(crate::ClientId(client)).is_some() {
        let view = &mut frame.client_meta_mut(crate::ClientId(client)).view_effects;
        view.blur = Some(if fading {
            crate::ScriptBlur {
                from: blur,
                to: 0.0,
                set_ms: now,
                duration_ms: duration_ms + 50,
            }
        } else {
            crate::ScriptBlur {
                from: view.blur.map_or(0.0, |b| b.sample(now)),
                to: blur,
                set_ms: now,
                duration_ms,
            }
        });
    }
    Ok(Value::Undefined)
}

fn set_invulnerable(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let client = super::player::player(world, receiver)?;
    if let Some(slot) = world.resource_mut::<RoundScript>().players.get_mut(&client) {
        slot.invulnerable = on;
    }
    Ok(Value::Undefined)
}

fn optional_team(args: &[Value]) -> Result<Option<String>, String> {
    match args {
        [] | [Value::Undefined] => Ok(None),
        [Value::String(team)] if team.as_bytes() == b"all" => Ok(None),
        [Value::String(team)] => Ok(Some(team.to_string())),
        _ => Err("takes an optional team name".into()),
    }
}

fn on_team(world: &mut World, id: u64, team: &str) -> bool {
    match world.resource_mut::<RoundScript>().object_field(id, "team") {
        Value::String(own) => own.as_bytes() == team.as_bytes(),
        _ => true,
    }
}

/// Map actors flagged as spawners, in entity order.
fn spawners(world: &mut World) -> Vec<u64> {
    let candidates: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("actor_"))
        .map(|(id, _)| *id)
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    candidates
        .into_iter()
        .filter(|&id| matches!(runtime.object_field(id, "spawnflags"), Value::Int(flags) if flags & 1 != 0))
        .collect()
}

/// Actors alive on `team` (or any team), in entity order.
/// The teams `getaiarray` takes: axis, allies and neutral.
const ALL_TEAMS: u32 = 0xe;

/// A team name's bit in Black Ops' team mask.
fn team_mask(name: &str) -> Result<u32, String> {
    match name {
        "axis" => Ok(1 << 1),
        "allies" => Ok(1 << 2),
        "neutral" => Ok(1 << 3),
        "all" => Ok(ALL_TEAMS),
        _ => Err(format!(
            "unknown team '{name}' in getaiarray (should be axis, allies, or neutral)"
        )),
    }
}

/// A species name; `None` for all.
fn species_named(name: &str) -> Result<Option<&'static str>, String> {
    match name {
        "all" => Ok(None),
        _ => ["human", "dog", "zombie", "zombie_dog"]
            .into_iter()
            .find(|species| *species == name)
            .map(Some)
            .ok_or_else(|| format!("unknown species '{name}' (should be human, dog, or all)")),
    }
}

/// Living actors, in entity order, on a team in the mask whose species
/// `keep` takes; an actor whose death is held off is left out.
fn ai_array(world: &mut World, mask: u32, keep: impl Fn(&str) -> bool) -> Vec<u64> {
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, _)| *id)
        .collect();
    actors
        .into_iter()
        .filter(|&id| {
            if !super::super::actor_brain::alive(world, id) {
                return false;
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            let delayed =
                matches!(runtime.object_field(id, "delayeddeath"), Value::Int(n) if n != 0);
            let bit = match runtime.object_field(id, "team") {
                Value::String(team) => team_mask(&team).ok().filter(|&bit| bit != ALL_TEAMS),
                _ => None,
            };
            let species = match runtime.object_field(id, "type") {
                Value::String(species) => species.to_string(),
                _ => String::new(),
            };
            !delayed && bit.is_some_and(|bit| bit & mask != 0) && keep(&species)
        })
        .collect()
}

fn no_args(args: &[Value]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("takes no arguments".into())
    }
}

/// Player entities in client order.
fn players(world: &mut World) -> Vec<u64> {
    let mut players: Vec<(i32, u64)> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Player)
        .map(|(id, entity)| (entity.number, *id))
        .collect();
    players.sort_unstable();
    players.into_iter().map(|(_, id)| id).collect()
}

/// What Black Ops keeps on an actor for the methods below: the species it
/// talks to.
#[derive(bevy_ecs::prelude::Resource, Default)]
struct ActorState(std::collections::BTreeMap<u64, ActorFields>);

#[derive(Default)]
struct ActorFields {
    /// One bit per species `settalktospecies` names; -1 for all.
    talk_species: i32,
    /// `setdeathcontents`'s contents.
    death_contents: Option<i32>,
    /// `setflashbanged`'s state and strength.
    flashbanged: (bool, f32),
}

/// Black Ops' objective states, by number.
const OBJECTIVE_STATES: [&str; 6] = ["empty", "active", "invisible", "done", "current", "failed"];

/// Each objective's state (`objective_state`).
#[derive(bevy_ecs::prelude::Resource, Default)]
struct Objectives([u8; 32]);

/// The species `settalktospecies` names, by bit.
const SPECIES: [&str; 4] = ["human", "dog", "zombie", "zombie_dog"];

fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("not an actor".into()),
    }
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match super::super::players::entity_field(world, id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn actor_fields(world: &mut World, id: u64) -> &mut ActorFields {
    world
        .get_resource_or_insert_with(ActorState::default)
        .into_inner()
        .0
        .entry(id)
        .or_default()
}

/// Black Ops' actor methods that only set what the actor keeps.
fn register_actor(registry: &mut NativeRegistry) {
    use Namespace::Method;
    // Each argument naming a species sets its bit (other names set none);
    // `all` sets every bit.
    registry.register(Method, "settalktospecies", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let mut mask = 0;
        for index in 0..args.len() {
            let name = super::super::args::string(args, index)?;
            if name == "all" {
                mask = -1;
                break;
            }
            if let Some(bit) = SPECIES.iter().position(|species| *species == name) {
                mask |= 1 << bit;
            }
        }
        actor_fields(world, id).talk_species = mask;
        Ok(Value::Undefined)
    });
    // The actor drops its enemy; its next think picks one again.
    registry.register(Method, "clearenemy", |world, receiver, _| {
        let id = actor(world, receiver)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "enemy", Value::Undefined);
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "getanglestolikelyenemypath",
        |world, receiver, _| {
            let id = actor(world, receiver)?;
            Ok(super::super::actor_nav::likely_enemy_path_angles(world, id)
                .map_or(Value::Undefined, Value::Vector))
        },
    );
    registry.register(Method, "geteyeapprox", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let (kind, classname) = {
            let runtime = world.resource::<RoundScript>();
            let entity = &runtime.entities[&id];
            (entity.kind.clone(), entity.classname.clone())
        };
        let origin = vector_field(world, id, "origin");
        let lift = match kind {
            EntityKind::Player => world
                .resource::<RoundScript>()
                .player_client(id)
                .and_then(|c| {
                    crate::frame::FrameWorld::from_world(world)
                        .player(crate::ClientId(c))
                        .map(|ps| ps.view_height_current)
                })
                .unwrap_or(0.0),
            EntityKind::Actor => {
                let height = super::super::triggers::entity_bounds(world, id).1[2];
                height * 0.9_f32
            }
            _ => {
                return Err(format!(
                    "getEye must be called on an AI or player, not on a '{classname}'"
                ));
            }
        };
        Ok(Value::Vector([origin[0], origin[1], origin[2] + lift]))
    });
    registry.register(Method, "getshootatpos", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let kind = world.resource::<RoundScript>().entities[&id].kind.clone();
        let origin = vector_field(world, id, "origin");
        match kind {
            EntityKind::Player => {
                let lift = world
                    .resource::<RoundScript>()
                    .player_client(id)
                    .and_then(|c| {
                        crate::frame::FrameWorld::from_world(world)
                            .player(crate::ClientId(c))
                            .map(|ps| ps.view_height_current)
                    })
                    .unwrap_or(0.0);
                Ok(Value::Vector([origin[0], origin[1], origin[2] + lift]))
            }
            EntityKind::Actor => {
                if let Some(offset) = super::super::presence::tag_offset(world, id, "tag_eye") {
                    let angles = vector_field(world, id, "angles");
                    let (sy, cy) = angles[1].to_radians().sin_cos();
                    return Ok(Value::Vector([
                        origin[0] + cy * offset[0] - sy * offset[1],
                        origin[1] + sy * offset[0] + cy * offset[1],
                        origin[2] + offset[2],
                    ]));
                }
                Ok(Value::Vector([origin[0], origin[1], origin[2] + 64.0]))
            }
            _ => Ok(Value::Vector(origin)),
        }
    });
    registry.register(Method, "getaivelocity", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let (kind, classname) = {
            let runtime = world.resource::<RoundScript>();
            let entity = &runtime.entities[&id];
            (entity.kind.clone(), entity.classname.clone())
        };
        let velocity = match kind {
            EntityKind::Player => world
                .resource::<RoundScript>()
                .player_client(id)
                .and_then(|c| {
                    crate::frame::FrameWorld::from_world(world)
                        .player(crate::ClientId(c))
                        .map(|ps| ps.velocity)
                })
                .unwrap_or([0.0; 3]),
            EntityKind::Actor => world
                .resource::<RoundScript>()
                .actor_moves
                .get(&id)
                .map_or([0.0; 3], |movement| movement.velocity),
            _ => return Err(format!("entity type '{classname}' is not a sentient")),
        };
        Ok(Value::Vector(velocity))
    });
    // `BloodImpact("none" | "normal" | "hero")` sets which blood impact
    // effect a flesh hit on the entity plays (client event 0x4f).
    registry.register(Method, "bloodimpact", |world, receiver, args| {
        let entity = super::engine::entity_id(world, receiver)?;
        if args.is_empty() {
            return Err("BloodImpact() called without an argument\n".into());
        }
        let mode = match super::super::args::string(args, 0)?.as_str() {
            "none" => super::t5::BloodImpact::None,
            "normal" => super::t5::BloodImpact::Normal,
            "hero" => super::t5::BloodImpact::Hero,
            _ => {
                return Err(
                    "BloodImpact() only takes \"normal\", \"hero\" or \"none\" as parameters.\n"
                        .into(),
                );
            }
        };
        world
            .resource_mut::<RoundScript>()
            .t5
            .blood_impact
            .insert(entity, mode);
        Ok(Value::Int(1))
    });
    registry.register(Method, "getplayerviewheight", |world, receiver, _| {
        let client = super::player::player(world, receiver)?;
        let height = crate::frame::FrameWorld::from_world(world)
            .player(crate::ClientId(client))
            .map_or(0.0, |ps| ps.view_height_current);
        Ok(Value::Float(height))
    });
    registry.register(Method, "getcentroid", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let kind = world.resource::<RoundScript>().entities[&id].kind.clone();
        let origin = vector_field(world, id, "origin");
        let angles = vector_field(world, id, "angles");
        // A client sits 35 up its axis, an actor 36; any other entity at the
        // middle of its bounds turned by its axis.
        let center = match kind {
            EntityKind::Actor => [0.0, 0.0, 36.0],
            EntityKind::Player => [0.0, 0.0, 35.0],
            _ => {
                let (mins, maxs) = super::super::triggers::entity_bounds(world, id);
                std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5)
            }
        };
        let (sp, cp) = angles[0].to_radians().sin_cos();
        let (sy, cy) = angles[1].to_radians().sin_cos();
        let (sr, cr) = angles[2].to_radians().sin_cos();
        let axis = [
            [cp * cy, cp * sy, -sp],
            [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
            [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
        ];
        Ok(Value::Vector(std::array::from_fn(|i| {
            origin[i] + center[0] * axis[0][i] + center[1] * axis[1][i] + center[2] * axis[2][i]
        })))
    });
    registry.register(Method, "getweaponmuzzlepoint", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let origin = vector_field(world, id, "origin");
        let lift = match world.resource::<RoundScript>().player_client(id) {
            Some(client) => crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map_or(0.0, |ps| ps.view_height_current),
            None => 0.0,
        };
        Ok(Value::Vector([origin[0], origin[1], origin[2] + lift]))
    });
    registry.register(Method, "getweaponforwarddir", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let angles = match world.resource::<RoundScript>().player_client(id) {
            Some(client) => crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map_or([0.0; 3], |ps| ps.viewangles),
            None => vector_field(world, id, "angles"),
        };
        let (sp, cp) = angles[0].to_radians().sin_cos();
        let (sy, cy) = angles[1].to_radians().sin_cos();
        Ok(Value::Vector([cp * cy, cp * sy, -sp]))
    });
    registry.register(
        Namespace::Function,
        "physicsexplosioncylinder",
        |world, _, args| {
            if args.len() != 4 {
                return Err("Incorrect number of parameters".into());
            }
            let origin = super::super::args::vector(args, 0)?;
            let outer = super::super::args::int(args, 1)? as u16;
            let inner = super::super::args::float(args, 2)?;
            if inner < 0.0 {
                return Err("Radius is negative".into());
            }
            if f32::from(outer) < inner {
                return Err("Inner radius is outside the outer radius".into());
            }
            let magnitude = super::super::args::float(args, 3)?;
            let tick = world.resource::<crate::step::StepRequest>().tick;
            crate::gentity::push_physics_explosion(
                &mut crate::frame::FrameWorld::from_world(world),
                tick,
                origin,
                f32::from(outer),
                inner,
                magnitude,
                0,
                0,
                true,
                [0.0; 3],
            );
            Ok(Value::Undefined)
        },
    );
    // `ent setElectrified(time)`, `ent setBurn(time)` and
    // `ent setTransported(time)`: the player's client plays the overlay for
    // the time in whole msec; 0 ends it, fading from then.
    macro_rules! flame {
        ($($name:literal => $kind:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let time = super::super::args::float(args, 0)?;
                if time < 0.0 {
                    return Err("Time must be positive".into());
                }
                let client = super::player::player(world, receiver).map_err(|_| {
                    concat!($name, "() called on an invalid client entity.\n").to_owned()
                })?;
                timed_flame(world, client, super::t6::flame_overlay::Flame::$kind, time);
                Ok(Value::Undefined)
            });
        )*};
    }
    flame!(
        "setelectrified" => Shock,
        "setburn" => Burn,
        "settransported" => Transporter,
    );
    registry.register(
        Method,
        "resetmissiledetonationtime",
        |world, receiver, args| {
            let id = super::engine::entity_id(world, receiver)?;
            let number = match world.resource::<RoundScript>().entities.get(&id) {
                Some(entity) if matches!(entity.kind, EntityKind::Missile(_)) => entity.number,
                _ => return Ok(Value::Undefined),
            };
            let after_ms = if !args.is_empty() {
                Some((super::super::args::float(args, 0)? * 1000.0) as i32)
            } else {
                None
            };
            let tick = world.resource::<crate::step::StepRequest>().tick;
            crate::game_fire::reset_game_missile_detonation_time(
                &mut crate::frame::FrameWorld::from_world(world),
                tick,
                number,
                after_ms,
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "setflashbanged", |world, receiver, args| {
        let id = actor(world, receiver)?;
        if args.len() != 1 && args.len() != 2 {
            return Err("Invalid call to setFlashBanged()".into());
        }
        let strength = if args.len() == 2 {
            super::super::args::float(args, 1)?
        } else {
            1.0
        };
        let on = super::super::args::int(args, 0)? != 0;
        actor_fields(world, id).flashbanged = (on, strength);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setdeathcontents", |world, receiver, args| {
        if args.len() != 1 {
            return Err("setdeathcontents takes one parameter\n".into());
        }
        super::engine::entity_id(world, receiver)?;
        let Ok(id) = actor(world, receiver) else {
            return Err("setdeathcontents must be called on an AI only.".into());
        };
        let contents = super::super::args::int(args, 0)?;
        actor_fields(world, id).death_contents = Some(contents);
        let prev = world
            .resource_mut::<RoundScript>()
            .t5
            .death_contents
            .insert(id, contents)
            .unwrap_or(0x0400_2000);
        Ok(Value::Int(prev))
    });
    registry.register(Namespace::Function, "objective_state", |world, _, args| {
        let index = super::super::args::int(args, 0)?;
        if !(0..32).contains(&index) {
            return Err(format!(
                "index {index} is an illegal objective index. Valid indexes are 0 to 31\n"
            ));
        }
        let name = super::super::args::string(args, 1)?;
        let Some(state) = OBJECTIVE_STATES.iter().position(|s| *s == name) else {
            return Err(format!(
                "Illegal objective state \"{name}\". Valid states are \"empty\", \"active\", \"invisible\", \"done\", \"current\", \"failed\"\n"
            ));
        };
        world.get_resource_or_insert_with(Objectives::default).0[index as usize] = state as u8;
        Ok(Value::Undefined)
    });
    // `dontInterpolate` flips the entity's teleport flag, a client's in its
    // player state, any other entity's in its own entity state, so the next
    // snapshot snaps it in place. An entity with no presented model sends no
    // state; its flag has nothing to reach.
    registry.register(Method, "dontinterpolate", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        if let Some(client) = runtime.player_client(id) {
            crate::script_player::snap(
                &mut crate::frame::FrameWorld::from_world(world),
                crate::ClientId(client),
            );
        } else if let Some(presence) = runtime.entities.get(&id).and_then(|e| e.presence) {
            crate::script_player::snap_presence(
                &mut crate::frame::FrameWorld::from_world(world),
                presence,
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Function, "isvec", |_, _, args| {
        Ok(Value::Int(
            matches!(super::super::args::arg(args, 0)?, Value::Vector(_)).into(),
        ))
    });
    // `LerpFloat(from, to, fraction)`, the fraction held to 0..1.
    registry.register(Namespace::Function, "lerpfloat", |_, _, args| {
        let from = super::super::args::float(args, 0)?;
        let to = super::super::args::float(args, 1)?;
        let mut fraction = super::super::args::float(args, 2)?;
        if fraction - 1.0 >= 0.0 {
            fraction = 1.0;
        }
        if -fraction >= 0.0 {
            fraction = 0.0;
        }
        Ok(Value::Float((to - from) * fraction + from))
    });
    // `LerpVector(from, to, fraction)`, the fraction held to 0..1.
    registry.register(Namespace::Function, "lerpvector", |_, _, args| {
        let from = super::super::args::vector(args, 0)?;
        let to = super::super::args::vector(args, 1)?;
        let mut fraction = super::super::args::float(args, 2)?;
        if fraction - 1.0 >= 0.0 {
            fraction = 1.0;
        }
        if -fraction >= 0.0 {
            fraction = 0.0;
        }
        Ok(Value::Vector(std::array::from_fn(|i| {
            (to[i] - from[i]) * fraction + from[i]
        })))
    });
    registry.register(Namespace::Function, "hascollectible", |world, _, args| {
        has_collectible(world, args)
    });
    // `getWeaponClipModel(name)`: the weapon's world clip model, else "". An
    // unknown name other than "" and "none" prints a warning.
    registry.register(
        Namespace::Function,
        "getweaponclipmodel",
        |world, _, args| {
            let name = super::super::args::string(args, 0)?;
            let frame = crate::frame::FrameWorld::from_world(world);
            let Some(index) = frame
                .weapon_index_by_script_name(&name)
                .filter(|&index| index != 0)
            else {
                if !name.is_empty() && !name.eq_ignore_ascii_case("none") {
                    diag::info!(Sim, "unknown weapon '{name}' in getWeaponClipModel");
                }
                return Ok(Value::string(""));
            };
            let model = frame
                .content()
                .game_weapon(index)
                .and_then(|game| game.world_clip_model.clone());
            Ok(Value::string(model.as_deref().unwrap_or("")))
        },
    );
    registry.register(Namespace::Function, "modelhasphyspreset", |_, _, args| {
        super::super::args::string(args, 0)?;
        Ok(Value::Int(0))
    });
}

/// `HasCollectible(n)`: Black Ops reads character `n − 1` of the
/// `bg_collectibles` dvar, which the player's profile fills, as unlocked
/// unless it is `'0'`. It does not check the index; with no profile the dvar
/// is its registered default `""`, and a character past its end is taken as
/// not unlocked.
pub(crate) fn has_collectible(world: &World, args: &[Value]) -> Result<Value, String> {
    let n = super::super::args::int(args, 0)?;
    let unlocked = world
        .get_resource::<crate::script::MatchScript>()
        .and_then(|script| script.dvars.get("bg_collectibles"))
        .and_then(|value| {
            let index = usize::try_from(n.checked_sub(1)?).ok()?;
            value.as_bytes().get(index).copied()
        })
        .is_some_and(|c| c != b'0');
    Ok(Value::Int(unlocked.into()))
}

/// Black Ops' `weaponType` gas.
const WEAPTYPE_GAS: i32 = 4;

/// A weapon by name, refused as Black Ops' builtins refuse "none" and a
/// name it has not loaded.
fn named_weapon(frame: &crate::frame::FrameWorld, name: &str) -> Result<u32, String> {
    if name.eq_ignore_ascii_case("none") {
        return Err(format!("Weapon name \"{name}\" is not valid.\n"));
    }
    frame
        .weapon_index_by_script_name(name)
        .filter(|&index| index != 0)
        .ok_or_else(|| {
            format!(
                "Unknown weapon name \"{name}\": script may need to call PreCacheItem(\"{name}\") during level init.\n"
            )
        })
}

/// The overlay the server sends its client for `seconds`: the client starts
/// it for the whole msec, or stops it at 0.
fn timed_flame(
    world: &mut World,
    client: u32,
    kind: super::t6::flame_overlay::Flame,
    seconds: f32,
) {
    let ms = (seconds * 1000.0) as i32;
    if ms == 0 {
        super::t6::flame_overlay::stop(world, client, kind);
    } else {
        super::t6::flame_overlay::start(world, client, kind, ms);
    }
}
