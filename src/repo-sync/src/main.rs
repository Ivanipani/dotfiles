mod manifest;
mod reconcile;

use anstream::{eprintln, println};
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::Parser;
use reconcile::{apply, observe, plan, State};
use std::path::PathBuf;
use std::process::ExitCode;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Green.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

/// Reconcile a YAML list of repos to colocated jujutsu checkouts.
///
/// Every repo in FILE is brought to the intended state: cloned if missing,
/// converted to a colocated jj repo if it is plain git, `origin` pointed at
/// the listed URL, and fetched. Repos removed from FILE are never deleted.
#[derive(Parser)]
#[command(
    name = "repo-sync",
    version,
    styles = STYLES,
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
  repo-sync --dry-run repos.yaml    Show the planned steps, change nothing"
)]
struct Cli {
    /// YAML manifest listing the repos to reconcile
    #[arg(value_name = "FILE", env = "REPO_SYNC_FILE")]
    file: PathBuf,

    /// Print the planned steps for each repo without running them
    #[arg(short = 'n', long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (name_style, step_style, err_style) =
        (STYLES.get_literal(), STYLES.get_placeholder(), STYLES.get_error());

    let targets = match manifest::load(&cli.file) {
        Ok(targets) => targets,
        Err(e) => {
            eprintln!("{err_style}error:{err_style:#} {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut failed = 0;
    for target in &targets {
        let result = observe(&target.path).map_err(|e| e.to_string()).and_then(|state| {
            let steps = plan(&state, &target.url)?;
            let list = steps.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ");
            println!("{name_style}{}{name_style:#}: {step_style}{list}{step_style:#}", target.name);
            if cli.dry_run {
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
            eprintln!("{name_style}{}{name_style:#}: {err_style}FAILED:{err_style:#} {e}", target.name);
            failed += 1;
        }
    }

    if failed > 0 {
        eprintln!("{err_style}{failed} of {} repos failed{err_style:#}", targets.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
