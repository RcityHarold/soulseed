use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use soulseed_agi_envctx::dto::SourceVersions;

pub use soulseed_agi_core_models::{
    AIId, AccessClass, ConversationScenario, EnvelopeId, EventId, GroupId, HumanId, MessageId,
    Provenance, SessionId, TenantId,
};

pub type EnvContext = soulseed_agi_envctx::EnvironmentContext;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Anchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    pub session_id: Option<SessionId>,
    pub sequence_number: Option<u64>,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ConversationScenario>,
}

fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Partition {
    P0Policy,
    P1TaskFacts,
    P2Evidence,
    P3WorkingDelta,
    P4Dialogue,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Level {
    L0,
    L1,
    L2,
    L3,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EvidencePointer {
    pub uri: String,
    pub blob_ref: Option<String>,
    pub span: Option<(u32, u32)>,
    pub checksum: String,
    pub access_policy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FeatureVec {
    pub rel: f32,
    pub cau: f32,
    pub rec: f32,
    pub auth: f32,
    pub stab: f32,
    pub dup: f32,
    pub len: f32,
    pub risk: f32,
}

impl FeatureVec {
    pub fn clamp(mut self) -> Self {
        self.rel = self.rel.clamp(0.0, 1.0);
        self.cau = self.cau.clamp(0.0, 1.0);
        self.rec = self.rec.clamp(0.0, 1.0);
        self.auth = self.auth.clamp(0.0, 1.0);
        self.stab = self.stab.clamp(0.0, 1.0);
        self.dup = self.dup.clamp(0.0, 1.0);
        self.len = self.len.clamp(0.0, 1.0);
        self.risk = self.risk.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextItem {
    pub anchor: Anchor,
    pub id: String,
    pub partition_hint: Option<Partition>,
    pub source_event_id: EventId,
    pub source_message_id: Option<MessageId>,
    pub content: serde_json::Value,
    pub tokens: u32,
    pub features: FeatureVec,
    pub policy_tags: serde_json::Value,
    pub evidence: Option<EvidencePointer>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextScore {
    pub relevance: f32,
    pub importance: f32,
    pub compressibility: f32,
    pub final_score: f32,
    pub partition_affinity: Partition,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScoredItem {
    pub item: ContextItem,
    pub score: ContextScore,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QualityStats {
    pub qag: f32,
    pub coverage: f32,
    pub nli_contrad: f32,
    pub pointer_ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lineage {
    pub version: u32,
    pub supersedes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SummaryUnit {
    pub anchor: Anchor,
    pub su_id: String,
    pub from_ids: Vec<String>,
    pub level: Level,
    pub summary: serde_json::Value,
    pub tokens_saved: u32,
    pub quality: QualityStats,
    pub lineage: Lineage,
    pub evidence: Option<EvidencePointer>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PlanAction {
    Keep,
    Evict,
    L1,
    L2,
    L3,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanItem {
    pub ci_id: String,
    pub partition: Partition,
    pub base_tokens: u32,
    pub action: PlanAction,
    pub tokens_after: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PartitionUsage {
    pub partition: Partition,
    pub tokens_before: u32,
    pub tokens_after: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetSummary {
    pub target_tokens: u32,
    pub projected_tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompressionPlan {
    pub plan_id: String,
    pub anchor: Anchor,
    pub items: Vec<PlanItem>,
    pub partitions: Vec<PartitionUsage>,
    pub budget: BudgetSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompressionReport {
    pub anchor: Anchor,
    pub plan_id: String,
    pub tokens_saved: i64,
    pub rolled_back: Vec<(String, String)>,
    pub quality_stats: Vec<(String, QualityStats)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleItem {
    pub ci_id: String,
    pub summary_level: Option<Level>,
    pub tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BundleSegment {
    pub partition: Partition,
    pub items: Vec<BundleItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExplainBundle {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indices_used: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextBundle {
    pub anchor: Anchor,
    pub segments: Vec<BundleSegment>,
    pub explain: ExplainBundle,
    pub budget: BudgetSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentSnapshotEvent {
    pub anchor: Anchor,
    pub context_digest: String,
    pub snapshot_digest: String,
    pub environment: String,
    pub region: String,
    pub scene: Option<String>,
    pub risk_flag: String,
    pub manifest_digest: String,
    pub source_versions: SourceVersions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_pointers: Vec<EvidencePointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    pub generated_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RedactionEntry {
    pub ci_id: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_pointer: Option<EvidencePointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_action: Option<PlanAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RedactionReport {
    pub anchor: Anchor,
    pub manifest_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<RedactionEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct GraphExplain {
    pub reasons: Vec<String>,
    pub indices_used: Vec<String>,
    pub query_hash: Option<String>,
    pub degradation_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RunInput {
    pub anchor: Anchor,
    pub env_context: EnvContext,
    pub config: crate::config::ContextConfig,
    pub items: Vec<ContextItem>,
    pub graph_explain: Option<GraphExplain>,
}

#[derive(Clone, Debug)]
pub struct RunOutput {
    pub plan: CompressionPlan,
    pub report: CompressionReport,
    pub bundle: ContextBundle,
    pub manifest: crate::assembly::ContextManifest,
    pub env_snapshot: EnvironmentSnapshotEvent,
    pub redaction: RedactionReport,
}
