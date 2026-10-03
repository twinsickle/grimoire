//! Who may write the store, held by finding every call to its write functions.
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
/// {"test": "only-the-owners-write-the-store", "checks": ["req:server-single-writer"]}
/// ```
#[test]
fn only_the_daemon_and_the_cli_write_the_store() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // The daemon owns the store while it serves; the CLI owns it when none does; and on a
    // project that names a server, the sync is what writes a machine's copy of its intent.
    let owners = ["server/src/daemon.rs", "cli/src/main.rs", "server/src/store_io.rs", "server/src/remote.rs"];
    let writes = ["store_io::persist", "store_io::write_all", "store_io::write_project_file", "store_io::write_serving", "store_io::remove_serving"];
    let mut callers = Vec::new();
    for krate in ["core", "extract", "server", "cli"] {
        for f in rust_files(&crates.join(krate).join("src")) {
            let text = std::fs::read_to_string(&f).unwrap();
            // Test code builds stores freely; the rule is about what ships.
            let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
            if writes.iter().any(|w| shipped.contains(w)) {
                callers.push(f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    assert!(!callers.is_empty(), "the search found the writers");
    for c in &callers {
        assert!(owners.contains(&c.as_str()), "{c} writes the store, but only the daemon and the CLI commands that own it may");
    }
}
