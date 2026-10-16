//! Lua 5.1 string patterns, as HavokScript's `string` library matches them
//! (`lstrlib.c`): classes, sets, anchors, the four quantifiers, captures,
//! position captures, back references, `%b` and `%f`.

const MAX_CAPTURES: usize = 32;
const MAX_DEPTH: usize = 200;

/// A capture a match found: text by its byte range, or a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Text(usize, usize),
    Position(usize),
}

/// What makes a pattern malformed, as the library raises it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternError(pub &'static str);

#[derive(Clone, Copy)]
enum Length {
    Closed(usize),
    Unclosed,
    Position,
}

struct State<'a> {
    source: &'a [u8],
    pattern: &'a [u8],
    level: usize,
    captures: [(usize, Length); MAX_CAPTURES],
    depth: usize,
}

/// The pattern's specials: a pattern without them matches as plain text.
pub fn has_specials(pattern: &[u8]) -> bool {
    pattern.iter().any(|byte| b"^$*+?.([%-".contains(byte))
}

fn class_matches(byte: u8, class: u8) -> bool {
    let matched = match class.to_ascii_lowercase() {
        b'a' => byte.is_ascii_alphabetic(),
        b'c' => byte.is_ascii_control(),
        b'd' => byte.is_ascii_digit(),
        b'l' => byte.is_ascii_lowercase(),
        b'p' => byte.is_ascii_punctuation(),
        b's' => matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'),
        b'u' => byte.is_ascii_uppercase(),
        b'w' => byte.is_ascii_alphanumeric(),
        b'x' => byte.is_ascii_hexdigit(),
        b'z' => byte == 0,
        _ => return class == byte,
    };
    if class.is_ascii_uppercase() {
        !matched
    } else {
        matched
    }
}

impl State<'_> {
    /// The end of the single-character class at `p`.
    fn class_end(&self, mut p: usize) -> Result<usize, PatternError> {
        let byte = self.pattern[p];
        p += 1;
        if byte == b'%' {
            if p >= self.pattern.len() {
                return Err(PatternError("malformed pattern (ends with '%')"));
            }
            return Ok(p + 1);
        }
        if byte == b'[' {
            if self.pattern.get(p) == Some(&b'^') {
                p += 1;
            }
            loop {
                let Some(&next) = self.pattern.get(p) else {
                    return Err(PatternError("malformed pattern (missing ']')"));
                };
                p += 1;
                if next == b'%' {
                    if p >= self.pattern.len() {
                        return Err(PatternError("malformed pattern (missing ']')"));
                    }
                    p += 1;
                }
                if self.pattern.get(p) == Some(&b']') {
                    return Ok(p + 1);
                }
            }
        }
        Ok(p)
    }

    /// Whether `byte` is in the set `[...]` spanning `p..end` (`end` at its
    /// closing `]`).
    fn in_set(&self, byte: u8, mut p: usize, end: usize) -> bool {
        let mut accept = true;
        p += 1;
        if self.pattern[p] == b'^' {
            accept = false;
            p += 1;
        }
        while p < end {
            if self.pattern[p] == b'%' && p + 1 < end {
                p += 1;
                if class_matches(byte, self.pattern[p]) {
                    return accept;
                }
                p += 1;
            } else if p + 2 < end && self.pattern[p + 1] == b'-' {
                if self.pattern[p] <= byte && byte <= self.pattern[p + 2] {
                    return accept;
                }
                p += 3;
            } else {
                if self.pattern[p] == byte {
                    return accept;
                }
                p += 1;
            }
        }
        !accept
    }

    fn single_matches(&self, s: usize, p: usize, end: usize) -> bool {
        let Some(&byte) = self.source.get(s) else {
            return false;
        };
        match self.pattern[p] {
            b'.' => true,
            b'%' => class_matches(byte, self.pattern[p + 1]),
            b'[' => self.in_set(byte, p, end - 1),
            literal => literal == byte,
        }
    }

    fn do_match(&mut self, mut s: usize, mut p: usize) -> Result<Option<usize>, PatternError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(PatternError("pattern too complex"));
        }
        let result = loop {
            if p >= self.pattern.len() {
                break Some(s);
            }
            match self.pattern[p] {
                b'(' => {
                    break if self.pattern.get(p + 1) == Some(&b')') {
                        self.start_capture(s, p + 2, Length::Position)?
                    } else {
                        self.start_capture(s, p + 1, Length::Unclosed)?
                    };
                }
                b')' => break self.end_capture(s, p + 1)?,
                b'$' if p + 1 == self.pattern.len() => {
                    break (s == self.source.len()).then_some(s);
                }
                b'%' if self.pattern.get(p + 1) == Some(&b'b') => {
                    break match self.balance(s, p + 2)? {
                        Some(next) => {
                            s = next;
                            p += 4;
                            continue;
                        }
                        None => None,
                    };
                }
                b'%' if self.pattern.get(p + 1) == Some(&b'f') => {
                    p += 2;
                    if self.pattern.get(p) != Some(&b'[') {
                        return Err(PatternError("missing '[' after '%f' in pattern"));
                    }
                    let end = self.class_end(p)?;
                    let previous = if s == 0 { 0 } else { self.source[s - 1] };
                    let current = self.source.get(s).copied().unwrap_or(0);
                    if !self.in_set(previous, p, end - 1) && self.in_set(current, p, end - 1) {
                        p = end;
                        continue;
                    }
                    break None;
                }
                b'%' if self.pattern.get(p + 1).is_some_and(u8::is_ascii_digit) => {
                    break match self.back_reference(s, self.pattern[p + 1])? {
                        Some(next) => {
                            s = next;
                            p += 2;
                            continue;
                        }
                        None => None,
                    };
                }
                _ => {
                    let end = self.class_end(p)?;
                    let matched = self.single_matches(s, p, end);
                    match self.pattern.get(end) {
                        Some(b'?') => {
                            if matched && let Some(found) = self.do_match(s + 1, end + 1)? {
                                break Some(found);
                            }
                            p = end + 1;
                        }
                        Some(b'*') => break self.max_expand(s, p, end)?,
                        Some(b'+') => {
                            break if matched {
                                self.max_expand(s + 1, p, end)?
                            } else {
                                None
                            };
                        }
                        Some(b'-') => break self.min_expand(s, p, end)?,
                        _ => {
                            if !matched {
                                break None;
                            }
                            s += 1;
                            p = end;
                        }
                    }
                }
            }
        };
        self.depth -= 1;
        Ok(result)
    }

    fn max_expand(
        &mut self,
        s: usize,
        p: usize,
        end: usize,
    ) -> Result<Option<usize>, PatternError> {
        let mut count = 0;
        while self.single_matches(s + count, p, end) {
            count += 1;
        }
        loop {
            if let Some(found) = self.do_match(s + count, end + 1)? {
                return Ok(Some(found));
            }
            if count == 0 {
                return Ok(None);
            }
            count -= 1;
        }
    }

    fn min_expand(
        &mut self,
        mut s: usize,
        p: usize,
        end: usize,
    ) -> Result<Option<usize>, PatternError> {
        loop {
            if let Some(found) = self.do_match(s, end + 1)? {
                return Ok(Some(found));
            }
            if self.single_matches(s, p, end) {
                s += 1;
            } else {
                return Ok(None);
            }
        }
    }

    fn start_capture(
        &mut self,
        s: usize,
        p: usize,
        length: Length,
    ) -> Result<Option<usize>, PatternError> {
        if self.level >= MAX_CAPTURES {
            return Err(PatternError("too many captures"));
        }
        self.captures[self.level] = (s, length);
        self.level += 1;
        let found = self.do_match(s, p)?;
        if found.is_none() {
            self.level -= 1;
        }
        Ok(found)
    }

    fn end_capture(&mut self, s: usize, p: usize) -> Result<Option<usize>, PatternError> {
        let open = (0..self.level)
            .rev()
            .find(|&index| matches!(self.captures[index].1, Length::Unclosed))
            .ok_or(PatternError("invalid pattern capture"))?;
        self.captures[open].1 = Length::Closed(s - self.captures[open].0);
        let found = self.do_match(s, p)?;
        if found.is_none() {
            self.captures[open].1 = Length::Unclosed;
        }
        Ok(found)
    }

    fn balance(&self, s: usize, p: usize) -> Result<Option<usize>, PatternError> {
        let (Some(&open), Some(&close)) = (self.pattern.get(p), self.pattern.get(p + 1)) else {
            return Err(PatternError("unbalanced pattern"));
        };
        if self.source.get(s) != Some(&open) {
            return Ok(None);
        }
        let mut depth = 1;
        let mut at = s + 1;
        while let Some(&byte) = self.source.get(at) {
            if byte == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(at + 1));
                }
            } else if byte == open {
                depth += 1;
            }
            at += 1;
        }
        Ok(None)
    }

    fn back_reference(&self, s: usize, digit: u8) -> Result<Option<usize>, PatternError> {
        let index = (digit - b'1') as usize;
        let Some(&(start, Length::Closed(length))) = self.captures[..self.level].get(index) else {
            return Err(PatternError("invalid capture index"));
        };
        let wanted = &self.source[start..start + length];
        Ok(self
            .source
            .get(s..s + length)
            .filter(|found| *found == wanted)
            .map(|_| s + length))
    }

    fn results(&self) -> Result<Vec<Capture>, PatternError> {
        self.captures[..self.level]
            .iter()
            .map(|(at, length)| match length {
                Length::Closed(length) => Ok(Capture::Text(*at, at + length)),
                Length::Position => Ok(Capture::Position(*at)),
                Length::Unclosed => Err(PatternError("unfinished capture")),
            })
            .collect()
    }
}

/// The first match of `pattern` in `source` at or after byte `init`: its
/// byte range and the pattern's captures (none when it has none).
pub fn find(
    source: &[u8],
    pattern: &[u8],
    init: usize,
) -> Result<Option<(usize, usize, Vec<Capture>)>, PatternError> {
    let anchored = pattern.first() == Some(&b'^');
    let mut state = State {
        source,
        pattern: if anchored { &pattern[1..] } else { pattern },
        level: 0,
        captures: [(0, Length::Unclosed); MAX_CAPTURES],
        depth: 0,
    };
    let mut start = init;
    loop {
        state.level = 0;
        state.depth = 0;
        if let Some(end) = state.do_match(start, 0)? {
            let captures = state.results()?;
            return Ok(Some((start, end, captures)));
        }
        start += 1;
        if anchored || start > source.len() {
            return Ok(None);
        }
    }
}

/// A match of `pattern` (without its `^` anchor) starting exactly at byte
/// `start`: where it ends and its captures.
pub fn match_at(
    source: &[u8],
    pattern: &[u8],
    start: usize,
) -> Result<Option<(usize, Vec<Capture>)>, PatternError> {
    let mut state = State {
        source,
        pattern,
        level: 0,
        captures: [(0, Length::Unclosed); MAX_CAPTURES],
        depth: 0,
    };
    match state.do_match(start, 0)? {
        Some(end) => Ok(Some((end, state.results()?))),
        None => Ok(None),
    }
}
