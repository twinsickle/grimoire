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

/// # Test contract
///
/// ```json
/// {"test": "the-seams-rule-is-installed-where-every-session-reads-it", "checks": ["req:large-file-examined-for-seams"]}
/// ```
#[test]
fn the_seams_rule_is_installed_where_every_session_reads_it() {
    // Held by people, as review and questions are: a test cannot see a file examined, only that the
    // procedure is where every session reads it. It names the rites' seams page as the practice, the size as a
    // guide, a generated file examined at its generator, a file with no seam said to have none, the size
    // that calls for it with the tests set aside, the split as a ticket of its own on the person's
    // agreement, and the closing decision as where the examination is said.
    for file in ["CLAUDE.md", "AGENTS.md"] {
        let own = own_guidance(file);
        let bullet = own.split("
- ").find(|b| b.starts_with("Large files are examined for their seams")).unwrap_or_else(|| panic!("{file} carries a Large files are examined bullet below init's marker"));
        let bullet = bullet.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in [
            "`seams` page carries the practice",
            "never by itself for a split",
            "tests aside",
            "at its generator",
            "600 lines or more",
            "1,000",
            "\"and\"",
            "none worth splitting",
            "ticket of its own",
            "user's yes",
            "moves code and changes no behaviour",
            "closing decision",
        ] {
            assert!(bullet.contains(phrase), "{file}: the seams procedure says {phrase:?}:
{bullet}");
        }
    }
}

/// # Test contract
///
/// ```json
/// {"test": "this-repository-is-worked-on-in-the-setup-a-team-uses", "checks": ["req:built-in-the-setup-it-is-used-in"]}
/// ```
#[test]
fn this_repository_is_worked_on_in_the_setup_a_team_uses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap();
    // Its own grimoire is a server's: the project file names one, and no store.
    let project = read(".grimoire.toml");
    let named: Vec<&str> = project.lines().filter(|l| !l.trim_start().starts_with('#') && l.contains('=')).map(|l| l.split('=').next().unwrap().trim()).collect();
    assert_eq!(named, ["server"], "{project}");
    // The loop starts that server, shared, from a copy of the installed build, at the address the
    // project file names, before the daemon that reads from it, and signs its person in.
    let dev = read("scripts/dev.mjs");
    for phrase in ["\"serve\", \"--shared\"", ".grimoire.toml", "cargo install --path crates/cli", "copyFileSync(installed, serverBin)", "[\"token\", \"add\", name,", "[\"login\"]", "refused this machine"] {
        assert!(dev.contains(phrase), "scripts/dev.mjs holds {phrase:?}");
    }
    let (server, rest) = (dev.find("start(server)").unwrap(), dev.find("processes.forEach(start)").unwrap());
    assert!(server < rest, "the server is started before the daemon and the interface");
    // Signing in never takes a token from anyone: the loop revokes none.
    assert!(!dev.contains("\"revoke\""), "scripts/dev.mjs revokes no token");
    // The procedure is where every session reads it: the rites' page as the rule, how a ticket lands here,
    // when a change to the server's side takes effect, and the way back.
    for file in ["CLAUDE.md", "AGENTS.md"] {
        let own = own_guidance(file);
        let bullet = own.split("\n- ").find(|b| b.starts_with("Team setup")).unwrap_or_else(|| panic!("{file} carries a Team setup bullet below init's marker"));
        let bullet = bullet.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in ["`setup` page", "`grimoire land <ticket>`", "cargo install --path crates/cli", "every write is refused", "`store = \"grimoire\"`"] {
            assert!(bullet.contains(phrase), "{file}: the setup procedure says {phrase:?}:\n{bullet}");
        }
    }
    // The setup of one person with a store and no server is held by tests, this repository no longer
    // being worked in it: the command line's own, and the browser tests' fixture, which init gives a store.
    assert!(read("crates/cli/tests/cli.rs").contains("\"init\", \"--store\""));
    assert!(read("web/behaviour/serve.mjs").contains("grimoire(\"init\", \"--store\", store)"));
}
