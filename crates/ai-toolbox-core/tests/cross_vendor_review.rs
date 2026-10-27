use ai_toolbox_core::{action, install, testing, Harness};

#[test]
fn cross_vendor_review_installs_its_helper_for_every_harness() {
    let catalogue = testing::catalogue();
    let skill = catalogue.skill("cross-vendor-review").unwrap();
    assert_eq!(skill.name, "cross-vendor-review");

    for harness in [Harness::Claude, Harness::Codex, Harness::Pi] {
        let temp = tempfile::tempdir().unwrap();
        let plan = install::everything(
            temp.path(),
            &catalogue,
            &[harness],
            &[],
            &[],
            std::slice::from_ref(&skill.key),
            true,
        )
        .unwrap();
        action::apply(&plan.actions).unwrap();

        for file in [
            "SKILL.md",
            "scripts/review.mjs",
            "scripts/selection.mjs",
            "references/assessment.md",
        ] {
            let installed = temp
                .path()
                .join(".agents/skills/cross-vendor-review")
                .join(file);
            assert_eq!(
                std::fs::read(installed).unwrap(),
                std::fs::read(skill.path.join(file)).unwrap()
            );
            if harness == Harness::Claude {
                assert_eq!(
                    std::fs::read(
                        temp.path()
                            .join(".claude/skills/cross-vendor-review")
                            .join(file)
                    )
                    .unwrap(),
                    std::fs::read(skill.path.join(file)).unwrap()
                );
            }
        }
    }
}
