use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use soulseed_agi_core_models::{AIId, AccessClass, HumanId, Provenance, SessionId, TenantId};

pub type EnvelopeId = uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Anchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[cfg(feature = "strict-privacy")]
    #[serde(default = "default_restricted")]
    pub access_class: AccessClass,
    #[cfg(not(feature = "strict-privacy"))]
    pub access_class: AccessClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
}

#[cfg(feature = "strict-privacy")]
const fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentContext {
    pub anchor: Anchor,
    pub internal_scene: InternalScene,
    pub external_systems: ExternalSystems,
    pub interaction_object: InteractionObject,
    pub tool_permission: ToolPermission,
    pub life_journey: LifeJourney,
    pub source_versions: SourceVersions,
    pub context_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<DegradationReason>,
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
    pub soul_state: SoulState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SoulState {
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
    pub observe_watermark: Option<OffsetDateTime>,
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
