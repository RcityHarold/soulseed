use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use soulseed_agi_context::{
    ContextConfig,
    assembly::ContextManifest,
    thinwaist::{ContextRuntimeInput, LocalContextRuntime},
    types::{
        Anchor as ContextAnchor, ContextItem, ContextItemDigests, ContextItemLinks, FeatureVec,
        Partition,
    },
};
use soulseed_agi_core_models::{
    AccessClass, AwarenessCycleId, ConversationScenario, DialogueEvent, DialogueEventType, EventId,
    awareness::{
        AwarenessAnchor, AwarenessDegradationReason, AwarenessEvent, AwarenessEventType,
        DeltaPatch, SyncPointKind,
    },
};
use std::sync::Mutex;
use time::{Duration, OffsetDateTime};

use crate::errors::AceError;
use crate::hitl::{HitlInjection, HitlPriority};
use crate::types::{BudgetSnapshot, SyncPointInput};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectionAction {
    Applied,
    Deferred,
    Ignored,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InjectionDecision {
    pub injection_id: uuid::Uuid,
    pub action: InjectionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeDeltaRequest {
    pub cycle_id: AwarenessCycleId,
    pub anchor: AwarenessAnchor,
    pub kind: SyncPointKind,
    pub timeframe: (OffsetDateTime, OffsetDateTime),
    pub events: Vec<soulseed_agi_core_models::DialogueEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_injections: Vec<HitlInjection>,
    pub budget_snapshot: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub context_manifest: Value,
}

impl From<&SyncPointInput> for MergeDeltaRequest {
    fn from(input: &SyncPointInput) -> Self {
        Self {
            cycle_id: input.cycle_id,
            anchor: input.anchor.clone(),
            kind: input.kind,
            timeframe: input.timeframe,
            events: input.events.clone(),
            pending_injections: input.pending_injections.clone(),
            budget_snapshot: input.budget.clone(),
            context_manifest: input.context_manifest.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeDeltaResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_patch: Option<DeltaPatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub awareness_events: Vec<AwarenessEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<InjectionDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_manifest: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_bundle: Option<soulseed_agi_context::types::ContextBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_bundle: Option<soulseed_agi_context::types::PromptBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_bundle: Option<soulseed_agi_context::types::ExplainBundle>,
}

pub trait CaService: Send + Sync {
    fn merge_delta(&self, request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError>;
}

pub struct CaServiceDefault {
    runtime: Mutex<LocalContextRuntime>,
}

impl Default for CaServiceDefault {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(LocalContextRuntime::default()),
        }
    }
}

impl CaService for CaServiceDefault {
    fn merge_delta(&self, request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError> {
        let MergeDeltaRequest {
            cycle_id,
            anchor,
            kind,
            timeframe,
            events,
            pending_injections,
            budget_snapshot,
            context_manifest,
        } = request;

        if events.is_empty() {
            return Err(AceError::InvalidRequest(
                "sync point requires events".into(),
            ));
        }

        let scenario = events.first().map(|event| event.base.scenario.clone());
        let context_anchor = to_context_anchor(&anchor, scenario);

        let mut config = ContextConfig::default();
        config.snapshot_hash = anchor.config_snapshot_hash.clone();
        config.snapshot_version = anchor.config_snapshot_version;
        config.target_tokens = budget_snapshot
            .tokens_allowed
            .max(256)
            .max(budget_snapshot.tokens_spent);
        config.plan_seed = cycle_id.as_u64() as u64;
        config.scoring_reference = timeframe.1;

        let mut items: Vec<ContextItem> = Vec::new();
        for event in &events {
            items.push(context_item_from_event(event, &context_anchor, kind));
        }
        for injection in &pending_injections {
            items.push(context_item_from_injection(injection, &context_anchor));
        }

        let previous_manifest = parse_manifest(&context_manifest);

        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AceError::Ca("ca_runtime_poisoned".into()))?;

        let output = runtime
            .run(ContextRuntimeInput {
                anchor: context_anchor.clone(),
                config,
                items,
                graph_explain: None,
                previous_manifest,
            })
            .map_err(|err| AceError::Ca(format!("context_engine:{err}")))?;

        let delta_patch = output.delta_patch.clone();
        let prompt_bundle = output.bundle.prompt.clone();
        let explain_bundle = output.bundle.explain.clone();
        let context_manifest_value = serde_json::to_value(&output.manifest)
            .map_err(|err| AceError::Ca(format!("manifest_serialize:{err}")))?;

        let awareness_events =
            build_awareness_events(&anchor, cycle_id, kind, timeframe, &output, &delta_patch);

        let injections = build_injection_decisions(pending_injections);

        Ok(MergeDeltaResponse {
            delta_patch: Some(delta_patch),
            awareness_events,
            injections,
            context_manifest: Some(context_manifest_value),
            context_bundle: Some(output.bundle.clone()),
            prompt_bundle: Some(prompt_bundle),
            explain_bundle: Some(explain_bundle),
        })
    }
}

fn parse_manifest(value: &Value) -> Option<ContextManifest> {
    if value.is_null() {
        None
    } else {
        serde_json::from_value(value.clone()).ok()
    }
}

fn to_context_anchor(
    anchor: &AwarenessAnchor,
    scenario: Option<ConversationScenario>,
) -> ContextAnchor {
    ContextAnchor {
        tenant_id: anchor.tenant_id,
        envelope_id: anchor.envelope_id,
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id,
        sequence_number: anchor.sequence_number,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        schema_v: anchor.schema_v,
        scenario,
        supersedes: None,
        superseded_by: None,
    }
}

fn to_offset_date_time(ts_ms: i64) -> OffsetDateTime {
    let seconds = ts_ms.div_euclid(1_000);
    let millis = ts_ms.rem_euclid(1_000);
    let base = OffsetDateTime::from_unix_timestamp(seconds).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    base + Duration::milliseconds(millis)
}

fn visibility_tag(access_class: AccessClass) -> &'static str {
    match access_class {
        AccessClass::Restricted => "minimal_sharing",
        AccessClass::Internal => "task_context",
        _ => "full_group",
    }
}

fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    ((chars / 4).max(1)).min(8192)
}

fn estimate_tokens_from_value(value: &Value) -> u32 {
    serde_json::to_string(value)
        .map(|s| estimate_tokens(&s))
        .unwrap_or(16)
}

fn hash_json(value: &Value) -> String {
    let payload = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("sha256-{:x}", hasher.finalize())
}

fn partition_for_event(event_type: DialogueEventType) -> Partition {
    match event_type {
        DialogueEventType::Message => Partition::P4Dialogue,
        DialogueEventType::ToolCall
        | DialogueEventType::ToolResult
        | DialogueEventType::SelfReflection => Partition::P3WorkingDelta,
        DialogueEventType::Decision => Partition::P1TaskFacts,
        DialogueEventType::Lifecycle | DialogueEventType::System => Partition::P0Policy,
    }
}

fn base_features_for_event(event: &DialogueEvent, tokens: u32) -> FeatureVec {
    FeatureVec {
        rel: match event.base.event_type {
            DialogueEventType::ToolResult => 0.88,
            DialogueEventType::ToolCall | DialogueEventType::SelfReflection => 0.74,
            DialogueEventType::Decision => 0.76,
            DialogueEventType::Lifecycle | DialogueEventType::System => 0.65,
            DialogueEventType::Message => 0.72,
        },
        cau: match event.base.event_type {
            DialogueEventType::Decision => 0.7,
            DialogueEventType::ToolResult => 0.68,
            DialogueEventType::Message => 0.52,
            _ => 0.6,
        },
        rec: 0.58,
        auth: if matches!(event.base.access_class, AccessClass::Public) {
            0.45
        } else {
            0.62
        },
        stab: 0.55,
        dup: 0.08,
        len: (tokens as f32 / 2048.0).min(1.0),
        risk: match event.base.access_class {
            AccessClass::Restricted => 0.82,
            AccessClass::Internal => 0.32,
            _ => 0.18,
        },
    }
    .clamp()
}

fn base_features_for_injection(priority: HitlPriority, tokens: u32) -> FeatureVec {
    let (rel, cau) = match priority {
        HitlPriority::P0Critical => (0.95, 0.85),
        HitlPriority::P1High => (0.88, 0.78),
        HitlPriority::P2Medium => (0.74, 0.62),
        HitlPriority::P3Low => (0.6, 0.5),
    };
    FeatureVec {
        rel,
        cau,
        rec: 0.55,
        auth: 0.6,
        stab: 0.48,
        dup: 0.05,
        len: (tokens as f32 / 1024.0).min(1.0),
        risk: if matches!(priority, HitlPriority::P0Critical | HitlPriority::P1High) {
            0.35
        } else {
            0.25
        },
    }
    .clamp()
}

fn context_item_from_event(
    event: &DialogueEvent,
    anchor: &ContextAnchor,
    kind: SyncPointKind,
) -> ContextItem {
    let content = serde_json::to_value(event).unwrap_or(Value::Null);
    let tokens = estimate_tokens_from_value(&content);
    let features = base_features_for_event(event, tokens);
    let partition = partition_for_event(event.base.event_type.clone());
    let mut digests = ContextItemDigests::default();
    digests.content = event
        .payload
        .content_digest_sha256()
        .cloned()
        .map(|digest| format!("sha256-{}", digest))
        .or_else(|| Some(hash_json(&content)));
    let mut links = ContextItemLinks::default();
    if let Some(pointer) = event.payload.evidence_pointer() {
        links.evidence_ptrs.push(pointer.clone());
        if let Some(digest) = pointer.digest_sha256.clone() {
            digests.evidence = Some(digest);
        }
    }
    links.supersedes = event.base.supersedes.map(|id| id.as_u64().to_string());

    ContextItem {
        anchor: anchor.clone(),
        id: format!("evt:{}", event.base.event_id.as_u64()),
        partition,
        partition_hint: Some(partition),
        source_event_id: event.base.event_id,
        source_message_id: event.payload.message_ref().map(|ptr| ptr.message_id),
        observed_at: to_offset_date_time(event.base.timestamp_ms),
        content,
        tokens,
        features,
        policy_tags: json!({
            "visibility": visibility_tag(event.base.access_class),
            "syncpoint": format!("{:?}", kind).to_lowercase(),
        }),
        typ: Some(format!("dialogue::{:?}", event.base.event_type)),
        digests,
        links,
    }
}

fn context_item_from_injection(injection: &HitlInjection, anchor: &ContextAnchor) -> ContextItem {
    let content = injection.payload.clone();
    let tokens = estimate_tokens_from_value(&content);
    let features = base_features_for_injection(injection.priority, tokens);
    let mut digests = ContextItemDigests::default();
    digests.content = Some(hash_json(&content));

    ContextItem {
        anchor: anchor.clone(),
        id: format!("inj:{}", injection.injection_id),
        partition: Partition::P3WorkingDelta,
        partition_hint: Some(Partition::P3WorkingDelta),
        source_event_id: EventId::from_raw_unchecked(injection.injection_id.as_u128() as u64),
        source_message_id: None,
        observed_at: injection.submitted_at,
        content,
        tokens,
        features,
        policy_tags: json!({
            "visibility": "task_context",
            "author_role": injection.author_role,
        }),
        typ: Some("hitl_injection".into()),
        digests,
        links: ContextItemLinks::default(),
    }
}

fn build_injection_decisions(pending: Vec<HitlInjection>) -> Vec<InjectionDecision> {
    pending
        .into_iter()
        .enumerate()
        .map(|(idx, inj)| {
            let action = match inj.priority {
                HitlPriority::P0Critical => InjectionAction::Applied,
                HitlPriority::P1High if idx == 0 => InjectionAction::Applied,
                HitlPriority::P1High => InjectionAction::Deferred,
                HitlPriority::P2Medium => InjectionAction::Deferred,
                HitlPriority::P3Low => InjectionAction::Ignored,
            };
            let reason = match action {
                InjectionAction::Applied => "hitl_applied",
                InjectionAction::Deferred => "hitl_deferred",
                InjectionAction::Ignored => "hitl_ignored",
            }
            .to_string();
            InjectionDecision {
                injection_id: inj.injection_id,
                action,
                reason: Some(reason),
                fingerprint: Some(format!("fp:{}", inj.injection_id)),
            }
        })
        .collect()
}

fn map_degradation_reason(reason: Option<&str>) -> Option<AwarenessDegradationReason> {
    let reason = reason?;
    for token in reason.split(|c| c == ';' || c == '|') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if token.contains("budget_tokens") {
            return Some(AwarenessDegradationReason::BudgetTokens);
        }
        if token.contains("budget_walltime") {
            return Some(AwarenessDegradationReason::BudgetWalltime);
        }
        if token.contains("budget_external_cost") {
            return Some(AwarenessDegradationReason::BudgetExternalCost);
        }
        if token.contains("privacy") {
            return Some(AwarenessDegradationReason::PrivacyBlocked);
        }
        if token.contains("graph") {
            return Some(AwarenessDegradationReason::GraphDegraded);
        }
        if token.contains("envctx") {
            return Some(AwarenessDegradationReason::EnvctxDegraded);
        }
        if token.contains("clarify") {
            return Some(AwarenessDegradationReason::ClarifyExhausted);
        }
    }
    None
}

fn build_awareness_event(
    anchor: &AwarenessAnchor,
    cycle_id: AwarenessCycleId,
    event_id: u64,
    event_type: AwarenessEventType,
    occurred_at_ms: i64,
    degradation: Option<AwarenessDegradationReason>,
    payload: Value,
) -> AwarenessEvent {
    AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(event_id),
        event_type,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: degradation,
        payload,
    }
}

fn build_awareness_events(
    anchor: &AwarenessAnchor,
    cycle_id: AwarenessCycleId,
    kind: SyncPointKind,
    timeframe: (OffsetDateTime, OffsetDateTime),
    output: &soulseed_agi_context::types::RunOutput,
    delta_patch: &DeltaPatch,
) -> Vec<AwarenessEvent> {
    let degrade = output
        .bundle
        .explain
        .degradation_reason
        .clone()
        .or(output.report.degradation_reason.clone());
    let degradation_reason = map_degradation_reason(degrade.as_deref());
    let occurred_at_ms = timeframe.1.unix_timestamp() * 1000;
    let mut events = Vec::new();
    let mut next_event_id = cycle_id.as_u64() + 1;

    let summary_payload = json!({
        "manifest_digest": output.manifest.manifest_digest,
        "bundle_version": output.bundle.version,
        "tokens": output.plan.budget.projected_tokens,
        "patch_digest": delta_patch.patch_digest,
    });
    events.push(build_awareness_event(
        anchor,
        cycle_id,
        next_event_id,
        map_syncpoint_event(&kind),
        occurred_at_ms,
        degradation_reason,
        summary_payload,
    ));
    next_event_id += 1;

    let snapshot_payload = serde_json::to_value(&output.env_snapshot).unwrap_or_else(|_| json!({}));
    events.push(build_awareness_event(
        anchor,
        cycle_id,
        next_event_id,
        AwarenessEventType::EnvironmentSnapshotRecorded,
        occurred_at_ms,
        degradation_reason,
        snapshot_payload,
    ));
    next_event_id += 1;

    let context_payload = json!({
        "plan_id": output.plan.plan_id,
        "manifest_digest": output.manifest.manifest_digest,
        "target_tokens": output.plan.budget.target_tokens,
        "projected_tokens": output.plan.budget.projected_tokens,
        "tokens_saved": output.report.tokens_saved,
        "degradation_reason": output.report.degradation_reason,
    });
    events.push(build_awareness_event(
        anchor,
        cycle_id,
        next_event_id,
        AwarenessEventType::ContextBuilt,
        occurred_at_ms,
        degradation_reason,
        context_payload,
    ));
    next_event_id += 1;

    events.push(build_awareness_event(
        anchor,
        cycle_id,
        next_event_id,
        AwarenessEventType::DeltaMerged,
        occurred_at_ms,
        degradation_reason,
        json!({
            "patch_id": delta_patch.patch_id,
            "manifest_digest": output.manifest.manifest_digest,
            "added": delta_patch.added,
            "updated": delta_patch.updated,
            "removed": delta_patch.removed,
            "score_stats": delta_patch.score_stats,
        }),
    ));

    events
}

fn map_syncpoint_event(kind: &SyncPointKind) -> AwarenessEventType {
    match kind {
        SyncPointKind::IcEnd => AwarenessEventType::InferenceCycleCompleted,
        SyncPointKind::ToolBarrier | SyncPointKind::ToolBarrierReached => {
            AwarenessEventType::ToolBarrierReached
        }
        SyncPointKind::ToolBarrierReleased => AwarenessEventType::ToolBarrierReleased,
        SyncPointKind::ToolBarrierTimeout => AwarenessEventType::ToolBarrierTimeout,
        SyncPointKind::ToolChainNext => AwarenessEventType::RouteSwitched,
        SyncPointKind::CollabTurnEnd => AwarenessEventType::CollabResolved,
        SyncPointKind::ClarifyAnswered => AwarenessEventType::ClarificationAnswered,
        SyncPointKind::ClarifyWindowOpened | SyncPointKind::ClarifyWindowClosed => {
            AwarenessEventType::SyncPointReported
        }
        SyncPointKind::ToolWindowOpened | SyncPointKind::ToolWindowClosed => {
            AwarenessEventType::SyncPointReported
        }
        SyncPointKind::HitlWindowOpened | SyncPointKind::HitlWindowClosed => {
            AwarenessEventType::SyncPointReported
        }
        SyncPointKind::HitlAbsorb => AwarenessEventType::HumanInjectionReceived,
        SyncPointKind::Merged => AwarenessEventType::SyncPointMerged,
        SyncPointKind::DriftDetected
        | SyncPointKind::LateSignalObserved
        | SyncPointKind::BudgetExceeded
        | SyncPointKind::BudgetRecovered
        | SyncPointKind::DegradationRecorded => AwarenessEventType::SyncPointReported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[cfg(feature = "vectors-extra")]
    use soulseed_agi_core_models::ExtraVectors;
    use soulseed_agi_core_models::legacy::dialogue_event as legacy;
    use soulseed_agi_core_models::{
        AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
        EnvelopeHead, EventId, HumanId, MessageId, MessagePointer, SessionId, Snapshot, Subject,
        SubjectRef, TraceId, convert_legacy_dialogue_event,
    };
    use time::OffsetDateTime;

    fn dummy_anchor() -> AwarenessAnchor {
        AwarenessAnchor {
            tenant_id: soulseed_agi_core_models::TenantId::new(1),
            envelope_id: uuid::Uuid::now_v7(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: None,
            sequence_number: None,
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        }
    }

    fn dummy_event() -> DialogueEvent {
        let now = OffsetDateTime::now_utc();
        let legacy_event = legacy::DialogueEvent {
            tenant_id: soulseed_agi_core_models::TenantId::new(1),
            event_id: EventId::from_raw_unchecked(1),
            session_id: SessionId::new(1),
            subject: Subject::Human(HumanId::new(1)),
            participants: vec![SubjectRef {
                kind: Subject::AI(soulseed_agi_core_models::AIId::new(1)),
                role: Some("assistant".into()),
            }],
            head: EnvelopeHead {
                envelope_id: uuid::Uuid::now_v7(),
                trace_id: TraceId("trace".into()),
                correlation_id: CorrelationId("correl".into()),
                config_snapshot_hash: "cfg".into(),
                config_snapshot_version: 1,
            },
            snapshot: Snapshot {
                schema_v: 1,
                created_at: now,
            },
            timestamp_ms: now.unix_timestamp() * 1000,
            scenario: ConversationScenario::HumanToAi,
            event_type: DialogueEventType::Message,
            time_window: None,
            access_class: AccessClass::Internal,
            provenance: None,
            sequence_number: 1,
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
            message_ref: Some(MessagePointer {
                message_id: MessageId::from_raw_unchecked(1),
            }),
            tool_invocation: None,
            tool_result: None,
            self_reflection: None,
            metadata: json!({}),
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        };
        convert_legacy_dialogue_event(legacy_event)
    }

    #[test]
    fn default_service_applies_high_priority() {
        let request = MergeDeltaRequest {
            cycle_id: AwarenessCycleId::from_raw_unchecked(1),
            anchor: dummy_anchor(),
            kind: SyncPointKind::HitlAbsorb,
            timeframe: (OffsetDateTime::now_utc(), OffsetDateTime::now_utc()),
            events: vec![dummy_event()],
            pending_injections: vec![HitlInjection::new(
                soulseed_agi_core_models::TenantId::new(1),
                crate::hitl::HitlPriority::P1High,
                "system",
                json!({}),
            )],
            budget_snapshot: crate::types::BudgetSnapshot {
                tokens_allowed: 100,
                tokens_spent: 10,
                walltime_ms_allowed: 1000,
                walltime_ms_used: 100,
                external_cost_allowed: 2.0,
                external_cost_spent: 0.5,
            },
            context_manifest: Value::Null,
        };

        let service = CaServiceDefault::default();
        let response = service.merge_delta(request).expect("merge");
        assert!(response.delta_patch.is_some());
        assert!(response.awareness_events.len() >= 2);
        assert_eq!(response.injections.len(), 1);
        assert_eq!(response.injections[0].action, InjectionAction::Applied);
        assert_eq!(
            response.injections[0].reason.as_deref(),
            Some("hitl_applied")
        );
        assert!(response.context_manifest.is_some());
        assert!(response.prompt_bundle.is_some());
        assert!(response.explain_bundle.is_some());
    }
}
