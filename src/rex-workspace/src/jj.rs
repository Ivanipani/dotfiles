//! Jujutsu calls: workspace lifecycle and per-project status.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The revsets every query here speaks in (`tips()`, `unpushed(x)`, `orphans()`, …), shared with
/// the user's jj config so `jj flight` and jjui read work the way this tool does.
const FLIGHT: &str = include_str!("../../../configs/vcs/jj/flight.toml");

/// `FLIGHT` as a file for `--config-file`, named by its content so builds never share a stale one.
fn flight_config() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        FLIGHT.hash(&mut h);
        let path = std::env::temp_dir().join(format!("rex-workspace-flight-{:x}.toml", h.finish()));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(FLIGHT) {
            let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
            if std::fs::write(&tmp, FLIGHT).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
        path
    })
}

fn jj(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("jj").arg("-R").arg(dir).arg("--config-file").arg(flight_config()).args(args).output()
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
    jj_ok(Path::new(source), &["workspace", "add", "--name", name, &dir.to_string_lossy()]).map(drop)
}

/// The bookmarks the agent of workspace `ws` owns: the only ones it creates, moves and pushes.
pub fn agent_prefix(ws: &str) -> String {
    format!("agent/{ws}/")
}

/// Work on the branch of `@` that no remote holds.
const STRANDED: &str = "unpushed(@)";

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

const LEFTOVERS: &str = r#"remote_bookmarks(glob:"agent/*") | orphans()"#;

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

/// The workspace root holding `cwd`: `-R` does not search parents.
pub fn root(cwd: &Path) -> Result<PathBuf> {
    let out = Command::new("jj").arg("workspace").arg("root").current_dir(cwd).output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// Long-lived bookmarks that sit on trunk by design; `flight.keep-bookmarks` (globs) replaces it.
const KEEP_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop", "dev", "prod", "production", "staging", "release/*"];

fn glob_match(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, rest)) => {
            name.strip_prefix(head).is_some_and(|tail| (0..=tail.len()).any(|i| tail.is_char_boundary(i) && glob_match(rest, &tail[i..])))
        }
    }
}

fn keep_bookmarks(dir: &Path) -> Vec<String> {
    jj_ok(dir, &["config", "get", "flight.keep-bookmarks"])
        .ok()
        .and_then(|s| serde_json::from_str(s.trim()).ok())
        .unwrap_or_else(|| KEEP_BOOKMARKS.iter().map(|s| s.to_string()).collect())
}

/// A bookmark with nothing left to land.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Landed {
    pub name: String,
    /// The remotes that hold it, its deletion to be pushed there.
    pub remotes: Vec<String>,
}

/// Per bookmark whose local target trunk contains: one line for it and one per remote, saying
/// whether trunk contains that target too, or else, for a target rewritten since (hidden), its change.
const LANDED_TEMPLATE: &str = r#"if(remote != "git", name ++ "\t" ++ remote ++ "\t" ++ if(normal_target, if(normal_target.contained_in("::trunk()"), "landed", if(normal_target.hidden(), normal_target.change_id()))) ++ "\n")"#;

/// Bookmarks whose every target is on trunk; `superseded` says whether a rewritten change landed.
fn parse_landed(out: &str, keep: &[String], superseded: impl Fn(&str) -> bool) -> Vec<Landed> {
    let mut rows: Vec<(Landed, bool)> = vec![];
    for line in out.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let (name, remote, state) = (f[0], f.get(1).copied().unwrap_or_default(), f.get(2).copied().unwrap_or_default());
        let landed = state == "landed" || (!state.is_empty() && superseded(state));
        let i = match rows.iter().position(|(l, _)| l.name == name) {
            Some(i) => i,
            None => {
                rows.push((Landed { name: name.into(), remotes: vec![] }, true));
                rows.len() - 1
            }
        };
        rows[i].1 &= landed;
        if !remote.is_empty() {
            rows[i].0.remotes.push(remote.into());
        }
    }
    rows.into_iter().filter(|(l, ok)| *ok && !keep.iter().any(|k| glob_match(k, &l.name))).map(|(l, _)| l).collect()
}

/// Bookmarks trunk already holds, here and on every remote, apart from the long-lived ones.
pub fn landed(dir: &Path) -> Result<Vec<Landed>> {
    let out = jj_ok(dir, &["bookmark", "list", "--all-remotes", "-r", "landed()", "-T", LANDED_TEMPLATE])?;
    let superseded = |change: &str| count(dir, &format!("present(change_id({change})) & ::trunk()")).is_ok_and(|n| n > 0);
    Ok(parse_landed(&out, &keep_bookmarks(dir), superseded))
}

/// Orphaned commits (`orphaned()`), newest first, as `change description` lines.
pub fn orphaned(dir: &Path) -> Result<Vec<String>> {
    let template = r#"change_id.shortest(8) ++ " " ++ if(description, description.first_line(), if(empty, "(empty)", "(no description)")) ++ "\n""#;
    Ok(jj_ok(dir, &["log", "-r", "orphaned()", "--no-graph", "-T", template])?.lines().map(String::from).collect())
}

pub fn abandon_orphaned(dir: &Path) -> Result<()> {
    jj_ok(dir, &["abandon", "orphaned()"]).map(drop)
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

    fn names(l: Vec<Landed>) -> Vec<String> {
        l.into_iter().map(|l| l.name).collect()
    }

    #[test]
    fn parse_landed_keeps_a_bookmark_landed_everywhere() {
        let out = "feat\t\tlanded\nfeat\torigin\tlanded\n";
        assert_eq!(names(parse_landed(out, &[], |_| false)), ["feat"]);
    }

    #[test]
    fn parse_landed_gathers_the_remotes_to_push_the_deletion_to() {
        let out = "feat\t\tlanded\nfeat\torigin\tlanded\nfeat\tfork\tlanded\n";
        assert_eq!(parse_landed(out, &[], |_| false)[0].remotes, ["origin", "fork"]);
    }

    #[test]
    fn parse_landed_when_a_remote_is_ahead_of_trunk_drops_the_bookmark() {
        // GIVEN the remote holds a visible commit trunk does not
        let out = "feat\t\tlanded\nfeat\torigin\t\n";
        assert!(parse_landed(out, &[], |_| true).is_empty());
    }

    #[test]
    fn parse_landed_when_the_remote_holds_a_rewrite_of_a_landed_change_keeps_the_bookmark() {
        let out = "feat\t\tlanded\nfeat\torigin\tabcdef\n";
        assert_eq!(names(parse_landed(out, &[], |c| c == "abcdef")), ["feat"]);
    }

    #[test]
    fn parse_landed_when_the_rewritten_change_never_landed_drops_the_bookmark() {
        let out = "feat\t\tlanded\nfeat\torigin\tabcdef\n";
        assert!(parse_landed(out, &[], |_| false).is_empty());
    }

    #[test]
    fn parse_landed_leaves_out_kept_bookmarks() {
        let out = "release/1.2\t\tlanded\nprod\t\tlanded\n";
        assert!(parse_landed(out, &["prod".into(), "release/*".into()], |_| false).is_empty());
    }

    #[test]
    fn glob_match_with_a_star_matches_any_suffix() {
        assert!(glob_match("release/*", "release/1.2"));
    }

    #[test]
    fn glob_match_without_a_star_needs_the_whole_name() {
        assert!(!glob_match("main", "main-old"));
    }

    #[test]
    fn parse_leftovers_reads_landed_and_the_commit_time() {
        let row = &parse_leftovers("abcd\tfix\tagent/x/a@origin\tlanded\t100\n")[0];
        assert_eq!((row.landed, row.committed), (true, 100));
    }
}
