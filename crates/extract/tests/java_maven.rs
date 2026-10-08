//! Extract a small Maven-style Java build and check what the Java adapter sees, joined.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use grimoire_core::Extraction;
use grimoire_extract::Extractor;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("maven")
}

const CORE: &str = "core/src/main/java/com/example/core";

/// # Test contract
///
/// ```json
/// {"test": "a-maven-build-joins", "checks": ["crates/extract/src/java#identity", "crates/extract/src/java#coverage", "crates/extract/src/java#imports-as-candidates", "crates/extract/src/java#same-package-references", "crates/extract/src/java#leading-javadoc"]}
/// ```
#[test]
fn a_maven_build_joins() {
    let x = Extractor::for_languages(fixture(), &["java".to_string()]);
    assert_eq!(x.languages(), vec!["java"]);
    let (files, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(files.len(), 5, "the five .java files and nothing else: {:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>());
    let joined = Extraction::from_files(&files);
    let ids: Vec<&String> = joined.modules.keys().collect();
    let util = format!("{CORE}/util");
    let test = "core/src/test/java/com/example/core/GreeterTest";
    assert_eq!(ids, vec![&CORE.to_string(), &format!("{CORE}/Greeter"), &format!("{util}/Numbers"), &format!("{util}/Strings"), &test.to_string()]);

    // The package's contract is in its package-info, after a licence header.
    let package = &joined.modules[CORE];
    assert!(package.purpose.is_some() && !package.expects_contract, "{:?}", package.contract_error);
    let greeter = &joined.modules[&format!("{CORE}/Greeter")];
    assert!(greeter.expects_contract && greeter.criteria.iter().any(|c| c.id == "greets"), "{:?}", greeter.contract_error);
    let numbers = &joined.modules[&format!("{util}/Numbers")];
    assert!(numbers.expects_contract && numbers.purpose.is_none(), "a class with a Javadoc but no contract");
    assert!(!joined.modules[test].expects_contract);

    // Imports: a single-type and a static one resolve to their classes; the JDK, OSGi and JUnit to nothing.
    assert_eq!(greeter.imports, vec![format!("{util}/Numbers"), format!("{util}/Strings")]);
    // The test's on-demand import stands for both types of the package; Greeter, in the test's own package, needs no import and is an edge all the same.
    assert_eq!(joined.modules[test].imports, vec![format!("{CORE}/Greeter"), format!("{util}/Numbers"), format!("{util}/Strings")]);
    // A class qualifying a call in its own package, with no import.
    assert_eq!(joined.modules[&format!("{util}/Strings")].imports, vec![format!("{util}/Numbers")]);
    assert_eq!(joined.unresolved_imports, 0, "nothing external is counted as unresolved");

    // The test blocks on JUnit methods, a Javadoc and a run of line comments.
    let tests: Vec<(&str, u32)> = joined.tests.iter().map(|t| (t.name.as_str(), t.line)).collect();
    assert_eq!(tests, vec![("greets-by-name", 10), ("loud-is-capitals", 21)], "{:?}", joined.test_errors);
    let decls = &joined.files[&format!("{CORE}/Greeter.java")].decls;
    assert!(decls.iter().any(|d| d.path == "Greeter::Tone" && d.kind == "type" && d.line == 38), "{decls:?}");
}
