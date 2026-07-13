//! Shim (M2 step 5): the concrete providers (Anthropic/OpenAI/Ollama/Gemini/
//! Groq/OpenRouter/terminal-agent), the OpenAI-compat translation helpers,
//! and the registry moved to `bastion_providers`. Re-exported here so every
//! existing `crate::provider::...` path keeps compiling unchanged.
//!
//! The kernel surface (`Provider`, `SharedProvider`, `call_with_retry`,
//! `complete_structured_via_forced_tool_call`) still lives in
//! `bastion_runtime::provider` (M2 step 3b) and is re-exported transitively
//! through `bastion_providers`.

pub use bastion_providers::{
    anthropic, gemini, groq, ollama, openai, openrouter, registry, terminal_agent,
};
pub use bastion_providers::{
    call_with_retry, clarify_openai_error, complete_structured_via_forced_tool_call, Provider,
    SharedProvider,
};
