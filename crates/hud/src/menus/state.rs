use bevy::prelude::*;
use frame::AppScreen;

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemState {
    pub(crate) hidden: bool,
    pub(crate) fore: Option<[f32; 4]>,
    pub(crate) back: Option<[f32; 4]>,
    /// Where an animation has moved the item.
    pub(crate) rect: Option<asset_game::MenuRect>,
    pub(crate) text_scale: Option<f32>,
    pub(crate) anim: Option<ItemAnim>,
}

impl ItemState {
    /// A menu's items as it opens: hidden where its data says so.
    pub(crate) fn for_menu(def: &asset_game::MenuDef) -> Vec<Self> {
        def.items
            .iter()
            .map(|item| Self {
                hidden: item.starts_hidden,
                ..Self::default()
            })
            .collect()
    }
}

/// An item moving to one of its animation states (`ui_animate`): from the
/// look it had when the move began, over `duration_ms` of the UI's clock.
#[derive(Clone, Debug)]
pub(crate) struct ItemAnim {
    pub(crate) from_rect: asset_game::MenuRect,
    pub(crate) from_fore: [f32; 4],
    pub(crate) from_back: [f32; 4],
    pub(crate) from_text_scale: f32,
    pub(crate) to: usize,
    pub(crate) start_ms: i32,
    pub(crate) duration_ms: i32,
}

/// The rank popup's icon: it starts tilted and swings to rest as its popup
/// ends, by Black Ops' own per-frame step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RankIconSwing {
    pub(crate) start_ms: i32,
    pub(crate) angle: f32,
    pub(crate) velocity: f32,
}

/// How long the icon swings, and its first tilt in degrees.
const RANK_ICON_SWING_MS: i32 = 2500;
pub(crate) const RANK_ICON_TILT: f32 = 7.0;

impl RankIconSwing {
    /// The next frame's angle; `None` once the swing is over.
    pub(crate) fn step(&mut self, now_ms: i32) -> Option<f32> {
        let left = self
            .start_ms
            .wrapping_add(RANK_ICON_SWING_MS)
            .wrapping_sub(now_ms);
        if left <= 0 {
            return None;
        }
        let k = (left as f32 * 0.0004).powi(2);
        let limit = k * RANK_ICON_TILT;
        if limit < self.angle {
            self.velocity = k * -0.5;
        }
        if self.angle < -limit {
            self.velocity = k * 0.5;
        }
        self.angle += self.velocity;
        Some(self.angle)
    }
}

/// A list box's cursor and first row, kept while its menu is closed as the
/// game keeps them on the list box.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ListPos {
    pub(crate) cursor: i32,
    pub(crate) top: i32,
    /// The UI time and row of the last click: a second soon after activates.
    pub(crate) click: Option<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenMenu {
    pub(crate) name: String,
    pub(crate) captures_input: bool,
    /// Paints into a UI3D window: it takes the keys but not the mouse.
    pub(crate) ui3d: bool,
    pub(crate) focus: Option<usize>,
    pub(crate) hover: Option<usize>,
    pub(crate) items: Vec<ItemState>,
}

#[derive(Clone, Debug)]
pub(crate) struct EditState {
    pub(crate) menu: String,
    pub(crate) item: usize,
    pub(crate) buffer: Vec<char>,
    pub(crate) cursor: usize,
    pub(crate) max_chars: usize,
}

#[derive(Resource, Default)]
pub struct ScriptMenus {
    pub(crate) stack: Vec<OpenMenu>,
    /// Menus `showMenu` made visible without opening them; they paint
    /// beneath the open stack.
    pub(crate) shown: Vec<String>,
    /// The map the menus were opened on: a new level closes them all.
    pub(crate) level: Option<String>,
    pub(crate) applied_serial: u32,
    pub(crate) responses: Vec<(String, String)>,
    /// Messages for the level's scripts, in order: `sendMenuNotify`'s, and
    /// the menus opened and closed (`true`: dropped while paused).
    pub(crate) menu_messages: Vec<(String, bool)>,
    pub(crate) exec: Vec<String>,
    pub(crate) sounds: Vec<String>,
    pub(crate) reported: std::collections::HashSet<String>,
    pub(crate) screen: Option<AppScreen>,
    pub(crate) cursor: Option<Vec2>,
    pub(crate) editing: Option<EditState>,
    pub(crate) slider_drag: Option<(String, usize)>,
    pub(crate) binding_menu: Option<String>,
    pub(crate) bind_requests: Vec<String>,
    /// The in-game menu key opened a menu its game pauses an offline match under.
    pub(crate) pause_requested: bool,
    /// The game's own cursor was drawn this frame: the system pointer hides.
    pub(crate) draws_cursor: bool,
    /// The level's own menu (`ModeRules::level_menu`) has been opened.
    pub(crate) level_menu_opened: bool,
    /// The item a game's create-a-class menus act on (Black Ops'
    /// `setCurrentItemIndex`).
    pub(crate) current_item: i32,
    /// The list the `buildItemListFor*` commands last filled.
    pub(crate) item_list: Vec<i32>,
    /// The attachment and the weapon option the lists have selected.
    pub(crate) current_attachment: i32,
    pub(crate) current_option: i32,
    pub(crate) list_positions: std::collections::HashMap<(String, usize), ListPos>,
    /// The HUD menus the game animates, which no stack holds: their items as
    /// they stand, by the menu's lowercased name.
    pub(crate) hud_items: std::collections::HashMap<String, Vec<ItemState>>,
    /// The rank popup's icon as it swings.
    pub(crate) rank_icon: Option<RankIconSwing>,
    /// When the rank popup's name began typing itself out.
    pub(crate) rank_line_start: Option<i32>,
    /// The centre print's start time and line.
    pub(crate) center_print: Option<(i32, String)>,
}

impl ScriptMenus {
    pub fn captures_input(&self) -> bool {
        self.input_menu().is_some()
    }

    /// The mouse turns the view while a menu holds the keys: the focused
    /// menu paints into a UI3D window, which takes no cursor.
    /// The game draws its own cursor over the menus this frame.
    pub fn draws_cursor(&self) -> bool {
        self.draws_cursor
    }

    pub fn mouse_looks(&self) -> bool {
        self.input_menu().is_some_and(|menu| menu.ui3d)
    }

    pub(crate) fn input_menu(&self) -> Option<&OpenMenu> {
        self.stack.iter().rev().find(|menu| menu.captures_input)
    }

    pub fn open_names(&self) -> Vec<String> {
        self.stack.iter().map(|m| m.name.clone()).collect()
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    pub fn focused_item(&self) -> Option<(&str, usize)> {
        let menu = self.input_menu()?;
        Some((&menu.name, menu.focus?))
    }

    pub fn focused_item_in(&self, name: &str) -> Option<usize> {
        self.stack.get(self.position(name)?)?.focus
    }

    pub(crate) fn position(&self, name: &str) -> Option<usize> {
        self.stack
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut OpenMenu> {
        let pos = self.position(name)?;
        self.stack.get_mut(pos)
    }
}
