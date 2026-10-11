//! `cargo xtask progress [--write]` — the progress table at the top of
//! `README.md` is generated from the milestone lists in `docs/PROGRESS.md`.
//! Without `--write` it fails when the README's table is not the one the
//! milestones give; with it, it writes that table.

use std::path::Path;

use crate::shell::Res;

const MILESTONES: &str = "docs/PROGRESS.md";
const README: &str = "README.md";
const START: &str =
    "<!-- progress:start (generated from docs/PROGRESS.md by `cargo xtask progress --write`) -->";
const END: &str = "<!-- progress:end -->";
const BAR: usize = 20;

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let write = match args {
        [] => false,
        [flag] if flag == "--write" => true,
        _ => return Err("usage: progress [--write]".into()),
    };
    let milestones = read(root, MILESTONES)?;
    let readme = read(root, README)?;
    let table = render(&games(&milestones)?);
    let (before, current, after) = split(&readme)?;
    if current == table {
        println!("progress: README.md matches {MILESTONES}");
        return Ok(());
    }
    if !write {
        return Err(format!(
            "README.md's progress table does not match {MILESTONES}; run `cargo xtask progress --write`"
        ));
    }
    std::fs::write(root.join(README), format!("{before}{table}{after}"))
        .map_err(|e| format!("{README}: {e}"))?;
    println!("progress: README.md table written from {MILESTONES}");
    Ok(())
}

fn read(root: &Path, file: &str) -> Res<String> {
    std::fs::read_to_string(root.join(file)).map_err(|e| format!("{file}: {e}"))
}

/// Each `## ` section's title and its ticked and total milestones.
fn games(text: &str) -> Res<Vec<(String, usize, usize)>> {
    let mut games: Vec<(String, usize, usize)> = Vec::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            games.push((title.trim().to_owned(), 0, 0));
        } else if let Some(game) = games.last_mut() {
            if line.starts_with("- [x] ") {
                game.1 += 1;
                game.2 += 1;
            } else if line.starts_with("- [ ] ") {
                game.2 += 1;
            }
        }
    }
    games.retain(|(_, _, total)| *total > 0);
    if games.is_empty() {
        return Err(format!("{MILESTONES} lists no milestones"));
    }
    Ok(games)
}

fn render(games: &[(String, usize, usize)]) -> String {
    let mut out = format!(
        "{START}\n**Progress** toward each game's goal ([milestones](docs/PROGRESS.md)):\n\n\
         | Game | Progress | Milestones |\n|---|---|---|\n"
    );
    for (title, done, total) in games {
        let percent = done * 100 / total;
        let filled = done * BAR / total;
        out.push_str(&format!(
            "| {title} | `{}{}` {percent}% | {done} / {total} |\n",
            "█".repeat(filled),
            "░".repeat(BAR - filled)
        ));
    }
    out.push_str(END);
    out
}

/// The README around its table, and the table itself (markers included).
fn split(readme: &str) -> Res<(&str, &str, &str)> {
    let start = readme
        .find(START)
        .ok_or("README.md has no progress table start marker")?;
    let end = readme[start..]
        .find(END)
        .map(|at| start + at + END.len())
        .ok_or("README.md has no progress table end marker")?;
    Ok((&readme[..start], &readme[start..end], &readme[end..]))
}
