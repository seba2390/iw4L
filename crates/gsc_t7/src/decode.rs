use crate::opcodes::{Layout, Opcode, opcode};

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    None,
    U8(u8),
    /// Absolute offset of the jump target in the module.
    Jump(u32),
    U16(u16),
    U32(u32),
    U64(u64),
    Call {
        params: u8,
        target: u64,
    },
    /// Argument count and method name hash of a class method call.
    Method {
        params: u8,
        name: u32,
    },
    /// Name hash and flags of each local, in slot order.
    Locals(Vec<(u32, u8)>),
    /// Case value and absolute target of each `switch` entry, in table order.
    SwitchTable(Vec<(u32, u32)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    pub at: u32,
    pub op: Opcode,
    pub operand: Operand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownOpcode {
        at: u32,
        value: u16,
    },
    OutOfBounds {
        at: u32,
    },
    /// The code runs to the end of the function without an `End` or `Return`
    /// followed only by padding.
    NoEnd,
}

const fn align(at: usize, to: usize) -> usize {
    at.div_ceil(to) * to
}

/// Decodes one function's code between `start` and `end` (the next function or
/// the end of the code segment).
pub fn decode_function(
    module: &[u8],
    start: usize,
    end: usize,
) -> Result<Vec<Instruction>, DecodeError> {
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        at = align(at, 2);
        let value = u16_at(module, at)?;
        let op = opcode(value).ok_or(DecodeError::UnknownOpcode {
            at: at as u32,
            value,
        })?;
        let (operand, next) = operand(module, at, op)?;
        out.push(Instruction {
            at: at as u32,
            op,
            operand,
        });
        at = next;
        // The last function of a module ends with the code segment, which need
        // not be 8-aligned: its tail is then empty.
        if matches!(op, Opcode::End | Opcode::Return)
            && module
                .get(align(at, 8).min(end)..end)
                .is_some_and(|tail| tail.iter().all(|&byte| byte == 0))
        {
            return Ok(out);
        }
    }
    Err(DecodeError::NoEnd)
}

fn operand(module: &[u8], at: usize, op: Opcode) -> Result<(Operand, usize), DecodeError> {
    let after = at + 2;
    Ok(match op.layout() {
        Layout::None if op == Opcode::EndSwitch => {
            let table = align(after, 4);
            let count = u32_at(module, table)? as usize;
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let entry = table + 4 + index * 8;
                let value = u32_at(module, entry)?;
                let offset = u32_at(module, entry + 4)? as i32;
                let target = (entry as i64 + 8 + i64::from(offset)) as u32;
                entries.push((value, target));
            }
            (Operand::SwitchTable(entries), table + 4 + count * 8)
        }
        Layout::None => (Operand::None, after),
        Layout::U8 => (Operand::U8(byte_at(module, after)?), after + 1),
        Layout::Jump => {
            let offset = u16_at(module, after)? as i16;
            let next = after + 2;
            let target = (next as i64 + i64::from(offset)) as u32;
            (Operand::Jump(target), next)
        }
        Layout::U16 => (Operand::U16(u16_at(module, after)?), after + 2),
        Layout::U32 => {
            let word = align(after, 4);
            (Operand::U32(u32_at(module, word)?), word + 4)
        }
        Layout::U64 => {
            let word = align(after, 8);
            (Operand::U64(u64_at(module, word)?), word + 8)
        }
        Layout::Call => {
            let params = byte_at(module, after)?;
            let word = align(after + 1, 8);
            (
                Operand::Call {
                    params,
                    target: u64_at(module, word)?,
                },
                word + 8,
            )
        }
        Layout::Method => {
            let params = byte_at(module, after)?;
            let word = align(after + 1, 4);
            (
                Operand::Method {
                    params,
                    name: u32_at(module, word)?,
                },
                word + 4,
            )
        }
        Layout::Locals => {
            let count = byte_at(module, after)?;
            let mut at = after + 1;
            let mut locals = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                let word = align(at, 4);
                locals.push((u32_at(module, word)?, byte_at(module, word + 4)?));
                at = word + 5;
            }
            (Operand::Locals(locals), at)
        }
    })
}

fn byte_at(module: &[u8], at: usize) -> Result<u8, DecodeError> {
    module
        .get(at)
        .copied()
        .ok_or(DecodeError::OutOfBounds { at: at as u32 })
}

fn u16_at(module: &[u8], at: usize) -> Result<u16, DecodeError> {
    module
        .get(at..at + 2)
        .map(|word| u16::from_le_bytes(word.try_into().unwrap()))
        .ok_or(DecodeError::OutOfBounds { at: at as u32 })
}

fn u32_at(module: &[u8], at: usize) -> Result<u32, DecodeError> {
    module
        .get(at..at + 4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .ok_or(DecodeError::OutOfBounds { at: at as u32 })
}

fn u64_at(module: &[u8], at: usize) -> Result<u64, DecodeError> {
    module
        .get(at..at + 8)
        .map(|word| u64::from_le_bytes(word.try_into().unwrap()))
        .ok_or(DecodeError::OutOfBounds { at: at as u32 })
}
