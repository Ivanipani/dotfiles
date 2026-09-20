mod manifest;
mod reconcile;

use reconcile::{apply, observe, plan, State};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: repo-sync [--dry-run] <repos.yaml>";

fn main() -> ExitCode {
    let mut dry_run = false;
    let mut file = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dry-run" | "-n" => dry_run = true,
            _ if file.is_none() && !arg.starts_with('-') => file = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let targets = match manifest::load(&file) {
        Ok(targets) => targets,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut failed = 0;
    for target in &targets {
        let result = observe(&target.path).map_err(|e| e.to_string()).and_then(|state| {
            let steps = plan(&state, &target.url)?;
            let list = steps.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ");
            println!("{}: {list}", target.name);
            if dry_run {
                return Ok(());
            }
            apply(target, &steps).map_err(|e| {
                // Don't leave a half-made clone behind for the next run to trip over.
                if state == State::Missing {
                    let _ = std::fs::remove_dir_all(&target.path);
                }
                e.to_string()
            })
        });
        if let Err(e) = result {
            eprintln!("{}: FAILED: {e}", target.name);
            failed += 1;
        }
    }

    if failed > 0 {
        eprintln!("{failed} of {} repos failed", targets.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
