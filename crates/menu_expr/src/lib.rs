//! Menu expressions as the HUD evaluates them: values, errors, the queries a
//! game's expression asks of the running match, and the programs each game
//! parses from its own menu catalog.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Int(i32),
    Float(f32),
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprError {
    TruncatedDump,
    FunctionBodyMissing,
    UnsupportedOp(i32),
    StackUnderflow,
    StrayOperands,
    EmptyResult,
    Host(&'static str),
    /// The game's rule for this operation is not known (a fidelity ledger id).
    Unknown(&'static str),
}

/// What a game's menu expression may ask of the running match.
pub trait GameQueries {
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError>;
    fn dvar_bool(&self, name: &str) -> Result<i32, ExprError>;
    fn dvar_string(&self, name: &str) -> Result<String, ExprError>;
    fn ui_active(&self) -> Result<i32, ExprError>;
    fn flashbanged(&self) -> Result<i32, ExprError>;
    fn in_killcam(&self) -> Result<i32, ExprError>;
    fn is_dual_wield(&self) -> Result<i32, ExprError>;
    fn is_fuel_weapon(&self) -> Result<i32, ExprError>;
    fn player_field(&self, field: &str) -> Result<Operand, ExprError>;
    fn ads_javelin(&self) -> Result<bool, ExprError>;
    fn key_binding(&self, command: &str) -> Result<Operand, ExprError>;
    fn local_var_int(&self, name: &str) -> Result<i32, ExprError>;
    fn local_var_string(&self, name: &str) -> Result<String, ExprError>;
    fn menu_is_open(&self, name: &str) -> Result<i32, ExprError>;
    /// Whether the menu the expression's item belongs to has the focus.
    fn menu_has_focus(&self) -> Result<i32, ExprError>;
    /// The localized text of a `locstring` argument list.
    fn localize(&self, args: &[Operand]) -> Result<String, ExprError>;
    /// `text`'s width in virtual pixels in font `font` at `scale`.
    fn text_width(&self, text: &str, font: i32, scale: f32) -> Result<i32, ExprError>;
    fn in_game(&self) -> Result<i32, ExprError>;
    /// Whether an open menu is a full-screen one.
    fn fullscreen_menu_open(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("fullscreen menu"))
    }
    /// A field of the local player's team (`name`, `score`).
    fn team_field(&self, field: &str) -> Result<Operand, ExprError> {
        let _ = field;
        Err(ExprError::Host("the host does not answer team"))
    }
    /// A field of the team the local player plays against.
    fn other_team_field(&self, field: &str) -> Result<Operand, ExprError> {
        let _ = field;
        Err(ExprError::Host("the host does not answer otherteam"))
    }
    /// Time left on the match clock, in the game's units.
    fn time_left(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("the host does not answer timeleft"))
    }
    /// The score of the player at `rank` (0 is the leader).
    fn score_at_rank(&self, rank: i32) -> Result<i32, ExprError> {
        let _ = rank;
        Err(ExprError::Host("the host does not answer scoreatrank"))
    }
    /// The localized name of the match's gametype.
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("the host does not answer gametypename"))
    }
    /// The client number the local player follows as a spectator.
    fn spectating_client(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("the host does not answer spectatingclient"))
    }
    /// Whether the match is in its intermission.
    fn intermission(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("the host does not answer isintermission"))
    }
    /// Whether the local client's HUD visibility bit `bit` is set.
    fn visibility_bit(&self, bit: i32) -> Result<i32, ExprError> {
        let _ = bit;
        Err(ExprError::Host(
            "the host does not answer isvisibilitybitset",
        ))
    }
    /// Whether the local player's weapon is an airburst weapon (Black Ops
    /// 2's `isairburstweapon`).
    fn airburst_weapon(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("the host does not answer isairburstweapon"))
    }
}

/// One game's compiled menu expression.
pub trait MenuProgram: Send + Sync + core::fmt::Debug {
    fn evaluate(&self, queries: &dyn GameQueries) -> Result<Operand, ExprError>;
    /// The value as a condition.
    fn truth(&self, value: &Operand) -> Result<bool, ExprError>;
    /// The value where a number is wanted.
    fn number(&self, value: &Operand) -> Result<f32, ExprError>;
    /// The value where text is wanted.
    fn text(&self, value: &Operand) -> Result<String, ExprError>;
}

/// Parses the tokens after a catalog's game tag.
pub type MenuParser = fn(&[&str]) -> Result<Box<dyn MenuProgram>, ExprError>;

/// The parser for each game tag a menu catalog may put before its programs.
#[derive(Clone, Default)]
pub struct MenuParsers(pub Vec<(&'static str, MenuParser)>);

impl MenuParsers {
    pub fn get(&self, tag: &str) -> Option<MenuParser> {
        self.0
            .iter()
            .find(|(known, _)| *known == tag)
            .map(|(_, parser)| *parser)
    }
}
