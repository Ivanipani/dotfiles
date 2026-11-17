//! Discovery of primary jj repositories under the configured roots.

use crate::manifest::Project;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 3;

/// Primary jj repositories (`.jj/repo` is a directory; secondary workspaces have a file).
pub fn scan(roots: &[PathBuf]) -> Vec<Project> {
    let mut found = BTreeSet::new();
    for root in roots {
        walk(root, 0, &mut found);
    }
    name_uniquely(found.into_iter().collect())
}

fn walk(dir: &Path, depth: usize, found: &mut BTreeSet<PathBuf>) {
    if depth > 0 && dir.join(".jj/repo").is_dir() {
        found.insert(dir.to_path_buf());
    }
    if depth == MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&entry.path(), depth + 1, found);
        }
    }
}

/// Basenames, prefixed by the parent directory's name when two repos share one.
pub fn name_uniquely(sources: Vec<PathBuf>) -> Vec<Project> {
    let base = |p: &Path| p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let names: Vec<_> = sources.iter().map(|p| base(p)).collect();
    sources
        .iter()
        .map(|source| {
            let b = base(source);
            let dup = names.iter().filter(|n| **n == b).count() > 1;
            let name = if dup { format!("{}-{b}", source.parent().map(base).unwrap_or_default()) } else { b };
            Project { name, source: Some(source.to_string_lossy().into_owned()) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_uniquely_when_basenames_collide_prefixes_the_parent() {
        // GIVEN
        let sources = vec![PathBuf::from("/a/app"), PathBuf::from("/b/app"), PathBuf::from("/a/lib")];
        // WHEN
        let names: Vec<_> = name_uniquely(sources).into_iter().map(|p| p.name).collect();
        // THEN
        assert_eq!(names, ["a-app", "b-app", "lib"]);
    }

    #[test]
    fn scan_finds_primary_repos_and_skips_secondary_workspaces() {
        // GIVEN a primary repo and a workspace whose .jj/repo is a file
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("primary/.jj/repo")).unwrap();
        std::fs::create_dir_all(tmp.path().join("secondary/.jj")).unwrap();
        std::fs::write(tmp.path().join("secondary/.jj/repo"), "../x").unwrap();
        // WHEN
        let names: Vec<_> = scan(&[tmp.path().to_path_buf()]).into_iter().map(|p| p.name).collect();
        // THEN
        assert_eq!(names, ["primary"]);
    }

    #[test]
    fn scan_skips_hidden_directories() {
        // GIVEN a repo under a dot directory
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".hidden/repo/.jj/repo")).unwrap();
        // WHEN / THEN
        assert!(scan(&[tmp.path().to_path_buf()]).is_empty());
    }
}
