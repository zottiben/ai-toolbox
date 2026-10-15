//! Real CLI regressions with isolated HOME, registry and Pi settings. No Pi/server calls.
use ai_toolbox_core::testing::{self, Fixture};
use serde_json::{json, Value};
use std::process::{Command, Output};
fn command(home: &Fixture, repo: &Fixture, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ai-toolbox"))
        .env("HOME", home.path())
        .env("PI_CODING_AGENT_DIR", home.path().join(".pi/agent"))
        .env("AI_TOOLBOX", testing::catalogue_root())
        .env("AI_TOOLBOX_DB", home.path().join("registry.db"))
        .env(
            "PATH",
            format!(
                "{}:{}",
                home.path().join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env_remove("PI_MCP_CONFIG")
        .env_remove("PI_MCP_CONFIG_MODE")
        .env_remove("MCP_DIRECT_TOOLS")
        .args(["--repo", repo.path().to_str().unwrap(), "--json"])
        .args(args)
        .output()
        .unwrap()
}
fn adapter(home: &Fixture, enabled: bool) {
    home.write_json(
        ".pi/agent/settings.json",
        if enabled {
            json!({"packages":["npm:pi-mcp-adapter"]})
        } else {
            json!({"packages":[{"source":"npm:pi-mcp-adapter","extensions":[]}]})
        },
    );
    home.write_json(
        ".pi/agent/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"name":"pi-mcp-adapter","version":"5.3.0"}),
    );
}
#[test]
fn a_disabled_adapter_package_selects_native_mcp() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    adapter(&home, false);
    let out = command(&home, &repo, &["mcp-report"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["backend"], "native");
}
#[test]
fn installing_a_preset_does_not_override_an_inherited_direct_tool_selection() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    adapter(&home, true);
    home.write_json(".pi/agent/mcp-adapter.json",json!({"mcpServers":{"chrome-devtools":{"command":"npx","directTools":["take_snapshot"],"lifecycle":"lazy"}}}));
    let out = command(&home, &repo, &["--harness", "pi", "mcp", "chrome-devtools"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = command(&home, &repo, &["mcp-report"]);
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["servers"][0]["exposure"], "selected");
    let local: Value =
        serde_json::from_slice(&std::fs::read(repo.path().join(".mcp.json")).unwrap()).unwrap();
    assert!(local["mcpServers"]["chrome-devtools"]
        .get("lifecycle")
        .is_none());
}
#[test]
fn a_native_preset_cannot_shadow_a_global_servers_credentials() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    home.write_json(".pi/agent/mcp.json",json!({"mcpServers":{"chrome-devtools":{"command":"npx","args":["private-launcher"],"env":{"TOKEN":"fixture-only"}}}}));
    repo.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"chrome-devtools":{"exposure":"deferred"}}}),
    );
    let before = std::fs::read(repo.path().join(".pi/mcp.json")).unwrap();
    let out = command(&home, &repo, &["--harness", "pi", "mcp", "chrome-devtools"]);
    assert!(
        !out.status.success(),
        "must refuse to replace a credential-bearing inherited native definition"
    );
    assert_eq!(
        std::fs::read(repo.path().join(".pi/mcp.json")).unwrap(),
        before
    );
}
#[test]
fn backend_override_is_inspection_only() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    adapter(&home, true);
    repo.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"s":{"command":"never-run","exposure":"direct"}}}),
    );
    let out = command(
        &home,
        &repo,
        &["mcp-optimise", "preview", "--backend", "native"],
    );
    assert!(
        !out.status.success(),
        "must not optimise an inactive backend"
    );
    assert!(!home.path().join(".ai-toolbox/mcp-optimisation").exists());
}
#[test]
fn an_old_pi_adapter_does_not_import_native_mcp_files() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    adapter(&home, true);
    home.write("bin/pi", "not executable; detection must never run this");
    home.write_json(
        "package.json",
        json!({"name":"@earendil-works/pi-coding-agent","version":"0.98.0"}),
    );
    home.write_json(
        ".pi/agent/mcp.json",
        json!({"mcpServers":{"global-native":{"command":"never-run","exposure":"direct"}}}),
    );
    repo.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"project-native":{"command":"never-run","exposure":"direct"}}}),
    );
    repo.write_json(
        ".mcp.json",
        json!({"mcpServers":{"adapter":{"command":"never-run","directTools":false}}}),
    );
    let out = command(&home, &repo, &["mcp-report"]);
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["servers"].as_array().unwrap().len(), 1);
    assert_eq!(report["servers"][0]["name"], "adapter");
}
#[test]
fn doctor_repair_does_not_require_a_resolved_mcp_backend() {
    let home = Fixture::bare();
    let repo = Fixture::bare();
    adapter(&home, true);
    home.write_json(
        ".pi/agent/settings.json",
        json!({"packages":[{"source":"npm:pi-mcp-adapter","extensions":["index.ts"]}]}),
    );
    repo.write_json(".pi/mcp.json", json!({}));
    let out = command(&home, &repo, &["doctor", "--fix"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["mcp_error"].is_string());
}
