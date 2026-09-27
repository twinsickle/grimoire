//! # Module contract
//!
//! ```json
//! {"purpose":"Expose the core tool dispatcher as an MCP server over streamable HTTP.","criteria":[{"id":"same-tools","statement":"The tools listed over MCP are exactly the core definitions, with the same names, descriptions and input schemas."},{"id":"errors-visible","statement":"A tool error is returned as a tool-level error result carrying the error JSON, so the caller sees the code and message."}]}
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
                "Grimoire holds a project's manual (pages, requirements, module contracts, code) and its tickets as one graph. \
                 Orient with `outline` and `board`; before changing anything call `rests_on` on the ticket or node to see what it depends on and the decisions behind it; \
                 `rests_on_me` shows the blast radius. Writes: `put_page`, `put_requirement`, `record_decision`, `file_ticket`, `update_ticket`, `start_ticket`, `close_ticket`. \
                 `lint` is the same check as `grimoire check`.",
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
