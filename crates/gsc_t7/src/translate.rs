use std::collections::BTreeMap;

use gsc::{Binary, Callee, Function, Global, Location, Op, ScriptString, Unary, Value};

use crate::{Export, Instruction, Module, Opcode, Operand};

/// What a call site or function reference names, for the linker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    /// Index into the module's imports.
    pub import: usize,
    pub method: bool,
}

/// The program-wide services a function's translation needs.
pub trait Linker {
    /// The id under which a call site is resolved after every module is read.
    fn site(&mut self, site: Site) -> u32;
    /// The native a code instruction (`isdefined`, `gettime`) stands for.
    fn native(&mut self, name: &'static str) -> Callee;
    /// The symbol id of a field or event name hash.
    fn symbol(&mut self, hash: u32) -> u32;
    /// The text a name hash stands for when the hash is used as a value.
    fn text(&self, hash: u32) -> String;
}

/// A place a value is stored, built from the reference instructions before
/// the instruction that stores, increments or clears it.
#[derive(Clone, Copy, Debug)]
enum Base {
    Local(u32),
    Field(Option<Global>, u32),
}

struct Chain {
    base: Base,
    levels: usize,
}

struct State {
    depth: usize,
    markers: Vec<usize>,
}

pub struct Translator<'a> {
    module: &'a Module,
    bytes: &'a [u8],
    strings: BTreeMap<u32, &'a str>,
    imports: BTreeMap<u32, usize>,
}

impl<'a> Translator<'a> {
    pub fn new(module: &'a Module, bytes: &'a [u8]) -> Self {
        let mut strings = BTreeMap::new();
        for string in &module.strings {
            for &at in &string.refs {
                strings.insert(at, string.text.as_str());
            }
        }
        let mut imports = BTreeMap::new();
        for (index, import) in module.imports.iter().enumerate() {
            for &at in &import.refs {
                imports.insert(at, index);
            }
        }
        Self {
            module,
            bytes,
            strings,
            imports,
        }
    }

    pub fn function(
        &self,
        name: &str,
        export: &Export,
        code: &[Instruction],
        linker: &mut dyn Linker,
    ) -> Result<Function, String> {
        let location = |at: u32| Location {
            module: self.module.name.clone(),
            function: name.to_owned(),
            line: at as usize,
            column: 0,
        };
        let locals = match code.first().map(|i| &i.operand) {
            Some(Operand::Locals(locals)) => locals.len(),
            _ => 0,
        };
        let slot = |t7: u8| -> Result<u32, String> {
            let t7 = usize::from(t7);
            if t7 >= locals {
                return Err(format!("local slot {t7} of {locals}"));
            }
            Ok((locals - 1 - t7) as u32)
        };
        let mut slots = locals;
        let mut out: Vec<(Location, Op)> = Vec::new();
        let mut pcs: BTreeMap<u32, usize> = BTreeMap::new();
        let mut fixups: Vec<(usize, u32)> = Vec::new();
        let mut entry_state: BTreeMap<u32, (usize, Vec<usize>)> = BTreeMap::new();
        let mut state = State {
            depth: 0,
            markers: Vec::new(),
        };
        let mut dead = false;
        let mut chain: Option<Chain> = None;
        let mut skip_until = 0usize;
        for (index, ins) in code.iter().enumerate() {
            if index < skip_until {
                continue;
            }
            pcs.insert(ins.at, out.len());
            if dead {
                let (depth, markers) = entry_state
                    .get(&ins.at)
                    .cloned()
                    .unwrap_or_else(|| (0, Vec::new()));
                state = State { depth, markers };
                dead = false;
            }
            let at = location(ins.at);
            macro_rules! emit {
                ($op:expr) => {
                    out.push((at.clone(), $op))
                };
            }
            macro_rules! push {
                ($value:expr) => {{
                    emit!(Op::Constant($value));
                    state.depth += 1;
                }};
            }
            macro_rules! global {
                ($global:expr) => {{
                    emit!(Op::Global($global));
                    state.depth += 1;
                }};
            }
            let operand_at = |to: u32| align(ins.at as usize + 2, to as usize) as u32;
            match (ins.op, &ins.operand) {
                (Opcode::Locals | Opcode::CheckClearParams | Opcode::ClearParams, _) => {}
                (Opcode::PreScriptCall, _) => state.markers.push(state.depth),
                (Opcode::GetZero, _) => push!(Value::Int(0)),
                (Opcode::GetByte, Operand::U8(n)) => {
                    push!(Value::Int(i32::from(*n)))
                }
                (Opcode::GetNegByte, Operand::U8(n)) => {
                    push!(Value::Int(-i32::from(*n)))
                }
                (Opcode::GetUShort, Operand::U16(n)) => {
                    push!(Value::Int(i32::from(*n)))
                }
                (Opcode::GetNegUShort, Operand::U16(n)) => {
                    push!(Value::Int(-i32::from(*n)))
                }
                (Opcode::GetInteger, Operand::U32(n)) => {
                    push!(Value::Int(*n as i32))
                }
                (Opcode::GetFloat, Operand::U32(bits)) => {
                    push!(Value::Float(f32::from_bits(*bits)))
                }
                (Opcode::GetString, Operand::U32(_)) => {
                    let text = self.strings.get(&operand_at(4)).copied().unwrap_or("");
                    push!(Value::String(ScriptString::from(text)))
                }
                (Opcode::GetIString, Operand::U32(_)) => {
                    let text = self.strings.get(&operand_at(4)).copied().unwrap_or("");
                    push!(Value::LocalizedString(text.into()))
                }
                (Opcode::GetHash, Operand::U32(hash)) => {
                    push!(Value::String(ScriptString::from(linker.text(*hash))))
                }
                (Opcode::GetUndefined, _) => push!(Value::Undefined),
                (Opcode::VectorConstant, Operand::U8(flags)) => {
                    push!(Value::Vector(unit_vector(*flags)?))
                }
                (Opcode::GetSelf, _) => global!(Global::SelfRef),
                (Opcode::GetLevel, _) => global!(Global::Level),
                (Opcode::GetGame, _) => global!(Global::Game),
                (Opcode::GetSelfObject, _) => emit!(Op::Global(Global::SelfRef)),
                (Opcode::GetLevelObject, _) => emit!(Op::Global(Global::Level)),
                (Opcode::GetEmptyArray, _) => {
                    emit!(Op::Array);
                    state.depth += 1;
                }
                (Opcode::GetFunction, _) => {
                    let import = *self.imports.get(&ins.at).ok_or("function reference")?;
                    let id = linker.site(Site {
                        import,
                        method: false,
                    });
                    emit!(Op::FunctionRef(id));
                    state.depth += 1;
                }
                (Opcode::GetTime, _) => {
                    emit!(Op::Call(linker.native("gettime"), 0, false));
                    state.depth += 1;
                }
                (Opcode::EvalLocal, Operand::U8(t7)) => {
                    emit!(Op::Load(slot(*t7)?));
                    state.depth += 1;
                }
                (Opcode::EvalSelfField | Opcode::EvalLevelField, Operand::U32(hash)) => {
                    emit!(Op::Global(if ins.op == Opcode::EvalSelfField {
                        Global::SelfRef
                    } else {
                        Global::Level
                    }));
                    emit!(Op::LoadField(linker.symbol(*hash)));
                    state.depth += 1;
                }
                (Opcode::CastFieldObject, _) => state.depth -= 1,
                (Opcode::EvalField, Operand::U32(hash)) => {
                    emit!(Op::LoadField(linker.symbol(*hash)));
                    state.depth += 1;
                }
                (Opcode::EvalArray, _) => {
                    emit!(Op::Swap);
                    emit!(Op::LoadIndex);
                    state.depth -= 1;
                }
                (Opcode::Size, _) => emit!(Op::Size),
                (Opcode::IsDefined, _) => emit!(Op::Call(linker.native("isdefined"), 1, false)),
                (Opcode::BoolNot, _) => emit!(Op::Unary(Unary::Not)),
                (Opcode::BoolComplement, _) => emit!(Op::Unary(Unary::Complement)),
                (op, _) if binary(op).is_some() => {
                    emit!(Op::Binary(binary(op).unwrap()));
                    state.depth -= 1;
                }
                (Opcode::VectorScale, _) => {
                    emit!(Op::Binary(Binary::Mul));
                    state.depth -= 1;
                }
                (Opcode::Vector, _) => {
                    emit!(Op::Reverse(3));
                    emit!(Op::Vector);
                    state.depth -= 2;
                }
                (Opcode::FirstArrayKey, _) => emit!(Op::FirstArrayKey),
                (Opcode::NextArrayKey, _) => {
                    emit!(Op::NextArrayKey);
                    state.depth -= 1;
                }
                (
                    Opcode::CallFunc
                    | Opcode::CallMethod
                    | Opcode::ThreadFunc
                    | Opcode::ThreadMethod,
                    _,
                ) => {
                    let method = matches!(ins.op, Opcode::CallMethod | Opcode::ThreadMethod);
                    let thread = matches!(ins.op, Opcode::ThreadFunc | Opcode::ThreadMethod);
                    let import = *self.imports.get(&ins.at).ok_or("call without an import")?;
                    let marker = state.markers.pop().ok_or("call without a marker")?;
                    let argc = state
                        .depth
                        .checked_sub(marker + usize::from(method))
                        .ok_or("call argument underflow")?;
                    emit!(Op::Reverse(argc + usize::from(method)));
                    let id = linker.site(Site { import, method });
                    emit!(if thread {
                        Op::Spawn(Callee::Unlinked(id), argc, method)
                    } else {
                        Op::Call(Callee::Unlinked(id), argc, method)
                    });
                    state.depth = marker + 1;
                }
                (
                    Opcode::CallPtr
                    | Opcode::CallMethodPtr
                    | Opcode::ThreadPtr
                    | Opcode::MethodThreadPtr,
                    _,
                ) => {
                    let method = matches!(ins.op, Opcode::CallMethodPtr | Opcode::MethodThreadPtr);
                    let thread = matches!(ins.op, Opcode::ThreadPtr | Opcode::MethodThreadPtr);
                    let marker = state.markers.pop().ok_or("call without a marker")?;
                    let argc = state
                        .depth
                        .checked_sub(marker + 1 + usize::from(method))
                        .ok_or("call argument underflow")?;
                    if method {
                        emit!(Op::Swap);
                    }
                    emit!(Op::Reverse(argc + 1 + usize::from(method)));
                    emit!(Op::Indirect(argc, method, thread));
                    state.depth = marker + 1;
                }
                (Opcode::DecTop, _) => {
                    emit!(Op::Pop);
                    state.depth -= 1;
                }
                (Opcode::Notify, _) => {
                    let marker = state.markers.pop().ok_or("notify without a marker")?;
                    let argc = state
                        .depth
                        .checked_sub(marker + 2)
                        .ok_or("notify argument underflow")?;
                    emit!(Op::Reverse(argc + 2));
                    emit!(Op::Notify(argc));
                    state.depth = marker;
                }
                (Opcode::WaitTill, _) => {
                    let mut outputs = Vec::new();
                    let mut next = index + 1;
                    while let Some(follow) = code.get(next) {
                        match (follow.op, &follow.operand) {
                            (Opcode::SetWaittillVar, Operand::U8(t7)) => outputs.push(slot(*t7)?),
                            (Opcode::ClearParams, _) => {
                                next += 1;
                                break;
                            }
                            _ => break,
                        }
                        next += 1;
                    }
                    for skipped in &code[index + 1..next] {
                        pcs.insert(skipped.at, out.len() + 2);
                    }
                    skip_until = next;
                    emit!(Op::Swap);
                    emit!(Op::Await(outputs));
                    state.depth -= 2;
                }
                (Opcode::Endon, _) => {
                    emit!(Op::Swap);
                    emit!(Op::Endon);
                    state.depth -= 2;
                }
                (Opcode::Wait, _) => {
                    emit!(Op::Wait);
                    state.depth -= 1;
                }
                (Opcode::WaitTillFrameEnd, _) => emit!(Op::FrameEnd),
                (Opcode::Jump | Opcode::Devblock, Operand::Jump(target)) => {
                    entry_state.insert(*target, (state.depth, state.markers.clone()));
                    fixups.push((out.len(), *target));
                    emit!(Op::Jump(0));
                    dead = true;
                }
                (Opcode::JumpOnFalse | Opcode::JumpOnTrue, Operand::Jump(target)) => {
                    if ins.op == Opcode::JumpOnTrue {
                        emit!(Op::Unary(Unary::Not));
                    }
                    state.depth -= 1;
                    entry_state.insert(*target, (state.depth, state.markers.clone()));
                    fixups.push((out.len(), *target));
                    emit!(Op::JumpFalse(0));
                }
                (Opcode::JumpOnFalseExpr | Opcode::JumpOnTrueExpr, Operand::Jump(target)) => {
                    entry_state.insert(*target, (state.depth, state.markers.clone()));
                    emit!(Op::Dup);
                    if ins.op == Opcode::JumpOnTrueExpr {
                        emit!(Op::Unary(Unary::Not));
                    }
                    fixups.push((out.len(), *target));
                    emit!(Op::JumpFalse(0));
                    emit!(Op::Pop);
                    state.depth -= 1;
                }
                (Opcode::Switch, Operand::U32(offset)) => {
                    let table = operand_at(4) + 4 + offset;
                    let (end, entries) = code
                        .iter()
                        .find_map(|other| match (other.op, &other.operand) {
                            (Opcode::EndSwitch, Operand::SwitchTable(entries))
                                if align(other.at as usize + 2, 4) as u32 == table =>
                            {
                                let end = table + 4 + 8 * entries.len() as u32;
                                Some((end, entries.clone()))
                            }
                            _ => None,
                        })
                        .ok_or("switch without its table")?;
                    let temp = slots as u32;
                    slots += 1;
                    emit!(Op::Store(temp));
                    state.depth -= 1;
                    let mut defaulted = false;
                    for (position, (value, target)) in entries.iter().enumerate() {
                        let entry = table + 4 + position as u32 * 8;
                        let case = match self.strings.get(&entry) {
                            Some(text) => Value::String(ScriptString::from(*text)),
                            None if *value == 0 && position + 1 == entries.len() => {
                                defaulted = true;
                                fixups.push((out.len(), *target));
                                emit!(Op::Jump(0));
                                continue;
                            }
                            None => Value::Int(*value as i32),
                        };
                        emit!(Op::Load(temp));
                        emit!(Op::Constant(case));
                        emit!(Op::Binary(Binary::NotEqual));
                        fixups.push((out.len(), *target));
                        emit!(Op::JumpFalse(0));
                        entry_state.insert(*target, (state.depth, state.markers.clone()));
                    }
                    if !defaulted {
                        fixups.push((out.len(), end));
                        emit!(Op::Jump(0));
                    }
                    entry_state.insert(end, (state.depth, state.markers.clone()));
                    dead = true;
                }
                (Opcode::EndSwitch, _) => {}
                (Opcode::Return, _) => {
                    emit!(Op::Return);
                    dead = true;
                }
                (Opcode::End, _) => {
                    emit!(Op::Constant(Value::Undefined));
                    emit!(Op::Return);
                    dead = true;
                }
                (Opcode::EvalLocalRef, Operand::U8(t7)) => {
                    chain = Some(Chain {
                        base: Base::Local(slot(*t7)?),
                        levels: 0,
                    })
                }
                (Opcode::EvalSelfFieldRef | Opcode::EvalLevelFieldRef, Operand::U32(hash)) => {
                    let owner = if ins.op == Opcode::EvalSelfFieldRef {
                        Global::SelfRef
                    } else {
                        Global::Level
                    };
                    chain = Some(Chain {
                        base: Base::Field(Some(owner), linker.symbol(*hash)),
                        levels: 0,
                    })
                }
                (Opcode::EvalFieldRef, Operand::U32(hash)) => {
                    chain = Some(Chain {
                        base: Base::Field(None, linker.symbol(*hash)),
                        levels: 0,
                    })
                }
                (Opcode::EvalArrayRef, _) => {
                    chain
                        .as_mut()
                        .ok_or("array reference without a base")?
                        .levels += 1;
                    state.depth -= 1;
                }
                (Opcode::Set | Opcode::Inc | Opcode::Dec | Opcode::ClearArray, _) => {
                    let taken = chain.take().ok_or("store without a reference")?;
                    store(&mut |op| out.push((at.clone(), op)), taken, ins.op);
                    state.depth -= match ins.op {
                        Opcode::Set => 1,
                        _ => 0,
                    };
                }
                (Opcode::ClearField, Operand::U32(hash)) => {
                    emit!(Op::Constant(Value::Undefined));
                    emit!(Op::StoreField(linker.symbol(*hash)));
                }
                (op, operand) => {
                    return Err(format!("{op:?} {operand:?} at {:#x}", ins.at));
                }
            }
        }
        let end = out.len();
        out.push((location(0), Op::Constant(Value::Undefined)));
        out.push((location(0), Op::Return));
        for (pc, target) in fixups {
            let to = pcs
                .range(align(target as usize, 2) as u32..)
                .next()
                .map_or(end, |(_, &pc)| pc);
            match &mut out[pc].1 {
                Op::Jump(to_pc) | Op::JumpFalse(to_pc) => *to_pc = to,
                _ => unreachable!(),
            }
        }
        Ok(Function {
            location: location(code.first().map_or(0, |i| i.at)),
            parameters: usize::from(export.params),
            slots,
            code: out,
        })
    }

    pub fn module(&self) -> &Module {
        self.module
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// Emits the store, increment or clear that ends a reference chain. The value
/// (for `Set`) sits below the chain's indices, the deepest index first; a field
/// of a computed object has the object on top.
fn store(emit: &mut impl FnMut(Op), chain: Chain, op: Opcode) {
    let one = || Op::Constant(Value::Int(1));
    let step = if op == Opcode::Dec {
        Binary::Sub
    } else {
        Binary::Add
    };
    if chain.levels == 0 {
        match (chain.base, op) {
            (Base::Local(slot), Opcode::Set) => emit(Op::Store(slot)),
            (Base::Local(slot), _) => {
                emit(Op::Load(slot));
                emit(one());
                emit(Op::Binary(step));
                emit(Op::Store(slot));
            }
            (Base::Field(owner, field), Opcode::Set) => {
                if let Some(owner) = owner {
                    emit(Op::Global(owner));
                }
                emit(Op::Swap);
                emit(Op::StoreField(field));
            }
            (Base::Field(owner, field), _) => {
                if let Some(owner) = owner {
                    emit(Op::Global(owner));
                }
                emit(Op::Dup);
                emit(Op::LoadField(field));
                emit(one());
                emit(Op::Binary(step));
                emit(Op::StoreField(field));
            }
        }
        return;
    }
    match chain.base {
        Base::Local(slot) => {
            emit(Op::EnsureLocalArray(slot));
            emit(Op::Load(slot));
        }
        Base::Field(owner, field) => {
            if let Some(owner) = owner {
                emit(Op::Global(owner));
            }
            emit(Op::EnsureFieldArray(field));
        }
    }
    for _ in 1..chain.levels {
        emit(Op::Swap);
        emit(Op::EnsureIndexArray);
    }
    match op {
        Opcode::Set => {
            emit(Op::Reverse(3));
            emit(Op::StoreIndex);
        }
        Opcode::ClearArray => {
            emit(Op::Swap);
            emit(Op::Constant(Value::Undefined));
            emit(Op::StoreIndex);
        }
        _ => {
            emit(Op::Swap);
            emit(Op::DupPair);
            emit(Op::LoadIndex);
            emit(one());
            emit(Op::Binary(step));
            emit(Op::StoreIndex);
        }
    }
}

fn binary(op: Opcode) -> Option<Binary> {
    Some(match op {
        Opcode::Plus => Binary::Add,
        Opcode::Minus => Binary::Sub,
        Opcode::Multiply => Binary::Mul,
        Opcode::Divide => Binary::Div,
        Opcode::Modulus => Binary::Mod,
        Opcode::Equal => Binary::Equal,
        Opcode::NotEqual => Binary::NotEqual,
        Opcode::SuperEqual => Binary::Identical,
        Opcode::SuperNotEqual => Binary::NotIdentical,
        Opcode::Less => Binary::Less,
        Opcode::Greater => Binary::Greater,
        Opcode::LessEqual => Binary::LessEqual,
        Opcode::GreaterEqual => Binary::GreaterEqual,
        Opcode::BitAnd => Binary::And,
        Opcode::BitOr => Binary::Or,
        Opcode::ShiftLeft => Binary::Shl,
        _ => return None,
    })
}

/// Two bits per axis, x highest: 0 is 0, 1 is -1, 2 is 1.
fn unit_vector(flags: u8) -> Result<[f32; 3], String> {
    let axis = |shift: u8| match (flags >> shift) & 3 {
        0 => Ok(0.0),
        1 => Ok(-1.0),
        2 => Ok(1.0),
        _ => Err(format!("vector constant {flags:#04x}")),
    };
    Ok([axis(4)?, axis(2)?, axis(0)?])
}

const fn align(at: usize, to: usize) -> usize {
    at.div_ceil(to) * to
}
