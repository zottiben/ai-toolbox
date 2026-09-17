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

    #[error("{path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
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

    pub fn toml(path: impl AsRef<Path>, source: toml::de::Error) -> Error {
        Error::Toml {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}
