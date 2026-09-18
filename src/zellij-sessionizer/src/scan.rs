use std::collections::{BTreeSet, VecDeque};
use std::path::{Component, Path, PathBuf};

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

fn mounted_path(mount: &Path, host: &Path) -> std::io::Result<PathBuf> {
    let mut pending: VecDeque<_> = host.components().map(|c| c.as_os_str().to_owned()).collect();
    let mut resolved = mount.to_path_buf();
    let mut links = 0;
    while let Some(part) = pending.pop_front() {
        match Path::new(&part).components().next() {
            Some(Component::RootDir) => resolved = mount.to_path_buf(),
            Some(Component::ParentDir) => {
                if resolved != mount {
                    resolved.pop();
                }
            }
            Some(Component::Normal(_)) => {
                resolved.push(&part);
                // Resolve host symlinks ourselves: WASI cannot follow absolute targets.
                if std::fs::symlink_metadata(&resolved)?.file_type().is_symlink() {
                    links += 1;
                    if links > 40 {
                        return Err(std::io::Error::other("too many symbolic links"));
                    }
                    let target = std::fs::read_link(&resolved)?;
                    resolved.pop();
                    for component in target.components().rev() {
                        pending.push_front(component.as_os_str().to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(resolved)
}

fn host_path(mount: &Path, guest: &Path) -> PathBuf {
    Path::new("/").join(guest.strip_prefix(mount).expect("path outside host mount"))
}

fn scan_at(config: &Config, mount: &Path) -> Vec<Candidate> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();

    for dir in &config.dirs {
        let Ok(path) = mounted_path(mount, Path::new(dir)) else { continue };
        if path.is_dir() && seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    for root in &config.roots {
        let Ok(root_path) = mounted_path(mount, Path::new(root)) else { continue };
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

    let mut found: Vec<PathBuf> = match mounted_path(mount, Path::new(layout_dir)).and_then(std::fs::read_dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| mounted_path(mount, &host_path(mount, &entry.path())).ok())
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

#[cfg(all(test, unix))]
mod symlink_tests {
    use super::*;

    #[test]
    fn discovers_layouts_through_absolute_host_symlinks() {
        let mount = std::env::temp_dir().join(format!("sessionizer-links-{}", std::process::id()));
        let target = mount.join("users/dev/dotfiles/layouts");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(mount.join("users/dev/.config")).unwrap();
        std::fs::write(target.join("alfa.kdl"), "layout {}").unwrap();
        std::os::unix::fs::symlink("/users/dev/dotfiles/layouts", mount.join("users/dev/.config/layouts")).unwrap();
        let layouts = scan_layouts_at("/users/dev/.config/layouts", &mount);
        assert_eq!(layouts.len(), 2);
        assert_eq!(layouts[1].name, "alfa");
        assert_eq!(layouts[1].path.as_deref(), Some("/users/dev/dotfiles/layouts/alfa.kdl"));
        std::os::unix::fs::symlink("layouts", mount.join("users/dev/.config/relative")).unwrap();
        assert_eq!(scan_layouts_at("/users/dev/.config/relative", &mount).len(), 2);
        std::os::unix::fs::symlink("loop", mount.join("users/dev/.config/loop")).unwrap();
        assert!(mounted_path(&mount, Path::new("/users/dev/.config/loop")).is_err());
        std::fs::remove_dir_all(mount).unwrap();
    }
}
