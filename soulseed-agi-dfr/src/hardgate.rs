use crate::types::{PolicySnapshot, RouterCandidate, RouterInput};
use serde_json::Value;
use soulseed_agi_core_models::{AccessClass, awareness::AwarenessFork};

#[derive(Clone, Copy, Debug)]
pub struct HardGateConfig {
    pub min_intent_clarity: f32,
}

impl Default for HardGateConfig {
    fn default() -> Self {
        Self {
            min_intent_clarity: 0.45,
        }
    }
}

#[derive(Default)]
pub struct HardGateOutcome {
    pub accepted: Vec<RouterCandidate>,
    pub blocked: Vec<(String, String)>,
}

pub struct HardGate {
    config: HardGateConfig,
}

impl Default for HardGate {
    fn default() -> Self {
        Self {
            config: HardGateConfig::default(),
        }
    }
}

impl HardGate {
    pub fn new(config: HardGateConfig) -> Self {
        Self { config }
    }

    pub fn apply(&self, input: &RouterInput, candidates: Vec<RouterCandidate>) -> HardGateOutcome {
        let mut outcome = HardGateOutcome::default();
        let clarity_gate = intent_threshold(&input.policies, self.config.min_intent_clarity);
        let clarity = input.assessment.intent_clarity.unwrap_or(1.0);

        let privacy_requires_provenance =
            matches!(input.anchor.access_class, AccessClass::Restricted)
                && input.anchor.provenance.is_none();

        for candidate in candidates {
            let label = candidate.label();

            if privacy_requires_provenance && candidate.fork != AwarenessFork::Clarify {
                outcome.blocked.push(("privacy_restricted".into(), label));
                continue;
            }

            if clarity < clarity_gate && candidate.fork != AwarenessFork::Clarify {
                outcome.blocked.push(("intent_clarity_low".into(), label));
                continue;
            }

            if is_denied_by_policy(&input.policies, &candidate) {
                outcome.blocked.push(("policy_denied".into(), label));
                continue;
            }

            if risk_above_threshold(&candidate, input.router_config.risk_threshold) {
                outcome
                    .blocked
                    .push(("risk_too_high".into(), candidate.label()));
                continue;
            }

            outcome.accepted.push(candidate);
        }

        outcome
    }
}

fn intent_threshold(policies: &PolicySnapshot, default_threshold: f32) -> f32 {
    policies
        .hard_gates
        .iter()
        .find(|gate| gate.gate.eq_ignore_ascii_case("intent_clarity"))
        .and_then(|gate| gate.threshold)
        .unwrap_or(default_threshold)
}

fn is_denied_by_policy(policies: &PolicySnapshot, candidate: &RouterCandidate) -> bool {
    match candidate.fork {
        AwarenessFork::ToolPath => {
            let label = candidate.label();
            if policies.denied_tools.iter().any(|id| id == &label) {
                return true;
            }
            if !policies.allowed_tools.is_empty()
                && !policies.allowed_tools.iter().any(|id| id == &label)
            {
                return true;
            }
        }
        AwarenessFork::Collab => {
            let label = candidate.label();
            if policies.denied_collabs.iter().any(|id| id == &label) {
                return true;
            }
            if !policies.allowed_collabs.is_empty()
                && !policies.allowed_collabs.iter().any(|id| id == &label)
            {
                return true;
            }
        }
        _ => {}
    }
    false
}

fn risk_above_threshold(candidate: &RouterCandidate, threshold: f32) -> bool {
    if threshold <= 0.0 {
        return false;
    }
    candidate
        .metadata
        .get("estimate")
        .and_then(|est| est.get("risk"))
        .and_then(Value::as_f64)
        .map(|risk| risk as f32 > threshold)
        .or_else(|| {
            candidate
                .metadata
                .get("risk")
                .and_then(Value::as_f64)
                .map(|risk| risk as f32 > threshold)
        })
        .unwrap_or(false)
}
