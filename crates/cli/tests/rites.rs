//! What of this repository's rites the code can show. A rite is held by the
//! work, which each ticket's closing decision shows, and its procedure is
//! written in the rites themselves; a test here checks only what is in code.
#![allow(clippy::unwrap_used)]

use std::path::Path;

/// # Test contract
///
/// ```json
/// {"test": "this-repository-is-worked-on-in-the-setup-a-team-uses", "checks": ["req:built-in-the-setup-it-is-used-in"]}
/// ```
#[test]
fn this_repository_is_worked_on_in_the_setup_a_team_uses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap();
    // Its own grimoire is a server's: the project file names one, and no store, its grimoire in the server's library.
    let project = read(".grimoire.toml");
    let named: Vec<&str> = project.lines().filter(|l| !l.trim_start().starts_with('#') && l.contains('=')).map(|l| l.split('=').next().unwrap().trim()).collect();
    assert!(named.contains(&"server") && !named.contains(&"store"), "{project}");
    assert!(project.contains("/g/grimoire\""), "the address names the grimoire in the library: {project}");
    // The server runs as a team runs it: the image this checkout builds, serving a library on a named
    // volume with this project's site beside it, or on the fast path a debug build made in a container.
    let compose = read("compose.yaml");
    for phrase in ["build: .", "\"--shared\", \"--library\", \"/data\"", "\"--site-of\", \"grimoire\"", "library:/data"] {
        assert!(compose.contains(phrase), "compose.yaml holds {phrase:?}");
    }
    let fast = read("compose.fast.yaml");
    for phrase in ["cargo build --locked -p grimoire-cli --target-dir /target", "PATH: /target/debug", "service_completed_successfully", "user: \"10001:10001\""] {
        assert!(fast.contains(phrase), "compose.fast.yaml holds {phrase:?}");
    }
    // The loop starts that server from the compose files, at the address the project file names, before the
    // daemon that reads from it; follows its log across a restart; stops it; and signs its person in with a
    // token made inside the container.
    let dev = read("scripts/dev.mjs");
    for phrase in [
        "\"compose.yaml\"",
        "\"compose.fast.yaml\"",
        "GRIMOIRE_DEV_SERVER",
        ".grimoire.toml",
        "[\"up\", \"--detach\", \"--build\", \"server\"]",
        "[\"logs\", \"--follow\"",
        "follow(start(server))",
        "[\"stop\", \"server\"]",
        "[\"exec\", \"-T\", \"server\", \"grimoire\", \"token\", \"add\", name,",
        "[\"login\"]",
        "refused this machine",
    ] {
        assert!(dev.contains(phrase), "scripts/dev.mjs holds {phrase:?}");
    }
    let (server, rest) = (dev.find("follow(start(server))").unwrap(), dev.find("processes.forEach(start)").unwrap());
    assert!(server < rest, "the server is started before the daemon and the interface");
    // Signing in never takes a token from anyone: the loop revokes none.
    assert!(!dev.contains("\"revoke\""), "scripts/dev.mjs revokes no token");

    // The setup of one person with a store and no server is held by tests, this repository no longer
    // being worked in it: the command line's own, and the browser tests' fixture, which init gives a store.
    assert!(read("crates/cli/tests/cli.rs").contains("\"init\", \"--store\""));
    assert!(read("web/behaviour/serve.mjs").contains("grimoire(\"init\", \"--store\", store"));
}
