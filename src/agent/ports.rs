//! Kernel ports (M2 step 3a): trait seams that let [`crate::agent::loop_::AgentLoop`]
//! depend on abstract capabilities instead of concrete product/cognition types.
//!
//! This is the in-monolith half of the M2 substrate split
//! (`docs/revamp/M2-ports-design.md`): the traits below are introduced and the
//! loop is wired to depend on them, but no file moves crate yet — that is a
//! separate step (3b). Behavior is unchanged; only the seam is added.

use bastion_types::FailureKind;

use crate::memory::PrivacyTier;

/// P2 — failure telemetry sink.
///
/// Absorbs `eval::capture::record_failure` (`src/eval/capture.rs`), called
/// from the loop's egress-reject path (`agent/loop_.rs`) and from
/// `hooks::output_validator`'s NL-contestation-revoke path (HOOK-03). Resolves
/// the ADR's V4 anomaly (`runtime → cognition` via `hooks/output_validator.rs`
/// using `crate::eval`): both call sites now depend on this trait instead of
/// naming the `eval` module directly.
pub trait FailureSink: Send + Sync {
    /// Record one production-failure signal (EVAL-01).
    ///
    /// Must never panic and never propagate an error — mirrors
    /// `eval::capture::record_failure`'s swallow-on-write-failure contract.
    /// `tier` is the resolved `PrivacyTier` of the turn/belief that failed
    /// (deny-on-ambiguity routing lives in the concrete implementation).
    /// `detail` is a fixed, hardcoded `structural_property` label chosen by
    /// the calling code — never derived from user input (Pitfall 1).
    fn record_failure(&self, kind: FailureKind, tier: Option<PrivacyTier>, detail: &str);
}
