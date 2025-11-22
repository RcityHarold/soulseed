use blake3::Hasher;
use serde_json::{Map, Value, json};
use soulseed_agi_core_models::awareness::{
    AwarenessFork, ClarifyLimits, ClarifyPlan, DecisionBlockReason, DecisionBudgetEstimate,
    DecisionExplain, DecisionInvalidReason, DecisionPath, DecisionPlan, DecisionRationale,
    SelfPlan,
};

use crate::scorer::ScoreBreakdown;
use crate::types::{
    BudgetEstimate, RouteExplain, RoutePlan, RouterCandidate, RouterInput, fork_key,
    map_degradation, new_cycle_id,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap},
};

pub struct RoutePlanner;

impl Default for RoutePlanner {
    fn default() -> Self {
        Self
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
        let routing_seed = self.compute_seed(
            input.routing_seed,
            &input.scene_label,
            candidate,
            &input.router_config.digest,
        );
        let mut explain = self.build_explain(input, candidate, routing_seed, rejected.to_vec());
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

    fn compute_seed(
        &self,
        base_seed: u64,
        scene: &str,
        candidate: &RouterCandidate,
        router_config_digest: &str,
    ) -> u64 {
        let mut hasher = Hasher::new();
        hasher.update(base_seed.to_le_bytes().as_ref());
        hasher.update(scene.as_bytes());
        hasher.update(router_config_digest.as_bytes());
        hasher.update(candidate.priority.to_le_bytes().as_ref());
        hasher.update(format!("{:?}", candidate.fork).as_bytes());
        hasher.update(candidate.metadata.to_string().as_bytes());
        let bytes = hasher.finalize().as_bytes().to_owned();
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[..8]);
        // 确保不超过 i64::MAX，防止 SurrealDB JSON 序列化时变成负数
        u64::from_le_bytes(arr) & (i64::MAX as u64)
    }

    fn build_explain(
        &self,
        input: &RouterInput,
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
        let mut diagnostics = candidate
            .metadata
            .get("diagnostics")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let digest = self.router_digest(
            &input.router_config.digest,
            &candidate.metadata,
            routing_seed,
        );
        if diagnostics.is_null() {
            diagnostics = json!({});
        }

        RouteExplain {
            routing_seed,
            router_digest: digest,
            router_config_digest: input.router_config.digest.clone(),
            indices_used,
            query_hash,
            degradation_reason: degradation,
            diagnostics,
            rejected,
        }
    }

    fn router_digest(
        &self,
        config_digest: &str,
        metadata: &serde_json::Value,
        routing_seed: u64,
    ) -> String {
        let mut hasher = Hasher::new();
        hasher.update(routing_seed.to_le_bytes().as_ref());
        hasher.update(config_digest.as_bytes());
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
        fork_scores: &HashMap<AwarenessFork, ScoreBreakdown>,
        rejected: &[(String, String)],
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
        for (fork, breakdown) in fork_scores {
            rationale
                .scores
                .insert(fork_key(*fork).to_string(), breakdown.score.clamp(0.0, 1.0));
        }
        if let Some(reason) = &plan.explain.degradation_reason {
            rationale
                .thresholds_hit
                .push(format!("candidate_degrade:{}", reason));
        }

        let selected_score = fork_scores
            .get(&plan.fork)
            .map(|score| score.score)
            .unwrap_or(plan.priority);

        let mut ranked: Vec<_> = fork_scores.iter().collect();
        ranked.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(Ordering::Equal));
        if ranked.len() > 1 {
            let top = ranked[0];
            let runner_up = ranked[1];
            rationale.tradeoff = Some(format!(
                "{} {:.2} vs {} {:.2}",
                fork_key(*top.0),
                top.1.score,
                fork_key(*runner_up.0),
                runner_up.1.score
            ));
        } else if let Some(top) = ranked.first() {
            rationale.tradeoff = Some(format!("{} {:.2} selected", fork_key(*top.0), top.1.score));
        }

        let (blocked, invalid) = partition_rejections(rejected);
        if !blocked.is_empty() {
            rationale.blocked = blocked;
        }
        if !invalid.is_empty() {
            rationale.invalid = invalid;
        }

        let mut fork_feature_map = Map::new();
        for (fork, breakdown) in fork_scores {
            fork_feature_map.insert(fork_key(*fork).to_string(), breakdown.as_value());
        }

        let mut features_root = Map::new();
        features_root.insert(
            "context_reasons".into(),
            json!(input.context.explain.reasons),
        );
        features_root.insert(
            "context_indices".into(),
            json!(input.context.explain.indices_used),
        );
        features_root.insert("selected_fork".into(), json!(fork_key(plan.fork)));
        features_root.insert(
            "fork_scores".into(),
            Value::Object(fork_feature_map.clone()),
        );

        let features = Value::Object(features_root);

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
            confidence: selected_score.clamp(0.0, 1.0),
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

    pub fn build_fallback_self(
        &self,
        input: &RouterInput,
        rejected: &[(String, String)],
        reason: &str,
    ) -> RoutePlan {
        let candidate = RouterCandidate {
            decision_plan: DecisionPlan::SelfReason {
                plan: SelfPlan {
                    hint: Some("budget_safety_reflection".into()),
                    max_ic: Some(1),
                },
            },
            fork: AwarenessFork::SelfReason,
            priority: 0.4,
            metadata: serde_json::json!({
                "label": "fallback_self",
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
            obj.insert(
                "fallback".into(),
                serde_json::json!({ "self": true, "reason": reason }),
            );
        }
        plan
    }
}

fn partition_rejections(
    rejected: &[(String, String)],
) -> (Vec<DecisionBlockReason>, Vec<DecisionInvalidReason>) {
    let mut blocked_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut invalid_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (code, label) in rejected {
        let code_lower = code.to_ascii_lowercase();
        if code_lower.contains("invalid") {
            invalid_map
                .entry(label.clone())
                .or_insert_with(BTreeSet::new)
                .insert(code.clone());
            continue;
        }

        let reason = if let Some((prefix, detail)) = code.split_once(':') {
            if prefix.eq_ignore_ascii_case("budget_exceeded") && !detail.is_empty() {
                detail.to_string()
            } else {
                code.clone()
            }
        } else {
            code.clone()
        };

        blocked_map
            .entry(label.clone())
            .or_insert_with(BTreeSet::new)
            .insert(reason);
    }

    let mut blocked = Vec::new();
    for (label, reasons) in blocked_map {
        if reasons.is_empty() {
            blocked.push(DecisionBlockReason {
                id: label,
                reason: None,
            });
        } else {
            for reason in reasons {
                let reason_opt = if reason.is_empty() {
                    None
                } else {
                    Some(reason)
                };
                blocked.push(DecisionBlockReason {
                    id: label.clone(),
                    reason: reason_opt,
                });
            }
        }
    }

    let mut invalid = Vec::new();
    for (label, reasons) in invalid_map {
        for reason in reasons {
            invalid.push(DecisionInvalidReason {
                id: label.clone(),
                reason,
            });
        }
    }

    (blocked, invalid)
}

fn clamp_priority(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
