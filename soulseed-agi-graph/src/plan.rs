use serde::{Deserialize, Serialize};

use crate::{
    AwarenessFilters, ConversationScenario, DialogueEventType, EventId, RecallFilters, SessionId,
    TimeWindow,
};

pub const IDX_SESSION_ORDER: &str = "idx_dialogue_event_session_order";
pub const IDX_TIMELINE: &str = "idx_dialogue_event_timeline";
pub const IDX_EDGE: &str = "idx_dialogue_event_causal";
pub const IDX_GRAPH_PATH: &str = "idx_graph_edge_path";
pub const IDX_GRAPH_NEIGHBORHOOD: &str = "idx_graph_edge_neighborhood";
pub const IDX_GRAPH_SUBGRAPH: &str = "idx_graph_edge_subgraph";
pub const IDX_GRAPH_INFLUENCE: &str = "idx_graph_edge_influence";
pub const IDX_GRAPH_PATTERN: &str = "idx_graph_edge_pattern";
pub const IDX_VEC: &str = "idx_dialogue_event_vector";
pub const IDX_SPARSE: &str = "idx_dialogue_event_sparse";
pub const IDX_AWARENESS_CYCLE: &str = "ix_awareness_cycle_time";
pub const IDX_AWARENESS_PARENT: &str = "ix_awareness_parent_cycle";
pub const IDX_AWARENESS_COLLAB: &str = "ix_awareness_collab_scope";
pub const IDX_AWARENESS_BARRIER: &str = "ix_awareness_barrier";
pub const IDX_AWARENESS_ENV: &str = "ix_awareness_env_mode";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Plan {
    Timeline {
        index: String,
        window: Option<TimeWindow>,
        order: (String, String),
        scenario: Option<ConversationScenario>,
        event_types: Option<Vec<DialogueEventType>>,
    },
    SessionOrder {
        index: String,
        session_id: SessionId,
        after: Option<String>,
        scenario: Option<ConversationScenario>,
    },
    Causal {
        index: String,
        root: EventId,
        dir: String,
        depth: u8,
        window: Option<TimeWindow>,
        scenario: Option<ConversationScenario>,
        event_types: Option<Vec<DialogueEventType>>,
    },
    Path {
        index: String,
        start: EventId,
        goal: EventId,
        strategy: String,
        max_depth: u8,
        max_paths: u16,
        scenario: Option<ConversationScenario>,
    },
    Neighborhood {
        index: String,
        center: EventId,
        direction: String,
        radius: u8,
        limit: u32,
        scenario: Option<ConversationScenario>,
    },
    Subgraph {
        index: String,
        seeds: Vec<EventId>,
        radius: u8,
        max_nodes: u32,
        scenario: Option<ConversationScenario>,
        include_artifacts: bool,
    },
    VectorAnn {
        index: String,
        dim: u16,
        k: u16,
        filter: RecallFilters,
    },
    SparseBm25 {
        index: String,
        k: u16,
        filter: RecallFilters,
    },
    Similarity {
        index: String,
        anchor: EventId,
        top_k: u16,
        strategy: String,
        filter: RecallFilters,
    },
    Hybrid {
        components: Vec<PlanComponent>,
        k: u16,
    },
    Influence {
        index: String,
        seeds: Vec<EventId>,
        horizon_ms: Option<i64>,
        damping_factor: f32,
        iterations: u8,
        scenario: Option<ConversationScenario>,
    },
    Pattern {
        index: String,
        template_id: String,
        limit: u32,
    },
    Live {
        filters: crate::LiveFilters,
        rate: u32,
        heartbeat_ms: u32,
        idle_timeout_ms: u32,
        max_buffer: u32,
        backpressure_mode: String,
    },
    Awareness {
        index: String,
        filters: AwarenessFilters,
        order: (String, String),
        limit: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanComponent {
    pub name: String,
    pub weight: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedPlan {
    pub plan: Plan,
    pub indices_used: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub default_causal_depth: u8,
    pub max_causal_depth: u8,
    pub default_limit: u32,
    pub max_limit: u32,
    pub max_path_depth: u8,
    pub max_neighborhood_radius: u8,
    pub max_subgraph_radius: u8,
    pub max_subgraph_nodes: u32,
    pub max_similarity_k: u16,
    pub max_influence_iterations: u8,
    pub max_pattern_limit: u32,
    pub live_default_rate: u32,
    pub live_max_rate: u32,
    pub live_default_heartbeat_ms: u32,
    pub live_min_heartbeat_ms: u32,
    pub live_default_idle_timeout_ms: u32,
    pub live_max_buffer: u32,
    pub live_default_backpressure_mode: String,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            default_causal_depth: 2,
            max_causal_depth: 4,
            default_limit: 50,
            max_limit: 200,
            max_path_depth: 6,
            max_neighborhood_radius: 4,
            max_subgraph_radius: 5,
            max_subgraph_nodes: 500,
            max_similarity_k: 128,
            max_influence_iterations: 12,
            max_pattern_limit: 100,
            live_default_rate: 20,
            live_max_rate: 60,
            live_default_heartbeat_ms: 3000,
            live_min_heartbeat_ms: 500,
            live_default_idle_timeout_ms: 10000,
            live_max_buffer: 500,
            live_default_backpressure_mode: "drop_tail".into(),
        }
    }
}
