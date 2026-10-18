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
    registry.register(Function, "issaverecentlyloaded", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    // The host keeps no AI budget, water simulation or exploder ids of its own.
    registry.register(Function, "setailimit", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "watersimenable", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "setexploderid", |_, _, _| Ok(Value::Undefined));
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
    registry.register(Method, "uploadscore", |world, receiver, args| {
        upload_score(world, receiver, args)
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

/// What Black Ops keeps on an actor for the methods below: the species it
/// talks to and its goal volume.
#[derive(bevy_ecs::prelude::Resource, Default)]
struct ActorState(std::collections::BTreeMap<u64, ActorFields>);

#[derive(Default)]
struct ActorFields {
    /// One bit per species `settalktospecies` names; -1 for all.
    talk_species: i32,
    /// `setgoalvolume`'s volume.
    goal_volume: Option<u64>,
    /// `setdeathcontents`'s contents.
    death_contents: Option<i32>,
    /// `setflashbanged`'s state and strength.
    flashbanged: (bool, f32),
    /// `allowpitchangle`'s flag.
    allow_pitch: bool,
    /// `setphysparams`' bounds; the actor's own (72 tall) until set.
    bounds: Option<([f32; 3], [f32; 3])>,
}

/// An actor's height before `setphysparams` changes its bounds.
const ACTOR_HEIGHT: f32 = 72.0;

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
    registry.register(Method, "cleargoalvolume", |world, receiver, _| {
        let id = actor(world, receiver)?;
        actor_fields(world, id).goal_volume = None;
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
                let height = actor_fields(world, id)
                    .bounds
                    .map_or(ACTOR_HEIGHT, |(_, maxs)| maxs[2]);
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
    // `BloodImpact("none" | "normal" | "hero")` picks the blood an entity's
    // hits show; the client event that carries it is not sent (ledgered).
    registry.register(Method, "bloodimpact", |world, receiver, args| {
        super::engine::entity_id(world, receiver)?;
        if args.is_empty() {
            return Err("BloodImpact() called without an argument\n".into());
        }
        match super::super::args::string(args, 0)?.as_str() {
            "none" | "normal" | "hero" => Ok(Value::Undefined),
            _ => Err(
                "BloodImpact() only takes \"normal\", \"hero\" or \"none\" as parameters.\n".into(),
            ),
        }
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
        |_, _, args| {
            if args.len() != 4 {
                return Err("Incorrect number of parameters".into());
            }
            let _origin = super::super::args::vector(args, 0)?;
            let outer = super::super::args::int(args, 1)? as u16;
            let inner = super::super::args::float(args, 2)?;
            if inner < 0.0 {
                return Err("Radius is negative".into());
            }
            if f32::from(outer) < inner {
                return Err("Inner radius is outside the outer radius".into());
            }
            let _magnitude = super::super::args::float(args, 3)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "setelectrified", |world, receiver, args| {
        let time = super::super::args::float(args, 0)?;
        if time < 0.0 {
            return Err("Time must be positive".into());
        }
        super::player::player(world, receiver)
            .map_err(|_| "setelectrified() called on an invalid client entity.\n".to_owned())?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setburn", |world, receiver, args| {
        let time = super::super::args::float(args, 0)?;
        if time < 0.0 {
            return Err("Time must be positive".into());
        }
        super::player::player(world, receiver)
            .map_err(|_| "setburn() called on an invalid client entity.\n".to_owned())?;
        Ok(Value::Undefined)
    });
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
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowpitchangle", |world, receiver, args| {
        let id = actor(world, receiver)
            .map_err(|_| "AllowPitchAngle() called on non-actor\n".to_owned())?;
        if args.is_empty() {
            return Err("AllowPitchAngle() called without an argument (0 or 1)\n".into());
        }
        let allow = super::super::args::int(args, 0)? != 0;
        actor_fields(world, id).allow_pitch = allow;
        Ok(Value::Undefined)
    });
    // `setPhysParams(radius, bottom, top)`: the actor's box becomes
    // (-radius, -radius, bottom)..(radius, radius, top).
    registry.register(Method, "setphysparams", |world, receiver, args| {
        if args.len() != 3 {
            return Err("setphysparams takes three parameters\n".into());
        }
        super::engine::entity_id(world, receiver)?;
        let Ok(id) = actor(world, receiver) else {
            return Err("setphysparams must be called on an AI only.".into());
        };
        let radius = super::super::args::float(args, 0)?;
        let bottom = super::super::args::float(args, 1)?;
        let top = super::super::args::float(args, 2)?;
        actor_fields(world, id).bounds = Some(([-radius, -radius, bottom], [radius, radius, top]));
        Ok(Value::Undefined)
    });
    // Black Ops answers "none" until the actor's movement runs into an
    // entity ("obstacle" for one flagged so, else "world"); the runtime's
    // actor movement collides with nothing, so it is always "none".
    registry.register(Method, "gethitenttype", |world, receiver, _| {
        actor(world, receiver)?;
        Ok(Value::string("none"))
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
    registry.register(Namespace::Function, "modelhasphyspreset", |_, _, args| {
        super::super::args::string(args, 0)?;
        Ok(Value::Int(0))
    });
}
