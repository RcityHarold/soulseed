use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::{
    CycleId, DialogueEvent, EventId, TenantId,
    awareness::{AwarenessAnchor, AwarenessEvent, DeltaPatch, SyncPointKind},
};
use soulseed_agi_tools::dto::ToolPlan;
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
    pub cycle_id: CycleId,
    pub lane: CycleLane,
    pub anchor: AwarenessAnchor,
    pub tool_plan: Option<ToolPlan>,
    pub llm_plan: Option<Value>,
    pub budget: BudgetSnapshot,
    pub created_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_events: Vec<AwarenessEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CycleRequest {
    pub lane: CycleLane,
    pub anchor: AwarenessAnchor,
    pub tool_plan: Option<ToolPlan>,
    pub llm_plan: Option<Value>,
    pub budget: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_explain: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncPointInput {
    pub cycle_id: CycleId,
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
    pub cycle_id: CycleId,
    pub kind: SyncPointKind,
    pub summary: String,
    pub degradation_reason: Option<String>,
    pub metrics: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<InjectionDecision>,
    pub applied: u32,
    pub missing: u32,
    pub ignored: u32,
    pub budget_snapshot: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_patch_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub cycle_id: CycleId,
    pub event_id: EventId,
    pub payload: AwarenessEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CycleEmission {
    pub cycle_id: CycleId,
    pub lane: CycleLane,
    pub final_event: DialogueEvent,
    pub awareness_events: Vec<AwarenessEvent>,
    pub budget: BudgetSnapshot,
    pub anchor: AwarenessAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetDecision {
    pub cycle_id: CycleId,
    pub allowed: bool,
    pub reason: Option<String>,
    pub snapshot: BudgetSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointState {
    pub tenant_id: TenantId,
    pub cycle_id: CycleId,
    pub lane: CycleLane,
    pub budget: BudgetSnapshot,
    pub since: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxEnvelope {
    pub tenant_id: TenantId,
    pub cycle_id: CycleId,
    pub messages: Vec<OutboxMessage>,
}

pub fn new_cycle_id() -> CycleId {
    CycleId(Uuid::now_v7().as_u128() as u64)
}
