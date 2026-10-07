//! Construction rules about the core and the findings it alone builds, held by reading the source.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(rust_files(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

/// # Test contract
///
/// ```json
/// {"test": "core-does-no-io", "checks": ["req:core-boundary"]}
/// ```
#[test]
fn the_core_names_no_io() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files(&src);
    assert!(files.len() > 5, "the core's source was found");
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for io in ["std::fs", "std::net", "std::process", "std::env", "tokio", "File::"] {
            assert!(!text.contains(io), "{} names `{io}`: the core takes and returns values, and I/O belongs to its callers", f.display());
        }
    }
}

/// The names every finding in `src` gives as its check: the literal a finding is built with,
/// or the literal paired with its reason where the check is chosen before the finding is built.
fn check_names(src: &str) -> Vec<String> {
    let literal = |s: &str| s.strip_prefix('"').and_then(|s| s.split_once('"')).map(|(name, _)| name.to_string());
    let mut out = Vec::new();
    for (i, _) in src.match_indices("Finding::node(Rule::") {
        let rest = &src[i..];
        let arg = rest.split_once(", ").map(|(_, a)| a).unwrap_or("");
        out.extend(literal(arg).or_else(|| arg.strip_prefix("&format!(").and_then(literal)));
    }
    for (i, _) in src.match_indices("check: \"") {
        out.extend(literal(&src[i + "check: ".len()..]));
    }
    for (i, _) in src.match_indices("(\"") {
        let rest = &src[i + 1..];
        if let Some(name) = literal(rest) {
            if rest[name.len() + 2..].starts_with(", format!(") {
                out.push(name);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// # Test contract
///
/// ```json
/// {"test": "findings-are-connection-only", "checks": ["req:lint-connection-only"]}
/// ```
#[test]
fn every_finding_is_about_connection() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // Findings are built by the core's lint alone, the lint module and those beneath it, a function of the graph:
    // the core does no I/O, so no model or network is asked.
    let mut lint_sources = Vec::new();
    for krate in ["core", "extract", "server", "cli"] {
        for f in rust_files(&crates.join(krate).join("src")) {
            let rel = f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&f).unwrap();
            if rel == "core/src/lint.rs" || rel.starts_with("core/src/lint/") {
                lint_sources.push(text.split("#[cfg(test)]").next().unwrap().to_string());
            } else {
                let squeezed: String = text.split_whitespace().collect();
                assert!(!squeezed.contains("Finding{"), "{rel} builds a finding: findings come from the core's lint alone");
            }
        }
    }
    // Every check the lint has, each with the connection it reports. A new check fails here until it is listed,
    // so whoever adds it says what it connects; one that would judge meaning has no place in this list.
    let connections = [
        ("checks-retired", "a test names a requirement that no longer holds"),
        ("dangling-edge", "a reference leads nowhere"),
        ("landing-moved", "a landing ticket's proposal rests on a truth that moved"),
        ("malformed-file", "a store record cannot be read"),
        ("malformed-test", "a test block cannot be read"),
        ("missing-{which}-root", "a root the config does not name, or names as no page"),
        ("no-contract", "a module in the contract scope carries no contract"),
        ("no-scenario", "a requirement has no scenario"),
        ("proposal-moved", "an active ticket's proposal rests on a truth that moved"),
        ("rite-names-code", "a rule of work names a criterion"),
        ("ticket-cycle", "tickets rely on one another in a loop"),
        ("uncovered-criterion", "no requirement names a criterion"),
        ("unheld", "nothing holds a requirement"),
        ("unlinked-module", "no page links a module"),
        ("unlinked-page", "no page links a page"),
        ("unlinked-requirement", "no page links a requirement"),
        ("unsupported-language", "a configured language has no adapter in this build"),
        ("untested", "no test checks a criterion"),
    ];
    assert!(!lint_sources.is_empty(), "the lint's source was found");
    let found = check_names(&lint_sources.concat());
    let listed: Vec<String> = connections.iter().map(|(c, _)| c.to_string()).collect();
    assert_eq!(found, listed, "the lint's checks and the connections listed here differ: a new check is listed with the connection it reports");
}
