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

const ROOT_PANE: &str = "pane";

/// nvim expanded on top, jjui collapsed beneath it.
const PROJECT_PANE: &str = "pane stacked=true {\n pane expanded=true command=\"nvim\" {\n args \".\"\n }\n pane command=\"jjui\"\n}";

/// Builds a zellij layout: a first tab at the workspace root, then one tab per
/// project with `nvim .` and `jjui` as stacked panes in the project root. Every
/// node under the template's `layout` (tab templates and the like) is kept once;
/// the template's own `tab` nodes are dropped. Only the first tab keeps focus.
pub fn compose(template: &str, root: &Project, projects: &[Project]) -> Result<String, String> {
    let doc: KdlDocument = template.parse().map_err(|e| format!("layout file: {e}"))?;
    let layout = doc.get("layout").ok_or("layout file has no `layout` node")?;
    let mut body = layout.children().cloned().unwrap_or_default();
    body.nodes_mut().retain(|node| node.name().value() != "tab");

    let tabs = std::iter::once((root, ROOT_PANE)).chain(projects.iter().map(|p| (p, PROJECT_PANE)));
    for (project, pane) in tabs {
        let source = format!("tab hide_floating_panes=true {{\n{pane}\n}}");
        let tab_doc: KdlDocument = source.parse().expect("static tab snippet is valid KDL");
        let mut tab = tab_doc.nodes()[0].clone();
        tab.insert("name", project.name.as_str());
        tab.insert("cwd", project.dir.to_string_lossy().as_ref());
        if !body.nodes().iter().any(|n| n.name().value() == "tab") {
            tab.insert("focus", true);
        }
        body.nodes_mut().push(tab);
    }

    let mut out_root = KdlNode::new("layout");
    out_root.set_children(body);
    let mut out = KdlDocument::new();
    out.nodes_mut().push(out_root);
    out.fmt();
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, dir: &str) -> Project {
        Project { name: name.into(), dir: dir.into() }
    }

    fn tab_names(out: &str) -> Vec<String> {
        let doc: KdlDocument = out.parse().unwrap();
        let body = doc.get("layout").unwrap().children().unwrap();
        body.nodes().iter().filter(|n| n.name().value() == "tab")
            .filter_map(|n| n.get("name").and_then(|e| e.value().as_string()).map(str::to_owned)).collect()
    }

    #[test]
    fn first_tab_is_the_workspace_root_followed_by_one_tab_per_project() {
        let out = compose(DEFAULT_TEMPLATE, &project("ws", "/w/s"), &[project("a", "/w/s/a"), project("b", "/w/s/b")]).unwrap();
        assert_eq!(tab_names(&out), ["ws", "a", "b"]);
        assert!(out.contains(r#"cwd="/w/s""#));
    }

    #[test]
    fn project_tab_stacks_nvim_and_jjui() {
        let out = compose(DEFAULT_TEMPLATE, &project("ws", "/w/s"), &[project("a", "/w/s/a")]).unwrap();
        assert_eq!(out.matches("stacked=").count(), 1);
        assert!(out.contains(r#"command="nvim""#) && out.contains(r#"command="jjui""#));
        assert!(!out.contains("a jj"));
    }

    #[test]
    fn only_the_root_tab_has_focus() {
        let out = compose(DEFAULT_TEMPLATE, &project("ws", "/w/s"), &[project("a", "/w/s/a")]).unwrap();
        assert_eq!(out.matches("focus=").count(), 1);
    }

    #[test]
    fn template_nodes_are_kept_once_and_template_tabs_dropped() {
        let template = "layout {\n default_tab_template { children; }\n tab name=\"x\" { pane; }\n}";
        let out = compose(template, &project("ws", "/w/s"), &[project("a", "/w/s/a"), project("b", "/w/s/b")]).unwrap();
        assert_eq!(out.matches("default_tab_template").count(), 1);
        assert!(!tab_names(&out).contains(&"x".to_string()));
    }

    #[test]
    fn no_projects_yields_only_the_root_tab() {
        let out = compose("layout { pane; }", &project("ws", "/w/s"), &[]).unwrap();
        assert_eq!(tab_names(&out), ["ws"]);
    }
}
