use serde::{Deserialize, Serialize};

use crate::{
    AIId, AwarenessDegradationReason, AwarenessEvent, AwarenessEventType, AwarenessFork,
    ConversationScenario, CycleId, DialogueEvent, DialogueEventType, EventId, HumanId,
    RelationshipEdge, RelationshipSnapshot, SessionId, SubjectRef, SyncPointKind, TenantId,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageCursor(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineQuery {
    pub tenant_id: TenantId,
    pub session_id: Option<SessionId>,
    pub participants: Option<Vec<SubjectRef>>,
    pub window: Option<TimeWindow>,
    pub after: Option<PageCursor>,
    pub limit: u32,
    pub require_fields: Option<Vec<String>>,
    pub scenario: Option<ConversationScenario>,
    pub event_types: Option<Vec<DialogueEventType>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineItem {
    pub event: DialogueEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineResponse {
    pub tenant_id: TenantId,
    pub items: Vec<TimelineItem>,
    pub next: Option<PageCursor>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CausalDir {
    Upstream,
    Downstream,
    Both,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalQuery {
    pub tenant_id: TenantId,
    pub root_event: EventId,
    pub direction: CausalDir,
    pub max_depth: Option<u8>,
    pub time_window: Option<TimeWindow>,
    pub scenario: Option<ConversationScenario>,
    pub event_types: Option<Vec<DialogueEventType>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalEdge {
    pub from: EventId,
    pub to: EventId,
    pub edge_type: String,
    pub weight: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalResponse {
    pub tenant_id: TenantId,
    pub nodes: Vec<DialogueEvent>,
    pub edges: Vec<CausalEdge>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StitchQuery {
    pub tenant_id: TenantId,
    pub center: EventId,
    pub k_prev: u16,
    pub k_next: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StitchResponse {
    pub tenant_id: TenantId,
    pub left: Vec<DialogueEvent>,
    pub center: DialogueEvent,
    pub right: Vec<DialogueEvent>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallFilters {
    pub scenes: Option<Vec<ConversationScenario>>,
    pub topics: Option<Vec<String>>,
    pub time_window: Option<TimeWindow>,
    pub participant: Option<SubjectRef>,
    pub event_types: Option<Vec<DialogueEventType>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecallQueryTextOrVec {
    Text(String),
    Vec(Vec<f32>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallQuery {
    pub tenant_id: TenantId,
    pub query: RecallQueryTextOrVec,
    pub k: u16,
    pub filters: RecallFilters,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallHit {
    pub event: DialogueEvent,
    pub score: f32,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallResponse {
    pub tenant_id: TenantId,
    pub hits: Vec<RecallHit>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

pub const EXPLAIN_EVENT_TYPES_DEFAULT: &[AwarenessEventType] = &[
    AwarenessEventType::DecisionRouted,
    AwarenessEventType::RouteReconsidered,
    AwarenessEventType::RouteSwitched,
    AwarenessEventType::ClarificationIssued,
    AwarenessEventType::ClarificationAnswered,
    AwarenessEventType::ToolCalled,
    AwarenessEventType::ToolResponded,
    AwarenessEventType::ToolFailed,
    AwarenessEventType::CollabRequested,
    AwarenessEventType::CollabResolved,
    AwarenessEventType::SyncPointReported,
    AwarenessEventType::InjectionApplied,
    AwarenessEventType::InjectionDeferred,
    AwarenessEventType::InjectionIgnored,
    AwarenessEventType::LateReceiptObserved,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessFilters {
    pub awareness_cycle_id: Option<CycleId>,
    pub parent_cycle_id: Option<CycleId>,
    pub collab_scope_id: Option<String>,
    pub barrier_id: Option<String>,
    pub env_mode: Option<String>,
    pub inference_cycle_sequence: Option<u32>,
    pub event_types: Option<Vec<AwarenessEventType>>,
    pub degradation_reasons: Option<Vec<AwarenessDegradationReason>>,
    pub sync_point_kinds: Option<Vec<SyncPointKind>>,
}

impl Default for AwarenessFilters {
    fn default() -> Self {
        Self {
            awareness_cycle_id: None,
            parent_cycle_id: None,
            collab_scope_id: None,
            barrier_id: None,
            env_mode: None,
            inference_cycle_sequence: None,
            event_types: None,
            degradation_reasons: None,
            sync_point_kinds: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessQuery {
    pub tenant_id: TenantId,
    pub filters: AwarenessFilters,
    pub limit: u32,
    pub after: Option<PageCursor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessResponse {
    pub tenant_id: TenantId,
    pub events: Vec<AwarenessEvent>,
    pub next: Option<PageCursor>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainReplayQuery {
    pub tenant_id: TenantId,
    pub awareness_cycle_id: CycleId,
    pub forks: Option<Vec<AwarenessFork>>,
    pub event_types: Option<Vec<AwarenessEventType>>,
    pub limit: u32,
    pub after: Option<PageCursor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainReplaySegment {
    pub event: AwarenessEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainReplayResponse {
    pub tenant_id: TenantId,
    pub segments: Vec<ExplainReplaySegment>,
    pub next: Option<PageCursor>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RerankWeights {
    pub vec: f32,
    pub sparse: f32,
    pub causal: f32,
    pub recency: f32,
    pub authority: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RerankQuery {
    pub tenant_id: TenantId,
    pub candidates: Vec<RecallHit>,
    pub weights: RerankWeights,
    pub now_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RationaleComponent {
    pub name: String,
    pub contribution: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rationale {
    pub components: Vec<RationaleComponent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RerankItem {
    pub event: DialogueEvent,
    pub score: f32,
    pub explain: Rationale,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RerankResponse {
    pub tenant_id: TenantId,
    pub items: Vec<RerankItem>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationQuery {
    pub tenant_id: TenantId,
    pub human_id: HumanId,
    pub ai_id: AIId,
    pub window: TimeWindow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationResponse {
    pub tenant_id: TenantId,
    pub edge_facts: RelationshipEdge,
    pub snapshot: Option<RelationshipSnapshot>,
    pub indices_used: Vec<String>,
    pub query_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveFilters {
    pub session: Option<SessionId>,
    pub participants: Option<Vec<SubjectRef>>,
    pub scene: Option<ConversationScenario>,
    pub priority: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveSubscribe {
    pub tenant_id: TenantId,
    pub filters: LiveFilters,
    pub max_rate: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveEventPointer {
    pub event_id: EventId,
    pub session_id: SessionId,
    pub timestamp_ms: i64,
    pub evidence_pointer: Option<String>,
    pub blob_ref: Option<String>,
    pub content_digest_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveAck {
    pub last_event_id: Option<EventId>,
}
