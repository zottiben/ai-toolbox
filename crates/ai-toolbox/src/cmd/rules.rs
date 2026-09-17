//! `ai-toolbox rules` - print stack snippets to stdout.
//!
//! The one command that deliberately writes nothing. These are seeds for a human to
//! paste the applicable parts of into AGENTS.md and delete the rest; a tool that
//! appended them wholesale would produce a file nobody had read.

use ai_toolbox_core::error::Error;
use ai_toolbox_core::Catalogue;

use crate::out;

pub fn run(catalogue: &Catalogue, names: &[String]) -> anyhow::Result<()> {
    for name in names {
        let rule = catalogue
            .rule(name.trim_end_matches(".md"))
            .ok_or_else(|| {
                Error::Catalogue(format!(
                    "no such rule snippet: {name} (see 'ai-toolbox list')"
                ))
            })?;
        println!(
            "\n# ===== {} (starters/rules/{}.md) =====\n",
            rule.name, rule.name
        );
        print!("{}", std::fs::read_to_string(&rule.path)?);
    }
    out::info(
        "Snippets are seeds - paste the parts that apply into AGENTS.md, delete the rest. Nothing was written.",
    );
    Ok(())
}
