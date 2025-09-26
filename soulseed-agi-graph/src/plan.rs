use serde::{Deserialize, Serialize};

use crate::{
    AwarenessFilters, ConversationScenario, DialogueEventType, EventId, RecallFilters, SessionId,
    TimeWindow,
};

pub const IDX_SESSION_ORDER: &str = "idx_dialogue_event_session_order";
pub const IDX_TIMELINE: &str = "idx_dialogue_event_timeline";
pub const IDX_EDGE: &str = "idx_dialogue_event_causal";
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
    Hybrid {
        components: Vec<PlanComponent>,
        k: u16,
    },
    Live {
        filters: crate::LiveFilters,
        rate: u32,
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
    pub live_default_rate: u32,
    pub live_max_rate: u32,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            default_causal_depth: 2,
            max_causal_depth: 4,
            default_limit: 50,
            max_limit: 200,
            live_default_rate: 20,
            live_max_rate: 60,
        }
    }
}
