//! # Module contract
//!
//! ```json
//! {"purpose":"Decide who is calling the daemon and what they may do: one step every API, event and MCP request passes, asking a policy, and one check before any tool runs; the local policy for a daemon on one person's machine, the token policy for a shared one.","criteria":[{"id":"one-step","statement":"admit asks the policy for the caller of a request from its headers alone, before any of its body is read, and puts it on the request, or answers 403 with the error object and goes no further, so a refused request reaches no handler; before it refuses it reads and discards the request's body up to 4 MiB, so a client that sends its body late reads the 403 rather than a reset, and answers a body past that with the 403 and the connection closed."},{"id":"local-policy","statement":"The local policy admits a request whose Host is localhost, 127.0.0.1 or [::1] and which no browser marks as from another site or another origin as a local caller who may read and write, and refuses any other with the reason."},{"id":"token-admits","statement":"The token policy admits a request that carries Authorization: Bearer with a token the store's tokens file holds, as a caller named for the token who may read, or read and write, as it says, whatever name its Host gives."},{"id":"token-refuses-other","statement":"The token policy refuses every other request, one with no token, one that is not a bearer token, one the file does not hold, whatever name its Host gives."},{"id":"token-refuses-unreadable","statement":"The token policy refuses any request when the tokens file cannot be read."},{"id":"token-other-site","statement":"Like the local policy, the token policy refuses a request a browser marks as from another site, or whose Origin is not the Host it was sent, over http or https, token or not."},{"id":"token-read-each-time","statement":"The token policy reads the tokens file on each request, so a token revoked is refused at once."},{"id":"writes-need-write","statement":"may_call lets a read tool run for any admitted caller and a write tool only for a caller who may write, and refuses a call with no admitted caller, so a tool is never run for a request that skipped the step."}]}
//! ```

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONNECTION, HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use grimoire_core::ToolError;

/// What a caller may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    ReadWrite,
}

/// Who is calling and what they may do, as the policy answered for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// Who, for messages: `local` under the local policy.
    pub who: String,
    pub access: Access,
}

/// Answers, for a request's headers, who is calling, or why the request is
/// refused. There are two: the local policy, for a daemon on one person's
/// machine, and the token policy, for a shared one.
pub trait Policy: Send + Sync {
    fn admit(&self, headers: &HeaderMap) -> Result<Caller, String>;
}

/// A caller on this machine, not a page in a browser on another site's behalf.
#[derive(Debug, Clone, Copy, Default)]
pub struct Local;

impl Policy for Local {
    fn admit(&self, headers: &HeaderMap) -> Result<Caller, String> {
        match refusal(headers) {
            Some(why) => Err(why),
            None => Ok(Caller { who: "local".to_string(), access: Access::ReadWrite }),
        }
    }
}

/// A caller who signs in to a shared daemon: admitted by the token the
/// request carries and by nothing else, to read or to read and write as the
/// token says. The tokens are read on each request, so one added or revoked
/// takes effect with no restart.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub store_dir: std::path::PathBuf,
}

impl Policy for Tokens {
    fn admit(&self, headers: &HeaderMap) -> Result<Caller, String> {
        // A token is not what a browser sends by itself, but a page on another site is refused all the same,
        // as under the local policy; behind a proxy the daemon's own origin may be https.
        let host = headers.get(HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        if let Some(why) = from_another_site(headers, host, &["http", "https"]) {
            return Err(why);
        }
        let sign_in = "sign in with a token: send `Authorization: Bearer <token>`";
        let Some(given) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
            return Err(format!("this is a shared daemon and the request carries no token; {sign_in}"));
        };
        let Some(token) = given.strip_prefix("Bearer ").map(str::trim) else {
            return Err(format!("the Authorization header is not a bearer token; {sign_in}"));
        };
        match crate::store_io::tokens::lookup(&self.store_dir, token) {
            Ok(Some(t)) => Ok(Caller { who: t.name, access: if t.write { Access::ReadWrite } else { Access::Read } }),
            Ok(None) => Err("the token is not one this daemon holds; it may have been revoked".to_string()),
            Err(e) => Err(format!("the daemon's tokens could not be read, so nobody is admitted: {e}")),
        }
    }
}

/// Why a request is refused under the local policy, if it is: a Host that is
/// not a loopback name (DNS rebinding), a fetch the browser marks as from
/// another site, or an Origin other than the daemon's own. Agents, MCP
/// clients and the CLI send no Origin and no Sec-Fetch-Site, and a loopback Host.
fn refusal(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !is_loopback(host) {
        return Some(format!("Host `{host}` is not a loopback name; the daemon answers only localhost, 127.0.0.1 and [::1]"));
    }
    from_another_site(headers, host, &["http"])
}

/// Why a request is refused as one a browser makes on another site's behalf,
/// under either policy: a fetch the browser marks as from another site, or
/// an Origin that is not the daemon's own under one of `schemes`. A caller
/// that is not a browser sends neither header.
fn from_another_site(headers: &HeaderMap, host: &str, schemes: &[&str]) -> Option<String> {
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if site == "cross-site" || site == "same-site" {
            return Some(format!("a request from another site ({site}) is refused"));
        }
    }
    if let Some(origin) = headers.get(ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !schemes.iter().any(|s| origin == format!("{s}://{host}")) {
            return Some(format!("a request from origin `{origin}` is refused; only the daemon's own origin may call it from a browser"));
        }
    }
    None
}

/// `localhost`, `127.0.0.1` or `[::1]`, with or without a port.
fn is_loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') { rest.split(']').next().map(|n| format!("[{n}]")).unwrap_or_default() } else { host.split(':').next().unwrap_or("").to_string() };
    matches!(name.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

fn forbidden(why: String) -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::to_value(ToolError::invalid(why, vec![])).unwrap_or_default())).into_response()
}

/// The most of a refused request's body the step reads before it answers: the most any handler
/// behind it takes from an admitted caller, MCP's 4 MiB (rmcp's default; the JSON API takes axum's 2 MiB).
const REFUSED_BODY_LIMIT: usize = 4 * 1024 * 1024;

/// The one step: the policy's caller put on the request, or a 403.
pub async fn admit(State(policy): State<Arc<dyn Policy>>, mut req: Request, next: Next) -> Response {
    match policy.admit(req.headers()) {
        Ok(caller) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(why) => {
            // Read what the caller is still sending before answering: a connection closed with its
            // body unread is reset, and the client's next read loses the 403 to that reset.
            if axum::body::to_bytes(req.into_body(), REFUSED_BODY_LIMIT).await.is_ok() {
                forbidden(why)
            } else {
                let mut answer = forbidden(why);
                answer.headers_mut().insert(CONNECTION, HeaderValue::from_static("close"));
                answer
            }
        }
    }
}

/// Whether `caller` may run `tool`. The JSON API and MCP both ask this
/// before dispatch; a call with no caller skipped the step and is refused.
pub fn may_call(caller: Option<&Caller>, tool: &str) -> Result<(), ToolError> {
    let Some(caller) = caller else {
        return Err(ToolError::invalid("the call was not admitted by the daemon's trust step", vec![]));
    };
    if grimoire_core::tools::is_write(tool) && caller.access != Access::ReadWrite {
        return Err(ToolError::invalid(format!("`{tool}` writes the store, and caller `{}` may only read", caller.who), vec![]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-local-policy-admits-this-machine-only", "checks": ["crates/server/src/access#local-policy"]}
    /// ```
    #[test]
    fn the_local_policy_admits_this_machine_only() {
        let local = Caller { who: "local".into(), access: Access::ReadWrite };
        // Agents: no Origin, no Sec-Fetch-Site, a loopback Host with or without a port.
        for host in ["127.0.0.1", "127.0.0.1:4242", "localhost:4242", "LOCALHOST", "[::1]:4242"] {
            assert_eq!(Local.admit(&headers(&[("host", host)])), Ok(local.clone()), "{host}");
        }
        // The daemon's own UI, and the dev server proxying to it.
        assert_eq!(Local.admit(&headers(&[("host", "127.0.0.1:4242"), ("origin", "http://127.0.0.1:4242"), ("sec-fetch-site", "same-origin")])), Ok(local.clone()));
        assert_eq!(Local.admit(&headers(&[("host", "localhost:4242"), ("sec-fetch-site", "none")])), Ok(local), "typed into the address bar");
        // Another site, a rebound name, a request marked cross-site, a missing Host.
        for refused in [
            headers(&[("host", "127.0.0.1:4242"), ("origin", "https://example.com")]),
            headers(&[("host", "127.0.0.1:4242"), ("origin", "null")]),
            headers(&[("host", "evil.example:4242")]),
            headers(&[("host", "127.0.0.1.evil.example")]),
            headers(&[("host", "127.0.0.1:4242"), ("sec-fetch-site", "cross-site")]),
            headers(&[("host", "127.0.0.1:4242"), ("sec-fetch-site", "same-site")]),
            HeaderMap::new(),
        ] {
            assert!(Local.admit(&refused).is_err(), "{refused:?}");
        }
    }

    struct Nobody;

    impl Policy for Nobody {
        fn admit(&self, _: &HeaderMap) -> Result<Caller, String> {
            Err("no one".into())
        }
    }

    /// One request through `admit` in front of a handler that records whether it ran.
    async fn through_the_step(policy: Arc<dyn Policy>) -> (u16, bool, Option<Caller>) {
        use std::io::{Read, Write};
        use std::sync::Mutex;
        let seen: Arc<Mutex<Option<Option<Caller>>>> = Arc::default();
        let handler_saw = seen.clone();
        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move |caller: Option<axum::Extension<Caller>>| async move {
                    *handler_saw.lock().unwrap() = Some(caller.map(|axum::Extension(c)| c));
                    "ran"
                }),
            )
            .layer(axum::middleware::from_fn_with_state(policy, admit));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        let answer = tokio::task::spawn_blocking(move || {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                s,
                "GET /x HTTP/1.0
Host: 127.0.0.1

"
            )
            .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        })
        .await
        .unwrap();
        let code = answer.split_whitespace().nth(1).unwrap().parse().unwrap();
        let ran = seen.lock().unwrap().clone();
        (code, ran.is_some(), ran.flatten())
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-step-runs-before-any-handler", "checks": ["crates/server/src/access#one-step"]}
    /// ```
    #[tokio::test(flavor = "multi_thread")]
    async fn the_step_runs_before_any_handler() {
        assert_eq!(through_the_step(Arc::new(Nobody)).await, (403, false, None), "refused: the handler never ran");
        let (code, ran, caller) = through_the_step(Arc::new(Local)).await;
        assert_eq!((code, ran), (200, true));
        assert_eq!(caller, Some(Caller { who: "local".into(), access: Access::ReadWrite }), "the handler sees the caller the step put on the request");
    }

    /// A refusing policy's step in front of a POST handler that records whether it ran.
    fn refusing_router(ran: Arc<std::sync::atomic::AtomicBool>, policy: Arc<dyn Policy>) -> axum::Router {
        axum::Router::new()
            .route(
                "/x",
                axum::routing::post(move || {
                    ran.store(true, std::sync::atomic::Ordering::SeqCst);
                    async { "ran" }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(policy, admit))
    }

    /// Refuses everyone, and says each time it has.
    struct Says(std::sync::Mutex<std::sync::mpsc::Sender<()>>);

    impl Policy for Says {
        fn admit(&self, _: &HeaderMap) -> Result<Caller, String> {
            let _ = self.0.lock().unwrap().send(());
            Err("no one".into())
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-refused-caller-sending-its-body-late-reads-the-403", "checks": ["crates/server/src/access#one-step"]}
    /// ```
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_caller_sending_its_body_late_reads_the_403() {
        use std::io::{Read, Write};
        use std::time::Duration;
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (said, heard) = std::sync::mpsc::channel();
        let app = refusing_router(ran.clone(), Arc::new(Says(std::sync::Mutex::new(said))));
        tokio::spawn(async move { axum::serve(listener, app).await });
        let (early, answer) = tokio::task::spawn_blocking(move || {
            let body = br#"{"name":"ana"}"#;
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST /x HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            // The headers alone: once the policy has refused, the step waits for the body rather
            // than answering and closing, as it did before it read the body.
            heard.recv_timeout(Duration::from_secs(10)).expect("the policy was asked");
            s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
            let mut first = [0u8; 1];
            let early = match s.read(&mut first) {
                Ok(n) => Some(n),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => None,
                Err(e) => panic!("the connection failed before the body was sent: {e}"),
            };
            s.write_all(body).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).expect("the 403 is read, not a reset");
            (early, out)
        })
        .await
        .unwrap();
        assert_eq!(early, None, "nothing is answered before the body arrives");
        assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
        assert!(answer.contains("\"error\":\"invalid\""), "the error object: {answer}");
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst), "no handler ran");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-refused-body-past-the-limit-is-answered-and-closed", "checks": ["crates/server/src/access#one-step"]}
    /// ```
    #[tokio::test]
    async fn a_refused_body_past_the_limit_is_answered_and_closed() {
        use tower::ServiceExt;
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let send = |size: usize| {
            let app = refusing_router(ran.clone(), Arc::new(Nobody));
            async move { app.oneshot(Request::post("/x").body(axum::body::Body::from(vec![b' '; size])).unwrap()).await.unwrap() }
        };
        let within = send(REFUSED_BODY_LIMIT).await;
        assert_eq!(within.status(), StatusCode::FORBIDDEN);
        assert!(within.headers().get(CONNECTION).is_none(), "a body read to its end leaves the connection to the client");
        let past = send(REFUSED_BODY_LIMIT + 1).await;
        assert_eq!(past.status(), StatusCode::FORBIDDEN);
        assert_eq!(past.headers().get(CONNECTION).map(|v| v.as_bytes()), Some(&b"close"[..]), "past the limit the connection is closed");
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst), "no handler ran");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "writes-need-a-caller-who-may-write", "checks": ["crates/server/src/access#writes-need-write"]}
    /// ```
    #[test]
    fn writes_need_a_caller_who_may_write() {
        let reader = Caller { who: "reader".into(), access: Access::Read };
        let writer = Caller { who: "local".into(), access: Access::ReadWrite };
        assert!(may_call(Some(&reader), "get").is_ok());
        assert!(may_call(Some(&reader), "put_page").unwrap_err().to_string().contains("may only read"));
        assert!(may_call(Some(&writer), "put_page").is_ok());
        for tool in ["get", "put_page"] {
            assert!(may_call(None, tool).is_err(), "no caller, no tool: {tool}");
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "the-token-policy-admits-by-token-and-refuses-another-site", "checks": ["crates/server/src/access#token-admits", "crates/server/src/access#token-refuses-other", "crates/server/src/access#token-refuses-unreadable", "crates/server/src/access#token-other-site", "crates/server/src/access#token-read-each-time"]}
    /// ```
    #[test]
    fn the_token_policy_admits_by_token_and_refuses_another_site() {
        use crate::store_io::tokens;
        let tmp = tempfile::Builder::new().prefix("grimoire-access-").tempdir().unwrap();
        std::fs::write(tmp.path().join(crate::store_io::CONFIG_FILE), "{}").unwrap();
        let policy = Tokens { store_dir: tmp.path().to_path_buf() };
        let ana = tokens::add(tmp.path(), "ana", true).unwrap();
        let reader = tokens::add(tmp.path(), "reader", false).unwrap();
        let bearer = |t: &str| format!("Bearer {t}");
        // Admitted by the token, to what the token says, whatever name the request was addressed to.
        assert_eq!(policy.admit(&headers(&[("host", "grimoire.example"), ("authorization", &bearer(&ana))])), Ok(Caller { who: "ana".into(), access: Access::ReadWrite }));
        assert_eq!(
            policy.admit(&headers(&[("host", "grimoire.example"), ("origin", "https://grimoire.example"), ("sec-fetch-site", "same-origin"), ("authorization", &bearer(&reader))])),
            Ok(Caller { who: "reader".into(), access: Access::Read }),
            "the daemon's own origin, behind a proxy"
        );
        // A page on another site is refused though it carries a token that is held.
        assert!(policy.admit(&headers(&[("host", "grimoire.example"), ("origin", "https://elsewhere.example"), ("authorization", &bearer(&reader))])).unwrap_err().contains("origin"));
        assert!(policy.admit(&headers(&[("host", "grimoire.example"), ("sec-fetch-site", "cross-site"), ("authorization", &bearer(&ana))])).unwrap_err().contains("another site"));
        // The daemon's own origin is the Host it is sent: behind a proxy that rewrites Host, a browser is refused and a command is not.
        assert!(policy.admit(&headers(&[("host", "127.0.0.1:4242"), ("origin", "https://grimoire.example"), ("authorization", &bearer(&reader))])).unwrap_err().contains("origin"));
        assert!(policy.admit(&headers(&[("host", "127.0.0.1:4242"), ("authorization", &bearer(&reader))])).is_ok());
        // A caller on this machine with no token is not admitted: a loopback Host signs nobody in.
        assert!(policy.admit(&headers(&[("host", "127.0.0.1")])).unwrap_err().contains("no token"));
        assert!(policy.admit(&headers(&[("host", "127.0.0.1"), ("authorization", &format!("Basic {ana}"))])).unwrap_err().contains("not a bearer token"));
        assert!(policy.admit(&headers(&[("host", "127.0.0.1"), ("authorization", "Bearer grm_unknown")])).unwrap_err().contains("not one this daemon holds"));
        // Read on each request: revoked now, refused now.
        tokens::revoke(tmp.path(), "ana").unwrap();
        assert!(policy.admit(&headers(&[("authorization", &bearer(&ana))])).is_err());
        assert!(policy.admit(&headers(&[("authorization", &bearer(&reader))])).is_ok());
        // A tokens file that cannot be read admits nobody.
        std::fs::write(tmp.path().join(tokens::TOKENS_FILE), "{ not json").unwrap();
        assert!(policy.admit(&headers(&[("authorization", &bearer(&reader))])).unwrap_err().contains("nobody is admitted"));
    }
}
