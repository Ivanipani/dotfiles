#![cfg(not(target_arch = "wasm32"))]
#[path = "../src/scripts.rs"]
mod scripts;
mod tests {
    use super::scripts::PREPARE_SCRIPT;
    use std::path::Path;
    use std::process::{Command, Output};

    #[test]
    fn prepares_stale_workspaces_without_changing_source_config() {
        let root = std::env::temp_dir().join(format!("workspace-recovery-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let workspace = root.join("work space 'quoted'");
        let jj = |dir: &Path, args: &[&str]| -> Output {
            Command::new("jj")
                .current_dir(dir)
                .env("JJ_CONFIG", "")
                .env("XDG_CONFIG_HOME", root.join("config"))
                .args([
                    "--config",
                    "user.name=Test",
                    "--config",
                    "user.email=test@example.com",
                ])
                .arg("-R")
                .arg(dir)
                .args(args)
                .output()
                .unwrap()
        };
        let ok = |output: Output| {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        ok(Command::new("jj")
            .env("JJ_CONFIG", "")
            .env("XDG_CONFIG_HOME", root.join("config"))
            .args(["git", "init"])
            .arg(&source)
            .output()
            .unwrap());
        ok(jj(
            &source,
            &[
                "config",
                "set",
                "--repo",
                "snapshot.auto-update-stale",
                "false",
            ],
        ));
        ok(jj(
            &source,
            &[
                "workspace",
                "add",
                "--name",
                "feature",
                workspace.to_str().unwrap(),
            ],
        ));
        std::fs::write(source.join("upstream.txt"), "first\n").unwrap();
        ok(jj(&source, &["rebase", "-r", "feature@", "-d", "@"]));
        std::fs::write(workspace.join("local.txt"), "unsnapshotted work\n").unwrap();
        let stale = jj(&workspace, &["status"]);
        assert!(!stale.status.success());
        assert!(String::from_utf8_lossy(&stale.stderr).contains("stale"));

        for _ in 0..2 {
            ok(Command::new("sh")
                .env("JJ_CONFIG", "")
                .env("XDG_CONFIG_HOME", root.join("config"))
                .args(["-c", PREPARE_SCRIPT, "sh"])
                .args([&root, &workspace])
                .output()
                .unwrap());
        }
        ok(jj(&workspace, &["status"]));
        assert_eq!(
            String::from_utf8_lossy(
                &jj(
                    &workspace,
                    &["file", "show", "-r", "divergent() ~ feature@", "local.txt"]
                )
                .stdout
            ),
            "unsnapshotted work\n"
        );
        assert_eq!(
            String::from_utf8_lossy(
                &jj(&source, &["config", "get", "snapshot.auto-update-stale"]).stdout
            )
            .trim(),
            "false"
        );
        // Recovery remains enabled when another workspace rewrites this one again.
        std::fs::write(source.join("upstream.txt"), "second\n").unwrap();
        ok(jj(&source, &["status"]));
        ok(jj(&workspace, &["status"]));
        assert_eq!(
            std::fs::read_to_string(workspace.join("upstream.txt")).unwrap(),
            "second\n"
        );
        let failed = Command::new("sh")
            .env("JJ_CONFIG", "")
            .env("XDG_CONFIG_HOME", root.join("config"))
            .args(["-c", PREPARE_SCRIPT, "sh"])
            .args([root.clone(), root.join("missing")])
            .output()
            .unwrap();
        assert!(!failed.status.success());
        std::fs::remove_dir_all(root).unwrap();
    }
}
