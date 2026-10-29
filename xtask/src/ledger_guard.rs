//! `cargo xtask ledger-guard [--base <ref>]` — a pull request that changes a
//! game's own code also changes that game's ledger (`docs/fidelity/<game>.md`
//! or its multiplayer half `<game>-mp.md`),
//! or says why not in a line `Ledger: none — <reason>` of its body or of a
//! commit message. Reads the pull request from the GitHub event payload;
//! outside a pull request it reports that and passes.

use std::path::Path;
use std::process::Command;

use crate::shell::Res;

/// Modern Warfare 2 is the engine every game runs on: its crates are shared,
/// so its ledger is kept by `boundary`'s id check, not by this guard.
const SHARED: &str = "iw4";

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let base = match args {
        [] => "origin/main",
        [flag, base] if flag == "--base" => base.as_str(),
        _ => return Err("usage: ledger-guard [--base <ref>]".into()),
    };
    let Some(body) = pull_request_body()? else {
        println!("ledger-guard: not a pull request (no GitHub event payload); skipped");
        return Ok(());
    };
    let merge_base = git(root, &["merge-base", base, "HEAD"])?;
    let merge_base = merge_base.trim();
    let changed = git(root, &["diff", "--name-only", merge_base, "HEAD"])?;
    let changed: Vec<&str> = changed.lines().collect();
    let messages = git(
        root,
        &["log", "--format=%B", &format!("{merge_base}..HEAD")],
    )?;
    if let Some(reason) = exemption(&body).or_else(|| exemption(&messages)) {
        println!("ledger-guard: Ledger: none — {reason}");
        return Ok(());
    }
    let mut missing = 0;
    for game in ledgered_games(root) {
        let ledger = format!("docs/fidelity/{game}.md");
        let owned: Vec<&str> = changed
            .iter()
            .copied()
            .filter(|file| owned_by(file, &game))
            .collect();
        let mp_ledger = format!("docs/fidelity/{game}-mp.md");
        if owned.is_empty()
            || changed.contains(&ledger.as_str())
            || changed.contains(&mp_ledger.as_str())
        {
            continue;
        }
        missing += 1;
        println!("ledger-guard: this pull request changes {game}'s code but not {ledger}:");
        for file in owned {
            println!("  {file}");
        }
    }
    if missing == 0 {
        println!("ledger-guard: every game whose code changed has its ledger changed too");
        return Ok(());
    }
    println!(
        "Log each rule IW4L decides under that game in its ledger (Open section, \
         severity, id), or add a line `Ledger: none — <reason>` to the pull \
         request body or a commit message."
    );
    Err(format!("{missing} game(s) changed without their ledger"))
}

fn pull_request_body() -> Res<Option<String>> {
    let Some(path) = std::env::var_os("GITHUB_EVENT_PATH") else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: {e}", Path::new(&path).display()))?;
    let event: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("GitHub event payload: {e}"))?;
    let pull_request = &event["pull_request"];
    if pull_request.is_null() {
        return Ok(None);
    }
    Ok(Some(pull_request["body"].as_str().unwrap_or("").to_owned()))
}

fn git(root: &Path, args: &[&str]) -> Res<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("git {}: {e}", args.join(" ")))
}

/// The reason of the first `Ledger: none — <reason>` line.
fn exemption(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Ledger: none")?.trim_start();
        let reason = rest
            .strip_prefix('—')
            .or_else(|| rest.strip_prefix('–'))
            .or_else(|| rest.strip_prefix('-'))?
            .trim();
        (!reason.is_empty()).then(|| reason.to_owned())
    })
}

fn ledgered_games(root: &Path) -> Vec<String> {
    let mut games: Vec<String> = std::fs::read_dir(root.join("docs/fidelity"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            Some(name.strip_suffix(".md")?.to_owned())
        })
        // `<game>-mp.md` is the multiplayer half of `<game>.md`.
        .filter(|game| game != SHARED && !game.ends_with("-mp"))
        .collect();
    games.sort();
    games
}

/// A file of `game`'s own code: its crates (`crates/*_<game>/`), its natives
/// and IW4L's rules for it in `sim`, its HUD and menus in `ui`, its asset lane.
fn owned_by(file: &str, game: &str) -> bool {
    let crate_dir = file
        .strip_prefix("crates/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(dir, _)| dir.ends_with(&format!("_{game}")));
    let host = "crates/sim/src/script/host/";
    let named = |dir: &str| {
        file.strip_prefix(dir).is_some_and(|rest| {
            rest.starts_with(&format!("{game}/"))
                || rest.starts_with(&format!("{game}_"))
                || rest == format!("{game}.rs")
        })
    };
    crate_dir
        || named(&format!("{host}natives/"))
        || file
            .strip_prefix(host)
            .is_some_and(|rest| rest.starts_with(&format!("{game}_")))
        || file
            .strip_prefix("crates/ui/src/")
            .is_some_and(|rest| rest.starts_with(&format!("{game}_")) && rest.ends_with(".rs"))
        || named("crates/assets/src/lane/")
}
