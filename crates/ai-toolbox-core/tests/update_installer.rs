//! Exercise the actual shipped/embedded installer with local release archives. Only
//! curl and uname are fakes; extraction, checksums, staging and renames are real.
//! HOME, binary and app destinations are always inside the fixture, never the machine.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};

struct Fixture {
    temp: tempfile::TempDir,
    archive: String,
}

fn script(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Fixture {
    fn new(os: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for dir in [
            "tools",
            "bin",
            "stage",
            "releases",
            "home",
            "apps/ai-toolbox.app",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        script(&root.join("stage/ai-toolbox"), "#!/bin/sh\n[ \"$1\" = --version ] && echo 'ai-toolbox 9.8.7' || echo '  catalogue item'\n");
        fs::write(root.join("bin/ai-toolbox"), "old binary").unwrap();
        fs::hard_link(root.join("bin/ai-toolbox"), root.join("old-inode")).unwrap();
        fs::write(root.join("apps/ai-toolbox.app/old"), "old desktop").unwrap();
        let platform = if os == "Darwin" {
            "macos-universal"
        } else {
            "linux-x86_64"
        };
        let archive = format!("ai-toolbox-v9.8.7-{platform}.tar.gz");
        if os == "Darwin" {
            fs::create_dir_all(root.join("stage/ai-toolbox.app/Contents/MacOS")).unwrap();
            script(
                &root.join("stage/ai-toolbox.app/Contents/MacOS/ai-toolbox-desktop"),
                "#!/bin/sh\nexit 0\n",
            );
        }
        let result = Command::new("tar")
            .args(["czf"])
            .arg(root.join("releases").join(&archive))
            .arg("-C")
            .arg(root.join("stage"))
            .arg(".")
            .output()
            .unwrap();
        assert!(result.status.success());
        let digest = Sha256::digest(fs::read(root.join("releases").join(&archive)).unwrap());
        fs::write(
            root.join("releases/checksums.txt"),
            format!("{digest:x}  {archive}\n"),
        )
        .unwrap();
        script(
            &root.join("tools/uname"),
            &format!("#!/bin/sh\n[ \"$1\" = -s ] && echo {os} || echo x86_64\n"),
        );
        script(
            &root.join("tools/curl"),
            r#"#!/bin/sh
url=""; out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    https://*) url="$1" ;;
    -o) shift; out="$1" ;;
  esac
  shift
done
printf '%s\n' "$url" >> "$FIXTURE/requests"
case "$url" in
  https://github.com/zottiben/ai-toolbox/releases/download/v9.8.7/*)
    cp "$FIXTURE/releases/${url##*/}" "$out" ;;
  *) echo "unexpected network request: $url" >&2; exit 1 ;;
esac
"#,
        );
        // An accidental source fallback or privilege prompt is a failure, not a way for
        // the test to pass because cargo happens to be installed on this machine.
        for tool in ["cargo", "sudo", "git"] {
            script(
                &root.join("tools").join(tool),
                "#!/bin/sh\necho 'unexpected command' >&2\nexit 99\n",
            );
        }
        Self { temp, archive }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }

    fn command(&self) -> Command {
        let mut command = self.command_with_catalogue();
        command.arg("--no-catalogue");
        command
    }

    fn command_with_catalogue(&self) -> Command {
        let mut command = Command::new("sh");
        command
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install/install.sh"))
            .args(["--version", "v9.8.7", "--non-interactive", "--bin-dir"])
            .arg(self.path("bin"))
            .arg("--app-dir")
            .arg(self.path("apps"))
            .env("HOME", self.path("home"))
            .env("FIXTURE", self.temp.path())
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                    self.path("tools").display()
                ),
            );
        command
    }

    fn run(&self) -> Output {
        self.command().output().unwrap()
    }

    fn repack(&self) {
        let archive = self.path("releases").join(&self.archive);
        assert!(Command::new("tar")
            .arg("czf")
            .arg(&archive)
            .arg("-C")
            .arg(self.path("stage"))
            .arg(".")
            .status()
            .unwrap()
            .success());
        let digest = Sha256::digest(fs::read(&archive).unwrap());
        fs::write(
            self.path("releases/checksums.txt"),
            format!("{digest:x}  {}\n", self.archive),
        )
        .unwrap();
    }

    fn unchanged(&self) {
        assert_eq!(
            fs::read_to_string(self.path("bin/ai-toolbox")).unwrap(),
            "old binary"
        );
        assert_eq!(
            fs::read_to_string(self.path("apps/ai-toolbox.app/old")).unwrap(),
            "old desktop"
        );
    }
}

fn log(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn pinned_linux_release_is_verified_and_atomically_replaces_only_the_selected_binary() {
    let fixture = Fixture::new("Linux");
    let output = fixture.run();
    assert!(output.status.success(), "{}", log(&output));
    assert_eq!(
        fs::read_to_string(fixture.path("old-inode")).unwrap(),
        "old binary",
        "must rename, not truncate the running inode"
    );
    assert!(fs::read_to_string(fixture.path("bin/ai-toolbox"))
        .unwrap()
        .contains("9.8.7"));
    assert!(fixture.path("apps/ai-toolbox.app/old").is_file());
    let requests = fs::read_to_string(fixture.path("requests")).unwrap();
    assert!(
        !requests.contains("latest"),
        "the confirmed version must not change during installation"
    );
    assert!(!fixture.path("home/.ai-toolbox").exists());
}

#[test]
fn macos_app_and_cli_are_replaced_together() {
    let fixture = Fixture::new("Darwin");
    let output = fixture.run();
    assert!(output.status.success(), "{}", log(&output));
    assert!(fixture
        .path("apps/ai-toolbox.app/Contents/MacOS/ai-toolbox-desktop")
        .is_file());
    assert!(!fixture.path("apps/ai-toolbox.app/old").exists());
    assert!(!fixture.path("apps/ai-toolbox.app.pre-update").exists());
}

#[test]
fn bad_missing_and_unlisted_checksums_leave_both_installed_copies_untouched() {
    for checksum in [
        None,
        Some("not a checksum"),
        Some("000000  unrelated.tar.gz"),
    ] {
        let fixture = Fixture::new("Darwin");
        if let Some(value) = checksum {
            fs::write(fixture.path("releases/checksums.txt"), value).unwrap();
        } else {
            fs::remove_file(fixture.path("releases/checksums.txt")).unwrap();
        }
        let output = fixture.run();
        assert!(!output.status.success(), "{}", log(&output));
        fixture.unchanged();
    }
    let fixture = Fixture::new("Linux");
    fs::write(
        fixture.path(&format!("releases/{}", fixture.archive)),
        "tampered",
    )
    .unwrap();
    let output = fixture.run();
    assert!(log(&output).contains("checksum mismatch"));
    fixture.unchanged();
}

#[test]
fn a_verified_archive_with_the_wrong_binary_version_is_not_installed() {
    let fixture = Fixture::new("Linux");
    script(
        &fixture.path("stage/ai-toolbox"),
        "#!/bin/sh\necho 'ai-toolbox 0.0.1'\n",
    );
    fixture.repack();
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(log(&output).contains("wrong version"));
    fixture.unchanged();
}

#[test]
fn a_macos_release_missing_its_app_cannot_claim_to_update_both() {
    let fixture = Fixture::new("Darwin");
    fs::remove_dir_all(fixture.path("stage/ai-toolbox.app")).unwrap();
    fixture.repack();
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(log(&output).contains("missing the desktop app"));
    fixture.unchanged();
}

#[test]
fn a_failed_download_never_falls_back_to_a_source_build() {
    let fixture = Fixture::new("Linux");
    fs::remove_file(fixture.path(&format!("releases/{}", fixture.archive))).unwrap();
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(
        log(&output).contains("no source fallback"),
        "{}",
        log(&output)
    );
    assert!(!log(&output).contains("unexpected command"));
    fixture.unchanged();
}

#[test]
fn app_staging_failure_leaves_the_cli_untouched() {
    let fixture = Fixture::new("Darwin");
    fs::write(fixture.path("not-a-directory"), "file").unwrap();
    let output = fixture
        .command()
        .arg("--app-dir")
        .arg(fixture.path("not-a-directory"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    fixture.unchanged();
}

#[test]
fn a_failed_binary_swap_restores_the_previous_app() {
    let fixture = Fixture::new("Darwin");
    script(
        &fixture.path("tools/mv"),
        r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "$FIXTURE/bin/ai-toolbox" ]; then
    echo 'simulated rename failure' >&2
    exit 1
  fi
done
exec /bin/mv "$@"
"#,
    );
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(log(&output).contains("previous installation preserved"));
    fixture.unchanged();
}

#[test]
fn a_failed_catalogue_fast_forward_preserves_local_edits_and_reports_a_warning() {
    let fixture = Fixture::new("Linux");
    let clone = fixture.path("home/.ai-toolbox/clone");
    fs::create_dir_all(clone.join(".git")).unwrap();
    fs::write(clone.join("my-hook.sh"), "my local edit").unwrap();
    script(
        &fixture.path("tools/git"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$FIXTURE/git-args\"\nexit 1\n",
    );
    let output = fixture.command_with_catalogue().output().unwrap();
    assert!(output.status.success(), "{}", log(&output));
    assert!(log(&output).contains("could not fast-forward"));
    assert!(fs::read_to_string(fixture.path("git-args"))
        .unwrap()
        .contains("pull --ff-only"));
    assert_eq!(
        fs::read_to_string(clone.join("my-hook.sh")).unwrap(),
        "my local edit"
    );
}

#[test]
fn an_invalid_catalogue_destination_is_rejected_before_replacing_the_binary() {
    let fixture = Fixture::new("Linux");
    fs::create_dir_all(fixture.path("home/.ai-toolbox/clone")).unwrap();
    let output = fixture.command_with_catalogue().output().unwrap();
    assert!(!output.status.success());
    assert!(log(&output).contains("is not a git clone"));
    fixture.unchanged();
}

#[test]
fn an_existing_app_backup_is_never_deleted_to_make_room() {
    let fixture = Fixture::new("Darwin");
    fs::create_dir(fixture.path("apps/ai-toolbox.app.pre-update")).unwrap();
    fs::write(
        fixture.path("apps/ai-toolbox.app.pre-update/precious"),
        "backup",
    )
    .unwrap();
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(fixture
        .path("apps/ai-toolbox.app.pre-update/precious")
        .is_file());
    fixture.unchanged();
}
