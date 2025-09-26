use crate::{
    AccessClass, ConversationScenario, EnvelopeHead, ModelError, Provenance, SessionId, Snapshot,
    Subject, SubjectRef, TenantId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub tenant_id: TenantId,
    pub session_id: SessionId,
    pub subject: Subject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    pub head: EnvelopeHead,
    pub snapshot: Snapshot,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ConversationScenario>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_class: Option<AccessClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

impl Session {
    pub fn validate(&self) -> Result<(), ModelError> {
        if let Some(access) = self.access_class {
            if matches!(access, AccessClass::Restricted) && self.provenance.is_none() {
                return Err(ModelError::Missing("provenance"));
            }
        }
        Ok(())
    }
}
