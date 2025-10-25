use crate::{
    AccessClass, AwarenessCycleId, EnvelopeId, EventId, InferenceCycleId, ModelError, Provenance,
    SessionId, Snapshot, Subject, TenantId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(feature = "strict-privacy")]
fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

/// Anchor information used by Awareness layer payloads (ACE/DFR/CA).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AwarenessAnchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[cfg(feature = "strict-privacy")]
    #[serde(default = "default_restricted")]
    pub access_class: AccessClass,
    #[cfg(not(feature = "strict-privacy"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
}

impl AwarenessAnchor {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.config_snapshot_hash.trim().is_empty() {
            return Err(ModelError::Missing("config_snapshot_hash"));
        }
        if self.schema_v == 0 {
            return Err(ModelError::Invariant("schema_v must be >= 1"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Invariant(
                "provenance required for restricted anchor",
            ));
        }
        Ok(())
    }
}

/// Primary fork decision paths supported by ACE/DFR.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessFork {
    SelfReason,
    Clarify,
    ToolPath,
    Collab,
}

/// Sync points at which ACE aggregates external signals.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SyncPointKind {
    IcEnd,
    ToolBarrier,
    ToolBarrierReached,
    ToolBarrierReleased,
    ToolBarrierTimeout,
    ToolChainNext,
    CollabTurnEnd,
    ClarifyAnswered,
    ClarifyWindowOpened,
    ClarifyWindowClosed,
    ToolWindowOpened,
    ToolWindowClosed,
    HitlWindowOpened,
    HitlWindowClosed,
    HitlAbsorb,
    Merged,
    DriftDetected,
    LateSignalObserved,
    BudgetExceeded,
    BudgetRecovered,
    DegradationRecorded,
}

/// Outcome of an awareness cycle once it reaches completion.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessOutcome {
    Continue,
    Finalized,
    Rejected,
}

/// Awareness layer append-only event enumeration.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessEventType {
    AcStarted,
    IcStarted,
    IcEnded,
    AssessmentProduced,
    DecisionRouted,
    ToolPathDecided,
    RouteReconsidered,
    RouteSwitched,
    ToolCalled,
    ToolResponded,
    ToolFailed,
    ToolBarrierReached,
    ToolBarrierReleased,
    ToolBarrierTimeout,
    CollabRequested,
    CollabResolved,
    ClarificationIssued,
    ClarificationAnswered,
    HumanInjectionReceived,
    InjectionApplied,
    InjectionDeferred,
    InjectionIgnored,
    DeltaPatchGenerated,
    ContextBuilt,
    DeltaMerged,
    SyncPointMerged,
    SyncPointReported,
    Finalized,
    Rejected,
    LateReceiptObserved,
    EnvironmentSnapshotRecorded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AwarenessEvent {
    #[serde(flatten)]
    pub anchor: AwarenessAnchor,
    pub event_id: EventId,
    pub event_type: AwarenessEventType,
    pub occurred_at_ms: i64,
    pub awareness_cycle_id: AwarenessCycleId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_cycle_id: Option<AwarenessCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collab_scope_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_mode: Option<String>,
    pub inference_cycle_sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: Value,
}

impl AwarenessEvent {
    pub fn validate(&self) -> Result<(), ModelError> {
        self.anchor.validate()?;
        if self.occurred_at_ms < 0 {
            return Err(ModelError::Invariant("occurred_at_ms must be >= 0"));
        }
        if self.inference_cycle_sequence == 0 {
            return Err(ModelError::Invariant(
                "inference_cycle_sequence must be >= 1",
            ));
        }
        if let Some(parent_cycle_id) = self.parent_cycle_id {
            if parent_cycle_id == self.awareness_cycle_id {
                return Err(ModelError::Invariant(
                    "parent_cycle_id cannot equal awareness_cycle_id",
                ));
            }
        }
        if let Some(scope_id) = &self.collab_scope_id {
            if scope_id.trim().is_empty() {
                return Err(ModelError::Invariant("collab_scope_id cannot be empty"));
            }
        }
        if let Some(barrier_id) = &self.barrier_id {
            if barrier_id.trim().is_empty() {
                return Err(ModelError::Invariant("barrier_id cannot be empty"));
            }
        }
        if let Some(env_mode) = &self.env_mode {
            if env_mode.trim().is_empty() {
                return Err(ModelError::Invariant("env_mode cannot be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionBudgetEstimate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walltime_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_cost: Option<f32>,
}

impl Default for DecisionBudgetEstimate {
    fn default() -> Self {
        Self {
            tokens: None,
            walltime_ms: None,
            external_cost: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionBlockReason {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionInvalidReason {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionRationale {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<DecisionBlockReason>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid: Vec<DecisionInvalidReason>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scores: HashMap<String, f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thresholds_hit: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tradeoff: Option<String>,
}

impl Default for DecisionRationale {
    fn default() -> Self {
        Self {
            blocked: Vec::new(),
            invalid: Vec::new(),
            scores: HashMap::new(),
            thresholds_hit: Vec::new(),
            tradeoff: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionExplain {
    pub routing_seed: u64,
    pub router_digest: String,
    pub router_config_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features_snapshot: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ic: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarifyQuestion {
    pub q_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClarifyLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_wait_ms: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClarifyPlan {
    #[serde(default)]
    pub questions: Vec<ClarifyQuestion>,
    #[serde(default)]
    pub limits: ClarifyLimits,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolPlanNode {
    pub id: String,
    pub tool_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_policy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPlanEdge {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolPlanBarrier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolPlan {
    #[serde(default)]
    pub nodes: Vec<ToolPlanNode>,
    #[serde(default)]
    pub edges: Vec<ToolPlanEdge>,
    #[serde(default)]
    pub barrier: ToolPlanBarrier,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CollabPlan {
    pub scope: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_mode: Option<String>,
    #[serde(default)]
    pub barrier: ToolPlanBarrier,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionPlan {
    SelfReason { plan: SelfPlan },
    Clarify { plan: ClarifyPlan },
    Tool { plan: ToolPlan },
    Collab { plan: CollabPlan },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessDegradationReason {
    BudgetTokens,
    BudgetWalltime,
    BudgetExternalCost,
    EmptyCatalog,
    PrivacyBlocked,
    InvalidPlan,
    ClarifyExhausted,
    GraphDegraded,
    EnvctxDegraded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionPath {
    pub anchor: AwarenessAnchor,
    pub awareness_cycle_id: AwarenessCycleId,
    pub inference_cycle_sequence: u32,
    pub fork: AwarenessFork,
    pub plan: DecisionPlan,
    #[serde(default)]
    pub budget_plan: DecisionBudgetEstimate,
    #[serde(default)]
    pub rationale: DecisionRationale,
    pub confidence: f32,
    pub explain: DecisionExplain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeltaPatch {
    pub patch_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub score_stats: HashMap<String, f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why_included: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointers: Option<Value>,
    pub patch_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncPointReport {
    pub anchor: AwarenessAnchor,
    pub awareness_cycle_id: AwarenessCycleId,
    pub inference_cycle_sequence: u32,
    pub kind: SyncPointKind,
    #[serde(default)]
    pub inbox_stats: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_digest: Option<String>,
    #[serde(default)]
    pub budget_snapshot: Value,
    pub report_digest: String,
}

/// Historical awareness cycle record. Retained for compatibility with existing contracts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessCycleRecord {
    pub tenant_id: TenantId,
    pub cycle_id: AwarenessCycleId,
    pub session_id: SessionId,
    pub subject: Subject,
    pub cycle_sequence: u64,
    pub cycle_duration_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance_trend: Option<String>,
    #[serde(default)]
    pub llm_calls: u32,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub resource_cost: Value,
    pub decision_path_taken: String,
    pub snapshot: Snapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_cycle_id: Option<InferenceCycleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_explain_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_snapshot_version: Option<u32>,
}

impl AwarenessCycleRecord {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.cycle_sequence == 0 {
            return Err(ModelError::Invariant("cycle_sequence must be >= 1"));
        }
        if self.decision_path_taken.trim().is_empty() {
            return Err(ModelError::Missing("decision_path_taken"));
        }
        if let Some(inference_cycle_id) = self.inference_cycle_id {
            if inference_cycle_id.into_inner() == 0 {
                return Err(ModelError::Invariant(
                    "inference_cycle_id must be >= 1 when provided",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_path_roundtrip() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(1),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: Some(SessionId::from_raw_unchecked(7)),
            sequence_number: Some(3),
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        };

        let decision = DecisionPath {
            anchor,
            awareness_cycle_id: AwarenessCycleId::from_raw_unchecked(11),
            inference_cycle_sequence: 2,
            fork: AwarenessFork::ToolPath,
            plan: DecisionPlan::Tool {
                plan: ToolPlan {
                    nodes: vec![ToolPlanNode {
                        id: "t1".into(),
                        tool_id: "web.search".into(),
                        version: Some("v1".into()),
                        input: serde_json::json!({"q": "demo"}),
                        timeout_ms: Some(2_000),
                        success_criteria: None,
                        evidence_policy: Some("pointer-only".into()),
                    }],
                    edges: vec![],
                    barrier: ToolPlanBarrier {
                        mode: Some("all".into()),
                        timeout_ms: Some(3_000),
                    },
                },
            },
            budget_plan: DecisionBudgetEstimate {
                tokens: Some(800),
                walltime_ms: Some(1_200),
                external_cost: None,
            },
            rationale: DecisionRationale {
                blocked: vec![DecisionBlockReason {
                    id: "tool:internal.db".into(),
                    reason: Some("privacy_scope".into()),
                }],
                invalid: vec![],
                scores: HashMap::from([("self".into(), 0.1), ("tool".into(), 0.85)]),
                thresholds_hit: vec!["budget_tokens".into()],
                tradeoff: Some("tool vs clarify".into()),
            },
            confidence: 0.76,
            explain: DecisionExplain {
                routing_seed: 42,
                router_digest: "sha256:abc".into(),
                router_config_digest: "sha256:def".into(),
                features_snapshot: Some(serde_json::json!({"top": ["rel", "risk"]})),
            },
            degradation_reason: Some(AwarenessDegradationReason::BudgetTokens),
        };

        let json = serde_json::to_string(&decision).expect("serialize");
        let back: DecisionPath = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decision, back);
    }

    #[test]
    fn delta_patch_defaults() {
        let patch = DeltaPatch {
            patch_id: "patch-1".into(),
            added: vec!["ctx-1".into()],
            updated: Vec::new(),
            removed: Vec::new(),
            score_stats: HashMap::new(),
            why_included: None,
            pointers: None,
            patch_digest: "sha256:xyz".into(),
        };

        let json = serde_json::to_string(&patch).expect("serialize");
        let back: DeltaPatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(patch, back);
    }

    #[test]
    fn awareness_event_validate_roundtrip() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(42),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg-hash".into(),
            config_snapshot_version: 3,
            session_id: Some(SessionId::from_raw_unchecked(11)),
            sequence_number: Some(5),
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        };

        let event = AwarenessEvent {
            anchor: anchor.clone(),
            event_id: EventId::from_raw_unchecked(999),
            event_type: AwarenessEventType::DecisionRouted,
            occurred_at_ms: 123_456,
            awareness_cycle_id: AwarenessCycleId::from_raw_unchecked(77),
            parent_cycle_id: Some(AwarenessCycleId::from_raw_unchecked(21)),
            collab_scope_id: Some("scope-1".into()),
            barrier_id: Some("barrier-A".into()),
            env_mode: Some("turbo".into()),
            inference_cycle_sequence: 1,
            degradation_reason: Some(AwarenessDegradationReason::BudgetTokens),
            payload: serde_json::json!({"decision": "self"}),
        };

        event.validate().expect("valid awareness event");
        let serialized = serde_json::to_string(&event).expect("serialize");
        let restored: AwarenessEvent = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(restored.event_type, AwarenessEventType::DecisionRouted);
        assert_eq!(restored.payload["decision"], "self");
        assert_eq!(restored.anchor.schema_v, 1);
    }

    #[test]
    fn anchor_validation_rejects_missing_provenance() {
        let anchor = AwarenessAnchor {
            tenant_id: TenantId::from_raw_unchecked(1),
            envelope_id: uuid::Uuid::nil(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: None,
            sequence_number: None,
            access_class: AccessClass::Restricted,
            provenance: None,
            schema_v: 1,
        };

        let err = anchor
            .validate()
            .expect_err("restricted requires provenance");
        assert!(matches!(err, ModelError::Invariant(_)));
    }
}
