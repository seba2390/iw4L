pub mod library;
pub mod runtime;
pub mod tables;

use fastfile_t6::hks::{self, Constant, Instruction};
use game_api::{Unknown, unknown};
use std::sync::Arc;

pub const ENGINE_API: &Unknown = unknown!(
    "t6.ui.engine_api",
    "an engine method or library function the UI runtime does not provide",
    "the function's handler in Black Ops 2"
);

#[derive(Clone, Debug, PartialEq)]
pub enum FieldLookup {
    Value(Value),
    MetaCall {
        callee: Value,
        arguments: Vec<Value>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallContinuation {
    Normal,
    CacheResult,
    Equal(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Boolean(u32),
    Number(f32),
    Bytes(Arc<[u8]>),
    Hash(u64),
    Reference { kind: u8, identity: u32 },
}

impl Value {
    pub fn truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(0))
    }
}

impl From<Constant> for Value {
    fn from(value: Constant) -> Self {
        match value {
            Constant::Nil => Self::Nil,
            Constant::Boolean(value) => Self::Boolean(u32::from(value)),
            Constant::Number(value) => Self::Number(value),
            Constant::String(value) => Self::Bytes(Arc::from(value)),
            Constant::Hash(value) => Self::Hash(value),
        }
    }
}

pub trait Environment {
    fn length(&self, value: &Value) -> Result<usize, FaultKind> {
        match value {
            Value::Bytes(value) => Ok(value.len()),
            _ => Err(FaultKind::BindingRequired),
        }
    }
    fn global(&mut self, key: &Value) -> Result<Value, FaultKind>;
    fn field(&mut self, container: &Value, key: &Value) -> Result<Value, FaultKind>;
    fn lookup_global(&mut self, key: &Value) -> Result<FieldLookup, FaultKind> {
        self.global(key).map(FieldLookup::Value)
    }
    fn lookup_field(&mut self, container: &Value, key: &Value) -> Result<FieldLookup, FaultKind> {
        self.field(container, key).map(FieldLookup::Value)
    }
    fn new_table(&mut self) -> Result<Value, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn set_global(&mut self, _key: &Value, _value: Value) -> Result<(), FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn set_field(
        &mut self,
        _container: &Value,
        _key: &Value,
        _value: Value,
    ) -> Result<(), FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn assign_global(
        &mut self,
        key: &Value,
        value: Value,
    ) -> Result<Option<(Value, Vec<Value>)>, FaultKind> {
        self.set_global(key, value).map(|()| None)
    }
    fn assign_field(
        &mut self,
        container: &Value,
        key: &Value,
        value: Value,
    ) -> Result<Option<(Value, Vec<Value>)>, FaultKind> {
        self.set_field(container, key, value).map(|()| None)
    }
    fn equal_metamethod(
        &mut self,
        _left: &Value,
        _right: &Value,
    ) -> Result<Option<Value>, FaultKind> {
        Ok(None)
    }
    fn child_upvalues(&self, _child: usize) -> Result<usize, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn closure(&mut self, _child: usize, _captures: &[Capture]) -> Result<Value, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn upvalue(&self, _slot: usize) -> Result<Value, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn set_upvalue(&mut self, _slot: usize, _value: Value) -> Result<(), FaultKind> {
        Err(FaultKind::BindingRequired)
    }
    fn close(&mut self, _first: usize, _registers: &[Value]) -> Result<(), FaultKind> {
        Err(FaultKind::BindingRequired)
    }
}

struct Unbound;

impl Environment for Unbound {
    fn global(&mut self, _key: &Value) -> Result<Value, FaultKind> {
        Err(FaultKind::BindingRequired)
    }

    fn field(&mut self, _container: &Value, _key: &Value) -> Result<Value, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub upvalues: usize,
    pub parameters: usize,
    pub varargs: u8,
    pub registers: usize,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
    pub children: Vec<usize>,
}

impl From<hks::Function> for Function {
    fn from(function: hks::Function) -> Self {
        Self {
            upvalues: function.upvalues as usize,
            parameters: function.parameters as usize,
            varargs: function.varargs,
            registers: function.registers as usize,
            instructions: function.instructions,
            constants: function.constants.into_iter().map(Value::from).collect(),
            children: function.children,
        }
    }
}

impl From<hks::Chunk> for Program {
    fn from(chunk: hks::Chunk) -> Self {
        Self {
            functions: chunk.functions.into_iter().map(Function::from).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub registers: Vec<Value>,
    pub varargs: Vec<Value>,
    pub cached: Value,
    pub pc: usize,
    pub top: usize,
    pub iterator: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Continue,
    Returned(Vec<Value>),
    Call {
        callee: Value,
        arguments: Vec<Value>,
        register: usize,
        results: Option<usize>,
        continuation: CallContinuation,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Register(usize),
    Upvalue(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub pc: usize,
    pub kind: FaultKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaultKind {
    ProgramCounter,
    Register(usize),
    Constant(usize),
    NumberRequired,
    ComparisonRequired,
    BindingRequired,
    Argument(usize),
    ProtectedMetatable,
    ScriptError(Vec<u8>),
    Table(u32),
    Userdata(u32),
    KeyRequired,
    NilKey,
    NanKey,
    IndexChain,
    Capacity,
    Function(usize),
    Upvalue(usize),
    Capture(usize),
    Unknown(&'static Unknown),
    Branch(isize),
    Budget,
}

impl Frame {
    fn fault(&self, kind: FaultKind) -> Fault {
        Fault { pc: self.pc, kind }
    }

    fn register(&self, index: usize) -> Result<Value, Fault> {
        self.registers
            .get(index)
            .cloned()
            .ok_or_else(|| self.fault(FaultKind::Register(index)))
    }

    fn store(&mut self, index: usize, value: Value) -> Result<(), Fault> {
        let fault = self.fault(FaultKind::Register(index));
        *self.registers.get_mut(index).ok_or(fault)? = value;
        Ok(())
    }

    fn constant(&self, function: &Function, index: usize) -> Result<Value, Fault> {
        function
            .constants
            .get(index)
            .cloned()
            .ok_or_else(|| self.fault(FaultKind::Constant(index)))
    }

    fn operand(&self, function: &Function, index: usize) -> Result<Value, Fault> {
        if index < 256 {
            self.register(index)
        } else {
            self.constant(function, index & 255)
        }
    }

    fn number(&self, value: Value) -> Result<f32, Fault> {
        match value {
            Value::Number(value) => Ok(value),
            _ => Err(self.fault(FaultKind::NumberRequired)),
        }
    }

    fn equal(&self, left: &Value, right: &Value) -> Result<bool, Fault> {
        match (left, right) {
            (Value::Nil, Value::Nil) => Ok(true),
            (Value::Boolean(left), Value::Boolean(right)) => Ok(left == right),
            (Value::Number(left), Value::Number(right)) => Ok(left == right),
            (Value::Bytes(left), Value::Bytes(right)) => Ok(left == right),
            (
                Value::Reference {
                    kind: left_kind,
                    identity: left,
                },
                Value::Reference {
                    kind: right_kind,
                    identity: right,
                },
            ) => Ok(left_kind == right_kind && left == right),
            (Value::Hash(_), _) | (_, Value::Hash(_)) => {
                Err(self.fault(FaultKind::ComparisonRequired))
            }
            _ => Ok(false),
        }
    }

    pub(crate) fn branch(&self, function: &Function, offset: isize) -> Result<usize, Fault> {
        let target = self.pc as isize + offset;
        if target < 0 || target as usize > function.instructions.len() {
            Err(self.fault(FaultKind::Branch(target)))
        } else {
            Ok(target as usize)
        }
    }

    pub(crate) fn conditional(&self, function: &Function, taken: bool) -> Result<usize, Fault> {
        if taken {
            let jump = function
                .instructions
                .get(self.pc + 1)
                .ok_or_else(|| self.fault(FaultKind::ProgramCounter))?;
            self.branch(function, ((jump.0 >> 8) & 0x1ffff) as isize - 65535 + 2)
        } else {
            self.branch(function, 2)
        }
    }

    pub fn step(&mut self, function: &Function) -> Result<Step, Fault> {
        self.step_with(function, &mut Unbound)
    }

    pub fn step_with(
        &mut self,
        function: &Function,
        environment: &mut dyn Environment,
    ) -> Result<Step, Fault> {
        if let Some(register) = self.iterator.take() {
            let value = self.register(register + 3)?;
            if value == Value::Nil {
                self.pc += 1;
            } else {
                self.store(register + 2, value)?;
            }
        }
        let instruction = function
            .instructions
            .get(self.pc)
            .ok_or_else(|| self.fault(FaultKind::ProgramCounter))?
            .0;
        let opcode = (instruction >> 25) as u8;
        let a = (instruction & 255) as usize;
        let b = ((instruction >> 17) & 255) as usize;
        let c = ((instruction >> 8) & 511) as usize;
        let mut next = self.pc + 1;
        let mut result = Step::Continue;
        match opcode {
            2 | 3 | 22..=24 | 29 | 30 | 37 | 68 | 69 => {
                let callee = if matches!(opcode, 68 | 69) {
                    self.cached.clone()
                } else {
                    self.register(a)?
                };
                match callee {
                    Value::Reference {
                        kind: 5 | 7 | 9 | 10,
                        ..
                    } => {}
                    Value::Nil => return Err(self.fault(FaultKind::Unknown(ENGINE_API))),
                    _ => return Err(self.fault(FaultKind::BindingRequired)),
                }
                let end = if b == 0 { self.top } else { a + b };
                let arguments = self
                    .registers
                    .get(a + 1..end)
                    .ok_or_else(|| self.fault(FaultKind::Register(end)))?
                    .to_vec();
                if opcode == 2 || matches!(callee, Value::Reference { kind: 9, .. }) {
                    self.cached = callee.clone();
                }
                result = Step::Call {
                    callee,
                    arguments,
                    register: a,
                    results: if matches!(opcode, 22..=24 | 30 | 37 | 68) {
                        None
                    } else {
                        (c & 255).checked_sub(1)
                    },
                    continuation: CallContinuation::Normal,
                };
            }
            14 => {
                self.iterator = Some(a);
                result = Step::Call {
                    callee: self.register(a)?,
                    arguments: vec![self.register(a + 1)?, self.register(a + 2)?],
                    register: a + 3,
                    results: Some(c & 255),
                    continuation: CallContinuation::Normal,
                };
            }
            38 => {
                let value = environment.upvalue(b).map_err(|kind| self.fault(kind))?;
                self.store(a, value.clone())?;
                self.cached = value;
            }
            39 | 70 => {
                let value = if opcode == 39 {
                    self.register(a)?
                } else {
                    self.cached.clone()
                };
                environment
                    .set_upvalue(b, value.clone())
                    .map_err(|kind| self.fault(kind))?;
                self.cached = value;
            }
            65 => {
                if a > self.registers.len() {
                    return Err(self.fault(FaultKind::Register(a)));
                }
                environment
                    .close(a, &self.registers)
                    .map_err(|kind| self.fault(kind))?;
            }
            66 => {
                let child = ((instruction >> 8) & 0x1ffff) as usize;
                let count = environment
                    .child_upvalues(child)
                    .map_err(|kind| self.fault(kind))?;
                let end = next
                    .checked_add(count)
                    .ok_or_else(|| self.fault(FaultKind::ProgramCounter))?;
                let data = function
                    .instructions
                    .get(next..end)
                    .ok_or_else(|| self.fault(FaultKind::ProgramCounter))?;
                let mut captures = Vec::with_capacity(count);
                for data in data {
                    let index = ((data.0 >> 8) & 0x1ffff) as usize;
                    let capture = if data.0 & 255 == 1 {
                        self.register(index)?;
                        Capture::Register(index)
                    } else {
                        Capture::Upvalue(index)
                    };
                    captures.push(capture);
                }
                let value = environment
                    .closure(child, &captures)
                    .map_err(|kind| self.fault(kind))?;
                self.store(a, value)?;
                next = end;
            }
            67 => {
                let count = if b == 0 { self.varargs.len() } else { b - 1 };
                let end = a
                    .checked_add(count)
                    .filter(|end| *end <= 256)
                    .ok_or_else(|| self.fault(FaultKind::Capacity))?;
                if self.registers.len() < end {
                    self.registers.resize(end, Value::Nil);
                }
                for index in 0..count {
                    self.registers[a + index] =
                        self.varargs.get(index).cloned().unwrap_or(Value::Nil);
                }
                if b == 0 {
                    self.top = end;
                }
            }
            0 | 73 | 88 => {
                let container = if matches!(opcode, 0 | 88) {
                    self.register(b)?
                } else {
                    self.cached.clone()
                };
                let key = self.constant(function, c & 255)?;
                next = self.branch(function, 3)?;
                match environment
                    .lookup_field(&container, &key)
                    .map_err(|kind| self.fault(kind))?
                {
                    FieldLookup::Value(value) => {
                        self.store(a, value.clone())?;
                        self.cached = value;
                    }
                    FieldLookup::MetaCall { callee, arguments } => {
                        result = Step::Call {
                            callee,
                            arguments,
                            register: a,
                            results: Some(1),
                            continuation: CallContinuation::CacheResult,
                        };
                    }
                }
            }
            6 | 91 => {
                let key = self.constant(function, ((instruction >> 8) & 0x1ffff) as usize)?;
                match environment
                    .lookup_global(&key)
                    .map_err(|kind| self.fault(kind))?
                {
                    FieldLookup::Value(value) => {
                        self.store(a, value.clone())?;
                        self.cached = value;
                    }
                    FieldLookup::MetaCall { callee, arguments } => {
                        result = Step::Call {
                            callee,
                            arguments,
                            register: a,
                            results: Some(1),
                            continuation: CallContinuation::CacheResult,
                        };
                    }
                }
            }
            8 | 10..=12 => {
                let container = self.register(b)?;
                let key = self.operand(function, c)?;
                if opcode == 8 && !matches!(key, Value::Bytes(_)) {
                    return Err(self.fault(FaultKind::BindingRequired));
                }
                next = self.branch(function, if opcode == 8 { 1 } else { 3 })?;
                if opcode == 8 {
                    self.store(a + 1, container.clone())?;
                }
                match environment
                    .lookup_field(&container, &key)
                    .map_err(|kind| self.fault(kind))?
                {
                    FieldLookup::Value(value) => {
                        if opcode == 8 {
                            self.cached = value.clone();
                        }
                        self.store(a, value)?;
                    }
                    FieldLookup::MetaCall { callee, arguments } => {
                        result = Step::Call {
                            callee,
                            arguments,
                            register: a,
                            results: Some(1),
                            continuation: if opcode == 8 {
                                CallContinuation::CacheResult
                            } else {
                                CallContinuation::Normal
                            },
                        };
                    }
                }
            }
            15 | 74 => {
                let container = if opcode == 15 {
                    self.register(a)?
                } else {
                    self.cached.clone()
                };
                let key = self.constant(function, b)?;
                let value = self.operand(function, c)?;
                self.cached = container.clone();
                if let Some((callee, arguments)) = environment
                    .assign_field(&container, &key, value)
                    .map_err(|kind| self.fault(kind))?
                {
                    result = Step::Call {
                        callee,
                        arguments,
                        register: 0,
                        results: Some(0),
                        continuation: CallContinuation::Normal,
                    };
                }
            }
            16..=21 => {
                let container = self.register(a)?;
                let key = self.operand(function, ((instruction >> 17) & 511) as usize)?;
                let value = self.operand(function, c)?;
                if let Some((callee, arguments)) = environment
                    .assign_field(&container, &key, value)
                    .map_err(|kind| self.fault(kind))?
                {
                    result = Step::Call {
                        callee,
                        arguments,
                        register: 0,
                        results: Some(0),
                        continuation: CallContinuation::Normal,
                    };
                }
            }
            27 => {
                let key = self.constant(function, ((instruction >> 8) & 0x1ffff) as usize)?;
                let value = self.register(a)?;
                if let Some((callee, arguments)) = environment
                    .assign_global(&key, value)
                    .map_err(|kind| self.fault(kind))?
                {
                    result = Step::Call {
                        callee,
                        arguments,
                        register: 0,
                        results: Some(0),
                        continuation: CallContinuation::Normal,
                    };
                }
            }
            52 => {
                let value = environment.new_table().map_err(|kind| self.fault(kind))?;
                self.store(a, value.clone())?;
                self.cached = value;
            }
            4 | 5 | 56..=59 => {
                let left = self.operand(function, ((instruction >> 17) & 511) as usize)?;
                let right = self.operand(function, c)?;
                if opcode == 4 || opcode == 5 {
                    if let (
                        Value::Reference {
                            kind: left_kind @ (5 | 7),
                            identity: left_id,
                        },
                        Value::Reference {
                            kind: right_kind,
                            identity: right_id,
                        },
                    ) = (&left, &right)
                        && left_kind == right_kind
                        && left_id != right_id
                        && let Some(callee) = environment
                            .equal_metamethod(&left, &right)
                            .map_err(|kind| self.fault(kind))?
                    {
                        next = self.pc;
                        result = Step::Call {
                            callee,
                            arguments: vec![left, right],
                            register: 0,
                            results: Some(0),
                            continuation: CallContinuation::Equal(a != 0),
                        };
                    } else {
                        let comparison = self.equal(&left, &right)?;
                        next = self.conditional(function, usize::from(comparison) == a)?;
                    }
                } else {
                    let left = self.number(left)?;
                    let right = self.number(right)?;
                    let comparison = if opcode <= 57 {
                        left < right
                    } else {
                        left <= right
                    };
                    next = self.conditional(function, usize::from(comparison) == a)?;
                }
            }
            7 => {
                let value = self.register(b)?;
                self.store(a, value.clone())?;
                self.cached = value;
            }
            9 => {
                let end = if b == 0 { self.top } else { a + b - 1 };
                let values = self
                    .registers
                    .get(a..end)
                    .ok_or_else(|| self.fault(FaultKind::Register(end)))?;
                result = Step::Returned(values.to_vec());
            }
            13 => {
                self.store(a, Value::Boolean(b as u32))?;
                next = self.branch(function, 1 + (c & 255) as isize)?;
            }
            25 => {
                let value = self.constant(function, ((instruction >> 8) & 0x1ffff) as usize)?;
                self.store(a, value)?;
            }
            26 => {
                if a <= b {
                    let fault = self.fault(FaultKind::Register(b));
                    self.registers.get_mut(a..=b).ok_or(fault)?.fill(Value::Nil);
                }
            }
            28 => {
                next = self.branch(
                    function,
                    ((instruction >> 8) & 0x1ffff) as isize - 65535 + 1,
                )?;
            }
            40..=51 => {
                let left =
                    self.number(self.operand(function, ((instruction >> 17) & 511) as usize)?)?;
                let right = self.number(self.operand(function, c)?)?;
                let value = match opcode {
                    40 | 41 => right + left,
                    42 | 43 => left - right,
                    44 | 45 => right * left,
                    46 | 47 => left / right,
                    48 | 49 => left - (left / right).floor() * right,
                    50 | 51 => left.powf(right),
                    _ => unreachable!(),
                };
                self.store(a, Value::Number(value))?;
            }
            53 => {
                let value = self.number(self.register(b)?)?;
                self.store(a, Value::Number(-value))?;
            }
            54 | 72 => {
                let value = if opcode == 54 {
                    self.register(b)?
                } else {
                    self.cached.clone()
                };
                self.store(a, Value::Boolean(u32::from(!value.truthy())))?;
                self.cached = value;
            }
            55 => {
                let length = environment
                    .length(&self.register(b)?)
                    .map_err(|kind| self.fault(kind))?;
                self.store(a, Value::Number(length as f32))?;
            }
            1 | 71 => {
                let value = if opcode == 1 {
                    self.register(a)?
                } else {
                    self.cached.clone()
                };
                next = self.conditional(function, usize::from(value.truthy()) == (c & 255))?;
                self.cached = value;
            }
            61 => {
                let value = self.register(b)?;
                let taken = usize::from(value.truthy()) == (c & 255);
                next = self.conditional(function, taken)?;
                if taken {
                    self.store(a, value)?;
                }
            }
            60 => {
                let mut bytes = Vec::new();
                for index in b..=(c & 255) {
                    match self.register(index)? {
                        Value::Bytes(value) => bytes.extend_from_slice(&value),
                        Value::Number(value) if value.is_finite() => {
                            bytes.extend_from_slice(value.to_string().as_bytes());
                        }
                        _ => return Err(self.fault(FaultKind::BindingRequired)),
                    }
                }
                self.store(a, Value::Bytes(Arc::from(bytes)))?;
            }
            62 => {
                let numeric = |value: Value| match value {
                    Value::Number(value) => Ok(value),
                    Value::Bytes(value) => core::str::from_utf8(&value)
                        .ok()
                        .and_then(|value| value.trim().parse::<f32>().ok())
                        .ok_or_else(|| self.fault(FaultKind::NumberRequired)),
                    _ => Err(self.fault(FaultKind::NumberRequired)),
                };
                let start = numeric(self.register(a)?)?;
                let limit = numeric(self.register(a + 1)?)?;
                let step = numeric(self.register(a + 2)?)?;
                let count = (limit - start) / step;
                let count = if count < 0.0 || count > 1_073_741_824.0 {
                    -1.0
                } else {
                    count
                };
                let remaining = if count.is_nan() {
                    i32::MIN
                } else {
                    count as i32
                };
                self.store(a, Value::Number(start - step))?;
                self.store(
                    a + 1,
                    Value::Number(f32::from_bits(remaining.wrapping_add(1) as u32)),
                )?;
                self.store(a + 2, Value::Number(step))?;
                self.store(a + 3, Value::Number(start))?;
                next = self.branch(
                    function,
                    ((instruction >> 8) & 0x1ffff) as isize - 65535 + 1,
                )?;
            }
            63 => {
                let index = self.number(self.register(a)?)? + self.number(self.register(a + 2)?)?;
                let remaining = self.number(self.register(a + 1)?)?.to_bits() as i32;
                if remaining > 0 {
                    self.store(a + 1, Value::Number(f32::from_bits((remaining - 1) as u32)))?;
                    self.store(a, Value::Number(index))?;
                    self.store(a + 3, Value::Number(index))?;
                    next = self.branch(
                        function,
                        ((instruction >> 8) & 0x1ffff) as isize - 65535 + 1,
                    )?;
                }
            }
            64 => {
                let count = if b == 0 {
                    self.top.saturating_sub(a + 1)
                } else {
                    b
                };
                let block = if c == 0 {
                    let data = function
                        .instructions
                        .get(next)
                        .ok_or_else(|| self.fault(FaultKind::ProgramCounter))?;
                    next += 1;
                    ((data.0 >> 8) & 0x1ffff) as usize
                } else {
                    c
                };
                let first = block
                    .checked_sub(1)
                    .and_then(|block| block.checked_mul(50))
                    .ok_or_else(|| self.fault(FaultKind::Capacity))?;
                let container = self.register(a)?;
                for index in 1..=count {
                    environment
                        .set_field(
                            &container,
                            &Value::Number((first + index) as f32),
                            self.register(a + index)?,
                        )
                        .map_err(|kind| self.fault(kind))?;
                }
            }
            31..=36 | 75..=87 | 89..=90 => {}
            _ => return Err(self.fault(FaultKind::ProgramCounter)),
        }
        self.pc = next;
        Ok(result)
    }

    pub fn run(&mut self, function: &Function, budget: usize) -> Result<Vec<Value>, Fault> {
        self.run_with(function, budget, &mut Unbound)
    }

    pub fn run_with(
        &mut self,
        function: &Function,
        budget: usize,
        environment: &mut dyn Environment,
    ) -> Result<Vec<Value>, Fault> {
        for _ in 0..budget {
            match self.step_with(function, environment)? {
                Step::Returned(values) => return Ok(values),
                Step::Call { .. } => return Err(self.fault(FaultKind::BindingRequired)),
                Step::Continue => {}
            }
        }
        Err(self.fault(FaultKind::Budget))
    }
}
