use std::time::Instant;
use tokio::sync::mpsc;
use crate::types::{Message, Role, MessageContent, ContentPart, CallConfig, BastionError, TokenUsage};
use crate::provider::{SharedProvider, call_with_retry};
use crate::session::SessionManager;
use crate::mcp::McpClient;
use crate::agent::compactor::AutoCompact;
use crate::agent::command::{handle_command, CommandResult};

const MAX_TOOL_ROUNDS: u32 = 10;
const DEFAULT_SYSTEM_PROMPT: &str = "You are Bastion, a proactive personal AI assistant.";

pub struct AgentLoop {
    pub provider:         SharedProvider,
    pub session:          SessionManager,
    pub mcp:              McpClient,
    pub compactor:        AutoCompact,
    pub session_id:       String,
    pub daily_budget_usd: f64,
    /// Pending queue for future mid-turn message injection (CORE-08 infrastructure).
    /// Phase 1: channel exists for Phase 2+ activation — receives but never sends.
    pub pending_tx:       mpsc::Sender<String>,
    pub pending_rx:       Option<mpsc::Receiver<String>>,
}

impl AgentLoop {
    pub fn new(
        provider:         SharedProvider,
        session:          SessionManager,
        mcp:              McpClient,
        session_id:       String,
        daily_budget_usd: f64,
    ) -> Self {
        let (pending_tx, pending_rx) = mpsc::channel(32);
        Self {
            provider,
            session,
            mcp,
            compactor: AutoCompact::new(),
            session_id,
            daily_budget_usd,
            pending_tx,
            pending_rx: Some(pending_rx),
        }
    }

    /// Execute one full agent turn (Nanobot pattern):
    /// user_msg → session.append → load_history → compaction_check → LLM → tool_loop → save → return text
    pub async fn run_turn(&mut self, user_input: &str) -> anyhow::Result<String> {
        let t_start = Instant::now();

        // 1. Persist user message
        self.session.append(
            &self.session_id,
            Message { role: Role::User, content: MessageContent::Text(user_input.to_owned()) },
            None,
        ).await?;

        // 2. Load history and build token estimate
        let mut history = self.session.load_recent(&self.session_id).await?;

        // 3. Token ratio check and compaction BEFORE LLM call (D-08, AI-SPEC §4b.4)
        let used_tokens: u32 = AutoCompact::estimate_tokens(&history);
        let context_limit = self.provider.read().await.context_limit();
        if self.compactor.needs_compaction(used_tokens, context_limit) {
            let provider_ref = self.provider.read().await;
            history = self.compactor.compact(
                &self.session_id,
                &history,
                &**provider_ref,
                &self.session,
            ).await?;
            drop(provider_ref);
        }

        // 4. Build tool definitions from ToolRegistry.
        // Schemas were fetched at connect_all time via list_tools() per MCP server (CORE-02).
        let tools: Vec<serde_json::Value> = self.mcp.registry().list_tool_names()
            .iter()
            .map(|name| {
                let schema = self.mcp.registry().get_tool_schema(name)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                serde_json::json!({
                    "name": name,
                    "description": format!("External tool: {}", name),
                    "input_schema": schema
                })
            })
            .collect();

        let config = CallConfig {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_owned(),
            max_tokens: 4096,
            tools,
        };

        // 5. Agentic tool loop with hard round cap (Pitfall 4)
        let mut rounds = 0u32;
        let final_text = loop {
            if rounds >= MAX_TOOL_ROUNDS {
                tracing::error!(
                    event = "tool_loop_cap",
                    rounds = rounds,
                    session_id = %self.session_id
                );
                anyhow::bail!(BastionError::ToolLoopCap);
            }

            // 6. Budget check BEFORE cloud call (PROV-06)
            let provider_name = self.provider.read().await.name().to_owned();
            if provider_name != "ollama" {
                if !self.session.check_budget(self.daily_budget_usd).await? {
                    anyhow::bail!(BastionError::BudgetExceeded);
                }
            }

            // 7. LLM call — hold READ lock for full stream duration (Pitfall 5)
            let response = {
                let provider = self.provider.read().await;
                let prov_ref: &dyn crate::provider::Provider = &**provider;
                // SAFETY: call_with_retry closure borrows prov_ref for the duration of this block.
                // The READ lock is held for the entire duration of complete(), released after this block.
                call_with_retry(|| prov_ref.complete(&history, &config), 3).await?
            };  // READ lock released here

            // 8. Update budget with actual cost
            let cost_usd = estimate_cost_usd(provider_name.as_str(), &response.usage);
            if let Err(e) = self.session.update_budget(cost_usd).await {
                tracing::warn!(error = %e, "failed to update budget");
            }

            // 9. Write assistant message to SQLite BEFORE dispatching tools (Pitfall 1)
            self.session.append(
                &self.session_id,
                Message {
                    role: Role::Assistant,
                    content: if let Some(ref tc) = response.tool_calls {
                        MessageContent::Parts(
                            std::iter::once(ContentPart::Text { text: response.text.clone() })
                                .chain(tc.iter().map(|t| ContentPart::ToolUse {
                                    id: t.id.clone(),
                                    name: t.name.clone(),
                                    input: t.arguments.clone(),
                                }))
                                .collect()
                        )
                    } else {
                        MessageContent::Text(response.text.clone())
                    },
                },
                Some(response.usage.output_tokens),
            ).await?;

            history.push(Message {
                role: Role::Assistant,
                content: MessageContent::Text(response.text.clone()),
            });

            // 10. Tool dispatch
            match response.tool_calls {
                None => break response.text,  // final answer — no more tool calls
                Some(tool_calls) => {
                    for tc in &tool_calls {
                        tracing::debug!(event = "tool_dispatch", tool = %tc.name);
                        let result = self.mcp.call_tool_with_timeout(&tc.name, tc.arguments.clone()).await
                            .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }));

                        // Tool result written to SQLite (assistant was written above — role sequence OK)
                        let result_str = result.to_string();
                        self.session.append(
                            &self.session_id,
                            Message {
                                role: Role::Tool,
                                content: MessageContent::Text(result_str.clone()),
                            },
                            None,
                        ).await?;
                        history.push(Message {
                            role: Role::Tool,
                            content: MessageContent::Text(result_str),
                        });
                    }
                    rounds += 1;
                }
            }
        };

        let latency_ms = t_start.elapsed().as_millis() as u64;
        tracing::info!(
            event = "turn_complete",
            latency_ms,
            session_id = %self.session_id,
            rounds,
        );

        Ok(final_text)
    }

    pub async fn handle_command(&mut self, input: &str) -> anyhow::Result<CommandResult> {
        handle_command(input, &self.provider).await
    }
}

/// Simple cost estimation for budget tracking.
/// Per AI-SPEC §4b.5: claude-sonnet-4-5 ≈ $3/1M input, $15/1M output
fn estimate_cost_usd(provider: &str, usage: &TokenUsage) -> f64 {
    match provider {
        "anthropic" => {
            let input_cost  = usage.input_tokens  as f64 * 3.0  / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 15.0 / 1_000_000.0;
            input_cost + output_cost
        }
        "openai" => {
            let input_cost  = usage.input_tokens  as f64 * 2.5  / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 10.0 / 1_000_000.0;
            input_cost + output_cost
        }
        "ollama" => 0.0,  // local — no cost
        _ => 0.0,
    }
}
