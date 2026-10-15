//! Read-only MCP inspection and explicitly approved exposure changes. Never starts a server.
mod cache;
mod config;
mod detect;
mod optimise;

pub(crate) use config::install_choices;
pub use config::{inspect, Backend, Context, Report, Server};
pub use optimise::{apply, preview, preview_read_only, rollback, Applied, Options, Preview, Scope};

#[cfg(test)]
mod accuracy_tests;
#[cfg(test)]
mod legacy_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;
