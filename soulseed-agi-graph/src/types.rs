use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use soulseed_agi_core_models::{
    AIId, AccessClass, AwarenessCycleId, AwarenessCycleRecord, AwarenessDegradationReason,
    AwarenessEvent, AwarenessEventType, AwarenessFork, ConversationScenario, CorrelationId,
    DialogueEvent, DialogueEventType, EmbeddingMeta, EnvelopeHead, EventId, EvidencePointer,
    GroupId, HumanId, Message, MessageId, MessagePointer, Provenance, RealTimePriority,
    RelationshipEdge, RelationshipSnapshot, SelfReflectionRecord, Session, SessionId, Snapshot,
    Subject, SubjectRef, SyncPointKind, TenantId, ToolId, ToolInvocation, ToolResult, TraceId,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeRef {
    Event(EventId),
    Message(MessageId),
    AwarenessCycle(AwarenessCycleId),
    Session(SessionId),
    Actor(SubjectRef),
    Context(String),
    Artifact(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Event,
    Message,
    AwarenessCycle,
    Session,
    Actor,
    Context,
    Artifact,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphNode {
    pub id: GraphNodeRef,
    pub kind: GraphNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl GraphNode {
    pub fn validate(&self) -> Result<(), GraphTopologyError> {
        if let Some(weight) = self.weight {
            if !(0.0..=1.0).contains(&weight) {
                return Err(GraphTopologyError::OutOfBounds("node.weight"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeKind {
    Causal,
    TriggeredBy,
    Supports,
    Blocks,
    Clarifies,
    Contradicts,
    Derives,
    Refines,
    Summarizes,
    References,
    Attaches,
    Follows,
    Precedes,
    SameTopic,
    SameActor,
    SameSession,
    SameContext,
    ParentAwarenessCycle,
    ChildAwarenessCycle,
    CollaboratesWith,
    Mentions,
    MentoredBy,
    Observes,
    Controls,
    Emits,
    Receives,
    GeneratesArtifact,
    ConsumesArtifact,
    VersionOf,
    Supersedes,
    SupersededBy,
    ImpactedBy,
    Affects,
    RoutesThrough,
    SyncPointWith,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphEdge {
    pub from: GraphNodeRef,
    pub to: GraphNodeRef,
    pub kind: GraphEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_decay: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub properties: Value,
}

impl GraphEdge {
    pub fn validate(&self) -> Result<(), GraphTopologyError> {
        if std::mem::discriminant(&self.from) == std::mem::discriminant(&self.to)
            && matches!(
                self.kind,
                GraphEdgeKind::ParentAwarenessCycle | GraphEdgeKind::ChildAwarenessCycle
            )
        {
            return Err(GraphTopologyError::InvalidRelation(
                "awareness cycle edges must connect distinct cycles",
            ));
        }

        if let Some(strength) = self.strength {
            if !(0.0..=1.0).contains(&strength) {
                return Err(GraphTopologyError::OutOfBounds("edge.strength"));
            }
        }
        if let Some(confidence) = self.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(GraphTopologyError::OutOfBounds("edge.confidence"));
            }
        }
        if let Some(decay) = self.temporal_decay {
            if decay < 0.0 {
                return Err(GraphTopologyError::OutOfBounds("edge.temporal_decay"));
            }
        }
        if let (Some(since), Some(until)) = (self.since_ms, self.until_ms) {
            if since > until {
                return Err(GraphTopologyError::InvalidRelation(
                    "edge.since_ms must be <= until_ms",
                ));
            }
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum GraphTopologyError {
    #[error("value out of bounds: {0}")]
    OutOfBounds(&'static str),
    #[error("invalid relationship: {0}")]
    InvalidRelation(&'static str),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConceptNode {
    pub concept_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
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
    pub evidence_pointer: Option<EvidencePointer>,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRef {
    Event(EventId),
    Concept(String),
    Topic(String),
    Emotion(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SemanticEdge {
    pub from: SemanticRef,
    pub to: SemanticRef,
    pub kind: SemanticEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_validation_bounds() {
        let edge = GraphEdge {
            from: GraphNodeRef::Event(EventId::from_raw_unchecked(1)),
            to: GraphNodeRef::Message(MessageId::from_raw_unchecked(2)),
            kind: GraphEdgeKind::References,
            strength: Some(0.5),
            confidence: Some(0.8),
            temporal_decay: Some(0.2),
            since_ms: Some(1),
            until_ms: Some(10),
            explain: Some("references message summary".into()),
            properties: Value::Null,
        };
        assert!(edge.validate().is_ok());

        let bad = GraphEdge {
            strength: Some(2.0),
            ..edge.clone()
        };
        assert!(matches!(
            bad.validate(),
            Err(GraphTopologyError::OutOfBounds("edge.strength"))
        ));
    }

    #[test]
    fn node_validation_bounds() {
        let node = GraphNode {
            id: GraphNodeRef::Session(SessionId::from_raw_unchecked(1)),
            kind: GraphNodeKind::Session,
            label: Some("session".into()),
            summary: None,
            weight: Some(0.7),
            metadata: None,
        };
        assert!(node.validate().is_ok());

        let bad = GraphNode {
            weight: Some(1.5),
            ..node.clone()
        };
        assert!(matches!(
            bad.validate(),
            Err(GraphTopologyError::OutOfBounds("node.weight"))
        ));
    }
}
