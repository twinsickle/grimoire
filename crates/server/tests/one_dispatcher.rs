//! One dispatcher for every transport, held by reading where the tools are handled.
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
/// {"test": "no-transport-handles-a-tool", "checks": ["req:tools-one-dispatcher"]}
/// ```
#[test]
fn no_transport_handles_a_tool_by_name() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // What hands a call to the dispatcher: the daemon, the team sync, and the CLI's own paths when no daemon serves.
    // These alone call it, and alone may route a call by its tool's name; the answer still comes from it.
    let bridges = ["server/src/daemon.rs", "server/src/remote.rs", "cli/src/main.rs", "cli/src/team.rs"];
    let names: Vec<&str> = grimoire_core::tools::definitions().iter().map(|d| d.name).collect();
    assert!(names.len() > 20, "the tools were listed");
    let mut callers = Vec::new();
    for krate in ["core", "extract", "server", "cli"] {
        for f in rust_files(&crates.join(krate).join("src")) {
            let rel = f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            // The dispatcher itself, and the definitions it answers by, are where the tools live.
            if rel == "core/src/tools.rs" || rel.starts_with("core/src/tools/") {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap();
            // Test code calls tools freely; the rule is about what ships.
            let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
            // A bridge may route a call by its tool's name, here or to the server; nothing else looks at it.
            if !bridges.contains(&rel.as_str()) {
                // A match arm, an or-pattern or a comparison on the name: each is looking at which tool it is.
                for name in &names {
                    for form in [format!("\"{name}\" =>"), format!("\"{name}\" |"), format!("== \"{name}\""), format!("\"{name}\" ==")] {
                        assert!(!shipped.contains(&form), "{rel} looks for the tool `{name}` by name (`{form}`); a tool lives once, behind tools::call");
                    }
                }
            }
            if shipped.contains("tools::call(") {
                callers.push(rel);
            }
        }
    }
    assert!(!callers.is_empty(), "the search found the dispatcher's callers");
    for c in &callers {
        assert!(bridges.contains(&c.as_str()), "{c} calls the dispatcher, but only the daemon, the team sync and the CLI hand it calls");
    }
}
