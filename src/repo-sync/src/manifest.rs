use serde::Deserialize;
use std::collections::HashSet;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default = "default_root")]
    root: String,
    #[serde(default)]
    repos: Vec<Repo>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Repo {
    url: String,
    name: Option<String>,
}

fn default_root() -> String {
    "~/projects".into()
}

/// A repo resolved to where it must live and what its `origin` must be.
#[derive(Debug, PartialEq)]
pub struct Target {
    pub name: String,
    pub url: String,
    pub path: PathBuf,
}

pub fn load(file: &Path) -> Result<Vec<Target>, Box<dyn Error>> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let home = std::env::var("HOME").ok();
    resolve(&text, home.as_deref())
}

fn resolve(text: &str, home: Option<&str>) -> Result<Vec<Target>, Box<dyn Error>> {
    let manifest: Manifest = serde_yaml_ng::from_str(text)?;
    let root = expand(&manifest.root, home)?;
    let mut seen = HashSet::new();
    manifest
        .repos
        .into_iter()
        .map(|repo| {
            let name = match repo.name {
                Some(name) => name,
                None => basename(&repo.url).ok_or(format!("cannot derive a name from {}", repo.url))?,
            };
            if !seen.insert(name.clone()) {
                return Err(format!("duplicate repo name: {name}").into());
            }
            Ok(Target { path: root.join(&name), name, url: repo.url })
        })
        .collect()
}

fn expand(path: &str, home: Option<&str>) -> Result<PathBuf, String> {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            let home = home.ok_or("HOME is not set")?;
            Ok(Path::new(home).join(rest.trim_start_matches('/')))
        }
        _ => Ok(path.into()),
    }
}

/// `https://host/owner/repo.git` and `git@host:owner/repo.git` both give `repo`.
fn basename(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_names_from_https_and_scp_urls() {
        assert_eq!(basename("https://github.com/o/repo.git").as_deref(), Some("repo"));
        assert_eq!(basename("git@github.com:o/repo.git").as_deref(), Some("repo"));
        assert_eq!(basename("https://github.com/o/repo/").as_deref(), Some("repo"));
    }

    #[test]
    fn resolves_paths_under_expanded_root() {
        let yaml = "root: ~/p\nrepos:\n  - url: https://h/o/a.git\n  - url: https://h/o/b.git\n    name: bee\n";
        let targets = resolve(yaml, Some("/home/me")).unwrap();
        assert_eq!(targets[0].path, Path::new("/home/me/p/a"));
        assert_eq!(targets[1].path, Path::new("/home/me/p/bee"));
    }

    #[test]
    fn rejects_duplicate_names() {
        let yaml = "repos:\n  - url: https://h/x/a.git\n  - url: https://h/y/a.git\n";
        assert!(resolve(yaml, Some("/h")).is_err());
    }

    #[test]
    fn empty_repo_list_is_valid() {
        assert!(resolve("root: /x\n", None).unwrap().is_empty());
    }
}
