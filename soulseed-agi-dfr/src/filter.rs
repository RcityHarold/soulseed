use crate::types::{FilterOutcome, RouterCandidate, RouterInput};
use std::collections::HashSet;

pub struct CandidateFilter;

impl Default for CandidateFilter {
    fn default() -> Self {
        Self
    }
}

impl CandidateFilter {
    pub fn filter(&self, input: &RouterInput, candidates: Vec<RouterCandidate>) -> FilterOutcome {
        let mut outcome = FilterOutcome::default();
        let mut seen: HashSet<(soulseed_agi_core_models::awareness::AwarenessFork, String)> =
            HashSet::new();

        for candidate in candidates {
            let label = candidate.label();

            if let Some(policy) = candidate.metadata.get("policy") {
                if policy
                    .get("allow")
                    .and_then(|v| v.as_bool())
                    .map(|allow| !allow)
                    .unwrap_or(false)
                {
                    outcome
                        .rejected
                        .push(("policy_denied".into(), label.clone()));
                    continue;
                }
            }

            if seen.contains(&(candidate.fork, label.clone())) {
                outcome
                    .rejected
                    .push(("duplicate_candidate".into(), label.clone()));
                continue;
            }

            if is_denied_by_snapshot(&label, &candidate, input) {
                outcome
                    .rejected
                    .push(("policy_denied".into(), label.clone()));
                continue;
            }

            seen.insert((candidate.fork, label));
            outcome.accepted.push(candidate);
        }

        outcome
    }
}

fn is_denied_by_snapshot(label: &str, candidate: &RouterCandidate, input: &RouterInput) -> bool {
    match candidate.fork {
        soulseed_agi_core_models::awareness::AwarenessFork::ToolPath => {
            if input.policies.denied_tools.iter().any(|id| id == label) {
                return true;
            }
            if !input.policies.allowed_tools.is_empty()
                && !input.policies.allowed_tools.iter().any(|id| id == label)
            {
                return true;
            }
        }
        soulseed_agi_core_models::awareness::AwarenessFork::Collab => {
            if input.policies.denied_collabs.iter().any(|id| id == label) {
                return true;
            }
            if !input.policies.allowed_collabs.is_empty()
                && !input.policies.allowed_collabs.iter().any(|id| id == label)
            {
                return true;
            }
        }
        _ => {}
    }
    false
}
