use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration as StdDuration;

use dotenvy::from_path_iter;
use serde_json::{Map as JsonMap, Value as JsonValue, json};
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, DecisionPlan, SyncPointKind,
};
use soulseed_agi_core_models::dialogue_event::payload::{
    ClarificationPayload, CollaborationPayload, DialogueEventPayload, SelfReflectionPayload,
    ToolPayload,
};
use soulseed_agi_core_models::legacy::dialogue_event as legacy;
use soulseed_agi_core_models::legacy::dialogue_event::SelfReflectionRecord;
use soulseed_agi_core_models::{AIId, HumanId};
use soulseed_agi_core_models::{
    AccessClass, AwarenessCycleId, ConversationScenario, CorrelationId, DialogueEvent,
    DialogueEventBase, DialogueEventType, EnvelopeHead, EventId, Provenance, Snapshot, Subject,
    SubjectRef, TenantId, TraceId,
};
use time::OffsetDateTime;
use tracing::warn;
use uuid::Uuid;

use crate::engine::{AceEngine, AceOrchestrator, CycleOutcome, CycleRuntime};
use crate::errors::AceError;
use crate::llm::OpenAiClient;
use crate::persistence::AcePersistence;
use crate::tw_gateway::{CollabSynthesis, SoulbaseGateway, ToolSynthesis};
use crate::types::{
    BudgetSnapshot, CycleEmission, CycleLane, CycleRequest, CycleSchedule, CycleStatus,
    SyncPointInput,
};

use soulseed_agi_context::types::{
    Anchor as ContextAnchor, BudgetSummary, BundleItem, BundleScoreStats, BundleSegment,
    ContextBundle, ContextItemDigests, ExplainBundle, Partition, PromptBundle,
};
use soulseed_agi_dfr::types::{
    AssessmentSnapshot, BudgetTarget, CatalogSnapshot, ContextSignals as RouterContextSignals,
    PolicySnapshot, RouterConfig, RouterInput,
};

const WAIT_TIMEOUT_MS: u64 = 200;

#[derive(Default, Clone)]
pub(crate) struct CallbackRegistry {
    inner: Arc<(Mutex<HashMap<u64, VecDeque<SyncPointInput>>>, Condvar)>,
}

impl CallbackRegistry {
    fn enqueue(&self, input: SyncPointInput) {
        let key = input.cycle_id.as_u64();
        tracing::info!("CallbackRegistry::enqueue cycle_id={} (u64={})", input.cycle_id, key);
        let (lock, cvar) = &*self.inner;
        let mut guard = lock.lock().unwrap();
        guard.entry(key).or_default().push_back(input);
        cvar.notify_all();
    }

    fn dequeue_blocking(&self, cycle_id: AwarenessCycleId) -> Result<SyncPointInput, AceError> {
        let key = cycle_id.as_u64();
        tracing::info!("CallbackRegistry::dequeue_blocking cycle_id={} (u64={})", cycle_id, key);
        let (lock, cvar) = &*self.inner;
        let mut guard = lock.lock().unwrap();
        let mut iterations = 0;
        loop {
            if let Some(queue) = guard.get_mut(&key) {
                if let Some(next) = queue.pop_front() {
                    tracing::info!("CallbackRegistry::dequeue_blocking found data for cycle_id={}", cycle_id);
                    if queue.is_empty() {
                        guard.remove(&key);
                    }
                    return Ok(next);
                }
            }
            iterations += 1;
            if iterations % 50 == 0 {
                tracing::warn!("CallbackRegistry::dequeue_blocking still waiting for cycle_id={} after {} iterations", cycle_id, iterations);
            }
            let (tmp_guard, _) = cvar
                .wait_timeout(guard, StdDuration::from_millis(WAIT_TIMEOUT_MS))
                .unwrap();
            guard = tmp_guard;
        }
    }
}

#[derive(Clone)]
struct AutoSyncDriver {
    registry: CallbackRegistry,
    llm: Option<OpenAiClient>,
    gateway: Option<SoulbaseGateway>,
}

impl AutoSyncDriver {
    fn new(registry: CallbackRegistry) -> Self {
        let llm = match OpenAiClient::from_env() {
            Some(Ok(client)) => Some(client),
            Some(Err(err)) => {
                warn!("OpenAI 初始化失败，将使用合成 SyncPoint：{err}");
                None
            }
            None => None,
        };
        let gateway = match SoulbaseGateway::from_env() {
            Some(Ok(client)) => Some(client),
            Some(Err(err)) => {
                warn!("Soulbase Gateway 初始化失败，将回退合成 Tool/Collab：{err}");
                None
            }
            None => None,
        };
        Self {
            registry,
            llm,
            gateway,
        }
    }

    fn prepare(&self, schedule: &CycleSchedule) -> Result<SyncPointInput, AceError> {
        let input = self.build_sync_input(schedule)?;
        self.registry.enqueue(input.clone());
        Ok(input)
    }

    fn build_sync_input(&self, schedule: &CycleSchedule) -> Result<SyncPointInput, AceError> {
        let synthesis = self.synthesize(schedule);
        build_sync_input_with_synthesis(schedule, synthesis)
    }

    fn synthesize(&self, schedule: &CycleSchedule) -> LaneSynthesis {
        let mut synthesis = LaneSynthesis::default();

        match &schedule.router_decision.plan.decision_plan {
            DecisionPlan::Clarify { plan } => {
                if let Some(client) = &self.llm {
                    let context = schedule_context(schedule);
                    match client.clarify_answer(plan, &context) {
                        Ok(answer) => synthesis.clarify_answer = Some(answer),
                        Err(err) => warn!("OpenAI Clarify 调用失败，回退为合成答案：{err}"),
                    }
                }
            }
            DecisionPlan::SelfReason { plan } => {
                if let Some(client) = &self.llm {
                    let context = schedule_context(schedule);
                    match client.self_reflection(plan, &context) {
                        Ok(text) => synthesis.self_reflection = Some(text),
                        Err(err) => warn!("OpenAI SelfReason 调用失败，回退为合成答案：{err}"),
                    }
                }
            }
            DecisionPlan::Tool { plan } => {
                if let Some(gateway) = &self.gateway {
                    match gateway.execute_tool_plan(schedule, plan) {
                        Ok(result) => synthesis.tool = Some(result),
                        Err(err) => warn!("Soulbase 工具执行失败，回退为合成结果：{err}"),
                    }
                } else {
                    warn!("未配置 Soulbase Gateway，工具路径将使用合成结果");
                }
            }
            DecisionPlan::Collab { plan } => {
                if let Some(gateway) = &self.gateway {
                    match gateway.execute_collab_plan(schedule, plan) {
                        Ok(result) => synthesis.collab = Some(result),
                        Err(err) => warn!("Soulbase 协作执行失败，回退为合成结果：{err}"),
                    }
                } else {
                    warn!("未配置 Soulbase Gateway，协作路径将使用合成结果");
                }
            }
        }

        synthesis
    }
}

#[derive(Clone)]
pub struct InMemoryRuntime {
    registry: CallbackRegistry,
    history: Arc<Mutex<HashMap<u64, SyncPointInput>>>,
}

impl InMemoryRuntime {
    pub(crate) fn new(registry: CallbackRegistry) -> Self {
        Self {
            registry,
            history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn remember(&self, input: &SyncPointInput) {
        self.history
            .lock()
            .unwrap()
            .insert(input.cycle_id.as_u64(), input.clone());
    }

    fn recall(&self, schedule: &CycleSchedule) -> Option<SyncPointInput> {
        self.history
            .lock()
            .unwrap()
            .remove(&schedule.cycle_id.as_u64())
    }
}

impl CycleRuntime for InMemoryRuntime {
    fn prepare_sync_point(&mut self, schedule: &CycleSchedule) -> Result<SyncPointInput, AceError> {
        let mut input = self.registry.dequeue_blocking(schedule.cycle_id)?;
        if input.parent_cycle_id.is_none() {
            input.parent_cycle_id = schedule.parent_cycle_id;
        }
        if input.collab_scope_id.is_none() {
            input.collab_scope_id = schedule.collab_scope_id.clone();
        }
        self.remember(&input);
        Ok(input)
    }

    fn produce_final_event(
        &mut self,
        schedule: &CycleSchedule,
        _aggregation: &crate::types::AggregationOutcome,
    ) -> Result<(DialogueEvent, CycleStatus), AceError> {
        let input = self
            .recall(schedule)
            .ok_or_else(|| AceError::InvalidRequest("syncpoint_missing_for_cycle".into()))?;
        if let Some(last) = input.events.last() {
            return Ok((last.clone(), CycleStatus::Completed));
        }
        let fallback =
            fallback_dialogue_event(schedule.anchor.tenant_id, &schedule.anchor, &schedule.lane);
        Ok((fallback, CycleStatus::Completed))
    }
}

fn fallback_dialogue_event(
    tenant_id: TenantId,
    anchor: &AwarenessAnchor,
    lane: &CycleLane,
) -> DialogueEvent {
    let now = OffsetDateTime::now_utc();
    let legacy = legacy::DialogueEvent {
        tenant_id,
        event_id: EventId::generate(),
        session_id: anchor
            .session_id
            .unwrap_or_else(|| soulseed_agi_core_models::SessionId::from_raw_unchecked(1)),
        subject: Subject::AI(soulseed_agi_core_models::AIId::from_raw_unchecked(0)),
        participants: vec![SubjectRef {
            kind: Subject::Human(soulseed_agi_core_models::HumanId::from_raw_unchecked(0)),
            role: Some("user".into()),
        }],
        head: EnvelopeHead {
            envelope_id: anchor.envelope_id,
            trace_id: soulseed_agi_core_models::TraceId(Uuid::now_v7().to_string()),
            correlation_id: soulseed_agi_core_models::CorrelationId(Uuid::now_v7().to_string()),
            config_snapshot_hash: anchor.config_snapshot_hash.clone(),
            config_snapshot_version: anchor.config_snapshot_version,
        },
        snapshot: soulseed_agi_core_models::Snapshot {
            schema_v: anchor.schema_v,
            created_at: now,
        },
        timestamp_ms: now.unix_timestamp() * 1000,
        scenario: anchor
            .session_id
            .map(|_| ConversationScenario::HumanToAi)
            .unwrap_or(ConversationScenario::HumanToAi),
        event_type: DialogueEventType::Message,
        time_window: None,
        access_class: AccessClass::Internal,
        provenance: anchor.provenance.clone(),
        sequence_number: anchor.sequence_number.unwrap_or(1),
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: Vec::new(),
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
        message_ref: Some(legacy::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId::from_raw_unchecked(0),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: json!({
            "generated_from": "ace_runtime",
            "lane": format!("{:?}", lane),
        }),
    };
    soulseed_agi_core_models::convert_legacy_dialogue_event(legacy)
}

fn build_sync_input_with_synthesis(
    schedule: &CycleSchedule,
    synthesis: LaneSynthesis,
) -> Result<SyncPointInput, AceError> {
    let final_event = build_final_event(schedule, &synthesis)?;
    let now = OffsetDateTime::now_utc();
    let manifest_digest = format!(
        "auto:{}:{}",
        lane_label(&schedule.lane),
        final_event.base.event_id.as_u64()
    );

    let mut manifest = JsonMap::new();
    manifest.insert("manifest_digest".into(), json!(manifest_digest));
    manifest.insert("auto_generated".into(), json!(true));
    manifest.insert("prepared_at_ms".into(), json!(now.unix_timestamp() * 1000));
    manifest.insert(
        "router_digest".into(),
        json!(schedule.router_decision.plan.explain.router_digest.clone()),
    );
    manifest.insert(
        "context_digest".into(),
        json!(schedule.router_decision.context_digest.clone()),
    );
    manifest.insert("cycle_lane".into(), json!(lane_label(&schedule.lane)));
    manifest.insert("cycle_id".into(), json!(schedule.cycle_id.as_u64()));
    if let Some(answer) = synthesis.clarify_answer.as_ref() {
        manifest.insert("clarify_answer".into(), json!(answer));
    }
    if let Some(reflection) = synthesis.self_reflection.as_ref() {
        manifest.insert("self_reflection".into(), json!(reflection));
    }
    if let Some(tool) = synthesis.tool.as_ref() {
        for (key, value) in &tool.manifest {
            manifest.insert(key.clone(), value.clone());
        }
    }
    if let Some(collab) = synthesis.collab.as_ref() {
        for (key, value) in &collab.manifest {
            manifest.insert(key.clone(), value.clone());
        }
    }

    Ok(SyncPointInput {
        cycle_id: schedule.cycle_id,
        kind: lane_to_sync_kind(&schedule.lane),
        anchor: schedule.anchor.clone(),
        events: vec![final_event],
        budget: schedule.budget.clone(),
        timeframe: (schedule.created_at, now),
        pending_injections: Vec::new(),
        context_manifest: JsonValue::Object(manifest),
        parent_cycle_id: schedule.parent_cycle_id,
        collab_scope_id: schedule.collab_scope_id.clone(),
    })
}

fn build_final_event(
    schedule: &CycleSchedule,
    synthesis: &LaneSynthesis,
) -> Result<DialogueEvent, AceError> {
    let now = OffsetDateTime::now_utc();
    let anchor = &schedule.anchor;
    let session_id = anchor
        .session_id
        .unwrap_or_else(|| soulseed_agi_core_models::SessionId::from_raw_unchecked(1));
    let sequence = anchor.sequence_number.unwrap_or(0).saturating_add(1);
    let subject = Subject::AI(AIId::from_raw_unchecked(0));
    let participants = vec![SubjectRef {
        kind: Subject::Human(HumanId::from_raw_unchecked(1)),
        role: Some("user".into()),
    }];

    let mut base = DialogueEventBase {
        tenant_id: anchor.tenant_id,
        event_id: EventId::generate(),
        session_id,
        subject,
        participants,
        head: EnvelopeHead {
            envelope_id: anchor.envelope_id,
            trace_id: TraceId(Uuid::now_v7().to_string()),
            correlation_id: CorrelationId(Uuid::now_v7().to_string()),
            config_snapshot_hash: anchor.config_snapshot_hash.clone(),
            config_snapshot_version: anchor.config_snapshot_version,
        },
        snapshot: Snapshot {
            schema_v: anchor.schema_v,
            created_at: now,
        },
        timestamp_ms: now.unix_timestamp() * 1000,
        scenario: ConversationScenario::HumanToAi,
        event_type: DialogueEventType::Decision,
        stage_hint: Some(lane_label(&schedule.lane).into()),
        access_class: anchor.access_class,
        provenance: effective_provenance(anchor.access_class, anchor.provenance.clone()),
        sequence_number: sequence.max(1),
        ac_id: Some(schedule.cycle_id),
        ic_sequence: Some(
            schedule
                .router_decision
                .decision_path
                .inference_cycle_sequence,
        ),
        parent_ac_id: schedule.parent_cycle_id,
        config_version: None,
        trigger_event_id: None,
        supersedes: None,
        superseded_by: None,
    };

    let (event_type, payload, mut metadata) = payload_for_lane(
        &schedule.lane,
        &schedule.router_decision.plan.decision_plan,
        schedule,
        synthesis,
    )?;
    base.event_type = event_type;

    metadata.insert("auto_generated".into(), json!(true));
    metadata.insert("cycle_lane".into(), json!(lane_label(&schedule.lane)));
    metadata.insert("cycle_id".into(), json!(schedule.cycle_id.as_u64()));
    metadata.insert(
        "router_digest".into(),
        json!(schedule.router_decision.plan.explain.router_digest.clone()),
    );
    metadata.insert(
        "context_digest".into(),
        json!(schedule.router_decision.context_digest.clone()),
    );
    metadata.insert(
        "decision_issued_at_ms".into(),
        json!(schedule.router_decision.issued_at.unix_timestamp() * 1000),
    );

    if let Some(reason) = schedule.router_decision.decision_path.degradation_reason {
        metadata.insert(
            "degradation_reason".into(),
            json!(degradation_to_string(reason)),
        );
    }

    let mut event = DialogueEvent::new(base, payload);
    event.metadata = JsonValue::Object(metadata);
    Ok(event)
}

fn payload_for_lane(
    lane: &CycleLane,
    decision_plan: &DecisionPlan,
    schedule: &CycleSchedule,
    synthesis: &LaneSynthesis,
) -> Result<
    (
        DialogueEventType,
        DialogueEventPayload,
        JsonMap<String, JsonValue>,
    ),
    AceError,
> {
    let degradation = schedule.router_decision.decision_path.degradation_reason;

    match (lane, decision_plan) {
        (CycleLane::Clarify, DecisionPlan::Clarify { plan }) => {
            let clar_id = format!("clarify-{}", schedule.cycle_id.as_u64());
            let questions: Vec<String> = plan.questions.iter().map(|q| q.text.clone()).collect();
            let payload = DialogueEventPayload::ClarificationAnswered(ClarificationPayload {
                clarification_id: clar_id.clone(),
                question_ref: None,
                answer_ref: None,
                assigned_to: None,
                ttl_ms: plan.limits.wait_ms.or(plan.limits.total_wait_ms),
                merged_into: None,
                parent_id: None,
                degradation_reason: degradation,
                attributes: json!({
                    "questions": questions,
                    "answer": synthesis.clarify_answer.clone().unwrap_or_else(|| "（OpenAI 未返回答案或已回退为默认描述）".into()),
                    "limits": {
                        "max_parallel": plan.limits.max_parallel,
                        "max_rounds": plan.limits.max_rounds,
                        "wait_ms": plan.limits.wait_ms,
                        "total_wait_ms": plan.limits.total_wait_ms,
                    }
                }),
            });

            let mut meta = JsonMap::new();
            meta.insert("clarification_id".into(), json!(clar_id));
            meta.insert("question_count".into(), json!(plan.questions.len()));
            if let Some(answer) = synthesis.clarify_answer.as_ref() {
                meta.insert("answer".into(), json!(answer));
            }
            Ok((DialogueEventType::Decision, payload, meta))
        }
        (CycleLane::SelfReason, DecisionPlan::SelfReason { plan }) => {
            let topic = plan
                .hint
                .clone()
                .unwrap_or_else(|| "self_reasoning_follow_up".into());
            let record = SelfReflectionRecord {
                topic: topic.clone(),
                outcome: json!({
                    "max_inference_cycles": plan.max_ic,
                    "auto_generated": true,
                    "reflection": synthesis.self_reflection.clone().unwrap_or_else(|| "（OpenAI 未返回自省结果或已回退为默认描述）".into()),
                }),
                confidence: Some(schedule.router_decision.decision_path.confidence),
            };
            let payload = DialogueEventPayload::SelfReflectionLogged(SelfReflectionPayload {
                record,
                insight_digest: Some(format!("self-reflection:{}", schedule.cycle_id.as_u64())),
                related_event_id: None,
                attributes: JsonValue::Null,
            });

            let mut meta = JsonMap::new();
            meta.insert("self_reflection_topic".into(), json!(topic));
            if let Some(max_ic) = plan.max_ic {
                meta.insert("max_inference_cycles".into(), json!(max_ic));
            }
            if let Some(reflection) = synthesis.self_reflection.as_ref() {
                meta.insert("reflection".into(), json!(reflection));
            }
            Ok((DialogueEventType::SelfReflection, payload, meta))
        }
        (CycleLane::Tool, DecisionPlan::Tool { plan }) => {
            if let Some(tool) = synthesis.tool.as_ref() {
                let mut payload = tool.payload.clone();
                if payload.degradation_reason.is_none() {
                    payload.degradation_reason = degradation;
                }
                let mut meta = tool.metadata.clone();
                meta.entry("soulbase_gateway".to_string())
                    .or_insert_with(|| json!("tools.execute"));
                if !tool.awareness.is_empty() {
                    meta.insert("gateway_awareness".to_string(), json!(tool.awareness));
                }
                Ok((
                    DialogueEventType::ToolResult,
                    DialogueEventPayload::ToolResultRecorded(payload),
                    meta,
                ))
            } else {
                let payload = DialogueEventPayload::ToolResultRecorded(ToolPayload {
                    plan: Some(plan.clone()),
                    invocation: None,
                    result: None,
                    barrier_id: plan.barrier.mode.clone(),
                    route_id: Some(format!("tool-route-{}", schedule.cycle_id.as_u64())),
                    attempt: Some(1),
                    latency_ms: None,
                    evidence: Vec::new(),
                    output_digest_sha256: None,
                    blob_ref: None,
                    degradation_reason: degradation,
                    attributes: json!({
                        "node_count": plan.nodes.len(),
                        "edge_count": plan.edges.len(),
                        "fallback": true,
                    }),
                });

                let mut meta = JsonMap::new();
                meta.insert("tool_node_count".into(), json!(plan.nodes.len()));
                meta.insert("tool_edge_count".into(), json!(plan.edges.len()));
                if let Some(mode) = plan.barrier.mode.clone() {
                    meta.insert("barrier_mode".into(), json!(mode));
                }
                meta.insert("soulbase_gateway_fallback".to_string(), json!(true));
                Ok((DialogueEventType::ToolResult, payload, meta))
            }
        }
        (CycleLane::Collab, DecisionPlan::Collab { plan }) => {
            if let Some(collab) = synthesis.collab.as_ref() {
                let mut payload = collab.payload.clone();
                if payload.degradation_reason.is_none() {
                    payload.degradation_reason = degradation;
                }
                let mut meta = collab.metadata.clone();
                meta.entry("soulbase_gateway".to_string())
                    .or_insert_with(|| json!("collab.execute"));
                if !collab.awareness.is_empty() {
                    meta.insert("gateway_awareness".to_string(), json!(collab.awareness));
                }
                Ok((
                    DialogueEventType::Decision,
                    DialogueEventPayload::CollabResolved(payload),
                    meta,
                ))
            } else {
                let scope_id = schedule
                    .collab_scope_id
                    .clone()
                    .unwrap_or_else(|| format!("collab-{}", schedule.cycle_id.as_u64()));
                let payload = DialogueEventPayload::CollabResolved(CollaborationPayload {
                    scope_id: scope_id.clone(),
                    participants: Vec::new(),
                    barrier_id: plan.barrier.mode.clone(),
                    summary_ref: None,
                    assigned_to: None,
                    degradation_reason: degradation,
                    attributes: plan.scope.clone(),
                });

                let mut meta = JsonMap::new();
                meta.insert("collab_scope_id".into(), json!(scope_id));
                if let Some(order) = plan.order.clone() {
                    meta.insert("collab_order".into(), json!(order));
                }
                if let Some(rounds) = plan.rounds {
                    meta.insert("collab_rounds".into(), json!(rounds));
                }
                if let Some(mode) = plan.privacy_mode.clone() {
                    meta.insert("privacy_mode".into(), json!(mode));
                }
                meta.insert("soulbase_gateway_fallback".to_string(), json!(true));
                Ok((DialogueEventType::Decision, payload, meta))
            }
        }
        _ => Err(AceError::InvalidRequest("decision_plan_mismatch".into())),
    }
}

#[derive(Default, Clone)]
struct LaneSynthesis {
    clarify_answer: Option<String>,
    self_reflection: Option<String>,
    tool: Option<ToolSynthesis>,
    collab: Option<CollabSynthesis>,
}

fn schedule_context(schedule: &CycleSchedule) -> String {
    let fork = schedule.router_decision.plan.fork;
    format!(
        "cycle_id: {}\ntenant_id: {}\nfork: {:?}\nrouter_digest: {}",
        schedule.cycle_id.as_u64(),
        schedule.anchor.tenant_id.as_u64(),
        fork,
        schedule.router_decision.plan.explain.router_digest
    )
}

fn lane_to_sync_kind(lane: &CycleLane) -> SyncPointKind {
    match lane {
        CycleLane::Clarify => SyncPointKind::ClarifyAnswered,
        CycleLane::Tool => SyncPointKind::ToolBarrierReleased,
        CycleLane::SelfReason => SyncPointKind::IcEnd,
        CycleLane::Collab => SyncPointKind::CollabTurnEnd,
    }
}

fn lane_label(lane: &CycleLane) -> &'static str {
    match lane {
        CycleLane::Clarify => "clarify",
        CycleLane::Tool => "tool",
        CycleLane::SelfReason => "self_reason",
        CycleLane::Collab => "collab",
    }
}

fn effective_provenance(
    access_class: AccessClass,
    provenance: Option<Provenance>,
) -> Option<Provenance> {
    match provenance {
        Some(existing) => Some(existing),
        None if matches!(access_class, AccessClass::Restricted) => Some(Provenance {
            source: "ace.auto".into(),
            method: "auto_pipeline".into(),
            model: Some("synthetic".into()),
            content_digest_sha256: None,
        }),
        other => other,
    }
}

fn degradation_to_string(reason: AwarenessDegradationReason) -> &'static str {
    match reason {
        AwarenessDegradationReason::BudgetTokens => "budget_tokens",
        AwarenessDegradationReason::BudgetWalltime => "budget_walltime",
        AwarenessDegradationReason::BudgetExternalCost => "budget_external_cost",
        AwarenessDegradationReason::EmptyCatalog => "empty_catalog",
        AwarenessDegradationReason::PrivacyBlocked => "privacy_blocked",
        AwarenessDegradationReason::InvalidPlan => "invalid_plan",
        AwarenessDegradationReason::ClarifyExhausted => "clarify_exhausted",
        AwarenessDegradationReason::GraphDegraded => "graph_degraded",
        AwarenessDegradationReason::EnvctxDegraded => "envctx_degraded",
    }
}

struct NoopPersistence;

impl AcePersistence for NoopPersistence {
    fn persist_cycle(&self, _emission: &CycleEmission) -> Result<(), AceError> {
        Ok(())
    }

    fn persist_cycle_snapshot(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        _snapshot: &serde_json::Value,
    ) -> Result<(), AceError> {
        Ok(())
    }

    fn load_cycle_snapshot(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _cycle_id: soulseed_agi_core_models::AwarenessCycleId,
    ) -> Result<Option<serde_json::Value>, AceError> {
        Ok(None)
    }

    fn list_awareness_events(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _limit: usize,
    ) -> Result<Vec<soulseed_agi_core_models::awareness::AwarenessEvent>, AceError> {
        Ok(Vec::new())
    }
}

pub struct AceService<'a> {
    engine: AceEngine<'a>,
    runtime: InMemoryRuntime,
    registry: CallbackRegistry,
    auto_driver: AutoSyncDriver,
    _persistence: Arc<dyn AcePersistence>,
}

impl<'a> AceService<'a> {
    pub fn new(mut engine: AceEngine<'a>) -> Result<Self, AceError> {
        let registry = CallbackRegistry::default();
        let runtime = InMemoryRuntime::new(registry.clone());
        let auto_driver = AutoSyncDriver::new(registry.clone());
        let persistence = Self::init_persistence()?;
        engine.set_persistence(persistence.clone());
        Ok(Self {
            engine,
            runtime,
            registry,
            auto_driver,
            _persistence: persistence,
        })
    }

    fn orchestrator(&mut self) -> AceOrchestrator<'_, InMemoryRuntime> {
        AceOrchestrator::new(&self.engine, self.runtime.clone())
    }

    pub fn submit_trigger(
        &self,
        request: CycleRequest,
    ) -> Result<(CycleSchedule, SyncPointInput), AceError> {
        let outcome = self.engine.schedule_cycle(request)?;
        let schedule = match outcome.cycle {
            Some(cycle) => cycle,
            None => {
                let reason = outcome.reason.clone().unwrap_or_else(|| "unknown".into());
                tracing::warn!(
                    reason = %reason,
                    "cycle rejected by scheduler"
                );
                return Err(AceError::InvalidRequest(format!("cycle_rejected:{reason}")));
            }
        };
        let prepared = self.auto_driver.prepare(&schedule)?;
        Ok((schedule, prepared))
    }

    pub fn submit_callback(&self, input: SyncPointInput) {
        self.registry.enqueue(input);
    }

    pub fn reschedule_cycle(&self, schedule: CycleSchedule) {
        self.engine.scheduler.reschedule(schedule);
    }

    pub fn drive_once(&mut self, tenant: u64) -> Result<Option<CycleOutcome>, AceError> {
        let mut orchestrator = self.orchestrator();
        orchestrator.drive_once(tenant)
    }

    pub fn drive_until_idle(&mut self, tenant: u64) -> Result<Vec<CycleOutcome>, AceError> {
        let mut orchestrator = self.orchestrator();
        orchestrator.drive_until_idle(tenant)
    }

    #[cfg(feature = "persistence-surreal")]
    fn init_persistence() -> Result<Arc<dyn AcePersistence>, AceError> {
        use crate::persistence::surreal::{SurrealPersistence, SurrealPersistenceConfig};
        use sb_storage::surreal::config::{SurrealConfig, SurrealCredentials, SurrealProtocol};

        let settings = load_surreal_dotenv_settings()?;
        if lookup_setting(&settings, "ACE_PERSISTENCE_DISABLED")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
        {
            return Ok(Arc::new(NoopPersistence));
        }
        let namespace = lookup_setting(&settings, "ACE_SURREAL_NAMESPACE")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "soul".into());
        let database = lookup_setting(&settings, "ACE_SURREAL_DATABASE")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "ace".into());

        let mut config = SurrealConfig::default();
        config.namespace = namespace;
        config.database = database;

        let pool_size = lookup_setting(&settings, "ACE_SURREAL_POOL_MAX")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(8);
        config = config.with_pool(pool_size);

        if let Some(url) = lookup_setting(&settings, "ACE_SURREAL_URL")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
        {
            if url.starts_with("memory://") {
                config.endpoint = url;
                config.protocol = SurrealProtocol::Memory;
                config = config.strict_mode(false);
            } else if url.starts_with("http://") || url.starts_with("https://") {
                config.endpoint = url;
                config.protocol = SurrealProtocol::Http;
            } else if url.starts_with("ws://") || url.starts_with("wss://") {
                config.endpoint = url;
                config.protocol = SurrealProtocol::Ws;
            } else {
                // 默认使用 WebSocket 协议，允许传入简写/自定义协议头
                config.endpoint = url;
                config.protocol = SurrealProtocol::Ws;
            }
        }

        if let (Some(username), Some(password)) = (
            lookup_setting(&settings, "ACE_SURREAL_USERNAME")
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty()),
            lookup_setting(&settings, "ACE_SURREAL_PASSWORD")
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty()),
        ) {
            config = config.with_credentials(SurrealCredentials::new(username, password));
        }

        let persistence_config = SurrealPersistenceConfig { datastore: config };

        if let Ok(current) = tokio::runtime::Handle::try_current() {
            let handle = current.clone();
            let config_clone = persistence_config.clone();
            let persistence = tokio::task::block_in_place(|| {
                SurrealPersistence::new_with_handle(handle, config_clone)
            })?;
            Ok(Arc::new(persistence))
        } else {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|err| {
                    AceError::Persistence(format!("tokio runtime init failed: {err}"))
                })?;
            let persistence =
                SurrealPersistence::new_with_owned_runtime(runtime, persistence_config)?;
            Ok(Arc::new(persistence))
        }
    }

    #[cfg(not(feature = "persistence-surreal"))]
    fn init_persistence() -> Result<Arc<dyn AcePersistence>, AceError> {
        Err(AceError::Persistence(
            "persistence-surreal feature disabled".into(),
        ))
    }
}

pub struct TriggerComposer;

pub fn load_surreal_dotenv_settings() -> Result<HashMap<String, String>, AceError> {
    let origin = env::current_dir()
        .map_err(|err| AceError::Persistence(format!("无法获取当前工作目录：{err}")))?;
    let mut cursor = origin.as_path();
    loop {
        let candidate = cursor.join(".env");
        if candidate.exists() {
            let iter = from_path_iter(&candidate).map_err(|err| {
                AceError::Persistence(format!("读取配置文件 {} 失败：{err}", candidate.display()))
            })?;
            let mut map = HashMap::new();
            for entry in iter {
                let (key, value) = entry.map_err(|err| {
                    AceError::Persistence(format!(
                        "解析配置文件 {} 失败：{err}",
                        candidate.display()
                    ))
                })?;
                map.insert(key, value);
            }
            return Ok(map);
        }

        cursor = match cursor.parent() {
            Some(parent) => parent,
            None => break,
        };
    }

    Ok(HashMap::new())
}

fn lookup_setting(settings: &HashMap<String, String>, key: &str) -> Option<String> {
    settings.get(key).cloned().or_else(|| env::var(key).ok())
}

impl TriggerComposer {
    pub fn cycle_request_from_message(message: &DialogueEvent) -> CycleRequest {
        let awareness_anchor = build_awareness_anchor(message);
        let router_input = build_router_input(&awareness_anchor, message);
        CycleRequest {
            router_input,
            candidates: Vec::new(),
            budget: default_budget_snapshot(),
            parent_cycle_id: message.base.parent_ac_id,
            collab_scope_id: None,
        }
    }
}

fn build_awareness_anchor(message: &DialogueEvent) -> AwarenessAnchor {
    AwarenessAnchor {
        tenant_id: message.base.tenant_id,
        envelope_id: message.base.head.envelope_id,
        config_snapshot_hash: message.base.head.config_snapshot_hash.clone(),
        config_snapshot_version: message.base.head.config_snapshot_version,
        session_id: Some(message.base.session_id),
        sequence_number: Some(message.base.sequence_number),
        access_class: message.base.access_class,
        provenance: message.base.provenance.clone(),
        schema_v: message.base.snapshot.schema_v,
    }
}

fn build_router_input(anchor: &AwarenessAnchor, message: &DialogueEvent) -> RouterInput {
    let context_anchor = ContextAnchor {
        tenant_id: anchor.tenant_id,
        envelope_id: anchor.envelope_id,
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id,
        sequence_number: anchor.sequence_number,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        schema_v: anchor.schema_v,
        scenario: Some(message.base.scenario.clone()),
        supersedes: None,
        superseded_by: None,
    };

    let bundle = ContextBundle {
        anchor: context_anchor,
        schema_v: anchor.schema_v,
        version: 1,
        segments: vec![BundleSegment {
            partition: Partition::P4Dialogue,
            items: vec![bundle_item_from_message(message)],
        }],
        explain: ExplainBundle {
            reasons: vec!["message_trigger".into()],
            degradation_reason: None,
            indices_used: Vec::new(),
            query_hash: None,
        },
        budget: BudgetSummary {
            target_tokens: 2_048,
            projected_tokens: 0,
        },
        prompt: PromptBundle {
            dialogue: vec![format!("event:{}", message.base.event_id.as_u64())],
            ..PromptBundle::default()
        },
    };

    RouterInput {
        anchor: anchor.clone(),
        context: bundle,
        context_digest: format!("ctx:{}", message.base.event_id.as_u64()),
        scenario: message.base.scenario.clone(),
        scene_label: "auto.generated".into(),
        user_prompt: extract_prompt(message),
        tags: json!({}),
        assessment: AssessmentSnapshot::default(),
        context_signals: RouterContextSignals::default(),
        policies: PolicySnapshot::default(),
        catalogs: CatalogSnapshot::default(),
        budget: BudgetTarget {
            max_tokens: 4_096,
            max_walltime_ms: 60_000,
            max_external_cost: 25.0,
        },
        router_config: RouterConfig::default(),
        routing_seed: message.base.event_id.as_u64(),
    }
}

fn bundle_item_from_message(message: &DialogueEvent) -> BundleItem {
    BundleItem {
        ci_id: format!("msg:{}", message.base.event_id.as_u64()),
        partition: Partition::P4Dialogue,
        summary_level: None,
        tokens: 0,
        score_scaled: 0,
        ts_ms: message.base.timestamp_ms,
        digests: ContextItemDigests::default(),
        typ: Some("message".into()),
        why_included: Some("trigger_message".into()),
        score_stats: BundleScoreStats::default(),
        supersedes: None,
        evidence_ptrs: Vec::new(),
    }
}

fn extract_prompt(message: &DialogueEvent) -> String {
    use soulseed_agi_core_models::dialogue_event::DialogueEventPayload;
    match &message.payload {
        DialogueEventPayload::MessagePrimary(payload)
        | DialogueEventPayload::MessageEdited(payload)
        | DialogueEventPayload::MessageRegenerated(payload)
        | DialogueEventPayload::MessageSummaryPublished(payload)
        | DialogueEventPayload::MessageRecalled(payload)
        | DialogueEventPayload::MessagePinned(payload)
        | DialogueEventPayload::MessageFeedbackCaptured(payload)
        | DialogueEventPayload::MessageReactionAdded(payload)
        | DialogueEventPayload::MessageReactionRemoved(payload)
        | DialogueEventPayload::MessageThreadStarted(payload)
        | DialogueEventPayload::MessageThreadReplied(payload)
        | DialogueEventPayload::MessageAttachmentLinked(payload)
        | DialogueEventPayload::MessageAttachmentRemoved(payload)
        | DialogueEventPayload::MessageTranslated(payload)
        | DialogueEventPayload::MessageBroadcast(payload) => payload
            .channel
            .clone()
            .unwrap_or_else(|| format!("message:{}", message.base.event_id.as_u64())),
        _ => format!("event:{}", message.base.event_id.as_u64()),
    }
}

fn default_budget_snapshot() -> BudgetSnapshot {
    BudgetSnapshot {
        tokens_allowed: 6_000,
        tokens_spent: 0,
        walltime_ms_allowed: 120_000,
        walltime_ms_used: 0,
        external_cost_allowed: 50.0,
        external_cost_spent: 0.0,
    }
}
