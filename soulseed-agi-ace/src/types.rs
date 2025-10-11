use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::{
    AwarenessCycleId, DialogueEvent, EventId, TenantId,
    awareness::{AwarenessAnchor, AwarenessEvent, AwarenessFork, DeltaPatch, SyncPointKind},
};
use soulseed_agi_dfr::types::{RouterCandidate, RouterDecision, RouterInput};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::ca::InjectionDecision;
use crate::hitl::HitlInjection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleLane {
    Clarify,
    Tool,
    SelfReason,
    Collab,
}

impl From<AwarenessFork> for CycleLane {
    fn from(value: AwarenessFork) -> Self {
        match value {
            AwarenessFork::Clarify => CycleLane::Clarify,
            AwarenessFork::ToolPath => CycleLane::Tool,
            AwarenessFork::SelfReason => CycleLane::SelfReason,
            AwarenessFork::Collab => CycleLane::Collab,
        }
    }
}

impl From<&AwarenessFork> for CycleLane {
    fn from(value: &AwarenessFork) -> Self {
        (*value).into()
    }
}

impl CycleLane {
    pub fn as_fork(&self) -> AwarenessFork {
        match self {
            CycleLane::Clarify => AwarenessFork::Clarify,
            CycleLane::Tool => AwarenessFork::ToolPath,
            CycleLane::SelfReason => AwarenessFork::SelfReason,
            CycleLane::Collab => AwarenessFork::Collab,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    pub tokens_allowed: u32,
    pub tokens_spent: u32,
    pub walltime_ms_allowed: u64,
    pub walltime_ms_used: u64,
    #[serde(default)]
    pub external_cost_allowed: f32,
    #[serde(default)]
    pub external_cost_spent: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CycleSchedule {
    pub cycle_id: AwarenessCycleId,
    pub lane: CycleLane,
    pub anchor: AwarenessAnchor,
    pub budget: BudgetSnapshot,
    pub created_at: OffsetDateTime,
    pub router_decision: RouterDecision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_events: Vec<AwarenessEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CycleRequest {
    pub router_input: RouterInput,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<RouterCandidate>,
    pub budget: BudgetSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncPointInput {
    pub cycle_id: AwarenessCycleId,
    pub kind: SyncPointKind,
    pub anchor: AwarenessAnchor,
    pub events: Vec<DialogueEvent>,
    pub budget: BudgetSnapshot,
    pub timeframe: (OffsetDateTime, OffsetDateTime),
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_injections: Vec<HitlInjection>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub context_manifest: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncPointReport {
    pub cycle_id: AwarenessCycleId,
    pub kind: SyncPointKind,
    pub summary: String,
    pub degradation_reason: Option<String>,
    pub metrics: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<InjectionDecision>,
    pub applied: u32,
    pub missing: u32,
    pub ignored: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_sequences: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delta_added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delta_updated: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delta_removed: Vec<String>,
    pub budget_snapshot: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_patch_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub cycle_id: AwarenessCycleId,
    pub event_id: EventId,
    pub payload: AwarenessEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CycleEmission {
    pub cycle_id: AwarenessCycleId,
    pub lane: CycleLane,
    pub final_event: DialogueEvent,
    pub awareness_events: Vec<AwarenessEvent>,
    pub budget: BudgetSnapshot,
    pub anchor: AwarenessAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
    pub router_decision: RouterDecision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduleOutcome {
    pub accepted: bool,
    pub reason: Option<String>,
    pub cycle: Option<CycleSchedule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub awareness_events: Vec<AwarenessEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregationOutcome {
    pub report: SyncPointReport,
    pub awareness_events: Vec<AwarenessEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_patch: Option<DeltaPatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<InjectionDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_manifest: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_bundle: Option<soulseed_agi_context::types::PromptBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_bundle: Option<soulseed_agi_context::types::ExplainBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_bundle: Option<soulseed_agi_context::types::ContextBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision: Option<soulseed_agi_dfr::types::RouterDecision>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetDecision {
    pub cycle_id: AwarenessCycleId,
    pub allowed: bool,
    pub reason: Option<String>,
    pub snapshot: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointState {
    pub tenant_id: TenantId,
    pub cycle_id: AwarenessCycleId,
    pub lane: CycleLane,
    pub budget: BudgetSnapshot,
    pub since: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxEnvelope {
    pub tenant_id: TenantId,
    pub cycle_id: AwarenessCycleId,
    pub messages: Vec<OutboxMessage>,
}

pub fn new_cycle_id() -> AwarenessCycleId {
    AwarenessCycleId::from_raw_unchecked(Uuid::now_v7().as_u128() as u64)
}
