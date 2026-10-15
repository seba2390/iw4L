use super::{Environment, FaultKind, FieldLookup, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Boolean(u32),
    Number(u32),
    Bytes(Arc<[u8]>),
    Reference(u8, u32),
}

impl Key {
    fn read(value: &Value) -> Result<Option<Self>, FaultKind> {
        match value {
            Value::Nil => Ok(None),
            Value::Boolean(value) => Ok(Some(Self::Boolean(*value))),
            Value::Number(value) if value.is_nan() => Ok(None),
            Value::Number(value) => Ok(Some(Self::Number(if *value == 0.0 {
                0
            } else {
                value.to_bits()
            }))),
            Value::Bytes(value) => Ok(Some(Self::Bytes(value.clone()))),
            Value::Reference { kind, identity } => Ok(Some(Self::Reference(*kind, *identity))),
            Value::Hash(_) => Err(FaultKind::KeyRequired),
        }
    }

    fn write(value: &Value) -> Result<Self, FaultKind> {
        Self::read(value)?.ok_or(if matches!(value, Value::Nil) {
            FaultKind::NilKey
        } else {
            FaultKind::NanKey
        })
    }
}

#[derive(Clone, Debug, Default)]
struct Table {
    entries: BTreeMap<Key, Value>,
    metatable: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Tables {
    tables: Vec<Table>,
    userdata: Vec<Option<Value>>,
    globals: Value,
}

impl Default for Tables {
    fn default() -> Self {
        Self {
            tables: vec![Table::default()],
            userdata: Vec::new(),
            globals: Value::Reference {
                kind: 5,
                identity: 1,
            },
        }
    }
}

impl Tables {
    pub fn entries(&self, container: &Value) -> Result<Vec<(Value, Value)>, FaultKind> {
        let table = &self.tables[self.index(container)?];
        Ok(table
            .entries
            .iter()
            .map(|(key, value)| {
                let key = match key {
                    Key::Boolean(value) => Value::Boolean(*value),
                    Key::Number(value) => Value::Number(f32::from_bits(*value)),
                    Key::Bytes(value) => Value::Bytes(value.clone()),
                    Key::Reference(kind, identity) => Value::Reference {
                        kind: *kind,
                        identity: *identity,
                    },
                };
                (key, value.clone())
            })
            .collect())
    }
    fn index(&self, value: &Value) -> Result<usize, FaultKind> {
        let Value::Reference { kind: 5, identity } = value else {
            return Err(FaultKind::BindingRequired);
        };
        identity
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.tables.len())
            .ok_or(FaultKind::Table(*identity))
    }

    fn userdata_index(&self, value: &Value) -> Result<usize, FaultKind> {
        let Value::Reference { kind: 7, identity } = value else {
            return Err(FaultKind::BindingRequired);
        };
        identity
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.userdata.len())
            .ok_or(FaultKind::Userdata(*identity))
    }

    pub fn allocate_userdata(&mut self, metatable: Option<Value>) -> Result<Value, FaultKind> {
        if let Some(value) = &metatable {
            self.index(value)?;
        }
        let identity = u32::try_from(self.userdata.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(FaultKind::Capacity)?;
        self.userdata.push(metatable);
        Ok(Value::Reference { kind: 7, identity })
    }

    pub fn globals(&self) -> Value {
        self.globals.clone()
    }

    pub fn allocate(&mut self) -> Result<Value, FaultKind> {
        let identity = u32::try_from(self.tables.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(FaultKind::Capacity)?;
        self.tables.push(Table::default());
        Ok(Value::Reference { kind: 5, identity })
    }

    pub fn raw_get(&self, container: &Value, key: &Value) -> Result<Value, FaultKind> {
        let index = self.index(container)?;
        let Some(key) = Key::read(key)? else {
            return Ok(Value::Nil);
        };
        Ok(self.tables[index]
            .entries
            .get(&key)
            .cloned()
            .unwrap_or(Value::Nil))
    }

    pub fn raw_set(
        &mut self,
        container: &Value,
        key: &Value,
        value: Value,
    ) -> Result<(), FaultKind> {
        let index = self.index(container)?;
        self.tables[index].entries.insert(Key::write(key)?, value);
        Ok(())
    }

    pub fn metatable(&self, container: &Value) -> Result<Option<Value>, FaultKind> {
        if matches!(container, Value::Reference { kind: 7, .. }) {
            Ok(self.userdata[self.userdata_index(container)?].clone())
        } else {
            Ok(self.tables[self.index(container)?].metatable.clone())
        }
    }

    pub fn set_metatable(
        &mut self,
        container: &Value,
        metatable: Option<Value>,
    ) -> Result<(), FaultKind> {
        if let Some(value) = &metatable {
            self.index(value)?;
        }
        if matches!(container, Value::Reference { kind: 7, .. }) {
            let index = self.userdata_index(container)?;
            self.userdata[index] = metatable;
        } else {
            let index = self.index(container)?;
            self.tables[index].metatable = metatable;
        }
        Ok(())
    }

    pub fn metafield(&self, container: &Value, name: &[u8]) -> Result<Value, FaultKind> {
        if !matches!(container, Value::Reference { kind: 5 | 7, .. }) {
            return Ok(Value::Nil);
        }
        match self.metatable(container)? {
            Some(table) => self.raw_get(&table, &Value::Bytes(Arc::from(name))),
            None => Ok(Value::Nil),
        }
    }
}

impl Environment for Tables {
    fn length(&self, value: &Value) -> Result<usize, FaultKind> {
        if let Value::Bytes(value) = value {
            return Ok(value.len());
        }
        let count = self.tables[self.index(value)?].entries.len();
        for index in 1..=count + 1 {
            if self.raw_get(value, &Value::Number(index as f32))? == Value::Nil {
                return Ok(index - 1);
            }
        }
        Err(FaultKind::Capacity)
    }
    fn global(&mut self, key: &Value) -> Result<Value, FaultKind> {
        self.field(&self.globals.clone(), key)
    }

    fn field(&mut self, container: &Value, key: &Value) -> Result<Value, FaultKind> {
        match self.lookup_field(container, key)? {
            FieldLookup::Value(value) => Ok(value),
            FieldLookup::MetaCall { .. } => Err(FaultKind::BindingRequired),
        }
    }

    fn lookup_global(&mut self, key: &Value) -> Result<FieldLookup, FaultKind> {
        self.lookup_field(&self.globals.clone(), key)
    }

    fn lookup_field(&mut self, container: &Value, key: &Value) -> Result<FieldLookup, FaultKind> {
        let mut current = container.clone();
        for _ in 0..=32 {
            if matches!(current, Value::Reference { kind: 5, .. }) {
                let value = self.raw_get(&current, key)?;
                if !matches!(value, Value::Nil) {
                    return Ok(FieldLookup::Value(value));
                }
            }
            if matches!(current, Value::Bytes(_)) {
                let library = self.raw_get(
                    &self.globals,
                    &Value::Bytes(std::sync::Arc::from(b"string".as_slice())),
                )?;
                return self.raw_get(&library, key).map(FieldLookup::Value);
            }
            let index = self.metafield(&current, b"__index")?;
            if matches!(index, Value::Reference { kind: 9 | 10, .. })
                || (matches!(index, Value::Reference { kind: 5 | 7, .. })
                    && !matches!(self.metafield(&index, b"__call")?, Value::Nil))
            {
                return Ok(FieldLookup::MetaCall {
                    callee: index,
                    arguments: vec![current, key.clone()],
                });
            }
            if matches!(index, Value::Nil) {
                return if matches!(current, Value::Reference { kind: 5, .. }) {
                    Ok(FieldLookup::Value(Value::Nil))
                } else {
                    Err(FaultKind::BindingRequired)
                };
            }
            self.index(&index)?;
            current = index;
        }
        Err(FaultKind::IndexChain)
    }

    fn new_table(&mut self) -> Result<Value, FaultKind> {
        self.allocate()
    }

    fn set_global(&mut self, key: &Value, value: Value) -> Result<(), FaultKind> {
        self.set_field(&self.globals.clone(), key, value)
    }

    fn set_field(&mut self, container: &Value, key: &Value, value: Value) -> Result<(), FaultKind> {
        match self.assign_field(container, key, value)? {
            None => Ok(()),
            Some(_) => Err(FaultKind::BindingRequired),
        }
    }

    fn assign_global(
        &mut self,
        key: &Value,
        value: Value,
    ) -> Result<Option<(Value, Vec<Value>)>, FaultKind> {
        self.assign_field(&self.globals.clone(), key, value)
    }

    fn assign_field(
        &mut self,
        container: &Value,
        key: &Value,
        value: Value,
    ) -> Result<Option<(Value, Vec<Value>)>, FaultKind> {
        let mut current = container.clone();
        for _ in 0..=32 {
            if matches!(current, Value::Reference { kind: 5, .. })
                && !matches!(self.raw_get(&current, key)?, Value::Nil)
            {
                self.raw_set(&current, key, value)?;
                return Ok(None);
            }
            let newindex = self.metafield(&current, b"__newindex")?;
            if matches!(newindex, Value::Nil) {
                self.raw_set(&current, key, value)?;
                return Ok(None);
            }
            if matches!(newindex, Value::Reference { kind: 9 | 10, .. })
                || (matches!(newindex, Value::Reference { kind: 5 | 7, .. })
                    && !matches!(self.metafield(&newindex, b"__call")?, Value::Nil))
            {
                return Ok(Some((newindex, vec![current, key.clone(), value])));
            }
            self.index(&newindex)?;
            current = newindex;
        }
        Err(FaultKind::IndexChain)
    }

    fn equal_metamethod(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>, FaultKind> {
        let tm1 = self.metafield(left, b"__eq")?;
        if matches!(tm1, Value::Nil) {
            return Ok(None);
        }
        let tm2 = self.metafield(right, b"__eq")?;
        Ok((tm1 == tm2).then_some(tm1))
    }
}
