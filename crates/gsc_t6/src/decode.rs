//! Operand layouts of Black Ops 2's opcodes, read from how each opcode's
//! handler advances the instruction pointer.

use core::ops::Range;

use crate::{Module, OPCODE_LIMIT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    None,
    U8,
    /// Aligned to 2.
    U16,
    /// Aligned to 4.
    U32,
    /// Three floats, aligned to 4.
    Vector,
    /// A parameter count, then the callee's address aligned to 4.
    Call,
    /// A count, then that many local names (each aligned to 2).
    Locals,
    /// A case count aligned to 4, then eight bytes per case.
    EndSwitch,
    /// Two words, each aligned to 4.
    TwoU32,
}

pub fn operand(opcode: u8) -> Option<Operand> {
    use Operand as O;
    Some(match opcode {
        0x04 | 0x05 | 0x18 | 0x19 | 0x1b | 0x23 | 0x24 | 0x27 | 0x2f | 0x31 | 0x33 | 0x35
        | 0x5e | 0x79 => O::U8,
        0x06 | 0x07 | 0x0a | 0x0b | 0x16 | 0x20 | 0x21 | 0x22 | 0x3b | 0x3c | 0x3d | 0x3e
        | 0x3f | 0x78 | 0x7a | 0x7b => O::U16,
        0x08 | 0x09 | 0x13 | 0x15 | 0x59 | 0x5c | 0x72 => O::U32,
        0x0c => O::Vector,
        0x29 | 0x2a | 0x2e | 0x30 | 0x32 | 0x34 => O::Call,
        0x17 => O::Locals,
        0x5a => O::EndSwitch,
        0x77 => O::TwoU32,
        _ if opcode < OPCODE_LIMIT => O::None,
        _ => return None,
    })
}

/// The final opcodes a function may end with.
const END: u8 = Opcode::End as u8;
const RETURN: u8 = Opcode::Return as u8;

/// Every opcode the interpreter dispatches, by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    End = 0x00,
    Return = 0x01,
    GetUndefined = 0x02,
    GetZero = 0x03,
    GetByte = 0x04,
    GetNegByte = 0x05,
    GetUShort = 0x06,
    GetNegUShort = 0x07,
    GetInteger = 0x08,
    GetFloat = 0x09,
    GetString = 0x0a,
    GetIString = 0x0b,
    GetVector = 0x0c,
    GetLevelObject = 0x0d,
    GetAnimObject = 0x0e,
    GetSelf = 0x0f,
    GetLevel = 0x10,
    GetGame = 0x11,
    GetAnim = 0x12,
    GetAnimation = 0x13,
    GetGameRef = 0x14,
    GetFunction = 0x15,
    CreateLocalVariable = 0x16,
    SafeCreateLocalVariables = 0x17,
    RemoveLocalVariables = 0x18,
    EvalLocalVariableCached = 0x19,
    EvalArray = 0x1a,
    EvalLocalArrayRefCached = 0x1b,
    EvalArrayRef = 0x1c,
    ClearArray = 0x1d,
    GetEmptyArray = 0x1e,
    GetSelfObject = 0x1f,
    EvalFieldVariable = 0x20,
    EvalFieldVariableRef = 0x21,
    ClearFieldVariable = 0x22,
    SafeSetVariableFieldCached = 0x23,
    SafeSetWaittillVariableFieldCached = 0x24,
    ClearParams = 0x25,
    CheckClearParams = 0x26,
    EvalLocalVariableRefCached = 0x27,
    SetVariableField = 0x28,
    CallBuiltin = 0x29,
    CallBuiltinMethod = 0x2a,
    Wait = 0x2b,
    WaitTillFrameEnd = 0x2c,
    PreScriptCall = 0x2d,
    ScriptFunctionCall = 0x2e,
    ScriptFunctionCallPointer = 0x2f,
    ScriptMethodCall = 0x30,
    ScriptMethodCallPointer = 0x31,
    ScriptThreadCall = 0x32,
    ScriptThreadCallPointer = 0x33,
    ScriptMethodThreadCall = 0x34,
    ScriptMethodThreadCallPointer = 0x35,
    DecTop = 0x36,
    CastFieldObject = 0x37,
    CastBool = 0x38,
    BoolNot = 0x39,
    BoolComplement = 0x3a,
    JumpOnFalse = 0x3b,
    JumpOnTrue = 0x3c,
    JumpOnFalseExpr = 0x3d,
    JumpOnTrueExpr = 0x3e,
    Jump = 0x3f,
    /// The fetch itself: does nothing.
    Nop = 0x40,
    Inc = 0x41,
    Dec = 0x42,
    BitOr = 0x43,
    BitXor = 0x44,
    BitAnd = 0x45,
    Equal = 0x46,
    NotEqual = 0x47,
    Less = 0x48,
    Greater = 0x49,
    LessEqual = 0x4a,
    GreaterEqual = 0x4b,
    ShiftLeft = 0x4c,
    ShiftRight = 0x4d,
    Plus = 0x4e,
    Minus = 0x4f,
    Multiply = 0x50,
    Divide = 0x51,
    Modulus = 0x52,
    SizeOf = 0x53,
    WaitTillMatch = 0x54,
    /// Same handler as `WaitTillMatch`.
    WaitTill = 0x55,
    Notify = 0x56,
    EndOn = 0x57,
    /// Same handler as `PreScriptCall`.
    VoidCodePos = 0x58,
    Switch = 0x59,
    EndSwitch = 0x5a,
    Vector = 0x5b,
    GetHash = 0x5c,
    RealWait = 0x5d,
    VectorConstant = 0x5e,
    IsDefined = 0x5f,
    VectorScale = 0x60,
    AnglesToUp = 0x61,
    AnglesToRight = 0x62,
    AnglesToForward = 0x63,
    AngleClamp180 = 0x64,
    VectorToAngles = 0x65,
    Abs = 0x66,
    GetTime = 0x67,
    GetDvar = 0x68,
    GetDvarInt = 0x69,
    GetDvarFloat = 0x6a,
    GetDvarVector = 0x6b,
    GetDvarColorRed = 0x6c,
    GetDvarColorGreen = 0x6d,
    GetDvarColorBlue = 0x6e,
    GetDvarColorAlpha = 0x6f,
    FirstArrayKey = 0x70,
    NextArrayKey = 0x71,
    ProfileStart = 0x72,
    /// Same handler as `GetUndefined`.
    ProfileStop = 0x73,
    SafeDecTop = 0x74,
    /// Same handler as `Nop`.
    Nop2 = 0x75,
    Abort = 0x76,
    Object = 0x77,
    ThreadObject = 0x78,
    EvalLocalVariable = 0x79,
    EvalLocalVariableRef = 0x7a,
    /// Same handler as `Jump`: developer blocks are jumped over.
    DevblockBegin = 0x7b,
}

const BY_VALUE: [Opcode; OPCODE_LIMIT as usize] = [
    Opcode::End,
    Opcode::Return,
    Opcode::GetUndefined,
    Opcode::GetZero,
    Opcode::GetByte,
    Opcode::GetNegByte,
    Opcode::GetUShort,
    Opcode::GetNegUShort,
    Opcode::GetInteger,
    Opcode::GetFloat,
    Opcode::GetString,
    Opcode::GetIString,
    Opcode::GetVector,
    Opcode::GetLevelObject,
    Opcode::GetAnimObject,
    Opcode::GetSelf,
    Opcode::GetLevel,
    Opcode::GetGame,
    Opcode::GetAnim,
    Opcode::GetAnimation,
    Opcode::GetGameRef,
    Opcode::GetFunction,
    Opcode::CreateLocalVariable,
    Opcode::SafeCreateLocalVariables,
    Opcode::RemoveLocalVariables,
    Opcode::EvalLocalVariableCached,
    Opcode::EvalArray,
    Opcode::EvalLocalArrayRefCached,
    Opcode::EvalArrayRef,
    Opcode::ClearArray,
    Opcode::GetEmptyArray,
    Opcode::GetSelfObject,
    Opcode::EvalFieldVariable,
    Opcode::EvalFieldVariableRef,
    Opcode::ClearFieldVariable,
    Opcode::SafeSetVariableFieldCached,
    Opcode::SafeSetWaittillVariableFieldCached,
    Opcode::ClearParams,
    Opcode::CheckClearParams,
    Opcode::EvalLocalVariableRefCached,
    Opcode::SetVariableField,
    Opcode::CallBuiltin,
    Opcode::CallBuiltinMethod,
    Opcode::Wait,
    Opcode::WaitTillFrameEnd,
    Opcode::PreScriptCall,
    Opcode::ScriptFunctionCall,
    Opcode::ScriptFunctionCallPointer,
    Opcode::ScriptMethodCall,
    Opcode::ScriptMethodCallPointer,
    Opcode::ScriptThreadCall,
    Opcode::ScriptThreadCallPointer,
    Opcode::ScriptMethodThreadCall,
    Opcode::ScriptMethodThreadCallPointer,
    Opcode::DecTop,
    Opcode::CastFieldObject,
    Opcode::CastBool,
    Opcode::BoolNot,
    Opcode::BoolComplement,
    Opcode::JumpOnFalse,
    Opcode::JumpOnTrue,
    Opcode::JumpOnFalseExpr,
    Opcode::JumpOnTrueExpr,
    Opcode::Jump,
    Opcode::Nop,
    Opcode::Inc,
    Opcode::Dec,
    Opcode::BitOr,
    Opcode::BitXor,
    Opcode::BitAnd,
    Opcode::Equal,
    Opcode::NotEqual,
    Opcode::Less,
    Opcode::Greater,
    Opcode::LessEqual,
    Opcode::GreaterEqual,
    Opcode::ShiftLeft,
    Opcode::ShiftRight,
    Opcode::Plus,
    Opcode::Minus,
    Opcode::Multiply,
    Opcode::Divide,
    Opcode::Modulus,
    Opcode::SizeOf,
    Opcode::WaitTillMatch,
    Opcode::WaitTill,
    Opcode::Notify,
    Opcode::EndOn,
    Opcode::VoidCodePos,
    Opcode::Switch,
    Opcode::EndSwitch,
    Opcode::Vector,
    Opcode::GetHash,
    Opcode::RealWait,
    Opcode::VectorConstant,
    Opcode::IsDefined,
    Opcode::VectorScale,
    Opcode::AnglesToUp,
    Opcode::AnglesToRight,
    Opcode::AnglesToForward,
    Opcode::AngleClamp180,
    Opcode::VectorToAngles,
    Opcode::Abs,
    Opcode::GetTime,
    Opcode::GetDvar,
    Opcode::GetDvarInt,
    Opcode::GetDvarFloat,
    Opcode::GetDvarVector,
    Opcode::GetDvarColorRed,
    Opcode::GetDvarColorGreen,
    Opcode::GetDvarColorBlue,
    Opcode::GetDvarColorAlpha,
    Opcode::FirstArrayKey,
    Opcode::NextArrayKey,
    Opcode::ProfileStart,
    Opcode::ProfileStop,
    Opcode::SafeDecTop,
    Opcode::Nop2,
    Opcode::Abort,
    Opcode::Object,
    Opcode::ThreadObject,
    Opcode::EvalLocalVariable,
    Opcode::EvalLocalVariableRef,
    Opcode::DevblockBegin,
];

impl Opcode {
    pub fn from_byte(value: u8) -> Option<Self> {
        BY_VALUE.get(usize::from(value)).copied()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub at: usize,
    pub opcode: u8,
    pub operand: Range<usize>,
}

impl Instruction {
    /// Where a jump lands: a signed 16-bit offset from the end of its operand.
    pub fn jump_target(&self, bytes: &[u8]) -> Option<isize> {
        use Opcode as O;
        let jumps = matches!(
            Opcode::from_byte(self.opcode)?,
            O::JumpOnFalse
                | O::JumpOnTrue
                | O::JumpOnFalseExpr
                | O::JumpOnTrueExpr
                | O::Jump
                | O::DevblockBegin
        );
        let offset = bytes.get(self.operand.end - 2..self.operand.end)?;
        jumps.then(|| {
            self.operand.end as isize + isize::from(i16::from_le_bytes([offset[0], offset[1]]))
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownOpcode {
        at: usize,
        value: u8,
    },
    /// An operand runs past the function's end.
    Overrun {
        at: usize,
    },
    /// The code reaches neither the next function nor an End or Return
    /// followed by alignment to 4 and four bytes that end the function.
    Unterminated {
        end: usize,
    },
    /// A jump lands outside its function or inside another instruction.
    JumpTarget {
        at: usize,
        target: isize,
    },
}

/// Decodes one function's code, `start..end`.
pub fn decode_function(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Result<Vec<Instruction>, DecodeError> {
    let align2 = |at: usize| (at + 1) & !1;
    let align4 = |at: usize| (at + 3) & !3;
    let mut out = Vec::new();
    // The function ends at its last End or Return followed by alignment to 4
    // and four bytes; the bytes after it are padding, not code.
    let mut ends = None;
    let mut at = start;
    let stopped = loop {
        if at == end {
            break None;
        }
        let value = bytes[at];
        let Some(layout) = operand(value) else {
            break Some(DecodeError::UnknownOpcode { at, value });
        };
        let from = at + 1;
        let next = match layout {
            Operand::None => from,
            Operand::U8 => from + 1,
            Operand::U16 => align2(from) + 2,
            Operand::U32 => align4(from) + 4,
            Operand::Vector => align4(from) + 12,
            Operand::Call => align4(from + 1) + 4,
            Operand::Locals => match bytes.get(from) {
                Some(&count) => (0..count).fold(from + 1, |name, _| align2(name) + 2),
                None => break Some(DecodeError::Overrun { at }),
            },
            Operand::EndSwitch => {
                let count = align4(from);
                match bytes.get(count..count + 4) {
                    Some(word) => {
                        count + 4 + u32::from_le_bytes(word.try_into().unwrap()) as usize * 8
                    }
                    None => break Some(DecodeError::Overrun { at }),
                }
            }
            Operand::TwoU32 => align4(align4(from) + 4) + 4,
        };
        if next > end {
            break Some(DecodeError::Overrun { at });
        }
        out.push(Instruction {
            at,
            opcode: value,
            operand: from..next,
        });
        if matches!(value, END | RETURN) && align4(from) + 4 == end {
            ends = Some(out.len());
        }
        at = next;
    };
    match (ends, stopped) {
        (Some(count), _) => {
            out.truncate(count);
            jumps_land(bytes, out, start, end)
        }
        (None, None) => jumps_land(bytes, out, start, end),
        (None, Some(error)) => Err(error),
    }
}

fn jumps_land(
    bytes: &[u8],
    code: Vec<Instruction>,
    start: usize,
    end: usize,
) -> Result<Vec<Instruction>, DecodeError> {
    for instruction in &code {
        let Some(target) = instruction.jump_target(bytes) else {
            continue;
        };
        let lands = usize::try_from(target).is_ok_and(|target| {
            (start..=end).contains(&target)
                && (target == end || code.binary_search_by_key(&target, |i| i.at).is_ok())
        });
        if !lands {
            return Err(DecodeError::JumpTarget {
                at: instruction.at,
                target,
            });
        }
    }
    Ok(code)
}

impl Module {
    /// Each export's index with its decoded code, in code order. A function's
    /// code runs to the next function or the end of the code segment.
    pub fn functions(&self, bytes: &[u8]) -> Vec<(usize, Result<Vec<Instruction>, DecodeError>)> {
        let mut order: Vec<usize> = (0..self.exports.len()).collect();
        order.sort_by_key(|&index| self.exports[index].code);
        let ends: Vec<usize> = order
            .iter()
            .skip(1)
            .map(|&index| self.exports[index].code as usize)
            .chain(core::iter::once(self.code.end))
            .collect();
        order
            .into_iter()
            .zip(ends)
            .map(|(index, end)| {
                (
                    index,
                    decode_function(bytes, self.exports[index].code as usize, end),
                )
            })
            .collect()
    }
}
