//! Black Ops multiplayer's create-a-class commands, as BlackOpsMP.exe runs
//! them on the local profile's own stats (its client buffer): `equipClass`,
//! `equipClassItem`, `equipClassAttachment`, `copyclass`, the
//! `equipDefault*` commands and the attachment and new-item clears. A class is
//! `cacLoadouts.<class>.<slot>` in `ddl_mp/stats.ddl`; an item is a row of
//! `mp/statsTable.csv`, an attachment one of `mp/attachmentTable.csv`.

use super::t5_classes::ITEMS as SLOTS;
use super::t5_stats::{BUFFER_BYTES, Cursor, ROOT, leaf, read_int, set_raw_int, step_name};
use super::t5_stats_reset::{
    ATTACHMENTS, CLASS_SLOTS, DEFAULT_CLASSES, GLOBAL_ROW, Item, default_table, items, slot_default,
};
use crate::script::{MatchScript, RoundScript, StringTable};
use asset_core::BitLayout;
use bevy_ecs::prelude::World;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

const STATS_TABLE: &str = "mp/statstable.csv";
const ATTACHMENT_TABLE: &str = "mp/attachmenttable.csv";
const RANK_TABLE: &str = "mp/ranktable.csv";
const MAX_ITEMS: i32 = 0x100;
/// Slots by their index in `SLOTS`.
const PRIMARY: usize = 0;
const SECONDARY: usize = 12;
const SPECIALTY1: usize = 24;
const BODY: usize = 29;
/// The perks' slots the Pro pass walks, `specialty1` to `specialty3`.
const PERK_SLOTS: std::ops::Range<usize> = 24..27;
/// `ITEM_GROUPS`' perks.
const SPECIALTY_GROUP: i32 = 14;
const ITEM_GROUPS: usize = 24;
/// The attachment points by index; 5 is none of them.
const POINTS: [&str; 5] = ["none", "top", "bottom", "trigger", "muzzle"];
const NO_POINT: usize = 5;
/// No attachment: the index past `ATTACHMENTS`.
const NO_ATTACHMENT: usize = 24;
/// `copyclass`' classes by number.
const COPY_CLASSES: [&str; 10] = [
    "customclass1",
    "customclass2",
    "customclass3",
    "customclass4",
    "customclass5",
    "prestigeclass1",
    "prestigeclass2",
    "prestigeclass3",
    "prestigeclass4",
    "prestigeclass5",
];
/// The classes the Pro pass walks unless `custom_class_mode` numbers them.
const PRO_CLASSES: [&str; 10] = [
    "customClass1",
    "customClass2",
    "customClass3",
    "customClass4",
    "customClass5",
    "prestigeClass1",
    "prestigeClass2",
    "prestigeClass3",
    "prestigeClass4",
    "prestigeClass5",
];
/// The dvars' registered defaults.
const MAX_ATTACHMENTS_PER_WEAPON: i32 = 2;
const ATTACHMENT_FILTER: i32 = 0;

/// One attachment of `mp/attachmentTable.csv`.
struct Attachment {
    /// Its index in `ATTACHMENTS`.
    id: usize,
    /// Column 3, the name its menus show.
    name: String,
    point: usize,
    cost: i32,
    /// The attachments it goes with, by index (the last, no attachment's,
    /// stands for an unknown name).
    compatible: [bool; NO_ATTACHMENT + 1],
}

/// The table's attachment rows (column 2 `attachment`), numbered from 0 in
/// column 0, at most 24.
fn attachments(table: &StringTable) -> Vec<Attachment> {
    let cell = |row: usize, column: usize| table.cell(row, column).unwrap_or("");
    let mut out = Vec::new();
    let mut read = 0;
    while read < NO_ATTACHMENT {
        let Some(row) = (0..table.rows).find(|&row| {
            cell(row, 2).eq_ignore_ascii_case("attachment") && cell(row, 0) == read.to_string()
        }) else {
            break;
        };
        read += 1;
        let id = index_of(&ATTACHMENTS, cell(row, 4)).unwrap_or(NO_ATTACHMENT);
        let point = index_of(&POINTS, cell(row, 1)).unwrap_or(NO_POINT);
        if point == NO_POINT {
            diag::warn!(
                Sim,
                "Unrecognized attachpoint name '{}' in {ATTACHMENT_TABLE} row {row}.",
                cell(row, 1)
            );
        }
        let mut compatible = [false; NO_ATTACHMENT + 1];
        for name in cell(row, 11).split_whitespace() {
            let other = index_of(&ATTACHMENTS, name).unwrap_or_else(|| {
                diag::warn!(
                    Sim,
                    "Invalid attachment name '{name}' in table '{ATTACHMENT_TABLE}'"
                );
                NO_ATTACHMENT
            });
            compatible[other] = true;
        }
        if id != NO_ATTACHMENT {
            out.push(Attachment {
                id,
                name: cell(row, 3).to_owned(),
                point,
                cost: super::iw4::atoi(cell(row, 12)),
                compatible,
            });
        }
    }
    out
}

fn index_of(names: &[&str], name: &str) -> Option<usize> {
    (!name.is_empty())
        .then(|| names.iter().position(|n| n.eq_ignore_ascii_case(name)))
        .flatten()
}

fn point_name(point: i32) -> &'static str {
    // Black Ops formats a point past the table from whatever follows it;
    // 5 (no point) prints as a null string.
    usize::try_from(point)
        .ok()
        .and_then(|point| POINTS.get(point))
        .copied()
        .unwrap_or("(null)")
}

struct Cac {
    layout: Arc<BitLayout>,
    /// The profile's client buffer.
    buffer: Vec<u8>,
    /// The item table, by index.
    items: Vec<Item>,
    attachments: Vec<Attachment>,
    /// `mp/rankTable.csv`'s rows: the least and most XP of a rank, and the
    /// rank.
    ranks: Vec<[i32; 3]>,
    dvars: BTreeMap<String, String>,
    messages: RefCell<Vec<String>>,
}

impl Cac {
    fn say(&self, message: String) {
        diag::warn!(Sim, "{message}");
        self.messages.borrow_mut().push(message);
    }

    fn dvar(&self, name: &str, default: i32) -> i32 {
        self.dvars
            .get(&name.to_ascii_lowercase())
            .map_or(default, |v| super::iw4::atoi(v))
    }

    fn get(&self, n: i32) -> Option<&Item> {
        self.items.iter().find(|item| item.index == n)
    }

    /// A valid item, or none with the game's warning for an index outside
    /// the table.
    fn item(&self, n: i32) -> Option<&Item> {
        if !(0..MAX_ITEMS).contains(&n) {
            self.say(format!("Invalid unlockable item {n} returning NULL"));
            return None;
        }
        self.get(n)
    }

    /// The item named `name`, -1 for none.
    fn index_by_name(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        self.items
            .iter()
            .find(|item| item.name.eq_ignore_ascii_case(name))
            .map_or(-1, |item| item.index)
    }

    fn walk(&self, path: &[&str]) -> Option<Cursor> {
        path.iter()
            .try_fold(ROOT, |at, name| step_name(&self.layout, at, name))
    }

    fn read(&self, at: Cursor) -> u32 {
        leaf(&self.layout, at)
            .and_then(|m| read_int(m, at, &self.buffer))
            .unwrap_or(0)
    }

    /// Every item is free: a hosted online session (a private match, Combat
    /// Training or the theater, not Combat Training itself) whose menus
    /// set `ui_items_no_cost`.
    fn all_unlocked(&self) -> bool {
        let online = self.dvar("onlinegame", 0) != 0;
        let training = self.dvar("xblive_basictraining", 0) != 0;
        let hosted = online
            && (self.dvar("xblive_privatematch", 0) != 0
                || training
                || self.dvar("xblive_theater", 0) != 0);
        if online && !hosted || training {
            return false;
        }
        hosted && self.dvar("ui_items_no_cost", 0) > 0
    }

    /// The rank the profile's XP reaches, by a binary search over the rank
    /// table (its first row past the last searched, as Black Ops counts).
    fn rank(&self) -> i32 {
        if self.ranks.is_empty() {
            return 0;
        }
        let xp = self
            .walk(&["PlayerStatsList", "RANKXP"])
            .map_or(0, |at| self.read(at)) as i32;
        let (mut low, mut high) = (0i32, self.ranks.len() as i32 - 1);
        let mut mid = high / 2;
        while low < high {
            let [min, max, _] = self.ranks[mid as usize];
            if xp < min {
                high = mid - 1;
            } else if xp < max {
                break;
            } else {
                low = mid + 1;
            }
            mid = (high - low) / 2 + low;
        }
        self.ranks
            .get(mid.max(0) as usize)
            .map_or(0, |&[_, _, rank]| rank)
    }

    fn prestige(&self) -> i32 {
        self.walk(&["PlayerStatsList", "PLEVEL"])
            .map_or(0, |at| self.read(at)) as i32
    }

    /// The profile's rank or prestige is below the item's.
    fn locked(&self, n: i32) -> bool {
        if self.all_unlocked() {
            return false;
        }
        let (prestige, rank) = (self.prestige(), self.rank());
        let Some(item) = self.item(n) else {
            return true;
        };
        !(item.unlock_rank <= rank && item.unlock_prestige <= prestige)
    }

    /// The item costs nothing or the profile bought it.
    fn purchased(&self, n: i32) -> bool {
        if self.all_unlocked() {
            return true;
        }
        let Some(item) = self.item(n) else {
            return false;
        };
        if item.cost == 0 {
            return true;
        }
        match self.walk(&["itemStats", &n.to_string(), "purchased"]) {
            Some(at) => self.read(at) != 0,
            None => {
                self.say(format!("DDL: Error could not find item {n}"));
                false
            }
        }
    }

    fn unlocked(&self, n: i32) -> bool {
        self.all_unlocked() || !self.locked(n) && self.purchased(n)
    }

    /// The table's first attachment `id`, with its position.
    fn attachment(&self, id: usize) -> Option<(usize, &Attachment)> {
        self.attachments
            .iter()
            .enumerate()
            .find(|(_, attachment)| attachment.id == id)
    }

    fn point_of(&self, id: usize) -> usize {
        self.attachment(id).map_or(NO_POINT, |(_, a)| a.point)
    }

    fn compatible(&self, a: usize, b: usize) -> bool {
        if a == NO_ATTACHMENT || b == NO_ATTACHMENT {
            return false;
        }
        if a == 0 || b == 0 {
            return true;
        }
        let (Some((_, first)), Some((_, second))) = (self.attachment(a), self.attachment(b)) else {
            return false;
        };
        first.compatible[b] && second.compatible[a]
    }

    /// Attachment `att` of item `n` (its position among the item's) is free
    /// or bought.
    fn attachment_unlocked(&self, n: i32, att: usize) -> bool {
        if self.all_unlocked() {
            return true;
        }
        if self.locked(n) || !self.unlocked(n) {
            return false;
        }
        let Some(item) = self.item(n) else {
            return false;
        };
        if att >= item.attachments.len() {
            return false;
        }
        let id = if att == 0 { 0 } else { item.attachments[att] };
        let cost = self
            .attachment(id)
            .filter(|(row, _)| *row != NO_ATTACHMENT)
            .map_or(0, |(_, a)| a.cost);
        if cost == 0 {
            return true;
        }
        let path = [
            "weaponStats",
            &n.to_string(),
            "purchasedattachments",
            &att.to_string(),
        ];
        match self.walk(&path) {
            Some(at) => self.read(at) != 0,
            None => {
                self.say(format!(
                    "DDL: Error could not find item {n} attachment {att}"
                ));
                false
            }
        }
    }

    /// Item `n`'s Pro version is unlocked: by the game settings in the
    /// menus' free mode, else by its three Pro challenges.
    fn pro_unlocked(&self, n: i32) -> bool {
        if self.dvar("ui_items_no_cost", 0) != 0 {
            return self.dvar("custom_perks_allow_pro", 0) != 0;
        }
        if self.all_unlocked() {
            return true;
        }
        if !self.unlocked(n) {
            return false;
        }
        (0..3).all(|challenge| {
            let path = [
                "itemStats",
                &n.to_string(),
                "isProVersionUnlocked",
                &challenge.to_string(),
            ];
            match self.walk(&path) {
                Some(at) => self.read(at) != 0,
                None => {
                    self.say(format!(
                        "DDL: Error could not find item version unlocked {n} challenge {challenge}"
                    ));
                    false
                }
            }
        })
    }

    /// A perk's Pro version: itself when it is one, else the item after it
    /// when that is; -1 for none.
    fn pro_version(&self, n: i32) -> i32 {
        let is_pro = |n: i32| {
            self.item(n)
                .is_some_and(|item| item.group == SPECIALTY_GROUP && item.pro)
        };
        if is_pro(n) {
            return n;
        }
        if self
            .item(n)
            .is_some_and(|item| item.group == SPECIALTY_GROUP)
            && is_pro(n + 1)
        {
            return n + 1;
        }
        -1
    }

    /// `cacLoadouts.<class>.<slot>`, or `cacLoadouts.<slot>` with no class.
    fn slot(&self, class: Option<&str>, slot: &str) -> Option<Cursor> {
        let at = match class {
            Some(class) => self.walk(&["CacLoadouts", class, slot]),
            None => self.walk(&["CacLoadouts", slot]),
        };
        if at.is_none() {
            self.say(format!("DDL: Error could not find loadout slot {slot}"));
        }
        at
    }

    fn custom_gametype_classes(&self) -> bool {
        let custom = self.dvar("ui_useCustomClassInfo", 0) != 0;
        if custom {
            self.say(
                "unknown!t5.mp.cac_custom_gametype_class: ui_useCustomClassInfo edits the custom game mode's class table, not read yet"
                    .into(),
            );
        }
        custom
    }

    fn read_slot(&self, class: Option<&str>, slot: &str) -> u32 {
        if self.custom_gametype_classes() {
            return 0;
        }
        self.slot(class, slot).map_or(u32::MAX, |at| self.read(at))
    }

    /// The slot takes the item, and what goes with the old one is reset
    /// first.
    fn write_slot(&mut self, class: Option<&str>, item: u32, slot: &str) {
        if self.custom_gametype_classes() {
            return;
        }
        let Some(at) = self.slot(class, slot) else {
            return;
        };
        if self.read(at) == item {
            return;
        }
        self.reset_dependents(class, item as i32, slot);
        if !set_raw_int(&self.layout, &mut self.buffer, at, item) {
            let name = self
                .item(item as i32)
                .map_or_else(String::new, |item| item.name.clone());
            self.say(format!(
                "DDL: Error equipping class {} with item {name}",
                class.unwrap_or("")
            ));
        }
    }

    /// A body to go with the item: the first body of the primary weapon's
    /// group (`bodyTypeFromGun`), else the first whose perk the first perk
    /// starts with.
    fn body_for(&self, n: i32) -> i32 {
        let from_gun = self.dvar("bodyTypeFromGun", 0) != 0;
        let Some(item) = self.item(n) else {
            return -1;
        };
        let bodies = || {
            self.items
                .iter()
                .filter(|body| !body.null && body.slot == Some(BODY))
        };
        let found = if from_gun {
            let group = usize::try_from(item.group)
                .ok()
                .filter(|&g| g < ITEM_GROUPS && item.slot == Some(PRIMARY))
                .map(|g| super::t5_stats_reset::ITEM_GROUPS[g]);
            group.and_then(|group| {
                bodies().find(|body| {
                    !body.body_group.is_empty() && body.body_group.eq_ignore_ascii_case(group)
                })
            })
        } else if item.slot == Some(SPECIALTY1) && !item.name.is_empty() {
            bodies().find(|body| {
                let prefix = body.body_perk.as_bytes();
                !prefix.is_empty()
                    && item
                        .name
                        .as_bytes()
                        .get(..prefix.len())
                        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
            })
        } else {
            None
        };
        found.map_or(-1, |body| body.index)
    }

    /// What a new weapon or perk resets: a weapon its body, attachments,
    /// camo, reticle, lens, tag and emblem; a perk its body.
    fn reset_dependents(&mut self, class: Option<&str>, item: i32, slot: &str) {
        let weapon = slot.eq_ignore_ascii_case("primary") || slot.eq_ignore_ascii_case("secondary");
        let perk = slot
            .as_bytes()
            .get(..9)
            .is_some_and(|start| start.eq_ignore_ascii_case(b"specialty"));
        if !weapon && !perk {
            return;
        }
        let body = self.body_for(item);
        if body != -1 {
            self.write_slot(class, body as u32, SLOTS[BODY]);
        }
        if !weapon {
            return;
        }
        for point in 1..5 {
            self.write_slot(class, 0, &format!("{slot}attachment{}", point_name(point)));
        }
        for part in ["camo", "reticle", "reticlecolor", "lens", "tag", "emblem"] {
            self.write_slot(class, 0, &format!("{slot}{part}"));
        }
    }

    /// `equipClass`' and `equipClassItem`'s write: the item into its slot.
    fn equip(&mut self, class: &str, n: i32) {
        if !(0..MAX_ITEMS).contains(&n) {
            self.say(format!("Invalid unlockable item {n} returning NULL"));
            self.say(format!("Item {n} is invalid"));
            return;
        }
        let Some(item) = self.get(n) else {
            self.say(format!("Item {n} is invalid"));
            return;
        };
        let Some(slot) = item.slot else {
            let name = item.name.clone();
            self.say(format!("Item {name} has an invalid loadout slot"));
            return;
        };
        self.write_slot(Some(class), n as u32, SLOTS[slot]);
    }

    /// The item into its slot, and its attachment `attachment` at `point`
    /// (blank for the slot's single attachment): an attachment already on
    /// is taken off; one that clashes with another on, or whose point is
    /// taken, is not put on.
    fn equip_attachment(&mut self, class: &str, name: &str, attachment: &str, point: &str) {
        let n = self.index_by_name(name);
        if n == -1 || !self.unlocked(n) {
            return;
        }
        let Some(item) = self.item(n) else {
            return;
        };
        let Some(slot) = item.slot else {
            return;
        };
        let attachments = item.attachments.clone();
        let Some(att) = index_of(&ATTACHMENTS, attachment)
            .and_then(|id| attachments.iter().position(|&a| a == id))
        else {
            return;
        };
        if !self.attachment_unlocked(n, att) {
            return;
        }
        let slot = SLOTS[slot];
        self.write_slot(Some(class), n as u32, slot);
        let id = attachments[att];
        let at_point = self.point_of(id);
        let mut on = 0;
        for i in 1..5 {
            let key = format!("{slot}attachment{}", point_name(i));
            let current = self.read_slot(Some(class), &key);
            // A slot outside the weapon's attachments reads past them in
            // Black Ops; it counts as none here.
            let current_id = attachments.get(current as usize).copied().unwrap_or(0);
            if current as usize == att {
                self.write_slot(Some(class), 0, &key);
                return;
            }
            if current_id != 0 {
                on += 1;
                if i as usize == at_point {
                    return;
                }
            }
            if !self.compatible(id, current_id) {
                return;
            }
        }
        let max = self.dvar("maxAttachmentsPerWeapon", MAX_ATTACHMENTS_PER_WEAPON);
        let key = if att != 0 && on < max {
            format!("{slot}attachment{point}")
        } else {
            let filter = self.dvar("attachmentFilter", ATTACHMENT_FILTER);
            format!("{slot}attachment{}", point_name(filter))
        };
        self.write_slot(Some(class), att as u32, &key);
    }

    /// A slot's item when a default class names none.
    fn slot_default(&self, slot: usize) -> u32 {
        if slot >= SLOTS.len() {
            return 0;
        }
        slot_default(&self.items, slot) as u32
    }

    /// `equipdefaultclass`: the default class's items in each of a class's
    /// slots, then the Pro pass.
    fn equip_default_class(&mut self, class: &str, row: usize) {
        let table = self.default_table();
        for slot in 0..CLASS_SLOTS {
            let item = match table[row * CLASS_SLOTS + slot] {
                -1 => self.slot_default(slot),
                item => item as u32,
            };
            self.write_slot(Some(class), item, SLOTS[slot]);
        }
        self.pro_pass();
    }

    /// `equipdefaultglobalitems`: the default killstreaks and deathstreak.
    fn equip_default_global_items(&mut self) {
        let table = self.default_table();
        for slot in CLASS_SLOTS..SLOTS.len() {
            let item = match table[GLOBAL_ROW + slot] {
                -1 => self.slot_default(slot),
                item => item as u32,
            };
            self.write_slot(None, item, SLOTS[slot]);
        }
    }

    fn default_table(&self) -> Vec<i32> {
        default_table(&self.items)
    }

    /// The four attachment points of the item's slot, or one of them,
    /// cleared.
    fn clear_attachments(&mut self, class: &str, n: i32, point: Option<i32>) {
        if !(0..MAX_ITEMS).contains(&n) {
            self.say(format!("Invalid unlockable item {n} returning NULL"));
            return;
        }
        let Some(slot) = self.get(n).and_then(|item| item.slot) else {
            return;
        };
        let slot = SLOTS[slot];
        let points = match point {
            Some(point) => vec![point],
            None => (1..5).collect(),
        };
        for point in points {
            self.write_slot(
                Some(class),
                0,
                &format!("{slot}attachment{}", point_name(point)),
            );
        }
    }

    /// The item is still marked new: never once everything is free, or
    /// while it is locked.
    fn is_new(&self, n: i32) -> bool {
        if self.all_unlocked() || self.locked(n) || self.item(n).is_none() {
            return false;
        }
        match self.walk(&["itemStats", &n.to_string(), "new"]) {
            Some(at) => self.read(at) != 0,
            None => {
                self.say(format!("DDL: Error could not find item {n}"));
                false
            }
        }
    }

    fn clear_new(&mut self, n: i32) {
        if !self.is_new(n) {
            return;
        }
        let Some(at) = self.walk(&["itemStats", &n.to_string(), "new"]) else {
            self.say(format!("DDL: Error could not find item {n}"));
            return;
        };
        if !set_raw_int(&self.layout, &mut self.buffer, at, 0) {
            self.say(format!(
                "DDL: Error could not set purchased flag for item {n}"
            ));
        }
    }

    /// Each class's perks become their Pro versions where those are
    /// unlocked.
    fn pro_pass(&mut self) {
        let numbered = self.dvar("custom_class_mode", 0) != 0;
        for (c, name) in PRO_CLASSES.iter().enumerate() {
            let class = if numbered {
                (c + 1).to_string()
            } else {
                (*name).to_owned()
            };
            for slot in PERK_SLOTS {
                let n = self.read_slot(Some(&class), SLOTS[slot]) as i32;
                let usable = self.get(n).is_some_and(|item| !item.null);
                if !usable || !self.pro_unlocked(n) {
                    continue;
                }
                let pro = self.pro_version(n);
                if pro != -1 && self.unlocked(pro) && pro != n {
                    self.write_slot(Some(&class), pro as u32, SLOTS[slot]);
                }
            }
        }
    }
}

fn arg(args: &[String], n: usize) -> &str {
    args.get(n).map_or("", String::as_str)
}

/// Runs a create-a-class command (`name` lowercased) on the local profile's
/// stats: `None` when `name` is none of them, else the messages the game
/// prints, or why it could not run.
pub(crate) fn run(
    world: &mut World,
    name: &str,
    args: &[String],
) -> Option<Result<Vec<String>, String>> {
    if !matches!(
        name,
        "equipclass"
            | "equipclassitem"
            | "equipclassattachment"
            | "copyclass"
            | "equipdefaultclass"
            | "equipdefaultitemtoslot"
            | "equipdefaultglobalitemtoslot"
            | "equipdefaultglobalitems"
            | "clearitemnew"
            | "clearallitemsnew"
            | "clearallattachmentsforitem"
            | "clearattachmentforitematpoint"
            | "statwriteddl"
            | "statsetbyname"
            | "uploadstats"
    ) {
        return None;
    }
    Some(run_command(world, name, args))
}

fn run_command(world: &mut World, name: &str, args: &[String]) -> Result<Vec<String>, String> {
    let mut cac = build(world)?;
    let before = cac.buffer.clone();
    run_on(world, &mut cac, name, args);
    finish(world, cac, &before)
}

/// The create-a-class state of the local profile: its stats (rebuilt as a
/// match's first read rebuilds them), items, attachments, ranks and dvars.
fn build(world: &mut World) -> Result<Cac, String> {
    let layout = super::t5_stats::layout(world)
        .ok_or("the match has no ddl_mp/stats.ddl (code_post_gfx_mp)")?;
    let bytes = super::t5_stats::profile_bytes(world, &layout)
        .ok_or("the match keeps no local profile stats")?;
    let runtime = world.resource::<RoundScript>();
    let table = |name: &str| runtime.tables.get(name);
    let ranks = table(RANK_TABLE).map_or_else(Vec::new, |table| {
        (0..table.rows)
            .map(|row| {
                [2, 7, 0].map(|column| super::iw4::atoi(table.cell(row, column).unwrap_or("")))
            })
            .collect()
    });
    let cac = Cac {
        buffer: bytes[..BUFFER_BYTES.min(bytes.len())].to_vec(),
        layout,
        items: table(STATS_TABLE).map(items).unwrap_or_else(Vec::new),
        attachments: table(ATTACHMENT_TABLE).map_or_else(Vec::new, attachments),
        ranks,
        dvars: world.resource::<MatchScript>().dvars.clone(),
        messages: RefCell::new(Vec::new()),
    };
    Ok(cac)
}

fn run_on(world: &mut World, cac: &mut Cac, name: &str, args: &[String]) {
    match name {
        // `statWriteDDL <path…> <value>`: the member the path names takes
        // the value.
        "statwriteddl" if args.len() < 2 => {
            cac.say("DDL: statsDDLTest usage: statsDDLTest <path>".into());
        }
        "statwriteddl" => {
            let (value, path) = args.split_last().unwrap_or((&args[0], &[]));
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            match cac.walk(&path) {
                Some(at) => {
                    let value = super::iw4::atoi(value) as u32;
                    if !set_raw_int(&cac.layout, &mut cac.buffer, at, value) {
                        cac.say(format!("DDL: could not write {}", path.join(".")));
                    }
                }
                None => cac.say(format!("DDL: Member '{}' not found.", path.join("."))),
            }
        }
        // `statSetByName <name> <value>`: `PlayerStatsList.<name>`.
        "statsetbyname" if args.is_empty() => {
            cac.say("statsetbyname usage: statset <statname> <value>".into());
        }
        "statsetbyname" => match cac.walk(&["PlayerStatsList", arg(args, 0)]) {
            Some(at) => {
                let value = super::iw4::atoi(arg(args, 1)) as u32;
                set_raw_int(&cac.layout, &mut cac.buffer, at, value);
            }
            None => cac.say(format!("DDL: Member '{}' not found.", arg(args, 0))),
        },
        // Black Ops sends the stats to its online service; the profile's
        // stats are saved on this machine as they change.
        "uploadstats" => {}
        "equipclass" if args.len() < 2 => {
            cac.say("equipclass usage: equipclass <customclassname> <item>".into());
        }
        "equipclass" => {
            let n = cac.index_by_name(arg(args, 1));
            if n != -1 && cac.unlocked(n) {
                cac.equip(arg(args, 0), n);
            }
        }
        "equipclassitem" if args.len() < 2 => {
            cac.say("equipclassitem usage: equipclassitem <customclassname> <itemindex>".into());
        }
        "equipclassitem" => {
            let class = arg(args, 0);
            let n = super::iw4::atoi(arg(args, 1));
            if n != -1 && cac.unlocked(n) {
                // A weapon with no attachment goes on with none.
                let bare = cac.item(n).and_then(|item| {
                    (matches!(item.slot, Some(PRIMARY | SECONDARY)) && item.attachments.len() == 1)
                        .then(|| item.name.clone())
                });
                match bare {
                    Some(weapon) => cac.equip_attachment(class, &weapon, "none", ""),
                    None if cac.get(n).is_some() => cac.equip(class, n),
                    None => {}
                }
            }
        }
        "equipclassattachment" if args.len() < 3 => cac.say(
            "equipclassattachment usage: equipclassattachment <customclassname> <item> <attachment>"
                .into(),
        ),
        "equipclassattachment" => {
            cac.equip_attachment(arg(args, 0), arg(args, 1), arg(args, 2), arg(args, 3));
        }
        "equipdefaultclass" if args.len() < 2 => cac.say(
            "equipdefaultclass usage: equipdefaultclass <customclassname> <defaultClass>".into(),
        ),
        "equipdefaultclass" => {
            if let Some(row) = DEFAULT_CLASSES
                .iter()
                .position(|class| class.eq_ignore_ascii_case(arg(args, 1)))
            {
                cac.equip_default_class(arg(args, 0), row);
            }
        }
        "equipdefaultitemtoslot" if args.len() < 2 => cac.say(
            "equipdefaultitemtoslot usage: equipdefaultitemtoslot <customclassname> <slotname>"
                .into(),
        ),
        "equipdefaultglobalitemtoslot" if args.is_empty() => cac.say(
            "equipdefaultglobalitemtoslot usage: equipdefaultglobalitemtoslot <slotname>".into(),
        ),
        "equipdefaultitemtoslot" | "equipdefaultglobalitemtoslot" => {
            let (class, slot) = if name == "equipdefaultitemtoslot" {
                (Some(arg(args, 0)), arg(args, 1))
            } else {
                (None, arg(args, 0))
            };
            if let Some(index) = index_of(&SLOTS, slot) {
                let item = cac.slot_default(index);
                cac.write_slot(class, item, slot);
            }
        }
        "equipdefaultglobalitems" => cac.equip_default_global_items(),
        "clearitemnew" if args.is_empty() => {
            cac.say("clearitemnew usage: clearitemnew <itemname>".into());
        }
        "clearitemnew" => {
            let n = cac.index_by_name(arg(args, 0));
            if n != -1 {
                cac.clear_new(n);
            }
        }
        "clearallitemsnew" => {
            let all: Vec<i32> = cac
                .items
                .iter()
                .filter(|item| !item.null)
                .map(|item| item.index)
                .collect();
            for n in all {
                cac.clear_new(n);
            }
        }
        "clearallattachmentsforitem" if args.len() < 2 => cac.say(
            "clearAllAttachmentsForItem usage: clearAllAttachmentsForItem <customClassName> <itemIndex>"
                .into(),
        ),
        "clearallattachmentsforitem" => {
            let n = super::iw4::atoi(arg(args, 1));
            cac.clear_attachments(arg(args, 0), n, None);
        }
        "clearattachmentforitematpoint" if args.len() < 3 => cac.say(
            "clearAttachmentForItemAtPoint usage: clearAttachmentForItemAtPoint <customClassName> <itemIndex> <attachmentPoint>"
                .into(),
        ),
        "clearattachmentforitematpoint" => {
            let n = super::iw4::atoi(arg(args, 1));
            let point = super::iw4::atoi(arg(args, 2));
            cac.clear_attachments(arg(args, 0), n, Some(point));
        }
        "copyclass" if args.len() < 2 => {
            cac.say(
                "copyclass usage: copyclass <classIndexToCopy> <classIndexToReplace>".into(),
            );
        }
        _ => {
            let from = super::iw4::atoi(arg(args, 0));
            let mut to = super::iw4::atoi(arg(args, 1));
            if from <= to {
                to += 1;
            }
            let class = |n: i32| usize::try_from(n).ok().and_then(|n| COPY_CLASSES.get(n));
            if let (Some(&from), Some(&to)) = (class(from), class(to)) {
                for slot in SLOTS {
                    let item = cac.read_slot(Some(from), slot);
                    cac.write_slot(Some(to), item, slot);
                }
                cac.pro_pass();
                // The class's name goes with it.
                let dvars = &mut world.resource_mut::<MatchScript>().dvars;
                let name = dvars.get(from).cloned().unwrap_or_else(String::new);
                dvars.insert(to.to_owned(), name);
            }
        }
    }
}

/// Saves what a command changed in the profile's stats; the messages the
/// game printed.
fn finish(world: &mut World, cac: Cac, before: &[u8]) -> Result<Vec<String>, String> {
    let messages = cac.messages.into_inner();
    if cac.buffer != *before {
        if super::t5_stats::profile_received(world) {
            diag::warn!(
                Sim,
                "unknown!t5.mp.cac_match_resend: the host already holds this profile's stats; how Black Ops sends it an edited class is not read yet, the next match has it"
            );
        }
        let mut stored = world.resource_mut::<crate::ProfileStats>();
        if let Some(bytes) = stored.bytes.as_mut() {
            bytes[..cac.buffer.len()].copy_from_slice(&cac.buffer);
        }
        stored.revision += 1;
    }
    Ok(messages)
}

/// What Black Ops' menus read of the local profile while it plays this
/// level: its stats (the client buffer `getDStat` reads, as the
/// create-a-class commands keep it), its items and the level's string
/// tables.
pub struct T5MenuData {
    cac: Cac,
    tables: Arc<BTreeMap<String, StringTable>>,
}

/// One step of a stats path a menu names: a member, or an array index.
pub enum MenuStatKey<'a> {
    Name(&'a str),
    Index(i32),
}

/// A stat as a menu reads it: an integer, or text (a 64-bit value or a
/// string member).
pub enum MenuStat {
    Int(i32),
    Text(String),
}

impl T5MenuData {
    /// `getDStat(path…)`: members of types 0 to 2 read as integers, type 3
    /// as its 64-bit value's digits, type 5 as its string.
    pub fn stat(&self, path: &[MenuStatKey<'_>]) -> Result<MenuStat, String> {
        use super::t5_stats::{MAX_PATH, member, read_string, read_u64, walk};
        if path.len() > MAX_PATH {
            return Err("getDStat: path is too deep.".into());
        }
        let keys: Vec<crate::script::Value> = path
            .iter()
            .map(|key| match key {
                MenuStatKey::Name(name) => crate::script::Value::String((*name).into()),
                MenuStatKey::Index(n) => crate::script::Value::Int(*n),
            })
            .collect();
        let (layout, buffer) = (&self.cac.layout, &self.cac.buffer);
        let at = walk(layout, &keys, "getDStat")?;
        let Some(m) = member(layout, at.member) else {
            return Err("getDStat: could not find ddl member.".into());
        };
        let leaf = leaf(layout, at);
        Ok(match m.kind {
            0..=2 => MenuStat::Int(
                leaf.and_then(|leaf| read_int(leaf, at, buffer))
                    .unwrap_or(0) as i32,
            ),
            3 => MenuStat::Text(
                leaf.and_then(|leaf| read_u64(leaf, at, buffer))
                    .unwrap_or(0)
                    .to_string(),
            ),
            5 => {
                MenuStat::Text(leaf.map_or_else(String::new, |leaf| read_string(leaf, at, buffer)))
            }
            _ => return Err("getDStat: stat type undefined".into()),
        })
    }

    /// `getItemIndex(name)`: the item of that name, -1 for none.
    pub fn item_index(&self, name: &str) -> i32 {
        self.cac.index_by_name(name)
    }

    /// `isItemLocked(n)`: the profile's rank or prestige is below the
    /// item's (free items and purchases do not count here).
    pub fn item_locked(&self, n: i32) -> bool {
        let (prestige, rank) = (self.cac.prestige(), self.cac.rank());
        self.cac
            .item(n)
            .is_none_or(|item| !(item.unlock_rank <= rank && item.unlock_prestige <= prestige))
    }

    /// `isItemNew(n)`.
    pub fn item_new(&self, n: i32) -> bool {
        self.cac.is_new(n)
    }

    /// `getItemName(n)`: the item's name, empty for none.
    pub fn item_name(&self, n: i32) -> String {
        self.cac
            .item(n)
            .map_or_else(String::new, |item| item.name.clone())
    }

    /// `getItemImage(n)`: column 6 of a usable item (and of item 0), empty
    /// otherwise.
    pub fn item_image(&self, n: i32) -> String {
        let usable = self.cac.get(n).is_some_and(|item| !item.null);
        if !usable && n != 0 {
            return String::new();
        }
        self.cac
            .item(n)
            .map_or_else(String::new, |item| item.body_perk.clone())
    }

    /// `getItemEquippedAttachment(class, n, k)`: the `k`th attachment (by
    /// point, top to muzzle) the class has on item `n`, -1 for none.
    pub fn equipped_attachment(&self, class: &str, n: i32, k: i32) -> i32 {
        let Some(item) = self.cac.get(n).filter(|item| !item.null) else {
            return -1;
        };
        let Some(slot) = item.slot.and_then(|slot| SLOTS.get(slot)) else {
            return -1;
        };
        let mut k = k;
        for point in 1..5 {
            let key = format!("{slot}attachment{}", point_name(point));
            let at = self.cac.read_slot(Some(class), &key) as usize;
            if at == 0 {
                continue;
            }
            let Some(&id) = item.attachments.get(at) else {
                self.cac.say(format!(
                    "Item {} does not have attachment number {at} it only has {} attachments",
                    item.name,
                    item.attachments.len()
                ));
                continue;
            };
            if id == 0 {
                continue;
            }
            if k == 0 {
                return id as i32;
            }
            k -= 1;
        }
        -1
    }

    /// One of the create-a-class menu functions by its lowercased name:
    /// `isitemlocked`, `isitemnew`, `getitemname`, `getitemimage`,
    /// `getitemequippedattachment`; `None` for another name.
    pub fn call(&self, name: &str, args: &[MenuStatKey<'_>]) -> Option<MenuStat> {
        let int = |n: usize| match args.get(n) {
            Some(MenuStatKey::Index(n)) => *n,
            Some(MenuStatKey::Name(text)) => super::iw4::atoi(text),
            None => 0,
        };
        let text = |n: usize| match args.get(n) {
            Some(MenuStatKey::Name(text)) => (*text).to_owned(),
            Some(MenuStatKey::Index(n)) => n.to_string(),
            None => String::new(),
        };
        Some(match name {
            "isitemlocked" => MenuStat::Int(i32::from(self.item_locked(int(0)))),
            "isitemnew" => MenuStat::Int(i32::from(self.item_new(int(0)))),
            "getitemname" => MenuStat::Text(self.item_name(int(0))),
            "getitemimage" => MenuStat::Text(self.item_image(int(0))),
            "getitemequippedattachment" => {
                MenuStat::Int(self.equipped_attachment(&text(0), int(1), int(2)))
            }
            // The attachment table's name of attachment `id`.
            "getattachmentname" => MenuStat::Text(
                usize::try_from(int(0))
                    .ok()
                    .filter(|id| *id < NO_ATTACHMENT)
                    .and_then(|id| self.cac.attachment(id))
                    .map_or_else(String::new, |(_, attachment)| attachment.name.clone()),
            ),
            // Some item of the group is new; never once everything is free.
            "isitemgroupnew" => {
                let group = super::t5_stats_reset::ITEM_GROUPS
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(&text(0)));
                let new = match group {
                    _ if self.cac.all_unlocked() => false,
                    None => {
                        self.cac.say(format!("Item Group {} not found", text(0)));
                        false
                    }
                    Some(group) => self
                        .cac
                        .items
                        .iter()
                        .any(|item| item.group == group as i32 && self.cac.is_new(item.index)),
                };
                MenuStat::Int(i32::from(new))
            }
            _ => return None,
        })
    }

    /// `tableLookup(table, column, value, result column)`.
    pub fn table_lookup(&self, name: &str, column: i32, value: &str, result: i32) -> String {
        let Some(table) = crate::script::host::tables::table(&self.tables, name) else {
            return String::new();
        };
        if column < 0 || result < 0 {
            return String::new();
        }
        crate::script::host::tables::table_search(table, column as usize, value)
            .and_then(|row| table.cell(row, result as usize))
            .unwrap_or("")
            .to_owned()
    }
}

/// The local profile's menu data, `dvars` (the menus' own) over the
/// match's; `None` on a level without Black Ops' multiplayer stats layout or
/// without a local profile.
pub(crate) fn menu_data(world: &mut World, dvars: &[(String, String)]) -> Option<T5MenuData> {
    let mut cac = build(world).ok()?;
    for (name, value) in dvars {
        cac.dvars.insert(name.to_ascii_lowercase(), value.clone());
    }
    Some(T5MenuData {
        cac,
        tables: world.resource::<RoundScript>().tables.clone(),
    })
}
