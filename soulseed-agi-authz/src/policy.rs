use serde::{Deserialize, Serialize};

use crate::{Action, Anchor, Effect, ResourceUrn, Role, Subject};
use time::OffsetDateTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub id: String,
    pub schema_v: u16,
    pub version: u32,
    pub parent: Option<String>,
    pub priority: i32,
    pub subject_roles: Vec<Role>,
    pub subject_attrs: serde_json::Value,
    pub resource_urn: ResourceUrn,
    pub actions: Vec<Action>,
    pub effect: Effect,
    pub conditions: Vec<Predicate>,
    pub obligations: Vec<Obligation>,
    pub policy_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Predicate {
    TenantEq,
    MemberOf(crate::GroupId),
    SceneIn(Vec<String>),
    HasConsent {
        kind: String,
    },
    RiskAtMost {
        level: u8,
    },
    TimeWindow {
        start_ms: i64,
        end_ms: i64,
    },
    Custom {
        name: String,
        value: serde_json::Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Obligation {
    MaskFields(Vec<String>),
    SummaryOnly,
    RedactReasoning,
    TtlSeconds(u32),
    LogExplain,
    AttachPolicyDigest,
    Custom(String),
}

impl Obligation {
    pub fn as_label(&self) -> String {
        match self {
            Obligation::MaskFields(fields) => format!("mask_fields:{}", fields.join(",")),
            Obligation::SummaryOnly => "summary_only".into(),
            Obligation::RedactReasoning => "redact_reasoning".into(),
            Obligation::TtlSeconds(ttl) => format!("ttl={}", ttl),
            Obligation::LogExplain => "log_explain".into(),
            Obligation::AttachPolicyDigest => "attach_policy_digest".into(),
            Obligation::Custom(name) => format!("custom:{}", name),
        }
    }
}

impl Policy {
    pub fn applies_to(&self, resource: &ResourceUrn, action: &Action) -> bool {
        if !self.actions.iter().any(|a| a == action) {
            return false;
        }
        resource.matches(&self.resource_urn)
    }

    pub fn roles_match(&self, roles: &[Role]) -> bool {
        if self.subject_roles.is_empty() {
            return true;
        }
        self.subject_roles.iter().any(|role| roles.contains(role))
    }

    pub fn evaluate_predicates(
        &self,
        anchor: &Anchor,
        subject: &Subject,
        context: &serde_json::Value,
    ) -> (Vec<(String, bool)>, bool) {
        let mut results = Vec::new();
        let mut all_true = true;
        for pred in &self.conditions {
            let (label, passed) = match pred {
                Predicate::TenantEq => ("TenantEq".into(), true),
                Predicate::MemberOf(group) => {
                    let groups = context
                        .get("groups")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    (format!("MemberOf({})", group.0), groups.contains(&group.0))
                }
                Predicate::SceneIn(list) => {
                    let scene = context.get("scene").and_then(|v| v.as_str()).unwrap_or("");
                    (format!("SceneIn"), list.iter().any(|v| v == scene))
                }
                Predicate::HasConsent { kind } => {
                    let consents = context
                        .get("consents")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    (
                        format!("HasConsent({})", kind),
                        consents.iter().any(|c| c == kind),
                    )
                }
                Predicate::RiskAtMost { level } => {
                    let risk = context
                        .get("risk_level")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    (format!("RiskAtMost({})", level), risk as u8 <= *level)
                }
                Predicate::TimeWindow { start_ms, end_ms } => {
                    let now = context
                        .get("time_ms")
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp() * 1000);
                    ("TimeWindow".into(), now >= *start_ms && now <= *end_ms)
                }
                Predicate::Custom { name, value: _ } => (format!("Custom({})", name), true),
            };
            results.push((label, passed));
            if !passed {
                all_true = false;
            }
        }
        // Privacy red line: Restricted 时必须含 provenance
        if anchor.access_class == crate::AccessClass::Restricted && anchor.provenance.is_none() {
            results.push(("RestrictedHasProvenance".into(), false));
            all_true = false;
        }
        match subject {
            Subject::Human(_) | Subject::AI(_) | Subject::System | Subject::Group(_) => {}
        }
        (results, all_true)
    }
}
