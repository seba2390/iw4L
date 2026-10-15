//! Black Ops 2 menu expressions: postfix programs exactly as the zones store
//! them (`ExpressionStatement::rpn`). Operators `0..=23` are Black Ops 2's
//! (its evaluator's handler table starts with them, a no-op at 0 and no
//! handler for the parentheses); a function's number is its token's index in
//! the game's `expressions/functions.txt` (`patch_zm`, `//` comments skipped)
//! plus 24, which the evaluator maps by name to its own handler.
//!
//! Only the operators and functions Black Ops 2's zombies HUD menus use are
//! evaluated, each as its handler reads its operands; any other is an error,
//! never a guess.

use menu_expr::{ExprError, GameQueries, MenuProgram, Operand};

const NOOP: i32 = 0;
const PLUS: i32 = 5;
const NOT: i32 = 8;
const EQ: i32 = 13;
const NOTEQ: i32 = 14;
const LOGAND: i32 = 15;

const FN_DVARINT: i32 = 30;
const FN_DVARBOOL: i32 = 31;
const FN_DVARSTRING: i32 = 33;
const FN_ISAIRBURSTWEAPON: i32 = 529;

/// The evaluator builds a joined string in a 256-byte buffer.
const STRING_CAP: usize = 255;
/// The result reader writes text into a 2048-byte buffer.
const RESULT_TEXT_CAP: usize = 2047;

#[derive(Clone, Debug)]
enum Token {
    Const(Operand),
    Op(i32),
}

fn parse(tokens: &[&str]) -> Result<Vec<Token>, ExprError> {
    tokens
        .iter()
        .map(|token| {
            if let Some(n) = token.strip_prefix('#') {
                return n
                    .parse()
                    .map(Token::Op)
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(n) = token.strip_prefix("i:") {
                return n
                    .parse()
                    .map(|v| Token::Const(Operand::Int(v)))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(bits) = token.strip_prefix("f:") {
                return u32::from_str_radix(bits, 16)
                    .map(|b| Token::Const(Operand::Float(f32::from_bits(b))))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(hex) = token.strip_prefix("s:") {
                let mut text = String::with_capacity(hex.len() / 2);
                for pair in hex.as_bytes().chunks(2) {
                    let digits =
                        core::str::from_utf8(pair).map_err(|_| ExprError::TruncatedDump)?;
                    let byte =
                        u8::from_str_radix(digits, 16).map_err(|_| ExprError::TruncatedDump)?;
                    text.push(char::from(byte));
                }
                return Ok(Token::Const(Operand::Str(text)));
            }
            Err(ExprError::TruncatedDump)
        })
        .collect()
}

/// C's `atoi`: leading blanks, a sign, then digits; 0 when none.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// C's `atof`: the longest leading number; 0 when none.
fn atof(text: &str) -> f32 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    end += digits(end);
    if bytes.get(end) == Some(&b'.') {
        end += 1 + digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exp = end + 1;
        if matches!(bytes.get(exp), Some(b'-' | b'+')) {
            exp += 1;
        }
        if digits(exp) > 0 {
            end = exp + digits(exp);
        }
    }
    text[..end].parse::<f64>().map_or(0.0, |v| v as f32)
}

fn source_int(value: &Operand) -> i32 {
    match value {
        Operand::Int(v) => *v,
        Operand::Float(v) => *v as i32,
        Operand::Str(v) => atoi(v),
    }
}

fn source_float(value: &Operand) -> f32 {
    match value {
        Operand::Int(v) => *v as f32,
        Operand::Float(v) => *v,
        Operand::Str(v) => atof(v),
    }
}

/// An operand as text where `+` joins it: an int as `%i`, a float as `%f`.
fn source_string(value: &Operand) -> String {
    match value {
        Operand::Str(v) => v.clone(),
        Operand::Int(v) => format!("{v}"),
        Operand::Float(v) => format!("{v:.6}"),
    }
}

/// `==`'s string conversion: an int when its `atof` and `atoi` agree, else
/// a float.
fn make_number(value: Operand) -> Operand {
    match value {
        Operand::Str(text) => {
            let (f, i) = (atof(&text), atoi(&text));
            if f == i as f32 {
                Operand::Int(i)
            } else {
                Operand::Float(f)
            }
        }
        other => other,
    }
}

/// `==`: two strings compare without case; operands of different types are
/// made numbers, two ints compare as ints, else as floats.
fn equals(a: Operand, b: Operand) -> bool {
    match (a, b) {
        (Operand::Str(a), Operand::Str(b)) => a.eq_ignore_ascii_case(&b),
        (a, b) => match (make_number(a), make_number(b)) {
            (Operand::Int(a), Operand::Int(b)) => a == b,
            (a, b) => source_float(&a) == source_float(&b),
        },
    }
}

/// `&&`'s operand: a string is its first character.
fn logic_value(value: &Operand) -> bool {
    match value {
        Operand::Int(v) => *v != 0,
        Operand::Float(v) => *v != 0.0,
        Operand::Str(v) => v.bytes().next().is_some_and(|b| b != 0),
    }
}

/// `+`: two ints add as an int, a float makes a float, and a string joins
/// the other operand's text, at most 255 characters.
fn plus(a: Operand, b: Operand) -> Operand {
    match (a, b) {
        (Operand::Int(a), Operand::Int(b)) => Operand::Int(a.wrapping_add(b)),
        (a @ Operand::Str(_), b) | (a, b @ Operand::Str(_)) => {
            let mut text = source_string(&a) + &source_string(&b);
            text.truncate(STRING_CAP);
            Operand::Str(text)
        }
        (a, b) => Operand::Float(source_float(&a) + source_float(&b)),
    }
}

/// A dvar function's name: a number names no dvar here, which the host does
/// not look up.
fn dvar_name(value: Operand) -> Result<String, ExprError> {
    match value {
        Operand::Str(v) => Ok(v),
        Operand::Int(_) => Err(ExprError::Host("a dvar named by number")),
        Operand::Float(_) => Err(ExprError::Host(
            "Must use a string as the name of a dvar, not a Float",
        )),
    }
}

fn pop(stack: &mut Vec<Operand>) -> Result<Operand, ExprError> {
    stack.pop().ok_or(ExprError::StackUnderflow)
}

fn evaluate(tokens: &[Token], host: &dyn GameQueries) -> Result<Operand, ExprError> {
    let mut stack: Vec<Operand> = Vec::new();
    for token in tokens {
        let op = match token {
            Token::Const(v) => {
                stack.push(v.clone());
                continue;
            }
            Token::Op(op) => *op,
        };
        let value = match op {
            NOOP => continue,
            // `!` on a string is an error that leaves 0.
            NOT => Operand::Int(match pop(&mut stack)? {
                Operand::Int(v) => i32::from(v == 0),
                Operand::Float(v) => i32::from(v == 0.0),
                Operand::Str(_) => 0,
            }),
            PLUS | EQ | NOTEQ | LOGAND => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                match op {
                    PLUS => plus(a, b),
                    EQ => Operand::Int(i32::from(equals(a, b))),
                    NOTEQ => Operand::Int(i32::from(!equals(a, b))),
                    _ => Operand::Int(i32::from(logic_value(&a) && logic_value(&b))),
                }
            }
            FN_DVARINT => Operand::Int(host.dvar_int(&dvar_name(pop(&mut stack)?)?)?),
            FN_DVARBOOL => Operand::Int(host.dvar_bool(&dvar_name(pop(&mut stack)?)?)?),
            FN_DVARSTRING => Operand::Str(host.dvar_string(&dvar_name(pop(&mut stack)?)?)?),
            FN_ISAIRBURSTWEAPON => Operand::Int(host.airburst_weapon()?),
            other => return Err(ExprError::UnsupportedOp(other)),
        };
        stack.push(value);
    }
    match stack.len() {
        0 => Err(ExprError::EmptyResult),
        1 => pop(&mut stack),
        _ => Err(ExprError::StrayOperands),
    }
}

#[derive(Debug)]
struct Program(Vec<Token>);

impl MenuProgram for Program {
    fn evaluate(&self, queries: &dyn GameQueries) -> Result<Operand, ExprError> {
        evaluate(&self.0, queries)
    }
    /// A condition: a float truncates to an int first, a string is `atoi`.
    fn truth(&self, value: &Operand) -> Result<bool, ExprError> {
        Ok(source_int(value) != 0)
    }
    fn number(&self, value: &Operand) -> Result<f32, ExprError> {
        Ok(source_float(value))
    }
    /// Text: an int as `%i`, a float as `%f`, at most 2047 characters.
    fn text(&self, value: &Operand) -> Result<String, ExprError> {
        let mut text = source_string(value);
        text.truncate(RESULT_TEXT_CAP);
        Ok(text)
    }
}

/// A Black Ops 2 menu expression from its catalog (the tokens after `t6`).
pub fn parse_menu_expression(tokens: &[&str]) -> Result<Box<dyn MenuProgram>, ExprError> {
    Ok(Box::new(Program(parse(tokens)?)))
}
