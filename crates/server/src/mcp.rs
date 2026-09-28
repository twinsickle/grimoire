//! # Module contract
//!
//! ```json
//! {"purpose":"Expose the core tool dispatcher as an MCP server over streamable HTTP.","criteria":[{"id":"same-tools","statement":"The tools listed over MCP are exactly the core definitions, with the same names, descriptions and input schemas."},{"id":"errors-visible","statement":"A tool error is returned as a tool-level error result carrying the error JSON, so the caller sees the code and message."},{"id":"stateless","statement":"The MCP transport keeps no session, so a daemon restart costs a client nothing but its next request."},{"id":"instructions","statement":"The server's instructions, sent to every client on connect, carry the non-negotiables: orient first, work is a ticket, the grimoire moves with the code, every module carries a contract, resolve references before writing them, never write the store's files, propose resolutions but do not decide them."}]}
//! ```

use std::sync::Arc;

use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};

use crate::daemon::Daemon;

#[derive(Clone)]
pub struct McpServer {
    daemon: Arc<Daemon>,
}

impl McpServer {
    pub fn new(daemon: Arc<Daemon>) -> Self {
        McpServer { daemon }
    }
}

fn tool_defs() -> Vec<Tool> {
    grimoire_core::tools::definitions()
        .into_iter()
        .map(|d| {
            let schema = match d.input_schema {
                serde_json::Value::Object(m) => m,
                _ => JsonObject::new(),
            };
            let mut t = Tool::new(d.name, d.description, schema);
            let read_only = !grimoire_core::tools::is_write(d.name);
            t.annotations = Some(ToolAnnotations::new().read_only(read_only).destructive(d.name == "delete_node" || d.name == "close_ticket").open_world(false));
            t
        })
        .collect()
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("grimoire", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "This project's grimoire (pages, requirements with decisions, the module contracts in the code) is the truth of what it is meant to do; code and grimoire change in the same piece of work. \
                 Orient first: `outline` at depth 2, then the goal, architecture, constraints and conventions pages; `get` a module before opening its files. \
                 Work is a ticket: `start_ticket` returns what it rests on and the decisions to challenge; write the grimoire before the code; `lint`, fix what you created, `close_ticket`. \
                 Every module in the contract scope carries a `# Module contract`; a new criterion gets a requirement. `find` before writing any reference. Never write the store's files. \
                 A gap or question closes on the person's answer to options you lay out, never on your own decision. \
                 `lint` is `grimoire check`; `status` shows the build's version and languages beside the store's.",
            )
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, ErrorData> {
        // Protocol 2026-07-28 requires the cache fields on list results. The
        // tool set is fixed for the life of the process, so cache it freely.
        Ok(ListToolsResult::with_all_items(tool_defs()).with_ttl_ms(24 * 60 * 60 * 1000).with_cache_scope(CacheScope::Public))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_defs().into_iter().find(|t| t.name == name)
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData> {
        let input = request.arguments.map(serde_json::Value::Object).unwrap_or_else(|| serde_json::json!({}));
        let daemon = self.daemon.clone();
        let name = request.name.to_string();
        let result = tokio::task::spawn_blocking(move || daemon.call(&name, input)).await.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(match result {
            Ok(output) => {
                let text = serde_json::to_string_pretty(&output).unwrap_or_default();
                let mut r = CallToolResult::success(vec![ContentBlock::text(text)]);
                r.structured_content = Some(output);
                r
            }
            Err(e) => {
                let v = serde_json::to_value(&e).unwrap_or_default();
                let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                let mut r = CallToolResult::error(vec![ContentBlock::text(text)]);
                r.structured_content = Some(v);
                r
            }
        }
        .into())
    }
}

/// The tower service to mount at `/mcp`. Stateless, so a daemon restart
/// costs a client nothing but a reconnect.
pub fn service(daemon: Arc<Daemon>) -> StreamableHttpService<McpServer, LocalSessionManager> {
    let mut config = StreamableHttpServerConfig::default();
    config.legacy_session_mode = false;
    config.json_response = true;
    StreamableHttpService::new(move || Ok(McpServer::new(daemon.clone())), Arc::new(LocalSessionManager::default()), config)
}
