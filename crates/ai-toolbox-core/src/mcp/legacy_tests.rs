use super::*;
use crate::{
    install,
    testing::{self, Fixture},
    Harness,
};
use serde_json::json;
#[test]
fn optimisation_cannot_shadow_a_legacy_server_map() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    let root = home.path().canonicalize().unwrap();
    let context = Context {
        agent_dir: root.join(".pi/agent"),
        home: root,
        backend: Backend::Adapter,
        adapter_manifest: None,
    };
    home.write_json(
        ".pi/agent/settings.json",
        json!({"packages":["npm:pi-mcp-adapter"]}),
    );
    home.write_json(
        ".pi/agent/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"version":"5.3.0"}),
    );
    home.write_json(
        ".config/mcp/mcp.json",
        json!({"mcpServers":{"other":{"command":"never-run","directTools":true}}}),
    );
    let legacy = json!({"mcp-servers":{"preserve":{"command":"never-run","directTools":true}}});
    home.write_json(".pi/agent/mcp-adapter.json", legacy.clone());
    let report = inspect(&context, repo.path()).unwrap();
    assert!(!report.complete);
    assert!(preview(
        &context,
        repo.path(),
        &[repo.path().into()],
        Options {
            scope: Scope::Machine,
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(
        super::config::document(&context.agent_dir.join("mcp-adapter.json")).unwrap(),
        legacy
    );
    assert!(!home.path().join(".ai-toolbox").exists());
}
#[test]
fn installing_for_claude_cannot_shadow_an_adapters_legacy_servers() {
    let repo = Fixture::bare();
    repo.write_json(
        ".mcp.json",
        json!({"mcp-servers":{"preserve":{"command":"never-run"}}}),
    );
    let result = install::mcp(
        &mut install::Plan::default(),
        repo.path(),
        &testing::catalogue(),
        &["chrome-devtools".into()],
        &[Harness::Claude],
    );
    assert!(result.is_err());
}
