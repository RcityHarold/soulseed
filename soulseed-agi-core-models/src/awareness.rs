use crate::{
    AccessClass, AwarenessCycleId, EnvelopeId, EventId, InferenceCycleId, ModelError, Provenance,
    SessionId, Snapshot, Subject, TenantId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "strict-privacy")]
fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

/// Anchor information used by Awareness layer payloads (ACE/DFR/CA).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AwarenessAnchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[cfg(feature = "strict-privacy")]
    #[serde(default = "default_restricted")]
    pub access_class: AccessClass,
    #[cfg(not(feature = "strict-privacy"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
}

impl AwarenessAnchor {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.config_snapshot_hash.trim().is_empty() {
            return Err(ModelError::Missing("config_snapshot_hash"));
        }
        if self.schema_v == 0 {
            return Err(ModelError::Invariant("schema_v must be >= 1"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Invariant(
                "provenance required for restricted anchor",
            ));
        }
        Ok(())
    }
}

/// Primary fork decision paths supported by ACE/DFR.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessFork {
    SelfReason,
    Clarify,
    ToolPath,
    Collab,
}

/// Sync points at which ACE aggregates external signals.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SyncPointKind {
    IcEnd,
    ToolBarrier,
    ToolBarrierReached,
    ToolBarrierReleased,
    ToolBarrierTimeout,
    ToolChainNext,
    CollabTurnEnd,
    ClarifyAnswered,
    ClarifyWindowOpened,
    ClarifyWindowClosed,
    ToolWindowOpened,
    ToolWindowClosed,
    HitlWindowOpened,
    HitlWindowClosed,
    HitlAbsorb,
    Merged,
    DriftDetected,
    LateSignalObserved,
    BudgetExceeded,
    BudgetRecovered,
    DegradationRecorded,
}

/// Outcome of an awareness cycle once it reaches completion.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessOutcome {
    Continue,
    Finalized,
    Rejected,
}

/// Awareness layer append-only event enumeration.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessEventType {
    #[serde(alias = "ac_started")]
    AwarenessCycleStarted,
    #[serde(alias = "ac_ended")]
    AwarenessCycleEnded,
    #[serde(alias = "ic_started")]
    InferenceCycleStarted,
    #[serde(alias = "ic_ended")]
    InferenceCycleCompleted,
    AssessmentProduced,
    DecisionRouted,
    ToolPathDecided,
    RouteReconsidered,
    RouteSwitched,
    ToolCalled,
    ToolResponded,
    ToolFailed,
    ToolBarrierReached,
    ToolBarrierReleased,
    ToolBarrierTimeout,
    CollabRequested,
    CollabResolved,
    ClarificationIssued,
    ClarificationAnswered,
    HumanInjectionReceived,
    #[serde(alias = "injection_applied")]
    HumanInjectionApplied,
    #[serde(alias = "injection_deferred")]
    HumanInjectionDeferred,
    #[serde(alias = "injection_ignored")]
    HumanInjectionIgnored,
    DeltaPatchGenerated,
    ContextBuilt,
    DeltaMerged,
    SyncPointMerged,
    SyncPointReported,
    Finalized,
    Rejected,
    LateReceiptObserved,
    EnvironmentSnapshotRecorded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AwarenessEvent {
    #[serde(flatten)]
    pub anchor: AwarenessAnchor,
    pub event_id: EventId,
    pub event_type: AwarenessEventType,
    pub occurred_at_ms: i64,
    pub awareness_cycle_id: AwarenessCycleId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_cycle_id: Option<AwarenessCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collab_scope_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_mode: Option<String>,
    pub inference_cycle_sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: Value,
}

impl AwarenessEvent {
    pub fn validate(&self) -> Result<(), ModelError> {
        self.anchor.validate()?;
        if self.occurred_at_ms < 0 {
            return Err(ModelError::Invariant("occurred_at_ms must be >= 0"));
        }
        if self.inference_cycle_sequence == 0 {
            return Err(ModelError::Invariant(
                "inference_cycle_sequence must be >= 1",
            ));
        }
        if let Some(parent_cycle_id) = self.parent_cycle_id {
            if parent_cycle_id == self.awareness_cycle_id {
                return Err(ModelError::Invariant(
                    "parent_cycle_id cannot equal awareness_cycle_id",
                ));
            }
        }
        if let Some(scope_id) = &self.collab_scope_id {
            if scope_id.trim().is_empty() {
                return Err(ModelError::Invariant("collab_scope_id cannot be empty"));
            }
        }
        if let Some(barrier_id) = &self.barrier_id {
            if barrier_id.trim().is_empty() {
                return Err(ModelError::Invariant("barrier_id cannot be empty"));
            }
        }
        if let Some(env_mode) = &self.env_mode {
            if env_mode.trim().is_empty() {
                return Err(ModelError::Invariant("env_mode cannot be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionBudgetEstimate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walltime_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_cost: Option<f32>,
}

impl Default for DecisionBudgetEstimate {
    fn default() -> Self {
        Self {
            tokens: None,
            walltime_ms: None,
            external_cost: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionBlockReason {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionInvalidReason {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionRationale {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<DecisionBlockReason>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid: Vec<DecisionInvalidReason>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scores: HashMap<String, f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thresholds_hit: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tradeoff: Option<String>,
}

impl Default for DecisionRationale {
    fn default() -> Self {
        Self {
            blocked: Vec::new(),
            invalid: Vec::new(),
            scores: HashMap::new(),
            thresholds_hit: Vec::new(),
            tradeoff: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionExplain {
    pub routing_seed: u64,
    pub router_digest: String,
    pub router_config_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features_snapshot: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ic: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarifyQuestion {
    pub q_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClarifyLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_wait_ms: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClarifyPlan {
    #[serde(default)]
    pub questions: Vec<ClarifyQuestion>,
    #[serde(default)]
    pub limits: ClarifyLimits,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolPlanNode {
    pub id: String,
    pub tool_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_policy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPlanEdge {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolPlanBarrier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolPlan {
    #[serde(default)]
    pub nodes: Vec<ToolPlanNode>,
    #[serde(default)]
    pub edges: Vec<ToolPlanEdge>,
    #[serde(default)]
    pub barrier: ToolPlanBarrier,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CollabPlan {
    pub scope: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_mode: Option<String>,
    #[serde(default)]
    pub barrier: ToolPlanBarrier,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionPlan {
    SelfReason { plan: SelfPlan },
    Clarify { plan: ClarifyPlan },
    Tool { plan: ToolPlan },
    Collab { plan: CollabPlan },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessDegradationReason {
    BudgetTokens,
    BudgetWalltime,
    BudgetExternalCost,
    EmptyCatalog,
    PrivacyBlocked,
    InvalidPlan,
    ClarifyExhausted,
    GraphDegraded,
    EnvctxDegraded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionPath {
    pub anchor: AwarenessAnchor,
    pub awareness_cycle_id: AwarenessCycleId,
    pub inference_cycle_sequence: u32,
    pub fork: AwarenessFork,
    pub plan: DecisionPlan,
    #[serde(default)]
    pub budget_plan: DecisionBudgetEstimate,
    #[serde(default)]
    pub rationale: DecisionRationale,
    pub confidence: f32,
    pub explain: DecisionExplain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeltaPatch {
    pub patch_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub score_stats: HashMap<String, f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why_included: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointers: Option<Value>,
    pub patch_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncPointReport {
    pub anchor: AwarenessAnchor,
    pub awareness_cycle_id: AwarenessCycleId,
    pub inference_cycle_sequence: u32,
    pub kind: SyncPointKind,
    #[serde(default)]
    pub inbox_stats: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_digest: Option<String>,
    #[serde(default)]
    pub budget_snapshot: Value,
    pub report_digest: String,
}

// ============================================================================
// 场景五自主延续执行模式 - Finalized Payload 扩展
// 依据文档: 03-场景论-02-《场景五·分论》.md
// ============================================================================

/// 下一步动作类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NextActionType {
    /// 继续执行议程
    ContinueAgenda,
    /// 延伸话语
    ExtendDiscourse,
    /// 终止执行
    Terminate,
    /// 等待用户输入
    WaitForInput,
}

/// 下一步动作
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NextAction {
    /// 动作类型
    pub action_type: NextActionType,
    /// 原因说明
    pub reason: String,
    /// 建议的关注点
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_focus: Option<String>,
    /// 关联的议程项 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda_item_id: Option<String>,
    /// 延伸话题
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_topic: Option<String>,
}

/// 延续信号
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationSignal {
    /// 停止
    #[default]
    Stop,
    /// 议程驱动继续
    ContinueWithAgenda,
    /// 话语延伸继续
    ContinueWithExtension,
}

/// 延续驱动来源
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationDriver {
    /// 议程驱动
    AgendaDriven,
    /// 话语延伸
    DiscourseExtension,
    /// 混合驱动
    Hybrid,
}

/// Finalized 事件的详细 Payload
/// 依据文档: 07-功能论.md - FinalizedPayload
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FinalizedPayload {
    /// AC 完成原因
    pub finalize_reason: String,
    /// 总 IC 数量
    pub total_ic_count: u32,
    /// 总成本
    pub total_cost: f64,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
    /// 决策路径摘要
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_path_summary: Option<String>,
    /// 输出摘要
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    /// 质量自评分
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_self_assessment: Option<f32>,

    // 场景五自主延续字段
    /// 下一步动作
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<NextAction>,
    /// 延续信号
    #[serde(default)]
    pub continuation_signal: ContinuationSignal,
    /// 延续驱动来源
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_driver: Option<ContinuationDriver>,
    /// 是否产生了有效输出（用于空转检测）
    #[serde(default)]
    pub produced_meaningful_output: bool,
    /// 话语延伸点检测结果
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discourse_extension_points: Vec<DiscourseExtensionPoint>,
}

impl Default for FinalizedPayload {
    fn default() -> Self {
        Self {
            finalize_reason: "completed".to_string(),
            total_ic_count: 1,
            total_cost: 0.0,
            total_duration_ms: 0,
            decision_path_summary: None,
            output_summary: None,
            quality_self_assessment: None,
            next_action: None,
            continuation_signal: ContinuationSignal::Stop,
            continuation_driver: None,
            produced_meaningful_output: true,
            discourse_extension_points: Vec::new(),
        }
    }
}

impl FinalizedPayload {
    /// 创建一个继续议程的 Finalized Payload
    pub fn continue_with_agenda(
        finalize_reason: impl Into<String>,
        agenda_item_id: impl Into<String>,
        suggested_focus: impl Into<String>,
    ) -> Self {
        Self {
            finalize_reason: finalize_reason.into(),
            continuation_signal: ContinuationSignal::ContinueWithAgenda,
            continuation_driver: Some(ContinuationDriver::AgendaDriven),
            next_action: Some(NextAction {
                action_type: NextActionType::ContinueAgenda,
                reason: "Agenda item pending".to_string(),
                suggested_focus: Some(suggested_focus.into()),
                agenda_item_id: Some(agenda_item_id.into()),
                extension_topic: None,
            }),
            produced_meaningful_output: true,
            ..Default::default()
        }
    }

    /// 创建一个话语延伸的 Finalized Payload
    pub fn continue_with_extension(
        finalize_reason: impl Into<String>,
        extension_topic: impl Into<String>,
    ) -> Self {
        Self {
            finalize_reason: finalize_reason.into(),
            continuation_signal: ContinuationSignal::ContinueWithExtension,
            continuation_driver: Some(ContinuationDriver::DiscourseExtension),
            next_action: Some(NextAction {
                action_type: NextActionType::ExtendDiscourse,
                reason: "Discourse extension detected".to_string(),
                suggested_focus: None,
                agenda_item_id: None,
                extension_topic: Some(extension_topic.into()),
            }),
            produced_meaningful_output: true,
            ..Default::default()
        }
    }

    /// 创建一个终止的 Finalized Payload
    pub fn terminate(finalize_reason: impl Into<String>) -> Self {
        Self {
            finalize_reason: finalize_reason.into(),
            continuation_signal: ContinuationSignal::Stop,
            next_action: Some(NextAction {
                action_type: NextActionType::Terminate,
                reason: "Task completed".to_string(),
                suggested_focus: None,
                agenda_item_id: None,
                extension_topic: None,
            }),
            produced_meaningful_output: true,
            ..Default::default()
        }
    }

    /// 检查是否应该继续执行
    pub fn should_continue(&self) -> bool {
        !matches!(self.continuation_signal, ContinuationSignal::Stop)
    }
}

/// 话语延伸点
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscourseExtensionPoint {
    /// 延伸点 ID
    pub point_id: String,
    /// 延伸主题
    pub topic: String,
    /// 延伸类型
    pub extension_type: DiscourseExtensionType,
    /// 相关性分数
    pub relevance_score: f32,
    /// 来源事件 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
}

/// 话语延伸类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscourseExtensionType {
    /// 未回答的问题
    UnansweredQuestion,
    /// 未展开的话题
    UnexploredTopic,
    /// 隐含的需求
    ImpliedNeed,
    /// 后续建议
    FollowUpSuggestion,
    /// 关联话题
    RelatedTopic,
}

/// Rejected 事件的详细 Payload
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RejectedPayload {
    /// 拒绝原因
    pub reject_reason: String,
    /// 拒绝类型
    pub reject_type: RejectType,
    /// 总 IC 数量
    pub total_ic_count: u32,
    /// 总成本
    pub total_cost: f64,
    /// 是否可重试
    pub retryable: bool,
    /// 建议的重试时间（毫秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

/// 拒绝类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RejectType {
    /// 预算耗尽
    BudgetExhausted,
    /// 策略违规
    PolicyViolation,
    /// 内容过滤
    ContentFiltered,
    /// 系统错误
    SystemError,
    /// 超时
    Timeout,
    /// 取消
    Cancelled,
}

/// Historical awareness cycle record. Retained for compatibility with existing contracts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessCycleRecord {
    pub tenant_id: TenantId,
    pub cycle_id: AwarenessCycleId,
    pub session_id: SessionId,
    pub subject: Subject,
    pub cycle_sequence: u64,
    pub cycle_duration_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance_trend: Option<String>,
    #[serde(default)]
    pub llm_calls: u32,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub resource_cost: Value,
    pub decision_path_taken: String,
    pub snapshot: Snapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_cycle_id: Option<InferenceCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_explain_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_snapshot_version: Option<u32>,
}

impl AwarenessCycleRecord {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.cycle_sequence == 0 {
            return Err(ModelError::Invariant("cycle_sequence must be >= 1"));
        }
        if self.decision_path_taken.trim().is_empty() {
            return Err(ModelError::Missing("decision_path_taken"));
        }
        if let Some(inference_cycle_id) = self.inference_cycle_id {
            if inference_cycle_id.into_inner() == 0 {
                return Err(ModelError::Invariant(
                    "inference_cycle_id must be >= 1 when provided",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_path_roundtrip() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(1),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: Some(SessionId::from_raw_unchecked(7)),
            sequence_number: Some(3),
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        };

        let decision = DecisionPath {
            anchor,
            awareness_cycle_id: AwarenessCycleId::from_raw_unchecked(11),
            inference_cycle_sequence: 2,
            fork: AwarenessFork::ToolPath,
            plan: DecisionPlan::Tool {
                plan: ToolPlan {
                    nodes: vec![ToolPlanNode {
                        id: "t1".into(),
                        tool_id: "web.search".into(),
                        version: Some("v1".into()),
                        input: serde_json::json!({"q": "demo"}),
                        timeout_ms: Some(2_000),
                        success_criteria: None,
                        evidence_policy: Some("pointer-only".into()),
                    }],
                    edges: vec![],
                    barrier: ToolPlanBarrier {
                        mode: Some("all".into()),
                        timeout_ms: Some(3_000),
                    },
                },
            },
            budget_plan: DecisionBudgetEstimate {
                tokens: Some(800),
                walltime_ms: Some(1_200),
                external_cost: None,
            },
            rationale: DecisionRationale {
                blocked: vec![DecisionBlockReason {
                    id: "tool:internal.db".into(),
                    reason: Some("privacy_scope".into()),
                }],
                invalid: vec![],
                scores: HashMap::from([("self".into(), 0.1), ("tool".into(), 0.85)]),
                thresholds_hit: vec!["budget_tokens".into()],
                tradeoff: Some("tool vs clarify".into()),
            },
            confidence: 0.76,
            explain: DecisionExplain {
                routing_seed: 42,
                router_digest: "sha256:abc".into(),
                router_config_digest: "sha256:def".into(),
                features_snapshot: Some(serde_json::json!({"top": ["rel", "risk"]})),
            },
            degradation_reason: Some(AwarenessDegradationReason::BudgetTokens),
        };

        let json = serde_json::to_string(&decision).expect("serialize");
        let back: DecisionPath = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decision, back);
    }

    #[test]
    fn delta_patch_defaults() {
        let patch = DeltaPatch {
            patch_id: "patch-1".into(),
            added: vec!["ctx-1".into()],
            updated: Vec::new(),
            removed: Vec::new(),
            score_stats: HashMap::new(),
            why_included: None,
            pointers: None,
            patch_digest: "sha256:xyz".into(),
        };

        let json = serde_json::to_string(&patch).expect("serialize");
        let back: DeltaPatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(patch, back);
    }

    #[test]
    fn awareness_event_validate_roundtrip() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(42),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg-hash".into(),
            config_snapshot_version: 3,
            session_id: Some(SessionId::from_raw_unchecked(11)),
            sequence_number: Some(5),
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        };

        let event = AwarenessEvent {
            anchor: anchor.clone(),
            event_id: EventId::from_raw_unchecked(999),
            event_type: AwarenessEventType::DecisionRouted,
            occurred_at_ms: 123_456,
            awareness_cycle_id: AwarenessCycleId::from_raw_unchecked(77),
            parent_cycle_id: Some(AwarenessCycleId::from_raw_unchecked(21)),
            collab_scope_id: Some("scope-1".into()),
            barrier_id: Some("barrier-A".into()),
            env_mode: Some("turbo".into()),
            inference_cycle_sequence: 1,
            degradation_reason: Some(AwarenessDegradationReason::BudgetTokens),
            payload: serde_json::json!({"decision": "self"}),
        };

        event.validate().expect("valid awareness event");
        let serialized = serde_json::to_string(&event).expect("serialize");
        let restored: AwarenessEvent = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(restored.event_type, AwarenessEventType::DecisionRouted);
        assert_eq!(restored.payload["decision"], "self");
        assert_eq!(restored.anchor.schema_v, 1);
    }

    #[test]
    fn anchor_validation_rejects_missing_provenance() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(1),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: None,
            sequence_number: None,
            access_class: AccessClass::Restricted,
            provenance: None,
            schema_v: 1,
        };

        let err = anchor
            .validate()
            .expect_err("restricted requires provenance");
        assert!(matches!(err, ModelError::Invariant(_)));
    }
}
