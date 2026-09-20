use std::path::PathBuf;

use kdl::{KdlDocument, KdlNode};

/// Used when no `layout` file is configured: a bare tab per project.
pub const DEFAULT_TEMPLATE: &str = "layout { tab; }";

pub struct Project {
    pub name: String,
    pub dir: PathBuf,
}

/// Builds a zellij layout with one tab per project. The template's first
/// `tab` node is the prototype: it is copied per project with `name` and
/// `cwd` set, and every other node under `layout` (tab templates and the like)
/// is kept once. Only the first project's tab keeps `focus`.
pub fn compose(template: &str, projects: &[Project]) -> Result<String, String> {
    let doc: KdlDocument = template.parse().map_err(|e| format!("layout file: {e}"))?;
    let layout = doc.get("layout").ok_or("layout file has no `layout` node")?;
    let mut body = layout.children().cloned().unwrap_or_default();
    let prototype = body
        .nodes()
        .iter()
        .find(|node| node.name().value() == "tab")
        .cloned()
        .ok_or("layout file has no `tab` node")?;
    body.nodes_mut().retain(|node| node.name().value() != "tab");

    for (i, project) in projects.iter().enumerate() {
        let mut tab = prototype.clone();
        tab.insert("name", project.name.as_str());
        tab.insert("cwd", project.dir.to_string_lossy().as_ref());
        if i > 0 {
            // `remove` compares source formatting too, so a parsed key never matches.
            tab.entries_mut().retain(|e| e.name().map(|n| n.value()) != Some("focus"));
        }
        body.nodes_mut().push(tab);
    }

    let mut root = KdlNode::new("layout");
    root.set_children(body);
    let mut out = KdlDocument::new();
    out.nodes_mut().push(root);
    out.fmt();
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tab_per_project_with_shared_template_kept_once() {
        let template = r#"layout {
            default_tab_template { children; }
            tab name="x" focus=true { pane command="nvim"; }
        }"#;
        let projects = [
            Project { name: "a".into(), dir: "/w/s/a".into() },
            Project { name: "b".into(), dir: "/w/s/b".into() },
        ];
        let out = compose(template, &projects).unwrap();
        assert_eq!(out.matches("default_tab_template").count(), 1);
        assert_eq!(out.matches("tab name=").count(), 2);
        assert!(out.contains(r#"cwd="/w/s/a""#) && out.contains(r#"cwd="/w/s/b""#));
        assert_eq!(out.matches("focus=").count(), 1);
        let reparsed: KdlDocument = out.parse().unwrap();
        assert!(reparsed.get("layout").is_some());
    }

    #[test]
    fn default_template_yields_bare_tabs() {
        let out = compose(DEFAULT_TEMPLATE, &[Project { name: "a".into(), dir: "/w/a".into() }]).unwrap();
        assert!(out.contains(r#"name="a""#));
    }

    #[test]
    fn rejects_layout_without_tab() {
        assert!(compose("layout { pane; }", &[]).is_err());
    }
}
