use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use soulseed_agi_context::types::ContextBundle;
use soulseed_agi_core_models::{
    ConversationScenario, AwarenessCycleId,
    awareness::{
        AwarenessAnchor, AwarenessDegradationReason, AwarenessFork, DecisionPath, DecisionPlan,
    },
};
use time::OffsetDateTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssessmentEntry {
    pub dimension: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub evidence: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AssessmentSnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<AssessmentEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_clarity: Option<f32>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextSignal {
    pub id: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ContextSignals {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<ContextSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyGate {
    pub gate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PolicySnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_gates: Vec<PolicyGate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_collabs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_collabs: Vec<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: String,
    pub fork: AwarenessFork,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_hint: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<BudgetEstimate>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CatalogSnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<CatalogItem>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetTarget {
    pub max_tokens: u32,
    pub max_walltime_ms: u32,
    pub max_external_cost: f32,
}

impl Default for BudgetTarget {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            max_walltime_ms: 0,
            max_external_cost: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterWeights {
    pub self_bias: f32,
    pub clarify_bias: f32,
    pub tool_bias: f32,
    pub collab_bias: f32,
    pub context_alignment: f32,
    pub intent_clarity: f32,
    pub risk_penalty: f32,
    pub budget_pressure: f32,
}

impl Default for RouterWeights {
    fn default() -> Self {
        Self {
            self_bias: 0.15,
            clarify_bias: 0.25,
            tool_bias: 0.35,
            collab_bias: 0.25,
            context_alignment: 0.30,
            intent_clarity: 0.20,
            risk_penalty: -0.40,
            budget_pressure: -0.30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterStickiness {
    pub cooldown_ms: u64,
    pub stickiness_bonus: f32,
    pub switch_penalty: f32,
}

impl Default for RouterStickiness {
    fn default() -> Self {
        Self {
            cooldown_ms: 5_000,
            stickiness_bonus: 0.09,
            switch_penalty: 0.18,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterConfig {
    pub digest: String,
    #[serde(default)]
    pub weights: RouterWeights,
    #[serde(default)]
    pub explore_rate: f32,
    #[serde(default)]
    pub max_candidates: usize,
    #[serde(default)]
    pub risk_threshold: f32,
    #[serde(default)]
    pub stickiness: RouterStickiness,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            digest: format!("blake3:{}", blake3::hash(b"dfr-router-config-v1")),
            weights: RouterWeights::default(),
            explore_rate: 0.05,
            max_candidates: 3,
            risk_threshold: 0.75,
            stickiness: RouterStickiness::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterCandidate {
    pub decision_plan: DecisionPlan,
    pub fork: AwarenessFork,
    pub priority: f32,
    #[serde(default)]
    pub metadata: Value,
}

impl RouterCandidate {
    pub fn label(&self) -> String {
        self.metadata
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{:?}", self.fork))
    }

    pub fn ensure_metadata_object(&mut self) -> &mut Map<String, Value> {
        if !self.metadata.is_object() {
            self.metadata = Value::Object(Map::new());
        }
        self.metadata
            .as_object_mut()
            .expect("metadata should be an object")
    }
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
    #[serde(default)]
    pub assessment: AssessmentSnapshot,
    #[serde(default)]
    pub context_signals: ContextSignals,
    #[serde(default)]
    pub policies: PolicySnapshot,
    #[serde(default)]
    pub catalogs: CatalogSnapshot,
    #[serde(default)]
    pub budget: BudgetTarget,
    #[serde(default)]
    pub router_config: RouterConfig,
    #[serde(default)]
    pub routing_seed: u64,
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
    pub cycle_id: AwarenessCycleId,
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

pub fn fork_key(fork: AwarenessFork) -> &'static str {
    match fork {
        AwarenessFork::SelfReason => "self",
        AwarenessFork::Clarify => "clarify",
        AwarenessFork::ToolPath => "tool",
        AwarenessFork::Collab => "collab",
    }
}

pub fn new_cycle_id() -> AwarenessCycleId {
    AwarenessCycleId::generate()
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
            "timeout_fallback" => Some(AwarenessDegradationReason::BudgetWalltime),
            "requires_follow_up" => None,
            "llm_timeout_recovered" => Some(AwarenessDegradationReason::BudgetWalltime),
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
