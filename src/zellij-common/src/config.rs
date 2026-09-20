/// Splits a colon-separated plugin config value (`roots "a:b"`) into entries.
pub fn split_list(value: &str) -> Vec<String> {
    value
        .split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Directory names never worth descending into when looking for projects.
pub fn default_excludes() -> Vec<String> {
    [
        "node_modules",
        ".git",
        "target",
        "build",
        "dist",
        ".cache",
        ".next",
        ".venv",
        "venv",
        "__pycache__",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
