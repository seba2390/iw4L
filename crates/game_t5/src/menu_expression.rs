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
const T5_FN_MILLISECONDS: i32 = 28;
const T5_FN_DVARINT: i32 = 30;
const T5_FN_DVARBOOL: i32 = 31;
const T5_FN_DVARSTRING: i32 = 33;
const T5_FN_UI_ACTIVE: i32 = 34;
const T5_FN_FLASHBANGED: i32 = 35;
const T5_FN_INKILLCAM: i32 = 38;
const T5_FN_ISDUALWIELD: i32 = 39;
const T5_FN_ISFUELWEAPON: i32 = 40;
const T5_FN_PLAYER: i32 = 41;
const T5_FN_GETATTACHMENTIMAGE: i32 = 279;
const T5_FN_GETCLANNAME: i32 = 141;
const T5_FN_GETCURRENTITEMATTACHMENTCOST: i32 = 254;
const T5_FN_GETCURRENTITEMATTACHMENTDESC: i32 = 258;
const T5_FN_GETCURRENTITEMATTACHMENTNAME: i32 = 257;
const T5_FN_GETCURRENTITEMATTACHMENTNUM: i32 = 262;
const T5_FN_GETCURRENTITEMATTACHMENTPOINT: i32 = 263;
const T5_FN_GETCURRENTITEMCOST: i32 = 252;
const T5_FN_GETCURRENTITEMNUMATTACHMENTS: i32 = 259;
const T5_FN_GETCURRENTITEMOPTION: i32 = 288;
const T5_FN_GETFLOATASFORMATTEDSTRING: i32 = 158;
const T5_FN_GETITEMCOST: i32 = 120;
const T5_FN_GETITEMCOUNT: i32 = 123;
const T5_FN_GETITEMDESC: i32 = 116;
const T5_FN_GETITEMGROUP: i32 = 124;
const T5_FN_GETITEMNUMATTACHMENTSEQUIPPED: i32 = 260;
const T5_FN_GETITEMOPTIONBYNAME: i32 = 382;
const T5_FN_GETITEMREF: i32 = 111;
const T5_FN_GETSELFGAMERTAG: i32 = 131;
const T5_FN_GETWEAPONOPTIONCOST: i32 = 289;
const T5_FN_GETWEAPONOPTIONGROUPINDEX: i32 = 330;
const T5_FN_GETWEAPONOPTIONIMAGE: i32 = 281;
const T5_FN_GETWEAPONOPTIONNAME: i32 = 282;
const T5_FN_GETWEAPONOPTIONUNLOCKLVL: i32 = 446;
const T5_FN_GETWEAPONOPTIONUNLOCKPLEVEL: i32 = 447;
const T5_FN_GETXUID: i32 = 130;
const T5_FN_ISCURRENTITEMATTACHMENTPURCHASED: i32 = 249;
const T5_FN_ISCURRENTITEMCLASSIFIED: i32 = 338;
const T5_FN_ISCURRENTITEMEQUIPPEDINANYCUSTOMCLASS: i32 = 335;
const T5_FN_ISCURRENTITEMOPTIONPURCHASED: i32 = 250;
const T5_FN_ISCURRENTITEMPURCHASED: i32 = 248;
const T5_FN_ISCURRENTSORTEDITEMEQUIPPED: i32 = 334;
const T5_FN_ISITEMOPTIONPURCHASEDBYNAME: i32 = 381;
const T5_FN_ISPROITEMVERSION: i32 = 414;
const T5_FN_ISPROITEMVERSIONUNLOCKED: i32 = 413;
const T5_FN_GETPROITEMVERSIONCOST: i32 = 433;
const T5_FN_ISITEMEQUIPPED: i32 = 336;
const T5_FN_ISATTACHMENTALLOWEDONITEMINDEX: i32 = 437;
const T5_FN_GETNUMSORTEDITEMSEQUIPPED: i32 = 434;
const T5_FN_GETFEEDERDATA: i32 = 147;
const T5_FN_GETFEEDERCOUNT: i32 = 337;
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
const T5_FN_FLOAT: i32 = 74;
const T5_FN_GAMETYPENAME: i32 = 75;
const T5_FN_SCOREATRANK: i32 = 78;
const T5_FN_SPECTATINGCLIENT: i32 = 80;
const T5_FN_KEYBINDING: i32 = 81;
const T5_FN_GAMEHOST: i32 = 86;
const T5_FN_ISVISIBILITYBITSET: i32 = 87;
const T5_FN_ISSPLITSCREEN: i32 = 88;
const T5_FN_ISSPLITSCREENHOST: i32 = 89;
const T5_FN_TABLELOOKUP: i32 = 61;
const T5_FN_HASINVITES: i32 = 100;
const T5_FN_ISITEMLOCKED: i32 = 107;
const T5_FN_ISITEMNEW: i32 = 110;
const T5_FN_GETITEMNAME: i32 = 112;
const T5_FN_GETITEMIMAGE: i32 = 113;
const T5_FN_GETITEMUNLOCKLEVEL: i32 = 114;
const T5_FN_GETITEMUNLOCKPLEVEL: i32 = 115;
const T5_FN_GETITEMATTACHMENT: i32 = 155;
const T5_FN_GETCURRENTITEMNAME: i32 = 255;
const T5_FN_GETCURRENTITEMINDEX: i32 = 256;
const T5_FN_GETITEMINDEX: i32 = 117;
const T5_FN_GETSTATBYNAME: i32 = 125;
const T5_FN_GETDSTAT: i32 = 221;
const T5_FN_GETITEMEQUIPPEDATTACHMENT: i32 = 261;
const T5_FN_ISPARTYREADY: i32 = 274;
const T5_FN_GETATTACHMENTNAME: i32 = 278;
const T5_FN_ISITEMGROUPNEW: i32 = 290;
const T5_FN_GETCACITEMINDEX: i32 = 313;
const T5_FN_ISINTERMISSION: i32 = 104;
const T5_FN_GETTEXTWIDTH: i32 = 204;
const T5_FN_ISPROFILESIGNEDIN: i32 = 209;
const T5_FN_GETNUMLIVES: i32 = 214;
const T5_FN_PLAYERSALIVE: i32 = 215;
const T5_FN_GETTIME: i32 = 217;
const T5_FN_SELECT: i32 = 291;
const T5_FN_ISSIGNEDINTOLIVE: i32 = 319;
const T5_FN_GETLOCALCLIENTNUM: i32 = 321;
const T5_FN_ISDEMONWAREFETCHINGDONE: i32 = 392;
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
        // The UI's clock: Black Ops' `Sys_Milliseconds`.
        T5_FN_MILLISECONDS => Operand::Int(host.milliseconds()?),
        // The count of pending invites; IW4L's private matches send none.
        T5_FN_HASINVITES => Operand::Int(0),
        // A Live sign-in and `dw_active` (registered 1): the player runs
        // the game they own, so the profile is signed in as it is above.
        T5_FN_ISSIGNEDINTOLIVE => Operand::Int(1),
        // Whether the profile's stats have been fetched: IW4L keeps them in
        // the profile on disk, read before any menu.
        T5_FN_ISDEMONWAREFETCHINGDONE => Operand::Int(1),
        // Its handler answers 0, always.
        T5_FN_ISPARTYREADY => Operand::Int(0),
        // `getDStat(path…)` on the local client's stats.
        T5_FN_GETDSTAT => host.profile_stat(&args()?)?,
        // `getStatByName(name)`: `PlayerStatsList.<name>` of the same stats.
        T5_FN_GETSTATBYNAME => {
            let name = arg_str(&args()?)?;
            host.profile_stat(&[Operand::Str("PlayerStatsList".into()), Operand::Str(name)])?
        }
        T5_FN_GETITEMINDEX => Operand::Int(host.item_index(&arg_str(&args()?)?)?),
        T5_FN_GETCACITEMINDEX => cac_item_index(&args()?, host)?,
        T5_FN_ISITEMLOCKED => host.profile_function("isitemlocked", &args()?)?,
        T5_FN_ISITEMNEW => host.profile_function("isitemnew", &args()?)?,
        T5_FN_GETITEMNAME => host.profile_function("getitemname", &args()?)?,
        T5_FN_GETITEMIMAGE => host.profile_function("getitemimage", &args()?)?,
        T5_FN_GETITEMUNLOCKLEVEL => host.profile_function("getitemunlocklevel", &args()?)?,
        T5_FN_GETITEMUNLOCKPLEVEL => host.profile_function("getitemunlockplevel", &args()?)?,
        T5_FN_GETITEMATTACHMENT => host.profile_function("getitemattachment", &args()?)?,
        // The create-a-class menus' current item, and its name.
        T5_FN_GETCURRENTITEMINDEX => Operand::Int(host.current_item()?),
        T5_FN_GETCURRENTITEMNAME => {
            host.profile_function("getitemname", &[Operand::Int(host.current_item()?)])?
        }
        T5_FN_GETATTACHMENTNAME => host.profile_function("getattachmentname", &args()?)?,
        T5_FN_ISITEMGROUPNEW => host.profile_function("isitemgroupnew", &args()?)?,
        T5_FN_GETITEMEQUIPPEDATTACHMENT => {
            host.profile_function("getitemequippedattachment", &args()?)?
        }
        // `tableLookup(table, column, value, result column)`, answered while
        // `useFastFile` (registered 1) holds.
        T5_FN_TABLELOOKUP => {
            let args = args()?;
            let [table, column, value, result] = args.as_slice() else {
                return Ok(Operand::Str(String::new()));
            };
            Operand::Str(host.table_lookup(
                &source_string(table),
                source_int(column),
                &source_string(value),
                source_int(result),
            )?)
        }
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
        T5_FN_GETFEEDERCOUNT => Operand::Int(host.feeder_count(&arg_str(&args()?)?)?),
        // `getFeederData(field)`, or with the item's name first; a third
        // form names the menu too.
        T5_FN_GETFEEDERDATA => {
            let args = args()?;
            let field = match args.as_slice() {
                [field] => host.feeder_field(None, &source_string(field))?,
                [item, field] => {
                    host.feeder_field(Some(&source_string(item)), &source_string(field))?
                }
                [_, item, field] => {
                    host.feeder_field(Some(&source_string(item)), &source_string(field))?
                }
                _ => {
                    diag::warn!(
                        Ui,
                        "UI Expression Error: Expected at least 1 parameter to getFeederData()"
                    );
                    0
                }
            };
            Operand::Int(field)
        }
        T5_FN_TEAM => host.team_field(&arg_str(&args()?)?)?,
        T5_FN_OTHERTEAM => host.other_team_field(&arg_str(&args()?)?)?,
        T5_FN_TIMELEFT => Operand::Int(host.time_left()?),
        T5_FN_GETNUMLIVES => Operand::Int(host.num_lives()?),
        T5_FN_PLAYERSALIVE => Operand::Int(host.players_alive(&arg_str(&args()?)?)?),
        T5_FN_GETTIME => Operand::Int(host.server_time()?),
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
        // `float(x)`: a number as a float, a string by `atof`.
        T5_FN_FLOAT => Operand::Float(source_float(
            args()?.first().ok_or(ExprError::StackUnderflow)?,
        )),
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
        // answers 0 too, as Black Ops behaves there (`t5.hud.front_end_in_game`),
        // and so does the multiplayer front end's level.
        T5_FN_ISINGAME => {
            let map = host.dvar_string("mapname")?;
            let menu_level = map.starts_with("menu_")
                || map == crate::FRONT_END_LEVEL
                || map == crate::MP_UI_LEVEL;
            Operand::Int(i32::from(
                host.in_game()? != 0 && host.fullscreen_menu_open()? == 0 && !menu_level,
            ))
        }
        // The local profile: IW4L keeps no Live identity or clan.
        T5_FN_GETSELFGAMERTAG => Operand::Str(host.dvar_string("name")?),
        T5_FN_GETXUID => Operand::Str("0".to_owned()),
        T5_FN_GETCLANNAME => Operand::Str(String::new()),
        // `getFloatAsFormattedString(value, places)`.
        T5_FN_GETFLOATASFORMATTEDSTRING => {
            let args = args()?;
            let [value, places] = args.as_slice() else {
                diag::warn!(
                    Ui,
                    "UI Expression Error: Expected 2 parameters to GetFloatAsFormattedString()"
                );
                return Ok(Operand::Str(String::new()));
            };
            Operand::Str(format!(
                "{:.*}",
                source_int(places).max(0) as usize,
                source_float(value)
            ))
        }
        other => {
            if let Some(name) = profile_function_name(other) {
                host.profile_function(name, &args()?)?
            } else if let Some(name) = current_item_function_name(other) {
                host.profile_function(name, &[])?
            } else {
                return Err(ExprError::UnsupportedOp(other));
            }
        }
    })
}

/// The functions the create-a-class menus answer from the local profile and
/// their current item, by the name the host knows them under.
fn profile_function_name(op: i32) -> Option<&'static str> {
    Some(match op {
        T5_FN_GETITEMREF => "getitemref",
        T5_FN_GETITEMDESC => "getitemdesc",
        T5_FN_GETITEMCOST => "getitemcost",
        T5_FN_GETITEMCOUNT => "getitemcount",
        T5_FN_GETITEMGROUP => "getitemgroup",
        T5_FN_GETATTACHMENTIMAGE => "getattachmentimage",
        T5_FN_GETWEAPONOPTIONIMAGE => "getweaponoptionimage",
        T5_FN_GETWEAPONOPTIONNAME => "getweaponoptionname",
        T5_FN_GETWEAPONOPTIONCOST => "getweaponoptioncost",
        T5_FN_GETWEAPONOPTIONUNLOCKLVL => "getweaponoptionunlocklvl",
        T5_FN_GETWEAPONOPTIONUNLOCKPLEVEL => "getweaponoptionunlockplevel",
        T5_FN_GETWEAPONOPTIONGROUPINDEX => "getweaponoptiongroupindex",
        T5_FN_GETITEMOPTIONBYNAME => "getitemoptionbyname",
        T5_FN_GETITEMNUMATTACHMENTSEQUIPPED => "getitemnumattachmentsequipped",
        T5_FN_ISITEMOPTIONPURCHASEDBYNAME => "isitemoptionpurchasedbyname",
        T5_FN_ISPROITEMVERSION => "isproitemversion",
        T5_FN_ISPROITEMVERSIONUNLOCKED => "isproitemversionunlocked",
        T5_FN_GETPROITEMVERSIONCOST => "getproitemversioncost",
        T5_FN_ISITEMEQUIPPED => "isitemequipped",
        T5_FN_ISATTACHMENTALLOWEDONITEMINDEX => "isattachmentallowedonitemindex",
        _ => return None,
    })
}

/// The same for the functions of the current item, which take no arguments.
fn current_item_function_name(op: i32) -> Option<&'static str> {
    Some(match op {
        T5_FN_ISCURRENTITEMPURCHASED => "iscurrentitempurchased",
        T5_FN_ISCURRENTITEMATTACHMENTPURCHASED => "iscurrentitemattachmentpurchased",
        T5_FN_ISCURRENTITEMOPTIONPURCHASED => "iscurrentitemoptionpurchased",
        T5_FN_GETCURRENTITEMCOST => "getcurrentitemcost",
        T5_FN_GETCURRENTITEMNUMATTACHMENTS => "getcurrentitemnumattachments",
        T5_FN_GETCURRENTITEMATTACHMENTNAME => "getcurrentitemattachmentname",
        T5_FN_GETCURRENTITEMATTACHMENTDESC => "getcurrentitemattachmentdesc",
        T5_FN_GETCURRENTITEMATTACHMENTCOST => "getcurrentitemattachmentcost",
        T5_FN_GETCURRENTITEMATTACHMENTNUM => "getcurrentitemattachmentnum",
        T5_FN_GETCURRENTITEMATTACHMENTPOINT => "getcurrentitemattachmentpoint",
        T5_FN_GETCURRENTITEMOPTION => "getcurrentitemoption",
        T5_FN_ISCURRENTITEMCLASSIFIED => "iscurrentitemclassified",
        T5_FN_ISCURRENTITEMEQUIPPEDINANYCUSTOMCLASS => "iscurrentitemequippedinanycustomclass",
        T5_FN_ISCURRENTSORTEDITEMEQUIPPED => "iscurrentsorteditemequipped",
        T5_FN_GETNUMSORTEDITEMSEQUIPPED => "getnumsorteditemsequipped",
        _ => return None,
    })
}

/// `getCACItemIndex(class, slot)`: the class's item in that slot, read from
/// `cacLoadouts` (`cacLoadouts` itself as the class names the global slots).
/// A killstreak slot under `custom_killstreak_mode` 2 reads the custom
/// killstreak dvar, and `ui_useCustomClassInfo` the custom gametype's class
/// table, which is not ported.
fn cac_item_index(args: &[Operand], host: &dyn GameQueries) -> Result<Operand, ExprError> {
    let [class, slot] = args else {
        return Ok(Operand::Int(0));
    };
    let (class, slot) = (source_string(class), source_string(slot));
    let killstreak = slot.contains("killstreak");
    if killstreak && host.dvar_int("custom_killstreak_mode")? == 2 {
        let last = slot.chars().last().unwrap_or('0');
        return Ok(Operand::Int(
            host.dvar_int(&format!("custom_killstreak_{last}"))?,
        ));
    }
    if !killstreak && host.dvar_int("ui_useCustomClassInfo")? != 0 {
        return Err(ExprError::Unknown(CUSTOM_CLASS_INFO));
    }
    let path = if class.eq_ignore_ascii_case("cacLoadouts") {
        vec![Operand::Str(class), Operand::Str(slot)]
    } else {
        vec![
            Operand::Str("cacLoadouts".into()),
            Operand::Str(class),
            Operand::Str(slot),
        ]
    };
    host.profile_stat(&path)
}

/// The custom gametype's class table (`ui_useCustomClassInfo`).
const CUSTOM_CLASS_INFO: &str = game_api::unknown!(
    "t5.hud.custom_class_info",
    "with `ui_useCustomClassInfo` set, the create-a-class menus read the custom game mode's class table, which is not read",
    "Black Ops' custom gametype class table (BlackOpsMP.exe)"
)
.id;

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
pub(crate) struct Program(pub(crate) Vec<T5Token>);

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
