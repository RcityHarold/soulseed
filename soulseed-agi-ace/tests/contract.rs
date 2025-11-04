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
use soulseed_agi_ace::errors::AceError;
use soulseed_agi_ace::hitl::{HitlInjection, HitlPriority, HitlQueueConfig, HitlService};
use soulseed_agi_ace::metrics::NoopMetrics;
use soulseed_agi_ace::outbox::OutboxService;
use soulseed_agi_ace::runtime::{AceService, TriggerComposer};
use soulseed_agi_ace::scheduler::{CycleScheduler, SchedulerConfig};
use soulseed_agi_ace::types::{
    AggregationOutcome, BudgetSnapshot, CycleEmission, CycleLane, CycleRequest, CycleStatus,
    SyncPointInput, new_cycle_id,
};
use soulseed_agi_ace::{AceOrchestrator, CycleRuntime};
use soulseed_agi_context::types::{
    Anchor as ContextAnchor, BudgetSummary, BundleSegment, ContextBundle, ExplainBundle,
    Partition as ContextPartition, PromptBundle,
};
#[cfg(feature = "vectors-extra")]
use soulseed_agi_core_models::ExtraVectors;
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessEventType, ClarifyLimits, ClarifyPlan, CollabPlan,
    DecisionBudgetEstimate, DecisionExplain, DecisionPath, DecisionPlan, DecisionRationale,
    SelfPlan, SyncPointKind, ToolPlan, ToolPlanBarrier, ToolPlanNode,
};
use soulseed_agi_core_models::legacy::dialogue_event as legacy;
use soulseed_agi_core_models::{
    AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
    EnvelopeHead, EventId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
    convert_legacy_dialogue_event,
};
use soulseed_agi_dfr::types::{
    AssessmentSnapshot, BudgetEstimate, BudgetTarget, CatalogSnapshot, ContextSignals,
    PolicySnapshot, RouteExplain, RoutePlan, RouterCandidate, RouterConfig, RouterDecision,
    RouterInput,
};
use soulseed_agi_dfr::{
    RoutePlanner, RouterService, filter::CandidateFilter, hardgate::HardGate,
    scorer::CandidateScorer,
};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

static ANCHOR: Lazy<AwarenessAnchor> = Lazy::new(|| AwarenessAnchor {
    tenant_id: TenantId::from_raw_unchecked(42),
    envelope_id: Uuid::now_v7(),
    config_snapshot_hash: "cfg-ace".into(),
    config_snapshot_version: 1,
    session_id: Some(soulseed_agi_core_models::SessionId::from_raw_unchecked(99)),
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
    let legacy_event = legacy::DialogueEvent {
        tenant_id: ANCHOR.tenant_id,
        event_id: EventId::from_raw_unchecked(now.unix_timestamp_nanos() as u64),
        session_id: ANCHOR.session_id.unwrap(),
        subject: Subject::AI(soulseed_agi_core_models::AIId::new(7)),
        participants: vec![SubjectRef {
            kind: Subject::Human(soulseed_agi_core_models::HumanId::from_raw_unchecked(1)),
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
        causal_links: vec![],
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
        growth_stage: None,
        processing_latency_ms: None,
        influence_score: None,
        community_impact: None,
        evidence_pointer: None,
        content_digest_sha256: None,
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(soulseed_agi_core_models::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId::from_raw_unchecked(1),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: json!({"degradation_reason": "clarify_concurrency"}),
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    };
    convert_legacy_dialogue_event(legacy_event)
}

#[derive(Clone)]
struct RuntimeStub {
    sync_events: Vec<DialogueEvent>,
    manifest_digest: String,
    final_event: DialogueEvent,
    status: CycleStatus,
}

impl CycleRuntime for RuntimeStub {
    fn prepare_sync_point(
        &mut self,
        schedule: &soulseed_agi_ace::types::CycleSchedule,
    ) -> Result<SyncPointInput, AceError> {
        let now = OffsetDateTime::now_utc();
        let entries: Vec<_> = self
            .sync_events
            .iter()
            .enumerate()
            .map(|(idx, _)| json!({"id": format!("evt-{idx}")}))
            .collect();
        Ok(SyncPointInput {
            cycle_id: schedule.cycle_id,
            kind: SyncPointKind::ClarifyAnswered,
            anchor: schedule.anchor.clone(),
            events: self.sync_events.clone(),
            budget: schedule.budget.clone(),
            timeframe: (now - Duration::seconds(1), now),
            pending_injections: Vec::new(),
            context_manifest: json!({
                "entries": entries,
                "manifest_digest": self.manifest_digest,
            }),
            parent_cycle_id: schedule.parent_cycle_id,
            collab_scope_id: schedule.collab_scope_id.clone(),
        })
    }

    fn produce_final_event(
        &mut self,
        _schedule: &soulseed_agi_ace::types::CycleSchedule,
        _aggregation: &AggregationOutcome,
    ) -> Result<(DialogueEvent, CycleStatus), AceError> {
        Ok((self.final_event.clone(), self.status))
    }
}

#[derive(Clone)]
struct CaStub {
    manifest: serde_json::Value,
}

impl CaService for CaStub {
    fn merge_delta(&self, _request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError> {
        Ok(MergeDeltaResponse {
            delta_patch: None,
            awareness_events: Vec::new(),
            injections: Vec::new(),
            context_manifest: Some(self.manifest.clone()),
            context_bundle: None,
            prompt_bundle: None,
            explain_bundle: None,
        })
    }
}

#[derive(Clone)]
struct StubCa {
    response: MergeDeltaResponse,
}

impl CaService for StubCa {
    fn merge_delta(
        &self,
        request: MergeDeltaRequest,
    ) -> Result<MergeDeltaResponse, soulseed_agi_ace::errors::AceError> {
        let mut response = self.response.clone();
        if let (Some(first), Some(pending)) = (
            response.injections.first_mut(),
            request.pending_injections.first(),
        ) {
            first.injection_id = pending.injection_id;
        }
        Ok(response)
    }
}

fn budget(tokens: u32) -> BudgetSnapshot {
    BudgetSnapshot {
        tokens_allowed: 200,
        tokens_spent: tokens,
        walltime_ms_allowed: 5_000,
        walltime_ms_used: 800,
        external_cost_allowed: 4.0,
        external_cost_spent: 1.2,
    }
}

fn decision_plan_for_lane(lane: &CycleLane) -> DecisionPlan {
    match lane {
        CycleLane::Clarify => DecisionPlan::Clarify {
            plan: ClarifyPlan {
                questions: vec![],
                limits: ClarifyLimits::default(),
            },
        },
        CycleLane::SelfReason => DecisionPlan::SelfReason {
            plan: SelfPlan {
                hint: Some("consider context".into()),
                max_ic: Some(1),
            },
        },
        CycleLane::Tool => DecisionPlan::Tool {
            plan: ToolPlan {
                nodes: vec![ToolPlanNode {
                    id: "tool-node-1".into(),
                    tool_id: "demo.search".into(),
                    version: Some("v1".into()),
                    input: json!({"q": "demo"}),
                    timeout_ms: Some(1_500),
                    success_criteria: None,
                    evidence_policy: Some("pointer-only".into()),
                }],
                edges: vec![],
                barrier: ToolPlanBarrier::default(),
            },
        },
        CycleLane::Collab => DecisionPlan::Collab {
            plan: CollabPlan {
                scope: json!({"participants": ["assistant", "planner"]}),
                order: Some("round_robin".into()),
                rounds: Some(1),
                privacy_mode: Some("summary_only".into()),
                barrier: ToolPlanBarrier::default(),
            },
        },
    }
}

fn router_decision_stub(lane: CycleLane) -> RouterDecision {
    let plan_variant = decision_plan_for_lane(&lane);
    let cycle_id = new_cycle_id();
    let fork = lane.as_fork();
    let route_plan = RoutePlan {
        cycle_id,
        anchor: ANCHOR.clone(),
        fork,
        decision_plan: plan_variant.clone(),
        budget: BudgetEstimate {
            tokens: 300,
            walltime_ms: 2_000,
            external_cost: 1.0,
        },
        priority: 0.92,
        explain: RouteExplain {
            routing_seed: 0xACE0FACE,
            router_digest: "router:digest".into(),
            router_config_digest: "router:config".into(),
            indices_used: vec![],
            query_hash: None,
            degradation_reason: None,
            diagnostics: json!({}),
            rejected: vec![],
        },
    };

    let decision_path = DecisionPath {
        anchor: ANCHOR.clone(),
        awareness_cycle_id: cycle_id,
        inference_cycle_sequence: 1,
        fork,
        plan: plan_variant,
        budget_plan: DecisionBudgetEstimate::default(),
        rationale: DecisionRationale::default(),
        confidence: 0.88,
        explain: DecisionExplain {
            routing_seed: route_plan.explain.routing_seed,
            router_digest: route_plan.explain.router_digest.clone(),
            router_config_digest: route_plan.explain.router_config_digest.clone(),
            features_snapshot: None,
        },
        degradation_reason: None,
    };

    RouterDecision {
        plan: route_plan,
        decision_path,
        rejected: vec![],
        context_digest: "ctx:stub".into(),
        issued_at: OffsetDateTime::now_utc(),
    }
}

fn router_input_stub() -> RouterInput {
    let ctx_anchor = ContextAnchor {
        tenant_id: ANCHOR.tenant_id,
        envelope_id: ANCHOR.envelope_id,
        config_snapshot_hash: ANCHOR.config_snapshot_hash.clone(),
        config_snapshot_version: ANCHOR.config_snapshot_version,
        session_id: ANCHOR.session_id,
        sequence_number: ANCHOR.sequence_number,
        access_class: ANCHOR.access_class,
        provenance: ANCHOR.provenance.clone(),
        schema_v: ANCHOR.schema_v,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    };

    let context_bundle = ContextBundle {
        anchor: ctx_anchor,
        schema_v: ANCHOR.schema_v,
        version: 1,
        segments: vec![BundleSegment {
            partition: ContextPartition::P4Dialogue,
            items: vec![],
        }],
        explain: ExplainBundle {
            reasons: vec!["baseline".into()],
            degradation_reason: None,
            indices_used: vec![],
            query_hash: None,
        },
        budget: BudgetSummary {
            target_tokens: 512,
            projected_tokens: 128,
        },
        prompt: PromptBundle::default(),
    };

    let mut assessment = AssessmentSnapshot::default();
    assessment.intent_clarity = Some(0.9);

    let mut signals = ContextSignals::default();
    signals.primary_intent = Some("demo.intent".into());

    RouterInput {
        anchor: ANCHOR.clone(),
        context: context_bundle,
        context_digest: "ctx:stub".into(),
        scenario: ConversationScenario::HumanToAi,
        scene_label: "unit_test".into(),
        user_prompt: "hello".into(),
        tags: json!({}),
        assessment,
        context_signals: signals,
        policies: PolicySnapshot::default(),
        catalogs: CatalogSnapshot::default(),
        budget: BudgetTarget {
            max_tokens: 2_048,
            max_walltime_ms: 5_000,
            max_external_cost: 10.0,
        },
        router_config: RouterConfig::default(),
        routing_seed: 0xCAFE_BABE,
    }
}

fn cycle_request_for_lane(lane: CycleLane, tokens_spent: u32) -> CycleRequest {
    let router_input = router_input_stub();
    let candidate = RouterCandidate {
        decision_plan: decision_plan_for_lane(&lane),
        fork: lane.as_fork(),
        priority: 0.95,
        metadata: json!({}),
    };
    CycleRequest {
        router_input,
        candidates: vec![candidate],
        budget: budget(tokens_spent),
        parent_cycle_id: None,
        collab_scope_id: None,
    }
}

#[test]
fn schedule_and_start_cycle() {
    let scheduler = CycleScheduler::new(SchedulerConfig {
        max_pending_per_tenant: 2,
        allow_parallel_lanes: false,
        clarify_round_limit: 3,
        clarify_wait_limit_ms: 60_000,
        clarify_queue_threshold: 4,
    });
    let decision = router_decision_stub(CycleLane::Clarify);
    let tenant = decision.plan.anchor.tenant_id.into_inner();
    let outcome = scheduler
        .schedule(decision, budget(10), None, None)
        .expect("schedule");
    assert!(outcome.accepted);
    assert!(!outcome.awareness_events.is_empty());
    let started = scheduler.start_next(tenant).expect("start next");
    assert_eq!(started.lane, CycleLane::Clarify);
    assert_eq!(started.status, CycleStatus::AwaitingExternal);
    scheduler.finish(tenant, started.cycle_id, CycleStatus::Completed);
}

#[test]
fn clarify_gate_blocks_parallel() {
    let scheduler = CycleScheduler::new(SchedulerConfig {
        max_pending_per_tenant: 4,
        allow_parallel_lanes: false,
        clarify_round_limit: 3,
        clarify_wait_limit_ms: 60_000,
        clarify_queue_threshold: 4,
    });
    let first = router_decision_stub(CycleLane::Clarify);
    let tenant = first.plan.anchor.tenant_id.into_inner();
    assert!(
        scheduler
            .schedule(first, budget(12), None, None)
            .unwrap()
            .accepted
    );
    let second = router_decision_stub(CycleLane::Clarify);
    let rejected = scheduler.schedule(second, budget(15), None, None).unwrap();
    assert!(!rejected.accepted);
    assert_eq!(rejected.reason.as_deref(), Some("clarify_lane_busy"));
    let started = scheduler.start_next(tenant).expect("start first");
    scheduler.finish(tenant, started.cycle_id, CycleStatus::Completed);
}

#[test]
fn aggregator_summarizes_events() {
    let aggregator = SyncPointAggregator::default();
    let events = vec![dialogue_event()];
    let outcome = aggregator
        .aggregate(SyncPointInput {
            cycle_id: soulseed_agi_core_models::AwarenessCycleId::from_raw_unchecked(88),
            kind: SyncPointKind::ClarifyAnswered,
            anchor: ANCHOR.clone(),
            events,
            budget: budget(90),
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            pending_injections: Vec::new(),
            context_manifest: json!({"entries": []}),
            parent_cycle_id: None,
            collab_scope_id: None,
        })
        .expect("aggregate");
    assert_eq!(outcome.report.kind, SyncPointKind::ClarifyAnswered);
    assert_eq!(outcome.report.metrics["events"], 1);
    assert!(
        outcome
            .awareness_events
            .iter()
            .any(|event| event.event_type == AwarenessEventType::SyncPointReported)
    );
    assert!(
        outcome
            .awareness_events
            .iter()
            .any(|event| event.event_type == AwarenessEventType::ContextBuilt)
    );
    assert!(
        outcome
            .awareness_events
            .iter()
            .any(|event| event.event_type == AwarenessEventType::DeltaMerged)
    );
    assert!(outcome.delta_patch.is_some());
    assert!(outcome.context_manifest.is_some());
    assert!(outcome.prompt_bundle.is_some());
    assert!(outcome.explain_bundle.is_some());
    assert!(outcome.report.missing_sequences.is_empty());
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
            context_manifest: None,
            context_bundle: None,
            prompt_bundle: None,
            explain_bundle: None,
        },
    });

    let aggregator = SyncPointAggregator::new(ca);

    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();

    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(150),
        lane_walltime_ceiling_ms: Some(2_000),
        lane_external_cost_ceiling: Some(5.0),
    };
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let cycle = engine
        .schedule_cycle(cycle_request_for_lane(CycleLane::Clarify, 40))
        .unwrap()
        .cycle
        .expect("scheduled");

    let priority_injection = HitlInjection::new(
        ANCHOR.tenant_id,
        HitlPriority::P1High,
        "system",
        json!({"kind": "clarify_override"}),
    );
    hitl.enqueue(priority_injection.clone());

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
            pending_injections: vec![priority_injection.clone()],
            context_manifest: json!({
                "entries": [
                    {"id": "fact-a"},
                    {"id": "fact-b"}
                ]
            }),
            parent_cycle_id: None,
            collab_scope_id: None,
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
    assert_eq!(aggregation.report.metrics["merged_groups"], 1);
    assert_eq!(aggregation.report.metrics["failure_sources"], 1);
    assert_eq!(
        aggregation.report.metrics["manifest_entries"].as_u64(),
        Some(2)
    );
    assert_eq!(aggregation.report.applied_ids.len(), 1);
    assert_eq!(aggregation.report.ignored_ids.len(), 1);
    assert!(aggregation.report.missing_sequences.is_empty());
    assert_eq!(aggregation.report.merged.len(), 1);
    assert_eq!(aggregation.report.failures.len(), 1);
    assert!(!aggregation.report.merged[0].sources.is_empty());
    assert_eq!(
        aggregation.report.failures[0].degradation_reason.as_deref(),
        Some("clarify_conflict")
    );
    assert_eq!(aggregation.report.delta_added, vec![String::from("fact-a")]);
    assert_eq!(
        aggregation.report.delta_updated,
        vec![String::from("fact-b")]
    );
    assert!(aggregation.awareness_events.len() >= 4);
    assert!(
        aggregation
            .awareness_events
            .iter()
            .any(|event| event.event_type == AwarenessEventType::SyncPointMerged)
    );
    assert!(
        aggregation
            .awareness_events
            .iter()
            .any(|event| event.event_type == AwarenessEventType::DeltaPatchGenerated)
    );
    assert_eq!(hitl.pending_for(ANCHOR.tenant_id), 0);

    let applied = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::HumanInjectionApplied)
        .expect("applied event");
    assert_eq!(applied.payload["reason"], "clarify_high_priority");
    assert_eq!(
        applied.degradation_reason,
        Some(soulseed_agi_core_models::awareness::AwarenessDegradationReason::ClarifyExhausted)
    );

    let deferred = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::HumanInjectionDeferred)
        .expect("deferred event");
    assert_eq!(deferred.payload["reason"], "requires_follow_up");

    let ignored = aggregation
        .awareness_events
        .iter()
        .find(|event| event.event_type == AwarenessEventType::HumanInjectionIgnored)
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
        lane_external_cost_ceiling: Some(5.0),
    };
    let aggregator = SyncPointAggregator::default();
    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let schedule = engine
        .schedule_cycle(cycle_request_for_lane(CycleLane::Clarify, 20))
        .expect("schedule");
    assert!(schedule.accepted);
    assert!(!schedule.awareness_events.is_empty());
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
            parent_cycle_id: None,
            collab_scope_id: None,
        })
        .expect("aggregate");
    assert_eq!(aggregation.report.cycle_id, cycle.cycle_id);

    let mut awareness_events = cycle.decision_events.clone();
    awareness_events.extend(aggregation.awareness_events.clone());
    engine
        .finalize_cycle(CycleEmission {
            cycle_id: cycle.cycle_id,
            lane: cycle.lane,
            final_event: events[0].clone(),
            awareness_events,
            budget: cycle.budget,
            anchor: ANCHOR.clone(),
            explain_fingerprint: aggregation.explain_fingerprint.clone(),
            router_decision: cycle.router_decision.clone(),
            status: CycleStatus::Completed,
            parent_cycle_id: None,
            collab_scope_id: None,
            manifest_digest: aggregation
                .context_manifest
                .as_ref()
                .and_then(|val| val.get("manifest_digest"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            context_manifest: aggregation.context_manifest.clone(),
        })
        .expect("finalize");

    let drained = outbox.dequeue(ANCHOR.tenant_id, 20);
    assert!(
        drained
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::Finalized)
    );
    assert!(
        drained
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::SyncPointReported)
    );
}

#[test]
fn finalize_rolls_back_when_outbox_commit_fails() {
    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let budget_mgr = BudgetManager::default();
    let aggregator = SyncPointAggregator::default();
    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let schedule = engine
        .schedule_cycle(cycle_request_for_lane(CycleLane::Clarify, 18))
        .expect("schedule");
    let cycle = schedule.cycle.expect("cycle");

    let emission = CycleEmission {
        cycle_id: cycle.cycle_id,
        lane: cycle.lane.clone(),
        final_event: dialogue_event(),
        awareness_events: cycle.decision_events.clone(),
        budget: cycle.budget.clone(),
        anchor: cycle.anchor.clone(),
        explain_fingerprint: cycle.explain_fingerprint.clone(),
        router_decision: cycle.router_decision.clone(),
        status: CycleStatus::Completed,
        parent_cycle_id: None,
        collab_scope_id: None,
        manifest_digest: None,
        context_manifest: None,
    };

    outbox.fail_next_commit();
    let finalize_err = engine.finalize_cycle(emission.clone());
    assert!(matches!(finalize_err, Err(AceError::Outbox(_))));

    let tenant = emission.anchor.tenant_id.into_inner();
    assert!(
        checkpointer
            .fetch(tenant, emission.cycle_id.as_u64())
            .is_some()
    );
    assert!(outbox.dequeue(emission.anchor.tenant_id, 10).is_empty());

    // retry succeeds once commit failure is cleared
    engine
        .finalize_cycle(emission.clone())
        .expect("finalize retry");
    let drained = outbox.dequeue(emission.anchor.tenant_id, 10);
    assert!(!drained.is_empty());
    assert!(
        checkpointer
            .fetch(tenant, emission.cycle_id.as_u64())
            .is_none()
    );
}

#[test]
fn late_receipt_emits_event() {
    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(200),
        lane_walltime_ceiling_ms: Some(2_000),
        lane_external_cost_ceiling: Some(5.0),
    };
    let aggregator = SyncPointAggregator::default();
    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let schedule = engine
        .schedule_cycle(cycle_request_for_lane(CycleLane::Clarify, 30))
        .unwrap();
    assert!(schedule.accepted);
    assert!(!schedule.awareness_events.is_empty());
    let cycle = schedule.cycle.expect("scheduled");

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
            parent_cycle_id: None,
            collab_scope_id: None,
        })
        .unwrap();

    let mut awareness_events = cycle.decision_events.clone();
    awareness_events.extend(aggregation.awareness_events.clone());
    let emission = CycleEmission {
        cycle_id: cycle.cycle_id,
        lane: cycle.lane,
        final_event: events[0].clone(),
        awareness_events,
        budget: cycle.budget.clone(),
        anchor: ANCHOR.clone(),
        explain_fingerprint: aggregation.explain_fingerprint.clone(),
        router_decision: cycle.router_decision.clone(),
        status: CycleStatus::Completed,
        parent_cycle_id: None,
        collab_scope_id: None,
        manifest_digest: aggregation
            .context_manifest
            .as_ref()
            .and_then(|val| val.get("manifest_digest"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        context_manifest: aggregation.context_manifest.clone(),
    };

    engine.finalize_cycle(emission.clone()).unwrap();
    let finalized_msgs = outbox.dequeue(ANCHOR.tenant_id, 20);
    assert!(
        finalized_msgs
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::Finalized)
    );

    engine.finalize_cycle(emission).unwrap();
    let late = outbox.dequeue(ANCHOR.tenant_id, 10);
    assert_eq!(late.len(), 1);
    assert_eq!(
        late[0].payload.event_type,
        AwarenessEventType::LateReceiptObserved
    );
}

#[test]
fn orchestrator_emits_cycle_end_with_parent_and_scope() {
    let manifest_digest = "man-orch-001";
    let manifest = json!({
        "entries": [{"id": "orch"}],
        "manifest_digest": manifest_digest,
    });
    let aggregator = SyncPointAggregator::new(Arc::new(CaStub {
        manifest: manifest.clone(),
    }));
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(256),
        lane_walltime_ceiling_ms: Some(5_000),
        lane_external_cost_ceiling: Some(5.0),
    };
    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();
    let mut engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );
    engine.lane_cooldown = Duration::milliseconds(0);

    let parent_cycle_id = new_cycle_id();
    let collab_scope = "scope-orch".to_string();

    let mut request = cycle_request_for_lane(CycleLane::Clarify, 40);
    request.parent_cycle_id = Some(parent_cycle_id);
    request.collab_scope_id = Some(collab_scope.clone());

    let schedule = engine.schedule_cycle(request).expect("schedule");
    let cycle = schedule.cycle.expect("cycle");
    let tenant = cycle.anchor.tenant_id.into_inner();

    let runtime = RuntimeStub {
        sync_events: vec![dialogue_event()],
        manifest_digest: manifest_digest.to_string(),
        final_event: dialogue_event(),
        status: CycleStatus::Completed,
    };

    let mut orchestrator = AceOrchestrator::new(&engine, runtime);
    let outcome = orchestrator
        .drive_once(tenant)
        .expect("drive_once completed")
        .expect("cycle outcome");

    assert_eq!(outcome.cycle_id, cycle.cycle_id);
    assert_eq!(outcome.status, CycleStatus::Completed);
    assert_eq!(outcome.manifest_digest.as_deref(), Some(manifest_digest));

    let emitted = outbox.dequeue(ANCHOR.tenant_id, 20);
    assert!(
        emitted
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::AwarenessCycleEnded)
    );

    let finalized = emitted
        .iter()
        .find(|msg| msg.payload.event_type == AwarenessEventType::Finalized)
        .expect("finalized event");
    assert_eq!(finalized.payload.parent_cycle_id, Some(parent_cycle_id));
    assert_eq!(
        finalized.payload.collab_scope_id.as_deref(),
        Some(collab_scope.as_str())
    );
    assert_eq!(
        finalized
            .payload
            .payload
            .get("manifest_digest")
            .and_then(|v| v.as_str()),
        Some(manifest_digest)
    );

    let ended = emitted
        .iter()
        .find(|msg| msg.payload.event_type == AwarenessEventType::AwarenessCycleEnded)
        .expect("cycle ended event");
    assert_eq!(ended.payload.parent_cycle_id, Some(parent_cycle_id));
    assert_eq!(
        ended.payload.collab_scope_id.as_deref(),
        Some(collab_scope.as_str())
    );
    assert_eq!(
        ended
            .payload
            .payload
            .get("manifest_digest")
            .and_then(|v| v.as_str()),
        Some(manifest_digest)
    );
}

#[test]
fn ace_service_runs_end_to_end() {
    unsafe {
        std::env::set_var("ACE_SURREAL_URL", "memory://ace-test");
        std::env::set_var("ACE_PERSISTENCE_DISABLED", "1");
    }

    let scheduler = CycleScheduler::new(SchedulerConfig::default());
    let mut budget_mgr = BudgetManager::default();
    budget_mgr.clarify_policy = BudgetPolicy {
        lane_token_ceiling: Some(512),
        lane_walltime_ceiling_ms: Some(10_000),
        lane_external_cost_ceiling: Some(10.0),
    };
    let aggregator = SyncPointAggregator::default();
    let router_service = RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    );
    let route_planner = RoutePlanner::default();
    let checkpointer = Checkpointer::default();
    let outbox = OutboxService::default();
    let emitter = Emitter;
    let hitl = HitlService::new(HitlQueueConfig::default());
    let metrics = NoopMetrics::default();

    let engine = AceEngine::new(
        &scheduler,
        &budget_mgr,
        &aggregator,
        &checkpointer,
        &outbox,
        &emitter,
        &hitl,
        &metrics,
        &router_service,
        &route_planner,
    );

    let mut service = AceService::new(engine).expect("ace service");

    let trigger_event = dialogue_event();
    let cycle_request = TriggerComposer::cycle_request_from_message(&trigger_event);
    let (schedule, _) = service
        .submit_trigger(cycle_request)
        .expect("cycle scheduled");

    let outcomes = service
        .drive_until_idle(schedule.anchor.tenant_id.into_inner())
        .expect("drive");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].cycle_id, schedule.cycle_id);
    assert_eq!(outcomes[0].status, CycleStatus::Completed);

    let drained = outbox.dequeue(schedule.anchor.tenant_id, 20);
    assert!(
        drained
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::Finalized)
    );
    assert!(
        drained
            .iter()
            .any(|msg| msg.payload.event_type == AwarenessEventType::AwarenessCycleEnded)
    );
}
