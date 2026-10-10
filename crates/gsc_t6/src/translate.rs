//! Black Ops 2's decoded functions onto IW4L's GSC IR.

use std::collections::BTreeMap;
use std::sync::Arc;

use gsc::{Binary, Callee, Function, Global, Location, Op, ScriptString, Unary, Value};

use crate::{Instruction, Module, Opcode};

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
    /// The native an instruction (`isdefined`, `gettime`, `getdvar`) stands for.
    fn native(&mut self, name: &'static str) -> Callee;
    /// The symbol of a field or event name.
    fn named(&mut self, name: &str) -> u32;
    /// The value a dvar name hash (`dvar_hash`) stands for.
    fn hash(&mut self, hash: u32) -> Result<Value, String>;
}

/// How Black Ops 2 hashes a dvar name its scripts refer to by hash: djb2 over
/// the lowercased name.
pub fn dvar_hash(name: &str) -> u32 {
    name.bytes().fold(5381u32, |hash, byte| {
        hash.wrapping_mul(33)
            .wrapping_add(u32::from(byte.to_ascii_lowercase()))
    })
}

/// A place a value is stored, built from the reference instructions before
/// the instruction that stores, increments or clears it.
#[derive(Clone, Copy, Debug)]
enum Base {
    Local(u32),
    /// A field of the current object, which sits on the stack.
    Field(u32),
    /// A global object the instruction before pushed (`game[…] = …`).
    Object,
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
    strings: BTreeMap<usize, &'a str>,
    imports: BTreeMap<usize, usize>,
    /// Animation trees and animations by the code offset the game patches.
    animations: BTreeMap<usize, Value>,
}

impl<'a> Translator<'a> {
    pub fn new(module: &'a Module, bytes: &'a [u8]) -> Self {
        let mut strings = BTreeMap::new();
        for string in &module.strings {
            for &at in &string.refs {
                strings.insert(at as usize, string.text.as_str());
            }
        }
        let mut imports = BTreeMap::new();
        for (index, import) in module.imports.iter().enumerate() {
            for &at in &import.refs {
                imports.insert(at as usize, index);
            }
        }
        let mut animations = BTreeMap::new();
        for animtree in &module.animtrees {
            let tree: Arc<str> = animtree.tree.as_str().into();
            for &at in &animtree.tree_refs {
                animations.insert(at as usize, Value::AnimationTree(tree.clone()));
            }
            for (name, at) in &animtree.animations {
                animations.insert(
                    *at as usize,
                    Value::Animation {
                        tree: tree.clone(),
                        name: name.as_str().into(),
                    },
                );
            }
        }
        Self {
            module,
            bytes,
            strings,
            imports,
            animations,
        }
    }

    pub fn function(
        &self,
        name: &str,
        params: u8,
        code: &[Instruction],
        linker: &mut dyn Linker,
    ) -> Result<Function, String> {
        let location = |at: usize| Location {
            module: self.module.name.clone(),
            function: name.to_owned(),
            line: at,
            column: 0,
        };
        let bytes = self.bytes;
        let u8_at = |i: &Instruction| bytes[i.operand.start];
        let u16_at = |i: &Instruction| {
            u16::from_le_bytes([bytes[i.operand.end - 2], bytes[i.operand.end - 1]])
        };
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let string_at = |at: usize| self.strings.get(&at).copied();
        let locals = match code.first() {
            Some(i) if i.opcode == Opcode::SafeCreateLocalVariables as u8 => usize::from(u8_at(i)),
            _ => 0,
        };
        let slot = |cached: u8| -> Result<u32, String> {
            let cached = usize::from(cached);
            if cached >= locals {
                return Err(format!("local {cached} of {locals}"));
            }
            Ok((locals - 1 - cached) as u32)
        };
        let mut slots = locals;
        let mut out: Vec<(Location, Op)> = Vec::new();
        let mut pcs: BTreeMap<usize, usize> = BTreeMap::new();
        let mut fixups: Vec<(usize, usize)> = Vec::new();
        let mut entry_state: BTreeMap<usize, (usize, Vec<usize>)> = BTreeMap::new();
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
            let op = Opcode::from_byte(ins.opcode).ok_or("opcode out of range")?;
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
            macro_rules! native {
                ($name:expr, $argc:expr) => {{
                    emit!(Op::Call(linker.native($name), $argc, false));
                    state.depth = state.depth + 1 - $argc;
                }};
            }
            let field = |linker: &mut dyn Linker| -> Result<u32, String> {
                let text = string_at(ins.operand.end - 2).ok_or("name without a string")?;
                Ok(linker.named(text))
            };
            match op {
                Opcode::SafeCreateLocalVariables
                | Opcode::CheckClearParams
                | Opcode::Nop
                | Opcode::Nop2 => {}
                Opcode::PreScriptCall | Opcode::VoidCodePos => state.markers.push(state.depth),
                Opcode::GetUndefined | Opcode::ProfileStop => push!(Value::Undefined),
                Opcode::ProfileStart => push!(Value::Undefined),
                Opcode::GetZero => push!(Value::Int(0)),
                Opcode::GetByte => push!(Value::Int(i32::from(u8_at(ins)))),
                Opcode::GetNegByte => push!(Value::Int(-i32::from(u8_at(ins)))),
                Opcode::GetUShort => push!(Value::Int(i32::from(u16_at(ins)))),
                Opcode::GetNegUShort => push!(Value::Int(-i32::from(u16_at(ins)))),
                Opcode::GetInteger => {
                    let word = ins.operand.end - 4;
                    push!(match self.animations.get(&word) {
                        Some(tree) => tree.clone(),
                        None => Value::Int(u32_at(word) as i32),
                    })
                }
                Opcode::GetFloat => {
                    push!(Value::Float(f32::from_bits(u32_at(ins.operand.end - 4))))
                }
                Opcode::GetString => {
                    let text = string_at(ins.operand.end - 2).ok_or("string without its text")?;
                    push!(Value::String(ScriptString::from(text)))
                }
                Opcode::GetIString => {
                    let text = string_at(ins.operand.end - 2).ok_or("string without its text")?;
                    push!(Value::LocalizedString(text.into()))
                }
                Opcode::GetVector => {
                    let at = ins.operand.end - 12;
                    let axis = |n: usize| f32::from_bits(u32_at(at + 4 * n));
                    push!(Value::Vector([axis(0), axis(1), axis(2)]))
                }
                Opcode::GetHash => push!(linker.hash(u32_at(ins.operand.end - 4))?),
                Opcode::VectorConstant => push!(Value::Vector(unit_vector(u8_at(ins))?)),
                Opcode::GetAnimation => {
                    let animation = self
                        .animations
                        .get(&(ins.operand.end - 4))
                        .ok_or("animation without an animation tree entry")?;
                    push!(animation.clone())
                }
                Opcode::GetSelf => global!(Global::SelfRef),
                Opcode::GetLevel => global!(Global::Level),
                Opcode::GetGame => global!(Global::Game),
                Opcode::GetAnim => global!(Global::Anim),
                Opcode::GetSelfObject => emit!(Op::Global(Global::SelfRef)),
                Opcode::GetLevelObject => emit!(Op::Global(Global::Level)),
                Opcode::GetAnimObject => emit!(Op::Global(Global::Anim)),
                Opcode::GetGameRef => {
                    emit!(Op::Global(Global::Game));
                    chain = Some(Chain {
                        base: Base::Object,
                        levels: 0,
                    });
                }
                Opcode::GetEmptyArray => {
                    emit!(Op::Array);
                    state.depth += 1;
                }
                Opcode::GetFunction => {
                    let import = *self.imports.get(&ins.at).ok_or("function reference")?;
                    emit!(Op::FunctionRef(linker.site(Site {
                        import,
                        method: false
                    })));
                    state.depth += 1;
                }
                Opcode::GetTime => native!("gettime", 0),
                Opcode::IsDefined => native!("isdefined", 1),
                Opcode::AnglesToUp => native!("anglestoup", 1),
                Opcode::AnglesToRight => native!("anglestoright", 1),
                Opcode::AnglesToForward => native!("anglestoforward", 1),
                Opcode::AngleClamp180 => native!("angleclamp180", 1),
                Opcode::VectorToAngles => native!("vectortoangles", 1),
                Opcode::Abs => native!("abs", 1),
                Opcode::GetDvar => native!("getdvar", 1),
                Opcode::GetDvarInt => native!("getdvarint", 1),
                Opcode::GetDvarFloat => native!("getdvarfloat", 1),
                Opcode::GetDvarVector => native!("getdvarvector", 1),
                Opcode::GetDvarColorRed => native!("getdvarcolorred", 1),
                Opcode::GetDvarColorGreen => native!("getdvarcolorgreen", 1),
                Opcode::GetDvarColorBlue => native!("getdvarcolorblue", 1),
                Opcode::GetDvarColorAlpha => native!("getdvarcoloralpha", 1),
                Opcode::EvalLocalVariableCached => {
                    emit!(Op::Load(slot(u8_at(ins))?));
                    state.depth += 1;
                }
                Opcode::CastFieldObject => state.depth -= 1,
                Opcode::EvalFieldVariable => {
                    emit!(Op::LoadField(field(linker)?));
                    state.depth += 1;
                }
                Opcode::EvalArray => {
                    emit!(Op::Swap);
                    emit!(Op::LoadIndex);
                    state.depth -= 1;
                }
                Opcode::SizeOf => emit!(Op::Size),
                Opcode::BoolNot => emit!(Op::Unary(Unary::Not)),
                Opcode::BoolComplement => emit!(Op::Unary(Unary::Complement)),
                _ if binary(op).is_some() => {
                    emit!(Op::Binary(binary(op).unwrap()));
                    state.depth -= 1;
                }
                Opcode::VectorScale => {
                    emit!(Op::Binary(Binary::Mul));
                    state.depth -= 1;
                }
                Opcode::Vector => {
                    emit!(Op::Reverse(3));
                    emit!(Op::Vector);
                    state.depth -= 2;
                }
                Opcode::FirstArrayKey => emit!(Op::FirstArrayKey),
                Opcode::NextArrayKey => {
                    emit!(Op::NextArrayKey);
                    state.depth -= 1;
                }
                Opcode::ScriptFunctionCall
                | Opcode::ScriptMethodCall
                | Opcode::ScriptThreadCall
                | Opcode::ScriptMethodThreadCall => {
                    let method = matches!(
                        op,
                        Opcode::ScriptMethodCall | Opcode::ScriptMethodThreadCall
                    );
                    let thread = matches!(
                        op,
                        Opcode::ScriptThreadCall | Opcode::ScriptMethodThreadCall
                    );
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
                Opcode::ScriptFunctionCallPointer
                | Opcode::ScriptMethodCallPointer
                | Opcode::ScriptThreadCallPointer
                | Opcode::ScriptMethodThreadCallPointer => {
                    let method = matches!(
                        op,
                        Opcode::ScriptMethodCallPointer | Opcode::ScriptMethodThreadCallPointer
                    );
                    let thread = matches!(
                        op,
                        Opcode::ScriptThreadCallPointer | Opcode::ScriptMethodThreadCallPointer
                    );
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
                Opcode::DecTop => {
                    emit!(Op::Pop);
                    state.depth -= 1;
                }
                Opcode::Notify => {
                    let marker = state.markers.pop().ok_or("notify without a marker")?;
                    let argc = state
                        .depth
                        .checked_sub(marker + 2)
                        .ok_or("notify argument underflow")?;
                    emit!(Op::Reverse(argc + 2));
                    emit!(Op::Notify(argc));
                    state.depth = marker;
                }
                Opcode::WaitTill => {
                    let mut outputs = Vec::new();
                    let mut next = index + 1;
                    while let Some(follow) = code.get(next) {
                        match Opcode::from_byte(follow.opcode) {
                            Some(Opcode::SafeSetWaittillVariableFieldCached) => {
                                outputs.push(slot(u8_at(follow))?)
                            }
                            Some(Opcode::ClearParams) => {
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
                Opcode::WaitTillMatch => {
                    let count = usize::from(u8_at(ins));
                    emit!(Op::Reverse(count + 2));
                    emit!(Op::AwaitMatch(count));
                    state.depth = state
                        .depth
                        .checked_sub(count + 2)
                        .ok_or("waittillmatch argument underflow")?;
                    if let Some(follow) = code.get(index + 1)
                        && Opcode::from_byte(follow.opcode) == Some(Opcode::ClearParams)
                    {
                        pcs.insert(follow.at, out.len());
                        skip_until = index + 2;
                    }
                }
                Opcode::EndOn => {
                    emit!(Op::Swap);
                    emit!(Op::Endon);
                    state.depth -= 2;
                }
                Opcode::Wait | Opcode::RealWait => {
                    emit!(Op::Wait);
                    state.depth -= 1;
                }
                Opcode::WaitTillFrameEnd => emit!(Op::FrameEnd),
                Opcode::Jump | Opcode::DevblockBegin => {
                    let target = jump(ins, bytes)?;
                    entry_state.insert(target, (state.depth, state.markers.clone()));
                    fixups.push((out.len(), target));
                    emit!(Op::Jump(0));
                    dead = true;
                }
                Opcode::JumpOnFalse | Opcode::JumpOnTrue => {
                    let target = jump(ins, bytes)?;
                    if op == Opcode::JumpOnTrue {
                        emit!(Op::Unary(Unary::Not));
                    }
                    state.depth -= 1;
                    entry_state.insert(target, (state.depth, state.markers.clone()));
                    fixups.push((out.len(), target));
                    emit!(Op::JumpFalse(0));
                }
                Opcode::JumpOnFalseExpr | Opcode::JumpOnTrueExpr => {
                    let target = jump(ins, bytes)?;
                    entry_state.insert(target, (state.depth, state.markers.clone()));
                    emit!(Op::Dup);
                    if op == Opcode::JumpOnTrueExpr {
                        emit!(Op::Unary(Unary::Not));
                    }
                    fixups.push((out.len(), target));
                    emit!(Op::JumpFalse(0));
                    emit!(Op::Pop);
                    state.depth -= 1;
                }
                Opcode::Switch => {
                    let table = (ins.operand.end + u32_at(ins.operand.end - 4) as usize + 3) & !3;
                    let count = u32_at(table) as usize;
                    let end = table + 4 + 8 * count;
                    let temp = slots as u32;
                    slots += 1;
                    emit!(Op::Store(temp));
                    state.depth -= 1;
                    let mut defaulted = false;
                    for position in 0..count {
                        let entry = table + 4 + 8 * position;
                        let value = u32_at(entry);
                        let target =
                            (entry as isize + 8 + u32_at(entry + 4) as i32 as isize) as usize;
                        let case = match string_at(entry) {
                            Some(text) => Value::String(ScriptString::from(text)),
                            None if value == 0 && position + 1 == count => {
                                defaulted = true;
                                entry_state.insert(target, (state.depth, state.markers.clone()));
                                fixups.push((out.len(), target));
                                emit!(Op::Jump(0));
                                continue;
                            }
                            None => Value::Int(value as i32),
                        };
                        emit!(Op::Load(temp));
                        emit!(Op::Constant(case));
                        emit!(Op::Binary(Binary::NotEqual));
                        fixups.push((out.len(), target));
                        emit!(Op::JumpFalse(0));
                        entry_state.insert(target, (state.depth, state.markers.clone()));
                    }
                    if !defaulted {
                        fixups.push((out.len(), end));
                        emit!(Op::Jump(0));
                    }
                    entry_state.insert(end, (state.depth, state.markers.clone()));
                    dead = true;
                }
                Opcode::EndSwitch => {}
                Opcode::Return => {
                    emit!(Op::Return);
                    dead = true;
                }
                Opcode::End => {
                    emit!(Op::Constant(Value::Undefined));
                    emit!(Op::Return);
                    dead = true;
                }
                Opcode::EvalLocalVariableRefCached => {
                    chain = Some(Chain {
                        base: Base::Local(slot(u8_at(ins))?),
                        levels: 0,
                    })
                }
                Opcode::EvalLocalArrayRefCached => {
                    chain = Some(Chain {
                        base: Base::Local(slot(u8_at(ins))?),
                        levels: 1,
                    });
                    state.depth -= 1;
                }
                Opcode::EvalFieldVariableRef => {
                    chain = Some(Chain {
                        base: Base::Field(field(linker)?),
                        levels: 0,
                    })
                }
                Opcode::EvalArrayRef => {
                    chain
                        .as_mut()
                        .ok_or("array reference without a base")?
                        .levels += 1;
                    state.depth -= 1;
                }
                Opcode::SetVariableField | Opcode::Inc | Opcode::Dec | Opcode::ClearArray => {
                    let mut taken = chain.take().ok_or("store without a reference")?;
                    if op == Opcode::ClearArray {
                        // Its key sits on the stack: one more level than the chain.
                        taken.levels += 1;
                    } else if matches!(taken.base, Base::Object) && taken.levels == 0 {
                        return Err(format!("store to a global object at {:#x}", ins.at));
                    }
                    store(&mut |op| out.push((at.clone(), op)), taken, op);
                    state.depth -= match op {
                        Opcode::SetVariableField | Opcode::ClearArray => 1,
                        _ => 0,
                    };
                }
                Opcode::ClearFieldVariable => {
                    emit!(Op::Constant(Value::Undefined));
                    emit!(Op::StoreField(field(linker)?));
                }
                other => return Err(format!("{other:?} at {:#x}", ins.at)),
            }
        }
        let end = out.len();
        out.push((location(0), Op::Constant(Value::Undefined)));
        out.push((location(0), Op::Return));
        for (pc, target) in fixups {
            let to = pcs.range(target..).next().map_or(end, |(_, &pc)| pc);
            match &mut out[pc].1 {
                Op::Jump(to_pc) | Op::JumpFalse(to_pc) => *to_pc = to,
                _ => unreachable!(),
            }
        }
        Ok(Function {
            location: location(code.first().map_or(0, |i| i.at)),
            parameters: usize::from(params),
            slots,
            code: out,
        })
    }

    pub fn module(&self) -> &Module {
        self.module
    }
}

fn jump(ins: &Instruction, bytes: &[u8]) -> Result<usize, String> {
    ins.jump_target(bytes)
        .and_then(|target| usize::try_from(target).ok())
        .ok_or_else(|| format!("jump at {:#x}", ins.at))
}

/// Emits the store, increment or clear that ends a reference chain. The value
/// (for a store) sits below the chain's indices, the deepest index first; a
/// field of the current object has the object on top.
fn store(emit: &mut impl FnMut(Op), chain: Chain, op: Opcode) {
    let one = || Op::Constant(Value::Int(1));
    let step = if op == Opcode::Dec {
        Binary::Sub
    } else {
        Binary::Add
    };
    if chain.levels == 0 {
        match (chain.base, op) {
            (Base::Object, _) => {}
            (Base::Local(slot), Opcode::SetVariableField) => emit(Op::Store(slot)),
            (Base::Local(slot), _) => {
                emit(Op::Load(slot));
                emit(one());
                emit(Op::Binary(step));
                emit(Op::Store(slot));
            }
            (Base::Field(field), Opcode::SetVariableField) => {
                emit(Op::Swap);
                emit(Op::StoreField(field));
            }
            (Base::Field(field), _) => {
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
        Base::Field(field) => emit(Op::EnsureFieldArray(field)),
        Base::Object => {}
    }
    for _ in 1..chain.levels {
        emit(Op::Swap);
        emit(Op::EnsureIndexArray);
    }
    match op {
        Opcode::SetVariableField => {
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
        Opcode::Less => Binary::Less,
        Opcode::Greater => Binary::Greater,
        Opcode::LessEqual => Binary::LessEqual,
        Opcode::GreaterEqual => Binary::GreaterEqual,
        Opcode::BitAnd => Binary::And,
        Opcode::BitOr => Binary::Or,
        Opcode::ShiftLeft => Binary::Shl,
        Opcode::ShiftRight => Binary::Shr,
        Opcode::BitXor => Binary::Xor,
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
