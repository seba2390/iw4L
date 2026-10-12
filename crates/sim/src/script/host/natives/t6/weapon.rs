//! Black Ops 2's weapon table: each weapon's index is the order the engine
//! registered it in, from the stats table at level start and then by the
//! builtins scripts call before their first wait.

use super::super::super::arrays::new_array;
use super::{int, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
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

pub(super) fn register_natives(registry: &mut NativeRegistry) {
    use Namespace::Function;

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
