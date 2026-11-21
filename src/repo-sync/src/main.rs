mod manifest;
mod reconcile;

use claw::output::{OutputArgs, Tabular};
use claw::table::{self, Cell};
use claw::console;
use serde::Serialize;
use clap::Parser;
use reconcile::{apply, observe, plan, State, Step};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

/// Reconcile a YAML list of repos to colocated jujutsu checkouts.
///
/// Every repo in FILE is brought to the intended state: cloned if missing,
/// converted to a colocated jj repo if it is plain git, `origin` pointed at
/// the listed URL, and fetched. Repos removed from FILE are never deleted.
#[derive(Parser)]
#[command(
    name = "repo-sync",
    version,
    styles = claw::cli::STYLES,
    after_long_help = "\
Reconciliation:
  missing or empty dir   init, remote, fetch, checkout
  plain git repo         init (colocate in place), remote, fetch
  jj, not colocated      colocate, remote (if drifted), fetch
  jj, wrong origin       remote, fetch
  jj, up to date         fetch
  non-empty, not a repo  error; left untouched

Manifest:
  root: ~/projects            # default
  repos:
    - url: https://github.com/owner/repo.git
      name: repo              # optional; defaults to the URL's basename

Examples:
  repo-sync repos.yaml              Sync every repo in the manifest
  repo-sync --dry-run repos.yaml    Show the planned steps, change nothing
  repo-sync -n repos.yaml | jq      The plan as JSON (automatic when piped)"
)]
struct Cli {
    /// YAML manifest listing the repos to reconcile
    #[arg(value_name = "FILE", env = "REPO_SYNC_FILE")]
    file: PathBuf,

    /// Print the planned steps for each repo without running them
    #[arg(short = 'n', long)]
    dry_run: bool,

    #[command(flatten)]
    output: OutputArgs,
}

/// What happened to one repo: the steps planned for it, and whether they ran.
#[derive(Serialize)]
struct Outcome {
    repo: String,
    steps: Vec<String>,
    applied: bool,
    error: Option<String>,
}

impl Tabular for Outcome {
    const COLUMNS: &'static [&'static str] = &["REPO", "STEPS", "RESULT"];

    fn row(&self) -> Vec<Cell> {
        let steps = table::optional(Some(self.steps.join(", ")).filter(|s| !s.is_empty()));
        let result = match (&self.error, self.applied) {
            (Some(e), _) => table::error(format!("failed: {e}")),
            (None, true) => table::ok("ok"),
            (None, false) => table::dim("planned"),
        };
        vec![Cell::new(&self.repo), steps, result]
    }
}

fn sync(target: &manifest::Target, dry_run: bool) -> Outcome {
    let mut steps = vec![];
    let result = observe(&target.path).map_err(|e| e.to_string()).and_then(|state| {
        steps = plan(&state, &target.url)?;
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
    Outcome {
        repo: target.name.clone(),
        steps: steps.iter().map(Step::to_string).collect(),
        applied: !dry_run && result.is_ok(),
        error: result.err(),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let targets = match manifest::load(&cli.file) {
        Ok(targets) => targets,
        Err(e) => {
            console::error(e);
            return ExitCode::FAILURE;
        }
    };

    let outcomes: Vec<_> = targets
        .iter()
        .map(|target| {
            if cli.dry_run {
                return sync(target, true);
            }
            console::start(&target.name);
            let started = Instant::now();
            let outcome = sync(target, false);
            match &outcome.error {
                None => console::done(&target.name, Some(started.elapsed())),
                Some(e) => console::failed(&target.name, e, Some(started.elapsed())),
            }
            outcome
        })
        .collect();
    cli.output.format().list(&outcomes, "no repos in the manifest");

    let failed = outcomes.iter().filter(|o| o.error.is_some()).count();
    if failed > 0 {
        console::error(format!("{failed} of {} repos failed", targets.len()));
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(applied: bool, error: Option<&str>) -> Outcome {
        Outcome { repo: "dotfiles".into(), steps: vec!["fetch".into()], applied, error: error.map(str::to_owned) }
    }

    #[test]
    fn row_for_a_dry_run_is_planned() {
        assert_eq!(outcome(false, None).row()[2], table::dim("planned"));
    }

    #[test]
    fn row_for_an_applied_sync_is_ok() {
        assert_eq!(outcome(true, None).row()[2], table::ok("ok"));
    }

    #[test]
    fn row_for_a_failed_sync_includes_the_error() {
        assert_eq!(outcome(false, Some("no route")).row()[2], table::error("failed: no route"));
    }

    #[test]
    fn row_lists_the_steps() {
        assert_eq!(outcome(false, None).row()[1], Cell::new("fetch"));
    }
}
