//! Black Ops menu expressions: postfix programs exactly as the zones store
//! them (`ExpressionStatement::rpn`). Operators `0..=23` are Black Ops'
//! `expressionOperatorType_e`; a comma folds call arguments into one list.
//!
//! Function indices are named only where Black Ops' own data proves them
//! (docs/fidelity/t5.md, "menu expression functions"); any other index is an
//! error, never a guess.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use crate::expr::{
    ExprError, ExprHost, OP_ADD, OP_AND, OP_DIVIDE, OP_EQUALS, OP_GREATERTHAN,
    OP_GREATERTHANEQUALTO, OP_LESSTHAN, OP_LESSTHANEQUALTO, OP_MODULUS, OP_MULTIPLY, OP_NOTEQUAL,
    OP_OR, OP_SUBTRACT, Operand, logic_op, source_str,
};

const T5_NOOP: i32 = 0;
const T5_MUL: i32 = 2;
const T5_DIV: i32 = 3;
const T5_MOD: i32 = 4;
const T5_PLUS: i32 = 5;
const T5_MINUS: i32 = 6;
const T5_NEGATE: i32 = 7;
const T5_NOT: i32 = 8;
const T5_SMALLER: i32 = 9;
const T5_SMALLEREQ: i32 = 10;
const T5_GREATER: i32 = 11;
const T5_GREATEREQ: i32 = 12;
const T5_EQ: i32 = 13;
const T5_NOTEQ: i32 = 14;
const T5_LOGAND: i32 = 15;
const T5_LOGOR: i32 = 16;
const T5_COMMA: i32 = 18;
const T5_BITAND: i32 = 19;
const T5_BITOR: i32 = 20;
const T5_BITNEG: i32 = 21;
const T5_SHIFTLEFT: i32 = 22;
const T5_SHIFTRIGHT: i32 = 23;

pub const T5_FN_DVARINT: i32 = 30;
pub const T5_FN_DVARBOOL: i32 = 31;
pub const T5_FN_DVARSTRING: i32 = 33;
pub const T5_FN_UI_ACTIVE: i32 = 34;
pub const T5_FN_FLASHBANGED: i32 = 35;
pub const T5_FN_INKILLCAM: i32 = 38;
pub const T5_FN_ISDUALWIELD: i32 = 39;
pub const T5_FN_ISFUELWEAPON: i32 = 40;
pub const T5_FN_PLAYER: i32 = 41;
pub const T5_FN_ADSJAVELIN: i32 = 56;
pub const T5_FN_KEYBINDING: i32 = 81;

#[derive(Clone, Debug)]
pub(crate) enum T5Token {
    Const(Operand),
    Op(i32),
}

#[derive(Clone, Debug)]
enum Value {
    One(Operand),
    Args(Vec<Operand>),
}

impl Value {
    fn into_args(self) -> Vec<Operand> {
        match self {
            Self::One(v) => alloc::vec![v],
            Self::Args(v) => v,
        }
    }

    fn operand(self) -> Result<Operand, ExprError> {
        match self {
            Self::One(v) => Ok(v),
            Self::Args(_) => Err(ExprError::StrayOperands),
        }
    }
}

pub(crate) fn parse(tokens: &[&str]) -> Result<Vec<T5Token>, ExprError> {
    tokens
        .iter()
        .map(|token| {
            if let Some(n) = token.strip_prefix('#') {
                return n
                    .parse()
                    .map(T5Token::Op)
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(n) = token.strip_prefix("i:") {
                return n
                    .parse()
                    .map(|v| T5Token::Const(Operand::Int(v)))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(bits) = token.strip_prefix("f:") {
                return u32::from_str_radix(bits, 16)
                    .map(|b| T5Token::Const(Operand::Float(f32::from_bits(b))))
                    .map_err(|_| ExprError::TruncatedDump);
            }
            if let Some(hex) = token.strip_prefix("s:") {
                let mut bytes = Vec::with_capacity(hex.len() / 2);
                for pair in hex.as_bytes().chunks(2) {
                    let digits =
                        core::str::from_utf8(pair).map_err(|_| ExprError::TruncatedDump)?;
                    bytes.push(
                        u8::from_str_radix(digits, 16).map_err(|_| ExprError::TruncatedDump)?,
                    );
                }
                let text = bytes.iter().map(|&b| char::from(b)).collect::<String>();
                return Ok(T5Token::Const(Operand::Str(text)));
            }
            Err(ExprError::TruncatedDump)
        })
        .collect()
}

/// How Black Ops converts between operand types is not known: only operations
/// whose operands need no conversion are evaluated.
const COERCION: &str = "t5.hud.expression_coercion";

fn same_type(a: &Operand, b: &Operand) -> bool {
    core::mem::discriminant(a) == core::mem::discriminant(b)
}

fn int(operand: Operand) -> Result<i32, ExprError> {
    match operand {
        Operand::Int(v) => Ok(v),
        _ => Err(ExprError::Unknown(COERCION)),
    }
}

fn pop(stack: &mut Vec<Value>) -> Result<Value, ExprError> {
    stack.pop().ok_or(ExprError::StackUnderflow)
}

fn pop_operand(stack: &mut Vec<Value>) -> Result<Operand, ExprError> {
    pop(stack)?.operand()
}

fn arg_str(args: &[Operand]) -> Result<String, ExprError> {
    args.first()
        .map(source_str)
        .ok_or(ExprError::StackUnderflow)
}

fn call(op: i32, stack: &mut Vec<Value>, host: &impl ExprHost) -> Result<Operand, ExprError> {
    let mut args = || pop(stack).map(Value::into_args);
    Ok(match op {
        T5_FN_DVARINT => Operand::Int(host.dvar_int(&arg_str(&args()?)?)?),
        T5_FN_DVARBOOL => Operand::Int(host.dvar_bool(&arg_str(&args()?)?)?),
        T5_FN_DVARSTRING => Operand::Str(host.dvar_string(&arg_str(&args()?)?)?),
        T5_FN_UI_ACTIVE => Operand::Int(host.ui_active()?),
        T5_FN_FLASHBANGED => Operand::Int(host.flashbanged()?),
        T5_FN_INKILLCAM => Operand::Int(host.in_killcam()?),
        T5_FN_ISDUALWIELD => Operand::Int(host.is_dual_wield()?),
        T5_FN_ISFUELWEAPON => Operand::Int(host.is_fuel_weapon()?),
        T5_FN_PLAYER => host.player_field(&arg_str(&args()?)?)?,
        T5_FN_ADSJAVELIN => Operand::Int(i32::from(host.weapon_lock()?.ads_javelin)),
        T5_FN_KEYBINDING => host.key_binding(&arg_str(&args()?)?)?,
        other => return Err(ExprError::UnsupportedOp(other)),
    })
}

pub(crate) fn evaluate(tokens: &[T5Token], host: &impl ExprHost) -> Result<Operand, ExprError> {
    let mut stack: Vec<Value> = Vec::new();
    for token in tokens {
        let op = match token {
            T5Token::Const(v) => {
                stack.push(Value::One(v.clone()));
                continue;
            }
            T5Token::Op(op) => *op,
        };
        let value = match op {
            T5_NOOP => continue,
            T5_COMMA => {
                let b = pop(&mut stack)?;
                let mut a = pop(&mut stack)?.into_args();
                a.extend(b.into_args());
                stack.push(Value::Args(a));
                continue;
            }
            T5_NEGATE => match pop_operand(&mut stack)? {
                Operand::Int(v) => Operand::Int(v.wrapping_neg()),
                Operand::Float(v) => Operand::Float(-v),
                Operand::Str(_) => return Err(ExprError::Host("negate a string")),
            },
            T5_NOT => Operand::Int(i32::from(int(pop_operand(&mut stack)?)? == 0)),
            T5_BITNEG => Operand::Int(!int(pop_operand(&mut stack)?)?),
            T5_MUL | T5_DIV | T5_MOD | T5_PLUS | T5_MINUS | T5_SMALLER..=T5_LOGOR => {
                let b = pop_operand(&mut stack)?;
                let a = pop_operand(&mut stack)?;
                let converts = !same_type(&a, &b)
                    || match a {
                        Operand::Int(_) => false,
                        Operand::Float(_) => matches!(op, T5_MOD | T5_LOGAND | T5_LOGOR),
                        Operand::Str(_) => !matches!(op, T5_PLUS | T5_EQ | T5_NOTEQ),
                    };
                if converts {
                    return Err(ExprError::Unknown(COERCION));
                }
                let engine_op = match op {
                    T5_MUL => OP_MULTIPLY,
                    T5_DIV => OP_DIVIDE,
                    T5_MOD => OP_MODULUS,
                    T5_PLUS => OP_ADD,
                    T5_MINUS => OP_SUBTRACT,
                    T5_SMALLER => OP_LESSTHAN,
                    T5_SMALLEREQ => OP_LESSTHANEQUALTO,
                    T5_GREATER => OP_GREATERTHAN,
                    T5_GREATEREQ => OP_GREATERTHANEQUALTO,
                    T5_EQ => OP_EQUALS,
                    T5_NOTEQ => OP_NOTEQUAL,
                    T5_LOGAND => OP_AND,
                    _ => OP_OR,
                };
                logic_op(engine_op, a, b)?
            }
            T5_BITAND | T5_BITOR | T5_SHIFTLEFT | T5_SHIFTRIGHT => {
                let b = int(pop_operand(&mut stack)?)?;
                let a = int(pop_operand(&mut stack)?)?;
                Operand::Int(match op {
                    T5_BITAND => a & b,
                    T5_BITOR => a | b,
                    T5_SHIFTLEFT => a.wrapping_shl(b as u32),
                    _ => a.wrapping_shr(b as u32),
                })
            }
            op if op >= 24 => call(op, &mut stack, host)?,
            other => return Err(ExprError::UnsupportedOp(other)),
        };
        stack.push(Value::One(value));
    }
    match stack.len() {
        0 => Err(ExprError::EmptyResult),
        1 => pop_operand(&mut stack),
        _ => Err(ExprError::StrayOperands),
    }
}
