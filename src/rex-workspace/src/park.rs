//! Parked sets. A closed set has no session but keeps its layout in `.session.json`, to come back as it was;
//! a paused set keeps its session with every process stopped, marked by `.paused`.

use crate::manifest::{Env, Manifest};
use crate::rex::{project_window, shell_window, Window};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SNAPSHOT: &str = ".session.json";
const PAUSED: &str = ".paused";

/// The tabs of a closed session, and which projects had one, so changes made while closed can be applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<String>,
    pub active: usize,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Open,
    Paused,
    /// No session, layout saved.
    Closed,
    /// No session and nothing saved.
    Idle,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Open => "open",
            State::Paused => "paused",
            State::Closed => "closed",
            State::Idle => "",
        }
    }
}

fn file(env: &Env, set: &str, name: &str) -> PathBuf {
    env.set_dir(set).join(name)
}

pub fn state(env: &Env, set: &str, has_session: bool) -> State {
    match (has_session, is_paused(env, set), file(env, set, SNAPSHOT).exists()) {
        (true, true, _) => State::Paused,
        (true, false, _) => State::Open,
        (false, _, true) => State::Closed,
        (false, _, false) => State::Idle,
    }
}

pub fn load(env: &Env, set: &str) -> Option<Snapshot> {
    serde_json::from_str(&std::fs::read_to_string(file(env, set, SNAPSHOT)).ok()?).ok()
}

pub fn save(env: &Env, set: &str, snapshot: &Snapshot) -> Result<()> {
    Ok(std::fs::write(file(env, set, SNAPSHOT), serde_json::to_string_pretty(snapshot)?)?)
}

pub fn forget(env: &Env, set: &str) {
    let _ = std::fs::remove_file(file(env, set, SNAPSHOT));
}

pub fn is_paused(env: &Env, set: &str) -> bool {
    file(env, set, PAUSED).exists()
}

pub fn mark_paused(env: &Env, set: &str, paused: bool) -> Result<()> {
    let path = file(env, set, PAUSED);
    match paused {
        true => std::fs::write(path, "")?,
        false => {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

/// A new session: a shell tab at the set root, then one tab per project.
pub fn fresh(manifest: &Manifest, env: &Env) -> Vec<Window> {
    let dir = env.set_dir(&manifest.name);
    let root = shell_window(&manifest.name, &dir.to_string_lossy());
    std::iter::once(root).chain(manifest.projects.iter().map(|p| project_window(&p.name, &dir.join(&p.name).to_string_lossy()))).collect()
}

/// The saved tabs, minus those of projects removed since, plus tabs for projects added since; and the active tab.
pub fn restore(snapshot: Snapshot, manifest: &Manifest, env: &Env) -> (Vec<Window>, usize) {
    if snapshot.windows.is_empty() {
        return (fresh(manifest, env), 0);
    }
    let names: Vec<_> = manifest.projects.iter().map(|p| p.name.as_str()).collect();
    let removed = |w: &Window| snapshot.projects.contains(&w.label) && !names.contains(&w.label.as_str());
    let active = snapshot.windows.get(snapshot.active).map(|w| w.label.clone());
    let added = fresh(manifest, env).into_iter().skip(1).filter(|w| !snapshot.projects.contains(&w.label));
    let windows: Vec<_> = snapshot.windows.iter().filter(|w| !removed(w)).cloned().chain(added).collect();
    let active = active.and_then(|l| windows.iter().position(|w| w.label == l)).unwrap_or(0);
    (windows, active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Project;

    fn env(dir: &std::path::Path) -> Env {
        Env { workspaces_dir: dir.to_path_buf(), roots: vec![] }
    }

    fn manifest(names: &[&str]) -> Manifest {
        Manifest {
            name: "demo".into(),
            projects: names.iter().map(|n| Project { name: (*n).into(), source: None, feature: None }).collect(),
            ..Manifest::default()
        }
    }

    fn snapshot(projects: &[&str], windows: &[&str], active: usize) -> Snapshot {
        Snapshot {
            projects: projects.iter().map(|p| p.to_string()).collect(),
            active,
            windows: windows.iter().map(|w| shell_window(w, "/x")).collect(),
        }
    }

    fn labels(windows: &[Window]) -> Vec<&str> {
        windows.iter().map(|w| w.label.as_str()).collect()
    }

    #[test]
    fn fresh_opens_a_root_tab_then_one_per_project() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(labels(&fresh(&manifest(&["api", "web"]), &env(tmp.path()))), ["demo", "api", "web"]);
    }

    #[test]
    fn restore_keeps_saved_tabs_in_their_order() {
        // GIVEN a saved session with a tab the user made between projects
        let tmp = tempfile::tempdir().unwrap();
        let snap = snapshot(&["api", "web"], &["demo", "api", "notes", "web"], 0);
        // WHEN
        let (windows, _) = restore(snap, &manifest(&["api", "web"]), &env(tmp.path()));
        // THEN
        assert_eq!(labels(&windows), ["demo", "api", "notes", "web"]);
    }

    #[test]
    fn restore_drops_the_tab_of_a_project_removed_while_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let (windows, _) = restore(snapshot(&["api", "web"], &["demo", "api", "web"], 0), &manifest(&["api"]), &env(tmp.path()));
        assert_eq!(labels(&windows), ["demo", "api"]);
    }

    #[test]
    fn restore_appends_a_tab_for_a_project_added_while_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let (windows, _) = restore(snapshot(&["api"], &["demo", "api"], 0), &manifest(&["api", "web"]), &env(tmp.path()));
        assert_eq!(labels(&windows), ["demo", "api", "web"]);
    }

    #[test]
    fn restore_keeps_the_active_tab_when_earlier_tabs_are_dropped() {
        // GIVEN "web" active, and "api" before it removed
        let tmp = tempfile::tempdir().unwrap();
        let snap = snapshot(&["api", "web"], &["demo", "api", "web"], 2);
        // WHEN
        let (windows, active) = restore(snap, &manifest(&["web"]), &env(tmp.path()));
        // THEN
        assert_eq!(windows[active].label, "web");
    }

    #[test]
    fn restore_when_the_active_tab_is_dropped_activates_the_first() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, active) = restore(snapshot(&["api"], &["demo", "api"], 1), &manifest(&[]), &env(tmp.path()));
        assert_eq!(active, 0);
    }

    #[test]
    fn restore_of_an_empty_snapshot_opens_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let (windows, _) = restore(snapshot(&[], &[], 0), &manifest(&["api"]), &env(tmp.path()));
        assert_eq!(labels(&windows), ["demo", "api"]);
    }

    #[test]
    fn state_with_a_saved_layout_and_no_session_is_closed() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        std::fs::create_dir_all(env.set_dir("demo")).unwrap();
        save(&env, "demo", &snapshot(&[], &["demo"], 0)).unwrap();
        // WHEN / THEN
        assert_eq!(state(&env, "demo", false), State::Closed);
    }

    #[test]
    fn state_with_a_session_marked_paused_is_paused() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        std::fs::create_dir_all(env.set_dir("demo")).unwrap();
        mark_paused(&env, "demo", true).unwrap();
        assert_eq!(state(&env, "demo", true), State::Paused);
    }

    #[test]
    fn state_with_a_stale_paused_marker_and_no_session_is_not_paused() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        std::fs::create_dir_all(env.set_dir("demo")).unwrap();
        mark_paused(&env, "demo", true).unwrap();
        assert_eq!(state(&env, "demo", false), State::Idle);
    }

    #[test]
    fn save_then_load_returns_the_same_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        std::fs::create_dir_all(env.set_dir("demo")).unwrap();
        let snap = snapshot(&["api"], &["demo", "api"], 1);
        save(&env, "demo", &snap).unwrap();
        assert_eq!(load(&env, "demo"), Some(snap));
    }
}
