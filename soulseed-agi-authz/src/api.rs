use serde::{Deserialize, Serialize};

use crate::{AccessTicket, Action, Anchor, Decision, QuotaCost, ResourceUrn, Role, Subject};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthzRequest {
    pub anchor: Anchor,
    pub subject: Subject,
    pub roles: Vec<Role>,
    pub resource: ResourceUrn,
    pub action: Action,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub want_trace_full: bool,
    #[serde(default)]
    pub access_ticket: Option<AccessTicket>,
    #[serde(default)]
    pub quota_cost: Option<QuotaCost>,
    #[serde(default)]
    pub idem_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthzResponse {
    pub decision: Decision,
}
