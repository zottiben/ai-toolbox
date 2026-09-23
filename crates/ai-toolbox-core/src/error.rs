use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Always says which path, because "No such file or directory" on its own has sent
    /// more than one person looking in the wrong repo.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Reading a config with comments in it is fine - see [`crate::jsonc`]. Merging into
    /// one is not, because a merge re-serialises the whole document and would drop every
    /// comment it contains.
    #[error("{path}: has comments, which a merge would delete. Remove them, or make this change by hand.")]
    JsonComments { path: PathBuf },

    #[error("{path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("{path}: {source}")]
    Db {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },

    #[error("{0}")]
    Catalogue(String),
}

impl Error {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn json(path: impl AsRef<Path>, source: serde_json::Error) -> Error {
        Error::Json {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn json_comments(path: impl AsRef<Path>) -> Error {
        Error::JsonComments {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn toml(path: impl AsRef<Path>, source: toml::de::Error) -> Error {
        Error::Toml {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn db(path: impl AsRef<Path>, source: rusqlite::Error) -> Error {
        Error::Db {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}
