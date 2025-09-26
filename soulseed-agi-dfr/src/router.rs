use crate::errors::DfrError;
use crate::filter::CandidateFilter;
use crate::planner::RoutePlanner;
use crate::types::{FilterOutcome, RoutePlan, RouterCandidate, RouterInput};
use serde_json::{Value, json};
use soulseed_agi_core_models::awareness::{AwarenessAnchor, AwarenessFork, DecisionPlan};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

pub struct RouterService {
    filter: CandidateFilter,
    planner: RoutePlanner,
    history: Arc<RouteHistory>,
}

impl RouterService {
    pub fn new(filter: CandidateFilter, planner: RoutePlanner) -> Self {
        Self {
            filter,
            planner,
            history: Arc::new(RouteHistory::default()),
        }
    }

    pub fn with_history(
        filter: CandidateFilter,
        planner: RoutePlanner,
        history: Arc<RouteHistory>,
    ) -> Self {
        Self {
            filter,
            planner,
            history,
        }
    }

    pub fn evaluate(
        &self,
        input: &RouterInput,
        candidates: Vec<RouterCandidate>,
    ) -> Result<(FilterOutcome, Vec<RoutePlan>), DfrError> {
        let mut outcome = self.filter.filter(input, candidates);
        if outcome.accepted.is_empty() {
            return Ok((outcome, Vec::new()));
        }

        let mut plans: Vec<RoutePlan> = outcome
            .accepted
            .iter()
            .map(|candidate| self.planner.build_plan(input, candidate, &outcome.rejected))
            .collect();
        let metas: Vec<Value> = outcome
            .accepted
            .iter()
            .map(|candidate| candidate.metadata.clone())
            .collect();

        self.history.apply(input, &mut plans);

        let mut validated = Vec::new();
        let mut invalid_reasons: Vec<String> = Vec::new();

        for (plan, meta) in plans.into_iter().zip(metas.into_iter()) {
            match validate_plan(&plan, &meta) {
                Ok(()) => {
                    validated.push(plan);
                }
                Err(reason) => {
                    outcome
                        .rejected
                        .push(("plan_invalid".into(), reason.clone()));
                    invalid_reasons.push(reason);
                }
            }
        }

        if validated.is_empty() {
            let mut fallback_plan =
                self.planner
                    .build_fallback_clarify(input, &outcome.rejected, "invalid_plan");
            if !invalid_reasons.is_empty() {
                if fallback_plan.explain.diagnostics.is_null() {
                    fallback_plan.explain.diagnostics = json!({});
                }
                if let Some(obj) = fallback_plan.explain.diagnostics.as_object_mut() {
                    obj.insert("invalid_candidates".into(), json!(invalid_reasons));
                }
            }
            self.history
                .apply(input, std::slice::from_mut(&mut fallback_plan));
            validated.push(fallback_plan);
        }

        validated.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(Ordering::Equal)
        });

        for plan in validated.iter_mut() {
            plan.explain.rejected = outcome.rejected.clone();
        }

        Ok((outcome, validated))
    }

    pub fn history(&self) -> Arc<RouteHistory> {
        Arc::clone(&self.history)
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
    policy: StickinessPolicy,
    records: Mutex<HashMap<RouteHistoryKey, RouteHistoryEntry>>,
}

impl Default for RouteHistory {
    fn default() -> Self {
        Self {
            policy: StickinessPolicy::default(),
            records: Mutex::new(HashMap::new()),
        }
    }
}

impl RouteHistory {
    fn apply(&self, input: &RouterInput, plans: &mut [RoutePlan]) {
        if plans.is_empty() {
            return;
        }

        let key = RouteHistoryKey::from_anchor(&input.anchor);
        let entry = {
            let guard = self.records.lock().expect("route history poisoned");
            guard.get(&key).cloned()
        };

        match entry {
            Some(entry) => {
                let now = OffsetDateTime::now_utc();
                let within_cooldown = now - entry.last_issued_at <= self.policy.cooldown;
                for plan in plans.iter_mut() {
                    let plan_kind = DecisionPlanKind::from_plan(&plan.decision_plan);
                    if !plan_kind.is_sticky() {
                        continue;
                    }

                    if plan.fork == entry.last_fork {
                        plan.priority =
                            clamp_priority(plan.priority + self.policy.stickiness_bonus);
                        annotate_oscillation(plan, entry.oscillation);
                    } else if within_cooldown {
                        plan.priority = clamp_priority(plan.priority - self.policy.switch_penalty);
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
            if issued_at - entry.last_issued_at <= self.policy.cooldown {
                entry.oscillation = entry.oscillation.saturating_add(1);
            } else {
                entry.oscillation = 1;
            }
        } else if issued_at - entry.last_issued_at > self.policy.cooldown {
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

    match &plan.decision_plan {
        DecisionPlan::Tool { plan } => {
            if plan.nodes.is_empty() {
                return Err("tool_plan_empty".into());
            }
        }
        DecisionPlan::Collab { plan } => {
            if plan.scope.is_null() {
                return Err("collab_scope_missing".into());
            }
        }
        _ => {}
    }

    Ok(())
}
