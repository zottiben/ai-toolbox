//! Reading a repo's stack, and turning that into a recommendation.
//!
//! Ported from `compute_reco()`. The rules are unchanged - what changes is that the
//! answer is a value rather than four shell arrays, so the GUI can show why something is
//! being suggested instead of just suggesting it.

use std::path::Path;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Recommendation {
    /// What was found, in the order it was looked for. Empty means nothing matched.
    pub detected: Vec<Stack>,
    pub hooks: Vec<String>,
    pub mcp: Vec<String>,
    pub skills: Vec<String>,
    /// Rule snippets to paste into AGENTS.md. Never written by the installer - they are
    /// seeds for a human to edit, and always have been.
    pub rules: Vec<String>,
    /// Things worth saying that are not an install: the Unity MCP configures itself, for
    /// one.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stack {
    Node,
    React,
    Vue,
    Expo,
    Supabase,
    Fly,
    GitHub,
    Go,
    Laravel,
    Unity,
}

impl Stack {
    pub fn label(self) -> &'static str {
        match self {
            Stack::Node => "node/ts",
            Stack::React => "react",
            Stack::Vue => "vue",
            Stack::Expo => "expo",
            Stack::Supabase => "supabase",
            Stack::Fly => "fly",
            Stack::GitHub => "github",
            Stack::Go => "go",
            Stack::Laravel => "laravel",
            Stack::Unity => "unity",
        }
    }
}

/// The four hooks every repo gets, whatever it is written in.
const BASELINE_HOOKS: [&str; 4] = [
    "format-on-edit",
    "session-context",
    "guard-irreversible",
    "conventional-commit",
];
const BASELINE_SKILLS: [&str; 4] = ["pre-pr", "capture", "lint", "handoff"];

pub fn recommend(repo: &Path) -> Recommendation {
    let mut rec = Recommendation {
        hooks: BASELINE_HOOKS.iter().map(|s| s.to_string()).collect(),
        mcp: vec!["context7".to_string()],
        skills: BASELINE_SKILLS.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    };
    let package = std::fs::read_to_string(repo.join("package.json")).unwrap_or_default();
    let has_package = repo.join("package.json").is_file();
    // Matching the raw text rather than parsing is what the bash does, and it is the
    // right call here: a dependency can sit in `dependencies`, `devDependencies`,
    // `peerDependencies` or an override, and all of them mean the same thing for this.
    let depends_on = |name: &str| package.contains(&format!("\"{name}\""));

    if has_package {
        rec.detected.push(Stack::Node);
        rec.rules.push("typescript".to_string());

        if depends_on("react") || depends_on("next") {
            rec.detected.push(Stack::React);
            rec.rules.push("react".to_string());
            rec.browser_tooling();
        }
        if depends_on("vue") {
            rec.detected.push(Stack::Vue);
            rec.rules.push("vue".to_string());
            rec.browser_tooling();
        }
        if ["app.json", "app.config.js", "app.config.ts"]
            .iter()
            .any(|f| repo.join(f).exists())
            || depends_on("expo")
        {
            rec.detected.push(Stack::Expo);
            rec.rules.push("expo".to_string());
            rec.mcp.push("expo".to_string());
            rec.skills.push("cli/eas".to_string());
        }
    }

    if repo.join("supabase").exists() || depends_on("@supabase/supabase-js") {
        rec.detected.push(Stack::Supabase);
        rec.mcp.push("supabase".to_string());
        rec.skills.push("cli/supabase".to_string());
        rec.rules.push("supabase".to_string());
    }
    if repo.join("fly.toml").exists() {
        rec.detected.push(Stack::Fly);
        rec.skills.push("cli/fly".to_string());
    }
    if repo.join(".github").exists() {
        rec.detected.push(Stack::GitHub);
        rec.skills.push("cli/gh".to_string());
    }
    if repo.join("go.mod").exists() {
        rec.detected.push(Stack::Go);
        rec.rules.push("go".to_string());
    }
    if std::fs::read_to_string(repo.join("composer.json"))
        .is_ok_and(|text| text.contains("laravel"))
    {
        rec.detected.push(Stack::Laravel);
        rec.rules.push("laravel".to_string());
    }

    let mut generated = false;
    if repo.join("ProjectSettings/ProjectVersion.txt").exists() {
        rec.detected.push(Stack::Unity);
        rec.rules.push("unity".to_string());
        rec.notes.push(
            "Unity: the unity MCP self-configures (Window -> MCP for Unity -> Configure) - no preset."
                .to_string(),
        );
        generated = true;
    }
    if ["codegen.yml", "codegen.ts", "buf.gen.yaml", "openapi.yaml"]
        .iter()
        .any(|f| repo.join(f).exists())
    {
        generated = true;
    }
    if generated {
        rec.hooks.push("protect-generated".to_string());
    }

    rec.dedup();
    rec
}

impl Recommendation {
    /// A repo with a browser UI wants the same two servers and the demo skill, whichever
    /// framework put it there.
    fn browser_tooling(&mut self) {
        self.mcp.push("chrome-devtools".to_string());
        self.mcp.push("playwright".to_string());
        self.skills.push("screen-record-demo".to_string());
    }

    /// React and Vue in one repo would otherwise ask for playwright twice.
    fn dedup(&mut self) {
        for list in [
            &mut self.hooks,
            &mut self.mcp,
            &mut self.skills,
            &mut self.rules,
        ] {
            let mut seen = Vec::new();
            list.retain(|item| {
                let fresh = !seen.contains(item);
                if fresh {
                    seen.push(item.clone());
                }
                fresh
            });
        }
    }

    pub fn detected_labels(&self) -> Vec<&'static str> {
        self.detected.iter().map(|s| s.label()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, contents) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, contents).unwrap();
        }
        dir
    }

    #[test]
    fn an_unrecognised_repo_still_gets_the_baseline() {
        let repo = repo_with(&[]);
        let rec = recommend(repo.path());
        assert!(rec.detected.is_empty());
        assert_eq!(rec.hooks.len(), BASELINE_HOOKS.len());
        assert_eq!(rec.mcp, vec!["context7".to_string()]);
        assert!(rec.rules.is_empty());
    }

    #[test]
    fn a_react_repo_asks_for_browser_tooling() {
        let repo = repo_with(&[("package.json", r#"{"dependencies":{"react":"19"}}"#)]);
        let rec = recommend(repo.path());
        assert_eq!(rec.detected, vec![Stack::Node, Stack::React]);
        assert!(rec.mcp.contains(&"playwright".to_string()));
        assert!(rec.skills.contains(&"screen-record-demo".to_string()));
        assert!(rec.rules.contains(&"react".to_string()));
    }

    #[test]
    fn react_and_vue_together_do_not_ask_for_playwright_twice() {
        let repo = repo_with(&[(
            "package.json",
            r#"{"dependencies":{"react":"19","vue":"3"}}"#,
        )]);
        let rec = recommend(repo.path());
        assert_eq!(
            rec.mcp.iter().filter(|m| *m == "playwright").count(),
            1,
            "got {:?}",
            rec.mcp
        );
        assert_eq!(
            rec.skills
                .iter()
                .filter(|s| *s == "screen-record-demo")
                .count(),
            1
        );
    }

    #[test]
    fn a_dev_dependency_counts_the_same_as_a_dependency() {
        let repo = repo_with(&[("package.json", r#"{"devDependencies":{"expo":"52"}}"#)]);
        assert!(recommend(repo.path()).detected.contains(&Stack::Expo));
    }

    #[test]
    fn a_codegen_config_adds_the_generated_file_guard() {
        let plain = repo_with(&[("go.mod", "module x")]);
        assert!(!recommend(plain.path())
            .hooks
            .contains(&"protect-generated".to_string()));

        let generated = repo_with(&[("go.mod", "module x"), ("buf.gen.yaml", "version: v1")]);
        assert!(recommend(generated.path())
            .hooks
            .contains(&"protect-generated".to_string()));
    }

    #[test]
    fn unity_is_detected_and_explains_why_it_has_no_preset() {
        let repo = repo_with(&[(
            "ProjectSettings/ProjectVersion.txt",
            "m_EditorVersion: 6000",
        )]);
        let rec = recommend(repo.path());
        assert!(rec.detected.contains(&Stack::Unity));
        assert!(rec.hooks.contains(&"protect-generated".to_string()));
        assert_eq!(rec.notes.len(), 1);
        assert!(!rec.mcp.contains(&"unity".to_string()));
    }

    #[test]
    fn composer_json_without_laravel_is_not_laravel() {
        let repo = repo_with(&[("composer.json", r#"{"require":{"symfony/console":"6"}}"#)]);
        assert!(!recommend(repo.path()).detected.contains(&Stack::Laravel));
    }

    #[test]
    fn everything_recommended_exists_in_the_catalogue() {
        // The recommendation is only useful if it names things that can be installed;
        // a typo here would fail at install time instead of at test time.
        let catalogue = crate::testing::catalogue();
        let repo = repo_with(&[
            (
                "package.json",
                r#"{"dependencies":{"react":"19","expo":"52","@supabase/supabase-js":"2"}}"#,
            ),
            ("go.mod", "module x"),
            ("fly.toml", "app = 'x'"),
            (".github/workflows/ci.yml", "on: push"),
            ("composer.json", r#"{"require":{"laravel/framework":"11"}}"#),
            (
                "ProjectSettings/ProjectVersion.txt",
                "m_EditorVersion: 6000",
            ),
            ("codegen.ts", "export default {}"),
        ]);
        let rec = recommend(repo.path());

        for hook in &rec.hooks {
            assert!(catalogue.hook(hook).is_some(), "no such hook: {hook}");
        }
        for preset in &rec.mcp {
            assert!(
                catalogue.preset(preset).is_some(),
                "no such preset: {preset}"
            );
        }
        for skill in &rec.skills {
            assert!(catalogue.skill(skill).is_some(), "no such skill: {skill}");
        }
        for rule in &rec.rules {
            assert!(catalogue.rule(rule).is_some(), "no such rule: {rule}");
        }
    }
}
