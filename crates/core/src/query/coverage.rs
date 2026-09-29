//! # Module contract
//!
//! ```json
//! {"purpose":"Answer coverage: the requirements and tests behind a criterion, a module's criteria, or the criteria a requirement names.","criteria":[{"id":"coverage","statement":"coverage returns, for a criterion, the requirements naming it and the tests checking it; for a module, each criterion with its requirements and tests, empty where there are none; and for a requirement, each criterion it names in covered_by order with its statement and its module's ref and purpose, null where the criterion or module does not exist."}]}
//! ```

use serde::Serialize;

use super::lookup::{summary, Summary};
use crate::graph::{Graph, Node};
use crate::model::*;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct CriterionCoverage {
    pub r#ref: Reference,
    pub statement: String,
    pub requirements: Vec<Summary>,
    pub tests: Vec<Summary>,
}

pub fn requirements_covering(g: &Graph, criterion: &Reference) -> Vec<Summary> {
    let mut v: Vec<Summary> = g.inbound_with(criterion, Relation::CoveredBy).filter_map(|e| g.get(&e.from).map(|n| summary(&e.from, n))).collect();
    v.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    v
}

pub fn tests_checking(g: &Graph, criterion: &Reference) -> Vec<Summary> {
    let mut v: Vec<Summary> = g.inbound_with(criterion, Relation::Checks).filter_map(|e| g.get(&e.from).map(|n| summary(&e.from, n))).collect();
    v.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
    v
}

pub fn module_coverage(g: &Graph, module: &Module) -> Vec<CriterionCoverage> {
    module
        .criteria
        .iter()
        .map(|c| {
            let r = Reference::criterion(&c.module, &c.id);
            CriterionCoverage { requirements: requirements_covering(g, &r), tests: tests_checking(g, &r), r#ref: r, statement: c.statement.clone() }
        })
        .collect()
}

/// A criterion as a requirement's coverage lists it: what it says and the
/// module it lives in. `statement` is `None` when the criterion is dangling.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoveringCriterion {
    pub r#ref: Reference,
    pub statement: Option<String>,
    pub module: ModuleSummary,
}

/// A module beside a criterion: its reference and purpose, `None` when the
/// module does not exist or has no contract.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleSummary {
    pub r#ref: Reference,
    pub purpose: Option<String>,
}

/// The criteria a requirement names, in `covered_by` order, each with its
/// module. Dangling entries are kept so a dangling requirement never looks covered.
pub fn requirement_coverage(g: &Graph, req: &Requirement) -> Vec<CoveringCriterion> {
    req.covered_by
        .iter()
        .map(|r| {
            let statement = match g.get(r) {
                Some(Node::Criterion(c)) => Some(c.statement.clone()),
                _ => None,
            };
            let module_id = r.split_hash().map_or(r.id.as_str(), |(m, _)| m);
            let module = Reference::module(module_id);
            let purpose = match g.get(&module) {
                Some(Node::Module(m)) => m.purpose.clone(),
                _ => None,
            };
            CoveringCriterion { r#ref: r.clone(), statement, module: ModuleSummary { r#ref: module, purpose } }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "requirement-coverage-in-order-with-modules", "checks": ["crates/core/src/query/coverage#coverage"]}
    /// ```
    #[test]
    fn requirement_coverage_in_order_with_modules() {
        let mut store = crate::graph::fixtures::store();
        let r = crate::graph::fixtures::req(
            "spans",
            &["criterion:src/a/layout#stable", "criterion:src/a/ready#gone", "criterion:src/a/ready#guarantee", "criterion:src/z/missing#x"],
            &[],
        );
        store.requirements.insert(r.slug.clone(), r.clone());
        let empty = crate::graph::fixtures::req("names-nothing", &[], &[]);
        store.requirements.insert(empty.slug.clone(), empty.clone());
        let g = Graph::build(&store, &crate::extraction::fixtures::extraction(), &[]);

        let cov = requirement_coverage(&g, &r);
        let refs: Vec<String> = cov.iter().map(|c| c.r#ref.to_string()).collect();
        assert_eq!(refs, vec!["criterion:src/a/layout#stable", "criterion:src/a/ready#gone", "criterion:src/a/ready#guarantee", "criterion:src/z/missing#x"], "covered_by order");
        assert_eq!(cov[0].statement.as_deref(), Some("Paths do not move."));
        assert_eq!(cov[0].module, ModuleSummary { r#ref: Reference::module("src/a/layout"), purpose: Some("Lay out the workspace.".into()) });
        // A dangling criterion stays, with no statement, beside its existing module.
        assert_eq!(cov[1].statement, None);
        assert_eq!(cov[1].module.purpose.as_deref(), Some("Decide readiness."));
        // A criterion in a module that does not exist: both null.
        assert_eq!(cov[3].statement, None);
        assert_eq!(cov[3].module, ModuleSummary { r#ref: Reference::module("src/z/missing"), purpose: None });
        assert!(requirement_coverage(&g, &empty).is_empty());
    }
}
