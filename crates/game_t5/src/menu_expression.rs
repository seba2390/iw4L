//! Black Ops menu expressions: postfix programs exactly as the zones store
//! them (`ExpressionStatement::rpn`). Operators `0..=23` are Black Ops'
//! `expressionOperatorType_e`; a comma folds call arguments into one list.
//!
//! A function's number is its token's index in the game's own
//! `expressions/functions.txt` (`code_post_gfx`, comments and blank lines
//! skipped) plus 24: the expression evaluator maps that index through the
//! file's names to its own handler (a function the file does not list keeps
//! the evaluator's own number plus 24). Only functions the host answers are
//! evaluated; any other is an error, never a guess. Operand conversions
//! follow Black Ops' own operators and result readers.

use menu_expr::{ExprError, GameQueries, MenuProgram, Operand};

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

const T5_FN_MIN: i32 = 26;
const T5_FN_MAX: i32 = 27;
const T5_FN_DVARINT: i32 = 30;
const T5_FN_DVARBOOL: i32 = 31;
const T5_FN_DVARSTRING: i32 = 33;
const T5_FN_UI_ACTIVE: i32 = 34;
const T5_FN_FLASHBANGED: i32 = 35;
const T5_FN_INKILLCAM: i32 = 38;
const T5_FN_ISDUALWIELD: i32 = 39;
const T5_FN_ISFUELWEAPON: i32 = 40;
const T5_FN_PLAYER: i32 = 41;
const T5_FN_TEAM: i32 = 43;
const T5_FN_OTHERTEAM: i32 = 44;
const T5_FN_MENUISOPEN: i32 = 47;
const T5_FN_INPRIVATEPARTY: i32 = 50;
const T5_FN_PRIVATEPARTYHOST: i32 = 51;
const T5_FN_PRIVATEPARTYHOSTINLOBBY: i32 = 52;
const T5_FN_ADSJAVELIN: i32 = 56;
const T5_FN_LOCSTRING: i32 = 64;
const T5_FN_LOCALVARINT: i32 = 65;
const T5_FN_LOCALVARBOOL: i32 = 66;
const T5_FN_LOCALVARSTRING: i32 = 68;
const T5_FN_TIMELEFT: i32 = 69;
const T5_FN_SECONDSASCOUNTDOWN: i32 = 70;
const T5_FN_INT: i32 = 72;
const T5_FN_GAMETYPENAME: i32 = 75;
const T5_FN_SCOREATRANK: i32 = 78;
const T5_FN_SPECTATINGCLIENT: i32 = 80;
const T5_FN_KEYBINDING: i32 = 81;
const T5_FN_GAMEHOST: i32 = 86;
const T5_FN_ISVISIBILITYBITSET: i32 = 87;
const T5_FN_ISSPLITSCREEN: i32 = 88;
const T5_FN_ISSPLITSCREENHOST: i32 = 89;
const T5_FN_ISINTERMISSION: i32 = 104;
const T5_FN_GETTEXTWIDTH: i32 = 204;
const T5_FN_ISPROFILESIGNEDIN: i32 = 209;
const T5_FN_SELECT: i32 = 291;
const T5_FN_GETLOCALCLIENTNUM: i32 = 321;
const T5_FN_TOUPPER: i32 = 425;
// Not in `functions.txt`: the evaluator keeps its own number for these.
const T5_FN_MENUHASFOCUS: i32 = 309;
const T5_FN_ISINGAME: i32 = 355;

/// The party functions read the live party's state, which IW4L does not
/// keep in Black Ops' terms.
const PARTY: &str = game_api::unknown!(
    "t5.hud.party_state",
    "Black Ops' menu party tests (`inprivateparty`, `privatepartyhost`, `privatepartyhostinlobby`, `gamehost`) fail, so an item they gate does not draw",
    "Black Ops' party state for a solo and a hosted zombies match, read from its executable"
)
.id;

#[derive(Clone, Debug)]
enum T5Token {
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
            Self::One(v) => vec![v],
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

fn parse(tokens: &[&str]) -> Result<Vec<T5Token>, ExprError> {
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

/// C's `atoi`: leading blanks, a sign, then digits; 0 when none.
pub(crate) fn atoi(text: &str) -> i32 {
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
pub(crate) fn atof(text: &str) -> f32 {
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

/// An operand as an int: a float truncates, a string is `atoi`.
fn source_int(value: &Operand) -> i32 {
    match value {
        Operand::Int(v) => *v,
        Operand::Float(v) => *v as i32,
        Operand::Str(v) => atoi(v),
    }
}

/// An operand as a float: a string is `atof`.
fn source_float(value: &Operand) -> f32 {
    match value {
        Operand::Int(v) => *v as f32,
        Operand::Float(v) => *v,
        Operand::Str(v) => atof(v),
    }
}

/// An operand as text: an int as `%i`, a float as `%f`.
fn source_string(value: &Operand) -> String {
    match value {
        Operand::Str(v) => v.clone(),
        Operand::Int(v) => format!("{v}"),
        Operand::Float(v) => format!("{v:.6}"),
    }
}

/// A string operand made a number: an int when its `atof` and `atoi` agree,
/// else a float.
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

/// `&&` and `||`: a string tests its first character.
fn logic_value(value: &Operand) -> bool {
    match value {
        Operand::Int(v) => *v != 0,
        Operand::Float(v) => *v != 0.0,
        Operand::Str(v) => v.bytes().next().is_some_and(|b| b != 0),
    }
}

/// A program's value as a condition: a float truncates to an int first, a
/// string is `atoi`.
fn truth(value: &Operand) -> Result<bool, ExprError> {
    Ok(source_int(value) != 0)
}

/// A program's value where a number is wanted.
fn number(value: &Operand) -> Result<f32, ExprError> {
    Ok(source_float(value))
}

/// A program's value where text is wanted.
fn text(value: &Operand) -> Result<String, ExprError> {
    Ok(source_string(value))
}

/// `*` and `-`: strings made numbers; two ints stay an int, else a float.
fn arithmetic(
    a: Operand,
    b: Operand,
    int: fn(i32, i32) -> i32,
    float: fn(f32, f32) -> f32,
) -> Operand {
    match (make_number(a), make_number(b)) {
        (Operand::Int(a), Operand::Int(b)) => Operand::Int(int(a, b)),
        (a, b) => Operand::Float(float(source_float(&a), source_float(&b))),
    }
}

/// The orderings: strings made numbers; two ints compare as ints, else as
/// floats.
fn compare(a: Operand, b: Operand, int: fn(i32, i32) -> bool, float: fn(f32, f32) -> bool) -> bool {
    match (make_number(a), make_number(b)) {
        (Operand::Int(a), Operand::Int(b)) => int(a, b),
        (a, b) => float(source_float(&a), source_float(&b)),
    }
}

/// `==`: two strings compare without case; operands of different types
/// compare as numbers.
fn equals(a: Operand, b: Operand) -> bool {
    match (a, b) {
        (Operand::Str(a), Operand::Str(b)) => a.eq_ignore_ascii_case(&b),
        (a, b) => compare(a, b, |a, b| a == b, |a, b| a == b),
    }
}

/// `%`'s operand: a float rounds to nearest after adding 2^-30 (the x87
/// store), a string is `atoi`.
fn modulus_int(value: &Operand) -> i32 {
    match value {
        Operand::Float(v) => (f64::from(*v) + f64::from(2.0f32.powi(-30))).round_ties_even() as i32,
        other => source_int(other),
    }
}

fn binary(op: i32, a: Operand, b: Operand) -> Result<Operand, ExprError> {
    let flag = |v: bool| Operand::Int(i32::from(v));
    Ok(match op {
        T5_MUL => arithmetic(a, b, i32::wrapping_mul, |a, b| a * b),
        T5_MINUS => arithmetic(a, b, i32::wrapping_sub, |a, b| a - b),
        // `+` joins text with `%i`/`%f` numbers and converts nothing.
        T5_PLUS => match (a, b) {
            (Operand::Int(a), Operand::Int(b)) => Operand::Int(a.wrapping_add(b)),
            (a @ Operand::Str(_), b) | (a, b @ Operand::Str(_)) => {
                Operand::Str(source_string(&a) + &source_string(&b))
            }
            (a, b) => Operand::Float(source_float(&a) + source_float(&b)),
        },
        // `/` is always a float, over 1 for a zero divisor.
        T5_DIV => {
            let divisor = source_float(&b);
            Operand::Float(source_float(&a) / if divisor == 0.0 { 1.0 } else { divisor })
        }
        // `%` is an int, over 1 for a zero divisor.
        T5_MOD => {
            let divisor = modulus_int(&b);
            Operand::Int(modulus_int(&a).wrapping_rem(if divisor == 0 { 1 } else { divisor }))
        }
        T5_SMALLER => flag(compare(a, b, |a, b| a < b, |a, b| a < b)),
        T5_SMALLEREQ => flag(compare(a, b, |a, b| a <= b, |a, b| a <= b)),
        T5_GREATER => flag(compare(a, b, |a, b| a > b, |a, b| a > b)),
        T5_GREATEREQ => flag(compare(a, b, |a, b| a >= b, |a, b| a >= b)),
        T5_EQ => flag(equals(a, b)),
        T5_NOTEQ => flag(!equals(a, b)),
        T5_LOGAND => flag(logic_value(&a) && logic_value(&b)),
        T5_LOGOR => flag(logic_value(&a) || logic_value(&b)),
        other => return Err(ExprError::UnsupportedOp(other)),
    })
}

/// `secondsascountdown`: milliseconds as minutes (two wide) and two-digit
/// seconds, with tenths when asked; a negative count is zero.
fn countdown(ms: i32, tenths: bool) -> String {
    let ms = ms.max(0);
    let seconds = ms / 1000;
    if tenths {
        format!("{:2}:{:02}.{}", seconds / 60, seconds % 60, ms % 1000 / 100)
    } else {
        format!("{:2}:{:02}", seconds / 60, seconds % 60)
    }
}

fn pop(stack: &mut Vec<Value>) -> Result<Value, ExprError> {
    stack.pop().ok_or(ExprError::StackUnderflow)
}

fn pop_operand(stack: &mut Vec<Value>) -> Result<Operand, ExprError> {
    pop(stack)?.operand()
}

/// A name argument. Black Ops reports a float there; a dvar function looks
/// an int up by number, which the host does not.
fn arg_str(args: &[Operand]) -> Result<String, ExprError> {
    match args.first() {
        Some(Operand::Str(v)) => Ok(v.clone()),
        Some(Operand::Float(_)) => Err(ExprError::Host("a float names nothing")),
        Some(Operand::Int(_)) => Err(ExprError::Host("a name given as a number")),
        None => Err(ExprError::StackUnderflow),
    }
}

fn call(op: i32, stack: &mut Vec<Value>, host: &dyn GameQueries) -> Result<Operand, ExprError> {
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
        T5_FN_ADSJAVELIN => Operand::Int(i32::from(host.ads_javelin()?)),
        T5_FN_KEYBINDING => host.key_binding(&arg_str(&args()?)?)?,
        T5_FN_MENUISOPEN => Operand::Int(host.menu_is_open(&arg_str(&args()?)?)?),
        T5_FN_INPRIVATEPARTY
        | T5_FN_PRIVATEPARTYHOST
        | T5_FN_PRIVATEPARTYHOSTINLOBBY
        | T5_FN_GAMEHOST => return Err(ExprError::Unknown(PARTY)),
        // One local client, never split; the client sets local client 0
        // primary at startup, and its profile is always signed in.
        T5_FN_ISSPLITSCREEN => Operand::Int(0),
        T5_FN_ISSPLITSCREENHOST => Operand::Int(1),
        T5_FN_ISPROFILESIGNEDIN => Operand::Int(1),
        T5_FN_GETLOCALCLIENTNUM => Operand::Int(0),
        T5_FN_MIN | T5_FN_MAX => {
            let args = args()?;
            let [a, b] = args.as_slice() else {
                return Err(ExprError::Host("min and max take two arguments"));
            };
            let less = compare(a.clone(), b.clone(), |a, b| a < b, |a, b| a < b);
            let pick_a = if op == T5_FN_MIN { less } else { !less };
            let pick = if pick_a { a } else { b };
            match (a, b) {
                (Operand::Int(_), Operand::Int(_)) => pick.clone(),
                _ => Operand::Float(source_float(pick)),
            }
        }
        // `select(condition, then, else)`.
        T5_FN_SELECT => {
            let args = args()?;
            let [condition, then, otherwise] = args.as_slice() else {
                return Err(ExprError::Host("select takes three arguments"));
            };
            if source_float(condition) != 0.0 {
                then.clone()
            } else {
                otherwise.clone()
            }
        }
        T5_FN_TOUPPER => Operand::Str(
            source_string(args()?.first().ok_or(ExprError::StackUnderflow)?).to_ascii_uppercase(),
        ),
        T5_FN_TEAM => host.team_field(&arg_str(&args()?)?)?,
        T5_FN_OTHERTEAM => host.other_team_field(&arg_str(&args()?)?)?,
        T5_FN_TIMELEFT => Operand::Int(host.time_left()?),
        T5_FN_SECONDSASCOUNTDOWN => {
            let args = args()?;
            let (ms, tenths) = match args.as_slice() {
                [ms] => (ms, false),
                [ms, tenths] => (ms, source_float(tenths) != 0.0),
                _ => {
                    return Err(ExprError::Host(
                        "secondsAsCountdown takes one or two arguments",
                    ));
                }
            };
            Operand::Str(countdown(source_float(ms) as i32, tenths))
        }
        T5_FN_GAMETYPENAME => host.gametype_name()?,
        T5_FN_SCOREATRANK => Operand::Int(host.score_at_rank(source_int(
            args()?.first().ok_or(ExprError::StackUnderflow)?,
        ))?),
        T5_FN_SPECTATINGCLIENT => Operand::Int(host.spectating_client()?),
        T5_FN_ISINTERMISSION => Operand::Int(host.intermission()?),
        T5_FN_ISVISIBILITYBITSET => Operand::Int(host.visibility_bit(source_int(
            args()?.first().ok_or(ExprError::StackUnderflow)?,
        ))?),
        T5_FN_LOCSTRING => Operand::Str(host.localize(&args()?)?),
        T5_FN_LOCALVARINT => Operand::Int(host.local_var_int(&arg_str(&args()?)?)?),
        T5_FN_LOCALVARBOOL => {
            Operand::Int(i32::from(host.local_var_int(&arg_str(&args()?)?)? != 0))
        }
        T5_FN_LOCALVARSTRING => Operand::Str(host.local_var_string(&arg_str(&args()?)?)?),
        // A float truncates, a string is `atoi`.
        T5_FN_INT => Operand::Int(source_int(
            args()?.first().ok_or(ExprError::StackUnderflow)?,
        )),
        T5_FN_GETTEXTWIDTH => {
            let args = args()?;
            let [text, font, scale] = args.as_slice() else {
                return Err(ExprError::Host("getTextWidth takes three arguments"));
            };
            Operand::Int(host.text_width(
                &source_string(text),
                source_int(font),
                source_float(scale),
            )?)
        }
        // The item's menu has the focus and is visible.
        T5_FN_MENUHASFOCUS => Operand::Int(host.menu_has_focus()?),
        // `isInGame`: a client is in a level, no open menu is a full-screen
        // one and the level is no menu level (`menu_`). The front end
        // answers 0 too, as Black Ops behaves there (`t5.hud.front_end_in_game`).
        T5_FN_ISINGAME => {
            let map = host.dvar_string("mapname")?;
            let menu_level = map.starts_with("menu_") || map == crate::FRONT_END_LEVEL;
            Operand::Int(i32::from(
                host.in_game()? != 0 && host.fullscreen_menu_open()? == 0 && !menu_level,
            ))
        }
        other => return Err(ExprError::UnsupportedOp(other)),
    })
}

fn evaluate(tokens: &[T5Token], host: &dyn GameQueries) -> Result<Operand, ExprError> {
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
            // `!` and `~` on a string are errors that leave 0.
            T5_NOT => Operand::Int(match pop_operand(&mut stack)? {
                Operand::Int(v) => i32::from(v == 0),
                Operand::Float(v) => i32::from(v == 0.0),
                Operand::Str(_) => 0,
            }),
            T5_BITNEG => Operand::Int(match pop_operand(&mut stack)? {
                Operand::Str(_) => 0,
                other => !source_int(&other),
            }),
            T5_MUL | T5_DIV | T5_MOD | T5_PLUS | T5_MINUS | T5_SMALLER..=T5_LOGOR => {
                let b = pop_operand(&mut stack)?;
                let a = pop_operand(&mut stack)?;
                binary(op, a, b)?
            }
            T5_BITAND | T5_BITOR | T5_SHIFTLEFT | T5_SHIFTRIGHT => {
                let b = source_int(&pop_operand(&mut stack)?);
                let a = source_int(&pop_operand(&mut stack)?);
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

#[derive(Debug)]
struct Program(Vec<T5Token>);

impl MenuProgram for Program {
    fn evaluate(&self, queries: &dyn GameQueries) -> Result<Operand, ExprError> {
        evaluate(&self.0, queries)
    }
    fn truth(&self, value: &Operand) -> Result<bool, ExprError> {
        truth(value)
    }
    fn number(&self, value: &Operand) -> Result<f32, ExprError> {
        number(value)
    }
    fn text(&self, value: &Operand) -> Result<String, ExprError> {
        text(value)
    }
}

/// A Black Ops menu expression from its catalog (the tokens after `t5`).
pub fn parse_menu_expression(tokens: &[&str]) -> Result<Box<dyn MenuProgram>, ExprError> {
    Ok(Box::new(Program(parse(tokens)?)))
}
