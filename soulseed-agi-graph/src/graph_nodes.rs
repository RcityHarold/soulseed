//! 六大核心图谱节点 (SurrealDB 存储层)
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! 定义 SoulseedAGI 图谱中的六大核心节点类型的完整数据结构：
//! - DBActorNode: 参与者节点 (AI/Human/System)
//! - DBEventNode: 事件节点 (所有 DialogueEvent)
//! - DBMessageNode: 消息节点 (对话消息内容)
//! - DBSessionNode: 会话节点 (会话上下文)
//! - DBAcNode: 觉知周期节点 (Awareness Cycle)
//! - DBArtifactNode: 工件节点 (生成物)
//!
//! 注意：这些类型用于 SurrealDB 持久化存储，与 types.rs 中的轻量级引用类型不同

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// 节点特征 Trait
// ============================================================================

/// 图谱节点基础特征 (用于 DB 存储层)
pub trait DBGraphNode {
    /// 获取节点 ID
    fn node_id(&self) -> &str;
    /// 获取节点类型
    fn node_type(&self) -> DBNodeType;
    /// 获取租户 ID
    fn tenant_id(&self) -> u64;
    /// 获取创建时间戳
    fn created_at_ms(&self) -> i64;
    /// 转换为通用节点数据
    fn to_node_data(&self) -> DBNodeData;
}

/// 节点类型枚举 (DB 存储层)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DBNodeType {
    Actor,
    Event,
    Message,
    Session,
    AwarenessCycle,
    Artifact,
}

impl std::fmt::Display for DBNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DBNodeType::Actor => write!(f, "actor"),
            DBNodeType::Event => write!(f, "event"),
            DBNodeType::Message => write!(f, "message"),
            DBNodeType::Session => write!(f, "session"),
            DBNodeType::AwarenessCycle => write!(f, "awareness_cycle"),
            DBNodeType::Artifact => write!(f, "artifact"),
        }
    }
}

/// 通用节点数据结构 (DB 存储层)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBNodeData {
    pub node_id: String,
    pub node_type: DBNodeType,
    pub tenant_id: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: Option<i64>,
    pub labels: Vec<String>,
    pub properties: HashMap<String, serde_json::Value>,
}

// ============================================================================
// 1. Actor 节点 - 参与者
// ============================================================================

/// 参与者类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// 人类用户
    Human,
    /// AI 智能体
    AI,
    /// 系统服务
    System,
    /// 外部服务
    External,
}

/// Actor 节点 - 表示对话中的参与者 (DB 存储层)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBActorNode {
    /// 节点 ID
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 参与者类型
    pub kind: ActorKind,
    /// 内部 ID (根据类型: human_id, ai_id, system_id)
    pub internal_id: u64,
    /// 显示名称
    pub display_name: Option<String>,
    /// 角色标签
    pub role: Option<String>,
    /// 能力列表
    pub capabilities: Vec<String>,
    /// 状态
    pub status: ActorStatus,
    /// 创建时间
    pub created_at_ms: i64,
    /// 最后活动时间
    pub last_active_at_ms: Option<i64>,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Actor 状态
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActorStatus {
    #[default]
    Active,
    Inactive,
    Suspended,
    Archived,
}

impl DBActorNode {
    pub fn new_human(tenant_id: u64, human_id: u64) -> Self {
        Self {
            node_id: format!("actor:human:{}", human_id),
            tenant_id,
            kind: ActorKind::Human,
            internal_id: human_id,
            display_name: None,
            role: None,
            capabilities: vec![],
            status: ActorStatus::Active,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            last_active_at_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn new_ai(tenant_id: u64, ai_id: u64) -> Self {
        Self {
            node_id: format!("actor:ai:{}", ai_id),
            tenant_id,
            kind: ActorKind::AI,
            internal_id: ai_id,
            display_name: None,
            role: None,
            capabilities: vec!["reasoning".into(), "tool_use".into(), "collaboration".into()],
            status: ActorStatus::Active,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            last_active_at_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn new_system(tenant_id: u64, system_id: u64) -> Self {
        Self {
            node_id: format!("actor:system:{}", system_id),
            tenant_id,
            kind: ActorKind::System,
            internal_id: system_id,
            display_name: Some("System".into()),
            role: Some("orchestrator".into()),
            capabilities: vec!["scheduling".into(), "monitoring".into()],
            status: ActorStatus::Active,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            last_active_at_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}

impl DBGraphNode for DBActorNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::Actor
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("kind".into(), serde_json::json!(self.kind));
        properties.insert("internal_id".into(), serde_json::json!(self.internal_id));
        if let Some(ref name) = self.display_name {
            properties.insert("display_name".into(), serde_json::json!(name));
        }
        if let Some(ref role) = self.role {
            properties.insert("role".into(), serde_json::json!(role));
        }
        properties.insert("capabilities".into(), serde_json::json!(self.capabilities));
        properties.insert("status".into(), serde_json::json!(self.status));

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::Actor,
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.last_active_at_ms,
            labels: vec![format!("{:?}", self.kind).to_lowercase()],
            properties,
        }
    }
}

// ============================================================================
// 2. Event 节点 - 对话事件
// ============================================================================

/// Event 节点 - 表示对话中的事件
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBEventNode {
    /// 节点 ID (event_id)
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 事件类型
    pub event_type: String,
    /// 所属会话 ID
    pub session_id: String,
    /// 所属 AC ID
    pub ac_id: Option<u64>,
    /// 触发者 Actor ID
    pub actor_id: String,
    /// 发生时间
    pub occurred_at_ms: i64,
    /// 序列号
    pub sequence_number: u64,
    /// 场景
    pub scenario: Option<String>,
    /// 触发事件 ID (因果关系)
    pub trigger_event_id: Option<String>,
    /// 访问级别
    pub access_class: String,
    /// Payload 摘要 (不存储完整 payload)
    pub payload_digest: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBEventNode {
    pub fn new(
        tenant_id: u64,
        event_id: impl Into<String>,
        event_type: impl Into<String>,
        session_id: impl Into<String>,
        actor_id: impl Into<String>,
    ) -> Self {
        Self {
            node_id: event_id.into(),
            tenant_id,
            event_type: event_type.into(),
            session_id: session_id.into(),
            ac_id: None,
            actor_id: actor_id.into(),
            occurred_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            sequence_number: 0,
            scenario: None,
            trigger_event_id: None,
            access_class: "internal".into(),
            payload_digest: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_ac_id(mut self, ac_id: u64) -> Self {
        self.ac_id = Some(ac_id);
        self
    }

    pub fn with_trigger(mut self, trigger_event_id: impl Into<String>) -> Self {
        self.trigger_event_id = Some(trigger_event_id.into());
        self
    }

    pub fn with_scenario(mut self, scenario: impl Into<String>) -> Self {
        self.scenario = Some(scenario.into());
        self
    }
}

impl DBGraphNode for DBEventNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::Event
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("event_type".into(), serde_json::json!(self.event_type));
        properties.insert("session_id".into(), serde_json::json!(self.session_id));
        properties.insert("actor_id".into(), serde_json::json!(self.actor_id));
        properties.insert("sequence_number".into(), serde_json::json!(self.sequence_number));
        properties.insert("access_class".into(), serde_json::json!(self.access_class));
        if let Some(ref ac_id) = self.ac_id {
            properties.insert("ac_id".into(), serde_json::json!(ac_id));
        }
        if let Some(ref scenario) = self.scenario {
            properties.insert("scenario".into(), serde_json::json!(scenario));
        }
        if let Some(ref trigger) = self.trigger_event_id {
            properties.insert("trigger_event_id".into(), serde_json::json!(trigger));
        }
        if let Some(ref digest) = self.payload_digest {
            properties.insert("payload_digest".into(), serde_json::json!(digest));
        }

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::Event,
            tenant_id: self.tenant_id,
            created_at_ms: self.occurred_at_ms,
            updated_at_ms: None,
            labels: vec![self.event_type.clone()],
            properties,
        }
    }
}

// ============================================================================
// 3. Message 节点 - 对话消息
// ============================================================================

/// 消息内容类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageContentType {
    Text,
    Code,
    Image,
    Audio,
    Video,
    File,
    Mixed,
}

/// Message 节点 - 表示对话中的消息内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBMessageNode {
    /// 节点 ID (message_id)
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 关联的事件 ID
    pub event_id: String,
    /// 所属会话 ID
    pub session_id: String,
    /// 发送者 Actor ID
    pub sender_id: String,
    /// 内容类型
    pub content_type: MessageContentType,
    /// 内容摘要 (不存储完整内容)
    pub content_summary: Option<String>,
    /// 内容 SHA256 摘要
    pub content_digest: Option<String>,
    /// Token 数量
    pub token_count: Option<u32>,
    /// 语言
    pub language: Option<String>,
    /// 情感分析结果
    pub sentiment: Option<String>,
    /// 是否为最终版本
    pub is_final: bool,
    /// 回复目标消息 ID
    pub reply_to: Option<String>,
    /// 创建时间
    pub created_at_ms: i64,
    /// 编辑时间
    pub edited_at_ms: Option<i64>,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBMessageNode {
    pub fn new(
        tenant_id: u64,
        message_id: impl Into<String>,
        event_id: impl Into<String>,
        session_id: impl Into<String>,
        sender_id: impl Into<String>,
    ) -> Self {
        Self {
            node_id: message_id.into(),
            tenant_id,
            event_id: event_id.into(),
            session_id: session_id.into(),
            sender_id: sender_id.into(),
            content_type: MessageContentType::Text,
            content_summary: None,
            content_digest: None,
            token_count: None,
            language: None,
            sentiment: None,
            is_final: true,
            reply_to: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            edited_at_ms: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_content_type(mut self, content_type: MessageContentType) -> Self {
        self.content_type = content_type;
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.content_summary = Some(summary.into());
        self
    }

    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }
}

impl DBGraphNode for DBMessageNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::Message
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("event_id".into(), serde_json::json!(self.event_id));
        properties.insert("session_id".into(), serde_json::json!(self.session_id));
        properties.insert("sender_id".into(), serde_json::json!(self.sender_id));
        properties.insert("content_type".into(), serde_json::json!(self.content_type));
        properties.insert("is_final".into(), serde_json::json!(self.is_final));
        if let Some(ref summary) = self.content_summary {
            properties.insert("content_summary".into(), serde_json::json!(summary));
        }
        if let Some(ref digest) = self.content_digest {
            properties.insert("content_digest".into(), serde_json::json!(digest));
        }
        if let Some(count) = self.token_count {
            properties.insert("token_count".into(), serde_json::json!(count));
        }
        if let Some(ref lang) = self.language {
            properties.insert("language".into(), serde_json::json!(lang));
        }
        if let Some(ref reply) = self.reply_to {
            properties.insert("reply_to".into(), serde_json::json!(reply));
        }

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::Message,
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.edited_at_ms,
            labels: vec![format!("{:?}", self.content_type).to_lowercase()],
            properties,
        }
    }
}

// ============================================================================
// 4. Session 节点 - 会话
// ============================================================================

/// Session 状态
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    #[default]
    Active,
    Paused,
    Completed,
    Abandoned,
    Archived,
}

/// Session 节点 - 表示对话会话
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBSessionNode {
    /// 节点 ID (session_id)
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 会话模式
    pub mode: String,
    /// 主题/场景
    pub scenario: Option<String>,
    /// 创建者 Actor ID
    pub creator_id: String,
    /// 参与者 Actor IDs
    pub participant_ids: Vec<String>,
    /// 状态
    pub status: SessionStatus,
    /// 创建时间
    pub created_at_ms: i64,
    /// 最后活动时间
    pub last_activity_at_ms: Option<i64>,
    /// 结束时间
    pub ended_at_ms: Option<i64>,
    /// 事件计数
    pub event_count: u32,
    /// 消息计数
    pub message_count: u32,
    /// AC 计数
    pub ac_count: u32,
    /// 场景栈深度
    pub scenario_stack_depth: u32,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBSessionNode {
    pub fn new(
        tenant_id: u64,
        session_id: impl Into<String>,
        creator_id: impl Into<String>,
    ) -> Self {
        Self {
            node_id: session_id.into(),
            tenant_id,
            mode: "interactive".into(),
            scenario: None,
            creator_id: creator_id.into(),
            participant_ids: vec![],
            status: SessionStatus::Active,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            last_activity_at_ms: None,
            ended_at_ms: None,
            event_count: 0,
            message_count: 0,
            ac_count: 0,
            scenario_stack_depth: 0,
            metadata: HashMap::new(),
        }
    }

    pub fn with_mode(mut self, mode: impl Into<String>) -> Self {
        self.mode = mode.into();
        self
    }

    pub fn with_scenario(mut self, scenario: impl Into<String>) -> Self {
        self.scenario = Some(scenario.into());
        self
    }

    pub fn add_participant(&mut self, actor_id: impl Into<String>) {
        self.participant_ids.push(actor_id.into());
    }

    pub fn increment_event_count(&mut self) {
        self.event_count += 1;
        self.last_activity_at_ms = Some(time::OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }

    pub fn increment_message_count(&mut self) {
        self.message_count += 1;
    }

    pub fn increment_ac_count(&mut self) {
        self.ac_count += 1;
    }
}

impl DBGraphNode for DBSessionNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::Session
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("mode".into(), serde_json::json!(self.mode));
        properties.insert("creator_id".into(), serde_json::json!(self.creator_id));
        properties.insert("participant_ids".into(), serde_json::json!(self.participant_ids));
        properties.insert("status".into(), serde_json::json!(self.status));
        properties.insert("event_count".into(), serde_json::json!(self.event_count));
        properties.insert("message_count".into(), serde_json::json!(self.message_count));
        properties.insert("ac_count".into(), serde_json::json!(self.ac_count));
        properties.insert("scenario_stack_depth".into(), serde_json::json!(self.scenario_stack_depth));
        if let Some(ref scenario) = self.scenario {
            properties.insert("scenario".into(), serde_json::json!(scenario));
        }
        if let Some(last) = self.last_activity_at_ms {
            properties.insert("last_activity_at_ms".into(), serde_json::json!(last));
        }
        if let Some(ended) = self.ended_at_ms {
            properties.insert("ended_at_ms".into(), serde_json::json!(ended));
        }

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::Session,
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.last_activity_at_ms,
            labels: vec![self.mode.clone(), format!("{:?}", self.status).to_lowercase()],
            properties,
        }
    }
}

// ============================================================================
// 5. AC 节点 - 觉知周期
// ============================================================================

/// AC 状态
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ACStatus {
    #[default]
    Pending,
    Running,
    AwaitingExternal,
    Suspended,
    Completed,
    Failed,
    Timeout,
}

/// AC 决策路径
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionLane {
    SelfReason,
    Clarify,
    ToolPath,
    Collab,
}

/// AC 节点 - 表示觉知周期
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBAcNode {
    /// 节点 ID (ac_id)
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 所属会话 ID
    pub session_id: String,
    /// 父 AC ID
    pub parent_ac_id: Option<String>,
    /// 决策路径
    pub lane: DecisionLane,
    /// IC 数量
    pub ic_count: u32,
    /// 状态
    pub status: ACStatus,
    /// 开始时间
    pub started_at_ms: i64,
    /// 结束时间
    pub ended_at_ms: Option<i64>,
    /// 执行时长 (毫秒)
    pub duration_ms: Option<u64>,
    /// Token 消耗
    pub tokens_spent: u64,
    /// 外部成本
    pub external_cost: f64,
    /// 决策置信度
    pub decision_confidence: Option<f32>,
    /// 协作范围 ID
    pub collab_scope_id: Option<String>,
    /// 触发事件 ID
    pub trigger_event_id: Option<String>,
    /// 最终事件 ID
    pub final_event_id: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBAcNode {
    pub fn new(
        tenant_id: u64,
        ac_id: u64,
        session_id: impl Into<String>,
        lane: DecisionLane,
    ) -> Self {
        Self {
            node_id: format!("ac:{}", ac_id),
            tenant_id,
            session_id: session_id.into(),
            parent_ac_id: None,
            lane,
            ic_count: 0,
            status: ACStatus::Pending,
            started_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            ended_at_ms: None,
            duration_ms: None,
            tokens_spent: 0,
            external_cost: 0.0,
            decision_confidence: None,
            collab_scope_id: None,
            trigger_event_id: None,
            final_event_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_parent(mut self, parent_ac_id: impl Into<String>) -> Self {
        self.parent_ac_id = Some(parent_ac_id.into());
        self
    }

    pub fn start(&mut self) {
        self.status = ACStatus::Running;
        self.started_at_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
    }

    pub fn complete(&mut self, final_event_id: impl Into<String>) {
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        self.status = ACStatus::Completed;
        self.ended_at_ms = Some(now);
        self.duration_ms = Some((now - self.started_at_ms) as u64);
        self.final_event_id = Some(final_event_id.into());
    }

    pub fn fail(&mut self) {
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        self.status = ACStatus::Failed;
        self.ended_at_ms = Some(now);
        self.duration_ms = Some((now - self.started_at_ms) as u64);
    }

    pub fn increment_ic(&mut self) {
        self.ic_count += 1;
    }

    pub fn add_cost(&mut self, tokens: u64, external_cost: f64) {
        self.tokens_spent += tokens;
        self.external_cost += external_cost;
    }
}

impl DBGraphNode for DBAcNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::AwarenessCycle
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("session_id".into(), serde_json::json!(self.session_id));
        properties.insert("lane".into(), serde_json::json!(self.lane));
        properties.insert("ic_count".into(), serde_json::json!(self.ic_count));
        properties.insert("status".into(), serde_json::json!(self.status));
        properties.insert("tokens_spent".into(), serde_json::json!(self.tokens_spent));
        properties.insert("external_cost".into(), serde_json::json!(self.external_cost));
        if let Some(ref parent) = self.parent_ac_id {
            properties.insert("parent_ac_id".into(), serde_json::json!(parent));
        }
        if let Some(ended) = self.ended_at_ms {
            properties.insert("ended_at_ms".into(), serde_json::json!(ended));
        }
        if let Some(duration) = self.duration_ms {
            properties.insert("duration_ms".into(), serde_json::json!(duration));
        }
        if let Some(conf) = self.decision_confidence {
            properties.insert("decision_confidence".into(), serde_json::json!(conf));
        }
        if let Some(ref scope) = self.collab_scope_id {
            properties.insert("collab_scope_id".into(), serde_json::json!(scope));
        }
        if let Some(ref trigger) = self.trigger_event_id {
            properties.insert("trigger_event_id".into(), serde_json::json!(trigger));
        }
        if let Some(ref final_evt) = self.final_event_id {
            properties.insert("final_event_id".into(), serde_json::json!(final_evt));
        }

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::AwarenessCycle,
            tenant_id: self.tenant_id,
            created_at_ms: self.started_at_ms,
            updated_at_ms: self.ended_at_ms,
            labels: vec![
                format!("{:?}", self.lane).to_lowercase(),
                format!("{:?}", self.status).to_lowercase(),
            ],
            properties,
        }
    }
}

// ============================================================================
// 6. Artifact 节点 - 工件/生成物
// ============================================================================

/// 工件类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// 代码
    Code,
    /// 文档
    Document,
    /// 图像
    Image,
    /// 数据文件
    Data,
    /// 配置
    Config,
    /// 报告
    Report,
    /// 摘要
    Summary,
    /// 其他
    Other,
}

/// Artifact 节点 - 表示对话产生的工件
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBArtifactNode {
    /// 节点 ID (artifact_id)
    pub node_id: String,
    /// 租户 ID
    pub tenant_id: u64,
    /// 工件类型
    pub kind: ArtifactKind,
    /// 标题/名称
    pub title: String,
    /// 描述
    pub description: Option<String>,
    /// 生成事件 ID
    pub source_event_id: String,
    /// 所属会话 ID
    pub session_id: String,
    /// 创建者 Actor ID
    pub creator_id: String,
    /// 内容 SHA256 摘要
    pub content_digest: Option<String>,
    /// Blob 引用
    pub blob_ref: Option<String>,
    /// 文件大小 (字节)
    pub size_bytes: Option<u64>,
    /// MIME 类型
    pub mime_type: Option<String>,
    /// 版本号
    pub version: u32,
    /// 前驱版本 ID
    pub supersedes: Option<String>,
    /// 创建时间
    pub created_at_ms: i64,
    /// 更新时间
    pub updated_at_ms: Option<i64>,
    /// 标签
    pub tags: Vec<String>,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBArtifactNode {
    pub fn new(
        tenant_id: u64,
        title: impl Into<String>,
        kind: ArtifactKind,
        source_event_id: impl Into<String>,
        session_id: impl Into<String>,
        creator_id: impl Into<String>,
    ) -> Self {
        Self {
            node_id: format!("artifact:{}", Uuid::new_v4()),
            tenant_id,
            kind,
            title: title.into(),
            description: None,
            source_event_id: source_event_id.into(),
            session_id: session_id.into(),
            creator_id: creator_id.into(),
            content_digest: None,
            blob_ref: None,
            size_bytes: None,
            mime_type: None,
            version: 1,
            supersedes: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            updated_at_ms: None,
            tags: vec![],
            metadata: HashMap::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_blob(mut self, blob_ref: impl Into<String>, size_bytes: u64) -> Self {
        self.blob_ref = Some(blob_ref.into());
        self.size_bytes = Some(size_bytes);
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.push(tag.into());
    }

    /// 创建新版本
    pub fn create_new_version(&self) -> Self {
        let mut new_artifact = self.clone();
        new_artifact.node_id = format!("artifact:{}", Uuid::new_v4());
        new_artifact.version = self.version + 1;
        new_artifact.supersedes = Some(self.node_id.clone());
        new_artifact.created_at_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        new_artifact.updated_at_ms = None;
        new_artifact
    }
}

impl DBGraphNode for DBArtifactNode {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn node_type(&self) -> DBNodeType {
        DBNodeType::Artifact
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_node_data(&self) -> DBNodeData {
        let mut properties = self.metadata.clone();
        properties.insert("kind".into(), serde_json::json!(self.kind));
        properties.insert("title".into(), serde_json::json!(self.title));
        properties.insert("source_event_id".into(), serde_json::json!(self.source_event_id));
        properties.insert("session_id".into(), serde_json::json!(self.session_id));
        properties.insert("creator_id".into(), serde_json::json!(self.creator_id));
        properties.insert("version".into(), serde_json::json!(self.version));
        properties.insert("tags".into(), serde_json::json!(self.tags));
        if let Some(ref desc) = self.description {
            properties.insert("description".into(), serde_json::json!(desc));
        }
        if let Some(ref digest) = self.content_digest {
            properties.insert("content_digest".into(), serde_json::json!(digest));
        }
        if let Some(ref blob) = self.blob_ref {
            properties.insert("blob_ref".into(), serde_json::json!(blob));
        }
        if let Some(size) = self.size_bytes {
            properties.insert("size_bytes".into(), serde_json::json!(size));
        }
        if let Some(ref mime) = self.mime_type {
            properties.insert("mime_type".into(), serde_json::json!(mime));
        }
        if let Some(ref supersedes) = self.supersedes {
            properties.insert("supersedes".into(), serde_json::json!(supersedes));
        }

        DBNodeData {
            node_id: self.node_id.clone(),
            node_type: DBNodeType::Artifact,
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            labels: {
                let mut labels = vec![format!("{:?}", self.kind).to_lowercase()];
                labels.extend(self.tags.clone());
                labels
            },
            properties,
        }
    }
}

// ============================================================================
// 节点存储服务
// ============================================================================

/// 节点存储服务 Trait
#[allow(async_fn_in_trait)]
pub trait DBNodeStore {
    /// 存储节点
    async fn store_node(&self, node: &dyn DBGraphNode) -> Result<(), DBNodeStoreError>;
    /// 获取节点
    async fn get_node(&self, node_id: &str) -> Result<Option<DBNodeData>, DBNodeStoreError>;
    /// 删除节点
    async fn delete_node(&self, node_id: &str) -> Result<bool, DBNodeStoreError>;
    /// 按类型查询节点
    async fn query_by_type(
        &self,
        tenant_id: u64,
        node_type: DBNodeType,
        limit: usize,
    ) -> Result<Vec<DBNodeData>, DBNodeStoreError>;
}

/// 节点存储错误
#[derive(Debug, thiserror::Error)]
pub enum DBNodeStoreError {
    #[error("Node not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ============================================================================
// SurrealDB Schema 定义
// ============================================================================

/// 生成 SurrealDB 节点表定义
pub struct NodeSchemaGenerator;

impl NodeSchemaGenerator {
    /// 生成所有节点表的 SurrealQL
    pub fn generate_schema() -> String {
        r#"
-- =====================
-- Graph Nodes Schema
-- =====================

-- Actor 节点表
DEFINE TABLE graph_actor SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_actor TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_actor TYPE string ASSERT $value != "";
DEFINE FIELD kind ON graph_actor TYPE string;
DEFINE FIELD internal_id ON graph_actor TYPE number;
DEFINE FIELD display_name ON graph_actor TYPE option<string>;
DEFINE FIELD role ON graph_actor TYPE option<string>;
DEFINE FIELD capabilities ON graph_actor TYPE array DEFAULT [];
DEFINE FIELD status ON graph_actor TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON graph_actor TYPE number;
DEFINE FIELD last_active_at_ms ON graph_actor TYPE option<number>;
DEFINE FIELD metadata ON graph_actor FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_actor_lookup ON TABLE graph_actor FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_actor_kind ON TABLE graph_actor FIELDS tenant_id, kind;

-- Event 节点表
DEFINE TABLE graph_event SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_event TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_event TYPE string ASSERT $value != "";
DEFINE FIELD event_type ON graph_event TYPE string;
DEFINE FIELD session_id ON graph_event TYPE string;
DEFINE FIELD ac_id ON graph_event TYPE option<number>;
DEFINE FIELD actor_id ON graph_event TYPE string;
DEFINE FIELD occurred_at_ms ON graph_event TYPE number;
DEFINE FIELD sequence_number ON graph_event TYPE number DEFAULT 0;
DEFINE FIELD scenario ON graph_event TYPE option<string>;
DEFINE FIELD trigger_event_id ON graph_event TYPE option<string>;
DEFINE FIELD access_class ON graph_event TYPE string DEFAULT 'internal';
DEFINE FIELD payload_digest ON graph_event TYPE option<string>;
DEFINE FIELD metadata ON graph_event FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_event_lookup ON TABLE graph_event FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_event_session ON TABLE graph_event FIELDS tenant_id, session_id, occurred_at_ms;
DEFINE INDEX idx_graph_event_type ON TABLE graph_event FIELDS tenant_id, event_type;
DEFINE INDEX idx_graph_event_actor ON TABLE graph_event FIELDS tenant_id, actor_id;

-- Message 节点表
DEFINE TABLE graph_message SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_message TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_message TYPE string ASSERT $value != "";
DEFINE FIELD event_id ON graph_message TYPE string;
DEFINE FIELD session_id ON graph_message TYPE string;
DEFINE FIELD sender_id ON graph_message TYPE string;
DEFINE FIELD content_type ON graph_message TYPE string DEFAULT 'text';
DEFINE FIELD content_summary ON graph_message TYPE option<string>;
DEFINE FIELD content_digest ON graph_message TYPE option<string>;
DEFINE FIELD token_count ON graph_message TYPE option<number>;
DEFINE FIELD language ON graph_message TYPE option<string>;
DEFINE FIELD sentiment ON graph_message TYPE option<string>;
DEFINE FIELD is_final ON graph_message TYPE bool DEFAULT true;
DEFINE FIELD reply_to ON graph_message TYPE option<string>;
DEFINE FIELD created_at_ms ON graph_message TYPE number;
DEFINE FIELD edited_at_ms ON graph_message TYPE option<number>;
DEFINE FIELD metadata ON graph_message FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_message_lookup ON TABLE graph_message FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_message_session ON TABLE graph_message FIELDS tenant_id, session_id, created_at_ms;
DEFINE INDEX idx_graph_message_sender ON TABLE graph_message FIELDS tenant_id, sender_id;

-- Session 节点表
DEFINE TABLE graph_session SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_session TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_session TYPE string ASSERT $value != "";
DEFINE FIELD mode ON graph_session TYPE string DEFAULT 'interactive';
DEFINE FIELD scenario ON graph_session TYPE option<string>;
DEFINE FIELD creator_id ON graph_session TYPE string;
DEFINE FIELD participant_ids ON graph_session TYPE array DEFAULT [];
DEFINE FIELD status ON graph_session TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON graph_session TYPE number;
DEFINE FIELD last_activity_at_ms ON graph_session TYPE option<number>;
DEFINE FIELD ended_at_ms ON graph_session TYPE option<number>;
DEFINE FIELD event_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD message_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD ac_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD scenario_stack_depth ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD metadata ON graph_session FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_session_lookup ON TABLE graph_session FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_session_status ON TABLE graph_session FIELDS tenant_id, status;
DEFINE INDEX idx_graph_session_creator ON TABLE graph_session FIELDS tenant_id, creator_id;

-- AC 节点表
DEFINE TABLE graph_ac SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_ac TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_ac TYPE string ASSERT $value != "";
DEFINE FIELD session_id ON graph_ac TYPE string;
DEFINE FIELD parent_ac_id ON graph_ac TYPE option<string>;
DEFINE FIELD lane ON graph_ac TYPE string;
DEFINE FIELD ic_count ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD status ON graph_ac TYPE string DEFAULT 'pending';
DEFINE FIELD started_at_ms ON graph_ac TYPE number;
DEFINE FIELD ended_at_ms ON graph_ac TYPE option<number>;
DEFINE FIELD duration_ms ON graph_ac TYPE option<number>;
DEFINE FIELD tokens_spent ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD external_cost ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD decision_confidence ON graph_ac TYPE option<number>;
DEFINE FIELD collab_scope_id ON graph_ac TYPE option<string>;
DEFINE FIELD trigger_event_id ON graph_ac TYPE option<string>;
DEFINE FIELD final_event_id ON graph_ac TYPE option<string>;
DEFINE FIELD metadata ON graph_ac FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_ac_lookup ON TABLE graph_ac FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_ac_session ON TABLE graph_ac FIELDS tenant_id, session_id;
DEFINE INDEX idx_graph_ac_lane ON TABLE graph_ac FIELDS tenant_id, lane;
DEFINE INDEX idx_graph_ac_status ON TABLE graph_ac FIELDS tenant_id, status;
DEFINE INDEX idx_graph_ac_parent ON TABLE graph_ac FIELDS tenant_id, parent_ac_id;

-- Artifact 节点表
DEFINE TABLE graph_artifact SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_artifact TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_artifact TYPE string ASSERT $value != "";
DEFINE FIELD kind ON graph_artifact TYPE string;
DEFINE FIELD title ON graph_artifact TYPE string;
DEFINE FIELD description ON graph_artifact TYPE option<string>;
DEFINE FIELD source_event_id ON graph_artifact TYPE string;
DEFINE FIELD session_id ON graph_artifact TYPE string;
DEFINE FIELD creator_id ON graph_artifact TYPE string;
DEFINE FIELD content_digest ON graph_artifact TYPE option<string>;
DEFINE FIELD blob_ref ON graph_artifact TYPE option<string>;
DEFINE FIELD size_bytes ON graph_artifact TYPE option<number>;
DEFINE FIELD mime_type ON graph_artifact TYPE option<string>;
DEFINE FIELD version ON graph_artifact TYPE number DEFAULT 1;
DEFINE FIELD supersedes ON graph_artifact TYPE option<string>;
DEFINE FIELD created_at_ms ON graph_artifact TYPE number;
DEFINE FIELD updated_at_ms ON graph_artifact TYPE option<number>;
DEFINE FIELD tags ON graph_artifact TYPE array DEFAULT [];
DEFINE FIELD metadata ON graph_artifact FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_artifact_lookup ON TABLE graph_artifact FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_artifact_session ON TABLE graph_artifact FIELDS tenant_id, session_id;
DEFINE INDEX idx_graph_artifact_kind ON TABLE graph_artifact FIELDS tenant_id, kind;
DEFINE INDEX idx_graph_artifact_creator ON TABLE graph_artifact FIELDS tenant_id, creator_id;
"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_node_creation() {
        let actor = DBActorNode::new_human(1, 42)
            .with_display_name("Alice")
            .with_role("developer");

        assert_eq!(actor.node_id, "actor:human:42");
        assert_eq!(actor.kind, ActorKind::Human);
        assert_eq!(actor.display_name, Some("Alice".into()));
        assert_eq!(actor.role, Some("developer".into()));
    }

    #[test]
    fn test_event_node_creation() {
        let event = DBEventNode::new(1, "evt-1", "message_primary", "session-1", "actor:human:1")
            .with_ac_id(100)
            .with_trigger("evt-0");

        assert_eq!(event.node_id, "evt-1");
        assert_eq!(event.event_type, "message_primary");
        assert_eq!(event.ac_id, Some(100));
        assert_eq!(event.trigger_event_id, Some("evt-0".into()));
    }

    #[test]
    fn test_message_node_creation() {
        let msg = DBMessageNode::new(1, "msg-1", "evt-1", "session-1", "actor:human:1")
            .with_content_type(MessageContentType::Code)
            .with_summary("A Python function");

        assert_eq!(msg.node_id, "msg-1");
        assert_eq!(msg.content_type, MessageContentType::Code);
        assert_eq!(msg.content_summary, Some("A Python function".into()));
    }

    #[test]
    fn test_session_node_creation() {
        let mut session = DBSessionNode::new(1, "session-1", "actor:human:1")
            .with_mode("autonomous_continuation")
            .with_scenario("ai_to_ai");

        session.add_participant("actor:ai:1");
        session.increment_event_count();
        session.increment_ac_count();

        assert_eq!(session.mode, "autonomous_continuation");
        assert_eq!(session.participant_ids.len(), 1);
        assert_eq!(session.event_count, 1);
        assert_eq!(session.ac_count, 1);
    }

    #[test]
    fn test_ac_node_lifecycle() {
        let mut ac = DBAcNode::new(1, 100, "session-1", DecisionLane::SelfReason);
        assert_eq!(ac.status, ACStatus::Pending);

        ac.start();
        assert_eq!(ac.status, ACStatus::Running);

        ac.increment_ic();
        ac.add_cost(500, 0.01);
        assert_eq!(ac.ic_count, 1);
        assert_eq!(ac.tokens_spent, 500);

        ac.complete("evt-final");
        assert_eq!(ac.status, ACStatus::Completed);
        assert!(ac.duration_ms.is_some());
    }

    #[test]
    fn test_artifact_versioning() {
        let v1 = DBArtifactNode::new(
            1,
            "My Document",
            ArtifactKind::Document,
            "evt-1",
            "session-1",
            "actor:ai:1",
        );
        assert_eq!(v1.version, 1);

        let v2 = v1.create_new_version();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.supersedes, Some(v1.node_id.clone()));
    }

    #[test]
    fn test_node_to_data_conversion() {
        let actor = DBActorNode::new_ai(1, 1);
        let data = actor.to_node_data();

        assert_eq!(data.node_type, DBNodeType::Actor);
        assert!(data.properties.contains_key("kind"));
        assert!(data.properties.contains_key("capabilities"));
    }
}
