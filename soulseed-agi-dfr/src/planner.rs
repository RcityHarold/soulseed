use blake3::Hasher;
use serde_json::json;
use soulseed_agi_core_models::awareness::{
    AwarenessFork, ClarifyLimits, ClarifyPlan, DecisionBudgetEstimate, DecisionExplain,
    DecisionPath, DecisionPlan, DecisionRationale,
};

use crate::types::{
    BudgetEstimate, RouteExplain, RoutePlan, RouterCandidate, RouterInput, map_degradation,
    new_cycle_id,
};

pub struct RoutePlanner {
    router_config_digest: String,
}

impl Default for RoutePlanner {
    fn default() -> Self {
        Self {
            router_config_digest: format!("blake3:{}", blake3::hash(b"dfr-router-config-v1")),
        }
    }
}

impl RoutePlanner {
    pub fn build_plan(
        &self,
        input: &RouterInput,
        candidate: &RouterCandidate,
        rejected: &[(String, String)],
    ) -> RoutePlan {
        let cycle_id = new_cycle_id();
        let routing_seed = self.compute_seed(&input.scene_label, candidate.priority);
        let mut explain = self.build_explain(candidate, routing_seed, rejected.to_vec());
        if explain.degradation_reason.is_none() {
            explain.degradation_reason = input.context.explain.degradation_reason.clone();
        }
        if explain.indices_used.is_empty() {
            explain.indices_used = input.context.explain.indices_used.clone();
        }
        if explain.query_hash.is_none() {
            explain.query_hash = input.context.explain.query_hash.clone();
        }
        let budget = self.estimate_budget(candidate);

        RoutePlan {
            cycle_id,
            anchor: input.anchor.clone(),
            fork: candidate.fork,
            decision_plan: candidate.decision_plan.clone(),
            budget,
            priority: candidate.priority,
            explain,
        }
    }

    fn compute_seed(&self, scene: &str, priority: f32) -> u64 {
        let mut hasher = Hasher::new();
        hasher.update(scene.as_bytes());
        hasher.update(priority.to_le_bytes().as_ref());
        let bytes = hasher.finalize().as_bytes().to_owned();
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[..8]);
        u64::from_le_bytes(arr)
    }

    fn build_explain(
        &self,
        candidate: &RouterCandidate,
        routing_seed: u64,
        rejected: Vec<(String, String)>,
    ) -> RouteExplain {
        let indices_used = candidate
            .metadata
            .get("indices_used")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let query_hash = candidate
            .metadata
            .get("query_hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let degradation = candidate
            .metadata
            .get("degrade_hint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let diagnostics = candidate
            .metadata
            .get("diagnostics")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let digest = self.router_digest(&candidate.metadata, routing_seed);

        RouteExplain {
            routing_seed,
            router_digest: digest,
            router_config_digest: self.router_config_digest.clone(),
            indices_used,
            query_hash,
            degradation_reason: degradation,
            diagnostics,
            rejected,
        }
    }

    fn router_digest(&self, metadata: &serde_json::Value, routing_seed: u64) -> String {
        let mut hasher = Hasher::new();
        hasher.update(routing_seed.to_le_bytes().as_ref());
        hasher.update(metadata.to_string().as_bytes());
        format!("blake3:{}", hasher.finalize())
    }

    fn estimate_budget(&self, candidate: &RouterCandidate) -> BudgetEstimate {
        let estimate = candidate.metadata.get("estimate");
        BudgetEstimate {
            tokens: estimate
                .and_then(|est| est.get("tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(0),
            walltime_ms: estimate
                .and_then(|est| est.get("walltime_ms"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(0),
            external_cost: estimate
                .and_then(|est| est.get("external_cost"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(0.0),
        }
    }

    pub fn build_decision_path(
        &self,
        input: &RouterInput,
        plan: &RoutePlan,
        sequence: u32,
    ) -> DecisionPath {
        let mut budget_plan = DecisionBudgetEstimate::default();
        if plan.budget.tokens > 0 {
            budget_plan.tokens = Some(plan.budget.tokens);
        }
        if plan.budget.walltime_ms > 0 {
            budget_plan.walltime_ms = Some(plan.budget.walltime_ms);
        }
        if plan.budget.external_cost > 0.0 {
            budget_plan.external_cost = Some(plan.budget.external_cost);
        }

        let mut rationale = DecisionRationale::default();
        rationale
            .scores
            .insert("priority".into(), plan.priority.clamp(0.0, 1.0));
        if let Some(reason) = &plan.explain.degradation_reason {
            rationale
                .thresholds_hit
                .push(format!("candidate_degrade:{}", reason));
        }

        let features = json!({
            "context_reasons": input.context.explain.reasons,
            "context_indices": input.context.explain.indices_used,
        });

        let explain = DecisionExplain {
            routing_seed: plan.explain.routing_seed,
            router_digest: plan.explain.router_digest.clone(),
            router_config_digest: plan.explain.router_config_digest.clone(),
            features_snapshot: Some(features),
        };

        DecisionPath {
            anchor: plan.anchor.clone(),
            awareness_cycle_id: plan.cycle_id,
            inference_cycle_sequence: sequence,
            fork: plan.fork,
            plan: plan.decision_plan.clone(),
            budget_plan,
            rationale,
            confidence: plan.priority.clamp(0.0, 1.0),
            explain,
            degradation_reason: plan
                .explain
                .degradation_reason
                .as_deref()
                .and_then(map_degradation),
        }
    }

    pub fn build_fallback_clarify(
        &self,
        input: &RouterInput,
        rejected: &[(String, String)],
        reason: &str,
    ) -> RoutePlan {
        let candidate = RouterCandidate {
            decision_plan: DecisionPlan::Clarify {
                plan: ClarifyPlan {
                    questions: Vec::new(),
                    limits: ClarifyLimits::default(),
                },
            },
            fork: AwarenessFork::Clarify,
            priority: 0.5,
            metadata: serde_json::json!({
                "label": "fallback_clarify",
                "degrade_hint": reason,
                "diagnostics": { "fallback": true },
            }),
        };

        let mut plan = self.build_plan(input, &candidate, rejected);
        plan.priority = clamp_priority(plan.priority.max(0.35));
        if plan.explain.degradation_reason.is_none() {
            plan.explain.degradation_reason = Some(reason.to_string());
        }
        if plan.explain.diagnostics.is_null() {
            plan.explain.diagnostics = serde_json::json!({});
        }
        if let Some(obj) = plan.explain.diagnostics.as_object_mut() {
            obj.insert("fallback".into(), serde_json::json!(true));
        }
        plan
    }
}

fn clamp_priority(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
