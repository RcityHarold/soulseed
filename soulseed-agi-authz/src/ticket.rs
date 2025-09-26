use serde::{Deserialize, Serialize};

use crate::{Action, ResourceUrn, Subject, TenantId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTicket {
    pub ticket_id: String,
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub approver: Subject,
    pub purpose: String,
    pub scope: ResourceUrn,
    pub actions: Vec<Action>,
    pub policy_digest: String,
    pub issued_at_ms: i64,
    pub ttl_ms: i64,
}

impl AccessTicket {
    pub fn is_valid(
        &self,
        now_ms: i64,
        request_subject: &Subject,
        resource: &ResourceUrn,
        action: &Action,
        policy_digest: &str,
    ) -> bool {
        if now_ms > self.issued_at_ms + self.ttl_ms {
            return false;
        }
        if &self.subject != request_subject {
            return false;
        }
        if !resource.matches(&self.scope) {
            return false;
        }
        if !self.actions.iter().any(|a| a == action) {
            return false;
        }
        if self.policy_digest != policy_digest {
            return false;
        }
        true
    }
}
