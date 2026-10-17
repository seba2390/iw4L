//! Black Ops 2's player builtins over IW4L's shared player state and the
//! state Black Ops 2's own movement keeps (`movement_t6`, through
//! `game_move`).

use super::super::super::players::{LinkView, PlayerLink};
use super::super::player::player;
use super::{float, int, vector};
use crate::ClientId;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use crate::script_player::{CmdButton, holds_now_or_last};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The stances a script has refused a player, for the previous allowance
/// each call answers.
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
    what: game_api::movement::MoveRestriction,
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let allowed = int(args, 0)? != 0;
    crate::game_move::restrict(
        &mut FrameWorld::from_world(world),
        ClientId(client),
        what,
        allowed,
    );
    let slot = stance(refused(world, client));
    let was = !*slot;
    *slot = !allowed;
    Ok(Value::Int(was.into()))
}

/// A player method's client, with the engine's error for another entity.
fn client_or_error(world: &World, receiver: &Value) -> Result<ClientId, String> {
    let id =
        super::super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    world
        .resource::<RoundScript>()
        .player_client(id)
        .map(ClientId)
        .ok_or_else(|| {
            let number = world.resource::<RoundScript>().entities[&id].number;
            format!("entity {number} is not a player")
        })
}

fn activity(
    world: &mut World,
    receiver: &Value,
) -> Result<game_api::movement::WeaponActivity, String> {
    let id = client_or_error(world, receiver)?;
    Ok(crate::script_player::weapon_activity(
        &FrameWorld::from_world(world),
        id,
    ))
}

/// The box and contents `playerphysicstrace` traces with.
const PLAYER_BOX: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
const PLAYER_PHYSICS_MASK: u32 = 0x0082_0011;

/// gscm_playerlinkto / gscm_playerlinktodelta: the player linked to the
/// entity (at its tag), his view held within the arcs (180 each unless
/// given) and turning with it by the view fraction (none unless given);
/// the eighth argument takes the link's angles from the tag.
fn player_link(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    view: LinkView,
) -> Result<Value, String> {
    let not_entity = |_| "not an entity".to_owned();
    let id = super::super::engine::entity_id(world, receiver).map_err(not_entity)?;
    let parent = super::super::engine::entity_id(world, args.first().unwrap_or(&Value::Undefined))
        .map_err(not_entity)?;
    let client = world
        .resource::<RoundScript>()
        .player_client(id)
        .ok_or("not a player entity")?;
    let tag = match args.get(1) {
        None | Some(Value::Undefined) => None,
        Some(_) => Some(super::string(args, 1)?),
    }
    .filter(|tag| !tag.is_empty());
    let given = |index: usize, default: f32| -> Result<f32, String> {
        if args.len() > index {
            float(args, index)
        } else {
            Ok(default)
        }
    };
    let arc = |index| -> Result<f32, String> { Ok(given(index, 180.0)?.clamp(0.0, 180.0)) };
    let clamp = [-arc(5)?, arc(6)?, -arc(3)?, arc(4)?];
    let view_fraction = given(2, 0.0)?;
    let use_tag_angles = args.len() > 7 && int(args, 7)? != 0;
    if parent == id
        || tag.as_deref().is_some_and(|tag| {
            super::super::super::presence::tag_world(world, parent, tag).is_none()
        })
    {
        return Err("failed to link entity".into());
    }
    let (base, axis) =
        super::super::super::players::link_parent_pose(world, parent, tag.as_deref());
    let origin = FrameWorld::from_world(world)
        .player(ClientId(client))
        .map_or(base, |ps| ps.origin);
    let delta: [f32; 3] = std::array::from_fn(|i| origin[i] - base[i]);
    let local = std::array::from_fn(|i| (0..3).map(|j| delta[j] * axis[i][j]).sum());
    super::super::super::players::link_player(
        world,
        client,
        PlayerLink {
            parent,
            tag: tag.map(std::sync::Arc::from),
            origin: local,
            angles: [0.0; 3],
            view,
            clamp: Some(clamp),
            parent_axis: axis,
            view_fraction,
            use_tag_angles,
            link_axis: if use_tag_angles {
                axis
            } else {
                [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
            },
            restore_view: None,
        },
    );
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    // gscm_giveweapon: registers the weapon in the table when called during
    // level init, and equips a weapon with an offhand slot when the player
    // has none.
    registry.register(Namespace::Method, "giveweapon", |world, receiver, args| {
        let name = super::string(args, 0)?;
        super::weapon::weapon_index(world, &name)?;
        if let Some(variant) = super::optional_int(args, 1)?
            && !(0..=15).contains(&variant)
        {
            return Err(format!(
                "GiveWeapon called with an invalid model variant: {variant}, valid is 0-15"
            ));
        }
        let camo = (super::optional_int(args, 2)?.unwrap_or(0) & 127) as u8;
        let forwarded = [args[0].clone()];
        let (id, weapon) = super::super::player::give_weapon(world, receiver, &forwarded)?;
        if camo != 0 {
            crate::script_player::set_weapon_model(
                &mut FrameWorld::from_world(world),
                id,
                weapon,
                camo,
            );
        }
        crate::script_player::equip_offhand(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    use Namespace::Method;

    // gscm_setorigin: players only; one unit up, the teleport flag flipped.
    registry.register(Method, "setorigin", |world, receiver, args| {
        let id = client_or_error(world, receiver)?;
        let origin = vector(args, 0)?;
        if !super::super::super::players::place_unspawned(world, id.0, Some(origin), None) {
            crate::script_player::teleport(&mut FrameWorld::from_world(world), id, origin);
        }
        Ok(Value::Undefined)
    });
    // gscm_ismeleeing / isfiring / isusingoffhand read the weapon state by
    // its meaning (a blow; firing or a blow; an offhand).
    registry.register(Method, "ismeleeing", |world, receiver, _| {
        Ok(Value::Int(activity(world, receiver)?.meleeing.into()))
    });
    registry.register(Method, "isfiring", |world, receiver, _| {
        let now = activity(world, receiver)?;
        Ok(Value::Int((now.firing || now.meleeing).into()))
    });
    registry.register(Method, "isusingoffhand", |world, receiver, _| {
        Ok(Value::Int(
            activity(world, receiver)?.throwing_grenade.into(),
        ))
    });
    // gscm_getplayercamerapos: the player's eye.
    registry.register(Method, "getplayercamerapos", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        let eye = crate::script_player::eye(&FrameWorld::from_world(world), id)
            .ok_or("player has not spawned")?;
        Ok(Value::Vector(eye))
    });
    // The entity's number.
    registry.register(Method, "getentnum", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(
            world.resource::<RoundScript>().entities[&id].number,
        ))
    });
    registry.register(Method, "playerlinkto", |world, receiver, args| {
        player_link(world, receiver, args, LinkView::Free)
    });
    registry.register(Method, "playerlinktodelta", |world, receiver, args| {
        player_link(world, receiver, args, LinkView::Delta)
    });
    // gsc_playerphysicstrace: where a standing player's box stops on the
    // way, against the world and brush models.
    registry.register(
        Namespace::Function,
        "playerphysicstrace",
        |world, _, args| {
            let (start, end) = (vector(args, 0)?, vector(args, 1)?);
            let fraction = super::super::engine::trace(
                world,
                start,
                end,
                PLAYER_BOX.0,
                PLAYER_BOX.1,
                PLAYER_PHYSICS_MASK,
            )
            .fraction;
            Ok(Value::Vector(std::array::from_fn(|i| {
                (end[i] - start[i]) * fraction + start[i]
            })))
        },
    );
    // IW4L's own of the same rules (`docs/fidelity/t6.md`).
    let shared = NativeRegistry::engine_services();
    for name in ["setplayerangles", "setstance", "getxuid"] {
        if let Some(native) = shared.get(Method, name) {
            registry.register(Method, name, native);
        }
    }
    macro_rules! register_weapon_method {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let w_name = super::string(args, 0)?;
                super::weapon::weapon_index(world, &w_name)?;
                let shared = NativeRegistry::engine_services();
                let native = shared.get(Method, $name).expect("shared weapon method");
                native(world, receiver, args)
            });
        )*};
    }
    register_weapon_method!(
        "givestartammo",
        "givemaxammo",
        "switchtoweapon",
        "switchtoweaponimmediate",
        "hasweapon",
        "setweaponammoclip",
        "setweaponammostock",
        "getweaponammoclip",
        "getweaponammostock",
        "getammocount",
        "takeweapon",
    );
    let black_ops = NativeRegistry::engine_services().with_black_ops();
    if let Some(native) = black_ops.get(Method, "islinkedto") {
        registry.register(Method, "islinkedto", native);
    }
    register_rest(registry);
}

fn register_rest(registry: &mut NativeRegistry) {
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
        crate::game_move::restrict(
            &mut frame,
            id,
            game_api::movement::MoveRestriction::Jump,
            allowed,
        );
        let controls = &mut frame.client_meta_mut(id).controls;
        let was = !controls.jump_disabled;
        controls.jump_disabled = !allowed;
        Ok(Value::Int(was.into()))
    });
    use game_api::movement::MoveRestriction;
    registry.register(Method, "allowstand", |world, receiver, args| {
        allow_stance(
            world,
            receiver,
            args,
            |r| &mut r.stand,
            MoveRestriction::Stand,
        )
    });
    registry.register(Method, "allowcrouch", |world, receiver, args| {
        allow_stance(
            world,
            receiver,
            args,
            |r| &mut r.crouch,
            MoveRestriction::Crouch,
        )
    });
    registry.register(Method, "allowprone", |world, receiver, args| {
        allow_stance(
            world,
            receiver,
            args,
            |r| &mut r.prone,
            MoveRestriction::Prone,
        )
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
        move_status(world, receiver, |s| s.on_ladder)
    });
    registry.register(Method, "ismantling", |world, receiver, _| {
        move_status(world, receiver, |s| s.mantling)
    });
    // Sprinting and aiming down the sights: the shared allowances, through
    // the game's own movement when it has one. Melee: the handler only sets
    // the no-melee pm flag, which nothing in the executable reads, so the
    // player still knifes. Each answers the previous allowance.
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
                let mut frame = FrameWorld::from_world(world);
                crate::game_move::set_script_flag(
                    &mut frame,
                    id,
                    game_api::movement::ScriptFlag::NoMelee,
                    !allowed,
                );
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
        if let Ok(client) = player(world, receiver) {
            return Ok(Value::Vector(
                FrameWorld::from_world(world)
                    .player(ClientId(client))
                    .map_or([0.0; 3], |ps| ps.velocity),
            ));
        }
        let id = super::super::engine::entity_id(world, receiver)?;
        let number = world.resource::<RoundScript>().entities[&id].number;
        if let Some(proj) = FrameWorld::from_world(world).projectile_by_number(number) {
            return Ok(Value::Vector(proj.velocity));
        }
        if let Some(vel) = super::vehicle::velocity(world, id) {
            return Ok(Value::Vector(vel));
        }
        let now = world
            .get_resource::<crate::step::StepRequest>()
            .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS));
        let vel = world
            .get_resource::<crate::script::host::mechanics::Mechanics>()
            .map_or([0.0; 3], |m| m.velocity(id, now));
        Ok(Value::Vector(vel))
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
        let duration_ms = (seconds * 1000.0) as i32;
        if blur < 0.0 {
            return Err("Blur value must be greater than 0".into());
        }
        let client = player(world, receiver)
            .map_err(|_| "setblur() called on an invalid client entity.\n".to_owned())?;
        let now = world
            .get_resource::<crate::step::StepRequest>()
            .map_or(0, |r| crate::level_time_ms(r.tick));
        if let Some(mut frame) = FrameWorld::try_from_world(world)
            && frame.client_meta(ClientId(client)).is_some()
        {
            frame.client_meta_mut(ClientId(client)).view_effects.blur = Some(crate::ScriptBlur {
                from: blur,
                to: 0.0,
                set_ms: now,
                duration_ms: duration_ms + 50,
            });
        }
        Ok(Value::Undefined)
    });
    for name in [
        "actionslotonebuttonpressed",
        "actionslottwobuttonpressed",
        "actionslotthreebuttonpressed",
        "actionslotfourbuttonpressed",
    ] {
        registry.register(Method, name, |world, receiver, _| {
            client_or_error(world, receiver)?;
            Ok(Value::Int(0))
        });
    }
    registry.register(Method, "getperks", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        let perks: Vec<Value> = world
            .resource::<RoundScript>()
            .players
            .get(&id.0)
            .map_or_else(Vec::new, |slot| {
                slot.perks
                    .iter()
                    .map(|p| Value::String(p.clone().into()))
                    .collect()
            });
        super::super::super::arrays::new_array(world, perks)
    });
    registry.register(Method, "geteyeapprox", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let origin = match super::super::super::players::entity_field(world, id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        Ok(Value::Vector([origin[0], origin[1], origin[2] + 40.0]))
    });
    registry.register(Method, "getweaponforwarddir", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let angles = match world.resource::<RoundScript>().player_client(id) {
            Some(client) => FrameWorld::from_world(world)
                .player(ClientId(client))
                .map_or([0.0; 3], |ps| ps.viewangles),
            None => match super::super::super::players::entity_field(world, id, "angles") {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            },
        };
        let (forward, _, _) = math_iw4::angle_vectors(angles);
        Ok(Value::Vector(forward))
    });
    registry.register(Method, "setstowedweapon", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        super::string(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearstowedweapon", |world, receiver, _| {
        super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "applyknockback", |world, receiver, args| {
        let id = client_or_error(world, receiver)?;
        let damage = int(args, 0)?;
        if damage <= 0 {
            return Ok(Value::Undefined);
        }
        let dir = vector(args, 1)?;
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let inv = 1.0 / if -len >= 0.0 { 1.0 } else { len };
        let stance = crate::script_player::stance(&FrameWorld::from_world(world), id);
        let scale = match stance {
            "prone" => 0.02,
            "crouch" => 0.15,
            _ => 0.9,
        };
        let knockback = ((damage as f32 * scale) as i32).clamp(0, 60);
        if knockback > 0
            && let Some(ps) = FrameWorld::from_world(world).player_mut(id)
        {
            let push = 1000.0 * knockback as f32 * 0.004 * inv;
            for (velocity, d) in ps.velocity.iter_mut().zip(dir) {
                *velocity += d * push;
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableusability", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(id).is_some() {
            frame.client_meta_mut(id).controls.usability_disabled = true;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "playerlinkedoffsetenable", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        let linked = world
            .resource::<RoundScript>()
            .players
            .get(&id.0)
            .is_some_and(|slot| slot.link.is_some());
        if !linked {
            return Err("Player is not linked.".into());
        }
        Ok(Value::Undefined)
    });
    for name in ["getnormalizedmovement", "getnormalizedcameramovement"] {
        registry.register(Method, name, |world, receiver, _| {
            client_or_error(world, receiver)?;
            Ok(Value::Vector([0.0; 3]))
        });
    }
    registry.register(Method, "setburn", |world, receiver, args| {
        let seconds = float(args, 0)?;
        if seconds < 0.0 {
            return Err("Time must be positive".into());
        }
        let client = player(world, receiver)
            .map_err(|_| "setburn() called on an invalid client entity.\n".to_owned())?;
        // The client takes the time in whole msec.
        let ms = (seconds * 1000.0) as i32;
        super::flame_overlay::start(world, client, super::flame_overlay::Flame::Burn, ms);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setelectrified", |world, receiver, args| {
        let seconds = float(args, 0)?;
        if seconds < 0.0 {
            return Err("Time must be positive".into());
        }
        let client = player(world, receiver)
            .map_err(|_| "setelectrified() called on an invalid client entity.\n".to_owned())?;
        let ms = (seconds * 1000.0) as i32;
        super::flame_overlay::start(world, client, super::flame_overlay::Flame::Shock, ms);
        Ok(Value::Undefined)
    });
    registry.register(Method, "sprintuprequired", |world, receiver, _| {
        client_or_error(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isempjammed", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)
            .map_err(|_| "not an entity".to_owned())?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            return Err("IsEMPJammed: Entity must be a player entity".into());
        };
        let jammed = world
            .get_resource::<EmpJammed>()
            .is_some_and(|j| j.0.contains(&client));
        Ok(Value::Int(jammed.into()))
    });
    registry.register(Method, "setempjammed", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)
            .map_err(|_| "not an entity".to_owned())?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            return Err("SetEMPJammed: Entity must be a player entity".into());
        };
        let on = int(args, 0)? != 0;
        let has_immune = world
            .resource::<RoundScript>()
            .players
            .get(&client)
            .is_some_and(|s| s.perks.iter().any(|p| p.as_ref() == "specialty_immuneemp"));
        let mut jammed = world.get_resource_or_insert_with(EmpJammed::default);
        if on && !has_immune {
            jammed.0.insert(client);
        } else {
            jammed.0.remove(&client);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "turnedhuman", |world, receiver, _| {
        client_or_error(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "giveachievement", |world, receiver, args| {
        let name = super::string(args, 0)?;
        if name.is_empty() {
            return Err("giveachievement: achievement must be named".into());
        }
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            return Err("giveachievement: entity must be a player entity".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setclientflag", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let index = int(args, 0)?;
        if !(0..=15).contains(&index) {
            return Err(format!(
                "SetClientFlag: Index {index} out of range (0 - 15)\n"
            ));
        }
        *world
            .get_resource_or_insert_with(ClientFlags::default)
            .0
            .entry(id)
            .or_default() |= 1 << index;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearclientflag", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let index = int(args, 0)?;
        if !(0..=15).contains(&index) {
            return Err(format!(
                "ClearClientFlag: Index {index} out of range (0 - 15)\n"
            ));
        }
        *world
            .get_resource_or_insert_with(ClientFlags::default)
            .0
            .entry(id)
            .or_default() &= !(1 << index);
        Ok(Value::Undefined)
    });
    registry.register(Method, "isinmovemode", |world, receiver, args| {
        client_or_error(world, receiver)?;
        for i in 0..args.len() {
            super::string(args, i)?;
        }
        Ok(Value::Int(0))
    });
    registry.register(Method, "isremotecontrolling", |world, receiver, _| {
        client_or_error(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "islookingat", |world, receiver, args| {
        client_or_error(world, receiver)?;
        super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "needsrevive", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "relinktoturret", |world, receiver, args| {
        client_or_error(world, receiver)?;
        super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "ishostforbots", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        Ok(Value::Int((id.0 == 0).into()))
    });
    registry.register(Method, "isplayeronsamemachine", |world, receiver, args| {
        let id = client_or_error(world, receiver)?;
        let other = super::arg(args, 0)?;
        let other_id = super::super::engine::entity_id(world, other)
            .ok()
            .and_then(|eid| world.resource::<RoundScript>().player_client(eid))
            .ok_or("IsPlayerOnSameMachine: Player must be passed in.")?;
        Ok(Value::Int((id.0 == other_id).into()))
    });
    registry.register(Method, "hasdlcavailable", |world, receiver, args| {
        client_or_error(world, receiver)?;
        super::string(args, 0)?;
        Ok(Value::Int(1))
    });
    registry.register(Namespace::Function, "hasdlcavailable", |_, _, _| {
        Ok(Value::Int(1))
    });
    registry.register(Method, "isgodmode", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        Ok(Value::Int(is_invulnerable(world, id.0).into()))
    });
    registry.register(Namespace::Function, "isgodmode", |world, _, args| {
        let id = client_or_error(world, super::arg(args, 0)?)?;
        Ok(Value::Int(is_invulnerable(world, id.0).into()))
    });
    registry.register(Method, "setentertime", |world, receiver, args| {
        client_or_error(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setclientdrawtalk", |world, receiver, args| {
        client_or_error(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "enabledeathstreak", |world, receiver, _| {
        client_or_error(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "buttonpressed", |world, receiver, args| {
        client_or_error(world, receiver)?;
        super::string(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(
        Method,
        "directionalhitindicator",
        |world, receiver, args| {
            super::super::engine::entity_id(world, receiver)?;
            int(args, 0)?;
            if args.len() == 2 {
                int(args, 1)?;
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "getloadoutitem", |world, receiver, args| {
        client_or_error(world, receiver)?;
        int(args, 0)?;
        super::string(args, 1)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "isbonuscardactive", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        let card = int(args, 0)?;
        let class = int(args, 1)?;
        if !(0..=7).contains(&card) {
            return Err("AddBonusCardStat: Invalid bonuscard.".into());
        }
        if !(0..=14).contains(&class) {
            return Err("AddBonusCardStat: Invalid loadoutClass.".into());
        }
        Ok(Value::Int(0))
    });
    registry.register(Method, "addbonuscardstat", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().player_client(id).is_none() {
            return Err("AddBonusCardStat: entity must be a player entity".into());
        }
        let card = int(args, 0)?;
        if !(0..=7).contains(&card) {
            return Err(format!("AddBonusCardStat: Invalid bonuscard {card}."));
        }
        super::string(args, 1)?;
        int(args, 2)?;
        let class = int(args, 3)?;
        if !(0..=14).contains(&class) {
            return Err(format!("AddBonusCardStat: Invalid loadoutClass {class}."));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "openmenu", |world, receiver, args| {
        client_or_error(world, receiver)?;
        super::string(args, 0)?;
        Ok(Value::Int(1))
    });
    registry.register(Method, "suicide", |world, receiver, _| {
        let id = client_or_error(world, receiver)?;
        let tick = world.resource::<crate::step::StepRequest>().tick;
        crate::script::force_death(world, tick, id.0);
        Ok(Value::Undefined)
    });
    registry.register(Method, "cloneplayer", |world, receiver, args| {
        let id = client_or_error(world, receiver)?;
        int(args, 0)?;
        let (origin, angles) = FrameWorld::from_world(world)
            .player(id)
            .map_or(([0.0; 3], [0.0; 3]), |ps| (ps.origin, ps.viewangles));
        let mut runtime = world.resource_mut::<RoundScript>();
        let corpse = runtime.create_entity(
            crate::script::host::entities::EntityKind::Corpse {
                slot: 0,
                anim: None,
            },
            "bodyque",
        )?;
        runtime.set_object_field(corpse, "origin", Value::Vector(origin));
        runtime.set_object_field(corpse, "angles", Value::Vector(angles));
        Ok(Value::Object(corpse))
    });
}

#[derive(Resource, Default)]
struct EmpJammed(std::collections::BTreeSet<u32>);

#[derive(Resource, Default)]
struct ClientFlags(BTreeMap<u64, u32>);

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
/// gscm_isonladder and gscm_ismantling: the ladder and mantle pm flags,
/// which `movement_t6` keeps.
fn move_status(
    world: &mut World,
    receiver: &Value,
    flag: fn(&game_api::movement::MoveStatus) -> bool,
) -> Result<Value, String> {
    let id = ClientId(
        player(world, receiver)
            .map_err(|_| "setstat: entity must be a player entity".to_owned())?,
    );
    Ok(Value::Int(
        crate::game_move::status(&FrameWorld::from_world(world), id)
            .is_some_and(|status| flag(&status))
            .into(),
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
