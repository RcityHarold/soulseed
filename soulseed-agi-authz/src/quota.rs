use serde::{Deserialize, Serialize};

use crate::{scenario_slug, Action, ConversationScenario, Effect, ResourceUrn, Subject, TenantId};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct QuotaCost {
    pub items: Vec<(String, i64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaRequest {
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub resource: ResourceUrn,
    pub action: Action,
    pub cost: QuotaCost,
    pub envelope_id: uuid::Uuid,
    pub idem_key: Option<String>,
    pub time_ms: i64,
    pub context: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaDecision {
    pub effect: Effect,
    pub reason: String,
    pub detail: serde_json::Value,
}

pub trait QuotaClient: Send + Sync {
    fn check_and_consume(&self, request: &QuotaRequest) -> QuotaDecision;
}

#[derive(Default)]
pub struct ScenarioQuotaClient {
    idem: Mutex<HashSet<String>>,
    tool_budget: Mutex<HashMap<TenantId, i64>>,
    clarify_counts: Mutex<HashMap<(TenantId, u64), u32>>,
}

impl ScenarioQuotaClient {
    const TOOL_BUDGET: i64 = 100;
    const CLARIFY_LIMIT: u32 = 3;
    const CLARIFY_CONCURRENCY_LIMIT: u32 = 2;
    const COLLAB_PARTICIPANT_LIMIT: u32 = 8;

    pub fn new() -> Self {
        Self::default()
    }

    fn idem_key(request: &QuotaRequest) -> String {
        request
            .idem_key
            .clone()
            .unwrap_or_else(|| format!("{}:{}", request.tenant_id.as_u64(), request.envelope_id))
    }

    fn parse_dialogue<'a>(urn: &'a ResourceUrn) -> Option<(u64, Option<&'a str>, Vec<&'a str>)> {
        let raw = urn.0.as_str();
        if !raw.starts_with("urn:soulseed:dialogue:session:") {
            return None;
        }
        let tail = &raw["urn:soulseed:dialogue:session:".len()..];
        let mut colon_split = tail.splitn(2, ':');
        let session_part = colon_split.next()?;
        let session_id = session_part.parse::<u64>().ok()?;
        let mut remainder: Vec<&str> = colon_split
            .next()
            .unwrap_or_default()
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();

        let mut scenario = None;
        if remainder.len() >= 2 && remainder[0].starts_with("scenario") {
            scenario = remainder.get(1).copied();
            remainder.drain(0..2);
        }

        Some((session_id, scenario, remainder))
    }

    fn scenario_slug_from_resource(resource: &ResourceUrn) -> Option<String> {
        Self::parse_dialogue(resource).and_then(|(_, scenario, _)| scenario.map(|s| s.to_string()))
    }

    fn is_self_reflection(resource: &ResourceUrn, action: &Action, context: &Value) -> bool {
        if !matches!(action, Action::AppendDialogueEvent) {
            return false;
        }
        let resource_slug = Self::scenario_slug_from_resource(resource);
        let context_slug = context
            .get("scene")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let ai_self_slug = scenario_slug(ConversationScenario::AiSelfTalk);
        resource_slug.as_deref() == Some(ai_self_slug)
            || context_slug.as_deref() == Some(ai_self_slug)
    }

    fn is_tool_event(action: &Action) -> bool {
        matches!(action, Action::EmitToolEvent)
    }

    fn is_clarify(context: &Value) -> bool {
        context
            .get("stage")
            .and_then(Value::as_str)
            .map(|s| s.eq_ignore_ascii_case("clarify"))
            .unwrap_or(false)
            || context
                .get("clarify")
                .and_then(Value::as_bool)
                .unwrap_or(false)
    }

    fn clarify_pending(context: &Value) -> Option<u32> {
        context
            .get("clarify_pending")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
    }

    fn collab_participants(context: &Value) -> Option<u32> {
        context
            .get("collab_participants")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
    }
}

impl QuotaClient for ScenarioQuotaClient {
    fn check_and_consume(&self, request: &QuotaRequest) -> QuotaDecision {
        let idem = Self::idem_key(request);
        {
            let mut guard = self.idem.lock().unwrap();
            if guard.contains(&idem) {
                return QuotaDecision {
                    effect: Effect::Allow,
                    reason: "idempotent".into(),
                    detail: json!({"reason": "idempotent"}),
                };
            }
            guard.insert(idem);
        }

        if request
            .context
            .get("requires_consent")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return QuotaDecision {
                effect: Effect::AskConsent,
                reason: "consent_required".into(),
                detail: json!({"reason": "consent_required"}),
            };
        }

        if Self::is_self_reflection(&request.resource, &request.action, &request.context) {
            return QuotaDecision {
                effect: Effect::Allow,
                reason: "self_reflection_fastlane".into(),
                detail: json!({"reason": "self_reflection_fastlane"}),
            };
        }

        if Self::is_clarify(&request.context) {
            if let Some(pending) = Self::clarify_pending(&request.context) {
                if pending >= Self::CLARIFY_CONCURRENCY_LIMIT {
                    return QuotaDecision {
                        effect: Effect::Degrade,
                        reason: "clarify_concurrency_limit".into(),
                        detail: json!({
                            "reason": "clarify_concurrency_limit",
                            "pending": pending,
                        }),
                    };
                }
            }
        }

        if Self::is_tool_event(&request.action) {
            let mut guard = self.tool_budget.lock().unwrap();
            let entry = guard.entry(request.tenant_id).or_default();
            let spend: i64 = request.cost.items.iter().map(|(_, v)| *v).sum();
            *entry += spend.max(0);
            if let Some(participants) = Self::collab_participants(&request.context) {
                if participants > Self::COLLAB_PARTICIPANT_LIMIT {
                    return QuotaDecision {
                        effect: Effect::Degrade,
                        reason: "collab_size_exceeded".into(),
                        detail: json!({
                            "reason": "collab_size_exceeded",
                            "participants": participants,
                        }),
                    };
                }
            }
            if *entry > Self::TOOL_BUDGET {
                return QuotaDecision {
                    effect: Effect::Degrade,
                    reason: "tool_budget_exceeded".into(),
                    detail: json!({"reason": "tool_budget_exceeded", "spend": *entry}),
                };
            }
            return QuotaDecision {
                effect: Effect::Allow,
                reason: "tool_budget_allow".into(),
                detail: json!({"reason": "tool_budget_allow", "spend": *entry}),
            };
        }

        if matches!(request.action, Action::AppendDialogueEvent)
            && Self::is_clarify(&request.context)
        {
            if let Some((session_id, _, _)) = Self::parse_dialogue(&request.resource) {
                let mut guard = self.clarify_counts.lock().unwrap();
                let entry = guard.entry((request.tenant_id, session_id)).or_default();
                *entry += 1;
                if *entry > Self::CLARIFY_LIMIT {
                    return QuotaDecision {
                        effect: Effect::Degrade,
                        reason: "clarify_retry_limit".into(),
                        detail: json!({"reason": "clarify_retry_limit", "count": *entry}),
                    };
                }
            }
            return QuotaDecision {
                effect: Effect::Allow,
                reason: "clarify_count".into(),
                detail: json!({"reason": "clarify_count"}),
            };
        }

        QuotaDecision {
            effect: Effect::Allow,
            reason: "quota_passthrough".into(),
            detail: json!({"reason": "passthrough"}),
        }
    }
}
