//! Extract a small project of TypeScript and JavaScript side by side, a package of the
//! repository entered through its exports, and check what the two languages see, joined.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use grimoire_core::Extraction;
use grimoire_extract::Extractor;

const CONTRACT: &str = "/**\n * # Module contract\n *\n * ```json\n * {\"purpose\":\"P.\",\"criteria\":[{\"id\":\"c\",\"statement\":\"S.\"}]}\n * ```\n */\n";

/// The project, written to a fresh folder: an application package of TypeScript and
/// JavaScript, and a JavaScript library package it imports by name.
fn project() -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix("grimoire-mixed-").tempdir().unwrap();
    let files: Vec<(&str, String)> = vec![
        ("package.json", r#"{"name": "mixed", "private": true, "workspaces": ["app", "packages/*"]}"#.into()),
        ("app/package.json", r#"{"name": "app"}"#.into()),
        ("app/src/main.ts", format!("{CONTRACT}import {{ legacy }} from \"./legacy.js\";\nimport {{ Button }} from \"@acme/ui/button\";\nimport {{ theme }} from \"@acme/ui\";\nimport React from \"react\";\nexport function main(): void {{}}\n")),
        ("app/src/legacy.js", format!("{CONTRACT}const {{ format }} = require(\"./format\");\nfunction legacy() {{ return format(1); }}\nmodule.exports = {{ legacy }};\n")),
        ("app/src/format.ts", format!("{CONTRACT}export const format = (n: number): string => `${{n}}`;\n")),
        ("app/src/Card.jsx", "import { legacy } from \"./legacy.js\";\nexport function Card() {\n  const open = () => import(\"./lazy.mjs\");\n  return <button onClick={open}>{legacy()}</button>;\n}\n".into()),
        ("app/src/lazy.mjs", "export const lazy = 1;\n".into()),
        ("app/src/main.spec.js", "import { main } from \"./main\";\n".into()),
        ("app/vite.config.js", "export default {};\n".into()),
        ("packages/ui/package.json", r#"{"name": "@acme/ui", "exports": {".": {"types": "./types/index.d.ts", "import": "./lib/index.mjs"}, "./button": {"import": "./lib/button.mjs"}}}"#.into()),
        ("packages/ui/lib/index.mjs", "export const theme = {};\n".into()),
        ("packages/ui/lib/button.mjs", "export function Button() {}\n".into()),
        ("packages/ui/types/index.d.ts", "export declare const theme: object;\n".into()),
    ];
    for (rel, text) in files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

/// # Test contract
///
/// ```json
/// {"test": "a-mixed-project-joins", "checks": ["crates/extract/src/typescript#files", "crates/extract/src/typescript#coverage", "crates/extract/src/typescript#imports-as-candidates", "crates/extract/src/typescript#package-exports", "crates/extract/src/typescript#require-and-dynamic-import", "req:javascript-files"]}
/// ```
#[test]
fn a_mixed_project_joins() {
    let dir = project();
    let x = Extractor::for_languages(dir.path(), &["typescript".to_string(), "javascript".to_string()]);
    let (files, errors) = x.extract_all().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let languages: Vec<(&str, &str)> = files.iter().map(|f| (f.path.as_str(), f.language.as_str())).collect();
    assert_eq!(
        languages,
        vec![
            ("app/src/Card.jsx", "javascript"),
            ("app/src/format.ts", "typescript"),
            ("app/src/lazy.mjs", "javascript"),
            ("app/src/legacy.js", "javascript"),
            ("app/src/main.spec.js", "javascript"),
            ("app/src/main.ts", "typescript"),
            ("app/vite.config.js", "javascript"),
            ("packages/ui/lib/button.mjs", "javascript"),
            ("packages/ui/lib/index.mjs", "javascript"),
        ],
        "every file of either language, and no declaration file"
    );
    let joined = Extraction::from_files(&files);
    let m = |id: &str| joined.modules.get(id).unwrap_or_else(|| panic!("no module {id}: {:?}", joined.modules.keys().collect::<Vec<_>>()));

    // TypeScript imports JavaScript by its .js name, JavaScript requires TypeScript, and a
    // package of the repository is entered through its exports; react is nobody's module.
    assert_eq!(m("app/src/main").imports, vec!["app/src/legacy", "packages/ui/lib", "packages/ui/lib/button"]);
    assert_eq!(m("app/src/legacy").imports, vec!["app/src/format"]);
    assert_eq!(m("app/src/Card").imports, vec!["app/src/lazy", "app/src/legacy"], "JSX parsed, a dynamic import followed");
    assert_eq!(joined.unresolved_imports, 0, "{:?}", joined.modules.values().map(|m| &m.imports).collect::<Vec<_>>());

    // Contracts in both languages; under src/ both are expected to carry one, a spec and config not.
    for id in ["app/src/main", "app/src/legacy", "app/src/format"] {
        assert!(m(id).expects_contract && m(id).criteria.iter().any(|c| c.id == "c"), "{id}: {:?}", m(id).contract_error);
    }
    assert!(m("app/src/Card").expects_contract && m("app/src/Card").purpose.is_none());
    for id in ["app/src/main.spec", "app/vite.config", "packages/ui/lib"] {
        assert!(!m(id).expects_contract, "{id}");
    }
    assert!(joined.files["app/src/Card.jsx"].decls.iter().any(|d| d.path == "Card" && d.kind == "function" && d.line == 2));

    // A project naming only typescript reads none of the JavaScript.
    let ts_only = Extractor::for_languages(dir.path(), &["typescript".to_string()]);
    assert_eq!(ts_only.walk().unwrap(), ["app/src/format.ts", "app/src/main.ts"]);
}
