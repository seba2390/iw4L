//! A multiplayer class as Black Ops 2's builtins read it. Default classes
//! (10 to 14) and the classes a new profile's custom slots hold
//! (`mp/reset_classes.cfg`) are in `mp/statsTable.csv`: an item's row names the
//! classes that carry it, each followed by what it carries the item with (its
//! attachments or its count). Every item is unlocked, as in a private match.
//! Edited custom classes are not read (`t6.mp.custom_classes`).

use super::{client_of, int, string};
use crate::script::host::arrays::new_array;
use crate::script::{Namespace, NativeRegistry, RoundScript, StringTable, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const STATS_TABLE: &str = "mp/statstable.csv";
const INDEX: usize = 0;
const REFERENCE: usize = 4;
const ATTACHMENTS: usize = 8;
const CLASSES: usize = 11;
const SLOT: usize = 13;

/// Default classes 10 to 14 (`_class.gsc`, `load_default_loadout`).
const DEFAULT_CLASSES: [&str; 5] = [
    "class_smg",
    "class_cqb",
    "class_assault",
    "class_lmg",
    "class_sniper",
];
/// What `equipdefaultclass` gives custom slots 0 to 9 (`mp/reset_classes.cfg`).
const CUSTOM_CLASSES: [&str; 5] = [
    "class_custom_assault",
    "class_custom_smg",
    "class_custom_lmg",
    "class_custom_cqb",
    "class_custom_sniper",
];

/// An item of a class.
#[derive(Clone, Debug)]
struct ClassItem {
    index: i32,
    reference: String,
    slot: String,
    /// What the class carries it with: attachments, or a count.
    extras: Vec<String>,
}

fn stats_table(world: &World) -> Option<StringTable> {
    world
        .resource::<RoundScript>()
        .tables
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(STATS_TABLE))
        .map(|(_, table)| table.clone())
}

/// The class a class number stands for.
fn class_name(class: i32) -> Result<&'static str, String> {
    match class {
        0..=9 => Ok(CUSTOM_CLASSES[class as usize % CUSTOM_CLASSES.len()]),
        10..=14 => Ok(DEFAULT_CLASSES[class as usize - 10]),
        _ => Err(format!("class number {class} is out of range")),
    }
}

/// The items of a class, in the table's order.
fn class_items(table: &StringTable, name: &str) -> Vec<ClassItem> {
    let mut items = Vec::new();
    for row in 0..table.rows {
        let cell = |column| table.cell(row, column).unwrap_or("");
        let mut extras = None;
        for token in cell(CLASSES).split_whitespace() {
            if token.starts_with("class_") {
                if extras.is_some() {
                    break;
                }
                if token.eq_ignore_ascii_case(name) {
                    extras = Some(Vec::new());
                }
            } else if let Some(extras) = &mut extras {
                extras.push(token.to_owned());
            }
        }
        if let (Some(extras), Ok(index)) = (extras, cell(INDEX).parse()) {
            items.push(ClassItem {
                index,
                reference: cell(REFERENCE).to_owned(),
                slot: cell(SLOT).to_owned(),
                extras,
            });
        }
    }
    items
}

/// The class's item in a loadout slot: its bonus cards in order under
/// `bonuscard1` to `3`, and otherwise the items the table puts in the slot,
/// the second of a greedy perk slot under `specialty4` to `6`.
fn slot_item(items: &[ClassItem], slot: &str) -> Option<ClassItem> {
    if let Some(number) = slot.strip_prefix("bonuscard") {
        let number: usize = number.parse().ok()?;
        return items
            .iter()
            .filter(|item| item.slot.starts_with("bonuscard"))
            .nth(number.checked_sub(1)?)
            .cloned();
    }
    if let Some(number) = slot.strip_prefix("specialty") {
        let number: usize = number.parse().ok()?;
        let (base, nth) = if number > 3 {
            (number - 3, 1)
        } else {
            (number, 0)
        };
        let base = format!("specialty{base}");
        return items
            .iter()
            .filter(|item| item.slot == base)
            .nth(nth)
            .cloned();
    }
    items.iter().find(|item| item.slot == slot).cloned()
}

fn class_of(world: &World, class: i32) -> Result<Vec<ClassItem>, String> {
    let table =
        stats_table(world).ok_or("t6.mp.stats_table: mp/statsTable.csv is not in the zones")?;
    Ok(class_items(&table, class_name(class)?))
}

/// The weapon a class's slot holds: `<reference>_mp`, then `+<attachment>`.
fn loadout_weapon(items: &[ClassItem], slot: &str) -> String {
    let Some(item) = slot_item(items, slot) else {
        return "weapon_null_mp".to_owned();
    };
    let mut name = format!("{}_mp", item.reference);
    for attachment in &item.extras {
        name.push('+');
        name.push_str(attachment);
    }
    name
}

/// What scripts stored on a player that no client reads yet.
#[derive(Resource, Default)]
pub(super) struct InventoryWeapons(BTreeMap<u32, String>);

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "getdefaultclassslot", |world, _, args| {
        let name = string(args, 0)?;
        let slot = string(args, 1)?;
        let table =
            stats_table(world).ok_or("t6.mp.stats_table: mp/statsTable.csv is not in the zones")?;
        let items = class_items(&table, &name);
        Ok(Value::string(
            &slot_item(&items, &slot).map_or(String::new(), |item| item.reference),
        ))
    });
    registry.register(Method, "getloadoutweapon", |world, receiver, args| {
        client_of(world, receiver)?;
        let items = class_of(world, int(args, 0)?)?;
        Ok(Value::string(&loadout_weapon(&items, &string(args, 1)?)))
    });
    registry.register(Method, "getloadoutitem", |world, receiver, args| {
        client_of(world, receiver)?;
        let items = class_of(world, int(args, 0)?)?;
        let slot = string(args, 1)?;
        // A grenade's count is what the class carries it with, one otherwise.
        if let Some(grenade) = slot.strip_suffix("count") {
            return Ok(Value::Int(slot_item(&items, grenade).map_or(0, |item| {
                item.extras
                    .first()
                    .and_then(|count| count.parse().ok())
                    .unwrap_or(1)
            })));
        }
        Ok(Value::Int(
            slot_item(&items, &slot).map_or(0, |item| item.index),
        ))
    });
    registry.register(Method, "getloadoutitemref", |world, receiver, args| {
        client_of(world, receiver)?;
        let items = class_of(world, int(args, 0)?)?;
        Ok(Value::string(
            &slot_item(&items, &string(args, 1)?)
                .map_or("weapon_null".to_owned(), |item| item.reference),
        ))
    });
    // Each perk slot's reference names its perks, joined by `|`.
    registry.register(Method, "getloadoutperks", |world, receiver, args| {
        client_of(world, receiver)?;
        let perks = class_of(world, int(args, 0)?)?
            .iter()
            .filter(|item| item.slot.starts_with("specialty"))
            .flat_map(|item| {
                item.reference
                    .split('|')
                    .map(Value::string)
                    .collect::<Vec<_>>()
            })
            .collect();
        new_array(world, perks)
    });
    // The item's attachment by its number in the item's list, from one.
    registry.register(Function, "getitemattachment", |world, _, args| {
        let (index, number) = (int(args, 0)?, int(args, 1)?);
        let table =
            stats_table(world).ok_or("t6.mp.stats_table: mp/statsTable.csv is not in the zones")?;
        let wanted = index.to_string();
        let attachment = (0..table.rows)
            .find(|&row| table.cell(row, INDEX) == Some(wanted.as_str()))
            .and_then(|row| table.cell(row, ATTACHMENTS))
            .and_then(|list| {
                list.split_whitespace()
                    .nth(usize::try_from(number).ok()?.checked_sub(1)?)
            })
            .unwrap_or("none");
        Ok(Value::string(attachment))
    });
    // A private match unlocks and restricts nothing.
    registry.register(Method, "isitemlocked", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isitemrestricted", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isweapondisallowedatmatchstart", |_, _, _| {
        Ok(Value::Int(0))
    });
    // Camo, reticle and the character's options (t6.mp.player_options).
    registry.register(Method, "calcplayeroptions", |world, receiver, args| {
        client_of(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "setinventoryweapon", |world, receiver, args| {
        let client = client_of(world, receiver)?;
        let weapon = string(args, 0)?;
        world
            .get_resource_or_insert_with(InventoryWeapons::default)
            .0
            .insert(client, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getinventoryweapon", |world, receiver, _| {
        let client = client_of(world, receiver)?;
        Ok(Value::string(
            world
                .get_resource::<InventoryWeapons>()
                .and_then(|weapons| weapons.0.get(&client))
                .map_or("none", String::as_str),
        ))
    });
}
