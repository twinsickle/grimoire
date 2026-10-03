//! What the crate root offers the other crates, held by naming it from outside.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

/// # Test contract
///
/// ```json
/// {"test": "the-root-names-what-other-crates-build-on", "checks": ["crates/core/src#root-reexports"]}
/// ```
#[test]
fn the_root_names_what_other_crates_build_on() {
    // Each is named from the root, as another crate names it: this compiles only while the root re-exports it.
    use grimoire_core::{Build, Extraction, FileExtraction, Graph, Malformed, State, Store, ToolError};
    let named = [
        std::any::type_name::<Build>(),
        std::any::type_name::<Extraction>(),
        std::any::type_name::<FileExtraction>(),
        std::any::type_name::<Graph>(),
        std::any::type_name::<Malformed>(),
        std::any::type_name::<State>(),
        std::any::type_name::<Store>(),
        std::any::type_name::<ToolError>(),
    ];
    // Each is defined in a module beneath, so the root is where it is re-exported.
    for name in named {
        assert!(name.starts_with("grimoire_core::") && name.matches("::").count() >= 2, "{name}");
    }
    // The model is re-exported whole; a reference, a stored contract and a derived module stand for it here.
    let _: Option<(grimoire_core::Reference, grimoire_core::Contract, grimoire_core::Module)> = None;
    let root = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    // A line of the root's own, not the words in a comment.
    assert!(root.lines().any(|l| l == "pub use model::*;"), "every model type is at the root");
}
