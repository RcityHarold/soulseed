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

#[derive(Clone, Debug)]
pub struct PredicateEvaluation {
    pub results: Vec<(String, bool)>,
    pub all_true: bool,
    pub missing_consent: Option<String>,
    pub quota_hint: Option<String>,
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
    OwnsResource,
    QuotaAvailable {
        key: String,
        minimum: i64,
    },
    RiskBand {
        min: u8,
        max: u8,
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
    AuditTrail,
    ConsentPrompt(String),
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
            Obligation::AuditTrail => "audit_trail".into(),
            Obligation::ConsentPrompt(kind) => format!("consent_prompt:{kind}"),
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
    ) -> PredicateEvaluation {
        let mut results = Vec::new();
        let mut all_true = true;
        let mut missing_consent = None;
        let mut quota_hint = None;
        let subject_owner = match subject {
            Subject::Human(h) => Some(h.as_u64()),
            _ => None,
        };
        let context_owner = context.get("owner_id").and_then(|v| v.as_u64());
        let context_quota = context.get("quota");
        let risk_level = context
            .get("risk_level")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(0);
        for pred in &self.conditions {
            let (label, passed) = match pred {
                Predicate::TenantEq => ("TenantEq".into(), true),
                Predicate::MemberOf(group) => {
                    let groups = context
                        .get("groups")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    (format!("MemberOf({})", group.as_u64()), groups.contains(&group.as_u64()))
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
                Predicate::OwnsResource => {
                    let owns = subject_owner
                        .zip(context_owner)
                        .map(|(sub, owner)| sub == owner)
                        .unwrap_or(false);
                    ("OwnsResource".into(), owns)
                }
                Predicate::RiskAtMost { level } => {
                    let risk = context
                        .get("risk_level")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    (format!("RiskAtMost({})", level), risk as u8 <= *level)
                }
                Predicate::RiskBand { min, max } => (
                    format!("RiskBand({min}-{max})"),
                    risk_level >= *min && risk_level <= *max,
                ),
                Predicate::TimeWindow { start_ms, end_ms } => {
                    let now = context
                        .get("time_ms")
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| OffsetDateTime::now_utc().unix_timestamp() * 1000);
                    ("TimeWindow".into(), now >= *start_ms && now <= *end_ms)
                }
                Predicate::QuotaAvailable { key, minimum } => {
                    let available = context_quota
                        .and_then(|quota| quota.get(key))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if available < *minimum {
                        quota_hint = Some(format!("quota:{}<{}", key, minimum));
                    }
                    (
                        format!("QuotaAvailable({key}>={minimum})"),
                        available >= *minimum,
                    )
                }
                Predicate::Custom { name, value: _ } => (format!("Custom({})", name), true),
            };
            results.push((label, passed));
            if !passed {
                all_true = false;
                if matches!(
                    pred,
                    Predicate::HasConsent { .. } | Predicate::QuotaAvailable { .. }
                ) {
                    if let Predicate::HasConsent { kind } = pred {
                        missing_consent = Some(kind.clone());
                    }
                }
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
        PredicateEvaluation {
            results,
            all_true,
            missing_consent,
            quota_hint,
        }
    }
}
