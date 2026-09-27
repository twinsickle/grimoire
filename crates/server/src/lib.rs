//! # Module contract
//!
//! ```json
//! {"purpose":"Run the daemon that owns the store, keeps the graph current, and serves the web UI, the JSON API and MCP from one port.","criteria":[{"id":"single-writer","statement":"Every write to the store goes through this process."},{"id":"no-repository-writes","statement":"The daemon never writes an index, cache or marker into the target repository."}]}
//! ```
//!
//! Scaffold only. See `PROPOSAL.md` at the repository root.
