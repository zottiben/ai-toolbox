use ai_toolbox_core::{action, install, testing, Harness};
use serde_json::json;

#[test]
fn reinstall_preserves_exposure_lifecycle_and_security_choices() {
    let fixture = testing::Fixture::bare();
    for exposure in [
        json!(false),
        json!("search"),
        json!(["take_snapshot"]),
        json!(true),
    ] {
        fixture.write_json(".mcp.json", json!({"mcpServers":{"chrome-devtools":{
            "command":"npx", "args":["old-package"], "directTools":exposure,
            "lifecycle":"lazy-keep-alive", "approveTools":"destructive", "includeTools":["take_snapshot"]
        }}}));
        let mut plan = install::Plan::default();
        install::mcp(
            &mut plan,
            fixture.path(),
            &testing::catalogue(),
            &["chrome-devtools".into()],
            &[Harness::Pi],
        )
        .unwrap();
        action::apply(&plan.actions).unwrap();
        let doc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(fixture.path().join(".mcp.json")).unwrap(),
        )
        .unwrap();
        let server = &doc["mcpServers"]["chrome-devtools"];
        assert_eq!(server["directTools"], exposure);
        assert_eq!(server["lifecycle"], "lazy-keep-alive");
        assert_eq!(server["approveTools"], "destructive");
        assert_eq!(server["includeTools"], json!(["take_snapshot"]));
        assert_ne!(server["args"], json!(["old-package"]));
    }
}

#[test]
fn new_stdio_catalogues_are_not_always_loaded() {
    let preset = testing::catalogue()
        .preset("chrome-devtools")
        .unwrap()
        .clone();
    let (shared, _, _) = ai_toolbox_core::convert::pi::adapter(
        "chrome-devtools",
        &preset.servers["chrome-devtools"],
        true,
    );
    assert_eq!(shared["directTools"], "search");
}

#[test]
fn native_installation_is_complete_and_reinstallation_keeps_tool_choices() {
    let fixture = testing::Fixture::bare();
    let catalogue = testing::catalogue();
    for attempt in 0..2 {
        let mut plan = install::Plan {
            pi_target: Some(ai_toolbox_core::convert::pi::Target::Native),
            ..install::Plan::default()
        };
        install::mcp(
            &mut plan,
            fixture.path(),
            &catalogue,
            &["chrome-devtools".into()],
            &[Harness::Pi],
        )
        .unwrap();
        action::apply(&plan.actions).unwrap();
        let path = fixture.path().join(".pi/mcp.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let server = &mut doc["mcpServers"]["chrome-devtools"];
        assert_eq!(server["command"], "npx");
        assert!(server.get("directTools").is_none());
        assert!(server.get("lifecycle").is_none());
        if attempt == 0 {
            assert_eq!(server["exposure"], "deferred");
            server["exposure"] = json!("codemode");
            server["toolExposure"] = json!({"take_snapshot":"direct","dangerous":"hidden"});
            fixture.write_json(".pi/mcp.json", doc);
        } else {
            assert_eq!(server["exposure"], "codemode");
            assert_eq!(
                server["toolExposure"],
                json!({"take_snapshot":"direct","dangerous":"hidden"})
            );
        }
    }
    assert!(!fixture.path().join(".pi/mcp-adapter.json").exists());
    assert_eq!(fixture.inventory().pi.native.len(), 1);
}

#[test]
fn adapter_overrides_are_visible_in_inventory_and_worktree_fingerprint() {
    let fixture = testing::Fixture::bare();
    let before = ai_toolbox_core::worktree::fingerprint(fixture.path());
    fixture.write_json(
        ".pi/mcp-adapter.json",
        json!({"mcpServers":{"browser":{"command":"must-not-run", "directTools":"search"}}}),
    );
    assert!(!fixture.inventory().pi.overrides.is_empty());
    assert_ne!(
        before,
        ai_toolbox_core::worktree::fingerprint(fixture.path())
    );
}
