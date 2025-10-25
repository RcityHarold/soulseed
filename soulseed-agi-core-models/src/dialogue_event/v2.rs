use crate::dialogue_event::{
    AwarenessPayload, ContentReference, DialogueEventPayload, LifecyclePayload, MessagePayload,
    SelfReflectionPayload, SystemPayload, ToolPayload,
};
#[cfg(feature = "vectors-extra")]
use crate::vectors_ext::ExtraVectors;
use crate::MessageId;
use crate::{
    common::{EmbeddingMeta, EvidencePointer, ModelError, Provenance, SubjectRef},
    legacy::dialogue_event as legacy,
    AccessClass, AwarenessCycleId, AwarenessEventType, ConversationScenario, DialogueEventType,
    EnvelopeHead, EventId, RealTimePriority, SessionId, Snapshot, Subject, TenantId,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use legacy::{CausalLink, MessagePointer, SelfReflectionRecord, ToolInvocation, ToolResult};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueEvent {
    pub base: DialogueEventBase,
    pub payload: DialogueEventPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enhancements: Option<DialogueEventEnhancements>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
    #[cfg(feature = "vectors-extra")]
    #[serde(default)]
    pub vectors: ExtraVectors,
}

impl DialogueEvent {
    pub fn new(base: DialogueEventBase, payload: DialogueEventPayload) -> Self {
        Self {
            base,
            payload,
            enhancements: None,
            metadata: Value::Null,
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        }
    }

    pub fn with_enhancements(mut self, enhancements: DialogueEventEnhancements) -> Self {
        self.enhancements = Some(enhancements);
        self
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        self.base.validate()?;
        self.payload
            .validate_for_event_type(self.base.event_type.clone())?;
        if let Some(enh) = &self.enhancements {
            enh.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueEventBase {
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
    pub stage_hint: Option<String>,
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub sequence_number: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac_id: Option<AwarenessCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ic_sequence: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_ac_id: Option<AwarenessCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_event_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EventId>,
}

impl DialogueEventBase {
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
        if let Some(version) = &self.config_version {
            let trimmed = version.trim();
            if trimmed.is_empty() {
                return Err(ModelError::Invariant(
                    "config_version cannot be empty when provided",
                ));
            }
            Version::parse(trimmed).map_err(|_| {
                ModelError::Invariant("config_version must be a valid semver string")
            })?;
        }
        if let (Some(parent), Some(current)) = (self.parent_ac_id, self.ac_id) {
            if parent == current {
                return Err(ModelError::Invariant("parent_ac_id cannot equal ac_id"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DialogueEventEnhancements {
    pub meta: EnhancementMeta,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<EnhancementSnapshot>,
}

impl Default for DialogueEventEnhancements {
    fn default() -> Self {
        Self {
            meta: EnhancementMeta::default(),
            versions: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::{Subject, SubjectRef},
        enums::RealTimePriority,
        ids::{EventId, HumanId},
    };

    #[test]
    fn legacy_record_roundtrip() {
        let record = LegacyEnhancementRecord {
            time_window: Some("stage-alpha".into()),
            temporal_pattern_id: Some("pattern-1".into()),
            causal_links: vec![CausalLink {
                from_event: EventId::from_raw_unchecked(1),
                relation: "causes".into(),
                weight: Some(0.8),
                confidence: Some(0.6),
            }],
            content_embedding: Some(vec![0.1, 0.2]),
            context_embedding: Some(vec![0.1, 0.2]),
            decision_embedding: Some(vec![0.2, 0.3]),
            embedding_meta: Some(EmbeddingMeta {
                model: "mini-embed".into(),
                dim: 2,
                ts: 1700,
            }),
            concept_vector: Some(vec![0.3, 0.4]),
            semantic_cluster_id: Some("cluster-1".into()),
            cluster_method: Some("hdbscan".into()),
            concept_distance_to_goal: Some(0.42),
            real_time_priority: Some(RealTimePriority::High),
            notification_targets: Some(vec![SubjectRef {
                kind: Subject::Human(HumanId::from_raw_unchecked(77)),
                role: Some("observer".into()),
            }]),
            live_stream_id: Some("live-1".into()),
            growth_stage: Some("scale".into()),
            processing_latency_ms: Some(35),
            influence_score: Some(0.91),
            community_impact: Some(0.37),
        };
        let migrated = migrate_legacy_enhancements(record.clone(), 1_234);
        assert_eq!(migrated.meta.latest_version, 1);
        assert_eq!(migrated.meta.last_generated_ms, Some(1_234));
        assert_eq!(migrated.versions.len(), 1);
        let restored = downgrade_to_legacy_enhancements(&migrated);
        assert_eq!(restored, record);
    }

    #[test]
    fn enhancements_append_only_rule() {
        let mut enhancements = DialogueEventEnhancements::default();
        let mut first = EnhancementSnapshot::default();
        first.version = 1;
        first.created_at_ms = 10;
        first.temporal.time_window = Some("alpha".into());
        enhancements.record_snapshot(first).expect("first snapshot ok");
        assert_eq!(enhancements.meta.latest_version, 1);
        let mut duplicate = EnhancementSnapshot::default();
        duplicate.version = 1;
        duplicate.created_at_ms = 11;
        duplicate.temporal.temporal_pattern_id = Some("duplicate".into());
        assert!(enhancements.record_snapshot(duplicate).is_err());
        let mut second = EnhancementSnapshot::default();
        second.version = 2;
        second.created_at_ms = 12;
        second.temporal.temporal_pattern_id = Some("beta".into());
        enhancements.record_snapshot(second).expect("second snapshot ok");
        assert_eq!(enhancements.meta.latest_version, 2);
        assert_eq!(enhancements.versions.len(), 2);
    }
}

impl DialogueEventEnhancements {
    pub fn latest(&self) -> Option<&EnhancementSnapshot> {
        self.versions.last()
    }

    pub fn latest_mut(&mut self) -> Option<&mut EnhancementSnapshot> {
        self.versions.last_mut()
    }

    pub fn record_snapshot(&mut self, snapshot: EnhancementSnapshot) -> Result<(), ModelError> {
        if snapshot.version == 0 {
            return Err(ModelError::Invariant(
                "enhancements.snapshot version must be >= 1",
            ));
        }
        if let Some(last) = self.versions.last() {
            if snapshot.version <= last.version {
                return Err(ModelError::Invariant(
                    "enhancements.snapshot version must be strictly increasing",
                ));
            }
        }
        if !self.meta.append_only {
            return Err(ModelError::Invariant(
                "enhancements meta must be append-only for new versions",
            ));
        }
        snapshot.validate()?;
        self.meta.latest_version = snapshot.version;
        self.meta.last_generated_ms = Some(snapshot.created_at_ms);
        self.versions.push(snapshot);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.meta.append_only {
            return Err(ModelError::Invariant("enhancements must be append-only"));
        }
        if self.versions.is_empty() {
            if self.meta.latest_version != 0 {
                return Err(ModelError::Invariant(
                    "enhancements meta.latest_version must be 0 when no versions present",
                ));
            }
            return Ok(());
        }
        let mut previous = 0u32;
        for snapshot in &self.versions {
            if snapshot.version == 0 {
                return Err(ModelError::Invariant(
                    "enhancements.snapshot version must be >= 1",
                ));
            }
            if snapshot.version <= previous {
                return Err(ModelError::Invariant(
                    "enhancements versions must be strictly increasing",
                ));
            }
            snapshot.validate()?;
            previous = snapshot.version;
        }
        if self.meta.latest_version != previous {
            return Err(ModelError::Invariant(
                "enhancements meta.latest_version must match latest snapshot version",
            ));
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty() || self.versions.iter().all(EnhancementSnapshot::is_empty)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EnhancementMeta {
    pub schema_version: Version,
    #[serde(default)]
    pub append_only: bool,
    #[serde(default)]
    pub latest_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_generated_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Default for EnhancementMeta {
    fn default() -> Self {
        Self {
            schema_version: Version::new(1, 0, 0),
            append_only: true,
            latest_version: 0,
            generator: None,
            last_generated_ms: None,
            notes: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct EnhancementSnapshot {
    pub version: u32,
    pub created_at_ms: i64,
    #[serde(default)]
    pub semantic: SemanticEnhancement,
    #[serde(default)]
    pub temporal: TemporalEnhancement,
    #[serde(default)]
    pub spatial: SpatialEnhancement,
    #[serde(default)]
    pub realtime: RealtimeEnhancement,
    #[serde(default)]
    pub graph: GraphEnhancement,
}

impl EnhancementSnapshot {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.created_at_ms < 0 {
            return Err(ModelError::Invariant(
                "enhancements.snapshot created_at_ms must be non-negative",
            ));
        }
        self.semantic.validate()?;
        self.temporal.validate()?;
        self.spatial.validate()?;
        self.realtime.validate()?;
        self.graph.validate()?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.semantic.is_empty()
            && self.temporal.is_empty()
            && self.spatial.is_empty()
            && self.realtime.is_empty()
            && self.graph.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SemanticEnhancement {
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
}

impl SemanticEnhancement {
    pub fn validate(&self) -> Result<(), ModelError> {
        if let Some(meta) = &self.embedding_meta {
            if let Some(vec) = &self.content_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "enhancements.semantic.content_embedding length mismatch",
                    ));
                }
            }
            if let Some(vec) = &self.context_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "enhancements.semantic.context_embedding length mismatch",
                    ));
                }
            }
            if let Some(vec) = &self.decision_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "enhancements.semantic.decision_embedding length mismatch",
                    ));
                }
            }
            if let Some(vec) = &self.concept_vector {
                if vec.len() as u16 != meta.dim {
                    return Err(ModelError::Invariant(
                        "enhancements.semantic.concept_vector length mismatch",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.content_embedding.is_none()
            && self.context_embedding.is_none()
            && self.decision_embedding.is_none()
            && self.embedding_meta.is_none()
            && self.concept_vector.is_none()
            && self.semantic_cluster_id.is_none()
            && self.cluster_method.is_none()
            && self.concept_distance_to_goal.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TemporalEnhancement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_pattern_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_latency_ms: Option<u32>,
}

impl TemporalEnhancement {
    pub fn validate(&self) -> Result<(), ModelError> {
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.time_window.is_none()
            && self.temporal_pattern_id.is_none()
            && self.processing_latency_ms.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SpatialEnhancement {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notification_targets: Vec<SubjectRef>,
}

impl SpatialEnhancement {
    pub fn validate(&self) -> Result<(), ModelError> {
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.notification_targets.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RealtimeEnhancement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_time_priority: Option<RealTimePriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub growth_stage: Option<String>,
}

impl RealtimeEnhancement {
    pub fn validate(&self) -> Result<(), ModelError> {
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.real_time_priority.is_none()
            && self.live_stream_id.is_none()
            && self.growth_stage.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GraphEnhancement {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causal_links: Vec<CausalLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_impact: Option<f32>,
}

impl GraphEnhancement {
    pub fn validate(&self) -> Result<(), ModelError> {
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.causal_links.is_empty()
            && self.influence_score.is_none()
            && self.community_impact.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LegacyEnhancementRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_pattern_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causal_links: Vec<CausalLink>,
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
}

impl LegacyEnhancementRecord {
    pub fn is_empty(&self) -> bool {
        self.time_window.is_none()
            && self.temporal_pattern_id.is_none()
            && self.causal_links.is_empty()
            && self.content_embedding.is_none()
            && self.context_embedding.is_none()
            && self.decision_embedding.is_none()
            && self.embedding_meta.is_none()
            && self.concept_vector.is_none()
            && self.semantic_cluster_id.is_none()
            && self.cluster_method.is_none()
            && self.concept_distance_to_goal.is_none()
            && self.real_time_priority.is_none()
            && self.notification_targets
                .as_ref()
                .map(|targets| targets.is_empty())
                .unwrap_or(true)
            && self.live_stream_id.is_none()
            && self.growth_stage.is_none()
            && self.processing_latency_ms.is_none()
            && self.influence_score.is_none()
            && self.community_impact.is_none()
    }

    fn into_snapshot(self, created_at_ms: i64) -> Option<EnhancementSnapshot> {
        if self.is_empty() {
            return None;
        }
        let spatial_targets = self.notification_targets.unwrap_or_default();
        Some(EnhancementSnapshot {
            version: 1,
            created_at_ms,
            semantic: SemanticEnhancement {
                content_embedding: self.content_embedding,
                context_embedding: self.context_embedding,
                decision_embedding: self.decision_embedding,
                embedding_meta: self.embedding_meta,
                concept_vector: self.concept_vector,
                semantic_cluster_id: self.semantic_cluster_id,
                cluster_method: self.cluster_method,
                concept_distance_to_goal: self.concept_distance_to_goal,
            },
            temporal: TemporalEnhancement {
                time_window: self.time_window,
                temporal_pattern_id: self.temporal_pattern_id,
                processing_latency_ms: self.processing_latency_ms,
            },
            spatial: SpatialEnhancement {
                notification_targets: spatial_targets,
            },
            realtime: RealtimeEnhancement {
                real_time_priority: self.real_time_priority,
                live_stream_id: self.live_stream_id,
                growth_stage: self.growth_stage,
            },
            graph: GraphEnhancement {
                causal_links: self.causal_links,
                influence_score: self.influence_score,
                community_impact: self.community_impact,
            },
        })
    }

    fn from_snapshot(snapshot: &EnhancementSnapshot) -> Self {
        LegacyEnhancementRecord {
            time_window: snapshot.temporal.time_window.clone(),
            temporal_pattern_id: snapshot.temporal.temporal_pattern_id.clone(),
            causal_links: snapshot.graph.causal_links.clone(),
            content_embedding: snapshot.semantic.content_embedding.clone(),
            context_embedding: snapshot.semantic.context_embedding.clone(),
            decision_embedding: snapshot.semantic.decision_embedding.clone(),
            embedding_meta: snapshot.semantic.embedding_meta.clone(),
            concept_vector: snapshot.semantic.concept_vector.clone(),
            semantic_cluster_id: snapshot.semantic.semantic_cluster_id.clone(),
            cluster_method: snapshot.semantic.cluster_method.clone(),
            concept_distance_to_goal: snapshot.semantic.concept_distance_to_goal,
            real_time_priority: snapshot.realtime.real_time_priority.clone(),
            notification_targets: if snapshot.spatial.notification_targets.is_empty() {
                None
            } else {
                Some(snapshot.spatial.notification_targets.clone())
            },
            live_stream_id: snapshot.realtime.live_stream_id.clone(),
            growth_stage: snapshot.realtime.growth_stage.clone(),
            processing_latency_ms: snapshot.temporal.processing_latency_ms,
            influence_score: snapshot.graph.influence_score,
            community_impact: snapshot.graph.community_impact,
        }
    }
}

pub fn migrate_legacy_enhancements(
    record: LegacyEnhancementRecord,
    created_at_ms: i64,
) -> DialogueEventEnhancements {
    let mut enhancements = DialogueEventEnhancements::default();
    if let Some(snapshot) = record.into_snapshot(created_at_ms) {
        enhancements.meta.generator = Some("legacy-migration".into());
        enhancements.meta.notes = Some("converted_from_legacy".into());
        enhancements.meta.latest_version = snapshot.version;
        enhancements.meta.last_generated_ms = Some(created_at_ms);
        enhancements.versions.push(snapshot);
    }
    enhancements
}

pub fn downgrade_to_legacy_enhancements(
    enhancements: &DialogueEventEnhancements,
) -> LegacyEnhancementRecord {
    enhancements
        .latest()
        .map(LegacyEnhancementRecord::from_snapshot)
        .unwrap_or_default()
}

impl From<legacy::DialogueEvent> for DialogueEvent {
    fn from(legacy: legacy::DialogueEvent) -> Self {
        let legacy::DialogueEvent {
            tenant_id,
            event_id,
            session_id,
            subject,
            participants,
            head,
            snapshot,
            timestamp_ms,
            scenario,
            event_type,
            time_window,
            access_class,
            provenance,
            sequence_number,
            trigger_event_id,
            temporal_pattern_id,
            causal_links,
            reasoning_trace,
            reasoning_confidence,
            reasoning_strategy,
            content_embedding,
            context_embedding,
            decision_embedding,
            embedding_meta,
            concept_vector,
            semantic_cluster_id,
            cluster_method,
            concept_distance_to_goal,
            real_time_priority,
            notification_targets,
            live_stream_id,
            growth_stage,
            processing_latency_ms,
            influence_score,
            community_impact,
            evidence_pointer,
            content_digest_sha256,
            blob_ref,
            supersedes,
            superseded_by,
            message_ref,
            tool_invocation,
            tool_result,
            self_reflection,
            metadata,
            #[cfg(feature = "vectors-extra")]
            vectors,
        } = legacy;

        let event_type_clone = event_type.clone();

        let base = DialogueEventBase {
            tenant_id,
            event_id,
            session_id,
            subject,
            participants,
            head,
            snapshot,
            timestamp_ms,
            scenario,
            event_type,
            stage_hint: time_window.clone(),
            access_class,
            provenance,
            sequence_number,
            ac_id: None,
            ic_sequence: None,
            parent_ac_id: None,
            config_version: None,
            trigger_event_id,
            supersedes,
            superseded_by,
        };

        let content_reference = ContentReference {
            message: message_ref.clone(),
            digest_sha256: content_digest_sha256.clone(),
            evidence: evidence_pointer.clone(),
            blob_ref: blob_ref.clone(),
        };
        let content_ref = if content_reference.is_empty() {
            None
        } else {
            Some(content_reference)
        };

        let evidence_vec: Vec<EvidencePointer> = evidence_pointer.clone().into_iter().collect();

        let payload = match event_type_clone {
            DialogueEventType::Message => {
                let message = message_ref.unwrap_or_else(|| MessagePointer {
                    message_id: MessageId::from_raw_unchecked(0),
                });
                DialogueEventPayload::MessagePrimary(MessagePayload {
                    message,
                    content_ref,
                    reply_to: None,
                    mentions: Vec::new(),
                    channel: None,
                    language: None,
                    sentiment: None,
                    is_final: None,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::ToolCall => {
                let invocation = tool_invocation.unwrap_or_else(|| ToolInvocation {
                    tool_id: "legacy.placeholder".into(),
                    call_id: "legacy-call".into(),
                    input: Value::Null,
                    strategy: None,
                });
                DialogueEventPayload::ToolInvocationDispatched(ToolPayload {
                    plan: None,
                    invocation: Some(invocation),
                    result: None,
                    barrier_id: None,
                    route_id: None,
                    attempt: None,
                    latency_ms: None,
                    evidence: evidence_vec.clone(),
                    output_digest_sha256: content_digest_sha256.clone(),
                    blob_ref: blob_ref.clone(),
                    degradation_reason: None,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::ToolResult => {
                let result = tool_result.unwrap_or_else(|| ToolResult {
                    tool_id: "legacy.placeholder".into(),
                    call_id: "legacy-call".into(),
                    success: false,
                    output: Value::Null,
                    error: Some("missing_legacy_tool_result".into()),
                    degradation_reason: None,
                });
                DialogueEventPayload::ToolResultRecorded(ToolPayload {
                    plan: None,
                    invocation: None,
                    result: Some(result),
                    barrier_id: None,
                    route_id: None,
                    attempt: None,
                    latency_ms: None,
                    evidence: evidence_vec,
                    output_digest_sha256: content_digest_sha256.clone(),
                    blob_ref: blob_ref.clone(),
                    degradation_reason: None,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::SelfReflection => {
                let record = self_reflection.unwrap_or_else(|| SelfReflectionRecord {
                    topic: "legacy".into(),
                    outcome: Value::Null,
                    confidence: None,
                });
                DialogueEventPayload::SelfReflectionLogged(SelfReflectionPayload {
                    record,
                    insight_digest: content_digest_sha256.clone(),
                    related_event_id: base.trigger_event_id,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::Decision => {
                DialogueEventPayload::AwarenessDecisionRouted(AwarenessPayload {
                    event_type: AwarenessEventType::DecisionRouted,
                    decision_path: None,
                    delta_patch: None,
                    sync_point_report: None,
                    reasoning_trace,
                    reasoning_confidence,
                    reasoning_strategy,
                    degradation_reason: None,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::Lifecycle => {
                DialogueEventPayload::LifecycleStageEntered(LifecyclePayload {
                    stage: time_window.clone(),
                    supersedes: None,
                    superseded_by: None,
                    parent_event_id: None,
                    chain_length: None,
                    checkpoint_id: None,
                    attributes: Value::Null,
                })
            }
            DialogueEventType::System => {
                DialogueEventPayload::SystemNotificationDispatched(SystemPayload {
                    category: "legacy_system".into(),
                    related_service: None,
                    detail: Value::Null,
                })
            }
        };

        let legacy_record = LegacyEnhancementRecord {
            time_window,
            temporal_pattern_id,
            causal_links,
            content_embedding,
            context_embedding,
            decision_embedding,
            embedding_meta,
            concept_vector,
            semantic_cluster_id,
            cluster_method,
            concept_distance_to_goal,
            real_time_priority,
            notification_targets,
            live_stream_id,
            growth_stage,
            processing_latency_ms,
            influence_score,
            community_impact,
        };
        let enhancements = migrate_legacy_enhancements(legacy_record, timestamp_ms);
        let enhancements_opt = if enhancements.is_empty() {
            None
        } else {
            Some(enhancements)
        };

        DialogueEvent {
            base,
            payload,
            enhancements: enhancements_opt,
            metadata,
            #[cfg(feature = "vectors-extra")]
            vectors,
        }
    }
}

impl From<DialogueEvent> for legacy::DialogueEvent {
    fn from(event: DialogueEvent) -> Self {
        let DialogueEvent {
            base,
            payload,
            enhancements,
            metadata,
            #[cfg(feature = "vectors-extra")]
            vectors,
        } = event;

        let enhancements = enhancements.unwrap_or_default();
        let legacy_record = downgrade_to_legacy_enhancements(&enhancements);
        let LegacyEnhancementRecord {
            time_window: temporal_time_window,
            temporal_pattern_id: temporal_pattern_id_opt,
            causal_links,
            content_embedding,
            context_embedding,
            decision_embedding,
            embedding_meta,
            concept_vector,
            semantic_cluster_id,
            cluster_method,
            concept_distance_to_goal,
            real_time_priority,
            notification_targets,
            live_stream_id,
            growth_stage,
            processing_latency_ms,
            influence_score,
            community_impact,
        } = legacy_record;

        let mut message_ref: Option<MessagePointer> = None;
        let mut tool_invocation: Option<ToolInvocation> = None;
        let mut tool_result: Option<ToolResult> = None;
        let mut self_reflection: Option<SelfReflectionRecord> = None;
        let mut reasoning_trace: Option<String> = None;
        let mut reasoning_confidence: Option<f32> = None;
        let mut reasoning_strategy: Option<String> = None;
        let mut evidence_pointer: Option<EvidencePointer> = None;
        let mut content_digest_sha256: Option<String> = None;
        let mut blob_ref: Option<String> = None;

        match &payload {
            DialogueEventPayload::MessagePrimary(inner)
            | DialogueEventPayload::MessageEdited(inner)
            | DialogueEventPayload::MessageRegenerated(inner)
            | DialogueEventPayload::MessageSummaryPublished(inner)
            | DialogueEventPayload::MessageRecalled(inner)
            | DialogueEventPayload::MessagePinned(inner)
            | DialogueEventPayload::MessageFeedbackCaptured(inner)
            | DialogueEventPayload::MessageReactionAdded(inner)
            | DialogueEventPayload::MessageReactionRemoved(inner)
            | DialogueEventPayload::MessageThreadStarted(inner)
            | DialogueEventPayload::MessageThreadReplied(inner)
            | DialogueEventPayload::MessageAttachmentLinked(inner)
            | DialogueEventPayload::MessageAttachmentRemoved(inner)
            | DialogueEventPayload::MessageTranslated(inner)
            | DialogueEventPayload::MessageBroadcast(inner) => {
                message_ref = Some(inner.message.clone());
                if let Some(content) = &inner.content_ref {
                    if content_digest_sha256.is_none() {
                        content_digest_sha256 = content.digest_sha256.clone();
                    }
                    if evidence_pointer.is_none() {
                        evidence_pointer = content.evidence.clone();
                    }
                    if blob_ref.is_none() {
                        blob_ref = content.blob_ref.clone();
                    }
                }
            }
            DialogueEventPayload::ToolPlanDrafted(inner)
            | DialogueEventPayload::ToolPlanValidated(inner)
            | DialogueEventPayload::ToolPlanRejected(inner)
            | DialogueEventPayload::ToolPlanCommitted(inner)
            | DialogueEventPayload::ToolPathDecided(inner)
            | DialogueEventPayload::ToolInvocationScheduled(inner)
            | DialogueEventPayload::ToolInvocationDispatched(inner)
            | DialogueEventPayload::ToolInvocationStarted(inner)
            | DialogueEventPayload::ToolInvocationProgressed(inner)
            | DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolInvocationRetried(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) => {
                if let Some(inv) = &inner.invocation {
                    tool_invocation = Some(inv.clone());
                }
                if let Some(res) = &inner.result {
                    tool_result = Some(res.clone());
                }
                if let Some(first) = inner.evidence.first().cloned() {
                    evidence_pointer = Some(first);
                }
                if content_digest_sha256.is_none() {
                    content_digest_sha256 = inner.output_digest_sha256.clone();
                }
                if blob_ref.is_none() {
                    blob_ref = inner.blob_ref.clone();
                }
            }
            DialogueEventPayload::SelfReflectionLogged(inner)
            | DialogueEventPayload::SelfReflectionHypothesisFormed(inner)
            | DialogueEventPayload::SelfReflectionActionCommitted(inner)
            | DialogueEventPayload::SelfReflectionScoreAdjusted(inner)
            | DialogueEventPayload::SelfReflectionArchived(inner) => {
                self_reflection = Some(inner.record.clone());
                if content_digest_sha256.is_none() {
                    content_digest_sha256 = inner.insight_digest.clone();
                }
            }
            DialogueEventPayload::AwarenessAcStarted(inner)
            | DialogueEventPayload::AwarenessIcStarted(inner)
            | DialogueEventPayload::AwarenessIcEnded(inner)
            | DialogueEventPayload::AwarenessAssessmentProduced(inner)
            | DialogueEventPayload::AwarenessDecisionRouted(inner)
            | DialogueEventPayload::AwarenessRouteReconsidered(inner)
            | DialogueEventPayload::AwarenessRouteSwitched(inner)
            | DialogueEventPayload::AwarenessDeltaPatchGenerated(inner)
            | DialogueEventPayload::AwarenessContextBuilt(inner)
            | DialogueEventPayload::AwarenessDeltaMerged(inner)
            | DialogueEventPayload::AwarenessFinalized(inner)
            | DialogueEventPayload::AwarenessRejected(inner)
            | DialogueEventPayload::AwarenessSyncPointReported(inner)
            | DialogueEventPayload::AwarenessLateReceiptObserved(inner) => {
                reasoning_trace = inner.reasoning_trace.clone();
                reasoning_confidence = inner.reasoning_confidence;
                reasoning_strategy = inner.reasoning_strategy.clone();
            }
            _ => {}
        }

        let payload_kind = payload.kind();
        let mut metadata_value = metadata;
        let mut metadata_map = match metadata_value {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                let mut map = Map::new();
                map.insert("legacy_metadata".into(), other);
                map
            }
        };
        metadata_map.insert(
            "_v2_payload_kind".into(),
            Value::String(payload_kind.to_string()),
        );
        if let Ok(serialized) = serde_json::to_value(&payload) {
            metadata_map.insert("_v2_payload".into(), serialized);
        }
        metadata_value = Value::Object(metadata_map);

        legacy::DialogueEvent {
            tenant_id: base.tenant_id,
            event_id: base.event_id,
            session_id: base.session_id,
            subject: base.subject,
            participants: base.participants,
            head: base.head,
            snapshot: base.snapshot,
            timestamp_ms: base.timestamp_ms,
            scenario: base.scenario,
            event_type: base.event_type,
            time_window: temporal_time_window,
            access_class: base.access_class,
            provenance: base.provenance,
            sequence_number: base.sequence_number,
            trigger_event_id: base.trigger_event_id,
            temporal_pattern_id: temporal_pattern_id_opt,
            causal_links,
            reasoning_trace,
            reasoning_confidence,
            reasoning_strategy,
            content_embedding,
            context_embedding,
            decision_embedding,
            embedding_meta,
            concept_vector,
            semantic_cluster_id,
            cluster_method,
            concept_distance_to_goal,
            real_time_priority,
            notification_targets,
            live_stream_id,
            growth_stage,
            processing_latency_ms,
            influence_score,
            community_impact,
            evidence_pointer,
            content_digest_sha256,
            blob_ref,
            supersedes: base.supersedes,
            superseded_by: base.superseded_by,
            message_ref,
            tool_invocation,
            tool_result,
            self_reflection,
            metadata: metadata_value,
            #[cfg(feature = "vectors-extra")]
            vectors,
        }
    }
}
