//! 七大关系边族群 (SurrealDB 存储层)
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! 定义 SoulseedAGI 图谱中的七大关系边族群：
//! 1. 因果边族群: TRIGGERED_BY, CAUSED_BY, ENABLED, LED_TO
//! 2. 归属边族群: BELONGS_TO, PART_OF, MEMBER_OF
//! 3. 引用边族群: REFERENCES, MENTIONS, RESPONDS_TO
//! 4. 协作边族群: COLLABORATES_WITH, REQUESTS_HELP_FROM, ASSISTS
//! 5. 版本边族群: SUPERSEDES, DERIVED_FROM, EVOLVED_INTO
//! 6. 影响边族群: INFLUENCES, IMPACTS, AFFECTS
//! 7. 评价边族群: EVALUATES, RATES, CRITIQUES
//!
//! 注意：这些类型用于 SurrealDB 持久化存储，与 types.rs 中的轻量级引用类型不同

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// 边基础特征 Trait
// ============================================================================

/// 图谱边基础特征 (用于 DB 存储层)
pub trait DBGraphEdge {
    /// 获取边 ID
    fn edge_id(&self) -> &str;
    /// 获取边类型
    fn edge_type(&self) -> DBEdgeType;
    /// 获取源节点 ID
    fn from_node_id(&self) -> &str;
    /// 获取目标节点 ID
    fn to_node_id(&self) -> &str;
    /// 获取租户 ID
    fn tenant_id(&self) -> u64;
    /// 获取创建时间戳
    fn created_at_ms(&self) -> i64;
    /// 转换为通用边数据
    fn to_edge_data(&self) -> DBEdgeData;
}

// ============================================================================
// 边类型定义
// ============================================================================

/// 边族群分类
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DBEdgeFamily {
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

/// 边类型枚举 - 七大族群的所有边类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DBEdgeType {
    // === 因果边族群 ===
    /// 由...触发 (event -> event)
    TriggeredBy,
    /// 由...导致 (event -> event)
    CausedBy,
    /// 使能 (event -> event)
    Enabled,
    /// 导致 (event -> event)
    LedTo,

    // === 归属边族群 ===
    /// 属于 (event -> session, message -> session, ac -> session)
    BelongsTo,
    /// 部分属于 (message -> event, ic -> ac)
    PartOf,
    /// 成员关系 (actor -> group)
    MemberOf,

    // === 引用边族群 ===
    /// 引用 (message -> artifact, event -> event)
    References,
    /// 提及 (message -> actor, message -> artifact)
    Mentions,
    /// 回复 (message -> message)
    RespondsTo,

    // === 协作边族群 ===
    /// 协作 (actor -> actor)
    CollaboratesWith,
    /// 请求帮助 (actor -> actor)
    RequestsHelpFrom,
    /// 协助 (actor -> actor)
    Assists,

    // === 版本边族群 ===
    /// 取代 (artifact -> artifact, session -> session)
    Supersedes,
    /// 派生自 (artifact -> artifact)
    DerivedFrom,
    /// 演化为 (artifact -> artifact)
    EvolvedInto,

    // === 影响边族群 ===
    /// 影响 (event -> event, actor -> decision)
    Influences,
    /// 冲击 (event -> state)
    Impacts,
    /// 作用于 (action -> entity)
    Affects,

    // === 评价边族群 ===
    /// 评估 (actor -> artifact, event -> event)
    Evaluates,
    /// 评分 (actor -> artifact)
    Rates,
    /// 批评 (actor -> artifact)
    Critiques,
}

impl DBEdgeType {
    /// 获取边所属的族群
    pub fn family(&self) -> DBEdgeFamily {
        match self {
            DBEdgeType::TriggeredBy
            | DBEdgeType::CausedBy
            | DBEdgeType::Enabled
            | DBEdgeType::LedTo => DBEdgeFamily::Causal,

            DBEdgeType::BelongsTo
            | DBEdgeType::PartOf
            | DBEdgeType::MemberOf => DBEdgeFamily::Belonging,

            DBEdgeType::References
            | DBEdgeType::Mentions
            | DBEdgeType::RespondsTo => DBEdgeFamily::Reference,

            DBEdgeType::CollaboratesWith
            | DBEdgeType::RequestsHelpFrom
            | DBEdgeType::Assists => DBEdgeFamily::Collaboration,

            DBEdgeType::Supersedes
            | DBEdgeType::DerivedFrom
            | DBEdgeType::EvolvedInto => DBEdgeFamily::Version,

            DBEdgeType::Influences
            | DBEdgeType::Impacts
            | DBEdgeType::Affects => DBEdgeFamily::Influence,

            DBEdgeType::Evaluates
            | DBEdgeType::Rates
            | DBEdgeType::Critiques => DBEdgeFamily::Evaluation,
        }
    }

    /// 获取 SurrealDB 边表名称
    pub fn table_name(&self) -> &'static str {
        match self {
            DBEdgeType::TriggeredBy => "triggered_by",
            DBEdgeType::CausedBy => "caused_by",
            DBEdgeType::Enabled => "enabled",
            DBEdgeType::LedTo => "led_to",
            DBEdgeType::BelongsTo => "belongs_to",
            DBEdgeType::PartOf => "part_of",
            DBEdgeType::MemberOf => "member_of",
            DBEdgeType::References => "references",
            DBEdgeType::Mentions => "mentions",
            DBEdgeType::RespondsTo => "responds_to",
            DBEdgeType::CollaboratesWith => "collaborates_with",
            DBEdgeType::RequestsHelpFrom => "requests_help_from",
            DBEdgeType::Assists => "assists",
            DBEdgeType::Supersedes => "supersedes",
            DBEdgeType::DerivedFrom => "derived_from",
            DBEdgeType::EvolvedInto => "evolved_into",
            DBEdgeType::Influences => "influences",
            DBEdgeType::Impacts => "impacts",
            DBEdgeType::Affects => "affects",
            DBEdgeType::Evaluates => "evaluates",
            DBEdgeType::Rates => "rates",
            DBEdgeType::Critiques => "critiques",
        }
    }

    /// 判断是否为有向边
    pub fn is_directed(&self) -> bool {
        // 所有边都是有向的
        true
    }

    /// 获取反向边类型
    pub fn inverse(&self) -> Option<DBEdgeType> {
        match self {
            DBEdgeType::TriggeredBy => Some(DBEdgeType::LedTo),
            DBEdgeType::LedTo => Some(DBEdgeType::TriggeredBy),
            DBEdgeType::CausedBy => Some(DBEdgeType::LedTo),
            DBEdgeType::Enabled => None,
            DBEdgeType::BelongsTo => None,
            DBEdgeType::PartOf => None,
            DBEdgeType::MemberOf => None,
            DBEdgeType::References => None,
            DBEdgeType::Mentions => None,
            DBEdgeType::RespondsTo => None,
            DBEdgeType::CollaboratesWith => Some(DBEdgeType::CollaboratesWith), // 对称
            DBEdgeType::RequestsHelpFrom => Some(DBEdgeType::Assists),
            DBEdgeType::Assists => Some(DBEdgeType::RequestsHelpFrom),
            DBEdgeType::Supersedes => None,
            DBEdgeType::DerivedFrom => Some(DBEdgeType::EvolvedInto),
            DBEdgeType::EvolvedInto => Some(DBEdgeType::DerivedFrom),
            DBEdgeType::Influences => None,
            DBEdgeType::Impacts => None,
            DBEdgeType::Affects => None,
            DBEdgeType::Evaluates => None,
            DBEdgeType::Rates => None,
            DBEdgeType::Critiques => None,
        }
    }
}

impl std::fmt::Display for DBEdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.table_name())
    }
}

// ============================================================================
// 通用边数据结构
// ============================================================================

/// 通用边数据结构
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBEdgeData {
    pub edge_id: String,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    pub tenant_id: u64,
    pub created_at_ms: i64,
    pub weight: Option<f32>,
    pub confidence: Option<f32>,
    pub labels: Vec<String>,
    pub properties: HashMap<String, serde_json::Value>,
}

// ============================================================================
// 1. 因果边族群
// ============================================================================

/// 因果关系边 - 用于描述事件之间的因果关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBCausalEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_event_id: String,
    pub to_event_id: String,
    /// 因果强度 (0.0-1.0)
    pub strength: f32,
    /// 因果路径深度
    pub depth: u32,
    /// 延迟时间 (毫秒)
    pub delay_ms: Option<u64>,
    /// 置信度
    pub confidence: f32,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBCausalEdge {
    pub fn triggered_by(
        tenant_id: u64,
        cause_event_id: impl Into<String>,
        effect_event_id: impl Into<String>,
    ) -> Self {
        let cause = cause_event_id.into();
        let effect = effect_event_id.into();
        Self {
            edge_id: format!("causal:{}:{}", effect, cause),
            tenant_id,
            edge_type: DBEdgeType::TriggeredBy,
            from_event_id: effect,
            to_event_id: cause,
            strength: 1.0,
            depth: 1,
            delay_ms: None,
            confidence: 1.0,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn caused_by(
        tenant_id: u64,
        cause_event_id: impl Into<String>,
        effect_event_id: impl Into<String>,
    ) -> Self {
        let cause = cause_event_id.into();
        let effect = effect_event_id.into();
        Self {
            edge_id: format!("causal:{}:{}", effect, cause),
            tenant_id,
            edge_type: DBEdgeType::CausedBy,
            from_event_id: effect,
            to_event_id: cause,
            strength: 1.0,
            depth: 1,
            delay_ms: None,
            confidence: 1.0,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn led_to(
        tenant_id: u64,
        from_event_id: impl Into<String>,
        to_event_id: impl Into<String>,
    ) -> Self {
        let from = from_event_id.into();
        let to = to_event_id.into();
        Self {
            edge_id: format!("causal:{}:{}", from, to),
            tenant_id,
            edge_type: DBEdgeType::LedTo,
            from_event_id: from,
            to_event_id: to,
            strength: 1.0,
            depth: 1,
            delay_ms: None,
            confidence: 1.0,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn enabled(
        tenant_id: u64,
        enabler_event_id: impl Into<String>,
        enabled_event_id: impl Into<String>,
    ) -> Self {
        let enabler = enabler_event_id.into();
        let enabled = enabled_event_id.into();
        Self {
            edge_id: format!("causal:{}:{}", enabler, enabled),
            tenant_id,
            edge_type: DBEdgeType::Enabled,
            from_event_id: enabler,
            to_event_id: enabled,
            strength: 1.0,
            depth: 1,
            delay_ms: None,
            confidence: 1.0,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

impl DBGraphEdge for DBCausalEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_event_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_event_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("strength".into(), serde_json::json!(self.strength));
        properties.insert("depth".into(), serde_json::json!(self.depth));
        if let Some(delay) = self.delay_ms {
            properties.insert("delay_ms".into(), serde_json::json!(delay));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_event_id.clone(),
            to_node_id: self.to_event_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: Some(self.strength),
            confidence: Some(self.confidence),
            labels: vec!["causal".into(), format!("{:?}", self.edge_type).to_lowercase()],
            properties,
        }
    }
}

// ============================================================================
// 2. 归属边族群
// ============================================================================

/// 归属关系边 - 用于描述实体之间的归属关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBBelongingEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    /// 归属角色
    pub role: Option<String>,
    /// 排序序号
    pub order_index: Option<u32>,
    /// 是否为主要归属
    pub is_primary: bool,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBBelongingEdge {
    pub fn belongs_to(
        tenant_id: u64,
        child_id: impl Into<String>,
        parent_id: impl Into<String>,
    ) -> Self {
        let child = child_id.into();
        let parent = parent_id.into();
        Self {
            edge_id: format!("belonging:{}:{}", child, parent),
            tenant_id,
            edge_type: DBEdgeType::BelongsTo,
            from_node_id: child,
            to_node_id: parent,
            role: None,
            order_index: None,
            is_primary: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn part_of(
        tenant_id: u64,
        part_id: impl Into<String>,
        whole_id: impl Into<String>,
    ) -> Self {
        let part = part_id.into();
        let whole = whole_id.into();
        Self {
            edge_id: format!("belonging:{}:{}", part, whole),
            tenant_id,
            edge_type: DBEdgeType::PartOf,
            from_node_id: part,
            to_node_id: whole,
            role: None,
            order_index: None,
            is_primary: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn member_of(
        tenant_id: u64,
        member_id: impl Into<String>,
        group_id: impl Into<String>,
    ) -> Self {
        let member = member_id.into();
        let group = group_id.into();
        Self {
            edge_id: format!("belonging:{}:{}", member, group),
            tenant_id,
            edge_type: DBEdgeType::MemberOf,
            from_node_id: member,
            to_node_id: group,
            role: None,
            order_index: None,
            is_primary: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    pub fn with_order(mut self, index: u32) -> Self {
        self.order_index = Some(index);
        self
    }

    pub fn as_secondary(mut self) -> Self {
        self.is_primary = false;
        self
    }
}

impl DBGraphEdge for DBBelongingEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_node_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_node_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("is_primary".into(), serde_json::json!(self.is_primary));
        if let Some(ref role) = self.role {
            properties.insert("role".into(), serde_json::json!(role));
        }
        if let Some(order) = self.order_index {
            properties.insert("order_index".into(), serde_json::json!(order));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_node_id.clone(),
            to_node_id: self.to_node_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: None,
            confidence: None,
            labels: vec!["belonging".into()],
            properties,
        }
    }
}

// ============================================================================
// 3. 引用边族群
// ============================================================================

/// 引用关系边 - 用于描述实体之间的引用关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBReferenceEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    /// 引用上下文
    pub context: Option<String>,
    /// 引用位置（在源内容中的偏移）
    pub offset: Option<u32>,
    /// 引用长度
    pub length: Option<u32>,
    /// 引用原因
    pub reason: Option<String>,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBReferenceEdge {
    pub fn references(
        tenant_id: u64,
        from_id: impl Into<String>,
        to_id: impl Into<String>,
    ) -> Self {
        let from = from_id.into();
        let to = to_id.into();
        Self {
            edge_id: format!("reference:{}:{}", from, to),
            tenant_id,
            edge_type: DBEdgeType::References,
            from_node_id: from,
            to_node_id: to,
            context: None,
            offset: None,
            length: None,
            reason: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn mentions(
        tenant_id: u64,
        from_id: impl Into<String>,
        mentioned_id: impl Into<String>,
    ) -> Self {
        let from = from_id.into();
        let mentioned = mentioned_id.into();
        Self {
            edge_id: format!("reference:{}:{}", from, mentioned),
            tenant_id,
            edge_type: DBEdgeType::Mentions,
            from_node_id: from,
            to_node_id: mentioned,
            context: None,
            offset: None,
            length: None,
            reason: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn responds_to(
        tenant_id: u64,
        reply_id: impl Into<String>,
        original_id: impl Into<String>,
    ) -> Self {
        let reply = reply_id.into();
        let original = original_id.into();
        Self {
            edge_id: format!("reference:{}:{}", reply, original),
            tenant_id,
            edge_type: DBEdgeType::RespondsTo,
            from_node_id: reply,
            to_node_id: original,
            context: None,
            offset: None,
            length: None,
            reason: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_position(mut self, offset: u32, length: u32) -> Self {
        self.offset = Some(offset);
        self.length = Some(length);
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

impl DBGraphEdge for DBReferenceEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_node_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_node_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        if let Some(ref ctx) = self.context {
            properties.insert("context".into(), serde_json::json!(ctx));
        }
        if let Some(offset) = self.offset {
            properties.insert("offset".into(), serde_json::json!(offset));
        }
        if let Some(length) = self.length {
            properties.insert("length".into(), serde_json::json!(length));
        }
        if let Some(ref reason) = self.reason {
            properties.insert("reason".into(), serde_json::json!(reason));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_node_id.clone(),
            to_node_id: self.to_node_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: None,
            confidence: None,
            labels: vec!["reference".into()],
            properties,
        }
    }
}

// ============================================================================
// 4. 协作边族群
// ============================================================================

/// 协作关系边 - 用于描述参与者之间的协作关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBCollaborationEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_actor_id: String,
    pub to_actor_id: String,
    /// 协作范围 ID
    pub scope_id: Option<String>,
    /// 协作会话 ID
    pub session_id: Option<String>,
    /// 协作开始时间
    pub started_at_ms: i64,
    /// 协作结束时间
    pub ended_at_ms: Option<i64>,
    /// 协作状态
    pub status: DBCollaborationStatus,
    /// 协作类型描述
    pub collaboration_type: Option<String>,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 协作状态
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DBCollaborationStatus {
    #[default]
    Active,
    Completed,
    Paused,
    Cancelled,
}

impl DBCollaborationEdge {
    pub fn collaborates_with(
        tenant_id: u64,
        actor_a_id: impl Into<String>,
        actor_b_id: impl Into<String>,
    ) -> Self {
        let a = actor_a_id.into();
        let b = actor_b_id.into();
        Self {
            edge_id: format!("collab:{}:{}", a, b),
            tenant_id,
            edge_type: DBEdgeType::CollaboratesWith,
            from_actor_id: a,
            to_actor_id: b,
            scope_id: None,
            session_id: None,
            started_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            ended_at_ms: None,
            status: DBCollaborationStatus::Active,
            collaboration_type: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn requests_help_from(
        tenant_id: u64,
        requester_id: impl Into<String>,
        helper_id: impl Into<String>,
    ) -> Self {
        let requester = requester_id.into();
        let helper = helper_id.into();
        Self {
            edge_id: format!("collab:{}:{}", requester, helper),
            tenant_id,
            edge_type: DBEdgeType::RequestsHelpFrom,
            from_actor_id: requester,
            to_actor_id: helper,
            scope_id: None,
            session_id: None,
            started_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            ended_at_ms: None,
            status: DBCollaborationStatus::Active,
            collaboration_type: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn assists(
        tenant_id: u64,
        helper_id: impl Into<String>,
        helpee_id: impl Into<String>,
    ) -> Self {
        let helper = helper_id.into();
        let helpee = helpee_id.into();
        Self {
            edge_id: format!("collab:{}:{}", helper, helpee),
            tenant_id,
            edge_type: DBEdgeType::Assists,
            from_actor_id: helper,
            to_actor_id: helpee,
            scope_id: None,
            session_id: None,
            started_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            ended_at_ms: None,
            status: DBCollaborationStatus::Active,
            collaboration_type: None,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_scope(mut self, scope_id: impl Into<String>) -> Self {
        self.scope_id = Some(scope_id.into());
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn complete(&mut self) {
        self.status = DBCollaborationStatus::Completed;
        self.ended_at_ms = Some(time::OffsetDateTime::now_utc().unix_timestamp() * 1000);
    }
}

impl DBGraphEdge for DBCollaborationEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_actor_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_actor_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("status".into(), serde_json::json!(self.status));
        properties.insert("started_at_ms".into(), serde_json::json!(self.started_at_ms));
        if let Some(ref scope) = self.scope_id {
            properties.insert("scope_id".into(), serde_json::json!(scope));
        }
        if let Some(ref session) = self.session_id {
            properties.insert("session_id".into(), serde_json::json!(session));
        }
        if let Some(ended) = self.ended_at_ms {
            properties.insert("ended_at_ms".into(), serde_json::json!(ended));
        }
        if let Some(ref collab_type) = self.collaboration_type {
            properties.insert("collaboration_type".into(), serde_json::json!(collab_type));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_actor_id.clone(),
            to_node_id: self.to_actor_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: None,
            confidence: None,
            labels: vec!["collaboration".into(), format!("{:?}", self.status).to_lowercase()],
            properties,
        }
    }
}

// ============================================================================
// 5. 版本边族群
// ============================================================================

/// 版本关系边 - 用于描述实体版本之间的关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBVersionEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    /// 版本号（源）
    pub from_version: Option<u32>,
    /// 版本号（目标）
    pub to_version: Option<u32>,
    /// 变更描述
    pub change_description: Option<String>,
    /// 变更原因
    pub change_reason: Option<String>,
    /// 是否为主要演化路径
    pub is_main_line: bool,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DBVersionEdge {
    pub fn supersedes(
        tenant_id: u64,
        new_id: impl Into<String>,
        old_id: impl Into<String>,
    ) -> Self {
        let new_node = new_id.into();
        let old_node = old_id.into();
        Self {
            edge_id: format!("version:{}:{}", new_node, old_node),
            tenant_id,
            edge_type: DBEdgeType::Supersedes,
            from_node_id: new_node,
            to_node_id: old_node,
            from_version: None,
            to_version: None,
            change_description: None,
            change_reason: None,
            is_main_line: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn derived_from(
        tenant_id: u64,
        derived_id: impl Into<String>,
        source_id: impl Into<String>,
    ) -> Self {
        let derived = derived_id.into();
        let source = source_id.into();
        Self {
            edge_id: format!("version:{}:{}", derived, source),
            tenant_id,
            edge_type: DBEdgeType::DerivedFrom,
            from_node_id: derived,
            to_node_id: source,
            from_version: None,
            to_version: None,
            change_description: None,
            change_reason: None,
            is_main_line: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn evolved_into(
        tenant_id: u64,
        old_id: impl Into<String>,
        new_id: impl Into<String>,
    ) -> Self {
        let old_node = old_id.into();
        let new_node = new_id.into();
        Self {
            edge_id: format!("version:{}:{}", old_node, new_node),
            tenant_id,
            edge_type: DBEdgeType::EvolvedInto,
            from_node_id: old_node,
            to_node_id: new_node,
            from_version: None,
            to_version: None,
            change_description: None,
            change_reason: None,
            is_main_line: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_versions(mut self, from_version: u32, to_version: u32) -> Self {
        self.from_version = Some(from_version);
        self.to_version = Some(to_version);
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.change_description = Some(description.into());
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.change_reason = Some(reason.into());
        self
    }

    pub fn as_branch(mut self) -> Self {
        self.is_main_line = false;
        self
    }
}

impl DBGraphEdge for DBVersionEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_node_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_node_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("is_main_line".into(), serde_json::json!(self.is_main_line));
        if let Some(from_v) = self.from_version {
            properties.insert("from_version".into(), serde_json::json!(from_v));
        }
        if let Some(to_v) = self.to_version {
            properties.insert("to_version".into(), serde_json::json!(to_v));
        }
        if let Some(ref desc) = self.change_description {
            properties.insert("change_description".into(), serde_json::json!(desc));
        }
        if let Some(ref reason) = self.change_reason {
            properties.insert("change_reason".into(), serde_json::json!(reason));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_node_id.clone(),
            to_node_id: self.to_node_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: None,
            confidence: None,
            labels: vec!["version".into()],
            properties,
        }
    }
}

// ============================================================================
// 6. 影响边族群
// ============================================================================

/// 影响关系边 - 用于描述实体之间的影响关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBInfluenceEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    /// 影响强度 (0.0-1.0)
    pub strength: f32,
    /// 影响类型（正面/负面/中性）
    pub polarity: DBInfluencePolarity,
    /// 影响范围描述
    pub scope: Option<String>,
    /// 持续性（瞬时/持续）
    pub is_persistent: bool,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 影响极性
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DBInfluencePolarity {
    Positive,
    Negative,
    #[default]
    Neutral,
}

impl DBInfluenceEdge {
    pub fn influences(
        tenant_id: u64,
        influencer_id: impl Into<String>,
        influenced_id: impl Into<String>,
    ) -> Self {
        let influencer = influencer_id.into();
        let influenced = influenced_id.into();
        Self {
            edge_id: format!("influence:{}:{}", influencer, influenced),
            tenant_id,
            edge_type: DBEdgeType::Influences,
            from_node_id: influencer,
            to_node_id: influenced,
            strength: 0.5,
            polarity: DBInfluencePolarity::Neutral,
            scope: None,
            is_persistent: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn impacts(
        tenant_id: u64,
        actor_id: impl Into<String>,
        target_id: impl Into<String>,
    ) -> Self {
        let actor = actor_id.into();
        let target = target_id.into();
        Self {
            edge_id: format!("influence:{}:{}", actor, target),
            tenant_id,
            edge_type: DBEdgeType::Impacts,
            from_node_id: actor,
            to_node_id: target,
            strength: 0.5,
            polarity: DBInfluencePolarity::Neutral,
            scope: None,
            is_persistent: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn affects(
        tenant_id: u64,
        action_id: impl Into<String>,
        entity_id: impl Into<String>,
    ) -> Self {
        let action = action_id.into();
        let entity = entity_id.into();
        Self {
            edge_id: format!("influence:{}:{}", action, entity),
            tenant_id,
            edge_type: DBEdgeType::Affects,
            from_node_id: action,
            to_node_id: entity,
            strength: 0.5,
            polarity: DBInfluencePolarity::Neutral,
            scope: None,
            is_persistent: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }

    pub fn positive(mut self) -> Self {
        self.polarity = DBInfluencePolarity::Positive;
        self
    }

    pub fn negative(mut self) -> Self {
        self.polarity = DBInfluencePolarity::Negative;
        self
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    pub fn persistent(mut self) -> Self {
        self.is_persistent = true;
        self
    }
}

impl DBGraphEdge for DBInfluenceEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_node_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_node_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("strength".into(), serde_json::json!(self.strength));
        properties.insert("polarity".into(), serde_json::json!(self.polarity));
        properties.insert("is_persistent".into(), serde_json::json!(self.is_persistent));
        if let Some(ref scope) = self.scope {
            properties.insert("scope".into(), serde_json::json!(scope));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_node_id.clone(),
            to_node_id: self.to_node_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: Some(self.strength),
            confidence: None,
            labels: vec![
                "influence".into(),
                format!("{:?}", self.polarity).to_lowercase(),
            ],
            properties,
        }
    }
}

// ============================================================================
// 7. 评价边族群
// ============================================================================

/// 评价关系边 - 用于描述评价关系
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBEvaluationEdge {
    pub edge_id: String,
    pub tenant_id: u64,
    pub edge_type: DBEdgeType,
    pub from_node_id: String,
    pub to_node_id: String,
    /// 评分 (0-100)
    pub score: Option<u8>,
    /// 评价类别
    pub category: Option<String>,
    /// 评价理由
    pub rationale: Option<String>,
    /// 评价维度
    pub dimensions: Vec<DBEvaluationDimension>,
    /// 是否为正式评价
    pub is_formal: bool,
    pub created_at_ms: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 评价维度
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DBEvaluationDimension {
    pub name: String,
    pub score: u8,
    pub weight: f32,
    pub comment: Option<String>,
}

impl DBEvaluationEdge {
    pub fn evaluates(
        tenant_id: u64,
        evaluator_id: impl Into<String>,
        target_id: impl Into<String>,
    ) -> Self {
        let evaluator = evaluator_id.into();
        let target = target_id.into();
        Self {
            edge_id: format!("evaluation:{}:{}", evaluator, target),
            tenant_id,
            edge_type: DBEdgeType::Evaluates,
            from_node_id: evaluator,
            to_node_id: target,
            score: None,
            category: None,
            rationale: None,
            dimensions: vec![],
            is_formal: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn rates(
        tenant_id: u64,
        rater_id: impl Into<String>,
        target_id: impl Into<String>,
        score: u8,
    ) -> Self {
        let rater = rater_id.into();
        let target = target_id.into();
        Self {
            edge_id: format!("evaluation:{}:{}", rater, target),
            tenant_id,
            edge_type: DBEdgeType::Rates,
            from_node_id: rater,
            to_node_id: target,
            score: Some(score.min(100)),
            category: None,
            rationale: None,
            dimensions: vec![],
            is_formal: false,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn critiques(
        tenant_id: u64,
        critic_id: impl Into<String>,
        target_id: impl Into<String>,
    ) -> Self {
        let critic = critic_id.into();
        let target = target_id.into();
        Self {
            edge_id: format!("evaluation:{}:{}", critic, target),
            tenant_id,
            edge_type: DBEdgeType::Critiques,
            from_node_id: critic,
            to_node_id: target,
            score: None,
            category: None,
            rationale: None,
            dimensions: vec![],
            is_formal: true,
            created_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            metadata: HashMap::new(),
        }
    }

    pub fn with_score(mut self, score: u8) -> Self {
        self.score = Some(score.min(100));
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    pub fn add_dimension(&mut self, name: impl Into<String>, score: u8, weight: f32) {
        self.dimensions.push(DBEvaluationDimension {
            name: name.into(),
            score: score.min(100),
            weight: weight.clamp(0.0, 1.0),
            comment: None,
        });
    }

    pub fn formal(mut self) -> Self {
        self.is_formal = true;
        self
    }

    /// 计算加权总分
    pub fn weighted_score(&self) -> Option<f32> {
        if self.dimensions.is_empty() {
            return self.score.map(|s| s as f32);
        }

        let total_weight: f32 = self.dimensions.iter().map(|d| d.weight).sum();
        if total_weight == 0.0 {
            return None;
        }

        let weighted_sum: f32 = self
            .dimensions
            .iter()
            .map(|d| d.score as f32 * d.weight)
            .sum();

        Some(weighted_sum / total_weight)
    }
}

impl DBGraphEdge for DBEvaluationEdge {
    fn edge_id(&self) -> &str {
        &self.edge_id
    }

    fn edge_type(&self) -> DBEdgeType {
        self.edge_type
    }

    fn from_node_id(&self) -> &str {
        &self.from_node_id
    }

    fn to_node_id(&self) -> &str {
        &self.to_node_id
    }

    fn tenant_id(&self) -> u64 {
        self.tenant_id
    }

    fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    fn to_edge_data(&self) -> DBEdgeData {
        let mut properties = self.metadata.clone();
        properties.insert("is_formal".into(), serde_json::json!(self.is_formal));
        if let Some(score) = self.score {
            properties.insert("score".into(), serde_json::json!(score));
        }
        if let Some(ref category) = self.category {
            properties.insert("category".into(), serde_json::json!(category));
        }
        if let Some(ref rationale) = self.rationale {
            properties.insert("rationale".into(), serde_json::json!(rationale));
        }
        if !self.dimensions.is_empty() {
            properties.insert("dimensions".into(), serde_json::json!(self.dimensions));
        }
        if let Some(weighted) = self.weighted_score() {
            properties.insert("weighted_score".into(), serde_json::json!(weighted));
        }

        DBEdgeData {
            edge_id: self.edge_id.clone(),
            edge_type: self.edge_type,
            from_node_id: self.from_node_id.clone(),
            to_node_id: self.to_node_id.clone(),
            tenant_id: self.tenant_id,
            created_at_ms: self.created_at_ms,
            weight: self.score.map(|s| s as f32 / 100.0),
            confidence: None,
            labels: vec!["evaluation".into()],
            properties,
        }
    }
}

// ============================================================================
// 边存储服务
// ============================================================================

/// 边存储服务 Trait
#[allow(async_fn_in_trait)]
pub trait DBEdgeStore {
    /// 存储边
    async fn store_edge(&self, edge: &dyn DBGraphEdge) -> Result<(), DBEdgeStoreError>;
    /// 获取边
    async fn get_edge(&self, edge_id: &str) -> Result<Option<DBEdgeData>, DBEdgeStoreError>;
    /// 删除边
    async fn delete_edge(&self, edge_id: &str) -> Result<bool, DBEdgeStoreError>;
    /// 获取从节点出发的所有边
    async fn get_outgoing_edges(
        &self,
        from_node_id: &str,
        edge_type: Option<DBEdgeType>,
    ) -> Result<Vec<DBEdgeData>, DBEdgeStoreError>;
    /// 获取指向节点的所有边
    async fn get_incoming_edges(
        &self,
        to_node_id: &str,
        edge_type: Option<DBEdgeType>,
    ) -> Result<Vec<DBEdgeData>, DBEdgeStoreError>;
}

/// 边存储错误
#[derive(Debug, thiserror::Error)]
pub enum DBEdgeStoreError {
    #[error("Edge not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid edge: {0}")]
    InvalidEdge(String),
}

// ============================================================================
// SurrealDB Schema 定义
// ============================================================================

/// 生成 SurrealDB 边表定义
pub struct EdgeSchemaGenerator;

impl EdgeSchemaGenerator {
    /// 生成所有边表的 SurrealQL
    pub fn generate_schema() -> String {
        r#"
-- =====================
-- Graph Edges Schema
-- =====================

-- 1. 因果边族群

-- triggered_by 边 (event -> event)
DEFINE TABLE triggered_by SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON triggered_by TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON triggered_by TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON triggered_by TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON triggered_by TYPE option<number>;
DEFINE FIELD confidence ON triggered_by TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON triggered_by TYPE number;
DEFINE FIELD metadata ON triggered_by FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_triggered_by_tenant ON TABLE triggered_by FIELDS tenant_id;

-- caused_by 边 (event -> event)
DEFINE TABLE caused_by SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON caused_by TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON caused_by TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON caused_by TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON caused_by TYPE option<number>;
DEFINE FIELD confidence ON caused_by TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON caused_by TYPE number;
DEFINE FIELD metadata ON caused_by FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_caused_by_tenant ON TABLE caused_by FIELDS tenant_id;

-- enabled 边 (event -> event)
DEFINE TABLE enabled SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON enabled TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON enabled TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON enabled TYPE number DEFAULT 1;
DEFINE FIELD created_at_ms ON enabled TYPE number;
DEFINE FIELD metadata ON enabled FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_enabled_tenant ON TABLE enabled FIELDS tenant_id;

-- led_to 边 (event -> event)
DEFINE TABLE led_to SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON led_to TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON led_to TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON led_to TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON led_to TYPE option<number>;
DEFINE FIELD confidence ON led_to TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON led_to TYPE number;
DEFINE FIELD metadata ON led_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_led_to_tenant ON TABLE led_to FIELDS tenant_id;

-- 2. 归属边族群

-- belongs_to 边 (通用归属)
DEFINE TABLE belongs_to SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON belongs_to TYPE number ASSERT $value > 0;
DEFINE FIELD role ON belongs_to TYPE option<string>;
DEFINE FIELD order_index ON belongs_to TYPE option<number>;
DEFINE FIELD is_primary ON belongs_to TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON belongs_to TYPE number;
DEFINE FIELD metadata ON belongs_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_belongs_to_tenant ON TABLE belongs_to FIELDS tenant_id;

-- part_of 边 (部分-整体)
DEFINE TABLE part_of SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON part_of TYPE number ASSERT $value > 0;
DEFINE FIELD role ON part_of TYPE option<string>;
DEFINE FIELD order_index ON part_of TYPE option<number>;
DEFINE FIELD is_primary ON part_of TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON part_of TYPE number;
DEFINE FIELD metadata ON part_of FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_part_of_tenant ON TABLE part_of FIELDS tenant_id;

-- member_of 边 (成员-组)
DEFINE TABLE member_of SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON member_of TYPE number ASSERT $value > 0;
DEFINE FIELD role ON member_of TYPE option<string>;
DEFINE FIELD joined_at_ms ON member_of TYPE number;
DEFINE FIELD created_at_ms ON member_of TYPE number;
DEFINE FIELD metadata ON member_of FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_member_of_tenant ON TABLE member_of FIELDS tenant_id;

-- 3. 引用边族群

-- references 边 (通用引用)
DEFINE TABLE references SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON references TYPE number ASSERT $value > 0;
DEFINE FIELD context ON references TYPE option<string>;
DEFINE FIELD offset ON references TYPE option<number>;
DEFINE FIELD length ON references TYPE option<number>;
DEFINE FIELD reason ON references TYPE option<string>;
DEFINE FIELD created_at_ms ON references TYPE number;
DEFINE FIELD metadata ON references FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_references_tenant ON TABLE references FIELDS tenant_id;

-- mentions 边 (message -> entity)
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION FROM graph_message;
DEFINE FIELD tenant_id ON mentions TYPE number ASSERT $value > 0;
DEFINE FIELD context ON mentions TYPE option<string>;
DEFINE FIELD offset ON mentions TYPE option<number>;
DEFINE FIELD length ON mentions TYPE option<number>;
DEFINE FIELD created_at_ms ON mentions TYPE number;
DEFINE FIELD metadata ON mentions FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_mentions_tenant ON TABLE mentions FIELDS tenant_id;

-- responds_to 边 (message -> message)
DEFINE TABLE responds_to SCHEMAFULL TYPE RELATION FROM graph_message TO graph_message;
DEFINE FIELD tenant_id ON responds_to TYPE number ASSERT $value > 0;
DEFINE FIELD reason ON responds_to TYPE option<string>;
DEFINE FIELD created_at_ms ON responds_to TYPE number;
DEFINE FIELD metadata ON responds_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_responds_to_tenant ON TABLE responds_to FIELDS tenant_id;

-- 4. 协作边族群

-- collaborates_with 边 (actor <-> actor)
DEFINE TABLE collaborates_with SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON collaborates_with TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON collaborates_with TYPE option<string>;
DEFINE FIELD session_id ON collaborates_with TYPE option<string>;
DEFINE FIELD started_at_ms ON collaborates_with TYPE number;
DEFINE FIELD ended_at_ms ON collaborates_with TYPE option<number>;
DEFINE FIELD status ON collaborates_with TYPE string DEFAULT 'active';
DEFINE FIELD collaboration_type ON collaborates_with TYPE option<string>;
DEFINE FIELD created_at_ms ON collaborates_with TYPE number;
DEFINE FIELD metadata ON collaborates_with FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_collaborates_tenant ON TABLE collaborates_with FIELDS tenant_id;
DEFINE INDEX idx_collaborates_status ON TABLE collaborates_with FIELDS tenant_id, status;

-- requests_help_from 边 (actor -> actor)
DEFINE TABLE requests_help_from SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON requests_help_from TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON requests_help_from TYPE option<string>;
DEFINE FIELD session_id ON requests_help_from TYPE option<string>;
DEFINE FIELD started_at_ms ON requests_help_from TYPE number;
DEFINE FIELD ended_at_ms ON requests_help_from TYPE option<number>;
DEFINE FIELD status ON requests_help_from TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON requests_help_from TYPE number;
DEFINE FIELD metadata ON requests_help_from FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_requests_help_tenant ON TABLE requests_help_from FIELDS tenant_id;

-- assists 边 (actor -> actor)
DEFINE TABLE assists SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON assists TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON assists TYPE option<string>;
DEFINE FIELD session_id ON assists TYPE option<string>;
DEFINE FIELD started_at_ms ON assists TYPE number;
DEFINE FIELD ended_at_ms ON assists TYPE option<number>;
DEFINE FIELD status ON assists TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON assists TYPE number;
DEFINE FIELD metadata ON assists FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_assists_tenant ON TABLE assists FIELDS tenant_id;

-- 5. 版本边族群

-- supersedes 边 (entity -> entity)
DEFINE TABLE supersedes SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON supersedes TYPE number ASSERT $value > 0;
DEFINE FIELD from_version ON supersedes TYPE option<number>;
DEFINE FIELD to_version ON supersedes TYPE option<number>;
DEFINE FIELD change_description ON supersedes TYPE option<string>;
DEFINE FIELD change_reason ON supersedes TYPE option<string>;
DEFINE FIELD is_main_line ON supersedes TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON supersedes TYPE number;
DEFINE FIELD metadata ON supersedes FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_supersedes_tenant ON TABLE supersedes FIELDS tenant_id;

-- derived_from 边 (artifact -> artifact)
DEFINE TABLE derived_from SCHEMAFULL TYPE RELATION FROM graph_artifact TO graph_artifact;
DEFINE FIELD tenant_id ON derived_from TYPE number ASSERT $value > 0;
DEFINE FIELD from_version ON derived_from TYPE option<number>;
DEFINE FIELD to_version ON derived_from TYPE option<number>;
DEFINE FIELD change_description ON derived_from TYPE option<string>;
DEFINE FIELD is_main_line ON derived_from TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON derived_from TYPE number;
DEFINE FIELD metadata ON derived_from FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_derived_from_tenant ON TABLE derived_from FIELDS tenant_id;

-- evolved_into 边 (artifact -> artifact)
DEFINE TABLE evolved_into SCHEMAFULL TYPE RELATION FROM graph_artifact TO graph_artifact;
DEFINE FIELD tenant_id ON evolved_into TYPE number ASSERT $value > 0;
DEFINE FIELD from_version ON evolved_into TYPE option<number>;
DEFINE FIELD to_version ON evolved_into TYPE option<number>;
DEFINE FIELD change_description ON evolved_into TYPE option<string>;
DEFINE FIELD is_main_line ON evolved_into TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON evolved_into TYPE number;
DEFINE FIELD metadata ON evolved_into FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_evolved_into_tenant ON TABLE evolved_into FIELDS tenant_id;

-- 6. 影响边族群

-- influences 边 (entity -> entity)
DEFINE TABLE influences SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON influences TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON influences TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON influences TYPE string DEFAULT 'neutral';
DEFINE FIELD scope ON influences TYPE option<string>;
DEFINE FIELD is_persistent ON influences TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON influences TYPE number;
DEFINE FIELD metadata ON influences FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_influences_tenant ON TABLE influences FIELDS tenant_id;

-- impacts 边 (event -> state)
DEFINE TABLE impacts SCHEMAFULL TYPE RELATION FROM graph_event;
DEFINE FIELD tenant_id ON impacts TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON impacts TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON impacts TYPE string DEFAULT 'neutral';
DEFINE FIELD scope ON impacts TYPE option<string>;
DEFINE FIELD is_persistent ON impacts TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON impacts TYPE number;
DEFINE FIELD metadata ON impacts FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_impacts_tenant ON TABLE impacts FIELDS tenant_id;

-- affects 边 (action -> entity)
DEFINE TABLE affects SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON affects TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON affects TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON affects TYPE string DEFAULT 'neutral';
DEFINE FIELD is_persistent ON affects TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON affects TYPE number;
DEFINE FIELD metadata ON affects FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_affects_tenant ON TABLE affects FIELDS tenant_id;

-- 7. 评价边族群

-- evaluates 边 (actor -> entity)
DEFINE TABLE evaluates SCHEMAFULL TYPE RELATION FROM graph_actor;
DEFINE FIELD tenant_id ON evaluates TYPE number ASSERT $value > 0;
DEFINE FIELD score ON evaluates TYPE option<number>;
DEFINE FIELD category ON evaluates TYPE option<string>;
DEFINE FIELD rationale ON evaluates TYPE option<string>;
DEFINE FIELD dimensions ON evaluates TYPE array DEFAULT [];
DEFINE FIELD is_formal ON evaluates TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON evaluates TYPE number;
DEFINE FIELD metadata ON evaluates FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_evaluates_tenant ON TABLE evaluates FIELDS tenant_id;

-- rates 边 (actor -> artifact)
DEFINE TABLE rates SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_artifact;
DEFINE FIELD tenant_id ON rates TYPE number ASSERT $value > 0;
DEFINE FIELD score ON rates TYPE number ASSERT $value >= 0 AND $value <= 100;
DEFINE FIELD category ON rates TYPE option<string>;
DEFINE FIELD created_at_ms ON rates TYPE number;
DEFINE FIELD metadata ON rates FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_rates_tenant ON TABLE rates FIELDS tenant_id;

-- critiques 边 (actor -> artifact)
DEFINE TABLE critiques SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_artifact;
DEFINE FIELD tenant_id ON critiques TYPE number ASSERT $value > 0;
DEFINE FIELD score ON critiques TYPE option<number>;
DEFINE FIELD rationale ON critiques TYPE option<string>;
DEFINE FIELD dimensions ON critiques TYPE array DEFAULT [];
DEFINE FIELD is_formal ON critiques TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON critiques TYPE number;
DEFINE FIELD metadata ON critiques FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_critiques_tenant ON TABLE critiques FIELDS tenant_id;
"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_type_family() {
        assert_eq!(DBEdgeType::TriggeredBy.family(), DBEdgeFamily::Causal);
        assert_eq!(DBEdgeType::BelongsTo.family(), DBEdgeFamily::Belonging);
        assert_eq!(DBEdgeType::References.family(), DBEdgeFamily::Reference);
        assert_eq!(DBEdgeType::CollaboratesWith.family(), DBEdgeFamily::Collaboration);
        assert_eq!(DBEdgeType::Supersedes.family(), DBEdgeFamily::Version);
        assert_eq!(DBEdgeType::Influences.family(), DBEdgeFamily::Influence);
        assert_eq!(DBEdgeType::Evaluates.family(), DBEdgeFamily::Evaluation);
    }

    #[test]
    fn test_causal_edge_creation() {
        let edge = DBCausalEdge::triggered_by(1, "evt-0", "evt-1")
            .with_strength(0.8)
            .with_depth(2)
            .with_delay(1000);

        assert_eq!(edge.edge_type, DBEdgeType::TriggeredBy);
        assert_eq!(edge.from_event_id, "evt-1");
        assert_eq!(edge.to_event_id, "evt-0");
        assert!((edge.strength - 0.8).abs() < 0.001);
        assert_eq!(edge.depth, 2);
        assert_eq!(edge.delay_ms, Some(1000));
    }

    #[test]
    fn test_belonging_edge_creation() {
        let edge = DBBelongingEdge::belongs_to(1, "event-1", "session-1")
            .with_role("primary_event")
            .with_order(0);

        assert_eq!(edge.edge_type, DBEdgeType::BelongsTo);
        assert_eq!(edge.from_node_id, "event-1");
        assert_eq!(edge.to_node_id, "session-1");
        assert_eq!(edge.role, Some("primary_event".into()));
        assert_eq!(edge.order_index, Some(0));
    }

    #[test]
    fn test_reference_edge_creation() {
        let edge = DBReferenceEdge::responds_to(1, "msg-2", "msg-1")
            .with_context("reply to question")
            .with_reason("answering user query");

        assert_eq!(edge.edge_type, DBEdgeType::RespondsTo);
        assert_eq!(edge.from_node_id, "msg-2");
        assert_eq!(edge.to_node_id, "msg-1");
        assert_eq!(edge.context, Some("reply to question".into()));
    }

    #[test]
    fn test_collaboration_edge_lifecycle() {
        let mut edge = DBCollaborationEdge::requests_help_from(1, "actor:ai:1", "actor:ai:2")
            .with_scope("scope-123")
            .with_session("session-1");

        assert_eq!(edge.status, DBCollaborationStatus::Active);

        edge.complete();
        assert_eq!(edge.status, DBCollaborationStatus::Completed);
        assert!(edge.ended_at_ms.is_some());
    }

    #[test]
    fn test_version_edge_creation() {
        let edge = DBVersionEdge::supersedes(1, "artifact-v2", "artifact-v1")
            .with_versions(2, 1)
            .with_description("Fixed bug in calculation");

        assert_eq!(edge.edge_type, DBEdgeType::Supersedes);
        assert_eq!(edge.from_version, Some(2));
        assert_eq!(edge.to_version, Some(1));
        assert!(edge.is_main_line);
    }

    #[test]
    fn test_influence_edge_creation() {
        let edge = DBInfluenceEdge::influences(1, "event-1", "decision-1")
            .with_strength(0.9)
            .positive()
            .with_scope("decision making")
            .persistent();

        assert_eq!(edge.edge_type, DBEdgeType::Influences);
        assert!((edge.strength - 0.9).abs() < 0.001);
        assert_eq!(edge.polarity, DBInfluencePolarity::Positive);
        assert!(edge.is_persistent);
    }

    #[test]
    fn test_evaluation_edge_weighted_score() {
        let mut edge = DBEvaluationEdge::evaluates(1, "actor:ai:1", "artifact-1");
        edge.add_dimension("quality", 80, 0.5);
        edge.add_dimension("completeness", 90, 0.3);
        edge.add_dimension("style", 70, 0.2);

        let weighted = edge.weighted_score().unwrap();
        // (80*0.5 + 90*0.3 + 70*0.2) / 1.0 = 40 + 27 + 14 = 81
        assert!((weighted - 81.0).abs() < 0.1);
    }

    #[test]
    fn test_edge_to_data_conversion() {
        let edge = DBCausalEdge::triggered_by(1, "evt-0", "evt-1");
        let data = edge.to_edge_data();

        assert_eq!(data.edge_type, DBEdgeType::TriggeredBy);
        assert!(data.properties.contains_key("strength"));
        assert!(data.properties.contains_key("depth"));
        assert!(data.labels.contains(&"causal".to_string()));
    }
}
