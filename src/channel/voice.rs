// Local voice channel — cpal capture/playback + push-to-talk (VOICE-01).
//
// Architecture (D-12): voice is BOTH a dedicated `Channel` AND a consumer of two
// capabilities (`voice_transcribe`/`voice_speak`) that any other channel could
// also call. This file never embeds STT/TTS logic directly — it only invokes
// those capabilities through `CapabilityRegistry::invoke`, exactly like every
// other capability caller in this codebase (T-10-07-01).
//
// Privacy: every capability invocation in `handle_voice_turn` is tagged
// `PrivacyTier::LocalOnly` — this is VOICE-01's entire privacy promise ("áudio
// nunca sai para a nuvem"). It only compiles/passes correctly once the
// `voice_transcribe`/`voice_speak` adapters override `is_local() -> true`
// (Plan 10-08) — otherwise `CapabilityRegistry::invoke`'s `check_egress` call
// blocks every turn fail-closed (see `src/hooks/egress.rs`).
//
// The `Channel` trait implementation (push-to-talk trigger, wake-word) is added
// in Plan 10-07's Task 2/4 — this file is filled in incrementally, one task per
// commit.
use crate::agent::handle::AgentHandle;
use crate::capability::registry::{CapabilityRegistry, InvokeCtx};
use crate::memory::PrivacyTier;

/// Pure capability-invocation chain for a single voice turn (VOICE-01).
///
/// record, then `voice_transcribe`, then `agent.ask`, then `voice_speak`, then the
/// caller decodes and plays back the result. Hardware-independent and fully
/// unit-testable — this is the load-bearing piece D-12 requires: the `Channel`
/// itself never embeds STT/TTS logic, it only calls through
/// `CapabilityRegistry::invoke` like any other capability caller.
///
/// SECURITY (T-10-07-01): both invocations below are tagged
/// `PrivacyTier::LocalOnly` and `needs_approval: false` — the entire privacy
/// promise of this channel. Returns `Err` (never panics) on a malformed
/// `voice_transcribe`/`voice_speak` response.
pub async fn handle_voice_turn(
    audio_b64_in: String,
    registry: &CapabilityRegistry,
    agent: &AgentHandle,
    owner: &str,
    voice_id: &str,
) -> anyhow::Result<String> {
    let ctx = InvokeCtx {
        owner: owner.to_string(),
        privacy_tier: Some(PrivacyTier::LocalOnly),
        needs_approval: false,
    };

    let transcribe_result = registry
        .invoke(
            "voice_transcribe",
            serde_json::json!({ "audio_b64": audio_b64_in }),
            &ctx,
        )
        .await?;
    let text = transcribe_result["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("voice_transcribe returned no text field"))?;

    let reply = agent.ask(text.to_string(), owner.to_string()).await?;

    let speak_result = registry
        .invoke(
            "voice_speak",
            serde_json::json!({ "text": reply, "voice": voice_id }),
            &ctx,
        )
        .await?;
    let audio_b64_out = speak_result["audio_b64"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("voice_speak returned no audio_b64 field"))?;

    Ok(audio_b64_out.to_string())
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::handle;
    use crate::capability::registry::Capability;
    use async_trait::async_trait;
    use base64::Engine;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    /// One recorded `(privacy_tier, needs_approval)` pair per stub invocation.
    type RecordedCalls = Arc<Mutex<Vec<(Option<PrivacyTier>, bool)>>>;

    /// Stub `voice_transcribe`/`voice_speak` capability — records every `InvokeCtx`
    /// it was called with (Test 2's load-bearing LocalOnly assertion) and returns a
    /// scripted response. Mirrors `registry.rs`'s `StubCap` test pattern.
    ///
    /// `is_local() -> true` mirrors the production adapter override this whole
    /// feature depends on (Plan 10-08) — without it, `CapabilityRegistry::invoke`
    /// would block every LocalOnly-tagged call before it ever reached this stub.
    struct StubVoiceCap {
        name: String,
        schema: Value,
        response: Value,
        recorded: RecordedCalls,
    }

    #[async_trait]
    impl Capability for StubVoiceCap {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn input_schema(&self) -> &Value {
            &self.schema
        }
        async fn invoke(&self, _args: Value, ctx: &InvokeCtx) -> anyhow::Result<Value> {
            self.recorded
                .lock()
                .unwrap()
                .push((ctx.privacy_tier, ctx.needs_approval));
            Ok(self.response.clone())
        }
        fn is_local(&self) -> bool {
            true
        }
    }

    /// Stub consumer: replies "echo:{text}" (mirrors `telegram.rs`'s test pattern).
    fn stub_consumer(mut rx: mpsc::Receiver<handle::AgentRequest>) {
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req.reply.send(Ok(format!("echo:{}", req.text)));
            }
        });
    }

    fn registry_with(
        transcribe_response: Value,
        speak_response: Value,
        recorded: RecordedCalls,
    ) -> CapabilityRegistry {
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(StubVoiceCap {
                name: "voice_transcribe".to_string(),
                schema: serde_json::json!({}),
                response: transcribe_response,
                recorded: recorded.clone(),
            }))
            .unwrap();
        registry
            .register(Arc::new(StubVoiceCap {
                name: "voice_speak".to_string(),
                schema: serde_json::json!({}),
                response: speak_response,
                recorded,
            }))
            .unwrap();
        registry
    }

    #[tokio::test]
    async fn handle_voice_turn_returns_decoded_speak_audio() {
        let known_bytes = b"fake-wav-bytes".to_vec();
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&known_bytes);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let registry = registry_with(
            serde_json::json!({ "text": "oi bastion" }),
            serde_json::json!({ "audio_b64": audio_b64, "sample_rate": 24000 }),
            recorded,
        );

        let (h, rx) = handle::channel();
        stub_consumer(rx);

        let out = handle_voice_turn(
            "fake-input-b64".to_string(),
            &registry,
            &h,
            "mario",
            "pf_dora",
        )
        .await
        .unwrap();

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(out)
            .unwrap();
        assert_eq!(decoded, known_bytes);
    }

    #[tokio::test]
    async fn handle_voice_turn_tags_both_invocations_local_only_no_approval() {
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(b"x");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let registry = registry_with(
            serde_json::json!({ "text": "oi bastion" }),
            serde_json::json!({ "audio_b64": audio_b64, "sample_rate": 24000 }),
            recorded.clone(),
        );

        let (h, rx) = handle::channel();
        stub_consumer(rx);

        handle_voice_turn(
            "fake-input-b64".to_string(),
            &registry,
            &h,
            "mario",
            "pf_dora",
        )
        .await
        .unwrap();

        let calls = recorded.lock().unwrap();
        assert_eq!(
            calls.len(),
            2,
            "both voice_transcribe and voice_speak must invoke"
        );
        for (tier, needs_approval) in calls.iter() {
            assert_eq!(*tier, Some(PrivacyTier::LocalOnly));
            assert!(!needs_approval);
        }
    }

    #[tokio::test]
    async fn handle_voice_turn_errors_on_missing_text_field() {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let registry = registry_with(
            serde_json::json!({ "not_text": "oops" }),
            serde_json::json!({ "audio_b64": "", "sample_rate": 24000 }),
            recorded,
        );

        let (h, rx) = handle::channel();
        stub_consumer(rx);

        let result = handle_voice_turn(
            "fake-input-b64".to_string(),
            &registry,
            &h,
            "mario",
            "pf_dora",
        )
        .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("voice_transcribe returned no text field"));
    }
}
