//! What a game provides to a match. A rule the game's own data does not give
//! is [`Rule::Unknown`]: the caller reports it as a gap and applies nothing in
//! its place — never another game's rule.

/// A rule this game does not have yet. `id` is listed in
/// `docs/fidelity/<game>.md` (`cargo xtask boundary` checks it).
#[derive(Debug)]
pub struct Unknown {
    pub id: &'static str,
    pub what: &'static str,
    pub needs: &'static str,
}

#[derive(Debug)]
pub enum Rule<T> {
    Known(T),
    Unknown(&'static Unknown),
}

/// `unknown!("t5.area.rule", "what is missing", "what would recover it")`.
#[macro_export]
macro_rules! unknown {
    ($id:literal, $what:literal, $needs:literal $(,)?) => {
        &$crate::Unknown {
            id: $id,
            what: $what,
            needs: $needs,
        }
    };
}

pub struct ScriptRequest<'a> {
    pub map: &'a str,
    pub gametype: &'a str,
    /// The map's entity string.
    pub entities: &'a str,
}

/// What a match compiles and where it starts.
pub struct ScriptProgram {
    pub catalog: gsc::Catalog,
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}

pub trait GameScripts: Sync {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram>;

    /// Engine dvars this game's code sets before scripts run.
    fn engine_dvars(&self) -> &'static [(&'static str, &'static str)];
}
