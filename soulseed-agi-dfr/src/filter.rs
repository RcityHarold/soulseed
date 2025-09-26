use crate::types::{FilterOutcome, RouterCandidate, RouterInput};

pub struct CandidateFilter {
    pub max_risk: Option<f32>,
}

impl Default for CandidateFilter {
    fn default() -> Self {
        Self {
            max_risk: Some(0.8),
        }
    }
}

impl CandidateFilter {
    pub fn filter(&self, input: &RouterInput, candidates: Vec<RouterCandidate>) -> FilterOutcome {
        let mut outcome = FilterOutcome::default();
        for candidate in candidates {
            if input.anchor.access_class == soulseed_agi_core_models::AccessClass::Restricted
                && input.anchor.provenance.is_none()
            {
                outcome
                    .rejected
                    .push(("privacy_restricted".into(), candidate_label(&candidate)));
                continue;
            }

            if let Some(policy) = candidate.metadata.get("policy") {
                if policy
                    .get("allow")
                    .and_then(|v| v.as_bool())
                    .map(|allow| !allow)
                    .unwrap_or(false)
                {
                    outcome
                        .rejected
                        .push(("policy_denied".into(), candidate_label(&candidate)));
                    continue;
                }
            }

            if let Some(max_risk) = self.max_risk {
                if candidate
                    .metadata
                    .get("estimate")
                    .and_then(|est| est.get("risk"))
                    .and_then(|r| r.as_f64())
                    .map(|risk| risk as f32 > max_risk)
                    .unwrap_or(false)
                {
                    outcome
                        .rejected
                        .push(("risk_too_high".into(), candidate_label(&candidate)));
                    continue;
                }
            }

            outcome.accepted.push(candidate);
        }

        outcome
    }
}

fn candidate_label(candidate: &RouterCandidate) -> String {
    candidate
        .metadata
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{:?}", candidate.fork))
}
