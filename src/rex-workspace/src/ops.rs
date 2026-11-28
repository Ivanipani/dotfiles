//! Set lifecycle: shared by the CLI and the TUI. Each operation returns notes to show the user.

use crate::manifest::{session_label, validate_name, Env, Project};
use crate::park::{self, Snapshot};
use crate::rex::{project_window, Rex};
use crate::{jj, procs, repos};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;

/// Resolve names to repositories under the roots; `repo:feature` names an extra workspace of `repo`.
/// Unknown names, and names already in the set, are an error.
pub fn resolve_repos(env: &Env, set: &str, names: &[String]) -> Result<Vec<Project>> {
    let have: Vec<_> = env.load(set)?.projects.into_iter().map(|p| p.name).collect();
    let all = repos::scan(&env.roots);
    let mut picks: Vec<Project> = vec![];
    let mut missing = vec![];
    for name in names {
        let (repo, feature) = name.split_once(':').map_or((name.as_str(), None), |(r, f)| (r, Some(f)));
        let pick = all.iter().find(|p| p.name == repo).and_then(|p| match feature {
            Some(f) if f.is_empty() || validate_name(f).is_err() => None,
            Some(f) => Some(p.with_feature(f)),
            None => Some(p.clone()),
        });
        match pick {
            Some(p) if !have.contains(&p.name) && !picks.iter().any(|q| q.name == p.name) => picks.push(p),
            _ => missing.push(name.clone()),
        }
    }
    if !missing.is_empty() {
        bail!("unknown or already added: {}", missing.join(", "));
    }
    Ok(picks)
}

pub fn add(env: &Env, rex: &Rex, set: &str, picks: &[Project]) -> Result<Vec<String>> {
    validate_name(set)?;
    let mut manifest = env.load(set)?;
    let session = rex.find(&session_label(set));
    let mut notes = vec![];
    for p in picks {
        let dir = env.set_dir(set).join(&p.name);
        let source = p.source.as_deref().ok_or_else(|| anyhow!("{} has no source", p.name))?;
        if manifest.projects.iter().any(|q| q.name == p.name) {
            bail!("{} is already in {set}", p.name);
        }
        // jj does not create missing parents.
        std::fs::create_dir_all(env.set_dir(set))?;
        if let Err(e) = jj::workspace_add(source, &p.workspace_name(set), &dir) {
            // Do not leave a bare directory that would block creating the set again.
            if !env.exists(set) {
                let _ = std::fs::remove_dir_all(env.set_dir(set));
            }
            return Err(e);
        }
        manifest.projects.push(p.clone());
        // Save per project so a later failure keeps the ones already created.
        env.save(&manifest)?;
        if let Some(session) = &session {
            rex.add_window(session, &project_window(&p.name, &dir.to_string_lossy()))?;
        }
        notes.push(format!("added {}", p.name));
    }
    Ok(notes)
}

pub fn remove(env: &Env, rex: &Rex, set: &str, names: &[String]) -> Result<Vec<String>> {
    let mut manifest = env.load(set)?;
    let unknown: Vec<_> = names.iter().filter(|n| !manifest.projects.iter().any(|p| &p.name == *n)).cloned().collect();
    if !unknown.is_empty() {
        bail!("unknown: {}", unknown.join(", "));
    }
    let session = rex.find(&session_label(set));
    let mut notes = vec![];
    for name in names {
        let dir = env.set_dir(set).join(name);
        let ws = manifest.projects.iter().find(|p| &p.name == name).map(|p| p.workspace_name(set)).unwrap_or_default();
        notes.extend(jj::forget(&dir, &ws)?);
        let _ = std::fs::remove_dir_all(&dir);
        if let Some(session) = &session {
            rex.close_window(session, name)?;
        }
        manifest.projects.retain(|p| &p.name != name);
        env.save(&manifest)?;
        notes.push(format!("removed {name}"));
    }
    Ok(notes)
}

/// Push stranded work under agent bookmarks, forget the workspaces, remove the directory and the session.
/// The session is killed last but even when cleanup fails or the directory is already gone.
pub fn delete(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    let session = rex.find(&session_label(set));
    // Stopped processes would outlive the session's terminals.
    if session.is_some() && park::is_paused(env, set) {
        signal_set(rex, set, "CONT")?;
    }
    let cleaned = remove_set_dir(env, set);
    if session.is_none() {
        return cleaned.map(|notes| with_deleted(notes, set));
    }
    let killed = session.map(|id| rex.kill(&id)).transpose();
    match (cleaned, killed) {
        (Ok(notes), Ok(_)) => Ok(with_deleted(notes, set)),
        (Err(e), Ok(_)) if !env.set_dir(set).exists() => Ok(vec![format!("closed session ws-{set}; {e:#}")]),
        (Err(e), _) | (Ok(_), Err(e)) => Err(e),
    }
}

fn with_deleted(mut notes: Vec<String>, set: &str) -> Vec<String> {
    notes.push(format!("deleted {set}"));
    notes
}

fn remove_set_dir(env: &Env, set: &str) -> Result<Vec<String>> {
    let dir = env.set_dir(set);
    if !dir.exists() {
        bail!("no set named {set}");
    }
    let manifest = env.load(set)?;
    let mut notes = vec![];
    let mut projects: Vec<_> = std::fs::read_dir(&dir)?.flatten().filter(|e| e.path().is_dir()).collect();
    projects.sort_by_key(|e| e.file_name());
    for project in projects {
        let name = project.file_name().to_string_lossy().into_owned();
        let ws = manifest.projects.iter().find(|p| p.name == name).map_or_else(|| set.to_string(), |p| p.workspace_name(set));
        notes.extend(jj::forget(&project.path(), &ws)?);
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(notes)
}

/// Switch to the set's session, resuming it if paused; without one, create it from the saved layout
/// of a closed set, else one tab per project.
pub fn open(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    let manifest = env.load(set)?;
    if manifest.projects.is_empty() {
        bail!("no set named {set}");
    }
    let mut notes = vec![];
    let id = match rex.find(&session_label(set)) {
        Some(id) => {
            if park::is_paused(env, set) {
                notes.extend(resume(env, rex, set)?);
            }
            id
        }
        None => {
            let (windows, active) = match park::load(env, set) {
                Some(snapshot) => park::restore(snapshot, &manifest, env),
                None => (park::fresh(&manifest, env), 0),
            };
            let id = rex.create(&session_label(set), &windows, active)?;
            park::forget(env, set);
            let _ = park::mark_paused(env, set, false);
            id
        }
    };
    // No app attached (headless, or several): the session is still created.
    notes.push(match rex.select(&id) {
        Ok(()) => format!("opened {set}"),
        Err(e) => format!("opened {id}; could not switch: {e}"),
    });
    Ok(notes)
}

fn require_session(rex: &Rex, set: &str) -> Result<String> {
    rex.find(&session_label(set)).ok_or_else(|| anyhow!("{set} has no open session"))
}

/// Save the session's layout and end it, freeing every process; `open` brings it back.
pub fn close(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    let id = require_session(rex, set)?;
    let (windows, active) = rex.capture(&id, &env.set_dir(set).to_string_lossy())?;
    let projects = env.load(set)?.projects.into_iter().map(|p| p.name).collect();
    park::save(env, set, &Snapshot { projects, active, windows })?;
    if park::is_paused(env, set) {
        signal_set(rex, set, "CONT")?;
        park::mark_paused(env, set, false)?;
    }
    rex.kill(&id)?;
    let mut notes = vec![format!("closed {set}; open it to bring its tabs back")];
    notes.extend(stranded_note(env, set));
    Ok(notes)
}

/// Stop every process under the set's panes: no CPU, memory kept, back instantly with `resume`.
pub fn pause(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    require_session(rex, set)?;
    if park::is_paused(env, set) {
        return Ok(vec![format!("{set} is already paused")]);
    }
    let (count, rss_kb) = signal_set(rex, set, "STOP")?;
    park::mark_paused(env, set, true)?;
    Ok(vec![format!("paused {set}: {count} process(es) stopped, {} held in memory", procs::mem(rss_kb))])
}

pub fn resume(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    require_session(rex, set)?;
    let (count, _) = signal_set(rex, set, "CONT")?;
    park::mark_paused(env, set, false)?;
    Ok(vec![format!("resumed {set}: {count} process(es)")])
}

/// Signal the processes under the set's panes, excluding this program's own; the count signalled and their memory.
/// Processes that are not ours (the root-owned `login` of each pane) are skipped.
fn signal_set(rex: &Rex, set: &str, signal: &str) -> Result<(usize, u64)> {
    let snap = procs::collect(rex, Some(&session_label(set)))?;
    let done: Vec<_> = snap.rows.iter().map(|r| &r.proc).filter(|p| procs::signal(signal, p.pid).is_ok()).collect();
    if done.is_empty() && !snap.rows.is_empty() {
        bail!("could not signal any process of {set}");
    }
    Ok((done.len(), procs::Usage::of(done).rss_kb))
}

/// Change what the set is for: `goal` replaces it (empty clears it), `links` are appended unless `clear_links`.
pub fn describe(env: &Env, set: &str, goal: Option<&str>, links: &[String], clear_links: bool) -> Result<Vec<String>> {
    if !env.exists(set) {
        bail!("no set named {set}");
    }
    let mut manifest = env.load(set)?;
    if let Some(goal) = goal {
        manifest.goal = Some(goal.trim()).filter(|g| !g.is_empty()).map(String::from);
    }
    if clear_links {
        manifest.links.clear();
    }
    for link in links {
        if !manifest.links.contains(link) {
            manifest.links.push(link.clone());
        }
    }
    env.save(&manifest)?;
    Ok(vec![format!("described {set}; its AGENTS.md states the goal")])
}

/// One project's outcome of `sync`.
pub struct SyncReport {
    pub project: String,
    /// Fetch failed (offline, no remote): the rebase still ran against what was already fetched.
    pub fetch_error: Option<String>,
    pub result: Result<jj::Synced, String>,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        let result = match &self.result {
            Ok(jj::Synced::NoTrunk) => "skipped: no trunk() to rebase onto".into(),
            Ok(jj::Synced::UpToDate { conflicts: 0 }) => "up to date".into(),
            Ok(jj::Synced::UpToDate { conflicts }) => format!("up to date, {conflicts} conflicted commit(s)"),
            Ok(jj::Synced::Rebased { behind, conflicts: 0 }) => format!("rebased onto trunk ({behind} new)"),
            Ok(jj::Synced::Rebased { behind, conflicts }) => {
                format!("rebased onto trunk ({behind} new), {conflicts} conflicted commit(s)")
            }
            Err(e) => e.clone(),
        };
        match &self.fetch_error {
            Some(e) => format!("{}: {result} (fetch failed: {e})", self.project),
            None => format!("{}: {result}", self.project),
        }
    }

    pub fn ok(&self) -> bool {
        matches!(self.result, Ok(jj::Synced::NoTrunk | jj::Synced::UpToDate { conflicts: 0 } | jj::Synced::Rebased { conflicts: 0, .. }))
    }
}

/// Fetch each repository once, then rebase every project's `@` onto `trunk()`. Repositories run in
/// parallel; workspaces of one repository run in turn, since they share its operation log.
pub fn sync(env: &Env, set: &str) -> Result<Vec<SyncReport>> {
    let manifest = env.load(set)?;
    if manifest.projects.is_empty() {
        bail!("no set named {set}");
    }
    let mut repos: Vec<(String, Vec<String>)> = vec![];
    for p in manifest.projects {
        let key = p.source.clone().unwrap_or_else(|| p.name.clone());
        match repos.iter_mut().find(|(k, _)| *k == key) {
            Some((_, names)) => names.push(p.name),
            None => repos.push((key, vec![p.name])),
        }
    }
    let dir = env.set_dir(set);
    let reports = std::thread::scope(|scope| {
        let handles: Vec<_> = repos
            .iter()
            .map(|(_, names)| {
                let dir = &dir;
                scope.spawn(move || {
                    let fetch_error = jj::fetch(&dir.join(&names[0])).err().map(|e| format!("{e:#}"));
                    names
                        .iter()
                        .map(|name| SyncReport {
                            project: name.clone(),
                            fetch_error: fetch_error.clone(),
                            result: jj::rebase_onto_trunk(&dir.join(name)).map_err(|e| format!("{e:#}")),
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect::<Vec<_>>()
    });
    Ok(reports)
}

/// The set this runs in: the one the working directory is inside, else that of the `ws-` session.
pub fn current_set(env: &Env, rex: &Rex) -> Option<String> {
    let here = std::env::current_dir().ok().and_then(|d| env.set_at(&d));
    here.or_else(|| rex.current_label()?.strip_prefix("ws-").map(String::from))
}

pub fn require_current_set(env: &Env, rex: &Rex) -> Result<String> {
    current_set(env, rex).ok_or_else(|| anyhow!("no set given, and neither inside a set's directory nor a ws- session"))
}

/// A stranded head of one of the set's projects.
#[derive(Debug, Serialize)]
pub struct StrandedRow {
    pub project: String,
    #[serde(flatten)]
    pub head: jj::Stranded,
}

/// Work in the set's workspaces that no remote holds. Projects without a remote are left out: there
/// is nowhere to push them.
pub fn stranded(env: &Env, set: &str) -> Result<Vec<StrandedRow>> {
    let manifest = env.load(set)?;
    if manifest.projects.is_empty() {
        bail!("no set named {set}");
    }
    let mut rows = vec![];
    for p in manifest.projects {
        let dir = env.set_dir(set).join(&p.name);
        if !jj::has_remote(&dir)? {
            continue;
        }
        rows.extend(jj::stranded(&dir)?.into_iter().map(|head| StrandedRow { project: p.name.clone(), head }));
    }
    Ok(rows)
}

fn stranded_note(env: &Env, set: &str) -> Option<String> {
    let commits: usize = stranded(env, set).ok()?.iter().map(|r| r.head.commits).sum();
    (commits > 0).then(|| format!("{commits} commit(s) exist only on this machine; see rex-workspace stranded {set}"))
}

/// Why a leftover is still around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Leftover {
    /// On trunk already: safe to delete.
    Landed,
    /// Its workspace is a set on this machine, or it changed recently.
    Active,
    /// No local set owns it and it has not changed in a while.
    Stale,
    /// A head nothing points at.
    Orphan,
}

#[derive(Debug, Serialize)]
pub struct ReapRow {
    pub repo: String,
    pub state: Leftover,
    pub bookmark: Option<String>,
    pub change: String,
    pub description: String,
    pub age_days: i64,
}

/// The workspace an `agent/<ws>/…` bookmark belongs to.
fn bookmark_owner(bookmark: &str) -> Option<&str> {
    bookmark.strip_prefix("agent/")?.split('/').next()
}

fn classify(l: &jj::Leftover, local: &[String], now: i64, stale_days: i64) -> Leftover {
    let Some(bookmark) = &l.bookmark else { return Leftover::Orphan };
    if l.landed {
        return Leftover::Landed;
    }
    let owned = bookmark_owner(bookmark).is_some_and(|ws| local.iter().any(|w| w == ws));
    if owned || now - l.committed < stale_days * 86_400 {
        Leftover::Active
    } else {
        Leftover::Stale
    }
}

/// Agent bookmarks and orphaned heads in every repository of every set, fetched first. `prune`
/// deletes the landed bookmarks; nothing else is ever deleted.
pub fn reap(env: &Env, stale_days: i64, prune: bool) -> Result<(Vec<ReapRow>, Vec<String>)> {
    let sets = env.sets();
    let local: Vec<String> = sets.iter().flat_map(|m| m.projects.iter().map(|p| p.workspace_name(&m.name))).collect();
    // One workspace per repository; querying it leaves the source checkout alone.
    let mut repos: Vec<(String, std::path::PathBuf)> = vec![];
    for m in &sets {
        for p in &m.projects {
            let key = p.source.clone().unwrap_or_else(|| p.name.clone());
            if !repos.iter().any(|(k, _)| *k == key) {
                repos.push((key, env.set_dir(&m.name).join(&p.name)));
            }
        }
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (mut rows, mut notes) = (vec![], vec![]);
    for (source, dir) in repos {
        let repo = std::path::Path::new(&source).file_name().map_or(source.clone(), |n| n.to_string_lossy().into_owned());
        if let Err(e) = jj::fetch(&dir) {
            notes.push(format!("{repo}: fetch failed, using what was fetched before: {e:#}"));
        }
        for l in jj::leftovers(&dir)? {
            let state = classify(&l, &local, now, stale_days);
            if prune && state == Leftover::Landed {
                let name = l.bookmark.as_deref().unwrap_or_default();
                match jj::delete_bookmark(&dir, name, &l.remotes) {
                    Ok(()) => notes.push(format!("{repo}: deleted landed {name}")),
                    Err(e) => notes.push(format!("{repo}: could not delete {name}: {e:#}")),
                }
                continue;
            }
            let age_days = (now - l.committed).max(0) / 86_400;
            rows.push(ReapRow { repo: repo.clone(), state, bookmark: l.bookmark, change: l.change, description: l.description, age_days });
        }
    }
    Ok((rows, notes))
}

/// What `tidy` would clear out of one repository.
#[derive(Debug, Serialize)]
pub struct Tidy {
    pub landed: Vec<jj::Landed>,
    /// Orphaned commits, when asked for.
    pub orphans: Vec<String>,
}

impl Tidy {
    pub fn is_empty(&self) -> bool {
        self.landed.is_empty() && self.orphans.is_empty()
    }
}

/// Fetch, then find landed bookmarks and, with `orphans`, orphaned commits.
pub fn tidy_plan(dir: &std::path::Path, orphans: bool) -> Result<(Tidy, Vec<String>)> {
    let mut notes = vec![];
    if jj::has_remote(dir)? {
        if let Err(e) = jj::fetch(dir) {
            notes.push(format!("fetch failed, using what was fetched before: {e:#}"));
        }
    }
    let orphans = if orphans { jj::orphaned(dir)? } else { vec![] };
    Ok((Tidy { landed: jj::landed(dir)?, orphans }, notes))
}

/// Delete the plan's bookmarks here and on their remotes, then abandon its orphans.
pub fn tidy(dir: &std::path::Path, plan: &Tidy) -> Vec<String> {
    let mut notes: Vec<String> = plan
        .landed
        .iter()
        .map(|l| match jj::delete_bookmark(dir, &l.name, &l.remotes) {
            Ok(()) => format!("deleted landed {}", l.name),
            Err(e) => format!("could not delete {}: {e:#}", l.name),
        })
        .collect();
    if !plan.orphans.is_empty() {
        notes.push(match jj::abandon_orphaned(dir) {
            Ok(()) => format!("abandoned {} orphaned commit(s); jj undo brings them back", plan.orphans.len()),
            Err(e) => format!("could not abandon orphans: {e:#}"),
        });
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn env_with_repos(names: &[&str]) -> (tempfile::TempDir, Env) {
        let tmp = tempfile::tempdir().unwrap();
        for n in names {
            std::fs::create_dir_all(tmp.path().join("src").join(n).join(".jj/repo")).unwrap();
        }
        let env = Env { workspaces_dir: tmp.path().join("ws"), roots: vec![tmp.path().join("src")] };
        (tmp, env)
    }

    fn resolved(env: &Env, names: &[&str]) -> Vec<String> {
        let names: Vec<_> = names.iter().map(|s| s.to_string()).collect();
        resolve_repos(env, "demo", &names).unwrap().into_iter().map(|p| p.name).collect()
    }

    #[test]
    fn resolve_repos_with_a_feature_suffix_names_an_extra_workspace() {
        let (_tmp, env) = env_with_repos(&["api"]);
        assert_eq!(resolved(&env, &["api", "api:v2"]), ["api", "api-v2"]);
    }

    #[test]
    fn resolve_repos_with_a_feature_gives_it_the_feature_workspace_name() {
        let (_tmp, env) = env_with_repos(&["api"]);
        let picks = resolve_repos(&env, "demo", &["api:v2".to_string()]).unwrap();
        assert_eq!(picks[0].workspace_name("demo"), "demo-v2");
    }

    #[test]
    fn resolve_repos_for_an_unknown_repo_fails() {
        let (_tmp, env) = env_with_repos(&["api"]);
        assert!(resolve_repos(&env, "demo", &["nope".to_string()]).is_err());
    }

    #[test]
    fn resolve_repos_for_a_repo_already_in_the_set_fails() {
        // GIVEN api already in the set
        let (_tmp, env) = env_with_repos(&["api"]);
        let api = resolve_repos(&env, "demo", &["api".to_string()]).unwrap();
        env.save(&crate::manifest::Manifest { name: "demo".into(), projects: api, ..Default::default() }).unwrap();
        // WHEN / THEN
        assert!(resolve_repos(&env, "demo", &["api".to_string()]).is_err());
    }

    #[test]
    fn resolve_repos_for_a_new_version_of_a_repo_already_in_the_set_succeeds() {
        // GIVEN api already in the set
        let (_tmp, env) = env_with_repos(&["api"]);
        let api = resolve_repos(&env, "demo", &["api".to_string()]).unwrap();
        env.save(&crate::manifest::Manifest { name: "demo".into(), projects: api, ..Default::default() }).unwrap();
        // WHEN / THEN
        assert_eq!(resolved(&env, &["api:v2"]), ["api-v2"]);
    }

    #[test]
    fn describe_replaces_the_goal() {
        // GIVEN a set with a goal
        let (_tmp, env) = env_with_repos(&[]);
        env.save(&crate::manifest::Manifest { name: "demo".into(), goal: Some("old".into()), ..Default::default() }).unwrap();
        // WHEN
        describe(&env, "demo", Some("new"), &[], false).unwrap();
        // THEN
        assert_eq!(env.load("demo").unwrap().goal.as_deref(), Some("new"));
    }

    #[test]
    fn describe_with_an_empty_goal_clears_it() {
        let (_tmp, env) = env_with_repos(&[]);
        env.save(&crate::manifest::Manifest { name: "demo".into(), goal: Some("old".into()), ..Default::default() }).unwrap();
        describe(&env, "demo", Some("  "), &[], false).unwrap();
        assert_eq!(env.load("demo").unwrap().goal, None);
    }

    #[test]
    fn describe_with_a_link_already_present_does_not_repeat_it() {
        // GIVEN
        let (_tmp, env) = env_with_repos(&[]);
        let links = vec!["https://x/1".to_string()];
        env.save(&crate::manifest::Manifest { name: "demo".into(), links: links.clone(), ..Default::default() }).unwrap();
        // WHEN
        describe(&env, "demo", None, &links, false).unwrap();
        // THEN
        assert_eq!(env.load("demo").unwrap().links, links);
    }

    #[test]
    fn describe_for_a_missing_set_fails() {
        let (_tmp, env) = env_with_repos(&[]);
        assert!(describe(&env, "nope", Some("x"), &[], false).is_err());
    }

    /// Run jj in `dir` with a neutral config; its stdout.
    fn run(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("jj").args(args).current_dir(dir).env("JJ_CONFIG", "/dev/null")
            .env("JJ_USER", "t").env("JJ_EMAIL", "t@t").output().unwrap();
        assert!(out.status.success(), "jj {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A set `demo` with one project `api`, a workspace of a fresh repo whose trunk is `main@origin`.
    /// Returns the source repository.
    fn remote_set(tmp: &Path, env: &Env) -> PathBuf {
        let (origin, src) = (tmp.join("origin.git"), tmp.join("src/api"));
        std::fs::create_dir_all(&src).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q", "--bare"]).arg(&origin).status().unwrap().success());
        run(&src, &["git", "init"]);
        run(&src, &["git", "remote", "add", "origin", &origin.to_string_lossy()]);
        run(&src, &["commit", "-m", "base"]);
        run(&src, &["bookmark", "create", "main", "-r", "@-"]);
        run(&src, &["git", "push", "-b", "main"]);
        let pick = Project { name: "api".into(), source: Some(src.to_string_lossy().into()), feature: None };
        add(env, &Rex::locate(), "demo", &[pick]).unwrap();
        src
    }

    /// `remote_set`, then trunk moves on in the source.
    fn synced_set(tmp: &Path, env: &Env) -> PathBuf {
        let src = remote_set(tmp, env);
        run(&src, &["new", "main", "-m", "upstream"]);
        run(&src, &["bookmark", "set", "main", "-r", "@"]);
        run(&src, &["git", "push", "-b", "main"]);
        src
    }

    #[test]
    fn workspace_add_when_source_is_off_trunk_starts_the_workspace_on_trunk() {
        // GIVEN a source whose working copy sits on unpushed work
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        run(&src, &["describe", "-m", "wip"]);
        run(&src, &["new"]);
        let dir = tmp.path().join("second");
        // WHEN
        jj::workspace_add(&src.to_string_lossy(), "second", &dir).unwrap();
        // THEN the new workspace's parent is trunk
        assert_eq!(run(&dir, &["log", "-r", "@-", "--no-graph", "-T", "bookmarks"]).trim(), "main");
    }

    #[test]
    fn sync_when_trunk_moved_rebases_the_workspace_onto_it() {
        // GIVEN
        let (tmp, env) = env_with_repos(&[]);
        synced_set(tmp.path(), &env);
        // WHEN
        let reports = sync(&env, "demo").unwrap();
        // THEN
        assert_eq!(reports[0].result, Ok(jj::Synced::Rebased { behind: 1, conflicts: 0 }));
    }

    #[test]
    fn sync_twice_reports_the_second_as_up_to_date() {
        // GIVEN a set synced once
        let (tmp, env) = env_with_repos(&[]);
        synced_set(tmp.path(), &env);
        sync(&env, "demo").unwrap();
        // WHEN
        let reports = sync(&env, "demo").unwrap();
        // THEN
        assert_eq!(reports[0].result, Ok(jj::Synced::UpToDate { conflicts: 0 }));
    }

    #[test]
    fn sync_without_a_remote_trunk_skips_the_project() {
        // GIVEN a repo with no remote
        let (tmp, env) = env_with_repos(&[]);
        let src = tmp.path().join("src/api");
        std::fs::create_dir_all(&src).unwrap();
        assert!(std::process::Command::new("jj").args(["git", "init"]).current_dir(&src).output().unwrap().status.success());
        let pick = Project { name: "api".into(), source: Some(src.to_string_lossy().into()), feature: None };
        add(&env, &Rex::locate(), "demo", &[pick]).unwrap();
        // WHEN
        let reports = sync(&env, "demo").unwrap();
        // THEN
        assert_eq!(reports[0].result, Ok(jj::Synced::NoTrunk));
    }

    #[test]
    fn add_creates_the_workspace_in_a_new_set_directory() {
        // GIVEN a real jj repo and a set that does not exist yet
        let (tmp, env) = env_with_repos(&[]);
        let src = tmp.path().join("src/api");
        std::fs::create_dir_all(&src).unwrap();
        let init = std::process::Command::new("jj").args(["git", "init"]).current_dir(&src).output().unwrap();
        assert!(init.status.success());
        let pick = Project { name: "api".into(), source: Some(src.to_string_lossy().into()), feature: None };
        // WHEN
        add(&env, &Rex::locate(), "demo", &[pick]).unwrap();
        // THEN
        assert!(env.set_dir("demo").join("api/.jj").is_dir());
    }

    #[test]
    fn add_when_jj_fails_leaves_no_directory_behind() {
        // GIVEN a source that is not a repository
        let (tmp, env) = env_with_repos(&[]);
        let bad = Project { name: "x".into(), source: Some(tmp.path().join("missing").to_string_lossy().into()), feature: None };
        // WHEN
        let _ = add(&env, &Rex::locate(), "demo", &[bad]);
        // THEN a retry of the same name is not blocked
        assert!(!env.set_dir("demo").exists());
    }

    /// Commit a file in `dir` as `message`, leaving an empty `@` on top.
    fn commit(dir: &Path, file: &str, message: &str) {
        std::fs::write(dir.join(file), message).unwrap();
        run(dir, &["commit", "-m", message]);
    }

    fn remote_bookmarks(src: &Path) -> String {
        run(src, &["git", "fetch"]);
        run(src, &["bookmark", "list", "--all-remotes", "-T", r#"if(remote, name ++ "@" ++ remote ++ "\n")"#])
    }

    fn api(env: &Env) -> PathBuf {
        env.set_dir("demo").join("api")
    }

    fn workspace_config(dir: &Path) -> PathBuf {
        PathBuf::from(jj::jj_ok(dir, &["config", "path", "--workspace"]).unwrap().trim())
    }

    #[test]
    fn add_writes_no_workspace_jj_config() {
        // GIVEN
        let (tmp, env) = env_with_repos(&[]);
        // WHEN
        remote_set(tmp.path(), &env);
        // THEN
        assert!(!workspace_config(&api(&env)).exists());
    }

    #[test]
    fn stranded_lists_an_unpushed_commit() {
        // GIVEN
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        // WHEN
        let rows = stranded(&env, "demo").unwrap();
        // THEN
        assert_eq!(rows.iter().map(|r| r.head.description.as_str()).collect::<Vec<_>>(), ["feature"]);
    }

    #[test]
    fn stranded_counts_every_commit_under_the_head() {
        // GIVEN two unpushed commits
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "one");
        commit(&api(&env), "b", "two");
        // WHEN
        let rows = stranded(&env, "demo").unwrap();
        // THEN
        assert_eq!(rows[0].head.commits, 2);
    }

    #[test]
    fn stranded_after_a_push_is_empty() {
        // GIVEN a pushed commit
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        jj::jj_ok(&api(&env), &["git", "push", "--named", "agent/demo/feature=@-"]).unwrap();
        // WHEN / THEN
        assert!(stranded(&env, "demo").unwrap().is_empty());
    }

    #[test]
    fn stranded_without_a_remote_leaves_the_project_out() {
        // GIVEN a repo with no remote and a commit in its workspace
        let (tmp, env) = env_with_repos(&[]);
        let src = tmp.path().join("src/api");
        std::fs::create_dir_all(&src).unwrap();
        run(&src, &["git", "init"]);
        let pick = Project { name: "api".into(), source: Some(src.to_string_lossy().into()), feature: None };
        add(&env, &Rex::locate(), "demo", &[pick]).unwrap();
        commit(&api(&env), "a", "local only");
        // WHEN / THEN
        assert!(stranded(&env, "demo").unwrap().is_empty());
    }

    #[test]
    fn delete_pushes_committed_work_under_a_saved_bookmark() {
        // GIVEN committed work and an empty @, as jj commit leaves it
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        // WHEN
        delete(&env, &Rex::locate(), "demo").unwrap();
        // THEN
        assert!(remote_bookmarks(&src).contains("agent/demo/saved@origin"));
    }

    #[test]
    fn delete_pushes_an_undescribed_working_copy() {
        // GIVEN uncommitted, undescribed changes in @
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        std::fs::write(api(&env).join("wip"), "wip").unwrap();
        // WHEN
        delete(&env, &Rex::locate(), "demo").unwrap();
        // THEN
        assert!(remote_bookmarks(&src).contains("agent/demo/saved@origin"));
    }

    #[test]
    fn delete_pushes_work_on_its_own_bookmark_under_that_name() {
        // GIVEN work on the agent's own unpushed bookmark
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        run(&api(&env), &["bookmark", "create", "agent/demo/feature", "-r", "@-"]);
        // WHEN
        delete(&env, &Rex::locate(), "demo").unwrap();
        // THEN
        assert!(remote_bookmarks(&src).contains("agent/demo/feature@origin"));
    }

    #[test]
    fn reap_reports_a_bookmark_already_on_trunk_as_landed() {
        // GIVEN an agent bookmark that main has since reached
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        jj::jj_ok(&api(&env), &["git", "push", "--named", "agent/demo/feature=@-"]).unwrap();
        run(&src, &["git", "fetch"]);
        run(&src, &["bookmark", "set", "main", "-r", r#"description(substring:"feature")"#]);
        run(&src, &["git", "push", "-b", "main"]);
        // WHEN
        let (rows, _) = reap(&env, 14, false).unwrap();
        // THEN
        assert_eq!(rows.iter().map(|r| r.state).collect::<Vec<_>>(), [Leftover::Landed]);
    }

    #[test]
    fn reap_with_prune_deletes_a_landed_bookmark_from_the_remote() {
        // GIVEN a landed agent bookmark
        let (tmp, env) = env_with_repos(&[]);
        let src = remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        jj::jj_ok(&api(&env), &["git", "push", "--named", "agent/demo/feature=@-"]).unwrap();
        run(&src, &["git", "fetch"]);
        run(&src, &["bookmark", "set", "main", "-r", r#"description(substring:"feature")"#]);
        run(&src, &["git", "push", "-b", "main"]);
        // WHEN
        reap(&env, 14, true).unwrap();
        // THEN
        assert!(!remote_bookmarks(&src).contains("agent/"));
    }

    /// `remote_set` with `feature` pushed from the workspace, then landed on main from the source.
    fn landed_feature(tmp: &Path, env: &Env) -> PathBuf {
        let src = remote_set(tmp, env);
        commit(&api(env), "a", "feature");
        jj::jj_ok(&api(env), &["git", "push", "--named", "feature=@-"]).unwrap();
        run(&src, &["git", "fetch"]);
        run(&src, &["bookmark", "set", "main", "-r", r#"description(substring:"feature")"#]);
        run(&src, &["git", "push", "-b", "main"]);
        src
    }

    #[test]
    fn tidy_plan_lists_a_bookmark_trunk_contains() {
        // GIVEN
        let (tmp, env) = env_with_repos(&[]);
        landed_feature(tmp.path(), &env);
        // WHEN
        let (plan, _) = tidy_plan(&api(&env), false).unwrap();
        // THEN
        assert_eq!(plan.landed.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["feature"]);
    }

    #[test]
    fn tidy_plan_leaves_out_the_trunk_bookmark() {
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        assert!(tidy_plan(&api(&env), false).unwrap().0.landed.is_empty());
    }

    #[test]
    fn tidy_plan_leaves_out_a_bookmark_not_on_trunk() {
        // GIVEN pushed work main has not reached
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "feature");
        jj::jj_ok(&api(&env), &["git", "push", "--named", "feature=@-"]).unwrap();
        // WHEN / THEN
        assert!(tidy_plan(&api(&env), false).unwrap().0.landed.is_empty());
    }

    #[test]
    fn tidy_deletes_a_landed_bookmark_from_the_remote() {
        // GIVEN
        let (tmp, env) = env_with_repos(&[]);
        let src = landed_feature(tmp.path(), &env);
        let (plan, _) = tidy_plan(&api(&env), false).unwrap();
        // WHEN
        tidy(&api(&env), &plan);
        // THEN
        assert!(!remote_bookmarks(&src).contains("feature@origin"));
    }

    #[test]
    fn tidy_with_orphans_abandons_work_nothing_points_at() {
        // GIVEN work walked away from with jj new
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "abandoned path");
        run(&api(&env), &["new", "main"]);
        let (plan, _) = tidy_plan(&api(&env), true).unwrap();
        // WHEN
        tidy(&api(&env), &plan);
        // THEN
        assert!(jj::orphaned(&api(&env)).unwrap().is_empty());
    }

    #[test]
    fn tidy_plan_without_orphans_lists_no_orphans() {
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "abandoned path");
        run(&api(&env), &["new", "main"]);
        assert!(tidy_plan(&api(&env), false).unwrap().0.orphans.is_empty());
    }

    #[test]
    fn reap_reports_a_head_nothing_points_at_as_an_orphan() {
        // GIVEN work the agent walked away from with jj new
        let (tmp, env) = env_with_repos(&[]);
        remote_set(tmp.path(), &env);
        commit(&api(&env), "a", "abandoned path");
        run(&api(&env), &["new", "main"]);
        // WHEN
        let (rows, _) = reap(&env, 14, false).unwrap();
        // THEN
        assert_eq!(rows.iter().map(|r| r.state).collect::<Vec<_>>(), [Leftover::Orphan]);
    }

    fn leftover(bookmark: Option<&str>, landed: bool, committed: i64) -> jj::Leftover {
        jj::Leftover { bookmark: bookmark.map(String::from), remotes: vec![], change: "c".into(), description: String::new(), landed, committed }
    }

    #[test]
    fn classify_an_old_bookmark_of_a_local_set_as_active() {
        let l = leftover(Some("agent/demo/x"), false, 0);
        assert_eq!(classify(&l, &["demo".into()], 100 * 86_400, 14), Leftover::Active);
    }

    #[test]
    fn classify_an_old_bookmark_no_local_set_owns_as_stale() {
        let l = leftover(Some("agent/gone/x"), false, 0);
        assert_eq!(classify(&l, &["demo".into()], 100 * 86_400, 14), Leftover::Stale);
    }

    #[test]
    fn classify_a_recent_bookmark_no_local_set_owns_as_active() {
        // GIVEN a bookmark pushed a day ago from another machine
        let l = leftover(Some("agent/remote/x"), false, 99 * 86_400);
        // WHEN / THEN
        assert_eq!(classify(&l, &[], 100 * 86_400, 14), Leftover::Active);
    }

    #[test]
    fn classify_a_landed_bookmark_as_landed_even_when_owned() {
        let l = leftover(Some("agent/demo/x"), true, 0);
        assert_eq!(classify(&l, &["demo".into()], 0, 14), Leftover::Landed);
    }

    #[test]
    fn bookmark_owner_is_the_segment_after_agent() {
        assert_eq!(bookmark_owner("agent/demo-v2/fix"), Some("demo-v2"));
    }
}
