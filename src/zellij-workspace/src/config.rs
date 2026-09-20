use std::collections::BTreeMap;

use zellij_common::config::{default_excludes, split_list};

/// Plugin configuration from the `plugin` block that loads this plugin.
///
/// Paths are taken as-is (no `~` expansion): the plugin sandbox does not
/// reliably see the host's `$HOME`, so callers must pass absolute paths.
pub struct Config {
    /// Directories searched for jujutsu repositories.
    pub roots: Vec<String>,
    pub excludes: Vec<String>,
    pub max_depth: usize,
    /// Where workspace sets live: `<workspaces_dir>/<set>/<project>`.
    pub workspaces_dir: String,
    /// Layout file whose first `tab` becomes each project's tab. Optional.
    pub layout: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            roots: Vec::new(),
            excludes: default_excludes(),
            max_depth: 3,
            workspaces_dir: String::new(),
            layout: String::new(),
        }
    }
}

impl Config {
    pub fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut config = Config::default();
        if let Some(value) = map.get("roots") {
            config.roots = split_list(value);
        }
        if let Some(value) = map.get("excludes") {
            config.excludes = split_list(value);
        }
        if let Some(depth) = map.get("max_depth").and_then(|v| v.trim().parse().ok()) {
            config.max_depth = depth;
        }
        if let Some(value) = map.get("workspaces_dir") {
            config.workspaces_dir = value.trim().trim_end_matches('/').to_string();
        }
        if let Some(value) = map.get("layout") {
            config.layout = value.trim().to_string();
        }
        config
    }
}
