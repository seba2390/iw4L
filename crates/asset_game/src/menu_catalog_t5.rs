//! Black Ops menus and fonts, captured from the zones that ship them
//! (`code_post_gfx`, `patch`, the localized `code_post_gfx`).
//!
//! Item types and expressions keep Black Ops' meaning: expressions are stored
//! as `t5` postfix programs (see `hud_iw4::expr_t5`), item types are mapped to
//! the menu engine's types by their names in the Black Ops enum.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use asset_transport::{T5ZoneMemory, open_zone};
use fastfile_iw4::GlyphCapture;
use fastfile_t5::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZonePtr, ZoneStream,
    load_asset_at_observed, load_zone,
};

use crate::localize::decode_localized_text;
use crate::menu_catalog::{
    FontDef, MenuCatalog, MenuDef, MenuEditField, MenuEvent, MenuItem, MenuRect, MenuSlider,
    T5EventScript, T5ScriptCondition,
};

/// The HUD menu lists a Black Ops zombies client draws, in its load order:
/// the common HUD, then the zombies HUD (the singleplayer HUD is loaded only
/// outside zombies, the co-op HUD only in online and system-link games). Each
/// menu's own visibility decides when it shows.
pub const T5_HUD_MENU_LISTS: &[&str] = &["ui/hud.txt", "ui/hud_zombie.txt"];

/// The menu lists a Black Ops zombies client loads in a match
/// (`UI_LoadIngameMenus`): the pause menu and its options.
pub const T5_INGAME_MENU_LISTS: &[&str] = &["ui/ingame.txt", "ui/ingame_options.txt"];

/// The `float_exp` slot of an item's forecolor alpha in the menu engine.
const FLOAT_FORECOLOR_A: u32 = 8;

const ITEM_DEF_WINDOW_RECT: usize = 4;
const WINDOW_STYLE: usize = 56;
const WINDOW_BORDER: usize = 57;
const WINDOW_BORDER_SIZE: usize = 76;
const WINDOW_OWNER_DRAW: usize = 68;
const WINDOW_STATIC_FLAGS: usize = 80;
const WINDOW_FORE_COLOR: usize = 92;
const WINDOW_BACK_COLOR: usize = 108;
const WINDOW_ROTATION: usize = 156;
const WINDOW_BACKGROUND: usize = 160;

const MENU_FULLSCREEN: usize = 168;
const MENU_UI3D_WINDOW: usize = 172;
/// Black Ops' 3D UI windows (`ui3dsetwindow` ids 0 to 5).
const UI3D_WINDOWS: usize = 6;
const MENU_ITEM_COUNT: usize = 0xb0;
const MENU_VISIBLE_EXP: usize = 276;
const MENU_SOUND_NAME: usize = 316;
const MENU_FOCUS_COLOR: usize = 328;
const MENU_RECT_X_EXP: usize = 360;
const MENU_RECT_Y_EXP: usize = 376;
const MENU_ITEMS: usize = 0x188;
const MENU_ON_EVENT: usize = 0x10c;
const MENU_ON_KEY: usize = 0x110;

const ITEM_ON_EVENT: usize = 0x104;

const ITEM_TYPE: usize = 164;
const ITEM_DVAR: usize = 176;
const ITEM_DVAR_TEST: usize = 180;
const ITEM_ENABLE_DVAR: usize = 184;
const ITEM_DVAR_FLAGS: usize = 188;
const ITEM_TYPE_DATA: usize = 192;
const ITEM_RECT_EXP_DATA: usize = 200;
const ITEM_VISIBLE_EXP: usize = 204;
const ITEM_FORECOLOR_A_EXP: usize = 240;

const TEXT_DEF_FONT: usize = 28;
const TEXT_DEF_ALIGN_MODE: usize = 36;
const TEXT_DEF_ALIGN_X: usize = 40;
const TEXT_DEF_ALIGN_Y: usize = 44;
const TEXT_DEF_SCALE: usize = 48;
const TEXT_DEF_STYLE: usize = 52;
const TEXT_DEF_TEXT: usize = 56;
const TEXT_DEF_TEXT_EXP: usize = 60;
const TEXT_DEF_TYPE_DATA: usize = 64;

const FOCUS_MOUSE_ENTER: usize = 8;
const FOCUS_MOUSE_EXIT: usize = 12;
const FOCUS_ON_KEY: usize = 16;
const FOCUS_TYPE_DATA: usize = 20;

const MULTI_DVAR_STR: usize = 0x80;
const MULTI_DVAR_VALUE: usize = 0x100;
const MULTI_COUNT: usize = 0x180;
const MULTI_STR_DEF: usize = 0x188;
const MULTI_MAX: usize = 32;

const EDIT_MIN: usize = 4;
const EDIT_MAX: usize = 8;
const EDIT_MAX_CHARS: usize = 20;

const HANDLER_NAME: usize = 0;
const HANDLER_SCRIPT: usize = 4;
const HANDLER_NEXT: usize = 8;
const KEY_HANDLER_KEY: usize = 0;

const SCRIPT_PREREQUISITES: usize = 0;
const SCRIPT_CONDITION: usize = 4;
const SCRIPT_TYPE: usize = 0x14;
const SCRIPT_FIRE_ON_TRUE: usize = 0x18;
const SCRIPT_ACTION: usize = 0x1c;
const SCRIPT_BLOCK: usize = 0x20;
const SCRIPT_CONSTRUCT: usize = 0x24;
const SCRIPT_NEXT: usize = 0x28;

const CONDITION_FIRE_ON_TRUE: usize = 0;
const CONDITION_CONSTRUCT: usize = 4;
const CONDITION_BLOCK: usize = 8;
const CONDITION_NEXT: usize = 0xc;

/// A chain longer than any menu writes is a broken link, not a script.
const MAX_CHAIN: usize = 4096;

const T5_ITEM_TYPE_IMAGE: i32 = 2;
const T5_ITEM_TYPE_OWNERDRAW: i32 = 6;
const T5_ITEM_TYPE_SLIDER: i32 = 8;
const T5_ITEM_TYPE_MULTI: i32 = 0xa;

/// Black Ops `ItemDefType` values whose text def carries a `focusItemDef_s`.
fn t5_text_has_focus_def(ty: i32) -> bool {
    matches!(
        ty,
        3 | 4 | 5 | 7 | 8 | 9 | 0xa | 0xb | 0xc | 0xd | 0xe | 0x10 | 0x14 | 0x16 | 0x1e
    )
}

/// Black Ops `ItemDefType` values whose item data is a `focusItemDef_s`
/// itself (a blank button).
fn t5_item_is_focus_def(ty: i32) -> bool {
    matches!(ty, 0x13 | 0x15)
}

/// Black Ops `ItemDefType` values whose focus def carries an
/// `editFieldDef_s` (`Item_IsEditFieldDef`).
fn t5_focus_has_edit_field(ty: i32) -> bool {
    matches!(ty, 5 | 7 | 8 | 9 | 0xc | 0xd | 0xe | 0x10 | 0x16 | 0x1e)
}

/// Black Ops `ItemDefType` values whose item carries a `textDef_s`.
fn t5_item_has_text_def(ty: i32) -> bool {
    matches!(
        ty,
        1 | 3 | 4 | 5 | 7 | 8 | 9 | 0xa | 0xb | 0xc | 0xd | 0xe | 0xf | 0x10 | 0x12 | 0x14 | 0x16
    )
}

/// The menu engine's item type for a Black Ops `ItemDefType`, matched by name
/// (`ITEM_TYPE_TEXT`, `ITEM_TYPE_BUTTON`, ...). Black Ops' image and
/// owner-draw text items draw like the engine's text and owner-draw items.
fn engine_item_type(t5: i32) -> i32 {
    match t5 {
        0 | 1 | T5_ITEM_TYPE_IMAGE => 0,
        3 | 0x15 => 1,
        0x19 => 2,
        0x1b => 3,
        5 => 4,
        0x1c => 5,
        4 => 6,
        0x1a => 7,
        T5_ITEM_TYPE_OWNERDRAW | 0x12 | 0x13 | 0x14 => 8,
        7 => 9,
        8 => 10,
        9 => 11,
        0xa => 12,
        0xb => 13,
        0xc => 14,
        0x27 => 15,
        0xd => 16,
        0x1e => 17,
        0xe => 18,
        0xf => 19,
        other => other,
    }
}

pub fn load_t5_menu_catalog(path: &Path) -> Result<MenuCatalog, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.t5_header().map_err(|e| e.to_string())?;
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = T5MenuSink {
        catalog: MenuCatalog {
            namespace: Some(asset_core::AssetNamespace::T5),
            ..MenuCatalog::default()
        },
        material_names: HashMap::new(),
        list_menus: Vec::new(),
        loading_asset: None,
    };
    load_zone(&mut stream, &mut sink).map_err(|error| match sink.loading_asset {
        Some((index, ty)) => format!("asset #{index} {ty:?}: {error:?}"),
        None => format!("asset table: {error:?}"),
    })?;
    Ok(sink.catalog)
}

struct T5MenuSink {
    catalog: MenuCatalog,
    material_names: HashMap<Ptr, String>,
    list_menus: Vec<String>,
    loading_asset: Option<(usize, AssetType)>,
}

fn str_at<'a>(s: &'a ZoneStream<'_>, p: Ptr, off: usize) -> Option<&'a str> {
    match s.ptr_at(p, off).ok()? {
        ZonePtr::Offset(q) => s.cstr(s.resolve_alias(q)).ok(),
        _ => None,
    }
}

fn bytes_at<'a>(s: &'a ZoneStream<'_>, p: Ptr, off: usize) -> Option<&'a [u8]> {
    match s.ptr_at(p, off).ok()? {
        ZonePtr::Offset(q) => s.cstr_bytes(s.resolve_alias(q)).ok(),
        _ => None,
    }
}

fn ptr_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Option<Ptr> {
    match s.ptr_at(p, off).ok()? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    }
}

fn f32_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> f32 {
    s.f32_at(p, off).unwrap_or(0.0)
}

fn i32_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> i32 {
    s.i32_at(p, off).unwrap_or(0)
}

fn color_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> [f32; 4] {
    [0, 4, 8, 12].map(|i| f32_at(s, p, off + i))
}

fn rect_at(s: &ZoneStream<'_>, p: Ptr) -> MenuRect {
    MenuRect {
        x: f32_at(s, p, 0),
        y: f32_at(s, p, 4),
        w: f32_at(s, p, 8),
        h: f32_at(s, p, 12),
        horz_align: i32_at(s, p, 16) as u8,
        vert_align: i32_at(s, p, 20) as u8,
    }
}

/// An `ExpressionStatement` as a `t5` postfix program: `i:`/`f:`/`s:`
/// constants and `#n` operators or functions, in the zone's order.
fn rpn_dump(s: &ZoneStream<'_>, statement: Ptr) -> String {
    let count = i32_at(s, statement, 8);
    if count <= 0 {
        return String::new();
    }
    let Some(rpn) = ptr_at(s, statement, 12) else {
        return String::new();
    };
    let mut out = String::from("t5");
    for i in 0..count as usize {
        let entry = rpn.at(i * fastfile_t5::size::EXPRESSION_RPN);
        match i32_at(s, entry, 0) {
            0 => match i32_at(s, entry, 4) {
                0 => {
                    let _ = write!(out, " i:{}", i32_at(s, entry, 8));
                }
                1 => {
                    let _ = write!(out, " f:{:08x}", s.u32_at(entry, 8).unwrap_or(0));
                }
                _ => {
                    out.push_str(" s:");
                    for b in bytes_at(s, entry, 8).unwrap_or(&[]) {
                        let _ = write!(out, "{b:02x}");
                    }
                }
            },
            1 => {
                let _ = write!(out, " #{}", i32_at(s, entry, 4));
            }
            3 => break,
            other => {
                let _ = write!(out, " rpn?{other}");
            }
        }
    }
    out
}

/// A `GenericEventScript` chain, link by link.
fn event_script(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<MenuEvent> {
    let mut events = Vec::new();
    let mut link = first;
    while let Some(p) = link {
        if events.len() >= MAX_CHAIN {
            break;
        }
        let mut prerequisites = Vec::new();
        let mut condition = ptr_at(s, p, SCRIPT_PREREQUISITES);
        while let Some(c) = condition {
            if prerequisites.len() >= MAX_CHAIN {
                break;
            }
            prerequisites.push(T5ScriptCondition {
                fire_on_true: s.u8_at(c, CONDITION_FIRE_ON_TRUE).unwrap_or(0) != 0,
                construct: i32_at(s, c, CONDITION_CONSTRUCT),
                block: i32_at(s, c, CONDITION_BLOCK),
            });
            condition = ptr_at(s, c, CONDITION_NEXT);
        }
        events.push(MenuEvent::T5Script(T5EventScript {
            prerequisites,
            condition: rpn_dump(s, p.at(SCRIPT_CONDITION)),
            kind: i32_at(s, p, SCRIPT_TYPE),
            fire_on_true: s.u8_at(p, SCRIPT_FIRE_ON_TRUE).unwrap_or(0) != 0,
            action: str_at(s, p, SCRIPT_ACTION).unwrap_or("").to_owned(),
            block: i32_at(s, p, SCRIPT_BLOCK),
            construct: i32_at(s, p, SCRIPT_CONSTRUCT),
        }));
        link = ptr_at(s, p, SCRIPT_NEXT);
    }
    events
}

/// A `GenericEventHandler` chain: each handler's name with its script.
fn event_handlers(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<(String, Vec<MenuEvent>)> {
    let mut handlers = Vec::new();
    let mut link = first;
    while let Some(p) = link {
        if handlers.len() >= MAX_CHAIN {
            break;
        }
        handlers.push((
            str_at(s, p, HANDLER_NAME)
                .unwrap_or("")
                .to_ascii_lowercase(),
            event_script(s, ptr_at(s, p, HANDLER_SCRIPT)),
        ));
        link = ptr_at(s, p, HANDLER_NEXT);
    }
    handlers
}

/// An `ItemKeyHandler` chain: each key with its script.
fn key_handlers(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<(i32, Vec<MenuEvent>)> {
    let mut handlers = Vec::new();
    let mut link = first;
    while let Some(p) = link {
        if handlers.len() >= MAX_CHAIN {
            break;
        }
        handlers.push((
            i32_at(s, p, KEY_HANDLER_KEY),
            event_script(s, ptr_at(s, p, HANDLER_SCRIPT)),
        ));
        link = ptr_at(s, p, HANDLER_NEXT);
    }
    handlers
}

/// A `multiDef_s`'s choices: (shown text, dvar value), the value a string
/// when `strDef` is set and a number otherwise.
fn multi_choices(s: &ZoneStream<'_>, multi: Ptr) -> Vec<(String, String)> {
    let count = (i32_at(s, multi, MULTI_COUNT).max(0) as usize).min(MULTI_MAX);
    let strings = i32_at(s, multi, MULTI_STR_DEF) != 0;
    (0..count)
        .map(|i| {
            let text = bytes_at(s, multi, i * 4).map_or_else(String::new, decode_localized_text);
            let value = if strings {
                str_at(s, multi, MULTI_DVAR_STR + i * 4)
                    .unwrap_or("")
                    .to_owned()
            } else {
                format!("{}", f32_at(s, multi, MULTI_DVAR_VALUE + i * 4))
            };
            (text, value)
        })
        .collect()
}

/// Plain script text as a one-link event script.
fn script_text(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Vec<MenuEvent> {
    match str_at(s, p, off) {
        Some(text) if !text.is_empty() => vec![MenuEvent::Script(text.to_owned())],
        _ => Vec::new(),
    }
}

impl T5MenuSink {
    fn material_key(&self, s: &ZoneStream<'_>, p: Ptr, off: usize) -> String {
        let slot = match s.ptr_at(p, off) {
            Ok(ZonePtr::Offset(target)) => target,
            Ok(ZonePtr::Null) | Err(_) => return String::new(),
            Ok(_) => p.at(off),
        };
        self.material_names
            .get(&slot)
            .map(|name| format!("t5:material/{name}"))
            .unwrap_or_else(String::new)
    }

    fn item(&self, s: &ZoneStream<'_>, p: Ptr) -> MenuItem {
        let ty = i32_at(s, p, ITEM_TYPE);
        let mut item = MenuItem {
            name: str_at(s, p, 0).unwrap_or("").to_owned(),
            item_type: engine_item_type(ty),
            style: i32::from(s.u8_at(p, WINDOW_STYLE).unwrap_or(0)),
            owner_draw: i32_at(s, p, WINDOW_OWNER_DRAW),
            rect: rect_at(s, p.at(ITEM_DEF_WINDOW_RECT)),
            fore_color: color_at(s, p, WINDOW_FORE_COLOR),
            back_color: color_at(s, p, WINDOW_BACK_COLOR),
            rotation: f32_at(s, p, WINDOW_ROTATION),
            background: self.material_key(s, p, WINDOW_BACKGROUND),
            dvar: str_at(s, p, ITEM_DVAR).unwrap_or("").to_owned(),
            dvar_test: str_at(s, p, ITEM_DVAR_TEST).unwrap_or("").to_owned(),
            enable_dvar: str_at(s, p, ITEM_ENABLE_DVAR).unwrap_or("").to_owned(),
            static_flags: i32_at(s, p, WINDOW_STATIC_FLAGS),
            dvar_flags: i32_at(s, p, ITEM_DVAR_FLAGS),
            vis_exp: rpn_dump(s, p.at(ITEM_VISIBLE_EXP)),
            ..MenuItem::default()
        };
        if let Some(data) = ptr_at(s, p, ITEM_TYPE_DATA) {
            if t5_item_has_text_def(ty) {
                item.text_key = bytes_at(s, data, TEXT_DEF_TEXT)
                    .map(decode_localized_text)
                    .unwrap_or_else(String::new);
                if let Some(exp) = ptr_at(s, data, TEXT_DEF_TEXT_EXP) {
                    item.text_exp = rpn_dump(s, exp);
                }
                item.font_enum = i32_at(s, data, TEXT_DEF_FONT);
                item.text_align_mode = i32_at(s, data, TEXT_DEF_ALIGN_MODE);
                item.text_align_x = f32_at(s, data, TEXT_DEF_ALIGN_X);
                item.text_align_y = f32_at(s, data, TEXT_DEF_ALIGN_Y);
                item.text_scale = f32_at(s, data, TEXT_DEF_SCALE);
                item.text_style = i32_at(s, data, TEXT_DEF_STYLE);
            } else if ty == T5_ITEM_TYPE_IMAGE {
                item.material_exp = rpn_dump(s, data);
            }
        }
        if let Some(rects) = ptr_at(s, p, ITEM_RECT_EXP_DATA) {
            for (key, off) in [(0u32, 0usize), (1, 16), (2, 32), (3, 48)] {
                let dump = rpn_dump(s, rects.at(off));
                if !dump.is_empty() {
                    item.float_exp.push((key, dump));
                }
            }
        }
        let alpha = rpn_dump(s, p.at(ITEM_FORECOLOR_A_EXP));
        if !alpha.is_empty() {
            item.float_exp.push((FLOAT_FORECOLOR_A, alpha));
        }
        for (name, events) in event_handlers(s, ptr_at(s, p, ITEM_ON_EVENT)) {
            let handlers = &mut item.handlers;
            match name.as_str() {
                "action" => handlers.action = events,
                "onaccept" => handlers.accept = events,
                "onfocus" => handlers.focus = events,
                "leavefocus" => handlers.leave_focus = events,
                _ => {}
            }
        }
        let focus = match ptr_at(s, p, ITEM_TYPE_DATA) {
            Some(data) if t5_item_is_focus_def(ty) => Some(data),
            Some(data) if t5_item_has_text_def(ty) && t5_text_has_focus_def(ty) => {
                ptr_at(s, data, TEXT_DEF_TYPE_DATA)
            }
            _ => None,
        };
        if let Some(focus) = focus {
            item.handlers.mouse_enter = script_text(s, focus, FOCUS_MOUSE_ENTER);
            item.handlers.mouse_exit = script_text(s, focus, FOCUS_MOUSE_EXIT);
            item.handlers.keys = key_handlers(s, ptr_at(s, focus, FOCUS_ON_KEY));
            let data = ptr_at(s, focus, FOCUS_TYPE_DATA);
            if let Some(multi) = data.filter(|_| ty == T5_ITEM_TYPE_MULTI) {
                item.choices = multi_choices(s, multi);
            } else if let Some(edit) = data.filter(|_| t5_focus_has_edit_field(ty)) {
                let (min, max) = (f32_at(s, edit, EDIT_MIN), f32_at(s, edit, EDIT_MAX));
                if ty == T5_ITEM_TYPE_SLIDER {
                    // `Item_Slider_HandleKey`: a key moves a twentieth of the range.
                    item.slider = Some(MenuSlider {
                        min,
                        max,
                        step: (max - min) * 0.05,
                        display_range: None,
                        decimals: 0,
                        suffix: String::new(),
                    });
                } else {
                    item.edit_field = Some(MenuEditField {
                        max_chars: i32_at(s, edit, EDIT_MAX_CHARS).max(1) as usize,
                        masked: false,
                        starts_empty: false,
                    });
                }
            }
        }
        item
    }
}

impl AssetSink for T5MenuSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_t5::Result<()> {
        self.loading_asset = Some((index, ty));
        self.catalog.walked += 1;
        if ty == AssetType::Font {
            self.catalog.font_headers += 1;
        }
        load_asset_at_observed(s, ty, slot, self)?;
        self.loading_asset = None;
        Ok(())
    }
}

impl AssetLinkSink for T5MenuSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_t5::Result<()> {
        if ty != AssetType::Material {
            return Ok(());
        }
        let Some(name) = s
            .latest_material()
            .and_then(|g| g.name)
            .and_then(|n| s.cstr(n).ok())
        else {
            return Ok(());
        };
        self.material_names.insert(slot, name.to_owned());
        if let Some(insert) = insert_slot {
            self.material_names.insert(insert, name.to_owned());
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_t5::Result<()> {
        if ty == AssetType::Material
            && let Some(name) = self.material_names.get(&target).cloned()
        {
            self.material_names.insert(slot, name);
        }
        Ok(())
    }

    fn capture_font(&mut self, s: &ZoneStream<'_>, p: Ptr) -> fastfile_t5::Result<()> {
        let Some(name) = str_at(s, p, 0) else {
            return Ok(());
        };
        let count = i32_at(s, p, fastfile_t5::size::FONT_GLYPH_COUNT_OFF).max(0) as usize;
        let mut glyphs = Vec::with_capacity(count);
        if let Some(table) = ptr_at(s, p, fastfile_t5::size::FONT_GLYPHS_OFF) {
            let rows = s.slice_at(table, 0, count * fastfile_t5::size::FONT_GLYPH)?;
            for chunk in rows.as_chunks::<{ fastfile_t5::size::FONT_GLYPH }>().0 {
                let mut row = [0u8; fastfile_t5::size::FONT_GLYPH];
                row.copy_from_slice(chunk);
                glyphs.push(GlyphCapture::from_row(&row));
            }
        }
        let material = |off| {
            self.material_key(s, p, off)
                .strip_prefix("t5:material/")
                .map(str::to_owned)
                .unwrap_or_else(String::new)
        };
        self.catalog.fonts.insert(
            name.to_ascii_lowercase(),
            FontDef {
                name: name.to_owned(),
                pixel_height: i32_at(s, p, 4),
                material: material(fastfile_t5::size::FONT_MATERIAL_OFF),
                glow_material: material(fastfile_t5::size::FONT_GLOW_MATERIAL_OFF),
                glyphs,
            },
        );
        Ok(())
    }

    fn capture_menu_list(&mut self, s: &ZoneStream<'_>, p: Ptr) -> fastfile_t5::Result<()> {
        let name = str_at(s, p, 0).unwrap_or("").to_owned();
        let menus = std::mem::take(&mut self.list_menus);
        self.catalog.lists.push((name.clone(), i32_at(s, p, 4)));
        self.catalog.list_menus.insert(name, menus);
        Ok(())
    }

    fn capture_menu_def(&mut self, s: &ZoneStream<'_>, p: Ptr) -> fastfile_t5::Result<()> {
        let Some(name) = str_at(s, p, 0).map(str::to_owned) else {
            return Ok(());
        };
        // A leading comma names a menu another zone defines.
        if let Some(external) = name.strip_prefix(',') {
            self.list_menus.push(external.to_owned());
            return Ok(());
        }
        let mut def = MenuDef::default();
        def.name = name.clone();
        def.sound_name = str_at(s, p, MENU_SOUND_NAME).unwrap_or("").to_owned();
        def.window_background = self.material_key(s, p, WINDOW_BACKGROUND);
        def.window_style = i32::from(s.u8_at(p, WINDOW_STYLE).unwrap_or(0));
        def.window_fore_color = color_at(s, p, WINDOW_FORE_COLOR);
        def.window_back_color = color_at(s, p, WINDOW_BACK_COLOR);
        def.window_border = i32::from(s.u8_at(p, WINDOW_BORDER).unwrap_or(0));
        def.window_border_size = f32_at(s, p, WINDOW_BORDER_SIZE);
        def.fullscreen = i32_at(s, p, MENU_FULLSCREEN);
        // Six windows; a negative id paints on the screen.
        def.ui3d_window = u8::try_from(i32_at(s, p, MENU_UI3D_WINDOW))
            .ok()
            .filter(|&window| usize::from(window) < UI3D_WINDOWS);
        def.static_flags = i32_at(s, p, WINDOW_STATIC_FLAGS);
        def.focus_color = Some(color_at(s, p, MENU_FOCUS_COLOR));
        def.rect = rect_at(s, p.at(ITEM_DEF_WINDOW_RECT));
        def.vis_exp = rpn_dump(s, p.at(MENU_VISIBLE_EXP));
        for (key, off) in [(0u32, MENU_RECT_X_EXP), (1, MENU_RECT_Y_EXP)] {
            let dump = rpn_dump(s, p.at(off));
            if !dump.is_empty() {
                def.float_exp.push((key, dump));
            }
        }
        for (name, events) in event_handlers(s, ptr_at(s, p, MENU_ON_EVENT)) {
            match name.as_str() {
                "onopen" => def.handlers.open = events,
                "onclose" => def.handlers.close = events,
                "onesc" => def.handlers.esc = events,
                _ => {}
            }
        }
        def.handlers.keys = key_handlers(s, ptr_at(s, p, MENU_ON_KEY));
        let count = i32_at(s, p, MENU_ITEM_COUNT).max(0) as usize;
        if let Some(items) = ptr_at(s, p, MENU_ITEMS) {
            for i in 0..count {
                if let Some(item) = ptr_at(s, items, i * 4) {
                    def.items.push(self.item(s, item));
                }
            }
        }
        self.list_menus.push(name.clone());
        self.catalog.menus.insert(name, def);
        Ok(())
    }
}
