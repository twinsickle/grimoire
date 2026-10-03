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
