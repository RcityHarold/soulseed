use crate::{AccessClass, DialogueEvent, GraphError};

pub trait InvariantCheck {
    fn validate(&self) -> Result<(), GraphError>;
}

impl InvariantCheck for DialogueEvent {
    fn validate(&self) -> Result<(), GraphError> {
        if self.base.sequence_number == 0 {
            return Err(GraphError::InvalidQuery("sequence_number must be >= 1"));
        }
        if self.base.timestamp_ms < 0 {
            return Err(GraphError::InvalidQuery("timestamp must be non-negative"));
        }
        if matches!(self.base.access_class, AccessClass::Restricted)
            && self.base.provenance.is_none()
        {
            return Err(GraphError::PrivacyRestricted);
        }
        if let Some(enh) = &self.enhancements {
            if let Some(snapshot) = enh.latest() {
                if let Some(meta) = &snapshot.semantic.embedding_meta {
                    if let Some(vec) = &snapshot.semantic.content_embedding {
                        if vec.len() as u16 != meta.dim {
                            return Err(GraphError::InvalidQuery("content embedding dim mismatch"));
                        }
                    }
                    if let Some(vec) = &snapshot.semantic.context_embedding {
                        if vec.len() as u16 != meta.dim {
                            return Err(GraphError::InvalidQuery("context embedding dim mismatch"));
                        }
                    }
                    if let Some(vec) = &snapshot.semantic.decision_embedding {
                        if vec.len() as u16 != meta.dim {
                            return Err(GraphError::InvalidQuery(
                                "decision embedding dim mismatch",
                            ));
                        }
                    }
                    if let Some(vec) = &snapshot.semantic.concept_vector {
                        if vec.len() as u16 != meta.dim {
                            return Err(GraphError::InvalidQuery("concept vector dim mismatch"));
                        }
                    }
                }
            }
            if !enh.meta.append_only {
                return Err(GraphError::InvalidQuery(
                    "enhancement snapshot must be append-only",
                ));
            }
            if enh.meta.latest_version == 0 && !enh.versions.is_empty() {
                return Err(GraphError::InvalidQuery(
                    "enhancement meta.latest_version mismatch",
                ));
            }
            for pair in enh.versions.windows(2) {
                if pair[0].version >= pair[1].version {
                    return Err(GraphError::InvalidQuery(
                        "enhancement versions must be strictly increasing",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use soulseed_agi_core_models::{
        common::{
            CorrelationId, EmbeddingMeta, EnvelopeHead, Snapshot, Subject, SubjectRef, TraceId,
        },
        dialogue_event::{
            DialogueEvent, DialogueEventBase, DialogueEventEnhancements, DialogueEventPayload,
            EnhancementMeta, EnhancementSnapshot, SystemPayload,
        },
        enums::{ConversationScenario, DialogueEventType, RealTimePriority},
        ids::{EventId, HumanId, SessionId, TenantId},
        AccessClass,
    };
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn base_event() -> DialogueEventBase {
        DialogueEventBase {
            tenant_id: TenantId::from_raw_unchecked(1),
            event_id: EventId::from_raw_unchecked(2),
            session_id: SessionId::from_raw_unchecked(3),
            subject: Subject::Human(HumanId::from_raw_unchecked(4)),
            participants: vec![SubjectRef {
                kind: Subject::System,
                role: Some("system".into()),
            }],
            head: EnvelopeHead {
                envelope_id: Uuid::nil(),
                trace_id: TraceId("trace-1".into()),
                correlation_id: CorrelationId("corr-1".into()),
                config_snapshot_hash:
                    "a7f0b2044a9be4f65d8c7e1f9c2d4b5a6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f".into(),
                config_snapshot_version: 1,
            },
            snapshot: Snapshot {
                schema_v: 1,
                created_at: OffsetDateTime::UNIX_EPOCH,
            },
            timestamp_ms: 1,
            scenario: ConversationScenario::HumanToAi,
            event_type: DialogueEventType::System,
            stage_hint: None,
            access_class: AccessClass::Public,
            provenance: None,
            sequence_number: 1,
            ac_id: None,
            ic_sequence: None,
            parent_ac_id: None,
            config_version: Some("1.0.0".into()),
            trigger_event_id: None,
            supersedes: None,
            superseded_by: None,
        }
    }

    #[test]
    fn validate_rejects_embedding_dim_mismatch() {
        let mut meta = EnhancementMeta::default();
        meta.latest_version = 1;
        let mut snapshot = EnhancementSnapshot::default();
        snapshot.version = 1;
        snapshot.created_at_ms = 100;
        snapshot.semantic.embedding_meta = Some(EmbeddingMeta {
            model: "embed-meta".into(),
            dim: 4,
            ts: 10,
        });
        snapshot.semantic.content_embedding = Some(vec![0.1, 0.2]);
        snapshot.realtime.real_time_priority = Some(RealTimePriority::High);
        let mut enhancements = DialogueEventEnhancements {
            meta,
            versions: vec![snapshot],
        };
        let event_payload = DialogueEventPayload::SystemNotificationDispatched(SystemPayload {
            category: "test".into(),
            related_service: None,
            detail: Value::Null,
        });
        #[cfg(feature = "vectors-extra")]
        let event = DialogueEvent {
            base: base_event(),
            payload: event_payload,
            enhancements: Some(enhancements.clone()),
            metadata: Value::Null,
            vectors: Default::default(),
        };
        #[cfg(not(feature = "vectors-extra"))]
        let event = DialogueEvent {
            base: base_event(),
            payload: event_payload,
            enhancements: Some(enhancements.clone()),
            metadata: Value::Null,
        };
        let err = InvariantCheck::validate(&event).expect_err("dim mismatch should fail");
        assert!(matches!(
            err,
            GraphError::InvalidQuery("content embedding dim mismatch")
        ));

        enhancements.meta.append_only = false;
        if let Some(snapshot) = enhancements.versions.first_mut() {
            snapshot.semantic.content_embedding = Some(vec![0.1, 0.2, 0.3, 0.4]);
        }
        #[cfg(feature = "vectors-extra")]
        let event = DialogueEvent {
            base: base_event(),
            payload: DialogueEventPayload::SystemNotificationDispatched(SystemPayload {
                category: "test".into(),
                related_service: None,
                detail: Value::Null,
            }),
            enhancements: Some(enhancements),
            metadata: Value::Null,
            vectors: Default::default(),
        };
        #[cfg(not(feature = "vectors-extra"))]
        let event = DialogueEvent {
            base: base_event(),
            payload: DialogueEventPayload::SystemNotificationDispatched(SystemPayload {
                category: "test".into(),
                related_service: None,
                detail: Value::Null,
            }),
            enhancements: Some(enhancements),
            metadata: Value::Null,
        };
        let err = InvariantCheck::validate(&event).expect_err("append-only violation should fail");
        assert!(matches!(
            err,
            GraphError::InvalidQuery("enhancement snapshot must be append-only")
        ));
    }
}
