//! The Rex CLI: sessions, windows (tabs) and layers.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

const FALLBACK: &str = "/Applications/Rex Beta.app/Contents/Helpers/Rex Server.rex/Contents/MacOS/rex";
/// The one session that hosts the manager TUIs (workspaces, processes) for the whole Rex app.
pub const HUB_LABEL: &str = "rex-manager";
/// The init.lua action that jumps to the hub, or back from it.
pub const TOGGLE_ACTION: &str = "workspace_toggle_plain";
const SHELL_FLAVOR: &str = "com.superlogical.terminal.shell";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub label: String,
}

/// A tab and its tiled layout, in the shape `session.create` and `session.new_window` take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    #[serde(rename = "window_label")]
    pub label: String,
    pub layout: Value,
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

    /// `(session label, pid)` of the program in every terminal pane; `only` limits to one session.
    pub fn pane_pids(&self, only: Option<&str>) -> Vec<(String, u32)> {
        let mut out = vec![];
        for s in self.sessions().into_iter().filter(|s| only.is_none_or(|l| s.label == l)) {
            let Ok(blocks) = self.json(&["block", "ls", "-s", &s.id, "--json"]) else { continue };
            for id in blocks["blocks"].as_array().into_iter().flatten().filter_map(|b| b["block_id"].as_str()) {
                let process = self.json(&["block", "call", "-s", &s.id, "-b", id, "process", "{}"]);
                if let Some(pid) = process.ok().and_then(|p| p["child"]["pid"].as_u64()) {
                    out.push((s.label.clone(), pid as u32));
                }
            }
        }
        out
    }

    pub fn call(&self, session: &str, method: &str, args: Value) -> Result<Value> {
        self.json(&["api", "call", "-s", session, method, &args.to_string()])
    }

    /// Number of tabs in a session.
    pub fn tab_count(&self, session: &str) -> Option<usize> {
        let view = self.call(session, "session.view", json!({})).ok()?;
        Some(view.get("windows")?.as_array()?.len())
    }

    /// Create a session with all its tabs at once, then make tab `active` the active one.
    pub fn create(&self, label: &str, windows: &[Window], active: usize) -> Result<String> {
        let args = json!({"label": label, "initial_windows": windows});
        let v = self.json(&["api", "call", "session.create", &args.to_string()])?;
        let id = v["session_id"].as_str().map(String::from).ok_or_else(|| anyhow!("session.create returned no session_id"))?;
        if let Some(window) = v["initial_windows"][active]["window_id"].as_str() {
            let _ = self.call(&id, "session.focus_window", json!({"window_id": window}));
        }
        Ok(id)
    }

    /// The session's tabs as they are now, with the index of the active one; floating layers are left out.
    /// A pane comes back in its last directory, re-running the program it was started with (else a shell).
    pub fn capture(&self, session: &str, fallback_cwd: &str) -> Result<(Vec<Window>, usize)> {
        let view = self.call(session, "session.view", json!({}))?;
        Ok(windows_from_view(&view, |block| {
            let id = block["block_id"].as_str().unwrap_or_default();
            let process = self.json(&["block", "call", "-s", session, "-b", id, "process", "{}"]).unwrap_or(Value::Null);
            restored_block(block["label"].as_str().unwrap_or("shell"), &process, fallback_cwd, crate::procs::args_of)
        }))
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

    pub fn add_window(&self, session: &str, window: &Window) -> Result<()> {
        self.call(session, "session.new_window", json!({"window_label": window.label, "layout": window.layout, "focus": false}))?;
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
    block("nvim", dir, Some(in_shell("exec nvim .")))
}

/// A tab holding a single nvim pane; jjui and agents live in floats opened on demand.
pub fn project_window(name: &str, dir: &str) -> Window {
    Window { label: name.into(), layout: project_layout(dir) }
}

/// A tab holding a plain shell.
pub fn shell_window(label: &str, dir: &str) -> Window {
    Window { label: label.into(), layout: block("shell", dir, None) }
}

/// Convert each window's tiled layout from `session.view` into a creatable layout, panes built by `pane`.
fn windows_from_view(view: &Value, pane: impl Fn(&Value) -> Value) -> (Vec<Window>, usize) {
    let mut windows = vec![];
    let mut active = 0;
    for w in view["windows"].as_array().into_iter().flatten() {
        let layers = w["layers"].as_array().into_iter().flatten();
        let Some(tiled) = layers.clone().find(|l| l["kind"] == "tiled") else { continue };
        let blocks: Vec<&Value> = layers.flat_map(|l| l["blocks"].as_array().into_iter().flatten()).collect();
        if w["window_id"] == view["active_window_id"] {
            active = windows.len();
        }
        let label = w["label"].as_str().unwrap_or_default().to_string();
        windows.push(Window { label, layout: layout_node(&tiled["layout"], &blocks, &pane) });
    }
    (windows, active)
}

fn layout_node(node: &Value, blocks: &[&Value], pane: &impl Fn(&Value) -> Value) -> Value {
    if let Some(split) = node.get("split") {
        let (before, after) = (layout_node(&split["before"], blocks, pane), layout_node(&split["after"], blocks, pane));
        return json!({"split": {"direction": split["direction"], "ratio": split["ratio"], "before": before, "after": after}});
    }
    let found = blocks.iter().find(|b| b["block_id"] == node["block_id"]);
    pane(found.copied().unwrap_or(&Value::Null))
}

/// A pane started with a command (its child is not a login shell) re-runs that command; any other gets a shell.
fn restored_block(label: &str, process: &Value, fallback_cwd: &str, args_of: impl Fn(u32) -> Option<String>) -> Value {
    let child = &process["child"];
    let login = child["name"] == "login" || child["argv0"].as_str().is_some_and(|a| a.starts_with('-'));
    let launched = child["pid"].as_u64().filter(|_| !login).and_then(|pid| args_of(pid as u32));
    let at = if launched.is_some() { child } else { &process["foreground"] };
    let cwd = at["cwd"].as_str().unwrap_or(fallback_cwd);
    block(label, cwd, launched.map(|args| in_shell(&format!("exec {args}"))))
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
    fn project_layout_is_a_single_nvim_pane() {
        // GIVEN / WHEN
        let l = project_layout("/d");
        // THEN
        assert_eq!(l["block"]["label"], "nvim");
        assert!(l.get("split").is_none());
    }

    fn shell_pane(cwd: &str) -> Value {
        json!({"child": {"pid": 1, "name": "login"}, "foreground": {"pid": 2, "name": "zsh", "argv0": "-zsh", "cwd": cwd}})
    }

    #[test]
    fn restored_block_for_a_login_shell_is_a_shell_in_its_last_directory() {
        // GIVEN a plain shell pane that moved to /work
        let b = restored_block("shell", &shell_pane("/work"), "/set", |_| Some("ignored".into()));
        // THEN
        assert_eq!(b, block("shell", "/work", None));
    }

    #[test]
    fn restored_block_for_a_launched_program_re_runs_its_command_line() {
        // GIVEN a pane started straight into nvim
        let process = json!({"child": {"pid": 7, "name": "nvim", "argv0": "nvim", "cwd": "/p"}});
        // WHEN
        let b = restored_block("nvim", &process, "/set", |pid| (pid == 7).then(|| "nvim .".into()));
        // THEN
        assert_eq!(b, block("nvim", "/p", Some(in_shell("exec nvim ."))));
    }

    #[test]
    fn restored_block_without_process_details_falls_back_to_the_set_directory() {
        assert_eq!(restored_block("shell", &Value::Null, "/set", |_| None), block("shell", "/set", None));
    }

    fn view() -> Value {
        json!({"active_window_id": "w2", "windows": [
            {"window_id": "w1", "label": "root", "layers": [
                {"kind": "tiled", "layout": {"block_id": "b1"}, "blocks": [{"block_id": "b1", "label": "shell"}]},
                {"kind": "floating", "layout": {"block_id": "f1"}, "blocks": [{"block_id": "f1", "label": "scratch"}]}]},
            {"window_id": "w2", "label": "api", "layers": [
                {"kind": "tiled", "layout": {"split": {"direction": "horizontal", "ratio": 0.3,
                    "before": {"block_id": "b2"}, "after": {"block_id": "b3"}}},
                 "blocks": [{"block_id": "b2", "label": "nvim"}, {"block_id": "b3", "label": "logs"}]}]}]})
    }

    fn labels(b: &Value) -> Value {
        b["label"].clone()
    }

    #[test]
    fn windows_from_view_keeps_tab_labels_in_order() {
        let (windows, _) = windows_from_view(&view(), labels);
        assert_eq!(windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["root", "api"]);
    }

    #[test]
    fn windows_from_view_reports_the_active_tab() {
        assert_eq!(windows_from_view(&view(), labels).1, 1);
    }

    #[test]
    fn windows_from_view_leaves_out_floating_layers() {
        assert_eq!(windows_from_view(&view(), labels).0[0].layout, json!("shell"));
    }

    #[test]
    fn windows_from_view_keeps_split_direction_ratio_and_both_sides() {
        let (windows, _) = windows_from_view(&view(), labels);
        assert_eq!(
            windows[1].layout,
            json!({"split": {"direction": "horizontal", "ratio": 0.3, "before": "nvim", "after": "logs"}})
        );
    }

    #[test]
    fn window_serializes_with_the_api_field_names() {
        let w = serde_json::to_value(shell_window("root", "/d")).unwrap();
        assert!(w.get("window_label").is_some() && w.get("layout").is_some());
    }

    #[test]
    fn block_without_a_command_omits_it() {
        assert!(block("x", "/d", None)["block"]["options"].get("command").is_none());
    }
}
