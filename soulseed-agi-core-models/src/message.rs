#[cfg(feature = "vectors-extra")]
use crate::vectors_ext::ExtraVectors;
use crate::{
    common::EvidencePointer, AccessClass, EnvelopeHead, MessageId, ModelError, Provenance,
    SessionId, Snapshot, Subject, SubjectRef, TenantId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub tenant_id: TenantId,
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub head: EnvelopeHead,
    pub snapshot: Snapshot,
    pub timestamp_ms: i64,
    pub sender: Subject,
    pub content_type: String,
    pub content: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest_sha256: Option<String>,
    #[cfg(feature = "vectors-extra")]
    #[serde(default)]
    pub vectors: ExtraVectors,
}

impl Message {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.timestamp_ms < 0 {
            return Err(ModelError::Invariant("timestamp_ms must be non-negative"));
        }
        if self.content_type.trim().is_empty() {
            return Err(ModelError::Missing("content_type"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        if let Some(seq) = self.sequence_number {
            if seq == 0 {
                return Err(ModelError::Invariant("sequence_number must be >= 1"));
            }
        }
        Ok(())
    }
}
