/// Point the set's `CLAUDE.md` at `AGENTS.md`; a real (non-symlink) `CLAUDE.md` is left alone.
macro_rules! link_claude {
    () => {
        r#"[ -e "$1/CLAUDE.md" ] && [ ! -L "$1/CLAUDE.md" ] || ln -sfn AGENTS.md "$1/CLAUDE.md"
"#
    };
}

/// `$1` is the workspace set directory, the rest are its workspaces. Links `CLAUDE.md`, then
/// enables recovery on every future jj snapshot, scoped to these workspaces.
pub const PREPARE_SCRIPT: &str = concat!(link_claude!(), r#"shift
for d; do
  jj -R "$d" config set --workspace snapshot.auto-update-stale true || exit 1
done
"#);

#[cfg(test)]
const LINK_CLAUDE_SCRIPT: &str = link_claude!();

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn set_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("workspace-claude-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn link(dir: &Path) {
        assert!(Command::new("sh").args(["-c", LINK_CLAUDE_SCRIPT, "sh"]).arg(dir).status().unwrap().success());
    }

    #[test]
    fn missing_claude_md_becomes_a_symlink_to_agents_md() {
        let dir = set_dir("missing");
        link(&dir);
        assert_eq!(std::fs::read_link(dir.join("CLAUDE.md")).unwrap(), Path::new("AGENTS.md"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn linking_twice_keeps_claude_md_a_symlink_to_agents_md() {
        let dir = set_dir("twice");
        link(&dir);
        link(&dir);
        assert_eq!(std::fs::read_link(dir.join("CLAUDE.md")).unwrap(), Path::new("AGENTS.md"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regular_claude_md_is_left_untouched() {
        let dir = set_dir("regular");
        std::fs::write(dir.join("CLAUDE.md"), "mine").unwrap();
        link(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap(), "mine");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
