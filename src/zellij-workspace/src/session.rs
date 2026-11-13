use std::collections::BTreeSet;
use std::path::Path;

use kdl::KdlDocument;
use serde::Deserialize;

/// Background creation is not idempotent in Zellij; check for a running session first.
pub const ENSURE: &str = r#"
if zellij --session "$1" action list-tabs --json >/dev/null 2>&1; then
  exit 0
fi
zellij --layout-string "$2" attach --create-background "$1" || exit 1
"#;

/// Read both snapshots from the target session, never the manager's session.
pub const INSPECT: &str = r#"
sleep 0.1
zellij --session "$1" action list-tabs --json || exit 1
printf '\n---WORKSPACE-LAYOUT---\n'
zellij --session "$1" action dump-layout
"#;

/// Add first so replacing every project cannot close the session's last tab.
pub const SYNC: &str = r#"
session=$1; layout=$2; shift 2
if [ -n "$layout" ]; then
  zellij --session "$session" action new-tab --layout-string "$layout" --no-focus || exit 1
  exit 0
fi
for id; do
  zellij --session "$session" action close-tab-by-id "$id" || exit 1
done
"#;

#[derive(Deserialize)]
struct Tab {
    tab_id: u64,
    name: String,
    #[serde(default = "one")]
    selectable_tiled_panes_count: usize,
    #[serde(default)]
    selectable_floating_panes_count: usize,
}

fn one() -> usize { 1 }

pub struct Plan {
    pub ready: bool,
    pub layout: String,
    pub close: Vec<u64>,
}

/// Recognize generated tabs by both name and project cwd. Leave custom tabs alone.
pub fn plan(snapshot: &str, desired: &str, root: &Path) -> Result<Plan, String> {
    let (tabs, layout) = snapshot.split_once("\n---WORKSPACE-LAYOUT---\n").ok_or("missing session layout")?;
    let tabs: Vec<Tab> = serde_yaml_ng::from_str(tabs).map_err(|e| format!("session tabs: {e}"))?;
    let actual: KdlDocument = layout.parse().map_err(|e| format!("session layout: {e}"))?;
    let mut desired: KdlDocument = desired.parse().map_err(|e| format!("workspace layout: {e}"))?;
    let body = desired.get_mut("layout").and_then(|l| l.children_mut().as_mut()).ok_or("missing workspace layout body")?;
    let wanted: BTreeSet<String> = body.nodes().iter().filter(|n| n.name().value() == "tab")
        .filter_map(|n| n.get("name").and_then(|e| e.value().as_string()).map(str::to_owned)).collect();
    let actual_body = actual.get("layout").and_then(|l| l.children()).ok_or("missing session layout body")?;
    let base = actual_body.get("cwd").and_then(|n| n.get(0)).and_then(|e| e.value().as_string())
        .map(Path::new).unwrap_or(Path::new("/"));
    let mut obsolete = BTreeSet::new();
    for tab in actual_body.nodes().iter().filter(|n| n.name().value() == "tab") {
        let Some(name) = tab.get("name").and_then(|e| e.value().as_string()) else { continue };
        let project = name.strip_suffix(" jj").unwrap_or(name);
        if wanted.contains(name) || project.is_empty() || Path::new(project).components().count() != 1 {
            continue;
        }
        let cwd = root.join(project);
        if has_cwd(tab, base, &cwd) {
            obsolete.insert(name.to_string());
        }
    }
    body.nodes_mut().retain(|n| {
        n.name().value() != "tab" || !tabs.iter().any(|t| {
            n.get("name").and_then(|e| e.value().as_string()) == Some(t.name.as_str())
        })
    });
    let has_additions = body.nodes().iter().any(|n| n.name().value() == "tab");
    desired.fmt();
    Ok(Plan {
        ready: !tabs.is_empty() && tabs.iter().filter(|t| wanted.contains(&t.name) || obsolete.contains(&t.name)).all(|t| t.selectable_tiled_panes_count + t.selectable_floating_panes_count > 0),
        layout: if has_additions { desired.to_string() } else { String::new() },
        close: tabs.iter().filter(|t| obsolete.contains(&t.name)).map(|t| t.tab_id).collect(),
    })
}

fn has_cwd(node: &kdl::KdlNode, base: &Path, cwd: &Path) -> bool {
    let own = node.get("cwd").and_then(|e| e.value().as_string()).map(|s| base.join(s));
    own.as_deref().unwrap_or(base).starts_with(cwd)
        || node.get("edit").and_then(|e| e.value().as_string()).is_some_and(|s| base.join(s).starts_with(cwd))
        || node.children().is_some_and(|body| body.nodes().iter().any(|n| has_cwd(n, own.as_deref().unwrap_or(base), cwd)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciles_restored_tabs_and_preserves_custom_tabs() {
        let snapshot = r#"[{"tab_id":4,"name":"old"},{"tab_id":9,"name":"old jj"},{"tab_id":12,"name":"kept"},{"tab_id":20,"name":"notes"}]
---WORKSPACE-LAYOUT---
layout {
    cwd "/work"
    tab name="old" { pane edit="/work/reek/old/file" cwd="other/repo"; }
    tab name="old jj" cwd="/work/reek/old" { pane; }
    tab name="kept" { pane cwd="/work/reek/kept"; }
    tab name="notes" { pane cwd="/elsewhere"; }
}
"#;
        let desired = crate::layout::compose(crate::layout::DEFAULT_TEMPLATE, &[
            crate::layout::Project { name: "kept".into(), dir: "/work/reek/kept".into() },
        ]).unwrap();
        let plan = plan(snapshot, &desired, Path::new("/work/reek")).unwrap();
        assert_eq!(plan.close, [4, 9]);
        assert!(plan.layout.contains("name=\"kept jj\""));
        assert!(!plan.layout.contains("name=\"kept\""));
    }

    #[test]
    fn unchanged_session_needs_no_tab_actions() {
        let snapshot = "[{\"tab_id\":2,\"name\":\"kept\"}]\n---WORKSPACE-LAYOUT---\nlayout { tab name=\"kept\" { pane; }; }";
        let plan = plan(snapshot, "layout { tab name=\"kept\" { pane; }; }", Path::new("/work/reek")).unwrap();
        assert!(plan.close.is_empty());
        assert!(plan.layout.is_empty());
        assert!(super::plan("invalid", "layout {}", Path::new("/work/reek")).is_err());
    }
}
