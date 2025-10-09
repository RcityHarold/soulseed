use serde::{Deserialize, Serialize};

pub use soulseed_agi_core_models::{
    AIId, AccessClass, AwarenessCycleRecord, AwarenessDegradationReason, AwarenessEvent,
    AwarenessEventType, AwarenessFork, ConversationScenario, CorrelationId, CycleId, DialogueEvent,
    DialogueEventType, EmbeddingMeta, EnvelopeHead, EventId, GroupId, HumanId, Message, MessageId,
    MessagePointer, Provenance, RealTimePriority, RelationshipEdge, RelationshipSnapshot,
    SelfReflectionRecord, Session, SessionId, Snapshot, Subject, SubjectRef, SyncPointKind,
    TenantId, ToolId, ToolInvocation, ToolResult, TraceId,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConceptNode {
    pub concept_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TopicNode {
    pub topic_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salience: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EmotionNode {
    pub emotion: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intensity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEdgeKind {
    EventToConcept,
    ConceptToConcept,
    EventToTopic,
    TopicToTopic,
    EventToEmotion,
    ConceptToTopic,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SemanticEdge {
    pub from: String,
    pub to: String,
    pub kind: SemanticEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}
