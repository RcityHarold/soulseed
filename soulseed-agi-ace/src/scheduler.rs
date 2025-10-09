use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{BudgetSnapshot, CycleLane, CycleSchedule, ScheduleOutcome, new_cycle_id};
use blake3::Hasher;
use serde_json::{Value, json};
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, AwarenessEvent, AwarenessEventType,
};
use soulseed_agi_core_models::{CycleId, EventId};
use soulseed_agi_tools::dto::ToolPlan;
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
        anchor: AwarenessAnchor,
        lane: CycleLane,
        tool_plan: Option<ToolPlan>,
        llm_plan: Option<Value>,
        budget: BudgetSnapshot,
    ) -> Result<ScheduleOutcome, AceError> {
        let tenant = anchor.tenant_id.into_inner();
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
        let existing = {
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

        if existing >= self.cfg.max_pending_per_tenant {
            return Ok(ScheduleOutcome {
                accepted: false,
                reason: Some("pending_limit".into()),
                cycle: None,
                awareness_events: Vec::new(),
            });
        }

        let predicted_queue = if matches!(lane, CycleLane::Clarify) {
            let queue = guard.pending.entry(tenant).or_default();
            queue
                .iter()
                .filter(|c| matches!(c.lane, CycleLane::Clarify))
                .count()
                + 1
        } else {
            0
        };

        if matches!(lane, CycleLane::Clarify) {
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

        let cycle_id = new_cycle_id();
        let decision_events = build_decision_events(&anchor, cycle_id, &lane, llm_plan.as_ref());
        let explain_fingerprint = fingerprint_decision(&decision_events);

        let cycle = CycleSchedule {
            cycle_id,
            lane,
            anchor,
            tool_plan,
            llm_plan,
            budget,
            created_at: now,
            decision_events: decision_events.clone(),
            explain_fingerprint: explain_fingerprint.clone(),
        };

        guard
            .pending
            .entry(tenant)
            .or_default()
            .push_back(cycle.clone());
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
        let cycle = queue.pop_front()?;
        guard.active.insert(tenant, cycle.lane.clone());
        Some(cycle)
    }

    pub fn finish(&self, tenant: u64) {
        let mut guard = self.state.lock().unwrap();
        guard.active.remove(&tenant);
        if let Some(stats) = guard.clarify_stats.get_mut(&tenant) {
            stats.rounds = stats.rounds.saturating_sub(1);
            if stats.rounds == 0 {
                guard.clarify_stats.remove(&tenant);
            }
        }
    }
}

fn build_decision_events(
    anchor: &AwarenessAnchor,
    cycle_id: CycleId,
    lane: &CycleLane,
    llm_plan: Option<&Value>,
) -> Vec<AwarenessEvent> {
    let mut events = Vec::new();
    let mut next_event_id = cycle_id.0;
    let occurred_at_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let degradation = llm_plan
        .and_then(|plan| plan.get("degradation_reason").and_then(Value::as_str))
        .and_then(map_degradation);

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId(next_event_id),
        event_type: AwarenessEventType::AcStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: None,
        payload: json!({"lane": format!("{:?}", lane)}),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId(next_event_id),
        event_type: AwarenessEventType::IcStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: None,
        payload: json!({"lane": format!("{:?}", lane)}),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId(next_event_id),
        event_type: AwarenessEventType::DecisionRouted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: degradation,
        payload: json!({
            "lane": format!("{:?}", lane),
            "explain": llm_plan.unwrap_or(&Value::Null),
        }),
    });
    next_event_id += 1;

    match lane {
        CycleLane::Tool | CycleLane::Collab => {
            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId(next_event_id),
                event_type: AwarenessEventType::RouteSwitched,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: degradation,
                payload: json!({"lane": format!("{:?}", lane)}),
            });
        }
        CycleLane::SelfReason => {
            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId(next_event_id),
                event_type: AwarenessEventType::AssessmentProduced,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: degradation,
                payload: json!({"lane": "self_reason"}),
            });
        }
        CycleLane::Clarify => {}
    }

    events
}

fn fingerprint_decision(events: &[AwarenessEvent]) -> Option<String> {
    if events.is_empty() {
        return None;
    }
    let mut hasher = Hasher::new();
    for event in events {
        hasher.update(&event.event_id.0.to_le_bytes());
        hasher.update(format!("{:?}", event.event_type).as_bytes());
        hasher.update(event.payload.to_string().as_bytes());
        if let Some(reason) = event.degradation_reason {
            hasher.update(format!("{:?}", reason).as_bytes());
        }
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
        event_id: EventId(cycle_id.0),
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

fn map_degradation(reason: &str) -> Option<AwarenessDegradationReason> {
    match reason {
        "clarify_exhausted" => Some(AwarenessDegradationReason::ClarifyExhausted),
        "graph_degraded" => Some(AwarenessDegradationReason::GraphDegraded),
        "envctx_degraded" => Some(AwarenessDegradationReason::EnvctxDegraded),
        "privacy_blocked" => Some(AwarenessDegradationReason::PrivacyBlocked),
        "budget_tokens" => Some(AwarenessDegradationReason::BudgetTokens),
        "budget_walltime" => Some(AwarenessDegradationReason::BudgetWalltime),
        "budget_external_cost" => Some(AwarenessDegradationReason::BudgetExternalCost),
        "invalid_plan" => Some(AwarenessDegradationReason::InvalidPlan),
        _ => None,
    }
}
