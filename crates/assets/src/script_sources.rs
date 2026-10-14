use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct ScriptSources {
    sources: BTreeMap<String, ScriptSource>,
    /// Compiled modules a game runs as they are, beside any source of the same name.
    modules: BTreeMap<String, Vec<u8>>,
    tables: BTreeMap<String, ScriptTable>,
    schemas: BTreeMap<String, std::sync::Arc<structured_data_iw4::DefinitionSet>>,
    layouts: BTreeMap<String, std::sync::Arc<asset_core::BitLayout>>,
    barriers: BTreeMap<String, std::sync::Arc<asset_core::BarrierType>>,
    weapons: BTreeMap<String, std::sync::Arc<asset_core::ZoneWeapon>>,
    entities: Option<String>,
    configs: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptSourceOrigin {
    Packaged,
    BuiltIn,
}

#[derive(Clone, Debug)]
struct ScriptSource {
    bytes: Result<Vec<u8>, String>,
    origin: ScriptSourceOrigin,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptTable {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}

fn normalize(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

impl ScriptSources {
    pub fn read(&self, module: &str) -> Result<Vec<u8>, String> {
        self.sources
            .get(module)
            .map(|source| source.bytes.clone())
            .unwrap_or_else(|| Err(format!("missing script asset {module}.gsc")))
    }

    pub fn origin(&self, module: &str) -> Option<ScriptSourceOrigin> {
        self.sources.get(module).map(|source| source.origin)
    }

    pub fn module(&self, name: &str) -> Option<&[u8]> {
        self.modules.get(name).map(Vec::as_slice)
    }

    /// A compiled server module (`maps/mp/x.gsc` is read as `maps/mp/x`); a
    /// client module keeps its extension (`clientscripts/mp/x.csc`), as a
    /// client module may share its name with a server one.
    pub(crate) fn capture_module(&mut self, name: &str, bytes: Vec<u8>) {
        let name = normalize(name);
        if let Some(module) = name.strip_suffix(".gsc") {
            self.modules.insert(module.to_owned(), bytes);
        } else if name.ends_with(".csc") {
            self.modules.insert(name, bytes);
        }
    }

    /// The compiled modules and tables of `other`, which a game reads as they
    /// are; nothing when it has no compiled modules.
    pub(crate) fn keep_compiled(&mut self, other: &Self) {
        if other.modules.is_empty() {
            return;
        }
        self.modules.extend(other.modules.clone());
        self.tables.extend(other.tables.clone());
        self.configs.extend(other.configs.clone());
        self.layouts.extend(other.layouts.clone());
        self.barriers.extend(other.barriers.clone());
        self.weapons.extend(other.weapons.clone());
    }

    pub(crate) fn capture_layout(&mut self, layout: asset_core::BitLayout) {
        self.layouts
            .insert(normalize(&layout.name), std::sync::Arc::new(layout));
    }

    pub fn layouts(&self) -> &BTreeMap<String, std::sync::Arc<asset_core::BitLayout>> {
        &self.layouts
    }

    /// A later zone's barrier type replaces an earlier one of the same name.
    pub(crate) fn capture_barrier(&mut self, barrier: asset_core::BarrierType) {
        self.barriers
            .insert(normalize(&barrier.name), std::sync::Arc::new(barrier));
    }

    pub fn barriers(&self) -> &BTreeMap<String, std::sync::Arc<asset_core::BarrierType>> {
        &self.barriers
    }

    /// A later zone's weapon replaces an earlier one of the same name.
    pub(crate) fn capture_weapon(&mut self, weapon: asset_core::ZoneWeapon) {
        self.weapons
            .insert(normalize(&weapon.name), std::sync::Arc::new(weapon));
    }

    pub fn weapons(&self) -> &BTreeMap<String, std::sync::Arc<asset_core::ZoneWeapon>> {
        &self.weapons
    }

    pub(crate) fn capture_zone_table(&mut self, table: asset_core::ZoneTable) {
        self.tables.insert(
            normalize(&table.name),
            ScriptTable {
                columns: table.columns,
                rows: table.rows,
                cells: table.cells,
            },
        );
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub(crate) fn asset_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for source in self.sources.values().filter_map(|s| s.bytes.as_ref().ok()) {
            quoted_names(source, &mut names);
        }
        if let Some(entities) = &self.entities {
            quoted_names(entities.as_bytes(), &mut names);
        }
        for table in self.tables.values() {
            names.extend(table.cells.iter().cloned());
        }
        names
    }

    pub fn schemas(&self) -> &BTreeMap<String, std::sync::Arc<structured_data_iw4::DefinitionSet>> {
        &self.schemas
    }

    pub(crate) fn capture_schema(
        &mut self,
        name: String,
        schema: structured_data_iw4::DefinitionSet,
    ) {
        self.schemas
            .insert(normalize(&name), std::sync::Arc::new(schema));
    }

    pub fn tables(&self) -> &BTreeMap<String, ScriptTable> {
        &self.tables
    }

    pub fn entities(&self) -> Option<&str> {
        self.entities.as_deref()
    }

    pub fn configs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.configs
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
    }

    pub fn config(&self, name: &str) -> Option<&str> {
        self.configs.get(&normalize(name)).map(String::as_str)
    }

    pub fn shocks(&self) -> impl Iterator<Item = (&str, &str)> {
        self.configs.iter().filter_map(|(path, text)| {
            let name = path.strip_prefix("shock/")?.strip_suffix(".shock")?;
            Some((name, text.as_str()))
        })
    }

    /// A config whose text a zone carries as is.
    pub(crate) fn capture_config(&mut self, name: &str, text: String) {
        self.configs.insert(normalize(name), text);
    }

    pub(crate) fn capture(&mut self, name: &str, data: &[u8], compressed: bool) {
        let name = normalize(name);
        if name.ends_with(".cfg")
            || name.ends_with(".shock")
            || name.ends_with(".atr")
            || name == "radiant/keys.txt"
            || name == "radiant/clientkeys.txt"
        {
            if let Some(text) = asset_world::decode_rawfile_text(data, compressed) {
                self.configs.insert(name, text);
            }
            return;
        }
        // Client scripts (`clientscripts/x.csc`) sit beside the server's
        // under their own folder, read as `clientscripts/x`.
        let Some(module) = name
            .strip_suffix(".gsc")
            .or_else(|| name.strip_suffix(".csc"))
        else {
            return;
        };
        let source = if compressed {
            asset_transport::inflate_zlib(data).map_err(|e| e.to_string())
        } else {
            Ok(asset_world::decode_packed_rawfile(data).unwrap_or_else(|| data.to_vec()))
        }
        .map(|mut bytes| {
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        });
        self.sources.insert(
            module.to_owned(),
            ScriptSource {
                bytes: source,
                origin: ScriptSourceOrigin::Packaged,
            },
        );
    }

    /// A compiled module, kept as it is: `scripts/zm/x.gsc` is read as
    /// `scripts/zm/x`.
    pub(crate) fn capture_compiled(&mut self, name: &str, bytes: Vec<u8>) {
        let name = normalize(name);
        let Some(module) = name.strip_suffix(".gsc") else {
            return;
        };
        self.sources.insert(
            module.to_owned(),
            ScriptSource {
                bytes: Ok(bytes),
                origin: ScriptSourceOrigin::Packaged,
            },
        );
    }

    pub(crate) fn capture_table(&mut self, table: &asset_game::CapturedStringTable) {
        self.tables.insert(
            normalize(&table.name),
            ScriptTable {
                columns: table.columns,
                rows: table.rows,
                cells: table.cells.clone(),
            },
        );
    }

    pub(crate) fn set_table_cells(
        &mut self,
        table: &str,
        key: &str,
        cells: &[(usize, String)],
    ) -> bool {
        let Some(table) = self.tables.get_mut(&normalize(table)) else {
            return false;
        };
        let Some(row) = table
            .cells
            .chunks_exact_mut(table.columns.max(1))
            .take(table.rows)
            .find(|row| row[0].eq_ignore_ascii_case(key))
        else {
            return false;
        };
        for (column, value) in cells {
            if let Some(cell) = row.get_mut(*column) {
                cell.clone_from(value);
            }
        }
        true
    }

    pub(crate) fn insert_source(&mut self, module: &str, source: String) {
        self.sources.insert(
            normalize(module),
            ScriptSource {
                bytes: Ok(source.into_bytes()),
                origin: ScriptSourceOrigin::BuiltIn,
            },
        );
    }

    pub(crate) fn set_entities(&mut self, entities: String) {
        self.entities = Some(entities);
    }

    pub(crate) fn overlay(&mut self, other: Self) {
        self.sources.extend(other.sources);
        self.modules.extend(other.modules);
        self.tables.extend(other.tables);
        self.schemas.extend(other.schemas);
        self.layouts.extend(other.layouts);
        self.barriers.extend(other.barriers);
        self.weapons.extend(other.weapons);
        self.configs.extend(other.configs);
        if other.entities.is_some() {
            self.entities = other.entities;
        }
    }
}

fn quoted_names(bytes: &[u8], names: &mut BTreeSet<String>) {
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1).copied()) {
            (b'/', Some(b'/')) => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            (b'/', Some(b'*')) => {
                at += 2;
                while at + 1 < bytes.len() && &bytes[at..at + 2] != b"*/" {
                    at += 1;
                }
                at = (at + 2).min(bytes.len());
            }
            (b'"', _) => {
                at += 1;
                let start = at;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' && at + 1 < bytes.len() {
                        at += 1;
                    }
                    at += 1;
                }
                if let Ok(name) = std::str::from_utf8(&bytes[start..at]) {
                    names.insert(name.to_owned());
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
}
