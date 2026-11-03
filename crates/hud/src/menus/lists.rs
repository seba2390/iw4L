//! Black Ops' list boxes: the cursor, its keys and clicks, and the commands
//! that fill and move them. Their rows come from the game's feeders
//! (`sim::T5MenuData`).

use super::Pressed;
use super::runtime::Runner;
use super::state::ListPos;
use crate::chrome::{ChromeList, ChromeListCell};
use asset_game::{MenuDef, MenuItem, MenuList};

/// A second click on a row within this activates it.
const DOUBLE_CLICK_MS: i32 = 300;
/// The list box's own margin, top and bottom.
const LIST_MARGIN: f32 = 2.0;

fn list_key(menu: &str, index: usize) -> (String, usize) {
    (menu.to_ascii_lowercase(), index)
}

/// The rows a list box of `height` shows.
fn rows_shown(list: &MenuList, height: f32) -> i32 {
    if list.element_height <= 0.0 {
        return 0;
    }
    ((height - LIST_MARGIN).max(0.0) / list.element_height) as i32
}

impl<'a> Runner<'a, '_> {
    /// The list box at `index`; it borrows the catalog, not the runner.
    fn list_item<'c>(&self, menu: &str, index: usize) -> Option<&'c MenuItem>
    where
        'a: 'c,
    {
        let catalog: &'a asset_game::MenuCatalog = self.catalog;
        catalog
            .get(menu)?
            .items
            .get(index)
            .filter(|item| item.list.is_some())
    }

    /// The item's height as it stands: where an animation has moved it.
    fn list_height(&self, menu: &str, index: usize, item: &MenuItem) -> f32 {
        self.menus
            .stack
            .iter()
            .find(|open| open.name.eq_ignore_ascii_case(menu))
            .and_then(|open| open.items.get(index))
            .and_then(|state| state.rect)
            .map_or(item.rect.h, |rect| rect.h)
    }

    fn list_pos(&self, menu: &str, index: usize) -> ListPos {
        self.menus
            .list_positions
            .get(&list_key(menu, index))
            .copied()
            .unwrap_or_else(ListPos::default)
    }

    fn feeder_context(&self, cursor: i32) -> sim::FeederContext<'_> {
        sim::FeederContext {
            list: &self.menus.item_list,
            current_item: self.menus.current_item,
            cursor,
            focused: false,
        }
    }

    fn list_count(&self, feeder: i32, cursor: i32) -> i32 {
        let context = self.feeder_context(cursor);
        self.world
            .t5_menu
            .and_then(|data| data.feeder_count(feeder, &context))
            .unwrap_or(0)
    }

    /// The menus' current item, which the console's class commands act on
    /// too.
    fn set_current_item(&mut self, n: i32) {
        if self.menus.current_item != n {
            self.menus.current_item = n;
            self.menus.exec.push(format!("setcurrentitemindex {n}"));
        }
    }

    /// What selecting the row sets.
    fn apply_selection(&mut self, feeder: i32, row: i32) {
        let Some(data) = self.world.t5_menu else {
            return;
        };
        let selection =
            data.feeder_select(feeder, row, &self.menus.item_list, self.menus.current_item);
        if let Some(n) = selection.item {
            self.set_current_item(n);
        }
        if let Some(attachment) = selection.attachment
            && self.menus.current_attachment != attachment
        {
            self.menus.current_attachment = attachment;
            self.menus
                .exec
                .push(format!("setcurrentattachmentindex {attachment}"));
        }
        if let Some(option) = selection.option
            && self.menus.current_option != option
        {
            self.menus.current_option = option;
            self.menus
                .exec
                .push(format!("setcurrentoptionindex {option}"));
        }
    }

    /// The cursor to `row`, keeping it in the window, and the selection's
    /// handlers (`onListboxSelectionChange`) when its menu has the input.
    fn list_select(&mut self, menu: &str, index: usize, row: i32) {
        let Some(item) = self.list_item(menu, index) else {
            return;
        };
        let Some(list) = item.list.as_ref() else {
            return;
        };
        let feeder = list.feeder;
        let shown = rows_shown(list, self.list_height(menu, index, item));
        let handlers = &item.handlers.selection_change;
        let count = self.list_count(feeder, row);
        let mut pos = self.list_pos(menu, index);
        let last = pos.cursor;
        pos.cursor = row;
        let most = (count - shown).max(0);
        if row < last {
            if row >= 0 && row < pos.top {
                pos.top = row;
            }
        } else if row != last && row < most + shown && pos.top + shown <= row {
            pos.top = row - shown + 1;
        }
        self.menus.list_positions.insert(list_key(menu, index), pos);
        self.apply_selection(feeder, row);
        let has_input = self
            .menus
            .input_menu()
            .is_some_and(|open| open.name.eq_ignore_ascii_case(menu));
        if has_input {
            self.run_events(menu, Some(index), handlers);
        }
    }

    /// A list box taking the focus runs its selection handlers and selects
    /// its cursor's row again.
    pub(super) fn list_focused(&mut self, menu: &str, index: usize) {
        let Some(item) = self.list_item(menu, index) else {
            return;
        };
        let Some(list) = item.list.as_ref() else {
            return;
        };
        let cursor = self.list_pos(menu, index).cursor;
        if cursor < 0 {
            return;
        }
        let has_input = self
            .menus
            .input_menu()
            .is_some_and(|open| open.name.eq_ignore_ascii_case(menu));
        if has_input {
            self.run_events(menu, Some(index), &item.handlers.selection_change);
        }
        self.apply_selection(list.feeder, cursor);
    }

    /// The focused list box takes the keys: up and down move its cursor (or
    /// its window when it has none), enter activates the row. `true`: the
    /// key went to a list box.
    pub(super) fn list_keys(&mut self, menu: &str, pressed: &Pressed) -> bool {
        let Some(index) = self.menus.input_menu().and_then(|open| open.focus) else {
            return false;
        };
        let Some(item) = self.list_item(menu, index) else {
            return false;
        };
        let Some(list) = item.list.as_ref() else {
            return false;
        };
        let (feeder, not_selectable, double_click, shown) = (
            list.feeder,
            list.not_selectable,
            &item.handlers.double_click,
            rows_shown(list, self.list_height(menu, index, item)),
        );
        let step = match (pressed.up, pressed.down) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        if step != 0 {
            let mut pos = self.list_pos(menu, index);
            let count = self.list_count(feeder, pos.cursor);
            if not_selectable {
                pos.top = (pos.top + step).clamp(0, (count - shown).max(0));
                self.menus.list_positions.insert(list_key(menu, index), pos);
            } else if count > 1 {
                self.list_select(menu, index, (pos.cursor + step).clamp(0, count - 1));
            }
            return true;
        }
        if pressed.enter {
            if !not_selectable {
                self.list_activate(menu, index, feeder, double_click);
            }
            return true;
        }
        false
    }

    fn list_activate(
        &mut self,
        menu: &str,
        index: usize,
        feeder: i32,
        handlers: &[asset_game::MenuEvent],
    ) {
        let cursor = self.list_pos(menu, index).cursor;
        if let Some(group) = self
            .world
            .t5_menu
            .and_then(|data| data.feeder_activated_group(feeder, cursor))
        {
            self.set_dvar("selected_group", group);
        }
        self.run_events(menu, Some(index), handlers);
    }

    /// A click in a list box picks the row under it; a second soon after, on
    /// the same row, activates it. `rect` is the item's on screen.
    pub(super) fn list_click(
        &mut self,
        menu: &str,
        index: usize,
        rect: [f32; 4],
        cursor_y: f32,
    ) -> bool {
        let Some(item) = self.list_item(menu, index) else {
            return false;
        };
        let Some(list) = item.list.as_ref() else {
            return false;
        };
        self.set_focus(menu, index);
        let height = self.list_height(menu, index, item);
        if height <= 0.0 {
            return true;
        }
        let per_unit = (rect[3] - rect[1]) / height;
        let (feeder, element, double_click) = (
            list.feeder,
            list.element_height,
            &item.handlers.double_click,
        );
        let mut pos = self.list_pos(menu, index);
        let count = self.list_count(feeder, pos.cursor);
        let row =
            pos.top + ((cursor_y - rect[1] - LIST_MARGIN * per_unit) / (element * per_unit)) as i32;
        if row < 0 || row >= count {
            return true;
        }
        let now = self.world.ms;
        let again = pos
            .click
            .is_some_and(|(at, last)| last == row && now - at < DOUBLE_CLICK_MS);
        if pos.cursor != row {
            self.list_select(menu, index, row);
            pos = self.list_pos(menu, index);
        }
        pos.click = Some((now, row));
        self.menus.list_positions.insert(list_key(menu, index), pos);
        if again {
            self.list_activate(menu, index, feeder, double_click);
        }
        true
    }

    /// Every shown list box's cursor stays within its rows, and the row under
    /// a painted cursor keeps setting the current item, as the feeders' own
    /// count and text handlers do each frame.
    pub(super) fn lists_frame(&mut self) {
        let menus: Vec<String> = self.menus.stack.iter().map(|m| m.name.clone()).collect();
        for menu in menus {
            let Some(def) = self.catalog.get(&menu) else {
                continue;
            };
            if !def.items.iter().any(|item| item.list.is_some()) {
                continue;
            }
            for index in self.visible_items(&menu) {
                let Some(item) = def.items.get(index) else {
                    continue;
                };
                let Some(list) = item.list.as_ref() else {
                    continue;
                };
                let mut pos = self.list_pos(&menu, index);
                let known = self.world.t5_menu.is_some_and(|data| {
                    let context = self.feeder_context(pos.cursor);
                    data.feeder_count(list.feeder, &context).is_some()
                });
                if !known {
                    if let Some(data) = self.world.t5_menu
                        && self
                            .menus
                            .reported
                            .insert(format!("feeder {}", list.feeder))
                    {
                        data.report_feeder(list.feeder);
                    }
                    continue;
                }
                let count = self.list_count(list.feeder, pos.cursor);
                if count != 0 && pos.cursor >= count {
                    pos.top = 0;
                    pos.cursor = count - 1;
                    self.menus
                        .list_positions
                        .insert(list_key(&menu, index), pos);
                    self.apply_selection(list.feeder, pos.cursor);
                }
                let shown = rows_shown(list, self.list_height(&menu, index, item));
                let painted = (pos.top..pos.top + shown).contains(&pos.cursor);
                if self
                    .world
                    .t5_menu
                    .is_some_and(|data| data.feeder_cursor_sets(list.feeder, painted))
                {
                    self.apply_selection(list.feeder, pos.cursor);
                }
            }
        }
    }

    /// The `buildItemListFor*` commands fill the list the item feeders read.
    pub(super) fn build_item_list(&mut self, command: &str, words: &[String]) {
        let arg = |n: usize| words.get(n).map_or("", String::as_str);
        let needs = if command == "builditemlistforslotnameandgroup" {
            3
        } else {
            2
        };
        if words.len() < needs {
            diag::warn!(
                Ui,
                "{command} usage: {}",
                match command {
                    "builditemlistforslotname" => "buildItemListForSlotName <loadout slot>",
                    "builditemlistforgroupname" => "buildItemListForGroupName <group name>",
                    _ => "buildItemListForSlotNameAndGroup <slot name> <group name>",
                }
            );
            return;
        }
        let Some(data) = self.world.t5_menu else {
            diag::warn!(Ui, "{command}: no profile stats");
            return;
        };
        self.menus.item_list = match command {
            "builditemlistforslotname" => data.item_list_for_slot(arg(1)),
            "builditemlistforgroupname" => data.item_list_for_group(arg(1)),
            _ => data.item_list_for_slot_and_group(arg(1), arg(2)),
        };
    }

    /// `refreshFeederSelection [item]`: the selection of the item the
    /// command runs on, or the one named, again.
    pub(super) fn refresh_feeder_selection(&mut self, menu: &str, item: Option<&str>) {
        let Some(index) = self.list_named(menu, item) else {
            diag::warn!(Ui, "refreshFeederSelection: Could not find an itemdef.");
            return;
        };
        let Some(list) = self.list_item(menu, index).and_then(|i| i.list.as_ref()) else {
            diag::warn!(Ui, "refreshFeederSelection: No list box found for itemdef.");
            return;
        };
        let feeder = list.feeder;
        let pos = self.list_pos(menu, index);
        let count = self.list_count(feeder, pos.cursor);
        let row = if pos.cursor < 0 || count <= pos.cursor {
            0
        } else {
            pos.cursor
        };
        self.apply_selection(feeder, row);
    }

    /// `selectFeeder <top|bottom|row> [item]`.
    pub(super) fn select_feeder(&mut self, menu: &str, words: &[String]) {
        let Some(place) = words.get(1) else {
            return;
        };
        let Some(index) = self.list_named(menu, words.get(2).map(String::as_str)) else {
            if let Some(name) = words.get(2) {
                diag::warn!(Ui, "selectFeeder: could not find widget named '{name}'");
            }
            return;
        };
        let Some(item) = self.list_item(menu, index) else {
            return;
        };
        let Some(list) = item.list.as_ref() else {
            return;
        };
        let feeder = list.feeder;
        let not_selectable = list.not_selectable;
        let shown = rows_shown(list, self.list_height(menu, index, item));
        let mut pos = self.list_pos(menu, index);
        let count = self.list_count(feeder, pos.cursor);
        let last = (count - 1).max(0);
        if place.eq_ignore_ascii_case("top") {
            pos.top = 0;
            if !not_selectable {
                pos.cursor = 0;
                self.apply_selection(feeder, 0);
            }
            self.menus.list_positions.insert(list_key(menu, index), pos);
        } else if place.eq_ignore_ascii_case("bottom") {
            if not_selectable {
                pos.top = (count - shown).max(0);
                self.menus.list_positions.insert(list_key(menu, index), pos);
            } else {
                self.list_select(menu, index, last);
            }
        } else if !not_selectable {
            self.list_select(menu, index, super::script::atoi_c(place));
        }
    }

    /// `focusItem <name>`.
    pub(super) fn focus_item(&mut self, menu: &str, name: &str) {
        let Some(index) = self.catalog.get(menu).and_then(|def| {
            def.items
                .iter()
                .position(|i| i.name.eq_ignore_ascii_case(name))
        }) else {
            return;
        };
        self.set_focus(menu, index);
    }

    /// The list box named, or the menu's focused item.
    fn list_named(&self, menu: &str, item: Option<&str>) -> Option<usize> {
        let def = self.catalog.get(menu)?;
        match item {
            Some(name) => def
                .items
                .iter()
                .position(|i| i.list.is_some() && i.name.eq_ignore_ascii_case(name)),
            None => self
                .menus
                .stack
                .iter()
                .find(|open| open.name.eq_ignore_ascii_case(menu))
                .and_then(|open| open.focus),
        }
    }
}

/// Everything a list box paints this frame: its rows from the first shown.
pub(super) fn list_view(
    data: &sim::T5MenuData,
    def: &MenuDef,
    index: usize,
    list: &MenuList,
    menus: &super::state::ScriptMenus,
    focused: bool,
) -> Option<ChromeList> {
    let pos = menus
        .list_positions
        .get(&list_key(&def.name, index))
        .copied()
        .unwrap_or_else(ListPos::default);
    let context = sim::FeederContext {
        list: &menus.item_list,
        current_item: menus.current_item,
        cursor: pos.cursor,
        focused,
    };
    let count = data.feeder_count(list.feeder, &context)?;
    let rows = (pos.top..count)
        .map(|row| {
            (0..list.columns.len() as i32)
                .map(|column| {
                    let cell = data.feeder_cell(list.feeder, row, column, &context);
                    ChromeListCell {
                        text: cell.text,
                        text_arg: cell.text_arg,
                        image: cell.image,
                        color: cell.color,
                    }
                })
                .collect()
        })
        .collect();
    Some(ChromeList {
        count,
        top: pos.top,
        cursor: pos.cursor,
        focused,
        rows,
    })
}
