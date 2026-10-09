//! # Module contract
//!
//! ```json
//! {"purpose":"Keep the tokens a shared daemon admits, in a file beside its store: for each a name, whether it may write, and a digest of the token, never the token.","criteria":[{"id":"tokens-digest-only","statement":"add makes a token from the operating system's random source, returns it once and stores only its name, whether it may write and its SHA-256 digest."},{"id":"tokens-locked-atomic","statement":"The tokens file is written atomically and read and rewritten under a lock of its own, tokens.lock, so two changes at once both hold."},{"id":"tokens-name-slug","statement":"A token's name is a slug used once."},{"id":"tokens-not-a-store","statement":"add refuses a directory that is not a store, or stopped being one while the command waited for the lock, and gains no file."},{"id":"tokens-revoke","statement":"revoke removes a token by name."},{"id":"lookup","statement":"lookup answers the name and access of the token whose digest the file holds and nothing for any other; a store with no tokens file holds none, and a file that does not parse is an error naming it, never read as holding none."}]}
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{to_pretty, write_atomic, StoreIoError, CONFIG_FILE};

pub const TOKENS_FILE: &str = "tokens.json";
/// The empty file a command changing the tokens locks, so two at once cannot lose one change.
pub const TOKENS_LOCK: &str = "tokens.lock";

/// One token a shared daemon admits: who it is for, what it may do, and the
/// digest of the token itself, which is never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Token {
    pub name: String,
    pub write: bool,
    pub digest: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tokens {
    tokens: Vec<Token>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn bad(store_dir: &Path, why: impl Into<String>) -> StoreIoError {
    StoreIoError::BadTokens(store_dir.join(TOKENS_FILE), why.into())
}

/// The tokens the store holds; none when it has no tokens file.
pub fn list(store_dir: &Path) -> Result<Vec<Token>, StoreIoError> {
    let path = store_dir.join(TOKENS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreIoError::Io(path, e)),
    };
    let parsed: Tokens = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| bad(store_dir, e.to_string()))?;
    Ok(parsed.tokens)
}

/// Held while the tokens file is read and rewritten. It is a lock of its own,
/// not the store's, which a serving daemon holds: tokens are added and
/// revoked while it serves. Waits for another holder; released when dropped.
fn locked(store_dir: &Path) -> Result<std::fs::File, StoreIoError> {
    // Only a store has tokens: no lock file is made in a folder that is not one, or is no longer.
    let not_a_store = || StoreIoError::NotAStore(store_dir.to_path_buf());
    if !store_dir.join(CONFIG_FILE).is_file() {
        return Err(not_a_store());
    }
    let path = store_dir.join(TOKENS_LOCK);
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path).map_err(|e| StoreIoError::Io(path.clone(), e))?;
    file.lock().map_err(|e| StoreIoError::Io(path, e))?;
    // Asked again once the lock is held: the store may have been deleted while this waited.
    if !store_dir.join(CONFIG_FILE).is_file() {
        return Err(not_a_store());
    }
    Ok(file)
}

/// The tokens' lock, for a command that removes the store: held with the
/// store's own, taken after it, so no token is being written into a folder
/// that is going. Released when dropped.
pub fn lock(store_dir: &Path) -> Result<std::fs::File, StoreIoError> {
    locked(store_dir)
}

fn save(store_dir: &Path, tokens: Vec<Token>) -> Result<(), StoreIoError> {
    write_atomic(&store_dir.join(TOKENS_FILE), &to_pretty(&Tokens { tokens }))
}

/// Make a token named `name` that may read, or read and write, and return
/// it. This is the only time the token exists outside its holder's hands:
/// the store keeps its digest.
pub fn add(store_dir: &Path, name: &str, write: bool) -> Result<String, StoreIoError> {
    if !grimoire_core::store::is_slug(name) {
        return Err(bad(store_dir, format!("`{name}` is not a slug: lower-case letters, digits and single hyphens")));
    }
    let _held = locked(store_dir)?;
    let mut tokens = list(store_dir)?;
    if tokens.iter().any(|t| t.name == name) {
        return Err(bad(store_dir, format!("a token named `{name}` exists; revoke it before making another")));
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| bad(store_dir, format!("the operating system gave no random bytes: {e}")))?;
    let token = format!("grm_{}", hex(&bytes));
    tokens.push(Token { name: name.to_string(), write, digest: digest(&token) });
    save(store_dir, tokens)?;
    Ok(token)
}

/// Remove the token named `name`; false when there was none.
pub fn revoke(store_dir: &Path, name: &str) -> Result<bool, StoreIoError> {
    let _held = locked(store_dir)?;
    let mut tokens = list(store_dir)?;
    let before = tokens.len();
    tokens.retain(|t| t.name != name);
    if tokens.len() == before {
        return Ok(false);
    }
    save(store_dir, tokens)?;
    Ok(true)
}

/// Whose token this is, if the store holds its digest.
pub fn lookup(store_dir: &Path, token: &str) -> Result<Option<Token>, StoreIoError> {
    let digest = digest(token);
    Ok(list(store_dir)?.into_iter().find(|t| t.digest == digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> tempfile::TempDir {
        let tmp = tempfile::Builder::new().prefix("grimoire-tokens-").tempdir().unwrap();
        std::fs::write(tmp.path().join(CONFIG_FILE), "{}").unwrap();
        tmp
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-token-is-returned-once-and-kept-as-a-digest", "checks": ["crates/server/src/store_io/tokens#tokens-digest-only", "crates/server/src/store_io/tokens#tokens-locked-atomic", "crates/server/src/store_io/tokens#tokens-name-slug", "crates/server/src/store_io/tokens#tokens-not-a-store", "crates/server/src/store_io/tokens#tokens-revoke"]}
    /// ```
    #[test]
    fn a_token_is_returned_once_and_kept_as_a_digest() {
        let tmp = store();
        let dir = tmp.path();
        let ana = add(dir, "ana", true).unwrap();
        let check = add(dir, "pipeline-check", false).unwrap();
        assert!(ana.starts_with("grm_") && ana.len() == 4 + 64 && ana != check, "32 random bytes each: {ana}");
        // The file holds the names, what each may do and a digest; neither token is in it.
        let text = std::fs::read_to_string(dir.join(TOKENS_FILE)).unwrap();
        assert!(!text.contains(&ana) && !text.contains(&check) && !text.contains(&ana[4..]), "{text}");
        let held = list(dir).unwrap();
        assert_eq!(held.iter().map(|t| (t.name.as_str(), t.write)).collect::<Vec<_>>(), vec![("ana", true), ("pipeline-check", false)]);
        assert!(held.iter().all(|t| t.digest.len() == 64 && t.digest != ana[4..]));
        // No temporary file is left beside it.
        let names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names.len(), 3, "the store's config, the tokens and their lock: {names:?}");
        // Changed from several places at once, every change holds.
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let dir = dir.to_path_buf();
                std::thread::spawn(move || add(&dir, &format!("worker-{i}"), i % 2 == 0).unwrap())
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(list(dir).unwrap().iter().filter(|t| t.name.starts_with("worker-")).count(), 8, "no addition was lost");
        for i in 0..8 {
            assert!(revoke(dir, &format!("worker-{i}")).unwrap());
        }
        // A name is a slug, used once.
        assert!(add(dir, "ana", false).unwrap_err().to_string().contains("exists"));
        assert!(add(dir, "Ana B", true).unwrap_err().to_string().contains("not a slug"));
        // Revoking removes one and leaves the other.
        assert!(revoke(dir, "ana").unwrap());
        assert!(!revoke(dir, "ana").unwrap(), "already gone");
        assert_eq!(list(dir).unwrap().len(), 1);
        // A folder that is not a store takes no tokens.
        let other = tempfile::tempdir().unwrap();
        assert!(matches!(add(other.path(), "ana", true), Err(StoreIoError::NotAStore(_))));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "only-a-held-token-is-found", "checks": ["crates/server/src/store_io/tokens#lookup"]}
    /// ```
    #[test]
    fn only_a_held_token_is_found() {
        let tmp = store();
        let dir = tmp.path();
        assert_eq!(lookup(dir, "grm_anything").unwrap(), None, "no file: no tokens");
        let ana = add(dir, "ana", true).unwrap();
        let reader = add(dir, "reader", false).unwrap();
        assert_eq!(lookup(dir, &ana).unwrap().map(|t| (t.name, t.write)), Some(("ana".to_string(), true)));
        assert_eq!(lookup(dir, &reader).unwrap().map(|t| (t.name, t.write)), Some(("reader".to_string(), false)));
        // Not another token, not a name, not a digest presented as a token.
        let held = list(dir).unwrap();
        for wrong in ["grm_0000", "ana", held[0].digest.as_str(), ""] {
            assert_eq!(lookup(dir, wrong).unwrap(), None, "{wrong}");
        }
        revoke(dir, "ana").unwrap();
        assert_eq!(lookup(dir, &ana).unwrap(), None, "revoked");
        // A file that does not parse is an error, never an empty list that would look like nobody may sign in, or worse.
        std::fs::write(dir.join(TOKENS_FILE), "{ not json").unwrap();
        assert!(matches!(lookup(dir, &reader), Err(StoreIoError::BadTokens(..))));
    }
}
