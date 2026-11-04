//! Zombies rounds (approved by the owner by name): a cold headless load of
//! Kino der Toten (Black Ops, `zombies`) or Nuketown (Black Ops II,
//! `zclassic`), the test player's rounds mode for a bounded game time, then
//! `quit`. The run passes when the round the scripts report reaches the
//! scenario's floor, no script fault outside its allow-list was logged, and
//! the process exits cleanly. A game that is not installed, or Black Ops II
//! without its captured patch zone, is a skipped prerequisite.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::report;
use crate::runner::{self, End, Launch};

pub struct Rounds {
    pub name: &'static str,
    /// The zone `serve` loads, with its game.
    zone: &'static str,
    gametype: &'static str,
    /// The round the scripts must reach within `game_secs`.
    min_round: i64,
    /// Game seconds of play after the world is up, run at `timescale`.
    game_secs: u32,
    timescale: u32,
    /// The captured downloaded patch the game needs (`ffotd_*<suffix>*.ff`).
    patch: Option<&'static str>,
    /// Script faults the run may log: ledger gaps a fault message names.
    allowed_faults: &'static [&'static str],
}

pub const SCENARIOS: [Rounds; 2] = [
    Rounds {
        name: "zombies_rounds_t5",
        zone: "t5:zombie_theater",
        gametype: "zombies",
        // Round 2 began at 85 and 90 game s in two runs on main, and round 3
        // was not reached within 300 s.
        min_round: 2,
        game_secs: 300,
        timescale: 8,
        patch: None,
        // Black Ops' own equality refuses a pair of arrays: `is_in_array` on
        // an array of arrays faults and goes on, as the game's VM does.
        allowed_faults: &["\"function\":\"is_in_array\""],
    },
    Rounds {
        name: "zombies_rounds_t6",
        zone: "t6:zm_nuked",
        gametype: "zclassic",
        // Round 2 began at 140 and 195 game s in two runs on main; round 3
        // waits on #885 (zombies stall near the player).
        min_round: 2,
        game_secs: 300,
        timescale: 8,
        patch: Some("_zm"),
        allowed_faults: &[],
    },
];

pub fn find(name: &str) -> Option<&'static Rounds> {
    SCENARIOS.iter().find(|scenario| scenario.name == name)
}

/// The run's result: passed, failed (with its reasons), or skipped for a
/// missing prerequisite.
pub enum Verdict {
    Passed,
    Failed,
    Skipped,
}

fn prerequisite(scenario: &Rounds) -> Result<(), String> {
    let games =
        asset_transport::games_root_from_env().map_err(|e| format!("no games root ({e})"))?;
    asset_transport::find_zone_file(&games, scenario.zone)
        .map_err(|e| format!("{} is not installed: {e}", scenario.zone))?;
    if let Some(suffix) = scenario.patch
        && asset_transport::t6_content::captured_patch_files(suffix).is_empty()
    {
        return Err(format!(
            "no captured patch zone (ffotd_*{suffix}*.ff) in {}",
            asset_transport::t6_content::capture_folders()
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(" or ")
        ));
    }
    Ok(())
}

/// The command script: the world, then the game time in waits of at most
/// a minute (a single `wait` is capped), then `quit`.
fn commands(scenario: &Rounds) -> String {
    let mut out = vec![
        "mark world.begin".to_owned(),
        "wait world".to_owned(),
        "mark world.end".to_owned(),
        "mark rounds.begin".to_owned(),
    ];
    let mut left = scenario.game_secs;
    while left > 0 {
        let step = left.min(60);
        out.push(format!("wait {step}s"));
        left -= step;
    }
    out.push("mark rounds.end".to_owned());
    out.push("quit".to_owned());
    out.join("; ")
}

/// The captured patch zones the game reads from its working directory's
/// artifacts, linked from where the owner keeps them.
fn link_patches(scenario: &Rounds, run_dir: &Path) -> Vec<String> {
    let Some(suffix) = scenario.patch else {
        return Vec::new();
    };
    let dir = run_dir.join("iw4l-artifacts/captured");
    let _ = std::fs::create_dir_all(&dir);
    let mut linked = Vec::new();
    for zone in asset_transport::t6_content::captured_patch_files(suffix) {
        let Some(name) = zone.file_name() else {
            continue;
        };
        let to = dir.join(name);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&zone, &to).is_ok();
        #[cfg(not(unix))]
        let made = std::fs::copy(&zone, &to).is_ok();
        if made {
            linked.push(zone.display().to_string());
        }
    }
    linked
}

/// The test player's five-second lines: the highest round, and the kills
/// and game time of the last line.
fn rounds_seen(log: &str) -> Value {
    let field = |line: &str, key: &str| -> Option<String> {
        line.split_whitespace()
            .find_map(|token| token.strip_prefix(key))
            .map(|value| value.trim_end_matches('s').to_owned())
    };
    let mut best: Option<i64> = None;
    let mut last = json!(null);
    let mut samples = 0;
    for line in log.lines().filter(|line| line.contains("test player: t=")) {
        samples += 1;
        let round = field(line, "round=").and_then(|v| v.parse::<i64>().ok());
        if let Some(round) = round {
            best = Some(best.map_or(round, |best| best.max(round)));
        }
        last = json!({
            "t": field(line, "t=").and_then(|v| v.parse::<f64>().ok()),
            "round": round,
            "kills": field(line, "kills=").and_then(|v| v.parse::<i64>().ok()),
            "score": field(line, "score=").and_then(|v| v.parse::<i64>().ok()),
        });
    }
    json!({ "max_round": best, "last": last, "samples": samples })
}

pub fn run(root: &Path, bin: &Path, scenario: &Rounds) -> Result<Verdict, String> {
    asset_transport::load_dotenv();
    if !bin.is_file() {
        return Err(format!(
            "{} is not built (make approved builds it)",
            bin.display()
        ));
    }
    let run_id = format!("{}-{}", crate::utc_stamp(), scenario.name);
    let run_dir = root.join("iw4l-artifacts/approved-tests").join(&run_id);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("create {}: {e}", run_dir.display()))?;
    let script = commands(scenario);
    let mut manifest = json!({
        "scenario": scenario.name,
        "approved": true,
        "run_id": run_id,
        "result": "running",
        "seed": Value::Null,
        "seed_rule": "none: the scripts' and the test player's randomness is not seeded, so rounds and kills vary between runs",
        "build": report::build_facts(root, bin),
        "platform": report::platform_facts(),
        "render": { "target": "none (headless serve)" },
        "zone": scenario.zone,
        "gametype": scenario.gametype,
        "requires": {
            "round_at_least": scenario.min_round,
            "game_secs": scenario.game_secs,
            "timescale": scenario.timescale,
            "allowed_faults": scenario.allowed_faults,
        },
        "commands": script,
    });
    if let Err(reason) = prerequisite(scenario) {
        manifest["result"] = "skipped".into();
        manifest["skipped"] = reason.clone().into();
        report::write(&run_dir, &manifest)?;
        println!("skipped: {reason}; {}", run_dir.join("run.json").display());
        return Ok(Verdict::Skipped);
    }
    manifest["patches"] = link_patches(scenario, &run_dir).into();
    let log = run_dir.join("iw4l-artifacts/logs/latest.log");
    let env = vec![
        ("IW4L_SOUND", "off".to_owned()),
        ("IW4L_GAMETYPE", scenario.gametype.to_owned()),
        ("IW4L_TEST_PLAYER", "rounds".to_owned()),
        ("IW4L_TIMESCALE", scenario.timescale.to_string()),
        ("IW4L_LOG", log.display().to_string()),
        (
            "IW4L_SETTINGS_PATH",
            run_dir.join("settings.cfg").display().to_string(),
        ),
        (
            "IW4L_PROFILE_PATH",
            run_dir.join("profile.cfg").display().to_string(),
        ),
    ];
    manifest["env"] = env
        .iter()
        .map(|(key, value)| (key.to_string(), Value::from(value.clone())))
        .collect::<serde_json::Map<_, _>>()
        .into();
    report::write(&run_dir, &manifest)?;
    let outcome = runner::run(Launch {
        bin,
        cwd: &run_dir,
        args: vec![
            "serve".to_owned(),
            scenario.zone.to_owned(),
            "--cmds".to_owned(),
            script,
        ],
        env,
        phase_timeout: Duration::from_secs(600),
        quit_timeout: Duration::from_secs(60),
    });

    let mut failures: Vec<String> = Vec::new();
    match &outcome.end {
        End::Exited {
            code: Some(0),
            at_ms,
        } => manifest["exit"] = json!({ "code": 0, "at_ms": at_ms }),
        End::Exited { code, at_ms } => {
            manifest["exit"] = json!({ "code": code, "at_ms": at_ms });
            failures.push(format!("the game exited with {code:?} at {at_ms} ms"));
        }
        End::Timeout { waiting_for, at_ms } => {
            manifest["exit"] = json!({ "timeout": waiting_for, "at_ms": at_ms });
            failures.push(format!("timed out at {at_ms} ms waiting for {waiting_for}"));
        }
        End::SpawnFailed(reason) => {
            manifest["exit"] = json!({ "spawn_failed": reason });
            failures.push(format!("the game did not start: {reason}"));
        }
    }
    manifest["marks"] = outcome
        .marks
        .iter()
        .map(|mark| json!({ "label": mark.get("label"), "at_ms": mark.at_ms }))
        .collect::<Vec<_>>()
        .into();
    if !outcome
        .marks
        .iter()
        .any(|mark| mark.get("label") == Some("rounds.end"))
    {
        failures.push("the rounds phase did not finish".into());
    }

    let faults: Vec<Value> = outcome
        .script
        .iter()
        .filter(|event| matches!(event.name.as_str(), "runtime_error" | "refused"))
        .map(|event| {
            json!({
                "kind": event.name,
                "at": event.get("at"),
                "function": event.get("function"),
                "fault": event.get("fault").or_else(|| event.get("unknown")),
            })
        })
        .collect();
    let unallowed: Vec<&Value> = faults
        .iter()
        .filter(|fault| {
            let text = fault.to_string();
            !scenario.allowed_faults.iter().any(|gap| text.contains(gap))
        })
        .collect();
    if let Some(first) = unallowed.first() {
        failures.push(format!(
            "{} script fault(s) outside the allow-list, first: {first}",
            unallowed.len()
        ));
    }
    manifest["script_faults"] = faults.clone().into();

    let log_text = match std::fs::read_to_string(&log) {
        Ok(text) => text,
        Err(error) => {
            failures.push(format!(
                "the log {} could not be read: {error}",
                log.display()
            ));
            String::new()
        }
    };
    let rounds = rounds_seen(&log_text);
    match rounds["max_round"].as_i64() {
        Some(round) if round >= scenario.min_round => {}
        Some(round) => failures.push(format!(
            "reached round {round}, short of round {}",
            scenario.min_round
        )),
        None => failures.push("the test player logged no round".into()),
    }
    manifest["rounds"] = rounds;
    manifest["log"] = log.display().to_string().into();
    manifest["result"] = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    }
    .into();
    manifest["failures"] = failures.clone().into();
    report::write(&run_dir, &manifest)?;
    let summary = run_dir.join("run.json");
    if failures.is_empty() {
        println!(
            "passed: round {} ({}); {}",
            manifest["rounds"]["max_round"],
            scenario.zone,
            summary.display()
        );
        Ok(Verdict::Passed)
    } else {
        for failure in &failures {
            println!("failed: {failure}");
        }
        println!("{}", summary.display());
        Ok(Verdict::Failed)
    }
}
