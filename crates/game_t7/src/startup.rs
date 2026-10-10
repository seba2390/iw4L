/// The compiled modules the roots need: each module's includes, depth first,
/// so a module comes after the modules it includes.
pub(crate) fn load(sources: &dyn gsc::SourceResolver, roots: &[String]) -> Vec<gsc_t7::Source> {
    let mut loaded = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for root in roots {
        visit(sources, root, &mut seen, &mut loaded);
    }
    loaded
}

fn visit(
    sources: &dyn gsc::SourceResolver,
    name: &str,
    seen: &mut std::collections::BTreeSet<String>,
    loaded: &mut Vec<gsc_t7::Source>,
) {
    let name = name.to_ascii_lowercase();
    if !seen.insert(name.clone()) {
        return;
    }
    let Ok(bytes) = sources.read_bytes(&name) else {
        return;
    };
    if let Ok(module) = gsc_t7::Module::parse(&bytes) {
        for include in &module.includes {
            visit(sources, include, seen, loaded);
        }
    }
    loaded.push(gsc_t7::Source {
        origin: sources.origin(&name),
        name,
        bytes,
    });
}
