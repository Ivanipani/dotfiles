//! Jujutsu calls: workspace lifecycle and per-project status.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

fn jj(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("jj").arg("-R").arg(dir).args(args).output()
}

pub(crate) fn jj_ok(dir: &Path, args: &[&str]) -> Result<String> {
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
    set_config(dir, "snapshot.auto-update-stale", "true")?;
    scope_to_agent(dir, name)
}

fn set_config(dir: &Path, key: &str, toml_value: &str) -> Result<()> {
    jj_ok(dir, &["config", "set", "--workspace", key, toml_value]).map(drop)
}

/// The bookmarks the agent of workspace `ws` owns: the only ones it creates, moves and pushes.
pub fn agent_prefix(ws: &str) -> String {
    format!("agent/{ws}/")
}

/// jj's own immutable commits, plus other agents' bookmarks and every other workspace's `@`.
fn immutable_heads(ws: &str) -> String {
    let mine = agent_prefix(ws);
    format!(
        r#"builtin_immutable_heads() | ((bookmarks(glob:"agent/*") | remote_bookmarks(glob:"agent/*") | working_copies()) ~ (bookmarks(glob:"{mine}*") | remote_bookmarks(glob:"{mine}*") | @))"#
    )
}

/// Keep the agent of `ws` off other agents' work, and name its `jj git push -c` bookmarks under its prefix.
pub fn scope_to_agent(dir: &Path, ws: &str) -> Result<()> {
    // TOML literal strings: the values hold double quotes, and names never hold a single quote.
    set_config(dir, r#"revset-aliases."immutable_heads()""#, &format!("'{}'", immutable_heads(ws)))?;
    set_config(dir, "templates.git_push_bookmark", &format!("'\"{}\" ++ {}'", agent_prefix(ws), push_slug()))
}

/// The first three words of the commit's subject, without a conventional `type(scope):`, or its change id.
fn push_slug() -> String {
    let words = r#"description.first_line().lower().replace(regex:"^[a-z]+(\\([^)]*\\))?!?:", "").replace(regex:"[^a-z0-9]+", "-").replace(regex:"^-+", "").replace(regex:"^([a-z0-9]+(-[a-z0-9]+){0,2}).*$", "$1")"#;
    format!("if({words}, {words}, change_id.short())")
}

/// Work on the branch of `@` that no remote holds.
const STRANDED: &str = "((trunk()..@) & mutable() & ~empty()) ~ ::remote_bookmarks()";

/// The tip of a run of stranded commits.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Stranded {
    pub change: String,
    pub description: String,
    pub bookmarks: Vec<String>,
    /// Stranded commits up to and including this one.
    pub commits: usize,
}

const STRANDED_TEMPLATE: &str =
    r#"change_id.shortest(8) ++ "\t" ++ description.first_line() ++ "\t" ++ local_bookmarks.map(|b| b.name()).join(" ") ++ "\n""#;

fn parse_stranded(line: &str) -> Stranded {
    let mut f = line.splitn(3, '\t');
    Stranded {
        change: f.next().unwrap_or_default().into(),
        description: f.next().unwrap_or_default().into(),
        bookmarks: f.next().unwrap_or_default().split_whitespace().map(String::from).collect(),
        commits: 0,
    }
}

pub fn has_remote(dir: &Path) -> Result<bool> {
    Ok(!jj_ok(dir, &["git", "remote", "list"])?.trim().is_empty())
}

/// The heads of this workspace's work that exist only on this machine, newest first.
pub fn stranded(dir: &Path) -> Result<Vec<Stranded>> {
    let out = jj_ok(dir, &["log", "-r", &format!("heads({STRANDED})"), "--no-graph", "-T", STRANDED_TEMPLATE])?;
    out.lines()
        .map(|line| {
            let mut head = parse_stranded(line);
            head.commits = count(dir, &format!("::{} & ({STRANDED})", head.change))?;
            Ok(head)
        })
        .collect()
}

/// A bookmark under `prefix` + `base` that neither this repository nor its remotes use yet.
fn free_bookmark(dir: &Path, prefix: &str, base: &str) -> Result<String> {
    let mut name = format!("{prefix}{base}");
    let mut n = 1;
    while !jj_ok(dir, &["bookmark", "list", "--all-remotes", "-T", r#"name ++ "\n""#, &format!("exact:{name}")])?.trim().is_empty() {
        n += 1;
        name = format!("{prefix}{base}-{n}");
    }
    Ok(name)
}

/// Put every stranded head of the workspace on a bookmark of its agent and push it, so the work
/// outlives the workspace and the machine. A head with no bookmark of the agent gets `saved`.
pub fn rescue(dir: &Path, ws: &str) -> Result<Vec<String>> {
    let prefix = agent_prefix(ws);
    let remote = has_remote(dir)?;
    let project = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut notes = vec![];
    for head in stranded(dir)? {
        let name = match head.bookmarks.iter().find(|b| b.starts_with(&prefix)) {
            Some(name) => name.clone(),
            None => {
                let name = free_bookmark(dir, &prefix, "saved")?;
                jj_ok(dir, &["bookmark", "create", &name, "-r", &head.change])?;
                name
            }
        };
        notes.push(match remote {
            false => format!("saved {name} in {project} (no remote to push to)"),
            true => match jj_ok(dir, &["git", "push", "--allow-empty-description", "-b", &name]) {
                Ok(_) => format!("pushed {name} from {project}"),
                Err(e) => format!("saved {name} in {project}, not pushed: {e:#}"),
            },
        });
    }
    Ok(notes)
}

/// Rescues stranded work, then forgets the workspace.
/// A workspace whose source repository is gone is left for plain deletion.
pub fn forget(dir: &Path, ws: &str) -> Result<Vec<String>> {
    if !jj(dir, &["log", "-r", "@", "--no-graph", "-T", "empty"])?.status.success() {
        return Ok(vec![]);
    }
    let notes = rescue(dir, ws)?;
    jj_ok(dir, &["workspace", "forget"])?;
    Ok(notes)
}

/// Work left in a repository: an `agent/` bookmark on a remote, or an orphan (a head that no
/// bookmark and no workspace points at).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Leftover {
    /// `None` for an orphan.
    pub bookmark: Option<String>,
    pub remotes: Vec<String>,
    pub change: String,
    pub description: String,
    /// Already on trunk: the bookmark has nothing left to land.
    pub landed: bool,
    /// Unix seconds.
    pub committed: i64,
}

const LEFTOVERS: &str = r#"remote_bookmarks(glob:"agent/*") | (heads(mutable() & ~empty()) ~ ::(bookmarks() | remote_bookmarks() | working_copies()))"#;

const LEFTOVER_TEMPLATE: &str = r#"change_id.shortest(8) ++ "\t" ++ description.first_line() ++ "\t" ++ remote_bookmarks.filter(|b| b.name().starts_with("agent/") && b.remote() != "git").map(|b| b.name() ++ "@" ++ b.remote()).join(" ") ++ "\t" ++ if(self.contained_in("::trunk() ~ root()"), "landed") ++ "\t" ++ committer.timestamp().format("%s") ++ "\n""#;

/// One commit's line: an orphan, or one leftover per `agent/` bookmark on it.
fn parse_leftovers(line: &str) -> Vec<Leftover> {
    let f: Vec<_> = line.split('\t').collect();
    let field = |i: usize| f.get(i).copied().unwrap_or_default();
    let row = |bookmark: Option<String>, remotes: Vec<String>| Leftover {
        bookmark,
        remotes,
        change: field(0).into(),
        description: field(1).into(),
        landed: field(3) == "landed",
        committed: field(4).trim().parse().unwrap_or_default(),
    };
    let mut named: Vec<(String, Vec<String>)> = vec![];
    for symbol in field(2).split_whitespace() {
        let Some((name, remote)) = symbol.rsplit_once('@') else { continue };
        match named.iter_mut().find(|(n, _)| n == name) {
            Some((_, remotes)) => remotes.push(remote.into()),
            None => named.push((name.into(), vec![remote.into()])),
        }
    }
    if named.is_empty() {
        return vec![row(None, vec![])];
    }
    named.into_iter().map(|(name, remotes)| row(Some(name), remotes)).collect()
}

pub fn leftovers(dir: &Path) -> Result<Vec<Leftover>> {
    let out = jj_ok(dir, &["log", "-r", LEFTOVERS, "--no-graph", "-T", LEFTOVER_TEMPLATE])?;
    Ok(out.lines().flat_map(parse_leftovers).collect())
}

/// Delete a bookmark here and on every remote that holds it.
pub fn delete_bookmark(dir: &Path, name: &str, remotes: &[String]) -> Result<()> {
    let exact = format!("exact:{name}");
    for remote in remotes {
        // Only a tracked bookmark's deletion reaches the remote.
        jj_ok(dir, &["bookmark", "track", &format!("{name}@{remote}")])?;
    }
    jj_ok(dir, &["bookmark", "delete", &exact])?;
    for remote in remotes {
        jj_ok(dir, &["git", "push", "--remote", remote, "-b", &exact])?;
    }
    Ok(())
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

    #[test]
    fn parse_stranded_splits_the_bookmarks() {
        assert_eq!(parse_stranded("abcd\tfix it\tagent/x/a agent/x/b").bookmarks, ["agent/x/a", "agent/x/b"]);
    }

    #[test]
    fn parse_leftovers_without_an_agent_bookmark_is_one_orphan() {
        assert_eq!(parse_leftovers("abcd\twip\t\t\t100")[0].bookmark, None);
    }

    #[test]
    fn parse_leftovers_gathers_the_remotes_of_one_bookmark() {
        // GIVEN one bookmark on two remotes
        let rows = parse_leftovers("abcd\tfix\tagent/x/a@origin agent/x/a@fork\t\t100");
        // THEN
        assert_eq!(rows[0].remotes, ["origin", "fork"]);
    }

    #[test]
    fn parse_leftovers_reads_landed_and_the_commit_time() {
        let row = &parse_leftovers("abcd\tfix\tagent/x/a@origin\tlanded\t100\n")[0];
        assert_eq!((row.landed, row.committed), (true, 100));
    }

    #[test]
    fn immutable_heads_exempts_the_workspaces_own_bookmarks() {
        assert!(immutable_heads("demo").contains(r#"~ (bookmarks(glob:"agent/demo/*")"#));
    }
}
