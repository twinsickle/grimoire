//! Extract Grimoire's own repository and check what the adapter sees.

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
