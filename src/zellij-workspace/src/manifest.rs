use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::repos::workspaces_root;

pub const FILE_NAME: &str = "workspace.yaml";

/// The intended components of a workspace set. Stored as `workspace.yaml` in
/// the set's directory; it is the source of truth for the set's members.
#[derive(Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Manifest {
    pub name: String,
    pub projects: Vec<Component>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Component {
    pub name: String,
    /// Host path of the repository the workspace was added from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
}

impl Manifest {
    fn file(config: &Config, mount: &Path, set: &str) -> Option<PathBuf> {
        workspaces_root(config, mount).map(|root| root.join(set).join(FILE_NAME))
    }

    /// `None` when the set has no readable manifest (e.g. it predates them).
    pub fn load(config: &Config, mount: &Path, set: &str) -> Option<Manifest> {
        let text = std::fs::read_to_string(Self::file(config, mount, set)?).ok()?;
        serde_yaml_ng::from_str(&text).ok()
    }

    pub fn save(&mut self, config: &Config, mount: &Path) -> Result<(), String> {
        self.projects.sort_by(|a, b| a.name.cmp(&b.name));
        let file = Self::file(config, mount, &self.name).ok_or("workspaces_dir is not accessible")?;
        let text = serde_yaml_ng::to_string(self).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(file.parent().expect("file has a parent"))
            .and_then(|_| std::fs::write(&file, text))
            .map_err(|e| format!("cannot write {}: {e}", FILE_NAME))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.projects.iter().any(|c| c.name == name)
    }

    pub fn add(&mut self, component: Component) {
        if !self.contains(&component.name) {
            self.projects.push(component);
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<Component> {
        let i = self.projects.iter().position(|c| c.name == name)?;
        Some(self.projects.remove(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_yaml() {
        let mount = std::env::temp_dir().join(format!("workspace-manifest-{}", std::process::id()));
        std::fs::create_dir_all(mount.join("ws")).unwrap();
        let config = Config { workspaces_dir: "/ws".into(), ..Config::default() };
        let mut manifest = Manifest { name: "set".into(), projects: Vec::new() };
        manifest.add(Component { name: "b".into(), source: Some("/src/b".into()) });
        manifest.add(Component { name: "a".into(), source: None });
        manifest.save(&config, &mount).unwrap();
        let loaded = Manifest::load(&config, &mount, "set").unwrap();
        assert_eq!(loaded, manifest);
        assert_eq!(loaded.projects[0].name, "a");
        std::fs::remove_dir_all(mount).unwrap();
    }
}
