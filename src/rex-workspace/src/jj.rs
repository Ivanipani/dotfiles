//! Jujutsu calls: workspace lifecycle and per-project status.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

fn jj(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("jj").arg("-R").arg(dir).args(args).output()
}

fn jj_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = jj(dir, args)?;
    if !out.status.success() {
        bail!("jj {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn workspace_add(source: &str, name: &str, dir: &Path) -> Result<()> {
    // Other workspaces rewrite the source's commit; `workspace add` refuses a stale source.
    let _ = jj(Path::new(source), &["workspace", "update-stale"]);
    jj_ok(Path::new(source), &["workspace", "add", "--name", name, &dir.to_string_lossy()])?;
    jj_ok(dir, &["config", "set", "--workspace", "snapshot.auto-update-stale", "true"])?;
    Ok(())
}

/// Keeps uncommitted work alive under a `saved-<set>` bookmark, then forgets the workspace.
/// A workspace whose source repository is gone is left for plain deletion.
pub fn forget(dir: &Path) -> Result<Option<String>> {
    let probe = jj(dir, &["log", "-r", "@", "--no-graph", "-T", "empty"])?;
    if !probe.status.success() {
        return Ok(None);
    }
    let mut note = None;
    if String::from_utf8_lossy(&probe.stdout).trim() == "false" {
        let set = dir.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let base = format!("saved-{set}");
        let mut name = base.clone();
        let mut n = 1;
        while !jj_ok(dir, &["bookmark", "list", "-T", r#"name ++ "\n""#, &format!("exact:{name}")])?.trim().is_empty() {
            n += 1;
            name = format!("{base}-{n}");
        }
        jj_ok(dir, &["bookmark", "create", &name, "-r", "@"])?;
        let project = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        note = Some(format!("saved {name} in {project}"));
    }
    jj_ok(dir, &["workspace", "forget"])?;
    Ok(note)
}

/// The working copy of one project at a glance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub change: String,
    pub dirty: bool,
    pub conflict: bool,
    pub description: String,
    /// Bookmarks on `@`, else the nearest ancestor's.
    pub bookmarks: String,
    pub changed_files: usize,
}

const STATUS_TEMPLATE: &str =
    r#"change_id.shortest(8) ++ "\t" ++ if(empty, "clean", "dirty") ++ if(conflict, " conflict") ++ "\t" ++ description.first_line() ++ "\t" ++ bookmarks"#;

pub fn parse_status(line: &str) -> Status {
    let mut f = line.trim_end_matches('\n').splitn(4, '\t');
    let change = f.next().unwrap_or_default().into();
    let state = f.next().unwrap_or_default();
    Status {
        change,
        dirty: state.starts_with("dirty"),
        conflict: state.ends_with(" conflict"),
        description: f.next().unwrap_or_default().into(),
        bookmarks: f.next().unwrap_or_default().trim().into(),
        changed_files: 0,
    }
}

pub fn status(dir: &Path) -> Option<Status> {
    let out = jj_ok(dir, &["--ignore-working-copy", "log", "-r", "@", "--no-graph", "-T", STATUS_TEMPLATE]).ok()?;
    let mut status = parse_status(&out);
    if status.bookmarks.is_empty() {
        status.bookmarks = jj_ok(
            dir,
            &["--ignore-working-copy", "log", "-r", "heads(::@ & bookmarks())", "--no-graph", "-T", r#"bookmarks ++ " ""#],
        )
        .unwrap_or_default()
        .trim()
        .to_string();
    }
    if status.dirty {
        status.changed_files =
            jj_ok(dir, &["--ignore-working-copy", "diff", "--summary"]).map(|s| s.lines().count()).unwrap_or(0);
    }
    Some(status)
}

/// What syncing one workspace did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synced {
    /// `trunk()` resolves to the root commit: no remote main/master/trunk to rebase onto.
    NoTrunk,
    UpToDate { conflicts: usize },
    /// Rebased past `behind` new trunk commits.
    Rebased { behind: usize, conflicts: usize },
}

pub fn fetch(dir: &Path) -> Result<()> {
    jj_ok(dir, &["git", "fetch"]).map(drop)
}

fn count(dir: &Path, revset: &str) -> Result<usize> {
    Ok(jj_ok(dir, &["log", "-r", revset, "--no-graph", "-T", r#""x\n""#])?.lines().count())
}

/// Rebase the branch of `@` onto `trunk()` when trunk has moved on.
pub fn rebase_onto_trunk(dir: &Path) -> Result<Synced> {
    if count(dir, "trunk() ~ root()")? == 0 {
        return Ok(Synced::NoTrunk);
    }
    let behind = count(dir, "::trunk() ~ ::@")?;
    if behind > 0 {
        jj_ok(dir, &["rebase", "-b", "@", "-d", "trunk()"])?;
    }
    let conflicts = count(dir, "conflicts() & (trunk()..@)")?;
    Ok(if behind == 0 { Synced::UpToDate { conflicts } } else { Synced::Rebased { behind, conflicts } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_when_dirty_marks_the_project_dirty() {
        assert!(parse_status("abcd1234\tdirty\twip\tmain\n").dirty);
    }

    #[test]
    fn parse_status_when_clean_marks_the_project_clean() {
        assert!(!parse_status("abcd1234\tclean\t\tmain \n").dirty);
    }

    #[test]
    fn parse_status_reads_description_and_trims_bookmarks() {
        // GIVEN / WHEN
        let s = parse_status("abcd1234\tclean\tfix it\tmain \n");
        // THEN
        assert_eq!((s.description.as_str(), s.bookmarks.as_str()), ("fix it", "main"));
    }

    #[test]
    fn parse_status_when_conflicted_marks_the_conflict() {
        assert!(parse_status("abcd1234\tdirty conflict\twip\tmain\n").conflict);
    }

    #[test]
    fn parse_status_when_conflicted_still_reads_dirty() {
        assert!(parse_status("abcd1234\tdirty conflict\twip\tmain\n").dirty);
    }

    #[test]
    fn parse_status_when_fields_are_missing_defaults_them() {
        assert_eq!(parse_status("abcd1234"), Status { change: "abcd1234".into(), ..Status::default() });
    }
}
