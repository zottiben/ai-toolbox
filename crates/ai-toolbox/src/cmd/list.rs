//! `ai-toolbox list` - everything the catalogue offers.
//!
//! Read from the clone every time (D2), so a hook someone dropped in this morning shows
//! up this morning.

use ai_toolbox_core::Catalogue;

use crate::out;

pub fn run(catalogue: &Catalogue, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(catalogue)?);
        return Ok(());
    }

    out::heading("hooks");
    for hook in &catalogue.hooks {
        entry(&hook.name, hook.summary.as_deref());
    }

    out::heading("mcp presets");
    for preset in &catalogue.presets {
        // What a preset installs is not always its own name - `supabase-multi-env` puts
        // two differently-named servers in, and finding that out after the fact is
        // worse than reading it here.
        let servers: Vec<&str> = preset.servers.keys().map(String::as_str).collect();
        let summary = (servers != [preset.name.as_str()]).then(|| servers.join(", "));
        entry(&preset.name, summary.as_deref());
    }

    out::heading("skills");
    for skill in &catalogue.skills {
        entry(&skill.key, skill.description.as_deref());
    }

    out::heading("rules (inline into AGENTS.md)");
    for rule in &catalogue.rules {
        entry(&rule.name, None);
    }

    Ok(())
}

/// One line per item. The description is trimmed rather than wrapped: this is a list to
/// scan, and a paragraph per skill would stop it being one.
fn entry(name: &str, summary: Option<&str>) {
    match summary {
        Some(text) => println!("  {:<22} {}", name, out::dim(&truncate(text, 70))),
        None => println!("  {name}"),
    }
}

fn truncate(text: &str, width: usize) -> String {
    let text = text.split(". ").next().unwrap_or(text).trim();
    if text.chars().count() <= width {
        return text.to_string();
    }
    let cut: String = text.chars().take(width).collect();
    // Break on a word so the ellipsis does not land mid-word.
    let cut = cut.rsplit_once(' ').map(|(head, _)| head).unwrap_or(&cut);
    format!("{cut}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_cuts_at_the_first_sentence() {
        assert_eq!(
            truncate("Do the thing. Then do another thing entirely.", 70),
            "Do the thing"
        );
    }

    #[test]
    fn truncate_breaks_on_a_word_boundary() {
        let long = "alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo";
        let short = truncate(long, 20);
        assert!(short.ends_with("..."), "got {short}");
        assert!(long.starts_with(short.trim_end_matches("...")));
    }

    #[test]
    fn truncate_leaves_a_short_description_alone() {
        assert_eq!(truncate("short one", 70), "short one");
    }
}
