//! Black Ops menus, items and fonts read out of the zone into plain data:
//! the `menuDef_t`/`itemDef_s` windows, their event scripts as the zone
//! chains them, and expressions as `t5` postfix programs.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::size as sz;
use crate::zone::{Ptr, ZonePtr, ZoneStream};

const ITEM_DEF_WINDOW_RECT: usize = 4;
const WINDOW_STYLE: usize = 56;
const WINDOW_FRAME_EDGES: usize = 59;
const WINDOW_FRAME_INSET: usize = 60;
const WINDOW_FRAME_SIZE: usize = 64;
const WINDOW_BORDER_COLOR: usize = 124;
const WINDOW_BORDER: usize = 57;
const WINDOW_BORDER_SIZE: usize = 76;
const WINDOW_OWNER_DRAW: usize = 68;
const WINDOW_STATIC_FLAGS: usize = 80;
const WINDOW_FORE_COLOR: usize = 92;
const WINDOW_BACK_COLOR: usize = 108;
const WINDOW_ROTATION: usize = 156;
const WINDOW_BACKGROUND: usize = 160;

const MENU_FULLSCREEN: usize = 168;
const MENU_BLUR_WORLD: usize = 0xd0;
const MENU_UI3D_WINDOW: usize = 172;
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
const ITEM_ANIM_INFO: usize = 0x108;
/// `UI_AnimInfo`: the state the item shows now follows the state count
/// and the states' list.
const ANIM_INFO_CURRENT: usize = 8;
const ANIM_PARAMS_NAME: usize = 0;
const ANIM_PARAMS_RECT: usize = 4;
const ANIM_PARAMS_FORE_COLOR: usize = 32;
const ANIM_PARAMS_BACK_COLOR: usize = 48;

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

/// `ItemDefType` values.
pub const ITEM_TYPE_IMAGE: i32 = 2;
pub const ITEM_TYPE_OWNERDRAW: i32 = 6;
pub const ITEM_TYPE_SLIDER: i32 = 8;
pub const ITEM_TYPE_MULTI: i32 = 0xa;

/// `ItemDefType` values whose text def carries a `focusItemDef_s`.
fn text_has_focus_def(ty: i32) -> bool {
    matches!(
        ty,
        3 | 4 | 5 | 7 | 8 | 9 | 0xa | 0xb | 0xc | 0xd | 0xe | 0x10 | 0x14 | 0x16 | 0x1e
    )
}

/// `ItemDefType` values whose item data is a `focusItemDef_s` itself (a
/// blank button).
fn item_is_focus_def(ty: i32) -> bool {
    matches!(ty, 0x13 | 0x15)
}

/// `ItemDefType` values whose focus def carries an `editFieldDef_s`
/// (`Item_IsEditFieldDef`).
fn focus_has_edit_field(ty: i32) -> bool {
    matches!(ty, 5 | 7 | 8 | 9 | 0xc | 0xd | 0xe | 0x10 | 0x16 | 0x1e)
}

/// `ItemDefType` values whose item carries a `textDef_s`.
fn item_has_text_def(ty: i32) -> bool {
    matches!(
        ty,
        1 | 3 | 4 | 5 | 7 | 8 | 9 | 0xa | 0xb | 0xc | 0xd | 0xe | 0xf | 0x10 | 0x12 | 0x14 | 0x16
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuRectRead {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub horz_align: u8,
    pub vert_align: u8,
}

/// A `windowDef_t` (the start of a menu and of an item).
#[derive(Clone, Debug, Default)]
pub struct WindowRead {
    pub name: Option<String>,
    pub rect: MenuRectRead,
    pub style: u8,
    pub frame_edges: u8,
    pub frame_inset: f32,
    pub frame_size: f32,
    pub border: u8,
    pub border_size: f32,
    pub border_color: [f32; 4],
    pub owner_draw: i32,
    pub static_flags: i32,
    pub fore_color: [f32; 4],
    pub back_color: [f32; 4],
    pub rotation: f32,
    /// The background material's slot, for the name it loaded under.
    pub background: Option<Ptr>,
}

#[derive(Clone, Copy, Debug)]
pub struct ScriptConditionRead {
    pub fire_on_true: bool,
    pub construct: i32,
    pub block: i32,
}

/// One link of a `GenericEventScript` chain.
#[derive(Clone, Debug)]
pub struct EventScriptRead {
    pub prerequisites: Vec<ScriptConditionRead>,
    pub condition: String,
    pub kind: i32,
    pub fire_on_true: bool,
    pub action: String,
    pub block: i32,
    pub construct: i32,
}

/// A `GenericEventHandler`: its name as the zone spells it, and its script.
#[derive(Clone, Debug)]
pub struct EventHandlerRead {
    pub name: String,
    pub script: Vec<EventScriptRead>,
}

#[derive(Clone, Debug)]
pub struct TextDefRead {
    /// The text's bytes (a localize reference or literal text).
    pub text: Option<Vec<u8>>,
    pub text_exp: Option<String>,
    pub font: i32,
    pub align_mode: i32,
    pub align_x: f32,
    pub align_y: f32,
    pub scale: f32,
    pub style: i32,
}

#[derive(Clone, Debug)]
pub struct MultiChoiceRead {
    pub text: Option<Vec<u8>>,
    /// The dvar value: a string when the def is a string def, else the
    /// number as text.
    pub value: String,
}

#[derive(Clone, Copy, Debug)]
pub struct EditFieldRead {
    pub min: f32,
    pub max: f32,
    pub max_chars: i32,
}

#[derive(Clone, Debug)]
pub struct FocusRead {
    pub mouse_enter: Option<String>,
    pub mouse_exit: Option<String>,
    pub keys: Vec<(i32, Vec<EventScriptRead>)>,
    pub multi: Option<Vec<MultiChoiceRead>>,
    pub edit: Option<EditFieldRead>,
}

/// An `itemDef_s`.
#[derive(Clone, Debug)]
pub struct ItemRead {
    pub window: WindowRead,
    /// The item's `ItemDefType`.
    pub item_type: i32,
    pub dvar: Option<String>,
    pub dvar_test: Option<String>,
    pub enable_dvar: Option<String>,
    pub dvar_flags: i32,
    pub vis_exp: String,
    pub text: Option<TextDefRead>,
    /// An image item's material expression.
    pub material_exp: Option<String>,
    /// The x, y, w and h expressions, when the item has any.
    pub rect_exps: Option<[String; 4]>,
    pub alpha_exp: String,
    pub handlers: Vec<EventHandlerRead>,
    pub focus: Option<FocusRead>,
    /// The animation state the item shows as loaded, when it has states.
    pub anim_state: Option<AnimStateRead>,
}

/// An item's animation state (`animParamsDef_t`).
#[derive(Clone, Debug)]
pub struct AnimStateRead {
    pub name: Option<String>,
    pub rect: MenuRectRead,
    pub fore_color: [f32; 4],
    pub back_color: [f32; 4],
}

/// A `menuDef_t`.
#[derive(Clone, Debug)]
pub struct MenuRead {
    pub window: WindowRead,
    pub sound_name: Option<String>,
    pub fullscreen: i32,
    pub blur_world: f32,
    pub ui3d_window: i32,
    pub focus_color: [f32; 4],
    pub vis_exp: String,
    pub rect_x_exp: String,
    pub rect_y_exp: String,
    pub handlers: Vec<EventHandlerRead>,
    pub keys: Vec<(i32, Vec<EventScriptRead>)>,
    pub items: Vec<ItemRead>,
}

/// A `Font_s`.
#[derive(Clone, Debug)]
pub struct FontRead {
    pub name: String,
    pub pixel_height: i32,
    pub glyphs: Vec<[u8; sz::FONT_GLYPH]>,
    pub material: Option<Ptr>,
    pub glow_material: Option<Ptr>,
}

fn str_at<'a>(s: &'a ZoneStream<'_>, p: Ptr, off: usize) -> Option<&'a str> {
    match s.ptr_at(p, off).ok()? {
        ZonePtr::Offset(q) => s.cstr(s.resolve_alias(q)).ok(),
        _ => None,
    }
}

fn string_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Option<String> {
    str_at(s, p, off).map(String::from)
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

fn u8_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> u8 {
    s.u8_at(p, off).unwrap_or(0)
}

fn color_at(s: &ZoneStream<'_>, p: Ptr, off: usize) -> [f32; 4] {
    [0, 4, 8, 12].map(|i| f32_at(s, p, off + i))
}

fn rect_at(s: &ZoneStream<'_>, p: Ptr) -> MenuRectRead {
    MenuRectRead {
        x: f32_at(s, p, 0),
        y: f32_at(s, p, 4),
        w: f32_at(s, p, 8),
        h: f32_at(s, p, 12),
        horz_align: i32_at(s, p, 16) as u8,
        vert_align: i32_at(s, p, 20) as u8,
    }
}

/// The slot of the asset a field points at: the asset's own slot for an
/// offset, the field itself for an inserted asset.
fn asset_slot(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Option<Ptr> {
    match s.ptr_at(p, off) {
        Ok(ZonePtr::Offset(target)) => Some(target),
        Ok(ZonePtr::Null) | Err(_) => None,
        Ok(_) => Some(p.at(off)),
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
        let entry = rpn.at(i * sz::EXPRESSION_RPN);
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
fn event_script(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<EventScriptRead> {
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
            prerequisites.push(ScriptConditionRead {
                fire_on_true: u8_at(s, c, CONDITION_FIRE_ON_TRUE) != 0,
                construct: i32_at(s, c, CONDITION_CONSTRUCT),
                block: i32_at(s, c, CONDITION_BLOCK),
            });
            condition = ptr_at(s, c, CONDITION_NEXT);
        }
        events.push(EventScriptRead {
            prerequisites,
            condition: rpn_dump(s, p.at(SCRIPT_CONDITION)),
            kind: i32_at(s, p, SCRIPT_TYPE),
            fire_on_true: u8_at(s, p, SCRIPT_FIRE_ON_TRUE) != 0,
            action: string_at(s, p, SCRIPT_ACTION).unwrap_or_else(String::new),
            block: i32_at(s, p, SCRIPT_BLOCK),
            construct: i32_at(s, p, SCRIPT_CONSTRUCT),
        });
        link = ptr_at(s, p, SCRIPT_NEXT);
    }
    events
}

/// A `GenericEventHandler` chain: each handler's name with its script.
fn event_handlers(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<EventHandlerRead> {
    let mut handlers = Vec::new();
    let mut link = first;
    while let Some(p) = link {
        if handlers.len() >= MAX_CHAIN {
            break;
        }
        handlers.push(EventHandlerRead {
            name: string_at(s, p, HANDLER_NAME).unwrap_or_else(String::new),
            script: event_script(s, ptr_at(s, p, HANDLER_SCRIPT)),
        });
        link = ptr_at(s, p, HANDLER_NEXT);
    }
    handlers
}

/// An `ItemKeyHandler` chain: each key with its script.
fn key_handlers(s: &ZoneStream<'_>, first: Option<Ptr>) -> Vec<(i32, Vec<EventScriptRead>)> {
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

/// A `multiDef_s`'s choices.
fn multi_choices(s: &ZoneStream<'_>, multi: Ptr) -> Vec<MultiChoiceRead> {
    let count = (i32_at(s, multi, MULTI_COUNT).max(0) as usize).min(MULTI_MAX);
    let strings = i32_at(s, multi, MULTI_STR_DEF) != 0;
    (0..count)
        .map(|i| {
            let value = if strings {
                string_at(s, multi, MULTI_DVAR_STR + i * 4).unwrap_or_else(String::new)
            } else {
                let mut value = String::new();
                let _ = write!(value, "{}", f32_at(s, multi, MULTI_DVAR_VALUE + i * 4));
                value
            };
            MultiChoiceRead {
                text: bytes_at(s, multi, i * 4).map(Vec::from),
                value,
            }
        })
        .collect()
}

fn window(s: &ZoneStream<'_>, p: Ptr) -> WindowRead {
    WindowRead {
        name: string_at(s, p, 0),
        rect: rect_at(s, p.at(ITEM_DEF_WINDOW_RECT)),
        style: u8_at(s, p, WINDOW_STYLE),
        frame_edges: u8_at(s, p, WINDOW_FRAME_EDGES),
        frame_inset: f32_at(s, p, WINDOW_FRAME_INSET),
        frame_size: f32_at(s, p, WINDOW_FRAME_SIZE),
        border: u8_at(s, p, WINDOW_BORDER),
        border_size: f32_at(s, p, WINDOW_BORDER_SIZE),
        border_color: color_at(s, p, WINDOW_BORDER_COLOR),
        owner_draw: i32_at(s, p, WINDOW_OWNER_DRAW),
        static_flags: i32_at(s, p, WINDOW_STATIC_FLAGS),
        fore_color: color_at(s, p, WINDOW_FORE_COLOR),
        back_color: color_at(s, p, WINDOW_BACK_COLOR),
        rotation: f32_at(s, p, WINDOW_ROTATION),
        background: asset_slot(s, p, WINDOW_BACKGROUND),
    }
}

fn item(s: &ZoneStream<'_>, p: Ptr) -> ItemRead {
    let ty = i32_at(s, p, ITEM_TYPE);
    let data = ptr_at(s, p, ITEM_TYPE_DATA);
    let text = data
        .filter(|_| item_has_text_def(ty))
        .map(|data| TextDefRead {
            text: bytes_at(s, data, TEXT_DEF_TEXT).map(Vec::from),
            text_exp: ptr_at(s, data, TEXT_DEF_TEXT_EXP).map(|exp| rpn_dump(s, exp)),
            font: i32_at(s, data, TEXT_DEF_FONT),
            align_mode: i32_at(s, data, TEXT_DEF_ALIGN_MODE),
            align_x: f32_at(s, data, TEXT_DEF_ALIGN_X),
            align_y: f32_at(s, data, TEXT_DEF_ALIGN_Y),
            scale: f32_at(s, data, TEXT_DEF_SCALE),
            style: i32_at(s, data, TEXT_DEF_STYLE),
        });
    let material_exp = data
        .filter(|_| !item_has_text_def(ty) && ty == ITEM_TYPE_IMAGE)
        .map(|data| rpn_dump(s, data));
    let rect_exps = ptr_at(s, p, ITEM_RECT_EXP_DATA)
        .map(|rects| [0, 16, 32, 48].map(|off| rpn_dump(s, rects.at(off))));
    let focus = match data {
        Some(data) if item_is_focus_def(ty) => Some(data),
        Some(data) if item_has_text_def(ty) && text_has_focus_def(ty) => {
            ptr_at(s, data, TEXT_DEF_TYPE_DATA)
        }
        _ => None,
    };
    let focus = focus.map(|focus| {
        let data = ptr_at(s, focus, FOCUS_TYPE_DATA);
        let multi = data
            .filter(|_| ty == ITEM_TYPE_MULTI)
            .map(|multi| multi_choices(s, multi));
        let edit = data
            .filter(|_| multi.is_none() && focus_has_edit_field(ty))
            .map(|edit| EditFieldRead {
                min: f32_at(s, edit, EDIT_MIN),
                max: f32_at(s, edit, EDIT_MAX),
                max_chars: i32_at(s, edit, EDIT_MAX_CHARS),
            });
        FocusRead {
            mouse_enter: string_at(s, focus, FOCUS_MOUSE_ENTER),
            mouse_exit: string_at(s, focus, FOCUS_MOUSE_EXIT),
            keys: key_handlers(s, ptr_at(s, focus, FOCUS_ON_KEY)),
            multi,
            edit,
        }
    });
    ItemRead {
        window: window(s, p),
        item_type: ty,
        dvar: string_at(s, p, ITEM_DVAR),
        dvar_test: string_at(s, p, ITEM_DVAR_TEST),
        enable_dvar: string_at(s, p, ITEM_ENABLE_DVAR),
        dvar_flags: i32_at(s, p, ITEM_DVAR_FLAGS),
        vis_exp: rpn_dump(s, p.at(ITEM_VISIBLE_EXP)),
        text,
        material_exp,
        rect_exps,
        alpha_exp: rpn_dump(s, p.at(ITEM_FORECOLOR_A_EXP)),
        handlers: event_handlers(s, ptr_at(s, p, ITEM_ON_EVENT)),
        focus,
        anim_state: ptr_at(s, p, ITEM_ANIM_INFO).map(|info| {
            let state = info.at(ANIM_INFO_CURRENT);
            AnimStateRead {
                name: string_at(s, state, ANIM_PARAMS_NAME),
                rect: rect_at(s, state.at(ANIM_PARAMS_RECT)),
                fore_color: color_at(s, state, ANIM_PARAMS_FORE_COLOR),
                back_color: color_at(s, state, ANIM_PARAMS_BACK_COLOR),
            }
        }),
    }
}

impl ZoneStream<'_> {
    /// The `menuDef_t` at `p`, with its items.
    pub fn read_menu_def(&self, p: Ptr) -> MenuRead {
        let s = self;
        let count = i32_at(s, p, MENU_ITEM_COUNT).max(0) as usize;
        let items = ptr_at(s, p, MENU_ITEMS)
            .map(|items| {
                (0..count)
                    .filter_map(|i| ptr_at(s, items, i * 4))
                    .map(|p| item(s, p))
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        MenuRead {
            window: window(s, p),
            sound_name: string_at(s, p, MENU_SOUND_NAME),
            fullscreen: i32_at(s, p, MENU_FULLSCREEN),
            blur_world: f32_at(s, p, MENU_BLUR_WORLD),
            ui3d_window: i32_at(s, p, MENU_UI3D_WINDOW),
            focus_color: color_at(s, p, MENU_FOCUS_COLOR),
            vis_exp: rpn_dump(s, p.at(MENU_VISIBLE_EXP)),
            rect_x_exp: rpn_dump(s, p.at(MENU_RECT_X_EXP)),
            rect_y_exp: rpn_dump(s, p.at(MENU_RECT_Y_EXP)),
            handlers: event_handlers(s, ptr_at(s, p, MENU_ON_EVENT)),
            keys: key_handlers(s, ptr_at(s, p, MENU_ON_KEY)),
            items,
        }
    }

    /// The name of the menu, list or font asset whose header is at `p`.
    pub fn read_asset_name(&self, p: Ptr) -> Option<&str> {
        str_at(self, p, 0)
    }

    /// A `MenuList`'s menu count.
    pub fn read_menu_list_count(&self, p: Ptr) -> i32 {
        i32_at(self, p, 4)
    }

    /// The `Font_s` at `p`: its glyph rows as the zone stores them.
    pub fn read_font(&self, p: Ptr) -> crate::Result<Option<FontRead>> {
        let s = self;
        let Some(name) = string_at(s, p, 0) else {
            return Ok(None);
        };
        let count = i32_at(s, p, sz::FONT_GLYPH_COUNT_OFF).max(0) as usize;
        let mut glyphs = Vec::with_capacity(count);
        if let Some(table) = ptr_at(s, p, sz::FONT_GLYPHS_OFF) {
            let rows = s.slice_at(table, 0, count * sz::FONT_GLYPH)?;
            for chunk in rows.as_chunks::<{ sz::FONT_GLYPH }>().0 {
                glyphs.push(*chunk);
            }
        }
        Ok(Some(FontRead {
            name,
            pixel_height: i32_at(s, p, 4),
            glyphs,
            material: asset_slot(s, p, sz::FONT_MATERIAL_OFF),
            glow_material: asset_slot(s, p, sz::FONT_GLOW_MATERIAL_OFF),
        }))
    }
}
