//! Black Ops menus and fonts, captured from the zones that ship them
//! (`code_post_gfx`, `patch`, the localized `code_post_gfx`).
//!
//! Item types and expressions keep Black Ops' meaning: expressions are stored
//! as `t5` postfix programs (see `hud_iw4::expr_t5`), item types are mapped to
//! the menu engine's types by their names in the Black Ops enum.

use std::collections::HashMap;
use std::path::Path;

use asset_transport::{T5ZoneMemory, open_zone};
use fastfile_iw4::GlyphCapture;
use fastfile_t5::menu_read::{
    EventHandlerRead, EventScriptRead, ITEM_TYPE_IMAGE, ITEM_TYPE_OWNERDRAW, ITEM_TYPE_SLIDER,
    ItemRead, MenuRectRead, WindowRead,
};
use fastfile_t5::{
    AssetLinkSink, AssetSink, AssetType, Ptr, Result as ZoneResult, ScriptStrings, ZoneStream,
    load_asset_at_observed, load_zone,
};

use crate::localize::decode_localized_text;
use crate::menu_catalog::{
    FontDef, MenuBorder, MenuCatalog, MenuDef, MenuEditField, MenuEvent, MenuFrame, MenuItem,
    MenuRect, MenuSlider, T5EventScript, T5ScriptCondition,
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

/// The window styles that draw their picture as a frame.
const STYLE_FRAMED: [u8; 2] = [10, 11];
/// Black Ops' 3D UI windows (`ui3dsetwindow` ids 0 to 5).
const UI3D_WINDOWS: usize = 6;

/// The menu engine's item type for a Black Ops `ItemDefType`, matched by name
/// (`ITEM_TYPE_TEXT`, `ITEM_TYPE_BUTTON`, ...). Black Ops' image and
/// owner-draw text items draw like the engine's text and owner-draw items.
fn engine_item_type(t5: i32) -> i32 {
    match t5 {
        0 | 1 | ITEM_TYPE_IMAGE => 0,
        3 | 0x15 => 1,
        0x19 => 2,
        0x1b => 3,
        5 => 4,
        0x1c => 5,
        4 => 6,
        0x1a => 7,
        ITEM_TYPE_OWNERDRAW | 0x12 | 0x13 | 0x14 => 8,
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

fn rect(read: MenuRectRead) -> MenuRect {
    MenuRect {
        x: read.x,
        y: read.y,
        w: read.w,
        h: read.h,
        horz_align: read.horz_align,
        vert_align: read.vert_align,
    }
}

fn event_script(script: Vec<EventScriptRead>) -> Vec<MenuEvent> {
    script
        .into_iter()
        .map(|link| {
            MenuEvent::T5Script(T5EventScript {
                prerequisites: link
                    .prerequisites
                    .into_iter()
                    .map(|c| T5ScriptCondition {
                        fire_on_true: c.fire_on_true,
                        construct: c.construct,
                        block: c.block,
                    })
                    .collect(),
                condition: link.condition,
                kind: link.kind,
                fire_on_true: link.fire_on_true,
                action: link.action,
                block: link.block,
                construct: link.construct,
            })
        })
        .collect()
}

/// Each handler's lowercased name with its script.
fn event_handlers(handlers: Vec<EventHandlerRead>) -> Vec<(String, Vec<MenuEvent>)> {
    handlers
        .into_iter()
        .map(|h| (h.name.to_ascii_lowercase(), event_script(h.script)))
        .collect()
}

fn key_handlers(keys: Vec<(i32, Vec<EventScriptRead>)>) -> Vec<(i32, Vec<MenuEvent>)> {
    keys.into_iter()
        .map(|(key, script)| (key, event_script(script)))
        .collect()
}

/// Plain script text as a one-link event script.
fn script_text(text: Option<String>) -> Vec<MenuEvent> {
    match text {
        Some(text) if !text.is_empty() => vec![MenuEvent::Script(text)],
        _ => Vec::new(),
    }
}

impl T5MenuSink {
    fn material_key(&self, slot: Option<Ptr>) -> String {
        slot.and_then(|slot| self.material_names.get(&slot))
            .map(|name| format!("t5:material/{name}"))
            .unwrap_or_else(String::new)
    }

    fn window_frame(window: &WindowRead) -> Option<MenuFrame> {
        STYLE_FRAMED.contains(&window.style).then_some(MenuFrame {
            size: window.frame_size,
            inset: window.frame_inset,
            edges: window.frame_edges,
        })
    }

    fn item(&self, read: ItemRead) -> MenuItem {
        let ty = read.item_type;
        let window = read.window;
        let mut item = MenuItem {
            name: window.name.clone().unwrap_or_else(String::new),
            item_type: engine_item_type(ty),
            style: i32::from(window.style),
            owner_draw: window.owner_draw,
            rect: rect(window.rect),
            fore_color: window.fore_color,
            back_color: window.back_color,
            rotation: window.rotation,
            frame: Self::window_frame(&window),
            border: (window.border != 0).then_some(MenuBorder {
                kind: window.border,
                size: window.border_size,
                color: window.border_color,
            }),
            background: self.material_key(window.background),
            dvar: read.dvar.unwrap_or_else(String::new),
            dvar_test: read.dvar_test.unwrap_or_else(String::new),
            enable_dvar: read.enable_dvar.unwrap_or_else(String::new),
            static_flags: window.static_flags,
            dvar_flags: read.dvar_flags,
            vis_exp: read.vis_exp,
            ..MenuItem::default()
        };
        if let Some(text) = read.text {
            item.text_key = text
                .text
                .as_deref()
                .map(decode_localized_text)
                .unwrap_or_else(String::new);
            if let Some(exp) = text.text_exp {
                item.text_exp = exp;
            }
            item.font_enum = text.font;
            item.text_align_mode = text.align_mode;
            item.text_align_x = text.align_x;
            item.text_align_y = text.align_y;
            item.text_scale = text.scale;
            item.text_style = text.style;
        } else if let Some(exp) = read.material_exp {
            item.material_exp = exp;
        }
        if let Some(rects) = read.rect_exps {
            for (key, dump) in (0u32..).zip(rects) {
                if !dump.is_empty() {
                    item.float_exp.push((key, dump));
                }
            }
        }
        if !read.alpha_exp.is_empty() {
            item.float_exp.push((FLOAT_FORECOLOR_A, read.alpha_exp));
        }
        for (name, events) in event_handlers(read.handlers) {
            let handlers = &mut item.handlers;
            match name.as_str() {
                "action" => handlers.action = events,
                "onaccept" => handlers.accept = events,
                "onfocus" => handlers.focus = events,
                "leavefocus" => handlers.leave_focus = events,
                _ => {}
            }
        }
        if let Some(focus) = read.focus {
            item.handlers.mouse_enter = script_text(focus.mouse_enter);
            item.handlers.mouse_exit = script_text(focus.mouse_exit);
            item.handlers.keys = key_handlers(focus.keys);
            if let Some(multi) = focus.multi {
                item.choices = multi
                    .into_iter()
                    .map(|choice| {
                        let text = choice
                            .text
                            .as_deref()
                            .map_or_else(String::new, decode_localized_text);
                        (text, choice.value)
                    })
                    .collect();
            } else if let Some(edit) = focus.edit {
                let (min, max) = (edit.min, edit.max);
                if ty == ITEM_TYPE_SLIDER {
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
                        max_chars: edit.max_chars.max(1) as usize,
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
    ) -> ZoneResult<()> {
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
    ) -> ZoneResult<()> {
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

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> ZoneResult<()> {
        if ty == AssetType::Material
            && let Some(name) = self.material_names.get(&target).cloned()
        {
            self.material_names.insert(slot, name);
        }
        Ok(())
    }

    fn capture_font(&mut self, s: &ZoneStream<'_>, p: Ptr) -> ZoneResult<()> {
        let Some(font) = s.read_font(p)? else {
            return Ok(());
        };
        let material = |slot| {
            self.material_key(slot)
                .strip_prefix("t5:material/")
                .map(str::to_owned)
                .unwrap_or_else(String::new)
        };
        self.catalog.fonts.insert(
            font.name.to_ascii_lowercase(),
            FontDef {
                pixel_height: font.pixel_height,
                material: material(font.material),
                glow_material: material(font.glow_material),
                glyphs: font.glyphs.iter().map(GlyphCapture::from_row).collect(),
                name: font.name,
            },
        );
        Ok(())
    }

    /// The game's config scripts (`default_controls.cfg`, ...) its menus and
    /// key binds run.
    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ZoneResult<()> {
        if name.ends_with(".cfg")
            && let Some(text) = asset_world::decode_rawfile_text(data, zlib_compressed)
        {
            self.catalog.rawfiles.insert(name.to_owned(), text);
        }
        Ok(())
    }

    fn capture_menu_list(&mut self, s: &ZoneStream<'_>, p: Ptr) -> ZoneResult<()> {
        let name = s.read_asset_name(p).unwrap_or("").to_owned();
        let menus = std::mem::take(&mut self.list_menus);
        self.catalog
            .lists
            .push((name.clone(), s.read_menu_list_count(p)));
        self.catalog.list_menus.insert(name, menus);
        Ok(())
    }

    fn capture_menu_def(&mut self, s: &ZoneStream<'_>, p: Ptr) -> ZoneResult<()> {
        let Some(name) = s.read_asset_name(p).map(str::to_owned) else {
            return Ok(());
        };
        // A leading comma names a menu another zone defines.
        if let Some(external) = name.strip_prefix(',') {
            self.list_menus.push(external.to_owned());
            return Ok(());
        }
        let read = s.read_menu_def(p);
        let window = &read.window;
        let mut def = MenuDef::default();
        def.name = name.clone();
        def.sound_name = read.sound_name.clone().unwrap_or_else(String::new);
        def.window_background = self.material_key(window.background);
        def.window_style = i32::from(window.style);
        def.window_fore_color = window.fore_color;
        def.window_back_color = window.back_color;
        def.window_border = i32::from(window.border);
        def.window_border_size = window.border_size;
        def.fullscreen = read.fullscreen;
        def.blur_world = read.blur_world;
        // Six windows; a negative id paints on the screen.
        def.ui3d_window = u8::try_from(read.ui3d_window)
            .ok()
            .filter(|&window| usize::from(window) < UI3D_WINDOWS);
        def.static_flags = window.static_flags;
        def.focus_color = Some(read.focus_color);
        def.rect = rect(window.rect);
        def.vis_exp = read.vis_exp;
        for (key, dump) in [(0u32, read.rect_x_exp), (1, read.rect_y_exp)] {
            if !dump.is_empty() {
                def.float_exp.push((key, dump));
            }
        }
        for (name, events) in event_handlers(read.handlers) {
            match name.as_str() {
                "onopen" => def.handlers.open = events,
                "onclose" => def.handlers.close = events,
                "onesc" => def.handlers.esc = events,
                _ => {}
            }
        }
        def.handlers.keys = key_handlers(read.keys);
        def.items = read.items.into_iter().map(|item| self.item(item)).collect();
        self.list_menus.push(name.clone());
        self.catalog.menus.insert(name, def);
        Ok(())
    }
}
