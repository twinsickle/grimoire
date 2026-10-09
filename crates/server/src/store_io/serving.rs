//! # Module contract
//!
//! ```json
//! {"purpose":"Keep serve.json, the marker that names the daemon serving a store: a hint a command reads to defer its write, never trusted without asking the daemon.","criteria":[{"id":"marker-names-daemon","statement":"write_serving_in writes serve.json naming this process and the port, with the host when a command on this machine does not reach the daemon at 127.0.0.1, and the path in the library when one is given."},{"id":"marker-every-address","statement":"A daemon bound to every address is marked as reached by the loopback of the same family: no host for IPv4, ::1 for IPv6."},{"id":"marker-address","statement":"Serving::addr gives where a command on this machine reaches the daemon: the marker's host, else the IPv4 loopback, at its port."},{"id":"marker-removed-by-its-process","statement":"remove_serving removes serve.json only when it names this process, leaving another process's marker alone."},{"id":"marker-read-as-hint","statement":"read_serving gives the marker, or none when there is none or it cannot be read."}]}
//! ```

use std::path::Path;

use super::{read_json, to_pretty, write_atomic, StoreIoError, SERVE_FILE};

/// `serve.json`: which daemon serves this store. A hint, never trusted
/// without asking the daemon itself (see `client::serving`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Serving {
    pub port: u16,
    pub pid: u32,
    /// Where a command on this machine reaches it, when that is not 127.0.0.1: the
    /// address it bound, or `::1` when it listens on every IPv6 address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Where in the daemon's library this store's grimoire is served, `/g/<name>`; none
    /// for a daemon serving it on the bare routes (req:machine-library).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Serving {
    /// Where a command on this machine reaches the daemon: the address it
    /// bound, or the loopback when it listens there or on every address.
    pub fn addr(&self) -> std::net::SocketAddr {
        let ip = self.host.as_deref().and_then(|h| h.parse().ok()).unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        std::net::SocketAddr::new(ip, self.port)
    }
}

/// Record that this process serves the store on `port` of this machine's loopback.
pub fn write_serving(store_dir: &Path, port: u16) -> Result<(), StoreIoError> {
    write_serving_at(store_dir, std::net::SocketAddr::from(([127, 0, 0, 1], port)))
}

/// Record that this process serves the store at `addr`.
pub fn write_serving_at(store_dir: &Path, addr: std::net::SocketAddr) -> Result<(), StoreIoError> {
    write_serving_in(store_dir, addr, None)
}

/// Record that this process serves the store at `addr`, its grimoire at `path` in the library.
pub fn write_serving_in(store_dir: &Path, addr: std::net::SocketAddr, path: Option<&str>) -> Result<(), StoreIoError> {
    let ip = addr.ip();
    let host = if ip.is_unspecified() {
        // Every address: reached on this machine by the loopback of the same family.
        ip.is_ipv6().then(|| std::net::Ipv6Addr::LOCALHOST.to_string())
    } else {
        (ip != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)).then(|| ip.to_string())
    };
    write_atomic(&store_dir.join(SERVE_FILE), &to_pretty(&Serving { port: addr.port(), pid: std::process::id(), host, path: path.map(str::to_string) }))
}

/// Remove the marker, if it is still this process's.
pub fn remove_serving(store_dir: &Path) {
    if read_serving(store_dir).is_some_and(|s| s.pid == std::process::id()) {
        let _ = std::fs::remove_file(store_dir.join(SERVE_FILE));
    }
}

pub fn read_serving(store_dir: &Path) -> Option<Serving> {
    read_json(&store_dir.join(SERVE_FILE)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-serving-marker-is-this-processs-to-remove", "checks": ["crates/server/src/store_io/serving#marker-names-daemon", "crates/server/src/store_io/serving#marker-removed-by-its-process", "crates/server/src/store_io/serving#marker-read-as-hint"]}
    /// ```
    #[test]
    fn the_serving_marker_is_this_process_s_to_remove() {
        let tmp = temp();
        let dir = tmp.path();
        write_serving(dir, 4242).unwrap();
        assert_eq!(read_serving(dir), Some(Serving { port: 4242, pid: std::process::id(), host: None, path: None }));
        remove_serving(dir);
        assert!(read_serving(dir).is_none());
        std::fs::write(dir.join(SERVE_FILE), r#"{"port":4242,"pid":1}"#).unwrap();
        remove_serving(dir);
        assert!(read_serving(dir).is_some(), "another process's marker is left alone");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-marker-says-where-a-command-reaches-the-daemon", "checks": ["crates/server/src/store_io/serving#marker-names-daemon", "crates/server/src/store_io/serving#marker-every-address", "crates/server/src/store_io/serving#marker-address", "crates/server/src/store_io/serving#marker-read-as-hint", "req:daemon-serving-marker"]}
    /// ```
    #[test]
    fn the_marker_says_where_a_command_reaches_the_daemon() {
        let tmp = temp();
        let dir = tmp.path();
        let at = |addr: &str, path: Option<&str>| {
            write_serving_in(dir, addr.parse().unwrap(), path).unwrap();
            read_serving(dir).unwrap()
        };
        // The loopback: no host; a library's grimoire: its path.
        let m = at("127.0.0.1:4242", Some("/g/ledger"));
        assert_eq!((m.host.as_deref(), m.path.as_deref(), m.addr().to_string()), (None, Some("/g/ledger"), "127.0.0.1:4242".to_string()));
        // Every IPv4 address: reached on the IPv4 loopback, so no host.
        let m = at("0.0.0.0:4242", None);
        assert_eq!((m.host.as_deref(), m.addr().to_string()), (None, "127.0.0.1:4242".to_string()));
        // Every IPv6 address: reached on ::1.
        let m = at("[::]:4242", None);
        assert_eq!((m.host.as_deref(), m.addr().to_string()), (Some("::1"), "[::1]:4242".to_string()));
        // A particular address: named, and reached there.
        let m = at("192.168.1.20:4242", None);
        assert_eq!((m.host.as_deref(), m.addr().to_string()), (Some("192.168.1.20"), "192.168.1.20:4242".to_string()));
        // A marker that cannot be read is none.
        std::fs::write(dir.join(SERVE_FILE), "not json").unwrap();
        assert!(read_serving(dir).is_none());
    }
}
