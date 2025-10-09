use blake3::Hasher;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use soulseed_agi_core_models::{
    CycleId,
    awareness::{AwarenessAnchor, AwarenessEvent, AwarenessEventType, DeltaPatch, SyncPointKind},
};
use std::collections::HashMap;
use time::OffsetDateTime;

use crate::errors::AceError;
use crate::hitl::HitlInjection;
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
    pub cycle_id: CycleId,
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
}

pub trait CaService: Send + Sync {
    fn merge_delta(&self, request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError>;
}

#[derive(Default)]
pub struct CaServiceDefault;

impl CaService for CaServiceDefault {
    fn merge_delta(&self, request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError> {
        let mut added: Vec<String> = Vec::new();
        let mut pointers = Vec::new();
        for event in &request.events {
            added.push(format!("event:{}", event.event_id.0));
            if let Some(ptr) = event.evidence_pointer.as_ref() {
                pointers.push(json!({
                    "event_id": event.event_id.0,
                    "evidence": ptr,
                }));
            }
        }

        let mut hasher = Hasher::new();
        hasher.update(&request.cycle_id.0.to_le_bytes());
        hasher.update(format!("{:?}", request.kind).as_bytes());
        hasher.update(
            request
                .anchor
                .tenant_id
                .into_inner()
                .to_le_bytes()
                .as_slice(),
        );
        for id in &added {
            hasher.update(id.as_bytes());
        }
        let patch_digest = format!("blake3:{}", hasher.finalize().to_hex());

        let delta_patch = if added.is_empty() {
            None
        } else {
            Some(DeltaPatch {
                patch_id: format!("ace-patch-{}-{}", request.cycle_id.0, request.kind as u32),
                added,
                updated: Vec::new(),
                removed: Vec::new(),
                score_stats: HashMap::new(),
                why_included: None,
                pointers: if pointers.is_empty() {
                    None
                } else {
                    Some(Value::Array(pointers))
                },
                patch_digest,
            })
        };

        let mut awareness_events: Vec<AwarenessEvent> = Vec::new();
        let base_event_id = request.cycle_id.0 + 1;
        if let Some(patch) = delta_patch.as_ref() {
            awareness_events.push(AwarenessEvent {
                anchor: request.anchor.clone(),
                event_id: soulseed_agi_core_models::EventId(base_event_id),
                event_type: map_syncpoint_event(&request.kind),
                occurred_at_ms: request.timeframe.1.unix_timestamp() * 1000,
                awareness_cycle_id: request.cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: json!({
                    "patch_id": patch.patch_id,
                    "digest": patch.patch_digest,
                }),
            });
        }

        let injections: Vec<InjectionDecision> = request
            .pending_injections
            .into_iter()
            .enumerate()
            .map(|(idx, inj)| {
                let action = match inj.priority {
                    crate::hitl::HitlPriority::P0Critical => InjectionAction::Applied,
                    crate::hitl::HitlPriority::P1High if idx == 0 => InjectionAction::Applied,
                    crate::hitl::HitlPriority::P1High => InjectionAction::Deferred,
                    crate::hitl::HitlPriority::P2Medium => InjectionAction::Deferred,
                    crate::hitl::HitlPriority::P3Low => InjectionAction::Ignored,
                };
                InjectionDecision {
                    injection_id: inj.injection_id,
                    action,
                    reason: Some(match action {
                        InjectionAction::Applied => "hitl_applied".into(),
                        InjectionAction::Deferred => "hitl_deferred".into(),
                        InjectionAction::Ignored => "hitl_ignored".into(),
                    }),
                    fingerprint: Some(format!("fp:{}", inj.injection_id)),
                }
            })
            .collect();

        Ok(MergeDeltaResponse {
            delta_patch,
            awareness_events,
            injections,
        })
    }
}

fn map_syncpoint_event(kind: &SyncPointKind) -> AwarenessEventType {
    match kind {
        SyncPointKind::IcEnd => AwarenessEventType::IcEnded,
        SyncPointKind::ToolBarrier => AwarenessEventType::ToolCalled,
        SyncPointKind::ToolChainNext => AwarenessEventType::RouteSwitched,
        SyncPointKind::CollabTurnEnd => AwarenessEventType::CollabResolved,
        SyncPointKind::ClarifyAnswered => AwarenessEventType::ClarificationAnswered,
        SyncPointKind::HitlAbsorb => AwarenessEventType::HumanInjectionReceived,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[cfg(feature = "vectors-extra")]
    use soulseed_agi_core_models::ExtraVectors;
    use soulseed_agi_core_models::{
        AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
        EnvelopeHead, EventId, HumanId, MessageId, MessagePointer, SessionId, Snapshot, Subject,
        SubjectRef, TraceId,
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
        DialogueEvent {
            tenant_id: soulseed_agi_core_models::TenantId::new(1),
            event_id: EventId(1),
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
                message_id: MessageId(1),
            }),
            tool_invocation: None,
            tool_result: None,
            self_reflection: None,
            metadata: json!({}),
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        }
    }

    #[test]
    fn default_service_applies_high_priority() {
        let request = MergeDeltaRequest {
            cycle_id: CycleId(1),
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
        assert_eq!(response.awareness_events.len(), 1);
        assert_eq!(response.injections.len(), 1);
        assert_eq!(response.injections[0].action, InjectionAction::Applied);
        assert_eq!(
            response.injections[0].reason.as_deref(),
            Some("hitl_applied")
        );
    }
}
