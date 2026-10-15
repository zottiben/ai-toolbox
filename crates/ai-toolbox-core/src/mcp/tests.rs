use super::*;
use crate::testing::Fixture;
use serde_json::json;

fn context(home: &Fixture) -> Context {
    home.write_json(
        ".pi/agent/settings.json",
        json!({"packages":["npm:pi-mcp-adapter"]}),
    );
    let root = home.path().canonicalize().unwrap();
    Context {
        home: root.clone(),
        agent_dir: root.join(".pi/agent"),
        backend: Backend::Adapter,
        adapter_manifest: None,
    }
}
fn seed(home: &Fixture, repo: &Fixture) -> Context {
    home.write_json(
        ".pi/agent/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"version":"5.3.0"}),
    );
    home.write_json(".pi/agent/mcp-adapter.json", json!({"mcpServers":{"browser":{"command":"must-not-run","directTools":true,"approveTools":"destructive","includeTools":["read_one"],"lifecycle":"eager"}}}));
    repo.write_json(
        ".pi/mcp-adapter.json",
        json!({"mcpServers":{"browser":{"directTools":true}}}),
    );
    context(home)
}
#[test]
fn effective_overrides_and_unknown_counts_are_honest() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    let report = inspect(&ctx, repo.path()).unwrap();
    assert_eq!(report.servers[0].exposure, "direct");
    assert_eq!(report.servers[0].direct_tools, None);
    assert_eq!(report.unknown_catalogues, 1);
    assert_eq!(
        report.servers[0].exposure_source,
        repo.path().join(".pi/mcp-adapter.json")
    );
    assert!(report.servers[0].recommend_search);
    assert!(!home.path().join(".ai-toolbox").exists());
}
#[test]
fn preview_apply_and_rollback_are_exact_and_keep_security() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    let global_before = std::fs::read(ctx.agent_dir.join("mcp-adapter.json")).unwrap();
    let path = repo.path().join(".pi/mcp-adapter.json");
    let before = std::fs::read(&path).unwrap();
    let preview = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(preview.changes.len(), 1);
    let applied = apply(&ctx, &preview.id).unwrap();
    assert_eq!(applied.files, 1);
    assert_eq!(
        inspect(&ctx, repo.path()).unwrap().servers[0].exposure,
        "search"
    );
    assert_eq!(
        std::fs::read(ctx.agent_dir.join("mcp-adapter.json")).unwrap(),
        global_before
    );
    assert!(apply(&ctx, &preview.id).is_err());
    rollback(&ctx, &preview.id).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(rollback(&ctx, &preview.id).is_err());
}
#[test]
fn changing_any_effective_input_refuses_the_entire_preview() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    let path = repo.path().join(".pi/mcp-adapter.json");
    let before = std::fs::read(&path).unwrap();
    let preview = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    home.write_json(
        ".pi/agent/mcp-adapter.json",
        json!({"mcpServers":{"browser":{"command":"different","directTools":true}}}),
    );
    assert!(apply(&ctx, &preview.id)
        .unwrap_err()
        .to_string()
        .contains("changed since preview"));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[test]
fn rollback_never_overwrites_later_edits() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    let p = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    apply(&ctx, &p.id).unwrap();
    repo.write_json(
        ".pi/mcp-adapter.json",
        json!({"mcpServers":{},"notes":"mine"}),
    );
    assert!(rollback(&ctx, &p.id).is_err());
}
#[test]
fn retained_servers_and_unknown_selected_tools() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    let p = preview(
        &ctx,
        repo.path(),
        &[],
        Options {
            keep_direct: vec!["browser".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(p.changes.is_empty());
    assert!(preview(
        &ctx,
        repo.path(),
        &[],
        Options {
            keep_tools: [("browser".into(), vec!["invented".into()])].into(),
            ..Options::default()
        }
    )
    .is_err());
}
#[test]
fn native_only_needs_no_adapter_or_shared_config() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let mut ctx = context(&home);
    ctx.backend = Backend::Native;
    std::fs::remove_file(ctx.agent_dir.join("settings.json")).unwrap();
    home.write_json(
        ".pi/agent/mcp.json",
        json!({"mcpServers":{"docs":{"url":"https://example.invalid/mcp","exposure":"direct"}}}),
    );
    let p = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    apply(&ctx, &p.id).unwrap();
    assert_eq!(
        inspect(&ctx, repo.path()).unwrap().servers[0].exposure,
        "search"
    );
    assert!(!repo.path().join(".pi/mcp-adapter.json").exists());
    rollback(&ctx, &p.id).unwrap();
    assert!(!repo.path().join(".pi/mcp.json").exists());
}
#[test]
fn cache_hash_private_expiry_and_filtering() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = seed(&home, &repo);
    home.write_json(
        ".pi/agent/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"version":"5.3.0"}),
    );
    let loaded = super::config::load(&ctx, repo.path()).unwrap();
    let definition = &loaded.entries["browser"].value;
    let hash = super::cache::identity(&ctx, definition).unwrap();
    let entry =
        json!({"configHash":hash,"cachedAt":1,"tools":[{"name":"read_one"},{"name":"delete"}]});
    home.write_json(
        ".pi/agent/mcp-cache.json",
        json!({"version":1,"servers":{"browser":entry}}),
    );
    let r = inspect(&ctx, repo.path()).unwrap();
    assert_eq!(r.servers[0].direct_tools, Some(1));
    for extra in [
        json!({"cacheScope":"private"}),
        json!({"ttlMs":0}),
        json!({"configHash":"mismatch"}),
    ] {
        let mut bad = entry.clone();
        for (k, v) in extra.as_object().unwrap() {
            bad[k] = v.clone();
        }
        home.write_json(
            ".pi/agent/mcp-cache.json",
            json!({"servers":{"browser":bad}}),
        );
        assert_eq!(
            inspect(&ctx, repo.path()).unwrap().servers[0].direct_tools,
            None
        );
    }
}
#[test]
fn multi_project_scope_is_explicit_and_symlinks_are_refused() {
    let home = Fixture::bare();
    let a = Fixture::bare();
    let b = Fixture::bare();
    let ctx = seed(&home, &a);
    b.write_json(
        ".mcp.json",
        json!({"mcpServers":{"browser":{"directTools":true}}}),
    );
    let p = preview(
        &ctx,
        a.path(),
        &[a.path().into(), b.path().into()],
        Options {
            scope: Scope::All,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(p.reports.len(), 2);
    apply(&ctx, &p.id).unwrap();
    assert_eq!(
        inspect(&ctx, b.path()).unwrap().servers[0].exposure,
        "search"
    );
    rollback(&ctx, &p.id).unwrap();
    std::fs::remove_dir(b.path().join(".pi")).unwrap();
    let external = Fixture::bare();
    std::os::unix::fs::symlink(external.path(), b.path().join(".pi")).unwrap();
    assert!(preview(&ctx, b.path(), &[], Options::default()).is_err());
}
