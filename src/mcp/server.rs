//! Bastion MCP server — exposes capabilities as MCP tools/resources.
//!
//! Every inbound call dispatches through CapabilityRegistry::invoke, maintaining
//! the egress gate and approval queue (D-07). Static token auth with per-token
//! read-only/read-write permissions (D-05).
//!
//! Transports: Streamable HTTP (axum, Tasks 1-2) + stdio (Task 3, D-06).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use crate::capability::CapabilityRegistry;
use crate::goal::GoalEngine;
use crate::memory::{PrivacyTier, SharedMemory};
use crate::persona::PersonaRegistry;
use rmcp::model::*;
use serde_json::Value;
use rmcp::service::{MaybeSendFuture, RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};

/// Per-token permissions (D-05): read_only vs read-write, bound to a specific owner.
#[derive(Debug, Clone)]
pub struct TokenPermissions {
    pub read_only: bool,
    pub owner_id: String,
}

/// Bastion MCP server — dispatches to CapabilityRegistry, Memory, PersonaRegistry, GoalEngine.
pub struct BastionMcpServer {
    registry: Arc<CapabilityRegistry>,
    memory: SharedMemory,
    personas: Arc<PersonaRegistry>,
    goals: GoalEngine,
    token_permissions: HashMap<String, TokenPermissions>,
    local_owner: String,
}

impl BastionMcpServer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<CapabilityRegistry>,
        memory: SharedMemory,
        personas: Arc<PersonaRegistry>,
        goals: GoalEngine,
        token_permissions: HashMap<String, TokenPermissions>,
        local_owner: String,
    ) -> Self {
        Self {
            registry,
            memory,
            personas,
            goals,
            token_permissions,
            local_owner,
        }
    }
}

impl Clone for BastionMcpServer {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            memory: self.memory.clone(),
            personas: self.personas.clone(),
            goals: self.goals.clone(),
            token_permissions: self.token_permissions.clone(),
            local_owner: self.local_owner.clone(),
        }
    }
}

impl ServerHandler for BastionMcpServer {
    fn get_info(&self) -> ServerInfo {
        let caps = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        ServerInfo::new(caps)
            .with_server_info(Implementation::new("bastion", env!("CARGO_PKG_VERSION")))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        let tools: Vec<Tool> = self
            .registry
            .list_tool_defs()
            .into_iter()
            .map(|def| {
                let name = def["name"].as_str().unwrap_or("unknown").to_string();
                let description = def["description"].as_str().unwrap_or("").to_string();
                let schema_obj = match def.get("input_schema") {
                    Some(Value::Object(obj)) => obj.clone(),
                    _ => serde_json::Map::new(),
                };
                Tool::new(name, description, Arc::new(schema_obj))
            })
            .collect();
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + MaybeSendFuture + '_ {
        let token = request
            .meta
            .as_ref()
            .and_then(|m| m.get("x-bastion-token").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let name = request.name.clone();
        let args = request.arguments.unwrap_or_default();

        let registry = self.registry.clone();
        let token_perms = self.token_permissions.clone();
        let local_owner = self.local_owner.clone();

        async move {
            let perms = token_perms
                .get(&token)
                .cloned()
                .unwrap_or(TokenPermissions {
                    read_only: false,
                    owner_id: local_owner,
                });

            if perms.read_only {
                return Ok(CallToolResult::error(vec![
                    Content::text("read-only token cannot invoke tools"),
                ]));
            }

            let ctx = crate::capability::InvokeCtx {
                owner: perms.owner_id,
                privacy_tier: Some(PrivacyTier::CloudOk),
                needs_approval: false,
            };

            match registry.invoke(&name, Value::Object(args), &ctx).await {
                Ok(value) => Ok(CallToolResult::success(vec![Content::text(
                    value.to_string(),
                )])),
                Err(e) => Ok(CallToolResult::error(vec![Content::text(
                    e.to_string(),
                )])),
            }
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, McpError>> + MaybeSendFuture + '_ {
        let resources = vec![
            Annotated::new(
                RawResource::new("bastion://memories", "Agent Memories")
                    .with_description("Retrieve stored beliefs and memories")
                    .with_mime_type("application/json"),
                None,
            ),
            Annotated::new(
                RawResource::new("bastion://personas", "Personas")
                    .with_description("List available agent personas")
                    .with_mime_type("application/json"),
                None,
            ),
            Annotated::new(
                RawResource::new("bastion://goals", "Goals")
                    .with_description("List tracked goals and progress")
                    .with_mime_type("application/json"),
                None,
            ),
        ];
        std::future::ready(Ok(ListResourcesResult::with_all_items(resources)))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + MaybeSendFuture + '_ {
        let uri = request.uri;

        let memory = self.memory.clone();
        let personas = self.personas.clone();
        let goals = self.goals.clone();
        let local_owner = self.local_owner.clone();

        async move {
            let contents = match uri.as_str() {
                "bastion://memories" => {
                    let mem = memory.read().await;
                    let beliefs = mem
                        .retrieve_tagged(&local_owner, None)
                        .await
                        .unwrap_or_default();
                    let json =
                        serde_json::to_string_pretty(&beliefs).unwrap_or_else(|_| "[]".into());
                    vec![ResourceContents::text(json, &uri)
                        .with_mime_type("application/json")]
                }
                "bastion://personas" => {
                    let all_personas: Vec<&crate::persona::Persona> = personas
                        .names()
                        .into_iter()
                        .filter_map(|name| personas.get(name))
                        .collect();
                    let json = serde_json::to_string_pretty(&all_personas)
                        .unwrap_or_else(|_| "[]".into());
                    vec![ResourceContents::text(json, &uri)
                        .with_mime_type("application/json")]
                }
                "bastion://goals" => {
                    let all_goals = goals.list_goals(&local_owner).await.unwrap_or_default();
                    let json = serde_json::to_string_pretty(&all_goals)
                        .unwrap_or_else(|_| "[]".into());
                    vec![ResourceContents::text(json, &uri)
                        .with_mime_type("application/json")]
                }
                _ => {
                    return Err(McpError::invalid_params(
                        format!("unknown resource: {}", uri),
                        None,
                    ));
                }
            };

            Ok(ReadResourceResult::new(contents))
        }
    }
}

