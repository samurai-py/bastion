use super::{Provider, anthropic::AnthropicProvider, openai::OpenAIProvider, ollama::OllamaProvider};

pub fn resolve_provider(model_name: &str) -> anyhow::Result<Box<dyn Provider>> {
    if model_name.starts_with("claude") {
        Ok(Box::new(AnthropicProvider::new(model_name)))
    } else if model_name.starts_with("gpt")
           || model_name.starts_with("o1")
           || model_name.starts_with("o3") {
        Ok(Box::new(OpenAIProvider::new(model_name)))
    } else {
        Ok(Box::new(OllamaProvider::new(model_name)))
    }
}

/// Test-only helper: resolve which provider kind a model name maps to
/// without constructing the provider (which reads env vars).
#[doc(hidden)]
pub fn resolve_provider_kind(model_name: &str) -> &'static str {
    if model_name.starts_with("claude") {
        "anthropic"
    } else if model_name.starts_with("gpt")
           || model_name.starts_with("o1")
           || model_name.starts_with("o3") {
        "openai"
    } else {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_provider_kind_anthropic() {
        assert_eq!(resolve_provider_kind("claude-opus-4-7"),  "anthropic");
        assert_eq!(resolve_provider_kind("claude-sonnet-4-5"), "anthropic");
    }

    #[test]
    fn resolve_provider_kind_openai() {
        assert_eq!(resolve_provider_kind("gpt-4o"),  "openai");
        assert_eq!(resolve_provider_kind("o1-mini"), "openai");
        assert_eq!(resolve_provider_kind("o3-mini"), "openai");
    }

    #[test]
    fn resolve_provider_kind_ollama() {
        assert_eq!(resolve_provider_kind("llama3"),  "ollama");
        assert_eq!(resolve_provider_kind("mistral"), "ollama");
    }
}
