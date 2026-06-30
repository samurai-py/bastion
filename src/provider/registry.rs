use super::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, ollama::OllamaProvider,
    openai::OpenAIProvider, openrouter::OpenRouterProvider, terminal_agent::TerminalAgentProvider,
    Provider,
};

pub fn resolve_provider(model_name: &str) -> anyhow::Result<Box<dyn Provider>> {
    // Exact match BEFORE the `claude` prefix check — "claude_code" must not hit Anthropic.
    if model_name == "claude_code" {
        Ok(Box::new(TerminalAgentProvider::new(
            "claude",
            "claude_code",
        )))
    } else if model_name == "opencode" {
        Ok(Box::new(TerminalAgentProvider::new("opencode", "opencode")))
    } else if model_name.starts_with("claude") {
        Ok(Box::new(AnthropicProvider::new(model_name)))
    } else if model_name.starts_with("gpt")
        || model_name.starts_with("o1")
        || model_name.starts_with("o3")
    {
        Ok(Box::new(OpenAIProvider::new(model_name)))
    } else if model_name.starts_with("gemini") {
        Ok(Box::new(GeminiProvider::new(model_name)))
    } else if model_name.contains('/') {
        // OpenRouter slugs are namespaced: `vendor/model[:tag]` (e.g. `:free`).
        Ok(Box::new(OpenRouterProvider::new(model_name)))
    } else {
        Ok(Box::new(OllamaProvider::new(model_name)))
    }
}

/// Test-only helper: resolve which provider kind a model name maps to
/// without constructing the provider (which reads env vars).
#[doc(hidden)]
pub fn resolve_provider_kind(model_name: &str) -> &'static str {
    if model_name == "claude_code" || model_name == "opencode" {
        "terminal_agent"
    } else if model_name.starts_with("claude") {
        "anthropic"
    } else if model_name.starts_with("gpt")
        || model_name.starts_with("o1")
        || model_name.starts_with("o3")
    {
        "openai"
    } else if model_name.starts_with("gemini") {
        "gemini"
    } else if model_name.contains('/') {
        "openrouter"
    } else {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_provider_kind_anthropic() {
        assert_eq!(resolve_provider_kind("claude-opus-4-7"), "anthropic");
        assert_eq!(resolve_provider_kind("claude-sonnet-4-5"), "anthropic");
    }

    #[test]
    fn resolve_provider_kind_openai() {
        assert_eq!(resolve_provider_kind("gpt-4o"), "openai");
        assert_eq!(resolve_provider_kind("o1-mini"), "openai");
        assert_eq!(resolve_provider_kind("o3-mini"), "openai");
    }

    #[test]
    fn resolve_provider_kind_ollama() {
        assert_eq!(resolve_provider_kind("llama3"), "ollama");
        assert_eq!(resolve_provider_kind("mistral"), "ollama");
    }

    #[test]
    fn resolve_provider_kind_gemini() {
        assert_eq!(resolve_provider_kind("gemini-2.0-flash"), "gemini");
        assert_eq!(resolve_provider_kind("gemini-3-pro-preview"), "gemini");
    }

    #[test]
    fn resolve_provider_kind_terminal_agent() {
        assert_eq!(resolve_provider_kind("claude_code"), "terminal_agent"); // not "anthropic"
        assert_eq!(resolve_provider_kind("opencode"), "terminal_agent");
    }

    #[test]
    fn resolve_provider_kind_openrouter() {
        assert_eq!(
            resolve_provider_kind("meta-llama/llama-3.3-70b-instruct:free"),
            "openrouter"
        );
        assert_eq!(
            resolve_provider_kind("deepseek/deepseek-chat-v3-0324:free"),
            "openrouter"
        );
        assert_eq!(
            resolve_provider_kind("google/gemma-2-9b-it:free"),
            "openrouter"
        );
    }
}
