//! `cargo xtask lints` — clippy and rustc warnings may only go down.
//!
//! Runs `cargo clippy --workspace --all-targets --message-format=json`, which
//! reports rustc's own warnings beside clippy's, and counts every warning in a
//! workspace package once (a library and its test build report the same one
//! twice), keyed by package, file and lint.
//!
//! `xtask/lints/allow.txt` lists the warnings that exist today. It is a
//! ratchet: a warning it does not list, or more of one than it allows, is new;
//! an entry that no longer occurs as often is stale; `--update` only ever
//! lowers it. `--enforce` fails on either.
//!
//! The counts depend on the platform (cfg'd code) and on the toolchain (a new
//! release adds lints): CI counts on Linux with the toolchain
//! `rust-toolchain.toml` pins.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::shell::Res;

const ALLOW: &str = "xtask/lints/allow.txt";

/// `(package, file, lint)` → warnings.
type Counts = BTreeMap<(String, String, String), usize>;

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let enforce = args.iter().any(|a| a == "--enforce");
    let update = args.iter().any(|a| a == "--update");
    if let Some(other) = args
        .iter()
        .find(|a| !matches!(a.as_str(), "--enforce" | "--update"))
    {
        return Err(format!(
            "unknown argument {other}; usage: lints [--enforce] [--update]"
        ));
    }
    let counts = clippy(root)?;
    summary(&counts);

    let allow_path = root.join(ALLOW);
    let allowed = match std::fs::read_to_string(&allow_path) {
        Ok(text) => parse_allow(&text)?,
        Err(_) if update => {
            write_allow(&allow_path, &counts)?;
            println!("lints: wrote {ALLOW} with {} entries", counts.len());
            return Ok(());
        }
        Err(e) => return Err(format!("{ALLOW}: {e}")),
    };
    let mut new = Vec::new();
    let mut stale = Vec::new();
    for (key, &count) in &counts {
        let allow = allowed.get(key).copied().unwrap_or(0);
        if count > allow {
            new.push(format!("{} {count} (allowed {allow})", line(key)));
        }
    }
    for (key, &allow) in &allowed {
        let count = counts.get(key).copied().unwrap_or(0);
        if count < allow {
            stale.push(format!("{} {count} (allowed {allow})", line(key)));
        }
    }
    for entry in &new {
        println!("new:   {entry}");
    }
    for entry in &stale {
        println!("stale: {entry}");
    }
    if update {
        let lowered: Counts = allowed
            .iter()
            .filter_map(|(key, &allow)| {
                let count = counts.get(key).copied().unwrap_or(0).min(allow);
                (count > 0).then(|| (key.clone(), count))
            })
            .collect();
        write_allow(&allow_path, &lowered)?;
        println!(
            "lints: {ALLOW} lowered to {} warnings in {} entries",
            lowered.values().sum::<usize>(),
            lowered.len()
        );
    }
    let failed = !new.is_empty() || (!stale.is_empty() && !update);
    if !failed {
        println!("lints: nothing beyond {ALLOW}");
        return Ok(());
    }
    if enforce {
        return Err(format!(
            "{} new, {} stale (fix new warnings — the list never grows; \
             stale entries: `cargo xtask lints --update`)",
            new.len(),
            stale.len()
        ));
    }
    println!("lints: report only; `--enforce` fails on the lines above");
    Ok(())
}

fn line((package, file, lint): &(String, String, String)) -> String {
    format!("{package} {file} {lint}")
}

/// The workspace's warnings, each counted once.
fn clippy(root: &Path) -> Res<Counts> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let members = workspace_members(root, &cargo)?;
    let mut child = Command::new(&cargo)
        .args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--message-format=json",
        ])
        .current_dir(root)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cargo clippy: {e}"))?;
    let stdout = child.stdout.take().ok_or("cargo clippy: no stdout")?;
    let root_prefix = format!("{}/", root.display());
    let mut seen = BTreeSet::new();
    let mut counts = Counts::new();
    let mut errors = 0;
    for text in BufReader::new(stdout).lines() {
        let text = text.map_err(|e| format!("cargo clippy: {e}"))?;
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if record["reason"] != "compiler-message" {
            continue;
        }
        let Some(package) = record["package_id"].as_str().and_then(|id| members.get(id)) else {
            continue;
        };
        let message = &record["message"];
        match message["level"].as_str() {
            Some("warning") => {}
            Some("error") => {
                errors += 1;
                if let Some(rendered) = message["rendered"].as_str() {
                    eprint!("{rendered}");
                }
                continue;
            }
            _ => continue,
        }
        let lint = message["code"]["code"].as_str().unwrap_or("-").to_owned();
        let text = message["message"].as_str().unwrap_or("").to_owned();
        let Some(span) = primary_span(message) else {
            // rustc's closing "N warnings emitted" carries no span and no lint.
            if lint != "-" {
                counts
                    .entry((package.clone(), "-".to_owned(), lint))
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
            }
            continue;
        };
        let (file, line, column) = span;
        let file = file
            .strip_prefix(&root_prefix)
            .unwrap_or(&file)
            .replace('\\', "/");
        if !seen.insert((file.clone(), line, column, lint.clone(), text)) {
            continue;
        }
        counts
            .entry((package.clone(), file, lint))
            .and_modify(|n| *n += 1)
            .or_insert(1);
    }
    let status = child.wait().map_err(|e| format!("cargo clippy: {e}"))?;
    if !status.success() || errors > 0 {
        return Err(format!(
            "cargo clippy failed ({errors} errors): fix the build before counting warnings"
        ));
    }
    Ok(counts)
}

/// Where a warning is: its primary span, followed out of macro expansions
/// until it lands in a workspace file.
fn primary_span(message: &serde_json::Value) -> Option<(String, u64, u64)> {
    let mut span = message["spans"]
        .as_array()?
        .iter()
        .find(|s| s["is_primary"] == true)?;
    loop {
        let file = span["file_name"].as_str()?;
        let outside = Path::new(file).is_absolute() || file.starts_with('<');
        match &span["expansion"]["span"] {
            next if outside && next.is_object() => span = next,
            _ => {
                return Some((
                    file.to_owned(),
                    span["line_start"].as_u64().unwrap_or(0),
                    span["column_start"].as_u64().unwrap_or(0),
                ));
            }
        }
    }
}

/// Package id → name, for the workspace's own packages only.
fn workspace_members(root: &Path, cargo: &str) -> Res<BTreeMap<String, String>> {
    let output = Command::new(cargo)
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
    meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
        .iter()
        .map(|p| {
            let id = p["id"]
                .as_str()
                .ok_or("cargo metadata: a package without an id")?;
            let name = p["name"]
                .as_str()
                .ok_or("cargo metadata: a package without a name")?;
            Ok((id.to_owned(), name.to_owned()))
        })
        .collect()
}

fn summary(counts: &Counts) {
    let mut per_package: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_lint: BTreeMap<&str, usize> = BTreeMap::new();
    for ((package, _, lint), &count) in counts {
        *per_package.entry(package).or_insert(0) += count;
        *per_lint.entry(lint).or_insert(0) += count;
    }
    let total: usize = counts.values().sum();
    println!("lints: {total} warnings");
    for (title, map) in [("package", per_package), ("lint", per_lint)] {
        if map.is_empty() {
            continue;
        }
        let mut ranked: Vec<_> = map.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let ranked: Vec<String> = ranked.iter().map(|(k, n)| format!("{k} {n}")).collect();
        println!("  by {title}: {}", ranked.join(", "));
    }
}

fn parse_allow(text: &str) -> Res<Counts> {
    let mut allowed = Counts::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [package, file, lint, count] = fields[..] else {
            return Err(format!(
                "{ALLOW}:{}: want `<package> <file> <lint> <count>`",
                n + 1
            ));
        };
        let count = count
            .parse()
            .map_err(|_| format!("{ALLOW}:{}: bad count", n + 1))?;
        allowed.insert((package.into(), file.into(), lint.into()), count);
    }
    Ok(allowed)
}

fn write_allow(path: &Path, counts: &Counts) -> Res<()> {
    let mut text = String::from(
        "# Clippy and rustc warnings that exist today (`cargo xtask lints`):\n\
         # <package> <file> <lint> <count>. It may only shrink: lower it with\n\
         # `cargo xtask lints --update`. Counted on Linux with the pinned toolchain.\n",
    );
    for (key, count) in counts {
        text.push_str(&format!("{} {count}\n", line(key)));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
