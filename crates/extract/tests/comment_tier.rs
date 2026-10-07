//! Read a repository of markup and styles by their comments alone, at tier 0,
//! and check what the grimoire sees of it.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use grimoire_core::tools::call;
use grimoire_core::{Build, Extraction, Reference, State, Store};
use grimoire_extract::{supported_languages, Extractor};
use serde_json::json;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("tiers")
}

/// This build, as the loader tells it to core.
fn build() -> Build {
    let supported = supported_languages();
    Build { version: "0.0.0".into(), supported_languages: supported.iter().map(|(l, _)| l.to_string()).collect(), tiers: supported.iter().map(|(l, t)| (l.to_string(), *t)).collect() }
}

/// A state over the fixture read for the given languages.
fn state(languages: &[&str]) -> State {
    let languages: Vec<String> = languages.iter().map(|l| l.to_string()).collect();
    let x = Extractor::for_languages(fixture(), &languages);
    let (files, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let mut store = Store::default();
    store.config.languages = languages;
    let mut s = State::new(store, Extraction::from_files(&files), vec![]);
    s.set_build(build());
    s
}

/// # Test contract
///
/// ```json
/// {"test": "markup-and-styles-join-at-tier-zero", "checks": ["req:extract-comment-identity", "req:extract-comment-reading", "req:extract-comment-coverage", "req:extract-every-comment"]}
/// ```
#[test]
fn markup_and_styles_join_at_tier_zero() {
    let x = Extractor::for_languages(fixture(), &["html".to_string(), "css".to_string()]);
    assert_eq!(x.walk().unwrap(), ["components/button/button.css", "components/button/button.html", "theme/plain.css"]);
    let (files, _) = x.extract_all().unwrap();
    let joined = Extraction::from_files(&files);
    // Markup and styles of one component are two modules, each with its contract.
    let html = &joined.modules["components/button/button.html"];
    let css = &joined.modules["components/button/button.css"];
    assert!(html.criteria.iter().any(|c| c.id == "labelled"), "{:?}", html.contract_error);
    assert!(css.criteria.iter().any(|c| c.id == "round"), "{:?}", css.contract_error);
    assert_eq!(html.contract_at.as_ref().unwrap(), &("components/button/button.html".to_string(), 3));
    assert!(html.imports.is_empty() && css.imports.is_empty(), "no imports, not even the @import");
    assert!(files.iter().all(|f| f.decls.is_empty()), "no declarations");

    // The test in the markup checks a criterion of each; the style sheet with no contract is reported.
    let mut s = state(&["html", "css"]);
    let lint = call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output;
    let findings = lint["findings"].as_array().unwrap();
    let on = |r: &str, check: &str| findings.iter().any(|f| f["ref"] == r && f["check"] == check);
    assert!(on("module:theme/plain.css", "no-contract"), "{findings:#?}");
    for criterion in ["components/button/button.html#labelled", "components/button/button.css#round"] {
        assert!(!on(&Reference::criterion(criterion.split_once('#').unwrap().0, criterion.split_once('#').unwrap().1).to_string(), "untested"), "{criterion} is tested: {findings:#?}");
    }
    assert!(!findings.iter().any(|f| ["unsupported-language", "malformed-test", "dangling-edge"].contains(&f["check"].as_str().unwrap())), "{findings:#?}");
    let test = call(&mut s, "get", json!({"ref": "test:components/button/button.html#button-has-a-label"}), "2026-10-06").unwrap().output;
    assert_eq!(test["node"]["line"], 12, "{test:#?}");
}

/// # Test contract
///
/// ```json
/// {"test": "status-names-each-language-tier", "checks": ["req:tool-status", "req:lint-unsupported-language", "req:extract-tiers"]}
/// ```
#[test]
fn status_names_each_language_tier() {
    let mut s = state(&["rust", "css", "cobol"]);
    let status = call(&mut s, "status", json!({}), "2026-10-06").unwrap().output;
    assert_eq!(status["tiers"], json!([{"language": "rust", "tier": 2}, {"language": "css", "tier": 0}, {"language": "cobol", "tier": null}]));
    // css is read by its comments, so only cobol is a language the build cannot read.
    let lint = call(&mut s, "lint", json!({}), "2026-10-06").unwrap().output;
    let unsupported: Vec<&str> = lint["findings"].as_array().unwrap().iter().filter(|f| f["check"] == "unsupported-language").map(|f| f["reason"].as_str().unwrap()).collect();
    assert_eq!(unsupported.len(), 1, "{unsupported:?}");
    assert!(unsupported[0].contains("`cobol`") && unsupported[0].contains("css"), "{}", unsupported[0]);
}
