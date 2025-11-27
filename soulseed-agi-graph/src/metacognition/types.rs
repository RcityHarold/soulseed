//! 元认知工具通用类型定义
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 分析模式
/// 依据文档: UnifiedMetacognitiveAnalyzer 的五大分析模式
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    /// 轨迹观察 - 追踪认知轨迹
    TrajectoryObservation,
    /// 因果决策分析 - 分析决策因果链
    CausalDecisionAnalysis,
    /// 交互模式发现 - 发现交互模式
    InteractionPatternDiscovery,
    /// 协作评估 - 评估协作效果
    CollaborationEvaluation,
    /// 认知成长追踪 - 追踪认知成长
    CognitiveGrowthTracking,
}

/// 时间范围
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TimeRange {
    /// 开始时间（毫秒时间戳）
    pub start_ms: i64,
    /// 结束时间（毫秒时间戳）
    pub end_ms: i64,
}

impl TimeRange {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }

    pub fn last_hours(hours: i64) -> Self {
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        Self {
            start_ms: now - hours * 3600 * 1000,
            end_ms: now,
        }
    }

    pub fn last_days(days: i64) -> Self {
        Self::last_hours(days * 24)
    }
}

/// 分析请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// 分析模式
    pub mode: AnalysisMode,
    /// 租户 ID
    pub tenant_id: u64,
    /// 目标实体 ID (Actor, Session, AC 等)
    pub target_id: Option<String>,
    /// 时间范围
    pub time_range: Option<TimeRange>,
    /// 深度限制
    pub depth_limit: Option<u32>,
    /// 额外参数
    pub params: HashMap<String, serde_json::Value>,
}

impl AnalysisRequest {
    pub fn new(mode: AnalysisMode, tenant_id: u64) -> Self {
        Self {
            mode,
            tenant_id,
            target_id: None,
            time_range: None,
            depth_limit: None,
            params: HashMap::new(),
        }
    }

    pub fn with_target(mut self, target_id: impl Into<String>) -> Self {
        self.target_id = Some(target_id.into());
        self
    }

    pub fn with_time_range(mut self, range: TimeRange) -> Self {
        self.time_range = Some(range);
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth_limit = Some(depth);
        self
    }

    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }
}

/// 分析结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// 分析模式
    pub mode: AnalysisMode,
    /// 成功标志
    pub success: bool,
    /// 结果数据
    pub data: serde_json::Value,
    /// 摘要
    pub summary: Option<String>,
    /// 洞察列表
    pub insights: Vec<Insight>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AnalysisResult {
    pub fn success(mode: AnalysisMode, data: serde_json::Value) -> Self {
        Self {
            mode,
            success: true,
            data,
            summary: None,
            insights: Vec::new(),
            execution_time_ms: 0,
            metadata: HashMap::new(),
        }
    }

    pub fn failure(mode: AnalysisMode, error: impl Into<String>) -> Self {
        Self {
            mode,
            success: false,
            data: serde_json::json!({ "error": error.into() }),
            summary: None,
            insights: Vec::new(),
            execution_time_ms: 0,
            metadata: HashMap::new(),
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_insight(mut self, insight: Insight) -> Self {
        self.insights.push(insight);
        self
    }

    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = time_ms;
        self
    }
}

/// 洞察
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Insight {
    /// 洞察类型
    pub insight_type: InsightType,
    /// 标题
    pub title: String,
    /// 描述
    pub description: String,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 重要性 (0.0 - 1.0)
    pub importance: f32,
    /// 相关实体 ID
    pub related_entities: Vec<String>,
    /// 建议的行动
    pub suggested_actions: Vec<String>,
}

/// 洞察类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InsightType {
    /// 性能瓶颈
    PerformanceBottleneck,
    /// 决策模式
    DecisionPattern,
    /// 异常行为
    AnomalyDetected,
    /// 改进建议
    ImprovementSuggestion,
    /// 协作问题
    CollaborationIssue,
    /// 成长机会
    GrowthOpportunity,
    /// 风险预警
    RiskWarning,
}

// ============================================================================
// 六大核心节点类型
// 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
// ============================================================================

/// Actor 节点 - 参与者实体
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorNode {
    pub actor_id: String,
    pub actor_type: ActorType,
    pub name: Option<String>,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Actor 类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    Human,
    AI,
    System,
}

/// Event 节点 - 对话事件
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventNode {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at_ms: i64,
    pub session_id: String,
    pub actor_id: String,
    pub payload_digest: Option<String>,
}

/// Message 节点 - 消息内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageNode {
    pub message_id: String,
    pub content_digest: String,
    pub channel: Option<String>,
    pub created_at_ms: i64,
}

/// Session 节点 - 会话上下文
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionNode {
    pub session_id: String,
    pub scenario: Option<String>,
    pub mode: String,
    pub created_at_ms: i64,
    pub orchestration_id: Option<String>,
}

/// AC 节点 - 觉知周期
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ACNode {
    pub ac_id: String,
    pub session_id: String,
    pub lane: String,
    pub status: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub ic_count: u32,
    pub total_cost: f64,
}

/// Artifact 节点 - 生成物
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactNode {
    pub artifact_id: String,
    pub artifact_type: String,
    pub content_digest: String,
    pub created_at_ms: i64,
    pub created_by_ac_id: Option<String>,
}

// ============================================================================
// 七大关系边族群
// 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
// ============================================================================

/// 边类型枚举
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeType {
    // 因果边族群
    TriggeredBy,
    CausedBy,
    Enabled,
    LedTo,

    // 归属边族群
    BelongsTo,
    PartOf,
    MemberOf,

    // 引用边族群
    References,
    Mentions,
    RespondsTo,

    // 协作边族群
    CollaboratesWith,
    RequestsHelpFrom,
    Assists,

    // 版本边族群
    Supersedes,
    DerivedFrom,
    EvolvedInto,

    // 影响边族群
    Influences,
    Impacts,
    Affects,

    // 评价边族群
    Evaluates,
    Rates,
    Critiques,
}

impl EdgeType {
    /// 获取边族群
    pub fn family(&self) -> EdgeFamily {
        match self {
            Self::TriggeredBy | Self::CausedBy | Self::Enabled | Self::LedTo => EdgeFamily::Causal,
            Self::BelongsTo | Self::PartOf | Self::MemberOf => EdgeFamily::Belonging,
            Self::References | Self::Mentions | Self::RespondsTo => EdgeFamily::Reference,
            Self::CollaboratesWith | Self::RequestsHelpFrom | Self::Assists => {
                EdgeFamily::Collaboration
            }
            Self::Supersedes | Self::DerivedFrom | Self::EvolvedInto => EdgeFamily::Version,
            Self::Influences | Self::Impacts | Self::Affects => EdgeFamily::Influence,
            Self::Evaluates | Self::Rates | Self::Critiques => EdgeFamily::Evaluation,
        }
    }
}

/// 边族群
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeFamily {
    /// 因果边族群
    Causal,
    /// 归属边族群
    Belonging,
    /// 引用边族群
    Reference,
    /// 协作边族群
    Collaboration,
    /// 版本边族群
    Version,
    /// 影响边族群
    Influence,
    /// 评价边族群
    Evaluation,
}

/// 元认知图边
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaGraphEdge {
    pub edge_type: EdgeType,
    pub from_id: String,
    pub to_id: String,
    pub weight: Option<f32>,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

// ============================================================================
// 因果链相关类型
// ============================================================================

/// 因果链
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalChain {
    /// 根事件 ID
    pub root_event_id: String,
    /// 链中的事件列表（按因果顺序）
    pub events: Vec<CausalChainNode>,
    /// 链的总长度
    pub length: usize,
    /// 链的总时间跨度（毫秒）
    pub time_span_ms: i64,
}

/// 因果链节点
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalChainNode {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at_ms: i64,
    pub cause_type: EdgeType,
    pub depth: u32,
}

// ============================================================================
// 性能剖析相关类型
// ============================================================================

/// AC 性能报告
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ACPerformanceReport {
    pub ac_id: String,
    pub total_duration_ms: u64,
    pub ic_count: u32,
    pub total_cost: f64,
    pub decision_latency_ms: u64,
    pub tool_execution_time_ms: u64,
    pub llm_time_ms: u64,
    pub bottlenecks: Vec<PerformanceBottleneck>,
}

/// Session 性能报告
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPerformanceReport {
    pub session_id: String,
    pub total_duration_ms: u64,
    pub ac_count: u32,
    pub total_cost: f64,
    pub average_ac_duration_ms: u64,
    pub bottlenecks: Vec<PerformanceBottleneck>,
}

/// 性能瓶颈
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceBottleneck {
    pub bottleneck_type: BottleneckType,
    pub component: String,
    pub duration_ms: u64,
    pub percentage: f32,
    pub suggestion: Option<String>,
}

/// 瓶颈类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckType {
    LlmLatency,
    ToolExecution,
    DecisionRouting,
    ContextAssembly,
    NetworkIO,
    DatabaseQuery,
}

// ============================================================================
// 模式检测相关类型
// ============================================================================

/// 行为模式
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviorPattern {
    pub pattern_id: String,
    pub pattern_type: PatternType,
    pub description: String,
    pub frequency: u32,
    pub confidence: f32,
    pub example_events: Vec<String>,
}

/// 模式类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// 重复决策模式
    RepetitiveDecision,
    /// 协作偏好
    CollaborationPreference,
    /// 工具使用习惯
    ToolUsageHabit,
    /// 澄清需求模式
    ClarificationPattern,
    /// 时间分布模式
    TemporalDistribution,
}

/// 交互模式
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionPattern {
    pub pattern_id: String,
    pub participants: Vec<String>,
    pub interaction_type: String,
    pub frequency: u32,
    pub avg_duration_ms: u64,
}

/// 异常
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Anomaly {
    pub anomaly_id: String,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub description: String,
    pub detected_at_ms: i64,
    pub related_events: Vec<String>,
}

/// 异常类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    UnusualLatency,
    UnexpectedDecision,
    CostSpike,
    FailureRate,
    UnusualPattern,
}

/// 异常严重程度
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

// ============================================================================
// 决策审计相关类型
// ============================================================================

/// 决策审计报告
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionAuditReport {
    pub decision_event_id: String,
    pub decision_path: String,
    pub confidence: f32,
    pub rationale: String,
    pub alternatives: Vec<AlternativeAnalysis>,
    pub context_factors: Vec<ContextFactor>,
    pub outcome_assessment: Option<OutcomeAssessment>,
}

/// 备选方案分析
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlternativeAnalysis {
    pub path: String,
    pub score: f32,
    pub rejection_reason: String,
    pub potential_outcome: Option<String>,
}

/// 上下文因素
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextFactor {
    pub factor_name: String,
    pub factor_value: serde_json::Value,
    pub influence_weight: f32,
}

/// 结果评估
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeAssessment {
    pub was_successful: bool,
    pub actual_cost: f64,
    pub actual_duration_ms: u64,
    pub user_satisfaction: Option<f32>,
    pub notes: Option<String>,
}
