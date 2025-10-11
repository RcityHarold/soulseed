use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub use soulseed_agi_core_models::components::{
    AIView, ConversationSummary, DegradationReason, EnvironmentAnchor, EnvironmentContext,
    EnvironmentSoulState, EnvironmentVector, ExternalSystems, FreshnessState, GroupView, HumanView,
    InteractionObject, InternalScene, LatencyBand, LatencyWindow, LifeJourney, LifeMilestone,
    NavigationPath, NavigationWaypoint, NetworkQuality, RiskLevel, ServiceFreshness,
    SourceVersions, TaskSummary, ToolDefLite, ToolPermission, VersionPointer,
};
pub use soulseed_agi_core_models::EvidencePointer;

pub type EnvelopeId = soulseed_agi_core_models::components::EnvironmentEnvelopeId;
pub type Anchor = EnvironmentAnchor;
pub use soulseed_agi_core_models::components::EnvironmentSoulState as SoulState;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentSnapshotEvent {
    pub anchor: Anchor,
    pub schema_v: u16,
    pub context_digest: String,
    pub snapshot_digest: String,
    pub lite_mode: bool,
    pub environment: String,
    pub region: String,
    pub scene: Option<String>,
    pub risk_flag: String,
    pub policy_digest: Option<String>,
    pub source_versions: SourceVersions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidencePointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<DegradationReason>,
    pub generated_at: OffsetDateTime,
}
