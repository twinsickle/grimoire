//! Where a person's tokens are kept, under GRIMOIRE_HOME or the platform's configuration directory.
// An integration test is not cfg(test), so clippy's tests exemption misses it.
#![allow(clippy::unwrap_used)]

use grimoire_server::store_io::{credentials_file, HOME_ENV};

/// # Test contract
///
/// ```json
/// {"test": "tokens-are-kept-in-credentials-json-under-home-or-config", "checks": ["crates/server/src/store_io/location#tokens-credentials-file"]}
/// ```
#[test]
fn tokens_are_kept_in_credentials_json_under_home_or_the_configuration_directory() {
    // This binary holds only this test, so changing the environment disturbs no other.
    let home = std::env::temp_dir().join("grimoire-credentials-home");
    std::env::set_var(HOME_ENV, &home);
    assert_eq!(credentials_file().unwrap(), home.join("credentials.json"), "under GRIMOIRE_HOME when it is set");

    std::env::remove_var(HOME_ENV);
    let config = directories::ProjectDirs::from("", "", "grimoire").unwrap().config_dir().to_path_buf();
    assert_eq!(credentials_file().unwrap(), config.join("credentials.json"), "the platform's configuration directory otherwise");
}
