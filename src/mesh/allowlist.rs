//! Per-owner export tag allowlist (D-03, D-05).
//! filter_for_mesh runs BEFORE check_egress — LocalOnly beliefs are stripped here
//! so the egress gate only ever sees CloudOk beliefs for mesh.

use crate::hooks::egress::check_egress;
use crate::memory::Belief;

/// Declares which belief tags this owner permits to flow to a specific peer.
#[derive(Debug, Clone)]
pub struct OwnerAllowlist {
    pub owner_id: String,
    /// Tags the remote peer may receive. Conservative: belief with no tag → filtered out.
    pub allowed_tags: Vec<String>,
}

/// Filter beliefs to only those the allowlist permits AND whose tier allows egress.
///
/// Two-stage:
/// 1. Tag allowlist: belief.persona_tag must be in allowed_tags. No tag → filtered out.
/// 2. Egress gate: check_egress(belief.tier, "mesh") — LocalOnly always denied.
///
/// Result: only CloudOk beliefs with an explicitly-allowlisted tag survive.
pub fn filter_for_mesh(beliefs: Vec<Belief>, allowlist: &OwnerAllowlist) -> Vec<Belief> {
    beliefs
        .into_iter()
        .filter(|b| {
            // Stage 1: tag allowlist
            let tag_ok = b
                .persona_tag
                .as_ref()
                .map(|t| allowlist.allowed_tags.contains(t))
                .unwrap_or(false); // no tag → deny (conservative)
            if !tag_ok {
                return false;
            }
            // Stage 2: egress gate — LocalOnly and None tier are denied
            check_egress(b.tier, "mesh").is_ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Belief, PrivacyTier};

    fn make_belief(tag: Option<&str>, tier: Option<PrivacyTier>) -> Belief {
        Belief {
            id: 0,
            owner_id: "mario".to_string(),
            persona_tag: tag.map(|t| t.to_string()),
            content: "test belief".to_string(),
            weight: 1.0,
            is_core: false,
            tier,
        }
    }

    fn allowlist(tags: &[&str]) -> OwnerAllowlist {
        OwnerAllowlist {
            owner_id: "ana".to_string(),
            allowed_tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn cloudok_with_allowed_tag_passes() {
        let beliefs = vec![make_belief(Some("mercado"), Some(PrivacyTier::CloudOk))];
        let result = filter_for_mesh(beliefs, &allowlist(&["mercado", "calendario"]));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn localonly_always_filtered_even_if_tag_allowed() {
        let beliefs = vec![make_belief(Some("mercado"), Some(PrivacyTier::LocalOnly))];
        let result = filter_for_mesh(beliefs, &allowlist(&["mercado"]));
        assert!(result.is_empty(), "LocalOnly must never leave the node");
    }

    #[test]
    fn tag_not_in_allowlist_filtered() {
        let beliefs = vec![make_belief(Some("saude"), Some(PrivacyTier::CloudOk))];
        let result = filter_for_mesh(beliefs, &allowlist(&["mercado"]));
        assert!(result.is_empty());
    }

    #[test]
    fn no_tag_filtered() {
        let beliefs = vec![make_belief(None, Some(PrivacyTier::CloudOk))];
        let result = filter_for_mesh(beliefs, &allowlist(&["mercado"]));
        assert!(result.is_empty());
    }

    #[test]
    fn none_tier_filtered() {
        let beliefs = vec![make_belief(Some("mercado"), None)];
        let result = filter_for_mesh(beliefs, &allowlist(&["mercado"]));
        assert!(result.is_empty());
    }
}
