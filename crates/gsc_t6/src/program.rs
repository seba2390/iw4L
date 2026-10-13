//! Black Ops 2's compiled modules linked into one program, by the rules its
//! loader links a module's imports with.

use std::collections::BTreeMap;
use std::sync::Arc;

use gsc::{
    Builtin, Callee, Function, Location, ModuleIdentity, Namespace, Op, Owner, Program, Realm,
    Site as ModuleSite, SourceOrigin, Value,
};
use sha2::{Digest, Sha256};

use crate::translate::{Linker, Site, Translator};
use crate::{CallKind, Import, Module};

/// One compiled module: its name without extension (`maps/mp/zm_nuked`),
/// bytes and origin.
pub struct Source {
    pub name: String,
    pub bytes: Vec<u8>,
    pub origin: SourceOrigin,
}

/// A builtin the engine offers a script: how many parameters a call to it
/// may pass, and whether only developer blocks may call it.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinSpec {
    pub name: &'static str,
    pub min_params: i32,
    pub max_params: i32,
    pub developer: bool,
}

/// The engine's builtins, each list in the order the loader searches it.
pub struct Builtins<'a> {
    pub functions: &'a [BuiltinSpec],
    pub methods: &'a [BuiltinSpec],
}

pub struct Built {
    pub program: Program,
    pub report: Vec<String>,
    /// Imports the loader cannot link, which make it refuse the scripts.
    pub unresolved: Vec<String>,
}

/// Export flag of a function only its own module may call.
const PRIVATE: u8 = 0x10;

/// The modules `roots` need: each root, then every module a loaded module
/// includes or calls into, as the loader loads them.
pub fn load(read: &dyn Fn(&str) -> Option<Source>, roots: &[String]) -> (Vec<Source>, Vec<String>) {
    let mut loaded = Vec::new();
    let mut missing = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue: Vec<String> = roots.iter().rev().cloned().collect();
    while let Some(name) = queue.pop() {
        let name = name.to_ascii_lowercase();
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(source) = read(&name) else {
            missing.push(format!("t6 program: could not find script {name}"));
            continue;
        };
        if let Ok(module) = Module::parse(&source.bytes) {
            let wanted = module.includes.iter().chain(
                module
                    .imports
                    .iter()
                    .map(|import| &import.namespace)
                    .filter(|namespace| !namespace.is_empty()),
            );
            queue.extend(wanted.rev().cloned());
        }
        loaded.push(source);
    }
    (loaded, missing)
}

struct ProgramLinker {
    module: usize,
    sites: Vec<(usize, Site)>,
    symbols: Vec<Arc<str>>,
    symbol_ids: BTreeMap<Arc<str>, u32>,
    natives: Vec<Builtin>,
    native_ids: BTreeMap<(Namespace, String), u32>,
}

impl ProgramLinker {
    fn native_id(&mut self, namespace: Namespace, name: &str, developer: bool) -> u32 {
        let key = (namespace, name.to_owned());
        if let Some(&id) = self.native_ids.get(&key) {
            return id;
        }
        let name: &'static str = Box::leak(name.to_owned().into_boxed_str());
        self.natives
            .push(Builtin::new(namespace, name, Owner::Script, developer));
        let id = self.natives.len() as u32 - 1;
        self.native_ids.insert(key, id);
        id
    }
}

impl Linker for ProgramLinker {
    fn site(&mut self, site: Site) -> u32 {
        self.sites.push((self.module, site));
        self.sites.len() as u32 - 1
    }

    fn native(&mut self, name: &'static str) -> Callee {
        Callee::Native(self.native_id(Namespace::Function, name, false))
    }

    fn named(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.symbol_ids.get(name) {
            return id;
        }
        let name: Arc<str> = name.into();
        let id = self.symbols.len() as u32;
        self.symbols.push(name.clone());
        self.symbol_ids.insert(name, id);
        id
    }

    /// The engine finds a dvar by its name's hash, and takes an integer as
    /// such a hash.
    fn hash(&mut self, hash: u32) -> Result<Value, String> {
        Ok(Value::Int(hash as i32))
    }
}

enum Link {
    /// A module and the index of its export.
    Script(usize, usize),
    Builtin(Namespace, &'static BuiltinSpec),
}

/// Reads, translates and links compiled modules into one program.
pub fn build(sources: &[Source], builtins: &Builtins<'static>, realm: Realm) -> Built {
    let mut report = Vec::new();
    let mut modules = Vec::new();
    for source in sources {
        match Module::parse(&source.bytes) {
            Ok(module) => modules.push((source, module)),
            Err(error) => report.push(format!("t6 program: {}: {error:?}", source.name)),
        }
    }
    let by_name: BTreeMap<&str, usize> = modules
        .iter()
        .enumerate()
        .map(|(index, (source, _))| (source.name.as_str(), index))
        .collect();
    let mut names = BTreeMap::new();
    let mut first = Vec::with_capacity(modules.len());
    let mut count = 0;
    for (source, module) in &modules {
        first.push(count);
        for export in &module.exports {
            names.insert(format!("{}::{}", source.name, export.name), count);
            count += 1;
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
    let mut functions: Vec<Function> = Vec::with_capacity(count);
    let mut slots: Vec<Option<Function>> = Vec::new();
    let (mut translated, mut refused) = (0, Vec::new());
    for (module_index, (source, module)) in modules.iter().enumerate() {
        let translator = Translator::new(module, &source.bytes);
        linker.module = module_index;
        slots.clear();
        slots.resize_with(module.exports.len(), || None);
        for (export, code) in module.functions(&source.bytes) {
            let entry = &module.exports[export];
            let function = code.map_err(|error| format!("{error:?}")).and_then(|code| {
                translator.function(&entry.name, entry.params, &code, &mut linker)
            });
            slots[export] = Some(match function {
                Ok(function) => {
                    translated += 1;
                    function
                }
                Err(error) => {
                    refused.push(format!("{}::{}: {error}", source.name, entry.name));
                    stub(&source.name, &entry.name, entry.params)
                }
            });
        }
        functions.extend(slots.drain(..).map(Option::unwrap));
    }
    let mut unresolved = Vec::new();
    let mut warnings = Vec::new();
    let resolved: Vec<Callee> = linker
        .sites
        .clone()
        .into_iter()
        .map(|(module_index, site)| {
            let (source, module) = &modules[module_index];
            let import = &module.imports[site.import];
            match link(&modules, &by_name, module_index, import, builtins) {
                Ok(Link::Script(target_module, export)) => {
                    Callee::Script((first[target_module] + export) as u32)
                }
                Ok(Link::Builtin(namespace, spec)) => {
                    // The loader prints this and links the call all the same.
                    if i32::from(import.params) > spec.max_params {
                        warnings.push(format!(
                            "{}: too many parameters: {} with {}",
                            source.name, import.name, import.params
                        ));
                    }
                    Callee::Native(linker.native_id(namespace, spec.name, spec.developer))
                }
                Err(error) => {
                    unresolved.push(format!("{}: {error}: {}", source.name, import.name));
                    let namespace = if site.method {
                        Namespace::Method
                    } else {
                        Namespace::Function
                    };
                    Callee::Native(linker.native_id(namespace, &import.name, false))
                }
            }
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
    unresolved.sort();
    unresolved.dedup();
    warnings.sort();
    warnings.dedup();
    report.push(format!(
        "t6 program: {} modules, {translated} of {} functions translated, {} builtins, {} unresolved imports",
        modules.len(),
        functions.len(),
        linker.natives.len(),
        unresolved.len()
    ));
    report.extend(refused.into_iter().take(8));
    report.extend(warnings);
    report.extend(unresolved.iter().take(16).cloned());
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
            field_name: None,
            // Black Ops 2's relational operators order two strings
            // (strcmp), as its VM's less and greater evaluators do.
            ordered_strings: true,
        },
        report,
        unresolved,
    }
}

/// An import without a namespace names a builtin first, when its kind may
/// call one; otherwise a function of its module, then of each module it
/// includes, in order. A namespace names the module.
fn link(
    modules: &[(&Source, Module)],
    by_name: &BTreeMap<&str, usize>,
    from: usize,
    import: &Import,
    builtins: &Builtins<'static>,
) -> Result<Link, &'static str> {
    if import.namespace.is_empty() {
        let builtin = match import.kind {
            CallKind::Reference | CallKind::Call => builtins
                .functions
                .iter()
                .find(|spec| spec.name == import.name)
                .map(|spec| (Namespace::Function, spec)),
            CallKind::MethodCall => builtins
                .methods
                .iter()
                .find(|spec| spec.name == import.name)
                .map(|spec| (Namespace::Method, spec)),
            CallKind::ThreadCall | CallKind::MethodThreadCall => None,
        };
        if let Some((namespace, spec)) = builtin {
            let params = i32::from(import.params);
            if spec.developer && !import.developer {
                return Err("developer builtin outside a developer block");
            }
            if params < spec.min_params {
                return Err("too few parameters");
            }
            return Ok(Link::Builtin(namespace, spec));
        }
        let modules_searched = core::iter::once(Some(from)).chain(
            modules[from]
                .1
                .includes
                .iter()
                .map(|include| by_name.get(include.to_ascii_lowercase().as_str()).copied()),
        );
        for module in modules_searched.flatten() {
            if let Some(export) = find_export(&modules[module].1, import, module == from) {
                return Ok(Link::Script(module, export));
            }
        }
        return Err("unresolved external");
    }
    let module = *by_name
        .get(import.namespace.to_ascii_lowercase().as_str())
        .ok_or("could not find script")?;
    find_export(&modules[module].1, import, module == from)
        .map(|export| Link::Script(module, export))
        .ok_or("unresolved external")
}

/// A call may pass no more parameters than the function declares; a
/// reference passes none.
fn find_export(module: &Module, import: &Import, own: bool) -> Option<usize> {
    module.exports.iter().position(|export| {
        export.name == import.name
            && (import.kind == CallKind::Reference || export.params >= import.params)
            && (own || export.flags & PRIVATE == 0)
    })
}

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
