//! Black Ops multiplayer's stats: each client's two stats buffers, laid out
//! by `ddl_mp/stats.ddl` and read and written by `getdstat` and `setdstat`.
//!
//! The first buffer is the one the client's own stats fill, the second the
//! one the server keeps. A member only the server writes (permission 2)
//! reads from the server's buffer, every other from the client's; every
//! write goes to the server's. In Combat Training (`xblive_basictraining`)
//! both read and write the client's.

use crate::script::Namespace::Method;
use crate::script::{MatchScript, NativeRegistry, RoundScript, Value};
use asset_core::{BitLayout, BitMember};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const LAYOUT: &str = "ddl_mp/stats.ddl";
/// Each buffer's size; it starts with a header of `HEADER_BITS`.
pub(super) const BUFFER_BYTES: usize = 0xa000;
const HEADER_BITS: i64 = 0x140;
/// The header's version word.
const VERSION_AT: usize = 4;
/// The header's byte whose low bit a changed value sets.
const CHANGED_AT: usize = 8;
const MAX_PATH: usize = 8;
/// A member only the server writes.
const SERVER_PERMISSION: u32 = 2;
const STRUCT_KINDS: [u32; 2] = [6, 7];

/// The two buffers, by their index in `ClientStats::buffers`.
const CLIENT: usize = 0;
const SERVER: usize = 1;

#[derive(Resource, Default)]
pub(crate) struct PlayerStats(BTreeMap<u32, ClientStats>);

pub(crate) struct ClientStats {
    /// The client's stats have arrived; until then integers read 0 and
    /// nothing is written.
    received: bool,
    /// A buffer was checked against the layout's version (once a client,
    /// whichever buffer was first read).
    validated: bool,
    buffers: [Vec<u8>; 2],
    /// One bit for each byte of a value written, and whether any is.
    changed_bytes: Vec<u8>,
    changed: bool,
}

impl Default for ClientStats {
    fn default() -> Self {
        Self {
            received: false,
            validated: false,
            buffers: [vec![0; BUFFER_BYTES], vec![0; BUFFER_BYTES]],
            changed_bytes: vec![0; BUFFER_BYTES / 8],
            changed: false,
        }
    }
}

/// Where a path has reached: a bit offset, the array index taken (-1 for
/// none) and the member, by struct and position in it.
#[derive(Clone, Copy)]
pub(super) struct Cursor {
    pub(super) offset: i64,
    pub(super) index: i32,
    pub(super) member: Option<(usize, usize)>,
}

pub(super) const ROOT: Cursor = Cursor {
    offset: 0,
    index: -1,
    member: None,
};

pub(super) fn member(layout: &BitLayout, at: Option<(usize, usize)>) -> Option<&BitMember> {
    let (s, m) = at?;
    layout.structs.get(s)?.members.get(m)
}

/// An element's size in bits: an array member's size is all of it.
pub(super) fn element_bits(member: &BitMember) -> i64 {
    if member.array_size == 0 {
        return 0;
    }
    i64::from(member.bits as i32 / member.array_size)
}

/// The position of `name` among `names`, which are sorted, by the game's
/// binary search over the name lowered.
fn search<'a>(names: impl Fn(usize) -> &'a str, count: usize, name: &str) -> Option<usize> {
    let name: String = name.chars().take(255).collect::<String>().to_lowercase();
    let (mut low, mut high) = (0i64, count as i64 - 1);
    while low <= high {
        let mid = (low + high) / 2;
        match names(mid as usize).as_bytes().cmp(name.as_bytes()) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid - 1,
            std::cmp::Ordering::Equal => return Some(mid as usize),
        }
    }
    None
}

/// The member of struct `s` named `name`.
fn find_member(layout: &BitLayout, s: usize, name: &str) -> Option<(usize, usize)> {
    let members = &layout.structs.get(s)?.members;
    let found = search(|i| &members[i].name, members.len(), name);
    if found.is_none() {
        diag::warn!(
            Sim,
            "DDL: Could not find '{name}' in '{}'.",
            layout.structs[s].name
        );
    }
    found.map(|m| (s, m))
}

/// `at` indexed by `index`, unchecked.
fn index(layout: &BitLayout, at: Cursor, index: i32) -> Cursor {
    let size = member(layout, at.member).map_or(0, element_bits);
    Cursor {
        offset: size * i64::from(index) + at.offset,
        index,
        member: at.member,
    }
}

/// One step of a path by a name: a root member, an array index (by number
/// or by the array's enum) or a struct's member.
pub(super) fn step_name(layout: &BitLayout, at: Cursor, name: &str) -> Option<Cursor> {
    let Some(current) = member(layout, at.member) else {
        let (s, m) = find_member(layout, 0, name)?;
        return Some(Cursor {
            offset: i64::from(layout.structs[s].members[m].offset),
            index: -1,
            member: Some((s, m)),
        });
    };
    if current.array_size > 1 && at.index == -1 {
        if current.enum_index < 0 {
            let n = super::iw4::atoi(name);
            if n >= current.array_size {
                diag::warn!(Sim, "DDL: Array index overflow.");
                return None;
            }
            return Some(index(layout, at, n));
        }
        let names = layout.enums.get(current.enum_index as usize)?;
        let Some(n) = search(|i| &names[i], names.len(), name) else {
            diag::warn!(Sim, "DDL: Enum Index '{name}' not found.");
            return None;
        };
        return Some(index(layout, at, n as i32));
    }
    if STRUCT_KINDS.contains(&current.kind) {
        let (s, m) = find_member(layout, current.external.max(0) as usize, name)?;
        return Some(Cursor {
            offset: i64::from(layout.structs[s].members[m].offset) + at.offset,
            index: -1,
            member: Some((s, m)),
        });
    }
    diag::warn!(
        Sim,
        "DDL: Trying to move into a leaf member. '{}' expected to be a struct.",
        current.name
    );
    None
}

/// The path `keys` walked from the root as `getdstat` and `setdstat` walk
/// it; `builtin` names the builtin in the errors.
fn walk(layout: &BitLayout, keys: &[Value], builtin: &str) -> Result<Cursor, String> {
    let mut at = ROOT;
    for key in keys {
        let current = member(layout, at.member);
        match key {
            Value::String(name) => {
                if current.is_some_and(|m| m.array_size > 1 && m.enum_index == -1) && at.index == -1
                {
                    return Err(format!(
                        "{builtin}: array index (integer) expected. Received a string instead."
                    ));
                }
                at = step_name(layout, at, name)
                    .ok_or_else(|| format!("{builtin}: Could not find member name {name}."))?;
            }
            Value::Int(n) => {
                let Some(current) = current else {
                    return Err(format!(
                        "{builtin}: an integer before any member name; Black Ops reads no member here"
                    ));
                };
                if current.array_size == 1 {
                    return Err(format!(
                        "{builtin}: member name (string) expected. Received an integer instead."
                    ));
                }
                at = index(layout, at, *n);
            }
            _ => return Err(format!("{builtin}: Expected strings or integers only.")),
        }
    }
    Ok(at)
}

fn read_bits(buffer: &[u8], offset: i64, bits: i64) -> u32 {
    let mut value = 0u64;
    for i in 0..bits {
        let at = offset + HEADER_BITS + i;
        let byte = buffer.get((at >> 3) as usize).copied().unwrap_or(0);
        value |= u64::from((byte >> (at & 7)) & 1) << i;
    }
    value as u32
}

fn write_bits(buffer: &mut [u8], offset: i64, bits: i64, value: u32) {
    for i in 0..bits {
        let at = offset + HEADER_BITS + i;
        if let Some(byte) = buffer.get_mut((at >> 3) as usize) {
            let bit = 1 << (at & 7);
            if (value >> i) & 1 != 0 {
                *byte |= bit;
            } else {
                *byte &= !bit;
            }
        }
    }
}

/// A leaf the path reached inside the layout, or why not.
pub(super) fn leaf(layout: &BitLayout, at: Cursor) -> Option<&BitMember> {
    let Some(m) = member(layout, at.member) else {
        diag::warn!(Sim, "DDL: Member not found in path.");
        return None;
    };
    if STRUCT_KINDS.contains(&m.kind) {
        diag::warn!(
            Sim,
            "DDL: Attempt to read or write a value to a non-leaf member."
        );
        return None;
    }
    if i64::from(layout.bits) <= at.offset {
        diag::warn!(
            Sim,
            "DDL: Buffer offset is out of the DDL buffer limit for member {}.",
            m.name
        );
        return None;
    }
    Some(m)
}

/// The offset and size of a value of `bits` at `at`, when both are ones
/// the game reads.
fn span(at: Cursor, bits: i64) -> Option<(i64, i64)> {
    if at.offset < 0 {
        diag::warn!(Sim, "DDL: Negative offset {}.", at.offset);
        return None;
    }
    if !(1..=32).contains(&bits) {
        diag::warn!(Sim, "DDL: size error 0 < {bits} <= 32.");
        return None;
    }
    Some((at.offset, bits))
}

pub(super) fn read_int(m: &BitMember, at: Cursor, buffer: &[u8]) -> Option<u32> {
    let (offset, bits) = span(at, element_bits(m))?;
    Some(m.min.wrapping_add(read_bits(buffer, offset, bits)))
}

fn read_u64(m: &BitMember, at: Cursor, buffer: &[u8]) -> Option<u64> {
    let (offset, half) = span(at, element_bits(m) / 2)?;
    let low = read_bits(buffer, offset, half);
    let high = read_bits(buffer, offset + 32, half);
    Some(u64::from(high) << 32 | u64::from(low))
}

/// A string member's bytes up to the first zero; it reads all of the
/// member's bits, at most 64 bytes.
fn read_string(m: &BitMember, at: Cursor, buffer: &[u8]) -> String {
    if at.offset < 0 {
        diag::warn!(Sim, "DDL: Negative offset {}.", at.offset);
        return String::new();
    }
    let bytes: Vec<u8> = (0..i64::from(m.bits as i32 / 8).min(64))
        .map(|i| read_bits(buffer, at.offset + i * 8, 8) as u8)
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn basic_training(world: &World) -> bool {
    world
        .resource::<MatchScript>()
        .dvars
        .get("xblive_basictraining")
        .is_some_and(|v| super::iw4::atoi(v) != 0)
}

fn layout(world: &World) -> Option<std::sync::Arc<BitLayout>> {
    world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.layouts.get(LAYOUT).cloned())
}

/// A client's stats. A test client's arrive at once, a zeroed client
/// buffer. The local profile's arrive when its client first reads or
/// writes one, reset first when the profile has none or keeps an older
/// stats version; any other client's never arrive (nothing sends them).
fn client_stats(world: &mut World, client: u32) -> &mut ClientStats {
    let received = world
        .get_resource::<PlayerStats>()
        .and_then(|stats| stats.0.get(&client))
        .is_some_and(|stats| stats.received);
    let profile = (!received)
        .then(|| world.get_resource::<crate::ProfileStats>())
        .flatten()
        .filter(|profile| profile.client == Some(crate::ClientId(client)))
        .map(|profile| profile.bytes.clone());
    let profile = match (profile, layout(world)) {
        (Some(bytes), Some(layout)) => {
            if super::t5_stats_reset::needs_reset(world, &layout, bytes.as_deref()) {
                let bytes = super::t5_stats_reset::reset(world, &layout);
                let mut stored = world.resource_mut::<crate::ProfileStats>();
                stored.bytes = Some(bytes.clone());
                stored.revision += 1;
                Some(bytes)
            } else {
                bytes
            }
        }
        _ => None,
    };
    let test_client = world
        .get_resource::<crate::TestClients>()
        .is_some_and(|clients| clients.0.contains(&client));
    let stats = world
        .get_resource_or_insert_with(PlayerStats::default)
        .into_inner()
        .0
        .entry(client)
        .or_default();
    if let Some(bytes) = profile {
        let (own, server) = bytes.split_at(BUFFER_BYTES.min(bytes.len()));
        stats.buffers[CLIENT][..own.len()].copy_from_slice(own);
        stats.buffers[SERVER][..server.len().min(BUFFER_BYTES)]
            .copy_from_slice(&server[..server.len().min(BUFFER_BYTES)]);
        stats.received = true;
    } else if test_client && !stats.received {
        stats.buffers[CLIENT].fill(0);
        stats.received = true;
    }
    stats
}

/// Checks `buffer` against the layout's version: a buffer with none is
/// cleared and takes it.
pub(super) fn validate(layout: &BitLayout, buffer: &mut [u8]) -> bool {
    let need = i64::from(layout.bits) + HEADER_BITS;
    if (buffer.len() as i64) * 8 < need {
        diag::warn!(
            Sim,
            "DDL: Buffer validation error - buffer is not large enough to accommodate this DDL. {} bytes required.",
            (need + 7) / 8
        );
        return false;
    }
    let header_bytes = (HEADER_BITS / 8) as usize;
    let mut header = buffer[..header_bytes].to_vec();
    let version = u32::from_le_bytes(
        header[VERSION_AT..VERSION_AT + 4]
            .try_into()
            .unwrap_or([0; 4]),
    );
    if version == 0 {
        diag::info!(
            Sim,
            "DDL: Buffer was not associated with any version. Associating it with version {}.",
            layout.version
        );
        buffer.fill(0);
        header[VERSION_AT..VERSION_AT + 4].copy_from_slice(&layout.version.to_le_bytes());
        buffer[..header_bytes].copy_from_slice(&header);
    } else if version != layout.version {
        diag::warn!(
            Sim,
            "DDL: Buffer validation error - buffer is already associated with a different DDL version."
        );
        return false;
    }
    true
}

/// The buffer a read of `m` takes, checked once a client.
fn read_buffer<'a>(
    stats: &'a mut ClientStats,
    layout: &BitLayout,
    m: Option<&BitMember>,
    basic_training: bool,
) -> Option<&'a [u8]> {
    let which = if !basic_training && m.is_some_and(|m| m.permission == SERVER_PERMISSION) {
        SERVER
    } else {
        CLIENT
    };
    if !stats.validated {
        if !validate(layout, &mut stats.buffers[which]) {
            diag::warn!(Sim, "DDL: Could not get stat. Buffer error.");
            return None;
        }
        stats.validated = true;
    }
    Some(&stats.buffers[which])
}

/// The buffer writes take, once the client's stats are in; checked once a
/// client.
fn write_buffer<'a>(
    stats: &'a mut ClientStats,
    layout: &BitLayout,
    basic_training: bool,
) -> Option<&'a mut Vec<u8>> {
    if !stats.received {
        return None;
    }
    let which = if basic_training { CLIENT } else { SERVER };
    if !stats.validated {
        if !validate(layout, &mut stats.buffers[which]) {
            diag::warn!(Sim, "DDL: Could not get stat. Buffer error.");
            return None;
        }
        stats.validated = true;
    }
    Some(&mut stats.buffers[which])
}

/// Marks the bytes of the value at `at` changed, whether or not the write
/// took.
fn mark_changed(stats: &mut ClientStats, m: Option<&BitMember>, at: Cursor) {
    let size = m.map_or(0, element_bits);
    let first = at.offset >> 3;
    let end = (at.offset + size + 7) >> 3;
    for byte in first.max(0)..end {
        if let Some(bits) = stats.changed_bytes.get_mut((byte >> 3) as usize) {
            *bits |= 1 << (byte & 7);
        }
    }
    stats.changed = true;
}

/// Writes `bits` of `value` at `offset` of `buffer`; whether it changed.
fn store(buffer: &mut [u8], offset: i64, bits: i64, value: u32) -> bool {
    if read_bits(buffer, offset, bits) == value {
        return false;
    }
    write_bits(buffer, offset, bits, value);
    buffer[CHANGED_AT] |= 1;
    true
}

/// An integer written into `buffer` as the client's and the server's set
/// write it: within the member's range, stored less its minimum.
pub(super) fn set_raw_int(layout: &BitLayout, buffer: &mut [u8], at: Cursor, value: u32) -> bool {
    leaf(layout, at).is_some_and(|m| {
        if !(m.min..=m.max).contains(&value) {
            diag::warn!(
                Sim,
                "DDL: Trying to set a value of {} when the valid range is {} to {}",
                value as i32,
                m.min as i32,
                m.max as i32
            );
            return false;
        }
        span(at, element_bits(m))
            .map(|(offset, bits)| store(buffer, offset, bits, value - m.min))
            .is_some()
    })
}

/// A 64-bit value written as two 32-bit halves.
pub(super) fn set_raw_u64(layout: &BitLayout, buffer: &mut [u8], at: Cursor, value: u64) -> bool {
    leaf(layout, at).is_some_and(|m| {
        span(at, element_bits(m) / 2).is_some_and(|(offset, half)| {
            store(buffer, offset, half, value as u32);
            store(buffer, offset + 32, half, (value >> 32) as u32);
            true
        })
    })
}

/// The string's bytes, cut to the member's size, with no zero after them:
/// a shorter string leaves the rest of an earlier one.
pub(super) fn set_raw_string(
    layout: &BitLayout,
    buffer: &mut [u8],
    at: Cursor,
    value: &str,
) -> bool {
    leaf(layout, at).is_some_and(|m| {
        let room = (element_bits(m) / 8) as usize;
        let mut bytes = value.as_bytes();
        if bytes.len() > room {
            diag::warn!(
                Sim,
                "DDL: '{value}' too long. Can be up to {room} chars but was {} chars. Truncating.",
                bytes.len()
            );
            bytes = &bytes[..room];
        }
        if at.offset < 0 {
            diag::warn!(Sim, "DDL: Negative offset {}.", at.offset);
            return false;
        }
        for (i, &b) in bytes.iter().enumerate() {
            write_bits(buffer, at.offset + i as i64 * 8, 8, u32::from(b));
        }
        buffer[CHANGED_AT] |= 1;
        true
    })
}

fn set_int(layout: &BitLayout, stats: &mut ClientStats, at: Cursor, value: u32, training: bool) {
    let m = member(layout, at.member);
    let Some(buffer) = write_buffer(stats, layout, training) else {
        return;
    };
    let current = leaf(layout, at).and_then(|m| read_int(m, at, buffer));
    if current == Some(value) {
        return;
    }
    if !set_raw_int(layout, buffer, at, value) {
        diag::warn!(Sim, "DDL: Could not set stat. DDL set error.");
    }
    mark_changed(stats, m, at);
}

fn set_u64(layout: &BitLayout, stats: &mut ClientStats, at: Cursor, value: u64, training: bool) {
    let m = member(layout, at.member);
    let Some(buffer) = write_buffer(stats, layout, training) else {
        return;
    };
    let current = leaf(layout, at).and_then(|m| read_u64(m, at, buffer));
    if current == Some(value) {
        return;
    }
    if !set_raw_u64(layout, buffer, at, value) {
        diag::warn!(Sim, "DDL: Could not set stat. DDL set error.");
    }
    mark_changed(stats, m, at);
}

fn set_string(
    layout: &BitLayout,
    stats: &mut ClientStats,
    at: Cursor,
    value: &str,
    training: bool,
) {
    let m = member(layout, at.member);
    let Some(buffer) = write_buffer(stats, layout, training) else {
        return;
    };
    let current = leaf(layout, at).map(|m| read_string(m, at, buffer));
    if current.as_deref() == Some(value) {
        return;
    }
    if !set_raw_string(layout, buffer, at, value) {
        diag::warn!(Sim, "DDL: Could not set stat. DDL set error.");
    }
    mark_changed(stats, m, at);
}

/// A player entity's client number, or the builtin's error.
fn player(world: &World, receiver: &Value, builtin: &str) -> Result<u32, String> {
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    world
        .resource::<RoundScript>()
        .player_client(id)
        .ok_or_else(|| format!("{builtin}: entity must be a player entity"))
}

fn stats_layout(world: &World) -> Result<std::sync::Arc<BitLayout>, String> {
    layout(world).ok_or_else(|| "the match has no ddl_mp/stats.ddl (code_post_gfx_mp)".into())
}

fn getdstat(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let client = player(world, receiver, "getdstat")?;
    if args.len() > MAX_PATH {
        return Err("getdstat: path is too deep.".into());
    }
    let layout = stats_layout(world)?;
    let at = walk(&layout, args, "getdstat")?;
    let Some(m) = member(&layout, at.member) else {
        return Err("getdstat: could not find ddl member.".into());
    };
    let training = basic_training(world);
    let stats = client_stats(world, client);
    match m.kind {
        0..=2 => {
            if !stats.received {
                return Ok(Value::Int(0));
            }
            let value = read_buffer(stats, &layout, Some(m), training)
                .and_then(|buffer| read_int(leaf(&layout, at)?, at, buffer));
            Ok(Value::Int(value.unwrap_or(0) as i32))
        }
        3 => {
            let value = read_buffer(stats, &layout, Some(m), training)
                .and_then(|buffer| read_u64(leaf(&layout, at)?, at, buffer));
            Ok(Value::String(value.unwrap_or(0).to_string().into()))
        }
        5 => {
            let value = read_buffer(stats, &layout, Some(m), training)
                .and_then(|buffer| Some(read_string(leaf(&layout, at)?, at, buffer)));
            Ok(Value::String(value.unwrap_or_else(String::new).into()))
        }
        _ => Err("getdstat: stat type undefined".into()),
    }
}

/// `_atoi64`: optional blanks and sign, then digits.
pub(super) fn atoi64(text: &str) -> u64 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |n, d| {
            n.wrapping_mul(10).wrapping_add(i64::from(d - b'0'))
        });
    (if negative {
        value.wrapping_neg()
    } else {
        value
    }) as u64
}

fn setdstat(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let mod_running = world
        .resource::<MatchScript>()
        .dvars
        .get("fs_game")
        .is_some_and(|v| !v.is_empty());
    if mod_running {
        return Ok(Value::Undefined);
    }
    let client = player(world, receiver, "setdstat")?;
    if args.len() > MAX_PATH {
        return Err("setdstat: path should be at most 8 element deep".into());
    }
    let (value, keys) = args
        .split_last()
        .ok_or("setdstat: Only string or integer values are acceptable.")?;
    let layout = stats_layout(world)?;
    let at = walk(&layout, keys, "setdstat")?;
    let m = member(&layout, at.member);
    if m.is_some_and(|m| m.array_size > 1) && at.index == -1 {
        return Err("setdstat: trying to set a non leaf member of the ddl".into());
    }
    let training = basic_training(world);
    match (value, m.map(|m| m.kind)) {
        (Value::Int(n), Some(0..=2)) => {
            set_int(
                &layout,
                client_stats(world, client),
                at,
                *n as u32,
                training,
            );
        }
        (Value::Int(_), _) => {
            return Err(
                "setdstat: member expects a string or 64 bit integers. Pass the value in quotes."
                    .into(),
            );
        }
        (Value::String(text), Some(3)) => {
            set_u64(
                &layout,
                client_stats(world, client),
                at,
                atoi64(text),
                training,
            );
        }
        (Value::String(text), Some(5)) => {
            set_string(&layout, client_stats(world, client), at, text, training);
        }
        (Value::String(_), _) => {
            return Err("setdstat: member expects a string or 64 bit integer only. For regular integers, don't use quotes.".into());
        }
        _ => {
            return Err("setdstat: Only string or integer values are acceptable.".into());
        }
    }
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "getdstat", getdstat);
    registry.register(Method, "setdstat", setdstat);
}
