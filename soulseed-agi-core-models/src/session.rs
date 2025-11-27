use crate::{
    common::EvidencePointer, AccessClass, ConversationScenario, CorrelationId, EnvelopeHead,
    ModelError, Provenance, SessionId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

// ============================================================================
// 场景五自主延续执行模式 - Session Mode
// ============================================================================

/// Session 执行模式
/// 依据文档: 03-场景论-02-《场景五·分论》.md
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// 交互模式（默认）- 等待用户输入后触发 AC
    #[default]
    Interactive,
    /// 自主延续模式 - AI 可在无用户输入时自主触发后续 AC
    AutonomousContinuation,
}

/// 议程项状态
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgendaStatus {
    /// 待处理
    #[default]
    Pending,
    /// 进行中
    InProgress,
    /// 已完成
    Completed,
    /// 已跳过
    Skipped,
    /// 已失败
    Failed,
}

/// 议程项 - 自主延续模式下的任务单元
/// 用于议程驱动轨道
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgendaItem {
    /// 议程项唯一标识
    pub item_id: Uuid,
    /// 议程项描述
    pub description: String,
    /// 优先级 (0-255, 值越小优先级越高)
    #[serde(default)]
    pub priority: u8,
    /// 当前状态
    #[serde(default)]
    pub status: AgendaStatus,
    /// 创建时间 (Unix毫秒时间戳)
    pub created_at_ms: i64,
    /// 预计完成时间 (Unix毫秒时间戳)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_completion_ms: Option<i64>,
    /// 实际完成时间 (Unix毫秒时间戳)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    /// 关联的 AC ID (处理此议程项的 AC)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_ac_id: Option<u64>,
    /// 元数据
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

impl AgendaItem {
    /// 创建新的议程项
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            item_id: Uuid::new_v4(),
            description: description.into(),
            priority: 100, // 默认中等优先级
            status: AgendaStatus::Pending,
            created_at_ms: OffsetDateTime::now_utc().unix_timestamp() * 1000,
            estimated_completion_ms: None,
            completed_at_ms: None,
            assigned_ac_id: None,
            metadata: Value::Null,
        }
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// 标记为进行中
    pub fn mark_in_progress(&mut self, ac_id: u64) {
        self.status = AgendaStatus::InProgress;
        self.assigned_ac_id = Some(ac_id);
    }

    /// 标记为完成
    pub fn mark_completed(&mut self) {
        self.status = AgendaStatus::Completed;
        self.completed_at_ms = Some(OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }

    /// 标记为跳过
    pub fn mark_skipped(&mut self) {
        self.status = AgendaStatus::Skipped;
        self.completed_at_ms = Some(OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }

    /// 标记为失败
    pub fn mark_failed(&mut self) {
        self.status = AgendaStatus::Failed;
        self.completed_at_ms = Some(OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }
}

/// 自主延续配置
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutonomousContinuationConfig {
    /// 最大连续 AC 数量
    #[serde(default = "default_max_consecutive_ac")]
    pub max_consecutive_ac: u32,
    /// 最大总成本限制
    #[serde(default = "default_max_total_cost")]
    pub max_total_cost: f64,
    /// 空闲超时（毫秒）
    #[serde(default = "default_idle_timeout_ms")]
    pub idle_timeout_ms: u64,
    /// 最大连续空转次数
    #[serde(default = "default_max_idle_count")]
    pub max_idle_count: u32,
}

fn default_max_consecutive_ac() -> u32 {
    10
}

fn default_max_total_cost() -> f64 {
    100.0
}

fn default_idle_timeout_ms() -> u64 {
    30_000 // 30 秒
}

fn default_max_idle_count() -> u32 {
    3
}

impl Default for AutonomousContinuationConfig {
    fn default() -> Self {
        Self {
            max_consecutive_ac: default_max_consecutive_ac(),
            max_total_cost: default_max_total_cost(),
            idle_timeout_ms: default_idle_timeout_ms(),
            max_idle_count: default_max_idle_count(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub tenant_id: TenantId,
    pub session_id: SessionId,
    pub trace_id: TraceId,
    pub correlation_id: CorrelationId,
    pub subject: Subject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    pub head: EnvelopeHead,
    pub snapshot: Snapshot,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ConversationScenario>,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,

    // ========================================================================
    // 场景五自主延续执行模式字段
    // 依据文档: 03-场景论-02-《场景五·分论》.md
    // ========================================================================

    /// Session 执行模式 (交互模式 / 自主延续模式)
    #[serde(default)]
    pub mode: SessionMode,

    /// 编排标识 - 用于追踪同一"自主执行批次"下的多个 AC
    /// 当进入自主延续模式时生成，用于关联该批次内所有 AC
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration_id: Option<Uuid>,

    /// 议程队列 - 自主延续模式下的任务列表
    /// 议程驱动轨道使用
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agenda_queue: Vec<AgendaItem>,

    /// 自主延续配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_config: Option<AutonomousContinuationConfig>,

    /// 当前连续 AC 计数 (用于限制检查)
    #[serde(default)]
    pub consecutive_ac_count: u32,

    /// 当前连续空转计数 (未产生有效输出的 AC 次数)
    #[serde(default)]
    pub consecutive_idle_count: u32,

    /// 累计成本 (用于成本限制检查)
    #[serde(default)]
    pub total_cost_spent: f64,

    /// 上次活动时间戳 (毫秒)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at_ms: Option<i64>,

    /// 场景栈 - 用于跨场景调用时的场景追踪
    /// 依据文档: 02-场景论-01-场景总论.md
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenario_stack: Vec<ConversationScenario>,
}

#[cfg(feature = "strict-privacy")]
fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

impl Session {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.head.trace_id != self.trace_id {
            return Err(ModelError::Invariant(
                "session.trace_id must match head.trace_id",
            ));
        }
        if self.head.correlation_id != self.correlation_id {
            return Err(ModelError::Invariant(
                "session.correlation_id must match head.correlation_id",
            ));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }

    // ========================================================================
    // 场景五自主延续执行模式方法
    // ========================================================================

    /// 检查是否处于自主延续模式
    pub fn is_autonomous_mode(&self) -> bool {
        matches!(self.mode, SessionMode::AutonomousContinuation)
    }

    /// 进入自主延续模式
    pub fn enter_autonomous_mode(&mut self) {
        self.mode = SessionMode::AutonomousContinuation;
        if self.orchestration_id.is_none() {
            self.orchestration_id = Some(Uuid::new_v4());
        }
        self.consecutive_ac_count = 0;
        self.consecutive_idle_count = 0;
        self.last_activity_at_ms = Some(OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }

    /// 退出自主延续模式，返回交互模式
    pub fn exit_autonomous_mode(&mut self) {
        self.mode = SessionMode::Interactive;
        // 保留 orchestration_id 用于历史追踪，不清除
    }

    /// 添加议程项
    pub fn add_agenda_item(&mut self, item: AgendaItem) {
        self.agenda_queue.push(item);
        // 按优先级排序
        self.agenda_queue.sort_by_key(|i| i.priority);
    }

    /// 获取下一个待处理的议程项
    pub fn next_pending_agenda(&self) -> Option<&AgendaItem> {
        self.agenda_queue
            .iter()
            .find(|item| matches!(item.status, AgendaStatus::Pending))
    }

    /// 获取下一个待处理的议程项（可变引用）
    pub fn next_pending_agenda_mut(&mut self) -> Option<&mut AgendaItem> {
        self.agenda_queue
            .iter_mut()
            .find(|item| matches!(item.status, AgendaStatus::Pending))
    }

    /// 检查议程队列是否已清空（所有项目都已完成或跳过）
    pub fn is_agenda_exhausted(&self) -> bool {
        self.agenda_queue.iter().all(|item| {
            matches!(
                item.status,
                AgendaStatus::Completed | AgendaStatus::Skipped | AgendaStatus::Failed
            )
        })
    }

    /// 记录一次 AC 完成，更新计数器
    pub fn record_ac_completion(&mut self, cost: f64, was_idle: bool) {
        self.consecutive_ac_count += 1;
        self.total_cost_spent += cost;
        self.last_activity_at_ms = Some(OffsetDateTime::now_utc().unix_timestamp() * 1000);

        if was_idle {
            self.consecutive_idle_count += 1;
        } else {
            self.consecutive_idle_count = 0;
        }
    }

    /// 检查是否应该终止自主延续
    pub fn should_terminate_continuation(&self) -> Option<ContinuationTerminationReason> {
        let config = self.continuation_config.as_ref().cloned().unwrap_or_default();

        // 检查议程是否耗尽
        if self.is_agenda_exhausted() && self.agenda_queue.is_empty() == false {
            return Some(ContinuationTerminationReason::AgendaExhausted);
        }

        // 检查连续 AC 数量
        if self.consecutive_ac_count >= config.max_consecutive_ac {
            return Some(ContinuationTerminationReason::MaxAcReached);
        }

        // 检查成本上限
        if self.total_cost_spent >= config.max_total_cost {
            return Some(ContinuationTerminationReason::CostLimitExceeded);
        }

        // 检查连续空转
        if self.consecutive_idle_count >= config.max_idle_count {
            return Some(ContinuationTerminationReason::MaxIdleReached);
        }

        // 检查空闲超时
        if let Some(last_activity) = self.last_activity_at_ms {
            let now = OffsetDateTime::now_utc().unix_timestamp() * 1000;
            if (now - last_activity) as u64 > config.idle_timeout_ms {
                return Some(ContinuationTerminationReason::IdleTimeout);
            }
        }

        None
    }

    // ========================================================================
    // 场景栈方法
    // ========================================================================

    /// 压入场景到场景栈
    pub fn push_scenario(&mut self, scenario: ConversationScenario) {
        self.scenario_stack.push(scenario);
    }

    /// 从场景栈弹出场景
    pub fn pop_scenario(&mut self) -> Option<ConversationScenario> {
        self.scenario_stack.pop()
    }

    /// 获取当前场景栈深度
    pub fn scenario_stack_depth(&self) -> usize {
        self.scenario_stack.len()
    }
}

/// 自主延续终止原因
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationTerminationReason {
    /// 议程队列已清空
    AgendaExhausted,
    /// 达到最大连续 AC 数量
    MaxAcReached,
    /// 超过成本上限
    CostLimitExceeded,
    /// 达到最大连续空转次数
    MaxIdleReached,
    /// 空闲超时
    IdleTimeout,
    /// 外部中断
    ExternalInterrupt,
    /// 用户请求终止
    UserRequested,
}
