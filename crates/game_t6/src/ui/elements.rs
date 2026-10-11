use super::Element;
use super::animation::AnimationState;
use super::script::library::Library;
use super::script::runtime::{Bindings, Callback};
use super::script::tables::Tables;
use super::script::{Environment, FaultKind, Value};
use game_api::{Unknown, unknown};
use std::collections::BTreeMap;
use std::sync::Arc;

const GC: &Unknown = unknown!(
    "t6.ui.gc",
    "LUI elements and animation states are never collected",
    "Black Ops 2's LUI element and animation-state collectors"
);
const TIMED_TRANSITIONS: &Unknown = unknown!(
    "t6.ui.timed_transitions",
    "animateToState with a duration, or while a transition runs",
    "Black Ops 2's LUI transition timing and interpolation"
);

const CONSTRUCT: u32 = 7;
const COLLECT: u32 = 8;
const SET_LAYOUT_CACHED: u32 = 9;
const REGISTER_ANIMATION_STATE: u32 = 10;
const COLLECT_ANIMATION_STATE: u32 = 11;
const ANIMATE_TO_STATE: u32 = 12;
const GET_LOCAL_RECTANGLE: u32 = 13;

fn key(name: &[u8]) -> Value {
    Value::Bytes(Arc::from(name))
}

#[derive(Debug)]
pub struct Elements {
    library: Library,
    elements: BTreeMap<u32, Element>,
    animation_states: BTreeMap<u32, AnimationState>,
    animation_state_metatable: Value,
    callback: Option<Callback>,
}

impl Elements {
    pub fn install(tables: &mut Tables) -> Result<Self, FaultKind> {
        let library = Library::install(tables)?;
        let animation_state_metatable = tables.allocate()?;
        tables.raw_set(
            &animation_state_metatable,
            &key(b"__gc"),
            Value::Reference {
                kind: 10,
                identity: COLLECT_ANIMATION_STATE,
            },
        )?;
        tables.raw_set(
            &tables.globals(),
            &key(b"ConstructLUIElement"),
            Value::Reference {
                kind: 10,
                identity: CONSTRUCT,
            },
        )?;
        Ok(Self {
            library,
            elements: BTreeMap::new(),
            animation_states: BTreeMap::new(),
            animation_state_metatable,
            callback: None,
        })
    }

    pub fn element(&self, value: &Value) -> Result<&Element, FaultKind> {
        let Value::Reference { kind: 7, identity } = value else {
            return Err(FaultKind::BindingRequired);
        };
        self.elements
            .get(identity)
            .ok_or(FaultKind::Userdata(*identity))
    }

    pub fn bind_methods(tables: &mut Tables, class: &Value) -> Result<(), FaultKind> {
        for (name, identity) in [
            (b"setLayoutCached".as_slice(), SET_LAYOUT_CACHED),
            (b"registerAnimationState", REGISTER_ANIMATION_STATE),
            (b"animateToState", ANIMATE_TO_STATE),
            (b"getLocalRect", GET_LOCAL_RECTANGLE),
        ] {
            tables.raw_set(class, &key(name), Value::Reference { kind: 10, identity })?;
        }
        Ok(())
    }

    pub fn animation_state(&self, value: &Value) -> Result<&AnimationState, FaultKind> {
        let Value::Reference { kind: 7, identity } = value else {
            return Err(FaultKind::BindingRequired);
        };
        self.animation_states
            .get(identity)
            .ok_or(FaultKind::Userdata(*identity))
    }

    fn register_animation_state(
        &mut self,
        arguments: &[Value],
        tables: &mut Tables,
    ) -> Result<(), FaultKind> {
        let receiver = arguments.first().ok_or(FaultKind::Argument(1))?;
        let states = tables.field(receiver, &key(b"m_animationStates"))?;
        let name = arguments.get(1).ok_or(FaultKind::Argument(2))?;
        let source = arguments.get(2).ok_or(FaultKind::BindingRequired)?;
        let state = match source {
            Value::Reference { kind: 5, .. } => AnimationState::parse(tables, source)?,
            Value::Reference { kind: 7, .. } => self.animation_state(source)?.clone(),
            Value::Reference { kind: 2, .. } => return Err(FaultKind::BindingRequired),
            _ => return Ok(()),
        };
        let value = tables.allocate_userdata(Some(self.animation_state_metatable.clone()))?;
        let Value::Reference { identity, .. } = &value else {
            return Err(FaultKind::BindingRequired);
        };
        self.animation_states.insert(*identity, state);
        tables.set_field(&states, name, value)
    }

    fn set_layout_cached(&mut self, arguments: &[Value]) -> Result<(), FaultKind> {
        let Some(Value::Reference { kind: 7, identity }) = arguments.first() else {
            return Err(FaultKind::Argument(1));
        };
        let element = self
            .elements
            .get_mut(identity)
            .ok_or(FaultKind::Userdata(*identity))?;
        element.current.cached =
            matches!(arguments.get(1), Some(Value::Boolean(value)) if *value != 0);
        Ok(())
    }

    fn animate_to_state(
        &mut self,
        arguments: &[Value],
        tables: &mut Tables,
    ) -> Result<(), FaultKind> {
        let receiver = arguments.first().ok_or(FaultKind::Argument(1))?;
        let Value::Reference { kind: 7, identity } = receiver else {
            return Err(FaultKind::Argument(1));
        };
        let element = self
            .elements
            .get(identity)
            .ok_or(FaultKind::Userdata(*identity))?;
        if element.animation_target.is_some() {
            return Err(FaultKind::Unknown(TIMED_TRANSITIONS));
        }
        match arguments.get(2) {
            None | Some(Value::Nil | Value::Boolean(_) | Value::Reference { .. }) => {}
            Some(Value::Number(value)) if value.is_finite() && value.trunc() == 0.0 => {}
            Some(Value::Number(_)) => return Err(FaultKind::Unknown(TIMED_TRANSITIONS)),
            _ => return Err(FaultKind::BindingRequired),
        }
        let name = match arguments.get(1) {
            Some(Value::Bytes(bytes)) => key(&bytes[..bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len())]),
            None | Some(Value::Nil | Value::Boolean(_) | Value::Reference { .. }) => Value::Nil,
            _ => return Err(FaultKind::BindingRequired),
        };
        let states = tables.field(receiver, &key(b"m_animationStates"))?;
        let value = tables.field(&states, &name)?;
        let options = [
            arguments.get(3).is_some_and(Value::truthy),
            arguments.get(4).is_some_and(Value::truthy),
        ];
        self.elements
            .get_mut(identity)
            .ok_or(FaultKind::Userdata(*identity))?
            .animation_options = options;
        if value == Value::Nil {
            return Ok(());
        }
        let state = self.animation_state(&value)?.clone();
        let element = self
            .elements
            .get_mut(identity)
            .ok_or(FaultKind::Userdata(*identity))?;
        element.current.cached = false;
        state.apply(&mut element.current);
        let Value::Bytes(name) = name else {
            return Err(FaultKind::BindingRequired);
        };
        let event_name = key(&[b"transition_complete_".as_slice(), &name].concat());
        let handlers = tables.field(receiver, &key(b"m_eventHandlers"))?;
        let callee = tables.field(&handlers, &event_name)?;
        if callee != Value::Nil {
            let event = tables.allocate()?;
            tables.raw_set(&event, &key(b"name"), event_name)?;
            tables.raw_set(&event, &key(b"lateness"), Value::Number(0.0))?;
            self.callback = Some(Callback {
                callee,
                arguments: vec![receiver.clone(), event],
            });
        }
        Ok(())
    }

    fn construct(&mut self, tables: &mut Tables) -> Result<Value, FaultKind> {
        let fields = tables.allocate()?;
        for name in [
            b"m_references".as_slice(),
            b"m_animationStates",
            b"m_eventHandlers",
        ] {
            let table = tables.allocate()?;
            tables.raw_set(&fields, &key(name), table)?;
        }
        let metatable = tables.allocate()?;
        for name in [b"__index".as_slice(), b"__newindex"] {
            tables.raw_set(&metatable, &key(name), fields.clone())?;
        }
        tables.raw_set(
            &metatable,
            &key(b"__gc"),
            Value::Reference {
                kind: 10,
                identity: COLLECT,
            },
        )?;
        let value = tables.allocate_userdata(Some(metatable))?;
        let Value::Reference { identity, .. } = &value else {
            return Err(FaultKind::BindingRequired);
        };
        self.elements.insert(*identity, Element::default());
        Ok(value)
    }
}

impl Bindings for Elements {
    fn take_callback(&mut self) -> Option<Callback> {
        self.callback.take()
    }

    fn call(
        &mut self,
        function: u32,
        arguments: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        match function {
            CONSTRUCT => Ok(vec![self.construct(tables)?]),
            GET_LOCAL_RECTANGLE => {
                let receiver = arguments.first().ok_or(FaultKind::Argument(1))?;
                Ok(self
                    .element(receiver)?
                    .local_rectangle()
                    .into_iter()
                    .map(Value::Number)
                    .collect())
            }
            COLLECT | COLLECT_ANIMATION_STATE => Err(FaultKind::Unknown(GC)),
            SET_LAYOUT_CACHED => {
                self.set_layout_cached(arguments)?;
                Ok(Vec::new())
            }
            REGISTER_ANIMATION_STATE => {
                self.register_animation_state(arguments, tables)?;
                Ok(Vec::new())
            }
            ANIMATE_TO_STATE => {
                self.animate_to_state(arguments, tables)?;
                Ok(Vec::new())
            }
            _ => self.library.call(function, arguments, tables),
        }
    }
}
