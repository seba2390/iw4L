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
    if current.replace("\r\n", "\n") == table {
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

/// A game's section: its title, ticked and total milestones, and each map's
/// builtins not implemented yet and named, from its `| map | N of M |` rows.
struct Game {
    title: String,
    done: usize,
    total: usize,
    /// Ticked and total steps: the indented detail items under milestones.
    steps_done: usize,
    steps: usize,
    builtins: Vec<(String, usize, usize)>,
}

/// Each `## ` section's milestones and builtin coverage.
fn games(text: &str) -> Res<Vec<Game>> {
    let mut games: Vec<Game> = Vec::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            games.push(Game {
                title: title.trim().to_owned(),
                done: 0,
                total: 0,
                steps_done: 0,
                steps: 0,
                builtins: Vec::new(),
            });
        } else if let Some(game) = games.last_mut() {
            if line.starts_with("- [x] ") {
                game.done += 1;
                game.total += 1;
            } else if line.starts_with("- [ ] ") {
                game.total += 1;
            } else if line.starts_with("  - [x] ") {
                game.steps_done += 1;
                game.steps += 1;
            } else if line.starts_with("  - [ ] ") {
                game.steps += 1;
            } else if let Some(row) = coverage_row(line) {
                game.builtins.push(row);
            }
        }
    }
    games.retain(|game| game.total > 0);
    if games.is_empty() {
        return Err(format!("{MILESTONES} lists no milestones"));
    }
    Ok(games)
}

/// `| Kino der Toten (`t5:zombie_theater`) | 192 of 610 |` → the map's
/// name, missing and named builtins.
fn coverage_row(line: &str) -> Option<(String, usize, usize)> {
    let cells: Vec<&str> = line
        .strip_prefix('|')?
        .strip_suffix('|')?
        .split('|')
        .map(str::trim)
        .collect();
    let [map, count] = cells.as_slice() else {
        return None;
    };
    let (missing, named) = count.split_once(" of ")?;
    let name = map.split(" (").next().unwrap_or(map).to_owned();
    Some((name, missing.parse().ok()?, named.parse().ok()?))
}

fn render(games: &[Game]) -> String {
    let mut out = format!(
        "{START}\n**Progress** toward each game's goal ([milestones](docs/PROGRESS.md)), the \
         steps shown under its milestones, and the builtins its scripts call that are \
         implemented, per map:\n\n\
         | Game | Progress | Milestones | Steps | Builtins |\n|---|---|---|---|---|\n"
    );
    for game in games {
        let (done, total) = (game.done, game.total);
        let percent = done * 100 / total;
        let filled = done * BAR / total;
        let builtins = if game.builtins.is_empty() {
            "—".to_owned()
        } else {
            game.builtins
                .iter()
                .map(|(map, missing, named)| {
                    let implemented = named - missing;
                    let percent = if *named == 0 {
                        100
                    } else {
                        implemented * 100 / named
                    };
                    format!("{map}: {implemented} / {named} ({percent}%)")
                })
                .collect::<Vec<_>>()
                .join("<br>")
        };
        let steps = if game.steps == 0 {
            "—".to_owned()
        } else {
            format!("{} / {}", game.steps_done, game.steps)
        };
        out.push_str(&format!(
            "| {} | `{}{}` {percent}% | {done} / {total} | {steps} | {builtins} |\n",
            game.title,
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
