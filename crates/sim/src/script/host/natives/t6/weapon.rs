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

/// Black Ops 2's 30 attachment names (`eAttachment`, `mp/attachmentTable.csv`).
const ATTACHMENT_NAMES: [&str; 30] = [
    "none",
    "acog",
    "dualclip",
    "dualoptic",
    "dw",
    "extbarrel",
    "extclip",
    "extramags",
    "fastads",
    "fastreload",
    "fmj",
    "gl",
    "grip",
    "holo",
    "ir",
    "is",
    "longbreath",
    "mk",
    "mms",
    "rangefinder",
    "reflex",
    "rf",
    "sf",
    "silencer",
    "stackfire",
    "stalker",
    "steadyaim",
    "swayreduc",
    "tacknife",
    "vzoom",
];

fn attachment_index(name: &str) -> Option<u32> {
    ATTACHMENT_NAMES
        .iter()
        .position(|a| a.eq_ignore_ascii_case(name))
        .map(|i| i as u32)
        .filter(|&i| i != 0 && i < 30)
}

fn is_optic_attachment(kind: u32) -> bool {
    matches!(kind, 1 | 3 | 13 | 14 | 15 | 18 | 19 | 20 | 29)
}

/// Sorts up to three 6-bit attachment indices in ascending order (`BG_SortAttachments`).
fn pack_attachments(base: usize, attachments: [u32; 3]) -> usize {
    let mut a = attachments;
    if a[0] == 0 || (a[1] != 0 && a[1] < a[0]) {
        a.swap(0, 1);
    }
    if a[1] == 0 || (a[2] != 0 && a[2] < a[1]) {
        a.swap(1, 2);
    }
    if a[0] == 0 || (a[1] != 0 && a[1] < a[0]) {
        a.swap(0, 1);
    }
    (base & 0xff)
        | (((a[0] as usize) & 0x3f) << 8)
        | (((a[1] as usize) & 0x3f) << 14)
        | (((a[2] as usize) & 0x3f) << 20)
}

fn unpacked_attachments(weapon_id: usize) -> [u32; 3] {
    [
        ((weapon_id >> 8) & 0x3f) as u32,
        ((weapon_id >> 14) & 0x3f) as u32,
        ((weapon_id >> 20) & 0x3f) as u32,
    ]
}

#[derive(Resource)]
struct WeaponTable {
    /// By index; index 0 is no weapon (`bg_weaponVariantDefs[256]`).
    weapons: Vec<Option<Arc<ZoneWeapon>>>,
    parent_variants: Vec<usize>,
    item_indices: Vec<Option<usize>>,
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

/// Sets an entity's hidden tags to a weapon's hide tags, as the engine's
/// weapon hide-tag builtins replace the model's hidden parts.
pub(super) fn hide_weapon_tags(world: &mut World, id: u64, name: &str) {
    let tags = zone_weapon(world, name)
        .map(|facts| facts.hide_tags.clone())
        .unwrap_or_default();
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.part_ops.push(("".into(), false));
        entity
            .part_ops
            .extend(tags.iter().map(|tag| (Arc::from(tag.as_str()), true)));
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
/// zones have it; 0 when they do not. Black Ops 2's `bg_weaponVariantDefs` and
/// `bg_weaponIndexHash` hold 256 entries indexed by the low 8 bits of the
/// weapon number (`uVar1 = bg_lastParsedWeaponIndex & 0xff`), while attachment
/// combinations ride in bits `8..26`.
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
    let index = table.weapons.len() & 0xff;
    if table.weapons.len() <= usize::from(u8::MAX) {
        table.weapons.push(Some(Arc::clone(&weapon)));
        table.parent_variants.push(0);
        table.item_indices.push(None);
    } else {
        table.weapons[index] = Some(Arc::clone(&weapon));
        table.parent_variants[index] = 0;
        table.item_indices[index] = None;
    }
    table.by_name.insert(name.to_ascii_lowercase(), index);
    for companion in &weapon.companions {
        let comp_idx = register(world, companion)?;
        if comp_idx == 0 {
            return Err(format!(
                "weapon '{companion}' that '{name}' names is in no zone"
            ));
        }
        let mut table = world.resource_mut::<WeaponTable>();
        if comp_idx != index && table.parent_variants.get(comp_idx).copied() == Some(0) {
            table.parent_variants[comp_idx] = index;
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
fn table_weapons(world: &World) -> Vec<(usize, String, Vec<String>)> {
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
            names.push((item.index, format!("{}_zm", item.name), variants));
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
        parent_variants: vec![0],
        item_indices: vec![None],
        by_name: BTreeMap::new(),
    });
    register(world, "defaultweapon_mp")?;
    // `g_connectpaths` is 0 at a normal start, so the stats table loads.
    for (item_index, name, variants) in table_weapons(world) {
        let base_idx = register(world, &name)?;
        if base_idx == 0 {
            continue;
        }
        let has_attachment_table = {
            let mut table = world.resource_mut::<WeaponTable>();
            table.item_indices[base_idx] = Some(item_index);
            table.weapons[base_idx]
                .as_ref()
                .is_some_and(|w| w.has_attachment_table)
        };
        if !has_attachment_table {
            for variant in variants {
                if zone_weapon(world, &variant).is_some() {
                    let v_idx = register(world, &variant)?;
                    if v_idx != 0 {
                        let mut table = world.resource_mut::<WeaponTable>();
                        table.parent_variants[v_idx] = base_idx;
                        table.item_indices[v_idx] = Some(item_index);
                    }
                }
            }
        }
    }
    Ok(())
}

/// The weapon a builtin names: registered as well before the level's first
/// wait, only found after it, with up to three `+` attachments packed into
/// bits `8..26` (`BG_GetWeaponIndexForName`).
pub(super) fn weapon_index(world: &mut World, name: &str) -> Result<usize, String> {
    table(world)?;
    let mut parts = name.split('+');
    let base_name = parts.next().unwrap_or("");
    let attach_parts: Vec<&str> = parts.collect();
    let valid_count = attach_parts
        .iter()
        .filter_map(|p| attachment_index(p))
        .count();
    if valid_count > 3 {
        return Err(format!("More than 3 attachments on weapon named {name}\n"));
    }
    let base_idx = if world.resource::<RoundScript>().loading {
        register(world, base_name)?
    } else {
        find(world, base_name)
    };
    if base_idx == 0 || attach_parts.is_empty() {
        return Ok(base_idx);
    }
    let mut chosen = [0u32; 3];
    let mut slot = 0;
    if let Some(weapon) = world
        .resource::<WeaponTable>()
        .weapons
        .get(base_idx & 0xff)
        .and_then(|w| w.as_ref())
    {
        for part in attach_parts {
            if let Some(kind) = attachment_index(part)
                && weapon.attachments.iter().any(|a| a.kind == kind)
                && slot < 3
            {
                chosen[slot] = kind;
                slot += 1;
            }
        }
    }
    Ok(pack_attachments(base_idx, chosen))
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
fn named_with_id(world: &mut World, args: &[Value]) -> Result<(usize, Arc<ZoneWeapon>), String> {
    let name = string(args, 0)?;
    let full_id = weapon_index(world, &name)?;
    let index = full_id & 0xff;
    let weapon = world
        .resource::<WeaponTable>()
        .weapons
        .get(index)
        .cloned()
        .flatten();
    match weapon {
        Some(weapon) if index != 0 => Ok((full_id, weapon)),
        _ if name.eq_ignore_ascii_case("none") => {
            Err(format!("Weapon name \"{name}\" is not valid.\n"))
        }
        _ => Err(format!(
            "Unknown weapon name \"{name}\": script may need to call PreCacheItem(\"{name}\") during level init.\n"
        )),
    }
}

fn named(world: &mut World, args: &[Value]) -> Result<Arc<ZoneWeapon>, String> {
    named_with_id(world, args).map(|(_, w)| w)
}

/// The weapon a builtin names, or none for an unknown name, which those
/// builtins answer quietly.
fn named_or_none_with_id(
    world: &mut World,
    args: &[Value],
) -> Result<Option<(usize, Arc<ZoneWeapon>)>, String> {
    let name = string(args, 0)?;
    let full_id = weapon_index(world, &name)?;
    let index = full_id & 0xff;
    Ok(world
        .resource::<WeaponTable>()
        .weapons
        .get(index)
        .cloned()
        .flatten()
        .filter(|_| index != 0)
        .map(|w| (full_id, w)))
}

fn named_or_none(world: &mut World, args: &[Value]) -> Result<Option<Arc<ZoneWeapon>>, String> {
    Ok(named_or_none_with_id(world, args)?.map(|(_, w)| w))
}

/// A clip as the engine sizes it (`BG_GetClipSize`): scaled by
/// `player_clipSizeMultiplier` and any attachment `clipSizeScale`, or
/// overridden by a `stackfire` attachment (`0x18`), rounded to even, never
/// below one; a one-round clip stays one.
fn clip_size(world: &World, weapon: &ZoneWeapon, weapon_id: usize) -> i32 {
    let mut scale = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("player_clipsizemultiplier")
        .and_then(|value| value.trim().parse::<f32>().ok())
        .unwrap_or(CLIP_SIZE_MULTIPLIER);
    for kind in unpacked_attachments(weapon_id) {
        if kind == 0 {
            break;
        }
        if let Some(att) = weapon.attachments.iter().find(|a| a.kind == kind) {
            if att.kind == 24 {
                return att.clip_size;
            }
            if att.clip_size_scale > 0.0 {
                scale *= att.clip_size_scale;
            }
        }
    }
    if weapon.clip_size > 1 {
        ((weapon.clip_size as f32 * scale + 9.313_226e-10).round_ties_even() as i32).max(1)
    } else {
        weapon.clip_size
    }
}

fn weapon_fire_type(weapon: &ZoneWeapon, weapon_id: usize) -> i32 {
    for kind in unpacked_attachments(weapon_id) {
        if kind == 0 {
            break;
        }
        if let Some(att) = weapon.attachments.iter().find(|a| a.kind == kind)
            && att.kind == 24
        {
            return att.fire_type;
        }
    }
    weapon.fire_type
}

/// Start or max ammo: rounds, or clips when the weapon counts in clips.
fn ammo(world: &World, weapon: &ZoneWeapon, weapon_id: usize, count: i32) -> i32 {
    if weapon.ammo_count_clip_relative {
        clip_size(world, weapon, weapon_id) * count
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

fn format_weapon_with_attachments(base_name: &str, weapon_id: usize) -> String {
    let mut out = base_name.to_owned();
    for kind in unpacked_attachments(weapon_id) {
        if kind == 0 {
            break;
        }
        if let Some(name) = ATTACHMENT_NAMES.get(kind as usize) {
            out.push('+');
            out.push_str(name);
        }
    }
    out
}

fn other_weapon(world: &mut World, name: &str, weapon_id: usize) -> Result<Value, String> {
    let index = if name.is_empty() {
        0
    } else {
        weapon_index(world, name)?
    };
    if index == 0 {
        return Ok(Value::string("none"));
    }
    let formatted = format_weapon_with_attachments(name, weapon_id);
    Ok(Value::string(&formatted))
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
    let Some((weapon_id, weapon)) = named_or_none_with_id(world, args)? else {
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
        clip_size(world, &weapon, weapon_id)
    } else {
        ammo(world, &weapon, weapon_id, weapon.max_ammo)
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
    let Some((weapon_id, weapon)) = named_or_none_with_id(world, args)? else {
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
    let start = ammo(world, &weapon, weapon_id, weapon.start_ammo);
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
    if *heat < 0.0 {
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
                (spin_before + -dt_ms * rate).clamp(-1.0, 0.0)
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
        let (weapon_id, _weapon) = named_with_id(world, args)?;
        let attached = unpacked_attachments(weapon_id);
        let mut has_any = false;
        for i in 1..args.len() {
            let name = string(args, i)?;
            if let Some(kind) = attachment_index(&name)
                && attached.contains(&kind)
            {
                has_any = true;
            }
        }
        Ok(Value::Int(has_any.into()))
    });
    registry.register(
        Function,
        "getweaponsupportedattachments",
        |world, _, args| {
            let weapon = named(world, args)?;
            let mut names = Vec::new();
            for kind in 0..30u32 {
                if (kind == 0 && weapon.has_attachment_table)
                    || weapon.attachments.iter().any(|a| a.kind == kind)
                {
                    names.push(Value::string(ATTACHMENT_NAMES[kind as usize]));
                }
            }
            new_array(world, names)
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
        let client = player_entity(world, receiver)?;
        let (weapon_id, _weapon) = named_with_id(world, args)?;
        let ads_full = FrameWorld::from_world(world)
            .player(client)
            .is_some_and(|ps| ps.f_weapon_pos_frac >= 1.0);
        if ads_full {
            let attached = unpacked_attachments(weapon_id);
            for kind in 1..30u32 {
                if attached.contains(&kind) && is_optic_attachment(kind) {
                    return Ok(Value::string(ATTACHMENT_NAMES[kind as usize]));
                }
            }
        }
        Ok(Value::string(""))
    });
    registry.register(Function, "getbaseweaponitemindex", |world, _, args| {
        let name = string(args, 0)?;
        let full_id = weapon_index(world, &name)?;
        let mut idx = full_id & 0xff;
        if idx == 0 {
            return Ok(Value::Int(0));
        }
        let item_index = {
            let table = world.resource::<WeaponTable>();
            if let Some(item) = table.item_indices.get(idx).copied().flatten() {
                Some(item)
            } else {
                let parent = table.parent_variants.get(idx).copied().unwrap_or(0);
                if parent != 0 {
                    idx = parent;
                }
                table.item_indices.get(idx).copied().flatten()
            }
        };
        let Some(item_index) = item_index else {
            return Err(format!(
                "GetBaseWeaponItemIndex: invalid weapon {name} passed."
            ));
        };
        Ok(Value::Int(item_index as i32))
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
    // options (camo, lens, reticle, tag, emblem), or reads the player's
    // custom class slot in the two-argument form (zero for a new profile).
    registry.register(Method, "calcweaponoptions", |world, receiver, args| {
        let client = player_entity(world, receiver)?;
        let mut bits = ((client.0 + 1) & 0x1f) << 15;
        if args.len() == 2 {
            int(args, 0)?;
            int(args, 1)?;
            return Ok(Value::Int(bits as i32));
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
        let (weapon_id, weapon) = named_with_id(world, args)?;
        Ok(Value::Int(clip_size(world, &weapon, weapon_id)))
    });
    registry.register(Function, "weaponfuellife", |world, _, args| {
        Ok(Value::Int(named(world, args)?.tank_life_time))
    });
    registry.register(Function, "weaponmaxammo", |world, _, args| {
        let (weapon_id, weapon) = named_with_id(world, args)?;
        Ok(Value::Int(ammo(world, &weapon, weapon_id, weapon.max_ammo)))
    });
    registry.register(Function, "weaponstartammo", |world, _, args| {
        let (weapon_id, weapon) = named_with_id(world, args)?;
        Ok(Value::Int(ammo(
            world,
            &weapon,
            weapon_id,
            weapon.start_ammo,
        )))
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
        let (weapon_id, weapon) = named_with_id(world, args)?;
        Ok(Value::Int(
            (weapon_fire_type(&weapon, weapon_id) == FIRE_SINGLE_SHOT).into(),
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
        let (weapon_id, weapon) = named_with_id(world, args)?;
        let alt = if weapon.inventory_type != 3 {
            unpacked_attachments(weapon_id)
                .into_iter()
                .filter(|&k| k != 0)
                .find_map(|k| {
                    weapon
                        .attachments
                        .iter()
                        .find(|a| a.kind == k && !a.alt_weapon.is_empty())
                        .map(|a| a.alt_weapon.clone())
                })
                .unwrap_or_else(|| weapon.alt_weapon.clone())
        } else {
            weapon.alt_weapon.clone()
        };
        other_weapon(world, &alt, weapon_id)
    });
    registry.register(Function, "weapondualwieldweaponname", |world, _, args| {
        let (weapon_id, weapon) = named_with_id(world, args)?;
        let attached_dw = unpacked_attachments(weapon_id)
            .into_iter()
            .filter(|&k| k != 0)
            .find_map(|k| {
                weapon
                    .attachments
                    .iter()
                    .find(|a| a.kind == k && !a.dual_wield_weapon.is_empty())
                    .map(|a| a.dual_wield_weapon.clone())
            });
        let is_dw = weapon.dual_wield
            || unpacked_attachments(weapon_id).contains(&4)
            || attached_dw.is_some();
        if !is_dw {
            return Ok(Value::string("none"));
        }
        let dw = if weapon.inventory_type != 3 {
            attached_dw.unwrap_or_else(|| weapon.dual_wield_weapon.clone())
        } else {
            weapon.dual_wield_weapon.clone()
        };
        other_weapon(world, &dw, weapon_id)
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
