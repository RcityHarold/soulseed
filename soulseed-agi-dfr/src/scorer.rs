use crate::types::{BudgetTarget, ContextSignals, RouterCandidate, RouterInput, RouterWeights};
use blake3::Hasher;
use serde_json::{Map, Value, json};
use soulseed_agi_core_models::awareness::AwarenessFork;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ScoreBreakdown {
    pub score: f32,
    pub components: Value,
}

impl ScoreBreakdown {
    pub fn as_value(&self) -> Value {
        json!({
            "score": self.score,
            "components": self.components
        })
    }
}

pub struct ScoreOutcome {
    pub candidates: Vec<RouterCandidate>,
    pub fork_scores: HashMap<AwarenessFork, ScoreBreakdown>,
}

impl Default for ScoreOutcome {
    fn default() -> Self {
        Self {
            candidates: Vec::new(),
            fork_scores: HashMap::new(),
        }
    }
}

#[derive(Default)]
pub struct CandidateScorer;

impl CandidateScorer {
    pub fn score(&self, input: &RouterInput, candidates: Vec<RouterCandidate>) -> ScoreOutcome {
        let mut outcome = ScoreOutcome::default();

        for candidate in candidates {
            let (score, components) = self.compute_score(input, &candidate);
            let mut updated = candidate;
            updated.priority = clamp_score(score);
            enrich_metadata(&mut updated, score, &components);

            let breakdown = ScoreBreakdown {
                score: updated.priority,
                components: components.clone(),
            };

            outcome
                .fork_scores
                .entry(updated.fork)
                .and_modify(|existing| {
                    if breakdown.score > existing.score {
                        *existing = breakdown.clone();
                    }
                })
                .or_insert(breakdown);

            outcome.candidates.push(updated);
        }

        outcome
    }

    fn compute_score(&self, input: &RouterInput, candidate: &RouterCandidate) -> (f32, Value) {
        let weights = &input.router_config.weights;
        let clarity = input.assessment.intent_clarity.unwrap_or(0.5);
        let base_bias = bias_for_fork(candidate.fork, weights);
        let context_alignment = context_alignment(&input.context_signals);
        let risk = candidate
            .metadata
            .get("estimate")
            .and_then(|est| est.get("risk"))
            .and_then(Value::as_f64)
            .map(|r| r as f32)
            .or_else(|| {
                candidate
                    .metadata
                    .get("risk")
                    .and_then(Value::as_f64)
                    .map(|r| r as f32)
            })
            .unwrap_or(0.0);
        let budget_penalty = budget_pressure(candidate, &input.budget);
        let jitter_raw =
            deterministic_jitter(input.routing_seed, candidate, &input.router_config.digest);
        let jitter = jitter_raw * input.router_config.explore_rate.max(0.0);

        let mut score = candidate.priority;
        score += base_bias;
        score += weights.intent_clarity * clarity;
        score += weights.context_alignment * context_alignment;
        score += weights.risk_penalty * risk;
        score += weights.budget_pressure * budget_penalty;
        score += jitter;

        (
            score,
            json!({
                "base_bias": base_bias,
                "candidate_priority": candidate.priority,
                "intent_clarity": weights.intent_clarity * clarity,
                "context_alignment": weights.context_alignment * context_alignment,
                "risk_penalty": weights.risk_penalty * risk,
                "budget_pressure": weights.budget_pressure * budget_penalty,
                "jitter": jitter,
            }),
        )
    }
}

fn bias_for_fork(fork: AwarenessFork, weights: &RouterWeights) -> f32 {
    match fork {
        AwarenessFork::SelfReason => weights.self_bias,
        AwarenessFork::Clarify => weights.clarify_bias,
        AwarenessFork::ToolPath => weights.tool_bias,
        AwarenessFork::Collab => weights.collab_bias,
    }
}

fn context_alignment(signals: &ContextSignals) -> f32 {
    if signals.signals.is_empty() {
        return 0.0;
    }
    let total: f32 = signals
        .signals
        .iter()
        .take(3)
        .map(|signal| signal.weight.unwrap_or(1.0) * signal.score)
        .sum();
    let denom: f32 = signals
        .signals
        .iter()
        .take(3)
        .map(|signal| signal.weight.unwrap_or(1.0))
        .sum();
    if denom <= f32::EPSILON {
        total
    } else {
        total / denom
    }
}

fn budget_pressure(candidate: &RouterCandidate, budget: &BudgetTarget) -> f32 {
    let estimate = candidate.metadata.get("estimate");
    let tokens = estimate
        .and_then(|est| est.get("tokens"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    let walltime = estimate
        .and_then(|est| est.get("walltime_ms"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    let external_cost = estimate
        .and_then(|est| est.get("external_cost"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;

    let token_ratio = if budget.max_tokens > 0 {
        tokens / budget.max_tokens as f32
    } else {
        0.0
    };
    let wall_ratio = if budget.max_walltime_ms > 0 {
        walltime / budget.max_walltime_ms as f32
    } else {
        0.0
    };
    let cost_ratio = if budget.max_external_cost > 0.0 {
        external_cost / budget.max_external_cost
    } else {
        0.0
    };
    let pressure = token_ratio.max(wall_ratio).max(cost_ratio);
    if pressure > 1.0 { pressure - 1.0 } else { 0.0 }
}

fn deterministic_jitter(
    routing_seed: u64,
    candidate: &RouterCandidate,
    config_digest: &str,
) -> f32 {
    let mut hasher = Hasher::new();
    hasher.update(routing_seed.to_le_bytes().as_ref());
    hasher.update(config_digest.as_bytes());
    hasher.update(candidate.label().as_bytes());
    hasher.update(format!("{:?}", candidate.fork).as_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&bytes[..4]);
    let value = u32::from_le_bytes(arr);
    (value as f32 / u32::MAX as f32) - 0.5
}

fn clamp_score(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn enrich_metadata(candidate: &mut RouterCandidate, score: f32, components: &Value) {
    let map = candidate.ensure_metadata_object();
    let routing_entry = map
        .entry("routing")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(obj) = routing_entry.as_object_mut() {
        obj.insert("score".into(), json!(score));
        obj.insert("components".into(), components.clone());
    }
}
