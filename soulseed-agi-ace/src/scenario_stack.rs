//! 场景栈管理器
//!
//! 依据文档: 02-场景论-01-场景总论.md
//!
//! ScenarioStackManager 负责自动管理 Session 的 scenario_stack:
//! - 当跨场景调用事件触发时自动压入场景
//! - 当对应完成事件发生时自动弹出场景
//! - 提供场景栈深度限制保护

use soulseed_agi_core_models::dialogue_event::payload::DialogueEventPayloadKind;
use soulseed_agi_core_models::enums::ConversationScenario;
use soulseed_agi_core_models::session::Session;

/// 场景栈配置
#[derive(Clone, Debug)]
pub struct ScenarioStackConfig {
    /// 最大场景栈深度
    pub max_depth: usize,
    /// 是否启用自动场景切换
    pub auto_switch_enabled: bool,
}

impl Default for ScenarioStackConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            auto_switch_enabled: true,
        }
    }
}

/// 场景栈管理器
///
/// 负责根据事件类型自动管理场景栈的压入和弹出
pub struct ScenarioStackManager {
    config: ScenarioStackConfig,
}

/// 场景切换动作
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScenarioStackAction {
    /// 压入新场景
    Push(ConversationScenario),
    /// 弹出当前场景
    Pop,
    /// 无操作
    NoOp,
}

/// 场景栈操作结果
#[derive(Clone, Debug)]
pub struct ScenarioStackResult {
    /// 执行的动作
    pub action: ScenarioStackAction,
    /// 操作前的场景栈深度
    pub depth_before: usize,
    /// 操作后的场景栈深度
    pub depth_after: usize,
    /// 当前活动场景
    pub current_scenario: Option<ConversationScenario>,
    /// 是否达到深度限制
    pub depth_limit_reached: bool,
}

impl ScenarioStackManager {
    /// 创建新的场景栈管理器
    pub fn new() -> Self {
        Self {
            config: ScenarioStackConfig::default(),
        }
    }

    /// 使用自定义配置创建场景栈管理器
    pub fn with_config(config: ScenarioStackConfig) -> Self {
        Self { config }
    }

    /// 根据事件类型决定场景栈操作
    ///
    /// # 跨场景调用事件 (Push)
    /// - ClarificationIssued -> HumanToAi (等待用户澄清)
    /// - ToolPlanDrafted/ToolInvocationScheduled -> AiToSystem (工具调用)
    /// - CollabRequested -> AiToAi / AiGroup (AI协作)
    ///
    /// # 完成事件 (Pop)
    /// - ClarificationAnswered/AutoResolved/Expired/Aborted -> 返回原场景
    /// - ToolInvocationCompleted/Failed -> 返回原场景
    /// - CollabResolved/Aborted -> 返回原场景
    pub fn determine_action(&self, event_kind: DialogueEventPayloadKind) -> ScenarioStackAction {
        if !self.config.auto_switch_enabled {
            return ScenarioStackAction::NoOp;
        }

        match event_kind {
            // Clarification domain - 发起澄清请求
            DialogueEventPayloadKind::ClarificationIssued => {
                ScenarioStackAction::Push(ConversationScenario::HumanToAi)
            }

            // Clarification domain - 澄清完成
            DialogueEventPayloadKind::ClarificationAnswered
            | DialogueEventPayloadKind::ClarificationAutoResolved
            | DialogueEventPayloadKind::ClarificationExpired
            | DialogueEventPayloadKind::ClarificationAborted => ScenarioStackAction::Pop,

            // Tooling domain - 发起工具调用
            DialogueEventPayloadKind::ToolPlanDrafted
            | DialogueEventPayloadKind::ToolInvocationScheduled => {
                ScenarioStackAction::Push(ConversationScenario::AiToSystem)
            }

            // Tooling domain - 工具调用完成
            DialogueEventPayloadKind::ToolInvocationCompleted
            | DialogueEventPayloadKind::ToolInvocationFailed => ScenarioStackAction::Pop,

            // Collaboration domain - 发起协作请求
            DialogueEventPayloadKind::CollabRequested | DialogueEventPayloadKind::CollabProposed => {
                ScenarioStackAction::Push(ConversationScenario::AiToAi)
            }

            // Collaboration domain - 协作完成
            DialogueEventPayloadKind::CollabResolved | DialogueEventPayloadKind::CollabAborted => {
                ScenarioStackAction::Pop
            }

            // 其他事件不触发场景切换
            _ => ScenarioStackAction::NoOp,
        }
    }

    /// 对 Session 执行场景栈操作
    ///
    /// 根据事件类型自动压入或弹出场景栈
    pub fn apply_event(
        &self,
        session: &mut Session,
        event_kind: DialogueEventPayloadKind,
    ) -> ScenarioStackResult {
        let depth_before = session.scenario_stack_depth();
        let action = self.determine_action(event_kind);

        let (depth_limit_reached, actual_action) = match &action {
            ScenarioStackAction::Push(scenario) => {
                if depth_before >= self.config.max_depth {
                    // 达到深度限制，不再压入
                    tracing::warn!(
                        "Scenario stack depth limit reached: {} >= {}",
                        depth_before,
                        self.config.max_depth
                    );
                    (true, ScenarioStackAction::NoOp)
                } else {
                    session.push_scenario(scenario.clone());
                    // 同时更新 session.scenario 为当前活动场景
                    session.scenario = Some(scenario.clone());
                    (false, action.clone())
                }
            }
            ScenarioStackAction::Pop => {
                if let Some(popped) = session.pop_scenario() {
                    // 恢复到上一个场景
                    session.scenario = session.scenario_stack.last().cloned();
                    tracing::debug!("Popped scenario: {:?}, new current: {:?}", popped, session.scenario);
                    (false, action.clone())
                } else {
                    // 栈为空，无法弹出
                    tracing::debug!("Scenario stack is empty, nothing to pop");
                    (false, ScenarioStackAction::NoOp)
                }
            }
            ScenarioStackAction::NoOp => (false, action.clone()),
        };

        let depth_after = session.scenario_stack_depth();

        ScenarioStackResult {
            action: actual_action,
            depth_before,
            depth_after,
            current_scenario: session.scenario.clone(),
            depth_limit_reached,
        }
    }

    /// 批量处理事件（用于重放场景）
    pub fn apply_events(
        &self,
        session: &mut Session,
        event_kinds: &[DialogueEventPayloadKind],
    ) -> Vec<ScenarioStackResult> {
        event_kinds
            .iter()
            .map(|kind| self.apply_event(session, *kind))
            .collect()
    }

    /// 检查场景栈是否平衡
    ///
    /// 用于验证场景栈的 push/pop 是否配对
    pub fn is_stack_balanced(&self, session: &Session) -> bool {
        session.scenario_stack.is_empty()
    }

    /// 获取当前场景栈的完整路径
    pub fn get_scenario_path(&self, session: &Session) -> Vec<ConversationScenario> {
        session.scenario_stack.clone()
    }

    /// 强制清空场景栈（用于错误恢复）
    pub fn clear_stack(&self, session: &mut Session) {
        session.scenario_stack.clear();
        // 恢复到基础场景
        session.scenario = None;
        tracing::info!("Scenario stack cleared for session");
    }

    /// 获取场景栈统计信息
    pub fn get_stack_stats(&self, session: &Session) -> ScenarioStackStats {
        let mut stats = ScenarioStackStats::default();
        stats.current_depth = session.scenario_stack_depth();
        stats.max_depth_allowed = self.config.max_depth;

        for scenario in &session.scenario_stack {
            match scenario {
                ConversationScenario::HumanToAi => stats.human_to_ai_count += 1,
                ConversationScenario::AiToSystem => stats.ai_to_system_count += 1,
                ConversationScenario::AiToAi => stats.ai_to_ai_count += 1,
                ConversationScenario::AiGroup => stats.ai_group_count += 1,
                _ => stats.other_count += 1,
            }
        }

        stats
    }
}

impl Default for ScenarioStackManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 场景栈统计信息
#[derive(Clone, Debug, Default)]
pub struct ScenarioStackStats {
    /// 当前深度
    pub current_depth: usize,
    /// 允许的最大深度
    pub max_depth_allowed: usize,
    /// HumanToAi 场景数量
    pub human_to_ai_count: usize,
    /// AiToSystem 场景数量
    pub ai_to_system_count: usize,
    /// AiToAi 场景数量
    pub ai_to_ai_count: usize,
    /// AiGroup 场景数量
    pub ai_group_count: usize,
    /// 其他场景数量
    pub other_count: usize,
}

/// 事件-场景映射表
///
/// 用于查询哪些事件会触发场景切换
pub struct EventScenarioMapping;

impl EventScenarioMapping {
    /// 获取所有会触发 Push 的事件类型
    pub fn push_events() -> Vec<DialogueEventPayloadKind> {
        vec![
            DialogueEventPayloadKind::ClarificationIssued,
            DialogueEventPayloadKind::ToolPlanDrafted,
            DialogueEventPayloadKind::ToolInvocationScheduled,
            DialogueEventPayloadKind::CollabRequested,
            DialogueEventPayloadKind::CollabProposed,
        ]
    }

    /// 获取所有会触发 Pop 的事件类型
    pub fn pop_events() -> Vec<DialogueEventPayloadKind> {
        vec![
            DialogueEventPayloadKind::ClarificationAnswered,
            DialogueEventPayloadKind::ClarificationAutoResolved,
            DialogueEventPayloadKind::ClarificationExpired,
            DialogueEventPayloadKind::ClarificationAborted,
            DialogueEventPayloadKind::ToolInvocationCompleted,
            DialogueEventPayloadKind::ToolInvocationFailed,
            DialogueEventPayloadKind::CollabResolved,
            DialogueEventPayloadKind::CollabAborted,
        ]
    }

    /// 检查事件是否会触发场景切换
    pub fn is_scenario_switching_event(kind: DialogueEventPayloadKind) -> bool {
        Self::push_events().contains(&kind) || Self::pop_events().contains(&kind)
    }

    /// 获取事件对应的目标场景（仅对 Push 事件有效）
    pub fn target_scenario_for_event(
        kind: DialogueEventPayloadKind,
    ) -> Option<ConversationScenario> {
        match kind {
            DialogueEventPayloadKind::ClarificationIssued => Some(ConversationScenario::HumanToAi),
            DialogueEventPayloadKind::ToolPlanDrafted
            | DialogueEventPayloadKind::ToolInvocationScheduled => {
                Some(ConversationScenario::AiToSystem)
            }
            DialogueEventPayloadKind::CollabRequested
            | DialogueEventPayloadKind::CollabProposed => Some(ConversationScenario::AiToAi),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::{
        CorrelationId, EnvelopeHead, SessionId, Snapshot, Subject, TenantId, TraceId,
    };

    fn create_test_session() -> Session {
        Session {
            tenant_id: TenantId::new(1),
            session_id: SessionId::new_v4(),
            trace_id: TraceId::new_v4(),
            correlation_id: CorrelationId::new_v4(),
            subject: Subject::Human(1),
            participants: vec![],
            head: EnvelopeHead {
                tenant_id: TenantId::new(1),
                trace_id: TraceId::new_v4(),
                correlation_id: CorrelationId::new_v4(),
            },
            snapshot: Snapshot::default(),
            created_at: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            scenario: None,
            access_class: soulseed_agi_core_models::AccessClass::Internal,
            provenance: None,
            supersedes: None,
            superseded_by: None,
            evidence_pointer: None,
            blob_ref: None,
            content_digest_sha256: None,
            metadata: serde_json::Value::Null,
            mode: soulseed_agi_core_models::session::SessionMode::Interactive,
            orchestration_id: None,
            agenda_queue: vec![],
            continuation_config: None,
            consecutive_ac_count: 0,
            consecutive_idle_count: 0,
            total_cost_spent: 0.0,
            last_activity_at_ms: None,
            scenario_stack: vec![],
        }
    }

    #[test]
    fn test_clarification_push_pop() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 发起澄清请求 - 应该压入 HumanToAi
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationIssued);
        assert_eq!(result.action, ScenarioStackAction::Push(ConversationScenario::HumanToAi));
        assert_eq!(result.depth_after, 1);
        assert_eq!(session.scenario, Some(ConversationScenario::HumanToAi));

        // 澄清回答 - 应该弹出
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationAnswered);
        assert_eq!(result.action, ScenarioStackAction::Pop);
        assert_eq!(result.depth_after, 0);
        assert_eq!(session.scenario, None);
    }

    #[test]
    fn test_tool_invocation_push_pop() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 工具计划草拟 - 应该压入 AiToSystem
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ToolPlanDrafted);
        assert_eq!(result.action, ScenarioStackAction::Push(ConversationScenario::AiToSystem));
        assert_eq!(result.depth_after, 1);

        // 工具调用完成 - 应该弹出
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationCompleted);
        assert_eq!(result.action, ScenarioStackAction::Pop);
        assert_eq!(result.depth_after, 0);
    }

    #[test]
    fn test_collaboration_push_pop() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 发起协作请求 - 应该压入 AiToAi
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::CollabRequested);
        assert_eq!(result.action, ScenarioStackAction::Push(ConversationScenario::AiToAi));
        assert_eq!(result.depth_after, 1);

        // 协作完成 - 应该弹出
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::CollabResolved);
        assert_eq!(result.action, ScenarioStackAction::Pop);
        assert_eq!(result.depth_after, 0);
    }

    #[test]
    fn test_nested_scenarios() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 嵌套场景：用户对话 -> 工具调用 -> AI协作
        manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationIssued);
        assert_eq!(session.scenario_stack_depth(), 1);

        manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationScheduled);
        assert_eq!(session.scenario_stack_depth(), 2);

        manager.apply_event(&mut session, DialogueEventPayloadKind::CollabRequested);
        assert_eq!(session.scenario_stack_depth(), 3);

        // 按顺序弹出
        manager.apply_event(&mut session, DialogueEventPayloadKind::CollabResolved);
        assert_eq!(session.scenario_stack_depth(), 2);

        manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationCompleted);
        assert_eq!(session.scenario_stack_depth(), 1);

        manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationAnswered);
        assert_eq!(session.scenario_stack_depth(), 0);
    }

    #[test]
    fn test_depth_limit() {
        let config = ScenarioStackConfig {
            max_depth: 3,
            auto_switch_enabled: true,
        };
        let manager = ScenarioStackManager::with_config(config);
        let mut session = create_test_session();

        // 压入到达限制
        for _ in 0..3 {
            manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationScheduled);
        }
        assert_eq!(session.scenario_stack_depth(), 3);

        // 尝试再次压入应该被阻止
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationScheduled);
        assert!(result.depth_limit_reached);
        assert_eq!(result.action, ScenarioStackAction::NoOp);
        assert_eq!(session.scenario_stack_depth(), 3);
    }

    #[test]
    fn test_pop_empty_stack() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 空栈弹出应该返回 NoOp
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationAnswered);
        assert_eq!(result.action, ScenarioStackAction::NoOp);
        assert_eq!(result.depth_after, 0);
    }

    #[test]
    fn test_non_switching_events() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 非场景切换事件应该返回 NoOp
        let result = manager.apply_event(&mut session, DialogueEventPayloadKind::MessagePrimary);
        assert_eq!(result.action, ScenarioStackAction::NoOp);
        assert_eq!(result.depth_after, 0);
    }

    #[test]
    fn test_event_scenario_mapping() {
        // 验证 Push 事件列表
        let push_events = EventScenarioMapping::push_events();
        assert!(push_events.contains(&DialogueEventPayloadKind::ClarificationIssued));
        assert!(push_events.contains(&DialogueEventPayloadKind::ToolPlanDrafted));
        assert!(push_events.contains(&DialogueEventPayloadKind::CollabRequested));

        // 验证 Pop 事件列表
        let pop_events = EventScenarioMapping::pop_events();
        assert!(pop_events.contains(&DialogueEventPayloadKind::ClarificationAnswered));
        assert!(pop_events.contains(&DialogueEventPayloadKind::ToolInvocationCompleted));
        assert!(pop_events.contains(&DialogueEventPayloadKind::CollabResolved));

        // 验证事件分类
        assert!(EventScenarioMapping::is_scenario_switching_event(
            DialogueEventPayloadKind::ClarificationIssued
        ));
        assert!(!EventScenarioMapping::is_scenario_switching_event(
            DialogueEventPayloadKind::MessagePrimary
        ));
    }

    #[test]
    fn test_get_stack_stats() {
        let manager = ScenarioStackManager::new();
        let mut session = create_test_session();

        // 添加多种场景
        manager.apply_event(&mut session, DialogueEventPayloadKind::ClarificationIssued);
        manager.apply_event(&mut session, DialogueEventPayloadKind::ToolInvocationScheduled);
        manager.apply_event(&mut session, DialogueEventPayloadKind::CollabRequested);

        let stats = manager.get_stack_stats(&session);
        assert_eq!(stats.current_depth, 3);
        assert_eq!(stats.human_to_ai_count, 1);
        assert_eq!(stats.ai_to_system_count, 1);
        assert_eq!(stats.ai_to_ai_count, 1);
    }
}
