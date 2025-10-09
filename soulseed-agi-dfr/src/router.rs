use crate::errors::DfrError;
use crate::filter::CandidateFilter;
use crate::hardgate::HardGate;
use crate::planner::RoutePlanner;
use crate::scorer::{CandidateScorer, ScoreBreakdown};
use crate::types::{BudgetTarget, CatalogItem, RoutePlan, RouterCandidate, RouterInput, fork_key};
use crate::validator;
use serde_json::{Map, Value, json};
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessFork, ClarifyLimits, ClarifyPlan, ClarifyQuestion, CollabPlan,
    DecisionPlan, SelfPlan, ToolPlan, ToolPlanBarrier, ToolPlanEdge, ToolPlanNode,
};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

pub struct RouterEvaluation {
    pub rejected: Vec<(String, String)>,
    pub plans: Vec<RoutePlan>,
    pub fork_scores: HashMap<AwarenessFork, ScoreBreakdown>,
}

pub struct RouterService {
    hardgate: HardGate,
    filter: CandidateFilter,
    scorer: CandidateScorer,
    planner: RoutePlanner,
    history: Arc<RouteHistory>,
}

impl RouterService {
    pub fn new(
        hardgate: HardGate,
        filter: CandidateFilter,
        scorer: CandidateScorer,
        planner: RoutePlanner,
    ) -> Self {
        Self {
            hardgate,
            filter,
            scorer,
            planner,
            history: Arc::new(RouteHistory::default()),
        }
    }

    pub fn with_history(
        hardgate: HardGate,
        filter: CandidateFilter,
        scorer: CandidateScorer,
        planner: RoutePlanner,
        history: Arc<RouteHistory>,
    ) -> Self {
        Self {
            hardgate,
            filter,
            scorer,
            planner,
            history,
        }
    }

    pub fn evaluate(
        &self,
        input: &RouterInput,
        candidates: Vec<RouterCandidate>,
    ) -> Result<RouterEvaluation, DfrError> {
        let mut candidates = candidates;
        if candidates.is_empty() && !input.catalogs.items.is_empty() {
            candidates = derive_catalog_candidates(input);
        }

        let gate_outcome = self.hardgate.apply(input, candidates);
        let mut rejected = gate_outcome.blocked.clone();

        if gate_outcome.accepted.is_empty() {
            let reason = fallback_reason(&rejected, "empty_catalog");
            let mut fallback = self
                .planner
                .build_fallback_clarify(input, &rejected, &reason);
            fallback.explain.rejected = rejected.clone();
            self.history
                .apply(input, std::slice::from_mut(&mut fallback));
            return Ok(RouterEvaluation {
                rejected,
                plans: vec![fallback],
                fork_scores: HashMap::new(),
            });
        }

        let mut filtered = self.filter.filter(input, gate_outcome.accepted);
        rejected.extend(filtered.rejected.clone());

        if filtered.accepted.is_empty() {
            let reason = fallback_reason(&rejected, "empty_catalog");
            let mut fallback = self
                .planner
                .build_fallback_clarify(input, &rejected, &reason);
            fallback.explain.rejected = rejected.clone();
            self.history
                .apply(input, std::slice::from_mut(&mut fallback));
            return Ok(RouterEvaluation {
                rejected,
                plans: vec![fallback],
                fork_scores: HashMap::new(),
            });
        }

        let mut score_outcome = self.scorer.score(input, filtered.accepted);
        if score_outcome.candidates.is_empty() {
            let reason = fallback_reason(&rejected, "empty_catalog");
            let mut fallback = self
                .planner
                .build_fallback_clarify(input, &rejected, &reason);
            fallback.explain.rejected = rejected.clone();
            self.history
                .apply(input, std::slice::from_mut(&mut fallback));
            return Ok(RouterEvaluation {
                rejected,
                plans: vec![fallback],
                fork_scores: HashMap::new(),
            });
        }

        score_outcome.candidates.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(Ordering::Equal)
        });
        if input.router_config.max_candidates > 0
            && score_outcome.candidates.len() > input.router_config.max_candidates
        {
            for candidate in score_outcome
                .candidates
                .iter()
                .skip(input.router_config.max_candidates)
            {
                rejected.push(("max_candidates_pruned".into(), candidate.label()));
            }
            score_outcome
                .candidates
                .truncate(input.router_config.max_candidates);
        }

        let mut plans: Vec<RoutePlan> = score_outcome
            .candidates
            .iter()
            .map(|candidate| self.planner.build_plan(input, candidate, &rejected))
            .collect();

        let metas: Vec<Value> = score_outcome
            .candidates
            .iter()
            .map(|candidate| candidate.metadata.clone())
            .collect();

        self.history.apply(input, &mut plans);

        let mut validated = Vec::new();
        let mut invalid_reasons: Vec<String> = Vec::new();

        for (plan, meta) in plans.into_iter().zip(metas.into_iter()) {
            match validate_plan(&plan, &meta) {
                Ok(()) => validated.push(plan),
                Err(reason) => {
                    rejected.push(("plan_invalid".into(), reason.clone()));
                    invalid_reasons.push(reason);
                }
            }
        }

        if validated.is_empty() {
            let reason = fallback_reason(&rejected, "invalid_plan");
            let mut fallback = self
                .planner
                .build_fallback_clarify(input, &rejected, &reason);
            if !invalid_reasons.is_empty() {
                if fallback.explain.diagnostics.is_null() {
                    fallback.explain.diagnostics = json!({});
                }
                if let Some(obj) = fallback.explain.diagnostics.as_object_mut() {
                    obj.insert("invalid_candidates".into(), json!(invalid_reasons));
                }
            }
            annotate_fork_scores(&mut fallback, &score_outcome.fork_scores);
            fallback.explain.rejected = rejected.clone();
            self.history
                .apply(input, std::slice::from_mut(&mut fallback));
            return Ok(RouterEvaluation {
                rejected,
                plans: vec![fallback],
                fork_scores: score_outcome.fork_scores,
            });
        }

        let mut fork_scores = score_outcome.fork_scores;
        let mut budget_checked = Vec::new();
        for plan in validated.into_iter() {
            if let Some(reason) = budget_exceeded(&plan, &input.budget) {
                rejected.push((
                    format!("budget_exceeded:{}", reason),
                    fork_key(plan.fork).to_string(),
                ));
                let mut fallback = match plan.fork {
                    AwarenessFork::Collab | AwarenessFork::ToolPath => self
                        .planner
                        .build_fallback_clarify(input, &rejected, reason),
                    AwarenessFork::Clarify => {
                        self.planner.build_fallback_self(input, &rejected, reason)
                    }
                    AwarenessFork::SelfReason => {
                        let mut downgraded = plan;
                        let downgraded_fork = downgraded.fork;
                        annotate_budget_diag(
                            &mut downgraded,
                            Some(&fork_scores),
                            reason,
                            fork_key(downgraded_fork),
                            Some(fork_key(AwarenessFork::SelfReason)),
                        );
                        budget_checked.push(downgraded);
                        continue;
                    }
                };
                let fallback_fork = fallback.fork;
                annotate_budget_diag(
                    &mut fallback,
                    Some(&fork_scores),
                    reason,
                    fork_key(fallback_fork),
                    Some(fork_key(plan.fork)),
                );
                fallback.explain.rejected = rejected.clone();
                fork_scores.entry(fallback.fork).or_insert(ScoreBreakdown {
                    score: fallback.priority,
                    components: json!({
                        "fallback": true,
                        "reason": reason,
                    }),
                });
                budget_checked.push(fallback);
            } else {
                budget_checked.push(plan);
            }
        }

        let mut validated = budget_checked;
        validated.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(Ordering::Equal)
        });

        let fork_scores_value = fork_scores_value(&fork_scores);
        for plan in validated.iter_mut() {
            plan.explain.rejected = rejected.clone();
            if plan.explain.diagnostics.is_null() {
                plan.explain.diagnostics = json!({});
            }
            if let Some(obj) = plan.explain.diagnostics.as_object_mut() {
                obj.insert("fork_scores".into(), fork_scores_value.clone());
            }
        }

        Ok(RouterEvaluation {
            rejected,
            plans: validated,
            fork_scores,
        })
    }

    pub fn history(&self) -> Arc<RouteHistory> {
        Arc::clone(&self.history)
    }
}

fn annotate_budget_diag(
    plan: &mut RoutePlan,
    fork_scores: Option<&HashMap<AwarenessFork, ScoreBreakdown>>,
    reason: &str,
    fork_label: &str,
    origin_fork: Option<&str>,
) {
    if plan.explain.degradation_reason.is_none() {
        plan.explain.degradation_reason = Some(reason.to_string());
    }

    if plan.explain.diagnostics.is_null() {
        plan.explain.diagnostics = json!({});
    }

    if let Some(obj) = plan.explain.diagnostics.as_object_mut() {
        obj.insert(
            "budget_exceeded".into(),
            json!({
                "fork": fork_label,
                "reason": reason,
                "origin": origin_fork,
            }),
        );
    }

    if let Some(scores) = fork_scores {
        if !scores.is_empty() {
            annotate_fork_scores(plan, scores);
        }
    }
}

fn derive_catalog_candidates(input: &RouterInput) -> Vec<RouterCandidate> {
    input
        .catalogs
        .items
        .iter()
        .filter_map(build_candidate_from_catalog)
        .collect()
}

fn build_candidate_from_catalog(item: &CatalogItem) -> Option<RouterCandidate> {
    let decision_plan = match item.fork {
        AwarenessFork::SelfReason => build_self_plan(&item.metadata),
        AwarenessFork::Clarify => build_clarify_plan(&item.metadata),
        AwarenessFork::ToolPath => build_tool_plan(item)?,
        AwarenessFork::Collab => build_collab_plan(&item.metadata)?,
    };

    let mut candidate = RouterCandidate {
        decision_plan,
        fork: item.fork,
        priority: item.score_hint.unwrap_or(0.5),
        metadata: item.metadata.clone(),
    };

    {
        let score_hint = candidate.priority;
        let meta = candidate.ensure_metadata_object();
        meta.insert(
            "label".into(),
            json!(item.label.clone().unwrap_or_else(|| item.id.clone())),
        );
        meta.insert("catalog_id".into(), json!(item.id.clone()));
        meta.insert("score_hint".into(), json!(score_hint));

        if let Some(estimate) = &item.estimate {
            meta.insert(
                "estimate".into(),
                json!({
                    "tokens": estimate.tokens,
                    "walltime_ms": estimate.walltime_ms,
                    "external_cost": estimate.external_cost,
                }),
            );
        }
        if let Some(risk) = item.risk {
            let estimate = meta.entry("estimate").or_insert_with(|| json!({}));
            if let Some(obj) = estimate.as_object_mut() {
                obj.insert("risk".into(), json!(risk));
            }
        }
    }

    Some(candidate)
}

fn build_self_plan(metadata: &Value) -> DecisionPlan {
    let hint = metadata
        .get("hint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let max_ic = metadata
        .get("max_ic")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    DecisionPlan::SelfReason {
        plan: SelfPlan { hint, max_ic },
    }
}

fn build_clarify_plan(metadata: &Value) -> DecisionPlan {
    let questions = metadata
        .get("questions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, value)| match value {
                    Value::String(text) => Some(ClarifyQuestion {
                        q_id: format!("catalog_q{}", idx + 1),
                        text: text.clone(),
                    }),
                    Value::Object(obj) => {
                        let q_id = obj
                            .get("q_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("catalog_q{}", idx + 1));
                        let text = obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())?;
                        Some(ClarifyQuestion { q_id, text })
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut limits = ClarifyLimits::default();
    if let Some(obj) = metadata.get("limits").and_then(|v| v.as_object()) {
        limits.max_parallel = obj
            .get("max_parallel")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        limits.max_rounds = obj
            .get("max_rounds")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        limits.wait_ms = obj
            .get("wait_ms")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        limits.total_wait_ms = obj
            .get("total_wait_ms")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
    }

    DecisionPlan::Clarify {
        plan: ClarifyPlan { questions, limits },
    }
}

fn build_tool_plan(item: &CatalogItem) -> Option<DecisionPlan> {
    let metadata = &item.metadata;
    let nodes = if let Some(arr) = metadata.get("nodes").and_then(|v| v.as_array()) {
        let mut nodes = Vec::with_capacity(arr.len());
        for node in arr {
            let id = node.get("id")?.as_str()?.to_string();
            let tool_id = node.get("tool_id")?.as_str()?.to_string();
            let version = node
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let input = node.get("input").cloned().unwrap_or_else(|| json!({}));
            let timeout_ms = node
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let success_criteria = node.get("success_criteria").cloned();
            let evidence_policy = node
                .get("evidence_policy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            nodes.push(ToolPlanNode {
                id,
                tool_id,
                version,
                input,
                timeout_ms,
                success_criteria,
                evidence_policy,
            });
        }
        nodes
    } else {
        let tool_id = metadata
            .get("tool_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())?;
        let node_id = metadata
            .get("node_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("node-{}", item.id));
        let input = metadata.get("input").cloned().unwrap_or_else(|| json!({}));
        let timeout_ms = metadata
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let success_criteria = metadata.get("success_criteria").cloned();
        let evidence_policy = metadata
            .get("evidence_policy")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        vec![ToolPlanNode {
            id: node_id,
            tool_id,
            version: metadata
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            input,
            timeout_ms,
            success_criteria,
            evidence_policy,
        }]
    };

    if nodes.is_empty() {
        return None;
    }

    let edges = metadata
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|edge| {
                    let from = edge.get("from")?.as_str()?.to_string();
                    let to = edge.get("to")?.as_str()?.to_string();
                    Some(ToolPlanEdge { from, to })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let barrier = parse_barrier(metadata);

    Some(DecisionPlan::Tool {
        plan: ToolPlan {
            nodes,
            edges,
            barrier,
        },
    })
}

fn build_collab_plan(metadata: &Value) -> Option<DecisionPlan> {
    let scope = metadata
        .get("scope")
        .cloned()
        .unwrap_or_else(|| json!({ "channel": "collab" }));
    if scope.is_null() {
        return None;
    }
    let order = metadata
        .get("order")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let rounds = metadata
        .get("rounds")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let privacy_mode = metadata
        .get("privacy_mode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let barrier = parse_barrier(metadata);

    Some(DecisionPlan::Collab {
        plan: CollabPlan {
            scope,
            order,
            rounds,
            privacy_mode,
            barrier,
        },
    })
}

fn parse_barrier(metadata: &Value) -> ToolPlanBarrier {
    let mut barrier = ToolPlanBarrier::default();
    if let Some(obj) = metadata.get("barrier").and_then(|v| v.as_object()) {
        barrier.mode = obj
            .get("mode")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        barrier.timeout_ms = obj
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
    }
    barrier
}
fn budget_exceeded(plan: &RoutePlan, target: &BudgetTarget) -> Option<&'static str> {
    if target.max_tokens > 0 && plan.budget.tokens > target.max_tokens {
        return Some("budget_tokens");
    }
    if target.max_walltime_ms > 0 && plan.budget.walltime_ms > target.max_walltime_ms {
        return Some("budget_walltime");
    }
    if target.max_external_cost > 0.0 && plan.budget.external_cost > target.max_external_cost {
        return Some("budget_external_cost");
    }
    None
}

fn fallback_reason(rejected: &[(String, String)], default_reason: &str) -> String {
    for code in [
        "intent_clarity_low",
        "privacy_restricted",
        "policy_denied",
        "risk_too_high",
    ] {
        if rejected.iter().any(|(c, _)| c == code) {
            return code.to_string();
        }
    }
    default_reason.to_string()
}

fn fork_scores_value(scores: &HashMap<AwarenessFork, ScoreBreakdown>) -> Value {
    let mut map = Map::new();
    for (fork, breakdown) in scores {
        map.insert(fork_key(*fork).to_string(), breakdown.as_value());
    }
    Value::Object(map)
}

fn annotate_fork_scores(
    plan: &mut RoutePlan,
    fork_scores: &HashMap<AwarenessFork, ScoreBreakdown>,
) {
    if fork_scores.is_empty() {
        return;
    }
    if plan.explain.diagnostics.is_null() {
        plan.explain.diagnostics = json!({});
    }
    if let Some(obj) = plan.explain.diagnostics.as_object_mut() {
        obj.insert("fork_scores".into(), fork_scores_value(fork_scores));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StickinessPolicy {
    pub cooldown: Duration,
    pub stickiness_bonus: f32,
    pub switch_penalty: f32,
}

impl Default for StickinessPolicy {
    fn default() -> Self {
        Self {
            cooldown: Duration::seconds(5),
            stickiness_bonus: 0.09,
            switch_penalty: 0.18,
        }
    }
}

impl StickinessPolicy {
    fn from_config(config: &crate::types::RouterStickiness) -> Self {
        Self {
            cooldown: Duration::milliseconds(config.cooldown_ms as i64),
            stickiness_bonus: config.stickiness_bonus,
            switch_penalty: config.switch_penalty,
        }
    }
}

#[derive(Clone, Debug)]
struct RouteHistoryEntry {
    last_fork: AwarenessFork,
    last_plan_kind: DecisionPlanKind,
    last_issued_at: OffsetDateTime,
    oscillation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RouteHistoryKey {
    tenant: u64,
    session: u64,
}

impl RouteHistoryKey {
    fn from_anchor(anchor: &AwarenessAnchor) -> Self {
        let tenant = anchor.tenant_id.into_inner();
        let session = anchor
            .session_id
            .map(|id| id.into_inner())
            .unwrap_or_default();
        Self { tenant, session }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecisionPlanKind {
    SelfReason,
    Clarify,
    Tool,
    Collab,
}

impl DecisionPlanKind {
    fn from_plan(plan: &DecisionPlan) -> Self {
        match plan {
            DecisionPlan::SelfReason { .. } => Self::SelfReason,
            DecisionPlan::Clarify { .. } => Self::Clarify,
            DecisionPlan::Tool { .. } => Self::Tool,
            DecisionPlan::Collab { .. } => Self::Collab,
        }
    }

    fn is_sticky(self) -> bool {
        matches!(self, Self::Clarify | Self::Tool | Self::Collab)
    }
}

#[derive(Debug)]
pub struct RouteHistory {
    records: Mutex<HashMap<RouteHistoryKey, RouteHistoryEntry>>,
}

impl Default for RouteHistory {
    fn default() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }
}

impl RouteHistory {
    fn apply(&self, input: &RouterInput, plans: &mut [RoutePlan]) {
        if plans.is_empty() {
            return;
        }

        let policy = StickinessPolicy::from_config(&input.router_config.stickiness);
        let key = RouteHistoryKey::from_anchor(&input.anchor);
        let entry = {
            let guard = self.records.lock().expect("route history poisoned");
            guard.get(&key).cloned()
        };

        match entry {
            Some(entry) => {
                let now = OffsetDateTime::now_utc();
                let within_cooldown = now - entry.last_issued_at <= policy.cooldown;
                for plan in plans.iter_mut() {
                    let plan_kind = DecisionPlanKind::from_plan(&plan.decision_plan);
                    if !plan_kind.is_sticky() {
                        continue;
                    }

                    if plan.fork == entry.last_fork {
                        plan.priority = clamp_priority(plan.priority + policy.stickiness_bonus);
                        annotate_oscillation(plan, entry.oscillation);
                    } else if within_cooldown {
                        plan.priority = clamp_priority(plan.priority - policy.switch_penalty);
                        annotate_oscillation(plan, entry.oscillation.saturating_add(1));
                    } else {
                        annotate_oscillation(plan, 0);
                    }
                }
            }
            None => {
                for plan in plans.iter_mut() {
                    let plan_kind = DecisionPlanKind::from_plan(&plan.decision_plan);
                    if plan_kind.is_sticky() {
                        annotate_oscillation(plan, 0);
                    }
                }
            }
        }
    }

    pub fn record_outcome(
        &self,
        input: &RouterInput,
        plan: &RoutePlan,
        issued_at: OffsetDateTime,
    ) -> u32 {
        let key = RouteHistoryKey::from_anchor(&input.anchor);
        let mut guard = self.records.lock().expect("route history poisoned");
        let plan_kind = DecisionPlanKind::from_plan(&plan.decision_plan);
        let policy = StickinessPolicy::from_config(&input.router_config.stickiness);

        let entry = guard.entry(key).or_insert(RouteHistoryEntry {
            last_fork: plan.fork,
            last_plan_kind: plan_kind,
            last_issued_at: issued_at,
            oscillation: 0,
        });

        if !plan_kind.is_sticky() {
            entry.last_fork = plan.fork;
            entry.last_plan_kind = plan_kind;
            entry.last_issued_at = issued_at;
            entry.oscillation = 0;
            return 0;
        }

        if entry.last_plan_kind != plan_kind || entry.last_fork != plan.fork {
            if issued_at - entry.last_issued_at <= policy.cooldown {
                entry.oscillation = entry.oscillation.saturating_add(1);
            } else {
                entry.oscillation = 1;
            }
        } else if issued_at - entry.last_issued_at > policy.cooldown {
            entry.oscillation = 0;
        }

        entry.last_fork = plan.fork;
        entry.last_plan_kind = plan_kind;
        entry.last_issued_at = issued_at;
        entry.oscillation
    }

    pub fn annotate(plan: &mut RoutePlan, oscillation: u32) {
        annotate_oscillation(plan, oscillation);
    }
}

fn clamp_priority(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn annotate_oscillation(plan: &mut RoutePlan, oscillation: u32) {
    if plan.explain.diagnostics.is_null() {
        plan.explain.diagnostics = json!({});
    }

    if let Some(obj) = plan.explain.diagnostics.as_object_mut() {
        obj.insert("route_oscillation".into(), json!(oscillation));
    }
}

fn validate_plan(plan: &RoutePlan, metadata: &Value) -> Result<(), String> {
    if let Some(validation) = metadata.get("validation") {
        if validation
            .get("status")
            .and_then(|v| v.as_str())
            .map(|v| v.eq_ignore_ascii_case("invalid"))
            .unwrap_or(false)
        {
            let reason = validation
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("invalid_plan");
            return Err(reason.to_string());
        }
    }

    validator::validate_decision_plan(plan)
}
