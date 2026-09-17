//! What the ai-toolbox clone offers, and what a repo has taken from it.
//!
//! One engine (D1). The CLI, the board's server and the desktop app all call this crate,
//! so there is no second implementation of the merge rules to drift from the first.
//!
//! The shape of it:
//!
//! - [`catalogue`] reads the clone - hooks, MCP presets, skills, rule snippets. Read off
//!   disk every time, because a user dropping their own hook in and seeing it appear is
//!   the point (D2).
//! - [`inventory`] reads a repo. Facts only, no judgement, no catalogue.
//! - [`classify`] holds one against the other and says, of every item, whether it is the
//!   catalogue's copy, an edited copy, someone's own, or broken (D5).
//! - [`detect`] reads a repo's stack and recommends what to install.
//! - [`machine`] covers the once-per-machine facts that are easy to mistake for repo
//!   state: the global charters, and whether Pi can speak MCP.
//!
//! Writing is a second step, never a side effect of asking a question (D3):
//!
//! - [`convert`] turns a preset authored once, in Claude Code's shape, into what Codex
//!   and Pi each need.
//! - [`merge`] adds to files the user also owns, by key, never rewriting the document.
//! - [`install`] plans, returning [`action::Action`]s that [`action::apply`] performs.
//! - [`doctor`] names what is broken and plans the repairs, using [`history`] to tell an
//!   item that is merely out of date from one someone edited here.
//! - [`worktree`] keeps every worktree of a repo configured the same way, which git
//!   cannot do for it because all of these files are ignored.
//! - [`registry`] remembers which repos exist, and nothing else about them (D4).

pub mod action;
pub mod catalogue;
pub mod classify;
pub mod convert;
pub mod detect;
pub mod doctor;
pub mod error;
pub mod git;
pub mod harness;
pub mod hash;
pub mod history;
pub mod install;
pub mod inventory;
pub mod machine;
pub mod merge;
pub mod paths;
pub mod registry;
pub mod root;
pub mod secrets;
pub mod worktree;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use action::{apply, Action, Outcome};
pub use catalogue::Catalogue;
pub use classify::{classify, state, Kind, Origin, Report, State};
pub use doctor::{diagnose, Finding, Severity};
pub use error::{Error, Result};
pub use git::Worktree;
pub use harness::Harness;
pub use install::Plan;
pub use inventory::Inventory;
pub use machine::Machine;
pub use registry::{Registry, Summary};

use std::path::Path;

/// Everything worth knowing about one repo, in one call - the shape both `status` and
/// the board's project endpoint want.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Survey {
    pub inventory: Inventory,
    pub report: Report,
    pub state: State,
    pub harnesses: Vec<Harness>,
    pub recommendation: detect::Recommendation,
}

pub fn survey(repo: impl AsRef<Path>, catalogue: &Catalogue) -> Result<Survey> {
    let repo = repo.as_ref();
    let inventory = Inventory::read(repo)?;
    let report = classify(&inventory, catalogue);
    Ok(Survey {
        state: state(&inventory, &report),
        harnesses: harness::configured(repo),
        recommendation: detect::recommend(repo),
        inventory,
        report,
    })
}
