//! Names a zone walk has seen loaded, by the slots that loaded them: what a
//! later reference to a slot (an alias, a pointer to its header) names.

use alloc::collections::BTreeMap;

use crate::zone::{Ptr, ZonePtr, ZoneStream};

#[derive(Clone, Debug, Default)]
pub struct LinkNames {
    names: BTreeMap<Ptr, Ptr>,
}

impl LinkNames {
    /// The name the asset loaded at `slot` (and at its insert slot) has.
    pub fn remember(&mut self, slot: Ptr, insert_slot: Option<Ptr>, name: Ptr) {
        self.names.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.names.insert(insert_slot, name);
        }
    }

    pub fn get(&self, slot: Ptr) -> Option<Ptr> {
        self.names.get(&slot).copied()
    }

    /// An alias names what its target names, when the target has a name.
    pub fn alias(&mut self, slot: Ptr, target: Ptr) {
        if let Some(name) = self.get(target) {
            self.names.insert(slot, name);
        }
    }

    /// An alias names what its target names, and nothing when the target
    /// has no name.
    pub fn alias_or_forget(&mut self, slot: Ptr, target: Ptr) {
        if let Some(name) = self.get(target) {
            self.names.insert(slot, name);
        } else {
            self.names.remove(&slot);
        }
    }

    /// The name of the asset an asset field at `slot` points at: the slot's
    /// own name, or the name of the asset its pointer resolves to.
    pub fn name_at<'s>(&self, s: &'s ZoneStream<'_>, slot: Ptr) -> Option<&'s str> {
        let name = self.get(slot).or_else(|| {
            let ZonePtr::Offset(target) = s.ptr_at(slot, 0).ok()? else {
                return None;
            };
            self.get(s.resolve_alias(target))
        })?;
        s.cstr(s.resolve_alias(name)).ok()
    }
}
