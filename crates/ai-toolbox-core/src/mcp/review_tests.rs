use super::*;
use crate::testing::Fixture;
use serde_json::json;
fn context(home: &Fixture, backend: Backend) -> Context {
    if backend == Backend::Adapter {
        home.write_json(
            ".pi/agent/settings.json",
            json!({"packages":["npm:pi-mcp-adapter"]}),
        );
    }
    home.write_json(
        ".pi/agent/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"version":"5.3.0"}),
    );
    let home = home.path().canonicalize().unwrap();
    Context {
        agent_dir: home.join(".pi/agent"),
        home,
        backend,
        adapter_manifest: None,
    }
}
#[test]
fn project_optimisation_cannot_turn_a_global_only_server_into_a_trust_gated_one() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Adapter);
    home.write_json(
        ".pi/agent/mcp-adapter.json",
        json!({"mcpServers":{"s":{"command":"never-run","directTools":true}}}),
    );
    let result = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    assert!(result.changes.is_empty());
    assert!(result.warnings.iter().any(|w| w.contains("global-only")));
    repo.write_json(
        ".mcp.json",
        json!({"mcpServers":{"local":{"command":"never-run","directTools":true}}}),
    );
    let result = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].server, "local");
    assert!(!repo.path().join(".pi/mcp-adapter.json").exists());
}
#[test]
fn adapter_native_codemode_does_not_cancel_a_lower_direct_choice() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Adapter);
    repo.write_json(
        ".mcp.json",
        json!({"mcpServers":{"s":{"command":"node","args":["x.js"],"directTools":true}}}),
    );
    repo.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"s":{"command":"node","args":["x.js"],"exposure":"codemode"}}}),
    );
    home.write_json(".pi/agent/mcp-cache.json",json!({"version":1,"servers":{"s":{"configHash":"ceb46d38f51cab86b226974b788e120c307120d0be0dd0f895fdbf188e440ab1","cachedAt":1,"tools":[{"name":"read"}]}}}));
    let report = inspect(&ctx, repo.path()).unwrap();
    assert_eq!(report.servers[0].exposure, "direct");
    assert_eq!(report.servers[0].direct_tools, Some(1));
    assert_eq!(
        report.servers[0].exposure_source,
        repo.path().join(".mcp.json")
    );
}
#[test]
fn native_explicit_tool_choices_survive_catalogue_deferral() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Native);
    repo.write_json(".pi/mcp.json",json!({"mcpServers":{"s":{"command":"never-run","exposure":"direct","toolExposure":{"read":"direct","delete":"hidden"}}}}));
    let p = preview(&ctx, repo.path(), &[], Options::default()).unwrap();
    apply(&ctx, &p.id).unwrap();
    let loaded = super::config::load(&ctx, repo.path()).unwrap();
    assert_eq!(loaded.entries["s"].value["exposure"], "deferred");
    assert_eq!(loaded.entries["s"].value["toolExposure"]["read"], "direct");
    assert_eq!(
        loaded.entries["s"].value["toolExposure"]["delete"],
        "hidden"
    );
}
#[test]
fn curated_small_native_servers_are_not_removed_just_because_the_cache_is_unknown() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Native);
    repo.write_json(".pi/mcp.json",json!({"mcpServers":{"context7":{"command":"never-run","exposure":"direct"},"excel":{"command":"never-run","exposure":"direct"}}}));
    assert!(preview(&ctx, repo.path(), &[], Options::default())
        .unwrap()
        .changes
        .is_empty());
}
#[test]
fn project_adapter_reenable_is_not_cancelled_by_translated_native_enabled() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Adapter);
    repo.write_json(".pi/mcp.json",json!({"mcpServers":{"s":{"command":"node","args":["x.js"],"enabled":false,"exposure":"direct"}}}));
    repo.write_json(
        ".pi/mcp-adapter.json",
        json!({"mcpServers":{"s":{"disabled":false}}}),
    );
    home.write_json(".pi/agent/mcp-cache.json",json!({"version":1,"servers":{"s":{"configHash":"ceb46d38f51cab86b226974b788e120c307120d0be0dd0f895fdbf188e440ab1","cachedAt":1,"tools":[{"name":"read"}]}}}));
    let report = inspect(&ctx, repo.path()).unwrap();
    assert!(report.servers[0].enabled);
    assert_eq!(report.servers[0].direct_tools, Some(1));
}
#[test]
fn provider_auth_named_by_a_project_is_reported_as_removed_not_available() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Adapter);
    home.write_json(".pi/agent/mcp-adapter.json",json!({"mcpServers":{"s":{"url":"https://fixture.invalid/mcp","auth":{"type":"provider","provider":"fixture"},"directTools":true}}}));
    repo.write_json(
        ".pi/mcp-adapter.json",
        json!({"mcpServers":{"s":{"directTools":"search"}}}),
    );
    let report = inspect(&ctx, repo.path()).unwrap();
    assert!(!report.complete);
    assert!(report.servers.is_empty());
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("removes provider-auth")));
    assert!(preview(&ctx, repo.path(), &[], Options::default()).is_err());
}
#[test]
fn mixed_backend_scopes_refuse_instead_of_editing_an_inactive_configuration() {
    let home = Fixture::bare();
    let a = Fixture::bare();
    let b = Fixture::bare();
    let ctx = context(&home, Backend::Native);
    a.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"s":{"command":"never-run","exposure":"direct"}}}),
    );
    b.write_json(
        ".pi/settings.json",
        json!({"packages":["npm:pi-mcp-adapter"]}),
    );
    b.write_json(
        ".pi/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"version":"5.3.0"}),
    );
    b.write_json(
        ".mcp.json",
        json!({"mcpServers":{"s":{"command":"never-run","directTools":true}}}),
    );
    let result = preview(
        &ctx,
        a.path(),
        &[a.path().into(), b.path().into()],
        Options {
            scope: Scope::All,
            ..Default::default()
        },
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("different MCP backend"));
    assert!(!home.path().join(".ai-toolbox").exists());
    assert!(!b.path().join(".pi/mcp-adapter.json").exists());
}
#[test]
fn disabled_builtin_and_filtered_adapter_registrations_fail_closed() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let ctx = context(&home, Backend::Native);
    for settings in [
        json!({"extensions":["-builtin:mcp"]}),
        json!({"packages":[{"source":"npm:pi-mcp-adapter","extensions":["index.ts"]}]}),
    ] {
        home.write_json(".pi/agent/settings.json", settings);
        assert!(ctx.for_project(repo.path()).is_err());
        assert!(preview(&ctx, repo.path(), &[], Options::default()).is_err());
    }
    assert!(!home.path().join(".ai-toolbox").exists());
}
