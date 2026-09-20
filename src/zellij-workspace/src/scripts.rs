/// Enable recovery on every future jj snapshot, scoped to these workspaces.
pub const PREPARE_SCRIPT: &str = r#"
for d; do
  jj -R "$d" config set --workspace snapshot.auto-update-stale true || exit 1
done
"#;
