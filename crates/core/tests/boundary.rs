//! Construction rules about the core, held by reading its source.
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
