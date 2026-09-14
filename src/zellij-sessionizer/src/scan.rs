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
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();

    for dir in &config.dirs {
        let path = PathBuf::from(dir);
        if path.is_dir() && seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    for root in &config.roots {
        let root_path = PathBuf::from(root);
        if !root_path.is_dir() {
            continue;
        }
        walk(&root_path, config.max_depth, &config.excludes, 1, &mut paths, &mut seen);
    }

    paths
        .into_iter()
        .map(|path| Candidate {
            session_name: session_name_for(&path),
            display: path.display().to_string(),
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
    let mut options = vec![LayoutOption { name: "No layout".to_string(), path: None }];
    if layout_dir.is_empty() {
        return options;
    }

    let mut found: Vec<PathBuf> = match std::fs::read_dir(layout_dir) {
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
        options.push(LayoutOption { name, path: Some(path.display().to_string()) });
    }
    options
}
