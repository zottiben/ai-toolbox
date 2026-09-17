//! The API over a real socket.
//!
//! Driven with plain HTTP rather than by calling handlers, because the things most likely
//! to break are the ones a direct call skips: routing, the token layer, and whether the
//! JSON the board will actually receive says what the client expects.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use ai_toolbox_core::registry::Registry;
use ai_toolbox_core::testing::{self, Fixture};
use ai_toolbox_ui::{ServeOptions, Server};

struct Board {
    addr: String,
    token: String,
    _fixture: Fixture,
    repo: PathBuf,
    id: i64,
}

/// A server over a temporary registry holding one configured repo.
async fn board() -> Board {
    let fixture = Fixture::configured();
    let repo = fixture.path().to_path_buf();

    let mut registry = Registry::memory().expect("an in-memory registry");
    let recorded = registry.record(&repo, true).expect("recording the fixture");

    let server = Server::bind(testing::catalogue_root(), registry, ServeOptions::default())
        .await
        .expect("binding the board");

    let addr = server.addr().to_string();
    let token = server.token().to_string();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });

    Board {
        addr,
        token,
        repo: recorded.path.clone(),
        id: recorded.id,
        _fixture: fixture,
    }
}

impl Board {
    fn get(&self, path: &str) -> (u16, String) {
        self.request("GET", path, None, true)
    }

    fn post(&self, path: &str, body: serde_json::Value) -> (u16, String) {
        self.request("POST", path, Some(body.to_string()), true)
    }

    fn without_token(&self, path: &str) -> (u16, String) {
        self.request("GET", path, None, false)
    }

    /// A hand-rolled client, so the test needs no HTTP dependency of its own.
    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<String>,
        token: bool,
    ) -> (u16, String) {
        let mut stream = TcpStream::connect(&self.addr).expect("connecting to the board");
        let body = body.unwrap_or_default();
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.addr
        );
        if token {
            request.push_str(&format!(
                "{}: {}\r\n",
                ai_toolbox_ui::TOKEN_HEADER,
                self.token
            ));
        }
        if !body.is_empty() {
            request.push_str("Content-Type: application/json\r\n");
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        request.push_str("\r\n");
        request.push_str(&body);
        stream.write_all(request.as_bytes()).expect("writing");
        stream.flush().expect("flushing");

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("a status line");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);

        // Drain the headers, then everything else is the body - `Connection: close`
        // means end-of-stream is end-of-body, so no chunk parsing is needed.
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("a header line");
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let mut rest = String::new();
        let _ = reader.read_to_string(&mut rest);
        (status, rest)
    }

    fn json(&self, path: &str) -> serde_json::Value {
        let (status, body) = self.get(path);
        assert_eq!(status, 200, "GET {path} -> {status}: {body}");
        serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("GET {path} gave non-JSON: {e}\n{body}"))
    }

    fn post_json(&self, path: &str, body: serde_json::Value) -> serde_json::Value {
        let (status, text) = self.post(path, body);
        assert_eq!(status, 200, "POST {path} -> {status}: {text}");
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("POST {path} gave non-JSON: {e}\n{text}"))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_refuses_a_request_without_the_token() {
    let board = board().await;
    let (status, _) = board.without_token("/api/projects");
    assert_eq!(status, 401);

    // And the board itself is served without one, because a fresh tab has to load
    // something before it can present a token.
    let (status, body) = board.request("GET", "/", None, false);
    assert_eq!(status, 200);
    assert!(
        body.contains("<!doctype html>") || body.contains("<html"),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_project_list_reports_the_state_of_each_repo() {
    let board = board().await;
    let projects = board.json("/api/projects");
    let list = projects.as_array().expect("an array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["state"], "healthy");
    assert!(list[0]["counts"]["managed"].as_u64().unwrap() > 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_carries_its_inventory_findings_and_worktrees() {
    let board = board().await;
    let detail = board.json(&format!("/api/projects/{}", board.id));

    assert_eq!(detail["exists"], true);
    assert!(detail["inventory"]["skills"].as_array().unwrap().len() >= 2);
    assert!(
        detail["findings"].as_array().unwrap().is_empty(),
        "{detail}"
    );
    // The fixture is not a git repo, so there is nothing to compare - and that has to
    // come back as an empty comparison rather than as an error.
    assert!(detail["worktrees"]["worktrees"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!detail["available"]["hooks"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_project_is_a_404_rather_than_a_crash() {
    let board = board().await;
    let (status, body) = board.get("/api/projects/99999");
    assert_eq!(status, 404);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["code"],
        "not_found"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn planning_writes_nothing_and_applying_writes_what_was_planned() {
    let board = board().await;
    let request = serde_json::json!({
        "kind": "install",
        "hooks": ["guard-irreversible"],
        "harnesses": ["claude"]
    });

    let plan = board.post_json(&format!("/api/projects/{}/plan", board.id), request.clone());
    assert!(plan["changes"].as_u64().unwrap() > 0);
    let summaries: Vec<&str> = plan["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["summary"].as_str().unwrap())
        .collect();
    assert!(
        summaries.iter().any(|s| s.contains("guard-irreversible")),
        "{summaries:?}"
    );
    assert!(
        !board
            .repo
            .join(".agents/hooks/guard-irreversible.sh")
            .exists(),
        "planning must not write"
    );

    let applied = board.post_json(
        &format!("/api/projects/{}/apply", board.id),
        request.clone(),
    );
    assert!(applied["applied"].as_u64().unwrap() > 0);
    assert!(applied["stale"].as_array().unwrap().is_empty());
    assert!(board
        .repo
        .join(".agents/hooks/guard-irreversible.sh")
        .is_file());

    // Planning the same thing again is all no-ops, which is what lets the board show
    // "already installed" rather than offering it twice.
    let again = board.post_json(&format!("/api/projects/{}/plan", board.id), request);
    assert_eq!(again["changes"].as_u64().unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn repair_fixes_what_doctor_found() {
    let board = board().await;
    std::fs::remove_file(board.repo.join(".agents/hooks/format-on-edit.sh")).unwrap();

    let detail = board.json(&format!("/api/projects/{}", board.id));
    let findings = detail["findings"].as_array().unwrap();
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f["severity"] == "broken"));

    let applied = board.post_json(
        &format!("/api/projects/{}/apply", board.id),
        serde_json::json!({ "kind": "repair" }),
    );
    assert!(applied["applied"].as_u64().unwrap() > 0);

    let after = board.json(&format!("/api/projects/{}", board.id));
    assert!(
        after["findings"].as_array().unwrap().is_empty(),
        "{}",
        after["findings"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_preset_name_is_a_bad_request_not_a_server_error() {
    let board = board().await;
    let (status, body) = board.post(
        &format!("/api/projects/{}/plan", board.id),
        serde_json::json!({ "kind": "install", "presets": ["no-such-preset"] }),
    );
    assert_eq!(status, 400, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["code"], "bad_request");
    assert!(json["error"].as_str().unwrap().contains("no-such-preset"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_catalogue_lists_what_can_be_installed() {
    let board = board().await;
    let catalogue = board.json("/api/catalogue");
    assert!(catalogue["hooks"].as_array().unwrap().len() >= 5);
    assert!(catalogue["presets"].as_array().unwrap().len() >= 5);
    // Group skills keep the key they are asked for by.
    let skills = catalogue["skills"].as_array().unwrap();
    assert!(skills.iter().any(|s| s["key"] == "cli/gh"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_machine_endpoint_separates_machine_facts_from_repo_state() {
    let board = board().await;
    let machine = board.json("/api/machine");
    assert!(machine["machine"]["charters"].as_array().unwrap().len() == 3);
    assert!(machine["catalogue_root"].is_string());
    assert_eq!(machine["harnesses"].as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worktree_can_be_converged_from_the_board() {
    let lab = testing::Lab::new();
    lab.configure();
    let feature = lab.add_worktree("feature");

    let mut registry = Registry::memory().unwrap();
    let recorded = registry.record(lab.main(), true).unwrap();
    let server = Server::bind(testing::catalogue_root(), registry, ServeOptions::default())
        .await
        .unwrap();
    let addr = server.addr().to_string();
    let token = server.token().to_string();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });

    let board = Board {
        addr,
        token,
        repo: lab.main().to_path_buf(),
        id: recorded.id,
        _fixture: Fixture::bare(),
    };

    let detail = board.json(&format!("/api/projects/{}", board.id));
    let worktrees = detail["worktrees"]["worktrees"].as_array().unwrap();
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[1]["standing"], "unconfigured");

    board.post_json(
        &format!("/api/projects/{}/apply", board.id),
        serde_json::json!({ "kind": "converge" }),
    );

    assert!(feature.join(".mcp.json").is_file());
    let after = board.json(&format!("/api/projects/{}", board.id));
    assert_eq!(after["worktrees"]["worktrees"][1]["standing"], "in-step");
    // Keeps the lab alive until the assertions are done.
    drop(lab);
}

#[tokio::test(flavor = "multi_thread")]
async fn scanning_finds_repos_and_adding_one_by_path_works() {
    let board = board().await;
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("discovered");
    std::fs::create_dir_all(&repo).unwrap();
    testing::git(&repo, &["init", "-q", "-b", "main", "."]);

    let result = board.post_json(
        "/api/projects/scan",
        serde_json::json!({ "roots": [root.path()] }),
    );
    assert_eq!(result["added"].as_array().unwrap().len(), 1);

    let projects = board.json("/api/projects");
    assert_eq!(projects.as_array().unwrap().len(), 2);
    // Discovered, not configured - the difference the list shows.
    let discovered = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["repo"]["path"].as_str().unwrap().ends_with("discovered"))
        .expect("the discovered repo");
    assert!(discovered["repo"]["last_configured"].is_null());
    assert_eq!(discovered["state"], "unconfigured");
}

#[tokio::test(flavor = "multi_thread")]
async fn forgetting_a_project_leaves_its_files_alone() {
    let board = board().await;
    let removed = board.post_json(
        &format!("/api/projects/{}/forget", board.id),
        serde_json::json!({}),
    );
    assert_eq!(removed, serde_json::json!(true));
    assert!(board.json("/api/projects").as_array().unwrap().is_empty());
    assert!(
        board.repo.join(".mcp.json").is_file(),
        "forget is not uninstall"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_route_serves_the_board_so_a_reload_on_a_deep_link_works() {
    let board = board().await;
    let (status, body) = board.request("GET", "/project/7", None, false);
    assert_eq!(status, 200);
    assert!(
        body.contains("<html") || body.contains("<!doctype"),
        "{body}"
    );
}

fn _assert_paths_are_absolute(path: &Path) -> bool {
    path.is_absolute()
}
