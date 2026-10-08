//! Extract Grimoire's own repository and check what the adapter sees.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use grimoire_core::Extraction;
use grimoire_extract::Extractor;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").canonicalize().unwrap()
}

#[test]
fn extracts_this_repository() {
    let x = Extractor::for_languages(repo_root(), &["rust".to_string()]);
    let files = x.walk().unwrap();
    assert!(files.contains(&"crates/core/src/model.rs".to_string()), "{files:?}");
    assert!(!files.iter().any(|f| f.starts_with("target/")), "gitignore not honoured");

    let (extracted, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let joined = Extraction::from_files(&extracted);

    let model = &joined.modules["crates/core/src/model"];
    assert!(model.purpose.is_some(), "{:?}", model.contract_error);
    assert!(model.criteria.iter().any(|c| c.id == "reference-form"));
    assert_eq!(model.contract_at.as_ref().unwrap().0, "crates/core/src/model.rs");

    let graph = &joined.modules["crates/core/src/graph"];
    assert!(graph.imports.contains(&"crates/core/src/model".to_string()), "{:?}", graph.imports);
    assert!(graph.imports.contains(&"crates/core/src/store".to_string()));

    let core_root = &joined.modules["crates/core/src"];
    assert!(core_root.purpose.is_some());
    assert!(core_root.files.contains(&"crates/core/src/lib.rs".to_string()));

    // Cross-crate import resolves through the package name.
    let extract_lib = &joined.modules["crates/extract/src"];
    assert!(extract_lib.imports.iter().any(|i| i.starts_with("crates/core/src")), "{:?}", extract_lib.imports);

    let file = &joined.files["crates/core/src/model.rs"];
    assert!(file.decls.iter().any(|d| d.path == "Reference" && d.kind == "type"));
    assert!(file.decls.iter().any(|d| d.path == "Reference::split_hash" && d.kind == "method"));
}

/// # Test contract
///
/// ```json
/// {"test": "reexports-reach-the-declaring-module", "checks": ["crates/extract/src/rust#reexports-recorded", "crates/core/src/extraction#imports-follow-reexports"]}
/// ```
#[test]
fn reexports_reach_the_declaring_module() {
    let x = Extractor::for_languages(repo_root(), &["rust".to_string()]);
    let (extracted, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let joined = Extraction::from_files(&extracted);
    // `use grimoire_core::Decl;` passes through the crate root's `pub use model::*;` and model's
    // `pub use derived::*;` to the module that declares it, so the Java adapter does not seem to
    // depend on everything the crate root declares.
    let java = &joined.modules["crates/extract/src/java"];
    assert!(java.imports.contains(&"crates/core/src/model/derived".to_string()), "{:?}", java.imports);
    assert!(!java.imports.contains(&"crates/core/src".to_string()), "{:?}", java.imports);
}

#[test]
fn extracts_the_web_ui() {
    let x = Extractor::for_languages(repo_root(), &["typescript".to_string()]);
    let files = x.walk().unwrap();
    assert!(files.contains(&"web/src/api.ts".to_string()), "{files:?}");
    assert!(!files.iter().any(|f| f.contains("node_modules") || f.starts_with("web/dist")), "gitignore not honoured");

    let (extracted, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let joined = Extraction::from_files(&extracted);

    let api = &joined.modules["web/src/api"];
    assert!(api.expects_contract);
    assert!(api.purpose.is_some(), "{:?}", api.contract_error);
    let home = &joined.modules["web/src/views/Home"];
    assert!(home.imports.contains(&"web/src/api".to_string()), "{:?}", home.imports);
    assert!(home.imports.contains(&"web/src/components".to_string()));
    let file = &joined.files["web/src/api.ts"];
    assert!(file.decls.iter().any(|d| d.path == "useQuery" && d.kind == "function"));
    assert!(!joined.modules.contains_key("web/vite.config") || !joined.modules["web/vite.config"].expects_contract);
}
