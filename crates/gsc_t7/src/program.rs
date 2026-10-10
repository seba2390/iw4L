use std::collections::BTreeMap;
use std::sync::Arc;

use gsc::{
    Builtin, Callee, Function, Location, ModuleIdentity, Namespace, Op, Owner, Program, Realm,
    Site as ModuleSite, SourceOrigin, Value,
};
use sha2::{Digest, Sha256};

use crate::translate::{Linker, Site, Translator};
use crate::Module;
pub use crate::names::name_of;

fn display(hash: u32) -> String {
    name_of(hash).map_or_else(|| format!("#{hash:08x}"), str::to_owned)
}

/// One compiled module to load: its name (`scripts/zm/zm_zod`), bytes and origin.
pub struct Source {
    pub name: String,
    pub bytes: Vec<u8>,
    pub origin: SourceOrigin,
}

pub struct Built {
    pub program: Program,
    /// `module::function` of every `autoexec` function, in module order.
    pub autoexec: Vec<String>,
    pub report: Vec<String>,
}

struct ProgramLinker {
    module: usize,
    sites: Vec<(usize, Site)>,
    symbols: Vec<Arc<str>>,
    symbol_ids: BTreeMap<Arc<str>, u32>,
    natives: Vec<Builtin>,
    native_ids: BTreeMap<(Namespace, &'static str), u32>,
}

impl ProgramLinker {
    fn native_id(&mut self, namespace: Namespace, name: &'static str) -> u32 {
        *self.native_ids.entry((namespace, name)).or_insert_with(|| {
            self.natives
                .push(Builtin::new(namespace, name, Owner::Script, false));
            self.natives.len() as u32 - 1
        })
    }
}

impl Linker for ProgramLinker {
    fn site(&mut self, site: Site) -> u32 {
        self.sites.push((self.module, site));
        self.sites.len() as u32 - 1
    }

    fn native(&mut self, name: &'static str) -> Callee {
        Callee::Native(self.native_id(Namespace::Function, name))
    }

    fn symbol(&mut self, hash: u32) -> u32 {
        let name: Arc<str> = display(hash).into();
        if let Some(&id) = self.symbol_ids.get(&name) {
            return id;
        }
        let id = self.symbols.len() as u32;
        self.symbols.push(name.clone());
        self.symbol_ids.insert(name, id);
        id
    }

    fn text(&self, hash: u32) -> String {
        display(hash)
    }
}

/// Reads, translates and links compiled modules into one program. Calls
/// between modules link by the callee's namespace and name hashes; a call no
/// module defines binds the builtin of that name.
pub fn build(sources: &[Source], realm: Realm) -> Built {
    let mut report = Vec::new();
    let mut modules = Vec::new();
    for source in sources {
        match Module::parse(&source.bytes) {
            Ok(module) => modules.push((source, module)),
            Err(error) => report.push(format!("t7 program: {}: {error:?}", source.name)),
        }
    }
    let mut names = BTreeMap::new();
    let mut exports = BTreeMap::new();
    let mut autoexec = Vec::new();
    let mut order = Vec::new();
    for (module_index, (source, module)) in modules.iter().enumerate() {
        let mut sorted = module.exports.clone();
        sorted.sort_by_key(|export| export.code);
        for export in sorted {
            let key = format!("{}::{}", source.name, display(export.name));
            let index = order.len();
            names.insert(key.clone(), index);
            exports.insert((export.namespace, export.name), index);
            if export.flags & EXPORT_AUTOEXEC != 0 {
                autoexec.push(key);
            }
            order.push((module_index, export));
        }
    }
    let mut linker = ProgramLinker {
        module: 0,
        sites: Vec::new(),
        symbols: Vec::new(),
        symbol_ids: BTreeMap::new(),
        natives: Vec::new(),
        native_ids: BTreeMap::new(),
    };
    let mut functions: Vec<Function> = Vec::with_capacity(order.len());
    let (mut translated, mut refused) = (0, Vec::new());
    for (module_index, (source, module)) in modules.iter().enumerate() {
        let translator = Translator::new(module, &source.bytes);
        linker.module = module_index;
        for (export, code) in module.functions(&source.bytes) {
            let name = display(export.name);
            let function = code
                .map_err(|error| format!("{error:?}"))
                .and_then(|code| translator.function(&name, &export, &code, &mut linker));
            functions.push(match function {
                Ok(function) => {
                    translated += 1;
                    function
                }
                Err(error) => {
                    refused.push(format!("{}::{name}: {error}", source.name));
                    stub(&source.name, &name, export.params)
                }
            });
        }
    }
    let mut unbound = BTreeMap::new();
    let resolved: Vec<Callee> = linker
        .sites
        .clone()
        .into_iter()
        .map(|(module_index, site)| {
            let import = &modules[module_index].1.imports[site.import];
            if let Some(&index) = exports.get(&(import.namespace, import.name)) {
                return Callee::Script(index as u32);
            }
            let namespace = if site.method {
                Namespace::Method
            } else {
                Namespace::Function
            };
            let name = name_of(import.name).unwrap_or_else(|| {
                *unbound.entry(import.name).or_insert_with(|| {
                    &*Box::leak(format!("#{:08x}", import.name).into_boxed_str())
                })
            });
            Callee::Native(linker.native_id(namespace, name))
        })
        .collect();
    for function in &mut functions {
        for (_, op) in &mut function.code {
            match op {
                Op::Call(callee @ Callee::Unlinked(_), ..)
                | Op::Spawn(callee @ Callee::Unlinked(_), ..) => {
                    let Callee::Unlinked(id) = *callee else {
                        unreachable!()
                    };
                    *callee = resolved[id as usize];
                }
                Op::FunctionRef(id) => {
                    *op = Op::Constant(match resolved[*id as usize] {
                        Callee::Script(target) => Value::Function(target),
                        Callee::Native(target) => Value::Builtin(target),
                        Callee::Unlinked(_) => unreachable!(),
                    });
                }
                _ => {}
            }
        }
    }
    report.push(format!(
        "t7 program: {} modules, {translated} of {} functions translated, {} natives ({} without a name)",
        modules.len(),
        functions.len(),
        linker.natives.len(),
        unbound.len()
    ));
    report.extend(refused.into_iter().take(8));
    let identities = modules
        .iter()
        .map(|(source, _)| ModuleIdentity {
            site: ModuleSite::Server,
            realm,
            module: source.name.clone(),
            sha256: Sha256::digest(&source.bytes).into(),
            origin: source.origin,
        })
        .collect();
    Built {
        program: Program {
            impure_scripts: sources
                .iter()
                .any(|source| source.origin == SourceOrigin::External),
            functions,
            names,
            modules: identities,
            symbols: linker.symbols,
            symbol_ids: linker.symbol_ids,
            natives: linker.natives,
            rules: realm,
        },
        autoexec,
        report,
    }
}

const EXPORT_AUTOEXEC: u8 = 0x02;

/// A function whose code could not be translated returns undefined; the load
/// report names it.
fn stub(module: &str, name: &str, params: u8) -> Function {
    let location = Location {
        module: module.to_owned(),
        function: name.to_owned(),
        line: 0,
        column: 0,
    };
    Function {
        location: location.clone(),
        parameters: usize::from(params),
        slots: usize::from(params),
        code: vec![
            (location.clone(), Op::Constant(Value::Undefined)),
            (location, Op::Return),
        ],
    }
}
