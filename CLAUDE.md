# Instructions for Claude

When updating dotfile configs, run `dotctl apply` to realize those changes on disk.

Before completing any change to the Rust crates in `src/` (the zellij plugins, `repo-sync`), always rebuild and relink them: run `task deploy` (builds everything, then links the results into place). Then run `dotctl apply` as well. Otherwise the live plugins and CLIs keep running the old build.
