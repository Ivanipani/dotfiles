//! The Rex CLI: sessions, windows (tabs) and layers.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

const FALLBACK: &str = "/Applications/Rex Beta.app/Contents/Helpers/Rex Server.rex/Contents/MacOS/rex";
/// The one session that hosts the manager TUI for the whole Rex app.
pub const HUB_LABEL: &str = "workspaces";
/// The init.lua action that jumps to the hub, or back from it.
pub const TOGGLE_ACTION: &str = "workspace_toggle_plain";
const SHELL_FLAVOR: &str = "com.superlogical.terminal.shell";
const JJUI_BOUNDS: [f64; 4] = [0.05, 0.05, 0.9, 0.9];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub label: String,
}

#[derive(Clone)]
pub struct Rex {
    bin: Option<PathBuf>,
}

impl Rex {
    pub fn locate() -> Self {
        Self { bin: locate_bin() }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let bin = self.bin.as_ref().ok_or_else(|| anyhow!("rex not found"))?;
        let out = Command::new(bin).args(args).output().with_context(|| format!("running {}", bin.display()))?;
        if !out.status.success() {
            bail!("rex {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn json(&self, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.run(args)?)?)
    }

    /// Live sessions; empty when Rex is absent or not running.
    pub fn sessions(&self) -> Vec<Session> {
        let Ok(v) = self.json(&["ls", "--json"]) else { return vec![] };
        parse_sessions(&v)
    }

    pub fn find(&self, label: &str) -> Option<String> {
        self.sessions().into_iter().find(|s| s.label == label).map(|s| s.id)
    }

    /// The set behind the session this process runs in, when it is a `ws-` session.
    pub fn current_label(&self) -> Option<String> {
        let id = std::env::var("REX_SESSION").ok()?;
        self.sessions().into_iter().find(|s| s.id == id).map(|s| s.label)
    }

    pub fn call(&self, session: &str, method: &str, args: Value) -> Result<Value> {
        self.json(&["api", "call", "-s", session, method, &args.to_string()])
    }

    /// Number of tabs in a session.
    pub fn tab_count(&self, session: &str) -> Option<usize> {
        let view = self.call(session, "session.view", json!({})).ok()?;
        Some(view.get("windows")?.as_array()?.len())
    }

    pub fn create(&self, label: &str, cwd: &str, window: &str) -> Result<String> {
        let v = self.json(&["new", label, "-c", cwd, "--window", window, "--json"])?;
        v["session_id"].as_str().map(String::from).ok_or_else(|| anyhow!("rex new returned no session_id"))
    }

    pub fn kill(&self, session: &str) -> Result<()> {
        self.run(&["kill", session]).map(drop)
    }

    /// Switch the attached app to a session. Errors when no app is attached (or several).
    pub fn select(&self, session: &str) -> Result<()> {
        self.perform(&["session.select", &format!("session_id={session}")])
    }

    /// Run an action on the attached app, e.g. one defined in init.lua.
    pub fn perform(&self, action_and_args: &[&str]) -> Result<()> {
        let client = match std::env::var("REX_CLIENT") {
            Ok(c) if !c.is_empty() => c,
            _ => self.app_client().ok_or_else(|| anyhow!("no connected Rex app"))?,
        };
        let mut args = vec!["-C", &client, "do"];
        args.extend(action_and_args);
        self.run(&args).map(drop)
    }

    /// The live app client; stale ones linger after an app restart and make `rex do` ambiguous.
    fn app_client(&self) -> Option<String> {
        let v = self.json(&["client", "ls", "--json"]).ok()?;
        v["clients"].as_array()?.iter().find_map(|c| {
            let app = c["info"]["kind"] == "app" && c["connection_state"] == "connected";
            app.then(|| c["client_id"].as_str().map(String::from)).flatten()
        })
    }

    /// Tiled nvim | agent split, plus jjui in a minimized floating layer.
    pub fn add_window(&self, session: &str, name: &str, dir: &str) -> Result<()> {
        let win = self.call(
            session,
            "session.new_window",
            json!({"window_label": name, "layout": project_layout(dir), "focus": false}),
        )?;
        let layer = self.call(
            session,
            "session.new_layer",
            json!({"window_id": win["window_id"], "bounds": JJUI_BOUNDS, "layout": block("jjui", dir, Some(in_shell("while jjui; do :; done")))}),
        )?;
        self.call(session, "session.set_layer_state", json!({"layer_id": layer["layer_id"], "state": "minimized"}))?;
        Ok(())
    }

    pub fn close_window(&self, session: &str, name: &str) -> Result<()> {
        let view = self.call(session, "session.view", json!({}))?;
        for w in view["windows"].as_array().into_iter().flatten().filter(|w| w["label"] == name) {
            self.call(session, "session.close_window", json!({"window_id": w["window_id"]}))?;
        }
        Ok(())
    }
}

fn locate_bin() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("REX_BIN").filter(|v| !v.is_empty()) {
        return Some(bin.into());
    }
    if let Some(dir) = std::env::var_os("REX_BIN_DIR").filter(|v| !v.is_empty()) {
        let local = PathBuf::from(dir).join("rex");
        if local.exists() {
            return Some(local);
        }
    }
    let on_path = std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|d| d.join("rex")).find(|c| c.is_file()));
    on_path.or_else(|| PathBuf::from(FALLBACK).exists().then(|| PathBuf::from(FALLBACK)))
}

fn parse_sessions(v: &Value) -> Vec<Session> {
    v["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some(Session { id: s["session_id"].as_str()?.into(), label: s["label"].as_str()?.into() }))
        .collect()
}

/// Server-spawned blocks get a bare PATH; run through the user's login shell to get ~/.local/bin etc.
fn in_shell(line: &str) -> Vec<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "zsh".into());
    vec![shell, "-lic".into(), line.into()]
}

fn block(label: &str, cwd: &str, command: Option<Vec<String>>) -> Value {
    let mut options = json!({"cwd": cwd});
    if let Some(command) = command {
        options["command"] = json!(command);
    }
    json!({"block": {"flavor": SHELL_FLAVOR, "label": label, "options": options}})
}

fn project_layout(dir: &str) -> Value {
    json!({"split": {
        "direction": "horizontal",
        "ratio": 0.6,
        "before": block("nvim", dir, Some(in_shell("exec nvim ."))),
        "after": block("agent", dir, Some(in_shell("exec claude"))),
    }})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sessions_reads_ids_and_labels() {
        // GIVEN
        let v = json!({"sessions": [{"session_id": "session:1", "label": "ws-a"}]});
        // WHEN / THEN
        assert_eq!(parse_sessions(&v), [Session { id: "session:1".into(), label: "ws-a".into() }]);
    }

    #[test]
    fn parse_sessions_when_key_is_missing_is_empty() {
        assert!(parse_sessions(&json!({})).is_empty());
    }

    #[test]
    fn project_layout_splits_nvim_before_the_agent() {
        // GIVEN / WHEN
        let l = project_layout("/d");
        // THEN
        assert_eq!(l["split"]["before"]["block"]["label"], "nvim");
        assert_eq!(l["split"]["after"]["block"]["label"], "agent");
    }

    #[test]
    fn block_without_a_command_omits_it() {
        assert!(block("x", "/d", None)["block"]["options"].get("command").is_none());
    }
}
