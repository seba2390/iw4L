//! A `StringTable` read out of the zone.

use alloc::string::String;
use alloc::vec::Vec;

use crate::size as sz;
use crate::zone::{Ptr, ZonePtr, ZoneStream};

pub struct StringTableRead {
    pub name: String,
    pub columns: usize,
    pub rows: usize,
    /// Row by row; empty when the table has no cell array.
    pub cells: Vec<String>,
}

impl ZoneStream<'_> {
    /// The string table whose header is at `header`; `None` when it has no
    /// name. A cell or name that does not read is empty.
    pub fn read_string_table(&self, header: Ptr) -> Option<StringTableRead> {
        let text = |p| String::from(self.cstr(self.resolve_alias(p)).ok().unwrap_or(""));
        let name = match self.ptr_at(header, 0).ok() {
            Some(ZonePtr::Offset(p)) => text(p),
            _ => return None,
        };
        let columns = self.i32_at(header, 4).unwrap_or(0).max(0) as usize;
        let rows = self.i32_at(header, 8).unwrap_or(0).max(0) as usize;
        let arr = match self.ptr_at(header, 12).ok() {
            Some(ZonePtr::Offset(p)) => self.resolve_alias(p),
            _ => {
                return Some(StringTableRead {
                    name,
                    columns,
                    rows,
                    cells: Vec::new(),
                });
            }
        };
        let cells = (0..columns.saturating_mul(rows))
            .map(
                |i| match self.ptr_at(arr.at(i * sz::STRING_TABLE_CELL), 0).ok() {
                    Some(ZonePtr::Offset(p)) => text(p),
                    _ => String::new(),
                },
            )
            .collect();
        Some(StringTableRead {
            name,
            columns,
            rows,
            cells,
        })
    }
}
