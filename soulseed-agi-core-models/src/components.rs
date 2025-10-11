use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{AIId, AccessClass, HumanId, Provenance, SessionId, TenantId};

#[cfg(feature = "strict-privacy")]
const fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub tool_id: String,
    pub version: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub input_schema: Value,
    pub output_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintainer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<ToolLifecycle>,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolLifecycle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introduced_at: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset_at: Option<OffsetDateTime>,
}

pub type EnvironmentEnvelopeId = Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentAnchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvironmentEnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EnvironmentEnvelopeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EnvironmentEnvelopeId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentContext {
    pub anchor: EnvironmentAnchor,
    pub internal_scene: InternalScene,
    pub external_systems: ExternalSystems,
    pub interaction_object: InteractionObject,
    pub tool_permission: ToolPermission,
    pub life_journey: LifeJourney,
    pub source_versions: SourceVersions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_vectors: Vec<EnvironmentVector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation_path: Option<NavigationPath>,
    pub context_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<DegradationReason>,
    #[serde(default)]
    pub lite_mode: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentVector {
    pub vector_id: String,
    pub namespace: String,
    pub model: String,
    pub dim: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_locator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavigationPath {
    pub path_id: String,
    pub current: NavigationWaypoint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breadcrumbs: Vec<NavigationWaypoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predictions: Vec<NavigationWaypoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavigationWaypoint {
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InternalScene {
    pub conversation: ConversationSummary,
    pub task: TaskSummary,
    pub latency_window: LatencyWindow,
    pub risk_flag: RiskLevel,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationSummary {
    pub rounds: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    pub scene: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSummary {
    pub goal: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatencyWindow {
    pub p50_ms: u32,
    pub p95_ms: u32,
    pub p50_band: LatencyBand,
    pub p95_band: LatencyBand,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum LatencyBand {
    Sub50,
    Ms50To100,
    Ms100To250,
    Ms250To500,
    Above500,
}

impl LatencyBand {
    pub const fn from_ms(ms: u32) -> Self {
        match ms {
            0..=49 => LatencyBand::Sub50,
            50..=99 => LatencyBand::Ms50To100,
            100..=249 => LatencyBand::Ms100To250,
            250..=499 => LatencyBand::Ms250To500,
            _ => LatencyBand::Above500,
        }
    }
}

impl LatencyWindow {
    pub fn new(p50_ms: u32, p95_ms: u32) -> Self {
        Self {
            p50_ms,
            p95_ms,
            p50_band: LatencyBand::from_ms(p50_ms),
            p95_band: LatencyBand::from_ms(p95_ms),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalSystems {
    pub environment: String,
    pub region: String,
    pub timezone: String,
    pub locale: String,
    pub network_quality: NetworkQuality,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_freshness: Vec<ServiceFreshness>,
    pub policy_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkQuality {
    Poor,
    Fair,
    Good,
    Excellent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFreshness {
    pub name: String,
    pub freshness: FreshnessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FreshnessState {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractionObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human: Option<HumanView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_companions: Vec<AIView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanView {
    pub id: HumanId,
    pub role: String,
    pub scope: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AIView {
    pub id: AIId,
    pub soul_state: EnvironmentSoulState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnvironmentSoulState {
    Active,
    Reflecting,
    Asleep,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupView {
    pub members: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPermission {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_tools: Vec<ToolDefLite>,
    pub policy_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefLite {
    pub tool_id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifeJourney {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub milestones: Vec<LifeMilestone>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_arc: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifeMilestone {
    pub name: String,
    pub occurred_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub significance: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceVersions {
    pub graph_snapshot: VersionPointer,
    pub policy_snapshot: VersionPointer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_catalog_snapshot: Option<VersionPointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authz_snapshot: Option<VersionPointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_snapshot: Option<VersionPointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observe_watermark: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitoring_snapshot: Option<VersionPointer>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionPointer {
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DegradationReason {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl DegradationReason {
    pub fn new(code: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code: code.into(),
            detail,
        }
    }
}
