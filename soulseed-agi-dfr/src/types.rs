use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_context::types::ContextBundle;
use soulseed_agi_core_models::{
    ConversationScenario, CycleId,
    awareness::{
        AwarenessAnchor, AwarenessDegradationReason, AwarenessFork, DecisionPath, DecisionPlan,
    },
};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterCandidate {
    pub decision_plan: DecisionPlan,
    pub fork: AwarenessFork,
    pub priority: f32,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FilterOutcome {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted: Vec<RouterCandidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterInput {
    pub anchor: AwarenessAnchor,
    pub context: ContextBundle,
    pub context_digest: String,
    pub scenario: ConversationScenario,
    pub scene_label: String,
    pub user_prompt: String,
    #[serde(default)]
    pub tags: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetEstimate {
    pub tokens: u32,
    pub walltime_ms: u32,
    pub external_cost: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteExplain {
    pub routing_seed: u64,
    pub router_digest: String,
    pub router_config_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indices_used: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub diagnostics: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutePlan {
    pub cycle_id: CycleId,
    pub anchor: AwarenessAnchor,
    pub fork: AwarenessFork,
    pub decision_plan: DecisionPlan,
    pub budget: BudgetEstimate,
    pub priority: f32,
    pub explain: RouteExplain,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterDecision {
    pub plan: RoutePlan,
    pub decision_path: DecisionPath,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<(String, String)>,
    pub context_digest: String,
    pub issued_at: OffsetDateTime,
}

pub fn new_cycle_id() -> CycleId {
    CycleId(Uuid::now_v7().as_u128() as u64)
}

pub fn map_degradation(reason: &str) -> Option<AwarenessDegradationReason> {
    for segment in reason.split('|') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        let direct = match trimmed {
            "privacy_blocked" => Some(AwarenessDegradationReason::PrivacyBlocked),
            "invalid_plan" => Some(AwarenessDegradationReason::InvalidPlan),
            "clarify_exhausted" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "clarify_timeout" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "clarify_conflict" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "timeout_fallback" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "requires_follow_up" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "llm_timeout_recovered" => Some(AwarenessDegradationReason::ClarifyExhausted),
            "graph_degraded" => Some(AwarenessDegradationReason::GraphDegraded),
            "stale_fact" => Some(AwarenessDegradationReason::GraphDegraded),
            "envctx_degraded" => Some(AwarenessDegradationReason::EnvctxDegraded),
            "budget_tokens" => Some(AwarenessDegradationReason::BudgetTokens),
            "budget_walltime" => Some(AwarenessDegradationReason::BudgetWalltime),
            "budget_external_cost" => Some(AwarenessDegradationReason::BudgetExternalCost),
            "tool_timeout" => Some(AwarenessDegradationReason::BudgetWalltime),
            "empty_catalog" => Some(AwarenessDegradationReason::EmptyCatalog),
            _ => None,
        };

        if direct.is_some() {
            return direct;
        }

        if let Some((prefix, code)) = trimmed.split_once(':') {
            let mapped = match (prefix, code) {
                ("envctx", _) => Some(AwarenessDegradationReason::EnvctxDegraded),
                ("graph", _) => Some(AwarenessDegradationReason::GraphDegraded),
                ("budget", "tokens") => Some(AwarenessDegradationReason::BudgetTokens),
                ("budget", "walltime") => Some(AwarenessDegradationReason::BudgetWalltime),
                ("budget", "external_cost") => Some(AwarenessDegradationReason::BudgetExternalCost),
                ("privacy", _) => Some(AwarenessDegradationReason::PrivacyBlocked),
                ("clarify", _) => Some(AwarenessDegradationReason::ClarifyExhausted),
                _ => None,
            };
            if mapped.is_some() {
                return mapped;
            }
        }
    }

    None
}
