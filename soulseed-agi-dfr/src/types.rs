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

// ============================================================================
// P2: Sticky 决策优化 - StickyDecision 相关类型
// ============================================================================

/// Sticky 决策的持续时长策略
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum StickyDuration {
    /// 直到成功完成才解除
    UntilSuccess,
    /// 持续 N 轮对话
    ForNTurns(u32),
    /// 直到满足特定条件（条件表达式）
    UntilCondition(String),
    /// 无时限，需要手动解除
    Indefinite,
}

impl Default for StickyDuration {
    fn default() -> Self {
        Self::UntilSuccess
    }
}

/// Sticky 决策配置
/// 用于指定某个决策路径应该"粘住"多长时间，避免频繁切换
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StickyDecision {
    /// 粘性持续时长
    pub duration: StickyDuration,
    /// 是否允许在粘性期间进行微调（refinement）
    #[serde(default)]
    pub refinement_allowed: bool,
    /// 如果粘性决策失败，回退到的路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_path: Option<DecisionPath>,
    /// 粘性开始时间戳（毫秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// 剩余轮数（仅当 duration 为 ForNTurns 时有效）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_turns: Option<u32>,
    /// 粘性原因说明
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Default for StickyDecision {
    fn default() -> Self {
        Self {
            duration: StickyDuration::default(),
            refinement_allowed: false,
            fallback_path: None,
            started_at_ms: None,
            remaining_turns: None,
            reason: None,
        }
    }
}

impl StickyDecision {
    /// 创建一个持续到成功的粘性决策
    pub fn until_success() -> Self {
        Self {
            duration: StickyDuration::UntilSuccess,
            ..Default::default()
        }
    }

    /// 创建一个持续 N 轮的粘性决策
    pub fn for_turns(n: u32) -> Self {
        Self {
            duration: StickyDuration::ForNTurns(n),
            remaining_turns: Some(n),
            ..Default::default()
        }
    }

    /// 创建一个基于条件的粘性决策
    pub fn until_condition(condition: impl Into<String>) -> Self {
        Self {
            duration: StickyDuration::UntilCondition(condition.into()),
            ..Default::default()
        }
    }

    /// 设置是否允许微调
    pub fn with_refinement(mut self, allowed: bool) -> Self {
        self.refinement_allowed = allowed;
        self
    }

    /// 设置回退路径
    pub fn with_fallback(mut self, path: DecisionPath) -> Self {
        self.fallback_path = Some(path);
        self
    }

    /// 设置原因
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// 检查粘性是否已过期
    pub fn is_expired(&self) -> bool {
        match &self.duration {
            StickyDuration::UntilSuccess => false,
            StickyDuration::ForNTurns(_) => {
                self.remaining_turns.map(|r| r == 0).unwrap_or(true)
            }
            StickyDuration::UntilCondition(_) => false, // 需要外部评估
            StickyDuration::Indefinite => false,
        }
    }

    /// 消耗一轮（仅对 ForNTurns 有效）
    pub fn consume_turn(&mut self) {
        if let StickyDuration::ForNTurns(_) = &self.duration {
            if let Some(ref mut remaining) = self.remaining_turns {
                *remaining = remaining.saturating_sub(1);
            }
        }
    }
}

// ============================================================================
// P2: alternatives_considered 完整记录
// ============================================================================

/// 被考虑但未选择的备选方案
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlternativeConsidered {
    /// 备选方案的决策路径
    pub path: DecisionPath,
    /// 评分
    pub score: f32,
    /// 拒绝原因
    pub rejection_reason: String,
    /// 如果选择此路径需要满足的条件
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub would_have_required: Vec<String>,
    /// 与选中方案的分数差距
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_delta: Option<f32>,
    /// 风险评估
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_assessment: Option<f32>,
    /// 额外元数据
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

impl AlternativeConsidered {
    pub fn new(path: DecisionPath, score: f32, rejection_reason: impl Into<String>) -> Self {
        Self {
            path,
            score,
            rejection_reason: rejection_reason.into(),
            would_have_required: Vec::new(),
            score_delta: None,
            risk_assessment: None,
            metadata: Value::Null,
        }
    }

    pub fn with_requirements(mut self, requirements: Vec<String>) -> Self {
        self.would_have_required = requirements;
        self
    }

    pub fn with_score_delta(mut self, delta: f32) -> Self {
        self.score_delta = Some(delta);
        self
    }

    pub fn with_risk(mut self, risk: f32) -> Self {
        self.risk_assessment = Some(risk);
        self
    }
}

// ============================================================================
// P2: 决策 rationale 增强
// ============================================================================

/// 证据引用
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// 证据类型 (context_signal, user_input, history, policy, etc.)
    pub evidence_type: String,
    /// 证据来源标识
    pub source_id: String,
    /// 证据描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 证据权重
    #[serde(default)]
    pub weight: f32,
    /// 原始证据数据
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

impl EvidenceRef {
    pub fn new(evidence_type: impl Into<String>, source_id: impl Into<String>) -> Self {
        Self {
            evidence_type: evidence_type.into(),
            source_id: source_id.into(),
            description: None,
            weight: 1.0,
            data: Value::Null,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

/// 置信度因素
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfidenceFactor {
    /// 因素名称
    pub name: String,
    /// 贡献值（正为增加置信度，负为降低）
    pub contribution: f32,
    /// 因素说明
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

impl ConfidenceFactor {
    pub fn positive(name: impl Into<String>, contribution: f32) -> Self {
        Self {
            name: name.into(),
            contribution: contribution.abs(),
            explanation: None,
        }
    }

    pub fn negative(name: impl Into<String>, contribution: f32) -> Self {
        Self {
            name: name.into(),
            contribution: -contribution.abs(),
            explanation: None,
        }
    }

    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = Some(explanation.into());
        self
    }
}

/// 潜在风险
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRisk {
    /// 风险类型
    pub risk_type: String,
    /// 风险等级 (0.0 - 1.0)
    pub severity: f32,
    /// 发生概率 (0.0 - 1.0)
    pub probability: f32,
    /// 风险描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 缓解措施
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mitigations: Vec<String>,
}

impl DecisionRisk {
    pub fn new(risk_type: impl Into<String>, severity: f32, probability: f32) -> Self {
        Self {
            risk_type: risk_type.into(),
            severity: severity.clamp(0.0, 1.0),
            probability: probability.clamp(0.0, 1.0),
            description: None,
            mitigations: Vec::new(),
        }
    }

    /// 计算风险分数 (severity * probability)
    pub fn risk_score(&self) -> f32 {
        self.severity * self.probability
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_mitigations(mut self, mitigations: Vec<String>) -> Self {
        self.mitigations = mitigations;
        self
    }
}

/// 结构化决策理由
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DecisionRationale {
    /// 主要原因
    pub primary_reason: String,
    /// 支持证据
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supporting_evidence: Vec<EvidenceRef>,
    /// 置信度因素
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_factors: Vec<ConfidenceFactor>,
    /// 潜在风险
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub potential_risks: Vec<DecisionRisk>,
    /// 总体置信度
    #[serde(default)]
    pub overall_confidence: f32,
    /// 额外说明
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_notes: Option<String>,
}

impl DecisionRationale {
    pub fn new(primary_reason: impl Into<String>) -> Self {
        Self {
            primary_reason: primary_reason.into(),
            overall_confidence: 0.5,
            ..Default::default()
        }
    }

    pub fn with_evidence(mut self, evidence: Vec<EvidenceRef>) -> Self {
        self.supporting_evidence = evidence;
        self
    }

    pub fn with_confidence_factors(mut self, factors: Vec<ConfidenceFactor>) -> Self {
        self.confidence_factors = factors;
        self.recalculate_confidence();
        self
    }

    pub fn with_risks(mut self, risks: Vec<DecisionRisk>) -> Self {
        self.potential_risks = risks;
        self
    }

    /// 根据置信度因素重新计算总体置信度
    fn recalculate_confidence(&mut self) {
        if self.confidence_factors.is_empty() {
            return;
        }
        let base = 0.5f32;
        let total_contribution: f32 = self.confidence_factors.iter()
            .map(|f| f.contribution)
            .sum();
        self.overall_confidence = (base + total_contribution).clamp(0.0, 1.0);
    }

    /// 获取最高风险
    pub fn highest_risk(&self) -> Option<&DecisionRisk> {
        self.potential_risks.iter()
            .max_by(|a, b| a.risk_score().partial_cmp(&b.risk_score()).unwrap_or(std::cmp::Ordering::Equal))
    }
}

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
