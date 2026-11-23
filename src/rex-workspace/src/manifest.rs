//! Workspace sets on disk: `<dir>/<set>/workspace.yaml`, shared with zellij-workspace.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Set on an extra workspace of a repository that is already in the set under its own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

impl Project {
    /// The jj workspace name: the set's, plus the feature for an extra workspace.
    pub fn workspace_name(&self, set: &str) -> String {
        match &self.feature {
            Some(feature) => format!("{set}-{feature}"),
            None => set.to_string(),
        }
    }

    /// An extra workspace of this repository, named `<repo>-<feature>`.
    pub fn with_feature(&self, feature: &str) -> Project {
        Project { name: format!("{}-{feature}", self.base_name()), source: self.source.clone(), feature: Some(feature.into()) }
    }

    fn base_name(&self) -> &str {
        match &self.feature {
            Some(f) => self.name.strip_suffix(&format!("-{f}")).unwrap_or(&self.name),
            None => &self.name,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    /// What the set is for, for people and agents: rendered into AGENTS.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// Tickets, PRs and docs about the goal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default)]
    pub projects: Vec<Project>,
}

/// Where sets, repository roots and the Rex binary come from.
#[derive(Debug, Clone)]
pub struct Env {
    pub workspaces_dir: PathBuf,
    pub roots: Vec<PathBuf>,
}

impl Env {
    pub fn from_process() -> Self {
        let home = home();
        let workspaces_dir = std::env::var_os("REX_WORKSPACES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share/workspaces"));
        let roots = match std::env::var("REX_WORKSPACE_ROOTS") {
            Ok(v) => v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect(),
            Err(_) => vec![home.join("github"), home],
        };
        Self { workspaces_dir, roots }
    }

    pub fn set_dir(&self, set: &str) -> PathBuf {
        self.workspaces_dir.join(set)
    }

    /// A set exists once it has a manifest; a bare directory is debris from a failed create.
    pub fn exists(&self, set: &str) -> bool {
        self.set_dir(set).join("workspace.yaml").exists()
    }

    pub fn load(&self, set: &str) -> Result<Manifest> {
        let file = self.set_dir(set).join("workspace.yaml");
        if !file.exists() {
            return Ok(Manifest { name: set.into(), ..Manifest::default() });
        }
        let text = std::fs::read_to_string(&file)?;
        serde_yaml_ng::from_str(&text).with_context(|| format!("parsing {}", file.display()))
    }

    /// Every set with a manifest, sorted by name.
    pub fn sets(&self) -> Vec<Manifest> {
        let Ok(entries) = std::fs::read_dir(&self.workspaces_dir) else { return vec![] };
        let mut sets: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let text = std::fs::read_to_string(e.path().join("workspace.yaml")).ok()?;
                serde_yaml_ng::from_str::<Manifest>(&text).ok()
            })
            .collect();
        sets.sort_by(|a, b| a.name.cmp(&b.name));
        sets
    }

    /// Writes workspace.yaml and AGENTS.md; a real CLAUDE.md is left alone, otherwise it links to AGENTS.md.
    pub fn save(&self, manifest: &Manifest) -> Result<()> {
        let mut manifest = manifest.clone();
        manifest.projects.sort_by(|a, b| a.name.cmp(&b.name));
        let dir = self.set_dir(&manifest.name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("workspace.yaml"), serde_yaml_ng::to_string(&manifest)?)?;
        std::fs::write(dir.join("AGENTS.md"), agents_md(&manifest))?;
        let claude = dir.join("CLAUDE.md");
        let replaceable = match claude.symlink_metadata() {
            Err(_) => true,
            Ok(meta) => meta.file_type().is_symlink(),
        };
        if replaceable {
            let _ = std::fs::remove_file(&claude);
            std::os::unix::fs::symlink("AGENTS.md", &claude)?;
        }
        Ok(())
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// A set name becomes a directory and a session label, so it must be a plain word.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', ':', ' ']) {
        bail!("invalid set name {name:?}: use letters, digits, '-' or '_'");
    }
    Ok(())
}

pub fn session_label(set: &str) -> String {
    format!("ws-{set}")
}

pub fn agents_md(m: &Manifest) -> String {
    let projects: String = m
        .projects
        .iter()
        .map(|p| match &p.source {
            Some(s) => format!("- `{}/` — from `{s}`\n", p.name),
            None => format!("- `{}/`\n", p.name),
        })
        .collect();
    let name = &m.name;
    let goal = goal_md(m);
    format!(
        "# Workspace: {name}

This directory is a special-purpose workspace set named `{name}`, created to develop one feature across several projects. It is generated by the rex-workspace tool; do not edit this file, it is rewritten whenever the set changes.
{goal}
## Layout

Each subdirectory below is a [jujutsu]\\(https://jj-vcs.github.io/jj/) workspace of a separate repository, not a clone. Its source repository shares the same commit graph.

{projects}
## Working here

- Use `jj`, never `git`, for version control. Run it inside the project you are changing.
- Keep changes scoped to the feature this workspace exists for.
- Each workspace has its own working-copy commit \\(`@`); changes are committed to the shared repository as soon as `jj` runs. Describe them with `jj describe`.
- Do not run `jj workspace forget` or delete project directories. Membership is managed through the tool and `workspace.yaml`.
- Do not touch the source repositories directly.
"
    )
}

/// The `## Goal` section, between blank lines; empty without a goal or links.
fn goal_md(m: &Manifest) -> String {
    let goal = m.goal.as_deref().map(str::trim).filter(|g| !g.is_empty());
    if goal.is_none() && m.links.is_empty() {
        return String::new();
    }
    let mut text = String::from("\n## Goal\n\n");
    if let Some(goal) = goal {
        text += &format!("{goal}\n\n");
    }
    if !m.links.is_empty() {
        text += "Links:\n\n";
        text.extend(m.links.iter().map(|l| format!("- <{l}>\n")));
        text += "\n";
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn env(dir: &Path) -> Env {
        Env { workspaces_dir: dir.to_path_buf(), roots: vec![] }
    }

    fn manifest(names: &[&str]) -> Manifest {
        Manifest {
            name: "demo".into(),
            projects: names.iter().map(|n| Project { name: (*n).into(), source: Some(format!("/src/{n}")), feature: None }).collect(),
            ..Manifest::default()
        }
    }

    #[test]
    fn save_when_projects_are_unsorted_writes_them_sorted() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        // WHEN
        env.save(&manifest(&["beta", "alpha"])).unwrap();
        // THEN
        let names: Vec<_> = env.load("demo").unwrap().projects.into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["alpha", "beta"]);
    }

    #[test]
    fn save_links_claude_md_to_agents_md() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        // WHEN
        env(tmp.path()).save(&manifest(&["alpha"])).unwrap();
        // THEN
        let target = std::fs::read_link(tmp.path().join("demo/CLAUDE.md")).unwrap();
        assert_eq!(target, PathBuf::from("AGENTS.md"));
    }

    #[test]
    fn save_keeps_a_real_claude_md() {
        // GIVEN a set with a hand-written CLAUDE.md
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("demo")).unwrap();
        std::fs::write(tmp.path().join("demo/CLAUDE.md"), "mine").unwrap();
        // WHEN
        env(tmp.path()).save(&manifest(&["alpha"])).unwrap();
        // THEN
        assert_eq!(std::fs::read_to_string(tmp.path().join("demo/CLAUDE.md")).unwrap(), "mine");
    }

    #[test]
    fn load_when_set_is_missing_returns_an_empty_manifest() {
        // GIVEN / WHEN
        let tmp = tempfile::tempdir().unwrap();
        let m = env(tmp.path()).load("nope").unwrap();
        // THEN
        assert!(m.projects.is_empty());
    }

    #[test]
    fn sets_lists_only_directories_with_a_manifest_sorted_by_name() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path());
        env.save(&Manifest { name: "zed".into(), ..Manifest::default() }).unwrap();
        env.save(&Manifest { name: "abe".into(), ..Manifest::default() }).unwrap();
        std::fs::create_dir(tmp.path().join("stray")).unwrap();
        // WHEN
        let names: Vec<_> = env.sets().into_iter().map(|m| m.name).collect();
        // THEN
        assert_eq!(names, ["abe", "zed"]);
    }

    #[test]
    fn exists_when_the_directory_has_no_manifest_is_false() {
        // GIVEN a bare directory left by a failed create
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("vms")).unwrap();
        // WHEN / THEN
        assert!(!env(tmp.path()).exists("vms"));
    }

    #[test]
    fn with_feature_names_the_project_after_the_repo_and_feature() {
        let base = Project { name: "api".into(), source: Some("/src/api".into()), feature: None };
        assert_eq!(base.with_feature("v2").name, "api-v2");
    }

    #[test]
    fn with_feature_on_a_feature_project_replaces_the_feature() {
        let base = Project { name: "api".into(), source: None, feature: None };
        assert_eq!(base.with_feature("v2").with_feature("v3").name, "api-v3");
    }

    #[test]
    fn workspace_name_for_a_feature_is_suffixed_with_it() {
        let p = Project { name: "api-v2".into(), source: None, feature: Some("v2".into()) };
        assert_eq!(p.workspace_name("demo"), "demo-v2");
    }

    #[test]
    fn workspace_name_without_a_feature_is_the_set() {
        let p = Project { name: "api".into(), source: None, feature: None };
        assert_eq!(p.workspace_name("demo"), "demo");
    }

    #[test]
    fn validate_name_rejects_a_path_separator() {
        assert!(validate_name("a/b").is_err());
    }

    #[test]
    fn validate_name_accepts_a_plain_word() {
        assert!(validate_name("feature-x_1").is_ok());
    }

    #[test]
    fn agents_md_lists_each_project_with_its_source() {
        // GIVEN / WHEN
        let text = agents_md(&manifest(&["alpha"]));
        // THEN
        assert!(text.contains("- `alpha/` — from `/src/alpha`"));
    }

    #[test]
    fn agents_md_with_a_goal_states_it_under_a_goal_heading() {
        // GIVEN
        let m = Manifest { goal: Some("Ship sync".into()), ..manifest(&["alpha"]) };
        // WHEN
        let text = agents_md(&m);
        // THEN
        assert!(text.contains("## Goal\n\nShip sync\n"));
    }

    #[test]
    fn agents_md_with_links_lists_each_link() {
        let m = Manifest { links: vec!["https://x/1".into()], ..manifest(&["alpha"]) };
        assert!(agents_md(&m).contains("- <https://x/1>"));
    }

    #[test]
    fn agents_md_without_a_goal_has_no_goal_heading() {
        assert!(!agents_md(&manifest(&["alpha"])).contains("## Goal"));
    }

    #[test]
    fn save_then_load_keeps_the_goal_and_links() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let m = Manifest { goal: Some("Ship sync".into()), links: vec!["https://x/1".into()], ..manifest(&["alpha"]) };
        // WHEN
        env(tmp.path()).save(&m).unwrap();
        // THEN
        assert_eq!(env(tmp.path()).load("demo").unwrap(), m);
    }
}
