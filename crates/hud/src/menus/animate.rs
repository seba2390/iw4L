//! Black Ops' item animations (`ui_animate menu item state ms`): an item
//! moves from the look it has to one of its animation states over the
//! given time, scaled by `ui_animSpeedScale`, and runs that state's
//! `onEnter` once it arrives. Also the console line's own `if` and its
//! `( … )` value groups, which the menus' `execNow` lines lean on.

use asset_game::MenuRect;

use super::runtime::Runner;
use super::state::ItemAnim;

/// `ui_animSpeedScale`'s registered value.
const DEFAULT_SPEED_SCALE: f32 = 1.0;

impl Runner<'_, '_> {
    /// `ui_animate menu item state ms`: every item of the open menu `menu`
    /// named `item` (`*`: all of them) that has a state named `state` starts
    /// towards it, and is stepped once at once.
    pub(crate) fn animate(&mut self, menu: &str, item: &str, state: &str, ms: i32) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            diag::warn!(Ui, "ui_animate - menu \"{menu}\" is not loaded");
            return;
        };
        let speed = self
            .dvar("ui_animSpeedScale")
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|speed| *speed != 0.0)
            .unwrap_or(DEFAULT_SPEED_SCALE);
        let duration_ms = (ms as f32 / speed) as i32;
        let now = self.world.ms;
        let Some(open) = self.menus.get_mut(menu) else {
            diag::debug!(Ui, "ui_animate - menu \"{menu}\" is not open");
            return;
        };
        let (mut named, mut moved) = (false, false);
        for (index, definition) in def.items.iter().enumerate() {
            if item != "*" && !definition.name.eq_ignore_ascii_case(item) {
                continue;
            }
            named = true;
            let Some(to) = definition
                .anim_states
                .iter()
                .position(|candidate| candidate.name.eq_ignore_ascii_case(state))
            else {
                continue;
            };
            moved = true;
            if open.items.len() <= index {
                open.items.resize_with(index + 1, Default::default);
            }
            let current = &mut open.items[index];
            current.anim = Some(ItemAnim {
                from_rect: current.rect.unwrap_or(definition.rect),
                from_fore: current.fore.unwrap_or(definition.fore_color),
                from_back: current.back.unwrap_or(definition.back_color),
                to,
                start_ms: now,
                duration_ms,
            });
        }
        if !named {
            diag::warn!(
                Ui,
                "ui_animate - menu \"{menu}\" does not contain an item named \"{item}\""
            );
        } else if !moved {
            diag::warn!(
                Ui,
                "ui_animate - item \"{item}\" in menu \"{menu}\" does not contain a state named \"{state}\""
            );
        }
        self.step_animations();
    }

    /// Moves every animating item of the open menus along; an item that has
    /// arrived runs its state's `onEnter`.
    pub(crate) fn step_animations(&mut self) {
        let catalog = self.catalog;
        let now = self.world.ms;
        let mut arrived = Vec::new();
        for open in &mut self.menus.stack {
            let Some(def) = catalog.get(&open.name) else {
                continue;
            };
            for (index, current) in open.items.iter_mut().enumerate() {
                let Some(anim) = current.anim.clone() else {
                    continue;
                };
                let (Some(item), Some(target)) = (
                    def.items.get(index),
                    def.items
                        .get(index)
                        .and_then(|item| item.anim_states.get(anim.to)),
                ) else {
                    current.anim = None;
                    continue;
                };
                let elapsed = now.wrapping_sub(anim.start_ms);
                let elapsed = if elapsed < 0 {
                    anim.duration_ms
                } else {
                    elapsed
                };
                let t = if anim.duration_ms == 0 {
                    1.0
                } else {
                    (elapsed as f32 / anim.duration_ms as f32).clamp(0.0, 1.0)
                };
                let lerp = |from: f32, to: f32| to * t + from * (1.0 - t);
                current.rect = Some(MenuRect {
                    x: lerp(anim.from_rect.x, target.rect.x),
                    y: lerp(anim.from_rect.y, target.rect.y),
                    w: lerp(anim.from_rect.w, target.rect.w),
                    h: lerp(anim.from_rect.h, target.rect.h),
                    ..item.rect
                });
                let colour = |from: [f32; 4], to: [f32; 4]| {
                    std::array::from_fn(|c| lerp(from[c], to[c]).clamp(0.0, 1.0))
                };
                current.fore = Some(colour(anim.from_fore, target.fore_color));
                current.back = Some(colour(anim.from_back, target.back_color));
                if elapsed >= anim.duration_ms {
                    current.anim = None;
                    arrived.push((open.name.clone(), index, target.on_enter.clone()));
                }
            }
        }
        for (menu, index, on_enter) in arrived {
            self.run_events(&menu, Some(index), &on_enter);
        }
    }
}

/// A console line's words as Black Ops' tokenizer gives them: a quoted
/// string is one word, and a `( … )` group that starts a word is one word,
/// the value `group` gives its text (parentheses included; `None`: the
/// group has no value and the line does not run).
pub(crate) fn console_words(
    line: &str,
    group: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<Vec<String>> {
    let chars: Vec<char> = line.chars().collect();
    let mut words = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            let end = chars[i + 1..]
                .iter()
                .position(|&c| c == '"')
                .map_or(chars.len(), |at| i + 1 + at);
            words.push(chars[i + 1..end].iter().collect());
            i = end + 1;
        } else if c == '(' {
            let end = group_end(&chars, i);
            let text: String = chars[i..(end + 1).min(chars.len())].iter().collect();
            words.push(group(&text)?);
            i = end + 1;
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            words.push(chars[start..i].iter().collect());
        }
    }
    Some(words)
}

/// Where the group opened at `open` closes: its matching `)`, a `)` inside
/// a quoted string not counting (the end of the line when none closes it).
fn group_end(chars: &[char], open: usize) -> usize {
    let mut depth = 0;
    let mut quoted = false;
    let mut at = open;
    while at < chars.len() {
        match chars[at] {
            '\\' if quoted => at += 1,
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
        at += 1;
    }
    chars.len()
}

/// Whether a console line has a `( … )` group outside quotes that starts a
/// word.
pub(crate) fn has_group(line: &str) -> bool {
    let mut quoted = false;
    let mut word_start = true;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted && word_start => return true,
            _ => {}
        }
        word_start = c.is_whitespace();
    }
    false
}

/// Words back into a console line, each that is empty or holds a blank in
/// quotes.
pub(crate) fn console_line(words: &[String]) -> String {
    words
        .iter()
        .map(|word| {
            if word.is_empty() || word.contains(char::is_whitespace) {
                format!("\"{word}\"")
            } else {
                word.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A constant group's value as Black Ops prints it, for a game without its
/// own expression compiler: `None` when it calls a function or names
/// anything.
pub(crate) fn constant_group(group: &str) -> Option<String> {
    let inner = group.strip_prefix('(')?;
    constant(inner.strip_suffix(')').unwrap_or(inner))
}

#[derive(Clone)]
enum Constant {
    Number(f64),
    Text(String),
}

impl Constant {
    fn number(&self) -> f64 {
        match self {
            Constant::Number(n) => *n,
            Constant::Text(s) => s.trim().parse().unwrap_or(0.0),
        }
    }

    fn word(&self) -> String {
        match self {
            Constant::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
            Constant::Number(n) => format!("{n}"),
            Constant::Text(s) => s.clone(),
        }
    }
}

/// A constant expression's value as a word; `None` when it calls a
/// function or names anything.
fn constant(text: &str) -> Option<String> {
    let tokens = constant_tokens(text)?;
    let mut at = 0;
    let value = binary(&tokens, &mut at, 0)?;
    (at == tokens.len()).then(|| value.word())
}

fn constant_tokens(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        } else if c == '"' {
            let end = chars[i + 1..]
                .iter()
                .position(|&c| c == '"')
                .map(|at| i + 1 + at)?;
            tokens.push(chars[i..=end].iter().collect());
            i = end + 1;
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if matches!(two.as_str(), "==" | "!=" | "<=" | ">=" | "&&" | "||") {
                tokens.push(two);
                i += 2;
            } else if "+-*/%<>!()".contains(c) {
                tokens.push(c.to_string());
                i += 1;
            } else {
                return None;
            }
        }
    }
    Some(tokens)
}

fn precedence(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | ">" | "<=" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => return None,
    })
}

fn binary(tokens: &[String], at: &mut usize, min: u8) -> Option<Constant> {
    let mut left = unary(tokens, at)?;
    while let Some(op) = tokens.get(*at) {
        let Some(level) = precedence(op).filter(|level| *level > min) else {
            break;
        };
        let op = op.clone();
        *at += 1;
        let right = binary(tokens, at, level)?;
        let (a, b) = (left.number(), right.number());
        let truth = |value: bool| Constant::Number(f64::from(u8::from(value)));
        left = match op.as_str() {
            "+" => match (&left, &right) {
                (Constant::Text(_), _) | (_, Constant::Text(_)) => {
                    Constant::Text(left.word() + &right.word())
                }
                _ => Constant::Number(a + b),
            },
            "-" => Constant::Number(a - b),
            "*" => Constant::Number(a * b),
            "/" => Constant::Number(if b == 0.0 { 0.0 } else { a / b }),
            "%" => Constant::Number(if b == 0.0 { 0.0 } else { a % b }),
            "==" => truth(left.word() == right.word()),
            "!=" => truth(left.word() != right.word()),
            "<" => truth(a < b),
            ">" => truth(a > b),
            "<=" => truth(a <= b),
            ">=" => truth(a >= b),
            "&&" => truth(a != 0.0 && b != 0.0),
            _ => truth(a != 0.0 || b != 0.0),
        };
    }
    Some(left)
}

fn unary(tokens: &[String], at: &mut usize) -> Option<Constant> {
    let token = tokens.get(*at)?.clone();
    *at += 1;
    match token.as_str() {
        "(" => {
            let value = binary(tokens, at, 0)?;
            (tokens.get(*at).map(String::as_str) == Some(")")).then(|| *at += 1)?;
            Some(value)
        }
        "-" => Some(Constant::Number(-unary(tokens, at)?.number())),
        "!" => Some(Constant::Number(f64::from(u8::from(
            unary(tokens, at)?.number() == 0.0,
        )))),
        _ if token.starts_with('"') => Some(Constant::Text(token.trim_matches('"').to_owned())),
        _ => token.parse().ok().map(Constant::Number),
    }
}
