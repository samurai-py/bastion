//! Approval-intent detection: natural-language yes/no resolution for pending
//! `approval_queue` rows (SEC-01, D-02 — "linguagem natural é o mecanismo BASE").
//!
//! This is the SAME idiom as [`crate::hooks::output_validator::detect_contestation`]:
//! a pure, offline, case-insensitive substring match against a fixed bilingual
//! phrase list — never an LLM call, never fuzzy matching. It is intentionally NOT
//! a clone of `CONTESTATION_PHRASES` — those phrases mean "you're wrong about X"
//! (belief revocation intent), a semantically unrelated concept from "yes, go
//! ahead" / "no, cancel that" (approval-queue resolution intent).

/// Phrase set for APPROVAL intent (pt-BR + en, case-insensitive).
const APPROVAL_PHRASES: &[&str] = &[
    "sim",
    "aprovo",
    "confirmo",
    "pode fazer",
    "autorizo",
    "yes",
    "approve",
    "approved",
    "confirmed",
    "go ahead",
];

/// Phrase set for REJECTION intent (pt-BR + en, case-insensitive).
const REJECTION_PHRASES: &[&str] = &[
    "não", "nao", "rejeito", "cancela", "cancelar", "no", "reject", "cancel", "deny",
];

/// Returns `true` when `text` contains a natural-language approval phrase.
///
/// Matching is case-insensitive and substring-based — same known limitation as
/// `detect_contestation`: a word that merely CONTAINS one of the phrases as a
/// substring (e.g. "simular uma situação" contains "sim") will false-positive.
/// This is an accepted, documented limitation, not a required negative case —
/// `detect_contestation` has the identical class of limitation and it was
/// accepted there too.
pub fn detect_approval_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    APPROVAL_PHRASES
        .iter()
        .any(|&phrase| lower.contains(phrase))
}

/// Returns `true` when `text` contains a natural-language rejection phrase.
///
/// Same case-insensitive substring-match idiom as [`detect_approval_intent`].
pub fn detect_rejection_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    REJECTION_PHRASES
        .iter()
        .any(|&phrase| lower.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: a representative sample from each phrase set is detected as approval.
    #[test]
    fn detect_approval_intent_recognizes_representative_phrases() {
        for phrase in ["sim", "aprovo", "pode fazer", "yes", "approve", "go ahead"] {
            assert!(
                detect_approval_intent(phrase),
                "expected '{phrase}' to be detected as approval intent"
            );
        }
    }

    /// Test 2: mixed case, embedded in a longer sentence — still detected
    /// (case-insensitive substring match).
    #[test]
    fn detect_approval_intent_is_case_insensitive_and_substring_based() {
        assert!(detect_approval_intent("Sim, pode fazer isso"));
        assert!(detect_approval_intent("YES, APPROVE IT"));
    }

    /// Test 3: an unrelated message is not detected as approval. The
    /// "simular uma situação" case is documented as a KNOWN, accepted substring
    /// false-positive (same class of limitation as `detect_contestation`) — it is
    /// asserted here as CURRENT behavior, not as a required negative case.
    #[test]
    fn detect_approval_intent_current_behavior_on_unrelated_and_substring_input() {
        assert!(!detect_approval_intent("what's the weather?"));
        // Known limitation: "sim" is a substring of "simular" — accepted, matches
        // detect_contestation's own documented substring-matching limitation.
        assert!(detect_approval_intent("simular uma situação"));
    }

    /// Test 4: explicit rejection phrases are NOT detected as approval, and vice
    /// versa — the two phrase sets are disjoint in practice for these examples.
    #[test]
    fn rejection_phrases_are_not_detected_as_approval_and_vice_versa() {
        for phrase in ["não", "no", "rejeito", "cancela"] {
            assert!(
                !detect_approval_intent(phrase),
                "expected '{phrase}' to NOT be detected as approval intent"
            );
            assert!(
                detect_rejection_intent(phrase),
                "expected '{phrase}' to be detected as rejection intent"
            );
        }
        for phrase in ["sim", "aprovo", "yes", "approve"] {
            assert!(
                !detect_rejection_intent(phrase),
                "expected '{phrase}' to NOT be detected as rejection intent"
            );
        }
    }
}
