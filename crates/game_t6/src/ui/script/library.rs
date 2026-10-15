use super::runtime::Bindings;
use super::tables::Tables;
use super::{FaultKind, Value};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
pub struct Library;

fn bytes(value: &[u8]) -> Value {
    Value::Bytes(Arc::from(value))
}

fn argument(arguments: &[Value], index: usize) -> Result<&Value, FaultKind> {
    arguments.get(index).ok_or(FaultKind::Argument(index + 1))
}

fn table(arguments: &[Value], index: usize) -> Result<&Value, FaultKind> {
    match argument(arguments, index)? {
        value @ Value::Reference { kind: 5, .. } => Ok(value),
        _ => Err(FaultKind::Argument(index + 1)),
    }
}

pub const PCALL: u32 = 13;
pub const XPCALL: u32 = 14;
const SELECT: u32 = 15;
const COLLECTGARBAGE: u32 = 16;
const RAWEQUAL: u32 = 17;

impl Library {
    pub fn install(tables: &mut Tables) -> Result<Self, FaultKind> {
        let globals = tables.globals();
        for (identity, name) in [
            (1, b"type".as_slice()),
            (2, b"rawget"),
            (3, b"rawset"),
            (4, b"getmetatable"),
            (5, b"setmetatable"),
            (6, b"assert"),
            (PCALL, b"pcall"),
            (XPCALL, b"xpcall"),
            (SELECT, b"select"),
            (COLLECTGARBAGE, b"collectgarbage"),
            (RAWEQUAL, b"rawequal"),
        ] {
            tables.raw_set(
                &globals,
                &bytes(name),
                Value::Reference { kind: 10, identity },
            )?;
        }
        Ok(Self)
    }

    fn type_name(value: &Value) -> Result<Value, FaultKind> {
        let name: &[u8] = match value {
            Value::Nil => b"nil",
            Value::Boolean(_) => b"boolean",
            Value::Number(_) => b"number",
            Value::Bytes(_) => b"string",
            Value::Reference { kind: 2 | 7, .. } => b"userdata",
            Value::Reference { kind: 5, .. } => b"table",
            Value::Reference {
                kind: 6 | 9 | 10, ..
            } => b"function",
            Value::Reference { kind: 8, .. } => b"thread",
            Value::Reference { kind: 11, .. } => b"ui64",
            Value::Reference { kind: 12, .. } => b"struct",
            Value::Reference { kind: 15, .. } => b"object",
            _ => return Err(FaultKind::BindingRequired),
        };
        Ok(bytes(name))
    }

    fn protected(tables: &Tables, metatable: &Value) -> Result<Value, FaultKind> {
        tables.raw_get(metatable, &bytes(b"__metatable"))
    }
}

impl Bindings for Library {
    fn call(
        &mut self,
        function: u32,
        arguments: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        match function {
            1 => Ok(vec![Self::type_name(
                arguments.first().unwrap_or(&Value::Nil),
            )?]),
            2 => Ok(vec![
                tables.raw_get(table(arguments, 0)?, argument(arguments, 1)?)?,
            ]),
            3 => {
                tables.raw_set(
                    table(arguments, 0)?,
                    argument(arguments, 1)?,
                    argument(arguments, 2)?.clone(),
                )?;
                Ok(Vec::new())
            }
            4 => {
                let container = argument(arguments, 0)?;
                let Some(metatable) = tables.metatable(container)? else {
                    return Ok(vec![Value::Nil]);
                };
                let protected = Self::protected(tables, &metatable)?;
                Ok(vec![if matches!(protected, Value::Nil) {
                    metatable
                } else {
                    protected
                }])
            }
            5 => {
                let container = table(arguments, 0)?;
                let metatable = match argument(arguments, 1)? {
                    Value::Nil => None,
                    value @ Value::Reference { kind: 5, .. } => Some(value.clone()),
                    _ => return Err(FaultKind::Argument(2)),
                };
                if let Some(current) = tables.metatable(container)?
                    && !matches!(Self::protected(tables, &current)?, Value::Nil)
                {
                    return Err(FaultKind::ProtectedMetatable);
                }
                tables.set_metatable(container, metatable)?;
                Ok(vec![container.clone()])
            }
            6 => {
                let condition = argument(arguments, 0)?;
                let message = match arguments.get(1) {
                    None | Some(Value::Nil) => b"assertion failed!".as_slice(),
                    Some(Value::Bytes(value)) => value.as_ref(),
                    _ => return Err(FaultKind::BindingRequired),
                };
                if condition.truthy() {
                    Ok(arguments.to_vec())
                } else {
                    Err(FaultKind::ScriptError(message.to_vec()))
                }
            }
            SELECT => match argument(arguments, 0)? {
                Value::Bytes(value) if value.as_ref() == b"#" => {
                    Ok(vec![Value::Number((arguments.len() - 1) as f32)])
                }
                Value::Number(value) if value.is_finite() => {
                    let count = arguments.len().saturating_sub(1) as isize;
                    let mut index = *value as isize;
                    if index < 0 {
                        index += count + 1;
                    }
                    if index < 1 {
                        return Err(FaultKind::Argument(1));
                    }
                    let start = (index as usize).min(arguments.len());
                    Ok(arguments[start..].to_vec())
                }
                _ => Err(FaultKind::Argument(1)),
            },
            COLLECTGARBAGE => Ok(vec![Value::Number(0.0)]),
            RAWEQUAL => Ok(vec![Value::Boolean(u32::from(
                argument(arguments, 0)? == argument(arguments, 1)?,
            ))]),
            _ => Err(FaultKind::Unknown(super::ENGINE_API)),
        }
    }
}
