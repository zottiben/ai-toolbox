#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn command(temp: &tempfile::TempDir, response: &str) -> Command {
    let tools = temp.path().join("tools");
    fs::create_dir_all(&tools).unwrap();
    fs::write(temp.path().join("release.json"), response).unwrap();
    fs::write(
        tools.join("curl"),
        "#!/bin/sh\n/bin/cat \"$HOME/release.json\"\n",
    )
    .unwrap();
    fs::set_permissions(tools.join("curl"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ai-toolbox"));
    command
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("PATH", &tools)
        .env("AI_TOOLBOX", temp.path().join("missing-catalogue"));
    command
}

#[test]
fn check_and_dry_run_work_without_a_catalogue_or_repo_and_write_nothing() {
    for flag in ["--check", "--dry-run"] {
        let temp = tempfile::tempdir().unwrap();
        let output = command(&temp, r#"{"tag_name":"v99.0.0"}"#)
            .args(["update", flag, "--json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["available"], true);
        assert_eq!(json["current"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["latest"], "99.0.0");
        assert!(!temp.path().join(".ai-toolbox").exists());
        assert!(!temp.path().join(".agents").exists());
    }
}

#[test]
fn an_up_to_date_update_is_a_noop() {
    let temp = tempfile::tempdir().unwrap();
    let json = format!("{{\"tag_name\":\"v{}\"}}", env!("CARGO_PKG_VERSION"));
    let output = command(&temp, &json)
        .args(["update", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["available"],
        false
    );
    assert!(!temp.path().join(".ai-toolbox").exists());
}

#[test]
fn a_bad_release_response_exits_nonzero_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let output = command(&temp, "rate limited")
        .args(["update", "--check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid GitHub release response"));
    assert!(!temp.path().join(".ai-toolbox").exists());
}

#[test]
fn a_release_that_changes_after_the_check_requires_another_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = command(&temp, "unused");
    fs::write(
        temp.path().join("tools/curl"),
        r#"#!/bin/sh
if [ -f "$HOME/checked" ]; then
  echo '{"tag_name":"v100.0.0"}'
else
  : > "$HOME/checked"
  echo '{"tag_name":"v99.0.0"}'
fi
"#,
    )
    .unwrap();
    let output = command.arg("update").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("latest release changed"));
    assert!(!temp.path().join(".ai-toolbox/clone").exists());
}

#[test]
fn a_concurrent_update_is_refused_before_starting_the_installer() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join(".ai-toolbox")).unwrap();
    let lock = fs::File::create(temp.path().join(".ai-toolbox/update.lock")).unwrap();
    lock.lock().unwrap();
    let output = command(&temp, r#"{"tag_name":"v99.0.0"}"#)
        .args(["update"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("another ai-toolbox update is running")
    );
}
