//! Black Ops multiplayer's `obituary`: the kill feed's event, carrying the
//! victim, the attacker and either a means of death or the weapon.

use super::super::args::string;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};

/// Black Ops' means of death, in its order.
const MEANS: [&str; 21] = [
    "MOD_UNKNOWN",
    "MOD_PISTOL_BULLET",
    "MOD_RIFLE_BULLET",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_PROJECTILE",
    "MOD_PROJECTILE_SPLASH",
    "MOD_MELEE",
    "MOD_BAYONET",
    "MOD_HEAD_SHOT",
    "MOD_CRUSH",
    "MOD_TELEFRAG",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TRIGGER_HURT",
    "MOD_EXPLOSIVE",
    "MOD_IMPACT",
    "MOD_BURNED",
    "MOD_HIT_BY_OBJECT",
    "MOD_DROWN",
    "MOD_GAS",
];

const MOD_MELEE: usize = 7;
const MOD_HEAD_SHOT: usize = 9;
const MOD_CRUSH: usize = 10;
const MOD_FALLING: usize = 12;
const MOD_SUICIDE: usize = 13;
const MOD_IMPACT: usize = 16;
/// The impact type of a weapon whose impact kill shows the weapon.
const IMPACT_TYPE_WEAPON_ICON: i32 = 15;

/// The client number of an entity argument, `None` for an entity that is
/// not a player, an error for anything else.
fn client(world: &World, value: &Value) -> Result<Option<ClientId>, String> {
    let id = super::engine::entity_id(world, value)?;
    Ok(world
        .resource::<RoundScript>()
        .player_client(id)
        .map(ClientId))
}

/// `t5.mp.wager_popup` has been logged.
#[derive(Resource, Default)]
struct WagerPopupLogged;

const MEDAL_TABLE: &str = "mp/medaltable.csv";

/// `displayMedal(index, teambased, xpscale, sound)`: the client finds the
/// medal's name and description by index and puts up its popup, with the
/// points it is worth (column 4 in a team game, 5 otherwise, scaled).
fn display_medal(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = super::player::player(world, receiver)?;
    let index = super::super::args::int(args, 0)?;
    let team_based = super::super::args::int(args, 1)? != 0;
    let scale = super::super::args::float(args, 2)?;
    let sound = match args.get(3) {
        Some(Value::String(sound)) => sound.to_string(),
        _ => String::new(),
    };
    let lines = {
        let runtime = world.resource::<crate::script::RoundScript>();
        let table = crate::script::host::tables::table(&runtime.tables, MEDAL_TABLE)
            .ok_or_else(|| format!("{MEDAL_TABLE} is not loaded"))?;
        let Some(row) = crate::script::host::tables::table_search(table, 0, &index.to_string())
        else {
            return Ok(Value::Undefined);
        };
        let cell = |column: usize| table.cell(row, column).unwrap_or("").to_owned();
        let points = super::iw4::atoi(&cell(if team_based { 4 } else { 5 })) as f32 * scale;
        [cell(2), cell(3), format!("+{}", points as i32)]
    };
    super::player::send_menu_command(
        world,
        client,
        crate::MenuCommandKind::PopUp {
            item: "MedalPopUp".to_owned(),
            lines,
            sound,
        },
    );
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "displaymedal", display_medal);
    // The client plays a wager popup from a server command, through the
    // HUD's popup menu; neither is ported.
    registry.register(Method, "displaywagerpopup", |world, receiver, args| {
        if !world.contains_resource::<WagerPopupLogged>() {
            world.insert_resource(WagerPopupLogged);
            diag::warn!(
                Sim,
                "t5.mp.wager_popup: a wager popup does not reach the client"
            );
        }
        super::player::present(world, receiver, "displaywagerpopup", args)
    });
    registry.register(Function, "obituary", |world, _, args| {
        let victim = client(world, args.first().unwrap_or(&Value::Undefined))?;
        let attacker = args
            .get(1)
            .and_then(|value| client(world, value).ok())
            .flatten();
        let name = string(args, 2)?;
        let weapon = crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)?;
        let means_name = string(args, 3)?;
        let means = MEANS
            .iter()
            .position(|m| m.eq_ignore_ascii_case(&means_name))
            .ok_or_else(|| format!("Unknown means of death \"{means_name}\"\n"))?;
        let tick = super::player::tick(world);
        let mut frame = FrameWorld::from_world(world);
        let shows_means = match means {
            MOD_IMPACT => frame
                .combat_facts_for(weapon)
                .is_none_or(|facts| facts.impact_type != IMPACT_TYPE_WEAPON_ICON),
            MOD_MELEE | MOD_HEAD_SHOT | MOD_SUICIDE | MOD_FALLING | MOD_CRUSH => true,
            _ => false,
        };
        let victim = victim.map_or(-1, |id| id.0 as i32);
        crate::script_player::push_obituary(
            &mut frame,
            tick,
            crate::EntityEventPayload {
                number: victim,
                other_entity_num: victim,
                attacker_entity_num: attacker.map_or(-1, |id| id.0 as i32),
                event_parm: if shows_means {
                    -1 - means as i32
                } else {
                    weapon as i32
                },
                weapon,
                ..Default::default()
            },
        );
        Ok(Value::Undefined)
    });
}
