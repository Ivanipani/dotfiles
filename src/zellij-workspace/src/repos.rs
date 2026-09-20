use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zellij_common::host::{host_path, mounted_path};

use crate::config::Config;
use crate::manifest::Manifest;

/// A project that can be a member of a workspace set.
#[derive(Clone)]
pub struct Repo {
    /// Directory (and tab) name inside the workspace set.
    pub name: String,
    /// Host path of the repository to add a workspace of; `None` for a member
    /// whose source repository is no longer found under the roots.
    pub source: Option<PathBuf>,
}

impl Repo {
    pub fn label(&self) -> String {
        match &self.source {
            Some(path) => format!("{}  {}", self.name, path.display()),
            None => format!("{}  (source not found)", self.name),
        }
    }
}

/// Finds jujutsu repositories under `config.roots`. A repository is a
/// directory whose `.jj/repo` is a directory — secondary workspaces have a
/// file there, so they are not offered as sources.
pub fn scan_repos(config: &Config, mount: &Path) -> Vec<Repo> {
    let mut found = BTreeSet::new();
    for root in &config.roots {
        let Ok(root_path) = mounted_path(mount, Path::new(root)) else { continue };
        walk(&root_path, config, 1, &mut found);
    }
    let sources: Vec<PathBuf> = found.into_iter().map(|p| host_path(mount, &p)).collect();

    let base_names: Vec<String> = sources.iter().map(|p| project_name(p)).collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for name in &base_names {
        *counts.entry(name).or_default() += 1;
    }
    sources
        .iter()
        .zip(&base_names)
        .map(|(source, name)| {
            let name = if counts[name.as_str()] > 1 { qualified_name(source) } else { name.clone() };
            Repo { name, source: Some(source.clone()) }
        })
        .collect()
}

fn walk(dir: &Path, config: &Config, depth: usize, found: &mut BTreeSet<PathBuf>) {
    if depth > config.max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_dir() || name.starts_with('.') || config.excludes.iter().any(|e| e == &*name) {
            continue;
        }
        if path.join(".jj").join("repo").is_dir() {
            found.insert(path);
        } else {
            walk(&path, config, depth + 1, found);
        }
    }
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or_default()
}

/// `repo/default` is the main workspace of `repo`, so it is named for `repo`.
fn project_name(source: &Path) -> String {
    let name = file_name(source);
    match source.parent() {
        Some(parent) if name == "default" => file_name(parent).to_string(),
        _ => name.to_string(),
    }
}

/// Disambiguates same-named repositories with their parent directory.
fn qualified_name(source: &Path) -> String {
    let base = project_name(source);
    let container = if file_name(source) == "default" { source.parent().and_then(Path::parent) } else { source.parent() };
    match container.map(file_name) {
        Some(parent) if !parent.is_empty() => format!("{parent}-{base}"),
        _ => base,
    }
}

fn subdirs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn workspaces_root(config: &Config, mount: &Path) -> Option<PathBuf> {
    mounted_path(mount, Path::new(&config.workspaces_dir)).ok()
}

/// Names of the existing workspace sets.
pub fn list_workspaces(config: &Config, mount: &Path) -> Vec<String> {
    workspaces_root(config, mount).map(|root| subdirs(&root)).unwrap_or_default()
}

/// Project names in the workspace set `workspace`: its manifest's, else
/// (for a set without one) the directories found on disk.
pub fn members(config: &Config, mount: &Path, workspace: &str) -> Vec<String> {
    if let Some(manifest) = Manifest::load(config, mount, workspace) {
        return manifest.projects.into_iter().map(|c| c.name).collect();
    }
    workspaces_root(config, mount).map(|root| subdirs(&root.join(workspace))).unwrap_or_default()
}

/// Host path of a workspace set or, with `project`, of one workspace in it.
pub fn workspace_path(config: &Config, workspace: &str, project: Option<&str>) -> PathBuf {
    let base = Path::new(&config.workspaces_dir).join(workspace);
    match project {
        Some(project) => base.join(project),
        None => base,
    }
}

/// Turns typed input into a usable set and session name: zellij session names
/// cannot contain dots, and the name is also a directory.
pub fn sanitize_name(input: &str) -> String {
    let cleaned: String = input
        .trim()
        .chars()
        .map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => c,
            _ => '-',
        })
        .collect();
    cleaned.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_primary_repositories_and_names_them() {
        let mount = std::env::temp_dir().join(format!("workspace-scan-{}", std::process::id()));
        for dir in [
            "src/one/.jj/repo",
            "src/multi/default/.jj/repo",
            "src/multi/topic/.jj",
            "src/a/dup/.jj/repo",
            "src/b/dup/.jj/repo",
            "src/plain/notes",
            "src/node_modules/x/.jj/repo",
            "ws/set/one",
            "ws/set/multi",
        ] {
            std::fs::create_dir_all(mount.join(dir)).unwrap();
        }
        let config = Config {
            roots: vec!["/src".into()],
            workspaces_dir: "/ws".into(),
            ..Config::default()
        };
        let names: Vec<_> = scan_repos(&config, &mount).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["a-dup", "b-dup", "multi", "one"]);
        assert_eq!(list_workspaces(&config, &mount), ["set"]);
        assert_eq!(members(&config, &mount, "set"), ["multi", "one"]);
        std::fs::remove_dir_all(mount).unwrap();
    }

    #[test]
    fn sanitizes_workspace_names() {
        assert_eq!(sanitize_name(" my.work space/1 "), "my-work-space-1");
        assert_eq!(sanitize_name("../"), "");
    }
}
