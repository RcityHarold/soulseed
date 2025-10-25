use soulseed_agi_dfr::{DfrEngine, RoutePlanner, RouterService};
use crate::aggregator::SyncPointAggregator;
use crate::budget::BudgetManager;
use crate::ca::InjectionAction;
use crate::checkpointer::Checkpointer;
use crate::emitter::Emitter;
use crate::errors::AceError;
use crate::hitl::HitlService;
use crate::metrics::AceMetrics;
use crate::outbox::OutboxService;
use crate::scheduler::CycleScheduler;
use crate::types::{
    AggregationOutcome, BudgetSnapshot, CycleEmission, CycleLane, CycleRequest, ScheduleOutcome,
    SyncPointInput,
};
use serde_json::{Map, Value, json};
use soulseed_agi_core_models::AwarenessCycleId;
use soulseed_agi_core_models::awareness::AwarenessEventType;
use soulseed_agi_core_models::common::EvidencePointer;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use time::{Duration, OffsetDateTime};

pub struct AceEngine<'a> {
    pub scheduler: &'a CycleScheduler,
    pub budget: &'a BudgetManager,
    pub aggregator: &'a SyncPointAggregator,
    pub checkpointer: &'a Checkpointer,
    pub outbox: &'a OutboxService,
    pub emitter: &'a Emitter,
    pub hitl: &'a HitlService,
    pub metrics: &'a dyn AceMetrics,
    pub router: &'a RouterService,
    pub route_planner: &'a RoutePlanner,
    pub lane_cooldown: Duration,
    finalized: Arc<Mutex<HashSet<u64>>>,
}

impl<'a> AceEngine<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scheduler: &'a CycleScheduler,
        budget: &'a BudgetManager,
        aggregator: &'a SyncPointAggregator,
        checkpointer: &'a Checkpointer,
        outbox: &'a OutboxService,
        emitter: &'a Emitter,
        hitl: &'a HitlService,
        metrics: &'a dyn AceMetrics,
        router: &'a RouterService,
        route_planner: &'a RoutePlanner,
    ) -> Self {
        Self {
            scheduler,
            budget,
            aggregator,
            checkpointer,
            outbox,
            emitter,
            hitl,
            metrics,
            router,
            route_planner,
            lane_cooldown: Duration::seconds(2),
            finalized: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn schedule_cycle(&self, request: CycleRequest) -> Result<ScheduleOutcome, AceError> {
        let decide_start = Instant::now();
        let dfr = DfrEngine::new(self.router, self.route_planner);
        let decision = dfr
            .route(request.router_input.clone(), request.candidates.clone())
            .map_err(|err| AceError::Dfr(err.to_string()))?;
        let lane: CycleLane = decision.plan.fork.into();
        let anchor = decision.plan.anchor.clone();
        let tenant_raw = anchor.tenant_id.into_inner();
        self.checkpointer
            .ensure_lane_idle(tenant_raw, &lane, self.lane_cooldown)?;

        let decide_latency_ms = decide_start.elapsed().as_secs_f64() * 1000.0;
        self.metrics.gauge(
            "ace.decide.latency_ms",
            decide_latency_ms,
            &[("lane", format!("{:?}", lane))],
        );

        let outcome = self
            .scheduler
            .schedule(decision, request.budget.clone())?;

        if let Some(cycle) = &outcome.cycle {
            self.checkpointer.record(crate::types::CheckpointState {
                tenant_id: cycle.anchor.tenant_id,
                cycle_id: cycle.cycle_id,
                lane: cycle.lane.clone(),
                budget: cycle.budget.clone(),
                since: cycle.created_at,
            });
            if matches!(cycle.lane, CycleLane::Clarify) {
                self.hitl.mark_cycle_active(cycle.anchor.tenant_id);
            }
            let labels = vec![
                ("lane", format!("{:?}", cycle.lane)),
                (
                    "fork",
                    format!("{:?}", cycle.router_decision.plan.fork),
                ),
            ];
            self.metrics.counter("ace.schedule.accepted", 1.0, &labels);
        } else {
            let mut labels = vec![("lane", format!("{:?}", lane))];
            if let Some(reason) = outcome.reason.clone() {
                labels.push(("reason", reason));
            }
            self.metrics.counter("ace.schedule.rejected", 1.0, &labels);
        }
        Ok(outcome)
    }

    pub fn evaluate_budget(
        &self,
        cycle_id: AwarenessCycleId,
        lane: &CycleLane,
        budget: BudgetSnapshot,
    ) -> Result<crate::types::BudgetDecision, AceError> {
        let decision = self.budget.evaluate(cycle_id, lane, budget.clone())?;
        let tags = vec![
            ("lane", format!("{:?}", lane)),
            (
                "allowed",
                if decision.allowed {
                    "true".into()
                } else {
                    "false".into()
                },
            ),
        ];
        self.metrics.gauge(
            "ace.budget.snapshot.tokens",
            budget.tokens_spent as f64,
            &tags,
        );
        self.metrics.gauge(
            "ace.budget.snapshot.walltime_ms",
            budget.walltime_ms_used as f64,
            &tags,
        );
        self.metrics.gauge(
            "ace.budget.snapshot.external_cost",
            budget.external_cost_spent as f64,
            &tags,
        );
        if let Some(reason) = &decision.degradation_reason {
            let labels = vec![("lane", format!("{:?}", lane)), ("reason", reason.clone())];
            self.metrics.counter("ace.budget.degrade", 1.0, &labels);
        }
        Ok(decision)
    }

    pub fn absorb_sync_point(
        &self,
        mut input: SyncPointInput,
    ) -> Result<AggregationOutcome, AceError> {
        if input.pending_injections.is_empty() {
            let topk = self.hitl.peek_clarify_topk(input.anchor.tenant_id, 3);
            if !topk.is_empty() {
                input.pending_injections = topk.into_iter().map(|entry| entry.injection).collect();
            }
        }
        let outcome = self.aggregator.aggregate(input)?;
        for decision in &outcome.injections {
            if matches!(
                decision.action,
                InjectionAction::Applied | InjectionAction::Ignored
            ) {
                self.hitl.resolve_injection(decision.injection_id);
            }
        }
        let labels = vec![
            ("kind", format!("{:?}", outcome.report.kind)),
            ("cycle_id", outcome.report.cycle_id.as_u64().to_string()),
        ];
        self.metrics.counter("ace.syncpoint.absorbed", 1.0, &labels);
        Ok(outcome)
    }

    pub fn finalize_cycle(&self, mut emission: CycleEmission) -> Result<(), AceError> {
        let tenant_id = emission.anchor.tenant_id.into_inner();
        emission.final_event = sanitize_final_event(&emission.lane, &emission.final_event);

        let cycle_key = emission.cycle_id.as_u64();
        let mut registry = self.finalized.lock().unwrap();
        if registry.contains(&cycle_key) {
            drop(registry);
            let late_event = soulseed_agi_core_models::awareness::AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: emission.final_event.base.event_id,
                event_type: AwarenessEventType::LateReceiptObserved,
                occurred_at_ms: OffsetDateTime::now_utc().unix_timestamp() * 1000,
                awareness_cycle_id: emission.cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: json!({
                    "late_event_id": emission.final_event.base.event_id.as_u64(),
                    "original_timestamp": emission.final_event.base.timestamp_ms,
                }),
            };
            let envelope = crate::types::OutboxEnvelope {
                tenant_id: emission.anchor.tenant_id,
                cycle_id: emission.cycle_id,
                messages: vec![crate::types::OutboxMessage {
                    cycle_id: emission.cycle_id,
                    event_id: late_event.event_id,
                    payload: late_event,
                }],
            };
            self.outbox.enqueue(envelope)?;
            self.metrics.counter("ace.cycle.late_receipt", 1.0, &[]);
            return Ok(());
        }

        registry.insert(cycle_key);
        drop(registry);

        let result = (|| -> Result<(), AceError> {
            let envelope = self.emitter.emit(emission.clone())?;
            let mut reservation = self.outbox.reserve(envelope);
            reservation.commit()?;
            self.checkpointer.finish(tenant_id);
            self.scheduler.finish(tenant_id);
            if matches!(emission.lane, CycleLane::Clarify) {
                self.hitl.clear_cycle_active(emission.anchor.tenant_id);
            }
            let labels = vec![("lane", format!("{:?}", emission.lane))];
            self.metrics.counter("ace.cycle.finalized", 1.0, &labels);
            Ok(())
        })();

        if let Err(err) = result {
            let mut registry = self.finalized.lock().unwrap();
            registry.remove(&cycle_key);
            return Err(err);
        }
        Ok(())
    }
}

fn sanitize_final_event(
    lane: &CycleLane,
    event: &soulseed_agi_core_models::DialogueEvent,
) -> soulseed_agi_core_models::DialogueEvent {
    let mut legacy: soulseed_agi_core_models::legacy::dialogue_event::DialogueEvent =
        event.clone().into();
    if matches!(lane, CycleLane::Collab | CycleLane::SelfReason) {
        legacy.reasoning_trace = None;
        legacy.reasoning_confidence = None;
        legacy.reasoning_strategy = None;
        legacy.metadata = merge_metadata(&legacy.metadata, lane);
        if legacy.evidence_pointer.is_none() {
            if let Some(digest) = legacy.content_digest_sha256.clone() {
                legacy.evidence_pointer = Some(EvidencePointer {
                    uri: format!("context://summary/{}", legacy.event_id.as_u64()),
                    digest_sha256: Some(digest.clone()),
                    media_type: Some("text/plain".into()),
                    blob_ref: None,
                    span: None,
                    access_policy: Some("summary_only".into()),
                });
            }
        }
    }
    let sanitized = soulseed_agi_core_models::convert_legacy_dialogue_event(legacy);
    validate_event(&sanitized);
    sanitized
}

fn merge_metadata(original: &Value, lane: &CycleLane) -> Value {
    let mut map = match original {
        Value::Object(obj) => obj.clone(),
        _ => Map::new(),
    };
    map.insert("summary_only".into(), Value::Bool(true));
    map.insert("lane".into(), Value::String(format!("{:?}", lane)));
    Value::Object(map)
}

#[cfg_attr(not(test), allow(dead_code))]
fn validate_event(event: &soulseed_agi_core_models::DialogueEvent) {
    if let Err(err) = soulseed_agi_core_models::validate_dialogue_event(event) {
        debug_assert!(false, "dialogue event validation failed: {:?}", err);
    }
}
