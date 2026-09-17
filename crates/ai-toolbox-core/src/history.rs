//! What a catalogue item used to look like.
//!
//! This exists to answer one question, and it is the question that decides whether
//! `doctor --fix` is safe: an installed item differs from the catalogue - did someone
//! edit it here, or has the catalogue simply moved on?
//!
//! Hashing the item at each commit that touched it in the clone answers that. If the
//! installed copy matches a version the catalogue *used* to have, it is out of date and
//! updating it loses nothing. If it matches no version at all, it is someone's own work
//! and must not be overwritten.
//!
//! Only consulted for items that already differ, so the cost is paid on the few rather
//! than on every item in every repo.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::hash::{self, Hash};

/// How far back to look. A catalogue item that has changed more times than this and is
/// still being matched against a very old copy is better reported as modified: the safe
/// answer, and the one that asks a human.
const DEPTH: usize = 40;

/// Every version of `relative` in the clone's history, newest first.
///
/// Returns an empty list rather than an error when git is unavailable or the clone is
/// not a repository - a tarball install has no history, and that is a reason to report
/// "modified" rather than a reason to fail.
pub fn versions(clone: &Path, relative: &str) -> Vec<Hash> {
    let Some(commits) = commits_touching(clone, relative) else {
        return Vec::new();
    };
    let is_dir = clone.join(relative).is_dir();
    commits
        .iter()
        .filter_map(|commit| {
            if is_dir {
                directory_at(clone, commit, relative)
            } else {
                file_at(clone, commit, relative)
            }
        })
        .collect()
}

/// Whether some past version of the catalogue item is what is installed.
pub fn is_former_version(clone: &Path, relative: &str, installed: &Hash) -> bool {
    versions(clone, relative)
        .iter()
        .any(|past| past == installed)
}

fn commits_touching(clone: &Path, relative: &str) -> Option<Vec<String>> {
    let out = git(
        clone,
        &["log", &format!("-n{DEPTH}"), "--format=%H", "--", relative],
    )?;
    Some(
        String::from_utf8_lossy(&out)
            .lines()
            .map(str::to_string)
            .collect(),
    )
}

fn file_at(clone: &Path, commit: &str, relative: &str) -> Option<Hash> {
    let bytes = git(clone, &["show", &format!("{commit}:{relative}")])?;
    Some(hash::bytes(&bytes))
}

/// A directory's hash at a commit, computed the same way [`hash::dir`] computes it for
/// a directory on disk - so the two are comparable, which is the whole point.
fn directory_at(clone: &Path, commit: &str, relative: &str) -> Option<Hash> {
    let listing = git(clone, &["ls-tree", "-r", commit, "--", relative])?;
    let listing = String::from_utf8_lossy(&listing);

    // `<mode> <type> <oid>\t<path>` per line. Only blobs: a submodule or a symlink
    // entry is not something the catalogue ships.
    let mut wanted: Vec<(String, String)> = Vec::new();
    for line in listing.lines() {
        let (meta, path) = line.split_once('\t')?;
        let mut parts = meta.split_whitespace();
        let _mode = parts.next()?;
        if parts.next()? != "blob" {
            continue;
        }
        let oid = parts.next()?;
        // Relative to the item, not to the repo root, matching `hash::dir`.
        let inside = path.strip_prefix(relative)?.trim_start_matches('/');
        wanted.push((inside.to_string(), oid.to_string()));
    }
    if wanted.is_empty() {
        return None;
    }

    let contents = blobs(
        clone,
        &wanted
            .iter()
            .map(|(_, oid)| oid.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut entries: Vec<(String, Hash)> = wanted
        .iter()
        .filter_map(|(path, oid)| Some((path.clone(), hash::bytes(contents.get(oid)?))))
        .collect();
    entries.sort();
    Some(hash::from_entries(&entries))
}

/// Read many blobs in one `git cat-file --batch`, rather than one process per file.
///
/// The batch protocol is `<oid> <type> <size>\n<contents>\n` per request, and the size
/// is what makes it parseable - contents are binary and may hold newlines.
fn blobs(clone: &Path, oids: &[String]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(clone)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let request: String = oids.iter().map(|oid| format!("{oid}\n")).collect();
    // Written on another thread: a large request would otherwise fill the pipe buffer
    // and deadlock against our own read of stdout.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(request.as_bytes());
        let _ = stdin.flush();
    });

    let output = child.wait_with_output().ok()?;
    let _ = writer.join();
    if !output.status.success() {
        return None;
    }

    let mut found = BTreeMap::new();
    let mut rest = output.stdout.as_slice();
    while !rest.is_empty() {
        let newline = rest.iter().position(|b| *b == b'\n')?;
        let header = String::from_utf8_lossy(&rest[..newline]).to_string();
        rest = &rest[newline + 1..];

        let mut parts = header.split_whitespace();
        let oid = parts.next()?.to_string();
        let kind = parts.next();
        if kind != Some("blob") {
            // "<oid> missing" and anything that is not a blob carry no body.
            continue;
        }
        let size: usize = parts.next()?.parse().ok()?;
        if rest.len() < size {
            return None;
        }
        found.insert(oid, rest[..size].to_vec());
        // The body is followed by a newline that is not part of it.
        rest = &rest[(size + 1).min(rest.len())..];
    }
    Some(found)
}

fn git(clone: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(clone)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clone this crate lives in, which has real history for its own catalogue.
    fn clone() -> std::path::PathBuf {
        crate::testing::catalogue_root()
    }

    #[test]
    fn the_current_content_of_a_file_is_among_its_versions() {
        let clone = clone();
        let relative = "hooks/format-on-edit.sh";
        let installed = hash::file(clone.join(relative)).unwrap();
        assert!(
            is_former_version(&clone, relative, &installed),
            "HEAD's own content should match the newest version in history"
        );
    }

    #[test]
    fn the_current_content_of_a_directory_is_among_its_versions() {
        // The directory case is the one with real work in it - reconstructing a tree
        // from git and hashing it the same way the on-disk walk does.
        let clone = clone();
        let relative = "skills/handoff";
        let installed = hash::dir(clone.join(relative)).unwrap();
        assert!(
            is_former_version(&clone, relative, &installed),
            "a directory at HEAD should hash the same from git as from disk"
        );
    }

    #[test]
    fn content_that_was_never_in_the_catalogue_matches_no_version() {
        let clone = clone();
        let invented = hash::bytes(b"#!/usr/bin/env bash\n# nobody ever committed this\n");
        assert!(!is_former_version(
            &clone,
            "hooks/format-on-edit.sh",
            &invented
        ));
    }

    #[test]
    fn an_earlier_version_of_a_changed_item_is_still_recognised() {
        // skills/handoff was updated in a398dd6; the version before it is a former
        // version, and a repo still holding that copy is out of date rather than edited.
        let clone = clone();
        let versions = versions(&clone, "skills/handoff");
        assert!(
            versions.len() >= 2,
            "the fixture relies on handoff having been changed at least once: {versions:?}"
        );
        assert_ne!(
            versions[0], versions[1],
            "two commits, two different contents"
        );
        assert!(is_former_version(&clone, "skills/handoff", &versions[1]));
    }

    #[test]
    fn a_directory_that_is_not_a_repository_has_no_history_rather_than_an_error() {
        let temp = tempfile::tempdir().unwrap();
        assert!(versions(temp.path(), "anything").is_empty());
    }

    #[test]
    fn a_path_the_clone_has_never_heard_of_has_no_versions() {
        assert!(versions(&clone(), "hooks/never-existed.sh").is_empty());
    }
}
