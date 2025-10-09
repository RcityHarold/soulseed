#[cfg(feature = "vectors-extra")]
use crate::vectors_ext::ExtraVectors;
use crate::{
    common::EvidencePointer, AccessClass, ConversationScenario, DialogueEventType, EmbeddingMeta,
    EnvelopeHead, EventId, MessageId, ModelError, Provenance, RealTimePriority, SessionId,
    Snapshot, Subject, SubjectRef, TenantId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueEvent {
    pub tenant_id: TenantId,
    pub event_id: EventId,
    pub session_id: SessionId,
    pub subject: Subject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    pub head: EnvelopeHead,
    pub snapshot: Snapshot,
    pub timestamp_ms: i64,
    pub scenario: ConversationScenario,
    pub event_type: DialogueEventType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<String>,
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,

    pub sequence_number: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_event_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_pattern_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causal_links: Vec<CausalLink>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_strategy: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_meta: Option<EmbeddingMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_vector: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_distance_to_goal: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_time_priority: Option<RealTimePriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_targets: Option<Vec<SubjectRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub growth_stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_latency_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_impact: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EventId>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<MessagePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_invocation: Option<ToolInvocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<ToolResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_reflection: Option<SelfReflectionRecord>,

    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    #[cfg(feature = "vectors-extra")]
    #[serde(default)]
    pub vectors: ExtraVectors,
}

impl DialogueEvent {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.sequence_number == 0 {
            return Err(ModelError::Invariant("sequence_number must be >= 1"));
        }
        if self.timestamp_ms < 0 {
            return Err(ModelError::Invariant("timestamp_ms must be non-negative"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        match self.event_type {
            DialogueEventType::Message => {
                if self.message_ref.is_none() {
                    return Err(ModelError::Missing("message_ref"));
                }
            }
            DialogueEventType::ToolCall => {
                if self.tool_invocation.is_none() {
                    return Err(ModelError::Missing("tool_invocation"));
                }
            }
            DialogueEventType::ToolResult => {
                if self.tool_result.is_none() {
                    return Err(ModelError::Missing("tool_result"));
                }
            }
            DialogueEventType::SelfReflection => {
                if self.self_reflection.is_none() {
                    return Err(ModelError::Missing("self_reflection"));
                }
            }
            DialogueEventType::Decision
            | DialogueEventType::Lifecycle
            | DialogueEventType::System => {}
        }
        if let Some(meta) = &self.embedding_meta {
            if let Some(vec) = &self.content_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "content_embedding length does not match embedding_meta.dim",
                    ));
                }
            }
            if let Some(vec) = &self.context_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "context_embedding length does not match embedding_meta.dim",
                    ));
                }
            }
            if let Some(vec) = &self.decision_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "decision_embedding length does not match embedding_meta.dim",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalLink {
    pub from_event: EventId,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessagePointer {
    pub message_id: MessageId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub tool_id: String,
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_id: String,
    pub call_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub output: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfReflectionRecord {
    pub topic: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub outcome: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}
