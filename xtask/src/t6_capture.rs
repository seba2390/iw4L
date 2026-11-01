use std::path::Path;
use std::process::Command;

use crate::shell::{Res, require_tools, run};

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let python = if cfg!(windows) { "python" } else { "python3" };
    require_tools(&[python])?;
    run(Command::new(python)
        .current_dir(root)
        .arg(root.join("scripts/t6_capture.py"))
        .args(args))
}
