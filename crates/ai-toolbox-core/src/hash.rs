//! Content hashes, the basis of every "is this still the catalogue's copy?" answer (D5).
//!
//! Two properties matter more than the algorithm. A hash must cover everything that
//! changes behaviour and nothing that does not, or the tool cries wolf; and a directory
//! must hash the same wherever it sits, or a skill would read as modified purely for
//! having been copied.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Short hex, like git's. Long enough that a collision is not a practical concern for a
/// few hundred files, short enough to sit in a table without wrapping.
const WIDTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct Hash(String);

impl Hash {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn finish(hasher: Sha256) -> Hash {
    let digest = hasher.finalize();
    Hash(
        digest
            .iter()
            .take(WIDTH / 2)
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

pub fn bytes(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    finish(hasher)
}

pub fn file(path: impl AsRef<Path>) -> Result<Hash> {
    let path = path.as_ref();
    let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(bytes(&data))
}

/// Hash a whole directory: every file under it, keyed by its path relative to the root.
///
/// The relative path is what makes this portable - `skills/pre-pr` in the clone and
/// `.agents/skills/pre-pr` in a repo are the same tree, and have to hash the same or
/// every installed skill would report as modified. Paths are fed to the hasher
/// explicitly so that renaming a file inside a skill changes the hash, which hashing
/// the concatenated contents alone would miss.
pub fn dir(root: impl AsRef<Path>) -> Result<Hash> {
    let root = root.as_ref();
    let mut entries = Vec::new();
    collect(root, root, &mut entries)?;
    // Sorted, so the hash does not depend on the order the filesystem hands them back.
    entries.sort();
    Ok(from_entries(&entries))
}

/// Combine `(path relative to the root, hash of that file)` pairs into one hash.
///
/// Shared with [`crate::history`], which reconstructs the same pairs out of a git tree.
/// The two have to agree exactly or an item read from history could never match the same
/// item read from disk - so there is one implementation, not two that look alike.
/// Entries must already be sorted.
pub fn from_entries(entries: &[(String, Hash)]) -> Hash {
    let mut hasher = Sha256::new();
    for (relative, digest) in entries {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(digest.as_str().as_bytes());
        hasher.update([0]);
    }
    finish(hasher)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Hash)>) -> Result<()> {
    let read = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in read {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| Error::io(&path, e))?;
        if kind.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        // A symlink inside a skill is hashed by where it points, not by its target
        // bytes, so a link and a copy of the same file stay distinguishable.
        if kind.is_symlink() {
            let target = std::fs::read_link(&path).map_err(|e| Error::io(&path, e))?;
            out.push((
                relative(root, &path),
                bytes(target.as_os_str().as_encoded_bytes()),
            ));
            continue;
        }
        out.push((relative(root, &path), file(&path)?));
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Hash a JSON value by its canonical form rather than its formatting, so that
/// reindenting `.mcp.json` - which every tool that touches it does differently - is not
/// mistaken for someone changing a server definition.
pub fn json(value: &serde_json::Value) -> Hash {
    let mut canonical = String::new();
    write_canonical(value, &mut canonical);
    bytes(canonical.as_bytes())
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    use std::fmt::Write;
    match value {
        serde_json::Value::Object(map) => {
            // Sorted keys: serde_json preserves insertion order for us elsewhere, which
            // is what we want when writing files, and exactly what we do not want here.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{key:?}:");
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => {
            let _ = write!(out, "{other}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_hashes_the_same_wherever_it_is() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for root in [a.path(), b.path()] {
            std::fs::create_dir_all(root.join("nested")).unwrap();
            std::fs::write(root.join("SKILL.md"), "# a skill").unwrap();
            std::fs::write(root.join("nested/helper.js"), "console.log(1)").unwrap();
        }
        assert_eq!(dir(a.path()).unwrap(), dir(b.path()).unwrap());
    }

    #[test]
    fn renaming_a_file_inside_a_directory_changes_its_hash() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("SKILL.md"), "body").unwrap();
        let before = dir(root.path()).unwrap();
        std::fs::rename(root.path().join("SKILL.md"), root.path().join("OTHER.md")).unwrap();
        assert_ne!(before, dir(root.path()).unwrap());
    }

    #[test]
    fn json_ignores_formatting_and_key_order_but_not_values() {
        let compact: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":[2,3]}"#).unwrap();
        let pretty: serde_json::Value =
            serde_json::from_str("{\n  \"a\": [\n    2,\n    3\n  ],\n  \"b\": 1\n}").unwrap();
        assert_eq!(json(&compact), json(&pretty));

        let changed: serde_json::Value = serde_json::from_str(r#"{"a":[2,4],"b":1}"#).unwrap();
        assert_ne!(json(&compact), json(&changed));
    }

    #[test]
    fn json_distinguishes_nested_objects_that_share_a_flattening() {
        let a: serde_json::Value = serde_json::from_str(r#"{"x":{"y":1}}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"x":{},"y":1}"#).unwrap();
        assert_ne!(json(&a), json(&b));
    }
}
