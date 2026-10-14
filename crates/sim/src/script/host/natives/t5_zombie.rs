use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getplayers", |world, _, args| {
        let team = optional_team(args)?;
        let players = players(world)
            .into_iter()
            .filter(|&id| team.as_deref().is_none_or(|team| on_team(world, id, team)))
            .map(Value::Object)
            .collect();
        new_array(world, players)
    });
    registry.register(Function, "getaiarray", |world, _, args| {
        let team = optional_team(args)?;
        let actors = living_actors(world, team.as_deref())
            .into_iter()
            .map(Value::Object)
            .collect();
        new_array(world, actors)
    });
    // `GetAISpeciesArray(team, species)`: every actor here is of one species.
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let team = optional_team(&args[..args.len().min(1)])?;
        let actors = living_actors(world, team.as_deref())
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
    // Script pacing on snapshot acknowledgement is left to the network layer:
    // scripts see no remote clients and wait a tenth of a second instead.
    registry.register(Function, "numremoteclients", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getdifficulty", |_, _, args| {
        no_args(args)?;
        Ok(Value::string("medium"))
    });
    registry.register(Function, "issaverecentlyloaded", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    // The host keeps no AI budget, water simulation or exploder ids of its own.
    registry.register(Function, "setailimit", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "watersimenable", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "setexploderid", |_, _, _| Ok(Value::Undefined));
    // Script-state tracing is a developer aid; eye glow is drawn from the head.
    registry.register(Method, "trackscriptstate", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "haseyes", |_, _, _| Ok(Value::Undefined));
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
    for name in [
        "addaieventlistener",
        "setthreatbiasgroup",
        "setteamforentity",
        "setengagementmindist",
        "setengagementmaxdist",
        "pushplayer",
        "playerknockback",
        "enablehealthshield",
        "useweaponhidetags",
        "settransported",
        // A perk machine shaking as the power comes on.
        "vibrate",
        // Gibbed limbs are not thrown yet; scripts swap the damaged models.
        "gib",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
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
    restriction!(
        "allowlean" => Lean,
        "allowcrouch" => Crouch,
        "allowprone" => Prone,
        "allowstand" => Stand,
    );
    // The melee lock is accepted but not enforced yet.
    registry.register(Method, "allowmelee", |world, receiver, _| {
        super::player::player(world, receiver)?;
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
    registry.register(Method, "reacquirestep", |_, _, _| Ok(Value::Int(0)));
    registry.register(Method, "usereacquirenode", |_, _, _| Ok(Value::Int(0)));
    for name in [
        "findreacquirenode",
        "getreacquirenode",
        "flagenemyunattackable",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    // No gas weapons (nova gas) exist in the zombie maps this host runs.
    registry.register(Function, "weaponisgasweapon", |_, _, _| Ok(Value::Int(0)));
    // A dual-wield weapon's left hand shares its clip here: there is no
    // separate left-hand weapon.
    registry.register(Function, "weapondualwieldweaponname", |_, _, _| {
        Ok(Value::string("none"))
    });
    // No water volumes are simulated.
    registry.register(Method, "depthinwater", |_, _, _| Ok(Value::Float(0.0)));
    // The last-stand vision set is not drawn yet.
    registry.register(Method, "visionsetlaststand", |world, receiver, _| {
        super::player::player(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "missionfailed", |world, _, _| {
        mission_failed(world)?;
        Ok(Value::Undefined)
    });
    for name in [
        "createthreatbiasgroup",
        "setthreatbias",
        "ropesetflag",
        "savegame",
        "reportmtu",
        "stopallrumbles",
        "disablegrenadesuicide",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Undefined));
    }
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
            runtime.reload_after_ms = Some(now + rules.reload_delay_ms + rules.death_delay_ms);
            if let Some(gap) = rules.unrun {
                diag::info!(Sim, "game gap {}: {}", gap.id, gap.what);
            }
        }
        was
    };
    if !already_reloading {
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
fn living_actors(world: &mut World, team: Option<&str>) -> Vec<u64> {
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, _)| *id)
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    actors
        .into_iter()
        .filter(|&id| {
            let alive = match runtime.object_field(id, "health") {
                Value::Int(n) => n > 0,
                Value::Float(f) => f > 0.0,
                _ => false,
            };
            let on_team = team.is_none_or(|team| {
                matches!(runtime.object_field(id, "team"), Value::String(own) if own.as_bytes() == team.as_bytes())
            });
            alive && on_team
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
