//! What of the image a team's server runs from the code can show without Docker:
//! the Dockerfile builds the interface and the binary in stages of their own and
//! keeps only the binary, run by a user of its own over a library volume.
#![allow(clippy::unwrap_used)]

use std::path::Path;

/// # Test contract
///
/// ```json
/// {"test": "the-image-serves-a-teams-library", "checks": ["req:server-container"]}
/// ```
#[test]
fn the_image_serves_a_teams_library() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap();
    let dockerfile = read("Dockerfile");
    let stages: Vec<&str> = dockerfile.lines().filter(|l| l.starts_with("FROM ")).collect();
    // The interface with Node and the binary with Rust, each in a stage of its own, then the image.
    assert_eq!(stages.len(), 3, "{stages:?}");
    assert!(stages[0].starts_with("FROM node:") && stages[0].ends_with(" AS web"), "{stages:?}");
    assert!(stages[1].starts_with("FROM rust:") && stages[1].ends_with(" AS build"), "{stages:?}");
    assert!(dockerfile.contains("RUN npm ci") && dockerfile.contains("RUN npm run build"), "the interface is built in the image");
    assert!(dockerfile.contains("COPY --from=web /src/web/dist web/dist"), "the binary embeds the interface built");
    assert!(dockerfile.contains("cargo build --release --locked -p grimoire-cli"), "a release build");
    // The image holds the binary alone, from the build stage, on the path.
    let last = &dockerfile[dockerfile.rfind("FROM ").unwrap()..];
    let copies: Vec<&str> = last.lines().filter(|l| l.starts_with("COPY")).collect();
    assert_eq!(copies, ["COPY --from=build /usr/local/bin/grimoire /usr/local/bin/grimoire"]);
    // Run by a user of its own, who owns the library volume, serving it as a team's library.
    assert!(last.contains("useradd --system --uid 10001") && last.contains("chown grimoire:grimoire /data") && last.contains("USER grimoire"), "{last}");
    assert!(last.contains("VOLUME /data") && last.contains("EXPOSE 4243"), "{last}");
    assert!(last.contains(r#"CMD ["grimoire", "serve", "--shared", "--library", "/data", "--listen", "0.0.0.0:4243"]"#), "{last}");
    // What the binary is built from reaches the build, and nothing built on the host does.
    let ignore = read(".dockerignore");
    for kept in ["!Cargo.toml", "!Cargo.lock", "!crates", "!web", "!.claude/skills"] {
        assert!(ignore.lines().any(|l| l == kept), ".dockerignore keeps {kept}");
    }
    for left in ["**/target", "web/node_modules", "web/dist"] {
        assert!(ignore.lines().any(|l| l == left), ".dockerignore leaves out {left}");
    }
    // How to run it is written where a team reads it.
    assert!(read("compose.yaml").contains("Running a team's server"));
}

/// Run `docker` with these arguments; its output, failing the test when it fails.
fn docker(args: &[&str]) -> String {
    let out = std::process::Command::new("docker").args(args).output().expect("docker runs");
    assert!(out.status.success(), "docker {args:?}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Removes what the test made in Docker, however it ends.
struct Made(Vec<Vec<String>>);

impl Drop for Made {
    fn drop(&mut self) {
        for args in &self.0 {
            let _ = std::process::Command::new("docker").args(args).output();
        }
    }
}

/// # Test contract
///
/// ```json
/// {"test": "the-image-runs-as-a-teams-server", "checks": ["req:server-container", "crates/server/src#graceful-stop"]}
/// ```
#[test]
#[ignore = "builds and runs the image: needs Docker; run with `cargo test -p grimoire-cli --test image -- --ignored`"]
fn the_image_runs_as_a_teams_server() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tag = format!("grimoire:image-test-{}", std::process::id());
    let (volume, container) = (format!("grimoire-image-test-{}", std::process::id()), format!("grimoire-image-test-{}", std::process::id()));
    let _made = Made(vec![vec!["rm".into(), "-f".into(), container.clone()], vec!["volume".into(), "rm".into(), volume.clone()], vec!["image".into(), "rm".into(), tag.clone()]]);
    docker(&["build", "-q", "-t", &tag, &root.to_string_lossy()]);
    // A store made on a machine, put into a volume the way the runbook says.
    let tmp = tempfile::Builder::new().prefix("grimoire-image-").tempdir().unwrap();
    let (repo, store) = (tmp.path().join("repo"), tmp.path().join("shop"));
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"shop\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src").join("lib.rs"), "pub fn a() {}\n").unwrap();
    let init = std::process::Command::new(env!("CARGO_BIN_EXE_grimoire")).args(["init", "--store", &store.to_string_lossy()]).current_dir(&repo).output().unwrap();
    assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));
    docker(&["volume", "create", &volume]);
    let from = format!("{}:/from:ro", store.to_string_lossy());
    docker(&["run", "--rm", "-v", &format!("{volume}:/data"), "-v", &from, &tag, "sh", "-c", "cp -r /from /data/.incoming && rm -f /data/.incoming/serve.json && mv /data/.incoming /data/shop"]);
    // Run as the runbook runs it, on a port of the system's choosing.
    docker(&["run", "-d", "--name", &container, "-p", "127.0.0.1::4243", "-v", &format!("{volume}:/data"), &tag]);
    let port: u16 = docker(&["port", &container, "4243"]).lines().next().unwrap().rsplit(':').next().unwrap().parse().unwrap();
    assert_eq!(docker(&["exec", &container, "id", "-u"]), "10001", "the server runs as a user of its own");
    let token = docker(&["exec", &container, "grimoire", "token", "add", "ann", "--store", "/data/shop"]);
    let status = |token: &str| {
        use std::io::{Read, Write};
        let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) else { return 0 };
        let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
        let _ = write!(s, "POST /g/shop/api/status HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}");
        let mut answer = String::new();
        let _ = s.read_to_string(&mut answer);
        answer.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while status(&token) != 200 {
        assert!(std::time::Instant::now() < deadline, "shop answers with its token within thirty seconds");
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert_eq!(status("not-a-token"), 403, "and refuses a caller without one");
    assert!(docker(&["exec", &container, "ls", "/data/shop"]).lines().any(|f| f == "serve.json"), "marked while it serves");
    // docker stop sends SIGTERM: the server stops well within the grace period, its marker removed.
    let stopping = std::time::Instant::now();
    docker(&["stop", &container]);
    assert!(stopping.elapsed() < std::time::Duration::from_secs(8), "stopped by SIGTERM, not killed at the end of the grace period: {:?}", stopping.elapsed());
    let left = docker(&["run", "--rm", "-v", &format!("{volume}:/data"), &tag, "ls", "/data/shop"]);
    assert!(!left.lines().any(|f| f == "serve.json"), "the marker is removed: {left}");
}
