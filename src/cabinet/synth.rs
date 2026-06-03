//! Cabinet synthesis — unified voice + explicit dissent (CAB-05, D-07).
//!
//! `synthesize()` takes a deliberation transcript and returns a `CabinetVerdict`:
//! - `recommendation`: a unified-voice summary of the Cabinet's collective position.
//! - `dissents`: all divergent positions (REQUIRED field — cannot be silently dropped, CF-3).
//!
//! On parse failure after 3 retries: returns a raw-positions fallback that surfaces
//! each participant's stance. NEVER fabricates a verdict (AI-SPEC §6).
//!
//! D-08: the full debate transcript is opt-in (exposed via `/cabinet`); this function
//! only returns the synthesized verdict, not the raw transcript.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use crate::cabinet::Turn;
use crate::provider::Provider;

/// A single persona's dissenting stance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Dissent {
    /// Name of the dissenting persona.
    pub persona: String,
    /// The position that differs from the recommendation.
    pub position: String,
}

/// The unified output of Cabinet synthesis.
///
/// `dissents` is a REQUIRED field (not Option) — the LLM is instructed to populate it
/// whenever any persona's position diverged from the recommendation. Callers must never
/// treat an empty `dissents` as proof of consensus; they should inspect the transcript.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CabinetVerdict {
    /// The Cabinet's unified recommendation (single voice).
    pub recommendation: String,
    /// Explicit dissenting positions. Empty only when ALL personas were aligned.
    pub dissents: Vec<Dissent>,
}

/// Synthesize the Cabinet transcript into a `CabinetVerdict`.
///
/// - Sends a structured completion request with a `CabinetVerdict` JSON schema.
/// - Retries serde-parse up to 3 attempts (AI-SPEC §4b).
/// - On exhaustion: returns a raw-positions `CabinetVerdict` assembled from the
///   transcript (never a fabricated consensus verdict — AI-SPEC §6).
pub async fn synthesize(
    provider: &dyn Provider,
    transcript: &[Turn],
) -> anyhow::Result<CabinetVerdict> {
    let schema = schemars::schema_for!(CabinetVerdict);
    let response_schema = serde_json::to_value(&schema)
        .map_err(|e| anyhow::anyhow!("failed to serialize CabinetVerdict schema: {e}"))?;

    let system = build_synthesis_prompt();
    let user = build_transcript_text(transcript);

    const MAX_ATTEMPTS: u32 = 3;
    for attempt in 1..=MAX_ATTEMPTS {
        let raw = provider
            .complete_structured(&system, &user, response_schema.clone(), 4096, 0.3)
            .await
            .map_err(|e| {
                anyhow::anyhow!("cabinet synthesis provider call failed (attempt {attempt}): {e}")
            })?;

        match serde_json::from_str::<CabinetVerdict>(&raw) {
            Ok(verdict) => return Ok(verdict),
            Err(parse_err) => {
                tracing::warn!(
                    attempt,
                    error = %parse_err,
                    "cabinet synthesis output was not valid CabinetVerdict JSON — retrying"
                );
            }
        }
    }

    // All 3 attempts failed. Surface raw positions — NEVER fabricate consensus (AI-SPEC §6).
    tracing::warn!(
        event = "cabinet_synthesis_fallback",
        "synthesis parse failed after 3 attempts; surfacing raw positions"
    );
    Ok(raw_positions_fallback(transcript))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_synthesis_prompt() -> String {
    r#"You are the Cabinet synthesis engine. Your task is to produce a single unified response from a multi-persona deliberation transcript.

Rules:
1. Synthesize a clear, unified RECOMMENDATION that represents the collective position.
2. For ANY persona whose position materially diverged from the recommendation, you MUST include a Dissent entry with their name and their position. This field is REQUIRED — do not omit dissents even if consensus appears strong.
3. If all personas were fully aligned, dissents may be empty — but only if there is ZERO divergence.
4. NEVER fabricate agreement. If in doubt, record the dissent.

Respond ONLY with a JSON object with exactly these fields:
  {"recommendation": "<the unified recommendation text>", "dissents": [{"persona": "<name>", "position": "<their diverging position>"}]}
"dissents" must be an array (empty [] only if there was zero divergence). No prose, no markdown fences."#.to_string()
}

fn build_transcript_text(transcript: &[Turn]) -> String {
    if transcript.is_empty() {
        return "No transcript turns available.".to_string();
    }
    transcript
        .iter()
        .map(|t| format!("[{}] ({}): {}", t.persona, format!("{:?}", t.kind).to_lowercase(), t.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a raw-positions fallback verdict.
///
/// `recommendation` explains that synthesis failed and lists raw positions.
/// `dissents` contains each participant's stance so nothing is dropped (CF-3).
fn raw_positions_fallback(transcript: &[Turn]) -> CabinetVerdict {
    let raw_text = build_transcript_text(transcript);
    let recommendation = format!(
        "Could not synthesize — raw positions follow:\n{raw_text}"
    );

    // Each unique persona gets a dissent entry with their last-seen turn as position.
    // This ensures the caller sees ALL stances, not a fabricated consensus.
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for turn in transcript {
        seen.insert(turn.persona.clone(), turn.text.clone());
    }
    let dissents: Vec<Dissent> = seen
        .into_iter()
        .map(|(persona, position)| Dissent { persona, position })
        .collect();

    CabinetVerdict { recommendation, dissents }
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider only, no live LLM)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cabinet::{Turn, TurnKind};
    use crate::provider::Provider;
    use crate::types::{CallConfig, LlmResponse, Message};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct ScriptedProvider {
        responses: Mutex<Vec<String>>,
    }

    impl ScriptedProvider {
        fn sequence(responses: &[&str]) -> Self {
            Self {
                responses: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
            }
        }

        fn always(response: &str) -> Self {
            Self::sequence(&[response])
        }
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            unimplemented!()
        }
        async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
            unimplemented!()
        }
        fn context_limit(&self) -> usize { 8192 }
        fn model_name(&self) -> &str { "mock" }
        fn name(&self) -> &'static str { "mock" }

        async fn complete_structured(
            &self,
            _system: &str,
            _user: &str,
            _schema: serde_json::Value,
            _max_tokens: u32,
            _temperature: f32,
        ) -> anyhow::Result<String> {
            let mut responses = self.responses.lock().unwrap();
            if responses.len() > 1 {
                Ok(responses.remove(0))
            } else {
                Ok(responses[0].clone())
            }
        }
    }

    fn make_divergent_transcript() -> Vec<Turn> {
        vec![
            Turn {
                persona: "Aria".to_string(),
                kind: TurnKind::Position,
                text: "I recommend approach A because it is safest.".to_string(),
            },
            Turn {
                persona: "Finance".to_string(),
                kind: TurnKind::Position,
                text: "I recommend approach B because it is cheapest.".to_string(),
            },
        ]
    }

    #[tokio::test]
    async fn valid_verdict_with_dissents_is_parsed() {
        let verdict_json = serde_json::json!({
            "recommendation": "Adopt approach A with cost controls.",
            "dissents": [
                { "persona": "Finance", "position": "Approach B is cheaper." }
            ]
        })
        .to_string();

        let provider = ScriptedProvider::always(&verdict_json);
        let transcript = make_divergent_transcript();

        let verdict = synthesize(&provider, &transcript).await.unwrap();

        assert!(!verdict.dissents.is_empty(), "dissents must be non-empty for divergent transcript");
        assert_eq!(verdict.dissents[0].persona, "Finance");
    }

    #[tokio::test]
    async fn garbage_3x_returns_raw_positions_fallback_not_panic() {
        // AI-SPEC §6: parse failure → raw positions, no fabricated verdict, no panic.
        let provider = ScriptedProvider::sequence(&["garbage", "also bad", "{{ invalid"]);
        let transcript = make_divergent_transcript();

        let verdict = synthesize(&provider, &transcript).await.unwrap();

        // Fallback: recommendation contains "raw positions"
        assert!(
            verdict.recommendation.contains("raw positions"),
            "expected raw-positions fallback, got: {}",
            verdict.recommendation
        );
        // Dissents must list all participants (CF-3 — nothing dropped)
        let persona_names: Vec<&str> = verdict.dissents.iter().map(|d| d.persona.as_str()).collect();
        assert!(persona_names.contains(&"Aria"), "Aria must be in dissents");
        assert!(persona_names.contains(&"Finance"), "Finance must be in dissents");
    }

    #[tokio::test]
    async fn two_garbage_then_valid_succeeds() {
        let verdict_json = serde_json::json!({
            "recommendation": "Go with approach A.",
            "dissents": []
        })
        .to_string();

        let provider = ScriptedProvider::sequence(&["garbage", "also bad", &verdict_json]);
        let transcript = make_divergent_transcript();

        let verdict = synthesize(&provider, &transcript).await.unwrap();
        assert_eq!(verdict.recommendation, "Go with approach A.");
    }

    #[tokio::test]
    async fn empty_transcript_returns_graceful_fallback() {
        let provider = ScriptedProvider::sequence(&["garbage", "garbage", "garbage"]);
        let verdict = synthesize(&provider, &[]).await.unwrap();
        // No panic, recommendation mentions raw positions
        assert!(verdict.recommendation.contains("raw positions") || verdict.recommendation.contains("No transcript"));
    }
}
