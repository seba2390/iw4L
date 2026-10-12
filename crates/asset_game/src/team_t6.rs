use std::collections::BTreeMap;

pub fn t6_team_properties(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    if bytes.get(..8)? != b"\x80GSC\r\n\0\x06" {
        return None;
    }
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let mut at = word(24)? as usize;
    let mut uses = Vec::new();
    for _ in 0..half(50)? {
        let offset = half(at)? as usize;
        let count = *bytes.get(at + 2)?;
        let kind = *bytes.get(at + 3)?;
        let tail = bytes.get(offset..)?;
        let value = std::str::from_utf8(tail.get(..tail.iter().position(|&b| b == 0)?)?).ok()?;
        at += 4;
        for _ in 0..count {
            let address = word(at)? as usize;
            at += 4;
            if kind == 0 {
                uses.push((address, value));
            }
        }
    }
    uses.sort_unstable_by_key(|u| u.0);
    let mut values = BTreeMap::new();
    for pair in uses.windows(2) {
        let [(value_at, value), (key_at, key)] = pair else {
            unreachable!()
        };
        if key_at - value_at > 12 {
            continue;
        }
        let tail = bytes.get(key_at + 2..)?;
        if tail.starts_with(&[0x14, 0x1c, 0x1c, 0x28])
            || (key.starts_with("g_") && tail.starts_with(&[0x19, 0, 0x4e, 0x2e]))
        {
            values.insert((*key).to_owned(), (*value).to_owned());
        }
    }
    for triple in uses.windows(3) {
        let [(value_at, value), (_, key), (category_at, category)] = triple else {
            unreachable!()
        };
        if category_at - value_at <= 20
            && matches!(*category, "strings" | "music")
            && bytes
                .get(category_at + 2..)?
                .starts_with(&[0x14, 0x1c, 0x1c, 0x28])
        {
            values.insert(format!("{category}/{key}"), (*value).to_owned());
        }
    }
    for &(address, key) in &uses {
        if matches!(key, "attackers" | "defenders")
            && bytes.get(address + 2..)?.starts_with(&[0x14, 0x1c, 0x28])
        {
            values.insert(key.to_owned(), String::new());
        }
    }
    Some(values)
}

/// Addresses in a compiled script, each with the text found there.
type Sites<'a> = Vec<(usize, &'a str)>;
/// Call sites in a compiled script: address, function and its namespace.
type Calls<'a> = Vec<(usize, &'a str, &'a str)>;

/// A compiled script's string literals by the address that uses each, and
/// the calls `wanted` picks by the address that makes each, both in address
/// order.
fn literals_and_calls<'a>(
    bytes: &'a [u8],
    wanted: impl Fn(&str, &str) -> bool,
) -> Option<(Sites<'a>, Calls<'a>)> {
    if bytes.get(..8)? != b"\x80GSC\r\n\0\x06" {
        return None;
    }
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let text = |at: usize| {
        let tail = bytes.get(at..)?;
        std::str::from_utf8(tail.get(..tail.iter().position(|&b| b == 0)?)?).ok()
    };
    let mut literals = Vec::new();
    let mut at = word(0x18)? as usize;
    for _ in 0..half(0x32)? {
        let value = text(half(at)? as usize)?;
        let count = *bytes.get(at + 2)?;
        at += 4;
        for _ in 0..count {
            literals.push((word(at)? as usize, value));
            at += 4;
        }
    }
    let mut calls = Vec::new();
    let mut at = word(0x20)? as usize;
    for _ in 0..half(0x36)? {
        let name = text(half(at)? as usize)?;
        let namespace = text(half(at + 2)? as usize)?;
        let count = half(at + 4)? as usize;
        at += 8;
        for _ in 0..count {
            if wanted(name, namespace) {
                calls.push((word(at)? as usize, name, namespace));
            }
            at += 4;
        }
    }
    literals.sort_unstable_by_key(|literal| literal.0);
    calls.sort_unstable_by_key(|call| call.0);
    Some((literals, calls))
}

/// Where a character's body comes from: a model a script sets, or the
/// character script (`character/<name>`) whose `main` sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum T6Body {
    Model(String),
    Character(String),
}

const CHARACTERS: &str = "character/";

/// The bodies and view hands a zombies map's script dresses its players in:
/// each `setmodel` literal, or call of a character script's `main`, with
/// the `setviewmodel` literal that follows it.
pub fn t6_player_characters(bytes: &[u8]) -> Option<Vec<(T6Body, String)>> {
    let (literals, calls) = literals_and_calls(bytes, |name, namespace| {
        matches!(name, "setmodel" | "setviewmodel")
            || name == "main" && namespace.replace('\\', "/").starts_with(CHARACTERS)
    })?;
    let before = |site: usize| {
        literals
            .iter()
            .rev()
            .find(|(at, _)| *at < site)
            .map(|(_, value)| (*value).to_owned())
    };
    let mut characters: Vec<(T6Body, String)> = Vec::new();
    let mut body = None;
    for (site, name, namespace) in calls {
        match name {
            "setmodel" => body = before(site).map(T6Body::Model),
            "main" => body = Some(T6Body::Character(namespace.replace('\\', "/"))),
            _ => {
                if let (Some(body), Some(hands)) = (body.take(), before(site))
                    && !characters.contains(&(body.clone(), hands.clone()))
                {
                    characters.push((body, hands));
                }
            }
        }
    }
    Some(characters)
}

/// The model a character script's `main` sets.
pub fn t6_character_model(bytes: &[u8]) -> Option<String> {
    let (literals, calls) = literals_and_calls(bytes, |name, _| name == "setmodel")?;
    let (site, _, _) = calls.first()?;
    literals
        .iter()
        .rev()
        .find(|(at, _)| at < site)
        .map(|(_, value)| (*value).to_owned())
}
