use super::board;
use serde_json::{json, Value};
#[tokio::test(flavor = "multi_thread")]
async fn an_open_board_redetects_backends_and_rejects_a_previous_backends_preview() {
    let board = board().await;
    board._fixture.write_json(
        ".mcp.json",
        json!({"mcpServers":{"browser":{"command":"never-run","directTools":true}}}),
    );
    let path = format!("/api/projects/{}/mcp", board.id);
    assert_eq!(board.json(&path)["backend"], "adapter");
    let previous = board.post_json(&format!("{path}/preview"), json!({"scope":"project"}));
    board
        ._home
        .write_json(".pi/agent/settings.json", json!({"packages":[]}));
    assert_eq!(board.json(&path)["backend"], "native");
    let (status, body) = board.post(
        &format!("/api/mcp/apply/{}", previous["id"].as_str().unwrap()),
        json!({}),
    );
    assert_ne!(status, 200);
    assert!(body.contains("changed since preview"));
    assert!(!board.repo.join(".pi/mcp-adapter.json").exists());
    board._fixture.write_json(
        ".pi/mcp.json",
        json!({"mcpServers":{"browser":{"command":"never-run","exposure":"direct"}}}),
    );
    let next = board.post_json(&format!("{path}/preview"), json!({"scope":"project"}));
    assert_eq!(next["reports"][0]["backend"], "native");
    assert!(next["changes"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with(".pi/mcp.json"));
    board.post_json(
        &format!("/api/mcp/apply/{}", next["id"].as_str().unwrap()),
        json!({}),
    );
    board.post_json(
        &format!("/api/projects/{}/apply", board.id),
        json!({"kind":"install","presets":["chrome-devtools"],"harnesses":["pi"]}),
    );
    let native: Value =
        serde_json::from_slice(&std::fs::read(board.repo.join(".pi/mcp.json")).unwrap()).unwrap();
    assert_eq!(native["mcpServers"]["chrome-devtools"]["command"], "npx");
    assert!(!board.repo.join(".pi/mcp-adapter.json").exists());
    // A project-only adapter also becomes the active backend, including its version.
    board._fixture.write_json(
        ".pi/settings.json",
        json!({"packages":["npm:pi-mcp-adapter"]}),
    );
    board._fixture.write_json(
        ".pi/npm/node_modules/pi-mcp-adapter/package.json",
        json!({"name":"pi-mcp-adapter","version":"5.3.1"}),
    );
    assert_eq!(board.json(&path)["backend"], "adapter");
}
