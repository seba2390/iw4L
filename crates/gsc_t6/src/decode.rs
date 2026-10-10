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
        | 0x5e => O::U8,
        0x06 | 0x07 | 0x0a | 0x0b | 0x16 | 0x20 | 0x21 | 0x22 | 0x3b | 0x3c | 0x3d | 0x3e
        | 0x3f | 0x78 | 0x7b => O::U16,
        0x08 | 0x09 | 0x13 | 0x15 | 0x59 | 0x5c => O::U32,
        0x0c => O::Vector,
        0x29 | 0x2a | 0x2e | 0x30 | 0x32 | 0x34 => O::Call,
        0x17 => O::Locals,
        0x5a => O::EndSwitch,
        0x72 | 0x77 => O::TwoU32,
        _ if opcode < OPCODE_LIMIT => O::None,
        _ => return None,
    })
}

/// The final opcodes a function may end with.
const END: u8 = 0x00;
const RETURN: u8 = 0x01;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub at: usize,
    pub opcode: u8,
    pub operand: Range<usize>,
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
    let mut at = start;
    while at < end {
        let value = bytes[at];
        let layout = operand(value).ok_or(DecodeError::UnknownOpcode { at, value })?;
        let from = at + 1;
        let next = match layout {
            Operand::None => from,
            Operand::U8 => from + 1,
            Operand::U16 => align2(from) + 2,
            Operand::U32 => align4(from) + 4,
            Operand::Vector => align4(from) + 12,
            Operand::Call => align4(from + 1) + 4,
            Operand::Locals => {
                let count = *bytes.get(from).ok_or(DecodeError::Overrun { at })?;
                (0..count).fold(from + 1, |name, _| align2(name) + 2)
            }
            Operand::EndSwitch => {
                let count = align4(from);
                let cases = bytes
                    .get(count..count + 4)
                    .map(|word| u32::from_le_bytes(word.try_into().unwrap()) as usize)
                    .ok_or(DecodeError::Overrun { at })?;
                count + 4 + cases * 8
            }
            Operand::TwoU32 => align4(align4(from) + 4) + 4,
        };
        if next > end {
            return Err(DecodeError::Overrun { at });
        }
        out.push(Instruction {
            at,
            opcode: value,
            operand: from..next,
        });
        if matches!(value, END | RETURN) && align4(from) + 4 == end {
            return Ok(out);
        }
        at = next;
    }
    if at == end {
        return Ok(out);
    }
    Err(DecodeError::Unterminated { end })
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
