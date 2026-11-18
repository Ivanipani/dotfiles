//! Set lifecycle: shared by the CLI and the TUI. Each operation returns notes to show the user.

use crate::manifest::{session_label, validate_name, Env, Project};
use crate::rex::Rex;
use crate::{jj, repos};
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
            rex.add_window(session, &p.name, &dir.to_string_lossy())?;
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

/// Create the set's session (one tab per project) unless open, then switch to it.
pub fn open(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
    let manifest = env.load(set)?;
    if manifest.projects.is_empty() {
        bail!("no set named {set}");
    }
    let id = match rex.find(&session_label(set)) {
        Some(id) => id,
        None => {
            let id = rex.create(&session_label(set), &env.set_dir(set).to_string_lossy(), set)?;
            for p in &manifest.projects {
                rex.add_window(&id, &p.name, &env.set_dir(set).join(&p.name).to_string_lossy())?;
            }
            id
        }
    };
    // No app attached (headless, or several): the session is still created.
    Ok(match rex.select(&id) {
        Ok(()) => vec![format!("opened {set}")],
        Err(e) => vec![format!("opened {id}; could not switch: {e}")],
    })
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
        env.save(&crate::manifest::Manifest { name: "demo".into(), projects: api }).unwrap();
        // WHEN / THEN
        assert!(resolve_repos(&env, "demo", &["api".to_string()]).is_err());
    }

    #[test]
    fn resolve_repos_for_a_new_version_of_a_repo_already_in_the_set_succeeds() {
        // GIVEN api already in the set
        let (_tmp, env) = env_with_repos(&["api"]);
        let api = resolve_repos(&env, "demo", &["api".to_string()]).unwrap();
        env.save(&crate::manifest::Manifest { name: "demo".into(), projects: api }).unwrap();
        // WHEN / THEN
        assert_eq!(resolved(&env, &["api:v2"]), ["api-v2"]);
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
