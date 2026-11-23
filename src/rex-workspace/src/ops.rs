//! Set lifecycle: shared by the CLI and the TUI. Each operation returns notes to show the user.

use crate::manifest::{session_label, validate_name, Env, Project};
use crate::park::{self, Snapshot};
use crate::rex::{project_window, Rex};
use crate::{jj, procs, repos};
use anyhow::{anyhow, bail, Result};

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
        notes.extend(jj::forget(&dir)?);
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

/// Save dirty workspaces under bookmarks, forget them, remove the directory and the session.
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
    let mut notes = vec![];
    let mut projects: Vec<_> = std::fs::read_dir(&dir)?.flatten().filter(|e| e.path().is_dir()).collect();
    projects.sort_by_key(|e| e.file_name());
    for project in projects {
        notes.extend(jj::forget(&project.path())?);
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
    Ok(vec![format!("closed {set}; open it to bring its tabs back")])
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

/// The set of the session this runs in, when it is a `ws-` session.
pub fn current_set(rex: &Rex) -> Option<String> {
    rex.current_label()?.strip_prefix("ws-").map(String::from)
}

pub fn require_current_set(rex: &Rex) -> Result<String> {
    current_set(rex).ok_or_else(|| anyhow!("no set given and not inside a ws- session"))
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

    /// A set `demo` with one project `api`, a workspace of a fresh repo whose trunk is `main@origin`.
    fn synced_set(tmp: &Path, env: &Env) -> PathBuf {
        let run = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("jj").args(args).current_dir(dir).env("JJ_CONFIG", "/dev/null")
                .env("JJ_USER", "t").env("JJ_EMAIL", "t@t").output().unwrap();
            assert!(out.status.success(), "jj {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
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
        // Trunk moves on in the source.
        run(&src, &["new", "main", "-m", "upstream"]);
        run(&src, &["bookmark", "set", "main", "-r", "@"]);
        run(&src, &["git", "push", "-b", "main"]);
        src
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
}
