use std::collections::BTreeMap;

use zellij_common::config::{default_excludes, split_list};

/// Plugin configuration, supplied by the layout/KDL `plugin` block that loads
/// this plugin (see the `configuration` argument of `ZellijPlugin::load`).
///
/// Paths are taken as-is (no `~` expansion): the plugin sandbox does not
/// reliably see the host's `$HOME`, so callers must pass already-resolved
/// absolute paths.
pub struct Config {
    pub roots: Vec<String>,
    pub dirs: Vec<String>,
    pub excludes: Vec<String>,
    pub max_depth: usize,
    pub layout_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            roots: Vec::new(),
            dirs: Vec::new(),
            excludes: default_excludes(),
            max_depth: 2,
            layout_dir: String::new(),
        }
    }
}

impl Config {
    pub fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut config = Config::default();
        if let Some(value) = map.get("roots") {
            config.roots = split_list(value);
        }
        if let Some(value) = map.get("dirs") {
            config.dirs = split_list(value);
        }
        if let Some(value) = map.get("excludes") {
            config.excludes = split_list(value);
        }
        if let Some(value) = map.get("max_depth") {
            if let Ok(depth) = value.trim().parse() {
                config.max_depth = depth;
            }
        }
        if let Some(value) = map.get("layout_dir") {
            config.layout_dir = value.trim().to_string();
        }
        config
    }
}
