//! The UI's generated TypeScript, held against the Rust answer types. It
//! reads and writes a file, so it lives here and not in the core's source,
//! which does no I/O.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;

use grimoire_core::answer::{tool_answers, typescript};

/// Where the generated TypeScript lives, from this crate's directory.
const GENERATED: &str = "../../web/src/generated/tools.ts";

fn generated_file() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(GENERATED)
}

/// # Test contract
///
/// ```json
/// {"test": "the-generated-typescript-is-current", "checks": ["crates/core/src/answer#generated-current", "web/src/types#mirrors-tools"]}
/// ```
#[test]
fn the_generated_typescript_is_current() {
    let path = generated_file();
    let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(have == typescript(), "{} is out of date with the Rust answer types; run `cargo test -p grimoire-core --test typescript -- --ignored` and commit it", path.display());
}

/// Rewrites the generated file. Ignored, so only asking for it runs it, and
/// the same command works in any shell.
#[test]
#[ignore = "rewrites web/src/generated/tools.ts; run on purpose"]
fn rewrite_the_generated_typescript() {
    let path = generated_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, typescript()).unwrap();
}

/// # Test contract
///
/// ```json
/// {"test": "every-tool-has-one-answer-type", "checks": ["crates/core/src/answer#typed-answers", "crates/core/src/query#documented-shapes"]}
/// ```
#[test]
fn every_tool_has_one_answer_type() {
    let answered: Vec<&str> = tool_answers(&ts_rs::Config::new()).into_iter().map(|(t, _)| t).collect();
    let unique: BTreeSet<&str> = answered.iter().copied().collect();
    assert_eq!(unique.len(), answered.len(), "a tool named by two answer types");
    let tools: BTreeSet<&str> = grimoire_core::tools::TOOL_NAMES.iter().copied().collect();
    assert_eq!(unique, tools, "every tool, and only the tools, have an answer type");
}
