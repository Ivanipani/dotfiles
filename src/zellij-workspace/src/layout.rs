use std::path::PathBuf;

use kdl::{KdlDocument, KdlNode};

/// Used when no `layout` file is configured: no shared nodes, just the tabs.
pub const DEFAULT_TEMPLATE: &str = "layout { }";

/// File name of the stored layout, beside the set's manifest.
pub const FILE_NAME: &str = "layout.kdl";

pub struct Project {
    pub name: String,
    pub dir: PathBuf,
}

/// Per project: a tab and the command it runs in the project root.
const TABS: [(&str, &str); 2] = [("", "pane command=\"nvim\" borderless=true {\n args \".\"\n}"), (" jj", "pane command=\"jjui\" borderless=true")];

/// Names of the tabs `compose` creates for a project.
pub fn tab_names(project: &str) -> impl Iterator<Item = String> + '_ {
    TABS.iter().map(move |(suffix, _)| format!("{project}{suffix}"))
}

/// Builds a zellij layout with two tabs per project: `nvim .` and `jjui`, both
/// in the project root. Every node under the template's `layout` (tab templates
/// and the like) is kept once; the template's own `tab` nodes are dropped. Only
/// the first tab keeps focus.
pub fn compose(template: &str, projects: &[Project]) -> Result<String, String> {
    let doc: KdlDocument = template.parse().map_err(|e| format!("layout file: {e}"))?;
    let layout = doc.get("layout").ok_or("layout file has no `layout` node")?;
    let mut body = layout.children().cloned().unwrap_or_default();
    body.nodes_mut().retain(|node| node.name().value() != "tab");

    for (project, (suffix, pane)) in projects.iter().flat_map(|p| std::iter::repeat(p).zip(TABS.iter())) {
        let source = format!("tab hide_floating_panes=true {{\n{pane}\n}}");
        let tab_doc: KdlDocument = source.parse().expect("static tab snippet is valid KDL");
        let mut tab = tab_doc.nodes()[0].clone();
        tab.insert("name", format!("{}{suffix}", project.name).as_str());
        tab.insert("cwd", project.dir.to_string_lossy().as_ref());
        if !body.nodes().iter().any(|n| n.name().value() == "tab") {
            tab.insert("focus", true);
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
    fn two_tabs_per_project_with_shared_template_kept_once() {
        let template = r#"layout {
            default_tab_template { children; }
            tab name="x" { pane; }
        }"#;
        let projects = [
            Project { name: "a".into(), dir: "/w/s/a".into() },
            Project { name: "b".into(), dir: "/w/s/b".into() },
        ];
        let out = compose(template, &projects).unwrap();
        assert_eq!(out.matches("default_tab_template").count(), 1);
        assert_eq!(out.matches("hide_floating_panes").count(), 4);
        assert!(out.contains(r#"name="a jj""#) && out.contains(r#"command="jjui""#) && out.contains(r#"command="nvim""#));
        assert!(out.contains(r#"cwd="/w/s/a""#) && out.contains(r#"cwd="/w/s/b""#));
        assert_eq!(out.matches("focus=").count(), 1);
        let reparsed: KdlDocument = out.parse().unwrap();
        assert!(reparsed.get("layout").is_some());
    }

    #[test]
    fn default_template_yields_tabs_only() {
        let out = compose(DEFAULT_TEMPLATE, &[Project { name: "a".into(), dir: "/w/a".into() }]).unwrap();
        assert!(out.contains(r#"name="a""#));
    }

    #[test]
    fn no_projects_yields_empty_layout() {
        assert!(!compose("layout { pane; }", &[]).unwrap().contains("tab"));
    }
}
