//! Black Ops 2 menus, captured from the zones that ship them (`patch_zm`,
//! the map's zone): the legacy menu definitions the client's HUD still draws
//! its code-drawn pieces with (cursor hints, the hit indicator, the hold
//! breath hints, the death screen).
//!
//! Black Ops 2's `menuDef_t`, `GenericEventScript` and `expressionRpn` lay
//! out as Black Ops' do; its `itemDef_s` and `textDef_s` move fields (the
//! zone schema's load plans name every offset). Expressions are stored as
//! `t6` postfix programs: operators keep Black Ops' numbers, functions are
//! their token's index in the game's `expressions/functions.txt` plus 24.

use std::fmt::Write as _;
use std::path::Path;

use asset_core::{Family, T6};
use fastfile_t6 as ff;
use ff::{AssetType, LoadedAsset, Ptr, ZoneLoad};

use crate::localize::decode_localized_text;
use crate::menu_catalog::{
    MenuCatalog, MenuDef, MenuEvent, MenuItem, MenuRect, T5EventScript, T5ScriptCondition,
};

/// The HUD menu lists a Black Ops 2 zombies client loads, in its load order
/// (`CG_LoadHudMenus` outside splitscreen): the zombies HUD, the gametype's
/// HUD (`ui_mp/hud_<g_gametype>.txt`), then the popups. Each menu's own
/// visibility decides when it shows.
pub fn t6_hud_menu_lists(gametype: &str) -> [String; 3] {
    [
        "ui_mp/hud_zombies.txt".to_owned(),
        format!("ui_mp/hud_{gametype}.txt"),
        "ui_mp/hud_popups.txt".to_owned(),
    ]
}

/// The `float_exp` slot of an item's forecolor alpha in the menu engine.
const FLOAT_FORECOLOR_A: u32 = 8;

// windowDef_t (164 bytes), as Black Ops'.
const WINDOW_RECT: usize = 4;
const WINDOW_RECT_CLIENT: usize = 28;
const WINDOW_STYLE: usize = 56;
const WINDOW_OWNER_DRAW: usize = 68;
const WINDOW_STATIC_FLAGS: usize = 80;
const WINDOW_FORE_COLOR: usize = 92;
const WINDOW_BACK_COLOR: usize = 108;
const WINDOW_BACKGROUND: usize = 160;

// menuDef_t (400 bytes).
const MENU_FULLSCREEN: usize = 168;
const MENU_ITEM_COUNT: usize = 176;
const MENU_ON_EVENT: usize = 268;
const MENU_ON_KEY: usize = 272;
const MENU_VISIBLE_EXP: usize = 276;
const MENU_SOUND_NAME: usize = 316;
const MENU_FOCUS_COLOR: usize = 328;
const MENU_RECT_X_EXP: usize = 360;
const MENU_RECT_Y_EXP: usize = 376;
const MENU_ITEMS: usize = 392;

// itemDef_s (272 bytes).
const ITEM_TYPE: usize = 164;
const ITEM_DVAR: usize = 176;
const ITEM_DVAR_TEST: usize = 180;
const ITEM_ENABLE_DVAR: usize = 184;
const ITEM_DVAR_FLAGS: usize = 188;
const ITEM_TYPE_DATA: usize = 196;
const ITEM_RECT_EXP_DATA: usize = 204;
const ITEM_VISIBLE_EXP: usize = 208;
const ITEM_FORECOLOR_A_EXP: usize = 240;
const ITEM_ON_EVENT: usize = 260;

// textDef_s (140 bytes).
const TEXT_DEF_FONT: usize = 28;
const TEXT_DEF_ALIGN_MODE: usize = 36;
const TEXT_DEF_ALIGN_X: usize = 40;
const TEXT_DEF_ALIGN_Y: usize = 44;
const TEXT_DEF_SCALE: usize = 48;
const TEXT_DEF_STYLE: usize = 56;
const TEXT_DEF_TEXT: usize = 128;
const TEXT_DEF_TEXT_EXP: usize = 132;

// ExpressionStatement (16 bytes) and expressionRpn (12 bytes).
const STATEMENT_COUNT: usize = 8;
const STATEMENT_RPN: usize = 12;
const EXPRESSION_RPN: usize = 12;

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

const ITEM_TYPE_IMAGE: i32 = 2;

/// Black Ops 2 `ItemDefType` values whose item carries a `textDef_s` (the
/// load plan's `itemDefData_t` cases), as Black Ops'.
fn item_has_text_def(ty: i32) -> bool {
    matches!(
        ty,
        1 | 3 | 4 | 5 | 7 | 8 | 9 | 0xa | 0xb | 0xc | 0xd | 0xe | 0xf | 0x10 | 0x12 | 0x14 | 0x16
    )
}

/// The menu engine's item type for a Black Ops 2 `ItemDefType`, whose
/// values are Black Ops'.
fn engine_item_type(ty: i32) -> i32 {
    match ty {
        0 | 1 | ITEM_TYPE_IMAGE => 0,
        3 | 0x15 => 1,
        0x19 => 2,
        0x1b => 3,
        5 => 4,
        0x1c => 5,
        4 => 6,
        0x1a => 7,
        6 | 0x12 | 0x13 | 0x14 => 8,
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

/// A struct's bytes: an asset's own header or a block's.
#[derive(Clone, Copy)]
enum At<'z> {
    Header(&'z LoadedAsset),
    Block(Ptr),
}

struct Reader<'z> {
    load: &'z ZoneLoad,
}

impl<'z> Reader<'z> {
    fn u32(&self, at: At<'z>, off: usize) -> u32 {
        match at {
            At::Header(asset) => asset
                .header
                .get(off..off + 4)
                .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap())),
            At::Block(p) => self.load.blocks.u32_at(p.at(off as u32)).unwrap_or(0),
        }
    }

    fn i32(&self, at: At<'z>, off: usize) -> i32 {
        self.u32(at, off) as i32
    }

    fn f32(&self, at: At<'z>, off: usize) -> f32 {
        f32::from_bits(self.u32(at, off))
    }

    fn u8(&self, at: At<'z>, off: usize) -> u8 {
        match at {
            At::Header(asset) => asset.header.get(off).copied().unwrap_or(0),
            At::Block(p) => self
                .load
                .blocks
                .bytes(p.at(off as u32), 1)
                .map_or(0, |b| b[0]),
        }
    }

    fn ptr(&self, at: At<'z>, off: usize) -> Option<Ptr> {
        match at {
            At::Header(_) => decode_ptr(self.u32(at, off)),
            At::Block(p) => self.load.blocks.ptr_at(p.at(off as u32)).ok().flatten(),
        }
    }

    fn bytes(&self, at: At<'z>, off: usize) -> Option<&'z [u8]> {
        self.load.blocks.cstr(self.ptr(at, off)?).ok()
    }

    fn str(&self, at: At<'z>, off: usize) -> Option<&'z str> {
        core::str::from_utf8(self.bytes(at, off)?).ok()
    }

    fn color(&self, at: At<'z>, off: usize) -> [f32; 4] {
        [0, 4, 8, 12].map(|i| self.f32(at, off + i))
    }

    fn rect(&self, at: At<'z>, off: usize) -> MenuRect {
        MenuRect {
            x: self.f32(at, off),
            y: self.f32(at, off + 4),
            w: self.f32(at, off + 8),
            h: self.f32(at, off + 12),
            horz_align: self.i32(at, off + 16) as u8,
            vert_align: self.i32(at, off + 20) as u8,
        }
    }

    /// The material a pointer field names, as a `t6:material/<name>` key.
    fn material(&self, owner: &LoadedAsset, at: At<'z>, off: usize) -> String {
        let asset = match at {
            At::Header(asset) => asset.field(off as u32).map(|i| &self.load.assets[i]),
            At::Block(p) => self.load.asset_in(owner, p.at(off as u32)),
        };
        let Some(asset) = asset.filter(|a| a.ty == AssetType::Material) else {
            return String::new();
        };
        asset
            .header
            .get(..4)
            .and_then(|b| decode_ptr(u32::from_le_bytes(b.try_into().unwrap())))
            .and_then(|p| self.load.blocks.cstr(p).ok())
            .and_then(|b| core::str::from_utf8(b).ok())
            .map_or_else(String::new, |name| format!("t6:material/{name}"))
    }

    /// An `ExpressionStatement` as a `t6` postfix program: `i:`/`f:`/`s:`
    /// constants and `#n` operators or functions, in the zone's order.
    fn rpn(&self, at: At<'z>, off: usize) -> String {
        let count = self.i32(at, off + STATEMENT_COUNT);
        let Some(rpn) = self.ptr(at, off + STATEMENT_RPN).filter(|_| count > 0) else {
            return String::new();
        };
        let mut out = String::from(T6::ID.as_str());
        for i in 0..count as usize {
            let entry = At::Block(rpn.at((i * EXPRESSION_RPN) as u32));
            match self.i32(entry, 0) {
                0 => match self.i32(entry, 4) {
                    0 => {
                        let _ = write!(out, " i:{}", self.i32(entry, 8));
                    }
                    1 => {
                        let _ = write!(out, " f:{:08x}", self.u32(entry, 8));
                    }
                    _ => {
                        out.push_str(" s:");
                        for b in self.bytes(entry, 8).unwrap_or(&[]) {
                            let _ = write!(out, "{b:02x}");
                        }
                    }
                },
                1 => {
                    let _ = write!(out, " #{}", self.i32(entry, 4));
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
    fn event_script(&self, first: Option<Ptr>) -> Vec<MenuEvent> {
        let mut events = Vec::new();
        let mut link = first;
        while let Some(p) = link {
            if events.len() >= MAX_CHAIN {
                break;
            }
            let at = At::Block(p);
            let mut prerequisites = Vec::new();
            let mut condition = self.ptr(at, SCRIPT_PREREQUISITES);
            while let Some(c) = condition {
                if prerequisites.len() >= MAX_CHAIN {
                    break;
                }
                let c = At::Block(c);
                prerequisites.push(T5ScriptCondition {
                    fire_on_true: self.u8(c, CONDITION_FIRE_ON_TRUE) != 0,
                    construct: self.i32(c, CONDITION_CONSTRUCT),
                    block: self.i32(c, CONDITION_BLOCK),
                });
                condition = self.ptr(c, CONDITION_NEXT);
            }
            events.push(MenuEvent::T5Script(T5EventScript {
                prerequisites,
                condition: self.rpn(at, SCRIPT_CONDITION),
                kind: self.i32(at, SCRIPT_TYPE),
                fire_on_true: self.u8(at, SCRIPT_FIRE_ON_TRUE) != 0,
                action: self.str(at, SCRIPT_ACTION).unwrap_or("").to_owned(),
                block: self.i32(at, SCRIPT_BLOCK),
                construct: self.i32(at, SCRIPT_CONSTRUCT),
            }));
            link = self.ptr(at, SCRIPT_NEXT);
        }
        events
    }

    /// A `GenericEventHandler` chain: each handler's name with its script.
    fn event_handlers(&self, first: Option<Ptr>) -> Vec<(String, Vec<MenuEvent>)> {
        let mut handlers = Vec::new();
        let mut link = first;
        while let Some(p) = link {
            if handlers.len() >= MAX_CHAIN {
                break;
            }
            let at = At::Block(p);
            handlers.push((
                self.str(at, HANDLER_NAME)
                    .unwrap_or("")
                    .to_ascii_lowercase(),
                self.event_script(self.ptr(at, HANDLER_SCRIPT)),
            ));
            link = self.ptr(at, HANDLER_NEXT);
        }
        handlers
    }

    /// An `ItemKeyHandler` chain: each key with its script.
    fn key_handlers(&self, first: Option<Ptr>) -> Vec<(i32, Vec<MenuEvent>)> {
        let mut handlers = Vec::new();
        let mut link = first;
        while let Some(p) = link {
            if handlers.len() >= MAX_CHAIN {
                break;
            }
            let at = At::Block(p);
            handlers.push((
                self.i32(at, KEY_HANDLER_KEY),
                self.event_script(self.ptr(at, HANDLER_SCRIPT)),
            ));
            link = self.ptr(at, HANDLER_NEXT);
        }
        handlers
    }

    fn item(&self, owner: &LoadedAsset, p: Ptr) -> MenuItem {
        let at = At::Block(p);
        let ty = self.i32(at, ITEM_TYPE);
        let mut item = MenuItem {
            name: self.str(at, 0).unwrap_or("").to_owned(),
            item_type: engine_item_type(ty),
            style: i32::from(self.u8(at, WINDOW_STYLE)),
            owner_draw: self.i32(at, WINDOW_OWNER_DRAW),
            rect: self.rect(at, WINDOW_RECT_CLIENT),
            fore_color: self.color(at, WINDOW_FORE_COLOR),
            back_color: self.color(at, WINDOW_BACK_COLOR),
            background: self.material(owner, at, WINDOW_BACKGROUND),
            dvar: self.str(at, ITEM_DVAR).unwrap_or("").to_owned(),
            dvar_test: self.str(at, ITEM_DVAR_TEST).unwrap_or("").to_owned(),
            enable_dvar: self.str(at, ITEM_ENABLE_DVAR).unwrap_or("").to_owned(),
            static_flags: self.i32(at, WINDOW_STATIC_FLAGS),
            dvar_flags: self.i32(at, ITEM_DVAR_FLAGS),
            vis_exp: self.rpn(at, ITEM_VISIBLE_EXP),
            ..MenuItem::default()
        };
        if let Some(data) = self.ptr(at, ITEM_TYPE_DATA) {
            let data = At::Block(data);
            if item_has_text_def(ty) {
                item.text_key = self
                    .bytes(data, TEXT_DEF_TEXT)
                    .map_or_else(String::new, decode_localized_text);
                if let Some(exp) = self.ptr(data, TEXT_DEF_TEXT_EXP) {
                    item.text_exp = self.rpn(At::Block(exp), 0);
                }
                item.font_enum = self.i32(data, TEXT_DEF_FONT);
                item.text_align_mode = self.i32(data, TEXT_DEF_ALIGN_MODE);
                item.text_align_x = self.f32(data, TEXT_DEF_ALIGN_X);
                item.text_align_y = self.f32(data, TEXT_DEF_ALIGN_Y);
                item.text_scale = self.f32(data, TEXT_DEF_SCALE);
                item.text_style = self.i32(data, TEXT_DEF_STYLE);
            } else if ty == ITEM_TYPE_IMAGE {
                item.material_exp = self.rpn(data, 0);
            }
        }
        if let Some(rects) = self.ptr(at, ITEM_RECT_EXP_DATA) {
            for (key, off) in [(0u32, 0usize), (1, 16), (2, 32), (3, 48)] {
                let dump = self.rpn(At::Block(rects), off);
                if !dump.is_empty() {
                    item.float_exp.push((key, dump));
                }
            }
        }
        let alpha = self.rpn(at, ITEM_FORECOLOR_A_EXP);
        if !alpha.is_empty() {
            item.float_exp.push((FLOAT_FORECOLOR_A, alpha));
        }
        for (name, events) in self.event_handlers(self.ptr(at, ITEM_ON_EVENT)) {
            let handlers = &mut item.handlers;
            match name.as_str() {
                "action" => handlers.action = events,
                "onaccept" => handlers.accept = events,
                "onfocus" => handlers.focus = events,
                "leavefocus" => handlers.leave_focus = events,
                _ => {}
            }
        }
        item
    }

    fn menu(&self, menu: &LoadedAsset) -> Option<MenuDef> {
        let at = At::Header(menu);
        let name = self.str(at, 0)?.to_owned();
        let mut def = MenuDef::default();
        def.name = name;
        def.sound_name = self.str(at, MENU_SOUND_NAME).unwrap_or("").to_owned();
        def.window_background = self.material(menu, at, WINDOW_BACKGROUND);
        def.fullscreen = self.i32(at, MENU_FULLSCREEN);
        def.static_flags = self.i32(at, WINDOW_STATIC_FLAGS);
        def.focus_color = Some(self.color(at, MENU_FOCUS_COLOR));
        def.rect = self.rect(at, WINDOW_RECT);
        def.vis_exp = self.rpn(at, MENU_VISIBLE_EXP);
        for (key, off) in [(0u32, MENU_RECT_X_EXP), (1, MENU_RECT_Y_EXP)] {
            let dump = self.rpn(at, off);
            if !dump.is_empty() {
                def.float_exp.push((key, dump));
            }
        }
        for (name, events) in self.event_handlers(self.ptr(at, MENU_ON_EVENT)) {
            match name.as_str() {
                "onopen" => def.handlers.open = events,
                "onclose" => def.handlers.close = events,
                "onesc" => def.handlers.esc = events,
                _ => {}
            }
        }
        def.handlers.keys = self.key_handlers(self.ptr(at, MENU_ON_KEY));
        let count = self.i32(at, MENU_ITEM_COUNT).max(0) as usize;
        if let Some(items) = self.ptr(at, MENU_ITEMS) {
            for i in 0..count {
                if let Some(item) = self.ptr(At::Block(items), i * 4) {
                    def.items.push(self.item(menu, item));
                }
            }
        }
        Some(def)
    }
}

fn decode_ptr(raw: u32) -> Option<Ptr> {
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    Some(Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    })
}

/// The menu lists and menus a loaded Black Ops 2 zone carries.
pub fn t6_menu_catalog(load: &ZoneLoad) -> MenuCatalog {
    let reader = Reader { load };
    let mut catalog = MenuCatalog {
        namespace: Some(T6::ID),
        ..MenuCatalog::default()
    };
    for asset in &load.assets {
        catalog.walked += 1;
        if asset.ty == AssetType::Menu
            && let Some(def) = reader.menu(asset)
        {
            catalog.menus.insert(def.name.clone(), def);
        }
    }
    for list in load.assets.iter().filter(|a| a.ty == AssetType::MenuList) {
        let at = At::Header(list);
        let name = reader.str(at, 0).unwrap_or("").to_owned();
        let count = reader.i32(at, 4).max(0) as usize;
        let mut menus = Vec::new();
        if let Some(array) = reader.ptr(at, 8) {
            for i in 0..count {
                let Some(menu) = load.asset_in(list, array.at((i * 4) as u32)) else {
                    continue;
                };
                if let Some(def) = reader.menu(menu) {
                    menus.push(def.name.clone());
                    catalog.menus.entry(def.name.clone()).or_insert(def);
                }
            }
        }
        catalog.lists.push((name.clone(), count as i32));
        catalog.list_menus.insert(name, menus);
    }
    catalog
}

/// The menus of a Black Ops 2 zone file.
pub fn load_t6_menu_catalog(path: &Path) -> Result<MenuCatalog, String> {
    let image = asset_transport::open_t6_zone(path).map_err(|e| format!("{e:?}"))?;
    let schema = ff::schema::parse().map_err(|e| format!("T6 load plan: {e:?}"))?;
    let (load, walked) = ff::load_zone(&schema, &image.bytes, |_, _| true);
    walked.map_err(|e| format!("{}: {e:?}", path.display()))?;
    Ok(t6_menu_catalog(&load))
}
