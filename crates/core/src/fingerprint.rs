//! # Module contract
//!
//! ```json
//! {"purpose":"Fingerprint every node that can change, so a ticket's close can tell which of the nodes it touched never did.","criteria":[{"id":"stable","statement":"A fingerprint is a 64-bit FNV-1a hash written as hex, the same for the same content in every build and on every platform."},{"id":"covers","statement":"Stored records are fingerprinted from their JSON, criteria from their statement, files from their text as read, tests from their block, and a module from its files' fingerprints together."}]}
//! ```

use std::collections::BTreeMap;

use crate::graph::{Graph, Node};
use crate::model::*;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// FNV-1a over the bytes, as sixteen hex digits. Chosen over the standard
/// library's hasher because that one may change between builds.
pub fn of_bytes(bytes: &[u8]) -> String {
    let mut h = FNV_OFFSET;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{h:016x}")
}

pub fn of_text(text: &str) -> String {
    of_bytes(text.as_bytes())
}

/// A stored record's fingerprint, from its JSON: what a proposal remembers
/// of the truth it was written against, and what `all` gives a page,
/// requirement or ticket.
pub fn of_record<T: serde::Serialize>(value: &T) -> String {
    of_text(&serde_json::to_string(value).unwrap_or_default())
}

fn of_json<T: serde::Serialize>(value: &T) -> String {
    of_record(value)
}

/// The fingerprint of every node that can change: stored records, modules,
/// criteria, files and tests. Declarations are left out; they change only
/// when their file does.
pub fn all(g: &Graph) -> BTreeMap<Reference, String> {
    let mut out = BTreeMap::new();
    for (r, node) in &g.nodes {
        let fp = match node {
            Node::Page(p) => of_json(p),
            Node::Req(q) => of_json(q),
            Node::Ticket(t) => of_json(t),
            Node::Criterion(c) => of_text(&c.statement),
            Node::File(f) => f.digest.clone(),
            Node::Test(t) => of_json(t),
            Node::Module(m) => {
                let digests: Vec<&str> = m.files.iter().filter_map(|f| match g.get(&Reference::file(f)) {
                    Some(Node::File(file)) => Some(file.digest.as_str()),
                    _ => None,
                }).collect();
                of_text(&digests.join("\n"))
            }
            Node::Decl(_) => continue,
        };
        out.insert(r.clone(), fp);
    }
    out
}

/// The nodes of `touches` whose fingerprint is the same now as at the start.
/// A node present at only one end has changed; one present at neither is left
/// to the dangling lint.
pub fn untouched(touches: &[Reference], start: &BTreeMap<String, String>, now: &BTreeMap<Reference, String>) -> Vec<Reference> {
    touches.iter().filter(|r| matches!((start.get(&r.to_string()), now.get(*r)), (Some(a), Some(b)) if a == b)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "fingerprints-are-fnv-1a", "checks": ["crates/core/src/fingerprint#stable"]}
    /// ```
    #[test]
    fn fingerprints_are_fnv_1a() {
        // Published FNV-1a 64 test vectors: the same in every build.
        assert_eq!(of_text(""), "cbf29ce484222325");
        assert_eq!(of_text("a"), "af63dc4c8601ec8c");
        assert_eq!(of_text("foobar"), "85944171f73967e8");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "every-changeable-node-is-fingerprinted", "checks": ["crates/core/src/fingerprint#covers"]}
    /// ```
    #[test]
    fn every_changeable_node_is_fingerprinted() {
        let g = crate::graph::fixtures::graph();
        let fp = all(&g);
        for r in ["page:overview", "req:stale", "ticket:phase-62", "module:src/a/ready", "criterion:src/a/ready#guarantee", "file:src/a/ready.rs", "test:src/a/ready.rs#ready-needs-a-commit"] {
            assert!(fp.contains_key(&r.parse::<Reference>().unwrap()), "{r} has a fingerprint");
        }
        assert!(!fp.keys().any(|r| r.kind == Kind::Decl), "declarations change only with their file");
        // A module's fingerprint follows its files' text.
        let mut files = crate::extraction::fixtures::extraction_files();
        files[0].digest = of_text("changed");
        let changed = all(&Graph::build(&crate::graph::fixtures::store(), &crate::extraction::Extraction::from_files(&files), &[]));
        assert_ne!(changed[&Reference::module("src/a/ready")], fp[&Reference::module("src/a/ready")]);
        assert_eq!(changed[&Reference::module("src/a/layout")], fp[&Reference::module("src/a/layout")]);
    }

    #[test]
    fn untouched_is_unchanged_at_both_ends() {
        let r = |s: &str| s.parse::<Reference>().unwrap();
        let start: BTreeMap<String, String> = [("page:a", "1"), ("page:b", "2"), ("page:gone", "3")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let now: BTreeMap<Reference, String> = [("page:a", "1"), ("page:b", "9"), ("page:new", "4")].iter().map(|(k, v)| (r(k), v.to_string())).collect();
        let touches = vec![r("page:a"), r("page:b"), r("page:gone"), r("page:new"), r("page:never")];
        assert_eq!(untouched(&touches, &start, &now), vec![r("page:a")]);
    }
}
