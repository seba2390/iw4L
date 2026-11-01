//! Black Ops multiplayer's stats reset: what the client writes into its
//! stats when they are empty or keep an older stats version. It clears the
//! client buffer and runs `mp/stats_init.cfg` with Combat Training forced
//! on, whose commands fill each custom class from Black Ops' default
//! classes; then it stamps the checksum and the stats version.

use super::t5_classes::ITEMS as SLOTS;
use super::t5_stats::{
    BUFFER_BYTES, Cursor, ROOT, element_bits, leaf, read_int, set_raw_int, set_raw_string,
    set_raw_u64, step_name, validate,
};
use crate::script::{MatchScript, RoundScript, StringTable};
use asset_core::BitLayout;
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

const STATS_INIT: &str = "mp/stats_init.cfg";
const STATS_TABLE: &str = "mp/statstable.csv";
/// `stat_version`'s registered default and `stats_version_check`'s.
const STAT_VERSION: i32 = 10;
const STATS_VERSION_CHECK: bool = true;
/// The header byte whose bits the reset sets (4) and clears (2).
const FLAGS_AT: usize = 8;
const MAX_ITEMS: usize = 0x100;
pub(super) const ITEM_GROUPS: [&str; 24] = [
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
    "inventory",
    "head",
    "headgear",
    "body",
    "miscweapon",
    "feature",
    "deathstreak",
    "killstreak",
];
/// Items of this group and after are no class's: the killstreaks, of
/// which the ones marked `all` are every class's.
const KILLSTREAK_GROUP: i32 = 23;
pub(super) const DEFAULT_CLASSES: [&str; 20] = [
    "CLASS_SMG",
    "CLASS_CQB",
    "CLASS_ASSAULT",
    "CLASS_LMG",
    "CLASS_SNIPER",
    "CLASS_CUSTOM_SMG",
    "CLASS_CUSTOM_CQB",
    "CLASS_CUSTOM_ASSAULT",
    "CLASS_CUSTOM_LMG",
    "CLASS_CUSTOM_SNIPER",
    "CLASS_WAGER1",
    "CLASS_WAGER2",
    "CLASS_WAGER3",
    "CLASS_WAGER4",
    "CLASS_WAGER5",
    "CLASS_WAGER6",
    "CLASS_WAGER7",
    "CLASS_WAGER8",
    "CLASS_WAGER9",
    "CLASS_WAGER10",
];
/// The slots a default class fills; the rest (the killstreaks and the
/// deathstreak) are every class's.
pub(super) const CLASS_SLOTS: usize = 35;
/// Each default class's row in the game's table, then the four slots it
/// keeps for the default killstreaks after them.
const DEFAULT_TABLE: usize = DEFAULT_CLASSES.len() * CLASS_SLOTS;
/// Where `equipdefaultglobalitems` reads its slots: the last class's row,
/// so that the killstreak slots fall on the default killstreaks.
pub(super) const GLOBAL_ROW: usize = DEFAULT_TABLE - CLASS_SLOTS;

/// One row of `mp/statsTable.csv`, as the game's item table keeps it.
pub(super) struct Item {
    pub(super) index: i32,
    /// Its group's position in `ITEM_GROUPS`, -1 for none.
    pub(super) group: i32,
    pub(super) name: String,
    /// Column 9, which orders the default killstreaks.
    order: i32,
    pub(super) slot: Option<usize>,
    classes: Vec<usize>,
    all_classes: bool,
    /// Column 5: a perk that is a Pro version.
    pub(super) pro: bool,
    /// Columns 6 and 7: the name a body's perk starts with, and the weapon
    /// group a body goes with.
    pub(super) body_perk: String,
    pub(super) body_group: String,
    /// The reference (column 4) names `null`: no body is chosen from it.
    pub(super) null: bool,
    /// Column 8: the weapon's attachments, by their index in
    /// `ATTACHMENTS`, after the first, none.
    pub(super) attachments: Vec<usize>,
    /// Columns 10 and 14: the rank and the prestige that unlock it.
    pub(super) unlock_rank: i32,
    pub(super) unlock_prestige: i32,
    /// Column 12; a dual-wield-only item's (its name ends `_DW`) is -1.
    pub(super) cost: i32,
}

/// The attachments by index, as the weapon table's names; `dw` is the
/// last an item lists.
pub(super) const ATTACHMENTS: [&str; 24] = [
    "none",
    "reflex",
    "elbit",
    "acog",
    "lps",
    "vzoom",
    "ir",
    "gl",
    "mk",
    "silencer",
    "grip",
    "extclip",
    "dualclip",
    "saddle",
    "fmj",
    "hp",
    "rf",
    "ft",
    "bayonet",
    "auto",
    "speed",
    "upgradesight",
    "snub",
    "dw",
];

fn index_in(names: &[&str], name: &str) -> Option<usize> {
    names.iter().position(|n| n.eq_ignore_ascii_case(name))
}

/// The item table: item `n` is the row whose first cell is `n`, kept when
/// it has a name and a reference.
pub(super) fn items(table: &StringTable) -> Vec<Item> {
    let cell = |row: usize, column: usize| table.cell(row, column).unwrap_or("");
    let mut items: Vec<Item> = (0..MAX_ITEMS)
        .filter_map(|n| {
            let row = (0..table.rows).find(|&row| cell(row, 0) == n.to_string())?;
            if cell(row, 3).is_empty() || cell(row, 4).is_empty() {
                return None;
            }
            let group = index_in(&ITEM_GROUPS, cell(row, 2)).map_or(-1, |g| g as i32);
            let classes = if group < KILLSTREAK_GROUP {
                cell(row, 11)
                    .split_whitespace()
                    .filter_map(|name| index_in(&DEFAULT_CLASSES, name))
                    .collect()
            } else {
                Vec::new()
            };
            Some(Item {
                index: n as i32,
                group,
                name: cell(row, 3).to_owned(),
                order: super::iw4::atoi(cell(row, 9)),
                slot: Some(cell(row, 13))
                    .filter(|s| !s.is_empty())
                    .and_then(|s| index_in(&SLOTS, s)),
                classes,
                all_classes: group >= KILLSTREAK_GROUP && cell(row, 11).eq_ignore_ascii_case("all"),
                pro: super::iw4::atoi(cell(row, 5)) != 0,
                body_perk: cell(row, 6).to_owned(),
                body_group: cell(row, 7).to_owned(),
                null: cell(row, 4).to_ascii_lowercase().contains("null"),
                attachments: std::iter::once(0)
                    .chain(
                        cell(row, 8)
                            .split_whitespace()
                            .filter_map(|name| index_in(&ATTACHMENTS, name)),
                    )
                    .collect(),
                unlock_rank: super::iw4::atoi(cell(row, 10)),
                unlock_prestige: super::iw4::atoi(cell(row, 14)),
                cost: super::iw4::atoi(cell(row, 12)),
            })
        })
        .collect();
    // A dual-wield-only weapon (named `*_DW`) is bought apart from the
    // weapon it is named after.
    for item in &mut items {
        let start = item.name.len().saturating_sub(3);
        if item
            .name
            .get(start..)
            .is_some_and(|end| end.eq_ignore_ascii_case("_DW"))
        {
            item.cost = -1;
        }
    }
    items
}

/// The default classes' items and the default killstreaks: an item fills
/// the slot of each default class it names unless an earlier item did; the
/// killstreaks every class has follow, ordered by column 9.
pub(super) fn default_table(items: &[Item]) -> Vec<i32> {
    let mut table = vec![-1; DEFAULT_TABLE];
    table.extend([0; 4]);
    for item in items.iter().filter(|item| item.group < KILLSTREAK_GROUP) {
        let Some(slot) = item.slot else {
            continue;
        };
        for &class in item.classes.iter().take(DEFAULT_CLASSES.len()) {
            if let Some(entry) = table.get_mut(class * CLASS_SLOTS + slot)
                && *entry == -1
            {
                *entry = item.index;
            }
        }
    }
    let mut killstreaks: Vec<&Item> = items
        .iter()
        .filter(|item| item.all_classes)
        .take(4)
        .collect();
    killstreaks.sort_by_key(|item| (item.order, item.index));
    for (entry, item) in table[DEFAULT_TABLE..].iter_mut().zip(killstreaks) {
        *entry = item.index;
    }
    table
}

/// A slot's item when its default class names none, found by its name
/// (column 3).
pub(super) fn slot_default(items: &[Item], slot: usize) -> i32 {
    let name = match slot {
        24..=27 | 38 => "perks_none",
        28 | 29 => "armor_null",
        35..=37 => "killstreak_null",
        _ => "weapon_null",
    };
    items
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(name))
        .map_or(-1, |item| item.index)
}

struct Reset<'a> {
    layout: &'a BitLayout,
    buffer: Vec<u8>,
    configs: &'a BTreeMap<String, Arc<str>>,
    items: Vec<Item>,
    defaults: Vec<i32>,
    dvars: BTreeMap<String, String>,
}

impl Reset<'_> {
    fn walk_names(&self, path: &[&str]) -> Option<Cursor> {
        path.iter()
            .try_fold(ROOT, |at, name| step_name(self.layout, at, name))
    }

    /// `equipdefaultclass` and `equipdefaultglobalitems`' write: the item
    /// goes in unless the slot already holds it.
    fn equip(&mut self, path: &[&str], item: i32) {
        let Some(at) = self.walk_names(path) else {
            diag::warn!(
                Sim,
                "DDL: Error could not find loadout slot {}",
                path.last().copied().unwrap_or("")
            );
            return;
        };
        let current = leaf(self.layout, at).and_then(|m| read_int(m, at, &self.buffer));
        if current != Some(item as u32)
            && !set_raw_int(self.layout, &mut self.buffer, at, item as u32)
        {
            diag::warn!(
                Sim,
                "DDL: Error equipping class {} with item {item}",
                path.get(1).copied().unwrap_or("")
            );
        }
    }

    /// `equipdefaultclass <customclass> <defaultclass>`. Black Ops then
    /// swaps each perk for its Pro version where that is unlocked; the
    /// reset runs this before it unlocks any.
    fn equip_default_class(&mut self, class: &str, default: &str) {
        let Some(row) = index_in(&DEFAULT_CLASSES, default) else {
            return;
        };
        for (slot, name) in SLOTS.iter().enumerate().take(CLASS_SLOTS) {
            let item = match self.defaults[row * CLASS_SLOTS + slot] {
                -1 => slot_default(&self.items, slot),
                item => item,
            };
            self.equip(&["cacLoadouts", class, name], item);
        }
    }

    fn equip_default_global_items(&mut self) {
        for (slot, name) in SLOTS.iter().enumerate().skip(CLASS_SLOTS) {
            let item = match self.defaults[GLOBAL_ROW + slot] {
                -1 => slot_default(&self.items, slot),
                item => item,
            };
            self.equip(&["cacLoadouts", name], item);
        }
    }

    /// `statsetbyname <name> <value>`: the integer member of that name in
    /// `PlayerStatsList`.
    fn stat_set_by_name(&mut self, name: &str, value: &str) {
        let set = self
            .walk_names(&["PlayerStatsList", name])
            .is_some_and(|at| {
                set_raw_int(
                    self.layout,
                    &mut self.buffer,
                    at,
                    super::iw4::atoi(value) as u32,
                )
            });
        if !set {
            diag::warn!(Sim, "DDL:Error setting player stat value for {name}.");
        }
    }

    /// `statwriteddl <path…> <value>`: a number indexes an array that has
    /// no enum; the value is read as the member's type.
    fn stat_write_ddl(&mut self, args: &[String]) {
        let Some((value, path)) = args.split_last() else {
            diag::warn!(Sim, "DDL: statsDDLTest usage: statsDDLTest <path>");
            return;
        };
        let mut at = ROOT;
        for key in path {
            let member = super::t5_stats::member(self.layout, at.member);
            let next = match member {
                Some(m) if m.array_size >= 2 && m.enum_index == -1 && at.index == -1 => {
                    let index = super::iw4::atoi(key);
                    let size = element_bits(m);
                    Some(Cursor {
                        offset: size * i64::from(index) + at.offset,
                        index,
                        member: at.member,
                    })
                }
                _ => step_name(self.layout, at, key),
            };
            let Some(next) = next else {
                diag::warn!(Sim, "DDL: Member '{key}' not found.");
                return;
            };
            at = next;
        }
        let written = leaf(self.layout, at).is_some_and(|m| {
            (m.array_size < 2 || at.index != -1)
                && match m.kind {
                    0..=2 => set_raw_int(
                        self.layout,
                        &mut self.buffer,
                        at,
                        super::iw4::atoi(value) as u32,
                    ),
                    3 => set_raw_u64(
                        self.layout,
                        &mut self.buffer,
                        at,
                        super::t5_stats::atoi64(value),
                    ),
                    5 => set_raw_string(self.layout, &mut self.buffer, at, value),
                    _ => false,
                }
        });
        if !written {
            diag::warn!(Sim, "DDL: Could not write value {value}.");
        }
    }

    fn dvar_bool(&self, name: &str) -> bool {
        self.dvars
            .get(&name.to_ascii_lowercase())
            .is_some_and(|v| super::iw4::atoi(v) != 0)
    }

    /// `if ( <condition> ) <command>`: the configs' only condition is
    /// `dvarbool( "name" )`.
    fn run_if(&mut self, line: &str) {
        let Some(open) = line.find('(') else {
            return;
        };
        let mut depth = 0;
        let Some(close) = line[open..].char_indices().find_map(|(i, c)| {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open + i);
                    }
                }
                _ => {}
            }
            None
        }) else {
            return;
        };
        let condition: String = line[open + 1..close]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let Some(name) = condition
            .strip_prefix("dvarbool(\"")
            .or_else(|| condition.strip_prefix("dvarBool(\""))
            .and_then(|rest| rest.strip_suffix("\")"))
        else {
            diag::warn!(
                Sim,
                "unknown!t5.mp.stats_init_condition: the stats reset reads no condition {condition}"
            );
            return;
        };
        if self.dvar_bool(name) {
            self.run_line(&line[close + 1..]);
        }
    }

    fn run_config(&mut self, name: &str, depth: usize) {
        let Some(text) = self.configs.get(&name.to_ascii_lowercase()).cloned() else {
            diag::warn!(Sim, "couldn't exec {name}");
            return;
        };
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("");
            for statement in statements(line) {
                if let Some(file) = statement.trim().strip_prefix("exec ")
                    && depth < 8
                {
                    self.run_config(file.trim(), depth + 1);
                } else {
                    self.run_line(&statement);
                }
            }
        }
    }

    fn run_line(&mut self, line: &str) {
        let line = line.trim();
        if line.starts_with("if") && line[2..].trim_start().starts_with('(') {
            self.run_if(line);
            return;
        }
        let args = tokens(line);
        let Some(command) = args.first() else {
            return;
        };
        match command.to_ascii_lowercase().as_str() {
            "equipdefaultclass" if args.len() >= 3 => {
                self.equip_default_class(&args[1], &args[2]);
            }
            "equipdefaultclass" => diag::warn!(
                Sim,
                "equipdefaultclass usage: equipdefaultclass <customclassname> <defaultClass>"
            ),
            "equipdefaultglobalitems" => self.equip_default_global_items(),
            "statsetbyname" if args.len() >= 2 => {
                let value = args.get(2).map_or("", String::as_str);
                self.stat_set_by_name(&args[1], value);
            }
            "statsetbyname" => {
                diag::warn!(Sim, "statsetbyname usage: statset <statname> <value>");
            }
            "statwriteddl" => self.stat_write_ddl(&args[1..]),
            // The class names and the clan name are the client's dvars, not
            // its stats.
            "set" | "setfromlocstring" => {}
            other => diag::warn!(
                Sim,
                "unknown!t5.mp.stats_init_command: the stats reset runs no command {other}"
            ),
        }
    }
}

/// A config line's commands, split at each `;` outside quotes.
fn statements(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = out.last_mut() {
            last.push(c);
        }
    }
    out
}

/// A command's words; a quoted word keeps its blanks.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                crc >> 1 ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn stat_version(world: &World) -> i32 {
    world
        .resource::<MatchScript>()
        .dvars
        .get("stat_version")
        .map_or(STAT_VERSION, |v| super::iw4::atoi(v))
}

/// Whether a profile's stats need the reset: none, or (with
/// `stats_version_check`) an older stats version in the client buffer.
pub(super) fn needs_reset(world: &World, layout: &BitLayout, bytes: Option<&[u8]>) -> bool {
    let Some(bytes) = bytes.filter(|b| b.len() == 2 * BUFFER_BYTES) else {
        return true;
    };
    let check = world
        .resource::<MatchScript>()
        .dvars
        .get("stats_version_check")
        .map_or(STATS_VERSION_CHECK, |v| super::iw4::atoi(v) != 0);
    let version = ["PlayerStatsList", "STATS_VERSION"]
        .into_iter()
        .try_fold(ROOT, |at, name| step_name(layout, at, name))
        .and_then(|at| read_int(leaf(layout, at)?, at, &bytes[..BUFFER_BYTES]));
    check && version != Some(stat_version(world) as u32)
}

/// The reset's client and server buffers, back to back.
pub(super) fn reset(world: &World, layout: &BitLayout) -> Vec<u8> {
    let runtime = world.resource::<RoundScript>();
    let empty = BTreeMap::new();
    let configs = runtime
        .restart
        .as_ref()
        .map_or(&empty, |plan| &plan.raw_files);
    let items = runtime
        .tables
        .get(STATS_TABLE)
        .map(items)
        .unwrap_or_else(Vec::new);
    let mut dvars = world.resource::<MatchScript>().dvars.clone();
    dvars.insert("xblive_basictraining".into(), "1".into());
    let mut reset = Reset {
        layout,
        buffer: vec![0; BUFFER_BYTES],
        configs,
        defaults: default_table(&items),
        items,
        dvars,
    };
    validate(layout, &mut reset.buffer);
    reset.run_config(STATS_INIT, 0);
    let checksum = crc32(&reset.buffer[4..]);
    reset.buffer[..4].copy_from_slice(&checksum.to_le_bytes());
    let version = stat_version(world).to_string();
    reset.stat_set_by_name("STATS_VERSION", &version);
    reset.buffer[FLAGS_AT] = (reset.buffer[FLAGS_AT] | 4) & !2;
    let mut bytes = reset.buffer;
    bytes.resize(2 * BUFFER_BYTES, 0);
    bytes
}
