//! Black Ops 2's weapon table: each weapon's index is the order the engine
//! registered it in, from the stats table at level start and then by the
//! builtins scripts call before their first wait.

use super::super::super::arrays::new_array;
use super::{int, string};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use crate::script_player;
use asset_core::ZoneWeapon;
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The stats table's item groups, by number.
const ITEM_GROUPS: [&str; 20] = [
    "weapon_smg",
    "weapon_assault",
    "weapon_cqb",
    "weapon_lmg",
    "weapon_sniper",
    "weapon_pistol",
    "weapon_launcher",
    "weapon_special",
    "weapon_shotgun",
    "weapon_explosive",
    "weapon_grenade",
    "weapon_masterkey",
    "weapon_grenadelauncher",
    "weapon_flamethrower",
    "specialty",
    "specialgrenade",
    "miscweapon",
    "feature",
    "bonuscard",
    "killstreak",
];
/// The groups whose items load as weapons.
const WEAPON_GROUPS: [usize; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15];
const STATS_TABLE: &str = "zm/zm_statstable.csv";
/// The newest content pack the owner's game had; a later item does not load.
const OWNED_CONTENT: i32 = 13;

#[derive(Resource)]
struct WeaponTable {
    /// By index; index 0 is no weapon.
    weapons: Vec<Option<Arc<ZoneWeapon>>>,
    by_name: BTreeMap<String, usize>,
}

/// C's `atoi`: leading blanks, a sign, then digits up to the first other byte.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

pub(super) fn zone_weapon(world: &World, name: &str) -> Option<Arc<ZoneWeapon>> {
    world
        .resource::<RoundScript>()
        .restart
        .as_ref()?
        .weapons
        .get(&name.to_ascii_lowercase())
        .cloned()
}

fn find(world: &World, name: &str) -> usize {
    world
        .get_resource::<WeaponTable>()
        .and_then(|table| table.by_name.get(&name.to_ascii_lowercase()).copied())
        .unwrap_or(0)
}

/// Finds a weapon, registering it and the weapons that come with it when the
/// zones have it; 0 when they do not.
fn register(world: &mut World, name: &str) -> Result<usize, String> {
    if name.is_empty() || name.eq_ignore_ascii_case("none") {
        return Ok(0);
    }
    let found = find(world, name);
    if found != 0 {
        return Ok(found);
    }
    let Some(weapon) = zone_weapon(world, name) else {
        return Ok(0);
    };
    let mut table = world.resource_mut::<WeaponTable>();
    let index = table.weapons.len();
    if index > usize::from(u8::MAX) {
        return Err(format!("t6.weapons.table_full: no index left for '{name}'"));
    }
    table.weapons.push(Some(Arc::clone(&weapon)));
    table.by_name.insert(name.to_ascii_lowercase(), index);
    for companion in &weapon.companions {
        if register(world, companion)? == 0 {
            return Err(format!(
                "weapon '{companion}' that '{name}' names is in no zone"
            ));
        }
    }
    Ok(index)
}

struct Item<'t> {
    index: usize,
    group: Option<usize>,
    order: u8,
    reference: &'t str,
    name: &'t str,
    content: &'t str,
    attachments: &'t str,
}

/// The stats table's weapons, group by group, each by its order column then
/// its item number, as `<name>_zm`, with the attachments its item lists.
fn table_weapons(world: &World) -> Vec<(String, Vec<String>)> {
    let tables = world.resource::<RoundScript>().tables.clone();
    let Some(table) = crate::script::host::tables::table(&tables, STATS_TABLE) else {
        return Vec::new();
    };
    let cell = |row, column| table.cell(row, column).unwrap_or("");
    let items: Vec<Item> = (0..256)
        .filter_map(|index: usize| {
            let key = index.to_string();
            let row = (0..table.rows).find(|&row| cell(row, 0) == key)?;
            let (reference, name) = (cell(row, 3), cell(row, 4));
            (!reference.is_empty() && !name.is_empty()).then(|| Item {
                index,
                group: ITEM_GROUPS
                    .iter()
                    .position(|group| group.eq_ignore_ascii_case(cell(row, 2))),
                order: atoi(cell(row, 9)) as u8,
                reference,
                name,
                content: cell(row, 18),
                attachments: cell(row, 8),
            })
        })
        .collect();
    let none = items
        .iter()
        .find(|item| item.reference.eq_ignore_ascii_case("weapon_null"))
        .map(|item| item.index);
    let mut names = Vec::new();
    for group in WEAPON_GROUPS {
        let mut listed: Vec<&Item> = items.iter().filter(|i| i.group == Some(group)).collect();
        listed.sort_by_key(|item| (item.order, item.index));
        for item in listed {
            if Some(item.index) == none {
                continue;
            }
            if !item.content.is_empty() {
                let pack = atoi(item.content);
                if pack != -1 && pack > OWNED_CONTENT {
                    continue;
                }
            }
            let attachments = item.attachments.split_whitespace();
            let variants = attachments
                .clone()
                .flat_map(|first| {
                    let one = format!("{}_{first}_zm", item.name);
                    let two = attachments
                        .clone()
                        .map(move |second| format!("{}_{first}_{second}_zm", item.name));
                    std::iter::once(one).chain(two)
                })
                .collect();
            names.push((format!("{}_zm", item.name), variants));
        }
    }
    names
}

/// The table as level start leaves it: the default weapon, then the stats
/// table's weapons.
fn table(world: &mut World) -> Result<(), String> {
    if world.contains_resource::<WeaponTable>() {
        return Ok(());
    }
    world.insert_resource(WeaponTable {
        weapons: vec![None],
        by_name: BTreeMap::new(),
    });
    register(world, "defaultweapon_mp")?;
    // `g_connectpaths` is 0 at a normal start, so the stats table loads.
    for (name, variants) in table_weapons(world) {
        if register(world, &name)? == 0 {
            continue;
        }
        if let Some(variant) = variants.iter().find(|v| zone_weapon(world, v).is_some()) {
            return Err(format!(
                "t6.weapons.table_variants: '{name}' has the attachment variant '{variant}'"
            ));
        }
    }
    Ok(())
}

/// The weapon a builtin names: registered as well before the level's first
/// wait, only found after it.
pub(super) fn weapon_index(world: &mut World, name: &str) -> Result<usize, String> {
    table(world)?;
    if name.contains('+') {
        return Err(format!(
            "t6.weapons.attachments: weapon '{name}' names attachments"
        ));
    }
    if world.resource::<RoundScript>().loading {
        register(world, name)
    } else {
        Ok(find(world, name))
    }
}

/// The weapon a script's index names, as the engine reads it: the low byte,
/// none for 0 and 255.
fn indexed(world: &mut World, args: &[Value]) -> Result<Option<Arc<ZoneWeapon>>, String> {
    table(world)?;
    let index = int(args, 0)? as u8;
    if index == 0 || index == u8::MAX {
        return Ok(None);
    }
    Ok(world
        .resource::<WeaponTable>()
        .weapons
        .get(usize::from(index))
        .cloned()
        .flatten())
}

fn sound(
    world: &mut World,
    args: &[Value],
    field: fn(&ZoneWeapon) -> &str,
) -> Result<Value, String> {
    let text = indexed(world, args)?.map_or(String::new(), |weapon| field(&weapon).to_owned());
    Ok(Value::String(text.into()))
}

/// The game's names for a weapon's type, class and inventory type, by number.
const WEAP_TYPES: [&str; 9] = [
    "bullet",
    "grenade",
    "projectile",
    "binoculars",
    "gas",
    "bomb",
    "mine",
    "melee",
    "riotshield",
];
const WEAP_CLASSES: [&str; 14] = [
    "rifle",
    "mg",
    "smg",
    "spread",
    "pistol",
    "grenade",
    "rocketlauncher",
    "turret",
    "non-player",
    "gas",
    "item",
    "melee",
    "Killstreak Alt Stored Weapon",
    "pistol spread",
];
const INVENTORY_TYPES: [&str; 6] = [
    "primary",
    "offhand",
    "item",
    "altmode",
    "melee",
    "dwlefthand",
];
const WEAP_TYPE_GAS: i32 = 4;
const INVENTORY_PRIMARY: i32 = 0;
const FIRE_SINGLE_SHOT: i32 = 1;
const OFFHAND_EQUIPMENT: i32 = 3;
const OFFHAND_SPECIFIC_USE: i32 = 4;
/// `player_clipSizeMultiplier` as the engine registers it.
const CLIP_SIZE_MULTIPLIER: f32 = 1.0;

/// The weapon a builtin's first argument names; an unknown one is the
/// engine's script error.
fn named(world: &mut World, args: &[Value]) -> Result<Arc<ZoneWeapon>, String> {
    let name = string(args, 0)?;
    let index = weapon_index(world, &name)? & 0xff;
    let weapon = world
        .resource::<WeaponTable>()
        .weapons
        .get(index)
        .cloned()
        .flatten();
    match weapon {
        Some(weapon) if index != 0 => Ok(weapon),
        _ if name.eq_ignore_ascii_case("none") => {
            Err(format!("Weapon name \"{name}\" is not valid.\n"))
        }
        _ => Err(format!(
            "Unknown weapon name \"{name}\": script may need to call PreCacheItem(\"{name}\") during level init.\n"
        )),
    }
}

/// The weapon a builtin names, or none for an unknown name, which those
/// builtins answer quietly.
fn named_or_none(world: &mut World, args: &[Value]) -> Result<Option<Arc<ZoneWeapon>>, String> {
    let name = string(args, 0)?;
    let index = weapon_index(world, &name)? & 0xff;
    Ok(world
        .resource::<WeaponTable>()
        .weapons
        .get(index)
        .cloned()
        .flatten()
        .filter(|_| index != 0))
}

/// A clip as the engine sizes it: scaled by `player_clipSizeMultiplier`,
/// rounded, never below one; a one-round clip stays one.
fn clip_size(world: &World, weapon: &ZoneWeapon) -> i32 {
    let scale = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("player_clipsizemultiplier")
        .and_then(|value| value.trim().parse::<f32>().ok())
        .unwrap_or(CLIP_SIZE_MULTIPLIER);
    if weapon.clip_size > 1 {
        ((weapon.clip_size as f32 * scale + 9.313_226e-10).round_ties_even() as i32).max(1)
    } else {
        weapon.clip_size
    }
}

/// Start or max ammo: rounds, or clips when the weapon counts in clips.
fn ammo(world: &World, weapon: &ZoneWeapon, count: i32) -> i32 {
    if weapon.ammo_count_clip_relative {
        clip_size(world, weapon) * count
    } else {
        count
    }
}

fn enum_name(names: &[&str], value: i32) -> Value {
    Value::string(
        usize::try_from(value)
            .ok()
            .and_then(|i| names.get(i))
            .unwrap_or(&""),
    )
}

fn other_weapon(world: &mut World, name: &str) -> Result<Value, String> {
    let index = if name.is_empty() {
        0
    } else {
        weapon_index(world, name)?
    };
    Ok(Value::string(if index == 0 { "none" } else { name }))
}

/// The names of the weapons with a flag, by index; the engine stops one short
/// of the last weapon registered.
fn flagged(world: &mut World, flag: fn(&ZoneWeapon) -> bool) -> Result<Value, String> {
    table(world)?;
    let weapons = &world.resource::<WeaponTable>().weapons;
    let last = weapons.len() - 1;
    let names: Vec<Value> = weapons[..last]
        .iter()
        .flatten()
        .filter(|weapon| flag(weapon))
        .map(|weapon| Value::String(weapon.name.as_str().into()))
        .collect();
    new_array(world, names)
}

/// The client of a player entity, with the engine's object errors.
fn player_entity(world: &World, receiver: &Value) -> Result<crate::ClientId, String> {
    let runtime = world.resource::<RoundScript>();
    let (id, entity) = runtime.entity(receiver).ok_or("not an entity")?;
    runtime
        .player_client(id)
        .map(crate::ClientId)
        .ok_or_else(|| format!("entity {} is not a player", entity.number))
}

/// `getfractionmaxammo`: a clip-only weapon's clip against its size, else
/// its stock against its max; 1 for a weapon the player lacks or that holds
/// none.
fn fraction_max_ammo(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = player_entity(world, receiver)?;
    let name = string(args, 0)?;
    let weapon = named_or_none(world, args)?;
    let Some(weapon) = weapon else {
        return Ok(Value::Float(1.0));
    };
    let count = {
        let frame = FrameWorld::from_world(world);
        let held = script_player::weapon_named(&frame, &name)
            .ok()
            .filter(|&held| script_player::has_weapon(&frame, client, held));
        let Some(held) = held else {
            return Ok(Value::Float(1.0));
        };
        if weapon.clip_only {
            script_player::ammo_clip(&frame, client, held)
        } else {
            script_player::ammo_stock(&frame, client, held)
        }
    };
    let max = if weapon.clip_only {
        clip_size(world, &weapon)
    } else {
        ammo(world, &weapon, weapon.max_ammo)
    };
    Ok(Value::Float(match (count, max) {
        (_, ..1) => 1.0,
        (1.., _) => count as f32 / max as f32,
        _ => 0.0,
    }))
}

/// `getfractionstartammo`: total ammo against start ammo; 1 when the player
/// does not hold the weapon or its start ammo is under 1.
fn fraction_start_ammo(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let client = player_entity(world, receiver)?;
    let name = string(args, 0)?;
    let Some(weapon) = named_or_none(world, args)? else {
        return Ok(Value::Float(1.0));
    };
    let count = {
        let frame = FrameWorld::from_world(world);
        let held = script_player::weapon_named(&frame, &name)
            .ok()
            .filter(|&held| script_player::has_weapon(&frame, client, held));
        let Some(held) = held else {
            return Ok(Value::Float(1.0));
        };
        script_player::ammo_clip(&frame, client, held)
            + script_player::ammo_stock(&frame, client, held)
    };
    let start = ammo(world, &weapon, weapon.start_ammo);
    Ok(Value::Float(match (count, start) {
        (_, ..1) => 1.0,
        (1.., _) => count as f32 / start as f32,
        _ => 0.0,
    }))
}

#[derive(Resource, Default)]
struct FuelAmmo(BTreeMap<(u32, String), i32>);

#[derive(Resource, Default)]
struct WeaponSpin(BTreeMap<u32, f32>);

#[derive(Resource, Default)]
struct WeaponHeat(BTreeMap<(u32, String), (f32, bool)>);

fn add_heat(world: &mut World, client: u32, def: &ZoneWeapon, delta: f32) {
    let key = (client, def.name.to_ascii_lowercase());
    let mut table = world.get_resource_or_insert_with(WeaponHeat::default);
    let (heat, overheating) = table.0.entry(key).or_insert((0.0, false));
    *heat += delta;
    if *heat >= 100.0 {
        *heat = 100.0;
        *overheating = true;
    }
    if *heat <= 0.0 && *heat != 0.0 {
        *heat = 0.0;
    }
    if *heat <= def.overheat_end_val || !def.overheat_weapon {
        *overheating = false;
    }
}

fn is_overheating(world: &mut World, client: u32, def: &ZoneWeapon) -> bool {
    if !def.overheat_weapon {
        return false;
    }
    if let Some(frame) = FrameWorld::try_from_world(world)
        && let Ok(weapon) = script_player::weapon_named(&frame, &def.name)
        && let Some((_, over)) =
            crate::game_move::weapon_heat(&frame, crate::ClientId(client), weapon)
    {
        return over;
    }
    world
        .get_resource::<WeaponHeat>()
        .and_then(|h| h.0.get(&(client, def.name.to_ascii_lowercase())).copied())
        .is_some_and(|(heat, over)| over || heat >= 100.0)
}

fn heat_value(world: &mut World, client: u32, def: &ZoneWeapon) -> f32 {
    if !def.overheat_weapon {
        return 0.0;
    }
    if let Some(frame) = FrameWorld::try_from_world(world)
        && let Ok(weapon) = script_player::weapon_named(&frame, &def.name)
        && let Some((heat, _)) =
            crate::game_move::weapon_heat(&frame, crate::ClientId(client), weapon)
    {
        return heat;
    }
    world
        .get_resource::<WeaponHeat>()
        .and_then(|h| h.0.get(&(client, def.name.to_ascii_lowercase())).copied())
        .map_or(0.0, |(heat, _)| heat)
}

fn current_or_named_weapon(
    world: &mut World,
    client: crate::ClientId,
    args: &[Value],
    name_idx: usize,
) -> Result<Option<Arc<ZoneWeapon>>, String> {
    if args.len() > name_idx {
        let name = string(args, name_idx)?;
        let index = weapon_index(world, &name)? & 0xff;
        return Ok(world
            .resource::<WeaponTable>()
            .weapons
            .get(index)
            .cloned()
            .flatten()
            .filter(|_| index != 0));
    }
    let name = {
        let frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player(client) else {
            return Ok(None);
        };
        if ps.weapon == 0 {
            return Ok(None);
        }
        script_player::weapon_name(&frame, ps.weapon).to_owned()
    };
    Ok(zone_weapon(world, &name))
}

/// Advances weapon spin and weapon heat one server tick.
fn tick(world: &mut World) {
    let dt_ms = crate::MATCH_TICK_MS as f32;
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        let cid = crate::ClientId(client);
        let (weapon_id, state, held_ids, attack) = {
            let mut frame = FrameWorld::from_world(world);
            let Some(ps) = frame.player(cid).copied() else {
                continue;
            };
            let attack =
                script_player::holds_now_or_last(&mut frame, cid, script_player::CmdButton::Attack);
            let held: Vec<u32> = ps
                .weapons
                .iter()
                .copied()
                .filter(|&w| w > 0)
                .map(|w| w as u32)
                .collect();
            (ps.weapon, ps.weaponstate_primary, held, attack)
        };
        let current_name = (weapon_id != 0).then(|| {
            let frame = FrameWorld::from_world(world);
            script_player::weapon_name(&frame, weapon_id).to_owned()
        });
        let current_def = current_name.as_deref().and_then(|n| zone_weapon(world, n));
        let spin_before = world
            .get_resource::<WeaponSpin>()
            .and_then(|s| s.0.get(&client).copied())
            .unwrap_or(0.0);
        let spin_after = match current_def.as_deref() {
            Some(def) if def.fire_type == 7 => {
                let (dir, time) = if attack {
                    (1.0, def.spin_up_time)
                } else {
                    (-1.0, def.spin_down_time)
                };
                let step = if time > 0 {
                    dt_ms * (dir / time as f32)
                } else {
                    0.0
                };
                (spin_before + step).clamp(0.0, 1.0)
            }
            Some(def) if def.fire_type == 9 => {
                let rate = if matches!(state, 1..=5) {
                    0.0
                } else if def.spin_up_time > 0 {
                    if attack && !is_overheating(world, client, def) {
                        1.0 / def.spin_up_time as f32
                    } else if def.spin_down_time > 0 {
                        -1.0 / def.spin_down_time as f32
                    } else {
                        -0.5
                    }
                } else if attack && !is_overheating(world, client, def) {
                    0.5
                } else {
                    -0.5
                };
                (spin_before + dt_ms * -1.0 * rate).clamp(-1.0, 0.0)
            }
            _ => 0.0,
        };
        world
            .get_resource_or_insert_with(WeaponSpin::default)
            .0
            .insert(client, spin_after);

        for held_id in held_ids {
            let name = {
                let frame = FrameWorld::from_world(world);
                script_player::weapon_name(&frame, held_id).to_owned()
            };
            let Some(def) = zone_weapon(world, &name) else {
                continue;
            };
            if !def.overheat_weapon {
                continue;
            }
            let firing = held_id == weapon_id
                && (matches!(state, 6 | 0x28)
                    || (def.fire_type == 9 && attack && !is_overheating(world, client, &def)));
            if firing {
                add_heat(world, client, &def, dt_ms * def.overheat_rate * 0.001);
            }
            if !firing || def.cool_while_firing {
                add_heat(world, client, &def, -(dt_ms * def.cooldown_rate * 0.001));
            }
        }
    }
}

pub(super) fn register_natives(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.on_tick(tick);
    registry.register(Method, "getfractionmaxammo", fraction_max_ammo);
    registry.register(Method, "getfractionstartammo", fraction_start_ammo);
    registry.register(Method, "getweaponammofuel", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let name = string(args, 0)?;
        let Some(weapon) = named_or_none(world, args)? else {
            return Ok(Value::Int(0));
        };
        if weapon.weap_type != WEAP_TYPE_GAS {
            return Ok(Value::Int(0));
        }
        let held = {
            let frame = FrameWorld::from_world(world);
            script_player::weapon_named(&frame, &name)
                .ok()
                .is_some_and(|w| script_player::has_weapon(&frame, client, w))
        };
        if !held {
            return Ok(Value::Int(0));
        }
        let key = (client.0, weapon.name.to_ascii_lowercase());
        let fuel = world
            .get_resource::<FuelAmmo>()
            .and_then(|f| f.0.get(&key).copied())
            .unwrap_or(weapon.tank_life_time);
        Ok(Value::Int(fuel))
    });
    registry.register(Method, "setweaponammofuel", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let name = string(args, 0)?;
        let amount = int(args, 1)?;
        let Some(weapon) = named_or_none(world, args)? else {
            return Ok(Value::Int(0));
        };
        if weapon.weap_type != WEAP_TYPE_GAS {
            return Ok(Value::Int(0));
        }
        let held = {
            let frame = FrameWorld::from_world(world);
            script_player::weapon_named(&frame, &name)
                .ok()
                .is_some_and(|w| script_player::has_weapon(&frame, client, w))
        };
        if !held {
            return Err(format!(
                "Trying to give player ammo for weapon {} that they are not holding",
                weapon.name
            ));
        }
        let clamped = amount.clamp(0, weapon.tank_life_time.max(0));
        world
            .get_resource_or_insert_with(FuelAmmo::default)
            .0
            .insert((client.0, weapon.name.to_ascii_lowercase()), clamped);
        Ok(Value::Undefined)
    });
    registry.register(Method, "anyammoforweaponmodes", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let weapon = named(world, args)?;
        let frame = FrameWorld::from_world(world);
        let count_for = |w_name: &str| -> i32 {
            let Ok(w) = script_player::weapon_named(&frame, w_name) else {
                return 0;
            };
            script_player::ammo_clip(&frame, client, w)
                + script_player::ammo_stock(&frame, client, w)
        };
        let mut total = count_for(&weapon.name);
        if !weapon.alt_weapon.is_empty() && !weapon.alt_weapon.eq_ignore_ascii_case("none") {
            total += count_for(&weapon.alt_weapon);
        }
        Ok(Value::Int((total != 0).into()))
    });
    registry.register(Function, "getweapondisplayname", |world, _, args| {
        let Some(weapon) = named_or_none(world, args)? else {
            return Ok(Value::string(""));
        };
        Ok(Value::String(weapon.display_name.as_str().into()))
    });
    registry.register(Function, "getweaponstowedmodel", |world, _, args| {
        let Some(weapon) = named_or_none(world, args)? else {
            return Ok(Value::Int(0));
        };
        let has = weapon.world_models.get(1).is_some_and(|m| !m.is_empty())
            && weapon.weap_class != 5
            && weapon.weap_class != 12;
        Ok(Value::Int(has.into()))
    });
    registry.register(Function, "weaponhasattachment", |world, _, args| {
        let _weapon = named(world, args)?;
        for i in 1..args.len() {
            string(args, i)?;
        }
        Ok(Value::Int(0))
    });
    registry.register(
        Function,
        "getweaponsupportedattachments",
        |world, _, args| {
            let _weapon = named(world, args)?;
            new_array(world, Vec::new())
        },
    );
    registry.register(Method, "itemweaponsetammo", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if !matches!(
            world.resource::<RoundScript>().entities[&id].kind,
            crate::script::host::entities::EntityKind::Item(_)
        ) {
            return Err("Entity is not an item.".into());
        }
        if int(args, 0)? < 0 {
            return Err("Ammo count must not be negative".into());
        }
        if int(args, 1)? < 0 {
            return Err("Ammo count must not be negative".into());
        }
        if args.len() > 2 {
            let mode = int(args, 2)?;
            if !(0..=1).contains(&mode) {
                return Err("Value out of range.  Allowed values: 0 to 2".into());
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getweaponoptic", |world, receiver, args| {
        super::super::engine::entity_id(world, receiver)?;
        let _weapon = named(world, args)?;
        Ok(Value::string(""))
    });
    registry.register(Function, "getbaseweaponitemindex", |world, _, args| {
        let name = string(args, 0)?;
        let idx = weapon_index(world, &name)?;
        if idx == 0 {
            return Err(format!(
                "GetBaseWeaponItemIndex: invalid weapon {name} passed."
            ));
        }
        Ok(Value::Int(idx as i32))
    });
    registry.register(Function, "getreffromitemindex", |world, _, args| {
        let index = int(args, 0)?;
        let tables = world.resource::<RoundScript>().tables.clone();
        let Some(table) = crate::script::host::tables::table(&tables, STATS_TABLE) else {
            return Ok(Value::string(""));
        };
        let key = index.to_string();
        let found = (0..table.rows)
            .find(|&row| table.cell(row, 0) == Some(key.as_str()))
            .and_then(|row| table.cell(row, 4))
            .unwrap_or("");
        Ok(Value::String(found.into()))
    });
    registry.register(Function, "getdroppedweapons", |world, _, _| {
        new_array(world, Vec::new())
    });
    registry.register(Method, "isweaponoverheating", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let want_heat = if !args.is_empty() {
            int(args, 0)? != 0
        } else {
            false
        };
        let Some(def) = current_or_named_weapon(world, client, args, 1)? else {
            return Ok(if want_heat {
                Value::Float(0.0)
            } else {
                Value::Int(0)
            });
        };
        Ok(if want_heat {
            Value::Float(heat_value(world, client.0, &def))
        } else {
            Value::Int(is_overheating(world, client.0, &def).into())
        })
    });
    registry.register(Method, "setweaponoverheating", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let overheating = int(args, 0)? != 0;
        let heat = super::float(args, 1)?;
        let Some(def) = current_or_named_weapon(world, client, args, 2)? else {
            return Ok(Value::Undefined);
        };
        if def.overheat_weapon {
            if let Some(mut frame) = FrameWorld::try_from_world(world)
                && let Ok(weapon) = script_player::weapon_named(&frame, &def.name)
            {
                let _ = crate::game_move::set_weapon_heat(
                    &mut frame,
                    client,
                    weapon,
                    overheating,
                    heat,
                    def.overheat_end_val,
                    def.overheat_weapon,
                );
            }
            if overheating {
                add_heat(world, client.0, &def, 100.0);
                add_heat(world, client.0, &def, heat - 100.0);
            } else {
                add_heat(world, client.0, &def, -100.0);
                add_heat(world, client.0, &def, heat);
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getcurrentweaponspinlerp", |world, receiver, _| {
        let client = player_entity(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        if !frame.player(client).is_some_and(|ps| ps.weapon != 0) {
            return Ok(Value::Float(0.0));
        }
        if let Some(spin) = crate::game_move::weapon_spin(&frame, client) {
            return Ok(Value::Float(spin));
        }
        let spin = world
            .get_resource::<WeaponSpin>()
            .and_then(|s| s.0.get(&client.0).copied())
            .unwrap_or(0.0);
        Ok(Value::Float(spin))
    });
    registry.register(
        Method,
        "setcurrentweaponspinlerp",
        |world, receiver, args| {
            let client = player_entity(world, receiver)?;
            let spin = super::float(args, 0)?;
            if let Some(mut frame) = FrameWorld::try_from_world(world) {
                let _ = crate::game_move::set_weapon_spin(&mut frame, client, spin);
            }
            world
                .get_resource_or_insert_with(WeaponSpin::default)
                .0
                .insert(client.0, spin);
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "weaponplayejectbrass", |world, receiver, _| {
        player_entity(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isweaponviewonlylinked", |world, receiver, _| {
        player_entity(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "finishmeleehit", |world, receiver, args| {
        player_entity(world, receiver)?;
        string(args, 1)?;
        super::vector(args, 2)?;
        super::vector(args, 3)?;
        int(args, 4)?;
        int(args, 5)?;
        int(args, 6)?;
        Ok(Value::Undefined)
    });
    // gscm_calcweaponoptions: packs the player's entity number and weapon
    // options (camo, lens, reticle, tag, emblem); the two-argument custom
    // class form is refused (`t6.weapons.class_options`).
    registry.register(Method, "calcweaponoptions", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let mut bits = ((client.0 + 1) & 0x1f) << 15;
        if args.len() == 2 {
            int(args, 0)?;
            int(args, 1)?;
            return Err(
                "t6.weapons.class_options: custom class weapon options are not read".into(),
            );
        }
        if args.len() < 3 {
            return Err("calcWeaponOptions takes 5 arguments: <camo> <lens> <reticle> <tag> <emblem>\n                     or 2 argument: <custom class slot> <primary weapon=0 | secondary weapon=1>".into());
        }
        bits ^= (int(args, 0)? as u32) & 0x7f;
        bits ^= ((int(args, 2)? as u32) & 0x3f) << 7;
        if args.len() > 3 {
            bits ^= ((int(args, 3)? as u32) & 1) << 13;
            if args.len() > 4 {
                bits ^= ((int(args, 4)? as u32) & 1) << 14;
            }
        }
        Ok(Value::Int(bits as i32))
    });

    registry.register(Function, "precacheitem", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err("precacheItem must be called before any wait statements in the gametype or level script\n".into());
        }
        let name = string(args, 0)?;
        if weapon_index(world, &name)? == 0 {
            return Err(format!("unknown weapon '{name}'"));
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "getweaponindexfromname", |world, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(weapon_index(world, &name)? as i32 & 0xff))
    });
    registry.register(Function, "getweaponfiresound", |world, _, args| {
        sound(world, args, |w| &w.fire_sound)
    });
    registry.register(Function, "getweaponfiresoundplayer", |world, _, args| {
        sound(world, args, |w| &w.fire_sound_player)
    });
    registry.register(Function, "getweaponpickupsound", |world, _, args| {
        sound(world, args, |w| &w.pickup_sound)
    });
    registry.register(Function, "getweaponpickupsoundplayer", |world, _, args| {
        sound(world, args, |w| &w.pickup_sound_player)
    });
    registry.register(Function, "weaponclipsize", |world, _, args| {
        let weapon = named(world, args)?;
        Ok(Value::Int(clip_size(world, &weapon)))
    });
    registry.register(Function, "weaponfuellife", |world, _, args| {
        Ok(Value::Int(named(world, args)?.tank_life_time))
    });
    registry.register(Function, "weaponmaxammo", |world, _, args| {
        let weapon = named(world, args)?;
        Ok(Value::Int(ammo(world, &weapon, weapon.max_ammo)))
    });
    registry.register(Function, "weaponstartammo", |world, _, args| {
        let weapon = named(world, args)?;
        Ok(Value::Int(ammo(world, &weapon, weapon.start_ammo)))
    });
    registry.register(Function, "weapontype", |world, _, args| {
        Ok(enum_name(&WEAP_TYPES, named(world, args)?.weap_type))
    });
    registry.register(Function, "weaponinventorytype", |world, _, args| {
        Ok(enum_name(
            &INVENTORY_TYPES,
            named(world, args)?.inventory_type,
        ))
    });
    registry.register(Function, "weaponissemiauto", |world, _, args| {
        Ok(Value::Int(
            (named(world, args)?.fire_type == FIRE_SINGLE_SHOT).into(),
        ))
    });
    registry.register(Function, "weaponclass", |world, _, args| {
        Ok(match named_or_none(world, args)? {
            Some(weapon) => enum_name(&WEAP_CLASSES, weapon.weap_class),
            None => Value::string("none"),
        })
    });
    registry.register(Function, "isweaponprimary", |world, _, args| {
        let weapon = named_or_none(world, args)?;
        Ok(Value::Int(
            weapon
                .is_some_and(|w| w.inventory_type == INVENTORY_PRIMARY)
                .into(),
        ))
    });
    registry.register(Function, "weaponisgasweapon", |world, _, args| {
        let weapon = named_or_none(world, args)?;
        Ok(Value::Int(
            weapon.is_some_and(|w| w.weap_type == WEAP_TYPE_GAS).into(),
        ))
    });
    registry.register(Function, "isweaponequipment", |world, _, args| {
        let weapon = named_or_none(world, args)?;
        Ok(Value::Int(
            weapon
                .is_some_and(|w| w.offhand_slot == OFFHAND_EQUIPMENT)
                .into(),
        ))
    });
    registry.register(Function, "isweaponspecificuse", |world, _, args| {
        let weapon = named_or_none(world, args)?;
        Ok(Value::Int(
            weapon
                .is_some_and(|w| w.offhand_slot == OFFHAND_SPECIFIC_USE)
                .into(),
        ))
    });
    registry.register(Function, "weaponaltweaponname", |world, _, args| {
        let alt = named(world, args)?.alt_weapon.clone();
        other_weapon(world, &alt)
    });
    registry.register(Function, "weapondualwieldweaponname", |world, _, args| {
        let weapon = named(world, args)?;
        if !weapon.dual_wield {
            return Ok(Value::string("none"));
        }
        other_weapon(world, &weapon.dual_wield_weapon.clone())
    });
    registry.register(Function, "getweaponmodel", |world, _, args| {
        let Some(weapon) = named_or_none(world, args)? else {
            return Ok(Value::string(""));
        };
        let wanted = if args.len() == 2 {
            int(args, 1)? as u32
        } else {
            0
        };
        let model = |index: u32| {
            weapon
                .world_models
                .get(index as usize)
                .filter(|name| !name.is_empty())
        };
        let index = if wanted < 0x100 && model(wanted).is_some() {
            wanted
        } else {
            0
        };
        Ok(Value::string(model(index).map_or("", String::as_str)))
    });
    registry.register(Function, "getwatcherweapons", |world, _, _| {
        flagged(world, |w| w.watcher)
    });
    registry.register(Function, "getretrievableweapons", |world, _, _| {
        flagged(world, |w| w.retrievable)
    });
}
