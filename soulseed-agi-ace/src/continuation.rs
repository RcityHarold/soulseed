//! 场景五自主延续执行模式 - 延续检查机制
//!
//! 依据文档: 03-场景论-02-《场景五·分论》.md
//!
//! 本模块实现了 AC Orchestrator 的延续检查机制，包括：
//! - ContinuationChecker: 延续条件检查器
//! - AgendaDriver: 议程驱动器
//! - DiscourseExtender: 话语延伸驱动器

use soulseed_agi_core_models::awareness::{
    ContinuationDriver, ContinuationSignal, DiscourseExtensionPoint, DiscourseExtensionType,
    FinalizedPayload, NextAction, NextActionType,
};
use soulseed_agi_core_models::session::{
    AgendaItem, AgendaStatus, AutonomousContinuationConfig, ContinuationTerminationReason,
    Session, SessionMode,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// 延续决策结果
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContinuationDecision {
    /// 是否应该继续
    pub should_continue: bool,
    /// 延续信号
    pub signal: ContinuationSignal,
    /// 延续驱动来源
    pub driver: Option<ContinuationDriver>,
    /// 终止原因（如果不继续）
    pub termination_reason: Option<ContinuationTerminationReason>,
    /// 下一步动作
    pub next_action: Option<NextAction>,
    /// 待处理的议程项
    pub pending_agenda_item: Option<AgendaItem>,
    /// 检测到的话语延伸点
    pub extension_points: Vec<DiscourseExtensionPoint>,
}

impl ContinuationDecision {
    /// 创建终止决策
    pub fn terminate(reason: ContinuationTerminationReason) -> Self {
        Self {
            should_continue: false,
            signal: ContinuationSignal::Stop,
            driver: None,
            termination_reason: Some(reason),
            next_action: Some(NextAction {
                action_type: NextActionType::Terminate,
                reason: format!("{:?}", reason),
                suggested_focus: None,
                agenda_item_id: None,
                extension_topic: None,
            }),
            pending_agenda_item: None,
            extension_points: Vec::new(),
        }
    }

    /// 创建议程驱动继续决策
    pub fn continue_with_agenda(item: AgendaItem) -> Self {
        Self {
            should_continue: true,
            signal: ContinuationSignal::ContinueWithAgenda,
            driver: Some(ContinuationDriver::AgendaDriven),
            termination_reason: None,
            next_action: Some(NextAction {
                action_type: NextActionType::ContinueAgenda,
                reason: format!("Processing agenda item: {}", item.description),
                suggested_focus: Some(item.description.clone()),
                agenda_item_id: Some(item.item_id.to_string()),
                extension_topic: None,
            }),
            pending_agenda_item: Some(item),
            extension_points: Vec::new(),
        }
    }

    /// 创建话语延伸继续决策
    pub fn continue_with_extension(point: DiscourseExtensionPoint) -> Self {
        Self {
            should_continue: true,
            signal: ContinuationSignal::ContinueWithExtension,
            driver: Some(ContinuationDriver::DiscourseExtension),
            termination_reason: None,
            next_action: Some(NextAction {
                action_type: NextActionType::ExtendDiscourse,
                reason: format!("Extending discourse: {}", point.topic),
                suggested_focus: Some(point.topic.clone()),
                agenda_item_id: None,
                extension_topic: Some(point.topic.clone()),
            }),
            pending_agenda_item: None,
            extension_points: vec![point],
        }
    }

    /// 创建等待用户输入决策
    pub fn wait_for_input() -> Self {
        Self {
            should_continue: false,
            signal: ContinuationSignal::Stop,
            driver: None,
            termination_reason: None,
            next_action: Some(NextAction {
                action_type: NextActionType::WaitForInput,
                reason: "Waiting for user input".to_string(),
                suggested_focus: None,
                agenda_item_id: None,
                extension_topic: None,
            }),
            pending_agenda_item: None,
            extension_points: Vec::new(),
        }
    }
}

/// 延续检查器
///
/// 负责检查是否应该继续执行自主延续，以及选择驱动来源
pub struct ContinuationChecker {
    /// 配置
    config: AutonomousContinuationConfig,
    /// 议程驱动器
    agenda_driver: AgendaDriver,
    /// 话语延伸器
    discourse_extender: DiscourseExtender,
}

impl ContinuationChecker {
    /// 创建新的延续检查器
    pub fn new(config: AutonomousContinuationConfig) -> Self {
        Self {
            config,
            agenda_driver: AgendaDriver::new(),
            discourse_extender: DiscourseExtender::new(),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(AutonomousContinuationConfig::default())
    }

    /// 检查是否应该继续执行
    ///
    /// 根据 Session 状态和 FinalizedPayload 判断下一步动作
    pub fn check_continuation(
        &self,
        session: &Session,
        finalized: &FinalizedPayload,
    ) -> ContinuationDecision {
        // 1. 检查是否处于自主延续模式
        if !session.is_autonomous_mode() {
            return ContinuationDecision::wait_for_input();
        }

        // 2. 检查终止条件
        if let Some(reason) = self.check_termination_conditions(session, finalized) {
            return ContinuationDecision::terminate(reason);
        }

        // 3. 检查 FinalizedPayload 中的延续信号
        if finalized.continuation_signal == ContinuationSignal::Stop {
            // 如果 AC 明确要求停止，但我们还有议程，继续处理议程
            if let Some(item) = session.next_pending_agenda().cloned() {
                return ContinuationDecision::continue_with_agenda(item);
            }
            return ContinuationDecision::terminate(ContinuationTerminationReason::AgendaExhausted);
        }

        // 4. 根据延续信号选择驱动来源
        match finalized.continuation_signal {
            ContinuationSignal::ContinueWithAgenda => {
                // 议程驱动
                if let Some(item) = session.next_pending_agenda().cloned() {
                    ContinuationDecision::continue_with_agenda(item)
                } else {
                    // 议程耗尽，检查是否有话语延伸点
                    if let Some(point) = finalized.discourse_extension_points.first().cloned() {
                        ContinuationDecision::continue_with_extension(point)
                    } else {
                        ContinuationDecision::terminate(
                            ContinuationTerminationReason::AgendaExhausted,
                        )
                    }
                }
            }
            ContinuationSignal::ContinueWithExtension => {
                // 话语延伸驱动
                if let Some(point) = finalized.discourse_extension_points.first().cloned() {
                    ContinuationDecision::continue_with_extension(point)
                } else if let Some(item) = session.next_pending_agenda().cloned() {
                    // 没有延伸点，回退到议程驱动
                    ContinuationDecision::continue_with_agenda(item)
                } else {
                    ContinuationDecision::terminate(
                        ContinuationTerminationReason::AgendaExhausted,
                    )
                }
            }
            ContinuationSignal::Stop => {
                ContinuationDecision::terminate(ContinuationTerminationReason::UserRequested)
            }
        }
    }

    /// 检查终止条件
    fn check_termination_conditions(
        &self,
        session: &Session,
        finalized: &FinalizedPayload,
    ) -> Option<ContinuationTerminationReason> {
        // 使用 Session 自带的检查方法
        if let Some(reason) = session.should_terminate_continuation() {
            return Some(reason);
        }

        // 额外检查：如果 AC 没有产生有效输出，计入空转
        if !finalized.produced_meaningful_output {
            if session.consecutive_idle_count + 1 >= self.config.max_idle_count {
                return Some(ContinuationTerminationReason::MaxIdleReached);
            }
        }

        None
    }

    /// 从 FinalizedPayload 决定延续
    ///
    /// 这是一个简化版本，只基于 FinalizedPayload 做决策
    pub fn decide_from_finalized(&self, finalized: &FinalizedPayload) -> ContinuationDecision {
        if !finalized.should_continue() {
            return ContinuationDecision::terminate(ContinuationTerminationReason::UserRequested);
        }

        match finalized.continuation_signal {
            ContinuationSignal::ContinueWithAgenda => {
                if let Some(ref action) = finalized.next_action {
                    if let Some(ref item_id) = action.agenda_item_id {
                        let item = AgendaItem {
                            item_id: Uuid::parse_str(item_id).unwrap_or_else(|_| Uuid::new_v4()),
                            description: action.suggested_focus.clone().unwrap_or_default(),
                            priority: 100,
                            status: AgendaStatus::Pending,
                            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
                            estimated_completion_ms: None,
                            completed_at_ms: None,
                            assigned_ac_id: None,
                            metadata: serde_json::Value::Null,
                        };
                        return ContinuationDecision::continue_with_agenda(item);
                    }
                }
                ContinuationDecision::terminate(ContinuationTerminationReason::AgendaExhausted)
            }
            ContinuationSignal::ContinueWithExtension => {
                if let Some(point) = finalized.discourse_extension_points.first().cloned() {
                    return ContinuationDecision::continue_with_extension(point);
                } else if let Some(ref action) = finalized.next_action {
                    if let Some(ref topic) = action.extension_topic {
                        let point = DiscourseExtensionPoint {
                            point_id: Uuid::new_v4().to_string(),
                            topic: topic.clone(),
                            extension_type: DiscourseExtensionType::FollowUpSuggestion,
                            relevance_score: 0.8,
                            source_event_id: None,
                        };
                        return ContinuationDecision::continue_with_extension(point);
                    }
                }
                ContinuationDecision::terminate(ContinuationTerminationReason::AgendaExhausted)
            }
            ContinuationSignal::Stop => {
                ContinuationDecision::terminate(ContinuationTerminationReason::UserRequested)
            }
        }
    }
}

/// 议程驱动器
///
/// 负责从议程队列获取下一个待处理任务，并生成触发事件
pub struct AgendaDriver {
    // 可以添加配置字段
}

impl AgendaDriver {
    pub fn new() -> Self {
        Self {}
    }

    /// 获取下一个待处理的议程项
    pub fn get_next_item<'a>(&self, session: &'a Session) -> Option<&'a AgendaItem> {
        session.next_pending_agenda()
    }

    /// 创建议程触发的 SelfReflectionTriggered 事件内容
    pub fn create_trigger_content(&self, item: &AgendaItem) -> AgendaTriggerContent {
        AgendaTriggerContent {
            trigger_type: "agenda_driven".to_string(),
            agenda_item_id: item.item_id.to_string(),
            description: item.description.clone(),
            priority: item.priority,
            focus_hint: Some(format!("Focus on completing: {}", item.description)),
        }
    }
}

impl Default for AgendaDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// 议程触发内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgendaTriggerContent {
    pub trigger_type: String,
    pub agenda_item_id: String,
    pub description: String,
    pub priority: u8,
    pub focus_hint: Option<String>,
}

/// 话语延伸驱动器
///
/// 负责分析上一轮对话，检测可延伸的话题点
pub struct DiscourseExtender {
    /// 最小相关性分数阈值
    min_relevance_threshold: f32,
    /// 最大延伸点数量
    max_extension_points: usize,
}

impl DiscourseExtender {
    pub fn new() -> Self {
        Self {
            min_relevance_threshold: 0.5,
            max_extension_points: 3,
        }
    }

    /// 设置相关性阈值
    pub fn with_relevance_threshold(mut self, threshold: f32) -> Self {
        self.min_relevance_threshold = threshold;
        self
    }

    /// 设置最大延伸点数量
    pub fn with_max_points(mut self, max: usize) -> Self {
        self.max_extension_points = max;
        self
    }

    /// 从 FinalizedPayload 获取话语延伸点
    pub fn get_extension_points(&self, finalized: &FinalizedPayload) -> Vec<DiscourseExtensionPoint> {
        finalized
            .discourse_extension_points
            .iter()
            .filter(|p| p.relevance_score >= self.min_relevance_threshold)
            .take(self.max_extension_points)
            .cloned()
            .collect()
    }

    /// 选择最佳延伸点
    pub fn select_best_point(
        &self,
        finalized: &FinalizedPayload,
    ) -> Option<DiscourseExtensionPoint> {
        self.get_extension_points(finalized)
            .into_iter()
            .max_by(|a, b| a.relevance_score.partial_cmp(&b.relevance_score).unwrap())
    }

    /// 创建话语延伸触发内容
    pub fn create_trigger_content(&self, point: &DiscourseExtensionPoint) -> ExtensionTriggerContent {
        ExtensionTriggerContent {
            trigger_type: "discourse_extension".to_string(),
            point_id: point.point_id.clone(),
            topic: point.topic.clone(),
            extension_type: point.extension_type,
            relevance_score: point.relevance_score,
            focus_hint: Some(format!("Explore the topic: {}", point.topic)),
        }
    }
}

impl Default for DiscourseExtender {
    fn default() -> Self {
        Self::new()
    }
}

/// 话语延伸触发内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionTriggerContent {
    pub trigger_type: String,
    pub point_id: String,
    pub topic: String,
    pub extension_type: DiscourseExtensionType,
    pub relevance_score: f32,
    pub focus_hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::session::SessionMode;

    fn create_test_session(mode: SessionMode, agenda_items: Vec<AgendaItem>) -> Session {
        use soulseed_agi_core_models::*;

        Session {
            tenant_id: TenantId::from_raw_unchecked(1),
            session_id: SessionId::from_raw_unchecked(1),
            trace_id: "test-trace".to_string(),
            correlation_id: "test-corr".to_string(),
            subject: Subject::AI(0),
            participants: vec![],
            head: EnvelopeHead {
                envelope_id: uuid::Uuid::new_v4(),
                trace_id: "test-trace".to_string(),
                correlation_id: "test-corr".to_string(),
                config_snapshot_hash: "test".to_string(),
                config_snapshot_version: 1,
            },
            snapshot: Snapshot {
                schema_v: 1,
                created_at: time::OffsetDateTime::now_utc(),
            },
            created_at: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            scenario: None,
            access_class: AccessClass::Internal,
            provenance: None,
            supersedes: None,
            superseded_by: None,
            evidence_pointer: None,
            blob_ref: None,
            content_digest_sha256: None,
            metadata: serde_json::Value::Null,
            mode,
            orchestration_id: Some(uuid::Uuid::new_v4()),
            agenda_queue: agenda_items,
            continuation_config: Some(AutonomousContinuationConfig::default()),
            consecutive_ac_count: 0,
            consecutive_idle_count: 0,
            total_cost_spent: 0.0,
            last_activity_at_ms: Some(time::OffsetDateTime::now_utc().unix_timestamp() * 1000),
            scenario_stack: vec![],
        }
    }

    #[test]
    fn test_continuation_checker_interactive_mode() {
        let checker = ContinuationChecker::with_defaults();
        let session = create_test_session(SessionMode::Interactive, vec![]);
        let finalized = FinalizedPayload::default();

        let decision = checker.check_continuation(&session, &finalized);
        assert!(!decision.should_continue);
        assert_eq!(
            decision.next_action.unwrap().action_type,
            NextActionType::WaitForInput
        );
    }

    #[test]
    fn test_continuation_checker_with_agenda() {
        let checker = ContinuationChecker::with_defaults();
        let item = AgendaItem::new("Test task");
        let session = create_test_session(SessionMode::AutonomousContinuation, vec![item]);
        let mut finalized = FinalizedPayload::default();
        finalized.continuation_signal = ContinuationSignal::ContinueWithAgenda;

        let decision = checker.check_continuation(&session, &finalized);
        assert!(decision.should_continue);
        assert_eq!(decision.signal, ContinuationSignal::ContinueWithAgenda);
        assert!(decision.pending_agenda_item.is_some());
    }

    #[test]
    fn test_continuation_checker_max_ac_reached() {
        let checker = ContinuationChecker::with_defaults();
        let item = AgendaItem::new("Test task");
        let mut session = create_test_session(SessionMode::AutonomousContinuation, vec![item]);
        session.consecutive_ac_count = 100; // 超过默认限制
        let finalized = FinalizedPayload::default();

        let decision = checker.check_continuation(&session, &finalized);
        assert!(!decision.should_continue);
        assert_eq!(
            decision.termination_reason,
            Some(ContinuationTerminationReason::MaxAcReached)
        );
    }
}
