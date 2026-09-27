//! # Module contract
//!
//! ```json
//! {"purpose":"Read a repository's source and produce its files, declarations, imports and module contracts as values.","criteria":[{"id":"read-only","statement":"The extractor reads the paths it is given and writes nothing anywhere."},{"id":"per-file","statement":"Extraction is per file, so one changed file is re-extracted without touching the others."},{"id":"no-guessed-calls","statement":"The extractor reports declarations and imports it parsed and never infers a call edge by name."}]}
//! ```
//!
//! Scaffold only. See `PROPOSAL.md` at the repository root.
