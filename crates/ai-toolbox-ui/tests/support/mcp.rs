use super::board;
use serde_json::{json, Value};

#[tokio::test(flavor = "multi_thread")]
async fn mcp_roundtrip_uses_a_retained_preview_and_private_backup() {
    let board = board().await;
    board._fixture.write_json(".mcp.json",json!({"mcpServers":{"browser":{"command":"never-run","directTools":true,"approveTools":["write"]}}}));
    let path = format!("/api/projects/{}/mcp", board.id);
    let (status, body) = board.get(&path);
    assert_eq!(status, 200, "{body}");
    let report: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["servers"][0]["direct_tools"], Value::Null);
    assert!(!board._home.path().join(".ai-toolbox").exists());
    let (status, body) = board.post(&format!("{path}/preview"), json!({"scope":"project"}));
    assert_eq!(status, 200, "{body}");
    let preview: Value = serde_json::from_str(&body).unwrap();
    let id = preview["id"].as_str().unwrap();
    assert!(!board.repo.join(".pi/mcp-adapter.json").exists());
    let (status, body) = board.post(&format!("/api/mcp/apply/{id}"), json!({}));
    assert_eq!(status, 200, "{body}");
    let applied: Value = serde_json::from_str(&body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(applied["backup"].as_str().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(board.repo.join(".pi/mcp-adapter.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["mcpServers"]["browser"]["directTools"], "search");
    let (status, body) = board.post(&format!("/api/mcp/rollback/{id}"), json!({}));
    assert_eq!(status, 200, "{body}");
    assert!(!board.repo.join(".pi/mcp-adapter.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_stale_previews_and_unauthorised_mutations_are_refused() {
    let board = board().await;
    board._fixture.write_json(
        ".mcp.json",
        json!({"mcpServers":{"browser":{"command":"never-run","directTools":true}}}),
    );
    let path = format!("/api/projects/{}/mcp", board.id);
    assert_eq!(board.without_token(&path).0, 401);
    let (status, body) = board.post(&format!("{path}/preview"), json!({"scope":"project"}));
    assert_eq!(status, 200, "{body}");
    let preview: Value = serde_json::from_str(&body).unwrap();
    let id = preview["id"].as_str().unwrap();
    for prefix in ["apply", "rollback"] {
        assert_eq!(
            board
                .request(
                    "POST",
                    &format!("/api/mcp/{prefix}/{id}"),
                    Some("{}".into()),
                    false
                )
                .0,
            401
        );
    }
    board._fixture.write_json(
        ".mcp.json",
        json!({"mcpServers":{"browser":{"command":"edited","directTools":true}}}),
    );
    let (status, body) = board.post(&format!("/api/mcp/apply/{id}"), json!({}));
    assert_ne!(status, 200);
    assert!(body.contains("changed since preview"));
    assert!(!board.repo.join(".pi/mcp-adapter.json").exists());
}
