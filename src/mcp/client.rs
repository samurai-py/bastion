use crate::mcp::registry::ToolRegistry;
use crate::types::BastionError;
use rmcp::model::CallToolRequestParams;
use rmcp::{
    service::{RoleClient, RunningService},
    ServiceExt,
};
use serde_json::Value;
use tokio::time::{timeout, Duration};

pub struct McpClient {
    // RunningService<RoleClient, ()> must live for daemon lifetime (Pitfall 3 in RESEARCH.md)
    // RunningService implements Deref<Target = Peer<RoleClient>>, so call list_all_tools()/call_tool() directly on it
    servers: Vec<(String, RunningService<RoleClient, ()>)>,
    registry: ToolRegistry,
}

pub fn load_mcp_config(path: &str) -> anyhow::Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let v: Value = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("failed to parse mcp-servers.json: {}", e))?;
            Ok(v)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_json::json!({"mcpServers": {}}))
        }
        Err(e) => Err(anyhow::anyhow!(
            "failed to read mcp config at {}: {}",
            path,
            e
        )),
    }
}

impl McpClient {
    /// Legacy: connect from a mcp-servers.json file path. Kept for compatibility/tests.
    /// Runtime uses [`connect_from_config`] (bastion.toml `[mcp.servers]`, D-09).
    pub async fn connect_all(config_path: &str) -> anyhow::Result<Self> {
        let config = load_mcp_config(config_path)?;
        match config.get("mcpServers").and_then(|v| v.as_object()) {
            Some(obj) => Self::connect_from_servers_obj(obj).await,
            None => {
                tracing::warn!(
                    "mcp config has no 'mcpServers' object — starting with empty registry"
                );
                Ok(McpClient {
                    servers: Vec::new(),
                    registry: ToolRegistry::new(),
                })
            }
        }
    }

    /// Connect from `bastion.toml [mcp.servers]` (D-09) — the runtime path. The legacy
    /// `.bastion/mcp-servers.json` file is no longer required (and isn't mounted in the
    /// FROM-scratch container, which is why MCP tools were silently absent before).
    pub async fn connect_from_config(
        servers: &std::collections::HashMap<String, crate::config::McpServerEntry>,
    ) -> anyhow::Result<Self> {
        let mut obj = serde_json::Map::new();
        for (key, entry) in servers {
            let label = if entry.label.is_empty() {
                key.clone()
            } else {
                entry.label.clone()
            };
            // url-based servers (streamable-http / SSE); internal network, no auth header.
            obj.insert(
                label,
                serde_json::json!({ "url": entry.url, "transport": "sse" }),
            );
        }
        Self::connect_from_servers_obj(&obj).await
    }

    /// Shared connect loop over a `{ label: {url|command,...} }` map.
    async fn connect_from_servers_obj(
        mcp_servers: &serde_json::Map<String, Value>,
    ) -> anyhow::Result<Self> {
        let mut servers: Vec<(String, RunningService<RoleClient, ()>)> = Vec::new();
        let mut registry = ToolRegistry::new();

        for (label, server_cfg) in mcp_servers {
            let transport = server_cfg
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("sse");

            let service_result = match transport {
                "stdio" => {
                    let command = match server_cfg.get("command").and_then(|v| v.as_str()) {
                        Some(c) => c.to_owned(),
                        None => {
                            tracing::warn!(server = %label, "STDIO server missing 'command' field, skipping");
                            continue;
                        }
                    };
                    let args: Vec<String> = server_cfg
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    connect_stdio(
                        &command,
                        &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    )
                    .await
                }
                // "sse" is the default transport (see unwrap_or("sse") above); any other
                // value falls through here too.
                _ => {
                    let url = match server_cfg.get("url").and_then(|v| v.as_str()) {
                        Some(u) => u.to_owned(),
                        None => {
                            tracing::warn!(server = %label, "SSE server missing 'url' field, skipping");
                            continue;
                        }
                    };
                    // Optional bearer token: literal or `${ENV_VAR}` reference. Sent as
                    // `Authorization: Bearer <token>`.
                    let auth_token =
                        resolve_secret(server_cfg.get("auth_token").and_then(|v| v.as_str()));
                    // Optional custom headers (each value: literal or `${ENV_VAR}`). Needed by
                    // servers with non-Bearer auth, e.g. Composio's `x-consumer-api-key`.
                    let custom_headers: Vec<(String, String)> = server_cfg
                        .get("headers")
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(k, v)| {
                                    resolve_secret(v.as_str()).map(|val| (k.clone(), val))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    connect_sse(&url, auth_token, custom_headers).await
                }
            };

            match service_result {
                Ok(service) => {
                    // Fetch tools at startup — satisfies CORE-02 (full schemas available immediately)
                    match service.list_all_tools().await {
                        Ok(tools) => {
                            for tool in tools {
                                // input_schema is Arc<JsonObject> (Map<String, Value>) — wrap as Value::Object
                                let schema = Value::Object((*tool.input_schema).clone());
                                let description = tool
                                    .description
                                    .as_ref()
                                    .map(|d| d.to_string())
                                    .unwrap_or_default();
                                registry.register_with_schema(
                                    label,
                                    tool.name.to_string(),
                                    schema,
                                    description,
                                );
                            }
                            tracing::info!(server = %label, "MCP server connected and tools registered");
                            servers.push((label.clone(), service));
                        }
                        Err(e) => {
                            tracing::warn!(server = %label, error = %e, "connected but failed to list tools, skipping");
                        }
                    }
                }
                Err(e) => {
                    // Non-fatal — Composio URL might not be configured yet
                    tracing::warn!(server = %label, error = %e, "failed to connect to MCP server, skipping");
                }
            }
        }

        Ok(McpClient { servers, registry })
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub async fn call_tool_with_timeout(&self, name: &str, args: Value) -> anyhow::Result<Value> {
        let server_label = match self.registry.server_for(name) {
            Some(label) => label.to_owned(),
            None => anyhow::bail!("tool '{}' not found in any connected MCP server", name),
        };

        let server = self
            .servers
            .iter()
            .find(|(label, _)| label == &server_label)
            .map(|(_, svc)| svc);

        let server = match server {
            Some(s) => s,
            None => anyhow::bail!(
                "server '{}' for tool '{}' not in active connections",
                server_label,
                name
            ),
        };

        let tool_name = name.to_owned();
        let mut params = CallToolRequestParams::new(tool_name.clone());
        params.arguments = args.as_object().cloned();

        let call_future = server.call_tool(params);
        match timeout(Duration::from_secs(30), call_future).await {
            Ok(Ok(result)) => {
                Ok(serde_json::to_value(result.content.first()).unwrap_or(Value::Null))
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("tool call failed: {}", e)),
            Err(_elapsed) => Err(BastionError::McpTimeout {
                tool: tool_name,
                elapsed_ms: 30_000,
            }
            .into()),
        }
    }
}

async fn connect_stdio(
    command: &str,
    args: &[&str],
) -> anyhow::Result<RunningService<RoleClient, ()>> {
    use rmcp::transport::TokioChildProcess;
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args);
    let transport = TokioChildProcess::new(cmd)?;
    let service: RunningService<RoleClient, ()> = ().serve(transport).await?;
    Ok(service)
}

async fn connect_sse(
    uri: &str,
    auth_token: Option<String>,
    custom_headers: Vec<(String, String)>,
) -> anyhow::Result<RunningService<RoleClient, ()>> {
    use reqwest::header::{HeaderName, HeaderValue};
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    use rmcp::transport::StreamableHttpClientTransport;

    let mut config = StreamableHttpClientTransportConfig::default();
    config.uri = uri.into();
    // Bearer convenience (`Authorization: Bearer <token>`).
    config.auth_header = auth_token;
    // Arbitrary headers — required by servers using non-Bearer auth, e.g.
    // Composio's `x-consumer-api-key`.
    for (name, value) in custom_headers {
        match (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            (Ok(n), Ok(v)) => {
                config.custom_headers.insert(n, v);
            }
            _ => tracing::warn!(header = %name, "invalid custom header name/value, skipping"),
        }
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    let service: RunningService<RoleClient, ()> = ().serve(transport).await?;
    Ok(service)
}

/// Resolve a config string that may reference an env var as `${VAR_NAME}`.
/// Literal values pass through unchanged. Returns None for missing/empty.
fn resolve_secret(raw: Option<&str>) -> Option<String> {
    let v = raw?.trim();
    if v.is_empty() {
        return None;
    }
    if let Some(var) = v.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        match std::env::var(var) {
            Ok(val) if !val.trim().is_empty() => Some(val),
            _ => {
                tracing::warn!(env = %var, "auth_token references unset/empty env var");
                None
            }
        }
    } else {
        Some(v.to_owned())
    }
}
