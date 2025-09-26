use once_cell::sync::Lazy;
use serde_json::json;
use soulseed_agi_ace::aggregator::SyncPointAggregator;
use soulseed_agi_ace::budget::{BudgetManager, BudgetPolicy};
use soulseed_agi_ace::ca::{
    CaService, InjectionAction, InjectionDecision, MergeDeltaRequest, MergeDeltaResponse,
};
use soulseed_agi_ace::checkpointer::Checkpointer;
use soulseed_agi_ace::emitter::Emitter;
use soulseed_agi_ace::engine::AceEngine;
use soulseed_agi_ace::outbox::OutboxService;
use soulseed_agi_ace::scheduler::{CycleScheduler, SchedulerConfig};
use soulseed_agi_ace::types::{
    BudgetSnapshot, CycleEmission, CycleLane, CycleRequest, SyncPointInput,
};
use soulseed_agi_core_models::awareness::{AwarenessAnchor, AwarenessEventType, SyncPointKind};
use soulseed_agi_core_models::{
    AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
    EnvelopeHead, EventId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

static ANCHOR: Lazy<AwarenessAnchor> = Lazy::new(|| AwarenessAnchor {
    tenant_id: TenantId(42),
    envelope_id: Uuid::now_v7(),
    config_snapshot_hash: "cfg-ace".into(),
    config_snapshot_version: 1,
    session_id: Some(soulseed_agi_core_models::SessionId(99)),
    sequence_number: Some(5),
    access_class: AccessClass::Restricted,
    provenance: Some(soulseed_agi_core_models::Provenance {
        source: "ace-test".into(),
        method: "contract".into(),
        model: Some("planner".into()),
        content_digest_sha256: Some("sha256:planner".into()),
    }),
    schema_v: 1,
});

fn dialogue_event() -> DialogueEvent {
    let now = OffsetDateTime::now_utc();
    DialogueEvent {
        tenant_id: ANCHOR.tenant_id,
        event_id: EventId(now.unix_timestamp_nanos() as u64),
        session_id: ANCHOR.session_id.unwrap(),
        subject: Subject::AI(soulseed_agi_core_models::AIId::new(7)),
        participants: vec![SubjectRef {
            kind: Subject::Human(soulseed_agi_core_models::HumanId(1)),
            role: Some("user".into()),
        }],
        head: EnvelopeHead {
            envelope_id: ANCHOR.envelope_id,
            trace_id: TraceId("trace".into()),
            correlation_id: CorrelationId("corr".into()),
            config_snapshot_hash: ANCHOR.config_snapshot_hash.clone(),
            config_snapshot_version: ANCHOR.config_snapshot_version,
        },
        snapshot: Snapshot {
            schema_v: ANCHOR.schema_v,
            created_at: now,
        },
        timestamp_ms: (now.unix_timestamp_nanos() / 1_000_000) as i64,
        scenario: ConversationScenario::HumanToAi,
        event_type: DialogueEventType::Message,
        time_window: None,
        access_class: ANCHOR.access_class,
        provenance: ANCHOR.provenance.clone(),
        sequence_number: ANCHOR.sequence_number.unwrap(),
        trigger_event_id: None,
        temporal_pattern_id: None,
        reasoning_trace: None,
        reasoning_confidence: None,
        reasoning_strategy: None,
        content_embedding: None,
        context_embedding: None,
        decision_embedding: None,
        embedding_meta: None,
        concept_vector: None,
        semantic_cluster_id: None,
        cluster_method: None,
        concept_distance_to_goal: None,
        real_time_priority: None,
        notification_targets: None,
        live_stream_id: None,
        evidence_pointer: None,
        content_digest_sha256: None,
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(soulseed_agi_core_models::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId(1),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: json!({"degradation_reason": "clarify_concurrency"}),
    }
}

#[derive(Clone)]
struct StubCa {
    response: MergeDeltaResponse,
}

impl CaService for StubCa {
    fn merge_delta(
        &self,
        _request: MergeDeltaRequest,
    ) -> Result<MergeDeltaResponse, soulseed_agi_ace::errors::AceError> {
        Ok(self.response.clone())
    }
}

fn budget(tokens: u32) -> BudgetSnapshot {
    BudgetSnapshot {
        tokens_allowed: 200,
        tokens_spent: tokens,
        walltime_ms_allowed: 5_000,
        walltime_ms_used: 800,
    }
}

#[test]
fn schedule_and_start_cycle() {
    let scheduler = CycleScheduler::new(SchedulerConfig {
        max_pending_per_tenant: 2,
        allow_parallel_lanes: false,
    });
    let request = CycleRequest {
        lane: CycleLane::Clarify,
        anchor: ANCHOR.clone(),
        tool_plan: None,
        llm_plan: None,
        budget: budget(10),
    };
    let outcome = scheduler
        .schedule(
            request.anchor.clone(),
            request.lane.clone(),
            request.tool_plan.clone(),
            request.llm_plan.clone(),
            request.budget.clone(),
        )
        .expect("schedule");
    assert!(outcome.accepted);
    let started = scheduler
        .start_next(request.anchor.tenant_id.into_inner())
        .expect("start next");
    assert_eq!(started.lane, CycleLane::Clarify);
    scheduler.finish(request.anchor.tenant_id.into_inner());
}

#[test]
fn clarify_gate_blocks_parallel() {
    let scheduler = CycleScheduler::new(SchedulerConfig {
        max_pending_per_tenant: 4,
        allow_parallel_lanes: false,
    });
    let request = CycleRequest {
        lane: CycleLane::Clarify,
        anchor: ANCHOR.clone(),
        tool_plan: None,
        llm_plan: None,
        budget: budget(10),
    };
    assert!(
        scheduler
            .schedule(
                request.anchor.clone(),
                request.lane.clone(),
                request.tool_plan.clone(),
                request.llm_plan.clone(),
                request.budget.clone(),
            )
            .unwrap()
            .accepted
    );
    let second = scheduler
        .schedule(
            request.anchor.clone(),
            request.lane.clone(),
            request.tool_plan.clone(),
            request.llm_plan.clone(),
            request.budget.clone(),
        )
        .unwrap();
    assert!(!second.accepted);
    assert_eq!(second.reason.as_deref(), Some("clarify_lane_busy"));
}

#[test]
fn aggregator_summarizes_events() {
    let aggregator = SyncPointAggregator::default();
    let events = vec![dialogue_event()];
    let outcome = aggregator
        .aggregate(SyncPointInput {
            cycle_id: soulseed_agi_core_models::CycleId(88),
            kind: SyncPointKind::ClarifyAnswered,
            anchor: ANCHOR.clone(),
            events,
            budget: budget(90),
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            pending_injections: Vec::new(),
            context_manifest: json!({"entries": []}),
        })
        .expect("aggregate");
    assert_eq!(outcome.report.kind, SyncPointKind::ClarifyAnswered);
    assert_eq!(outcome.report.metrics["events"], 1);
    assert_eq!(outcome.awareness_events.len(), 1);
}

#[test]
fn hitl_injection_contract_flow() {
    let delta_patch = soulseed_agi_core_models::awareness::DeltaPatch {
        patch_id: "patch-contract".into(),
        added: vec!["fact-a".into()],
        updated: vec!["fact-b".into()],
        removed: vec![],
        score_stats: std::collections::HashMap::new(),
        why_included: None,
        pointers: None,
        patch_digest: "digest-hitl".into(),
    };

    let ca: Arc<dyn CaService> = Arc::new(StubCa {
        response: MergeDeltaResponse {
            delta_patch: Some(delta_patch.clone()),
            awareness_events: Vec::new(),
            injections: vec![
                InjectionDecision {
                    injection_id: uuid::Uuid::now_v7(),
                    action: InjectionAction::Applied,
                    reason: Some("clarify_high_priority".into()),
                    fingerprint: Some("fp-hitl-1".into()),
                },
                InjectionDecision {
                    injection_id: uuid::Uuid::now_v7(),
                    action: InjectionAction::Deferred,
                    reason: Some("requires_follow_up".into()),
                    fingerprint: None,
                },
                InjectionDecision {
                    injection_id: uuid::Uuid::now_v7(),
                    action: InjectionAction::Ignored,
                    reason: Some("stale_fact".into()),
                    fingerprint: None,
                },
            ],
        },
    });

    let aggregator = SyncPointAggregator::new(ca);

    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(150),
        lane_walltime_ceiling_ms: Some(2_000),
    };
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let cycle = engine
        .schedule_cycle(CycleRequest {
            lane: CycleLane::Clarify,
            anchor: ANCHOR.clone(),
            tool_plan: None,
            llm_plan: None,
            budget: budget(40),
        })
        .unwrap()
        .cycle
        .expect("scheduled");

    let priority_injection = soulseed_agi_ace::hitl::HitlInjection::new(
        ANCHOR.tenant_id,
        soulseed_agi_ace::hitl::HitlPriority::P1High,
        "system",
        json!({"kind": "clarify_override"}),
    );

    let events = vec![{
        let mut event = dialogue_event();
        event.metadata = json!({
            "degradation_reason": "clarify_conflict",
            "clarify_conflict_id": "conf-7",
        });
        event
    }];

    let aggregation = engine
        .absorb_sync_point(SyncPointInput {
            cycle_id: cycle.cycle_id,
            kind: SyncPointKind::ClarifyAnswered,
            anchor: ANCHOR.clone(),
            events: events.clone(),
            budget: cycle.budget.clone(),
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            pending_injections: vec![priority_injection],
            context_manifest: json!({
                "entries": [
                    {"id": "fact-a"},
                    {"id": "fact-b"}
                ]
            }),
        })
        .expect("aggregate");

    assert!(aggregation.delta_patch.is_some());
    assert_eq!(
        aggregation.report.delta_patch_digest.as_deref(),
        Some("digest-hitl")
    );
    assert_eq!(aggregation.injections.len(), 3);
    assert_eq!(
        aggregation.report.degradation_reason.as_deref(),
        Some("clarify_conflict")
    );
    assert_eq!(aggregation.report.metrics["injections_applied"], 1);
    assert_eq!(aggregation.report.metrics["injections_deferred"], 1);
    assert_eq!(aggregation.report.metrics["injections_ignored"], 1);
    assert_eq!(
        aggregation.report.metrics["manifest_entries"].as_u64(),
        Some(2)
    );
    assert_eq!(aggregation.awareness_events.len(), 4);

    let applied = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::InjectionApplied)
        .expect("applied event");
    assert_eq!(applied.payload["reason"], "clarify_high_priority");
    assert_eq!(
        applied.degradation_reason,
        Some(soulseed_agi_core_models::awareness::AwarenessDegradationReason::ClarifyExhausted)
    );

    let deferred = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::InjectionDeferred)
        .expect("deferred event");
    assert_eq!(deferred.payload["reason"], "requires_follow_up");

    let ignored = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::InjectionIgnored)
        .expect("ignored event");
    assert_eq!(ignored.payload["reason"], "stale_fact");

    let summary_event = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::SyncPointReported)
        .expect("summary event");
    assert_eq!(
        summary_event.degradation_reason,
        Some(soulseed_agi_core_models::awareness::AwarenessDegradationReason::ClarifyExhausted)
    );
    assert_eq!(
        summary_event.payload["degradation_reason"],
        "clarify_conflict"
    );
}

#[test]
fn engine_pipeline_enqueues_outbox() {
    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(150),
        lane_walltime_ceiling_ms: Some(2_000),
    };
    let aggregator = SyncPointAggregator::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let schedule = engine
        .schedule_cycle(CycleRequest {
            lane: CycleLane::Clarify,
            anchor: ANCHOR.clone(),
            tool_plan: None,
            llm_plan: None,
            budget: budget(20),
        })
        .expect("schedule");
    assert!(schedule.accepted);
    let cycle = schedule.cycle.unwrap();

    let _budget_decision = engine
        .evaluate_budget(cycle.cycle_id, &cycle.lane, budget(120))
        .expect("budget");

    let events = vec![dialogue_event()];
    let aggregation = engine
        .absorb_sync_point(SyncPointInput {
            cycle_id: cycle.cycle_id,
            kind: SyncPointKind::ClarifyAnswered,
            anchor: ANCHOR.clone(),
            events: events.clone(),
            budget: cycle.budget.clone(),
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            pending_injections: Vec::new(),
            context_manifest: json!({"entries": []}),
        })
        .expect("aggregate");
    assert_eq!(aggregation.report.cycle_id, cycle.cycle_id);

    engine
        .finalize_cycle(CycleEmission {
            cycle_id: cycle.cycle_id,
            lane: cycle.lane,
            final_event: events[0].clone(),
            awareness_events: aggregation.awareness_events.clone(),
            budget: cycle.budget,
            anchor: ANCHOR.clone(),
        })
        .expect("finalize");

    let drained = outbox.dequeue(ANCHOR.tenant_id, 10);
    assert_eq!(drained.len(), 2);
}

#[test]
fn late_receipt_emits_event() {
    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(200),
        lane_walltime_ceiling_ms: Some(2_000),
    };
    let aggregator = SyncPointAggregator::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let cycle = engine
        .schedule_cycle(CycleRequest {
            lane: CycleLane::Clarify,
            anchor: ANCHOR.clone(),
            tool_plan: None,
            llm_plan: None,
            budget: budget(30),
        })
        .unwrap()
        .cycle
        .expect("scheduled");

    let events = vec![dialogue_event()];
    let aggregation = engine
        .absorb_sync_point(SyncPointInput {
            cycle_id: cycle.cycle_id,
            kind: SyncPointKind::ClarifyAnswered,
            anchor: ANCHOR.clone(),
            events: events.clone(),
            budget: cycle.budget.clone(),
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            pending_injections: Vec::new(),
            context_manifest: json!({"entries": []}),
        })
        .unwrap();

    let emission = CycleEmission {
        cycle_id: cycle.cycle_id,
        lane: cycle.lane,
        final_event: events[0].clone(),
        awareness_events: aggregation.awareness_events.clone(),
        budget: cycle.budget.clone(),
        anchor: ANCHOR.clone(),
    };

    engine.finalize_cycle(emission.clone()).unwrap();
    assert_eq!(outbox.dequeue(ANCHOR.tenant_id, 10).len(), 2);

    engine.finalize_cycle(emission).unwrap();
    let late = outbox.dequeue(ANCHOR.tenant_id, 10);
    assert_eq!(late.len(), 1);
    assert_eq!(
        late[0].payload.event_type,
        AwarenessEventType::LateReceiptObserved
    );
}
