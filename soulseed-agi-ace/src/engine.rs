use crate::aggregator::SyncPointAggregator;
use crate::budget::BudgetManager;
use crate::checkpointer::Checkpointer;
use crate::emitter::Emitter;
use crate::errors::AceError;
use crate::outbox::OutboxService;
use crate::scheduler::CycleScheduler;
use crate::types::{
    AggregationOutcome, BudgetSnapshot, CycleEmission, CycleRequest, ScheduleOutcome,
    SyncPointInput,
};
use serde_json::json;
use soulseed_agi_core_models::CycleId;
use soulseed_agi_core_models::awareness::AwarenessEventType;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

pub struct AceEngine<'a> {
    pub scheduler: &'a CycleScheduler,
    pub budget: &'a BudgetManager,
    pub aggregator: &'a SyncPointAggregator,
    pub checkpointer: &'a Checkpointer,
    pub outbox: &'a OutboxService,
    pub emitter: &'a Emitter,
    pub lane_cooldown: Duration,
    finalized: Arc<Mutex<HashSet<u64>>>,
}

impl<'a> AceEngine<'a> {
    pub fn new(
        scheduler: &'a CycleScheduler,
        budget: &'a BudgetManager,
        aggregator: &'a SyncPointAggregator,
        checkpointer: &'a Checkpointer,
        outbox: &'a OutboxService,
        emitter: &'a Emitter,
    ) -> Self {
        Self {
            scheduler,
            budget,
            aggregator,
            checkpointer,
            outbox,
            emitter,
            lane_cooldown: Duration::seconds(2),
            finalized: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn schedule_cycle(&self, request: CycleRequest) -> Result<ScheduleOutcome, AceError> {
        self.checkpointer.ensure_lane_idle(
            request.anchor.tenant_id.into_inner(),
            &request.lane,
            self.lane_cooldown,
        )?;

        let outcome = self.scheduler.schedule(
            request.anchor.clone(),
            request.lane.clone(),
            request.tool_plan.clone(),
            request.llm_plan.clone(),
            request.budget.clone(),
        )?;

        if let Some(cycle) = &outcome.cycle {
            self.checkpointer.record(crate::types::CheckpointState {
                tenant_id: cycle.anchor.tenant_id,
                cycle_id: cycle.cycle_id,
                lane: cycle.lane.clone(),
                budget: cycle.budget.clone(),
                since: cycle.created_at,
            });
        }
        Ok(outcome)
    }

    pub fn evaluate_budget(
        &self,
        cycle_id: CycleId,
        lane: &crate::types::CycleLane,
        budget: BudgetSnapshot,
    ) -> Result<crate::types::BudgetDecision, AceError> {
        self.budget.evaluate(cycle_id, lane, budget)
    }

    pub fn absorb_sync_point(&self, input: SyncPointInput) -> Result<AggregationOutcome, AceError> {
        let outcome = self.aggregator.aggregate(input)?;
        Ok(outcome)
    }

    pub fn finalize_cycle(&self, emission: CycleEmission) -> Result<(), AceError> {
        let tenant_id = emission.anchor.tenant_id.into_inner();
        let mut registry = self.finalized.lock().unwrap();
        if registry.insert(emission.cycle_id.0) {
            drop(registry);
            let envelope = self.emitter.emit(emission.clone())?;
            self.outbox.enqueue(envelope)?;
            self.checkpointer.finish(tenant_id);
            self.scheduler.finish(tenant_id);
        } else {
            drop(registry);
            let late_event = soulseed_agi_core_models::awareness::AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: emission.final_event.event_id,
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
                    "late_event_id": emission.final_event.event_id.0,
                    "original_timestamp": emission.final_event.timestamp_ms,
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
        }
        Ok(())
    }
}
