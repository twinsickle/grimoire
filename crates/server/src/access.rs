//! # Module contract
//!
//! ```json
//! {"purpose":"Decide who is calling the daemon and what they may do: one step every API, event and MCP request passes, asking a policy, and one check before any tool runs; the local policy is the only one today.","criteria":[{"id":"one-step","statement":"admit asks the policy for the caller of a request and puts it on the request, or answers 403 with the error object and goes no further, so a refused request reaches no handler."},{"id":"local-policy","statement":"The local policy admits a request whose Host is localhost, 127.0.0.1 or [::1] and which no browser marks as from another site or another origin as a local caller who may read and write, and refuses any other with the reason."},{"id":"writes-need-write","statement":"may_call lets a read tool run for any admitted caller and a write tool only for a caller who may write, and refuses a call with no admitted caller, so a tool is never run for a request that skipped the step."}]}
//! ```

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, StatusCode};
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
/// refused. The local policy is the one there is; a shared daemon will add
/// a token or login policy here and change nothing else.
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

/// Why a request is refused under the local policy, if it is: a Host that is
/// not a loopback name (DNS rebinding), a fetch the browser marks as from
/// another site, or an Origin other than the daemon's own. Agents, MCP
/// clients and the CLI send no Origin and no Sec-Fetch-Site, and a loopback Host.
fn refusal(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !is_loopback(host) {
        return Some(format!("Host `{host}` is not a loopback name; the daemon answers only localhost, 127.0.0.1 and [::1]"));
    }
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if site == "cross-site" || site == "same-site" {
            return Some(format!("a request from another site ({site}) is refused"));
        }
    }
    if let Some(origin) = headers.get(ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if origin != format!("http://{host}") {
            return Some(format!("a request from origin `{origin}` is refused; only the daemon's own origin may call it from a browser"));
        }
    }
    None
}

/// `localhost`, `127.0.0.1` or `[::1]`, with or without a port.
fn is_loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().map(|n| format!("[{n}]")).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or("").to_string()
    };
    matches!(name.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

fn forbidden(why: String) -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::to_value(ToolError::invalid(why, vec![])).unwrap_or_default())).into_response()
}

/// The one step: the policy's caller put on the request, or a 403.
pub async fn admit(State(policy): State<Arc<dyn Policy>>, mut req: Request, next: Next) -> Response {
    match policy.admit(req.headers()) {
        Ok(caller) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(why) => forbidden(why),
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
    /// {"test": "the-local-policy-admits-this-machine-only", "checks": ["crates/server/src/access#local-policy", "crates/server/src/http#same-origin"]}
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
            .route("/x", axum::routing::get(move |caller: Option<axum::Extension<Caller>>| async move {
                *handler_saw.lock().unwrap() = Some(caller.map(|axum::Extension(c)| c));
                "ran"
            }))
            .layer(axum::middleware::from_fn_with_state(policy, admit));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await });
        let answer = tokio::task::spawn_blocking(move || {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET /x HTTP/1.0
Host: 127.0.0.1

").unwrap();
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
}
