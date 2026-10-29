//! `cargo xtask boundary` — keeps each game's rules inside that game's crates.
//!
//! Every workspace crate declares `[package.metadata.iw4l] layer` (`neutral`,
//! `format`, `game`, `session`, `mixed`) and, for `format`/`game`, the `game` it
//! belongs to. Three checks:
//!
//! * graph — a game crate depends only on neutral crates and its own game's
//!   format/game crates; a format crate on neutral crates and its own game's
//!   format crates; a neutral crate on neutral and format crates;
//! * patterns — outside game/format crates, no code names a game: game enum
//!   variants, `GameModeKind::Zombies`, a game crate's path, a game literal,
//!   `is_<game>` helpers, a gametype token literal (`"zclassic"`, …) or a
//!   `.token() == "…"` test. The session's registration point is the
//!   exception, and for gametypes the mode enum's token table and a game's
//!   natives;
//! * ledger — every `unknown!("<id>", ..)` in a game crate, and every id that
//!   starts an error string (`"t6.weapons.table_full: …"`) in any crate, is
//!   listed in `docs/fidelity/<game>.md` of the game the id names;
//! * ownership — every game has its own game crate (`game_<game>`, answering
//!   every `game_api` trait) and its own format crate, and the registration
//!   answers a game only with that game's crates. A rule lent from another
//!   game must be a named item there whose type that game's ledger names.
//!
//! Ownership and ledger failures are errors; graph and pattern findings are
//! recorded in `xtask/boundary/allow.txt`. That file is a
//! ratchet: a violation it does not list is new, an entry that no longer
//! occurs as often is stale, and `--update` only ever lowers it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::shell::Res;

const ALLOW: &str = "xtask/boundary/allow.txt";
const REGISTRATION: &str = "crates/session/src/games.rs";
/// The game identity itself has to name the games.
const IDENTITY: &str = "crates/asset_core/src/family.rs";
/// This check has to spell the shapes it hunts for.
const SELF: &str = "xtask/src/boundary.rs";
/// The ledger guard names the game whose crates every game shares.
const LEDGER_GUARD: &str = "xtask/src/ledger_guard.rs";
const GAMES: [&str; 5] = ["iw4", "t5", "iw5", "t6", "t7"];
const GAME_VARIANTS: [&str; 5] = ["Iw4", "T5", "Iw5", "T6", "T7"];
const GAME_ENUMS: [&str; 4] = ["ZoneGame", "AssetNamespace", "FamilyId", "Realm"];
/// The traits through which the session asks a game for its rules.
const GAME_TRAITS: [&str; 4] = ["GameScripts", "GameModes", "GameMenus", "GameVision"];
/// Gametype tokens: outside a game's code the mode is asked, not named.
const MODE_TOKENS: [&str; 4] = ["zclassic", "zstandard", "zgrief", "zcleansed"];
/// The mode enum's own token table.
const MODE_TABLE: &str = "crates/gamemode_iw4/src/kind.rs";
const NATIVES: &str = "crates/sim/src/script/host/natives/";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Neutral,
    Format,
    Game,
    Session,
    Mixed,
}

impl Layer {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "neutral" => Self::Neutral,
            "format" => Self::Format,
            "game" => Self::Game,
            "session" => Self::Session,
            "mixed" => Self::Mixed,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Format => "format",
            Self::Game => "game",
            Self::Session => "session",
            Self::Mixed => "mixed",
        }
    }
}

struct Package {
    name: String,
    ident: String,
    layer: Option<Layer>,
    game: Option<String>,
    dir: PathBuf,
    deps: Vec<String>,
}

/// Violation key → occurrences. Keys are `edge <from> <to>`, `mixed <crate>`
/// and `pattern <file> <shape>`.
type Findings = BTreeMap<String, usize>;

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let enforce = args.iter().any(|a| a == "--enforce");
    let update = args.iter().any(|a| a == "--update");
    if let Some(other) = args
        .iter()
        .find(|a| !matches!(a.as_str(), "--enforce" | "--update"))
    {
        return Err(format!(
            "unknown argument {other}; usage: boundary [--enforce] [--update]"
        ));
    }
    let packages = workspace(root)?;
    let mut errors = Vec::new();
    let mut findings = Findings::new();

    let by_name: BTreeMap<&str, &Package> = packages.iter().map(|p| (p.name.as_str(), p)).collect();
    for package in &packages {
        let Some(layer) = package.layer else {
            errors.push(format!(
                "{}: no [package.metadata.iw4l] layer",
                package.name
            ));
            continue;
        };
        if matches!(layer, Layer::Format | Layer::Game)
            && !package.game.as_deref().is_some_and(|g| GAMES.contains(&g))
        {
            errors.push(format!(
                "{}: layer {} needs game = one of {GAMES:?}",
                package.name,
                layer.name()
            ));
        }
        if layer == Layer::Mixed {
            findings.insert(format!("mixed {}", package.name), 1);
        }
        for dep in &package.deps {
            let Some(target) = by_name.get(dep.as_str()) else {
                continue;
            };
            if target
                .layer
                .is_some_and(|to| !edge_allowed(layer, package, to, target))
            {
                findings.insert(format!("edge {} {}", package.name, dep), 1);
            }
        }
    }

    let game_idents: BTreeSet<&str> = packages
        .iter()
        .filter(|p| match p.layer {
            Some(Layer::Game | Layer::Format) => true,
            Some(Layer::Neutral) => false,
            _ => {
                p.ident.starts_with("game_")
                    || GAMES.iter().any(|g| p.ident.ends_with(&format!("_{g}")))
            }
        })
        .map(|p| p.ident.as_str())
        .collect();
    let mut unknown_ids = 0;
    for package in &packages {
        for file in rust_files(&package.dir) {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if rel != SELF {
                let mut ids = error_ids_in(&text);
                if package.layer == Some(Layer::Game) {
                    ids.extend(unknown_ids_in(&text));
                }
                for id in ids {
                    unknown_ids += 1;
                    let game = GAMES
                        .iter()
                        .copied()
                        .find(|g| id.strip_prefix(g).is_some_and(|r| r.starts_with('.')))
                        .or(package.game.as_deref())
                        .unwrap_or("?");
                    if !ledger_lists(root, game, &id) {
                        errors.push(format!(
                            "{rel}: ledger id `{id}` is not in docs/fidelity/{game}.md"
                        ));
                    }
                }
            }
            match package.layer {
                Some(Layer::Game | Layer::Format) | None => {}
                Some(_) if [REGISTRATION, IDENTITY, SELF, LEDGER_GUARD].contains(&rel.as_str()) => {
                }
                Some(_) => {
                    let shapes = game_shapes(&text, &game_idents);
                    for (shape, count) in shapes.into_iter().chain(mode_shapes(&rel, &text)) {
                        findings.insert(format!("pattern {rel} {shape}"), count);
                    }
                }
            }
        }
    }

    let allow_path = root.join(ALLOW);
    errors.extend(ownership(root, &packages));
    let allowed = match std::fs::read_to_string(&allow_path) {
        Ok(text) => parse_allow(&text)?,
        Err(_) if update => {
            write_allow(&allow_path, &findings)?;
            println!("boundary: wrote {ALLOW} with {} entries", findings.len());
            return Ok(());
        }
        Err(e) => return Err(format!("{ALLOW}: {e}")),
    };
    let mut new = Vec::new();
    let mut stale = Vec::new();
    for (key, &count) in &findings {
        let allow = allowed.get(key).copied().unwrap_or(0);
        if count > allow {
            new.push(format!("{key} {count} (allowed {allow})"));
        }
    }
    for (key, &allow) in &allowed {
        let count = findings.get(key).copied().unwrap_or(0);
        if count < allow {
            stale.push(format!("{key} {count} (allowed {allow})"));
        }
    }

    summary(&packages, &findings, unknown_ids);
    for error in &errors {
        println!("error: {error}");
    }
    for line in &new {
        println!("new:   {line}");
    }
    for line in &stale {
        println!("stale: {line}");
    }
    if update {
        let lowered: Findings = allowed
            .iter()
            .filter_map(|(key, &allow)| {
                let count = findings.get(key).copied().unwrap_or(0).min(allow);
                (count > 0).then(|| (key.clone(), count))
            })
            .collect();
        write_allow(&allow_path, &lowered)?;
        println!("boundary: {ALLOW} lowered to {} entries", lowered.len());
    }
    let failed = !errors.is_empty() || !new.is_empty() || (!stale.is_empty() && !update);
    if !failed {
        println!("boundary: nothing beyond {ALLOW}");
        return Ok(());
    }
    if enforce {
        return Err(format!(
            "{} errors, {} new, {} stale (stale entries: `cargo xtask boundary --update`)",
            errors.len(),
            new.len(),
            stale.len()
        ));
    }
    println!("boundary: report only; `--enforce` fails on the lines above");
    Ok(())
}

fn edge_allowed(from: Layer, package: &Package, to: Layer, target: &Package) -> bool {
    let same_game = package.game.is_some() && package.game == target.game;
    match from {
        Layer::Session | Layer::Mixed => true,
        Layer::Neutral => matches!(to, Layer::Neutral | Layer::Format),
        Layer::Format => to == Layer::Neutral || (to == Layer::Format && same_game),
        Layer::Game => {
            to == Layer::Neutral || (matches!(to, Layer::Format | Layer::Game) && same_game)
        }
    }
}

fn summary(packages: &[Package], findings: &Findings, unknown_ids: usize) {
    let mut layers: BTreeMap<&str, usize> = BTreeMap::new();
    for package in packages {
        *layers
            .entry(package.layer.map_or("none", Layer::name))
            .or_default() += 1;
    }
    let layers: Vec<String> = layers.iter().map(|(l, n)| format!("{l} {n}")).collect();
    println!(
        "boundary: {} crates — {}",
        packages.len(),
        layers.join(", ")
    );
    let edges = findings.keys().filter(|k| k.starts_with("edge ")).count();
    println!("  (a) graph: {edges} dependencies outside the layer rules");
    let mut per_crate: BTreeMap<String, usize> = BTreeMap::new();
    for (key, count) in findings.iter().filter(|(k, _)| k.starts_with("pattern ")) {
        let file = key.split(' ').nth(1).unwrap_or("");
        let krate = file
            .strip_prefix("crates/")
            .unwrap_or(file)
            .split('/')
            .next()
            .unwrap_or("");
        *per_crate.entry(krate.to_owned()).or_default() += count;
    }
    let total: usize = per_crate.values().sum();
    let mut ranked: Vec<_> = per_crate.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let ranked: Vec<String> = ranked.iter().map(|(c, n)| format!("{c} {n}")).collect();
    println!(
        "  (b) patterns: {total} game checks outside game crates: {}",
        ranked.join(", ")
    );
    println!(
        "  (c) ledger: {unknown_ids} ids (unknown!() in game crates, `<id>: …` errors anywhere)"
    );
}

fn workspace(root: &Path) -> Res<Vec<Package>> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let packages = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    packages
        .iter()
        .map(|p| {
            let name = p["name"]
                .as_str()
                .ok_or("cargo metadata: a package without a name")?;
            let manifest = p["manifest_path"]
                .as_str()
                .ok_or_else(|| format!("cargo metadata: {name} has no manifest_path"))?;
            let iw4l = &p["metadata"]["iw4l"];
            Ok(Package {
                ident: name.replace('-', "_"),
                layer: iw4l["layer"].as_str().and_then(Layer::parse),
                game: iw4l["game"].as_str().map(str::to_owned),
                dir: Path::new(manifest)
                    .parent()
                    .ok_or_else(|| format!("{manifest}: no parent directory"))?
                    .to_path_buf(),
                deps: p["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d["name"].as_str().map(str::to_owned))
                    .collect(),
                name: name.to_owned(),
            })
        })
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_dir() {
                if file_path.file_name().is_some_and(|n| n != "target") {
                    stack.push(file_path);
                }
            } else if file_path.extension().is_some_and(|e| e == "rs") {
                files.push(file_path);
            }
        }
    }
    files.sort();
    files
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Count of `needle` in `line` as a whole token: no identifier character right
/// before it, and none right after unless `open_end`.
fn token_hits(line: &str, needle: &str, open_end: bool) -> usize {
    line.match_indices(needle)
        .filter(|(at, _)| {
            let before = line[..*at].chars().next_back();
            let after = line[at + needle.len()..].chars().next();
            !before.is_some_and(is_ident) && (open_end || !after.is_some_and(is_ident))
        })
        .count()
}

/// The code part of a line: whole-line comments are dropped, a trailing `//`
/// comment is cut when it is not inside a string.
fn code_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return "";
    }
    let mut quotes = 0;
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' if i == 0 || bytes[i - 1] != b'\\' => quotes += 1,
            b'/' if quotes % 2 == 0 && bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn game_shapes(text: &str, game_idents: &BTreeSet<&str>) -> BTreeMap<String, usize> {
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |shape: String, n: usize| {
        if n > 0 {
            *shapes.entry(shape).or_default() += n;
        }
    };
    for line in text.lines().map(code_of) {
        add(
            "GameModeKind::Zombies".into(),
            token_hits(line, "GameModeKind::Zombies", false),
        );
        for game_enum in GAME_ENUMS {
            let n = GAME_VARIANTS
                .iter()
                .map(|v| token_hits(line, &format!("{game_enum}::{v}"), false))
                .sum();
            add(format!("{game_enum}::<game>"), n);
        }
        for ident in game_idents {
            add(
                format!("{ident}::"),
                token_hits(line, &format!("{ident}::"), true),
            );
        }
        let literals = GAMES
            .iter()
            .map(|g| line.matches(&format!("\"{g}\"")).count())
            .sum();
        add("\"<game>\"".into(), literals);
        let helpers = GAMES
            .iter()
            .map(|g| g.to_string())
            .chain(["zombies".to_owned()])
            .map(|g| token_hits(line, &format!("is_{g}"), false))
            .sum();
        add("is_<game>".into(), helpers);
    }
    shapes
}

/// Gametype tokens named, and modes tested by their token against a literal.
fn mode_shapes(rel: &str, text: &str) -> BTreeMap<String, usize> {
    let game_natives = rel.strip_prefix(NATIVES).is_some_and(|rest| {
        GAMES.iter().any(|g| {
            rest.starts_with(&format!("{g}/"))
                || rest.starts_with(&format!("{g}_"))
                || rest == format!("{g}.rs")
        })
    });
    let mut shapes = BTreeMap::new();
    if rel == MODE_TABLE || game_natives {
        return shapes;
    }
    for line in text.lines().map(code_of) {
        let tokens: usize = MODE_TOKENS
            .iter()
            .map(|token| line.matches(&format!("\"{token}\"")).count())
            .sum();
        let tests = line
            .match_indices(".token()")
            .filter(|(at, m)| {
                let rest = line[at + m.len()..].trim_start();
                rest.strip_prefix("==")
                    .or_else(|| rest.strip_prefix("!="))
                    .is_some_and(|rest| rest.trim_start().starts_with('"'))
            })
            .count();
        for (shape, n) in [("\"<gametype>\"", tokens), (".token()==\"…\"", tests)] {
            if n > 0 {
                *shapes.entry(shape.to_owned()).or_default() += n;
            }
        }
    }
    shapes
}

fn unknown_ids_in(text: &str) -> Vec<String> {
    let code: Vec<&str> = text.lines().map(code_of).collect();
    let code = code.join("\n");
    code.match_indices("unknown!(")
        .filter_map(|(at, m)| {
            let rest = code[at + m.len()..].trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_owned())
        })
        .collect()
}

/// Ids that open a string literal as `"<game>.<area>.<rule>: …"`.
fn error_ids_in(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines().map(code_of) {
        for (at, _) in line.match_indices('"') {
            let rest = &line[at + 1..];
            let len = rest
                .find(|c: char| {
                    !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
                })
                .unwrap_or(rest.len());
            let (id, after) = rest.split_at(len);
            let named = GAMES.iter().any(|g| {
                id.strip_prefix(g)
                    .and_then(|r| r.strip_prefix('.'))
                    .is_some_and(|r| !r.is_empty() && !r.ends_with('.'))
            });
            if named && after.starts_with(':') {
                ids.push(id.to_owned());
            }
        }
    }
    ids
}

/// A game's ledger is `docs/fidelity/<game>.md`, with its multiplayer half
/// in `<game>-mp.md` where the game has one.
fn ledger_lists(root: &Path, game: &str, id: &str) -> bool {
    [format!("{game}.md"), format!("{game}-mp.md")]
        .iter()
        .any(|file| {
            std::fs::read_to_string(root.join("docs/fidelity").join(file))
                .is_ok_and(|ledger| ledger.contains(&format!("`{id}`")))
        })
}

fn ownership(root: &Path, packages: &[Package]) -> Vec<String> {
    let mut errors = Vec::new();
    for game in GAMES {
        let own = |layer: Layer| {
            packages
                .iter()
                .filter(move |p| p.layer == Some(layer) && p.game.as_deref() == Some(game))
        };
        if own(Layer::Format).next().is_none() {
            errors.push(format!(
                "{game}: no format crate reads this game's own data"
            ));
        }
        let ident = format!("game_{game}");
        let Some(rules) = own(Layer::Game).find(|p| p.ident == ident) else {
            errors.push(format!(
                "{game}: no `{ident}` crate holds this game's rules"
            ));
            continue;
        };
        let text: String = rust_files(&rules.dir)
            .iter()
            .filter_map(|file| std::fs::read_to_string(file).ok())
            .collect();
        for name in GAME_TRAITS {
            if !text.contains(&format!("impl game_api::{name} for ")) {
                errors.push(format!("{ident}: does not answer `game_api::{name}`"));
            }
        }
    }

    let Ok(registration) = std::fs::read_to_string(root.join(REGISTRATION)) else {
        errors.push(format!("{REGISTRATION}: missing"));
        return errors;
    };
    for (n, line) in registration.lines().enumerate() {
        let line = code_of(line);
        let Some((arm, answer)) = line.split_once("=>") else {
            continue;
        };
        let answered: Vec<&str> = GAME_VARIANTS
            .iter()
            .zip(GAMES)
            .filter(|(variant, _)| token_hits(arm, &format!("FamilyId::{variant}"), false) > 0)
            .map(|(_, game)| game)
            .collect();
        for game in answered {
            for other in packages.iter().filter(|p| {
                matches!(p.layer, Some(Layer::Game | Layer::Format))
                    && p.game.as_deref().is_some_and(|g| g != game)
            }) {
                if token_hits(answer, &other.ident, false) > 0 {
                    errors.push(format!(
                        "{REGISTRATION}:{}: {game} is answered by `{}`, another game's crate",
                        n + 1,
                        other.ident
                    ));
                }
            }
            for item in answer.split(|c: char| !is_ident(c)).filter(|word| {
                word.len() > 1
                    && word
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                    && !answer.contains(&format!("::{word}"))
            }) {
                let lent = registration
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(&format!("static {item}: ")))
                    .and_then(|rest| rest.split(|c: char| !is_ident(c)).next());
                let ledgered = lent.is_some_and(|ty| ledger_lists(root, game, ty));
                if !ledgered {
                    errors.push(format!(
                        "{REGISTRATION}:{}: {game} is answered by `{item}`, whose type docs/fidelity/{game}.md does not name",
                        n + 1
                    ));
                }
            }
        }
    }
    errors
}

fn parse_allow(text: &str) -> Res<Findings> {
    let mut allowed = Findings::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, count) = match line.rsplit_once(' ') {
            Some((key, count)) if line.starts_with("pattern ") => (key, count.parse().ok()),
            _ => (line, Some(1)),
        };
        let count = count.ok_or_else(|| format!("{ALLOW}:{}: bad count", n + 1))?;
        allowed.insert(key.to_owned(), count);
    }
    Ok(allowed)
}

fn write_allow(path: &Path, findings: &Findings) -> Res<()> {
    let mut text = String::from(
        "# Game-boundary debt that exists today (`cargo xtask boundary`).\n\
         # It may only shrink: lower it with `cargo xtask boundary --update`.\n",
    );
    for (key, count) in findings {
        if key.starts_with("pattern ") {
            text.push_str(&format!("{key} {count}\n"));
        } else {
            text.push_str(&format!("{key}\n"));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
