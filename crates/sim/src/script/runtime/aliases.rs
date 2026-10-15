//! Copies of one array that the engine would still share.
//!
//! The engine shares an array between every variable, field and argument it
//! is stored in, and copies it only when a script writes to a shared one.
//! This VM copies eagerly instead (`copy_value`), which reads the same until
//! a builtin edits an array where it is (`arrayinsert`, `arrayremoveindex`,
//! `arrayremovevalue`): every holder of the shared array sees that edit. So
//! every eager copy joins its original's group, a script write leaves it, and
//! an in-place builtin is replayed on the rest of the group.

use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Default)]
pub(crate) struct ArrayAliases {
    group: HashMap<u64, u64>,
    members: HashMap<u64, BTreeSet<u64>>,
    next_group: u64,
}

impl ArrayAliases {
    /// `copy` was copied from `original` and shares it.
    pub(crate) fn join(&mut self, original: u64, copy: u64) {
        let group = match self.group.get(&original) {
            Some(group) => *group,
            None => {
                let group = self.next_group;
                self.next_group += 1;
                self.group.insert(original, group);
                self.members.entry(group).or_default().insert(original);
                group
            }
        };
        self.group.insert(copy, group);
        self.members.entry(group).or_default().insert(copy);
    }

    /// A script wrote to `id`, which now holds its own copy.
    pub(crate) fn leave(&mut self, id: u64) {
        let Some(group) = self.group.remove(&id) else {
            return;
        };
        let members = self.members.get_mut(&group).unwrap();
        members.remove(&id);
        if members.len() < 2 {
            for member in std::mem::take(members) {
                self.group.remove(&member);
            }
            self.members.remove(&group);
        }
    }

    /// The other arrays that share `id`.
    pub(crate) fn others(&self, id: u64) -> Vec<u64> {
        let members = self
            .group
            .get(&id)
            .and_then(|group| self.members.get(group));
        members
            .into_iter()
            .flatten()
            .copied()
            .filter(|member| *member != id)
            .collect()
    }

    /// Drops the arrays the heap no longer holds.
    pub(crate) fn retain(&mut self, live: impl Fn(u64) -> bool) {
        let dead: Vec<u64> = self.group.keys().copied().filter(|id| !live(*id)).collect();
        for id in dead {
            self.leave(id);
        }
    }
}
