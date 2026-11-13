#![cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
#[path = "../src/layout.rs"]
mod layout;
#[path = "../src/session.rs"]
mod session;

use std::process::{Command, Output, Stdio};

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("ZELLIJ_SOCKET_DIR", format!("/tmp/ws-sync-sockets-{}", std::process::id()))
        .env("TMPDIR", "/tmp").env_remove("ZELLIJ").env_remove("ZELLIJ_SESSION_NAME");
    command
}

struct Client(std::process::Child);

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn attach(name: &str) -> Client {
    let client = command("script").args(["-q", "/dev/null", "zellij", "attach", name])
        .env("TERM", "xterm-256color").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    Client(client)
}

fn ok(output: Output) -> String {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "starts an isolated Zellij session; needs host socket access"]
fn reconciles_running_and_resurrected_sessions() {
    let name = format!("workspace-sync-test-{}", std::process::id());
    let root = std::env::temp_dir().join(&name);
    std::fs::create_dir_all(root.join("old")).unwrap();
    std::fs::create_dir_all(root.join("kept")).unwrap();
    let root = root.canonicalize().unwrap();
    let config = root.join("config.kdl");
    std::fs::write(&config, "session_serialization true\n").unwrap();
    struct Cleanup(String, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = command("zellij").args(["delete-session", "--force", &self.0]).output();
            let _ = std::fs::remove_dir_all(&self.1);
            let _ = std::fs::remove_dir_all(format!("/tmp/ws-sync-sockets-{}", std::process::id()));
        }
    }
    let _cleanup = Cleanup(name.clone(), root.clone());
    let old = format!("layout {{ tab name=\"old\" cwd=\"{}\" {{ pane command=\"sleep\" {{ args \"300\"; }}; }}; }}", root.join("old").display());
    let desired = format!("layout {{ tab name=\"kept\" cwd=\"{}\" {{ pane command=\"sleep\" {{ args \"300\"; }}; }}; }}", root.join("kept").display());
    let ensure = |layout: &str| {
        ok(command("sh").env("ZELLIJ_CONFIG_FILE", &config)
            .args(["-c", session::ENSURE, "sh", &name, layout]).output().unwrap());
    };
    let inspect = || {
        let mut last = String::new();
        for _ in 0..30 {
            let snapshot = ok(command("sh").args(["-c", session::INSPECT, "sh", &name]).output().unwrap());
            if session::plan(&snapshot, &desired, &root).unwrap().ready {
                return snapshot;
            }
            last = snapshot;
        }
        panic!("session did not become ready: {last}");
    };
    let sync = |desired: &str| {
        let snapshot = inspect();
        let plan = session::plan(&snapshot, desired, &root).unwrap_or_else(|err| panic!("{err}: {snapshot:?}"));
        assert_eq!(plan.close.len(), 1, "root={root:?}, snapshot={snapshot}");
        assert!(!plan.layout.is_empty());
        for _ in 0..3 {
            let plan = session::plan(&inspect(), desired, &root).unwrap();
            if plan.layout.is_empty() && plan.close.is_empty() { return; }
            let mut cmd = command("sh");
            cmd.args(["-c", session::SYNC, "sh", &name, &plan.layout]);
            cmd.args(plan.close.iter().map(u64::to_string));
            ok(cmd.output().unwrap());
        }
        panic!("session did not converge");
    };
    ensure(&old);
    let client = attach(&name);
    ensure(&desired); // Existing sessions must not gain duplicate tabs here.
    sync(&desired);
    ok(command("zellij").args(["--session", &name, "action", "save-session"]).output().unwrap());
    ok(command("zellij").args(["kill-session", &name]).output().unwrap());
    drop(client);
    ensure(&old);
    let _client = attach(&name);
    // Resurrection restores 'kept'; reconcile back to 'old'.
    sync(&old);
}
