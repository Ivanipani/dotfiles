# Instructions for Claude

When updating dotfile configs, run `dot apply` to realize those changes on disk.
Attempt this before finishing every configuration task, using the normal permission flow if needed. Resolve routine failures where possible and report any remaining apply failure or required reload/restart in the final response.

Before completing any change to the Rust crates in `src/` (the zellij plugins, `repo-sync`), always rebuild and relink them: run `task deploy` (builds everything, then links the results into place). Then run `dot apply` as well. Otherwise the live plugins and CLIs keep running the old build.
