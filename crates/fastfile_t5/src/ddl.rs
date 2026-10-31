//! A `ddl` asset (Black Ops' stats layout) read out of the zone.

use alloc::string::String;
use alloc::vec::Vec;

use crate::zone::{Ptr, ZonePtr, ZoneStream};

pub struct DdlRead {
    pub name: String,
    /// Each version of the layout, in the zone's order.
    pub defs: Vec<DdlDefRead>,
}

/// One version of the layout: its version number, its size in bits, its
/// structs (the first is the root) and its enums.
pub struct DdlDefRead {
    pub version: u32,
    pub bits: u32,
    pub structs: Vec<DdlStructRead>,
    pub enums: Vec<DdlEnumRead>,
}

pub struct DdlStructRead {
    pub name: String,
    pub size: u32,
    /// Sorted by name, as the game searches them.
    pub members: Vec<DdlMemberRead>,
}

/// A member: its size in bits (all of an array) and offset in bits, its
/// type, the struct it holds, the range a stored integer may take (it is
/// kept less `min`), its array length, the enum that names its indices and
/// who may write it (1 client, 2 server, 3 both). Words 7 and 8 are not
/// read.
pub struct DdlMemberRead {
    pub name: String,
    pub bits: u32,
    pub offset: u32,
    pub kind: u32,
    pub external: i32,
    pub min: u32,
    pub max: u32,
    pub array_size: i32,
    pub enum_index: i32,
    pub permission: u32,
}

pub struct DdlEnumRead {
    pub name: String,
    pub members: Vec<String>,
}

/// A guard against a version list that loops.
const MAX_DEFS: usize = 64;
const STRUCT_SIZE: usize = 16;
const MEMBER_SIZE: usize = 48;
const ENUM_SIZE: usize = 12;

impl ZoneStream<'_> {
    /// The layout whose header is at `root`; `None` when it has no name.
    pub fn read_ddl(&self, root: Ptr) -> Option<DdlRead> {
        let target = |p: Ptr, field: usize| match self.ptr_at(p, field).ok() {
            Some(ZonePtr::Offset(q)) => Some(self.resolve_alias(q)),
            _ => None,
        };
        let text = |p: Ptr| {
            target(p, 0)
                .and_then(|q| self.cstr(q).ok())
                .map(String::from)
                .unwrap_or_else(String::new)
        };
        let word = |p: Ptr, field: usize| self.u32_at(p, field).unwrap_or(0);
        target(root, 0)?;
        let name = text(root);
        let mut defs = Vec::new();
        let mut next = target(root, 4);
        while let Some(def) = next {
            let structs = target(def, 8).map_or_else(Vec::new, |array| {
                (0..word(def, 12) as usize)
                    .map(|i| {
                        let s = array.at(i * STRUCT_SIZE);
                        let members = target(s, 12).map_or_else(Vec::new, |members| {
                            (0..word(s, 8) as usize)
                                .map(|j| {
                                    let m = members.at(j * MEMBER_SIZE);
                                    DdlMemberRead {
                                        name: text(m),
                                        bits: word(m, 4),
                                        offset: word(m, 8),
                                        kind: word(m, 12),
                                        external: word(m, 16) as i32,
                                        min: word(m, 20),
                                        max: word(m, 24),
                                        array_size: word(m, 36) as i32,
                                        enum_index: word(m, 40) as i32,
                                        permission: word(m, 44),
                                    }
                                })
                                .collect()
                        });
                        DdlStructRead {
                            name: text(s),
                            size: word(s, 4),
                            members,
                        }
                    })
                    .collect()
            });
            let enums = target(def, 16).map_or_else(Vec::new, |array| {
                (0..word(def, 20) as usize)
                    .map(|i| {
                        let e = array.at(i * ENUM_SIZE);
                        let members = target(e, 8).map_or_else(Vec::new, |names| {
                            (0..word(e, 4) as usize)
                                .map(|j| text(names.at(j * 4)))
                                .collect()
                        });
                        DdlEnumRead {
                            name: text(e),
                            members,
                        }
                    })
                    .collect()
            });
            defs.push(DdlDefRead {
                version: word(def, 0),
                bits: word(def, 4),
                structs,
                enums,
            });
            next = target(def, 24).filter(|_| defs.len() < MAX_DEFS);
        }
        Some(DdlRead { name, defs })
    }
}
