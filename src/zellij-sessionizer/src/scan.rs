use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Clone)]
pub struct Candidate {
    pub display: String,
    pub session_name: String,
}

/// Walk `config.roots` (plus the standalone `config.dirs`) looking for
/// project directories, mirroring `select_project_directory` /
/// `PROJECT_ROOTS` / `PROJECT_DIRS` from sessionizer-david.zsh.
pub fn scan(config: &Config) -> Vec<Candidate> {
    scan_at(config, Path::new("/host"))
}

fn mounted_path(mount: &Path, host: &Path) -> PathBuf {
    mount.join(host.strip_prefix("/").unwrap_or(host))
}

fn host_path(mount: &Path, guest: &Path) -> PathBuf {
    Path::new("/").join(guest.strip_prefix(mount).expect("path outside host mount"))
}

fn scan_at(config: &Config, mount: &Path) -> Vec<Candidate> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();

    for dir in &config.dirs {
        let path = mounted_path(mount, Path::new(dir));
        if path.is_dir() && seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    for root in &config.roots {
        let root_path = mounted_path(mount, Path::new(root));
        if !root_path.is_dir() {
            continue;
        }
        walk(&root_path, config.max_depth, &config.excludes, 1, &mut paths, &mut seen);
    }

    paths
        .into_iter()
        .map(|path| Candidate {
            session_name: session_name_for(&path),
            display: host_path(mount, &path).display().to_string(),
        })
        .collect()
}

/// Same traversal `fd --type directory --max-depth N --exclude ...` performs:
/// every directory from depth 1 through `max_depth`, skipping (and not
/// descending into) excluded names.
fn walk(
    dir: &Path,
    max_depth: usize,
    excludes: &[String],
    depth: usize,
    out: &mut Vec<PathBuf>,
    seen: &mut BTreeSet<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if excludes.iter().any(|excluded| excluded == name) {
            continue;
        }
        if seen.insert(path.clone()) {
            out.push(path.clone());
        }
        walk(&path, max_depth, excludes, depth + 1, out, seen);
    }
}

/// Ports `get_session_name_for_directory` from sessionizer-david.zsh:
/// worktrees/workspaces become `parent--child`, everything else is just the
/// basename, and dots become underscores either way (zellij session names
/// can't contain dots).
pub fn session_name_for(dir: &Path) -> String {
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let parent = dir.parent();

    let is_workspace = parent.is_some_and(|p| p.join(".bare").is_dir())
        || (dir_name == "default" && dir.join(".jj").is_dir())
        || parent.is_some_and(|p| p.join("default").join(".jj").is_dir())
        || (dir.join(".jj").is_dir() && !dir.join(".git").is_dir());

    let raw = if is_workspace {
        let parent_name = parent
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        format!("{parent_name}--{dir_name}")
    } else {
        dir_name.to_string()
    };

    raw.replace('.', "_")
}

pub struct LayoutOption {
    pub name: String,
    /// `None` is the "no layout / simple shell" entry, always first.
    pub path: Option<String>,
}

/// Ports `select_zellij_layout`: every `.kdl`/`.yaml`/`.yml` file directly
/// under `layout_dir`, named by its stem, with "No layout" pinned first.
pub fn scan_layouts(layout_dir: &str) -> Vec<LayoutOption> {
    scan_layouts_at(layout_dir, Path::new("/host"))
}

fn scan_layouts_at(layout_dir: &str, mount: &Path) -> Vec<LayoutOption> {
    let mut options = vec![LayoutOption { name: "No layout".to_string(), path: None }];
    if layout_dir.is_empty() {
        return options;
    }

    let mut found: Vec<PathBuf> = match std::fs::read_dir(mounted_path(mount, Path::new(layout_dir))) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("kdl" | "yaml" | "yml")
                )
            })
            .collect(),
        Err(_) => return options,
    };
    found.sort();

    for path in found {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        options.push(LayoutOption { name, path: Some(host_path(mount, &path).display().to_string()) });
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_mounted_projects_and_layouts_but_returns_host_paths() {
        let mount = std::env::temp_dir().join(format!("sessionizer-scan-{}", std::process::id()));
        for dir in ["users/dev/projects/repo/default/.jj", "users/dev/projects/repo/topic",
                    "users/dev/projects/node_modules/ignored", "users/dev/dotfiles", "users/dev/layouts"] {
            std::fs::create_dir_all(mount.join(dir)).unwrap();
        }
        std::fs::write(mount.join("users/dev/layouts/work.kdl"), "layout {}").unwrap();
        let config = Config {
            roots: vec!["/users/dev/projects".into()],
            dirs: vec!["/users/dev/dotfiles".into()],
            ..Config::default()
        };
        let candidates = scan_at(&config, &mount);
        let mut actual: Vec<_> = candidates.iter().map(|c| (c.display.as_str(), c.session_name.as_str())).collect();
        actual.sort();
        assert_eq!(actual, vec![
            ("/users/dev/dotfiles", "dotfiles"),
            ("/users/dev/projects/repo", "repo"),
            ("/users/dev/projects/repo/default", "repo--default"),
            ("/users/dev/projects/repo/topic", "repo--topic"),
        ]);
        let layouts = scan_layouts_at("/users/dev/layouts", &mount);
        assert_eq!(layouts.len(), 2);
        assert!(layouts[0].path.is_none());
        assert_eq!(layouts[1].name, "work");
        assert_eq!(layouts[1].path.as_deref(), Some("/users/dev/layouts/work.kdl"));
        std::fs::remove_dir_all(mount).unwrap();
    }
}
