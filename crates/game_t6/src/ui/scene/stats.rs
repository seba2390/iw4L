//! A player's stats as the UI scripts read them (`Engine.GetPlayerStats`):
//! a state in the session's stats layout (`ddl_zm/stats.ddl` in a zombies
//! session, `ddl_mp/stats.ddl` otherwise) whose metatable
//! is `ddlParentMetatable` (members by name), `ddlArrayMetatable` (elements
//! by index, `#` the length) or `ddlLeafMetatable` (`get`, `set`,
//! `increment`). A new profile's stats are all zero.

use super::{Host, key, number, text};
use crate::ui::script::tables::Tables;
use crate::ui::script::{FaultKind, Value};

/// The data definitions the zombies and multiplayer executables keep a
/// player's stats in.
const ZOMBIES_STATS_LAYOUT: &str = "ddl_zm/stats.ddl";
const MULTIPLAYER_STATS_LAYOUT: &str = "ddl_mp/stats.ddl";

/// The stats locations `GetPlayerStats` takes (0 to 4).
const LOCATIONS: usize = 5;

/// DDL member types: the integers, an unsigned int that shows as text past
/// `i32::MAX`, a 64-bit int shown as text, the two floats, a string, and the
/// two kinds that hold a struct.
const UINT: u32 = 2;
const INT64: u32 = 4;
const FLOAT: u32 = 5;
const FIXED: u32 = 6;
const STRING: u32 = 7;
const STRUCT: u32 = 8;
const STRUCT_ALT: u32 = 9;

/// Where a state points: a bit offset, the member it names (struct and
/// member index; none for the root) and whether an array element is
/// selected.
#[derive(Clone, Copy, Debug)]
pub(super) struct DdlState {
    location: usize,
    offset: u32,
    member: Option<(usize, usize)>,
    indexed: bool,
}

#[derive(Debug, Default)]
pub(super) struct Stats {
    /// Each location's buffer, made when first read.
    buffers: [Vec<u8>; LOCATIONS],
    pub(super) states: Vec<DdlState>,
}

fn read_bits(buffer: &[u8], offset: u32, bits: u32) -> u64 {
    (0..bits.min(64)).fold(0, |value, i| {
        let at = offset + i;
        let bit = buffer
            .get((at / 8) as usize)
            .map_or(0, |byte| (byte >> (at % 8)) & 1);
        value | (u64::from(bit) << i)
    })
}

fn write_bits(buffer: &mut [u8], offset: u32, bits: u32, value: u64) {
    for i in 0..bits.min(64) {
        let at = offset + i;
        if let Some(byte) = buffer.get_mut((at / 8) as usize) {
            let mask = 1 << (at % 8);
            if value >> i & 1 != 0 {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }
}

impl Host {
    fn stats_layout(&self) -> Result<&asset_core::BitLayout, FaultKind> {
        let name = if self.front_end.session >> super::front_end::SESSION_ZOMBIES & 1 != 0 {
            ZOMBIES_STATS_LAYOUT
        } else {
            MULTIPLAYER_STATS_LAYOUT
        };
        self.input
            .layouts
            .iter()
            .find(|layout| layout.name.eq_ignore_ascii_case(name))
            .ok_or(FaultKind::Unknown(super::super::script::ENGINE_API))
    }

    /// A state's Lua object: a table naming the state, with the metatable
    /// its member's shape picks.
    fn ddl_object(&mut self, state: DdlState, tables: &mut Tables) -> Result<Value, FaultKind> {
        let layout = self.stats_layout()?;
        let shape: &'static str = match state.member {
            None => "parent",
            Some((s, m)) => {
                let member = &layout.structs[s].members[m];
                if member.array_size > 1 && !state.indexed {
                    "array"
                } else if !matches!(member.kind, STRUCT | STRUCT_ALT) {
                    "leaf"
                } else {
                    "parent"
                }
            }
        };
        let index = self.front_end.stats.states.len();
        self.front_end.stats.states.push(state);
        let object = tables.allocate()?;
        tables.raw_set(&object, &key("__ddl"), Value::Number(index as f32))?;
        let metatable = match self.front_end.ddl_metatables.get(shape) {
            Some(metatable) => metatable.clone(),
            None => {
                let metatable = tables.allocate()?;
                match shape {
                    "parent" => {
                        let index = self.binding("ddl:parent_index".into());
                        tables.raw_set(&metatable, &key("__index"), index)?;
                    }
                    "array" => {
                        let index = self.binding("ddl:array_index".into());
                        tables.raw_set(&metatable, &key("__index"), index)?;
                        let length = self.binding("ddl:array_len".into());
                        tables.raw_set(&metatable, &key("__len"), length)?;
                    }
                    _ => {
                        let methods = tables.allocate()?;
                        for method in ["set", "get", "increment"] {
                            let function = self.binding(format!("ddl:{method}"));
                            tables.raw_set(&methods, &key(method), function)?;
                        }
                        tables.raw_set(&metatable, &key("__index"), methods)?;
                    }
                }
                self.front_end
                    .ddl_metatables
                    .insert(shape.into(), metatable.clone());
                metatable
            }
        };
        tables.set_metatable(&object, Some(metatable))?;
        Ok(object)
    }

    fn ddl_state(
        &self,
        object: Option<&Value>,
        tables: &mut Tables,
    ) -> Result<DdlState, FaultKind> {
        let object = object.ok_or(FaultKind::Argument(1))?;
        let index = number(Some(&tables.raw_get(object, &key("__ddl"))?))? as usize;
        self.front_end
            .stats
            .states
            .get(index)
            .copied()
            .ok_or(FaultKind::BindingRequired)
    }

    /// `Engine.GetPlayerStats(controller[, location])`: the root of the
    /// controller's stats at a location from 0 to 4; nil (after the
    /// engine's "not fetched" message) for any other controller or
    /// location.
    pub(super) fn get_player_stats(
        &mut self,
        args: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        let controller = match args.first() {
            Some(Value::Number(value)) => *value as i32,
            _ => 0,
        };
        let location = match args.get(1) {
            Some(Value::Number(value)) => *value as i32,
            _ => 0,
        };
        if controller != 0 || !(0..LOCATIONS as i32).contains(&location) {
            return Ok(vec![Value::Nil]);
        }
        let layout = self.stats_layout()?;
        let bits = layout.bits;
        let reset_warning = layout.structs.first().and_then(|root| {
            let loadouts = root
                .members
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case("cacLoadouts"))?;
            let warning = layout
                .structs
                .get(loadouts.external.max(0) as usize)?
                .members
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case("resetWarningDisplayed"))?;
            Some((loadouts.offset + warning.offset, warning.bits))
        });
        let buffer = &mut self.front_end.stats.buffers[location as usize];
        if buffer.is_empty() {
            *buffer = vec![0; bits.div_ceil(8) as usize];
            if let Some((offset, width)) = reset_warning {
                write_bits(buffer, offset, width, 1);
            }
        }
        let state = DdlState {
            location: location as usize,
            offset: 0,
            member: None,
            indexed: false,
        };
        Ok(vec![self.ddl_object(state, tables)?])
    }

    /// The DDL objects' metamethods and leaf methods.
    pub(super) fn ddl_call(
        &mut self,
        name: &str,
        args: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        let state = self.ddl_state(args.first(), tables)?;
        let layout = self.stats_layout()?.clone();
        let layout = &layout;
        match name {
            "parent_index" => {
                let wanted = text(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                let holder = match state.member {
                    None => 0,
                    Some((s, m)) => layout.structs[s].members[m].external.max(0) as usize,
                };
                let Some(found) = layout.structs.get(holder).and_then(|s| {
                    s.members
                        .iter()
                        .position(|member| member.name.eq_ignore_ascii_case(&wanted))
                }) else {
                    return Ok(vec![Value::Nil]);
                };
                let child = DdlState {
                    location: state.location,
                    offset: state.offset + layout.structs[holder].members[found].offset,
                    member: Some((holder, found)),
                    indexed: false,
                };
                Ok(vec![self.ddl_object(child, tables)?])
            }
            "array_index" | "array_len" => {
                let (s, m) = state.member.ok_or(FaultKind::BindingRequired)?;
                let member = &layout.structs[s].members[m];
                let size = member.array_size;
                if name == "array_len" {
                    return Ok(vec![Value::Number(size as f32)]);
                }
                // An array over an enum also takes the enum member's name.
                let position = match args.get(1) {
                    Some(Value::Number(position)) => *position as i32,
                    Some(name @ Value::Bytes(_)) if member.enum_index >= 0 => {
                        let name = text(name)?;
                        let Some(found) =
                            layout
                                .enums
                                .get(member.enum_index as usize)
                                .and_then(|members| {
                                    members
                                        .iter()
                                        .position(|member| member.eq_ignore_ascii_case(&name))
                                })
                        else {
                            return Ok(vec![Value::Nil]);
                        };
                        found as i32
                    }
                    _ => return Ok(vec![Value::Nil]),
                };
                if position < 0 || position >= size {
                    return Ok(vec![Value::Nil]);
                }
                let child = DdlState {
                    offset: state.offset + member.bits / size as u32 * position as u32,
                    indexed: true,
                    ..state
                };
                Ok(vec![self.ddl_object(child, tables)?])
            }
            _ => {
                let (s, m) = state.member.ok_or(FaultKind::BindingRequired)?;
                let member = &layout.structs[s].members[m];
                let bits = if member.array_size > 1 && state.indexed {
                    member.bits / member.array_size as u32
                } else {
                    member.bits
                };
                let kind = member.kind;
                let buffer = &mut self.front_end.stats.buffers[state.location];
                match name {
                    "get" => {
                        let raw = read_bits(buffer, state.offset, bits);
                        Ok(vec![match kind {
                            UINT if raw > i32::MAX as u64 => key(&raw.to_string()),
                            INT64 => key(&raw.to_string()),
                            FLOAT | FIXED => Value::Number(f32::from_bits(raw as u32)),
                            STRING => {
                                let bytes: Vec<u8> = (0..bits / 8)
                                    .map(|i| read_bits(buffer, state.offset + i * 8, 8) as u8)
                                    .take_while(|&b| b != 0)
                                    .collect();
                                Value::Bytes(bytes.into())
                            }
                            _ => Value::Number(raw as u32 as i32 as f32),
                        }])
                    }
                    "set" | "increment" => {
                        let value = number(args.get(1));
                        let current = read_bits(buffer, state.offset, bits) as u32 as i32;
                        let written = match (kind, value) {
                            (FLOAT | FIXED, Ok(value)) => u64::from(value.to_bits()),
                            (STRING, _) => {
                                let text = text(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                                for i in 0..bits / 8 {
                                    let byte =
                                        text.as_bytes().get(i as usize).copied().unwrap_or(0);
                                    write_bits(buffer, state.offset + i * 8, 8, u64::from(byte));
                                }
                                return Ok(Vec::new());
                            }
                            (_, Ok(value)) if name == "increment" => {
                                current.wrapping_add(value as i32) as u32 as u64
                            }
                            (_, Ok(value)) => value as i64 as u64,
                            (_, Err(_)) => return Ok(Vec::new()),
                        };
                        write_bits(buffer, state.offset, bits, written);
                        Ok(Vec::new())
                    }
                    _ => Err(FaultKind::BindingRequired),
                }
            }
        }
    }
}

impl Host {
    /// `UIExpression.GetDStat(controller, path...)`: the stat a path of
    /// member names and array indices (or their enum names) leads to in the
    /// controller's stats: text for a string or a 64-bit int, else an int;
    /// 0 for a path that does not lead to one.
    pub(super) fn get_dstat(&mut self, args: &[Value]) -> Result<Vec<Value>, FaultKind> {
        let layout = self.stats_layout()?.clone();
        let mut state: Option<(usize, usize)> = None;
        let mut offset = 0u32;
        let mut indexed = false;
        for step in args.iter().skip(1) {
            let holder = match state {
                None => Some(0),
                Some((s, m)) => {
                    let member = &layout.structs[s].members[m];
                    if member.array_size > 1 && !indexed {
                        let position = match step {
                            Value::Number(position) => *position as i32,
                            other => {
                                let name = text(other)?;
                                match layout.enums.get(member.enum_index.max(0) as usize) {
                                    Some(names) if member.enum_index >= 0 => names
                                        .iter()
                                        .position(|n| n.eq_ignore_ascii_case(&name))
                                        .map_or(-1, |p| p as i32),
                                    _ => name.trim().parse().unwrap_or(-1),
                                }
                            }
                        };
                        if position < 0 || position >= member.array_size {
                            return Ok(vec![Value::Number(0.0)]);
                        }
                        offset += member.bits / member.array_size as u32 * position as u32;
                        indexed = true;
                        continue;
                    }
                    matches!(member.kind, STRUCT | STRUCT_ALT)
                        .then_some(member.external.max(0) as usize)
                }
            };
            let Some(holder) = holder else {
                return Ok(vec![Value::Number(0.0)]);
            };
            let name = text(step)?;
            let Some(found) = layout.structs.get(holder).and_then(|s| {
                s.members
                    .iter()
                    .position(|m| m.name.eq_ignore_ascii_case(&name))
            }) else {
                return Ok(vec![Value::Number(0.0)]);
            };
            offset += layout.structs[holder].members[found].offset;
            state = Some((holder, found));
            indexed = false;
        }
        let Some((s, m)) = state else {
            return Ok(vec![Value::Number(0.0)]);
        };
        let member = &layout.structs[s].members[m];
        if (member.array_size > 1 && !indexed) || matches!(member.kind, STRUCT | STRUCT_ALT) {
            return Ok(vec![Value::Number(0.0)]);
        }
        let bits = if member.array_size > 1 {
            member.bits / member.array_size as u32
        } else {
            member.bits
        };
        if self.front_end.stats.buffers[0].is_empty() {
            self.front_end.stats.buffers[0] = vec![0; layout.bits.div_ceil(8) as usize];
        }
        let buffer = &self.front_end.stats.buffers[0];
        let raw = read_bits(buffer, offset, bits);
        Ok(vec![match member.kind {
            STRING => {
                let bytes: Vec<u8> = (0..bits / 8)
                    .map(|i| read_bits(buffer, offset + i * 8, 8) as u8)
                    .take_while(|&b| b != 0)
                    .collect();
                Value::Bytes(bytes.into())
            }
            INT64 => key(&(raw as i64).to_string()),
            _ => Value::Number(raw as u32 as i32 as f32),
        }])
    }
}
