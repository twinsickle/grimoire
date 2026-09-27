//! # Module contract
//!
//! ```json
//! {"purpose":"Find and parse the one contract grammar every language shares: a JSON block under a `# Module contract` heading in a file's leading comment.","criteria":[{"id":"language-neutral","statement":"The parser sees comment text with the markers already stripped and knows nothing of any language."},{"id":"malformed-is-reported","statement":"A heading with a missing or invalid JSON block is reported as malformed with the reason, never silently treated as absent."}]}
//! ```

use grimoire_core::Contract;

/// The result of looking for a contract in a leading comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub contract: Result<Contract, String>,
    /// Line of the heading, 1-based, within the file.
    pub heading_line: u32,
}

fn is_heading(line: &str) -> bool {
    let t = line.trim();
    let Some(rest) = t.strip_prefix('#') else { return false };
    let rest = rest.trim_start_matches('#').trim();
    rest.eq_ignore_ascii_case("module contract")
}

/// `text` is the comment with markers stripped; `first_line` is the file line
/// of its first line, 1-based.
pub fn find(text: &str, first_line: u32) -> Option<Found> {
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines.iter().position(|l| is_heading(l))?;
    let heading_line = first_line + heading as u32;
    let mut i = heading + 1;
    // Skip blank lines to the fence.
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let contract = (|| {
        if i >= lines.len() || !lines[i].trim_start().starts_with("```") {
            return Err("the heading is not followed by a ``` fenced block".to_string());
        }
        let fence = lines[i].trim();
        let lang = fence.trim_start_matches('`').trim();
        if !lang.is_empty() && !lang.eq_ignore_ascii_case("json") {
            return Err(format!("the fenced block is `{lang}`, not json"));
        }
        let mut body = String::new();
        let mut j = i + 1;
        let mut closed = false;
        while j < lines.len() {
            if lines[j].trim() == "```" {
                closed = true;
                break;
            }
            body.push_str(lines[j]);
            body.push('\n');
            j += 1;
        }
        if !closed {
            return Err("the fenced block is not closed".to_string());
        }
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

    #[test]
    fn parses_a_contract() {
        let text = "# Module contract\n\n```json\n{\"purpose\":\"P\",\"criteria\":[{\"id\":\"a\",\"statement\":\"S\"}]}\n```\n\nMore prose.";
        let f = find(text, 1).unwrap();
        assert_eq!(f.heading_line, 1);
        let c = f.contract.unwrap();
        assert_eq!(c.purpose, "P");
        assert_eq!(c.criteria[0].id, "a");
    }

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

    #[test]
    fn absent_is_none() {
        assert!(find("Just a doc comment.", 1).is_none());
    }
}

