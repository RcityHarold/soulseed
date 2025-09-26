use crate::{AIId, HumanId, ModelError, Provenance, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipEdge {
    pub tenant_id: TenantId,
    pub human_id: HumanId,
    pub ai_id: AIId,
    pub first_interaction_ts: i64,
    pub last_active_ts: i64,
    pub total_rounds: u32,
    pub active_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gift_value_sum_lc: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lucky_cards_to_human: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion_resonance_count: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipSnapshot {
    pub tenant_id: TenantId,
    pub human_id: HumanId,
    pub ai_id: AIId,
    pub snapshot_ts: i64,
    pub version: u32,
    pub state: String,
    pub intensity: f32,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub depth: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub nature: serde_json::Value,
    pub stage: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    pub provenance: Provenance,
}

impl RelationshipSnapshot {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.state.trim().is_empty() {
            return Err(ModelError::Missing("state"));
        }
        if self.intensity.is_nan() {
            return Err(ModelError::Invariant("intensity cannot be NaN"));
        }
        Ok(())
    }
}
