use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{
    BudgetSnapshot, CycleLane, CycleSchedule, CycleStatus, ScheduleOutcome, new_cycle_id,
};
use blake3::Hasher;
use serde_json::{Value, json, to_value};
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, AwarenessEvent, AwarenessEventType, AwarenessFork,
};
use soulseed_agi_core_models::{AwarenessCycleId, EventId};
use soulseed_agi_dfr::types::RouterDecision;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct SchedulerConfig {
    pub max_pending_per_tenant: usize,
    pub allow_parallel_lanes: bool,
    pub clarify_round_limit: u32,
    pub clarify_wait_limit_ms: u64,
    pub clarify_queue_threshold: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_per_tenant: 8,
            allow_parallel_lanes: false,
            clarify_round_limit: 3,
            clarify_wait_limit_ms: 60_000,
            clarify_queue_threshold: 4,
        }
    }
}

#[derive(Default)]
struct SchedulerState {
    pending: HashMap<u64, VecDeque<CycleSchedule>>,
    active: HashMap<u64, CycleLane>,
    clarify_stats: HashMap<u64, ClarifyStats>,
    last_fork: HashMap<u64, AwarenessFork>,
    status: HashMap<u64, CycleStatus>,
}

#[derive(Clone, Copy, Debug)]
struct ClarifyStats {
    rounds: u32,
    first_enqueued_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct CycleScheduler {
    cfg: SchedulerConfig,
    state: Arc<Mutex<SchedulerState>>,
}

impl CycleScheduler {
    pub fn new(cfg: SchedulerConfig) -> Self {
        Self {
            cfg,
            state: Arc::new(Mutex::new(SchedulerState::default())),
        }
    }

    pub fn schedule(
        &self,
        decision: RouterDecision,
        budget: BudgetSnapshot,
        parent_cycle_id: Option<AwarenessCycleId>,
        collab_scope_id: Option<String>,
    ) -> Result<ScheduleOutcome, AceError> {
        let plan = &decision.plan;
        let plan_cycle_id = plan.cycle_id;
        let anchor = plan.anchor.clone();
        let tenant = anchor.tenant_id.into_inner();
        let lane: CycleLane = plan.fork.into();
        let now = OffsetDateTime::now_utc();

        if !self.cfg.allow_parallel_lanes {
            let guard = self.state.lock().unwrap();
            if let Some(active_lane) = guard.active.get(&tenant) {
                if matches!(
                    (active_lane, &lane),
                    (CycleLane::Clarify, CycleLane::Clarify)
                ) {
                    return Ok(ScheduleOutcome {
                        accepted: false,
                        reason: Some("clarify_lane_busy".into()),
                        cycle: None,
                        awareness_events: Vec::new(),
                    });
                }
            }
        }

        let mut guard = self.state.lock().unwrap();
        let queue_len = {
            let queue = guard.pending.entry(tenant).or_default();
            if !self.cfg.allow_parallel_lanes && matches!(lane, CycleLane::Clarify) {
                if queue.iter().any(|c| matches!(c.lane, CycleLane::Clarify)) {
                    return Ok(ScheduleOutcome {
                        accepted: false,
                        reason: Some("clarify_lane_busy".into()),
                        cycle: None,
                        awareness_events: Vec::new(),
                    });
                }
            }
            queue.len()
        };

        if queue_len >= self.cfg.max_pending_per_tenant {
            return Ok(ScheduleOutcome {
                accepted: false,
                reason: Some("pending_limit".into()),
                cycle: None,
                awareness_events: Vec::new(),
            });
        }

        if matches!(lane, CycleLane::Clarify) {
            let queue = guard.pending.entry(tenant).or_default();
            let predicted_queue = queue
                .iter()
                .filter(|c| matches!(c.lane, CycleLane::Clarify))
                .count()
                + 1;
            let stats_entry = guard.clarify_stats.entry(tenant).or_insert(ClarifyStats {
                rounds: 0,
                first_enqueued_at: now,
            });
            if stats_entry.rounds == 0 {
                stats_entry.first_enqueued_at = now;
            }
            let waited_ms = (now - stats_entry.first_enqueued_at)
                .whole_milliseconds()
                .max(0) as u64;
            let predicted_rounds = stats_entry.rounds.saturating_add(1);
            if predicted_rounds > self.cfg.clarify_round_limit
                || waited_ms > self.cfg.clarify_wait_limit_ms
                || predicted_queue > self.cfg.clarify_queue_threshold
            {
                let event =
                    build_degradation_event(&anchor, predicted_rounds, waited_ms, predicted_queue);
                return Ok(ScheduleOutcome {
                    accepted: false,
                    reason: Some("clarify_exhausted".into()),
                    cycle: None,
                    awareness_events: vec![event],
                });
            }
            stats_entry.rounds = predicted_rounds;
        }

        let prev_fork = guard.last_fork.get(&tenant).copied();
        let collab_scope_ref = collab_scope_id.as_deref();
        let decision_events = build_decision_events(
            &anchor,
            plan_cycle_id,
            &decision,
            &lane,
            prev_fork,
            parent_cycle_id,
            collab_scope_ref,
        );
        let explain_fingerprint = fingerprint_decision(&decision);

        let initial_status = if matches!(lane, CycleLane::SelfReason) {
            CycleStatus::Running
        } else {
            CycleStatus::AwaitingExternal
        };

        let cycle = CycleSchedule {
            cycle_id: plan_cycle_id,
            lane: lane.clone(),
            anchor: anchor.clone(),
            budget: budget.clone(),
            created_at: now,
            router_decision: decision,
            decision_events: decision_events.clone(),
            explain_fingerprint: explain_fingerprint.clone(),
            status: initial_status,
            parent_cycle_id,
            collab_scope_id: collab_scope_id.clone(),
        };

        guard
            .pending
            .entry(tenant)
            .or_default()
            .push_back(cycle.clone());
        guard.last_fork.insert(tenant, lane.as_fork());
        guard.status.insert(plan_cycle_id.as_u64(), initial_status);
        Ok(ScheduleOutcome {
            accepted: true,
            reason: None,
            cycle: Some(cycle),
            awareness_events: decision_events,
        })
    }

    pub fn start_next(&self, tenant: u64) -> Option<CycleSchedule> {
        let mut guard = self.state.lock().unwrap();
        let queue = guard.pending.get_mut(&tenant)?;
        let mut cycle = queue.pop_front()?;
        let current_status = guard.status.get(&cycle.cycle_id.as_u64()).copied();
        match current_status {
            Some(CycleStatus::AwaitingExternal | CycleStatus::Suspended) => {
                cycle.status = current_status.unwrap();
            }
            Some(status) if status != CycleStatus::Running => {
                cycle.status = status;
            }
            _ => {
                cycle.status = CycleStatus::Running;
                guard
                    .status
                    .insert(cycle.cycle_id.as_u64(), CycleStatus::Running);
            }
        }
        guard.active.insert(tenant, cycle.lane.clone());
        Some(cycle)
    }

    pub fn finish(&self, tenant: u64, cycle_id: AwarenessCycleId, status: CycleStatus) {
        let mut guard = self.state.lock().unwrap();
        guard.active.remove(&tenant);
        guard.status.insert(cycle_id.as_u64(), status);
        if matches!(status, CycleStatus::Completed | CycleStatus::Failed) {
            guard.status.remove(&cycle_id.as_u64());
        }
        if let Some(stats) = guard.clarify_stats.get_mut(&tenant) {
            stats.rounds = stats.rounds.saturating_sub(1);
            if stats.rounds == 0 {
                guard.clarify_stats.remove(&tenant);
            }
        }
    }

    pub fn mark_status(&self, cycle_id: AwarenessCycleId, status: CycleStatus) {
        let mut guard = self.state.lock().unwrap();
        guard.status.insert(cycle_id.as_u64(), status);
    }

    pub fn status_of(&self, cycle_id: AwarenessCycleId) -> Option<CycleStatus> {
        let guard = self.state.lock().unwrap();
        guard.status.get(&cycle_id.as_u64()).copied()
    }
}

fn build_decision_events(
    anchor: &AwarenessAnchor,
    cycle_id: AwarenessCycleId,
    decision: &RouterDecision,
    lane: &CycleLane,
    previous: Option<AwarenessFork>,
    parent_cycle_id: Option<AwarenessCycleId>,
    collab_scope_id: Option<&str>,
) -> Vec<AwarenessEvent> {
    let mut events = Vec::new();
    let mut next_event_id = cycle_id.as_u64();
    let occurred_at_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let mut degradation = decision.decision_path.degradation_reason;
    if degradation.is_none() {
        degradation = map_degradation(decision.plan.explain.degradation_reason.as_deref());
    }

    let plan_value = to_value(&decision.decision_path.plan).unwrap_or_else(|_| Value::Null);
    let rationale_value =
        to_value(&decision.decision_path.rationale).unwrap_or_else(|_| Value::Null);
    let budget_plan_value =
        to_value(&decision.decision_path.budget_plan).unwrap_or_else(|_| Value::Null);
    let decision_path_value = to_value(&decision.decision_path).unwrap_or_else(|_| Value::Null);
    let route_plan_value = to_value(&decision.plan).unwrap_or_else(|_| Value::Null);
    let collab_scope_string = collab_scope_id.map(|s| s.to_string());

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::AwarenessCycleStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: None,
        payload: json!({
            "lane": format!("{:?}", lane),
            "context_digest": decision.context_digest,
            "router_digest": decision.plan.explain.router_digest,
            "router_config_digest": decision.plan.explain.router_config_digest,
            "routing_seed": decision.plan.explain.routing_seed,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::InferenceCycleStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: None,
        payload: json!({
            "lane": format!("{:?}", lane),
            "ic_sequence": decision.decision_path.inference_cycle_sequence,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::AssessmentProduced,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: degradation,
        payload: json!({
            "lane": format!("{:?}", lane),
            "plan": plan_value.clone(),
            "rationale": rationale_value,
            "budget_plan": budget_plan_value,
            "confidence": decision.decision_path.confidence,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::DecisionRouted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: degradation,
        payload: json!({
            "fork": format!("{:?}", lane),
            "decision_path": decision_path_value,
            "route_plan": route_plan_value,
            "rejected": decision.rejected,
        }),
    });
    next_event_id += 1;

    if let Some(prev) = previous {
        if prev != lane.as_fork() {
            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId::from_raw_unchecked(next_event_id),
                event_type: AwarenessEventType::RouteReconsidered,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id,
                collab_scope_id: collab_scope_string.clone(),
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
                degradation_reason: degradation,
                payload: json!({
                    "from": format!("{:?}", CycleLane::from(prev)),
                    "to": format!("{:?}", lane),
                    "rejected": decision.rejected,
                }),
            });
            next_event_id += 1;

            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId::from_raw_unchecked(next_event_id),
                event_type: AwarenessEventType::RouteSwitched,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id,
                collab_scope_id: collab_scope_string,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
                degradation_reason: degradation,
                payload: json!({
                    "from": format!("{:?}", CycleLane::from(prev)),
                    "to": format!("{:?}", lane),
                    "router_digest": decision.plan.explain.router_digest,
                }),
            });
        }
    }

    events
}

fn fingerprint_decision(decision: &RouterDecision) -> Option<String> {
    let mut hasher = Hasher::new();
    let path_bytes = serde_json::to_vec(&decision.decision_path).ok()?;
    hasher.update(&path_bytes);
    if let Ok(plan_bytes) = serde_json::to_vec(&decision.plan.explain) {
        hasher.update(&plan_bytes);
    }
    Some(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn build_degradation_event(
    anchor: &AwarenessAnchor,
    rounds: u32,
    wait_ms: u64,
    queue_depth: usize,
) -> AwarenessEvent {
    let cycle_id = new_cycle_id();
    AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(cycle_id.as_u64()),
        event_type: AwarenessEventType::DecisionRouted,
        occurred_at_ms: OffsetDateTime::now_utc().unix_timestamp() * 1000,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: Some(AwarenessDegradationReason::ClarifyExhausted),
        payload: json!({
            "lane": "clarify",
            "reason": "clarify_exhausted",
            "rounds": rounds,
            "wait_ms": wait_ms,
            "queue_depth": queue_depth,
        }),
    }
}

fn map_degradation(reason: Option<&str>) -> Option<AwarenessDegradationReason> {
    match reason {
        Some("clarify_exhausted") => Some(AwarenessDegradationReason::ClarifyExhausted),
        Some("graph_degraded") => Some(AwarenessDegradationReason::GraphDegraded),
        Some("envctx_degraded") => Some(AwarenessDegradationReason::EnvctxDegraded),
        Some("privacy_blocked") => Some(AwarenessDegradationReason::PrivacyBlocked),
        Some("budget_tokens") => Some(AwarenessDegradationReason::BudgetTokens),
        Some("budget_walltime") => Some(AwarenessDegradationReason::BudgetWalltime),
        Some("budget_external_cost") => Some(AwarenessDegradationReason::BudgetExternalCost),
        Some("invalid_plan") => Some(AwarenessDegradationReason::InvalidPlan),
        _ => None,
    }
}
