//! # Module contract
//!
//! ```json
//! {"purpose":"Find and parse the contract grammar every language shares: a JSON block under a `# Module contract` heading in a file's leading comment, and under a `# Test contract` heading in any comment.","criteria":[{"id":"test-blocks","statement":"A # Test contract heading followed by a fenced JSON block {test, checks} names one test and what it checks, each check a criterion as module id#criterion id or a requirement as req:<slug>; a block whose JSON is invalid, whose test is not a slug, or whose checks is empty or holds anything else, is reported with the reason."},{"id":"language-neutral","statement":"The parser sees comment text with the markers already stripped and knows nothing of any language."},{"id":"malformed-is-reported","statement":"A heading with a missing or invalid JSON block is reported as malformed with the reason, never silently treated as absent."}]}
//! ```

use grimoire_core::Contract;

/// The result of looking for a contract in a leading comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub contract: Result<Contract, String>,
    /// Line of the heading, 1-based, within the file.
    pub heading_line: u32,
}

fn is_heading(line: &str, name: &str) -> bool {
    let t = line.trim();
    let Some(rest) = t.strip_prefix('#') else { return false };
    let rest = rest.trim_start_matches('#').trim();
    rest.eq_ignore_ascii_case(name)
}

/// The body of the ```json block that follows the heading at `heading`,
/// skipping blank lines to the fence.
fn fenced_json(lines: &[&str], heading: usize) -> Result<String, String> {
    let mut i = heading + 1;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() || !lines[i].trim_start().starts_with("```") {
        return Err("the heading is not followed by a ``` fenced block".to_string());
    }
    let lang = lines[i].trim().trim_start_matches('`').trim();
    if !lang.is_empty() && !lang.eq_ignore_ascii_case("json") {
        return Err(format!("the fenced block is `{lang}`, not json"));
    }
    let mut body = String::new();
    for line in &lines[i + 1..] {
        if line.trim() == "```" {
            return Ok(body);
        }
        body.push_str(line);
        body.push('\n');
    }
    Err("the fenced block is not closed".to_string())
}

/// One `# Test contract` block: the test's name and what it checks, or why
/// it could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundTest {
    /// Line of the heading, 1-based, within the file.
    pub line: u32,
    pub test: Result<(String, Vec<String>), String>,
}

/// `{"test": string, "checks": [string]}` and nothing else.
fn test_block(body: &str) -> Result<(String, Vec<String>), String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("expected an object")?;
    if let Some(k) = obj.keys().find(|k| *k != "test" && *k != "checks") {
        return Err(format!("unknown field `{k}`, expected `test` and `checks`"));
    }
    let name = obj.get("test").and_then(|t| t.as_str()).ok_or("`test` must be a string")?;
    let checks = obj.get("checks").and_then(|c| c.as_array()).ok_or("`checks` must be a list")?;
    let checks = checks.iter().map(|c| c.as_str().map(str::to_string).ok_or("every check must be a string")).collect::<Result<Vec<_>, _>>()?;
    Ok((name.to_string(), checks))
}

/// Every test block in a comment. `text` has its markers stripped; its
/// first line is `first_line` in the file.
pub fn find_tests(text: &str, first_line: u32) -> Vec<FoundTest> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (h, line) in lines.iter().enumerate() {
        if !is_heading(line, "test contract") {
            continue;
        }
        let test = fenced_json(&lines, h).and_then(|body| {
            let (name, checks) = test_block(&body).map_err(|e| format!("invalid test contract JSON: {e}"))?;
            if !grimoire_core::store::is_slug(&name) {
                return Err(format!("test name `{name}` is not a slug"));
            }
            if checks.is_empty() {
                return Err(format!("test `{name}` checks nothing"));
            }
            // A requirement is req:<slug>, whatever else it holds; anything else is a criterion,
            // split at its last #, since a module id may hold one and a criterion id never does.
            let requirement = |c: &str| c.strip_prefix("req:").is_some_and(grimoire_core::store::is_slug);
            let criterion = |c: &str| !c.starts_with("req:") && c.rsplit_once('#').is_some_and(|(m, id)| !m.is_empty() && grimoire_core::store::is_slug(id));
            if let Some(bad) = checks.iter().find(|c| !criterion(c) && !requirement(c)) {
                return Err(format!("`{bad}` is neither a criterion as module id#criterion id nor a requirement as req:<slug>"));
            }
            Ok((name, checks))
        });
        out.push(FoundTest { line: first_line + h as u32, test });
    }
    out
}

/// `text` is the comment with markers stripped; `first_line` is the file line
/// of its first line, 1-based.
pub fn find(text: &str, first_line: u32) -> Option<Found> {
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines.iter().position(|l| is_heading(l, "module contract"))?;
    let heading_line = first_line + heading as u32;
    let contract = (|| {
        let body = fenced_json(&lines, heading)?;
        let contract: Contract = serde_json::from_str(&body).map_err(|e| format!("invalid contract JSON: {e}"))?;
        validate(&contract)?;
        Ok(contract)
    })();
    Some(Found { contract, heading_line })
}

fn validate(c: &Contract) -> Result<(), String> {
    if c.purpose.trim().is_empty() {
        return Err("purpose is empty".into());
    }
    let mut seen = std::collections::HashSet::new();
    for cr in &c.criteria {
        if !grimoire_core::store::is_slug(&cr.id) {
            return Err(format!("criterion id `{}` is not a slug", cr.id));
        }
        if !seen.insert(&cr.id) {
            return Err(format!("criterion id `{}` appears twice", cr.id));
        }
        if cr.statement.trim().is_empty() {
            return Err(format!("criterion `{}` has an empty statement", cr.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "parses-a-contract", "checks": ["crates/extract/src/contract#language-neutral"]}
    /// ```
    #[test]
    fn parses_a_contract() {
        let text = "# Module contract\n\n```json\n{\"purpose\":\"P\",\"criteria\":[{\"id\":\"a\",\"statement\":\"S\"}]}\n```\n\nMore prose.";
        let f = find(text, 1).unwrap();
        assert_eq!(f.heading_line, 1);
        let c = f.contract.unwrap();
        assert_eq!(c.purpose, "P");
        assert_eq!(c.criteria[0].id, "a");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reports-malformed", "checks": ["crates/extract/src/contract#malformed-is-reported"]}
    /// ```
    #[test]
    fn reports_malformed() {
        let f = find("intro\n# Module contract\nnot a fence", 3).unwrap();
        assert_eq!(f.heading_line, 4);
        assert!(f.contract.unwrap_err().contains("fenced"));
        let f = find("# Module contract\n```json\n{\"purpose\":\"P\"}\n```", 1).unwrap();
        assert!(f.contract.unwrap_err().contains("invalid contract JSON"));
        let f = find("# Module contract\n```json\n{\"purpose\":\"P\",\"criteria\":[{\"id\":\"Bad Id\",\"statement\":\"S\"}]}\n```", 1).unwrap();
        assert!(f.contract.unwrap_err().contains("not a slug"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reads-test-blocks", "checks": ["crates/extract/src/contract#test-blocks"]}
    /// ```
    #[test]
    fn reads_test_blocks() {
        let text = "Checks ordering.\n\n# Test contract\n\n```json\n{\"test\":\"findings-are-stable\",\"checks\":[\"crates/core/src/lint#stable-order\"]}\n```\n";
        let found = find_tests(text, 10);
        assert_eq!(found, vec![FoundTest { line: 12, test: Ok(("findings-are-stable".into(), vec!["crates/core/src/lint#stable-order".into()])) }]);
        assert!(find_tests("# Module contract\n```json\n{}\n```", 1).is_empty(), "a module contract is not a test");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "reports-broken-test-blocks", "checks": ["crates/extract/src/contract#test-blocks"]}
    /// ```
    #[test]
    fn reports_broken_test_blocks() {
        let one = |json: &str| find_tests(&format!("# Test contract\n```json\n{json}\n```"), 1).remove(0).test.unwrap_err();
        assert!(one(r#"{"test":"Bad Name","checks":["m#c"]}"#).contains("not a slug"));
        assert!(one(r#"{"test":"t","checks":[]}"#).contains("checks nothing"));
        assert!(one(r#"{"test":"t","checks":["no-hash"]}"#).contains("module id#criterion id"));
        assert!(one(r#"{"test":"t","checks":["req:Not A Slug"]}"#).contains("req:<slug>"));
        assert!(one(r#"{"test":"t","checks":["req:foo#bar"]}"#).contains("req:<slug>"), "a requirement is never read as a criterion");
        assert!(one(r#"{"test":"t","checks":["m#Not A Slug"]}"#).contains("module id#criterion id"));
        assert!(find_tests("# Test contract
```json
{\"test\":\"t\",\"checks\":[\"src/a#b#kept\"]}
```", 1)[0].test.is_ok(), "a module id may hold #");
        assert!(find_tests("# Test contract
```json
{\"test\":\"t\",\"checks\":[\"req:src-boundary\"]}
```", 1)[0].test.is_ok(), "a requirement may be checked");
        assert!(one(r#"{"test":"t"}"#).contains("invalid test contract JSON"));
    }

    #[test]
    fn absent_is_none() {
        assert!(find("Just a doc comment.", 1).is_none());
    }
}

