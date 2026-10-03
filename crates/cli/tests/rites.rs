//! The rites of this repository that code can hold. A rule people follow is
//! held here only as far as a test can see it: that the rule is written
//! where every session reads it, with the procedure that runs it.
#![allow(clippy::unwrap_used)]

use std::path::Path;

/// This repository's own guidance in `file`, below the marked section `init` owns.
fn own_guidance(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    let after_marker = text.split("<!-- /grimoire -->").nth(1).expect("the guidance section ends with its marker");
    after_marker.to_string()
}

/// # Test contract
///
/// ```json
/// {"test": "the-review-rule-is-installed-where-every-session-reads-it", "checks": ["req:work-reviewed-before-close"]}
/// ```
#[test]
fn the_review_rule_is_installed_where_every_session_reads_it() {
    // The review rule is held by people, not by code; the most a test can
    // check is that the procedure is where every session reads it. It names
    // the rites' review page as the rule, a read-only reviewer other than the
    // one who did the work, and that the closing decision says what was found.
    // Every session reads one of the two files, so the procedure is in both.
    for file in ["CLAUDE.md", "AGENTS.md"] {
        let own = own_guidance(file);
        let bullet = own.split("\n- ").find(|b| b.starts_with("Review before close")).unwrap_or_else(|| panic!("{file} carries a Review before close bullet below init's marker"));
        for phrase in ["`review` page", "read-only", "codex exec", "closing decision"] {
            assert!(bullet.contains(phrase), "{file}: the review procedure says {phrase:?}:\n{bullet}");
        }
    }
}

/// # Test contract
///
/// ```json
/// {"test": "the-question-rule-is-installed-where-every-session-reads-it", "checks": ["req:question-rooted-before-options"]}
/// ```
#[test]
fn the_question_rule_is_installed_where_every_session_reads_it() {
    // Held by people, like the review rule: a test cannot see whether a
    // question was taken to its root, only that the procedure is where every
    // session reads it. It names the rites' questions page as the rule, the
    // goal page and the assumption beneath the present design as what comes
    // before the options, tooling as never a requirement, the goal's "what it
    // is not" as what a growing design is held against, the work asked ahead
    // of time as part of the measure, and the closing decision as where the
    // root is said.
    for file in ["CLAUDE.md", "AGENTS.md"] {
        let own = own_guidance(file);
        let bullet = own.split("\n- ").find(|b| b.starts_with("Questions start from the goal")).unwrap_or_else(|| panic!("{file} carries a Questions start from the goal bullet below init's marker"));
        // The bullet wraps, so its words are compared with the line breaks taken out.
        let bullet = bullet.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in ["`questions` page", "read the goal page", "name the assumption", "never a requirement", "work it asks ahead of time", "\"what it is not\"", "closing decision"] {
            assert!(bullet.contains(phrase), "{file}: the question procedure says {phrase:?}:\n{bullet}");
        }
    }
}
