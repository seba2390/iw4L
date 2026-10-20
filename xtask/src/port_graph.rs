//! `cargo xtask port-graph [args…]`: `scripts/port_graph.py`, the SQLite index
//! of builtins, natives, ledger ids and entity events. No arguments is `check`.

use std::path::Path;
use std::process::Command;

use crate::shell::{Res, require_tools, run};

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    require_tools(&["python3"])?;
    let mut cmd = Command::new("python3");
    cmd.current_dir(root)
        .arg(root.join("scripts/port_graph.py"));
    if args.is_empty() {
        cmd.arg("check");
    } else {
        cmd.args(args);
    }
    run(&mut cmd)
}
