//! Black Ops' runtime expression compiler: the `( … )` group a menu's
//! console line carries (`execnow setCurrentItemIndex ( getCACItemIndex (
//! … ) )`) is read as infix text, turned to the postfix form its menus are
//! compiled to, and evaluated as they are. The console then sees the
//! group's value as one word: an integer as `%i`, a float as `%f`, a string
//! as it is; a group that does not compile is the empty word.

use menu_expr::{ExprError, MenuProgram, Operand};

use crate::menu_expression::{Program, T5Token, atof, atoi};
use crate::menu_functions::MENU_FUNCTIONS;

/// The operators by number, as the expression parser matches a token
/// against them (the first match wins, so `-` is subtraction until the
/// unary rule makes it a negation).
const OPERATORS: [&str; 24] = [
    "NOOP", ")", "*", "/", "%", "+", "-", "-", "!", "<", "<=", ">", ">=", "==", "!=", "&&", "||",
    "(", ",", "&", "|", "~", "<<", ">>",
];
/// Each operator's binding: a lower number binds first; an operator on the
/// stack is emitted before one of the same or a looser binding.
const PRECEDENCE: [i32; 24] = [
    i32::MAX,
    0,
    11,
    11,
    11,
    13,
    13,
    9,
    9,
    15,
    15,
    15,
    15,
    16,
    16,
    25,
    25,
    99,
    80,
    17,
    18,
    9,
    14,
    14,
];
const RIGHT_PAREN: i32 = 1;
const MINUS: i32 = 6;
const NEGATE: i32 = 7;
const LEFT_PAREN: i32 = 0x11;
/// Operation numbers from here on are functions.
const FIRST_FUNCTION: i32 = 24;
/// The punctuation the parser reads as one token; any other punctuation
/// character is a token alone.
const PUNCTUATION: [&str; 16] = [
    "+=", "-=", "*=", "/=", "&=", "|=", "++", "--", "&&", "||", "<=", ">=", "==", "!=", "<<", ">>",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Number,
    String,
    Name,
    Punctuation,
}

struct Lexeme {
    kind: Kind,
    text: String,
}

fn name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'_' | b'/' | b'\\')
}

fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The script parser's tokens: blanks (and bytes past 0x7f) and comments
/// skipped, a quoted string with `\"` and `\\` escapes, a number (digits
/// and dots, an exponent), a name, or punctuation.
fn lex(text: &str) -> Vec<Lexeme> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let string = |from: &[u8]| from.iter().map(|&b| char::from(b)).collect::<String>();
    while i < bytes.len() {
        let b = bytes[i];
        if !(b'!'..=0x7f).contains(&b) {
            i += 1;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if b == b'"' {
            let mut value = Vec::new();
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && matches!(bytes.get(i + 1), Some(b'"' | b'\\')) {
                    i += 1;
                }
                value.push(bytes[i]);
                i += 1;
            }
            i += 1;
            out.push(Lexeme {
                kind: Kind::String,
                text: string(&value),
            });
            continue;
        }
        let start = i;
        let number =
            b.is_ascii_digit() || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit));
        if number {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if matches!(bytes.get(i), Some(b'e' | b'E')) {
                i += 1;
                if matches!(bytes.get(i), Some(b'-' | b'+')) {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(Lexeme {
                kind: Kind::Number,
                text: string(&bytes[start..i]),
            });
            continue;
        }
        if name_start(b) {
            i += 1;
            while i < bytes.len() && name_char(bytes[i]) {
                i += 1;
            }
            out.push(Lexeme {
                kind: Kind::Name,
                text: string(&bytes[start..i]),
            });
            continue;
        }
        let len = PUNCTUATION
            .iter()
            .find(|p| bytes[i..].starts_with(p.as_bytes()))
            .map_or(1, |p| p.len());
        i += len;
        out.push(Lexeme {
            kind: Kind::Punctuation,
            text: string(&bytes[start..i]),
        });
    }
    out
}

/// An operator's or a function's number; 0 for a token that is neither.
fn operation(text: &str) -> i32 {
    if let Some(op) = OPERATORS.iter().position(|o| o.eq_ignore_ascii_case(text)) {
        return op as i32;
    }
    MENU_FUNCTIONS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
        .map_or(0, |&(_, op)| op)
}

enum Infix {
    Operand(Operand),
    Op(i32),
}

#[derive(PartialEq, Eq)]
enum Last {
    Start,
    Operand,
    Op(i32),
}

/// The group's tokens in order, from its opening `(` to the `)` that closes
/// it (or a `;`): a function's own `(` is not kept, its function stands for
/// it on the operator stack.
fn infix(text: &str) -> Result<Vec<Infix>, String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut last = Last::Start;
    for lexeme in lex(text) {
        if lexeme.text == ";" {
            break;
        }
        let op = if lexeme.kind == Kind::String {
            0
        } else {
            operation(&lexeme.text)
        };
        if op != 0 {
            let mut op = op;
            match op {
                MINUS
                    if last == Last::Start
                        || matches!(last, Last::Op(prev) if prev != RIGHT_PAREN) =>
                {
                    op = NEGATE;
                }
                LEFT_PAREN => {
                    depth += 1;
                    match last {
                        Last::Operand => {
                            return Err(format!("Expression Error: {}(...", lexeme.text));
                        }
                        Last::Op(prev) if prev >= FIRST_FUNCTION => {
                            last = Last::Op(LEFT_PAREN);
                            continue;
                        }
                        _ => {}
                    }
                }
                RIGHT_PAREN => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(
                            "Expression Error: Found a right parenthesis that doesn't match any left parenthesis"
                                .into(),
                        );
                    }
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            out.push(Infix::Op(op));
            last = Last::Op(op);
            continue;
        }
        let operand = match lexeme.kind {
            Kind::Number => {
                let (f, n) = (atof(&lexeme.text), atoi(&lexeme.text));
                if f == n as f32 {
                    Operand::Int(n)
                } else {
                    Operand::Float(f)
                }
            }
            Kind::String | Kind::Name => Operand::Str(lexeme.text),
            Kind::Punctuation => {
                return Err(format!("Expression Error: Unknown token '{}'", lexeme.text));
            }
        };
        out.push(Infix::Operand(operand));
        last = Last::Operand;
    }
    Ok(out)
}

/// The infix tokens in postfix order: operators wait on a stack until one
/// that binds no tighter comes, a `)` empties the stack down to its `(` or
/// function (emitting the function), and what is left at the end, down to
/// the group's own `(`, follows.
fn postfix(tokens: Vec<Infix>) -> Result<Vec<T5Token>, String> {
    let mut out = Vec::new();
    let mut stack: Vec<i32> = Vec::new();
    let precedence = |op: i32| PRECEDENCE[op as usize];
    for token in tokens {
        let op = match token {
            Infix::Operand(value) => {
                out.push(T5Token::Const(value));
                continue;
            }
            Infix::Op(op) => op,
        };
        if op == RIGHT_PAREN {
            while let Some(&top) = stack.last() {
                if top == LEFT_PAREN || top >= FIRST_FUNCTION {
                    break;
                }
                stack.pop();
                out.push(T5Token::Op(top));
            }
            match stack.pop() {
                Some(LEFT_PAREN) => {}
                Some(function) => out.push(T5Token::Op(function)),
                None => return Err("Expression Error: unbalanced parentheses".into()),
            }
            continue;
        }
        if op != LEFT_PAREN && op < FIRST_FUNCTION {
            while let Some(&top) = stack.last() {
                if top >= FIRST_FUNCTION || precedence(op) < precedence(top) {
                    break;
                }
                stack.pop();
                out.push(T5Token::Op(top));
            }
        }
        stack.push(op);
    }
    while let Some(top) = stack.pop() {
        if top == LEFT_PAREN {
            break;
        }
        out.push(T5Token::Op(top));
    }
    Ok(out)
}

/// A `( … )` group's text, as its bytes in hex (the tokens after the
/// `t5-infix` tag), compiled to a program. A group Black Ops cannot compile
/// reads as the empty word, as it does there.
pub fn parse_menu_infix(tokens: &[&str]) -> Result<Box<dyn MenuProgram>, ExprError> {
    let [hex] = tokens else {
        return Err(ExprError::TruncatedDump);
    };
    let mut text = String::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks(2) {
        let digits = core::str::from_utf8(pair).map_err(|_| ExprError::TruncatedDump)?;
        let byte = u8::from_str_radix(digits, 16).map_err(|_| ExprError::TruncatedDump)?;
        text.push(char::from(byte));
    }
    let program = infix(&text).and_then(postfix).unwrap_or_else(|error| {
        diag::warn!(Ui, "runtime:0 - {error}\n{text}");
        vec![T5Token::Const(Operand::Str(String::new()))]
    });
    Ok(Box::new(Program(program)))
}
