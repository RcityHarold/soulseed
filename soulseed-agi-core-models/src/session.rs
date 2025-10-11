use crate::{
    common::EvidencePointer, AccessClass, ConversationScenario, CorrelationId, EnvelopeHead,
    ModelError, Provenance, SessionId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub tenant_id: TenantId,
    pub session_id: SessionId,
    pub trace_id: TraceId,
    pub correlation_id: CorrelationId,
    pub subject: Subject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    pub head: EnvelopeHead,
    pub snapshot: Snapshot,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ConversationScenario>,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[cfg(feature = "strict-privacy")]
fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

impl Session {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.head.trace_id != self.trace_id {
            return Err(ModelError::Invariant(
                "session.trace_id must match head.trace_id",
            ));
        }
        if self.head.correlation_id != self.correlation_id {
            return Err(ModelError::Invariant(
                "session.correlation_id must match head.correlation_id",
            ));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }
}
