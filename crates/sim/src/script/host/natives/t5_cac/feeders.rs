//! The create-a-class list boxes' feeders, as BlackOpsMP.exe answers them:
//! the item lists the `buildItemListFor*` commands fill, and each feeder's
//! count, selection and row cells.

use super::super::t5_stats_reset::ITEM_GROUPS;
use super::{Cac, NO_ATTACHMENT, SPECIALTY_GROUP, T5MenuData};

/// `perk_list_*`.
const FEEDER_PERKS: i32 = 0x31;
const FEEDER_ATTACHMENTS: i32 = 0x39;
const FEEDER_ITEMS: i32 = 0x3a;
const FEEDER_WEAPONS: i32 = 0x3b;
const FEEDER_OPTIONS: i32 = 0x3c;
const FEEDER_CUSTOM_KILLSTREAKS: i32 = 0x3d;
const FEEDER_SORTED_A: i32 = 0x41;
const FEEDER_SORTED_B: i32 = 0x42;
const FEEDER_KILLSTREAKS: i32 = 0x43;
const FEEDER_WEAPON_GROUPS: i32 = 0x4e;
const FEEDER_CHALLENGE_PERKS: i32 = 0x61;
const FEEDER_CHALLENGE_ITEMS: i32 = 100;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// A row's band when the cursor is elsewhere.
const BAND: [f32; 4] = [0.0, 0.0, 0.0, 0.2];
const AFFORDABLE: [f32; 4] = [0.42, 0.68, 0.46, 1.0];
const TOO_DEAR: [f32; 4] = [0.73, 0.19, 0.19, 1.0];
const REFUSED: [f32; 4] = [0.4, 0.4, 0.4, 1.0];

const IMAGE_WHITE: &str = "white";
const IMAGE_LOCKED: &str = "menu_mp_lobby_locked";
const IMAGE_NEW: &str = "menu_mp_lobby_new";
const IMAGE_EQUIPPED: &str = "menu_mp_killstreak_select";

const CLASSIFIED_NAME: &str = "MENU_CLASSIFIED";
const NO_KILLSTREAK_NAME: &str = "CUSTOM_NO_KILLSTREAK";
const COST_NAME: &str = "MENU_POINTS";
/// `ITEM_GROUPS`' group whose locked items read as classified.
const CLASSIFIED_GROUP: i32 = 0x11;
/// The option groups by `ui_currentWeaponOptionGroup`.
const OPTION_GROUPS: [&str; 6] = ["camo", "reticle", "lens", "tag", "emblem", "reticle_color"];

/// What a feeder reads besides the profile: the list the builders filled,
/// the menus' current item and the list box's cursor.
#[derive(Clone, Copy)]
pub struct FeederContext<'a> {
    pub list: &'a [i32],
    pub current_item: i32,
    pub cursor: i32,
    /// The list box has the focus.
    pub focused: bool,
}

/// What selecting a row sets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeederSelection {
    pub item: Option<i32>,
    pub attachment: Option<i32>,
    pub option: Option<i32>,
}

/// One column of one row.
#[derive(Clone, Debug, Default)]
pub struct FeederCell {
    /// A localized name, its digit argument, or nothing to draw.
    pub text: String,
    pub text_arg: Option<i32>,
    pub image: Option<&'static str>,
    pub color: Option<[f32; 4]>,
}

impl FeederCell {
    fn text(name: &str) -> Self {
        Self {
            text: name.to_owned(),
            ..Self::default()
        }
    }

    fn image(image: &'static str) -> Self {
        Self {
            image: Some(image),
            ..Self::default()
        }
    }

    fn cost(cost: i32) -> Self {
        Self {
            text: COST_NAME.to_owned(),
            text_arg: Some(cost),
            ..Self::default()
        }
    }

    fn colored(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }
}

/// A weapon option of `mp/attachmentTable.csv` (its rows `weaponoption`).
pub(super) struct WeaponOption {
    pub(super) group: usize,
    pub(super) name: String,
    /// Column 4.
    pub(super) reference: String,
    /// Column 6.
    pub(super) image: String,
    pub(super) cost: i32,
    pub(super) unlock_rank: i32,
    pub(super) unlock_prestige: i32,
}

/// The table's weapon options by their place, and each group's by order.
pub(super) fn weapon_options(table: &crate::script::StringTable) -> Vec<WeaponOption> {
    let cell = |row: usize, column: usize| table.cell(row, column).unwrap_or("");
    let mut options = Vec::new();
    for index in 0..0x48 {
        let Some(row) = (0..table.rows).find(|&row| {
            cell(row, 2).eq_ignore_ascii_case("weaponoption") && cell(row, 0) == index.to_string()
        }) else {
            break;
        };
        // A row of an unknown group keeps its place, in no group.
        let group = OPTION_GROUPS
            .iter()
            .position(|name| name.eq_ignore_ascii_case(cell(row, 1)))
            .unwrap_or_else(|| {
                diag::warn!(
                    Sim,
                    "Unrecognized weaponoption name '{}' in mp/attachmenttable.csv row {row}.",
                    cell(row, 1)
                );
                OPTION_GROUPS.len()
            });
        options.push(WeaponOption {
            group,
            name: cell(row, 3).to_owned(),
            reference: cell(row, 4).to_owned(),
            image: cell(row, 6).to_owned(),
            cost: crate::script::host::natives::iw4::atoi(cell(row, 12)),
            unlock_rank: crate::script::host::natives::iw4::atoi(cell(row, 13)),
            unlock_prestige: crate::script::host::natives::iw4::atoi(cell(row, 14)),
        });
    }
    options
}

impl Cac {
    /// The option group's `n`th option, by its place in the table.
    pub(super) fn option_in_group(&self, group: usize, n: i32) -> Option<usize> {
        let n = usize::try_from(n).ok()?;
        self.options
            .iter()
            .enumerate()
            .filter(|(_, option)| option.group == group)
            .map(|(index, _)| index)
            .nth(n)
    }

    /// The option is free, or everything is, or the profile bought it for
    /// this reachable weapon.
    pub(super) fn option_unlocked(&self, n: i32, option: usize) -> bool {
        if self.all_unlocked() || option == 0 {
            return true;
        }
        if option >= self.options.len() || self.locked(n) || !self.unlocked(n) {
            return false;
        }
        if self.options[option].cost == 0 {
            return true;
        }
        let path = [
            "weaponStats",
            &n.to_string(),
            "purchasedoptions",
            &option.to_string(),
        ];
        match self.walk(&path) {
            Some(at) => self.read(at) != 0,
            None => {
                self.say(format!("DDL: Error could not find item {n} camo {option}"));
                false
            }
        }
    }

    /// `equipClassCurrentAttachment`: the current item's selected attachment,
    /// on or off as `equipClassAttachment` puts it.
    pub(super) fn equip_current_attachment(&mut self, class: &str, n: i32, attachment: i32) {
        let Some(item) = self.item(n) else {
            return;
        };
        let Some(&id) = usize::try_from(attachment)
            .ok()
            .and_then(|at| item.attachments.get(at))
        else {
            return;
        };
        let Some(&name) = super::ATTACHMENTS.get(id) else {
            return;
        };
        let (item_name, point) = (
            item.name.clone(),
            super::point_name(self.point_of(id) as i32),
        );
        self.equip_attachment(Some(class), &item_name, name, point);
    }

    /// The current item's slot, while the item and the group's option at
    /// `row` can be used.
    fn option_slot(&self, n: i32, row: i32) -> Option<&'static str> {
        let item = self.item(n)?;
        let group = self.dvar("ui_currentWeaponOptionGroup", 0).clamp(0, 5) as usize;
        let option = self.option_in_group(group, row).unwrap_or(usize::MAX);
        if !self.unlocked(n) || !self.option_unlocked(n, option) {
            return None;
        }
        item.slot.and_then(|slot| super::SLOTS.get(slot)).copied()
    }

    /// `equipClassCurrentOption`: the current item in its slot, the selected
    /// row in `<slot><option name>`.
    pub(super) fn equip_current_option(&mut self, class: &str, n: i32, row: i32, name: &str) {
        let Some(slot) = self.option_slot(n, row) else {
            return;
        };
        self.write_slot(Some(class), n as u32, slot);
        self.write_slot(Some(class), row as u32, &format!("{slot}{name}"));
    }

    /// `toggleWeaponOption`: the same for the group's first option, which
    /// `<slot><option name>` turns on and off.
    pub(super) fn toggle_option(&mut self, class: &str, n: i32, name: &str) {
        let Some(slot) = self.option_slot(n, 0) else {
            return;
        };
        self.write_slot(Some(class), n as u32, slot);
        let key = format!("{slot}{name}");
        let value = self.read_slot(Some(class), &key);
        self.write_slot(Some(class), value ^ 1, &key);
    }

    /// The option's rank or prestige is out of reach.
    fn option_locked(&self, option: usize) -> bool {
        if self.all_unlocked() {
            return false;
        }
        self.options.get(option).is_some_and(|option| {
            option.unlock_rank > self.rank() && option.unlock_prestige > self.prestige()
        })
    }

    /// The CoD Points an item still costs.
    fn wallet(&self) -> i32 {
        self.walk(&["PlayerStatsList", "CODPOINTS"])
            .map_or(0, |at| self.read(at)) as i32
    }

    /// An unbought item's cost, 0 once it is free.
    fn cost_left(&self, n: i32) -> i32 {
        if self.dvar("ui_items_no_cost", 0) != 0 || self.purchased(n) {
            return 0;
        }
        self.get(n).map_or(0, |item| item.cost.max(0))
    }

    /// The unlocked items of `list`: an item whose column 15 passes it reads
    /// as classified.
    fn unlocked_in(&self, list: &[i32]) -> i32 {
        list.iter()
            .filter(|&&n| self.get(n).is_some() && self.unlocked(n))
            .count() as i32
    }

    pub(super) fn classified(&self, n: i32, list: &[i32]) -> bool {
        self.get(n)
            .is_some_and(|item| item.classified_after > self.unlocked_in(list))
    }

    /// The item rows of a list, sorted as the builders sort them: column 9,
    /// then the item's number.
    fn sorted(&self, mut rows: Vec<i32>) -> Vec<i32> {
        rows.sort_by_key(|&n| (self.get(n).map_or(0, |item| item.order), n));
        rows
    }

    /// The colour a sorted row's name, cost and attribute share.
    fn row_color(&self, selected: bool, refused: bool, cost: i32) -> [f32; 4] {
        if refused {
            REFUSED
        } else if cost != 0 {
            if self.wallet() < cost {
                TOO_DEAR
            } else {
                AFFORDABLE
            }
        } else if selected {
            BLACK
        } else {
            WHITE
        }
    }
}

impl T5MenuData {
    /// `buildItemListForSlotName`: the slot's priced items. A perk is listed
    /// once, as its Pro version where that is unlocked.
    pub fn item_list_for_slot(&self, slot: &str) -> Vec<i32> {
        let Some(slot) = super::index_of(&super::SLOTS, slot) else {
            return Vec::new();
        };
        let cac = &self.cac;
        let rows = cac
            .items
            .iter()
            .filter(|item| item.slot == Some(slot) && item.cost >= 0)
            .filter_map(|item| {
                if item.group != SPECIALTY_GROUP {
                    return Some(item.index);
                }
                if item.pro {
                    return None;
                }
                let pro = cac.pro_version(item.index);
                Some(if pro != -1 && cac.pro_unlocked(item.index) {
                    pro
                } else {
                    item.index
                })
            })
            .collect();
        cac.sorted(rows)
    }

    /// `buildItemListForSlotNameAndGroup`.
    pub fn item_list_for_slot_and_group(&self, slot: &str, group: &str) -> Vec<i32> {
        let slot = super::index_of(&super::SLOTS, slot);
        let group = ITEM_GROUPS
            .iter()
            .position(|name| name.eq_ignore_ascii_case(group));
        let (Some(slot), Some(group)) = (slot, group) else {
            return Vec::new();
        };
        let cac = &self.cac;
        let rows = cac
            .items
            .iter()
            .filter(|item| item.slot == Some(slot) && item.group == group as i32 && item.cost >= 0)
            .map(|item| item.index)
            .collect();
        cac.sorted(rows)
    }

    /// `buildItemListForGroupName`.
    pub fn item_list_for_group(&self, group: &str) -> Vec<i32> {
        let Some(group) = ITEM_GROUPS
            .iter()
            .position(|name| name.eq_ignore_ascii_case(group))
        else {
            return Vec::new();
        };
        let cac = &self.cac;
        let rows = cac
            .items
            .iter()
            .filter(|item| item.group == group as i32 && item.cost >= 0)
            .map(|item| item.index)
            .collect();
        cac.sorted(rows)
    }

    /// The current item's attachment rows: its attachments after none, or
    /// the single row of none.
    fn attachment_rows(&self, item: i32) -> i32 {
        let filter = self.cac.dvar("attachmentFilter", super::ATTACHMENT_FILTER);
        let Some(item) = self.cac.get(item) else {
            return 0;
        };
        let count = if filter == 0 {
            item.attachments.len() as i32
        } else {
            1 + item
                .attachments
                .iter()
                .skip(1)
                .filter(|&&id| self.cac.point_of(id) as i32 == filter)
                .count() as i32
        };
        if count == 1 { 1 } else { count - 1 }
    }

    /// The attachment place a row names.
    fn attachment_at_row(&self, item: i32, row: i32) -> i32 {
        let filter = self.cac.dvar("attachmentFilter", super::ATTACHMENT_FILTER);
        let Some(item) = self.cac.get(item) else {
            return 0;
        };
        let k = if item.attachments.len() == 1 {
            row
        } else {
            row + 1
        };
        if k == 0 {
            return 0;
        }
        if filter == 0 && (k as usize) < item.attachments.len() {
            return k;
        }
        let mut left = k;
        for (place, &id) in item.attachments.iter().enumerate().skip(1) {
            if self.cac.point_of(id) as i32 == filter {
                left -= 1;
                if left < 1 {
                    return place as i32;
                }
            }
        }
        0
    }

    pub(super) fn current_option_group(&self) -> usize {
        self.cac.dvar("ui_currentWeaponOptionGroup", 0).clamp(0, 5) as usize
    }

    /// The rows a create-a-class feeder has; `None` for another feeder.
    pub fn feeder_count(&self, feeder: i32, context: &FeederContext<'_>) -> Option<i32> {
        Some(match feeder {
            FEEDER_ATTACHMENTS if context.current_item == -1 => 0,
            FEEDER_ATTACHMENTS => self.attachment_rows(context.current_item),
            FEEDER_OPTIONS => self
                .cac
                .options
                .iter()
                .filter(|option| option.group == self.current_option_group())
                .count() as i32,
            FEEDER_CUSTOM_KILLSTREAKS => context.list.len() as i32 + 1,
            FEEDER_WEAPON_GROUPS => {
                let slot = self
                    .cac
                    .dvars
                    .get("selected_loadout_slot")
                    .map_or("", String::as_str);
                if slot.eq_ignore_ascii_case("primary") {
                    5
                } else if slot.eq_ignore_ascii_case("secondary") {
                    3
                } else {
                    0
                }
            }
            FEEDER_PERKS
            | FEEDER_ITEMS
            | FEEDER_WEAPONS
            | FEEDER_SORTED_A
            | FEEDER_SORTED_B
            | FEEDER_KILLSTREAKS
            | FEEDER_CHALLENGE_PERKS
            | FEEDER_CHALLENGE_ITEMS => context.list.len() as i32,
            _ => return None,
        })
    }

    /// A list box's feeder has no port.
    pub fn report_feeder(&self, feeder: i32) {
        diag::warn!(
            Sim,
            "unknown!t5.mp.menu_feeder: feeder {feeder} answers no rows; only the create-a-class lists' are ported"
        );
    }

    /// What selecting the row sets (the row under the cursor keeps setting
    /// the item, as its painted cell does in the game).
    pub fn feeder_select(
        &self,
        feeder: i32,
        row: i32,
        list: &[i32],
        current: i32,
    ) -> FeederSelection {
        let at = |row: i32| {
            usize::try_from(row)
                .ok()
                .and_then(|row| list.get(row))
                .copied()
                .unwrap_or(-1)
        };
        match feeder {
            FEEDER_PERKS
            | FEEDER_ITEMS
            | FEEDER_WEAPONS
            | FEEDER_SORTED_A
            | FEEDER_SORTED_B
            | FEEDER_KILLSTREAKS
            | FEEDER_CHALLENGE_PERKS
            | FEEDER_CHALLENGE_ITEMS => FeederSelection {
                item: Some(at(row)),
                ..FeederSelection::default()
            },
            FEEDER_CUSTOM_KILLSTREAKS => FeederSelection {
                item: Some(if row == 0 { 0 } else { at(row - 1) }),
                ..FeederSelection::default()
            },
            FEEDER_ATTACHMENTS => FeederSelection {
                attachment: Some(self.attachment_at_row(current, row)),
                ..FeederSelection::default()
            },
            FEEDER_OPTIONS => FeederSelection {
                option: Some(row),
                ..FeederSelection::default()
            },
            _ => FeederSelection::default(),
        }
    }

    /// The cursor's row sets the current item each frame: the count of these
    /// feeders does so whenever the list box paints, the text of the others
    /// while the row is `painted`.
    pub fn feeder_cursor_sets(&self, feeder: i32, painted: bool) -> bool {
        match feeder {
            FEEDER_PERKS | FEEDER_ITEMS | FEEDER_CHALLENGE_PERKS => true,
            FEEDER_WEAPONS
            | FEEDER_ATTACHMENTS
            | FEEDER_CUSTOM_KILLSTREAKS
            | FEEDER_SORTED_A
            | FEEDER_SORTED_B
            | FEEDER_KILLSTREAKS
            | FEEDER_CHALLENGE_ITEMS => painted,
            _ => false,
        }
    }

    /// Activating a weapon group row: the group the next list shows.
    pub fn feeder_activated_group(&self, feeder: i32, row: i32) -> Option<&'static str> {
        if feeder != FEEDER_WEAPON_GROUPS {
            return None;
        }
        let slot = self.cac.dvars.get("selected_loadout_slot")?;
        let group = if slot.eq_ignore_ascii_case("primary") {
            row
        } else if slot.eq_ignore_ascii_case("secondary") {
            row + 5
        } else {
            return None;
        };
        ITEM_GROUPS.get(usize::try_from(group).ok()?).copied()
    }

    /// One column of a row; an empty cell for a column the feeder leaves
    /// out.
    pub fn feeder_cell(
        &self,
        feeder: i32,
        row: i32,
        column: i32,
        context: &FeederContext<'_>,
    ) -> FeederCell {
        let selected = context.focused && context.cursor == row;
        let at = |row: i32| {
            usize::try_from(row)
                .ok()
                .and_then(|row| context.list.get(row))
                .copied()
                .unwrap_or(-1)
        };
        match feeder {
            FEEDER_ITEMS | FEEDER_WEAPONS => {
                self.item_cell(at(row), column, selected, context.list)
            }
            FEEDER_PERKS => self.perk_cell(at(row), column, selected),
            FEEDER_ATTACHMENTS => self.attachment_cell(row, column, selected, context),
            FEEDER_OPTIONS => self.option_cell(row, column, selected, context),
            FEEDER_WEAPON_GROUPS => {
                self.group_cell(row, column, context.cursor == row && context.focused)
            }
            FEEDER_CUSTOM_KILLSTREAKS => self.custom_streak_cell(row, column, selected, context),
            FEEDER_SORTED_A | FEEDER_SORTED_B | FEEDER_KILLSTREAKS => {
                self.sorted_cell(feeder, at(row), column, selected)
            }
            FEEDER_CHALLENGE_PERKS | FEEDER_CHALLENGE_ITEMS => {
                self.challenge_cell(at(row), column, selected)
            }
            _ => FeederCell::default(),
        }
    }

    /// A row of a create-a-class choice: the band, the name, the cost, the
    /// lock and whether it is new.
    #[allow(clippy::too_many_arguments)]
    fn choice_cell(
        column: i32,
        selected: bool,
        name: &str,
        cost: i32,
        owned: bool,
        locked: bool,
        classified: bool,
        wallet: i32,
        new: bool,
    ) -> FeederCell {
        let shaded = if classified || locked {
            REFUSED
        } else if !owned {
            if cost <= wallet { AFFORDABLE } else { TOO_DEAR }
        } else if selected {
            BLACK
        } else {
            WHITE
        };
        match column {
            0 => FeederCell::image(IMAGE_WHITE).colored(if selected { WHITE } else { BAND }),
            1 => FeederCell::text(name).colored(shaded),
            3 if cost != 0 => FeederCell::cost(cost).colored(shaded),
            4 if locked => FeederCell::image(IMAGE_LOCKED),
            7 if new => FeederCell::image(IMAGE_NEW),
            _ => FeederCell::default(),
        }
    }

    fn item_cell(&self, n: i32, column: i32, selected: bool, list: &[i32]) -> FeederCell {
        let cac = &self.cac;
        let Some(item) = cac.get(n) else {
            return FeederCell::default();
        };
        let no_cost = cac.dvar("ui_items_no_cost", 0) != 0;
        let locked = self.item_locked(n) && !no_cost;
        let classified = cac.classified(n, list);
        let hidden = (item.group == CLASSIFIED_GROUP && locked) || classified;
        let name = if hidden { CLASSIFIED_NAME } else { &item.name };
        Self::choice_cell(
            column,
            selected,
            name,
            cac.cost_left(n),
            cac.purchased(n) || no_cost,
            locked,
            hidden,
            cac.wallet(),
            column == 7 && cac.is_new(n),
        )
    }

    fn perk_cell(&self, n: i32, column: i32, selected: bool) -> FeederCell {
        let cac = &self.cac;
        let Some(item) = cac.get(n) else {
            return FeederCell::default();
        };
        let editable = !is_fixed_perk(n);
        match column {
            0 => FeederCell::image(IMAGE_WHITE).colored(if selected { WHITE } else { BAND }),
            1 => FeederCell::text(&item.name).colored(if !editable {
                REFUSED
            } else if selected {
                BLACK
            } else {
                WHITE
            }),
            _ => FeederCell::default(),
        }
    }

    fn attachment_cell(
        &self,
        row: i32,
        column: i32,
        selected: bool,
        context: &FeederContext<'_>,
    ) -> FeederCell {
        let cac = &self.cac;
        let n = context.current_item;
        let place = self.attachment_at_row(n, row);
        let Some(item) = cac.get(n) else {
            return FeederCell::default();
        };
        let id = item
            .attachments
            .get(place as usize)
            .copied()
            .unwrap_or(NO_ATTACHMENT);
        let class = cac
            .dvars
            .get("ui_custom_name")
            .map_or("", String::as_str)
            .to_owned();
        let slot = item.slot.and_then(|slot| super::SLOTS.get(slot));
        let mut on = 0;
        let mut equipped = false;
        let mut refused = false;
        for point in 1..5 {
            let Some(slot) = slot else {
                break;
            };
            let key = format!("{slot}attachment{}", super::point_name(point));
            let current = cac.read_slot(Some(&class), &key) as usize;
            let current_id = item.attachments.get(current).copied().unwrap_or(0);
            if !cac.compatible(id, current_id) {
                refused = true;
            }
            if current_id != 0 {
                on += 1;
                if current_id == id {
                    equipped = true;
                }
                if point as usize == cac.point_of(id) {
                    refused = true;
                }
            }
        }
        if on >= cac.dvar("maxAttachmentsPerWeapon", super::MAX_ATTACHMENTS_PER_WEAPON) {
            refused = true;
        }
        let cost = if cac.attachment_unlocked(n, place as usize) {
            0
        } else {
            cac.attachment(id).map_or(0, |(_, a)| a.cost)
        };
        let color = cac.row_color(selected, refused, cost);
        match column {
            0 => {
                let name = cac.attachment(id).map_or("", |(_, a)| a.name.as_str());
                FeederCell::text(name).colored(color)
            }
            1 if equipped && id != 0 => FeederCell::image(IMAGE_EQUIPPED),
            2 if cost != 0 => FeederCell::cost(cost).colored(color),
            _ => FeederCell::default(),
        }
    }

    fn option_cell(
        &self,
        row: i32,
        column: i32,
        selected: bool,
        context: &FeederContext<'_>,
    ) -> FeederCell {
        let cac = &self.cac;
        let Some(place) = cac.option_in_group(self.current_option_group(), row) else {
            return FeederCell::default();
        };
        let option = &cac.options[place];
        let owned = option.cost == 0 || cac.option_unlocked(context.current_item, place);
        let locked = cac.option_locked(place);
        let name = if locked {
            CLASSIFIED_NAME
        } else {
            &option.name
        };
        Self::choice_cell(
            column,
            selected,
            name,
            if owned { 0 } else { option.cost },
            owned,
            locked,
            false,
            cac.wallet(),
            false,
        )
    }

    fn group_cell(&self, row: i32, column: i32, selected: bool) -> FeederCell {
        let cac = &self.cac;
        let slot = cac
            .dvars
            .get("selected_loadout_slot")
            .map_or("", String::as_str);
        let group = if slot.eq_ignore_ascii_case("secondary") {
            row + 5
        } else {
            row
        };
        let Some(name) = usize::try_from(group)
            .ok()
            .and_then(|group| ITEM_GROUPS.get(group))
        else {
            return FeederCell::default();
        };
        match column {
            0 => FeederCell::image(IMAGE_WHITE).colored(if selected { WHITE } else { BAND }),
            1 => FeederCell::text(&format!("MPUI_{}_CAPS", name.to_ascii_uppercase()))
                .colored(if selected { BLACK } else { WHITE }),
            7 if !cac.all_unlocked()
                && cac
                    .items
                    .iter()
                    .any(|item| item.group == group && cac.is_new(item.index)) =>
            {
                FeederCell::image(IMAGE_NEW)
            }
            _ => FeederCell::default(),
        }
    }

    fn custom_streak_cell(
        &self,
        row: i32,
        column: i32,
        selected: bool,
        context: &FeederContext<'_>,
    ) -> FeederCell {
        let cac = &self.cac;
        if column != 0 {
            return FeederCell::default();
        }
        let color = cac.row_color(selected, !context.focused, 0);
        if row == 0 {
            return FeederCell::text(NO_KILLSTREAK_NAME).colored(color);
        }
        let n = usize::try_from(row - 1)
            .ok()
            .and_then(|row| context.list.get(row))
            .copied()
            .unwrap_or(-1);
        cac.get(n).map_or_else(FeederCell::default, |item| {
            FeederCell::text(&item.name).colored(color)
        })
    }

    /// A row of the sorted slots' lists (the killstreaks): the name, a mark
    /// on those the slots hold, the cost, the lock and the attribute.
    fn sorted_cell(&self, feeder: i32, n: i32, column: i32, selected: bool) -> FeederCell {
        let cac = &self.cac;
        let Some(item) = cac.get(n) else {
            return FeederCell::default();
        };
        let cost = if cac.purchased(n) {
            0
        } else {
            item.cost.max(0)
        };
        let mut equipped = false;
        let mut refused = feeder != FEEDER_KILLSTREAKS;
        if feeder == FEEDER_KILLSTREAKS {
            let count = cac.dvar("ui_slot_count", 0);
            let group = cac.dvars.get("selected_group").map_or("", String::as_str);
            let mut held = 0;
            for k in 1..=count.min(0x100) {
                let slot = format!("{group}{k}");
                let Some(place) = super::index_of(&super::SLOTS, &slot) else {
                    break;
                };
                let in_slot = cac.read_slot(None, &slot) as i32;
                if in_slot != cac.slot_default(place) as i32 {
                    held += 1;
                    if in_slot == n {
                        equipped = true;
                    }
                    if cac
                        .get(in_slot)
                        .is_some_and(|i| i.attribute == item.attribute)
                    {
                        refused = true;
                    }
                }
            }
            refused = refused && !equipped || held == count;
        }
        let color = cac.row_color(selected, refused, cost);
        match column {
            0 => FeederCell::text(&item.name).colored(color),
            1 if equipped => FeederCell::image(IMAGE_EQUIPPED),
            2 if cost != 0 => FeederCell::cost(cost).colored(color),
            3 if self.item_locked(n) => FeederCell::image(IMAGE_LOCKED),
            4 => FeederCell {
                text: item.attribute.to_string(),
                color: Some(color),
                ..FeederCell::default()
            },
            _ => FeederCell::default(),
        }
    }

    fn challenge_cell(&self, n: i32, column: i32, selected: bool) -> FeederCell {
        let cac = &self.cac;
        let Some(item) = cac.get(n) else {
            return FeederCell::default();
        };
        let unlocked = cac.unlocked(n);
        match column {
            0 => FeederCell::text(&item.name).colored(if !unlocked {
                REFUSED
            } else if selected {
                BLACK
            } else {
                WHITE
            }),
            1 if !unlocked => FeederCell::image(IMAGE_LOCKED),
            _ => FeederCell::default(),
        }
    }
}

/// The perks a class cannot edit (their numbers in `mp/statsTable.csv`).
fn is_fixed_perk(n: i32) -> bool {
    matches!(
        n,
        0x98..=0x9b | 0xa8 | 0xa9 | 0xab | 0xac | 0xad | 0xb0 | 0xb2 | 0xb3
    )
}
