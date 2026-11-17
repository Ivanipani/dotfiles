//! Set lifecycle: shared by the CLI and the TUI. Each operation returns notes to show the user.

use crate::manifest::{session_label, validate_name, Env, Project};
use crate::rex::Rex;
use crate::{jj, repos};
use anyhow::{anyhow, bail, Result};

/// Resolve names to repositories under the roots; unknown names are an error.
pub fn resolve_repos(env: &Env, set: &str, names: &[String]) -> Result<Vec<Project>> {
    let have: Vec<_> = env.load(set)?.projects.into_iter().map(|p| p.name).collect();
    let candidates: Vec<_> = repos::scan(&env.roots).into_iter().filter(|p| !have.contains(&p.name)).collect();
    let missing: Vec<_> = names.iter().filter(|n| !candidates.iter().any(|c| &c.name == *n)).cloned().collect();
    if !missing.is_empty() {
        bail!("unknown: {}", missing.join(", "));
    }
    Ok(candidates.into_iter().filter(|c| names.contains(&c.name)).collect())
}

pub fn add(env: &Env, rex: &Rex, set: &str, picks: &[Project]) -> Result<Vec<String>> {
    validate_name(set)?;
    let mut manifest = env.load(set)?;
    std::fs::create_dir_all(env.set_dir(set))?;
    let session = rex.find(&session_label(set));
    let mut notes = vec![];
    for p in picks {
        let dir = env.set_dir(set).join(&p.name);
        let source = p.source.as_deref().ok_or_else(|| anyhow!("{} has no source", p.name))?;
        jj::workspace_add(source, set, &dir)?;
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
pub fn delete(env: &Env, rex: &Rex, set: &str) -> Result<Vec<String>> {
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
    if let Some(session) = rex.find(&session_label(set)) {
        rex.kill(&session)?;
    }
    notes.push(format!("deleted {set}"));
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
