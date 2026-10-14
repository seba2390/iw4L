use super::tables::Tables;
use super::{Capture, Environment, Fault, FaultKind, Frame, Program, Step, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct Closure {
    program: usize,
    function: usize,
    environment: Value,
    upvalues: Vec<usize>,
}

#[derive(Clone, Debug)]
enum Cell {
    Open { frame: usize, register: usize },
    Closed(Value),
}

#[derive(Clone, Copy, Debug)]
struct ReturnTo {
    register: usize,
    results: Option<usize>,
}

struct CallSite {
    cached: Value,
    return_to: Option<ReturnTo>,
    pc: usize,
}

#[derive(Clone, Debug)]
struct Activation {
    closure: usize,
    frame: Frame,
    return_to: Option<ReturnTo>,
    returned_override: Option<Vec<Value>>,
}

#[derive(Clone, Debug)]
pub struct Callback {
    pub callee: Value,
    pub arguments: Vec<Value>,
}

pub trait Bindings {
    fn call(
        &mut self,
        function: u32,
        arguments: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind>;

    fn take_callback(&mut self) -> Option<Callback> {
        None
    }
}

struct Unbound;

impl Bindings for Unbound {
    fn call(
        &mut self,
        _function: u32,
        _arguments: &[Value],
        _tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        Err(FaultKind::BindingRequired)
    }
}

#[derive(Debug, Default)]
pub struct Runtime {
    pub tables: Tables,
    programs: Vec<Arc<Program>>,
    closures: Vec<Closure>,
    cells: Vec<Cell>,
    open: BTreeMap<(usize, usize), usize>,
    frames: Vec<Activation>,
    last_fault: Option<(usize, usize)>,
}

impl Runtime {
    pub fn failure_source(&self) -> Option<(usize, usize)> {
        self.last_fault
    }
    fn closure_index(&self, value: &Value) -> Result<usize, FaultKind> {
        let Value::Reference { kind: 9, identity } = value else {
            return Err(FaultKind::BindingRequired);
        };
        identity
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.closures.len())
            .ok_or(FaultKind::Function(*identity as usize))
    }

    fn insert_closure(&mut self, closure: Closure) -> Result<Value, FaultKind> {
        let identity = u32::try_from(self.closures.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(FaultKind::Capacity)?;
        self.closures.push(closure);
        Ok(Value::Reference { kind: 9, identity })
    }

    pub fn load(&mut self, program: Program) -> Result<Value, FaultKind> {
        let root = program.functions.first().ok_or(FaultKind::Function(0))?;
        if root.upvalues != 0 {
            return Err(FaultKind::Upvalue(root.upvalues));
        }
        let index = self.programs.len();
        self.programs.push(Arc::new(program));
        self.insert_closure(Closure {
            program: index,
            function: 0,
            environment: self.tables.globals(),
            upvalues: Vec::new(),
        })
    }

    fn activation(
        &self,
        callee: &Value,
        arguments: &[Value],
        cached: Value,
        return_to: Option<ReturnTo>,
    ) -> Result<Activation, FaultKind> {
        let closure = self.closure_index(callee)?;
        let target = &self.closures[closure];
        let function = self.programs[target.program]
            .functions
            .get(target.function)
            .ok_or(FaultKind::Function(target.function))?;
        if function.registers > 256 || function.parameters > function.registers {
            return Err(FaultKind::Register(function.registers));
        }
        let mut registers = vec![Value::Nil; function.registers];
        for (destination, argument) in registers
            .iter_mut()
            .take(function.parameters)
            .zip(arguments)
        {
            *destination = argument.clone();
        }
        Ok(Activation {
            closure,
            frame: Frame {
                iterator: None,
                registers,
                cached,
                pc: 0,
                top: function.registers,
            },
            return_to,
            returned_override: None,
        })
    }

    fn read_cell(&self, cell: usize) -> Result<Value, FaultKind> {
        match self.cells.get(cell).ok_or(FaultKind::Upvalue(cell))? {
            Cell::Closed(value) => Ok(value.clone()),
            Cell::Open { frame, register } => self
                .frames
                .get(*frame)
                .and_then(|activation| activation.frame.registers.get(*register))
                .cloned()
                .ok_or(FaultKind::Capture(*register)),
        }
    }

    fn write_cell(&mut self, cell: usize, value: Value) -> Result<(), FaultKind> {
        match self.cells.get_mut(cell).ok_or(FaultKind::Upvalue(cell))? {
            Cell::Closed(destination) => *destination = value,
            Cell::Open { frame, register } => {
                *self
                    .frames
                    .get_mut(*frame)
                    .and_then(|activation| activation.frame.registers.get_mut(*register))
                    .ok_or(FaultKind::Capture(*register))? = value;
            }
        }
        Ok(())
    }

    fn close_frame(
        &mut self,
        frame: usize,
        first: usize,
        registers: &[Value],
    ) -> Result<(), FaultKind> {
        let keys: Vec<_> = self
            .open
            .range((frame, first)..=(frame, usize::MAX))
            .map(|(location, cell)| (*location, *cell))
            .collect();
        for ((_, register), cell) in keys {
            let value = registers
                .get(register)
                .cloned()
                .ok_or(FaultKind::Capture(register))?;
            self.cells[cell] = Cell::Closed(value);
            self.open.remove(&(frame, register));
        }
        Ok(())
    }

    fn unwind(&mut self) {
        self.last_fault = self.frames.last().map(|active| {
            let closure = &self.closures[active.closure];
            (closure.program, closure.function)
        });
        while let Some(activation) = self.frames.pop() {
            let _ = self.close_frame(self.frames.len(), 0, &activation.frame.registers);
        }
    }

    pub fn call(
        &mut self,
        callee: &Value,
        arguments: &[Value],
        budget: usize,
    ) -> Result<Vec<Value>, Fault> {
        self.call_with(callee, arguments, budget, &mut Unbound)
    }

    pub fn call_with(
        &mut self,
        callee: &Value,
        arguments: &[Value],
        budget: usize,
        bindings: &mut dyn Bindings,
    ) -> Result<Vec<Value>, Fault> {
        if !self.frames.is_empty() {
            return Err(Fault {
                pc: 0,
                kind: FaultKind::BindingRequired,
            });
        }
        if let Value::Reference { kind: 10, identity } = callee {
            if budget == 0 {
                return Err(Fault {
                    pc: 0,
                    kind: FaultKind::Budget,
                });
            }
            let mut remaining = budget - 1;
            let result = self
                .start_binding(
                    *identity,
                    arguments,
                    CallSite {
                        cached: Value::Nil,
                        return_to: None,
                        pc: 0,
                    },
                    &mut remaining,
                    bindings,
                )
                .and_then(|values| match values {
                    Some(values) => Ok(values),
                    None => self.drive(remaining, bindings),
                });
            if result.is_err() {
                self.unwind();
            }
            return result;
        }
        let activation = self
            .activation(callee, arguments, Value::Nil, None)
            .map_err(|kind| Fault { pc: 0, kind })?;
        self.frames.push(activation);
        let result = self.drive(budget, bindings);
        if result.is_err() {
            self.unwind();
        }
        result
    }

    fn start_binding(
        &mut self,
        identity: u32,
        arguments: &[Value],
        site: CallSite,
        remaining: &mut usize,
        bindings: &mut dyn Bindings,
    ) -> Result<Option<Vec<Value>>, Fault> {
        let CallSite {
            cached,
            return_to,
            pc,
        } = site;
        let values = bindings
            .call(identity, arguments, &mut self.tables)
            .map_err(|kind| Fault { pc, kind })?;
        let mut lua_callbacks = Vec::new();
        while let Some(callback) = bindings.take_callback() {
            match callback.callee {
                Value::Reference { kind: 10, identity } => {
                    *remaining = remaining.checked_sub(1).ok_or(Fault {
                        pc,
                        kind: FaultKind::Budget,
                    })?;
                    bindings
                        .call(identity, &callback.arguments, &mut self.tables)
                        .map_err(|kind| Fault { pc, kind })?;
                }
                callee => lua_callbacks.push((callee, callback.arguments)),
            }
        }
        if !lua_callbacks.is_empty() {
            let last_index = lua_callbacks.len() - 1;
            for (index, (callee, arguments)) in lua_callbacks.into_iter().enumerate().rev() {
                let is_last = index == last_index;
                let frame_return = if is_last {
                    return_to
                } else {
                    Some(ReturnTo {
                        register: 0,
                        results: Some(0),
                    })
                };
                let mut active = self
                    .activation(&callee, &arguments, cached.clone(), frame_return)
                    .map_err(|kind| Fault { pc, kind })?;
                if is_last {
                    active.returned_override = Some(values.clone());
                }
                self.frames.push(active);
            }
            return Ok(None);
        }
        Ok(Some(values))
    }

    fn drive(
        &mut self,
        mut budget: usize,
        bindings: &mut dyn Bindings,
    ) -> Result<Vec<Value>, Fault> {
        while budget > 0 {
            budget -= 1;
            let mut active = self.frames.pop().ok_or(Fault {
                pc: 0,
                kind: FaultKind::Function(0),
            })?;
            let index = self.frames.len();
            let closure = &self.closures[active.closure];
            let program = self.programs[closure.program].clone();
            let function = &program.functions[closure.function];
            let mut environment = Scope {
                runtime: self,
                closure: active.closure,
                frame: index,
            };
            let step = match active.frame.step_with(function, &mut environment) {
                Ok(step) => step,
                Err(fault) => {
                    self.frames.push(active);
                    return Err(fault);
                }
            };
            match step {
                Step::Continue => self.frames.push(active),
                Step::Call {
                    callee,
                    arguments,
                    register,
                    results,
                } => {
                    let return_to = ReturnTo { register, results };
                    let pc = active.frame.pc - 1;
                    if let Value::Reference { kind: 10, identity } = callee {
                        let cached = active.frame.cached.clone();
                        self.frames.push(active);
                        if let Some(values) = self.start_binding(
                            identity,
                            &arguments,
                            CallSite {
                                cached,
                                return_to: Some(return_to),
                                pc,
                            },
                            &mut budget,
                            bindings,
                        )? {
                            self.finish_call(return_to, &values)?;
                        }
                    } else {
                        let child = self.activation(
                            &callee,
                            &arguments,
                            active.frame.cached.clone(),
                            Some(return_to),
                        );
                        self.frames.push(active);
                        self.frames.push(child.map_err(|kind| Fault { pc, kind })?);
                    }
                }
                Step::Returned(values) => {
                    let values = active.returned_override.take().unwrap_or(values);
                    self.close_frame(index, 0, &active.frame.registers)
                        .map_err(|kind| Fault {
                            pc: active.frame.pc - 1,
                            kind,
                        })?;
                    let Some(return_to) = active.return_to else {
                        return Ok(values);
                    };
                    self.frames
                        .last_mut()
                        .ok_or(Fault {
                            pc: 0,
                            kind: FaultKind::Function(0),
                        })?
                        .frame
                        .cached = active.frame.cached;
                    self.finish_call(return_to, &values)?;
                }
            }
        }
        let pc = self
            .frames
            .last()
            .map_or(0, |activation| activation.frame.pc);
        Err(Fault {
            pc,
            kind: FaultKind::Budget,
        })
    }

    fn finish_call(&mut self, return_to: ReturnTo, values: &[Value]) -> Result<(), Fault> {
        let parent = self.frames.last_mut().ok_or(Fault {
            pc: 0,
            kind: FaultKind::Function(0),
        })?;
        let count = return_to.results.unwrap_or(values.len());
        let end = return_to
            .register
            .checked_add(count)
            .filter(|end| *end <= 256)
            .ok_or(Fault {
                pc: parent.frame.pc,
                kind: FaultKind::Capacity,
            })?;
        if parent.frame.registers.len() < end {
            parent.frame.registers.resize(end, Value::Nil);
        }
        for (index, destination) in parent.frame.registers[return_to.register..end]
            .iter_mut()
            .enumerate()
        {
            *destination = values.get(index).cloned().unwrap_or(Value::Nil);
        }
        parent.frame.top = if return_to.results.is_none() {
            end
        } else {
            let closure = &self.closures[parent.closure];
            self.programs[closure.program].functions[closure.function].registers
        };
        Ok(())
    }
}

struct Scope<'a> {
    runtime: &'a mut Runtime,
    closure: usize,
    frame: usize,
}

impl Scope<'_> {
    fn child(&self, child: usize) -> Result<usize, FaultKind> {
        let closure = &self.runtime.closures[self.closure];
        self.runtime.programs[closure.program].functions[closure.function]
            .children
            .get(child)
            .copied()
            .ok_or(FaultKind::Function(child))
    }

    fn cell(&self, slot: usize) -> Result<usize, FaultKind> {
        self.runtime.closures[self.closure]
            .upvalues
            .get(slot)
            .copied()
            .ok_or(FaultKind::Upvalue(slot))
    }
}

impl Environment for Scope<'_> {
    fn length(&self, value: &Value) -> Result<usize, FaultKind> {
        self.runtime.tables.length(value)
    }
    fn global(&mut self, key: &Value) -> Result<Value, FaultKind> {
        let environment = self.runtime.closures[self.closure].environment.clone();
        self.runtime.tables.field(&environment, key)
    }

    fn field(&mut self, container: &Value, key: &Value) -> Result<Value, FaultKind> {
        self.runtime.tables.field(container, key)
    }

    fn new_table(&mut self) -> Result<Value, FaultKind> {
        self.runtime.tables.allocate()
    }

    fn set_global(&mut self, key: &Value, value: Value) -> Result<(), FaultKind> {
        let environment = self.runtime.closures[self.closure].environment.clone();
        self.runtime.tables.set_field(&environment, key, value)
    }

    fn set_field(&mut self, container: &Value, key: &Value, value: Value) -> Result<(), FaultKind> {
        self.runtime.tables.set_field(container, key, value)
    }

    fn child_upvalues(&self, child: usize) -> Result<usize, FaultKind> {
        let closure = &self.runtime.closures[self.closure];
        self.runtime.programs[closure.program]
            .functions
            .get(self.child(child)?)
            .map(|function| function.upvalues)
            .ok_or(FaultKind::Function(child))
    }

    fn closure(&mut self, child: usize, captures: &[Capture]) -> Result<Value, FaultKind> {
        let function = self.child(child)?;
        let mut upvalues = Vec::with_capacity(captures.len());
        for capture in captures {
            let cell = match *capture {
                Capture::Upvalue(slot) => self.cell(slot)?,
                Capture::Register(register) => {
                    let location = (self.frame, register);
                    if let Some(cell) = self.runtime.open.get(&location) {
                        *cell
                    } else {
                        let cell = self.runtime.cells.len();
                        self.runtime.cells.push(Cell::Open {
                            frame: self.frame,
                            register,
                        });
                        self.runtime.open.insert(location, cell);
                        cell
                    }
                }
            };
            upvalues.push(cell);
        }
        let parent = &self.runtime.closures[self.closure];
        self.runtime.insert_closure(Closure {
            program: parent.program,
            function,
            environment: parent.environment.clone(),
            upvalues,
        })
    }

    fn upvalue(&self, slot: usize) -> Result<Value, FaultKind> {
        self.runtime.read_cell(self.cell(slot)?)
    }

    fn set_upvalue(&mut self, slot: usize, value: Value) -> Result<(), FaultKind> {
        self.runtime.write_cell(self.cell(slot)?, value)
    }

    fn close(&mut self, first: usize, registers: &[Value]) -> Result<(), FaultKind> {
        self.runtime.close_frame(self.frame, first, registers)
    }
}
