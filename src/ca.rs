use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::{
    CycleId,
    awareness::{AwarenessAnchor, AwarenessEvent, DeltaPatch, SyncPointKind},
};
use time::OffsetDateTime;

use crate::errors::AceError;
use crate::hitl::HitlInjection;
use crate::types::{BudgetSnapshot, SyncPointInput};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectionAction {
    Applied,
    Deferred,
    Ignored,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InjectionDecision {
    pub injection_id: uuid::Uuid,
    pub action: InjectionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MergeDeltaRequest {
    pub cycle_id: CycleId,
    pub anchor: AwarenessAnchor,
    pub kind: SyncPointKind,
    pub timeframe: (OffsetDateTime, OffsetDateTime),
    pub events: Vec<soulseed_agi_core_models::DialogueEvent>,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
pub struct CaServiceNoop;

impl CaService for CaServiceNoop {
    fn merge_delta(&self, request: MergeDeltaRequest) -> Result<MergeDeltaResponse, AceError> {
        Ok(MergeDeltaResponse {
            delta_patch: None,
            awareness_events: Vec::new(),
            injections: request
                .pending_injections
                .into_iter()
                .map(|inj| InjectionDecision {
                    injection_id: inj.injection_id,
                    action: InjectionAction::Ignored,
                    reason: Some("noop_ca_service".into()),
                    fingerprint: None,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
            message_ref: Some(MessagePointer {
                message_id: MessageId(1),
            }),
            tool_invocation: None,
            tool_result: None,
            self_reflection: None,
            metadata: json!({}),
        }
    }

    #[test]
    fn noop_service_marks_injections_ignored() {
        let request = MergeDeltaRequest {
            cycle_id: CycleId(1),
            anchor: dummy_anchor(),
            kind: SyncPointKind::Barrier,
            timeframe: (
                OffsetDateTime::now_utc(),
                OffsetDateTime::now_utc(),
            ),
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
            },
            context_manifest: Value::Null,
        };

        let service = CaServiceNoop::default();
        let response = service.merge_delta(request).expect("merge");
        assert!(response.delta_patch.is_none());
        assert!(response.awareness_events.is_empty());
        assert_eq!(response.injections.len(), 1);
        assert_eq!(response.injections[0].action, InjectionAction::Ignored);
    }
}
